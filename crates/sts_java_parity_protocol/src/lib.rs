use serde::{Deserialize, Serialize};
use serde_json::Value;
use sts_parity_lockstep::NaturalRunRngCallV1;

pub const PROTOCOL_NAME: &str = "communicationmod.parity";
pub const PROTOCOL_MAJOR: u32 = 1;
pub const PROTOCOL_MINOR: u32 = 101;
pub const FRAME_SCHEMA_ID: &str = "communicationmod.parity-frame.v1";
pub const SNAPSHOT_PROFILE: &str = "parity_snapshot";
pub const CONTROL_PROFILE: &str = "parity_combat_control";
pub const RUN_CONTROL_PROFILE: &str = "parity_run_control";
pub const SCENARIO_CONTROL_PROFILE: &str = "parity_scenario_control";
pub const OBSERVATION_GRANTS: [&str; 3] = ["public_base", "parity_internal", "rng_internal"];
pub const COMBAT_CONTROL_GRANTS: [&str; 2] = ["combat_play_card", "combat_end_turn"];
pub const RUN_CONTROL_GRANTS: [&str; 44] = [
    "run_start",
    "run_continue_save",
    "run_reset_to_main_menu_preserving_save",
    "run_discard_to_main_menu",
    "run_choose_event",
    "run_choose_map_node",
    "run_close_map",
    "run_claim_combat_reward",
    "run_close_combat_reward",
    "run_choose_card_reward",
    "run_take_singing_bowl",
    "run_enter_shop",
    "run_open_shop_rewards",
    "run_proceed_shop_room",
    "run_buy_shop_card",
    "run_buy_shop_relic",
    "run_buy_shop_potion",
    "run_begin_shop_purge",
    "run_choose_shop_purge_card",
    "run_confirm_shop_purge_card",
    "run_leave_shop",
    "run_begin_campfire_rest",
    "run_begin_campfire_recall",
    "run_begin_campfire_smith",
    "run_begin_campfire_dig",
    "run_choose_smith_card",
    "run_confirm_smith_card",
    "run_proceed_rest_room",
    "run_open_chest",
    "run_proceed_treasure",
    "run_open_boss_chest",
    "run_proceed_boss_treasure",
    "run_proceed_double_boss",
    "run_proceed_act3_victory",
    "run_proceed_true_victory",
    "run_discard_potion",
    "run_use_potion",
    "run_acknowledge_ftue",
    "run_submit_hand_select",
    "run_submit_grid_select",
    "run_choose_boss_relic",
    "combat_use_potion",
    "combat_play_card",
    "combat_end_turn",
];
pub const SCENARIO_CONTROL_GRANTS: [&str; 10] = [
    "run_start",
    "run_acknowledge_ftue",
    "scenario_create_combat",
    "scenario_reset_to_main_menu",
    "run_claim_combat_reward",
    "run_close_combat_reward",
    "run_submit_hand_select",
    "combat_use_potion",
    "combat_play_card",
    "combat_end_turn",
];

/// Shared closed vocabulary for combat pile-selection screens.
/// Callers still validate counts, flags, and the source-bound candidate objects.
pub fn natural_combat_grid_kind_v1(
    source_pile: Option<&str>,
    reason: Option<&str>,
) -> Option<&'static str> {
    match (source_pile, reason) {
        (Some("Exhaust"), Some("Exhume")) => Some("exhume"),
        (Some("Discard"), Some("DiscardToHand")) => Some("discard_to_hand"),
        (Some("Discard"), Some("DiscardToHandNoCostChange")) => {
            Some("discard_to_hand_no_cost_change")
        }
        (Some("Discard"), Some("DiscardToHandRetain")) => Some("discard_to_hand_retain"),
        (Some("Discard"), Some("MoveToDrawPile")) => Some("discard_to_draw"),
        (Some("Draw"), Some("SkillFromDeckToHand")) => Some("skill_from_deck_to_hand"),
        (Some("Draw"), Some("AttackFromDeckToHand")) => Some("attack_from_deck_to_hand"),
        (Some("Draw"), Some("DrawPileToHand")) => Some("draw_to_hand"),
        (Some("Draw"), Some("Omniscience")) => Some("omniscience"),
        (Some("Draw"), Some("Scry")) => Some("scry"),
        _ => None,
    }
}

