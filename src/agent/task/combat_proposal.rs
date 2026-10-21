//! Public combat proposals and fail-closed grounding.
//!
//! A proposal states a small semantic intention. The grounder resolves that
//! intention only against the current canonical public decision and its typed
//! mechanical facts. It never executes an action, inspects an exact successor,
//! or turns missing projection coverage into a negative fact.

use serde::{Deserialize, Serialize};

use crate::agent::information::effect::{
    PublicCombatCandidateEffectV1, PublicCombatCardMechanicalFactsV1, PublicCombatCoverageGapV1,
    PublicCombatDamageRecipientCertaintyV1, PublicCombatEnergyPaymentV1,
    PublicCombatMechanicalFactV1, PublicCombatMonsterReferenceV1, PublicCombatPublicConditionV1,
    PublicCombatStatusApplicationV1, PublicCombatTargetUncertaintyV1,
};
use crate::agent::information::state::PublicCombatStateV1;
use crate::ai::planner_core::stable_planner_id;
use crate::content::powers::PowerId;

use super::{
    TaskPublicChoiceV1, TaskPublicCombatCandidateEffectV1, TaskPublicDecisionV1,
    TaskPublicObservationV1,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatProposalPhaseV1 {
    Setup,
    Payoff,
    Complete,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "fact", rename_all = "snake_case")]
