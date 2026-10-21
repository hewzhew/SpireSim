use super::*;
use crate::content::cards::resolve_card_play;
use crate::content::monsters::EnemyId;
use crate::content::powers::{store, PowerId};
use crate::engine::action_handlers::{cards, damage, execute_action};
use crate::runtime::action::{Action, AddTo};
use crate::runtime::combat::{CombatCard, CombatState, Power};
use crate::state::{GridSelectFilter, GridSelectReason, PileType};

fn state() -> CombatState {
    let mut state = crate::test_support::blank_test_combat();
    state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    state
}

fn power(id: PowerId, amount: i32) -> Power {
    Power {
        power_type: id,
        amount,
        instance_id: None,
        extra_data: 0,
        payload: Default::default(),
        just_applied: false,
    }
}

#[test]
fn random_attack_hand_contract_matches_infernal_blade_requests() {
    for upgrades in [0, 1] {
        let rule = card_random_attack_to_hand_v1(CardId::InfernalBlade).unwrap();
        let mut card = CombatCard::for_scenario(CardId::InfernalBlade, 1);
        card.upgrades = upgrades;
        let actions = resolve_card_play(card.id, &state(), &card, None);
        assert_eq!(usize::from(rule.count), actions.len());
        assert_eq!(
            actions[0].action,
            Action::MakeRandomCardInHand {
                card_type: Some(crate::content::cards::CardType::Attack),
                cost_for_turn: Some(rule.cost_for_turn),
            }
        );
    }
    assert!(card_random_attack_to_hand_v1(CardId::Anger).is_none());
}

#[test]
fn shockwave_order_decides_which_debuff_artifact_prevents() {
    for upgrades in [0, 1] {
        for artifact in [0, 1, 2] {
            let mut state = state();
            let mut second = crate::test_support::test_monster(EnemyId::Cultist);
            second.id = 2;
            state.entities.monsters.push(second);
            let first = state.entities.monsters[0].id;
            if artifact > 0 {
                store::set_powers_for(&mut state, first, vec![power(PowerId::Artifact, artifact)]);
            }
            let mut card = CombatCard::for_scenario(CardId::Shockwave, 1);
            card.upgrades = upgrades;
            let amount = if upgrades == 0 { 3 } else { 5 };
            let actions = resolve_card_play(card.id, &state, &card, None);
            assert_eq!(actions.len(), 4);
            for (row, (target, power_id)) in actions.into_iter().zip([
                (first, PowerId::Weak),
                (first, PowerId::Vulnerable),
                (2, PowerId::Weak),
                (2, PowerId::Vulnerable),
            ]) {
                assert_eq!(row.insertion_mode, AddTo::Bottom);
                assert_eq!(
                    row.action,
                    Action::ApplyPower {
                        source: 0,
                        target,
                        power_id,
                        amount
                    }
                );
                execute_action(row.action, &mut state);
            }
            assert_eq!(
                store::power_amount(&state, first, PowerId::Weak),
                if artifact == 0 { amount } else { 0 }
            );
            assert_eq!(
                store::power_amount(&state, first, PowerId::Vulnerable),
                if artifact < 2 { amount } else { 0 }
            );
            assert_eq!(store::power_amount(&state, 2, PowerId::Weak), amount);
            assert_eq!(store::power_amount(&state, 2, PowerId::Vulnerable), amount);
        }
    }
}

#[test]
fn rupture_needs_positive_eligible_loss_and_grants_stacks_per_event() {
    for (loss, eligible, stacks, expected) in [
        (0, true, 2, 0),
        (5, false, 2, 0),
        (1, true, 1, 1),
        (5, true, 2, 2),
    ] {
        let mut state = state();
        store::set_powers_for(&mut state, 0, vec![power(PowerId::Rupture, stacks)]);
        damage::handle_lose_hp(0, loss, eligible, &mut state);
        let mut requests = vec![];
        while let Some(action) = state.pop_next_action() {
            if let Action::ApplyPower {
                power_id: PowerId::Strength,
                amount,
                ..
            } = action
            {
                requests.push(amount);
            }
        }
        assert_eq!(
            requests,
            if expected == 0 {
                vec![]
            } else {
                vec![expected]
            }
        );
    }
    // Combust stacks combine into one HP-loss event, not one event per copy.
    let mut state = state();
    let mut combust = power(PowerId::Combust, 10);
    combust.extra_data = 2;
    store::set_powers_for(&mut state, 0, vec![power(PowerId::Rupture, 2), combust]);
    let mut losses = 0;
    for action in crate::content::powers::ironclad::combust::at_end_of_turn(&state, 0, 10) {
        if let Action::LoseHp {
            target,
            amount,
            triggers_rupture,
        } = action
        {
            losses += 1;
            damage::handle_lose_hp(target, amount, triggers_rupture, &mut state);
        }
    }
    assert_eq!(losses, 1);
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 2,
        })
    );
}

