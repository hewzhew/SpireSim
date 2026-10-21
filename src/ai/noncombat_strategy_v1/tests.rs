use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;
use crate::state::map::node::{MapEdge, MapRoomNode, RoomType};
use crate::state::map::state::MapState;
use crate::state::run::RunState;

use super::candidate::candidate_plan_delta_v1;
use super::snapshot::build_run_strategy_snapshot_v1;
use super::types::{
    StrategyCandidateFactsV1, StrategyDeckFactsV1, StrategyDeckFormationNeedV1,
    StrategyDeckFormationStageV1, StrategyPackageGapV2, StrategyPlanEffectV1, StrategyPlanIdV1,
    StrategyPlanSupportV1, StrategyRouteFutureV1, StrategyRoutePackageIdV1, StrategyThreatSourceV1,
    StrategyThreatTagV1,
};

#[test]
fn run_strategy_snapshot_keeps_multiple_plan_hypotheses() {
    let snapshot = build_run_strategy_snapshot_v1(
        StrategyDeckFactsV1 {
            deck_size: 11,
            attacks: 7,
            skills: 4,
            powers: 0,
            starter_strikes: 5,
            starter_defends: 4,
            strength_sources: 1,
            strength_payoffs: 0,
            weak_sources: 0,
            draw_sources: 0,
            energy_sources: 0,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 1,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 42,
            total_block: 20,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 2,
            max_fires: 4,
            first_fire_floor: Some(5),
            max_early_pressure: 2,
            need_heal: 0.2,
            avoid_damage: 0.3,
            ..Default::default()
        }),
    );

    assert_eq!(
        snapshot
            .plan(StrategyPlanIdV1::StrengthScaling)
            .map(|plan| plan.support),
        Some(StrategyPlanSupportV1::Strong)
    );
    assert_eq!(
        snapshot
            .plan(StrategyPlanIdV1::WeakControl)
            .map(|plan| plan.support),
        Some(StrategyPlanSupportV1::Plausible)
    );
    assert!(snapshot.plans.len() >= 6);
}

#[test]
fn candidate_plan_delta_uses_strategy_snapshot_not_card_score() {
    let snapshot = build_run_strategy_snapshot_v1(
        StrategyDeckFactsV1 {
            deck_size: 10,
            attacks: 6,
            skills: 4,
            powers: 0,
            starter_strikes: 5,
            starter_defends: 4,
            strength_sources: 0,
            strength_payoffs: 0,
            weak_sources: 0,
            draw_sources: 0,
            energy_sources: 0,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 1,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 36,
            total_block: 15,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 3,
            max_fires: 4,
            first_fire_floor: Some(4),
            max_early_pressure: 1,
            need_heal: 0.0,
            avoid_damage: 0.1,
            ..Default::default()
        }),
    );

    let searing = candidate_plan_delta_v1(
        StrategyCandidateFactsV1 {
            card: CardId::SearingBlow,
            damage_total: 12,
            weak: 0,
            strength_gain: 0,
            plan_effects: vec![
                StrategyPlanEffectV1::UpgradeSink,
                StrategyPlanEffectV1::UpgradeBudgetConsumer,
            ],
        },
        &snapshot,
    );
    let clothesline = candidate_plan_delta_v1(
        StrategyCandidateFactsV1 {
            card: CardId::Clothesline,
            damage_total: 12,
            weak: 2,
            strength_gain: 0,
            plan_effects: vec![
                StrategyPlanEffectV1::WeakCoverage,
                StrategyPlanEffectV1::DamageMitigation,
            ],
        },
        &snapshot,
    );

    assert!(searing.effects.contains(&StrategyPlanEffectV1::UpgradeSink));
    assert!(clothesline
        .effects
        .contains(&StrategyPlanEffectV1::WeakCoverage));
    assert_eq!(searing.support, StrategyPlanSupportV1::Strong);
}

#[test]
fn starter_shell_formation_marks_frontload_and_scaling_needs() {
    let snapshot = build_run_strategy_snapshot_v1(
        StrategyDeckFactsV1 {
            deck_size: 10,
            attacks: 6,
            skills: 4,
            powers: 0,
            starter_strikes: 5,
            starter_defends: 4,
            strength_sources: 0,
            strength_payoffs: 0,
            weak_sources: 0,
            draw_sources: 0,
            energy_sources: 0,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 1,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 36,
            total_block: 15,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 1,
            max_fires: 2,
            first_fire_floor: Some(6),
            max_early_pressure: 3,
            need_heal: 0.4,
            avoid_damage: 0.5,
            ..Default::default()
        }),
    );

    assert_eq!(
        snapshot.formation.stage,
        StrategyDeckFormationStageV1::StarterShell
    );
    assert!(snapshot
        .formation
        .needs
        .contains(&StrategyDeckFormationNeedV1::Frontload));
    assert!(snapshot
        .formation
        .needs
        .contains(&StrategyDeckFormationNeedV1::Scaling));
}

