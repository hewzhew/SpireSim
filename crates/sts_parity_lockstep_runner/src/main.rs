use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use sts_java_parity_runtime::{
    launch_java_parity_runtime_v1, JavaParityInitialBoundaryV1, JavaParityLaunchProfileV1,
    JavaParityLaunchRequestV1, JavaParityStorageSeedV1,
};
use sts_oracle_run_control::{
    content::events::neow::NeowEligibilityV1,
    eval::run_control::{
        AtomicCombatSearchOptionsV2, RunCombatResolutionKindV1, RunControlCombatSegmentMode,
        RunControlConfig, RunControlSession,
    },
};
use sts_parity_lockstep::{
    ComparisonStatusV1, DecisionSurfaceV1, LockstepAdapterV1, LockstepModeV1, LockstepRunnerV1,
    SemanticActionSpaceV1, SemanticActionV1, SemanticScalarV1, NATURAL_RUN_SNAPSHOT_DOMAINS_V3,
};
use sts_parity_lockstep_java::{
    JavaLockstepAdapterV1, JavaLockstepTransportV1, LeasedJavaLockstepTransportV1,
    OwnedJavaLockstepTransportV1, RecordedJavaEvidenceStoreV1, RecordedJavaEvidenceV1,
};
use sts_parity_lockstep_runner::{
    admit_exact_parity_corpus_case_v1, check_current_java_oracle_v1,
    connect_current_java_oracle_v1, is_immediate_neow_reward_v1, is_map_choice_v1,
    is_neow_proceed_v1, latest_recorded_natural_combat_entry_action_count_v1,
    manual_natural_combat_entry_coordinate_v1, parse_manual_command_v1,
    record_started_run_rng_probe_v1, render_manual_map_v1, render_manual_observation_v1,
    replay_parity_corpus_v1, replay_recorded_evidence_into_rust_v1, replay_recorded_evidence_v1,
    request_current_java_oracle_stop_v1, run_java_oracle_host_v1, spawn_java_oracle_host_image_v1,
    ManualCommandV1, ManualSessionV1, ParityCorpusAdmissionV1, ParityCorpusCaseKindV1,
    RunRngProbeReportV1,
};
use sts_parity_lockstep_rust::{natural_run_bootstrap_v1, RustLockstepAdapterV1};
use sts_workbench_supervisor::{
    OwnedStorageRegistryV1, RepositoryLifecycleOwnerV1, RepositoryLifecycleRoleV1,
};
use uuid::Uuid;

mod cli_profile;
mod live_cli;

const DEFAULT_GAME_DIRECTORY: &str = r"C:\Program Files (x86)\Steam\steamapps\common\SlayTheSpire";
const DEFAULT_SEED: u64 = 2_026_090_301;
const DEFAULT_ASCENSION: u8 = 20;

enum RecordedReplaySourceV1 {
    Evidence(PathBuf),
    Manual {
        repository: PathBuf,
        session: String,
        through: Option<usize>,
    },
}

struct ArgumentsV1 {
    repository: PathBuf,
    game_directory: PathBuf,
    seed: u64,
    ascension: u8,
}

struct RunRngBatchArgumentsV1 {
    repository: PathBuf,
    seeds: Vec<u64>,
    ascension: u8,
}

struct ManualStartArgumentsV1 {
    repository: PathBuf,
    seed: u64,
    ascension: u8,
    player_class: &'static str,
    replace_active: bool,
}

enum CommandV1 {
    Workspace {
        repository: PathBuf,
        reference: Option<String>,
        at: Option<usize>,
    },
    RecordNatural(ArgumentsV1),
    RecordHosted(ArgumentsV1),
    ProbeHostedRunRng(RunRngBatchArgumentsV1),
    Replay {
        evidence: PathBuf,
    },
    ReplayTraining {
        source: RecordedReplaySourceV1,
        output: PathBuf,
    },
    ReplaySuite {
        suite: PathBuf,
        output: PathBuf,
    },
    ReplayCorpus {
        repository: PathBuf,
    },
    ProbeCombat {
        source: RecordedReplaySourceV1,
        work: usize,
        wall_ms: u64,
        save_proposal: bool,
        workspace: Option<String>,
        max_hp_loss: Option<u32>,
        export_root_case: Option<PathBuf>,
    },
    Host {
        repository: PathBuf,
        game_directory: PathBuf,
        storage_seed: Option<PathBuf>,
    },
    HostLaunch {
        repository: PathBuf,
        game_directory: PathBuf,
        storage_seed: Option<PathBuf>,
        resume_manual: bool,
    },
    HostStatus {
        repository: PathBuf,
    },
    HostStop {
        repository: PathBuf,
    },
    ManualStart(ManualStartArgumentsV1),
    ManualResume {
        repository: PathBuf,
        native: bool,
        json: bool,
        target: Option<(String, usize)>,
    },
    ManualInspect {
        repository: PathBuf,
        transition: Option<(String, usize)>,
        output: Option<PathBuf>,
    },
    ManualDraft {
        repository: PathBuf,
        reference: Option<String>,
    },
    ManualCertifyAct {
        repository: PathBuf,
        act: u8,
        offline_only: bool,
    },
    ManualRetryCombat {
        repository: PathBuf,
        target: Option<(String, usize)>,
    },
}

fn main() -> ExitCode {
    cli_profile::run(run_command)
}

