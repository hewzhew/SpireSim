use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::content::identity::gameplay_display_label_v1;
use crate::runtime::action::Action;
use crate::runtime::combat::{
    CardLineageId, CardLineageOrigin, CombatCard, CombatState, PowerPayload,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CombatCardIdentityAudit {
    pub active_cards: usize,
    pub master_deck_cards: usize,
    pub action_payload_cards: usize,
    pub action_payload_constructed_cards: usize,
    pub next_combat_generated_ordinal: u32,
    pub unallocated_combat_generated_lineages: Vec<CardLineageId>,
    pub duplicate_active_lineage_groups: Vec<CombatCardIdentityGroup>,
    pub lineage_card_id_conflict_groups: Vec<CombatCardIdentityGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatCardIdentityGroup {
    pub lineage: CardLineageId,
    pub occurrence_count: usize,
    pub distinct_card_labels: Vec<String>,
    pub distinct_card_id_labels: Vec<String>,
    pub locations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CombatCardIdentityOccurrence {
    location: String,
    card_label: String,
    card_id_label: String,
}

impl CombatCardIdentityAudit {
    pub fn has_lineage_card_id_conflict(&self) -> bool {
        !self.lineage_card_id_conflict_groups.is_empty()
    }

    pub fn capture_rejection_reason(&self) -> Option<String> {
        let group = self.lineage_card_id_conflict_groups.first()?;
        Some(format!(
            "combat card identity conflict: active lineage {} maps to multiple card ids [{}] at [{}]",
            group.lineage,
            group.distinct_card_id_labels.join(", "),
            group.locations.join(", ")
        ))
    }

    pub fn allocator_rejection_reason(&self) -> Option<String> {
        let lineage = self.unallocated_combat_generated_lineages.first()?;
        Some(format!(
            "combat card identity conflict: combat-generated lineage {lineage} was not allocated by the owning combat allocator (next ordinal {})",
            self.next_combat_generated_ordinal
        ))
    }
}

pub fn audit_combat_card_identity(combat: &CombatState) -> CombatCardIdentityAudit {
    let mut active_by_lineage: BTreeMap<CardLineageId, Vec<CombatCardIdentityOccurrence>> =
        BTreeMap::new();
    collect_active_cards("hand", &combat.zones.hand, &mut active_by_lineage);
    collect_active_cards("draw", &combat.zones.draw_pile, &mut active_by_lineage);
    collect_active_cards(
        "discard",
        &combat.zones.discard_pile,
        &mut active_by_lineage,
    );
    collect_active_cards(
        "exhaust",
        &combat.zones.exhaust_pile,
        &mut active_by_lineage,
    );
    collect_active_cards("limbo", &combat.zones.limbo, &mut active_by_lineage);
    for (idx, queued) in combat.zones.queued_cards.iter().enumerate() {
        push_card_occurrence(
            &mut active_by_lineage,
            format!("queued[{idx}]"),
            &queued.card,
        );
    }

    let mut referenced_lineages = combat
        .meta
        .master_deck_snapshot
        .iter()
        .chain(combat.zones.hand.iter())
        .chain(combat.zones.draw_pile.iter())
        .chain(combat.zones.discard_pile.iter())
        .chain(combat.zones.exhaust_pile.iter())
        .chain(combat.zones.limbo.iter())
        .map(|card| card.lineage)
        .collect::<BTreeSet<_>>();
    referenced_lineages.extend(
        combat
            .zones
            .queued_cards
            .iter()
            .map(|queued| queued.card.lineage),
    );

    let mut duplicate_active_lineage_groups = Vec::new();
    let mut lineage_card_id_conflict_groups = Vec::new();
    for (lineage, occurrences) in active_by_lineage {
        if occurrences.len() > 1 {
            duplicate_active_lineage_groups.push(group_summary(lineage, &occurrences));
        }
        let distinct_ids = occurrences
            .iter()
            .map(|occurrence| occurrence.card_id_label.as_str())
            .collect::<BTreeSet<_>>();
        if distinct_ids.len() > 1 {
            lineage_card_id_conflict_groups.push(group_summary(lineage, &occurrences));
        }
    }

    let action_payload_cards = action_payload_cards(&combat.engine.action_queue);
    let action_payload_constructed_cards = action_payload_cards
        .iter()
        .filter(|card| card.lineage.origin() == CardLineageOrigin::ConstructedScenario)
        .count();
    referenced_lineages.extend(action_payload_cards.iter().map(|card| card.lineage));
    let unallocated_combat_generated_lineages = referenced_lineages
        .into_iter()
        .filter(|lineage| {
            lineage.origin() == CardLineageOrigin::CombatGenerated
                && !combat
                    .zones
                    .combat_lineage_allocator
                    .contains_allocated(*lineage)
        })
        .collect();

    CombatCardIdentityAudit {
        active_cards: active_card_count(combat),
        master_deck_cards: combat.meta.master_deck_snapshot.len(),
        action_payload_cards: action_payload_cards.len(),
        action_payload_constructed_cards,
        next_combat_generated_ordinal: combat
            .zones
            .combat_lineage_allocator
            .next_creation_ordinal(),
        unallocated_combat_generated_lineages,
        duplicate_active_lineage_groups,
        lineage_card_id_conflict_groups,
    }
}

pub fn validate_combat_card_identity_for_capture(combat: &CombatState) -> Result<(), String> {
    let audit = audit_combat_card_identity(combat);
    if let Some(reason) = audit.capture_rejection_reason() {
        return Err(reason);
    }
    if let Some(reason) = audit.allocator_rejection_reason() {
        return Err(reason);
    }
    Ok(())
}

fn collect_active_cards<'a>(
    zone: &str,
    cards: impl IntoIterator<Item = &'a CombatCard>,
    by_lineage: &mut BTreeMap<CardLineageId, Vec<CombatCardIdentityOccurrence>>,
) {
    for (idx, card) in cards.into_iter().enumerate() {
        push_card_occurrence(by_lineage, format!("{zone}[{idx}]"), card);
    }
}

fn push_card_occurrence(
    by_lineage: &mut BTreeMap<CardLineageId, Vec<CombatCardIdentityOccurrence>>,
    location: String,
    card: &CombatCard,
) {
    by_lineage
        .entry(card.lineage)
        .or_default()
        .push(CombatCardIdentityOccurrence {
            location: format!("{location}:{}", card_label(card)),
            card_label: card_label(card),
            card_id_label: card.id.stable_id().to_string(),
        });
}

fn active_card_count(combat: &CombatState) -> usize {
    combat.zones.hand.len()
        + combat.zones.draw_pile.len()
        + combat.zones.discard_pile.len()
        + combat.zones.exhaust_pile.len()
        + combat.zones.limbo.len()
        + combat.zones.queued_cards.len()
}

fn action_payload_cards(queue: &VecDeque<Action>) -> Vec<&CombatCard> {
    let mut cards = Vec::new();
    for action in queue {
        collect_action_payload_cards(action, &mut cards);
    }
    cards
}

fn collect_action_payload_cards<'a>(action: &'a Action, cards: &mut Vec<&'a CombatCard>) {
    match action {
        Action::AttackDamageRandomEnemyCard { card }
        | Action::PlayCardDirect { card, .. }
        | Action::MakeCopyInHand { original: card, .. }
        | Action::MakeCopyInDiscard { original: card, .. }
        | Action::UseCardAfterUseHooks { card } => cards.push(card),
        Action::EnqueueCardPlay { item, .. } => cards.push(&item.card),
        Action::ApplyPowerWithPayload {
            payload: PowerPayload::Card(card),
            ..
        } => cards.push(card),
        _ => {}
    }
}

fn group_summary(
    lineage: CardLineageId,
    occurrences: &[CombatCardIdentityOccurrence],
) -> CombatCardIdentityGroup {
    CombatCardIdentityGroup {
        lineage,
        occurrence_count: occurrences.len(),
        distinct_card_labels: occurrences
            .iter()
            .map(|occurrence| occurrence.card_label.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        distinct_card_id_labels: occurrences
            .iter()
            .map(|occurrence| occurrence.card_id_label.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        locations: occurrences
            .iter()
            .map(|occurrence| occurrence.location.clone())
            .collect(),
    }
}

fn card_label(card: &CombatCard) -> String {
    format!(
        "{}+{}#{} cost:{} misc:{} free:{}",
        gameplay_display_label_v1(card.id),
        card.upgrades,
        card.lineage,
        card.cost_for_turn_java(),
        card.misc_value,
        card.free_to_play_once
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::runtime::action::Action;
    use crate::runtime::combat::CombatCard;
    use crate::test_support::blank_test_combat;

    #[test]
    fn combat_card_identity_reports_lineage_card_id_conflict() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 10),
        ];

        let audit = audit_combat_card_identity(&combat);

        assert_eq!(audit.active_cards, 2);
        assert_eq!(audit.master_deck_cards, 0);
        assert_eq!(audit.duplicate_active_lineage_groups.len(), 1);
        assert_eq!(audit.lineage_card_id_conflict_groups.len(), 1);
        assert!(validate_combat_card_identity_for_capture(&combat)
            .expect_err("capture validation should reject conflicting card identity")
            .contains("active lineage constructed_scenario:10"));
    }

    #[test]
    fn combat_card_identity_allows_same_lineage_same_card_id_instances() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Strike, 10),
        ];
        let audit = audit_combat_card_identity(&combat);

        assert_eq!(audit.duplicate_active_lineage_groups.len(), 1);
        assert!(audit.lineage_card_id_conflict_groups.is_empty());
        validate_combat_card_identity_for_capture(&combat)
            .expect("same-card same-lineage Java same-instance paths should remain capturable");
    }

    #[test]
    fn combat_card_identity_counts_action_payload_placeholders_separately() {
        let mut combat = blank_test_combat();
        combat
            .engine
            .action_queue
            .push_back(Action::MakeCopyInHand {
                original: Box::new(CombatCard::for_scenario(CardId::Strike, 0)),
                amount: 1,
            });

        let audit = audit_combat_card_identity(&combat);

        assert!(audit.duplicate_active_lineage_groups.is_empty());
        assert_eq!(audit.action_payload_cards, 1);
        assert_eq!(audit.action_payload_constructed_cards, 1);
        validate_combat_card_identity_for_capture(&combat)
            .expect("action payload cards are not active card identity conflicts");
    }

    #[test]
    fn combat_card_identity_rejects_unallocated_combat_lineage() {
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![CombatCard::with_lineage(
            CardId::Strike,
            CardLineageId::combat_generated(20),
        )];

        let audit = audit_combat_card_identity(&combat);

        assert_eq!(
            audit.unallocated_combat_generated_lineages,
            vec![CardLineageId::combat_generated(20)]
        );
        assert!(validate_combat_card_identity_for_capture(&combat)
            .expect_err("combat-generated lineage must come from the allocator")
            .contains("combat_generated:20"));
    }
}
