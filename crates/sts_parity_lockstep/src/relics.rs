use serde::{Deserialize, Serialize};

/// Persistent relic identity and the observed state needed to continue this combat.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalRelicV3 {
    pub ordinal: u64,
    pub id: String,
    pub counter: i64,
    pub used_up: bool,
    pub bound_card: Option<String>,
    pub combat_state: Option<NaturalRelicCombatStateV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalRelicCombatStateV1 {
    HoveringKite {
        triggered_this_turn: bool,
    },
    RedSkull {
        active: bool,
    },
    CentennialPuzzle {
        used_this_combat: bool,
    },
    OrangePellets {
        attack_played: bool,
        skill_played: bool,
        power_played: bool,
    },
    Lantern {
        first_turn: bool,
    },
    ArtOfWar {
        first_turn: bool,
        gain_energy_next: bool,
    },
}
