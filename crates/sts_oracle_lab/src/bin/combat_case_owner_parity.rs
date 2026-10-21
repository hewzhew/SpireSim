use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::combat_lab_v1::atomic_write_json;
use sts_oracle_runtime::runtime::branch::combat_case_owner_parity::{
    run_combat_case_owner_parity_v1, CombatCaseOwnerParityReportV1, CombatCaseOwnerParityRequestV1,
};
use sts_oracle_runtime::runtime::branch::OracleAnalysisAdvanceRequestV1;

#[derive(Debug, Args)]
pub(super) struct CombatCaseOwnerParityArgs {
    #[arg(long)]
    case: PathBuf,
    #[arg(long, default_value_t = 32)]
    max_quanta: usize,
    #[arg(long, default_value_t = 50_000)]
    quantum_generation_work: usize,
    #[arg(long, default_value_t = 1_000)]
    quantum_ms: u64,
    #[arg(long)]
    wall_ms: Option<u64>,
    #[arg(long)]
    improve_incumbent: bool,
    /// Persist the full advance report and resumable analysis checkpoint to
    /// this single file. Without this flag, the command writes no artifacts.
    #[arg(long, value_name = "PATH")]
    keep_debug: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatCaseOwnerParityCommandReportV1 {
    #[serde(flatten)]
    report: CombatCaseOwnerParityReportV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    debug_checkpoint: Option<PathBuf>,
}

pub(super) fn run(
    args: CombatCaseOwnerParityArgs,
) -> Result<CombatCaseOwnerParityCommandReportV1, String> {
    let case = load_combat_case(&args.case)?;
    let result = run_combat_case_owner_parity_v1(
        &case,
        CombatCaseOwnerParityRequestV1 {
            advance: OracleAnalysisAdvanceRequestV1 {
                max_quanta: args.max_quanta,
                quantum_generation_work: args.quantum_generation_work,
                quantum_ms: Some(args.quantum_ms),
                wall_ms: args.wall_ms,
                improve_incumbent: args.improve_incumbent,
            },
            keep_debug_checkpoint: args.keep_debug.is_some(),
        },
    )?;
    if let Some(path) = args.keep_debug.as_ref() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create owner-parity debug directory '{}': {error}",
                    parent.display()
                )
            })?;
        }
        let debug = result
            .debug
            .as_ref()
            .ok_or_else(|| "owner-parity debug checkpoint was not retained".to_string())?;
        atomic_write_json(path, debug)?;
    }
    Ok(CombatCaseOwnerParityCommandReportV1 {
        report: result.report,
        debug_checkpoint: args.keep_debug,
    })
}
