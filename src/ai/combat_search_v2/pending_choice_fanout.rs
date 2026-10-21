use crate::state::core::PendingChoice;

pub(super) const HIGH_PENDING_CHOICE_ACTION_FANOUT: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct PendingChoiceFanoutSummary {
    pub(super) candidate_count: usize,
    pub(super) estimated_action_fanout: usize,
    pub(super) high_fanout: bool,
}

pub(super) fn pending_choice_fanout(choice: &PendingChoice) -> PendingChoiceFanoutSummary {
    let (candidate_count, estimated_action_fanout) = match choice {
        PendingChoice::HandSelect {
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            ..
        } => (
            candidate_lineages.len(),
            bounded_selection_fanout(
                candidate_lineages.len(),
                *min_cards as usize,
                *max_cards as usize,
                *can_cancel,
            ),
        ),
        PendingChoice::GridSelect {
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            ..
        } => (
            candidate_lineages.len(),
            bounded_selection_fanout(
                candidate_lineages.len(),
                *min_cards as usize,
                *max_cards as usize,
                *can_cancel,
            ),
        ),
        PendingChoice::DiscoverySelect(choice) => (
            choice.cards.len(),
            choice
                .cards
                .len()
                .saturating_add(usize::from(choice.can_skip)),
        ),
        PendingChoice::ScrySelect { cards, .. } => (cards.len(), scry_fanout(cards.len())),
        PendingChoice::CardRewardSelect {
            cards, can_skip, ..
        } => (
            cards.len(),
            cards.len().saturating_add(usize::from(*can_skip)),
        ),
        PendingChoice::ForeignInfluenceSelect { cards, .. } => (cards.len(), cards.len()),
        PendingChoice::ChooseOneSelect { choices } => (choices.len(), choices.len()),
        PendingChoice::StanceChoice => (2, 2),
    };

    PendingChoiceFanoutSummary {
        candidate_count,
        estimated_action_fanout,
        high_fanout: estimated_action_fanout > HIGH_PENDING_CHOICE_ACTION_FANOUT,
    }
}

pub(super) fn fanout_class(estimated_action_fanout: usize) -> &'static str {
    match estimated_action_fanout {
        0 => "empty",
        1..=8 => "small",
        9..=HIGH_PENDING_CHOICE_ACTION_FANOUT => "medium",
        _ => "large",
    }
}

fn bounded_selection_fanout(
    candidate_count: usize,
    min_cards: usize,
    max_cards: usize,
    can_cancel: bool,
) -> usize {
    let min_cards = min_cards.min(candidate_count);
    let max_cards = max_cards.min(candidate_count);
    if min_cards > max_cards {
        return usize::from(can_cancel);
    }

    let selection_fanout = (min_cards..=max_cards)
        .map(|selected| combination_count_capped(candidate_count, selected))
        .fold(0usize, usize::saturating_add);
    selection_fanout.saturating_add(usize::from(can_cancel))
}

fn scry_fanout(candidate_count: usize) -> usize {
    if candidate_count >= usize::BITS as usize {
        return usize::MAX;
    }
    1usize << candidate_count
}

fn combination_count_capped(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut value = 1usize;
    for idx in 0..k {
        value = value.saturating_mul(n - idx) / (idx + 1);
        if value > HIGH_PENDING_CHOICE_ACTION_FANOUT {
            return value;
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_card_grid_select_is_linear_not_high_fanout() {
        let choice = PendingChoice::GridSelect {
            source_pile: crate::state::core::PileType::Discard,
            candidate_lineages: crate::test_support::scenario_lineages(0..13),
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: crate::state::core::GridSelectReason::MoveToDrawPile,
        };

        let fanout = pending_choice_fanout(&choice);

        assert_eq!(fanout.candidate_count, 13);
        assert_eq!(fanout.estimated_action_fanout, 13);
        assert!(!fanout.high_fanout);
        assert_eq!(fanout_class(fanout.estimated_action_fanout), "medium");
    }

    #[test]
    fn large_scry_is_combinatorial_high_fanout() {
        let choice = PendingChoice::ScrySelect {
            cards: vec![crate::content::cards::CardId::Strike; 7],
            card_lineages: crate::test_support::scenario_lineages(0..7),
        };

        let fanout = pending_choice_fanout(&choice);

        assert_eq!(fanout.candidate_count, 7);
        assert_eq!(fanout.estimated_action_fanout, 128);
        assert!(fanout.high_fanout);
        assert_eq!(fanout_class(fanout.estimated_action_fanout), "large");
    }
}
