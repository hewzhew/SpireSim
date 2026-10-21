use serde::{Deserialize, Serialize};

use super::trace_annotation::{CombatSearchTerminalLineSummary, RunControlTraceAnnotationV1};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedCombatLineEvidenceV1 {
    pub original: CombatSearchTerminalLineSummary,
    pub selected: CombatSearchTerminalLineSummary,
    pub hp_saved_by_selection: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_summary: Option<String>,
}

impl AcceptedCombatLineEvidenceV1 {
    pub fn new(
        original: CombatSearchTerminalLineSummary,
        selected: CombatSearchTerminalLineSummary,
        selection_summary: Option<String>,
    ) -> Self {
        Self {
            hp_saved_by_selection: original.hp_loss.saturating_sub(selected.hp_loss),
            original,
            selected,
            selection_summary,
        }
    }

    pub fn into_annotation(self) -> RunControlTraceAnnotationV1 {
        RunControlTraceAnnotationV1::AcceptedCombatLine { evidence: self }
    }
}

pub fn accepted_combat_line_evidence_v1(
    annotations: &[RunControlTraceAnnotationV1],
) -> Option<&AcceptedCombatLineEvidenceV1> {
    annotations.iter().find_map(|annotation| match annotation {
        RunControlTraceAnnotationV1::AcceptedCombatLine { evidence } => Some(evidence),
        _ => None,
    })
}
