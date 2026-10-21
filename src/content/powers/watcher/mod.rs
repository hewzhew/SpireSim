use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, StanceId};
use smallvec::SmallVec;

pub fn battle_hymn_at_turn_start(state: &CombatState, amount: i32) -> SmallVec<[Action; 2]> {
    if state.are_monsters_basically_dead_java() {
        smallvec::smallvec![]
    } else {
        smallvec::smallvec![
            crate::content::cards::make_constructed_temp_card_in_hand_action(
                crate::content::cards::CardId::Smite,
                amount.max(0).min(u8::MAX as i32) as u8,
                false,
                state,
            )
        ]
    }
}

pub fn foresight_at_turn_start(state: &mut CombatState, amount: i32) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    if state.zones.draw_pile.is_empty() {
        state.queue_action_front(Action::EmptyDeckShuffle);
    }
    actions.push(Action::Scry(amount.max(0) as usize));
    actions
}

pub fn devotion_on_post_draw(
    owner: usize,
    amount: i32,
    state: &CombatState,
) -> SmallVec<[Action; 2]> {
    if owner != 0 {
        return smallvec::smallvec![];
    }
    if !crate::content::powers::store::has_power(
        state,
        owner,
        crate::content::powers::PowerId::Mantra,
    ) && amount >= 10
    {
        smallvec::smallvec![Action::EnterStance("Divinity".to_string())]
    } else {
        smallvec::smallvec![Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: crate::content::powers::PowerId::Mantra,
            amount,
        }]
    }
}

pub fn nirvana_on_scry(owner: usize, amount: i32) -> SmallVec<[Action; 2]> {
    smallvec::smallvec![Action::GainBlock {
        target: owner,
        amount,
    }]
}

pub fn like_water_at_end_of_turn(
    state: &CombatState,
    owner: usize,
    amount: i32,
) -> SmallVec<[Action; 2]> {
    if owner == 0 && state.entities.player.stance == StanceId::Calm {
        smallvec::smallvec![Action::GainBlock {
            target: owner,
            amount
        }]
    } else {
        smallvec::smallvec![]
    }
}

pub fn mental_fortress_on_change_stance(
    owner: usize,
    amount: i32,
    old_stance: StanceId,
    new_stance: StanceId,
) -> SmallVec<[Action; 2]> {
    if old_stance != new_stance {
        smallvec::smallvec![Action::GainBlock {
            target: owner,
            amount
        }]
    } else {
        smallvec::smallvec![]
    }
}

pub fn rushdown_on_change_stance(
    owner: usize,
    amount: i32,
    old_stance: StanceId,
    new_stance: StanceId,
) -> SmallVec<[Action; 2]> {
    if owner == 0 && old_stance != new_stance && new_stance == StanceId::Wrath {
        smallvec::smallvec![Action::DrawCards(amount.max(0) as u32)]
    } else {
        smallvec::smallvec![]
    }
}

pub fn wave_of_the_hand_on_block_gained(
    state: &CombatState,
    owner: usize,
    amount: i32,
    block_amount: i32,
) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    if owner != 0 || block_amount <= 0 || amount <= 0 {
        return actions;
    }
    for monster in &state.entities.monsters {
        actions.push(Action::ApplyPower {
            source: owner,
            target: monster.id,
            power_id: crate::content::powers::PowerId::Weak,
            amount,
        });
    }
    actions
}
