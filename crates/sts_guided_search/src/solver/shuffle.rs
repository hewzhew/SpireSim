//! Incremental post-win order proposals. Every engine transition and child
//! search/replay unit is charged to the enclosing solver's existing budget.
use super::*;
use crate::shuffle_order::{
    checked_mapping, discard_play, mapped_orders, rebind_follow_input, recorded_draw_cuts,
    DrawSummary, Mapping, Play,
};
use sts_core::runtime::combat::CardLineageId;
use sts_core::sim::combat::{
    apply_combat_input_to_stable_oracle_shuffles_v1, is_exact_combat_victory,
};

#[derive(Clone, Debug, Serialize)]
pub struct ShufflePlanOrigin {
    pub source_found_at_work: usize,
    /// Replayable from the solver root, even after the source leaves plans().
    pub source_inputs: Vec<ClientInput>,
    pub source_outcome: CombatWitnessMaterializedOutcomeV1,
    pub block_start: usize,
    pub shuffle_action: usize,
    pub order: Vec<usize>,
    /// The unchanged order is also tried as a continuation-search control.
    pub reordered: bool,
    pub supplied_suffix: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShuffleProgress {
    pub source_found_at_work: usize,
    pub phase: &'static str,
    pub traced_inputs: usize,
    pub remaining_orders: usize,
    pub continuation_work: usize,
    /// Saved exact roots, not active child solvers. Only one child runs at once.
    pub queued_continuations: usize,
    pub pending_sources: usize,
}

pub(super) enum ShuffleEvent {
    Progress,
    TimedOut,
    Done,
    Win {
        inputs: Vec<ClientInput>,
        expected: CombatPosition,
        origin: SolverPlanOrigin,
    },
}

struct Window {
    start: usize,
    block_end: usize,
    end: usize,
    plays: Vec<Play>,
    offset: usize,
    orders: VecDeque<Vec<usize>>,
}

#[derive(Clone, Copy)]
enum Phase {
    Plays,
    Following,
    Suffix,
    Search,
}

struct Candidate {
    window: usize,
    order: Vec<usize>,
    current: CombatPosition,
    prefix: Vec<ClientInput>,
    cursor: usize,
    replacement: Vec<CardLineageId>,
    phase: Phase,
    boundary: Option<CombatPosition>,
    suffix_inputs: Vec<ClientInput>,
    child: Option<Box<CombatSolver>>,
    submitted_child_work: usize,
    expenditures: CombatInputExpendituresV1,
}

pub(super) struct ShuffleJob {
    source: SolverPlan,
    config: SolverConfig,
    states: Vec<CombatPosition>,
    draws: Vec<DrawSummary>,
    maps: Vec<Option<Mapping>>,
    windows: Option<Vec<Window>>,
    next_window: usize,
    candidate: Option<Candidate>,
    continuations: VecDeque<Candidate>,
}

impl ShuffleJob {
    #[cfg(test)]
    pub(super) fn child_for_test(&self) -> Option<&CombatSolver> {
        self.candidate.as_ref().and_then(|c| c.child.as_deref())
    }
    pub(super) fn new(root: CombatPosition, source: SolverPlan, config: SolverConfig) -> Self {
        Self {
            source,
            config,
            states: vec![root],
            draws: vec![],
            maps: vec![],
            windows: None,
            next_window: 0,
            candidate: None,
            continuations: VecDeque::new(),
        }
    }

    pub(super) fn progress(&self, pending_sources: usize) -> ShuffleProgress {
        ShuffleProgress {
            source_found_at_work: self.source.found_at_work,
            phase: self.candidate.as_ref().map_or("trace", |c| match c.phase {
                Phase::Plays => "reorder",
                Phase::Following => "follow_to_shuffle",
                Phase::Suffix => "replay_supplied_suffix",
                Phase::Search => "continuation",
            }),
            traced_inputs: self.draws.len(),
            remaining_orders: self
                .windows
                .as_ref()
                .map_or(0, |ws| ws.iter().map(|w| w.orders.len()).sum()),
            continuation_work: self
                .candidate
                .as_ref()
                .and_then(|c| c.child.as_ref())
                .map_or(0, |s| s.stats.work()),
            queued_continuations: self.continuations.len(),
            pending_sources,
        }
    }

