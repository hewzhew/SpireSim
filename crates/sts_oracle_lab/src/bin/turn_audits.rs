use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::{json, Value};
use sts_combat_planner::{CombatPolicyChoice, SelectionTransactionCursor};
use sts_oracle_runtime::eval::combat_case::{load_combat_case, save_combat_case};
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
use sts_oracle_runtime::sim::combat_action::combat_action_key;
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_policy_controls::load_action_imitation_policy;
use super::combat_replay_tools::save_combat_inputs;
use super::combat_trace_view::{combat_action_label, combat_turn_snapshot};
use super::print_json;

fn replay_optional_prefix(
    position: sts_oracle_runtime::sim::combat::CombatPosition,
    actions: Option<&Path>,
    through: usize,
    max_engine_steps_per_transition: usize,
) -> Result<sts_oracle_runtime::sim::combat::CombatPosition, String> {
    let Some(actions) = actions else {
        return Ok(position);
    };
    let actions = serde_json::from_slice::<Vec<ClientInput>>(
        &std::fs::read(actions).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("invalid prefix action list: {error}"))?;
    if through > actions.len() {
        return Err(format!(
            "--through {through} exceeds the {} available prefix actions",
            actions.len()
        ));
    }
    replay_prefix(
        position,
        actions.into_iter().take(through),
        max_engine_steps_per_transition,
    )
}

