//! Public-only structured combat plan generation and execution.
//!
//! The generator considers semantic plans without executing successors. The
//! follower acts only when the current public decision has one unambiguous,
//! fully covered grounding. Combat choices are commit-only: this module does
//! not claim that playing a card preserves the current autosave generation.

use serde::{Deserialize, Serialize};

use super::{
    PublicCombatGroundedProposalStepV1, PublicCombatProposalGroundingV1, PublicCombatProposalV1,
    TaskActorBoundaryV1, TaskActorIdentityV1, TaskActorIntentV1, TaskActorV1, TaskActorViewV1,
    TaskPrefixProposalV1, TaskProposalExplorationAuthorizationV1, TaskProposalProvenanceV1,
    TaskProposalRoleV1, TaskPublicDecisionV1, TaskPublicObservationV1, TaskScopeOrdinalV1,
};

pub const VULNERABLE_SETUP_PAYOFF_PLAN_GENERATOR_NAME_V1: &str =
    "public_vulnerable_setup_payoff_plan";
pub const VULNERABLE_SETUP_PAYOFF_PLAN_GENERATOR_REVISION_V1: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatPlanGroundingAttemptV1 {
    pub proposal: PublicCombatProposalV1,
    pub grounding: PublicCombatProposalGroundingV1,
}

/// Typed answer to "does this public decision contain exactly one safe
/// semantic-plan grounding?" Unknown mechanics and ambiguity block selection
/// rather than being treated as negative facts.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum PublicCombatPlanGenerationV1 {
    UniqueGrounded {
        decision_snapshot_id: String,
        proposal: PublicCombatProposalV1,
        step: PublicCombatGroundedProposalStepV1,
        attempts: Vec<PublicCombatPlanGroundingAttemptV1>,
    },
    Ambiguous {
        decision_snapshot_id: String,
        attempts: Vec<PublicCombatPlanGroundingAttemptV1>,
    },
    UnknownCoverage {
        decision_snapshot_id: String,
        attempts: Vec<PublicCombatPlanGroundingAttemptV1>,
    },
    Inapplicable {
        decision_snapshot_id: String,
        attempts: Vec<PublicCombatPlanGroundingAttemptV1>,
    },
    Invalidated {
        decision_snapshot_id: String,
        detail: String,
        attempts: Vec<PublicCombatPlanGroundingAttemptV1>,
    },
}

impl PublicCombatPlanGenerationV1 {
    fn unique_grounded(
        &self,
    ) -> Option<(&PublicCombatProposalV1, &PublicCombatGroundedProposalStepV1)> {
        match self {
            Self::UniqueGrounded { proposal, step, .. } => Some((proposal, step)),
            Self::Ambiguous { .. }
            | Self::UnknownCoverage { .. }
            | Self::Inapplicable { .. }
            | Self::Invalidated { .. } => None,
        }
    }
}

pub trait PublicCombatPlanGeneratorV1 {
    fn provenance(&self) -> TaskProposalProvenanceV1;

    fn generate(&mut self, decision: &TaskPublicDecisionV1) -> PublicCombatPlanGenerationV1;
}

/// Generates one setup/payoff proposal for every currently actionable public
/// monster occurrence, then requires a unique grounding across the complete
/// set. It does not rank targets or card occurrences.
#[derive(Clone, Debug, Default)]
pub struct VulnerableSetupPayoffPlanGeneratorV1;

impl PublicCombatPlanGeneratorV1 for VulnerableSetupPayoffPlanGeneratorV1 {
    fn provenance(&self) -> TaskProposalProvenanceV1 {
        TaskProposalProvenanceV1::public_structured_plan(
            VULNERABLE_SETUP_PAYOFF_PLAN_GENERATOR_NAME_V1,
            VULNERABLE_SETUP_PAYOFF_PLAN_GENERATOR_REVISION_V1,
        )
    }