#[test]
fn act1_first_elite_pressure_keeps_frontload_need_after_one_transition_attack() {
    let snapshot = build_run_strategy_snapshot_v1(
        StrategyDeckFactsV1 {
            deck_size: 11,
            attacks: 7,
            skills: 4,
            powers: 0,
            starter_strikes: 5,
            starter_defends: 4,
            strength_sources: 0,
            strength_payoffs: 0,
            weak_sources: 0,
            draw_sources: 0,
            energy_sources: 0,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 1,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 48,
            total_block: 15,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 0,
            max_fires: 1,
            first_fire_floor: Some(6),
            max_early_pressure: 2,
            need_heal: 0.2,
            avoid_damage: 0.2,
            ..Default::default()
        }),
    );

    assert!(
        snapshot
            .formation
            .needs
            .contains(&StrategyDeckFormationNeedV1::Frontload),
        "one extra transition attack should not clear the Act1 first-elite frontload gate"
    );
}

#[test]
fn forced_first_elite_without_bailout_makes_route_pressure_high() {
    let snapshot = build_run_strategy_snapshot_v1(
        StrategyDeckFactsV1 {
            deck_size: 11,
            attacks: 7,
            skills: 4,
            powers: 0,
            starter_strikes: 5,
            starter_defends: 4,
            strength_sources: 0,
            strength_payoffs: 0,
            weak_sources: 0,
            draw_sources: 0,
            energy_sources: 0,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 1,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 55,
            total_block: 20,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 1,
            max_fires: 2,
            first_fire_floor: Some(7),
            max_early_pressure: 1,
            need_heal: 0.1,
            avoid_damage: 0.1,
            first_elite_forced: true,
            max_hallways_before_first_elite: 1,
            can_bail_to_rest_before_first_elite: false,
            can_bail_to_shop_before_first_elite: false,
        }),
    );

    let route_package = snapshot
        .route_packages
        .iter()
        .find(|package| package.id == StrategyRoutePackageIdV1::CombatPatchWindow)
        .expect("combat patch package should exist");

    assert!(snapshot
        .formation
        .needs
        .contains(&StrategyDeckFormationNeedV1::Frontload));
    assert!(snapshot
        .formation
        .needs
        .contains(&StrategyDeckFormationNeedV1::Block));
    assert_eq!(route_package.support, StrategyPlanSupportV1::Strong);
    assert!(route_package
        .evidence
        .iter()
        .any(|line| line.contains("route pressure is High")));
}

#[test]
fn supported_engine_formation_marks_plan_committed() {
    let snapshot = build_run_strategy_snapshot_v1(
        StrategyDeckFactsV1 {
            deck_size: 14,
            attacks: 8,
            skills: 4,
            powers: 2,
            starter_strikes: 4,
            starter_defends: 3,
            strength_sources: 2,
            strength_payoffs: 1,
            weak_sources: 1,
            draw_sources: 1,
            energy_sources: 1,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 0,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 86,
            total_block: 32,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 2,
            max_fires: 3,
            first_fire_floor: Some(5),
            max_early_pressure: 1,
            need_heal: 0.1,
            avoid_damage: 0.1,
            ..Default::default()
        }),
    );

    assert_eq!(
        snapshot.formation.stage,
        StrategyDeckFormationStageV1::PlanCommitted
    );
    assert!(snapshot
        .formation
        .strengths
        .contains(&StrategyPlanIdV1::StrengthScaling));
    assert!(!snapshot
        .formation
        .needs
        .contains(&StrategyDeckFormationNeedV1::Scaling));
}

#[test]
fn route_packages_link_upgrade_commitment_to_visible_fire_budget() {
    let snapshot = build_run_strategy_snapshot_v1(
        StrategyDeckFactsV1 {
            deck_size: 10,
            attacks: 6,
            skills: 4,
            powers: 0,
            starter_strikes: 5,
            starter_defends: 4,
            strength_sources: 0,
            strength_payoffs: 0,
            weak_sources: 0,
            draw_sources: 0,
            energy_sources: 0,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 1,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 36,
            total_block: 15,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 3,
            max_fires: 4,
            first_fire_floor: Some(4),
            max_early_pressure: 1,
            need_heal: 0.0,
            avoid_damage: 0.1,
            ..Default::default()
        }),
    );

    assert_eq!(
        snapshot
            .route_packages
            .iter()
            .find(|package| package.id == StrategyRoutePackageIdV1::UpgradeCommitment)
            .map(|package| package.support),
        Some(StrategyPlanSupportV1::Strong)
    );
}

