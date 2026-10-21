use serde::{Deserialize, Serialize};

use crate::ai::planner_core::PlannerPlayerClass;

use super::{ReloadProtocolV1, TaskDecisionSiteV1};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskObjectiveV1 {
    ActThreeVictory,
    HeartVictory,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskInitialConditionV1 {
    NaturalNewGameEntrance,
    ExactRunDecision {
        source_seed: u64,
        act: u8,
        floor: i32,
        root_exact_state_hash: String,
        source_session_id: String,
        source_head_revision: u64,
        source_archive_node_id: String,
        source_public_snapshot_id: String,
    },
    ExactProductionCombatCase {
        source_seed: u64,
        act: u8,
        floor: i32,
        root_exact_state_hash: String,
        run_session_fingerprint: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskInformationAccessV1 {
    PublicHistoryAndLegalReloadMemory,
    PublicHistoryLegalReloadMemoryAndPrivilegedSupport,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSeedDisclosureV1 {
    HiddenFromActor,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSpecV1 {
    pub player_class: PlannerPlayerClass,
    pub ascension_level: u8,
    pub objective: TaskObjectiveV1,
    pub initial_condition: TaskInitialConditionV1,
    pub information_access: TaskInformationAccessV1,
    pub seed_disclosure: TaskSeedDisclosureV1,
    pub reload_protocol: ReloadProtocolV1,
}

impl TaskSpecV1 {
    pub fn ironclad_a0_act_three_fail_closed() -> Self {
        Self {
            player_class: PlannerPlayerClass::Ironclad,
            ascension_level: 0,
            objective: TaskObjectiveV1::ActThreeVictory,
            initial_condition: TaskInitialConditionV1::NaturalNewGameEntrance,
            information_access: TaskInformationAccessV1::PublicHistoryAndLegalReloadMemory,
            seed_disclosure: TaskSeedDisclosureV1::HiddenFromActor,
            reload_protocol: ReloadProtocolV1::fail_closed(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.ascension_level > 20 {
            return Err(format!(
                "task ascension level {} is outside 0..=20",
                self.ascension_level
            ));
        }
        match &self.initial_condition {
            TaskInitialConditionV1::NaturalNewGameEntrance => {}
            TaskInitialConditionV1::ExactRunDecision {
                act,
                floor,
                root_exact_state_hash,
                source_session_id,
                source_archive_node_id,
                source_public_snapshot_id,
                ..
            } => {
                validate_exact_run_location(*act, *floor, "exact run decision")?;
                if [
                    root_exact_state_hash.as_str(),
                    source_session_id.as_str(),
                    source_archive_node_id.as_str(),
                    source_public_snapshot_id.as_str(),
                ]
                .iter()
                .any(|value| value.trim().is_empty())
                {
                    return Err(
                        "exact run decision requires non-empty state and source identities"
                            .to_string(),
                    );
                }
            }
            TaskInitialConditionV1::ExactProductionCombatCase {
                act,
                floor,
                root_exact_state_hash,
                run_session_fingerprint,
                ..
            } => {
                validate_exact_run_location(*act, *floor, "exact production combat case")?;
                if root_exact_state_hash.trim().is_empty()
                    || run_session_fingerprint.trim().is_empty()
                {
                    return Err(
                        "exact production combat case requires non-empty exact identities"
                            .to_string(),
                    );
                }
            }
        }
        self.reload_protocol.validate()
    }

    pub fn is_complete_run_task(&self) -> bool {
        matches!(
            self.initial_condition,
            TaskInitialConditionV1::NaturalNewGameEntrance
                | TaskInitialConditionV1::ExactRunDecision { .. }
        )
    }
}

fn validate_exact_run_location(act: u8, floor: i32, description: &str) -> Result<(), String> {
    if !(1..=4).contains(&act) {
        return Err(format!("{description} act {act} is outside 1..=4"));
    }
    if floor < 0 {
        return Err(format!("{description} floor {floor} must be non-negative"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskBudgetV1 {
    pub max_world_transitions: u64,
    pub max_reloads: u32,
}

impl TaskBudgetV1 {
    pub fn validate(self) -> Result<(), String> {
        if self.max_world_transitions == 0 {
            return Err("task transition budget must be positive".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskBudgetStateV1 {
    pub remaining_world_transitions: u64,
    pub remaining_reloads: u32,
}

impl From<TaskBudgetV1> for TaskBudgetStateV1 {
    fn from(value: TaskBudgetV1) -> Self {
        Self {
            remaining_world_transitions: value.max_world_transitions,
            remaining_reloads: value.max_reloads,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GameOutcomeV1 {
    ActThreeVictory,
    HeartVictory,
    Defeat,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskEpisodeOutcomeV1 {
    Success {
        game_outcome: GameOutcomeV1,
    },
    Paused {
        reason: TaskEpisodePauseV1,
    },
    Failure {
        reason: TaskEpisodeFailureV1,
        game_outcome: Option<GameOutcomeV1>,
    },
}

/// A non-terminal controller handoff. A paused episode has reached a real,
/// replayable public boundary; it is neither a task failure nor evidence that
/// any continuation wins or loses.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskEpisodePauseV1 {
    PublicDecisionControlYield { site: TaskDecisionSiteV1 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskEpisodeFailureV1 {
    CommittedDefeat,
    CommittedTerminalWithoutObjective,
    RetryBudgetExhausted,
    TransitionBudgetExhausted,
    UnsupportedDecisionSurface,
    MechanicsOrProtocolError { message: String },
}

pub(super) fn outcome_for_game_terminal_v1(
    objective: TaskObjectiveV1,
    game_outcome: GameOutcomeV1,
) -> TaskEpisodeOutcomeV1 {
    let success = matches!(
        (objective, game_outcome),
        (
            TaskObjectiveV1::ActThreeVictory,
            GameOutcomeV1::ActThreeVictory | GameOutcomeV1::HeartVictory
        ) | (TaskObjectiveV1::HeartVictory, GameOutcomeV1::HeartVictory)
    );
    if success {
        TaskEpisodeOutcomeV1::Success { game_outcome }
    } else {
        TaskEpisodeOutcomeV1::Failure {
            reason: match game_outcome {
                GameOutcomeV1::Defeat => TaskEpisodeFailureV1::CommittedDefeat,
                GameOutcomeV1::ActThreeVictory | GameOutcomeV1::HeartVictory => {
                    TaskEpisodeFailureV1::CommittedTerminalWithoutObjective
                }
            },
            game_outcome: Some(game_outcome),
        }
    }
}
