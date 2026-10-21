//! Privileged, bounded one-turn continuation evidence for an already
//! materialized complete-turn surface.
//!
//! This module is diagnostic only. It does not schedule the resident graph,
//! prune exact states, or turn a bounded miss into a loss.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use blake2::{Blake2b512, Digest};
use serde::Serialize;
use sts_combat_planner::{
    CombatDecisionRoot, CombatPlanningQuantum, CompleteTurnOption, CompleteTurnOptionBoundary,
    SharedCombatActionPolicy, TurnOptionGenerationStatus, TurnOptionGeneratorConfig,
    TurnOptionGeneratorSession,
};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
};
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_trace_view::combat_action_label;
use super::exact_turn_corridor::typed_feature_components;

#[derive(Clone, Debug, Serialize)]
struct GuideObservationV1 {
    lane: u32,
    components: Vec<i32>,
}

#[derive(Clone, Debug, Serialize)]
struct ResourceObservationV1 {
    exact_state_hash: String,
    boundary: String,
    player_turn: u32,
    player_hp: i32,
    player_max_hp: i32,
    player_block: i32,
    alive_enemy_count: usize,
    enemy_total_hp: i32,
    potion_slots: Vec<Option<String>>,
    card_setup: CardSetupObservationV1,
    typed_value_features: Vec<i32>,
    guides: Vec<GuideObservationV1>,
}

#[derive(Clone, Debug, Serialize)]
struct CardSetupObservationV1 {
    semantic_live_exhaust_blake2b_512: String,
    semantic_zone_order_blake2b_512: String,
    exact_zone_order_blake2b_512: String,
    live_semantic_cards: Vec<SemanticCardCountV1>,
    exhaust_semantic_cards: Vec<SemanticCardCountV1>,
}

#[derive(Clone, Debug, Serialize)]
struct SemanticCardCountV1 {
    card: serde_json::Value,
    count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct GuideEnvelopeV1 {
    lane: u32,
    best_components: Vec<i32>,
    best_exact_state_hash: String,
}

#[derive(Clone, Debug, Serialize)]
struct ContinuationGenerationV1 {
    status: String,
    complete_surface: bool,
    generation_work: usize,
    engine_steps: usize,
    completed_options: usize,
    next_turn_boundaries: usize,
    terminal_wins: usize,
    terminal_losses: usize,
    escapes: usize,
    gap_count: usize,
    retained_work_items: usize,
}

#[derive(Clone, Debug, Serialize)]
struct ContinuationCandidateV1 {
    candidate_index: usize,
    first_action_family: String,
    continuation_selected: bool,
    continuation_selection_reasons: Vec<ContinuationSelectionReasonV1>,
    parent_successor_exact_state_hash: String,
    parent_actions: Vec<ClientInput>,
    parent_action_labels: Vec<String>,
    parent_negative_log_policy: f64,
    exact_target_actions: bool,
    equivalent_target_successor: bool,
    parent_boundary: String,
    parent_successor: ResourceObservationV1,
    generation: Option<ContinuationGenerationV1>,
    expected_next_corridor_state_seen: Option<bool>,
    best_next_guides: Vec<GuideEnvelopeV1>,
    next_boundaries: Option<Vec<ResourceObservationV1>>,
}

#[derive(Clone, Debug, Serialize)]
struct ContinuationSelectionReasonV1 {
    kind: &'static str,
    parent_guide_lane: Option<u32>,
}

struct PreparedParentCandidateV1 {
    candidate_index: usize,
    option: CompleteTurnOption,
    first_action_family: String,
    parent_action_labels: Vec<String>,
    parent_successor: ResourceObservationV1,
    exact_target_actions: bool,
    equivalent_target_successor: bool,
}

struct ParentSelectionViewV1<'a> {
    candidate_index: usize,
    first_action_family: &'a str,
    negative_log_policy: f64,
    exact_state_hash: &'a str,
    semantic_setup_hash: &'a str,
    guides: &'a [GuideObservationV1],
}

