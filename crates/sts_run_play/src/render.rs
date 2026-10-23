use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use sts_presentation::Locale;

use sts_oracle_learning::agent::information::action::{
    PublicCombatAtomicActionV1, PublicCombatIndexedChoiceCandidateV1,
    PublicCombatSelectionDomainCandidateV1, PublicHandCopyProjectionV1,
};
use sts_oracle_learning::agent::information::combat::ObservationEvidenceKindV1;
use sts_oracle_learning::agent::information::effect::{
    PublicCombatCandidateEffectV1, PublicCombatXCostPaymentV1,
};
use sts_oracle_learning::agent::information::state::{
    CombatLearningCardCollectionV1, CombatLearningCardV1, CombatLearningEnemyIdentityV1,
    CombatLearningPowerV1, PublicCombatStateV1,
};
use sts_oracle_learning::agent::task::{
    task_research_plan_non_dominated_candidate_ordinals_v1, TaskDecisionSiteV1,
    TaskPrivilegedStrategicSupportOutcomeV1, TaskPublicCandidateV1, TaskPublicChoiceV1,
    TaskPublicCombatCandidateEffectV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskRunArchiveNodeProofV1, TaskRunSolutionArchiveV1, TaskStrategicActionV1,
    TaskStrategicObservationV1,
};
use sts_oracle_learning::ai::planner_core::{
    PlannerEventCardPoolV1, PlannerEventOptionStructuredSemanticsV1, PlannerEventPublicEffectV1,
    PlannerEventRelicPoolV1, PlannerEventSelectionV1, PlannerEventTransitionV1, PlannerPublicMap,
    PlannerRewardDescriptor, PlannerShopPurchaseBlockReasonV1, PlannerShopPurchaseStatusV1,
};
use sts_oracle_learning::content::identity::{gameplay_display_label_v1, GameplayIdentitySurface};
use sts_oracle_learning::content::powers::PowerId;
use sts_oracle_learning::content::relics::RelicId;
use sts_oracle_learning::eval::run_control::{
    BossRelicActionSupplyDeltaV1, BossRelicStartupDeltaV1, CardRewardPolicyAcquisitionV1,
    ExactBossRelicPolicyAuditV1, ExactCampfirePolicyAuditV1, ExactCardRewardPolicyAuditV1,
    ExactRoutePolicyAuditV1, ExactShopPolicyAuditV1, PotionRewardPolicyAuditV1,
    PotionRewardPolicyDecisionV1, RoutePolicyActionV1, TaskRunBranchDecisionInventoryV1,
};
use sts_oracle_learning::sim::combat_action_surface::CombatSelectionReasonV2;
use sts_oracle_learning::state::HandSelectReason;

pub(crate) fn run_selection_reward_label_v1(
    observation: &TaskStrategicObservationV1,
    card: sts_oracle_learning::content::cards::CardId,
    locale: Locale,
) -> Option<String> {
    use sts_oracle_learning::ai::planner_core::PlannerDecisionContext;
    use sts_oracle_learning::content::events::bonfire_elementals::{
        describe_offer_reward, offer_reward_for_rarity,
    };
    use sts_oracle_learning::state::{events::EventId, selection::DomainEventSource};
    let PlannerDecisionContext::RunChoice {
        source: Some(DomainEventSource::Event(EventId::BonfireElementals | EventId::BonfireSpirits)),
        ..
    } = observation.context
    else {
        return None;
    };
    let rarity = sts_oracle_learning::content::cards::get_card_definition(card).rarity;
    let owns = |id| observation.relics.iter().any(|relic| relic.relic == id);
    Some(
        locale
            .text(
                "event",
                "bonfire.selectedCard",
                &[
                    (
                        "rarity",
                        locale.entity("cardRarity", rarity.vocabulary_token_v1()),
                    ),
                    (
                        "reward",
                        &describe_offer_reward(
                            offer_reward_for_rarity(rarity, owns(RelicId::SpiritPoop)),
                            owns(RelicId::MarkOfTheBloom),
                            locale,
                        ),
                    ),
                ],
            )
            .expect("bonfire selected-card reward"),
    )
}

use crate::compact::render_comparison_v1;
use crate::driver::{
    combat_research_candidate_id_v1, RunPlayAdviceDetailV1, RunPlayCombatHistoryV1,
    RunPlayCombatResearchAttemptV1, RunPlayConstructionHistoryV1,
};
use crate::model::{
    RunPlayBoundaryKindV1, RunPlayEventV1, RunPlayHeadV1, RunPlayPathStepV1, RunPlayPositionV1,
    RunPlayStrategicOwnerAdviceV1,
};
use crate::selector::{
    canonical_upgraded_name_v1, semantic_action_for_choice_v1,
    transactional_play_alias_for_choice_v1,
};

pub fn render_show_v1(
    head: &crate::model::RunPlayHeadV1,
    locale: Locale,
) -> Result<String, String> {
    let mut output = String::new();
    let mode = if head.scratch {
        "mode=scratch".to_string()
    } else {
        format!(
            "evidence_compatibility={}",
            crate::model::RunPlayEvidenceIdentityV1::compatibility_label_v1(
                head.evidence_identity.as_ref(),
            )
        )
    };
    writeln!(
        output,
        "run-play {} revision=h{} state={} node=n{} {}",
        head.session_id,
        head.head_revision,
        head.state_revision,
        short_id(head.position.archive_node_id()),
        mode
    )
    .expect("string write");
    match &head.position {
        RunPlayPositionV1::Terminal { outcome, .. } => {
            writeln!(
                output,
                "{}",
                locale
                    .text(
                        "compact",
                        "show.terminal",
                        &[(
                            "outcome",
                            &crate::compact::render_outcome_compact(outcome, locale)
                        )]
                    )
                    .expect("terminal display message")
            )
            .expect("string write");
        }
        RunPlayPositionV1::Decision { archive_node_id } => {
            let decision = head.current_decision.as_ref().ok_or_else(|| {
                format!("run-play head has no cached decision for {archive_node_id}")
            })?;
            if head.scratch
                || crate::model::RunPlayEvidenceIdentityV1::require_current_v1(
                    head.evidence_identity.as_ref(),
                )
                .is_ok()
            {
                let public = decision.public_decision_v1()?;
                let reading =
                    crate::decision::read_preview_decision_v1(&public, None, false, locale)?;
                output.push_str(&reading.view);
            } else {
                // Retained, unverified sources keep their fixed diagnostic
                // representation; reading them does not reproject a new runtime.
                render_decision_v1(&mut output, decision)?;
            }
        }
    }
    Ok(output)
}

pub fn render_advice_v1(
    advice: &RunPlayStrategicOwnerAdviceV1,
    decision: &TaskRunBranchDecisionInventoryV1,
    locale: Locale,
) -> Result<String, String> {
    let public = decision.public_decision_v1()?;
    let text = |key: &str, parameters: &[(&str, &str)]| {
        locale
            .text("compact", &format!("advice.{key}"), parameters)
            .expect("owner advice message")
    };
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        text(
            "provenance",
            &[
                ("producer", &advice.support.provenance.producer_name),
                (
                    "revision",
                    &advice.support.provenance.producer_revision.to_string()
                ),
                ("access", &format!("{:?}", advice.support.provenance.access)),
                (
                    "snapshot",
                    short_id(&advice.support.root_public_snapshot_id)
                )
            ]
        )
    )
    .expect("string write");
    match &advice.support.outcome {
        TaskPrivilegedStrategicSupportOutcomeV1::Choice { choice } => {
            let root = *choice
                .ordinal_path
                .first()
                .ok_or_else(|| "owner advice choice is empty".to_string())?;
            let candidate = public
                .candidates
                .get(root)
                .ok_or_else(|| "owner advice root candidate is missing".to_string())?;
            let TaskPublicObservationV1::Strategic { observation } = &public.observation else {
                return Err("strategic owner advice requires a strategic observation".into());
            };
            let action = advice
                .semantic_action
                .as_deref()
                .ok_or_else(|| "owner advice choice has no stable semantic action".to_string())?;
            let label = crate::decision::option_label(
                candidate,
                observation,
                &public.observation,
                false,
                locale,
            )?;
            writeln!(
                output,
                "{}",
                text("choice", &[("action", action), ("candidate", &label)])
            )
            .expect("string write");
        }
        TaskPrivilegedStrategicSupportOutcomeV1::Unavailable { reason } => {
            writeln!(
                output,
                "{}",
                text("unavailable", &[("reason", &format!("{reason:?}"))])
            )
            .expect("string write");
        }
    }
    Ok(output)
}

pub fn render_advice_detail_v1(detail: &RunPlayAdviceDetailV1, locale: Locale) -> String {
    match detail {
        RunPlayAdviceDetailV1::Route(audit) => render_route_advice_detail_v1(audit),
        RunPlayAdviceDetailV1::CardReward(audit) => render_card_reward_advice_detail_v1(audit),
        RunPlayAdviceDetailV1::Campfire(audit) => render_campfire_advice_detail_v1(audit),
        RunPlayAdviceDetailV1::BossRelic(audit) => render_boss_relic_advice_detail_v1(audit),
        RunPlayAdviceDetailV1::Shop(audit) => render_shop_advice_detail_v1(audit),
        RunPlayAdviceDetailV1::RunChoice(audit) => render_run_choice_advice_detail_v1(audit),
        RunPlayAdviceDetailV1::PotionReward(audit) => render_potion_reward_advice_detail_v1(audit),
        RunPlayAdviceDetailV1::Event(audit) => {
            let mut output = audit
                .direct_card_acquisition
                .as_ref()
                .map(render_card_reward_advice_detail_v1)
                .unwrap_or_default();
            output.push_str(&format!(
                "event_frontier lines={} max_steps=12 max_lines=64 truncated={}\n",
                audit.comparison.lines.len(),
                audit.truncated,
            ));
            match render_comparison_v1(&audit.comparison, true, locale) {
                Ok(rendered) => output.push_str(&rendered),
                Err(error) => {
                    writeln!(output, "event_frontier_render_error={error}").expect("string write");
                }
            }
            output
        }
        RunPlayAdviceDetailV1::Unavailable { site } => {
            format!("owner_detail unavailable_for_site={site:?}\n")
        }
    }
}

fn render_run_choice_advice_detail_v1(
    audit: &sts_oracle_learning::eval::run_control::ExactRunChoicePolicyAuditV1,
) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "run_choice reason={} choices={}..={} candidates={} selected={}",
        audit.reason,
        audit.min_choices,
        audit.max_choices,
        audit.candidates.len(),
        audit.selected_plan_id.as_deref().unwrap_or("-")
    )
    .expect("string write");
    for candidate in &audit.candidates {
        writeln!(
            output,
            "run_choice_rank={} selected={} label={} deck={:?} role={} score={} confidence={:.2} allowed=[{}]",
            candidate.owner_rank,
            candidate.selected,
            candidate.label,
            candidate.deck_indices,
            candidate.role,
            candidate.score_hint,
            candidate.confidence,
            candidate.allowed_consumers.join(", ")
        )
        .expect("string write");
        for duplicate in &candidate.duplicate_targets {
            writeln!(
                output,
                "  duplicate={:?}+{} priority={} premium={} role={} stack={} reasons=[{}] risks=[{}]",
                duplicate.card,
                duplicate.upgrades,
                duplicate.priority,
                duplicate.premium,
                duplicate.role,
                duplicate.stack_behavior,
                duplicate.reasons.join(", "),
                duplicate.risks.join(", ")
            )
            .expect("string write");
        }
        writeln!(
            output,
            "  target=cards:[{}] class:[{}] removal_benefit:[{}] loss:[{}] reasons=[{}] risks=[{}]",
            candidate.cards.join(", "),
            candidate.target_classes.join(", "),
            candidate.removal_benefits.join(", "),
            candidate.target_losses.join(", "),
            candidate.reasons.join(", "),
            candidate.risks.join(", ")
        )
        .expect("string write");
    }
    output
}

fn render_potion_reward_advice_detail_v1(audit: &PotionRewardPolicyAuditV1) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "potion_reward incoming={} band={:?} traits={:?} support={:?} decision={:?}",
        format_identity(audit.incoming.potion),
        audit.incoming.band,
        audit.incoming.traits,
        audit.incoming.deck_support,
        audit.decision,
    )
    .expect("string write");
    for held in &audit.held {
        let selected = matches!(
            audit.decision,
            PotionRewardPolicyDecisionV1::ReplaceUniqueWeakest { slot, .. }
                if slot == held.slot
        );
        writeln!(
            output,
            "  held slot={} potion={} band={:?} traits={:?} support={:?} discardable={} selected_for_replacement={}",
            held.slot,
            format_identity(held.retention.potion),
            held.retention.band,
            held.retention.traits,
            held.retention.deck_support,
            held.discardable,
            selected,
        )
        .expect("string write");
    }
    output
}

pub fn render_combat_research_attempt_v1(
    research: &RunPlayCombatResearchAttemptV1,
    detail: bool,
) -> Result<String, String> {
    let attempt = &research.attempt;
    let frontier = task_research_plan_non_dominated_candidate_ordinals_v1(&attempt.candidates);
    let frontier_set = frontier.iter().copied().collect::<BTreeSet<_>>();
    let mut output = String::new();
    writeln!(
        output,
        "combat_research summary_exit={:?} lanes={} candidates={} frontier={frontier:?} milestone_candidates={}",
        attempt.summary_exit(),
        attempt.lanes.len(),
        attempt.candidates.len(),
        research.milestone_candidates.len(),
    )
    .expect("string write");
    // Observed-fact hint only; the ranking contract (final HP, then potions,
    // then steps, over verified frontier candidates only) lives in the CLI
    // help and README instead of repeating on every report.
    if let Some(best) = frontier.iter().copied().max_by(|&a, &b| {
        let left = &attempt.candidates[a];
        let right = &attempt.candidates[b];
        left.outcome
            .final_hp
            .cmp(&right.outcome.final_hp)
            .then_with(|| {
                right
                    .outcome
                    .total_potions_consumed
                    .cmp(&left.outcome.total_potions_consumed)
            })
            .then_with(|| left.plan.steps.len().cmp(&right.plan.steps.len()))
            .then_with(|| b.cmp(&a))
    }) {
        writeln!(output, "observed_best={best}").expect("string write");
    }
    if !detail {
        let total_work = attempt
            .lanes
            .iter()
            .map(|lane| lane.work_consumed)
            .sum::<u64>();
        writeln!(
            output,
            "work lanes={} total={} bounded_progress={} absence_after_budget={}",
            attempt.lanes.len(),
            total_work,
            research.bounded_progress.len(),
            if attempt.candidates.is_empty() {
                "unknown"
            } else {
                "not_applicable"
            },
        )
        .expect("string write");
        for index in &frontier {
            let candidate = &attempt.candidates[*index];
            let candidate_id = combat_research_candidate_id_v1(candidate)?;
            writeln!(
                output,
                "candidate={} id={} frontier=true quality={} final={}/{} gold={} potions_used={} steps={} pending_rewards={}",
                index,
                candidate_id,
                candidate.satisfies_requested_quality,
                candidate.outcome.final_hp,
                candidate.outcome.persistent.max_hp,
                candidate.outcome.final_gold,
                candidate.outcome.total_potions_consumed,
                candidate.plan.steps.len(),
                candidate.outcome.pending_rewards.len(),
            )
            .expect("string write");
        }
        writeln!(
            output,
            "detail=hidden use --detail for internal lanes, dominated candidates, and action lines"
        )
        .expect("string write");
        return Ok(output);
    }
    for lane in &attempt.lanes {
        writeln!(
            output,
            "lane producer={} revision={} unit={:?} work={} exit={:?}",
            lane.provenance.producer_name,
            lane.provenance.producer_revision,
            lane.work_unit,
            lane.work_consumed,
            lane.exit,
        )
        .expect("string write");
    }
    for progress in &research.bounded_progress {
        writeln!(
            output,
            "bounded_progress producer={} max_player_turn={} survival_envelope={} paths={} bounded_search_only=true absence_means_unknown=true",
            progress.provenance.producer_name,
            progress.max_player_turn,
            progress.recent_turn_survival_envelope.len(),
            progress.paths.len(),
        )
        .expect("string write");
        for path in &progress.paths {
            writeln!(
                output,
                "  progress_path roles={:?} turn={} hp={} block={} enemies={} enemy_hp={} steps={}",
                path.candidate.roles,
                path.candidate.endpoint.player_turn,
                path.candidate.endpoint.player_hp,
                path.candidate.endpoint.player_block,
                path.candidate.endpoint.alive_enemy_count,
                path.candidate.endpoint.enemy_total_hp,
                path.candidate.plan.steps.len(),
            )
            .expect("string write");
            writeln!(output, "    action_line={}", path.action_line.join("/"))
                .expect("string write");
        }
    }
    for (index, candidate) in attempt.candidates.iter().enumerate() {
        let candidate_id = combat_research_candidate_id_v1(candidate)?;
        writeln!(
            output,
            "candidate={} id={} frontier={} supporting_producers={} steps={} quality={} final_hp={} final_gold={} consumed_entry_potions={:?} total_potions={} external_burden={}",
            index,
            candidate_id,
            frontier_set.contains(&index),
            candidate
                .supporting_provenances
                .iter()
                .map(|provenance| provenance.producer_name.as_str())
                .collect::<Vec<_>>()
                .join(","),
            candidate.plan.steps.len(),
            candidate.satisfies_requested_quality,
            candidate.outcome.final_hp,
            candidate.outcome.final_gold,
            candidate.outcome.consumed_entry_potion_slots,
            candidate.outcome.total_potions_consumed,
            candidate.outcome.persistent.external_burden_count,
        )
        .expect("string write");
        writeln!(
            output,
            "  comparison_state persistent={:?} final_relics={:?} generated_potions={:?} meta_changes={:?} pending_rewards={:?} mugged={}",
            candidate.outcome.persistent,
            candidate.outcome.final_relics,
            candidate.outcome.generated_potions,
            candidate.outcome.meta_changes,
            candidate.outcome.pending_rewards,
            candidate.outcome.combat_mugged,
        )
        .expect("string write");
        writeln!(
            output,
            "  ordinal_line={}",
            candidate
                .plan
                .steps
                .iter()
                .map(|step| step
                    .choice
                    .ordinal_path
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join("/"))
                .collect::<Vec<_>>()
                .join(" -> "),
        )
        .expect("string write");
        let witness = &research.witness_lines[index];
        writeln!(
            output,
            "  action_line={}",
            witness
                .steps
                .iter()
                .map(|step| {
                    semantic_action_for_choice_v1(&step.source_decision, &step.choice)
                        .unwrap_or_else(|error| format!("<semantic-error:{error}>"))
                })
                .collect::<Vec<_>>()
                .join("/")
        )
        .expect("string write");
    }
    for (index, milestone) in research.milestone_candidates.iter().enumerate() {
        let candidate = &milestone.candidate;
        writeln!(
            output,
            "milestone_candidate={} id={} producer={} steps={} milestones={:?} final_hp={} successor_stage={:?} next_milestone={:?} combat_continues=true",
            index,
            milestone.candidate_id,
            candidate.provenance.producer_name,
            candidate.plan.steps.len(),
            candidate.transition.completed_milestones(),
            candidate.final_player_hp,
            candidate.successor_plan.stage,
            candidate.successor_plan.next_milestone,
        )
        .expect("string write");
        writeln!(output, "  action_line={}", milestone.action_line.join("/"),)
            .expect("string write");
    }
    Ok(output)
}

