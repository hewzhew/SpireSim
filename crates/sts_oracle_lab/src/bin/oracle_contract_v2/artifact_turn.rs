use std::collections::{BTreeMap, HashSet};

use serde_json::{json, Value};
use sts_oracle_runtime::ai::combat_search_v2::{
    recoverable_stolen_gold, unrecovered_stolen_gold, CombatSearchV2Config,
    CombatSearchV2TurnPlanProbeCandidate, CombatSearchV2TurnPlanProbeEnumeration,
};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::sim::combat::{combat_terminal, CombatTerminal};
use sts_oracle_runtime::state::core::EngineState;

use super::super::combat_trace_view::combat_turn_snapshot;
use super::super::print_json;
use super::artifact_navigation::{
    branch_state, ensure_player_turn, enumerate_turn, resolve, unique_state_candidate,
    ArtifactNavigationSpec,
};
use super::{ArtifactTurnArgs, CombatContractArtifactV2};

pub(super) fn run(
    args: &ArtifactTurnArgs,
    artifact: &CombatContractArtifactV2,
) -> Result<(), String> {
    if args.limit == 0 {
        return Err("artifact turn output limit must be positive".to_owned());
    }
    let navigation = resolve(
        &args.artifact,
        artifact,
        ArtifactNavigationSpec {
            candidate: args.navigation.candidate,
            turn: args.navigation.turn,
            follow_plan: &args.navigation.follow_plan,
            follow_state: &args.navigation.follow_state,
            max_inner_nodes: args.navigation.max_inner_nodes,
            max_end_states: args.navigation.max_end_states,
            per_bucket_limit: args.navigation.per_bucket_limit,
            input_label: "oracle_contract_v2_artifact_turn",
        },
    )?;
    let candidate = navigation.candidate;
    let position = navigation.position;
    let config = navigation.config;
    let source = navigation.source;
    let followed = navigation
        .followed
        .iter()
        .map(|step| {
            json!({
                "depth": step.depth,
                "from_turn": step.from_turn,
                "state_query": step.state_query,
                "matching_plan_count": step.matching_plan_count,
                "plan": candidate_summary(&step.candidate),
            })
        })
        .collect::<Vec<_>>();

    let terminal = combat_terminal(&position.engine, &position.combat);
    let surface = if args.reached_only {
        Value::Null
    } else if terminal == CombatTerminal::Unresolved {
        ensure_player_turn(
            &position,
            args.navigation.follow_plan.len() + args.navigation.follow_state.len(),
        )?;
        let audit = enumerate_turn(&position, &config);
        let successor_selection = args
            .successor_state
            .as_deref()
            .map(|query| {
                unique_state_candidate(&audit, query).map(|matched| {
                    matched.map_or_else(
                        || {
                            json!({
                                "query": query,
                                "matched": false,
                                "matching_plan_count": 0,
                            })
                        },
                        |(candidate, matching_plan_count)| {
                            json!({
                                "query": query,
                                "matched": true,
                                "matching_plan_count": matching_plan_count,
                                "plan": candidate_summary(candidate),
                            })
                        },
                    )
                })
            })
            .transpose()?;
        let selected = if args.successor_state.is_some() {
            Vec::new()
        } else {
            audit
                .candidates
                .iter()
                .filter(|candidate| candidate.report.bucket != "terminal_loss")
                .take(args.limit)
                .map(candidate_summary)
                .collect::<Vec<Value>>()
        };
        let outcome_groups = if args.successor_state.is_some() {
            Vec::new()
        } else {
            turn_outcome_groups(&audit)
        };
        let next_terminal_scan = if args.scan_next_terminal {
            scan_next_terminal(&audit, &config, artifact, args.limit)
        } else {
            Value::Null
        };
        json!({
            "turn": position.combat.turn.turn_count,
            "exact_state_hash": combat_exact_state_hash_v2(&position.engine, &position.combat),
            "state": combat_turn_snapshot(&position),
            "config": audit.report.config,
            "enumeration": audit.report.enumeration,
            "outcome_groups": outcome_groups,
            "selected_non_loss": selected,
            "successor_selection": successor_selection,
            "next_terminal_scan": next_terminal_scan,
        })
    } else {
        Value::Null
    };
    print_json(&json!({
        "schema_name": "OracleCombatContractCandidateTurnV2",
        "schema_version": 4,
        "artifact": args.artifact,
        "case_id": artifact.request.case_id,
        "candidate": candidate,
        "source": source,
        "followed": followed,
        "reached": branch_state(&position),
        "terminal": format!("{terminal:?}"),
        "surface": surface,
    }))
}

