use std::collections::{BTreeMap, BTreeSet};

use crate::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper};
use crate::sim::combat_action::CombatActionChoice;

use super::super::super::frontier::SearchNode;
use super::super::super::pending_choice_action_prefix::PendingChoiceActionFamily;
use super::super::super::pending_choice_fanout::pending_choice_fanout;
use super::super::super::plugins::CombatSearchPluginStack;
use super::super::super::state_abstraction::StateDivergenceKind;
use super::super::super::transition::filtered_legal_actions;
use super::super::super::turn_branching::classify_turn_branch_transition;
use super::super::super::turn_sequence_effect::{
    effect_fingerprint, effect_key, TurnSequenceDivergence, TurnSequenceEffectAggregate,
    TurnSequenceEffectFingerprint,
};
use super::super::super::types::{CombatSearchV2ActionTrace, CombatSearchV2Config};
use super::super::is_static_discard_order_candidate;
use super::types::{
    DiscardOrderShadowAuditExactGroupResult, DiscardOrderShadowAuditGroup,
    EXACT_SHADOW_ACTIONS_PER_GROUP,
};

pub(super) fn audit_group_one_step(
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    plugins: &CombatSearchPluginStack,
    group: &DiscardOrderShadowAuditGroup,
) -> Option<DiscardOrderShadowAuditExactGroupResult> {
    let [left, right] = group.representatives.as_slice() else {
        return None;
    };

    let initial_divergence = classify_pair(&left.effect_fingerprint, &right.effect_fingerprint);
    if left.effect_key == right.effect_key
        || !is_static_discard_order_candidate(
            initial_divergence.kind,
            initial_divergence.first_divergence_path,
            initial_divergence.guessed_reveal_gate,
        )
    {
        return None;
    }

    if combinatorial_pending_choice_owned(stepper, &left.node)
        || combinatorial_pending_choice_owned(stepper, &right.node)
    {
        return Some(DiscardOrderShadowAuditExactGroupResult {
            status: "skipped_combinatorial_pending_choice",
            checked_actions: 0,
            verified_actions: 0,
            blocked_actions: 0,
            blocking_action_key: None,
            blocking_divergence_kind: None,
            blocking_path: None,
        });
    }

    let left_actions = legal_action_map(stepper, plugins, &left.node);
    let right_actions = legal_action_map(stepper, plugins, &right.node);
    let left_keys = left_actions.keys().cloned().collect::<BTreeSet<_>>();
    let right_keys = right_actions.keys().cloned().collect::<BTreeSet<_>>();
    if left_keys != right_keys {
        return Some(DiscardOrderShadowAuditExactGroupResult {
            status: "blocked_legal_action_set_delta",
            checked_actions: 0,
            verified_actions: 0,
            blocked_actions: 1,
            blocking_action_key: None,
            blocking_divergence_kind: Some(StateDivergenceKind::LegalActionDelta),
            blocking_path: Some("combat.legal_actions.action_key_set"),
        });
    }

    let mut result = DiscardOrderShadowAuditExactGroupResult {
        status: "sample_verified_one_step",
        checked_actions: 0,
        verified_actions: 0,
        blocked_actions: 0,
        blocking_action_key: None,
        blocking_divergence_kind: None,
        blocking_path: None,
    };

    for action_key in left_keys.into_iter().take(EXACT_SHADOW_ACTIONS_PER_GROUP) {
        let left_choice = left_actions
            .get(&action_key)
            .expect("key collected from left action map");
        let right_choice = right_actions
            .get(&action_key)
            .expect("matching key checked before action audit");
        result.checked_actions += 1;

        let left_child = one_step_effect(stepper, config, plugins, &left.node, left_choice);
        let right_child = one_step_effect(stepper, config, plugins, &right.node, right_choice);
        let action_status = match (left_child, right_child) {
            (Ok(left_fingerprint), Ok(right_fingerprint)) => {
                let divergence = classify_pair(&left_fingerprint, &right_fingerprint);
                if effect_key(&left_fingerprint) == effect_key(&right_fingerprint)
                    || is_static_discard_order_candidate(
                        divergence.kind,
                        divergence.first_divergence_path,
                        divergence.guessed_reveal_gate,
                    )
                {
                    Ok(())
                } else {
                    Err((
                        divergence.kind,
                        divergence.first_divergence_path,
                        "blocked_one_step_divergence",
                    ))
                }
            }
            _ => Err((
                StateDivergenceKind::EngineRuntimeDelta,
                Some("combat.shadow_audit.apply_to_stable"),
                "blocked_engine_step",
            )),
        };

        match action_status {
            Ok(()) => {
                result.verified_actions += 1;
            }
            Err((kind, path, status)) => {
                result.status = status;
                result.blocked_actions += 1;
                if result.blocking_action_key.is_none() {
                    result.blocking_action_key = Some(action_key);
                    result.blocking_divergence_kind = Some(kind);
                    result.blocking_path = path;
                }
            }
        }
    }

    Some(result)
}