fn render_route_advice_detail_v1(audit: &ExactRoutePolicyAuditV1) -> String {
    let mut output = String::new();
    let context = &audit.context;
    writeln!(
        output,
        "route_context act={} ascension={} hp={}/{} gold={} critical_recovery={} recovery_pressure={} recent_combat_hp_loss={:?} shop_conversion={:?}",
        context.act,
        context.ascension,
        context.current_hp,
        context.max_hp,
        context.gold,
        context.critical_recovery,
        context.recovery_pressure,
        context.recent_combat_hp_loss,
        context.shop_conversion_support,
    )
    .expect("string write");
    writeln!(
        output,
        "boss_readiness {:?}",
        context.boss_encounter_readiness
    )
    .expect("string write");
    for candidate in &audit.candidates {
        match &candidate.action {
            RoutePolicyActionV1::Select {
                x,
                y,
                arrival,
                path,
                uses_wing_boots,
                ..
            } => {
                writeln!(
                    output,
                    "route_rank={} action=route:{x}.{y} arrival={arrival:?} band={:?} prior={:.6} wing_boots={} coverage={:?} paths={} early_pressure={}..{} first_shop_floor={:?} first_campfire_floor={:?} damage_before_recovery={}..{} selectable_recovery_after_damage={:?} recovery_before_damage={}/{} elites={}..{} campfires={}..{} shops={}..{} treasures={}..{} unknowns={}..{}",
                    candidate.owner_rank,
                    candidate.band,
                    candidate.prior_probability,
                    uses_wing_boots,
                    path.coverage,
                    path.observed_path_count,
                    path.min_early_pressure,
                    path.max_early_pressure,
                    path.first_shop_floor,
                    path.first_campfire_floor,
                    path.min_damage_rooms_before_recovery,
                    path.max_damage_rooms_before_recovery,
                    path.min_damage_rooms_to_recovery,
                    path.paths_with_recovery_before_damage,
                    path.observed_path_count,
                    path.min_elites,
                    path.max_elites,
                    path.min_campfires,
                    path.max_campfires,
                    path.min_shops,
                    path.max_shops,
                    path.min_treasures,
                    path.max_treasures,
                    path.min_unknowns,
                    path.max_unknowns,
                )
                .expect("string write");
                writeln!(
                    output,
                    "  first_elite paths={} forced={} optional={} hallways_before={}..{} unknowns_before={}..{} campfires_before={}..{} shops_before={}..{} bail_to_rest={} bail_to_shop={}",
                    path.first_elite.paths_with_first_elite,
                    path.first_elite.forced,
                    path.first_elite.optional,
                    path.first_elite.min_hallway_fights_before,
                    path.first_elite.max_hallway_fights_before,
                    path.first_elite.min_unknowns_before,
                    path.first_elite.max_unknowns_before,
                    path.first_elite.min_campfires_before,
                    path.first_elite.max_campfires_before,
                    path.first_elite.min_shops_before,
                    path.first_elite.max_shops_before,
                    path.first_elite.can_bail_to_rest_before,
                    path.first_elite.can_bail_to_shop_before,
                )
                .expect("string write");
            }
            RoutePolicyActionV1::CancelToPendingRewards => {
                writeln!(
                    output,
                    "route_rank={} action=cancel arrival=pending_rewards band={:?} prior={:.6}",
                    candidate.owner_rank, candidate.band, candidate.prior_probability
                )
                .expect("string write");
            }
        }
    }
    output
}

fn render_card_reward_advice_detail_v1(audit: &ExactCardRewardPolicyAuditV1) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "card_reward_context hp={}/{} gold={}",
        audit.current_hp, audit.max_hp, audit.gold
    )
    .expect("string write");
    for candidate in &audit.candidates {
        let closed_gaps = candidate
            .closed_threat_gaps
            .iter()
            .map(|gap| format!("{:?}@{:?}:{}", gap.tag, gap.source, gap.subject))
            .collect::<Vec<_>>()
            .join(",");
        let capability_changes = candidate
            .capability_improvements
            .iter()
            .map(|change| {
                format!(
                    "{:?}:{:?}->{:?}",
                    change.capability, change.before, change.after
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let rule_transitions = candidate
            .capability_rule_changes
            .iter()
            .filter(|change| change.before != change.after)
            .map(|change| {
                format!(
                    "{:?}:{:?}->{:?}",
                    change.capability, change.before, change.after
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let persistent_status = candidate
            .persistent_draw_pile_status
            .as_ref()
            .map(|status| {
                format!(
                    "{:?}/injections={}",
                    status.handling,
                    status.injections.len()
                )
            });
        let acquisition = match &candidate.acquisition {
            CardRewardPolicyAcquisitionV1::Card {
                card,
                upgrades,
                copies_before,
                semantics,
                component_signals,
                construction,
                ..
            } => format!(
                "card={} upgrades={upgrades} copies_before={copies_before} roles={:?} combat_upgrade_scope={:?} dependencies={:?} components=+{:?}/debt:{:?}/note:{:?} acquisition_policy={:?} construction=roles:{:?}/hard_gap:{}/any_gap:{}/boss_scaling_repair:{}/package_reliability_repair:{}/deployability_debt:{}/hp_access_debt:{}/short_debuff_redundant:{}/multi_axis:{}/run_rewards:{:?}",
                format_identity(*card),
                semantics.roles,
                semantics.combat_upgrade_scope,
                semantics.dependencies,
                component_signals.positive_signals,
                component_signals.debt_signals,
                component_signals.note_signals,
                candidate.card_acquisition_policy,
                construction.construction_roles,
                construction.strategic_delta.improves_hard_gap,
                construction.strategic_delta.improves_any_gap,
                construction.strategic_delta.repairs_boss_scaling_plan,
                construction.strategic_delta.repairs_package_reliability,
                construction.strategic_delta.adds_deployability_debt,
                construction.hp_cost_access_debt,
                construction.fully_redundant_expensive_short_debuff_package,
                construction.multi_axis_tactical_delivery,
                construction.run_rewards,
            ),
            CardRewardPolicyAcquisitionV1::SingingBowl => "singing_bowl".to_string(),
            CardRewardPolicyAcquisitionV1::Skip => "skip".to_string(),
            CardRewardPolicyAcquisitionV1::OpenReward => "open_reward".to_string(),
        };
        writeln!(
            output,
            "card_rank={} action={} band={:?} prior={:.6} {}",
            candidate.owner_rank,
            candidate.label,
            candidate.band,
            candidate.prior_probability,
            acquisition,
        )
        .expect("string write");
        writeln!(
            output,
            "  gains gaps=[{}] capabilities=[{}] rule_transitions=[{}] resolved_needs={} formation_strengths={:?} improves_threat={} amplifies_answers={} boss_damage={:?} boss_survival_repair={:?} combat_upgrade={:?}->{:?} existing_targets={} route_upgrade_support={:?}",
            closed_gaps,
            capability_changes,
            rule_transitions,
            candidate.resolved_formation_needs.len(),
            candidate.added_formation_strengths,
            candidate.improves_threat_relevant_capability,
            candidate.amplifies_existing_answers,
            candidate.boss_damage_plan_improvement,
            candidate.boss_survival_plan_repair,
            candidate.combat_upgrade_scope_before,
            candidate.combat_upgrade_scope_after,
            candidate.combat_upgrade_target_count_before,
            candidate.upgrade_investment_support,
        )
        .expect("string write");
        writeln!(
            output,
            "  liabilities status_burden={} duplicate_low_marginal={} access_conflict={} power_tax={} deck_shape={:?} persistent_status={:?}",
            candidate.introduces_undigested_status_burden,
            candidate.duplicate_low_marginal,
            candidate.access_conflict_or_redundancy,
            candidate.boss_power_tax_conflict,
            candidate.added_deck_shape_risks,
            persistent_status,
        )
        .expect("string write");
    }
    output
}

fn render_campfire_advice_detail_v1(audit: &ExactCampfirePolicyAuditV1) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "campfire_context hp={}/{} rest_heal={} boss_nodes_remaining={:?} combat_before_next={:?} elite_before_next={:?}",
        audit.current_hp,
        audit.max_hp,
        audit.recovery.rest_heal_capacity,
        audit.recovery.boss_nodes_remaining,
        audit.recovery.known_combat_before_next_campfire,
        audit.recovery.known_elite_before_next_campfire,
    )
    .expect("string write");
    for candidate in &audit.candidates {
        writeln!(
            output,
            "campfire_rank={} action={} band={:?} prior={:.6} detail={} mechanical_rank={:?} mechanical={} delta=hp:{} max_hp:{} gold:{} deck:{} closed_gaps={} capability_changes={}",
            candidate.owner_rank,
            candidate.label,
            candidate.band,
            candidate.prior_probability,
            candidate.action,
            candidate.mechanical_upgrade_rank,
            candidate.mechanical_upgrade_label.as_deref().unwrap_or("-"),
            candidate.hp_gain,
            candidate.max_hp_gain,
            candidate.gold_delta,
            candidate.deck_size_delta,
            candidate.closed_threat_gaps.len(),
            candidate.capability_improvements.len(),
        )
        .expect("string write");
    }
    output
}

fn render_boss_relic_advice_detail_v1(audit: &ExactBossRelicPolicyAuditV1) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "boss_relic_context energy={} energy_pressure={:?} deck={} access={:?} playability={:?} burden={:?} costs=zero:{} low:{} high:{} x:{} self_damage={} upgrades=all:{} important:{} cost_reduction:{} unpaid_core:{} slot_pressure:{} curse_support={} potions={}/{}",
        audit.context.energy_master,
        audit.context.energy_pressure,
        audit.context.deck_size,
        audit.context.deck_access,
        audit.context.energy_or_playability,
        audit.context.deck_burden,
        audit.context.zero_cost_cards,
        audit.context.low_cost_cards,
        audit.context.high_cost_cards,
        audit.context.x_cost_cards,
        audit.context.self_damage_sources,
        audit.context.upgrade_candidates,
        audit.context.important_upgrade_candidates,
        audit.context.cost_reduction_upgrade_candidates,
        audit.context.unpaid_core_upgrade_debts,
        audit.context.upgrade_slot_pressure,
        audit.context.curse_mitigation_or_synergy,
        audit.context.potion_count,
        audit.context.potion_slots,
    )
    .expect("string write");
    for candidate in &audit.candidates {
        writeln!(
            output,
            "boss_relic_rank={} label={} band={:?} prior={:.6} relic={} energy={} replaces={} changed_cards={} followup={} generated_relics=[{}] strategic_relics={} unresolved_debt={} compounding_debt={}",
            candidate.owner_rank,
            candidate.label,
            candidate.band,
            candidate.prior_probability,
            candidate
                .relic
                .map(format_identity)
                .unwrap_or_else(|| "none".to_string()),
            candidate.energy_gain,
            candidate.replaces_existing_relic,
            candidate.changed_deck_cards,
            candidate.followup.as_deref().unwrap_or("none"),
            candidate
                .generated_relics
                .iter()
                .map(|relic| format_identity(*relic))
                .collect::<Vec<_>>()
                .join(", "),
            candidate.strategic_relic_count,
            candidate.unresolved_debt_terms,
            candidate.compounding_debt_count,
        )
        .expect("string write");
        writeln!(
            output,
            "  traits=[{}] debts=[{}] startup={} action_supply={}",
            candidate.traits.join(", "),
            candidate.added_debts.join(", "),
            render_boss_relic_startup_delta_v1(candidate.startup),
            render_boss_relic_action_supply_delta_v1(candidate.action_supply),
        )
        .expect("string write");
    }
    output
}

fn render_boss_relic_startup_delta_v1(delta: BossRelicStartupDeltaV1) -> String {
    format!(
        "setup_debt:{:+} setup_payment:{:+} survival:{:+} payoff:{:+} shape_risk:{:+} strong_draw:{:+} snecko_draw:{:+} snecko_cost_debt:{:+} snecko_high_cost:{:+} snecko_pellets_cleanup:{:+}",
        delta.setup_debt,
        delta.setup_payment,
        delta.immediate_survival,
        delta.payoff_engine,
        delta.combat_shape_risk,
        delta.strong_draw,
        delta.snecko_draw_bonus,
        delta.snecko_random_cost_debt,
        delta.snecko_high_cost_payoff,
        delta.snecko_pellets_cleanup_access,
    )
}

fn render_boss_relic_action_supply_delta_v1(delta: BossRelicActionSupplyDeltaV1) -> String {
    format!(
        "opening:{:+} delayed:{:+} burst:{:+} repeatable:{:+} additional_play:{:+} compression:{:+}",
        delta.opening_once_options,
        delta.delayed_per_turn_sources,
        delta.same_turn_burst_sources,
        delta.triggered_repeatable_sources,
        delta.additional_play_sources,
        delta.cost_or_resource_compression_sources,
    )
}

fn render_shop_advice_detail_v1(audit: &ExactShopPolicyAuditV1) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "shop_context hp={}/{} gold={}",
        audit.current_hp, audit.max_hp, audit.gold,
    )
    .expect("string write");
    for candidate in &audit.candidates {
        writeln!(
            output,
            "shop_rank={} label={} band={:?} prior={:.6} spent={} followup={:?} acquisition={}",
            candidate.owner_rank,
            candidate.label,
            candidate.band,
            candidate.prior_probability,
            candidate.gold_spent,
            candidate.followup,
            candidate.acquisition,
        )
        .expect("string write");
        writeln!(
            output,
            "  delta={} gaps=[{}] capabilities=[{}] reinforced=[{}] needs=[{}] strengths=[{}] consumables=[{}] consumable_packages=[{}] relic_threats=[{}]",
            render_shop_state_delta_v1(
                candidate.hp_gain,
                candidate.max_hp_gain,
                candidate.missing_hp_recovered,
                candidate.guaranteed_realizable_hp_gain,
                candidate.guaranteed_realizable_max_hp_gain,
                candidate.conditional_combat_on_use_hp_capacity,
                candidate.deck_size_delta,
                candidate.action_supply,
            ),
            candidate.closed_threat_gaps.join(", "),
            candidate.capability_improvements.join(", "),
            candidate.reinforced_threat_capabilities.join(", "),
            candidate.resolved_formation_needs.join(", "),
            candidate.added_formation_strengths.join(", "),
            candidate.matched_consumable_capabilities.join(", "),
            candidate.matched_consumable_packages.join(", "),
            candidate.matched_relic_threats.join(", "),
        )
        .expect("string write");
        writeln!(
            output,
            "  requirements={:?} support={:?}",
            candidate.acquisition_requirements, candidate.acquisition_requirement_support,
        )
        .expect("string write");
        writeln!(
            output,
            "  card=verdict:{:?} reason:{:?} opportunity:{:?} upgrade:{:?}->{:?} liabilities=status:{} package_debt:{} redundant_upgrade:{} deck_shape:[{}] purge=class:{:?} benefit:{:?} signals:[{}] offsets:[{}] loss:{:?} durable:{} consumable_conversion:{} spent_before:{}",
            candidate.card_acquisition_verdict,
            candidate.card_acquisition_reason,
            candidate.card_acquisition_opportunity_cost,
            candidate.upgrade_scope_before,
            candidate.upgrade_scope_after,
            candidate.introduces_status_burden,
            candidate.introduces_package_debt,
            candidate.redundant_upgrade_access,
            candidate.added_deck_shape_risks.join(", "),
            candidate.purge_target_class,
            candidate.purge_removal_benefit,
            candidate.purge_removal_signals.join(", "),
            candidate.purge_compensating_signals.join(", "),
            candidate.purge_target_loss,
            candidate.durable_asset_support,
            candidate.consumable_conversion_support,
            candidate.spent_gold_before_action,
        )
        .expect("string write");
    }
    output
}

