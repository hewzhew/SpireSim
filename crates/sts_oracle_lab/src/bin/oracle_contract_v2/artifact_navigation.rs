use serde_json::{json, Value};
use sts_oracle_runtime::ai::combat_search_v2::{
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices,
    recoverable_stolen_gold, unrecovered_stolen_gold, CombatSearchV2Config,
    CombatSearchV2PotionPolicy, CombatSearchV2TurnPlanProbeCandidate,
    CombatSearchV2TurnPlanProbeEnumeration,
};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::sim::combat::{combat_terminal, CombatPosition, CombatTerminal};
use sts_oracle_runtime::state::core::{ClientInput, EngineState};

use super::super::combat_trace_view::combat_turn_snapshot;
use super::artifact_trace::{load_root, replay_candidate, ReplayedActionTraceV2};
use super::{ArtifactCandidateRole, CombatContractArtifactV2, CombatContractTerminalCandidateV2};

pub(super) struct ArtifactNavigationSpec<'a> {
    pub(super) candidate: ArtifactCandidateRole,
    pub(super) turn: u32,
    pub(super) follow_plan: &'a [usize],
    pub(super) follow_state: &'a [String],
    pub(super) max_inner_nodes: usize,
    pub(super) max_end_states: usize,
    pub(super) per_bucket_limit: usize,
    pub(super) input_label: &'static str,
}

pub(super) struct ArtifactNavigationResult {
    pub(super) candidate: CombatContractTerminalCandidateV2,
    pub(super) prefix_inputs: Vec<ClientInput>,
    pub(super) position: CombatPosition,
    pub(super) source: Value,
    pub(super) followed: Vec<ArtifactNavigationStep>,
    pub(super) config: CombatSearchV2Config,
}

pub(super) struct ArtifactNavigationStep {
    pub(super) depth: usize,
    pub(super) from_turn: u32,
    pub(super) state_query: Option<String>,
    pub(super) matching_plan_count: Option<usize>,
    pub(super) candidate: CombatSearchV2TurnPlanProbeCandidate,
}

pub(super) fn resolve(
    artifact_path: &std::path::Path,
    artifact: &CombatContractArtifactV2,
    spec: ArtifactNavigationSpec<'_>,
) -> Result<ArtifactNavigationResult, String> {
    if spec.max_inner_nodes == 0 || spec.max_end_states == 0 || spec.per_bucket_limit == 0 {
        return Err("artifact navigation limits must be positive".to_owned());
    }
    let candidate = candidate_for_role(artifact, spec.candidate)
        .cloned()
        .ok_or_else(|| {
            format!(
                "V2 artifact '{}' has no {:?} terminal candidate",
                artifact_path.display(),
                spec.candidate
            )
        })?;
    let root = load_root(artifact_path, artifact)?;
    let (candidate_actions, trace) = replay_candidate(&root, &candidate)?;
    let (prefix_action_count, mut position) = position_at_player_turn(&root, &trace, spec.turn)?;
    let mut prefix_inputs = candidate_actions[..prefix_action_count].to_vec();
    let config = navigation_config(&spec, artifact);
    let source = branch_state(&position);
    let mut followed = Vec::with_capacity(spec.follow_plan.len() + spec.follow_state.len());

    for (depth, plan_index) in spec.follow_plan.iter().copied().enumerate() {
        ensure_player_turn(&position, depth)?;
        let audit = enumerate_turn(&position, &config);
        let selected = candidate_by_plan_index(&audit, plan_index)?;
        verify_candidate_successor(&selected, plan_index)?;
        prefix_inputs.extend(
            selected
                .report
                .actions
                .iter()
                .map(|action| action.input.clone()),
        );
        followed.push(ArtifactNavigationStep {
            depth,
            from_turn: position.combat.turn.turn_count,
            state_query: None,
            matching_plan_count: None,
            candidate: selected.clone(),
        });
        position = selected.position;
    }
    for (depth, state_query) in spec.follow_state.iter().enumerate() {
        ensure_player_turn(&position, depth)?;
        let audit = enumerate_turn(&position, &config);
        let (selected, matching_plan_count) =
            unique_state_candidate(&audit, state_query)?.ok_or_else(|| {
                format!(
                    "turn surface has no exact successor matching '{state_query}' across {} selected plans",
                    audit.candidates.len()
                )
            })?;
        verify_candidate_successor(selected, selected.report.plan_index)?;
        prefix_inputs.extend(
            selected
                .report
                .actions
                .iter()
                .map(|action| action.input.clone()),
        );
        followed.push(ArtifactNavigationStep {
            depth,
            from_turn: position.combat.turn.turn_count,
            state_query: Some(state_query.clone()),
            matching_plan_count: Some(matching_plan_count),
            candidate: selected.clone(),
        });
        position = selected.position.clone();
    }

    Ok(ArtifactNavigationResult {
        candidate,
        prefix_inputs,
        position,
        source,
        followed,
        config,
    })
}

fn navigation_config(
    spec: &ArtifactNavigationSpec<'_>,
    artifact: &CombatContractArtifactV2,
) -> CombatSearchV2Config {
    let mut config = CombatSearchV2Config::default();
    config.max_engine_steps_per_action = 250;
    config.turn_plan_probe_max_inner_nodes = Some(spec.max_inner_nodes);
    config.turn_plan_probe_max_end_states = Some(spec.max_end_states);
    config.turn_plan_probe_per_bucket_limit = Some(spec.per_bucket_limit);
    config.potion_policy = if artifact.request.max_potions_used == 0 {
        CombatSearchV2PotionPolicy::Never
    } else {
        CombatSearchV2PotionPolicy::All
    };
    config.max_potions_used = Some(artifact.request.max_potions_used);
    config.allow_potion_discard = Some(false);
    config.input_label = Some(spec.input_label.to_owned());
    config
}