    fn generate(&mut self, decision: &TaskPublicDecisionV1) -> PublicCombatPlanGenerationV1 {
        let decision_snapshot_id = decision.information.snapshot_id.clone();
        if decision.validate().is_err() {
            return PublicCombatPlanGenerationV1::Invalidated {
                decision_snapshot_id,
                detail: "combat plan generator received a non-canonical decision".to_string(),
                attempts: Vec::new(),
            };
        }
        let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
            return PublicCombatPlanGenerationV1::Inapplicable {
                decision_snapshot_id,
                attempts: Vec::new(),
            };
        };

        let mut attempts = Vec::new();
        for (monster_index, _) in observation
            .monsters
            .iter()
            .enumerate()
            .filter(|(_, monster)| monster.alive && !monster.dying && !monster.escaped)
        {
            let proposal = match PublicCombatProposalV1::setup_then_payoff_vulnerable_v1(
                decision,
                monster_index,
            ) {
                Ok(proposal) => proposal,
                Err(detail) => {
                    return PublicCombatPlanGenerationV1::Invalidated {
                        decision_snapshot_id,
                        detail,
                        attempts,
                    }
                }
            };
            attempts.push(PublicCombatPlanGroundingAttemptV1 {
                grounding: proposal.ground_v1(decision),
                proposal,
            });
        }

        if attempts.iter().any(|attempt| {
            matches!(
                attempt.grounding,
                PublicCombatProposalGroundingV1::Invalidated { .. }
            )
        }) {
            return PublicCombatPlanGenerationV1::Invalidated {
                decision_snapshot_id,
                detail:
                    "at least one generated public combat proposal invalidated during grounding"
                        .to_string(),
                attempts,
            };
        }
        if attempts.iter().any(|attempt| {
            matches!(
                attempt.grounding,
                PublicCombatProposalGroundingV1::UnknownCoverage { .. }
            )
        }) {
            return PublicCombatPlanGenerationV1::UnknownCoverage {
                decision_snapshot_id,
                attempts,
            };
        }