/// Returns one stable cross-runtime identity for original-game entity IDs.
///
/// The original game sometimes exposes a historical external ID while the
/// simulator names the same typed entity after its current class or domain
/// concept.  These aliases are protocol facts, not caller-specific preview
/// exceptions, so every Java/Rust comparison must share this table.
pub fn canonical_game_id(value: &str) -> String {
    let compact: String = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();

    match compact.as_str() {
        "striker" | "strike" => "strike".to_owned(),
        "defendr" | "defend" => "defend".to_owned(),
        "ghostly" | "apparition" => "apparition".to_owned(),
        "underhandedstrike" | "sneakystrike" => "sneakystrike".to_owned(),
        "nightterror" | "nightmare" => "nightmare".to_owned(),
        "wraithformv2" | "wraithform" => "wraithform".to_owned(),
        "corpseexplosionpower" | "corpseexplosion" => "corpseexplosion".to_owned(),
        "venomology" | "alchemize" => "alchemize".to_owned(),
        "redo" | "recursion" => "recursion".to_owned(),
        "gash" | "claw" => "claw".to_owned(),
        "steam" | "steambarrier" => "steambarrier".to_owned(),
        "steampower" | "overclock" => "overclock".to_owned(),
        "undo" | "equilibrium" => "equilibrium".to_owned(),
        "vengeance" | "simmeringfury" => "simmeringfury".to_owned(),
        "clearthemind" | "tranquility" => "tranquility".to_owned(),
        "adaptation" | "rushdown" => "rushdown".to_owned(),
        "wireheading" | "foresight" => "foresight".to_owned(),
        "wireheadingpower" | "foresightpower" => "foresightpower".to_owned(),
        "endturndeath" | "endturndeathpower" => "endturndeathpower".to_owned(),
        "controlled" | "mentalfortresspower" => "mentalfortresspower".to_owned(),
        "pathtovictory" | "pressurepoints" => "pressurepoints".to_owned(),
        "fasting2" | "fasting" => "fasting".to_owned(),
        "theabacus" | "abacus" => "abacus".to_owned(),
        "thecourier" | "courier" => "courier".to_owned(),
        "yang" | "duality" => "duality".to_owned(),
        "cables" | "goldplatedcables" => "goldplatedcables".to_owned(),
        "neowsblessing" | "neowslament" => "neowslament".to_owned(),
        "fuzzylousenormal" | "lousenormal" => "lousenormal".to_owned(),
        "fuzzylousedefensive" | "lousedefensive" => "lousedefensive".to_owned(),
        "dagger" | "snakedagger" => "snakedagger".to_owned(),
        "slaverboss" | "taskmaster" => "taskmaster".to_owned(),
        "banditchild" | "banditpointy" => "banditpointy".to_owned(),
        "serpent" | "spiregrowth" => "spiregrowth".to_owned(),
        "philosophersstone" | "philosopherstone" => "philosopherstone".to_owned(),
        "weakened" | "weak" => "weak".to_owned(),
        "lifelink" | "regrow" => "regrow".to_owned(),
        "ringofthesnake" | "snakering" => "snakering".to_owned(),
        "snakeskull" | "sneckoskull" => "sneckoskull".to_owned(),
        "leeswaffle" | "waffle" => "waffle".to_owned(),
        "wingedgreaves" | "wingboots" => "wingboots".to_owned(),
        "toxicegg2" | "toxicegg" => "toxicegg".to_owned(),
        "frozenegg2" | "frozenegg" => "frozenegg".to_owned(),
        "moltenegg2" | "moltenegg" => "moltenegg".to_owned(),
        "weakpotion" | "weakenpotion" => "weakenpotion".to_owned(),
        "distilledchaospotion" | "distilledchaos" => "distilledchaos".to_owned(),
        "elixirpotion" | "elixir" => "elixir".to_owned(),
        _ => compact,
    }
}

pub fn game_ids_match(left: &str, right: &str) -> bool {
    canonical_game_id(left) == canonical_game_id(right)
}

