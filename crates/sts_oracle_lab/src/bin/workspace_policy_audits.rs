use std::path::{Path, PathBuf};

use clap::Args;
use serde::Serialize;
use serde_json::Value;
use sts_oracle_runtime::eval::combat_lab_v1::atomic_write_json;
use sts_oracle_runtime::runtime::branch::load_oracle_analysis_workspace_v1;

#[derive(Args, Debug)]
pub(super) struct RoutePolicyAuditArgs {
    /// Workspace containing the exact retained map node. Defaults to the cursor.
    #[arg(long)]
    pub workspace: PathBuf,
    #[arg(long)]
    pub node: Option<usize>,
}

#[derive(Args, Debug)]
pub(super) struct ShopPolicyAuditArgs {
    /// Workspace containing the exact retained shop node. Defaults to the cursor.
    #[arg(long)]
    pub workspace: PathBuf,
    #[arg(long)]
    pub node: Option<usize>,
}

#[derive(Args, Debug)]
pub(super) struct CardRewardPathArgs {
    #[arg(long)]
    pub workspace: PathBuf,
    #[arg(long)]
    pub node: Option<usize>,
    /// Write full typed evidence here and print only a compact receipt.
    #[arg(long)]
    pub output: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CardRewardPathAuditWriteReceiptV1<'a> {
    schema_name: &'static str,
    schema_version: u32,
    target_node_id: usize,
    boundary_count: usize,
    candidate_count: usize,
    output: &'a Path,
}

pub(super) fn route(workspace: &Path, node: Option<usize>) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node_id = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    encode(analysis.session.route_policy_audit(node_id)?)
}

pub(super) fn shop(workspace: &Path, node: Option<usize>) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node_id = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    encode(analysis.session.shop_policy_audit(node_id)?)
}

pub(super) fn card_reward_path(
    workspace: &Path,
    node: Option<usize>,
    output: Option<&Path>,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let target_node_id = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    let report = analysis.session.card_reward_path_audit(target_node_id)?;
    let Some(output) = output else {
        return encode(report);
    };
    if output.exists() {
        return Err(format!(
            "card reward path audit output already exists: '{}'",
            output.display()
        ));
    }

    atomic_write_json(output, &report)?;
    encode(CardRewardPathAuditWriteReceiptV1 {
        schema_name: "OracleAnalysisCardRewardPathAuditWriteReceipt",
        schema_version: 1,
        target_node_id,
        boundary_count: report.boundaries.len(),
        candidate_count: report
            .boundaries
            .iter()
            .map(|boundary| boundary.audit.candidates.len())
            .sum(),
        output,
    })
}

fn encode<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}
