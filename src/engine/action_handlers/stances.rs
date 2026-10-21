use crate::runtime::action::{Action, DamageInfo};
use crate::runtime::combat::CombatState;

pub(super) fn handle_inner_peace(draw_amount: u32, state: &mut CombatState) {
    if state.entities.player.stance == crate::runtime::combat::StanceId::Calm {
        state.queue_action_front(Action::DrawCards(draw_amount));
    } else {
        state.queue_action_front(Action::EnterStance("Calm".to_string()));
    }
}

pub(super) fn handle_indignation(amount: i32, state: &mut CombatState) {
    if state.entities.player.stance == crate::runtime::combat::StanceId::Wrath {
        let targets: Vec<_> = state
            .entities
            .monsters
            .iter()
            .map(|monster| monster.id)
            .collect();
        for target in targets {
            state.queue_action_back(Action::ApplyPower {
                source: 0,
                target,
                power_id: crate::content::powers::PowerId::Vulnerable,
                amount,
            });
        }
    } else {
        state.queue_action_back(Action::EnterStance("Wrath".to_string()));
    }
}

pub(super) fn handle_follow_up(state: &mut CombatState) {
    if previous_played_card_type(state) == Some(crate::content::cards::CardType::Attack) {
        state.queue_action_front(Action::GainEnergy { amount: 1 });
    }
}

pub(super) fn handle_sanctity(draw_amount: u32, state: &mut CombatState) {
    if previous_played_card_type(state) == Some(crate::content::cards::CardType::Skill) {
        state.queue_action_front(Action::DrawCards(draw_amount));
    }
}

pub(super) fn handle_crush_joints(target: usize, amount: i32, state: &mut CombatState) {
    if previous_played_card_type(state) == Some(crate::content::cards::CardType::Skill) {
        state.queue_action_front(Action::ApplyPower {
            source: 0,
            target,
            power_id: crate::content::powers::PowerId::Vulnerable,
            amount,
        });
    }
}

pub(super) fn handle_sash_whip(target: usize, amount: i32, state: &mut CombatState) {
    if previous_played_card_type(state) == Some(crate::content::cards::CardType::Attack) {
        state.queue_action_front(Action::ApplyPower {
            source: 0,
            target,
            power_id: crate::content::powers::PowerId::Weak,
            amount,
        });
    }
}

pub(super) fn previous_played_card_type(
    state: &CombatState,
) -> Option<crate::content::cards::CardType> {
    let played = &state.turn.counters.card_ids_played_this_combat;
    if played.len() < 2 {
        return None;
    }
    let previous_card_id = played[played.len() - 2];
    Some(crate::content::cards::get_card_definition(previous_card_id).card_type)
}

pub(super) fn handle_fear_no_evil(target: usize, damage_info: DamageInfo, state: &mut CombatState) {
    if monster_has_java_attack_intent_for_fear_no_evil(state, target) {
        state.queue_action_front(Action::EnterStance("Calm".to_string()));
    }
    state.queue_action_front(Action::Damage(damage_info));
}

pub(super) fn monster_has_java_attack_intent_for_fear_no_evil(
    state: &CombatState,
    target: usize,
) -> bool {
    if state
        .monster_protocol_visible_intent(target)
        .is_java_attack_intent()
    {
        return true;
    }

    state
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target)
        .is_some_and(|monster| {
            monster
                .move_state
                .planned_visible_spec
                .as_ref()
                .is_some_and(|spec| spec.attack().is_some())
                || monster
                    .move_state
                    .planned_steps
                    .as_ref()
                    .is_some_and(|steps| {
                        steps.iter().any(|step| {
                            matches!(step, crate::runtime::monster_move::MoveStep::Attack(_))
                        })
                    })
        })
}

pub(super) fn handle_enter_stance(stance: &str, state: &mut CombatState) {
    if crate::content::powers::store::has_power(
        state,
        0,
        crate::content::powers::PowerId::CannotChangeStance,
    ) {
        return;
    }
    let new_stance = match stance {
        "Wrath" => crate::runtime::combat::StanceId::Wrath,
        "Calm" => crate::runtime::combat::StanceId::Calm,
        "Divinity" => crate::runtime::combat::StanceId::Divinity,
        _ => crate::runtime::combat::StanceId::Neutral,
    };
    let old_stance = state.entities.player.stance;
    if old_stance == new_stance {
        return;
    }
    for power in &crate::content::powers::store::powers_snapshot_for(state, 0) {
        for action in crate::content::powers::resolve_power_on_change_stance(
            power.power_type,
            0,
            power.amount,
            old_stance,
            new_stance,
        ) {
            state.queue_action_back(action);
        }
    }
    crate::content::relics::hooks::on_change_stance(state, old_stance, new_stance);
    if old_stance == crate::runtime::combat::StanceId::Calm {
        state.queue_action_back(Action::GainEnergy { amount: 2 });
    }
    state.entities.player.stance = new_stance;
    if new_stance == crate::runtime::combat::StanceId::Divinity {
        state.queue_action_back(Action::GainEnergy { amount: 3 });
    }
    let card_actions = crate::content::cards::hooks::on_change_stance_from_discard(state);
    state.queue_actions(card_actions);
}
