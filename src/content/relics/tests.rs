use super::*;
use crate::content::cards::{evaluate_card_for_play, CardId};
use crate::content::monsters::EnemyId;
use crate::content::powers::{store, PowerId};
use crate::runtime::action::{Action, AddTo, DamageInfo, DamageType, NO_SOURCE};
use crate::runtime::combat::{CombatCard, OrbEntity, OrbId, Power, StanceId};
use crate::state::events::EventId;
use crate::state::selection::{DomainEvent, DomainEventSource};

fn drain_test_actions(state: &mut crate::runtime::combat::CombatState) {
    while let Some(action) = state.pop_next_action() {
        crate::engine::action_handlers::execute_action(action, state);
    }
}

#[test]
fn ironclad_blood_skull_and_frog_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::BurningBlood), RelicTier::Starter);
    assert_eq!(get_relic_tier(RelicId::BlackBlood), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::RedSkull), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::PaperFrog), RelicTier::Uncommon);

    assert!(get_relic_subscriptions(RelicId::BurningBlood).on_victory);
    assert!(get_relic_subscriptions(RelicId::BlackBlood).on_victory);
    assert!(get_relic_subscriptions(RelicId::RedSkull).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::RedSkull).on_victory);
    assert!(get_relic_subscriptions(RelicId::PaperFrog).on_calculate_vulnerable_multiplier);
}

#[test]
fn burning_and_black_blood_victory_heal_matches_java_current_hp_guard() {
    let mut burning_state = crate::test_support::blank_test_combat();
    burning_state.entities.player.current_hp = 10;
    burning_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::BurningBlood));
    let burning_actions = hooks::on_victory(&mut burning_state);
    assert_eq!(burning_actions.len(), 1);
    assert_eq!(burning_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        burning_actions[0].action,
        Action::Heal {
            target: 0,
            amount: 6
        }
    ));

    burning_state.entities.player.current_hp = 0;
    assert!(hooks::on_victory(&mut burning_state).is_empty());

    let mut black_state = crate::test_support::blank_test_combat();
    black_state.entities.player.current_hp = 10;
    black_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::BlackBlood));
    let black_actions = hooks::on_victory(&mut black_state);
    assert_eq!(black_actions.len(), 1);
    assert_eq!(black_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        black_actions[0].action,
        Action::Heal {
            target: 0,
            amount: 12
        }
    ));

    black_state.entities.player.current_hp = 0;
    assert!(hooks::on_victory(&mut black_state).is_empty());
}

#[test]
fn red_skull_threshold_hooks_match_java_bloodied_edges() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.current_hp = 30;
    state.entities.player.max_hp = 60;
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RedSkull));

    let start_actions = hooks::at_battle_start(&mut state);
    assert_eq!(start_actions.len(), 1);
    assert_eq!(start_actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        start_actions[0].action,
        Action::RedSkullBattleStartCheck
    ));

    let mut relic = RelicState::new(RelicId::RedSkull);
    assert!(matches!(
        red_skull::at_battle_start(&mut relic)[0].action,
        Action::RedSkullBattleStartCheck
    ));
    assert!(matches!(
        red_skull::on_player_hp_changed(&mut state, 31, 30, 60)[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 3
        }
    ));
    assert!(matches!(
        red_skull::on_player_hp_changed(&mut state, 30, 31, 60)[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: -3
        }
    ));
    assert!(red_skull::on_player_hp_changed(&mut state, 30, 20, 60).is_empty());
}

#[test]
fn red_skull_battle_start_check_uses_execution_time_hp_like_java() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.current_hp = 29;
    state.entities.player.max_hp = 60;
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::BloodVial));
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RedSkull));

    let actions = hooks::at_battle_start(&mut state);
    state.queue_actions(actions);
    drain_test_actions(&mut state);

    assert_eq!(
        state.entities.player.current_hp, 31,
        "Java Blood Vial addToTop heal resolves before Red Skull's addToBot battle-start check"
    );
    assert_eq!(
        store::power_amount(&state, 0, PowerId::Strength),
        0,
        "Java Red Skull checks isBloodied when its queued battle-start action executes, not when atBattleStart() is called"
    );
}

#[test]
fn red_skull_tracks_private_active_flag_for_battle_start_and_heal_edges() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.current_hp = 30;
    state.entities.player.max_hp = 60;
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RedSkull));

    let actions = hooks::at_battle_start(&mut state);
    state.queue_actions(actions);
    drain_test_actions(&mut state);

    assert_eq!(store::power_amount(&state, 0, PowerId::Strength), 3);
    assert!(
        state
            .entities
            .player
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::RedSkull)
            .is_some_and(|relic| relic.used_up),
        "Rust stores Java RedSkull.isActive in RelicState.used_up"
    );

    crate::engine::action_handlers::damage::handle_heal(0, 1, &mut state);
    drain_test_actions(&mut state);

    assert_eq!(state.entities.player.current_hp, 31);
    assert_eq!(
        store::power_amount(&state, 0, PowerId::Strength),
        0,
        "Java Red Skull onNotBloodied removes Strength only after it was active"
    );
    assert!(state
        .entities
        .player
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::RedSkull)
        .is_some_and(|relic| !relic.used_up));
}

#[test]
fn paper_frog_vulnerable_multiplier_applies_only_to_enemy_targets() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::PaperFrog));
    assert_eq!(
        hooks::on_calculate_vulnerable_multiplier(&state, false),
        1.75
    );
    assert_eq!(hooks::on_calculate_vulnerable_multiplier(&state, true), 1.5);

    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 801;
    state.entities.monsters = vec![monster];
    store::set_powers_for(
        &mut state,
        801,
        vec![Power {
            power_type: PowerId::Vulnerable,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    let strike = CombatCard::for_scenario(CardId::Strike, 1);
    let evaluated = evaluate_card_for_play(&strike, &state, Some(801));
    assert_eq!(evaluated.base_damage_mut, 10);
}

#[test]
fn ironclad_brimstone_belt_ashes_flower_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Brimstone), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::ChampionBelt), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::CharonsAshes), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::MagicFlower), RelicTier::Rare);

    assert!(get_relic_subscriptions(RelicId::Brimstone).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::ChampionBelt).on_apply_power);
    assert!(get_relic_subscriptions(RelicId::CharonsAshes).on_exhaust);
    assert!(get_relic_subscriptions(RelicId::MagicFlower).on_calculate_heal);
}

#[test]
fn brimstone_turn_start_matches_java_strength_sources_and_top_order() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 901;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 902;
    state.entities.monsters = vec![first, second];

    let actions = brimstone::Brimstone::at_turn_start(&state);
    assert_eq!(actions.len(), 3);

    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 2
        }
    ));

    assert_eq!(actions[1].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[1].action,
        Action::ApplyPower {
            source: 901,
            target: 901,
            power_id: PowerId::Strength,
            amount: 1
        }
    ));

    assert_eq!(actions[2].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[2].action,
        Action::ApplyPower {
            source: 902,
            target: 902,
            power_id: PowerId::Strength,
            amount: 1
        }
    ));

    state.queue_actions(actions);
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 902,
            target: 902,
            power_id: PowerId::Strength,
            amount: 1
        })
    ));
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 901,
            target: 901,
            power_id: PowerId::Strength,
            amount: 1
        })
    ));
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 2
        })
    ));
}

#[test]
fn mutagenic_strength_battle_start_matches_java_add_to_top_order() {
    assert_eq!(
        get_relic_tier(RelicId::MutagenicStrength),
        RelicTier::Special
    );
    assert!(get_relic_subscriptions(RelicId::MutagenicStrength).at_battle_start);

    let mut state = crate::test_support::blank_test_combat();
    let actions = mutagenic_strength::at_battle_start();
    assert_eq!(actions.len(), 2);
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 3
        }
    ));
    assert!(matches!(
        actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::LoseStrength,
            amount: 3
        }
    ));

    state.queue_actions(actions);
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::LoseStrength,
            amount: 3
        })
    ));
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 3
        })
    ));
}

#[test]
fn champion_belt_respects_java_player_source_and_artifact_guard() {
    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 901;
    state.entities.monsters = vec![target];
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::ChampionBelt));

    let actions = hooks::on_apply_power(&mut state, 0, PowerId::Vulnerable, 901);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 901,
            power_id: PowerId::Weak,
            amount: 1
        }
    ));

    assert!(hooks::on_apply_power(&mut state, 901, PowerId::Vulnerable, 0).is_empty());
    assert!(hooks::on_apply_power(&mut state, 901, PowerId::Vulnerable, 902).is_empty());
    assert!(hooks::on_apply_power(&mut state, 0, PowerId::Weak, 901).is_empty());

    store::set_powers_for(
        &mut state,
        901,
        vec![Power {
            power_type: PowerId::Artifact,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    assert!(hooks::on_apply_power(&mut state, 0, PowerId::Vulnerable, 901).is_empty());
}

#[test]
fn charons_ashes_exhaust_damage_matches_java_thorns_null_source() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.monsters = vec![
        crate::test_support::test_monster(EnemyId::JawWorm),
        crate::test_support::test_monster(EnemyId::Cultist),
    ];

    let actions = charons_ashes::CharonsAshes::on_exhaust(&state);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    match &actions[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => {
            assert_eq!(*source, NO_SOURCE);
            assert_eq!(damages.as_slice(), &[3, 3]);
            assert_eq!(*damage_type, DamageType::Thorns);
            assert!(!*is_modified);
        }
        other => panic!("Charon's Ashes should emit DamageAllEnemiesAction, got {other:?}"),
    }
}

#[test]
fn magic_flower_combat_heal_rounding_matches_java_mathutils_round() {
    assert_eq!(magic_flower::modify_heal(1), 2);
    assert_eq!(magic_flower::modify_heal(2), 3);
    assert_eq!(magic_flower::modify_heal(5), 8);

    let mut state = crate::test_support::blank_test_combat();
    assert_eq!(hooks::on_calculate_heal(&state, 5), 5);

    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::MagicFlower));
    assert_eq!(hooks::on_calculate_heal(&state, 5), 8);
}

#[test]
fn ironclad_pain_cube_clay_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::MarkOfPain), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::RunicCube), RelicTier::Boss);
    assert_eq!(
        get_relic_tier(RelicId::SelfFormingClay),
        RelicTier::Uncommon
    );

    assert!(get_relic_subscriptions(RelicId::MarkOfPain).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::RunicCube).on_lose_hp);
    assert!(get_relic_subscriptions(RelicId::SelfFormingClay).on_lose_hp);
    assert_eq!(energy_master_delta(RelicId::MarkOfPain), 1);
    assert_eq!(energy_master_delta(RelicId::RunicCube), 0);
    assert_eq!(energy_master_delta(RelicId::SelfFormingClay), 0);
}

#[test]
fn mark_of_pain_battle_start_matches_java_wound_generation_and_energy() {
    let actions = mark_of_pain::at_battle_start();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::MakeTempCardInDrawPile {
            card_id: CardId::Wound,
            amount: 2,
            random_spot: true,
            to_bottom: false,
            upgraded: false,
        }
    ));

    let mut state = crate::test_support::blank_test_combat();
    assert_eq!(state.entities.player.energy_master, 3);
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::MarkOfPain));
    assert_eq!(state.entities.player.energy_master, 4);
}

#[test]
fn runic_cube_hp_loss_hook_matches_java_positive_damage_guard() {
    assert!(runic_cube::was_hp_lost(0).is_empty());
    assert!(runic_cube::was_hp_lost(-1).is_empty());

    let actions = runic_cube::was_hp_lost(3);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(actions[0].action, Action::DrawCards(1)));
}

#[test]
fn self_forming_clay_hp_loss_hook_matches_java_positive_damage_guard() {
    assert!(self_forming_clay::on_lose_hp(0).is_empty());
    assert!(self_forming_clay::on_lose_hp(-1).is_empty());

    let actions = self_forming_clay::on_lose_hp(3);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::NextTurnBlock,
            amount: 3
        }
    ));
}

#[test]
fn shared_common_battle_start_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Akabeko), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::Anchor), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::BagOfMarbles), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::BagOfPreparation), RelicTier::Common);

    assert!(get_relic_subscriptions(RelicId::Akabeko).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::Anchor).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::BagOfMarbles).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::BagOfPreparation).at_battle_start);
}

#[test]
fn akabeko_anchor_and_bag_of_preparation_battle_start_actions_match_java_sources() {
    let akabeko_actions = akabeko::Akabeko::at_battle_start();
    assert_eq!(akabeko_actions.len(), 1);
    assert_eq!(akabeko_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        akabeko_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Vigor,
            amount: 8
        }
    ));

    let anchor_actions = anchor::Anchor::at_battle_start();
    assert_eq!(anchor_actions.len(), 1);
    assert_eq!(anchor_actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        anchor_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 10
        }
    ));

    let bag_actions = bag_of_preparation::BagOfPreparation::at_battle_start();
    assert_eq!(bag_actions.len(), 1);
    assert_eq!(bag_actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(bag_actions[0].action, Action::DrawCards(2)));
}

#[test]
fn bag_of_marbles_queues_vulnerable_for_every_current_monster() {
    let mut state = crate::test_support::blank_test_combat();
    let mut alive = crate::test_support::test_monster(EnemyId::JawWorm);
    alive.id = 901;
    let mut dying = crate::test_support::test_monster(EnemyId::Cultist);
    dying.id = 902;
    dying.is_dying = true;
    let mut escaped = crate::test_support::test_monster(EnemyId::AcidSlimeM);
    escaped.id = 903;
    escaped.is_escaped = true;
    state.entities.monsters = vec![alive, dying, escaped];

    let actions = bag_of_marbles::BagOfMarbles::at_battle_start(&state);
    assert_eq!(actions.len(), 3);
    for (action, target) in actions.iter().zip([901, 902, 903]) {
        assert_eq!(action.insertion_mode, AddTo::Bottom);
        assert!(matches!(
            action.action,
            Action::ApplyPower {
                source: 0,
                target: actual_target,
                power_id: PowerId::Vulnerable,
                amount: 1
            } if actual_target == target
        ));
    }
}

#[test]
fn shared_common_hp_and_thorns_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::BloodVial), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::BronzeScales), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::CentennialPuzzle), RelicTier::Common);

    assert!(get_relic_subscriptions(RelicId::BloodVial).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::BronzeScales).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::CentennialPuzzle).at_pre_battle);
    assert!(get_relic_subscriptions(RelicId::CentennialPuzzle).on_lose_hp);
}

#[test]
fn blood_vial_and_bronze_scales_battle_start_actions_match_java_sources() {
    let blood_actions = blood_vial::BloodVial::at_battle_start();
    assert_eq!(blood_actions.len(), 1);
    assert_eq!(blood_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        blood_actions[0].action,
        Action::Heal {
            target: 0,
            amount: 2
        }
    ));

    let scale_actions = bronze_scales::BronzeScales::at_battle_start(0);
    assert_eq!(scale_actions.len(), 1);
    assert_eq!(scale_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        scale_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Thorns,
            amount: 3
        }
    ));
}

#[test]
fn centennial_puzzle_marks_used_immediately_and_resets_pre_battle() {
    let mut relic_state = RelicState::new(RelicId::CentennialPuzzle);
    assert!(centennial_puzzle::CentennialPuzzle::on_lose_hp(&mut relic_state, 0).is_empty());
    assert!(!relic_state.used_up);

    let actions = centennial_puzzle::CentennialPuzzle::on_lose_hp(&mut relic_state, 4);
    assert!(relic_state.used_up);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(actions[0].action, Action::DrawCards(3)));
    assert!(centennial_puzzle::CentennialPuzzle::on_lose_hp(&mut relic_state, 4).is_empty());

    let reset_actions = centennial_puzzle::CentennialPuzzle::at_pre_battle(&mut relic_state);
    assert!(reset_actions.is_empty());
    assert!(!relic_state.used_up);
}

#[test]
fn centennial_puzzle_hook_updates_relic_state_before_draw_action_executes() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::CentennialPuzzle));

    let actions = hooks::on_lose_hp(&mut state, 3);
    assert!(state.entities.player.relics[0].used_up);
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0].action, Action::DrawCards(3)));

    assert!(hooks::on_lose_hp(&mut state, 3).is_empty());
}

#[test]
fn shared_common_counter_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::HappyFlower), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::Lantern), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::Nunchaku), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::PenNib), RelicTier::Common);

    assert!(get_relic_subscriptions(RelicId::HappyFlower).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::Lantern).at_pre_battle);
    assert!(get_relic_subscriptions(RelicId::Lantern).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::Nunchaku).on_use_card);
    assert!(get_relic_subscriptions(RelicId::PenNib).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::PenNib).on_use_card);
}

#[test]
fn happy_flower_counter_updates_immediately_like_java() {
    let mut relic = RelicState::new(RelicId::HappyFlower);

    assert!(happy_flower::HappyFlower::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, 1);
    assert!(happy_flower::HappyFlower::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, 2);

    let actions = happy_flower::HappyFlower::at_turn_start(&mut relic);
    assert_eq!(relic.counter, 0);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::GainEnergy { amount: 1 }
    ));

    relic.counter = -1;
    assert!(happy_flower::HappyFlower::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, 1);
}

#[test]
fn lantern_first_turn_state_updates_immediately_like_java() {
    let mut relic = RelicState::new(RelicId::Lantern);
    relic.used_up = true;

    let pre_battle_actions = lantern::at_pre_battle(&mut relic);
    assert!(pre_battle_actions.is_empty());
    assert!(!relic.used_up);

    let first_turn_actions = lantern::at_turn_start(&mut relic);
    assert!(relic.used_up);
    assert_eq!(first_turn_actions.len(), 1);
    assert_eq!(first_turn_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        first_turn_actions[0].action,
        Action::GainEnergy { amount: 1 }
    ));

    assert!(lantern::at_turn_start(&mut relic).is_empty());
}

#[test]
fn nunchaku_counter_updates_immediately_like_java() {
    let mut relic = RelicState::new(RelicId::Nunchaku);
    relic.counter = 8;

    assert!(nunchaku::on_use_card(&mut relic).is_empty());
    assert_eq!(relic.counter, 9);

    let actions = nunchaku::on_use_card(&mut relic);
    assert_eq!(relic.counter, 0);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::GainEnergy { amount: 1 }
    ));

    relic.counter = -1;
    let actions = nunchaku::on_use_card(&mut relic);
    assert_eq!(
        relic.counter, 0,
        "Java ++counter from -1 reaches 0 and then the modulo condition fires"
    );
    assert_eq!(actions.len(), 1);
    assert!(matches!(
        actions[0].action,
        Action::GainEnergy { amount: 1 }
    ));
}

#[test]
fn pen_nib_counter_and_power_timing_match_java() {
    let mut relic = RelicState::new(RelicId::PenNib);
    relic.counter = 8;

    let ninth_actions = pen_nib::on_use_card(&mut relic);
    assert_eq!(relic.counter, 9);
    assert_eq!(ninth_actions.len(), 1);
    assert_eq!(ninth_actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        ninth_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::PenNibPower,
            amount: 1
        }
    ));

    let tenth_actions = pen_nib::on_use_card(&mut relic);
    assert_eq!(relic.counter, 0);
    assert!(tenth_actions.is_empty());

    let battle_start_actions = pen_nib::at_battle_start(9);
    assert_eq!(battle_start_actions.len(), 1);
    assert!(matches!(
        battle_start_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::PenNibPower,
            amount: 1
        }
    ));
    assert!(pen_nib::at_battle_start(8).is_empty());

    relic.counter = -1;
    assert!(pen_nib::on_use_card(&mut relic).is_empty());
    assert_eq!(
        relic.counter, 0,
        "Java ++counter from -1 reaches 0 without adding Pen Nib power"
    );
}

#[test]
fn shared_common_turn_state_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::AncientTeaSet), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::ArtOfWar), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::Orichalcum), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::OddlySmoothStone), RelicTier::Common);

    assert!(get_relic_subscriptions(RelicId::AncientTeaSet).at_pre_battle);
    assert!(get_relic_subscriptions(RelicId::AncientTeaSet).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::AncientTeaSet).on_enter_rest_room);
    assert!(get_relic_subscriptions(RelicId::ArtOfWar).at_pre_battle);
    assert!(get_relic_subscriptions(RelicId::ArtOfWar).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::ArtOfWar).on_use_card);
    assert!(get_relic_subscriptions(RelicId::Orichalcum).at_end_of_turn);
    assert!(get_relic_subscriptions(RelicId::OddlySmoothStone).at_battle_start);
}

#[test]
fn ancient_tea_set_first_turn_state_matches_java() {
    let mut relic = RelicState::new(RelicId::AncientTeaSet);
    ancient_tea_set::AncientTeaSet::on_enter_rest_room(&mut relic);
    assert_eq!(relic.counter, -2);

    assert!(ancient_tea_set::AncientTeaSet::at_pre_battle(&mut relic).is_empty());
    assert!(!relic.used_up);

    let actions = ancient_tea_set::AncientTeaSet::at_turn_start(&mut relic);
    assert!(relic.used_up);
    assert_eq!(relic.counter, -1);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[0].action,
        Action::GainEnergy { amount: 2 }
    ));

    assert!(ancient_tea_set::AncientTeaSet::at_turn_start(&mut relic).is_empty());
}

#[test]
fn art_of_war_turn_and_attack_state_matches_java() {
    let state = crate::test_support::blank_test_combat();
    let mut relic = RelicState::new(RelicId::ArtOfWar);

    assert!(art_of_war::at_pre_battle(&mut relic).is_empty());
    assert_eq!(relic.counter, -1);

    assert!(art_of_war::at_turn_start(&state, &mut relic).is_empty());
    assert_eq!(relic.counter, 1);

    assert!(art_of_war::on_use_card(&state, &mut relic, CardId::Defend).is_empty());
    assert_eq!(relic.counter, 1);
    assert!(art_of_war::on_use_card(&state, &mut relic, CardId::Strike).is_empty());
    assert_eq!(relic.counter, 0);

    assert!(art_of_war::at_turn_start(&state, &mut relic).is_empty());
    assert_eq!(relic.counter, 1);

    let actions = art_of_war::at_turn_start(&state, &mut relic);
    assert_eq!(relic.counter, 1);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::GainEnergy { amount: 1 }
    ));
}

#[test]
fn orichalcum_and_smooth_stone_actions_match_java_sources() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.block = 0;
    let orichalcum_actions = orichalcum::at_end_of_turn(&state);
    assert_eq!(orichalcum_actions.len(), 1);
    assert_eq!(orichalcum_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        orichalcum_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 6
        }
    ));

    state.entities.player.block = 1;
    assert!(orichalcum::at_end_of_turn(&state).is_empty());

    let stone_actions = oddly_smooth_stone::at_battle_start();
    assert_eq!(stone_actions.len(), 1);
    assert_eq!(stone_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        stone_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Dexterity,
            amount: 1
        }
    ));
}

#[test]
fn shared_common_damage_hp_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Boot), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::PreservedInsect), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::Vajra), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::Strawberry), RelicTier::Common);

    assert!(!get_relic_subscriptions(RelicId::Boot).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::PreservedInsect).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::Vajra).at_battle_start);
    assert_eq!(energy_master_delta(RelicId::Strawberry), 0);
}