fn legal_action_map(
    stepper: &impl CombatStepper,
    plugins: &CombatSearchPluginStack,
    node: &SearchNode,
) -> BTreeMap<String, CombatActionChoice> {
    let position = CombatPosition::new(node.engine.clone(), node.combat.clone());
    filtered_legal_actions(
        stepper.atomic_action_choices(&position),
        plugins.potion.policy,
        plugins.potion.allowed_potion_slots,
        plugins.potion.allow_potion_discard,
        &node.combat,
    )
    .into_iter()
    .map(|choice| (choice.action_key.clone(), choice))
    .collect()
}

fn one_step_effect(
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    plugins: &CombatSearchPluginStack,
    node: &SearchNode,
    choice: &CombatActionChoice,
) -> Result<TurnSequenceEffectFingerprint, ()> {
    let position = CombatPosition::new(node.engine.clone(), node.combat.clone());
    let step = stepper.apply_to_stable(
        &position,
        choice.input.clone(),
        CombatStepLimits {
            max_engine_steps: config.max_engine_steps_per_action,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err(());
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
    child.push_action(CombatSearchV2ActionTrace {
        step_index: node.actions.len(),
        action_id: 0,
        action_key: choice.action_key.clone(),
        action_debug: choice.action_debug.clone(),
        input: choice.input.clone(),
    });

    let child_position = CombatPosition::new(child.engine.clone(), child.combat.clone());
    let child_legal_count = legal_action_count(stepper, plugins, &child, &child_position);
    Ok(effect_fingerprint(&child, child_legal_count))
}

fn combinatorial_pending_choice_owned(stepper: &impl CombatStepper, node: &SearchNode) -> bool {
    if !stepper.supports_canonical_pending_choice_actions() {
        return false;
    }
    let crate::state::core::EngineState::PendingChoice(choice) = &node.engine else {
        return false;
    };
    PendingChoiceActionFamily::from_choice(choice).is_some()
}

fn legal_action_count(
    stepper: &impl CombatStepper,
    plugins: &CombatSearchPluginStack,
    node: &SearchNode,
    position: &CombatPosition,
) -> usize {
    if stepper.supports_canonical_pending_choice_actions() {
        if let crate::state::core::EngineState::PendingChoice(choice) = &node.engine {
            if PendingChoiceActionFamily::from_choice(choice).is_some() {
                return pending_choice_fanout(choice).estimated_action_fanout;
            }
        }
    }
    filtered_legal_actions(
        stepper.atomic_action_choices(position),
        plugins.potion.policy,
        plugins.potion.allowed_potion_slots,
        plugins.potion.allow_potion_discard,
        &node.combat,
    )
    .len()
}

fn classify_pair(
    left: &TurnSequenceEffectFingerprint,
    right: &TurnSequenceEffectFingerprint,
) -> TurnSequenceDivergence {
    let mut aggregate = TurnSequenceEffectAggregate::default();
    aggregate.observe(left);
    aggregate.observe(right);
    aggregate.classify()
}
