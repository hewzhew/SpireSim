use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::ai::combat_upgrade_coverage_v1::{
    combat_upgrade_coverage_profile_v1, CombatUpgradeCoverageProfileV1,
};
use crate::ai::noncombat_strategy_v1::{
    build_run_strategy_snapshot_from_run_state_v2, StrategyCapabilityCoverageV1,
    StrategyCapabilityInputV1, StrategyCapabilityKindV1, StrategyDeckFactsV1,
    StrategyDeckFormationNeedV1, StrategyFormationSummaryV2, StrategyPackageIdV2,
    StrategyResourceFactsV2, StrategyThreatCoverageLedgerV1, StrategyThreatProfileV1,
    StrategyThreatSourceV1, StrategyThreatTagV1,
};

use super::{
    build_decision_surface, exact_run_decision_successor_v1, DecisionCandidateKey,
    ExactRunDecisionSuccessorV1, RunControlSession,
};

/// Typed strategic facts projected from one exact run state.
///
/// This deliberately omits package scores, verdicts, and rendered reasons.
/// Policies may consume these facts, but the exact session remains the
/// mechanics authority.
#[derive(Clone, Debug, PartialEq)]
pub struct RunPolicyStateEvidenceV1 {
    pub deck: StrategyDeckFactsV1,
    pub combat_upgrade_coverage: CombatUpgradeCoverageProfileV1,
    pub resources: StrategyResourceFactsV2,
    pub threats: StrategyThreatProfileV1,
    pub threat_coverage: StrategyThreatCoverageLedgerV1,
    pub formation: StrategyFormationSummaryV2,
}

/// One exact legal action after applying it to an isolated child session.
#[derive(Clone, Debug)]
pub struct ExactRunPolicyActionSuccessorV1 {
    pub candidate_id: String,
    pub candidate_key: Option<DecisionCandidateKey>,
    pub after: RunPolicyStateEvidenceV1,
    pub exact: ExactRunDecisionSuccessorV1,
}