#[derive(Clone, Debug, Serialize)]
struct GuideLeaderV1 {
    lane: u32,
    best_components: Vec<i32>,
    members: Vec<GuideLeaderMemberV1>,
}

#[derive(Clone, Debug, Serialize)]
struct GuideLeaderMemberV1 {
    candidate_index: usize,
    parent_successor_exact_state_hash: String,
    exact_target_actions: bool,
    equivalent_target_successor: bool,
    best_exact_state_hash: String,
}

#[derive(Clone, Copy)]
pub(super) struct BoundedContinuationProbeOptions<'a> {
    pub(super) parent_status: TurnOptionGenerationStatus,
    pub(super) parent_generation_work: usize,
    pub(super) parent_gap_count: usize,
    pub(super) continuation_work_per_candidate: usize,
    pub(super) max_engine_steps_per_transition: usize,
    pub(super) target_actions: &'a [ClientInput],
    pub(super) target_successor_exact_state_hash: &'a str,
    pub(super) expected_next_corridor_state_hash: Option<&'a str>,
    pub(super) include_all_candidates: bool,
    pub(super) representative_cohort: bool,
}

pub(super) fn probe_bounded_continuations_v1(
    parent_root: &CombatPosition,
    parent_options: &[CompleteTurnOption],
    policy: SharedCombatActionPolicy,
    generator_config: TurnOptionGeneratorConfig,
    options: BoundedContinuationProbeOptions<'_>,
) -> Result<serde_json::Value, String> {
    if options.continuation_work_per_candidate == 0 {
        return Err("continuation work per candidate must be positive".to_string());
    }
    let parent_complete = matches!(options.parent_status, TurnOptionGenerationStatus::Complete)
        && options.parent_gap_count == 0;
    let unique_options = unique_parent_options(parent_options);
    let mut prepared = Vec::with_capacity(unique_options.len());
    for (candidate_index, option) in unique_options.into_iter().enumerate() {
        let exact_target_actions = option.actions().len() == options.target_actions.len()
            && option
                .actions()
                .iter()
                .zip(options.target_actions)
                .all(|(actual, expected)| actual.input == *expected);
        let equivalent_target_successor =
            option.exact_successor_hash() == options.target_successor_exact_state_hash;
        let parent_successor =
            observe_state(option.exact_successor(), option.boundary(), policy.as_ref());
        let parent_action_labels = option_action_labels(
            parent_root,
            &option,
            options.max_engine_steps_per_transition,
        )?;
        let first_action_family = parent_action_labels
            .first()
            .cloned()
            .unwrap_or_else(|| "<no action>".to_string());
        prepared.push(PreparedParentCandidateV1 {
            candidate_index,
            option,
            first_action_family,
            parent_action_labels,
            parent_successor,
            exact_target_actions,
            equivalent_target_successor,
        });
    }
    let selection_views = prepared
        .iter()
        .map(|candidate| ParentSelectionViewV1 {
            candidate_index: candidate.candidate_index,
            first_action_family: &candidate.first_action_family,
            negative_log_policy: candidate.option.negative_log_policy(),
            exact_state_hash: candidate.option.exact_successor_hash(),
            semantic_setup_hash: &candidate
                .parent_successor
                .card_setup
                .semantic_live_exhaust_blake2b_512,
            guides: &candidate.parent_successor.guides,
        })
        .collect::<Vec<_>>();
    let selection_reasons =
        continuation_selection_reasons(&selection_views, options.representative_cohort);
    let observed_family_count = selection_views
        .iter()
        .map(|candidate| candidate.first_action_family)
        .collect::<BTreeSet<_>>()
        .len();
    let selected_candidate_indices = selection_reasons.keys().copied().collect::<Vec<_>>();
    let mut candidates = Vec::with_capacity(prepared.len());
    let mut total_continuation_work = 0usize;
    let mut total_continuation_engine_steps = 0usize;
    for prepared_candidate in prepared {
        let candidate_index = prepared_candidate.candidate_index;
        let option = prepared_candidate.option;
        let parent_actions = option
            .actions()
            .iter()
            .map(|action| action.input.clone())
            .collect::<Vec<_>>();
        let continuation_selection_reasons = selection_reasons
            .get(&candidate_index)
            .cloned()
            .unwrap_or_default();
        let continuation_selected = !continuation_selection_reasons.is_empty();
        if option.boundary() != CompleteTurnOptionBoundary::NextPlayerTurn || !continuation_selected
        {
            candidates.push(ContinuationCandidateV1 {
                candidate_index,
                first_action_family: prepared_candidate.first_action_family,
                continuation_selected,
                continuation_selection_reasons,
                parent_successor_exact_state_hash: option.exact_successor_hash().to_string(),
                parent_actions,
                parent_action_labels: prepared_candidate.parent_action_labels,
                parent_negative_log_policy: option.negative_log_policy(),
                exact_target_actions: prepared_candidate.exact_target_actions,
                equivalent_target_successor: prepared_candidate.equivalent_target_successor,
                parent_boundary: boundary_label(option.boundary()).to_string(),
                parent_successor: prepared_candidate.parent_successor,
                generation: None,
                expected_next_corridor_state_seen: options
                    .expected_next_corridor_state_hash
                    .map(|expected| option.exact_successor_hash() == expected),
                best_next_guides: Vec::new(),
                next_boundaries: options.include_all_candidates.then(Vec::new),
            });
            continue;
        }

        let root = CombatDecisionRoot::new(option.exact_successor().clone())
            .map_err(|error| format!("invalid continuation root: {error:?}"))?;
        let mut generator =
            TurnOptionGeneratorSession::with_policy(root, generator_config, policy.clone());
        let report = generator.advance(
            &EngineCombatStepper,
            CombatPlanningQuantum::deterministic(
                options.continuation_work_per_candidate,
                options
                    .continuation_work_per_candidate
                    .saturating_mul(options.max_engine_steps_per_transition),
            ),
        );
        total_continuation_work =
            total_continuation_work.saturating_add(report.after.generation_work);
        total_continuation_engine_steps =
            total_continuation_engine_steps.saturating_add(report.after.engine_steps);
        let complete_surface =
            matches!(report.status, TurnOptionGenerationStatus::Complete) && report.gaps.is_empty();
        let mut next_turn_boundaries = Vec::new();
        let mut terminal_wins = 0usize;
        let mut terminal_losses = 0usize;
        let mut escapes = 0usize;
        let mut expected_next_seen = false;
        let mut best_guides = BTreeMap::<u32, (Vec<i32>, String)>::new();
        for continuation in generator.completed_options() {
            match continuation.boundary() {
                CompleteTurnOptionBoundary::NextPlayerTurn => {}
                CompleteTurnOptionBoundary::TerminalWin => {
                    terminal_wins = terminal_wins.saturating_add(1)
                }
                CompleteTurnOptionBoundary::TerminalLoss => {
                    terminal_losses = terminal_losses.saturating_add(1)
                }
                CompleteTurnOptionBoundary::Escape => escapes = escapes.saturating_add(1),
            }
            if options.expected_next_corridor_state_hash
                == Some(continuation.exact_successor_hash())
            {
                expected_next_seen = true;
            }
            let observation = observe_state(
                continuation.exact_successor(),
                continuation.boundary(),
                policy.as_ref(),
            );
            for guide in &observation.guides {
                let replace = best_guides.get(&guide.lane).is_none_or(|(rank, hash)| {
                    guide.components > *rank
                        || (guide.components == *rank && observation.exact_state_hash < *hash)
                });
                if replace {
                    best_guides.insert(
                        guide.lane,
                        (
                            guide.components.clone(),
                            observation.exact_state_hash.clone(),
                        ),
                    );
                }
            }
            next_turn_boundaries.push(observation);
        }
        let best_next_guides = best_guides
            .into_iter()
            .map(
                |(lane, (best_components, best_exact_state_hash))| GuideEnvelopeV1 {
                    lane,
                    best_components,
                    best_exact_state_hash,
                },
            )
            .collect::<Vec<_>>();
        let generation = ContinuationGenerationV1 {
            status: format!("{:?}", report.status),
            complete_surface,
            generation_work: report.after.generation_work,
            engine_steps: report.after.engine_steps,
            completed_options: report.total_completed_options,
            next_turn_boundaries: next_turn_boundaries
                .iter()
                .filter(|observation| observation.boundary == "next_player_turn")
                .count(),
            terminal_wins,
            terminal_losses,
            escapes,
            gap_count: report.gaps.len(),
            retained_work_items: report.retained_work_items,
        };
        candidates.push(ContinuationCandidateV1 {
            candidate_index,
            first_action_family: prepared_candidate.first_action_family,
            continuation_selected,
            continuation_selection_reasons,
            parent_successor_exact_state_hash: option.exact_successor_hash().to_string(),
            parent_actions,
            parent_action_labels: prepared_candidate.parent_action_labels,
            parent_negative_log_policy: option.negative_log_policy(),
            exact_target_actions: prepared_candidate.exact_target_actions,
            equivalent_target_successor: prepared_candidate.equivalent_target_successor,
            parent_boundary: boundary_label(option.boundary()).to_string(),
            parent_successor: prepared_candidate.parent_successor,
            generation: Some(generation),
            expected_next_corridor_state_seen: options
                .expected_next_corridor_state_hash
                .map(|_| expected_next_seen),
            best_next_guides,
            next_boundaries: options
                .include_all_candidates
                .then_some(next_turn_boundaries),
        });
    }

    let leaders = guide_leaders(&candidates);
    let selected_indices = candidates
        .iter()
        .filter(|candidate| candidate.exact_target_actions || candidate.equivalent_target_successor)
        .map(|candidate| candidate.candidate_index)
        .chain(
            leaders
                .iter()
                .flat_map(|leader| leader.members.iter().map(|member| member.candidate_index)),
        )
        .collect::<BTreeSet<_>>();
    let reported_candidates = if options.include_all_candidates {
        candidates.clone()
    } else {
        candidates
            .iter()
            .filter(|candidate| selected_indices.contains(&candidate.candidate_index))
            .cloned()
            .collect()
    };
    let target_candidate_indices = candidates
        .iter()
        .filter(|candidate| candidate.exact_target_actions || candidate.equivalent_target_successor)
        .map(|candidate| candidate.candidate_index)
        .collect::<Vec<_>>();
    let incomplete_continuations = candidates
        .iter()
        .filter(|candidate| {
            candidate
                .generation
                .as_ref()
                .is_some_and(|generation| !generation.complete_surface)
        })
        .count();

    Ok(serde_json::json!({
        "schema_name": "OracleBoundedContinuationProbeV1",
        "schema_version": 1,
        "authority": "diagnostic_only",
        "knowledge_mode": "privileged_exact_diagnostic",
        "changes_search_order": false,
        "terminal_authority": false,
        "absence_means_unknown": true,
        "parent": {
            "status": format!("{:?}", options.parent_status),
            "complete_surface": parent_complete,
            "generation_work": options.parent_generation_work,
            "gap_count": options.parent_gap_count,
            "observed_options": parent_options.len(),
            "unique_successors": candidates.len(),
        },
        "continuation": {
            "cohort_mode": if options.representative_cohort { "first_action_family_parent_guides_and_semantic_setup" } else { "all_unique_successors" },
            "observed_first_action_families": observed_family_count,
            "selected_candidate_count": selected_candidate_indices.len(),
            "selected_candidate_indices": selected_candidate_indices,
            "work_per_candidate": options.continuation_work_per_candidate,
            "total_generation_work": total_continuation_work,
            "total_engine_steps": total_continuation_engine_steps,
            "incomplete_candidate_surfaces": incomplete_continuations,
        },
        "target_candidate_indices": target_candidate_indices,
        "expected_next_corridor_state_hash": options.expected_next_corridor_state_hash,
        "guide_leaders": leaders,
        "reported_candidate_count": reported_candidates.len(),
        "all_candidate_count": candidates.len(),
        "candidate_reporting": if options.include_all_candidates { "all" } else { "target_and_guide_leaders" },
        "candidates": reported_candidates,
    }))
}