    fn prepare_windows(&self) -> Vec<Window> {
        let mut windows = Vec::new();
        let inputs = &self.source.inputs;
        for start in 0..inputs.len() {
            if start > 0
                && discard_play(
                    &self.states[start - 1],
                    &inputs[start - 1],
                    &self.states[start],
                )
                .is_some()
            {
                continue;
            }
            let mut block_end = start;
            let mut plays = vec![];
            while block_end < inputs.len() {
                let Some(play) = discard_play(
                    &self.states[block_end],
                    &inputs[block_end],
                    &self.states[block_end + 1],
                ) else {
                    break;
                };
                plays.push(play);
                block_end += 1;
            }
            if !(2..=6).contains(&plays.len()) {
                continue;
            }
            // Never jump over an unmodelled shuffle to reuse a later mapping.
            let Some(end) = (block_end..inputs.len()).find(|&i| {
                self.states[i].combat.rng.shuffle_rng != self.states[i + 1].combat.rng.shuffle_rng
            }) else {
                continue;
            };
            let Some(map) = &self.maps[end] else {
                continue;
            };
            let offset = self.states[start].combat.zones.discard_pile.len();
            let appended: Vec<_> = plays
                .iter()
                .flat_map(|p| p.discarded.iter().copied())
                .collect();
            if map.input.get(offset..offset + appended.len()) != Some(appended.as_slice()) {
                continue;
            }
            let cuts = recorded_draw_cuts(map, self.draws[end + 1..].iter().cloned());
            let widths: Vec<_> = plays.iter().map(|p| p.discarded.len()).collect();
            let orders = mapped_orders(&widths, offset, map, &cuts)
                .into_iter()
                .collect();
            windows.push(Window {
                start,
                block_end,
                end,
                plays,
                offset,
                orders,
            });
        }
        windows
    }

    fn win(
        &self,
        inputs: Vec<ClientInput>,
        expected: CombatPosition,
        supplied_suffix: bool,
    ) -> ShuffleEvent {
        let outcome = project_observed_combat_outcome_with_expenditures_v1(
            &self.states[0],
            CombatInputExpendituresV1::from_inputs(&inputs),
            &expected,
        );
        // The repair cannot add consumption, including a passive Fairy, or
        // substitute a different original bottle just to obtain a higher HP.
        if outcome.total_potion_expenditures > self.source.outcome.total_potion_expenditures
            || outcome.starting_potion_expenditure_slots
                & !self.source.outcome.starting_potion_expenditure_slots
                != 0
        {
            return ShuffleEvent::Progress;
        }
        let c = self.candidate.as_ref().unwrap();
        let w = &self.windows.as_ref().unwrap()[c.window];
        let mut origin = self.source.origin.clone();
        origin.shuffle = Some(ShufflePlanOrigin {
            source_found_at_work: self.source.found_at_work,
            source_inputs: self.source.inputs.clone(),
            source_outcome: self.source.outcome.clone(),
            block_start: w.start,
            shuffle_action: w.end,
            reordered: c.order.iter().copied().ne(0..c.order.len()),
            order: c.order.clone(),
            supplied_suffix,
        });
        ShuffleEvent::Win {
            inputs,
            expected,
            origin,
        }
    }

    /// Preserve the already validated shuffle endpoint. Expensive child search
    /// starts after the cheap proposal/suffix sweep. Equal endpoints share a
    /// continuation only with equal explicit expenditures from the same root.
    fn queue_continuation(&mut self, stats: &mut SolverStats) {
        let mut c = self.candidate.take().unwrap();
        c.current = c.boundary.take().unwrap();
        c.suffix_inputs.clear();
        if self
            .continuations
            .iter()
            .any(|other| other.current == c.current && other.expenditures == c.expenditures)
        {
            stats.shuffle_duplicate_continuations += 1;
        } else {
            self.continuations.push_back(c);
            stats.shuffle_continuations_queued += 1;
        }
    }

