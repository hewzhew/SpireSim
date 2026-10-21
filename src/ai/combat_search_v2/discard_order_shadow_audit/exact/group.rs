use super::super::super::frontier::SearchNode;
use super::super::super::turn_sequence_effect::TurnSequenceEffectFingerprint;
use super::super::DiscardOrderShadowAuditKey;
use super::types::{
    DiscardOrderShadowAuditExactGroupResult, DiscardOrderShadowAuditExactSummary,
    DiscardOrderShadowAuditGroup, DiscardOrderShadowAuditRepresentative,
    EXACT_SHADOW_REPRESENTATIVES_PER_GROUP,
};

impl DiscardOrderShadowAuditGroup {
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) fn observe_representative(
        &mut self,
        ordered_key: &str,
        effect_key: &str,
        effect_fingerprint: &TurnSequenceEffectFingerprint,
        node: &SearchNode,
    ) {
        if self
            .representatives
            .iter()
            .any(|representative| representative.ordered_key == ordered_key)
        {
            return;
        }
        if self.representatives.len() >= EXACT_SHADOW_REPRESENTATIVES_PER_GROUP {
            return;
        }

        self.representatives
            .push(DiscardOrderShadowAuditRepresentative {
                ordered_key: ordered_key.to_string(),
                effect_key: effect_key.to_string(),
                effect_fingerprint: effect_fingerprint.clone(),
                node: node.clone(),
            });
    }
}

impl DiscardOrderShadowAuditExactSummary {
    pub(in crate::ai::combat_search_v2::discard_order_shadow_audit) fn result_for(
        &self,
        origin_key: &str,
        unordered_key_preview: &str,
    ) -> Option<&DiscardOrderShadowAuditExactGroupResult> {
        self.group_results
            .iter()
            .find(|(key, _)| {
                key.origin_key == origin_key && preview(&key.unordered_key) == unordered_key_preview
            })
            .map(|(_, result)| result)
    }

    pub(super) fn insert_result(
        &mut self,
        key: DiscardOrderShadowAuditKey,
        result: DiscardOrderShadowAuditExactGroupResult,
    ) {
        self.checked_groups += 1;
        self.checked_actions += result.checked_actions;
        self.verified_actions += result.verified_actions;
        self.blocked_actions += result.blocked_actions;
        if result.status == "sample_verified_one_step" {
            self.sample_verified_groups += 1;
        } else if result.blocked_actions > 0 {
            self.blocked_groups += 1;
        }
        self.group_results.insert(key, result);
    }
}

fn preview(value: &str) -> String {
    const PREVIEW_LIMIT: usize = 180;
    if value.len() <= PREVIEW_LIMIT {
        value.to_string()
    } else {
        format!("{}...", &value[..PREVIEW_LIMIT])
    }
}
