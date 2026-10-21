use std::collections::{HashMap, HashSet};

use crate::ai::combat_search_v2::{
    CombatSearchV2Config, CombatSearchV2Report, CombatSearchV2TrajectoryReport,
};
use crate::content::cards::{get_card_definition, CardId, CardType};
use crate::runtime::combat::{CardLineageId, CombatCard};
use crate::sim::combat::CombatPosition;

use super::combat_candidate_line::{replay_candidate_line, CombatCandidateLine};
use super::combat_line_adjudication::{
    CombatLineAcceptancePolicy, CombatLineAdjudicationV1, CombatLineCleanlinessV1,
    CombatLineObservedOutcomeV1,
};
use super::session::RunControlSession;
use super::transition_report::CardSnapshot;

pub(super) struct CombatLineEvaluation {
    pub(super) line: CombatCandidateLine,
    pub(super) outcome: CombatLineObservedOutcomeV1,
}

impl CombatLineObservedOutcomeV1 {
    pub(super) fn gained_curse_count(&self) -> usize {
        self.gained_curses.len()
    }
}

pub(super) fn find_accepted_alternative_in_report(
    session: &RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    report: &CombatSearchV2Report,
    policy: CombatLineAcceptancePolicy,
) -> Result<Option<CombatLineEvaluation>, String> {
    find_accepted_alternative_in_report_matching(
        session,
        start,
        config,
        report,
        policy,
        |_| true,
        |_| true,
        prefer_accepted_outcome,
    )
}

pub(super) fn find_accepted_alternative_in_report_matching(
    session: &RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    report: &CombatSearchV2Report,
    policy: CombatLineAcceptancePolicy,
    mut trajectory_matches: impl FnMut(&CombatSearchV2TrajectoryReport) -> bool,
    mut outcome_matches: impl FnMut(&CombatLineObservedOutcomeV1) -> bool,
    mut prefer_outcome: impl FnMut(&CombatLineObservedOutcomeV1, &CombatLineObservedOutcomeV1) -> bool,
) -> Result<Option<CombatLineEvaluation>, String> {
    let mut best_clean: Option<CombatLineEvaluation> = None;
    for trajectory in &report.win_candidate_trajectories {
        if !trajectory_matches(trajectory) {
            continue;
        }
        let line = CombatCandidateLine::try_from_search_trajectory(trajectory)?;
        let evaluation = evaluate_combat_candidate_line_outcome(session, start, config, line)?;
        if !matches!(
            policy.adjudicate(evaluation.outcome.clone()),
            CombatLineAdjudicationV1::Accepted {
                cleanliness: CombatLineCleanlinessV1::Clean,
                ..
            }
        ) || !outcome_matches(&evaluation.outcome)
        {
            continue;
        }
        let replace = best_clean
            .as_ref()
            .map(|best| prefer_outcome(&evaluation.outcome, &best.outcome))
            .unwrap_or(true);
        if replace {
            best_clean = Some(evaluation);
        }
    }
    Ok(best_clean)
}

pub(super) fn evaluate_combat_candidate_line_outcome(
    session: &RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    line: CombatCandidateLine,
) -> Result<CombatLineEvaluation, String> {
    let before_master_deck = session.run_state.master_deck.clone();
    let before_deck_cards = master_deck_cards_by_lineage(session);
    let before_gold = session.run_state.gold;
    let replay = replay_candidate_line(start, line.source, &line.actions, config)?;
    let mut trial = session.clone();
    trial.mark_current_combat_search_resolved();
    for action in &replay.line.actions {
        trial.apply_input(action.input.clone())?;
    }
    let gained_curses = newly_gained_curses(&before_master_deck, &trial.run_state.master_deck);
    let ritual_dagger_growth = trial
        .run_state
        .master_deck
        .iter()
        .filter(|card| card.id == CardId::RitualDagger)
        .filter_map(|card| {
            before_deck_cards
                .get(&card.lineage)
                .filter(|before| before.id == CardId::RitualDagger)
                .map(|before| (card.misc_value - before.misc_value).max(0))
        })
        .sum();
    let outcome = CombatLineObservedOutcomeV1 {
        terminal: replay.line.terminal,
        final_hp: replay.line.final_hp,
        hp_loss: replay.line.hp_loss,
        potions_used: replay.line.potions_used,
        action_count: replay.line.actions.len(),
        gold_delta: trial.run_state.gold - before_gold,
        ritual_dagger_growth,
        gained_curses,
    };
    Ok(CombatLineEvaluation {
        line: replay.line,
        outcome,
    })
}