    pub(super) fn advance(
        &mut self,
        limits: CombatStepLimits,
        stats: &mut SolverStats,
    ) -> Result<ShuffleEvent, String> {
        loop {
            if limits.deadline.is_some_and(|d| Instant::now() >= d) {
                return Ok(ShuffleEvent::TimedOut);
            }
            if self.draws.len() < self.source.inputs.len() {
                let i = self.draws.len();
                let before = self.states.last().unwrap();
                let obs = apply_combat_input_to_stable_oracle_shuffles_v1(
                    before,
                    self.source.inputs[i].clone(),
                    limits,
                );
                stats.engine_steps += obs.observed.step.engine_steps;
                if obs.observed.step.timed_out {
                    return Ok(ShuffleEvent::TimedOut);
                }
                if obs.observed.step.truncated {
                    return Err("shuffle source replay exceeded engine-step limit".into());
                }
                stats.shuffle_work += 1;
                stats.shuffle_source_work += 1;
                self.draws.push(DrawSummary::from(&obs.observed));
                let next = obs.observed.step.position.clone();
                self.maps.push(
                    if before.combat.rng.shuffle_rng != next.combat.rng.shuffle_rng {
                        match checked_mapping(before, &next, obs) {
                            Ok(map) => Some(map),
                            Err(_) => {
                                stats.shuffle_unmapped_boundaries += 1;
                                None
                            }
                        }
                    } else {
                        None
                    },
                );
                self.states.push(next);
                return Ok(ShuffleEvent::Progress);
            }
            if self.windows.is_none() {
                if self.states.last() != Some(&self.source.successor) {
                    return Err("shuffle source no longer replays exactly".into());
                }
                self.windows = Some(self.prepare_windows());
            }
            let windows = self.windows.as_mut().unwrap();
            if self.candidate.is_none() {
                let Some(w) = windows.get_mut(self.next_window) else {
                    if let Some(candidate) = self.continuations.pop_front() {
                        self.candidate = Some(candidate);
                        continue;
                    }
                    return Ok(ShuffleEvent::Done);
                };
                let Some(order) = w.orders.pop_front() else {
                    self.next_window += 1;
                    continue;
                };
                stats.shuffle_candidates += 1;
                self.candidate = Some(Candidate {
                    window: self.next_window,
                    order,
                    current: self.states[w.start].clone(),
                    prefix: self.source.inputs[..w.start].to_vec(),
                    cursor: 0,
                    replacement: vec![],
                    phase: Phase::Plays,
                    boundary: None,
                    suffix_inputs: vec![],
                    child: None,
                    submitted_child_work: 0,
                    expenditures: CombatInputExpendituresV1::from_inputs(
                        &self.source.inputs[..w.start],
                    ),
                });
            }
            if self
                .candidate
                .as_ref()
                .is_some_and(|c| matches!(c.phase, Phase::Search) && c.boundary.is_some())
            {
                self.queue_continuation(stats);
                continue;
            }
            let c = self.candidate.as_mut().unwrap();
            let w = &self.windows.as_ref().unwrap()[c.window];
            match c.phase {
                Phase::Plays | Phase::Following => {
                    let playing = matches!(c.phase, Phase::Plays);
                    let input = if playing {
                        let play = &w.plays[c.order[c.cursor]];
                        let matches: Vec<_> = c
                            .current
                            .combat
                            .zones
                            .hand
                            .iter()
                            .enumerate()
                            .filter(|(_, card)| card.lineage == play.lineage)
                            .map(|(i, _)| i)
                            .collect();
                        if matches.len() != 1 {
                            self.candidate = None;
                            stats.shuffle_rejected += 1;
                            continue;
                        }
                        ClientInput::PlayCard {
                            card_index: matches[0],
                            target: play.target,
                        }
                    } else {
                        if c.current.combat.rng.shuffle_rng
                            != self.states[c.cursor].combat.rng.shuffle_rng
                        {
                            self.candidate = None;
                            stats.shuffle_rejected += 1;
                            continue;
                        }
                        match rebind_follow_input(
                            &c.current,
                            &self.states[c.cursor],
                            &self.source.inputs[c.cursor],
                        ) {
                            Ok(input) => input,
                            Err(_) => {
                                self.candidate = None;
                                stats.shuffle_rejected += 1;
                                continue;
                            }
                        }
                    };
                    if !EngineCombatStepper.is_legal_action(&c.current, &input) {
                        self.candidate = None;
                        stats.shuffle_rejected += 1;
                        continue;
                    }
                    let obs = apply_combat_input_to_stable_oracle_shuffles_v1(
                        &c.current,
                        input.clone(),
                        limits,
                    );
                    stats.engine_steps += obs.observed.step.engine_steps;
                    if obs.observed.step.timed_out {
                        return Ok(ShuffleEvent::TimedOut);
                    }
                    if obs.observed.step.truncated {
                        return Err("shuffle candidate exceeded engine-step limit".into());
                    }
                    stats.shuffle_work += 1;
                    stats.shuffle_prefix_work += 1;
                    let next = obs.observed.step.position.clone();
                    if playing {
                        let expected = &w.plays[c.order[c.cursor]];
                        if !discard_play(&c.current, &input, &next)
                            .is_some_and(|p| p.discarded.len() == expected.discarded.len())
                        {
                            self.candidate = None;
                            stats.shuffle_rejected += 1;
                            return Ok(ShuffleEvent::Progress);
                        }
                        c.cursor += 1;
                        if c.cursor == w.plays.len() {
                            c.replacement = next
                                .combat
                                .zones
                                .discard_pile
                                .iter()
                                .skip(w.offset)
                                .map(|card| card.lineage)
                                .collect();
                            c.phase = Phase::Following;
                            c.cursor = w.block_end;
                        }
                    } else if c.cursor == w.end {
                        let map = self.maps[w.end].as_ref().unwrap();
                        let mut predicted = map.input.clone();
                        predicted[w.offset..w.offset + c.replacement.len()]
                            .copy_from_slice(&c.replacement);
                        if !checked_mapping(&c.current, &next, obs).is_ok_and(|actual| {
                            actual.input == predicted
                                && actual.draw_to_input == map.draw_to_input
                                && actual.drawn == map.drawn
                        }) {
                            self.candidate = None;
                            stats.shuffle_rejected += 1;
                            return Ok(ShuffleEvent::Progress);
                        }
                        c.boundary = Some(next.clone());
                        c.phase = Phase::Suffix;
                        c.cursor = w.end + 1;
                    } else {
                        c.cursor += 1;
                    }
                    c.current = next;
                    c.expenditures.record(&input);
                    c.prefix.push(input);
                    return Ok(ShuffleEvent::Progress);
                }
                Phase::Suffix => {
                    if is_exact_combat_victory(&c.current) {
                        c.phase = Phase::Search;
                        let mut inputs = c.prefix.clone();
                        inputs.extend(c.suffix_inputs.clone());
                        let expected = c.current.clone();
                        return Ok(self.win(inputs, expected, true));
                    }
                    let Some(input) = self.source.inputs.get(c.cursor) else {
                        c.phase = Phase::Search;
                        continue;
                    };
                    if !matches!(
                        input,
                        ClientInput::PlayCard { .. }
                            | ClientInput::EndTurn
                            | ClientInput::SubmitSelection(_)
                    ) || !EngineCombatStepper.is_legal_action(&c.current, input)
                    {
                        c.phase = Phase::Search;
                        continue;
                    }
                    let step =
                        EngineCombatStepper.apply_to_stable(&c.current, input.clone(), limits);
                    stats.engine_steps += step.engine_steps;
                    if step.timed_out {
                        return Ok(ShuffleEvent::TimedOut);
                    }
                    if step.truncated {
                        return Err("shuffle suffix exceeded engine-step limit".into());
                    }
                    stats.shuffle_work += 1;
                    stats.shuffle_suffix_work += 1;
                    c.current = step.position;
                    c.suffix_inputs.push(input.clone());
                    c.cursor += 1;
                    return Ok(ShuffleEvent::Progress);
                }
                Phase::Search => {
                    if c.child.is_none() {
                        let boundary = &c.current;
                        if is_exact_combat_victory(boundary) {
                            self.candidate = None;
                            continue;
                        }
                        if sts_core::sim::combat::combat_terminal(
                            &boundary.engine,
                            &boundary.combat,
                        ) != sts_core::sim::combat::CombatTerminal::Unresolved
                        {
                            self.candidate = None;
                            stats.shuffle_rejected += 1;
                            continue;
                        }
                        c.child = Some(Box::new(CombatSolver::new(
                            boundary.clone(),
                            SolverConfig {
                                max_potion_uses: Some(0),
                                shuffle_continuation_work: 0,
                                order_family_work: 0,
                                ..self.config.clone()
                            },
                        )?));
                    }
                    let child = c.child.as_mut().unwrap();
                    // Publish each newly verified continuation promptly; do not
                    // wait for this candidate's entire allowance to expire.
                    if let Some(plan) = child
                        .plans()
                        .iter()
                        .filter(|p| p.found_at_work > c.submitted_child_work)
                        .min_by_key(|p| p.found_at_work)
                        .cloned()
                    {
                        c.submitted_child_work = plan.found_at_work;
                        let mut inputs = c.prefix.clone();
                        inputs.extend(plan.inputs);
                        return Ok(self.win(inputs, plan.successor, false));
                    }
                    if child.is_complete()
                        || child.stats.work() >= self.config.shuffle_continuation_work
                    {
                        self.candidate = None;
                        continue;
                    }
                    let prior_work = child.stats.work();
                    let prior_steps = child.stats.engine_steps;
                    let remaining = limits.deadline.map_or(Duration::from_secs(3600), |d| {
                        d.saturating_duration_since(Instant::now())
                    });
                    let result = child.advance(1, remaining);
                    stats.shuffle_work += child.stats.work() - prior_work;
                    stats.shuffle_continuation_work += child.stats.work() - prior_work;
                    stats.engine_steps += child.stats.engine_steps - prior_steps;
                    if result? == SolverStop::TimeBudget {
                        return Ok(ShuffleEvent::TimedOut);
                    }
                    return Ok(ShuffleEvent::Progress);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::{cards::CardId, monsters::EnemyId};
    use sts_core::runtime::combat::CombatCard;
    use sts_core::state::core::EngineState;

    #[test]
    fn queued_search_reuses_only_identical_boundary_and_expenditures() {
        let mut combat = sts_core::test_support::blank_test_combat();
        let mut enemy = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        enemy.current_hp = 6;
        combat.entities.monsters = vec![enemy];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
        combat.zones.discard_pile = vec![
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Defend, 3),
        ]
        .into();
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let mut solver = CombatSolver::new(root.clone(), SolverConfig::default()).unwrap();
        solver.advance(100, Duration::from_secs(30)).unwrap();
        let source = solver.plans()[0].clone();
        let mut job = ShuffleJob::new(root.clone(), source.clone(), SolverConfig::default());
        let candidate = |boundary: CombatPosition, expenditures| Candidate {
            window: 0,
            order: vec![0, 1],
            // All attempted suffixes happened to share an endpoint. Continuation
            // equality must concern the saved shuffle root, not this endpoint.
            current: source.successor.clone(),
            prefix: vec![],
            cursor: 0,
            replacement: vec![],
            phase: Phase::Search,
            boundary: Some(boundary),
            suffix_inputs: source.inputs.clone(),
            child: None,
            submitted_child_work: 0,
            expenditures,
        };
        let mut stats = SolverStats::default();
        let mut boundaries = vec![root.clone(), root.clone()];
        let mut reordered = root.clone();
        let mut cards = root
            .combat
            .zones
            .discard_pile
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        cards.swap(0, 1);
        reordered.combat.zones.discard_pile = cards.into();
        boundaries.push(reordered);
        let mut different_rng = root.clone();
        different_rng.combat.rng.shuffle_rng.random_long();
        boundaries.push(different_rng);
        for boundary in boundaries {
            job.candidate = Some(candidate(boundary, CombatInputExpendituresV1::default()));
            job.queue_continuation(&mut stats);
        }
        let spent = CombatInputExpendituresV1::from_inputs(&[ClientInput::DiscardPotion(0)]);
        job.candidate = Some(candidate(root.clone(), spent));
        job.queue_continuation(&mut stats);
        assert_eq!(stats.shuffle_continuations_queued, 4);
        assert_eq!(stats.shuffle_duplicate_continuations, 1);
        assert_eq!(job.continuations.len(), 4);
        assert_eq!(job.continuations[0].current, root);
        assert!(job
            .continuations
            .iter()
            .all(|c| c.boundary.is_none() && c.suffix_inputs.is_empty() && c.child.is_none()));
    }
}
