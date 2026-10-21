use super::*;
use crate::content::cards::{evaluate_card_for_play, resolve_card_play, CardType};
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState, OrbEntity, OrbId};

fn state_with_targets() -> CombatState {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.monsters = [7, 8]
        .into_iter()
        .map(|id| {
            let mut monster =
                crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
            monster.id = id;
            monster.current_hp = 1000;
            monster
        })
        .collect();
    state
}

#[test]
fn reviewed_fixed_damage_matches_production_requests_and_target_scope() {
    let state = state_with_targets();
    let mut cases = 0;
    for id in CardId::STABLE_IDS
        .iter()
        .map(|id| CardId::from_stable_id(id).unwrap())
    {
        if get_card_definition(id).card_type != CardType::Attack {
            continue;
        }
        for upgrades in [0, 1] {
            let rule =
                card_base_damage_delivery_v1(id, upgrades).expect("review every current attack");
            let CardBaseDamageDeliveryV1::Attack {
                amount: CardAttackAmountV1::Base,
                hits: CardAttackHitsV1::Fixed { count },
                targets,
                ..
            } = rule
            else {
                continue;
            };
            let mut card = CombatCard::for_scenario(id, 1);
            card.upgrades = upgrades;
            let card = evaluate_card_for_play(&card, &state, Some(7));
            let expected =
                super::super::card_base_damage_for_upgrades_v1(&get_card_definition(id), upgrades);
            let mut observed = Vec::new();
            for action in resolve_card_play(id, &state, &card, Some(7)) {
                match action.action {
                    Action::Damage(info)
                    | Action::PummelDamage(info)
                    | Action::WallopDamage(info)
                    | Action::HeelHook(info) => {
                        observed.push((CardAttackTargetsV1::SelectedEnemy, info.output))
                    }
                    Action::Feed { damage_info, .. }
                    | Action::HandOfGreed { damage_info, .. }
                    | Action::RitualDagger { damage_info, .. }
                    | Action::LessonLearned { damage_info, .. }
                    | Action::Ftl { damage_info, .. }
                    | Action::Sunder { damage_info, .. }
                    | Action::DropkickDamageAndEffect { damage_info, .. }
                    | Action::FearNoEvil { damage_info, .. } => {
                        observed.push((CardAttackTargetsV1::SelectedEnemy, damage_info.output))
                    }
                    Action::DamageAllEnemies { damages, .. }
                    | Action::VampireDamageAllEnemies { damages, .. } => {
                        assert_eq!(
                            damages.as_slice(),
                            [expected, expected],
                            "{id:?}+{upgrades}"
                        );
                        observed.push((CardAttackTargetsV1::EachEnemy, expected));
                    }
                    Action::AttackDamageRandomEnemyCard { card } => {
                        let evaluated = evaluate_card_for_play(&card, &state, Some(7));
                        observed.push((
                            CardAttackTargetsV1::RandomEnemyPerHit,
                            evaluated.base_damage_mut,
                        ));
                    }
                    _ => {}
                }
            }
            assert_eq!(
                observed,
                vec![(targets, expected); count as usize],
                "{id:?}+{upgrades}"
            );
            cases += 1;
        }
    }
    assert!(cases > 150);
    println!("verified {cases} fixed damage request cases with two targets");
}

#[test]
fn variable_repetitions_use_resolution_state_not_one_assumed_hit() {
    for (id, rule) in [
        (
            CardId::FiendFire,
            CardAttackHitsV1::PerHandCardAtResolutionBeforeExhaust,
        ),
        (
            CardId::Flechettes,
            CardAttackHitsV1::PerSkillInHandAtResolution,
        ),
        (CardId::Barrage, CardAttackHitsV1::PerFilledOrbAtResolution),
        (
            CardId::Finisher,
            CardAttackHitsV1::PerPriorAttackThisTurnAtResolution,
        ),
    ] {
        for upgrades in [0, 1] {
            let delivery = card_base_damage_delivery_v1(id, upgrades).unwrap();
            assert!(
                matches!(delivery, CardBaseDamageDeliveryV1::Attack { hits, .. } if hits == rule)
            );
            assert_eq!(delivery.unconditional_base_hits(), None);
            for count in [0, 3] {
                let mut state = state_with_targets();
                let mut card = CombatCard::for_scenario(id, 1);
                card.upgrades = upgrades;
                let pending = resolve_card_play(id, &state, &card, Some(7));
                // Mutate only after use: these four rules read at action execution.
                state.zones.hand = (0..count)
                    .map(|n| CombatCard::for_scenario(CardId::Defend, 10 + n))
                    .collect();
                if id == CardId::Flechettes {
                    state
                        .zones
                        .hand
                        .push(CombatCard::for_scenario(CardId::Strike, 20));
                }
                state.entities.player.orbs =
                    (0..count).map(|_| OrbEntity::new(OrbId::Frost)).collect();
                state
                    .entities
                    .player
                    .orbs
                    .push(OrbEntity::new(OrbId::Empty));
                state.turn.counters.attacks_played_this_turn = count as u8 + 1;
                for info in pending {
                    crate::engine::action_handlers::execute_action(info.action, &mut state);
                }
                let mut hits = 0;
                let expected = super::super::card_base_damage_for_upgrades_v1(
                    &get_card_definition(id),
                    upgrades,
                );
                while let Some(action) = state.pop_next_action() {
                    if let Action::Damage(info) = action {
                        assert_eq!(info.output, expected);
                        hits += 1;
                    }
                }
                assert_eq!(hits, count, "{id:?}+{upgrades}");
            }
        }
    }
}

