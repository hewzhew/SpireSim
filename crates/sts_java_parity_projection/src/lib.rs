//! Stable Java observation and PairedRoot projection boundary.
//!
//! This crate does not launch Java, interpret Rust simulator state, compare
//! runtimes, or publish recovery certificates. It owns the private, content-addressed
//! observation handed from one disposable validator to one Engine job and the
//! Java-only canonical projections required to validate supported natural
//! decision roots.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use sts_java_parity_protocol::{canonical_game_id, ProtocolDescriptor, RUN_CONTROL_PROFILE};
use sts_paired_root::{
    CardBindingEvidenceV1, CrossRuntimeCardBindingV1, JavaCardLeaseV1, MapRoomIdentityV1,
    NeowDrawbackIdentityV1, NeowRewardIdentityV1, ProjectionDomainV1, VersionIdentityV1,
};

pub const NATIVE_RECOVERY_OBSERVATION_SCHEMA_V1: &str =
    "sts-workbench-native-recovery-observation-v1";
pub const NATIVE_RECOVERY_OBSERVATION_REF_DOMAIN_V1: &str =
    "workbench-native-recovery-observation-v1";
pub const PAIRED_PROJECTION_MEDIA_TYPE_V1: &str = "application/vnd.sts.paired-projection+json";

pub const RUN_SCALARS_SCHEMA_V1: &str = "sts-paired-run-scalars-v1";
pub const MASTER_DECK_SCHEMA_V1: &str = "sts-paired-master-deck-order-v1";
pub const RELICS_SCHEMA_V1: &str = "sts-paired-relic-order-counter-v1";
pub const PERSISTENT_POTIONS_SCHEMA_V1: &str = "sts-paired-persistent-potion-slots-v1";
pub const RNG_STREAMS_SCHEMA_V1: &str = "sts-paired-rng-streams-v1";
pub const CARD_POOLS_SCHEMA_V1: &str = "sts-paired-card-pools-v1";
pub const PUBLIC_MAP_SCHEMA_V1: &str = "sts-paired-public-map-v1";
pub const MAP_ACTION_SPACE_SCHEMA_V1: &str = "sts-paired-map-action-space-v1";
pub const CARD_BINDINGS_SCHEMA_V1: &str = "sts-paired-master-deck-binding-relation-v1";
pub const MAP_RUNTIME_SETTLEMENT_SCHEMA_V1: &str = "sts-paired-map-runtime-settlement-v1";
pub const NEOW_ROOM_SCHEMA_V1: &str = "sts-paired-neow-room-v1";
pub const NEOW_ACTION_SPACE_SCHEMA_V1: &str = "sts-paired-neow-action-space-v1";
pub const NEOW_RUNTIME_SETTLEMENT_SCHEMA_V1: &str = "sts-paired-neow-runtime-settlement-v1";
pub const NEOW_RNG_STREAMS_SCHEMA_V1: &str = "sts-paired-neow-rng-streams-v1";

const NEOW_EVENT_ID: &str = "Neow";
const JAVA_NEOW_RNG_ID: &str = "com.megacrit.cardcrawl.neow.NeowEvent.rng";
const JAVA_MISC_RNG_ID: &str = "com.megacrit.cardcrawl.dungeons.AbstractDungeon.miscRng";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRecoveryObservationScopeV1 {
    pub engine_epoch: u64,
    pub job_id: String,
}

impl NativeRecoveryObservationScopeV1 {
    pub fn new(engine_epoch: u64, job_id: impl Into<String>) -> Result<Self, String> {
        let scope = Self {
            engine_epoch,
            job_id: job_id.into(),
        };
        scope.validate()?;
        Ok(scope)
    }

