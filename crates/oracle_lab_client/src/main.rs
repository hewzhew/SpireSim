use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use fs2::FileExt;
use oracle_artifact_contract::{ensure_artifact_fresh, CanonicalArtifact};
use oracle_lab_protocol::{
    call_oracle_analysis_tcp_v1, resolve_owned_resident_workspace,
    OracleAnalysisCombatLabBaselineV1, OracleAnalysisCombatLabLineV1,
    OracleAnalysisServiceCommandV1, OracleAnalysisServiceEndpointV1,
};
use serde::Serialize;
use serde_json::{json, Value};

mod resident_process;

use resident_process::spawn_resident_service;

#[derive(Debug, Parser)]
#[command(
    name = "oracle_lab",
    about = "Fast client for a resident oracle laboratory",
    after_help = "Use `start` and `live` for routine work. Deliberately invoke heavyweight offline commands through `.\\ol.ps1`, for example `.\\ol.ps1 combat-case ...`."
)]
struct Cli {
    #[arg(long, hide = true, global = true)]
    canonical_oracle: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start or reconnect to a named resident workspace service.
    Start {
        #[arg(long)]
        session: String,
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Send one raw JSON command to a resident loopback service.
    Call {
        #[arg(long)]
        endpoint: PathBuf,
        #[arg(long)]
        request: String,
    },
    /// Use typed commands against a resident oracle workspace.
    Live {
        #[arg(long, conflicts_with = "session")]
        endpoint: Option<PathBuf>,
        #[arg(long, conflicts_with = "endpoint")]
        session: Option<String>,
        #[command(subcommand)]
        command: LiveCommand,
    },
    /// Deliberately invoke a heavyweight offline oracle-lab command.
    Offline {
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
}

#[derive(Debug, Subcommand)]
enum LiveCommand {
    /// Show the current node, choices, and tactical progress.
    Status {
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, default_value_t = 8)]
        limit: usize,
    },
    /// Continue tactical search and return its new compact status.
    Advance {
        #[arg(long, default_value_t = 100_000)]
        max_quanta: usize,
        #[arg(long, default_value_t = 4_096)]
        quantum_generation_work: usize,
        #[arg(long, default_value_t = 100)]
        quantum_ms: u64,
        #[arg(long, default_value_t = 10_000)]
        wall_ms: u64,
        /// Continue past an insufficient first win until configured strategic
        /// quality is reached.
        #[arg(long)]
        improve_incumbent: bool,
    },
    /// Spend a fixed diagnostic work budget only in the current combat stage.
    /// This never promotes potion identity or materializes the incumbent.
    ProbeCombat {
        #[arg(long, default_value_t = 4_096)]
        generation_work: usize,
        #[arg(long, default_value_t = 256)]
        quantum_generation_work: usize,
        #[arg(long, default_value_t = 1_000)]
        wall_ms: u64,
    },
    /// Choose an owner-ranked decision at the current node.
    Choose {
        #[arg(long)]
        owner_rank: u64,
        #[arg(long)]
        node: Option<usize>,
    },
    /// Move the resident cursor to an existing retained node without replaying it.
    Focus {
        #[arg(long)]
        node: usize,
    },
    /// Show the retained run inventory at the current or selected node.
    Inspect {
        #[arg(long)]
        node: Option<usize>,
    },
    /// Explain the exact route owner's complete ranked evidence at a map node.
    Route {
        #[arg(long)]
        node: Option<usize>,
    },
    /// Explain the exact shop owner's complete ranked evidence at a shop node.
    Shop {
        #[arg(long)]
        node: Option<usize>,
    },
    /// Explain the exact card-reward owner's complete ranked evidence.
    CardReward {
        #[arg(long)]
        node: Option<usize>,
    },
    /// Audit every exact card-reward surface on the retained path ending at
    /// the selected node. Defaults to the resident cursor.
    CardRewardPath {
        #[arg(long)]
        node: Option<usize>,
        /// Write the complete typed report here and print only a compact
        /// receipt, keeping large evidence out of the terminal.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Explain the exact campfire owner's complete ranked evidence at a campfire node.
    Campfire {
        #[arg(long)]
        node: Option<usize>,
    },
    /// List every retained exact variation and its parent/child edges.
    Tree,
    /// Apply the owner's first choice for a bounded number of decisions.
    Owner {
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=64))]
        steps: u8,
    },
    /// Drive owner decisions and bounded combat search until victory or the first real stop.
    Run {
        #[arg(long, default_value_t = 5_000)]
        hallway_wall_ms: u64,
        #[arg(long, default_value_t = 15_000)]
        elite_wall_ms: u64,
        #[arg(long, default_value_t = 30_000)]
        boss_wall_ms: u64,
        #[arg(long, default_value_t = 100_000)]
        max_quanta: usize,
        #[arg(long, default_value_t = 4_096)]
        quantum_generation_work: usize,
        #[arg(long, default_value_t = 100)]
        quantum_ms: u64,
        #[arg(long, default_value_t = 256)]
        max_boundaries: usize,
        /// Stop this seed at a resumable boundary after the total wall allowance.
        #[arg(long)]
        run_wall_ms: Option<u64>,
        /// Export the exact continuation after a verified terminal victory.
        #[arg(long)]
        export_continuation: Option<PathBuf>,
    },
    /// Accept the current verified combat incumbent.
    Accept,
    /// Explicitly take the current exact Smoke Bomb escape.
    Escape,
    /// Restart tactical search at the unchanged exact combat state.
    Restart,
    /// Print a compact timeline for the current or selected node.
    Timeline {
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, default_value_t = 30)]
        tail: usize,
    },
    /// Export the current or selected exact combat case.
    ExportCase {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        node: Option<usize>,
    },
    /// Export the current or selected exact run continuation.
    ExportContinuation {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        node: Option<usize>,
    },
    /// Replay the committed run journal from the canonical seed state.
    Verify {
        #[arg(long)]
        node: Option<usize>,
    },
    /// Show the exact combat root and replayed deepest trajectories.
    /// Heavy replay executes inside the resident workspace service.
    Combat {
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, default_value_t = 512)]
        max_engine_steps_per_transition: usize,
    },
    /// Show a compact, turn-by-turn trace of the trajectories retained by combat search.
    Trace {
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, default_value_t = 512)]
        max_engine_steps_per_transition: usize,
    },
    /// Compare generation, admission, and downstream service by root action.
    RootActions {
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, default_value_t = 512)]
        max_engine_steps_per_transition: usize,
    },
    /// Open one incumbent-backed combat line, branch by semantic choices, and compare exact paths.
    Lab {
        #[command(subcommand)]
        command: LiveLabCommand,
    },
    /// Incrementally play, branch, rewind, and commit one exact combat scratch DAG.
    Scratch {
        #[command(subcommand)]
        command: LiveScratchCommand,
    },
    /// Save the resident workspace immediately.
    Save,
    /// Save and stop the resident workspace service.
    Shutdown,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum LiveLabBaseline {
    Root,
    Incumbent,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum LiveLabLine {
    Baseline,
    Current,
}

#[derive(Debug, Subcommand)]
enum LiveLabCommand {
    Open {
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, value_enum, default_value = "incumbent")]
        baseline: LiveLabBaseline,
        #[arg(long, default_value_t = 512)]
        max_engine_steps_per_transition: usize,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Observe {
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    /// Move to the exact selected-line frame before one zero-based action in a turn.
    Goto {
        #[arg(long, value_enum, default_value = "current")]
        line: LiveLabLine,
        #[arg(long)]
        turn: u32,
        #[arg(long, default_value_t = 0)]
        before: usize,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    /// Move back one action on the current line.
    Back {
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    /// Keep the exact current line as the one-slot restore point.
    Keep,
    /// Return to the exact kept line without naming an internal node.
    Restore {
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    /// Play a typed card id from the current frame; duplicate copies return typed ambiguity.
    Play {
        #[arg(long)]
        card: String,
        #[arg(long = "copy")]
        occurrence: Option<usize>,
        #[arg(long)]
        target: Option<usize>,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    /// Use a typed potion id; duplicate copies or targets return typed ambiguity.
    Potion {
        #[arg(long)]
        potion: String,
        #[arg(long = "copy")]
        occurrence: Option<usize>,
        #[arg(long)]
        target: Option<usize>,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    /// Submit one structured Hand/Grid/Scry choice by current-frame domain indices.
    Select {
        #[arg(long, value_delimiter = ',', num_args = 0.., required = true)]
        indices: Vec<usize>,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    End {
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Search {
        #[arg(long, default_value_t = 4)]
        max_quanta: usize,
        #[arg(long, default_value_t = 1_024)]
        quantum_generation_work: usize,
        #[arg(long, default_value_t = 100)]
        quantum_ms: u64,
        #[arg(long, default_value_t = 1_000)]
        wall_ms: u64,
    },
    Compare,
    Commit,
    Clear,
}

#[derive(Debug, Subcommand)]
enum LiveScratchCommand {
    Start {
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, default_value_t = 512)]
        max_engine_steps_per_transition: usize,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Status {
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Observe {
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Play {
        #[arg(long)]
        action_ref: String,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Atomic {
        #[arg(long)]
        from: u64,
        #[arg(long)]
        action: usize,
        #[arg(long)]
        full: bool,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Card {
        #[arg(long)]
        from: u64,
        #[arg(long, required_unless_present = "lineage", conflicts_with = "lineage")]
        hand: Option<usize>,
        #[arg(long, hide = true, conflicts_with = "hand")]
        lineage: Option<String>,
        #[arg(long)]
        target: Option<usize>,
        #[arg(long)]
        full: bool,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Potion {
        #[arg(long)]
        from: u64,
        #[arg(long, required_unless_present = "uuid", conflicts_with = "uuid")]
        slot: Option<usize>,
        #[arg(long, hide = true, conflicts_with = "slot")]
        uuid: Option<u32>,
        #[arg(long)]
        target: Option<usize>,
        #[arg(long)]
        full: bool,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    End {
        #[arg(long)]
        from: Option<u64>,
        #[arg(long)]
        full: bool,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Selection {
        #[arg(long)]
        from: u64,
        #[arg(long)]
        family: usize,
        #[arg(long)]
        input: usize,
        #[arg(long)]
        full: bool,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Back {
        #[arg(long)]
        full: bool,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Focus {
        #[arg(long)]
        scratch_node: u64,
        #[arg(long)]
        full: bool,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Search {
        #[arg(long, default_value_t = 4)]
        max_quanta: usize,
        #[arg(long, default_value_t = 1_024)]
        quantum_generation_work: usize,
        #[arg(long, default_value_t = 100)]
        quantum_ms: u64,
        #[arg(long, default_value_t = 1_000)]
        wall_ms: u64,
        #[command(flatten)]
        page: LiveScratchPageArgs,
    },
    Tree,
    Commit,
    Clear,
}

#[derive(Clone, Copy, Debug, clap::Args)]
struct LiveScratchPageArgs {
    #[arg(long, default_value_t = 0)]
    selection_offset: usize,
    #[arg(long, default_value_t = 24, value_parser = clap::value_parser!(u8).range(1..=64))]
    selection_limit: u8,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    validate_canonical_launch(cli.canonical_oracle)?;
    match cli.command {
        Command::Start { session, workspace } => start_session(&session, &workspace),
        Command::Call { endpoint, request } => {
            print_json(&call_oracle_analysis_tcp_v1(&endpoint, &request)?)
        }
        Command::Live {
            endpoint,
            session,
            command,
        } => {
            let endpoint = resolve_endpoint_argument(endpoint, session)?;
            run_live_command(&endpoint, command)
        }
        Command::Offline { arguments } => delegate_heavy(arguments),
    }
}

fn run_live_command(endpoint: &Path, command: LiveCommand) -> Result<(), String> {
    let _mutation_guard = live_command_mutates(&command)
        .then(|| LiveMutationGuard::acquire(endpoint))
        .transpose()?;
    match command {
        LiveCommand::Status { node, limit } => {
            let result = live_call(endpoint, OracleAnalysisServiceCommandV1::Status { node })?;
            print_json(&compact_live_node(&result, limit))
        }
        LiveCommand::Advance {
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
            improve_incumbent,
        } => {
            let before = live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::Status { node: None },
            )?;
            let result = live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::Advance {
                    max_quanta,
                    quantum_generation_work,
                    quantum_ms,
                    wall_ms: Some(wall_ms),
                    improve_incumbent,
                },
            )?;
            print_json(&compact_live_advance(&before, &result))
        }
        LiveCommand::ProbeCombat {
            generation_work,
            quantum_generation_work,
            wall_ms,
        } => {
            let before = live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::Status { node: None },
            )?;
            let result = live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::ProbeCombat {
                    generation_work,
                    quantum_generation_work,
                    wall_ms,
                },
            )?;
            print_json(&compact_live_advance(&before, &result))
        }
        LiveCommand::Choose { owner_rank, node } => {
            let node = resolve_live_node(endpoint, node)?;
            let result = live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::Choose { node, owner_rank },
            )?;
            print_json(&compact_live_node(&result, 8))
        }
        LiveCommand::Focus { node } => print_json(&compact_live_node(
            &live_call(endpoint, OracleAnalysisServiceCommandV1::Focus { node })?,
            8,
        )),
        LiveCommand::Inspect { node } => {
            let node = resolve_live_node(endpoint, node)?;
            let view = live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::View { node: Some(node) },
            )?;
            print_json(&compact_live_inventory(&view))
        }
        LiveCommand::Route { node } => {
            let node = resolve_live_node(endpoint, node)?;
            print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::RoutePolicyAudit { node },
            )?)
        }
        LiveCommand::Shop { node } => {
            let node = resolve_live_node(endpoint, node)?;
            print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::ShopPolicyAudit { node },
            )?)
        }
        LiveCommand::CardReward { node } => {
            let node = resolve_live_node(endpoint, node)?;
            print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CardRewardPolicyAudit { node },
            )?)
        }
        LiveCommand::CardRewardPath { node, output } => {
            let node = resolve_live_node(endpoint, node)?;
            let report = live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CardRewardPathAudit { node },
            )?;
            if let Some(output) = output {
                write_fresh_json(&output, &report)?;
                print_json(&json!({
                    "schema_name": "OracleAnalysisCardRewardPathAuditWriteReceipt",
                    "schema_version": 1,
                    "target_node_id": node,
                    "boundary_count": report
                        .get("boundaries")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len),
                    "candidate_count": report
                        .get("boundaries")
                        .and_then(Value::as_array)
                        .map_or(0, |boundaries| boundaries
                            .iter()
                            .filter_map(|boundary| boundary.get("audit"))
                            .filter_map(|audit| audit.get("candidates"))
                            .filter_map(Value::as_array)
                            .map(Vec::len)
                            .sum::<usize>()),
                    "output": output,
                }))
            } else {
                print_json(&report)
            }
        }
        LiveCommand::Campfire { node } => {
            let node = resolve_live_node(endpoint, node)?;
            print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CampfirePolicyAudit { node },
            )?)
        }
        LiveCommand::Tree => {
            print_json(&live_call(endpoint, OracleAnalysisServiceCommandV1::Tree)?)
        }
        LiveCommand::Owner { steps } => print_json(&run_live_owner(endpoint, steps)?),
        LiveCommand::Run {
            hallway_wall_ms,
            elite_wall_ms,
            boss_wall_ms,
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            max_boundaries,
            run_wall_ms,
            export_continuation,
        } => print_json(&live_call(
            endpoint,
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
            },
        )?),
        LiveCommand::Accept => print_json(&compact_live_node(
            &live_call(endpoint, OracleAnalysisServiceCommandV1::AcceptCombat)?,
            8,
        )),
        LiveCommand::Escape => print_json(&compact_live_node(
            &live_call(endpoint, OracleAnalysisServiceCommandV1::EscapeCombat)?,
            8,
        )),
        LiveCommand::Restart => print_json(&compact_live_node(
            &live_call(endpoint, OracleAnalysisServiceCommandV1::RestartCombat)?,
            8,
        )),
        LiveCommand::Timeline { node, tail } => {
            let node = resolve_live_node(endpoint, node)?;
            print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::Timeline { node, tail },
            )?)
        }
        LiveCommand::ExportCase { path, node } => {
            let node = resolve_live_node(endpoint, node)?;
            print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::ExportCombatCase { node, path },
            )?)
        }
        LiveCommand::ExportContinuation { path, node } => {
            let node = resolve_live_node(endpoint, node)?;
            print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::ExportContinuation { node, path },
            )?)
        }
        LiveCommand::Verify { node } => print_json(&live_call(
            endpoint,
            OracleAnalysisServiceCommandV1::VerifyRunWitness { node },
        )?),
        LiveCommand::Save => {
            print_json(&live_call(endpoint, OracleAnalysisServiceCommandV1::Save)?)
        }
        LiveCommand::Shutdown => print_json(&shutdown_live_session(endpoint)?),
        LiveCommand::Combat {
            node,
            max_engine_steps_per_transition,
        } => {
            let node = resolve_live_node(endpoint, node)?;
            print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatDiagnostic {
                    node,
                    max_engine_steps_per_transition,
                },
            )?)
        }
        LiveCommand::Trace {
            node,
            max_engine_steps_per_transition,
        } => {
            let node = resolve_live_node(endpoint, node)?;
            let diagnostic = live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatDiagnostic {
                    node,
                    max_engine_steps_per_transition,
                },
            )?;
            print_json(&compact_live_combat_trace(&diagnostic))
        }
        LiveCommand::RootActions {
            node,
            max_engine_steps_per_transition,
        } => {
            let node = resolve_live_node(endpoint, node)?;
            let diagnostic = live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatDiagnostic {
                    node,
                    max_engine_steps_per_transition,
                },
            )?;
            print_json(&compact_root_action_report(&diagnostic))
        }
        LiveCommand::Lab { command } => match command {
            LiveLabCommand::Open {
                node,
                baseline,
                max_engine_steps_per_transition,
                page,
            } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabOpen {
                    node,
                    baseline: match baseline {
                        LiveLabBaseline::Root => OracleAnalysisCombatLabBaselineV1::Root,
                        LiveLabBaseline::Incumbent => {
                            OracleAnalysisCombatLabBaselineV1::ResidentIncumbent
                        }
                    },
                    max_engine_steps_per_transition,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveLabCommand::Observe { page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabObserve {
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveLabCommand::Goto {
                line,
                turn,
                before,
                page,
            } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabGoto {
                    line: match line {
                        LiveLabLine::Baseline => OracleAnalysisCombatLabLineV1::Baseline,
                        LiveLabLine::Current => OracleAnalysisCombatLabLineV1::Current,
                    },
                    turn,
                    before_action: before,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveLabCommand::Back { page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabBack {
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveLabCommand::Keep => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabKeep,
            )?),
            LiveLabCommand::Restore { page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabRestore {
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveLabCommand::Play {
                card,
                occurrence,
                target,
                page,
            } => print_combat_lab_semantic_action(live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabPlayCard {
                    card_id: card,
                    occurrence,
                    target_index: target,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveLabCommand::Potion {
                potion,
                occurrence,
                target,
                page,
            } => print_combat_lab_semantic_action(live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabUsePotion {
                    potion_id: potion,
                    occurrence,
                    target_index: target,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveLabCommand::Select { indices, page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabSelect {
                    indices,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveLabCommand::End { page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabEnd {
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveLabCommand::Search {
                max_quanta,
                quantum_generation_work,
                quantum_ms,
                wall_ms,
            } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabSearch {
                    max_quanta,
                    quantum_generation_work,
                    quantum_ms,
                    wall_ms,
                },
            )?),
            LiveLabCommand::Compare => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabCompare,
            )?),
            LiveLabCommand::Commit => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabCommit,
            )?),
            LiveLabCommand::Clear => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatLabClear,
            )?),
        },
        LiveCommand::Scratch { command } => match command {
            LiveScratchCommand::Start {
                node,
                max_engine_steps_per_transition,
                page,
            } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchStart {
                    node,
                    max_engine_steps_per_transition,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Status { page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchStatus {
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Observe { page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchObserve {
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Play { action_ref, page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchPlay {
                    action_ref,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Atomic {
                from,
                action,
                full,
                page,
            } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchAtomic {
                    scratch_node: from,
                    action_index: action,
                    full_observation: full,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Card {
                from,
                hand,
                lineage,
                target,
                full,
                page,
            } => {
                let command = match (hand, lineage) {
                    (Some(hand_index), None) => {
                        OracleAnalysisServiceCommandV1::CombatScratchHandCard {
                            scratch_node: from,
                            hand_index,
                            target_index: target,
                            full_observation: full,
                            selection_offset: page.selection_offset,
                            selection_limit: usize::from(page.selection_limit),
                        }
                    }
                    (None, Some(card_lineage)) => {
                        OracleAnalysisServiceCommandV1::CombatScratchCard {
                            scratch_node: from,
                            card_lineage,
                            target,
                            full_observation: full,
                            selection_offset: page.selection_offset,
                            selection_limit: usize::from(page.selection_limit),
                        }
                    }
                    _ => return Err("choose exactly one of --hand or --lineage".to_string()),
                };
                print_json(&live_call(endpoint, command)?)
            }
            LiveScratchCommand::Potion {
                from,
                slot,
                uuid,
                target,
                full,
                page,
            } => {
                let command = match (slot, uuid) {
                    (Some(potion_slot), None) => {
                        OracleAnalysisServiceCommandV1::CombatScratchPotionSlot {
                            scratch_node: from,
                            potion_slot,
                            target_index: target,
                            full_observation: full,
                            selection_offset: page.selection_offset,
                            selection_limit: usize::from(page.selection_limit),
                        }
                    }
                    (None, Some(potion_uuid)) => {
                        OracleAnalysisServiceCommandV1::CombatScratchPotion {
                            scratch_node: from,
                            potion_uuid,
                            target,
                            full_observation: full,
                            selection_offset: page.selection_offset,
                            selection_limit: usize::from(page.selection_limit),
                        }
                    }
                    _ => return Err("choose exactly one of --slot or --uuid".to_string()),
                };
                print_json(&live_call(endpoint, command)?)
            }
            LiveScratchCommand::End { from, full, page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchEnd {
                    scratch_node: from,
                    full_observation: full,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Selection {
                from,
                family,
                input,
                full,
                page,
            } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchSelection {
                    scratch_node: from,
                    family_index: family,
                    input_index: input,
                    full_observation: full,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Back { full, page } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchBack {
                    full_observation: full,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Focus {
                scratch_node,
                full,
                page,
            } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchFocus {
                    scratch_node,
                    full_observation: full,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Search {
                max_quanta,
                quantum_generation_work,
                quantum_ms,
                wall_ms,
                page,
            } => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchSearch {
                    max_quanta,
                    quantum_generation_work,
                    quantum_ms,
                    wall_ms,
                    selection_offset: page.selection_offset,
                    selection_limit: usize::from(page.selection_limit),
                },
            )?),
            LiveScratchCommand::Tree => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchTree,
            )?),
            LiveScratchCommand::Commit => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchCommit,
            )?),
            LiveScratchCommand::Clear => print_json(&live_call(
                endpoint,
                OracleAnalysisServiceCommandV1::CombatScratchClear,
            )?),
        },
    }
}

fn live_command_mutates(command: &LiveCommand) -> bool {
    matches!(
        command,
        LiveCommand::Advance { .. }
            | LiveCommand::ProbeCombat { .. }
            | LiveCommand::Choose { .. }
            | LiveCommand::Focus { .. }
            | LiveCommand::Owner { .. }
            | LiveCommand::Run { .. }
            | LiveCommand::Accept
            | LiveCommand::Escape
            | LiveCommand::Restart
            | LiveCommand::Lab {
                command: LiveLabCommand::Open { .. }
                    | LiveLabCommand::Goto { .. }
                    | LiveLabCommand::Back { .. }
                    | LiveLabCommand::Keep
                    | LiveLabCommand::Restore { .. }
                    | LiveLabCommand::Play { .. }
                    | LiveLabCommand::Potion { .. }
                    | LiveLabCommand::Select { .. }
                    | LiveLabCommand::End { .. }
                    | LiveLabCommand::Search { .. }
                    | LiveLabCommand::Commit
                    | LiveLabCommand::Clear,
            }
            | LiveCommand::Scratch {
                command: LiveScratchCommand::Start { .. }
                    | LiveScratchCommand::Play { .. }
                    | LiveScratchCommand::Atomic { .. }
                    | LiveScratchCommand::Card { .. }
                    | LiveScratchCommand::Potion { .. }
                    | LiveScratchCommand::End { .. }
                    | LiveScratchCommand::Selection { .. }
                    | LiveScratchCommand::Back { .. }
                    | LiveScratchCommand::Focus { .. }
                    | LiveScratchCommand::Search { .. }
                    | LiveScratchCommand::Commit
                    | LiveScratchCommand::Clear,
            }
            | LiveCommand::Save
            | LiveCommand::Shutdown
    )
}

struct LiveMutationGuard {
    file: fs::File,
}

impl LiveMutationGuard {
    fn acquire(endpoint: &Path) -> Result<Self, String> {
        let path = endpoint.with_extension("mutation.lock");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| {
                format!(
                    "failed to open resident mutation lock {}: {error}",
                    path.display()
                )
            })?;
        file.try_lock_exclusive().map_err(|error| {
            format!(
                "another typed mutating command is already active for this resident session (lock {}): {error}",
                path.display()
            )
        })?;
        Ok(Self { file })
    }
}

impl Drop for LiveMutationGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn live_call(endpoint: &Path, command: OracleAnalysisServiceCommandV1) -> Result<Value, String> {
    let request = serde_json::to_string(&command)
        .map_err(|error| format!("failed to encode typed oracle command: {error}"))?;
    let response = call_oracle_analysis_tcp_v1(endpoint, &request)?;
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| format!("oracle service returned event '{}'", response.event)));
    }
    response.result.ok_or_else(|| {
        format!(
            "oracle service event '{}' returned no result",
            response.event
        )
    })
}

fn shutdown_live_session(endpoint_path: &Path) -> Result<Value, String> {
    let endpoint = active_endpoint(endpoint_path);
    let result = live_call(endpoint_path, OracleAnalysisServiceCommandV1::Shutdown)?;

    // The service saves before acknowledging shutdown, then removes its
    // endpoint as the process exits. Wait briefly so stale host images can be
    // reclaimed immediately instead of surviving until the next session
    // launch.
    let deadline = Instant::now() + Duration::from_secs(2);
    while active_endpoint(endpoint_path).is_some() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(25));
    }
    if let Some(executable) = endpoint.and_then(|endpoint| endpoint.executable) {
        prune_unreferenced_service_images(
            &oracle_lab_state_root().join("hosts"),
            &oracle_lab_state_root().join("sessions"),
            &executable,
        );
    }
    Ok(result)
}

fn resolve_live_node(endpoint: &Path, node: Option<usize>) -> Result<usize, String> {
    if let Some(node) = node {
        return Ok(node);
    }
    live_call(
        endpoint,
        OracleAnalysisServiceCommandV1::Status { node: None },
    )?
    .get("node_id")
    .and_then(Value::as_u64)
    .and_then(|node| usize::try_from(node).ok())
    .ok_or_else(|| "oracle status did not contain a valid current node_id".to_string())
}

fn run_live_owner(endpoint: &Path, steps: u8) -> Result<Value, String> {
    live_call(endpoint, OracleAnalysisServiceCommandV1::Owner { steps })
}

fn compact_live_node(node: &Value, limit: usize) -> Value {
    let choices = limited_values(node.get("choices"), limit);
    let children = limited_values(node.get("children"), limit);
    let choice_count = node
        .get("choice_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            node.get("choices")
                .and_then(Value::as_array)
                .map_or(0, |values| values.len() as u64)
        });
    let child_count = node
        .get("child_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            node.get("children")
                .and_then(Value::as_array)
                .map_or(0, |values| values.len() as u64)
        });
    json!({
        "node": node.get("node_id"),
        "parent": node.get("canonical_parent_node_id"),
        "act": node.get("act"),
        "floor": node.get("floor"),
        "hp": node.get("current_hp"),
        "max_hp": node.get("max_hp"),
        "gold": node.get("gold"),
        "boundary": node.get("boundary"),
        "event": node.get("event"),
        "reward": node.get("reward"),
        "choice_count": choice_count,
        "owner_rank_scope": node.get("owner_rank_scope"),
        "choices_shown": choices.len(),
        "choices_truncated": choice_count > choices.len() as u64,
        "choices": choices,
        "child_count": child_count,
        "children_shown": children.len(),
        "children_truncated": child_count > children.len() as u64,
        "children": children,
        "encounter": compact_encounter(node.get("encounter")),
        "combat": compact_combat_progress(node.get("combat")),
    })
}

fn compact_live_inventory(node: &Value) -> Value {
    json!({
        "node": node.get("node_id"),
        "act": node.get("act"),
        "floor": node.get("floor"),
        "hp": node.get("current_hp"),
        "max_hp": node.get("max_hp"),
        "gold": node.get("gold"),
        "deck": node.get("deck"),
        "relics": node.get("relics"),
        "potions": node.get("potions"),
    })
}

fn limited_values(values: Option<&Value>, limit: usize) -> Vec<Value> {
    values
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(limit)
        .cloned()
        .collect()
}

fn compact_live_advance(before: &Value, result: &Value) -> Value {
    let report = result.get("report");
    let before_combat = before.get("combat");
    let after_combat = report.and_then(|report| report.get("combat"));
    json!({
        "status": report.and_then(|report| report.get("status").or_else(|| report.get("stop"))),
        "elapsed_ms": report.and_then(|report| report.get("elapsed_ms")),
        "quanta": report.and_then(|report| report.get("quanta_served")),
        "generation_work_requested": report.and_then(|report| report.get("generation_work_requested")),
        "generation_work_consumed": report.and_then(|report| report.get("generation_work_consumed")),
        "work_delta": {
            "generation_work": value_u64(after_combat, "generation_work").saturating_sub(value_u64(before_combat, "generation_work")),
            "current_search_generation_work": value_u64(after_combat, "current_search_generation_work").saturating_sub(value_u64(before_combat, "current_search_generation_work")),
            "exact_states": value_u64(after_combat, "exact_states").saturating_sub(value_u64(before_combat, "exact_states")),
            "completed_turn_options": value_u64(after_combat, "completed_turn_options").saturating_sub(value_u64(before_combat, "completed_turn_options")),
        },
        "combat": compact_combat_progress(after_combat),
        "node": result.get("node"),
    })
}

fn value_u64(value: Option<&Value>, field: &str) -> u64 {
    value
        .and_then(|value| value.get(field))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn compact_encounter(encounter: Option<&Value>) -> Value {
    let Some(encounter) = encounter.filter(|value| !value.is_null()) else {
        return Value::Null;
    };
    json!({
        "turn": encounter.get("turn"),
        "phase": encounter.get("phase"),
        "energy": encounter.get("energy"),
        "player_block": encounter.get("player_block"),
        "hand": encounter.get("hand").and_then(Value::as_array).map(|cards| cards.iter().map(card_value_label).collect::<Vec<_>>()),
        "draw": encounter.get("draw_pile_count"),
        "discard": encounter.get("discard_pile_count"),
        "exhaust": encounter.get("exhaust_pile_count"),
        "is_elite": encounter.get("is_elite"),
        "is_boss": encounter.get("is_boss"),
        "monsters": encounter.get("monsters"),
    })
}

fn compact_combat_progress(combat: Option<&Value>) -> Value {
    let Some(combat) = combat.filter(|value| !value.is_null()) else {
        return Value::Null;
    };
    json!({
        "root_exact_state_hash": combat.get("root_exact_state_hash"),
        "stage_trace": combat.get("stage_trace"),
        "search_stage": combat.get("search_stage"),
        "max_potions_used": combat.get("max_potions_used"),
        "allowed_potion_slots": combat.get("allowed_potion_slots"),
        "potion_spend_requires_satisfaction": combat.get("potion_spend_requires_satisfaction"),
        "generation_work": combat.get("generation_work"),
        "historical_generation_work": combat.get("historical_generation_work"),
        "current_search_generation_work": combat.get("current_search_generation_work"),
        "local_generation_work": combat.get("local_generation_work"),
        "discrepancy_generation_work": combat.get("discrepancy_generation_work"),
        "exact_states": combat.get("exact_states"),
        "local_exact_states": combat.get("local_exact_states"),
        "discrepancy_exact_states": combat.get("discrepancy_exact_states"),
        "completed_turn_options": combat.get("completed_turn_options"),
        "max_player_turn": combat.get("max_player_turn"),
        "deepest_progress": combat.get("deepest_progress_state"),
        "deepest_survival": combat.get("deepest_survival_state"),
        "incumbent_discovery_source": combat.get("incumbent_discovery_source"),
        "incumbent_final_hp": combat.get("incumbent_final_hp"),
        "incumbent_hp_loss": combat.get("incumbent_hp_loss"),
        "incumbent_actions": combat.get("incumbent_action_count"),
        "incumbent_potions_used": combat.get("incumbent_potions_used"),
        "incumbent_potion_slots": combat.get("incumbent_potion_slots"),
        "incumbent_satisfies_satisfaction": combat.get("incumbent_satisfies_satisfaction"),
        "incumbent_ends_quality_refinement": combat.get("incumbent_ends_quality_refinement"),
        "last_status": combat.get("last_status"),
        "quantum_count": combat.get("quantum_count"),
        "remaining_generation_work": combat.get("remaining_generation_work"),
        "remaining_wall_ms": combat.get("remaining_wall_ms"),
        "resume_kind": combat.get("resume_kind"),
        "restart_count": combat.get("restart_count"),
    })
}

fn compact_live_combat_trace(diagnostic: &Value) -> Value {
    let root_actions = compact_root_action_report(diagnostic);
    json!({
        "node": diagnostic.get("node"),
        "root": diagnostic.get("root"),
        "search": diagnostic.get("search"),
        "root_policy_top": compact_policy_top(diagnostic.get("root_policy"), 8),
        "root_action_summary": {
            "totals": root_actions.get("totals"),
            "families_by_reachable_work": root_actions.get("families_by_reachable_work"),
        },
        "progress_trace": compact_replayed_trace(diagnostic.get("deepest_progress_trace")),
        "survival_trace": compact_replayed_trace(diagnostic.get("deepest_survival_trace")),
    })
}

fn compact_root_action_report(diagnostic: &Value) -> Value {
    let policy_actions = diagnostic
        .get("root_policy")
        .and_then(|policy| policy.get("actions"))
        .and_then(Value::as_array);
    let families = diagnostic
        .get("root_action_families")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let total_work = families
        .iter()
        .map(|family| value_u64(Some(family), "reachable_generation_work"))
        .sum::<u64>();
    let total_reachable_options = families
        .iter()
        .map(|family| value_u64(Some(family), "reachable_completed_turn_options"))
        .sum::<u64>();
    let mut compact_families = families
        .iter()
        .map(|family| {
            let action = family.get("action").and_then(Value::as_str);
            let policy = policy_actions
                .into_iter()
                .flatten()
                .find(|candidate| candidate.get("action").and_then(Value::as_str) == action);
            let work = value_u64(Some(family), "reachable_generation_work");
            json!({
                "action": family.get("action"),
                "prior_rank": policy.and_then(|value| value.get("rank")),
                "prior_probability": policy.and_then(|value| value.get("probability")),
                "root_turn_options": family.get("completed_root_turn_options"),
                "terminal_wins": family.get("terminal_wins"),
                "terminal_losses": family.get("terminal_losses"),
                "escapes": family.get("escapes"),
                "next_turn_successors": family.get("unique_next_turn_successors"),
                "retained_next_turn_successors": family.get("retained_next_turn_successors"),
                "reachable_exact_states": family.get("reachable_exact_states"),
                "reachable_retained_states": family.get("reachable_retained_states"),
                "reachable_work": work,
                "work_share_percent": if total_work == 0 { 0.0 } else { work as f64 * 100.0 / total_work as f64 },
                "reachable_turn_options": family.get("reachable_completed_turn_options"),
                "max_player_turn": family.get("max_player_turn"),
                "best_hp_at_max_turn": family.get("best_hp_at_max_turn"),
                "lowest_enemy_hp_at_max_turn": family.get("lowest_enemy_hp_at_max_turn"),
            })
        })
        .collect::<Vec<_>>();
    compact_families.sort_by_key(|family| {
        std::cmp::Reverse(
            family
                .get("reachable_work")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )
    });

    let node = diagnostic.get("node");
    let root = diagnostic.get("root");
    let search = diagnostic.get("search");
    json!({
        "node": {
            "node_id": node.and_then(|value| value.get("node_id")),
            "act": node.and_then(|value| value.get("act")),
            "floor": node.and_then(|value| value.get("floor")),
            "hp": node.and_then(|value| value.get("hp")),
            "max_hp": node.and_then(|value| value.get("max_hp")),
        },
        "root": {
            "turn": root.and_then(|value| value.get("turn")),
            "phase": root.and_then(|value| value.get("phase")),
            "player": root.and_then(|value| value.get("player")),
            "hand": root.and_then(|value| value.get("hand")),
            "monsters": root.and_then(|value| value.get("monsters")),
        },
        "search": {
            "generation_work": search.and_then(|value| value.get("generation_work")),
            "exact_states": search.and_then(|value| value.get("exact_states")),
            "completed_turn_options": search.and_then(|value| value.get("completed_turn_options")),
            "max_player_turn": search.and_then(|value| value.get("max_player_turn")),
            "incumbent_final_hp": search.and_then(|value| value.get("incumbent_final_hp")),
            "last_status": search.and_then(|value| value.get("last_status")),
        },
        "totals": {
            "root_action_families": compact_families.len(),
            "reachable_work": total_work,
            "reachable_turn_options": total_reachable_options,
            "reachability_is_nonexclusive": true,
        },
        "families_by_reachable_work": compact_families,
    })
}

fn compact_replayed_trace(trace: Option<&Value>) -> Value {
    let Some(trace) = trace.filter(|value| !value.is_null()) else {
        return Value::Null;
    };
    if let Some(same_as) = trace.get("same_as") {
        return json!({"same_as": same_as});
    }
    let turns = trace
        .get("turns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|turn| {
            let end = turn.get("end");
            json!({
                "turn": turn.get("turn"),
                "start_hp": turn.get("start_hp"),
                "actions": turn.get("actions"),
                "policy_top": compact_policy_top(turn.get("start_policy"), 5),
                "end": {
                    "hp": end.and_then(|value| value.get("hp")),
                    "block": end.and_then(|value| value.get("block")),
                    "energy": end.and_then(|value| value.get("energy")),
                    "hand": end.and_then(|value| value.get("hand")),
                    "monsters": end.and_then(|value| value.get("monsters")),
                    "player_powers": end.and_then(|value| value.get("player_powers")),
                    "piles": end.and_then(|value| value.get("piles")),
                },
            })
        })
        .collect::<Vec<_>>();
    json!({
        "action_count": trace.get("action_count"),
        "terminal": trace.get("terminal"),
        "turns": turns,
    })
}

fn compact_policy_top(policy: Option<&Value>, limit: usize) -> Vec<Value> {
    policy
        .and_then(|value| value.get("actions"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(limit)
        .map(|action| {
            json!({
                "rank": action.get("rank"),
                "action": action.get("action"),
                "probability": action.get("probability"),
            })
        })
        .collect()
}

fn card_value_label(card: &Value) -> String {
    let id = card
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("UnknownCard");
    let upgrades = card.get("upgrades").and_then(Value::as_u64).unwrap_or(0);
    if upgrades == 0 {
        id.to_string()
    } else {
        format!("{id}+{upgrades}")
    }
}

fn start_session(session: &str, workspace: &Path) -> Result<(), String> {
    validate_session_name(session)?;
    let workspace = resolve_owned_resident_workspace(workspace, &oracle_lab_state_root())?;
    let endpoint_path = session_endpoint_path(session)?;
    let mut executable = service_executable()?;
    let mut restarted_stale_runtime = false;
    let mut previous_executable = None;
    if let Some(endpoint) = active_endpoint(&endpoint_path) {
        ensure_endpoint_workspace(session, &endpoint, &workspace)?;
        if endpoint_uses_service_runtime(&endpoint, &executable) {
            return print_json(&json!({
                "session": session,
                "status": "already_running",
                "process_id": endpoint.process_id,
                "workspace": endpoint.workspace,
                "executable": endpoint.executable,
                "endpoint": endpoint_path,
            }));
        }

        restarted_stale_runtime = true;
        previous_executable = endpoint.executable.clone();
        shutdown_live_session(&endpoint_path).map_err(|error| {
            format!(
                "oracle session `{session}` uses a stale resident runtime and could not be saved before restart: {error}"
            )
        })?;
        // Shutdown pruning keeps the retiring image while the process exits,
        // so materialize the current immutable image again before launch.
        executable = service_executable()?;
    }

    let mut child =
        spawn_resident_service(&executable, &workspace, &endpoint_path, &repository_root())?;

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(endpoint) = active_endpoint(&endpoint_path) {
            ensure_endpoint_workspace(session, &endpoint, &workspace)?;
            return print_json(&json!({
                "session": session,
                "status": if restarted_stale_runtime {
                    "restarted_stale_runtime"
                } else {
                    "started"
                },
                "process_id": endpoint.process_id,
                "workspace": endpoint.workspace,
                "executable": endpoint.executable,
                "previous_executable": previous_executable,
                "endpoint": endpoint_path,
            }));
        }
        if let Some(status) = child.try_exit()? {
            return Err(format!(
                "resident oracle service exited before publishing its endpoint: {status}"
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "resident oracle service did not become ready within 10 seconds; process {}, endpoint {}",
                child.id(),
                endpoint_path.display()
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn active_endpoint(endpoint_path: &Path) -> Option<OracleAnalysisServiceEndpointV1> {
    let response = call_oracle_analysis_tcp_v1(endpoint_path, r#"{"command":"ping"}"#).ok()?;
    if !response.ok {
        return None;
    }
    let bytes = fs::read(endpoint_path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn endpoint_uses_service_runtime(
    endpoint: &OracleAnalysisServiceEndpointV1,
    current_runtime: &Path,
) -> bool {
    endpoint
        .executable
        .as_deref()
        .is_some_and(|active| paths_refer_to_same_file(active, current_runtime))
}

fn ensure_endpoint_workspace(
    session: &str,
    endpoint: &OracleAnalysisServiceEndpointV1,
    requested_workspace: &Path,
) -> Result<(), String> {
    let active_workspace = endpoint.workspace.canonicalize().map_err(|error| {
        format!("failed to resolve workspace for active oracle session `{session}`: {error}")
    })?;
    if active_workspace != requested_workspace {
        return Err(format!(
            "oracle session `{session}` already serves '{}', not requested '{}'",
            active_workspace.display(),
            requested_workspace.display()
        ));
    }
    Ok(())
}

fn delegate_heavy(arguments: Vec<OsString>) -> Result<(), String> {
    let executable = heavy_executable()?;
    let status = ProcessCommand::new(&executable)
        .arg("--canonical-oracle")
        .args(arguments)
        .status()
        .map_err(|error| {
            format!(
                "failed to start heavy oracle laboratory '{}': {error}",
                executable.display()
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("heavy oracle laboratory exited with {status}"))
    }
}

fn heavy_executable() -> Result<PathBuf, String> {
    let executable = repository_root()
        .join("target")
        .join("release")
        .join(if cfg!(windows) {
            "oracle_lab.exe"
        } else {
            "oracle_lab"
        });
    if !executable.is_file() {
        return Err(format!(
            "heavy oracle laboratory is missing at {}; run `.\\ol.ps1 build oracle`",
            executable.display()
        ));
    }
    ensure_heavy_artifact_fresh(&executable)
        .map_err(|error| format!("{error}; rebuild explicitly with `.\\ol.ps1 build oracle`"))?;
    Ok(executable)
}

fn service_executable() -> Result<PathBuf, String> {
    let canonical = repository_root()
        .join("target")
        .join("release")
        .join(if cfg!(windows) {
            "oracle_lab_service.exe"
        } else {
            "oracle_lab_service"
        });
    if !canonical.is_file() {
        return Err(format!(
            "resident oracle host is missing at {}; run `.\\ol-live.ps1 build host`",
            canonical.display()
        ));
    }
    ensure_service_artifact_fresh(&canonical)
        .map_err(|error| format!("{error}; rebuild explicitly with `.\\ol-live.ps1 build host`"))?;
    materialize_service_runtime(&canonical)
}

fn materialize_service_runtime(canonical: &Path) -> Result<PathBuf, String> {
    materialize_service_runtime_in(
        canonical,
        &oracle_lab_state_root().join("hosts"),
        &oracle_lab_state_root().join("sessions"),
    )
}

fn materialize_service_runtime_in(
    canonical: &Path,
    image_directory: &Path,
    session_directory: &Path,
) -> Result<PathBuf, String> {
    let metadata = fs::metadata(canonical).map_err(|error| {
        format!(
            "failed to inspect canonical resident host '{}': {error}",
            canonical.display()
        )
    })?;
    let modified = metadata
        .modified()
        .and_then(|time| {
            time.duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)
        })
        .map_err(|error| format!("failed to timestamp canonical resident host: {error}"))?;
    fs::create_dir_all(image_directory).map_err(|error| {
        format!(
            "failed to create resident-host image directory '{}': {error}",
            image_directory.display()
        )
    })?;
    let extension = canonical
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    let file_name = format!(
        "oracle_lab_service-{}-{}{}",
        metadata.len(),
        modified.as_nanos(),
        if extension.is_empty() {
            String::new()
        } else {
            format!(".{extension}")
        }
    );
    let runtime = image_directory.join(file_name);
    if !runtime.is_file() {
        let temporary = image_directory.join(format!(
            ".oracle_lab_service-{}-{}.tmp",
            std::process::id(),
            modified.as_nanos()
        ));
        fs::copy(canonical, &temporary).map_err(|error| {
            format!(
                "failed to materialize resident-host image '{}' from '{}': {error}",
                temporary.display(),
                canonical.display()
            )
        })?;
        if let Err(error) = fs::rename(&temporary, &runtime) {
            if !runtime.is_file() {
                let _ = fs::remove_file(&temporary);
                return Err(format!(
                    "failed to publish resident-host image '{}': {error}",
                    runtime.display()
                ));
            }
            let _ = fs::remove_file(&temporary);
        }
    }
    prune_unreferenced_service_images(image_directory, session_directory, &runtime);
    Ok(runtime)
}

fn prune_unreferenced_service_images(directory: &Path, session_directory: &Path, current: &Path) {
    let mut referenced = Vec::new();
    if let Ok(entries) = fs::read_dir(session_directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(endpoint) = active_endpoint(&path) {
                if let Some(executable) = endpoint.executable {
                    referenced.push(executable);
                }
            }
        }
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if paths_refer_to_same_file(&path, current)
            || referenced
                .iter()
                .any(|executable| paths_refer_to_same_file(&path, executable))
        {
            continue;
        }
        let _ = fs::remove_file(path);
    }
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    left == right
        || match (left.canonicalize(), right.canonicalize()) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
}

fn ensure_heavy_artifact_fresh(executable: &Path) -> Result<(), String> {
    ensure_artifact_fresh(
        executable,
        &repository_root(),
        CanonicalArtifact::OracleHost,
        "heavy oracle laboratory",
        ".\\ol.ps1 build oracle",
    )
}

fn ensure_service_artifact_fresh(executable: &Path) -> Result<(), String> {
    ensure_artifact_fresh(
        executable,
        &repository_root(),
        CanonicalArtifact::OracleHost,
        "resident oracle host",
        ".\\ol-live.ps1 build host",
    )
}

fn resolve_endpoint_argument(
    endpoint: Option<PathBuf>,
    session: Option<String>,
) -> Result<PathBuf, String> {
    match (endpoint, session) {
        (Some(endpoint), None) => Ok(endpoint),
        (None, Some(session)) => session_endpoint_path(&session),
        (None, None) => implicit_session_endpoint_path(&oracle_lab_state_root().join("sessions")),
        (Some(_), Some(_)) => {
            Err("oracle_lab live accepts only one of --session or --endpoint".to_string())
        }
    }
}

fn implicit_session_endpoint_path(sessions_root: &Path) -> Result<PathBuf, String> {
    let entries = match fs::read_dir(sessions_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(
                "oracle_lab live requires --session or --endpoint because no active resident session exists"
                    .to_string(),
            );
        }
        Err(error) => {
            return Err(format!(
                "failed to inspect resident sessions at {}: {error}",
                sessions_root.display()
            ));
        }
    };
    let endpoints = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".endpoint.json"))
        })
        .filter(|path| endpoint_process_matches_runtime(path))
        .collect::<Vec<_>>();
    select_implicit_session_endpoint(endpoints)
}

fn endpoint_process_matches_runtime(endpoint_path: &Path) -> bool {
    let endpoint = fs::read(endpoint_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<OracleAnalysisServiceEndpointV1>(&bytes).ok());
    endpoint.is_some_and(|endpoint| {
        oracle_lab_protocol::validate_endpoint(&endpoint).is_ok()
            && endpoint.executable.as_deref().is_some_and(|executable| {
                resident_process::process_matches_executable(endpoint.process_id, executable)
            })
    })
}

fn select_implicit_session_endpoint(mut endpoints: Vec<PathBuf>) -> Result<PathBuf, String> {
    endpoints.sort();
    match endpoints.as_slice() {
        [endpoint] => Ok(endpoint.clone()),
        [] => Err(
            "oracle_lab live requires --session or --endpoint because no active resident session exists"
                .to_string(),
        ),
        _ => Err(format!(
            "oracle_lab live requires --session or --endpoint because {} active resident sessions exist",
            endpoints.len()
        )),
    }
}

fn session_endpoint_path(session: &str) -> Result<PathBuf, String> {
    validate_session_name(session)?;
    Ok(oracle_lab_state_root()
        .join("sessions")
        .join(format!("{session}.endpoint.json")))
}

fn oracle_lab_state_root() -> PathBuf {
    repository_root().join(".oracle-lab")
}

fn validate_session_name(session: &str) -> Result<(), String> {
    if session.is_empty()
        || session.len() > 64
        || matches!(session, "." | "..")
        || !session.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        || !session
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
    {
        return Err(format!(
            "invalid oracle session name `{session}`; use 1-64 ASCII letters, digits, '.', '_' or '-', starting with a letter or digit"
        ));
    }
    Ok(())
}

fn validate_canonical_launch(canonical_oracle: bool) -> Result<(), String> {
    if !canonical_oracle {
        return Err(
            "oracle_lab client refuses direct execution; run `.\\ol-live.ps1 <command> ...`"
                .to_string(),
        );
    }
    let expected = repository_root()
        .join("target")
        .join("release")
        .join(if cfg!(windows) {
            "oracle_lab_client.exe"
        } else {
            "oracle_lab_client"
        });
    let current = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("failed to identify running oracle_lab client: {error}"))?;
    let expected = expected.canonicalize().map_err(|error| {
        format!(
            "canonical oracle_lab client is missing at {}: {error}; run `.\\ol-live.ps1 build client`",
            expected.display()
        )
    })?;
    if current != expected {
        return Err(format!(
            "oracle_lab client refuses non-canonical artifact {}; expected {}",
            current.display(),
            expected.display()
        ));
    }
    ensure_artifact_fresh(
        &current,
        &repository_root(),
        CanonicalArtifact::OracleClient,
        "lightweight oracle client",
        "cargo build --release -p oracle_lab_client --bin oracle_lab_client",
    )?;
    Ok(())
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn write_fresh_json<T: Serialize>(output: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create oracle artifact directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| {
            format!(
                "failed to create fresh oracle artifact '{}': {error}",
                output.display()
            )
        })?;
    if let Err(error) = serde_json::to_writer(&mut file, value)
        .map_err(|error| format!("failed to serialize oracle artifact: {error}"))
        .and_then(|_| {
            writeln!(&mut file)
                .map_err(|error| format!("failed to finish oracle artifact: {error}"))
        })
        .and_then(|_| {
            file.sync_all()
                .map_err(|error| format!("failed to sync oracle artifact: {error}"))
        })
    {
        let _ = fs::remove_file(output);
        return Err(error);
    }
    Ok(())
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, value)
        .map_err(|error| format!("failed to print JSON: {error}"))?;
    writeln!(stdout).map_err(|error| format!("failed to finish JSON output: {error}"))
}

fn print_combat_lab_semantic_action(result: Value) -> Result<(), String> {
    let applied = combat_lab_semantic_action_applied(&result)?;
    print_json(&result)?;
    if applied {
        Ok(())
    } else {
        Err(
            "combat line lab action was not executed; rerun with an explicit copy or target"
                .to_string(),
        )
    }
}

fn combat_lab_semantic_action_applied(result: &Value) -> Result<bool, String> {
    match result.get("status").and_then(Value::as_str) {
        Some("played" | "used") => Ok(true),
        Some("ambiguous_card" | "ambiguous_potion" | "ambiguous_target") => Ok(false),
        Some(status) => Err(format!(
            "combat line lab returned unknown semantic action status '{status}'"
        )),
        None => Err("combat line lab semantic action response has no status".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(any(windows, target_os = "linux", target_os = "android"))]
    #[test]
    fn implicit_live_session_requires_exactly_one_endpoint() {
        let current_executable = std::env::current_exe().expect("identify current test executable");
        assert!(resident_process::process_matches_executable(
            std::process::id(),
            &current_executable
        ));
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "sts-oracle-client-session-selection-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create session selection fixture");

        let missing = implicit_session_endpoint_path(&directory)
            .expect_err("missing session must remain explicit");
        assert!(missing.contains("no active resident session"));

        let ignored = directory.join("notes.json");
        fs::write(&ignored, b"{}").expect("write ignored non-endpoint file");
        let endpoint_fixture = |process_id, executable| OracleAnalysisServiceEndpointV1 {
            schema_name: oracle_lab_protocol::ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA.to_string(),
            schema_version: oracle_lab_protocol::ORACLE_ANALYSIS_SERVICE_ENDPOINT_SCHEMA_VERSION,
            address: "127.0.0.1:1".parse().expect("static loopback address"),
            auth_token: "fixture-token".to_string(),
            workspace: directory.join("fixture.workspace.json"),
            process_id,
            executable: Some(executable),
        };
        let stale_path = directory.join("stale.endpoint.json");
        fs::write(
            &stale_path,
            serde_json::to_vec(&endpoint_fixture(u32::MAX, current_executable.clone()))
                .expect("encode stale endpoint"),
        )
        .expect("write stale endpoint fixture");
        let stale = implicit_session_endpoint_path(&directory)
            .expect_err("stale endpoint must not become an implicit session");
        assert!(stale.contains("no active resident session"));

        let mismatched_path = directory.join("mismatched.endpoint.json");
        fs::write(
            &mismatched_path,
            serde_json::to_vec(&endpoint_fixture(
                std::process::id(),
                directory.join("different-runtime.exe"),
            ))
            .expect("encode mismatched endpoint"),
        )
        .expect("write mismatched endpoint fixture");
        let mismatched = implicit_session_endpoint_path(&directory)
            .expect_err("a reused pid with another image must not become an implicit session");
        assert!(mismatched.contains("no active resident session"));

        let only = directory.join("alpha.endpoint.json");
        fs::write(
            &only,
            serde_json::to_vec(&endpoint_fixture(
                std::process::id(),
                current_executable.clone(),
            ))
            .expect("encode live endpoint"),
        )
        .expect("write only live endpoint fixture");
        assert_eq!(
            implicit_session_endpoint_path(&directory).expect("select only live endpoint"),
            only
        );

        let second = directory.join("beta.endpoint.json");
        fs::write(
            &second,
            serde_json::to_vec(&endpoint_fixture(std::process::id(), current_executable))
                .expect("encode second live endpoint"),
        )
        .expect("write second live endpoint fixture");
        let ambiguous = implicit_session_endpoint_path(&directory)
            .expect_err("multiple live endpoints must remain explicit");
        assert!(ambiguous.contains("2 active resident sessions"));

        fs::remove_dir_all(&directory).expect("remove session selection fixture");
    }

    #[test]
    fn typed_mutations_hold_one_process_scoped_session_lease() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let endpoint = std::env::temp_dir().join(format!(
            "sts-oracle-client-lock-{}-{nonce}.endpoint.json",
            std::process::id()
        ));
        let lock_path = endpoint.with_extension("mutation.lock");

        let first = LiveMutationGuard::acquire(&endpoint).expect("first mutation lease");
        let error = LiveMutationGuard::acquire(&endpoint)
            .err()
            .expect("overlapping mutation must be rejected");
        assert!(error.contains("another typed mutating command"));
        drop(first);
        let second = LiveMutationGuard::acquire(&endpoint).expect("lease released with process");
        drop(second);
        let _ = fs::remove_file(lock_path);
    }

    #[test]
    fn typed_live_status_parses_without_loading_heavy_commands() {
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "status",
            "--limit",
            "3",
        ])
        .expect("parse typed live status");
        assert!(cli.canonical_oracle);
        assert!(matches!(
            cli.command,
            Command::Live {
                command: LiveCommand::Status { limit: 3, .. },
                ..
            }
        ));
    }

    #[test]
    fn typed_live_run_delegates_the_production_loop_to_the_service() {
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed009",
            "run",
            "--hallway-wall-ms",
            "5000",
            "--export-continuation",
            "victory.json",
        ])
        .expect("parse autonomous live run");
        assert!(matches!(
            cli.command,
            Command::Live {
                command: LiveCommand::Run {
                    hallway_wall_ms: 5000,
                    elite_wall_ms: 15000,
                    boss_wall_ms: 30000,
                    export_continuation: Some(path),
                    ..
                },
                ..
            } if path.as_os_str() == "victory.json"
        ));
    }

    #[test]
    fn typed_live_advance_exposes_bounded_incumbent_improvement() {
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed009",
            "advance",
            "--wall-ms",
            "5000",
            "--improve-incumbent",
        ])
        .expect("parse quality-improving live advance");
        assert!(matches!(
            cli.command,
            Command::Live {
                command: LiveCommand::Advance {
                    wall_ms: 5000,
                    improve_incumbent: true,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn typed_live_exposes_a_separate_current_stage_probe() {
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed009",
            "probe-combat",
            "--generation-work",
            "768",
            "--quantum-generation-work",
            "128",
            "--wall-ms",
            "250",
        ])
        .expect("parse current-stage live probe");
        assert!(matches!(
            cli.command,
            Command::Live {
                command: LiveCommand::ProbeCombat {
                    generation_work: 768,
                    quantum_generation_work: 128,
                    wall_ms: 250,
                },
                ..
            }
        ));
    }

    #[test]
    fn typed_live_verify_stays_on_the_resident_service_path() {
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed009",
            "verify",
            "--node",
            "204",
        ])
        .expect("parse resident witness verification");
        assert!(matches!(
            cli.command,
            Command::Live {
                command: LiveCommand::Verify { node: Some(204) },
                ..
            }
        ));
    }

    #[test]
    fn typed_live_combat_stays_on_the_resident_service_path() {
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "combat",
            "--node",
            "69",
        ])
        .expect("parse typed live combat");
        assert!(matches!(
            cli.command,
            Command::Live {
                command: LiveCommand::Combat {
                    node: Some(69),
                    max_engine_steps_per_transition: 512,
                },
                ..
            }
        ));
    }

    #[test]
    fn typed_live_trace_stays_on_the_resident_service_path() {
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "trace",
            "--node",
            "69",
        ])
        .expect("parse typed live trace");
        assert!(matches!(
            cli.command,
            Command::Live {
                command: LiveCommand::Trace {
                    node: Some(69),
                    max_engine_steps_per_transition: 512,
                },
                ..
            }
        ));
    }

    #[test]
    fn typed_live_root_actions_stays_on_the_resident_service_path() {
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "root-actions",
        ])
        .expect("parse typed live root actions");
        assert!(matches!(
            cli.command,
            Command::Live {
                command: LiveCommand::RootActions {
                    node: None,
                    max_engine_steps_per_transition: 512,
                },
                ..
            }
        ));
    }

    #[test]
    fn typed_live_combat_lab_uses_semantic_line_selectors() {
        let open = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "lab",
            "open",
            "--node",
            "36",
        ])
        .expect("parse incumbent-backed combat lab");
        assert!(matches!(
            open.command,
            Command::Live {
                command: LiveCommand::Lab {
                    command: LiveLabCommand::Open {
                        node: Some(36),
                        baseline: LiveLabBaseline::Incumbent,
                        ..
                    }
                },
                ..
            }
        ));

        let play = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "lab",
            "play",
            "--card",
            "PowerThrough",
        ])
        .expect("parse semantic card play");
        assert!(matches!(
            play.command,
            Command::Live {
                command: LiveCommand::Lab {
                    command: LiveLabCommand::Play {
                        card,
                        occurrence: None,
                        target: None,
                        ..
                    }
                },
                ..
            } if card == "PowerThrough"
        ));

        let goto = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "lab",
            "goto",
            "--turn",
            "3",
        ])
        .expect("parse current-line navigation");
        assert!(matches!(
            goto.command,
            Command::Live {
                command: LiveCommand::Lab {
                    command: LiveLabCommand::Goto {
                        line: LiveLabLine::Current,
                        turn: 3,
                        before: 0,
                        ..
                    }
                },
                ..
            }
        ));

        let potion = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "lab",
            "potion",
            "--potion",
            "FearPotion",
            "--copy",
            "1",
            "--target",
            "0",
        ])
        .expect("parse semantic potion use");
        assert!(matches!(
            potion.command,
            Command::Live {
                command: LiveCommand::Lab {
                    command: LiveLabCommand::Potion {
                        potion,
                        occurrence: Some(1),
                        target: Some(0),
                        ..
                    }
                },
                ..
            } if potion == "FearPotion"
        ));

        let selection = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "lab",
            "select",
            "--indices",
            "0,2,3",
        ])
        .expect("parse observation-local structured selection");
        assert!(matches!(
            selection.command,
            Command::Live {
                command: LiveCommand::Lab {
                    command: LiveLabCommand::Select { indices, .. }
                },
                ..
            } if indices == [0, 2, 3]
        ));

        let empty_selection = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "lab",
            "select",
            "--indices",
        ])
        .expect("parse an explicit empty structured selection");
        assert!(matches!(
            empty_selection.command,
            Command::Live {
                command: LiveCommand::Lab {
                    command: LiveLabCommand::Select { indices, .. }
                },
                ..
            } if indices.is_empty()
        ));

        let back = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "lab",
            "back",
        ])
        .expect("parse current-line back");
        assert!(matches!(
            back.command,
            Command::Live {
                command: LiveCommand::Lab {
                    command: LiveLabCommand::Back { .. }
                },
                ..
            }
        ));

        let keep = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "lab",
            "keep",
        ])
        .expect("parse keep current line");
        assert!(matches!(
            keep.command,
            Command::Live {
                command: LiveCommand::Lab {
                    command: LiveLabCommand::Keep
                },
                ..
            }
        ));

        let restore = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "live",
            "--session",
            "seed008",
            "lab",
            "restore",
        ])
        .expect("parse kept line restore");
        assert!(matches!(
            restore.command,
            Command::Live {
                command: LiveCommand::Lab {
                    command: LiveLabCommand::Restore { .. }
                },
                ..
            }
        ));
    }

    #[test]
    fn semantic_line_lab_ambiguity_is_not_an_applied_shell_action() {
        assert!(
            combat_lab_semantic_action_applied(&json!({"status": "played"}))
                .expect("played status")
        );
        assert!(
            combat_lab_semantic_action_applied(&json!({"status": "used"})).expect("used status")
        );
        for status in ["ambiguous_card", "ambiguous_potion", "ambiguous_target"] {
            assert!(
                !combat_lab_semantic_action_applied(&json!({"status": status}))
                    .expect("typed ambiguity status"),
                "{status} must not let a chained shell command continue"
            );
        }
    }

    #[test]
    fn typed_start_owns_named_session_launch() {
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "start",
            "--session",
            "seed008",
            "--workspace",
            "seed008.workspace.json",
        ])
        .expect("parse typed session start");
        assert!(matches!(
            cli.command,
            Command::Start { session, workspace }
                if session == "seed008" && workspace.as_os_str() == "seed008.workspace.json"
        ));
    }

    #[test]
    fn unknown_commands_do_not_silently_launch_the_heavy_tool() {
        assert!(
            Cli::try_parse_from(["oracle_lab_client", "--canonical-oracle", "combat-csae",])
                .is_err()
        );
        let cli = Cli::try_parse_from([
            "oracle_lab_client",
            "--canonical-oracle",
            "offline",
            "combat-case",
            "--case",
            "fight.json",
        ])
        .expect("parse deliberate heavyweight invocation");
        assert!(matches!(
            cli.command,
            Command::Offline { arguments }
                if arguments == ["combat-case", "--case", "fight.json"]
        ));
    }

    #[test]
    fn stale_heavy_artifact_is_rejected_before_execution() {
        let directory = std::env::temp_dir().join(format!(
            "oracle-lab-client-stale-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("create stale-artifact fixture");
        let executable = directory.join("oracle_lab.exe");
        let dependency = directory.join("search_source.rs");
        fs::write(&executable, b"old artifact").expect("write fake artifact");
        thread::sleep(Duration::from_millis(20));
        fs::write(&dependency, b"new source").expect("write newer dependency");
        fs::write(
            executable.with_extension("d"),
            format!("{}: {}\n", executable.display(), dependency.display()),
        )
        .expect("write fake depfile");

        let error = ensure_heavy_artifact_fresh(&executable)
            .expect_err("newer source must reject stale artifact");
        assert!(error.contains("refusing to run stale search code"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn resident_host_images_are_reused_per_build_and_pruned_when_unreferenced() {
        let directory = std::env::temp_dir().join(format!(
            "oracle-lab-client-host-image-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let canonical = directory.join("oracle_lab_service.exe");
        let images = directory.join("images");
        let sessions = directory.join("sessions");
        fs::create_dir_all(&sessions).expect("create session fixture");
        fs::write(&canonical, b"first build").expect("write canonical host");

        let first = materialize_service_runtime_in(&canonical, &images, &sessions)
            .expect("materialize first host image");
        let reused = materialize_service_runtime_in(&canonical, &images, &sessions)
            .expect("reuse first host image");
        assert_eq!(reused, first);
        assert_eq!(fs::read(&first).expect("read first image"), b"first build");

        thread::sleep(Duration::from_millis(20));
        fs::write(&canonical, b"second build").expect("replace canonical host");
        let second = materialize_service_runtime_in(&canonical, &images, &sessions)
            .expect("materialize second host image");
        assert_ne!(second, first);
        assert_eq!(
            fs::read(&second).expect("read second image"),
            b"second build"
        );
        assert!(!first.exists(), "unreferenced old image should be pruned");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn resident_reconnect_accepts_only_the_current_runtime_image() {
        let current = PathBuf::from("oracle_lab_service-current.exe");
        let endpoint = |executable| OracleAnalysisServiceEndpointV1 {
            schema_name: "OracleAnalysisServiceEndpoint".to_string(),
            schema_version: 1,
            address: "127.0.0.1:1".parse().expect("loopback address"),
            auth_token: "test".to_string(),
            workspace: PathBuf::from("workspace.json"),
            process_id: 7,
            executable,
        };

        assert!(endpoint_uses_service_runtime(
            &endpoint(Some(current.clone())),
            &current
        ));
        assert!(!endpoint_uses_service_runtime(
            &endpoint(Some(PathBuf::from("oracle_lab_service-old.exe"))),
            &current
        ));
        assert!(!endpoint_uses_service_runtime(&endpoint(None), &current));
    }

    #[test]
    fn compact_status_reports_truncation_instead_of_hiding_candidates() {
        let node = json!({
            "node_id": 7,
            "choices": [
                {"label": "a"},
                {"label": "b"},
                {"label": "c"}
            ],
            "children": [],
            "event": {"title": "test"},
            "reward": {
                "items": [{"Potion": {"potion_id": "CultistPotion"}}],
                "skippable": true
            },
            "encounter": {
                "turn": 0,
                "phase": "PlayerTurn",
                "energy": 3,
                "player_block": 0,
                "hand": [],
                "draw_pile_count": 5,
                "discard_pile_count": 0,
                "exhaust_pile_count": 0,
                "is_elite": true,
                "is_boss": false,
                "monsters": []
            },
            "combat": {
                "search_stage": 2,
                "max_potions_used": 1,
                "allowed_potion_slots": 2,
                "generation_work": 125,
                "historical_generation_work": 100,
                "current_search_generation_work": 25,
                "local_generation_work": 20,
                "discrepancy_generation_work": 5,
                "incumbent_discovery_source": "local_turn_graph",
                "incumbent_potions_used": 1,
                "incumbent_potion_slots": 2
            }
        });
        let compact = compact_live_node(&node, 2);
        assert_eq!(compact.get("choice_count"), Some(&json!(3)));
        assert_eq!(compact.get("choices_shown"), Some(&json!(2)));
        assert_eq!(compact.get("choices_truncated"), Some(&json!(true)));
        assert_eq!(compact.get("event"), node.get("event"));
        assert_eq!(compact.get("reward"), node.get("reward"));
        assert_eq!(compact["combat"]["search_stage"], 2);
        assert_eq!(compact["combat"]["max_potions_used"], 1);
        assert_eq!(compact["combat"]["allowed_potion_slots"], 2);
        assert_eq!(compact["combat"]["generation_work"], 125);
        assert_eq!(compact["combat"]["historical_generation_work"], 100);
        assert_eq!(compact["combat"]["current_search_generation_work"], 25);
        assert_eq!(compact["combat"]["local_generation_work"], 20);
        assert_eq!(compact["combat"]["discrepancy_generation_work"], 5);
        assert_eq!(
            compact["combat"]["incumbent_discovery_source"],
            "local_turn_graph"
        );
        assert_eq!(compact["combat"]["incumbent_potions_used"], 1);
        assert_eq!(compact["combat"]["incumbent_potion_slots"], 2);
        assert_eq!(compact["encounter"]["is_elite"], true);
        assert_eq!(compact["encounter"]["is_boss"], false);
    }

    #[test]
    fn compact_trace_keeps_chosen_actions_and_small_policy_context() {
        let diagnostic = json!({
            "node": {"node_id": 7},
            "root": {"hand": "Offering | Defend"},
            "root_policy": {
                "actions": [
                    {"rank": 1, "action": "use BlockPotion", "probability": 0.5},
                    {"rank": 2, "action": "play Offering", "probability": 0.25}
                ]
            },
            "deepest_progress_trace": {
                "action_count": 2,
                "terminal": "Unresolved",
                "turns": [{
                    "turn": 0,
                    "start_hp": 45,
                    "actions": ["use BlockPotion", "end turn"],
                    "start_policy": {"actions": [
                        {"rank": 1, "action": "use BlockPotion", "probability": 0.5}
                    ]},
                    "end": {
                        "hp": 45,
                        "block": 0,
                        "energy": 4,
                        "hand": "Defend",
                        "monsters": ["Donu 250/250"],
                        "player_powers": [],
                        "piles": "draw 1 / discard 1 / exhaust 0"
                    }
                }]
            },
            "deepest_survival_trace": {"same_as": "deepest_progress_trace"}
        });
        let compact = compact_live_combat_trace(&diagnostic);
        assert_eq!(compact["root_policy_top"][1]["action"], "play Offering");
        assert_eq!(
            compact["root_action_summary"]["families_by_reachable_work"],
            json!([])
        );
        assert_eq!(
            compact["progress_trace"]["turns"][0]["actions"],
            json!(["use BlockPotion", "end turn"])
        );
        assert_eq!(
            compact["survival_trace"]["same_as"],
            "deepest_progress_trace"
        );
    }

    #[test]
    fn root_action_report_attributes_work_without_copying_deep_search_state() {
        let diagnostic = json!({
            "node": {"node_id": 134, "act": 3, "floor": 48, "hp": 45, "max_hp": 46},
            "root": {"turn": 0, "phase": "PlayerTurn", "player": {"hp": 45}, "hand": "Offering", "monsters": []},
            "root_policy": {"actions": [
                {"rank": 1, "action": "use BlockPotion", "probability": 0.34},
                {"rank": 7, "action": "play Offering", "probability": 0.046}
            ]},
            "search": {
                "generation_work": 100,
                "exact_states": 20,
                "completed_turn_options": 30,
                "max_player_turn": 4,
                "deepest_progress": {"large": [1, 2, 3]}
            },
            "root_action_families": [
                {"action": "play Offering", "completed_root_turn_options": 2, "terminal_wins": 0, "terminal_losses": 0, "escapes": 0, "unique_next_turn_successors": 2, "retained_next_turn_successors": 2, "reachable_exact_states": 3, "reachable_retained_states": 2, "reachable_generation_work": 1, "reachable_completed_turn_options": 1, "max_player_turn": 2, "best_hp_at_max_turn": 19, "lowest_enemy_hp_at_max_turn": 476},
                {"action": "use BlockPotion", "completed_root_turn_options": 8, "terminal_wins": 0, "terminal_losses": 1, "escapes": 0, "unique_next_turn_successors": 8, "retained_next_turn_successors": 7, "reachable_exact_states": 9, "reachable_retained_states": 7, "reachable_generation_work": 9, "reachable_completed_turn_options": 6, "max_player_turn": 4, "best_hp_at_max_turn": 3, "lowest_enemy_hp_at_max_turn": 387}
            ]
        });

        let report = compact_root_action_report(&diagnostic);
        assert_eq!(
            report["families_by_reachable_work"][0]["action"],
            "use BlockPotion"
        );
        assert_eq!(report["families_by_reachable_work"][1]["prior_rank"], 7);
        assert_eq!(
            report["families_by_reachable_work"][1]["work_share_percent"],
            10.0
        );
        assert_eq!(
            report["families_by_reachable_work"][0]["terminal_losses"],
            1
        );
        assert_eq!(report["totals"]["reachability_is_nonexclusive"], true);
        assert!(report["search"].get("deepest_progress").is_none());
    }

    #[test]
    fn session_names_cannot_escape_the_session_directory() {
        assert!(validate_session_name("seed008").is_ok());
        assert!(validate_session_name("../seed008").is_err());
        assert!(validate_session_name("seed/008").is_err());
    }
}
