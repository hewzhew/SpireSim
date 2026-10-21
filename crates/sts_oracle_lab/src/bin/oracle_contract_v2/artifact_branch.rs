use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;

use super::super::print_json;
use super::artifact_navigation::{resolve, unresolved_player_turn, ArtifactNavigationSpec};
use super::{
    run_combat_contract, ArtifactBranchArgs, CombatContractArtifactV2,
    CombatContractDiagnosticPrefixV2,
};

pub(super) fn run(
    args: &ArtifactBranchArgs,
    artifact: &CombatContractArtifactV2,
) -> Result<(), String> {
    if args.generation_work == 0 {
        return Err("--generation-work must be positive".to_owned());
    }
    if args.wall_ms == 0 {
        return Err("--wall-ms must be positive".to_owned());
    }
    let navigation = resolve(
        &args.artifact,
        artifact,
        ArtifactNavigationSpec {
            candidate: args.navigation.candidate,
            turn: args.navigation.turn,
            follow_plan: &args.navigation.follow_plan,
            follow_state: &args.navigation.follow_state,
            max_inner_nodes: args.navigation.max_inner_nodes,
            max_end_states: args.navigation.max_end_states,
            per_bucket_limit: args.navigation.per_bucket_limit,
            input_label: "oracle_contract_v2_artifact_branch",
        },
    )?;
    if navigation.prefix_inputs.is_empty() {
        return Err(
            "artifact branch must select a non-empty exact prefix; use artifact rerun for the unchanged root"
                .to_owned(),
        );
    }
    if !unresolved_player_turn(&navigation.position) {
        return Err(
            "artifact branch prefix must end at an unresolved player-turn boundary".to_owned(),
        );
    }
    let search_root_exact_state_hash =
        combat_exact_state_hash_v2(&navigation.position.engine, &navigation.position.combat);
    let mut request = artifact.request.clone();
    request.generation_work = args.generation_work;
    request.wall_ms = args.wall_ms;
    request.diagnostic_prefix = Some(CombatContractDiagnosticPrefixV2 {
        source_candidate_id: navigation.candidate.candidate_id,
        source_candidate_terminal_exact_state_hash: navigation.candidate.terminal_exact_state_hash,
        source_turn: args.navigation.turn,
        follow_plan: args.navigation.follow_plan.clone(),
        follow_state: args.navigation.follow_state.clone(),
        expected_search_root_exact_state_hash: search_root_exact_state_hash,
        inputs: navigation.prefix_inputs,
    });

    let result = run_combat_contract(request)?;
    print_json(&result)
}
