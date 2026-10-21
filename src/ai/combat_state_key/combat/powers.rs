use crate::runtime::combat::{CombatState, Power, PowerPayload};

use super::super::types::{CombatEntityPowersKey, CombatPowerKey, CombatPowerPayloadKey};
use super::cards::card_key;

pub(super) fn powers_key(combat: &CombatState) -> Vec<CombatEntityPowersKey> {
    let mut entries = combat
        .entities
        .power_db
        .iter()
        .map(|(entity, powers)| {
            let powers = powers.iter().map(power_key).collect::<Vec<_>>();
            CombatEntityPowersKey {
                entity_id: *entity,
                powers,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.entity_id);
    entries
}

fn power_key(power: &Power) -> CombatPowerKey {
    CombatPowerKey {
        power_type: power.power_type,
        instance_id: power.instance_id,
        amount: power.amount,
        extra_data: power.extra_data,
        payload: match &power.payload {
            PowerPayload::None => CombatPowerPayloadKey::None,
            PowerPayload::Card(card) => CombatPowerPayloadKey::Card(card_key(card)),
            PowerPayload::SourceEntity(source) => CombatPowerPayloadKey::SourceEntity(*source),
        },
        just_applied: power.just_applied,
    }
}
