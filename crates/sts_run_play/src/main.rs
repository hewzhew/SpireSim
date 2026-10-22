use sts_presentation::Locale;
mod adapter_failure;
mod agent_combat_draft;
#[cfg(test)]
mod agent_combat_draft_tests;
mod agent_protocol;
mod combat_explain;
mod combat_query;
mod combat_reading;
mod combat_repair;
mod combat_trace;
mod compact;
mod counterfactual_branch;
mod decision;
#[cfg(test)]
mod decision_tests;
mod deck_lab;
mod driver;
mod event_scene;
mod history;
mod history_render;
mod history_state;
mod mechanics_info;
mod model;
mod paired_root_adapter;
mod proposal_audit;
mod public_decision;
mod public_map_reading;
mod reference_request;
mod render;
mod return_points;
mod reward_sequence;
mod run_selection;
#[cfg(test)]
#[path = "../../../tools/rust_build_source.rs"]
mod rust_build_source;
mod selector;
mod session_control;
mod storage;
mod victory_dossier;
mod victory_dossier_agent;
mod workspace_lifecycle;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use std::{io, io::BufReader, io::IsTerminal, io::Write as _};

use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use sts_oracle_learning::agent::task::{
    TaskBudgetV1, TaskDecisionSiteV1, TaskObjectiveV1, TaskPublicCandidateV1, TaskPublicChoiceV1,
    TaskPublicObservationV1, TaskStrategicActionV1,
};
use sts_oracle_learning::content::cards::CardId;
use sts_oracle_learning::content::potions::PotionId;
use sts_oracle_learning::content::relics::RelicId;
use sts_oracle_learning::eval::run_control::{
    CombatResearchPlanOptionsV1, CombatResearchProgressEventV1,
    CompleteTurnProposalBeamResearchOptionsV1, ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1,
    COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1, RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1,
};

use agent_protocol::{
    agent_game_facts_v1, anchor_combat_research_artifact_candidate_v1,
    capture_combat_research_artifact_v1, capture_combat_research_artifact_with_progress_v1,
    capture_exact_prefix_artifact_v1, combat_research_options_for_effort_v1,
    load_combat_research_artifact_v1, run_agent_jsonl_v1, run_terminal_agent_jsonl_v1,
    AgentCapabilitiesV1, AgentCombatResearchEffortV1, AgentGameFactEntityV1,
    AgentPermissionEnvelopeV1,
};
use compact::{render_comparison_v1, render_inventory_v1, render_preview_v1, render_status_v1};
use counterfactual_branch::{
    capture_counterfactual_branch_evidence_v1, write_counterfactual_branch_evidence_v1,
    DEFAULT_COUNTERFACTUAL_BRANCH_MAX_STEPS_V1,
};
use driver::{
    adopt_current_session_v1, back_v1, choose_v1, combat_history_v1, compact_active_v1,
    construction_history_v1, create_natural_session_v1, create_scratch_natural_session_v1,
    export_v1, fork_session_v1, import_session_v1, inspect_advice_at_v1, jump_v1,
    profile_storage_v1, promote_branch_v1, reroot_current_session_v1,
    reroot_session_from_capsule_v1, resolve_jump_node_from_parts_v1,
    snapshot_current_for_reroot_v1, RunPlayActiveCompactionProfileV1, RunPlayControllerV1,
    RunPlayOpenProfileV1, RunPlayRetainedSemanticPrefixStepV1, RunPlayStorageCensusV1,
    MAX_ANCHOR_LINE_STEPS,
};
use history::{journey_history_v1, RunPlayJourneyWindowV1};
use history_render::render_journey_history_v1;
use proposal_audit::render_proposal_audit_v1;
use reference_request::reference_request_v3;
use render::{
    render_advice_detail_v1, render_advice_v1, render_combat_history_v1,
    render_combat_research_attempt_v1, render_construction_history_detail_v1,
    render_construction_history_v1, render_history_v1, render_show_v1, render_strategic_history_v1,
    render_terminals_v1,
};
use selector::{parse_action_line_v1, parse_action_selector_v1, semantic_action_for_choice_v1};
use session_control::{
    build_overview_v1, clear_focus_v1, describe_session_v1, load_focus_v1, load_handoff_summary_v1,
    load_session_control_v1, migrate_legacy_focus_v1, open_handoff_v2, park_session_v1,
    render_control_v1, render_focus_v1, render_handoff_summary_v1, render_overview_v1,
    resolve_focus_session_path_v1, resolve_handoff_v2, set_focus_v1, unpark_session_v1,
    RunPlaySessionPurposeV1,
};
use storage::RunPlayPathsV1;
use victory_dossier::victory_dossier_v1;
use workspace_lifecycle::{
    execute_workspace_gc_plan_v1, inspect_workspace_gc_v1, prepare_disposable_target_v1,
    register_disposable_fork_v1, render_workspace_gc_summary_v1, write_workspace_gc_plan_v1,
};