fn option_action_labels(
    root: &CombatPosition,
    option: &CompleteTurnOption,
    max_engine_steps_per_transition: usize,
) -> Result<Vec<String>, String> {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut labels = Vec::with_capacity(option.actions().len());
    for (action_index, action) in option.actions().iter().enumerate() {
        labels.push(combat_action_label(&position, &action.input));
        let step = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "continuation audit could not replay parent action {action_index}"
            ));
        }
        position = step.position;
    }
    if combat_exact_state_hash_v2(&position.engine, &position.combat)
        != option.exact_successor_hash()
    {
        return Err("continuation audit parent replay successor mismatch".to_string());
    }
    Ok(labels)
}

fn unique_parent_options(options: &[CompleteTurnOption]) -> Vec<CompleteTurnOption> {
    let mut indexes = HashMap::<String, usize>::new();
    let mut unique = Vec::<CompleteTurnOption>::new();
    for option in options {
        let hash = option.exact_successor_hash().to_string();
        if let Some(index) = indexes.get(&hash).copied() {
            let incumbent = &unique[index];
            if option
                .negative_log_policy()
                .total_cmp(&incumbent.negative_log_policy())
                .is_lt()
            {
                unique[index] = option.clone();
            }
        } else {
            indexes.insert(hash, unique.len());
            unique.push(option.clone());
        }
    }
    unique
}