#[test]
fn boot_damage_floor_applies_only_to_positive_normal_player_damage() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Boot));
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 901;
    monster.current_hp = 20;
    monster.max_hp = 20;
    state.entities.monsters = vec![monster];

    crate::engine::action_handlers::damage::handle_damage(
        DamageInfo {
            source: 0,
            target: 901,
            base: 3,
            output: 3,
            damage_type: DamageType::Normal,
            is_modified: false,
        },
        &mut state,
    );
    assert_eq!(state.entities.monsters[0].current_hp, 15);

    state.entities.monsters[0].current_hp = 20;
    crate::engine::action_handlers::damage::handle_damage(
        DamageInfo {
            source: NO_SOURCE,
            target: 901,
            base: 3,
            output: 3,
            damage_type: DamageType::Thorns,
            is_modified: false,
        },
        &mut state,
    );
    assert_eq!(state.entities.monsters[0].current_hp, 17);
}

#[test]
fn preserved_insect_uses_elite_room_flag_and_reduces_current_hp_only() {
    let mut state = crate::test_support::blank_test_combat();
    state.meta.is_elite_fight = true;
    let mut first = crate::test_support::test_monster(EnemyId::Cultist);
    first.id = 901;
    first.current_hp = 100;
    first.max_hp = 100;
    let mut second = crate::test_support::test_monster(EnemyId::JawWorm);
    second.id = 902;
    second.current_hp = 20;
    second.max_hp = 100;
    state.entities.monsters = vec![first, second];

    let actions = preserved_insect::at_battle_start(&mut state);
    assert!(actions.is_empty());
    assert_eq!(state.entities.monsters[0].current_hp, 75);
    assert_eq!(state.entities.monsters[0].max_hp, 100);
    assert_eq!(
        state.entities.monsters[1].current_hp, 20,
        "Java does not heal monsters that are already below the 75% threshold"
    );
    assert_eq!(state.entities.monsters[1].max_hp, 100);

    state.meta.is_elite_fight = false;
    state.entities.monsters[0].current_hp = 100;
    assert!(preserved_insect::at_battle_start(&mut state).is_empty());
    assert_eq!(state.entities.monsters[0].current_hp, 100);
}

#[test]
fn vajra_and_strawberry_match_java_sources() {
    let vajra_actions = vajra::at_battle_start();
    assert_eq!(vajra_actions.len(), 1);
    assert_eq!(vajra_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        vajra_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 1
        }
    ));

    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.current_hp = 50;
    run_state.max_hp = 80;
    assert!(strawberry::on_equip(&mut run_state).is_none());
    assert_eq!(run_state.max_hp, 87);
    assert_eq!(run_state.current_hp, 57);
}

#[test]
fn shared_common_run_gold_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::CeramicFish), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::DreamCatcher), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::JuzuBracelet), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::MawBank), RelicTier::Common);

    assert!(!get_relic_subscriptions(RelicId::CeramicFish).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::DreamCatcher).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::JuzuBracelet).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::MawBank).at_battle_start);
}

#[test]
fn ceramic_fish_obtain_card_gold_uses_java_gain_gold_semantics() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.gold = 10;
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::CeramicFish));

    let deck_len = run_state.master_deck.len();
    assert!(run_state.add_card_to_deck(CardId::Strike));
    assert_eq!(run_state.master_deck.len(), deck_len + 1);
    assert_eq!(run_state.gold, 19);

    let mut blocked = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    blocked.gold = 10;
    blocked.relics.clear();
    blocked.relics.push(RelicState::new(RelicId::CeramicFish));
    blocked.relics.push(RelicState::new(RelicId::Omamori));

    let blocked_len = blocked.master_deck.len();
    assert!(!blocked.add_card_to_deck(CardId::Regret));
    assert_eq!(blocked.master_deck.len(), blocked_len);
    assert_eq!(
        blocked.gold, 10,
        "Omamori prevents the curse obtain, so Ceramic Fish onObtainCard does not fire"
    );

    let mut ectoplasm = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    ectoplasm.gold = 10;
    ectoplasm.relics.clear();
    ectoplasm.relics.push(RelicState::new(RelicId::CeramicFish));
    ectoplasm.relics.push(RelicState::new(RelicId::Ectoplasm));

    assert!(ectoplasm.add_card_to_deck(CardId::Strike));
    assert_eq!(
        ectoplasm.gold, 10,
        "Java routes Ceramic Fish through AbstractPlayer.gainGold, which Ectoplasm blocks"
    );
}

#[test]
fn ectoplasm_blocks_run_combat_and_on_equip_gold_gain_paths() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.gold = 10;
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::Ectoplasm));
    assert_eq!(
        run_state.change_gold_with_source(50, DomainEventSource::Event(EventId::GoldenShrine)),
        0
    );
    assert_eq!(run_state.gold, 10);

    old_coin::on_equip(&mut run_state);
    assert_eq!(
        run_state.gold, 10,
        "Old Coin also routes through Java gainGold semantics when Ectoplasm is present"
    );

    let mut combat_state = crate::test_support::blank_test_combat();
    combat_state.entities.player.gold = 10;
    combat_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Ectoplasm));
    crate::engine::action_handlers::damage::handle_gain_gold(50, &mut combat_state);
    assert_eq!(combat_state.entities.player.gold, 10);
    assert_eq!(combat_state.entities.player.gold_delta_this_combat, 0);
}

#[test]
fn maw_bank_only_spending_in_shop_uses_it_up_like_java_lose_gold() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.gold = 100;
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::MawBank));

    run_state.change_gold_with_source(-25, DomainEventSource::Event(EventId::TheJoust));
    let maw_bank = run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::MawBank)
        .expect("MawBank should be present");
    assert!(!maw_bank.used_up);
    assert_eq!(maw_bank.counter, -1);

    run_state.change_gold_with_source(-25, DomainEventSource::Shop);
    let maw_bank = run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::MawBank)
        .expect("MawBank should be present");
    assert!(maw_bank.used_up);
    assert_eq!(maw_bank.counter, -2);
}

#[test]
fn juzu_bracelet_converts_monster_event_roll_without_preserving_monster_chance() {
    let mut generator = crate::state::events::generator::EventGenerator::new(1);
    generator.monster_chance = 1.0;
    generator.shop_chance = 0.0;
    generator.treasure_chance = 0.0;
    let mut rng = crate::runtime::rng::RngPool::new(1);
    let ctx = crate::state::events::context::EventContext {
        act_num: 1,
        ascension_level: 0,
        is_daily_run: false,
        highest_unlocked_ascension_level: 0,
        floor_num: 10,
        map_current_y: Some(10),
        map_height: 15,
        gold: 99,
        current_hp: 80,
        max_hp: 80,
        playtime_seconds: 0.0,
        has_curses: false,
        tiny_chest_counter: 0,
        has_golden_idol: false,
        has_juzu_bracelet: true,
        previous_room_was_shop: false,
        relic_count: 1,
    };

    assert_eq!(
        generator.roll_room_type(&mut rng, &ctx),
        crate::state::events::generator::RoomRoll::Event
    );
    assert_eq!(
        generator.monster_chance, 0.10,
        "Java resets MONSTER_CHANCE after a Juzu-converted monster roll"
    );
}

#[test]
fn shared_common_shop_rest_event_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::MealTicket), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::RegalPillow), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::SmilingMask), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::TinyChest), RelicTier::Common);

    assert!(!get_relic_subscriptions(RelicId::MealTicket).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::RegalPillow).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::SmilingMask).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::TinyChest).at_battle_start);
}

#[test]
fn tiny_chest_counter_forces_treasure_roll_every_fourth_unknown_room() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    let mut tiny_chest = RelicState::new(RelicId::TinyChest);
    tiny_chest.counter = 3;
    run_state.relics.push(tiny_chest);

    let _ = run_state.roll_question_mark_room_type(None);
    let tiny_chest = run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::TinyChest)
        .expect("Tiny Chest should be present");
    assert_eq!(tiny_chest.counter, 0);

    let mut generator = crate::state::events::generator::EventGenerator::new(1);
    generator.monster_chance = 1.0;
    generator.shop_chance = 0.0;
    generator.treasure_chance = 0.0;
    let mut rng = crate::runtime::rng::RngPool::new(1);
    let ctx = crate::state::events::context::EventContext {
        act_num: 1,
        ascension_level: 0,
        is_daily_run: false,
        highest_unlocked_ascension_level: 0,
        floor_num: 10,
        map_current_y: Some(10),
        map_height: 15,
        gold: 99,
        current_hp: 80,
        max_hp: 80,
        playtime_seconds: 0.0,
        has_curses: false,
        tiny_chest_counter: 4,
        has_golden_idol: false,
        has_juzu_bracelet: false,
        previous_room_was_shop: false,
        relic_count: 1,
    };

    assert_eq!(
        generator.roll_room_type(&mut rng, &ctx),
        crate::state::events::generator::RoomRoll::Treasure
    );
}

#[test]
fn shared_common_obtain_potion_upgrade_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Omamori), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::PotionBelt), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::ToyOrnithopter), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::WarPaint), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::Whetstone), RelicTier::Common);

    assert!(!get_relic_subscriptions(RelicId::Omamori).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::PotionBelt).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::ToyOrnithopter).on_use_potion);
    assert!(!get_relic_subscriptions(RelicId::WarPaint).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::Whetstone).at_battle_start);
}

#[test]
fn omamori_blocks_exactly_two_curse_obtains_then_marks_used_up() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::Omamori));
    let deck_len = run_state.master_deck.len();

    assert!(!run_state.add_card_to_deck(CardId::Regret));
    let omamori = run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::Omamori)
        .expect("Omamori should be present");
    assert_eq!(omamori.counter, 1);
    assert!(!omamori.used_up);
    assert_eq!(run_state.master_deck.len(), deck_len);

    assert!(!run_state.add_card_to_deck(CardId::Injury));
    let omamori = run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::Omamori)
        .expect("Omamori should be present");
    assert_eq!(omamori.counter, 0);
    assert!(omamori.used_up);
    assert_eq!(run_state.master_deck.len(), deck_len);

    assert!(run_state.add_card_to_deck(CardId::Doubt));
    assert_eq!(run_state.master_deck.len(), deck_len + 1);
}

#[test]
fn potion_belt_appends_two_empty_slots_without_reordering_existing_potions() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.potions = vec![
        Some(crate::content::potions::Potion::new(
            crate::content::potions::PotionId::StrengthPotion,
            1,
        )),
        None,
    ];

    assert!(potion_belt::on_equip(&mut run_state).is_none());
    assert_eq!(run_state.potions.len(), 4);
    assert_eq!(
        run_state.potions[0].as_ref().map(|potion| potion.id),
        Some(crate::content::potions::PotionId::StrengthPotion)
    );
    assert!(run_state.potions[1].is_none());
    assert!(run_state.potions[2].is_none());
    assert!(run_state.potions[3].is_none());
}

#[test]
fn toy_ornithopter_queues_bottom_heal_when_potion_is_used() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::ToyOrnithopter));

    let actions = hooks::on_use_potion(&state, 0);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::Heal {
            target: 0,
            amount: 5
        }
    ));
}

#[test]
fn war_paint_and_whetstone_upgrade_only_matching_card_types_with_relic_source() {
    let mut whetstone_run = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    whetstone_run.master_deck = vec![
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::Defend, 2),
    ];
    whetstone_run.emitted_events.clear();

    assert!(whetstone::on_equip(&mut whetstone_run).is_none());
    assert_eq!(whetstone_run.master_deck[0].upgrades, 1);
    assert_eq!(whetstone_run.master_deck[1].upgrades, 0);
    assert!(whetstone_run.emitted_events.iter().any(|event| matches!(
        event,
        DomainEvent::CardUpgraded {
            source: DomainEventSource::Relic(RelicId::Whetstone),
            ..
        }
    )));

    let mut war_paint_run = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    war_paint_run.master_deck = vec![
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::Defend, 2),
    ];
    war_paint_run.emitted_events.clear();

    assert!(war_paint::on_equip(&mut war_paint_run).is_none());
    assert_eq!(war_paint_run.master_deck[0].upgrades, 0);
    assert_eq!(war_paint_run.master_deck[1].upgrades, 1);
    assert!(war_paint_run.emitted_events.iter().any(|event| matches!(
        event,
        DomainEvent::CardUpgraded {
            source: DomainEventSource::Relic(RelicId::WarPaint),
            ..
        }
    )));
}

#[test]
fn shared_uncommon_card_reward_relic_metadata_matches_java_sources() {
    assert_eq!(
        get_relic_tier(RelicId::DarkstonePeriapt),
        RelicTier::Uncommon
    );
    assert_eq!(get_relic_tier(RelicId::MoltenEgg), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::ToxicEgg), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::FrozenEgg), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::QuestionCard), RelicTier::Uncommon);

    assert!(!get_relic_subscriptions(RelicId::DarkstonePeriapt).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::MoltenEgg).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::ToxicEgg).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::FrozenEgg).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::QuestionCard).at_battle_start);
}

#[test]
fn darkstone_periapt_triggers_only_after_curse_is_obtained() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state
        .relics
        .push(RelicState::new(RelicId::DarkstonePeriapt));
    run_state.current_hp = 50;
    run_state.max_hp = 80;
    run_state.emitted_events.clear();

    let deck_len = run_state.master_deck.len();
    assert!(run_state.add_card_to_deck_with_upgrades_from(
        CardId::Regret,
        0,
        DomainEventSource::RewardScreen
    ));
    assert_eq!(run_state.master_deck.len(), deck_len + 1);
    assert_eq!(run_state.max_hp, 86);
    assert_eq!(run_state.current_hp, 56);
    assert!(run_state.emitted_events.iter().any(|event| matches!(
        event,
        DomainEvent::MaxHpChanged {
            delta: 6,
            source: DomainEventSource::RewardScreen,
            ..
        }
    )));

    let mut blocked = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    blocked.relics.clear();
    blocked
        .relics
        .push(RelicState::new(RelicId::DarkstonePeriapt));
    blocked.relics.push(RelicState::new(RelicId::Omamori));
    blocked.current_hp = 50;
    blocked.max_hp = 80;

    let blocked_len = blocked.master_deck.len();
    assert!(!blocked.add_card_to_deck(CardId::Regret));
    assert_eq!(blocked.master_deck.len(), blocked_len);
    assert_eq!(
        blocked.max_hp, 80,
        "Omamori prevents the curse obtain, so Darkstone Periapt onObtainCard does not fire"
    );
    assert_eq!(blocked.current_hp, 50);
}

#[test]
fn egg_relics_preview_obtain_upgrades_without_double_upgrading_existing_plus_cards() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::MoltenEgg));
    run_state.relics.push(RelicState::new(RelicId::ToxicEgg));
    run_state.relics.push(RelicState::new(RelicId::FrozenEgg));
    run_state.master_deck.clear();

    assert!(run_state.add_card_to_deck(CardId::Strike));
    assert!(run_state.add_card_to_deck(CardId::Defend));
    assert!(run_state.add_card_to_deck(CardId::Inflame));
    assert!(run_state.add_card_to_deck_with_upgrades(CardId::SearingBlow, 1));

    assert_eq!(run_state.master_deck[0].upgrades, 1);
    assert_eq!(run_state.master_deck[1].upgrades, 1);
    assert_eq!(run_state.master_deck[2].upgrades, 1);
    assert_eq!(
        run_state.master_deck[3].upgrades, 1,
        "Java Egg relics call upgrade only when !card.upgraded; pre-upgraded Searing Blow is not incremented again"
    );
}

#[test]
fn shared_uncommon_combat_trigger_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::GremlinHorn), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::LetterOpener), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::Kunai), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::Shuriken), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::OrnamentalFan), RelicTier::Uncommon);
    assert_eq!(
        get_relic_tier(RelicId::MercuryHourglass),
        RelicTier::Uncommon
    );

    assert!(get_relic_subscriptions(RelicId::GremlinHorn).on_monster_death);
    assert!(get_relic_subscriptions(RelicId::LetterOpener).on_use_card);
    assert!(get_relic_subscriptions(RelicId::LetterOpener).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::LetterOpener).on_victory);
    assert!(get_relic_subscriptions(RelicId::Kunai).on_use_card);
    assert!(get_relic_subscriptions(RelicId::Kunai).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::Kunai).on_victory);
    assert!(get_relic_subscriptions(RelicId::Shuriken).on_use_card);
    assert!(get_relic_subscriptions(RelicId::Shuriken).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::Shuriken).on_victory);
    assert!(get_relic_subscriptions(RelicId::OrnamentalFan).on_use_card);
    assert!(get_relic_subscriptions(RelicId::OrnamentalFan).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::OrnamentalFan).on_victory);
    assert!(get_relic_subscriptions(RelicId::MercuryHourglass).at_turn_start);
}

#[test]
fn gremlin_horn_triggers_only_when_another_monster_remains_alive() {
    let mut dying = crate::test_support::test_monster(EnemyId::JawWorm);
    dying.id = 1;
    dying.current_hp = 0;
    dying.is_dying = true;
    let mut alive = crate::test_support::test_monster(EnemyId::Cultist);
    alive.id = 2;

    let state = crate::test_support::combat_with_monsters(vec![dying.clone(), alive]);
    let actions = gremlin_horn::GremlinHorn::on_monster_death(&state, 1);
    assert_eq!(actions.len(), 2);
    assert!(matches!(
        actions[0].action,
        Action::GainEnergy { amount: 1 }
    ));
    assert!(matches!(actions[1].action, Action::DrawCards(1)));

    let last_monster_state = crate::test_support::combat_with_monsters(vec![dying]);
    assert!(
        gremlin_horn::GremlinHorn::on_monster_death(&last_monster_state, 1).is_empty(),
        "Java Gremlin Horn checks !areMonstersBasicallyDead(), so the final kill does not trigger"
    );
}

#[test]
fn letter_opener_resets_each_turn_and_fires_on_third_skill() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::Cultist));

    let mut relic = RelicState::new(RelicId::LetterOpener);
    relic.counter = 2;
    letter_opener::at_turn_start(&mut relic);
    assert_eq!(relic.counter, 0);

    relic.counter = -1;
    let first_skill = letter_opener::on_use_card(&state, CardId::Defend, &mut relic);
    assert!(first_skill.is_empty());
    assert_eq!(relic.counter, 1);

    relic.counter = 2;
    let third_skill = letter_opener::on_use_card(&state, CardId::Defend, &mut relic);
    assert_eq!(relic.counter, 0);
    assert_eq!(third_skill.len(), 1);
    match &third_skill[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            ..
        } => {
            assert_eq!(
                *source, NO_SOURCE,
                "Java Letter Opener uses DamageAllEnemiesAction(null, ...)"
            );
            assert_eq!(damages.as_slice(), &[5, 5]);
            assert_eq!(*damage_type, crate::runtime::action::DamageType::Thorns);
        }
        other => panic!("expected Letter Opener all-enemy damage, got {other:?}"),
    }
}

#[test]
fn attack_counter_relics_fire_on_third_attack_and_reset_on_victory() {
    let mut kunai_relic = RelicState::new(RelicId::Kunai);
    kunai_relic.counter = 2;
    let kunai_actions = kunai::on_use_card(CardId::Strike, &mut kunai_relic);
    assert_eq!(kunai_relic.counter, 0);
    assert_eq!(kunai_actions.len(), 1);
    assert!(matches!(
        kunai_actions[0].action,
        Action::ApplyPower {
            power_id: PowerId::Dexterity,
            amount: 1,
            ..
        }
    ));
    kunai_relic.counter = 2;
    assert!(kunai::on_use_card(CardId::Defend, &mut kunai_relic).is_empty());
    assert_eq!(kunai_relic.counter, 2);

    let mut shuriken_relic = RelicState::new(RelicId::Shuriken);
    shuriken_relic.counter = 2;
    let shuriken_actions = shuriken::on_use_card(CardId::Strike, &mut shuriken_relic);
    assert_eq!(shuriken_relic.counter, 0);
    assert_eq!(shuriken_actions.len(), 1);
    assert!(matches!(
        shuriken_actions[0].action,
        Action::ApplyPower {
            power_id: PowerId::Strength,
            amount: 1,
            ..
        }
    ));

    let mut fan_relic = RelicState::new(RelicId::OrnamentalFan);
    // Java inherits AbstractRelic.counter = -1 until atTurnStart.
    assert_eq!(fan_relic.counter, -1);
    ornamental_fan::at_turn_start(&mut fan_relic);
    assert_eq!(fan_relic.counter, 0);
    for expected in 1..=2 {
        assert!(ornamental_fan::on_use_card(&mut fan_relic).is_empty());
        assert_eq!(fan_relic.counter, expected);
    }
    let fan_actions = ornamental_fan::on_use_card(&mut fan_relic);
    assert_eq!(fan_relic.counter, 0);
    assert_eq!(fan_actions.len(), 1);
    assert!(matches!(
        fan_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 4
        }
    ));
    assert!(ornamental_fan::on_use_card(&mut fan_relic).is_empty());
    assert_eq!(fan_relic.counter, 1);
    ornamental_fan::at_turn_start(&mut fan_relic);
    assert_eq!(fan_relic.counter, 0);
    ornamental_fan::on_victory(&mut fan_relic);
    assert_eq!(fan_relic.counter, -1);

    let mut relic = RelicState::new(RelicId::Kunai);
    relic.counter = 2;
    kunai::on_victory(&mut relic);
    assert_eq!(relic.counter, -1);
}

#[test]
fn mercury_hourglass_queues_thorns_damage_to_all_monster_slots() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::Cultist));

    let actions = mercury_hourglass::at_turn_start(&state);
    assert_eq!(actions.len(), 1);
    match &actions[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            ..
        } => {
            assert_eq!(
                *source, NO_SOURCE,
                "Java Mercury Hourglass uses DamageAllEnemiesAction(null, ...)"
            );
            assert_eq!(damages.as_slice(), &[3, 3]);
            assert_eq!(*damage_type, crate::runtime::action::DamageType::Thorns);
        }
        other => panic!("expected Mercury Hourglass all-enemy damage, got {other:?}"),
    }
}

#[test]
fn shared_uncommon_start_victory_reward_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::HornCleat), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::Pantograph), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::MeatOnTheBone), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::Pear), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::SingingBowl), RelicTier::Uncommon);
    assert_eq!(
        get_relic_tier(RelicId::WhiteBeastStatue),
        RelicTier::Uncommon
    );

    assert!(get_relic_subscriptions(RelicId::HornCleat).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::HornCleat).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::HornCleat).on_victory);
    assert!(get_relic_subscriptions(RelicId::Pantograph).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::MeatOnTheBone).on_victory);
    assert!(!get_relic_subscriptions(RelicId::Pear).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::SingingBowl).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::WhiteBeastStatue).at_battle_start);
}

#[test]
fn horn_cleat_triggers_only_on_second_turn_then_disables_until_next_combat() {
    let mut relic = RelicState::new(RelicId::HornCleat);
    horn_cleat::HornCleat::at_battle_start(&mut relic);
    assert_eq!(relic.counter, 0);

    assert!(horn_cleat::HornCleat::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, 1);

    let second_turn = horn_cleat::HornCleat::at_turn_start(&mut relic);
    assert_eq!(relic.counter, -1);
    assert_eq!(second_turn.len(), 1);
    assert_eq!(second_turn[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        second_turn[0].action,
        Action::GainBlock {
            target: 0,
            amount: 14
        }
    ));

    assert!(horn_cleat::HornCleat::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, -1);

    let mut state = crate::test_support::blank_test_combat();
    let mut state_relic = RelicState::new(RelicId::HornCleat);
    state.entities.player.add_relic(state_relic.clone());
    assert!(hooks::at_battle_start(&mut state).is_empty());
    assert_eq!(state.entities.player.relics[0].counter, 0);
    assert!(hooks::at_turn_start(&mut state).is_empty());
    assert_eq!(state.entities.player.relics[0].counter, 1);

    state.entities.player.relics.clear();
    state_relic.counter = 1;
    state_relic.used_up = true;
    state.entities.player.add_relic(state_relic);
    assert!(hooks::on_victory(&mut state).is_empty());
    assert_eq!(state.entities.player.relics[0].counter, -1);
    assert!(!state.entities.player.relics[0].used_up);
}