#[derive(Parser)]
#[command(
    name = "run_play",
    about = "Persistent exact full-run research workbench",
    long_about = "A typed, persistent full-run workbench over current-simulator natural roots or explicitly imported exact run archives. It shows real public decisions, applies one controller-selected choice, permits automatic combat continuation only, yields at the next strategic boundary, and supports exact navigation without shell-side JSON parsing."
)]
struct Cli {
    /// Create a private typed failure report for a supervising adapter caller.
    #[arg(long, global = true, hide = true)]
    adapter_failure_report: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum RunCharacterArgV1 {
    Ironclad,
    Silent,
    Defect,
    Watcher,
}

impl RunCharacterArgV1 {
    fn player_class(self) -> &'static str {
        match self {
            Self::Ironclad => "Ironclad",
            Self::Silent => "Silent",
            Self::Defect => "Defect",
            Self::Watcher => "Watcher",
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum NeowOfferArgV1 {
    Rich,
    Mini,
}

impl From<NeowOfferArgV1> for sts_oracle_learning::content::events::neow::NeowEligibilityV1 {
    fn from(value: NeowOfferArgV1) -> Self {
        match value {
            NeowOfferArgV1::Rich => Self::Rich,
            NeowOfferArgV1::Mini => Self::Mini,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum RunGoalArgV1 {
    ActThreeVictory,
    HeartVictory,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum RunPlaySessionPurposeArgV1 {
    Unknown,
    Play,
    Lab,
    Probe,
    Archive,
}

impl From<RunPlaySessionPurposeArgV1> for RunPlaySessionPurposeV1 {
    fn from(value: RunPlaySessionPurposeArgV1) -> Self {
        match value {
            RunPlaySessionPurposeArgV1::Unknown => Self::Unknown,
            RunPlaySessionPurposeArgV1::Play => Self::Play,
            RunPlaySessionPurposeArgV1::Lab => Self::Lab,
            RunPlaySessionPurposeArgV1::Probe => Self::Probe,
            RunPlaySessionPurposeArgV1::Archive => Self::Archive,
        }
    }
}

#[derive(Subcommand)]
enum FocusCommandV1 {
    /// Show the latest typed focus event.
    Show {
        #[arg(long, default_value = ".oracle-lab/run-play-focus")]
        focus_root: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Select one durable session for overview convenience only.
    Set {
        #[arg(long)]
        session: PathBuf,
        #[arg(long, default_value = ".oracle-lab/run-play-focus")]
        focus_root: PathBuf,
    },
    /// Append an explicit empty focus event.
    Clear {
        #[arg(long, default_value = ".oracle-lab/run-play-focus")]
        focus_root: PathBuf,
    },
    /// Import the old raw active-session pointer once without deleting it.
    Migrate {
        #[arg(long, default_value = ".oracle-lab/active-run-session.txt")]
        legacy_pointer: PathBuf,
        #[arg(long, default_value = ".oracle-lab/run-play-focus")]
        focus_root: PathBuf,
    },
}

#[derive(Subcommand)]
enum SessionCommandV1 {
    /// Show one session's control-plane state.
    Show {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Assign a catalog purpose and optional human label without changing the game head.
    Describe {
        #[arg(long)]
        session: PathBuf,
        #[arg(long, value_enum)]
        purpose: RunPlaySessionPurposeArgV1,
        #[arg(long)]
        label: Option<String>,
    },
    /// Freeze every head-changing entry point at the current exact head.
    Park {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        reason: String,
    },
    /// Explicitly reopen a parked session for head-changing commands.
    Unpark {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        reason: String,
    },
}

#[derive(Subcommand)]
enum HandoffCommandV2 {
    /// Open or supersede one exact-head, repository-bound recovery note.
    Open {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        repository_commit: String,
        #[arg(long)]
        pause_reason: String,
        #[arg(long)]
        do_not_act: String,
        #[arg(long = "open", action = clap::ArgAction::Append)]
        open_issues: Vec<String>,
        #[arg(long = "completed", action = clap::ArgAction::Append)]
        completed_issues: Vec<String>,
    },
    /// Resolve the currently open V2 handoff without changing the game head.
    Resolve {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        repository_commit: String,
        #[arg(long)]
        resolution: String,
    },
    /// Inspect handoff history; legacy V1 receipts are historical only.
    Show {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        repository_commit: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum MapQueryV1 {
    /// Expand all published map nodes (grouped connected routes are the default).
    Full,
    /// Compare the maximum number of elites on published ordinary paths.
    Elites,
    /// Inspect the next ordinary branches from one published map coordinate.
    Route { x: i32, y: i32 },
}

impl From<RunGoalArgV1> for TaskObjectiveV1 {
    fn from(value: RunGoalArgV1) -> Self {
        match value {
            RunGoalArgV1::ActThreeVictory => Self::ActThreeVictory,
            RunGoalArgV1::HeartVictory => Self::HeartVictory,
        }
    }
}

const MAX_RETAINED_MIRROR_PLAN_BYTES_V1: u64 = 16 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct RetainedMirrorPlanReplaySourceV1 {
    schema: String,
    run_start: RetainedMirrorRunStartV1,
    steps: Vec<RetainedMirrorStepV1>,
}

#[derive(Debug, Deserialize)]
struct RetainedMirrorRunStartV1 {
    seed: String,
    character: String,
    ascension: u8,
    content_profile: String,
    objective: String,
}

#[derive(Debug, Deserialize)]
struct RetainedMirrorStepV1 {
    depth: usize,
    site: TaskDecisionSiteV1,
    before: model::RunPlayPublicScalarsV1,
    semantic_action: String,
    selected_candidate: serde_json::Value,
}

struct LoadedRetainedMirrorPrefixV1 {
    schema: String,
    seed: u64,
    ascension: u8,
    objective: TaskObjectiveV1,
    omitted_legacy_intro_steps: usize,
    steps: Vec<RunPlayRetainedSemanticPrefixStepV1>,
}

#[derive(Subcommand)]
enum Command {
    /// Create a natural session from the current simulator.
    New {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        seed: u64,
        #[arg(long, value_enum, default_value_t = RunCharacterArgV1::Ironclad)]
        character: RunCharacterArgV1,
        #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=20))]
        ascension: u8,
        /// Source-audited Neow eligibility captured before this run starts.
        #[arg(long, value_enum, default_value_t = NeowOfferArgV1::Rich)]
        neow_offer: NeowOfferArgV1,
        /// Complete-run objective. Heart victory enables keys and Act 4 from the natural root.
        #[arg(long, value_enum, default_value_t = RunGoalArgV1::ActThreeVictory)]
        goal: RunGoalArgV1,
        #[arg(long, default_value_t = 4096)]
        max_world_transitions: u64,
        #[arg(long, default_value_t = 0)]
        max_reloads: u32,
        /// Set this new durable run as the convenience focus after creation.
        #[arg(long)]
        focus: bool,
    },
    /// Create a fresh current-simulator session by replaying a typed prefix from a retained Java mirror plan.
    ReplayMirrorPrefix {
        /// Historical or current versioned mirror plan; its typed actions are never shell-expanded.
        #[arg(long)]
        plan: PathBuf,
        /// Fresh target session. A pre-existing path is rejected.
        #[arg(long)]
        session: PathBuf,
        /// Inclusive retained source depth to replay.
        #[arg(long)]
        through_depth: usize,
        /// Source-audited Neow eligibility for the natural Rust root.
        #[arg(long, value_enum, default_value_t = NeowOfferArgV1::Rich)]
        neow_offer: NeowOfferArgV1,
        #[arg(long, default_value_t = 4096)]
        max_world_transitions: u64,
        #[arg(long, default_value_t = 0)]
        max_reloads: u32,
    },
    /// Import a session from a validated historical archive/checkpoint pair.
    Import {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        archive: PathBuf,
        #[arg(long)]
        checkpoints: PathBuf,
        #[arg(long)]
        node: Option<String>,
        #[arg(long, default_value_t = 4096)]
        max_world_transitions: u64,
        #[arg(long, default_value_t = 0)]
        max_reloads: u32,
    },
    /// Read-only verify and capture an exact state/RNG, including from a source-drifted session when reconstruction succeeds.
    SnapshotCurrent {
        #[arg(long)]
        session: PathBuf,
        /// Fresh public-observation-schema-independent reroot capsule path.
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify an exact-state capsule and create a fresh current-build root; the source trajectory is not certified.
    Reroot {
        #[arg(long)]
        snapshot: PathBuf,
        /// Fresh target session directory.
        #[arg(long)]
        target: PathBuf,
    },
    /// Verify the current exact state and create a fresh root without writing an intermediate capsule.
    RerootCurrent {
        #[arg(long)]
        session: Option<PathBuf>,
        /// Retained nNODE or source:hREV/target:hREV to restore instead of the old head.
        #[arg(long)]
        at: Option<String>,
        /// Fresh target session directory; omitted to generate one beside the source.
        #[arg(long)]
        target: Option<PathBuf>,
        /// Focus the new session. Implicit when the source is the focused session.
        #[arg(long)]
        focus: bool,
    },
    /// Keep only the selected exact game state in a disposable play session.
    AdoptCurrent {
        #[arg(long)]
        session: Option<PathBuf>,
        #[arg(long)]
        target: PathBuf,
        #[arg(long)]
        focus: bool,
    },
    /// Create an independent writable session from a retained source decision without moving the source head.
    Fork {
        /// Existing source session.
        #[arg(long)]
        session: PathBuf,
        /// Fresh target session directory.
        #[arg(long)]
        target: PathBuf,
        /// Optional retained nNODE or explicit source:hREV/target:hREV boundary; defaults to the source head.
        #[arg(long)]
        at: Option<String>,
        /// Register this fork as disposable below the declared managed root.
        #[arg(long)]
        disposable: bool,
        /// Root whose direct registered children may be considered by workspace GC.
        #[arg(long, default_value = ".oracle-lab/run-play-tmp")]
        managed_root: PathBuf,
        /// Human recovery cue retained in the disposable workspace manifest.
        #[arg(long, default_value = "after probe review")]
        expected_cleanup_phase: String,
    },
    /// Transactionally replay one verified source branch onto a divergent target session.
    PromoteBranch {
        /// Session whose active suffix is the desired branch.
        #[arg(long)]
        source: PathBuf,
        /// Existing session that should receive the verified suffix.
        #[arg(long)]
        target: PathBuf,
    },
    /// Retain one parked disposable probe as read-only root-bound reachability evidence.
    CounterfactualBranch {
        /// Registered disposable probe whose completed active suffix should be captured.
        #[arg(long)]
        session: PathBuf,
        /// Root whose direct registered children are eligible probes.
        #[arg(long, default_value = ".oracle-lab/run-play-tmp")]
        managed_root: PathBuf,
        /// Fresh JSON evidence path outside the disposable workspace.
        #[arg(long)]
        output: PathBuf,
        /// Maximum retained typed decisions in the probe suffix.
        #[arg(long, default_value_t = DEFAULT_COUNTERFACTUAL_BRANCH_MAX_STEPS_V1)]
        max_steps: usize,
    },
    /// Show the current exact public state and legal candidates.
    Show {
        #[arg(long)]
        session: PathBuf,
        /// Export the validated strategic public decision once for offline selective reading.
        #[arg(long)]
        public_output: Option<PathBuf>,
        /// Print the current public decision as JSON instead of the human
        /// debug rendering, so tooling consumes a typed contract rather
        /// than parsing the presentation format.
        #[arg(long)]
        decision_json: bool,
    },
    /// Read the published map without the full diagnostic decision dump.
    Map {
        #[arg(long, global = true)]
        session: Option<PathBuf>,
        #[command(subcommand)]
        query: Option<MapQueryV1>,
    },
    /// Check current resources with the live combat solver against diagnostic encounters, including Heart.
    DeckLab {
        #[arg(long)]
        session: Option<PathBuf>,
        /// Fresh JSON artifact; terminal output stays short.
        #[arg(long)]
        output: Option<PathBuf>,
        /// List the same-act battery and the explicit counterfactual Heart test.
        #[arg(long)]
        list: bool,
        /// Repeat each selected encounter under reproducible diagnostic RNG.
        #[arg(long, default_value_t = 1)]
        scenarios: usize,
        /// Select same-act encounters or TheHeart; repeat to select several.
        #[arg(long)]
        encounter: Vec<String>,
        #[arg(long, default_value_t = 200_000)]
        work: usize,
        #[arg(long, default_value_t = 8000)]
        ms: u64,
        #[arg(long)]
        full_hp: bool,
        /// Include opening examples and per-turn traces in this diagnostic run.
        #[arg(long)]
        details: bool,
        #[arg(long, default_value_t = 0)]
        max_potion_uses: u32,
        /// Privately apply current preparation selectors (separated by /) before testing resources.
        #[arg(long, conflicts_with = "list")]
        after: Option<String>,
    },
    /// Inspect power or enemy mechanics using definitions and current public state only.
    Info {
        #[arg(long)]
        session: PathBuf,
        #[arg(long, default_value = "en")]
        locale: Locale,
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
    },
    /// Show executable choices at the current decision.
    Options {
        #[arg(long)]
        session: PathBuf,
    },
    /// Show a compact typed resume context for the current controller head.
    Status {
        #[arg(long)]
        session: PathBuf,
    },
    /// Read the complete current master deck and relic list on demand.
    Inventory {
        #[arg(long)]
        session: PathBuf,
        /// Include printed card rules and upgrade-sensitive costs.
        #[arg(long)]
        effects: bool,
        #[arg(long, default_value = "en")]
        locale: Locale,
    },
    /// Read exact persistent run-level RNG streams for scoped Java/Rust parity work.
    ParityRng {
        #[arg(long)]
        session: PathBuf,
        /// Inspect a retained decision without moving the durable head.
        #[arg(long)]
        at: Option<String>,
    },
    /// Read exact-state and future-continuation identities without mutating the run.
    StateFingerprint {
        #[arg(long)]
        session: PathBuf,
        /// Inspect a retained decision without moving the durable head.
        #[arg(long)]
        at: Option<String>,
    },
    /// Reject a PairedRoot source whose Rust simulator/RNG semantics are not current.
    #[command(name = "paired-root-prepare-preflight", hide = true)]
    PairedRootPreparePreflight {
        #[arg(long)]
        session: PathBuf,
    },
    /// Build one parentless natural Neow root after a fresh-JVM reconstruction proof.
    #[command(name = "paired-root-bootstrap-natural-neow", hide = true)]
    PairedRootBootstrapNaturalNeow {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u8).range(0..=20))]
        ascension: u8,
        #[arg(long)]
        first_java_frame: PathBuf,
        #[arg(long)]
        first_java_talk_receipt: PathBuf,
        #[arg(long)]
        java_recovery_storage: PathBuf,
        #[arg(long)]
        native_autosave_sha256: String,
        #[arg(long)]
        java_live_output: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        attempt: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Build one fresh constructed combat root from a strictly validated scenario recipe.
    #[command(name = "paired-root-bootstrap-constructed-combat", hide = true)]
    PairedRootBootstrapConstructedCombat {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        scenario_plan: PathBuf,
        #[arg(long)]
        scenario_report: PathBuf,
        #[arg(long)]
        java_live_output: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        attempt: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Prepare one probe-only PairedRoot from a retained Rust decision and a freshly restored Java frame.
    #[command(name = "paired-root-prepare", hide = true)]
    PairedRootPrepare {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        at: String,
        #[arg(long)]
        java_checkpoint: PathBuf,
        #[arg(long)]
        java_plan: PathBuf,
        #[arg(long)]
        java_live_output: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        attempt: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Derive stable typed action handles from one immutable PairedRoot.
    #[command(name = "paired-root-observe-actions", hide = true)]
    PairedRootObserveActions {
        #[arg(long)]
        root: String,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Run the routine combat-search owner from one immutable PairedRoot.
    #[command(name = "paired-root-search", hide = true)]
    PairedRootSearch {
        #[arg(long)]
        root: String,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Preview one stable typed action handle inside the live PairedRoot restoration.
    #[command(name = "paired-root-preview-action", hide = true)]
    PairedRootPreviewAction {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        root: String,
        #[arg(long)]
        activation: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        java_live_output: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        attempt: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Materialize the nearest native Java recovery bundle for one target root.
    #[command(name = "paired-root-materialize-native", hide = true)]
    PairedRootMaterializeNative {
        #[arg(long)]
        target_root: String,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Activate, recover, and preview one target-root action in one workbench process.
    #[command(name = "paired-root-resume-preview", hide = true)]
    PairedRootResumePreview {
        #[arg(long)]
        target_root: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        run_root: PathBuf,
        #[arg(long)]
        java_live_output: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        recovery_attempt: PathBuf,
        #[arg(long)]
        preview_attempt: PathBuf,
        #[arg(long)]
        activation_report: PathBuf,
        #[arg(long)]
        recovery_report: PathBuf,
        #[arg(long)]
        preview_report: PathBuf,
    },
    /// Activate, recover, and preview one typed witness in one live transaction.
    #[command(name = "paired-root-resume-witness", hide = true)]
    PairedRootResumeWitness {
        #[arg(long)]
        target_root: String,
        #[arg(long)]
        witness_plan: PathBuf,
        #[arg(long)]
        run_root: PathBuf,
        #[arg(long)]
        java_live_output: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        recovery_attempt: PathBuf,
        #[arg(long)]
        transaction_attempt: PathBuf,
        #[arg(long)]
        activation_report: PathBuf,
        #[arg(long)]
        recovery_report: PathBuf,
        #[arg(long)]
        transaction_report: PathBuf,
    },
    /// Preview one typed witness directly from an already parked live activation.
    #[command(name = "paired-root-preview-witness", hide = true)]
    PairedRootPreviewWitness {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        root: String,
        #[arg(long)]
        activation: String,
        #[arg(long)]
        witness_plan: PathBuf,
        #[arg(long)]
        java_live_output: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        attempt: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Prove receipt-only compare-and-swap on one disposable PairedHead.
    #[command(name = "paired-root-cas-proof", hide = true)]
    PairedRootCasProof {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        head: String,
        #[arg(long)]
        root: String,
        #[arg(long, action = clap::ArgAction::Append, required = true)]
        receipt: Vec<String>,
        #[arg(long)]
        report: PathBuf,
    },
    /// Initialize one named PairedHead without executing a game transition.
    #[command(name = "paired-root-head-init", hide = true)]
    PairedRootHeadInit {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        head: String,
        #[arg(long)]
        root: String,
        #[arg(long)]
        report: PathBuf,
    },
    /// Commit one already verified receipt to a named PairedHead.
    #[command(name = "paired-root-head-commit", hide = true)]
    PairedRootHeadCommit {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        head: String,
        #[arg(long)]
        receipt: String,
        #[arg(long)]
        report: PathBuf,
    },
    /// Select one named PairedHead through an append-only selector.
    #[command(name = "paired-root-head-select", hide = true)]
    PairedRootHeadSelect {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        selector: String,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        head: String,
        #[arg(long)]
        report: PathBuf,
    },
    /// Resolve the current live head behind an append-only selector.
    #[command(name = "paired-root-head-resolve", hide = true)]
    PairedRootHeadResolve {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        selector: String,
        #[arg(long)]
        report: PathBuf,
    },
    /// Commit one receipt to the PairedHead currently selected by a selector.
    #[command(name = "paired-root-selected-head-commit", hide = true)]
    PairedRootSelectedHeadCommit {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        selector: String,
        #[arg(long)]
        receipt: String,
        #[arg(long)]
        report: PathBuf,
    },
    /// Show repository/build identity, typed focus, the focused decision, and recent sessions.
    Overview {
        #[arg(long)]
        repository_commit: String,
        #[arg(long)]
        repository_dirty: bool,
        #[arg(long)]
        workbench_sha256: String,
        #[arg(long, default_value = ".oracle-lab/run-play")]
        run_root: PathBuf,
        #[arg(long, default_value = ".oracle-lab/run-play-focus")]
        focus_root: PathBuf,
        #[arg(long, default_value_t = 12)]
        max_sessions: usize,
        #[arg(long)]
        json: bool,
    },
    /// Manage the project-local convenience focus; it never grants mutation authority.
    Focus {
        #[command(subcommand)]
        action: FocusCommandV1,
    },
    /// Manage session purpose and enforceable open/parked lifecycle.
    Session {
        #[command(subcommand)]
        action: SessionCommandV1,
    },
    /// Manage repository- and exact-head-scoped recovery notes.
    Handoff {
        #[command(subcommand)]
        action: HandoffCommandV2,
    },
    /// Export the current strategic decision as a typed reference-service request.
    ReferenceRequest {
        #[arg(long)]
        session: PathBuf,
    },
    /// Read the exact strategic owner's projected public recommendation.
    Advice {
        #[arg(long)]
        session: PathBuf,
        /// Inspect an already retained nNODE or explicit source:hREV/target:hREV boundary without moving the durable head.
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        detail: bool,
    },
    /// Run the configured combat-research portfolio read-only at this root.
    CombatResearch {
        #[arg(long)]
        session: PathBuf,
        #[arg(long, value_enum, default_value_t = AgentCombatResearchEffortV1::Standard)]
        effort: AgentCombatResearchEffortV1,
        /// Expand internal lanes, dominated candidates, and complete action lines.
        #[arg(long)]
        detail: bool,
        /// Save the same research frontier as a fresh root-bound typed JSON artifact.
        #[arg(long)]
        artifact: Option<PathBuf>,
    },
    /// Run one read-only composable typed trajectory query at the current combat root.
    CombatQuery {
        #[arg(long)]
        session: PathBuf,
        /// Require selecting this canonical card ID from a Discovery choice. Repeatable.
        #[arg(long, value_parser = parse_construction_card_v1, action = clap::ArgAction::Append)]
        eventually_generated_card: Vec<CardId>,
        /// Require this card's typed lethal payoff. Currently Feed is supported. Repeatable.
        #[arg(long, value_parser = parse_construction_card_v1, action = clap::ArgAction::Append)]
        eventually_lethal_payoff: Vec<CardId>,
        #[arg(long, default_value_t = 500_000)]
        max_generation_work: usize,
        #[arg(long, default_value_t = 1_000_000)]
        max_selections: usize,
        #[arg(long, default_value_t = 30_000)]
        wall_ms: u64,
        #[arg(long, default_value_t = 32)]
        max_turn_depth: usize,
        /// Maximum potion uses/discards in the complete combat. Omit for unbounded.
        #[arg(long)]
        max_potions_used: Option<u32>,
        /// Restrict potion actions to these zero-based slots. Repeatable; omit for all slots.
        #[arg(long, action = clap::ArgAction::Append)]
        allowed_potion_slot: Vec<usize>,
        #[arg(long)]
        allow_potion_discard: bool,
        /// Save the complete typed result as a fresh JSON artifact.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Transactionally anchor the selected witness from a root-bound V2 combat-query artifact.
    AnchorCombatQuery {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
        /// Zero-based exact witness frontier member. Omit for the local-HP-selected witness.
        #[arg(long)]
        candidate: Option<usize>,
    },
    /// Replay selected artifact candidates at their retained historical root and report typed combat deltas.
    CombatTrace {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
        /// Combat-research global candidate index; use 0 for a combat-query selected witness. Repeatable for research.
        #[arg(long, action = clap::ArgAction::Append, required = true)]
        candidate: Vec<usize>,
        /// Save the complete typed trace as a fresh JSON artifact.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Reopen a replayed candidate after a fixed prefix and search explicitly for a healthy Feed payoff.
    CombatFeedRepair {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        candidate: usize,
        #[arg(long)]
        fixed_prefix_actions: usize,
        #[arg(long)]
        minimum_final_hp: i32,
        #[arg(long)]
        minimum_final_max_hp: i32,
        #[arg(long, default_value_t = 250_000)]
        max_generation_work: usize,
        #[arg(long, default_value_t = 1_000_000)]
        max_selections: usize,
        #[arg(long, default_value_t = 5_000)]
        wall_ms: u64,
        /// Save the complete typed repair result as a fresh JSON artifact.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Detect nonfatal Feed use in strong resource-frontier witnesses and open bounded suffix challengers.
    CombatFeedChallenge {
        #[arg(long)]
        session: PathBuf,
        /// Replay-proved combat-research or V2 combat-query source artifact.
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long, default_value_t = 4)]
        max_challengers: usize,
        #[arg(long, default_value_t = 500_000)]
        max_generation_work_per_challenger: usize,
        #[arg(long, default_value_t = 1_000_000)]
        max_selections_per_challenger: usize,
        #[arg(long, default_value_t = 30_000)]
        wall_ms_per_challenger: u64,
        /// Save the complete typed challenge result as a fresh JSON artifact.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Promote one globally non-dominated Feed challenger at its unchanged exact root.
    CombatFeedAnchor {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
        /// Zero-based index from the V2 artifact's global_resource_frontier.
        #[arg(long)]
        frontier: usize,
    },
    /// Challenge every turn-advancing HP-loss cutpoint without exceeding the source potion spend.
    CombatDamageChallenge {
        #[arg(long)]
        session: PathBuf,
        /// Replay-proved combat-research or V2 combat-query source artifact.
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long, default_value_t = 8)]
        max_challengers: usize,
        #[arg(long, default_value_t = 500_000)]
        max_generation_work_per_challenger: usize,
        #[arg(long, default_value_t = 1_000_000)]
        max_selections_per_challenger: usize,
        #[arg(long, default_value_t = 30_000)]
        wall_ms_per_challenger: u64,
        /// Save the complete typed challenge result as a fresh JSON artifact.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Promote one globally non-dominated damage challenger at its unchanged exact root.
    CombatDamageAnchor {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
        /// Zero-based index from the artifact's global_resource_frontier.
        #[arg(long)]
        frontier: usize,
    },
    /// Export the current exact combat root as a fresh production case.
    ExportCombatCase {
        #[arg(long)]
        session: PathBuf,
        /// Inspect a retained exact boundary without moving the session head.
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Audit whether one semantic complete-turn line survives proposal generation.
    ProposalAudit {
        #[arg(long)]
        session: PathBuf,
        /// For example play:Armaments/select:DarkEmbrace/play:DarkEmbrace+/end.
        line: String,
    },
    /// Compare read-only semantic lines from one hot root.
    Compare {
        #[arg(long)]
        session: PathBuf,
        /// Inspect an already retained nNODE or explicit source:hREV/target:hREV boundary without moving the durable head.
        #[arg(long, conflicts_with = "artifact")]
        at: Option<String>,
        /// Include every intermediate step and its timing.
        #[arg(long)]
        detail: bool,
        /// Save the exact semantic lines and public endpoints as a fresh root-bound artifact.
        #[arg(long)]
        artifact: Option<PathBuf>,
        #[arg(required = true, num_args = 1..)]
        lines: Vec<String>,
    },
    /// Execute one exact public line transactionally and anchor its final decision.
    AnchorLine {
        #[arg(long)]
        session: PathBuf,
        /// For example play:Defend/play:Whirlwind/end.
        line: String,
    },
    /// Select one replay-verified complete combat plan by producer and anchor it transactionally.
    AnchorCombatPlan {
        #[arg(long)]
        session: PathBuf,
        /// Replay a fresh root-bound typed artifact instead of repeating its search.
        #[arg(long)]
        artifact: Option<PathBuf>,
        /// Global candidate index printed by `combat-research`; required when a producer has more than one candidate.
        #[arg(long)]
        candidate: Option<usize>,
        #[arg(long, value_enum, default_value_t = AgentCombatResearchEffortV1::Standard)]
        effort: AgentCombatResearchEffortV1,
        /// One of resident, proposal, or atomic.
        producer: String,
    },
    /// Low-level protocol diagnostic using one explicit ordinal path.
    #[command(name = "protocol-choose", hide = true)]
    ProtocolChoose {
        #[arg(long)]
        session: PathBuf,
        #[arg(required = true, num_args = 1..)]
        ordinals: Vec<usize>,
        #[arg(long)]
        max_world_transitions: Option<u64>,
        #[arg(long)]
        max_reloads: Option<u32>,
        /// Yield after exactly this public action, including inside combat.
        #[arg(long)]
        manual: bool,
    },
    /// Rehearse one semantic action line on a private exact fork without publishing it.
    Preview {
        #[arg(long)]
        session: PathBuf,
        /// Preview from one exact retained public node without moving the durable head.
        /// Head revisions are audit metadata and are deliberately not accepted here.
        #[arg(long)]
        at: Option<String>,
        /// Include every intermediate public successor and timing.
        #[arg(long)]
        detail: bool,
        /// Save this exact semantic line and public endpoint as a fresh root-bound artifact.
        #[arg(long)]
        artifact: Option<PathBuf>,
        /// The same positional semantic actions accepted by `act`.
        #[arg(required = true, num_args = 1..)]
        actions: Vec<String>,
    },
    /// Resolve and execute one semantic action line, then yield at its public successor.
    Act {
        #[arg(long)]
        session: PathBuf,
        /// Branch directly from one exact retained public node. This replaces a
        /// separate jump followed by act, and publishes nothing when resolution fails.
        /// Head revisions are audit metadata and are deliberately not accepted here.
        #[arg(long)]
        at: Option<String>,
        /// One action executes immediately. Two or more are privately
        /// validated and published as one all-or-nothing transaction.
        /// For example: play:Corruption play:Impervious play:Strike@0 end.
        #[arg(required = true, num_args = 1..)]
        actions: Vec<String>,
        /// After this action, let the built-in actor continue through combat to the next strategic decision.
        #[arg(long)]
        auto_combat: bool,
    },
    /// Undo the latest public action. This is the routine correction surface;
    /// use jump only for deliberate historical navigation.
    Undo {
        #[arg(long)]
        session: PathBuf,
    },
    /// Move the controller head to an earlier recoverable public decision.
    Back {
        #[arg(long)]
        session: PathBuf,
        #[arg(long, conflicts_with = "floor")]
        steps: Option<usize>,
        /// Return to the retained end of this floor.
        #[arg(long)]
        floor: Option<i32>,
        /// Return to a named decision kept in this scratch session.
        #[arg(long, conflicts_with_all = ["steps", "floor"])]
        mark: Option<String>,
        /// List retained floor targets and named alternatives without moving.
        #[arg(long, conflicts_with_all = ["steps", "floor", "mark"])]
        list: bool,
        /// Return structured retained targets instead of text.
        #[arg(long, requires = "list", conflicts_with = "request")]
        json: bool,
        /// Restore the exact target from a source-bound return request JSON.
        #[arg(long, conflicts_with_all = ["steps", "floor", "mark", "list", "json"])]
        request: Option<PathBuf>,
    },
    /// Keep or list named alternatives inside one scratch session.
    Mark {
        #[arg(long)]
        session: PathBuf,
        /// Save the current exact decision under this name.
        #[arg(long, conflicts_with = "drop")]
        name: Option<String>,
        /// Release a previously kept alternative.
        #[arg(long)]
        drop: Option<String>,
    },
    /// Render the active archive path as exact boundaries or human-scale episodes.
    History {
        #[arg(long)]
        session: PathBuf,
        #[arg(long, conflicts_with = "journey")]
        limit: Option<usize>,
        /// Omit combat actions and apply the limit to strategic decisions only.
        #[arg(long, conflicts_with_all = ["combat_detail", "journey"])]
        strategic_only: bool,
        /// Include the public hand, energy, intent, powers, and enemy state for combat actions.
        #[arg(long, conflicts_with_all = ["strategic_only", "journey"])]
        combat_detail: bool,
        /// Start combat detail at this retained action source (nNODE or source:hREVISION).
        #[arg(long, requires = "combat_detail", conflicts_with = "journey")]
        at: Option<String>,
        /// Render the active path as human-scale run episodes backed by the complete archive surface.
        #[arg(long)]
        journey: bool,
        /// Show the first N journey episodes.
        #[arg(
            long,
            requires = "journey",
            conflicts_with_all = ["last", "after_depth", "count"],
            value_parser = parse_positive_usize
        )]
        first: Option<usize>,
        /// Show the last N journey episodes.
        #[arg(
            long,
            requires = "journey",
            conflicts_with_all = ["first", "after_depth", "count"],
            value_parser = parse_positive_usize
        )]
        last: Option<usize>,
        /// Continue journey pagination after the episode ending at this archive depth.
        #[arg(
            long,
            requires = "journey",
            conflicts_with_all = ["first", "last"]
        )]
        after_depth: Option<usize>,
        /// Number of episodes to show after --after-depth (default 8).
        #[arg(
            long,
            requires = "after_depth",
            value_parser = parse_positive_usize
        )]
        count: Option<usize>,
        /// Expand complete strategic candidate surfaces and the starting deck.
        #[arg(long, requires = "journey")]
        detail: bool,
    },
    /// Show every non-route, non-combat construction boundary on the current path.
    ConstructionHistory {
        #[arg(long)]
        session: PathBuf,
        /// Optional strategic site such as card_reward, shop, campfire, or event.
        #[arg(long)]
        site: Option<String>,
        /// Only retain boundaries whose candidate surface explicitly mentions this canonical CardId.
        #[arg(long, conflicts_with = "added_card")]
        candidate_card: Option<String>,
        /// Only retain boundaries whose observed successor adds this canonical CardId to the deck.
        #[arg(long, conflicts_with = "candidate_card")]
        added_card: Option<String>,
        /// Include routine open/proceed and fixed gold-claim boundaries.
        #[arg(long)]
        all_boundaries: bool,
        /// Expand every retained boundary with deck, relics, potions, and all candidates.
        #[arg(long)]
        detail: bool,
        /// Emit the typed projection as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Write a typed, reproducible ledger for a successful active run path.
    VictoryDossier {
        #[arg(long)]
        session: PathBuf,
        /// Fresh create-new JSON output path.
        #[arg(long)]
        output: PathBuf,
        /// Explicit agent transcript file or flat directory; repeat as needed.
        #[arg(long = "transcript-source")]
        transcript_sources: Vec<PathBuf>,
    },
    /// List retained terminal outcomes and their exact resumable source node.
    Terminals {
        #[arg(long)]
        session: PathBuf,
        #[arg(long, default_value_t = 40)]
        limit: usize,
    },
    /// Move to an existing decision by nNODE or an explicit source:hN/target:hN event boundary.
    Jump {
        #[arg(long)]
        session: PathBuf,
        selector: String,
    },
    /// Keep exact archive/checkpoints hot for repeated manual play.
    Shell {
        #[arg(long)]
        session: PathBuf,
    },
    /// Keep exact state hot behind a machine-only JSONL protocol.
    AgentShell {
        #[arg(long)]
        session: PathBuf,
        /// Optional external knowledge-pack identifier; this process does not load it.
        #[arg(long)]
        knowledge_pack: Option<String>,
        /// Public-information manual play: no search, private probes or hidden piles.
        #[arg(long)]
        manual: bool,
        #[arg(long, default_value = "en")]
        locale: Locale,
    },
    /// Print simulator-owned typed facts for exact gameplay entity IDs.
    GameFacts {
        #[arg(
            long = "card",
            action = clap::ArgAction::Append,
            required_unless_present_any = ["relics", "potions"],
            value_parser = parse_construction_card_v1
        )]
        cards: Vec<CardId>,
        #[arg(
            long = "relic",
            action = clap::ArgAction::Append,
            required_unless_present_any = ["cards", "potions"],
            value_parser = parse_game_fact_relic_v1
        )]
        relics: Vec<RelicId>,
        #[arg(
            long = "potion",
            action = clap::ArgAction::Append,
            required_unless_present_any = ["cards", "relics"],
            value_parser = parse_game_fact_potion_v1
        )]
        potions: Vec<PotionId>,
    },
    /// Rebase the selected prefix into a durable exact recovery point.
    Checkpoint {
        #[arg(long)]
        session: PathBuf,
    },
    /// Retain only the selected recovery path and its exact checkpoints.
    CompactActive {
        #[arg(long)]
        session: PathBuf,
        /// Atomically publish the destructive projection; omit for dry-run.
        #[arg(long)]
        commit: bool,
    },
    /// Measure the read-only phases of opening the current exact decision.
    ProfileOpen {
        #[arg(long)]
        session: PathBuf,
    },
    /// Attribute persisted workspace size without opening the exact simulator.
    ProfileStorage {
        #[arg(long)]
        session: PathBuf,
    },
    /// Inspect, plan, or execute cleanup of registered disposable forks.
    WorkspaceGc {
        #[command(subcommand)]
        action: WorkspaceGcCommandV1,
    },
    /// Export the current head's active recovery path to fresh artifacts.
    Export {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        archive_output: PathBuf,
        #[arg(long)]
        checkpoints_output: PathBuf,
    },
}

#[derive(Subcommand)]
enum WorkspaceGcCommandV1 {
    /// Print the compact cleanup summary used by run-play resume.
    Summary {
        #[arg(long, default_value = ".oracle-lab/run-play-tmp")]
        managed_root: PathBuf,
        #[arg(long, default_value = ".oracle-lab/run-play-focus")]
        focus_root: PathBuf,
    },
    /// Write a create-new plan containing the exact currently eligible targets.
    Plan {
        #[arg(long, default_value = ".oracle-lab/run-play-tmp")]
        managed_root: PathBuf,
        #[arg(long, default_value = ".oracle-lab/run-play-focus")]
        focus_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Revalidate and delete only the exact targets named by a prior plan.
    Execute {
        #[arg(long, default_value = ".oracle-lab/run-play-tmp")]
        managed_root: PathBuf,
        #[arg(long, default_value = ".oracle-lab/run-play-focus")]
        focus_root: PathBuf,
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        receipt: PathBuf,
    },
}

const RUN_PLAY_WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;

fn main() -> ExitCode {
    let worker = std::thread::Builder::new()
        .name("run-play".to_string())
        .stack_size(RUN_PLAY_WORKER_STACK_BYTES)
        .spawn(run_main);
    match worker {
        Ok(worker) => match worker.join() {
            Ok(exit) => exit,
            Err(_) => {
                eprintln!("error: run-play worker panicked");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("error: failed to start run-play worker: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_main() -> ExitCode {
    let cli = Cli::parse();
    let adapter_failure_report = cli.adapter_failure_report.clone();
    match ensure_current_run_play_is_fresh_v1().and_then(|()| run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(path) = adapter_failure_report.as_deref() {
                if let Err(report_error) = adapter_failure::write_failure_report_if_absent(
                    path,
                    &adapter_failure::RunPlayAdapterFailureV1::from_error_detail(error.clone()),
                ) {
                    eprintln!("error: {report_error}");
                }
            }
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("expected a positive integer, observed {value:?}"))?;
    if parsed == 0 {
        return Err("expected a positive integer, observed 0".to_string());
    }
    Ok(parsed)
}

fn require_exact_node_selector_v1(selector: &str, command: &str) -> Result<(), String> {
    if selector.starts_with('n') && selector.len() > 1 {
        return Ok(());
    }
    Err(format!(
        "run-play {command} --at requires an exact nNODE selector; head revisions such as h108 are audit metadata, not decision identity"
    ))
}

fn parse_construction_site_v1(value: &str) -> Result<TaskDecisionSiteV1, String> {
    match value {
        "card_reward" => Ok(TaskDecisionSiteV1::CardReward),
        "neow" => Ok(TaskDecisionSiteV1::Neow),
        "event" => Ok(TaskDecisionSiteV1::Event),
        "shop" => Ok(TaskDecisionSiteV1::Shop),
        "campfire" => Ok(TaskDecisionSiteV1::Campfire),
        "boss_relic" => Ok(TaskDecisionSiteV1::BossRelic),
        "reward" => Ok(TaskDecisionSiteV1::Reward),
        "run_choice" => Ok(TaskDecisionSiteV1::RunChoice),
        "treasure" => Ok(TaskDecisionSiteV1::Treasure),
        "combat" | "map" => Err(format!(
            "construction history excludes {value}; use `history` or `history --combat-detail`"
        )),
        _ => Err(format!(
            "unknown construction site {value:?}; expected card_reward, neow, event, shop, campfire, boss_relic, reward, run_choice, or treasure"
        )),
    }
}

fn parse_construction_card_v1(value: &str) -> Result<CardId, String> {
    serde_json::from_value(serde_json::Value::String(value.to_string())).map_err(|_| {
        format!(
            "unknown canonical card id {value:?}; use the simulator identifier such as IronWave"
        )
    })
}

fn parse_game_fact_relic_v1(value: &str) -> Result<RelicId, String> {
    serde_json::from_value(serde_json::Value::String(value.to_string())).map_err(|_| {
        format!(
            "unknown canonical relic id {value:?}; use the simulator identifier such as OrangePellets"
        )
    })
}

fn parse_game_fact_potion_v1(value: &str) -> Result<PotionId, String> {
    serde_json::from_value(serde_json::Value::String(value.to_string())).map_err(|_| {
        format!(
            "unknown canonical potion id {value:?}; use the simulator identifier such as EssenceOfSteel"
        )
    })
}

fn potion_slot_mask_v1(slots: &[usize]) -> Result<Option<u64>, String> {
    if slots.is_empty() {
        return Ok(None);
    }
    let mut mask = 0_u64;
    for slot in slots {
        let bit = u32::try_from(*slot)
            .ok()
            .and_then(|slot| 1_u64.checked_shl(slot))
            .ok_or_else(|| {
                format!(
                    "potion slot {slot} is not representable; expected a zero-based slot below {}",
                    u64::BITS
                )
            })?;
        mask |= bit;
    }
    Ok(Some(mask))
}

fn ensure_current_run_play_is_fresh_v1() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to locate run-play executable: {error}"))?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "run-play manifest is outside the workspace".to_string())?;
    oracle_artifact_contract::ensure_depfile_artifact_fresh(
        &executable,
        repository,
        &[
            repository.join("Cargo.toml"),
            repository.join("Cargo.lock"),
            repository.join(".cargo/config.toml"),
            repository.join("crates/sts_run_play/Cargo.toml"),
        ],
        "run-play workbench",
        r"pwsh -File .\scripts\run-play.ps1 build",
    )
}

fn load_retained_mirror_prefix_v1(
    path: &Path,
    through_depth: usize,
) -> Result<LoadedRetainedMirrorPrefixV1, String> {
    if through_depth == 0 {
        return Err("replay-mirror-prefix through-depth must be positive".to_string());
    }
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "failed to inspect retained mirror plan {}: {error}",
            path.display()
        )
    })?;
    if metadata.len() > MAX_RETAINED_MIRROR_PLAN_BYTES_V1 {
        return Err(format!(
            "retained mirror plan {} exceeds the {} byte read limit",
            path.display(),
            MAX_RETAINED_MIRROR_PLAN_BYTES_V1
        ));
    }
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read retained mirror plan {}: {error}",
            path.display()
        )
    })?;
    let source: RetainedMirrorPlanReplaySourceV1 =
        serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "retained mirror plan {} does not satisfy the typed replay projection: {error}",
                path.display()
            )
        })?;
    if !matches!(
        source.schema.as_str(),
        "sts-java-parity-mirror-plan-v2"
            | "sts-java-parity-mirror-plan-v3"
            | "sts-java-parity-mirror-plan-v4"
    ) {
        return Err(format!(
            "retained mirror plan has unsupported schema {:?}",
            source.schema
        ));
    }
    if source.run_start.character != "IRONCLAD" {
        return Err(format!(
            "replay-mirror-prefix only supports the Ironclad workbench, observed {:?}",
            source.run_start.character
        ));
    }
    if source.run_start.ascension > 20 {
        return Err(format!(
            "retained mirror plan ascension {} exceeds 20",
            source.run_start.ascension
        ));
    }
    if source.run_start.content_profile != "all_unlocked" {
        return Err(format!(
            "retained mirror plan content profile {:?} cannot seed the current all-unlocked simulator root",
            source.run_start.content_profile
        ));
    }
    let objective = match source.run_start.objective.as_str() {
        "act_three_victory" => TaskObjectiveV1::ActThreeVictory,
        "heart_victory" => TaskObjectiveV1::HeartVictory,
        other => {
            return Err(format!(
                "retained mirror plan has unsupported objective {other:?}"
            ));
        }
    };
    let seed = source.run_start.seed.parse::<u64>().map_err(|error| {
        format!(
            "retained mirror plan seed {:?} is not a canonical unsigned simulator seed: {error}",
            source.run_start.seed
        )
    })?;
    if source.steps.is_empty() {
        return Err("retained mirror plan contains no semantic steps".to_string());
    }
    for (index, step) in source.steps.iter().enumerate() {
        let expected_depth = index + 1;
        if step.depth != expected_depth {
            return Err(format!(
                "retained mirror plan is not depth-contiguous: step {} records depth {}",
                expected_depth, step.depth
            ));
        }
    }
    if through_depth > source.steps.len() {
        return Err(format!(
            "replay-mirror-prefix requested depth {through_depth}, but the plan ends at depth {}",
            source.steps.len()
        ));
    }
    let omitted_legacy_intro_steps = usize::from(
        source
            .steps
            .first()
            .is_some_and(is_legacy_neow_intro_continue_v1),
    );
    let steps = source
        .steps
        .into_iter()
        .skip(omitted_legacy_intro_steps)
        .take(through_depth - omitted_legacy_intro_steps)
        .map(|step| {
            let selector = parse_action_selector_v1(&step.semantic_action, "replay-mirror-prefix")
                .map_err(|error| {
                    format!(
                        "retained mirror plan depth {} has invalid semantic action {:?}: {error}",
                        step.depth, step.semantic_action
                    )
                })?;
            Ok(RunPlayRetainedSemanticPrefixStepV1 {
                source_depth: step.depth,
                expected_site: step.site,
                expected_before: step.before,
                selector,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(LoadedRetainedMirrorPrefixV1 {
        schema: source.schema,
        seed,
        ascension: source.run_start.ascension,
        objective,
        omitted_legacy_intro_steps,
        steps,
    })
}

fn is_legacy_neow_intro_continue_v1(step: &RetainedMirrorStepV1) -> bool {
    let action = &step.selected_candidate["action"];
    step.depth == 1
        && step.site == TaskDecisionSiteV1::Neow
        && step.semantic_action == "event:Neow:0"
        && action["kind"].as_str() == Some("choose_event_option")
        && action["event"].as_str() == Some("Neow")
        && action["screen"].as_u64() == Some(0)
        && action["option_index"].as_u64() == Some(0)
        && action["action"].as_str() == Some("Continue")
}

fn next_reroot_target_v1(source: &Path) -> Result<PathBuf, String> {
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "reroot-current source needs a directory name".to_string())?;
    let current = model::RunPlayEvidenceIdentityV1::current_v1();
    let digest = current
        .build_source
        .as_ref()
        .and_then(|source| source.source_sha256.get(..8))
        .ok_or_else(|| "current build source has no short digest".to_string())?;
    let base_name = match name.rsplit_once("-current-") {
        Some((base, suffix))
            if suffix
                .get(..8)
                .is_some_and(|hash| hash.bytes().all(|c| c.is_ascii_hexdigit()))
                && (suffix.len() == 8
                    || suffix
                        .get(8..)
                        .and_then(|tail| tail.strip_prefix('-'))
                        .is_some_and(|ordinal| {
                            !ordinal.is_empty() && ordinal.bytes().all(|c| c.is_ascii_digit())
                        })) =>
        {
            base
        }
        _ => name,
    };
    let stem = format!(
        "{}-current-{digest}",
        base_name.chars().take(80).collect::<String>()
    );
    for ordinal in 1..=999 {
        let candidate = parent.join(if ordinal == 1 {
            stem.clone()
        } else {
            format!("{stem}-{ordinal}")
        });
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "no free reroot-current target beside {}",
        source.display()
    ))
}

fn run(cli: Cli) -> Result<(), String> {
    let adapter_failure_report = cli.adapter_failure_report;
    match cli.command {
        Command::New {
            session,
            seed,
            character,
            ascension,
            neow_offer,
            goal,
            max_world_transitions,
            max_reloads,
            focus,
        } => {
            let paths = RunPlayPathsV1::new(&session);
            let workspace = create_scratch_natural_session_v1(
                &paths,
                sts_oracle_learning::eval::run_control::RunControlConfig {
                    seed,
                    ascension_level: ascension,
                    neow_eligibility: neow_offer.into(),
                    player_class: character.player_class(),
                    final_act: matches!(goal, RunGoalArgV1::HeartVictory),
                    ..Default::default()
                },
                TaskBudgetV1 {
                    max_world_transitions,
                    max_reloads,
                },
                CombatResearchPlanOptionsV1 {
                    complete_turn_proposal_beam: Some(
                        CompleteTurnProposalBeamResearchOptionsV1::default(),
                    ),
                    ..CombatResearchPlanOptionsV1::default()
                },
            )?;
            describe_session_v1(&paths, RunPlaySessionPurposeV1::Play, None)?;
            if focus {
                set_focus_v1(Path::new(".oracle-lab/run-play-focus"), &session)?;
            }
            print!("{}", render_status_v1(&workspace.head)?);
        }
        Command::ReplayMirrorPrefix {
            plan,
            session,
            through_depth,
            neow_offer,
            max_world_transitions,
            max_reloads,
        } => {
            let loaded = load_retained_mirror_prefix_v1(&plan, through_depth)?;
            let paths = RunPlayPathsV1::new(&session);
            create_natural_session_v1(
                &paths,
                sts_oracle_learning::eval::run_control::RunControlConfig {
                    seed: loaded.seed,
                    ascension_level: loaded.ascension,
                    neow_eligibility: neow_offer.into(),
                    final_act: (loaded.objective)
                        == sts_oracle_learning::agent::task::TaskObjectiveV1::HeartVictory,
                    ..Default::default()
                },
                TaskBudgetV1 {
                    max_world_transitions,
                    max_reloads,
                },
                CombatResearchPlanOptionsV1 {
                    complete_turn_proposal_beam: Some(
                        CompleteTurnProposalBeamResearchOptionsV1::default(),
                    ),
                    ..CombatResearchPlanOptionsV1::default()
                },
            )?;
            describe_session_v1(
                &paths,
                RunPlaySessionPurposeV1::Lab,
                Some(format!(
                    "current-semantics replay of {} through depth {}",
                    plan.display(),
                    through_depth
                )),
            )?;
            let mut controller = RunPlayControllerV1::open(paths)?;
            let mut cursor = 0usize;
            let mut batch_count = 0usize;
            let mut normalized_interstitials = 0usize;
            while cursor < loaded.steps.len() {
                normalized_interstitials += usize::from(
                    controller
                        .normalize_retained_semantic_prefix_boundary_v1(&loaded.steps[cursor])?,
                );
                let site = loaded.steps[cursor].expected_site;
                let mut end = cursor + 1;
                while end < loaded.steps.len()
                    && end - cursor < MAX_ANCHOR_LINE_STEPS
                    && loaded.steps[end].expected_site == site
                {
                    end += 1;
                }
                let steps = &loaded.steps[cursor..end];
                controller.anchor_retained_semantic_prefix_chunk_v1(steps)?;
                batch_count += 1;
                eprintln!(
                    "run-play operation=replay-mirror-prefix batch={} committed_through_depth={} head=h{}",
                    batch_count,
                    steps.last().map(|step| step.source_depth).unwrap_or_default(),
                    controller.head().head_revision
                );
                cursor = end;
            }
            println!(
                "replay_mirror_prefix schema={} source={} through_depth={} omitted_legacy_intro_steps={} actions={} normalized_interstitials={} batches={} session={}",
                loaded.schema,
                plan.display(),
                through_depth,
                loaded.omitted_legacy_intro_steps,
                loaded.steps.len(),
                normalized_interstitials,
                batch_count,
                session.display()
            );
            print!("{}", render_show_v1(controller.head())?);
        }
        Command::Import {
            session,
            archive,
            checkpoints,
            node,
            max_world_transitions,
            max_reloads,
        } => {
            let paths = RunPlayPathsV1::new(&session);
            let workspace = import_session_v1(
                &paths,
                &archive,
                &checkpoints,
                node.as_deref(),
                TaskBudgetV1 {
                    max_world_transitions,
                    max_reloads,
                },
                CombatResearchPlanOptionsV1 {
                    complete_turn_proposal_beam: Some(
                        CompleteTurnProposalBeamResearchOptionsV1::default(),
                    ),
                    ..CombatResearchPlanOptionsV1::default()
                },
            )?;
            describe_session_v1(&paths, RunPlaySessionPurposeV1::Archive, None)?;
            print!("{}", render_show_v1(&workspace.head)?);
        }
        Command::SnapshotCurrent { session, output } => {
            snapshot_current_for_reroot_v1(RunPlayPathsV1::new(session), &output)?;
            println!("snapshot_current output={}", output.display());
        }
        Command::Reroot { snapshot, target } => {
            let paths = RunPlayPathsV1::new(&target);
            let workspace = reroot_session_from_capsule_v1(&snapshot, &paths)?;
            describe_session_v1(&paths, RunPlaySessionPurposeV1::Lab, None)?;
            println!(
                "rerooted exact_state=verified old_trajectory=unverified source_capsule={} target_session={} target_head={} target_node={} target={}",
                snapshot.display(),
                workspace.head.session_id,
                workspace.head.head_revision,
                workspace.head.position.archive_node_id(),
                target.display(),
            );
            print!("{}", render_show_v1(&workspace.head)?);
        }
        Command::RerootCurrent {
            session,
            at,
            target,
            focus,
        } => {
            let from_focus = session.is_none();
            let session = match session {
                Some(session) => session,
                None => resolve_focus_session_path_v1(Path::new(".oracle-lab/run-play-focus"))?
                    .ok_or_else(|| "No run-play session is focused".to_string())?,
            };
            let target = match target {
                Some(target) => target,
                None => next_reroot_target_v1(&session)?,
            };
            let paths = RunPlayPathsV1::new(&target);
            let workspace =
                reroot_current_session_v1(RunPlayPathsV1::new(&session), &paths, at.as_deref())?;
            describe_session_v1(&paths, RunPlaySessionPurposeV1::Lab, None)?;
            if from_focus || focus {
                set_focus_v1(Path::new(".oracle-lab/run-play-focus"), &target)?;
            }
            let source_name = session.file_name().map_or_else(
                || session.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            let target_name = target.file_name().map_or_else(
                || target.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            println!(
                "Refreshed: {source_name} -> {target_name} (exact state/RNG verified; old session retained{})",
                if from_focus || focus { "; focus updated" } else { "" },
            );
            print!("{}", render_status_v1(&workspace.head)?);
        }
        Command::AdoptCurrent {
            session,
            target,
            focus,
        } => {
            let from_focus = session.is_none();
            let source = match session {
                Some(session) => session,
                None => resolve_focus_session_path_v1(Path::new(".oracle-lab/run-play-focus"))?
                    .ok_or_else(|| "No run-play session is focused".to_string())?,
            };
            let paths = RunPlayPathsV1::new(&target);
            let workspace = adopt_current_session_v1(RunPlayPathsV1::new(&source), &paths)?;
            describe_session_v1(&paths, RunPlaySessionPurposeV1::Play, None)?;
            if from_focus || focus {
                set_focus_v1(Path::new(".oracle-lab/run-play-focus"), &target)?;
            }
            println!("Current game state kept at {}", target.display());
            print!("{}", render_status_v1(&workspace.head)?);
        }
        Command::Fork {
            session,
            target,
            at,
            disposable,
            managed_root,
            expected_cleanup_phase,
        } => {
            let prepared = disposable
                .then(|| prepare_disposable_target_v1(&managed_root, &target))
                .transpose()?;
            let source_paths = RunPlayPathsV1::new(&session);
            let source_head = source_paths.load_head()?;
            eprintln!(
                "run-play operation=fork phase=started source_head=h{} source_node={} at={} target={}",
                source_head.head_revision,
                source_head.position.archive_node_id(),
                at.as_deref().unwrap_or("current"),
                target.display()
            );
            io::stderr()
                .flush()
                .map_err(|error| format!("failed to flush fork start signal: {error}"))?;
            let workspace =
                fork_session_v1(source_paths, &RunPlayPathsV1::new(&target), at.as_deref())?;
            describe_session_v1(
                &RunPlayPathsV1::new(&target),
                if disposable {
                    RunPlaySessionPurposeV1::Probe
                } else {
                    RunPlaySessionPurposeV1::Lab
                },
                None,
            )?;
            let manifest = prepared
                .as_ref()
                .map(|prepared| {
                    register_disposable_fork_v1(
                        prepared,
                        &session,
                        &source_head,
                        &workspace.head,
                        &expected_cleanup_phase,
                    )
                })
                .transpose()?;
            eprintln!(
                "run-play operation=fork phase=completed target_head=h{} target_node={}",
                workspace.head.head_revision,
                workspace.head.position.archive_node_id()
            );
            io::stderr()
                .flush()
                .map_err(|error| format!("failed to flush fork completion signal: {error}"))?;
            println!(
                "forked source_session={} source_head={} source_node={} target_session={} target_node={} target={}{}",
                source_head.session_id,
                source_head.head_revision,
                source_head.position.archive_node_id(),
                workspace.head.session_id,
                workspace.head.position.archive_node_id(),
                target.display(),
                manifest.as_ref().map_or_else(String::new, |path| format!(
                    " lifecycle=disposable manifest={}",
                    path.display()
                )),
            );
            print!("{}", render_show_v1(&workspace.head)?);
        }
        Command::PromoteBranch { source, target } => {
            let source_head = RunPlayPathsV1::new(&source).load_head()?;
            let target_head = RunPlayPathsV1::new(&target).load_head()?;
            eprintln!(
                "run-play operation=promote-branch phase=started source_head=h{} source_node={} target_head=h{} target_node={}",
                source_head.head_revision,
                source_head.position.archive_node_id(),
                target_head.head_revision,
                target_head.position.archive_node_id(),
            );
            io::stderr()
                .flush()
                .map_err(|error| format!("failed to flush promote-branch start signal: {error}"))?;
            let promotion =
                promote_branch_v1(RunPlayPathsV1::new(source), RunPlayPathsV1::new(target))?;
            eprintln!(
                "run-play operation=promote-branch phase=completed common_node={} skipped_public_only_ancestors={} steps={} target_head=h{} target_node={}",
                promotion.common_archive_node_id,
                promotion.skipped_public_only_ancestors,
                promotion.promoted_step_count,
                promotion.head.head_revision,
                promotion.head.position.archive_node_id(),
            );
            io::stderr().flush().map_err(|error| {
                format!("failed to flush promote-branch completion signal: {error}")
            })?;
            println!(
                "promoted source_session={} source_head=h{} source_node={} target_session={} previous_target_head=h{} common_node={} skipped_public_only_ancestors={} steps={}",
                promotion.source_session_id,
                promotion.source_head_revision,
                promotion.source_endpoint_archive_node_id,
                promotion.target_session_id,
                promotion.previous_target_head_revision,
                promotion.common_archive_node_id,
                promotion.skipped_public_only_ancestors,
                promotion.promoted_step_count,
            );
            print!("{}", render_status_v1(&promotion.head)?);
        }
        Command::CounterfactualBranch {
            session,
            managed_root,
            output,
            max_steps,
        } => {
            let artifact =
                capture_counterfactual_branch_evidence_v1(&session, &managed_root, max_steps)?;
            write_counterfactual_branch_evidence_v1(&output, &managed_root, &artifact)?;
            println!(
                "counterfactual-branch scope={} source_session={} source_head=h{} source_node={} branch_session={} branch_head=h{} branch_node={} steps={} authority=none output={}",
                artifact.evidence_scope,
                artifact.source_root.session_id,
                artifact.source_root.head_revision,
                artifact.source_root.archive_node_id,
                artifact.branch_endpoint.session_id,
                artifact.branch_endpoint.head_revision,
                artifact.branch_endpoint.archive_node_id,
                artifact.steps.len(),
                output.display(),
            );
        }
        Command::Show {
            session,
            public_output,
            decision_json,
        } => {
            // Reproject only a source compatible with this build. Older heads
            // remain readable as stored evidence without runtime reconstruction.
            let paths = RunPlayPathsV1::new(session);
            let head = paths.load_head()?;
            let stored_only = !head.scratch
                && model::RunPlayEvidenceIdentityV1::require_current_v1(
                    head.evidence_identity.as_ref(),
                )
                .is_err();
            if stored_only || matches!(head.position, model::RunPlayPositionV1::Terminal { .. }) {
                if public_output.is_some() {
                    if !head.scratch {
                        model::RunPlayEvidenceIdentityV1::require_current_v1(
                            head.evidence_identity.as_ref(),
                        )?;
                    }
                    return Err("a terminal has no public decision to export".into());
                }
                let workspace = paths.load()?;
                if stored_only {
                    println!(
                        "Stored evidence only; this build has not reconstructed this decision."
                    );
                }
                print!("{}", render_show_v1(&workspace.head)?);
            } else {
                let controller = RunPlayControllerV1::open(paths)?;
                if let Some(output) = public_output {
                    public_decision::export_head_public_decision_v1(controller.head(), &output)?;
                    println!("Public decision exported for offline reading; Rust-only, not an action permit.");
                } else if decision_json {
                    let inventory = controller
                        .head()
                        .current_decision
                        .as_ref()
                        .ok_or("head has no public decision")?;
                    let decision = inventory.public_decision_v1()?;
                    // Attach the executable semantic action string per
                    // candidate, using the same projector the agent shell
                    // uses, so tooling never has to guess string forms.
                    let semantic_actions: Vec<Option<String>> = inventory
                        .candidates
                        .iter()
                        .map(|candidate| {
                            candidate
                                .direct_choice
                                .as_ref()
                                .map(|choice| semantic_action_for_choice_v1(&decision, choice))
                                .transpose()
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    let document = serde_json::json!({
                        "decision": decision,
                        "semantic_actions": semantic_actions,
                    });
                    let text = serde_json::to_string_pretty(&document)
                        .map_err(|error| error.to_string())?;
                    println!("{text}");
                } else {
                    print!("{}", render_show_v1(controller.head())?);
                }
            }
        }
        Command::Map { session, query } => {
            let session =
                session.ok_or("map requires --session (or a focused session via p.ps1)")?;
            let paths = RunPlayPathsV1::new(session);
            let saved = paths.load_head()?;
            let head = if saved.scratch && saved.current_decision.is_some() {
                RunPlayControllerV1::open(paths)?.head().clone()
            } else {
                saved
            };
            let decision = head
                .current_decision
                .as_ref()
                .ok_or("map requires a current public decision")?
                .public_decision_v1()?;
            let map = match &decision.observation {
                TaskPublicObservationV1::Strategic { observation } => &observation.public_map,
                TaskPublicObservationV1::Combat { run_context, .. } => match run_context {
                    sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Available { public_map, .. } => public_map,
                    sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Unavailable { .. } => {
                        return Err("the current combat has no public map".into());
                    }
                },
            };
            let legal_routes: Vec<_> = decision
                .candidates
                .iter()
                .filter_map(|candidate| match candidate {
                    TaskPublicCandidateV1::Strategic {
                        action: TaskStrategicActionV1::ChooseRouteNode { x, y, .. },
                    } => Some((*x, *y)),
                    _ => None,
                })
                .collect();
            match query {
                None => print!(
                    "{}",
                    public_map_reading::route_examples(
                        map,
                        &public_map_reading::planning_roots(map, &legal_routes),
                    )?
                ),
                Some(MapQueryV1::Full) => {
                    print!("{}", public_map_reading::map_grid(map, &legal_routes))
                }
                Some(MapQueryV1::Elites) => {
                    let (from, next, corridor) =
                        public_map_reading::next_nodes(map, &legal_routes, None)?;
                    if legal_routes.is_empty()
                        && !map.nodes.iter().any(|node| (node.x, node.y) == from)
                    {
                        println!("Published entrances before a route decision:");
                    } else if from == (map.current_x, map.current_y) {
                        println!("From ({},{}):", from.0, from.1);
                    } else {
                        println!(
                            "Next fork ({},{}); shared corridor excluded from counts: {}",
                            from.0,
                            from.1,
                            public_map_reading::format_corridor(map, &corridor)
                        );
                    }
                    print!(
                        "{}",
                        public_map_reading::elite_routes(map, &next, &corridor)?
                    );
                }
                Some(MapQueryV1::Route { x, y }) => {
                    print!(
                        "{}",
                        public_map_reading::route_from(map, &legal_routes, (x, y))?
                    );
                }
            }
        }
        Command::DeckLab {
            session,
            output,
            list,
            scenarios,
            encounter,
            work,
            ms,
            full_hp,
            details,
            max_potion_uses,
            after,
        } => {
            let session =
                session.ok_or("deck-lab requires --session (or a focused session via p.ps1)")?;
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(&session))?;
            let head = controller.head().clone();
            let exact = controller
                .current_external_exact_checkpoint_v1()?
                .into_session()?;
            if exact.active_combat.is_some() {
                return Err("deck-lab requires an out-of-combat decision".into());
            }
            let source_position = compact::render_position_resources_v1(
                &head
                    .current_decision
                    .as_ref()
                    .ok_or("deck-lab requires a decision")?
                    .public_decision_v1()?
                    .observation,
            );
            let original_run = exact.run_state;
            let source_act = original_run.act_num;
            let source_floor = original_run.floor_num;
            let mut preparation = None;
            let mut preparation_text = String::new();
            let run = if let Some(line) = after {
                let before = head
                    .current_decision
                    .as_ref()
                    .ok_or("deck-lab requires a decision")?
                    .public_decision_v1()?;
                let selectors = selector::parse_action_line_v1(&line, "deck-lab after")?;
                let (prepared, actions) =
                    controller.prepare_resources_for_deck_lab_v1(&selectors)?;
                let after = prepared
                    .current_decision()
                    .ok_or("deck-lab preparation has no decision")?
                    .public_decision_v1()?;
                let resources = prepared
                    .current_run_decision_checkpoint_for_run_play_v1()?
                    .into_session()?
                    .run_state;
                preparation_text.push_str(&format!(
                    "Preparation (private exact scratch RNG): {}\nHP {}/{} -> {}/{} | Gold {} -> {}\n",
                    actions.join(" / "), original_run.current_hp, original_run.max_hp,
                    resources.current_hp, resources.max_hp, original_run.gold, resources.gold));
                preparation_text.push_str(&compact::render_inventory_change_v1(&before, &after));
                preparation_text.push_str(&compact::render_inventory_decision_v1(
                    after.clone(),
                    false,
                    None,
                    Locale::En,
                )?);
                if let TaskPublicObservationV1::Strategic { observation } = &after.observation {
                    preparation_text.push_str(&format!("Base energy/turn {} | Potions: {}\n",
                        agent_protocol::public_base_energy_per_turn_v1(&observation.relics),
                        observation.potions.iter().map(|slot| format!("P{} {}", slot.slot,
                            slot.potion.as_ref().map_or_else(|| "empty".into(), |potion|
                                sts_oracle_learning::content::identity::gameplay_display_label_v1(potion.potion).to_string()))).collect::<Vec<_>>().join(", ")));
                }
                preparation_text.push_str("Random preparations show one exact sampled outcome; combat tests then use the declared fixed diagnostic seeds.\n");
                preparation = Some(
                    serde_json::json!({ "actions": actions, "before": before, "after": after,
                    "rng_contract": "Preparation uses the exact scratch RNG on a private branch. A random preparation is one sampled outcome. Encounter diagnostics then replace combat RNG with the declared fixed diagnostic seeds." }),
                );
                resources
            } else {
                original_run
            };
            if list {
                println!("Actual position unchanged: {source_position}");
                println!(
                    "Act {} diagnostic battery (TheHeart is an explicit counterfactual):",
                    run.act_num
                );
                for spec in deck_lab::available(&run) {
                    println!("  {}", spec.encounter.stable_id());
                }
                return Ok(());
            }
            let encounters = deck_lab::select(&run, &encounter)?;
            let output = output.ok_or("deck-lab requires --output PATH unless --list is used")?;
            if output.exists() || !output.parent().is_some_and(|parent| parent.is_dir()) {
                return Err(format!(
                    "deck-lab needs a fresh file in an existing directory: {}",
                    output.display()
                ));
            }
            let report = deck_lab::evaluate(
                &run,
                &encounters,
                deck_lab::Budget {
                    scenarios,
                    work,
                    ms,
                    full_hp,
                    max_potion_uses: max_potion_uses as usize,
                },
            )?;
            if controller.head() != &head {
                return Err("deck-lab changed the durable run head".into());
            }
            let source_kind = if preparation.is_some() {
                "run_play_prepared_resources_counterfactual_diagnostic_rng"
            } else {
                "run_play_current_resources_counterfactual_diagnostic_rng"
            };
            let artifact = serde_json::json!({
                "producer": { "name": "run_play_deck_lab_v4", "build_identity": model::RunPlayEvidenceIdentityV1::current_v1() },
                "source": { "session_id": head.session_id, "head_revision": head.head_revision,
                    "state_revision": head.state_revision, "act": source_act, "floor": source_floor,
                    "source_kind": source_kind },
                "preparation": preparation,
                "probe": report,
            });
            let bytes = serde_json::to_vec_pretty(&artifact).map_err(|error| error.to_string())?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)
                .map_err(|error| format!("failed to create {}: {error}", output.display()))?;
            file.write_all(&bytes)
                .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
            println!("Actual position unchanged: {source_position}");
            print!("{}{}", preparation_text, deck_lab::render(&report, details));
            println!("Full cases: {}", output.display());
        }
        Command::Info {
            session,
            query,
            locale,
        } => {
            let paths = RunPlayPathsV1::new(session);
            let head = paths.load_head()?;
            let head = if head.scratch && head.current_decision.is_some() {
                RunPlayControllerV1::open(paths)?.head().clone()
            } else {
                if !head.scratch {
                    model::RunPlayEvidenceIdentityV1::require_current_v1(
                        head.evidence_identity.as_ref(),
                    )?;
                }
                head
            };
            let decision = head
                .current_decision
                .as_ref()
                .map(|inventory| inventory.public_decision_v1())
                .transpose()?;
            print!(
                "{}",
                mechanics_info::render(decision.as_ref(), &query.join(" "), locale)?
            );
        }
        Command::Options { session } => {
            let controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            print_shell_actions_v1(controller.head())?;
        }
        Command::Status { session } => {
            let paths = RunPlayPathsV1::new(session);
            let head = paths.load_head()?;
            let head = if head.scratch && head.current_decision.is_some() {
                RunPlayControllerV1::open(paths)?.head().clone()
            } else {
                head
            };
            print!("{}", render_status_v1(&head)?);
        }
        Command::Inventory {
            session,
            effects,
            locale,
        } => {
            let mut controller =
                RunPlayControllerV1::open_for_read_only_snapshot_v1(RunPlayPathsV1::new(session))?;
            print!("{}", render_inventory_v1(&mut controller, effects, locale)?);
        }
        Command::ParityRng { session, at } => {
            let paths = RunPlayPathsV1::new(session);
            let mut controller = match at.as_deref() {
                Some(selector) => RunPlayControllerV1::open_at(paths, selector)?,
                None => RunPlayControllerV1::open(paths)?,
            };
            let (card_rng, neow_rng, map_rng, combat_rng) =
                controller.current_rngs_for_parity_v2()?;
            let combat_streams = combat_rng
                .map(|rng| {
                    vec![
                        ("AbstractDungeon.aiRng", "floor", rng.ai_rng),
                        ("AbstractDungeon.shuffleRng", "floor", rng.shuffle_rng),
                        (
                            "AbstractDungeon.cardRandomRng",
                            "floor",
                            rng.card_random_rng,
                        ),
                        ("AbstractDungeon.miscRng", "floor", rng.misc_rng),
                        (
                            "AbstractDungeon.monsterHpRng",
                            "floor",
                            rng.monster_hp_rng,
                        ),
                        ("AbstractDungeon.potionRng", "run", rng.potion_rng),
                    ]
                    .into_iter()
                    .map(|(owner, scope, rng)| {
                        serde_json::json!({
                            "owner": format!("RunState.active_combat.rng.{owner}"),
                            "java_owner": format!("com.megacrit.cardcrawl.dungeons.{owner}"),
                            "algorithm": "com.badlogic.gdx.math.RandomXS128",
                            "scope": scope,
                            "state_hex": [format!("{:016X}", rng.seed0), format!("{:016X}", rng.seed1)],
                            "counter": rng.counter,
                        })
                    })
                    .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let payload = serde_json::json!({
                "schema": "sts-run-play-parity-rng-v3",
                "evidence_scope": "privileged_parity_only",
                "head_revision": controller.head().head_revision,
                "archive_node_id": controller.head().position.archive_node_id(),
                "streams": [
                    {
                        "owner": "RunState.rng_pool.card_rng",
                        "java_owner": "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRng",
                        "algorithm": "com.badlogic.gdx.math.RandomXS128",
                        "scope": "run",
                        "state_hex": [format!("{:016X}", card_rng.seed0), format!("{:016X}", card_rng.seed1)],
                        "counter": card_rng.counter,
                    },
                    {
                        "owner": "RunState.neow_rng",
                        "java_owner": "com.megacrit.cardcrawl.neow.NeowEvent.rng",
                        "algorithm": "com.badlogic.gdx.math.RandomXS128",
                        "scope": "run",
                        "state_hex": [format!("{:016X}", neow_rng.seed0), format!("{:016X}", neow_rng.seed1)],
                        "counter": neow_rng.counter,
                    },
                    {
                        "owner": "RunState.map_rng",
                        "java_owner": "com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng",
                        "algorithm": "com.badlogic.gdx.math.RandomXS128",
                        "scope": "run",
                        "state_hex": [format!("{:016X}", map_rng.seed0), format!("{:016X}", map_rng.seed1)],
                        "counter": map_rng.counter,
                    }
                ],
                "combat_streams": combat_streams,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&payload)
                    .map_err(|error| format!("failed to encode parity RNG observation: {error}"))?
            );
        }
        Command::StateFingerprint { session, at } => {
            let paths = RunPlayPathsV1::new(session);
            let mut controller = match at.as_deref() {
                Some(selector) => RunPlayControllerV1::open_at(paths, selector)?,
                None => RunPlayControllerV1::open(paths)?,
            };
            let (exact_state_fingerprint, continuation_fingerprint) =
                controller.current_state_fingerprints_v1()?;
            let payload = serde_json::json!({
                "schema": "sts-run-play-state-fingerprint-v1",
                "evidence_scope": "privileged_exact_state_only",
                "head_revision": controller.head().head_revision,
                "archive_node_id": controller.head().position.archive_node_id(),
                "exact_state_fingerprint": exact_state_fingerprint,
                "continuation_fingerprint": continuation_fingerprint,
                "continuation_contract": "future simulator behavior with controller-local combat diagnostics removed",
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&payload).map_err(|error| {
                    format!("failed to encode run-play state fingerprints: {error}")
                })?
            );
        }
        Command::PairedRootPrepare {
            session,
            at,
            java_checkpoint,
            java_plan,
            java_live_output,
            store,
            attempt,
            report,
        } => {
            let prepared = paired_root_adapter::prepare_paired_root_probe_v1(
                &paired_root_adapter::PreparePairedRootRequestV1 {
                    session,
                    retained_selector: at,
                    java_checkpoint_manifest: java_checkpoint,
                    java_mirror_plan: java_plan,
                    java_live_output,
                    store_root: store,
                    attempt_directory: attempt,
                },
            )?;
            paired_root_adapter::write_preparation_report_new_v1(&report, &prepared)?;
            println!(
                "paired_root_prepare status={} source_node={} root_id={} report={}",
                prepared.status,
                prepared.source_archive_node_id,
                prepared.root_id.as_deref().unwrap_or("none"),
                report.display(),
            );
        }
        Command::PairedRootPreparePreflight { session } => {
            let preflight = paired_root_adapter::preflight_paired_root_source_v1(&session)?;
            println!(
                "{}",
                serde_json::to_string(&preflight).map_err(|error| {
                    format!("failed to encode paired-root source preflight: {error}")
                })?
            );
        }
        Command::PairedRootBootstrapNaturalNeow {
            session,
            seed,
            ascension,
            first_java_frame,
            first_java_talk_receipt,
            java_recovery_storage,
            native_autosave_sha256,
            java_live_output,
            store,
            attempt,
            report,
        } => {
            let prepared = paired_root_adapter::prepare_parentless_natural_neow_root_v1(
                &paired_root_adapter::PrepareParentlessNaturalNeowRootRequestV1 {
                    session,
                    seed,
                    ascension,
                    first_java_frame,
                    first_java_talk_receipt,
                    java_recovery_storage,
                    native_autosave_sha256,
                    java_live_output,
                    store_root: store,
                    attempt_directory: attempt,
                },
            )?;
            paired_root_adapter::write_parentless_natural_neow_report_new_v1(&report, &prepared)?;
            println!(
                "paired_root_parentless_neow status={} root_id={} activation_id={} report={}",
                prepared.status,
                prepared.root_id,
                prepared.activation_id,
                report.display(),
            );
        }
        Command::PairedRootBootstrapConstructedCombat {
            session,
            scenario_plan,
            scenario_report,
            java_live_output,
            store,
            attempt,
            report,
        } => {
            let prepared = paired_root_adapter::prepare_constructed_combat_root_v1(
                &paired_root_adapter::PrepareConstructedCombatRootRequestV1 {
                    session,
                    scenario_plan,
                    scenario_report,
                    java_live_output,
                    store_root: store,
                    attempt_directory: attempt,
                },
            )?;
            paired_root_adapter::write_constructed_combat_root_report_new_v1(&report, &prepared)?;
            println!(
                "paired_root_constructed_combat status={} case={} root_id={} activation_id={} report={}",
                prepared.status,
                prepared.case_id,
                prepared.root_id.as_deref().unwrap_or("none"),
                prepared.activation_id.as_deref().unwrap_or("none"),
                report.display(),
            );
        }
        Command::PairedRootObserveActions {
            root,
            store,
            report,
        } => {
            let observed = paired_root_adapter::observe_paired_root_actions_v1(
                &paired_root_adapter::ObservePairedRootActionsRequestV1 {
                    root_id: root,
                    store_root: store,
                },
            )?;
            paired_root_adapter::write_action_observation_report_new_v1(&report, &observed)?;
            println!(
                "paired_root_actions root={} action_space={} count={} report={}",
                observed.root_id,
                observed.action_space_id,
                observed.observed_actions.len(),
                report.display(),
            );
            for action in &observed.observed_actions {
                let authorization = observed
                    .authorizations
                    .iter()
                    .find(|authorization| authorization.handle_id == action.handle.handle_id)
                    .ok_or_else(|| {
                        "paired-root observed action omitted its authorization".to_owned()
                    })?;
                let capability_status = match &authorization.capability.status {
                    sts_paired_root::CapabilityStatusV1::Verified { .. } => "verified",
                    sts_paired_root::CapabilityStatusV1::Unknown { .. } => "unknown",
                    sts_paired_root::CapabilityStatusV1::Unsupported { .. } => "unsupported",
                };
                match (
                    &action.handle.action,
                    &action.card,
                    &action.potion,
                    &action.combat_reward,
                    &action.card_reward,
                ) {
                    (
                        sts_paired_root::PairedActionV1::ChooseNeowOption {
                            reward,
                            drawback,
                        },
                        None,
                        None,
                        None,
                        None,
                    ) => println!(
                        "paired_action handle={} kind=choose_neow_option reward={:?} drawback={:?} capability_status={}",
                        action.handle.handle_id, reward, drawback, capability_status,
                    ),
                    (
                        sts_paired_root::PairedActionV1::LeaveNeow,
                        None,
                        None,
                        None,
                        None,
                    ) => println!(
                        "paired_action handle={} kind=leave_neow capability_status={}",
                        action.handle.handle_id, capability_status,
                    ),
                    (
                        sts_paired_root::PairedActionV1::PlayCard { target, .. },
                        Some(card),
                        None,
                        None,
                        None,
                    ) => {
                        let target = target
                            .as_ref()
                            .map(|target| {
                                format!("{}#{}", target.enemy_id, target.same_enemy_ordinal)
                            })
                            .unwrap_or_else(|| "none".to_owned());
                        println!(
                            "paired_action handle={} kind=play_card card={} upgrades={} cost={} target={} capability_status={}",
                            action.handle.handle_id,
                            card.card_id,
                            card.upgrades,
                            card.cost_for_turn,
                            target,
                            capability_status,
                        );
                    }
                    (
                        sts_paired_root::PairedActionV1::UsePotion { slot, target, .. },
                        None,
                        Some(potion),
                        None,
                        None,
                    ) => {
                        let target = target
                            .as_ref()
                            .map(|target| {
                                format!("{}#{}", target.enemy_id, target.same_enemy_ordinal)
                            })
                            .unwrap_or_else(|| "none".to_owned());
                        println!(
                            "paired_action handle={} kind=use_potion slot={} potion={} potency={} target={} capability_status={}",
                            action.handle.handle_id,
                            slot,
                            potion.potion_id,
                            potion.potency,
                            target,
                            capability_status,
                        );
                    }
                    (sts_paired_root::PairedActionV1::EndTurn, None, None, None, None) => println!(
                        "paired_action handle={} kind=end_turn capability_status={}",
                        action.handle.handle_id,
                        capability_status,
                    ),
                    (
                        sts_paired_root::PairedActionV1::ClaimCombatReward {
                            reward,
                            same_reward_ordinal,
                        },
                        None,
                        None,
                        Some(observed_reward),
                        None,
                    ) if reward == observed_reward => {
                        let reward_detail = match reward {
                            sts_paired_root::CombatRewardIdentityV1::Gold { amount } => {
                                format!("reward=gold amount={amount}")
                            }
                            sts_paired_root::CombatRewardIdentityV1::StolenGold { amount } => {
                                format!("reward=stolen_gold amount={amount}")
                            }
                            sts_paired_root::CombatRewardIdentityV1::Card { choice_count } => {
                                format!("reward=card choice_count={choice_count}")
                            }
                            sts_paired_root::CombatRewardIdentityV1::Potion { potion_id } => {
                                format!("reward=potion id={potion_id}")
                            }
                        };
                        println!(
                            "paired_action handle={} kind=claim_combat_reward {} ordinal={} capability_status={}",
                            action.handle.handle_id,
                            reward_detail,
                            same_reward_ordinal,
                            capability_status,
                        );
                    }
                    (
                        sts_paired_root::PairedActionV1::ChooseCardReward {
                            card_id,
                            upgrades,
                            same_card_ordinal,
                        },
                        None,
                        None,
                        None,
                        Some(paired_root_adapter::PairedRootObservedCardRewardV1::Card {
                            card_id: observed_card_id,
                            upgrades: observed_upgrades,
                        }),
                    ) if card_id == observed_card_id && upgrades == observed_upgrades => println!(
                        "paired_action handle={} kind=choose_card_reward card={} upgrades={} ordinal={} capability_status={}",
                        action.handle.handle_id,
                        card_id,
                        upgrades,
                        same_card_ordinal,
                        capability_status,
                    ),
                    (
                        sts_paired_root::PairedActionV1::SkipCardReward,
                        None,
                        None,
                        None,
                        Some(paired_root_adapter::PairedRootObservedCardRewardV1::Skip),
                    ) => println!(
                        "paired_action handle={} kind=skip_card_reward capability_status={}",
                        action.handle.handle_id,
                        capability_status,
                    ),
                    (
                        sts_paired_root::PairedActionV1::ChooseEventOption {
                            event_id,
                            screen,
                            option_index,
                        },
                        None,
                        None,
                        None,
                        None,
                    ) => println!(
                        "paired_action handle={} kind=choose_event_option event={} screen={} option={} capability_status={}",
                        action.handle.handle_id,
                        event_id,
                        screen,
                        option_index,
                        capability_status,
                    ),
                    (
                        sts_paired_root::PairedActionV1::CloseCombatReward,
                        None,
                        None,
                        None,
                        None,
                    ) => println!(
                        "paired_action handle={} kind=close_combat_reward capability_status={}",
                        action.handle.handle_id,
                        capability_status,
                    ),
                    (
                        sts_paired_root::PairedActionV1::ChooseMapNode {
                            x,
                            y,
                            room_type,
                        },
                        None,
                        None,
                        None,
                        None,
                    ) => println!(
                        "paired_action handle={} kind=choose_map_node x={} y={} room={:?} capability_status={}",
                        action.handle.handle_id, x, y, room_type, capability_status,
                    ),
                    _ => {
                        return Err(
                            "paired-root action observation has inconsistent display evidence"
                                .to_owned(),
                        );
                    }
                }
            }
        }
        Command::PairedRootSearch {
            root,
            store,
            report,
        } => {
            let searched = paired_root_adapter::search_paired_root_v1(
                &paired_root_adapter::SearchPairedRootRequestV1 {
                    root_id: root,
                    store_root: store,
                },
            )?;
            paired_root_adapter::write_search_report_new_v1(&report, &searched)?;
            println!(
                "paired_root_search status={} root={} action_space={} witnesses={} bounded_frontiers={} summary_exit={:?} report={}",
                searched.status,
                searched.root_id,
                searched.action_space_id,
                searched.witnesses.len(),
                searched.bounded_frontier.len(),
                searched.summary_exit,
                report.display(),
            );
        }
        Command::PairedRootPreviewAction {
            session,
            root,
            activation,
            action,
            java_live_output,
            store,
            attempt,
            report,
        } => {
            let preview = paired_root_adapter::preview_paired_root_action_v1(
                &paired_root_adapter::PreviewPairedRootActionRequestV1 {
                    source_session: session,
                    source_root_id: root,
                    source_activation_id: activation,
                    action_handle_id: action,
                    java_live_output,
                    store_root: store,
                    attempt_directory: attempt,
                    adapter_failure_report: adapter_failure_report.clone(),
                },
            )?;
            paired_root_adapter::write_preview_report_new_v1(&report, &preview)?;
            println!(
                "paired_root_preview status={} source_root={} source_activation={} action={} successor_root={} successor_activation={} receipt={} divergence={} report={}",
                preview.status,
                preview.source_root_id,
                preview.source_activation_id,
                preview.action.handle_id,
                preview.successor_root_id.as_deref().unwrap_or("none"),
                preview
                    .successor_activation_id
                    .as_deref()
                    .unwrap_or("none"),
                preview.receipt_id.as_deref().unwrap_or("none"),
                preview.divergence_id.as_deref().unwrap_or("none"),
                report.display(),
            );
        }
        Command::PairedRootMaterializeNative {
            target_root,
            store,
            output,
            report,
        } => {
            let materialized = paired_root_adapter::materialize_paired_root_native_v1(
                &paired_root_adapter::MaterializePairedRootNativeRequestV1 {
                    target_root_id: target_root,
                    store_root: store,
                    output_directory: output,
                },
            )?;
            paired_root_adapter::write_native_materialization_report_new_v1(
                &report,
                &materialized,
            )?;
            println!(
                "paired_root_native_materialized target_root={} native_root={} bundle={} files={} recovery_steps={} storage={} report={}",
                materialized.requested_target_root_id,
                materialized.native_root_id,
                materialized.java_bundle_id,
                materialized.file_count,
                materialized.recovery_steps,
                materialized.storage_directory,
                report.display(),
            );
        }
        Command::PairedRootResumePreview {
            target_root,
            action,
            run_root,
            java_live_output,
            store,
            recovery_attempt,
            preview_attempt,
            activation_report,
            recovery_report,
            preview_report,
        } => {
            let activated = paired_root_adapter::activate_paired_root_native_v1(
                &paired_root_adapter::ActivatePairedRootNativeRequestV1 {
                    target_root_id: target_root.clone(),
                    run_root,
                    java_live_output: java_live_output.clone(),
                    store_root: store.clone(),
                },
            )?;
            paired_root_adapter::write_native_activation_report_new_v1(
                &activation_report,
                &activated,
            )?;
            let expected_target_root = target_root.clone();
            let recovered = paired_root_adapter::recover_paired_root_v1(
                &paired_root_adapter::RecoverPairedRootRequestV1 {
                    source_session: PathBuf::from(&activated.source_session),
                    native_root_id: activated.native_root_id.clone(),
                    native_activation_id: activated.native_activation_id.clone(),
                    target_root_id: target_root,
                    java_live_output: java_live_output.clone(),
                    store_root: store.clone(),
                    attempt_directory: recovery_attempt,
                },
            );
            let recovered = recovered?;
            paired_root_adapter::write_recovery_report_new_v1(&recovery_report, &recovered)?;
            let target_activation = recovered.target_activation_id.clone().ok_or_else(|| {
                format!(
                    "paired-root recovery stopped before target activation; report={}",
                    recovery_report.display()
                )
            })?;
            if recovered.status != "ready" || recovered.target_root_id != expected_target_root {
                return Err(format!(
                    "paired-root recovery did not reach its target; report={}",
                    recovery_report.display()
                ));
            }
            let preview = paired_root_adapter::preview_paired_root_action_v1(
                &paired_root_adapter::PreviewPairedRootActionRequestV1 {
                    source_session: PathBuf::from(&activated.source_session),
                    source_root_id: recovered.target_root_id.clone(),
                    source_activation_id: target_activation,
                    action_handle_id: action,
                    java_live_output,
                    store_root: store,
                    attempt_directory: preview_attempt,
                    adapter_failure_report: adapter_failure_report.clone(),
                },
            )?;
            paired_root_adapter::write_preview_report_new_v1(&preview_report, &preview)?;
            println!(
                "paired_root_resume_preview recovery_status={} native_root={} native_activation={} target_root={} target_activation={} steps={} preview_status={} action={} successor_root={} receipt={} divergence={} activation_report={} recovery_report={} preview_report={}",
                recovered.status,
                recovered.native_root_id,
                recovered.native_activation_id,
                recovered.target_root_id,
                recovered.target_activation_id.as_deref().unwrap_or("none"),
                recovered.steps.len(),
                preview.status,
                preview.action.handle_id,
                preview.successor_root_id.as_deref().unwrap_or("none"),
                preview.receipt_id.as_deref().unwrap_or("none"),
                preview.divergence_id.as_deref().unwrap_or("none"),
                activation_report.display(),
                recovery_report.display(),
                preview_report.display(),
            );
        }
        Command::PairedRootResumeWitness {
            target_root,
            witness_plan,
            run_root,
            java_live_output,
            store,
            recovery_attempt,
            transaction_attempt,
            activation_report,
            recovery_report,
            transaction_report,
        } => {
            let activated = paired_root_adapter::activate_paired_root_native_v1(
                &paired_root_adapter::ActivatePairedRootNativeRequestV1 {
                    target_root_id: target_root.clone(),
                    run_root,
                    java_live_output: java_live_output.clone(),
                    store_root: store.clone(),
                },
            )?;
            paired_root_adapter::write_native_activation_report_new_v1(
                &activation_report,
                &activated,
            )?;
            let recovered = paired_root_adapter::recover_paired_root_v1(
                &paired_root_adapter::RecoverPairedRootRequestV1 {
                    source_session: PathBuf::from(&activated.source_session),
                    native_root_id: activated.native_root_id.clone(),
                    native_activation_id: activated.native_activation_id.clone(),
                    target_root_id: target_root.clone(),
                    java_live_output: java_live_output.clone(),
                    store_root: store.clone(),
                    attempt_directory: recovery_attempt,
                },
            )?;
            paired_root_adapter::write_recovery_report_new_v1(&recovery_report, &recovered)?;
            let target_activation = recovered.target_activation_id.clone().ok_or_else(|| {
                format!(
                    "paired-root witness recovery stopped before target activation; report={}",
                    recovery_report.display()
                )
            })?;
            if recovered.status != "ready" || recovered.target_root_id != target_root {
                return Err(format!(
                    "paired-root witness recovery did not reach its target; report={}",
                    recovery_report.display()
                ));
            }
            let transaction = paired_root_adapter::preview_paired_root_witness_transaction_v1(
                &paired_root_adapter::PreviewPairedRootWitnessTransactionRequestV1 {
                    source_session: PathBuf::from(&activated.source_session),
                    source_root_id: recovered.target_root_id.clone(),
                    source_activation_id: target_activation,
                    witness_plan,
                    java_live_output,
                    store_root: store,
                    attempt_directory: transaction_attempt,
                    adapter_failure_report: adapter_failure_report.clone(),
                },
            )?;
            paired_root_adapter::write_witness_transaction_report_new_v1(
                &transaction_report,
                &transaction,
            )?;
            println!(
                "paired_root_resume_witness recovery_status={} native_root={} target_root={} recovery_steps={} witness={} preview_status={} completed_steps={} reached_root={} stopped_step={} activation_report={} recovery_report={} transaction_report={}",
                recovered.status,
                recovered.native_root_id,
                recovered.target_root_id,
                recovered.steps.len(),
                transaction.witness_id,
                transaction.status,
                transaction.steps.len(),
                transaction.reached_root_id,
                transaction
                    .stopped_step_ordinal
                    .map(|value| value.to_string())
                    .as_deref()
                    .unwrap_or("none"),
                activation_report.display(),
                recovery_report.display(),
                transaction_report.display(),
            );
        }
        Command::PairedRootPreviewWitness {
            session,
            root,
            activation,
            witness_plan,
            java_live_output,
            store,
            attempt,
            report,
        } => {
            let transaction = paired_root_adapter::preview_paired_root_witness_transaction_v1(
                &paired_root_adapter::PreviewPairedRootWitnessTransactionRequestV1 {
                    source_session: session,
                    source_root_id: root,
                    source_activation_id: activation,
                    witness_plan,
                    java_live_output,
                    store_root: store,
                    attempt_directory: attempt,
                    adapter_failure_report: adapter_failure_report.clone(),
                },
            )?;
            paired_root_adapter::write_witness_transaction_report_new_v1(&report, &transaction)?;
            println!(
                "paired_root_preview_witness witness={} status={} completed_steps={} reached_root={} report={}",
                transaction.witness_id,
                transaction.status,
                transaction.steps.len(),
                transaction.reached_root_id,
                report.display(),
            );
        }
        Command::PairedRootCasProof {
            store,
            head,
            root,
            receipt,
            report,
        } => {
            let proof = paired_root_adapter::verify_paired_root_receipt_chain_cas_v1(
                &store, &head, &root, &receipt,
            )?;
            paired_root_adapter::write_cas_proof_report_new_v1(&report, &proof)?;
            println!(
                "paired_root_cas head={} revision={} root={} steps={} stale_retries_rejected={} report={}",
                proof.head_name,
                proof.final_head.revision,
                proof.final_head.root_id,
                proof.steps.len(),
                proof.steps.iter().all(|step| step.stale_retry_rejected),
                report.display(),
            );
        }
        Command::PairedRootHeadInit {
            store,
            head,
            root,
            report,
        } => {
            let initialized =
                paired_root_adapter::initialize_paired_root_head_v1(&store, &head, &root)?;
            paired_root_adapter::write_head_init_report_new_v1(&report, &initialized)?;
            println!(
                "paired_root_head_init head={} revision={} root={} scope={} report={}",
                initialized.head.head_name,
                initialized.head.revision,
                initialized.head.root_id,
                initialized.execution_scope,
                report.display(),
            );
        }
        Command::PairedRootHeadCommit {
            store,
            head,
            receipt,
            report,
        } => {
            let committed =
                paired_root_adapter::commit_paired_root_head_receipt_v1(&store, &head, &receipt)?;
            paired_root_adapter::write_head_commit_report_new_v1(&report, &committed)?;
            println!(
                "paired_root_head_commit head={} status={:?} committed_revision={} current_revision={} receipt={} scope={} report={}",
                committed.current_head.head_name,
                committed.status,
                committed.committed_head.revision,
                committed.current_head.revision,
                committed.receipt_id,
                committed.execution_scope,
                report.display(),
            );
        }
        Command::PairedRootHeadSelect {
            store,
            selector,
            expected_revision,
            head,
            report,
        } => {
            let selected = paired_root_adapter::select_paired_root_head_v1(
                &store,
                &selector,
                expected_revision,
                &head,
            )?;
            paired_root_adapter::write_head_selector_report_new_v1(&report, &selected)?;
            println!(
                "paired_root_head_select selector={} selector_revision={} head={} status={:?} scope={} report={}",
                selected.selector.selector_name,
                selected.selector.revision,
                selected.selector.head_name,
                selected.status,
                selected.execution_scope,
                report.display(),
            );
        }
        Command::PairedRootHeadResolve {
            store,
            selector,
            report,
        } => {
            let resolved = paired_root_adapter::resolve_paired_root_head_v1(&store, &selector)?;
            paired_root_adapter::write_head_resolution_report_new_v1(&report, &resolved)?;
            println!(
                "paired_root_head_resolve selector={} selector_revision={} head={} head_revision={} root={} scope={} report={}",
                resolved.resolution.selector.selector_name,
                resolved.resolution.selector.revision,
                resolved.resolution.head.head_name,
                resolved.resolution.head.revision,
                resolved.resolution.head.root_id,
                resolved.execution_scope,
                report.display(),
            );
        }
        Command::PairedRootSelectedHeadCommit {
            store,
            selector,
            receipt,
            report,
        } => {
            let committed = paired_root_adapter::commit_selected_paired_root_head_receipt_v1(
                &store, &selector, &receipt,
            )?;
            paired_root_adapter::write_selected_head_commit_report_new_v1(&report, &committed)?;
            println!(
                "paired_root_selected_head_commit selector={} selector_revision={} head={} status={:?} committed_revision={} current_revision={} receipt={} scope={} report={}",
                committed.selector.selector_name,
                committed.selector.revision,
                committed.current_head.head_name,
                committed.status,
                committed.committed_head.revision,
                committed.current_head.revision,
                committed.receipt_id,
                committed.execution_scope,
                report.display(),
            );
        }
        Command::Overview {
            repository_commit,
            repository_dirty,
            workbench_sha256,
            run_root,
            focus_root,
            max_sessions,
            json,
        } => {
            let overview = build_overview_v1(
                repository_commit,
                repository_dirty,
                workbench_sha256,
                &run_root,
                &focus_root,
                max_sessions,
            )?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&overview)
                        .map_err(|error| format!("failed to encode run-play overview: {error}"))?
                );
            } else {
                print!("{}", render_overview_v1(&overview));
            }
        }
        Command::Focus { action } => match action {
            FocusCommandV1::Show { focus_root, json } => {
                let focus = load_focus_v1(&focus_root)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&focus)
                            .map_err(|error| format!("failed to encode run-play focus: {error}"))?
                    );
                } else {
                    print!("{}", render_focus_v1(&focus));
                }
            }
            FocusCommandV1::Set {
                session,
                focus_root,
            } => {
                let focus = set_focus_v1(&focus_root, &session)?;
                print!("{}", render_focus_v1(&focus));
            }
            FocusCommandV1::Clear { focus_root } => {
                let focus = clear_focus_v1(&focus_root)?;
                print!("{}", render_focus_v1(&focus));
            }
            FocusCommandV1::Migrate {
                legacy_pointer,
                focus_root,
            } => {
                let focus = migrate_legacy_focus_v1(&focus_root, &legacy_pointer)?;
                print!("{}", render_focus_v1(&focus));
            }
        },
        Command::Session { action } => match action {
            SessionCommandV1::Show { session, json } => {
                let control = load_session_control_v1(&RunPlayPathsV1::new(session))?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&control).map_err(|error| {
                            format!("failed to encode run-play session control: {error}")
                        })?
                    );
                } else {
                    print!("{}", render_control_v1(&control));
                }
            }
            SessionCommandV1::Describe {
                session,
                purpose,
                label,
            } => {
                let control =
                    describe_session_v1(&RunPlayPathsV1::new(session), purpose.into(), label)?;
                print!("{}", render_control_v1(&control));
            }
            SessionCommandV1::Park { session, reason } => {
                let control = park_session_v1(&RunPlayPathsV1::new(session), reason)?;
                print!("{}", render_control_v1(&control));
            }
            SessionCommandV1::Unpark { session, reason } => {
                let control = unpark_session_v1(&RunPlayPathsV1::new(session), reason)?;
                print!("{}", render_control_v1(&control));
            }
        },
        Command::Handoff { action } => match action {
            HandoffCommandV2::Open {
                session,
                repository_commit,
                pause_reason,
                do_not_act,
                open_issues,
                completed_issues,
            } => {
                let summary = open_handoff_v2(
                    &RunPlayPathsV1::new(session),
                    repository_commit,
                    pause_reason,
                    do_not_act,
                    open_issues,
                    completed_issues,
                )?;
                print!("{}", render_handoff_summary_v1(&summary));
            }
            HandoffCommandV2::Resolve {
                session,
                repository_commit,
                resolution,
            } => {
                let summary = resolve_handoff_v2(
                    &RunPlayPathsV1::new(session),
                    repository_commit,
                    resolution,
                )?;
                print!("{}", render_handoff_summary_v1(&summary));
            }
            HandoffCommandV2::Show {
                session,
                repository_commit,
                json,
            } => {
                let summary = load_handoff_summary_v1(
                    &RunPlayPathsV1::new(session),
                    repository_commit.as_deref(),
                )?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&summary).map_err(|error| {
                            format!("failed to encode run-play handoff summary: {error}")
                        })?
                    );
                } else {
                    print!("{}", render_handoff_summary_v1(&summary));
                }
            }
        },
        Command::ReferenceRequest { session } => {
            let controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            let inventory =
                controller.head().current_decision.as_ref().ok_or_else(|| {
                    "run-play reference-request requires a decision head".to_string()
                })?;
            let decision = inventory.public_decision_v1()?;
            let request = reference_request_v3(&decision)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&request).map_err(|error| {
                    format!("failed to serialize reference request: {error}")
                })?
            );
        }
        Command::Advice {
            session,
            at,
            detail,
        } => {
            let paths = RunPlayPathsV1::new(session);
            if let Some(selector) = at {
                let inspection = inspect_advice_at_v1(paths, &selector, detail)?;
                print!(
                    "{}",
                    render_advice_v1(&inspection.advice, &inspection.decision)?
                );
                if let Some(detail) = inspection.detail {
                    print!("{}", render_advice_detail_v1(&detail));
                }
            } else {
                let mut controller = RunPlayControllerV1::open(paths)?;
                let advice = controller.advise_v1()?;
                let decision = controller
                    .head()
                    .current_decision
                    .as_ref()
                    .ok_or_else(|| "run-play advice requires a decision head".to_string())?;
                print!("{}", render_advice_v1(&advice, decision)?);
                if detail {
                    let detail = controller.advice_detail_v1()?;
                    print!("{}", render_advice_detail_v1(&detail));
                }
            }
        }
        Command::CombatResearch {
            session,
            effort,
            detail,
            artifact,
        } => {
            if let Some(path) = artifact.as_ref() {
                if path.exists() {
                    return Err(format!(
                        "combat research artifact already exists: {}",
                        path.display()
                    ));
                }
            }
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            let options = combat_research_options_for_effort_v1(
                &controller.head().combat_plan_options,
                effort,
                None,
            );
            if detail {
                println!(
                    "combat_research_start effort={effort:?} schedule={:?} resident_work={} proposal={} atomic={}",
                    options.lane_schedule,
                    options.max_generation_work,
                    options
                        .complete_turn_proposal_beam
                        .as_ref()
                        .map(|beam| format!("{}@{}ms", beam.max_expanded_boundaries, beam.wall_ms))
                        .unwrap_or_else(|| "off".to_string()),
                    options
                        .atomic_challenger
                        .as_ref()
                        .map(|atomic| format!("{}@{}ms", atomic.max_nodes, atomic.wall_ms))
                        .unwrap_or_else(|| "off".to_string()),
                );
            } else {
                println!("combat_research_start effort={effort:?}");
            }
            std::io::stdout()
                .flush()
                .map_err(|error| format!("failed to flush combat research start: {error}"))?;
            let mut progress = |event| {
                match event {
                    CombatResearchProgressEventV1::LaneStarted { lane } => {
                        println!("combat_research_lane={} event=started", lane.label());
                    }
                    CombatResearchProgressEventV1::LaneCompleted {
                        lane,
                        elapsed,
                        attempt,
                    } => {
                        println!(
                            "combat_research_lane={} event=completed elapsed_ms={} work_unit={:?} work={} exit={:?}",
                            lane.label(),
                            elapsed.as_millis(),
                            attempt.work_unit,
                            attempt.work_consumed,
                            attempt.exit,
                        );
                    }
                    CombatResearchProgressEventV1::LaneFailed { lane, elapsed } => {
                        println!(
                            "combat_research_lane={} event=failed elapsed_ms={}",
                            lane.label(),
                            elapsed.as_millis(),
                        );
                    }
                }
                let _ = std::io::stdout().flush();
            };
            let (attempt, captured) = capture_combat_research_artifact_with_progress_v1(
                &mut controller,
                effort,
                &mut progress,
            )?;
            if let Some(path) = artifact {
                let mut payload = serde_json::to_vec_pretty(&captured).map_err(|error| {
                    format!("failed to serialize combat research artifact: {error}")
                })?;
                payload.push(b'\n');
                storage::write_new_synced(&path, &payload, "run-play combat research artifact")?;
                println!("combat_research_artifact={}", path.display());
            }
            print!("{}", render_combat_research_attempt_v1(&attempt, detail)?);
        }
        Command::CombatQuery {
            session,
            eventually_generated_card,
            eventually_lethal_payoff,
            max_generation_work,
            max_selections,
            wall_ms,
            max_turn_depth,
            max_potions_used,
            allowed_potion_slot,
            allow_potion_discard,
            output,
        } => {
            if let Some(path) = output.as_ref() {
                if path.exists() {
                    return Err(format!(
                        "combat query artifact already exists: {}",
                        path.display()
                    ));
                }
            }
            let formulas = eventually_generated_card
                .into_iter()
                .map(|card_id| {
                    sts_combat_planner::CombatTrajectoryQueryV1::eventually(
                        sts_combat_planner::CombatTrajectoryAtomV1::GeneratedCardSelected {
                            card_id,
                        },
                    )
                })
                .chain(eventually_lethal_payoff.into_iter().map(|card_id| {
                    sts_combat_planner::CombatTrajectoryQueryV1::eventually(
                        sts_combat_planner::CombatTrajectoryAtomV1::CardLethalPayoff { card_id },
                    )
                }))
                .collect::<Vec<_>>();
            let query = match formulas.len() {
                0 => sts_combat_planner::CombatTrajectoryQueryV1::True,
                1 => formulas.into_iter().next().expect("one query formula"),
                _ => sts_combat_planner::CombatTrajectoryQueryV1::all(formulas),
            };
            let allowed_potion_slots = potion_slot_mask_v1(&allowed_potion_slot)?;
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            let report = combat_query::run_current_combat_trajectory_query_v2(
                &mut controller,
                combat_query::CombatTrajectoryExperimentOptionsV1 {
                    query,
                    max_generation_work,
                    max_selections,
                    wall_ms,
                    max_turn_depth,
                    max_potions_used,
                    allow_potion_discard,
                    allowed_potion_slots,
                },
            )?;
            if let Some(path) = output {
                let mut payload = serde_json::to_vec_pretty(&report).map_err(|error| {
                    format!("failed to serialize combat query artifact: {error}")
                })?;
                payload.push(b'\n');
                storage::write_new_synced(&path, &payload, "run-play combat query artifact")?;
                println!("combat_query_artifact={}", path.display());
            }
            print!(
                "{}",
                combat_query::render_combat_trajectory_query_v2(&report)
            );
        }
        Command::AnchorCombatQuery {
            session,
            artifact,
            candidate,
        } => {
            let captured = combat_query::load_combat_trajectory_query_commit_artifact_candidate_v2(
                &artifact, candidate,
            )?;
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            let profile = combat_query::anchor_combat_trajectory_query_artifact_v2(
                &mut controller,
                &captured,
            )?;
            println!("combat_query_artifact_replay={}", artifact.display());
            if let Some(candidate) = candidate {
                println!("combat_query_frontier_candidate={candidate}");
            }
            println!(
                "artifact_replay_profile total_ms={:.3} restore_ms={:.3} prepare_ms={:.3} initial_fork_ms={:.3} step_ms={:.3} actor_ms={:.3} archive_ms={:.3} checkpoint_merge_ms={:.3} final_checkpoint_ms={:.3} endpoint_validation_ms={:.3} publish_ms={:.3}",
                profile.total.as_secs_f64() * 1_000.0,
                profile.exact_restore.as_secs_f64() * 1_000.0,
                profile.prepared_line.total.as_secs_f64() * 1_000.0,
                profile.prepared_line.initial_fork.as_secs_f64() * 1_000.0,
                profile.prepared_line.step_total.as_secs_f64() * 1_000.0,
                profile.prepared_line.actor_episode.as_secs_f64() * 1_000.0,
                profile.prepared_line.archive_record.as_secs_f64() * 1_000.0,
                profile.prepared_line.checkpoint_merge.as_secs_f64() * 1_000.0,
                profile.prepared_line.final_checkpoint.as_secs_f64() * 1_000.0,
                profile.endpoint_validation.as_secs_f64() * 1_000.0,
                profile.publication.as_secs_f64() * 1_000.0,
            );
            print!("{}", render_status_v1(controller.head())?);
        }
        Command::CombatTrace {
            session,
            artifact,
            candidate,
            output,
        } => {
            if let Some(path) = output.as_ref() {
                if path.exists() {
                    return Err(format!(
                        "combat trace artifact already exists: {}",
                        path.display()
                    ));
                }
            }
            let trace = combat_trace::trace_combat_artifact_v1(
                RunPlayPathsV1::new(session),
                &artifact,
                &candidate,
            )?;
            if let Some(path) = output {
                let mut payload = serde_json::to_vec_pretty(&trace)
                    .map_err(|error| format!("failed to serialize combat trace: {error}"))?;
                payload.push(b'\n');
                storage::write_new_synced(&path, &payload, "run-play combat trace artifact")?;
                println!("combat_trace_artifact={}", path.display());
            }
            print!("{}", combat_trace::render_combat_trace_v1(&trace));
        }
        Command::CombatFeedRepair {
            session,
            artifact,
            candidate,
            fixed_prefix_actions,
            minimum_final_hp,
            minimum_final_max_hp,
            max_generation_work,
            max_selections,
            wall_ms,
            output,
        } => {
            if let Some(path) = output.as_ref() {
                if path.exists() {
                    return Err(format!(
                        "combat Feed repair artifact already exists: {}",
                        path.display()
                    ));
                }
            }
            let captured = load_combat_research_artifact_v1(&artifact)?;
            let report = combat_repair::repair_feed_from_combat_artifact_v1(
                RunPlayPathsV1::new(session),
                &captured,
                combat_repair::CombatFeedRepairOptionsV1 {
                    candidate_index: candidate,
                    fixed_prefix_actions,
                    minimum_final_hp,
                    minimum_final_max_hp,
                    max_generation_work,
                    max_selections,
                    wall_ms,
                },
            )?;
            if let Some(path) = output {
                let mut payload = serde_json::to_vec_pretty(&report)
                    .map_err(|error| format!("failed to serialize combat Feed repair: {error}"))?;
                payload.push(b'\n');
                storage::write_new_synced(&path, &payload, "run-play combat Feed repair artifact")?;
                println!("combat_feed_repair_artifact={}", path.display());
            }
            print!("{}", combat_repair::render_combat_feed_repair_v1(&report));
        }
        Command::CombatFeedChallenge {
            session,
            artifact,
            max_challengers,
            max_generation_work_per_challenger,
            max_selections_per_challenger,
            wall_ms_per_challenger,
            output,
        } => {
            if let Some(path) = output.as_ref() {
                if path.exists() {
                    return Err(format!(
                        "combat Feed challenge artifact already exists: {}",
                        path.display()
                    ));
                }
            }
            let captured = combat_repair::load_verified_combat_source_v1(&artifact)?;
            let report = combat_repair::challenge_nonfatal_feed_from_verified_source_v1(
                RunPlayPathsV1::new(session),
                &captured,
                combat_repair::CombatFeedChallengeOptionsV1 {
                    max_challengers,
                    max_generation_work_per_challenger,
                    max_selections_per_challenger,
                    wall_ms_per_challenger,
                },
            )?;
            if let Some(path) = output {
                let mut payload = serde_json::to_vec_pretty(&report).map_err(|error| {
                    format!("failed to serialize combat Feed challenge: {error}")
                })?;
                payload.push(b'\n');
                storage::write_new_synced(
                    &path,
                    &payload,
                    "run-play combat Feed challenge artifact",
                )?;
                println!("combat_feed_challenge_artifact={}", path.display());
            }
            print!(
                "{}",
                combat_repair::render_combat_feed_challenge_v2(&report)
            );
        }
        Command::CombatFeedAnchor {
            session,
            artifact,
            frontier,
        } => {
            let captured = combat_repair::load_combat_feed_challenge_artifact_v2(&artifact)?;
            let selected = captured
                .global_resource_frontier
                .get(frontier)
                .ok_or_else(|| {
                    format!(
                        "combat Feed frontier {frontier} is outside artifact frontier count {}",
                        captured.global_resource_frontier.len()
                    )
                })?
                .clone();
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            let current_decision = controller
                .head()
                .current_decision
                .as_ref()
                .ok_or_else(|| "combat Feed anchor requires a public decision".to_string())?
                .public_decision_v1()?;
            if controller.head().session_id != captured.session_id
                || controller.head().position.archive_node_id() != captured.root_archive_node_id
                || current_decision.information.snapshot_id != captured.root_public_snapshot_id
            {
                return Err(format!(
                    "combat Feed anchor root drifted: artifact_session={} current_session={} artifact_node={} current_node={} artifact_snapshot={} current_snapshot={}",
                    captured.session_id,
                    controller.head().session_id,
                    captured.root_archive_node_id,
                    controller.head().position.archive_node_id(),
                    captured.root_public_snapshot_id,
                    current_decision.information.snapshot_id,
                ));
            }
            let inputs = selected
                .exact_actions
                .iter()
                .map(|action| action.input.clone())
                .collect::<Vec<_>>();
            controller.anchor_exact_combat_replay_v1(&inputs, &selected.public_outcome)?;
            println!(
                "combat_feed_anchor frontier={} public={}/{} terminal={}/{} max_hp_gain={} potions={} actions={}",
                frontier,
                selected.public_outcome.final_hp,
                selected.public_outcome.persistent.max_hp,
                selected.combat_terminal_hp,
                selected.combat_terminal_max_hp,
                selected.max_hp_gain,
                selected.total_potions_consumed,
                selected.total_action_count,
            );
            print!("{}", render_status_v1(controller.head())?);
        }
        Command::CombatDamageChallenge {
            session,
            artifact,
            max_challengers,
            max_generation_work_per_challenger,
            max_selections_per_challenger,
            wall_ms_per_challenger,
            output,
        } => {
            if let Some(path) = output.as_ref() {
                if path.exists() {
                    return Err(format!(
                        "combat damage challenge artifact already exists: {}",
                        path.display()
                    ));
                }
            }
            let captured = combat_repair::load_verified_combat_source_v1(&artifact)?;
            let report = combat_repair::challenge_turn_damage_from_verified_source_v1(
                RunPlayPathsV1::new(session),
                &captured,
                combat_repair::CombatDamageChallengeOptionsV1 {
                    max_challengers,
                    max_generation_work_per_challenger,
                    max_selections_per_challenger,
                    wall_ms_per_challenger,
                },
            )?;
            if let Some(path) = output {
                let mut payload = serde_json::to_vec_pretty(&report).map_err(|error| {
                    format!("failed to serialize combat damage challenge: {error}")
                })?;
                payload.push(b'\n');
                storage::write_new_synced(
                    &path,
                    &payload,
                    "run-play combat damage challenge artifact",
                )?;
                println!("combat_damage_challenge_artifact={}", path.display());
            }
            print!(
                "{}",
                combat_repair::render_combat_damage_challenge_v1(&report)
            );
        }
        Command::CombatDamageAnchor {
            session,
            artifact,
            frontier,
        } => {
            let captured = combat_repair::load_combat_damage_challenge_artifact_v1(&artifact)?;
            let selected = captured
                .global_resource_frontier
                .get(frontier)
                .ok_or_else(|| {
                    format!(
                        "combat damage frontier {frontier} is outside artifact frontier count {}",
                        captured.global_resource_frontier.len()
                    )
                })?
                .clone();
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            let current_decision = controller
                .head()
                .current_decision
                .as_ref()
                .ok_or_else(|| "combat damage anchor requires a public decision".to_string())?
                .public_decision_v1()?;
            if controller.head().position.archive_node_id() != captured.root_archive_node_id
                || current_decision.information.snapshot_id != captured.root_public_snapshot_id
            {
                return Err(format!(
                    "combat damage anchor root drifted: artifact_node={} current_node={} artifact_snapshot={} current_snapshot={}",
                    captured.root_archive_node_id,
                    controller.head().position.archive_node_id(),
                    captured.root_public_snapshot_id,
                    current_decision.information.snapshot_id,
                ));
            }
            let inputs = selected
                .exact_actions
                .iter()
                .map(|action| action.input.clone())
                .collect::<Vec<_>>();
            controller.anchor_exact_combat_replay_v1(&inputs, &selected.public_outcome)?;
            println!(
                "combat_damage_anchor frontier={} public={}/{} potions={} actions={}",
                frontier,
                selected.public_outcome.final_hp,
                selected.public_outcome.persistent.max_hp,
                selected.total_potions_consumed,
                selected.total_action_count,
            );
            print!("{}", render_status_v1(controller.head())?);
        }
        Command::ExportCombatCase {
            session,
            at,
            output,
        } => {
            if output.exists() {
                return Err(format!(
                    "combat case output already exists: {}",
                    output.display()
                ));
            }
            let paths = RunPlayPathsV1::new(session);
            let mut controller = match at {
                Some(selector) => RunPlayControllerV1::open_at(paths, &selector)?,
                None => RunPlayControllerV1::open(paths)?,
            };
            controller.export_current_combat_case_v2(&output)?;
            println!("exported combat case={}", output.display());
        }
        Command::ProposalAudit { session, line } => {
            let choices = parse_action_line_v1(&line, "proposal-audit")?;
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            let audit = controller.proposal_audit_v1(
                &choices,
                sts_complete_turn_proposal::ExactCompleteTurnProposalConfigV1::default(),
            )?;
            print!("{}", render_proposal_audit_v1(&audit));
        }
        Command::Compare {
            session,
            at,
            detail,
            artifact,
            lines,
        } => {
            if let Some(path) = artifact.as_ref() {
                if path.exists() {
                    return Err(format!(
                        "exact prefix artifact already exists: {}",
                        path.display()
                    ));
                }
            }
            let parsed = parse_comparison_lines_v1(lines.iter().map(String::as_str))?;
            let paths = RunPlayPathsV1::new(session);
            let mut controller = match at {
                Some(selector) => RunPlayControllerV1::open_at(paths, &selector)?,
                None => RunPlayControllerV1::open(paths)?,
            };
            let comparison = match artifact {
                Some(path) => {
                    let (comparison, captured) =
                        capture_exact_prefix_artifact_v1(&mut controller, &parsed)?;
                    let mut payload = serde_json::to_vec_pretty(&captured).map_err(|error| {
                        format!("failed to serialize exact prefix artifact: {error}")
                    })?;
                    payload.push(b'\n');
                    storage::write_new_synced(&path, &payload, "run-play exact prefix artifact")?;
                    println!("exact_prefix_artifact={}", path.display());
                    comparison
                }
                None => controller.compare_action_lines_v1(&parsed)?,
            };
            print!("{}", render_comparison_v1(&comparison, detail)?);
        }
        Command::AnchorLine { session, line } => {
            let choices = parse_action_line_v1(&line, "anchor-line")?;
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            controller.anchor_action_line_v1(&choices)?;
            print!("{}", render_status_v1(controller.head())?);
        }
        Command::AnchorCombatPlan {
            session,
            artifact,
            candidate,
            effort,
            producer,
        } => {
            let producer_name = combat_research_producer_name_v1(&producer)?;
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            if let Some(path) = artifact {
                let captured = load_combat_research_artifact_v1(&path)?;
                let profile = anchor_combat_research_artifact_candidate_v1(
                    &mut controller,
                    &captured,
                    producer_name,
                    candidate,
                )?;
                println!("combat_research_artifact_replay={}", path.display());
                println!(
                    "artifact_replay_profile total_ms={:.3} restore_ms={:.3} prepare_ms={:.3} initial_fork_ms={:.3} step_ms={:.3} actor_ms={:.3} archive_ms={:.3} checkpoint_merge_ms={:.3} final_checkpoint_ms={:.3} endpoint_validation_ms={:.3} publish_ms={:.3}",
                    profile.total.as_secs_f64() * 1_000.0,
                    profile.exact_restore.as_secs_f64() * 1_000.0,
                    profile.prepared_line.total.as_secs_f64() * 1_000.0,
                    profile.prepared_line.initial_fork.as_secs_f64() * 1_000.0,
                    profile.prepared_line.step_total.as_secs_f64() * 1_000.0,
                    profile.prepared_line.actor_episode.as_secs_f64() * 1_000.0,
                    profile.prepared_line.archive_record.as_secs_f64() * 1_000.0,
                    profile.prepared_line.checkpoint_merge.as_secs_f64() * 1_000.0,
                    profile.prepared_line.final_checkpoint.as_secs_f64() * 1_000.0,
                    profile.endpoint_validation.as_secs_f64() * 1_000.0,
                    profile.publication.as_secs_f64() * 1_000.0,
                );
            } else {
                let (_, captured) = capture_combat_research_artifact_v1(&mut controller, effort)?;
                anchor_combat_research_artifact_candidate_v1(
                    &mut controller,
                    &captured,
                    producer_name,
                    candidate,
                )?;
            }
            print!("{}", render_status_v1(controller.head())?);
        }
        Command::ProtocolChoose {
            session,
            ordinals,
            max_world_transitions,
            max_reloads,
            manual,
        } => {
            let paths = RunPlayPathsV1::new(session);
            let budget_override = match (max_world_transitions, max_reloads) {
                (None, None) => None,
                (transitions, reloads) => {
                    let current = paths.load()?.head.default_budget;
                    Some(TaskBudgetV1 {
                        max_world_transitions: transitions.unwrap_or(current.max_world_transitions),
                        max_reloads: reloads.unwrap_or(current.max_reloads),
                    })
                }
            };
            let workspace = choose_v1(
                &paths,
                TaskPublicChoiceV1 {
                    ordinal_path: ordinals,
                },
                budget_override,
                manual,
            )?;
            print!("{}", render_show_v1(&workspace.head)?);
        }
        Command::Preview {
            session,
            at,
            detail,
            artifact,
            actions,
        } => {
            if let Some(path) = artifact.as_ref() {
                if path.exists() {
                    return Err(format!(
                        "exact prefix artifact already exists: {}",
                        path.display()
                    ));
                }
            }
            let selectors = parse_shell_actions_v1(
                &actions.iter().map(String::as_str).collect::<Vec<_>>(),
                "preview",
            )?;
            if let Some(selector) = at.as_deref() {
                require_exact_node_selector_v1(selector, "preview")?;
            }
            eprintln!("Preview input: {}", compact_action_input_v1(&actions));
            io::stderr()
                .flush()
                .map_err(|error| format!("failed to flush preview start signal: {error}"))?;
            let paths = RunPlayPathsV1::new(session);
            let mut controller = match at {
                Some(selector) => RunPlayControllerV1::open_at(paths, &selector)?,
                None => RunPlayControllerV1::open(paths)?,
            };
            let comparison = match artifact {
                Some(path) => {
                    let (comparison, captured) =
                        capture_exact_prefix_artifact_v1(&mut controller, &[selectors])?;
                    let mut payload = serde_json::to_vec_pretty(&captured).map_err(|error| {
                        format!("failed to serialize exact prefix artifact: {error}")
                    })?;
                    payload.push(b'\n');
                    storage::write_new_synced(&path, &payload, "run-play exact prefix artifact")?;
                    println!("exact_prefix_artifact={}", path.display());
                    comparison
                }
                None => controller.compare_action_lines_v1(&[selectors])?,
            };
            if detail {
                eprintln!("Preview complete: current position unchanged");
                io::stderr().flush().map_err(|error| {
                    format!("failed to flush preview completion signal: {error}")
                })?;
            }
            print!("{}", render_preview_v1(&comparison, detail)?);
        }
        Command::Act {
            session,
            at,
            actions,
            auto_combat,
        } => {
            let selectors = parse_shell_actions_v1(
                &actions.iter().map(String::as_str).collect::<Vec<_>>(),
                "act",
            )?;
            if auto_combat && selectors.len() != 1 {
                return Err(
                    "run-play act --auto-combat accepts exactly one action; explicit multi-action lines are already transactional"
                        .to_string(),
                );
            }
            if let Some(selector) = at.as_deref() {
                require_exact_node_selector_v1(selector, "act")?;
            }
            println!("Input: {}", compact_action_input_v1(&actions));
            io::stdout()
                .flush()
                .map_err(|error| format!("failed to flush act start signal: {error}"))?;
            let paths = RunPlayPathsV1::new(session);
            let mut controller = match at {
                Some(selector) => RunPlayControllerV1::open_at(paths, &selector)?,
                None => RunPlayControllerV1::open(paths)?,
            };
            let before = controller
                .head()
                .current_decision
                .as_ref()
                .ok_or_else(|| "run-play act requires a decision head".to_string())?
                .public_decision_v1()?;
            if let [selector] = selectors.as_slice() {
                controller.choose_action_v1(selector, !auto_combat)?;
            } else {
                controller.anchor_action_line_v1(&selectors)?;
            }
            println!("Completed: h{}", controller.head().head_revision);
            io::stdout()
                .flush()
                .map_err(|error| format!("failed to flush act completion signal: {error}"))?;
            if let Some(after) = controller.head().current_decision.as_ref() {
                print!(
                    "{}",
                    compact::render_inventory_change_v1(&before, &after.public_decision_v1()?)
                );
            }
            print!("{}", render_status_v1(controller.head())?);
        }
        Command::Undo { session } => {
            let workspace = back_v1(&RunPlayPathsV1::new(session), 1)?;
            print!(
                "{}",
                if workspace.head.scratch {
                    render_status_v1(&workspace.head)?
                } else {
                    render_show_v1(&workspace.head)?
                }
            );
        }
        Command::Mark {
            session,
            name,
            drop,
        } => {
            let paths = RunPlayPathsV1::new(session);
            if let Some(name) = name {
                driver::save_scratch_mark_v1(&paths, &name)?;
                println!("Saved mark: {name}");
            } else if let Some(name) = drop {
                driver::drop_scratch_mark_v1(&paths, &name)?;
                println!("Dropped mark: {name}");
            } else {
                let head = paths.load_head()?;
                print!("{}", compact::render_mark_targets_v1(&paths, &head)?);
            }
        }
        Command::Back {
            session,
            steps,
            floor,
            mark,
            list,
            json,
            request,
        } => {
            let paths = RunPlayPathsV1::new(session);
            if let Some(request) = request {
                let bytes = storage::read_bounded(&request, 64 * 1024, "return request")?;
                let request = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                let receipt = return_points::restore_return_point_v1(&paths, &request)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&receipt).map_err(|error| error.to_string())?
                );
                return Ok(());
            }
            if list {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&return_points::return_points_v2(&paths)?)
                            .map_err(|error| error.to_string())?
                    );
                } else {
                    print!("{}", compact::render_back_targets_v1(&paths)?);
                }
                return Ok(());
            }
            let workspace = if let Some(name) = mark {
                driver::back_scratch_to_mark_v1(&paths, &name)?
            } else if let Some(floor) = floor {
                driver::back_scratch_to_floor_v1(&paths, floor)?
            } else {
                back_v1(&paths, steps.unwrap_or(1))?
            };
            print!(
                "{}",
                if workspace.head.scratch {
                    render_status_v1(&workspace.head)?
                } else {
                    render_show_v1(&workspace.head)?
                }
            );
        }
        Command::History {
            session,
            limit,
            strategic_only,
            combat_detail,
            at,
            journey,
            first,
            last,
            after_depth,
            count,
            detail,
        } => {
            let paths = RunPlayPathsV1::new(session);
            if journey {
                let window = if let Some(count) = first {
                    RunPlayJourneyWindowV1::First { count }
                } else if let Some(count) = last {
                    RunPlayJourneyWindowV1::Last { count }
                } else if let Some(depth) = after_depth {
                    RunPlayJourneyWindowV1::AfterDepth {
                        depth,
                        count: count.unwrap_or(8),
                    }
                } else {
                    RunPlayJourneyWindowV1::Last { count: 8 }
                };
                let history = journey_history_v1(&paths, window)?;
                print!("{}", render_journey_history_v1(&history, detail));
            } else if combat_detail {
                let workspace = paths.load()?;
                let from_node_id = at
                    .as_deref()
                    .map(|selector| {
                        resolve_jump_node_from_parts_v1(
                            &workspace.state.archive,
                            &workspace.head,
                            selector,
                        )
                    })
                    .transpose()?;
                let history = combat_history_v1(
                    &workspace.state.archive,
                    workspace.head.position.archive_node_id(),
                    limit.unwrap_or(40),
                    from_node_id.as_deref(),
                )?;
                print!("{}", render_combat_history_v1(&history));
            } else if strategic_only {
                let head = paths.load_head()?;
                print!(
                    "{}",
                    render_strategic_history_v1(&head, limit.unwrap_or(40))
                );
            } else {
                let head = paths.load_head()?;
                print!("{}", render_history_v1(&head, limit.unwrap_or(40)));
            }
        }
        Command::ConstructionHistory {
            session,
            site,
            candidate_card,
            added_card,
            all_boundaries,
            detail,
            json,
        } => {
            let site = site
                .as_deref()
                .map(parse_construction_site_v1)
                .transpose()?;
            let candidate_card = candidate_card
                .as_deref()
                .map(parse_construction_card_v1)
                .transpose()?;
            let added_card = added_card
                .as_deref()
                .map(parse_construction_card_v1)
                .transpose()?;
            let history = construction_history_v1(
                &RunPlayPathsV1::new(session),
                site,
                candidate_card,
                added_card,
                all_boundaries,
            )?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&history).map_err(|error| {
                        format!("failed to serialize construction history: {error}")
                    })?
                );
            } else if detail {
                print!("{}", render_construction_history_detail_v1(&history));
            } else {
                print!("{}", render_construction_history_v1(&history));
            }
        }
        Command::VictoryDossier {
            session,
            output,
            transcript_sources,
        } => {
            let dossier = victory_dossier_v1(&RunPlayPathsV1::new(session), &transcript_sources)?;
            let mut payload = serde_json::to_vec_pretty(&dossier)
                .map_err(|error| format!("failed to serialize victory dossier: {error}"))?;
            payload.push(b'\n');
            storage::write_new_synced(&output, &payload, "run-play victory dossier")?;
            println!(
                "victory_dossier session={} h{} strategic={} transcripts={} proposal_decisions={} proposals={}/{} accepted/rejected model_commits={} output={}",
                dossier.session.session_id,
                dossier.session.head_revision,
                dossier.active_path.strategic_decision_count,
                dossier.external_agent_evidence.matched_transcripts.len(),
                dossier
                    .external_agent_evidence
                    .distinct_final_active_decision_count_with_proposal,
                dossier.external_agent_evidence.proposal_accepted_count,
                dossier.external_agent_evidence.proposal_rejected_count,
                dossier
                    .external_agent_evidence
                    .distinct_final_active_model_action_commit_count,
                output.display()
            );
        }
        Command::Terminals { session, limit } => {
            let workspace = RunPlayPathsV1::new(session).load()?;
            print!(
                "{}",
                render_terminals_v1(&workspace.head, &workspace.state.archive, limit)
            );
        }
        Command::Jump { session, selector } => {
            let workspace = jump_v1(&RunPlayPathsV1::new(session), &selector)?;
            print!("{}", render_show_v1(&workspace.head)?);
        }
        Command::Shell { session } => run_shell_v1(session)?,
        Command::AgentShell {
            session,
            knowledge_pack,
            manual,
            locale,
        } => run_agent_shell_v1(session, knowledge_pack, manual, locale)?,
        Command::GameFacts {
            cards,
            relics,
            potions,
        } => {
            let mut entities = cards
                .into_iter()
                .map(|card| AgentGameFactEntityV1::Card { card })
                .collect::<Vec<_>>();
            entities.extend(
                relics
                    .into_iter()
                    .map(|relic| AgentGameFactEntityV1::Relic { relic }),
            );
            entities.extend(
                potions
                    .into_iter()
                    .map(|potion| AgentGameFactEntityV1::Potion { potion }),
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&agent_game_facts_v1(&entities))
                    .map_err(|error| format!("failed to serialize game facts: {error}"))?
            );
        }
        Command::Checkpoint { session } => {
            let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
            controller.checkpoint()?;
            print!("{}", render_show_v1(controller.head())?);
        }
        Command::CompactActive { session, commit } => {
            let (workspace, profile) = compact_active_v1(&RunPlayPathsV1::new(session), commit)?;
            print_active_compaction_profile_v1(profile, commit);
            if commit {
                print!("{}", render_show_v1(&workspace.head)?);
            }
        }
        Command::ProfileOpen { session } => {
            let (_controller, profile) =
                RunPlayControllerV1::open_profiled_v1(RunPlayPathsV1::new(session))?;
            print_open_profile_v1(profile);
        }
        Command::ProfileStorage { session } => {
            print_storage_census_v1(profile_storage_v1(&RunPlayPathsV1::new(session))?);
        }
        Command::WorkspaceGc { action } => match action {
            WorkspaceGcCommandV1::Summary {
                managed_root,
                focus_root,
            } => {
                let focus_session = resolve_focus_session_path_v1(&focus_root)?;
                let summary = inspect_workspace_gc_v1(&managed_root, focus_session.as_deref())?;
                print!("{}", render_workspace_gc_summary_v1(&summary));
                for invalid in summary.invalid_directories {
                    println!("workspace_gc_invalid {invalid}");
                }
            }
            WorkspaceGcCommandV1::Plan {
                managed_root,
                focus_root,
                output,
            } => {
                let focus_session = resolve_focus_session_path_v1(&focus_root)?;
                let plan =
                    write_workspace_gc_plan_v1(&managed_root, focus_session.as_deref(), &output)?;
                println!(
                    "workspace_gc_plan targets={} bytes={} output={}",
                    plan.targets.len(),
                    plan.total_bytes,
                    output.display()
                );
            }
            WorkspaceGcCommandV1::Execute {
                managed_root,
                focus_root,
                plan,
                receipt,
            } => {
                let focus_session = resolve_focus_session_path_v1(&focus_root)?;
                let (deleted, bytes) = execute_workspace_gc_plan_v1(
                    &managed_root,
                    focus_session.as_deref(),
                    &plan,
                    &receipt,
                )?;
                println!(
                    "workspace_gc_executed deleted={} bytes={} receipt={}",
                    deleted,
                    bytes,
                    receipt.display()
                );
            }
        },
        Command::Export {
            session,
            archive_output,
            checkpoints_output,
        } => {
            let workspace = RunPlayPathsV1::new(session).load()?;
            export_v1(&workspace, &archive_output, &checkpoints_output)?;
            println!(
                "exported active_head={} archive={} checkpoints={}",
                workspace.head.position.archive_node_id(),
                archive_output.display(),
                checkpoints_output.display()
            );
        }
    }
    Ok(())
}