fn candidate_for_role(
    artifact: &CombatContractArtifactV2,
    role: ArtifactCandidateRole,
) -> Option<&CombatContractTerminalCandidateV2> {
    artifact
        .terminal_candidates
        .iter()
        .find(|candidate| match role {
            ArtifactCandidateRole::Contract => candidate.selected_by_contract_view,
            ArtifactCandidateRole::LocalHp => candidate.selected_by_local_hp_view,
        })
}

fn position_at_player_turn(
    root: &CombatPosition,
    trace: &ReplayedActionTraceV2,
    turn: u32,
) -> Result<(usize, CombatPosition), String> {
    if root.combat.turn.turn_count == turn && matches!(root.engine, EngineState::CombatPlayerTurn) {
        return Ok((0, root.clone()));
    }
    trace
        .prefix_positions
        .iter()
        .enumerate()
        .find(|(_, position)| {
            position.combat.turn.turn_count == turn
                && matches!(position.engine, EngineState::CombatPlayerTurn)
        })
        .map(|(index, position)| (index + 1, position.clone()))
        .ok_or_else(|| {
            format!("candidate does not contain an exact player-turn boundary for turn {turn}")
        })
}

fn candidate_by_plan_index(
    audit: &CombatSearchV2TurnPlanProbeEnumeration,
    plan_index: usize,
) -> Result<CombatSearchV2TurnPlanProbeCandidate, String> {
    audit
        .candidates
        .iter()
        .find(|candidate| candidate.report.plan_index == plan_index)
        .cloned()
        .ok_or_else(|| {
            let available = audit
                .candidates
                .iter()
                .take(32)
                .map(|candidate| candidate.report.plan_index.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "turn surface has no selected plan index {plan_index}; available selected indices (up to 32): [{available}]"
            )
        })
}

fn verify_candidate_successor(
    candidate: &CombatSearchV2TurnPlanProbeCandidate,
    plan_index: usize,
) -> Result<(), String> {
    let expected_hash = candidate
        .report
        .steps
        .last()
        .map(|step| step.state_after_exact_state_hash.as_str());
    let actual_hash = exact_candidate_hash(candidate);
    if expected_hash != Some(actual_hash.as_str()) {
        return Err(format!(
            "followed plan {plan_index} exact successor drifted: report={expected_hash:?}, replay={actual_hash}"
        ));
    }
    Ok(())
}

pub(super) fn enumerate_turn(
    position: &CombatPosition,
    config: &CombatSearchV2Config,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices(
        &position.engine,
        &position.combat,
        config,
    )
}

pub(super) fn ensure_player_turn(
    position: &CombatPosition,
    branch_depth: usize,
) -> Result<(), String> {
    if matches!(position.engine, EngineState::CombatPlayerTurn) {
        Ok(())
    } else {
        Err(format!(
            "followed branch depth {branch_depth} did not reach a player-turn boundary: engine={:?}, terminal={:?}",
            position.engine,
            combat_terminal(&position.engine, &position.combat),
        ))
    }
}

pub(super) fn unique_state_candidate<'a>(
    audit: &'a CombatSearchV2TurnPlanProbeEnumeration,
    query: &str,
) -> Result<Option<(&'a CombatSearchV2TurnPlanProbeCandidate, usize)>, String> {
    if query.is_empty() {
        return Err("exact successor state query must not be empty".to_owned());
    }
    let mut by_hash =
        std::collections::BTreeMap::<String, Vec<&CombatSearchV2TurnPlanProbeCandidate>>::new();
    for candidate in &audit.candidates {
        let exact_hash = exact_candidate_hash(candidate);
        if exact_hash.starts_with(query) {
            by_hash.entry(exact_hash).or_default().push(candidate);
        }
    }
    if by_hash.len() > 1 {
        return Err(format!(
            "exact successor prefix '{query}' is ambiguous across {} states",
            by_hash.len()
        ));
    }
    Ok(by_hash.into_values().next().map(|mut candidates| {
        candidates.sort_by_key(|candidate| candidate.report.plan_index);
        (candidates[0], candidates.len())
    }))
}

fn exact_candidate_hash(candidate: &CombatSearchV2TurnPlanProbeCandidate) -> String {
    combat_exact_state_hash_v2(&candidate.position.engine, &candidate.position.combat)
}

pub(super) fn branch_state(position: &CombatPosition) -> Value {
    json!({
        "turn": position.combat.turn.turn_count,
        "exact_state_hash": combat_exact_state_hash_v2(&position.engine, &position.combat),
        "recoverable_stolen_gold": recoverable_stolen_gold(&position.combat),
        "unrecovered_stolen_gold": unrecovered_stolen_gold(&position.combat),
        "state": combat_turn_snapshot(position),
    })
}

pub(super) fn unresolved_player_turn(position: &CombatPosition) -> bool {
    combat_terminal(&position.engine, &position.combat) == CombatTerminal::Unresolved
        && matches!(position.engine, EngineState::CombatPlayerTurn)
}