fn candidate_summary(candidate: &CombatSearchV2TurnPlanProbeCandidate) -> Value {
    let report = &candidate.report;
    let visible_hp_loss = report
        .end_state
        .visible_incoming_damage
        .saturating_sub(candidate.position.combat.entities.player.block)
        .max(0);
    json!({
        "plan_index": report.plan_index,
        "bucket": report.bucket,
        "stop_reason": report.stop_reason,
        "actions": report.actions.iter().map(|action| {
            action.action_key.as_str()
        }).collect::<Vec<_>>(),
        "end_exact_state_hash": combat_exact_state_hash_v2(
            &candidate.position.engine,
            &candidate.position.combat,
        ),
        "terminal": format!(
            "{:?}",
            combat_terminal(&candidate.position.engine, &candidate.position.combat),
        ),
        "player_hp": candidate.position.combat.entities.player.current_hp,
        "player_block": candidate.position.combat.entities.player.block,
        "visible_incoming_damage": report.end_state.visible_incoming_damage,
        "visible_hp_loss": visible_hp_loss,
        "visible_survival_hp": candidate
            .position
            .combat
            .entities
            .player
            .current_hp
            .saturating_sub(visible_hp_loss),
        "living_enemy_count": report.end_state.living_enemy_count,
        "total_enemy_hp": report.end_state.total_enemy_hp,
        "recoverable_stolen_gold": recoverable_stolen_gold(&candidate.position.combat),
        "unrecovered_stolen_gold": unrecovered_stolen_gold(&candidate.position.combat),
        "state": combat_turn_snapshot(&candidate.position),
    })
}

fn turn_outcome_groups(audit: &CombatSearchV2TurnPlanProbeEnumeration) -> Vec<Value> {
    let mut groups = BTreeMap::<(i32, usize, i32, i32, bool), Vec<usize>>::new();
    for candidate in &audit.candidates {
        if candidate.report.bucket == "terminal_loss" {
            continue;
        }
        let terminal = combat_terminal(&candidate.position.engine, &candidate.position.combat)
            != CombatTerminal::Unresolved;
        groups
            .entry((
                candidate.position.combat.entities.player.current_hp,
                candidate.report.end_state.living_enemy_count,
                candidate.report.end_state.total_enemy_hp,
                unrecovered_stolen_gold(&candidate.position.combat),
                terminal,
            ))
            .or_default()
            .push(candidate.report.plan_index);
    }
    groups
        .into_iter()
        .map(
            |(
                (player_hp, living_enemy_count, total_enemy_hp, unrecovered_stolen_gold, terminal),
                plan_indices,
            )| {
                let count = plan_indices.len();
                json!({
                    "player_hp": player_hp,
                    "living_enemy_count": living_enemy_count,
                    "total_enemy_hp": total_enemy_hp,
                    "unrecovered_stolen_gold": unrecovered_stolen_gold,
                    "terminal": terminal,
                    "plan_count": count,
                    "plan_indices": plan_indices.into_iter().take(12).collect::<Vec<_>>(),
                })
            },
        )
        .collect()
}