fn combat_research_producer_name_v1(alias: &str) -> Result<&'static str, String> {
    match alias {
        "resident" => Ok(RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1),
        "proposal" => Ok(COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1),
        "atomic" => Ok(ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1),
        _ => Err(format!(
            "unknown combat research producer {alias}; expected resident, proposal, or atomic"
        )),
    }
}

fn print_active_compaction_profile_v1(profile: RunPlayActiveCompactionProfileV1, committed: bool) {
    println!("committed={committed}");
    println!("before_nodes={}", profile.before_nodes);
    println!("after_nodes={}", profile.after_nodes);
    println!("before_attempts={}", profile.before_attempts);
    println!("after_attempts={}", profile.after_attempts);
    println!("before_checkpoints={}", profile.before_checkpoints);
    println!("after_checkpoints={}", profile.after_checkpoints);
    println!(
        "before_checkpoint_payload_bytes={}",
        profile.before_checkpoint_payload_bytes
    );
    println!(
        "after_checkpoint_payload_bytes={}",
        profile.after_checkpoint_payload_bytes
    );
    println!(
        "after_state_payload_bytes={}",
        profile.after_state_payload_bytes
    );
}

fn print_storage_census_v1(census: RunPlayStorageCensusV1) {
    println!("elapsed_ms={:.3}", census.elapsed.as_secs_f64() * 1000.0);
    println!(
        "workspace_load_ms={:.3}",
        census.workspace_load.total.as_secs_f64() * 1000.0
    );
    println!(
        "workspace_state_deserialize_ms={:.3}",
        census.workspace_load.state_deserialize.as_secs_f64() * 1000.0
    );
    println!(
        "workspace_state_payload_bytes={}",
        census.workspace_load.state_payload_bytes
    );
    println!("archive_payload_bytes={}", census.archive_payload_bytes);
    println!(
        "rewritten_state_payload_bytes={}",
        census.rewritten_state_payload_bytes
    );
    println!(
        "archive_task_payload_bytes={}",
        census.archive_task_payload_bytes
    );
    println!(
        "archive_nodes_payload_bytes={}",
        census.archive_nodes_payload_bytes
    );
    println!(
        "archive_attempts_payload_bytes={}",
        census.archive_attempts_payload_bytes
    );
    println!(
        "checkpoint_payload_bytes={}",
        census.checkpoint_payload_bytes
    );
    println!("node_count={}", census.node_count);
    println!("attempt_count={}", census.attempt_count);
    println!("incoming_step_count={}", census.incoming_step_count);
    println!(
        "persisted_decision_occurrences={}",
        census.persisted_decision_occurrences
    );
    println!(
        "persisted_decision_payload_bytes={}",
        census.persisted_decision_payload_bytes
    );
    println!(
        "initial_decision_payload_bytes={}",
        census.initial_decision_payload_bytes
    );
    println!(
        "source_decision_payload_bytes={}",
        census.source_decision_payload_bytes
    );
    println!(
        "observed_decision_payload_bytes={}",
        census.observed_decision_payload_bytes
    );
    println!(
        "decision_observation_payload_bytes={}",
        census.decision_observation_payload_bytes
    );
    println!(
        "decision_candidates_payload_bytes={}",
        census.decision_candidates_payload_bytes
    );
    println!(
        "derived_information_payload_bytes={}",
        census.derived_information_payload_bytes
    );
    println!(
        "exact_parent_source_duplicates={}",
        census.exact_parent_source_duplicates
    );
    println!(
        "exact_parent_source_duplicate_payload_bytes={}",
        census.exact_parent_source_duplicate_payload_bytes
    );
}

