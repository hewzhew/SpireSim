use sts_oracle_runtime::content::identity::gameplay_display_label_v1;
use sts_oracle_runtime::runtime::combat::CombatState;

use super::types::{
    CardZoneAtStep, CardZoneLabel, KeyCardLifecycle, KeyCardLifecycleReport, TrackedKeyCard,
};
use super::zones::zone_for_lineage;

pub(super) fn report_without_focus(tracked_cards: Vec<TrackedKeyCard>) -> KeyCardLifecycleReport {
    KeyCardLifecycleReport {
        schema: "key_card_lifecycle_v0",
        contract: "exact_replay_key_card_visibility_and_play_timing_no_strategy_verdict",
        basis_line: "none",
        witness_action_count: None,
        replayed_actions: 0,
        truncated_by_preview: false,
        truncated: false,
        timed_out: false,
        tracked_cards: tracked_cards
            .into_iter()
            .map(|tracked| lifecycle_from_tracked(tracked, 0, CardZoneLabel::Missing))
            .collect(),
    }
}

pub(super) fn finish_lifecycles(
    tracked_cards: Vec<TrackedKeyCard>,
    replayed_actions: usize,
    combat: &CombatState,
) -> Vec<KeyCardLifecycle> {
    tracked_cards
        .into_iter()
        .map(|tracked| {
            let final_zone = zone_for_lineage(combat, tracked.card.lineage);
            lifecycle_from_tracked(tracked, replayed_actions, final_zone)
        })
        .collect()
}

fn lifecycle_from_tracked(
    tracked: TrackedKeyCard,
    final_step_index: usize,
    final_zone: CardZoneLabel,
) -> KeyCardLifecycle {
    KeyCardLifecycle {
        card: format!(
            "{}+{}",
            gameplay_display_label_v1(tracked.card.id),
            tracked.card.upgrades
        ),
        lineage: tracked.card.lineage,
        upgrades: tracked.card.upgrades,
        reason: tracked.reason,
        initial_zone: tracked.initial_zone,
        first_seen_zone: tracked.first_seen_zone,
        played_in_replay: tracked.first_play.is_some(),
        first_play: tracked.first_play,
        final_zone: CardZoneAtStep {
            step_index: final_step_index,
            zone: final_zone,
        },
    }
}
