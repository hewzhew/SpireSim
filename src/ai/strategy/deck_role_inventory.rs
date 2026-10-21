use crate::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CardBurden, CardDefinition, CombatEvent, DamageScalingAxis,
    DeckMechanicContext, InstalledRule, Mechanic, PlayEffect, TriggeredEffect,
};
use crate::ai::card_analysis_v1::{card_analysis_profile_v1, CardAnalysisAoeSupportV1};
use crate::ai::card_semantics_v1::card_reward_facts_v1;
use crate::ai::card_tactical_delivery_v1::card_tactical_delivery_profile_v1;
use crate::ai::deck_shape_v1::is_status_digest_card;
use crate::content::cards::{get_card_definition, CardId};
use crate::runtime::combat::CombatCard;
use crate::state::rewards::RewardCard;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeckRoleInventory {
    pub frontload_units: u8,
    pub aoe_units: u8,
    pub strong_aoe_units: u8,
    pub block_units: u8,
    /// Known non-flat mechanisms are retained without filling the ordinary
    /// Block or cycle-Block slots before their conditions are evaluated.
    pub conditional_or_variable_block_units: u8,
    pub high_quality_block_units: u8,
    pub cycle_block_units: u8,
    pub mitigation_units: u8,
    pub weak_units: u8,
    pub persistent_enemy_strength_down_units: u8,
    pub temporary_enemy_strength_down_units: u8,
    pub debuff_units: u8,
    pub vulnerable_units: u8,
    pub draw_units: u8,
    pub energy_units: u8,
    pub x_cost_payoff_units: u8,
    pub strength_source_units: u8,
    pub conditional_strength_source_units: u8,
    pub repeatable_self_damage_supply: bool,
    pub strength_multiplier_units: u8,
    pub corruption_units: u8,
    pub exhaust_stream_units: u8,
    pub broad_exhaust_units: u8,
    pub second_wind_units: u8,
    pub non_attack_units: u8,
    pub exhaust_payoff_units: u8,
    pub exhaust_block_payoff_units: u8,
    pub exhaust_draw_payoff_units: u8,
    pub block_payoff_units: u8,
    pub strength_payoff_units: u8,
    pub upgrade_access_units: u8,
    pub status_digest_units: u8,
}