fn print_open_profile_v1(profile: RunPlayOpenProfileV1) {
    println!("binary_profile={}", env!("STS_RUN_PLAY_BUILD_PROFILE"));
    for (name, duration) in [
        ("total", profile.total),
        ("workspace_load", profile.workspace_load),
        ("workspace_total", profile.workspace_load_detail.total),
        (
            "workspace_head_load",
            profile.workspace_load_detail.head_load,
        ),
        (
            "workspace_state_read",
            profile.workspace_load_detail.state_read,
        ),
        (
            "workspace_state_deserialize",
            profile.workspace_load_detail.state_deserialize,
        ),
        (
            "workspace_delta_read",
            profile.workspace_load_detail.delta_read,
        ),
        (
            "workspace_delta_deserialize",
            profile.workspace_load_detail.delta_deserialize,
        ),
        (
            "workspace_delta_apply",
            profile.workspace_load_detail.delta_apply,
        ),
        ("checkpoint_decode", profile.checkpoint_decode),
        ("checkpoint_header", profile.checkpoint_decode_detail.header),
        (
            "checkpoint_deserialize",
            profile.checkpoint_decode_detail.deserialize,
        ),
        (
            "checkpoint_materialize",
            profile.checkpoint_decode_detail.runtime_materialization,
        ),
        (
            "checkpoint_validation",
            profile.checkpoint_decode_detail.validation,
        ),
        (
            "checkpoint_validation_preamble",
            profile.checkpoint_decode_detail.validation_preamble,
        ),
        (
            "checkpoint_validation_entries",
            profile
                .checkpoint_decode_detail
                .validation_checkpoint_entries,
        ),
        (
            "checkpoint_validation_archive_joins",
            profile.checkpoint_decode_detail.validation_archive_joins,
        ),
        (
            "checkpoint_canonicality",
            profile.checkpoint_decode_detail.canonicality,
        ),
        ("artifact_validation", profile.exact.artifact_validation),
        ("recovery_resolution", profile.exact.recovery_resolution),
        (
            "environment_construction",
            profile.exact.environment_construction,
        ),
        ("checkpoint_restore", profile.exact.checkpoint_restore),
        ("replay_episode", profile.exact.replay_episode),
        ("final_validation", profile.exact.final_validation),
        ("head_refresh", profile.head_refresh),
        ("hot_snapshot", profile.hot_snapshot),
    ] {
        println!("{name}_ms={:.3}", duration.as_secs_f64() * 1_000.0);
    }
    println!("archive_nodes={}", profile.archive_nodes);
    println!("archive_attempts={}", profile.archive_attempts);
    println!(
        "checkpoint_payload_bytes={}",
        profile.checkpoint_payload_bytes
    );
    println!("checkpoint_count={}", profile.checkpoint_count);
    println!("active_path_nodes={}", profile.active_path_nodes);
    println!("off_path_archive_nodes={}", profile.off_path_archive_nodes);
    println!(
        "active_path_checkpoints={}",
        profile.active_path_checkpoints
    );
    println!(
        "workspace_state_payload_bytes={}",
        profile.workspace_load_detail.state_payload_bytes
    );
    println!(
        "workspace_delta_payload_bytes={}",
        profile.workspace_load_detail.delta_payload_bytes
    );
    println!(
        "workspace_delta_count={}",
        profile.workspace_load_detail.delta_count
    );
    println!(
        "exact_replayed_public_steps={}",
        profile.exact.replayed_public_steps
    );
    println!(
        "cached_head_replayed_public_steps={}",
        profile.cached_head_replayed_public_steps
    );
}