fn scan_next_terminal(
    surface: &CombatSearchV2TurnPlanProbeEnumeration,
    config: &CombatSearchV2Config,
    artifact: &CombatContractArtifactV2,
    limit: usize,
) -> Value {
    let mut parents_scanned = 0usize;
    let mut parents_not_player_turn = 0usize;
    let mut censored_parent_surfaces = 0usize;
    let mut generated_child_plans = 0usize;
    let mut terminal_descendants = 0usize;
    let mut no_unrecovered_descendants = 0usize;
    let mut hp_and_no_unrecovered_descendants = 0usize;
    let mut min_unrecovered_stolen_gold = None::<i32>;
    let mut max_final_hp_with_no_unrecovered_stolen_gold = None::<i32>;
    let mut seen_terminal_hashes = HashSet::<String>::new();
    let mut seen_nonterminal_hashes = HashSet::<String>::new();
    let mut max_nonterminal_hp = None::<i32>;
    let mut min_nonterminal_enemy_hp = None::<i32>;
    let mut witnesses = Vec::<(bool, i32, i32, usize, Value)>::new();

    for parent in &surface.candidates {
        if combat_terminal(&parent.position.engine, &parent.position.combat)
            != CombatTerminal::Unresolved
            || !matches!(parent.position.engine, EngineState::CombatPlayerTurn)
        {
            parents_not_player_turn = parents_not_player_turn.saturating_add(1);
            continue;
        }
        parents_scanned = parents_scanned.saturating_add(1);
        let descendants = enumerate_turn(&parent.position, config);
        generated_child_plans = generated_child_plans.saturating_add(descendants.candidates.len());
        let enumeration = &descendants.report.enumeration;
        if enumeration.truncated_children > 0
            || enumeration.preselection_plans != enumeration.plans
            || enumeration.nodes_expanded
                >= config.turn_plan_probe_max_inner_nodes.unwrap_or(usize::MAX)
        {
            censored_parent_surfaces = censored_parent_surfaces.saturating_add(1);
        }
        for child in &descendants.candidates {
            if combat_terminal(&child.position.engine, &child.position.combat)
                == CombatTerminal::Unresolved
            {
                let exact_hash =
                    combat_exact_state_hash_v2(&child.position.engine, &child.position.combat);
                if seen_nonterminal_hashes.insert(exact_hash) {
                    let player_hp = child.position.combat.entities.player.current_hp;
                    let enemy_hp = child.report.end_state.total_enemy_hp;
                    max_nonterminal_hp = Some(
                        max_nonterminal_hp.map_or(player_hp, |current| current.max(player_hp)),
                    );
                    min_nonterminal_enemy_hp = Some(
                        min_nonterminal_enemy_hp.map_or(enemy_hp, |current| current.min(enemy_hp)),
                    );
                }
                continue;
            }
            let exact_hash =
                combat_exact_state_hash_v2(&child.position.engine, &child.position.combat);
            if !seen_terminal_hashes.insert(exact_hash.clone()) {
                continue;
            }
            terminal_descendants = terminal_descendants.saturating_add(1);
            let unrecovered = unrecovered_stolen_gold(&child.position.combat);
            let final_hp = child.position.combat.entities.player.current_hp;
            let no_unrecovered = unrecovered == 0;
            let hp_satisfied = artifact
                .request
                .min_final_hp
                .is_none_or(|minimum| final_hp >= minimum);
            min_unrecovered_stolen_gold = Some(
                min_unrecovered_stolen_gold.map_or(unrecovered, |current| current.min(unrecovered)),
            );
            if no_unrecovered {
                no_unrecovered_descendants = no_unrecovered_descendants.saturating_add(1);
                max_final_hp_with_no_unrecovered_stolen_gold = Some(
                    max_final_hp_with_no_unrecovered_stolen_gold
                        .map_or(final_hp, |current| current.max(final_hp)),
                );
                if hp_satisfied {
                    hp_and_no_unrecovered_descendants =
                        hp_and_no_unrecovered_descendants.saturating_add(1);
                }
            }
            witnesses.push((
                hp_satisfied && no_unrecovered,
                final_hp,
                unrecovered,
                parent.report.plan_index,
                json!({
                    "parent_plan_index": parent.report.plan_index,
                    "parent_actions": action_keys(parent),
                    "terminal_plan_index": child.report.plan_index,
                    "terminal_actions": action_keys(child),
                    "terminal_exact_state_hash": exact_hash,
                    "final_hp": final_hp,
                    "recoverable_stolen_gold": recoverable_stolen_gold(&child.position.combat),
                    "unrecovered_stolen_gold": unrecovered,
                    "hp_satisfied": hp_satisfied,
                    "no_unrecovered_stolen_gold": no_unrecovered,
                    "state": combat_turn_snapshot(&child.position),
                }),
            ));
        }
    }
    witnesses.sort_by(
        |(left_pass, left_hp, left_gold, left_parent, _),
         (right_pass, right_hp, right_gold, right_parent, _)| {
            right_pass
                .cmp(left_pass)
                .then_with(|| left_gold.cmp(right_gold))
                .then_with(|| right_hp.cmp(left_hp))
                .then_with(|| left_parent.cmp(right_parent))
        },
    );
    let witnesses = witnesses
        .into_iter()
        .take(limit)
        .map(|(_, _, _, _, witness)| witness)
        .collect::<Vec<_>>();
    json!({
        "scope": "one_additional_exact_complete_turn_from_every_selected_parent",
        "parent_plans": surface.candidates.len(),
        "parents_scanned": parents_scanned,
        "parents_not_player_turn": parents_not_player_turn,
        "censored_parent_surfaces": censored_parent_surfaces,
        "generated_child_plans": generated_child_plans,
        "unique_terminal_descendants": terminal_descendants,
        "unique_nonterminal_descendants": seen_nonterminal_hashes.len(),
        "max_nonterminal_hp": max_nonterminal_hp,
        "min_nonterminal_enemy_hp": min_nonterminal_enemy_hp,
        "unique_terminal_descendants_with_no_unrecovered_stolen_gold": no_unrecovered_descendants,
        "unique_terminal_descendants_meeting_hp_and_stolen_gold": hp_and_no_unrecovered_descendants,
        "min_unrecovered_stolen_gold": min_unrecovered_stolen_gold,
        "max_final_hp_with_no_unrecovered_stolen_gold":
            max_final_hp_with_no_unrecovered_stolen_gold,
        "complete": censored_parent_surfaces == 0,
        "witnesses": witnesses,
    })
}

fn action_keys(candidate: &CombatSearchV2TurnPlanProbeCandidate) -> Vec<&str> {
    candidate
        .report
        .actions
        .iter()
        .map(|action| action.action_key.as_str())
        .collect()
}
