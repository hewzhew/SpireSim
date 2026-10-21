use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use sts_java_parity_live::{
    act, bundle_elixir_hand_select_reports, compile_mirror_plan_with_scope,
    compile_mirror_suffix_batch, compile_scenario_case, compile_scenario_plan,
    compile_scenario_probe_case, compile_scenario_trajectory_case,
    continue_run_with_content_profile, render_status,
    reset_natural_run_to_main_menu_preserving_save, reset_scenario_to_main_menu,
    run_constructed_combat_probe, run_constructed_combat_scenario,
    run_constructed_combat_trajectory, run_elixir_hand_select_probe, run_mirror_plan,
    run_mirror_suffix, run_natural_base_strike_probe, run_scenario_reuse_probe,
    run_scenario_transition_reuse_probe, scenario_case_catalog, scenario_probe_case_catalog,
    scenario_registry, scenario_trajectory_case_catalog, serve, start_run, status,
    submit_grid_select, submit_hand_select, ElixirProbeVariant,
};

#[derive(Debug, Parser)]
#[command(name = "sts_java_parity_live")]
#[command(about = "Run or control a durable CommunicationMod parity session")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run as the long-lived child process launched by CommunicationMod.
    Serve {
        #[arg(long)]
        output_dir: PathBuf,
    },
    /// Show the newest live session, latest frame, and frame-bound action handles.
    Status {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Submit one action handle such as f3:a2 and wait for its durable receipt.
    Act {
        #[arg(long)]
        output_dir: PathBuf,
        selector: String,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Start one run from the current typed main-menu frame.
    StartRun {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        character: String,
        #[arg(long)]
        ascension: u32,
        #[arg(long)]
        seed: Option<String>,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Resume one native run with an explicit in-memory content profile.
    ContinueRun {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long, default_value = "current_profile")]
        content_profile: String,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Atomically submit an ordered set of card UUIDs at a typed HAND_SELECT frame.
    HandSelect {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long = "card-uuid")]
        card_uuids: Vec<String>,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Submit the UUID selected at an authorized typed GRID_SELECT frame.
    GridSelect {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long = "card-uuid", required = true)]
        card_uuids: Vec<String>,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Reset one constructed scenario run to a proven fresh main-menu epoch.
    ResetScenario {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Return a natural combat to the main menu without deleting its native autosave.
    ResetNaturalRun {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Run one fresh-session natural Elixir HAND_SELECT probe variant.
    ProbeElixir {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        variant: String,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Reach the first combat naturally and capture one typed base Strike transition.
    ProbeNaturalStrike {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Compile a retained path into a read-only continuous mirror plan.
    CompileMirrorPlan {
        #[arg(long)]
        run_play_head: PathBuf,
        #[arg(long)]
        output_plan: PathBuf,
        #[arg(long)]
        seed: String,
        #[arg(long, default_value = "IRONCLAD")]
        character: String,
        #[arg(long, default_value_t = 20)]
        ascension: u32,
        #[arg(long, default_value = "first-reward")]
        scope: String,
    },
    /// Execute one compiled continuous mirror prefix in a fresh Java run.
    RunMirrorPlan {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        mirror_plan: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Restore one validated native combat checkpoint and execute only its exact suffix.
    RunMirrorSuffix {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        mirror_plan: PathBuf,
        #[arg(long)]
        checkpoint_manifest: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Bind two to sixty-four exact checkpoint suffix members before Java launch.
    CompileMirrorSuffixBatch {
        #[arg(long)]
        checkpoint_manifest: PathBuf,
        #[arg(long = "mirror-plan", required = true)]
        mirror_plans: Vec<PathBuf>,
        #[arg(long)]
        output_plan: PathBuf,
    },
    /// Bind the two passing natural Elixir probe variants into one proof bundle.
    BundleElixirProbe {
        #[arg(long = "report", num_args = 2)]
        reports: Vec<PathBuf>,
        #[arg(long)]
        output_path: PathBuf,
    },
    /// Show the closed scenario recipes and capability dependencies.
    ScenarioRegistry,
    /// Validate and show every exact-ID draft in a scenario case catalog.
    ScenarioCaseCatalog {
        #[arg(long)]
        catalog_directory: PathBuf,
    },
    /// Validate and show every exact-ID probe draft in a probe case catalog.
    ScenarioProbeCaseCatalog {
        #[arg(long)]
        catalog_directory: PathBuf,
    },
    /// Validate and show every exact-ID multi-step trajectory draft.
    ScenarioTrajectoryCaseCatalog {
        #[arg(long)]
        catalog_directory: PathBuf,
    },
    /// Compile a human-authored typed draft into one fresh immutable scenario plan.
    CompileScenario {
        #[arg(long)]
        draft: PathBuf,
        #[arg(long)]
        output_plan: PathBuf,
    },
    /// Compile one exact case ID from a validated catalog into a fresh immutable plan.
    CompileScenarioCase {
        #[arg(long)]
        catalog_directory: PathBuf,
        #[arg(long)]
        case_id: String,
        #[arg(long)]
        output_plan: PathBuf,
    },
    /// Compile one exact probe case and its bound scenario root plans before launch.
    CompileScenarioProbeCase {
        #[arg(long)]
        probe_catalog_directory: PathBuf,
        #[arg(long)]
        probe_case_id: String,
        #[arg(long)]
        scenario_catalog_directory: PathBuf,
        #[arg(long)]
        output_scenario_plan: PathBuf,
        #[arg(long)]
        output_probe_plan: PathBuf,
    },
    /// Compile one exact multi-step trajectory and its root before launch.
    CompileScenarioTrajectoryCase {
        #[arg(long)]
        trajectory_catalog_directory: PathBuf,
        #[arg(long)]
        trajectory_case_id: String,
        #[arg(long)]
        scenario_catalog_directory: PathBuf,
        #[arg(long)]
        output_scenario_plan: PathBuf,
        #[arg(long)]
        output_trajectory_plan: PathBuf,
    },
    /// Construct one typed combat from a fresh scenario-profile main menu and prove its root.
    RunScenario {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        scenario_plan: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Construct one typed combat, submit one registered transition, and preserve its receipt.
    RunScenarioProbe {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        probe_plan: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        scenario_report_path: PathBuf,
        #[arg(long)]
        probe_report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Construct one typed combat and execute its pre-bound ordered trajectory.
    RunScenarioTrajectory {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        trajectory_plan: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        scenario_report_path: PathBuf,
        #[arg(long)]
        trajectory_report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Prove two constructed roots separated by a typed reset in one shared runtime.
    RunScenarioReuseProbe {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        first_scenario_plan: PathBuf,
        #[arg(long)]
        second_scenario_plan: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        first_report_path: PathBuf,
        #[arg(long)]
        reset_report_path: PathBuf,
        #[arg(long)]
        second_report_path: PathBuf,
        #[arg(long)]
        report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Prove two captured transitions separated by a typed reset in one shared runtime.
    RunScenarioTransitionReuseProbe {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        first_probe_plan: PathBuf,
        #[arg(long)]
        second_probe_plan: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        first_root_report_path: PathBuf,
        #[arg(long)]
        first_probe_report_path: PathBuf,
        #[arg(long)]
        reset_report_path: PathBuf,
        #[arg(long)]
        second_root_report_path: PathBuf,
        #[arg(long)]
        second_probe_report_path: PathBuf,
        #[arg(long)]
        report_path: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Serve { output_dir } => serve(&output_dir).map(|_| String::new()),
        Command::Status { output_dir, json } => status(&output_dir).and_then(|view| {
            if json {
                serde_json::to_string_pretty(&view)
                    .map_err(|error| format!("failed to encode live status: {error}"))
            } else {
                Ok(render_status(&view))
            }
        }),
        Command::Act {
            output_dir,
            selector,
            timeout_seconds,
        } => {
            act(&output_dir, &selector, Duration::from_secs(timeout_seconds)).and_then(|receipt| {
                serde_json::to_string_pretty(&receipt)
                    .map_err(|error| format!("failed to encode action receipt: {error}"))
            })
        }
        Command::StartRun {
            output_dir,
            character,
            ascension,
            seed,
            timeout_seconds,
        } => start_run(
            &output_dir,
            &character,
            ascension,
            seed.as_deref(),
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|receipt| {
            serde_json::to_string_pretty(&receipt)
                .map_err(|error| format!("failed to encode action receipt: {error}"))
        }),
        Command::ContinueRun {
            output_dir,
            content_profile,
            timeout_seconds,
        } => continue_run_with_content_profile(
            &output_dir,
            &content_profile,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|receipt| {
            serde_json::to_string_pretty(&receipt)
                .map_err(|error| format!("failed to encode action receipt: {error}"))
        }),
        Command::HandSelect {
            output_dir,
            card_uuids,
            timeout_seconds,
        } => submit_hand_select(
            &output_dir,
            &card_uuids,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|receipt| {
            serde_json::to_string_pretty(&receipt)
                .map_err(|error| format!("failed to encode action receipt: {error}"))
        }),
        Command::GridSelect {
            output_dir,
            card_uuids,
            timeout_seconds,
        } => submit_grid_select(
            &output_dir,
            &card_uuids,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|receipt| {
            serde_json::to_string_pretty(&receipt)
                .map_err(|error| format!("failed to encode action receipt: {error}"))
        }),
        Command::ResetScenario {
            output_dir,
            report_path,
            timeout_seconds,
        } => reset_scenario_to_main_menu(
            &output_dir,
            &report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to encode scenario reset report: {error}"))
        }),
        Command::ResetNaturalRun {
            output_dir,
            report_path,
            timeout_seconds,
        } => reset_natural_run_to_main_menu_preserving_save(
            &output_dir,
            &report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to encode natural run reset report: {error}"))
        }),
        Command::ProbeElixir {
            output_dir,
            variant,
            expected_launch_plan,
            report_path,
            timeout_seconds,
        } => ElixirProbeVariant::parse(&variant)
            .and_then(|variant| {
                run_elixir_hand_select_probe(
                    &output_dir,
                    variant,
                    &expected_launch_plan,
                    &report_path,
                    Duration::from_secs(timeout_seconds),
                )
            })
            .and_then(|report| {
                serde_json::to_string_pretty(&report)
                    .map_err(|error| format!("failed to encode natural probe report: {error}"))
            }),
        Command::ProbeNaturalStrike {
            output_dir,
            expected_launch_plan,
            report_path,
            timeout_seconds,
        } => run_natural_base_strike_probe(
            &output_dir,
            &expected_launch_plan,
            &report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to encode natural combat report: {error}"))
        }),
        Command::CompileMirrorPlan {
            run_play_head,
            output_plan,
            seed,
            character,
            ascension,
            scope,
        } => compile_mirror_plan_with_scope(
            &run_play_head,
            &output_plan,
            &seed,
            &character,
            ascension,
            &scope,
        )
        .and_then(|plan| {
            serde_json::to_string_pretty(&plan)
                .map_err(|error| format!("failed to encode mirror plan: {error}"))
        }),
        Command::RunMirrorPlan {
            output_dir,
            mirror_plan,
            expected_launch_plan,
            report_path,
            timeout_seconds,
        } => run_mirror_plan(
            &output_dir,
            &mirror_plan,
            &expected_launch_plan,
            &report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to encode mirror execution report: {error}"))
        }),
        Command::RunMirrorSuffix {
            output_dir,
            mirror_plan,
            checkpoint_manifest,
            expected_launch_plan,
            report_path,
            timeout_seconds,
        } => run_mirror_suffix(
            &output_dir,
            &mirror_plan,
            &checkpoint_manifest,
            &expected_launch_plan,
            &report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to encode mirror suffix report: {error}"))
        }),
        Command::CompileMirrorSuffixBatch {
            checkpoint_manifest,
            mirror_plans,
            output_plan,
        } => compile_mirror_suffix_batch(&checkpoint_manifest, &mirror_plans, &output_plan)
            .and_then(|plan| {
                serde_json::to_string_pretty(&plan)
                    .map_err(|error| format!("failed to encode mirror suffix batch plan: {error}"))
            }),
        Command::BundleElixirProbe {
            reports,
            output_path,
        } => bundle_elixir_hand_select_reports(&reports, &output_path).and_then(|bundle| {
            serde_json::to_string_pretty(&bundle)
                .map_err(|error| format!("failed to encode natural probe bundle: {error}"))
        }),
        Command::ScenarioRegistry => serde_json::to_string_pretty(&scenario_registry())
            .map_err(|error| format!("failed to encode scenario registry: {error}")),
        Command::ScenarioCaseCatalog { catalog_directory } => {
            scenario_case_catalog(&catalog_directory).and_then(|catalog| {
                serde_json::to_string_pretty(&catalog)
                    .map_err(|error| format!("failed to encode scenario case catalog: {error}"))
            })
        }
        Command::ScenarioProbeCaseCatalog { catalog_directory } => {
            scenario_probe_case_catalog(&catalog_directory).and_then(|catalog| {
                serde_json::to_string_pretty(&catalog).map_err(|error| {
                    format!("failed to encode scenario probe case catalog: {error}")
                })
            })
        }
        Command::ScenarioTrajectoryCaseCatalog { catalog_directory } => {
            scenario_trajectory_case_catalog(&catalog_directory).and_then(|catalog| {
                serde_json::to_string_pretty(&catalog).map_err(|error| {
                    format!("failed to encode scenario trajectory case catalog: {error}")
                })
            })
        }
        Command::CompileScenario { draft, output_plan } => {
            compile_scenario_plan(&draft, &output_plan).and_then(|plan| {
                serde_json::to_string_pretty(&plan)
                    .map_err(|error| format!("failed to encode scenario plan: {error}"))
            })
        }
        Command::CompileScenarioCase {
            catalog_directory,
            case_id,
            output_plan,
        } => compile_scenario_case(&catalog_directory, &case_id, &output_plan).and_then(|plan| {
            serde_json::to_string_pretty(&plan)
                .map_err(|error| format!("failed to encode scenario plan: {error}"))
        }),
        Command::CompileScenarioProbeCase {
            probe_catalog_directory,
            probe_case_id,
            scenario_catalog_directory,
            output_scenario_plan,
            output_probe_plan,
        } => compile_scenario_probe_case(
            &probe_catalog_directory,
            &probe_case_id,
            &scenario_catalog_directory,
            &output_scenario_plan,
            &output_probe_plan,
        )
        .and_then(|plan| {
            serde_json::to_string_pretty(&plan)
                .map_err(|error| format!("failed to encode scenario probe plan: {error}"))
        }),
        Command::CompileScenarioTrajectoryCase {
            trajectory_catalog_directory,
            trajectory_case_id,
            scenario_catalog_directory,
            output_scenario_plan,
            output_trajectory_plan,
        } => compile_scenario_trajectory_case(
            &trajectory_catalog_directory,
            &trajectory_case_id,
            &scenario_catalog_directory,
            &output_scenario_plan,
            &output_trajectory_plan,
        )
        .and_then(|plan| {
            serde_json::to_string_pretty(&plan)
                .map_err(|error| format!("failed to encode scenario trajectory plan: {error}"))
        }),
        Command::RunScenario {
            output_dir,
            scenario_plan,
            expected_launch_plan,
            report_path,
            timeout_seconds,
        } => run_constructed_combat_scenario(
            &output_dir,
            &scenario_plan,
            &expected_launch_plan,
            &report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to encode constructed scenario report: {error}"))
        }),
        Command::RunScenarioProbe {
            output_dir,
            probe_plan,
            expected_launch_plan,
            scenario_report_path,
            probe_report_path,
            timeout_seconds,
        } => run_constructed_combat_probe(
            &output_dir,
            &probe_plan,
            &expected_launch_plan,
            &scenario_report_path,
            &probe_report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to encode constructed probe report: {error}"))
        }),
        Command::RunScenarioTrajectory {
            output_dir,
            trajectory_plan,
            expected_launch_plan,
            scenario_report_path,
            trajectory_report_path,
            timeout_seconds,
        } => run_constructed_combat_trajectory(
            &output_dir,
            &trajectory_plan,
            &expected_launch_plan,
            &scenario_report_path,
            &trajectory_report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to encode constructed trajectory report: {error}"))
        }),
        Command::RunScenarioReuseProbe {
            output_dir,
            first_scenario_plan,
            second_scenario_plan,
            expected_launch_plan,
            first_report_path,
            reset_report_path,
            second_report_path,
            report_path,
            timeout_seconds,
        } => run_scenario_reuse_probe(
            &output_dir,
            &first_scenario_plan,
            &second_scenario_plan,
            &expected_launch_plan,
            &first_report_path,
            &reset_report_path,
            &second_report_path,
            &report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("failed to encode scenario reuse report: {error}"))
        }),
        Command::RunScenarioTransitionReuseProbe {
            output_dir,
            first_probe_plan,
            second_probe_plan,
            expected_launch_plan,
            first_root_report_path,
            first_probe_report_path,
            reset_report_path,
            second_root_report_path,
            second_probe_report_path,
            report_path,
            timeout_seconds,
        } => run_scenario_transition_reuse_probe(
            &output_dir,
            &first_probe_plan,
            &second_probe_plan,
            &expected_launch_plan,
            &first_root_report_path,
            &first_probe_report_path,
            &reset_report_path,
            &second_root_report_path,
            &second_probe_report_path,
            &report_path,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report).map_err(|error| {
                format!("failed to encode scenario transition reuse report: {error}")
            })
        }),
    };
    match result {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