fn continuation_selection_reasons(
    candidates: &[ParentSelectionViewV1<'_>],
    representative_cohort: bool,
) -> BTreeMap<usize, Vec<ContinuationSelectionReasonV1>> {
    if !representative_cohort {
        return candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.candidate_index,
                    vec![ContinuationSelectionReasonV1 {
                        kind: "all_observed_unique_successors",
                        parent_guide_lane: None,
                    }],
                )
            })
            .collect();
    }

    let mut by_family = BTreeMap::<&str, Vec<&ParentSelectionViewV1<'_>>>::new();
    for candidate in candidates {
        by_family
            .entry(candidate.first_action_family)
            .or_default()
            .push(candidate);
    }

    let mut selected = BTreeMap::<usize, Vec<ContinuationSelectionReasonV1>>::new();
    for family in by_family.values() {
        let policy_head = family
            .iter()
            .copied()
            .min_by(|left, right| parent_candidate_tie_break(left, right))
            .expect("first-action family cannot be empty");
        selected
            .entry(policy_head.candidate_index)
            .or_default()
            .push(ContinuationSelectionReasonV1 {
                kind: "first_action_family_policy_head",
                parent_guide_lane: None,
            });

        let guide_lanes = family
            .iter()
            .flat_map(|candidate| candidate.guides.iter().map(|guide| guide.lane))
            .collect::<BTreeSet<_>>();
        for lane in guide_lanes {
            let mut best: Option<(&ParentSelectionViewV1<'_>, &GuideObservationV1)> = None;
            for candidate in family {
                let Some(guide) = candidate.guides.iter().find(|guide| guide.lane == lane) else {
                    continue;
                };
                let replace = best.is_none_or(|(incumbent, incumbent_guide)| {
                    guide.components > incumbent_guide.components
                        || (guide.components == incumbent_guide.components
                            && parent_candidate_tie_break(candidate, incumbent).is_lt())
                });
                if replace {
                    best = Some((candidate, guide));
                }
            }
            let (guide_head, _) = best.expect("observed guide lane must have a member");
            selected
                .entry(guide_head.candidate_index)
                .or_default()
                .push(ContinuationSelectionReasonV1 {
                    kind: "first_action_family_parent_guide_head",
                    parent_guide_lane: Some(lane),
                });
        }
    }

    let mut by_family_setup = BTreeMap::<(&str, &str), Vec<&ParentSelectionViewV1<'_>>>::new();
    for candidate in candidates {
        by_family_setup
            .entry((candidate.first_action_family, candidate.semantic_setup_hash))
            .or_default()
            .push(candidate);
    }
    for setup_family in by_family_setup.values() {
        let minimum_policy = setup_family
            .iter()
            .map(|candidate| candidate.negative_log_policy)
            .min_by(f64::total_cmp)
            .expect("semantic setup family cannot be empty");
        for setup_head in setup_family.iter().copied().filter(|candidate| {
            candidate
                .negative_log_policy
                .total_cmp(&minimum_policy)
                .is_eq()
        }) {
            selected
                .entry(setup_head.candidate_index)
                .or_default()
                .push(ContinuationSelectionReasonV1 {
                    kind: "first_action_family_semantic_setup_policy_head",
                    parent_guide_lane: None,
                });
        }
    }
    selected
}