fn run_command() -> ExitCode {
    let command = match parse_command(std::env::args().skip(1)) {
        Ok(arguments) => arguments,
        Err(error) => {
            eprintln!("lockstep rejected: {error}");
            return ExitCode::from(2);
        }
    };
    match command {
        CommandV1::Workspace {
            repository,
            reference,
            at,
        } => {
            let result = (|| {
                let repository = canonical_directory(&repository, "repository")?;
                let session = ManualSessionV1::load_active(&repository)?;
                let draft = sts_parity_lockstep_runner::open_manual_decision_workspace_v1(
                    &session,
                    reference.as_deref(),
                    at,
                    &mut manual_progress(),
                )?;
                sts_parity_lockstep_runner::serve_decision_workspace_v1(
                    sts_parity_lockstep_runner::DecisionWorkspaceServiceV1::new(session, draft),
                    &mut std::io::stdin().lock(),
                    &mut std::io::stdout(),
                )
            })();
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("workspace stopped: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        CommandV1::ManualCertifyAct {
            repository,
            act,
            offline_only,
        } => match run_manual_certify_act(repository, act, offline_only) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Act {act} certification stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::ManualStart(arguments) => match run_manual_start(arguments) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("manual lockstep stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::ManualDraft {
            repository,
            reference,
        } => match run_manual_draft(repository, reference.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("private combat draft stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::ManualInspect {
            repository,
            transition,
            output,
        } => match run_manual_inspect(repository, transition, output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("manual offline replay stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::ManualResume {
            repository,
            native,
            json,
            target,
        } => match run_manual_resume(repository, native, json, target) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("manual lockstep stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::ManualRetryCombat { repository, target } => {
            match run_manual_retry_combat(repository, target) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("manual combat retry stopped: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        CommandV1::ReplayCorpus { repository } => match replay_parity_corpus_v1(&repository) {
            Ok(report) => {
                println!(
                    "corpus exact: {} cases, {} committed actions, {} total Java actions; offline",
                    report.cases, report.committed_actions, report.java_actions
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("corpus stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::ReplaySuite { suite, output } => {
            let result = (|| {
                let report = sts_parity_lockstep_runner::replay_suite_v1(&suite)?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&output)
                    .map_err(|e| format!("cannot create suite report: {e}"))?;
                serde_json::to_writer_pretty(&mut file, &report).map_err(|e| e.to_string())?;
                writeln!(file).map_err(|e| e.to_string())?;
                let passed = report
                    .cases
                    .iter()
                    .filter(|case| case.failures.is_empty())
                    .count();
                let covered = report
                    .required_flows
                    .iter()
                    .filter(|row| !row.witnesses.is_empty())
                    .count();
                println!(
                    "replay suite: {passed}/{} cases; {covered}/{} required flows",
                    report.cases.len(),
                    report.required_flows.len()
                );
                for case in &report.cases {
                    for failure in case.failures.iter().take(3) {
                        eprintln!("{}: {failure}", case.id);
                    }
                }
                for row in report
                    .required_flows
                    .iter()
                    .filter(|row| row.witnesses.is_empty())
                    .take(3)
                {
                    eprintln!(
                        "missing flow: {}",
                        serde_json::to_string(&row.flow).unwrap()
                    );
                }
                Ok::<_, String>(report.passed)
            })();
            match result {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::FAILURE,
                Err(error) => {
                    eprintln!("replay suite stopped: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        CommandV1::ReplayTraining { source, output } => {
            let result = (|| -> Result<(), String> {
                let trajectory = match source {
                    RecordedReplaySourceV1::Evidence(evidence) => {
                        sts_parity_lockstep_runner::replay_training_trajectory_v2(&evidence)?
                    }
                    RecordedReplaySourceV1::Manual {
                        repository,
                        session,
                        through,
                    } => sts_parity_lockstep_runner::replay_manual_training_trajectory_v2(
                        &repository,
                        &session,
                        through,
                    )?,
                };
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&output)
                    .map_err(|e| format!("cannot create training trajectory: {e}"))?;
                serde_json::to_writer(file, &trajectory).map_err(|e| e.to_string())?;
                println!(
                    "offline exact: {} semantic actions, {} learning commits; trajectory: {}",
                    trajectory.replay.committed_actions,
                    trajectory.commits.len(),
                    output.display()
                );
                Ok(())
            })();
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("training replay stopped: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        CommandV1::Replay { evidence } => match replay_recorded_evidence_v1(&evidence) {
            Ok(report) => {
                println!(
                    "offline exact: {} domains, {} committed actions, {} total Java actions",
                    report.domains, report.committed_actions, report.java_actions
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("offline stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::ProbeCombat {
            source,
            work,
            wall_ms,
            save_proposal,
            workspace,
            max_hp_loss,
            export_root_case,
        } => match probe_recorded_combat(
            &source,
            work,
            wall_ms,
            save_proposal,
            workspace.as_deref(),
            max_hp_loss,
            export_root_case.as_deref(),
        ) {
            Ok((actions, proposal)) => {
                println!("offline complete combat win: {} actions", actions.len());
                for (index, action) in actions.iter().enumerate() {
                    println!("W{index} {action}");
                }
                if let Some(proposal) = proposal {
                    println!("private complete proposal: {proposal}");
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("offline combat probe stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::Host {
            repository,
            game_directory,
            storage_seed,
        } => match run_java_oracle_host_v1(
            &repository,
            &game_directory,
            storage_seed.as_deref(),
            |startup| {
                println!(
                    "oracle ready ({:.1}s one-time startup)",
                    startup.as_secs_f64()
                );
                let _ = std::io::stdout().flush();
            },
        ) {
            Ok(_) => {
                println!("oracle stopped");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("oracle stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::HostLaunch {
            repository,
            game_directory,
            storage_seed,
            resume_manual,
        } => match (|| {
            let storage_seed = if resume_manual {
                Some(
                    sts_parity_lockstep_runner::prepare_manual_native_startup_storage_v1(
                        &repository,
                    )?,
                )
            } else {
                storage_seed
            };
            spawn_java_oracle_host_image_v1(&repository, &game_directory, storage_seed.as_deref())
        })() {
            Ok(report) => {
                println!(
                    "oracle ready | {:.1}s startup | logs {}",
                    report.startup.as_secs_f64(),
                    report.diagnostic_id
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("oracle launch rejected: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::HostStatus { repository } => match check_current_java_oracle_v1(&repository) {
            Ok(()) => {
                println!("oracle ready");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("oracle unavailable: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::HostStop { repository } => {
            match request_current_java_oracle_stop_v1(&repository) {
                Ok(()) => {
                    println!("oracle stop requested");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("oracle stop rejected: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        CommandV1::RecordHosted(arguments) => match audit_hosted_natural_act1_prefix(arguments) {
            Ok(report) => {
                println!(
                    "lockstep exact: Neow -> Act 1 boss reward {} ({} domains, {} actions, resident oracle)",
                    report.end_boundary, report.exact_domains, report.actions,
                );
                println!("journal: {}", report.journal_id);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("lockstep stopped: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::ProbeHostedRunRng(arguments) => match probe_hosted_run_rng_batch(arguments) {
            Ok(results) => {
                let mut stopped = 0;
                for result in &results {
                    match &result.outcome {
                        Ok((report, admission)) => {
                            let corpus = match admission {
                                ParityCorpusAdmissionV1::Admitted { .. } => "new",
                                ParityCorpusAdmissionV1::Duplicate { .. } => "duplicate discarded",
                            };
                            println!(
                                "case {} exact: {} domains, {} action, {}; corpus {corpus}",
                                result.ordinal,
                                report.domains,
                                report.committed_actions,
                                report.end_boundary,
                            );
                        }
                        Err(reason) => {
                            stopped += 1;
                            eprintln!("case {} stopped: {reason}", result.ordinal);
                        }
                    }
                }
                if stopped == 0 {
                    println!("batch exact: {} cases, one resident oracle", results.len());
                    ExitCode::SUCCESS
                } else {
                    eprintln!(
                            "batch stopped: {stopped} of {} cases; oracle remained reusable between cases",
                            results.len()
                        );
                    ExitCode::FAILURE
                }
            }
            Err(error) => {
                eprintln!("batch rejected: {error}");
                ExitCode::FAILURE
            }
        },
        CommandV1::RecordNatural(arguments) => match audit_natural_act1_prefix(arguments) {
            Ok(report) => {
                println!(
                    "lockstep exact: Neow -> Act 1 boss reward {} ({} domains, {} actions, {:.1}s startup)",
                    report.end_boundary,
                    report.exact_domains,
                    report.actions,
                    report.startup.as_secs_f64(),
                );
                println!("journal: {}", report.journal_id);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("lockstep stopped: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

fn parse_command(arguments: impl IntoIterator<Item = String>) -> Result<CommandV1, String> {
    let mut arguments = arguments.into_iter().peekable();
    if arguments.peek().map(String::as_str) == Some("manual-certify-act") {
        arguments.next();
        let act: u8 = arguments
            .next()
            .ok_or("manual-certify-act requires an Act from 1 to 4")?
            .parse()
            .map_err(|_| "manual-certify-act requires an Act from 1 to 4")?;
        if !(1..=4).contains(&act) {
            return Err("manual-certify-act requires an Act from 1 to 4".to_owned());
        }
        let offline_only = arguments.peek().map(String::as_str) == Some("--offline");
        if offline_only {
            arguments.next();
        }
        return Ok(CommandV1::ManualCertifyAct {
            repository: parse_repository_argument(arguments)?,
            act,
            offline_only,
        });
    }
    if arguments.peek().map(String::as_str) == Some("manual-start") {
        arguments.next();
        return parse_manual_start_arguments(arguments).map(CommandV1::ManualStart);
    }
    if arguments.peek().map(String::as_str) == Some("manual-resume") {
        arguments.next();
        let native = arguments.peek().map(String::as_str) == Some("--native");
        if native {
            arguments.next();
        }
        let target = if arguments.peek().map(String::as_str) == Some("--at") {
            arguments.next();
            if native {
                return Err("choose native resume or an explicit adopted boundary".into());
            }
            Some(parse_manual_source_coordinate(
                &arguments
                    .next()
                    .ok_or("--at requires <source-id>:<action-count>")?,
            )?)
        } else {
            None
        };
        let json = arguments.peek().map(String::as_str) == Some("--json");
        if json {
            arguments.next();
        }
        return Ok(CommandV1::ManualResume {
            repository: parse_repository_argument(arguments)?,
            native,
            json,
            target,
        });
    }
    if arguments.peek().map(String::as_str) == Some("manual-inspect") {
        arguments.next();
        return parse_manual_inspect(arguments);
    }
    if arguments.peek().map(String::as_str) == Some("manual-draft") {
        arguments.next();
        let reference = arguments
            .peek()
            .filter(|value| !value.starts_with("--"))
            .cloned();
        if reference.is_some() {
            arguments.next();
        }
        return Ok(CommandV1::ManualDraft {
            repository: parse_repository_argument(arguments)?,
            reference,
        });
    }
    if arguments.peek().map(String::as_str) == Some("workspace") {
        arguments.next();
        let mut reference = None;
        let mut at = None;
        if arguments.peek().map(String::as_str) == Some("--at") {
            arguments.next();
            at = Some(
                arguments
                    .next()
                    .ok_or("workspace --at requires a source action coordinate")?
                    .parse()
                    .map_err(|_| "workspace --at requires an unsigned coordinate")?,
            );
        } else if arguments.peek().is_some_and(|s| !s.starts_with("--")) {
            reference = arguments.next();
        }
        return Ok(CommandV1::Workspace {
            repository: parse_repository_argument(arguments)?,
            reference,
            at,
        });
    }
    if arguments.peek().map(String::as_str) == Some("manual-retry-combat") {
        arguments.next();
        return parse_manual_retry_combat(arguments);
    }
    if arguments.peek().map(String::as_str) == Some("record") {
        arguments.next();
        return parse_arguments(arguments).map(CommandV1::RecordHosted);
    }
    if arguments.peek().map(String::as_str) == Some("probe-hosted-run-rng") {
        arguments.next();
        return parse_run_rng_batch_arguments(arguments).map(CommandV1::ProbeHostedRunRng);
    }
    if arguments.peek().map(String::as_str) == Some("host") {
        arguments.next();
        let (repository, game_directory, storage_seed) = parse_host_arguments(arguments)?;
        return Ok(CommandV1::Host {
            repository,
            game_directory,
            storage_seed,
        });
    }
    if arguments.peek().map(String::as_str) == Some("host-launch") {
        arguments.next();
        let resume_manual = arguments.peek().map(String::as_str) == Some("--resume-manual");
        if resume_manual {
            arguments.next();
        }
        let (repository, game_directory, storage_seed) = parse_host_arguments(arguments)?;
        if resume_manual && storage_seed.is_some() {
            return Err("choose manual recovery or an explicit storage seed, not both".to_owned());
        }
        return Ok(CommandV1::HostLaunch {
            repository,
            game_directory,
            storage_seed,
            resume_manual,
        });
    }
    if arguments.peek().map(String::as_str) == Some("host-status") {
        arguments.next();
        return Ok(CommandV1::HostStatus {
            repository: parse_repository_argument(arguments)?,
        });
    }
    if arguments.peek().map(String::as_str) == Some("host-stop") {
        arguments.next();
        return Ok(CommandV1::HostStop {
            repository: parse_repository_argument(arguments)?,
        });
    }
    if arguments.peek().map(String::as_str) == Some("replay") {
        arguments.next();
        let mut evidence = None;
        let mut suite = None;
        let mut output = None;
        let mut learning_output = None;
        let mut session = None;
        let mut repository = None;
        let mut through = None;
        while let Some(flag) = arguments.next() {
            let value = arguments
                .next()
                .ok_or_else(|| format!("{flag} requires a path"))?;
            if flag == "--session" {
                if session.replace(value).is_some() {
                    return Err("duplicate replay session".into());
                }
                continue;
            }
            if flag == "--repository" {
                if repository.replace(PathBuf::from(value)).is_some() {
                    return Err("duplicate replay repository".into());
                }
                continue;
            }
            if flag == "--through" {
                let count = value.parse::<usize>().map_err(|_| {
                    "replay --through requires a non-negative source action count".to_owned()
                })?;
                if through.replace(count).is_some() {
                    return Err("duplicate replay prefix".into());
                }
                continue;
            }
            let target = match flag.as_str() {
                "--evidence" => &mut evidence,
                "--suite" => &mut suite,
                "--output" => &mut output,
                "--learning-output" => &mut learning_output,
                _ => return Err(format!("unknown replay argument {flag}")),
            };
            if target.replace(PathBuf::from(value)).is_some() {
                return Err(format!("duplicate replay argument {flag}"));
            }
        }
        if let Some(session) = session {
            if evidence.is_some()
                || suite.is_some()
                || output.is_some()
                || learning_output.is_none()
            {
                return Err(
                    "manual training replay requires --session ID --learning-output NEW_TRAJECTORY"
                        .into(),
                );
            }
            return Ok(CommandV1::ReplayTraining {
                source: RecordedReplaySourceV1::Manual {
                    session,
                    through,
                    repository: repository
                        .unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?),
                },
                output: learning_output.unwrap(),
            });
        }
        if repository.is_some() || through.is_some() {
            return Err("replay --repository and --through require --session".into());
        }
        return match (evidence, suite, output, learning_output) {
            (Some(evidence), None, None, None) => Ok(CommandV1::Replay { evidence }),
            (Some(evidence), None, None, Some(output)) => Ok(CommandV1::ReplayTraining { source:RecordedReplaySourceV1::Evidence(evidence), output }),
            (None, Some(suite), Some(output), None) => Ok(CommandV1::ReplaySuite { suite, output }),
            _ => Err(
                "replay requires --evidence PATH [--learning-output NEW_TRAJECTORY], --session ID [--repository PATH] [--through N] --learning-output NEW_TRAJECTORY, or --suite PATH --output NEW_REPORT".to_owned(),
            ),
        };
    }
    if arguments.peek().map(String::as_str) == Some("replay-corpus") {
        arguments.next();
        return Ok(CommandV1::ReplayCorpus {
            repository: parse_repository_argument(arguments)?,
        });
    }
    if arguments.peek().map(String::as_str) == Some("probe-combat") {
        arguments.next();
        return parse_combat_probe_arguments(arguments);
    }
    parse_arguments(arguments).map(CommandV1::RecordNatural)
}

fn parse_manual_start_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<ManualStartArgumentsV1, String> {
    let mut parsed = ManualStartArgumentsV1 {
        repository: std::env::current_dir()
            .map_err(|error| format!("failed to read current directory: {error}"))?,
        seed: DEFAULT_SEED,
        ascension: DEFAULT_ASCENSION,
        player_class: "Ironclad",
        replace_active: false,
    };
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        if argument == "--replace-active" {
            parsed.replace_active = true;
            continue;
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("{argument} requires one value"))?;
        match argument.as_str() {
            "--repository" => parsed.repository = PathBuf::from(value),
            "--seed" => {
                parsed.seed = value
                    .parse()
                    .map_err(|_| "--seed must be an unsigned 64-bit integer".to_owned())?
            }
            "--character" => {
                parsed.player_class = match value.as_str() {
                    "ironclad" => "Ironclad",
                    "silent" => "Silent",
                    "defect" => "Defect",
                    "watcher" => "Watcher",
                    _ => {
                        return Err(
                            "--character must be ironclad, silent, defect, or watcher".into()
                        )
                    }
                };
            }
            "--ascension" => {
                parsed.ascension = value
                    .parse()
                    .map_err(|_| "--ascension must be an unsigned 8-bit integer".to_owned())?
            }
            _ => return Err(format!("unsupported manual-start argument {argument:?}")),
        }
    }
    if parsed.ascension != 20 {
        return Err("manual parity play is currently scoped to A20".to_owned());
    }
    Ok(parsed)
}

fn run_manual_certify_act(repository: PathBuf, act: u8, offline_only: bool) -> Result<(), String> {
    let repository = canonical_directory(&repository, "repository")?;
    let plan = sts_parity_lockstep_runner::prepare_manual_act_replay_v1(&repository, act)?;
    println!(
        "Act {act} preflight exact | {} adopted actions | {} source segments | no Java actions",
        plan.action_count(),
        plan.source_count()
    );
    if offline_only {
        return Ok(());
    }
    let trial = ManualSessionV1::prepare_replacement(&repository)?;
    let mut transport = connect_current_java_oracle_v1(&repository, Duration::from_secs(10))?;
    let storage = transport
        .isolated_storage_directory()
        .map_err(|failure| failure.message)?;
    // Preserve the currently parked native generation before the managed host
    // discards it. No source artifact or installed-game save is edited.
    let backup = trial.directory().join("previous-native-storage");
    std::fs::create_dir(&backup).map_err(|error| error.to_string())?;
    for name in ["saves", "preferences", "betaPreferences"] {
        let source = storage.join(name);
        let destination = backup.join(name);
        std::fs::create_dir(&destination).map_err(|error| error.to_string())?;
        for entry in std::fs::read_dir(&source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("native backup encountered a non-file entry".to_owned());
            }
            std::fs::copy(entry.path(), destination.join(entry.file_name()))
                .map_err(|error| error.to_string())?;
        }
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    use sha2::Digest;
    let executable_identity = format!(
        "sha256:{:x}",
        sha2::Sha256::digest(std::fs::read(&executable).map_err(|error| error.to_string())?)
    );
    let source_version = std::process::Command::new("git")
        .args(["describe", "--always", "--dirty", "--abbrev=40"])
        .current_dir(&repository)
        .output()
        .map_err(|error| error.to_string())?;
    if !source_version.status.success() {
        return Err("cannot record the certification source version".to_owned());
    }
    let version = String::from_utf8(source_version.stdout).map_err(|error| error.to_string())?;
    let version = version.trim();
    write_certification_json(
        &trial.directory().join("certification-input.json"),
        &serde_json::json!({
            "schema": "sts-act-certification-input-v1", "plan": plan,
        "rust_source_version": version, "worker_executable": executable,
        "worker_identity": executable_identity,
            "scope": "accelerated_sts_wrapper_rng_ledger_and_declared_snapshot_domains",
        }),
    )?;
    println!("Act {act} preparation started | adopted actions only");
    let result = sts_parity_lockstep_runner::certify_manual_act_v1(
        &repository,
        transport,
        &plan,
        &trial,
        &executable_identity,
        |count, runner| {
            if count > 0 {
                sts_parity_lockstep_runner::capture_manual_checkpoints_after_step_v1(
                    &trial, &storage, runner,
                )?;
            } else {
                sts_parity_lockstep_runner::capture_native_generation_if_ready_v1(
                    &repository,
                    &trial,
                    &storage,
                )?;
            }
            let snapshot = runner.current_snapshots().0;
            if count % 10 == 0 || count == plan.action_count() {
                let actions = SemanticActionSpaceV1::from_snapshot(snapshot)?;
                let rendered = render_manual_observation_v1(snapshot, &actions, &[])?;
                println!(
                    "{count}/{} | {}",
                    plan.action_count(),
                    rendered.lines().next().unwrap_or("Exact")
                );
                std::io::stdout()
                    .flush()
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        },
    );
    let report = match result {
        Ok(report) => report,
        Err(error) => {
            write_certification_json(
                &trial.directory().join("certification-result.json"),
                &serde_json::json!({
                    "schema": "sts-act-certification-result-v1", "status": "stopped", "act": act, "reason": error,
                }),
            )?;
            return Err(format!("{error}; retained trial {}", trial.session_id()));
        }
    };
    write_certification_json(
        &trial.directory().join("certification-result.json"),
        &serde_json::json!({
            "schema": "sts-act-certification-result-v1", "status": "exact", "act": act,
            "start": if act == 1 { "natural_neow".to_owned() } else { format!("act{act}_map_entrance") },
            "end": plan.terminal_label(), "domains": report.domains,
            "committed_actions": report.committed_actions, "java_actions": report.java_actions,
        "rust_source_version": version, "worker_identity": executable_identity,
            "scope": "accelerated_sts_wrapper_rng_ledger_and_declared_snapshot_domains",
        }),
    )?;
    println!(
        "Act {act} certified | {} domains | {} committed actions | {}; active Audit branch preserved",
        report.domains, report.committed_actions, plan.terminal_label()
    );
    Ok(())
}

fn write_certification_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())
}

fn run_manual_start(arguments: ManualStartArgumentsV1) -> Result<(), String> {
    let repository = canonical_directory(&arguments.repository, "repository")?;
    check_current_java_oracle_v1(&repository)?;
    let session = if arguments.replace_active {
        ManualSessionV1::prepare_replacement(&repository)?
    } else {
        ManualSessionV1::prepare(&repository)?
    };
    let mut transport = connect_current_java_oracle_v1(&repository, Duration::from_secs(10))?;
    let java_storage = transport
        .isolated_storage_directory()
        .map_err(|failure| failure.message)?;
    let mut current = transport.status().map_err(|failure| failure.message)?;
    if arguments.replace_active
        && (current.frame["boundary"]["kind"].as_str() != Some("main_menu_ready")
            || !current.frame["public_base"]["run"].is_null())
    {
        transport
            .discard_run_for_next_case()
            .map_err(|failure| failure.message)?;
        current = transport.status().map_err(|failure| failure.message)?;
    }
    if current.frame["boundary"]["kind"].as_str() != Some("main_menu_ready")
        || !current.frame["public_base"]["run"].is_null()
    {
        return Err(
            "manual-start requires the resident Java oracle at the empty main menu; an existing run was left untouched"
                .to_owned(),
        );
    }

    let config = RunControlConfig {
        seed: arguments.seed,
        ascension_level: arguments.ascension,
        final_act: true,
        player_class: arguments.player_class,
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config)?;
    let seed_token =
        sts_java_parity_live::java_seed_token_for_engine_seed(&arguments.seed.to_string())?;
    transport
        .start_run_with_content_profile(
            &bootstrap.player_class,
            u32::from(arguments.ascension),
            Some(&seed_token),
            "all_unlocked",
            Duration::from_secs(10),
        )
        .map_err(|failure| failure.message)?;
    let evidence_store =
        RecordedJavaEvidenceStoreV1::create_new(&session.evidence_directory(), bootstrap.clone())?;
    let java =
        JavaLockstepAdapterV1::with_evidence_store(transport, bootstrap.clone(), evidence_store)?;
    let rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap,
        env!("CARGO_PKG_VERSION"),
        Uuid::new_v4().simple().to_string(),
    )?;
    let (epoch, epoch_directory) = session.next_epoch_directory()?;
    let epoch_id = format!("{}-epoch-{epoch:08}", session.session_id());
    let mut runner = LockstepRunnerV1::start_new(
        java,
        rust,
        LockstepModeV1::Audit,
        &epoch_directory,
        epoch_id,
    )?;
    if arguments.replace_active {
        session.publish_replacing_active()?;
    } else {
        session.publish_active()?;
    }
    require_exact(&runner)?;
    println!("manual lockstep ready | new A20H run | epoch {epoch}");
    run_manual_repl(&mut runner, &session, &java_storage)
}

fn parse_manual_inspect(arguments: impl IntoIterator<Item = String>) -> Result<CommandV1, String> {
    let mut repository = None;
    let mut source = None;
    let mut action = None;
    let mut output = None;
    let mut arguments = arguments.into_iter();
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--repository" if repository.is_none() => repository = Some(PathBuf::from(value)),
            "--session" if source.is_none() => source = Some(value),
            "--action" if action.is_none() => {
                let number = value
                    .parse::<usize>()
                    .map_err(|_| "--action requires a positive committed action number")?;
                if number == 0 {
                    return Err("--action is one-based; use a positive number".into());
                }
                action = Some(number);
            }
            "--output" if output.is_none() => output = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown or duplicate inspection argument {flag}")),
        }
    }
    let transition =
        match (source, action) {
            (Some(source), Some(action)) => Some((source, action)),
            (None, None) if output.is_none() => None,
            _ => return Err(
                "targeted inspection requires --session ID --action N; --output PATH is optional"
                    .into(),
            ),
        };
    Ok(CommandV1::ManualInspect {
        repository: repository.unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?),
        transition,
        output,
    })
}

fn run_manual_inspect(
    repository: PathBuf,
    transition: Option<(String, usize)>,
    output: Option<PathBuf>,
) -> Result<(), String> {
    let repository = canonical_directory(&repository, "repository")?;
    if let Some((source, action)) = transition {
        let session = ManualSessionV1::load_existing(&repository, &source)?;
        let report = sts_parity_lockstep_runner::inspect_manual_transition_v1(
            &session,
            action,
            &mut manual_progress(),
        )?;
        if let Some(output) = output {
            sts_parity_lockstep_runner::save_manual_transition_inspection_v1(
                &repository,
                &output,
                &report,
            )?;
            println!("inspection evidence saved");
        }
        println!("{}", report.summary());
        return if report.reached_exact {
            Ok(())
        } else {
            Err("recorded transition did not verify; evidence retained".into())
        };
    }
    let session = ManualSessionV1::load_active(&repository)?;
    let (report, mut rust, _) = sts_parity_lockstep_runner::replay_manual_session_with_progress_v1(
        &repository,
        &session,
        &mut manual_progress(),
    )?;
    let snapshot = rust.observe().map_err(|failure| failure.message)?;
    let actions = SemanticActionSpaceV1::from_snapshot(&snapshot)?;
    println!(
        "offline exact | {} domains | {} local committed actions",
        report.domains, report.committed_actions
    );
    println!(
        "{}",
        render_manual_observation_v1(&snapshot, &actions, &rust.event_action_labels_v1())?
    );
    println!("{}", render_manual_map_v1(&snapshot)?);
    Ok(())
}

fn run_manual_draft(repository: PathBuf, reference: Option<&str>) -> Result<(), String> {
    let repository = canonical_directory(&repository, "repository")?;
    let session = ManualSessionV1::load_active(&repository)?;
    if reference == Some("list") {
        let drafts = sts_parity_lockstep_runner::list_manual_draft_workspaces_v1(&session)?;
        if drafts.is_empty() {
            println!("no saved private workspaces");
        }
        for (handle, count, certified) in drafts {
            println!(
                "{handle} | {count} selected actions | {}",
                if certified {
                    "saved victory proposal"
                } else {
                    "unfinished"
                }
            );
        }
        return Ok(());
    }
    let mut draft = sts_parity_lockstep_runner::open_manual_draft_workspace_v1(
        &session,
        reference,
        &mut manual_progress(),
    )?;
    run_private_draft_prompt(
        &session,
        &mut draft,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout(),
    )
}

fn run_private_draft_prompt(
    session: &ManualSessionV1,
    draft: &mut sts_parity_lockstep_runner::ManualDraftWorkspaceV1,
    input: &mut impl std::io::BufRead,
    output: &mut impl std::io::Write,
) -> Result<(), String> {
    loop {
        let snapshot = draft.snapshot()?;
        let view = draft.action_view()?;
        writeln!(
            output,
            "{} | private Rust draft | {} selected actions | autosaved",
            draft.handle(),
            draft.selected_actions().len()
        )
        .map_err(|e| e.to_string())?;
        if let Some(reason) = draft.replay_stop() {
            writeln!(
                output,
                "{reason} | {} actions replayed; undo to a valid boundary",
                draft.verified_actions()
            )
            .map_err(|e| e.to_string())?;
        }
        writeln!(
            output,
            "{}",
            sts_parity_lockstep_runner::render_manual_action_view_v1(&snapshot, &view, &[])?
                .replacen("Exact |", "Rust only |", 1)
        )
        .map_err(|e| e.to_string())?;
        write!(output, "draft> ")
            .and_then(|_| output.flush())
            .map_err(|e| e.to_string())?;
        let mut line = String::new();
        if input.read_line(&mut line).map_err(|e| e.to_string())? == 0 || line.trim() == "park" {
            writeln!(
                output,
                "saved {} | resume: manual-draft {}",
                draft.handle(),
                draft.handle()
            )
            .map_err(|e| e.to_string())?;
            return Ok(());
        }
        let result = match line.trim() {
            "undo" => draft.undo(),
            "certify" => match draft.certify(session) {
                Ok(handle) => {
                    let kind = if handle.starts_with('P') {
                        "bounded reward plan"
                    } else {
                        "complete victory"
                    };
                    writeln!(output, "private {kind} verified | {} actions | saved {handle}\nmanual command: adopt {handle}", draft.selected_actions().len()).map_err(|e| e.to_string())?;
                    return Ok(());
                }
                Err(error) => Err(error),
            },
            "help" | "?" => {
                writeln!(
                    output,
                    "V…:0 | observe | undo | certify | park; choices autosaved; Rust only"
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }
            command => match parse_manual_command_v1(command) {
                Ok(ManualCommandV1::Observe) => Ok(()),
                Ok(ManualCommandV1::Step(handle)) => draft.choose_handle(&handle),
                _ => Err("use V…:0, observe, undo, certify, or park".into()),
            },
        };
        if let Err(error) = result {
            writeln!(output, "draft rejected: {error}").map_err(|e| e.to_string())?;
        }
    }
}

fn run_manual_resume(
    repository: PathBuf,
    native: bool,
    json: bool,
    target: Option<(String, usize)>,
) -> Result<(), String> {
    let _timing = sts_parity_lockstep::timing_span_v1("manual.resume");
    let repository = canonical_directory(&repository, "repository")?;
    let mut progress = manual_progress();
    let session = ManualSessionV1::load_active(&repository)?;
    if let Some((source_id, count)) = target {
        let source = ManualSessionV1::load_existing(&repository, &source_id)?;
        let plan = sts_parity_lockstep_runner::prepare_native_recovery_prefix_with_progress_v1(
            &repository,
            session.session_id(),
            &source,
            count,
            &mut progress,
        )?;
        let transport = connect_current_java_oracle_v1(&repository, Duration::from_secs(10))?;
        let (replacement, mut runner, storage) =
            sts_parity_lockstep_runner::recover_native_with_progress_v1(
                &repository,
                transport,
                plan,
                &mut progress,
            )?;
        return if json {
            live_cli::serve(&mut runner, &replacement, &storage)
        } else {
            run_manual_repl(&mut runner, &replacement, &storage)
        };
    }
    let evidence_directory = session.evidence_directory();
    let evidence_store = RecordedJavaEvidenceStoreV1::open_append(&evidence_directory)?;
    let bootstrap = evidence_store.bootstrap().clone();
    let mut transport = connect_current_java_oracle_v1(&repository, Duration::from_secs(10))?;
    let java_storage = transport
        .isolated_storage_directory()
        .map_err(|failure| failure.message)?;
    let current = transport.status().map_err(|failure| failure.message)?;
    let matches_recorded_tail = evidence_store.matches_resume_observation_v1(&current);
    if native || !matches_recorded_tail {
        // A matching terminal frame is still live, but original victory has
        // deleted its autosave. Recover from the recorded durable generation.
        eprintln!("preparing | native save selection");
        let generation = if matches_recorded_tail
            && current.frame["boundary"]["kind"].as_str() != Some("terminal")
        {
            sts_parity_lockstep_runner::capture_native_generation_v1(
                &repository,
                &session,
                &java_storage,
            )?
        } else {
            sts_parity_lockstep_runner::load_latest_native_generation_v1(&repository, &session)?
        };
        let plan = sts_parity_lockstep_runner::prepare_native_recovery_with_progress_v1(
            &repository,
            &session,
            generation,
            &mut progress,
        )?;
        let count = plan.suffix_actions();
        eprintln!("native recovery | original autosave + {count} adopted suffix actions");
        let (replacement, mut runner, storage) =
            sts_parity_lockstep_runner::recover_native_with_progress_v1(
                &repository,
                transport,
                plan,
                &mut progress,
            )?;
        eprintln!("native recovery Exact | {count} suffix actions | current progress restored");
        return if json {
            live_cli::serve(&mut runner, &replacement, &storage)
        } else {
            run_manual_repl(&mut runner, &replacement, &storage)
        };
    }
    let (report, rust, java_resume) =
        sts_parity_lockstep_runner::replay_manual_session_with_progress_v1(
            &repository,
            &session,
            &mut progress,
        )?;
    let java = JavaLockstepAdapterV1::resume_with_evidence_store(
        transport,
        bootstrap,
        evidence_store,
        java_resume,
    )?;
    let (epoch, epoch_directory) = session.next_epoch_directory()?;
    let epoch_id = format!("{}-epoch-{epoch:08}", session.session_id());
    let mut runner = LockstepRunnerV1::start_new(
        java,
        rust,
        LockstepModeV1::Audit,
        &epoch_directory,
        epoch_id,
    )?;
    require_exact(&runner)?;
    eprintln!(
        "manual lockstep resumed | {} actions replayed offline | epoch {epoch}",
        report.committed_actions
    );
    if json {
        live_cli::serve(&mut runner, &session, &java_storage)
    } else {
        run_manual_repl(&mut runner, &session, &java_storage)
    }
}

fn manual_progress() -> impl FnMut(sts_parity_lockstep_runner::LockstepProgressV1) {
    use sts_parity_lockstep_runner::LockstepProgressV1;
    let mut last = std::time::Instant::now();
    move |event| match event {
        LockstepProgressV1::Started(phase) => {
            eprintln!("preparing | {phase}");
            last = std::time::Instant::now();
        }
        LockstepProgressV1::Finished {
            phase,
            elapsed,
            succeeded,
        } => {
            eprintln!(
                "{} | {phase} | {:.2}s",
                if succeeded { "done" } else { "stopped" },
                elapsed.as_secs_f64()
            );
        }
        LockstepProgressV1::Replay {
            scope,
            scopes,
            completed,
            total,
        } if last.elapsed() >= Duration::from_secs(2) => {
            eprintln!(
                "offline replay | source {scope}/{scopes} | {completed}/{total} actions checked"
            );
            last = std::time::Instant::now();
        }
        LockstepProgressV1::NativeSuffix { completed, total }
            if completed == total || last.elapsed() >= Duration::from_secs(2) =>
        {
            eprintln!("native suffix | {completed}/{total} Exact");
            last = std::time::Instant::now();
        }
        _ => (),
    }
}

fn parse_manual_source_coordinate(value: &str) -> Result<(String, usize), String> {
    let (session, count) = value
        .split_once(':')
        .ok_or("--at requires <source-id>:<action-count>")?;
    let count = count
        .parse::<usize>()
        .ok()
        .filter(|count| *count > 0)
        .ok_or("--at requires a positive committed action count")?;
    if session.is_empty() {
        return Err("--at requires a source session identity".into());
    }
    Ok((session.to_owned(), count))
}

fn parse_manual_retry_combat(
    arguments: impl IntoIterator<Item = String>,
) -> Result<CommandV1, String> {
    let mut repository = None;
    let mut target = None;
    let mut arguments = arguments.into_iter();
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--repository" if repository.is_none() => repository = Some(PathBuf::from(value)),
            "--at" if target.is_none() => {
                target = Some(parse_manual_source_coordinate(&value)?);
            }
            _ => {
                return Err(format!(
                    "unsupported or repeated combat retry argument {flag:?}"
                ))
            }
        }
    }
    Ok(CommandV1::ManualRetryCombat {
        repository: repository.unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?),
        target,
    })
}

fn run_manual_retry_combat(
    repository: PathBuf,
    target: Option<(String, usize)>,
) -> Result<(), String> {
    let repository = canonical_directory(&repository, "repository")?;
    check_current_java_oracle_v1(&repository)?;
    let source = ManualSessionV1::load_active(&repository)?;
    let mut progress = manual_progress();
    let plan = sts_parity_lockstep_runner::prepare_native_combat_retry_with_progress_v1(
        &repository,
        &source,
        target
            .as_ref()
            .map(|(session, count)| (session.as_str(), *count)),
        &mut progress,
    )?;
    let count = plan.suffix_actions();
    println!("combat retry | original autosave + {count} adopted suffix actions");
    let transport = connect_current_java_oracle_v1(&repository, Duration::from_secs(10))?;
    let (replacement, mut runner, storage) =
        sts_parity_lockstep_runner::recover_native_with_progress_v1(
            &repository,
            transport,
            plan,
            &mut progress,
        )?;
    println!("combat retry Exact | combat entry restored");
    run_manual_repl(&mut runner, &replacement, &storage)
}

fn run_manual_repl(
    runner: &mut ManualRunner,
    session: &ManualSessionV1,
    java_storage: &Path,
) -> Result<(), String> {
    capture_manual_checkpoints_on_attach(runner, session, java_storage)?;
    print_manual_observation(runner)?;
    print!("manual> ");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let mut line = String::new();
        let bytes = {
            let _timing = sts_parity_lockstep::timing_span_v1("input.wait");
            input.read_line(&mut line).map_err(|e| e.to_string())?
        };
        if bytes == 0 {
            break;
        }
        let command = match parse_manual_command_v1(&line) {
            Ok(command) => command,
            Err(error) => {
                println!("input rejected: {error}");
                print!("manual> ");
                std::io::stdout().flush().map_err(|e| e.to_string())?;
                continue;
            }
        };
        let mut output = Vec::new();
        let result = execute_manual_command(runner, session, java_storage, command, &mut output);
        print!("{}", String::from_utf8_lossy(&output));
        let receipt = result?;
        if let Some(error) = receipt.rejection {
            println!("input rejected: {error}");
        }
        if receipt.closed {
            return Ok(());
        }
        print_manual_observation(runner)?;
        print!("manual> ");
        std::io::stdout().flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Default)]
struct ManualExecution {
    closed: bool,
    rejection: Option<String>,
}
impl ManualExecution {
    fn rejected(message: String) -> Self {
        Self {
            rejection: Some(message),
            closed: false,
        }
    }
}

fn execute_manual_command(
    runner: &mut ManualRunner,
    session: &ManualSessionV1,
    java_storage: &Path,
    command: ManualCommandV1,
    output: &mut Vec<u8>,
) -> Result<ManualExecution, String> {
    macro_rules! report {
        ($($arg:tt)*) => { writeln!(output, $($arg)*).map_err(|e| e.to_string())? };
    }
    match command {
        ManualCommandV1::Observe => (),
        ManualCommandV1::Map => {
            report!("{}", render_manual_map_v1(runner.current_snapshots().0)?);
        }
        ManualCommandV1::Help => {
            report!("observe | map | drafts | V…:0 | plan rewards potion gold cards | adopt <handle or path> | park");
        }
        ManualCommandV1::Drafts => {
            match sts_parity_lockstep_runner::list_manual_reward_plans_v1(session) {
                Ok(plans) => {
                    for (handle, summary) in plans {
                        report!("{handle} | private reward plan | {summary} | adopt {handle}");
                    }
                }
                Err(error) => report!("reward plan listing unavailable: {error}"),
            }
            match sts_parity_lockstep_runner::list_manual_combat_drafts_v1(
                session,
                runner.current_snapshots().1,
            ) {
                Ok(drafts) if drafts.is_empty() => report!("no saved drafts for this root"),
                Ok(drafts) => {
                    for (handle, count) in drafts {
                        report!("{handle} | {count} actions | private proposal | adopt {handle}");
                    }
                }
                Err(error) => report!("draft listing unavailable: {error}"),
            }
        }
        ManualCommandV1::Park => {
            report!("manual lockstep writer closed");
            return Ok(ManualExecution {
                closed: true,
                rejection: None,
            });
        }
        ManualCommandV1::Step(handle) => {
            let action = match runner.resolve_action_handle(&handle) {
                Ok(action) => action,
                Err(error) => {
                    return Ok(ManualExecution::rejected(error));
                }
            };
            commit_manual_action(runner, session, java_storage, action)?;
        }
        ManualCommandV1::PlanRewards(steps) => {
            require_exact(runner)?;
            match sts_parity_lockstep_runner::save_manual_reward_plan_v1(
                session,
                runner.rust_adapter(),
                steps,
            ) {
                Ok((handle, summary)) => report!(
                    "{handle} | Rust preview saved | {summary}\nJava unchanged | adopt {handle}"
                ),
                Err(error) => report!("plan rejected: {error}"),
            }
        }
        ManualCommandV1::Adopt(path) => {
            if let Some(handle) = path
                .to_str()
                .filter(|s| s.starts_with('P') && !s.contains(['/', '\\']))
            {
                let prepared = match sts_parity_lockstep_runner::prepare_manual_reward_adoption_v1(
                    session,
                    handle,
                    runner,
                    &mut manual_progress(),
                ) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        return Ok(ManualExecution::rejected(error));
                    }
                };
                report!(
                    "verified reward plan | {} already committed | {} remaining",
                    prepared.completed_actions(),
                    prepared.remaining_actions()
                );
                let count = prepared.commit(runner, |index, runner| {
                    sts_parity_lockstep_runner::capture_manual_checkpoints_after_step_v1(
                        session,
                        java_storage,
                        runner,
                    )?;
                    report!("reward {index}/{} Exact", prepared.remaining_actions());
                    Ok(())
                })?;
                report!("reward plan adopted | {count} actions Exact | stopped before card choice");

                return Ok(ManualExecution::default());
            }
            let draft =
                match sts_parity_lockstep_runner::load_manual_combat_draft_v1(session, &path) {
                    Ok(draft) => draft,
                    Err(error) => {
                        return Ok(ManualExecution::rejected(error));
                    }
                };
            let prepared = match sts_parity_lockstep_runner::prepare_manual_draft_adoption_v1(
                session,
                &draft,
                runner.rust_adapter(),
                &mut manual_progress(),
            ) {
                Ok(prepared) => prepared,
                Err(error) => {
                    return Ok(ManualExecution::rejected(error));
                }
            };
            report!(
                "verified draft | {} already committed | {} remaining",
                prepared.completed_actions(),
                prepared.remaining_actions()
            );
            let start = std::time::Instant::now();
            let count = prepared.commit(runner, |index, runner| {
                sts_parity_lockstep_runner::capture_manual_checkpoints_after_step_v1(
                    session,
                    java_storage,
                    runner,
                )?;
                report!(
                    "draft {index}/{} Exact | {:.2}s elapsed",
                    prepared.remaining_actions(),
                    start.elapsed().as_secs_f64()
                );
                Ok(())
            })?;
            report!(
                "adopted complete battle | {count} actions Exact | {:.2}s",
                start.elapsed().as_secs_f64()
            );
        }
    }
    Ok(ManualExecution::default())
}

fn capture_manual_checkpoints_on_attach(
    runner: &LockstepRunnerV1<
        JavaLockstepAdapterV1<LeasedJavaLockstepTransportV1>,
        RustLockstepAdapterV1,
    >,
    session: &ManualSessionV1,
    java_storage: &Path,
) -> Result<(), String> {
    sts_parity_lockstep_runner::capture_native_generation_if_ready_v1(
        session.repository(),
        session,
        java_storage,
    )?;
    let Some((act, floor)) =
        manual_natural_combat_entry_coordinate_v1(runner.current_snapshots().0)?
    else {
        return Ok(());
    };
    let evidence = RecordedJavaEvidenceV1::load_complete(&session.evidence_directory())?;
    let Some(entry_action_count) = latest_recorded_natural_combat_entry_action_count_v1(&evidence)
    else {
        return Ok(());
    };
    if entry_action_count != evidence.steps.len() {
        return Ok(());
    }
    session.capture_seeded_combat_checkpoint(java_storage, entry_action_count, act, floor)?;
    Ok(())
}

type ManualRunner =
    LockstepRunnerV1<JavaLockstepAdapterV1<LeasedJavaLockstepTransportV1>, RustLockstepAdapterV1>;

fn commit_manual_action(
    runner: &mut ManualRunner,
    session: &ManualSessionV1,
    storage: &Path,
    action: SemanticActionV1,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    runner.step(action)?;
    if runner.is_stopped() {
        return Err(runner
            .stop_reason()
            .unwrap_or("lockstep stopped without a reason")
            .to_owned());
    }
    let transition_time = start.elapsed();
    sts_parity_lockstep_runner::capture_manual_checkpoints_after_step_v1(session, storage, runner)?;
    if std::env::var_os("STS_LOCKSTEP_TIMING").is_some() {
        eprintln!(
            "timing | lockstep {:.1} ms | native capture {:.1} ms",
            transition_time.as_secs_f64() * 1000.0,
            (start.elapsed() - transition_time).as_secs_f64() * 1000.0
        );
    }
    Ok(())
}

fn print_manual_observation<J>(
    runner: &LockstepRunnerV1<J, RustLockstepAdapterV1>,
) -> Result<(), String>
where
    J: LockstepAdapterV1,
{
    require_exact(runner)?;
    let view = runner.current_action_view()?;
    println!(
        "{}",
        sts_parity_lockstep_runner::render_manual_action_view_v1(
            runner.current_snapshots().0,
            &view,
            &runner.rust_adapter().event_action_labels_v1(),
        )?
    );
    Ok(())
}

fn parse_run_rng_batch_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<RunRngBatchArgumentsV1, String> {
    let mut parsed = RunRngBatchArgumentsV1 {
        repository: std::env::current_dir()
            .map_err(|error| format!("failed to read current directory: {error}"))?,
        seeds: Vec::new(),
        ascension: DEFAULT_ASCENSION,
    };
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{argument} requires one value"))?;
        match argument.as_str() {
            "--repository" => parsed.repository = PathBuf::from(value),
            "--seed" => parsed.seeds.push(
                value
                    .parse()
                    .map_err(|_| "--seed must be an unsigned 64-bit integer".to_owned())?,
            ),
            "--ascension" => {
                parsed.ascension = value
                    .parse()
                    .map_err(|_| "--ascension must be an unsigned 8-bit integer".to_owned())?
            }
            _ => {
                return Err(format!(
                    "unsupported probe-hosted-run-rng argument {argument:?}"
                ))
            }
        }
    }
    if parsed.seeds.len() < 2 {
        return Err("probe-hosted-run-rng requires at least two --seed values".to_owned());
    }
    if parsed.ascension != 20 {
        return Err("Run + RNG batch probe is scoped to A20".to_owned());
    }
    Ok(parsed)
}

fn parse_combat_probe_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<CommandV1, String> {
    let mut evidence = None;
    let mut session = None;
    let mut repository = None;
    let mut through = None;
    let mut save_proposal = false;
    let mut workspace = None;
    let mut max_hp_loss = None;
    let mut export_root_case = None;
    let mut work = 200_000;
    let mut wall_ms = 20_000;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        if argument == "--save-proposal" {
            if save_proposal {
                return Err("duplicate --save-proposal".into());
            }
            save_proposal = true;
            continue;
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("{argument} requires one value"))?;
        match argument.as_str() {
            "--evidence" if evidence.is_none() => evidence = Some(PathBuf::from(value)),
            "--session" if session.is_none() => session = Some(value),
            "--repository" if repository.is_none() => repository = Some(PathBuf::from(value)),
            "--through" if through.is_none() => {
                through = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "--through requires a non-negative action count")?,
                );
            }
            "--workspace" if workspace.is_none() => workspace = Some(value),
            "--export-root-case" if export_root_case.is_none() => {
                export_root_case = Some(PathBuf::from(value));
            }
            "--max-hp-loss" if max_hp_loss.is_none() => {
                max_hp_loss = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "--max-hp-loss requires a non-negative count")?,
                );
            }
            "--work" => {
                work = value
                    .parse()
                    .map_err(|_| "--work must be a positive integer".to_owned())?
            }
            "--wall-ms" => {
                wall_ms = value
                    .parse()
                    .map_err(|_| "--wall-ms must be a positive integer".to_owned())?
            }
            _ => return Err(format!("unsupported probe-combat argument {argument:?}")),
        }
    }
    let source = match (evidence, session) {
        (Some(evidence), None) if repository.is_none() && through.is_none() && !save_proposal && workspace.is_none() => RecordedReplaySourceV1::Evidence(evidence),
        (None, Some(session)) => RecordedReplaySourceV1::Manual {
            session,
            repository: repository.unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?),
            through,
        },
        _ => return Err("probe-combat requires --evidence PATH or --session ID; repository, through and saving a proposal require a session".into()),
    };
    if work == 0 || wall_ms == 0 {
        return Err("probe-combat budgets must be positive".to_owned());
    }
    Ok(CommandV1::ProbeCombat {
        source,
        work,
        wall_ms,
        save_proposal,
        workspace,
        max_hp_loss,
        export_root_case,
    })
}

fn probe_recorded_combat(
    source: &RecordedReplaySourceV1,
    work: usize,
    wall_ms: u64,
    save_proposal: bool,
    workspace: Option<&str>,
    max_hp_loss: Option<u32>,
    export_root_case: Option<&Path>,
) -> Result<(Vec<String>, Option<String>), String> {
    let (mut rust, anchor) = match source {
        RecordedReplaySourceV1::Evidence(evidence) => {
            (replay_recorded_evidence_into_rust_v1(evidence)?.1, None)
        }
        RecordedReplaySourceV1::Manual {
            repository,
            session,
            through,
        } => {
            let repository = canonical_directory(repository, "repository")?;
            let owner = ManualSessionV1::load_existing(&repository, session)?;
            let (report, rust, _) = match through {
                Some(count) => sts_parity_lockstep_runner::replay_manual_session_prefix_v1(
                    &repository,
                    &owner,
                    *count,
                )?,
                None => sts_parity_lockstep_runner::replay_manual_session_v1(&repository, &owner)?,
            };
            (rust, Some((owner, report.committed_actions)))
        }
    };
    let mut current = rust.observe().map_err(|failure| failure.message)?;
    let root = current.clone();
    if let Some(path) = export_root_case {
        let origin = match &anchor {
            Some((owner, count)) => format!("manual {} through {count}", owner.session_id()),
            None => match source {
                RecordedReplaySourceV1::Evidence(path) => format!("evidence {}", path.display()),
                _ => unreachable!("manual replay supplies an anchor"),
            },
        };
        export_combat_probe_root_case(rust.session(), &origin, path)?;
        eprintln!("offline probe source root exported: {}", path.display());
    }
    let actions = if let Some(workspace) = workspace {
        let (owner, committed_actions) = anchor
            .as_ref()
            .ok_or("workspace probe requires a manual source")?;
        let workspace = sts_parity_lockstep_runner::open_manual_draft_workspace_v1(
            owner,
            Some(workspace),
            &mut manual_progress(),
        )?;
        let expected_source = sts_parity_lockstep_runner::ManualDraftSourceV1 {
            session_id: owner.session_id().into(),
            committed_actions: *committed_actions,
        };
        let draft =
            workspace.plan_combat_completion(&expected_source, work, wall_ms, max_hp_loss)?;
        if draft.source.as_ref() != Some(&expected_source) || draft.root != root {
            return Err("workspace root does not match the selected manual source prefix".into());
        }
        draft.actions
    } else {
        sts_parity_lockstep_runner::plan_manual_combat_v1(&rust, work, wall_ms, max_hp_loss)?
    };
    let mut rendered = Vec::with_capacity(actions.len());
    for action in &actions {
        let singleton = SemanticActionSpaceV1::new([action.clone()])?;
        let observation = render_manual_observation_v1(&current, &singleton, &[])?;
        rendered.push(
            observation
                .rsplit_once("Actions:\n")
                .map(|(_, action)| action.trim_start_matches("A0 ").to_owned())
                .ok_or_else(|| "manual action renderer omitted its action section".to_owned())?,
        );
        let prepared = rust
            .prepare(action, &current)
            .map_err(|failure| failure.message)?;
        current = rust.execute(prepared).map_err(|failure| failure.message)?;
    }
    let proposal = if save_proposal {
        let (owner, committed_actions) =
            anchor.ok_or("saving a combat proposal requires a manual source")?;
        let draft = sts_parity_lockstep_runner::ManualCombatDraftV1 {
            schema: "sts-manual-combat-draft-v1".into(),
            session_id: owner.session_id().into(),
            rust_version: env!("CARGO_PKG_VERSION").into(),
            source: Some(sts_parity_lockstep_runner::ManualDraftSourceV1 {
                session_id: owner.session_id().into(),
                committed_actions,
            }),
            root,
            actions,
            terminal: current,
        };
        Some(sts_parity_lockstep_runner::save_manual_combat_draft_v1(
            &owner, &draft,
        )?)
    } else {
        None
    };
    Ok((rendered, proposal))
}

/// Export the selected recorded root before any search or private workspace
/// suffix. The standard case carries the exact position and run continuation;
/// existing offline solver diagnostics consume it without another replay path.
fn export_combat_probe_root_case(
    session: &RunControlSession,
    origin: &str,
    path: &Path,
) -> Result<(), String> {
    use sha2::Digest;
    use sts_oracle_learning::eval::combat_case::{
        capture_production_combat_case_v2, CombatCaseGap, CombatCaseSource,
        CombatCaseWitnessBudgetV1,
    };
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let identity =
        sha2::Sha256::digest(std::fs::read(executable).map_err(|error| error.to_string())?);
    let case = capture_production_combat_case_v2(
        session,
        CombatCaseSource {
            seed: session.run_state.seed,
            ascension: session.run_state.ascension_level,
            generation: 0,
            branch_id: 0,
            parent_id: None,
        },
        CombatCaseGap {
            boundary: "offline_combat_probe_root".into(),
            reason: format!(
                "{origin}; producer=sts_lockstep/{} sha256:{identity:x}; exported before search",
                env!("CARGO_PKG_VERSION"),
            ),
            witness_budget: CombatCaseWitnessBudgetV1::NotRun,
        },
    )?;
    let payload = serde_json::to_vec_pretty(&case).map_err(|error| error.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create probe root case {}: {error}", path.display()))?;
    file.write_all(&payload).map_err(|error| error.to_string())
}

fn parse_host_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<(PathBuf, PathBuf, Option<PathBuf>), String> {
    let mut repository = std::env::current_dir()
        .map_err(|error| format!("failed to read current directory: {error}"))?;
    let mut game_directory = PathBuf::from(DEFAULT_GAME_DIRECTORY);
    let mut storage_seed = None;
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{argument} requires one value"))?;
        match argument.as_str() {
            "--repository" => repository = PathBuf::from(value),
            "--game-directory" => game_directory = PathBuf::from(value),
            "--storage-seed" if storage_seed.is_none() => storage_seed = Some(PathBuf::from(value)),
            _ => return Err(format!("unsupported host argument {argument:?}")),
        }
    }
    Ok((repository, game_directory, storage_seed))
}

fn parse_repository_argument(
    arguments: impl IntoIterator<Item = String>,
) -> Result<PathBuf, String> {
    let mut repository = std::env::current_dir()
        .map_err(|error| format!("failed to read current directory: {error}"))?;
    let mut arguments = arguments.into_iter();
    if let Some(argument) = arguments.next() {
        if argument != "--repository" {
            return Err(format!("unsupported host argument {argument:?}"));
        }
        repository = PathBuf::from(
            arguments
                .next()
                .ok_or_else(|| "--repository requires one value".to_owned())?,
        );
    }
    if arguments.next().is_some() {
        return Err("host command accepts only --repository PATH".to_owned());
    }
    Ok(repository)
}

struct AuditReportV1 {
    exact_domains: usize,
    actions: usize,
    startup: Duration,
    journal_id: String,
    end_boundary: String,
}

struct RunRngBatchCaseResultV1 {
    ordinal: usize,
    outcome: Result<(RunRngProbeReportV1, ParityCorpusAdmissionV1), String>,
}

fn probe_hosted_run_rng_batch(
    arguments: RunRngBatchArgumentsV1,
) -> Result<Vec<RunRngBatchCaseResultV1>, String> {
    let repository = canonical_directory(&arguments.repository, "repository")?;
    check_current_java_oracle_v1(&repository)?;
    restore_host_to_start(&repository)?;
    let mut results = Vec::with_capacity(arguments.seeds.len());
    for (ordinal, seed) in arguments.seeds.into_iter().enumerate() {
        let outcome = probe_one_hosted_run_rng(&repository, seed, arguments.ascension);
        let cleanup = restore_host_to_start(&repository);
        match cleanup {
            Ok(()) => results.push(RunRngBatchCaseResultV1 { ordinal, outcome }),
            Err(cleanup) => {
                return Err(match outcome {
                    Ok(_) => format!("case {ordinal} cleanup failed: {cleanup}"),
                    Err(error) => {
                        format!("case {ordinal} stopped: {error}; cleanup failed: {cleanup}")
                    }
                })
            }
        }
    }
    Ok(results)
}

fn probe_one_hosted_run_rng(
    repository: &Path,
    seed: u64,
    ascension: u8,
) -> Result<(RunRngProbeReportV1, ParityCorpusAdmissionV1), String> {
    let mut java = connect_current_java_oracle_v1(repository, Duration::from_secs(10))?;
    let seed_token = sts_java_parity_live::java_seed_token_for_engine_seed(&seed.to_string())?;
    java.start_run_with_content_profile(
        "IRONCLAD",
        u32::from(ascension),
        Some(&seed_token),
        "all_unlocked",
        Duration::from_secs(10),
    )
    .map_err(|failure| failure.message)?;
    let config = RunControlConfig {
        seed,
        ascension_level: ascension,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config)?;
    let (report, staging_case, java) =
        record_started_run_rng_probe_v1(repository, java, config, bootstrap)?;
    drop(java);
    let admission = admit_exact_parity_corpus_case_v1(
        repository,
        &staging_case,
        ParityCorpusCaseKindV1::RunRngProbe,
    )?;
    Ok((report, admission))
}

fn audit_natural_act1_prefix(arguments: ArgumentsV1) -> Result<AuditReportV1, String> {
    let repository = canonical_directory(&arguments.repository, "repository")?;
    let game_directory = canonical_directory(&arguments.game_directory, "game directory")?;
    let lifecycle = RepositoryLifecycleOwnerV1::acquire_for(
        &repository,
        RepositoryLifecycleRoleV1::LockstepRunner,
    )
    .map_err(|error| format!("runtime ownership unavailable: {error}"))?;
    let storage_registry = OwnedStorageRegistryV1::new(&lifecycle, &game_directory);
    let (prepared, startup) = launch_java_parity_runtime_v1(JavaParityLaunchRequestV1 {
        storage_registry: &storage_registry,
        storage_seed: JavaParityStorageSeedV1::Fresh,
        profile: JavaParityLaunchProfileV1::RunControl,
        initial_boundary: JavaParityInitialBoundaryV1::FreshStart,
    })?;

    let seed_token =
        sts_java_parity_live::java_seed_token_for_engine_seed(&arguments.seed.to_string())?;
    sts_java_parity_live::start_run_with_content_profile(
        prepared.live_output(),
        "IRONCLAD",
        u32::from(arguments.ascension),
        Some(&seed_token),
        "all_unlocked",
        Duration::from_secs(10),
    )?;
    let config = RunControlConfig {
        seed: arguments.seed,
        ascension_level: arguments.ascension,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config)?;
    let (report, _transport) = audit_started_natural_act1_prefix(
        &repository,
        OwnedJavaLockstepTransportV1::new(
            prepared.into_owner().map_err(|error| error.to_string())?,
            Duration::from_secs(10),
        ),
        config,
        bootstrap,
        startup,
    )?;
    Ok(report)
}

fn audit_hosted_natural_act1_prefix(arguments: ArgumentsV1) -> Result<AuditReportV1, String> {
    let repository = canonical_directory(&arguments.repository, "repository")?;
    let result = audit_one_hosted_natural_act1_prefix(&repository, arguments);
    let cleanup = restore_host_to_start(&repository);
    match (result, cleanup) {
        (Ok(report), Ok(())) => Ok(report),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup)) => Err(format!("hosted case cleanup failed: {cleanup}")),
        (Err(error), Err(cleanup)) => {
            Err(format!("{error}; hosted case cleanup failed: {cleanup}"))
        }
    }
}

fn audit_one_hosted_natural_act1_prefix(
    repository: &Path,
    arguments: ArgumentsV1,
) -> Result<AuditReportV1, String> {
    let mut java = connect_current_java_oracle_v1(repository, Duration::from_secs(10))?;
    let seed_token =
        sts_java_parity_live::java_seed_token_for_engine_seed(&arguments.seed.to_string())?;
    java.start_run_with_content_profile(
        "IRONCLAD",
        u32::from(arguments.ascension),
        Some(&seed_token),
        "all_unlocked",
        Duration::from_secs(10),
    )
    .map_err(|failure| failure.message)?;
    let config = RunControlConfig {
        seed: arguments.seed,
        ascension_level: arguments.ascension,
        final_act: true,
        player_class: "Ironclad",
        neow_eligibility: NeowEligibilityV1::Rich,
        ..Default::default()
    };
    let bootstrap = natural_run_bootstrap_v1(&config)?;
    audit_started_natural_act1_prefix(repository, java, config, bootstrap, Duration::ZERO)
        .map(|(report, _java)| report)
}

fn restore_host_to_start(repository: &Path) -> Result<(), String> {
    let mut java = connect_current_java_oracle_v1(repository, Duration::from_secs(10))?;
    let status = java.status().map_err(|failure| failure.message)?;
    if status.frame["boundary"]["kind"].as_str() == Some("main_menu_ready")
        && status.frame["public_base"]["run"].is_null()
    {
        return Ok(());
    }
    java.discard_run_for_next_case()
        .map_err(|failure| failure.message)
}

fn audit_started_natural_act1_prefix<T>(
    repository: &Path,
    mut java_transport: T,
    config: RunControlConfig,
    bootstrap: sts_parity_lockstep::NaturalRunBootstrapV1,
    startup: Duration,
) -> Result<(AuditReportV1, T), String>
where
    T: JavaLockstepTransportV1,
{
    let first = java_transport
        .status()
        .map_err(|failure| format!("Java initial observation failed: {}", failure.message))?;
    if first.frame["boundary"]["kind"].as_str() != Some("run_decision")
        || first.frame["public_base"]["decision"]["kind"].as_str() != Some("event")
        || first.frame["public_base"]["decision"]["event_id"].as_str() != Some("Neow")
    {
        return Err("fresh Java run did not settle at natural Neow".to_owned());
    }
    let journal_id = format!("natural-act1-audit-{}", Uuid::new_v4().simple());
    let journal = repository
        .join(".oracle-lab")
        .join("lockstep-runs")
        .join(&journal_id);
    let evidence_store =
        RecordedJavaEvidenceStoreV1::create_new(&journal.join("java-evidence"), bootstrap.clone())?;
    let java = JavaLockstepAdapterV1::with_evidence_store(
        java_transport,
        bootstrap.clone(),
        evidence_store,
    )?;
    let rust = RustLockstepAdapterV1::new(
        RunControlSession::new(config),
        bootstrap,
        env!("CARGO_PKG_VERSION"),
        Uuid::new_v4().simple().to_string(),
    )?;
    let mut runner =
        LockstepRunnerV1::start_new(java, rust, LockstepModeV1::Audit, &journal, &journal_id)?;
    require_exact(&runner)?;

    step_matching(&mut runner, is_neow_proceed_v1, "Neow introduction")?;
    step_first_matching(
        &mut runner,
        is_immediate_neow_reward_v1,
        "immediate Neow reward",
    )?;
    step_matching(&mut runner, is_neow_proceed_v1, "Neow leave")?;
    step_first_matching(&mut runner, is_map_choice_v1, "first map node")?;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat" || rust.boundary_kind != "natural_combat" {
        return Err("lockstep did not finish at the first combat entry".to_owned());
    }

    let first_combat_actions = win_current_combat(&mut runner, "first combat")?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat_reward"
        || rust.boundary_kind != "natural_combat_reward"
    {
        return Err("lockstep did not finish at the initial combat reward".to_owned());
    }

    step_matching(
        &mut runner,
        is_gold_combat_reward,
        "first combat gold reward",
    )?;
    require_exact(&runner)?;
    let first_optional_reward_actions = claim_optional_combat_rewards(&mut runner, "first combat")?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat_reward"
        || rust.boundary_kind != "natural_combat_reward"
    {
        return Err("claiming the first combat gold did not retain the reward boundary".to_owned());
    }
    step_matching(
        &mut runner,
        is_combat_reward_proceed,
        "first combat reward close",
    )?;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_map" || rust.boundary_kind != "natural_map" {
        return Err("lockstep did not return to the map after the first combat reward".to_owned());
    }

    step_first_matching(
        &mut runner,
        is_monster_map_choice,
        "second-floor combat node",
    )?;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat" || rust.boundary_kind != "natural_combat" {
        return Err("lockstep did not enter the second-floor combat".to_owned());
    }

    let second_combat_actions = win_current_combat(&mut runner, "second combat")?;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat_reward"
        || rust.boundary_kind != "natural_combat_reward"
    {
        return Err("lockstep did not finish at the second combat reward".to_owned());
    }

    step_matching(
        &mut runner,
        is_gold_combat_reward,
        "second combat gold reward",
    )?;
    require_exact(&runner)?;
    let second_optional_reward_actions =
        claim_optional_combat_rewards(&mut runner, "second combat")?;
    step_matching(
        &mut runner,
        is_combat_reward_proceed,
        "second combat reward close",
    )?;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_map" || rust.boundary_kind != "natural_map" {
        return Err("lockstep did not return to the map after the second combat reward".to_owned());
    }

    step_first_matching(
        &mut runner,
        is_question_mark_map_choice,
        "third-floor question-mark node",
    )?;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_event" || rust.boundary_kind != "natural_event" {
        return Err("lockstep did not enter the first natural event".to_owned());
    }

    step_matching(&mut runner, is_bonfire_approach, "Bonfire approach")?;
    require_exact(&runner)?;
    step_matching(&mut runner, is_bonfire_offer, "Bonfire card offer")?;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_grid_select" || rust.boundary_kind != "natural_grid_select" {
        return Err("Bonfire offer did not open the shared deck selection".to_owned());
    }

    step_matching(
        &mut runner,
        is_first_starter_strike_selection,
        "Bonfire starter Strike selection",
    )?;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_event" || rust.boundary_kind != "natural_event" {
        return Err("Bonfire card selection did not reach its completion page".to_owned());
    }

    step_matching(&mut runner, is_bonfire_leave, "Bonfire leave")?;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_map" || rust.boundary_kind != "natural_map" {
        return Err("lockstep did not return to the map after Bonfire".to_owned());
    }
    report_progress("F3 Bonfire successor")?;

    let mut actions = 14
        + first_combat_actions
        + first_optional_reward_actions
        + second_combat_actions
        + second_optional_reward_actions;
    step_first_matching(
        &mut runner,
        is_monster_map_choice,
        "first post-Bonfire combat node",
    )?;
    actions += 1 + win_and_close_combat(&mut runner, "first post-Bonfire combat", false)?;

    step_first_matching(
        &mut runner,
        is_monster_map_choice,
        "second post-Bonfire combat node",
    )?;
    actions += 1 + win_and_close_combat(&mut runner, "second post-Bonfire combat", false)?;

    step_first_matching(&mut runner, is_rest_map_choice, "first campfire node")?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_campfire" || rust.boundary_kind != "natural_campfire" {
        return Err("lockstep did not enter the first campfire".to_owned());
    }

    step_matching(&mut runner, is_campfire_rest, "first campfire Rest")?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_map" || rust.boundary_kind != "natural_map" {
        return Err("lockstep did not return to the map after the first campfire Rest".to_owned());
    }
    report_progress("F6 campfire successor")?;

    step_first_matching(&mut runner, is_elite_map_choice, "first elite node")?;
    actions += 1 + win_and_close_combat(&mut runner, "first elite combat", true)?;
    report_progress("F7 elite successor")?;

    step_first_matching(
        &mut runner,
        is_monster_map_choice,
        "third post-Bonfire combat node",
    )?;
    actions += 1 + win_and_close_combat(&mut runner, "third post-Bonfire combat", false)?;
    report_progress("F8 combat successor")?;

    step_first_matching(&mut runner, is_treasure_map_choice, "first treasure node")?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_treasure" || rust.boundary_kind != "natural_treasure" {
        return Err("lockstep did not enter the first treasure room".to_owned());
    }

    step_matching(&mut runner, is_treasure_open, "first treasure chest open")?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_treasure_reward"
        || rust.boundary_kind != "natural_treasure_reward"
    {
        return Err("opening the first treasure did not reach its reward boundary".to_owned());
    }
    report_progress("F9 treasure reward")?;

    if runner
        .current_action_space()?
        .actions
        .iter()
        .any(|candidate| is_gold_treasure_reward(&candidate.action))
    {
        step_matching(
            &mut runner,
            is_gold_treasure_reward,
            "first treasure gold reward",
        )?;
        actions += 1;
        require_exact(&runner)?;
    }
    step_matching(
        &mut runner,
        is_sapphire_key_treasure_reward,
        "first treasure Sapphire Key",
    )?;
    actions += 1;
    require_exact(&runner)?;
    step_matching(
        &mut runner,
        is_treasure_reward_proceed,
        "first treasure reward close",
    )?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_map" || rust.boundary_kind != "natural_map" {
        return Err("lockstep did not return to the map after the first treasure".to_owned());
    }

    step_first_matching(
        &mut runner,
        is_rest_map_choice,
        "post-treasure campfire node",
    )?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_campfire" || rust.boundary_kind != "natural_campfire" {
        return Err("lockstep did not enter the post-treasure campfire".to_owned());
    }
    step_matching(&mut runner, is_campfire_rest, "post-treasure campfire Rest")?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_map" || rust.boundary_kind != "natural_map" {
        return Err("lockstep did not return to the map after the post-treasure Rest".to_owned());
    }
    report_progress("F10 campfire successor")?;

    step_first_matching(&mut runner, is_elite_map_choice, "second elite node")?;
    actions += 1 + win_and_close_combat(&mut runner, "second elite combat", true)?;
    report_progress("F11 elite successor")?;

    step_first_matching(
        &mut runner,
        is_question_mark_map_choice,
        "first post-treasure question-mark node",
    )?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind == "natural_map" || rust.boundary_kind == "natural_map" {
        return Err("post-treasure question-mark node did not leave the map".to_owned());
    }
    report_progress("F12 question-mark successor")?;

    if java.boundary_kind != "natural_combat" || rust.boundary_kind != "natural_combat" {
        return Err("F12 question-mark did not resolve to the expected combat".to_owned());
    }
    actions += win_and_close_combat(&mut runner, "F12 question-mark combat", false)?;
    report_progress("F12 combat successor")?;

    step_map_node(&mut runner, "F13X03", "monster", "F13 normal-combat node")?;
    actions += 1 + win_and_close_combat(&mut runner, "F13 normal combat", false)?;
    report_progress("F13 combat successor")?;

    step_map_node(&mut runner, "F14X04", "monster", "F14 normal-combat node")?;
    actions += 1 + win_and_close_combat(&mut runner, "F14 normal combat", false)?;
    report_progress("F14 combat successor")?;

    step_map_node(&mut runner, "F15X03", "rest", "final Act 1 campfire")?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_campfire" || rust.boundary_kind != "natural_campfire" {
        return Err("lockstep did not enter the final Act 1 campfire".to_owned());
    }
    step_matching(&mut runner, is_campfire_rest, "final Act 1 campfire Rest")?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_map" || rust.boundary_kind != "natural_map" {
        return Err("final Act 1 Rest did not return to the map".to_owned());
    }
    report_progress("F15 campfire successor")?;

    step_map_node(&mut runner, "F16X00", "boss", "Act 1 boss node")?;
    actions += 1;
    require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat" || rust.boundary_kind != "natural_combat" {
        return Err("lockstep did not enter the Act 1 boss combat".to_owned());
    }
    report_progress("F17 boss combat entry")?;

    actions += win_current_combat(&mut runner, "Act 1 boss combat")?;
    let exact_domains = require_exact(&runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat_reward"
        || rust.boundary_kind != "natural_combat_reward"
    {
        return Err("Act 1 boss combat did not finish at its reward boundary".to_owned());
    }
    let end_boundary = java.boundary_kind.clone();
    report_progress("F17 boss reward")?;

    let (java, _rust) = runner.into_adapters();
    let transport = java.into_transport();
    Ok((
        AuditReportV1 {
            exact_domains,
            actions,
            startup,
            journal_id,
            end_boundary,
        },
        transport,
    ))
}

