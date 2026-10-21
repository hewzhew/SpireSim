//! Shared card mechanics, not desirability or guaranteed play outcomes.
use super::{CardObservation, EntityPosition, WIDTH};
use sts_core::content::cards::{
    get_card_definition,
    mechanics::{
        card_enemy_strength_restoration_v1, card_mechanic_profile,
        unconditional_card_draw_request_v1, EnemyStrengthRestorationV1, CARD_MECHANIC_ROLE_COUNT,
    },
    CardId, CardType,
};

pub(super) fn card_type(values: &mut [f32; WIDTH], card: CardId) {
    let index = match get_card_definition(card).card_type {
        CardType::Attack => 27,
        CardType::Skill => 28,
        CardType::Power => 29,
        CardType::Status => 30,
        CardType::Curse => 31,
    };
    values[index] = 1.;
}

pub(super) fn encode(p: &mut EntityPosition, card: &CardObservation, owner: i64) {
    mechanics(
        p,
        card.card,
        card.upgrades,
        Some(card.magic),
        card.exhaust_override,
        owner,
    );
}

/// An offered identity has no live instance number or override yet. Its roles
/// and restoration schedule are known; their numeric requests remain absent.
pub(super) fn encode_description(p: &mut EntityPosition, id: CardId, upgrades: u8, owner: i64) {
    mechanics(p, id, upgrades, None, None, owner);
}

fn mechanics(
    p: &mut EntityPosition,
    id: CardId,
    upgrades: u8,
    magic: Option<i32>,
    exhaust_override: Option<bool>,
    owner: i64,
) {
    // The bounded role vocabulary has 21 entries. A changed vocabulary needs
    // explicit layout/schema review rather than overwriting later columns.
    assert_eq!(CARD_MECHANIC_ROLE_COUNT, 21);
    let profile = card_mechanic_profile(id, upgrades);
    let mut values = [0.; WIDTH];
    for role in profile.roles.iter() {
        values[role as usize] = 1.;
    }
    values[21 + profile.coverage as usize] = 1.;
    values[24] = f32::from(exhaust_override.unwrap_or(profile.exhausts_when_played));
    values[25] = f32::from(profile.ethereal);
    super::optional(
        &mut values,
        26,
        magic.and_then(|m| unconditional_card_draw_request_v1(id, m).map(|n| n as i32)),
        5.,
    );
    if let Some(restoration) = card_enemy_strength_restoration_v1(id) {
        values[29] = f32::from(restoration == EnemyStrengthRestorationV1::NoScheduledRestoration);
        values[30] = f32::from(restoration == EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd);
        if let Some(magnitude) = magic {
            values[28] = 1.;
            values[31] = -magnitude as f32 / 10.;
        }
    }
    p.push(23, 0, values, [owner, -1, -1]);
}
