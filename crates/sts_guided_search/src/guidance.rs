use std::time::{Duration, Instant};

use crate::{NodeRef, SearchError};

/// Fixed allowance for external guidance attempts; default disables them.
#[derive(Clone, Copy, Debug, Default)]
pub struct GuidanceBudget {
    pub max_calls: usize,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
}

/// Caller-supplied upper bounds, to be enforced by a future provider adapter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TokenReservation {
    pub input: u64,
    pub output: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuidanceTicket {
    session: u64,
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuidanceOutcome {
    InFlight,
    Applied,
    Rejected,
    Failed,
    Expired,
    UsageExceededReservation,
}

#[derive(Clone, Debug)]
pub struct GuidanceReceipt {
    pub sources: Vec<NodeRef>,
    pub reservation: TokenReservation,
    /// None stays unknown; reservation charges are not observed usage.
    pub usage: Option<TokenUsage>,
    pub outcome: GuidanceOutcome,
    pub round_trip: Option<Duration>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GuidanceCosts {
    pub calls: usize,
    pub in_flight: usize,
    pub applied: usize,
    pub rejected: usize,
    pub failed: usize,
    pub expired: usize,
    pub usage_overruns: usize,
    pub known_input_tokens: u128,
    pub known_output_tokens: u128,
    /// Finished calls whose usage is unknown; in-flight calls are separate.
    pub unknown_usage_calls: usize,
    /// Known usage + reservations for unknown and in-flight attempts.
    pub charged_input_tokens: u128,
    pub charged_output_tokens: u128,
    /// Sum of request durations; overlapping calls are not wall latency.
    pub round_trip_sum: Duration,
}

struct Attempt {
    receipt: GuidanceReceipt,
    started: Instant,
}

pub(crate) struct GuidanceLedger {
    budget: GuidanceBudget,
    attempts: Vec<Attempt>,
    costs: GuidanceCosts,
}

impl GuidanceLedger {
    pub fn new(budget: GuidanceBudget) -> Self {
        Self {
            budget,
            attempts: Vec::new(),
            costs: GuidanceCosts::default(),
        }
    }

    pub fn costs(&self) -> GuidanceCosts {
        self.costs
    }

    pub fn in_flight(&self, source: NodeRef) -> bool {
        self.attempts.iter().any(|a| {
            a.receipt.outcome == GuidanceOutcome::InFlight && a.receipt.sources.contains(&source)
        })
    }

    pub fn begin(
        &mut self,
        session: u64,
        sources: Vec<NodeRef>,
        reservation: TokenReservation,
        now: Instant,
    ) -> Result<GuidanceTicket, SearchError> {
        if self.costs.usage_overruns != 0
            || self.costs.calls >= self.budget.max_calls
            || self.costs.charged_input_tokens + u128::from(reservation.input)
                > u128::from(self.budget.max_input_tokens)
            || self.costs.charged_output_tokens + u128::from(reservation.output)
                > u128::from(self.budget.max_output_tokens)
        {
            return Err(SearchError::GuidanceBudgetExceeded);
        }
        let ticket = GuidanceTicket {
            session,
            index: self.attempts.len(),
        };
        self.attempts.push(Attempt {
            started: now,
            receipt: GuidanceReceipt {
                sources,
                reservation,
                usage: None,
                outcome: GuidanceOutcome::InFlight,
                round_trip: None,
            },
        });
        self.costs.calls += 1;
        self.costs.in_flight += 1;
        self.costs.charged_input_tokens += u128::from(reservation.input);
        self.costs.charged_output_tokens += u128::from(reservation.output);
        Ok(ticket)
    }

    pub fn receipt(
        &self,
        session: u64,
        ticket: GuidanceTicket,
    ) -> Result<&GuidanceReceipt, SearchError> {
        if ticket.session != session {
            return Err(SearchError::InvalidGuidanceTicket);
        }
        self.attempts
            .get(ticket.index)
            .map(|a| &a.receipt)
            .ok_or(SearchError::InvalidGuidanceTicket)
    }

    pub fn open_sources(
        &self,
        session: u64,
        ticket: GuidanceTicket,
    ) -> Result<Vec<NodeRef>, SearchError> {
        let receipt = self.receipt(session, ticket)?;
        if receipt.outcome != GuidanceOutcome::InFlight {
            return Err(SearchError::GuidanceAlreadyFinished);
        }
        Ok(receipt.sources.clone())
    }

    /// Closing an attempt always accounts its cost, including rejected replies.
    /// It never writes guidance into the search tree.
    pub fn finish(
        &mut self,
        session: u64,
        ticket: GuidanceTicket,
        usage: Option<TokenUsage>,
        requested: GuidanceOutcome,
        now: Instant,
    ) -> Result<GuidanceOutcome, SearchError> {
        self.open_sources(session, ticket)?;
        let attempt = &mut self.attempts[ticket.index];
        let reserved = attempt.receipt.reservation;
        let outcome =
            if usage.is_some_and(|u| u.input > reserved.input || u.output > reserved.output) {
                GuidanceOutcome::UsageExceededReservation
            } else {
                requested
            };
        let elapsed = now.saturating_duration_since(attempt.started);
        attempt.receipt.usage = usage;
        attempt.receipt.outcome = outcome;
        attempt.receipt.round_trip = Some(elapsed);
        self.costs.in_flight -= 1;
        self.costs.round_trip_sum += elapsed;
        match outcome {
            GuidanceOutcome::Applied => self.costs.applied += 1,
            GuidanceOutcome::Rejected => self.costs.rejected += 1,
            GuidanceOutcome::Failed => self.costs.failed += 1,
            GuidanceOutcome::Expired => self.costs.expired += 1,
            GuidanceOutcome::UsageExceededReservation => self.costs.usage_overruns += 1,
            GuidanceOutcome::InFlight => unreachable!("attempt cannot finish in flight"),
        }
        if let Some(usage) = usage {
            self.costs.known_input_tokens += u128::from(usage.input);
            self.costs.known_output_tokens += u128::from(usage.output);
            self.costs.charged_input_tokens = self.costs.charged_input_tokens
                - u128::from(reserved.input)
                + u128::from(usage.input);
            self.costs.charged_output_tokens = self.costs.charged_output_tokens
                - u128::from(reserved.output)
                + u128::from(usage.output);
        } else {
            self.costs.unknown_usage_calls += 1;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_durations_keep_overlapping_calls_separate_from_wall_latency() {
        let mut ledger = GuidanceLedger::new(GuidanceBudget {
            max_calls: 2,
            max_input_tokens: 20,
            max_output_tokens: 20,
        });
        let start = Instant::now();
        let first = ledger
            .begin(
                1,
                vec![],
                TokenReservation {
                    input: 10,
                    output: 10,
                },
                start,
            )
            .unwrap();
        let second = ledger
            .begin(
                1,
                vec![],
                TokenReservation {
                    input: 10,
                    output: 10,
                },
                start + Duration::from_secs(1),
            )
            .unwrap();
        ledger
            .finish(
                1,
                second,
                None,
                GuidanceOutcome::Failed,
                start + Duration::from_secs(4),
            )
            .unwrap();
        ledger
            .finish(
                1,
                first,
                Some(TokenUsage {
                    input: 5,
                    output: 2,
                }),
                GuidanceOutcome::Rejected,
                start + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            ledger.receipt(1, first).unwrap().round_trip,
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            ledger.receipt(1, second).unwrap().round_trip,
            Some(Duration::from_secs(3))
        );
        assert_eq!(ledger.costs().round_trip_sum, Duration::from_secs(8));
        assert_eq!(ledger.costs().charged_input_tokens, 15);
        assert_eq!(ledger.costs().charged_output_tokens, 12);
        assert_eq!(ledger.costs().unknown_usage_calls, 1);
        let before = ledger.costs();
        assert_eq!(
            ledger.finish(
                1,
                first,
                None,
                GuidanceOutcome::Failed,
                start + Duration::from_secs(6)
            ),
            Err(SearchError::GuidanceAlreadyFinished)
        );
        assert_eq!(ledger.costs(), before);
    }
}
