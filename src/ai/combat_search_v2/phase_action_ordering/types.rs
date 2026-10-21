use super::super::enemy_phase_transition::{
    AwakenedOneStrengthTransitionOpportunity, EnemyPhaseTransitionHint,
};
use crate::content::cards::CardType;
use crate::content::monsters::EnemyId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ai::combat_search_v2) struct PhaseActionOrderingFacts {
    pub(in crate::ai::combat_search_v2) card_type: CardType,
    pub(in crate::ai::combat_search_v2) block: i32,
    pub(in crate::ai::combat_search_v2) mitigation: i32,
    pub(in crate::ai::combat_search_v2) target_progress: i32,
    pub(in crate::ai::combat_search_v2) target_lethal: bool,
    pub(in crate::ai::combat_search_v2) future_debuff: bool,
    pub(in crate::ai::combat_search_v2) access: PhaseActionAccessFacts,
    pub(in crate::ai::combat_search_v2) target_enemy_id: Option<EnemyId>,
    pub(in crate::ai::combat_search_v2) target_has_stasis_card: bool,
    pub(in crate::ai::combat_search_v2) phase_transition: EnemyPhaseTransitionHint,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct PhaseActionAccessFacts {
    pub(in crate::ai::combat_search_v2) declared_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) conditional_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) total_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) bad_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) forced_turn_end: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct PhaseActionOrderingHint {
    pub(in crate::ai::combat_search_v2) role_rank_adjustment: i32,
    pub(in crate::ai::combat_search_v2) phase_setup: i32,
    pub(in crate::ai::combat_search_v2) phase_survival: i32,
    pub(in crate::ai::combat_search_v2) phase_transition_safety: i32,
    pub(in crate::ai::combat_search_v2) awakened_one_strength_transition_setup:
        Option<AwakenedOneStrengthTransitionOpportunity>,
}

impl PhaseActionOrderingHint {
    pub(in crate::ai::combat_search_v2) fn has_signal(self) -> bool {
        self.role_rank_adjustment != 0
            || self.phase_setup != 0
            || self.phase_survival != 0
            || self.phase_transition_safety != 0
            || self.awakened_one_strength_transition_setup.is_some()
    }
}

impl PhaseActionAccessFacts {
    pub(in crate::ai::combat_search_v2) fn draw_cards(self) -> i32 {
        self.total_draw_cards.max(
            self.declared_draw_cards
                .saturating_add(self.conditional_draw_cards),
        )
    }

    pub(in crate::ai::combat_search_v2) fn time_warp_access_risk(self) -> i32 {
        self.bad_draw_cards
            .max(0)
            .saturating_add(i32::from(self.forced_turn_end))
    }
}
