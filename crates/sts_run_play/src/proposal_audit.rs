use sts_combat_option_grounder::public_combat_agenda_partial_commitment_rank_v1;
use sts_combat_planner::combat_plan_state_guide_policy_v1;
use sts_complete_turn_proposal::{
    ExactCompleteTurnPartialRetentionLaneV1, ExactCompleteTurnPrefixSessionV1,
    ExactCompleteTurnProposalConfigV1, ExactCompleteTurnProposalGuideRankV1,
    ExactCompleteTurnProposalSupportClaimV1, ExactCompleteTurnProposalV1,
};
use sts_oracle_learning::agent::information::action::{
    project_public_combat_actions_v1, resolve_public_combat_action_v1, PublicCombatActionChoiceV1,
};
use sts_oracle_learning::agent::information::state::public_combat_state_v1;
use sts_oracle_learning::agent::task::TaskPublicChoiceV1;
use sts_oracle_learning::agent::task::{public_combat_agenda_for_plan_v1, public_combat_plan_v1};
use sts_oracle_learning::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_learning::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
};
use sts_oracle_learning::state::core::ClientInput;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunPlayProposalAuditConclusionV1 {
    RetainedByProduction,
    DroppedByCompletedEndpointSelection,
    DroppedByPartialPrefixSelection,
    NotGeneratedWithinBound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunPlayProposalAuditLayerV1 {
    pub atomic_depth: usize,
    pub candidate_count: usize,
    pub target_prefix_ordinal: Option<usize>,
    pub guide_competition: Vec<RunPlayProposalAuditGuideCompetitionV1>,
    pub target_retention: Option<RunPlayProposalAuditTargetRetentionV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunPlayProposalAuditGuideCompetitionV1 {
    pub lane: u32,
    pub target_components: Vec<i32>,
    pub stronger_candidates: usize,
    pub tied_candidates: usize,
    pub lane_members: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunPlayProposalAuditTargetRetentionV1 {
    pub max_retained_states: usize,
    pub selected_by: Option<ExactCompleteTurnPartialRetentionLaneV1>,
    pub survival_rank: usize,
    pub progress_rank: usize,
    pub setup_rank: usize,
    pub power_installation_rank: Option<usize>,
    pub resource_transformation_rank: Option<usize>,
    pub pending_choice_continuation_rank: Option<usize>,
    pub pending_choice_selection_rank: Option<usize>,
    pub balanced_rank: usize,
    pub first_action_balanced_rank: usize,
    pub guide_conditioned_ranks: Vec<RunPlayProposalAuditGuideConditionedRanksV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunPlayProposalAuditGuideConditionedRanksV1 {
    pub lane: u32,
    pub peer_count: usize,
    pub survival_rank: usize,
    pub progress_rank: usize,
    pub setup_rank: usize,
    pub balanced_rank: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunPlayProposalAuditTraceV1 {
    pub target_reserved: bool,
    pub layers: Vec<RunPlayProposalAuditLayerV1>,
    pub completed_endpoint_count: usize,
    pub target_completed_endpoint_ordinal: Option<usize>,
    pub target_completed_endpoint_audit: Option<RunPlayProposalAuditCompletedEndpointV1>,
    pub selected_endpoint_count: usize,
    pub completed_endpoint_reserved_candidate_count: usize,
    pub resource_profile_retained_candidate_count: usize,
    pub support_retained_candidate_count: usize,
    pub target_selected: bool,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub exact_state_skips: usize,
    pub truncated_children: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunPlayProposalAuditCompletedEndpointV1 {
    pub potion_slot_mask: u64,
    pub potion_expenditures: u32,
    pub support_claim_count: usize,
    pub guide_competition: Vec<RunPlayProposalAuditGuideCompetitionV1>,
    pub resource_profile_guide_undominated_count: usize,
    pub support_guide_undominated_count: usize,
}

#[derive(Clone)]
struct CompletedEndpointAuditProjectionV1 {
    target: bool,
    potion_slot_mask: u64,
    potion_expenditures: u32,
    support_claims: Vec<ExactCompleteTurnProposalSupportClaimV1>,
    guides: Vec<ExactCompleteTurnProposalGuideRankV1>,
}

type ProposalGuideProviderV1<'a> =
    dyn Fn(&CombatPosition) -> Vec<ExactCompleteTurnProposalGuideRankV1> + 'a;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunPlayProposalAuditV1 {
    pub target_action_count: usize,
    pub production: RunPlayProposalAuditTraceV1,
    pub target_reserved: RunPlayProposalAuditTraceV1,
    pub conclusion: RunPlayProposalAuditConclusionV1,
}

pub fn audit_complete_turn_target_v1(
    root: &CombatPosition,
    choices: &[TaskPublicChoiceV1],
    config: ExactCompleteTurnProposalConfigV1,
) -> Result<RunPlayProposalAuditV1, String> {
    let target_inputs =
        resolve_target_inputs_v1(root, choices, config.max_engine_steps_per_action)?;
    let root_public = public_combat_state_v1(&root.combat);
    let plan = public_combat_plan_v1(&root_public);
    let agenda = public_combat_agenda_for_plan_v1(&root_public, &plan);
    let production_guides = |partial: &CombatPosition| {
        public_combat_agenda_partial_commitment_rank_v1(&agenda, partial)
            .into_iter()
            .flat_map(|rank| rank.guide_ranks())
            .map(|(lane, components)| ExactCompleteTurnProposalGuideRankV1 { lane, components })
            .collect()
    };
    let completed_policy = combat_plan_state_guide_policy_v1(retained_combat_tactical_policy_v1());
    let production_completed_guides = |successor: &CombatPosition| {
        completed_policy
            .state_guides(successor)
            .into_iter()
            .map(|guide| ExactCompleteTurnProposalGuideRankV1 {
                lane: guide.lane.value(),
                components: guide.rank.components().to_vec(),
            })
            .collect()
    };
    let production = trace_target_membership_v1(
        root,
        &target_inputs,
        config,
        false,
        Some(&production_guides),
        Some(&production_completed_guides),
    )?;
    let target_reserved =
        trace_target_membership_v1(root, &target_inputs, config, true, None, None)?;
    let conclusion = if production.target_selected {
        RunPlayProposalAuditConclusionV1::RetainedByProduction
    } else if production.target_completed_endpoint_ordinal.is_some() {
        RunPlayProposalAuditConclusionV1::DroppedByCompletedEndpointSelection
    } else if target_reserved.target_completed_endpoint_ordinal.is_some() {
        RunPlayProposalAuditConclusionV1::DroppedByPartialPrefixSelection
    } else {
        RunPlayProposalAuditConclusionV1::NotGeneratedWithinBound
    };
    Ok(RunPlayProposalAuditV1 {
        target_action_count: target_inputs.len(),
        production,
        target_reserved,
        conclusion,
    })
}

/// Materializes one caller-supplied public complete-turn line through the
/// bounded prefix generator while reserving that exact prefix at every
/// partial layer. The returned candidate is still generated and replayed by
/// the complete-turn owner; the caller's semantic line is not promoted to an
/// executable candidate by itself.
pub(crate) fn reserve_complete_turn_target_v1(
    root: &CombatPosition,
    choices: &[TaskPublicChoiceV1],
    config: ExactCompleteTurnProposalConfigV1,
) -> Result<ExactCompleteTurnProposalV1, String> {
    let target_inputs =
        resolve_target_inputs_v1(root, choices, config.max_engine_steps_per_action)?;
    let mut session = ExactCompleteTurnPrefixSessionV1::new(root, config);
    while let Some(layer) = session.observe_layer() {
        let target_prefix_matches = layer.try_project_candidates(|inputs, _position| {
            Ok(inputs.len() <= target_inputs.len() && inputs == &target_inputs[..inputs.len()])
        })?;
        let target_prefix_ordinals = target_prefix_matches
            .iter()
            .enumerate()
            .filter_map(|(ordinal, matches)| matches.then_some(ordinal))
            .collect::<Vec<_>>();
        if target_prefix_ordinals.len() > 1 {
            return Err(format!(
                "combat-turn reservation found duplicate target prefixes at atomic depth {}",
                layer.atomic_depth()
            ));
        }
        session.retain_layer_with_default_fill(&target_prefix_ordinals)?;
    }
    let completed = session.finish_completed_endpoints(&|_| Vec::new())?;
    let candidates =
        completed.try_project_candidates(|candidate, _guides| Ok(candidate.clone()))?;
    let mut matches = candidates.into_iter().filter(|candidate| {
        candidate.actions.len() == target_inputs.len()
            && candidate
                .actions
                .iter()
                .zip(&target_inputs)
                .all(|(action, target)| action.input == *target)
    });
    let candidate = matches.next().ok_or_else(|| {
        "combat-turn reservation line did not reach a generated complete-turn endpoint".to_string()
    })?;
    if matches.next().is_some() {
        return Err("combat-turn reservation generated duplicate exact endpoints".to_string());
    }
    Ok(candidate)
}

pub(crate) fn resolve_target_inputs_v1(
    root: &CombatPosition,
    choices: &[TaskPublicChoiceV1],
    max_engine_steps_per_action: usize,
) -> Result<Vec<ClientInput>, String> {
    if choices.is_empty() {
        return Err("run-play proposal audit requires a non-empty complete-turn line".to_string());
    }
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut inputs = Vec::with_capacity(choices.len());
    for (index, choice) in choices.iter().enumerate() {
        let candidate_ordinal = choice
            .ordinal_path
            .first()
            .copied()
            .ok_or_else(|| format!("proposal audit choice {} has an empty path", index + 1))?;
        let projection = project_public_combat_actions_v1(&position.engine, &position.combat)
            .map_err(|error| {
                format!(
                    "proposal audit could not project combat action {}: {error}",
                    index + 1
                )
            })?;
        let action_choice = if candidate_ordinal < projection.public.atomic_actions.len() {
            if choice.ordinal_path.len() != 1 {
                return Err(format!(
                    "proposal audit atomic action {} has a structured payload",
                    index + 1
                ));
            }
            PublicCombatActionChoiceV1::Atomic {
                action_ordinal: candidate_ordinal,
            }
        } else {
            PublicCombatActionChoiceV1::Selection {
                family_ordinal: candidate_ordinal - projection.public.atomic_actions.len(),
                selected_domain_ordinals: choice.ordinal_path[1..].to_vec(),
            }
        };
        let input =
            resolve_public_combat_action_v1(&projection, &action_choice).map_err(|error| {
                format!(
                    "proposal audit could not resolve action {}: {error}",
                    index + 1
                )
            })?;
        let step = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_action,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "proposal audit action {} did not reach a stable exact boundary",
                index + 1
            ));
        }
        inputs.push(input);
        position = step.position;
    }
    Ok(inputs)
}

fn trace_target_membership_v1(
    root: &CombatPosition,
    target_inputs: &[ClientInput],
    config: ExactCompleteTurnProposalConfigV1,
    reserve_target: bool,
    partial_guides: Option<&ProposalGuideProviderV1<'_>>,
    completed_guides: Option<&ProposalGuideProviderV1<'_>>,
) -> Result<RunPlayProposalAuditTraceV1, String> {
    let mut session = ExactCompleteTurnPrefixSessionV1::new(root, config);
    let mut layers = Vec::new();
    while let Some(layer) = session.observe_layer() {
        let target_prefix_matches = layer.try_project_candidates(|inputs, _position| {
            Ok(inputs.len() <= target_inputs.len() && inputs == &target_inputs[..inputs.len()])
        })?;
        let target_prefix_ordinals = target_prefix_matches
            .iter()
            .enumerate()
            .filter_map(|(ordinal, matches)| matches.then_some(ordinal))
            .collect::<Vec<_>>();
        if target_prefix_ordinals.len() > 1 {
            return Err(format!(
                "proposal audit found duplicate target prefixes at atomic depth {}",
                layer.atomic_depth()
            ));
        }
        let target_prefix_ordinal = target_prefix_ordinals.first().copied();
        let guide_competition =
            if let (Some(target_ordinal), Some(guides)) = (target_prefix_ordinal, partial_guides) {
                let projected = layer.try_project_candidates(|_, position| Ok(guides(position)))?;
                projected
                    .get(target_ordinal)
                    .into_iter()
                    .flatten()
                    .map(|target| {
                        let lane_ranks = projected
                            .iter()
                            .filter_map(|ranks| ranks.iter().find(|rank| rank.lane == target.lane))
                            .collect::<Vec<_>>();
                        RunPlayProposalAuditGuideCompetitionV1 {
                            lane: target.lane,
                            target_components: target.components.clone(),
                            stronger_candidates: lane_ranks
                                .iter()
                                .filter(|rank| rank.components > target.components)
                                .count(),
                            tied_candidates: lane_ranks
                                .iter()
                                .filter(|rank| rank.components == target.components)
                                .count(),
                            lane_members: lane_ranks.len(),
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
        let target_retention = if let Some(guides) = partial_guides {
            let retention = session.retain_layer_with_guides(guides);
            target_prefix_ordinal
                .and_then(|ordinal| retention.candidates.get(ordinal))
                .map(|candidate| RunPlayProposalAuditTargetRetentionV1 {
                    max_retained_states: retention.max_retained_states,
                    selected_by: candidate.selected_by,
                    survival_rank: candidate.survival_rank,
                    progress_rank: candidate.progress_rank,
                    setup_rank: candidate.setup_rank,
                    power_installation_rank: candidate.power_installation_rank,
                    resource_transformation_rank: candidate.resource_transformation_rank,
                    pending_choice_continuation_rank: candidate.pending_choice_continuation_rank,
                    pending_choice_selection_rank: candidate.pending_choice_selection_rank,
                    balanced_rank: candidate.balanced_rank,
                    first_action_balanced_rank: candidate.first_action_balanced_rank,
                    guide_conditioned_ranks: candidate
                        .guide_conditioned_ranks
                        .iter()
                        .map(|conditioned| RunPlayProposalAuditGuideConditionedRanksV1 {
                            lane: conditioned.lane,
                            peer_count: conditioned.peer_count,
                            survival_rank: conditioned.survival_rank,
                            progress_rank: conditioned.progress_rank,
                            setup_rank: conditioned.setup_rank,
                            balanced_rank: conditioned.balanced_rank,
                        })
                        .collect(),
                })
        } else {
            let reservations = if reserve_target {
                target_prefix_ordinals.as_slice()
            } else {
                Default::default()
            };
            session.retain_layer_with_default_fill(reservations)?;
            None
        };
        layers.push(RunPlayProposalAuditLayerV1 {
            atomic_depth: layer.atomic_depth(),
            candidate_count: layer.candidate_count(),
            target_prefix_ordinal,
            guide_competition,
            target_retention,
        });
    }

    let completed = session.finish_completed_endpoints(&|successor| {
        completed_guides.map_or_else(Vec::new, |guides| guides(successor))
    })?;
    let completed_projection = completed.try_project_candidates(|candidate, guides| {
        Ok(CompletedEndpointAuditProjectionV1 {
            target: candidate.actions.len() == target_inputs.len()
                && candidate
                    .actions
                    .iter()
                    .zip(target_inputs)
                    .all(|(action, target)| action.input == *target),
            potion_slot_mask: candidate.potion_slot_mask,
            potion_expenditures: candidate.potion_expenditures,
            support_claims: candidate.support_claims.clone(),
            guides: guides.to_vec(),
        })
    })?;
    let target_completed_ordinals = completed_projection
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| candidate.target.then_some(ordinal))
        .collect::<Vec<_>>();
    if target_completed_ordinals.len() > 1 {
        return Err("proposal audit found duplicate completed target endpoints".to_string());
    }
    let target_completed_endpoint_ordinal = target_completed_ordinals.first().copied();
    let target_completed_endpoint_audit = target_completed_endpoint_ordinal
        .map(|ordinal| completed_endpoint_audit_v1(&completed_projection, ordinal));
    let completed_endpoint_count = completed.candidate_count();
    let endpoint_reservation = if reserve_target {
        target_completed_ordinals.as_slice()
    } else {
        &[]
    };
    let batch = completed.retain_with_default_fill(endpoint_reservation)?;
    let target_selected = batch.candidates.iter().any(|candidate| {
        candidate.actions.len() == target_inputs.len()
            && candidate
                .actions
                .iter()
                .zip(target_inputs)
                .all(|(action, target)| action.input == *target)
    });
    Ok(RunPlayProposalAuditTraceV1 {
        target_reserved: reserve_target,
        layers,
        completed_endpoint_count,
        target_completed_endpoint_ordinal,
        target_completed_endpoint_audit,
        selected_endpoint_count: batch.candidates.len(),
        completed_endpoint_reserved_candidate_count: batch
            .completed_endpoint_reserved_candidate_count,
        resource_profile_retained_candidate_count: batch.resource_profile_retained_candidate_count,
        support_retained_candidate_count: batch.support_retained_candidate_count,
        target_selected,
        nodes_expanded: batch.nodes_expanded,
        nodes_generated: batch.nodes_generated,
        exact_state_skips: batch.exact_state_skips,
        truncated_children: batch.truncated_children,
    })
}

fn completed_endpoint_audit_v1(
    candidates: &[CompletedEndpointAuditProjectionV1],
    target_ordinal: usize,
) -> RunPlayProposalAuditCompletedEndpointV1 {
    let target = &candidates[target_ordinal];
    let guide_competition = target
        .guides
        .iter()
        .map(|target_guide| {
            let lane_members = candidates
                .iter()
                .filter_map(|candidate| {
                    candidate
                        .guides
                        .iter()
                        .find(|guide| guide.lane == target_guide.lane)
                })
                .collect::<Vec<_>>();
            RunPlayProposalAuditGuideCompetitionV1 {
                lane: target_guide.lane,
                target_components: target_guide.components.clone(),
                stronger_candidates: lane_members
                    .iter()
                    .filter(|guide| guide.components > target_guide.components)
                    .count(),
                tied_candidates: lane_members
                    .iter()
                    .filter(|guide| guide.components == target_guide.components)
                    .count(),
                lane_members: lane_members.len(),
            }
        })
        .collect::<Vec<_>>();
    let resource_profile_guide_undominated_count = target
        .guides
        .iter()
        .filter(|target_guide| {
            !candidates.iter().any(|candidate| {
                candidate.potion_slot_mask == target.potion_slot_mask
                    && candidate.potion_expenditures == target.potion_expenditures
                    && candidate.guides.iter().any(|guide| {
                        guide.lane == target_guide.lane
                            && guide.components > target_guide.components
                    })
            })
        })
        .count();
    let support_guide_undominated_count = target
        .support_claims
        .iter()
        .flat_map(|claim| {
            target
                .guides
                .iter()
                .map(move |target_guide| (claim, target_guide))
        })
        .filter(|(claim, target_guide)| {
            !candidates.iter().any(|candidate| {
                candidate.support_claims.contains(claim)
                    && candidate.guides.iter().any(|guide| {
                        guide.lane == target_guide.lane
                            && guide.components > target_guide.components
                    })
            })
        })
        .count();
    RunPlayProposalAuditCompletedEndpointV1 {
        potion_slot_mask: target.potion_slot_mask,
        potion_expenditures: target.potion_expenditures,
        support_claim_count: target.support_claims.len(),
        guide_competition,
        resource_profile_guide_undominated_count,
        support_guide_undominated_count,
    }
}

pub fn render_proposal_audit_v1(audit: &RunPlayProposalAuditV1) -> String {
    let mut output = format!(
        "proposal_audit target_actions={} conclusion={:?}\n",
        audit.target_action_count, audit.conclusion
    );
    for trace in [&audit.production, &audit.target_reserved] {
        output.push_str(&format!(
            "trace mode={} nodes={}/{} exact_skips={} truncated_children={} completed={}/{} reserved={} resource_profiles={} support={} target_completed={:?} target_selected={}\n",
            if trace.target_reserved { "target_reserved" } else { "production" },
            trace.nodes_expanded,
            trace.nodes_generated,
            trace.exact_state_skips,
            trace.truncated_children,
            trace.selected_endpoint_count,
            trace.completed_endpoint_count,
            trace.completed_endpoint_reserved_candidate_count,
            trace.resource_profile_retained_candidate_count,
            trace.support_retained_candidate_count,
            trace.target_completed_endpoint_ordinal,
            trace.target_selected,
        ));
        if let Some(endpoint) = &trace.target_completed_endpoint_audit {
            output.push_str(&format!(
                "  completed_target potion_mask=0x{:x} potion_expenditures={} support_claims={} resource_guide_undominated={} support_guide_undominated={}\n",
                endpoint.potion_slot_mask,
                endpoint.potion_expenditures,
                endpoint.support_claim_count,
                endpoint.resource_profile_guide_undominated_count,
                endpoint.support_guide_undominated_count,
            ));
            for competition in &endpoint.guide_competition {
                output.push_str(&format!(
                    "    completed_guide=0x{:08x} rank={:?} stronger={} tied={} members={}\n",
                    competition.lane,
                    competition.target_components,
                    competition.stronger_candidates,
                    competition.tied_candidates,
                    competition.lane_members,
                ));
            }
        }
        for layer in &trace.layers {
            match layer.target_prefix_ordinal {
                Some(ordinal) => {
                    output.push_str(&format!(
                        "  depth={} candidates={} target_prefix={}\n",
                        layer.atomic_depth, layer.candidate_count, ordinal,
                    ));
                    for competition in &layer.guide_competition {
                        output.push_str(&format!(
                            "    guide=0x{:08x} rank={:?} stronger={} tied={} members={}\n",
                            competition.lane,
                            competition.target_components,
                            competition.stronger_candidates,
                            competition.tied_candidates,
                            competition.lane_members,
                        ));
                    }
                    if let Some(retention) = &layer.target_retention {
                        output.push_str(&format!(
                            "    retention selected_by={:?} width={} ranks=survival:{} progress:{} setup:{} power_installation:{:?} resource_transformation:{:?} pending_choice_continuation:{:?} pending_choice_selection:{:?} balanced:{} first_action_balanced:{}\n",
                            retention.selected_by,
                            retention.max_retained_states,
                            retention.survival_rank.saturating_add(1),
                            retention.progress_rank.saturating_add(1),
                            retention.setup_rank.saturating_add(1),
                            retention
                                .power_installation_rank
                                .map(|rank| rank.saturating_add(1)),
                            retention
                                .resource_transformation_rank
                                .map(|rank| rank.saturating_add(1)),
                            retention
                                .pending_choice_continuation_rank
                                .map(|rank| rank.saturating_add(1)),
                            retention
                                .pending_choice_selection_rank
                                .map(|rank| rank.saturating_add(1)),
                            retention.balanced_rank.saturating_add(1),
                            retention.first_action_balanced_rank.saturating_add(1),
                        ));
                        for conditioned in &retention.guide_conditioned_ranks {
                            output.push_str(&format!(
                                "      conditioned guide=0x{:08x} peers={} ranks=survival:{} progress:{} setup:{} balanced:{}\n",
                                conditioned.lane,
                                conditioned.peer_count,
                                conditioned.survival_rank.saturating_add(1),
                                conditioned.progress_rank.saturating_add(1),
                                conditioned.setup_rank.saturating_add(1),
                                conditioned.balanced_rank.saturating_add(1),
                            ));
                        }
                    }
                }
                None if trace.target_completed_endpoint_ordinal.is_some()
                    && layer.atomic_depth >= audit.target_action_count =>
                {
                    output.push_str(&format!(
                        "  depth={} candidates={} target=completed_endpoint\n",
                        layer.atomic_depth, layer.candidate_count,
                    ));
                    break;
                }
                None => {
                    output.push_str(&format!(
                        "  depth={} candidates={} target=dropped\n",
                        layer.atomic_depth, layer.candidate_count,
                    ));
                    break;
                }
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_learning::content::{cards::CardId, monsters::EnemyId};
    use sts_oracle_learning::runtime::combat::CombatCard;
    use sts_oracle_learning::state::core::EngineState;

    #[test]
    fn target_reservation_carries_a_generated_complete_turn_to_the_endpoint() {
        let mut combat = sts_oracle_learning::test_support::blank_test_combat();
        let monster = sts_oracle_learning::test_support::planned_monster(EnemyId::JawWorm, 1);
        let target = monster.id;
        combat.entities.monsters = vec![monster];
        combat.turn.energy = 1;
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 7)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let config = ExactCompleteTurnProposalConfigV1 {
            max_inner_nodes: 64,
            max_partial_frontier_states: 64,
            max_end_states: 64,
            per_bucket_limit: 64,
            ..ExactCompleteTurnProposalConfigV1::default()
        };
        let target_inputs = vec![
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
            ClientInput::EndTurn,
        ];

        let trace =
            trace_target_membership_v1(&root, &target_inputs, config, true, None, None).unwrap();

        assert!(trace.target_completed_endpoint_ordinal.is_some());
        assert!(trace.target_selected);
    }

    #[test]
    fn compact_render_stops_at_drop_or_completed_endpoint() {
        let layer = |depth, target_prefix_ordinal| RunPlayProposalAuditLayerV1 {
            atomic_depth: depth,
            candidate_count: 10,
            target_prefix_ordinal,
            guide_competition: Vec::new(),
            target_retention: None,
        };
        let trace = |target_reserved, completed, layers| RunPlayProposalAuditTraceV1 {
            target_reserved,
            layers,
            completed_endpoint_count: 5,
            target_completed_endpoint_ordinal: completed,
            target_completed_endpoint_audit: None,
            selected_endpoint_count: 5,
            completed_endpoint_reserved_candidate_count: usize::from(target_reserved),
            resource_profile_retained_candidate_count: 0,
            support_retained_candidate_count: 0,
            target_selected: completed.is_some(),
            nodes_expanded: 1,
            nodes_generated: 2,
            exact_state_skips: 0,
            truncated_children: 0,
        };
        let audit = RunPlayProposalAuditV1 {
            target_action_count: 2,
            production: trace(
                false,
                None,
                vec![layer(1, Some(0)), layer(2, None), layer(3, None)],
            ),
            target_reserved: trace(
                true,
                Some(4),
                vec![layer(1, Some(0)), layer(2, None), layer(3, None)],
            ),
            conclusion: RunPlayProposalAuditConclusionV1::DroppedByPartialPrefixSelection,
        };

        let rendered = render_proposal_audit_v1(&audit);

        assert!(rendered.contains("depth=2 candidates=10 target=dropped"));
        assert!(rendered.contains("depth=2 candidates=10 target=completed_endpoint"));
        assert!(!rendered.contains("depth=3"));
    }
}