fn win_and_close_combat<J>(
    runner: &mut LockstepRunnerV1<J, RustLockstepAdapterV1>,
    label: &str,
    claim_relic: bool,
) -> Result<usize, String>
where
    J: sts_parity_lockstep::LockstepAdapterV1,
{
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat" || rust.boundary_kind != "natural_combat" {
        return Err(format!("{label} did not begin at a combat boundary"));
    }
    let combat_actions = win_current_combat(runner, label)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat_reward"
        || rust.boundary_kind != "natural_combat_reward"
    {
        return Err(format!("{label} did not finish at a combat reward"));
    }

    step_matching(
        runner,
        is_gold_combat_reward,
        &format!("{label} gold reward"),
    )?;
    require_exact(runner)?;
    if claim_relic {
        step_matching(
            runner,
            is_relic_combat_reward,
            &format!("{label} relic reward"),
        )?;
        require_exact(runner)?;
    }
    let optional_reward_actions = claim_optional_combat_rewards(runner, label)?;
    step_matching(
        runner,
        is_combat_reward_proceed,
        &format!("{label} reward close"),
    )?;
    require_exact(runner)?;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_map" || rust.boundary_kind != "natural_map" {
        return Err(format!("{label} did not return to the map"));
    }
    Ok(combat_actions + 2 + usize::from(claim_relic) + optional_reward_actions)
}

