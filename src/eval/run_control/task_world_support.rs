use std::collections::BTreeSet;

use crate::agent::information::action::{
    resolve_public_combat_action_v1, CombatActionProjectionV1, PublicCombatActionChoiceV1,
};
use crate::agent::task::{
    task_public_combat_decision_v1, task_public_strategic_decision_v1, GameOutcomeV1,
    NativeReloadAnchorV1, TaskObjectiveV1, TaskPublicChoiceV1, TaskPublicDecisionV1,
};
use crate::ai::planner_core::PlannerAction;
use crate::content::events::neow::NeowEligibilityV1;
use crate::state::core::EngineState;
use crate::state::events::EventId;
use crate::state::selection::SelectionResolution;

use super::{
    LearningActionV1, LearningBoundaryV1, LearningEnvV3, LearningPreparedActionV3, RunControlConfig,
};

pub(super) fn natural_ironclad_learning_env_v1(
    seed: u64,
    ascension_level: u8,
) -> Result<LearningEnvV3, String> {
    natural_ironclad_learning_env_with_neow_v1(seed, ascension_level, NeowEligibilityV1::Rich)
}

pub(super) fn natural_ironclad_learning_env_with_neow_v1(
    seed: u64,
    ascension_level: u8,
    neow_eligibility: NeowEligibilityV1,
) -> Result<LearningEnvV3, String> {
    natural_ironclad_learning_env_with_neow_and_objective_v1(
        seed,
        ascension_level,
        neow_eligibility,
        TaskObjectiveV1::ActThreeVictory,
    )
}

pub(super) fn natural_ironclad_learning_env_with_neow_and_objective_v1(
    seed: u64,
    ascension_level: u8,
    neow_eligibility: NeowEligibilityV1,
    objective: TaskObjectiveV1,
) -> Result<LearningEnvV3, String> {
    natural_learning_env_v1(RunControlConfig {
        seed,
        ascension_level,
        final_act: objective == TaskObjectiveV1::HeartVictory,
        player_class: "Ironclad",
        neow_eligibility,
        ..RunControlConfig::default()
    })
}

pub(super) fn natural_learning_env_v1(
    mut config: RunControlConfig,
) -> Result<LearningEnvV3, String> {
    config.player_class = super::canonical_player_class(config.player_class)?;
    let player_class = config.player_class;
    let ascension_level = config.ascension_level;
    if ascension_level > 20 {
        return Err(format!(
            "natural {player_class} ascension level {ascension_level} is outside 0..=20"
        ));
    }
    let env = LearningEnvV3::new(config);
    let session = env.session();
    if session.run_state.act_num != 1
        || session.run_state.floor_num != 0
        || session.run_state.ascension_level != ascension_level
        || session.run_state.player_class != player_class
        || session.active_combat.is_some()
        || !matches!(session.engine_state, EngineState::EventRoom)
        || !session
            .run_state
            .event_state
            .as_ref()
            .is_some_and(|event| event.id == EventId::Neow)
    {
        return Err(format!(
            "task entry is not a natural {player_class} A{ascension_level} Neow root"
        ));
    }
    Ok(env)
}