#[test]
fn pantograph_heals_only_in_boss_combat_with_java_top_insertion() {
    let mut boss_state = crate::test_support::blank_test_combat();
    boss_state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::SlimeBoss));
    let actions = pantograph::at_battle_start(&boss_state);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[0].action,
        Action::Heal {
            target: 0,
            amount: 25
        }
    ));

    let mut hallway_state = crate::test_support::blank_test_combat();
    hallway_state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    assert!(pantograph::at_battle_start(&hallway_state).is_empty());
}

#[test]
fn meat_on_the_bone_heals_at_or_below_half_hp_without_used_up_gate() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.current_hp = 40;
    state.entities.player.max_hp = 80;

    let actions = meat_on_the_bone::on_victory(&state);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[0].action,
        Action::Heal {
            target: 0,
            amount: 12
        }
    ));

    state.entities.player.current_hp = 41;
    assert!(meat_on_the_bone::on_victory(&state).is_empty());

    state.entities.player.current_hp = 0;
    assert!(meat_on_the_bone::on_victory(&state).is_empty());

    state.entities.player.current_hp = 40;
    let mut used_relic = RelicState::new(RelicId::MeatOnTheBone);
    used_relic.used_up = true;
    state.entities.player.add_relic(used_relic);
    assert_eq!(
        hooks::on_victory(&mut state).len(),
        1,
        "Java Meat on the Bone is not a one-time used_up relic"
    );
}

#[test]
fn pear_on_equip_grants_ten_max_hp_and_heals_same_amount() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.current_hp = 50;
    run_state.max_hp = 80;

    assert!(pear::on_equip(&mut run_state).is_none());
    assert_eq!(run_state.max_hp, 90);
    assert_eq!(run_state.current_hp, 60);
}

#[test]
fn singing_bowl_card_reward_option_grants_two_max_hp_with_reward_source() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.current_hp = 50;
    run_state.max_hp = 80;
    run_state.relics.clear();
    run_state.relics.push(RelicState::new(RelicId::SingingBowl));
    run_state.emitted_events.clear();

    let mut reward_state = crate::rewards::state::RewardState {
        items: Vec::new(),
        skippable: true,
        screen_context: crate::rewards::state::RewardScreenContext::Standard,
        pending_card_choice: Some(vec![crate::rewards::state::RewardCard::new(
            CardId::Strike,
            0,
        )]),
        pending_card_reward_index: None,
    };

    assert_eq!(
        crate::rewards::handler::handle(
            &mut run_state,
            &mut reward_state,
            Some(crate::state::core::ClientInput::SelectCard(1)),
        ),
        None,
        "Singing Bowl must return to the empty combat reward screen"
    );
    assert_eq!(run_state.max_hp, 82);
    assert_eq!(run_state.current_hp, 52);
    assert!(run_state.emitted_events.iter().any(|event| matches!(
        event,
        DomainEvent::MaxHpChanged {
            delta: 2,
            source: DomainEventSource::RewardScreen,
            ..
        }
    )));
    let next = crate::rewards::handler::handle(
        &mut run_state,
        &mut reward_state,
        Some(crate::state::core::ClientInput::Proceed),
    )
    .expect("Proceed should open the Java-style map preview");
    let crate::state::core::EngineState::MapOverlay { return_state } = next else {
        panic!("empty reward Proceed should preserve the reward return path");
    };
    let crate::state::core::EngineState::RewardScreen(returned_rewards) = *return_state else {
        panic!("empty map preview should return to its reward screen");
    };
    assert!(returned_rewards.items.is_empty());
}

#[test]
fn white_beast_statue_forces_potion_reward_even_with_sozu() {
    let mut run_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run_state.relics.clear();
    run_state
        .relics
        .push(RelicState::new(RelicId::WhiteBeastStatue));

    let rewards = crate::rewards::generator::generate_combat_rewards(&mut run_state, false, false);
    assert!(
        rewards
            .items
            .iter()
            .any(|item| matches!(item, crate::rewards::state::RewardItem::Potion { .. })),
        "White Beast Statue sets the post-combat potion drop chance to 100"
    );

    let mut sozu_state = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    sozu_state.relics.clear();
    sozu_state
        .relics
        .push(RelicState::new(RelicId::WhiteBeastStatue));
    sozu_state.relics.push(RelicState::new(RelicId::Sozu));

    let sozu_rewards =
        crate::rewards::generator::generate_combat_rewards(&mut sozu_state, false, false);
    assert!(
        sozu_rewards
            .items
            .iter()
            .any(|item| matches!(item, crate::rewards::state::RewardItem::Potion { .. })),
        "Java addPotionToRewards ignores Sozu; RewardItem.claimReward handles Sozu later"
    );
}

#[test]
fn shared_uncommon_action_counter_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::BlueCandle), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::InkBottle), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::MummifiedHand), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::Sundial), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::StrikeDummy), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::Matryoshka), RelicTier::Uncommon);

    assert!(get_relic_subscriptions(RelicId::BlueCandle).on_use_card);
    assert!(get_relic_subscriptions(RelicId::InkBottle).on_use_card);
    assert!(get_relic_subscriptions(RelicId::MummifiedHand).on_use_card);
    assert!(get_relic_subscriptions(RelicId::Sundial).on_shuffle);
    assert!(!get_relic_subscriptions(RelicId::StrikeDummy).on_use_card);
    assert!(!get_relic_subscriptions(RelicId::Matryoshka).on_use_card);
}

#[test]
fn blue_candle_only_loses_hp_for_curse_cards_and_marks_rupture_path() {
    assert!(blue_candle::BlueCandle::on_use_card(CardId::Strike).is_empty());

    let actions = blue_candle::BlueCandle::on_use_card(CardId::Regret);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::LoseHp {
            target: 0,
            amount: 1,
            triggers_rupture: true
        }
    ));
}

#[test]
fn ink_bottle_counter_mutates_immediately_and_draws_on_tenth_card() {
    let mut relic = RelicState::new(RelicId::InkBottle);
    relic.counter = 8;
    assert!(ink_bottle::on_use_card(&mut relic).is_empty());
    assert_eq!(relic.counter, 9);

    let actions = ink_bottle::on_use_card(&mut relic);
    assert_eq!(relic.counter, 0);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(actions[0].action, Action::DrawCards(1)));

    relic.counter = -1;
    assert!(ink_bottle::on_use_card(&mut relic).is_empty());
    assert_eq!(
        relic.counter, 0,
        "Java ++counter from -1 reaches 0 without triggering the tenth-card draw"
    );

    let mut state = crate::test_support::blank_test_combat();
    let mut state_relic = RelicState::new(RelicId::InkBottle);
    state_relic.counter = 9;
    state.entities.player.add_relic(state_relic);
    let strike = CombatCard::for_scenario(CardId::Strike, 11);
    assert_eq!(hooks::on_use_card(&mut state, &strike, None).len(), 1);
    assert_eq!(state.entities.player.relics[0].counter, 0);
}

#[test]
fn sundial_counter_mutates_immediately_and_grants_energy_on_third_shuffle() {
    let mut relic = RelicState::new(RelicId::Sundial);
    relic.counter = 1;
    assert!(sundial::on_shuffle(&mut relic).is_empty());
    assert_eq!(relic.counter, 2);

    let actions = sundial::on_shuffle(&mut relic);
    assert_eq!(relic.counter, 0);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::GainEnergy { amount: 2 }
    ));

    relic.counter = -1;
    assert!(sundial::on_shuffle(&mut relic).is_empty());
    assert_eq!(
        relic.counter, 0,
        "Java ++counter from -1 reaches 0 without granting energy"
    );

    let mut state = crate::test_support::blank_test_combat();
    let mut state_relic = RelicState::new(RelicId::Sundial);
    state_relic.counter = 2;
    state.entities.player.add_relic(state_relic);
    assert_eq!(hooks::on_shuffle(&mut state).len(), 1);
    assert_eq!(state.entities.player.relics[0].counter, 0);
}

#[test]
fn corruption_constructs_skill_discounts_before_mummified_hand_selects_a_card() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::MummifiedHand));
    state.turn.energy = 3;
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Corruption, 1),
        CombatCard::for_scenario(CardId::Defend, 2),
        CombatCard::for_scenario(CardId::Bash, 3),
    ];
    let trace = crate::runtime::rng::begin_sts_rng_trace().unwrap();
    crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut state).unwrap();
    let calls = trace.finish().unwrap();
    assert!(matches!(
        calls.as_slice(),
        [crate::runtime::rng::StsRngTraceEvent {
            call: crate::runtime::rng::StsRngCall::RandomIntRange {
                start: 0,
                end: 0,
                ..
            },
            ..
        }]
    ));
    assert_eq!(state.zones.hand[0].id, CardId::Defend);
    assert_eq!(state.zones.hand[0].cost_modifier, -1);
    assert_eq!(state.zones.hand[1].id, CardId::Bash);
    assert_eq!(state.zones.hand[1].cost_modifier, 0);
    assert_eq!(state.zones.hand[1].cost_for_turn_java(), 0);
    assert_eq!(
        state.get_power(0, crate::content::powers::PowerId::Corruption),
        0
    );

    // A card created after construction must not be discounted by a second,
    // delayed scan when the power application itself executes.
    state
        .zones
        .hand
        .push(CombatCard::for_scenario(CardId::Defend, 4));
    crate::engine::action_handlers::powers::handle_apply_power(
        0,
        0,
        crate::content::powers::PowerId::Corruption,
        -1,
        &mut state,
    );
    assert_eq!(state.zones.hand[2].cost_for_turn_java(), 1);
}

#[test]
fn mummified_hand_uses_java_range_draw_only_for_a_nonempty_pool() {
    for count in [0, 1, 4] {
        let mut state = crate::test_support::blank_test_combat();
        state.zones.hand = (0..count)
            .map(|index| CombatCard::for_scenario(CardId::Defend, index + 1))
            .collect();
        let trace = crate::runtime::rng::begin_sts_rng_trace().unwrap();
        mummified_hand::apply_effect(&mut state);
        let calls = trace.finish().unwrap();
        if count == 0 {
            assert!(calls.is_empty());
        } else {
            assert_eq!(calls.len(), 1);
            assert!(matches!(calls[0].call,
                crate::runtime::rng::StsRngCall::RandomIntRange { start: 0, end, .. }
                    if end == count as i32 - 1
            ));
            assert_eq!(
                state
                    .zones
                    .hand
                    .iter()
                    .filter(|card| card.cost_for_turn == Some(0))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn mummified_hand_sets_one_eligible_hand_card_cost_to_zero_immediately() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 1),
        {
            let mut zero_cost = CombatCard::for_scenario(CardId::Strike, 2);
            zero_cost.cost_for_turn = Some(0);
            zero_cost
        },
        {
            let mut free_once = CombatCard::for_scenario(CardId::Bash, 3);
            free_once.free_to_play_once = true;
            free_once
        },
    ];

    let power = CombatCard::for_scenario(CardId::Inflame, 99);
    let actions = mummified_hand::on_use_card(&power, &mut state);
    assert!(actions.is_empty());
    assert_eq!(state.zones.hand[0].cost_for_turn, Some(0));
    assert_eq!(state.zones.hand[1].cost_for_turn, Some(0));
    assert_eq!(state.zones.hand[2].cost_for_turn, None);

    state.zones.hand[0].cost_for_turn = None;
    let attack = CombatCard::for_scenario(CardId::Strike, 100);
    assert!(mummified_hand::on_use_card(&attack, &mut state).is_empty());
    assert_eq!(state.zones.hand[0].cost_for_turn, None);
}

#[test]
fn strike_dummy_adds_three_damage_to_strike_tag_attacks_before_power_modifiers() {
    let strike = CombatCard::for_scenario(CardId::Strike, 1);
    let pommel = CombatCard::for_scenario(CardId::PommelStrike, 2);
    let bash = CombatCard::for_scenario(CardId::Bash, 3);

    assert_eq!(
        strike_dummy::modify_attack_damage_for_card(&strike, 6.0),
        9.0
    );
    assert_eq!(
        strike_dummy::modify_attack_damage_for_card(&pommel, 9.0),
        12.0
    );
    assert_eq!(strike_dummy::modify_attack_damage_for_card(&bash, 8.0), 8.0);
}

#[test]
fn matryoshka_counter_starts_at_two_and_only_positive_counter_grants_extra_relic() {
    let relic = RelicState::new(RelicId::Matryoshka);
    assert_eq!(relic.counter, 2);
    assert!(matryoshka::should_grant_extra_relic(2));
    assert!(matryoshka::should_grant_extra_relic(1));
    assert!(!matryoshka::should_grant_extra_relic(0));
    assert!(!matryoshka::should_grant_extra_relic(-2));
}

#[test]
fn shared_uncommon_bottle_shop_and_rest_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::BottledFlame), RelicTier::Uncommon);
    assert_eq!(
        get_relic_tier(RelicId::BottledLightning),
        RelicTier::Uncommon
    );
    assert_eq!(get_relic_tier(RelicId::BottledTornado), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::Courier), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::EternalFeather), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::NlothsGift), RelicTier::Special);

    assert!(!get_relic_subscriptions(RelicId::BottledFlame).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::BottledLightning).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::BottledTornado).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::Courier).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::EternalFeather).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::NlothsGift).at_battle_start);
}

#[test]
fn normal_relic_rewards_can_return_bottled_relics_but_screenless_rewards_skip_them() {
    let mut no_non_basic_attack = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    no_non_basic_attack.uncommon_relic_pool = vec![RelicId::Pear, RelicId::BottledFlame];
    assert_eq!(
        no_non_basic_attack.random_relic_by_tier(RelicTier::Uncommon),
        RelicId::Pear,
        "Java Bottled Flame canSpawn rejects starter-only attack decks"
    );

    let mut normal = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    normal
        .master_deck
        .push(CombatCard::for_scenario(CardId::PommelStrike, 1001));
    normal.uncommon_relic_pool = vec![RelicId::BottledFlame, RelicId::Pear];
    assert_eq!(
        normal.random_relic_by_tier(RelicTier::Uncommon),
        RelicId::BottledFlame,
        "Java returnRandomRelic can return bottled relics from normal reward paths"
    );

    let mut screenless = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    screenless
        .master_deck
        .push(CombatCard::for_scenario(CardId::PommelStrike, 1001));
    screenless.uncommon_relic_pool = vec![RelicId::BottledFlame, RelicId::Pear];
    assert_eq!(
        screenless.random_screenless_relic(RelicTier::Uncommon),
        RelicId::Pear,
        "Java returnRandomScreenlessRelic skips Bottled Flame/Lightning/Tornado and Whetstone"
    );
}

#[test]
fn normal_and_end_relic_paths_consume_opposite_pool_ends_like_java() {
    let mut normal = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    normal.common_relic_pool = vec![RelicId::Anchor, RelicId::Akabeko];
    assert_eq!(
        normal.random_relic_by_tier(RelicTier::Common),
        RelicId::Anchor,
        "Java returnRandomRelicKey removes index 0 for normal rewards"
    );
    assert_eq!(normal.common_relic_pool, vec![RelicId::Akabeko]);

    let mut end = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    end.common_relic_pool = vec![RelicId::Anchor, RelicId::Akabeko];
    assert_eq!(
        end.random_relic_end_by_tier(RelicTier::Common),
        RelicId::Akabeko,
        "Java returnEndRandomRelicKey removes the last pool entry for shop/end paths"
    );
    assert_eq!(end.common_relic_pool, vec![RelicId::Anchor]);
}

#[test]
fn relic_pool_build_order_matches_java_hashmap_traversal() {
    assert_eq!(
        build_relic_pool(RelicTier::Common, "Ironclad"),
        vec![
            RelicId::Whetstone,
            RelicId::Boot,
            RelicId::BloodVial,
            RelicId::MealTicket,
            RelicId::PenNib,
            RelicId::Akabeko,
            RelicId::Lantern,
            RelicId::RegalPillow,
            RelicId::BagOfPreparation,
            RelicId::AncientTeaSet,
            RelicId::SmilingMask,
            RelicId::PotionBelt,
            RelicId::PreservedInsect,
            RelicId::Omamori,
            RelicId::MawBank,
            RelicId::ArtOfWar,
            RelicId::ToyOrnithopter,
            RelicId::CeramicFish,
            RelicId::Vajra,
            RelicId::CentennialPuzzle,
            RelicId::Strawberry,
            RelicId::HappyFlower,
            RelicId::OddlySmoothStone,
            RelicId::WarPaint,
            RelicId::BronzeScales,
            RelicId::JuzuBracelet,
            RelicId::DreamCatcher,
            RelicId::Nunchaku,
            RelicId::TinyChest,
            RelicId::Orichalcum,
            RelicId::Anchor,
            RelicId::BagOfMarbles,
            RelicId::RedSkull,
        ],
        "RelicLibrary.populateRelicPool iterates Java HashMap entrySet order, not source add order"
    );
    assert_eq!(
        build_relic_pool(RelicTier::Boss, "Silent"),
        vec![
            RelicId::FusionHammer,
            RelicId::VelvetChoker,
            RelicId::RunicDome,
            RelicId::SlaversCollar,
            RelicId::SneckoEye,
            RelicId::PandorasBox,
            RelicId::CursedKey,
            RelicId::BustedCrown,
            RelicId::Ectoplasm,
            RelicId::TinyHouse,
            RelicId::Sozu,
            RelicId::PhilosopherStone,
            RelicId::Astrolabe,
            RelicId::BlackStar,
            RelicId::SacredBark,
            RelicId::EmptyCage,
            RelicId::RunicPyramid,
            RelicId::CallingBell,
            RelicId::CoffeeDripper,
            RelicId::WristBlade,
            RelicId::HoveringKite,
            RelicId::RingOfTheSerpent,
        ],
        "Class-specific relic HashMap order is appended after the shared HashMap order"
    );
}

#[test]
fn shared_rare_start_and_turn_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::CaptainsWheel), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::ClockworkSouvenir), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::FossilizedHelix), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::IncenseBurner), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::Pocketwatch), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::StoneCalendar), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::ThreadAndNeedle), RelicTier::Rare);

    let captains = get_relic_subscriptions(RelicId::CaptainsWheel);
    assert!(captains.at_battle_start);
    assert!(captains.at_turn_start);
    assert!(captains.on_victory);

    assert!(get_relic_subscriptions(RelicId::ClockworkSouvenir).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::FossilizedHelix).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::IncenseBurner).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::ThreadAndNeedle).at_battle_start);

    let pocketwatch = get_relic_subscriptions(RelicId::Pocketwatch);
    assert!(pocketwatch.at_battle_start);
    assert!(pocketwatch.at_turn_start_post_draw);
    assert!(pocketwatch.on_use_card);
    assert!(pocketwatch.on_victory);

    let stone_calendar = get_relic_subscriptions(RelicId::StoneCalendar);
    assert!(stone_calendar.at_battle_start);
    assert!(stone_calendar.at_turn_start);
    assert!(stone_calendar.at_end_of_turn);
    assert!(stone_calendar.on_victory);
}

#[test]
fn captains_wheel_mutates_counter_immediately_and_fires_once_on_third_turn() {
    let mut relic = RelicState::new(RelicId::CaptainsWheel);
    captains_wheel::CaptainsWheel::at_battle_start(&mut relic);
    assert_eq!(relic.counter, 0);

    assert!(captains_wheel::CaptainsWheel::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, 1);
    assert!(captains_wheel::CaptainsWheel::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, 2);

    let actions = captains_wheel::CaptainsWheel::at_turn_start(&mut relic);
    assert_eq!(relic.counter, -1, "Java sets counter to -1 after firing");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 18
        }
    ));

    assert!(captains_wheel::CaptainsWheel::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, -1);
    captains_wheel::CaptainsWheel::on_victory(&mut relic);
    assert_eq!(relic.counter, -1);
}

#[test]
fn incense_burner_counter_mutates_immediately_and_grants_intangible_on_six() {
    let mut relic = RelicState::new(RelicId::IncenseBurner);
    relic.counter = 4;
    assert!(incense_burner::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, 5);

    let actions = incense_burner::at_turn_start(&mut relic);
    assert_eq!(relic.counter, 0, "Java resets counter to 0 when it fires");
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::IntangiblePlayer,
            amount: 1
        }
    ));

    relic.counter = -1;
    assert!(incense_burner::at_turn_start(&mut relic).is_empty());
    assert_eq!(relic.counter, 1, "Java -1 counter advances to 1, not 0");
}

#[test]
fn hook_persists_mutated_turn_start_relic_counters_before_actions_execute() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::CaptainsWheel));
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::IncenseBurner));
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::StoneCalendar));

    state.entities.player.relics[0].counter = 2;
    state.entities.player.relics[1].counter = 5;
    state.entities.player.relics[2].counter = 6;

    let actions = hooks::at_turn_start(&mut state);
    assert_eq!(state.entities.player.relics[0].counter, -1);
    assert_eq!(state.entities.player.relics[1].counter, 0);
    assert_eq!(state.entities.player.relics[2].counter, 7);
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().any(|info| matches!(
        info.action,
        Action::GainBlock {
            target: 0,
            amount: 18
        }
    )));
    assert!(actions.iter().any(|info| matches!(
        info.action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::IntangiblePlayer,
            amount: 1
        }
    )));
}

#[test]
fn stone_calendar_counter_and_null_source_damage_match_java() {
    let mut relic = RelicState::new(RelicId::StoneCalendar);
    stone_calendar::at_battle_start(&mut relic);
    assert_eq!(relic.counter, 0);

    for expected in 1..=7 {
        assert!(stone_calendar::at_turn_start(&mut relic).is_empty());
        assert_eq!(relic.counter, expected);
    }

    let mut state = crate::test_support::blank_test_combat();
    state.entities.monsters = vec![
        crate::test_support::test_monster(EnemyId::JawWorm),
        crate::test_support::test_monster(EnemyId::Cultist),
    ];
    let actions = stone_calendar::at_end_of_turn(&state, relic.counter);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    match &actions[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => {
            assert_eq!(*source, NO_SOURCE);
            assert_eq!(damages.as_slice(), &[52, 52]);
            assert_eq!(*damage_type, DamageType::Thorns);
            assert!(!is_modified);
        }
        other => panic!("unexpected Stone Calendar action: {other:?}"),
    }

    stone_calendar::on_victory(&mut relic);
    assert_eq!(relic.counter, -1);
}

#[test]
fn start_relic_action_order_matches_java_non_ui_actions() {
    let thread_actions = thread_and_needle::at_battle_start();
    assert_eq!(thread_actions.len(), 1);
    assert_eq!(
        thread_actions[0].insertion_mode,
        AddTo::Top,
        "Java uses addToTop for Thread and Needle PlatedArmor; UI relic action is ignored"
    );
    assert!(matches!(
        thread_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::PlatedArmor,
            amount: 4
        }
    ));

    let clockwork_actions = clockwork_souvenir::ClockworkSouvenir::at_battle_start();
    assert_eq!(clockwork_actions.len(), 1);
    assert_eq!(clockwork_actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        clockwork_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Artifact,
            amount: 1
        }
    ));

    let helix_actions = fossilized_helix::FossilizedHelix::at_battle_start();
    assert_eq!(helix_actions.len(), 1);
    assert_eq!(helix_actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        helix_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Buffer,
            amount: 1
        }
    ));
}

