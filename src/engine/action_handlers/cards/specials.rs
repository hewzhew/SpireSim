use crate::content::cards::CardId;
use crate::runtime::action::{Action, DamageInfo};
use crate::runtime::combat::{CardLineageId, CombatState};

pub fn handle_barrage(damage: DamageInfo, state: &mut CombatState) {
    let count = state
        .entities
        .player
        .orbs
        .iter()
        .filter(|orb| orb.id != crate::runtime::combat::OrbId::Empty)
        .count();

    for _ in 0..count {
        state.queue_action_front(Action::Damage(damage.clone()));
    }
}

pub fn handle_escape_plan_block_if_skill(block: i32, state: &mut CombatState) {
    if state.runtime.last_drawn_cards.iter().any(|record| {
        crate::content::cards::get_card_definition(record.card_id).card_type
            == crate::content::cards::CardType::Skill
    }) {
        state.queue_action_front(Action::GainBlock {
            target: 0,
            amount: block,
        });
    }
}

pub fn handle_blade_fury(upgraded: bool, state: &mut CombatState) {
    let count = state.zones.hand.len() as u8;
    state.queue_action_front(
        crate::content::cards::make_constructed_temp_card_in_hand_action(
            CardId::Shiv,
            count,
            upgraded,
            state,
        ),
    );
    state.queue_action_front(Action::DiscardFromHand {
        amount: count as i32,
        random: false,
        end_turn: false,
    });
}

pub fn handle_unload_non_attack(state: &mut CombatState) {
    let non_attacks: Vec<CardLineageId> = state
        .zones
        .hand
        .iter()
        .filter(|card| {
            crate::content::cards::get_card_definition(card.id).card_type
                != crate::content::cards::CardType::Attack
        })
        .map(|card| card.lineage)
        .collect();

    for lineage in non_attacks {
        state.queue_action_front(Action::DiscardCard {
            card_lineage: lineage,
        });
    }
}

pub fn handle_expertise_draw(target_hand_size: i32, state: &mut CombatState) {
    let to_draw = target_hand_size - state.zones.hand.len() as i32;
    if to_draw > 0 {
        state.queue_action_front(Action::DrawCards(to_draw as u32));
    }
}

pub fn handle_halt(block: i32, additional: i32, state: &mut CombatState) {
    let amount = if state.entities.player.stance == crate::runtime::combat::StanceId::Wrath {
        block + additional
    } else {
        block
    };
    state.queue_action_front(Action::GainBlock { target: 0, amount });
}