#[test]
fn route_packages_mark_combat_patch_window_under_pressure() {
    let snapshot = build_run_strategy_snapshot_v1(
        StrategyDeckFactsV1 {
            deck_size: 10,
            attacks: 6,
            skills: 4,
            powers: 0,
            starter_strikes: 5,
            starter_defends: 4,
            strength_sources: 0,
            strength_payoffs: 0,
            weak_sources: 0,
            draw_sources: 0,
            energy_sources: 0,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 1,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 36,
            total_block: 15,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 1,
            max_fires: 2,
            first_fire_floor: Some(6),
            max_early_pressure: 3,
            need_heal: 0.4,
            avoid_damage: 0.5,
            ..Default::default()
        }),
    );

    assert_eq!(
        snapshot
            .route_packages
            .iter()
            .find(|package| package.id == StrategyRoutePackageIdV1::CombatPatchWindow)
            .map(|package| package.support),
        Some(StrategyPlanSupportV1::Strong)
    );
}

#[test]
fn route_packages_protect_committed_core_plan() {
    let snapshot = build_run_strategy_snapshot_v1(
        StrategyDeckFactsV1 {
            deck_size: 14,
            attacks: 8,
            skills: 4,
            powers: 2,
            starter_strikes: 4,
            starter_defends: 3,
            strength_sources: 2,
            strength_payoffs: 1,
            weak_sources: 1,
            draw_sources: 1,
            energy_sources: 1,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 0,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 86,
            total_block: 32,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 2,
            max_fires: 3,
            first_fire_floor: Some(5),
            max_early_pressure: 1,
            need_heal: 0.1,
            avoid_damage: 0.1,
            ..Default::default()
        }),
    );

    assert_eq!(
        snapshot
            .route_packages
            .iter()
            .find(|package| package.id == StrategyRoutePackageIdV1::CorePlanProtection)
            .map(|package| package.support),
        Some(StrategyPlanSupportV1::Strong)
    );
}

#[test]
fn strategy_snapshot_v2_unifies_archetype_route_and_resource_packages() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 14,
            attacks: 8,
            skills: 4,
            powers: 2,
            starter_strikes: 4,
            starter_defends: 3,
            strength_sources: 2,
            strength_payoffs: 1,
            weak_sources: 1,
            draw_sources: 1,
            energy_sources: 1,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 0,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 86,
            total_block: 32,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 2,
            max_fires: 3,
            first_fire_floor: Some(5),
            max_early_pressure: 1,
            need_heal: 0.1,
            avoid_damage: 0.1,
            ..Default::default()
        }),
        None,
    );

    assert_eq!(
        snapshot
            .package(super::StrategyPackageIdV2::StrengthScaling)
            .map(|package| package.domain),
        Some(super::StrategyPackageDomainV2::Archetype)
    );
    assert_eq!(
        snapshot
            .package(super::StrategyPackageIdV2::CorePlanProtection)
            .map(|package| package.domain),
        Some(super::StrategyPackageDomainV2::Route)
    );
    assert_eq!(
        snapshot
            .package(super::StrategyPackageIdV2::HpSafety)
            .map(|package| package.domain),
        Some(super::StrategyPackageDomainV2::Resource)
    );
    assert!(snapshot
        .packages
        .iter()
        .any(|package| package.domain == super::StrategyPackageDomainV2::Archetype));
    assert!(snapshot
        .packages
        .iter()
        .any(|package| package.domain == super::StrategyPackageDomainV2::Route));
    assert!(snapshot
        .packages
        .iter()
        .any(|package| package.domain == super::StrategyPackageDomainV2::Resource));
}

#[test]
fn strategy_snapshot_v2_from_run_state_tracks_resource_facts() {
    let mut run_state = crate::state::run::RunState::new(123, 0, false, "Ironclad");
    run_state.current_hp = 20;
    run_state.max_hp = 80;
    run_state.gold = 160;
    run_state.add_card_to_deck_without_interception_from(
        crate::content::cards::CardId::Doubt,
        0,
        crate::state::selection::DomainEventSource::DeckMutation,
    );
    run_state.potions[0] = Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::FirePotion,
        7,
    ));

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert_eq!(snapshot.resources.current_hp, 20);
    assert_eq!(snapshot.resources.max_hp, 80);
    assert_eq!(snapshot.resources.gold, 160);
    assert_eq!(snapshot.resources.curses, 1);
    assert_eq!(snapshot.resources.potion_slots, 3);
    assert_eq!(snapshot.resources.potion_count, 1);
    assert_eq!(snapshot.resources.empty_potion_slots, 2);
    assert_eq!(
        snapshot
            .package(super::StrategyPackageIdV2::HpSafety)
            .map(|package| package.support),
        Some(StrategyPlanSupportV1::Strong)
    );
    assert_eq!(
        snapshot
            .package(super::StrategyPackageIdV2::ShopRemoveWindow)
            .map(|package| package.support),
        Some(StrategyPlanSupportV1::Strong)
    );
}

