use crate::state::core::EngineState;
use crate::state::selection::{
    DomainEventSource, SelectionConstraint, SelectionReason, SelectionRequest, SelectionScope,
    SelectionTargetRef,
};
use serde::{Deserialize, Serialize};

use super::run_choice_targets::run_pending_choice_allows_card_for_run;

/// Run-level selections over the master deck. These are produced by macro
/// systems such as relics, events, shops, and campfires.
///
/// This shares the UI-facing `SelectionRequest` / `SelectionResolution` protocol
/// with combat `PendingChoice`, but its lifecycle and effects are resolved by
/// the run-level pending choice handler.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub enum RunPendingChoiceReason {
    Purge,
    PurgeNonBottled,
    Upgrade,
    Transform,
    TransformNonBottled,
    TransformUpgraded,
    Duplicate,
    BottleFlame,
    BottleLightning,
    BottleTornado,
}

impl RunPendingChoiceReason {
    pub const fn kind_v1(self) -> &'static str {
        match self {
            Self::Purge => "purge",
            Self::PurgeNonBottled => "purge_non_bottled",
            Self::Upgrade => "upgrade",
            Self::Transform => "transform",
            Self::TransformNonBottled => "transform_non_bottled",
            Self::TransformUpgraded => "transform_upgraded",
            Self::Duplicate => "duplicate",
            Self::BottleFlame => "bottle_flame",
            Self::BottleLightning => "bottle_lightning",
            Self::BottleTornado => "bottle_tornado",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct RunPendingChoiceState {
    /// Original selection order, before the final choice applies the mutation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected: Vec<crate::runtime::combat::CardLineageId>,
    pub min_choices: usize,
    pub max_choices: usize,
    pub reason: RunPendingChoiceReason,
    pub source: DomainEventSource,
    pub return_state: Box<EngineState>,
}

impl From<RunPendingChoiceReason> for SelectionReason {
    fn from(value: RunPendingChoiceReason) -> Self {
        match value {
            RunPendingChoiceReason::Purge => SelectionReason::Purge,
            RunPendingChoiceReason::PurgeNonBottled => SelectionReason::Purge,
            RunPendingChoiceReason::Upgrade => SelectionReason::Upgrade,
            RunPendingChoiceReason::Transform => SelectionReason::Transform,
            RunPendingChoiceReason::TransformNonBottled => SelectionReason::Transform,
            RunPendingChoiceReason::TransformUpgraded => SelectionReason::TransformUpgraded,
            RunPendingChoiceReason::Duplicate => SelectionReason::Duplicate,
            RunPendingChoiceReason::BottleFlame => SelectionReason::BottleFlame,
            RunPendingChoiceReason::BottleLightning => SelectionReason::BottleLightning,
            RunPendingChoiceReason::BottleTornado => SelectionReason::BottleTornado,
        }
    }
}

impl RunPendingChoiceState {
    pub fn selection_request(&self, run_state: &crate::state::run::RunState) -> SelectionRequest {
        let targets: Vec<_> = run_state
            .master_deck
            .iter()
            .filter(|card| run_pending_choice_allows_card_for_run(&self.reason, card, run_state))
            .filter(|card| !self.selected.contains(&card.lineage))
            .map(|card| SelectionTargetRef::CardLineage(card.lineage))
            .collect();

        SelectionRequest {
            scope: SelectionScope::Deck,
            reason: self.reason.into(),
            constraint: SelectionConstraint::from_bounds(
                self.min_choices.saturating_sub(self.selected.len()),
                self.max_choices.saturating_sub(self.selected.len()),
                targets.len(),
            ),
            can_cancel: self.min_choices == 0,
            targets,
        }
    }
}
