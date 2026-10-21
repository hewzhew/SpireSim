use super::super::frontier::{
    remember_best_complete, remember_best_frontier, remember_win_candidate, RetainedWinCandidate,
    SearchNode,
};
use super::super::*;
use super::win_acceptance::accepted_complete_win;

#[derive(Clone)]
pub(super) struct SearchTrajectoryBook {
    root: std::sync::Arc<CombatPosition>,
    pub(super) best_complete: Option<SearchNode>,
    pub(super) best_win: Option<SearchNode>,
    pub(super) win_candidates: Vec<RetainedWinCandidate>,
    pub(super) win_frontier_revision: u64,
    pub(super) best_frontier: Option<SearchNode>,
}

impl SearchTrajectoryBook {
    pub(super) fn new(root: CombatPosition) -> Self {
        Self::with_root(std::sync::Arc::new(root))
    }

    fn with_root(root: std::sync::Arc<CombatPosition>) -> Self {
        Self {
            root,
            best_complete: None,
            best_win: None,
            win_candidates: Vec::new(),
            win_frontier_revision: 0,
            best_frontier: None,
        }
    }

    pub(super) fn empty_at_same_root(&self) -> Self {
        Self::with_root(self.root.clone())
    }

    pub(super) fn contains_exact_win_path(&self, node: &SearchNode) -> bool {
        self.win_candidates
            .iter()
            .map(|candidate| &candidate.node)
            .chain(self.best_win.iter())
            .any(|observed| same_action_path(observed, node))
    }

    pub(super) fn remember_best_frontier(&mut self, node: &SearchNode) {
        remember_best_frontier(&mut self.best_frontier, node);
    }

    pub(super) fn remember_complete(&mut self, node: SearchNode) {
        remember_best_complete(&mut self.best_complete, node);
    }

    pub(super) fn remember_win(
        &mut self,
        node: SearchNode,
        config: &CombatSearchV2Config,
        initial_external_burden_count: i32,
    ) -> bool {
        let candidate_satisfied =
            accepted_complete_win(&node, config, initial_external_burden_count);
        if remember_win_candidate(&self.root, &mut self.win_candidates, &node) {
            self.win_frontier_revision = self.win_frontier_revision.saturating_add(1);
        }
        remember_best_complete(&mut self.best_win, node.clone());
        remember_best_complete(&mut self.best_complete, node);
        candidate_satisfied
    }
}

fn same_action_path(left: &SearchNode, right: &SearchNode) -> bool {
    left.actions.len() == right.actions.len()
        && left
            .actions
            .iter()
            .zip(&right.actions)
            .all(|(left, right)| left.action_key == right.action_key && left.input == right.input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::RunResult;
    use crate::test_support::blank_test_combat;

    #[test]
    fn any_new_exact_candidate_can_satisfy_the_session_objective() {
        let mut rich_combat = blank_test_combat();
        rich_combat.entities.player.current_hp = 80;
        let mut dagger = CombatCard::for_scenario(CardId::RitualDagger, 41);
        dagger.misc_value = 50;
        rich_combat.meta.master_deck_snapshot = vec![dagger].into();
        let mut rich = SearchNode::root(EngineState::GameOver(RunResult::Victory), rich_combat);
        rich.combat.entities.player.current_hp = 60;

        let mut healthy_combat = blank_test_combat();
        healthy_combat.entities.player.current_hp = 80;
        let mut healthy =
            SearchNode::root(EngineState::GameOver(RunResult::Victory), healthy_combat);
        healthy.combat.entities.player.current_hp = 70;

        let config = CombatSearchV2Config {
            satisfaction: CombatSearchV2Satisfaction::HpLossAtMost(10),
            ..CombatSearchV2Config::default()
        };
        let mut book = SearchTrajectoryBook::new(CombatPosition::new(
            EngineState::CombatPlayerTurn,
            blank_test_combat(),
        ));

        assert!(!book.remember_win(rich, &config, 0));
        assert!(book.remember_win(healthy, &config, 0));
        assert_eq!(
            book.best_win
                .as_ref()
                .expect("persistent-payoff win remains raw best")
                .combat
                .entities
                .player
                .current_hp,
            60
        );
    }
}
