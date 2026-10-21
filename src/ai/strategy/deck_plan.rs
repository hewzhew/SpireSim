use crate::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CardBurden, CombatEvent, Mechanic, TriggeredEffect,
};
use crate::ai::block_plan_profile_v1::{
    block_plan_profile_from_deck_v1, block_plan_profile_v1, has_second_wind_block_engine_fuel_v1,
    BlockPlanReadinessV1,
};
use crate::ai::strategy::deck_admission::{
    assess_deck_admission_from_inventory, DeckAdmission, DeckAdmissionContext,
};
use crate::ai::strategy::deck_construction_pressure::{
    assess_deck_construction_pressure, reward_construction_lane_adjustment,
    ConstructionLaneAdjustment, DeckConstructionContext, DeckConstructionPressure,
};
use crate::ai::strategy::deck_role_inventory::{
    card_is_stable_strength_source, card_is_strong_aoe, DeckRoleInventory,
};
use crate::ai::strategy::deck_strategic_deficit::{
    assess_deck_strategic_deficit_summary, DeckStrategicDeficitSummary, StrategicDeficitLevel,
};
use crate::ai::strategy::reward_admission::{RewardAdmission, RewardAdmissionReason};
use crate::ai::strategy::run_strategic_facts::RunStrategicFacts;
use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;
use crate::runtime::combat::CombatCard;
use crate::state::run::RunState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeckPlanSnapshot {
    pub context: DeckAdmissionContext,
    pub boss_key: Option<EncounterId>,
    pub deck_size: usize,
    pub roles: DeckRoleInventory,
    pub block_plan_readiness: BlockPlanReadinessV1,
    pub construction: DeckConstructionPressure,
    pub strategic_deficit: DeckStrategicDeficitSummary,
    pub run_facts: RunStrategicFacts,
}

impl DeckPlanSnapshot {
    pub fn from_run_state(run_state: &RunState) -> Self {
        let mut snapshot = Self::from_deck(
            &run_state.master_deck,
            DeckAdmissionContext {
                act: run_state.act_num,
                current_hp: run_state.current_hp,
                max_hp: run_state.max_hp,
            },
            RunStrategicFacts::from_run_state(run_state),
        )
        .with_boss_key(run_state.boss_key);
        snapshot.block_plan_readiness = block_plan_profile_v1(run_state).readiness;
        snapshot
    }

    pub fn from_deck(
        deck: &[CombatCard],
        context: DeckAdmissionContext,
        run_facts: RunStrategicFacts,
    ) -> Self {
        Self {
            context,
            boss_key: None,
            deck_size: deck.len(),
            roles: DeckRoleInventory::from_deck(deck),
            block_plan_readiness: block_plan_profile_from_deck_v1(deck).readiness,
            construction: assess_deck_construction_pressure(
                deck,
                DeckConstructionContext { act: context.act },
            ),
            strategic_deficit: assess_deck_strategic_deficit_summary(deck, run_facts),
            run_facts,
        }
    }

    pub fn with_boss_key(mut self, boss_key: Option<EncounterId>) -> Self {
        self.boss_key = boss_key;
        self
    }

    pub fn survival_pressure(self) -> bool {
        self.context.survival_pressure()
    }

    pub fn deck_admission(self, admission: &RewardAdmission) -> DeckAdmission {
        assess_deck_admission_from_inventory(self.deck_size, self.context, &self.roles, admission)
    }

    pub fn reward_lane_adjustment(self, admission: &RewardAdmission) -> ConstructionLaneAdjustment {
        reward_construction_lane_adjustment(self.construction, admission)
    }

    pub fn repairs_strength_package_reliability(self, candidate: Option<(CardId, u8)>) -> bool {
        self.roles.strength_multiplier_units > 0
            && self.roles.strength_source_units == 1
            && matches!(
                self.strategic_deficit.boss_scaling_plan,
                StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
            )
            && candidate.is_some_and(|(card, upgrades)| {
                card_is_stable_strength_source(
                    card,
                    upgrades,
                    self.roles.repeatable_self_damage_supply,
                )
            })
    }

    pub fn has_open_stable_strength_payoff_slot(self) -> bool {
        self.roles.strength_source_units > 0 && self.roles.strength_payoff_units == 0
    }

    pub fn has_supported_hand_exhaust_conversion(self, admission: &RewardAdmission) -> bool {
        self.roles.exhaust_payoff_units > 0
            && admission
                .reasons
                .contains(&RewardAdmissionReason::Emits(CombatEvent::CardExhausted))
            && admission
                .reasons
                .contains(&RewardAdmissionReason::Burden(CardBurden::ExhaustsHand))
    }

    /// The deck already emits exhaust events and this candidate supplies a
    /// triggered payoff axis not yet represented by the deck. A second handler
    /// for an existing axis remains marginally unresolved instead of being
    /// promoted merely because stacking is legal.
    pub fn has_open_supported_exhaust_payoff_axis(self, card: CardId, upgrades: u8) -> bool {
        if self.roles.broad_exhaust_units == 0
            && self.roles.corruption_units == 0
            && self.roles.exhaust_stream_units < 2
        {
            return false;
        }
        card_definition_with_upgrades(card, upgrades)
            .event_handlers
            .iter()
            .any(|handler| {
                handler.on == CombatEvent::CardExhausted
                    && match handler.effect {
                        TriggeredEffect::Provide(Mechanic::Block) => {
                            self.roles.exhaust_block_payoff_units == 0
                        }
                        TriggeredEffect::Provide(Mechanic::CardDraw) => {
                            self.roles.exhaust_draw_payoff_units == 0
                        }
                        _ => false,
                    }
            })
    }

    pub fn candidate_establishes_fuel_backed_second_wind_plan(
        self,
        candidate: Option<(CardId, u8)>,
    ) -> bool {
        self.roles.second_wind_units == 0
            && has_second_wind_block_engine_fuel_v1(self.roles.non_attack_units)
            && candidate.is_some_and(|(card, _)| card == CardId::SecondWind)
    }

    pub fn candidate_improves_aoe_gap(
        self,
        candidate: Option<(CardId, u8)>,
        admission: &RewardAdmission,
    ) -> bool {
        if !admission
            .reasons
            .contains(&RewardAdmissionReason::AreaDamage)
        {
            return false;
        }
        match self.strategic_deficit.aoe_or_minion_control {
            StrategicDeficitLevel::Missing => true,
            StrategicDeficitLevel::Thin => {
                let Some((card, upgrades)) = candidate else {
                    return false;
                };
                if self.roles.strong_aoe_units == 0 {
                    card_is_strong_aoe(card, upgrades)
                } else {
                    self.roles.aoe_units < 2
                }
            }
            StrategicDeficitLevel::Adequate | StrategicDeficitLevel::Surplus => false,
        }
    }
}
