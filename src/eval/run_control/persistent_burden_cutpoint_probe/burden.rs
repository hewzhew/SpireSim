use std::collections::HashMap;

use crate::content::cards::{get_card_definition, CardId, CardType};

use super::super::session::RunControlSession;
use super::PersistentBurdenGainedCurseCountV1;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct PersistentCurseBurdenSnapshot {
    counts: HashMap<CardId, usize>,
}

impl PersistentCurseBurdenSnapshot {
    pub(super) fn capture(session: &RunControlSession) -> Self {
        let mut counts = HashMap::new();
        for card in &session.run_state.master_deck {
            if get_card_definition(card.id).card_type == CardType::Curse {
                *counts.entry(card.id).or_default() += 1;
            }
        }
        Self { counts }
    }
}

pub(super) fn newly_gained_persistent_curses(
    before: &PersistentCurseBurdenSnapshot,
    after: &PersistentCurseBurdenSnapshot,
) -> Vec<PersistentBurdenGainedCurseCountV1> {
    let mut gained = after
        .counts
        .iter()
        .filter_map(|(card, after_count)| {
            let count = after_count.saturating_sub(before.counts.get(card).copied().unwrap_or(0));
            (count > 0).then_some(PersistentBurdenGainedCurseCountV1 { card: *card, count })
        })
        .collect::<Vec<_>>();
    gained.sort_by_key(|entry| entry.card as i32);
    gained
}