impl DeckRoleInventory {
    pub fn from_deck(deck: &[CombatCard]) -> Self {
        let mut inventory = Self::default();
        let definitions = deck
            .iter()
            .map(|card| card_definition_with_upgrades(card.id, card.upgrades))
            .collect::<Vec<_>>();
        let context = DeckMechanicContext::from_definitions(&definitions);
        inventory.repeatable_self_damage_supply = context
            .repeatable_event_streams
            .contains(&CombatEvent::CardSelfDamage);

        for (card, definition) in deck.iter().zip(&definitions) {
            let analysis = card_analysis_profile_v1(card.id, card.upgrades);
            let facts = card_reward_facts_v1(&RewardCard::new(card.id, card.upgrades));
            let tactical = card_tactical_delivery_profile_v1(card.id, card.upgrades);
            if is_status_digest_card(card.id) {
                inventory.status_digest_units += 1;
            }
            if analysis.is_block_plan_high_quality_chunk {
                inventory.high_quality_block_units += 1;
            }
            if analysis.is_second_wind_source {
                inventory.second_wind_units += 1;
            }
            if analysis.is_block_plan_broad_exhaust_source {
                inventory.broad_exhaust_units += 1;
            }
            if analysis.is_non_attack {
                inventory.non_attack_units += 1;
            }
            if analysis.aoe_support == CardAnalysisAoeSupportV1::Strong
                || card.id == CardId::Combust
            {
                inventory.strong_aoe_units += 1;
            }
            if get_card_definition(card.id).cost == -1 {
                inventory.x_cost_payoff_units += 1;
            }
            let context_dependent_block = facts.context_dependent_block_delivery_v1().is_some();
            if context_dependent_block {
                inventory.conditional_or_variable_block_units += 1;
            }
            let explicitly_provides_block = !context_dependent_block
                && definition
                    .play_effects
                    .contains(&PlayEffect::Provide(Mechanic::Block));
            let factually_provides_nonstarter_block = facts
                .unconditional_base_block_request_v1()
                .is_some_and(|block| block > 0)
                && card.id != CardId::Defend;
            let provides_block = explicitly_provides_block || factually_provides_nonstarter_block;
            let explicitly_provides_draw = definition
                .play_effects
                .contains(&PlayEffect::Provide(Mechanic::CardDraw));
            let provides_draw = explicitly_provides_draw || facts.draw_cards > 0;
            if provides_block && provides_draw {
                inventory.cycle_block_units += 1;
            }
            if factually_provides_nonstarter_block && !explicitly_provides_block {
                inventory.add_mechanic(Mechanic::Block);
            }
            if facts.draw_cards > 0 && !explicitly_provides_draw {
                inventory.add_mechanic(Mechanic::CardDraw);
            }
            let explicitly_provides_energy = definition
                .play_effects
                .contains(&PlayEffect::Provide(Mechanic::Energy));
            if facts.energy_gain > 0 && !explicitly_provides_energy {
                inventory.add_mechanic(Mechanic::Energy);
            }
            if tactical.direct_damage_dense
                && !facts.is_random_output
                && !facts.has_conditional_playability
                && !definition
                    .play_effects
                    .contains(&PlayEffect::FrontloadDamage)
            {
                inventory.add_play_effect(PlayEffect::FrontloadDamage);
            }
            if facts.is_aoe
                && facts
                    .damage
                    .unconditional_base_total()
                    .is_some_and(|amount| amount > 0)
                && !definition.play_effects.contains(&PlayEffect::AreaDamage)
            {
                inventory.add_play_effect(PlayEffect::AreaDamage);
            }
            let conditional_strength_provider =
                definition_is_conditional_strength_source(definition);
            if conditional_strength_provider {
                inventory.conditional_strength_source_units += 1;
            }
            for effect in &definition.play_effects {
                if context_dependent_block && *effect == PlayEffect::Provide(Mechanic::Block) {
                    continue;
                }
                if conditional_strength_provider
                    && *effect == PlayEffect::Provide(Mechanic::Strength)
                {
                    continue;
                }
                inventory.add_play_effect(*effect);
            }
            for rule in &definition.installed_rules {
                inventory.add_installed_rule(*rule);
            }
            for handler in &definition.event_handlers {
                inventory.add_event_handler(handler.on, handler.effect);
            }
        }
        inventory
    }

    fn add_play_effect(&mut self, effect: PlayEffect) {
        match effect {
            PlayEffect::FrontloadDamage => self.frontload_units += 1,
            PlayEffect::AreaDamage => self.aoe_units += 1,
            PlayEffect::DamageUses(Mechanic::Block) => self.block_payoff_units += 1,
            PlayEffect::DamageUses(Mechanic::Strength)
            | PlayEffect::DamageScalesWith(DamageScalingAxis::PerHitStrength) => {
                self.strength_payoff_units += 1;
            }
            PlayEffect::Provide(mechanic) => self.add_mechanic(mechanic),
            PlayEffect::EmitEvent(CombatEvent::CardExhausted)
            | PlayEffect::PlayTopCardAndExhaust => self.exhaust_stream_units += 1,
            PlayEffect::CombatUpgradeSingle | PlayEffect::CombatUpgradeAll => {
                self.upgrade_access_units += 1;
            }
            PlayEffect::DamageUses(_)
            | PlayEffect::DamageScalesWith(_)
            | PlayEffect::EmitEvent(_)
            | PlayEffect::ExhaustsSelf
            | PlayEffect::RunReward(_)
            | PlayEffect::RecoverCurrentHp
            | PlayEffect::CostReducedByHpLossThisCombat
            | PlayEffect::AddCombatDeckClutter => {}
        }
    }

    fn add_event_handler(&mut self, event: CombatEvent, effect: TriggeredEffect) {
        if event == CombatEvent::CardExhausted {
            match effect {
                TriggeredEffect::Provide(Mechanic::Block) => {
                    self.exhaust_payoff_units += 1;
                    self.exhaust_block_payoff_units += 1;
                }
                TriggeredEffect::Provide(Mechanic::CardDraw) => {
                    self.exhaust_payoff_units += 1;
                    self.exhaust_draw_payoff_units += 1;
                }
                _ => {}
            }
        }
        if effect != TriggeredEffect::Provide(Mechanic::Strength) {
            return;
        }
        if event == CombatEvent::CardSelfDamage && !self.repeatable_self_damage_supply {
            self.conditional_strength_source_units += 1;
            return;
        }
        self.strength_source_units += match event {
            CombatEvent::TurnStart => 2,
            CombatEvent::CardSelfDamage => 1,
            _ => 1,
        };
    }