fn render_shop_state_delta_v1(
    hp_gain: i32,
    max_hp_gain: i32,
    missing_hp_recovered: i32,
    guaranteed_realizable_hp_gain: i32,
    guaranteed_realizable_max_hp_gain: i32,
    conditional_combat_on_use_hp_capacity: i32,
    deck_size_delta: isize,
    action_supply: BossRelicActionSupplyDeltaV1,
) -> String {
    format!(
        "hp:{hp_gain} max_hp:{max_hp_gain} recovered_missing_hp:{missing_hp_recovered} guaranteed_realizable_hp:{guaranteed_realizable_hp_gain} guaranteed_realizable_max_hp:{guaranteed_realizable_max_hp_gain} conditional_combat_on_use_hp_capacity:{conditional_combat_on_use_hp_capacity} deck:{deck_size_delta} action_supply={}",
        render_boss_relic_action_supply_delta_v1(action_supply),
    )
}

fn render_decision_v1(
    output: &mut String,
    decision: &TaskRunBranchDecisionInventoryV1,
) -> Result<(), String> {
    writeln!(
        output,
        "depth={} replay={} site={:?} proof={} snapshot={}",
        decision.target_archive_depth,
        decision.replayed_public_step_count,
        decision.site,
        render_proof(&decision.proof),
        short_id(&decision.public_snapshot_id)
    )
    .expect("string write");
    match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => {
            writeln!(output, "{}", render_strategic_run_header_v1(observation))
                .expect("string write");
            let mut cards = BTreeMap::<String, usize>::new();
            for card in &observation.cards {
                let label = format_card(card.card, card.upgrades);
                *cards.entry(label).or_default() += 1;
            }
            writeln!(
                output,
                "deck [{}]",
                cards
                    .into_iter()
                    .map(|(card, count)| if count == 1 {
                        card
                    } else {
                        format!("{card}x{count}")
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("string write");
            writeln!(
                output,
                "relics [{}]",
                observation
                    .relics
                    .iter()
                    .map(|relic| format_identity(relic.relic))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("string write");
            writeln!(
                output,
                "potions [{}]",
                observation
                    .potions
                    .iter()
                    .map(|slot| slot.potion.as_ref().map_or_else(
                        || "empty".to_string(),
                        |potion| format_identity(potion.potion)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("string write");
            render_shop_inventory_v1(output, &observation.context);
            if let Some(route) = render_single_route_v1(&observation.public_map) {
                writeln!(output, "{route}").expect("string write");
            }
            for route in render_route_options_v1(
                &observation.public_map,
                decision.candidates.iter().map(|row| &row.candidate),
            ) {
                writeln!(output, "{route}").expect("string write");
            }
            // The original game lets the player inspect the generated map while
            // choosing a Neow blessing.  Hiding it here makes shop-dependent and
            // path-dependent blessings impossible to evaluate from public data.
            if matches!(
                decision.site,
                TaskDecisionSiteV1::Neow | TaskDecisionSiteV1::Map
            ) {
                writeln!(
                    output,
                    "map current=({},{}) boss={}",
                    observation.public_map.current_x,
                    observation.public_map.current_y,
                    observation
                        .public_map
                        .boss
                        .map(format_identity)
                        .unwrap_or_else(|| "unknown".to_string())
                )
                .expect("string write");
                let mut rows = BTreeMap::<i32, Vec<String>>::new();
                for node in &observation.public_map.nodes {
                    let room = format!("{:?}", node.room);
                    let key = if node.has_emerald_key { "+key" } else { "" };
                    let edges = node
                        .edges
                        .iter()
                        .map(|edge| format!("({}, {})", edge.destination_x, edge.destination_y))
                        .collect::<Vec<_>>()
                        .join(",");
                    rows.entry(node.y)
                        .or_default()
                        .push(format!("({},{})={room}{key}->[{edges}]", node.x, node.y));
                }
                for (y, mut nodes) in rows {
                    nodes.sort();
                    writeln!(output, "  y{y:02} {}", nodes.join(" ")).expect("string write");
                }
            }
        }
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } => {
            let context = match run_context {
                sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Available {
                    act,
                    floor,
                    encounter_id,
                    ..
                } => format!(
                    "act={act} floor={floor} encounter={}",
                    encounter_id
                        .as_ref()
                        .map(|encounter_id| format_identity(*encounter_id))
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                sts_oracle_learning::agent::information::run::PublicCombatRunContextV1::Unavailable {
                    reason,
                } => format!("detached={reason:?}"),
            };
            writeln!(
                output,
                "combat {} hp={}/{} block={} turn={} energy={}/{} phase={:?}",
                context,
                observation.player.hp,
                observation.player.max_hp,
                render_player_block_v1(observation),
                observation.turn.turn_count,
                observation.turn.energy,
                observation.player.energy_master,
                observation.turn.phase
            )
            .expect("string write");
            writeln!(
                output,
                "player stance={} cards_played={} attacks_played={} powers=[{}]",
                format_identity(observation.player.stance),
                observation.turn.counters.cards_played_this_turn,
                observation.turn.counters.attacks_played_this_turn,
                render_powers(&observation.player.powers, Locale::En)
            )
            .expect("string write");
            writeln!(
                output,
                "relics [{}]",
                observation
                    .player
                    .relics
                    .iter()
                    .map(|relic| {
                        let mut state = format_identity(relic.id);
                        if relic.counter != -1 {
                            write!(state, "(counter={})", relic.counter).expect("string write");
                        }
                        if relic.amount != 0 {
                            write!(state, "(amount={})", relic.amount).expect("string write");
                        }
                        if let Some(status) = sts_oracle_learning::content::relics::mechanics::trigger_state_description(relic.id, relic.used_up) {
                            write!(state, "({status})").expect("string write");
                        }
                        state
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("string write");
            writeln!(
                output,
                "potions [{}]",
                observation
                    .potions
                    .iter()
                    .enumerate()
                    .map(|(slot, potion)| potion.as_ref().map_or_else(
                        || format!("{slot}:empty"),
                        |potion| format!(
                            "{slot}:{:?} usable={} target={}",
                            potion.potion_id, potion.can_use, potion.requires_target
                        )
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("string write");
            writeln!(
                output,
                "hand [{}]",
                observation
                    .cards
                    .hand
                    .cards
                    .iter()
                    .enumerate()
                    .map(|(index, card)| format!(
                        "{index}:{} cost={} untargeted_card_damage={} block={}",
                        format_card(card.card_id, card.upgrades),
                        card.effective_cost,
                        card.current_damage,
                        card.current_block
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .expect("string write");
            writeln!(
                output,
                "{}",
                render_card_collection("draw", &observation.cards.draw)
            )
            .expect("string write");
            writeln!(
                output,
                "{}",
                render_card_collection("discard", &observation.cards.discard)
            )
            .expect("string write");
            writeln!(
                output,
                "{}",
                render_card_collection("exhaust", &observation.cards.exhaust)
            )
            .expect("string write");
            writeln!(
                output,
                "{}",
                render_card_collection("limbo", &observation.cards.limbo)
            )
            .expect("string write");
            writeln!(
                output,
                "{}",
                render_card_collection("master", &observation.cards.master_deck)
            )
            .expect("string write");
            writeln!(
                output,
                "monsters [{}]",
                observation
                    .monsters
                    .iter()
                    .map(|monster| format!(
                        "{}:{} hp={}/{} block={} public_intent={:?}:{:?} public_damage_per_hit={:?} public_intent_hidden_reason={:?} executed_move_history={:?}:{:?} powers=[{}] counters={:?}",
                        monster.slot,
                        match monster.enemy {
                            CombatLearningEnemyIdentityV1::Known { enemy_id } => {
                                format_identity(enemy_id)
                            }
                        },
                        monster.hp,
                        monster.max_hp,
                        render_monster_block_v1(monster.block, &monster.powers),
                        monster.intent.evidence,
                        monster.intent.intent,
                        monster.intent.preview_damage_per_hit,
                        monster.intent.hidden_reason,
                        monster.executed_moves.evidence,
                        monster.executed_moves.move_ids,
                        render_powers(&monster.powers, Locale::En),
                        monster.public_counters
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
            .expect("string write");
        }
    }
    let public_decision = decision.public_decision_v1()?;
    let public_combat_decision = matches!(
        &decision.observation,
        TaskPublicObservationV1::Combat { .. }
    )
    .then_some(&public_decision);
    let candidate_heading = if matches!(
        &decision.observation,
        TaskPublicObservationV1::Strategic {
            observation: TaskStrategicObservationV1 {
                context: sts_oracle_learning::ai::planner_core::PlannerDecisionContext::Shop { .. },
                ..
            }
        }
    ) {
        "executable_candidates"
    } else {
        "candidates"
    };
    writeln!(output, "{candidate_heading}={}", decision.candidates.len()).expect("string write");
    render_reward_display_hints_v1(output, &decision.observation, &decision.candidates);
    for (alias, copies) in transactional_play_aliases_v1(&public_decision)? {
        writeln!(
            output,
            "  transaction_alias action={alias} equivalent_visible_copies={copies} repeat_against_each_successor=true"
        )
        .expect("string write");
    }
    for candidate in &decision.candidates {
        let evidence = if candidate.observed_choices.is_empty() {
            "new"
        } else {
            "observed"
        };
        let semantic_action = candidate
            .direct_choice
            .as_ref()
            .map(|choice| semantic_action_for_choice_v1(&public_decision, choice))
            .transpose()?
            .unwrap_or_else(|| "<parameterized>".to_string());
        let effect = public_combat_decision
            .as_ref()
            .map(|public_decision| {
                public_decision
                    .combat_candidate_effect_v1(candidate.candidate_ordinal)
                    .map(|effect| render_candidate_effect(&effect))
                    .map_err(|error| {
                        format!(
                            "cannot project candidate {} public effect: {error}",
                            candidate.candidate_ordinal
                        )
                    })
            })
            .transpose()?
            .unwrap_or_default();
        writeln!(
            output,
            "  [{}] evidence={:<8} action={} {}{}",
            candidate.candidate_ordinal,
            evidence,
            semantic_action,
            render_candidate_with_observation(
                &candidate.candidate,
                Some(&decision.observation),
                Locale::En
            ),
            effect
        )
        .expect("string write");
    }
    Ok(())
}

fn transactional_play_aliases_v1(
    decision: &TaskPublicDecisionV1,
) -> Result<Vec<(String, usize)>, String> {
    let mut counts = BTreeMap::<String, usize>::new();
    for ordinal in 0..decision.candidates.len() {
        let choice = TaskPublicChoiceV1 {
            ordinal_path: vec![ordinal],
        };
        if let Some(alias) = transactional_play_alias_for_choice_v1(decision, &choice)? {
            *counts.entry(alias).or_default() += 1;
        }
    }
    Ok(counts
        .into_iter()
        .filter(|(_, copies)| *copies > 1)
        .collect())
}

pub(crate) fn render_strategic_run_header_v1(observation: &TaskStrategicObservationV1) -> String {
    let run = &observation.run;
    let context = match &observation.context {
        sts_oracle_learning::ai::planner_core::PlannerDecisionContext::Shop {
            purge_cost,
            purge_available,
            inventory,
        } => format!(
            "Shop {{ purge_cost: {purge_cost}, purge_available: {purge_available}, remaining_cards: {}, remaining_relics: {}, remaining_potions: {} }}",
            inventory.cards.len(),
            inventory.relics.len(),
            inventory.potions.len()
        ),
        context => format!("{context:?}"),
    };
    format!(
        "run goal={:?} keys=[ruby:{},sapphire:{},emerald:{}] act={} floor={} hp={}/{} gold={} boss={:?} map=({},{}) context={}",
        observation.run_goal,
        run.keys[0],
        run.keys[1],
        run.keys[2],
        run.act,
        run.floor,
        run.current_hp,
        run.max_hp,
        run.gold,
        observation.public_map.boss,
        observation.public_map.current_x,
        observation.public_map.current_y,
        context
    )
}

/// Display-only reward hints appended to the candidate list. They never add,
/// remove, or alter any candidate: they surface why a pending reward item has
/// no executable claim (full potion slots make claiming a no-op, so the view
/// model omits it) and the engine-enforced exclusivity between a chest's
/// sapphire key and its linked relic.
fn render_reward_display_hints_v1(
    output: &mut String,
    observation: &TaskPublicObservationV1,
    candidates: &[sts_oracle_learning::eval::run_control::TaskRunBranchCandidateInventoryV1],
) {
    let TaskPublicObservationV1::Strategic {
        observation:
            TaskStrategicObservationV1 {
                context:
                    sts_oracle_learning::ai::planner_core::PlannerDecisionContext::Reward {
                        pending_rewards,
                    },
                potions,
                ..
            },
    } = observation
    else {
        return;
    };
    let addressed = candidates
        .iter()
        .filter_map(|candidate| match &candidate.candidate {
            TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::ClaimReward {
                        reward_item_index, ..
                    }
                    | TaskStrategicActionV1::OpenCardReward { reward_item_index },
            } => Some(reward_item_index),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let blocked: Vec<String> = pending_rewards
        .iter()
        .enumerate()
        .filter(|(index, reward)| {
            !addressed.contains(index) && matches!(reward, PlannerRewardDescriptor::Potion { .. })
        })
        .map(|(index, reward)| {
            format!(
                "[{index}] {} blocked:no_empty_potion_slot",
                render_reward_descriptor(reward, Locale::En)
            )
        })
        .collect();
    if !blocked.is_empty() && potions.iter().all(|slot| slot.potion.is_some()) {
        writeln!(
            output,
            "unclaimable_rewards={} (claiming would be a no-op; discard a held potion first)",
            blocked.join(" | ")
        )
        .expect("string write");
    }
    let has_sapphire = pending_rewards
        .iter()
        .any(|reward| matches!(reward, PlannerRewardDescriptor::SapphireKey));
    let has_relic = pending_rewards
        .iter()
        .any(|reward| matches!(reward, PlannerRewardDescriptor::Relic { .. }));
    if has_sapphire && has_relic {
        writeln!(
            output,
            "reward_exclusivity=chest sapphire key and its linked relic are mutually exclusive; claiming either cancels the other"
        )
        .expect("string write");
    }
}

pub(crate) fn render_shop_inventory_v1(
    output: &mut String,
    context: &sts_oracle_learning::ai::planner_core::PlannerDecisionContext,
) {
    let sts_oracle_learning::ai::planner_core::PlannerDecisionContext::Shop { inventory, .. } =
        context
    else {
        return;
    };
    if inventory.is_empty() {
        writeln!(output, "shop_inventory unavailable_in_legacy_observation").expect("string write");
        return;
    }
    writeln!(
        output,
        "shop_inventory public_remaining=true action_slots=current"
    )
    .expect("string write");
    render_shop_inventory_group_v1(
        output,
        "cards",
        inventory.cards.iter().map(|offer| {
            format!(
                "{}:{} {}g affordable={} {}",
                offer.slot,
                format_card(offer.card, offer.upgrades),
                offer.price,
                offer.affordable,
                render_shop_purchase_status_v1(&offer.purchase_status)
            )
        }),
    );
    render_shop_inventory_group_v1(
        output,
        "relics",
        inventory.relics.iter().map(|offer| {
            format!(
                "{}:{} {}g affordable={} {}",
                offer.slot,
                format_identity(offer.relic),
                offer.price,
                offer.affordable,
                render_shop_purchase_status_v1(&offer.purchase_status)
            )
        }),
    );
    render_shop_inventory_group_v1(
        output,
        "potions",
        inventory.potions.iter().map(|offer| {
            format!(
                "{}:{} {}g affordable={} {}",
                offer.slot,
                format_identity(offer.potion),
                offer.price,
                offer.affordable,
                render_shop_purchase_status_v1(&offer.purchase_status)
            )
        }),
    );
    if let Some(purge) = &inventory.purge {
        writeln!(
            output,
            "  purge {}g affordable={} {}",
            purge.price,
            purge.affordable,
            render_shop_purchase_status_v1(&purge.purchase_status)
        )
        .expect("string write");
    }
}

fn render_shop_inventory_group_v1(
    output: &mut String,
    label: &str,
    offers: impl Iterator<Item = String>,
) {
    writeln!(
        output,
        "  {label} [{}]",
        offers.collect::<Vec<_>>().join("; ")
    )
    .expect("string write");
}

fn render_shop_purchase_status_v1(status: &PlannerShopPurchaseStatusV1) -> String {
    match status {
        PlannerShopPurchaseStatusV1::Purchasable => "status=purchasable".to_string(),
        PlannerShopPurchaseStatusV1::Blocked { reason } => {
            format!(
                "status=blocked:{}",
                render_shop_purchase_block_reason_v1(reason)
            )
        }
    }
}

fn render_shop_purchase_block_reason_v1(reason: &PlannerShopPurchaseBlockReasonV1) -> String {
    match reason {
        PlannerShopPurchaseBlockReasonV1::MerchantUnavailable { detail } => detail
            .as_ref()
            .map(|detail| format!("merchant_unavailable({detail})"))
            .unwrap_or_else(|| "merchant_unavailable".to_string()),
        PlannerShopPurchaseBlockReasonV1::NotEnoughGold { gold, price } => {
            format!("not_enough_gold(gold={gold},price={price})")
        }
        PlannerShopPurchaseBlockReasonV1::BlockedBySozu => "blocked_by_sozu".to_string(),
        PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot => "no_empty_potion_slot".to_string(),
        PlannerShopPurchaseBlockReasonV1::PurgeAlreadyUsed => "purge_already_used".to_string(),
        PlannerShopPurchaseBlockReasonV1::NoEligiblePurgeTarget => {
            "no_eligible_purge_target".to_string()
        }
    }
}

pub(crate) fn render_single_route_v1(map: &PlannerPublicMap) -> Option<String> {
    match crate::public_map_reading::PublicRouteReading::new(
        map,
        (map.current_x, map.current_y),
        sts_presentation::Locale::En,
    ) {
        Ok(route) => route.single_path(),
        Err(error) => Some(format!("route topology Unknown: {error}")),
    }
}

/// Summarize exactly the owner's current route candidates, including entrance
/// and flight choices. No outgoing map edge is promoted into a legal action.
pub(crate) fn render_route_options_v1<'a>(
    map: &PlannerPublicMap,
    candidates: impl IntoIterator<Item = &'a TaskPublicCandidateV1>,
) -> Vec<String> {
    candidates
        .into_iter()
        .filter_map(|candidate| {
            let TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ChooseRouteNode { x, y, flight, .. },
            } = candidate
            else {
                return None;
            };
            let summary = match crate::public_map_reading::PublicRouteReading::new(
                map,
                (*x, *y),
                sts_presentation::Locale::En,
            ) {
                Ok(route) => route.summary(),
                Err(error) => format!("topology Unknown: {error}"),
            };
            Some(format!(
                "route_option ({x},{y}){} {summary}",
                if *flight { " flight" } else { "" }
            ))
        })
        .collect()
}

fn render_card_collection(label: &str, collection: &CombatLearningCardCollectionV1) -> String {
    let cards = if matches!(
        collection.evidence,
        ObservationEvidenceKindV1::VisibleExact
            | ObservationEvidenceKindV1::PublicOrderedCollection
    ) {
        collection
            .cards
            .iter()
            .enumerate()
            .map(|(index, card)| format!("{index}:{}", format_pile_card(card)))
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        let mut counts = BTreeMap::<String, usize>::new();
        for card in &collection.cards {
            *counts.entry(format_pile_card(card)).or_default() += 1;
        }
        counts
            .into_iter()
            .map(|(card, count)| {
                if count == 1 {
                    card
                } else {
                    format!("{card}x{count}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "{label} count={} evidence={:?} [{}]",
        collection.cards.len(),
        collection.evidence,
        cards
    )
}

fn render_player_block_rule_v1(observation: &PublicCombatStateV1) -> &'static str {
    if observation
        .player
        .powers
        .iter()
        .any(|power| power.power == PowerId::Barricade)
    {
        "retain_remaining:barricade"
    } else if observation
        .player
        .powers
        .iter()
        .any(|power| power.power == PowerId::Blur && power.amount > 0)
    {
        "retain_remaining_next_turn:blur"
    } else if observation
        .player
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Calipers)
    {
        "retain_max(remaining-15,0):calipers"
    } else {
        "clear_remaining_before_next_player_turn"
    }
}

fn render_monster_block_rule_v1(powers: &[CombatLearningPowerV1]) -> &'static str {
    if powers.iter().any(|power| power.power == PowerId::Barricade) {
        "retain_remaining:barricade"
    } else {
        "clear_remaining_at_monster_turn_start"
    }
}

pub(crate) fn render_player_block_v1(observation: &PublicCombatStateV1) -> String {
    if observation.player.block == 0 {
        "0".to_string()
    } else {
        format!(
            "{}({})",
            observation.player.block,
            render_player_block_rule_v1(observation)
        )
    }
}

pub(crate) fn render_monster_block_v1(block: i32, powers: &[CombatLearningPowerV1]) -> String {
    if block == 0 {
        "0".to_string()
    } else {
        format!("{block}({})", render_monster_block_rule_v1(powers))
    }
}

fn format_pile_card(card: &CombatLearningCardV1) -> String {
    let mut label = format_card(card.card_id, card.upgrades);
    if let Some(cost) = card.cost_for_turn {
        write!(label, "({cost})").expect("string write");
    }
    label
}

pub(crate) fn render_powers(
    powers: &[CombatLearningPowerV1],
    locale: sts_presentation::Locale,
) -> String {
    powers
        .iter()
        .map(|power| {
            let name = locale
                .entry("game", &format!("power.{}", power.power.stable_id()))
                .unwrap_or_else(|| gameplay_display_label_v1(power.power));
            let key = if power.power == sts_oracle_learning::content::powers::PowerId::Invincible {
                "powerState.invincible"
            } else if sts_oracle_learning::content::powers::mechanics::get(power.power)
                .is_some_and(|m| m.round_duration)
            {
                if power.amount == 1 {
                    "powerState.duration_one"
                } else {
                    "powerState.duration_other"
                }
            } else {
                "powerState.amount"
            };
            let mut label = locale
                .text(
                    "game",
                    key,
                    &[
                        ("name", name),
                        ("amount", &power.amount.to_string()),
                        ("limit", &power.extra_data.to_string()),
                    ],
                )
                .expect("power state message");
            if power.extra_data != 0
                && power.power != sts_oracle_learning::content::powers::PowerId::Invincible
            {
                write!(label, "/{}", power.extra_data).expect("string write");
            }
            if let Some(card) = &power.payload_card {
                write!(
                    label,
                    "<{}>",
                    locale.card_name(card.card_id.stable_id(), card.upgrades)
                )
                .expect("string write");
            }
            label
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_candidate_effect(effect: &TaskPublicCombatCandidateEffectV1) -> String {
    match &effect.effect.effect {
        PublicCombatCandidateEffectV1::PlayCard { card } => {
            let target_damage = card
                .displayed_damage
                .iter()
                .map(|damage| {
                    format!(
                        "slot{}:{}({:?})",
                        damage.recipient.monster_slot,
                        damage.amount_per_hit,
                        damage.recipient_certainty
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let roles = card
                .auxiliary_roles
                .iter()
                .map(|role| format!("{role:?}"))
                .collect::<Vec<_>>()
                .join(",");
            let x_cost = card
                .cost
                .x_cost
                .as_ref()
                .map(|x_cost| {
                    let payment = match x_cost.payment {
                        PublicCombatXCostPaymentV1::SpendAllCurrentEnergy { amount } => {
                            format!("all_current:{amount}")
                        }
                        PublicCombatXCostPaymentV1::FreeToPlayOnce => "free_to_play:0".to_string(),
                    };
                    let repeated_damage = x_cost
                        .repeated_damage
                        .as_ref()
                        .map(|repeated| {
                            repeated
                                .projected_totals
                                .iter()
                                .zip(&card.displayed_damage)
                                .map(|(total, per_hit)| {
                                    format!(
                                        "slot{}:{}x{}={}",
                                        total.recipient.monster_slot,
                                        per_hit.amount_per_hit,
                                        repeated.hit_count,
                                        total.amount_before_hit_by_hit_resolution
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .filter(|damage| !damage.is_empty())
                        .unwrap_or_else(|| "-".to_string());
                    format!(
                        " x_cost=payment:{payment},captured_energy_on_use:{},chemical_x_bonus:{},card_effect_bonus:{},effect_x:{},repeated_damage:[{}],post_play_energy_retained:{}",
                        x_cost.captured_energy_on_use,
                        x_cost.chemical_x_bonus,
                        x_cost.card_effect_bonus,
                        x_cost.effect_x,
                        repeated_damage,
                        x_cost.post_play_energy_gains_are_not_retroactively_spent
                    )
                })
                .unwrap_or_default();
            format!(
                " target_adjusted_damage_per_hit:[{}] displayed_block={} mechanics=coverage:{:?},roles:[{}],exhaust:{:?}{}",
                target_damage,
                card.displayed_block,
                card.auxiliary_role_coverage,
                roles,
                card.exhaust_disposition,
                x_cost
            )
        }
        PublicCombatCandidateEffectV1::UsePotion { potion } => format!(
            " mechanics=potency:{},roles:{:?}",
            potion.effective_potency, potion.mechanic_roles
        ),
        _ => String::new(),
    }
}

pub fn render_history_v1(head: &RunPlayHeadV1, limit: usize, locale: Locale) -> String {
    render_filtered_history_v1(head, limit, false, locale)
}

pub fn render_combat_history_v1(history: &RunPlayCombatHistoryV1, locale: Locale) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        crate::history_state::message(
            locale,
            "combatPath",
            &[
                ("total", &history.total_combat_steps.to_string()),
                ("showing", &history.steps.len().to_string()),
                ("current", short_id(&history.current_archive_node_id)),
            ]
        )
    )
    .expect("string write");
    for step in &history.steps {
        let action = step.path.semantic_action.clone().unwrap_or_else(|| {
            render_candidate_with_observation(
                &step.path.selected_candidate,
                Some(&step.source),
                locale,
            )
        });
        writeln!(
            output,
            "  d{} from=n{} to=n{} action={} -> {}",
            step.path.depth,
            short_id(&step.from_archive_node_id),
            short_id(&step.path.archive_node_id),
            action,
            render_boundary(step.path.observed_boundary)
        )
        .expect("string write");
        if let TaskPublicObservationV1::Combat {
            run_context,
            observation,
        } = &step.source
        {
            let reading =
                crate::decision::combat::reading(run_context, observation, true, locale, true);
            for line in reading.lines() {
                writeln!(output, "    {line}").expect("string write");
            }
        }
    }
    output
}

pub fn render_strategic_history_v1(head: &RunPlayHeadV1, limit: usize, locale: Locale) -> String {
    render_filtered_history_v1(head, limit, true, locale)
}

pub fn render_construction_history_v1(
    history: &RunPlayConstructionHistoryV1,
    locale: Locale,
) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        crate::history_state::message(
            locale,
            "construction",
            &[
                ("count", &history.steps.len().to_string()),
                ("current", short_id(&history.current_archive_node_id)),
                ("retained", &history.owner_ranking_retained.to_string()),
            ]
        ),
    )
    .expect("string write");
    if !history.owner_ranking_retained {
        writeln!(
            output,
            "{}",
            crate::history_state::message(locale, "constructionEvidence", &[])
        )
        .expect("string write");
    }
    for step in &history.steps {
        let site_options = step
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.kind == crate::driver::RunPlayConstructionCandidateKindV1::SiteOption
            })
            .collect::<Vec<_>>();
        let options = if site_options.len() <= 8 {
            format!(
                " options=[{}]",
                site_options
                    .iter()
                    .map(|candidate| format!(
                        "{}{}",
                        if candidate.selected { "*" } else { "" },
                        candidate.action
                    ))
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        } else {
            format!(" options={}", site_options.len())
        };
        writeln!(
            output,
            "d{} A{}F{} {:?} hp={}/{} gold={} root=n{} choose={}{}",
            step.depth,
            step.act,
            step.floor,
            step.site,
            step.current_hp,
            step.max_hp,
            step.gold,
            short_id(&step.from_archive_node_id),
            step.selected_action,
            options,
        )
        .expect("string write");
    }
    output
}

pub fn render_construction_history_detail_v1(
    history: &RunPlayConstructionHistoryV1,
    locale: Locale,
) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        crate::history_state::message(
            locale,
            "construction",
            &[
                ("count", &history.steps.len().to_string()),
                ("current", short_id(&history.current_archive_node_id)),
                ("retained", &history.owner_ranking_retained.to_string()),
            ]
        ),
    )
    .expect("string write");
    if !history.owner_ranking_retained {
        writeln!(
            output,
            "{}",
            crate::history_state::message(locale, "constructionEvidence", &[])
        )
        .expect("string write");
    }
    for step in &history.steps {
        writeln!(
            output,
            "d{} act={} floor={} site={:?} hp={}/{} gold={} edge={}->{}",
            step.depth,
            step.act,
            step.floor,
            step.site,
            step.current_hp,
            step.max_hp,
            step.gold,
            short_id(&step.from_archive_node_id),
            short_id(&step.to_archive_node_id),
        )
        .expect("string write");
        writeln!(
            output,
            "{}",
            crate::history_state::message(
                locale,
                "deck",
                &[(
                    "cards",
                    &crate::history_state::render_deck_v1(&step.deck, locale)
                )]
            )
        )
        .expect("string write");
        // These report fields carry explicit stable IDs, never rendered names.
        let relics = step
            .relics
            .iter()
            .map(|id| locale.entity("relic", id))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            output,
            "{}",
            crate::history_state::message(locale, "constructionRelics", &[("relics", &relics)])
        )
        .expect("string write");
        writeln!(
            output,
            "{}",
            crate::history_state::message(
                locale,
                "constructionPotions",
                &[("potions", &step.potions.join(", "))]
            )
        )
        .expect("string write");
        for candidate in &step.candidates {
            writeln!(
                output,
                "  {}[{}] kind={:?} {}",
                if candidate.selected { "*" } else { " " },
                candidate.ordinal,
                candidate.kind,
                candidate.action,
            )
            .expect("string write");
        }
        writeln!(
            output,
            "  choice={:?} action={}",
            step.choice.ordinal_path, step.selected_action
        )
        .expect("string write");
    }
    output
}

fn render_filtered_history_v1(
    head: &RunPlayHeadV1,
    limit: usize,
    strategic_only: bool,
    locale: Locale,
) -> String {
    let matching = head
        .current_path
        .iter()
        .filter(|step| !strategic_only || step.site != TaskDecisionSiteV1::Combat)
        .collect::<Vec<_>>();
    let skip = matching.len().saturating_sub(limit);
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        crate::history_state::message(
            locale,
            "path",
            &[
                ("total", &head.current_path.len().to_string()),
                ("matched", &matching.len().to_string()),
                ("showing", &matching.len().saturating_sub(skip).to_string()),
                (
                    "filter",
                    &crate::history_state::message(
                        locale,
                        if strategic_only {
                            "strategicFilter"
                        } else {
                            "allFilter"
                        },
                        &[]
                    )
                ),
                ("current", short_id(head.position.archive_node_id())),
            ]
        )
    )
    .expect("string write");
    for step in matching.into_iter().skip(skip) {
        let action = step.semantic_action.clone().unwrap_or_else(|| {
            render_candidate_with_observation(&step.selected_candidate, None, locale)
        });
        writeln!(
            output,
            "  d{:>3} {:<12?} hp={} {} -> {}",
            step.depth,
            step.site,
            format_args!("{}/{}", step.before.current_hp, step.before.max_hp),
            action,
            render_boundary(step.observed_boundary)
        )
        .expect("string write");
    }
    if !head.events.is_empty() {
        writeln!(
            output,
            "{}",
            crate::history_state::message(locale, "controllerEvents", &[])
        )
        .expect("string write");
        for event in head.events.iter().rev().take(limit).rev() {
            match event {
                RunPlayEventV1::Choice {
                    head_revision,
                    from_archive_node_id,
                    to_archive_node_id,
                    choice,
                    strategic_owner_advice,
                    executed_new_branch,
                    outcome,
                    ..
                } => {
                    let committed = render_recorded_committed_action_v1(
                        &head.current_path,
                        from_archive_node_id,
                    );
                    let advice = strategic_owner_advice
                        .as_ref()
                        .map(render_recorded_owner_advice_v1)
                        .unwrap_or_default();
                    writeln!(
                        output,
                        "  h{head_revision} choose {:?} {} -> {} branch={} outcome={:?}{committed}{advice}",
                        choice.ordinal_path,
                        short_id(from_archive_node_id),
                        to_archive_node_id
                            .as_deref()
                            .map(short_id)
                            .unwrap_or("same"),
                        if *executed_new_branch {
                            "new"
                        } else {
                            "existing"
                        },
                        outcome
                    )
                    .expect("string write");
                }
                RunPlayEventV1::AnchorLine {
                    head_revision,
                    from_archive_node_id,
                    to_archive_node_id,
                    choices,
                    state_revision,
                    before_nodes,
                    after_nodes,
                    before_attempts,
                    after_attempts,
                    before_checkpoints,
                    after_checkpoints,
                    combat_research_candidate,
                    combat_milestone_candidate,
                } => {
                    let producer =
                        combat_research_candidate
                            .as_ref()
                            .map_or_else(String::new, |candidate| {
                                format!(
                                    " supporting_producers={} hp={} potions={}",
                                    candidate
                                        .supporting_provenances
                                        .iter()
                                        .map(|provenance| provenance.producer_name.as_str())
                                        .collect::<Vec<_>>()
                                        .join(","),
                                    candidate.outcome.final_hp,
                                    candidate.outcome.total_potions_consumed
                                )
                            });
                    let milestone =
                        combat_milestone_candidate
                            .as_ref()
                            .map_or_else(String::new, |candidate| {
                                format!(
                                    " milestone_producer={} milestones={:?} hp={}",
                                    candidate.provenance.producer_name,
                                    candidate.transition.completed_milestones(),
                                    candidate.final_player_hp
                                )
                            });
                    writeln!(
                        output,
                        "  h{head_revision} anchor-line steps={} {} -> {} state={} nodes={}->{} attempts={}->{} checkpoints={}->{}{producer}{milestone}",
                        choices.len(),
                        short_id(from_archive_node_id),
                        short_id(to_archive_node_id),
                        state_revision,
                        before_nodes,
                        after_nodes,
                        before_attempts,
                        after_attempts,
                        before_checkpoints,
                        after_checkpoints
                    )
                    .expect("string write");
                }
                RunPlayEventV1::Back {
                    head_revision,
                    from_archive_node_id,
                    to_archive_node_id,
                    steps,
                } => {
                    writeln!(
                        output,
                        "  h{head_revision} back {steps} {} -> {}",
                        short_id(from_archive_node_id),
                        short_id(to_archive_node_id)
                    )
                    .expect("string write");
                }
                RunPlayEventV1::Jump {
                    head_revision,
                    from_archive_node_id,
                    to_archive_node_id,
                    selector,
                } => {
                    writeln!(
                        output,
                        "  h{head_revision} jump selector={selector} active_from=n{} selected_to=n{}",
                        short_id(from_archive_node_id),
                        short_id(to_archive_node_id)
                    )
                    .expect("string write");
                }
                RunPlayEventV1::Checkpoint {
                    head_revision,
                    archive_node_id,
                    state_revision,
                    checkpoint_count,
                } => {
                    writeln!(
                        output,
                        "  h{head_revision} checkpoint {} state={} checkpoints={}",
                        short_id(archive_node_id),
                        state_revision,
                        checkpoint_count
                    )
                    .expect("string write");
                }
                RunPlayEventV1::CompactActive {
                    head_revision,
                    archive_node_id,
                    state_revision,
                    before_nodes,
                    after_nodes,
                    before_checkpoints,
                    after_checkpoints,
                } => {
                    writeln!(
                        output,
                        "  h{head_revision} compact-active {} state={} nodes={}->{} checkpoints={}->{}",
                        short_id(archive_node_id),
                        state_revision,
                        before_nodes,
                        after_nodes,
                        before_checkpoints,
                        after_checkpoints
                    )
                    .expect("string write");
                }
            }
        }
    }
    output
}

fn render_recorded_owner_advice_v1(advice: &RunPlayStrategicOwnerAdviceV1) -> String {
    match (&advice.support.outcome, advice.semantic_action.as_deref()) {
        (TaskPrivilegedStrategicSupportOutcomeV1::Choice { .. }, Some(action)) => {
            format!(" advice={action}")
        }
        (TaskPrivilegedStrategicSupportOutcomeV1::Choice { .. }, None) => {
            " advice=<missing-semantic-action>".to_string()
        }
        (TaskPrivilegedStrategicSupportOutcomeV1::Unavailable { reason }, _) => {
            format!(" advice=unavailable:{reason:?}")
        }
    }
}

fn render_recorded_committed_action_v1(
    current_path: &[RunPlayPathStepV1],
    from_archive_node_id: &str,
) -> String {
    let action = current_path
        .iter()
        .position(|step| step.archive_node_id == from_archive_node_id)
        .and_then(|source_index| current_path.get(source_index + 1))
        .and_then(|step| step.semantic_action.as_deref());
    action.map_or_else(
        || " committed=<not-on-active-path>".to_string(),
        |action| format!(" committed={action}"),
    )
}

pub fn render_terminals_v1(
    head: &RunPlayHeadV1,
    archive: &TaskRunSolutionArchiveV1,
    limit: usize,
    locale: Locale,
) -> String {
    let terminals = head
        .events
        .iter()
        .filter_map(|event| match event {
            RunPlayEventV1::Choice {
                head_revision,
                from_archive_node_id,
                site,
                choice,
                outcome: Some(outcome),
                ..
            } if !matches!(
                outcome,
                sts_oracle_learning::agent::task::TaskEpisodeOutcomeV1::Paused { .. }
            ) =>
            {
                Some((
                    *head_revision,
                    from_archive_node_id.clone(),
                    *site,
                    choice.clone(),
                    outcome.clone(),
                ))
            }
            RunPlayEventV1::AnchorLine {
                head_revision,
                to_archive_node_id,
                ..
            } => {
                let node = archive.node(to_archive_node_id)?;
                let incoming = node.incoming_step.as_ref()?;
                let outcome = archive.attempts.iter().rev().find_map(|attempt| {
                    (attempt.node_path.last() == Some(to_archive_node_id)
                        && matches!(
                            attempt.outcome,
                            sts_oracle_learning::agent::task::TaskEpisodeOutcomeV1::Success { .. }
                                | sts_oracle_learning::agent::task::TaskEpisodeOutcomeV1::Failure {
                                    game_outcome: Some(_),
                                    ..
                                }
                        ))
                    .then(|| attempt.outcome.clone())
                })?;
                Some((
                    *head_revision,
                    node.parent_node_id.clone()?,
                    incoming.step.source.site,
                    incoming.step.choice.clone(),
                    outcome,
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let skip = terminals.len().saturating_sub(limit);
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        crate::history_state::message(
            locale,
            "terminals",
            &[
                ("total", &terminals.len().to_string()),
                ("showing", &terminals.len().saturating_sub(skip).to_string()),
            ]
        )
    )
    .expect("string write");
    for (revision, node_id, site, choice, outcome) in terminals.into_iter().skip(skip) {
        writeln!(
            output,
            "  h{revision} source=n{} command=\"jump n{}\" site={site:?} choice={:?} result={}",
            short_id(&node_id),
            short_id(&node_id),
            choice.ordinal_path,
            crate::compact::render_outcome_compact(&outcome, locale)
        )
        .expect("string write");
    }
    output
}

pub(crate) fn render_candidate_with_observation(
    candidate: &TaskPublicCandidateV1,
    observation: Option<&TaskPublicObservationV1>,
    locale: Locale,
) -> String {
    match candidate {
        TaskPublicCandidateV1::Strategic { action } => {
            render_strategic_action(action, observation, locale)
        }
        TaskPublicCandidateV1::CombatAtomic {
            action:
                PublicCombatAtomicActionV1::PlayCard {
                    hand_index,
                    target_monster_index,
                },
        } => {
            let card = observation.and_then(|observation| match observation {
                TaskPublicObservationV1::Combat { observation, .. } => {
                    observation.cards.hand.cards.get(*hand_index)
                }
                TaskPublicObservationV1::Strategic { .. } => None,
            });
            card.map_or_else(
                || format!("play hand[{hand_index}] target={target_monster_index:?}"),
                |card| {
                    format!(
                        "play {} hand[{hand_index}] target={target_monster_index:?} cost={} untargeted_card_damage={} block={}",
                        locale.card_name(card.card_id.stable_id(), card.upgrades),
                        card.effective_cost,
                        card.current_damage,
                        card.current_block
                    )
                },
            )
        }
        TaskPublicCandidateV1::CombatAtomic { action } => format!("combat {action:?}"),
        TaskPublicCandidateV1::CombatIndexedChoice {
            reason, candidate, ..
        } => format!(
            "combat choice {reason:?} {}",
            render_indexed_choice_candidate(*candidate)
        ),
        TaskPublicCandidateV1::CombatSelectionFamily { family } => {
            let domain = family
                .domain
                .iter()
                .enumerate()
                .map(|(selection_ordinal, candidate)| match candidate {
                    PublicCombatSelectionDomainCandidateV1::Card {
                        ordinal,
                        card_id,
                        upgrades,
                        eligible,
                    } => {
                        let card = match (card_id, upgrades) {
                            (Some(card), Some(upgrades)) => {
                                locale.card_name(card.stable_id(), *upgrades)
                            }
                            (Some(card), None) => {
                                locale.entity("card", card.stable_id()).to_owned()
                            }
                            (None, _) => "unknown-card".to_string(),
                        };
                        let token = combat_selection_token_v1(&family.domain, selection_ordinal)
                            .map_or_else(String::new, |token| format!(" token={token}"));
                        format!(
                            "{selection_ordinal}:{card}@source[{ordinal}]{token}{}",
                            if *eligible { "" } else { "(ineligible)" }
                        )
                    }
                    PublicCombatSelectionDomainCandidateV1::Scry {
                        index,
                        card_id,
                        currently_present,
                    } => format!(
                        "{selection_ordinal}:{}@scry[{index}]{}{}",
                        card_id.map_or_else(
                            || "unknown-card".to_string(),
                            |card| locale.entity("card", card.stable_id()).to_owned()
                        ),
                        combat_selection_token_v1(&family.domain, selection_ordinal)
                            .map_or_else(String::new, |token| format!(" token={token}")),
                        if *currently_present { "" } else { "(absent)" }
                    ),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "combat selection {} choices={}..={} input=select:<token>[,<token>...] domain=[{domain}]",
                render_combat_selection_reason_v2(
                    &family.reason,
                    family.hand_copy_projection.as_ref(),
                ),
                family.declared_min,
                family.effective_max
            )
        }
    }
}

fn combat_selection_token_v1(
    domain: &[PublicCombatSelectionDomainCandidateV1],
    selection_ordinal: usize,
) -> Option<String> {
    let selected = domain.get(selection_ordinal)?;
    let (card_id, upgrades, available) = match selected {
        PublicCombatSelectionDomainCandidateV1::Card {
            card_id,
            upgrades,
            eligible,
            ..
        } => (*card_id, *upgrades, *eligible),
        PublicCombatSelectionDomainCandidateV1::Scry {
            card_id,
            currently_present,
            ..
        } => (*card_id, None, *currently_present),
    };
    let card_id = card_id.filter(|_| available)?;
    let occurrence = domain
        .iter()
        .take(selection_ordinal)
        .filter(|candidate| match candidate {
            PublicCombatSelectionDomainCandidateV1::Card {
                card_id: candidate_id,
                upgrades: candidate_upgrades,
                eligible,
                ..
            } => *eligible && *candidate_id == Some(card_id) && *candidate_upgrades == upgrades,
            PublicCombatSelectionDomainCandidateV1::Scry {
                card_id: candidate_id,
                currently_present,
                ..
            } => *currently_present && *candidate_id == Some(card_id) && upgrades.is_none(),
        })
        .count();
    Some(format!(
        "{}#{occurrence}",
        canonical_upgraded_name_v1(card_id.stable_id(), upgrades)
    ))
}

fn render_combat_selection_reason_v2(
    reason: &CombatSelectionReasonV2,
    hand_copy_projection: Option<&PublicHandCopyProjectionV1>,
) -> String {
    match (reason, hand_copy_projection) {
        (
            CombatSelectionReasonV2::Hand(HandSelectReason::Copy { amount: _ }),
            Some(projection),
        ) => format!(
            "hand copy requested_resulting_count={} in_hand_resulting_count={} overflow_to_discard={} net_new_total_copies={} net_new_in_hand_copies={}",
            projection.requested_resulting_count,
            projection.in_hand_resulting_count,
            projection.overflow_to_discard_count,
            projection.net_new_total_copies,
            projection.net_new_in_hand_copies,
        ),
        (CombatSelectionReasonV2::Hand(HandSelectReason::Copy { amount }), None) => format!(
            "hand copy requested_resulting_count={amount} in_hand_resulting_count=unknown overflow_to_discard=unknown net_new_total_copies={}",
            amount.saturating_sub(1)
        ),
        _ => format!("{reason:?}"),
    }
}

pub(crate) fn render_strategic_action(
    action: &TaskStrategicActionV1,
    observation: Option<&TaskPublicObservationV1>,
    locale: Locale,
) -> String {
    let text = |key: &str, parameters: &[(&str, &str)]| {
        locale
            .text("decision", key, parameters)
            .expect("strategic action message")
    };
    match action {
        TaskStrategicActionV1::ChooseRouteNode {
            x,
            y,
            flight,
            question_mark_outcomes,
        } => {
            let public_target = observation.and_then(|observation| match observation {
                TaskPublicObservationV1::Strategic { observation } => observation
                    .public_map
                    .nodes
                    .iter()
                    .find(|node| node.x == *x && node.y == *y),
                TaskPublicObservationV1::Combat { .. } => None,
            });
            let target = public_target.map_or_else(String::new, |node| {
                let room = match node.room {
                    sts_oracle_learning::ai::planner_core::PlannerMapRoom::QuestionMark => {
                        "QuestionMark(?)".to_string()
                    }
                    other => format!("{other:?}"),
                };
                let outcomes = if question_mark_outcomes.is_empty() {
                    String::new()
                } else {
                    format!(
                        " may_resolve_as=[{}]",
                        question_mark_outcomes
                            .iter()
                            .map(|outcome| match outcome {
                                sts_oracle_learning::ai::planner_core::PlannerQuestionMarkOutcome::Event => "event",
                                sts_oracle_learning::ai::planner_core::PlannerQuestionMarkOutcome::Monster => "normal combat",
                                sts_oracle_learning::ai::planner_core::PlannerQuestionMarkOutcome::Shop => "shop",
                                sts_oracle_learning::ai::planner_core::PlannerQuestionMarkOutcome::Treasure => "treasure",
                                sts_oracle_learning::ai::planner_core::PlannerQuestionMarkOutcome::Elite => "elite combat",
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!(
                    " room={room}{outcomes}{}",
                    if node.has_emerald_key {
                        " emerald-key"
                    } else {
                        ""
                    }
                )
            });
            format!(
                "route ({x},{y}){}{target}",
                if *flight { " flight" } else { "" }
            )
        }
        TaskStrategicActionV1::ChooseEventOption {
            event,
            option_index,
            action,
            public_semantics,
            ..
        } => {
            let mut effects = Vec::new();
            let mut push_delta = |label: &str, value: i32| {
                if value != 0 {
                    effects.push(format!("{label}={value:+}"));
                }
            };
            push_delta("gold_min", public_semantics.gold_delta_min);
            push_delta("gold_max", public_semantics.gold_delta_max);
            push_delta("hp", public_semantics.hp_delta);
            push_delta("max_hp", public_semantics.max_hp_delta);
            for (label, value) in [
                ("relics+", public_semantics.relics_gained),
                ("relics-", public_semantics.relics_lost),
                ("potions+", public_semantics.potions_gained),
                ("cards+", public_semantics.cards_gained),
                ("offered", public_semantics.cards_offered),
                ("curses+", public_semantics.curses_gained),
                ("cards-", public_semantics.cards_removed),
                ("upgrades", public_semantics.cards_upgraded),
                ("transforms", public_semantics.cards_transformed),
                ("duplicates", public_semantics.cards_duplicated),
            ] {
                if value != 0 {
                    effects.push(format!("{label}={value}"));
                }
            }
            if !public_semantics.specific_cards_gained.is_empty() {
                effects.push(format!(
                    "gain=[{}]",
                    public_semantics
                        .specific_cards_gained
                        .iter()
                        .map(|card| locale.card_name(card.card.stable_id(), card.upgrades))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
            if !public_semantics.specific_cards_removed.is_empty() {
                effects.push(format!(
                    "remove=[{}]",
                    public_semantics
                        .specific_cards_removed
                        .iter()
                        .map(|card| locale.card_name(card.card.stable_id(), card.upgrades))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
            if public_semantics.random_outcome {
                effects.push(
                    public_semantics
                        .random_outcome_chance_percent
                        .map(|chance| format!("random@{chance}%"))
                        .unwrap_or_else(|| "random".to_string()),
                );
            }
            for (label, enabled) in [
                ("upgrade_all", public_semantics.upgrades_all_cards),
                ("combat", public_semantics.starts_combat),
                ("repeatable", public_semantics.repeatable),
                ("terminal", public_semantics.terminal),
            ] {
                if enabled {
                    effects.push(label.to_string());
                }
            }
            let label = public_semantics
                .public_label
                .as_ref()
                .map_or_else(|| "<unavailable>".to_string(), |label| format!("{label:?}"));
            let mechanics = render_event_structured_semantics_v1(
                public_semantics.structured_semantics.as_ref(),
                locale,
            );
            format!(
                "event {} option={option_index} label={label} action={action:?} mechanics=[{mechanics}] summary=[{}]",
                format_identity(*event),
                effects.join(", ")
            )
        }
        TaskStrategicActionV1::TakeCard { card, upgrades, .. } => text(
            "action.take",
            &[("name", &locale.card_name(card.stable_id(), *upgrades))],
        ),
        TaskStrategicActionV1::OpenCardReward { reward_item_index } => text(
            "action.openCardReward",
            &[("index", &reward_item_index.to_string())],
        ),
        TaskStrategicActionV1::SingingBowl { .. } => text(
            "action.singingBowl",
            &[(
                "amount",
                &sts_oracle_learning::content::relics::singing_bowl::MAX_HP_GAIN.to_string(),
            )],
        ),
        TaskStrategicActionV1::SkipCardReward { .. } => text("action.skipCard", &[]),
        TaskStrategicActionV1::ClaimReward {
            reward_item_index,
            reward,
        } => {
            let mut label = text(
                "action.claim",
                &[
                    ("index", &reward_item_index.to_string()),
                    ("reward", &render_reward_descriptor(reward, locale)),
                ],
            );
            if let Some(TaskPublicObservationV1::Strategic {
                observation:
                    TaskStrategicObservationV1 {
                        context:
                            sts_oracle_learning::ai::planner_core::PlannerDecisionContext::Reward {
                                pending_rewards,
                            },
                        ..
                    },
            }) = observation
            {
                // The engine links the key to its immediately preceding relic.
                // Matryoshka's extra relic appears earlier and remains available.
                match reward {
                    PlannerRewardDescriptor::SapphireKey => {
                        if let Some(linked_index) = reward_item_index.checked_sub(1) {
                            if let Some(linked @ PlannerRewardDescriptor::Relic { .. }) =
                                pending_rewards.get(linked_index)
                            {
                                label.push_str(&text(
                                    "action.alternative",
                                    &[
                                        ("reward", &render_reward_descriptor(linked, locale)),
                                        ("index", &linked_index.to_string()),
                                    ],
                                ));
                            }
                        }
                    }
                    PlannerRewardDescriptor::Relic { .. }
                        if matches!(
                            pending_rewards.get(reward_item_index + 1),
                            Some(PlannerRewardDescriptor::SapphireKey)
                        ) =>
                    {
                        label.push_str(&text("action.alternativeKey", &[]));
                    }
                    _ => {}
                }
            }
            label
        }
        TaskStrategicActionV1::Rest => text("action.restSimple", &[]),
        TaskStrategicActionV1::Smith {
            card_ordinal,
            card,
            upgrades,
        } => text(
            "action.smith",
            &[
                ("card", &locale.card_name(card.stable_id(), *upgrades)),
                ("index", &card_ordinal.to_string()),
            ],
        ),
        TaskStrategicActionV1::Dig => text("action.dig", &[]),
        TaskStrategicActionV1::Lift => text("action.lift", &[]),
        TaskStrategicActionV1::Toke {
            card_ordinal,
            card,
            upgrades,
        } => text(
            "action.toke",
            &[
                ("card", &locale.card_name(card.stable_id(), *upgrades)),
                ("index", &card_ordinal.to_string()),
            ],
        ),
        TaskStrategicActionV1::Recall => text("action.recall", &[]),
        TaskStrategicActionV1::BuyCard {
            card,
            upgrades,
            price,
            ..
        } => text(
            "action.buy",
            &[
                ("name", &locale.card_name(card.stable_id(), *upgrades)),
                ("price", &price.to_string()),
            ],
        ),
        TaskStrategicActionV1::BuyRelic { relic, price, .. } => text(
            "action.buy",
            &[
                ("name", locale.entity("relic", relic.stable_id())),
                ("price", &price.to_string()),
            ],
        ),
        TaskStrategicActionV1::BuyPotion { potion, price, .. } => text(
            "action.buy",
            &[
                ("name", locale.entity("potion", potion.stable_id())),
                ("price", &price.to_string()),
            ],
        ),
        TaskStrategicActionV1::UseRunPotion { slot, potion } => text(
            "action.usePotion",
            &[
                ("slot", &slot.to_string()),
                ("name", locale.entity("potion", potion.stable_id())),
            ],
        ),
        TaskStrategicActionV1::DiscardRunPotion { slot, potion } => text(
            "action.discardPotion",
            &[
                ("slot", &slot.to_string()),
                ("name", locale.entity("potion", potion.stable_id())),
            ],
        ),
        TaskStrategicActionV1::RemoveCard {
            card_ordinal,
            card,
            upgrades,
            price,
            ..
        } => text(
            "action.remove",
            &[
                ("card", &locale.card_name(card.stable_id(), *upgrades)),
                ("index", &card_ordinal.to_string()),
                ("price", &price.to_string()),
            ],
        ),
        TaskStrategicActionV1::OpenPendingRewards => text("action.pendingRewards", &[]),
        TaskStrategicActionV1::LeaveShop => text("action.leaveShop", &[]),
        TaskStrategicActionV1::TakeBossRelic { relic, .. } => text(
            "action.take",
            &[("name", locale.entity("relic", relic.stable_id()))],
        ),
        TaskStrategicActionV1::SkipBossRelic => text("action.skipBossRelic", &[]),
        TaskStrategicActionV1::BeginRunCardSelection {
            reason,
            min_choices,
            max_choices,
            selectable_card_ordinals,
            ..
        } => {
            let domain = observation
                .and_then(|observation| match observation {
                    TaskPublicObservationV1::Strategic { observation } => Some(observation),
                    TaskPublicObservationV1::Combat { .. } => None,
                })
                .map(|observation| {
                    selectable_card_ordinals
                        .iter()
                        .enumerate()
                        .map(|(selection_ordinal, card_ordinal)| {
                            observation
                                .cards
                                .iter()
                                .find(|card| card.ordinal == *card_ordinal)
                                .map_or_else(
                                    || format!("{selection_ordinal}:deck[{card_ordinal}]"),
                                    |card| {
                                        format!(
                                            "{selection_ordinal}:{}@deck[{card_ordinal}]{}",
                                            locale.card_name(card.card.stable_id(), card.upgrades),
                                            run_selection_reward_label_v1(
                                                observation,
                                                card.card,
                                                locale
                                            )
                                            .map_or_else(String::new, |label| format!(
                                                " ({label})"
                                            ))
                                        )
                                    },
                                )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| {
                    selectable_card_ordinals
                        .iter()
                        .enumerate()
                        .map(|(selection_ordinal, card_ordinal)| {
                            format!("{selection_ordinal}:deck[{card_ordinal}]")
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                });
            format!(
                "begin selection {reason:?} choices={min_choices}..={max_choices} input=<candidate> <selection ordinals...> domain=[{domain}]"
            )
        }
        TaskStrategicActionV1::SubmitRunSelection {
            selected_card_ordinals,
            ..
        } => {
            let selected = observation
                .and_then(|observation| match observation {
                    TaskPublicObservationV1::Strategic { observation } => Some(observation),
                    TaskPublicObservationV1::Combat { .. } => None,
                })
                .map(|observation| {
                    selected_card_ordinals
                        .iter()
                        .map(|card_ordinal| {
                            observation
                                .cards
                                .iter()
                                .find(|card| card.ordinal == *card_ordinal)
                                .map_or_else(
                                    || format!("deck[{card_ordinal}]"),
                                    |card| {
                                        format!(
                                            "{}@deck[{card_ordinal}]{}",
                                            locale.card_name(card.card.stable_id(), card.upgrades),
                                            run_selection_reward_label_v1(
                                                observation,
                                                card.card,
                                                locale
                                            )
                                            .map_or_else(String::new, |label| format!(
                                                " ({label})"
                                            ))
                                        )
                                    },
                                )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| {
                    selected_card_ordinals
                        .iter()
                        .map(|card_ordinal| format!("deck[{card_ordinal}]"))
                        .collect::<Vec<_>>()
                        .join(", ")
                });
            format!("submit {selected}")
        }
        TaskStrategicActionV1::OpenChest => text("action.openChest", &[]),
        TaskStrategicActionV1::Proceed {
            site: sts_oracle_learning::ai::planner_core::PlannerDecisionSite::Treasure,
        } => text("action.leaveChest", &[]),
        TaskStrategicActionV1::Proceed {
            site: sts_oracle_learning::ai::planner_core::PlannerDecisionSite::RunTransition,
        } => match observation {
            Some(TaskPublicObservationV1::Strategic { observation }) => match &observation.context {
                sts_oracle_learning::ai::planner_core::PlannerDecisionContext::RunTransition {
                    stage,
                } => crate::decision::run_transition_label_v1(*stage, locale).into(),
                _ => text("action.proceed", &[]),
            },
            _ => text("action.proceed", &[]),
        },
        TaskStrategicActionV1::Proceed { site } => {
            text("action.proceedSite", &[("site", &format!("{site:?}"))])
        }
        TaskStrategicActionV1::Cancel { site } => {
            text("action.cancel", &[("site", &format!("{site:?}"))])
        }
    }
}

fn event_text(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("event", key, parameters)
        .expect("event reading message")
}

pub(crate) fn render_event_structured_semantics_v1(
    structured: Option<&PlannerEventOptionStructuredSemanticsV1>,
    locale: Locale,
) -> String {
    let Some(structured) = structured else {
        return event_text(locale, "unavailable", &[]);
    };
    let mut parts = structured
        .effects
        .iter()
        .filter(|effect| {
            !(matches!(effect, PlannerEventPublicEffectV1::StartCombat)
                && structured.transition == PlannerEventTransitionV1::StartCombat)
        })
        .map(|effect| render_event_public_effect_v1(effect, locale))
        .collect::<Vec<_>>();
    if structured.effects.iter().any(|effect| {
        matches!(
            effect,
            PlannerEventPublicEffectV1::ObtainRandomPotions { .. }
        )
    }) {
        let delivery = match structured.transition {
            PlannerEventTransitionV1::OpenReward => Some("potions.reward"),
            PlannerEventTransitionV1::None => Some("potions.immediate"),
            _ => None,
        };
        if let Some(key) = delivery {
            parts.push(event_text(locale, key, &[]));
        }
    }
    parts.push(render_event_transition_v1(structured.transition, locale));
    parts.join(" ")
}

pub(crate) fn render_event_public_effect_v1(
    effect: &PlannerEventPublicEffectV1,
    locale: Locale,
) -> String {
    use sts_oracle_learning::ai::planner_core::PlannerEventRandomOutcomeV1 as Random;
    let count_text = |key: &str, count: usize, parameters: &[(&str, &str)]| {
        let count_value = count.to_string();
        let mut parameters = parameters.to_vec();
        parameters.push(("count", &count_value));
        event_text(
            locale,
            &format!("effect.{key}_{}", if count == 1 { "one" } else { "other" }),
            &parameters,
        )
    };
    let amount_text = |key: &str, amount: i32| {
        event_text(
            locale,
            &format!("effect.{key}"),
            &[("amount", &amount.to_string())],
        )
    };
    match effect {
        PlannerEventPublicEffectV1::GainGold { amount } => amount_text("gainGold", *amount),
        PlannerEventPublicEffectV1::GainGoldRange { min, max } => event_text(
            locale,
            "effect.gainGoldRange",
            &[("min", &min.to_string()), ("max", &max.to_string())],
        ),
        PlannerEventPublicEffectV1::LoseGold { amount } => amount_text("loseGold", *amount),
        PlannerEventPublicEffectV1::LoseHp { amount } => amount_text("loseHp", *amount),
        PlannerEventPublicEffectV1::LoseMaxHp { amount } => amount_text("loseMaxHp", *amount),
        PlannerEventPublicEffectV1::Heal { amount } => amount_text("heal", *amount),
        PlannerEventPublicEffectV1::GainMaxHp { amount } => amount_text("gainMaxHp", *amount),
        PlannerEventPublicEffectV1::ObtainRelic { count, pool } => count_text(
            "obtainRelic",
            *count,
            &[("pool", &render_event_relic_pool_v1(pool, locale))],
        ),
        PlannerEventPublicEffectV1::ObtainRandomPotions { count } => {
            count_text("potions", *count, &[])
        }
        PlannerEventPublicEffectV1::ObtainCard { count, pool } => count_text(
            "obtainCard",
            *count,
            &[("pool", &render_event_card_pool_v1(pool, locale))],
        ),
        PlannerEventPublicEffectV1::ObtainColorlessCard { count, pool } => count_text(
            "colorless",
            *count,
            &[("pool", &render_event_card_pool_v1(pool, locale))],
        ),
        PlannerEventPublicEffectV1::OfferCards { count, pool } => count_text(
            "offer",
            *count,
            &[("pool", &render_event_card_pool_v1(pool, locale))],
        ),
        PlannerEventPublicEffectV1::ObtainCurse { count, pool } => count_text(
            "curse",
            *count,
            &[("pool", &render_event_card_pool_v1(pool, locale))],
        ),
        PlannerEventPublicEffectV1::RemoveCard {
            count,
            target,
            pool,
        } => {
            let mut result = count_text(
                "remove",
                *count,
                &[("pool", &render_event_card_pool_v1(pool, locale))],
            );
            if let Some(card) = target {
                result.push(' ');
                result.push_str(&event_text(
                    locale,
                    "effect.removeTarget",
                    &[
                        (
                            "name",
                            &locale.card_name(card.card.stable_id(), card.upgrades),
                        ),
                        ("id", card.card.stable_id()),
                    ],
                ));
            } else {
                result.push(' ');
                result.push_str(&event_text(locale, "effect.removeTargetUnknown", &[]));
            }
            result
        }
        PlannerEventPublicEffectV1::UpgradeCard { count } => count_text("upgrade", *count, &[]),
        PlannerEventPublicEffectV1::UpgradeAllCards => event_text(locale, "effect.upgradeAll", &[]),
        PlannerEventPublicEffectV1::TransformCard { count } => count_text("transform", *count, &[]),
        PlannerEventPublicEffectV1::DuplicateCard { count } => count_text("duplicate", *count, &[]),
        PlannerEventPublicEffectV1::LoseRelic {
            specific,
            starter_only,
        } => {
            let name = specific.map(|relic| {
                event_text(
                    locale,
                    "identity",
                    &[
                        ("name", locale.entity("relic", relic.stable_id())),
                        ("id", relic.stable_id()),
                    ],
                )
            });
            let key = match (name.is_some(), starter_only) {
                (true, true) => "effect.loseNamedStarter",
                (true, false) => "effect.loseNamedRelic",
                (false, true) => "effect.loseStarter",
                (false, false) => "effect.loseRelic",
            };
            event_text(locale, key, &[("name", name.as_deref().unwrap_or(""))])
        }
        PlannerEventPublicEffectV1::LoseStarterRelic { specific } => match specific {
            Some(relic) => event_text(
                locale,
                "effect.loseNamedStarter",
                &[(
                    "name",
                    &event_text(
                        locale,
                        "identity",
                        &[
                            ("name", locale.entity("relic", relic.stable_id())),
                            ("id", relic.stable_id()),
                        ],
                    ),
                )],
            ),
            None => event_text(locale, "effect.loseStarter", &[]),
        },
        PlannerEventPublicEffectV1::RandomOutcome {
            outcome: Random::JoustOwnerWin,
            chance_percent: Some(chance),
        } => event_text(
            locale,
            "joust",
            &[
                ("owner", &chance.to_string()),
                ("murderer", &100_u8.saturating_sub(*chance).to_string()),
            ],
        ),
        PlannerEventPublicEffectV1::RandomOutcome {
            outcome,
            chance_percent,
        } => {
            let key = match outcome {
                Random::GremlinWheel => "wheel",
                Random::JoustOwnerWin => "joust",
                Random::DeadAdventurerSearch => "adventurer",
                Random::MatchAndKeepFlip => "flip",
                Random::ScrapOozeRelicRoll => "ooze",
                Random::MausoleumCurseRoll => "mausoleum",
            };
            let chance = chance_percent.map_or_else(
                || event_text(locale, "chanceUnknown", &[]),
                |chance| event_text(locale, "chance", &[("chance", &chance.to_string())]),
            );
            event_text(
                locale,
                "random",
                &[
                    (
                        "outcome",
                        &event_text(locale, &format!("outcome.{key}"), &[]),
                    ),
                    ("chance", &chance),
                ],
            )
        }
        PlannerEventPublicEffectV1::StartCombat => event_text(locale, "effect.combat", &[]),
    }
}

fn render_event_card_pool_v1(pool: &PlannerEventCardPoolV1, locale: Locale) -> String {
    let key = match pool {
        PlannerEventCardPoolV1::Specific { card, upgrades } => {
            return event_text(
                locale,
                "identity",
                &[
                    ("name", &locale.card_name(card.stable_id(), *upgrades)),
                    ("id", card.stable_id()),
                ],
            )
        }
        PlannerEventCardPoolV1::Unknown => "unknown",
        PlannerEventCardPoolV1::RandomCurse => "curse",
        PlannerEventCardPoolV1::RandomClassCard => "class",
        PlannerEventCardPoolV1::RandomClassCommonOrUncommon => "classCommonUncommon",
        PlannerEventCardPoolV1::RandomClassRare => "classRare",
        PlannerEventCardPoolV1::RandomColorless => "colorless",
        PlannerEventCardPoolV1::RandomColorlessUncommon => "colorlessUncommon",
        PlannerEventCardPoolV1::RandomColorlessRare => "colorlessRare",
    };
    event_text(locale, &format!("cardPool.{key}"), &[])
}

fn render_event_relic_pool_v1(pool: &PlannerEventRelicPoolV1, locale: Locale) -> String {
    let key = match pool {
        PlannerEventRelicPoolV1::Specific { relic } => {
            return event_text(
                locale,
                "identity",
                &[
                    ("name", locale.entity("relic", relic.stable_id())),
                    ("id", relic.stable_id()),
                ],
            )
        }
        PlannerEventRelicPoolV1::Unknown => "unknown",
        PlannerEventRelicPoolV1::RandomCommonRelic => "common",
        PlannerEventRelicPoolV1::RandomUncommonRelic => "uncommon",
        PlannerEventRelicPoolV1::RandomRareRelic => "rare",
        PlannerEventRelicPoolV1::RandomShopRelic => "shop",
        PlannerEventRelicPoolV1::RandomBossRelic => "boss",
        PlannerEventRelicPoolV1::RandomRelic => "any",
        PlannerEventRelicPoolV1::RandomBook => "book",
        PlannerEventRelicPoolV1::RandomFace => "face",
    };
    event_text(locale, &format!("relicPool.{key}"), &[])
}

fn render_event_transition_v1(transition: PlannerEventTransitionV1, locale: Locale) -> String {
    let key = match transition {
        PlannerEventTransitionV1::None => "stay",
        PlannerEventTransitionV1::AdvanceScreen => "advance",
        PlannerEventTransitionV1::Complete => "complete",
        PlannerEventTransitionV1::OpenSelection { selection } => match selection {
            PlannerEventSelectionV1::None => "selection",
            PlannerEventSelectionV1::RemoveCard => "remove",
            PlannerEventSelectionV1::UpgradeCard => "upgrade",
            PlannerEventSelectionV1::TransformCard => "transform",
            PlannerEventSelectionV1::DuplicateCard => "duplicate",
            PlannerEventSelectionV1::OfferCard => "offer",
        },
        PlannerEventTransitionV1::OpenReward => "reward",
        PlannerEventTransitionV1::StartCombat => "combat",
    };
    event_text(locale, &format!("transition.{key}"), &[])
}

fn render_indexed_choice_candidate(candidate: PublicCombatIndexedChoiceCandidateV1) -> String {
    match candidate {
        PublicCombatIndexedChoiceCandidateV1::Card { card_id, upgrades } => {
            format_card(card_id, upgrades)
        }
        PublicCombatIndexedChoiceCandidateV1::Stance { stance } => format_identity(stance),
    }
}

pub(crate) fn render_reward_descriptor(reward: &PlannerRewardDescriptor, locale: Locale) -> String {
    let text = |key: &str, parameters: &[(&str, &str)]| {
        locale
            .text("decision", key, parameters)
            .expect("reward message")
    };
    match reward {
        PlannerRewardDescriptor::Gold { amount } => {
            text("reward.gold", &[("amount", &amount.to_string())])
        }
        PlannerRewardDescriptor::StolenGold { amount } => {
            text("reward.stolenGold", &[("amount", &amount.to_string())])
        }
        PlannerRewardDescriptor::CardReward { cards } => text(
            "reward.card",
            &[(
                "cards",
                &cards
                    .iter()
                    .map(|card| locale.card_name(card.card.stable_id(), card.upgrades))
                    .collect::<Vec<_>>()
                    .join(", "),
            )],
        ),
        PlannerRewardDescriptor::Relic { relic } => text(
            "reward.relic",
            &[("name", locale.entity("relic", relic.stable_id()))],
        ),
        PlannerRewardDescriptor::Potion { potion } => text(
            "reward.potion",
            &[("name", locale.entity("potion", potion.stable_id()))],
        ),
        PlannerRewardDescriptor::EmeraldKey => text("reward.emeraldKey", &[]),
        PlannerRewardDescriptor::SapphireKey => text("reward.sapphireKey", &[]),
    }
}

pub(crate) fn render_proof(proof: &TaskRunArchiveNodeProofV1) -> &'static str {
    match proof {
        TaskRunArchiveNodeProofV1::ReachableUnknown => "unknown",
        TaskRunArchiveNodeProofV1::ProvenVictory { .. } => "victory",
    }
}

fn render_boundary(boundary: RunPlayBoundaryKindV1) -> &'static str {
    match boundary {
        RunPlayBoundaryKindV1::Decision => "decision",
        RunPlayBoundaryKindV1::CommitReady => "commit-ready",
        RunPlayBoundaryKindV1::ExploredScopeExit => "scope-exit",
        RunPlayBoundaryKindV1::CommittedNextScope => "next-scope",
        RunPlayBoundaryKindV1::CommittedGameTerminal => "terminal",
    }
}

pub(crate) fn format_card(
    card: sts_oracle_learning::content::cards::CardId,
    upgrades: u8,
) -> String {
    let name = format_identity(card);
    if upgrades == 0 {
        name
    } else {
        crate::selector::canonical_upgraded_name_v1(&name, Some(upgrades))
    }
}

fn format_identity<T: GameplayIdentitySurface>(identity: T) -> String {
    gameplay_display_label_v1(identity).to_string()
}

pub(crate) fn short_id(value: &str) -> &str {
    &value[value.len().saturating_sub(12)..]
}

#[allow(dead_code)]
fn _choice_label(choice: &TaskPublicChoiceV1) -> String {
    choice
        .ordinal_path
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_learning::agent::task::{TaskStrategicPotionSlotV1, TaskStrategicPotionV1};
    use sts_oracle_learning::ai::planner_core::{
        PlannerEventOptionPublicSemanticsV1, PlannerOfferedCard, PlannerPublicMapEdge,
        PlannerPublicMapNode,
    };
    use sts_oracle_learning::content::cards::CardId;
    use sts_oracle_learning::content::monsters::factory::EncounterId;
    use sts_oracle_learning::content::relics::RelicId;
    use sts_oracle_learning::eval::run_control::{
        BossRelicPolicyAuditCandidateV1, BossRelicPolicyAuditContextV1, BossRelicPolicyBandV1,
        DecisionCandidateKey,
    };
    use sts_oracle_learning::state::events::{EventActionKind, EventId};
    use sts_oracle_learning::state::map::node::RoomType;

    #[test]
    fn dual_wield_selection_explains_hand_capacity_and_overflow() {
        let projection = PublicHandCopyProjectionV1 {
            requested_resulting_count: 3,
            in_hand_resulting_count: 2,
            overflow_to_discard_count: 1,
            net_new_total_copies: 2,
            net_new_in_hand_copies: 1,
        };
        assert_eq!(
            render_combat_selection_reason_v2(
                &CombatSelectionReasonV2::Hand(HandSelectReason::Copy { amount: 3 }),
                Some(&projection),
            ),
            "hand copy requested_resulting_count=3 in_hand_resulting_count=2 overflow_to_discard=1 net_new_total_copies=2 net_new_in_hand_copies=1"
        );
    }

    #[test]
    fn combat_selection_tokens_match_the_semantic_select_parser() {
        let domain = vec![
            PublicCombatSelectionDomainCandidateV1::Card {
                ordinal: 4,
                card_id: Some(CardId::Strike),
                upgrades: Some(0),
                eligible: true,
            },
            PublicCombatSelectionDomainCandidateV1::Card {
                ordinal: 7,
                card_id: Some(CardId::Defend),
                upgrades: Some(0),
                eligible: true,
            },
            PublicCombatSelectionDomainCandidateV1::Card {
                ordinal: 9,
                card_id: Some(CardId::Strike),
                upgrades: Some(0),
                eligible: true,
            },
        ];

        assert_eq!(
            combat_selection_token_v1(&domain, 0).as_deref(),
            Some("Strike+0#0")
        );
        assert_eq!(
            combat_selection_token_v1(&domain, 1).as_deref(),
            Some("Defend+0#0")
        );
        assert_eq!(
            combat_selection_token_v1(&domain, 2).as_deref(),
            Some("Strike+0#1")
        );
    }

    #[test]
    fn localized_event_effects_keep_costs_order_targets_and_followup_selection_distinct() {
        let semantics = PlannerEventOptionStructuredSemanticsV1 {
            effects: vec![
                PlannerEventPublicEffectV1::LoseHp { amount: 18 },
                PlannerEventPublicEffectV1::RemoveCard {
                    count: 2,
                    target: None,
                    pool: PlannerEventCardPoolV1::Unknown,
                },
            ],
            transition: PlannerEventTransitionV1::OpenSelection {
                selection: PlannerEventSelectionV1::RemoveCard,
            },
        };
        let before = semantics.clone();
        let en = render_event_structured_semantics_v1(Some(&semantics), Locale::En);
        let zh = render_event_structured_semantics_v1(Some(&semantics), Locale::ZhCn);
        assert!(en.find("Lose 18 HP.").unwrap() < en.find("Permanently remove 2 cards").unwrap());
        assert!(zh.find("失去 18 点生命。").unwrap() < zh.find("永久移除 2 张牌").unwrap());
        assert!(zh.contains("未指定的牌池") && zh.contains("尚未指定具体目标"));
        assert!(zh.contains("具体卡牌在下一屏选择"));
        assert!(!zh.contains("生命上限"));
        assert_eq!(semantics, before);

        let specific = PlannerEventPublicEffectV1::ObtainCard {
            count: 1,
            pool: PlannerEventCardPoolV1::Specific {
                card: CardId::SearingBlow,
                upgrades: 2,
            },
        };
        let zh = render_event_public_effect_v1(&specific, Locale::ZhCn);
        assert!(zh.contains("永久获得 1 张牌：灼热攻击+2 [SearingBlow]"));
        assert!(!zh.contains("随机"));
    }

    #[test]
    fn localized_random_event_effects_do_not_turn_an_outcome_range_into_a_guaranteed_prize() {
        use sts_oracle_learning::ai::planner_core::PlannerEventRandomOutcomeV1 as Random;
        let semantics = PlannerEventOptionStructuredSemanticsV1 {
            effects: vec![
                PlannerEventPublicEffectV1::LoseGold { amount: 50 },
                PlannerEventPublicEffectV1::GainGoldRange { min: 0, max: 250 },
                PlannerEventPublicEffectV1::RandomOutcome {
                    outcome: Random::JoustOwnerWin,
                    chance_percent: Some(30),
                },
            ],
            transition: PlannerEventTransitionV1::AdvanceScreen,
        };
        let zh = render_event_structured_semantics_v1(Some(&semantics), Locale::ZhCn);
        assert!(zh.contains("失去 50 金币") && zh.contains("依结果获得 0–250 金币"));
        assert!(zh.contains("宠物主人 30%，杀宠凶手 70%"));
        assert!(zh.contains("否则奖金为 0 金币"));
        assert!(zh.contains("进入事件下一阶段"));
        let unknown = render_event_public_effect_v1(
            &PlannerEventPublicEffectV1::RandomOutcome {
                outcome: Random::ScrapOozeRelicRoll,
                chance_percent: None,
            },
            Locale::ZhCn,
        );
        assert!(unknown.contains("概率未提供"));
        assert!(!unknown.contains('%'));
        let zero = render_event_public_effect_v1(
            &PlannerEventPublicEffectV1::RandomOutcome {
                outcome: Random::ScrapOozeRelicRoll,
                chance_percent: Some(0),
            },
            Locale::ZhCn,
        );
        assert!(zero.contains("概率 0%"));
        assert!(!zero.contains("概率未提供"));
    }

    #[test]
    fn random_curse_pool_renders_as_known_instead_of_unknown() {
        assert_eq!(
            render_event_card_pool_v1(&PlannerEventCardPoolV1::RandomCurse, Locale::En),
            "random ordinary curses"
        );
    }

    #[test]
    fn compact_route_view_stops_at_the_first_public_branch() {
        let map = PlannerPublicMap {
            current_x: 3,
            current_y: 6,
            boss: Some(EncounterId::SlimeBoss),
            nodes: vec![
                public_map_node(3, 6, RoomType::MonsterRoomElite, &[(2, 7)]),
                public_map_node(2, 7, RoomType::MonsterRoom, &[(2, 8)]),
                public_map_node(2, 8, RoomType::TreasureRoom, &[(1, 9)]),
                public_map_node(1, 9, RoomType::MonsterRoom, &[(0, 10), (1, 10)]),
                public_map_node(0, 10, RoomType::RestRoom, &[]),
                public_map_node(1, 10, RoomType::MonsterRoom, &[]),
            ],
        };

        let prefix = render_single_route_v1(&map).unwrap();
        assert!(prefix.find("(2,7)").unwrap() < prefix.find("(2,8)").unwrap());
        assert!(prefix.find("(2,8)").unwrap() < prefix.find("(1,9)").unwrap());
        assert!(!prefix.contains("(0,10)"));
        assert!(!prefix.contains("(1,10)"));
    }

    fn public_map_node(
        x: i32,
        y: i32,
        room: RoomType,
        destinations: &[(i32, i32)],
    ) -> PlannerPublicMapNode {
        PlannerPublicMapNode {
            x,
            y,
            room: room.into(),
            has_emerald_key: false,
            edges: destinations
                .iter()
                .map(|(destination_x, destination_y)| PlannerPublicMapEdge {
                    destination_x: *destination_x,
                    destination_y: *destination_y,
                })
                .collect(),
        }
    }

    #[test]
    fn combat_history_renders_jumpable_source_and_target_nodes() {
        let history = RunPlayCombatHistoryV1 {
            current_archive_node_id: "task_run_archive_node_v1:cccccccccccc".to_string(),
            total_combat_steps: 1,
            steps: vec![crate::driver::RunPlayCombatHistoryStepV1 {
                from_archive_node_id: "task_run_archive_node_v1:aaaaaaaaaaaa".to_string(),
                path: crate::model::RunPlayPathStepV1 {
                    archive_node_id: "task_run_archive_node_v1:bbbbbbbbbbbb".to_string(),
                    depth: 7,
                    site: TaskDecisionSiteV1::Combat,
                    before: crate::model::RunPlayPublicScalarsV1 {
                        act: Some(3),
                        floor: Some(36),
                        current_hp: 34,
                        max_hp: 39,
                        gold: 286,
                    },
                    combat_before: None,
                    choice: TaskPublicChoiceV1::one(0),
                    selected_candidate: TaskPublicCandidateV1::CombatAtomic {
                        action: PublicCombatAtomicActionV1::EndTurn,
                    },
                    semantic_action: Some("end".to_string()),
                    observed_boundary: RunPlayBoundaryKindV1::Decision,
                },
                source: {
                    let directory = crate::decision_tests::TestDirectory::new();
                    let game = crate::decision_tests::controller_with_session(
                        &directory.0,
                        "combat-history-view",
                        &crate::decision_tests::fixture("combat_multiturn"),
                    );
                    game.head()
                        .current_decision
                        .as_ref()
                        .unwrap()
                        .public_decision_v1()
                        .unwrap()
                        .observation
                },
            }],
        };

        let rendered = render_combat_history_v1(&history, Locale::En);
        assert!(rendered.contains("from=naaaaaaaaaaaa to=nbbbbbbbbbbbb"));
        let original = history.clone();
        let chinese = render_combat_history_v1(&history, Locale::ZhCn);
        assert!(chinese.contains("from=naaaaaaaaaaaa to=nbbbbbbbbbbbb action=end"));
        assert!(chinese.starts_with("战斗路径："));
        assert!(chinese.contains("打击") && rendered.contains("Strike"));
        assert_eq!(history, original);
    }

    #[test]
    fn combat_history_rereads_actual_public_source_in_both_languages_without_state_changes() {
        let directory = crate::decision_tests::TestDirectory::new();
        let mut game = crate::decision_tests::controller_with_session(
            &directory.0,
            "history-source",
            &crate::decision_tests::fixture("combat_multiturn"),
        );
        let source = game
            .head()
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap()
            .observation;
        game.choose_action_v1(
            &crate::selector::parse_action_selector_v1(
                "play:Strike@0",
                "history test",
                sts_presentation::Locale::En,
            )
            .unwrap(),
            true,
            sts_presentation::Locale::En,
        )
        .unwrap();
        let head = game.head().clone();
        let archive = game.archive().clone();
        let fingerprints = game.current_state_fingerprints_v1().unwrap();
        let history = crate::driver::combat_history_v1(
            game.archive(),
            game.head().position.archive_node_id(),
            40,
            None,
        )
        .unwrap();
        assert_eq!(history.steps.len(), 1);
        assert_eq!(history.steps[0].source, source);
        let english = render_combat_history_v1(&history, Locale::En);
        let chinese = render_combat_history_v1(&history, Locale::ZhCn);
        assert!(english.contains("Strike") && chinese.contains("打击"));
        assert!(english.contains("play:Strike") && chinese.contains("play:Strike"));
        assert_eq!(game.head(), &head);
        assert_eq!(game.archive(), &archive);
        assert_eq!(game.current_state_fingerprints_v1().unwrap(), fingerprints);
    }

    #[test]
    fn terminal_history_keeps_actual_jump_source_and_localizes_committed_defeat() {
        let directory = crate::decision_tests::TestDirectory::new();
        let mut game = crate::decision_tests::controller_with_session(
            &directory.0,
            "terminal-language",
            &crate::decision_tests::fixture("combat_defeat"),
        );
        game.choose_action_v1(
            &crate::selector::parse_action_selector_v1(
                "end",
                "terminal test",
                sts_presentation::Locale::En,
            )
            .unwrap(),
            true,
            sts_presentation::Locale::En,
        )
        .unwrap();
        let head = game.head().clone();
        let archive = game.archive().clone();
        let english = render_terminals_v1(&head, &archive, 8, Locale::En);
        let chinese = render_terminals_v1(&head, &archive, 8, Locale::ZhCn);
        assert!(english.starts_with("Terminal outcomes: 1"), "{english}");
        assert!(chinese.starts_with("终局记录：1"), "{chinese}");
        assert!(english.contains("result=Defeat") && chinese.contains("result=失败"));
        let english_source = english
            .lines()
            .find(|line| line.contains("command=\"jump n"))
            .unwrap()
            .split(" result=")
            .next()
            .unwrap();
        assert!(chinese.lines().any(|line| line.starts_with(english_source)));
        assert_eq!(game.head(), &head);
        assert_eq!(game.archive(), &archive);
    }

    #[test]
    fn construction_history_uses_permanent_card_identity_and_preserves_json() {
        let directory = crate::decision_tests::TestDirectory::new();
        let mut game = crate::decision_tests::controller_with_session(
            &directory.0,
            "construction-language",
            &crate::decision_tests::fixture("reward"),
        );
        game.choose_action_v1(
            &crate::selector::parse_action_selector_v1(
                "take:ShrugItOff",
                "construction test",
                sts_presentation::Locale::En,
            )
            .unwrap(),
            true,
            sts_presentation::Locale::En,
        )
        .unwrap();
        let paths = crate::storage::RunPlayPathsV1::new(directory.0.join("construction-language"));
        let mut history =
            crate::driver::construction_history_v1(&paths, None, None, None, false).unwrap();
        assert!(!history.steps.is_empty());
        // Construct a collision and a different permanent cost on that same retained public report.
        let mut red = history.steps[0].deck[0];
        red.card = sts_oracle_learning::content::cards::CardId::Strike;
        let mut green = red;
        green.card = sts_oracle_learning::content::cards::CardId::StrikeG;
        green.ordinal += 1;
        let mut modified = red;
        modified.cost_modifier += 2;
        modified.ordinal += 2;
        history.steps[0].deck = vec![red, green, modified];
        let original = serde_json::to_value(&history).unwrap();
        let fingerprint = game.current_state_fingerprints_v1().unwrap();
        for locale in [Locale::En, Locale::ZhCn] {
            let text = render_construction_history_detail_v1(&history, locale);
            assert!(
                text.contains("[Strike]") && text.contains("[StrikeG]"),
                "{text}"
            );
            assert!(text.contains("+2"), "{text}");
            assert!(text.contains("take:ShrugItOff"), "{text}");
            assert_eq!(serde_json::to_value(&history).unwrap(), original);
        }
        assert_eq!(game.current_state_fingerprints_v1().unwrap(), fingerprint);
    }

    #[test]
    fn controller_event_distinguishes_committed_action_from_owner_advice() {
        let current_path = vec![
            crate::model::RunPlayPathStepV1 {
                archive_node_id: "task_run_archive_node_v1:aaaaaaaaaaaa".to_string(),
                depth: 6,
                site: TaskDecisionSiteV1::Map,
                before: crate::model::RunPlayPublicScalarsV1 {
                    act: Some(3),
                    floor: Some(35),
                    current_hp: 34,
                    max_hp: 39,
                    gold: 286,
                },
                combat_before: None,
                choice: TaskPublicChoiceV1::one(0),
                selected_candidate: TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::Proceed {
                        site: sts_oracle_learning::ai::planner_core::PlannerDecisionSite::Map,
                    },
                },
                semantic_action: Some("proceed".to_string()),
                observed_boundary: RunPlayBoundaryKindV1::Decision,
            },
            crate::model::RunPlayPathStepV1 {
                archive_node_id: "task_run_archive_node_v1:bbbbbbbbbbbb".to_string(),
                depth: 7,
                site: TaskDecisionSiteV1::Map,
                before: crate::model::RunPlayPublicScalarsV1 {
                    act: Some(3),
                    floor: Some(36),
                    current_hp: 34,
                    max_hp: 39,
                    gold: 286,
                },
                combat_before: None,
                choice: TaskPublicChoiceV1::one(0),
                selected_candidate: TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::ChooseRouteNode {
                        x: 6,
                        y: 5,
                        flight: false,
                        question_mark_outcomes: Vec::new(),
                    },
                },
                semantic_action: Some("route:6.5".to_string()),
                observed_boundary: RunPlayBoundaryKindV1::Decision,
            },
            crate::model::RunPlayPathStepV1 {
                archive_node_id: "task_run_archive_node_v1:cccccccccccc".to_string(),
                depth: 8,
                site: TaskDecisionSiteV1::Combat,
                before: crate::model::RunPlayPublicScalarsV1 {
                    act: Some(3),
                    floor: Some(36),
                    current_hp: 34,
                    max_hp: 39,
                    gold: 286,
                },
                combat_before: None,
                choice: TaskPublicChoiceV1::one(0),
                selected_candidate: TaskPublicCandidateV1::CombatAtomic {
                    action: PublicCombatAtomicActionV1::EndTurn,
                },
                semantic_action: Some("end".to_string()),
                observed_boundary: RunPlayBoundaryKindV1::Decision,
            },
        ];

        assert_eq!(
            render_recorded_committed_action_v1(
                &current_path,
                "task_run_archive_node_v1:aaaaaaaaaaaa",
            ),
            " committed=route:6.5"
        );
        assert_eq!(
            render_recorded_committed_action_v1(
                &current_path,
                "task_run_archive_node_v1:dddddddddddd",
            ),
            " committed=<not-on-active-path>"
        );
    }

    #[test]
    fn event_action_renders_a_publicly_identified_gained_card() {
        let rendered = render_strategic_action(
            &TaskStrategicActionV1::ChooseEventOption {
                event: EventId::TheLibrary,
                screen: 1,
                option_index: 8,
                action: EventActionKind::Gain,
                public_semantics: PlannerEventOptionPublicSemanticsV1 {
                    public_label: Some("Take Shrug It Off+.".to_string()),
                    structured_semantics: Some(PlannerEventOptionStructuredSemanticsV1 {
                        effects: vec![PlannerEventPublicEffectV1::ObtainCard {
                            count: 1,
                            pool: PlannerEventCardPoolV1::Specific {
                                card: CardId::ShrugItOff,
                                upgrades: 1,
                            },
                        }],
                        transition: PlannerEventTransitionV1::Complete,
                    }),
                    cards_gained: 1,
                    specific_cards_gained: vec![PlannerOfferedCard {
                        card: CardId::ShrugItOff,
                        upgrades: 1,
                    }],
                    ..PlannerEventOptionPublicSemanticsV1::default()
                },
            },
            None,
            Locale::En,
        );

        assert!(rendered.contains("label=\"Take Shrug It Off+.\""));
        assert!(rendered.contains(
            "mechanics=[Gain 1 permanent card: Shrug It Off+ [ShrugItOff]. Complete this event.]"
        ));
        assert!(rendered.contains("cards+=1"));
        assert!(rendered.contains("gain=[Shrug It Off+]"));
    }

    #[test]
    fn event_action_renders_random_probability_without_promising_a_reward() {
        let rendered = render_strategic_action(
            &TaskStrategicActionV1::ChooseEventOption {
                event: EventId::ScrapOoze,
                screen: 0,
                option_index: 0,
                action: EventActionKind::Special,
                public_semantics: PlannerEventOptionPublicSemanticsV1 {
                    hp_delta: -5,
                    random_outcome: true,
                    random_outcome_chance_percent: Some(25),
                    repeatable: true,
                    ..PlannerEventOptionPublicSemanticsV1::default()
                },
            },
            None,
            Locale::En,
        );

        assert!(rendered.contains("hp=-5"));
        assert!(rendered.contains("random@25%"));
        assert!(rendered.contains("repeatable"));
        assert!(!rendered.contains("relics+="));
    }

    #[test]
    fn boss_relic_detail_renders_benefit_and_debt_evidence() {
        let detail = RunPlayAdviceDetailV1::BossRelic(ExactBossRelicPolicyAuditV1 {
            context: BossRelicPolicyAuditContextV1 {
                energy_master: 3,
                energy_pressure: sts_oracle_learning::eval::run_control::BossRelicEnergyPressureV1::High,
                deck_size: 18,
                deck_access: sts_oracle_learning::ai::strategy::deck_strategic_deficit::StrategicDeficitLevel::Thin,
                energy_or_playability: sts_oracle_learning::ai::strategy::deck_strategic_deficit::StrategicDeficitLevel::Missing,
                deck_burden: sts_oracle_learning::ai::strategy::deck_strategic_deficit::StrategicBurdenLevel::Watch,
                zero_cost_cards: 1,
                low_cost_cards: 8,
                high_cost_cards: 4,
                x_cost_cards: 1,
                self_damage_sources: 2,
                upgrade_candidates: 5,
                important_upgrade_candidates: 2,
                cost_reduction_upgrade_candidates: 1,
                unpaid_core_upgrade_debts: 1,
                upgrade_slot_pressure: "medium".to_string(),
                curse_mitigation_or_synergy: false,
                potion_slots: 2,
                potion_count: 0,
            },
            candidates: vec![BossRelicPolicyAuditCandidateV1 {
                owner_rank: 0,
                candidate_id: "boss-relic:FusionHammer#0".to_string(),
                label: "take FusionHammer".to_string(),
                candidate_key: DecisionCandidateKey::BossRelicPick {
                    option_index: 0,
                    relic: RelicId::FusionHammer,
                },
                action: "Pick".to_string(),
                band: BossRelicPolicyBandV1::ConstrainedEnergy,
                relic: Some(RelicId::FusionHammer),
                energy_gain: 1,
                replaces_existing_relic: false,
                changed_deck_cards: 0,
                followup: Some("ActAdvanced".to_string()),
                generated_relics: Vec::new(),
                strategic_relic_count: 1,
                traits: vec!["ImmediateRecovery".to_string()],
                added_debts: vec!["SmithLock".to_string()],
                unresolved_debt_terms: 2,
                compounding_debt_count: 1,
                startup: BossRelicStartupDeltaV1 {
                    immediate_survival: 1,
                    ..BossRelicStartupDeltaV1::default()
                },
                action_supply: BossRelicActionSupplyDeltaV1 {
                    cost_or_resource_compression_sources: 1,
                    ..BossRelicActionSupplyDeltaV1::default()
                },
                surface_index: 0,
                prior_probability: 0.75,
            }],
        });

        let rendered = render_advice_detail_v1(&detail, Locale::En);
        assert!(rendered.contains("costs=zero:1 low:8 high:4 x:1"));
        assert!(rendered.contains("unpaid_core:1 slot_pressure:medium"));
        assert!(rendered.contains("label=take FusionHammer"));
        assert!(rendered.contains("energy=1"));
        assert!(rendered.contains("debts=[SmithLock]"));
        assert!(rendered.contains("survival:+1"));
        assert!(rendered.contains("compression:+1"));
    }

    #[test]
    fn shop_delta_keeps_capacity_recovery_and_action_supply_distinct() {
        let rendered = render_shop_state_delta_v1(
            7,
            7,
            0,
            5,
            5,
            8,
            0,
            BossRelicActionSupplyDeltaV1 {
                triggered_repeatable_sources: 1,
                cost_or_resource_compression_sources: 1,
                ..BossRelicActionSupplyDeltaV1::default()
            },
        );

        assert!(rendered.contains("hp:7 max_hp:7 recovered_missing_hp:0"));
        assert!(rendered.contains("guaranteed_realizable_hp:5 guaranteed_realizable_max_hp:5"));
        assert!(rendered.contains("conditional_combat_on_use_hp_capacity:8"));
        assert!(rendered.contains("repeatable:+1"));
        assert!(rendered.contains("compression:+1"));
    }

    fn strategic_reward_observation(
        pending_rewards: Vec<PlannerRewardDescriptor>,
        potions: Vec<TaskStrategicPotionSlotV1>,
    ) -> TaskPublicObservationV1 {
        TaskPublicObservationV1::Strategic {
            observation: TaskStrategicObservationV1 {
                mechanics: sts_oracle_learning::ai::planner_core::PlannerMechanicsManifest {
                    mechanics_id: "render-reward-hint-fixture".into(),
                    mechanics_version: 1,
                },
                run_goal: sts_oracle_learning::ai::planner_core::PlannerRunGoal::HeartVictory,
                decision_site: sts_oracle_learning::ai::planner_core::PlannerDecisionSite::Reward,
                run: sts_oracle_learning::ai::planner_core::PlannerRunScalars {
                    player_class:
                        sts_oracle_learning::ai::planner_core::PlannerPlayerClass::Ironclad,
                    ascension_level: 20,
                    act: 1,
                    floor: 9,
                    current_hp: 40,
                    max_hp: 75,
                    gold: 100,
                    keys: [false; 3],
                    potion_capacity: 2,
                    pending_act_entry: None,
                },
                cards: vec![],
                relics: vec![],
                potions,
                public_map: PlannerPublicMap {
                    current_x: 0,
                    current_y: 0,
                    boss: None,
                    nodes: vec![],
                },
                context: sts_oracle_learning::ai::planner_core::PlannerDecisionContext::Reward {
                    pending_rewards,
                },
                public_history: sts_oracle_learning::ai::planner_core::PlannerPublicHistory {
                    shop_purge_count: 0,
                    recent_combat_hp_loss: None,
                    recent_combat_turns: None,
                    recent_combat_potions_used: None,
                },
            },
        }
    }

    fn claim_reward_candidate(
        ordinal: usize,
        reward_item_index: usize,
        reward: PlannerRewardDescriptor,
    ) -> sts_oracle_learning::eval::run_control::TaskRunBranchCandidateInventoryV1 {
        sts_oracle_learning::eval::run_control::TaskRunBranchCandidateInventoryV1 {
            candidate_ordinal: ordinal,
            candidate: TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ClaimReward {
                    reward_item_index,
                    reward,
                },
            },
            observed_choices: vec![],
            direct_choice: None,
            direct_choice_observed: false,
        }
    }

    fn occupied_potion_slots() -> Vec<TaskStrategicPotionSlotV1> {
        (0..2)
            .map(|slot| TaskStrategicPotionSlotV1 {
                slot,
                potion: Some(TaskStrategicPotionV1 {
                    potion: sts_oracle_learning::content::potions::PotionId::FirePotion,
                    can_use: false,
                    can_discard: true,
                    requires_target: true,
                }),
            })
            .collect()
    }

    #[test]
    fn reward_labels_identify_the_sapphire_link_with_multiple_relics() {
        for with_gold in [false, true] {
            let mut rewards = vec![
                PlannerRewardDescriptor::Relic {
                    relic: RelicId::ArtOfWar,
                },
                PlannerRewardDescriptor::Relic {
                    relic: RelicId::BloodVial,
                },
                PlannerRewardDescriptor::SapphireKey,
            ];
            if with_gold {
                rewards.insert(1, PlannerRewardDescriptor::Gold { amount: 50 });
            }
            let observation = strategic_reward_observation(rewards.clone(), vec![]);
            let labels: Vec<_> = rewards
                .into_iter()
                .enumerate()
                .map(|(index, reward)| {
                    render_candidate_with_observation(
                        &claim_reward_candidate(index, index, reward).candidate,
                        Some(&observation),
                        Locale::En,
                    )
                })
                .collect();
            let linked_index = if with_gold { 2 } else { 1 };
            assert!(!labels[0].contains("alternative"));
            assert!(labels[linked_index].contains("alternative to Sapphire Key"));
            assert!(labels[linked_index + 1].contains(&format!(
                "alternative to relic Blood Vial reward[{linked_index}]"
            )));
        }
    }

    #[test]
    fn reward_hints_flag_blocked_potion_and_chest_exclusivity() {
        let observation = strategic_reward_observation(
            vec![
                PlannerRewardDescriptor::Relic {
                    relic: RelicId::Shovel,
                },
                PlannerRewardDescriptor::SapphireKey,
                PlannerRewardDescriptor::Potion {
                    potion: sts_oracle_learning::content::potions::PotionId::FirePotion,
                },
            ],
            occupied_potion_slots(),
        );
        let candidates = vec![
            claim_reward_candidate(
                0,
                0,
                PlannerRewardDescriptor::Relic {
                    relic: RelicId::Shovel,
                },
            ),
            claim_reward_candidate(1, 1, PlannerRewardDescriptor::SapphireKey),
        ];
        let mut output = String::new();
        render_reward_display_hints_v1(&mut output, &observation, &candidates);
        assert!(output.contains("[2] potion"));
        assert!(output.contains("blocked:no_empty_potion_slot"));
        assert!(output.contains("discard a held potion first"));
        assert!(output.contains(
            "reward_exclusivity=chest sapphire key and its linked relic are mutually exclusive"
        ));
    }

    #[test]
    fn reward_hints_stay_silent_when_every_pending_item_is_claimable() {
        let observation = strategic_reward_observation(
            vec![PlannerRewardDescriptor::Gold { amount: 20 }],
            occupied_potion_slots(),
        );
        let candidates = vec![claim_reward_candidate(
            0,
            0,
            PlannerRewardDescriptor::Gold { amount: 20 },
        )];
        let mut output = String::new();
        render_reward_display_hints_v1(&mut output, &observation, &candidates);
        assert!(output.is_empty());
    }
}