fn claim_optional_combat_rewards<J>(
    runner: &mut LockstepRunnerV1<J, RustLockstepAdapterV1>,
    label: &str,
) -> Result<usize, String>
where
    J: sts_parity_lockstep::LockstepAdapterV1,
{
    let mut count = 0;
    let potion_is_available = runner
        .current_action_space()?
        .actions
        .iter()
        .any(|candidate| is_potion_combat_reward(&candidate.action));
    let has_empty_potion_slot = runner
        .rust_adapter()
        .session()
        .run_state
        .potions
        .iter()
        .any(Option::is_none);
    if potion_is_available && has_empty_potion_slot {
        step_matching(
            runner,
            is_potion_combat_reward,
            &format!("{label} potion reward"),
        )?;
        require_exact(runner)?;
        count += 1;
    }

    let card_is_available = runner
        .current_action_space()?
        .actions
        .iter()
        .any(|candidate| is_card_combat_reward(&candidate.action));
    if !card_is_available {
        return Ok(count);
    }
    step_matching(
        runner,
        is_card_combat_reward,
        &format!("{label} card reward open"),
    )?;
    require_exact(runner)?;
    count += 1;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_card_reward" || rust.boundary_kind != "natural_card_reward" {
        return Err(format!("{label} did not open a shared card reward"));
    }

    let preferred = runner.rust_adapter().preferred_card_reward_action_v1()?;
    step_action(runner, preferred, &format!("{label} preferred card reward"))?;
    require_exact(runner)?;
    count += 1;
    let (java, rust) = runner.current_snapshots();
    if java.boundary_kind != "natural_combat_reward"
        || rust.boundary_kind != "natural_combat_reward"
    {
        return Err(format!(
            "{label} card choice did not return to the combat reward"
        ));
    }
    Ok(count)
}

