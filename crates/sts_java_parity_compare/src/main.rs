use clap::Parser;
use std::path::PathBuf;
use sts_java_parity_compare::{
    compare_base_defend_capture, compare_live_action, recompare_constructed_probe_new,
    recompare_constructed_trajectory_steps_new, verify_constructed_trajectory_new,
    verify_natural_combat_action_new, write_constructed_comparison_workflow_new,
    write_json_report_new,
};

#[derive(Debug, Parser)]
#[command(about = "Compare a scoped original-game card transition with the Rust stepper")]
struct Cli {
    /// Completed JSONL artifact produced by sts_java_parity_capture.
    #[arg(long, requires = "receipt", conflicts_with_all = ["live_action_receipt", "recompare_constructed_probe"])]
    artifact: Option<PathBuf>,
    /// Completion receipt paired with the artifact.
    #[arg(long, requires = "artifact", conflicts_with_all = ["live_action_receipt", "recompare_constructed_probe"])]
    receipt: Option<PathBuf>,
    /// Completed action receipt produced by sts_java_parity_live.
    #[arg(long, conflicts_with_all = ["artifact", "receipt", "recompare_constructed_probe"])]
    live_action_receipt: Option<PathBuf>,
    /// Passing constructed probe whose bound action receipt should be compared again offline.
    #[arg(long, conflicts_with_all = ["artifact", "receipt", "live_action_receipt", "constructed_probe_report"])]
    recompare_constructed_probe: Option<PathBuf>,
    /// Passing constructed multi-step capture to compare and bind as one trajectory.
    #[arg(long, conflicts_with_all = ["artifact", "receipt", "live_action_receipt", "recompare_constructed_probe", "constructed_probe_report"])]
    verify_constructed_trajectory: Option<PathBuf>,
    /// Passing constructed trajectory whose selected captured transitions should be compared.
    #[arg(long, conflicts_with_all = ["artifact", "receipt", "live_action_receipt", "recompare_constructed_probe", "verify_constructed_trajectory", "verify_natural_combat", "constructed_probe_report"])]
    recompare_constructed_trajectory: Option<PathBuf>,
    /// Passing natural combat producer report to compare and promote at workflow scope.
    #[arg(long, conflicts_with_all = ["artifact", "receipt", "live_action_receipt", "recompare_constructed_probe", "verify_constructed_trajectory", "constructed_probe_report"])]
    verify_natural_combat: Option<PathBuf>,
    /// Managed lifecycle receipt paired with a constructed trajectory or natural capture.
    #[arg(long)]
    lifecycle_receipt: Option<PathBuf>,
    /// Fresh directory for per-step trajectory comparison reports.
    #[arg(long)]
    comparison_directory: Option<PathBuf>,
    /// Zero-based captured trajectory step indices to compare, in ascending order.
    #[arg(
        long = "trajectory-step-index",
        value_delimiter = ',',
        requires = "recompare_constructed_trajectory"
    )]
    trajectory_step_indices: Vec<usize>,
    /// Fresh scoped comparison report paired with --verify-natural-combat.
    #[arg(long, requires = "verify_natural_combat")]
    comparison_report: Option<PathBuf>,
    /// Fresh path for the typed comparison report.
    #[arg(long)]
    output: PathBuf,
    /// Passing constructed probe report to bind with this live comparison.
    #[arg(long, conflicts_with = "recompare_constructed_probe")]
    constructed_probe_report: Option<PathBuf>,
    /// Fresh workflow report binding probe capture and simulator comparison.
    #[arg(long)]
    workflow_output: Option<PathBuf>,
    /// First frame of the adjacent transition to compare.
    #[arg(long, default_value_t = 1)]
    from_frame: u64,
}

