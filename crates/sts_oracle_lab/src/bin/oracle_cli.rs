use std::path::PathBuf;

use clap::{Parser, Subcommand};

use super::action_boundary_evidence::{
    ActionBoundaryEvidenceArgs, ActionBoundaryEvidenceBatchArgs,
};
use super::action_boundary_policy::ActionBoundaryPolicyArgs;
use super::action_boundary_root_race::ActionBoundaryRootRaceArgs;
use super::action_reanalysis_policy::ActionReanalysisPolicyArgs;
use super::action_reanalysis_queue::{ActionReanalysisBatchArgs, ActionReanalysisQueueArgs};
use super::action_successor_reanalysis::ActionSuccessorReanalysisArgs;
use super::boundary_successor_corpus::BoundarySuccessorCorpusArgs;
use super::combat_case_loadout_counterfactual::CombatCaseLoadoutCounterfactualArgs;
use super::combat_case_local_graph::CombatCaseLocalGraphArgs;
use super::combat_case_owner_parity::CombatCaseOwnerParityArgs;
use super::combat_evidence_audit::CombatEvidenceAuditArgs;
use super::combat_plan_diagnostics::{CombatCasePlanAnnotationsArgs, CombatCasePlanTraceArgs};
use super::combat_route_compare::CombatCaseRouteCompareArgs;
use super::combat_scratch_cli::CombatScratchCommand;
use super::depth_beam_audits::DepthBeamTurnAuditArgs;
use super::guidance_combination_audit::GuidanceCombinationAuditArgs;
use super::learning_root_commands::LearningRootCommand;
use super::oracle_budget_cli::BudgetArgs;
use super::oracle_case_catalog_v2::CaseCommandArgs;
use super::oracle_contract_v2::{ArtifactCommandArgs, ContractCommandArgs};
use super::oracle_seed_panel::OracleSeedPanelArgs;
use super::policy_discrepancy_search::CombatCasePolicyDiscrepancyArgs;
use super::potion_expenditure_audit::CombatCasePotionExpenditureAuditArgs;
use super::run_witness_suite::RunWitnessSuiteArgs;
use super::turn_audits::{TurnActionAuditArgs, TurnPlanAuditArgs};
use super::turn_membership_audit::TurnMembershipArgs;
use super::turn_quality_corridor::{TurnQualityCorridorArgs, TurnQualityFrontierArgs};
use super::workspace_drive::OracleDriveBoundaryArg;
use super::workspace_policy_audits::{
    CardRewardPathArgs, RoutePolicyAuditArgs, ShopPolicyAuditArgs,
};