fn win_current_combat<J>(
    runner: &mut LockstepRunnerV1<J, RustLockstepAdapterV1>,
    label: &str,
) -> Result<usize, String>
where
    J: sts_parity_lockstep::LockstepAdapterV1,
{
    let mut count = 0;
    for segment_index in 0..50 {
        let (kind, actions) =
            runner
                .rust_adapter()
                .plan_combat_trajectory_v1(AtomicCombatSearchOptionsV2 {
                    max_nodes: Some(2_000),
                    wall_ms: Some(5_000),
                    segment_mode: Some(RunControlCombatSegmentMode::TurnBoundary),
                    ..Default::default()
                })?;
        for (action_index, action) in actions.into_iter().enumerate() {
            step_action(
                runner,
                action,
                &format!("{label} segment {segment_index} action {action_index}"),
            )?;
            count += 1;
        }
        require_exact(runner)?;
        match kind {
            RunCombatResolutionKindV1::CompleteVictory => return Ok(count),
            RunCombatResolutionKindV1::TurnSegment => {
                let (java, rust) = runner.current_snapshots();
                if java.boundary_kind != "natural_combat" || rust.boundary_kind != "natural_combat"
                {
                    return Err(format!(
                        "{label} turn segment did not retain the combat boundary"
                    ));
                }
            }
            RunCombatResolutionKindV1::SmokeBombEscape => {
                return Err(format!("{label} unexpectedly planned a Smoke Bomb escape"))
            }
        }
    }
    Err(format!("{label} exceeded 50 planned turn segments"))
}

