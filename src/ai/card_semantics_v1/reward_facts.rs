use crate::ai::analysis::card_semantics::{
    card_definition as strategic_card_definition, DamageScalingAxis, Mechanic, PayoffRequirement,
    PlayEffect,
};
use crate::ai::combat_upgrade_coverage_v1::{
    combat_upgrade_persistence_for_card_v1, combat_upgrade_scope_for_card_v1,
};
use crate::content::cards::mechanics::{
    card_base_block_delivery_v1, card_base_damage_for_upgrades_v1,
    card_copy_to_discard_after_damage_v1, card_draw_pile_skill_to_hand_v1,
    card_end_of_turn_block_each_turn_v1, card_end_of_turn_hp_loss_and_damage_v1,
    card_enemy_debuffs_in_monster_order_v1, card_enemy_strength_restoration_v1,
    card_energy_gain_v1, card_exhaust_block_power_v1, card_exhaust_draw_power_v1,
    card_exhausts_when_played, card_hand_topdeck_after_draw_v1, card_hp_loss_strength_power_v1,
    card_is_ethereal, card_mechanic_profile, card_player_strength_delivery_v1,
    card_self_hp_loss_on_play_v1, effective_card_target, unconditional_card_draw_request_v1,
    CardEnergyDeliveryV1, CardMechanicRole,
};
use crate::content::cards::{
    can_upgrade_card_once_for, get_card_definition, is_innate_card_for, CardId, CardTarget,
};
use crate::state::rewards::RewardCard;

use super::{
    card_mechanics_profile_v1, CardAcquisitionMechanicsFactsV1, CardPlayTargetV1,
    CardRewardDamageFactsV1, CardRewardFactsV1, CardRewardPickDependencyV1,
    CardRewardStatusDestinationV1, CardRewardStatusInjectionV1, CardRewardStatusPersistenceV1,
};

pub fn card_acquisition_mechanics_facts_v1(card: &RewardCard) -> CardAcquisitionMechanicsFactsV1 {
    CardAcquisitionMechanicsFactsV1 {
        current: card_reward_facts_v1(card),
        upgraded: card
            .upgrades
            .checked_add(1)
            .filter(|_| can_upgrade_card_once_for(card.id, card.upgrades))
            .map(|upgrades| card_reward_facts_v1(&RewardCard::new(card.id, upgrades))),
    }
}

