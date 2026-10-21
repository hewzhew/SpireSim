use std::collections::{BTreeMap, BTreeSet};

use crate::sim::combat::CombatStepper;

mod exact;
use super::frontier::SearchNode;
use super::state_abstraction::{StateAbstractionRevealGate, StateDivergenceKind};
use super::turn_sequence_effect::TurnSequenceEffectFingerprint;
use super::types::{
    CombatSearchV2Config, CombatSearchV2DiagnosticsDiscardOrderShadowAudit,
    CombatSearchV2DiagnosticsDiscardOrderShadowAuditSample,
};
use exact::{
    run_one_step_exact_shadow_audit, DiscardOrderShadowAuditExactGroupResult,
    DiscardOrderShadowAuditExactSummary, DiscardOrderShadowAuditGroup,
    EXACT_SHADOW_ACTIONS_PER_GROUP, EXACT_SHADOW_GROUP_SAMPLE_LIMIT,
    EXACT_SHADOW_STORED_GROUP_LIMIT,
};

const DISCARD_ORDER_SHADOW_AUDIT_SAMPLE_LIMIT: usize = 8;

#[derive(Clone, Debug)]
pub(super) struct DiscardOrderShadowAuditObservation {
    pub origin_key: String,
    pub unordered_key_preview: String,
    pub states: u64,
    pub max_prefix_length: usize,
    pub ordered_variants: usize,
    pub effect_variants: usize,
    pub max_legal_actions: usize,
    pub first_divergence_path: Option<&'static str>,
    pub reveal_gate: StateAbstractionRevealGate,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct DiscardOrderShadowAuditKey {
    pub origin_key: String,
    pub unordered_key: String,
}

#[derive(Default)]
pub(super) struct DiscardOrderShadowAuditCollector {
    groups: BTreeMap<DiscardOrderShadowAuditKey, DiscardOrderShadowAuditGroup>,
    exact: DiscardOrderShadowAuditExactSummary,
}

pub(super) fn is_static_discard_order_candidate(
    kind: StateDivergenceKind,
    first_divergence_path: Option<&'static str>,
    reveal_gate: StateAbstractionRevealGate,
) -> bool {
    matches!(kind, StateDivergenceKind::DiscardOrderDelta)
        && first_divergence_path == Some("combat.zones.discard_pile")
        && matches!(reveal_gate, StateAbstractionRevealGate::NextShuffle)
}

impl DiscardOrderShadowAuditCollector {
    pub(super) fn observe_state(
        &mut self,
        key: DiscardOrderShadowAuditKey,
        ordered_key: &str,
        effect_key: &str,
        effect_fingerprint: &TurnSequenceEffectFingerprint,
        node: &SearchNode,
    ) {
        if !self.groups.contains_key(&key) && self.groups.len() >= EXACT_SHADOW_STORED_GROUP_LIMIT {
            return;
        }

        let group = self.groups.entry(key).or_default();
        group.observe_representative(ordered_key, effect_key, effect_fingerprint, node);
    }

    pub(super) fn run_one_step_exact_shadow_audit(
        &mut self,
        stepper: &impl CombatStepper,
        config: &CombatSearchV2Config,
        plugins: &super::plugins::CombatSearchPluginStack,
        candidate_keys: &BTreeSet<DiscardOrderShadowAuditKey>,
    ) {
        self.exact =
            run_one_step_exact_shadow_audit(&self.groups, stepper, config, plugins, candidate_keys);
    }

