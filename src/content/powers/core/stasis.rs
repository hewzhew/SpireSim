use crate::runtime::action::Action;
use crate::EntityId;

pub struct StasisPower;

pub fn held_card_lineage(card_lineage: i32) -> Option<crate::runtime::combat::CardLineageId> {
    crate::runtime::combat::CardLineageId::from_runtime_i32_payload(card_lineage)
}

/// Builds the mechanic-owned Stasis power without exposing the compact
/// lineage encoding stored in the legacy `extra_data` slot.
pub fn power_holding_card(
    lineage: crate::runtime::combat::CardLineageId,
) -> crate::runtime::combat::Power {
    crate::runtime::combat::Power {
        power_type: crate::content::powers::PowerId::Stasis,
        instance_id: None,
        amount: -1,
        extra_data: lineage.to_runtime_i32_payload(),
        payload: crate::runtime::combat::PowerPayload::None,
        just_applied: false,
    }
}

pub fn on_death(
    state: &crate::runtime::combat::CombatState,
    _owner: EntityId,
    card_lineage: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::SmallVec::new();
    let Some(card_lineage) = held_card_lineage(card_lineage) else {
        return actions;
    };
    if !state
        .zones
        .limbo
        .iter()
        .any(|card| card.lineage == card_lineage)
    {
        return actions;
    }

    actions.push(Action::ReturnStasisCard {
        card_lineage,
        to_hand: state.zones.hand.len() != 10,
    });
    actions
}
