use crate::content::relics::RelicId;
use crate::runtime::combat::{OrbId, RelicBuses, StanceId};
use serde::ser::{SerializeSeq, SerializeStruct};

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatExactPlayerKey {
    pub(crate) current_hp: i32,
    pub(crate) block: i32,
    pub(crate) future_relevant: CombatPlayerFutureKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatDominancePlayerKey {
    pub(crate) future_relevant: CombatPlayerFutureKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CombatPlayerFutureKey {
    pub(crate) entity_id: usize,
    pub(crate) max_hp: i32,
    pub(crate) facing_left: bool,
    pub(crate) gold_delta_this_combat: i32,
    pub(crate) gold: i32,
    pub(crate) max_orbs: u8,
    pub(crate) orbs: Vec<CombatOrbKey>,
    pub(crate) stance: StanceId,
    pub(crate) relics: Vec<CombatRelicKey>,
    pub(crate) energy_master: u8,
}

impl serde::Serialize for CombatPlayerFutureKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Preserve the field order and sequence-of-buses representation used
        // by durable exact identity V2.  Only the in-memory key semantics are
        // narrowed; existing replay hashes remain byte-for-byte stable.
        let mut fields = serializer.serialize_struct("CombatPlayerFutureKey", 11)?;
        fields.serialize_field("entity_id", &self.entity_id)?;
        fields.serialize_field("max_hp", &self.max_hp)?;
        fields.serialize_field("facing_left", &self.facing_left)?;
        fields.serialize_field("gold_delta_this_combat", &self.gold_delta_this_combat)?;
        fields.serialize_field("gold", &self.gold)?;
        fields.serialize_field("max_orbs", &self.max_orbs)?;
        fields.serialize_field("orbs", &self.orbs)?;
        fields.serialize_field("stance", &self.stance)?;
        fields.serialize_field("relics", &self.relics)?;
        let relic_buses = RelicBuses::from_relic_ids(self.relics.iter().map(|relic| relic.id));
        fields.serialize_field("relic_buses", &RelicBusesV2(&relic_buses))?;
        fields.serialize_field("energy_master", &self.energy_master)?;
        fields.end()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatOrbKey {
    pub(crate) id: OrbId,
    pub(crate) base_passive_amount: i32,
    pub(crate) base_evoke_amount: i32,
    pub(crate) passive_amount: i32,
    pub(crate) evoke_amount: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatRelicKey {
    pub(crate) id: RelicId,
    pub(crate) counter: i32,
    pub(crate) used_up: bool,
    pub(crate) amount: i32,
}

const RELIC_BUS_COUNT: usize = 26;

struct RelicBusesV2<'a>(&'a RelicBuses);

impl serde::Serialize for RelicBusesV2<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut buses = serializer.serialize_seq(Some(RELIC_BUS_COUNT))?;
        for bus in relic_bus_slices(self.0) {
            buses.serialize_element(bus)?;
        }
        buses.end()
    }
}

fn relic_bus_slices(buses: &RelicBuses) -> [&[usize]; RELIC_BUS_COUNT] {
    [
        &buses.at_pre_battle,
        &buses.at_battle_start_pre_draw,
        &buses.at_battle_start,
        &buses.at_turn_start,
        &buses.at_turn_start_post_draw,
        &buses.on_use_card,
        &buses.on_shuffle,
        &buses.on_exhaust,
        &buses.on_lose_hp,
        &buses.on_victory,
        &buses.on_apply_power,
        &buses.on_monster_death,
        &buses.on_spawn_monster,
        &buses.at_end_of_turn,
        &buses.on_use_potion,
        &buses.on_discard,
        &buses.on_change_stance,
        &buses.on_attacked_to_change_damage,
        &buses.on_lose_hp_last,
        &buses.on_calculate_heal,
        &buses.on_calculate_x_cost,
        &buses.on_calculate_block_retained,
        &buses.on_calculate_energy_retained,
        &buses.on_scry,
        &buses.on_receive_power_modify,
        &buses.on_calculate_vulnerable_multiplier,
    ]
}

#[cfg(test)]
mod tests {
    use super::CombatPlayerFutureKey;

    #[test]
    fn future_key_does_not_own_the_derived_relic_dispatch_cache() {
        assert!(
            std::mem::size_of::<CombatPlayerFutureKey>() <= 15 * std::mem::size_of::<usize>(),
            "future key should contain authoritative relic state, not a derived bus cache"
        );
    }
}
