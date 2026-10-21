use std::collections::BTreeMap;

use super::super::super::frontier::SearchNode;
use super::super::super::state_abstraction::StateDivergenceKind;
use super::super::super::turn_sequence_effect::TurnSequenceEffectFingerprint;
use super::super::DiscardOrderShadowAuditKey;

pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) const EXACT_SHADOW_STORED_GROUP_LIMIT: usize = 1024;
pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) const EXACT_SHADOW_GROUP_SAMPLE_LIMIT: usize = 16;
pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) const EXACT_SHADOW_REPRESENTATIVES_PER_GROUP: usize = 2;
pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) const EXACT_SHADOW_ACTIONS_PER_GROUP: usize = 8;

#[derive(Clone)]
pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) struct DiscardOrderShadowAuditRepresentative
{
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) ordered_key: String,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) effect_key: String,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) effect_fingerprint:
        TurnSequenceEffectFingerprint,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) node: SearchNode,
}

#[derive(Clone, Default)]
pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) struct DiscardOrderShadowAuditGroup
{
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) representatives:
        Vec<DiscardOrderShadowAuditRepresentative>,
}

#[derive(Clone, Debug, Default)]
pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) struct DiscardOrderShadowAuditExactSummary
{
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) checked_groups: usize,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) sample_verified_groups: usize,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) blocked_groups: usize,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) checked_actions: usize,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) verified_actions: usize,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) blocked_actions: usize,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) group_results:
        BTreeMap<DiscardOrderShadowAuditKey, DiscardOrderShadowAuditExactGroupResult>,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) struct DiscardOrderShadowAuditExactGroupResult
{
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) status: &'static str,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) checked_actions: usize,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) verified_actions: usize,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) blocked_actions: usize,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) blocking_action_key:
        Option<String>,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) blocking_divergence_kind:
        Option<StateDivergenceKind>,
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) blocking_path:
        Option<&'static str>,
}