fn parent_candidate_tie_break(
    left: &ParentSelectionViewV1<'_>,
    right: &ParentSelectionViewV1<'_>,
) -> std::cmp::Ordering {
    left.negative_log_policy
        .total_cmp(&right.negative_log_policy)
        .then_with(|| left.exact_state_hash.cmp(right.exact_state_hash))
        .then_with(|| left.candidate_index.cmp(&right.candidate_index))
}

fn guide_leaders(candidates: &[ContinuationCandidateV1]) -> Vec<GuideLeaderV1> {
    let mut best_ranks = BTreeMap::<u32, Vec<i32>>::new();
    for candidate in candidates {
        for guide in &candidate.best_next_guides {
            let replace = best_ranks
                .get(&guide.lane)
                .is_none_or(|current| guide.best_components > *current);
            if replace {
                best_ranks.insert(guide.lane, guide.best_components.clone());
            }
        }
    }
    best_ranks
        .into_iter()
        .map(|(lane, best_components)| {
            let members = candidates
                .iter()
                .filter_map(|candidate| {
                    let guide = candidate
                        .best_next_guides
                        .iter()
                        .find(|guide| guide.lane == lane)?;
                    (guide.best_components == best_components).then(|| GuideLeaderMemberV1 {
                        candidate_index: candidate.candidate_index,
                        parent_successor_exact_state_hash: candidate
                            .parent_successor_exact_state_hash
                            .clone(),
                        exact_target_actions: candidate.exact_target_actions,
                        equivalent_target_successor: candidate.equivalent_target_successor,
                        best_exact_state_hash: guide.best_exact_state_hash.clone(),
                    })
                })
                .collect();
            GuideLeaderV1 {
                lane,
                best_components,
                members,
            }
        })
        .collect()
}

