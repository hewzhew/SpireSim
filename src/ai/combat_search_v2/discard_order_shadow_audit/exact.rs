use std::collections::{BTreeMap, BTreeSet};

use crate::sim::combat::CombatStepper;

use super::super::plugins::CombatSearchPluginStack;
use super::super::types::CombatSearchV2Config;
use super::DiscardOrderShadowAuditKey;

mod group;
mod one_step;
mod types;
use one_step::audit_group_one_step;
pub(super) use types::{
    DiscardOrderShadowAuditExactGroupResult, DiscardOrderShadowAuditExactSummary,
    DiscardOrderShadowAuditGroup, EXACT_SHADOW_ACTIONS_PER_GROUP, EXACT_SHADOW_GROUP_SAMPLE_LIMIT,
    EXACT_SHADOW_STORED_GROUP_LIMIT,
};

pub(super) fn run_one_step_exact_shadow_audit(
    groups: &BTreeMap<DiscardOrderShadowAuditKey, DiscardOrderShadowAuditGroup>,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    plugins: &CombatSearchPluginStack,
    candidate_keys: &BTreeSet<DiscardOrderShadowAuditKey>,
) -> DiscardOrderShadowAuditExactSummary {
    let mut exact = DiscardOrderShadowAuditExactSummary::default();
    for (key, group) in groups
        .iter()
        .filter(|(key, _)| candidate_keys.contains(*key))
        .take(EXACT_SHADOW_GROUP_SAMPLE_LIMIT)
    {
        let Some(result) = audit_group_one_step(stepper, config, plugins, group) else {
            continue;
        };
        exact.insert_result(key.clone(), result);
    }
    exact
}