        let grounded = attempts
            .iter()
            .filter_map(|attempt| match &attempt.grounding {
                PublicCombatProposalGroundingV1::Grounded { step } => {
                    Some((attempt.proposal.clone(), step.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if grounded.len() == 1
            && !attempts.iter().any(|attempt| {
                matches!(
                    attempt.grounding,
                    PublicCombatProposalGroundingV1::Ambiguous { .. }
                )
            })
        {
            let (proposal, step) = grounded.into_iter().next().expect("one grounded plan");
            return PublicCombatPlanGenerationV1::UniqueGrounded {
                decision_snapshot_id,
                proposal,
                step,
                attempts,
            };
        }
        if !grounded.is_empty()
            || attempts.iter().any(|attempt| {
                matches!(
                    attempt.grounding,
                    PublicCombatProposalGroundingV1::Ambiguous { .. }
                )
            })
        {
            return PublicCombatPlanGenerationV1::Ambiguous {
                decision_snapshot_id,
                attempts,
            };
        }
        PublicCombatPlanGenerationV1::Inapplicable {
            decision_snapshot_id,
            attempts,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PublicCombatPlanFollowerEventV1 {
    Generated {
        generation: PublicCombatPlanGenerationV1,
    },
    StepIssued {
        proposal: PublicCombatProposalV1,
        step: PublicCombatGroundedProposalStepV1,
        prefix_proposal: TaskPrefixProposalV1,
    },
    SuccessorGrounded {
        proposal: PublicCombatProposalV1,
        grounding: PublicCombatProposalGroundingV1,
    },
    AdvanceFailed {
        proposal: PublicCombatProposalV1,
        detail: String,
    },
    InterruptedAtCommitReady {
        proposal: PublicCombatProposalV1,
        issued_step: PublicCombatGroundedProposalStepV1,
    },
    InterruptedAtScopeChange {
        proposal: PublicCombatProposalV1,
        issued_step: PublicCombatGroundedProposalStepV1,
        source_scope: TaskScopeOrdinalV1,
        observed_scope: Option<TaskScopeOrdinalV1>,
    },
}

#[derive(Clone)]
struct ActivePublicCombatPlanV1 {
    scope: TaskScopeOrdinalV1,
    proposal: PublicCombatProposalV1,
    source_decision: TaskPublicDecisionV1,
    issued_step: PublicCombatGroundedProposalStepV1,
}

/// Follows one public semantic plan across actor boundaries. It never reloads,
/// evaluates a hidden successor, or silently chooses among ambiguous plans.
#[derive(Clone)]
pub struct TaskPublicCombatPlanFollowerActorV1<G, F> {
    identity: TaskActorIdentityV1,
    generator: G,
    fallback: F,
    active: Option<ActivePublicCombatPlanV1>,
    events: Vec<PublicCombatPlanFollowerEventV1>,
}

impl<G, F> TaskPublicCombatPlanFollowerActorV1<G, F> {
    pub fn new(identity: TaskActorIdentityV1, generator: G, fallback: F) -> Self {
        Self {
            identity,
            generator,
            fallback,
            active: None,
            events: Vec::new(),
        }
    }

    pub fn events(&self) -> &[PublicCombatPlanFollowerEventV1] {
        &self.events
    }

    pub fn fallback(&self) -> &F {
        &self.fallback
    }

    fn unsupported(reason: impl Into<String>) -> TaskActorIntentV1 {
        TaskActorIntentV1::UnsupportedDecisionSurface {
            reason: reason.into(),
        }
    }

    fn issue_step(
        &mut self,
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
        proposal: PublicCombatProposalV1,
        step: PublicCombatGroundedProposalStepV1,
        provenance: TaskProposalProvenanceV1,
    ) -> Result<TaskActorIntentV1, String> {
        if provenance.role != TaskProposalRoleV1::StructuredPlan {
            return Err("combat plan follower requires structured-plan provenance".to_string());
        }
        let prefix_proposal = TaskPrefixProposalV1::new_public(
            view,
            decision,
            step.choice.clone(),
            TaskProposalExplorationAuthorizationV1::CommitOnly,
            provenance,
        )?;
        self.events
            .push(PublicCombatPlanFollowerEventV1::StepIssued {
                proposal: proposal.clone(),
                step: step.clone(),
                prefix_proposal,
            });
        let choice = step.choice.clone();
        let scope = view
            .node_memory
            .as_ref()
            .ok_or_else(|| "combat plan follower requires current node memory".to_string())?
            .scope;
        self.active = Some(ActivePublicCombatPlanV1 {
            scope,
            proposal,
            source_decision: decision.clone(),
            issued_step: step,
        });
        Ok(TaskActorIntentV1::Act { choice })
    }
}

impl<G, F> TaskActorV1 for TaskPublicCombatPlanFollowerActorV1<G, F>
where
    G: PublicCombatPlanGeneratorV1,
    F: TaskActorV1,
{
    fn identity(&self) -> TaskActorIdentityV1 {
        self.identity.clone()
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        if let Some(active) = self.active.take() {
            let observed_scope = view.node_memory.as_ref().map(|memory| memory.scope);
            if observed_scope != Some(active.scope) {
                self.events
                    .push(PublicCombatPlanFollowerEventV1::InterruptedAtScopeChange {
                        proposal: active.proposal,
                        issued_step: active.issued_step,
                        source_scope: active.scope,
                        observed_scope,
                    });
                return self.fallback.next_intent(view);
            }
            match &view.boundary {
                TaskActorBoundaryV1::CommitReady => {
                    self.events
                        .push(PublicCombatPlanFollowerEventV1::InterruptedAtCommitReady {
                            proposal: active.proposal,
                            issued_step: active.issued_step,
                        });
                    return self.fallback.next_intent(view);
                }
                TaskActorBoundaryV1::ExploredScopeExit { .. } => {
                    return self.fallback.next_intent(view)
                }
                TaskActorBoundaryV1::Decision { decision } => {
                    let advanced = match active
                        .proposal
                        .advance_after_grounded_v1(&active.source_decision, &active.issued_step)
                    {
                        Ok(advanced) => advanced,
                        Err(detail) => {
                            self.events
                                .push(PublicCombatPlanFollowerEventV1::AdvanceFailed {
                                    proposal: active.proposal,
                                    detail,
                                });
                            return self.fallback.next_intent(view);
                        }
                    };
                    let grounding = advanced.ground_v1(decision);
                    self.events
                        .push(PublicCombatPlanFollowerEventV1::SuccessorGrounded {
                            proposal: advanced.clone(),
                            grounding: grounding.clone(),
                        });
                    if let PublicCombatProposalGroundingV1::Grounded { step } = grounding {
                        let provenance = self.generator.provenance();
                        return self
                            .issue_step(view, decision, advanced, step, provenance)
                            .unwrap_or_else(Self::unsupported);
                    }
                    return self.fallback.next_intent(view);
                }
                TaskActorBoundaryV1::Terminal { .. } => {
                    return Self::unsupported("task runner invoked combat plan after terminal")
                }
            }
        }

        let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return self.fallback.next_intent(view);
        };
        if !matches!(decision.observation, TaskPublicObservationV1::Combat { .. }) {
            return self.fallback.next_intent(view);
        }
        let generation = self.generator.generate(decision);
        let selected = generation
            .unique_grounded()
            .map(|(proposal, step)| (proposal.clone(), step.clone()));
        self.events
            .push(PublicCombatPlanFollowerEventV1::Generated { generation });
        let Some((proposal, step)) = selected else {
            return self.fallback.next_intent(view);
        };
        let provenance = self.generator.provenance();
        self.issue_step(view, decision, proposal, step, provenance)
            .unwrap_or_else(Self::unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::information::action::project_public_combat_actions_v1;
    use crate::agent::information::effect::PublicCombatCandidateEffectV1;
    use crate::agent::information::run::{PublicCombatRunContextGapV1, PublicCombatRunContextV1};
    use crate::agent::information::state::public_combat_state_v1;
    use crate::agent::task::{
        task_public_combat_decision_v1, CommittedPublicHistoryV1, LegalKnowledgeLedgerV1,
        NoReloadReasonV1, NodeMemoryV1, ReloadPermissionV1, TaskBudgetV1, TaskObservedBoundaryV1,
        TaskProposalRoleV1, TaskPublicChoiceV1, TaskPublicStepV1, TaskScopeOrdinalV1, TaskSpecV1,
    };
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::{store, PowerId};
    use crate::runtime::combat::{CombatCard, Power, PowerPayload};
    use crate::state::EngineState;

    fn combat_decision(
        cards: &[CardId],
        monster_power: Option<(PowerId, i32)>,
    ) -> TaskPublicDecisionV1 {
        combat_decision_with_monsters(cards, monster_power, 1)
    }

    fn combat_decision_with_monsters(
        cards: &[CardId],
        monster_power: Option<(PowerId, i32)>,
        monster_count: usize,
    ) -> TaskPublicDecisionV1 {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = cards
            .iter()
            .enumerate()
            .map(|(index, card)| CombatCard::for_scenario(*card, 100 + index as u32))
            .collect();
        combat.entities.monsters = (0..monster_count)
            .map(|index| {
                let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
                monster.id = 77 + index;
                monster.slot = 4 + index as u8;
                monster
            })
            .collect();
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
        task_public_combat_decision_v1(
            PublicCombatRunContextV1::Unavailable {
                reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
            },
            public_combat_state_v1(&combat),
            &actions.public,
        )
        .expect("combat decision")
    }

    fn view_at_root(decision: TaskPublicDecisionV1) -> TaskActorViewV1 {
        TaskActorViewV1 {
            task: TaskSpecV1::ironclad_a0_act_three_fail_closed(),
            budget: TaskBudgetV1 {
                max_world_transitions: 10,
                max_reloads: 0,
            }
            .into(),
            committed_history: CommittedPublicHistoryV1::default(),
            legal_knowledge: LegalKnowledgeLedgerV1::default(),
            node_memory: Some(NodeMemoryV1::new(TaskScopeOrdinalV1(0), decision.clone())),
            reload_permission: Some(ReloadPermissionV1::NoReload {
                reason: NoReloadReasonV1::UnverifiedSimulatorParity,
            }),
            boundary: TaskActorBoundaryV1::Decision { decision },
        }
    }

    fn view_after_step(
        source: TaskPublicDecisionV1,
        choice: TaskPublicChoiceV1,
        decision: TaskPublicDecisionV1,
    ) -> TaskActorViewV1 {
        let mut view = view_at_root(source.clone());
        let memory = view.node_memory.as_mut().expect("node memory");
        memory.current_mut().steps.push(TaskPublicStepV1 {
            source,
            choice,
            observed_boundary: TaskObservedBoundaryV1::Decision {
                decision: decision.clone(),
            },
        });
        view.boundary = TaskActorBoundaryV1::Decision { decision };
        view
    }

    fn chosen_card(decision: &TaskPublicDecisionV1, intent: &TaskActorIntentV1) -> CardId {
        let TaskActorIntentV1::Act { choice } = intent else {
            panic!("expected actor choice, got {intent:?}")
        };
        let effect = decision
            .combat_candidate_effect_v1(choice.ordinal_path[0])
            .expect("chosen public effect");
        let PublicCombatCandidateEffectV1::PlayCard { card } = effect.effect.effect else {
            panic!("expected a card choice")
        };
        card.card_id
    }

    #[derive(Clone)]
    struct FixedFallback {
        choice: TaskPublicChoiceV1,
    }

    impl TaskActorV1 for FixedFallback {
        fn identity(&self) -> TaskActorIdentityV1 {
            TaskActorIdentityV1::task_actor_view_only("fixed-combat-plan-test-fallback", 1)
        }

        fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
            match view.boundary {
                TaskActorBoundaryV1::CommitReady
                | TaskActorBoundaryV1::ExploredScopeExit { .. } => TaskActorIntentV1::CommitScope,
                _ => TaskActorIntentV1::Act {
                    choice: self.choice.clone(),
                },
            }
        }
    }

    fn follower(
        fallback_choice: TaskPublicChoiceV1,
    ) -> TaskPublicCombatPlanFollowerActorV1<VulnerableSetupPayoffPlanGeneratorV1, FixedFallback>
    {
        TaskPublicCombatPlanFollowerActorV1::new(
            TaskActorIdentityV1::task_actor_view_only("public-combat-plan-test-follower", 1),
            VulnerableSetupPayoffPlanGeneratorV1,
            FixedFallback {
                choice: fallback_choice,
            },
        )
    }

    #[test]
    fn unique_public_plan_issues_commit_only_bash_then_grounds_strike_at_the_successor() {
        let setup_decision = combat_decision(&[CardId::Bash, CardId::Strike], None);
        let fallback_choice = TaskPublicChoiceV1::one(setup_decision.candidates.len() - 1);
        let mut actor = follower(fallback_choice);
        let setup_intent = actor.next_intent(&view_at_root(setup_decision.clone()));
        assert_eq!(chosen_card(&setup_decision, &setup_intent), CardId::Bash);
        let TaskActorIntentV1::Act {
            choice: setup_choice,
        } = setup_intent
        else {
            unreachable!()
        };

        let payoff_decision = combat_decision(&[CardId::Strike], Some((PowerId::Vulnerable, 2)));
        let payoff_intent = actor.next_intent(&view_after_step(
            setup_decision,
            setup_choice,
            payoff_decision.clone(),
        ));
        assert_eq!(
            chosen_card(&payoff_decision, &payoff_intent),
            CardId::Strike
        );

        let issued = actor
            .events()
            .iter()
            .filter_map(|event| match event {
                PublicCombatPlanFollowerEventV1::StepIssued {
                    prefix_proposal, ..
                } => Some(prefix_proposal),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(issued.len(), 2);
        assert!(issued.iter().all(|proposal| {
            proposal.exploration_authorization == TaskProposalExplorationAuthorizationV1::CommitOnly
                && proposal.provenance.role == TaskProposalRoleV1::StructuredPlan
        }));
    }

    #[test]
    fn ambiguity_unknown_coverage_and_public_inapplicability_delegate_without_ranking() {
        for (decision, expected) in [
            (
                combat_decision(&[CardId::Bash, CardId::Bash, CardId::Strike], None),
                "ambiguous",
            ),
            (
                combat_decision(&[CardId::Bash, CardId::Strike, CardId::Anger], None),
                "unknown",
            ),
            (
                combat_decision(
                    &[CardId::Bash, CardId::Strike],
                    Some((PowerId::Artifact, 1)),
                ),
                "inapplicable",
            ),
        ] {
            let fallback_choice = TaskPublicChoiceV1::one(decision.candidates.len() - 1);
            let mut actor = follower(fallback_choice.clone());
            assert_eq!(
                actor.next_intent(&view_at_root(decision)),
                TaskActorIntentV1::Act {
                    choice: fallback_choice
                }
            );
            let PublicCombatPlanFollowerEventV1::Generated { generation } = &actor.events()[0]
            else {
                panic!("expected typed generation event")
            };
            assert!(matches!(
                (expected, generation),
                ("ambiguous", PublicCombatPlanGenerationV1::Ambiguous { .. })
                    | (
                        "unknown",
                        PublicCombatPlanGenerationV1::UnknownCoverage { .. }
                    )
                    | (
                        "inapplicable",
                        PublicCombatPlanGenerationV1::Inapplicable { .. }
                    )
            ));
        }
    }

    #[test]
    fn multiple_grounded_targets_are_ambiguous_instead_of_using_monster_order() {
        let decision = combat_decision_with_monsters(&[CardId::Bash, CardId::Strike], None, 2);
        let fallback_choice = TaskPublicChoiceV1::one(decision.candidates.len() - 1);
        let mut actor = follower(fallback_choice.clone());

        assert_eq!(
            actor.next_intent(&view_at_root(decision)),
            TaskActorIntentV1::Act {
                choice: fallback_choice
            }
        );
        assert!(matches!(
            actor.events().first(),
            Some(PublicCombatPlanFollowerEventV1::Generated {
                generation: PublicCombatPlanGenerationV1::Ambiguous { attempts, .. }
            }) if attempts.len() == 2
        ));
    }

    #[test]
    fn commit_ready_after_setup_does_not_fabricate_a_payoff_step() {
        let decision = combat_decision(&[CardId::Bash, CardId::Strike], None);
        let mut actor = follower(TaskPublicChoiceV1::one(0));
        assert_eq!(
            chosen_card(
                &decision,
                &actor.next_intent(&view_at_root(decision.clone()))
            ),
            CardId::Bash
        );
        let mut commit_ready = view_at_root(decision);
        commit_ready.boundary = TaskActorBoundaryV1::CommitReady;
        assert_eq!(
            actor.next_intent(&commit_ready),
            TaskActorIntentV1::CommitScope
        );
        assert!(matches!(
            actor.events().last(),
            Some(PublicCombatPlanFollowerEventV1::InterruptedAtCommitReady { .. })
        ));
        assert_eq!(
            actor
                .events()
                .iter()
                .filter(|event| matches!(event, PublicCombatPlanFollowerEventV1::StepIssued { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn active_plan_never_crosses_into_another_native_scope() {
        let setup_decision = combat_decision(&[CardId::Bash, CardId::Strike], None);
        let mut actor = follower(TaskPublicChoiceV1::one(0));
        let _ = actor.next_intent(&view_at_root(setup_decision));

        let next_scope_decision =
            combat_decision(&[CardId::Strike], Some((PowerId::Vulnerable, 2)));
        let fallback_choice = TaskPublicChoiceV1::one(next_scope_decision.candidates.len() - 1);
        actor.fallback.choice = fallback_choice.clone();
        let mut next_scope = view_at_root(next_scope_decision);
        next_scope.node_memory.as_mut().expect("node memory").scope = TaskScopeOrdinalV1(1);

        assert_eq!(
            actor.next_intent(&next_scope),
            TaskActorIntentV1::Act {
                choice: fallback_choice
            }
        );
        assert!(matches!(
            actor.events().last(),
            Some(PublicCombatPlanFollowerEventV1::InterruptedAtScopeChange {
                source_scope: TaskScopeOrdinalV1(0),
                observed_scope: Some(TaskScopeOrdinalV1(1)),
                ..
            })
        ));
    }
}