#[test]
fn preventing_eligible_hp_loss_also_prevents_rupture_strength() {
    use crate::content::relics::{RelicId, RelicState};
    let mut state = state();
    state
        .entities
        .player
        .add_relic(RelicState::new(RelicId::TungstenRod));
    store::set_powers_for(&mut state, 0, vec![power(PowerId::Rupture, 2)]);
    let before = state.entities.player.current_hp;
    damage::handle_lose_hp(0, 1, true, &mut state);
    assert_eq!(state.entities.player.current_hp, before);
    assert!(state.pop_next_action().is_none());
}

#[test]
fn secret_technique_moves_existing_skill_without_draw_or_cost_reset() {
    for hand_size in [0, 10] {
        let mut state = state();
        store::set_powers_for(&mut state, 0, vec![power(PowerId::NoDraw, 1)]);
        state.zones.hand = (10..10 + hand_size)
            .map(|i| CombatCard::for_scenario(CardId::Strike, i))
            .collect();
        let mut skill = CombatCard::for_scenario(CardId::ShrugItOff, 2);
        skill.upgrades = 1;
        skill.cost_for_turn = Some(0);
        let expected = skill.clone();
        state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Bash, 3), skill].into();
        let card = CombatCard::for_scenario(CardId::SecretTechnique, 1);
        let actions = resolve_card_play(card.id, &state, &card, None);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions[0].action,
            Action::SuspendForGridSelect {
                source_pile: PileType::Draw,
                reason: GridSelectReason::SkillFromDeckToHand,
                ..
            }
        ));
        state.queue_actions(actions);
        let mut engine = crate::state::EngineState::CombatProcessing;
        for _ in 0..10 {
            if engine != crate::state::EngineState::CombatProcessing {
                break;
            }
            crate::engine::core::tick_engine(&mut engine, &mut state, None);
        }
        assert_eq!(engine, crate::state::EngineState::CombatPlayerTurn);
        assert_eq!(state.zones.draw_pile.len(), 1);
        let moved = if hand_size == 10 {
            &state.zones.discard_pile[0]
        } else {
            &state.zones.hand[0]
        };
        assert_eq!(moved, &expected);
    }
    let mut state = state();
    let card = CombatCard::for_scenario(CardId::SecretTechnique, 1);
    state.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::Defend, 2),
        CombatCard::for_scenario(CardId::ShrugItOff, 3),
    ]
    .into();
    assert_eq!(
        resolve_card_play(card.id, &state, &card, None)[0].action,
        Action::SuspendForGridSelect {
            source_pile: PileType::Draw,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: GridSelectFilter::Skill,
            reason: GridSelectReason::SkillFromDeckToHand
        }
    );
}

#[test]
fn anger_copy_keeps_instance_changes_but_has_new_lineage_and_no_rendered_damage() {
    let mut state = state();
    let target = state.entities.monsters[0].id;
    let mut card = CombatCard::for_scenario(CardId::Anger, 1);
    card.upgrades = 1;
    card.base_damage_override = Some(11);
    card.cost_for_turn = Some(1);
    let actions = resolve_card_play(card.id, &state, &card, Some(target));
    assert!(matches!(actions[0].action, Action::Damage(_)));
    let Action::MakeCopyInDiscard { original, amount } = actions[1].action.clone() else {
        panic!("copy after damage")
    };
    cards::handle_make_copy_in_discard(*original, amount, &mut state);
    assert_eq!(state.zones.discard_pile.len(), 1);
    let copy = &state.zones.discard_pile[0];
    assert_ne!(copy.lineage, card.lineage);
    assert_eq!(copy.upgrades, 1);
    assert_eq!(copy.base_damage_override, Some(11));
    assert_eq!(copy.cost_for_turn, Some(1));
    assert_eq!(copy.base_damage_mut, 0);
}
