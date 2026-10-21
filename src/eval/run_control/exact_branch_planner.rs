use crate::agent::task::{
    GameOutcomeV1, TaskExactBranchSelectionControllerV1, TaskExactBranchTrialEndV1,
    TaskExactBranchTrialV1,
};

/// Combat-local ordering for independently complete actor trials.
///
/// This controller deliberately knows nothing about how a trial chose its
/// actions. In particular, it does not append a legality bootstrap to a
/// partial combat plan and does not invent a scalar exchange rate for HP,
/// potions, or later run value.
#[derive(Clone, Copy, Debug)]
pub struct CombatOutcomeFirstControllerV1;

impl TaskExactBranchSelectionControllerV1 for CombatOutcomeFirstControllerV1 {
    fn select_trial(&mut self, trials: &[TaskExactBranchTrialV1]) -> Result<usize, String> {
        if trials.is_empty() {
            return Err("exact branch controller received no trials".to_string());
        }

        fn rank(trial: &TaskExactBranchTrialV1) -> u8 {
            match &trial.end {
                TaskExactBranchTrialEndV1::ExploredScopeExit {
                    exit:
                        crate::agent::task::TaskExploredScopeExitV1::GameTerminal {
                            game_outcome:
                                GameOutcomeV1::ActThreeVictory | GameOutcomeV1::HeartVictory,
                        },
                } => 3,
                TaskExactBranchTrialEndV1::Decision { decision }
                | TaskExactBranchTrialEndV1::ControllerStopped { decision }
                    if decision.site != crate::agent::task::TaskDecisionSiteV1::Combat =>
                {
                    2
                }
                TaskExactBranchTrialEndV1::ExploredScopeExit {
                    exit: crate::agent::task::TaskExploredScopeExitV1::NextScope { .. },
                } => 2,
                TaskExactBranchTrialEndV1::Decision { .. }
                | TaskExactBranchTrialEndV1::ControllerStopped { .. } => 1,
                TaskExactBranchTrialEndV1::ExploredScopeExit {
                    exit:
                        crate::agent::task::TaskExploredScopeExitV1::GameTerminal {
                            game_outcome: GameOutcomeV1::Defeat,
                        },
                } => 0,
            }
        }

        let mut selected = 0usize;
        for index in 1..trials.len() {
            if rank(&trials[index]) > rank(&trials[selected]) {
                selected = index;
            }
        }
        Ok(selected)
    }
}