fn observe_state(
    position: &CombatPosition,
    boundary: CompleteTurnOptionBoundary,
    policy: &dyn sts_combat_planner::CombatActionPolicy,
) -> ResourceObservationV1 {
    let alive_monsters = position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .collect::<Vec<_>>();
    let guides = policy
        .state_guides(position)
        .into_iter()
        .map(|guide| GuideObservationV1 {
            lane: guide.lane.value(),
            components: guide.rank.components().to_vec(),
        })
        .collect();
    ResourceObservationV1 {
        exact_state_hash: combat_exact_state_hash_v2(&position.engine, &position.combat),
        boundary: boundary_label(boundary).to_string(),
        player_turn: position.combat.turn.turn_count,
        player_hp: position.combat.entities.player.current_hp,
        player_max_hp: position.combat.entities.player.max_hp,
        player_block: position.combat.entities.player.block,
        alive_enemy_count: alive_monsters.len(),
        enemy_total_hp: alive_monsters
            .into_iter()
            .map(|monster| monster.current_hp.max(0))
            .sum(),
        potion_slots: position
            .combat
            .entities
            .potions
            .iter()
            .map(|potion| potion.as_ref().map(|potion| format!("{:?}", potion.id)))
            .collect(),
        card_setup: card_setup_observation(&position.combat),
        typed_value_features: typed_feature_components(position),
        guides,
    }
}