pub fn card_reward_facts_v1(card: &RewardCard) -> CardRewardFactsV1 {
    let def = get_card_definition(card.id);
    let upgrades = i32::from(card.upgrades > 0);
    let mechanic_profile = card_mechanic_profile(card.id, card.upgrades);
    let damage_per_hit = card_base_damage_for_upgrades_v1(&def, card.upgrades).max(0);
    let damage = CardRewardDamageFactsV1::from_base_value(card.id, card.upgrades, damage_per_hit);
    let delivery = damage.delivery;
    let magic = (def.base_magic + def.upgrade_magic * upgrades).max(0);
    let energy_gain_rule = card_energy_gain_v1(card.id, card.upgrades, magic);
    let energy_at = |delivery| {
        energy_gain_rule
            .filter(|rule| rule.delivery == delivery)
            .map_or(0, |rule| rule.amount)
    };
    let status_injections = status_injections(card.id);

    let intangible_gain = intangible_gain(card.id, magic);
    let mechanics = card_mechanics_profile_v1(card.id);
    let temporary_strength_gain = if mechanics.temporary_strength_burst {
        strength_gain(card.id, magic)
    } else {
        0
    };

    CardRewardFactsV1 {
        card: card.id,
        upgrades: card.upgrades,
        name: def.name.to_string(),
        card_type: def.card_type,
        tags: Some(def.tags.to_vec()),
        rarity: def.rarity,
        cost: crate::content::cards::upgraded_base_cost_override_for(card.id, card.upgrades)
            .unwrap_or(def.cost),
        play_target: card_play_target_v1(effective_card_target(card.id, card.upgrades)),
        damage,
        block: (def.base_block + def.upgrade_block * upgrades).max(0),
        base_block_delivery: card_base_block_delivery_v1(card.id),
        end_of_turn_block_each_turn: card_end_of_turn_block_each_turn_v1(card.id, card.upgrades),
        draw_cards: unconditional_card_draw_request_v1(card.id, magic).unwrap_or(0) as i32,
        exhaust_draw_power: card_exhaust_draw_power_v1(card.id, magic),
        exhaust_block_power: card_exhaust_block_power_v1(card.id, magic),
        hand_topdeck_after_draw: card_hand_topdeck_after_draw_v1(card.id),
        status_creation_on_play: crate::content::cards::mechanics::card_status_creation_on_play_v1(
            card.id,
        ),
        damage_growth_on_play: crate::content::cards::mechanics::card_damage_growth_on_play_v1(
            card.id, magic,
        ),
        cost_change_on_hp_loss: crate::content::cards::mechanics::card_cost_change_on_hp_loss_v1(
            card.id,
        ),
        target_vulnerable_follow_up:
            crate::content::cards::mechanics::card_target_vulnerable_follow_up_v1(card.id),
        hand_copy: crate::content::cards::mechanics::card_hand_copy_v1(card.id, magic),
        exhaust_pile_to_hand: crate::content::cards::mechanics::card_exhaust_pile_to_hand_v1(
            card.id,
        ),
        top_deck_play: crate::content::cards::mechanics::card_top_deck_play_v1(card.id),
        enemy_debuffs_in_monster_order: card_enemy_debuffs_in_monster_order_v1(card.id, magic),
        hp_loss_strength_power: card_hp_loss_strength_power_v1(card.id, magic),
        draw_pile_skill_to_hand: card_draw_pile_skill_to_hand_v1(card.id),
        copy_to_discard_after_damage: card_copy_to_discard_after_damage_v1(card.id),
        random_attack_to_hand: crate::content::cards::mechanics::card_random_attack_to_hand_v1(
            card.id,
        ),
        end_of_turn_hp_loss_and_damage: card_end_of_turn_hp_loss_and_damage_v1(card.id, magic),
        draw_cards_each_turn: draw_cards_each_turn(card.id),
        discard_cards_each_turn: discard_cards_each_turn(card.id),
        hp_loss_each_turn: hp_loss_each_turn(card.id, magic),
        self_hp_loss_on_play: card_self_hp_loss_on_play_v1(card.id, card.upgrades),
        energy_gain: energy_at(CardEnergyDeliveryV1::OnPlay),
        energy_gain_each_turn: energy_at(CardEnergyDeliveryV1::EachPlayerTurnStart),
        energy_gain_on_exhaust: energy_at(CardEnergyDeliveryV1::OnExhaust),
        energy_gain_rule,
        vulnerable: vulnerable(card.id, magic),
        player_vulnerable: player_vulnerable(card.id, magic),
        weak: weak(card.id, magic),
        strength_gain: strength_gain(card.id, magic),
        strength_gain_delivery: card_player_strength_delivery_v1(card.id),
        temporary_strength_gain,
        strength_loss_at_end_of_turn: temporary_strength_gain,
        enemy_strength_down: if card_enemy_strength_restoration_v1(card.id).is_some() {
            magic
        } else {
            0
        },
        enemy_strength_restoration: card_enemy_strength_restoration_v1(card.id),
        max_hp_gain_on_fatal: max_hp_gain_on_fatal(card.id, magic),
        current_hp_gain_on_fatal: max_hp_gain_on_fatal(card.id, magic),
        intangible_gain,
        intangible_damage_cap: i32::from(intangible_gain > 0),
        intangible_caps_direct_hp_loss_to_one: intangible_gain > 0,
        exhausts: card_exhausts_when_played(card.id, card.upgrades),
        ethereal: card_is_ethereal(card.id, card.upgrades),
        innate: is_innate_card_for(card.id, card.upgrades),
        exhausts_other_cards: exhausts_other_cards(card.id),
        selects_from_hand: mechanic_profile
            .roles
            .contains(CardMechanicRole::SelectFromHand),
        adds_status_cards: status_injections
            .iter()
            .map(|injection| injection.count)
            .sum(),
        status_injections,
        upgrades_cards: upgrades_cards(card.id),
        combat_upgrade_scope: combat_upgrade_scope_for_card_v1(card.id, card.upgrades),
        combat_upgrade_persistence: combat_upgrade_persistence_for_card_v1(card.id, card.upgrades),
        is_random_output: is_random_output(card.id)
            || delivery.and_then(|rule| rule.targets())
                == Some(crate::content::cards::mechanics::CardAttackTargetsV1::RandomEnemyPerHit)
            || mechanic_profile
                .roles
                .contains(CardMechanicRole::RandomOutcome)
            || mechanic_profile
                .roles
                .contains(CardMechanicRole::RandomTarget),
        has_conditional_playability: has_conditional_playability(card.id)
            || matches!(
                delivery,
                Some(
                    crate::content::cards::mechanics::CardBaseDamageDeliveryV1::Attack {
                        play_condition: Some(_),
                        ..
                    }
                )
            ),
        // Targetless random hits and all-enemy debuffs are not AoE damage.
        // The reviewed request scope owns this fact, separately from totals.
        is_aoe: delivery.and_then(|rule| rule.targets())
            == Some(crate::content::cards::mechanics::CardAttackTargetsV1::EachEnemy),
        pick_dependencies: pick_dependencies(card.id),
    }
}

