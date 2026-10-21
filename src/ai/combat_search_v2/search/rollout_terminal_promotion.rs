use super::super::*;
use super::loop_state::SearchLoopState;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RolloutPromotionOutcome {
    Unchanged,
    Promoted,
    ReplayInterrupted,
}

pub(super) fn promote_replayable_terminal_rollout(
    loop_state: &mut SearchLoopState,
    root: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) -> RolloutPromotionOutcome {
    let witness = match config.satisfaction {
        CombatSearchV2Satisfaction::PotionFreeHpLossAtMostWithoutNewExternalBurden(_) => loop_state
            .rollout_cache
            .best_replayable_terminal_potion_free_win_without_new_external_burden
            .clone(),
        CombatSearchV2Satisfaction::FirstCompleteWinWithoutNewExternalBurden
        | CombatSearchV2Satisfaction::HpLossAtMostWithoutNewExternalBurden(_) => loop_state
            .rollout_cache
            .best_replayable_terminal_win_without_new_external_burden
            .clone(),
        CombatSearchV2Satisfaction::BudgetOrExhaustion
        | CombatSearchV2Satisfaction::ZeroLossOrBudget
        | CombatSearchV2Satisfaction::FirstCompleteWin
        | CombatSearchV2Satisfaction::HpLossAtMost(_)
        | CombatSearchV2Satisfaction::MaterializedPersistentPayoffGain => loop_state
            .rollout_cache
            .best_replayable_terminal_win
            .clone(),
    };
    let Some(witness) = witness else {
        return RolloutPromotionOutcome::Unchanged;
    };
    if loop_state.last_promoted_rollout_witness.as_ref() == Some(&witness.estimate) {
        return RolloutPromotionOutcome::Unchanged;
    }
    let mut replayed_actions = 0;
    let replay = replay_terminal_witness(
        root,
        &witness.estimate,
        stepper,
        config,
        deadline,
        &mut replayed_actions,
    );
    loop_state.performance.rollout_promotion_actions_replayed = loop_state
        .performance
        .rollout_promotion_actions_replayed
        .saturating_add(replayed_actions);
    let mut node = match replay {
        TerminalWitnessReplay::Complete(node) => node,
        TerminalWitnessReplay::Interrupted => {
            return RolloutPromotionOutcome::ReplayInterrupted;
        }
        TerminalWitnessReplay::Invalid => {
            loop_state.last_promoted_rollout_witness = Some(witness.estimate);
            return RolloutPromotionOutcome::Unchanged;
        }
    };
    loop_state.last_promoted_rollout_witness = Some(witness.estimate);
    loop_state.materialize_root_lineage(&mut node);

    if loop_state.remember_promoted_win_observed_at(
        node,
        config,
        witness.nodes_generated_at_discovery,
    ) {
        loop_state.mark_accepted_complete_candidate();
    }
    RolloutPromotionOutcome::Promoted
}

enum TerminalWitnessReplay {
    Complete(SearchNode),
    Interrupted,
    Invalid,
}

fn replay_terminal_witness(
    root: &SearchNode,
    witness: &RolloutNodeEstimate,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    replayed_actions: &mut u64,
) -> TerminalWitnessReplay {
    if !witness.is_replayable_terminal_win() {
        return TerminalWitnessReplay::Invalid;
    }

    let mut node = root.clone();
    node.rollout_estimate = RolloutNodeEstimate::unevaluated();
    for action in &witness.action_preview {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            return TerminalWitnessReplay::Interrupted;
        }
        if terminal_label(&node.engine, &node.combat) != SearchTerminalLabel::Unresolved {
            return TerminalWitnessReplay::Invalid;
        }
        let position = CombatPosition::new(node.engine.clone(), node.combat.clone());
        let original_action_id = filtered_legal_actions(
            stepper.atomic_action_choices(&position),
            config.potion_policy,
            config.allowed_potion_slots,
            config.allow_potion_discard,
            &node.combat,
        )
        .iter()
        .position(|choice| choice.input == action.input && choice.action_key == action.action_key);
        let Some(candidate) = stepper.choice_for_legal_input(&position, &action.input) else {
            return TerminalWitnessReplay::Invalid;
        };
        let choice = filtered_legal_actions(
            vec![candidate],
            config.potion_policy,
            config.allowed_potion_slots,
            config.allow_potion_discard,
            &node.combat,
        )
        .into_iter()
        .find(|choice| choice.input == action.input && choice.action_key == action.action_key);
        let Some(choice) = choice else {
            return TerminalWitnessReplay::Invalid;
        };
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            return TerminalWitnessReplay::Interrupted;
        }
        *replayed_actions = (*replayed_actions).saturating_add(1);
        let step = stepper.apply_to_stable(
            &position,
            choice.input.clone(),
            CombatStepLimits {
                max_engine_steps: config.max_engine_steps_per_action,
                deadline,
            },
        );
        if step.timed_out || deadline.is_some_and(|limit| Instant::now() >= limit) {
            return TerminalWitnessReplay::Interrupted;
        }
        if step.truncated {
            return TerminalWitnessReplay::Invalid;
        }

        let mut child = node.clone_for_child(step.position.engine, step.position.combat);
        let transition = classify_turn_branch_transition(
            &node.engine,
            &node.combat,
            &choice.input,
            &child.engine,
            &child.combat,
        );
        child.note_turn_prefix(&node.combat, &choice.input, transition);
        child.note_input(&choice.input);
        child.note_turn_branch_priority(transition.frontier_priority_hint());
        child.push_action(CombatSearchV2ActionTrace {
            step_index: node.actions.len(),
            action_id: original_action_id.unwrap_or(node.actions.len()),
            action_key: choice.action_key,
            action_debug: choice.action_debug,
            input: choice.input,
        });
        node = child;
    }

    if exact_terminal_matches_witness(&node, witness) {
        TerminalWitnessReplay::Complete(node)
    } else {
        TerminalWitnessReplay::Invalid
    }
}

fn exact_terminal_matches_witness(node: &SearchNode, witness: &RolloutNodeEstimate) -> bool {
    let phase_profile = combat_search_phase_profile(&node.engine, &node.combat);
    terminal_label(&node.engine, &node.combat) == SearchTerminalLabel::Win
        && witness.terminal == SearchTerminalLabel::Win
        && node.actions.len() == witness.total_actions
        && node.combat.entities.player.current_hp == witness.final_hp
        && super::super::outcome_score::external_burden_count(&node.combat)
            == witness.external_burden_count
        && node.combat.turn.turn_count == witness.turns
        && node.potions_used == witness.potions_used
        && node.potions_discarded == witness.potions_discarded
        && node.cards_played == witness.cards_played
        && living_enemy_count(&node.combat) == witness.living_enemy_count
        && phase_profile.enemy_phase.raw_living_enemy_hp == witness.total_enemy_hp
        && phase_profile.enemy_phase.raw_living_enemy_block == witness.total_enemy_block
}