fn card_setup_observation(
    combat: &sts_oracle_runtime::runtime::combat::CombatState,
) -> CardSetupObservationV1 {
    let semantic_zones = serde_json::json!({
        "draw_pile": combat.zones.draw_pile.iter().map(semantic_card_value).collect::<Vec<_>>(),
        "hand": combat.zones.hand.iter().map(semantic_card_value).collect::<Vec<_>>(),
        "discard_pile": combat.zones.discard_pile.iter().map(semantic_card_value).collect::<Vec<_>>(),
        "exhaust_pile": combat.zones.exhaust_pile.iter().map(semantic_card_value).collect::<Vec<_>>(),
        "limbo": combat.zones.limbo.iter().map(semantic_card_value).collect::<Vec<_>>(),
    });
    let exact_zones = serde_json::json!({
        "draw_pile": combat.zones.draw_pile.iter().collect::<Vec<_>>(),
        "hand": combat.zones.hand.iter().collect::<Vec<_>>(),
        "discard_pile": combat.zones.discard_pile.iter().collect::<Vec<_>>(),
        "exhaust_pile": combat.zones.exhaust_pile.iter().collect::<Vec<_>>(),
        "limbo": combat.zones.limbo.iter().collect::<Vec<_>>(),
    });
    let live_semantic_cards = semantic_card_counts(
        combat
            .zones
            .draw_pile
            .iter()
            .chain(&combat.zones.hand)
            .chain(&combat.zones.discard_pile)
            .chain(&combat.zones.limbo),
    );
    let exhaust_semantic_cards = semantic_card_counts(combat.zones.exhaust_pile.iter());
    let semantic_live_exhaust = serde_json::json!({
        "live": live_semantic_cards,
        "exhaust": exhaust_semantic_cards,
    });
    CardSetupObservationV1 {
        semantic_live_exhaust_blake2b_512: json_blake2b_512(&semantic_live_exhaust),
        semantic_zone_order_blake2b_512: json_blake2b_512(&semantic_zones),
        exact_zone_order_blake2b_512: json_blake2b_512(&exact_zones),
        live_semantic_cards,
        exhaust_semantic_cards,
    }
}

fn semantic_card_value(
    card: &sts_oracle_runtime::runtime::combat::CombatCard,
) -> serde_json::Value {
    let mut value = serde_json::to_value(card).expect("combat cards must serialize");
    value
        .as_object_mut()
        .expect("combat cards serialize as objects")
        .remove("lineage");
    value
}

fn semantic_card_counts<'a>(
    cards: impl Iterator<Item = &'a sts_oracle_runtime::runtime::combat::CombatCard>,
) -> Vec<SemanticCardCountV1> {
    let mut counts = BTreeMap::<String, (serde_json::Value, usize)>::new();
    for card in cards {
        let value = semantic_card_value(card);
        let key = serde_json::to_string(&value).expect("semantic card values must serialize");
        let (_, count) = counts.entry(key).or_insert((value, 0));
        *count = count.saturating_add(1);
    }
    counts
        .into_values()
        .map(|(card, count)| SemanticCardCountV1 { card, count })
        .collect()
}