#[test]
fn wish_uses_choice_action_and_dynamic_amounts_replace_the_definition() {
    for upgrades in [0, 1] {
        let mut state = state_with_targets();
        let mut wish = CombatCard::for_scenario(CardId::Wish, 1);
        wish.upgrades = upgrades;
        let actions = resolve_card_play(wish.id, &state, &wish, None);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions[0].action,
            Action::SuspendForChooseOne { .. }
        ));
        assert_eq!(
            card_base_damage_delivery_v1(wish.id, upgrades),
            Some(CardBaseDamageDeliveryV1::StrengthIfChosen)
        );
        for count in [0, 7] {
            state.entities.player.block = count;
            state.zones.draw_pile = (0..count)
                .map(|n| CombatCard::for_scenario(CardId::Defend, 10 + n as u32))
                .collect();
            for id in [CardId::BodySlam, CardId::MindBlast] {
                let mut card = CombatCard::for_scenario(id, 2);
                card.upgrades = upgrades;
                let actions = resolve_card_play(id, &state, &card, Some(7));
                assert!(
                    matches!(&actions[0].action, Action::Damage(info) if info.output == count),
                    "{id:?}"
                );
                assert_eq!(
                    card_base_damage_delivery_v1(id, upgrades)
                        .unwrap()
                        .unconditional_base_hits(),
                    None
                );
            }
        }
    }
}

#[test]
fn variable_x_history_and_instance_counts_match_production() {
    for id in [
        CardId::Whirlwind,
        CardId::Skewer,
        CardId::Expunger,
        CardId::ThunderStrike,
    ] {
        for upgrades in [0, 1] {
            for count in [0, 3] {
                let mut state = state_with_targets();
                state.turn.energy = count;
                for _ in 0..count {
                    state.turn.record_orb_channeled(OrbId::Lightning);
                }
                state.turn.record_orb_channeled(OrbId::Frost);
                let mut card = CombatCard::for_scenario(id, 1);
                card.upgrades = upgrades;
                card.energy_on_use = count as i32;
                card.misc_value = count as i32;
                let requests = resolve_card_play(id, &state, &card, Some(7));
                let mut hits = 0;
                for info in requests {
                    match info.action {
                        Action::Whirlwind { .. } | Action::Skewer { .. } => {
                            crate::engine::action_handlers::execute_action(info.action, &mut state);
                        }
                        Action::Damage(_) | Action::AttackDamageRandomEnemyCard { .. } => hits += 1,
                        other => panic!("unexpected request: {other:?}"),
                    }
                }
                while let Some(action) = state.pop_next_action() {
                    if matches!(action, Action::Damage(_) | Action::DamageAllEnemies { .. }) {
                        hits += 1;
                    }
                }
                assert_eq!(hits, count, "{id:?}+{upgrades}");
                assert_eq!(
                    card_base_damage_delivery_v1(id, upgrades)
                        .unwrap()
                        .unconditional_base_hits(),
                    None
                );
            }
        }
    }
    let state = state_with_targets();
    let card = CombatCard::for_scenario(CardId::BowlingBash, 1);
    let attacks = resolve_card_play(card.id, &state, &card, Some(7));
    assert_eq!(attacks.len(), 2);
    assert!(attacks
        .iter()
        .all(|info| matches!(&info.action, Action::Damage(damage) if damage.target == 7)));
}