pub enum PublicCombatProposalRequiredFactV1 {
    CardAppliesStatus {
        power: PowerId,
        minimum_amount: i32,
    },
    DeterministicDirectCardDamage {
        minimum_amount_per_hit: i32,
        minimum_hit_count: u32,
    },
    KnownEnergyPayment,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatProposalV1 {
    pub proposal_id: String,
    pub origin_public_snapshot_id: String,
    pub target: PublicCombatMonsterReferenceV1,
    pub required_facts: Vec<PublicCombatProposalRequiredFactV1>,
    pub phase: PublicCombatProposalPhaseV1,
}

impl PublicCombatProposalV1 {
    /// Creates the first deliberately narrow universal proposal: establish
    /// public Vulnerable, then spend a separate deterministic direct-damage
    /// action on the same living target.
    pub fn setup_then_payoff_vulnerable_v1(
        decision: &TaskPublicDecisionV1,
        target_monster_index: usize,
    ) -> Result<Self, String> {
        decision.validate()?;
        let observation = combat_observation(decision)?;
        let target = observation
            .monsters
            .get(target_monster_index)
            .ok_or_else(|| {
                "combat proposal target index is outside the public monster list".to_string()
            })?;
        if !target.alive || target.dying || target.escaped {
            return Err("combat proposal target is not a live actionable monster".to_string());
        }
        let phase = if target_has_positive_power(target, PowerId::Vulnerable) {
            PublicCombatProposalPhaseV1::Payoff
        } else {
            PublicCombatProposalPhaseV1::Setup
        };
        let target = PublicCombatMonsterReferenceV1 {
            monster_index: target_monster_index,
            monster_slot: target.slot,
        };
        let required_facts = vec![
            PublicCombatProposalRequiredFactV1::CardAppliesStatus {
                power: PowerId::Vulnerable,
                minimum_amount: 1,
            },
            PublicCombatProposalRequiredFactV1::DeterministicDirectCardDamage {
                minimum_amount_per_hit: 1,
                minimum_hit_count: 1,
            },
            PublicCombatProposalRequiredFactV1::KnownEnergyPayment,
        ];
        let origin_public_snapshot_id = decision.information.snapshot_id.clone();
        let proposal_id = proposal_id(&origin_public_snapshot_id, &target, &required_facts, phase)?;
        Ok(Self {
            proposal_id,
            origin_public_snapshot_id,
            target,
            required_facts,
            phase,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.required_facts
            != [
                PublicCombatProposalRequiredFactV1::CardAppliesStatus {
                    power: PowerId::Vulnerable,
                    minimum_amount: 1,
                },
                PublicCombatProposalRequiredFactV1::DeterministicDirectCardDamage {
                    minimum_amount_per_hit: 1,
                    minimum_hit_count: 1,
                },
                PublicCombatProposalRequiredFactV1::KnownEnergyPayment,
            ]
        {
            return Err("combat proposal required-fact contract is not canonical".to_string());
        }
        let expected = proposal_id(
            &self.origin_public_snapshot_id,
            &self.target,
            &self.required_facts,
            self.phase,
        )?;
        if self.proposal_id != expected {
            return Err("combat proposal identity disagrees with its semantic payload".to_string());
        }
        Ok(())
    }

    pub fn ground_v1(&self, decision: &TaskPublicDecisionV1) -> PublicCombatProposalGroundingV1 {
        if let Err(reason) = self.validate() {
            return PublicCombatProposalGroundingV1::Invalidated {
                reason: PublicCombatProposalInvalidationReasonV1::NonCanonicalProposal,
                detail: reason,
            };
        }
        if let Err(reason) = decision.validate() {
            return PublicCombatProposalGroundingV1::Invalidated {
                reason: PublicCombatProposalInvalidationReasonV1::NonCanonicalDecision,
                detail: reason,
            };
        }
        let observation = match combat_observation(decision) {
            Ok(observation) => observation,
            Err(detail) => {
                return PublicCombatProposalGroundingV1::Invalidated {
                    reason: PublicCombatProposalInvalidationReasonV1::NotCombatDecision,
                    detail,
                }
            }
        };
        let Some(target) = observation.monsters.get(self.target.monster_index) else {
            return PublicCombatProposalGroundingV1::Invalidated {
                reason: PublicCombatProposalInvalidationReasonV1::TargetOccurrenceMissing,
                detail: "proposal target index is absent from the current public combat"
                    .to_string(),
            };
        };
        if target.slot != self.target.monster_slot {
            return PublicCombatProposalGroundingV1::Invalidated {
                reason: PublicCombatProposalInvalidationReasonV1::TargetOccurrenceMissing,
                detail: "proposal target index now names a different public monster slot"
                    .to_string(),
            };
        }
        if !target.alive || target.dying || target.escaped {
            return PublicCombatProposalGroundingV1::Completed {
                reason: PublicCombatProposalCompletionReasonV1::TargetNoLongerActionable,
            };
        }
        if self.phase == PublicCombatProposalPhaseV1::Complete {
            return PublicCombatProposalGroundingV1::Completed {
                reason: PublicCombatProposalCompletionReasonV1::DeclaredStepsGrounded,
            };
        }
        if self.phase == PublicCombatProposalPhaseV1::Setup
            && decision.information.snapshot_id != self.origin_public_snapshot_id
        {
            return PublicCombatProposalGroundingV1::Invalidated {
                reason: PublicCombatProposalInvalidationReasonV1::OriginDecisionChanged,
                detail: "setup proposal is being grounded against a different public origin"
                    .to_string(),
            };
        }
        if self.phase == PublicCombatProposalPhaseV1::Setup
            && target_has_positive_power(target, PowerId::Vulnerable)
        {
            return PublicCombatProposalGroundingV1::Invalidated {
                reason: PublicCombatProposalInvalidationReasonV1::PhaseContradictsPublicState,
                detail: "setup phase is stale because the target already visibly has Vulnerable"
                    .to_string(),
            };
        }
        if self.phase == PublicCombatProposalPhaseV1::Payoff
            && !target_has_positive_power(target, PowerId::Vulnerable)
        {
            return PublicCombatProposalGroundingV1::Invalidated {
                reason: PublicCombatProposalInvalidationReasonV1::RequiredPublicConditionFailed,
                detail: "payoff phase requires the target to visibly have Vulnerable".to_string(),
            };
        }

        match self.phase {
            PublicCombatProposalPhaseV1::Setup => self.ground_setup(decision, observation),
            PublicCombatProposalPhaseV1::Payoff => self.ground_payoff(decision, observation),
            PublicCombatProposalPhaseV1::Complete => unreachable!("completed above"),
        }
    }

    pub fn advance_after_grounded_v1(
        &self,
        decision: &TaskPublicDecisionV1,
        grounded: &PublicCombatGroundedProposalStepV1,
    ) -> Result<Self, String> {
        self.validate()?;
        let PublicCombatProposalGroundingV1::Grounded { step: canonical } =
            self.ground_v1(decision)
        else {
            return Err(
                "combat proposal can advance only from one uniquely grounded current decision"
                    .to_string(),
            );
        };
        if canonical != *grounded {
            return Err(
                "grounded combat step is not canonical for the supplied decision".to_string(),
            );
        }
        let mut next = self.clone();
        next.phase = match self.phase {
            PublicCombatProposalPhaseV1::Setup => PublicCombatProposalPhaseV1::Payoff,
            PublicCombatProposalPhaseV1::Payoff => PublicCombatProposalPhaseV1::Complete,
            PublicCombatProposalPhaseV1::Complete => {
                return Err("completed combat proposal cannot advance again".to_string())
            }
        };
        next.proposal_id = proposal_id(
            &next.origin_public_snapshot_id,
            &next.target,
            &next.required_facts,
            next.phase,
        )?;
        Ok(next)
    }

    fn ground_setup(
        &self,
        decision: &TaskPublicDecisionV1,
        observation: &PublicCombatStateV1,
    ) -> PublicCombatProposalGroundingV1 {
        let scan = match scan_candidates(decision, |effect, facts| {
            matches_status_setup(effect, facts, observation, &self.target)
        }) {
            Ok(scan) => scan,
            Err(detail) => return projection_invalidated(detail),
        };
        if !scan.gaps.is_empty() {
            return unknown_coverage(PublicCombatProposalPhaseV1::Setup, scan.gaps);
        }

        let payoff_scan = match scan_candidates(decision, |effect, facts| {
            matches_deterministic_damage(effect, facts, &self.target)
        }) {
            Ok(scan) => scan,
            Err(detail) => return projection_invalidated(detail),
        };
        if !payoff_scan.gaps.is_empty() {
            return unknown_coverage(PublicCombatProposalPhaseV1::Setup, payoff_scan.gaps);
        }
        let viable_setups = scan
            .matches
            .into_iter()
            .filter(|setup| {
                payoff_scan.matches.iter().any(|payoff| {
                    setup.ordinal != payoff.ordinal
                        && setup.energy_payment.saturating_add(payoff.energy_payment)
                            <= observation.turn.energy
                })
            })
            .collect::<Vec<_>>();
        finish_grounding(
            &self.proposal_id,
            decision,
            PublicCombatProposalPhaseV1::Setup,
            viable_setups,
            PublicCombatProposalInapplicableReasonV1::NoAffordableSetupPayoffPair,
        )
    }

    fn ground_payoff(
        &self,
        decision: &TaskPublicDecisionV1,
        _observation: &PublicCombatStateV1,
    ) -> PublicCombatProposalGroundingV1 {
        let scan = match scan_candidates(decision, |effect, facts| {
            matches_deterministic_damage(effect, facts, &self.target)
        }) {
            Ok(scan) => scan,
            Err(detail) => return projection_invalidated(detail),
        };
        if !scan.gaps.is_empty() {
            return unknown_coverage(PublicCombatProposalPhaseV1::Payoff, scan.gaps);
        }
        finish_grounding(
            &self.proposal_id,
            decision,
            PublicCombatProposalPhaseV1::Payoff,
            scan.matches,
            PublicCombatProposalInapplicableReasonV1::NoMatchingCandidate,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatGroundedProposalStepV1 {
    pub proposal_id: String,
    pub decision_snapshot_id: String,
    pub phase: PublicCombatProposalPhaseV1,
    pub choice: TaskPublicChoiceV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "grounding", rename_all = "snake_case")]
pub enum PublicCombatProposalGroundingV1 {
    Grounded {
        step: PublicCombatGroundedProposalStepV1,
    },
    Ambiguous {
        phase: PublicCombatProposalPhaseV1,
        choices: Vec<TaskPublicChoiceV1>,
    },
    Inapplicable {
        phase: PublicCombatProposalPhaseV1,
        reason: PublicCombatProposalInapplicableReasonV1,
    },
    UnknownCoverage {
        phase: PublicCombatProposalPhaseV1,
        gaps: Vec<PublicCombatCoverageGapV1>,
    },
    Invalidated {
        reason: PublicCombatProposalInvalidationReasonV1,
        detail: String,
    },
    Completed {
        reason: PublicCombatProposalCompletionReasonV1,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatProposalInapplicableReasonV1 {
    NoMatchingCandidate,
    NoAffordableSetupPayoffPair,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatProposalInvalidationReasonV1 {
    NonCanonicalProposal,
    NonCanonicalDecision,
    NotCombatDecision,
    TargetOccurrenceMissing,
    OriginDecisionChanged,
    PhaseContradictsPublicState,
    RequiredPublicConditionFailed,
    CandidateProjectionFailed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicCombatProposalCompletionReasonV1 {
    TargetNoLongerActionable,
    DeclaredStepsGrounded,
}

#[derive(Clone, Copy, Debug)]
struct CandidateMatchV1 {
    ordinal: usize,
    energy_payment: u8,
}

#[derive(Default)]
struct CandidateScanV1 {
    matches: Vec<CandidateMatchV1>,
    gaps: Vec<PublicCombatCoverageGapV1>,
}

enum CandidateFactMatchV1 {
    Match { energy_payment: u8 },
    NoMatch,
    Unknown(Vec<PublicCombatCoverageGapV1>),
}

fn scan_candidates(
    decision: &TaskPublicDecisionV1,
    matcher: impl Fn(
        &TaskPublicCombatCandidateEffectV1,
        &PublicCombatCardMechanicalFactsV1,
    ) -> CandidateFactMatchV1,
) -> Result<CandidateScanV1, String> {
    let mut scan = CandidateScanV1::default();
    for ordinal in 0..decision.candidates.len() {
        let effect = decision
            .combat_candidate_effect_v1(ordinal)
            .map_err(|error| format!("combat candidate {ordinal} projection failed: {error}"))?;
        let PublicCombatCandidateEffectV1::PlayCard { card } = &effect.effect.effect else {
            continue;
        };
        let facts = card.mechanical_facts_v1();
        match matcher(&effect, &facts) {
            CandidateFactMatchV1::Match { energy_payment } => {
                scan.matches.push(CandidateMatchV1 {
                    ordinal,
                    energy_payment,
                });
            }
            CandidateFactMatchV1::NoMatch => {}
            CandidateFactMatchV1::Unknown(gaps) => {
                for gap in gaps {
                    if !scan.gaps.contains(&gap) {
                        scan.gaps.push(gap);
                    }
                }
            }
        }
    }
    Ok(scan)
}

fn matches_status_setup(
    _effect: &TaskPublicCombatCandidateEffectV1,
    facts: &PublicCombatCardMechanicalFactsV1,
    observation: &PublicCombatStateV1,
    target: &PublicCombatMonsterReferenceV1,
) -> CandidateFactMatchV1 {
    let statuses = match resolve_conditioned_fact(&facts.status_applications, observation) {
        ResolvedFactV1::Known(statuses) => statuses,
        ResolvedFactV1::ConditionFalse => return CandidateFactMatchV1::NoMatch,
        ResolvedFactV1::Unknown(gap) => return CandidateFactMatchV1::Unknown(vec![gap]),
    };
    if !statuses
        .iter()
        .any(|status| status_matches(status, target, PowerId::Vulnerable) && status.amount >= 1)
    {
        return CandidateFactMatchV1::NoMatch;
    }
    known_energy_payment(&facts.energy_payment)
}

fn matches_deterministic_damage(
    _effect: &TaskPublicCombatCandidateEffectV1,
    facts: &PublicCombatCardMechanicalFactsV1,
    target: &PublicCombatMonsterReferenceV1,
) -> CandidateFactMatchV1 {
    let mut gaps = Vec::new();
    let damages = match &facts.damage_per_hit {
        PublicCombatMechanicalFactV1::Known { value } => Some(value),
        PublicCombatMechanicalFactV1::UnknownCoverage { gap } => {
            gaps.push(*gap);
            None
        }
        PublicCombatMechanicalFactV1::Conditional { .. } => None,
    };
    let hit_count = match &facts.hit_count {
        PublicCombatMechanicalFactV1::Known { value } => Some(*value),
        PublicCombatMechanicalFactV1::UnknownCoverage { gap } => {
            gaps.push(*gap);
            None
        }
        PublicCombatMechanicalFactV1::Conditional { .. } => None,
    };
    let uncertainty = match &facts.target_uncertainty {
        PublicCombatMechanicalFactV1::Known { value } => Some(*value),
        PublicCombatMechanicalFactV1::UnknownCoverage { gap } => {
            gaps.push(*gap);
            None
        }
        PublicCombatMechanicalFactV1::Conditional { .. } => None,
    };
    if !gaps.is_empty() {
        return CandidateFactMatchV1::Unknown(gaps);
    }
    let matches = hit_count.is_some_and(|count| count >= 1)
        && uncertainty == Some(PublicCombatTargetUncertaintyV1::Deterministic)
        && damages.is_some_and(|damages| {
            damages.iter().any(|damage| {
                damage.amount_per_hit >= 1
                    && damage.recipient.monster_slot == target.monster_slot
                    && damage.recipient_certainty
                        != PublicCombatDamageRecipientCertaintyV1::PossibleRandomTarget
            })
        });
    if !matches {
        return CandidateFactMatchV1::NoMatch;
    }
    known_energy_payment(&facts.energy_payment)
}

fn known_energy_payment(
    payment: &PublicCombatMechanicalFactV1<PublicCombatEnergyPaymentV1>,
) -> CandidateFactMatchV1 {
    match payment {
        PublicCombatMechanicalFactV1::Known { value } => CandidateFactMatchV1::Match {
            energy_payment: value.amount,
        },
        PublicCombatMechanicalFactV1::UnknownCoverage { gap } => {
            CandidateFactMatchV1::Unknown(vec![*gap])
        }
        PublicCombatMechanicalFactV1::Conditional { .. } => CandidateFactMatchV1::NoMatch,
    }
}

enum ResolvedFactV1<'a, T> {
    Known(&'a T),
    ConditionFalse,
    Unknown(PublicCombatCoverageGapV1),
}

fn resolve_conditioned_fact<'a, T>(
    fact: &'a PublicCombatMechanicalFactV1<T>,
    observation: &PublicCombatStateV1,
) -> ResolvedFactV1<'a, T> {
    match fact {
        PublicCombatMechanicalFactV1::Known { value } => ResolvedFactV1::Known(value),
        PublicCombatMechanicalFactV1::Conditional { value, condition } => {
            if public_condition_holds(condition, observation) {
                ResolvedFactV1::Known(value)
            } else {
                ResolvedFactV1::ConditionFalse
            }
        }
        PublicCombatMechanicalFactV1::UnknownCoverage { gap } => ResolvedFactV1::Unknown(*gap),
    }
}

fn public_condition_holds(
    condition: &PublicCombatPublicConditionV1,
    observation: &PublicCombatStateV1,
) -> bool {
    match condition {
        PublicCombatPublicConditionV1::TargetLacksPower { target, power } => observation
            .monsters
            .iter()
            .find(|monster| monster.slot == target.monster_slot)
            .is_some_and(|monster| !target_has_positive_power(monster, *power)),
    }
}

fn target_has_positive_power(
    target: &crate::agent::information::state::CombatLearningMonsterStateV1,
    power: PowerId,
) -> bool {
    target
        .powers
        .iter()
        .any(|status| status.power == power && status.amount > 0)
}

fn status_matches(
    status: &PublicCombatStatusApplicationV1,
    target: &PublicCombatMonsterReferenceV1,
    power: PowerId,
) -> bool {
    status.target.monster_slot == target.monster_slot && status.power == power
}

fn finish_grounding(
    proposal_id: &str,
    decision: &TaskPublicDecisionV1,
    phase: PublicCombatProposalPhaseV1,
    matches: Vec<CandidateMatchV1>,
    inapplicable_reason: PublicCombatProposalInapplicableReasonV1,
) -> PublicCombatProposalGroundingV1 {
    match matches.as_slice() {
        [] => PublicCombatProposalGroundingV1::Inapplicable {
            phase,
            reason: inapplicable_reason,
        },
        [candidate] => PublicCombatProposalGroundingV1::Grounded {
            step: PublicCombatGroundedProposalStepV1 {
                proposal_id: proposal_id.to_string(),
                decision_snapshot_id: decision.information.snapshot_id.clone(),
                phase,
                choice: TaskPublicChoiceV1::one(candidate.ordinal),
            },
        },
        candidates => PublicCombatProposalGroundingV1::Ambiguous {
            phase,
            choices: candidates
                .iter()
                .map(|candidate| TaskPublicChoiceV1::one(candidate.ordinal))
                .collect(),
        },
    }
}

fn unknown_coverage(
    phase: PublicCombatProposalPhaseV1,
    gaps: Vec<PublicCombatCoverageGapV1>,
) -> PublicCombatProposalGroundingV1 {
    PublicCombatProposalGroundingV1::UnknownCoverage { phase, gaps }
}

fn projection_invalidated(detail: String) -> PublicCombatProposalGroundingV1 {
    PublicCombatProposalGroundingV1::Invalidated {
        reason: PublicCombatProposalInvalidationReasonV1::CandidateProjectionFailed,
        detail,
    }
}

fn combat_observation(decision: &TaskPublicDecisionV1) -> Result<&PublicCombatStateV1, String> {
    match &decision.observation {
        TaskPublicObservationV1::Combat { observation, .. } => Ok(observation),
        TaskPublicObservationV1::Strategic { .. } => {
            Err("public combat proposal requires a combat decision".to_string())
        }
    }
}

fn proposal_id(
    origin_public_snapshot_id: &str,
    target: &PublicCombatMonsterReferenceV1,
    required_facts: &[PublicCombatProposalRequiredFactV1],
    phase: PublicCombatProposalPhaseV1,
) -> Result<String, String> {
    stable_planner_id(
        "public_combat_proposal_v1",
        &(origin_public_snapshot_id, target, required_facts, phase),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::information::action::project_public_combat_actions_v1;
    use crate::agent::information::run::{PublicCombatRunContextGapV1, PublicCombatRunContextV1};
    use crate::agent::information::state::public_combat_state_v1;
    use crate::agent::task::task_public_combat_decision_v1;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::store;
    use crate::runtime::combat::CombatCard;
    use crate::runtime::combat::{Power, PowerPayload};
    use crate::state::EngineState;

    fn combat_decision(
        cards: &[CardId],
        monster_power: Option<(PowerId, i32)>,
    ) -> TaskPublicDecisionV1 {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = cards
            .iter()
            .enumerate()
            .map(|(index, card)| CombatCard::for_scenario(*card, 100 + index as u32))
            .collect();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 77;
        monster.slot = 4;
        combat.entities.monsters = vec![monster];
        if let Some((power, amount)) = monster_power {
            store::set_powers_for(
                &mut combat,
                77,
                vec![Power {
                    power_type: power,
                    instance_id: Some(700),
                    amount,
                    extra_data: 0,
                    payload: PowerPayload::None,
                    just_applied: false,
                }],
            );
        }
        let actions = project_public_combat_actions_v1(&EngineState::CombatPlayerTurn, &combat)
            .expect("public actions");
        let observation = public_combat_state_v1(&combat);
        task_public_combat_decision_v1(
            PublicCombatRunContextV1::Unavailable {
                reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
            },
            observation,
            &actions.public,
        )
        .expect("combat decision")
    }

    fn grounded_step(
        grounding: PublicCombatProposalGroundingV1,
    ) -> PublicCombatGroundedProposalStepV1 {
        let PublicCombatProposalGroundingV1::Grounded { step } = grounding else {
            panic!("expected one grounded proposal step, got {grounding:?}")
        };
        step
    }

    fn chosen_card(
        decision: &TaskPublicDecisionV1,
        step: &PublicCombatGroundedProposalStepV1,
    ) -> CardId {
        let ordinal = step.choice.ordinal_path[0];
        let effect = decision
            .combat_candidate_effect_v1(ordinal)
            .expect("chosen effect");
        let PublicCombatCandidateEffectV1::PlayCard { card } = effect.effect.effect else {
            panic!("proposal chose a non-card candidate")
        };
        card.card_id
    }

    #[test]
    fn vulnerable_setup_then_deterministic_damage_grounds_across_public_decisions() {
        let setup_decision = combat_decision(&[CardId::Bash, CardId::Strike], None);
        let proposal = PublicCombatProposalV1::setup_then_payoff_vulnerable_v1(&setup_decision, 0)
            .expect("proposal");
        let setup = grounded_step(proposal.ground_v1(&setup_decision));
        assert_eq!(chosen_card(&setup_decision, &setup), CardId::Bash);

        let payoff_proposal = proposal
            .advance_after_grounded_v1(&setup_decision, &setup)
            .expect("payoff phase");
        let payoff_decision = combat_decision(&[CardId::Strike], Some((PowerId::Vulnerable, 2)));
        let payoff = grounded_step(payoff_proposal.ground_v1(&payoff_decision));
        assert_eq!(chosen_card(&payoff_decision, &payoff), CardId::Strike);
        let complete = payoff_proposal
            .advance_after_grounded_v1(&payoff_decision, &payoff)
            .expect("complete phase");
        assert!(matches!(
            complete.ground_v1(&payoff_decision),
            PublicCombatProposalGroundingV1::Completed {
                reason: PublicCombatProposalCompletionReasonV1::DeclaredStepsGrounded
            }
        ));
    }

    #[test]
    fn multiple_valid_setups_are_ambiguous_instead_of_silently_ranked() {
        let decision = combat_decision(&[CardId::Bash, CardId::Bash, CardId::Strike], None);
        let proposal = PublicCombatProposalV1::setup_then_payoff_vulnerable_v1(&decision, 0)
            .expect("proposal");

        let PublicCombatProposalGroundingV1::Ambiguous { phase, choices } =
            proposal.ground_v1(&decision)
        else {
            panic!("two Bash occurrences must stay ambiguous")
        };
        assert_eq!(phase, PublicCombatProposalPhaseV1::Setup);
        assert_eq!(choices.len(), 2);
    }

    #[test]
    fn artifact_and_random_target_do_not_become_a_fake_setup_payoff_line() {
        let artifact = combat_decision(
            &[CardId::Bash, CardId::Strike],
            Some((PowerId::Artifact, 1)),
        );
        let proposal = PublicCombatProposalV1::setup_then_payoff_vulnerable_v1(&artifact, 0)
            .expect("proposal");
        assert!(matches!(
            proposal.ground_v1(&artifact),
            PublicCombatProposalGroundingV1::Inapplicable {
                reason: PublicCombatProposalInapplicableReasonV1::NoAffordableSetupPayoffPair,
                ..
            }
        ));

        let random = combat_decision(&[CardId::Bash, CardId::SwordBoomerang], None);
        let proposal =
            PublicCombatProposalV1::setup_then_payoff_vulnerable_v1(&random, 0).expect("proposal");
        assert!(matches!(
            proposal.ground_v1(&random),
            PublicCombatProposalGroundingV1::Inapplicable {
                reason: PublicCombatProposalInapplicableReasonV1::NoAffordableSetupPayoffPair,
                ..
            }
        ));
    }

    #[test]
    fn already_vulnerable_target_starts_at_payoff_and_advance_rechecks_the_decision() {
        let decision = combat_decision(&[CardId::Strike], Some((PowerId::Vulnerable, 1)));
        let proposal = PublicCombatProposalV1::setup_then_payoff_vulnerable_v1(&decision, 0)
            .expect("proposal");
        assert_eq!(proposal.phase, PublicCombatProposalPhaseV1::Payoff);
        let grounded = grounded_step(proposal.ground_v1(&decision));

        let another_decision =
            combat_decision(&[CardId::PommelStrike], Some((PowerId::Vulnerable, 1)));
        assert!(proposal
            .advance_after_grounded_v1(&another_decision, &grounded)
            .is_err());
    }

    #[test]
    fn unreviewed_candidate_mechanics_return_a_typed_coverage_gap() {
        let decision = combat_decision(&[CardId::Bash, CardId::Strike, CardId::Anger], None);
        let proposal = PublicCombatProposalV1::setup_then_payoff_vulnerable_v1(&decision, 0)
            .expect("proposal");

        let PublicCombatProposalGroundingV1::UnknownCoverage { phase, gaps } =
            proposal.ground_v1(&decision)
        else {
            panic!("unreviewed Anger must not be interpreted as lacking status mechanics")
        };
        assert_eq!(phase, PublicCombatProposalPhaseV1::Setup);
        assert!(gaps.iter().any(|gap| {
            gap.card_id == CardId::Anger
                && gap.axis == crate::agent::information::effect::PublicCombatMechanicAxisV1::Status
        }));
    }
}
