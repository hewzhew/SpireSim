use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};

use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

mod resident_state;

pub use resident_state::resolve_owned_resident_workspace;

pub const ORACLE_ANALYSIS_SERVICE_PROTOCOL: &str = "oracle-analysis-jsonl";
pub const ORACLE_ANALYSIS_SERVICE_PROTOCOL_VERSION: u32 = 1;
pub const ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA: &str = "OracleAnalysisServiceEndpoint";
pub const ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct OracleAnalysisServiceRequestV1 {
    pub id: Option<Value>,
    pub auth_token: Option<String>,
    pub command: OracleAnalysisServiceCommandV1,
}

impl<'de> Deserialize<'de> for OracleAnalysisServiceRequestV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut object = serde_json::Map::<String, Value>::deserialize(deserializer)?;
        let id = object.remove("id");
        let auth_token = object
            .remove("auth_token")
            .map(serde_json::from_value::<String>)
            .transpose()
            .map_err(D::Error::custom)?;
        let command = OracleAnalysisServiceCommandV1::deserialize(Value::Object(object))
            .map_err(D::Error::custom)?;
        Ok(Self {
            id,
            auth_token,
            command,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisServiceEndpointV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub address: SocketAddr,
    pub auth_token: String,
    pub workspace: PathBuf,
    pub process_id: u32,
    /// Immutable runtime image used by this resident process. Older endpoint
    /// files omit it and remain readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnalysisCombatLabBaselineV1 {
    Root,
    #[default]
    ResidentIncumbent,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleAnalysisCombatLabLineV1 {
    Baseline,
    #[default]
    Current,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum OracleAnalysisServiceCommandV1 {
    Ping,
    Capabilities,
    Status {
        #[serde(default)]
        node: Option<usize>,
    },
    Explain {
        node: usize,
        owner_rank: u64,
    },
    RoutePolicyAudit {
        node: usize,
    },
    ShopPolicyAudit {
        node: usize,
    },
    CardRewardPolicyAudit {
        node: usize,
    },
    CardRewardPathAudit {
        node: usize,
    },
    CampfirePolicyAudit {
        node: usize,
    },
    View {
        #[serde(default)]
        node: Option<usize>,
    },
    Tree,
    Try {
        choice_ref: String,
    },
    Choose {
        node: usize,
        owner_rank: u64,
    },
    Owner {
        steps: u8,
    },
    Run {
        #[serde(default = "default_hallway_wall_ms")]
        hallway_wall_ms: u64,
        #[serde(default = "default_elite_wall_ms")]
        elite_wall_ms: u64,
        #[serde(default = "default_boss_wall_ms")]
        boss_wall_ms: u64,
        #[serde(default = "default_run_max_quanta")]
        max_quanta: usize,
        #[serde(default = "default_run_quantum_generation_work")]
        quantum_generation_work: usize,
        #[serde(default = "default_run_quantum_ms")]
        quantum_ms: u64,
        #[serde(default = "default_run_max_boundaries")]
        max_boundaries: usize,
        #[serde(default)]
        run_wall_ms: Option<u64>,
        #[serde(default)]
        export_continuation: Option<PathBuf>,
    },
    ChoosePath {
        node: usize,
        candidate_ids: Vec<String>,
    },
    Focus {
        node: usize,
    },
    Follow {
        edge: u64,
    },
    Back,
    Promote,
    Advance {
        #[serde(default = "default_max_quanta")]
        max_quanta: usize,
        #[serde(default = "default_quantum_generation_work")]
        quantum_generation_work: usize,
        #[serde(default = "default_quantum_ms")]
        quantum_ms: u64,
        #[serde(default)]
        wall_ms: Option<u64>,
        /// Keep the verified incumbent resident and spend the requested
        /// allowance until the configured strategic quality is reached.
        #[serde(default)]
        improve_incumbent: bool,
    },
    /// Spend one bounded diagnostic grant in the current exact combat stage
    /// without promotion or materialization.
    ProbeCombat {
        #[serde(default = "default_probe_generation_work")]
        generation_work: usize,
        #[serde(default = "default_probe_quantum_generation_work")]
        quantum_generation_work: usize,
        #[serde(default = "default_probe_wall_ms")]
        wall_ms: u64,
    },
    AcceptCombat,
    EscapeCombat,
    RestartCombat,
    CombatLabOpen {
        #[serde(default)]
        node: Option<usize>,
        #[serde(default)]
        baseline: OracleAnalysisCombatLabBaselineV1,
        #[serde(default = "default_max_engine_steps_per_transition")]
        max_engine_steps_per_transition: usize,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatLabObserve {
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatLabGoto {
        #[serde(default)]
        line: OracleAnalysisCombatLabLineV1,
        turn: u32,
        #[serde(default)]
        before_action: usize,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatLabBack {
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatLabKeep,
    CombatLabRestore {
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatLabPlayCard {
        card_id: String,
        #[serde(default)]
        occurrence: Option<usize>,
        #[serde(default)]
        target_index: Option<usize>,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatLabUsePotion {
        potion_id: String,
        #[serde(default)]
        occurrence: Option<usize>,
        #[serde(default)]
        target_index: Option<usize>,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatLabSelect {
        indices: Vec<usize>,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatLabEnd {
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatLabSearch {
        #[serde(default = "default_combat_scratch_max_quanta")]
        max_quanta: usize,
        #[serde(default = "default_combat_scratch_quantum_generation_work")]
        quantum_generation_work: usize,
        #[serde(default = "default_combat_scratch_quantum_ms")]
        quantum_ms: u64,
        #[serde(default = "default_combat_scratch_wall_ms")]
        wall_ms: u64,
    },
    CombatLabCompare,
    CombatLabCommit,
    CombatLabClear,
    CombatScratchStart {
        #[serde(default)]
        node: Option<usize>,
        #[serde(default = "default_max_engine_steps_per_transition")]
        max_engine_steps_per_transition: usize,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchStatus {
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchObserve {
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchPlay {
        action_ref: String,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchAtomic {
        scratch_node: u64,
        action_index: usize,
        #[serde(default)]
        full_observation: bool,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchCard {
        scratch_node: u64,
        card_lineage: String,
        #[serde(default)]
        target: Option<usize>,
        #[serde(default)]
        full_observation: bool,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchHandCard {
        scratch_node: u64,
        hand_index: usize,
        #[serde(default)]
        target_index: Option<usize>,
        #[serde(default)]
        full_observation: bool,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchPotion {
        scratch_node: u64,
        potion_uuid: u32,
        #[serde(default)]
        target: Option<usize>,
        #[serde(default)]
        full_observation: bool,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchPotionSlot {
        scratch_node: u64,
        potion_slot: usize,
        #[serde(default)]
        target_index: Option<usize>,
        #[serde(default)]
        full_observation: bool,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchEnd {
        #[serde(default)]
        scratch_node: Option<u64>,
        #[serde(default)]
        full_observation: bool,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchSelection {
        scratch_node: u64,
        family_index: usize,
        input_index: usize,
        #[serde(default)]
        full_observation: bool,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchBack {
        #[serde(default)]
        full_observation: bool,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchFocus {
        scratch_node: u64,
        #[serde(default)]
        full_observation: bool,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchSearch {
        #[serde(default = "default_combat_scratch_max_quanta")]
        max_quanta: usize,
        #[serde(default = "default_combat_scratch_quantum_generation_work")]
        quantum_generation_work: usize,
        #[serde(default = "default_combat_scratch_quantum_ms")]
        quantum_ms: u64,
        #[serde(default = "default_combat_scratch_wall_ms")]
        wall_ms: u64,
        #[serde(default)]
        selection_offset: usize,
        #[serde(default = "default_combat_scratch_selection_limit")]
        selection_limit: usize,
    },
    CombatScratchTree,
    CombatScratchCommit,
    CombatScratchClear,
    History {
        #[serde(default)]
        node: Option<usize>,
    },
    Journal {
        node: usize,
        #[serde(default = "default_journal_tail")]
        tail: usize,
    },
    Timeline {
        node: usize,
        #[serde(default = "default_journal_tail")]
        tail: usize,
    },
    JournalEntry {
        node: usize,
        index: usize,
    },
    Trajectory {
        node: usize,
    },
    CombatSummary {
        node: usize,
    },
    CombatDiagnostic {
        node: usize,
        #[serde(default = "default_max_engine_steps_per_transition")]
        max_engine_steps_per_transition: usize,
    },
    ExportCombatCase {
        node: usize,
        path: PathBuf,
    },
    ExportContinuation {
        node: usize,
        path: PathBuf,
    },
    VerifyRunWitness {
        #[serde(default)]
        node: Option<usize>,
    },
    Save,
    Shutdown,
}

fn default_max_engine_steps_per_transition() -> usize {
    512
}

fn default_combat_scratch_selection_limit() -> usize {
    24
}

const fn default_combat_scratch_max_quanta() -> usize {
    4
}

const fn default_combat_scratch_quantum_generation_work() -> usize {
    1_024
}

const fn default_combat_scratch_quantum_ms() -> u64 {
    100
}

const fn default_combat_scratch_wall_ms() -> u64 {
    1_000
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisServiceTimingV1 {
    pub execute_ms: u64,
    pub autosave_ms: u64,
    pub total_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleAnalysisServiceResponseV1 {
    pub protocol: String,
    pub protocol_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub event: String,
    pub ok: bool,
    pub revision: u64,
    pub saved_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<OracleAnalysisServiceTimingV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn call_oracle_analysis_tcp_v1(
    endpoint_path: &Path,
    request_json: &str,
) -> Result<OracleAnalysisServiceResponseV1, String> {
    let bytes = fs::read(endpoint_path).map_err(|error| {
        format!(
            "failed to read oracle service endpoint '{}': {error}",
            endpoint_path.display()
        )
    })?;
    let endpoint =
        serde_json::from_slice::<OracleAnalysisServiceEndpointV1>(&bytes).map_err(|error| {
            format!(
                "failed to parse oracle service endpoint '{}': {error}",
                endpoint_path.display()
            )
        })?;
    validate_endpoint(&endpoint)?;
    let mut request = serde_json::from_str::<Value>(request_json)
        .map_err(|error| format!("invalid oracle service request JSON: {error}"))?;
    let object = request
        .as_object_mut()
        .ok_or_else(|| "oracle service request must be a JSON object".to_string())?;
    object.insert("auth_token".to_string(), json!(endpoint.auth_token));

    let mut stream = TcpStream::connect(endpoint.address).map_err(|error| {
        format!(
            "failed to connect to oracle service at {}: {error}",
            endpoint.address
        )
    })?;
    serde_json::to_writer(&mut stream, &request)
        .map_err(|error| format!("failed to serialize oracle service request: {error}"))?;
    stream
        .write_all(b"\n")
        .map_err(|error| format!("failed to write oracle service request: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("failed to flush oracle service request: {error}"))?;
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .map_err(|error| format!("failed to read oracle service response: {error}"))?;
    serde_json::from_str::<OracleAnalysisServiceResponseV1>(&response)
        .map_err(|error| format!("failed to parse oracle service response: {error}"))
}

pub fn validate_endpoint(endpoint: &OracleAnalysisServiceEndpointV1) -> Result<(), String> {
    if endpoint.schema_name != ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA
        || endpoint.schema_version != ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA_VERSION
    {
        return Err("unsupported oracle analysis service endpoint schema".to_string());
    }
    if !endpoint.address.ip().is_loopback() {
        return Err(format!(
            "oracle analysis endpoint is not loopback-only: {}",
            endpoint.address
        ));
    }
    Ok(())
}

const fn default_max_quanta() -> usize {
    1
}
const fn default_quantum_generation_work() -> usize {
    50_000
}
const fn default_quantum_ms() -> u64 {
    1_000
}
const fn default_probe_generation_work() -> usize {
    4_096
}
const fn default_probe_quantum_generation_work() -> usize {
    256
}
const fn default_probe_wall_ms() -> u64 {
    1_000
}
const fn default_journal_tail() -> usize {
    32
}
const fn default_hallway_wall_ms() -> u64 {
    5_000
}
const fn default_elite_wall_ms() -> u64 {
    15_000
}
const fn default_boss_wall_ms() -> u64 {
    30_000
}
const fn default_run_max_quanta() -> usize {
    100_000
}
const fn default_run_quantum_generation_work() -> usize {
    4_096
}
const fn default_run_quantum_ms() -> u64 {
    100
}
const fn default_run_max_boundaries() -> usize {
    256
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loopback_call_injects_endpoint_auth_without_exposing_it_to_the_caller() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback test server");
        let address = listener.local_addr().expect("inspect test address");
        let endpoint_path = std::env::temp_dir().join(format!(
            "oracle-lab-protocol-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let endpoint = OracleAnalysisServiceEndpointV1 {
            schema_name: ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA.to_string(),
            schema_version: ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA_VERSION,
            address,
            auth_token: "secret-token".to_string(),
            workspace: PathBuf::from("workspace.json"),
            process_id: std::process::id(),
            executable: None,
        };
        fs::write(
            &endpoint_path,
            serde_json::to_vec(&endpoint).expect("serialize endpoint"),
        )
        .expect("write endpoint");

        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept client");
            let mut connection = BufReader::new(stream);
            let mut request = String::new();
            connection.read_line(&mut request).expect("read request");
            let request: Value = serde_json::from_str(&request).expect("parse request");
            assert_eq!(request.get("command"), Some(&json!("ping")));
            assert_eq!(request.get("auth_token"), Some(&json!("secret-token")));
            let response = OracleAnalysisServiceResponseV1 {
                protocol: ORACLE_ANALYSIS_SERVICE_PROTOCOL.to_string(),
                protocol_version: ORACLE_ANALYSIS_SERVICE_PROTOCOL_VERSION,
                id: None,
                event: "pong".to_string(),
                ok: true,
                revision: 0,
                saved_revision: 0,
                timing: Some(OracleAnalysisServiceTimingV1 {
                    execute_ms: 2,
                    autosave_ms: 3,
                    total_ms: 5,
                }),
                result: Some(json!({"alive": true})),
                error: None,
            };
            serde_json::to_writer(connection.get_mut(), &response).expect("write response");
            connection
                .get_mut()
                .write_all(b"\n")
                .expect("finish response");
        });

        let response = call_oracle_analysis_tcp_v1(&endpoint_path, r#"{"command":"ping"}"#)
            .expect("call loopback service");
        assert!(response.ok);
        assert_eq!(response.result, Some(json!({"alive": true})));
        let timing = response.timing.expect("typed service timing");
        assert_eq!(timing.execute_ms, 2);
        assert_eq!(timing.autosave_ms, 3);
        assert_eq!(timing.total_ms, 5);
        server.join().expect("join test server");
        let _ = fs::remove_file(endpoint_path);
    }

    #[test]
    fn endpoint_validation_rejects_non_loopback_addresses() {
        let endpoint = OracleAnalysisServiceEndpointV1 {
            schema_name: ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA.to_string(),
            schema_version: ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA_VERSION,
            address: "192.0.2.1:1234".parse().expect("static address"),
            auth_token: "unused".to_string(),
            workspace: PathBuf::new(),
            process_id: 0,
            executable: None,
        };
        assert!(validate_endpoint(&endpoint).is_err());
    }

    #[test]
    fn legacy_endpoint_without_runtime_image_remains_readable() {
        let endpoint = serde_json::from_value::<OracleAnalysisServiceEndpointV1>(json!({
            "schema_name": ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA,
            "schema_version": ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA_VERSION,
            "address": "127.0.0.1:1234",
            "auth_token": "legacy",
            "workspace": "workspace.json",
            "process_id": 7
        }))
        .expect("deserialize legacy endpoint");
        assert_eq!(endpoint.executable, None);
    }

    #[test]
    fn combat_diagnostic_uses_a_bounded_transition_default() {
        let command = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_diagnostic",
            "node": 17,
        }))
        .expect("parse diagnostic command");
        assert!(matches!(
            command,
            OracleAnalysisServiceCommandV1::CombatDiagnostic {
                node: 17,
                max_engine_steps_per_transition: 512,
            }
        ));
    }

    #[test]
    fn combat_scratch_status_uses_a_bounded_lazy_selection_page() {
        let command = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_status",
        }))
        .expect("parse combat scratch status command");
        assert!(matches!(
            command,
            OracleAnalysisServiceCommandV1::CombatScratchStatus {
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let observe = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_observe",
        }))
        .expect("parse compact combat scratch observation");
        assert!(matches!(
            observe,
            OracleAnalysisServiceCommandV1::CombatScratchObserve {
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let atomic = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_atomic",
            "scratch_node": 7,
            "action_index": 3,
        }))
        .expect("parse short combat scratch selector");
        assert!(matches!(
            atomic,
            OracleAnalysisServiceCommandV1::CombatScratchAtomic {
                scratch_node: 7,
                action_index: 3,
                full_observation: false,
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let card = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_card",
            "scratch_node": 7,
            "card_lineage": "constructed_scenario:10006",
        }))
        .expect("parse identity-bound combat scratch card");
        assert!(matches!(
            card,
            OracleAnalysisServiceCommandV1::CombatScratchCard {
                scratch_node: 7,
                card_lineage,
                target: None,
                full_observation: false,
                selection_offset: 0,
                selection_limit: 24,
            } if card_lineage == "constructed_scenario:10006"
        ));

        let hand_card = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_hand_card",
            "scratch_node": 7,
            "hand_index": 2,
            "target_index": 0,
        }))
        .expect("parse node-local combat scratch card");
        assert!(matches!(
            hand_card,
            OracleAnalysisServiceCommandV1::CombatScratchHandCard {
                scratch_node: 7,
                hand_index: 2,
                target_index: Some(0),
                full_observation: false,
                selection_offset: 0,
                selection_limit: 24,
            }
        ));
        assert!(
            serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
                "command": "combat_scratch_hand_card",
                "hand_index": 2,
            }))
            .is_err()
        );

        let potion = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_potion_slot",
            "scratch_node": 9,
            "potion_slot": 1,
            "target_index": 0,
        }))
        .expect("parse node-local combat scratch potion");
        assert!(matches!(
            potion,
            OracleAnalysisServiceCommandV1::CombatScratchPotionSlot {
                scratch_node: 9,
                potion_slot: 1,
                target_index: Some(0),
                full_observation: false,
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let end = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_end",
        }))
        .expect("parse cursor-local combat scratch end turn");
        assert!(matches!(
            end,
            OracleAnalysisServiceCommandV1::CombatScratchEnd {
                scratch_node: None,
                full_observation: false,
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let back = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_back",
        }))
        .expect("parse compact combat scratch back navigation");
        assert!(matches!(
            back,
            OracleAnalysisServiceCommandV1::CombatScratchBack {
                full_observation: false,
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let focus = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_focus",
            "scratch_node": 3,
        }))
        .expect("parse compact combat scratch focus navigation");
        assert!(matches!(
            focus,
            OracleAnalysisServiceCommandV1::CombatScratchFocus {
                scratch_node: 3,
                full_observation: false,
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let search = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_scratch_search",
        }))
        .expect("parse combat scratch search command");
        assert!(matches!(
            search,
            OracleAnalysisServiceCommandV1::CombatScratchSearch {
                max_quanta: 4,
                quantum_generation_work: 1_024,
                quantum_ms: 100,
                wall_ms: 1_000,
                selection_offset: 0,
                selection_limit: 24,
            }
        ));
    }

    #[test]
    fn combat_line_lab_defaults_to_the_resident_incumbent_and_hides_node_selection() {
        let open = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_lab_open",
            "node": 36,
        }))
        .expect("parse combat line lab open");
        assert!(matches!(
            open,
            OracleAnalysisServiceCommandV1::CombatLabOpen {
                node: Some(36),
                baseline: OracleAnalysisCombatLabBaselineV1::ResidentIncumbent,
                max_engine_steps_per_transition: 512,
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let goto = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_lab_goto",
            "turn": 3,
        }))
        .expect("parse current-line combat lab navigation");
        assert!(matches!(
            goto,
            OracleAnalysisServiceCommandV1::CombatLabGoto {
                line: OracleAnalysisCombatLabLineV1::Current,
                turn: 3,
                before_action: 0,
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let back = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_lab_back",
        }))
        .expect("parse combat lab back");
        assert!(matches!(
            back,
            OracleAnalysisServiceCommandV1::CombatLabBack {
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let keep = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_lab_keep",
        }))
        .expect("parse combat lab keep");
        assert!(matches!(
            keep,
            OracleAnalysisServiceCommandV1::CombatLabKeep
        ));

        let restore = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_lab_restore",
        }))
        .expect("parse combat lab restore");
        assert!(matches!(
            restore,
            OracleAnalysisServiceCommandV1::CombatLabRestore {
                selection_offset: 0,
                selection_limit: 24,
            }
        ));

        let play = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_lab_play_card",
            "card_id": "PowerThrough",
        }))
        .expect("parse semantic combat line lab card");
        assert!(matches!(
            play,
            OracleAnalysisServiceCommandV1::CombatLabPlayCard {
                card_id,
                occurrence: None,
                target_index: None,
                selection_offset: 0,
                selection_limit: 24,
            } if card_id == "PowerThrough"
        ));

        let potion = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_lab_use_potion",
            "potion_id": "FearPotion",
        }))
        .expect("parse semantic combat line lab potion");
        assert!(matches!(
            potion,
            OracleAnalysisServiceCommandV1::CombatLabUsePotion {
                potion_id,
                occurrence: None,
                target_index: None,
                selection_offset: 0,
                selection_limit: 24,
            } if potion_id == "FearPotion"
        ));

        let selection = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "combat_lab_select",
            "indices": [0, 2, 3],
        }))
        .expect("parse observation-local combat line lab selection");
        assert!(matches!(
            selection,
            OracleAnalysisServiceCommandV1::CombatLabSelect {
                indices,
                selection_offset: 0,
                selection_limit: 24,
            } if indices == [0, 2, 3]
        ));
    }

    #[test]
    fn route_policy_audit_requires_an_explicit_node() {
        let command = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "route_policy_audit",
            "node": 185,
        }))
        .expect("parse route policy audit command");
        assert!(matches!(
            command,
            OracleAnalysisServiceCommandV1::RoutePolicyAudit { node: 185 }
        ));
    }

    #[test]
    fn card_reward_policy_audit_requires_an_explicit_node() {
        let command = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "card_reward_policy_audit",
            "node": 53,
        }))
        .expect("parse card reward policy audit command");
        assert!(matches!(
            command,
            OracleAnalysisServiceCommandV1::CardRewardPolicyAudit { node: 53 }
        ));
    }

    #[test]
    fn card_reward_path_audit_requires_an_explicit_target_node() {
        let command = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "card_reward_path_audit",
            "node": 72,
        }))
        .expect("parse card reward path audit command");
        assert!(matches!(
            command,
            OracleAnalysisServiceCommandV1::CardRewardPathAudit { node: 72 }
        ));
    }

    #[test]
    fn advance_quality_mode_is_explicit_and_backward_compatible() {
        let defaulted = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "advance",
        }))
        .expect("parse legacy advance command");
        assert!(matches!(
            defaulted,
            OracleAnalysisServiceCommandV1::Advance {
                improve_incumbent: false,
                ..
            }
        ));

        let improving = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "advance",
            "improve_incumbent": true,
        }))
        .expect("parse quality advance command");
        assert!(matches!(
            improving,
            OracleAnalysisServiceCommandV1::Advance {
                improve_incumbent: true,
                ..
            }
        ));
    }

    #[test]
    fn current_stage_probe_has_its_own_typed_budget_contract() {
        let probe = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "probe_combat",
        }))
        .expect("parse default current-stage probe command");
        assert!(matches!(
            probe,
            OracleAnalysisServiceCommandV1::ProbeCombat {
                generation_work: 4_096,
                quantum_generation_work: 256,
                wall_ms: 1_000,
            }
        ));

        let explicit = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "probe_combat",
            "generation_work": 768,
            "quantum_generation_work": 128,
            "wall_ms": 250,
        }))
        .expect("parse explicit current-stage probe command");
        assert!(matches!(
            explicit,
            OracleAnalysisServiceCommandV1::ProbeCombat {
                generation_work: 768,
                quantum_generation_work: 128,
                wall_ms: 250,
            }
        ));
        assert!(
            serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
                "command": "advance",
                "probe_current_stage": true,
            }))
            .is_err()
        );
    }

    #[test]
    fn witness_verification_defaults_to_the_current_node() {
        let command = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "verify_run_witness",
        }))
        .expect("parse witness verification command");
        assert!(matches!(
            command,
            OracleAnalysisServiceCommandV1::VerifyRunWitness { node: None }
        ));
    }

    #[test]
    fn owner_batch_requires_an_explicit_bounded_step_count() {
        let command = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "owner",
            "steps": 64,
        }))
        .expect("parse owner batch command");
        assert!(matches!(
            command,
            OracleAnalysisServiceCommandV1::Owner { steps: 64 }
        ));
        assert!(
            serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
                "command": "owner",
            }))
            .is_err()
        );
    }

    #[test]
    fn autonomous_run_has_stable_service_side_defaults() {
        let command = serde_json::from_value::<OracleAnalysisServiceCommandV1>(json!({
            "command": "run",
        }))
        .expect("parse autonomous run command");
        assert!(matches!(
            command,
            OracleAnalysisServiceCommandV1::Run {
                hallway_wall_ms: 5_000,
                elite_wall_ms: 15_000,
                boss_wall_ms: 30_000,
                max_quanta: 100_000,
                quantum_generation_work: 4_096,
                quantum_ms: 100,
                max_boundaries: 256,
                run_wall_ms: None,
                export_continuation: None,
            }
        ));
    }
}