fn main() {
    let cli = Cli::parse();
    let result = (|| {
        if let Some(trajectory_report) = &cli.recompare_constructed_trajectory {
            if cli.lifecycle_receipt.is_some()
                || cli.comparison_report.is_some()
                || cli.workflow_output.is_some()
                || cli.constructed_probe_report.is_some()
            {
                return Err(
                    "selected trajectory recompare does not consume lifecycle or workflow options"
                        .to_owned(),
                );
            }
            let comparisons = cli.comparison_directory.as_ref().ok_or_else(|| {
                "--recompare-constructed-trajectory requires --comparison-directory".to_owned()
            })?;
            let attempt = recompare_constructed_trajectory_steps_new(
                trajectory_report,
                comparisons,
                &cli.output,
                &cli.trajectory_step_indices,
            )?;
            println!(
                "java_parity_trajectory_selected status=matched selected={:?} captured={} matched={} session={} output={}",
                attempt.requested_step_indices,
                attempt.captured_steps,
                attempt.completed_steps,
                attempt.session_id.as_deref().unwrap_or("unknown"),
                cli.output.display()
            );
        } else if let Some(producer_report) = &cli.verify_natural_combat {
            let lifecycle = cli
                .lifecycle_receipt
                .as_ref()
                .ok_or_else(|| "--verify-natural-combat requires --lifecycle-receipt".to_owned())?;
            let comparison = cli
                .comparison_report
                .as_ref()
                .ok_or_else(|| "--verify-natural-combat requires --comparison-report".to_owned())?;
            if cli.comparison_directory.is_some() || cli.workflow_output.is_some() {
                return Err(
                    "natural verification uses --comparison-report and writes its workflow to --output"
                        .to_owned(),
                );
            }
            let workflow = verify_natural_combat_action_new(
                producer_report,
                lifecycle,
                comparison,
                &cli.output,
            )?;
            println!(
                "java_parity_natural status=matched kind={} transition={}->{} session={} output={}",
                workflow.comparison_kind,
                workflow.action.from_frame_id,
                workflow.action.to_frame_id,
                workflow.session_id,
                cli.output.display()
            );
        } else if let Some(trajectory_report) = &cli.verify_constructed_trajectory {
            let lifecycle = cli.lifecycle_receipt.as_ref().ok_or_else(|| {
                "--verify-constructed-trajectory requires --lifecycle-receipt".to_owned()
            })?;
            let comparisons = cli.comparison_directory.as_ref().ok_or_else(|| {
                "--verify-constructed-trajectory requires --comparison-directory".to_owned()
            })?;
            if cli.workflow_output.is_some() {
                return Err(
                    "trajectory verification writes its workflow to --output; omit --workflow-output"
                        .to_owned(),
                );
            }
            let workflow = verify_constructed_trajectory_new(
                trajectory_report,
                lifecycle,
                comparisons,
                &cli.output,
            )?;
            println!(
                "java_parity_trajectory status=matched steps={} frames={}->{} session={} output={}",
                workflow.steps.len(),
                workflow.root_frame_id,
                workflow.final_frame_id,
                workflow.session_id,
                cli.output.display()
            );
        } else if let Some(probe_report) = &cli.recompare_constructed_probe {
            if cli.lifecycle_receipt.is_some() || cli.comparison_report.is_some() {
                return Err("lifecycle/comparison report options require trajectory or natural verification".to_owned());
            }
            let workflow = cli.workflow_output.as_ref().ok_or_else(|| {
                "--recompare-constructed-probe requires --workflow-output".to_owned()
            })?;
            let (report, workflow_report) =
                recompare_constructed_probe_new(probe_report, &cli.output, workflow)?;
            let (from_frame, to_frame) = report.frame_ids();
            println!(
                "java_parity_recompare status={} kind={} transition={}->{} session={} output={} workflow={}",
                report.status(),
                report.transition_kind(),
                from_frame,
                to_frame,
                workflow_report.session_id,
                cli.output.display(),
                workflow.display()
            );
        } else if let Some(action_receipt) = &cli.live_action_receipt {
            if cli.lifecycle_receipt.is_some() || cli.comparison_report.is_some() {
                return Err("lifecycle/comparison report options require trajectory or natural verification".to_owned());
            }
            let workflow_inputs = match (&cli.constructed_probe_report, &cli.workflow_output) {
                (Some(probe), Some(workflow)) => Some((probe, workflow)),
                (None, None) => None,
                _ => return Err(
                    "--constructed-probe-report and --workflow-output must be provided together"
                        .to_owned(),
                ),
            };
            let report = compare_live_action(action_receipt)?;
            write_json_report_new(&cli.output, &report)?;
            if let Some((probe, workflow)) = workflow_inputs {
                write_constructed_comparison_workflow_new(probe, &cli.output, workflow, &report)?;
            }
            let (from_frame, to_frame) = report.frame_ids();
            println!(
                "java_parity_compare status={} kind={} transition={}->{} output={}",
                report.status(),
                report.transition_kind(),
                from_frame,
                to_frame,
                cli.output.display()
            );
        } else {
            if cli.constructed_probe_report.is_some()
                || cli.workflow_output.is_some()
                || cli.lifecycle_receipt.is_some()
                || cli.comparison_report.is_some()
            {
                return Err(
                    "workflow/lifecycle options require a matching verification mode".to_owned(),
                );
            }
            let artifact = cli.artifact.as_ref().ok_or_else(|| {
                "provide --artifact/--receipt or --live-action-receipt".to_owned()
            })?;
            let receipt = cli.receipt.as_ref().ok_or_else(|| {
                "provide --artifact/--receipt or --live-action-receipt".to_owned()
            })?;
            let report = compare_base_defend_capture(artifact, receipt, cli.from_frame)?;
            write_json_report_new(&cli.output, &report)?;
            println!(
                "java_parity_compare status={} transition={}->{} card={} output={}",
                report.status,
                report.transition.from_frame_id,
                report.transition.to_frame_id,
                report.transition.java_card_id,
                cli.output.display()
            );
        }
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        eprintln!("java parity comparison failed: {error}");
        std::process::exit(1);
    }
}