#[test]
fn owned_summary_uses_reviewed_output_and_instance_growth() {
    use crate::runtime::combat::CombatCard;
    let mut run = RunState::new(1, 0, false, "Ironclad");
    for (id, upgrades, misc, damage, block) in [
        (CardId::Wish, 1, 0, 0, 0),
        (CardId::EscapePlan, 1, 0, 0, 0),
        (CardId::Pummel, 1, 0, 10, 0),
        (CardId::SearingBlow, 5, 0, 42, 0),
        (CardId::GeneticAlgorithm, 1, 27, 0, 27),
        (CardId::RitualDagger, 1, 31, 31, 0),
    ] {
        let mut card = CombatCard::for_scenario(id, 1);
        card.upgrades = upgrades;
        card.misc_value = misc;
        run.master_deck = vec![card];
        let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run);
        assert_eq!(snapshot.v1.deck.total_attack_damage, damage, "{id:?}");
        assert_eq!(snapshot.v1.deck.total_block, block, "{id:?}");
    }
}

#[test]
fn strategy_snapshot_v2_exposes_formation_and_candidate_delta_without_v1_access() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 12,
            attacks: 7,
            skills: 4,
            powers: 1,
            starter_strikes: 5,
            starter_defends: 4,
            strength_sources: 1,
            strength_payoffs: 0,
            weak_sources: 0,
            draw_sources: 0,
            energy_sources: 0,
            vulnerable_sources: 0,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 1,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 48,
            total_block: 20,
        },
        None,
        None,
    );

    assert!(snapshot.has_formation_strength(super::StrategyPackageIdV2::StrengthScaling));

    let delta = super::candidate_plan_delta_v2(
        StrategyCandidateFactsV1 {
            card: crate::content::cards::CardId::HeavyBlade,
            damage_total: 14,
            weak: 0,
            strength_gain: 0,
            plan_effects: vec![StrategyPlanEffectV1::StrengthPayoff],
        },
        &snapshot,
    );
    assert_eq!(delta.support, StrategyPlanSupportV1::Strong);
}

#[test]
fn strategy_snapshot_does_not_treat_flex_as_committed_strength_scaling() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.add_card_to_deck(CardId::Flex);

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert_eq!(
        snapshot.support(super::StrategyPackageIdV2::StrengthScaling),
        StrategyPlanSupportV1::Blocked
    );
    assert!(!snapshot.has_formation_strength(super::StrategyPackageIdV2::StrengthScaling));
}

#[test]
fn block_engine_package_recognizes_barricade_body_slam_followup() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 14,
            attacks: 7,
            skills: 5,
            powers: 2,
            starter_strikes: 4,
            starter_defends: 3,
            strength_sources: 0,
            strength_payoffs: 0,
            weak_sources: 1,
            draw_sources: 1,
            energy_sources: 1,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 0,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 1,
            block_payoffs: 0,
            block_multipliers: 1,
            total_attack_damage: 58,
            total_block: 38,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 2,
            max_fires: 3,
            first_fire_floor: Some(5),
            max_early_pressure: 1,
            need_heal: 0.1,
            avoid_damage: 0.1,
            ..Default::default()
        }),
        None,
    );

    let delta = super::candidate_plan_delta_v2(
        StrategyCandidateFactsV1 {
            card: CardId::BodySlam,
            damage_total: 0,
            weak: 0,
            strength_gain: 0,
            plan_effects: vec![StrategyPlanEffectV1::BlockPayoff],
        },
        &snapshot,
    );

    assert_eq!(
        snapshot.support(super::StrategyPackageIdV2::BlockEngine),
        StrategyPlanSupportV1::Strong
    );
    assert_eq!(delta.support, StrategyPlanSupportV1::Strong);
    assert!(delta.effects.contains(&StrategyPlanEffectV1::BlockPayoff));
    assert!(delta
        .notes
        .iter()
        .any(|note| note.contains("block retention sources=1")));
}

#[test]
fn candidate_plan_delta_uses_semantic_effects_without_card_identity() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 14,
            attacks: 7,
            skills: 5,
            powers: 2,
            weak_sources: 1,
            block_retention_sources: 1,
            block_multipliers: 1,
            total_attack_damage: 58,
            total_block: 38,
            ..Default::default()
        },
        None,
        None,
    );

    let delta = super::candidate_plan_delta_v2(
        StrategyCandidateFactsV1 {
            card: CardId::Strike,
            damage_total: 0,
            weak: 0,
            strength_gain: 0,
            plan_effects: vec![StrategyPlanEffectV1::BlockPayoff],
        },
        &snapshot,
    );

    assert_eq!(delta.support, StrategyPlanSupportV1::Strong);
    assert!(delta.effects.contains(&StrategyPlanEffectV1::BlockPayoff));
}

