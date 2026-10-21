use super::{
    audit_delta_coverage, CandidateAction, CandidateDelta, PressureLedger, StrategicAuditReport,
    StrategicSnapshot, VerdictHint,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionVerdict {
    MustTake,
    StrongTake,
    ContextTake,
    Speculative,
    SkipPreferred,
    Reject,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CompiledDecision {
    pub action: CandidateAction,
    pub verdict: AcquisitionVerdict,
    pub score: f32,
    #[serde(default)]
    pub matched_pressure_kinds: Vec<super::PressureKind>,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StrategicDecisionTrace {
    pub snapshot: StrategicSnapshot,
    pub ledger: PressureLedger,
    pub candidate_deltas: Vec<CandidateDelta>,
    pub compiled: Vec<CompiledDecision>,
    pub would_choose: Option<CandidateAction>,
    pub audit: StrategicAuditReport,
}

impl AcquisitionVerdict {
    pub fn allows_behavior_acquisition(self) -> bool {
        matches!(self, Self::MustTake | Self::StrongTake | Self::ContextTake)
    }

    pub fn retention_order(self) -> usize {
        match self {
            Self::MustTake => 0,
            Self::StrongTake => 1,
            Self::ContextTake => 2,
            Self::Speculative => 3,
            Self::SkipPreferred => 4,
            Self::Reject => 5,
        }
    }
}

impl StrategicDecisionTrace {
    pub fn compiled_for_action(&self, action: &CandidateAction) -> Option<&CompiledDecision> {
        let candidate_id = action.candidate_id();
        self.compiled
            .iter()
            .find(|decision| decision.action.candidate_id() == candidate_id)
    }
}

pub fn compile_decision(
    snapshot: StrategicSnapshot,
    ledger: PressureLedger,
    candidate_count: usize,
    candidate_deltas: Vec<CandidateDelta>,
) -> StrategicDecisionTrace {
    let mut compiled = candidate_deltas
        .iter()
        .map(|delta| compile_candidate(delta, &ledger))
        .collect::<Vec<_>>();
    compiled.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let would_choose = compiled
        .iter()
        .find(|decision| decision.verdict != AcquisitionVerdict::Reject)
        .map(|decision| decision.action.clone());
    let audit = audit_delta_coverage(candidate_count, candidate_deltas.len());

    StrategicDecisionTrace {
        snapshot,
        ledger,
        candidate_deltas,
        compiled,
        would_choose,
        audit,
    }
}

fn compile_candidate(delta: &CandidateDelta, ledger: &PressureLedger) -> CompiledDecision {
    let score = delta.positive_amount() + ledger_alignment_bonus(delta, ledger)
        - delta.negative_amount()
        - ledger_pressure_penalty(delta, ledger)
        + verdict_bias(delta.verdict_hint);
    let verdict = if !delta.contraindications.is_empty() {
        AcquisitionVerdict::Reject
    } else if score >= 1.20 {
        AcquisitionVerdict::MustTake
    } else if score >= 0.75 {
        AcquisitionVerdict::StrongTake
    } else if score >= 0.30 {
        AcquisitionVerdict::ContextTake
    } else if score >= -0.10 {
        AcquisitionVerdict::Speculative
    } else if score >= -0.45 {
        AcquisitionVerdict::SkipPreferred
    } else {
        AcquisitionVerdict::Reject
    };
    let mut reasons = delta
        .positive
        .iter()
        .map(|delta| format!("+{}:{:?}", delta.reason, delta.kind))
        .collect::<Vec<_>>();
    reasons.extend(
        delta
            .negative
            .iter()
            .map(|delta| format!("-{}:{:?}", delta.reason, delta.kind)),
    );
    reasons.extend(ledger_alignment_reasons(delta, ledger));
    reasons.extend(ledger_pressure_reasons(delta, ledger));

    CompiledDecision {
        action: delta.action.clone(),
        verdict,
        score,
        matched_pressure_kinds: matched_pressure_kinds(delta, ledger),
        reasons,
    }
}

fn matched_pressure_kinds(
    delta: &CandidateDelta,
    ledger: &PressureLedger,
) -> Vec<super::PressureKind> {
    let mut kinds = delta
        .positive
        .iter()
        .filter_map(|candidate| {
            (ledger_match_strength(ledger, candidate.kind) > 0.0).then_some(candidate.kind)
        })
        .collect::<Vec<_>>();
    kinds.sort();
    kinds.dedup();
    kinds
}

fn ledger_alignment_bonus(delta: &CandidateDelta, ledger: &PressureLedger) -> f32 {
    delta
        .positive
        .iter()
        .map(|delta| delta.amount * ledger_match_strength(ledger, delta.kind))
        .sum()
}

fn ledger_alignment_reasons(delta: &CandidateDelta, ledger: &PressureLedger) -> Vec<String> {
    delta
        .positive
        .iter()
        .filter_map(|delta| {
            let strength = ledger_match_strength(ledger, delta.kind);
            (strength > 0.0).then(|| format!("+ledger_match:{:?}:{strength:.2}", delta.kind))
        })
        .collect()
}

fn ledger_pressure_penalty(delta: &CandidateDelta, ledger: &PressureLedger) -> f32 {
    delta
        .negative
        .iter()
        .map(|delta| delta.amount * ledger_match_strength(ledger, delta.kind))
        .sum()
}

fn ledger_pressure_reasons(delta: &CandidateDelta, ledger: &PressureLedger) -> Vec<String> {
    delta
        .negative
        .iter()
        .filter_map(|delta| {
            let strength = ledger_match_strength(ledger, delta.kind);
            (strength > 0.0).then(|| format!("-ledger_pressure:{:?}:{strength:.2}", delta.kind))
        })
        .collect()
}

fn ledger_match_strength(ledger: &PressureLedger, kind: super::PressureKind) -> f32 {
    ledger
        .items
        .iter()
        .filter(|item| item.kind == kind)
        .map(|item| item.severity * item.confidence)
        .fold(0.0_f32, f32::max)
}

fn verdict_bias(hint: VerdictHint) -> f32 {
    match hint {
        VerdictHint::MustTake => 0.80,
        VerdictHint::StrongTake => 0.45,
        VerdictHint::ContextTake => 0.20,
        VerdictHint::Speculative => 0.0,
        VerdictHint::SkipPreferred => -0.25,
        VerdictHint::Reject => -0.80,
    }
}
