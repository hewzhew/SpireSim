use std::path::Path;

use serde_json::{json, Value};
use sts_oracle_runtime::runtime::branch::{
    load_oracle_analysis_workspace_v1, OracleAnalysisCombatScratchActionSelectorV1,
    OracleAnalysisCombatScratchDecisionViewV1, OracleAnalysisCombatScratchSearchRequestV1,
    OracleAnalysisWorkspaceV1,
};

use super::combat_scratch_cli::CombatScratchCommand;
use super::workspace_commands::{encode, mutate};

pub(super) fn execute(workspace: &Path, command: CombatScratchCommand) -> Result<Value, String> {
    match command {
        CombatScratchCommand::Start {
            node,
            max_engine_steps_per_transition,
            page,
        } => start(
            workspace,
            node,
            max_engine_steps_per_transition,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Status { page } => status(
            workspace,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Observe { page } => observe(
            workspace,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Play { action_ref, page } => play(
            workspace,
            &action_ref,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Atomic {
            from,
            action,
            full,
            page,
        } => atomic(
            workspace,
            from,
            action,
            full,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Card {
            from,
            hand,
            lineage,
            target,
            full,
            page,
        } => card(
            workspace,
            from,
            hand,
            lineage,
            target,
            full,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Potion {
            from,
            slot,
            uuid,
            target,
            full,
            page,
        } => potion(
            workspace,
            from,
            slot,
            uuid,
            target,
            full,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::End { from, full, page } => end(
            workspace,
            from,
            full,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Selection {
            from,
            family,
            input,
            full,
            page,
        } => selection(
            workspace,
            from,
            family,
            input,
            full,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Back { full, page } => back(
            workspace,
            full,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Focus {
            scratch_node,
            full,
            page,
        } => focus(
            workspace,
            scratch_node,
            full,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Search {
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
            page,
        } => search(
            workspace,
            max_quanta,
            quantum_generation_work,
            quantum_ms,
            wall_ms,
            page.selection_offset,
            usize::from(page.selection_limit),
        ),
        CombatScratchCommand::Tree => tree(workspace),
        CombatScratchCommand::Commit => commit(workspace),
        CombatScratchCommand::Clear => clear(workspace),
    }
}

pub(super) fn start(
    workspace: &Path,
    node: Option<usize>,
    max_engine_steps_per_transition: usize,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        let view = analysis.session.start_combat_scratch(
            node,
            max_engine_steps_per_transition,
            selection_offset,
            selection_limit,
        )?;
        Ok(OracleAnalysisCombatScratchDecisionViewV1::from(view))
    })
}

pub(super) fn status(
    workspace: &Path,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    encode(
        analysis
            .session
            .combat_scratch_view(selection_offset, selection_limit)?,
    )
}

pub(super) fn observe(
    workspace: &Path,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    encode(
        analysis
            .session
            .combat_scratch_decision_view(selection_offset, selection_limit)?,
    )
}

pub(super) fn play(
    workspace: &Path,
    action_ref: &str,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        analysis
            .session
            .play_combat_scratch_action(action_ref, selection_offset, selection_limit)
    })
}

pub(super) fn atomic(
    workspace: &Path,
    from: u64,
    action: usize,
    full: bool,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    apply_selector(
        workspace,
        OracleAnalysisCombatScratchActionSelectorV1::Atomic {
            scratch_node_id: from,
            action_index: action,
        },
        full,
        selection_offset,
        selection_limit,
    )
}

pub(super) fn card(
    workspace: &Path,
    from: u64,
    hand: Option<usize>,
    lineage: Option<sts_oracle_runtime::runtime::combat::CardLineageId>,
    target: Option<usize>,
    full: bool,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    let selector = match (hand, lineage) {
        (Some(hand_index), None) => OracleAnalysisCombatScratchActionSelectorV1::HandCard {
            scratch_node_id: from,
            hand_index,
            target_index: target,
        },
        (None, Some(card_lineage)) => OracleAnalysisCombatScratchActionSelectorV1::Card {
            scratch_node_id: from,
            card_lineage,
            target,
        },
        _ => return Err("choose exactly one of --hand or --lineage".to_string()),
    };
    apply_selector(workspace, selector, full, selection_offset, selection_limit)
}

pub(super) fn potion(
    workspace: &Path,
    from: u64,
    slot: Option<usize>,
    uuid: Option<u32>,
    target: Option<usize>,
    full: bool,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    let selector = match (slot, uuid) {
        (Some(potion_slot), None) => OracleAnalysisCombatScratchActionSelectorV1::PotionSlot {
            scratch_node_id: from,
            potion_slot,
            target_index: target,
        },
        (None, Some(potion_uuid)) => OracleAnalysisCombatScratchActionSelectorV1::Potion {
            scratch_node_id: from,
            potion_uuid,
            target,
        },
        _ => return Err("choose exactly one of --slot or --uuid".to_string()),
    };
    apply_selector(workspace, selector, full, selection_offset, selection_limit)
}

pub(super) fn end(
    workspace: &Path,
    from: Option<u64>,
    full: bool,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let source = from.unwrap_or(analysis.session.combat_scratch_cursor_node_id()?);
    apply_selector(
        workspace,
        OracleAnalysisCombatScratchActionSelectorV1::EndTurn {
            scratch_node_id: source,
        },
        full,
        selection_offset,
        selection_limit,
    )
}

fn apply_selector(
    workspace: &Path,
    selector: OracleAnalysisCombatScratchActionSelectorV1,
    full: bool,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        if full {
            let view = analysis.session.play_combat_scratch_selector(
                selector,
                selection_offset,
                selection_limit,
            )?;
            encode(OracleAnalysisCombatScratchDecisionViewV1::from(view))
        } else {
            encode(analysis.session.play_combat_scratch_selector_delta(
                selector,
                selection_offset,
                selection_limit,
            )?)
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn selection(
    workspace: &Path,
    from: u64,
    family: usize,
    input: usize,
    full: bool,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    apply_selector(
        workspace,
        OracleAnalysisCombatScratchActionSelectorV1::Selection {
            scratch_node_id: from,
            family_index: family,
            input_index: input,
        },
        full,
        selection_offset,
        selection_limit,
    )
}

pub(super) fn back(
    workspace: &Path,
    full: bool,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        if full {
            let view = analysis
                .session
                .back_combat_scratch(selection_offset, selection_limit)?;
            encode(OracleAnalysisCombatScratchDecisionViewV1::from(view))
        } else {
            encode(analysis.session.back_combat_scratch_receipt()?)
        }
    })
}

pub(super) fn focus(
    workspace: &Path,
    scratch_node: u64,
    full: bool,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        if full {
            let view = analysis.session.focus_combat_scratch_node(
                scratch_node,
                selection_offset,
                selection_limit,
            )?;
            encode(OracleAnalysisCombatScratchDecisionViewV1::from(view))
        } else {
            encode(
                analysis
                    .session
                    .focus_combat_scratch_node_receipt(scratch_node)?,
            )
        }
    })
}

pub(super) fn tree(workspace: &Path) -> Result<Value, String> {
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    encode(analysis.session.combat_scratch_tree()?)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn search(
    workspace: &Path,
    max_quanta: usize,
    quantum_generation_work: usize,
    quantum_ms: u64,
    wall_ms: u64,
    selection_offset: usize,
    selection_limit: usize,
) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        let (report, view) = analysis.session.search_combat_scratch(
            OracleAnalysisCombatScratchSearchRequestV1 {
                max_quanta,
                quantum_generation_work,
                quantum_ms,
                wall_ms,
            },
            selection_offset,
            selection_limit,
        )?;
        Ok(json!({
            "report": report,
            "view": OracleAnalysisCombatScratchDecisionViewV1::from(view),
        }))
    })
}

pub(super) fn commit(workspace: &Path) -> Result<Value, String> {
    mutate(workspace, OracleAnalysisWorkspaceV1::commit_combat_scratch)
}

pub(super) fn clear(workspace: &Path) -> Result<Value, String> {
    mutate(workspace, |analysis| {
        Ok(json!({
            "schema_name": "OracleAnalysisCombatScratchClearReceiptV1",
            "schema_version": 1,
            "cleared": analysis.session.clear_combat_scratch(),
            "run_cursor_node_id": analysis.session.cursor_node_id(),
        }))
    })
}