    fn add_installed_rule(&mut self, rule: InstalledRule) {
        match rule {
            InstalledRule::SkillCardsCostZeroAndExhaust => self.corruption_units += 1,
        }
    }

    fn add_mechanic(&mut self, mechanic: Mechanic) {
        match mechanic {
            Mechanic::Block => self.block_units += 1,
            Mechanic::Weak => {
                self.weak_units += 1;
                self.mitigation_units += 1;
                self.debuff_units += 1;
            }
            Mechanic::EnemyStrengthDown => {
                self.persistent_enemy_strength_down_units += 1;
                self.mitigation_units += 1;
                self.debuff_units += 1;
            }
            Mechanic::TemporaryEnemyStrengthDown => {
                self.temporary_enemy_strength_down_units += 1;
                self.mitigation_units += 1;
                self.debuff_units += 1;
            }
            Mechanic::Vulnerable => {
                self.debuff_units += 1;
                self.vulnerable_units += 1;
            }
            Mechanic::CardDraw => self.draw_units += 1,
            Mechanic::Energy => self.energy_units += 1,
            Mechanic::Strength => self.strength_source_units += 1,
            Mechanic::StrengthMultiplier => self.strength_multiplier_units += 1,
            Mechanic::TemporaryStrength | Mechanic::TopdeckControl => {}
        }
    }
}

pub(super) fn card_is_strong_aoe(card: CardId, upgrades: u8) -> bool {
    card == CardId::Combust
        || card_analysis_profile_v1(card, upgrades).aoe_support == CardAnalysisAoeSupportV1::Strong
}

pub(super) fn card_is_stable_strength_source(
    card: CardId,
    upgrades: u8,
    repeatable_self_damage_supply: bool,
) -> bool {
    let definition = card_definition_with_upgrades(card, upgrades);
    (definition
        .play_effects
        .contains(&PlayEffect::Provide(Mechanic::Strength))
        && !definition_is_conditional_strength_source(&definition))
        || definition.event_handlers.iter().any(|handler| {
            handler.effect == TriggeredEffect::Provide(Mechanic::Strength)
                && (handler.on != CombatEvent::CardSelfDamage || repeatable_self_damage_supply)
        })
}

