use super::super::phase_profile::{combat_search_phase_profile, CombatSearchPhaseProfileV1};
use super::super::value_facts::living_enemy_count;
use crate::runtime::combat::CombatState;
use crate::state::core::EngineState;
use sts_combat_knowledge::{
    combat_tactical_choker_capacity_v1, combat_tactical_hand_value_v1,
    combat_tactical_next_draw_value_v1, combat_tactical_sustained_mitigation_score_v1,
    CombatTacticalCardPileValueV1, CombatTacticalChokerCapacityV1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2::value) struct CombatSearchCoreValueFactsV1 {
    pub(in crate::ai::combat_search_v2::value) living_enemy_count: usize,
    pub(in crate::ai::combat_search_v2::value) phase_profile: CombatSearchPhaseProfileV1,
    pub(in crate::ai::combat_search_v2::value) sustained_mitigation: i32,
    pub(in crate::ai::combat_search_v2::value) hand: CombatTacticalCardPileValueV1,
    pub(in crate::ai::combat_search_v2::value) choker_capacity: CombatTacticalChokerCapacityV1,
    pub(in crate::ai::combat_search_v2::value) next_draw: CombatTacticalCardPileValueV1,
}

pub(in crate::ai::combat_search_v2::value) fn combat_search_core_value_facts(
    engine: &EngineState,
    combat: &CombatState,
) -> CombatSearchCoreValueFactsV1 {
    CombatSearchCoreValueFactsV1 {
        living_enemy_count: living_enemy_count(combat),
        phase_profile: combat_search_phase_profile(engine, combat),
        sustained_mitigation: combat_tactical_sustained_mitigation_score_v1(combat),
        hand: combat_tactical_hand_value_v1(combat),
        choker_capacity: combat_tactical_choker_capacity_v1(combat),
        next_draw: combat_tactical_next_draw_value_v1(combat),
    }
}
