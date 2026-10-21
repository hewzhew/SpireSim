use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use blake2::{Blake2s256, Digest};
use serde::Serialize;
use serde_json::{json, Value};

pub use oracle_lab_protocol::resolve_owned_resident_workspace;
pub use oracle_lab_protocol::{
    call_oracle_analysis_tcp_v1, OracleAnalysisCombatLabBaselineV1, OracleAnalysisCombatLabLineV1,
    OracleAnalysisServiceCommandV1, OracleAnalysisServiceEndpointV1,
    OracleAnalysisServiceRequestV1, OracleAnalysisServiceResponseV1, OracleAnalysisServiceTimingV1,
    ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA, ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA_VERSION,
    ORACLE_ANALYSIS_SERVICE_PROTOCOL, ORACLE_ANALYSIS_SERVICE_PROTOCOL_VERSION,
};

use super::oracle_analysis_session::{
    self as analysis_session, OracleAnalysisAdvanceRequestV1, OracleAnalysisCombatProbeRequestV1,
    OracleAnalysisNodeViewV1,
};
use crate::eval::combat_lab_v1::atomic_write_json;

use super::{
    oracle_autonomous_run::{
        apply_owner_steps, elapsed_millis, run_oracle_analysis_to_stop_v1,
        OracleAutonomousRunConfigV1,
    },
    oracle_live_combat_diagnostic_v1, save_oracle_analysis_workspace_v1, OracleAnalysisWorkspaceV1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OracleAnalysisServiceExitV1 {
    pub revision: u64,
    pub saved_revision: u64,
}

struct CommandResult {
    result: Value,
    mutated: bool,
    save_requested: bool,
    shutdown: bool,
}

pub fn serve_oracle_analysis_jsonl_v1<R, W>(
    workspace_path: &Path,
    workspace: OracleAnalysisWorkspaceV1,
    reader: R,
    mut writer: W,
) -> Result<OracleAnalysisServiceExitV1, String>
where
    R: BufRead,
    W: Write,
{
    let mut service = OracleAnalysisServiceState::new(workspace_path, workspace, None);
    write_response(&mut writer, &service.ready_response())?;

    for line in reader.lines() {
        let line = line.map_err(|error| format!("failed to read oracle service input: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let (response, shutdown) = service.handle_line(&line);
        write_response(&mut writer, &response)?;
        if shutdown {
            return Ok(service.exit());
        }
    }

    service.finish()
}

pub fn serve_oracle_analysis_tcp_v1(
    workspace_path: &Path,
    workspace: OracleAnalysisWorkspaceV1,
    bind_address: SocketAddr,
    endpoint_path: &Path,
) -> Result<OracleAnalysisServiceExitV1, String> {
    let listener = TcpListener::bind(bind_address).map_err(|error| {
        format!("failed to bind oracle analysis service at {bind_address}: {error}")
    })?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect oracle service address: {error}"))?;
    if !address.ip().is_loopback() {
        return Err(format!(
            "oracle analysis service must bind a loopback address, got {address}"
        ));
    }
    let auth_token = service_auth_token(workspace_path, address);
    if let Some(parent) = endpoint_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create oracle service endpoint directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let endpoint = OracleAnalysisServiceEndpointV1 {
        schema_name: ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA.to_string(),
        schema_version: ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA_VERSION,
        address,
        auth_token: auth_token.clone(),
        workspace: workspace_path.to_path_buf(),
        process_id: std::process::id(),
        executable: std::env::current_exe()
            .ok()
            .and_then(|path| path.canonicalize().ok()),
    };
    atomic_write_json(endpoint_path, &endpoint)?;
    let _endpoint_guard = EndpointFileGuard(endpoint_path.to_path_buf());
    let mut service = OracleAnalysisServiceState::new(workspace_path, workspace, Some(auth_token));

    for stream in listener.incoming() {
        let stream = stream
            .map_err(|error| format!("failed to accept oracle service connection: {error}"))?;
        let mut connection = BufReader::new(stream);
        let mut line = String::new();
        connection
            .read_line(&mut line)
            .map_err(|error| format!("failed to read oracle service request: {error}"))?;
        let (response, shutdown) = if line.trim().is_empty() {
            (
                error_response(
                    None,
                    "error",
                    service.revision,
                    service.saved_revision,
                    "oracle service connection contained no request".to_string(),
                ),
                false,
            )
        } else {
            service.handle_line(&line)
        };
        write_response(connection.get_mut(), &response)?;
        if shutdown {
            return Ok(service.exit());
        }
    }
    Err("oracle analysis listener stopped unexpectedly".to_string())
}

struct OracleAnalysisServiceState {
    workspace_path: PathBuf,
    workspace: OracleAnalysisWorkspaceV1,
    revision: u64,
    saved_revision: u64,
    auth_token: Option<String>,
}

impl OracleAnalysisServiceState {
    fn new(
        workspace_path: &Path,
        workspace: OracleAnalysisWorkspaceV1,
        auth_token: Option<String>,
    ) -> Self {
        Self {
            workspace_path: workspace_path.to_path_buf(),
            workspace,
            revision: 0,
            saved_revision: 0,
            auth_token,
        }
    }

    fn ready_response(&self) -> OracleAnalysisServiceResponseV1 {
        success_response(
            None,
            "ready",
            self.revision,
            self.saved_revision,
            json!({
                "workspace": self.workspace_path.to_string_lossy(),
                "seed": self.workspace.seed,
                "ascension": self.workspace.ascension,
                "cursor_node_id": self.workspace.session.cursor_node_id(),
                "mainline_node_id": self.workspace.session.mainline_node_id(),
            }),
            None,
        )
    }

    fn handle_line(&mut self, line: &str) -> (OracleAnalysisServiceResponseV1, bool) {
        let request = match serde_json::from_str::<OracleAnalysisServiceRequestV1>(line) {
            Ok(request) => request,
            Err(error) => {
                return (
                    error_response(
                        None,
                        "error",
                        self.revision,
                        self.saved_revision,
                        format!("invalid oracle service request: {error}"),
                    ),
                    false,
                );
            }
        };
        let request_id = request.id.clone();
        if self
            .auth_token
            .as_deref()
            .is_some_and(|expected| request.auth_token.as_deref() != Some(expected))
        {
            return (
                error_response(
                    request_id,
                    "unauthorized",
                    self.revision,
                    self.saved_revision,
                    "oracle service authentication token did not match".to_string(),
                ),
                false,
            );
        }
        let execute_started = Instant::now();
        let command = match execute_command(&mut self.workspace, request.command) {
            Ok(command) => command,
            Err(error) => {
                return (
                    error_response(
                        request_id,
                        "error",
                        self.revision,
                        self.saved_revision,
                        error,
                    ),
                    false,
                );
            }
        };
        let execute_elapsed_ms = elapsed_millis(execute_started);
        if command.mutated {
            self.revision = self.revision.saturating_add(1);
        }
        let mut autosave_elapsed_ms = 0;
        if command.mutated || command.save_requested || command.shutdown {
            let autosave_started = Instant::now();
            if let Err(error) =
                save_oracle_analysis_workspace_v1(&self.workspace_path, &self.workspace)
            {
                return (
                    error_response(
                        request_id,
                        "save_error",
                        self.revision,
                        self.saved_revision,
                        format!(
                            "command was applied in memory, but workspace autosave failed: {error}"
                        ),
                    ),
                    false,
                );
            }
            autosave_elapsed_ms = elapsed_millis(autosave_started);
            self.saved_revision = self.revision;
        }
        let event = if command.shutdown {
            "shutdown"
        } else {
            "result"
        };
        (
            success_response(
                request_id,
                event,
                self.revision,
                self.saved_revision,
                command.result,
                Some(OracleAnalysisServiceTimingV1 {
                    execute_ms: execute_elapsed_ms,
                    autosave_ms: autosave_elapsed_ms,
                    total_ms: execute_elapsed_ms.saturating_add(autosave_elapsed_ms),
                }),
            ),
            command.shutdown,
        )
    }

    fn finish(&mut self) -> Result<OracleAnalysisServiceExitV1, String> {
        save_oracle_analysis_workspace_v1(&self.workspace_path, &self.workspace)?;
        self.saved_revision = self.revision;
        Ok(self.exit())
    }

    fn exit(&self) -> OracleAnalysisServiceExitV1 {
        OracleAnalysisServiceExitV1 {
            revision: self.revision,
            saved_revision: self.saved_revision,
        }
    }
}

struct EndpointFileGuard(PathBuf);

impl Drop for EndpointFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn execute_command(
    workspace: &mut OracleAnalysisWorkspaceV1,
    command: OracleAnalysisServiceCommandV1,
) -> Result<CommandResult, String> {
    let (result, mutated, save_requested, shutdown) = match command {
        OracleAnalysisServiceCommandV1::Ping => (
            json!({
                "seed": workspace.seed,
                "ascension": workspace.ascension,
                "cursor_node_id": workspace.session.cursor_node_id(),
                "mainline_node_id": workspace.session.mainline_node_id(),
            }),
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::Capabilities => (
            json!({
                "commands": [
                    "ping", "capabilities", "status", "explain", "route_policy_audit", "shop_policy_audit", "card_reward_policy_audit", "card_reward_path_audit", "campfire_policy_audit", "view", "tree", "try",
                    "focus", "choose", "owner", "run", "choose_path", "follow", "back", "promote", "advance", "probe_combat", "accept_combat", "restart_combat",
                    "combat_lab_open", "combat_lab_observe", "combat_lab_goto", "combat_lab_back", "combat_lab_keep", "combat_lab_restore", "combat_lab_play_card", "combat_lab_use_potion", "combat_lab_end", "combat_lab_search", "combat_lab_compare", "combat_lab_commit", "combat_lab_clear",
                    "combat_scratch_start", "combat_scratch_status", "combat_scratch_observe", "combat_scratch_play", "combat_scratch_atomic", "combat_scratch_card", "combat_scratch_potion", "combat_scratch_end", "combat_scratch_selection", "combat_scratch_back", "combat_scratch_focus", "combat_scratch_search", "combat_scratch_tree", "combat_scratch_commit", "combat_scratch_clear", "history",
                    "journal", "timeline", "journal_entry", "trajectory", "combat_summary", "combat_diagnostic",
                    "export_combat_case", "export_continuation", "verify_run_witness", "escape_combat", "save", "shutdown"
                ],
                "transport": "newline-delimited JSON over stdin/stdout",
                "autosave": "after every successful mutation",
                "pause": "omit advance commands; in-memory combat work remains resident",
                "status": "compact actionable node summary",
                "view": "full node state including deck and relics",
            }),
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::Status { node } => {
            let view = if let Some(node) = node {
                workspace.session.view_node(node)?
            } else {
                workspace.view()?
            };
            (current_node_summary(workspace, &view)?, false, false, false)
        }
        OracleAnalysisServiceCommandV1::Explain { node, owner_rank } => {
            let view = workspace.session.view_node(node)?;
            let current_owner_order = workspace.session.current_choice_order(node)?;
            let choice_ref = current_owner_order
                .get(usize::try_from(owner_rank).map_err(|_| {
                    format!("owner rank {owner_rank} does not fit the current platform")
                })?)
                .ok_or_else(|| {
                    format!("oracle node {node} has no current owner choice at rank {owner_rank}")
                })?;
            let matching = view
                .choices
                .iter()
                .filter(|choice| choice.choice_ref == *choice_ref)
                .collect::<Vec<_>>();
            let [choice] = matching.as_slice() else {
                return Err(format!(
                    "oracle node {node} has {} materialized choices for current-owner choice '{}'; expected exactly one",
                    matching.len(),
                    choice_ref,
                ));
            };
            let mut result = to_value(choice)?;
            let object = result
                .as_object_mut()
                .ok_or_else(|| "oracle choice did not serialize as an object".to_string())?;
            object.insert(
                "materialized_owner_rank".to_string(),
                json!(choice.owner_rank),
            );
            object.insert("owner_rank".to_string(), json!(owner_rank));
            object.insert(
                "owner_rank_changed".to_string(),
                json!(choice.owner_rank != owner_rank),
            );
            (result, false, false, false)
        }
        OracleAnalysisServiceCommandV1::RoutePolicyAudit { node } => (
            to_value(&workspace.session.route_policy_audit(node)?)?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::ShopPolicyAudit { node } => (
            to_value(&workspace.session.shop_policy_audit(node)?)?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CardRewardPolicyAudit { node } => (
            to_value(&workspace.session.card_reward_policy_audit(node)?)?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CardRewardPathAudit { node } => (
            to_value(&workspace.session.card_reward_path_audit(node)?)?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CampfirePolicyAudit { node } => (
            to_value(&workspace.session.campfire_policy_audit(node)?)?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::View { node } => {
            let view = if let Some(node) = node {
                workspace.session.view_node(node)?
            } else {
                workspace.view()?
            };
            (to_value(view)?, false, false, false)
        }
        OracleAnalysisServiceCommandV1::Tree => {
            (to_value(workspace.session.tree())?, false, false, false)
        }
        OracleAnalysisServiceCommandV1::Try { choice_ref } => {
            workspace.try_choice(&choice_ref)?;
            let view = workspace.view()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::Choose { node, owner_rank } => {
            let current_node = workspace.session.cursor_node_id();
            if current_node != node {
                return Err(format!(
                    "oracle choose expected cursor node {node}, but current cursor is {current_node}"
                ));
            }
            let current = workspace.view()?;
            let current_owner_order = workspace.session.current_choice_order(node)?;
            let choice_ref = current_owner_order
                .get(usize::try_from(owner_rank).map_err(|_| {
                    format!("owner rank {owner_rank} does not fit the current platform")
                })?)
                .ok_or_else(|| {
                    format!("oracle node {node} has no current owner choice at rank {owner_rank}")
                })?;
            let matching = current
                .choices
                .iter()
                .filter(|choice| choice.choice_ref == *choice_ref)
                .collect::<Vec<_>>();
            let [choice] = matching.as_slice() else {
                return Err(format!(
                    "oracle node {node} has {} materialized choices for current-owner choice '{}'; expected exactly one",
                    matching.len(),
                    choice_ref,
                ));
            };
            let choice_ref = choice.choice_ref.clone();
            workspace.try_choice(&choice_ref)?;
            let view = workspace.view()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::Owner { steps } => {
            let batch = apply_owner_steps(workspace, steps)?;
            let status = current_node_summary(workspace, &batch.current)?;
            (
                json!({
                    "requested_steps": steps,
                    "applied_count": batch.applied_count,
                    "applied": batch.applied,
                    "stopped": batch.stopped,
                    "status": status,
                }),
                batch.applied_count > 0,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::Run {
            hallway_wall_ms,
            elite_wall_ms,
            boss_wall_ms,
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            max_boundaries,
            run_wall_ms,
            export_continuation,
        } => (
            run_oracle_analysis_to_stop_v1(
                workspace,
                &OracleAutonomousRunConfigV1 {
                    hallway_wall_ms,
                    elite_wall_ms,
                    boss_wall_ms,
                    max_quanta,
                    quantum_generation_work,
                    quantum_ms,
                    max_boundaries,
                    run_wall_ms,
                    export_continuation,
                },
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::ChoosePath {
            node,
            candidate_ids,
        } => {
            let current_node = workspace.session.cursor_node_id();
            if current_node != node {
                return Err(format!(
                    "oracle choose_path expected cursor node {node}, but current cursor is {current_node}"
                ));
            }
            if candidate_ids.is_empty() {
                return Err("oracle choose_path requires at least one candidate id".to_string());
            }

            let mut applied = Vec::new();
            let mut stopped = None;
            for candidate_id in candidate_ids {
                let current = workspace.view()?;
                let matching = current
                    .choices
                    .iter()
                    .filter(|choice| choice.candidate_id == candidate_id)
                    .collect::<Vec<_>>();
                let [choice] = matching.as_slice() else {
                    stopped = Some(format!(
                        "oracle node {} has {} choices with candidate id '{}'; expected exactly one",
                        current.node_id,
                        matching.len(),
                        candidate_id
                    ));
                    break;
                };
                let parent_node_id = current.node_id;
                let label = choice.label.clone();
                let choice_ref = choice.choice_ref.clone();
                let view = workspace.try_choice(&choice_ref)?;
                applied.push(json!({
                    "parent_node_id": parent_node_id,
                    "child_node_id": view.node_id,
                    "candidate_id": candidate_id,
                    "label": label,
                }));
            }

            let view = workspace.view()?;
            (
                json!({
                    "completed": stopped.is_none(),
                    "applied": applied,
                    "stopped": stopped,
                    "node": current_node_summary(workspace, &view)?,
                }),
                !applied.is_empty(),
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::Focus { node } => {
            workspace.session.focus_node(node)?;
            let view = workspace.view()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::Follow { edge } => {
            workspace.session.follow_edge(edge)?;
            let view = workspace.view()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::Back => {
            workspace.session.back()?;
            let view = workspace.view()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::Promote => {
            workspace.session.promote_cursor();
            let view = workspace.view()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::Advance {
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
            improve_incumbent,
        } => {
            if max_quanta == 0 || quantum_generation_work == 0 || quantum_ms == 0 {
                return Err(
                    "advance max_quanta, quantum_generation_work, and quantum_ms must be positive"
                        .to_string(),
                );
            }
            let (report, view) = workspace.advance(OracleAnalysisAdvanceRequestV1 {
                max_quanta,
                quantum_generation_work,
                quantum_ms: Some(quantum_ms),
                wall_ms,
                improve_incumbent,
            })?;
            (
                json!({"report": report, "node": current_node_transition_summary(workspace, &view)?}),
                true,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::ProbeCombat {
            generation_work,
            quantum_generation_work,
            wall_ms,
        } => {
            if generation_work == 0 || quantum_generation_work == 0 || wall_ms == 0 {
                return Err(
                    "probe_combat requires positive generation_work, quantum_generation_work, and wall_ms"
                        .to_string(),
                );
            }
            let (report, view) = workspace.probe_combat(OracleAnalysisCombatProbeRequestV1 {
                generation_work,
                quantum_generation_work,
                wall_ms,
            })?;
            (
                json!({"report": report, "node": current_node_transition_summary(workspace, &view)?}),
                true,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::AcceptCombat => {
            workspace.accept_combat_incumbent()?;
            let view = workspace.view()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::EscapeCombat => {
            workspace.accept_smoke_bomb_escape()?;
            let view = workspace.view()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::RestartCombat => {
            workspace.session.restart_cursor_combat_search()?;
            let view = workspace.view()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::CombatLabOpen {
            node,
            baseline,
            max_engine_steps_per_transition,
            selection_offset,
            selection_limit,
        } => {
            let baseline = match baseline {
                OracleAnalysisCombatLabBaselineV1::Root => {
                    analysis_session::OracleAnalysisCombatLineLabBaselineSourceV1::Root
                }
                OracleAnalysisCombatLabBaselineV1::ResidentIncumbent => {
                    analysis_session::OracleAnalysisCombatLineLabBaselineSourceV1::ResidentIncumbent
                }
            };
            (
                to_value(workspace.session.open_combat_line_lab(
                    node,
                    baseline,
                    max_engine_steps_per_transition,
                    selection_offset,
                    selection_limit,
                )?)?,
                true,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::CombatLabObserve {
            selection_offset,
            selection_limit,
        } => (
            to_value(
                workspace
                    .session
                    .combat_line_lab_frame(selection_offset, selection_limit)?,
            )?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatLabGoto {
            line,
            turn,
            before_action,
            selection_offset,
            selection_limit,
        } => {
            let line = match line {
                OracleAnalysisCombatLabLineV1::Baseline => {
                    analysis_session::OracleAnalysisCombatLineLabLineV1::Baseline
                }
                OracleAnalysisCombatLabLineV1::Current => {
                    analysis_session::OracleAnalysisCombatLineLabLineV1::Current
                }
            };
            (
                to_value(workspace.session.goto_combat_line_lab(
                    line,
                    turn,
                    before_action,
                    selection_offset,
                    selection_limit,
                )?)?,
                true,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::CombatLabBack {
            selection_offset,
            selection_limit,
        } => (
            to_value(
                workspace
                    .session
                    .back_combat_line_lab(selection_offset, selection_limit)?,
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatLabKeep => (
            to_value(workspace.session.keep_combat_line_lab()?)?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatLabRestore {
            selection_offset,
            selection_limit,
        } => (
            to_value(
                workspace
                    .session
                    .restore_kept_combat_line_lab(selection_offset, selection_limit)?,
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatLabPlayCard {
            card_id,
            occurrence,
            target_index,
            selection_offset,
            selection_limit,
        } => {
            let card_id = serde_json::from_value(serde_json::Value::String(card_id.clone()))
                .map_err(|_| format!("unknown typed card id '{card_id}'"))?;
            let result = workspace.session.play_combat_line_lab_card(
                card_id,
                occurrence,
                target_index,
                selection_offset,
                selection_limit,
            )?;
            let mutated = matches!(
                result,
                analysis_session::OracleAnalysisCombatLineLabPlayCardResultV1::Played { .. }
            );
            (to_value(result)?, mutated, false, false)
        }
        OracleAnalysisServiceCommandV1::CombatLabUsePotion {
            potion_id,
            occurrence,
            target_index,
            selection_offset,
            selection_limit,
        } => {
            let potion_id = serde_json::from_value(serde_json::Value::String(potion_id.clone()))
                .map_err(|_| format!("unknown typed potion id '{potion_id}'"))?;
            let result = workspace.session.use_combat_line_lab_potion(
                potion_id,
                occurrence,
                target_index,
                selection_offset,
                selection_limit,
            )?;
            let mutated = matches!(
                result,
                analysis_session::OracleAnalysisCombatLineLabUsePotionResultV1::Used { .. }
            );
            (to_value(result)?, mutated, false, false)
        }
        OracleAnalysisServiceCommandV1::CombatLabSelect {
            indices,
            selection_offset,
            selection_limit,
        } => (
            to_value(workspace.session.select_combat_line_lab(
                indices,
                selection_offset,
                selection_limit,
            )?)?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatLabEnd {
            selection_offset,
            selection_limit,
        } => (
            to_value(
                workspace
                    .session
                    .end_combat_line_lab_turn(selection_offset, selection_limit)?,
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatLabSearch {
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
        } => {
            let report = workspace.session.search_combat_line_lab(
                analysis_session::OracleAnalysisCombatScratchSearchRequestV1 {
                    max_quanta,
                    quantum_generation_work,
                    quantum_ms,
                    wall_ms,
                },
            )?;
            (
                json!({
                    "report": report,
                    "comparison": workspace.session.compare_combat_line_lab()?,
                }),
                true,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::CombatLabCompare => (
            to_value(workspace.session.compare_combat_line_lab()?)?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatLabCommit => {
            let view = workspace.commit_combat_scratch()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::CombatLabClear => (
            json!({
                "schema_name": "OracleAnalysisCombatLineLabClearReceiptV1",
                "schema_version": 1,
                "cleared": workspace.session.clear_combat_scratch(),
                "run_cursor_node_id": workspace.session.cursor_node_id(),
            }),
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchStart {
            node,
            max_engine_steps_per_transition,
            selection_offset,
            selection_limit,
        } => (
            to_value(
                analysis_session::OracleAnalysisCombatScratchDecisionViewV1::from(
                    workspace.session.start_combat_scratch(
                        node,
                        max_engine_steps_per_transition,
                        selection_offset,
                        selection_limit,
                    )?,
                ),
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchStatus {
            selection_offset,
            selection_limit,
        } => (
            to_value(
                workspace
                    .session
                    .combat_scratch_view(selection_offset, selection_limit)?,
            )?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchObserve {
            selection_offset,
            selection_limit,
        } => (
            to_value(
                workspace
                    .session
                    .combat_scratch_decision_view(selection_offset, selection_limit)?,
            )?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchPlay {
            action_ref,
            selection_offset,
            selection_limit,
        } => (
            to_value(workspace.session.play_combat_scratch_action(
                &action_ref,
                selection_offset,
                selection_limit,
            )?)?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchAtomic {
            scratch_node,
            action_index,
            full_observation,
            selection_offset,
            selection_limit,
        } => (
            play_combat_scratch_selector(
                workspace,
                analysis_session::OracleAnalysisCombatScratchActionSelectorV1::Atomic {
                    scratch_node_id: scratch_node,
                    action_index,
                },
                full_observation,
                selection_offset,
                selection_limit,
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchCard {
            scratch_node,
            card_lineage,
            target,
            full_observation,
            selection_offset,
            selection_limit,
        } => {
            let card_lineage = card_lineage
                .parse()
                .map_err(|error| format!("invalid typed combat scratch card lineage: {error}"))?;
            (
                play_combat_scratch_selector(
                    workspace,
                    analysis_session::OracleAnalysisCombatScratchActionSelectorV1::Card {
                        scratch_node_id: scratch_node,
                        card_lineage,
                        target,
                    },
                    full_observation,
                    selection_offset,
                    selection_limit,
                )?,
                true,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::CombatScratchHandCard {
            scratch_node,
            hand_index,
            target_index,
            full_observation,
            selection_offset,
            selection_limit,
        } => (
            play_combat_scratch_selector(
                workspace,
                analysis_session::OracleAnalysisCombatScratchActionSelectorV1::HandCard {
                    scratch_node_id: scratch_node,
                    hand_index,
                    target_index,
                },
                full_observation,
                selection_offset,
                selection_limit,
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchPotion {
            scratch_node,
            potion_uuid,
            target,
            full_observation,
            selection_offset,
            selection_limit,
        } => (
            play_combat_scratch_selector(
                workspace,
                analysis_session::OracleAnalysisCombatScratchActionSelectorV1::Potion {
                    scratch_node_id: scratch_node,
                    potion_uuid,
                    target,
                },
                full_observation,
                selection_offset,
                selection_limit,
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchPotionSlot {
            scratch_node,
            potion_slot,
            target_index,
            full_observation,
            selection_offset,
            selection_limit,
        } => (
            play_combat_scratch_selector(
                workspace,
                analysis_session::OracleAnalysisCombatScratchActionSelectorV1::PotionSlot {
                    scratch_node_id: scratch_node,
                    potion_slot,
                    target_index,
                },
                full_observation,
                selection_offset,
                selection_limit,
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchEnd {
            scratch_node,
            full_observation,
            selection_offset,
            selection_limit,
        } => {
            let scratch_node = scratch_node
                .map(Ok)
                .unwrap_or_else(|| workspace.session.combat_scratch_cursor_node_id())?;
            (
                play_combat_scratch_selector(
                    workspace,
                    analysis_session::OracleAnalysisCombatScratchActionSelectorV1::EndTurn {
                        scratch_node_id: scratch_node,
                    },
                    full_observation,
                    selection_offset,
                    selection_limit,
                )?,
                true,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::CombatScratchSelection {
            scratch_node,
            family_index,
            input_index,
            full_observation,
            selection_offset,
            selection_limit,
        } => (
            play_combat_scratch_selector(
                workspace,
                analysis_session::OracleAnalysisCombatScratchActionSelectorV1::Selection {
                    scratch_node_id: scratch_node,
                    family_index,
                    input_index,
                },
                full_observation,
                selection_offset,
                selection_limit,
            )?,
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchBack {
            full_observation,
            selection_offset,
            selection_limit,
        } => (
            if full_observation {
                to_value(
                    analysis_session::OracleAnalysisCombatScratchDecisionViewV1::from(
                        workspace
                            .session
                            .back_combat_scratch(selection_offset, selection_limit)?,
                    ),
                )?
            } else {
                to_value(workspace.session.back_combat_scratch_receipt()?)?
            },
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchFocus {
            scratch_node,
            full_observation,
            selection_offset,
            selection_limit,
        } => (
            if full_observation {
                to_value(
                    analysis_session::OracleAnalysisCombatScratchDecisionViewV1::from(
                        workspace.session.focus_combat_scratch_node(
                            scratch_node,
                            selection_offset,
                            selection_limit,
                        )?,
                    ),
                )?
            } else {
                to_value(
                    workspace
                        .session
                        .focus_combat_scratch_node_receipt(scratch_node)?,
                )?
            },
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchSearch {
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
            selection_offset,
            selection_limit,
        } => {
            let (report, view) = workspace.session.search_combat_scratch(
                analysis_session::OracleAnalysisCombatScratchSearchRequestV1 {
                    max_quanta,
                    quantum_generation_work,
                    quantum_ms,
                    wall_ms,
                },
                selection_offset,
                selection_limit,
            )?;
            (
                json!({
                    "report": report,
                    "view": analysis_session::OracleAnalysisCombatScratchDecisionViewV1::from(view),
                }),
                true,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::CombatScratchTree => (
            to_value(workspace.session.combat_scratch_tree()?)?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatScratchCommit => {
            let view = workspace.commit_combat_scratch()?;
            (current_node_summary(workspace, &view)?, true, false, false)
        }
        OracleAnalysisServiceCommandV1::CombatScratchClear => (
            json!({
                "schema_name": "OracleAnalysisCombatScratchClearReceiptV1",
                "schema_version": 1,
                "cleared": workspace.session.clear_combat_scratch(),
                "run_cursor_node_id": workspace.session.cursor_node_id(),
            }),
            true,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::History { node } => {
            let node = node.unwrap_or_else(|| workspace.session.cursor_node_id());
            (
                to_value(workspace.session.replay(node)?)?,
                false,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::Journal { node, tail } => {
            if tail == 0 || tail > 500 {
                return Err("journal tail must be in 1..=500".to_string());
            }
            let entries = workspace.session.journal_entries(node)?;
            let start = entries.len().saturating_sub(tail);
            (
                json!({
                    "node_id": node,
                    "total_entries": entries.len(),
                    "returned_entries": entries.len().saturating_sub(start),
                    "entries": &entries[start..],
                }),
                false,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::Timeline { node, tail } => {
            if tail == 0 || tail > 500 {
                return Err("timeline tail must be in 1..=500".to_string());
            }
            let entries = workspace.session.journal_entries(node)?;
            let start = entries.len().saturating_sub(tail);
            let compact = entries[start..]
                .iter()
                .enumerate()
                .map(|(offset, entry)| match entry {
                    crate::eval::run_control::RunProgressStepV1::Decision(record) => json!({
                        "journal_index": start + offset,
                        "kind": "decision",
                        "location": record.before.location,
                        "title": record.before.title,
                        "chosen": record.result.chosen_label,
                        "candidates": record.before.candidates.iter().map(|candidate| &candidate.label).collect::<Vec<_>>(),
                    }),
                    crate::eval::run_control::RunProgressStepV1::ForcedTransition(record) => json!({
                        "journal_index": start + offset,
                        "kind": "forced_transition",
                        "location": record.before.location,
                        "title": record.before.title,
                    }),
                    crate::eval::run_control::RunProgressStepV1::CombatResolution(record) => json!({
                        "journal_index": start + offset,
                        "kind": "combat_resolution",
                        "location": record.before.location,
                        "title": record.before.title,
                        "resolution": record.kind,
                        "actions": record.trajectory.action_count,
                        "hp_change": record.result.changes.iter().find_map(|change| match change {
                            crate::eval::run_control::RunActionResultChangeV1::HpChanged {
                                before_current,
                                before_max,
                                after_current,
                                after_max,
                            } => Some(json!({
                                "before_current": before_current,
                                "before_max": before_max,
                                "after_current": after_current,
                                "after_max": after_max,
                            })),
                            _ => None,
                        }),
                        "potions_lost": record.result.changes.iter().filter_map(|change| match change {
                            crate::eval::run_control::RunActionResultChangeV1::PotionLost { potion, slot } => {
                                Some(json!({"potion": potion, "slot": slot}))
                            }
                            _ => None,
                        }).collect::<Vec<_>>(),
                    }),
                    crate::eval::run_control::RunProgressStepV1::Stop(record) => json!({
                        "journal_index": start + offset,
                        "kind": "stop",
                        "stop_kind": record.kind,
                        "reason": record.reason,
                    }),
                })
                .collect::<Vec<_>>();
            (
                json!({
                    "node_id": node,
                    "total_entries": entries.len(),
                    "returned_entries": compact.len(),
                    "entries": compact,
                }),
                false,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::JournalEntry { node, index } => {
            let entries = workspace.session.journal_entries(node)?;
            let entry = entries.get(index).ok_or_else(|| {
                format!(
                    "oracle node {node} journal index {index} is out of range 0..{}",
                    entries.len()
                )
            })?;
            (to_value(entry)?, false, false, false)
        }
        OracleAnalysisServiceCommandV1::Trajectory { node } => {
            let trajectory = workspace
                .session
                .combat_trajectory(node)?
                .ok_or_else(|| format!("oracle node {node} has no recorded combat trajectory"))?;
            (
                json!({"node_id": node, "trajectory": trajectory}),
                false,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::CombatSummary { node } => (
            to_value(workspace.session.combat_summary(node)?)?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::CombatDiagnostic {
            node,
            max_engine_steps_per_transition,
        } => (
            oracle_live_combat_diagnostic_v1(workspace, node, max_engine_steps_per_transition)?,
            false,
            false,
            false,
        ),
        OracleAnalysisServiceCommandV1::ExportCombatCase { node, path } => {
            let view = workspace.session.view_node(node)?;
            let (generation_work, wall_ms) = if view
                .encounter
                .as_ref()
                .is_some_and(|encounter| encounter.is_boss)
            {
                (
                    workspace.budget.boss_generation_work,
                    workspace.budget.boss_ms,
                )
            } else if view
                .encounter
                .as_ref()
                .is_some_and(|encounter| encounter.is_elite)
            {
                (
                    workspace.budget.elite_generation_work,
                    workspace.budget.elite_ms,
                )
            } else {
                (
                    workspace.budget.hallway_generation_work,
                    workspace.budget.hallway_ms,
                )
            };
            let case = workspace.session.combat_case(
                node,
                workspace.seed,
                workspace.ascension,
                generation_work,
                wall_ms,
            )?;
            crate::eval::combat_case::save_combat_case(&path, &case)?;
            (
                json!({
                    "node_id": node,
                    "path": path,
                    "combat": case.core.combat,
                }),
                false,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::ExportContinuation { node, path } => {
            let continuation = workspace.continuation(node)?;
            super::oracle_run::save_oracle_run_continuation_v1(&path, &continuation)?;
            (
                json!({
                    "node_id": node,
                    "path": path,
                    "journal_entries": continuation.journal.entries().len(),
                }),
                false,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::VerifyRunWitness { node } => {
            let node = node.unwrap_or_else(|| workspace.session.cursor_node_id());
            let continuation = workspace.continuation(node)?;
            let expected_final = continuation.session.into_session()?;
            let report = crate::eval::run_control::exact_replay_run_progress_journal_v1(
                workspace.seed,
                workspace.ascension,
                &continuation.journal,
                &expected_final,
            )?;
            (
                json!({
                    "schema_name": "ExactOracleRunWitnessReplayV1",
                    "schema_version": 1,
                    "node_id": node,
                    "report": report,
                }),
                false,
                false,
                false,
            )
        }
        OracleAnalysisServiceCommandV1::Save => (json!({"saved": true}), false, true, false),
        OracleAnalysisServiceCommandV1::Shutdown => (json!({"saved": true}), false, true, true),
    };
    Ok(CommandResult {
        result,
        mutated,
        save_requested,
        shutdown,
    })
}

fn success_response(
    id: Option<Value>,
    event: &str,
    revision: u64,
    saved_revision: u64,
    result: Value,
    timing: Option<OracleAnalysisServiceTimingV1>,
) -> OracleAnalysisServiceResponseV1 {
    OracleAnalysisServiceResponseV1 {
        protocol: ORACLE_ANALYSIS_SERVICE_PROTOCOL.to_string(),
        protocol_version: ORACLE_ANALYSIS_SERVICE_PROTOCOL_VERSION,
        id,
        event: event.to_string(),
        ok: true,
        revision,
        saved_revision,
        timing,
        result: Some(result),
        error: None,
    }
}

fn error_response(
    id: Option<Value>,
    event: &str,
    revision: u64,
    saved_revision: u64,
    error: String,
) -> OracleAnalysisServiceResponseV1 {
    OracleAnalysisServiceResponseV1 {
        protocol: ORACLE_ANALYSIS_SERVICE_PROTOCOL.to_string(),
        protocol_version: ORACLE_ANALYSIS_SERVICE_PROTOCOL_VERSION,
        id,
        event: event.to_string(),
        ok: false,
        revision,
        saved_revision,
        timing: None,
        result: None,
        error: Some(error),
    }
}

fn write_response<W: Write>(
    writer: &mut W,
    response: &OracleAnalysisServiceResponseV1,
) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, response)
        .map_err(|error| format!("failed to serialize oracle service response: {error}"))?;
    writer
        .write_all(b"\n")
        .map_err(|error| format!("failed to write oracle service response: {error}"))?;
    writer
        .flush()
        .map_err(|error| format!("failed to flush oracle service response: {error}"))
}

fn play_combat_scratch_selector(
    workspace: &mut OracleAnalysisWorkspaceV1,
    selector: analysis_session::OracleAnalysisCombatScratchActionSelectorV1,
    full_observation: bool,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    if full_observation {
        let view = workspace.session.play_combat_scratch_selector(
            selector,
            selection_offset,
            selection_limit,
        )?;
        to_value(analysis_session::OracleAnalysisCombatScratchDecisionViewV1::from(view))
    } else {
        to_value(workspace.session.play_combat_scratch_selector_delta(
            selector,
            selection_offset,
            selection_limit,
        )?)
    }
}

fn to_value<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value)
        .map_err(|error| format!("failed to serialize oracle service result: {error}"))
}

fn current_node_summary(
    workspace: &OracleAnalysisWorkspaceV1,
    view: &OracleAnalysisNodeViewV1,
) -> Result<Value, String> {
    let current_owner_order = workspace.session.current_choice_order(view.node_id)?;
    Ok(node_summary(view, &current_owner_order))
}

fn node_summary(view: &OracleAnalysisNodeViewV1, current_owner_order: &[String]) -> Value {
    let choices = view
        .choices
        .iter()
        .map(|choice| {
            let annotation_kind = choice.annotation.as_ref().and_then(annotation_kind);
            let current_owner_rank = current_owner_order
                .iter()
                .position(|choice_ref| choice_ref == &choice.choice_ref);
            json!({
                "choice_ref": choice.choice_ref,
                "kind": choice.kind,
                "candidate_id": choice.candidate_id,
                "label": choice.label,
                "owner_rank": current_owner_rank,
                "materialized_owner_rank": choice.owner_rank,
                "owner_rank_changed": current_owner_rank != usize::try_from(choice.owner_rank).ok(),
                "path_discrepancy": choice.path_discrepancy,
                "annotation_kind": annotation_kind,
            })
        })
        .collect::<Vec<_>>();
    let children = view
        .children
        .iter()
        .map(|child| {
            json!({
                "edge_id": child.edge_id,
                "child_node_id": child.child_node_id,
                "kind": child.kind,
                "label": child.label,
                "is_on_mainline": child.is_on_mainline,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "node_id": view.node_id,
        "canonical_parent_node_id": view.canonical_parent_node_id,
        "is_cursor": view.is_cursor,
        "is_on_mainline": view.is_on_mainline,
        "boundary": view.boundary,
        "act": view.act,
        "floor": view.floor,
        "current_hp": view.current_hp,
        "max_hp": view.max_hp,
        "gold": view.gold,
        "event": view.event,
        "reward": view.reward,
        "choice_count": choices.len(),
        "owner_rank_scope": "current_policy_or_materialized_exact_choice",
        "choices": choices,
        "child_count": children.len(),
        "children": children,
        "encounter": view.encounter,
        "combat": view.combat,
    })
}

fn current_node_transition_summary(
    workspace: &OracleAnalysisWorkspaceV1,
    view: &OracleAnalysisNodeViewV1,
) -> Result<Value, String> {
    let mut summary = current_node_summary(workspace, view)?;
    if let Some(object) = summary.as_object_mut() {
        object.remove("combat");
        object.remove("encounter");
    }
    Ok(summary)
}

fn annotation_kind<T: Serialize>(annotation: &T) -> Option<String> {
    serde_json::to_value(annotation)
        .ok()?
        .get("kind")?
        .as_str()
        .map(str::to_string)
}

fn service_auth_token(workspace_path: &Path, address: SocketAddr) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut hasher = Blake2s256::new();
    hasher.update(workspace_path.to_string_lossy().as_bytes());
    hasher.update(address.to_string().as_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(now.to_le_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
