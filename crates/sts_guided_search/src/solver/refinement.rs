//! Local edits of replay-verified plans, sharing the enclosing search budget.
//! A retained suffix is a proposal: every changed transition runs the engine.
use super::*;
use crate::shuffle_order::rebind_follow_input;
use sts_core::sim::combat::is_exact_combat_victory;

#[derive(Clone, Debug, Serialize)]
pub struct PotionEditOrigin {
    pub source_found_at_work: usize,
    pub source_inputs: Vec<ClientInput>,
    pub input_index: usize,
    /// None removes the original potion use/discard at input_index.
    pub inserted: Option<ClientInput>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrderEditOrigin {
    pub source_found_at_work: usize,
    pub source_inputs: Vec<ClientInput>,
    /// Swap adjacent complete card actions [start, middle), [middle, end).
    /// Selections travel with their producing action; never cross EndTurn.
    pub start: usize,
    pub middle: usize,
    pub end: usize,
}

enum Edit {
    Potion(Option<ClientInput>),
    Order { middle: usize, end: usize },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum SuffixFamily {
    OriginalOrder,
    InsertPotion(usize),
    RemovePotion,
}

impl Edit {
    fn suffix_family(&self) -> SuffixFamily {
        match self {
            Self::Order { .. } => SuffixFamily::OriginalOrder,
            Self::Potion(None) => SuffixFamily::RemovePotion,
            Self::Potion(Some(ClientInput::UsePotion { potion_index, .. })) => {
                SuffixFamily::InsertPotion(*potion_index)
            }
            Self::Potion(Some(_)) => unreachable!("only potion uses are inserted"),
        }
    }
}

/// A completed fixed-suffix check at a real player-turn boundary. Full state,
/// source cursor and explicit expenditures are required; equal-looking hands
/// or different fixed-RNG shuffle orders never establish this equivalence.
struct SettledSuffix {
    family: SuffixFamily,
    current: CombatPosition,
    remaining: VecDeque<usize>,
    spent: CombatInputExpendituresV1,
    input_count: usize,
}

impl SettledSuffix {
    fn covers(&self, candidate: &Candidate) -> bool {
        self.input_count <= candidate.inputs.len()
            && self.remaining == candidate.remaining
            && self.spent == CombatInputExpendituresV1::from_inputs(&candidate.inputs)
            && self.current == candidate.current
    }
}

pub(super) enum Event {
    Progress,
    TimedOut,
    Done,
    Win {
        inputs: Vec<ClientInput>,
        expected: CombatPosition,
        origin: SolverPlanOrigin,
    },
}

struct Candidate {
    current: CombatPosition,
    inputs: Vec<ClientInput>,
    remaining: VecDeque<usize>,
    edit: Edit,
    insertion_pending: bool,
    at: usize,
    check_boundary: bool,
    checkpoints: Vec<SettledSuffix>,
}

pub(super) struct PlanEditJob {
    source: SolverPlan,
    states: Vec<CombatPosition>,
    at: usize,
    edits: VecDeque<Edit>,
    prepared: bool,
    /// Check existing discards before enumerating insertion points. A deletion
    /// still executes the full changed suffix and must pass normal outcome retention.
    discard_checks: VecDeque<usize>,
    candidate: Option<Candidate>,
    max_uses: Option<usize>,
    potions: bool,
    order: bool,
    /// At most one completed suffix per source cursor and edit family/slot.
    /// Storage scales with the source line and potion slots, not all edits.
    /// Replacement can lose a reuse opportunity, never change the result.
    completed: HashMap<(usize, SuffixFamily), SettledSuffix>,
}

impl PlanEditJob {
    /// Bounded publication cleanup, including wins produced by another edit.
    /// It checks only existing discards and cannot propose inserted potion uses.
    pub(super) fn discard_cleanup(
        root: CombatPosition,
        source: SolverPlan,
        max_uses: Option<usize>,
    ) -> Self {
        let mut job = Self::new(root, source, max_uses, true, false);
        job.at = job.source.inputs.len();
        job
    }

    pub(super) fn source_inputs(&self) -> &[ClientInput] {
        &self.source.inputs
    }

    pub(super) fn new(
        root: CombatPosition,
        source: SolverPlan,
        max_uses: Option<usize>,
        potions: bool,
        order: bool,
    ) -> Self {
        let discard_checks = source
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(i, input)| {
                (potions && matches!(input, ClientInput::DiscardPotion(_))).then_some(i)
            })
            .collect();
        Self {
            source,
            states: vec![root],
            at: 0,
            edits: VecDeque::new(),
            prepared: false,
            discard_checks,
            candidate: None,
            max_uses,
            potions,
            order,
            completed: HashMap::new(),
        }
    }

