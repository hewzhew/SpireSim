use super::*;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct HexaghostRuntimeState {
    pub activated: bool,
    pub orb_active_count: u8,
    pub burn_upgraded: bool,
    pub divider_damage: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LouseRuntimeState {
    pub bite_damage: Option<i32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct JawWormRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
    pub hard_mode: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ThiefRuntimeState {
    pub protocol_seeded: bool,
    pub slash_count: u8,
    pub stolen_gold: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ByrdRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
    pub is_flying: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ChosenRuntimeState {
    pub protocol_seeded: bool,
    pub first_turn: bool,
    pub used_hex: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SneckoRuntimeState {
    pub protocol_seeded: bool,
    pub first_turn: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ShelledParasiteRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BronzeAutomatonRuntimeState {
    pub protocol_seeded: bool,
    pub first_turn: bool,
    pub num_turns: u8,
}

impl Default for BronzeAutomatonRuntimeState {
    fn default() -> Self {
        Self {
            protocol_seeded: false,
            first_turn: true,
            num_turns: 0,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BronzeOrbRuntimeState {
    pub protocol_seeded: bool,
    pub used_stasis: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct BookOfStabbingRuntimeState {
    pub protocol_seeded: bool,
    pub stab_count: u8,
}

impl Default for BookOfStabbingRuntimeState {
    fn default() -> Self {
        Self {
            protocol_seeded: false,
            stab_count: 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CollectorRuntimeState {
    pub protocol_seeded: bool,
    pub initial_spawn: bool,
    pub ult_used: bool,
    pub turns_taken: u8,
    pub enemy_slots: [Option<EntityId>; 2],
}

impl Default for CollectorRuntimeState {
    fn default() -> Self {
        Self {
            protocol_seeded: false,
            initial_spawn: true,
            ult_used: false,
            turns_taken: 0,
            enemy_slots: [None, None],
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ChampRuntimeState {
    pub protocol_seeded: bool,
    pub first_turn: bool,
    pub num_turns: u8,
    pub forge_times: u8,
    pub threshold_reached: bool,
}

impl Default for ChampRuntimeState {
    fn default() -> Self {
        Self {
            protocol_seeded: false,
            first_turn: true,
            num_turns: 0,
            forge_times: 0,
            threshold_reached: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct AwakenedOneRuntimeState {
    pub protocol_seeded: bool,
    pub form1: bool,
    pub first_turn: bool,
}

impl Default for AwakenedOneRuntimeState {
    fn default() -> Self {
        Self {
            protocol_seeded: false,
            form1: true,
            first_turn: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CorruptHeartRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
    pub move_count: u8,
    pub buff_count: u8,
    pub blood_hit_count: u8,
}

impl Default for CorruptHeartRuntimeState {
    fn default() -> Self {
        Self {
            protocol_seeded: false,
            first_move: true,
            move_count: 0,
            buff_count: 0,
            blood_hit_count: 12,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct WrithingMassRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
    pub used_mega_debuff: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SpikerRuntimeState {
    pub protocol_seeded: bool,
    pub thorns_count: u8,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SpireShieldRuntimeState {
    pub protocol_seeded: bool,
    pub move_count: u8,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SpireSpearRuntimeState {
    pub protocol_seeded: bool,
    pub move_count: u8,
    pub skewer_count: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SlaverRedRuntimeState {
    pub protocol_seeded: bool,
    pub first_turn: bool,
    pub used_entangle: bool,
}

impl Default for SlaverRedRuntimeState {
    fn default() -> Self {
        Self {
            protocol_seeded: false,
            first_turn: true,
            used_entangle: false,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GremlinLeaderRuntimeState {
    pub protocol_seeded: bool,
    pub gremlin_slots: [Option<EntityId>; 3],
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GremlinNobRuntimeState {
    pub protocol_seeded: bool,
    pub used_bellow: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GremlinWizardRuntimeState {
    pub protocol_seeded: bool,
    pub current_charge: u8,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct CultistRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SentryRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SlimeBossRuntimeState {
    pub protocol_seeded: bool,
    pub first_turn: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LargeSlimeRuntimeState {
    pub protocol_seeded: bool,
    pub split_triggered: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SphericGuardianRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
    pub second_move: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ReptomancerRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
    pub dagger_slots: [Option<EntityId>; 4],
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct DarklingRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
    pub nip_dmg: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct NemesisRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
    pub scythe_cooldown: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GiantHeadRuntimeState {
    pub protocol_seeded: bool,
    pub count: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TimeEaterRuntimeState {
    pub protocol_seeded: bool,
    pub used_haste: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct DonuRuntimeState {
    pub protocol_seeded: bool,
    pub is_attacking: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct DecaRuntimeState {
    pub protocol_seeded: bool,
    pub is_attacking: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct TransientRuntimeState {
    pub protocol_seeded: bool,
    pub count: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct ExploderRuntimeState {
    pub protocol_seeded: bool,
    pub turn_count: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct MawRuntimeState {
    pub protocol_seeded: bool,
    pub roared: bool,
    pub turn_count: i32,
}

impl Default for MawRuntimeState {
    fn default() -> Self {
        Self {
            protocol_seeded: false,
            roared: false,
            turn_count: 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct SnakeDaggerRuntimeState {
    pub protocol_seeded: bool,
    pub first_move: bool,
}

impl Default for SnakeDaggerRuntimeState {
    fn default() -> Self {
        Self {
            protocol_seeded: false,
            first_move: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LagavulinRuntimeState {
    pub is_out: bool,
    pub idle_count: u8,
    pub debuff_turn_count: u8,
    pub is_out_triggered: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct GuardianRuntimeState {
    pub damage_threshold: i32,
    pub damage_taken: i32,
    pub is_open: bool,
    pub close_up_triggered: bool,
}

impl Default for GuardianRuntimeState {
    fn default() -> Self {
        Self {
            damage_threshold: 0,
            damage_taken: 0,
            is_open: true,
            close_up_triggered: false,
        }
    }
}