#[test]
fn pocketwatch_first_turn_and_three_card_limit_match_java() {
    let mut relic = RelicState::new(RelicId::Pocketwatch);
    pocketwatch::at_battle_start(&mut relic);
    assert_eq!(relic.counter, 0);
    assert_eq!(relic.amount, 1, "amount stores Java firstTurn=true");

    assert!(pocketwatch::at_turn_start_post_draw(&mut relic).is_empty());
    assert_eq!(relic.counter, 0);
    assert_eq!(relic.amount, 0, "first turn is consumed without drawing");

    for _ in 0..3 {
        pocketwatch::on_use_card(&mut relic);
    }
    let draw_actions = pocketwatch::at_turn_start_post_draw(&mut relic);
    assert_eq!(draw_actions.len(), 1);
    assert_eq!(draw_actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(draw_actions[0].action, Action::DrawCards(3)));
    assert_eq!(relic.counter, 0);
    assert_eq!(relic.amount, 0);

    for _ in 0..4 {
        pocketwatch::on_use_card(&mut relic);
    }
    assert!(pocketwatch::at_turn_start_post_draw(&mut relic).is_empty());
    assert_eq!(relic.counter, 0);
    assert_eq!(relic.amount, 0);

    pocketwatch::on_victory(&mut relic);
    assert_eq!(relic.counter, -1);
    assert_eq!(relic.amount, 0);
}

#[test]
fn shared_rare_card_flow_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::BirdFacedUrn), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::DeadBranch), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::DuVuDoll), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::GamblingChip), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::UnceasingTop), RelicTier::Rare);

    assert!(get_relic_subscriptions(RelicId::BirdFacedUrn).on_use_card);
    assert!(get_relic_subscriptions(RelicId::DeadBranch).on_exhaust);
    assert!(get_relic_subscriptions(RelicId::DuVuDoll).at_battle_start);

    let gambling_chip = get_relic_subscriptions(RelicId::GamblingChip);
    assert!(gambling_chip.at_battle_start_pre_draw);
    assert!(gambling_chip.at_turn_start_post_draw);
    assert!(
        !gambling_chip.at_turn_start,
        "Java Gambling Chip fires atTurnStartPostDraw, not atTurnStart"
    );

    let top = get_relic_subscriptions(RelicId::UnceasingTop);
    assert!(top.at_pre_battle);
    assert!(top.at_turn_start);
}

#[test]
fn player_add_relic_registers_pre_battle_and_pre_draw_buses() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::CrackedCore));
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::GamblingChip));

    assert_eq!(
        state.entities.player.relic_buses.at_pre_battle.as_slice(),
        &[0]
    );
    assert_eq!(
        state
            .entities
            .player
            .relic_buses
            .at_battle_start_pre_draw
            .as_slice(),
        &[1]
    );
}

#[test]
fn bird_faced_urn_heals_only_when_power_card_is_used() {
    let actions = bird_faced_urn::BirdFacedUrn::on_use_card(CardId::Inflame);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[0].action,
        Action::Heal {
            target: 0,
            amount: 2
        }
    ));

    assert!(bird_faced_urn::BirdFacedUrn::on_use_card(CardId::Strike).is_empty());
}

#[test]
fn dead_branch_skips_when_monsters_are_basically_dead() {
    let mut active = crate::test_support::blank_test_combat();
    active
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    assert_eq!(active.rng.card_random_rng.counter, 0);
    let actions = dead_branch::on_exhaust(&mut active, &mut RelicState::new(RelicId::DeadBranch));
    assert_eq!(actions.len(), 1);
    assert_eq!(
        active.rng.card_random_rng.counter, 1,
        "Java DeadBranch.onExhaust samples the random card before queuing MakeTempCardInHandAction"
    );
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    match &actions[0].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(*amount, 1);
            assert!(!crate::content::cards::get_card_definition(original.id)
                .tags
                .contains(&crate::content::cards::CardTag::Healing));
        }
        other => panic!("Dead Branch should queue a concrete generated card, got {other:?}"),
    }

    let mut dying = crate::test_support::test_monster(EnemyId::JawWorm);
    dying.current_hp = 0;
    dying.is_dying = true;
    let mut basically_dead = crate::test_support::combat_with_monsters(vec![dying]);
    assert!(dead_branch::on_exhaust(
        &mut basically_dead,
        &mut RelicState::new(RelicId::DeadBranch),
    )
    .is_empty());

    let mut zero_hp_not_dying = crate::test_support::test_monster(EnemyId::JawWorm);
    zero_hp_not_dying.current_hp = 0;
    zero_hp_not_dying.is_dying = false;
    let mut zero_hp_state = crate::test_support::combat_with_monsters(vec![zero_hp_not_dying]);
    assert_eq!(
        dead_branch::on_exhaust(
            &mut zero_hp_state,
            &mut RelicState::new(RelicId::DeadBranch)
        )
        .len(),
        1,
        "Java MonsterGroup.areMonstersBasicallyDead ignores currentHealth"
    );
}

#[test]
fn nilrys_codex_queues_action_and_action_uses_java_basically_dead_guard() {
    let mut state = crate::test_support::blank_test_combat();
    let mut zero_hp_not_dying = crate::test_support::test_monster(EnemyId::JawWorm);
    zero_hp_not_dying.current_hp = 0;
    zero_hp_not_dying.is_dying = false;
    zero_hp_not_dying.is_escaped = false;
    state.entities.monsters = vec![zero_hp_not_dying];

    let actions = nilrys_codex::at_end_of_turn(&state);
    assert_eq!(
        actions.len(),
        1,
        "Java CodexAction checks MonsterGroup.areMonstersBasicallyDead, which ignores currentHealth"
    );
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::SuspendForCardReward {
            skip_if_monsters_basically_dead: true,
            ..
        }
    ));

    let mut dying_monster = crate::test_support::test_monster(EnemyId::JawWorm);
    dying_monster.is_dying = true;
    let mut dying_state = crate::test_support::combat_with_monsters(vec![dying_monster]);
    dying_state.queue_action_back(actions[0].action.clone());
    let mut engine_state = crate::state::core::EngineState::CombatProcessing;

    assert!(crate::engine::core::tick_engine(
        &mut engine_state,
        &mut dying_state,
        None,
    ));
    assert_eq!(
        engine_state,
        crate::state::core::EngineState::CombatProcessing,
        "Java CodexAction skips at action execution when MonsterGroup.areMonstersBasicallyDead is true"
    );
    assert_eq!(
        dying_state.rng.card_random_rng.counter, 0,
        "CodexAction's execution-time basically-dead guard runs before generating card choices"
    );
}

#[test]
fn du_vu_doll_counter_tracks_master_deck_and_battle_start_uses_counter() {
    let mut run = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run.master_deck.clear();
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Strike, 1));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Injury, 2));

    assert!(run
        .obtain_relic(
            RelicId::DuVuDoll,
            crate::state::core::EngineState::MapNavigation
        )
        .is_none());
    assert_eq!(
        run.relics
            .iter()
            .find(|relic| relic.id == RelicId::DuVuDoll)
            .unwrap()
            .counter,
        1
    );

    assert!(run.add_card_to_deck_with_upgrades(CardId::Pain, 0));
    assert_eq!(
        run.relics
            .iter()
            .find(|relic| relic.id == RelicId::DuVuDoll)
            .unwrap()
            .counter,
        2
    );

    let injury_uuid = run
        .master_deck
        .iter()
        .find(|card| card.id == CardId::Injury)
        .unwrap()
        .lineage;
    run.remove_card_from_deck(injury_uuid);
    assert_eq!(
        run.relics
            .iter()
            .find(|relic| relic.id == RelicId::DuVuDoll)
            .unwrap()
            .counter,
        1
    );

    let mut relic = RelicState::new(RelicId::DuVuDoll);
    relic.counter = 2;
    let actions =
        du_vu_doll::at_battle_start(&crate::test_support::blank_test_combat(), &mut relic);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 2
        }
    ));
}

#[test]
fn gambling_chip_resets_pre_draw_and_fires_once_post_draw() {
    let mut relic = RelicState::new(RelicId::GamblingChip);
    relic.used_up = true;
    gambling_chip::at_battle_start_pre_draw(&mut relic);
    assert!(!relic.used_up);

    let actions = gambling_chip::at_turn_start_post_draw(&mut relic);
    assert!(relic.used_up);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::SuspendForHandSelect {
            min: 0,
            max: 99,
            can_cancel: true,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::GamblingChip
        }
    ));
    assert!(gambling_chip::at_turn_start_post_draw(&mut relic).is_empty());

    let mut state = crate::test_support::blank_test_combat();
    let mut state_relic = RelicState::new(RelicId::GamblingChip);
    state_relic.used_up = true;
    state.entities.player.add_relic(state_relic);
    assert!(hooks::at_battle_start_pre_draw(&mut state).is_empty());
    assert!(!state.entities.player.relics[0].used_up);
    assert_eq!(hooks::at_turn_start_post_draw(&mut state).len(), 1);
    assert!(state.entities.player.relics[0].used_up);
}

#[test]
fn unceasing_top_uses_mechanical_refresh_conditions_without_ui_state() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::UnceasingTop));
    state
        .zones
        .draw_pile
        .push(CombatCard::for_scenario(CardId::Strike, 1));

    hooks::at_pre_battle(&mut state);
    assert_eq!(state.entities.player.relics[0].amount, 0);
    assert!(!unceasing_top::maybe_on_refresh_hand(&mut state));

    hooks::at_turn_start(&mut state);
    assert_eq!(state.entities.player.relics[0].amount, 1);
    assert!(unceasing_top::maybe_on_refresh_hand(&mut state));
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::DrawCards(1))
    ));

    state
        .zones
        .draw_pile
        .push(CombatCard::for_scenario(CardId::Strike, 2));
    state
        .entities
        .player
        .relics
        .iter_mut()
        .find(|relic| relic.id == RelicId::UnceasingTop)
        .map(unceasing_top::disable_until_turn_ends);
    assert!(!unceasing_top::maybe_on_refresh_hand(&mut state));

    state.entities.player.relics[0].used_up = false;
    store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::NoDraw,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    assert!(!unceasing_top::maybe_on_refresh_hand(&mut state));
}

#[test]
fn shared_boss_relic_first_batch_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Astrolabe), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::BlackStar), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::BustedCrown), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::CallingBell), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::CoffeeDripper), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::CursedKey), RelicTier::Boss);

    assert_eq!(energy_master_delta(RelicId::BustedCrown), 1);
    assert_eq!(energy_master_delta(RelicId::CoffeeDripper), 1);
    assert_eq!(energy_master_delta(RelicId::CursedKey), 1);
}

#[test]
fn astrolabe_uses_java_purgeable_cards_and_auto_transforms_three_or_fewer() {
    let mut run = crate::state::run::RunState::new(7, 0, false, "Ironclad");
    run.master_deck.clear();
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Strike, 1));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Injury, 2));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::AscendersBane, 3));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::CurseOfTheBell, 4));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Necronomicurse, 5));

    let next = astrolabe::on_equip(&mut run, crate::state::core::EngineState::MapNavigation);
    assert!(
        next.is_none(),
        "Java auto-transforms when there are <= 3 candidates"
    );
    assert_eq!(run.master_deck.len(), 5);
    assert!(run
        .master_deck
        .iter()
        .any(|card| card.id == CardId::AscendersBane));
    assert!(run
        .master_deck
        .iter()
        .any(|card| card.id == CardId::CurseOfTheBell));
    assert!(run
        .master_deck
        .iter()
        .any(|card| card.id == CardId::Necronomicurse));
    assert!(!run.master_deck.iter().any(|card| card.id == CardId::Strike));
    assert!(!run.master_deck.iter().any(|card| card.id == CardId::Injury));

    let mut pending = crate::state::run::RunState::new(7, 0, false, "Ironclad");
    pending.master_deck.clear();
    for (idx, card_id) in [
        CardId::Strike,
        CardId::Defend,
        CardId::Bash,
        CardId::Pain,
        CardId::AscendersBane,
    ]
    .into_iter()
    .enumerate()
    {
        pending
            .master_deck
            .push(CombatCard::for_scenario(card_id, idx as u32 + 10));
    }

    let Some(crate::state::core::EngineState::RunPendingChoice(choice)) =
        astrolabe::on_equip(&mut pending, crate::state::core::EngineState::MapNavigation)
    else {
        panic!("expected Astrolabe to open a 3-card transform choice");
    };
    assert_eq!(choice.min_choices, 3);
    assert_eq!(choice.max_choices, 3);
    assert_eq!(
        choice.reason,
        crate::state::core::RunPendingChoiceReason::TransformUpgraded
    );
    let request = choice.selection_request(&pending);
    assert!(
        !request
            .targets
            .contains(&crate::state::selection::SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(14)
            )),
        "Java Astrolabe uses getPurgeableCards; Ascender's Bane is not selectable"
    );
}

#[test]
fn astrolabe_auto_upgrade_respects_java_can_upgrade_on_transformed_card() {
    let mut run = crate::state::run::RunState::new(7, 0, false, "Ironclad");
    run.master_deck.clear();
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Injury, 91));

    let next = astrolabe::on_equip(&mut run, crate::state::core::EngineState::MapNavigation);
    assert!(next.is_none());

    let transformed = run
        .emitted_events
        .iter()
        .find_map(|event| match event {
            DomainEvent::CardTransformed {
                before,
                after,
                source: DomainEventSource::Relic(RelicId::Astrolabe),
            } if before.id == CardId::Injury => Some(*after),
            _ => None,
        })
        .expect("Astrolabe should transform the purgeable curse");

    assert_eq!(
        crate::content::cards::get_card_definition(transformed.id).card_type,
        crate::content::cards::CardType::Curse
    );
    assert_eq!(
        transformed.upgrades, 0,
        "Java AbstractDungeon.transformCard(autoUpgrade=true) only upgrades when transformedCard.canUpgrade(); curses cannot upgrade"
    );
    assert!(run
        .master_deck
        .iter()
        .any(|card| card.id == transformed.id && card.upgrades == 0));
}

#[test]
fn calling_bell_uses_screenless_relic_rewards_after_curse_obtain() {
    let mut run = crate::state::run::RunState::new(9, 0, false, "Ironclad");
    run.master_deck
        .push(CombatCard::for_scenario(CardId::PommelStrike, 1001));
    run.common_relic_pool = vec![RelicId::Anchor];
    run.uncommon_relic_pool = vec![RelicId::Pear, RelicId::BottledFlame];
    run.rare_relic_pool = vec![RelicId::Mango];

    let Some(crate::state::core::EngineState::RewardScreen(rewards)) =
        calling_bell::on_equip(&mut run, crate::state::core::EngineState::MapNavigation)
    else {
        panic!("expected Calling Bell reward screen");
    };

    assert!(run
        .master_deck
        .iter()
        .any(|card| card.id == CardId::CurseOfTheBell));
    let relic_rewards = rewards
        .items
        .iter()
        .filter_map(|item| match item {
            crate::rewards::state::RewardItem::Relic { relic_id } => Some(*relic_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        relic_rewards,
        vec![RelicId::Anchor, RelicId::Pear, RelicId::Mango]
    );
}

#[test]
fn calling_bell_curse_uses_fast_obtain_hooks_before_relic_rewards() {
    let mut run = crate::state::run::RunState::new(9, 0, false, "Ironclad");
    run.relics.push(RelicState::new(RelicId::CeramicFish));
    run.common_relic_pool = vec![RelicId::Anchor];
    run.uncommon_relic_pool = vec![RelicId::Pear];
    run.rare_relic_pool = vec![RelicId::Mango];

    let Some(crate::state::core::EngineState::RewardScreen(_)) =
        calling_bell::on_equip(&mut run, crate::state::core::EngineState::MapNavigation)
    else {
        panic!("expected Calling Bell reward screen");
    };

    let events = run.take_emitted_events();
    let fish_gold_pos = events
        .iter()
        .position(|event| {
            matches!(
                event,
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Relic(RelicId::CallingBell),
                    ..
                }
            )
        })
        .expect("Calling Bell curse should run Ceramic Fish onObtainCard hook");
    let curse_pos = events
        .iter()
        .position(|event| {
            matches!(
                event,
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Relic(RelicId::CallingBell),
                } if card.id == CardId::CurseOfTheBell
            )
        })
        .expect("Calling Bell should obtain Curse of the Bell through FastCardObtainEffect");

    assert!(
        fish_gold_pos < curse_pos,
        "Java openConfirmationGrid queues FastCardObtainEffect, whose update calls relic onObtainCard before Soul.obtain"
    );
}

#[test]
fn calling_bell_curse_can_be_blocked_by_omamori_but_rewards_still_open() {
    let mut run = crate::state::run::RunState::new(9, 0, false, "Ironclad");
    run.relics.push(RelicState::new(RelicId::Omamori));
    run.common_relic_pool = vec![RelicId::Anchor];
    run.uncommon_relic_pool = vec![RelicId::Pear];
    run.rare_relic_pool = vec![RelicId::Mango];

    let Some(crate::state::core::EngineState::RewardScreen(rewards)) =
        calling_bell::on_equip(&mut run, crate::state::core::EngineState::MapNavigation)
    else {
        panic!("expected Calling Bell reward screen");
    };

    assert!(!run
        .master_deck
        .iter()
        .any(|card| card.id == CardId::CurseOfTheBell));
    let omamori = run
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::Omamori)
        .expect("Omamori should be present");
    assert_eq!(omamori.counter, 1);
    assert!(!omamori.used_up);

    let relic_rewards = rewards
        .items
        .iter()
        .filter_map(|item| match item {
            crate::rewards::state::RewardItem::Relic { relic_id } => Some(*relic_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        relic_rewards,
        vec![RelicId::Anchor, RelicId::Pear, RelicId::Mango],
        "Java CallingBell.update opens the three relic rewards after the confirmation screen closes, even if Omamori blocked the curse effect"
    );
    assert!(!run.emitted_events.iter().any(|event| matches!(
        event,
        DomainEvent::CardObtained {
            card,
            source: DomainEventSource::Relic(RelicId::CallingBell),
        } if card.id == CardId::CurseOfTheBell
    )));
}

#[test]
fn boss_relic_passives_affect_rewards_campfires_and_curse_pool() {
    let mut elite = crate::state::run::RunState::new(11, 0, false, "Ironclad");
    elite.relics.push(RelicState::new(RelicId::BlackStar));
    let rewards = crate::rewards::generator::generate_combat_rewards(&mut elite, true, false);
    assert_eq!(
        rewards
            .items
            .iter()
            .filter(|item| matches!(item, crate::rewards::state::RewardItem::Relic { .. }))
            .count(),
        2,
        "Java Black Star adds a second elite relic reward"
    );

    let mut campfire = crate::state::run::RunState::new(11, 0, false, "Ironclad");
    campfire
        .relics
        .push(RelicState::new(RelicId::CoffeeDripper));
    let options = crate::engine::campfire_handler::get_available_options(&campfire);
    assert!(
        !options
            .iter()
            .any(|choice| matches!(choice, crate::state::core::CampfireChoice::Rest)),
        "Java Coffee Dripper disables the normal Rest option"
    );

    let curse_pool = crate::content::cards::get_curse_pool();
    assert!(!curse_pool.contains(&CardId::AscendersBane));
    assert!(!curse_pool.contains(&CardId::CurseOfTheBell));
    assert!(!curse_pool.contains(&CardId::Necronomicurse));
    assert!(!curse_pool.contains(&CardId::Pride));
}

#[test]
fn shared_boss_relic_second_batch_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Ectoplasm), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::EmptyCage), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::FusionHammer), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::PandorasBox), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::PhilosopherStone), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::RunicDome), RelicTier::Boss);

    assert_eq!(energy_master_delta(RelicId::Ectoplasm), 1);
    assert_eq!(energy_master_delta(RelicId::FusionHammer), 1);
    assert_eq!(energy_master_delta(RelicId::PhilosopherStone), 1);
    assert_eq!(energy_master_delta(RelicId::RunicDome), 1);

    assert!(!get_relic_subscriptions(RelicId::Ectoplasm).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::FusionHammer).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::PhilosopherStone).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::PhilosopherStone).on_spawn_monster);
    assert!(!get_relic_subscriptions(RelicId::RunicDome).at_battle_start);
}

#[test]
fn ectoplasm_can_spawn_only_in_act_one_and_blocks_gold_gain() {
    fn boss_spawn_result(act_num: u8) -> RelicId {
        let mut run = crate::state::run::RunState::new(13, 0, false, "Ironclad");
        run.act_num = act_num;
        run.boss_relic_pool = vec![RelicId::Ectoplasm, RelicId::CoffeeDripper];
        run.random_relic_by_tier(RelicTier::Boss)
    }

    assert_eq!(boss_spawn_result(1), RelicId::Ectoplasm);
    assert_eq!(
        boss_spawn_result(2),
        RelicId::CoffeeDripper,
        "Java Ectoplasm.canSpawn rejects actNum > 1"
    );

    let mut run = crate::state::run::RunState::new(13, 0, false, "Ironclad");
    run.gold = 25;
    run.relics.push(RelicState::new(RelicId::Ectoplasm));
    assert_eq!(
        run.change_gold_with_source(50, DomainEventSource::RewardScreen),
        0
    );
    assert_eq!(run.gold, 25);
}

#[test]
fn empty_cage_uses_java_purgeable_cards_and_auto_deletes_two_or_fewer() {
    let mut run = crate::state::run::RunState::new(14, 0, false, "Ironclad");
    run.master_deck = vec![
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::Injury, 2),
        CombatCard::for_scenario(CardId::AscendersBane, 3),
        CombatCard::for_scenario(CardId::CurseOfTheBell, 4),
        CombatCard::for_scenario(CardId::Necronomicurse, 5),
    ];
    run.emitted_events.clear();

    let next = empty_cage::on_equip(&mut run, crate::state::core::EngineState::MapNavigation);
    assert!(
        next.is_none(),
        "Java Empty Cage auto-deletes when there are <= 2 purgeable cards"
    );
    assert_eq!(
        run.master_deck
            .iter()
            .map(|card| card.id)
            .collect::<Vec<_>>(),
        vec![
            CardId::AscendersBane,
            CardId::CurseOfTheBell,
            CardId::Necronomicurse
        ]
    );
    assert_eq!(
        run.emitted_events
            .iter()
            .filter(|event| matches!(
                event,
                DomainEvent::CardRemoved {
                    source: DomainEventSource::Relic(RelicId::EmptyCage),
                    ..
                }
            ))
            .count(),
        2
    );

    let mut pending = crate::state::run::RunState::new(14, 0, false, "Ironclad");
    pending.relics.push(RelicState::new(RelicId::EmptyCage));
    pending.event_state = None;
    pending.master_deck = vec![
        CombatCard::for_scenario(CardId::Strike, 10),
        CombatCard::for_scenario(CardId::Defend, 11),
        CombatCard::for_scenario(CardId::Pain, 12),
        CombatCard::for_scenario(CardId::AscendersBane, 13),
    ];
    pending.emitted_events.clear();
    let Some(crate::state::core::EngineState::RunPendingChoice(choice)) =
        empty_cage::on_equip(&mut pending, crate::state::core::EngineState::MapNavigation)
    else {
        panic!("expected Empty Cage to open a 2-card purge choice");
    };
    assert_eq!(choice.min_choices, 2);
    assert_eq!(choice.max_choices, 2);
    let request = choice.selection_request(&pending);
    assert!(
        !request
            .targets
            .contains(&crate::state::selection::SelectionTargetRef::CardLineage(
                crate::runtime::combat::CardLineageId::constructed_scenario(13)
            )),
        "Java Empty Cage uses getPurgeableCards; Ascender's Bane is not selectable"
    );

    let mut invalid_engine_state =
        crate::state::core::EngineState::RunPendingChoice(choice.clone());
    let mut invalid_combat_state = None;
    assert!(crate::engine::run_loop::tick_run(
        &mut invalid_engine_state,
        &mut pending,
        &mut invalid_combat_state,
        Some(crate::state::core::ClientInput::SubmitSelection(
            crate::state::selection::SelectionResolution {
                scope: crate::state::selection::SelectionScope::Deck,
                selected: vec![crate::state::selection::SelectionTargetRef::CardLineage(
                    crate::runtime::combat::CardLineageId::constructed_scenario(13)
                )],
            },
        )),
    ));
    assert!(matches!(
        invalid_engine_state,
        crate::state::core::EngineState::RunPendingChoice(_)
    ));
    assert_eq!(
        pending.master_deck.len(),
        4,
        "invalid direct input must not remove unpurgeable cards"
    );

    let selected = [10, 11]
        .into_iter()
        .map(crate::runtime::combat::CardLineageId::constructed_scenario)
        .map(crate::state::selection::SelectionTargetRef::CardLineage)
        .collect();
    let mut engine_state = crate::state::core::EngineState::RunPendingChoice(choice);
    let mut combat_state = None;
    assert!(crate::engine::run_loop::tick_run(
        &mut engine_state,
        &mut pending,
        &mut combat_state,
        Some(crate::state::core::ClientInput::SubmitSelection(
            crate::state::selection::SelectionResolution {
                scope: crate::state::selection::SelectionScope::Deck,
                selected,
            },
        )),
    ));
    assert!(matches!(
        engine_state,
        crate::state::core::EngineState::MapNavigation
    ));
    let removed_ids = pending
        .emitted_events
        .iter()
        .filter_map(|event| match event {
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::Relic(RelicId::EmptyCage),
            } => Some(card.id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        removed_ids,
        vec![CardId::Strike, CardId::Defend],
        "Java Empty Cage deletes gridSelectScreen.selectedCards in selected order"
    );
    assert_eq!(
        pending
            .emitted_events
            .iter()
            .filter(|event| matches!(
                event,
                DomainEvent::CardRemoved {
                    source: DomainEventSource::Relic(RelicId::EmptyCage),
                    ..
                }
            ))
            .count(),
        2,
        "pending Empty Cage purge must keep relic source, not generic Selection(Purge)"
    );
}