#[test]
fn body_slam_without_block_engine_remains_blocked_package_candidate() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 11,
            attacks: 7,
            skills: 4,
            powers: 0,
            starter_strikes: 5,
            starter_defends: 4,
            strength_sources: 0,
            strength_payoffs: 0,
            weak_sources: 0,
            draw_sources: 0,
            energy_sources: 0,
            vulnerable_sources: 1,
            route_upgrade_payoffs: 0,
            important_cards_unupgraded: 1,
            exhaust_generators: 0,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 0,
            block_retention_sources: 0,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 42,
            total_block: 15,
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 1,
            max_fires: 2,
            first_fire_floor: Some(6),
            max_early_pressure: 2,
            need_heal: 0.3,
            avoid_damage: 0.4,
            ..Default::default()
        }),
        None,
    );

    let delta = super::candidate_plan_delta_v2(
        StrategyCandidateFactsV1 {
            card: CardId::BodySlam,
            damage_total: 0,
            weak: 0,
            strength_gain: 0,
            plan_effects: vec![StrategyPlanEffectV1::BlockPayoff],
        },
        &snapshot,
    );

    assert_eq!(delta.support, StrategyPlanSupportV1::Blocked);
    assert!(delta.effects.contains(&StrategyPlanEffectV1::BlockPayoff));
}

#[test]
fn exhaust_engine_candidate_delta_uses_generator_and_payoff_roles() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 13,
            attacks: 7,
            skills: 5,
            powers: 1,
            starter_strikes: 4,
            starter_defends: 3,
            exhaust_payoffs: 1,
            total_attack_damage: 48,
            total_block: 22,
            ..Default::default()
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 1,
            max_fires: 2,
            first_fire_floor: Some(7),
            max_early_pressure: 2,
            need_heal: 0.2,
            avoid_damage: 0.3,
            ..Default::default()
        }),
        None,
    );

    let burning_pact = super::candidate_plan_delta_v2(
        StrategyCandidateFactsV1 {
            card: CardId::BurningPact,
            damage_total: 0,
            weak: 0,
            strength_gain: 0,
            plan_effects: vec![StrategyPlanEffectV1::ExhaustGenerator],
        },
        &snapshot,
    );

    assert_eq!(
        snapshot.support(super::StrategyPackageIdV2::ExhaustEngine),
        StrategyPlanSupportV1::Plausible
    );
    assert_eq!(burning_pact.support, StrategyPlanSupportV1::Plausible);
    assert!(burning_pact
        .effects
        .contains(&StrategyPlanEffectV1::ExhaustGenerator));
}

#[test]
fn run_snapshot_treats_corruption_as_generator_not_payoff() {
    let mut run_state = crate::state::run::RunState::new(2, 0, false, "Ironclad");
    run_state.add_card_to_deck(CardId::Corruption);

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert_eq!(snapshot.v1.deck.exhaust_generators, 1);
    assert_eq!(snapshot.v1.deck.exhaust_payoffs, 0);
}

#[test]
fn status_package_candidate_delta_uses_generator_and_payoff_roles() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 13,
            attacks: 7,
            skills: 5,
            powers: 1,
            starter_strikes: 4,
            starter_defends: 3,
            status_generators: 1,
            total_attack_damage: 48,
            total_block: 25,
            ..Default::default()
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 1,
            max_fires: 2,
            first_fire_floor: Some(7),
            max_early_pressure: 2,
            need_heal: 0.2,
            avoid_damage: 0.3,
            ..Default::default()
        }),
        None,
    );

    let evolve = super::candidate_plan_delta_v2(
        StrategyCandidateFactsV1 {
            card: CardId::Evolve,
            damage_total: 0,
            weak: 0,
            strength_gain: 0,
            plan_effects: vec![StrategyPlanEffectV1::StatusPayoff],
        },
        &snapshot,
    );

    assert_eq!(
        snapshot.support(super::StrategyPackageIdV2::StatusPackage),
        StrategyPlanSupportV1::Plausible
    );
    assert_eq!(evolve.support, StrategyPlanSupportV1::Plausible);
    assert!(evolve.effects.contains(&StrategyPlanEffectV1::StatusPayoff));
}

#[test]
fn strategy_package_v2_reports_block_engine_missing_roles() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 13,
            attacks: 7,
            skills: 5,
            powers: 1,
            starter_strikes: 4,
            starter_defends: 3,
            block_retention_sources: 1,
            block_payoffs: 0,
            block_multipliers: 0,
            total_attack_damage: 48,
            total_block: 32,
            ..Default::default()
        },
        None,
        None,
    );

    let block_engine = snapshot
        .package(super::StrategyPackageIdV2::BlockEngine)
        .expect("block engine package");

    assert!(block_engine
        .missing_roles
        .contains(&StrategyPackageGapV2::BlockPayoff));
    assert!(block_engine
        .missing_roles
        .contains(&StrategyPackageGapV2::BlockMultiplier));
    assert!(!block_engine
        .missing_roles
        .contains(&StrategyPackageGapV2::BlockRetention));
}