pub(super) fn render_combat_line_outcome_detail(outcome: &CombatLineObservedOutcomeV1) -> String {
    let gained_curses = outcome
        .gained_curses
        .iter()
        .map(|card| {
            format!(
                "{}#{}",
                crate::content::identity::gameplay_display_label_v1(card.id),
                card.lineage
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "terminal={:?} final_hp={} hp_loss={} potions_used={} actions={} gold_delta={} ritual_dagger_growth={} gained_curses=[{}]",
        outcome.terminal,
        outcome.final_hp,
        outcome.hp_loss,
        outcome.potions_used,
        outcome.action_count,
        outcome.gold_delta,
        outcome.ritual_dagger_growth,
        gained_curses
    )
}

pub(super) fn prefer_accepted_outcome(
    left: &CombatLineObservedOutcomeV1,
    right: &CombatLineObservedOutcomeV1,
) -> bool {
    left.final_hp > right.final_hp
        || (left.final_hp == right.final_hp
            && (
                left.potions_used,
                -left.ritual_dagger_growth,
                -left.gold_delta,
                left.action_count,
            ) < (
                right.potions_used,
                -right.ritual_dagger_growth,
                -right.gold_delta,
                right.action_count,
            ))
}

pub(super) fn prefer_quality_gated_accepted_outcome(
    left: &CombatLineObservedOutcomeV1,
    right: &CombatLineObservedOutcomeV1,
) -> bool {
    left.potions_used < right.potions_used
        || (left.potions_used == right.potions_used && prefer_accepted_outcome(left, right))
}

pub(super) fn newly_gained_curses(
    before: &[CombatCard],
    after: &[CombatCard],
) -> Vec<CardSnapshot> {
    let before_lineages = before
        .iter()
        .map(|card| card.lineage)
        .collect::<HashSet<_>>();
    after
        .iter()
        .filter(|card| {
            !before_lineages.contains(&card.lineage)
                && get_card_definition(card.id).card_type == CardType::Curse
        })
        .map(|card| CardSnapshot {
            id: card.id,
            lineage: card.lineage,
            upgrades: card.upgrades,
        })
        .collect()
}

struct DeckCardLineSnapshot {
    id: CardId,
    misc_value: i32,
}

fn master_deck_cards_by_lineage(
    session: &RunControlSession,
) -> HashMap<CardLineageId, DeckCardLineSnapshot> {
    session
        .run_state
        .master_deck
        .iter()
        .map(|card| {
            (
                card.lineage,
                DeckCardLineSnapshot {
                    id: card.id,
                    misc_value: card.misc_value,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::content::cards::CardId;
    use crate::runtime::combat::CombatCard;

    use crate::sim::combat::CombatTerminal;

    use super::{
        newly_gained_curses, prefer_accepted_outcome, prefer_quality_gated_accepted_outcome,
        CombatLineObservedOutcomeV1,
    };

    fn accepted_outcome(final_hp: i32, potions_used: u32) -> CombatLineObservedOutcomeV1 {
        CombatLineObservedOutcomeV1 {
            terminal: CombatTerminal::Win,
            final_hp,
            hp_loss: 80 - final_hp,
            potions_used,
            action_count: 10,
            gold_delta: 0,
            ritual_dagger_growth: 0,
            gained_curses: Vec::new(),
        }
    }

    #[test]
    fn newly_gained_curses_uses_lineage_and_ignores_preexisting_curses() {
        let before = vec![
            CombatCard::for_scenario(CardId::Parasite, 7),
            CombatCard::for_scenario(CardId::Strike, 8),
        ];
        let after = vec![
            CombatCard::for_scenario(CardId::Parasite, 7),
            CombatCard::for_scenario(CardId::Strike, 8),
            CombatCard::for_scenario(CardId::Parasite, 9),
            CombatCard::for_scenario(CardId::Defend, 10),
        ];

        assert_eq!(newly_gained_curses(&before, &after).len(), 1);
        assert_eq!(
            newly_gained_curses(&before, &after)[0].lineage,
            crate::test_support::scenario_lineage(9)
        );
    }

    #[test]
    fn quality_gated_preference_conserves_potions_before_polishing_hp() {
        let potion_free = accepted_outcome(64, 0);
        let spending = accepted_outcome(72, 1);

        assert!(prefer_accepted_outcome(&spending, &potion_free));
        assert!(prefer_quality_gated_accepted_outcome(
            &potion_free,
            &spending
        ));
        assert!(!prefer_quality_gated_accepted_outcome(
            &spending,
            &potion_free
        ));
    }

    #[test]
    fn quality_gated_preference_keeps_existing_order_with_equal_potion_use() {
        let lower_hp = accepted_outcome(64, 0);
        let higher_hp = accepted_outcome(72, 0);

        assert!(prefer_quality_gated_accepted_outcome(&higher_hp, &lower_hp));
    }
}
