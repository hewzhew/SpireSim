use std::path::PathBuf;

use clap::Parser;

use super::{Cli, Command, OracleDriveBoundaryArg};

#[test]
fn drive_parses_typed_stop_boundary() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "drive",
        "--workspace",
        "case.workspace.json",
        "--stop-at",
        "map-decision",
    ])
    .expect("typed drive boundary should parse");
    let Command::Drive { stop_at, .. } = cli.command else {
        panic!("expected drive command");
    };

    assert_eq!(stop_at, Some(OracleDriveBoundaryArg::MapDecision));
}

#[test]
fn drive_parses_optional_full_ledger_output() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "drive",
        "--workspace",
        "case.workspace.json",
        "--output",
        "drive.json",
    ])
    .expect("drive ledger output should parse");
    let Command::Drive { output, .. } = cli.command else {
        panic!("expected drive command");
    };

    assert_eq!(output, Some(PathBuf::from("drive.json")));
}

#[test]
fn compact_combat_contract_is_the_public_exact_search_surface() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "contract",
        "combat",
        "--case",
        "fight.case.json",
        "--min-final-hp",
        "66",
        "--max-potions-used",
        "0",
        "--require-recovered-stolen-gold",
    ])
    .expect("compact V2 combat contract should parse");
    assert!(matches!(cli.command, Command::Contract(_)));
}

#[test]
fn artifact_trace_needs_only_the_v2_artifact_identity() {
    let cli = Cli::try_parse_from(["oracle_lab", "artifact", "trace", "contract-artifact"])
        .expect("artifact-owned witness trace should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn artifact_trace_accepts_explicit_policy_detail_projection() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "artifact",
        "trace",
        "contract-artifact",
        "--detail",
        "policy",
    ])
    .expect("artifact trace policy detail opt-in should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn artifact_trace_accepts_checkpoint_only_projection() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "artifact",
        "trace",
        "contract-artifact",
        "--detail",
        "checkpoints",
    ])
    .expect("artifact trace checkpoint projection should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn artifact_search_needs_only_the_v2_artifact_identity() {
    let cli = Cli::try_parse_from(["oracle_lab", "artifact", "search", "contract-artifact"])
        .expect("artifact-owned search accounting should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn artifact_search_accepts_one_exact_state_query() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "artifact",
        "search",
        "contract-artifact",
        "--state",
        "3273823f",
    ])
    .expect("artifact-owned exact-state service query should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn artifact_summaries_accepts_several_artifacts_without_shell_aggregation() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "artifact",
        "summaries",
        "contract-artifact-a",
        "contract-artifact-b",
        "contract-artifact-c",
    ])
    .expect("artifact-owned result collection should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn artifact_turn_can_follow_displayed_exact_plan_indices() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "artifact",
        "turn",
        "contract-artifact",
        "--candidate",
        "contract",
        "--turn",
        "1",
        "--follow-plan",
        "2",
        "--follow-plan",
        "7",
        "--scan-next-terminal",
    ])
    .expect("artifact-owned exact branch traversal should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn artifact_turn_can_navigate_and_filter_by_exact_successor_identity() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "artifact",
        "turn",
        "contract-artifact",
        "--candidate",
        "contract",
        "--turn",
        "1",
        "--follow-state",
        "3273823f",
        "--successor-state",
        "8122e07a",
    ])
    .expect("artifact-owned exact-state traversal should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn artifact_turn_can_return_only_the_replay_checked_reached_state() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "artifact",
        "turn",
        "contract-artifact",
        "--candidate",
        "contract",
        "--turn",
        "1",
        "--follow-state",
        "3273823f",
        "--reached-only",
    ])
    .expect("artifact-owned compact reached-state traversal should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn artifact_turn_reached_only_rejects_surface_queries() {
    let error = Cli::try_parse_from([
        "oracle_lab",
        "artifact",
        "turn",
        "contract-artifact",
        "--turn",
        "1",
        "--reached-only",
        "--successor-state",
        "8122e07a",
    ])
    .expect_err("reached-only traversal must not silently ignore a surface query");
    assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn artifact_branch_inherits_contract_and_selects_one_exact_successor() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "artifact",
        "branch",
        "contract-artifact",
        "--candidate",
        "contract",
        "--turn",
        "1",
        "--follow-state",
        "3273823f",
        "--generation-work",
        "4096",
        "--wall-ms",
        "2000",
    ])
    .expect("artifact-owned exact branch contract should parse");
    assert!(matches!(cli.command, Command::Artifact(_)));
}

#[test]
fn retired_rollout_control_surfaces_are_not_accepted() {
    assert!(
        Cli::try_parse_from(["oracle_lab", "artifact", "rollout", "contract-artifact"]).is_err()
    );
    for command in ["v2-capability-audit", "audit-boundary-successor-lookahead"] {
        assert!(
            Cli::try_parse_from(["oracle_lab", command]).is_err(),
            "{command} must not remain a compatibility alias"
        );
    }
}

