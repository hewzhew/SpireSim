use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Canonical deterministic run state shared by both lockstep adapters.
///
/// Endpoint adapters still own extraction from their real object graphs, but
/// the field names and types below have exactly one compilation owner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunStateV2 {
    pub seed: String,
    pub ascension: u64,
    pub character: String,
    pub act: u64,
    pub floor: i64,
    pub current_hp: i64,
    pub max_hp: i64,
    pub gold: i64,
    pub keys: NaturalRunKeysV2,
    pub final_act_available: bool,
    pub daily: bool,
    pub shop_purge_base_cost: i64,
    pub drop_state: NaturalRunDropStateV2,
    pub room_flags: NaturalRunRoomFlagsV2,
    pub relic_pools: NaturalRunRelicPoolsV2,
    pub encounter_schedule: NaturalRunEncounterScheduleV2,
    pub event_schedule: NaturalRunEventScheduleV2,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunKeysV2 {
    pub ruby: bool,
    pub sapphire: bool,
    pub emerald: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunDropStateV2 {
    pub potion_chance_modifier: i64,
    pub card_rarity_offset: i64,
    pub card_upgrade_chance_bits: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunRoomFlagsV2 {
    pub mugged: bool,
    pub smoked: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunRelicPoolsV2 {
    pub common: Vec<String>,
    pub uncommon: Vec<String>,
    pub rare: Vec<String>,
    pub shop: Vec<String>,
    pub boss: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunEncounterScheduleV2 {
    pub normal: Vec<String>,
    pub elite: Vec<String>,
    pub boss_key: Option<String>,
    pub boss_queue: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunEventScheduleV2 {
    pub events: Vec<String>,
    pub shrines: Vec<String>,
    pub one_time_events: Vec<String>,
    pub chance_bits: NaturalRunEventChanceBitsV2,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRunEventChanceBitsV2 {
    pub elite: u32,
    pub monster: u32,
    pub shop: u32,
    pub treasure: u32,
    pub shrine: u32,
}

/// Original-game choices that cannot return to this Act's event generator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NaturalActExitV1 {
    BossRelicChoice,
    SpireHeart,
}

impl NaturalActExitV1 {
    /// Boss relic choices leave Act 1/2; SpireHeart leaves Act 3 or ends the run.
    /// SpireHeart.buttonEffect never draws another event. A continuing run calls
    /// AbstractDungeon.dungeonTransitionSetup, which clears shrineList before
    /// the next dungeon rebuilds it. Daily/endless exits are outside this policy.
    pub fn retires_shrines(self, act: u64, daily: bool) -> bool {
        !daily
            && match self {
                Self::BossRelicChoice => matches!(act, 1 | 2),
                Self::SpireHeart => act == 3,
            }
    }
}

impl NaturalRunStateV2 {
    /// Retire only a finished Act's shrine pool; other run fields still compare.
    pub fn retire_finished_act_shrines(&mut self, exit: Option<NaturalActExitV1>) {
        if exit.is_some_and(|exit| exit.retires_shrines(self.act, self.daily)) {
            self.event_schedule.shrines.clear();
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.seed.is_empty() || self.character.is_empty() || self.act == 0 {
            return Err("canonical run state has an invalid identity".to_owned());
        }
        if self.max_hp <= 0 || self.current_hp > self.max_hp || self.gold < 0 {
            return Err("canonical run state has invalid player resources".to_owned());
        }
        if self.shop_purge_base_cost < 75 || (self.shop_purge_base_cost - 75) % 25 != 0 {
            return Err("canonical run state has an invalid shop purge cost".to_owned());
        }
        for (label, values) in [
            ("common relic pool", self.relic_pools.common.as_slice()),
            ("uncommon relic pool", self.relic_pools.uncommon.as_slice()),
            ("rare relic pool", self.relic_pools.rare.as_slice()),
            ("shop relic pool", self.relic_pools.shop.as_slice()),
            ("boss relic pool", self.relic_pools.boss.as_slice()),
            ("event pool", self.event_schedule.events.as_slice()),
            ("shrine pool", self.event_schedule.shrines.as_slice()),
            (
                "one-time event pool",
                self.event_schedule.one_time_events.as_slice(),
            ),
        ] {
            require_unique_nonempty(label, values)?;
        }
        for (label, values) in [
            (
                "normal encounter schedule",
                self.encounter_schedule.normal.as_slice(),
            ),
            (
                "elite encounter schedule",
                self.encounter_schedule.elite.as_slice(),
            ),
            (
                "boss encounter schedule",
                self.encounter_schedule.boss_queue.as_slice(),
            ),
        ] {
            if values.iter().any(String::is_empty) {
                return Err(format!("canonical {label} contains an empty identity"));
            }
        }
        if self
            .encounter_schedule
            .boss_key
            .as_ref()
            .is_some_and(String::is_empty)
        {
            return Err("canonical boss encounter key is empty".to_owned());
        }
        Ok(())
    }
}

fn require_unique_nonempty(label: &str, values: &[String]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.is_empty() {
            return Err(format!("canonical {label} contains an empty identity"));
        }
        if !seen.insert(value) {
            return Err(format!("canonical {label} repeats identity {value:?}"));
        }
    }
    Ok(())
}
