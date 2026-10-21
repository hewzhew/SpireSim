//! Shared partial power semantics. A description never creates an active power.
use super::{EntityPosition, WIDTH};
use sts_core::content::{
    cards::CardType,
    powers::{mechanics::typed_profile, mechanics::PowerEffectTarget, PowerId},
};

fn values(id: PowerId) -> [f32; WIDTH] {
    let mut v = [0.; WIDTH];
    if let Some(profile) = typed_profile(id) {
        v[0] = 1.; // partial profile available; zero does not mean no effect
        v[1 + profile.trigger as usize] = 1.;
        if let Some(card_type) = profile.card_type {
            v[5 + match card_type {
                CardType::Attack => 0,
                CardType::Skill => 1,
                CardType::Power => 2,
                CardType::Status => 3,
                CardType::Curse => 4,
            }] = 1.;
        }
        v[10 + profile.effect as usize] = 1.;
        v[17 + profile.target as usize] = 1.;
        v[19 + profile.amount_kind as usize] = 1.;
        v[22 + profile.lifetime as usize] = 1.;
        v[26] = f32::from(profile.positive_amount_only);
    }
    v
}

fn applied_description(p: &mut EntityPosition, id: PowerId) -> i64 {
    if let Some(row) = p
        .ids
        .iter()
        .position(|kind| *kind == [25, id.stable_ordinal() as i64])
    {
        return row as i64;
    }
    // Register before descending so even a future cyclic profile is bounded.
    let row = p.push(25, id.stable_ordinal() as i64, values(id), [-1; 3]);
    if let Some(applied) = typed_profile(id).and_then(|profile| profile.applied_power) {
        p.refs[row as usize][0] = applied_description(p, applied);
    }
    row
}

/// `subject` is either an active power or a planned application/removal. Its
/// kind and flags retain that distinction. `owner` is the power's recipient,
/// not necessarily the creature originating a planned application.
pub(super) fn encode(p: &mut EntityPosition, id: PowerId, subject: i64, owner: i64) {
    let profile = typed_profile(id);
    let target = match profile.map(|profile| profile.target) {
        Some(PowerEffectTarget::Player) => 0,
        _ => owner,
    };
    let applied = profile
        .and_then(|profile| profile.applied_power)
        .map(|id| applied_description(p, id))
        .unwrap_or(-1);
    p.push(24, 0, values(id), [subject, target, applied]);
}