fn step_action<J, R>(
    runner: &mut LockstepRunnerV1<J, R>,
    action: SemanticActionV1,
    label: &str,
) -> Result<(), String>
where
    J: sts_parity_lockstep::LockstepAdapterV1,
    R: sts_parity_lockstep::LockstepAdapterV1,
{
    let entry = runner.step(action)?;
    if runner.is_stopped() {
        return Err(runner
            .stop_reason()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{label} stopped without a reason")));
    }
    if !matches!(
        entry.outcome,
        sts_parity_lockstep::TransitionOutcomeV1::Continued { .. }
    ) {
        return Err(format!("{label} did not continue"));
    }
    Ok(())
}

fn step_matching<J, R>(
    runner: &mut LockstepRunnerV1<J, R>,
    predicate: fn(&SemanticActionV1) -> bool,
    label: &str,
) -> Result<(), String>
where
    J: sts_parity_lockstep::LockstepAdapterV1,
    R: sts_parity_lockstep::LockstepAdapterV1,
{
    let space = runner.current_action_space()?;
    let matches = space
        .actions
        .iter()
        .filter(|candidate| predicate(&candidate.action))
        .collect::<Vec<_>>();
    let [selected] = matches.as_slice() else {
        return Err(format!(
            "{label} resolved to {} shared actions instead of one",
            matches.len()
        ));
    };
    let entry = runner.step(selected.action.clone())?;
    if runner.is_stopped() {
        return Err(runner
            .stop_reason()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{label} stopped without a reason")));
    }
    if !matches!(
        entry.outcome,
        sts_parity_lockstep::TransitionOutcomeV1::Continued { .. }
    ) {
        return Err(format!("{label} did not continue"));
    }
    Ok(())
}

fn step_first_matching<J, R>(
    runner: &mut LockstepRunnerV1<J, R>,
    predicate: fn(&SemanticActionV1) -> bool,
    label: &str,
) -> Result<(), String>
where
    J: sts_parity_lockstep::LockstepAdapterV1,
    R: sts_parity_lockstep::LockstepAdapterV1,
{
    let space = runner.current_action_space()?;
    let selected = space
        .actions
        .iter()
        .filter(|candidate| predicate(&candidate.action))
        .min_by_key(|candidate| match &candidate.action {
            SemanticActionV1::Select { choice, .. } => choice.occurrence,
            _ => u32::MAX,
        })
        .ok_or_else(|| format!("{label} resolved to no shared action"))?;
    let entry = runner.step(selected.action.clone())?;
    if runner.is_stopped() {
        return Err(runner
            .stop_reason()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{label} stopped without a reason")));
    }
    if !matches!(
        entry.outcome,
        sts_parity_lockstep::TransitionOutcomeV1::Continued { .. }
    ) {
        return Err(format!("{label} did not continue"));
    }
    Ok(())
}

fn step_map_node<J, R>(
    runner: &mut LockstepRunnerV1<J, R>,
    node_id: &str,
    room: &str,
    label: &str,
) -> Result<(), String>
where
    J: sts_parity_lockstep::LockstepAdapterV1,
    R: sts_parity_lockstep::LockstepAdapterV1,
{
    let space = runner.current_action_space()?;
    let matches = space
        .actions
        .iter()
        .filter(|candidate| {
            matches!(
                &candidate.action,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Map,
                    choice,
                } if choice.kind == "map_node"
                    && choice.id == node_id
                    && choice.attributes.get("room")
                        == Some(&SemanticScalarV1::Text(room.to_owned()))
            )
        })
        .collect::<Vec<_>>();
    let [selected] = matches.as_slice() else {
        return Err(format!(
            "{label} resolved to {} shared actions instead of one",
            matches.len()
        ));
    };
    step_action(runner, selected.action.clone(), label)
}

fn require_exact<J, R>(runner: &LockstepRunnerV1<J, R>) -> Result<usize, String>
where
    J: sts_parity_lockstep::LockstepAdapterV1,
    R: sts_parity_lockstep::LockstepAdapterV1,
{
    let comparison = runner.current_comparison();
    if comparison.status != ComparisonStatusV1::Exact
        || comparison.domains.len() != NATURAL_RUN_SNAPSHOT_DOMAINS_V3.len()
        || comparison
            .domains
            .iter()
            .any(|domain| domain.status != ComparisonStatusV1::Exact)
    {
        return Err(runner
            .stop_reason()
            .map(str::to_owned)
            .unwrap_or_else(|| "current Java/Rust snapshot is not exact".to_owned()));
    }
    Ok(comparison.domains.len())
}

fn report_progress(label: &str) -> Result<(), String> {
    println!("lockstep progress: {label} Exact");
    std::io::stdout()
        .flush()
        .map_err(|error| format!("failed to publish lockstep progress: {error}"))
}

fn is_monster_map_choice(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice,
        } if matches!(
            choice.attributes.get("room"),
            Some(SemanticScalarV1::Text(room)) if room == "monster"
        )
    )
}

fn is_elite_map_choice(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice,
        } if matches!(
            choice.attributes.get("room"),
            Some(SemanticScalarV1::Text(room)) if room == "elite"
        )
    )
}

fn is_treasure_map_choice(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice,
        } if matches!(
            choice.attributes.get("room"),
            Some(SemanticScalarV1::Text(room)) if room == "treasure"
        )
    )
}

fn is_question_mark_map_choice(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice,
        } if matches!(
            choice.attributes.get("room"),
            Some(SemanticScalarV1::Text(room)) if room == "question_mark"
        )
    )
}

fn is_rest_map_choice(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice,
        } if matches!(
            choice.attributes.get("room"),
            Some(SemanticScalarV1::Text(room)) if room == "rest"
        )
    )
}