#[test]
fn pandoras_box_uses_floor_card_random_without_advancing_reward_card_rng() {
    let seed = 2026101811;
    let mut run = crate::state::run::RunState::new(seed, 20, true, "Silent");
    run.rng_pool.generate_floor_seeds(seed, 34);
    for _ in 0..17 {
        run.rng_pool.card_rng.random(99);
    }
    let mut expected = run.rng_pool.clone();
    let pool = [
        crate::content::cards::CardRarity::Common,
        crate::content::cards::CardRarity::Uncommon,
        crate::content::cards::CardRarity::Rare,
    ]
    .into_iter()
    .flat_map(|rarity| {
        crate::engine::campfire_handler::source_card_pool_for_class(run.player_class, rarity)
    })
    .collect::<Vec<_>>();
    assert_eq!(pool.len(), 71);
    assert_eq!(
        expected.card_random_rng.clone().random(70),
        45,
        "original F34 Pandora evidence: floor.card_random#0 RandomInt(range70)=45"
    );
    let count = run
        .master_deck
        .iter()
        .filter(|c| crate::content::cards::is_starter_basic(c.id))
        .count();
    let products = (0..count)
        .map(|_| pool[expected.card_random_rng.random(70) as usize])
        .collect::<Vec<_>>();
    run.emitted_events.clear();
    pandoras_box::on_equip(&mut run);
    assert_eq!(
        run.rng_pool, expected,
        "Pandora must consume only its floor-scoped generation RNG"
    );
    let obtained = run
        .emitted_events
        .iter()
        .filter_map(|event| match event {
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Relic(RelicId::PandorasBox),
            } => Some(card.id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(obtained, products.into_iter().rev().collect::<Vec<_>>());
}

#[test]
fn pandoras_box_replaces_only_starter_strike_defend_with_relic_source() {
    let mut run = crate::state::run::RunState::new(15, 0, false, "Ironclad");
    run.emitted_events.clear();

    let deck_len = run.master_deck.len();
    let results = pandoras_box::on_equip(&mut run);
    assert_eq!(results.len(), 9);
    assert_eq!(run.master_deck.len(), deck_len);
    assert!(run.master_deck.iter().any(|card| card.id == CardId::Bash));
    assert!(!run
        .master_deck
        .iter()
        .any(|card| crate::content::cards::is_starter_basic(card.id)));

    assert_eq!(
        run.emitted_events
            .iter()
            .filter(|event| matches!(
                event,
                DomainEvent::CardRemoved {
                    source: DomainEventSource::Relic(RelicId::PandorasBox),
                    ..
                }
            ))
            .count(),
        9
    );
    assert_eq!(
        run.emitted_events
            .iter()
            .filter(|event| matches!(
                event,
                DomainEvent::CardObtained {
                    source: DomainEventSource::Relic(RelicId::PandorasBox),
                    ..
                }
            ))
            .count(),
        9
    );

    let obtained_names = run
        .emitted_events
        .iter()
        .filter_map(|event| match event {
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Relic(RelicId::PandorasBox),
            } => Some(
                crate::content::cards::get_card_definition(card.id)
                    .name
                    .to_string(),
            ),
            _ => None,
        })
        .collect::<Vec<_>>();
    let generated_names = results
        .iter()
        .map(|(_, new_name)| new_name.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        obtained_names,
        generated_names.into_iter().rev().collect::<Vec<_>>(),
        "Java PandorasBox puts generated cards into the confirmation grid with CardGroup.addToBottom, so FastCardObtainEffect obtains them in reverse generation order"
    );
}

#[test]
fn pandoras_box_generated_cards_use_preview_upgrades_then_fast_obtain_hooks() {
    let mut run = crate::state::run::RunState::new(15, 0, false, "Ironclad");
    run.relics.push(RelicState::new(RelicId::MoltenEgg));
    run.relics.push(RelicState::new(RelicId::ToxicEgg));
    run.relics.push(RelicState::new(RelicId::FrozenEgg));
    run.relics.push(RelicState::new(RelicId::CeramicFish));
    run.emitted_events.clear();

    let results = pandoras_box::on_equip(&mut run);
    assert_eq!(results.len(), 9);

    let obtained = run
        .emitted_events
        .iter()
        .filter_map(|event| match event {
            DomainEvent::CardObtained {
                card,
                source: DomainEventSource::Relic(RelicId::PandorasBox),
            } => Some(*card),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(obtained.len(), 9);
    assert!(
        obtained.iter().all(|card| card.upgrades == 1),
        "Java PandorasBox previews each generated card through Egg relics before the confirmation grid queues FastCardObtainEffect"
    );
    assert_eq!(
        run.emitted_events
            .iter()
            .filter(|event| matches!(
                event,
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Relic(RelicId::PandorasBox),
                    ..
                }
            ))
            .count(),
        9,
        "Java confirmation grid obtains each generated card through FastCardObtainEffect, so Ceramic Fish still fires once per obtained card"
    );
}

#[test]
fn fusion_hammer_blocks_only_normal_smith_option() {
    let mut run = crate::state::run::RunState::new(16, 0, false, "Ironclad");
    let baseline = crate::engine::campfire_handler::get_available_options(&run);
    assert!(baseline
        .iter()
        .any(|choice| matches!(choice, crate::state::core::CampfireChoice::Smith(_))));

    run.relics.push(RelicState::new(RelicId::FusionHammer));
    let blocked = crate::engine::campfire_handler::get_available_options(&run);
    assert!(!blocked
        .iter()
        .any(|choice| matches!(choice, crate::state::core::CampfireChoice::Smith(_))));
    assert!(blocked.contains(&crate::state::core::CampfireChoice::Rest));
}

#[test]
fn philosopher_stone_strength_matches_java_battle_and_spawn_hooks() {
    let mut state = crate::test_support::blank_test_combat();
    let mut alive = crate::test_support::test_monster(EnemyId::JawWorm);
    alive.id = 41;
    let mut dying = crate::test_support::test_monster(EnemyId::JawWorm);
    dying.id = 42;
    dying.is_dying = true;
    state.entities.monsters = vec![alive, dying];

    let actions = philosopher_stone::at_battle_start(&state);
    assert_eq!(
        actions.len(),
        2,
        "Java iterates every monster in AbstractDungeon.getMonsters().monsters"
    );
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            source: 41,
            target: 41,
            power_id: PowerId::Strength,
            amount: 1
        }
    ));
    assert!(matches!(
        actions[1].action,
        Action::ApplyPower {
            source: 42,
            target: 42,
            power_id: PowerId::Strength,
            amount: 1
        }
    ));

    let mut spawn_state = crate::test_support::blank_test_combat();
    spawn_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::PhilosopherStone));
    spawn_state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    hooks::on_spawn_monster(&mut spawn_state, 1);
    assert_eq!(
        crate::content::powers::store::power_amount(&spawn_state, 1, PowerId::Strength),
        1,
        "Java PhilosopherStone.onSpawnMonster calls monster.addPower directly instead of queuing ApplyPowerAction"
    );
}

#[test]
fn shared_boss_relic_third_batch_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::SacredBark), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::SlaversCollar), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::SneckoEye), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::Sozu), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::TinyHouse), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::VelvetChoker), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::WristBlade), RelicTier::Boss);

    assert_eq!(energy_master_delta(RelicId::Sozu), 1);
    assert_eq!(energy_master_delta(RelicId::VelvetChoker), 1);
    assert_eq!(energy_master_delta(RelicId::SneckoEye), 0);
    assert_eq!(energy_master_delta(RelicId::SlaversCollar), 0);

    let velvet = get_relic_subscriptions(RelicId::VelvetChoker);
    assert!(velvet.at_battle_start);
    assert!(velvet.at_turn_start);
    assert!(velvet.on_use_card);
    assert!(velvet.on_victory);
    assert!(get_relic_subscriptions(RelicId::SneckoEye).at_pre_battle);
    assert!(get_relic_subscriptions(RelicId::SlaversCollar).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::SlaversCollar).on_victory);
    assert!(!get_relic_subscriptions(RelicId::WristBlade).on_use_card);
}

#[test]
fn velvet_choker_public_counter_matches_java_turn_and_victory_hooks() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::VelvetChoker));
    state.entities.player.relics[0].counter = -1;

    assert!(hooks::at_battle_start(&mut state).is_empty());
    assert_eq!(state.entities.player.relics[0].counter, 0);

    for _ in 0..6 {
        let card = CombatCard::for_scenario(CardId::Strike, 990);
        assert!(hooks::on_use_card(&mut state, &card, Some(1)).is_empty());
    }
    assert_eq!(
        state.entities.player.relics[0].counter, 6,
        "Java VelvetChoker.onPlayCard increments the public relic counter up to the play limit"
    );

    assert!(hooks::on_use_card(
        &mut state,
        &CombatCard::for_scenario(CardId::Strike, 991),
        Some(1)
    )
    .is_empty());
    assert_eq!(
        state.entities.player.relics[0].counter, 6,
        "Java keeps the counter capped once the play limit is reached"
    );

    assert!(hooks::at_turn_start(&mut state).is_empty());
    assert_eq!(
        state.entities.player.relics[0].counter, 0,
        "Java VelvetChoker.atTurnStart resets the public relic counter"
    );

    assert!(hooks::on_victory(&mut state).is_empty());
    assert_eq!(
        state.entities.player.relics[0].counter, -1,
        "Java VelvetChoker.onVictory hides the public counter with -1"
    );
}

#[test]
fn slavers_collar_uses_java_elite_or_boss_detection_and_affects_current_turn_energy() {
    let mut boss_state = crate::test_support::blank_test_combat();
    boss_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::SlaversCollar));
    boss_state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::SlimeBoss));

    let actions = hooks::at_battle_start(&mut boss_state);
    assert!(actions.is_empty());
    assert_eq!(boss_state.entities.player.energy_master, 4);
    assert_eq!(
        boss_state.turn.energy, 4,
        "Java beforeEnergyPrep increments energyMaster before first-turn energy is prepared"
    );
    assert_eq!(boss_state.entities.player.relics[0].counter, -1);
    assert_eq!(boss_state.entities.player.relics[0].amount, 1);

    assert!(hooks::on_victory(&mut boss_state).is_empty());
    assert_eq!(boss_state.entities.player.energy_master, 3);
    assert_eq!(boss_state.entities.player.relics[0].counter, -1);
    assert_eq!(boss_state.entities.player.relics[0].amount, 0);
    assert!(hooks::on_victory(&mut boss_state).is_empty());
    assert_eq!(boss_state.entities.player.energy_master, 3);

    let mut hallway_state = crate::test_support::blank_test_combat();
    hallway_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::SlaversCollar));
    hallway_state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    hooks::at_battle_start(&mut hallway_state);
    assert_eq!(hallway_state.entities.player.energy_master, 3);
    assert_eq!(hallway_state.turn.energy, 3);
    assert_eq!(hallway_state.entities.player.relics[0].counter, -1);
    assert_eq!(hallway_state.entities.player.relics[0].amount, 0);
    assert!(hooks::on_victory(&mut hallway_state).is_empty());
    assert_eq!(hallway_state.entities.player.energy_master, 3);

    hallway_state.meta.is_elite_fight = true;
    hooks::at_battle_start(&mut hallway_state);
    assert_eq!(hallway_state.entities.player.energy_master, 4);
    assert_eq!(hallway_state.entities.player.relics[0].counter, -1);
    assert_eq!(hallway_state.entities.player.relics[0].amount, 1);
    let persistent = persistent_state::post_combat_persistent_relic_state_v1(
        &hallway_state.entities.player.relics[0],
    );
    assert_eq!(persistent.counter, -1);
    assert_eq!(persistent.amount, 0);
    hallway_state.entities.player.energy_master = 99;
    hallway_state.entities.player.relics[0].amount = 0;
    restore_combat_energy_master(&mut hallway_state);
    assert_eq!(hallway_state.entities.player.energy_master, 4);
    assert_eq!(hallway_state.entities.player.relics[0].counter, -1);
    assert_eq!(hallway_state.entities.player.relics[0].amount, 1);
}

#[test]
fn tiny_house_uses_reward_screen_for_gold_potion_and_card_reward() {
    let mut run = crate::state::run::RunState::new(18, 0, false, "Ironclad");
    run.current_hp = 50;
    run.max_hp = 80;
    let starting_gold = run.gold;
    run.emitted_events.clear();

    let Some(crate::state::core::EngineState::RewardScreen(rewards)) =
        tiny_house::on_equip(&mut run)
    else {
        panic!("expected Tiny House to open a reward screen");
    };

    assert_eq!(run.max_hp, 85);
    assert_eq!(run.current_hp, 55);
    assert_eq!(
        run.gold, starting_gold,
        "Java Tiny House adds gold to rewards; it is not gained on equip"
    );
    assert_eq!(
        run.emitted_events
            .iter()
            .filter(|event| matches!(
                event,
                DomainEvent::CardUpgraded {
                    source: DomainEventSource::Relic(RelicId::TinyHouse),
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(run.emitted_events.iter().any(|event| matches!(
        event,
        DomainEvent::MaxHpChanged {
            delta: 5,
            source: DomainEventSource::Relic(RelicId::TinyHouse),
            ..
        }
    )));

    assert!(rewards
        .items
        .iter()
        .any(|item| matches!(item, crate::rewards::state::RewardItem::Gold { amount: 50 })));
    assert!(rewards
        .items
        .iter()
        .any(|item| matches!(item, crate::rewards::state::RewardItem::Potion { .. })));
    assert!(rewards.items.iter().any(|item| matches!(
        item,
        crate::rewards::state::RewardItem::Card { cards } if !cards.is_empty()
    )));
}

#[test]
fn tiny_house_upgrade_candidates_use_java_can_upgrade_filter() {
    let mut run = crate::state::run::RunState::new(18, 0, false, "Ironclad");
    run.master_deck = vec![
        CombatCard::for_scenario(CardId::Wound, 9101),
        CombatCard::for_scenario(CardId::Injury, 9102),
    ];
    run.emitted_events.clear();

    let Some(crate::state::core::EngineState::RewardScreen(_)) = tiny_house::on_equip(&mut run)
    else {
        panic!("expected Tiny House to open a reward screen");
    };

    assert!(run
        .emitted_events
        .iter()
        .all(|event| !matches!(event, DomainEvent::CardUpgraded { .. })));
    assert_eq!(run.master_deck[0].upgrades, 0);
    assert_eq!(run.master_deck[1].upgrades, 0);
}

#[test]
fn wrist_blade_adds_four_damage_to_java_zero_cost_attacks_only() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::WristBlade));

    let mut zero_for_turn = CombatCard::for_scenario(CardId::Strike, 701);
    zero_for_turn.cost_for_turn = Some(0);
    assert_eq!(
        hooks::modify_player_attack_damage_for_card(&state, &zero_for_turn, 6.0),
        10.0
    );

    let mut free_non_x = CombatCard::for_scenario(CardId::Bash, 702);
    free_non_x.free_to_play_once = true;
    assert_eq!(
        hooks::modify_player_attack_damage_for_card(&state, &free_non_x, 8.0),
        12.0
    );

    let mut free_x = CombatCard::for_scenario(CardId::Whirlwind, 703);
    free_x.free_to_play_once = true;
    assert_eq!(
        hooks::modify_player_attack_damage_for_card(&state, &free_x, 5.0),
        5.0,
        "Java excludes free-to-play X-cost cards unless costForTurn is actually 0"
    );

    let mut zero_skill = CombatCard::for_scenario(CardId::Defend, 704);
    zero_skill.cost_for_turn = Some(0);
    assert_eq!(
        hooks::modify_player_attack_damage_for_card(&state, &zero_skill, 0.0),
        0.0
    );
}

#[test]
fn shared_shop_special_relic_gap_batch_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Abacus), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::BloodyIdol), RelicTier::Special);
    assert_eq!(get_relic_tier(RelicId::Cauldron), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::ChemicalX), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::Circlet), RelicTier::Special);
    assert_eq!(get_relic_tier(RelicId::RedCirclet), RelicTier::Special);
    assert_eq!(
        get_relic_tier(RelicId::DiscerningMonocle),
        RelicTier::Uncommon,
        "Java DiscerningMonocle constructor is UNCOMMON, not SHOP"
    );
    assert!(
        !build_relic_pool(RelicTier::Uncommon, "Ironclad").contains(&RelicId::DiscerningMonocle),
        "RelicLibrary.initialize never registers DiscerningMonocle, so its constructor tier must not put it in normal pools"
    );

    assert!(get_relic_subscriptions(RelicId::Abacus).on_shuffle);
    assert!(get_relic_subscriptions(RelicId::ChemicalX).on_calculate_x_cost);
    assert!(!get_relic_subscriptions(RelicId::BloodyIdol).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::Cauldron).at_battle_start);
    assert_eq!(RelicState::new(RelicId::Circlet).counter, 1);
    assert_eq!(RelicState::new(RelicId::RedCirclet).counter, -1);
    assert!(!get_relic_subscriptions(RelicId::RedCirclet).at_battle_start);
}

#[test]
fn deprecated_dodecahedron_triggers_energy_at_turn_start_only_like_java_source() {
    assert_eq!(get_relic_tier(RelicId::Dodecahedron), RelicTier::Deprecated);

    let sub = get_relic_subscriptions(RelicId::Dodecahedron);
    assert!(
        !sub.at_battle_start,
        "Java DEPRECATEDDodecahedron.atBattleStart is UI pulse only"
    );
    assert!(sub.at_turn_start);

    let mut full_hp_then_damaged = crate::test_support::blank_test_combat();
    full_hp_then_damaged.entities.player.max_hp = 80;
    full_hp_then_damaged.entities.player.current_hp = 80;
    full_hp_then_damaged
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Dodecahedron));

    assert!(hooks::at_battle_start(&mut full_hp_then_damaged).is_empty());
    let actions = hooks::at_turn_start(&mut full_hp_then_damaged);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert_eq!(actions[0].action, Action::DodecahedronTurnStartCheck);
    full_hp_then_damaged.queue_actions(actions);
    full_hp_then_damaged.entities.player.current_hp = 79;
    drain_test_actions(&mut full_hp_then_damaged);
    assert_eq!(
        full_hp_then_damaged.turn.energy, 3,
        "Java Dodecahedron checks full HP when its queued anonymous action executes, not when atTurnStart queues it"
    );

    let mut damaged_then_healed = crate::test_support::blank_test_combat();
    damaged_then_healed.entities.player.max_hp = 80;
    damaged_then_healed.entities.player.current_hp = 79;
    damaged_then_healed
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Dodecahedron));
    let actions = hooks::at_turn_start(&mut damaged_then_healed);
    assert_eq!(actions.len(), 1);
    damaged_then_healed.queue_actions(actions);
    damaged_then_healed.entities.player.current_hp = 80;
    drain_test_actions(&mut damaged_then_healed);
    assert_eq!(damaged_then_healed.turn.energy, 4);
}

#[test]
fn abacus_grants_six_block_on_shuffle() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Abacus));

    let actions = hooks::on_shuffle(&mut state);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 6
        }
    ));
}

#[test]
fn bloody_idol_heals_from_run_level_gold_gain_unless_ectoplasm_blocks_gold() {
    let mut run = crate::state::run::RunState::new(19, 0, false, "Ironclad");
    run.current_hp = 40;
    run.max_hp = 80;
    run.relics.push(RelicState::new(RelicId::BloodyIdol));
    run.emitted_events.clear();

    assert_eq!(
        run.change_gold_with_source(50, DomainEventSource::RewardScreen),
        50
    );
    assert_eq!(run.gold, 149);
    assert_eq!(run.current_hp, 45);
    assert!(run.emitted_events.iter().any(|event| matches!(
        event,
        DomainEvent::HpChanged {
            delta: 5,
            source: DomainEventSource::Relic(RelicId::BloodyIdol),
            ..
        }
    )));

    let mut blocked = crate::state::run::RunState::new(20, 0, false, "Ironclad");
    blocked.current_hp = 40;
    blocked.relics.push(RelicState::new(RelicId::BloodyIdol));
    blocked.relics.push(RelicState::new(RelicId::Ectoplasm));
    blocked.emitted_events.clear();

    assert_eq!(
        blocked.change_gold_with_source(50, DomainEventSource::RewardScreen),
        0
    );
    assert_eq!(blocked.gold, 99);
    assert_eq!(blocked.current_hp, 40);
    assert!(blocked.emitted_events.is_empty());
}

#[test]
fn bloody_idol_combat_gold_heals_immediately_once_with_current_modifiers() {
    for (extra, hp, expected_hp, gain) in [
        (None, 40, 45, 20),
        (Some(RelicId::MagicFlower), 40, 48, 20),
        (Some(RelicId::MarkOfTheBloom), 40, 40, 20),
        (Some(RelicId::Ectoplasm), 40, 40, 0),
        (None, 78, 80, 20),
    ] {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.current_hp = hp;
        state.entities.player.max_hp = 80;
        state.entities.player.gold = 10;
        state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::BloodyIdol));
        if let Some(relic) = extra {
            state.entities.player.add_relic(RelicState::new(relic));
        }
        crate::engine::action_handlers::damage::handle_gain_gold(20, &mut state);
        assert_eq!(state.entities.player.gold, 10 + gain);
        assert_eq!(state.entities.player.gold_delta_this_combat, gain);
        assert_eq!(
            state.entities.player.current_hp, expected_hp,
            "Java onGainGold heals inside the callback, before returning: {extra:?}"
        );
        drain_test_actions(&mut state);
        assert_eq!(
            state.entities.player.current_hp, expected_hp,
            "do not heal twice"
        );
        for amount in [0, -1] {
            crate::engine::action_handlers::damage::handle_gain_gold(amount, &mut state);
            drain_test_actions(&mut state);
            assert_eq!(
                (state.entities.player.gold, state.entities.player.current_hp),
                (10 + gain, expected_hp)
            );
        }
    }
}

