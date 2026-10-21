use serde::Serialize;

use crate::content::cards::{CardTarget, CardType};
use crate::runtime::combat::CardLineageId;

use super::super::SearchTerminalLabel;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2ActionFacts {
    pub action_kind: &'static str,
    pub card: Option<CombatSearchV2ActionCardFacts>,
    pub target: Option<CombatSearchV2ActionTargetFacts>,
    pub pending_choice_selection: Option<CombatSearchV2ActionPendingChoiceSelectionFacts>,
    pub immediate: CombatSearchV2ActionImmediateFacts,
    pub mechanics: CombatSearchV2ActionMechanicsFacts,
    pub exact_one_step_delta: CombatSearchV2ActionExactDeltaFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2ActionPendingChoiceSelectionRole {
    Value,
    Removal,
    Neutral,
    Cancel,
}

/// Context-local ordering facts for one legal action on a pending-choice
/// surface. `primary`, `secondary`, and `selected_count_tiebreak` form one
/// lexicographic key; they are not cardinal values and must not be added to
/// unrelated combat scores or compared across different choice surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2ActionPendingChoiceSelectionFacts {
    pub role: CombatSearchV2ActionPendingChoiceSelectionRole,
    pub primary: i32,
    pub secondary: i32,
    pub selected_count_tiebreak: i32,
    pub policy_log2_bias: i32,
    /// Exact card identity when this selection explicitly prepares the next
    /// draw by moving one card to the top of the draw pile.
    pub prepared_draw_card_lineage: Option<CardLineageId>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2ActionCardFacts {
    pub hand_index: usize,
    pub lineage: CardLineageId,
    pub card_id: String,
    pub name: &'static str,
    pub upgraded: bool,
    pub card_type: CardType,
    pub definition_target: CardTarget,
    pub effective_target: CardTarget,
    pub cost_for_turn: i32,
    pub base_cost: i8,
    pub evaluated_damage: i32,
    pub evaluated_block: i32,
    pub evaluated_magic: i32,
    pub exhaust: bool,
    pub ethereal: bool,
    pub innate: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2ActionTargetFacts {
    pub target_slot: usize,
    pub entity_id: usize,
    pub enemy_id: String,
    pub hp: i32,
    pub block: i32,
    pub visible_incoming_damage: i32,
    pub vulnerable: i32,
    pub weak: i32,
    pub strength: i32,
    pub timed_enemy_threat: Option<CombatSearchV2TimedEnemyThreatTargetFacts>,
    pub attack_retaliation: Option<CombatSearchV2AttackRetaliationTargetFacts>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TimedEnemyThreatTargetFacts {
    pub kind: &'static str,
    pub owner_turns_until_trigger: u32,
    pub raw_player_damage: i32,
    pub canceled_by_owner_death: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2AttackRetaliationTargetFacts {
    pub power_source_count: usize,
    pub raw_player_damage_per_damage_event: i32,
    pub projected_player_block_loss_for_next_damage_event: i32,
    pub projected_player_hp_loss_for_next_damage_event: i32,
    pub visible_growth_amount: i32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2ActionImmediateFacts {
    pub damage_hint: i32,
    pub action_payload_damage_hint: i32,
    pub action_payload_damage_hit_count_hint: usize,
    pub block_hint: i32,
    pub target_progress_hint: i32,
    pub all_enemy_progress_hint: i32,
    pub exhausts_card: bool,
    pub creates_pending_choice_after_one_step: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2ActionMechanicsFacts {
    pub direct: CombatSearchV2ActionDirectMechanicsFacts,
    pub reactive: CombatSearchV2ActionReactiveMechanicsFacts,
    pub access: CombatSearchV2ActionAccessMechanicsFacts,
    pub resource_timing: CombatSearchV2ActionResourceTimingFacts,
    pub derived: CombatSearchV2ActionDerivedMechanicsFacts,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2ActionResourceTimingFacts {
    pub hand_resource_conversion: bool,
    pub hand_exhaust_target_count: usize,
    pub hand_exhaust_fuel_count: usize,
    pub hand_exhaust_high_value_count: usize,
    pub hand_exhaust_value_at_risk: i32,
    pub conversion_damage_hint: i32,
    pub conversion_block_hint: i32,
    pub conversion_window_score: i32,
    pub premature_conversion_risk: i32,
    pub ordering_score: i32,
    pub role_rank_adjustment: i32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2ActionDirectMechanicsFacts {
    pub persistent_enemy_strength_down: i32,
    pub temporary_enemy_strength_down: i32,
    pub visible_attack_mitigation_hint: i32,
    pub enemy_weak: i32,
    pub enemy_vulnerable: i32,
    pub enemy_strength_gain: i32,
    pub visible_attack_pressure_hint: i32,
    pub player_strength_gain: i32,
    pub player_temporary_strength_gain: i32,
    pub player_recurring_turn_energy_delta: i32,
    pub player_attack_replay_charges: i32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2ActionReactiveMechanicsFacts {
    pub player_hp_loss: i32,
    pub attack_retaliation_trigger_count_hint: usize,
    pub attack_retaliation_raw_player_damage_hint: i32,
    pub attack_retaliation_player_block_loss_hint: i32,
    pub attack_retaliation_player_hp_loss_hint: i32,
    pub player_block: i32,
    pub enemy_damage: i32,
    pub bad_draw_cards: i32,
    pub forced_turn_end: bool,
    pub enemy_strength_gain: i32,
    pub visible_attack_pressure_hint: i32,
    pub enemy_weak: i32,
    pub enemy_vulnerable: i32,
    pub future_attack_damage_percent_gain_hint: i32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2ActionAccessMechanicsFacts {
    pub declared_draw_cards: i32,
    pub conditional_draw_cards: i32,
    pub total_draw_cards: i32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2ActionDerivedMechanicsFacts {
    pub mitigation_score: i32,
    pub enemy_scaling_risk_score: i32,
    pub reactive_risk_score: i32,
    pub net_mitigation_score: i32,
    pub enemy_weak: i32,
    pub enemy_vulnerable: i32,
    pub enemy_strength_gain: i32,
    pub visible_attack_pressure_hint: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2ActionExactDeltaFacts {
    pub status: &'static str,
    pub terminal: SearchTerminalLabel,
    pub engine_steps: usize,
    pub player_hp_delta: i32,
    pub player_block_delta: i32,
    pub player_strength_delta: i32,
    pub energy_delta: i32,
    pub hand_delta: i32,
    pub draw_delta: i32,
    pub discard_delta: i32,
    pub exhaust_delta: i32,
    pub limbo_delta: i32,
    pub queued_cards_delta: i32,
    pub total_enemy_hp_delta: i32,
    pub total_enemy_block_delta: i32,
    pub pending_choice_present: bool,
    pub pending_choice_estimated_action_fanout: usize,
}