fn is_campfire_rest(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Campfire,
            choice,
        } if choice.kind == "campfire_option"
            && choice.id == "rest"
            && choice.occurrence == 0
    )
}

fn is_gold_combat_reward(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatReward,
            choice,
        } if choice.kind == "run_reward" && choice.id == "gold"
    )
}

fn is_relic_combat_reward(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatReward,
            choice,
        } if choice.kind == "run_reward" && choice.id == "relic" && choice.occurrence == 0
    )
}

fn is_potion_combat_reward(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatReward,
            choice,
        } if choice.kind == "run_reward" && choice.id == "potion" && choice.occurrence == 0
    )
}

fn is_card_combat_reward(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatReward,
            choice,
        } if choice.kind == "run_reward" && choice.id == "card" && choice.occurrence == 0
    )
}

fn is_combat_reward_proceed(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward
        }
    )
}

fn is_treasure_open(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Treasure,
            choice,
        } if choice.kind == "treasure_chest" && choice.id == "open" && choice.occurrence == 0
    )
}

fn is_gold_treasure_reward(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::TreasureReward,
            choice,
        } if choice.kind == "run_reward" && choice.id == "gold"
    )
}

fn is_sapphire_key_treasure_reward(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::TreasureReward,
            choice,
        } if choice.kind == "run_reward" && choice.id == "sapphire_key"
    )
}

fn is_treasure_reward_proceed(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::TreasureReward
        }
    )
}

fn is_bonfire_approach(action: &SemanticActionV1) -> bool {
    is_bonfire_option_at_screen(action, 0)
}

fn is_bonfire_offer(action: &SemanticActionV1) -> bool {
    is_bonfire_option_at_screen(action, 1)
}

fn is_bonfire_leave(action: &SemanticActionV1) -> bool {
    is_bonfire_option_at_screen(action, 2)
}

fn is_bonfire_option_at_screen(action: &SemanticActionV1, screen: i64) -> bool {
    matches!(
        action,
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Event,
            choice,
        } if choice.kind == "event_option"
            && choice.id == "bonfireelementals"
            && choice.occurrence == 0
            && choice.attributes.get("screen")
                == Some(&SemanticScalarV1::Integer(screen))
    )
}

fn is_first_starter_strike_selection(action: &SemanticActionV1) -> bool {
    matches!(
        action,
        SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices,
        } if matches!(choices.as_slice(), [choice]
            if choice.kind == "deck_card"
                && choice.id == "natural_run/starter/0"
                && choice.occurrence == 0
                && choice.attributes.get("card_id")
                    == Some(&SemanticScalarV1::Text("strike".to_owned()))
                && choice.attributes.get("upgrades")
                    == Some(&SemanticScalarV1::Integer(0)))
    )
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let path = std::fs::canonicalize(path)
        .map_err(|error| format!("failed to resolve {label}: {error}"))?;
    path.is_dir()
        .then_some(path)
        .ok_or_else(|| format!("{label} is not a directory"))
}

