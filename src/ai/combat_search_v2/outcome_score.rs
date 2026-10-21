use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CombatOutcomeScore {
    terminal_rank: i32,
    run_hygiene: i32,
    persistent_adjusted_hp: i32,
    final_hp: i32,
    persistent_run_value: i32,
    potion_conservation: i32,
    faster_turns: i32,
    fewer_cards_played: i32,
    enemy_progress: i32,
    shorter_line: i32,
}

impl CombatOutcomeScore {
    pub(super) fn from_node(node: &SearchNode) -> Self {
        let persistent_run_value =
            super::external_payoff::materialized_persistent_payoff_score_v1(&node.combat);
        Self {
            terminal_rank: terminal_rank(terminal_label(&node.engine, &node.combat)),
            run_hygiene: -super::external_payoff::external_burden_count(&node.combat),
            persistent_adjusted_hp: node
                .combat
                .entities
                .player
                .current_hp
                .saturating_add(persistent_run_value),
            final_hp: node.combat.entities.player.current_hp,
            persistent_run_value,
            potion_conservation: -((node.potions_used + node.potions_discarded) as i32),
            faster_turns: -(node.combat.turn.turn_count as i32),
            fewer_cards_played: -(node.cards_played as i32),
            enemy_progress: -total_living_enemy_hp(&node.combat),
            shorter_line: -(node.actions.len() as i32),
        }
    }

    pub(super) fn to_report_key(self) -> CombatSearchV2OutcomeOrderKeyReport {
        CombatSearchV2OutcomeOrderKeyReport {
            terminal_rank: self.terminal_rank,
            run_hygiene: self.run_hygiene,
            persistent_adjusted_hp: self.persistent_adjusted_hp,
            final_hp: self.final_hp,
            persistent_run_value: self.persistent_run_value,
            potion_conservation: self.potion_conservation,
            faster_turns: self.faster_turns,
            fewer_cards_played: self.fewer_cards_played,
            enemy_progress: self.enemy_progress,
            shorter_line: self.shorter_line,
        }
    }
}

impl Ord for CombatOutcomeScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.terminal_rank
            .cmp(&other.terminal_rank)
            .then_with(|| self.run_hygiene.cmp(&other.run_hygiene))
            .then_with(|| {
                self.persistent_adjusted_hp
                    .cmp(&other.persistent_adjusted_hp)
            })
            .then_with(|| self.final_hp.cmp(&other.final_hp))
            .then_with(|| self.persistent_run_value.cmp(&other.persistent_run_value))
            .then_with(|| self.potion_conservation.cmp(&other.potion_conservation))
            .then_with(|| self.faster_turns.cmp(&other.faster_turns))
            .then_with(|| self.fewer_cards_played.cmp(&other.fewer_cards_played))
            .then_with(|| self.enemy_progress.cmp(&other.enemy_progress))
            .then_with(|| self.shorter_line.cmp(&other.shorter_line))
    }
}

impl PartialOrd for CombatOutcomeScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(super) fn external_burden_count(combat: &CombatState) -> i32 {
    super::external_payoff::external_burden_count(combat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::MetaChange;
    use crate::test_support::blank_test_combat;

    #[test]
    fn outcome_score_prefers_higher_final_hp_for_same_terminal() {
        let low = SearchNode::test_node_with_hp(10);
        let high = SearchNode::test_node_with_hp(20);

        assert!(CombatOutcomeScore::from_node(&high) > CombatOutcomeScore::from_node(&low));
    }

    #[test]
    fn outcome_order_report_key_compares_identically_to_internal_score() {
        let zero = CombatOutcomeScore {
            terminal_rank: 0,
            run_hygiene: 0,
            persistent_adjusted_hp: 0,
            final_hp: 0,
            persistent_run_value: 0,
            potion_conservation: 0,
            faster_turns: 0,
            fewer_cards_played: 0,
            enemy_progress: 0,
            shorter_line: 0,
        };
        let scores = [
            zero,
            CombatOutcomeScore {
                terminal_rank: 1,
                ..zero
            },
            CombatOutcomeScore {
                run_hygiene: 1,
                ..zero
            },
            CombatOutcomeScore {
                persistent_adjusted_hp: 1,
                ..zero
            },
            CombatOutcomeScore {
                final_hp: 1,
                ..zero
            },
            CombatOutcomeScore {
                persistent_run_value: 1,
                ..zero
            },
            CombatOutcomeScore {
                potion_conservation: 1,
                ..zero
            },
            CombatOutcomeScore {
                faster_turns: 1,
                ..zero
            },
            CombatOutcomeScore {
                fewer_cards_played: 1,
                ..zero
            },
            CombatOutcomeScore {
                enemy_progress: 1,
                ..zero
            },
            CombatOutcomeScore {
                shorter_line: 1,
                ..zero
            },
        ];

        for left in scores {
            for right in scores {
                assert_eq!(
                    left.cmp(&right),
                    left.to_report_key().cmp(&right.to_report_key())
                );
            }
        }
    }

    #[test]
    fn external_burden_counts_actual_unblocked_curse_additions() {
        let mut combat = blank_test_combat();
        combat
            .meta
            .meta_changes
            .push(MetaChange::AddCardToMasterDeck(
                crate::test_support::test_master_deck_acquisition(CardId::Parasite, 999),
            ));
        combat
            .entities
            .player
            .relics
            .push(RelicState::new(RelicId::Omamori));

        assert_eq!(
            super::super::external_payoff::external_burden_count(&combat),
            1
        );

        combat
            .meta
            .meta_changes
            .push(MetaChange::AddCardToMasterDeck(
                crate::test_support::test_master_deck_acquisition(CardId::Pain, 999),
            ));
        combat
            .meta
            .meta_changes
            .push(MetaChange::AddCardToMasterDeck(
                crate::test_support::test_master_deck_acquisition(CardId::Doubt, 999),
            ));

        assert_eq!(
            super::super::external_payoff::external_burden_count(&combat),
            3
        );
    }

    impl SearchNode {
        fn test_node_with_hp(hp: i32) -> Self {
            let mut combat = blank_test_combat();
            combat.entities.player.current_hp = hp;
            SearchNode {
                engine: EngineState::CombatPlayerTurn,
                combat,
                actions: Vec::new(),
                turn_prefix: TurnPrefixState::default(),
                initial_hp: 80,
                potions_used: 0,
                potions_discarded: 0,
                cards_played: 0,
                potion_tactical_priority: 0,
                last_turn_branch_priority: 0,
                action_prior_score: None,
                action_ordering_frontier_hint: 0,
                rollout_estimate: RolloutNodeEstimate::unevaluated(),
                root_lineage: Default::default(),
            }
        }
    }
}
