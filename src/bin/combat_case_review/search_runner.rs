use sts_oracle_runtime::ai::combat_search_v2::{
    run_combat_search_v2, CombatSearchAcceptancePluginId, CombatSearchArtifactPluginId,
    CombatSearchAttemptPolicy, CombatSearchBudgetSpec, CombatSearchEngineProfile,
    CombatSearchPluginStack, CombatSearchProfile, CombatSearchRolloutPluginId,
    CombatSearchV2Config, CombatSearchV2PotionPolicy, CombatSearchV2Report,
};
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::options::ReviewOptions;
use super::search_review::search_review;
use super::search_types::SearchReview;

pub(crate) fn review_search_profile(
    label: &'static str,
    nodes: usize,
    wall_ms: u64,
    options: &ReviewOptions,
) -> CombatSearchProfile {
    CombatSearchProfile {
        label,
        engine: CombatSearchEngineProfile {
            budget: CombatSearchBudgetSpec {
                max_nodes: nodes,
                wall_ms,
            },
            plugins: CombatSearchPluginStack {
                expansion: options.expansion_plugin(),
                turn_plan: options.turn_plan_plugin(),
                rollout: review_rollout_plugin(options),
                child_rollout: options.child_rollout_plugin(),
                ..CombatSearchPluginStack::default()
            },
        },
        policy: CombatSearchAttemptPolicy {
            acceptance: CombatSearchAcceptancePluginId::AcceptedLineOnly,
            artifacts: CombatSearchArtifactPluginId::None,
        },
    }
}

pub(crate) fn review_no_potion_profile(
    label: &'static str,
    nodes: usize,
    wall_ms: u64,
    options: &ReviewOptions,
) -> CombatSearchProfile {
    review_search_profile(label, nodes, wall_ms, options)
        .with_potion_policy(CombatSearchV2PotionPolicy::Never)
        .with_max_potions_used(0)
}

pub(crate) fn review_all_potions_profile(
    label: &'static str,
    nodes: usize,
    wall_ms: u64,
    options: &ReviewOptions,
) -> CombatSearchProfile {
    review_search_profile(label, nodes, wall_ms, options)
        .with_potion_policy(CombatSearchV2PotionPolicy::All)
        .with_max_potions_used(options.diagnostic_potion_max)
}

pub(crate) fn review_rollout_plugin(options: &ReviewOptions) -> CombatSearchRolloutPluginId {
    options.rollout_plugin()
}

pub(crate) fn run_profile_search(
    case: &CombatCase,
    profile: CombatSearchProfile,
    action_preview_limit: usize,
) -> (SearchReview, CombatSearchV2Report) {
    run_config_search(
        profile.label,
        case,
        profile.to_config(),
        action_preview_limit,
    )
}

pub(crate) fn run_config_search(
    label: &'static str,
    case: &CombatCase,
    config: CombatSearchV2Config,
    action_preview_limit: usize,
) -> (SearchReview, CombatSearchV2Report) {
    let nodes = config.max_nodes;
    let wall_ms = config
        .wall_time
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default();
    let report = run_combat_search_v2(
        &case.core.position.engine,
        &case.core.position.combat,
        config,
    );
    let review = search_review(label, nodes, wall_ms, &report, action_preview_limit);
    (review, report)
}
