use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sts_core::sim::combat::CombatPosition;
use sts_core::state::core::ClientInput;

use crate::types::TurnOptionAction;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OracleCombatWitnessSatisfaction {
    #[default]
    FirstWitness,
    HpLossAtMost(u32),
    FinalHpAtLeast(i32),
    FinalHpAndMaxHpAtLeast {
        final_hp: i32,
        final_max_hp: i32,
    },
    FirstWitnessWithoutNewExternalBurden,
    HpLossAtMostWithoutNewExternalBurden(u32),
    PotionFreeHpLossAtMostWithoutNewExternalBurden(u32),
    BudgetOrExhaustion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OracleCombatWitnessReplayError {
    IllegalInput { action_index: usize },
    TransitionStepLimit { action_index: usize },
    SuccessorMismatch { action_index: usize },
    FinalStateIsNotWin,
    TrajectoryQueryMismatch,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OracleCombatWitness {
    pub actions: Vec<TurnOptionAction>,
    pub final_position: CombatPosition,
    pub negative_log_policy: f64,
    pub replay_engine_steps: usize,
    #[serde(default)]
    pub discovery_source: OracleCombatWitnessDiscoverySource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PotionContractUsage {
    pub expenditures: u32,
    used_slot_mask: u64,
    all_used_slots_representable: bool,
}

pub(crate) fn potion_input_uses_allowed_slot(
    input: &ClientInput,
    allowed_slots: Option<u64>,
) -> bool {
    let slot = match input {
        ClientInput::UsePotion { potion_index, .. } => Some(*potion_index),
        ClientInput::DiscardPotion(slot) => Some(*slot),
        _ => None,
    };
    slot.is_none_or(|slot| {
        allowed_slots.is_none_or(|allowed_slots| {
            u32::try_from(slot)
                .ok()
                .and_then(|slot| 1_u64.checked_shl(slot))
                .is_some_and(|slot_mask| allowed_slots & slot_mask != 0)
        })
    })
}

pub(crate) fn actions_use_only_allowed_potion_slots(
    actions: &[TurnOptionAction],
    allowed_slots: Option<u64>,
) -> bool {
    actions
        .iter()
        .all(|action| potion_input_uses_allowed_slot(&action.input, allowed_slots))
}

pub(crate) fn trajectory_potion_contract_usage(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    final_position: &CombatPosition,
) -> PotionContractUsage {
    let mut all_used_slots_representable = true;
    let remaining_uuids = final_position
        .combat
        .entities
        .potions
        .iter()
        .flatten()
        .map(|potion| potion.uuid)
        .collect::<BTreeSet<_>>();
    let missing_starting_slots = root
        .combat
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            let potion = potion.as_ref()?;
            if remaining_uuids.contains(&potion.uuid) {
                return None;
            }
            let slot_mask = u32::try_from(slot)
                .ok()
                .and_then(|slot| 1_u64.checked_shl(slot));
            if slot_mask.is_none() {
                all_used_slots_representable = false;
            }
            slot_mask
        })
        .fold(0_u64, |mask, slot| mask | slot);
    let mut explicit_slot_mask = 0_u64;
    let explicit_expenditures = actions
        .iter()
        .filter(|action| {
            let slot = match action.input {
                ClientInput::UsePotion { potion_index, .. } => Some(potion_index),
                ClientInput::DiscardPotion(slot) => Some(slot),
                _ => None,
            };
            let Some(slot) = slot else {
                return false;
            };
            let slot_mask = u32::try_from(slot)
                .ok()
                .and_then(|slot| 1_u64.checked_shl(slot));
            if let Some(slot_mask) = slot_mask {
                explicit_slot_mask |= slot_mask;
            } else {
                all_used_slots_representable = false;
            }
            true
        })
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    let implicit_starting_slots = missing_starting_slots & !explicit_slot_mask;
    PotionContractUsage {
        // Explicit inputs also count generated potions. A starting UUID that
        // disappears without any expenditure action targeting its slot is an
        // additional passive expenditure, such as Fairy Potion.
        expenditures: explicit_expenditures.saturating_add(implicit_starting_slots.count_ones()),
        used_slot_mask: explicit_slot_mask | missing_starting_slots,
        all_used_slots_representable,
    }
}

/// Counts potion expenditures on one replay-exact action line, including
/// passive starting-potion consumption such as Fairy Potion.
pub fn exact_trajectory_potion_expenditures(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    final_position: &CombatPosition,
) -> u32 {
    trajectory_potion_contract_usage(root, actions, final_position).expenditures
}

pub(crate) fn trajectory_within_potion_contract(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    final_position: &CombatPosition,
    max_potions_used: Option<u32>,
    allowed_potion_slots: Option<u64>,
) -> bool {
    let usage = trajectory_potion_contract_usage(root, actions, final_position);
    max_potions_used.is_none_or(|limit| usage.expenditures <= limit)
        && allowed_potion_slots.is_none_or(|allowed_slots| {
            usage.all_used_slots_representable && usage.used_slot_mask & !allowed_slots == 0
        })
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleCombatWitnessDiscoverySource {
    /// Older serialized witnesses predate discovery provenance. They remain
    /// exact replay evidence but cannot prove which search capability found
    /// their action sequence.
    #[default]
    LegacyUnattributed,
    PlannerSearch,
    PolicyDiscrepancySearch,
    RestoredExactActions,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OracleCombatWitnessProgressSnapshot {
    pub retained_states: usize,
    pub queued_anchor_entries: usize,
    pub queued_guided_entries: Vec<usize>,
    pub guide_queues: Vec<OracleCombatGuideQueueSnapshot>,
    pub max_player_turn: u32,
    pub max_path_atomic_depth: usize,
    pub max_completed_turn_options_at_state: usize,
    pub generation_gap_count: usize,
    pub pending_witness_replay: bool,
    pub root_state: Option<OracleCombatWitnessStateProgressSnapshot>,
    pub deepest_survival_state: Option<OracleCombatDeepStateSnapshot>,
    pub deepest_progress_state: Option<OracleCombatDeepStateSnapshot>,
    /// Exact public action prefix that reaches `deepest_survival_state`.
    /// Diagnostic only; it has no authority over queue ordering.
    pub deepest_survival_actions: Vec<TurnOptionAction>,
    /// Exact public action prefix that reaches `deepest_progress_state`.
    /// Diagnostic only; it has no authority over queue ordering.
    pub deepest_progress_actions: Vec<TurnOptionAction>,
    /// For each of the most recent retained player turns, the state with the
    /// highest player HP (then least remaining enemy HP). This is diagnostic:
    /// it exposes whether deeper search is advancing only along a dying line
    /// without assigning that envelope any search authority.
    pub recent_turn_survival_envelope: Vec<OracleCombatDeepStateSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OracleCombatGuideQueueSnapshot {
    pub lane_id: u32,
    pub entries: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OracleCombatGuideRankSnapshot {
    pub lane_id: u32,
    pub states_ahead: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OracleCombatDeepStateSnapshot {
    pub player_turn: u32,
    pub player_hp: i32,
    pub player_block: i32,
    pub alive_enemy_count: usize,
    pub enemy_total_hp: i32,
    pub hand_size: usize,
    pub draw_pile_size: usize,
    pub discard_pile_size: usize,
    pub exhaust_pile_size: usize,
    pub path_atomic_depth: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OracleCombatWitnessStateProgressSnapshot {
    pub exact_state_hash: String,
    pub path_atomic_depth: usize,
    pub path_negative_log_policy: f64,
    pub generator_work: usize,
    pub generator_engine_steps: usize,
    pub completed_turn_options: usize,
    pub retained_generator_work_items: usize,
    pub synced_options: usize,
    pub anchor_states_ahead: Option<usize>,
    pub guided_states_ahead: Option<Vec<usize>>,
    pub guided_lane_ranks: Option<Vec<OracleCombatGuideRankSnapshot>>,
}