#[derive(Debug, Parser)]
#[command(
    name = "oracle_lab",
    about = "Inspect and steer exact oracle-run variations without editing checkpoints"
)]
struct Cli {
    /// Proves that the canonical `.\scripts\ol.ps1` surface launched this process.
    /// Direct execution is intentionally rejected so that a stale or
    /// wrongly-profiled oracle laboratory cannot silently produce evidence.
    #[arg(long, hide = true, global = true)]
    canonical_oracle: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
pub(super) enum Command {
    /// Run bounded, replay-verified experiments through the compact V2 protocol.
    Contract(ContractCommandArgs),
    /// Inspect or rerun one V2 experiment artifact without parsing its full report.
    Artifact(ArtifactCommandArgs),
    /// Import and query exact combat roots in the explicit V2 case catalog.
    Case(CaseCommandArgs),
    /// Run consecutive A0 oracle seeds with durable per-seed reports and resumable stops.
    SeedPanel(OracleSeedPanelArgs),
    /// Start a new A0-style oracle analysis workspace at Neow.
    New {
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value_t = 0)]
        ascension: u8,
        #[arg(long)]
        workspace: PathBuf,
        /// Embed one validated guidance bundle into the workspace so every
        /// later process restore uses the same immutable search policy.
        #[arg(long)]
        combat_guidance_bundle: Option<PathBuf>,
        #[command(flatten)]
        budget: BudgetArgs,
    },
    /// Import an exact state from an oracle_run continuation.
    Import {
        #[arg(long)]
        continuation: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
        /// Import one retained frontier branch instead of the report-selected state.
        #[arg(long)]
        branch_id: Option<usize>,
        /// Embed one validated guidance bundle into the workspace so every
        /// later process restore uses the same immutable search policy.
        #[arg(long)]
        combat_guidance_bundle: Option<PathBuf>,
        #[command(flatten)]
        budget: BudgetArgs,
    },
    /// Export one exact analysis node as an oracle_run continuation.
    ExportContinuation {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Collect or export opaque production combat roots for online learning.
    #[command(subcommand)]
    LearningRoot(LearningRootCommand),
    /// Create a fresh one-node workspace from an exact committed node.
    /// The source workspace is never modified and the output must not exist.
    CompactWorkspace {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Rewrite a workspace with the current pooled checkpoint format while
    /// preserving its complete variation tree. The source is never modified.
    RepackWorkspace {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Recover one exact combat branch from a stale analysis workspace without
    /// restoring or validating unrelated frontier branches.
    RecoverCombatCase {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        branch: usize,
        #[arg(long)]
        output: PathBuf,
    },
    /// Replay the selected workspace node's entire committed journal from the
    /// canonical seed state and verify its exact final session fingerprint.
    VerifyRunWitness {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
    },
    /// Exact-replay a saved run up to one journal boundary and export that
    /// historical prefix as an importable continuation.
    ExportRunWitnessPrefix {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
        /// Stop immediately before this committed journal entry.
        #[arg(long)]
        journal_entry: usize,
        #[arg(long)]
        output: PathBuf,
    },
    /// Replay a versioned set of exact F0-to-Act-3 witnesses in one process.
    VerifyRunWitnessSuite {
        #[command(flatten)]
        args: RunWitnessSuiteArgs,
    },
    /// Replace one historical combat trajectory with another exact trajectory
    /// and emit a continuation only if the full run still replays exactly.
    SpliceCombatWitness {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: usize,
        #[arg(long)]
        journal_entry: usize,
        #[arg(long)]
        replacement_workspace: PathBuf,
        #[arg(long)]
        replacement_node: usize,
        #[arg(long)]
        output: PathBuf,
    },
    /// Export an exact historical combat root and its verified action witness
    /// from a complete run. The run journal is replayed to the requested
    /// entry; no continuation JSON editing is involved.
    ExportHistoricalCombatWitness {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
        #[arg(long)]
        journal_entry: usize,
        #[arg(long)]
        case_output: PathBuf,
        #[arg(long)]
        actions_output: PathBuf,
        /// Optionally emit the exact run prefix as an importable continuation
        /// at this combat root.
        #[arg(long)]
        continuation_output: Option<PathBuf>,
    },
    /// Annotate every finite atomic successor with read-only typed combat-plan
    /// facts. This command does not search, rank, prune, or modify a policy.
    CombatCasePlanAnnotations(CombatCasePlanAnnotationsArgs),
    /// Replay one exact action sequence and report typed combat-plan changes.
    /// This is a read-only trace: actions are supplied by the caller, never
    /// selected or ranked by this command.
    CombatCasePlanTrace(CombatCasePlanTraceArgs),
    /// Compare two caller-supplied exact routes from one unchanged combat
    /// root. The report aligns typed turn boundaries without ranking either
    /// route or inferring that one is a teacher label.
    CombatCaseRouteCompare(CombatCaseRouteCompareArgs),
    /// Build a fresh isolated combat counterfactual by replacing one exact
    /// master-deck card with Bronze Scales and its typed battle-start effect.
    CombatCaseLoadoutCounterfactual(CombatCaseLoadoutCounterfactualArgs),
    /// Batch-index exact combat artifacts and execute bounded typed transition queries.
    CombatEvidenceAudit(CombatEvidenceAuditArgs),
    /// Follow the action policy to terminal states and search complete
    /// trajectories by increasing weighted policy discrepancy.
    CombatCasePolicyDiscrepancy(CombatCasePolicyDiscrepancyArgs),
    /// Internal full-fidelity local-graph diagnostic surface.
    #[command(name = "combat-case-diagnostic", hide = true)]
    CombatCaseLocalGraph(CombatCaseLocalGraphArgs),
    /// Restore one case's captured production owner and serve one bounded in-memory attempt.
    CombatCaseOwnerParity(CombatCaseOwnerParityArgs),
    /// Compare isolated no-potion, per-potion, and bounded combination lanes
    /// from one unchanged exact combat root.
    CombatCasePotionExpenditureAudit(CombatCasePotionExpenditureAuditArgs),
    /// Distill one exact terminal witness into a semantic action-order artifact.
    BuildActionImitation {
        #[arg(long)]
        case: PathBuf,
        /// One or more consecutive exact action segments. Repeat the flag to
        /// compose a witness without rewriting JSON by hand.
        #[arg(long, required = true)]
        actions: Vec<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 250)]
        max_engine_steps_per_transition: usize,
    },
    /// Reanalyse every bounded legal action at one exact case root or witness state.
    ///
    /// The offline corpus keeps exact wins, exact refutations, terminal
    /// non-wins, and budget-unknown successors as distinct evidence kinds.
    BuildActionSuccessorCorpus {
        #[command(flatten)]
        args: ActionSuccessorReanalysisArgs,
    },
    /// Expand every legal root action only to the next player-turn boundary
    /// and score complete boundary surfaces with one frozen value artifact.
    BuildActionBoundaryEvidence {
        #[command(flatten)]
        args: ActionBoundaryEvidenceArgs,
    },
    /// Build next-boundary evidence for a ranked state queue in one process.
    BuildActionBoundaryEvidenceBatch {
        #[command(flatten)]
        args: ActionBoundaryEvidenceBatchArgs,
    },
    /// Train an action residual from complete next-boundary evidence while
    /// retaining positive base mass for every non-refuted unknown action.
    BuildActionBoundaryPolicy {
        #[command(flatten)]
        args: ActionBoundaryPolicyArgs,
    },
    /// Give every materialized first action equal initial service, then race
    /// exact next-turn generators under a frozen boundary-value teacher.
    /// This is a read-only shadow audit and cannot alter production guidance.
    AuditActionBoundaryRootRace {
        #[command(flatten)]
        args: ActionBoundaryRootRaceArgs,
    },
    /// Train a conservative residual policy from exact witnesses plus typed
    /// action-successor reanalysis. Budget-unknown actions retain base mass.
    BuildActionReanalysisPolicy {
        #[command(flatten)]
        args: ActionReanalysisPolicyArgs,
    },
    /// Rank exact witness states for bounded action-successor reanalysis.
    ///
    /// This is a read-only compute-order tool. It does not treat policy
    /// disagreement as negative evidence and cannot alter production policy.
    BuildActionReanalysisQueue {
        #[command(flatten)]
        args: ActionReanalysisQueueArgs,
    },
    /// Reanalyse the highest-priority states from a saved queue in one
    /// invocation, reusing the same verified manifest and policy identity.
    BuildActionReanalysisBatch {
        #[command(flatten)]
        args: ActionReanalysisBatchArgs,
    },
    /// Build offline complete-turn successor evidence from verified witnesses.
    ///
    /// This command never changes a production policy. Exact wins,
    /// exhaustive refutations, and budget-unknown observations remain
    /// distinct in the exported corpus.
    BuildBoundarySuccessorCorpus {
        #[command(flatten)]
        args: BoundarySuccessorCorpusArgs,
    },
    /// Run base, action-only, value-only, and combined guidance controls in
    /// one process against the same exact combat root and bounded allowance.
    AuditGuidanceCombination(GuidanceCombinationAuditArgs),
    /// Distill several exact terminal witnesses from one compact manifest.
    /// Relative case and action paths are resolved beside the manifest.
    BuildActionImitationCorpus {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Learn corrections to the mature combat action policy instead of
        /// replacing its action distribution. This mode is explicit because
        /// the resulting artifact must be paired with that same base policy.
        #[arg(long)]
        residual_over_existing_policy: bool,
        #[arg(long, default_value_t = 250)]
        max_engine_steps_per_transition: usize,
    },
    /// Evaluate an existing semantic action policy against a verified witness
    /// without retraining it or changing the artifact.
    AuditActionImitation {
        #[arg(long)]
        case: PathBuf,
        #[arg(long, required = true)]
        actions: Vec<PathBuf>,
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long, default_value_t = 250)]
        max_engine_steps_per_transition: usize,
    },
    /// Distill one exact terminal witness into a typed-feature prototype
    /// artifact for lab-only state-value inference.
    BuildValuePrototype {
        #[arg(long)]
        case: PathBuf,
        /// One or more consecutive exact action segments. Repeat the flag to
        /// compose a witness without rewriting JSON by hand.
        #[arg(long, required = true)]
        actions: Vec<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 250)]
        max_engine_steps_per_transition: usize,
    },
    /// Distill several exact terminal witnesses into one typed-feature value
    /// corpus. Uses the same compact manifest as action-imitation training.
    BuildValuePrototypeCorpus {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 250)]
        max_engine_steps_per_transition: usize,
    },
    /// Package already-built action and value artifacts into one immutable,
    /// runtime-compatible guidance unit.
    BuildCombatGuidanceBundle {
        #[arg(long)]
        action_imitation_artifact: PathBuf,
        #[arg(long)]
        value_prototype_artifact: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Check when one exact complete-turn action sequence is generated.
    TurnMembership(TurnMembershipArgs),
    /// Audit action-policy order and exact one-step successor guides at one turn prefix.
    TurnActionAudit(TurnActionAuditArgs),
    /// Audit the mature V2 bounded complete-turn proposer on one exact case.
    /// This is read-only evidence: it does not seed either production search.
    TurnPlanAudit(TurnPlanAuditArgs),
    /// Explore exact complete-turn successors under separate unresolved-boundary
    /// and post-victory HP floors. Any cap remains an explicit unknown.
    TurnQualityCorridor(TurnQualityCorridorArgs),
    /// Census a resumable exact turn frontier and optionally export a few
    /// machine-replayable diagnostic descendant cases.
    TurnQualityFrontier(TurnQualityFrontierArgs),
    /// Generate complete-turn proposals with an independent action-depth beam.
    /// Finished short turns never displace still-live longer prefixes.
    DepthBeamTurnAudit(DepthBeamTurnAuditArgs),
    /// View the current cursor or another exact analysis node.
    View {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
    },
    RoutePolicyAudit(RoutePolicyAuditArgs),
    /// Explain the exact shop owner's complete ranked evidence at one shop node.
    ShopPolicyAudit(ShopPolicyAuditArgs),
    CardRewardPath(CardRewardPathArgs),
    /// Show a compact actionable summary of the current or selected node.
    Status {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, default_value_t = 8)]
        limit: usize,
    },
    /// Choose one candidate by its owner rank at the current cursor.
    Choose {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        owner_rank: u64,
        #[arg(long)]
        node: Option<usize>,
    },
    /// Apply the owner's first choice for a bounded number of decisions.
    Owner {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=64))]
        steps: u8,
    },
    /// Alternate current typed owner decisions and ordinary combat advances
    /// in one resident process, saving after every mutation.
    Drive {
        #[arg(long)]
        workspace: PathBuf,
        /// Write the complete event ledger here. Stdout remains a compact
        /// execution receipt whether or not this is supplied.
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = 64, value_parser = clap::value_parser!(u16).range(1..=256))]
        max_steps: u16,
        #[arg(long, default_value_t = 32)]
        max_quanta: usize,
        #[arg(long, default_value_t = 50_000)]
        quantum_generation_work: usize,
        #[arg(long, default_value_t = 1_000)]
        quantum_ms: u64,
        #[arg(long, default_value_t = 60_000)]
        wall_ms: u64,
        /// Stop before executing the first step at this typed run boundary.
        #[arg(long, value_enum)]
        stop_at: Option<OracleDriveBoundaryArg>,
    },
    /// Print a compact tail of the committed run journal.
    Timeline {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, default_value_t = 30)]
        tail: usize,
    },
    /// Export the current or selected exact combat as a standalone case.
    ExportCombatCase {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Show the exact combat root, search progress, action families, and traces.
    Combat {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
        #[arg(long, default_value_t = 512)]
        max_engine_steps_per_transition: usize,
    },
    CombatScratch {
        #[arg(long)]
        workspace: PathBuf,
        #[command(subcommand)]
        command: CombatScratchCommand,
    },
    /// List every materialized variation and its edges.
    Tree {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Create a child variation from an exact choice reference returned by view.
    Try {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        choice_ref: String,
    },
    /// Move the analysis cursor to an existing node.
    Focus {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: usize,
    },
    /// Follow one already materialized child edge from the current cursor.
    Follow {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        edge: u64,
    },
    /// Return to the parent variation used to reach the current cursor.
    Back {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Mark the current variation as the preferred mainline without deleting siblings.
    Promote {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Run one bounded tactical attempt at the current cursor.
    ///
    /// Exact state, accounting, and any verified witness persist in the
    /// workspace. The in-memory tactical frontier does not survive a process
    /// exit, so repeated invocations restart search from the same combat root.
    Advance {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long, default_value_t = 32)]
        max_quanta: usize,
        #[arg(long, default_value_t = 50_000)]
        quantum_generation_work: usize,
        #[arg(long, default_value_t = 1_000)]
        quantum_ms: u64,
        #[arg(long)]
        wall_ms: Option<u64>,
        /// Continue past an insufficient first win until configured strategic
        /// quality is reached.
        #[arg(long)]
        improve_incumbent: bool,
        /// Print the full tactical progress report and node view. The default
        /// output is intentionally compact; detailed traces remain opt-in.
        #[arg(long)]
        detailed: bool,
    },
    /// Spend a fixed diagnostic work budget only in the current combat stage.
    /// This never promotes potion identity or materializes the incumbent.
    ProbeCombat {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long, default_value_t = 4_096)]
        generation_work: usize,
        #[arg(long, default_value_t = 256)]
        quantum_generation_work: usize,
        #[arg(long, default_value_t = 1_000)]
        wall_ms: u64,
        /// Print the full tactical progress report and node view.
        #[arg(long)]
        detailed: bool,
    },
    /// Accept the current combat's already verified incumbent.
    AcceptCombat {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Replay and accept an explicit exact combat witness at the cursor.
    AcceptCombatActions {
        #[arg(long)]
        workspace: PathBuf,
        /// One or more action-list files, composed in flag order.
        #[arg(long)]
        actions: Vec<PathBuf>,
    },
    /// Restart tactical search from the cursor's unchanged exact combat state.
    RestartCombat {
        #[arg(long)]
        workspace: PathBuf,
    },
    /// Print the strategic replay attached to a node.
    History {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        node: Option<usize>,
        /// Print the committed run journal, including history imported from an
        /// oracle_run continuation, instead of only oracle-lab variation edges.
        #[arg(long)]
        journal: bool,
    },
}

pub(super) fn parse() -> (bool, Command) {
    let cli = Cli::parse();
    (cli.canonical_oracle, cli.command)
}

#[cfg(test)]
mod tests;
