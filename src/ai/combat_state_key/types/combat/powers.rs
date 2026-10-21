use crate::content::powers::PowerId;

use super::CombatCardKey;

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatEntityPowersKey {
    pub(crate) entity_id: usize,
    pub(crate) powers: Vec<CombatPowerKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatPowerKey {
    pub(crate) power_type: PowerId,
    pub(crate) instance_id: Option<u32>,
    pub(crate) amount: i32,
    pub(crate) extra_data: i32,
    pub(crate) payload: CombatPowerPayloadKey,
    pub(crate) just_applied: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatPowerPayloadKey {
    None,
    Card(CombatCardKey),
    SourceEntity(crate::EntityId),
}