#[test]
fn strategy_package_v2_reports_exhaust_and_status_package_missing_roles() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 13,
            attacks: 7,
            skills: 5,
            powers: 1,
            starter_strikes: 4,
            starter_defends: 3,
            exhaust_generators: 1,
            exhaust_payoffs: 0,
            status_generators: 0,
            status_payoffs: 1,
            total_attack_damage: 48,
            total_block: 25,
            ..Default::default()
        },
        None,
        None,
    );

    let exhaust = snapshot
        .package(super::StrategyPackageIdV2::ExhaustEngine)
        .expect("exhaust package");
    let status = snapshot
        .package(super::StrategyPackageIdV2::StatusPackage)
        .expect("status package");

    assert!(exhaust
        .missing_roles
        .contains(&StrategyPackageGapV2::Payoff));
    assert!(!exhaust
        .missing_roles
        .contains(&StrategyPackageGapV2::Generator));
    assert!(status
        .missing_roles
        .contains(&StrategyPackageGapV2::Generator));
    assert!(!status.missing_roles.contains(&StrategyPackageGapV2::Payoff));
}

#[test]
fn strategy_package_v2_reports_strength_scaling_missing_roles() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 13,
            attacks: 7,
            skills: 5,
            powers: 1,
            starter_strikes: 4,
            starter_defends: 3,
            strength_sources: 1,
            strength_payoffs: 0,
            total_attack_damage: 48,
            total_block: 25,
            ..Default::default()
        },
        None,
        None,
    );
    let strength = snapshot
        .package(super::StrategyPackageIdV2::StrengthScaling)
        .expect("strength package");

    assert!(strength
        .missing_roles
        .contains(&StrategyPackageGapV2::Payoff));
    assert!(!strength
        .missing_roles
        .contains(&StrategyPackageGapV2::Generator));

    let payoff_only = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 13,
            attacks: 7,
            skills: 5,
            powers: 1,
            starter_strikes: 4,
            starter_defends: 3,
            strength_sources: 0,
            strength_payoffs: 1,
            total_attack_damage: 48,
            total_block: 25,
            ..Default::default()
        },
        None,
        None,
    );
    let payoff_strength = payoff_only
        .package(super::StrategyPackageIdV2::StrengthScaling)
        .expect("strength package");
    assert!(payoff_strength
        .missing_roles
        .contains(&StrategyPackageGapV2::Generator));
    assert!(!payoff_strength
        .missing_roles
        .contains(&StrategyPackageGapV2::Payoff));
}

#[test]
fn strategy_package_v2_reports_upgrade_sink_missing_route_budget() {
    let no_budget = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 13,
            attacks: 7,
            skills: 5,
            powers: 1,
            starter_strikes: 4,
            starter_defends: 3,
            route_upgrade_payoffs: 1,
            important_cards_unupgraded: 1,
            total_attack_damage: 48,
            total_block: 25,
            ..Default::default()
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 1,
            max_fires: 2,
            first_fire_floor: Some(6),
            max_early_pressure: 3,
            need_heal: 0.4,
            avoid_damage: 0.5,
            ..Default::default()
        }),
        None,
    );
    let upgrade = no_budget
        .package(super::StrategyPackageIdV2::UpgradeSink)
        .expect("upgrade package");

    assert!(upgrade
        .missing_roles
        .contains(&StrategyPackageGapV2::UpgradeBudget));
    assert!(!upgrade
        .missing_roles
        .contains(&StrategyPackageGapV2::UpgradeConsumer));
}

#[test]
fn strategy_package_v2_reports_weak_control_missing_source() {
    let snapshot = super::build_run_strategy_snapshot_v2(
        StrategyDeckFactsV1 {
            deck_size: 13,
            attacks: 7,
            skills: 5,
            powers: 1,
            starter_strikes: 4,
            starter_defends: 3,
            weak_sources: 0,
            total_attack_damage: 48,
            total_block: 25,
            ..Default::default()
        },
        Some(StrategyRouteFutureV1 {
            min_fires: 1,
            max_fires: 2,
            first_fire_floor: Some(6),
            max_early_pressure: 3,
            need_heal: 0.2,
            avoid_damage: 0.4,
            ..Default::default()
        }),
        None,
    );
    let weak_control = snapshot
        .package(super::StrategyPackageIdV2::WeakControl)
        .expect("weak control package");

    assert_eq!(weak_control.support, StrategyPlanSupportV1::Strong);
    assert!(weak_control
        .missing_roles
        .contains(&StrategyPackageGapV2::Generator));
}