#[test]
fn bloody_idol_greed_healing_threshold_precedes_last_kill_queue_cleanup() {
    for has_survivor in [true, false] {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.current_hp = 38;
        state.entities.player.max_hp = 80;
        state.entities.player.gold = 10;
        for relic in [RelicId::BloodyIdol, RelicId::RedSkull] {
            state.entities.player.add_relic(RelicState::new(relic));
        }
        let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
        target.id = 41;
        target.current_hp = 5;
        state.entities.monsters = vec![target];
        if has_survivor {
            let mut survivor = crate::test_support::test_monster(EnemyId::JawWorm);
            survivor.id = 42;
            state.entities.monsters.push(survivor);
        }
        let start = hooks::at_battle_start(&mut state);
        state.queue_actions(start);
        drain_test_actions(&mut state);
        assert_eq!(store::power_amount(&state, 0, PowerId::Strength), 3);
        crate::engine::action_handlers::damage::handle_hand_of_greed(
            41,
            DamageInfo {
                source: 0,
                target: 41,
                base: 10,
                output: 10,
                damage_type: DamageType::Normal,
                is_modified: false,
            },
            20,
            &mut state,
        );
        drain_test_actions(&mut state);
        assert_eq!(
            (state.entities.player.gold, state.entities.player.current_hp),
            (30, 43)
        );
        assert_eq!(
            store::power_amount(&state, 0, PowerId::Strength),
            if has_survivor { 0 } else { 3 },
            "Java healing queues Red Skull before GreedAction clears post-combat powers"
        );
        assert!(
            !state
                .entities
                .player
                .relics
                .iter()
                .find(|r| r.id == RelicId::RedSkull)
                .unwrap()
                .used_up
        );
    }
}

#[test]
fn cauldron_opens_potion_reward_screen_and_removes_auto_card_reward() {
    let mut run = crate::state::run::RunState::new(21, 0, false, "Ironclad");
    let mut existing_rewards = crate::rewards::state::RewardState::new();
    existing_rewards
        .items
        .push(crate::rewards::state::RewardItem::Gold { amount: 12 });
    existing_rewards
        .items
        .push(crate::rewards::state::RewardItem::Relic {
            relic_id: RelicId::Akabeko,
        });

    let Some(crate::state::core::EngineState::RewardScreen(rewards)) = cauldron::on_equip(
        &mut run,
        crate::state::core::EngineState::RewardScreen(existing_rewards),
    ) else {
        panic!("expected Cauldron to open a reward screen");
    };

    assert_eq!(
        rewards
            .items
            .iter()
            .filter(|item| matches!(item, crate::rewards::state::RewardItem::Potion { .. }))
            .count(),
        5
    );
    assert!(!rewards
        .items
        .iter()
        .any(|item| matches!(item, crate::rewards::state::RewardItem::Card { .. })));
    assert!(rewards
        .items
        .iter()
        .any(|item| matches!(item, crate::rewards::state::RewardItem::Gold { amount: 12 })));
    assert!(rewards.items.iter().any(|item| matches!(
        item,
        crate::rewards::state::RewardItem::Relic {
            relic_id: RelicId::Akabeko
        }
    )));
}

#[test]
fn cauldron_burns_standard_card_reward_rng_before_removing_it_like_java_open() {
    let mut run = crate::state::run::RunState::new(21, 0, false, "Ironclad");
    let card_rng_before = run.rng_pool.card_rng.counter;

    let Some(crate::state::core::EngineState::RewardOverlay {
        reward_state: rewards,
        return_state,
    }) = cauldron::on_equip(&mut run, crate::state::core::EngineState::MapNavigation)
    else {
        panic!("expected Cauldron to open a reward overlay");
    };

    assert!(
        run.rng_pool.card_rng.counter > card_rng_before,
        "Java CombatRewardScreen.open() creates the ordinary card reward before Cauldron removes it"
    );
    assert!(rewards
        .items
        .iter()
        .all(|item| !matches!(item, crate::rewards::state::RewardItem::Card { .. })));
    assert!(matches!(
        *return_state,
        crate::state::core::EngineState::MapNavigation
    ));
}

#[test]
fn chemical_x_adds_two_to_x_cost_amount() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::ChemicalX));

    assert_eq!(hooks::on_calculate_x_cost(&state, 0), 2);
    assert_eq!(hooks::on_calculate_x_cost(&state, 3), 5);
}

#[test]
fn circlet_duplicate_obtain_increments_existing_counter_instead_of_adding_copy() {
    let mut run = crate::state::run::RunState::new(22, 0, false, "Ironclad");
    assert!(run
        .obtain_relic_with_source(
            RelicId::Circlet,
            crate::state::core::EngineState::MapNavigation,
            DomainEventSource::RewardScreen,
        )
        .is_none());
    assert_eq!(
        run.relics
            .iter()
            .filter(|relic| relic.id == RelicId::Circlet)
            .count(),
        1
    );
    let circlet = run
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::Circlet)
        .expect("Circlet should be obtained");
    assert_eq!(circlet.counter, 1);

    assert!(run
        .obtain_relic_with_source(
            RelicId::Circlet,
            crate::state::core::EngineState::MapNavigation,
            DomainEventSource::RewardScreen,
        )
        .is_none());
    assert_eq!(
        run.relics
            .iter()
            .filter(|relic| relic.id == RelicId::Circlet)
            .count(),
        1
    );
    let circlet = run
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::Circlet)
        .expect("Circlet should still be a single relic");
    assert_eq!(circlet.counter, 2);
    assert_eq!(
        run.emitted_events
            .iter()
            .filter(|event| matches!(
                event,
                DomainEvent::RelicObtained {
                    relic_id: RelicId::Circlet,
                    ..
                }
            ))
            .count(),
        2
    );
}

#[test]
fn empty_boss_relic_pool_returns_red_circlet_like_java_sources() {
    let mut front = crate::state::run::RunState::new(23, 0, false, "Ironclad");
    front.boss_relic_pool.clear();
    assert_eq!(
        front.random_relic_by_tier(RelicTier::Boss),
        RelicId::RedCirclet,
        "Java AbstractDungeon.returnRandomRelicKey(BOSS) returns Red Circlet when bossRelicPool is empty"
    );

    let mut end = crate::state::run::RunState::new(23, 0, false, "Ironclad");
    end.boss_relic_pool.clear();
    assert_eq!(
        end.random_relic_end_by_tier(RelicTier::Boss),
        RelicId::RedCirclet,
        "Java AbstractDungeon.returnEndRandomRelicKey(BOSS) also returns Red Circlet when bossRelicPool is empty"
    );
}

#[test]
fn shared_event_special_relic_gap_batch_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::DollysMirror), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::Enchiridion), RelicTier::Special);
    assert_eq!(get_relic_tier(RelicId::FaceOfCleric), RelicTier::Special);
    assert_eq!(get_relic_tier(RelicId::GremlinMask), RelicTier::Special);

    assert!(get_relic_subscriptions(RelicId::Enchiridion).at_pre_battle);
    assert!(get_relic_subscriptions(RelicId::FaceOfCleric).on_victory);
    assert!(get_relic_subscriptions(RelicId::GremlinMask).at_battle_start);
}

#[test]
fn dollys_mirror_opens_duplicate_selection_when_deck_has_cards() {
    let mut run = crate::state::run::RunState::new(23, 0, false, "Ironclad");
    run.master_deck = vec![CombatCard::for_scenario(CardId::Strike, 101)];

    let Some(crate::state::core::EngineState::RunPendingChoice(choice)) =
        dollys_mirror::on_equip(&mut run, crate::state::core::EngineState::MapNavigation)
    else {
        panic!("Dolly's Mirror should open a duplicate selection");
    };

    assert_eq!(choice.min_choices, 1);
    assert_eq!(choice.max_choices, 1);
    assert_eq!(
        choice.reason,
        crate::state::core::RunPendingChoiceReason::Duplicate
    );
}

#[test]
fn enchiridion_adds_random_zero_cost_power_at_pre_battle() {
    let mut state = crate::test_support::blank_test_combat();
    let mut relic = RelicState::new(RelicId::Enchiridion);

    assert_eq!(state.rng.card_random_rng.counter, 0);
    let actions = enchiridion::at_battle_start(&mut state, &mut relic);
    assert_eq!(actions.len(), 1);
    assert_eq!(
        state.rng.card_random_rng.counter, 1,
        "Java Enchiridion.atPreBattle samples the random Power before queuing MakeTempCardInHandAction"
    );
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    match &actions[0].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(*amount, 1);
            let def = crate::content::cards::get_card_definition(original.id);
            assert_eq!(def.card_type, crate::content::cards::CardType::Power);
            if original.combat_cost_without_turn_override_java() != -1 {
                assert_eq!(original.cost_for_turn_java(), 0);
            }
        }
        other => panic!("Enchiridion should queue a concrete generated Power, got {other:?}"),
    }

    state.entities.player.add_relic(relic);
    assert!(get_relic_subscriptions(RelicId::Enchiridion).at_pre_battle);
}

#[test]
fn enchiridion_applies_make_temp_card_constructor_reality_before_queue_execution() {
    let mut state = crate::test_support::blank_test_combat();
    store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::MasterRealityPower,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let mut relic = RelicState::new(RelicId::Enchiridion);
    let actions = enchiridion::at_battle_start(&mut state, &mut relic);
    let Action::MakeConstructedCopyInHand { original, .. } = &actions[0].action else {
        panic!("Enchiridion should queue constructed MakeTempCardInHandAction payload");
    };
    assert_eq!(
        original.upgrades, 1,
        "Java MakeTempCardInHandAction constructor applies Master Reality before the action later executes"
    );

    store::set_powers_for(&mut state, 0, vec![]);
    state.queue_action_back(actions[0].action.clone());
    drain_test_actions(&mut state);
    assert_eq!(
        state.zones.hand[0].upgrades, 1,
        "Removing Master Reality before the queued action executes must not erase the constructor-time upgrade"
    );
}

#[test]
fn face_of_cleric_gains_one_max_hp_on_victory() {
    let state = crate::test_support::blank_test_combat();
    let mut relic = RelicState::new(RelicId::FaceOfCleric);

    let actions = face_of_cleric::on_victory(&state, &mut relic);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(actions[0].action, Action::GainMaxHp { amount: 1 }));
}

#[test]
fn gremlin_mask_applies_one_weak_to_player_at_battle_start() {
    let state = crate::test_support::blank_test_combat();

    let actions = gremlin_mask::at_battle_start(&state, &state.entities.player);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Weak,
            amount: 1
        }
    ));
}

#[test]
fn shared_shop_relic_gap_batch_two_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::MembershipCard), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::Orrery), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::MedicalKit), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::OrangePellets), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::Sling), RelicTier::Shop);

    assert!(get_relic_subscriptions(RelicId::MedicalKit).on_use_card);
    let pellets = get_relic_subscriptions(RelicId::OrangePellets);
    assert!(pellets.at_turn_start);
    assert!(pellets.on_use_card);
    assert!(get_relic_subscriptions(RelicId::Sling).at_battle_start);
}

#[test]
fn warped_tongs_triggers_after_turn_start_draw_like_java_sources() {
    assert_eq!(get_relic_tier(RelicId::WarpedTongs), RelicTier::Special);
    let subs = get_relic_subscriptions(RelicId::WarpedTongs);
    assert!(
        !subs.at_turn_start,
        "Java WarpedTongs implements atTurnStartPostDraw, not atTurnStart"
    );
    assert!(subs.at_turn_start_post_draw);

    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::WarpedTongs));

    assert!(
        hooks::at_turn_start(&mut state).is_empty(),
        "Warped Tongs must not run before the start-of-turn draw"
    );
    let actions = hooks::at_turn_start_post_draw(&mut state);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(actions[0].action, Action::UpgradeRandomCard));
}

#[test]
fn medical_kit_allows_status_cards_to_be_played() {
    let burn = CombatCard::for_scenario(CardId::Burn, 901);
    let mut state = crate::test_support::blank_test_combat();
    assert!(crate::content::cards::can_play_card(&burn, &state).is_err());

    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::MedicalKit));
    assert!(crate::content::cards::can_play_card(&burn, &state).is_ok());
}

#[test]
fn orange_pellets_uses_remove_all_debuffs_action_and_resets_combo_counter() {
    let mut relic = RelicState::new(RelicId::OrangePellets);
    let first = orange_pellets::on_use_card(CardId::Strike, &mut relic);
    assert!(first.is_empty());
    assert_eq!(relic.counter, 0b001);

    relic.counter = 0b011;
    let actions = orange_pellets::on_use_card(CardId::Inflame, &mut relic);
    assert_eq!(relic.counter, 0);

    assert!(actions
        .iter()
        .any(|info| matches!(info.action, Action::RemoveAllDebuffs { target: 0 })));
    assert_eq!(actions.len(), 1);
}

#[test]
fn orange_pellets_turn_start_resets_stale_combo_counter_like_java() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::OrangePellets));
    state.entities.player.relics[0].counter = 0b011;

    assert!(hooks::at_turn_start(&mut state).is_empty());
    assert_eq!(
        state.entities.player.relics[0].counter, 0,
        "Java OrangePellets.atTurnStart clears the attack/skill/power combo flags"
    );
}

#[test]
fn orange_pellets_initial_battle_start_resets_cross_combat_combo_counter_like_java() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::OrangePellets));
    state.entities.player.relics[0].counter = 0b011;

    crate::engine::action_handlers::cards::handle_battle_start_pre_draw_trigger(&mut state);
    assert_eq!(
        state.entities.player.relics[0].counter, 0,
        "Java initial combat calls relic atTurnStart, so stale Orange Pellets state from the previous combat must not survive into the first turn"
    );
}

#[test]
fn sling_only_grants_strength_in_elite_combats() {
    let hallway = crate::test_support::blank_test_combat();
    assert!(sling::at_battle_start(&hallway).is_empty());

    let mut elite = crate::test_support::blank_test_combat();
    elite.meta.is_elite_fight = true;
    let actions = sling::at_battle_start(&elite);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            target: 0,
            power_id: PowerId::Strength,
            amount: 2,
            ..
        }
    ));
}

#[test]
fn defect_orb_relic_gap_batch_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::CloakClasp), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::CrackedCore), RelicTier::Starter);
    assert_eq!(get_relic_tier(RelicId::Damaru), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::DataDisk), RelicTier::Common);
    assert_eq!(
        get_relic_tier(RelicId::GoldPlatedCables),
        RelicTier::Uncommon
    );
    assert_eq!(get_relic_tier(RelicId::EmotionChip), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::FrozenCore), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::HandDrill), RelicTier::Shop);

    assert!(get_relic_subscriptions(RelicId::CloakClasp).at_end_of_turn);
    assert!(get_relic_subscriptions(RelicId::CrackedCore).at_pre_battle);
    assert!(get_relic_subscriptions(RelicId::Damaru).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::DataDisk).at_battle_start);

    let emotion_chip = get_relic_subscriptions(RelicId::EmotionChip);
    assert!(emotion_chip.at_turn_start);
    assert!(emotion_chip.on_lose_hp);
    assert!(emotion_chip.on_victory);

    assert!(get_relic_subscriptions(RelicId::FrozenCore).at_end_of_turn);
    assert!(
        !get_relic_subscriptions(RelicId::GoldPlatedCables).at_end_of_turn,
        "Java Gold-Plated Cables is read by orb trigger code, not a relic callback"
    );
}

#[test]
fn cracked_core_channels_lightning_from_pre_battle_hook() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::CrackedCore));

    let actions = hooks::at_pre_battle(&mut state);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        actions[0].action,
        Action::ChannelOrb(OrbId::Lightning)
    ));
}

#[test]
fn end_turn_marker_triggers_cloak_clasp_once() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::CloakClasp));
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 11),
        CombatCard::for_scenario(CardId::Defend, 12),
    ];

    crate::engine::action_handlers::execute_action(Action::EndTurnTrigger, &mut state);
    drain_test_actions(&mut state);

    assert_eq!(
        state.entities.player.block, 2,
        "Cloak Clasp must not fire once from core and once from EndTurnTrigger"
    );
}

#[test]
fn channeling_into_full_orb_slots_evokes_oldest_before_channeling_new_orb() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.max_orbs = 1;
    state.entities.player.orbs = vec![OrbEntity::new(OrbId::Frost)];

    crate::engine::action_handlers::execute_action(
        Action::ChannelOrb(OrbId::Lightning),
        &mut state,
    );
    assert!(matches!(state.pop_next_action(), Some(Action::EvokeOrb)));
    crate::engine::action_handlers::execute_action(Action::EvokeOrb, &mut state);
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 5
        })
    ));
    crate::engine::action_handlers::execute_action(
        Action::GainBlock {
            target: 0,
            amount: 5,
        },
        &mut state,
    );
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning))
    ));
    crate::engine::action_handlers::execute_action(
        Action::ChannelOrb(OrbId::Lightning),
        &mut state,
    );
    drain_test_actions(&mut state);

    assert_eq!(
        state.entities.player.block, 5,
        "full-slot channeling should evoke the old Frost instead of silently deleting it"
    );
    assert_eq!(state.entities.player.orbs.len(), 1);
    assert_eq!(state.entities.player.orbs[0].id, OrbId::Lightning);
}

#[test]
fn orb_passives_fire_from_marker_actions_and_gold_plated_cables_doubles_first_orb() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.max_orbs = 3;
    state.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Frost),
        OrbEntity::new(OrbId::Frost),
        OrbEntity::new(OrbId::Empty),
    ];
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::GoldPlatedCables));

    crate::engine::action_handlers::execute_action(Action::TriggerEndOfTurnOrbs, &mut state);
    drain_test_actions(&mut state);

    assert_eq!(
        state.entities.player.block, 6,
        "two Frost passives plus one extra rightmost passive from Cables"
    );
}

#[test]
fn data_disk_focus_changes_orb_passive_amounts() {
    let mut state =
        crate::test_support::combat_with_monsters(vec![crate::test_support::test_monster(
            EnemyId::JawWorm,
        )]);
    state.entities.player.max_orbs = 3;
    state.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Lightning),
        OrbEntity::new(OrbId::Empty),
        OrbEntity::new(OrbId::Empty),
    ];
    store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Focus,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    crate::engine::action_handlers::execute_action(Action::TriggerEndOfTurnOrbs, &mut state);
    drain_test_actions(&mut state);

    assert_eq!(
        state.entities.monsters[0].current_hp,
        state.entities.monsters[0].max_hp - 4,
        "Lightning passive is 3 plus one Focus"
    );
}

#[test]
fn emotion_chip_impulse_triggers_start_and_end_orb_passives_then_resets_counter() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.max_orbs = 3;
    state.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Plasma),
        OrbEntity::new(OrbId::Frost),
        OrbEntity::new(OrbId::Empty),
    ];
    let mut relic = RelicState::new(RelicId::EmotionChip);
    relic.counter = 1;

    let actions = emotion_chip::at_turn_start(&state, &mut relic);
    assert_eq!(relic.counter, 0);
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0].action, Action::TriggerImpulseOrbs));

    state.queue_actions(actions);
    drain_test_actions(&mut state);
    assert_eq!(state.turn.energy, 4, "Plasma start passive gives 1 energy");
    assert_eq!(
        state.entities.player.block, 2,
        "Frost end passive gives block"
    );
}

#[test]
fn frozen_core_channels_frost_only_when_an_orb_slot_is_empty() {
    let mut full = crate::test_support::blank_test_combat();
    full.entities.player.max_orbs = 1;
    full.entities.player.orbs = vec![OrbEntity::new(OrbId::Lightning)];
    assert!(
        frozen_core::at_end_of_turn(&full, &mut RelicState::new(RelicId::FrozenCore)).is_empty()
    );

    let mut empty = crate::test_support::blank_test_combat();
    empty.entities.player.max_orbs = 1;
    empty.entities.player.orbs = vec![OrbEntity::new(OrbId::Empty)];
    let actions = frozen_core::at_end_of_turn(&empty, &mut RelicState::new(RelicId::FrozenCore));
    assert_eq!(actions.len(), 1);
    assert!(matches!(
        actions[0].action,
        Action::ChannelOrb(OrbId::Frost)
    ));
}

#[test]
fn hand_drill_applies_vulnerable_when_damage_exactly_breaks_block() {
    let mut state =
        crate::test_support::combat_with_monsters(vec![crate::test_support::test_monster(
            EnemyId::JawWorm,
        )]);
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::HandDrill));
    state.entities.monsters[0].block = 5;
    let target = state.entities.monsters[0].id;

    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: 0,
            target,
            base: 5,
            output: 5,
            damage_type: DamageType::Normal,
            is_modified: false,
        }),
        &mut state,
    );
    drain_test_actions(&mut state);

    assert_eq!(state.entities.monsters[0].block, 0);
    assert_eq!(
        state.entities.monsters[0].current_hp,
        state.entities.monsters[0].max_hp
    );
    assert_eq!(store::power_amount(&state, target, PowerId::Vulnerable), 2);
}

#[test]
fn watcher_relic_gap_batch_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::PureWater), RelicTier::Starter);
    assert_eq!(get_relic_tier(RelicId::HolyWater), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::Duality), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::GoldenEye), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::Melange), RelicTier::Shop);

    assert!(get_relic_subscriptions(RelicId::PureWater).at_battle_start_pre_draw);
    assert!(get_relic_subscriptions(RelicId::HolyWater).at_battle_start_pre_draw);
    assert!(get_relic_subscriptions(RelicId::Duality).on_use_card);
    assert!(get_relic_subscriptions(RelicId::GoldenEye).on_scry);
    assert!(get_relic_subscriptions(RelicId::Melange).on_shuffle);
}

#[test]
fn pure_and_holy_water_add_correct_miracle_counts_pre_draw() {
    let state = crate::test_support::blank_test_combat();
    let pure_actions = pure_water::at_battle_start(&state);
    assert_eq!(pure_actions.len(), 1);
    assert_eq!(pure_actions[0].insertion_mode, AddTo::Bottom);
    match &pure_actions[0].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Miracle);
            assert_eq!(*amount, 1);
        }
        other => panic!("Pure Water should queue constructed Miracle copies, got {other:?}"),
    }

    let holy_actions = holy_water::at_battle_start(&state);
    assert_eq!(holy_actions.len(), 1);
    assert_eq!(holy_actions[0].insertion_mode, AddTo::Bottom);
    match &holy_actions[0].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Miracle);
            assert_eq!(*amount, 3);
        }
        other => panic!("Holy Water should queue constructed Miracle copies, got {other:?}"),
    }
}

#[test]
fn duality_grants_temporary_dexterity_only_for_attack_cards() {
    let state = crate::test_support::blank_test_combat();
    let mut relic = RelicState::new(RelicId::Duality);

    let strike_actions = duality::on_use_card(
        &state,
        &mut relic,
        &CombatCard::for_scenario(CardId::Strike, 19001),
    );
    assert_eq!(strike_actions.len(), 2);
    assert!(matches!(
        strike_actions[0].action,
        Action::ApplyPower {
            target: 0,
            power_id: PowerId::Dexterity,
            amount: 1,
            ..
        }
    ));
    assert!(matches!(
        strike_actions[1].action,
        Action::ApplyPower {
            target: 0,
            power_id: PowerId::DexterityDown,
            amount: 1,
            ..
        }
    ));

    assert!(duality::on_use_card(
        &state,
        &mut relic,
        &CombatCard::for_scenario(CardId::Defend, 19002)
    )
    .is_empty());
}