#[test]
fn retired_combat_case_names_are_not_accepted() {
    for command in ["combat-case", "combat-case-local-graph"] {
        assert!(
            Cli::try_parse_from(["oracle_lab", command, "--case", "fight.case.json",]).is_err(),
            "{command} must not remain a compatibility alias"
        );
    }
}

#[test]
fn compact_workspace_parses_source_node_and_fresh_output() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "compact-workspace",
        "--workspace",
        "source.workspace.json",
        "--node",
        "42",
        "--output",
        "active.workspace.json",
    ])
    .expect("compact workspace command should parse");
    let Command::CompactWorkspace {
        workspace,
        node,
        output,
    } = cli.command
    else {
        panic!("expected compact workspace command");
    };

    assert_eq!(workspace, PathBuf::from("source.workspace.json"));
    assert_eq!(node, Some(42));
    assert_eq!(output, PathBuf::from("active.workspace.json"));
}

#[test]
fn repack_workspace_parses_source_and_fresh_output() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "repack-workspace",
        "--workspace",
        "historical.workspace.json",
        "--output",
        "pooled.workspace.json",
    ])
    .expect("repack workspace command should parse");
    let Command::RepackWorkspace { workspace, output } = cli.command else {
        panic!("expected repack workspace command");
    };

    assert_eq!(workspace, PathBuf::from("historical.workspace.json"));
    assert_eq!(output, PathBuf::from("pooled.workspace.json"));
}

#[test]
fn route_policy_audit_parses_an_optional_exact_node() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "route-policy-audit",
        "--workspace",
        "run.workspace.json",
        "--node",
        "186",
    ])
    .expect("route policy audit should parse");
    let Command::RoutePolicyAudit(args) = cli.command else {
        panic!("expected route policy audit command");
    };

    assert_eq!(args.workspace, PathBuf::from("run.workspace.json"));
    assert_eq!(args.node, Some(186));
}

#[test]
fn shop_policy_audit_parses_an_optional_exact_node() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "shop-policy-audit",
        "--workspace",
        "run.workspace.json",
        "--node",
        "27",
    ])
    .expect("shop policy audit should parse");
    let Command::ShopPolicyAudit(args) = cli.command else {
        panic!("expected shop policy audit command");
    };

    assert_eq!(args.workspace, PathBuf::from("run.workspace.json"));
    assert_eq!(args.node, Some(27));
}

#[test]
fn witness_verification_defaults_to_cursor_and_accepts_an_exact_node() {
    let omitted = Cli::try_parse_from([
        "oracle_lab",
        "verify-run-witness",
        "--workspace",
        "run.workspace.json",
    ])
    .expect("cursor witness verification should parse");
    let Command::VerifyRunWitness { node, .. } = omitted.command else {
        panic!("expected witness verification command");
    };
    assert_eq!(node, None);

    let explicit = Cli::try_parse_from([
        "oracle_lab",
        "verify-run-witness",
        "--workspace",
        "run.workspace.json",
        "--node",
        "35",
    ])
    .expect("exact-node witness verification should parse");
    let Command::VerifyRunWitness { node, .. } = explicit.command else {
        panic!("expected witness verification command");
    };
    assert_eq!(node, Some(35));
}

#[test]
fn current_stage_probe_is_a_separate_bounded_command() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "probe-combat",
        "--workspace",
        "run.workspace.json",
        "--generation-work",
        "768",
        "--quantum-generation-work",
        "128",
        "--wall-ms",
        "250",
    ])
    .expect("current-stage probe should parse");
    let Command::ProbeCombat {
        generation_work,
        quantum_generation_work,
        wall_ms,
        ..
    } = cli.command
    else {
        panic!("expected probe-combat command");
    };
    assert_eq!(generation_work, 768);
    assert_eq!(quantum_generation_work, 128);
    assert_eq!(wall_ms, 250);
}

#[test]
fn run_witness_suite_can_list_manifest_entries_without_replay() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "verify-run-witness-suite",
        "--manifest",
        "suite.json",
        "--list",
    ])
    .expect("run-witness suite inventory should parse");
    assert!(matches!(cli.command, Command::VerifyRunWitnessSuite { .. }));
}

#[test]
fn run_witness_suite_can_select_one_exact_witness() {
    let cli = Cli::try_parse_from([
        "oracle_lab",
        "verify-run-witness-suite",
        "--manifest",
        "suite.json",
        "--witness",
        "seed006",
    ])
    .expect("single run-witness replay should parse");
    assert!(matches!(cli.command, Command::VerifyRunWitnessSuite { .. }));
}