#[test]
fn run_strategy_snapshot_v2_exposes_boss_threat_tags() {
    let mut run_state = RunState::new(521, 0, false, "Ironclad");
    run_state.act_num = 2;
    run_state.floor_num = 20;
    run_state.boss_key = Some(EncounterId::TheChamp);

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert_eq!(snapshot.threats.boss.as_deref(), Some("TheChamp"));
    assert!(snapshot
        .threats
        .tags
        .contains(&StrategyThreatTagV1::StrengthDebuffValuable));
    assert!(snapshot
        .threats
        .tags
        .contains(&StrategyThreatTagV1::HighIncomingDamage));
    assert!(snapshot
        .threats
        .evidence
        .iter()
        .any(|entry| entry.contains("Champ")));
    assert!(snapshot.threats.sources.iter().any(|source| {
        source.tag == StrategyThreatTagV1::StrengthDebuffValuable
            && source.source == StrategyThreatSourceV1::ActBoss
            && source.subject == "TheChamp"
    }));
    assert!(snapshot.threats.sources.iter().any(|source| {
        source.tag == StrategyThreatTagV1::MultiHit
            && source.source == StrategyThreatSourceV1::ActElitePool
            && source.subject == "Act2ElitePool"
    }));
}

#[test]
fn run_strategy_snapshot_v2_exposes_collector_minion_control_threat() {
    let mut run_state = RunState::new(522, 0, false, "Ironclad");
    run_state.act_num = 2;
    run_state.floor_num = 21;
    run_state.boss_key = Some(EncounterId::Collector);

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert!(snapshot
        .threats
        .tags
        .contains(&StrategyThreatTagV1::AoEValuable));
    assert!(snapshot.threats.sources.iter().any(|source| {
        source.tag == StrategyThreatTagV1::AoEValuable
            && source.source == StrategyThreatSourceV1::ActBoss
            && source.subject == "Collector"
    }));
}

#[test]
fn run_strategy_snapshot_v2_expands_elite_pool_into_specific_encounter_threats() {
    let mut run_state = RunState::new(521, 0, false, "Ironclad");
    run_state.act_num = 1;
    run_state.floor_num = 5;
    run_state.boss_key = Some(EncounterId::TheGuardian);

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert!(snapshot.threats.sources.iter().any(|source| {
        source.tag == StrategyThreatTagV1::SkillPunish
            && source.source == StrategyThreatSourceV1::ActEliteEncounter
            && source.subject == "GremlinNob"
    }));
    assert!(snapshot.threats.sources.iter().any(|source| {
        source.tag == StrategyThreatTagV1::SetupWindow
            && source.source == StrategyThreatSourceV1::ActEliteEncounter
            && source.subject == "Lagavulin"
    }));
    assert!(snapshot.threats.sources.iter().any(|source| {
        source.tag == StrategyThreatTagV1::StatusFlood
            && source.source == StrategyThreatSourceV1::ActEliteEncounter
            && source.subject == "ThreeSentries"
    }));
}

#[test]
fn later_act_threat_profiles_never_reintroduce_cleared_monster_pools() {
    for (act, boss) in [(2, EncounterId::TheChamp), (3, EncounterId::AwakenedOne)] {
        let mut run_state = RunState::new(523 + u64::from(act), 0, false, "Ironclad");
        run_state.act_num = act;
        run_state.boss_key = Some(boss);

        let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

        assert_eq!(snapshot.threats.current_act, act);
        assert!(snapshot
            .threats
            .sources
            .iter()
            .all(|source| source.source_act >= act));
        assert!(!snapshot.threats.sources.iter().any(|source| matches!(
            source.subject.as_str(),
            "GremlinNob" | "Lagavulin" | "ThreeSentries" | "SlimeBoss"
        )));
        if act >= 3 {
            assert!(!snapshot.threats.sources.iter().any(|source| matches!(
                source.subject.as_str(),
                "Slavers" | "GremlinLeader" | "BookOfStabbing" | "TheChamp"
            )));
        }
    }
}

#[test]
fn stale_past_act_boss_key_is_not_admitted_as_a_current_threat() {
    let mut run_state = RunState::new(526, 0, false, "Ironclad");
    run_state.act_num = 2;
    run_state.boss_key = Some(EncounterId::SlimeBoss);

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert_eq!(snapshot.threats.current_act, 2);
    assert_eq!(snapshot.threats.boss, None);
    assert!(!snapshot
        .threats
        .sources
        .iter()
        .any(|source| source.subject == "SlimeBoss"));
}

