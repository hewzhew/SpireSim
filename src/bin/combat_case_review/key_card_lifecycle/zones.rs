use sts_oracle_runtime::runtime::combat::{CardLineageId, CombatState};

use super::types::CardZoneLabel;

pub(super) fn zone_for_lineage(combat: &CombatState, lineage: CardLineageId) -> CardZoneLabel {
    if combat.zones.hand.iter().any(|card| card.lineage == lineage) {
        CardZoneLabel::Hand
    } else if combat
        .zones
        .draw_pile
        .iter()
        .any(|card| card.lineage == lineage)
    {
        CardZoneLabel::Draw
    } else if combat
        .zones
        .discard_pile
        .iter()
        .any(|card| card.lineage == lineage)
    {
        CardZoneLabel::Discard
    } else if combat
        .zones
        .exhaust_pile
        .iter()
        .any(|card| card.lineage == lineage)
    {
        CardZoneLabel::Exhaust
    } else if combat
        .zones
        .limbo
        .iter()
        .any(|card| card.lineage == lineage)
    {
        CardZoneLabel::Limbo
    } else if combat
        .zones
        .queued_cards
        .iter()
        .any(|queued| queued.card.lineage == lineage)
    {
        CardZoneLabel::Queued
    } else if combat
        .meta
        .master_deck_snapshot
        .iter()
        .any(|card| card.lineage == lineage)
    {
        CardZoneLabel::MasterOnly
    } else {
        CardZoneLabel::Missing
    }
}