fn compact_action_input_v1<T: AsRef<str>>(parts: &[T]) -> String {
    let full = parts
        .iter()
        .map(|part| part.as_ref())
        .collect::<Vec<_>>()
        .join(" ");
    if parts.len() <= 6 || (parts.len() <= 8 && full.len() <= 240) {
        return full;
    }
    let first = parts[..4]
        .iter()
        .map(|part| part.as_ref())
        .collect::<Vec<_>>()
        .join(" ");
    let last = parts[parts.len() - 2..]
        .iter()
        .map(|part| part.as_ref())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{first} … (+{} args) … {last}", parts.len() - 6)
}

fn parse_shell_actions_v1(
    words: &[&str],
    command: &str,
) -> Result<Vec<selector::RunPlayActionSelectorV1>, String> {
    if words.is_empty() {
        return Err(format!("{command} requires at least one action"));
    }
    let mut selectors = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let word = words[index];
        if word == "select" {
            let start = index + 1;
            index = start;
            while index < words.len() && !shell_action_start_v1(words[index]) {
                let card = words[index];
                if card.is_empty() || card.contains(',') || card.starts_with('@') {
                    return Err(format!(
                        "{command} select needs card names separated by spaces; use select:<card> for an explicit action"
                    ));
                }
                index += 1;
            }
            if index == start {
                return Err(format!("{command} select requires at least one card"));
            }
            selectors.push(parse_action_selector_v1(
                &format!("select:{}", words[start..index].join(",")),
                command,
            )?);
            continue;
        }
        let spaced_verb = match word {
            "play" => Some("play"),
            "go" => Some("route"),
            "use" => Some("use-potion"),
            "claim" | "take" | "buy" | "remove" | "smith" | "toke" | "choose"
            | "discard-potion" | "boss-relic" => Some(word),
            _ => None,
        };
        let mut spec = if let Some(verb) = spaced_verb {
            let name = words
                .get(index + 1)
                .ok_or_else(|| format!("{word} requires a name"))?;
            if name.starts_with('@') || name.contains(':') {
                return Err(format!("{word} requires a plain name after the verb"));
            }
            index += 1;
            format!("{verb}:{name}")
        } else {
            word.to_string()
        };
        if (spec.starts_with("play:") || spec.starts_with("use-potion:"))
            && words
                .get(index + 1)
                .is_some_and(|next| next.starts_with('@'))
        {
            if spec.contains('@') {
                return Err(format!("target specified twice in {command}: {spec}"));
            }
            index += 1;
            spec.push_str(words[index]);
        }
        selectors.push(parse_action_selector_v1(&spec, command)?);
        index += 1;
    }
    Ok(selectors)
}

