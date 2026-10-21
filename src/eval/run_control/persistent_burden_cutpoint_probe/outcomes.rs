use crate::ai::combat_search_v2::{filter_combat_search_legal_actions, CombatSearchV2Config};
use crate::runtime::combat::CombatState;
use crate::sim::combat::{CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper};
use crate::sim::combat_action::CombatActionChoice;

use super::burden::{newly_gained_persistent_curses, PersistentCurseBurdenSnapshot};
use super::cutpoint::{GroupedBurdenCutpoint, LocatedBurdenCutpoint};
use super::{
    PersistentBurdenCutpointActionDomainV1, PersistentBurdenCutpointInputOutcomeKindV1,
    PersistentBurdenCutpointInputOutcomeV1, PersistentBurdenCutpointSummaryV1,
    PersistentBurdenEnemyPlanChangeV1,
};
use crate::eval::run_control::combat_candidate_line::enforce_replay_potion_budget;

pub(super) struct ProbedCutpointActions {
    pub(super) action_domain: PersistentBurdenCutpointActionDomainV1,
    pub(super) outcomes: Vec<PersistentBurdenCutpointInputOutcomeV1>,
}

pub(super) fn probe_cutpoint_actions(
    cutpoint: &LocatedBurdenCutpoint,
    config: &CombatSearchV2Config,
) -> ProbedCutpointActions {
    let surface = EngineCombatStepper.legal_action_surface(&cutpoint.position);
    if !surface.selection_families.is_empty() {
        return ProbedCutpointActions {
            action_domain:
                PersistentBurdenCutpointActionDomainV1::UnsupportedStructuredActionDomain {
                    selection_input_encodings: surface
                        .selection_families
                        .into_iter()
                        .map(|family| family.input_encoding)
                        .collect(),
                },
            outcomes: Vec::new(),
        };
    }

    let outcomes = enforce_replay_potion_budget(
        filter_combat_search_legal_actions(
            surface
                .atomic_actions
                .into_iter()
                .map(|input| CombatActionChoice::from_input(&cutpoint.position.combat, input))
                .collect(),
            config.potion_policy,
            config.allowed_potion_slots,
            &cutpoint.position.combat,
        ),
        config,
        cutpoint.potions_used_before,
    )
    .into_iter()
    .map(|choice| probe_one_action(cutpoint, config, choice))
    .collect();
    ProbedCutpointActions {
        action_domain: PersistentBurdenCutpointActionDomainV1::AtomicActionsComplete,
        outcomes,
    }
}

fn probe_one_action(
    cutpoint: &LocatedBurdenCutpoint,
    config: &CombatSearchV2Config,
    choice: CombatActionChoice,
) -> PersistentBurdenCutpointInputOutcomeV1 {
    let step = EngineCombatStepper.apply_to_stable(
        &cutpoint.position,
        choice.input.clone(),
        CombatStepLimits {
            max_engine_steps: config.max_engine_steps_per_action,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return failed_outcome(
            choice,
            format!(
                "one-action step truncated={} timed_out={} engine_steps={}",
                step.truncated, step.timed_out, step.engine_steps
            ),
        );
    }

    let before = PersistentCurseBurdenSnapshot::capture(&cutpoint.session);
    let mut trial = cutpoint.session.clone();
    if let Err(error) = trial.apply_input(choice.input.clone()) {
        return failed_outcome(choice, error);
    }
    let after = PersistentCurseBurdenSnapshot::capture(&trial);
    let gained_curse_counts = newly_gained_persistent_curses(&before, &after);
    let plan_changes = living_enemy_plan_changes(&cutpoint.position.combat, &step.position.combat);
    let kind = if step.terminal == CombatTerminal::Win && gained_curse_counts.is_empty() {
        PersistentBurdenCutpointInputOutcomeKindV1::CleanCombatVictory
    } else if !gained_curse_counts.is_empty() {
        PersistentBurdenCutpointInputOutcomeKindV1::NewCurse
    } else if !plan_changes.is_empty() {
        PersistentBurdenCutpointInputOutcomeKindV1::LivingEnemyPlanChanged
    } else {
        PersistentBurdenCutpointInputOutcomeKindV1::Neutral
    };
    PersistentBurdenCutpointInputOutcomeV1 {
        action_key: choice.action_key,
        input: choice.input,
        kind,
        terminal: step.terminal,
        gained_curse_counts,
        living_enemy_plan_changes: plan_changes,
        error: None,
    }
}

fn failed_outcome(
    choice: CombatActionChoice,
    error: String,
) -> PersistentBurdenCutpointInputOutcomeV1 {
    PersistentBurdenCutpointInputOutcomeV1 {
        action_key: choice.action_key,
        input: choice.input,
        kind: PersistentBurdenCutpointInputOutcomeKindV1::ApplyFailed,
        terminal: CombatTerminal::Unresolved,
        gained_curse_counts: Vec::new(),
        living_enemy_plan_changes: Vec::new(),
        error: Some(error),
    }
}

fn living_enemy_plan_changes(
    before: &CombatState,
    after: &CombatState,
) -> Vec<PersistentBurdenEnemyPlanChangeV1> {
    before
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .filter_map(|before_monster| {
            let after_monster = after
                .entities
                .monsters
                .iter()
                .find(|monster| monster.id == before_monster.id)?;
            if !after_monster.is_alive_for_action()
                || before_monster.planned_move_id() == after_monster.planned_move_id()
            {
                return None;
            }
            Some(PersistentBurdenEnemyPlanChangeV1 {
                entity_id: before_monster.id,
                enemy: before_monster.enemy_id.stable_id().to_owned(),
                before_plan_id: before_monster.planned_move_id(),
                after_plan_id: after_monster.planned_move_id(),
            })
        })
        .collect()
}

pub(super) fn probe_grouped_cutpoint(
    cutpoint: GroupedBurdenCutpoint,
    config: &CombatSearchV2Config,
) -> PersistentBurdenCutpointSummaryV1 {
    let action_probe = probe_cutpoint_actions(&cutpoint.representative, config);
    PersistentBurdenCutpointSummaryV1 {
        cutpoint_state_hash: cutpoint.representative.identity.state_hash.clone(),
        candidate_frequency: cutpoint.candidate_frequency,
        retained_indices: cutpoint.retained_indices,
        trigger_step_index: cutpoint.representative.trigger_step_index,
        trigger_action_key: cutpoint.representative.trigger_action_key.clone(),
        trigger_input: cutpoint.representative.trigger_input.clone(),
        trigger_gained_curse_counts: cutpoint.representative.trigger_gained_curse_counts.clone(),
        player_hp: cutpoint
            .representative
            .position
            .combat
            .entities
            .player
            .current_hp,
        player_block: cutpoint
            .representative
            .position
            .combat
            .entities
            .player
            .block,
        enemy_hp: cutpoint
            .representative
            .position
            .combat
            .entities
            .monsters
            .iter()
            .map(|monster| monster.current_hp)
            .collect(),
        action_domain: action_probe.action_domain,
        outcomes: action_probe.outcomes,
    }
}