fn card_play_target_v1(target: CardTarget) -> CardPlayTargetV1 {
    match target {
        CardTarget::Enemy => CardPlayTargetV1::OneEnemy,
        CardTarget::AllEnemy => CardPlayTargetV1::AllEnemies,
        CardTarget::All => CardPlayTargetV1::AllCharacters,
        CardTarget::SelfAndEnemy => CardPlayTargetV1::PlayerAndOneEnemy,
        CardTarget::SelfTarget => CardPlayTargetV1::Player,
        CardTarget::None => CardPlayTargetV1::NoExplicitTarget,
    }
}

fn draw_cards_each_turn(card_id: CardId) -> i32 {
    match card_id {
        CardId::Brutality | CardId::MachineLearning | CardId::ToolsOfTheTrade => 1,
        _ => 0,
    }
}

fn discard_cards_each_turn(card_id: CardId) -> i32 {
    match card_id {
        CardId::ToolsOfTheTrade => 1,
        _ => 0,
    }
}

fn hp_loss_each_turn(card_id: CardId, magic: i32) -> i32 {
    match card_id {
        CardId::Brutality => 1,
        _ => card_end_of_turn_hp_loss_and_damage_v1(card_id, magic)
            .map_or(0, |effect| effect.hp_loss_per_application),
    }
}

fn vulnerable(card_id: CardId, magic: i32) -> i32 {
    match card_id {
        CardId::Bash | CardId::Uppercut | CardId::Shockwave | CardId::Terror => magic,
        CardId::ThunderClap | CardId::Trip | CardId::BeamCell => 1,
        _ => 0,
    }
}

fn player_vulnerable(card_id: CardId, magic: i32) -> i32 {
    match card_id {
        CardId::Berserk => magic,
        _ => 0,
    }
}

fn weak(card_id: CardId, magic: i32) -> i32 {
    match card_id {
        CardId::Clothesline | CardId::Uppercut | CardId::Shockwave | CardId::Intimidate => magic,
        CardId::Blind | CardId::SuckerPunch | CardId::GoForTheEyes => magic.max(1),
        _ => 0,
    }
}

fn strength_gain(card_id: CardId, magic: i32) -> i32 {
    card_player_strength_delivery_v1(card_id).map_or(0, |_| magic)
}

fn max_hp_gain_on_fatal(card_id: CardId, magic: i32) -> i32 {
    match card_id {
        CardId::Feed => magic,
        _ => 0,
    }
}

fn intangible_gain(card_id: CardId, magic: i32) -> i32 {
    match card_id {
        CardId::Apparition | CardId::WraithForm => magic,
        _ => 0,
    }
}

fn exhausts_other_cards(card_id: CardId) -> bool {
    matches!(
        card_id,
        CardId::BurningPact
            | CardId::TrueGrit
            | CardId::SecondWind
            | CardId::SeverSoul
            | CardId::FiendFire
            | CardId::Recycle
    )
}

fn status_injections(card_id: CardId) -> Vec<CardRewardStatusInjectionV1> {
    use CardRewardStatusDestinationV1::{DiscardPile, DrawPile, Hand};
    use CardRewardStatusPersistenceV1::{Ethereal, Persistent};

    if let Some(request) =
        crate::content::cards::mechanics::card_status_creation_on_play_v1(card_id)
    {
        use crate::content::cards::mechanics::CardStatusCreationDestinationV1 as Destination;
        return vec![CardRewardStatusInjectionV1 {
            card: request.card,
            count: request.amount as i32,
            destination: match request.destination {
                Destination::Hand => Hand,
                Destination::DrawPileRandom => DrawPile,
                Destination::DiscardPile => DiscardPile,
            },
            persistence: if crate::content::cards::mechanics::card_is_ethereal(
                request.card,
                u8::from(request.upgraded),
            ) {
                Ethereal
            } else {
                Persistent
            },
        }];
    }

    let injection = match card_id {
        CardId::Turbo => CardRewardStatusInjectionV1 {
            card: CardId::Void,
            count: 1,
            destination: DiscardPile,
            persistence: Ethereal,
        },
        _ => return Vec::new(),
    };
    vec![injection]
}

fn upgrades_cards(card_id: CardId) -> bool {
    matches!(card_id, CardId::Armaments | CardId::Apotheosis)
}

fn is_random_output(card_id: CardId) -> bool {
    matches!(
        card_id,
        CardId::SwordBoomerang
            | CardId::Discovery
            | CardId::InfernalBlade
            | CardId::JackOfAllTrades
            | CardId::WhiteNoise
            | CardId::Chrysalis
            | CardId::Metamorphosis
            | CardId::Transmutation
            | CardId::Magnetism
            | CardId::Mayhem
    )
}

fn has_conditional_playability(card_id: CardId) -> bool {
    matches!(
        card_id,
        CardId::Clash
            | CardId::BodySlam
            | CardId::PerfectedStrike
            | CardId::GrandFinale
            | CardId::Expertise
    )
}