/// The complete exact successor surface available to a policy at one decision.
///
/// `before` is stored once. Every action is still executed independently from
/// the same parent, so sequential state changes (discounts, restocks, nested
/// selections) are observed from the simulator rather than predicted here.
#[derive(Clone, Debug)]
pub struct ExactRunPolicyDecisionV1 {
    pub before: RunPolicyStateEvidenceV1,
    pub actions: Vec<ExactRunPolicyActionSuccessorV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPolicyCapabilityChangeV1 {
    pub capability: StrategyCapabilityKindV1,
    pub before: StrategyCapabilityCoverageV1,
    pub after: StrategyCapabilityCoverageV1,
    pub before_inputs: Vec<StrategyCapabilityInputV1>,
    pub after_inputs: Vec<StrategyCapabilityInputV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPolicyCapabilityRuleChangeV1 {
    pub capability: StrategyCapabilityKindV1,
    pub before: StrategyCapabilityCoverageV1,
    pub after: StrategyCapabilityCoverageV1,
    pub before_inputs: Vec<StrategyCapabilityInputV1>,
    pub after_inputs: Vec<StrategyCapabilityInputV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPolicyThreatGapKeyV1 {
    pub tag: StrategyThreatTagV1,
    pub source: StrategyThreatSourceV1,
    pub subject: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunPolicyStateDeltaV1 {
    pub gold_delta: i32,
    pub hp_gain: i32,
    pub max_hp_gain: i32,
    pub deck_size_delta: isize,
    pub closed_threat_gaps: Vec<RunPolicyThreatGapKeyV1>,
    pub capability_improvements: Vec<RunPolicyCapabilityChangeV1>,
    pub capability_rule_changes: Vec<RunPolicyCapabilityRuleChangeV1>,
    pub resolved_formation_needs: Vec<StrategyDeckFormationNeedV1>,
    pub added_formation_strengths: Vec<StrategyPackageIdV2>,
}

pub fn run_policy_state_evidence_v1(session: &RunControlSession) -> RunPolicyStateEvidenceV1 {
    let snapshot = build_run_strategy_snapshot_from_run_state_v2(&session.run_state);
    RunPolicyStateEvidenceV1 {
        deck: snapshot.deck_facts().clone(),
        combat_upgrade_coverage: combat_upgrade_coverage_profile_v1(&session.run_state),
        resources: snapshot.resources.clone(),
        threats: snapshot.threats.clone(),
        threat_coverage: snapshot.threat_coverage.clone(),
        formation: snapshot.formation_summary(),
    }
}

pub fn run_policy_state_delta_v1(
    before: &RunPolicyStateEvidenceV1,
    after: &RunPolicyStateEvidenceV1,
) -> RunPolicyStateDeltaV1 {
    let before_by_kind = before
        .threat_coverage
        .capabilities
        .iter()
        .map(|capability| (capability.capability, capability))
        .collect::<BTreeMap<_, _>>();
    let capability_rule_changes = after
        .threat_coverage
        .capabilities
        .iter()
        .filter_map(|capability| {
            let previous = before_by_kind.get(&capability.capability).copied();
            let previous_coverage = previous
                .map(|evidence| evidence.coverage)
                .unwrap_or(StrategyCapabilityCoverageV1::Unknown);
            let previous_inputs = previous
                .map(|evidence| evidence.inputs.clone())
                .unwrap_or_default();
            (capability.coverage != previous_coverage || capability.inputs != previous_inputs)
                .then_some(RunPolicyCapabilityRuleChangeV1 {
                    capability: capability.capability,
                    before: previous_coverage,
                    after: capability.coverage,
                    before_inputs: previous_inputs,
                    after_inputs: capability.inputs.clone(),
                })
        })
        .collect::<Vec<_>>();
    let capability_improvements = capability_rule_changes
        .iter()
        .filter(|change| coverage_strength(change.after) > coverage_strength(change.before))
        .map(|change| RunPolicyCapabilityChangeV1 {
            capability: change.capability,
            before: change.before,
            after: change.after,
            before_inputs: change.before_inputs.clone(),
            after_inputs: change.after_inputs.clone(),
        })
        .collect();

    RunPolicyStateDeltaV1 {
        gold_delta: after.resources.gold.saturating_sub(before.resources.gold),
        hp_gain: after
            .resources
            .current_hp
            .saturating_sub(before.resources.current_hp),
        max_hp_gain: after
            .resources
            .max_hp
            .saturating_sub(before.resources.max_hp),
        deck_size_delta: after.deck.deck_size as isize - before.deck.deck_size as isize,
        closed_threat_gaps: before
            .threat_coverage
            .gaps
            .iter()
            .filter(|gap| {
                !after.threat_coverage.gaps.iter().any(|remaining| {
                    remaining.tag == gap.tag
                        && remaining.source == gap.source
                        && remaining.subject == gap.subject
                })
            })
            .map(|gap| RunPolicyThreatGapKeyV1 {
                tag: gap.tag,
                source: gap.source,
                subject: gap.subject.clone(),
            })
            .collect(),
        capability_improvements,
        capability_rule_changes,
        resolved_formation_needs: before
            .formation
            .needs
            .iter()
            .filter(|need| !after.formation.needs.contains(need))
            .copied()
            .collect(),
        added_formation_strengths: after
            .formation
            .strengths
            .iter()
            .filter(|strength| !before.formation.strengths.contains(strength))
            .copied()
            .collect(),
    }
}

fn coverage_strength(coverage: StrategyCapabilityCoverageV1) -> u8 {
    match coverage {
        StrategyCapabilityCoverageV1::Unknown | StrategyCapabilityCoverageV1::Missing => 0,
        StrategyCapabilityCoverageV1::Thin => 1,
        StrategyCapabilityCoverageV1::Supported => 2,
        StrategyCapabilityCoverageV1::Strong => 3,
    }
}

pub fn exact_run_policy_decision_v1(
    parent: &RunControlSession,
) -> Result<ExactRunPolicyDecisionV1, String> {
    let surface = build_decision_surface(parent);
    let legal = surface
        .view
        .candidates
        .iter()
        .filter_map(|candidate| {
            candidate
                .action
                .executable_action_ref()
                .map(|action| (candidate, action))
        })
        .collect::<Vec<_>>();
    if legal.is_empty() {
        return Err("cannot build policy evidence for an empty decision surface".to_string());
    }

    let before = run_policy_state_evidence_v1(parent);
    let mut candidate_ids = BTreeSet::new();
    let mut actions = Vec::with_capacity(legal.len());
    for (candidate, action) in legal {
        if !candidate_ids.insert(candidate.id.as_str()) {
            return Err(format!(
                "exact decision surface duplicated candidate '{}'",
                candidate.id
            ));
        }
        let exact = exact_run_decision_successor_v1(parent, &candidate.id, action.clone())?;
        let candidate_key = exact
            .transaction
            .before
            .candidates
            .iter()
            .find(|snapshot| snapshot.candidate_id == candidate.id)
            .and_then(|snapshot| snapshot.key.clone());
        let after = run_policy_state_evidence_v1(&exact.session);
        actions.push(ExactRunPolicyActionSuccessorV1 {
            candidate_id: candidate.id.clone(),
            candidate_key,
            after,
            exact,
        });
    }

    Ok(ExactRunPolicyDecisionV1 { before, actions })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::combat_upgrade_coverage_v1::CombatUpgradeScopeV1;
    use crate::content::cards::CardId;
    use crate::content::relics::RelicId;
    use crate::eval::run_control::{build_decision_surface, RunControlConfig};
    use crate::state::core::EngineState;
    use crate::state::shop::{ShopCard, ShopRelic, ShopState};

    #[test]
    fn membership_evidence_observes_the_real_discounted_successor() {
        let mut parent = RunControlSession::new(RunControlConfig::default());
        parent.run_state.gold = 300;
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.cards.push(ShopCard {
            card_id: CardId::Armaments,
            upgrades: 0,
            price: 80,
            can_buy: true,
            blocked_reason: None,
        });
        shop.relics.push(ShopRelic {
            relic_id: RelicId::MembershipCard,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        parent.engine_state = EngineState::Shop(shop);

        let evidence =
            exact_run_policy_decision_v1(&parent).expect("exact Membership Card decision");
        let successor = evidence
            .actions
            .iter()
            .find(|action| {
                matches!(
                    action.candidate_key,
                    Some(DecisionCandidateKey::ShopBuyRelic {
                        relic: RelicId::MembershipCard,
                        ..
                    })
                )
            })
            .expect("Membership Card successor");

        assert_eq!(parent.run_state.gold, 300);
        assert_eq!(evidence.before.resources.gold, 300);
        assert_eq!(
            evidence.actions.len(),
            build_decision_surface(&parent)
                .view
                .candidates
                .iter()
                .filter(|candidate| candidate.action.executable_action_ref().is_some())
                .count()
        );
        assert_eq!(successor.after.resources.gold, 150);
        assert!(successor
            .exact
            .session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::MembershipCard));
        assert!(successor
            .exact
            .transaction
            .after
            .candidates
            .iter()
            .any(|candidate| matches!(
                candidate.key,
                Some(DecisionCandidateKey::ShopBuyCard {
                    card: CardId::Armaments,
                    price: 40,
                    ..
                })
            )));
    }

    #[test]
    fn duplicate_armaments_is_an_exact_marginal_deck_change() {
        let mut parent = RunControlSession::new(RunControlConfig::default());
        parent.run_state.gold = 100;
        parent
            .run_state
            .master_deck
            .push(crate::runtime::combat::CombatCard::for_scenario(
                CardId::Armaments,
                10_001,
            ));
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.cards.push(ShopCard {
            card_id: CardId::Armaments,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });
        parent.engine_state = EngineState::Shop(shop);

        let evidence = exact_run_policy_decision_v1(&parent).expect("exact Armaments decision");
        let successor = evidence
            .actions
            .iter()
            .find(|action| {
                matches!(
                    action.candidate_key,
                    Some(DecisionCandidateKey::ShopBuyCard {
                        card: CardId::Armaments,
                        ..
                    })
                )
            })
            .expect("Armaments successor");

        assert_eq!(
            successor.after.deck.deck_size,
            evidence.before.deck.deck_size + 1
        );
        assert_eq!(
            evidence.before.combat_upgrade_coverage.strongest_scope(),
            Some(CombatUpgradeScopeV1::SelectedCardInHand)
        );
        assert_eq!(
            successor.after.combat_upgrade_coverage.strongest_scope(),
            evidence.before.combat_upgrade_coverage.strongest_scope()
        );
        assert_eq!(
            successor
                .after
                .combat_upgrade_coverage
                .source_count(CombatUpgradeScopeV1::SelectedCardInHand),
            evidence
                .before
                .combat_upgrade_coverage
                .source_count(CombatUpgradeScopeV1::SelectedCardInHand)
                + 1
        );
        assert_eq!(successor.after.resources.gold, 50);
        assert_eq!(
            successor
                .exact
                .session
                .run_state
                .master_deck
                .iter()
                .filter(|card| card.id == CardId::Armaments)
                .count(),
            2
        );
    }

    #[test]
    fn nested_shop_purchase_exposes_the_real_followup_boundary() {
        let mut parent = RunControlSession::new(RunControlConfig::default());
        parent.run_state.gold = 300;
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Orrery,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        parent.engine_state = EngineState::Shop(shop);

        let evidence = exact_run_policy_decision_v1(&parent).expect("exact Orrery decision");
        let successor = evidence
            .actions
            .iter()
            .find(|action| {
                matches!(
                    action.candidate_key,
                    Some(DecisionCandidateKey::ShopBuyRelic {
                        relic: RelicId::Orrery,
                        ..
                    })
                )
            })
            .expect("Orrery successor");

        assert_eq!(successor.after.resources.gold, 150);
        assert!(matches!(
            successor.exact.session.engine_state,
            EngineState::RewardOverlay { .. }
        ));
        assert_eq!(
            successor
                .exact
                .transaction
                .after
                .candidates
                .iter()
                .filter(|candidate| matches!(
                    candidate.key,
                    Some(DecisionCandidateKey::CardRewardOpen { .. })
                ))
                .count(),
            5
        );
    }
}