fn replay_prefix(
    mut position: sts_oracle_runtime::sim::combat::CombatPosition,
    actions: impl IntoIterator<Item = ClientInput>,
    max_engine_steps_per_transition: usize,
) -> Result<sts_oracle_runtime::sim::combat::CombatPosition, String> {
    for (index, input) in actions.into_iter().enumerate() {
        if EngineCombatStepper
            .choice_for_legal_input(&position, &input)
            .is_none()
        {
            return Err(format!("prefix action {index} is not legal"));
        }
        let result = EngineCombatStepper.apply_to_stable(
            &position,
            input,
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if result.truncated || result.timed_out {
            return Err(format!(
                "prefix action {index} did not reach a stable state"
            ));
        }
        position = result.position;
    }
    Ok(position)
}

fn turn_plan_tactical_end_snapshot(
    position: &sts_oracle_runtime::sim::combat::CombatPosition,
    visible_incoming_damage: i32,
) -> Value {
    let combat = &position.combat;
    let visible_hp_loss = visible_incoming_damage
        .saturating_sub(combat.entities.player.block)
        .max(0);
    let player = combat.entities.player.id;
    let player_powers = combat
        .entities
        .power_db
        .get(&player)
        .into_iter()
        .flatten()
        .map(|power| {
            json!({
                "id": power.power_type,
                "amount": power.amount,
            })
        })
        .collect::<Vec<_>>();
    let enemies = combat
        .entities
        .monsters
        .iter()
        .map(|monster| {
            let powers = combat
                .entities
                .power_db
                .get(&monster.id)
                .into_iter()
                .flatten()
                .map(|power| {
                    json!({
                        "id": power.power_type,
                        "amount": power.amount,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "entity_id": monster.id,
                "enemy_id": monster.enemy_id,
                "slot": monster.slot,
                "alive_for_action": monster.is_alive_for_action(),
                "current_hp": monster.current_hp,
                "max_hp": monster.max_hp,
                "block": monster.block,
                "powers": powers,
            })
        })
        .collect::<Vec<_>>();
    let hand = combat
        .zones
        .hand
        .iter()
        .map(|card| {
            json!({
                "id": card.id,
                "lineage": card.lineage,
                "upgrades": card.upgrades,
                "cost_for_turn": card.cost_for_turn_java(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "visible_hp_loss": visible_hp_loss,
        "visible_survival_margin": combat
            .entities
            .player
            .current_hp
            .saturating_sub(visible_hp_loss),
        "hand": hand,
        "player_powers": player_powers,
        "enemies": enemies,
    })
}

pub(super) fn single_potion_slot_mask(slot: usize) -> Result<u64, String> {
    1_u64
        .checked_shl(slot.try_into().unwrap_or(u32::MAX))
        .ok_or_else(|| format!("--potion-slot {slot} exceeds the 64-slot mask"))
}

pub(super) fn potion_slot_mask(slots: &[usize]) -> Result<u64, String> {
    slots.iter().try_fold(0_u64, |mask, slot| {
        single_potion_slot_mask(*slot).map(|slot_mask| mask | slot_mask)
    })
}

#[derive(Debug, Args)]
pub(super) struct TurnActionAuditArgs {
    #[arg(long)]
    case: PathBuf,
    #[arg(long)]
    action_imitation_artifact: Option<PathBuf>,
    /// Optional exact action list used only to reach the audited prefix.
    #[arg(long)]
    actions: Option<PathBuf>,
    /// Number of actions from --actions to replay before auditing.
    #[arg(long, default_value_t = 0, requires = "actions")]
    through: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    /// Maximum number of concrete inputs materialized from each structured
    /// selection family. The report declares any omitted remainder.
    #[arg(long, default_value_t = 256)]
    max_structured_members_per_family: usize,
}

pub(super) fn run_action(args: TurnActionAuditArgs) -> Result<(), String> {
    let TurnActionAuditArgs {
        case,
        action_imitation_artifact,
        actions,
        through,
        max_engine_steps_per_transition,
        max_structured_members_per_family,
    } = args;
    let case = load_combat_case(&case)?;
    let position = replay_optional_prefix(
        case.core.position,
        actions.as_deref(),
        through,
        max_engine_steps_per_transition,
    )?;

    let policy = action_imitation_artifact
        .as_deref()
        .map(|path| load_action_imitation_policy(path, retained_combat_tactical_policy_v1()))
        .transpose()?
        .unwrap_or_else(retained_combat_tactical_policy_v1);
    let surface = EngineCombatStepper.legal_action_surface(&position);
    let choices = surface
        .atomic_actions
        .iter()
        .map(CombatPolicyChoice::Atomic)
        .chain(
            surface
                .selection_families
                .iter()
                .map(CombatPolicyChoice::StructuredSelection),
        )
        .collect::<Vec<_>>();
    let raw_weights = policy.weights(&position, &choices);
    let raw_weights = (raw_weights.len() == choices.len())
        .then_some(raw_weights)
        .unwrap_or_else(|| vec![1.0; choices.len()]);
    let safe_weights = raw_weights
        .iter()
        .map(|weight| {
            if weight.is_finite() && *weight > 0.0 {
                *weight
            } else {
                1.0
            }
        })
        .collect::<Vec<_>>();
    let total = safe_weights.iter().sum::<f64>();
    let uniform = 1.0 / safe_weights.len().max(1) as f64;
    let probabilities = safe_weights
        .iter()
        .map(|weight| 0.95 * (*weight / total) + 0.05 * uniform)
        .collect::<Vec<_>>();
    let atomic_priority_diagnostics =
                sts_oracle_runtime::ai::combat_search_v2::oracle_action_policy::
                    oracle_atomic_action_policy_priority_diagnostics_v1(
                        &position,
                        &surface.atomic_actions,
                    );
    let atomic = surface
                .atomic_actions
                .iter()
                .enumerate()
                .map(|(index, input)| {
                    let result = EngineCombatStepper.apply_to_stable(
                        &position,
                        input.clone(),
                        CombatStepLimits {
                            max_engine_steps: max_engine_steps_per_transition,
                            deadline: None,
                        },
                    );
                    let raw_weight = safe_weights[index];
                    let rank = 1 + safe_weights
                        .iter()
                        .filter(|candidate| **candidate > raw_weight)
                        .count();
                    let successor_guides = (!result.truncated && !result.timed_out)
                        .then(|| {
                            policy
                                .turn_generation_guides(&result.position)
                                .into_iter()
                                .map(|guide| json!({
                                    "lane": guide.lane.value(),
                                    "components": guide.rank.components(),
                                }))
                                .collect::<Vec<_>>()
                        });
                    json!({
                        "canonical_index": index,
                        "label": combat_action_label(&position, input),
                        "key": combat_action_key(&position.combat, input),
                        "raw_weight": raw_weight,
                        "probability": probabilities[index],
                        "ordinal_rank": rank,
                        "priority": atomic_priority_diagnostics[index],
                        "transition": {
                            "truncated": result.truncated,
                            "timed_out": result.timed_out,
                            "engine_steps": result.engine_steps,
                            "terminal": format!("{:?}", result.terminal),
                            "exact_successor_hash": sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
                                &result.position.engine,
                                &result.position.combat,
                            ),
                            "snapshot": combat_turn_snapshot(&result.position),
                            "generation_guides": successor_guides,
                        },
                    })
                })
                .collect::<Vec<_>>();
    let family_offset = surface.atomic_actions.len();
    let structured_families = surface
        .selection_families
        .iter()
        .enumerate()
        .map(|(index, family)| {
            let weight_index = family_offset + index;
            let raw_weight = safe_weights[weight_index];
            let rank = 1 + safe_weights
                .iter()
                .filter(|candidate| **candidate > raw_weight)
                .count();
            let member_policy_applied = family.declared_min == 1 && family.effective_max == 1;
            let (total_member_count, members, members_truncated, enumeration_gap) =
                match SelectionTransactionCursor::new(family) {
                    Ok(mut cursor) => {
                        let total = cursor.remaining_input_count();
                        let members = std::iter::from_fn(|| cursor.next_input())
                            .take(max_structured_members_per_family.max(1))
                            .collect::<Vec<_>>();
                        let truncated = members.len() < total;
                        (total, members, truncated, None)
                    }
                    Err(kind) => (0, Vec::new(), false, Some(format!("{kind:?}"))),
                };
            let raw_member_weights = (member_policy_applied && !members_truncated)
                .then(|| policy.structured_selection_member_weights(&position, family, &members));
            let member_weights_complete = raw_member_weights
                .as_ref()
                .is_some_and(|weights| weights.len() == members.len())
                && !members_truncated;
            let effective_member_weights = raw_member_weights.as_ref().map(|weights| {
                if weights.len() == members.len() {
                    weights
                        .iter()
                        .map(|weight| {
                            if weight.is_finite() && *weight > 0.0 {
                                *weight
                            } else {
                                1.0
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![1.0; members.len()]
                }
            });
            let member_probabilities = if member_policy_applied && member_weights_complete {
                let weights = effective_member_weights
                    .as_ref()
                    .expect("complete member weights are present");
                let total = weights.iter().sum::<f64>();
                let uniform = 1.0 / weights.len().max(1) as f64;
                Some(
                    weights
                        .iter()
                        .map(|weight| 0.95 * (*weight / total) + 0.05 * uniform)
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            };
            let members = members
                .iter()
                .enumerate()
                .map(|(member_index, input)| {
                    let effective_weight = effective_member_weights
                        .as_ref()
                        .and_then(|weights| weights.get(member_index))
                        .copied();
                    let ordinal_rank = member_weights_complete.then(|| {
                        1 + effective_member_weights
                            .as_ref()
                            .expect("complete member weights are present")
                            .iter()
                            .filter(|candidate| Some(**candidate) > effective_weight)
                            .count()
                    });
                    let selected_card_lineages = match input {
                        ClientInput::SubmitSelection(resolution) => {
                            Some(resolution.selected_card_lineages())
                        }
                        _ => None,
                    };
                    let selected_scry_indices = match input {
                        ClientInput::SubmitScryDiscard(indices) => Some(indices.as_slice()),
                        _ => None,
                    };
                    json!({
                        "canonical_index": member_index,
                        "input": input,
                        "key": combat_action_key(&position.combat, input),
                        "selected_card_lineages": selected_card_lineages,
                        "selected_scry_indices": selected_scry_indices,
                        "raw_weight": raw_member_weights
                            .as_ref()
                            .and_then(|weights| weights.get(member_index))
                            .filter(|weight| weight.is_finite())
                            .copied(),
                        "effective_weight": effective_weight,
                        "conditional_probability": member_probabilities
                            .as_ref()
                            .and_then(|probabilities| probabilities.get(member_index))
                            .copied()
                            .or_else(|| (!member_policy_applied && total_member_count > 0)
                                .then_some(1.0 / total_member_count as f64)),
                        "ordinal_rank": ordinal_rank,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "family_index": index,
                "reason": format!("{:?}", family.reason),
                "declared_min": family.declared_min,
                "effective_max": family.effective_max,
                "eligible_domain_count": family.eligible_domain_count,
                "raw_weight": raw_weight,
                "probability": probabilities[weight_index],
                "ordinal_rank": rank,
                "member_policy_applied": member_policy_applied,
                "total_member_count": total_member_count,
                "reported_member_count": members.len(),
                "members_truncated": members_truncated,
                "member_weights_complete": member_weights_complete,
                "enumeration_gap": enumeration_gap,
                "members": members,
            })
        })
        .collect::<Vec<_>>();
    print_json(&json!({
        "schema_name": "OracleTurnActionAuditV1",
        "schema_version": 3,
        "through": through,
        "max_engine_steps_per_transition": max_engine_steps_per_transition,
        "max_structured_members_per_family": max_structured_members_per_family,
        "position_hash": sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
            &position.engine,
            &position.combat,
        ),
        "position": combat_turn_snapshot(&position),
        "current_generation_guides": policy
            .turn_generation_guides(&position)
            .into_iter()
            .map(|guide| json!({
                "lane": guide.lane.value(),
                "components": guide.rank.components(),
            }))
            .collect::<Vec<_>>(),
        "atomic_actions": atomic,
        "structured_families": structured_families,
    }))
}

#[derive(Debug, Args)]
pub(super) struct TurnPlanAuditArgs {
    #[arg(long)]
    case: PathBuf,
    /// Optional exact action list used only to reach the audited prefix.
    #[arg(long)]
    actions: Option<PathBuf>,
    /// Number of actions from --actions to replay before auditing.
    #[arg(long, default_value_t = 0, requires = "actions")]
    through: usize,
    #[arg(long, default_value_t = 256)]
    max_inner_nodes: usize,
    #[arg(long, default_value_t = 24)]
    max_end_states: usize,
    #[arg(long, default_value_t = 24)]
    per_bucket_limit: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    /// Open exactly one concrete potion identity lane. Slots are zero-based.
    #[arg(long)]
    potion_slot: Option<usize>,
    /// Number of selected non-loss turn plans shown by the default compact
    /// report.
    #[arg(long, default_value_t = 8)]
    limit: usize,
    /// Include every selected plan and the complete preselection audit.
    #[arg(long)]
    full: bool,
    /// Export this zero-based rank among the displayed non-loss plans.
    #[arg(long)]
    export_rank: Option<usize>,
    /// Save the selected plan's exact next-turn state as a combat case.
    #[arg(long, requires = "export_rank")]
    export_case: Option<PathBuf>,
    /// Save the selected plan's exact ClientInput list.
    #[arg(long, requires = "export_rank")]
    export_actions: Option<PathBuf>,
}

pub(super) fn run_plan(args: TurnPlanAuditArgs) -> Result<(), String> {
    let TurnPlanAuditArgs {
        case,
        actions,
        through,
        max_inner_nodes,
        max_end_states,
        per_bucket_limit,
        max_engine_steps_per_transition,
        potion_slot,
        limit,
        full,
        export_rank,
        export_case,
        export_actions,
    } = args;
    let mut case = load_combat_case(&case)?;
    case.core.position = replay_optional_prefix(
        case.core.position,
        actions.as_deref(),
        through,
        max_engine_steps_per_transition,
    )?;
    let prefix = json!({
        "actions": actions,
        "through": through,
        "position_hash": sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
            &case.core.position.engine,
            &case.core.position.combat,
        ),
    });
    let mut config = sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Config::default();
    config.max_engine_steps_per_action = max_engine_steps_per_transition.max(1);
    config.turn_plan_probe_max_inner_nodes = Some(max_inner_nodes.max(1));
    config.turn_plan_probe_max_end_states = Some(max_end_states.max(1));
    config.turn_plan_probe_per_bucket_limit = Some(per_bucket_limit.max(1));
    if let Some(slot) = potion_slot {
        config.potion_policy =
            sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2PotionPolicy::All;
        config.max_potions_used = Some(1);
        config.allowed_potion_slots = Some(single_potion_slot_mask(slot)?);
        config.allow_potion_discard = Some(false);
    }
    config.input_label = Some("oracle_lab_turn_plan_audit".to_string());
    let audit = sts_oracle_runtime::ai::combat_search_v2::
                enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices(
                    &case.core.position.engine,
                    &case.core.position.combat,
                    &config,
                );
    let exported_plan = if let Some(rank) = export_rank {
        let candidate = audit
            .candidates
            .iter()
            .filter(|candidate| candidate.report.bucket != "terminal_loss")
            .nth(rank)
            .ok_or_else(|| format!("non-loss turn-plan rank {rank} is unavailable"))?;
        if let Some(path) = export_case.as_ref() {
            let mut exported = case.clone();
            exported.core.position = candidate.position.clone();
            exported.refresh_derived_summaries_and_clear_production_context();
            exported.core.gap.boundary = format!(
                "{} + audited turn plan rank {rank}",
                exported.core.gap.boundary
            );
            exported.core.gap.reason = "oracle_lab_turn_plan_audit_successor".to_string();
            exported.atomic_combat_search_attempts.clear();
            exported.failed_atomic_combat_search = None;
            save_combat_case(path, &exported)?;
        }
        if let Some(path) = export_actions.as_ref() {
            save_combat_inputs(
                path,
                candidate
                    .report
                    .actions
                    .iter()
                    .map(|action| action.input.clone()),
            )?;
        }
        Some(json!({
            "rank": rank,
            "plan_index": candidate.report.plan_index,
            "case": export_case,
            "actions": export_actions,
        }))
    } else {
        None
    };
    let selected = audit
        .candidates
        .iter()
        .map(|candidate| {
            let report = &candidate.report;
            json!({
                "plan_index": report.plan_index,
                "bucket": report.bucket,
                "stop_reason": report.stop_reason,
                "action_count": report.action_count,
                "actions": report.actions.iter().map(|action| {
                    json!({
                        "key": action.action_key,
                        "debug": action.action_debug,
                    })
                }).collect::<Vec<_>>(),
                "end_exact_state_hash": report.steps.last().map(|step| {
                    step.state_after_exact_state_hash.as_str()
                }),
                "final_hp": report.eval_final_hp,
                "risk_margin": report.eval_risk_margin,
                "enemy_progress": report.eval_enemy_progress,
                "end_state": report.end_state,
                "tactical_end": turn_plan_tactical_end_snapshot(
                    &candidate.position,
                    report.end_state.visible_incoming_damage,
                ),
            })
        })
        .collect::<Vec<_>>();
    let preselection = audit
        .report
        .selection_audit
        .candidates
        .iter()
        .map(|candidate| {
            json!({
                "preselection_rank": candidate.preselection_rank,
                "selected_plan_index": candidate.selected_plan_index,
                "outcome": candidate.outcome,
                "drop_reason": candidate.drop_reason,
                "bucket": candidate.bucket,
                "action_keys": candidate.action_keys,
            })
        })
        .collect::<Vec<_>>();
    if !full {
        let compact_selected = audit
            .candidates
            .iter()
            .filter(|candidate| candidate.report.bucket != "terminal_loss")
            .take(limit)
            .map(|candidate| {
                let report = &candidate.report;
                json!({
                    "plan_index": report.plan_index,
                    "bucket": report.bucket,
                    "stop_reason": report.stop_reason,
                    "action_count": report.action_count,
                    "actions": report.actions.iter().map(|action| {
                        action.action_key.as_str()
                    }).collect::<Vec<_>>(),
                    "end_exact_state_hash": report.steps.last().map(|step| {
                        step.state_after_exact_state_hash.as_str()
                    }),
                    "final_hp": report.eval_final_hp,
                    "risk_margin": report.eval_risk_margin,
                    "enemy_progress": report.eval_enemy_progress,
                    "end_state": report.end_state,
                    "tactical_end": turn_plan_tactical_end_snapshot(
                        &candidate.position,
                        report.end_state.visible_incoming_damage,
                    ),
                })
            })
            .collect::<Vec<_>>();
        return print_json(&json!({
            "schema_name": "OracleTurnPlanAuditCompactV3",
            "schema_version": 3,
            "behavioral_scope": "read_only_no_search_seeding",
            "prefix": prefix,
            "config": audit.report.config,
            "enumeration": audit.report.enumeration,
            "exported_plan": exported_plan,
            "selected_non_loss": compact_selected,
        }));
    }
    print_json(&json!({
        "schema_name": "OracleTurnPlanAuditV3",
        "schema_version": 3,
        "behavioral_scope": "read_only_no_search_seeding",
        "prefix": prefix,
        "config": audit.report.config,
        "enumeration": audit.report.enumeration,
        "exported_plan": exported_plan,
        "preselection": preselection,
        "selected": selected,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_runtime::content::cards::CardId;
    use sts_oracle_runtime::content::monsters::EnemyId;
    use sts_oracle_runtime::content::powers::PowerId;
    use sts_oracle_runtime::runtime::combat::{CombatCard, Power, PowerPayload};
    use sts_oracle_runtime::sim::combat::CombatPosition;
    use sts_oracle_runtime::state::core::EngineState;
    use sts_oracle_runtime::test_support::{blank_test_combat, test_monster};

    #[test]
    fn exact_prefix_replay_reaches_a_stable_same_turn_state() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 41)];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let original_turn = position.combat.turn.turn_count;

        let replayed = replay_prefix(
            position,
            [ClientInput::PlayCard {
                card_index: 0,
                target: None,
            }],
            250,
        )
        .expect("defend should replay to a stable same-turn state");

        assert_eq!(replayed.engine, EngineState::CombatPlayerTurn);
        assert_eq!(replayed.combat.turn.turn_count, original_turn);
        assert!(replayed.combat.entities.player.block > 0);
        assert!(replayed.combat.zones.hand.is_empty());
    }

    #[test]
    fn concrete_potion_lane_uses_one_exact_slot_bit() {
        assert_eq!(single_potion_slot_mask(0), Ok(1));
        assert_eq!(single_potion_slot_mask(2), Ok(4));
        assert!(single_potion_slot_mask(64).is_err());
    }

    #[test]
    fn concrete_potion_lane_combines_and_deduplicates_exact_slots() {
        assert_eq!(potion_slot_mask(&[0, 2]), Ok(5));
        assert_eq!(potion_slot_mask(&[2, 2]), Ok(4));
        assert!(potion_slot_mask(&[0, 64]).is_err());
    }

    #[test]
    fn tactical_end_snapshot_keeps_enemy_slots_and_typed_powers() {
        let mut combat = blank_test_combat();
        let mut monster = test_monster(EnemyId::SphericGuardian);
        monster.id = 7;
        monster.slot = 2;
        monster.current_hp = 19;
        monster.block = 11;
        combat.entities.monsters = vec![monster];
        combat.entities.power_db.insert(
            7,
            vec![
                Power {
                    power_type: PowerId::Artifact,
                    instance_id: None,
                    amount: 2,
                    extra_data: 0,
                    payload: PowerPayload::None,
                    just_applied: false,
                },
                Power {
                    power_type: PowerId::Vulnerable,
                    instance_id: None,
                    amount: 3,
                    extra_data: 0,
                    payload: PowerPayload::None,
                    just_applied: false,
                },
            ],
        );
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

        let snapshot = turn_plan_tactical_end_snapshot(&position, 0);
        let enemy = &snapshot["enemies"][0];

        assert_eq!(enemy["entity_id"], 7);
        assert_eq!(enemy["slot"], 2);
        assert_eq!(enemy["current_hp"], 19);
        assert_eq!(enemy["block"], 11);
        assert_eq!(enemy["powers"][0]["id"], "Artifact");
        assert_eq!(enemy["powers"][0]["amount"], 2);
        assert_eq!(enemy["powers"][1]["id"], "Vulnerable");
        assert_eq!(enemy["powers"][1]["amount"], 3);
    }
}