    fn exact_result(
        &self,
        origin_key: &str,
        unordered_key_preview: &str,
    ) -> Option<&DiscardOrderShadowAuditExactGroupResult> {
        self.exact.result_for(origin_key, unordered_key_preview)
    }
}

pub(super) fn summarize_discard_order_shadow_audit(
    mut observations: Vec<DiscardOrderShadowAuditObservation>,
    collector: &DiscardOrderShadowAuditCollector,
) -> CombatSearchV2DiagnosticsDiscardOrderShadowAudit {
    observations.sort_by(|left, right| {
        let left_exact = collector
            .exact_result(&left.origin_key, &left.unordered_key_preview)
            .is_some();
        let right_exact = collector
            .exact_result(&right.origin_key, &right.unordered_key_preview)
            .is_some();
        right_exact
            .cmp(&left_exact)
            .then_with(|| right.states.cmp(&left.states))
            .then_with(|| right.ordered_variants.cmp(&left.ordered_variants))
            .then_with(|| right.effect_variants.cmp(&left.effect_variants))
            .then_with(|| left.origin_key.cmp(&right.origin_key))
            .then_with(|| left.unordered_key_preview.cmp(&right.unordered_key_preview))
    });

    let candidate_groups = observations.len();
    let candidate_states = observations.iter().map(|item| item.states).sum();
    let samples = observations
        .into_iter()
        .take(DISCARD_ORDER_SHADOW_AUDIT_SAMPLE_LIMIT)
        .map(|item| {
            let exact = collector.exact_result(&item.origin_key, &item.unordered_key_preview);
            CombatSearchV2DiagnosticsDiscardOrderShadowAuditSample {
                origin_key: item.origin_key,
                unordered_key_preview: item.unordered_key_preview,
                states: item.states,
                max_prefix_length: item.max_prefix_length,
                ordered_variants: item.ordered_variants,
                effect_variants: item.effect_variants,
                max_legal_actions: item.max_legal_actions,
                first_divergence_path: item.first_divergence_path,
                reveal_gate: item.reveal_gate,
                one_step_exact_status: exact.map(|result| result.status).unwrap_or("not_sampled"),
                one_step_exact_checked_actions: exact
                    .map(|result| result.checked_actions)
                    .unwrap_or(0),
                one_step_exact_verified_actions: exact
                    .map(|result| result.verified_actions)
                    .unwrap_or(0),
                one_step_exact_blocked_actions: exact
                    .map(|result| result.blocked_actions)
                    .unwrap_or(0),
                one_step_exact_blocking_action_key: exact
                    .and_then(|result| result.blocking_action_key.clone()),
                one_step_exact_blocking_divergence_kind: exact
                    .and_then(|result| result.blocking_divergence_kind),
                one_step_exact_blocking_path: exact.and_then(|result| result.blocking_path),
            }
        })
        .collect();

    CombatSearchV2DiagnosticsDiscardOrderShadowAudit {
        audit_policy: "static_discard_order_candidate_plus_bounded_one_step_exact_shadow",
        behavioral_effect: "diagnostic_only_no_prune_no_state_merge",
        candidate_groups,
        candidate_states,
        static_immediate_safe_groups: candidate_groups,
        static_immediate_safe_states: candidate_states,
        exact_rollout_verified_groups: 0,
        proof_pruning_enabled: false,
        reveal_gate: StateAbstractionRevealGate::NextShuffle,
        one_step_exact_policy: "sample_representative_pairs_compare_common_actions_one_step",
        one_step_exact_stored_group_limit: EXACT_SHADOW_STORED_GROUP_LIMIT,
        one_step_exact_sample_limit_groups: EXACT_SHADOW_GROUP_SAMPLE_LIMIT,
        one_step_exact_sample_limit_actions_per_group: EXACT_SHADOW_ACTIONS_PER_GROUP,
        one_step_exact_checked_groups: collector.exact.checked_groups,
        one_step_exact_sample_verified_groups: collector.exact.sample_verified_groups,
        one_step_exact_blocked_groups: collector.exact.blocked_groups,
        one_step_exact_checked_actions: collector.exact.checked_actions,
        one_step_exact_verified_actions: collector.exact.verified_actions,
        one_step_exact_blocked_actions: collector.exact.blocked_actions,
        sample_limit: DISCARD_ORDER_SHADOW_AUDIT_SAMPLE_LIMIT,
        samples,
        notes: vec![
            "static audit only identifies candidate groups; exact shadow audit is bounded and sampled",
            "one-step exact audit applies common legal actions from paired exact states and compares the resulting typed effect boundary",
            "sample-verified groups are not prune-safe because the audit is one-step and action-sampled",
            "exact_rollout_verified_groups stays zero until a simulator-backed until-reveal-gate rollout audit is implemented",
        ],
    }
}

#[cfg(test)]
mod tests;