    fn validate(&self) -> Result<(), String> {
        if self.engine_epoch == 0 || !safe_token(&self.job_id) {
            return Err("native recovery observation scope is invalid".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRecoveryObservationBodyV1 {
    pub scope: NativeRecoveryObservationScopeV1,
    pub java_versions: Vec<VersionIdentityV1>,
    pub java_runtime_epoch: String,
    pub frame: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRecoveryObservationArtifactV1 {
    pub schema: String,
    pub observation_ref: String,
    pub observation: NativeRecoveryObservationBodyV1,
}

/// Private capability store for fresh-validator observations.
///
/// It has no enumeration or model-facing inspect API. Loading requires both
/// the exact content ref and the Engine job scope that authorized its creation.
#[derive(Clone, Debug)]
pub struct NativeRecoveryObservationStoreV1 {
    directory: PathBuf,
}

impl NativeRecoveryObservationStoreV1 {
    pub fn new(repository: &Path) -> Result<Self, String> {
        let repository = fs::canonicalize(repository).map_err(|error| {
            format!("failed to resolve native recovery observation repository: {error}")
        })?;
        if !repository.is_dir() {
            return Err("native recovery observation repository is not a directory".to_owned());
        }
        Ok(Self {
            directory: repository
                .join(".oracle-lab")
                .join("workbench-service")
                .join("native-recovery-observations"),
        })
    }

    pub fn publish(
        &self,
        scope: NativeRecoveryObservationScopeV1,
        java_versions: Vec<VersionIdentityV1>,
        frame: Value,
    ) -> Result<NativeRecoveryObservationArtifactV1, String> {
        scope.validate()?;
        let java_runtime_epoch = java_runtime_epoch(&frame)?;
        let observation = NativeRecoveryObservationBodyV1 {
            scope,
            java_versions,
            java_runtime_epoch,
            frame,
        };
        validate_observation(&observation)?;
        let observation_ref = scoped_content_id(
            NATIVE_RECOVERY_OBSERVATION_REF_DOMAIN_V1,
            &canonical_json(&observation)?,
        );
        let artifact = NativeRecoveryObservationArtifactV1 {
            schema: NATIVE_RECOVERY_OBSERVATION_SCHEMA_V1.to_owned(),
            observation_ref: observation_ref.clone(),
            observation,
        };
        fs::create_dir_all(&self.directory).map_err(|error| {
            format!("failed to create native recovery observation store: {error}")
        })?;
        publish_immutable(
            &self.directory.join(format!("{observation_ref}.json")),
            &canonical_json(&artifact)?,
        )?;
        self.load(&observation_ref, &artifact.observation.scope)
    }

    pub fn load(
        &self,
        observation_ref: &str,
        expected_scope: &NativeRecoveryObservationScopeV1,
    ) -> Result<NativeRecoveryObservationArtifactV1, String> {
        expected_scope.validate()?;
        require_scoped_content_id(observation_ref, NATIVE_RECOVERY_OBSERVATION_REF_DOMAIN_V1)?;
        let bytes = fs::read(self.directory.join(format!("{observation_ref}.json")))
            .map_err(|error| format!("failed to read native recovery observation: {error}"))?;
        let artifact: NativeRecoveryObservationArtifactV1 = serde_json::from_slice(&bytes)
            .map_err(|error| format!("invalid native recovery observation: {error}"))?;
        if artifact.schema != NATIVE_RECOVERY_OBSERVATION_SCHEMA_V1
            || artifact.observation_ref != observation_ref
            || &artifact.observation.scope != expected_scope
        {
            return Err("native recovery observation identity is invalid".to_owned());
        }
        validate_observation(&artifact.observation)?;
        let expected_ref = scoped_content_id(
            NATIVE_RECOVERY_OBSERVATION_REF_DOMAIN_V1,
            &canonical_json(&artifact.observation)?,
        );
        if expected_ref != observation_ref {
            return Err("native recovery observation content identity changed".to_owned());
        }
        Ok(artifact)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JavaProjectionComponentV1 {
    pub domain: ProjectionDomainV1,
    pub schema: &'static str,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JavaStableDecisionProjectionV1 {
    pub java_runtime_epoch: String,
    pub content_profile: String,
    pub components: Vec<JavaProjectionComponentV1>,
    pub card_leases: Vec<JavaCardLeaseV1>,
}

impl JavaStableDecisionProjectionV1 {
    pub fn component(&self, domain: ProjectionDomainV1) -> Option<&JavaProjectionComponentV1> {
        self.components
            .iter()
            .find(|component| component.domain == domain)
    }
}

/// Independently projects a fresh Java Map observation against the target
/// root's already-proven stable card bindings.
pub fn project_natural_map_root(
    frame: &Value,
    bindings: &[CrossRuntimeCardBindingV1],
) -> Result<JavaStableDecisionProjectionV1, String> {
    validate_natural_map_boundary(frame)?;
    let content_profile = required_str(&frame["parity_internal"], "content_profile")?.to_owned();
    let card_leases = rebuild_master_deck_leases(frame, bindings)?;
    let mut components = project_stable_noncombat_base(
        frame,
        &card_leases,
        RNG_STREAMS_SCHEMA_V1,
        project_post_neow_rng_streams(frame)?,
    )?;
    components.extend([
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::ActionSpace,
            schema: MAP_ACTION_SPACE_SCHEMA_V1,
            value: project_map_action_space(frame)?,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::RuntimeSettlement,
            schema: MAP_RUNTIME_SETTLEMENT_SCHEMA_V1,
            value: map_runtime_settlement(),
        },
    ]);
    canonicalize_components(&mut components)?;
    Ok(JavaStableDecisionProjectionV1 {
        java_runtime_epoch: java_runtime_epoch(frame)?,
        content_profile,
        components,
        card_leases,
    })
}

/// Independently projects a fresh Java Neow observation against the target
/// root's already-proven stable card bindings.
pub fn project_natural_neow_root(
    frame: &Value,
    bindings: &[CrossRuntimeCardBindingV1],
) -> Result<JavaStableDecisionProjectionV1, String> {
    validate_natural_neow_boundary(frame)?;
    let content_profile = required_str(&frame["parity_internal"], "content_profile")?.to_owned();
    let card_leases = rebuild_master_deck_leases(frame, bindings)?;
    let mut components = project_stable_noncombat_base(
        frame,
        &card_leases,
        NEOW_RNG_STREAMS_SCHEMA_V1,
        project_neow_rng_streams(frame)?,
    )?;
    components.extend([
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::Room,
            schema: NEOW_ROOM_SCHEMA_V1,
            value: project_neow_room(frame)?,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::ActionSpace,
            schema: NEOW_ACTION_SPACE_SCHEMA_V1,
            value: project_neow_action_space(frame)?,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::RuntimeSettlement,
            schema: NEOW_RUNTIME_SETTLEMENT_SCHEMA_V1,
            value: neow_runtime_settlement(),
        },
    ]);
    canonicalize_components(&mut components)?;
    Ok(JavaStableDecisionProjectionV1 {
        java_runtime_epoch: java_runtime_epoch(frame)?,
        content_profile,
        components,
        card_leases,
    })
}

fn project_stable_noncombat_base(
    frame: &Value,
    card_leases: &[JavaCardLeaseV1],
    rng_schema: &'static str,
    rng_value: Value,
) -> Result<Vec<JavaProjectionComponentV1>, String> {
    Ok(vec![
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::RunScalars,
            schema: RUN_SCALARS_SCHEMA_V1,
            value: project_run_scalars(frame)?,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::Map,
            schema: PUBLIC_MAP_SCHEMA_V1,
            value: project_public_map(frame)?,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::MasterDeck,
            schema: MASTER_DECK_SCHEMA_V1,
            value: project_master_deck(frame)?,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::Relics,
            schema: RELICS_SCHEMA_V1,
            value: project_relics(frame)?,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::Potions,
            schema: PERSISTENT_POTIONS_SCHEMA_V1,
            value: project_persistent_potions(frame)?,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::RngStreams,
            schema: rng_schema,
            value: rng_value,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::CardPools,
            schema: CARD_POOLS_SCHEMA_V1,
            value: project_card_pools(frame)?,
        },
        JavaProjectionComponentV1 {
            domain: ProjectionDomainV1::CardBindings,
            schema: CARD_BINDINGS_SCHEMA_V1,
            value: project_master_deck_bindings(frame, card_leases)?,
        },
    ])
}

fn canonicalize_components(components: &mut [JavaProjectionComponentV1]) -> Result<(), String> {
    components.sort_by_key(|component| component.domain);
    if components
        .windows(2)
        .any(|pair| pair[0].domain >= pair[1].domain)
    {
        return Err("Java decision projection domains are not canonical".to_owned());
    }
    Ok(())
}

pub fn project_run_scalars(frame: &Value) -> Result<Value, String> {
    let run = frame["public_base"]["run"]
        .as_object()
        .ok_or_else(|| "Java frame omitted public_base.run".to_owned())?;
    Ok(json!({
        "seed": canonical_engine_seed_from_java_long(required_str(&frame["parity_internal"], "run_seed")?)?,
        "ascension": required_u64_object(run, "ascension")?,
        "character": required_str_object(run, "character")?,
        "act": required_u64_object(run, "act")?,
        "floor": required_i64_object(run, "floor")?,
        "current_hp": required_i64_object(run, "current_hp")?,
        "max_hp": required_i64_object(run, "max_hp")?,
        "gold": required_i64_object(run, "gold")?,
        "final_act_available": required_bool_object(run, "final_act_available")?,
        "keys": [
            required_bool_object(run, "has_ruby_key")?,
            required_bool_object(run, "has_sapphire_key")?,
            required_bool_object(run, "has_emerald_key")?,
        ],
    }))
}

pub fn project_master_deck(frame: &Value) -> Result<Value, String> {
    let cards = master_deck(frame)?;
    Ok(Value::Array(
        cards
            .iter()
            .map(|card| {
                Ok(json!({
                    "ordinal": required_u64(card, "ordinal")?,
                    "card_id": required_str(card, "card_id")?,
                    "upgrades": required_u64(card, "upgrades")?,
                }))
            })
            .collect::<Result<Vec<_>, String>>()?,
    ))
}

pub fn project_relics(frame: &Value) -> Result<Value, String> {
    let relics = frame["parity_internal"]["relics"]
        .as_array()
        .ok_or_else(|| "Java frame omitted parity_internal.relics".to_owned())?;
    Ok(Value::Array(
        relics
            .iter()
            .map(|relic| {
                let id = canonical_game_id(required_str(relic, "relic_id")?);
                let counter = required_i64(relic, "counter")?;
                let mut projected = json!({
                    "ordinal": required_u64(relic, "index")?,
                    "id": id,
                    "counter": counter,
                });
                if projected["id"] == "neowslament" && counter <= 0 {
                    projected["used_up"] = Value::Bool(true);
                }
                Ok(projected)
            })
            .collect::<Result<Vec<_>, String>>()?,
    ))
}

pub fn project_persistent_potions(frame: &Value) -> Result<Value, String> {
    let potions = frame["parity_internal"]["potions"]
        .as_array()
        .ok_or_else(|| "Java frame omitted parity_internal.potions".to_owned())?;
    Ok(Value::Array(
        potions
            .iter()
            .enumerate()
            .map(|(expected_slot, potion)| {
                let slot = usize::try_from(required_u64(potion, "slot_index")?)
                    .map_err(|_| "Java persistent potion slot exceeds usize".to_owned())?;
                if slot != expected_slot {
                    return Err(format!(
                        "Java persistent potion slots are not contiguous: expected {expected_slot}, found {slot}"
                    ));
                }
                if required_bool(potion, "empty")? {
                    Ok(json!({"slot": slot, "id": Value::Null}))
                } else {
                    let potion_slot = usize::try_from(required_u64(potion, "potion_slot")?)
                        .map_err(|_| "Java occupied potion slot exceeds usize".to_owned())?;
                    if potion_slot != slot {
                        return Err(format!(
                            "occupied Java persistent potion slot {slot} reports internal slot {potion_slot}"
                        ));
                    }
                    Ok(json!({
                        "slot": slot,
                        "id": canonical_game_id(required_str(potion, "potion_id")?),
                    }))
                }
            })
            .collect::<Result<Vec<_>, String>>()?,
    ))
}

pub fn project_post_neow_rng_streams(frame: &Value) -> Result<Value, String> {
    let mut streams = project_all_rng_streams(frame)?;
    streams.retain(|stream| stream["owner"].as_str() != Some(JAVA_NEOW_RNG_ID));
    Ok(Value::Array(streams))
}

pub fn project_neow_rng_streams(frame: &Value) -> Result<Value, String> {
    let screen = neow_screen(frame)?;
    let mut streams = project_all_rng_streams(frame)?;
    // Original-game new-run construction can consume miscRng before Neow,
    // while native Continue recreates that floor-local stream from seed +
    // floor. No Neow reward path reads miscRng, and the first room transition
    // recreates it again, so it is outside this decision root's future RNG
    // capability rather than a reconstructible cursor.
    // Once the reward has settled at screen 99, the only remaining action is
    // Leave and the original button handler goes straight to openMap. The
    // event RNG is therefore equally dead at that completed decision root.
    streams.retain(|stream| {
        let owner = stream["owner"].as_str();
        owner != Some(JAVA_MISC_RNG_ID) && (screen == 3 || owner != Some(JAVA_NEOW_RNG_ID))
    });
    Ok(Value::Array(streams))
}

fn project_all_rng_streams(frame: &Value) -> Result<Vec<Value>, String> {
    let mut streams = Vec::new();
    for group in ["streams", "run_streams"] {
        let values = frame["rng_internal"][group]
            .as_array()
            .ok_or_else(|| format!("Java frame omitted rng_internal.{group}"))?;
        for stream in values {
            let owner = required_str(stream, "owner")?;
            streams.push(json!({
                "owner": owner,
                "scope": required_str(stream, "scope")?,
                "state_hex": stream["state_hex"].clone(),
                "counter": comparable_rng_counter(owner, required_i64(stream, "counter")?),
            }));
        }
    }
    streams.sort_by(|left, right| left["owner"].as_str().cmp(&right["owner"].as_str()));
    Ok(streams)
}

pub fn project_card_pools(frame: &Value) -> Result<Value, String> {
    let pools = frame["parity_internal"]["card_pools"]
        .as_object()
        .ok_or_else(|| "Java frame omitted parity_internal.card_pools".to_owned())?;
    Ok(json!({
        "common": required_string_array_object(pools, "common")?,
        "uncommon": required_string_array_object(pools, "uncommon")?,
        "rare": required_string_array_object(pools, "rare")?,
        "colorless_combat": required_string_array_object(pools, "colorless_combat")?,
    }))
}

pub fn project_public_map(frame: &Value) -> Result<Value, String> {
    let map = frame["public_base"]["map"]
        .as_object()
        .ok_or_else(|| "Java frame omitted its typed public map".to_owned())?;
    let map_value = Value::Object(map.clone());
    let current_x = to_i32(required_i64(&map_value, "current_x")?, "current_x")?;
    let current_y = to_i32(required_i64(&map_value, "current_y")?, "current_y")?;
    let boss_id = match map.get("boss") {
        Some(Value::String(value)) => Some(canonical_game_id(value)),
        Some(Value::Null) | None => None,
        Some(_) => return Err("Java public-map boss must be a string or null".to_owned()),
    };
    let raw_nodes = map
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| "Java public map omitted its node array".to_owned())?;
    let mut nodes = Vec::with_capacity(raw_nodes.len());
    for raw_node in raw_nodes {
        let node = raw_node
            .as_object()
            .ok_or_else(|| "Java public-map node is not an object".to_owned())?;
        let node_value = Value::Object(node.clone());
        let x = to_i32(required_i64(&node_value, "x")?, "node x")?;
        let y = to_i32(required_i64(&node_value, "y")?, "node y")?;
        let room_type = map_room_identity_from_java(required_str(&node_value, "room_type")?)?;
        if room_type == MapRoomIdentityV1::Neow {
            return Err("Java public-map topology cannot contain a Neow node".to_owned());
        }
        let has_emerald_key = required_bool(&node_value, "has_emerald_key")?;
        let raw_edges = node
            .get("edges")
            .and_then(Value::as_array)
            .ok_or_else(|| "Java public-map node omitted its edge array".to_owned())?;
        let edges = raw_edges
            .iter()
            .map(|raw_edge| {
                Ok(PublicMapEdgeProjectionV1 {
                    destination_x: to_i32(
                        required_i64(raw_edge, "destination_x")?,
                        "edge destination_x",
                    )?,
                    destination_y: to_i32(
                        required_i64(raw_edge, "destination_y")?,
                        "edge destination_y",
                    )?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        nodes.push(PublicMapNodeProjectionV1 {
            x,
            y,
            room_type,
            has_emerald_key,
            edges,
        });
    }
    let mut projection = PublicMapProjectionV1 {
        current_x,
        current_y,
        boss_id,
        boss_anchor: None,
        nodes,
    };
    validate_and_order_public_map(&mut projection)?;
    serde_json::to_value(projection)
        .map_err(|error| format!("failed to encode Java public map: {error}"))
}

pub fn project_map_action_space(frame: &Value) -> Result<Value, String> {
    let actions = frame["action_space"]["actions"]
        .as_array()
        .ok_or_else(|| "Java Map frame omitted action_space.actions".to_owned())?;
    let mut coordinates = BTreeSet::new();
    let mut projected = Vec::new();
    for action in actions
        .iter()
        .filter(|action| action["kind"].as_str() == Some("choose_map_node"))
    {
        let x = to_i32(required_i64(action, "map_x")?, "map action x")?;
        let y = to_i32(required_i64(action, "map_y")?, "map action y")?;
        if x < 0 || y < 0 || !coordinates.insert((x, y)) {
            return Err("Java Map action has negative or repeated coordinates".to_owned());
        }
        let room_type = map_room_identity_from_java(required_str(action, "map_room_type")?)?;
        let expected_action_id = format!("map_node:{x}:{y}");
        if required_str(action, "action_id")? != expected_action_id
            || !action.get("parameter_schema").is_none_or(Value::is_null)
        {
            return Err("Java Map action is not one parameter-free typed node".to_owned());
        }
        projected.push(json!({
            "kind": "choose_map_node",
            "x": x,
            "y": y,
            "room_type": room_type,
        }));
    }
    sort_json_values(&mut projected)?;
    Ok(Value::Array(projected))
}

pub fn map_runtime_settlement() -> Value {
    json!({
        "native_autosave": "may_differ_from_native_root",
        "resident_completion": "retire",
    })
}

pub fn project_neow_room(frame: &Value) -> Result<Value, String> {
    let screen = neow_screen(frame)?;
    let stage = match screen {
        3 => "choose_reward",
        99 => "leave",
        _ => unreachable!("validated Neow screen is closed"),
    };
    Ok(json!({"kind": "neow", "stage": stage}))
}

pub fn project_neow_action_space(frame: &Value) -> Result<Value, String> {
    let screen = neow_screen(frame)?;
    let candidates = frame["action_space"]["actions"]
        .as_array()
        .ok_or_else(|| "Java Neow frame omitted action_space.actions".to_owned())?
        .iter()
        .filter(|action| {
            action["kind"].as_str() == Some("choose_event_option")
                && action["event_id"].as_str() == Some(NEOW_EVENT_ID)
        })
        .collect::<Vec<_>>();
    let actions = match screen {
        3 => {
            if !matches!(candidates.len(), 2 | 4) {
                return Err(format!(
                    "Java Neow reward screen exposes {} typed actions instead of two or four",
                    candidates.len()
                ));
            }
            let mut identities = BTreeSet::new();
            candidates
                .into_iter()
                .map(|action| {
                    let reward = neow_reward_identity(required_str(action, "neow_reward_type")?)?;
                    let drawback = neow_drawback_identity(required_str(action, "neow_drawback")?)?;
                    if !identities.insert((reward, drawback)) {
                        return Err("Java Neow action space repeats a typed identity".to_owned());
                    }
                    Ok(JavaNeowActionProjectionV1::ChooseNeowOption { reward, drawback })
                })
                .collect::<Result<Vec<_>, String>>()?
        }
        99 => {
            let [action] = candidates.as_slice() else {
                return Err(format!(
                    "Java completed Neow screen exposes {} leave actions instead of one",
                    candidates.len()
                ));
            };
            if action
                .get("neow_reward_type")
                .is_some_and(|value| !value.is_null())
                || action
                    .get("neow_drawback")
                    .is_some_and(|value| !value.is_null())
            {
                return Err("Java Neow leave action carries a reward identity".to_owned());
            }
            vec![JavaNeowActionProjectionV1::LeaveNeow]
        }
        _ => unreachable!("validated Neow screen is closed"),
    };
    let mut projected = actions
        .into_iter()
        .map(|action| {
            serde_json::to_value(action)
                .map_err(|error| format!("failed to encode Java Neow action: {error}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    sort_json_values(&mut projected)?;
    Ok(Value::Array(projected))
}

pub fn neow_runtime_settlement() -> Value {
    json!({
        "native_autosave": "may_differ_from_native_root",
        "resident_completion": "retire",
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum JavaNeowActionProjectionV1 {
    ChooseNeowOption {
        reward: NeowRewardIdentityV1,
        drawback: NeowDrawbackIdentityV1,
    },
    LeaveNeow,
}

fn neow_screen(frame: &Value) -> Result<i64, String> {
    validate_natural_neow_boundary(frame)?;
    frame["public_base"]["decision"]["event_screen_index"]
        .as_i64()
        .ok_or_else(|| "Java Neow decision omitted its screen index".to_owned())
}

fn neow_reward_identity(value: &str) -> Result<NeowRewardIdentityV1, String> {
    match value {
        "THREE_ENEMY_KILL" => Ok(NeowRewardIdentityV1::ThreeEnemyKill),
        "TEN_PERCENT_HP_BONUS" => Ok(NeowRewardIdentityV1::TenPercentHpBonus),
        "THREE_CARDS" => Ok(NeowRewardIdentityV1::ThreeCards),
        "ONE_RANDOM_RARE_CARD" => Ok(NeowRewardIdentityV1::OneRandomRareCard),
        "REMOVE_CARD" => Ok(NeowRewardIdentityV1::RemoveCard),
        "UPGRADE_CARD" => Ok(NeowRewardIdentityV1::UpgradeCard),
        "TRANSFORM_CARD" => Ok(NeowRewardIdentityV1::TransformCard),
        "RANDOM_COLORLESS" => Ok(NeowRewardIdentityV1::RandomColorless),
        "THREE_SMALL_POTIONS" => Ok(NeowRewardIdentityV1::ThreeSmallPotions),
        "RANDOM_COMMON_RELIC" => Ok(NeowRewardIdentityV1::RandomCommonRelic),
        "HUNDRED_GOLD" => Ok(NeowRewardIdentityV1::HundredGold),
        "RANDOM_COLORLESS_2" => Ok(NeowRewardIdentityV1::RandomColorless2),
        "REMOVE_TWO" => Ok(NeowRewardIdentityV1::RemoveTwo),
        "ONE_RARE_RELIC" => Ok(NeowRewardIdentityV1::OneRareRelic),
        "THREE_RARE_CARDS" => Ok(NeowRewardIdentityV1::ThreeRareCards),
        "TWO_FIFTY_GOLD" => Ok(NeowRewardIdentityV1::TwoFiftyGold),
        "TRANSFORM_TWO_CARDS" => Ok(NeowRewardIdentityV1::TransformTwoCards),
        "TWENTY_PERCENT_HP_BONUS" => Ok(NeowRewardIdentityV1::TwentyPercentHpBonus),
        "BOSS_RELIC" => Ok(NeowRewardIdentityV1::BossRelic),
        other => Err(format!("Java Neow reward {other:?} is unsupported")),
    }
}

fn neow_drawback_identity(value: &str) -> Result<NeowDrawbackIdentityV1, String> {
    match value {
        "NONE" => Ok(NeowDrawbackIdentityV1::None),
        "TEN_PERCENT_HP_LOSS" => Ok(NeowDrawbackIdentityV1::TenPercentHpLoss),
        "NO_GOLD" => Ok(NeowDrawbackIdentityV1::NoGold),
        "CURSE" => Ok(NeowDrawbackIdentityV1::Curse),
        "PERCENT_DAMAGE" => Ok(NeowDrawbackIdentityV1::PercentDamage),
        other => Err(format!("Java Neow drawback {other:?} is unsupported")),
    }
}

pub fn rebuild_master_deck_leases(
    frame: &Value,
    bindings: &[CrossRuntimeCardBindingV1],
) -> Result<Vec<JavaCardLeaseV1>, String> {
    let cards = master_deck(frame)?;
    if cards.len() != bindings.len() {
        return Err("fresh Java master deck and target bindings have different sizes".to_owned());
    }
    let mut leases = Vec::with_capacity(bindings.len());
    let mut seen_ordinals = BTreeSet::new();
    let mut seen_uuids = BTreeSet::new();
    for binding in bindings {
        let CardBindingEvidenceV1::MasterDeckObservation {
            java_master_deck_ordinal,
            ..
        } = &binding.evidence
        else {
            return Err("natural noncombat recovery found a non-master-deck binding".to_owned());
        };
        let ordinal = usize::try_from(*java_master_deck_ordinal)
            .map_err(|_| "target Java master-deck ordinal exceeds usize".to_owned())?;
        if !seen_ordinals.insert(ordinal) {
            return Err("target bindings repeat a Java master-deck ordinal".to_owned());
        }
        let card = cards
            .get(ordinal)
            .ok_or_else(|| format!("fresh Java master deck omitted ordinal {ordinal}"))?;
        if required_u64(card, "ordinal")? != *java_master_deck_ordinal
            || required_str(card, "card_id")? != binding.card_id
            || required_u64(card, "upgrades")? != u64::from(binding.upgrades)
        {
            return Err(format!(
                "fresh Java master-deck source changed at ordinal {ordinal}"
            ));
        }
        let java_card_uuid = required_str(card, "card_uuid")?;
        if !seen_uuids.insert(java_card_uuid) {
            return Err("fresh Java master deck reused one runtime UUID".to_owned());
        }
        leases.push(JavaCardLeaseV1 {
            binding_id: binding.binding_id.clone(),
            java_card_uuid: java_card_uuid.to_owned(),
        });
    }
    if seen_ordinals.len() != cards.len() {
        return Err("target bindings do not cover every fresh Java master-deck card".to_owned());
    }
    leases.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    if leases
        .windows(2)
        .any(|pair| pair[0].binding_id >= pair[1].binding_id)
    {
        return Err("fresh Java card leases are not unique".to_owned());
    }
    Ok(leases)
}

pub fn project_master_deck_bindings(
    frame: &Value,
    leases: &[JavaCardLeaseV1],
) -> Result<Value, String> {
    let by_uuid = leases
        .iter()
        .map(|lease| (lease.java_card_uuid.as_str(), lease.binding_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    if by_uuid.len() != leases.len() {
        return Err("fresh Java card leases alias one runtime UUID".to_owned());
    }
    Ok(Value::Array(
        master_deck(frame)?
            .iter()
            .map(|card| {
                let java_uuid = required_str(card, "card_uuid")?;
                Ok(json!({
                    "binding_id": by_uuid.get(java_uuid).copied().ok_or_else(|| format!("unbound fresh Java master-deck UUID {java_uuid}"))?,
                    "ordinal": required_u64(card, "ordinal")?,
                    "card_id": required_str(card, "card_id")?,
                    "upgrades": required_u64(card, "upgrades")?,
                }))
            })
            .collect::<Result<Vec<_>, String>>()?,
    ))
}

pub fn java_runtime_epoch(frame: &Value) -> Result<String, String> {
    let producer = required_nonempty_str(frame, "producer_session_id")?;
    let connection = required_nonempty_str(frame, "connection_id")?;
    let capture = required_nonempty_str(frame, "capture_id")?;
    Ok(format!("{producer}:{connection}:{capture}"))
}

pub fn canonical_engine_seed_from_java_long(value: &str) -> Result<String, String> {
    if let Ok(signed) = value.parse::<i64>() {
        return Ok((signed as u64).to_string());
    }
    value
        .parse::<u64>()
        .map(|unsigned| unsigned.to_string())
        .map_err(|_| format!("Java run seed is not an integer: {value:?}"))
}

fn validate_observation(observation: &NativeRecoveryObservationBodyV1) -> Result<(), String> {
    observation.scope.validate()?;
    validate_java_versions(&observation.java_versions)?;
    if observation.java_runtime_epoch != java_runtime_epoch(&observation.frame)? {
        return Err("native recovery observation runtime identity changed".to_owned());
    }
    if observation.frame["kind"].as_str() != Some("state_frame")
        || observation.frame["profile"].as_str() != Some(RUN_CONTROL_PROFILE)
        || observation.frame["frame_id"].as_u64().is_none()
    {
        return Err("native recovery observation is not a run-control state frame".to_owned());
    }
    serde_json::from_value::<ProtocolDescriptor>(observation.frame["protocol"].clone())
        .map_err(|error| format!("native recovery observation protocol is invalid: {error}"))?
        .validate()?;
    Ok(())
}

fn validate_java_versions(versions: &[VersionIdentityV1]) -> Result<(), String> {
    const COMPONENTS: [&str; 3] = [
        "communicationmod_jar",
        "communicationmod_source",
        "slay_the_spire_jar",
    ];
    if versions.len() != COMPONENTS.len()
        || versions
            .iter()
            .map(|version| version.component.as_str())
            .ne(COMPONENTS)
        || versions.iter().any(|version| version.identity.is_empty())
    {
        return Err("native recovery observation Java version set is invalid".to_owned());
    }
    Ok(())
}

/// Verifies the scene-neutral fresh-runtime boundary captured by the physical
/// validator. It does not decide whether a particular PairedRoot profile can
/// project the frame.
pub fn validate_natural_run_decision_boundary(frame: &Value) -> Result<(), String> {
    if frame["kind"].as_str() != Some("state_frame")
        || frame["profile"].as_str() != Some(RUN_CONTROL_PROFILE)
        || !frame["scenario"].is_null()
        || frame["boundary"]["stable"].as_bool() != Some(true)
        || frame["boundary"]["kind"].as_str() != Some("run_decision")
    {
        return Err("fresh Java frame is not one stable natural run decision".to_owned());
    }
    java_runtime_epoch(frame)?;
    Ok(())
}

/// Verifies the narrow natural Map scene projected by native recovery.
pub fn validate_natural_map_boundary(frame: &Value) -> Result<(), String> {
    validate_natural_run_decision_boundary(frame)?;
    if frame["boundary"]["evidence"]["screen_type"].as_str() != Some("MAP")
        || frame["boundary"]["evidence"]["room_phase"].as_str() != Some("COMPLETE")
        || frame["public_base"]["decision"]["kind"].as_str() != Some("map")
    {
        return Err("fresh Java frame is not one stable Map decision".to_owned());
    }
    Ok(())
}

/// Verifies the narrow natural Neow scene projected by native recovery.
pub fn validate_natural_neow_boundary(frame: &Value) -> Result<(), String> {
    validate_natural_run_decision_boundary(frame)?;
    let decision = &frame["public_base"]["decision"];
    if frame["boundary"]["evidence"]["screen_type"].as_str() != Some("EVENT")
        || frame["boundary"]["evidence"]["room_phase"].as_str() != Some("EVENT")
        || decision["kind"].as_str() != Some("event")
        || decision["event_id"].as_str() != Some(NEOW_EVENT_ID)
        || !matches!(decision["event_screen_index"].as_i64(), Some(3 | 99))
    {
        return Err("fresh Java frame is not one stable Neow decision".to_owned());
    }
    Ok(())
}

fn master_deck(frame: &Value) -> Result<&Vec<Value>, String> {
    frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "Java frame omitted parity_internal.master_deck".to_owned())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PublicMapProjectionV1 {
    current_x: i32,
    current_y: i32,
    boss_id: Option<String>,
    boss_anchor: Option<PublicMapEdgeProjectionV1>,
    nodes: Vec<PublicMapNodeProjectionV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PublicMapNodeProjectionV1 {
    x: i32,
    y: i32,
    room_type: MapRoomIdentityV1,
    has_emerald_key: bool,
    edges: Vec<PublicMapEdgeProjectionV1>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
struct PublicMapEdgeProjectionV1 {
    destination_x: i32,
    destination_y: i32,
}

fn validate_and_order_public_map(map: &mut PublicMapProjectionV1) -> Result<(), String> {
    if (map.current_x == -1) != (map.current_y == -1) {
        return Err("Java public map has a partial pre-entry coordinate".to_owned());
    }
    if map.current_x < -1 || map.current_y < -1 {
        return Err("Java public map has a negative coordinate".to_owned());
    }
    let mut coordinates = BTreeSet::new();
    for node in &mut map.nodes {
        if node.x < 0 || node.y < 0 || !coordinates.insert((node.x, node.y)) {
            return Err("Java public map has an invalid or repeated node".to_owned());
        }
        node.edges.sort();
        if node.edges.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("Java public map repeats an edge".to_owned());
        }
    }
    let external = map
        .nodes
        .iter()
        .flat_map(|node| &node.edges)
        .filter(|edge| !coordinates.contains(&(edge.destination_x, edge.destination_y)))
        .cloned()
        .collect::<BTreeSet<_>>();
    map.boss_anchor = match external.len() {
        0 => None,
        1 if map.boss_id.is_some() => external.into_iter().next(),
        1 => return Err("Java public map has a boss anchor without a boss".to_owned()),
        count => return Err(format!("Java public map has {count} external destinations")),
    };
    let current_is_boss = map.boss_anchor.as_ref().is_some_and(|anchor| {
        (anchor.destination_x, anchor.destination_y) == (map.current_x, map.current_y)
    });
    if map.current_x >= 0
        && !coordinates.contains(&(map.current_x, map.current_y))
        && !current_is_boss
    {
        return Err("Java public map current coordinate is absent from topology".to_owned());
    }
    for node in &map.nodes {
        for edge in &node.edges {
            if !coordinates.contains(&(edge.destination_x, edge.destination_y))
                && map.boss_anchor.as_ref() != Some(edge)
            {
                return Err("Java public map edge targets an absent node".to_owned());
            }
        }
    }
    map.nodes.sort_by_key(|node| (node.y, node.x));
    Ok(())
}

pub fn map_room_identity_from_java(value: &str) -> Result<MapRoomIdentityV1, String> {
    match value.rsplit('.').next().unwrap_or(value) {
        "EventRoom" => Ok(MapRoomIdentityV1::QuestionMark),
        "MonsterRoom" => Ok(MapRoomIdentityV1::Monster),
        "MonsterRoomElite" => Ok(MapRoomIdentityV1::Elite),
        "MonsterRoomBoss" => Ok(MapRoomIdentityV1::Boss),
        "RestRoom" => Ok(MapRoomIdentityV1::Rest),
        "ShopRoom" => Ok(MapRoomIdentityV1::Shop),
        "TreasureRoom" => Ok(MapRoomIdentityV1::Treasure),
        "TrueVictoryRoom" => Ok(MapRoomIdentityV1::Victory),
        "NeowRoom" => Ok(MapRoomIdentityV1::Neow),
        other => Err(format!("Java map room type {other:?} is unsupported")),
    }
}

fn comparable_rng_counter(owner: &str, counter: i64) -> Value {
    if owner == "com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng" {
        Value::Null
    } else {
        json!(counter)
    }
}

fn to_i32(value: i64, label: &str) -> Result<i32, String> {
    i32::try_from(value).map_err(|_| format!("Java public-map {label} exceeds i32"))
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("required string field {key:?} is absent or empty"))
}

fn required_nonempty_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    required_str(value, key).and_then(|text| {
        (!text.is_empty())
            .then_some(text)
            .ok_or_else(|| format!("required string field {key:?} is empty"))
    })
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value[key]
        .as_u64()
        .ok_or_else(|| format!("required unsigned field {key:?} is absent"))
}

fn required_i64(value: &Value, key: &str) -> Result<i64, String> {
    value[key]
        .as_i64()
        .ok_or_else(|| format!("required signed field {key:?} is absent"))
}

fn required_bool(value: &Value, key: &str) -> Result<bool, String> {
    value[key]
        .as_bool()
        .ok_or_else(|| format!("required boolean field {key:?} is absent"))
}

fn required_str_object<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("required string field {key:?} is absent or empty"))
}

fn required_u64_object(object: &Map<String, Value>, key: &str) -> Result<u64, String> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("required unsigned field {key:?} is absent"))
}

fn required_i64_object(object: &Map<String, Value>, key: &str) -> Result<i64, String> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("required signed field {key:?} is absent"))
}

fn required_bool_object(object: &Map<String, Value>, key: &str) -> Result<bool, String> {
    object
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("required boolean field {key:?} is absent"))
}

fn required_string_array_object(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, String> {
    object
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("required array field {key:?} is absent"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("array field {key:?} contains a non-string"))
        })
        .collect()
}

fn sort_json_values(values: &mut [Value]) -> Result<(), String> {
    let mut keyed = values
        .iter()
        .cloned()
        .map(|value| canonical_json(&value).map(|key| (key, value)))
        .collect::<Result<Vec<_>, _>>()?;
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    for (target, (_, value)) in values.iter_mut().zip(keyed) {
        *target = value;
    }
    Ok(())
}

fn safe_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn canonical_json(value: &impl Serialize) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| error.to_string())
}

fn scoped_content_id(domain: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    format!("{domain}-{:x}", hasher.finalize())
}

fn require_scoped_content_id(value: &str, domain: &str) -> Result<(), String> {
    let Some(suffix) = value.strip_prefix(&format!("{domain}-")) else {
        return Err("native recovery observation ref has the wrong domain".to_owned());
    };
    if suffix.len() != 64 || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("native recovery observation ref is malformed".to_owned());
    }
    Ok(())
}

fn publish_immutable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut output) => output
            .write_all(bytes)
            .and_then(|_| output.sync_all())
            .map_err(|error| format!("failed to publish native recovery observation: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(path).map_err(|read_error| {
                format!("failed to verify existing native recovery observation: {read_error}")
            })?;
            if existing != bytes {
                return Err("native recovery observation content collision".to_owned());
            }
            Ok(())
        }
        Err(error) => Err(format!(
            "failed to create native recovery observation: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_paired_root::{BlobRefV1, RustCardLineageV1};
    use uuid::Uuid;

    fn blob(byte: char) -> BlobRefV1 {
        BlobRefV1 {
            sha256: byte.to_string().repeat(64),
            bytes: 1,
            media_type: "application/json".to_owned(),
        }
    }

    fn binding(ordinal: u64, id: &str, upgrades: u8) -> CrossRuntimeCardBindingV1 {
        CrossRuntimeCardBindingV1 {
            binding_id: format!("paired-card-binding-v1-{}", "a".repeat(64)),
            rust_lineage: RustCardLineageV1::MasterDeck {
                acquisition_ordinal: ordinal,
            },
            card_id: id.to_owned(),
            upgrades,
            evidence: CardBindingEvidenceV1::MasterDeckObservation {
                java_master_deck_ordinal: ordinal,
                rust_master_deck_ordinal: ordinal,
                java_projection: blob('b'),
                rust_projection: blob('c'),
            },
        }
    }

    fn map_frame() -> Value {
        json!({
            "kind": "state_frame",
            "profile": RUN_CONTROL_PROFILE,
            "protocol": {
                "name": "communicationmod.parity",
                "major": 1,
                "minor": sts_java_parity_protocol::PROTOCOL_MINOR,
                "schema_id": "communicationmod.parity-frame.v1"
            },
            "producer_session_id": "producer-1",
            "connection_id": "connection-1",
            "capture_id": "capture-1",
            "frame_id": 7,
            "boundary": {
                "kind": "run_decision",
                "stable": true,
                "evidence": {"screen_type": "MAP", "room_phase": "COMPLETE"}
            },
            "public_base": {
                "run": {
                    "ascension": 20,
                    "character": "IRONCLAD",
                    "act": 1,
                    "floor": 0,
                    "current_hp": 80,
                    "max_hp": 80,
                    "gold": 99,
                    "final_act_available": true,
                    "has_ruby_key": false,
                    "has_sapphire_key": false,
                    "has_emerald_key": false
                },
                "decision": {"kind": "map"},
                "map": {
                    "current_x": -1,
                    "current_y": -1,
                    "boss": "TheGuardian",
                    "nodes": [{
                        "x": 0,
                        "y": 0,
                        "room_type": "com.megacrit.cardcrawl.rooms.MonsterRoom",
                        "has_emerald_key": false,
                        "edges": [{"destination_x": 3, "destination_y": 16}]
                    }]
                }
            },
            "parity_internal": {
                "run_seed": "-1",
                "content_profile": "all_unlocked",
                "master_deck": [{
                    "ordinal": 0,
                    "card_uuid": "java-card-1",
                    "card_id": "Strike_R",
                    "upgrades": 0
                }],
                "relics": [{"index": 0, "relic_id": "Burning Blood", "counter": -1}],
                "potions": [{"slot_index": 0, "empty": true}],
                "card_pools": {
                    "common": ["Anger"],
                    "uncommon": ["Battle Trance"],
                    "rare": ["Feed"],
                    "colorless_combat": ["Finesse"]
                }
            },
            "rng_internal": {
                "streams": [{
                    "owner": "com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng",
                    "scope": "run",
                    "state_hex": ["00", "01"],
                    "counter": 9
                }],
                "run_streams": [{
                    "owner": "com.megacrit.cardcrawl.neow.NeowEvent.rng",
                    "scope": "run",
                    "state_hex": ["02", "03"],
                    "counter": 1
                }]
            },
            "action_space": {"actions": [{
                "action_id": "map_node:0:0",
                "kind": "choose_map_node",
                "map_x": 0,
                "map_y": 0,
                "map_room_type": "com.megacrit.cardcrawl.rooms.MonsterRoom",
                "parameter_schema": null
            }]}
        })
    }

    fn completed_neow_frame() -> Value {
        let mut frame = map_frame();
        frame["boundary"]["evidence"] = json!({
            "screen_type": "EVENT",
            "room_phase": "EVENT",
        });
        frame["public_base"]["decision"] = json!({
            "kind": "event",
            "event_id": NEOW_EVENT_ID,
            "event_screen_index": 99,
        });
        frame["action_space"]["actions"] = json!([{
            "action_id": "event_option:Neow:index:99:0",
            "kind": "choose_event_option",
            "event_id": NEOW_EVENT_ID,
            "event_screen_index": 99,
            "option_index": 0,
        }]);
        frame["rng_internal"]["streams"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "owner": JAVA_MISC_RNG_ID,
                "scope": "floor",
                "state_hex": ["04", "05"],
                "counter": 2,
            }));
        frame
    }

    #[test]
    fn observation_store_binds_exact_ref_and_job_scope() {
        let root = std::env::temp_dir().join(format!(
            "sts-java-projection-observation-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let store = NativeRecoveryObservationStoreV1::new(&root).unwrap();
        let scope = NativeRecoveryObservationScopeV1::new(7, "job-1").unwrap();
        let artifact = store
            .publish(scope.clone(), java_versions(), map_frame())
            .unwrap();
        assert!(artifact
            .observation_ref
            .starts_with("workbench-native-recovery-observation-v1-"));
        assert_eq!(
            store
                .load(&artifact.observation_ref, &scope)
                .unwrap()
                .observation
                .java_runtime_epoch,
            "producer-1:connection-1:capture-1"
        );
        let other = NativeRecoveryObservationScopeV1::new(7, "job-2").unwrap();
        assert!(store.load(&artifact.observation_ref, &other).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    fn java_versions() -> Vec<VersionIdentityV1> {
        vec![
            VersionIdentityV1 {
                component: "communicationmod_jar".to_owned(),
                identity: "mod-jar".to_owned(),
            },
            VersionIdentityV1 {
                component: "communicationmod_source".to_owned(),
                identity: "mod-source".to_owned(),
            },
            VersionIdentityV1 {
                component: "slay_the_spire_jar".to_owned(),
                identity: "game-jar".to_owned(),
            },
        ]
    }

    #[test]
    fn natural_map_projection_is_complete_and_rebinds_fresh_uuid() {
        let projection =
            project_natural_map_root(&map_frame(), &[binding(0, "Strike_R", 0)]).unwrap();
        assert_eq!(projection.components.len(), 10);
        assert_eq!(projection.card_leases.len(), 1);
        assert_eq!(projection.card_leases[0].java_card_uuid, "java-card-1");
        assert_eq!(
            projection
                .component(ProjectionDomainV1::RunScalars)
                .unwrap()
                .value["seed"],
            u64::MAX.to_string()
        );
        assert_eq!(
            projection
                .component(ProjectionDomainV1::RngStreams)
                .unwrap()
                .value[0]["counter"],
            Value::Null
        );
        assert_eq!(
            projection
                .component(ProjectionDomainV1::ActionSpace)
                .unwrap()
                .value[0],
            json!({"kind":"choose_map_node","x":0,"y":0,"room_type":"monster"})
        );
    }

    #[test]
    fn map_projection_rejects_stale_binding_semantics_before_comparison() {
        let error =
            project_natural_map_root(&map_frame(), &[binding(0, "Defend_R", 0)]).unwrap_err();
        assert!(error.contains("source changed"));
    }

    #[test]
    fn completed_neow_projection_reuses_the_shared_noncombat_owner() {
        let projection =
            project_natural_neow_root(&completed_neow_frame(), &[binding(0, "Strike_R", 0)])
                .unwrap();

        assert_eq!(projection.components.len(), 11);
        assert_eq!(
            projection
                .component(ProjectionDomainV1::Room)
                .unwrap()
                .value,
            json!({"kind":"neow","stage":"leave"})
        );
        assert_eq!(
            projection
                .component(ProjectionDomainV1::ActionSpace)
                .unwrap()
                .value,
            json!([{"kind":"leave_neow"}])
        );
        let rng = &projection
            .component(ProjectionDomainV1::RngStreams)
            .unwrap()
            .value;
        assert!(rng
            .as_array()
            .unwrap()
            .iter()
            .all(|stream| stream["owner"] != JAVA_MISC_RNG_ID));
        assert!(rng
            .as_array()
            .unwrap()
            .iter()
            .all(|stream| stream["owner"] != JAVA_NEOW_RNG_ID));
    }

    #[test]
    fn reward_choice_neow_projection_retains_the_live_event_rng() {
        let mut frame = completed_neow_frame();
        frame["public_base"]["decision"]["event_screen_index"] = json!(3);

        let rng = project_neow_rng_streams(&frame).unwrap();
        assert!(rng
            .as_array()
            .unwrap()
            .iter()
            .any(|stream| stream["owner"] == JAVA_NEOW_RNG_ID));
    }
}