fn pick_dependencies(card_id: CardId) -> Vec<CardRewardPickDependencyV1> {
    let mut dependencies = Vec::new();
    match card_id {
        CardId::SearingBlow => dependencies.push(CardRewardPickDependencyV1::RouteUpgradeDensity),
        CardId::BodySlam | CardId::Barricade | CardId::Entrench | CardId::Juggernaut => {
            dependencies.push(CardRewardPickDependencyV1::BlockDensity)
        }
        CardId::PerfectedStrike => dependencies.push(CardRewardPickDependencyV1::StrikeDensity),
        CardId::FeelNoPain | CardId::DarkEmbrace | CardId::Corruption => {
            dependencies.push(CardRewardPickDependencyV1::ExhaustPackage)
        }
        CardId::Sentinel => dependencies.push(CardRewardPickDependencyV1::ExhaustPackage),
        CardId::Evolve | CardId::FireBreathing => {
            dependencies.push(CardRewardPickDependencyV1::StatusPackage)
        }
        CardId::Rupture => dependencies.push(CardRewardPickDependencyV1::SelfDamagePackage),
        _ => {}
    }
    let strategic = strategic_card_definition(card_id);
    if strategic
        .payoff_requirements
        .contains(&PayoffRequirement::WantsMechanic(Mechanic::Strength))
        || strategic.play_effects.iter().any(|effect| {
            matches!(
                effect,
                PlayEffect::DamageUses(Mechanic::Strength)
                    | PlayEffect::DamageScalesWith(DamageScalingAxis::PerHitStrength)
            )
        })
    {
        dependencies.push(CardRewardPickDependencyV1::StrengthScaling);
    }
    if is_random_output(card_id) {
        dependencies.push(CardRewardPickDependencyV1::RandomOutputPolicy);
    }
    if has_conditional_playability(card_id) {
        dependencies.push(CardRewardPickDependencyV1::ConditionalPlayabilityPolicy);
    }
    dependencies
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::rewards::RewardCard;

    #[test]
    fn formerly_blacklisted_cards_have_specific_effects_and_upgrade_facts() {
        use super::super::{card_reward_semantic_profile_v1, CardRewardSemanticRoleV1 as Role};
        for upgrades in [0, 1] {
            let facts = |card| card_reward_facts_v1(&RewardCard::new(card, upgrades));
            assert_eq!(facts(CardId::Warcry).hand_topdeck_after_draw, Some(1));
            assert_eq!(facts(CardId::Warcry).draw_cards, 1 + i32::from(upgrades));
            assert_eq!(
                facts(CardId::Rampage).damage_growth_on_play,
                Some(5 + 3 * i32::from(upgrades))
            );
            assert_eq!(
                facts(CardId::BloodForBlood).cost_change_on_hp_loss,
                Some(-1)
            );
            assert_eq!(facts(CardId::BloodForBlood).cost, 4 - upgrades as i8);
            let dropkick = facts(CardId::Dropkick);
            assert_eq!(
                dropkick.draw_cards, 0,
                "conditional draw must not become unconditional"
            );
            assert_eq!(dropkick.energy_gain, 0);
            let follow_up = dropkick.target_vulnerable_follow_up.unwrap();
            assert_eq!((follow_up.energy_gain, follow_up.draw_cards), (1, 1));
            assert_eq!(
                facts(CardId::DualWield).hand_copy.unwrap().eligible_types,
                [
                    crate::content::cards::CardType::Attack,
                    crate::content::cards::CardType::Power
                ]
            );
            assert_eq!(
                facts(CardId::Exhume)
                    .exhaust_pile_to_hand
                    .unwrap()
                    .excluded_card,
                CardId::Exhume
            );
            assert!(
                facts(CardId::Havoc)
                    .top_deck_play
                    .unwrap()
                    .exhaust_after_play
            );

            for (card, role) in [
                (CardId::Warcry, Role::HandTopdeckSelection),
                (CardId::Rampage, Role::CardInstanceDamageGrowth),
                (CardId::BloodForBlood, Role::HpLossCostReduction),
                (CardId::Dropkick, Role::ConditionalCardAccess),
                (CardId::DualWield, Role::HandCopy),
                (CardId::Exhume, Role::ExhaustReuse),
                (CardId::Havoc, Role::TopDeckPlay),
            ] {
                let profile = card_reward_semantic_profile_v1(&RewardCard::new(card, upgrades));
                assert!(
                    profile.roles.contains(&role),
                    "{card:?}+{upgrades}: {profile:?}"
                );
                assert!(crate::content::cards::mechanics::explanation(card).is_some());
            }
        }
    }

    #[test]
    fn reward_damage_does_not_turn_choice_parameters_into_attacks() {
        for upgrades in [0, 1] {
            let facts = card_reward_facts_v1(&RewardCard::new(CardId::Wish, upgrades));
            assert_eq!(facts.damage.total_damage, 0, "Wish's field is Strength");
        }
    }

    #[test]
    fn reward_damage_counts_reviewed_repeated_attacks() {
        for (card, base_hits, upgraded_hits) in [
            (CardId::Pummel, 4, 5),
            (CardId::Ragnarok, 5, 6),
            (CardId::RipAndTear, 2, 2),
            (CardId::DaggerSpray, 2, 2),
            (CardId::Eviscerate, 3, 3),
            (CardId::Tantrum, 3, 4),
            (CardId::FlyingSleeves, 2, 2),
            (CardId::GlassKnife, 2, 2),
        ] {
            for (upgrades, hits) in [(0, base_hits), (1, upgraded_hits)] {
                let facts = card_reward_facts_v1(&RewardCard::new(card, upgrades));
                assert_eq!(facts.damage.hit_count, hits, "{card:?}+{upgrades}");
            }
        }
    }

    #[test]
    fn block_delivery_qualifies_definition_fields_and_old_unknown_facts() {
        use crate::content::cards::mechanics::CardBaseBlockDeliveryV1;
        for (card, delivery) in [
            (
                CardId::AutoShields,
                CardBaseBlockDeliveryV1::IfPlayerBlockZeroAtUse,
            ),
            (
                CardId::EscapePlan,
                CardBaseBlockDeliveryV1::IfThisDrawHistoryContainsSkill,
            ),
            (
                CardId::SecondWind,
                CardBaseBlockDeliveryV1::PerNonAttackInHandAtResolution,
            ),
            (
                CardId::ReinforcedBody,
                CardBaseBlockDeliveryV1::PerEffectiveX,
            ),
            (CardId::Wish, CardBaseBlockDeliveryV1::PlatedArmorIfChosen),
            (
                CardId::Stack,
                CardBaseBlockDeliveryV1::ReplacedByDiscardPileSizePlusUpgrade,
            ),
        ] {
            let facts = card_reward_facts_v1(&RewardCard::new(card, 1));
            assert_eq!(facts.base_block_delivery, Some(delivery));
            assert_eq!(facts.unconditional_base_block_request_v1(), None);
        }
        let shrug = card_reward_facts_v1(&RewardCard::new(CardId::ShrugItOff, 0));
        assert_eq!(shrug.unconditional_base_block_request_v1(), Some(8));
        let mut old = shrug;
        old.base_block_delivery = None;
        assert_eq!(old.unconditional_base_block_request_v1(), None);
    }

    #[test]
    fn damage_unknown_and_random_or_variable_scope_are_not_focused_totals() {
        for id in [
            CardId::Whirlwind,
            CardId::Skewer,
            CardId::FiendFire,
            CardId::Barrage,
            CardId::ThunderStrike,
            CardId::Finisher,
            CardId::Flechettes,
            CardId::BodySlam,
            CardId::MindBlast,
            CardId::Blizzard,
            CardId::Expunger,
        ] {
            let facts = card_reward_facts_v1(&RewardCard::new(id, 0));
            assert!(facts.damage.has_reviewed_attack());
            assert_eq!(facts.damage.unconditional_base_total(), None, "{id:?}");
        }
        let random = card_reward_facts_v1(&RewardCard::new(CardId::Ragnarok, 1));
        assert_eq!(random.damage.unconditional_base_total(), Some(36));
        assert_eq!(random.damage.unconditional_targeted_base_total(), None);
        let aoe = card_reward_facts_v1(&RewardCard::new(CardId::DaggerSpray, 0));
        assert_eq!(aoe.damage.unconditional_targeted_base_total(), Some(8));
        let mut old = aoe.damage;
        old.delivery = None;
        assert_eq!(
            old.unconditional_base_total(),
            None,
            "old numbers cannot certify delivery"
        );
    }

    #[test]
    fn acquisition_facts_preserve_enemy_strength_restoration_and_upgrade_amounts() {
        use crate::content::cards::mechanics::EnemyStrengthRestorationV1;
        for (id, base, upgraded, restoration) in [
            (
                CardId::Disarm,
                2,
                3,
                EnemyStrengthRestorationV1::NoScheduledRestoration,
            ),
            (
                CardId::DarkShackles,
                9,
                15,
                EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd,
            ),
            (
                CardId::PiercingWail,
                6,
                8,
                EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd,
            ),
        ] {
            let facts = card_acquisition_mechanics_facts_v1(&RewardCard::new(id, 0));
            let upgraded_facts = facts.upgraded.expect("upgrade facts");
            assert_eq!(facts.current.enemy_strength_down, base);
            assert_eq!(upgraded_facts.enemy_strength_down, upgraded);
            for f in [&facts.current, &upgraded_facts] {
                assert_eq!(f.enemy_strength_restoration, Some(restoration));
                assert_eq!(
                    f.has_persistent_enemy_strength_reduction_v1(),
                    id == CardId::Disarm
                );
                let semantics = strategic_card_definition(id);
                assert_eq!(
                    semantics
                        .play_effects
                        .contains(&PlayEffect::Provide(Mechanic::EnemyStrengthDown)),
                    id == CardId::Disarm
                );
                assert_eq!(
                    semantics
                        .play_effects
                        .contains(&PlayEffect::Provide(Mechanic::TemporaryEnemyStrengthDown)),
                    id != CardId::Disarm
                );
            }
        }
    }

    #[test]
    fn reward_draw_requests_preserve_upgrades_and_non_ironclad_access() {
        for (id, base, upgraded) in [
            (CardId::DeepBreath, 1, 2),
            (CardId::Coolheaded, 1, 2),
            (CardId::Prepared, 1, 2),
            (CardId::ThinkingAhead, 2, 2),
            (CardId::EscapePlan, 1, 1),
            (CardId::Scrape, 4, 5),
            (CardId::CutThroughFate, 1, 1),
        ] {
            let facts = card_acquisition_mechanics_facts_v1(&RewardCard::new(id, 0));
            assert_eq!(facts.current.draw_cards, base, "{id:?}");
            assert_eq!(facts.upgraded.unwrap().draw_cards, upgraded, "{id:?}+");
        }
    }

    #[test]
    fn acquisition_upgrade_facts_follow_real_upgrade_legality_and_searing_growth() {
        for (upgrades, damage, next_damage) in [(0, 12, 16), (1, 16, 21), (5, 42, 51)] {
            let facts = card_acquisition_mechanics_facts_v1(&RewardCard::new(
                CardId::SearingBlow,
                upgrades,
            ));
            assert_eq!(facts.current.damage.damage_per_hit, damage);
            let next = facts.upgraded.expect("Searing Blow can upgrade again");
            assert_eq!(next.upgrades, upgrades + 1);
            assert_eq!(next.damage.damage_per_hit, next_damage);
        }
        for id in [CardId::AscendersBane, CardId::Wound] {
            assert!(card_acquisition_mechanics_facts_v1(&RewardCard::new(id, 0))
                .upgraded
                .is_none());
        }
        assert!(
            card_acquisition_mechanics_facts_v1(&RewardCard::new(CardId::Bash, 1))
                .upgraded
                .is_none()
        );
    }

    #[test]
    fn seeing_red_facts_report_fixed_energy_gain() {
        let facts = card_reward_facts_v1(&RewardCard::new(CardId::SeeingRed, 0));

        assert_eq!(facts.energy_gain, 2);
    }

    #[test]
    fn pommel_strike_facts_preserve_the_draw_upgrade() {
        let base = card_reward_facts_v1(&RewardCard::new(CardId::PommelStrike, 0));
        let upgraded = card_reward_facts_v1(&RewardCard::new(CardId::PommelStrike, 1));

        assert_eq!(base.draw_cards, 1);
        assert_eq!(upgraded.draw_cards, 2);
    }

    #[test]
    fn acquisition_facts_preserve_brutalitys_innate_only_upgrade() {
        let facts = card_acquisition_mechanics_facts_v1(&RewardCard::new(CardId::Brutality, 0));
        let upgraded = facts.upgraded.expect("unupgraded card has upgrade facts");

        assert_eq!(facts.current.draw_cards, 0);
        assert_eq!(upgraded.draw_cards, 0);
        assert_eq!(facts.current.draw_cards_each_turn, 1);
        assert_eq!(upgraded.draw_cards_each_turn, 1);
        assert_eq!(facts.current.hp_loss_each_turn, 1);
        assert_eq!(upgraded.hp_loss_each_turn, 1);
        assert!(!facts.current.innate);
        assert!(upgraded.innate);
    }

    #[test]
    fn acquisition_facts_preserve_direct_self_hp_loss_on_play() {
        for (card_id, expected_hp_loss) in [
            (CardId::Bloodletting, 3),
            (CardId::Hemokinesis, 2),
            (CardId::Offering, 6),
            (CardId::JAX, 3),
        ] {
            let facts = card_acquisition_mechanics_facts_v1(&RewardCard::new(card_id, 0));
            let loss = facts
                .current
                .self_hp_loss_on_play
                .expect("direct self HP loss fact");
            assert_eq!(loss.base_hp_loss, expected_hp_loss);
            assert!(loss.bypasses_block);
            assert!(!loss.bypasses_intangible);
            assert!(loss.triggers_rupture);
            assert_eq!(
                facts.upgraded.expect("upgrade facts").self_hp_loss_on_play,
                Some(loss)
            );
        }

        assert_eq!(
            card_reward_facts_v1(&RewardCard::new(CardId::Strike, 0)).self_hp_loss_on_play,
            None
        );
    }

    #[test]
    fn acquisition_facts_preserve_recurring_end_of_turn_block() {
        let facts = card_acquisition_mechanics_facts_v1(&RewardCard::new(CardId::Metallicize, 0));
        let current = facts
            .current
            .end_of_turn_block_each_turn
            .expect("Metallicize recurring Block facts");
        assert_eq!(current.amount, 3);
        assert!(current.persists_for_combat);
        assert!(current.stacks_additively);
        assert!(!current.affected_by_dexterity);
        assert!(!current.affected_by_frail);
        assert_eq!(
            facts
                .upgraded
                .expect("Metallicize upgrade facts")
                .end_of_turn_block_each_turn
                .expect("upgraded Metallicize recurring Block")
                .amount,
            4
        );
        assert_eq!(
            card_reward_facts_v1(&RewardCard::new(CardId::Defend, 0)).end_of_turn_block_each_turn,
            None
        );
    }

    #[test]
    fn acquisition_facts_preserve_true_grits_control_upgrade() {
        let facts = card_acquisition_mechanics_facts_v1(&RewardCard::new(CardId::TrueGrit, 0));
        let upgraded = facts.upgraded.expect("unupgraded card has upgrade facts");

        assert!(facts.current.exhausts_other_cards);
        assert!(facts.current.is_random_output);
        assert!(!facts.current.selects_from_hand);
        assert!(upgraded.exhausts_other_cards);
        assert!(!upgraded.is_random_output);
        assert!(upgraded.selects_from_hand);
    }

    #[test]
    fn acquisition_facts_distinguish_armaments_target_scope() {
        use crate::ai::combat_upgrade_coverage_v1::{
            CombatUpgradePersistenceV1, CombatUpgradeScopeV1,
        };

        let facts = card_acquisition_mechanics_facts_v1(&RewardCard::new(CardId::Armaments, 0));
        let upgraded = facts.upgraded.expect("unupgraded card has upgrade facts");

        assert!(facts.current.selects_from_hand);
        assert_eq!(
            facts.current.combat_upgrade_scope,
            Some(CombatUpgradeScopeV1::SelectedCardInHand)
        );
        assert_eq!(
            facts.current.combat_upgrade_persistence,
            Some(CombatUpgradePersistenceV1::CombatOnly)
        );
        assert!(!facts.current.is_random_output);
        assert!(!upgraded.selects_from_hand);
        assert_eq!(
            upgraded.combat_upgrade_scope,
            Some(CombatUpgradeScopeV1::WholeHand)
        );
        assert_eq!(
            upgraded.combat_upgrade_persistence,
            Some(CombatUpgradePersistenceV1::CombatOnly)
        );
        assert!(!upgraded.is_random_output);
    }

    #[test]
    fn reward_facts_use_upgrade_sensitive_exhaust_and_ethereal_mechanics() {
        let limit_break =
            card_acquisition_mechanics_facts_v1(&RewardCard::new(CardId::LimitBreak, 0));
        assert!(limit_break.current.exhausts);
        assert!(!limit_break.upgraded.expect("upgrade facts").exhausts);

        let apparition =
            card_acquisition_mechanics_facts_v1(&RewardCard::new(CardId::Apparition, 0));
        assert!(apparition.current.ethereal);
        assert_eq!(apparition.current.intangible_gain, 1);
        assert_eq!(apparition.current.intangible_damage_cap, 1);
        assert!(apparition.current.intangible_caps_direct_hp_loss_to_one);
        let upgraded_apparition = apparition.upgraded.expect("upgrade facts");
        assert!(!upgraded_apparition.ethereal);
        assert_eq!(upgraded_apparition.intangible_gain, 1);
        assert_eq!(upgraded_apparition.intangible_damage_cap, 1);
        assert!(upgraded_apparition.intangible_caps_direct_hp_loss_to_one);
    }

    #[test]
    fn wraith_form_facts_preserve_the_intangible_upgrade() {
        let facts = card_acquisition_mechanics_facts_v1(&RewardCard::new(CardId::WraithForm, 0));
        let upgraded = facts.upgraded.expect("upgrade facts");

        assert_eq!(facts.current.intangible_gain, 2);
        assert_eq!(facts.current.intangible_damage_cap, 1);
        assert!(facts.current.intangible_caps_direct_hp_loss_to_one);
        assert_eq!(upgraded.intangible_gain, 3);
        assert_eq!(upgraded.intangible_damage_cap, 1);
        assert!(upgraded.intangible_caps_direct_hp_loss_to_one);
    }

    #[test]
    fn typed_discard_topdeck_access_has_its_actual_role() {
        let profile =
            super::super::card_reward_semantic_profile_v1(&RewardCard::new(CardId::Headbutt, 0));
        assert!(profile
            .roles
            .contains(&super::super::CardRewardSemanticRoleV1::DiscardPileTopdeckAccess));
    }

    #[test]
    fn typed_fiend_fire_hand_conversion_preserves_its_actual_effects() {
        let facts = card_reward_facts_v1(&RewardCard::new(CardId::FiendFire, 0));
        let strategic = strategic_card_definition(CardId::FiendFire);

        assert!(facts.exhausts_other_cards);
        assert!(strategic
            .play_effects
            .contains(&PlayEffect::DamageScalesWith(DamageScalingAxis::HandSize)));
        assert!(strategic.play_effects.contains(&PlayEffect::EmitEvent(
            crate::ai::analysis::card_semantics::CombatEvent::CardExhausted
        )));
        assert!(strategic
            .burdens
            .contains(&crate::ai::analysis::card_semantics::CardBurden::ExhaustsHand));
    }

    #[test]
    fn shockwave_facts_do_not_claim_direct_strength_reduction() {
        let facts = card_reward_facts_v1(&RewardCard::new(CardId::Shockwave, 0));

        assert!(facts.weak > 0);
        assert!(facts.vulnerable > 0);
        assert_eq!(facts.enemy_strength_down, 0);
    }

    #[test]
    fn targetless_random_hits_do_not_masquerade_as_all_enemy_damage() {
        let sword_boomerang = card_reward_facts_v1(&RewardCard::new(CardId::SwordBoomerang, 0));
        let whirlwind = card_reward_facts_v1(&RewardCard::new(CardId::Whirlwind, 0));
        let shockwave = card_reward_facts_v1(&RewardCard::new(CardId::Shockwave, 0));

        assert!(!sword_boomerang.is_aoe);
        assert!(whirlwind.is_aoe);
        assert!(!shockwave.is_aoe);
    }

    #[test]
    fn random_attack_hand_facts_keep_upgrade_payment_separate_from_generation() {
        for (upgrades, cost) in [(0, 1), (1, 0)] {
            let facts = card_reward_facts_v1(&RewardCard::new(CardId::InfernalBlade, upgrades));
            let rule = facts.random_attack_to_hand.unwrap();
            assert_eq!(facts.cost, cost);
            assert_eq!(rule.count, 1);
            assert_eq!(rule.cost_for_turn, 0);
            assert!(facts.exhausts);
            assert!(facts.is_random_output);
        }
        assert!(card_reward_facts_v1(&RewardCard::new(CardId::FiendFire, 0))
            .random_attack_to_hand
            .is_none());
    }

    #[test]
    fn per_hit_strength_scalers_share_the_strength_payoff_semantics() {
        for card in [CardId::Whirlwind, CardId::FiendFire] {
            let facts = card_reward_facts_v1(&RewardCard::new(card, 0));
            assert!(
                facts
                    .pick_dependencies
                    .contains(&CardRewardPickDependencyV1::StrengthScaling),
                "{card:?} should inherit strength payoff semantics from its damage model"
            );
        }
    }

    #[test]
    fn status_injections_preserve_destination_and_persistence() {
        let cases = [
            (
                CardId::Turbo,
                CardId::Void,
                1,
                CardRewardStatusDestinationV1::DiscardPile,
                CardRewardStatusPersistenceV1::Ethereal,
            ),
            (
                CardId::WildStrike,
                CardId::Wound,
                1,
                CardRewardStatusDestinationV1::DrawPile,
                CardRewardStatusPersistenceV1::Persistent,
            ),
            (
                CardId::RecklessCharge,
                CardId::Dazed,
                1,
                CardRewardStatusDestinationV1::DrawPile,
                CardRewardStatusPersistenceV1::Ethereal,
            ),
            (
                CardId::PowerThrough,
                CardId::Wound,
                2,
                CardRewardStatusDestinationV1::Hand,
                CardRewardStatusPersistenceV1::Persistent,
            ),
            (
                CardId::Immolate,
                CardId::Burn,
                1,
                CardRewardStatusDestinationV1::DiscardPile,
                CardRewardStatusPersistenceV1::Persistent,
            ),
        ];

        for (card, status, count, destination, persistence) in cases {
            let facts = card_reward_facts_v1(&RewardCard::new(card, 0));
            assert_eq!(
                facts.status_injections,
                vec![CardRewardStatusInjectionV1 {
                    card: status,
                    count,
                    destination,
                    persistence,
                }],
                "card={card:?}"
            );
            assert_eq!(facts.adds_status_cards, count);
        }
    }

    #[test]
    fn reward_facts_use_the_upgraded_base_cost() {
        let base = card_reward_facts_v1(&RewardCard::new(CardId::DarkEmbrace, 0));
        let upgraded = card_reward_facts_v1(&RewardCard::new(CardId::DarkEmbrace, 1));

        assert_eq!(base.cost, 2);
        assert_eq!(upgraded.cost, 1);
    }

    #[test]
    fn reward_facts_preserve_native_tags_without_name_inference() {
        for card in [
            CardId::Strike,
            CardId::PerfectedStrike,
            CardId::TwinStrike,
            CardId::Bash,
        ] {
            let facts = card_reward_facts_v1(&RewardCard::new(card, 0));
            assert_eq!(facts.tags.as_deref(), Some(get_card_definition(card).tags));
        }
    }
}
