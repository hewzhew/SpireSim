use blake2::{Blake2b512, Digest};

use crate::ai::combat_search_v2::{
    filter_combat_search_legal_actions, CombatSearchV2Config, CombatSearchV2Report,
    CombatSearchV2TrajectoryReport,
};
use crate::ai::combat_state_key::combat_exact_state_hash_v2;
use crate::sim::combat::{CombatPosition, CombatStepper, EngineCombatStepper};
use crate::state::core::ClientInput;

use super::super::combat_candidate_line::enforce_replay_potion_budget;
use super::super::combat_case_candidate_census::CombatCaseCandidateReplayFailureV1;
use super::super::combat_case_retained_candidates::unique_retained_win_trajectories;
use super::super::session::RunControlSession;
use super::burden::{newly_gained_persistent_curses, PersistentCurseBurdenSnapshot};
use super::PersistentBurdenGainedCurseCountV1;

pub(super) struct LocatedBurdenCutpoint {
    pub(super) retained_index: usize,
    pub(super) trigger_step_index: usize,
    pub(super) trigger_action_key: String,
    pub(super) trigger_input: ClientInput,
    pub(super) trigger_gained_curse_counts: Vec<PersistentBurdenGainedCurseCountV1>,
    pub(super) potions_used_before: u32,
    pub(super) identity: BurdenCutpointIdentity,
    pub(super) session: RunControlSession,
    pub(super) position: CombatPosition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BurdenCutpointIdentity {
    pub(super) state_hash: String,
    pub(super) canonical: String,
}

pub(super) fn locate_candidate_cutpoint(
    base_session: &RunControlSession,
    config: &CombatSearchV2Config,
    retained_index: usize,
    trajectory: &CombatSearchV2TrajectoryReport,
) -> Result<Option<LocatedBurdenCutpoint>, String> {
    let mut trial = base_session.clone();
    trial.mark_current_combat_search_resolved();
    let mut potions_used = 0u32;

    for action in &trajectory.actions {
        let position = trial.current_active_combat_position()?;
        let Some(candidate) = EngineCombatStepper.choice_for_legal_input(&position, &action.input)
        else {
            return Err(format!(
                "persistent burden cutpoint replay drift at step {}: expected {}",
                action.step_index, action.action_key
            ));
        };
        let choices = enforce_replay_potion_budget(
            filter_combat_search_legal_actions(
                vec![candidate],
                config.potion_policy,
                config.allowed_potion_slots,
                &position.combat,
            ),
            config,
            potions_used,
        );
        let Some(choice) = choices
            .into_iter()
            .find(|choice| choice.input == action.input && choice.action_key == action.action_key)
        else {
            return Err(format!(
                "persistent burden cutpoint replay drift at step {}: expected {}",
                action.step_index, action.action_key
            ));
        };

        let before = PersistentCurseBurdenSnapshot::capture(&trial);
        let clean_session = trial.clone();
        trial.apply_input(choice.input.clone())?;
        let after = PersistentCurseBurdenSnapshot::capture(&trial);
        let gained = newly_gained_persistent_curses(&before, &after);
        if !gained.is_empty() {
            let identity = cutpoint_identity(&clean_session, &position);
            return Ok(Some(LocatedBurdenCutpoint {
                retained_index,
                trigger_step_index: action.step_index,
                trigger_action_key: action.action_key.clone(),
                trigger_input: action.input.clone(),
                trigger_gained_curse_counts: gained,
                potions_used_before: potions_used,
                identity,
                session: clean_session,
                position,
            }));
        }
        if matches!(choice.input, ClientInput::UsePotion { .. }) {
            potions_used = potions_used.saturating_add(1);
        }
    }
    Ok(None)
}

pub(super) fn cutpoint_identity(
    session: &RunControlSession,
    position: &CombatPosition,
) -> BurdenCutpointIdentity {
    let run = &session.run_state;
    let combat_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
    let rng_counters = [
        run.rng_pool.monster_rng.counter,
        run.rng_pool.event_rng.counter,
        run.rng_pool.merchant_rng.counter,
        run.rng_pool.card_rng.counter,
        run.rng_pool.treasure_rng.counter,
        run.rng_pool.relic_rng.counter,
        run.rng_pool.potion_rng.counter,
        run.rng_pool.monster_hp_rng.counter,
        run.rng_pool.ai_rng.counter,
        run.rng_pool.shuffle_rng.counter,
        run.rng_pool.card_random_rng.counter,
        run.rng_pool.misc_rng.counter,
        run.rng_pool.math_rng.counter,
    ];
    let neow_rng_counter = run.neow_rng.as_ref().map(|rng| rng.counter);
    let canonical = format!(
        "{:?}",
        (
            combat_hash,
            run.current_hp,
            run.max_hp,
            run.gold,
            &run.master_deck,
            &run.relics,
            &run.potions,
            rng_counters,
            neow_rng_counter,
        )
    );
    let mut hasher = Blake2b512::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    let state_hash = digest[..32]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    BurdenCutpointIdentity {
        state_hash,
        canonical,
    }
}

pub(super) struct GroupedBurdenCutpoint {
    pub(super) representative: LocatedBurdenCutpoint,
    pub(super) candidate_frequency: usize,
    pub(super) retained_indices: Vec<usize>,
}

pub(super) fn group_cutpoints(located: Vec<LocatedBurdenCutpoint>) -> Vec<GroupedBurdenCutpoint> {
    let mut grouped = Vec::<GroupedBurdenCutpoint>::new();
    for cutpoint in located {
        if let Some(existing) = grouped.iter_mut().find(|existing| {
            existing.representative.identity.canonical == cutpoint.identity.canonical
        }) {
            existing.candidate_frequency += 1;
            existing.retained_indices.push(cutpoint.retained_index);
        } else {
            let retained_index = cutpoint.retained_index;
            grouped.push(GroupedBurdenCutpoint {
                representative: cutpoint,
                candidate_frequency: 1,
                retained_indices: vec![retained_index],
            });
        }
    }
    grouped
}

pub(super) struct CutpointLocationReport {
    pub(super) retained_candidate_count: usize,
    pub(super) unique_candidate_count: usize,
    pub(super) dirty_candidate_count: usize,
    pub(super) grouped: Vec<GroupedBurdenCutpoint>,
    pub(super) replay_failures: Vec<CombatCaseCandidateReplayFailureV1>,
}

pub(super) fn locate_and_group_cutpoints(
    base_session: &RunControlSession,
    config: &CombatSearchV2Config,
    report: &CombatSearchV2Report,
) -> CutpointLocationReport {
    let retained = unique_retained_win_trajectories(report);
    let retained_candidate_count = retained.retained_candidate_count;
    let unique_candidate_count = retained.trajectories.len();
    let mut located = Vec::new();
    let mut replay_failures = Vec::new();

    for candidate in retained.trajectories {
        match locate_candidate_cutpoint(
            base_session,
            config,
            candidate.retained_index,
            candidate.trajectory,
        ) {
            Ok(Some(cutpoint)) => located.push(cutpoint),
            Ok(None) => {}
            Err(error) => replay_failures.push(CombatCaseCandidateReplayFailureV1 {
                retained_index: candidate.retained_index,
                action_count: candidate.trajectory.actions.len(),
                error,
            }),
        }
    }
    let dirty_candidate_count = located.len();
    CutpointLocationReport {
        retained_candidate_count,
        unique_candidate_count,
        dirty_candidate_count,
        grouped: group_cutpoints(located),
        replay_failures,
    }
}
