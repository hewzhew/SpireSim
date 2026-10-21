use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use sts_java_parity_compare::{
    bundle_constructed_suite, capture_shared_runtime_suite,
    capture_shared_runtime_trajectory_suite, compile_constructed_suite_case,
    compile_shared_runtime_suite_case, compile_shared_runtime_trajectory_suite_case,
    constructed_suite_catalog, prepare_shared_runtime_suite_execution,
    prepare_shared_runtime_trajectory_suite_execution, shared_runtime_suite_catalog,
    shared_runtime_trajectory_suite_catalog, verify_shared_runtime_suite_new,
    verify_shared_runtime_trajectory_suite_new,
};

#[derive(Debug, Parser)]
#[command(about = "Compile and bind topology-explicit constructed Java parity suites")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate and list exact-ID tracked suite manifests.
    Catalog {
        #[arg(long)]
        suite_catalog_directory: PathBuf,
    },
    /// Validate and list exact-ID shared-runtime suite manifests.
    CatalogShared {
        #[arg(long)]
        suite_catalog_directory: PathBuf,
    },
    /// Validate and list exact-ID shared-runtime trajectory suite manifests.
    CatalogSharedTrajectory {
        #[arg(long)]
        suite_catalog_directory: PathBuf,
    },
    /// Compile one exact-ID suite into an immutable case-bound plan.
    Compile {
        #[arg(long)]
        suite_catalog_directory: PathBuf,
        #[arg(long)]
        suite_id: String,
        #[arg(long)]
        probe_case_catalog_directory: PathBuf,
        #[arg(long)]
        scenario_case_catalog_directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Compile one exact-ID shared-runtime suite into an immutable case-bound plan.
    CompileShared {
        #[arg(long)]
        suite_catalog_directory: PathBuf,
        #[arg(long)]
        suite_id: String,
        #[arg(long)]
        probe_case_catalog_directory: PathBuf,
        #[arg(long)]
        scenario_case_catalog_directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Compile one shared-runtime trajectory suite into an immutable plan.
    CompileSharedTrajectory {
        #[arg(long)]
        suite_catalog_directory: PathBuf,
        #[arg(long)]
        suite_id: String,
        #[arg(long)]
        trajectory_case_catalog_directory: PathBuf,
        #[arg(long)]
        scenario_case_catalog_directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Compile every fresh scenario/probe UUID plan for one shared runtime before launch.
    PrepareShared {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        member_plan_directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Compile every fresh scenario/trajectory UUID plan before one shared launch.
    PrepareSharedTrajectory {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        member_plan_directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Capture every prepared member and typed inter-member reset in one live runtime.
    CaptureShared {
        #[arg(long)]
        execution_plan: PathBuf,
        #[arg(long)]
        capture_directory: PathBuf,
        #[arg(long)]
        live_output_directory: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Capture every trajectory and typed reset in one live runtime.
    CaptureSharedTrajectory {
        #[arg(long)]
        execution_plan: PathBuf,
        #[arg(long)]
        capture_directory: PathBuf,
        #[arg(long)]
        live_output_directory: PathBuf,
        #[arg(long)]
        expected_launch_plan: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Recompare every immutable capture and bind resets plus managed lifecycle.
    VerifyShared {
        #[arg(long)]
        execution_plan: PathBuf,
        #[arg(long)]
        capture_report: PathBuf,
        #[arg(long)]
        lifecycle_receipt: PathBuf,
        #[arg(long)]
        comparison_directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Recompare every trajectory and bind resets plus one managed lifecycle.
    VerifySharedTrajectory {
        #[arg(long)]
        execution_plan: PathBuf,
        #[arg(long)]
        capture_report: PathBuf,
        #[arg(long)]
        lifecycle_receipt: PathBuf,
        #[arg(long)]
        comparison_directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Bind ordered member workflows to their immutable suite plan.
    Bundle {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long, required = true)]
        member_workflow: Vec<PathBuf>,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Catalog {
            suite_catalog_directory,
        } => constructed_suite_catalog(&suite_catalog_directory).and_then(|catalog| {
            serde_json::to_string_pretty(&catalog)
                .map(|json| println!("{json}"))
                .map_err(|error| format!("failed to encode suite catalog: {error}"))
        }),
        Command::CatalogShared {
            suite_catalog_directory,
        } => shared_runtime_suite_catalog(&suite_catalog_directory).and_then(|catalog| {
            serde_json::to_string_pretty(&catalog)
                .map(|json| println!("{json}"))
                .map_err(|error| format!("failed to encode shared-runtime suite catalog: {error}"))
        }),
        Command::CatalogSharedTrajectory {
            suite_catalog_directory,
        } => {
            shared_runtime_trajectory_suite_catalog(&suite_catalog_directory).and_then(|catalog| {
                serde_json::to_string_pretty(&catalog)
                    .map(|json| println!("{json}"))
                    .map_err(|error| {
                        format!("failed to encode shared-runtime trajectory suite catalog: {error}")
                    })
            })
        }
        Command::Compile {
            suite_catalog_directory,
            suite_id,
            probe_case_catalog_directory,
            scenario_case_catalog_directory,
            output,
        } => compile_constructed_suite_case(
            &suite_catalog_directory,
            &suite_id,
            &probe_case_catalog_directory,
            &scenario_case_catalog_directory,
            &output,
        )
        .and_then(|plan| {
            serde_json::to_string_pretty(&plan)
                .map(|json| println!("{json}"))
                .map_err(|error| format!("failed to encode suite plan: {error}"))
        }),
        Command::CompileShared {
            suite_catalog_directory,
            suite_id,
            probe_case_catalog_directory,
            scenario_case_catalog_directory,
            output,
        } => compile_shared_runtime_suite_case(
            &suite_catalog_directory,
            &suite_id,
            &probe_case_catalog_directory,
            &scenario_case_catalog_directory,
            &output,
        )
        .and_then(|plan| {
            serde_json::to_string_pretty(&plan)
                .map(|json| println!("{json}"))
                .map_err(|error| format!("failed to encode shared-runtime suite plan: {error}"))
        }),
        Command::CompileSharedTrajectory {
            suite_catalog_directory,
            suite_id,
            trajectory_case_catalog_directory,
            scenario_case_catalog_directory,
            output,
        } => compile_shared_runtime_trajectory_suite_case(
            &suite_catalog_directory,
            &suite_id,
            &trajectory_case_catalog_directory,
            &scenario_case_catalog_directory,
            &output,
        )
        .and_then(|plan| {
            serde_json::to_string_pretty(&plan)
                .map(|json| println!("{json}"))
                .map_err(|error| {
                    format!("failed to encode shared-runtime trajectory suite plan: {error}")
                })
        }),
        Command::PrepareShared {
            plan,
            member_plan_directory,
            output,
        } => prepare_shared_runtime_suite_execution(&plan, &member_plan_directory, &output)
            .and_then(|execution| {
                serde_json::to_string_pretty(&execution)
                    .map(|json| println!("{json}"))
                    .map_err(|error| {
                        format!("failed to encode shared-runtime execution plan: {error}")
                    })
            }),
        Command::PrepareSharedTrajectory {
            plan,
            member_plan_directory,
            output,
        } => prepare_shared_runtime_trajectory_suite_execution(
            &plan,
            &member_plan_directory,
            &output,
        )
        .and_then(|execution| {
            serde_json::to_string_pretty(&execution)
                .map(|json| println!("{json}"))
                .map_err(|error| {
                    format!("failed to encode shared-runtime trajectory execution plan: {error}")
                })
        }),
        Command::CaptureShared {
            execution_plan,
            capture_directory,
            live_output_directory,
            expected_launch_plan,
            output,
            timeout_seconds,
        } => capture_shared_runtime_suite(
            &execution_plan,
            &capture_directory,
            &live_output_directory,
            &expected_launch_plan,
            &output,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|capture| {
            serde_json::to_string_pretty(&capture)
                .map(|json| println!("{json}"))
                .map_err(|error| format!("failed to encode shared-runtime capture: {error}"))
        }),
        Command::CaptureSharedTrajectory {
            execution_plan,
            capture_directory,
            live_output_directory,
            expected_launch_plan,
            output,
            timeout_seconds,
        } => capture_shared_runtime_trajectory_suite(
            &execution_plan,
            &capture_directory,
            &live_output_directory,
            &expected_launch_plan,
            &output,
            Duration::from_secs(timeout_seconds),
        )
        .and_then(|capture| {
            serde_json::to_string_pretty(&capture)
                .map(|json| println!("{json}"))
                .map_err(|error| {
                    format!("failed to encode shared-runtime trajectory capture: {error}")
                })
        }),
        Command::VerifyShared {
            execution_plan,
            capture_report,
            lifecycle_receipt,
            comparison_directory,
            output,
        } => verify_shared_runtime_suite_new(
            &execution_plan,
            &capture_report,
            &lifecycle_receipt,
            &comparison_directory,
            &output,
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map(|json| println!("{json}"))
                .map_err(|error| format!("failed to encode shared-runtime report: {error}"))
        }),
        Command::VerifySharedTrajectory {
            execution_plan,
            capture_report,
            lifecycle_receipt,
            comparison_directory,
            output,
        } => verify_shared_runtime_trajectory_suite_new(
            &execution_plan,
            &capture_report,
            &lifecycle_receipt,
            &comparison_directory,
            &output,
        )
        .and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map(|json| println!("{json}"))
                .map_err(|error| {
                    format!("failed to encode shared-runtime trajectory report: {error}")
                })
        }),
        Command::Bundle {
            plan,
            member_workflow,
            output,
        } => bundle_constructed_suite(&plan, &member_workflow, &output).and_then(|report| {
            serde_json::to_string_pretty(&report)
                .map(|json| println!("{json}"))
                .map_err(|error| format!("failed to encode suite report: {error}"))
        }),
    };
    if let Err(error) = result {
        eprintln!("java parity suite failed: {error}");
        std::process::exit(1);
    }
}
