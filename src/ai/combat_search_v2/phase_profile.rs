use super::enemy_mechanics_profile::{enemy_mechanics_profile, EnemyMechanicsProfileV1};
use super::pending_choice_fanout::pending_choice_fanout;
use super::types::CombatSearchV2PhaseProfileReport;
use crate::runtime::combat::CombatState;
use crate::state::core::EngineState;
use sts_combat_knowledge::{
    combat_tactical_enemy_phase_value_v1, combat_tactical_pressure_v1,
    CombatTacticalEnemyPhaseValueV1, CombatTacticalPressureV1,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CombatSearchPhaseProfileV1 {
    pub(super) enemy_phase: CombatTacticalEnemyPhaseValueV1,
    pub(super) enemy_mechanics: EnemyMechanicsProfileV1,
    pub(super) pressure: CombatTacticalPressureV1,
    pub(super) pending_choice: PendingChoicePhaseProfileV1,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct PendingChoicePhaseProfileV1 {
    pub(super) present: bool,
    pub(super) kind: Option<PendingChoicePhaseKind>,
    pub(super) candidate_count: usize,
    pub(super) estimated_action_fanout: usize,
    pub(super) high_fanout: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PendingChoicePhaseKind {
    HandSelect,
    GridSelect,
    DiscoverySelect,
    ScrySelect,
    CardRewardSelect,
    ForeignInfluenceSelect,
    ChooseOneSelect,
    StanceChoice,
}

pub(super) fn combat_search_phase_profile(
    engine: &EngineState,
    combat: &CombatState,
) -> CombatSearchPhaseProfileV1 {
    CombatSearchPhaseProfileV1 {
        enemy_phase: combat_tactical_enemy_phase_value_v1(combat),
        enemy_mechanics: enemy_mechanics_profile(combat),
        pressure: combat_tactical_pressure_v1(combat),
        pending_choice: pending_choice_phase_profile(engine),
    }
}

pub(super) fn combat_search_phase_profile_report(
    profile: CombatSearchPhaseProfileV1,
) -> CombatSearchV2PhaseProfileReport {
    CombatSearchV2PhaseProfileReport {
        profiling_policy: "typed_phase_profile_for_value_and_rollout_no_prune_no_terminal_claim",
        special_enemy_phase_count: profile.special_enemy_phase_count(),
        split_pending_count: profile.enemy_phase.split_pending_count,
        split_debt_hp: profile.enemy_phase.split_debt_hp,
        awakened_rebirth_pending_count: profile.enemy_phase.awakened_rebirth_pending_count,
        awakened_rebirth_debt_hp: profile.enemy_phase.awakened_rebirth_debt_hp,
        guardian_mode_shift_pending_count: profile
            .enemy_mechanics
            .guardian_mode_shift_pending_count,
        guardian_defensive_count: profile.enemy_mechanics.guardian_defensive_count,
        lagavulin_sleeping_count: profile.enemy_mechanics.lagavulin_sleeping_count,
        lagavulin_waking_count: profile.enemy_mechanics.lagavulin_waking_count,
        pending_choice_present: profile.pending_choice.present,
        pending_choice_kind: profile
            .pending_choice
            .kind
            .map(PendingChoicePhaseKind::label),
        pending_choice_candidate_count: profile.pending_choice.candidate_count,
        pending_choice_estimated_action_fanout: profile.pending_choice.estimated_action_fanout,
        high_fanout_pending_choice: profile.pending_choice.high_fanout,
        notes: vec![
            "phase profile is a read-only state classifier used by value and rollout",
            "high-fanout pending choices stop rollout estimates but do not prune main search",
            "enemy phase counts are structural signals, not standalone evidence of line quality",
        ],
    }
}

impl CombatSearchPhaseProfileV1 {
    pub(super) fn special_enemy_phase_count(self) -> usize {
        self.enemy_phase
            .split_pending_count
            .saturating_add(self.enemy_phase.awakened_rebirth_pending_count)
            .saturating_add(self.enemy_mechanics.guardian_mode_shift_pending_count)
            .saturating_add(self.enemy_mechanics.guardian_defensive_count)
            .saturating_add(self.enemy_mechanics.lagavulin_sleeping_count)
            .saturating_add(self.enemy_mechanics.lagavulin_waking_count)
            .saturating_add(
                self.enemy_mechanics
                    .bronze_automaton_spawn_orbs_pending_count,
            )
            .saturating_add(
                self.enemy_mechanics
                    .bronze_automaton_hyper_beam_pending_count,
            )
            .saturating_add(self.enemy_mechanics.bronze_orb_stasis_pending_count)
            .saturating_add(self.enemy_mechanics.bronze_orb_stasis_card_count)
            .saturating_add(self.enemy_mechanics.time_eater_pending_haste_count)
    }
}

impl PendingChoicePhaseKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            PendingChoicePhaseKind::HandSelect => "hand_select",
            PendingChoicePhaseKind::GridSelect => "grid_select",
            PendingChoicePhaseKind::DiscoverySelect => "discovery_select",
            PendingChoicePhaseKind::ScrySelect => "scry_select",
            PendingChoicePhaseKind::CardRewardSelect => "card_reward_select",
            PendingChoicePhaseKind::ForeignInfluenceSelect => "foreign_influence_select",
            PendingChoicePhaseKind::ChooseOneSelect => "choose_one_select",
            PendingChoicePhaseKind::StanceChoice => "stance_choice",
        }
    }
}

fn pending_choice_phase_profile(engine: &EngineState) -> PendingChoicePhaseProfileV1 {
    let EngineState::PendingChoice(choice) = engine else {
        return PendingChoicePhaseProfileV1::default();
    };

    let kind = match choice {
        crate::state::core::PendingChoice::HandSelect { .. } => PendingChoicePhaseKind::HandSelect,
        crate::state::core::PendingChoice::GridSelect { .. } => PendingChoicePhaseKind::GridSelect,
        crate::state::core::PendingChoice::DiscoverySelect(_) => {
            PendingChoicePhaseKind::DiscoverySelect
        }
        crate::state::core::PendingChoice::ScrySelect { .. } => PendingChoicePhaseKind::ScrySelect,
        crate::state::core::PendingChoice::CardRewardSelect { .. } => {
            PendingChoicePhaseKind::CardRewardSelect
        }
        crate::state::core::PendingChoice::ForeignInfluenceSelect { .. } => {
            PendingChoicePhaseKind::ForeignInfluenceSelect
        }
        crate::state::core::PendingChoice::ChooseOneSelect { .. } => {
            PendingChoicePhaseKind::ChooseOneSelect
        }
        crate::state::core::PendingChoice::StanceChoice => PendingChoicePhaseKind::StanceChoice,
    };
    let fanout = pending_choice_fanout(choice);

    PendingChoicePhaseProfileV1 {
        present: true,
        kind: Some(kind),
        candidate_count: fanout.candidate_count,
        estimated_action_fanout: fanout.estimated_action_fanout,
        high_fanout: fanout.high_fanout,
    }
}

#[cfg(test)]
mod tests;
