use super::super::turn_sequence_effect::{
    TurnSequenceEffectAggregate, TurnSequenceEffectFingerprint,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct TurnSequenceSummary {
    pub(super) prefix_length: usize,
    pub(super) legal_actions: usize,
    pub(super) origin_key: Option<String>,
    pub(super) ordered_key: Option<String>,
    pub(super) unordered_key: Option<String>,
    pub(super) effect_key: Option<String>,
    pub(super) effect_fingerprint: Option<TurnSequenceEffectFingerprint>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct TurnSequenceGroupKey {
    pub(super) origin_key: String,
    pub(super) unordered_key: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TurnSequenceGroupAggregate {
    pub(super) states: u64,
    pub(super) max_prefix_length: usize,
    pub(super) max_legal_actions: usize,
    pub(super) ordered_variants: BTreeSet<String>,
    pub(super) effect_variants: BTreeSet<String>,
    pub(super) effect_components: TurnSequenceEffectAggregate,
}