fn shell_action_start_v1(word: &str) -> bool {
    word.contains(':')
        || matches!(
            word,
            "select"
                | "play"
                | "go"
                | "use"
                | "claim"
                | "take"
                | "buy"
                | "remove"
                | "smith"
                | "toke"
                | "choose"
                | "discard-potion"
                | "boss-relic"
                | "end"
                | "proceed"
                | "cancel"
                | "leave"
                | "advance"
                | "rest"
                | "dig"
                | "lift"
                | "recall"
                | "open-rewards"
                | "open-card"
                | "skip-card"
                | "singing-bowl"
                | "skip-boss-relic"
                | "open-chest"
        )
}

fn parse_shell_single_action_v1(
    words: &[&str],
    command: &str,
) -> Result<selector::RunPlayActionSelectorV1, String> {
    let selectors = parse_shell_actions_v1(words, "shell")?;
    if selectors.len() != 1 {
        return Err(format!(
            "{command} accepts exactly one action; use act for a sequence"
        ));
    }
    Ok(selectors.into_iter().next().expect("checked one action"))
}

fn run_shell_v1(session: PathBuf) -> Result<(), String> {
    let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(session))?;
    let mut known = None;
    let mut displayed = None;
    print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?;
    println!("Enter play Strike @0, claim Relic#0, act <actions...>, or help.");
    let stdin = io::stdin();
    let echo_input = !stdin.is_terminal();
    loop {
        print!("run-play> ");
        io::stdout()
            .flush()
            .map_err(|error| format!("failed to flush run-play prompt: {error}"))?;
        let mut line = String::new();
        if stdin
            .read_line(&mut line)
            .map_err(|error| format!("failed to read run-play command: {error}"))?
            == 0
        {
            break;
        }
        if echo_input && !line.trim().is_empty() {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            println!("\nInput: {}", compact_action_input_v1(&parts));
        }
        let mut words = line.split_whitespace();
        let Some(command) = words.next() else {
            continue;
        };
        let command_started = Instant::now();
        let command_result = (|| -> Result<bool, String> {
            match command {
                "view" => print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?,
                "status" => print_shell_view_v1(&render_status_v1(controller.head())?),
                "show" => print_shell_view_v1(&render_show_v1(controller.head())?),
                "actions" => print_shell_actions_v1(controller.head())?,
                "info" => {
                    let query = words.collect::<Vec<_>>().join(" ");
                    if query.is_empty() {
                        return Err("info requires a name; enter info help for examples".into());
                    }
                    let decision = controller
                        .head()
                        .current_decision
                        .as_ref()
                        .map(|inventory| inventory.public_decision_v1())
                        .transpose()?;
                    print_shell_view_v1(&mechanics_info::render(
                        decision.as_ref(),
                        &query,
                        Locale::En,
                    )?);
                }
                "advice" => {
                    let advice = controller.advise_v1()?;
                    let decision =
                        controller.head().current_decision.as_ref().ok_or_else(|| {
                            "run-play advice requires a decision head".to_string()
                        })?;
                    print!("{}", render_advice_v1(&advice, decision)?);
                }
                "advice-detail" => {
                    let advice = controller.advise_v1()?;
                    let decision =
                        controller.head().current_decision.as_ref().ok_or_else(|| {
                            "run-play advice requires a decision head".to_string()
                        })?;
                    print!("{}", render_advice_v1(&advice, decision)?);
                    let detail = controller.advice_detail_v1()?;
                    print!("{}", render_advice_detail_v1(&detail));
                }
                "compare" | "compare-detail" => {
                    let lines = parse_comparison_lines_v1(words)?;
                    let comparison = controller.compare_action_lines_v1(&lines)?;
                    print!(
                        "{}",
                        render_comparison_v1(&comparison, command == "compare-detail")?
                    );
                }
                "anchor" => {
                    let spec = words
                        .next()
                        .ok_or_else(|| "anchor requires one slash-separated line".to_string())?;
                    if words.next().is_some() {
                        return Err("anchor accepts exactly one line".to_string());
                    }
                    let choices = parse_action_line_v1(spec, "anchor")?;
                    controller.anchor_action_line_v1(&choices)?;
                    println!("Completed: anchor {spec}");
                    print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?;
                }
                "history" => {
                    let limit = words
                        .next()
                        .map(|value| {
                            value
                                .parse::<usize>()
                                .map_err(|error| format!("invalid history limit {value}: {error}"))
                        })
                        .transpose()?
                        .unwrap_or(40);
                    print!("{}", render_history_v1(controller.head(), limit));
                }
                "terminals" => {
                    let limit = words
                        .next()
                        .map(|value| {
                            value.parse::<usize>().map_err(|error| {
                                format!("invalid terminals limit {value}: {error}")
                            })
                        })
                        .transpose()?
                        .unwrap_or(40);
                    print!(
                        "{}",
                        render_terminals_v1(controller.head(), controller.archive(), limit)
                    );
                }
                "act" => {
                    let actions = words.collect::<Vec<_>>();
                    let selectors = parse_shell_actions_v1(&actions, "act")?;
                    if selectors.len() == 1 {
                        controller.choose_action_v1(&selectors[0], true)?;
                    } else {
                        controller.anchor_action_line_v1(&selectors)?;
                    }
                    println!("Completed: act {}", compact_action_input_v1(&actions));
                    print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?;
                }
                "play" | "use" | "claim" | "take" | "buy" | "remove" | "smith" | "toke"
                | "choose" | "discard-potion" | "boss-relic" => {
                    let actions = std::iter::once(command).chain(words).collect::<Vec<_>>();
                    let selector = parse_shell_single_action_v1(&actions, command)?;
                    controller.choose_action_v1(&selector, true)?;
                    println!("Completed: {}", actions.join(" "));
                    print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?;
                }
                "manual" | "auto" => {
                    let action = words
                        .next()
                        .ok_or_else(|| format!("{command} requires one semantic action"))?;
                    if words.next().is_some() {
                        return Err(format!("{command} accepts exactly one semantic action"));
                    }
                    let selector = parse_action_selector_v1(action, command)?;
                    controller.choose_action_v1(&selector, command == "manual")?;
                    println!("Completed: {command} {action}");
                    print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?;
                }
                "back" => {
                    let steps = words
                        .next()
                        .map(|value| {
                            value
                                .parse::<usize>()
                                .map_err(|error| format!("invalid back steps {value}: {error}"))
                        })
                        .transpose()?
                        .unwrap_or(1);
                    controller.back(steps)?;
                    println!("Completed: back {steps}");
                    known = None;
                    print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?;
                }
                "jump" => {
                    let selector = words.next().ok_or_else(|| {
                        "jump requires n<NODE>, source:h<revision>, or target:h<revision>"
                            .to_string()
                    })?;
                    controller.jump(selector)?;
                    println!("Completed: jump {selector}");
                    known = None;
                    print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?;
                }
                "checkpoint" => {
                    controller.checkpoint()?;
                    println!("Completed: checkpoint");
                    print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?;
                }
                "help" => {
                    println!("play <card>[#copy] [@monster] | use <potion> [@monster] | end");
                    println!("claim <reward> | take <card> | buy <item> | remove <card>");
                    println!("smith <card> | toke <card> | boss-relic <relic>");
                    println!("choose <card> | discard-potion <potion>");
                    println!("act play Strike @0 play Defend end (atomic) | view | actions");
                    println!("Legacy play:<card>[@monster] and use-potion:<potion> remain valid.");
                    println!("<displayed option number> | route:x.y | info <name>");
                    println!("status | show | info help");
                    println!(
                        "advice | advice-detail | compare <lines...> | compare-detail <lines...>"
                    );
                    println!("anchor <slash-separated-line> | history [limit] | terminals [limit]");
                    println!("manual <action> | auto <action> | checkpoint | back [steps]");
                    println!("jump <nNODE|source:hN|target:hN> | quit");
                }
                "quit" | "exit" => return Ok(true),
                unknown => {
                    if words.next().is_some() {
                        return Err(format!(
                            "unknown run-play command: {unknown}; enter help for commands"
                        ));
                    }
                    if let Ok(index) = unknown.parse::<usize>() {
                        if shell_requires_semantic_action_v1(
                            controller.head().current_decision.as_ref().map(|d| d.site),
                        ) {
                            return Err("Use the displayed semantic action on reward, card reward, and shop screens; their option numbers can move".to_string());
                        }
                        let reading = displayed
                            .as_ref()
                            .ok_or_else(|| "No displayed decision to choose from".to_string())?;
                        let (choice, label) = shell_numbered_choice_v1(reading, index)?;
                        let source = reading.source.clone().ok_or_else(|| {
                            "Displayed decision has no selectable source".to_string()
                        })?;
                        controller
                            .choose_bound_v1(&decision::RunPlayBoundChoiceV1 { source, choice })?;
                        println!("Completed: {index} ({label})");
                    } else {
                        let selector = parse_action_selector_v1(unknown, "shell")?;
                        controller.choose_action_v1(&selector, true)?;
                        println!("Completed: {unknown}");
                    }
                    print_shell_reading_v1(&mut controller, &mut known, &mut displayed)?;
                }
            }
            Ok(false)
        })();
        match command_result {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) => println!("error: {error}"),
        }
        let elapsed = command_started.elapsed();
        if elapsed >= Duration::from_secs(2) {
            println!(
                "slow command={command} elapsed_ms={:.0}",
                elapsed.as_secs_f64() * 1_000.0
            );
        }
    }
    Ok(())
}