fn parse_arguments(arguments: impl IntoIterator<Item = String>) -> Result<ArgumentsV1, String> {
    let repository = std::env::current_dir()
        .map_err(|error| format!("failed to read current directory: {error}"))?;
    let mut parsed = ArgumentsV1 {
        repository,
        game_directory: PathBuf::from(DEFAULT_GAME_DIRECTORY),
        seed: DEFAULT_SEED,
        ascension: DEFAULT_ASCENSION,
    };
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{argument} requires one value"))?;
        match argument.as_str() {
            "--repository" => parsed.repository = PathBuf::from(value),
            "--game-directory" => parsed.game_directory = PathBuf::from(value),
            "--seed" => {
                parsed.seed = value
                    .parse()
                    .map_err(|_| "--seed must be an unsigned 64-bit integer".to_owned())?
            }
            "--ascension" => {
                parsed.ascension = value
                    .parse()
                    .map_err(|_| "--ascension must be an unsigned 8-bit integer".to_owned())?
            }
            _ => return Err(format!("unsupported argument {argument:?}")),
        }
    }
    if parsed.ascension != 20 {
        return Err("natural Act 1 audit is scoped to A20".to_owned());
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_parity_lockstep::SemanticChoiceV1;

    #[test]
    fn resume_target_requires_one_explicit_adopted_source_coordinate() {
        assert!(
            matches!(parse_command(["manual-resume", "--at", "manual-source:8", "--json", "--repository", "repo"].map(str::to_owned)),
            Ok(CommandV1::ManualResume { target: Some((source, 8)), native: false, json: true, .. }) if source == "manual-source")
        );
        for args in [
            vec!["manual-resume", "--at"],
            vec!["manual-resume", "--at", "manual-source:0"],
            vec!["manual-resume", "--at", ":8"],
            vec!["manual-resume", "--at", "manual-source"],
            vec!["manual-resume", "--native", "--at", "manual-source:8"],
            vec![
                "manual-resume",
                "--at",
                "manual-source:8",
                "--at",
                "manual-source:9",
            ],
        ] {
            assert!(parse_command(args.into_iter().map(str::to_owned)).is_err());
        }
    }

    #[test]
    fn combat_retry_parser_keeps_explicit_source_and_action_together() {
        assert!(matches!(
            parse_command(["manual-retry-combat", "--at", "manual-source:12", "--repository", "repo"].map(str::to_owned)),
            Ok(CommandV1::ManualRetryCombat { repository, target: Some((source, 12)) })
                if source == "manual-source" && repository == Path::new("repo")
        ));
        for target in [
            "manual-source",
            "manual-source:0",
            "manual-source:-1",
            ":12",
            "manual-source:1:2",
        ] {
            assert!(
                parse_command(["manual-retry-combat", "--at", target].map(str::to_owned)).is_err()
            );
        }
        assert!(parse_command(["manual-retry-combat", "--at"].map(str::to_owned)).is_err());
        assert!(parse_command(
            [
                "manual-retry-combat",
                "--at",
                "manual-source:1",
                "--at",
                "manual-source:2"
            ]
            .map(str::to_owned)
        )
        .is_err());
    }

    #[test]
    fn inspection_parser_requires_an_explicit_source_and_positive_action_for_targeting() {
        assert!(matches!(
            parse_command(["manual-inspect"].map(str::to_owned)),
            Ok(CommandV1::ManualInspect {
                transition: None,
                output: None,
                ..
            })
        ));
        assert!(
            matches!(parse_command(["manual-inspect", "--session", "manual-source", "--action", "28", "--output", "report.json", "--repository", "repo"].map(str::to_owned)),
            Ok(CommandV1::ManualInspect { transition: Some((session,28)), output: Some(output), repository })
                if session == "manual-source" && output == Path::new("report.json") && repository == Path::new("repo"))
        );
        for args in [
            vec!["--session", "manual-source"],
            vec!["--action", "1"],
            vec!["--output", "report.json"],
            vec!["--session", "manual-source", "--action", "0"],
            vec![
                "--session",
                "manual-source",
                "--action",
                "1",
                "--action",
                "2",
            ],
            vec!["--session", "manual-source", "--action", "bad"],
            vec!["--unknown", "value"],
        ] {
            assert!(parse_command(
                std::iter::once("manual-inspect")
                    .chain(args)
                    .map(str::to_owned)
            )
            .is_err());
        }
    }

    #[test]
    fn decision_workspace_parser_separates_resume_from_private_source_selection() {
        for (arguments, expected_reference, expected_at) in [
            (vec!["workspace"], None, None),
            (vec!["workspace", "W01234567"], Some("W01234567"), None),
            (vec!["workspace", "--at", "0"], None, Some(0)),
            (vec!["workspace", "--at", "14"], None, Some(14)),
        ] {
            let CommandV1::Workspace {
                repository,
                reference,
                at,
            } = parse_command(
                arguments
                    .into_iter()
                    .chain(["--repository", "repo"])
                    .map(str::to_owned),
            )
            .unwrap()
            else {
                panic!("expected decision workspace");
            };
            assert_eq!(repository, Path::new("repo"));
            assert_eq!(reference.as_deref(), expected_reference);
            assert_eq!(at, expected_at);
        }
        for arguments in [
            vec!["workspace", "--at"],
            vec!["workspace", "--at", "-1"],
            vec!["workspace", "--at", "1", "W01234567"],
            vec!["workspace", "W01234567", "--at", "1"],
            vec!["workspace", "--unknown", "1"],
        ] {
            assert!(parse_command(arguments.into_iter().map(str::to_owned)).is_err());
        }
    }

    #[test]
    fn private_draft_parser_keeps_explicit_resume_and_new_separate() {
        for reference in [None, Some("new"), Some("list"), Some("W01234567")] {
            let mut args = vec!["manual-draft".to_owned()];
            args.extend(reference.map(str::to_owned));
            args.extend(["--repository".into(), "repo".into()]);
            let CommandV1::ManualDraft {
                repository,
                reference: parsed,
            } = parse_command(args).unwrap()
            else {
                panic!("expected private draft")
            };
            assert_eq!(repository, Path::new("repo"));
            assert_eq!(parsed.as_deref(), reference);
        }
        assert!(parse_command(["manual-draft", "new", "W01234567"].map(str::to_owned)).is_err());
    }

    #[test]
    fn manual_commands_keep_start_configuration_separate_from_resume() {
        let start = parse_command([
            "manual-start".to_owned(),
            "--repository".to_owned(),
            "repo".to_owned(),
            "--seed".to_owned(),
            "42".to_owned(),
        ])
        .unwrap();
        let CommandV1::ManualStart(arguments) = start else {
            panic!("expected manual start command")
        };
        assert_eq!(arguments.repository, Path::new("repo"));
        assert_eq!(arguments.seed, 42);
        assert_eq!(arguments.ascension, 20);
        assert!(!arguments.replace_active);

        let replacement = parse_command([
            "manual-start".to_owned(),
            "--replace-active".to_owned(),
            "--repository".to_owned(),
            "repo".to_owned(),
        ])
        .unwrap();
        let CommandV1::ManualStart(replacement) = replacement else {
            panic!("expected replacement manual start command")
        };
        assert!(replacement.replace_active);

        assert!(matches!(
            parse_command([
                "manual-resume".to_owned(),
                "--repository".to_owned(),
                "repo".to_owned(),
            ]),
            Ok(CommandV1::ManualResume { repository, native: false, json: false, target: None }) if repository == Path::new("repo")
        ));
        assert!(matches!(
            parse_command([
                "manual-retry-combat".to_owned(),
                "--repository".to_owned(),
                "repo".to_owned(),
            ]),
            Ok(CommandV1::ManualRetryCombat { repository, target: None }) if repository == Path::new("repo")
        ));
        for native in [false, true] {
            let mut args = vec!["manual-resume".to_owned()];
            if native {
                args.push("--native".into());
            }
            args.extend(["--json".into(), "--repository".into(), "repo".into()]);
            assert!(
                matches!(parse_command(args), Ok(CommandV1::ManualResume { native: n, json: true, .. }) if n == native)
            );
        }
        assert!(parse_command([
            "manual-resume".to_owned(),
            "--seed".to_owned(),
            "42".to_owned(),
        ])
        .is_err());
    }

    #[test]
    fn run_rng_batch_parser_requires_multiple_explicit_seeds() {
        assert!(parse_command([
            "probe-hosted-run-rng".to_owned(),
            "--seed".to_owned(),
            "7".to_owned(),
        ])
        .is_err());

        let command = parse_command([
            "probe-hosted-run-rng".to_owned(),
            "--seed".to_owned(),
            "7".to_owned(),
            "--seed".to_owned(),
            "9".to_owned(),
        ])
        .unwrap();
        let CommandV1::ProbeHostedRunRng(arguments) = command else {
            panic!("expected Run + RNG batch command")
        };
        assert_eq!(arguments.seeds, [7, 9]);
        assert_eq!(arguments.ascension, 20);
    }

    #[test]
    fn immediate_neow_selector_excludes_follow_up_screens() {
        let choice = SemanticChoiceV1::new("neow_reward", "hundred_gold", 0);
        assert!(is_immediate_neow_reward_v1(&SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Neow,
            choice,
        }));
        assert!(!is_immediate_neow_reward_v1(&SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Neow,
            choice: SemanticChoiceV1::new("neow_reward", "transform_card", 0),
        }));
        assert!(!is_immediate_neow_reward_v1(&SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Neow,
            choice: SemanticChoiceV1::new("neow_reward", "three_small_potions", 1),
        }));
    }

    #[test]
    fn first_reward_selectors_distinguish_gold_from_close() {
        let gold = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatReward,
            choice: SemanticChoiceV1::new("run_reward", "gold", 0),
        };
        let card = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatReward,
            choice: SemanticChoiceV1::new("run_reward", "card", 0),
        };
        let relic = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatReward,
            choice: SemanticChoiceV1::new("run_reward", "relic", 0),
        };
        let potion = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::CombatReward,
            choice: SemanticChoiceV1::new("run_reward", "potion", 0),
        };
        let close = SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward,
        };

        assert!(is_gold_combat_reward(&gold));
        assert!(!is_gold_combat_reward(&card));
        assert!(!is_gold_combat_reward(&close));
        assert!(is_relic_combat_reward(&relic));
        assert!(!is_relic_combat_reward(&gold));
        assert!(!is_relic_combat_reward(&card));
        assert!(is_card_combat_reward(&card));
        assert!(!is_card_combat_reward(&potion));
        assert!(is_potion_combat_reward(&potion));
        assert!(!is_potion_combat_reward(&card));
        assert!(is_combat_reward_proceed(&close));
        assert!(!is_combat_reward_proceed(&gold));
    }

    #[test]
    fn treasure_selectors_keep_map_open_reward_and_close_surfaces_distinct() {
        let mut node = SemanticChoiceV1::new("map_node", "F09X03", 0);
        node.attributes.insert(
            "room".to_owned(),
            SemanticScalarV1::Text("treasure".to_owned()),
        );
        let open = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Treasure,
            choice: SemanticChoiceV1::new("treasure_chest", "open", 0),
        };
        let gold = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::TreasureReward,
            choice: SemanticChoiceV1::new("run_reward", "gold", 0),
        };
        let sapphire = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::TreasureReward,
            choice: SemanticChoiceV1::new("run_reward", "sapphire_key", 0),
        };
        let close = SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::TreasureReward,
        };

        assert!(is_treasure_map_choice(&SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice: node,
        }));
        assert!(is_treasure_open(&open));
        assert!(is_gold_treasure_reward(&gold));
        assert!(is_sapphire_key_treasure_reward(&sapphire));
        assert!(is_treasure_reward_proceed(&close));
        assert!(!is_gold_combat_reward(&gold));
        assert!(!is_treasure_open(&sapphire));
    }

    #[test]
    fn second_floor_selector_accepts_only_monster_map_nodes() {
        let mut monster = SemanticChoiceV1::new("map_node", "F02X02", 0);
        monster.attributes.insert(
            "room".to_owned(),
            SemanticScalarV1::Text("monster".to_owned()),
        );
        let mut shop = SemanticChoiceV1::new("map_node", "F02X00", 0);
        shop.attributes
            .insert("room".to_owned(), SemanticScalarV1::Text("shop".to_owned()));
        let mut question = SemanticChoiceV1::new("map_node", "F03X03", 0);
        question.attributes.insert(
            "room".to_owned(),
            SemanticScalarV1::Text("question_mark".to_owned()),
        );
        let mut rest = SemanticChoiceV1::new("map_node", "F06X03", 0);
        rest.attributes
            .insert("room".to_owned(), SemanticScalarV1::Text("rest".to_owned()));

        assert!(is_monster_map_choice(&SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice: monster,
        }));
        assert!(!is_monster_map_choice(&SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice: shop,
        }));
        assert!(!is_monster_map_choice(&SemanticActionV1::Proceed {
            surface: DecisionSurfaceV1::CombatReward,
        }));
        assert!(is_question_mark_map_choice(&SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice: question,
        }));
        assert!(is_rest_map_choice(&SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Map,
            choice: rest,
        }));
    }

    #[test]
    fn campfire_rest_selector_accepts_only_the_stable_primary_option() {
        let rest = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Campfire,
            choice: SemanticChoiceV1::new("campfire_option", "rest", 0),
        };
        let smith = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Campfire,
            choice: SemanticChoiceV1::new("campfire_option", "smith", 0),
        };
        let repeated = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Campfire,
            choice: SemanticChoiceV1::new("campfire_option", "rest", 1),
        };

        assert!(is_campfire_rest(&rest));
        assert!(!is_campfire_rest(&smith));
        assert!(!is_campfire_rest(&repeated));
    }

    #[test]
    fn bonfire_selectors_are_bound_to_stable_event_screen_and_deck_source() {
        let mut option = SemanticChoiceV1::new("event_option", "bonfireelementals", 0);
        option
            .attributes
            .insert("screen".to_owned(), SemanticScalarV1::Integer(0));
        let approach = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Event,
            choice: option.clone(),
        };
        assert!(is_bonfire_approach(&approach));
        assert!(!is_bonfire_offer(&approach));
        option
            .attributes
            .insert("screen".to_owned(), SemanticScalarV1::Integer(2));
        assert!(is_bonfire_leave(&SemanticActionV1::Select {
            surface: DecisionSurfaceV1::Event,
            choice: option,
        }));

        let mut card = SemanticChoiceV1::new("deck_card", "natural_run/starter/0", 0);
        card.attributes.insert(
            "card_id".to_owned(),
            SemanticScalarV1::Text("strike".to_owned()),
        );
        card.attributes
            .insert("upgrades".to_owned(), SemanticScalarV1::Integer(0));
        let selection = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::GridSelect,
            choices: vec![card.clone()],
        };
        assert!(is_first_starter_strike_selection(&selection));
        card.id = "natural_run/starter/1".to_owned();
        assert!(!is_first_starter_strike_selection(
            &SemanticActionV1::SubmitSelection {
                surface: DecisionSurfaceV1::GridSelect,
                choices: vec![card],
            }
        ));
    }

    #[test]
    fn parser_rejects_unknown_or_unpaired_arguments() {
        assert!(parse_arguments(["--seed".to_owned()]).is_err());
        assert!(parse_arguments(["--unknown".to_owned(), "1".to_owned()]).is_err());
    }

    #[test]
    fn replay_parser_requires_one_evidence_directory() {
        assert!(matches!(
            parse_command(["replay".to_owned(), "--evidence".to_owned(), "case".to_owned()]),
            Ok(CommandV1::Replay { evidence }) if evidence.as_path() == Path::new("case")
        ));
        assert!(parse_command(["replay".to_owned()]).is_err());
        assert!(parse_command([
            "replay".to_owned(),
            "--evidence".to_owned(),
            "case".to_owned(),
            "extra".to_owned(),
        ])
        .is_err());
    }

    #[test]
    fn training_replay_requires_evidence_and_one_separate_output() {
        assert!(matches!(parse_command([
            "replay", "--evidence", "case", "--learning-output", "training.json",
        ].map(String::from)), Ok(CommandV1::ReplayTraining { source:RecordedReplaySourceV1::Evidence(evidence), output })
            if evidence == Path::new("case") && output == Path::new("training.json")));
        assert!(matches!(parse_command([
            "replay", "--session", "manual-selected", "--repository", "repo", "--learning-output", "training.json",
        ].map(String::from)), Ok(CommandV1::ReplayTraining { source:RecordedReplaySourceV1::Manual {repository,session,through:None},output })
            if repository==Path::new("repo")&&session=="manual-selected"&&output==Path::new("training.json")));
        for count in ["0", "13"] {
            assert!(matches!(parse_command([
                "replay", "--session", "manual-selected", "--through", count, "--learning-output", "training.json",
            ].map(String::from)), Ok(CommandV1::ReplayTraining { source:RecordedReplaySourceV1::Manual {through:Some(value),..},.. })
                if value==count.parse::<usize>().unwrap()));
        }
        for args in [
            vec!["replay", "--learning-output", "out"],
            vec!["replay", "--suite", "suite", "--learning-output", "out"],
            vec!["replay", "--session", "manual-selected"],
            vec![
                "replay",
                "--session",
                "manual-selected",
                "--through",
                "-1",
                "--learning-output",
                "out",
            ],
            vec![
                "replay",
                "--session",
                "manual-selected",
                "--through",
                "1",
                "--through",
                "2",
                "--learning-output",
                "out",
            ],
            vec![
                "replay",
                "--evidence",
                "case",
                "--through",
                "1",
                "--learning-output",
                "out",
            ],
            vec![
                "replay",
                "--session",
                "manual-selected",
                "--evidence",
                "case",
                "--learning-output",
                "out",
            ],
            vec![
                "replay",
                "--evidence",
                "case",
                "--repository",
                "repo",
                "--learning-output",
                "out",
            ],
            vec![
                "replay",
                "--evidence",
                "case",
                "--output",
                "out",
                "--learning-output",
                "other",
            ],
        ] {
            assert!(parse_command(args.into_iter().map(String::from)).is_err());
        }
    }

    #[test]
    fn replay_suite_parser_requires_one_suite_and_new_output() {
        assert!(matches!(
            parse_command(["replay", "--suite", "suite.json", "--output", "report.json"].map(String::from)),
            Ok(CommandV1::ReplaySuite { suite, output }) if suite == Path::new("suite.json") && output == Path::new("report.json")
        ));
        for arguments in [
            vec!["replay", "--suite", "suite.json"],
            vec![
                "replay",
                "--evidence",
                "evidence",
                "--output",
                "report.json",
            ],
            vec![
                "replay",
                "--suite",
                "suite.json",
                "--evidence",
                "evidence",
                "--output",
                "report.json",
            ],
            vec![
                "replay",
                "--suite",
                "a",
                "--suite",
                "b",
                "--output",
                "report.json",
            ],
        ] {
            assert!(parse_command(arguments.into_iter().map(String::from)).is_err());
        }
    }

    #[test]
    fn corpus_replay_parser_accepts_only_a_repository() {
        assert!(matches!(
            parse_command([
                "replay-corpus".to_owned(),
                "--repository".to_owned(),
                "repo".to_owned(),
            ]),
            Ok(CommandV1::ReplayCorpus { repository }) if repository == Path::new("repo")
        ));
        assert!(parse_command(["replay-corpus".to_owned(), "extra".to_owned()]).is_err());
    }

    #[test]
    fn combat_probe_parser_requires_evidence_and_positive_budgets() {
        assert!(matches!(
            parse_command([
                "probe-combat".to_owned(),
                "--evidence".to_owned(),
                "case".to_owned(),
                "--work".to_owned(),
                "123".to_owned(),
                "--wall-ms".to_owned(),
                "456".to_owned(),
            ]),
            Ok(CommandV1::ProbeCombat {
                source: RecordedReplaySourceV1::Evidence(evidence),
                work: 123,
                wall_ms: 456,
                save_proposal: false,
                workspace: None,
                max_hp_loss: None,
                export_root_case: None,
            }) if evidence.as_path() == Path::new("case")
        ));
        assert!(parse_command(["probe-combat".to_owned()]).is_err());
        assert!(parse_command([
            "probe-combat".to_owned(),
            "--evidence".to_owned(),
            "case".to_owned(),
            "--work".to_owned(),
            "0".to_owned(),
        ])
        .is_err());
    }

    #[test]
    fn combat_probe_reuses_manual_prefixes_and_saves_only_explicit_session_proposals() {
        assert!(matches!(
            parse_command(["probe-combat", "--session", "source", "--repository", "repo", "--through", "185", "--workspace", "W01234567", "--max-hp-loss", "0", "--save-proposal"].map(str::to_owned)),
            Ok(CommandV1::ProbeCombat {
                source: RecordedReplaySourceV1::Manual { repository, session, through: Some(185) },
                save_proposal: true, workspace:Some(workspace), max_hp_loss:Some(0), ..
            }) if repository == Path::new("repo") && session == "source" && workspace == "W01234567"
        ));
        assert!(matches!(
            parse_command(["probe-combat", "--session", "source", "--through", "146", "--export-root-case", "root.json"].map(str::to_owned)),
            Ok(CommandV1::ProbeCombat { export_root_case: Some(path), .. }) if path == Path::new("root.json")
        ));
        assert!(parse_command(
            [
                "probe-combat",
                "--session",
                "source",
                "--export-root-case",
                "one.json",
                "--export-root-case",
                "two.json"
            ]
            .map(str::to_owned)
        )
        .is_err());
        for arguments in [
            vec!["probe-combat", "--evidence", "case", "--session", "source"],
            vec!["probe-combat", "--session", "source", "--max-nodes", "100"],
            vec!["probe-combat", "--evidence", "case", "--through", "1"],
            vec!["probe-combat", "--evidence", "case", "--save-proposal"],
            vec![
                "probe-combat",
                "--evidence",
                "case",
                "--workspace",
                "W01234567",
            ],
            vec![
                "probe-combat",
                "--session",
                "source",
                "--workspace",
                "W01234567",
                "--workspace",
                "W89abcdef",
            ],
            vec!["probe-combat", "--session", "source", "--through", "-1"],
            vec!["probe-combat", "--session", "source", "--max-hp-loss", "-1"],
            vec![
                "probe-combat",
                "--session",
                "source",
                "--max-hp-loss",
                "1",
                "--max-hp-loss",
                "2",
            ],
            vec!["probe-combat", "--session", "source", "--session", "other"],
            vec![
                "probe-combat",
                "--session",
                "source",
                "--save-proposal",
                "--save-proposal",
            ],
        ] {
            assert!(parse_command(arguments.into_iter().map(str::to_owned)).is_err());
        }
    }

    #[test]
    fn combat_probe_export_preserves_exact_root_and_refuses_to_replace_evidence() {
        use sts_oracle_learning::eval::combat_case::load_combat_case;
        use sts_parity_lockstep::{DomainEvidenceV1, ParityDomainV1};

        let config = RunControlConfig {
            seed: 2_026_090_301,
            ascension_level: 20,
            player_class: "Silent",
            neow_eligibility: NeowEligibilityV1::Mini,
            ..Default::default()
        };
        let bootstrap = natural_run_bootstrap_v1(&config).unwrap();
        let mut rust = RustLockstepAdapterV1::new(
            RunControlSession::new(config),
            bootstrap,
            "export-test",
            "export-test-epoch",
        )
        .unwrap();
        for _ in 0..4 {
            let snapshot = rust.observe().unwrap();
            let DomainEvidenceV1::Observed { value, .. } =
                &snapshot.domains[&ParityDomainV1::ActionSpace].evidence
            else {
                panic!("action space")
            };
            let actions: Vec<SemanticActionV1> = serde_json::from_value(value.clone()).unwrap();
            let action = actions
                .into_iter()
                .find(|action| {
                    matches!(
                        action,
                        SemanticActionV1::Proceed {
                            surface: DecisionSurfaceV1::Neow
                        } | SemanticActionV1::Select {
                            surface: DecisionSurfaceV1::Neow | DecisionSurfaceV1::Map,
                            ..
                        }
                    )
                })
                .unwrap();
            let prepared = rust.prepare(&action, &snapshot).unwrap();
            rust.execute(prepared).unwrap();
        }
        let before = rust.observe().unwrap();
        assert_eq!(before.boundary_kind, "natural_combat");
        let exact = rust.session().current_active_combat_position().unwrap();
        let path = std::env::temp_dir().join(format!("sts-probe-root-{}.json", Uuid::new_v4()));
        export_combat_probe_root_case(rust.session(), "manual export-test through 4", &path)
            .unwrap();
        let payload = std::fs::read(&path).unwrap();
        let case = load_combat_case(&path).unwrap();
        assert_eq!(
            serde_json::to_value(&case.core.position).unwrap(),
            serde_json::to_value(exact).unwrap()
        );
        case.replay_identity_v2().unwrap();
        assert!(case
            .core
            .gap
            .reason
            .contains("manual export-test through 4"));
        assert!(case.core.gap.reason.contains("sha256:"));
        assert_eq!(
            case.core.gap.witness_budget,
            sts_oracle_learning::eval::combat_case::CombatCaseWitnessBudgetV1::NotRun
        );
        assert!(export_combat_probe_root_case(rust.session(), "different source", &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), payload);
        assert_eq!(rust.observe().unwrap(), before);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn act_certification_parser_requires_an_explicit_valid_act() {
        for act in 1..=4 {
            assert!(matches!(
                parse_command(["manual-certify-act".to_owned(), act.to_string(), "--offline".to_owned()]),
                Ok(CommandV1::ManualCertifyAct { act: selected, offline_only: true, .. }) if selected == act
            ));
        }
        for invalid in ["0", "5", "255", "-1", "--offline"] {
            assert!(parse_command(["manual-certify-act".to_owned(), invalid.to_owned()]).is_err());
        }
        assert!(parse_command(["manual-certify-act".to_owned()]).is_err());
    }

    #[test]
    fn host_commands_are_explicit_and_do_not_accept_run_arguments() {
        assert!(matches!(
            parse_command(["host-status".to_owned()]),
            Ok(CommandV1::HostStatus { .. })
        ));
        assert!(matches!(
            parse_command(["host-stop".to_owned()]),
            Ok(CommandV1::HostStop { .. })
        ));
        assert!(matches!(
            parse_command(["host-launch".to_owned()]),
            Ok(CommandV1::HostLaunch { .. })
        ));
        assert!(matches!(
            parse_command([
                "host-launch".to_owned(),
                "--storage-seed".to_owned(),
                "seed".to_owned(),
            ]),
            Ok(CommandV1::HostLaunch {
                storage_seed: Some(path),
                ..
            }) if path == Path::new("seed")
        ));
        assert!(parse_command(["host".to_owned(), "--seed".to_owned(), "7".to_owned(),]).is_err());
        assert!(matches!(
            parse_command(["record".to_owned()]),
            Ok(CommandV1::RecordHosted(_))
        ));
    }
}