    fn remember(&mut self, checkpoints: impl IntoIterator<Item = SettledSuffix>) {
        for checkpoint in checkpoints {
            let Some(&cursor) = checkpoint.remaining.front() else {
                continue;
            };
            let key = (cursor, checkpoint.family);
            let keep_previous = self.completed.get(&key).is_some_and(|previous| {
                previous.current == checkpoint.current
                    && previous.remaining == checkpoint.remaining
                    && previous.spent == checkpoint.spent
                    && previous.input_count <= checkpoint.input_count
            });
            if !keep_previous {
                self.completed.insert(key, checkpoint);
            }
        }
    }

    fn finish_candidate(&mut self) -> Candidate {
        let mut candidate = self.candidate.take().unwrap();
        self.remember(std::mem::take(&mut candidate.checkpoints));
        candidate
    }

    fn card_action_end(&self, start: usize) -> Option<usize> {
        use sts_core::state::core::EngineState;
        if !matches!(
            self.source.inputs.get(start),
            Some(ClientInput::PlayCard { .. })
        ) || !matches!(self.states[start].engine, EngineState::CombatPlayerTurn)
        {
            return None;
        }
        (start + 1..self.states.len())
            .find(|&i| !matches!(self.states[i].engine, EngineState::PendingChoice(_)))
    }

