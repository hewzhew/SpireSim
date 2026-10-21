use sts_oracle_runtime::runtime::combat::CombatState;
use sts_oracle_runtime::state::core::ClientInput;

use super::targets::key_card_targets;
use super::types::{CardZoneAtStep, CardZoneLabel, KeyCardPlay, TrackedKeyCard};
use super::zones::zone_for_lineage;

pub(super) fn tracked_key_cards(combat: &CombatState) -> Vec<TrackedKeyCard> {
    key_card_targets(combat)
        .into_iter()
        .map(|target| {
            let initial_zone = zone_for_lineage(combat, target.card.lineage);
            TrackedKeyCard {
                card: target.card,
                reason: target.reason,
                initial_zone,
                first_seen_zone: CardZoneAtStep {
                    step_index: 0,
                    zone: initial_zone,
                },
                first_play: None,
            }
        })
        .collect()
}

pub(super) fn note_played_key_card(
    tracked_cards: &mut [TrackedKeyCard],
    step_index: usize,
    action_key: &str,
    input: &ClientInput,
) {
    let ClientInput::PlayCard { card_index: _, .. } = input else {
        return;
    };
    for tracked in tracked_cards {
        if tracked.first_play.is_none()
            && action_key.contains(&format!("#{}", tracked.card.lineage))
            && action_key.contains("combat/play_card/")
        {
            tracked.first_play = Some(KeyCardPlay {
                step_index,
                action_key: action_key.to_string(),
                input: input.clone(),
            });
        }
    }
}

pub(super) fn note_seen_zones(
    tracked_cards: &mut [TrackedKeyCard],
    step_index: usize,
    combat: &CombatState,
) {
    for tracked in tracked_cards {
        let zone = zone_for_lineage(combat, tracked.card.lineage);
        if tracked.first_seen_zone.zone == CardZoneLabel::Missing && zone != CardZoneLabel::Missing
        {
            tracked.first_seen_zone = CardZoneAtStep { step_index, zone };
        }
    }
}