fn definition_is_conditional_strength_source(definition: &CardDefinition) -> bool {
    definition
        .play_effects
        .contains(&PlayEffect::Provide(Mechanic::Strength))
        && definition
            .burdens
            .contains(&CardBurden::RequiresEnemyAttackIntent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditional_block_does_not_fill_plain_or_cycle_block_slots() {
        let inventory = DeckRoleInventory::from_deck(&[
            card(CardId::Defend, 1),
            card(CardId::EscapePlan, 2),
            card(CardId::AutoShields, 3),
            card(CardId::SecondWind, 4),
            card(CardId::ShrugItOff, 5),
        ]);
        assert_eq!(inventory.block_units, 1);
        assert_eq!(inventory.cycle_block_units, 1);
        assert_eq!(inventory.conditional_or_variable_block_units, 3);
        assert_eq!(inventory.draw_units, 2);
        assert_eq!(inventory.second_wind_units, 1);
    }

    fn card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    #[test]
    fn role_inventory_separates_weak_persistent_and_temporary_mitigation() {
        let inventory = DeckRoleInventory::from_deck(&[
            card(CardId::Clothesline, 1),
            card(CardId::Disarm, 2),
            card(CardId::DarkShackles, 3),
        ]);

        assert_eq!(inventory.weak_units, 1);
        assert_eq!(inventory.persistent_enemy_strength_down_units, 1);
        assert_eq!(inventory.temporary_enemy_strength_down_units, 1);
        assert_eq!(inventory.mitigation_units, 3);
        assert_eq!(inventory.debuff_units, 3);
    }

    #[test]
    fn spot_weakness_counts_as_conditional_strength_not_stable_strength() {
        let inventory = DeckRoleInventory::from_deck(&[
            card(CardId::SpotWeakness, 1),
            card(CardId::SpotWeakness, 2),
        ]);

        assert_eq!(inventory.strength_source_units, 0);
        assert_eq!(inventory.conditional_strength_source_units, 2);
    }

    #[test]
    fn inflame_and_demon_form_count_as_stable_strength_sources() {
        let inventory =
            DeckRoleInventory::from_deck(&[card(CardId::Inflame, 1), card(CardId::DemonForm, 2)]);

        assert_eq!(inventory.strength_source_units, 3);
        assert_eq!(inventory.conditional_strength_source_units, 0);
    }

    #[test]
    fn limit_break_counts_as_multiplier_without_becoming_a_strength_source() {
        let inventory =
            DeckRoleInventory::from_deck(&[card(CardId::Inflame, 1), card(CardId::LimitBreak, 2)]);

        assert_eq!(inventory.strength_source_units, 1);
        assert_eq!(inventory.strength_multiplier_units, 1);
    }

    #[test]
    fn offering_backed_rupture_is_conditional_not_stable_strength() {
        let inventory =
            DeckRoleInventory::from_deck(&[card(CardId::Offering, 1), card(CardId::Rupture, 2)]);

        assert!(!inventory.repeatable_self_damage_supply);
        assert_eq!(inventory.strength_source_units, 0);
        assert_eq!(inventory.conditional_strength_source_units, 1);
    }

    #[test]
    fn repeatable_self_damage_makes_rupture_stable_strength() {
        let inventory = DeckRoleInventory::from_deck(&[
            card(CardId::Bloodletting, 1),
            card(CardId::Rupture, 2),
        ]);

        assert!(inventory.repeatable_self_damage_supply);
        assert_eq!(inventory.strength_source_units, 1);
    }

    #[test]
    fn role_inventory_distinguishes_weak_and_strong_aoe() {
        let inventory = DeckRoleInventory::from_deck(&[
            card(CardId::Cleave, 1),
            card(CardId::Cleave, 2),
            card(CardId::Whirlwind, 3),
        ]);

        assert_eq!(inventory.aoe_units, 3);
        assert_eq!(inventory.strong_aoe_units, 1);
    }

    #[test]
    fn role_inventory_separates_plain_from_high_quality_block_chunks() {
        let inventory = DeckRoleInventory::from_deck(&[
            card(CardId::Defend, 1),
            card(CardId::ShrugItOff, 2),
            card(CardId::PowerThrough, 3),
            card(CardId::Impervious, 4),
        ]);

        assert_eq!(inventory.block_units, 3);
        assert_eq!(inventory.high_quality_block_units, 2);
    }

    #[test]
    fn role_inventory_records_second_wind_and_its_non_attack_fuel() {
        let inventory = DeckRoleInventory::from_deck(&[
            card(CardId::Strike, 1),
            card(CardId::Defend, 2),
            card(CardId::PowerThrough, 3),
            card(CardId::Inflame, 4),
            card(CardId::SecondWind, 5),
        ]);

        assert_eq!(inventory.second_wind_units, 1);
        assert_eq!(inventory.broad_exhaust_units, 1);
        assert_eq!(inventory.non_attack_units, 4);
    }

    #[test]
    fn one_card_exhaust_access_is_not_mass_exhaust_throughput() {
        let inventory = DeckRoleInventory::from_deck(&[
            card(CardId::BurningPact, 1),
            card(CardId::TrueGrit, 2),
            card(CardId::Havoc, 3),
        ]);

        assert!(inventory.exhaust_stream_units >= 3);
        assert_eq!(inventory.broad_exhaust_units, 0);
    }

    #[test]
    fn exhaust_payoff_inventory_preserves_triggered_output_axes() {
        let inventory = DeckRoleInventory::from_deck(&[
            card(CardId::FeelNoPain, 1),
            card(CardId::FeelNoPain, 2),
            card(CardId::DarkEmbrace, 3),
        ]);

        assert_eq!(inventory.exhaust_payoff_units, 3);
        assert_eq!(inventory.exhaust_block_payoff_units, 2);
        assert_eq!(inventory.exhaust_draw_payoff_units, 1);
    }
}