/// Returns the shared schedule identity for one original-game encounter key.
///
/// Java's dungeon tables spell encounter cardinalities with digits while the
/// simulator's typed enum spells them as words. This is intentionally scoped
/// to encounter schedules: globally treating words and digits as equivalent
/// could collapse unrelated card, relic, event, or mod identities.
pub fn canonical_encounter_schedule_id_v1(value: &str) -> String {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut previous: Option<char> = None;
    for character in value.chars() {
        if !character.is_ascii_alphanumeric() {
            push_encounter_token(&mut tokens, &mut token);
            previous = None;
            continue;
        }
        let starts_new_token = previous.is_some_and(|prior| {
            (character.is_ascii_uppercase() && prior.is_ascii_lowercase())
                || (character.is_ascii_digit() && !prior.is_ascii_digit())
                || (!character.is_ascii_digit() && prior.is_ascii_digit())
        });
        if starts_new_token {
            push_encounter_token(&mut tokens, &mut token);
        }
        token.push(character.to_ascii_lowercase());
        previous = Some(character);
    }
    push_encounter_token(&mut tokens, &mut token);
    let id = tokens.concat();
    match id.as_str() {
        // The typed encounter enum predates Java's dungeon-table spelling.
        "thechamp" => "champ".to_owned(),
        // TheCity's "Chosen" constructs the single-monster ChosenAlone encounter.
        "chosenalone" => "chosen".to_owned(),
        // TheBeyond.generateStrongEnemies/generateElites use these exact keys.
        "themaw" => "maw".to_owned(),
        "thenemesis" => "nemesis".to_owned(),
        _ => id,
    }
}

fn push_encounter_token(tokens: &mut Vec<String>, token: &mut String) {
    if token.is_empty() {
        return;
    }
    let normalized = match token.as_str() {
        "zero" => "0",
        "one" => "1",
        "two" => "2",
        "three" => "3",
        "four" => "4",
        "five" => "5",
        "six" => "6",
        "seven" => "7",
        "eight" => "8",
        "nine" => "9",
        "ten" => "10",
        other => other,
    };
    tokens.push(normalized.to_owned());
    token.clear();
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub kind: &'static str,
    pub protocol: ProtocolRequest,
    pub client: ClientIdentity,
    pub requested_profile: String,
    pub requested_grants: Grants,
}

