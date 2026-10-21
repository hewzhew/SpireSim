//! Lightweight, shared CLI contract for routine V2 oracle surfaces.
//!
//! The heavy oracle host consumes these exact clap types, while the tiny help
//! binary parses them without linking simulator, search, run-control, or
//! strategic owners.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Parser)]
#[command(
    name = "oracle_lab",
    about = "Routine exact-combat contract, artifact, and case commands"
)]
pub struct RoutineV2Cli {
    #[command(subcommand)]
    pub command: RoutineV2Command,
}

#[derive(Debug, Subcommand)]
pub enum RoutineV2Command {
    /// Run bounded, replay-verified experiments through the compact V2 protocol.
    Contract(ContractCommandArgs),
    /// Inspect or rerun one V2 experiment artifact without parsing its full report.
    Artifact(ArtifactCommandArgs),
    /// Import and query exact combat roots in the explicit V2 case catalog.
    Case(CaseCommandArgs),
}

#[derive(Debug, Args)]
pub struct ContractCommandArgs {
    #[command(subcommand)]
    pub command: ContractCommand,
}

#[derive(Debug, Subcommand)]
pub enum ContractCommand {
    /// Run one bounded exact-frontier contract from an exact combat root.
    Combat(CombatContractRunArgs),
}

#[derive(Debug, Args)]
pub struct ArtifactCommandArgs {
    #[command(subcommand)]
    pub command: ArtifactCommand,
}

#[derive(Debug, Subcommand)]
pub enum ArtifactCommand {
    /// Print only the stable compact result from a V2 artifact.
    Summary(ArtifactPathArgs),
    /// Collect stable compact results from several V2 artifacts.
    Summaries(ArtifactPathsArgs),
    /// Inspect compact exact-search service accounting without parsing report.json.
    Search(ArtifactSearchArgs),
    /// Replay and inspect the selected witness without reading the full report.
    Trace(ArtifactTraceArgs),
    /// Replay-compare the contract-aligned and local-HP terminal candidates.
    Compare(ArtifactPathArgs),
    /// Enumerate one exact complete-turn surface along a retained candidate.
    Turn(ArtifactTurnArgs),
    /// Continue the same contract after one replay-exact diagnostic prefix.
    Branch(ArtifactBranchArgs),
    /// Re-run the typed request stored in a V2 artifact.
    Rerun(ArtifactPathArgs),
}

#[derive(Debug, Args)]
pub struct ArtifactPathArgs {
    /// V2 artifact directory or its manifest.json.
    pub artifact: PathBuf,
}

#[derive(Debug, Args)]
pub struct ArtifactPathsArgs {
    /// V2 artifact directories or manifest.json files.
    #[arg(required = true, num_args = 1..)]
    pub artifacts: Vec<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ArtifactSearchArgs {
    /// V2 artifact directory or its manifest.json.
    pub artifact: PathBuf,
    /// Include a few highest-service exact-state samples per turn depth.
    #[arg(long)]
    pub states: bool,
    /// Query one retained exact state by its full hash or a unique prefix.
    #[arg(long, conflicts_with = "states")]
    pub state: Option<String>,
}

#[derive(Debug, Args)]
pub struct ArtifactTraceArgs {
    /// V2 artifact directory or its manifest.json.
    pub artifact: PathBuf,
    /// Select compact action ranks, checkpoints only, or complete policy evidence.
    #[arg(long, value_enum, default_value_t = ArtifactTraceDetail::Compact)]
    pub detail: ArtifactTraceDetail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ArtifactTraceDetail {
    Compact,
    Checkpoints,
    Policy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ArtifactCandidateRole {
    Contract,
    LocalHp,
}

#[derive(Debug, Args)]
pub struct ArtifactTurnArgs {
    /// V2 artifact directory or its manifest.json.
    pub artifact: PathBuf,
    #[command(flatten)]
    pub navigation: ArtifactNavigationArgs,
    /// Return only the reached plan matching this exact successor hash or prefix.
    #[arg(long)]
    pub successor_state: Option<String>,
    /// Enumerate exactly one additional turn from every reached candidate.
    #[arg(long, conflicts_with = "reached_only")]
    pub scan_next_terminal: bool,
    /// Return only the replay-checked reached state.
    #[arg(long, conflicts_with_all = ["successor_state", "scan_next_terminal"])]
    pub reached_only: bool,
    #[arg(long, default_value_t = 16)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct ArtifactNavigationArgs {
    /// Retained terminal candidate whose exact prefix owns the turn.
    #[arg(long, value_enum, default_value_t = ArtifactCandidateRole::Contract)]
    pub candidate: ArtifactCandidateRole,
    /// Exact player-turn number shown by artifact compare.
    #[arg(long)]
    pub turn: u32,
    /// Follow one displayed plan index. Repeat to walk several turns.
    #[arg(long, conflicts_with = "follow_state")]
    pub follow_plan: Vec<usize>,
    /// Follow one exact successor hash or unique prefix. Repeat to walk turns.
    #[arg(long, conflicts_with = "follow_plan")]
    pub follow_state: Vec<String>,
    #[arg(long, default_value_t = 1_024)]
    pub max_inner_nodes: usize,
    #[arg(long, default_value_t = 96)]
    pub max_end_states: usize,
    #[arg(long, default_value_t = 96)]
    pub per_bucket_limit: usize,
}

#[derive(Debug, Args)]
pub struct ArtifactBranchArgs {
    /// V2 artifact whose exact root and contract are inherited.
    pub artifact: PathBuf,
    #[command(flatten)]
    pub navigation: ArtifactNavigationArgs,
    /// Fresh suffix-search work. The inherited prefix is replayed, not charged.
    #[arg(long, default_value_t = 4_096)]
    pub generation_work: usize,
    #[arg(long, default_value_t = 2_000)]
    pub wall_ms: u64,
}

#[derive(Clone, Debug, Args, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatContractRunArgs {
    /// Exact CombatCase path. The case is admitted to the V2 catalog.
    #[arg(long, conflicts_with = "case_id", required_unless_present = "case_id")]
    pub case: Option<PathBuf>,
    /// Exact-root id or one unique prefix from case list.
    #[arg(long, conflicts_with = "case", required_unless_present = "case")]
    pub case_id: Option<String>,
    #[arg(long)]
    pub min_final_hp: Option<i32>,
    #[arg(long, default_value_t = 0)]
    pub max_potions_used: u32,
    #[arg(long)]
    pub require_recovered_stolen_gold: bool,
    #[arg(long, default_value_t = 4_096)]
    pub generation_work: usize,
    #[arg(long, default_value_t = 2_000)]
    pub wall_ms: u64,
}

#[derive(Debug, Args)]
pub struct CaseCommandArgs {
    #[command(subcommand)]
    pub command: CaseCommand,
}

#[derive(Debug, Subcommand)]
pub enum CaseCommand {
    /// Admit one exact CombatCase to the V2 catalog.
    Import(CaseImportArgs),
    /// Query only cases explicitly admitted to the V2 catalog.
    List(CaseListArgs),
}

#[derive(Debug, Args)]
pub struct CaseImportArgs {
    #[arg(long)]
    pub case: PathBuf,
}

#[derive(Debug, Args)]
pub struct CaseListArgs {
    #[arg(long)]
    pub seed: Option<u64>,
    #[arg(long)]
    pub act: Option<u8>,
    #[arg(long)]
    pub floor: Option<i32>,
    #[arg(long)]
    pub enemy: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}
