use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::ai::deck_mutation_compiler_v1::{
    deck_removal_target_snapshots_v1, DeckMutationTargetLossTierV1,
};
use crate::ai::deck_repair_profile_v1::{
    deck_repair_profile_from_upgrade_plan_v1, DeckRepairUpgradePriorityV1,
};
use crate::ai::route_window_facts::{
    build_route_window_facts, RouteWindowFactsConfig, RouteWindowModality, RouteWindowPredicate,
    RouteWindowSubject,
};
use crate::ai::strategy::campfire_upgrade_quality::rank_campfire_upgrades;
use crate::ai::upgrade_planner_v1::{
    plan_upgrades_v1, UpgradeDebtKindV1, UpgradeDebtSeverityV1, UpgradeRoleV1, UpgradeVerdictV1,
};
use crate::content::cards::CardId;
use crate::content::relics::RelicId;
use crate::engine::campfire_handler::apply_campfire_rest_healing;
use crate::runtime::combat::CardLineageId;

use super::{
    exact_run_policy_decision_v1, owned_run_policy_actions_v1, positive_ranked_run_policy_prior_v1,
    run_policy_state_delta_v1, DecisionCandidateKey, ExactRunPolicyActionSuccessorV1,
    ExactRunPolicyDecisionV1, RunControlSession, RunPolicyCandidateV1, RunPolicyPriorV1,
    RunPolicyStateDeltaV1,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CampfirePolicyBandV1 {
    HeartKeyDeadline,
    ImmediateSurvival,
    PayRequiredUpgradeDebt,
    PrepareBossSurvival,
    RepairDeck,
    ImproveReliability,
    PreserveSurvival,
    PersistentGrowth,
    KeyProgress,
    Speculative,
    MarginalRecovery,
    Liability,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CampfirePolicyActionV1 {
    Rest {
        hp_gain: i32,
    },
    Smith {
        deck_index: usize,
        card_lineage: CardLineageId,
        card: CardId,
        upgrades_before: u8,
        repair_priority: Option<DeckRepairUpgradePriorityV1>,
        urgency: Option<UpgradeDebtSeverityV1>,
        verdict: Option<UpgradeVerdictV1>,
        roles: Vec<UpgradeRoleV1>,
        pays_debts: Vec<UpgradeDebtKindV1>,
    },
    Dig {
        gained_relic: Option<RelicId>,
    },
    Lift {
        girya_counter_gain: i32,
    },
    Toke {
        deck_index: usize,
        card_lineage: CardLineageId,
        card: CardId,
        upgrades: u8,
        target_loss: Option<DeckMutationTargetLossTierV1>,
        shared_repair_target: bool,
    },
    Recall {
        ruby_key_gained: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CampfirePolicyActionEvidenceV1 {
    pub candidate_id: String,
    pub candidate_key: DecisionCandidateKey,
    pub action: CampfirePolicyActionV1,
    pub band: CampfirePolicyBandV1,
    pub delta: RunPolicyStateDeltaV1,
    mechanical_upgrade_rank: Option<usize>,
    mechanical_upgrade_label: Option<String>,
    surface_index: usize,
}

#[derive(Clone, Debug)]
pub struct ExactCampfirePolicyDecisionV1 {
    pub exact: ExactRunPolicyDecisionV1,
    pub recovery: CampfireRecoveryContextV1,
    pub evidence: Vec<CampfirePolicyActionEvidenceV1>,
    pub prior: RunPolicyPriorV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireRecoveryContextV1 {
    pub boss_nodes_remaining: Option<usize>,
    pub rest_heal_capacity: i32,
    pub known_combat_before_next_campfire: RouteWindowModality,
    pub known_elite_before_next_campfire: RouteWindowModality,
}

struct CampfirePolicyContextV1 {
    recovery: CampfireRecoveryContextV1,
    repair_upgrades: BTreeMap<(usize, CardLineageId), DeckRepairUpgradePriorityV1>,
    upgrade_evidence: BTreeMap<(usize, CardLineageId), CampfireSmithEvidenceV1>,
    target_losses: BTreeMap<(usize, CardLineageId), DeckMutationTargetLossTierV1>,
    low_loss_removals: BTreeSet<(usize, CardLineageId)>,
}

struct CampfireSmithEvidenceV1 {
    urgency: UpgradeDebtSeverityV1,
    verdict: UpgradeVerdictV1,
    roles: Vec<UpgradeRoleV1>,
    pays_debts: Vec<UpgradeDebtKindV1>,
    mechanical_rank: Option<usize>,
    mechanical_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfirePolicyAuditCandidateV1 {
    pub owner_rank: usize,
    pub candidate_id: String,
    pub label: String,
    pub candidate_key: DecisionCandidateKey,
    pub action: String,
    pub band: CampfirePolicyBandV1,
    pub hp_gain: i32,
    pub max_hp_gain: i32,
    pub gold_delta: i32,
    pub deck_size_delta: isize,
    pub closed_threat_gaps: Vec<String>,
    pub capability_improvements: Vec<String>,
    pub mechanical_upgrade_rank: Option<usize>,
    pub mechanical_upgrade_label: Option<String>,
    pub surface_index: usize,
    pub prior_probability: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactCampfirePolicyAuditV1 {
    pub current_hp: i32,
    pub max_hp: i32,
    pub recovery: CampfireRecoveryContextV1,
    pub candidates: Vec<CampfirePolicyAuditCandidateV1>,
}

impl ExactCampfirePolicyDecisionV1 {
    pub fn audit(
        &self,
        legal: &[RunPolicyCandidateV1<'_>],
    ) -> Result<ExactCampfirePolicyAuditV1, String> {
        let candidates = self
            .evidence
            .iter()
            .enumerate()
            .map(|(owner_rank, evidence)| {
                let legal_candidate = legal
                    .iter()
                    .find(|candidate| candidate.candidate_id == evidence.candidate_id)
                    .ok_or_else(|| {
                        format!(
                            "campfire policy audit could not find legal candidate '{}'",
                            evidence.candidate_id
                        )
                    })?;
                let prior_probability = self
                    .prior
                    .entries
                    .iter()
                    .find(|entry| entry.candidate_id == evidence.candidate_id)
                    .map(|entry| entry.probability)
                    .ok_or_else(|| {
                        format!(
                            "campfire policy audit could not find prior for candidate '{}'",
                            evidence.candidate_id
                        )
                    })?;
                Ok(CampfirePolicyAuditCandidateV1 {
                    owner_rank,
                    candidate_id: evidence.candidate_id.clone(),
                    label: legal_candidate.label.to_string(),
                    candidate_key: evidence.candidate_key.clone(),
                    action: format!("{:?}", evidence.action),
                    band: evidence.band,
                    hp_gain: evidence.delta.hp_gain,
                    max_hp_gain: evidence.delta.max_hp_gain,
                    gold_delta: evidence.delta.gold_delta,
                    deck_size_delta: evidence.delta.deck_size_delta,
                    closed_threat_gaps: evidence
                        .delta
                        .closed_threat_gaps
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    capability_improvements: evidence
                        .delta
                        .capability_improvements
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    mechanical_upgrade_rank: evidence.mechanical_upgrade_rank,
                    mechanical_upgrade_label: evidence.mechanical_upgrade_label.clone(),
                    surface_index: evidence.surface_index,
                    prior_probability,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ExactCampfirePolicyAuditV1 {
            current_hp: self.exact.before.resources.current_hp,
            max_hp: self.exact.before.resources.max_hp,
            recovery: self.recovery,
            candidates,
        })
    }
}

pub fn exact_campfire_policy_audit_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactCampfirePolicyAuditV1, String> {
    exact_campfire_policy_decision_v1(session, legal)?.audit(legal)
}

pub fn exact_campfire_policy_prior_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<RunPolicyPriorV1, String> {
    Ok(exact_campfire_policy_decision_v1(session, legal)?.prior)
}

pub fn exact_campfire_policy_decision_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactCampfirePolicyDecisionV1, String> {
    let exact = exact_run_policy_decision_v1(session)?;
    let owned = owned_run_policy_actions_v1(&exact, legal, "campfire", is_campfire_key)?;
    let context = campfire_policy_context_v1(session);
    let mut evidence = owned
        .into_iter()
        .map(|(surface_index, action)| {
            campfire_action_evidence_v1(session, &exact, action, surface_index, &context)
        })
        .collect::<Result<Vec<_>, _>>()?;
    evidence.sort_by(compare_campfire_evidence);

    let ranked_ids = evidence
        .iter()
        .map(|candidate| candidate.candidate_id.clone())
        .collect::<BTreeSet<_>>();
    let prior = positive_ranked_run_policy_prior_v1(
        legal,
        evidence
            .iter()
            .map(|candidate| candidate.candidate_id.clone())
            .chain(
                legal
                    .iter()
                    .filter(|candidate| !ranked_ids.contains(candidate.candidate_id))
                    .map(|candidate| candidate.candidate_id.to_string()),
            ),
    )?;

    Ok(ExactCampfirePolicyDecisionV1 {
        exact,
        recovery: context.recovery,
        evidence,
        prior,
    })
}

fn campfire_policy_context_v1(parent: &RunControlSession) -> CampfirePolicyContextV1 {
    let upgrade_plan = plan_upgrades_v1(&parent.run_state);
    let mechanical_upgrades = rank_campfire_upgrades(
        &parent.run_state.master_deck,
        parent
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == crate::content::relics::RelicId::SneckoEye),
    )
    .into_iter()
    .enumerate()
    .filter_map(|(rank, quality)| {
        let card = parent.run_state.master_deck.get(quality.deck_index)?;
        Some((
            (quality.deck_index, card.lineage),
            (rank, quality.compact_label()),
        ))
    })
    .collect::<BTreeMap<_, _>>();
    let repair = deck_repair_profile_from_upgrade_plan_v1(&parent.run_state, &upgrade_plan);
    let repair_upgrades = repair
        .reliability_upgrades
        .iter()
        .map(|candidate| {
            (
                (candidate.deck_index, candidate.lineage),
                candidate.priority,
            )
        })
        .collect();
    let low_loss_removals = repair
        .low_loss_removals
        .iter()
        .map(|candidate| (candidate.deck_index, candidate.lineage))
        .collect();
    let upgrade_evidence = upgrade_plan
        .candidates
        .into_iter()
        .map(|candidate| {
            let mechanical =
                mechanical_upgrades.get(&(candidate.deck_index, candidate.card_lineage));
            (
                (candidate.deck_index, candidate.card_lineage),
                CampfireSmithEvidenceV1 {
                    urgency: candidate.urgency,
                    verdict: candidate.verdict,
                    roles: candidate.roles,
                    pays_debts: candidate.pays_debts,
                    mechanical_rank: mechanical.map(|(rank, _)| *rank),
                    mechanical_label: mechanical.map(|(_, label)| label.clone()),
                },
            )
        })
        .collect();
    let target_losses = deck_removal_target_snapshots_v1(&parent.run_state)
        .into_iter()
        .map(|snapshot| {
            (
                (snapshot.deck_index, snapshot.lineage),
                snapshot.target_loss.tier,
            )
        })
        .collect();
    CampfirePolicyContextV1 {
        recovery: campfire_recovery_context_v1(parent),
        repair_upgrades,
        upgrade_evidence,
        target_losses,
        low_loss_removals,
    }
}

fn campfire_recovery_context_v1(parent: &RunControlSession) -> CampfireRecoveryContextV1 {
    let mut empty_hp = parent.run_state.clone();
    empty_hp.current_hp = 0;
    apply_campfire_rest_healing(&mut empty_hp);
    let boss_nodes_remaining = parent
        .run_state
        .boss_key
        .is_some()
        .then_some(parent.run_state.map.current_y)
        .filter(|current_y| (0..=14).contains(current_y))
        .map(|current_y| 15_usize.saturating_sub(current_y as usize));
    let map_cursor_available = parent.run_state.map.current_y == -1
        || (parent.run_state.map.current_y >= 0
            && parent.run_state.map.current_x >= 0
            && parent
                .run_state
                .map
                .graph
                .get(parent.run_state.map.current_y as usize)
                .and_then(|row| row.get(parent.run_state.map.current_x as usize))
                .is_some());
    let route_facts = map_cursor_available.then(|| {
        build_route_window_facts(
            &parent.run_state,
            RouteWindowFactsConfig {
                horizon_nodes: boss_nodes_remaining.unwrap_or(5).max(1),
                ..RouteWindowFactsConfig::default()
            },
        )
    });
    let occurs_before_campfire = |subject| {
        route_facts
            .as_ref()
            .and_then(|route_facts| {
                route_facts
                    .facts
                    .iter()
                    .find(|fact| {
                        fact.predicate
                            == (RouteWindowPredicate::OccursBefore {
                                subject,
                                before: RouteWindowSubject::Campfire,
                            })
                    })
                    .map(|fact| fact.modality)
            })
            .unwrap_or(RouteWindowModality::Unknown)
    };
    CampfireRecoveryContextV1 {
        boss_nodes_remaining,
        rest_heal_capacity: empty_hp.current_hp.max(0),
        known_combat_before_next_campfire: occurs_before_campfire(RouteWindowSubject::KnownCombat),
        known_elite_before_next_campfire: occurs_before_campfire(RouteWindowSubject::Elite),
    }
}

fn is_campfire_key(key: &DecisionCandidateKey) -> bool {
    matches!(
        key,
        DecisionCandidateKey::CampfireRest
            | DecisionCandidateKey::CampfireSmith { .. }
            | DecisionCandidateKey::CampfireDig
            | DecisionCandidateKey::CampfireLift
            | DecisionCandidateKey::CampfireToke { .. }
            | DecisionCandidateKey::CampfireRecall
    )
}

fn campfire_action_evidence_v1(
    parent: &RunControlSession,
    decision: &ExactRunPolicyDecisionV1,
    successor: &ExactRunPolicyActionSuccessorV1,
    surface_index: usize,
    context: &CampfirePolicyContextV1,
) -> Result<CampfirePolicyActionEvidenceV1, String> {
    let candidate_key = successor.candidate_key.clone().ok_or_else(|| {
        format!(
            "campfire candidate '{}' has no typed key",
            successor.candidate_id
        )
    })?;
    let delta = run_policy_state_delta_v1(&decision.before, &successor.after);
    let action = campfire_action_v1(parent, successor, &candidate_key, &delta, context)?;
    let band = campfire_policy_band_v1(parent, &action, context.recovery);
    let mechanical = match &action {
        CampfirePolicyActionV1::Smith {
            deck_index,
            card_lineage,
            ..
        } => context.upgrade_evidence.get(&(*deck_index, *card_lineage)),
        _ => None,
    };

    Ok(CampfirePolicyActionEvidenceV1 {
        candidate_id: successor.candidate_id.clone(),
        candidate_key,
        action,
        band,
        delta,
        mechanical_upgrade_rank: mechanical.and_then(|upgrade| upgrade.mechanical_rank),
        mechanical_upgrade_label: mechanical.and_then(|upgrade| upgrade.mechanical_label.clone()),
        surface_index,
    })
}

fn campfire_action_v1(
    parent: &RunControlSession,
    successor: &ExactRunPolicyActionSuccessorV1,
    key: &DecisionCandidateKey,
    delta: &RunPolicyStateDeltaV1,
    context: &CampfirePolicyContextV1,
) -> Result<CampfirePolicyActionV1, String> {
    Ok(match key {
        DecisionCandidateKey::CampfireRest => CampfirePolicyActionV1::Rest {
            hp_gain: delta.hp_gain,
        },
        DecisionCandidateKey::CampfireSmith {
            deck_index,
            card_lineage,
            card,
            upgrades,
        } => {
            let repair_priority = context
                .repair_upgrades
                .get(&(*deck_index, *card_lineage))
                .copied();
            let upgrade = context.upgrade_evidence.get(&(*deck_index, *card_lineage));
            CampfirePolicyActionV1::Smith {
                deck_index: *deck_index,
                card_lineage: *card_lineage,
                card: *card,
                upgrades_before: *upgrades,
                repair_priority,
                urgency: upgrade.map(|candidate| candidate.urgency),
                verdict: upgrade.map(|candidate| candidate.verdict),
                roles: upgrade
                    .map(|candidate| candidate.roles.clone())
                    .unwrap_or_default(),
                pays_debts: upgrade
                    .map(|candidate| candidate.pays_debts.clone())
                    .unwrap_or_default(),
            }
        }
        DecisionCandidateKey::CampfireDig => {
            let before = parent
                .run_state
                .relics
                .iter()
                .map(|relic| relic.id)
                .collect::<Vec<_>>();
            let gained_relic = successor
                .exact
                .session
                .run_state
                .relics
                .iter()
                .map(|relic| relic.id)
                .find(|relic| !before.contains(relic));
            CampfirePolicyActionV1::Dig { gained_relic }
        }
        DecisionCandidateKey::CampfireLift => {
            let before = relic_counter(parent, RelicId::Girya);
            let after = relic_counter(&successor.exact.session, RelicId::Girya);
            CampfirePolicyActionV1::Lift {
                girya_counter_gain: after.saturating_sub(before),
            }
        }
        DecisionCandidateKey::CampfireToke {
            deck_index,
            card_lineage,
            card,
            upgrades,
        } => {
            let target_loss = context
                .target_losses
                .get(&(*deck_index, *card_lineage))
                .copied();
            let shared_repair_target = context
                .low_loss_removals
                .contains(&(*deck_index, *card_lineage));
            CampfirePolicyActionV1::Toke {
                deck_index: *deck_index,
                card_lineage: *card_lineage,
                card: *card,
                upgrades: *upgrades,
                target_loss,
                shared_repair_target,
            }
        }
        DecisionCandidateKey::CampfireRecall => CampfirePolicyActionV1::Recall {
            ruby_key_gained: !parent.run_state.keys[0] && successor.exact.session.run_state.keys[0],
        },
        other => {
            return Err(format!(
                "exact campfire policy received non-campfire candidate key {other:?}"
            ))
        }
    })
}

fn relic_counter(session: &RunControlSession, relic_id: RelicId) -> i32 {
    session
        .run_state
        .relics
        .iter()
        .find(|relic| relic.id == relic_id)
        .map(|relic| relic.counter)
        .unwrap_or(0)
}

fn campfire_policy_band_v1(
    parent: &RunControlSession,
    action: &CampfirePolicyActionV1,
    recovery: CampfireRecoveryContextV1,
) -> CampfirePolicyBandV1 {
    match action {
        CampfirePolicyActionV1::Recall {
            ruby_key_gained: true,
        } if parent.run_state.is_final_act_available
            && parent.run_state.act_num == 3
            && !parent.run_state.keys[0]
            && recovery
                .boss_nodes_remaining
                .is_some_and(|nodes| nodes <= 1) =>
        {
            CampfirePolicyBandV1::HeartKeyDeadline
        }
        CampfirePolicyActionV1::Rest { hp_gain } if *hp_gain <= 0 => {
            CampfirePolicyBandV1::Liability
        }
        CampfirePolicyActionV1::Rest { hp_gain } if parent.run_state.current_hp <= *hp_gain => {
            CampfirePolicyBandV1::ImmediateSurvival
        }
        CampfirePolicyActionV1::Rest { hp_gain }
            if recovery.known_elite_before_next_campfire == RouteWindowModality::Must
                && campfire_rest_is_efficient_recovery_v1(
                    *hp_gain,
                    recovery.rest_heal_capacity,
                ) =>
        {
            CampfirePolicyBandV1::ImmediateSurvival
        }
        CampfirePolicyActionV1::Rest { hp_gain }
            if (recovery
                .boss_nodes_remaining
                .is_some_and(|nodes| nodes <= 3)
                || recovery.known_combat_before_next_campfire == RouteWindowModality::Must)
                && campfire_rest_is_efficient_recovery_v1(
                    *hp_gain,
                    recovery.rest_heal_capacity,
                ) =>
        {
            CampfirePolicyBandV1::PrepareBossSurvival
        }
        CampfirePolicyActionV1::Rest { hp_gain }
            if recovery.rest_heal_capacity > 0
                && !campfire_rest_is_efficient_recovery_v1(
                    *hp_gain,
                    recovery.rest_heal_capacity,
                ) =>
        {
            CampfirePolicyBandV1::MarginalRecovery
        }
        CampfirePolicyActionV1::Rest { .. } => CampfirePolicyBandV1::PreserveSurvival,
        CampfirePolicyActionV1::Smith {
            repair_priority: Some(DeckRepairUpgradePriorityV1::NeededFunction),
            ..
        }
        | CampfirePolicyActionV1::Smith {
            urgency:
                Some(
                    UpgradeDebtSeverityV1::ImportantBeforeBoss
                    | UpgradeDebtSeverityV1::CriticalBeforeBoss,
                ),
            ..
        }
        | CampfirePolicyActionV1::Smith {
            verdict: Some(UpgradeVerdictV1::CoreDebtPayment | UpgradeVerdictV1::Important),
            ..
        } => CampfirePolicyBandV1::PayRequiredUpgradeDebt,
        CampfirePolicyActionV1::Smith {
            repair_priority: Some(DeckRepairUpgradePriorityV1::Reliability),
            ..
        }
        | CampfirePolicyActionV1::Smith {
            verdict: Some(UpgradeVerdictV1::Useful),
            ..
        } => CampfirePolicyBandV1::ImproveReliability,
        CampfirePolicyActionV1::Smith {
            verdict: Some(UpgradeVerdictV1::Avoid),
            ..
        } => CampfirePolicyBandV1::Liability,
        CampfirePolicyActionV1::Smith { .. } => CampfirePolicyBandV1::Speculative,
        CampfirePolicyActionV1::Toke {
            target_loss: Some(DeckMutationTargetLossTierV1::LowValue),
            ..
        }
        | CampfirePolicyActionV1::Toke {
            shared_repair_target: true,
            ..
        } => CampfirePolicyBandV1::RepairDeck,
        CampfirePolicyActionV1::Toke {
            target_loss:
                Some(
                    DeckMutationTargetLossTierV1::CoreFunctional
                    | DeckMutationTargetLossTierV1::Unsupported,
                ),
            ..
        } => CampfirePolicyBandV1::Liability,
        CampfirePolicyActionV1::Toke { .. } => CampfirePolicyBandV1::Speculative,
        CampfirePolicyActionV1::Dig { gained_relic } if gained_relic.is_some() => {
            CampfirePolicyBandV1::PersistentGrowth
        }
        CampfirePolicyActionV1::Lift { girya_counter_gain } if *girya_counter_gain > 0 => {
            CampfirePolicyBandV1::PersistentGrowth
        }
        CampfirePolicyActionV1::Recall {
            ruby_key_gained: true,
        } => CampfirePolicyBandV1::KeyProgress,
        CampfirePolicyActionV1::Dig { .. }
        | CampfirePolicyActionV1::Lift { .. }
        | CampfirePolicyActionV1::Recall { .. } => CampfirePolicyBandV1::Speculative,
    }
}

/// Whether a Rest realizes enough of its available healing capacity to be a
/// meaningful recovery action rather than marginal top-up healing.
///
/// This is a public run-policy fact so combat-plan selection cannot promise a
/// Rest that the campfire owner would independently reject as marginal.
pub fn campfire_rest_is_efficient_recovery_v1(hp_gain: i32, rest_heal_capacity: i32) -> bool {
    hp_gain > 0
        && rest_heal_capacity > 0
        && hp_gain.saturating_mul(4) >= rest_heal_capacity.saturating_mul(3)
}

fn compare_campfire_evidence(
    left: &CampfirePolicyActionEvidenceV1,
    right: &CampfirePolicyActionEvidenceV1,
) -> Ordering {
    left.band
        .cmp(&right.band)
        .then_with(|| {
            compare_action_evidence(
                &left.action,
                left.mechanical_upgrade_rank,
                &right.action,
                right.mechanical_upgrade_rank,
            )
        })
        .then_with(|| right.delta.hp_gain.cmp(&left.delta.hp_gain))
        .then_with(|| {
            right
                .delta
                .closed_threat_gaps
                .len()
                .cmp(&left.delta.closed_threat_gaps.len())
        })
        .then_with(|| {
            right
                .delta
                .capability_improvements
                .len()
                .cmp(&left.delta.capability_improvements.len())
        })
        .then_with(|| left.surface_index.cmp(&right.surface_index))
}

fn compare_action_evidence(
    left: &CampfirePolicyActionV1,
    left_mechanical_rank: Option<usize>,
    right: &CampfirePolicyActionV1,
    right_mechanical_rank: Option<usize>,
) -> Ordering {
    match (left, right) {
        (
            CampfirePolicyActionV1::Smith {
                repair_priority: left_repair,
                urgency: left_urgency,
                verdict: left_verdict,
                pays_debts: left_debts,
                ..
            },
            CampfirePolicyActionV1::Smith {
                repair_priority: right_repair,
                urgency: right_urgency,
                verdict: right_verdict,
                pays_debts: right_debts,
                ..
            },
        ) => repair_priority_rank(*left_repair)
            .cmp(&repair_priority_rank(*right_repair))
            .then_with(|| right_urgency.cmp(left_urgency))
            .then_with(|| {
                upgrade_verdict_rank(*right_verdict).cmp(&upgrade_verdict_rank(*left_verdict))
            })
            .then_with(|| {
                left_mechanical_rank
                    .unwrap_or(usize::MAX)
                    .cmp(&right_mechanical_rank.unwrap_or(usize::MAX))
            })
            .then_with(|| right_debts.len().cmp(&left_debts.len())),
        (
            CampfirePolicyActionV1::Toke {
                target_loss: left_loss,
                ..
            },
            CampfirePolicyActionV1::Toke {
                target_loss: right_loss,
                ..
            },
        ) => left_loss.cmp(right_loss),
        _ => Ordering::Equal,
    }
}

fn repair_priority_rank(priority: Option<DeckRepairUpgradePriorityV1>) -> u8 {
    match priority {
        Some(DeckRepairUpgradePriorityV1::NeededFunction) => 0,
        Some(DeckRepairUpgradePriorityV1::Reliability) => 1,
        None => 2,
    }
}

fn upgrade_verdict_rank(verdict: Option<UpgradeVerdictV1>) -> u8 {
    match verdict {
        None => 0,
        Some(UpgradeVerdictV1::Avoid) => 1,
        Some(UpgradeVerdictV1::Defer) => 2,
        Some(UpgradeVerdictV1::Opportunistic) => 3,
        Some(UpgradeVerdictV1::Useful) => 4,
        Some(UpgradeVerdictV1::Important) => 5,
        Some(UpgradeVerdictV1::CoreDebtPayment) => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::factory::EncounterId;
    use crate::content::relics::RelicState;
    use crate::eval::run_control::{build_decision_surface, RunControlConfig};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::EngineState;
    use crate::state::map::node::{MapEdge, MapRoomNode, RoomType};
    use crate::state::map::state::MapState;

    fn policy_candidates<'a>(
        surface: &'a super::super::DecisionSurface,
    ) -> Vec<RunPolicyCandidateV1<'a>> {
        surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: &candidate.id,
                        label: &candidate.label,
                        action,
                    })
            })
            .collect()
    }

    fn campfire_session(cards: &[CardId]) -> RunControlSession {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::Campfire;
        session.run_state.master_deck = cards
            .iter()
            .enumerate()
            .map(|(index, card)| CombatCard::for_scenario(*card, 10_000 + index as u32))
            .collect();
        session
    }

    fn map_node(x: i32, y: i32, room_type: RoomType) -> MapRoomNode {
        let mut node = MapRoomNode::new(x, y);
        node.class = Some(room_type);
        node
    }

    fn set_linear_map(session: &mut RunControlSession, rooms: &[RoomType]) {
        let mut graph = rooms
            .iter()
            .enumerate()
            .map(|(y, room_type)| vec![map_node(0, y as i32, *room_type)])
            .collect::<Vec<_>>();
        for y in 0..graph.len().saturating_sub(1) {
            graph[y][0]
                .edges
                .insert(MapEdge::new(0, y as i32, 0, y as i32 + 1));
        }
        session.run_state.map = MapState::new(graph);
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 0;
    }

    fn set_pre_boss_map_position(session: &mut RunControlSession) {
        let current_y = 14usize;
        assert!(
            session
                .run_state
                .map
                .graph
                .get(current_y)
                .is_some_and(|row| !row.is_empty()),
            "campfire fixture requires a populated final map row",
        );
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = current_y as i32;
    }

    #[test]
    fn exact_campfire_surface_has_typed_positive_support_for_every_action() {
        let mut session = campfire_session(&[CardId::Strike, CardId::Defend]);
        session.run_state.is_final_act_available = true;
        session.run_state.relics = [RelicId::Shovel, RelicId::Girya, RelicId::PeacePipe]
            .into_iter()
            .map(RelicState::new)
            .collect();
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_campfire_policy_decision_v1(&session, &legal).expect("exact campfire decision");

        assert_eq!(decision.evidence.len(), legal.len());
        assert_eq!(decision.prior.entries.len(), legal.len());
        assert!(decision
            .prior
            .entries
            .iter()
            .all(|entry| entry.probability.is_finite() && entry.probability > 0.0));
        assert!(decision
            .evidence
            .iter()
            .all(|entry| is_campfire_key(&entry.candidate_key)));
    }

    #[test]
    fn campfire_owner_partitions_auxiliary_potion_actions_from_its_typed_surface() {
        use crate::content::potions::{Potion, PotionId};

        let mut session = campfire_session(&[CardId::Strike, CardId::Defend]);
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::AttackPotion, 1)),
            Some(Potion::new(PotionId::SpeedPotion, 2)),
            Some(Potion::new(PotionId::EssenceOfSteel, 3)),
        ];
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_campfire_policy_decision_v1(&session, &legal).expect("mixed campfire decision");

        assert!(legal.len() > decision.evidence.len());
        decision
            .prior
            .validate_for(&legal)
            .expect("auxiliary actions keep positive support");
        assert!(decision
            .evidence
            .iter()
            .all(|entry| is_campfire_key(&entry.candidate_key)));
    }

    #[test]
    fn low_hp_rest_is_urgent_but_full_hp_rest_is_not_universally_preferred() {
        let mut low_hp = campfire_session(&[CardId::Apparition, CardId::Cleave]);
        low_hp.run_state.current_hp = 1;
        low_hp.run_state.max_hp = 80;
        let surface = build_decision_surface(&low_hp);
        let legal = policy_candidates(&surface);
        let decision = exact_campfire_policy_decision_v1(&low_hp, &legal).expect("low hp campfire");
        assert_eq!(decision.prior.entries[0].candidate_id, "rest");

        let mut full_hp = low_hp.clone();
        full_hp.run_state.current_hp = full_hp.run_state.max_hp;
        let surface = build_decision_surface(&full_hp);
        let legal = policy_candidates(&surface);
        let decision =
            exact_campfire_policy_decision_v1(&full_hp, &legal).expect("full hp campfire");
        assert_ne!(decision.prior.entries[0].candidate_id, "rest");
    }

    #[test]
    fn efficient_pre_boss_rest_precedes_useful_upgrade_but_wasteful_rest_does_not() {
        let mut needs_recovery = campfire_session(&[CardId::Whirlwind, CardId::SecondWind]);
        needs_recovery.run_state.current_hp = 56;
        needs_recovery.run_state.max_hp = 88;
        needs_recovery.run_state.boss_key = Some(EncounterId::TimeEater);
        set_pre_boss_map_position(&mut needs_recovery);
        let surface = build_decision_surface(&needs_recovery);
        let legal = policy_candidates(&surface);
        let decision = exact_campfire_policy_decision_v1(&needs_recovery, &legal)
            .expect("pre-boss recovery decision");
        assert_eq!(decision.recovery.boss_nodes_remaining, Some(1));
        assert_eq!(decision.recovery.rest_heal_capacity, 26);
        assert_eq!(decision.prior.entries[0].candidate_id, "rest");
        assert_eq!(
            decision.evidence[0].band,
            CampfirePolicyBandV1::PrepareBossSurvival
        );

        let mut healthy = needs_recovery;
        healthy.run_state.current_hp = 70;
        let surface = build_decision_surface(&healthy);
        let legal = policy_candidates(&surface);
        let decision =
            exact_campfire_policy_decision_v1(&healthy, &legal).expect("healthy boss campfire");
        assert_ne!(decision.prior.entries[0].candidate_id, "rest");
    }

    #[test]
    fn final_act_three_campfire_recalls_before_even_urgent_recovery() {
        let mut session = campfire_session(&[CardId::Whirlwind, CardId::SecondWind]);
        session.run_state.is_final_act_available = true;
        session.run_state.act_num = 3;
        session.run_state.keys[0] = false;
        session.run_state.current_hp = 1;
        session.run_state.max_hp = 80;
        session.run_state.boss_key = Some(EncounterId::TimeEater);
        set_pre_boss_map_position(&mut session);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_campfire_policy_decision_v1(&session, &legal)
            .expect("final Act 3 campfire decision");

        assert_eq!(
            decision.evidence[0].band,
            CampfirePolicyBandV1::HeartKeyDeadline
        );
        assert!(matches!(
            decision.evidence[0].action,
            CampfirePolicyActionV1::Recall {
                ruby_key_gained: true
            }
        ));
    }

    #[test]
    fn forced_combat_before_next_campfire_promotes_efficient_recovery() {
        let mut session = campfire_session(&[CardId::Apparition, CardId::Cleave]);
        session.run_state.current_hp = 26;
        session.run_state.max_hp = 50;
        session.run_state.boss_key = Some(EncounterId::DonuAndDeca);
        set_linear_map(
            &mut session,
            &[
                RoomType::RestRoom,
                RoomType::MonsterRoom,
                RoomType::RestRoom,
            ],
        );

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_campfire_policy_decision_v1(&session, &legal).expect("forced combat recovery");

        assert_eq!(
            decision.recovery.known_combat_before_next_campfire,
            RouteWindowModality::Must
        );
        assert_eq!(decision.prior.entries[0].candidate_id, "rest");
        assert_eq!(
            decision.evidence[0].band,
            CampfirePolicyBandV1::PrepareBossSurvival
        );
    }

    #[test]
    fn forced_elite_recovery_precedes_required_upgrade_debt() {
        let mut session = campfire_session(&[
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::PowerThrough,
            CardId::Uppercut,
            CardId::SecondWind,
            CardId::Cleave,
            CardId::ShrugItOff,
            CardId::DarkEmbrace,
            CardId::Uppercut,
            CardId::WildStrike,
            CardId::DarkEmbrace,
            CardId::FiendFire,
            CardId::Cleave,
            CardId::DemonForm,
            CardId::SwordBoomerang,
            CardId::Clothesline,
            CardId::ShrugItOff,
        ]);
        session.run_state.master_deck[8].upgrades = 1;
        session.run_state.master_deck[13].upgrades = 1;
        session.run_state.act_num = 2;
        session.run_state.floor_num = 27;
        session.run_state.current_hp = 35;
        session.run_state.max_hp = 80;
        session.run_state.boss_key = Some(EncounterId::TheChamp);
        set_linear_map(
            &mut session,
            &[
                RoomType::RestRoom,
                RoomType::MonsterRoomElite,
                RoomType::RestRoom,
            ],
        );

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_campfire_policy_decision_v1(&session, &legal)
            .expect("forced elite recovery decision");
        let rest = decision
            .evidence
            .iter()
            .find(|candidate| candidate.candidate_id == "rest")
            .expect("rest evidence");

        assert_eq!(
            decision.recovery.known_elite_before_next_campfire,
            RouteWindowModality::Must
        );
        assert_eq!(decision.recovery.rest_heal_capacity, 24);
        assert_eq!(rest.delta.hp_gain, 24);
        assert_eq!(rest.band, CampfirePolicyBandV1::ImmediateSurvival);
        assert_eq!(decision.prior.entries[0].candidate_id, "rest");
        assert!(
            decision.evidence.iter().any(|candidate| {
                matches!(candidate.action, CampfirePolicyActionV1::Smith { .. })
                    && candidate.band == CampfirePolicyBandV1::PayRequiredUpgradeDebt
            }),
            "the forced-elite recovery must outrank, not erase, required upgrade debt; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn forced_elite_survival_requires_must_and_efficient_recovery() {
        let mut session = campfire_session(&[CardId::Entrench]);
        session.run_state.current_hp = 35;
        session.run_state.max_hp = 80;
        let rest = CampfirePolicyActionV1::Rest { hp_gain: 24 };
        let optional_elite = CampfireRecoveryContextV1 {
            boss_nodes_remaining: Some(5),
            rest_heal_capacity: 24,
            known_combat_before_next_campfire: RouteWindowModality::Must,
            known_elite_before_next_campfire: RouteWindowModality::Can,
        };

        assert_eq!(
            campfire_policy_band_v1(&session, &rest, optional_elite),
            CampfirePolicyBandV1::PrepareBossSurvival
        );

        session.run_state.current_hp = 70;
        let inefficient_rest = CampfirePolicyActionV1::Rest { hp_gain: 10 };
        let forced_elite = CampfireRecoveryContextV1 {
            known_elite_before_next_campfire: RouteWindowModality::Must,
            ..optional_elite
        };
        assert_eq!(
            campfire_policy_band_v1(&session, &inefficient_rest, forced_elite),
            CampfirePolicyBandV1::MarginalRecovery
        );
    }

    #[test]
    fn marginal_recovery_does_not_displace_an_available_permanent_upgrade() {
        let mut session = campfire_session(&[
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Armaments,
        ]);
        session
            .run_state
            .master_deck
            .iter_mut()
            .find(|card| card.id == CardId::Bash)
            .expect("Bash")
            .upgrades = 1;
        session.run_state.current_hp = 79;
        session.run_state.max_hp = 85;
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);

        let decision =
            exact_campfire_policy_decision_v1(&session, &legal).expect("healthy campfire decision");
        let rest = decision
            .evidence
            .iter()
            .find(|candidate| matches!(candidate.action, CampfirePolicyActionV1::Rest { .. }))
            .expect("rest evidence");

        assert_eq!(rest.band, CampfirePolicyBandV1::MarginalRecovery);
        assert!(matches!(
            decision.evidence[0].action,
            CampfirePolicyActionV1::Smith {
                card: CardId::Armaments,
                ..
            }
        ));
    }

    #[test]
    fn recovery_before_combat_does_not_displace_reliability_upgrade() {
        let mut session = campfire_session(&[CardId::Apparition, CardId::Cleave]);
        session.run_state.current_hp = 26;
        session.run_state.max_hp = 50;
        session.run_state.boss_key = Some(EncounterId::DonuAndDeca);
        set_linear_map(
            &mut session,
            &[
                RoomType::RestRoom,
                RoomType::RestRoom,
                RoomType::MonsterRoom,
            ],
        );

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_campfire_policy_decision_v1(&session, &legal).expect("recovery-first route");

        assert_eq!(
            decision.recovery.known_combat_before_next_campfire,
            RouteWindowModality::Cannot
        );
        assert_ne!(decision.prior.entries[0].candidate_id, "rest");
    }

    #[test]
    fn required_boss_upgrade_precedes_non_emergency_pre_boss_rest() {
        let mut session = campfire_session(&[CardId::Entrench]);
        session.run_state.current_hp = 51;
        session.run_state.max_hp = 82;
        session.run_state.boss_key = Some(EncounterId::Collector);
        set_pre_boss_map_position(&mut session);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_campfire_policy_decision_v1(&session, &legal).expect("boss upgrade decision");

        assert_eq!(decision.evidence[0].candidate_id, "smith-0");
        assert_eq!(
            decision.evidence[0].band,
            CampfirePolicyBandV1::PayRequiredUpgradeDebt
        );
        assert_eq!(
            decision
                .evidence
                .iter()
                .find(|candidate| candidate.candidate_id == "rest")
                .map(|candidate| candidate.band),
            Some(CampfirePolicyBandV1::PrepareBossSurvival)
        );
    }

    #[test]
    fn reliability_upgrade_uses_shared_deck_repair_evidence() {
        let session = campfire_session(&[CardId::Apparition, CardId::Cleave]);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_campfire_policy_decision_v1(&session, &legal).expect("upgrade decision");
        let apparition = decision
            .evidence
            .iter()
            .find(|entry| {
                matches!(
                    entry.action,
                    CampfirePolicyActionV1::Smith {
                        card: CardId::Apparition,
                        ..
                    }
                )
            })
            .expect("Apparition smith");

        assert_eq!(apparition.band, CampfirePolicyBandV1::ImproveReliability);
        assert!(matches!(
            apparition.action,
            CampfirePolicyActionV1::Smith {
                repair_priority: Some(DeckRepairUpgradePriorityV1::Reliability),
                ..
            }
        ));
    }

    #[test]
    fn toke_uses_shared_target_loss_instead_of_card_name_fallbacks() {
        let mut session = campfire_session(&[CardId::Flex, CardId::Flex, CardId::Barricade]);
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::PeacePipe));
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_campfire_policy_decision_v1(&session, &legal).expect("toke decision");
        let best_toke = decision
            .evidence
            .iter()
            .find(|entry| matches!(entry.action, CampfirePolicyActionV1::Toke { .. }))
            .expect("toke action");

        assert!(matches!(
            best_toke.action,
            CampfirePolicyActionV1::Toke {
                card: CardId::Flex,
                shared_repair_target: true,
                ..
            }
        ));
        assert_eq!(best_toke.band, CampfirePolicyBandV1::RepairDeck);
    }
}