impl Hello {
    pub fn new(
        client_name: impl Into<String>,
        client_version: impl Into<String>,
        capture_id: impl Into<String>,
        profile: &str,
    ) -> Result<Self, String> {
        Ok(Self {
            kind: "hello",
            protocol: ProtocolRequest {
                name: PROTOCOL_NAME,
                min_major: PROTOCOL_MAJOR,
                max_major: PROTOCOL_MAJOR,
            },
            client: ClientIdentity {
                name: client_name.into(),
                version: client_version.into(),
                capture_id: capture_id.into(),
            },
            requested_profile: profile.to_owned(),
            requested_grants: Grants::for_profile(profile)?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRequest {
    pub name: &'static str,
    pub min_major: u32,
    pub max_major: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientIdentity {
    pub name: String,
    pub version: String,
    pub capture_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Grants {
    pub observation: Vec<String>,
    pub control: Vec<String>,
}

impl Grants {
    pub fn for_profile(profile: &str) -> Result<Self, String> {
        let control = match profile {
            SNAPSHOT_PROFILE => Vec::new(),
            CONTROL_PROFILE => COMBAT_CONTROL_GRANTS
                .iter()
                .map(|grant| (*grant).to_owned())
                .collect(),
            RUN_CONTROL_PROFILE => RUN_CONTROL_GRANTS
                .iter()
                .map(|grant| (*grant).to_owned())
                .collect(),
            SCENARIO_CONTROL_PROFILE => SCENARIO_CONTROL_GRANTS
                .iter()
                .map(|grant| (*grant).to_owned())
                .collect(),
            _ => return Err(format!("unsupported parity profile: {profile}")),
        };
        Ok(Self {
            observation: OBSERVATION_GRANTS
                .iter()
                .map(|grant| (*grant).to_owned())
                .collect(),
            control,
        })
    }

    pub fn validate_for(&self, profile: &str) -> Result<(), String> {
        if *self != Self::for_profile(profile)? {
            return Err(format!(
                "grants do not exactly match parity profile {profile}"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProtocolDescriptor {
    pub name: String,
    pub major: u32,
    pub minor: u32,
    pub schema_id: String,
}

impl ProtocolDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        if self.name != PROTOCOL_NAME
            || self.major != PROTOCOL_MAJOR
            || self.minor != PROTOCOL_MINOR
            || self.schema_id != FRAME_SCHEMA_ID
        {
            return Err("protocol descriptor does not match parity major 1".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloAck {
    pub kind: String,
    pub protocol: ProtocolDescriptor,
    pub producer_session_id: String,
    pub connection_id: String,
    pub capture_id: String,
    pub profile: String,
    pub grants: Grants,
    pub producer: ProducerIdentity,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerIdentity {
    pub source_commit: String,
    pub source_dirty: bool,
    pub source_worktree_sha256: String,
    pub build_timestamp: String,
    pub project_version: String,
    pub loaded_mod_jar_sha256: String,
    pub sts_version: String,
    pub sts_jar_sha256: String,
    pub mod_the_spire_version: String,
    pub mod_the_spire_jar_sha256: String,
    pub loaded_mods: Vec<LoadedModIdentity>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadedModIdentity {
    pub id: String,
    pub version: String,
    pub jar_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateFrame {
    pub kind: String,
    pub protocol: ProtocolDescriptor,
    pub producer_session_id: String,
    pub connection_id: String,
    pub capture_id: String,
    pub frame_id: u64,
    pub previous_frame_id: Option<u64>,
    pub profile: String,
    pub grants: Grants,
    pub boundary: Boundary,
    pub public_base: PublicBase,
    pub parity_internal: ParityInternal,
    pub rng_internal: RngInternal,
    #[serde(default)]
    pub rng_call_witness: Option<RngCallWitness>,
    #[serde(default)]
    pub execution_input_witness: Option<ExecutionInputWitness>,
    #[serde(default)]
    pub card_birth_witness: Option<CardBirthWitness>,
    #[serde(default)]
    pub action_space: Option<CombatActionSpace>,
    #[serde(default)]
    pub scenario: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RngCallWitness {
    pub kind: String,
    pub schema: String,
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub request_id: String,
    pub action_id: String,
    pub complete: bool,
    pub calls: Vec<NaturalRunRngCallV1>,
    #[serde(default)]
    pub unsupported_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionInputWitness {
    pub kind: String,
    pub schema: String,
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub request_id: String,
    pub action_id: String,
    pub complete: bool,
    pub inputs: sts_parity_lockstep::ExecutionInputsV1,
    #[serde(default)]
    pub unsupported_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CardBirthWitness {
    pub kind: String,
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub request_id: String,
    pub action_id: String,
    pub cause: CardBirthCause,
    pub complete: bool,
    pub products: Vec<CardBirthProduct>,
    #[serde(default)]
    pub unsupported_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum CardBirthCause {
    CombatEntryResolution,
    CardPlay {
        producer_java_uuid: String,
    },
    EndTurnResolution,
    DiscoveryResolution,
    CombatSelectionResolution,
    PotionUse {
        potion_id: String,
        slot: usize,
    },
    RelicTrigger {
        relic_id: String,
    },
    PowerTrigger {
        power_id: String,
        owner_instance_id: u64,
    },
    EncounterRule {
        encounter_id: String,
    },
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CardBirthProduct {
    pub generation_index: u32,
    pub java_uuid: String,
    pub card_id: String,
    pub upgrades: u8,
    pub first_zone: Option<CardBirthZone>,
    pub autoplay_copy: Option<CardBirthAutoplayCopy>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CardBirthAutoplayCopy {
    pub producer_java_uuid: String,
    pub purged: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CardBirthZone {
    Draw,
    Hand,
    Discard,
    Exhaust,
    Limbo,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundary {
    pub kind: String,
    pub stable: bool,
    pub capture_point: String,
    pub evidence: BoundaryEvidence,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryEvidence {
    pub in_dungeon: bool,
    pub room_phase: String,
    pub room_wait_timer: f32,
    pub screen_up: bool,
    pub fading_in: bool,
    pub fading_out: bool,
    pub battle_over: bool,
    pub player_alive: bool,
    pub monsters_basically_dead: bool,
    pub action_manager_phase: String,
    pub current_action_class: Option<String>,
    pub actions_pending: u64,
    pub pre_turn_actions_pending: u64,
    pub card_queue_pending: u64,
    pub monster_queue_pending: u64,
    pub using_card: bool,
    pub end_turn_queued: bool,
    pub turn_has_ended: bool,
    pub monster_attacks_queued: bool,
    pub action_manager_has_control: bool,
    pub mod_update_blocked: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicBase {
    pub run: Value,
    pub combat: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParityInternal {
    pub run_seed: Option<String>,
    #[serde(default)]
    pub content_profile: Option<String>,
    #[serde(default)]
    pub card_pools: Option<Value>,
    /// Current Java master-deck group order. This is an observation ordinal,
    /// not an acquisition identity and must never be compared to a Rust
    /// lineage ordinal without an explicit binding owner.
    #[serde(default)]
    pub master_deck: Option<Vec<MasterDeckCard>>,
    pub zones: Value,
    #[serde(default)]
    pub monsters: Vec<Value>,
    #[serde(default)]
    pub relics: Vec<Value>,
    #[serde(default)]
    pub potions: Vec<Value>,
    #[serde(default)]
    pub player_powers: Vec<Value>,
    #[serde(default)]
    pub combat_rewards: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MasterDeckCard {
    pub ordinal: usize,
    pub card_uuid: String,
    pub card_id: String,
    pub upgrades: u8,
    #[serde(default)]
    pub source: Option<NaturalRunCardSource>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NaturalRunCardSource {
    StarterDeck {
        player_class: String,
        source_slot: u32,
    },
    AscendersBane {
        rule: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RngInternal {
    pub streams: Vec<RngStream>,
    #[serde(default)]
    pub run_streams: Vec<RngStream>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RngStream {
    pub owner: String,
    pub algorithm: String,
    pub scope: String,
    pub state_hex: Vec<String>,
    pub counter: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatActionSpace {
    pub kind: String,
    pub actions: Vec<CombatAction>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAction {
    pub action_id: String,
    pub kind: String,
    #[serde(default)]
    pub card_uuid: Option<String>,
    #[serde(default)]
    pub card_id: Option<String>,
    #[serde(default)]
    pub hand_index: Option<usize>,
    #[serde(default)]
    pub potion_slot: Option<usize>,
    #[serde(default)]
    pub potion_id: Option<String>,
    #[serde(default)]
    pub potion_potency: Option<i32>,
    #[serde(default)]
    pub target_monster_instance_id: Option<u64>,
    #[serde(default)]
    pub target_monster_index: Option<usize>,
    #[serde(default)]
    pub target_monster_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRequest {
    pub kind: &'static str,
    pub connection_id: String,
    pub request_id: String,
    pub based_on_frame_id: u64,
    pub action_id: String,
    pub parameters: Value,
}

impl ActionRequest {
    pub fn new(
        connection_id: impl Into<String>,
        request_id: impl Into<String>,
        based_on_frame_id: u64,
        action_id: impl Into<String>,
    ) -> Self {
        Self {
            kind: "action_request",
            connection_id: connection_id.into(),
            request_id: request_id.into(),
            based_on_frame_id,
            action_id: action_id.into(),
            parameters: Value::Object(serde_json::Map::new()),
        }
    }

    pub fn with_parameters(mut self, parameters: Value) -> Result<Self, String> {
        if !parameters.is_object() {
            return Err("action parameters must be a JSON object".to_owned());
        }
        self.parameters = parameters;
        Ok(self)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionResult {
    pub kind: String,
    pub protocol: ProtocolDescriptor,
    pub connection_id: String,
    pub request_id: String,
    pub based_on_frame_id: u64,
    pub action_id: String,
    pub status: String,
    #[serde(default)]
    pub code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolError {
    pub kind: String,
    pub code: String,
    pub message: String,
    pub connection_id: Option<String>,
    pub rejected_input_characters: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_runtime_game_id_aliases_have_one_shared_identity() {
        for (stable, external, canonical) in [
            ("Strike", "Strike_R", "strike"),
            ("Apparition", "Ghostly", "apparition"),
            ("SneakyStrike", "Underhanded Strike", "sneakystrike"),
            ("Nightmare", "Night Terror", "nightmare"),
            ("WraithForm", "Wraith Form v2", "wraithform"),
            ("CorpseExplosion", "CorpseExplosionPower", "corpseexplosion"),
            ("Alchemize", "Venomology", "alchemize"),
            ("Recursion", "Redo", "recursion"),
            ("Claw", "Gash", "claw"),
            ("SteamBarrier", "Steam", "steambarrier"),
            ("Overclock", "Steam Power", "overclock"),
            ("Equilibrium", "Undo", "equilibrium"),
            ("SimmeringFury", "Vengeance", "simmeringfury"),
            ("Tranquility", "ClearTheMind", "tranquility"),
            ("Rushdown", "Adaptation", "rushdown"),
            ("Foresight", "Wireheading", "foresight"),
            ("ForesightPower", "WireheadingPower", "foresightpower"),
            ("EndTurnDeathPower", "EndTurnDeath", "endturndeathpower"),
            ("MentalFortressPower", "Controlled", "mentalfortresspower"),
            ("PressurePoints", "PathToVictory", "pressurepoints"),
            ("Fasting", "Fasting2", "fasting"),
            ("NeowsLament", "NeowsBlessing", "neowslament"),
            ("LouseNormal", "FuzzyLouseNormal", "lousenormal"),
            ("SnakeDagger", "Dagger", "snakedagger"),
            ("Taskmaster", "SlaverBoss", "taskmaster"),
            ("BanditPointy", "BanditChild", "banditpointy"),
            ("SpireGrowth", "Serpent", "spiregrowth"),
            ("LouseDefensive", "FuzzyLouseDefensive", "lousedefensive"),
            ("GoldPlatedCables", "Cables", "goldplatedcables"),
            ("WeakenPotion", "Weak Potion", "weakenpotion"),
            ("DistilledChaosPotion", "Distilled Chaos", "distilledchaos"),
            ("Weak", "Weakened", "weak"),
            ("Regrow", "Life Link", "regrow"),
        ] {
            assert!(game_ids_match(stable, external));
            assert_eq!(canonical_game_id(stable), canonical);
            assert_eq!(canonical_game_id(external), canonical);
        }
        assert!(!game_ids_match("FrozenEgg", "Toxic Egg 2"));
        assert!(!game_ids_match("Wireheading", "WireheadingPower"));
        assert!(!game_ids_match("BanditPointy", "BanditLeader"));
        assert!(!game_ids_match("BanditPointy", "BanditBear"));
        assert!(!game_ids_match("Ghostly", "Ghostly Armor"));
        assert!(!game_ids_match("SlaverBoss", "SlaverRed"));
        assert!(!game_ids_match("SlaverBoss", "SlaverBlue"));
    }

    #[test]
    fn encounter_schedule_identity_normalizes_numeric_tokens_only_in_its_scope() {
        for (rust, java, expected) in [
            ("TwoLouse", "2 Louse", "2louse"),
            ("ThreeSentries", "3 Sentries", "3sentries"),
            (
                "SphereAndTwoShapes",
                "Sphere and 2 Shapes",
                "sphereand2shapes",
            ),
            ("The Guardian", "TheGuardian", "theguardian"),
            ("TheChamp", "Champ", "champ"),
            ("ChosenAlone", "Chosen", "chosen"),
            ("TheMaw", "Maw", "maw"),
            ("TheNemesis", "Nemesis", "nemesis"),
        ] {
            assert_eq!(canonical_encounter_schedule_id_v1(rust), expected);
            assert_eq!(canonical_encounter_schedule_id_v1(java), expected);
        }
        assert_eq!(canonical_game_id("TwoLouse"), "twolouse");
        assert_eq!(canonical_game_id("TheNemesis"), "thenemesis");
        assert_eq!(canonical_game_id("ChosenAlone"), "chosenalone");
        assert_ne!(
            canonical_encounter_schedule_id_v1("ChosenAlone"),
            canonical_encounter_schedule_id_v1("ChosenAndByrds")
        );
        assert_ne!(
            canonical_encounter_schedule_id_v1("ChosenAlone"),
            canonical_encounter_schedule_id_v1("CultistAndChosen")
        );
    }

    #[test]
    fn profiles_have_distinct_exact_control_grants() {
        assert!(Grants::for_profile(SNAPSHOT_PROFILE)
            .unwrap()
            .control
            .is_empty());
        assert_eq!(
            Grants::for_profile(CONTROL_PROFILE).unwrap().control,
            ["combat_play_card", "combat_end_turn"]
        );
        assert_eq!(
            Grants::for_profile(RUN_CONTROL_PROFILE).unwrap().control,
            [
                "run_start",
                "run_continue_save",
                "run_reset_to_main_menu_preserving_save",
                "run_discard_to_main_menu",
                "run_choose_event",
                "run_choose_map_node",
                "run_close_map",
                "run_claim_combat_reward",
                "run_close_combat_reward",
                "run_choose_card_reward",
                "run_take_singing_bowl",
                "run_enter_shop",
                "run_open_shop_rewards",
                "run_proceed_shop_room",
                "run_buy_shop_card",
                "run_buy_shop_relic",
                "run_buy_shop_potion",
                "run_begin_shop_purge",
                "run_choose_shop_purge_card",
                "run_confirm_shop_purge_card",
                "run_leave_shop",
                "run_begin_campfire_rest",
                "run_begin_campfire_recall",
                "run_begin_campfire_smith",
                "run_begin_campfire_dig",
                "run_choose_smith_card",
                "run_confirm_smith_card",
                "run_proceed_rest_room",
                "run_open_chest",
                "run_proceed_treasure",
                "run_open_boss_chest",
                "run_proceed_boss_treasure",
                "run_proceed_double_boss",
                "run_proceed_act3_victory",
                "run_proceed_true_victory",
                "run_discard_potion",
                "run_use_potion",
                "run_acknowledge_ftue",
                "run_submit_hand_select",
                "run_submit_grid_select",
                "run_choose_boss_relic",
                "combat_use_potion",
                "combat_play_card",
                "combat_end_turn"
            ]
        );
        assert_eq!(
            Grants::for_profile(SCENARIO_CONTROL_PROFILE)
                .unwrap()
                .control,
            [
                "run_start",
                "run_acknowledge_ftue",
                "scenario_create_combat",
                "scenario_reset_to_main_menu",
                "run_claim_combat_reward",
                "run_close_combat_reward",
                "run_submit_hand_select",
                "combat_use_potion",
                "combat_play_card",
                "combat_end_turn"
            ]
        );
        let scenario = Grants::for_profile(SCENARIO_CONTROL_PROFILE)
            .unwrap()
            .control;
        assert!(!scenario.contains(&"run_choose_event".to_owned()));
        assert!(!scenario.contains(&"run_choose_map_node".to_owned()));
        assert!(scenario.contains(&"run_claim_combat_reward".to_owned()));
        assert!(scenario.contains(&"run_close_combat_reward".to_owned()));
    }

    #[test]
    fn action_request_is_frame_and_connection_bound() {
        let request = ActionRequest::new("connection", "request", 4, "end_turn");
        let encoded = serde_json::to_value(request).unwrap();
        assert_eq!(encoded["connection_id"], "connection");
        assert_eq!(encoded["based_on_frame_id"], 4);
        assert_eq!(encoded["action_id"], "end_turn");
        assert_eq!(encoded["parameters"], serde_json::json!({}));
    }

    #[test]
    fn typed_potion_action_preserves_slot_identity_potency_and_target() {
        let action: CombatAction = serde_json::from_value(serde_json::json!({
            "action_id": "use_potion:1:Block Potion:none",
            "kind": "use_potion",
            "potion_slot": 1,
            "potion_id": "Block Potion",
            "potion_potency": 12
        }))
        .unwrap();

        assert_eq!(action.potion_slot, Some(1));
        assert_eq!(action.potion_id.as_deref(), Some("Block Potion"));
        assert_eq!(action.potion_potency, Some(12));
        assert_eq!(action.target_monster_instance_id, None);
    }

    #[test]
    fn combat_parity_internal_accepts_current_evidence_arrays_only() {
        let value = serde_json::json!({
            "run_seed": "0",
            "master_deck": [{
                "ordinal": 0,
                "card_uuid": "00000000-0000-0000-0000-000000000001",
                "card_id": "Strike_R",
                "upgrades": 0,
                "source": {
                    "kind": "starter_deck",
                    "player_class": "IRONCLAD",
                    "source_slot": 0
                }
            }],
            "zones": {},
            "monsters": [{"instance_id": 1}],
            "relics": [{"index": 0, "relic_id": "Burning Blood"}],
            "player_powers": [{
                "index": 0,
                "id": "Corruption",
                "class_name": "com.megacrit.cardcrawl.powers.CorruptionPower",
                "amount": -1,
                "type": "BUFF"
            }]
        });
        let parsed: ParityInternal = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.monsters.len(), 1);
        assert_eq!(parsed.relics.len(), 1);
        assert_eq!(parsed.player_powers.len(), 1);
        assert_eq!(parsed.master_deck.as_ref().unwrap()[0].ordinal, 0);
        assert_eq!(
            parsed.master_deck.as_ref().unwrap()[0].source,
            Some(NaturalRunCardSource::StarterDeck {
                player_class: "IRONCLAD".to_owned(),
                source_slot: 0,
            })
        );

        let unknown = serde_json::json!({
            "run_seed": "0",
            "zones": {},
            "monsters": [],
            "relics": [],
            "player_powers": [],
            "arbitrary_internal": []
        });
        assert!(serde_json::from_value::<ParityInternal>(unknown).is_err());
    }
}
