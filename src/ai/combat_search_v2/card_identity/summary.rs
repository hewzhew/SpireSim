use crate::runtime::combat::{CardLineageId, CombatState};
use crate::sim::combat_identity::{audit_combat_card_identity, CombatCardIdentityGroup};

#[derive(Clone, Debug, Default)]
pub(in crate::ai::combat_search_v2) struct CardIdentitySummary {
    pub(in crate::ai::combat_search_v2::card_identity) active_cards: usize,
    pub(in crate::ai::combat_search_v2::card_identity) action_payload_cards: usize,
    pub(in crate::ai::combat_search_v2::card_identity) action_payload_constructed_cards: usize,
    pub(in crate::ai::combat_search_v2::card_identity) duplicate_groups:
        Vec<CardIdentityGroupSummary>,
    pub(in crate::ai::combat_search_v2::card_identity) conflict_groups:
        Vec<CardIdentityGroupSummary>,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2::card_identity) struct CardIdentityGroupSummary {
    pub(in crate::ai::combat_search_v2::card_identity) lineage: CardLineageId,
    pub(in crate::ai::combat_search_v2::card_identity) occurrence_count: usize,
    pub(in crate::ai::combat_search_v2::card_identity) distinct_card_labels: Vec<String>,
    pub(in crate::ai::combat_search_v2::card_identity) locations: Vec<String>,
}

pub(in crate::ai::combat_search_v2) fn summarize_card_identity(
    combat: &CombatState,
) -> CardIdentitySummary {
    let audit = audit_combat_card_identity(combat);

    CardIdentitySummary {
        active_cards: audit.active_cards,
        action_payload_cards: audit.action_payload_cards,
        action_payload_constructed_cards: audit.action_payload_constructed_cards,
        duplicate_groups: audit
            .duplicate_active_lineage_groups
            .iter()
            .map(group_summary)
            .collect(),
        conflict_groups: audit
            .lineage_card_id_conflict_groups
            .iter()
            .map(group_summary)
            .collect(),
    }
}

fn group_summary(group: &CombatCardIdentityGroup) -> CardIdentityGroupSummary {
    CardIdentityGroupSummary {
        lineage: group.lineage,
        occurrence_count: group.occurrence_count,
        distinct_card_labels: group.distinct_card_labels.clone(),
        locations: group.locations.clone(),
    }
}
