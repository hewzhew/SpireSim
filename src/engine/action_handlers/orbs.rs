use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

pub(super) fn handle_increase_max_orb(amount: u8, state: &mut CombatState) {
    if amount == 0 {
        return;
    }
    state.entities.player.max_orbs = state.entities.player.max_orbs.saturating_add(amount);
    while state.entities.player.orbs.len() < state.entities.player.max_orbs as usize {
        state
            .entities
            .player
            .orbs
            .push(crate::runtime::combat::OrbEntity::new(
                crate::runtime::combat::OrbId::Empty,
            ));
    }
}

pub(super) fn handle_decrease_max_orb(amount: u8, state: &mut CombatState) {
    for _ in 0..amount {
        if state.entities.player.max_orbs == 0 {
            return;
        }
        state.entities.player.max_orbs = state.entities.player.max_orbs.saturating_sub(1);
        if !state.entities.player.orbs.is_empty() {
            state.entities.player.orbs.pop();
        }
    }
}

pub(super) fn handle_channel_orb(orb_id: crate::runtime::combat::OrbId, state: &mut CombatState) {
    if state.entities.player.max_orbs == 0 {
        return;
    }
    while state.entities.player.orbs.len() < state.entities.player.max_orbs as usize {
        state
            .entities
            .player
            .orbs
            .push(crate::runtime::combat::OrbEntity::new(
                crate::runtime::combat::OrbId::Empty,
            ));
    }
    let mut new_orb = crate::runtime::combat::OrbEntity::new(orb_id);
    if !matches!(
        new_orb.id,
        crate::runtime::combat::OrbId::Empty | crate::runtime::combat::OrbId::Plasma
    ) {
        let focus = crate::content::powers::store::power_amount(
            state,
            0,
            crate::content::powers::PowerId::Focus,
        );
        new_orb.passive_amount = (new_orb.base_passive_amount + focus).max(0);
        if new_orb.id != crate::runtime::combat::OrbId::Dark {
            new_orb.evoke_amount = (new_orb.base_evoke_amount + focus).max(0);
        }
    }
    if let Some(empty_slot) = state
        .entities
        .player
        .orbs
        .iter()
        .position(|orb| orb.id == crate::runtime::combat::OrbId::Empty)
    {
        state.entities.player.orbs[empty_slot] = new_orb;
        state.turn.record_orb_channeled(orb_id);
    } else {
        state.queue_action_front(Action::ChannelOrb(orb_id));
        state.queue_action_front(Action::EvokeOrb);
    }
}

pub(super) fn handle_channel_random_orbs(amount: u8, state: &mut CombatState) {
    use crate::runtime::combat::OrbId;

    let mut orbs = Vec::with_capacity(amount as usize);
    for _ in 0..amount {
        let roll = state.rng.card_random_rng.random(3);
        let orb = match roll {
            0 => OrbId::Dark,
            1 => OrbId::Frost,
            2 => OrbId::Lightning,
            _ => OrbId::Plasma,
        };
        orbs.push(orb);
    }

    for orb in orbs.into_iter().rev() {
        state.queue_action_front(Action::ChannelOrb(orb));
    }
}

pub(super) fn handle_channel_orb_entity(
    orb: crate::runtime::combat::OrbEntity,
    state: &mut CombatState,
) {
    let orb_id = orb.id;
    if state.entities.player.max_orbs == 0 {
        return;
    }
    while state.entities.player.orbs.len() < state.entities.player.max_orbs as usize {
        state
            .entities
            .player
            .orbs
            .push(crate::runtime::combat::OrbEntity::new(
                crate::runtime::combat::OrbId::Empty,
            ));
    }
    if let Some(empty_slot) = state
        .entities
        .player
        .orbs
        .iter()
        .position(|existing| existing.id == crate::runtime::combat::OrbId::Empty)
    {
        state.entities.player.orbs[empty_slot] = orb;
        state.turn.record_orb_channeled(orb_id);
    }
}

pub(super) fn handle_fission(upgraded: bool, state: &mut CombatState) {
    let orb_count = crate::content::orbs::hooks::filled_orb_count(state) as i32;
    state.queue_action_front(Action::DrawCards(orb_count.max(0) as u32));
    state.queue_action_front(Action::GainEnergy { amount: orb_count });
    if upgraded {
        state.queue_action_front(Action::EvokeAllOrbs);
    } else {
        state.queue_action_front(Action::RemoveAllOrbs);
    }
}

pub(super) fn handle_redo_orb(state: &mut CombatState) {
    let Some(orb) = state.entities.player.orbs.first().cloned() else {
        return;
    };
    if orb.id == crate::runtime::combat::OrbId::Empty {
        return;
    }
    state.queue_action_front(Action::ChannelOrbEntity { orb });
    state.queue_action_front(Action::EvokeOrb);
}
