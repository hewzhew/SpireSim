use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sts_core::content::cards::CardId;
use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper};
use sts_core::state::core::{ClientInput, EngineState, PendingChoice};

use crate::types::{exact_hash, TurnOptionAction};
use crate::witness::OracleCombatWitnessReplayError;

/// A path fact observed from one exact combat transition.
///
/// Events are mechanics evidence, not search lanes.  A query may combine any
/// supported atoms without creating a new scheduler or ranking vocabulary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CombatTrajectoryEventV1 {
    GeneratedCardSelected {
        card_id: CardId,
        choice_index: usize,
    },
    CardLethalPayoff {
        card_id: CardId,
        target_entity_id: usize,
        target_slot: u8,
        target_hp_before: i32,
        target_block_before: i32,
        target_hp_after: Option<i32>,
        target_alive_after: Option<bool>,
        player_max_hp_gain: i32,
        player_gold_gain: i32,
    },
}

/// One reusable path requirement.  The first vertical slice deliberately
/// keeps the atom vocabulary small; adding another mechanic belongs here,
/// not in a permanent experiment lane.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "atom", rename_all = "snake_case")]
pub enum CombatTrajectoryAtomV1 {
    GeneratedCardSelected { card_id: CardId },
    CardLethalPayoff { card_id: CardId },
}

/// Minimal composable trajectory language for bounded combat experiments.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum CombatTrajectoryQueryV1 {
    True,
    Eventually {
        atom: CombatTrajectoryAtomV1,
    },
    All {
        formulas: Vec<CombatTrajectoryQueryV1>,
    },
}

impl CombatTrajectoryQueryV1 {
    pub fn eventually(atom: CombatTrajectoryAtomV1) -> Self {
        Self::Eventually { atom }
    }

    pub fn all(formulas: impl IntoIterator<Item = Self>) -> Self {
        Self::All {
            formulas: formulas.into_iter().collect(),
        }
    }

    /// Reports whether one exact typed requirement occurs anywhere in this
    /// composable query. Search callers may use this to install a matching
    /// semantic guide without reparsing serialized query text.
    pub fn contains_atom(&self, expected: CombatTrajectoryAtomV1) -> bool {
        match self {
            Self::True => false,
            Self::Eventually { atom } => *atom == expected,
            Self::All { formulas } => formulas
                .iter()
                .any(|formula| formula.contains_atom(expected)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum CombatTrajectoryQueryCompileErrorV1 {
    UnsupportedLethalPayoffCard { card_id: CardId },
}

impl std::fmt::Display for CombatTrajectoryQueryCompileErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedLethalPayoffCard { card_id } => write!(
                formatter,
                "typed lethal-payoff observation is not implemented for {}",
                card_id.stable_id()
            ),
        }
    }
}

impl std::error::Error for CombatTrajectoryQueryCompileErrorV1 {}

#[derive(Clone, Debug)]
pub(crate) struct CompiledCombatTrajectoryQueryV1 {
    requirements: Arc<[CombatTrajectoryAtomV1]>,
}

impl CompiledCombatTrajectoryQueryV1 {
    pub(crate) fn compile(
        query: &CombatTrajectoryQueryV1,
    ) -> Result<Self, CombatTrajectoryQueryCompileErrorV1> {
        let mut requirements = Vec::new();
        flatten_requirements(query, &mut requirements)?;
        requirements.dedup();
        Ok(Self {
            requirements: requirements.into(),
        })
    }

    pub(crate) fn initial_progress(&self) -> CombatTrajectoryProgressV1 {
        CombatTrajectoryProgressV1 {
            satisfied: vec![0; self.requirements.len().div_ceil(u64::BITS as usize)],
        }
    }

    pub(crate) fn advance(
        &self,
        progress: &CombatTrajectoryProgressV1,
        before: &CombatPosition,
        input: &ClientInput,
        after: &CombatPosition,
    ) -> CombatTrajectoryProgressV1 {
        let mut advanced = progress.clone();
        for event in observe_combat_trajectory_transition_v1(before, input, after) {
            for (index, requirement) in self.requirements.iter().enumerate() {
                if requirement.matches(&event) {
                    advanced.set(index);
                }
            }
        }
        advanced
    }

    pub(crate) fn is_satisfied(&self, progress: &CombatTrajectoryProgressV1) -> bool {
        (0..self.requirements.len()).all(|index| progress.contains(index))
    }