    pub(super) fn advance(
        &mut self,
        limits: CombatStepLimits,
        stats: &mut SolverStats,
    ) -> Result<Event, String> {
        // One source replay, then clone its exact prefix at each edit point.
        if self.states.len() <= self.source.inputs.len() {
            let i = self.states.len() - 1;
            let step = EngineCombatStepper.apply_to_stable(
                &self.states[i],
                self.source.inputs[i].clone(),
                limits,
            );
            stats.engine_steps += step.engine_steps;
            if step.timed_out {
                return Ok(Event::TimedOut);
            }
            if step.truncated {
                return Err("plan edit source replay exceeded engine-step limit".into());
            }
            stats.plan_edit_work += 1;
            stats.plan_edit_source_work += 1;
            self.states.push(step.position);
            if self.states.len() > self.source.inputs.len()
                && self.states.last() != Some(&self.source.successor)
            {
                return Err("plan edit source replay changed".into());
            }
            if self.states.len() > self.source.inputs.len() {
                let checkpoints = (1..self.states.len())
                    .filter_map(|cursor| {
                        let before = &self.states[cursor - 1];
                        let current = &self.states[cursor];
                        (cursor < self.source.inputs.len()
                            && current.combat.turn.turn_count != before.combat.turn.turn_count
                            && matches!(
                                current.engine,
                                sts_core::state::core::EngineState::CombatPlayerTurn
                            ))
                        .then(|| SettledSuffix {
                            family: SuffixFamily::OriginalOrder,
                            current: current.clone(),
                            remaining: (cursor..self.source.inputs.len()).collect(),
                            spent: CombatInputExpendituresV1::from_inputs(
                                &self.source.inputs[..cursor],
                            ),
                            input_count: cursor,
                        })
                    })
                    .collect::<Vec<_>>();
                self.remember(checkpoints);
            }
            return Ok(Event::Progress);
        }
        if self.candidate.is_none() {
            let (at, edit) = if let Some(at) = self.discard_checks.pop_front() {
                (at, Edit::Potion(None))
            } else {
                if self.at == self.source.inputs.len() {
                    return Ok(Event::Done);
                }
                if !self.prepared {
                    let original = &self.source.inputs[self.at];
                    if self.order {
                        if let Some((middle, end)) =
                            self.card_action_end(self.at).and_then(|middle| {
                                self.card_action_end(middle).map(|end| (middle, end))
                            })
                        {
                            self.edits.push_back(Edit::Order { middle, end });
                        }
                    }
                    if self.potions && matches!(original, ClientInput::UsePotion { .. }) {
                        self.edits.push_back(Edit::Potion(None));
                    }
                    let spent =
                        CombatInputExpendituresV1::from_inputs(&self.source.inputs[..self.at]);
                    if self.potions
                        && self
                            .max_uses
                            .is_none_or(|max| (spent.explicit_count() as usize) < max)
                    {
                        self.edits.extend(
                            EngineCombatStepper
                                .legal_action_surface(&self.states[self.at])
                                .atomic_actions
                                .into_iter()
                                .filter(|a| {
                                    matches!(a, ClientInput::UsePotion { .. }) && a != original
                                })
                                .map(|input| Edit::Potion(Some(input))),
                        );
                    }
                    self.prepared = true;
                }
                let Some(edit) = self.edits.pop_front() else {
                    self.at += 1;
                    self.prepared = false;
                    return Ok(Event::Progress);
                };
                (self.at, edit)
            };
            stats.plan_edit_candidates += 1;
            let remaining = match &edit {
                Edit::Potion(inserted) => {
                    (at + usize::from(inserted.is_none())..self.source.inputs.len()).collect()
                }
                Edit::Order { middle, end } => (*middle..*end)
                    .chain(at..*middle)
                    .chain(*end..self.source.inputs.len())
                    .collect(),
            };
            self.candidate = Some(Candidate {
                current: self.states[at].clone(),
                inputs: self.source.inputs[..at].to_vec(),
                remaining,
                insertion_pending: matches!(&edit, Edit::Potion(Some(_))),
                edit,
                at,
                check_boundary: false,
                checkpoints: vec![],
            });
        }
        let c = self.candidate.as_mut().unwrap();
        if c.check_boundary {
            c.check_boundary = false;
            if c.remaining
                .front()
                .and_then(|cursor| self.completed.get(&(*cursor, c.edit.suffix_family())))
                .is_some_and(|previous| previous.covers(c))
            {
                stats.plan_edit_suffix_cache_hits += 1;
                self.finish_candidate();
                return Ok(Event::Progress);
            }
        }
        if is_exact_combat_victory(&c.current) {
            let c = self.finish_candidate();
            let mut origin = self.source.origin.clone();
            match c.edit {
                Edit::Potion(inserted) => {
                    origin.potion_edit = Some(PotionEditOrigin {
                        source_found_at_work: self.source.found_at_work,
                        source_inputs: self.source.inputs.clone(),
                        input_index: c.at,
                        inserted,
                    })
                }
                Edit::Order { middle, end } => {
                    origin.order_edit = Some(OrderEditOrigin {
                        source_found_at_work: self.source.found_at_work,
                        source_inputs: self.source.inputs.clone(),
                        start: c.at,
                        middle,
                        end,
                    })
                }
            }
            return Ok(Event::Win {
                inputs: c.inputs,
                expected: c.current,
                origin,
            });
        }
        let input = if c.insertion_pending {
            match &c.edit {
                Edit::Potion(input) => input.clone(),
                Edit::Order { .. } => unreachable!(),
            }
        } else {
            c.remaining.front().and_then(|&i| {
                rebind_follow_input(&c.current, &self.states[i], &self.source.inputs[i]).ok()
            })
        };
        let Some(input) = input.filter(|i| EngineCombatStepper.is_legal_action(&c.current, i))
        else {
            stats.plan_edit_rejected += 1;
            self.finish_candidate();
            return Ok(Event::Progress);
        };
        let mut spent = CombatInputExpendituresV1::from_inputs(&c.inputs);
        spent.record(&input);
        if self
            .max_uses
            .is_some_and(|max| spent.explicit_count() as usize > max)
        {
            stats.plan_edit_rejected += 1;
            self.finish_candidate();
            return Ok(Event::Progress);
        }
        let step = EngineCombatStepper.apply_to_stable(&c.current, input.clone(), limits);
        stats.engine_steps += step.engine_steps;
        if step.timed_out {
            return Ok(Event::TimedOut);
        }
        if step.truncated {
            return Err("plan edit continuation exceeded engine-step limit".into());
        }
        stats.plan_edit_work += 1;
        match c.edit {
            Edit::Potion(_) => stats.plan_edit_potion_work += 1,
            Edit::Order { .. } => stats.plan_edit_order_work += 1,
        }
        let previous_turn = c.current.combat.turn.turn_count;
        c.current = step.position;
        c.inputs.push(input);
        if c.insertion_pending {
            c.insertion_pending = false;
        } else {
            c.remaining.pop_front();
        }
        if c.current.combat.turn.turn_count != previous_turn
            && matches!(
                c.current.engine,
                sts_core::state::core::EngineState::CombatPlayerTurn
            )
            && !c.remaining.is_empty()
        {
            c.check_boundary = true;
            c.checkpoints.push(SettledSuffix {
                family: c.edit.suffix_family(),
                current: c.current.clone(),
                remaining: c.remaining.clone(),
                spent: CombatInputExpendituresV1::from_inputs(&c.inputs),
                input_count: c.inputs.len(),
            });
        }
        Ok(Event::Progress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::{
        cards::CardId,
        monsters::EnemyId,
        potions::{Potion, PotionId},
    };
    use sts_core::runtime::combat::CombatCard;
    use sts_core::state::core::EngineState;

    fn fixture() -> (CombatPosition, SolverPlan) {
        let mut combat = sts_core::test_support::blank_test_combat();
        let mut monster = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        monster.current_hp = 12;
        monster.max_hp = 12;
        combat.entities.monsters = vec![monster];
        combat.zones.hand = (0..3)
            .map(|i| CombatCard::for_scenario(CardId::Defend, i + 1))
            .collect();
        combat.zones.draw_pile = (0..2)
            .map(|i| CombatCard::for_scenario(CardId::Strike, i + 10))
            .collect();
        combat.entities.potions = vec![
            Some(Potion::new(PotionId::SpeedPotion, 1)),
            Some(Potion::new(PotionId::SteroidPotion, 2)),
        ];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let mut current = root.clone();
        let mut inputs = vec![
            ClientInput::PlayCard {
                card_index: 0,
                target: None
            };
            3
        ];
        inputs.push(ClientInput::EndTurn);
        for input in &inputs {
            current = EngineCombatStepper
                .apply_to_stable(&current, input.clone(), limits())
                .position;
        }
        for _ in 0..2 {
            let card_index = current
                .combat
                .zones
                .hand
                .iter()
                .position(|c| c.id == CardId::Strike)
                .unwrap();
            let input = ClientInput::PlayCard {
                card_index,
                target: Some(1),
            };
            current = EngineCombatStepper
                .apply_to_stable(&current, input.clone(), limits())
                .position;
            inputs.push(input);
        }
        assert!(is_exact_combat_victory(&current));
        let outcome =
            materialize_replay_checked_combat_outcome_v1(&root, &inputs, &current).unwrap();
        let plan = SolverPlan {
            inputs,
            outcome,
            successor: current,
            found_at_work: 0,
            min_stable_hp: 1,
            origin: SolverPlanOrigin {
                width: 4,
                card_only: true,
                recovery: false,
                observed_at_work: 0,
                pass_search_work: 0,
                shuffle: None,
                potion_edit: None,
                order_edit: None,
                order_family: None,
            },
        };
        (root, plan)
    }

    fn limits() -> CombatStepLimits {
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        }
    }

    fn settle(
        root: &CombatPosition,
        source: &SolverPlan,
        memoize: bool,
    ) -> (
        Vec<(CombatWitnessMaterializedOutcomeV1, usize)>,
        SolverStats,
    ) {
        let mut job = PlanEditJob::new(root.clone(), source.clone(), None, true, true);
        let mut outcomes = vec![(source.outcome.clone(), source.inputs.len())];
        let mut stats = SolverStats::default();
        for _ in 0..10000 {
            if !memoize {
                job.completed.clear();
            }
            match job.advance(limits(), &mut stats).unwrap() {
                Event::Win {
                    inputs, expected, ..
                } => {
                    let mut replay = root.clone();
                    for input in &inputs {
                        assert!(EngineCombatStepper.is_legal_action(&replay, input));
                        replay = EngineCombatStepper
                            .apply_to_stable(&replay, input.clone(), limits())
                            .position;
                    }
                    assert_eq!(replay, expected);
                    outcomes.push((
                        materialize_replay_checked_combat_outcome_v1(root, &inputs, &replay)
                            .unwrap(),
                        inputs.len(),
                    ));
                }
                Event::Done => return (outcomes, stats),
                Event::Progress => {}
                Event::TimedOut => panic!("no deadline"),
            }
            assert!(
                job.completed.len()
                    <= source.inputs.len() * (root.combat.entities.potions.len() + 2)
            );
        }
        panic!("finite edit set did not finish");
    }

    #[test]
    fn completed_suffix_reuse_preserves_resources_and_shortest_edit_witnesses() {
        let (root, source) = fixture();
        let (cached, cached_stats) = settle(&root, &source, true);
        let (uncached, uncached_stats) = settle(&root, &source, false);
        for (from, into) in [(&cached, &uncached), (&uncached, &cached)] {
            for (outcome, inputs) in from {
                assert!(into.iter().any(|(other, n)| outcome
                    .has_same_materialized_resources_v1(other)
                    && n <= inputs));
            }
        }
        assert!(cached_stats.plan_edit_suffix_cache_hits > 0);
        assert!(cached_stats.plan_edit_work < uncached_stats.plan_edit_work);
        for stats in [cached_stats, uncached_stats] {
            assert_eq!(
                stats.plan_edit_work,
                stats.plan_edit_source_work
                    + stats.plan_edit_potion_work
                    + stats.plan_edit_order_work
            );
            assert_eq!(stats.plan_edit_source_work, source.inputs.len());
            assert!(stats.plan_edit_potion_work > 0);
            assert!(stats.plan_edit_order_work > 0);
        }
    }

    fn replay_source(root: &CombatPosition, source: &mut SolverPlan) {
        let mut current = root.clone();
        for input in &source.inputs {
            assert!(EngineCombatStepper.is_legal_action(&current, input));
            current = EngineCombatStepper
                .apply_to_stable(&current, input.clone(), limits())
                .position;
        }
        source.outcome =
            materialize_replay_checked_combat_outcome_v1(root, &source.inputs, &current).unwrap();
        source.successor = current;
    }

    fn first_edit(
        root: &CombatPosition,
        source: &SolverPlan,
    ) -> (CombatWitnessMaterializedOutcomeV1, SolverStats) {
        let mut job = PlanEditJob::new(root.clone(), source.clone(), None, true, true);
        let mut stats = SolverStats::default();
        // Source plus one changed suffix, serviced one transition at a time.
        for _ in 0..source.inputs.len() * 3 {
            if let Event::Win {
                inputs,
                expected,
                origin,
            } = job.advance(limits(), &mut stats).unwrap()
            {
                assert_eq!(origin.potion_edit.unwrap().inserted, None);
                assert_eq!(stats.plan_edit_candidates, 1);
                let mut replay = root.clone();
                for input in &inputs {
                    replay = EngineCombatStepper
                        .apply_to_stable(&replay, input.clone(), limits())
                        .position;
                }
                assert_eq!(replay, expected);
                return (
                    materialize_replay_checked_combat_outcome_v1(root, &inputs, &replay).unwrap(),
                    stats,
                );
            }
        }
        panic!("discard check was delayed behind insertion/order edits");
    }

    #[test]
    fn unused_discard_is_checked_before_insertion_and_order_edits() {
        let (root, mut source) = fixture();
        source.inputs.insert(3, ClientInput::DiscardPotion(0));
        replay_source(&root, &mut source);
        let (clean, stats) = first_edit(&root, &source);
        assert_eq!(clean.final_hp, source.outcome.final_hp);
        assert!(clean.strictly_dominates(&source.outcome));
        assert!(stats.plan_edit_work <= 2 * source.inputs.len());
    }

    #[test]
    fn discard_for_entropic_brew_is_not_assumed_useless() {
        let (mut root, mut source) = fixture();
        root.combat.entities.potions[1] = Some(Potion::new(PotionId::EntropicBrew, 2));
        source.inputs.insert(3, ClientInput::DiscardPotion(0));
        source.inputs.insert(
            4,
            ClientInput::UsePotion {
                potion_index: 1,
                target: None,
            },
        );
        replay_source(&root, &mut source);
        let (without_discard, _) = first_edit(&root, &source);
        assert_eq!(without_discard.final_hp, source.outcome.final_hp);
        assert!(!without_discard.strictly_dominates(&source.outcome));
        assert_eq!(
            source
                .successor
                .combat
                .entities
                .potions
                .iter()
                .filter(|p| p.is_some())
                .count(),
            2
        );
    }

    #[test]
    fn suffix_identity_includes_piles_rng_expenditures_and_remaining_input_order() {
        let (root, source) = fixture();
        let candidate = Candidate {
            current: root.clone(),
            inputs: vec![],
            remaining: (0..source.inputs.len()).collect(),
            edit: Edit::Potion(None),
            insertion_pending: false,
            at: 0,
            check_boundary: true,
            checkpoints: vec![],
        };
        let mut memo = SettledSuffix {
            family: SuffixFamily::RemovePotion,
            current: root,
            remaining: candidate.remaining.clone(),
            spent: CombatInputExpendituresV1::default(),
            input_count: 0,
        };
        assert!(memo.covers(&candidate));
        memo.current.combat.zones.draw_pile.swap(0, 1);
        assert!(!memo.covers(&candidate));
        memo.current = candidate.current.clone();
        memo.current.combat.rng.shuffle_rng.random_long();
        assert!(!memo.covers(&candidate));
        memo.current = candidate.current.clone();
        memo.spent.record(&ClientInput::DiscardPotion(0));
        assert!(!memo.covers(&candidate));
        memo.spent = CombatInputExpendituresV1::default();
        memo.remaining.swap(0, 1);
        assert!(!memo.covers(&candidate));
        memo.remaining = candidate.remaining.clone();
        memo.input_count = 1;
        assert!(!memo.covers(&candidate));
    }
}
