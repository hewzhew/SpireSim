use sts_oracle_runtime::ai::combat_search_v2::CombatSearchRolloutPluginId;
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::super::focus::review_focus;
use super::super::key_card_lifecycle::key_card_lifecycle;
use super::super::options::ReviewOptions;
use super::super::search_runner::run_profile_search;
use super::types::{FrozenPanelLaneConfigSummary, FrozenPanelLaneResult, FrozenPanelLaneSpec};

pub(super) fn run_frozen_panel_lane(
    options: &ReviewOptions,
    case: &CombatCase,
    spec: FrozenPanelLaneSpec,
    rollout_plugin: CombatSearchRolloutPluginId,
) -> FrozenPanelLaneResult {
    let profile = spec.profile(options, rollout_plugin);
    let summary = FrozenPanelLaneConfigSummary::from_profile(profile);
    let (review, _) = run_profile_search(case, profile, options.action_preview_limit);
    let focus = review_focus(std::slice::from_ref(&review));
    let key_card_lifecycle = key_card_lifecycle(&case.core.position, focus.as_ref());
    FrozenPanelLaneResult {
        lane: spec.lane,
        search_config_summary: summary,
        review,
        focus,
        key_card_lifecycle,
    }
}