    pub(crate) fn evaluation(
        &self,
        progress: &CombatTrajectoryProgressV1,
    ) -> CombatTrajectoryQueryEvaluationV1 {
        let requirements = self
            .requirements
            .iter()
            .enumerate()
            .map(|(index, atom)| CombatTrajectoryRequirementEvaluationV1 {
                atom: *atom,
                satisfied: progress.contains(index),
            })
            .collect::<Vec<_>>();
        CombatTrajectoryQueryEvaluationV1 {
            satisfied: requirements.iter().all(|requirement| requirement.satisfied),
            requirements,
        }
    }

    pub(crate) fn evaluation_for_events(
        &self,
        events: &[CombatTrajectoryEventV1],
    ) -> CombatTrajectoryQueryEvaluationV1 {
        let mut progress = self.initial_progress();
        for event in events {
            for (index, requirement) in self.requirements.iter().enumerate() {
                if requirement.matches(event) {
                    progress.set(index);
                }
            }
        }
        self.evaluation(&progress)
    }
}

fn flatten_requirements(
    query: &CombatTrajectoryQueryV1,
    requirements: &mut Vec<CombatTrajectoryAtomV1>,
) -> Result<(), CombatTrajectoryQueryCompileErrorV1> {
    match query {
        CombatTrajectoryQueryV1::True => Ok(()),
        CombatTrajectoryQueryV1::Eventually { atom } => {
            if let CombatTrajectoryAtomV1::CardLethalPayoff { card_id } = atom {
                if *card_id != CardId::Feed {
                    return Err(
                        CombatTrajectoryQueryCompileErrorV1::UnsupportedLethalPayoffCard {
                            card_id: *card_id,
                        },
                    );
                }
            }
            if !requirements.contains(atom) {
                requirements.push(*atom);
            }
            Ok(())
        }
        CombatTrajectoryQueryV1::All { formulas } => {
            for formula in formulas {
                flatten_requirements(formula, requirements)?;
            }
            Ok(())
        }
    }
}

impl CombatTrajectoryAtomV1 {
    fn matches(self, event: &CombatTrajectoryEventV1) -> bool {
        match (self, event) {
            (
                Self::GeneratedCardSelected { card_id },
                CombatTrajectoryEventV1::GeneratedCardSelected {
                    card_id: observed, ..
                },
            ) => card_id == *observed,
            (
                Self::CardLethalPayoff { card_id },
                CombatTrajectoryEventV1::CardLethalPayoff {
                    card_id: observed, ..
                },
            ) => card_id == *observed,
            _ => false,
        }
    }
}

/// Product-state component retained alongside exact simulator identity.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct CombatTrajectoryProgressV1 {
    satisfied: Vec<u64>,
}

impl CombatTrajectoryProgressV1 {
    fn set(&mut self, index: usize) {
        let word = index / u64::BITS as usize;
        let bit = index % u64::BITS as usize;
        if let Some(value) = self.satisfied.get_mut(word) {
            *value |= 1_u64 << bit;
        }
    }