fn shell_numbered_choice_v1(
    reading: &decision::RunPlayDecisionReadingV1,
    index: usize,
) -> Result<(sts_oracle_learning::agent::task::TaskPublicChoiceV1, String), String> {
    if reading.options.is_empty() && reading.selections.len() == 1 {
        let selection = &reading.selections[0];
        if selection.min == 1 && selection.max == 1 {
            let member = selection.members.get(index).ok_or_else(|| {
                format!("Selection member {index} is not in the displayed decision")
            })?;
            let mut choice = selection.choice.clone();
            choice.ordinal_path.push(member.ordinal);
            return Ok((choice, member.label.clone()));
        }
    }
    let option = reading
        .options
        .get(index)
        .ok_or_else(|| format!("Option {index} is not in the displayed decision"))?;
    Ok((option.choice.clone(), option.label.clone()))
}

fn print_shell_reading_v1(
    controller: &mut RunPlayControllerV1,
    known: &mut Option<decision::DecisionKnownContextV1>,
    displayed: &mut Option<decision::RunPlayDecisionReadingV1>,
) -> Result<(), String> {
    let reading = controller.decision_reading_v1(&[], known.as_ref(), Locale::En)?;
    println!(
        "h{} node=n{}",
        controller.head().head_revision,
        render::short_id(controller.head().position.archive_node_id())
    );
    print_shell_view_v1(&shell_view_with_actions_v1(&reading, controller.head()));
    *known = reading.context.clone();
    *displayed = Some(reading);
    Ok(())
}

fn shell_view_with_actions_v1(
    reading: &decision::RunPlayDecisionReadingV1,
    head: &model::RunPlayHeadV1,
) -> String {
    if !shell_requires_semantic_action_v1(head.current_decision.as_ref().map(|d| d.site)) {
        return reading.view.clone();
    }
    let public = head
        .current_decision
        .as_ref()
        .and_then(|inventory| inventory.public_decision_v1().ok());
    let mut output = String::new();
    for line in reading.view.lines() {
        output.push_str(line);
        if let (Some(public), Some((prefix, _))) = (&public, line.split_once(": ")) {
            if let Ok(index) = prefix.parse::<usize>() {
                if let Some(option) = reading.options.get(index) {
                    if let Ok(action) = semantic_action_for_choice_v1(public, &option.choice) {
                        output.push_str("  [");
                        output.push_str(&action);
                        output.push(']');
                    }
                }
            }
        }
        output.push('\n');
    }
    output
}

fn shell_requires_semantic_action_v1(site: Option<TaskDecisionSiteV1>) -> bool {
    matches!(
        site,
        Some(
            TaskDecisionSiteV1::Reward | TaskDecisionSiteV1::Shop | TaskDecisionSiteV1::CardReward
        )
    )
}

fn print_shell_view_v1(view: &str) {
    const WIDTH: usize = 70;
    for line in view.lines() {
        if shell_cell_width_v1(line) <= WIDTH {
            println!("{line}");
            continue;
        }
        let leading = line.chars().take_while(|c| c.is_whitespace()).count();
        let indent = " ".repeat(leading + 2);
        let mut row = " ".repeat(leading);
        for word in line.split_whitespace() {
            if !row.trim().is_empty()
                && shell_cell_width_v1(&row) + shell_cell_width_v1(word) + 1 > WIDTH
            {
                println!("{row}");
                row = indent.clone();
            }
            if !row.is_empty() && !row.ends_with(' ') {
                row.push(' ');
            }
            row.push_str(word);
        }
        println!("{row}");
    }
}

fn shell_cell_width_v1(text: &str) -> usize {
    text.chars()
        .map(|character| if character.is_ascii() { 1 } else { 2 })
        .sum()
}

fn print_shell_actions_v1(head: &model::RunPlayHeadV1) -> Result<(), String> {
    let Some(inventory) = head.current_decision.as_ref() else {
        println!("No actions at this terminal.");
        return Ok(());
    };
    let decision = inventory.public_decision_v1()?;
    for candidate in &inventory.candidates {
        if let Some(choice) = &candidate.direct_choice {
            println!("{}", semantic_action_for_choice_v1(&decision, choice)?);
        }
    }
    Ok(())
}

fn run_agent_shell_v1(
    session: PathBuf,
    knowledge_pack_reference: Option<String>,
    manual: bool,
    locale: Locale,
) -> Result<(), String> {
    let paths = RunPlayPathsV1::new(session);
    let head = paths.load_head()?;
    let capabilities = AgentCapabilitiesV1::new(if manual {
        AgentPermissionEnvelopeV1::public_manual_v1(knowledge_pack_reference)
    } else {
        AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(knowledge_pack_reference)
    });
    let stdin = io::stdin();
    let stdout = io::stdout();
    if matches!(head.position, model::RunPlayPositionV1::Terminal { .. }) {
        return run_terminal_agent_jsonl_v1(
            &head,
            capabilities,
            BufReader::new(stdin.lock()),
            stdout.lock(),
            locale,
        );
    }
    let mut controller = RunPlayControllerV1::open(paths)?;
    run_agent_jsonl_v1(
        &mut controller,
        capabilities,
        BufReader::new(stdin.lock()),
        stdout.lock(),
        locale,
    )
}