#[test]
fn golden_eye_adds_two_to_scry_amount_and_melange_queues_scry_three_on_shuffle() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::GoldenEye));
    assert_eq!(hooks::on_scry(&state, 3), 5);

    let mut shuffle_state = crate::test_support::blank_test_combat();
    shuffle_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Melange));
    let actions = hooks::on_shuffle(&mut shuffle_state);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(actions[0].action, Action::Scry(3)));
}

#[test]
fn watcher_stance_relic_hooks_match_java_sources() {
    assert_eq!(get_relic_tier(RelicId::TeardropLocket), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::VioletLotus), RelicTier::Boss);

    assert!(get_relic_subscriptions(RelicId::TeardropLocket).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::Damaru).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::VioletLotus).on_change_stance);

    let mut teardrop_state = crate::test_support::blank_test_combat();
    teardrop_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::TeardropLocket));
    let teardrop_actions = hooks::at_battle_start(&mut teardrop_state);
    assert_eq!(teardrop_actions.len(), 1);
    assert_eq!(teardrop_actions[0].insertion_mode, AddTo::Top);
    assert_eq!(
        teardrop_actions[0].action,
        Action::EnterStance("Calm".to_string()),
        "Java TeardropLocket.atBattleStart addToTop queues ChangeStanceAction(\"Calm\"); relic VFX is omitted"
    );

    let mut damaru_state = crate::test_support::blank_test_combat();
    damaru_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Damaru));
    let damaru_actions = hooks::at_turn_start(&mut damaru_state);
    assert_eq!(damaru_actions.len(), 1);
    assert_eq!(damaru_actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(
        damaru_actions[0].action,
        Action::ApplyPower {
            target: 0,
            power_id: PowerId::Mantra,
            amount: 1,
            ..
        }
    ));

    let mut lotus_state = crate::test_support::blank_test_combat();
    lotus_state.entities.player.stance = StanceId::Calm;
    lotus_state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::VioletLotus));
    crate::engine::action_handlers::execute_action(
        Action::EnterStance("Wrath".to_string()),
        &mut lotus_state,
    );
    assert_eq!(lotus_state.entities.player.stance, StanceId::Wrath);
    assert_eq!(
        lotus_state.pop_next_action(),
        Some(Action::GainEnergy { amount: 1 }),
        "Java ChangeStanceAction calls relic onChangeStance before Calm.onExitStance"
    );
    assert_eq!(
        lotus_state.pop_next_action(),
        Some(Action::GainEnergy { amount: 2 }),
        "Java CalmStance.onExitStance then queues its normal 2 energy"
    );
}

#[test]
fn silent_relic_gap_batch_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::SnakeRing), RelicTier::Starter);
    assert_eq!(get_relic_tier(RelicId::RingOfTheSerpent), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::NinjaScroll), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::HoveringKite), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::SneckoSkull), RelicTier::Common);
    assert_eq!(get_relic_tier(RelicId::PaperCrane), RelicTier::Uncommon);

    assert!(get_relic_subscriptions(RelicId::SnakeRing).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::NinjaScroll).at_battle_start_pre_draw);
    assert!(get_relic_subscriptions(RelicId::HoveringKite).at_turn_start);
    assert!(get_relic_subscriptions(RelicId::HoveringKite).on_discard);
    assert!(get_relic_subscriptions(RelicId::SneckoSkull).on_apply_power);
    assert!(
        !get_relic_subscriptions(RelicId::RingOfTheSerpent).at_turn_start,
        "Java atTurnStart only flashes; hand-size effect is passive state"
    );
}

#[test]
fn snake_ring_and_ninja_scroll_start_actions_match_java_counts() {
    let snake_actions = snake_ring::at_battle_start();
    assert_eq!(snake_actions.len(), 1);
    assert_eq!(snake_actions[0].insertion_mode, AddTo::Bottom);
    assert!(matches!(snake_actions[0].action, Action::DrawCards(2)));

    let state = crate::test_support::blank_test_combat();
    let ninja_actions = ninja_scroll::at_battle_start(&state);
    assert_eq!(ninja_actions.len(), 1);
    assert_eq!(ninja_actions[0].insertion_mode, AddTo::Bottom);
    match &ninja_actions[0].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Shiv);
            assert_eq!(*amount, 3);
        }
        other => panic!("Ninja Scroll should queue constructed Shiv copies, got {other:?}"),
    }
}

#[test]
fn ring_of_the_serpent_increases_opening_and_turn_start_draw_count() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RingOfTheSerpent));
    state.reset_turn_energy_from_player();

    assert_eq!(
        crate::engine::core::compute_player_turn_start_draw_count(&state),
        6
    );

    crate::engine::action_handlers::cards::handle_battle_start_pre_draw_trigger(&mut state);
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::DrawCards(6))
    ));
}

#[test]
fn hovering_kite_gains_energy_only_on_first_manual_discard_each_turn() {
    let state = crate::test_support::blank_test_combat();
    let mut relic = RelicState::new(RelicId::HoveringKite);

    let first = hovering_kite::on_discard(&state, &mut relic);
    assert_eq!(first.len(), 1);
    assert!(matches!(first[0].action, Action::GainEnergy { amount: 1 }));
    assert!(hovering_kite::on_discard(&state, &mut relic).is_empty());

    assert!(hovering_kite::at_turn_start(&mut relic).is_empty());
    let next_turn = hovering_kite::on_discard(&state, &mut relic);
    assert_eq!(next_turn.len(), 1);
}

#[test]
fn snecko_skull_adds_one_poison_only_when_player_applies_poison_to_monster() {
    let mut state =
        crate::test_support::combat_with_monsters(vec![crate::test_support::test_monster(
            EnemyId::JawWorm,
        )]);
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::SneckoSkull));
    let target = state.entities.monsters[0].id;

    crate::engine::action_handlers::powers::handle_apply_power(
        0,
        target,
        PowerId::Poison,
        2,
        &mut state,
    );
    assert_eq!(store::power_amount(&state, target, PowerId::Poison), 3);

    crate::engine::action_handlers::powers::handle_apply_power(
        target,
        0,
        PowerId::Poison,
        2,
        &mut state,
    );
    assert_eq!(store::power_amount(&state, 0, PowerId::Poison), 2);
}

#[test]
fn paper_crane_changes_weak_monster_damage_from_75_to_60_percent() {
    let mut normal =
        crate::test_support::combat_with_monsters(vec![crate::test_support::test_monster(
            EnemyId::JawWorm,
        )]);
    let source = normal.entities.monsters[0].id;
    store::set_powers_for(
        &mut normal,
        source,
        vec![Power {
            power_type: PowerId::Weak,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    assert_eq!(
        crate::content::powers::calculate_monster_damage(20, source, 0, &normal),
        15
    );

    normal
        .entities
        .player
        .add_relic(RelicState::new(RelicId::PaperCrane));
    assert_eq!(
        crate::content::powers::calculate_monster_damage(20, source, 0, &normal),
        12
    );
}

#[test]
fn defect_orb_slot_relic_gap_batch_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::NuclearBattery), RelicTier::Boss);
    assert_eq!(get_relic_tier(RelicId::SymbioticVirus), RelicTier::Uncommon);
    assert_eq!(get_relic_tier(RelicId::RunicCapacitor), RelicTier::Shop);
    assert_eq!(get_relic_tier(RelicId::Inserter), RelicTier::Boss);

    assert!(get_relic_subscriptions(RelicId::NuclearBattery).at_pre_battle);
    assert!(get_relic_subscriptions(RelicId::SymbioticVirus).at_pre_battle);

    let runic = get_relic_subscriptions(RelicId::RunicCapacitor);
    assert!(runic.at_pre_battle);
    assert!(runic.at_turn_start);

    assert!(get_relic_subscriptions(RelicId::Inserter).at_turn_start);
    assert_eq!(RelicState::new(RelicId::Inserter).counter, 0);
}

#[test]
fn nuclear_battery_and_symbiotic_virus_channel_expected_orbs_pre_battle() {
    let nuclear_actions = nuclear_battery::at_battle_start();
    assert_eq!(nuclear_actions.len(), 1);
    assert!(matches!(
        nuclear_actions[0].action,
        Action::ChannelOrb(OrbId::Plasma)
    ));

    let virus_actions = symbiotic_virus::at_battle_start();
    assert_eq!(virus_actions.len(), 1);
    assert!(matches!(
        virus_actions[0].action,
        Action::ChannelOrb(OrbId::Dark)
    ));
}

#[test]
fn runic_capacitor_increases_orb_slots_on_first_turn_after_pre_battle_only() {
    let mut relic = RelicState::new(RelicId::RunicCapacitor);
    runic_capacitor::at_pre_battle(&mut relic);
    assert_eq!(relic.amount, 1);

    let first = runic_capacitor::at_turn_start(&mut relic);
    assert_eq!(relic.amount, 0);
    assert_eq!(first.len(), 1);
    assert!(matches!(first[0].action, Action::IncreaseMaxOrb(3)));

    assert!(runic_capacitor::at_turn_start(&mut relic).is_empty());
}

#[test]
fn inserter_counter_starts_at_zero_and_adds_orb_slot_every_second_turn() {
    let mut relic = RelicState::new(RelicId::Inserter);
    assert_eq!(relic.counter, 0);

    let first = inserter::Inserter::at_turn_start(&mut relic);
    assert!(first.is_empty());
    assert_eq!(relic.counter, 1);

    let second = inserter::Inserter::at_turn_start(&mut relic);
    assert_eq!(relic.counter, 0);
    assert_eq!(second.len(), 1);
    assert!(matches!(second[0].action, Action::IncreaseMaxOrb(1)));
}

#[test]
fn shared_rare_damage_retention_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Calipers), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::Torii), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::TungstenRod), RelicTier::Rare);

    let calipers = get_relic_subscriptions(RelicId::Calipers);
    assert!(calipers.on_calculate_block_retained);
    assert!(!calipers.at_battle_start);

    let torii = get_relic_subscriptions(RelicId::Torii);
    assert!(torii.on_attacked_to_change_damage);
    assert!(!torii.on_lose_hp_last);

    let tungsten = get_relic_subscriptions(RelicId::TungstenRod);
    assert!(
        !tungsten.on_lose_hp,
        "Java Tungsten Rod only overrides onLoseHpLast"
    );
    assert!(tungsten.on_lose_hp_last);
}

#[test]
fn calipers_retains_only_block_above_fifteen_without_barricade_logic() {
    assert_eq!(calipers::on_calculate_block_retained(0), 0);
    assert_eq!(calipers::on_calculate_block_retained(14), 0);
    assert_eq!(calipers::on_calculate_block_retained(15), 0);
    assert_eq!(calipers::on_calculate_block_retained(16), 1);
    assert_eq!(calipers::on_calculate_block_retained(40), 25);

    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Calipers));
    assert_eq!(hooks::on_calculate_block_retained(&state, 40), 25);
}

#[test]
fn torii_requires_real_non_player_owner_and_normal_non_thorns_damage() {
    let owned_normal = DamageInfo {
        source: 901,
        target: 0,
        base: 4,
        output: 4,
        damage_type: DamageType::Normal,
        is_modified: true,
    };
    assert_eq!(torii::on_attacked_to_change_damage(&owned_normal, 4), 1);
    assert_eq!(torii::on_attacked_to_change_damage(&owned_normal, 1), 1);
    assert_eq!(torii::on_attacked_to_change_damage(&owned_normal, 6), 6);

    let player_owned = DamageInfo {
        source: 0,
        ..owned_normal.clone()
    };
    assert_eq!(torii::on_attacked_to_change_damage(&player_owned, 4), 4);

    let no_owner = DamageInfo {
        source: NO_SOURCE,
        ..owned_normal.clone()
    };
    assert_eq!(
        torii::on_attacked_to_change_damage(&no_owner, 4),
        4,
        "Java requires info.owner != null; Rust NO_SOURCE must not trigger Torii"
    );

    let hp_loss = DamageInfo {
        damage_type: DamageType::HpLoss,
        ..owned_normal.clone()
    };
    assert_eq!(torii::on_attacked_to_change_damage(&hp_loss, 4), 4);

    let thorns = DamageInfo {
        damage_type: DamageType::Thorns,
        ..owned_normal
    };
    assert_eq!(torii::on_attacked_to_change_damage(&thorns, 4), 4);
}

#[test]
fn tungsten_rod_is_only_final_hp_loss_modifier() {
    assert_eq!(tungsten_rod::modify_hp_loss(0), 0);
    assert_eq!(tungsten_rod::modify_hp_loss(1), 0);
    assert_eq!(tungsten_rod::modify_hp_loss(5), 4);

    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::TungstenRod));
    assert_eq!(hooks::on_lose_hp_last(&state, 5), 4);
    assert!(
        hooks::on_lose_hp(&mut state, 5).is_empty(),
        "Java Tungsten Rod has no onLoseHp action hook"
    );
}

#[test]
fn shared_rare_passive_resource_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Ginger), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::Turnip), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::IceCream), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::LizardTail), RelicTier::Rare);

    assert!(get_relic_subscriptions(RelicId::Ginger).on_receive_power_modify);
    assert!(get_relic_subscriptions(RelicId::Turnip).on_receive_power_modify);
    assert!(get_relic_subscriptions(RelicId::IceCream).on_calculate_energy_retained);

    let lizard = get_relic_subscriptions(RelicId::LizardTail);
    assert!(!lizard.on_lose_hp);
    assert!(!lizard.on_lose_hp_last);
    assert!(!lizard.on_calculate_heal);
}

#[test]
fn ginger_and_turnip_block_apply_power_before_artifact_without_blocking_cleanup() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Ginger));
    store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Artifact,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    crate::engine::action_handlers::powers::handle_apply_power(
        901,
        0,
        PowerId::Weak,
        2,
        &mut state,
    );
    let player_powers = store::powers_snapshot_for(&state, 0);
    assert!(
        !player_powers.iter().any(|p| p.power_type == PowerId::Weak),
        "Ginger should block Weakened ApplyPowerAction"
    );
    assert_eq!(
        player_powers
            .iter()
            .find(|p| p.power_type == PowerId::Artifact)
            .map(|p| p.amount),
        Some(1),
        "Java checks Ginger before Artifact, so Artifact is not consumed"
    );

    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::Turnip));
    crate::engine::action_handlers::powers::handle_apply_power(
        901,
        0,
        PowerId::Frail,
        2,
        &mut state,
    );
    assert!(
        !store::powers_snapshot_for(&state, 0)
            .iter()
            .any(|p| p.power_type == PowerId::Frail),
        "Turnip should block Frail ApplyPowerAction"
    );

    let weak_cleanup = crate::content::powers::core::weak::at_end_of_round(0, 2, false);
    assert_eq!(weak_cleanup.len(), 1);
    assert!(matches!(
        weak_cleanup[0],
        Action::ReducePower {
            target: 0,
            power_id: PowerId::Weak,
            amount: 1
        }
    ));

    let frail_cleanup = crate::content::powers::core::frail::at_end_of_round(0, 2, false);
    assert_eq!(frail_cleanup.len(), 1);
    assert!(matches!(
        frail_cleanup[0],
        Action::ReducePower {
            target: 0,
            power_id: PowerId::Frail,
            amount: 1
        }
    ));

    store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Weak,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::powers::handle_reduce_power(0, PowerId::Weak, 1, &mut state);
    assert_eq!(
        store::powers_snapshot_for(&state, 0)
            .iter()
            .find(|p| p.power_type == PowerId::Weak)
            .map(|p| p.amount),
        Some(1),
        "Weak/Frail cleanup is ReducePowerAction in Java and must not be blocked by Ginger/Turnip"
    );
}

#[test]
fn ice_cream_recharge_preserves_unspent_energy_before_adding_base_energy() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 2;
    state.entities.player.energy_master = 3;
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::IceCream));

    assert!(hooks::on_calculate_energy_retained(&state));
    state.begin_next_player_turn();
    assert_eq!(
        state.turn.energy, 5,
        "Java Ice Cream uses EnergyPanel.addEnergy(base), preserving prior total"
    );
    assert_eq!(state.turn.counters.cards_played_this_turn, 0);
}

#[test]
fn lizard_tail_uses_java_counter_gate_and_fairy_priority() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.current_hp = 0;
    state.entities.player.max_hp = 80;
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::LizardTail));

    crate::engine::action_handlers::try_revive(&mut state);
    assert_eq!(state.entities.player.current_hp, 40);
    assert_eq!(state.entities.player.relics[0].counter, -2);
    assert!(state.entities.player.relics[0].used_up);

    let mut counter_used = crate::test_support::blank_test_combat();
    counter_used.entities.player.current_hp = 0;
    let mut used_lizard = RelicState::new(RelicId::LizardTail);
    used_lizard.counter = -2;
    used_lizard.used_up = false;
    counter_used.entities.player.add_relic(used_lizard);
    crate::engine::action_handlers::try_revive(&mut counter_used);
    assert_eq!(
        counter_used.entities.player.current_hp, 0,
        "Java checks LizardTail.counter == -1, not just used_up"
    );

    let mut fairy_first = crate::test_support::blank_test_combat();
    fairy_first.entities.player.current_hp = 0;
    fairy_first.entities.player.max_hp = 80;
    fairy_first
        .entities
        .player
        .add_relic(RelicState::new(RelicId::LizardTail));
    fairy_first.entities.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::FairyPotion,
        7001,
    ))];
    crate::engine::action_handlers::try_revive(&mut fairy_first);
    assert_eq!(fairy_first.entities.player.current_hp, 24);
    assert!(fairy_first.entities.potions[0].is_none());
    assert_eq!(
        fairy_first.entities.player.relics[0].counter, -1,
        "Java consumes Fairy before Lizard Tail"
    );
    assert!(!fairy_first.entities.player.relics[0].used_up);

    let mut bloom_blocks = crate::test_support::blank_test_combat();
    bloom_blocks.entities.player.current_hp = 0;
    bloom_blocks
        .entities
        .player
        .add_relic(RelicState::new(RelicId::MarkOfTheBloom));
    bloom_blocks
        .entities
        .player
        .add_relic(RelicState::new(RelicId::LizardTail));
    bloom_blocks.entities.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::FairyPotion,
        7002,
    ))];
    crate::engine::action_handlers::try_revive(&mut bloom_blocks);
    assert_eq!(bloom_blocks.entities.player.current_hp, 0);
    assert!(
        bloom_blocks.entities.potions[0].is_some(),
        "Mark of the Bloom blocks Fairy and Lizard revive before either is consumed"
    );
    assert_eq!(bloom_blocks.entities.player.relics[1].counter, -1);
}

#[test]
fn fairy_passive_revive_does_not_trigger_toy_ornithopter_on_use_potion() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.current_hp = 0;
    state.entities.player.max_hp = 80;
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::ToyOrnithopter));
    state.entities.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::FairyPotion,
        7003,
    ))];

    crate::engine::action_handlers::try_revive(&mut state);

    assert_eq!(state.entities.player.current_hp, 24);
    assert!(state.entities.potions[0].is_none());
    assert_eq!(
        state.action_queue_len(),
        0,
        "Java Fairy Potion passive revive is handled by AbstractPlayer.damage, not PotionPopUp, so relic onUsePotion hooks do not fire"
    );
}

#[test]
fn revival_inventory_matches_each_actual_trigger_with_healing_modifiers() {
    use crate::content::potions::{Potion, PotionId};
    use crate::engine::action_handlers::{available_revival_requests, try_revive, RevivalSource};
    for max_hp in [1, 7, 68, 80] {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.max_hp = max_hp;
        state.entities.player.current_hp = max_hp;
        for id in [
            RelicId::MagicFlower,
            RelicId::SacredBark,
            RelicId::LizardTail,
        ] {
            state.entities.player.add_relic(RelicState::new(id));
        }
        state.entities.potions = vec![
            Some(Potion::new(PotionId::FairyPotion, 7004)),
            None,
            Some(Potion::new(PotionId::FairyPotion, 7005)),
        ];
        let source = state.clone();
        let requests = available_revival_requests(&state).collect::<Vec<_>>();
        assert_eq!(state, source, "inventory inspection is pure");
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].source, RevivalSource::FairyPotion { slot: 0 });
        assert_eq!(requests[1].source, RevivalSource::FairyPotion { slot: 2 });
        assert_eq!(
            requests[2].source,
            RevivalSource::LizardTail { relic_index: 2 }
        );
        for request in requests {
            state.entities.player.current_hp = 0;
            try_revive(&mut state);
            assert_eq!(state.entities.player.current_hp, request.healing_from_zero);
        }
        assert!(available_revival_requests(&state).next().is_none());
        let mut blocked = source;
        blocked
            .entities
            .player
            .add_relic(RelicState::new(RelicId::MarkOfTheBloom));
        assert!(available_revival_requests(&blocked).next().is_none());
    }
}

fn assert_revive_healing_updates_red_skull(fairy: bool) {
    // Check both an already active Red Skull and a lethal hit that crosses
    // below half HP before revival immediately crosses back above it.
    for initial_hp in [20, 60] {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.current_hp = initial_hp;
        state.entities.player.max_hp = 80;
        state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
        for relic in [
            RelicId::RedSkull,
            RelicId::LizardTail,
            if fairy {
                RelicId::SacredBark
            } else {
                RelicId::MagicFlower
            },
        ] {
            state.entities.player.add_relic(RelicState::new(relic));
        }
        if fairy {
            state.entities.potions = vec![Some(crate::content::potions::Potion::new(
                crate::content::potions::PotionId::FairyPotion,
                7100,
            ))];
        }
        let start = hooks::at_battle_start(&mut state);
        state.queue_actions(start);
        drain_test_actions(&mut state);
        assert_eq!(
            store::power_amount(&state, 0, PowerId::Strength),
            if initial_hp <= 40 { 3 } else { 0 }
        );

        crate::engine::action_handlers::damage::handle_lose_hp(0, 99, false, &mut state);
        assert_eq!(
            state.entities.player.current_hp,
            if fairy { 48 } else { 60 }
        );
        drain_test_actions(&mut state);
        assert_eq!(
            store::power_amount(&state, 0, PowerId::Strength),
            0,
            "Java revival uses normal heal callbacks at HP 0; initial HP {initial_hp}"
        );
        let skull = state
            .entities
            .player
            .relics
            .iter()
            .find(|r| r.id == RelicId::RedSkull)
            .unwrap();
        assert!(
            !skull.used_up,
            "Red Skull is inactive above half HP after revival"
        );
        let tail = state
            .entities
            .player
            .relics
            .iter()
            .find(|r| r.id == RelicId::LizardTail)
            .unwrap();
        assert_eq!(tail.counter, if fairy { -1 } else { -2 });
        assert_eq!(tail.used_up, !fairy);
        if fairy {
            assert!(state.entities.potions[0].is_none());
        }
    }
}

#[test]
fn lizard_tail_revive_healing_runs_red_skull_threshold_callbacks() {
    assert_revive_healing_updates_red_skull(false);
}

#[test]
fn fairy_revive_healing_runs_red_skull_threshold_callbacks() {
    assert_revive_healing_updates_red_skull(true);
}

#[test]
fn shared_rare_run_campfire_relic_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::Mango), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::OldCoin), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::Girya), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::PeacePipe), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::PrayerWheel), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::Shovel), RelicTier::Rare);
    assert_eq!(get_relic_tier(RelicId::WingBoots), RelicTier::Rare);

    assert_eq!(RelicState::new(RelicId::Girya).counter, 0);
    assert_eq!(RelicState::new(RelicId::WingBoots).counter, 3);

    assert!(get_relic_subscriptions(RelicId::Girya).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::Mango).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::OldCoin).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::PeacePipe).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::PrayerWheel).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::Shovel).at_battle_start);
    assert!(!get_relic_subscriptions(RelicId::WingBoots).at_battle_start);
}