    fn contains(&self, index: usize) -> bool {
        let word = index / u64::BITS as usize;
        let bit = index % u64::BITS as usize;
        self.satisfied
            .get(word)
            .is_some_and(|value| value & (1_u64 << bit) != 0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CombatTrajectoryQueryEvaluationV1 {
    pub satisfied: bool,
    pub requirements: Vec<CombatTrajectoryRequirementEvaluationV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CombatTrajectoryRequirementEvaluationV1 {
    pub atom: CombatTrajectoryAtomV1,
    pub satisfied: bool,
}

pub(crate) fn observe_combat_trajectory_transition_v1(
    before: &CombatPosition,
    input: &ClientInput,
    after: &CombatPosition,
) -> Vec<CombatTrajectoryEventV1> {
    let mut events = Vec::new();
    if let (
        EngineState::PendingChoice(PendingChoice::DiscoverySelect(choice)),
        ClientInput::SubmitDiscoverChoice(choice_index),
    ) = (&before.engine, input)
    {
        if let Some(card_id) = choice.cards.get(*choice_index) {
            events.push(CombatTrajectoryEventV1::GeneratedCardSelected {
                card_id: *card_id,
                choice_index: *choice_index,
            });
        }
    }

    let ClientInput::PlayCard {
        card_index,
        target: Some(target_entity_id),
    } = input
    else {
        return events;
    };
    let Some(card) = before.combat.zones.hand.get(*card_index) else {
        return events;
    };
    // The first supported lethal-payoff owner is Feed.  A target merely
    // dying is insufficient: minions and suppressed rewards must not satisfy
    // the query, so the exact max-HP delta is authoritative.
    if card.id != CardId::Feed {
        return events;
    }
    let Some(target_before) = before
        .combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == *target_entity_id)
    else {
        return events;
    };
    let player_max_hp_gain =
        after.combat.entities.player.max_hp - before.combat.entities.player.max_hp;
    if player_max_hp_gain <= 0 {
        return events;
    }
    let target_after = after
        .combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == *target_entity_id);
    events.push(CombatTrajectoryEventV1::CardLethalPayoff {
        card_id: card.id,
        target_entity_id: *target_entity_id,
        target_slot: target_before.slot,
        target_hp_before: target_before.current_hp,
        target_block_before: target_before.block,
        target_hp_after: target_after.map(|monster| monster.current_hp),
        target_alive_after: target_after.map(|monster| monster.is_alive_for_action()),
        player_max_hp_gain,
        player_gold_gain: after.combat.entities.player.gold - before.combat.entities.player.gold,
    });
    events
}

/// Replays an exact action line and returns the same typed events used by the
/// search product state.  This is the independent final-report verifier.
pub fn replay_combat_trajectory_events_v1(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    stepper: &dyn CombatStepper,
) -> Result<Vec<CombatTrajectoryEventV1>, OracleCombatWitnessReplayError> {
    let mut position = root.clone();
    let mut events = Vec::new();
    for (action_index, action) in actions.iter().enumerate() {
        if stepper
            .choice_for_legal_input(&position, &action.input)
            .is_none()
        {
            return Err(OracleCombatWitnessReplayError::IllegalInput { action_index });
        }
        let result = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: action.engine_steps.max(1),
                deadline: None,
            },
        );
        if result.truncated || result.timed_out {
            return Err(OracleCombatWitnessReplayError::TransitionStepLimit { action_index });
        }
        if exact_hash(&result.position) != action.expected_successor_hash.as_str() {
            return Err(OracleCombatWitnessReplayError::SuccessorMismatch { action_index });
        }
        events.extend(observe_combat_trajectory_transition_v1(
            &position,
            &action.input,
            &result.position,
        ));
        position = result.position;
    }
    Ok(events)
}

pub fn evaluate_combat_trajectory_query_v1(
    query: &CombatTrajectoryQueryV1,
    events: &[CombatTrajectoryEventV1],
) -> Result<CombatTrajectoryQueryEvaluationV1, CombatTrajectoryQueryCompileErrorV1> {
    let compiled = CompiledCombatTrajectoryQueryV1::compile(query)?;
    let mut progress = compiled.initial_progress();
    for event in events {
        for (index, requirement) in compiled.requirements.iter().enumerate() {
            if requirement.matches(event) {
                progress.set(index);
            }
        }
    }
    Ok(compiled.evaluation(&progress))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::state::core::DiscoveryChoiceState;
    use sts_core::test_support::blank_test_combat;

    #[test]
    fn all_eventually_compiles_to_independent_progress_bits() {
        let query = CombatTrajectoryQueryV1::all([
            CombatTrajectoryQueryV1::eventually(CombatTrajectoryAtomV1::GeneratedCardSelected {
                card_id: CardId::MasterOfStrategy,
            }),
            CombatTrajectoryQueryV1::eventually(CombatTrajectoryAtomV1::CardLethalPayoff {
                card_id: CardId::Feed,
            }),
        ]);
        let compiled = CompiledCombatTrajectoryQueryV1::compile(&query).expect("supported query");
        assert!(
            query.contains_atom(CombatTrajectoryAtomV1::CardLethalPayoff {
                card_id: CardId::Feed,
            })
        );
        assert!(
            !query.contains_atom(CombatTrajectoryAtomV1::GeneratedCardSelected {
                card_id: CardId::Discovery,
            })
        );
        let progress = compiled.initial_progress();
        assert!(!compiled.is_satisfied(&progress));

        let before = CombatPosition::new(
            EngineState::PendingChoice(PendingChoice::DiscoverySelect(DiscoveryChoiceState {
                cards: vec![CardId::MasterOfStrategy],
                colorless: true,
                card_type: None,
                amount: 1,
                can_skip: false,
            })),
            blank_test_combat(),
        );
        let after = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
        let progress = compiled.advance(
            &progress,
            &before,
            &ClientInput::SubmitDiscoverChoice(0),
            &after,
        );
        let evaluation = compiled.evaluation(&progress);
        assert!(!evaluation.satisfied);
        assert_eq!(
            evaluation
                .requirements
                .iter()
                .map(|requirement| requirement.satisfied)
                .collect::<Vec<_>>(),
            vec![true, false]
        );
    }
}