#[test]
fn cleared_current_act_combat_pools_do_not_leak_past_the_visible_map_horizon() {
    let mut run_state = RunState::new(527, 0, false, "Ironclad");
    run_state.act_num = 1;
    run_state.floor_num = 13;
    run_state.boss_key = Some(EncounterId::SlimeBoss);
    run_state.map = late_act_map_v1(RoomType::RestRoom);

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert!(snapshot
        .threats
        .sources
        .iter()
        .any(|source| source.source == StrategyThreatSourceV1::ActBoss));
    assert!(!snapshot.threats.sources.iter().any(|source| matches!(
        source.source,
        StrategyThreatSourceV1::ActElitePool
            | StrategyThreatSourceV1::ActEliteEncounter
            | StrategyThreatSourceV1::ActHallwayPool
    )));
}

#[test]
fn visible_future_elite_keeps_the_current_act_elite_pool_live() {
    let mut run_state = RunState::new(528, 0, false, "Ironclad");
    run_state.act_num = 1;
    run_state.floor_num = 12;
    run_state.boss_key = Some(EncounterId::SlimeBoss);
    run_state.map = late_act_map_v1(RoomType::MonsterRoomElite);

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert!(snapshot.threats.sources.iter().any(|source| {
        source.source == StrategyThreatSourceV1::ActEliteEncounter && source.subject == "GremlinNob"
    }));
    assert!(!snapshot
        .threats
        .sources
        .iter()
        .any(|source| source.source == StrategyThreatSourceV1::ActHallwayPool));
}

#[test]
fn unknown_event_branch_does_not_become_positive_elite_evidence() {
    let mut run_state = RunState::new(529, 0, false, "Ironclad");
    run_state.act_num = 1;
    run_state.floor_num = 13;
    run_state.boss_key = Some(EncounterId::SlimeBoss);
    run_state.map = late_act_monster_or_event_map_v1();

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert!(snapshot
        .threats
        .sources
        .iter()
        .any(|source| source.source == StrategyThreatSourceV1::ActHallwayPool));
    assert!(!snapshot.threats.sources.iter().any(|source| matches!(
        source.source,
        StrategyThreatSourceV1::ActElitePool | StrategyThreatSourceV1::ActEliteEncounter
    )));
}

#[test]
fn post_boss_reward_closes_every_cleared_act_threat_source() {
    let mut run_state = RunState::new(530, 0, false, "Ironclad");
    run_state.act_num = 1;
    run_state.floor_num = 16;
    run_state.boss_key = Some(EncounterId::SlimeBoss);
    run_state.pending_boss_reward = true;
    run_state.map.current_x = 0;
    run_state.map.current_y = 15;

    let snapshot = super::build_run_strategy_snapshot_from_run_state_v2(&run_state);

    assert_eq!(snapshot.threats.boss, None);
    assert!(snapshot.threats.sources.is_empty());
    assert!(snapshot.threat_coverage.gaps.is_empty());
}

fn late_act_map_v1(next_room: RoomType) -> MapState {
    let mut graph = vec![Vec::new(); 15];
    let mut current = MapRoomNode::new(0, 12);
    current.class = Some(RoomType::MonsterRoom);
    current.edges.insert(MapEdge::new(0, 12, 0, 13));
    let mut next = MapRoomNode::new(0, 13);
    next.class = Some(next_room);
    next.edges.insert(MapEdge::new(0, 13, 0, 14));
    let mut final_fire = MapRoomNode::new(0, 14);
    final_fire.class = Some(RoomType::RestRoom);
    graph[12] = vec![current];
    graph[13] = vec![next];
    graph[14] = vec![final_fire];
    let mut map = MapState::new(graph);
    map.current_x = 0;
    map.current_y = 12;
    map
}

fn late_act_monster_or_event_map_v1() -> MapState {
    let mut graph = vec![Vec::new(); 15];
    let mut current = MapRoomNode::new(0, 12);
    current.class = Some(RoomType::MonsterRoom);
    current.edges.insert(MapEdge::new(0, 12, 0, 13));
    current.edges.insert(MapEdge::new(0, 12, 1, 13));
    let mut monster = MapRoomNode::new(0, 13);
    monster.class = Some(RoomType::MonsterRoom);
    monster.edges.insert(MapEdge::new(0, 13, 0, 14));
    let mut event = MapRoomNode::new(1, 13);
    event.class = Some(RoomType::EventRoom);
    event.edges.insert(MapEdge::new(1, 13, 1, 14));
    let mut fire_left = MapRoomNode::new(0, 14);
    fire_left.class = Some(RoomType::RestRoom);
    let mut fire_right = MapRoomNode::new(1, 14);
    fire_right.class = Some(RoomType::RestRoom);
    graph[12] = vec![current];
    graph[13] = vec![monster, event];
    graph[14] = vec![fire_left, fire_right];
    let mut map = MapState::new(graph);
    map.current_x = 0;
    map.current_y = 12;
    map
}