fn parse_comparison_lines_v1<'a>(
    specs: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<Vec<selector::RunPlayActionSelectorV1>>, String> {
    specs
        .into_iter()
        .map(|spec| parse_action_line_v1(spec, "compare"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_cli_separates_route_examples_and_full_grid() {
        for (arguments, full) in [
            (vec!["run_play", "map", "--session", "scratch"], false),
            (
                vec!["run_play", "map", "--session", "scratch", "full"],
                true,
            ),
        ] {
            let cli = Cli::try_parse_from(arguments).unwrap();
            assert!(matches!(cli.command, Command::Map { query, .. }
                if matches!(query, Some(MapQueryV1::Full)) == full));
        }
    }

    #[test]
    fn scratch_back_cli_lists_retained_targets() {
        let listed =
            Cli::try_parse_from(["run_play", "back", "--session", "scratch", "--list"]).unwrap();
        assert!(matches!(listed.command, Command::Back { list: true, .. }));
        let json = Cli::try_parse_from([
            "run_play",
            "back",
            "--session",
            "scratch",
            "--list",
            "--json",
        ])
        .unwrap();
        assert!(matches!(
            json.command,
            Command::Back {
                list: true,
                json: true,
                ..
            }
        ));
        let restore = Cli::try_parse_from([
            "run_play",
            "back",
            "--session",
            "scratch",
            "--request",
            "return.json",
        ])
        .unwrap();
        assert!(matches!(
            restore.command,
            Command::Back {
                request: Some(_),
                list: false,
                ..
            }
        ));
        assert!(
            Cli::try_parse_from(["run_play", "back", "--session", "scratch", "--json"]).is_err()
        );
        assert!(Cli::try_parse_from([
            "run_play",
            "back",
            "--session",
            "scratch",
            "--request",
            "return.json",
            "--steps",
            "1"
        ])
        .is_err());
    }

    #[test]
    fn scratch_back_cli_rejects_list_with_a_restore_target() {
        assert!(Cli::try_parse_from([
            "run_play",
            "back",
            "--session",
            "scratch",
            "--list",
            "--floor",
            "49"
        ])
        .is_err());
    }

    #[test]
    fn scratch_back_cli_separates_floor_and_mark_targets() {
        let floor =
            Cli::try_parse_from(["run_play", "back", "--session", "scratch", "--floor", "8"])
                .unwrap();
        assert!(matches!(
            floor.command,
            Command::Back {
                floor: Some(8),
                mark: None,
                ..
            }
        ));
        let marked =
            Cli::try_parse_from(["run_play", "back", "--session", "scratch", "--mark", "shop"])
                .unwrap();
        assert!(
            matches!(marked.command, Command::Back { floor: None, mark: Some(name), .. } if name == "shop")
        );
        assert!(Cli::try_parse_from([
            "run_play",
            "back",
            "--session",
            "scratch",
            "--floor",
            "8",
            "--mark",
            "shop",
        ])
        .is_err());
        let saved =
            Cli::try_parse_from(["run_play", "mark", "--session", "scratch", "--name", "shop"])
                .unwrap();
        assert!(
            matches!(saved.command, Command::Mark { name: Some(name), drop: None, .. } if name == "shop")
        );
        assert!(Cli::try_parse_from([
            "run_play",
            "mark",
            "--session",
            "scratch",
            "--name",
            "shop",
            "--drop",
            "shop",
        ])
        .is_err());
    }

    #[test]
    fn long_action_input_keeps_ends_and_omission_count() {
        let short = ["play", "Bash@0"];
        assert_eq!(compact_action_input_v1(&short), "play Bash@0");
        let long = (0..47)
            .map(|index| format!("action:{index}"))
            .collect::<Vec<_>>();
        let reading = compact_action_input_v1(&long);
        assert!(reading.starts_with("action:0 action:1 action:2 action:3 … (+41 args)"));
        assert!(reading.ends_with("action:45 action:46"));
        assert!(!reading.contains("action:20"));
    }

    #[test]
    fn reroot_target_is_fresh_and_tied_to_the_current_build() {
        let directory = decision_tests::TestDirectory::new();
        let source = directory.0.join("cat-run");
        let first = next_reroot_target_v1(&source).unwrap();
        assert!(first.starts_with(&directory.0));
        assert!(first
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("cat-run-current-"));
        fs::create_dir(&first).unwrap();
        let second = next_reroot_target_v1(&source).unwrap();
        assert_eq!(
            second.file_name().unwrap().to_string_lossy(),
            format!("{}-2", first.file_name().unwrap().to_string_lossy())
        );
        assert_eq!(next_reroot_target_v1(&first).unwrap(), second);
    }

    #[test]
    fn reroot_current_accepts_focused_defaults() {
        let cli = Cli::try_parse_from(["run_play", "reroot-current"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::RerootCurrent {
                session: None,
                at: None,
                target: None,
                focus: false
            }
        ));
        let historical = Cli::try_parse_from([
            "run_play",
            "reroot-current",
            "--session",
            "old-session",
            "--at",
            "n123",
            "--target",
            "new-session",
        ])
        .unwrap();
        assert!(matches!(
            historical.command,
            Command::RerootCurrent {
                at: Some(ref node),
                ..
            } if node == "n123"
        ));
    }

    #[test]
    fn shell_accepts_spaced_combat_actions_with_existing_target_and_copy_rules() {
        let actions = parse_shell_actions_v1(
            &[
                "play",
                "Strike+0#1",
                "@2",
                "use",
                "WeakPotion#0@1",
                "play:Defend",
                "end",
            ],
            "act",
        )
        .unwrap();
        assert_eq!(
            actions,
            vec![
                selector::RunPlayActionSelectorV1::PlayCard {
                    card: selector::NamedOccurrenceV1 {
                        name: "Strike".into(),
                        upgrades: Some(0),
                        occurrence: Some(1),
                    },
                    target_monster_slot: Some(2),
                },
                selector::RunPlayActionSelectorV1::UsePotion {
                    potion: selector::NamedOccurrenceV1 {
                        name: "WeakPotion".into(),
                        upgrades: None,
                        occurrence: Some(0),
                    },
                    target_monster_slot: Some(1),
                },
                parse_action_selector_v1("play:Defend", "act").unwrap(),
                selector::RunPlayActionSelectorV1::EndTurn,
            ]
        );
        assert_eq!(
            parse_shell_actions_v1(&["play", "SeverSoul+@0"], "shell").unwrap(),
            parse_action_line_v1("play:SeverSoul+@0", "shell").unwrap()
        );
    }

    #[test]
    fn shell_accepts_the_displayed_map_action_and_powershell_comma_coercion() {
        let expected = vec![selector::RunPlayActionSelectorV1::ChooseRoute { x: 2, y: 2 }];
        assert_eq!(
            parse_shell_actions_v1(&["go", "2,2"], "act").unwrap(),
            expected
        );
        assert_eq!(
            parse_shell_actions_v1(&["go", "2 2"], "act").unwrap(),
            expected
        );
        assert_eq!(
            parse_shell_actions_v1(&["route:2.2"], "act").unwrap(),
            expected
        );
    }

    #[test]
    fn shell_rejects_incomplete_or_ambiguous_spaced_actions_before_execution() {
        for words in [
            vec!["play"],
            vec!["use", "@0"],
            vec!["play", "Strike@0", "@1"],
            vec!["play", "Strike", "@x"],
        ] {
            assert!(parse_shell_actions_v1(&words, "act").is_err(), "{words:?}");
        }
    }

    #[test]
    fn spaced_multi_card_select_is_one_action_in_shell_and_cli() {
        let words = ["select", "Strike+0#0", "Strike+0#1", "Strike+0#2"];
        let expected =
            parse_action_line_v1("select:Strike+0#0,Strike+0#1,Strike+0#2", "test").unwrap();
        assert_eq!(parse_shell_actions_v1(&words, "shell").unwrap(), expected);
        for verb in ["preview", "act"] {
            let mut args = vec!["run_play", verb, "--session", "fixture"];
            args.extend(words);
            let cli = Cli::try_parse_from(args).unwrap();
            let actions = match cli.command {
                Command::Preview { actions, .. } | Command::Act { actions, .. } => actions,
                _ => unreachable!(),
            };
            assert_eq!(
                parse_shell_actions_v1(
                    &actions.iter().map(String::as_str).collect::<Vec<_>>(),
                    verb
                )
                .unwrap(),
                expected
            );
        }
        assert_eq!(
            parse_shell_actions_v1(&["select:Strike+0#0,Strike+0#1"], "act").unwrap(),
            parse_action_line_v1("select:Strike+0#0,Strike+0#1", "test").unwrap()
        );
        assert_eq!(
            parse_shell_actions_v1(
                &["select", "Entrench+", "play", "PommelStrike+@1", "end"],
                "preview"
            )
            .unwrap(),
            parse_action_line_v1("select:Entrench+/play:PommelStrike+@1/end", "test").unwrap()
        );
        for invalid in [
            vec!["select"],
            vec!["select", "end"],
            vec!["select", "Strike+0#0,Strike+0#1"],
        ] {
            assert!(
                parse_shell_actions_v1(&invalid, "act").is_err(),
                "{invalid:?}"
            );
        }
    }

    #[test]
    fn shell_spaced_reward_and_shop_actions_keep_typed_name_and_copy_rules() {
        for (words, canonical) in [
            (vec!["claim", "BronzeScales#0"], "claim:BronzeScales#0"),
            (vec!["take", "Rupture+0#1"], "take:Rupture+0#1"),
            (vec!["buy", "Metallicize#0"], "buy:Metallicize#0"),
            (vec!["remove", "Strike#2"], "remove:Strike#2"),
            (vec!["smith", "FiendFire#0"], "smith:FiendFire#0"),
            (vec!["toke", "Defend#1"], "toke:Defend#1"),
            (vec!["choose", "Strike#0"], "choose:Strike#0"),
            (
                vec!["discard-potion", "EnergyPotion#0"],
                "discard-potion:EnergyPotion#0",
            ),
            (vec!["boss-relic", "RunicDome#0"], "boss-relic:RunicDome#0"),
        ] {
            assert_eq!(
                parse_shell_single_action_v1(&words, words[0]).unwrap(),
                parse_action_selector_v1(canonical, "shell").unwrap(),
                "{words:?}"
            );
        }
    }

    #[test]
    fn shell_rejects_incomplete_or_extra_spaced_reward_actions() {
        for words in [
            vec!["claim"],
            vec!["take", "@0"],
            vec!["buy", "Metallicize", "extra"],
            vec!["claim", "BronzeScales", "end"],
        ] {
            assert!(
                parse_shell_single_action_v1(&words, words[0]).is_err(),
                "{words:?}"
            );
        }
    }

    #[test]
    fn shell_numbered_combat_selection_uses_displayed_member_and_domain_ordinal() {
        let reading = decision::RunPlayDecisionReadingV1 {
            control: decision::DecisionControlV1::Combat,
            view: "Selection 0 | Exhaust from hand | Choose 1\n  0: Wound\n".into(),
            summary: "Combat\n".into(),
            context: None,
            source: None,
            options: vec![],
            selections: vec![decision::DecisionSelectionV1 {
                label: "Exhaust from hand".into(),
                choice: sts_oracle_learning::agent::task::TaskPublicChoiceV1::one(7),
                min: 1,
                max: 1,
                members: vec![decision::DecisionSelectionMemberV1 {
                    ordinal: 9,
                    label: "Wound".into(),
                }],
            }],
            can_collect_rewards: false,
            state: None,
            last_choice: None,
        };
        let (choice, label) = shell_numbered_choice_v1(&reading, 0).unwrap();
        assert_eq!(choice.ordinal_path, vec![7, 9]);
        assert_eq!(label, "Wound");
        assert!(shell_numbered_choice_v1(&reading, 1).is_err());
    }

    #[test]
    fn new_accepts_the_heart_victory_goal() {
        let cli = Cli::try_parse_from([
            "run_play",
            "new",
            "--session",
            "fixture",
            "--seed",
            "0",
            "--ascension",
            "20",
            "--goal",
            "heart-victory",
        ])
        .expect("Heart goal command");

        let Command::New { goal, .. } = cli.command else {
            panic!("expected new command");
        };
        assert!(matches!(goal, RunGoalArgV1::HeartVictory));
    }

    #[test]
    fn act_accepts_a_transactional_sequence_without_shell_separators() {
        let cli = Cli::try_parse_from([
            "run_play",
            "act",
            "--session",
            "fixture",
            "play:Corruption",
            "play:Impervious",
            "play:Strike+0#1@1",
            "end",
        ])
        .expect("multi-action transaction");

        let Command::Act {
            at,
            actions,
            auto_combat,
            ..
        } = cli.command
        else {
            panic!("expected act command");
        };
        assert_eq!(
            actions,
            vec![
                "play:Corruption",
                "play:Impervious",
                "play:Strike+0#1@1",
                "end"
            ]
        );
        assert_eq!(at, None);
        assert!(!auto_combat);
    }

    #[test]
    fn promote_branch_requires_explicit_source_and_target_sessions() {
        let cli = Cli::try_parse_from([
            "run_play",
            "promote-branch",
            "--source",
            "verified-probe",
            "--target",
            "durable-run",
        ])
        .expect("parse promote-branch");
        match cli.command {
            Command::PromoteBranch { source, target } => {
                assert_eq!(source, PathBuf::from("verified-probe"));
                assert_eq!(target, PathBuf::from("durable-run"));
            }
            _ => panic!("expected promote-branch command"),
        }
        assert!(
            Cli::try_parse_from(["run_play", "promote-branch", "--source", "verified-probe",])
                .is_err()
        );
    }

    #[test]
    fn counterfactual_branch_requires_a_probe_and_fresh_output() {
        let cli = Cli::try_parse_from([
            "run_play",
            "counterfactual-branch",
            "--session",
            "managed/probe",
            "--output",
            "evidence/branch.json",
        ])
        .expect("parse counterfactual-branch");
        match cli.command {
            Command::CounterfactualBranch {
                session,
                managed_root,
                output,
                max_steps,
            } => {
                assert_eq!(session, PathBuf::from("managed/probe"));
                assert_eq!(managed_root, PathBuf::from(".oracle-lab/run-play-tmp"));
                assert_eq!(output, PathBuf::from("evidence/branch.json"));
                assert_eq!(max_steps, DEFAULT_COUNTERFACTUAL_BRANCH_MAX_STEPS_V1);
            }
            _ => panic!("expected counterfactual-branch command"),
        }
        assert!(Cli::try_parse_from([
            "run_play",
            "counterfactual-branch",
            "--session",
            "managed/probe",
        ])
        .is_err());
    }

    #[test]
    fn preview_and_act_accept_exact_historical_nodes_without_shell_macros() {
        let preview = Cli::try_parse_from([
            "run_play",
            "preview",
            "--session",
            "fixture",
            "--at",
            "n0123456789ab",
            "--artifact",
            "prefix.json",
            "play:Bash@0",
            "play:Strike+0#1@0",
        ])
        .expect("historical preview command");
        let Command::Preview {
            at,
            actions,
            detail,
            artifact,
            ..
        } = preview.command
        else {
            panic!("expected preview command");
        };
        assert_eq!(at.as_deref(), Some("n0123456789ab"));
        assert_eq!(actions, vec!["play:Bash@0", "play:Strike+0#1@0"]);
        assert!(!detail);
        assert_eq!(artifact, Some(PathBuf::from("prefix.json")));

        let act = Cli::try_parse_from([
            "run_play",
            "act",
            "--session",
            "fixture",
            "--at",
            "n0123456789ab",
            "play:Bash@0",
        ])
        .expect("historical act command");
        let Command::Act { at, actions, .. } = act.command else {
            panic!("expected act command");
        };
        assert_eq!(at.as_deref(), Some("n0123456789ab"));
        assert_eq!(actions, vec!["play:Bash@0"]);
    }

    #[test]
    fn state_fingerprint_accepts_an_exact_historical_node() {
        let parsed = Cli::try_parse_from([
            "run_play",
            "state-fingerprint",
            "--session",
            "fixture",
            "--at",
            "n0123456789ab",
        ])
        .expect("state fingerprint command");
        let Command::StateFingerprint { at, .. } = parsed.command else {
            panic!("expected state fingerprint command");
        };
        assert_eq!(at.as_deref(), Some("n0123456789ab"));
    }

    #[test]
    fn paired_root_prepare_requires_explicit_isolated_inputs_and_outputs() {
        let parsed = Cli::try_parse_from([
            "run_play",
            "paired-root-prepare",
            "--session",
            "official",
            "--at",
            "h11",
            "--java-checkpoint",
            "checkpoint.json",
            "--java-plan",
            "plan.json",
            "--java-live-output",
            "java-live",
            "--store",
            "paired-store",
            "--attempt",
            "attempt-001",
            "--report",
            "prepare.json",
        ])
        .expect("paired-root prepare command");

        let Command::PairedRootPrepare {
            session,
            at,
            java_checkpoint,
            java_plan,
            java_live_output,
            store,
            attempt,
            report,
        } = parsed.command
        else {
            panic!("expected paired-root prepare command");
        };
        assert_eq!(session, PathBuf::from("official"));
        assert_eq!(at, "h11");
        assert_eq!(java_checkpoint, PathBuf::from("checkpoint.json"));
        assert_eq!(java_plan, PathBuf::from("plan.json"));
        assert_eq!(java_live_output, PathBuf::from("java-live"));
        assert_eq!(store, PathBuf::from("paired-store"));
        assert_eq!(attempt, PathBuf::from("attempt-001"));
        assert_eq!(report, PathBuf::from("prepare.json"));
    }

    #[test]
    fn paired_root_action_observation_and_preview_use_stable_handles() {
        let root = "state-root-v2-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let observed = Cli::try_parse_from([
            "run_play",
            "paired-root-observe-actions",
            "--root",
            root,
            "--store",
            "paired-store",
            "--report",
            "actions.json",
        ])
        .expect("paired-root action observation command");
        let Command::PairedRootObserveActions {
            root: observed_root,
            ..
        } = observed.command
        else {
            panic!("expected paired-root action observation command");
        };
        assert_eq!(observed_root, root);

        let parsed = Cli::try_parse_from([
            "run_play",
            "paired-root-preview-action",
            "--session",
            "official",
            "--root",
            root,
            "--activation",
            "state-root-activation-v2-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "--action",
            "paired-action-v1-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "--java-live-output",
            "java-live",
            "--store",
            "paired-store",
            "--attempt",
            "attempt-002",
            "--report",
            "preview.json",
        ])
        .expect("paired-root preview command");

        let Command::PairedRootPreviewAction {
            root,
            activation,
            action,
            ..
        } = parsed.command
        else {
            panic!("expected paired-root preview command");
        };
        assert!(root.starts_with("state-root-v2-"));
        assert!(activation.starts_with("state-root-activation-v2-"));
        assert!(action.starts_with("paired-action-v1-"));
    }

    #[test]
    fn paired_root_native_materialization_and_resume_preview_are_target_bound() {
        let target =
            "state-root-v2-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let materialize = Cli::try_parse_from([
            "run_play",
            "paired-root-materialize-native",
            "--target-root",
            target,
            "--store",
            "paired-store",
            "--output",
            "native-payload",
            "--report",
            "materialized.json",
        ])
        .expect("paired-root native materialization command");
        let Command::PairedRootMaterializeNative { target_root, .. } = materialize.command else {
            panic!("expected paired-root native materialization command");
        };
        assert_eq!(target_root, target);

        let resume = Cli::try_parse_from([
            "run_play",
            "paired-root-resume-preview",
            "--target-root",
            target,
            "--action",
            "paired-action-v1-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "--run-root",
            "run-play",
            "--java-live-output",
            "java-live",
            "--store",
            "paired-store",
            "--recovery-attempt",
            "recover-work",
            "--preview-attempt",
            "preview-work",
            "--activation-report",
            "activation.json",
            "--recovery-report",
            "recovery.json",
            "--preview-report",
            "preview.json",
        ])
        .expect("paired-root resume-preview command");
        let Command::PairedRootResumePreview {
            target_root,
            action,
            recovery_attempt,
            preview_attempt,
            ..
        } = resume.command
        else {
            panic!("expected paired-root resume-preview command");
        };
        assert_eq!(target_root, target);
        assert!(action.starts_with("paired-action-v1-"));
        assert_eq!(recovery_attempt, PathBuf::from("recover-work"));
        assert_eq!(preview_attempt, PathBuf::from("preview-work"));

        let witness = Cli::try_parse_from([
            "run_play",
            "paired-root-resume-witness",
            "--target-root",
            target,
            "--witness-plan",
            "witness-plan.json",
            "--run-root",
            "run-play",
            "--java-live-output",
            "java-live",
            "--store",
            "paired-store",
            "--recovery-attempt",
            "recover-work",
            "--transaction-attempt",
            "transaction-work",
            "--activation-report",
            "activation.json",
            "--recovery-report",
            "recovery.json",
            "--transaction-report",
            "transaction.json",
        ])
        .expect("paired-root resume-witness command");
        let Command::PairedRootResumeWitness {
            target_root,
            witness_plan,
            transaction_attempt,
            ..
        } = witness.command
        else {
            panic!("expected paired-root resume-witness command");
        };
        assert_eq!(target_root, target);
        assert_eq!(witness_plan, PathBuf::from("witness-plan.json"));
        assert_eq!(transaction_attempt, PathBuf::from("transaction-work"));
    }

    #[test]
    fn paired_root_cas_accepts_an_ordered_receipt_chain() {
        let parsed = Cli::try_parse_from([
            "run_play",
            "paired-root-cas-proof",
            "--store",
            "paired-store",
            "--head",
            "two-step-proof",
            "--root",
            "state-root-v2-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--receipt",
            "paired-transition-receipt-v1-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "--receipt",
            "paired-transition-receipt-v1-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "--report",
            "cas.json",
        ])
        .expect("paired-root CAS chain command");

        let Command::PairedRootCasProof { receipt, .. } = parsed.command else {
            panic!("expected paired-root CAS proof command");
        };
        assert_eq!(receipt.len(), 2);
        assert!(receipt[0].contains("bbbb"));
        assert!(receipt[1].contains("cccc"));
    }

    #[test]
    fn exact_action_roots_reject_head_revision_metadata() {
        assert!(require_exact_node_selector_v1("n0123456789ab", "preview").is_ok());
        let error = require_exact_node_selector_v1("h108", "act")
            .expect_err("head revision must not identify an exact action root");
        assert!(error.contains("audit metadata"));
    }

    #[test]
    fn combat_research_producer_aliases_are_typed_and_closed() {
        assert_eq!(
            combat_research_producer_name_v1("resident").expect("resident alias"),
            RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1
        );
        assert_eq!(
            combat_research_producer_name_v1("proposal").expect("proposal alias"),
            COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1
        );
        assert_eq!(
            combat_research_producer_name_v1("atomic").expect("atomic alias"),
            ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1
        );
        assert!(combat_research_producer_name_v1("baseline").is_err());
    }

    #[test]
    fn anchor_combat_plan_accepts_a_report_candidate_index() {
        let cli = Cli::try_parse_from([
            "run_play",
            "anchor-combat-plan",
            "--session",
            "fixture",
            "--candidate",
            "7",
            "resident",
        ])
        .expect("indexed combat plan command");

        let Command::AnchorCombatPlan {
            session,
            artifact,
            candidate,
            effort,
            producer,
        } = cli.command
        else {
            panic!("expected anchor-combat-plan command");
        };
        assert_eq!(session, PathBuf::from("fixture"));
        assert_eq!(artifact, None);
        assert_eq!(candidate, Some(7));
        assert_eq!(effort, AgentCombatResearchEffortV1::Standard);
        assert_eq!(producer, "resident");
    }

    #[test]
    fn anchor_combat_plan_accepts_a_typed_artifact() {
        let cli = Cli::try_parse_from([
            "run_play",
            "anchor-combat-plan",
            "--session",
            "fixture",
            "--artifact",
            "research.json",
            "--candidate",
            "2",
            "proposal",
        ])
        .expect("artifact-backed combat plan command");

        let Command::AnchorCombatPlan {
            artifact,
            candidate,
            producer,
            ..
        } = cli.command
        else {
            panic!("expected anchor-combat-plan command");
        };
        assert_eq!(artifact, Some(PathBuf::from("research.json")));
        assert_eq!(candidate, Some(2));
        assert_eq!(producer, "proposal");
    }

    #[test]
    fn combat_research_accepts_a_fresh_typed_artifact_path() {
        let cli = Cli::try_parse_from([
            "run_play",
            "combat-research",
            "--session",
            "fixture",
            "--effort",
            "expanded",
            "--artifact",
            "research.json",
        ])
        .expect("combat research artifact command");

        let Command::CombatResearch {
            session,
            effort,
            detail,
            artifact,
        } = cli.command
        else {
            panic!("expected combat-research command");
        };
        assert_eq!(session, PathBuf::from("fixture"));
        assert_eq!(effort, AgentCombatResearchEffortV1::Expanded);
        assert!(!detail);
        assert_eq!(artifact, Some(PathBuf::from("research.json")));
    }

    #[test]
    fn anchor_combat_query_accepts_one_typed_artifact() {
        let cli = Cli::try_parse_from([
            "run_play",
            "anchor-combat-query",
            "--session",
            "fixture",
            "--artifact",
            "query.json",
        ])
        .expect("combat query artifact anchor command");

        let Command::AnchorCombatQuery {
            session,
            artifact,
            candidate,
        } = cli.command
        else {
            panic!("expected anchor-combat-query command");
        };
        assert_eq!(session, PathBuf::from("fixture"));
        assert_eq!(artifact, PathBuf::from("query.json"));
        assert_eq!(candidate, None);
    }

    #[test]
    fn anchor_combat_query_accepts_an_explicit_frontier_candidate() {
        let cli = Cli::try_parse_from([
            "run_play",
            "anchor-combat-query",
            "--session",
            "fixture",
            "--artifact",
            "query.json",
            "--candidate",
            "4",
        ])
        .expect("combat query frontier anchor command");

        let Command::AnchorCombatQuery { candidate, .. } = cli.command else {
            panic!("expected anchor-combat-query command");
        };
        assert_eq!(candidate, Some(4));
    }

    #[test]
    fn combat_feed_anchor_requires_one_typed_global_frontier_index() {
        let cli = Cli::try_parse_from([
            "run_play",
            "combat-feed-anchor",
            "--session",
            "fixture",
            "--artifact",
            "challenge.json",
            "--frontier",
            "2",
        ])
        .expect("combat Feed anchor command");

        let Command::CombatFeedAnchor {
            artifact, frontier, ..
        } = cli.command
        else {
            panic!("expected combat Feed anchor command");
        };
        assert_eq!(artifact, PathBuf::from("challenge.json"));
        assert_eq!(frontier, 2);
        assert!(Cli::try_parse_from([
            "run_play",
            "combat-feed-anchor",
            "--session",
            "fixture",
            "--artifact",
            "challenge.json",
        ])
        .is_err());
    }

    #[test]
    fn combat_research_accepts_fast_witness_effort() {
        let cli = Cli::try_parse_from([
            "run_play",
            "combat-research",
            "--session",
            "fixture",
            "--effort",
            "fast-witness",
        ])
        .expect("fast-witness combat research command");

        let Command::CombatResearch { effort, .. } = cli.command else {
            panic!("expected combat-research command");
        };
        assert_eq!(effort, AgentCombatResearchEffortV1::FastWitness);
    }

    #[test]
    fn compare_accepts_a_fresh_exact_prefix_artifact_path() {
        let cli = Cli::try_parse_from([
            "run_play",
            "compare",
            "--session",
            "fixture",
            "--artifact",
            "prefix.json",
            "play:Defend+0#0/end",
        ])
        .expect("exact prefix artifact command");

        let Command::Compare {
            session,
            at,
            detail,
            artifact,
            lines,
        } = cli.command
        else {
            panic!("expected compare command");
        };
        assert_eq!(session, PathBuf::from("fixture"));
        assert_eq!(at, None);
        assert!(!detail);
        assert_eq!(artifact, Some(PathBuf::from("prefix.json")));
        assert_eq!(lines, vec!["play:Defend+0#0/end"]);
    }

    #[test]
    fn compare_accepts_a_read_only_retained_node_selector() {
        let cli = Cli::try_parse_from([
            "run_play",
            "compare",
            "--session",
            "fixture",
            "--at",
            "n0123456789ab",
            "--detail",
            "play:ShrugItOff/play:DemonForm/end",
        ])
        .expect("compare at retained node command");

        let Command::Compare {
            at,
            detail,
            artifact,
            ..
        } = cli.command
        else {
            panic!("expected compare command");
        };
        assert_eq!(at.as_deref(), Some("n0123456789ab"));
        assert!(detail);
        assert_eq!(artifact, None);

        assert!(Cli::try_parse_from([
            "run_play",
            "compare",
            "--session",
            "fixture",
            "--at",
            "n0123456789ab",
            "--artifact",
            "prefix.json",
            "end",
        ])
        .is_err());
    }

    #[test]
    fn history_accepts_a_typed_strategic_only_filter() {
        let cli = Cli::try_parse_from([
            "run_play",
            "history",
            "--session",
            "fixture",
            "--limit",
            "7",
            "--strategic-only",
        ])
        .expect("strategic history command");

        let Command::History {
            limit,
            strategic_only,
            combat_detail,
            ..
        } = cli.command
        else {
            panic!("expected history command");
        };
        assert_eq!(limit, Some(7));
        assert!(strategic_only);
        assert!(!combat_detail);
    }

    #[test]
    fn history_accepts_combat_detail_and_rejects_conflicting_filters() {
        let cli = Cli::try_parse_from([
            "run_play",
            "history",
            "--session",
            "fixture",
            "--limit",
            "9",
            "--combat-detail",
            "--at",
            "nabc123",
        ])
        .expect("combat detail history command");

        let Command::History {
            limit,
            strategic_only,
            combat_detail,
            at,
            ..
        } = cli.command
        else {
            panic!("expected history command");
        };
        assert_eq!(limit, Some(9));
        assert!(!strategic_only);
        assert!(combat_detail);
        assert_eq!(at.as_deref(), Some("nabc123"));
        assert!(Cli::try_parse_from([
            "run_play",
            "history",
            "--session",
            "fixture",
            "--strategic-only",
            "--combat-detail",
        ])
        .is_err());
        assert!(Cli::try_parse_from([
            "run_play",
            "history",
            "--session",
            "fixture",
            "--at",
            "nabc123",
        ])
        .is_err());
    }

    #[test]
    fn history_journey_has_explicit_first_page_and_detail_controls() {
        let cli = Cli::try_parse_from([
            "run_play",
            "history",
            "--session",
            "fixture",
            "--journey",
            "--first",
            "8",
            "--detail",
        ])
        .expect("journey history command");

        let Command::History {
            journey,
            first,
            last,
            after_depth,
            count,
            detail,
            ..
        } = cli.command
        else {
            panic!("expected history command");
        };
        assert!(journey);
        assert_eq!(first, Some(8));
        assert_eq!(last, None);
        assert_eq!(after_depth, None);
        assert_eq!(count, None);
        assert!(detail);

        assert!(Cli::try_parse_from([
            "run_play",
            "history",
            "--session",
            "fixture",
            "--journey",
            "--first",
            "8",
            "--last",
            "8",
        ])
        .is_err());
    }

    #[test]
    fn construction_history_accepts_typed_site_and_json_projection() {
        let cli = Cli::try_parse_from([
            "run_play",
            "construction-history",
            "--session",
            "fixture",
            "--site",
            "card_reward",
            "--candidate-card",
            "IronWave",
            "--all-boundaries",
            "--detail",
            "--json",
        ])
        .expect("construction history command");

        let Command::ConstructionHistory {
            site,
            candidate_card,
            added_card,
            all_boundaries,
            detail,
            json,
            ..
        } = cli.command
        else {
            panic!("expected construction history command");
        };
        assert_eq!(
            site.as_deref().map(parse_construction_site_v1).transpose(),
            Ok(Some(TaskDecisionSiteV1::CardReward))
        );
        assert_eq!(
            candidate_card
                .as_deref()
                .map(parse_construction_card_v1)
                .transpose(),
            Ok(Some(CardId::IronWave))
        );
        assert_eq!(added_card, None);
        assert!(json);
        assert!(all_boundaries);
        assert!(detail);
        assert!(parse_construction_site_v1("combat").is_err());
        assert!(parse_construction_site_v1("unknown").is_err());
        assert!(parse_construction_card_v1("iron-wave").is_err());
    }

    #[test]
    fn construction_history_added_card_filter_is_explicit_and_exclusive() {
        let cli = Cli::try_parse_from([
            "run_play",
            "construction-history",
            "--session",
            "fixture",
            "--added-card",
            "RecklessCharge",
        ])
        .expect("added-card construction history command");
        let Command::ConstructionHistory {
            candidate_card,
            added_card,
            ..
        } = cli.command
        else {
            panic!("expected construction history command");
        };
        assert_eq!(candidate_card, None);
        assert_eq!(added_card.as_deref(), Some("RecklessCharge"));

        assert!(Cli::try_parse_from([
            "run_play",
            "construction-history",
            "--session",
            "fixture",
            "--candidate-card",
            "RecklessCharge",
            "--added-card",
            "RecklessCharge",
        ])
        .is_err());
    }

    #[test]
    fn victory_dossier_requires_explicit_output_and_accepts_evidence_sources() {
        let cli = Cli::try_parse_from([
            "run_play",
            "victory-dossier",
            "--session",
            "fixture",
            "--output",
            "victory.json",
            "--transcript-source",
            "proposal.jsonl",
            "--transcript-source",
            "evaluations",
        ])
        .expect("victory dossier command");

        let Command::VictoryDossier {
            output,
            transcript_sources,
            ..
        } = cli.command
        else {
            panic!("expected victory dossier command");
        };
        assert_eq!(output, PathBuf::from("victory.json"));
        assert_eq!(
            transcript_sources,
            [
                PathBuf::from("proposal.jsonl"),
                PathBuf::from("evaluations")
            ]
        );
        assert!(
            Cli::try_parse_from(["run_play", "victory-dossier", "--session", "fixture",]).is_err()
        );
    }

    #[test]
    fn advice_accepts_a_read_only_retained_node_selector() {
        let cli = Cli::try_parse_from([
            "run_play",
            "advice",
            "--session",
            "fixture",
            "--at",
            "n0123456789ab",
            "--detail",
        ])
        .expect("advice at retained node command");

        let Command::Advice { at, detail, .. } = cli.command else {
            panic!("expected advice command");
        };
        assert_eq!(at.as_deref(), Some("n0123456789ab"));
        assert!(detail);
    }

    #[test]
    fn new_accepts_each_explicit_character() {
        std::thread::Builder::new()
            .stack_size(RUN_PLAY_WORKER_STACK_BYTES)
            .spawn(|| {
                for (name, expected) in [
                    ("ironclad", "Ironclad"),
                    ("silent", "Silent"),
                    ("defect", "Defect"),
                    ("watcher", "Watcher"),
                ] {
                    let cli = Cli::try_parse_from([
                        "run_play",
                        "new",
                        "--session",
                        "fixture",
                        "--seed",
                        "17",
                        "--character",
                        name,
                    ])
                    .unwrap();
                    let Command::New { character, .. } = cli.command else {
                        panic!("expected new")
                    };
                    assert_eq!(character.player_class(), expected);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn new_rejects_an_unknown_character() {
        std::thread::Builder::new()
            .stack_size(RUN_PLAY_WORKER_STACK_BYTES)
            .spawn(|| {
                assert!(Cli::try_parse_from([
                    "run_play",
                    "new",
                    "--session",
                    "fixture",
                    "--seed",
                    "17",
                    "--character",
                    "gremlin"
                ])
                .is_err());
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn new_is_a_current_simulator_seed_not_an_artifact_import() {
        let cli = Cli::try_parse_from(["run_play", "new", "--session", "fixture", "--seed", "17"])
            .expect("natural new command");

        let Command::New {
            seed, ascension, ..
        } = cli.command
        else {
            panic!("expected new command");
        };
        assert_eq!(seed, 17);
        assert_eq!(ascension, 0);
        assert!(Cli::try_parse_from([
            "run_play",
            "new",
            "--session",
            "fixture",
            "--archive",
            "old.json",
            "--checkpoints",
            "old.bin",
        ])
        .is_err());
    }

    #[test]
    fn new_accepts_explicit_ironclad_a20_and_rejects_a21() {
        let cli = Cli::try_parse_from([
            "run_play",
            "new",
            "--session",
            "fixture",
            "--seed",
            "17",
            "--ascension",
            "20",
        ])
        .expect("natural A20 command");
        let Command::New { ascension, .. } = cli.command else {
            panic!("expected new command");
        };
        assert_eq!(ascension, 20);
        assert!(Cli::try_parse_from([
            "run_play",
            "new",
            "--session",
            "fixture",
            "--seed",
            "17",
            "--ascension",
            "21",
        ])
        .is_err());
    }

    #[test]
    fn historical_artifacts_require_the_explicit_import_command() {
        let cli = Cli::try_parse_from([
            "run_play",
            "import",
            "--session",
            "fixture",
            "--archive",
            "old.json",
            "--checkpoints",
            "old.bin",
        ])
        .expect("artifact import command");

        assert!(matches!(cli.command, Command::Import { .. }));
    }

    #[test]
    fn agent_shell_accepts_manual_play_and_an_external_knowledge_pack_reference() {
        let cli = Cli::try_parse_from([
            "run_play",
            "agent-shell",
            "--session",
            "fixture",
            "--knowledge-pack",
            "ironclad-a20-v1",
            "--manual",
            "--locale",
            "zh-CN",
        ])
        .expect("agent shell command");

        let Command::AgentShell {
            knowledge_pack,
            manual,
            locale,
            ..
        } = cli.command
        else {
            panic!("expected agent shell command");
        };
        assert_eq!(knowledge_pack.as_deref(), Some("ironclad-a20-v1"));
        assert!(manual);
        assert_eq!(locale, Locale::ZhCn);
    }
}