pub(super) fn game_outcome_for_learning_terminal_v1(
    env: &LearningEnvV3,
    result: crate::state::core::RunResult,
) -> GameOutcomeV1 {
    match result {
        crate::state::core::RunResult::Victory if env.session().run_state.act_num >= 4 => {
            GameOutcomeV1::HeartVictory
        }
        crate::state::core::RunResult::Victory => GameOutcomeV1::ActThreeVictory,
        crate::state::core::RunResult::Defeat => GameOutcomeV1::Defeat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::GameOutcomeV1;
    use crate::eval::run_control::RunControlSession;
    use crate::state::core::RunResult;

    #[test]
    fn victory_identity_requires_reaching_act_four() {
        for (act_num, expected) in [
            (3, GameOutcomeV1::ActThreeVictory),
            (4, GameOutcomeV1::HeartVictory),
        ] {
            let mut session = RunControlSession::new(RunControlConfig::default());
            session.run_state.act_num = act_num;
            let env = LearningEnvV3::from_session(session);
            assert_eq!(
                game_outcome_for_learning_terminal_v1(&env, RunResult::Victory),
                expected
            );
        }
    }
}

pub(super) fn prepare_task_choice_v1(
    env: &LearningEnvV3,
    choice: &TaskPublicChoiceV1,
) -> Result<LearningPreparedActionV3, String> {
    let boundary = env.observe()?;
    let candidate_ordinal = choice
        .ordinal_path
        .first()
        .copied()
        .ok_or_else(|| "task choice path is empty".to_string())?;
    let action = match &boundary {
        LearningBoundaryV1::Strategic { boundary } => {
            let candidate = boundary
                .legal_candidates
                .candidates
                .get(candidate_ordinal)
                .ok_or_else(|| "strategic task candidate ordinal is out of range".to_string())?;
            match &candidate.action {
                PlannerAction::BeginRunCardSelection {
                    scope,
                    min_choices,
                    max_choices,
                    selectable_card_lineages,
                    ..
                } => {
                    let selected_domain_ordinals = &choice.ordinal_path[1..];
                    if selected_domain_ordinals.len() < *min_choices
                        || selected_domain_ordinals.len() > *max_choices
                    {
                        return Err(
                            "run selection payload is outside its declared bounds".to_string()
                        );
                    }
                    let mut unique = BTreeSet::new();
                    let selected = selected_domain_ordinals
                        .iter()
                        .copied()
                        .map(|ordinal| {
                            if !unique.insert(ordinal) {
                                return Err(
                                    "run selection repeats one public domain ordinal".to_string()
                                );
                            }
                            selectable_card_lineages
                                .get(ordinal)
                                .copied()
                                .ok_or_else(|| {
                                    "run selection domain ordinal is out of range".to_string()
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    LearningActionV1::RunSelection {
                        candidate_id: candidate.candidate_id.clone(),
                        resolution: SelectionResolution::card_lineages(*scope, selected),
                    }
                }
                _ => {
                    require_atomic_choice(choice)?;
                    LearningActionV1::StrategicCandidate {
                        candidate_id: candidate.candidate_id.clone(),
                    }
                }
            }
        }
        LearningBoundaryV1::Combat { boundary } => {
            let atomic_count = boundary.public_actions.atomic_actions.len();
            let input = if candidate_ordinal < atomic_count {
                require_atomic_choice(choice)?;
                boundary
                    .private_resolution
                    .atomic_inputs
                    .get(candidate_ordinal)
                    .cloned()
                    .ok_or_else(|| {
                        "combat public/private atomic surfaces are misaligned".to_string()
                    })?
            } else {
                let family_ordinal = candidate_ordinal - atomic_count;
                let projection = CombatActionProjectionV1 {
                    public: boundary.public_actions.clone(),
                    private_resolution: boundary.private_resolution.clone(),
                };
                resolve_public_combat_action_v1(
                    &projection,
                    &PublicCombatActionChoiceV1::Selection {
                        family_ordinal,
                        selected_domain_ordinals: choice.ordinal_path[1..].to_vec(),
                    },
                )
                .map_err(|error| format!("cannot resolve public combat choice: {error}"))?
            };
            LearningActionV1::CombatInput { input }
        }
        LearningBoundaryV1::Terminal { .. } => {
            return Err("task world is already terminal".to_string())
        }
        LearningBoundaryV1::Unsupported => {
            return Err("task world reached an unsupported boundary".to_string())
        }
    };
    env.prepare_action(action)
}

pub(super) fn task_public_decision_v1(env: &LearningEnvV3) -> Result<TaskPublicDecisionV1, String> {
    match env.observe()? {
        LearningBoundaryV1::Strategic { boundary } => {
            task_public_strategic_decision_v1(&boundary.observation, &boundary.legal_candidates)
        }
        LearningBoundaryV1::Combat { boundary } => task_public_combat_decision_v1(
            boundary.public_run_context,
            boundary.observation,
            &boundary.public_actions,
        ),
        LearningBoundaryV1::Terminal { .. } => {
            Err("terminal task state has no public decision".to_string())
        }
        LearningBoundaryV1::Unsupported => {
            Err("unsupported task state has no public decision".to_string())
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AutosaveBoundaryFactsV1 {
    act: u8,
    floor: i32,
    completed_combat_sequence: Option<u64>,
    neow_active: bool,
    boss_relic_select: bool,
}

impl AutosaveBoundaryFactsV1 {
    pub(super) fn capture(env: &LearningEnvV3) -> Self {
        let session = env.session();
        Self {
            act: session.run_state.act_num,
            floor: session.run_state.floor_num,
            completed_combat_sequence: session
                .recent_combat_attrition()
                .map(|fact| fact.combat_sequence),
            neow_active: session
                .run_state
                .event_state
                .as_ref()
                .is_some_and(|event| event.id == EventId::Neow)
                && !matches!(
                    session.engine_state,
                    EngineState::MapNavigation | EngineState::MapOverlay { .. }
                ),
            boss_relic_select: matches!(session.engine_state, EngineState::BossRelicSelect(_)),
        }
    }
}

pub(super) fn next_autosave_generation_v1(
    current: NativeReloadAnchorV1,
    before: AutosaveBoundaryFactsV1,
    after: AutosaveBoundaryFactsV1,
) -> Option<NativeReloadAnchorV1> {
    if before.completed_combat_sequence != after.completed_combat_sequence
        && after.completed_combat_sequence.is_some()
    {
        return Some(NativeReloadAnchorV1::PostCombatAutosave);
    }
    if current == NativeReloadAnchorV1::EnterRoomAutosave
        && before.neow_active
        && !after.neow_active
    {
        return Some(NativeReloadAnchorV1::PostNeowAutosave);
    }
    if after.floor > before.floor
        || (after.boss_relic_select && !before.boss_relic_select)
        || after.act > before.act && after.floor != before.floor
    {
        return Some(NativeReloadAnchorV1::EnterRoomAutosave);
    }
    None
}

fn require_atomic_choice(choice: &TaskPublicChoiceV1) -> Result<(), String> {
    if choice.ordinal_path.len() != 1 {
        return Err("atomic task candidate received an autoregressive payload".to_string());
    }
    Ok(())
}