fn json_blake2b_512(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).expect("diagnostic JSON values must serialize");
    Blake2b512::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn boundary_label(boundary: CompleteTurnOptionBoundary) -> &'static str {
    match boundary {
        CompleteTurnOptionBoundary::NextPlayerTurn => "next_player_turn",
        CompleteTurnOptionBoundary::TerminalWin => "terminal_win",
        CompleteTurnOptionBoundary::TerminalLoss => "terminal_loss",
        CompleteTurnOptionBoundary::Escape => "escape",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_runtime::content::cards::CardId;
    use sts_oracle_runtime::runtime::combat::CombatCard;
    use sts_oracle_runtime::test_support::blank_test_combat;

    #[test]
    fn card_setup_separates_exact_lineage_from_semantic_zone_changes() {
        let mut first = blank_test_combat();
        first.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Defend, 11)].into();
        let mut second = first.clone();
        second.zones.discard_pile[0].lineage =
            sts_oracle_runtime::runtime::combat::CardLineageId::constructed_scenario(22);

        let first_setup = card_setup_observation(&first);
        let second_setup = card_setup_observation(&second);
        assert_eq!(
            first_setup.semantic_zone_order_blake2b_512,
            second_setup.semantic_zone_order_blake2b_512
        );
        assert_ne!(
            first_setup.exact_zone_order_blake2b_512,
            second_setup.exact_zone_order_blake2b_512
        );

        let mut exhausted = first;
        let card = exhausted.zones.discard_pile.remove(0);
        exhausted.zones.exhaust_pile.push(card);
        let exhausted_setup = card_setup_observation(&exhausted);
        assert_ne!(
            first_setup.semantic_zone_order_blake2b_512,
            exhausted_setup.semantic_zone_order_blake2b_512
        );
        assert_eq!(first_setup.live_semantic_cards[0].count, 1);
        assert_eq!(exhausted_setup.exhaust_semantic_cards[0].count, 1);
    }

    #[test]
    fn representative_cohort_keeps_policy_and_independent_guide_heads_per_family() {
        let family_a_policy_guides = vec![GuideObservationV1 {
            lane: 2,
            components: vec![5],
        }];
        let family_a_guide_head_guides = vec![GuideObservationV1 {
            lane: 2,
            components: vec![9],
        }];
        let family_a_tied_guide_guides = vec![GuideObservationV1 {
            lane: 2,
            components: vec![9],
        }];
        let family_b_guides = vec![GuideObservationV1 {
            lane: 7,
            components: vec![1],
        }];
        let candidates = vec![
            ParentSelectionViewV1 {
                candidate_index: 0,
                first_action_family: "play A",
                negative_log_policy: 1.0,
                exact_state_hash: "a0",
                semantic_setup_hash: "setup-a",
                guides: &family_a_policy_guides,
            },
            ParentSelectionViewV1 {
                candidate_index: 1,
                first_action_family: "play A",
                negative_log_policy: 3.0,
                exact_state_hash: "a1",
                semantic_setup_hash: "setup-a",
                guides: &family_a_guide_head_guides,
            },
            ParentSelectionViewV1 {
                candidate_index: 2,
                first_action_family: "play A",
                negative_log_policy: 4.0,
                exact_state_hash: "a2",
                semantic_setup_hash: "setup-a",
                guides: &family_a_tied_guide_guides,
            },
            ParentSelectionViewV1 {
                candidate_index: 3,
                first_action_family: "end turn",
                negative_log_policy: 2.0,
                exact_state_hash: "b0",
                semantic_setup_hash: "setup-b",
                guides: &family_b_guides,
            },
            ParentSelectionViewV1 {
                candidate_index: 4,
                first_action_family: "play A",
                negative_log_policy: 5.0,
                exact_state_hash: "a3",
                semantic_setup_hash: "setup-c",
                guides: &family_a_policy_guides,
            },
            ParentSelectionViewV1 {
                candidate_index: 5,
                first_action_family: "play A",
                negative_log_policy: 5.0,
                exact_state_hash: "a4",
                semantic_setup_hash: "setup-c",
                guides: &family_a_policy_guides,
            },
        ];

        let selected = continuation_selection_reasons(&candidates, true);

        assert_eq!(
            selected.keys().copied().collect::<Vec<_>>(),
            vec![0, 1, 3, 4, 5]
        );
        assert!(selected[&0]
            .iter()
            .any(|reason| reason.kind == "first_action_family_policy_head"));
        assert!(selected[&1].iter().any(|reason| {
            reason.kind == "first_action_family_parent_guide_head"
                && reason.parent_guide_lane == Some(2)
        }));
        assert_eq!(
            selected[&3]
                .iter()
                .filter(|reason| reason.kind == "first_action_family_parent_guide_head")
                .count(),
            1
        );
        for setup_head in [4, 5] {
            assert!(selected[&setup_head]
                .iter()
                .any(|reason| { reason.kind == "first_action_family_semantic_setup_policy_head" }));
        }

        let all = continuation_selection_reasons(&candidates, false);
        assert_eq!(all.len(), candidates.len());
    }
}