#[test]
fn mango_and_old_coin_on_equip_match_java_resource_changes() {
    let mut mango_run = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    mango_run.max_hp = 80;
    mango_run.current_hp = 40;
    assert!(mango::on_equip(&mut mango_run).is_none());
    assert_eq!(mango_run.max_hp, 94);
    assert_eq!(mango_run.current_hp, 54);

    let mut old_coin_run = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    old_coin_run.gold = 25;
    assert!(old_coin::on_equip(&mut old_coin_run).is_none());
    assert_eq!(old_coin_run.gold, 325);

    let mut ectoplasm_run = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    ectoplasm_run.gold = 25;
    ectoplasm_run
        .relics
        .push(RelicState::new(RelicId::Ectoplasm));
    assert!(old_coin::on_equip(&mut ectoplasm_run).is_none());
    assert_eq!(
        ectoplasm_run.gold, 25,
        "Java Old Coin uses player.gainGold, so Ectoplasm blocks it"
    );
}

#[test]
fn rare_run_relic_can_spawn_gates_match_java_sources() {
    fn spawn_result(
        tier: RelicTier,
        blocked_candidate: RelicId,
        fallback: RelicId,
        floor_num: i32,
        owned_relics: Vec<RelicState>,
        room_type: crate::state::map::node::RoomType,
    ) -> RelicId {
        let mut run = crate::state::run::RunState::new(1, 0, false, "Ironclad");
        run.floor_num = floor_num;
        run.relics = owned_relics;

        let mut node = crate::state::map::node::MapRoomNode::new(0, 0);
        node.class = Some(room_type);
        run.map = crate::state::map::state::MapState::new(vec![vec![node]]);
        run.map.current_x = 0;
        run.map.current_y = 0;

        match tier {
            RelicTier::Common => run.common_relic_pool = vec![blocked_candidate, fallback],
            RelicTier::Uncommon => run.uncommon_relic_pool = vec![blocked_candidate, fallback],
            RelicTier::Rare => run.rare_relic_pool = vec![blocked_candidate, fallback],
            RelicTier::Shop => run.shop_relic_pool = vec![blocked_candidate, fallback],
            RelicTier::Boss => run.boss_relic_pool = vec![blocked_candidate, fallback],
            _ => panic!("unsupported tier in canSpawn test: {tier:?}"),
        }
        run.random_relic_by_tier(tier)
    }

    let monster_room = crate::state::map::node::RoomType::MonsterRoom;
    let shop_room = crate::state::map::node::RoomType::ShopRoom;

    let floor_48_gates = [
        (
            RelicId::AncientTeaSet,
            RelicTier::Common,
            RelicId::BloodVial,
        ),
        (RelicId::CeramicFish, RelicTier::Common, RelicId::BloodVial),
        (
            RelicId::DarkstonePeriapt,
            RelicTier::Uncommon,
            RelicId::OrnamentalFan,
        ),
        (RelicId::DreamCatcher, RelicTier::Common, RelicId::BloodVial),
        (
            RelicId::FrozenEgg,
            RelicTier::Uncommon,
            RelicId::OrnamentalFan,
        ),
        (RelicId::JuzuBracelet, RelicTier::Common, RelicId::BloodVial),
        (RelicId::MealTicket, RelicTier::Common, RelicId::BloodVial),
        (
            RelicId::MeatOnTheBone,
            RelicTier::Uncommon,
            RelicId::OrnamentalFan,
        ),
        (
            RelicId::MoltenEgg,
            RelicTier::Uncommon,
            RelicId::OrnamentalFan,
        ),
        (RelicId::Omamori, RelicTier::Common, RelicId::BloodVial),
        (RelicId::PotionBelt, RelicTier::Common, RelicId::BloodVial),
        (RelicId::PrayerWheel, RelicTier::Rare, RelicId::Mango),
        (
            RelicId::QuestionCard,
            RelicTier::Uncommon,
            RelicId::OrnamentalFan,
        ),
        (RelicId::RegalPillow, RelicTier::Common, RelicId::BloodVial),
        (
            RelicId::SingingBowl,
            RelicTier::Uncommon,
            RelicId::OrnamentalFan,
        ),
        (
            RelicId::ToxicEgg,
            RelicTier::Uncommon,
            RelicId::OrnamentalFan,
        ),
    ];
    for (candidate, tier, fallback) in floor_48_gates {
        assert_eq!(
            spawn_result(tier, candidate, fallback, 48, vec![], monster_room),
            candidate,
            "{candidate:?} should spawn on floor 48 like Java canSpawn"
        );
        assert_eq!(
            spawn_result(tier, candidate, fallback, 49, vec![], monster_room),
            fallback,
            "{candidate:?} should reject after floor 48 like Java canSpawn"
        );
    }

    let shop_room_gates = [
        (RelicId::Courier, RelicTier::Shop, RelicId::ChemicalX),
        (RelicId::MawBank, RelicTier::Common, RelicId::BloodVial),
        (RelicId::OldCoin, RelicTier::Rare, RelicId::Mango),
        (RelicId::SmilingMask, RelicTier::Common, RelicId::BloodVial),
    ];
    for (candidate, tier, fallback) in shop_room_gates {
        assert_eq!(
            spawn_result(tier, candidate, fallback, 48, vec![], monster_room),
            candidate,
            "{candidate:?} should spawn outside ShopRoom before floor cutoff"
        );
        assert_eq!(
            spawn_result(tier, candidate, fallback, 48, vec![], shop_room),
            fallback,
            "{candidate:?} should reject in ShopRoom like Java canSpawn"
        );
        assert_eq!(
            spawn_result(tier, candidate, fallback, 49, vec![], monster_room),
            fallback,
            "{candidate:?} should reject after floor 48 like Java canSpawn"
        );
    }

    assert_eq!(
        spawn_result(
            RelicTier::Common,
            RelicId::PreservedInsect,
            RelicId::BloodVial,
            52,
            vec![],
            monster_room,
        ),
        RelicId::PreservedInsect
    );
    assert_eq!(
        spawn_result(
            RelicTier::Common,
            RelicId::PreservedInsect,
            RelicId::BloodVial,
            53,
            vec![],
            monster_room,
        ),
        RelicId::BloodVial
    );
    assert_eq!(
        spawn_result(
            RelicTier::Common,
            RelicId::TinyChest,
            RelicId::BloodVial,
            35,
            vec![],
            monster_room,
        ),
        RelicId::TinyChest
    );
    assert_eq!(
        spawn_result(
            RelicTier::Common,
            RelicId::TinyChest,
            RelicId::BloodVial,
            36,
            vec![],
            monster_room,
        ),
        RelicId::BloodVial
    );
    assert_eq!(
        spawn_result(
            RelicTier::Rare,
            RelicId::WingBoots,
            RelicId::Mango,
            40,
            vec![],
            monster_room,
        ),
        RelicId::WingBoots
    );
    assert_eq!(
        spawn_result(
            RelicTier::Rare,
            RelicId::WingBoots,
            RelicId::Mango,
            41,
            vec![],
            monster_room,
        ),
        RelicId::Mango
    );

    assert_eq!(
        spawn_result(
            RelicTier::Rare,
            RelicId::Girya,
            RelicId::Mango,
            48,
            vec![],
            monster_room,
        ),
        RelicId::Mango,
        "Java Girya/PeacePipe/Shovel reject floorNum >= 48"
    );
    assert_eq!(
        spawn_result(
            RelicTier::Rare,
            RelicId::Girya,
            RelicId::Mango,
            47,
            vec![
                RelicState::new(RelicId::PeacePipe),
                RelicState::new(RelicId::Shovel)
            ],
            monster_room,
        ),
        RelicId::Mango,
        "Java campfire relics reject when two of Girya/Peace Pipe/Shovel are already owned"
    );
    assert_eq!(
        spawn_result(
            RelicTier::Rare,
            RelicId::Girya,
            RelicId::Mango,
            47,
            vec![RelicState::new(RelicId::PeacePipe)],
            monster_room,
        ),
        RelicId::Girya
    );
    assert_eq!(
        spawn_result(
            RelicTier::Rare,
            RelicId::PeacePipe,
            RelicId::Mango,
            47,
            vec![
                RelicState::new(RelicId::Girya),
                RelicState::new(RelicId::Shovel)
            ],
            monster_room,
        ),
        RelicId::Mango
    );
    assert_eq!(
        spawn_result(
            RelicTier::Rare,
            RelicId::Shovel,
            RelicId::Mango,
            47,
            vec![
                RelicState::new(RelicId::Girya),
                RelicState::new(RelicId::PeacePipe)
            ],
            monster_room,
        ),
        RelicId::Mango
    );
}

#[test]
fn girya_lift_counter_and_battle_start_strength_match_java() {
    assert_eq!(girya::Girya::battle_start_strength(-1), 0);
    assert_eq!(girya::Girya::battle_start_strength(0), 0);
    assert_eq!(girya::Girya::battle_start_strength(1), 1);
    assert_eq!(girya::Girya::battle_start_strength(3), 3);

    let mut engine_state = crate::state::core::EngineState::Campfire;
    let mut run = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    run.relics.clear();
    run.relics.push(RelicState::new(RelicId::Girya));

    assert!(crate::engine::campfire_handler::handle(
        &mut engine_state,
        &mut run,
        Some(crate::state::core::ClientInput::CampfireOption(
            crate::state::core::CampfireChoice::Lift,
        )),
    ));
    assert_eq!(run.relics[0].counter, 1);

    let actions = girya::Girya::at_battle_start(run.relics[0].counter);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].insertion_mode, AddTo::Top);
    assert!(matches!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 1
        }
    ));

    run.relics[0].counter = 3;
    let options = crate::engine::campfire_handler::get_available_options(&run);
    assert!(
        !options.contains(&crate::state::core::CampfireChoice::Lift),
        "Java LiftOption is disabled once Girya counter reaches 3"
    );
}

#[test]
fn prayer_wheel_adds_second_non_boss_card_reward() {
    let mut normal = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    normal.relics.clear();
    normal.relics.push(RelicState::new(RelicId::PrayerWheel));
    let normal_rewards =
        crate::rewards::generator::generate_combat_rewards(&mut normal, false, false);
    assert_eq!(
        normal_rewards
            .items
            .iter()
            .filter(|item| matches!(item, crate::rewards::state::RewardItem::Card { .. }))
            .count(),
        2
    );

    let mut elite = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    elite.relics.clear();
    elite.relics.push(RelicState::new(RelicId::PrayerWheel));
    let elite_rewards = crate::rewards::generator::generate_combat_rewards(&mut elite, true, false);
    assert_eq!(
        elite_rewards
            .items
            .iter()
            .filter(|item| matches!(item, crate::rewards::state::RewardItem::Card { .. }))
            .count(),
        1,
        "Java CombatRewardScreen.setupItemReward excludes MonsterRoomElite from Prayer Wheel's extra card reward"
    );

    let mut boss = crate::state::run::RunState::new(1, 0, false, "Ironclad");
    boss.relics.clear();
    boss.relics.push(RelicState::new(RelicId::PrayerWheel));
    let boss_rewards = crate::rewards::generator::generate_combat_rewards(&mut boss, false, true);
    assert_eq!(
        boss_rewards
            .items
            .iter()
            .filter(|item| matches!(item, crate::rewards::state::RewardItem::Card { .. }))
            .count(),
        1,
        "Java Prayer Wheel does not add an extra boss card reward"
    );
}

#[test]
fn shared_event_special_relic_followup_metadata_matches_java_sources() {
    assert_eq!(get_relic_tier(RelicId::CultistMask), RelicTier::Special);
    assert_eq!(get_relic_tier(RelicId::GoldenIdol), RelicTier::Special);
    assert_eq!(get_relic_tier(RelicId::NeowsLament), RelicTier::Special);
    assert_eq!(get_relic_tier(RelicId::NlothsMask), RelicTier::Special);
    assert_eq!(get_relic_tier(RelicId::OddMushroom), RelicTier::Special);
    assert_eq!(get_relic_tier(RelicId::RedMask), RelicTier::Special);

    assert!(get_relic_subscriptions(RelicId::CultistMask).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::NeowsLament).at_battle_start);
    assert!(get_relic_subscriptions(RelicId::OddMushroom).on_calculate_vulnerable_multiplier);
    assert!(get_relic_subscriptions(RelicId::RedMask).at_battle_start);
    assert_eq!(RelicState::new(RelicId::NeowsLament).counter, 3);
    assert_eq!(RelicState::new(RelicId::NlothsMask).counter, 1);
}

#[test]
fn cultist_mask_battle_start_is_ui_only_in_headless_simulator() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::CultistMask));

    let actions = hooks::at_battle_start(&mut state);
    assert!(
        actions.is_empty(),
        "Java CultistMask.atBattleStart only flashes, plays SFX, and queues TalkAction"
    );
}

#[test]
fn red_mask_applies_one_weak_to_each_live_enemy_at_battle_start() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::RedMask));
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 11;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 12;
    state.entities.monsters = vec![first, second];

    let actions = hooks::at_battle_start(&mut state);
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().any(|action| matches!(
        action.action,
        Action::ApplyPower {
            target: 11,
            power_id: PowerId::Weak,
            amount: 1,
            ..
        }
    )));
    assert!(actions.iter().any(|action| matches!(
        action.action,
        Action::ApplyPower {
            target: 12,
            power_id: PowerId::Weak,
            amount: 1,
            ..
        }
    )));
}

#[test]
fn odd_mushroom_changes_only_player_vulnerable_multiplier() {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::OddMushroom));

    assert_eq!(
        hooks::on_calculate_vulnerable_multiplier(&state, true),
        1.25
    );
    assert_eq!(
        hooks::on_calculate_vulnerable_multiplier(&state, false),
        1.5
    );
}

#[test]
fn golden_idol_reward_gold_bonus_is_applied_once_when_claimed() {
    let mut run = crate::state::run::RunState::new(2, 0, false, "Ironclad");
    run.gold = 0;
    run.relics.clear();
    run.relics.push(RelicState::new(RelicId::GoldenIdol));

    let mut rewards = crate::rewards::generator::generate_combat_rewards(&mut run, false, false);
    let base_gold = rewards
        .items
        .iter()
        .find_map(|item| match item {
            crate::rewards::state::RewardItem::Gold { amount } => Some(*amount),
            _ => None,
        })
        .expect("combat rewards should include base gold");
    assert!(
        (10..=20).contains(&base_gold),
        "generated RewardItem stores Java goldAmt; Golden Idol bonus is not pre-applied"
    );
    let gold_index = rewards
        .items
        .iter()
        .position(|item| matches!(item, crate::rewards::state::RewardItem::Gold { .. }))
        .unwrap();

    crate::rewards::handler::handle(
        &mut run,
        &mut rewards,
        Some(crate::state::core::ClientInput::ClaimReward(gold_index)),
    );

    assert_eq!(
        run.gold,
        crate::content::relics::golden_idol::apply_reward_gold_bonus(base_gold)
    );
}

#[test]
fn golden_idol_does_not_apply_to_stolen_gold() {
    let mut run = crate::state::run::RunState::new(3, 0, false, "Ironclad");
    run.gold = 0;
    run.relics.clear();
    run.relics.push(RelicState::new(RelicId::GoldenIdol));
    let mut rewards = crate::rewards::state::RewardState::new();
    rewards
        .items
        .push(crate::rewards::state::RewardItem::StolenGold { amount: 40 });

    crate::rewards::handler::handle(
        &mut run,
        &mut rewards,
        Some(crate::state::core::ClientInput::ClaimReward(0)),
    );

    assert_eq!(run.gold, 40);
}

#[test]
fn neows_lament_sets_live_enemy_hp_to_one_and_expires_on_third_combat() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 21;
    first.current_hp = 20;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 22;
    second.current_hp = 7;
    state.entities.monsters = vec![first, second];
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::NeowsLament));
    state.entities.player.relics[0].counter = 1;

    let actions = hooks::at_battle_start(&mut state);
    assert!(actions.is_empty());
    assert_eq!(state.entities.monsters[0].current_hp, 1);
    assert_eq!(state.entities.monsters[1].current_hp, 1);
    assert_eq!(state.entities.player.relics[0].counter, -2);
    assert!(
        state.entities.player.relics[0].used_up,
        "Java NeowsLament.setCounter(-2) calls usedUp synchronously"
    );
}

#[test]
fn neows_lament_decrements_counter_synchronously_before_expiring() {
    let mut state = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.current_hp = 18;
    state.entities.monsters = vec![monster];
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::NeowsLament));
    state.entities.player.relics[0].counter = 3;

    let actions = hooks::at_battle_start(&mut state);
    assert!(actions.is_empty());
    assert_eq!(state.entities.monsters[0].current_hp, 1);
    assert_eq!(state.entities.player.relics[0].counter, 2);
    assert!(!state.entities.player.relics[0].used_up);
}

#[test]
fn necronomicon_on_equip_adds_necronomicurse_to_master_deck() {
    let mut run = crate::state::run::RunState::new(4, 0, false, "Ironclad");
    run.master_deck.clear();

    assert!(run
        .obtain_relic_with_source(
            RelicId::Necronomicon,
            crate::state::core::EngineState::MapNavigation,
            DomainEventSource::RewardScreen,
        )
        .is_none());

    assert!(run
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Necronomicon));
    assert_eq!(
        run.master_deck
            .iter()
            .filter(|card| card.id == CardId::Necronomicurse)
            .count(),
        1,
        "Java Necronomicon.onEquip obtains one Necronomicurse"
    );
    assert!(run.emitted_events.iter().any(|event| matches!(
        event,
        DomainEvent::CardObtained {
            card,
            source: DomainEventSource::Relic(RelicId::Necronomicon),
        } if card.id == CardId::Necronomicurse
    )));
}

#[test]
fn necronomicon_on_equip_runs_obtain_hooks_before_necronomicurse_is_added() {
    let mut run = crate::state::run::RunState::new(4, 0, false, "Ironclad");
    run.relics.push(RelicState::new(RelicId::CeramicFish));
    run.master_deck.clear();

    assert!(run
        .obtain_relic_with_source(
            RelicId::Necronomicon,
            crate::state::core::EngineState::MapNavigation,
            DomainEventSource::RewardScreen,
        )
        .is_none());

    let events = run.take_emitted_events();
    let fish_gold_pos = events
        .iter()
        .position(|event| {
            matches!(
                event,
                DomainEvent::GoldChanged {
                    delta: 9,
                    source: DomainEventSource::Relic(RelicId::Necronomicon),
                    ..
                }
            )
        })
        .expect("Necronomicon curse should run Ceramic Fish onObtainCard hook");
    let curse_pos = events
        .iter()
        .position(|event| {
            matches!(
                event,
                DomainEvent::CardObtained {
                    card,
                    source: DomainEventSource::Relic(RelicId::Necronomicon),
                } if card.id == CardId::Necronomicurse
            )
        })
        .expect("Necronomicon should obtain Necronomicurse through ShowCardAndObtainEffect");

    assert!(
        fish_gold_pos < curse_pos,
        "Java Necronomicon.onEquip queues ShowCardAndObtainEffect; that effect runs relic onObtainCard before Soul.obtain"
    );
}

#[test]
fn necronomicon_on_equip_necronomicurse_can_be_blocked_by_omamori() {
    let mut run = crate::state::run::RunState::new(4, 0, false, "Ironclad");
    run.relics.push(RelicState::new(RelicId::Omamori));
    run.master_deck.clear();

    assert!(run
        .obtain_relic_with_source(
            RelicId::Necronomicon,
            crate::state::core::EngineState::MapNavigation,
            DomainEventSource::RewardScreen,
        )
        .is_none());

    assert!(run
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Necronomicon));
    assert!(!run
        .master_deck
        .iter()
        .any(|card| card.id == CardId::Necronomicurse));
    let omamori = run
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::Omamori)
        .expect("Omamori should be present");
    assert_eq!(omamori.counter, 1);
    assert!(!omamori.used_up);
    assert!(!run.emitted_events.iter().any(|event| matches!(
        event,
        DomainEvent::CardObtained {
            card,
            source: DomainEventSource::Relic(RelicId::Necronomicon),
        } if card.id == CardId::Necronomicurse
    )));
}

#[test]
fn necronomicon_replay_uses_same_instance_copy_like_java() {
    let mut card = CombatCard::for_scenario(CardId::Bludgeon, 6201);
    card.base_damage_mut = 99;
    card.energy_on_use = 2;
    let mut relic = RelicState::new(RelicId::Necronomicon);
    relic.used_up = false;

    let actions = necronomicon::on_use_card(CardId::Bludgeon, 3, &mut relic, &card, Some(70));
    assert!(
        relic.used_up,
        "Java Necronomicon.activated is cleared synchronously in onUseCard"
    );
    assert_eq!(actions.len(), 1);
    match &actions[0].action {
        Action::EnqueueCardPlay { item, in_front } => {
            assert!(*in_front);
            assert_eq!(
                item.card.lineage,
                crate::runtime::combat::CardLineageId::constructed_scenario(6201)
            );
            assert_eq!(
                item.card.base_damage_mut, 0,
                "Java Necronomicon uses makeSameInstanceOf(), not a raw transient clone"
            );
            assert_eq!(item.energy_on_use, 2);
            assert_eq!(item.target, Some(70));
            assert_eq!(
                item.source,
                crate::runtime::combat::QueuedCardSource::Necronomicon
            );
        }
        other => panic!("Necronomicon should enqueue a same-instance card play, got {other:?}"),
    }

    assert!(necronomicon::on_use_card(CardId::Bludgeon, 3, &mut relic, &card, Some(70)).is_empty());
    necronomicon::at_turn_start(&mut relic);
    assert!(
        !relic.used_up,
        "Java Necronomicon.atTurnStart reactivates the relic"
    );
}

#[test]
fn necronomicon_on_unequip_removes_one_necronomicurse_without_regenerating_it() {
    let mut run = crate::state::run::RunState::new(5, 0, false, "Ironclad");
    run.relics.clear();
    run.relics.push(RelicState::new(RelicId::Necronomicon));
    let mut du_vu = RelicState::new(RelicId::DuVuDoll);
    du_vu.counter = 1;
    run.relics.push(du_vu);
    run.master_deck.clear();
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Necronomicurse, 6001));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Strike, 6002));

    assert_eq!(
        run.remove_first_relic_with_id_and_source(
            RelicId::Necronomicon,
            DomainEventSource::Event(EventId::CursedTome),
        ),
        Some(RelicId::Necronomicon)
    );

    assert!(!run
        .master_deck
        .iter()
        .any(|card| card.id == CardId::Necronomicurse));
    assert!(run.master_deck.iter().any(|card| card.id == CardId::Strike));
    assert_eq!(
        run.relics
            .iter()
            .find(|relic| relic.id == RelicId::DuVuDoll)
            .map(|relic| relic.counter),
        Some(1),
        "Java Necronomicon.onUnequip uses masterDeck.group.remove, so it does not call onMasterDeckChange"
    );
    assert_eq!(
        run.emitted_events
            .iter()
            .filter(|event| matches!(
                event,
                DomainEvent::CardObtained {
                    card,
                    ..
                } if card.id == CardId::Necronomicurse
            ))
            .count(),
        0,
        "Java Necronomicon.onUnequip directly removes a Necronomicurse; it must not trigger the card's self-regeneration"
    );
}
