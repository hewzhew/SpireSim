use serde::{Deserialize, Serialize};

use crate::ai::route_window_facts::{
    build_route_window_facts, RouteWindowFacts, RouteWindowFactsConfig,
};
use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::runtime::combat::CombatState;
use crate::state::RunState;

pub const POTION_RUN_CONTINUATION_CONTEXT_SCHEMA_NAME: &str = "PotionRunContinuationContext";
pub const POTION_RUN_CONTINUATION_CONTEXT_SCHEMA_VERSION: u32 = 1;

const ROUTE_HORIZON_NODES: usize = 5;
const ROUTE_PATH_BUDGET: usize = 2_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRunInventoryContextV1 {
    pub slot_capacity: usize,
    pub occupied_slots: usize,
    pub empty_slots: usize,
    pub inventory_full: bool,
    pub new_potion_would_require_replacement_if_obtainable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRunSupplyContextV1 {
    pub potion_drop_chance_modifier: i32,
    pub ordinary_combat_drop_chance_percent: i32,
    pub relic_adjusted_drop_chance_percent: i32,
    pub white_beast_statue: bool,
    pub sozu_blocks_acquisition: bool,
    pub alchemize_copies: usize,
    pub entropic_brew_count: usize,
    pub reward_size_gate_can_suppress_drop: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PotionRunContinuationLimitationV1 {
    CurrentCombatRewardSizeGateUnknown,
    FuturePotionIdentityUnknownUntilRoll,
    UnknownRoomsDoNotRevealEncounterIdentity,
    FutureHandsAndDrawOrderNotSimulated,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRunContinuationContextV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub capture_boundary: String,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub visible_boss: Option<EncounterId>,
    pub inventory: PotionRunInventoryContextV1,
    pub supply: PotionRunSupplyContextV1,
    pub route_window: RouteWindowFacts,
    pub limitations: Vec<PotionRunContinuationLimitationV1>,
}

pub fn potion_run_continuation_context_v1(
    run_state: &RunState,
    combat: &CombatState,
) -> PotionRunContinuationContextV1 {
    let slot_capacity = combat.entities.potions.len();
    let occupied_slots = combat
        .entities
        .potions
        .iter()
        .filter(|slot| slot.is_some())
        .count();
    let empty_slots = slot_capacity.saturating_sub(occupied_slots);
    let inventory_full = empty_slots == 0;
    let has_relic = |id| run_state.relics.iter().any(|relic| relic.id == id);
    let ordinary_combat_drop_chance_percent = (40 + run_state.potion_drop_chance_mod).clamp(0, 100);
    let white_beast_statue = has_relic(RelicId::WhiteBeastStatue);
    let relic_adjusted_drop_chance_percent = if white_beast_statue {
        100
    } else {
        ordinary_combat_drop_chance_percent
    };

    PotionRunContinuationContextV1 {
        schema_name: POTION_RUN_CONTINUATION_CONTEXT_SCHEMA_NAME.to_string(),
        schema_version: POTION_RUN_CONTINUATION_CONTEXT_SCHEMA_VERSION,
        capture_boundary: "before_atomic_combat_search".to_string(),
        act: run_state.act_num,
        floor: run_state.floor_num,
        current_hp: combat.entities.player.current_hp,
        max_hp: combat.entities.player.max_hp,
        visible_boss: run_state.boss_key,
        inventory: PotionRunInventoryContextV1 {
            slot_capacity,
            occupied_slots,
            empty_slots,
            inventory_full,
            new_potion_would_require_replacement_if_obtainable: inventory_full,
        },
        supply: PotionRunSupplyContextV1 {
            potion_drop_chance_modifier: run_state.potion_drop_chance_mod,
            ordinary_combat_drop_chance_percent,
            relic_adjusted_drop_chance_percent,
            white_beast_statue,
            sozu_blocks_acquisition: has_relic(RelicId::Sozu),
            alchemize_copies: run_state
                .master_deck
                .iter()
                .filter(|card| card.id == CardId::Alchemize)
                .count(),
            entropic_brew_count: combat
                .entities
                .potions
                .iter()
                .flatten()
                .filter(|potion| potion.id == PotionId::EntropicBrew)
                .count(),
            reward_size_gate_can_suppress_drop: true,
        },
        route_window: build_route_window_facts(
            run_state,
            RouteWindowFactsConfig {
                horizon_nodes: ROUTE_HORIZON_NODES,
                path_budget: ROUTE_PATH_BUDGET,
            },
        ),
        limitations: vec![
            PotionRunContinuationLimitationV1::CurrentCombatRewardSizeGateUnknown,
            PotionRunContinuationLimitationV1::FuturePotionIdentityUnknownUntilRoll,
            PotionRunContinuationLimitationV1::UnknownRoomsDoNotRevealEncounterIdentity,
            PotionRunContinuationLimitationV1::FutureHandsAndDrawOrderNotSimulated,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::potions::Potion;
    use crate::content::relics::RelicState;

    #[test]
    fn context_preserves_route_supply_and_replacement_pressure_without_scoring() {
        let mut run_state = RunState::new(7, 0, false, "Silent");
        run_state.floor_num = 12;
        run_state.current_hp = 31;
        run_state.potion_drop_chance_mod = 20;
        run_state.boss_key = Some(EncounterId::TheGuardian);
        run_state.potions = vec![
            Some(Potion::new(PotionId::EntropicBrew, 10)),
            Some(Potion::new(PotionId::BlockPotion, 20)),
            Some(Potion::new(PotionId::WeakenPotion, 30)),
        ];
        run_state
            .relics
            .push(RelicState::new(RelicId::WhiteBeastStatue));
        run_state.relics.push(RelicState::new(RelicId::Sozu));
        run_state.add_card_to_deck(CardId::Alchemize);
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.player.current_hp = 31;
        combat.entities.potions = run_state.potions.clone();

        let context = potion_run_continuation_context_v1(&run_state, &combat);

        assert_eq!(context.capture_boundary, "before_atomic_combat_search");
        assert_eq!(context.current_hp, 31);
        assert_eq!(context.visible_boss, Some(EncounterId::TheGuardian));
        assert_eq!(context.inventory.occupied_slots, 3);
        assert!(context.inventory.inventory_full);
        assert_eq!(context.supply.potion_drop_chance_modifier, 20);
        assert_eq!(context.supply.ordinary_combat_drop_chance_percent, 60);
        assert_eq!(context.supply.relic_adjusted_drop_chance_percent, 100);
        assert!(context.supply.sozu_blocks_acquisition);
        assert_eq!(context.supply.alchemize_copies, 1);
        assert_eq!(context.supply.entropic_brew_count, 1);
        assert_eq!(context.route_window.coverage.horizon_nodes, 5);

        let payload =
            serde_json::to_value(&context).expect("serialize potion continuation context");
        assert_eq!(
            payload["schema_name"],
            POTION_RUN_CONTINUATION_CONTEXT_SCHEMA_NAME
        );
        let restored: PotionRunContinuationContextV1 =
            serde_json::from_value(payload).expect("deserialize potion continuation context");
        assert_eq!(restored, context);
    }
}
