//! Deterministic legality bootstrap over the model-visible learning surface.
//!
//! This owner is deliberately downstream from `LearningModelDecisionV1`: it
//! cannot access exact RNG state, hidden draw order, card lineages, or exact
//! successor states.  Rules are a bootstrap behavior, not teacher truth.

use crate::agent::information::action::{
    PublicCombatAtomicActionV1, PublicCombatIndexedChoiceCandidateV1,
    PublicCombatSelectionDomainCandidateV1,
};
use crate::agent::information::state::{
    CombatLearningCardZonesV1, CombatLearningEncounterV1, CombatLearningMonsterStateV1,
    CombatLearningPlayerStateV1, CombatLearningPotionV1, CombatLearningTurnV1,
};
use crate::agent::task::{
    TaskActorBoundaryV1, TaskActorIdentityV1, TaskActorIntentV1, TaskActorV1, TaskActorViewV1,
    TaskObservedBoundaryV1, TaskPublicCandidateV1, TaskPublicChoiceV1,
    TaskPublicCombatPlanFollowerActorV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskStrategicActionV1, TaskStrategicObservationV1, VulnerableSetupPayoffPlanGeneratorV1,
};
use crate::ai::planner_core::{
    PlannerDecisionSite, PlannerMapRoom, PlannerPlayerClass, PlannerPublicMap, PlannerRunGoal,
};
use crate::ai::strategy::campfire_upgrade_quality::{rank_campfire_upgrades, CampfireUpgradeTier};
use crate::ai::strategy::decision_pipeline::{
    evaluate_decision_candidate, CandidateEvaluation, CandidateOrderKey, CleanupTarget,
    DecisionCandidateKind, DecisionPipelineContext,
};
use crate::ai::strategy::deck_admission::DeckAdmissionContext;
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::reward_admission::{
    assess_reward_admission_from_master_deck, skip_reward_admission,
};
use crate::ai::strategy::run_strategic_facts::RunStrategicFacts;
use crate::content::cards::{
    get_card_definition, is_starter_basic, CardId, CardRarity, CardTag, CardType,
};
use crate::content::potions::{
    get_potion_definition, potion_mechanic_roles, PotionId, PotionMechanicRole,
};
use crate::content::powers::PowerId;
use crate::content::relics::{get_relic_tier, RelicId, RelicTier};
use crate::runtime::combat::{CombatCard, Intent};
use crate::sim::combat_action_surface::CombatSelectionReasonV2;
use crate::state::core::{GridSelectReason, HandSelectReason};
use crate::state::events::EventActionKind;
use crate::state::selection::SelectionReason;

use super::public_policy_candidate_admission::{
    admitted_strategic_candidate_ordinals_v1, public_combat_atomic_candidate_allowed_v1,
    PublicStrategicAdmissionKindV1,
};
use super::{
    LearningCombatAtomicActionV1, LearningCombatModelObservationV1,
    LearningCombatSelectionDomainSemanticsV1, LearningModelCandidateSemanticsV1,
    LearningModelDecisionV1, LearningModelObservationV1, LearningSelectionCandidateSemanticsV1,
    LearningSelectionDraftV1, LearningStrategicActionSemanticsV2,
    LearningStrategicModelObservationV1,
};

#[path = "public_rule_search_actor.rs"]
mod rule_search;
pub use rule_search::{
    public_rule_search_task_actor_v1, PublicRuleSearchStatsV1, PublicRuleSearchTaskActorV1,
    PUBLIC_RULE_SEARCH_TASK_ACTOR_NAME_V1, PUBLIC_RULE_SEARCH_TASK_ACTOR_REVISION_V1,
};

pub const PUBLIC_DETERMINISTIC_BASELINE_NAME_V1: &str = "public_legality_bootstrap";
pub const PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1: u32 = 29;
pub const PUBLIC_DETERMINISTIC_TASK_ACTOR_NAME_V1: &str = "public_legality_bootstrap_task_actor";
pub const PUBLIC_STRUCTURED_DETERMINISTIC_TASK_ACTOR_NAME_V1: &str =
    "public_structured_legality_bootstrap_task_actor";
pub const PUBLIC_STRUCTURED_DETERMINISTIC_TASK_ACTOR_REVISION_V1: u32 = 13;
pub const PUBLIC_DETERMINISTIC_BASELINE_MANIFEST_SCHEMA_V1: &str =
    "sts-public-deterministic-baseline-manifest-v1";

pub const BASELINE_LIMIT_NO_BELIEF_V1: &str = "PDB-001-no-hidden-state-belief";
pub const BASELINE_LIMIT_SHORT_HORIZON_V1: &str = "PDB-002-short-horizon-strategy";
pub const BASELINE_LIMIT_COARSE_CONTENT_V1: &str = "PDB-003-coarse-content-values";
pub const BASELINE_LIMIT_POTION_OPPORTUNITY_V1: &str = "PDB-004-myopic-potion-opportunity-cost";
pub const BASELINE_LIMIT_SELECTION_V1: &str = "PDB-005-generic-symbolic-selection";
pub const BASELINE_LIMIT_EVENT_RISK_V1: &str = "PDB-006-incomplete-event-risk-models";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicBaselineLimitationV1 {
    pub id: &'static str,
    pub title: &'static str,
    pub consequence: &'static str,
}

const PUBLIC_BASELINE_LIMITATIONS_V1: &[PublicBaselineLimitationV1] = &[
    PublicBaselineLimitationV1 {
        id: BASELINE_LIMIT_NO_BELIEF_V1,
        title: "no hidden-state belief",
        consequence:
            "acts on the current public view without marginalizing hidden draw or RNG futures",
    },
    PublicBaselineLimitationV1 {
        id: BASELINE_LIMIT_SHORT_HORIZON_V1,
        title: "short-horizon strategy",
        consequence: "route and resource rules do not estimate complete-run continuation value",
    },
    PublicBaselineLimitationV1 {
        id: BASELINE_LIMIT_COARSE_CONTENT_V1,
        title: "coarse content values",
        consequence: "card, relic, potion, and encounter-conditioned synergies are incomplete",
    },
    PublicBaselineLimitationV1 {
        id: BASELINE_LIMIT_POTION_OPPORTUNITY_V1,
        title: "myopic potion opportunity cost",
        consequence:
            "combat spend choices do not compare future-route survival and replacement value",
    },
    PublicBaselineLimitationV1 {
        id: BASELINE_LIMIT_SELECTION_V1,
        title: "generic symbolic selection",
        consequence: "multi-card selections use local card scores rather than sequence-aware plans",
    },
    PublicBaselineLimitationV1 {
        id: BASELINE_LIMIT_EVENT_RISK_V1,
        title: "incomplete event risk models",
        consequence:
            "public consequences are represented, but stochastic event continuation is incomplete",
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicDeterministicBaselineManifestV1 {
    pub schema: &'static str,
    pub name: &'static str,
    pub revision: u32,
    pub lifecycle: &'static str,
    pub role: &'static str,
    pub information_access: PublicBaselineInformationAccessV1,
    pub deterministic: bool,
    pub same_public_history_same_action: bool,
    pub clear_defect_policy: &'static str,
    pub ambiguous_weakness_policy: &'static str,
    pub prohibited_change: &'static str,
    pub capabilities: &'static [PublicBaselineCapabilityV1],
    pub limitations: &'static [PublicBaselineLimitationV1],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicBaselineInformationAccessV1 {
    PublicSemanticViewsOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicBaselineCoverageV1 {
    SpecializedHeuristic,
    GenericHeuristic,
    MechanicalFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicBaselineCapabilityV1 {
    pub domain: &'static str,
    pub owner: &'static str,
    pub information_access: PublicBaselineInformationAccessV1,
    pub deterministic: bool,
    pub coverage: PublicBaselineCoverageV1,
    pub fallback: &'static str,
    pub limitation_ids: &'static [&'static str],
}

const STRATEGIC_LIMITS: &[&str] = &[
    BASELINE_LIMIT_SHORT_HORIZON_V1,
    BASELINE_LIMIT_COARSE_CONTENT_V1,
];
const COMBAT_LIMITS: &[&str] = &[
    BASELINE_LIMIT_NO_BELIEF_V1,
    BASELINE_LIMIT_COARSE_CONTENT_V1,
    BASELINE_LIMIT_POTION_OPPORTUNITY_V1,
];
const SELECTION_LIMITS: &[&str] = &[
    BASELINE_LIMIT_COARSE_CONTENT_V1,
    BASELINE_LIMIT_SELECTION_V1,
];
const EVENT_LIMITS: &[&str] = &[
    BASELINE_LIMIT_SHORT_HORIZON_V1,
    BASELINE_LIMIT_COARSE_CONTENT_V1,
    BASELINE_LIMIT_EVENT_RISK_V1,
];

const PUBLIC_BASELINE_CAPABILITIES_V1: &[PublicBaselineCapabilityV1] = &[
    PublicBaselineCapabilityV1 {
        domain: "strategic.map",
        owner: PUBLIC_DETERMINISTIC_BASELINE_NAME_V1,
        information_access: PublicBaselineInformationAccessV1::PublicSemanticViewsOnly,
        deterministic: true,
        coverage: PublicBaselineCoverageV1::SpecializedHeuristic,
        fallback: "stable candidate ordinal",
        limitation_ids: STRATEGIC_LIMITS,
    },
    PublicBaselineCapabilityV1 {
        domain: "strategic.card_reward",
        owner: PUBLIC_DETERMINISTIC_BASELINE_NAME_V1,
        information_access: PublicBaselineInformationAccessV1::PublicSemanticViewsOnly,
        deterministic: true,
        coverage: PublicBaselineCoverageV1::SpecializedHeuristic,
        fallback: "skip low-value card",
        limitation_ids: STRATEGIC_LIMITS,
    },
    PublicBaselineCapabilityV1 {
        domain: "strategic.shop_campfire_boss_relic",
        owner: PUBLIC_DETERMINISTIC_BASELINE_NAME_V1,
        information_access: PublicBaselineInformationAccessV1::PublicSemanticViewsOnly,
        deterministic: true,
        coverage: PublicBaselineCoverageV1::SpecializedHeuristic,
        fallback: "leave or stable candidate ordinal",
        limitation_ids: STRATEGIC_LIMITS,
    },
    PublicBaselineCapabilityV1 {
        domain: "strategic.event",
        owner: PUBLIC_DETERMINISTIC_BASELINE_NAME_V1,
        information_access: PublicBaselineInformationAccessV1::PublicSemanticViewsOnly,
        deterministic: true,
        coverage: PublicBaselineCoverageV1::GenericHeuristic,
        fallback: "prefer public safe exit when event risk is not modeled",
        limitation_ids: EVENT_LIMITS,
    },
    PublicBaselineCapabilityV1 {
        domain: "strategic.neow_reward_run_choice_treasure",
        owner: PUBLIC_DETERMINISTIC_BASELINE_NAME_V1,
        information_access: PublicBaselineInformationAccessV1::PublicSemanticViewsOnly,
        deterministic: true,
        coverage: PublicBaselineCoverageV1::GenericHeuristic,
        fallback: "stable candidate ordinal",
        limitation_ids: STRATEGIC_LIMITS,
    },
    PublicBaselineCapabilityV1 {
        domain: "combat.atomic_and_indexed",
        owner: PUBLIC_DETERMINISTIC_BASELINE_NAME_V1,
        information_access: PublicBaselineInformationAccessV1::PublicSemanticViewsOnly,
        deterministic: true,
        coverage: PublicBaselineCoverageV1::GenericHeuristic,
        fallback: "legal action with stable ordinal tie-break",
        limitation_ids: COMBAT_LIMITS,
    },
    PublicBaselineCapabilityV1 {
        domain: "strategic_and_combat.symbolic_selection",
        owner: PUBLIC_DETERMINISTIC_BASELINE_NAME_V1,
        information_access: PublicBaselineInformationAccessV1::PublicSemanticViewsOnly,
        deterministic: true,
        coverage: PublicBaselineCoverageV1::GenericHeuristic,
        fallback: "minimum legal payload then submit",
        limitation_ids: SELECTION_LIMITS,
    },
];

pub fn public_deterministic_baseline_capabilities_v1() -> &'static [PublicBaselineCapabilityV1] {
    PUBLIC_BASELINE_CAPABILITIES_V1
}

pub fn public_deterministic_baseline_manifest_v1() -> PublicDeterministicBaselineManifestV1 {
    PublicDeterministicBaselineManifestV1 {
        schema: PUBLIC_DETERMINISTIC_BASELINE_MANIFEST_SCHEMA_V1,
        name: PUBLIC_DETERMINISTIC_BASELINE_NAME_V1,
        revision: PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
        lifecycle: "experimental",
        role: "legality_bootstrap_not_capability_baseline_or_teacher",
        information_access: PublicBaselineInformationAccessV1::PublicSemanticViewsOnly,
        deterministic: true,
        same_public_history_same_action: true,
        clear_defect_policy: "fix_and_increment_behavior_revision",
        ambiguous_weakness_policy: "measure_and_iterate_before_capability_admission",
        prohibited_change: "do_not_add_private_state_or_privileged_search",
        capabilities: PUBLIC_BASELINE_CAPABILITIES_V1,
        limitations: PUBLIC_BASELINE_LIMITATIONS_V1,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicDeterministicBaselineDecisionV1 {
    pub candidate_ordinal: usize,
    pub revision: u32,
    pub rule_family: &'static str,
    pub coverage: PublicBaselineCoverageV1,
    pub limitation_ids: &'static [&'static str],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicDeterministicTaskDecisionV1 {
    pub public_snapshot_id: String,
    pub choice: TaskPublicChoiceV1,
    pub revision: u32,
    pub rule_family: &'static str,
    pub coverage: PublicBaselineCoverageV1,
    pub limitation_ids: &'static [&'static str],
}

#[derive(Clone, Debug, Default)]
pub struct PublicDeterministicTaskActorV1 {
    decisions: Vec<PublicDeterministicTaskDecisionV1>,
}

impl PublicDeterministicTaskActorV1 {
    pub fn decisions(&self) -> &[PublicDeterministicTaskDecisionV1] {
        &self.decisions
    }
}

impl TaskActorV1 for PublicDeterministicTaskActorV1 {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only(
            PUBLIC_DETERMINISTIC_TASK_ACTOR_NAME_V1,
            PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
        )
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        match &view.boundary {
            TaskActorBoundaryV1::Decision { decision } => {
                match public_deterministic_task_actor_decision_v1(view, decision) {
                    Ok(selected) => {
                        let choice = selected.choice.clone();
                        self.decisions.push(selected);
                        TaskActorIntentV1::Act { choice }
                    }
                    Err(reason) => TaskActorIntentV1::UnsupportedDecisionSurface { reason },
                }
            }
            TaskActorBoundaryV1::CommitReady | TaskActorBoundaryV1::ExploredScopeExit { .. } => {
                TaskActorIntentV1::CommitScope
            }
            TaskActorBoundaryV1::Terminal { .. } => TaskActorIntentV1::UnsupportedDecisionSurface {
                reason: "task runner invoked deterministic actor after terminal".to_string(),
            },
        }
    }
}

fn public_deterministic_task_actor_decision_v1(
    view: &TaskActorViewV1,
    decision: &TaskPublicDecisionV1,
) -> Result<PublicDeterministicTaskDecisionV1, String> {
    if let Some(candidate_ordinal) = proceeded_reward_map_route_ordinal_v1(view, decision) {
        let choice = complete_task_choice_v1(decision, candidate_ordinal)?;
        decision.validate_choice(&choice)?;
        return Ok(PublicDeterministicTaskDecisionV1 {
            public_snapshot_id: decision.information.snapshot_id.clone(),
            choice,
            revision: PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
            rule_family: "strategic.honor_reward_proceed_on_map_overlay",
            coverage: PublicBaselineCoverageV1::SpecializedHeuristic,
            limitation_ids: STRATEGIC_LIMITS,
        });
    }
    if let Some(candidate_ordinal) = skipped_card_reward_exit_ordinal_v1(view, decision) {
        let choice = complete_task_choice_v1(decision, candidate_ordinal)?;
        decision.validate_choice(&choice)?;
        return Ok(PublicDeterministicTaskDecisionV1 {
            public_snapshot_id: decision.information.snapshot_id.clone(),
            choice,
            revision: PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
            rule_family: "strategic.exit_skipped_card_reward",
            coverage: PublicBaselineCoverageV1::SpecializedHeuristic,
            limitation_ids: STRATEGIC_LIMITS,
        });
    }
    public_deterministic_task_decision_v1(decision)
}

fn proceeded_reward_map_route_ordinal_v1(
    view: &TaskActorViewV1,
    decision: &TaskPublicDecisionV1,
) -> Option<usize> {
    // A chosen Reward/Proceed means the actor has decided to leave that screen.
    // Picking Map/Cancel immediately afterward undoes the choice and can loop.
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return None;
    };
    if !matches!(
        observation.context,
        crate::ai::planner_core::PlannerDecisionContext::Map { overlay: true }
    ) {
        return None;
    }
    let previous = view
        .node_memory
        .as_ref()
        .and_then(|memory| memory.attempts.last())
        .and_then(|attempt| attempt.steps.last())
        .or_else(|| {
            view.committed_history
                .steps
                .last()
                .map(|committed| &committed.step)
        })?;
    let TaskPublicObservationV1::Strategic {
        observation: previous_observation,
    } = &previous.source.observation
    else {
        return None;
    };
    if !matches!(
        &previous_observation.context,
        crate::ai::planner_core::PlannerDecisionContext::Reward { .. }
    ) {
        return None;
    }
    let previous_ordinal = *previous.choice.ordinal_path.first()?;
    if !matches!(
        previous.source.candidates.get(previous_ordinal),
        Some(TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Proceed {
                site: PlannerDecisionSite::Reward,
            },
        })
    ) {
        return None;
    }
    let baseline_view = StrategicBaselineViewV1::from_task(observation);
    let actions = decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| {
            let TaskPublicCandidateV1::Strategic {
                action: action @ TaskStrategicActionV1::ChooseRouteNode { .. },
            } = candidate
            else {
                return None;
            };
            Some((ordinal, action.clone()))
        })
        .collect::<Vec<_>>();
    reward_exit_route_ordinal_v1(&baseline_view, &actions)
}

fn reward_exit_route_ordinal_v1(
    view: &StrategicBaselineViewV1<'_>,
    actions: &[(usize, TaskStrategicActionV1)],
) -> Option<usize> {
    let goal_routes = heart_goal_route_ordinals(view, actions);
    actions
        .iter()
        .filter(|(_, action)| matches!(action, TaskStrategicActionV1::ChooseRouteNode { .. }))
        .filter(|(ordinal, _)| {
            goal_routes
                .as_ref()
                .is_none_or(|routes| routes.contains(ordinal))
        })
        .map(|(ordinal, action)| (*ordinal, strategic_action_score(view, action)))
        .max_by(|(left_ordinal, left_score), (right_ordinal, right_score)| {
            left_score
                .cmp(right_score)
                .then_with(|| right_ordinal.cmp(left_ordinal))
        })
        .map(|(ordinal, _)| ordinal)
}

/// Public caller memory says Reward/Proceed was committed. Use the same route
/// preference as the task actor, without reversing that already-made choice.
pub fn public_deterministic_baseline_after_reward_proceed_v1(
    decision: &LearningModelDecisionV1<'_>,
) -> PublicDeterministicBaselineDecisionV1 {
    let LearningModelObservationV1::Strategic(observation) = decision.observation else {
        return public_deterministic_baseline_decision_v1(decision);
    };
    if !matches!(
        observation.context,
        crate::ai::planner_core::PlannerDecisionContext::Map { overlay: true }
    ) {
        return public_deterministic_baseline_decision_v1(decision);
    }
    let view = StrategicBaselineViewV1::from_learning(observation);
    let actions = decision
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| match candidate.semantics {
            LearningModelCandidateSemanticsV1::Strategic { action } => {
                action.semantics().ok().map(|action| (ordinal, action))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some(candidate_ordinal) = reward_exit_route_ordinal_v1(&view, &actions) else {
        return public_deterministic_baseline_decision_v1(decision);
    };
    PublicDeterministicBaselineDecisionV1 {
        candidate_ordinal,
        revision: PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
        rule_family: "strategic.honor_reward_proceed_on_map_overlay",
        coverage: PublicBaselineCoverageV1::SpecializedHeuristic,
        limitation_ids: STRATEGIC_LIMITS,
    }
}

fn skipped_card_reward_exit_ordinal_v1(
    view: &TaskActorViewV1,
    decision: &TaskPublicDecisionV1,
) -> Option<usize> {
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return None;
    };
    if observation.decision_site != PlannerDecisionSite::Reward {
        return None;
    }

    let previous = view
        .node_memory
        .as_ref()
        .and_then(|memory| memory.attempts.last())
        .and_then(|attempt| attempt.steps.last())
        .or_else(|| {
            view.committed_history
                .steps
                .last()
                .map(|committed| &committed.step)
        })?;
    let TaskObservedBoundaryV1::Decision { decision: observed } = &previous.observed_boundary
    else {
        return None;
    };
    if observed != decision {
        return None;
    }
    let previous_ordinal = *previous.choice.ordinal_path.first()?;
    let TaskPublicCandidateV1::Strategic {
        action: TaskStrategicActionV1::SkipCardReward { reward_item_index },
    } = previous.source.candidates.get(previous_ordinal)?
    else {
        return None;
    };
    let same_card_reward_remains = decision.candidates.iter().any(|candidate| {
        matches!(
            candidate,
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::OpenCardReward {
                    reward_item_index: candidate_reward_item_index,
                },
            } if candidate_reward_item_index == reward_item_index
        )
    });
    if !same_card_reward_remains {
        return None;
    }
    if decision.candidates.iter().any(|candidate| match candidate {
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::ClaimReward { .. },
        } => true,
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::OpenCardReward {
                    reward_item_index: candidate_reward_item_index,
                },
        } => candidate_reward_item_index != reward_item_index,
        _ => false,
    }) {
        return None;
    }

    decision.candidates.iter().position(|candidate| {
        matches!(
            candidate,
            TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::Proceed {
                    site: PlannerDecisionSite::Reward,
                } | TaskStrategicActionV1::Cancel {
                    site: PlannerDecisionSite::Reward,
                },
            }
        )
    })
}

/// Coverage-only composition: one narrow structured combat override backed by
/// the deterministic legality bootstrap on every represented task decision.
/// It is not a combat continuation, incumbent, or capability reference actor.
pub type PublicStructuredDeterministicTaskActorV1 = TaskPublicCombatPlanFollowerActorV1<
    VulnerableSetupPayoffPlanGeneratorV1,
    PublicDeterministicTaskActorV1,
>;

pub fn public_structured_deterministic_task_actor_v1() -> PublicStructuredDeterministicTaskActorV1 {
    TaskPublicCombatPlanFollowerActorV1::new(
        TaskActorIdentityV1::task_actor_view_only(
            PUBLIC_STRUCTURED_DETERMINISTIC_TASK_ACTOR_NAME_V1,
            PUBLIC_STRUCTURED_DETERMINISTIC_TASK_ACTOR_REVISION_V1,
        ),
        VulnerableSetupPayoffPlanGeneratorV1,
        PublicDeterministicTaskActorV1::default(),
    )
}

#[derive(Clone)]
struct StrategicBaselineViewV1<'a> {
    run_goal: PlannerRunGoal,
    decision_site: PlannerDecisionSite,
    run: &'a crate::ai::planner_core::PlannerRunScalars,
    public_map: &'a PlannerPublicMap,
    context: &'a crate::ai::planner_core::PlannerDecisionContext,
    public_history: &'a crate::ai::planner_core::PlannerPublicHistory,
    deck: Vec<CombatCard>,
    deck_plan: DeckPlanSnapshot,
    base_cost_randomized_on_draw: bool,
    can_recover_at_rest: bool,
    occupied_potions: usize,
}

impl<'a> StrategicBaselineViewV1<'a> {
    fn from_learning(observation: LearningStrategicModelObservationV1<'a>) -> Self {
        let deck = observation
            .cards()
            .map(|card| {
                public_combat_card(
                    card.ordinal(),
                    card.card(),
                    card.upgrades(),
                    card.misc_value(),
                    card.base_damage_override(),
                    card.base_block_override(),
                    card.cost_modifier(),
                )
            })
            .collect::<Vec<_>>();
        let relics = observation
            .relics
            .iter()
            .map(|relic| relic.relic)
            .collect::<Vec<_>>();
        let base_cost_randomized_on_draw = relics.contains(&RelicId::SneckoEye);
        let can_recover_at_rest =
            !relics.contains(&RelicId::CoffeeDripper) && !relics.contains(&RelicId::MarkOfTheBloom);
        let deck_plan = public_deck_plan(observation.run, observation.public_map, &deck, &relics);
        Self {
            run_goal: observation.run_goal,
            decision_site: observation.decision_site,
            run: observation.run,
            public_map: observation.public_map,
            context: observation.context,
            public_history: observation.public_history,
            deck,
            deck_plan,
            base_cost_randomized_on_draw,
            can_recover_at_rest,
            occupied_potions: observation
                .potion_slots()
                .filter(|slot| slot.potion().is_some())
                .count(),
        }
    }

    fn from_task(observation: &'a TaskStrategicObservationV1) -> Self {
        let deck = observation
            .cards
            .iter()
            .map(|card| {
                public_combat_card(
                    card.ordinal,
                    card.card,
                    card.upgrades,
                    card.misc_value,
                    card.base_damage_override,
                    card.base_block_override,
                    card.cost_modifier,
                )
            })
            .collect::<Vec<_>>();
        let relics = observation
            .relics
            .iter()
            .map(|relic| relic.relic)
            .collect::<Vec<_>>();
        let base_cost_randomized_on_draw = relics.contains(&RelicId::SneckoEye);
        let can_recover_at_rest =
            !relics.contains(&RelicId::CoffeeDripper) && !relics.contains(&RelicId::MarkOfTheBloom);
        let deck_plan = public_deck_plan(&observation.run, &observation.public_map, &deck, &relics);
        Self {
            run_goal: observation.run_goal,
            decision_site: observation.decision_site,
            run: &observation.run,
            public_map: &observation.public_map,
            context: &observation.context,
            public_history: &observation.public_history,
            deck,
            deck_plan,
            base_cost_randomized_on_draw,
            can_recover_at_rest,
            occupied_potions: observation
                .potions
                .iter()
                .filter(|slot| slot.potion.is_some())
                .count(),
        }
    }
}

fn public_combat_card(
    ordinal: usize,
    card: CardId,
    upgrades: u8,
    misc_value: i32,
    base_damage_override: Option<i32>,
    base_block_override: Option<i32>,
    cost_modifier: i8,
) -> CombatCard {
    let recipe_slot = u32::try_from(ordinal)
        .expect("public deck ordinal must fit a constructed scenario recipe slot")
        .checked_add(1)
        .expect("public deck recipe slot must not overflow");
    let mut projected = CombatCard::for_scenario(card, recipe_slot);
    projected.upgrades = upgrades;
    projected.misc_value = misc_value;
    projected.base_damage_override = base_damage_override;
    projected.base_block_override = base_block_override;
    projected.cost_modifier = cost_modifier;
    projected
}

fn public_deck_plan(
    run: &crate::ai::planner_core::PlannerRunScalars,
    public_map: &PlannerPublicMap,
    deck: &[CombatCard],
    relics: &[RelicId],
) -> DeckPlanSnapshot {
    DeckPlanSnapshot::from_deck(
        deck,
        DeckAdmissionContext {
            act: run.act,
            current_hp: run.current_hp,
            max_hp: run.max_hp,
        },
        RunStrategicFacts::from_public_state(run.act, deck, relics),
    )
    .with_boss_key(public_map.boss)
}

#[derive(Clone, Copy)]
struct CombatBaselineViewV1<'a> {
    potions: &'a [Option<CombatLearningPotionV1>],
    encounter: &'a CombatLearningEncounterV1,
    turn: &'a CombatLearningTurnV1,
    player: &'a CombatLearningPlayerStateV1,
    cards: &'a CombatLearningCardZonesV1,
    monsters: &'a [CombatLearningMonsterStateV1],
}

#[derive(Clone, Copy)]
enum CombatBaselineCandidateV1<'a> {
    PlayCard {
        hand_index: usize,
        target_monster_index: Option<usize>,
    },
    UsePotion {
        potion_index: usize,
        target_monster_index: Option<usize>,
    },
    DiscardPotion,
    EndTurn,
    SubmitIndexedChoice {
        candidate: &'a PublicCombatIndexedChoiceCandidateV1,
    },
    SelectionFamily,
    Proceed,
    Cancel,
    InvalidDomain,
}

impl<'a> CombatBaselineCandidateV1<'a> {
    fn from_learning(semantics: LearningModelCandidateSemanticsV1<'a>) -> Self {
        match semantics {
            LearningModelCandidateSemanticsV1::CombatAtomic { action } => match action {
                LearningCombatAtomicActionV1::PlayCard {
                    hand_index,
                    target_monster_index,
                } => Self::PlayCard {
                    hand_index,
                    target_monster_index,
                },
                LearningCombatAtomicActionV1::UsePotion {
                    potion_index,
                    target_monster_index,
                } => Self::UsePotion {
                    potion_index,
                    target_monster_index,
                },
                LearningCombatAtomicActionV1::DiscardPotion { .. } => Self::DiscardPotion,
                LearningCombatAtomicActionV1::EndTurn => Self::EndTurn,
                LearningCombatAtomicActionV1::SubmitIndexedChoice { indexed, .. } => {
                    Self::SubmitIndexedChoice {
                        candidate: indexed.candidate,
                    }
                }
                LearningCombatAtomicActionV1::Proceed => Self::Proceed,
                LearningCombatAtomicActionV1::Cancel => Self::Cancel,
            },
            LearningModelCandidateSemanticsV1::CombatSelectionFamily { .. } => {
                Self::SelectionFamily
            }
            LearningModelCandidateSemanticsV1::Strategic { .. } => Self::InvalidDomain,
        }
    }

    fn from_task(candidate: &'a TaskPublicCandidateV1) -> Self {
        match candidate {
            TaskPublicCandidateV1::CombatAtomic { action } => match action {
                PublicCombatAtomicActionV1::PlayCard {
                    hand_index,
                    target_monster_index,
                } => Self::PlayCard {
                    hand_index: *hand_index,
                    target_monster_index: *target_monster_index,
                },
                PublicCombatAtomicActionV1::UsePotion {
                    potion_index,
                    target_monster_index,
                } => Self::UsePotion {
                    potion_index: *potion_index,
                    target_monster_index: *target_monster_index,
                },
                PublicCombatAtomicActionV1::DiscardPotion { .. } => Self::DiscardPotion,
                PublicCombatAtomicActionV1::EndTurn => Self::EndTurn,
                PublicCombatAtomicActionV1::SubmitIndexedChoice { .. } => Self::InvalidDomain,
                PublicCombatAtomicActionV1::Proceed => Self::Proceed,
                PublicCombatAtomicActionV1::Cancel => Self::Cancel,
            },
            TaskPublicCandidateV1::CombatIndexedChoice { candidate, .. } => {
                Self::SubmitIndexedChoice { candidate }
            }
            TaskPublicCandidateV1::CombatSelectionFamily { .. } => Self::SelectionFamily,
            TaskPublicCandidateV1::Strategic { .. } => Self::InvalidDomain,
        }
    }
}

impl<'a> CombatBaselineViewV1<'a> {
    fn from_learning(observation: LearningCombatModelObservationV1<'a>) -> Self {
        Self {
            potions: observation.potions,
            encounter: observation.encounter,
            turn: observation.turn,
            player: observation.player,
            cards: observation.cards,
            monsters: observation.monsters.as_slice(),
        }
    }

    fn from_task(observation: &'a crate::agent::information::state::PublicCombatStateV1) -> Self {
        Self {
            potions: &observation.potions,
            encounter: &observation.encounter,
            turn: &observation.turn,
            player: &observation.player,
            cards: &observation.cards,
            monsters: &observation.monsters,
        }
    }
}

pub fn public_deterministic_baseline_decision_v1(
    decision: &LearningModelDecisionV1<'_>,
) -> PublicDeterministicBaselineDecisionV1 {
    let (candidate_ordinal, rule_family, coverage, limitation_ids) =
        match decision.observation {
            LearningModelObservationV1::Strategic(observation) => {
                let view = StrategicBaselineViewV1::from_learning(observation);
                let actions = decision
                    .candidates
                    .iter()
                    .enumerate()
                    .filter_map(|(ordinal, candidate)| match candidate.semantics {
                        LearningModelCandidateSemanticsV1::Strategic { action } => {
                            action.semantics().ok().map(|action| (ordinal, action))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                (
                    best_strategic_action_ordinal(&view, &actions),
                    strategic_rule_family(observation.decision_site),
                    if matches!(
                        observation.decision_site,
                        PlannerDecisionSite::Map
                            | PlannerDecisionSite::CardReward
                            | PlannerDecisionSite::Shop
                            | PlannerDecisionSite::Campfire
                            | PlannerDecisionSite::BossRelic
                    ) {
                        PublicBaselineCoverageV1::SpecializedHeuristic
                    } else {
                        PublicBaselineCoverageV1::GenericHeuristic
                    },
                    if observation.decision_site == PlannerDecisionSite::Event {
                        EVENT_LIMITS
                    } else {
                        STRATEGIC_LIMITS
                    },
                )
            }
            LearningModelObservationV1::Combat(observation) => {
                (
                    best_ordinal(decision.candidates.iter().enumerate().map(
                        |(ordinal, candidate)| {
                            (
                                ordinal,
                                combat_candidate_score(
                                    CombatBaselineViewV1::from_learning(observation),
                                    CombatBaselineCandidateV1::from_learning(candidate.semantics),
                                ),
                            )
                        },
                    )),
                    "combat.public_tactical_priority",
                    PublicBaselineCoverageV1::GenericHeuristic,
                    COMBAT_LIMITS,
                )
            }
        };
    PublicDeterministicBaselineDecisionV1 {
        candidate_ordinal,
        revision: PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
        rule_family,
        coverage,
        limitation_ids,
    }
}

/// Runs the same public deterministic scoring core directly on the deployable
/// task surface. The returned choice contains a complete public selection
/// payload when the winning candidate is symbolic; no private resolution id
/// or exact successor is consulted.
pub fn public_deterministic_task_decision_v1(
    decision: &TaskPublicDecisionV1,
) -> Result<PublicDeterministicTaskDecisionV1, String> {
    decision.validate()?;
    if decision.candidates.is_empty() {
        return Err("public deterministic task actor received no candidates".to_string());
    }
    let (candidate_ordinal, rule_family, coverage, limitation_ids) = match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => {
            let view = StrategicBaselineViewV1::from_task(observation);
            let admitted =
                admitted_task_strategic_candidate_ordinals_v1(observation, &decision.candidates);
            let actions = admitted
                .into_iter()
                .filter_map(|ordinal| match &decision.candidates[ordinal] {
                    TaskPublicCandidateV1::Strategic { action } => Some((ordinal, action.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            (
                best_strategic_action_ordinal(&view, &actions),
                strategic_rule_family(observation.decision_site),
                if matches!(
                    observation.decision_site,
                    PlannerDecisionSite::Map
                        | PlannerDecisionSite::CardReward
                        | PlannerDecisionSite::Shop
                        | PlannerDecisionSite::Campfire
                        | PlannerDecisionSite::BossRelic
                ) {
                    PublicBaselineCoverageV1::SpecializedHeuristic
                } else {
                    PublicBaselineCoverageV1::GenericHeuristic
                },
                if observation.decision_site == PlannerDecisionSite::Event {
                    EVENT_LIMITS
                } else {
                    STRATEGIC_LIMITS
                },
            )
        }
        TaskPublicObservationV1::Combat { observation, .. } => {
            let admitted =
                admitted_task_combat_candidate_ordinals_v1(observation, &decision.candidates);
            (
                best_ordinal(admitted.into_iter().map(|ordinal| {
                    let candidate = &decision.candidates[ordinal];
                    (
                        ordinal,
                        combat_candidate_score(
                            CombatBaselineViewV1::from_task(observation),
                            CombatBaselineCandidateV1::from_task(candidate),
                        ),
                    )
                })),
                "combat.public_tactical_priority",
                PublicBaselineCoverageV1::GenericHeuristic,
                COMBAT_LIMITS,
            )
        }
    };
    let choice = complete_task_choice_v1(decision, candidate_ordinal)?;
    decision.validate_choice(&choice)?;
    Ok(PublicDeterministicTaskDecisionV1 {
        public_snapshot_id: decision.information.snapshot_id.clone(),
        choice,
        revision: PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
        rule_family,
        coverage,
        limitation_ids,
    })
}

fn admitted_task_strategic_candidate_ordinals_v1(
    observation: &TaskStrategicObservationV1,
    candidates: &[TaskPublicCandidateV1],
) -> Vec<usize> {
    admitted_strategic_candidate_ordinals_v1(
        matches!(
            observation.context,
            crate::ai::planner_core::PlannerDecisionContext::Reward { .. }
        ),
        matches!(
            observation.decision_site,
            PlannerDecisionSite::Map | PlannerDecisionSite::CardReward
        ),
        observation.potions.iter().any(|slot| slot.potion.is_none()),
        observation
            .relics
            .iter()
            .any(|relic| relic.relic == RelicId::Sozu),
        candidates.iter().enumerate().map(|(ordinal, candidate)| {
            let kind = match candidate {
                TaskPublicCandidateV1::Strategic {
                    action:
                        TaskStrategicActionV1::ClaimReward {
                            reward:
                                crate::ai::planner_core::PlannerRewardDescriptor::Gold { .. }
                                | crate::ai::planner_core::PlannerRewardDescriptor::StolenGold {
                                    ..
                                },
                            ..
                        },
                } => PublicStrategicAdmissionKindV1::FreeGoldReward,
                TaskPublicCandidateV1::Strategic {
                    action:
                        TaskStrategicActionV1::ClaimReward {
                            reward: crate::ai::planner_core::PlannerRewardDescriptor::Potion { .. },
                            ..
                        },
                } => PublicStrategicAdmissionKindV1::PotionReward,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::OpenCardReward { .. },
                } => PublicStrategicAdmissionKindV1::OpenCardReward,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::DiscardRunPotion { .. },
                } => PublicStrategicAdmissionKindV1::DiscardRunPotion,
                _ => PublicStrategicAdmissionKindV1::Other,
            };
            (ordinal, kind)
        }),
    )
}

fn admitted_task_combat_candidate_ordinals_v1(
    observation: &crate::agent::information::state::PublicCombatStateV1,
    candidates: &[TaskPublicCandidateV1],
) -> Vec<usize> {
    let atomic_actions = candidates
        .iter()
        .filter_map(|candidate| match candidate {
            TaskPublicCandidateV1::CombatAtomic { action } => Some(action),
            TaskPublicCandidateV1::CombatIndexedChoice { action, .. } => Some(action),
            TaskPublicCandidateV1::Strategic { .. }
            | TaskPublicCandidateV1::CombatSelectionFamily { .. } => None,
        })
        .collect::<Vec<_>>();
    candidates
        .iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| match candidate {
            TaskPublicCandidateV1::CombatAtomic { action }
            | TaskPublicCandidateV1::CombatIndexedChoice { action, .. } => {
                public_combat_atomic_candidate_allowed_v1(observation, action, &atomic_actions)
                    .then_some(ordinal)
            }
            TaskPublicCandidateV1::CombatSelectionFamily { .. } => Some(ordinal),
            TaskPublicCandidateV1::Strategic { .. } => None,
        })
        .collect()
}

fn complete_task_choice_v1(
    decision: &TaskPublicDecisionV1,
    candidate_ordinal: usize,
) -> Result<TaskPublicChoiceV1, String> {
    let candidate = decision
        .candidates
        .get(candidate_ordinal)
        .ok_or_else(|| "public deterministic task candidate is out of range".to_string())?;
    let mut ordinal_path = vec![candidate_ordinal];
    match (&decision.observation, candidate) {
        (
            TaskPublicObservationV1::Strategic { observation },
            TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::BeginRunCardSelection {
                        reason,
                        min_choices,
                        max_choices,
                        selectable_card_ordinals,
                        ..
                    },
            },
        ) => {
            let view = StrategicBaselineViewV1::from_task(observation);
            let scores = selectable_card_ordinals
                .iter()
                .enumerate()
                .map(|(domain_index, card_ordinal)| {
                    let score = observation
                        .cards
                        .get(*card_ordinal)
                        .map(|card| strategic_selection_card_score(&view, *reason, card.ordinal))
                        .unwrap_or(-1_000);
                    (domain_index, score, *card_ordinal as u64)
                })
                .collect::<Vec<_>>();
            ordinal_path.extend(greedy_complete_selection_v1(
                scores,
                *min_choices,
                *max_choices,
            )?);
        }
        (
            TaskPublicObservationV1::Combat { .. },
            TaskPublicCandidateV1::CombatSelectionFamily { family },
        ) => {
            let min_choices = usize::try_from(family.declared_min)
                .map_err(|_| "combat selection minimum does not fit this platform".to_string())?;
            let mut max_choices = usize::try_from(family.effective_max)
                .map_err(|_| "combat selection maximum does not fit this platform".to_string())?;
            if matches!(family.reason, CombatSelectionReasonV2::ScryDiscard)
                && family.max_distinct_selection_count < family.eligible_domain_count
            {
                // A duplicated Scry address can refer to the same private card
                // occurrence. The public task view intentionally does not expose
                // that lineage equivalence class, so a deployable actor conservatively
                // selects at most one address instead of manufacturing an illegal
                // multi-address payload from hidden identity.
                max_choices = max_choices.min(1);
            }
            let scores = family
                .domain
                .iter()
                .enumerate()
                .filter_map(|(domain_index, domain)| {
                    let available = match domain {
                        PublicCombatSelectionDomainCandidateV1::Card { eligible, .. } => *eligible,
                        PublicCombatSelectionDomainCandidateV1::Scry {
                            currently_present, ..
                        } => *currently_present,
                    };
                    available.then(|| {
                        let public_occurrence = match domain {
                            PublicCombatSelectionDomainCandidateV1::Card { ordinal, .. } => {
                                *ordinal
                            }
                            PublicCombatSelectionDomainCandidateV1::Scry { index, .. } => *index,
                        };
                        (
                            domain_index,
                            public_combat_selection_domain_score(&family.reason, domain),
                            public_occurrence,
                        )
                    })
                })
                .collect::<Vec<_>>();
            ordinal_path.extend(greedy_complete_selection_v1(
                scores,
                min_choices,
                max_choices,
            )?);
        }
        _ => {}
    }
    Ok(TaskPublicChoiceV1 { ordinal_path })
}

fn greedy_complete_selection_v1(
    scores: Vec<(usize, i64, u64)>,
    min_choices: usize,
    max_choices: usize,
) -> Result<Vec<usize>, String> {
    let effective_max = max_choices.min(scores.len());
    if min_choices > effective_max {
        return Err("public selection exposes fewer candidates than its minimum".to_string());
    }
    let mut remaining = scores;
    let mut selected = Vec::new();
    while selected.len() < effective_max {
        let best = remaining
            .iter()
            .enumerate()
            .max_by(
                |(_, (left_ordinal, left_score, _)), (_, (right_ordinal, right_score, _))| {
                    left_score
                        .cmp(right_score)
                        .then_with(|| right_ordinal.cmp(left_ordinal))
                },
            )
            .map(|(position, (_, score, _))| (position, *score));
        let Some((position, score)) = best else {
            break;
        };
        if selected.len() >= min_choices && score <= 0 {
            break;
        }
        let (domain_index, _, public_occurrence) = remaining.remove(position);
        selected.push(domain_index);
        remaining.retain(|(_, _, occurrence)| *occurrence != public_occurrence);
    }
    Ok(selected)
}

pub fn public_deterministic_baseline_selection_decision_v1(
    observation: LearningModelObservationV1<'_>,
    draft: &LearningSelectionDraftV1,
) -> PublicDeterministicBaselineDecisionV1 {
    let decision = draft.decision();
    let can_submit = decision.candidates.iter().any(|candidate| {
        matches!(
            candidate.semantics,
            LearningSelectionCandidateSemanticsV1::Submit
        )
    });
    let ordinal = best_ordinal(decision.candidates.iter().enumerate().map(
        |(ordinal, candidate)| {
            let score = match candidate.semantics {
                LearningSelectionCandidateSemanticsV1::Submit => 0,
                LearningSelectionCandidateSemanticsV1::Append { domain_index } => {
                    selection_append_score(observation, draft, domain_index)
                        + if can_submit { 0 } else { 100_000 }
                }
            };
            (ordinal, score)
        },
    ));
    PublicDeterministicBaselineDecisionV1 {
        candidate_ordinal: ordinal,
        revision: PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
        rule_family: "selection.public_semantic_priority",
        coverage: PublicBaselineCoverageV1::GenericHeuristic,
        limitation_ids: SELECTION_LIMITS,
    }
}

fn best_ordinal(scores: impl IntoIterator<Item = (usize, i64)>) -> usize {
    scores
        .into_iter()
        .max_by(|(left_ordinal, left_score), (right_ordinal, right_score)| {
            left_score
                .cmp(right_score)
                .then_with(|| right_ordinal.cmp(left_ordinal))
        })
        .map(|(ordinal, _)| ordinal)
        .unwrap_or(0)
}

fn best_strategic_action_ordinal(
    observation: &StrategicBaselineViewV1<'_>,
    actions: &[(usize, TaskStrategicActionV1)],
) -> usize {
    if let Some(ordinal) = heart_key_deadline_action(observation, actions) {
        return ordinal;
    }
    if matches!(observation.run.player_class, PlannerPlayerClass::Ironclad)
        && (matches!(
            observation.decision_site,
            PlannerDecisionSite::CardReward | PlannerDecisionSite::Shop
        ) || (!actions.is_empty()
            && actions
                .iter()
                .all(|(_, action)| free_event_card_gain(action).is_some())))
    {
        let evaluations = actions
            .iter()
            .map(|(_, action)| strategic_pipeline_evaluation(observation, action))
            .collect::<Vec<_>>();
        let has_mainline_take = evaluations
            .iter()
            .flatten()
            .any(CandidateEvaluation::is_mainline);
        return actions
            .iter()
            .zip(evaluations)
            .min_by(
                |((left_ordinal, left_action), left), ((right_ordinal, right_action), right)| {
                    strategic_pipeline_order_key(left.as_ref(), left_action, has_mainline_take)
                        .cmp(&strategic_pipeline_order_key(
                            right.as_ref(),
                            right_action,
                            has_mainline_take,
                        ))
                        .then_with(|| left_ordinal.cmp(right_ordinal))
                },
            )
            .map(|((ordinal, _), _)| *ordinal)
            .unwrap_or(0);
    }

    let goal_routes = heart_goal_route_ordinals(observation, actions);
    best_ordinal(
        actions
            .iter()
            .filter(|(ordinal, action)| {
                !matches!(action, TaskStrategicActionV1::ChooseRouteNode { .. })
                    || goal_routes
                        .as_ref()
                        .is_none_or(|routes| routes.contains(ordinal))
            })
            .map(|(ordinal, action)| (*ordinal, strategic_action_score(observation, action))),
    )
}

fn missing_heart_keys(view: &StrategicBaselineViewV1<'_>) -> u8 {
    view.run
        .keys
        .iter()
        .enumerate()
        .fold(0, |mask, (key, held)| {
            mask | if *held { 0 } else { 1 << key }
        })
}

fn route_can_collect_keys(route: VisibleRoutePressureV1, required: u8) -> Option<bool> {
    route
        .key_opportunity_sets
        .map(|sets| (0..8).any(|mask| sets & (1 << mask) != 0 && mask & required == required))
}

/// Keep a complete path to the missing keys, rather than independently finding
/// a campfire on one branch and a burning elite on an incompatible branch.
/// Unknown topology stays eligible; no known viable path leaves ordinary ranking
/// in charge instead of inventing a route to Heart.
fn heart_goal_route_ordinals(
    view: &StrategicBaselineViewV1<'_>,
    actions: &[(usize, TaskStrategicActionV1)],
) -> Option<Vec<usize>> {
    if view.run_goal != PlannerRunGoal::HeartVictory || view.run.act != 3 {
        return None;
    }
    let required = missing_heart_keys(view);
    let routes = actions
        .iter()
        .filter_map(|(ordinal, action)| {
            let TaskStrategicActionV1::ChooseRouteNode { x, y, .. } = action else {
                return None;
            };
            Some((
                *ordinal,
                route_can_collect_keys(
                    visible_route_pressure(view.public_map, *x, *y, view.can_recover_at_rest),
                    required,
                ),
            ))
        })
        .collect::<Vec<_>>();
    routes
        .iter()
        .any(|(_, reachable)| *reachable == Some(true))
        .then(|| {
            routes
                .into_iter()
                .filter(|(_, reachable)| *reachable != Some(false))
                .map(|(ordinal, _)| ordinal)
                .collect()
        })
}

fn heart_key_deadline_action(
    view: &StrategicBaselineViewV1<'_>,
    actions: &[(usize, TaskStrategicActionV1)],
) -> Option<usize> {
    if view.run_goal != PlannerRunGoal::HeartVictory || view.run.act != 3 {
        return None;
    }
    if !view.run.keys[1] {
        if let Some((ordinal, _)) = actions.iter().find(|(_, action)| {
            matches!(
                action,
                TaskStrategicActionV1::ClaimReward {
                    reward: crate::ai::planner_core::PlannerRewardDescriptor::SapphireKey,
                    ..
                }
            )
        }) {
            return Some(*ordinal);
        }
    }
    if view.run.keys[0] {
        return None;
    }
    let recall = actions
        .iter()
        .find(|(_, action)| matches!(action, TaskStrategicActionV1::Recall))?;
    let current = view.public_map.nodes.iter().find(|node| {
        (node.x, node.y) == (view.public_map.current_x, view.public_map.current_y)
            && node.room == PlannerMapRoom::Rest
    })?;
    if current.edges.is_empty() {
        return None;
    }
    let can_defer = current
        .edges
        .iter()
        .map(|edge| {
            route_can_collect_keys(
                visible_route_pressure(
                    view.public_map,
                    edge.destination_x,
                    edge.destination_y,
                    view.can_recover_at_rest,
                ),
                missing_heart_keys(view),
            )
        })
        .collect::<Option<Vec<_>>>()?;
    (!can_defer.into_iter().any(|reachable| reachable)).then_some(recall.0)
}

fn strategic_pipeline_evaluation(
    observation: &StrategicBaselineViewV1<'_>,
    action: &TaskStrategicActionV1,
) -> Option<CandidateEvaluation> {
    let context = match observation.context {
        crate::ai::planner_core::PlannerDecisionContext::CardReward { .. } => {
            DecisionPipelineContext::reward(observation.deck_plan)
        }
        crate::ai::planner_core::PlannerDecisionContext::Event { .. }
            if free_event_card_gain(action).is_some() =>
        {
            DecisionPipelineContext::reward(observation.deck_plan)
        }
        crate::ai::planner_core::PlannerDecisionContext::Shop {
            purge_cost,
            purge_available,
            ..
        } => DecisionPipelineContext::shop_with_purge_reserve(
            observation.deck_plan,
            observation.run.gold,
            purge_available.then_some(*purge_cost),
        ),
        _ => return None,
    };
    let (kind, admission) = match action {
        TaskStrategicActionV1::TakeCard { card, upgrades, .. } => (
            DecisionCandidateKind::CardRewardPick {
                card: *card,
                upgrades: *upgrades,
            },
            Some(assess_reward_admission_from_master_deck(
                &observation.deck,
                *card,
                *upgrades,
            )),
        ),
        TaskStrategicActionV1::SkipCardReward { .. } => (
            DecisionCandidateKind::CardRewardSkip,
            Some(skip_reward_admission()),
        ),
        TaskStrategicActionV1::BuyCard {
            card,
            upgrades,
            price,
            ..
        } => (
            DecisionCandidateKind::ShopBuyCard {
                card: *card,
                upgrades: *upgrades,
                price: *price,
            },
            Some(assess_reward_admission_from_master_deck(
                &observation.deck,
                *card,
                *upgrades,
            )),
        ),
        TaskStrategicActionV1::BuyRelic { relic, price, .. } => (
            DecisionCandidateKind::ShopBuyRelic {
                relic: *relic,
                price: *price,
            },
            None,
        ),
        TaskStrategicActionV1::BuyPotion { potion, price, .. } => (
            DecisionCandidateKind::ShopBuyPotion {
                potion: *potion,
                price: *price,
            },
            None,
        ),
        TaskStrategicActionV1::RemoveCard { card, .. } => (
            DecisionCandidateKind::ShopPurge {
                target: public_cleanup_target(*card, action_card_upgrades(action)),
            },
            None,
        ),
        TaskStrategicActionV1::OpenPendingRewards => (DecisionCandidateKind::ShopOpenRewards, None),
        TaskStrategicActionV1::LeaveShop => (DecisionCandidateKind::ShopLeave, None),
        _ => {
            let card = free_event_card_gain(action)?;
            (
                DecisionCandidateKind::CardRewardPick {
                    card: card.card,
                    upgrades: card.upgrades,
                },
                Some(assess_reward_admission_from_master_deck(
                    &observation.deck,
                    card.card,
                    card.upgrades,
                )),
            )
        }
    };
    Some(evaluate_decision_candidate(
        context,
        kind,
        admission.as_ref(),
    ))
}

/// Reuse card acquisition judgment only for a complete, cost-free one-card
/// effect. Mixed trades and multiple copies retain their event resource score.
fn free_event_card_gain(
    action: &TaskStrategicActionV1,
) -> Option<crate::ai::planner_core::PlannerOfferedCard> {
    use crate::ai::planner_core::{
        PlannerEventCardPoolV1, PlannerEventPublicEffectV1, PlannerOfferedCard,
    };
    let TaskStrategicActionV1::ChooseEventOption {
        public_semantics, ..
    } = action
    else {
        return None;
    };
    match public_semantics
        .structured_semantics
        .as_ref()?
        .effects
        .as_slice()
    {
        [PlannerEventPublicEffectV1::ObtainCard {
            count: 1,
            pool: PlannerEventCardPoolV1::Specific { card, upgrades },
        }]
        | [PlannerEventPublicEffectV1::ObtainColorlessCard {
            count: 1,
            pool: PlannerEventCardPoolV1::Specific { card, upgrades },
        }] => Some(PlannerOfferedCard {
            card: *card,
            upgrades: *upgrades,
        }),
        _ => None,
    }
}

fn strategic_pipeline_order_key(
    evaluation: Option<&CandidateEvaluation>,
    action: &TaskStrategicActionV1,
    has_mainline_take: bool,
) -> CandidateOrderKey {
    if let Some(evaluation) = evaluation {
        return evaluation.order_key(has_mainline_take);
    }
    if matches!(action, TaskStrategicActionV1::SingingBowl { .. }) {
        let mut key = CandidateOrderKey::optional_skip(has_mainline_take);
        key.score_rank = -1;
        return key;
    }
    if matches!(
        action,
        TaskStrategicActionV1::UseRunPotion {
            potion: PotionId::FruitJuice,
            ..
        }
    ) {
        return CandidateOrderKey {
            lane_rank: 0,
            score_rank: -3_000,
            tiebreak_rank: 0,
        };
    }
    CandidateOrderKey::fallback()
}

fn action_card_upgrades(action: &TaskStrategicActionV1) -> u8 {
    match action {
        TaskStrategicActionV1::RemoveCard { upgrades, .. } => *upgrades,
        _ => 0,
    }
}

fn public_cleanup_target(card: CardId, upgrades: u8) -> CleanupTarget {
    let definition = get_card_definition(card);
    if definition.card_type == CardType::Curse {
        CleanupTarget::Curse
    } else if definition.card_type == CardType::Status {
        CleanupTarget::Status
    } else if definition.tags.contains(&CardTag::StarterStrike) {
        if upgrades == 0 {
            CleanupTarget::StarterStrike
        } else {
            CleanupTarget::UpgradedStarterStrike
        }
    } else if definition.tags.contains(&CardTag::StarterDefend) {
        if upgrades == 0 {
            CleanupTarget::StarterDefend
        } else {
            CleanupTarget::UpgradedStarterDefend
        }
    } else if is_starter_basic(card) {
        CleanupTarget::OtherStarter
    } else {
        CleanupTarget::Other
    }
}

fn strategic_rule_family(site: PlannerDecisionSite) -> &'static str {
    match site {
        PlannerDecisionSite::Map => "strategic.visible_route",
        PlannerDecisionSite::CardReward => "strategic.marginal_deck_construction",
        PlannerDecisionSite::Shop => "strategic.shop_marginal_construction",
        PlannerDecisionSite::Campfire => "strategic.campfire_survival_growth",
        PlannerDecisionSite::BossRelic => "strategic.boss_relic_debt",
        PlannerDecisionSite::Neow => "strategic.neow_public_option",
        PlannerDecisionSite::Event => "strategic.event_public_option",
        PlannerDecisionSite::Reward => "strategic.claim_public_reward",
        PlannerDecisionSite::RunChoice => "strategic.run_choice",
        PlannerDecisionSite::Treasure => "strategic.open_treasure",
        PlannerDecisionSite::RunTransition => "strategic.proceed_run_transition",
    }
}

fn strategic_action_score(
    observation: &StrategicBaselineViewV1<'_>,
    action: &TaskStrategicActionV1,
) -> i64 {
    let run = observation.run;
    match action {
        LearningStrategicActionSemanticsV2::ChooseRouteNode { x, y, flight, .. } => {
            visible_route_score(observation, *x, *y) - i64::from(*flight) * 80
        }
        LearningStrategicActionSemanticsV2::ChooseEventOption {
            event,
            screen,
            action,
            public_semantics,
            ..
        } => {
            if *event == crate::state::events::EventId::ShiningLight
                && public_semantics.hp_delta < 0
                && public_semantics.cards_upgraded > 0
            {
                shining_light_score(observation, public_semantics)
            } else {
                event_action_score(*event, *screen, *action, public_semantics, observation)
            }
        }
        LearningStrategicActionSemanticsV2::TakeCard { card, upgrades, .. } => {
            strategic_card_value(observation, *card, *upgrades)
        }
        LearningStrategicActionSemanticsV2::OpenCardReward { .. } => 2_000,
        LearningStrategicActionSemanticsV2::SingingBowl { .. } => {
            if run.max_hp > 0 && run.current_hp * 4 < run.max_hp * 3 {
                280
            } else {
                120
            }
        }
        LearningStrategicActionSemanticsV2::SkipCardReward { .. } => 100,
        LearningStrategicActionSemanticsV2::ClaimReward {
            reward_item_index: _,
            reward,
        } => {
            use crate::ai::planner_core::PlannerRewardDescriptor as Reward;
            match reward {
                Reward::Relic { .. } => 2_400,
                Reward::EmeraldKey | Reward::SapphireKey => 2_100,
                Reward::Gold { amount } | Reward::StolenGold { amount } => {
                    1_600 + i64::from(*amount)
                }
                Reward::CardReward { .. } => 1_500,
                Reward::Potion { .. } => {
                    if observation.occupied_potions < run.potion_capacity {
                        1_300
                    } else {
                        -200
                    }
                }
            }
        }
        LearningStrategicActionSemanticsV2::Rest => {
            let missing = (run.max_hp - run.current_hp).max(0);
            if missing == 0 {
                0
            } else if run.max_hp > 0 && run.current_hp * 5 <= run.max_hp * 2 {
                4_000 + i64::from(missing)
            } else {
                600 + i64::from(missing) * 5
            }
        }
        LearningStrategicActionSemanticsV2::Smith {
            card_ordinal,
            card,
            upgrades,
        } => campfire_smith_score(observation, *card_ordinal, *card, *upgrades),
        LearningStrategicActionSemanticsV2::Dig => 1_050,
        LearningStrategicActionSemanticsV2::Lift => 1_000,
        LearningStrategicActionSemanticsV2::Toke { card, .. } => {
            900 - strategic_card_value(observation, *card, 0)
        }
        LearningStrategicActionSemanticsV2::Recall => {
            if observation.run_goal != PlannerRunGoal::HeartVictory {
                -500
            } else if run.act >= 3 && !run.keys[0] {
                1_600
            } else {
                300
            }
        }
        LearningStrategicActionSemanticsV2::BuyCard {
            card,
            upgrades,
            price,
            ..
        } => affordable_score(
            run.gold,
            *price,
            strategic_card_value(observation, *card, *upgrades),
        ),
        LearningStrategicActionSemanticsV2::BuyRelic { relic, price, .. } => {
            affordable_score(run.gold, *price, relic_value(*relic))
        }
        LearningStrategicActionSemanticsV2::BuyPotion { potion, price, .. } => {
            affordable_score(run.gold, *price, potion_inventory_value(*potion))
        }
        LearningStrategicActionSemanticsV2::UseRunPotion { potion, .. } => match potion {
            PotionId::FruitJuice => 3_000,
            _ => -500,
        },
        LearningStrategicActionSemanticsV2::DiscardRunPotion { .. } => -1_000,
        LearningStrategicActionSemanticsV2::RemoveCard { card, price, .. } => {
            affordable_score(run.gold, *price, removal_value(*card))
        }
        LearningStrategicActionSemanticsV2::OpenPendingRewards => 2_200,
        LearningStrategicActionSemanticsV2::LeaveShop => 0,
        LearningStrategicActionSemanticsV2::TakeBossRelic { relic, .. } => boss_relic_value(*relic),
        LearningStrategicActionSemanticsV2::SkipBossRelic => -2_000,
        LearningStrategicActionSemanticsV2::BeginRunCardSelection { .. } => 1_000,
        LearningStrategicActionSemanticsV2::SubmitRunSelection {
            selected_card_ordinals,
            ..
        } => {
            let crate::ai::planner_core::PlannerDecisionContext::RunChoice { reason, .. } =
                observation.context
            else {
                return -1_000;
            };
            selected_card_ordinals
                .iter()
                .map(|ordinal| strategic_selection_card_score(observation, *reason, *ordinal))
                .sum()
        }
        LearningStrategicActionSemanticsV2::OpenChest => 2_500,
        LearningStrategicActionSemanticsV2::Proceed { .. } => {
            if observation.decision_site == PlannerDecisionSite::Reward {
                // A reward screen may offer several independent free potions.
                // Proceed must not outrank claiming one while a slot is open.
                0
            } else {
                2_000
            }
        }
        LearningStrategicActionSemanticsV2::Cancel { .. } => -1_500,
    }
}

fn campfire_smith_score(
    observation: &StrategicBaselineViewV1<'_>,
    card_ordinal: usize,
    card: CardId,
    upgrades: u8,
) -> i64 {
    let ranked =
        rank_campfire_upgrades(&observation.deck, observation.base_cost_randomized_on_draw);
    let Some((position, quality)) = ranked
        .iter()
        .enumerate()
        .find(|(_, quality)| quality.deck_index == card_ordinal)
    else {
        return 0;
    };
    if quality.card != card || observation.deck[quality.deck_index].upgrades != upgrades {
        return 0;
    }
    let tier = match quality.tier {
        CampfireUpgradeTier::Strong => 1_900,
        CampfireUpgradeTier::Useful => 1_500,
        CampfireUpgradeTier::Low => 1_050,
        CampfireUpgradeTier::Avoid => 200,
    };
    tier - i64::try_from(position).unwrap_or(i64::MAX).min(100)
}

fn visible_route_score(observation: &StrategicBaselineViewV1<'_>, x: i32, y: i32) -> i64 {
    let run = observation.run;
    let hp_ratio = if run.max_hp > 0 {
        run.current_hp.saturating_mul(100) / run.max_hp
    } else {
        0
    };
    let room = observation
        .public_map
        .nodes
        .iter()
        .find(|node| node.x == x && node.y == y)
        .map(|node| node.room);
    let recent_attrition_pressure = observation
        .public_history
        .recent_combat_hp_loss
        .is_some_and(|loss| run.max_hp > 0 && loss.max(0).saturating_mul(6) >= run.max_hp);
    let route = visible_route_pressure(
        observation.public_map,
        x,
        y,
        observation.can_recover_at_rest,
    );
    let immediate = match room {
        Some(PlannerMapRoom::Rest) => {
            if hp_ratio < 55 && observation.can_recover_at_rest {
                1_500
            } else {
                450
            }
        }
        Some(PlannerMapRoom::Shop) => {
            if run.gold >= 150 {
                1_100
            } else {
                150
            }
        }
        Some(PlannerMapRoom::Elite) => {
            if hp_ratio >= 75
                && run.act == 1
                && !recent_attrition_pressure
                && route.min_elites <= 1
                && route.min_damage_rooms_before_recovery <= 1
            {
                950
            } else {
                -1_100
            }
        }
        Some(PlannerMapRoom::Treasure) => 1_000,
        Some(PlannerMapRoom::Boss) => 2_000,
        Some(PlannerMapRoom::QuestionMark) => 500,
        Some(PlannerMapRoom::Monster) => 250,
        Some(PlannerMapRoom::Victory) => 3_000,
        None => 100,
    };
    let flexibility = observation
        .public_map
        .nodes
        .iter()
        .find(|node| node.x == x && node.y == y)
        .map(|node| node.edges.len() as i64 * 30)
        .unwrap_or_default();
    let recovery = if hp_ratio < 75 && route.recovery_before_damage_reachable {
        180
    } else {
        0
    };
    let forced_pressure = i64::try_from(route.min_damage_rooms_before_recovery)
        .unwrap_or(i64::MAX)
        .saturating_mul(if hp_ratio < 75 { 260 } else { 90 });
    let forced_elites = i64::try_from(route.min_elites)
        .unwrap_or(i64::MAX)
        .saturating_mul(450);
    immediate + flexibility + recovery - forced_pressure - forced_elites
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct VisibleRoutePressureV1 {
    min_elites: usize,
    max_elites: usize,
    min_damage_rooms_before_recovery: usize,
    recovery_before_damage_reachable: bool,
    /// Bit n denotes a path offering the key subset n (ruby, sapphire, emerald).
    /// None means the public suffix does not reach a represented act endpoint.
    key_opportunity_sets: Option<u8>,
}

fn visible_route_pressure(
    public_map: &PlannerPublicMap,
    start_x: i32,
    start_y: i32,
    can_recover_at_rest: bool,
) -> VisibleRoutePressureV1 {
    let mut nodes = public_map.nodes.iter().collect::<Vec<_>>();
    nodes.sort_by_key(|node| std::cmp::Reverse(node.y));
    let mut suffix = std::collections::BTreeMap::new();
    for node in nodes {
        let children = node
            .edges
            .iter()
            .map(|edge| {
                suffix
                    .get(&(edge.destination_x, edge.destination_y))
                    .copied()
            })
            .collect::<Option<Vec<VisibleRoutePressureV1>>>();
        let Some(children) = children else {
            return VisibleRoutePressureV1::default();
        };
        let elite = usize::from(node.room == PlannerMapRoom::Elite);
        let damage = usize::from(matches!(
            node.room,
            PlannerMapRoom::Monster | PlannerMapRoom::Elite | PlannerMapRoom::Boss
        ));
        let recovery = can_recover_at_rest && node.room == PlannerMapRoom::Rest;
        let opportunities = u8::from(node.room == PlannerMapRoom::Rest)
            | (u8::from(node.room == PlannerMapRoom::Treasure) << 1)
            | (u8::from(node.room == PlannerMapRoom::Elite && node.has_emerald_key) << 2);
        let key_opportunity_sets = if children.is_empty() {
            matches!(node.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory)
                .then_some(1 << opportunities)
        } else {
            children.iter().try_fold(0, |sets, child| {
                let child_sets = child.key_opportunity_sets?;
                Some(
                    (0..8)
                        .filter(|mask| child_sets & (1 << mask) != 0)
                        .fold(sets, |sets, mask| sets | (1 << (mask | opportunities))),
                )
            })
        };
        let summary = VisibleRoutePressureV1 {
            key_opportunity_sets,
            min_elites: elite
                + children
                    .iter()
                    .map(|child| child.min_elites)
                    .min()
                    .unwrap_or(0),
            max_elites: elite
                + children
                    .iter()
                    .map(|child| child.max_elites)
                    .max()
                    .unwrap_or(0),
            min_damage_rooms_before_recovery: if recovery {
                0
            } else {
                damage
                    + children
                        .iter()
                        .map(|child| child.min_damage_rooms_before_recovery)
                        .min()
                        .unwrap_or(0)
            },
            recovery_before_damage_reachable: recovery
                || (damage == 0
                    && children
                        .iter()
                        .any(|child| child.recovery_before_damage_reachable)),
        };
        if suffix.insert((node.x, node.y), summary).is_some() {
            return VisibleRoutePressureV1::default();
        }
    }
    suffix.get(&(start_x, start_y)).copied().unwrap_or_default()
}

fn event_action_score(
    event: crate::state::events::EventId,
    screen: usize,
    action: EventActionKind,
    public: &crate::ai::planner_core::PlannerEventOptionPublicSemanticsV1,
    observation: &StrategicBaselineViewV1<'_>,
) -> i64 {
    use crate::ai::planner_core::{PlannerEventCardPoolV1, PlannerEventPublicEffectV1};

    let hp = observation.run.current_hp;
    let max_hp = observation.run.max_hp;
    let low_hp = max_hp > 0 && hp * 2 <= max_hp;
    let generic = match action {
        EventActionKind::Gain => 1_200,
        // Revealed card effects already carry their own value or loss below.
        // An operation label must not turn removing a useful card into a gain.
        EventActionKind::DeckOperation
            if !public.specific_cards_removed.is_empty()
                || !public.specific_cards_gained.is_empty() =>
        {
            0
        }
        EventActionKind::DeckOperation => 950,
        EventActionKind::Accept => {
            if low_hp {
                250
            } else {
                750
            }
        }
        EventActionKind::Continue => 600,
        EventActionKind::Fight => {
            if low_hp {
                -800
            } else {
                450
            }
        }
        EventActionKind::Trade => {
            if low_hp {
                -300
            } else {
                350
            }
        }
        EventActionKind::Special => 300,
        EventActionKind::Leave | EventActionKind::Decline => 0,
        EventActionKind::Unknown => -100,
    };
    // The summary lists a specific kind once per effect, not once per copy
    // (for example, Vampires grants five Bites). Typed effects own the count.
    let known_gains: i64 = if let Some(structured) = &public.structured_semantics {
        structured
            .effects
            .iter()
            .filter_map(|effect| match effect {
                PlannerEventPublicEffectV1::ObtainCard {
                    count,
                    pool: PlannerEventCardPoolV1::Specific { card, upgrades },
                }
                | PlannerEventPublicEffectV1::ObtainColorlessCard {
                    count,
                    pool: PlannerEventCardPoolV1::Specific { card, upgrades },
                } => Some(
                    i64::try_from(*count).unwrap_or(i64::MAX)
                        * strategic_card_value(observation, *card, *upgrades),
                ),
                _ => None,
            })
            .sum()
    } else {
        public
            .specific_cards_gained
            .iter()
            .map(|card| strategic_card_value(observation, card.card, card.upgrades))
            .sum()
    };
    let known_removals: i64 = public
        .specific_cards_removed
        .iter()
        .map(|card| {
            selection_card_score(
                SelectionReason::Purge,
                strategic_card_value(observation, card.card, card.upgrades),
                card.card,
            )
        })
        .sum();
    let unspecified_removals = public
        .cards_removed
        .saturating_sub(public.specific_cards_removed.len());
    let resource = known_gains
        + known_removals
        + i64::from(public.gold_delta_min.saturating_add(public.gold_delta_max)) * 2
        + i64::from(public.hp_delta) * if low_hp { 65 } else { 50 }
        + i64::from(public.max_hp_delta) * 55
        + i64::try_from(public.relics_gained).unwrap_or(i64::MAX) * 900
        - i64::try_from(public.relics_lost).unwrap_or(i64::MAX) * 1_200
        + i64::try_from(public.potions_gained).unwrap_or(i64::MAX) * 250
        + i64::try_from(public.cards_offered).unwrap_or(i64::MAX) * 180
        - i64::try_from(public.curses_gained).unwrap_or(i64::MAX) * 900
        + i64::try_from(unspecified_removals).unwrap_or(i64::MAX) * 600
        + i64::try_from(public.cards_upgraded).unwrap_or(i64::MAX) * 400
        + i64::from(public.upgrades_all_cards) * 2_500
        + i64::try_from(public.cards_transformed).unwrap_or(i64::MAX) * 180
        + i64::try_from(public.cards_duplicated).unwrap_or(i64::MAX) * 350;
    let risk = i64::from(public.starts_combat) * if low_hp { 2_000 } else { 900 }
        + i64::from(public.repeatable && public.random_outcome) * 1_100;
    let unknown_repeatable_event = event == crate::state::events::EventId::DeadAdventurer
        && screen == 0
        && public.repeatable
        && public.random_outcome;
    generic + resource - risk - i64::from(unknown_repeatable_event) * 500
}

fn shining_light_score(
    observation: &StrategicBaselineViewV1<'_>,
    public: &crate::ai::planner_core::PlannerEventOptionPublicSemanticsV1,
) -> i64 {
    let upgrade_value = expected_random_upgrade_value(
        &observation.deck,
        observation.base_cost_randomized_on_draw,
        public.cards_upgraded,
    );
    let hp_cost = i64::from(public.hp_delta.saturating_neg());
    let hp_after = observation.run.current_hp.saturating_add(public.hp_delta);
    if hp_after <= 0 {
        return -100_000;
    }
    let hp_price = if hp_after.saturating_mul(2) <= observation.run.max_hp {
        65
    } else {
        50
    };
    upgrade_value - hp_cost.saturating_mul(hp_price)
}

fn expected_random_upgrade_value(
    deck: &[CombatCard],
    base_cost_randomized_on_draw: bool,
    upgrade_count: usize,
) -> i64 {
    let targets = rank_campfire_upgrades(deck, base_cost_randomized_on_draw);
    if targets.is_empty() {
        return 0;
    }
    // Shining Light selects random eligible cards, so value the whole eligible
    // pool rather than treating both upgrades as chosen smith targets.
    let total: i64 = targets
        .iter()
        .map(|target| match target.tier {
            CampfireUpgradeTier::Strong => 1_000,
            CampfireUpgradeTier::Useful => 650,
            CampfireUpgradeTier::Low => 250,
            CampfireUpgradeTier::Avoid => 0,
        })
        .sum();
    let upgrades = upgrade_count.min(targets.len());
    total.saturating_mul(i64::try_from(upgrades).unwrap_or(i64::MAX))
        / i64::try_from(targets.len()).unwrap_or(i64::MAX)
}

fn strategic_card_value(
    observation: &StrategicBaselineViewV1<'_>,
    card: CardId,
    upgrades: u8,
) -> i64 {
    let definition = get_card_definition(card);
    if matches!(definition.card_type, CardType::Curse | CardType::Status) {
        return -2_000;
    }
    let upgraded = i64::from(upgrades > 0);
    let mut value = match definition.rarity {
        CardRarity::Rare => 480,
        CardRarity::Uncommon => 330,
        CardRarity::Common => 220,
        CardRarity::Basic => 40,
        CardRarity::Special => 300,
        CardRarity::Curse => -2_000,
    } + i64::from(definition.base_damage.max(0)) * 9
        + i64::from(definition.base_block.max(0)) * 8
        + i64::from(definition.base_magic.max(0)) * 3
        + upgraded
            * i64::from(
                definition.upgrade_damage.max(0) * 9
                    + definition.upgrade_block.max(0) * 8
                    + definition.upgrade_magic.max(0) * 3,
            );
    if definition.tags.contains(&CardTag::StarterStrike)
        || definition.tags.contains(&CardTag::StarterDefend)
    {
        value -= 180;
    }
    let deck_size = observation.deck.len();
    if deck_size >= 25 {
        value -= 120;
    }
    if observation.run.player_class == PlannerPlayerClass::Ironclad {
        value += match card {
            CardId::Offering | CardId::Corruption | CardId::FeelNoPain | CardId::DarkEmbrace => 420,
            CardId::BattleTrance | CardId::ShrugItOff | CardId::Shockwave | CardId::Disarm => 260,
            CardId::Immolate | CardId::Whirlwind | CardId::FiendFire => 220,
            CardId::Clash | CardId::SearingBlow => -260,
            _ => 0,
        };
    }
    value
}

fn affordable_score(gold: i32, price: i32, value: i64) -> i64 {
    if price > gold {
        i64::MIN / 8
    } else {
        value - i64::from(price) * 2
    }
}

fn removal_value(card: CardId) -> i64 {
    let definition = get_card_definition(card);
    if definition.card_type == CardType::Curse {
        2_200
    } else if definition.tags.contains(&CardTag::StarterStrike) {
        1_000
    } else if definition.tags.contains(&CardTag::StarterDefend) {
        650
    } else {
        100
    }
}

fn relic_value(relic: RelicId) -> i64 {
    match get_relic_tier(relic) {
        RelicTier::Boss => 1_800,
        RelicTier::Rare => 1_300,
        RelicTier::Uncommon => 1_000,
        RelicTier::Common => 750,
        RelicTier::Shop => 1_050,
        RelicTier::Event | RelicTier::Special => 900,
        RelicTier::Starter => 600,
        RelicTier::Deprecated => -1_000,
    }
}

fn boss_relic_value(relic: RelicId) -> i64 {
    let base = relic_value(relic) + 1_500;
    base + match relic {
        RelicId::RunicPyramid | RelicId::SneckoEye | RelicId::CoffeeDripper => 700,
        RelicId::CursedKey | RelicId::FusionHammer | RelicId::SlaversCollar => 450,
        RelicId::BustedCrown | RelicId::Ectoplasm | RelicId::Sozu | RelicId::RunicDome => -550,
        RelicId::VelvetChoker => -250,
        _ => 0,
    }
}

fn potion_inventory_value(potion: PotionId) -> i64 {
    let roles = potion_mechanic_roles(potion);
    350 + roles.len() as i64 * 80
        + if roles.contains(&PotionMechanicRole::PreventDeath) {
            700
        } else {
            0
        }
        + if roles.contains(&PotionMechanicRole::GainIntangible) {
            500
        } else {
            0
        }
}

fn combat_candidate_score(
    observation: CombatBaselineViewV1<'_>,
    candidate: CombatBaselineCandidateV1<'_>,
) -> i64 {
    match candidate {
        CombatBaselineCandidateV1::SelectionFamily => 300,
        CombatBaselineCandidateV1::InvalidDomain => i64::MIN / 4,
        candidate => combat_atomic_score(observation, candidate),
    }
}

fn combat_atomic_score(
    observation: CombatBaselineViewV1<'_>,
    action: CombatBaselineCandidateV1<'_>,
) -> i64 {
    let incoming = visible_incoming_damage(observation);
    let exposed = incoming.saturating_sub(observation.player.block).max(0);
    match action {
        CombatBaselineCandidateV1::PlayCard {
            hand_index,
            target_monster_index,
        } => {
            let Some(card) = observation.cards.hand.cards.get(hand_index) else {
                return i64::MIN / 4;
            };
            let definition = get_card_definition(card.card_id);
            if matches!(definition.card_type, CardType::Curse | CardType::Status) {
                return -20_000;
            }
            if card.effective_cost < 0
                && observation.turn.energy == 0
                && !card.free_to_play_once
                && card.energy_on_use <= 0
            {
                return -20_000;
            }
            let mut target_damage = target_monster_index
                .and_then(|index| card.damage_by_monster_order.get(index).copied())
                .unwrap_or(card.current_damage)
                .max(0);
            if card.effective_cost < 0 {
                target_damage = target_damage.saturating_mul(i32::from(observation.turn.energy));
            }
            if definition.is_multi_damage {
                let living = observation
                    .monsters
                    .iter()
                    .filter(|monster| monster.alive)
                    .count()
                    .try_into()
                    .unwrap_or(i32::MAX);
                target_damage = target_damage.saturating_mul(living);
            }
            let useful_damage = if definition.is_multi_damage {
                target_damage
            } else {
                target_monster_index
                    .and_then(|index| observation.monsters.get(index))
                    .filter(|monster| monster.alive)
                    .map(|monster| {
                        target_damage.min(monster.hp.saturating_add(monster.block).max(0))
                    })
                    .unwrap_or(target_damage)
            };
            let block = card.current_block.max(0);
            let mut score = i64::from(useful_damage) * 14 + i64::from(block.min(exposed)) * 18
                - i64::from(card.effective_cost.max(0)) * 12;
            // Sharp Hide triggers when an Attack is played, before the card's
            // own block or damage resolves. Current public block can absorb it;
            // block needed for the visible enemy attack cannot be counted free.
            let retaliation = visible_sharp_hide_on_attack(observation, definition.card_type);
            let spare_block = observation.player.block.saturating_sub(incoming).max(0);
            score -= i64::from(retaliation.saturating_sub(spare_block).max(0)) * 50;
            if retaliation.saturating_sub(observation.player.block) >= observation.player.hp {
                score -= 200_000;
            }
            if let Some(index) = target_monster_index {
                if let Some(monster) = observation.monsters.get(index) {
                    let effective_hp = monster.hp.saturating_add(monster.block).max(0);
                    if monster.alive && target_damage >= effective_hp {
                        score += 2_500 + i64::from(visible_monster_attack_damage(monster)) * 20;
                    } else {
                        score += i64::from((200 - effective_hp).max(0));
                    }
                    if card.card_id == CardId::Strike
                        && monster.intent.intent == Some(Intent::Sleep)
                        && target_damage < effective_hp
                    {
                        // A basic Strike is a poor way to spend a public setup
                        // window. Keep lethal attacks available, but wait for
                        // a stronger opening or setup card instead.
                        score -= 1_000;
                    }
                }
            }
            score += match definition.card_type {
                CardType::Power => {
                    if exposed <= 0 {
                        260
                    } else {
                        -120
                    }
                }
                CardType::Skill => 0,
                CardType::Attack => 80,
                CardType::Status | CardType::Curse => unreachable!(),
            };
            score += visible_turn_survival_priority(
                observation,
                hand_index,
                target_monster_index,
                target_damage,
                exposed,
            );
            score
        }
        CombatBaselineCandidateV1::UsePotion {
            potion_index,
            target_monster_index,
        } => observation
            .potions
            .get(potion_index)
            .and_then(Option::as_ref)
            .map(|potion| {
                combat_potion_score(potion.potion_id, target_monster_index, observation, exposed)
            })
            .unwrap_or(-2_000),
        CombatBaselineCandidateV1::DiscardPotion => -4_000,
        CombatBaselineCandidateV1::EndTurn => {
            -i64::from(exposed) * 30 - i64::from(observation.player.hp <= exposed) * 100_000
        }
        CombatBaselineCandidateV1::SubmitIndexedChoice { candidate } => match candidate {
            PublicCombatIndexedChoiceCandidateV1::Card { card_id, upgrades } => {
                let definition = get_card_definition(*card_id);
                i64::from(definition.base_damage + definition.base_block + definition.base_magic)
                    + i64::from(*upgrades) * 20
            }
            PublicCombatIndexedChoiceCandidateV1::Stance { .. } => 100,
        },
        CombatBaselineCandidateV1::Proceed => 10_000,
        CombatBaselineCandidateV1::Cancel => -2_000,
        CombatBaselineCandidateV1::SelectionFamily | CombatBaselineCandidateV1::InvalidDomain => {
            i64::MIN / 4
        }
    }
}

fn visible_sharp_hide_on_attack(observation: CombatBaselineViewV1<'_>, card_type: CardType) -> i32 {
    if card_type != CardType::Attack {
        return 0;
    }
    observation
        .monsters
        .iter()
        .filter(|monster| monster.alive)
        .flat_map(|monster| monster.powers.iter())
        .filter(|power| power.power == PowerId::SharpHide)
        .fold(0_i32, |total, power| {
            total.saturating_add(power.amount.max(0))
        })
}

/// A narrow public-hand guard for an otherwise lethal visible attack. It does
/// not simulate draws or future turns; the ordinary scorer handles positions
/// where the currently playable block cannot establish a safe end turn.
fn visible_turn_survival_priority(
    observation: CombatBaselineViewV1<'_>,
    hand_index: usize,
    target_monster_index: Option<usize>,
    target_damage: i32,
    exposed: i32,
) -> i64 {
    if exposed < observation.player.hp || observation.player.hp <= 0 {
        return 0;
    }
    let energy = usize::from(observation.turn.energy);
    let mut block_by_energy = vec![0_i32; energy + 1];
    for card in &observation.cards.hand.cards {
        if card.current_block <= 0 {
            continue;
        }
        let cost = if card.free_to_play_once {
            0
        } else if let Ok(cost) = usize::try_from(card.effective_cost) {
            cost
        } else {
            continue;
        };
        if cost > energy {
            continue;
        }
        for available in (cost..=energy).rev() {
            block_by_energy[available] = block_by_energy[available]
                .max(block_by_energy[available - cost].saturating_add(card.current_block));
        }
    }
    let best_block = block_by_energy[energy];
    if best_block <= 0 || exposed.saturating_sub(best_block) >= observation.player.hp {
        return 0;
    }
    if let Some(index) = target_monster_index {
        if let Some(monster) = observation.monsters.get(index) {
            if monster.alive
                && target_damage >= monster.hp.saturating_add(monster.block)
                && exposed.saturating_sub(visible_monster_attack_damage(monster))
                    < observation.player.hp
            {
                return 200_000;
            }
        }
    }
    observation
        .cards
        .hand
        .cards
        .get(hand_index)
        .filter(|card| card.current_block > 0)
        .map(|card| 100_000 + i64::from(card.current_block.min(exposed)) * 1_000)
        .unwrap_or(0)
}

fn visible_incoming_damage(observation: CombatBaselineViewV1<'_>) -> i32 {
    observation
        .monsters
        .iter()
        .filter(|monster| monster.alive)
        .map(visible_monster_attack_damage)
        .sum()
}

fn visible_monster_attack_damage(monster: &CombatLearningMonsterStateV1) -> i32 {
    match &monster.intent {
        crate::agent::information::state::CombatLearningIntentV1 {
            intent:
                Some(
                    Intent::Attack { damage, hits }
                    | Intent::AttackBuff { damage, hits }
                    | Intent::AttackDebuff { damage, hits }
                    | Intent::AttackDefend { damage, hits },
                ),
            preview_damage_per_hit,
            ..
        } => preview_damage_per_hit
            .unwrap_or(*damage)
            .saturating_mul(i32::from(*hits)),
        _ => 0,
    }
}

fn combat_potion_score(
    potion: PotionId,
    target_monster_index: Option<usize>,
    observation: CombatBaselineViewV1<'_>,
    exposed: i32,
) -> i64 {
    let speed_block_value = if potion == PotionId::SpeedPotion
        && !observation.player.powers.iter().any(|power| {
            power.power == crate::content::powers::PowerId::Artifact && power.amount > 0
        }) {
        // Temporary Dexterity must precede this turn's block plays. Estimate
        // only currently playable block; future draws remain unknown.
        let playable = observation
            .cards
            .hand
            .cards
            .iter()
            .filter(|card| {
                card.current_block > 0
                    && (card.free_to_play_once
                        || (card.effective_cost >= 0
                            && card.effective_cost <= i32::from(observation.turn.energy)))
            })
            .collect::<Vec<_>>();
        let base_block: i32 = playable.iter().map(|card| card.current_block).sum();
        let extra_block = get_potion_definition(potion)
            .base_potency
            .saturating_mul(playable.len() as i32);
        let useful_extra = extra_block.min(exposed.saturating_sub(base_block).max(0));
        if useful_extra <= 0 {
            return -200_000;
        }
        i64::from(useful_extra)
            * if observation.player.hp <= exposed.saturating_mul(2) {
                30
            } else {
                10
            }
    } else {
        0
    };
    let roles = potion_mechanic_roles(potion);
    let living = observation
        .monsters
        .iter()
        .filter(|monster| monster.alive)
        .count() as i64;
    let emergency = observation.player.hp <= exposed.saturating_add(8);
    let mut score = -650;
    if roles.contains(&PotionMechanicRole::DirectDamage) {
        if let Some(index) = target_monster_index {
            let Some(monster) = observation
                .monsters
                .get(index)
                .filter(|monster| monster.alive)
            else {
                return i64::MIN / 4;
            };
            let sacred_bark = observation
                .player
                .relics
                .iter()
                .any(|relic| relic.id == RelicId::SacredBark);
            let damage = get_potion_definition(potion)
                .base_potency
                .saturating_mul(if sacred_bark { 2 } else { 1 });
            let effective_hp = monster.hp.saturating_add(monster.block).max(0);
            score += i64::from(damage.min(effective_hp)) * 14;
            if damage >= effective_hp {
                score += 2_200 + i64::from(visible_monster_attack_damage(monster)) * 20;
            }
        } else {
            score += 450;
        }
    }
    if roles.contains(&PotionMechanicRole::MultiTarget) {
        score += living.saturating_sub(1) * 350;
    }
    if roles.contains(&PotionMechanicRole::GainBlock)
        || roles.contains(&PotionMechanicRole::GainIntangible)
        || roles.contains(&PotionMechanicRole::PreventDeath)
    {
        score += if exposed > 0 {
            i64::from(exposed) * 45
        } else {
            -500
        };
        if emergency {
            score += 4_000;
        }
    }
    if roles.contains(&PotionMechanicRole::Heal)
        && observation.player.hp * 2 < observation.player.max_hp
    {
        score += 1_500;
    }
    if roles.contains(&PotionMechanicRole::GainEnergy) && observation.turn.energy <= 1 {
        score += 550;
    }
    if roles.iter().any(|role| {
        matches!(
            role,
            PotionMechanicRole::ApplyWeak
                | PotionMechanicRole::ApplyVulnerable
                | PotionMechanicRole::GainStrength
                | PotionMechanicRole::GainDexterity
                | PotionMechanicRole::GainArtifact
                | PotionMechanicRole::GainRegeneration
                | PotionMechanicRole::GainPlatedArmor
                | PotionMechanicRole::GainThorns
                | PotionMechanicRole::GainMetallicize
                | PotionMechanicRole::GainRitual
        )
    }) {
        score += if observation.encounter.is_boss_fight || observation.encounter.is_elite_fight {
            650
        } else {
            100
        };
    }
    score + speed_block_value
}

fn selection_append_score(
    observation: LearningModelObservationV1<'_>,
    draft: &LearningSelectionDraftV1,
    domain_index: usize,
) -> i64 {
    if let Some(family) = draft.run_family() {
        let card_ordinal = family.domain_card_ordinal(domain_index);
        let LearningModelObservationV1::Strategic(strategic) = observation else {
            return -1_000;
        };
        return card_ordinal
            .map(|ordinal| {
                strategic_selection_card_score(
                    &StrategicBaselineViewV1::from_learning(strategic),
                    family.reason(),
                    ordinal,
                )
            })
            .unwrap_or(-500);
    }
    let Some(family) = draft.combat_family() else {
        return -1_000;
    };
    let Some(domain) = family.domain(domain_index) else {
        return -1_000;
    };
    public_combat_selection_domain_score(
        family.reason(),
        &match domain.semantics() {
            LearningCombatSelectionDomainSemanticsV1::Card {
                ordinal,
                card_id,
                upgrades,
                eligible,
            } => PublicCombatSelectionDomainCandidateV1::Card {
                ordinal,
                card_id,
                upgrades,
                eligible,
            },
            LearningCombatSelectionDomainSemanticsV1::Scry {
                index,
                card_id,
                currently_present,
            } => PublicCombatSelectionDomainCandidateV1::Scry {
                index,
                card_id,
                currently_present,
            },
        },
    )
}

fn public_combat_selection_domain_score(
    reason: &CombatSelectionReasonV2,
    domain: &PublicCombatSelectionDomainCandidateV1,
) -> i64 {
    let card_value = match domain {
        PublicCombatSelectionDomainCandidateV1::Card {
            card_id: Some(card),
            upgrades,
            eligible: true,
            ..
        } => {
            let definition = get_card_definition(*card);
            i64::from(definition.base_damage + definition.base_block + definition.base_magic)
                + i64::from(upgrades.unwrap_or(0)) * 20
                + if matches!(definition.card_type, CardType::Curse | CardType::Status) {
                    -1_000
                } else {
                    0
                }
        }
        PublicCombatSelectionDomainCandidateV1::Scry {
            card_id: Some(card),
            currently_present: true,
            ..
        } => {
            let definition = get_card_definition(*card);
            i64::from(definition.base_damage + definition.base_block + definition.base_magic)
                + if matches!(definition.card_type, CardType::Curse | CardType::Status) {
                    -1_000
                } else {
                    0
                }
        }
        _ => -1_000,
    };
    match reason {
        CombatSelectionReasonV2::ScryDiscard => -card_value,
        CombatSelectionReasonV2::Hand(reason) => match reason {
            HandSelectReason::Exhaust
            | HandSelectReason::PotionExhaust { .. }
            | HandSelectReason::Discard
            | HandSelectReason::Recycle => -card_value,
            HandSelectReason::Retain
            | HandSelectReason::Copy { .. }
            | HandSelectReason::Nightmare { .. }
            | HandSelectReason::Upgrade => card_value,
            HandSelectReason::PutOnDrawPile
            | HandSelectReason::PutToBottomOfDraw
            | HandSelectReason::Setup
            | HandSelectReason::GamblingChip => card_value / 2,
        },
        CombatSelectionReasonV2::Grid(reason) => match reason {
            GridSelectReason::Exhume { .. }
            | GridSelectReason::DrawPileToHand
            | GridSelectReason::SkillFromDeckToHand
            | GridSelectReason::AttackFromDeckToHand
            | GridSelectReason::DiscardToHand
            | GridSelectReason::DiscardToHandNoCostChange
            | GridSelectReason::DiscardToHandRetain
            | GridSelectReason::Omniscience { .. } => card_value,
            GridSelectReason::MoveToDrawPile => card_value / 2,
        },
    }
}

fn strategic_selection_card_score(
    view: &StrategicBaselineViewV1<'_>,
    reason: SelectionReason,
    ordinal: usize,
) -> i64 {
    let Some(card) = view.deck.get(ordinal) else {
        return -1_000;
    };
    if reason == SelectionReason::Upgrade {
        return campfire_smith_score(view, ordinal, card.id, card.upgrades);
    }
    selection_card_score(
        reason,
        strategic_card_value(view, card.id, card.upgrades),
        card.id,
    )
}

fn selection_card_score(reason: SelectionReason, value: i64, card: CardId) -> i64 {
    match reason {
        SelectionReason::Purge | SelectionReason::Exhaust | SelectionReason::Discard => {
            removal_value(card) - value
        }
        SelectionReason::Upgrade
        | SelectionReason::Duplicate
        | SelectionReason::BottleFlame
        | SelectionReason::BottleLightning
        | SelectionReason::BottleTornado
        | SelectionReason::Retain
        | SelectionReason::Copy
        | SelectionReason::Nightmare
        | SelectionReason::Exhume
        | SelectionReason::DrawPileToHand
        | SelectionReason::SkillFromDeckToHand
        | SelectionReason::AttackFromDeckToHand
        | SelectionReason::DiscardToHand
        | SelectionReason::DiscardToHandNoCostChange
        | SelectionReason::DiscardToHandRetain
        | SelectionReason::Omniscience => value,
        SelectionReason::Transform
        | SelectionReason::TransformUpgraded
        | SelectionReason::Recycle => -value,
        SelectionReason::PutOnDrawPile
        | SelectionReason::PutToBottomOfDraw
        | SelectionReason::Setup
        | SelectionReason::GamblingChip
        | SelectionReason::MoveToDrawPile
        | SelectionReason::Discovery => value / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::information::action::{
        resolve_public_combat_action_v1, CombatActionProjectionV1, PublicCombatActionChoiceV1,
    };
    use crate::agent::task::{
        task_public_combat_decision_v1, task_public_strategic_decision_v1, TaskBudgetV1,
    };
    use crate::ai::planner_core::{PlannerPublicMapEdge, PlannerPublicMapNode};
    use crate::content::cards::CardId;
    use crate::content::monsters::{factory::EncounterId, EnemyId};
    use crate::content::potions::{Potion, PotionId};
    use crate::eval::run_control::{
        LearningActionV1, LearningBoundaryV1, LearningEnvV3, LearningModelChoiceV1,
        LearningSelectionStepV1, RunControlConfig, RunControlSession,
    };
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{
        ActiveCombat, CombatContext, EngineState, PendingChoice, RoomCombatContext,
        RunPendingChoiceReason, RunPendingChoiceState,
    };
    use crate::state::map::node::RoomType;
    use crate::state::rewards::{RewardCard, RewardItem, RewardState};
    use crate::state::selection::SelectionResolution;
    use crate::state::DomainEventSource;

    fn selected_task_strategic_action(
        decision: &TaskPublicDecisionV1,
        choice: &TaskPublicChoiceV1,
    ) -> TaskStrategicActionV1 {
        match &decision.candidates[choice.ordinal_path[0]] {
            TaskPublicCandidateV1::Strategic { action } => action.clone(),
            candidate => panic!("expected strategic task candidate, got {candidate:?}"),
        }
    }

    fn selected_task_combat_action(
        decision: &TaskPublicDecisionV1,
        choice: &TaskPublicChoiceV1,
    ) -> PublicCombatAtomicActionV1 {
        match &decision.candidates[choice.ordinal_path[0]] {
            TaskPublicCandidateV1::CombatAtomic { action }
            | TaskPublicCandidateV1::CombatIndexedChoice { action, .. } => action.clone(),
            candidate => panic!("expected atomic combat task candidate, got {candidate:?}"),
        }
    }

    fn learning_combat_action(
        action: LearningCombatAtomicActionV1<'_>,
    ) -> PublicCombatAtomicActionV1 {
        match action {
            LearningCombatAtomicActionV1::PlayCard {
                hand_index,
                target_monster_index,
            } => PublicCombatAtomicActionV1::PlayCard {
                hand_index,
                target_monster_index,
            },
            LearningCombatAtomicActionV1::UsePotion {
                potion_index,
                target_monster_index,
            } => PublicCombatAtomicActionV1::UsePotion {
                potion_index,
                target_monster_index,
            },
            LearningCombatAtomicActionV1::DiscardPotion { potion_index } => {
                PublicCombatAtomicActionV1::DiscardPotion { potion_index }
            }
            LearningCombatAtomicActionV1::EndTurn => PublicCombatAtomicActionV1::EndTurn,
            LearningCombatAtomicActionV1::SubmitIndexedChoice { choice_index, .. } => {
                PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index }
            }
            LearningCombatAtomicActionV1::Proceed => PublicCombatAtomicActionV1::Proceed,
            LearningCombatAtomicActionV1::Cancel => PublicCombatAtomicActionV1::Cancel,
        }
    }

    fn strategic_card_reward_decision(
        deck: &[CardId],
        offered: &[CardId],
        boss: EncounterId,
    ) -> TaskPublicDecisionV1 {
        let cards = deck
            .iter()
            .enumerate()
            .map(|(ordinal, card)| crate::agent::task::TaskStrategicCardV1 {
                ordinal,
                card: *card,
                upgrades: 0,
                misc_value: 0,
                base_damage_override: None,
                base_block_override: None,
                cost_modifier: 0,
                bottled_by: None,
            })
            .collect();
        let observation = TaskStrategicObservationV1 {
            mechanics: crate::ai::planner_core::PlannerMechanicsManifest {
                mechanics_id: "public-marginal-construction-test".to_string(),
                mechanics_version: 1,
            },
            run_goal: crate::ai::planner_core::PlannerRunGoal::ActThreeVictory,
            decision_site: PlannerDecisionSite::CardReward,
            run: crate::ai::planner_core::PlannerRunScalars {
                player_class: PlannerPlayerClass::Ironclad,
                ascension_level: 0,
                act: 2,
                floor: 25,
                current_hp: 70,
                max_hp: 80,
                gold: 140,
                keys: [false; 3],
                potion_capacity: 3,
                pending_act_entry: None,
            },
            cards,
            relics: Vec::new(),
            potions: Vec::new(),
            public_map: PlannerPublicMap {
                current_x: 0,
                current_y: 8,
                boss: Some(boss),
                nodes: Vec::new(),
            },
            context: crate::ai::planner_core::PlannerDecisionContext::CardReward {
                reward_item_index: Some(0),
                pending_rewards: Vec::new(),
            },
            public_history: crate::ai::planner_core::PlannerPublicHistory {
                shop_purge_count: 0,
                recent_combat_hp_loss: Some(0),
                recent_combat_turns: Some(3),
                recent_combat_potions_used: Some(0),
            },
        };
        let mut candidates = offered
            .iter()
            .enumerate()
            .map(|(option_index, card)| TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::TakeCard {
                    reward_item_index: Some(0),
                    option_index,
                    card: *card,
                    upgrades: 0,
                },
            })
            .collect::<Vec<_>>();
        candidates.push(TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::SkipCardReward {
                reward_item_index: 0,
            },
        });
        TaskPublicDecisionV1::new(
            crate::agent::task::TaskDecisionSiteV1::CardReward,
            TaskPublicObservationV1::Strategic { observation },
            candidates,
        )
        .expect("public strategic card reward")
    }

    fn strategic_task_decision(
        site: crate::agent::task::TaskDecisionSiteV1,
        planner_site: PlannerDecisionSite,
        context: crate::ai::planner_core::PlannerDecisionContext,
        deck: &[(CardId, u8)],
        candidates: Vec<TaskStrategicActionV1>,
        hp: (i32, i32),
        gold: i32,
    ) -> TaskPublicDecisionV1 {
        let observation = TaskStrategicObservationV1 {
            mechanics: crate::ai::planner_core::PlannerMechanicsManifest {
                mechanics_id: "public-marginal-strategy-test".to_string(),
                mechanics_version: 1,
            },
            run_goal: crate::ai::planner_core::PlannerRunGoal::ActThreeVictory,
            decision_site: planner_site,
            run: crate::ai::planner_core::PlannerRunScalars {
                player_class: PlannerPlayerClass::Ironclad,
                ascension_level: 0,
                act: 1,
                floor: 12,
                current_hp: hp.0,
                max_hp: hp.1,
                gold,
                keys: [false; 3],
                potion_capacity: 3,
                pending_act_entry: None,
            },
            cards: deck
                .iter()
                .enumerate()
                .map(
                    |(ordinal, (card, upgrades))| crate::agent::task::TaskStrategicCardV1 {
                        ordinal,
                        card: *card,
                        upgrades: *upgrades,
                        misc_value: 0,
                        base_damage_override: None,
                        base_block_override: None,
                        cost_modifier: 0,
                        bottled_by: None,
                    },
                )
                .collect(),
            relics: Vec::new(),
            potions: Vec::new(),
            public_map: PlannerPublicMap {
                current_x: 0,
                current_y: 12,
                boss: Some(EncounterId::SlimeBoss),
                nodes: Vec::new(),
            },
            context,
            public_history: crate::ai::planner_core::PlannerPublicHistory {
                shop_purge_count: 0,
                recent_combat_hp_loss: Some(0),
                recent_combat_turns: Some(3),
                recent_combat_potions_used: Some(0),
            },
        };
        TaskPublicDecisionV1::new(
            site,
            TaskPublicObservationV1::Strategic { observation },
            candidates
                .into_iter()
                .map(|action| TaskPublicCandidateV1::Strategic { action })
                .collect(),
        )
        .expect("public strategic decision")
    }

    fn key_goal_observation(act: u8, keys: [bool; 3]) -> TaskStrategicObservationV1 {
        let decision = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Map,
            PlannerDecisionSite::Map,
            crate::ai::planner_core::PlannerDecisionContext::Map { overlay: false },
            &[(CardId::Bash, 0)],
            vec![TaskStrategicActionV1::Proceed {
                site: PlannerDecisionSite::Map,
            }],
            (70, 80),
            200,
        );
        let TaskPublicObservationV1::Strategic { mut observation } = decision.observation else {
            unreachable!();
        };
        observation.run_goal = PlannerRunGoal::HeartVictory;
        observation.run.act = act;
        observation.run.keys = keys;
        observation
    }

    fn key_map_node(
        x: i32,
        y: i32,
        room: PlannerMapRoom,
        green: bool,
        edges: &[(i32, i32)],
    ) -> PlannerPublicMapNode {
        PlannerPublicMapNode {
            x,
            y,
            room,
            has_emerald_key: green,
            edges: edges
                .iter()
                .map(|(x, y)| PlannerPublicMapEdge {
                    destination_x: *x,
                    destination_y: *y,
                })
                .collect(),
        }
    }

    #[test]
    fn heart_goal_claims_sapphire_in_act_three_without_sacrificing_early_relics() {
        use crate::ai::planner_core::PlannerRewardDescriptor as Reward;
        let actions = vec![
            (
                0,
                TaskStrategicActionV1::ClaimReward {
                    reward_item_index: 0,
                    reward: Reward::Relic {
                        relic: RelicId::Strawberry,
                    },
                },
            ),
            (
                1,
                TaskStrategicActionV1::ClaimReward {
                    reward_item_index: 1,
                    reward: Reward::SapphireKey,
                },
            ),
        ];
        for (act, goal, keys, expected) in [
            (1, PlannerRunGoal::HeartVictory, [false; 3], 0),
            (2, PlannerRunGoal::HeartVictory, [false; 3], 0),
            (3, PlannerRunGoal::HeartVictory, [false; 3], 1),
            (3, PlannerRunGoal::HeartVictory, [false, true, false], 0),
            (3, PlannerRunGoal::ActThreeVictory, [false; 3], 0),
        ] {
            let mut observation = key_goal_observation(act, keys);
            observation.run_goal = goal;
            observation.decision_site = PlannerDecisionSite::Reward;
            assert_eq!(
                best_strategic_action_ordinal(
                    &StrategicBaselineViewV1::from_task(&observation),
                    &actions,
                ),
                expected,
                "{act} {goal:?} {keys:?}"
            );
        }
    }

    #[test]
    fn heart_goal_recall_deadline_beats_upgrade_and_rest_but_can_be_deferred() {
        let mut observation = key_goal_observation(3, [false, true, true]);
        observation.decision_site = PlannerDecisionSite::Campfire;
        observation.run.current_hp = 1;
        observation.public_map.current_x = 0;
        observation.public_map.current_y = 12;
        observation.public_map.nodes = vec![
            key_map_node(0, 12, PlannerMapRoom::Rest, false, &[(0, 15)]),
            key_map_node(0, 15, PlannerMapRoom::Boss, false, &[]),
        ];
        let actions = vec![
            (
                0,
                TaskStrategicActionV1::Smith {
                    card_ordinal: 0,
                    card: CardId::Bash,
                    upgrades: 0,
                },
            ),
            (1, TaskStrategicActionV1::Rest),
            (2, TaskStrategicActionV1::Recall),
        ];
        assert_eq!(
            best_strategic_action_ordinal(
                &StrategicBaselineViewV1::from_task(&observation),
                &actions,
            ),
            2
        );
        observation.public_map.nodes[0].edges[0].destination_y = 14;
        observation.public_map.nodes.push(key_map_node(
            0,
            14,
            PlannerMapRoom::Rest,
            false,
            &[(0, 15)],
        ));
        assert_eq!(
            best_strategic_action_ordinal(
                &StrategicBaselineViewV1::from_task(&observation),
                &actions,
            ),
            1
        );
        observation.run_goal = PlannerRunGoal::ActThreeVictory;
        observation.run.current_hp = 80;
        assert_eq!(
            best_strategic_action_ordinal(
                &StrategicBaselineViewV1::from_task(&observation),
                &actions,
            ),
            0
        );
    }

    #[test]
    fn heart_goal_routes_require_keys_on_one_path_and_reward_exit_uses_same_constraint() {
        let mut observation = key_goal_observation(3, [false, true, false]);
        observation.public_map.nodes = vec![
            // The attractive shop reaches a campfire OR the green elite, not both.
            key_map_node(0, 10, PlannerMapRoom::Shop, false, &[(0, 11), (1, 11)]),
            key_map_node(0, 11, PlannerMapRoom::Rest, false, &[(0, 15)]),
            key_map_node(1, 11, PlannerMapRoom::Elite, true, &[(0, 15)]),
            key_map_node(2, 10, PlannerMapRoom::Elite, true, &[(2, 11)]),
            key_map_node(2, 11, PlannerMapRoom::Rest, false, &[(0, 15)]),
            key_map_node(0, 15, PlannerMapRoom::Boss, false, &[]),
        ];
        let actions = vec![
            (
                0,
                TaskStrategicActionV1::ChooseRouteNode {
                    x: 0,
                    y: 10,
                    flight: false,
                    question_mark_outcomes: Vec::new(),
                },
            ),
            (
                1,
                TaskStrategicActionV1::ChooseRouteNode {
                    x: 2,
                    y: 10,
                    flight: false,
                    question_mark_outcomes: Vec::new(),
                },
            ),
        ];
        let view = StrategicBaselineViewV1::from_task(&observation);
        assert!(
            strategic_action_score(&view, &actions[0].1)
                > strategic_action_score(&view, &actions[1].1)
        );
        assert_eq!(best_strategic_action_ordinal(&view, &actions), 1);
        assert_eq!(reward_exit_route_ordinal_v1(&view, &actions), Some(1));
        for keys in [[true, true, true], [true, true, false]] {
            observation.run.keys = keys;
            let view = StrategicBaselineViewV1::from_task(&observation);
            assert_eq!(best_strategic_action_ordinal(&view, &actions), 0);
        }
        observation.run.keys = [false, true, false];
        observation.run_goal = PlannerRunGoal::ActThreeVictory;
        assert_eq!(
            best_strategic_action_ordinal(
                &StrategicBaselineViewV1::from_task(&observation),
                &actions
            ),
            0
        );
    }

    #[test]
    fn heart_goal_incomplete_map_does_not_invent_key_opportunities_or_a_deadline() {
        let mut observation = key_goal_observation(3, [false; 3]);
        observation.public_map.nodes = vec![key_map_node(0, 12, PlannerMapRoom::Rest, false, &[])];
        observation.public_map.current_x = 0;
        observation.public_map.current_y = 12;
        let view = StrategicBaselineViewV1::from_task(&observation);
        assert_eq!(
            route_can_collect_keys(
                visible_route_pressure(&observation.public_map, 0, 12, true),
                7
            ),
            None
        );
        assert_eq!(
            heart_key_deadline_action(&view, &[(0, TaskStrategicActionV1::Recall)]),
            None
        );
    }

    #[test]
    fn capability_inventory_has_one_public_deterministic_owner() {
        let manifest = public_deterministic_baseline_manifest_v1();
        let capabilities = manifest.capabilities;
        assert_eq!(manifest.name, PUBLIC_DETERMINISTIC_BASELINE_NAME_V1);
        assert_eq!(manifest.revision, PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1);
        assert_eq!(manifest.lifecycle, "experimental");
        assert_eq!(
            manifest.information_access,
            PublicBaselineInformationAccessV1::PublicSemanticViewsOnly
        );
        assert!(manifest.deterministic && manifest.same_public_history_same_action);
        assert!(!capabilities.is_empty());
        assert!(capabilities.iter().all(|capability| {
            capability.owner == PUBLIC_DETERMINISTIC_BASELINE_NAME_V1
                && capability.deterministic
                && capability.information_access
                    == PublicBaselineInformationAccessV1::PublicSemanticViewsOnly
        }));
        assert!(manifest.limitations.iter().all(|limitation| capabilities
            .iter()
            .any(|capability| capability.limitation_ids.contains(&limitation.id))));
    }

    #[test]
    fn public_reward_skips_an_unsupported_entrench() {
        let decision = strategic_card_reward_decision(
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
                CardId::Cleave,
                CardId::PommelStrike,
            ],
            &[CardId::Entrench],
            EncounterId::Collector,
        );

        let selected = public_deterministic_task_decision_v1(&decision)
            .expect("public marginal construction decision");

        assert!(matches!(
            selected_task_strategic_action(&decision, &selected.choice),
            TaskStrategicActionV1::SkipCardReward { .. }
        ));
    }

    #[test]
    fn public_reward_takes_a_supported_boss_scaling_repair_over_skip() {
        let decision = strategic_card_reward_decision(
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
                CardId::Clothesline,
                CardId::BattleTrance,
                CardId::Armaments,
                CardId::FlameBarrier,
                CardId::Cleave,
                CardId::PowerThrough,
                CardId::Brutality,
                CardId::GhostlyArmor,
            ],
            &[CardId::DemonForm],
            EncounterId::TheChamp,
        );

        let selected = public_deterministic_task_decision_v1(&decision)
            .expect("public marginal construction decision");

        assert!(matches!(
            selected_task_strategic_action(&decision, &selected.choice),
            TaskStrategicActionV1::TakeCard {
                card: CardId::DemonForm,
                ..
            }
        ));
    }

    #[test]
    fn public_reward_skips_a_repeated_low_marginal_aoe() {
        let decision = strategic_card_reward_decision(
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
                CardId::BattleTrance,
                CardId::Armaments,
                CardId::PommelStrike,
                CardId::Cleave,
                CardId::Cleave,
            ],
            &[CardId::Cleave],
            EncounterId::TheChamp,
        );

        let selected = public_deterministic_task_decision_v1(&decision)
            .expect("public marginal construction decision");

        assert!(matches!(
            selected_task_strategic_action(&decision, &selected.choice),
            TaskStrategicActionV1::SkipCardReward { .. }
        ));
    }

    #[test]
    fn public_reward_takes_a_supported_corruption_component() {
        let decision = strategic_card_reward_decision(
            &[
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
                CardId::Immolate,
                CardId::Cleave,
                CardId::ShrugItOff,
                CardId::PommelStrike,
                CardId::Bloodletting,
                CardId::Armaments,
                CardId::Impervious,
                CardId::SpotWeakness,
            ],
            &[CardId::Corruption],
            EncounterId::Collector,
        );

        let selected = public_deterministic_task_decision_v1(&decision)
            .expect("public marginal construction decision");

        assert!(matches!(
            selected_task_strategic_action(&decision, &selected.choice),
            TaskStrategicActionV1::TakeCard {
                card: CardId::Corruption,
                ..
            }
        ));
    }

    #[test]
    fn public_campfire_upgrades_whirlwind_before_a_starter_strike() {
        let deck = [
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::Whirlwind, 0),
            (CardId::SecondWind, 0),
        ];
        let decision = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Campfire,
            PlannerDecisionSite::Campfire,
            crate::ai::planner_core::PlannerDecisionContext::Campfire,
            &deck,
            vec![
                TaskStrategicActionV1::Rest,
                TaskStrategicActionV1::Smith {
                    card_ordinal: 0,
                    card: CardId::Strike,
                    upgrades: 0,
                },
                TaskStrategicActionV1::Smith {
                    card_ordinal: 3,
                    card: CardId::Whirlwind,
                    upgrades: 0,
                },
            ],
            (66, 80),
            84,
        );

        let selected =
            public_deterministic_task_decision_v1(&decision).expect("public campfire decision");

        assert!(matches!(
            selected_task_strategic_action(&decision, &selected.choice),
            TaskStrategicActionV1::Smith {
                card: CardId::Whirlwind,
                ..
            }
        ));
    }

    #[test]
    fn public_campfire_does_not_rest_at_full_hp_when_smithing_is_legal() {
        let deck = [(CardId::Strike, 0), (CardId::Defend, 0), (CardId::Bash, 0)];
        let decision = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Campfire,
            PlannerDecisionSite::Campfire,
            crate::ai::planner_core::PlannerDecisionContext::Campfire,
            &deck,
            vec![
                TaskStrategicActionV1::Rest,
                TaskStrategicActionV1::Smith {
                    card_ordinal: 0,
                    card: CardId::Strike,
                    upgrades: 0,
                },
                TaskStrategicActionV1::Smith {
                    card_ordinal: 2,
                    card: CardId::Bash,
                    upgrades: 0,
                },
            ],
            (80, 80),
            84,
        );

        let selected =
            public_deterministic_task_decision_v1(&decision).expect("public campfire decision");

        assert!(matches!(
            selected_task_strategic_action(&decision, &selected.choice),
            TaskStrategicActionV1::Smith { .. }
        ));
    }

    #[test]
    fn public_shop_does_not_remove_an_upgraded_strike_before_a_basic_one() {
        let deck = [
            (CardId::Strike, 1),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::Whirlwind, 0),
            (CardId::Carnage, 0),
            (CardId::Immolate, 0),
            (CardId::PommelStrike, 0),
        ];
        let decision = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Shop,
            PlannerDecisionSite::Shop,
            crate::ai::planner_core::PlannerDecisionContext::Shop {
                purge_cost: 75,
                purge_available: true,
                inventory: Default::default(),
            },
            &deck,
            vec![
                TaskStrategicActionV1::RemoveCard {
                    card_ordinal: 0,
                    card: CardId::Strike,
                    upgrades: 1,
                    price: 75,
                },
                TaskStrategicActionV1::RemoveCard {
                    card_ordinal: 1,
                    card: CardId::Strike,
                    upgrades: 0,
                    price: 75,
                },
                TaskStrategicActionV1::LeaveShop,
            ],
            (73, 74),
            112,
        );

        let selected =
            public_deterministic_task_decision_v1(&decision).expect("public shop decision");

        assert!(matches!(
            selected_task_strategic_action(&decision, &selected.choice),
            TaskStrategicActionV1::RemoveCard {
                card_ordinal: 1,
                upgrades: 0,
                ..
            }
        ));
    }

    #[test]
    fn fresh_public_decision_is_stable() {
        let env = LearningEnvV3::new(RunControlConfig::default());
        let boundary = env.observe().expect("fresh boundary");
        let LearningBoundaryV1::Strategic { .. } = boundary else {
            panic!("fresh run must begin at a strategic boundary");
        };
        let decision = LearningModelDecisionV1::from_boundary(&boundary).expect("model decision");
        let first = public_deterministic_baseline_decision_v1(&decision);
        let second = public_deterministic_baseline_decision_v1(&decision);
        assert_eq!(first, second);
        assert!(first.candidate_ordinal < decision.candidates.len());
    }

    #[test]
    fn task_and_model_baselines_choose_the_same_public_reward_semantics() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Gold { amount: 12 },
            RewardItem::Potion {
                potion_id: PotionId::SmokeBomb,
            },
            RewardItem::Card {
                cards: vec![RewardCard::new(CardId::PommelStrike, 0)],
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("reward boundary");
        let LearningBoundaryV1::Strategic {
            boundary: strategic,
        } = &boundary
        else {
            panic!("expected strategic reward boundary");
        };

        let model = LearningModelDecisionV1::from_boundary(&boundary).expect("model decision");
        let model_choice = public_deterministic_baseline_decision_v1(&model);
        let LearningModelCandidateSemanticsV1::Strategic { action } =
            model.candidates[model_choice.candidate_ordinal].semantics
        else {
            panic!("expected strategic model candidate");
        };
        let model_action = action.semantics().expect("public model semantics");

        let task =
            task_public_strategic_decision_v1(&strategic.observation, &strategic.legal_candidates)
                .expect("task decision");
        assert!(task.candidates.len() > model.candidates.len());
        let task_choice = public_deterministic_task_decision_v1(&task).expect("task baseline");

        assert_eq!(
            selected_task_strategic_action(&task, &task_choice.choice),
            model_action
        );
        assert!(matches!(
            model_action,
            TaskStrategicActionV1::ClaimReward {
                reward: crate::ai::planner_core::PlannerRewardDescriptor::Gold { amount: 12 },
                ..
            }
        ));
    }

    #[test]
    fn task_and_model_share_discard_deferral_only_while_card_choice_is_open() {
        for card_choice_open in [false, true] {
            let mut session = RunControlSession::new(RunControlConfig::default());
            session.run_state.potions = vec![
                Some(Potion::new(PotionId::SmokeBomb, 1)),
                Some(Potion::new(PotionId::BlockPotion, 2)),
            ];
            let cards = vec![RewardCard::new(CardId::PommelStrike, 0)];
            let mut reward = RewardState::new();
            reward.items = vec![
                RewardItem::Card {
                    cards: cards.clone(),
                },
                RewardItem::Potion {
                    potion_id: PotionId::StrengthPotion,
                },
            ];
            if card_choice_open {
                reward.pending_card_choice = Some(cards);
                reward.pending_card_reward_index = Some(0);
            }
            session.engine_state = EngineState::RewardScreen(reward);
            let boundary = LearningEnvV3::from_session(session).observe().unwrap();
            let LearningBoundaryV1::Strategic {
                boundary: strategic,
            } = &boundary
            else {
                panic!("expected reward boundary");
            };
            let model = LearningModelDecisionV1::from_boundary(&boundary).unwrap();
            let model_actions = model
                .candidates
                .iter()
                .map(|candidate| {
                    let LearningModelCandidateSemanticsV1::Strategic { action } =
                        candidate.semantics
                    else {
                        panic!("expected strategic model candidate");
                    };
                    action.semantics().unwrap()
                })
                .collect::<Vec<_>>();
            let task = task_public_strategic_decision_v1(
                &strategic.observation,
                &strategic.legal_candidates,
            )
            .unwrap();
            assert!(task.candidates.iter().any(|candidate| matches!(
                candidate,
                TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::DiscardRunPotion { .. }
                }
            )));
            let TaskPublicObservationV1::Strategic { observation } = &task.observation else {
                panic!("expected strategic task observation");
            };
            let task_actions =
                admitted_task_strategic_candidate_ordinals_v1(observation, &task.candidates)
                    .into_iter()
                    .map(|ordinal| match &task.candidates[ordinal] {
                        TaskPublicCandidateV1::Strategic { action } => action.clone(),
                        _ => panic!("expected strategic task candidate"),
                    })
                    .collect::<Vec<_>>();
            assert_eq!(task_actions, model_actions);
            assert_eq!(
                task_actions
                    .iter()
                    .any(|action| matches!(action, TaskStrategicActionV1::DiscardRunPotion { .. })),
                !card_choice_open
            );
        }
    }

    #[test]
    fn public_reward_claims_a_free_potion_before_proceeding_when_several_are_offered() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut reward = RewardState::new();
        reward.items = vec![
            RewardItem::Potion {
                potion_id: PotionId::SwiftPotion,
            },
            RewardItem::Potion {
                potion_id: PotionId::SpeedPotion,
            },
        ];
        session.engine_state = EngineState::RewardScreen(reward);
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("reward boundary");
        let LearningBoundaryV1::Strategic {
            boundary: strategic,
        } = &boundary
        else {
            panic!("expected strategic reward boundary");
        };
        let decision =
            task_public_strategic_decision_v1(&strategic.observation, &strategic.legal_candidates)
                .expect("task decision");
        let selected =
            public_deterministic_task_decision_v1(&decision).expect("public reward decision");
        assert!(matches!(
            selected_task_strategic_action(&decision, &selected.choice),
            TaskStrategicActionV1::ClaimReward {
                reward: crate::ai::planner_core::PlannerRewardDescriptor::Potion { .. },
                ..
            }
        ));
    }

    #[test]
    fn task_and_model_baselines_choose_the_same_public_combat_semantics() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Bash, 11),
            CombatCard::for_scenario(CardId::Strike, 12),
            CombatCard::for_scenario(CardId::Defend, 13),
        ];
        combat.entities.potions = vec![
            Some(Potion::new(PotionId::BlockPotion, 21)),
            Some(Potion::new(PotionId::SkillPotion, 22)),
        ];
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        combat.entities.monsters.push(monster);
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let LearningBoundaryV1::Combat { boundary: combat } = &boundary else {
            panic!("expected combat boundary");
        };

        let model = LearningModelDecisionV1::from_boundary(&boundary).expect("model decision");
        let model_choice = public_deterministic_baseline_decision_v1(&model);
        let LearningModelCandidateSemanticsV1::CombatAtomic { action } =
            model.candidates[model_choice.candidate_ordinal].semantics
        else {
            panic!("fixture should choose an atomic combat action");
        };
        let model_action = learning_combat_action(action);

        let task = task_public_combat_decision_v1(
            combat.public_run_context.clone(),
            combat.observation.clone(),
            &combat.public_actions,
        )
        .expect("task combat decision");
        assert!(task.candidates.len() > model.candidates.len());
        let task_choice = public_deterministic_task_decision_v1(&task).expect("task baseline");

        assert_eq!(
            selected_task_combat_action(&task, &task_choice.choice),
            model_action
        );
        assert!(!matches!(
            model_action,
            PublicCombatAtomicActionV1::DiscardPotion { .. }
        ));
    }

    #[test]
    fn sleeping_strike_and_speed_potion_respect_current_turn() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Defend, 12),
        ];
        combat
            .entities
            .monsters
            .push(crate::test_support::test_monster(EnemyId::Lagavulin));
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomElite,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let LearningBoundaryV1::Combat { boundary: combat } = boundary else {
            panic!("expected combat boundary");
        };
        let mut observation = combat.observation;
        observation.encounter.is_elite_fight = true;
        observation.monsters[0].hp = 100;
        observation.monsters[0].block = 8;
        observation.monsters[0].intent.intent = Some(Intent::Sleep);
        let strike = CombatBaselineCandidateV1::PlayCard {
            hand_index: 0,
            target_monster_index: Some(0),
        };
        assert!(
            combat_atomic_score(CombatBaselineViewV1::from_task(&observation), strike)
                < combat_atomic_score(
                    CombatBaselineViewV1::from_task(&observation),
                    CombatBaselineCandidateV1::EndTurn
                )
        );

        observation.monsters[0].intent.intent = Some(Intent::Attack {
            damage: 20,
            hits: 1,
        });
        assert!(
            combat_atomic_score(CombatBaselineViewV1::from_task(&observation), strike)
                > combat_atomic_score(
                    CombatBaselineViewV1::from_task(&observation),
                    CombatBaselineCandidateV1::EndTurn
                )
        );

        observation.turn.energy = 1;
        observation.player.hp = 35;
        assert!(
            combat_potion_score(
                PotionId::SpeedPotion,
                None,
                CombatBaselineViewV1::from_task(&observation),
                20,
            ) > combat_atomic_score(
                CombatBaselineViewV1::from_task(&observation),
                CombatBaselineCandidateV1::PlayCard {
                    hand_index: 1,
                    target_monster_index: None,
                },
            )
        );
        observation.turn.energy = 0;
        assert!(
            combat_potion_score(
                PotionId::SpeedPotion,
                None,
                CombatBaselineViewV1::from_task(&observation),
                20,
            ) < combat_atomic_score(
                CombatBaselineViewV1::from_task(&observation),
                CombatBaselineCandidateV1::EndTurn,
            )
        );
    }

    #[test]
    fn visible_hand_block_prevents_an_avoidable_lethal_end_turn() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 11),
            CombatCard::for_scenario(CardId::Defend, 12),
            CombatCard::for_scenario(CardId::Defend, 13),
            CombatCard::for_scenario(CardId::Bash, 14),
            CombatCard::for_scenario(CardId::Whirlwind, 15),
        ];
        combat
            .entities
            .monsters
            .push(crate::test_support::test_monster(EnemyId::Lagavulin));
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomElite,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let LearningBoundaryV1::Combat { boundary: combat } = boundary else {
            panic!("expected combat boundary");
        };
        let mut observation = combat.observation;
        observation.player.hp = 15;
        observation.player.block = 0;
        observation.turn.energy = 3;
        observation.monsters[0].hp = 80;
        observation.monsters[0].intent.intent = Some(Intent::Attack {
            damage: 20,
            hits: 1,
        });
        observation.monsters[0].intent.preview_damage_per_hit = Some(20);
        for card in &mut observation.cards.hand.cards[..3] {
            card.current_block = 3;
        }
        let view = CombatBaselineViewV1::from_task(&observation);
        let defend = CombatBaselineCandidateV1::PlayCard {
            hand_index: 0,
            target_monster_index: None,
        };
        let bash = CombatBaselineCandidateV1::PlayCard {
            hand_index: 3,
            target_monster_index: Some(0),
        };
        let whirlwind = CombatBaselineCandidateV1::PlayCard {
            hand_index: 4,
            target_monster_index: None,
        };
        assert!(combat_atomic_score(view, defend) > combat_atomic_score(view, bash));
        assert!(combat_atomic_score(view, defend) > combat_atomic_score(view, whirlwind));

        observation.monsters[0].hp = 4;
        let view = CombatBaselineViewV1::from_task(&observation);
        assert!(combat_atomic_score(view, bash) > combat_atomic_score(view, defend));

        observation.monsters[0].hp = 80;
        observation.monsters[0].intent.preview_damage_per_hit = Some(30);
        let view = CombatBaselineViewV1::from_task(&observation);
        assert_eq!(visible_turn_survival_priority(view, 0, None, 0, 30), 0);
    }

    #[test]
    fn combat_priority_uses_public_preview_damage_instead_of_raw_intent_damage() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 11),
            CombatCard::for_scenario(CardId::Strike, 12),
        ];
        combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 21))];
        combat
            .entities
            .monsters
            .push(crate::test_support::test_monster(EnemyId::JawWorm));
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let LearningBoundaryV1::Combat { boundary: combat } = &boundary else {
            panic!("expected combat boundary");
        };
        let mut observation = combat.observation.clone();
        observation.player.hp = 17;
        observation.player.max_hp = 75;
        observation.player.block = 0;
        observation.turn.energy = 3;
        observation.monsters[0].intent.intent = Some(Intent::Attack { damage: 6, hits: 1 });
        observation.monsters[0].intent.preview_damage_per_hit = Some(16);
        assert_eq!(
            visible_incoming_damage(CombatBaselineViewV1::from_task(&observation)),
            16
        );
        let endangered = task_public_combat_decision_v1(
            combat.public_run_context.clone(),
            observation.clone(),
            &combat.public_actions,
        )
        .expect("endangered public decision");
        let choice = public_deterministic_task_decision_v1(&endangered).expect("task actor");
        assert!(matches!(
            selected_task_combat_action(&endangered, &choice.choice),
            PublicCombatAtomicActionV1::UsePotion {
                potion_index: 0,
                ..
            }
        ));

        observation.player.hp = 60;
        let healthy = task_public_combat_decision_v1(
            combat.public_run_context.clone(),
            observation.clone(),
            &combat.public_actions,
        )
        .expect("healthy public decision");
        let choice = public_deterministic_task_decision_v1(&healthy).expect("task actor");
        assert!(!matches!(
            selected_task_combat_action(&healthy, &choice.choice),
            PublicCombatAtomicActionV1::UsePotion { .. }
        ));
        observation.monsters[0].intent.preview_damage_per_hit = None;
        assert_eq!(
            visible_incoming_damage(CombatBaselineViewV1::from_task(&observation)),
            6
        );
    }

    #[test]
    fn fire_potion_targets_a_publicly_killable_attacker_instead_of_the_first_enemy() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand.clear();
        combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 21))];
        let mut first = crate::test_support::test_monster(EnemyId::GremlinWarrior);
        first.current_hp = 23;
        first.max_hp = 23;
        let mut second = crate::test_support::test_monster(EnemyId::GremlinFat);
        second.id = 2;
        second.slot = 1;
        second.current_hp = 15;
        second.max_hp = 15;
        combat.entities.monsters = vec![first, second];
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let LearningBoundaryV1::Combat { boundary: combat } = boundary else {
            panic!("expected combat boundary");
        };
        let mut observation = combat.observation;
        observation.monsters[0].intent.intent = Some(Intent::Attack { damage: 5, hits: 1 });
        observation.monsters[0].intent.preview_damage_per_hit = Some(5);
        observation.monsters[1].intent.intent = Some(Intent::Attack { damage: 5, hits: 1 });
        observation.monsters[1].intent.preview_damage_per_hit = Some(5);
        let decision = task_public_combat_decision_v1(
            combat.public_run_context,
            observation,
            &combat.public_actions,
        )
        .expect("public combat decision");
        let choice = public_deterministic_task_decision_v1(&decision).expect("task actor");
        assert_eq!(
            selected_task_combat_action(&decision, &choice.choice),
            PublicCombatAtomicActionV1::UsePotion {
                potion_index: 0,
                target_monster_index: Some(1),
            }
        );
    }

    #[test]
    fn a_two_hp_enemy_does_not_make_bash_a_better_killing_blow_than_strike() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Bash, 11),
            CombatCard::for_scenario(CardId::Strike, 12),
            CombatCard::for_scenario(CardId::Defend, 13),
        ];
        let mut first = crate::test_support::test_monster(EnemyId::Sentry);
        first.current_hp = 2;
        first.max_hp = 40;
        let mut second = crate::test_support::test_monster(EnemyId::Sentry);
        second.id = 2;
        second.slot = 1;
        second.current_hp = 40;
        second.max_hp = 40;
        combat.entities.monsters = vec![first, second];
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let LearningBoundaryV1::Combat { boundary: combat } = boundary else {
            panic!("expected combat boundary");
        };
        let decision = task_public_combat_decision_v1(
            combat.public_run_context,
            combat.observation,
            &combat.public_actions,
        )
        .expect("public combat decision");
        let choice = public_deterministic_task_decision_v1(&decision).expect("task actor");
        assert_eq!(
            selected_task_combat_action(&decision, &choice.choice),
            PublicCombatAtomicActionV1::PlayCard {
                hand_index: 1,
                target_monster_index: Some(0),
            }
        );
    }

    #[test]
    fn a_kill_prevents_the_more_dangerous_public_attack() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Carnage, 11)];
        let mut thief = crate::test_support::test_monster(EnemyId::GremlinThief);
        thief.current_hp = 14;
        thief.max_hp = 14;
        let mut fat = crate::test_support::test_monster(EnemyId::GremlinFat);
        fat.id = 2;
        fat.slot = 1;
        fat.current_hp = 17;
        fat.max_hp = 17;
        combat.entities.monsters = vec![thief, fat];
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat boundary");
        let LearningBoundaryV1::Combat { boundary: combat } = boundary else {
            panic!("expected combat boundary");
        };
        let mut observation = combat.observation;
        observation.monsters[0].intent.intent = Some(Intent::Attack {
            damage: 10,
            hits: 1,
        });
        observation.monsters[0].intent.preview_damage_per_hit = Some(10);
        observation.monsters[1].intent.intent = Some(Intent::AttackDebuff { damage: 5, hits: 1 });
        observation.monsters[1].intent.preview_damage_per_hit = Some(5);
        let decision = task_public_combat_decision_v1(
            combat.public_run_context,
            observation,
            &combat.public_actions,
        )
        .expect("public combat decision");
        let choice = public_deterministic_task_decision_v1(&decision).expect("task actor");
        assert_eq!(
            selected_task_combat_action(&decision, &choice.choice),
            PublicCombatAtomicActionV1::PlayCard {
                hand_index: 0,
                target_monster_index: Some(0),
            }
        );
    }

    #[test]
    fn concrete_run_selections_use_the_effect_and_agree_with_task_choices() {
        use crate::state::selection::SelectionScope;
        for (reason, expected) in [
            (RunPendingChoiceReason::Duplicate, 2),
            (RunPendingChoiceReason::Upgrade, 2),
            (RunPendingChoiceReason::Purge, 3),
            (RunPendingChoiceReason::Transform, 3),
        ] {
            let mut session = RunControlSession::new(RunControlConfig::default());
            session.run_state.master_deck = vec![
                CombatCard::for_scenario(CardId::AscendersBane, 1),
                CombatCard::for_scenario(CardId::Strike, 2),
                CombatCard::for_scenario(CardId::Whirlwind, 3),
                CombatCard::for_scenario(CardId::Pain, 4),
            ];
            session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                min_choices: 1,
                max_choices: 1,
                reason,
                source: DomainEventSource::Selection(reason.into()),
                return_state: Box::new(EngineState::MapNavigation),
            });
            let boundary = LearningEnvV3::from_session(session).observe().unwrap();
            let LearningBoundaryV1::Strategic {
                boundary: strategic,
            } = &boundary
            else {
                panic!("expected strategic selection");
            };
            assert_eq!(
                strategic.observation.context,
                crate::ai::planner_core::PlannerDecisionContext::RunChoice {
                    scope: SelectionScope::Deck,
                    reason: reason.into(),
                    source: Some(DomainEventSource::Selection(reason.into())),
                    min_choices: 1,
                    max_choices: 1,
                }
            );
            let model = LearningModelDecisionV1::from_boundary(&boundary).unwrap();
            let chosen = public_deterministic_baseline_decision_v1(&model);
            let LearningModelCandidateSemanticsV1::Strategic { action } =
                model.candidates[chosen.candidate_ordinal].semantics
            else {
                panic!("expected strategic action");
            };
            let selected = action.semantics().unwrap();
            assert_eq!(
                selected,
                TaskStrategicActionV1::SubmitRunSelection {
                    scope: SelectionScope::Deck,
                    selected_card_ordinals: vec![expected],
                },
                "{reason:?}"
            );
            let task = task_public_strategic_decision_v1(
                &strategic.observation,
                &strategic.legal_candidates,
            )
            .unwrap();
            let task_chosen = public_deterministic_task_decision_v1(&task).unwrap();
            assert_eq!(
                selected_task_strategic_action(&task, &task_chosen.choice),
                selected
            );
        }
    }

    #[test]
    fn public_event_known_card_effects_do_not_collapse_to_counts() {
        use crate::ai::planner_core::{
            PlannerCardObservation, PlannerEventCardPoolV1, PlannerEventOptionPublicSemanticsV1,
            PlannerEventOptionStructuredSemanticsV1, PlannerEventPublicEffectV1,
            PlannerEventTransitionV1, PlannerOfferedCard,
        };
        use crate::state::events::EventId;

        for (event, removing, optional, deck) in [
            (
                EventId::Falling,
                true,
                false,
                [(CardId::Offering, 0), (CardId::Strike, 0)],
            ),
            (
                EventId::Falling,
                true,
                false,
                [(CardId::ShrugItOff, 1), (CardId::ShrugItOff, 0)],
            ),
            (
                EventId::TheLibrary,
                false,
                false,
                [(CardId::Anger, 0), (CardId::Offering, 0)],
            ),
            (
                EventId::Falling,
                true,
                true,
                [(CardId::Offering, 0), (CardId::ShrugItOff, 1)],
            ),
        ] {
            let mut actions = deck
                .iter()
                .enumerate()
                .map(|(index, &(card, upgrades))| {
                    let mut public = PlannerEventOptionPublicSemanticsV1::default();
                    if removing {
                        public.cards_removed = 1;
                        public.specific_cards_removed.push(PlannerCardObservation {
                            card_lineage: CombatCard::for_scenario(card, index as u32 + 1).lineage,
                            card,
                            upgrades,
                            misc_value: 0,
                            base_damage_override: None,
                            base_block_override: None,
                            cost_modifier: 0,
                        });
                    } else {
                        public.cards_gained = 1;
                        public
                            .specific_cards_gained
                            .push(PlannerOfferedCard { card, upgrades });
                        public.structured_semantics =
                            Some(PlannerEventOptionStructuredSemanticsV1 {
                                effects: vec![PlannerEventPublicEffectV1::ObtainCard {
                                    count: 1,
                                    pool: PlannerEventCardPoolV1::Specific { card, upgrades },
                                }],
                                transition: PlannerEventTransitionV1::Complete,
                            });
                    }
                    TaskStrategicActionV1::ChooseEventOption {
                        event,
                        screen: 1,
                        option_index: index,
                        action: EventActionKind::DeckOperation,
                        public_semantics: public,
                    }
                })
                .collect::<Vec<_>>();
            // A synthetic optional-removal surface checks that the operation
            // label itself supplies no positive value for losing useful cards.
            if optional {
                actions.push(TaskStrategicActionV1::ChooseEventOption {
                    event,
                    screen: 1,
                    option_index: 2,
                    action: EventActionKind::Leave,
                    public_semantics: PlannerEventOptionPublicSemanticsV1::default(),
                });
            }
            for reverse in [false, true] {
                let mut candidates = actions.clone();
                if reverse {
                    candidates.reverse();
                }
                let decision = strategic_task_decision(
                    crate::agent::task::TaskDecisionSiteV1::Event,
                    PlannerDecisionSite::Event,
                    crate::ai::planner_core::PlannerDecisionContext::Event {
                        event,
                        screen: 1,
                        face_trader_result: None,
                    },
                    &deck,
                    candidates,
                    (72, 72),
                    23,
                );
                let selected = public_deterministic_task_decision_v1(&decision).unwrap();
                let expected = if optional { 2 } else { 1 };
                assert!(
                    matches!(
                        selected_task_strategic_action(&decision, &selected.choice),
                        TaskStrategicActionV1::ChooseEventOption { option_index, .. } if option_index == expected
                    ),
                    "{event:?}, reverse={reverse}"
                );
            }
        }
    }

    #[test]
    fn public_event_card_kind_summary_does_not_erase_typed_gain_multiplicity() {
        use crate::ai::planner_core::{
            PlannerEventCardPoolV1, PlannerEventOptionPublicSemanticsV1,
            PlannerEventOptionStructuredSemanticsV1, PlannerEventPublicEffectV1,
            PlannerEventTransitionV1, PlannerOfferedCard,
        };
        use crate::state::events::EventId;

        let decision = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Event,
            PlannerDecisionSite::Event,
            crate::ai::planner_core::PlannerDecisionContext::Event {
                event: EventId::Vampires,
                screen: 0,
                face_trader_result: None,
            },
            &[],
            vec![TaskStrategicActionV1::Proceed {
                site: PlannerDecisionSite::Event,
            }],
            (72, 72),
            0,
        );
        let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
            panic!("expected strategic observation");
        };
        let view = StrategicBaselineViewV1::from_task(observation);
        let gain = |count| PlannerEventOptionPublicSemanticsV1 {
            cards_gained: count,
            specific_cards_gained: vec![PlannerOfferedCard {
                card: CardId::Bite,
                upgrades: 0,
            }],
            structured_semantics: Some(PlannerEventOptionStructuredSemanticsV1 {
                effects: vec![PlannerEventPublicEffectV1::ObtainCard {
                    count,
                    pool: PlannerEventCardPoolV1::Specific {
                        card: CardId::Bite,
                        upgrades: 0,
                    },
                }],
                transition: PlannerEventTransitionV1::Complete,
            }),
            ..Default::default()
        };
        assert_eq!(
            event_action_score(
                EventId::Vampires,
                0,
                EventActionKind::DeckOperation,
                &gain(5),
                &view
            ),
            event_action_score(
                EventId::Vampires,
                0,
                EventActionKind::DeckOperation,
                &gain(1),
                &view
            ) * 5
        );
    }

    #[test]
    fn free_event_offerings_share_card_reward_construction_judgment() {
        use crate::ai::planner_core::{
            PlannerEventCardPoolV1, PlannerEventOptionPublicSemanticsV1,
            PlannerEventOptionStructuredSemanticsV1, PlannerEventPublicEffectV1,
            PlannerEventTransitionV1,
        };
        use crate::state::events::EventId;

        let deck = [
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::DemonForm, 0),
            (CardId::ShrugItOff, 1),
            (CardId::Reaper, 0),
        ];
        let offered = [CardId::SeverSoul, CardId::Inflame, CardId::SwordBoomerang];
        let event_actions = offered
            .iter()
            .enumerate()
            .map(
                |(option_index, &card)| TaskStrategicActionV1::ChooseEventOption {
                    event: EventId::TheLibrary,
                    screen: 1,
                    option_index,
                    action: EventActionKind::Gain,
                    public_semantics: PlannerEventOptionPublicSemanticsV1 {
                        cards_gained: 1,
                        structured_semantics: Some(PlannerEventOptionStructuredSemanticsV1 {
                            effects: vec![PlannerEventPublicEffectV1::ObtainCard {
                                count: 1,
                                pool: PlannerEventCardPoolV1::Specific { card, upgrades: 0 },
                            }],
                            transition: PlannerEventTransitionV1::AdvanceScreen,
                        }),
                        ..Default::default()
                    },
                },
            )
            .collect::<Vec<_>>();
        let reward = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::CardReward,
            PlannerDecisionSite::CardReward,
            crate::ai::planner_core::PlannerDecisionContext::CardReward {
                reward_item_index: None,
                pending_rewards: Vec::new(),
            },
            &deck,
            offered
                .iter()
                .enumerate()
                .map(|(option_index, &card)| TaskStrategicActionV1::TakeCard {
                    reward_item_index: None,
                    option_index,
                    card,
                    upgrades: 0,
                })
                .collect(),
            (56, 72),
            226,
        );
        let event = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Event,
            PlannerDecisionSite::Event,
            crate::ai::planner_core::PlannerDecisionContext::Event {
                event: EventId::TheLibrary,
                screen: 1,
                face_trader_result: None,
            },
            &deck,
            event_actions.clone(),
            (56, 72),
            226,
        );
        let reward_choice = public_deterministic_task_decision_v1(&reward).unwrap();
        let event_choice = public_deterministic_task_decision_v1(&event).unwrap();
        assert_eq!(
            reward_choice.choice.ordinal_path,
            event_choice.choice.ordinal_path
        );
        let mut costly = event_actions[0].clone();
        let TaskStrategicActionV1::ChooseEventOption {
            public_semantics, ..
        } = &mut costly
        else {
            unreachable!();
        };
        public_semantics
            .structured_semantics
            .as_mut()
            .unwrap()
            .effects
            .push(PlannerEventPublicEffectV1::LoseHp { amount: 5 });
        assert!(free_event_card_gain(&costly).is_none());
    }

    #[test]
    fn task_symbolic_payload_matches_the_stepwise_model_decoder() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Defend, 22),
            CombatCard::for_scenario(CardId::Bash, 33),
        ];
        session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 3,
            reason: RunPendingChoiceReason::Transform,
            source: DomainEventSource::Selection(SelectionReason::Transform),
            return_state: Box::new(EngineState::MapNavigation),
        });
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("run selection boundary");
        let LearningBoundaryV1::Strategic {
            boundary: strategic,
        } = &boundary
        else {
            panic!("expected strategic selection boundary");
        };
        let model = LearningModelDecisionV1::from_boundary(&boundary).expect("model decision");
        let root = public_deterministic_baseline_decision_v1(&model);
        let LearningModelChoiceV1::DecodeSelection(mut draft) =
            model.choose(root.candidate_ordinal).expect("start decoder")
        else {
            panic!("expected symbolic model decoder");
        };
        loop {
            let selected =
                public_deterministic_baseline_selection_decision_v1(model.observation, &draft);
            if matches!(
                draft
                    .choose(selected.candidate_ordinal)
                    .expect("decode step"),
                LearningSelectionStepV1::Apply(LearningActionV1::RunSelection {
                    resolution: SelectionResolution { .. },
                    ..
                })
            ) {
                break;
            }
        }
        let model_payload = draft.selected_domain_indices().to_vec();

        let task =
            task_public_strategic_decision_v1(&strategic.observation, &strategic.legal_candidates)
                .expect("task selection decision");
        let task_choice = public_deterministic_task_decision_v1(&task).expect("task baseline");

        assert_eq!(&task_choice.choice.ordinal_path[1..], model_payload);
        assert!((1..=3).contains(&model_payload.len()));
    }

    #[test]
    fn task_combat_symbolic_payload_matches_the_stepwise_model_decoder() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Defend, 22),
        ];
        let choice = PendingChoice::HandSelect {
            candidate_lineages: crate::test_support::scenario_lineages([11, 22]),
            min_cards: 1,
            max_cards: 2,
            can_cancel: false,
            reason: HandSelectReason::Discard,
        };
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::PendingChoice(choice.clone());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::PendingChoice(choice),
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("combat selection boundary");
        let LearningBoundaryV1::Combat { boundary: combat } = &boundary else {
            panic!("expected combat selection boundary");
        };
        let model = LearningModelDecisionV1::from_boundary(&boundary).expect("model decision");
        let root = public_deterministic_baseline_decision_v1(&model);
        let LearningModelChoiceV1::DecodeSelection(mut draft) =
            model.choose(root.candidate_ordinal).expect("start decoder")
        else {
            panic!("expected combat symbolic decoder");
        };
        loop {
            let selected =
                public_deterministic_baseline_selection_decision_v1(model.observation, &draft);
            if matches!(
                draft
                    .choose(selected.candidate_ordinal)
                    .expect("decode step"),
                LearningSelectionStepV1::Apply(LearningActionV1::CombatInput { .. })
            ) {
                break;
            }
        }
        let model_payload = draft.selected_domain_indices().to_vec();
        let task = task_public_combat_decision_v1(
            combat.public_run_context.clone(),
            combat.observation.clone(),
            &combat.public_actions,
        )
        .expect("task combat selection decision");
        let task_choice = public_deterministic_task_decision_v1(&task).expect("task baseline");

        assert_eq!(&task_choice.choice.ordinal_path[1..], model_payload);
    }

    #[test]
    fn task_scry_payload_is_publicly_conservative_for_duplicate_private_occurrences() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::AscendersBane, 7),
            CombatCard::for_scenario(CardId::Defend, 8),
        ]
        .into();
        let choice = PendingChoice::ScrySelect {
            cards: vec![CardId::AscendersBane, CardId::AscendersBane, CardId::Defend],
            card_lineages: crate::test_support::scenario_lineages([7, 7, 8]),
        };
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::PendingChoice(choice.clone());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::PendingChoice(choice),
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let boundary = LearningEnvV3::from_session(session)
            .observe()
            .expect("duplicate Scry boundary");
        let LearningBoundaryV1::Combat { boundary: combat } = &boundary else {
            panic!("expected duplicate Scry combat boundary");
        };
        let task = task_public_combat_decision_v1(
            combat.public_run_context.clone(),
            combat.observation.clone(),
            &combat.public_actions,
        )
        .expect("task Scry decision");
        let task_choice = public_deterministic_task_decision_v1(&task).expect("task baseline");

        assert_eq!(task_choice.choice.ordinal_path.len(), 2);
        let family_ordinal = task_choice.choice.ordinal_path[0]
            .checked_sub(combat.public_actions.atomic_actions.len())
            .expect("task selected symbolic Scry family");
        let projection = CombatActionProjectionV1 {
            public: combat.public_actions.clone(),
            private_resolution: combat.private_resolution.clone(),
        };
        resolve_public_combat_action_v1(
            &projection,
            &PublicCombatActionChoiceV1::Selection {
                family_ordinal,
                selected_domain_ordinals: task_choice.choice.ordinal_path[1..].to_vec(),
            },
        )
        .expect("conservative public Scry payload must resolve legally");
    }

    #[test]
    fn task_actor_exits_a_reward_screen_immediately_after_skipping_its_card_reward() {
        let source = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::CardReward,
            PlannerDecisionSite::CardReward,
            crate::ai::planner_core::PlannerDecisionContext::CardReward {
                reward_item_index: Some(0),
                pending_rewards: Vec::new(),
            },
            &[(CardId::Strike, 0)],
            vec![TaskStrategicActionV1::SkipCardReward {
                reward_item_index: 0,
            }],
            (70, 80),
            0,
        );
        let reward = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Reward,
            PlannerDecisionSite::Reward,
            crate::ai::planner_core::PlannerDecisionContext::Reward {
                pending_rewards: vec![
                    crate::ai::planner_core::PlannerRewardDescriptor::CardReward {
                        cards: vec![crate::ai::planner_core::PlannerOfferedCard {
                            card: CardId::Entrench,
                            upgrades: 0,
                        }],
                    },
                ],
            },
            &[(CardId::Strike, 0)],
            vec![
                TaskStrategicActionV1::OpenCardReward {
                    reward_item_index: 0,
                },
                TaskStrategicActionV1::Proceed {
                    site: PlannerDecisionSite::Reward,
                },
            ],
            (70, 80),
            0,
        );
        let stateless = public_deterministic_task_decision_v1(&reward)
            .expect("stateless reward decision still prefers opening its sole card reward");
        assert!(matches!(
            selected_task_strategic_action(&reward, &stateless.choice),
            TaskStrategicActionV1::OpenCardReward { .. }
        ));

        let mut actor = PublicDeterministicTaskActorV1::default();
        let view = crate::agent::task::TaskActorViewV1 {
            task: crate::agent::task::TaskSpecV1::ironclad_a0_act_three_fail_closed(),
            budget: TaskBudgetV1 {
                max_world_transitions: 10,
                max_reloads: 0,
            }
            .into(),
            committed_history: Default::default(),
            legal_knowledge: Default::default(),
            node_memory: Some(crate::agent::task::NodeMemoryV1 {
                scope: crate::agent::task::TaskScopeOrdinalV1(0),
                entry_site: crate::agent::task::TaskDecisionSiteV1::CardReward,
                attempts: vec![crate::agent::task::NodeAttemptV1 {
                    attempt: crate::agent::task::TaskAttemptOrdinalV1(0),
                    root: source.clone(),
                    steps: vec![crate::agent::task::TaskPublicStepV1 {
                        source,
                        choice: TaskPublicChoiceV1::one(0),
                        observed_boundary: TaskObservedBoundaryV1::Decision {
                            decision: reward.clone(),
                        },
                    }],
                    status: crate::agent::task::NodeAttemptStatusV1::InProgress,
                }],
            }),
            reload_permission: None,
            boundary: TaskActorBoundaryV1::Decision {
                decision: reward.clone(),
            },
        };

        let TaskActorIntentV1::Act { choice } = actor.next_intent(&view) else {
            panic!("history-aware actor should leave the reward screen");
        };
        assert!(matches!(
            selected_task_strategic_action(&reward, &choice),
            TaskStrategicActionV1::Proceed {
                site: PlannerDecisionSite::Reward
            }
        ));
        assert_eq!(
            actor
                .decisions()
                .last()
                .map(|decision| decision.rule_family),
            Some("strategic.exit_skipped_card_reward")
        );

        let mut reward_with_gold = reward.clone();
        let TaskPublicObservationV1::Strategic { observation } = &mut reward_with_gold.observation
        else {
            panic!("expected reward observation");
        };
        let crate::ai::planner_core::PlannerDecisionContext::Reward { pending_rewards } =
            &mut observation.context
        else {
            panic!("expected reward context");
        };
        pending_rewards.push(crate::ai::planner_core::PlannerRewardDescriptor::Gold { amount: 12 });
        reward_with_gold
            .candidates
            .push(TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::ClaimReward {
                    reward_item_index: 1,
                    reward: crate::ai::planner_core::PlannerRewardDescriptor::Gold { amount: 12 },
                },
            });
        let reward_with_gold = TaskPublicDecisionV1::new(
            reward_with_gold.site,
            reward_with_gold.observation,
            reward_with_gold.candidates,
        )
        .expect("reward with gold decision");
        let mut view_with_gold = view.clone();
        view_with_gold.node_memory.as_mut().unwrap().attempts[0].steps[0].observed_boundary =
            TaskObservedBoundaryV1::Decision {
                decision: reward_with_gold.clone(),
            };
        view_with_gold.boundary = TaskActorBoundaryV1::Decision {
            decision: reward_with_gold.clone(),
        };
        assert!(skipped_card_reward_exit_ordinal_v1(&view_with_gold, &reward_with_gold).is_none());
        let TaskActorIntentV1::Act { choice } = actor.next_intent(&view_with_gold) else {
            panic!("actor should handle an unclaimed gold reward");
        };
        assert!(matches!(
            selected_task_strategic_action(&reward_with_gold, &choice),
            TaskStrategicActionV1::ClaimReward {
                reward: crate::ai::planner_core::PlannerRewardDescriptor::Gold { amount: 12 },
                ..
            }
        ));
    }

    #[test]
    fn structured_coverage_actor_uses_the_legality_bootstrap_outside_its_narrow_plan() {
        let env = LearningEnvV3::new(RunControlConfig::default());
        let LearningBoundaryV1::Strategic { boundary } =
            env.observe().expect("fresh strategic boundary")
        else {
            panic!("expected fresh strategic boundary");
        };
        let decision =
            task_public_strategic_decision_v1(&boundary.observation, &boundary.legal_candidates)
                .expect("task decision");
        let expected = public_deterministic_task_decision_v1(&decision)
            .expect("legality bootstrap decision")
            .choice;
        let mut actor = public_structured_deterministic_task_actor_v1();
        let view = crate::agent::task::TaskActorViewV1 {
            task: crate::agent::task::TaskSpecV1::ironclad_a0_act_three_fail_closed(),
            budget: TaskBudgetV1 {
                max_world_transitions: 10,
                max_reloads: 0,
            }
            .into(),
            committed_history: Default::default(),
            legal_knowledge: Default::default(),
            node_memory: None,
            reload_permission: None,
            boundary: TaskActorBoundaryV1::Decision { decision },
        };

        assert_eq!(
            actor.identity().actor_name,
            PUBLIC_STRUCTURED_DETERMINISTIC_TASK_ACTOR_NAME_V1
        );
        assert_eq!(
            actor.next_intent(&view),
            TaskActorIntentV1::Act { choice: expected }
        );
    }

    #[test]
    fn visible_route_pressure_distinguishes_forced_elite_chain_from_recovery() {
        let node = |x, y, room: RoomType, edges: &[(i32, i32)]| PlannerPublicMapNode {
            x,
            y,
            room: room.into(),
            has_emerald_key: false,
            edges: edges
                .iter()
                .map(|(destination_x, destination_y)| PlannerPublicMapEdge {
                    destination_x: *destination_x,
                    destination_y: *destination_y,
                })
                .collect(),
        };
        let map = PlannerPublicMap {
            current_x: 0,
            current_y: 0,
            boss: None,
            nodes: vec![
                node(0, 1, RoomType::MonsterRoomElite, &[(0, 2)]),
                node(0, 2, RoomType::MonsterRoomElite, &[(0, 3)]),
                node(0, 3, RoomType::MonsterRoomBoss, &[]),
                node(1, 1, RoomType::MonsterRoom, &[(1, 2)]),
                node(1, 2, RoomType::RestRoom, &[(1, 3)]),
                node(1, 3, RoomType::MonsterRoomBoss, &[]),
            ],
        };

        let elite_chain = visible_route_pressure(&map, 0, 1, true);
        assert_eq!(elite_chain.min_elites, 2);
        assert_eq!(elite_chain.min_damage_rooms_before_recovery, 3);
        assert!(!elite_chain.recovery_before_damage_reachable);

        let recovery = visible_route_pressure(&map, 1, 1, true);
        assert_eq!(recovery.min_elites, 0);
        assert_eq!(recovery.min_damage_rooms_before_recovery, 1);
        assert!(!recovery.recovery_before_damage_reachable);

        let rest = visible_route_pressure(&map, 1, 2, true);
        assert_eq!(rest.min_damage_rooms_before_recovery, 0);
        assert!(rest.recovery_before_damage_reachable);
        let unusable_rest = visible_route_pressure(&map, 1, 2, false);
        assert_eq!(unusable_rest.min_damage_rooms_before_recovery, 1);
        assert!(!unusable_rest.recovery_before_damage_reachable);
    }

    #[test]
    fn learning_reward_exit_memory_uses_task_route_ranking_without_cancelling() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::MapOverlay {
            return_state: Box::new(EngineState::RewardScreen(RewardState::new())),
        };
        let LearningBoundaryV1::Strategic { mut boundary } =
            LearningEnvV3::from_session(session).observe().unwrap()
        else {
            panic!("expected map decision");
        };
        let route = boundary
            .legal_candidates
            .candidates
            .iter()
            .find_map(|candidate| match candidate.action {
                crate::ai::planner_core::PlannerAction::ChooseRouteNode { x, y, .. } => {
                    Some((x, y))
                }
                _ => None,
            })
            .unwrap();
        boundary.legal_candidates.candidates.retain(|candidate| {
            matches!(
                candidate.action,
                crate::ai::planner_core::PlannerAction::ChooseRouteNode { x, y, .. }
                    if (x, y) == route
            ) || matches!(
                candidate.action,
                crate::ai::planner_core::PlannerAction::Cancel { .. }
            )
        });
        boundary.observation.run.current_hp = 1;
        boundary.observation.public_map.nodes = vec![PlannerPublicMapNode {
            x: route.0,
            y: route.1,
            room: PlannerMapRoom::Elite,
            has_emerald_key: false,
            edges: Vec::new(),
        }];
        let model = LearningModelDecisionV1::from_strategic_boundary(&boundary).unwrap();
        let action = |ordinal: usize| {
            let LearningModelCandidateSemanticsV1::Strategic { action } =
                model.candidates[ordinal].semantics
            else {
                panic!("expected strategic action");
            };
            action.semantics().unwrap()
        };
        assert!(matches!(
            action(public_deterministic_baseline_decision_v1(&model).candidate_ordinal),
            TaskStrategicActionV1::ChooseRouteNode { .. }
        ));
        let continued = public_deterministic_baseline_after_reward_proceed_v1(&model);
        assert!(matches!(
            action(continued.candidate_ordinal),
            TaskStrategicActionV1::ChooseRouteNode { .. }
        ));
        assert_eq!(
            continued.rule_family,
            "strategic.honor_reward_proceed_on_map_overlay"
        );
    }

    #[test]
    fn map_overlay_takes_a_legal_route_even_when_its_score_is_below_cancel() {
        let root = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Map,
            PlannerDecisionSite::Map,
            crate::ai::planner_core::PlannerDecisionContext::Map { overlay: true },
            &[(CardId::Strike, 0), (CardId::Defend, 0)],
            vec![
                TaskStrategicActionV1::ChooseRouteNode {
                    x: 4,
                    y: 5,
                    flight: false,
                    question_mark_outcomes: Vec::new(),
                },
                TaskStrategicActionV1::Cancel {
                    site: PlannerDecisionSite::Map,
                },
            ],
            (34, 75),
            66,
        );
        let TaskPublicObservationV1::Strategic { mut observation } = root.observation else {
            panic!("expected strategic observation");
        };
        observation.run.floor = 5;
        observation.public_map.current_y = 4;
        observation.public_map.nodes = vec![PlannerPublicMapNode {
            x: 4,
            y: 5,
            room: PlannerMapRoom::Elite,
            has_emerald_key: false,
            edges: Vec::new(),
        }];
        assert!(
            visible_route_score(&StrategicBaselineViewV1::from_task(&observation), 4, 5) < -1_500
        );
        let decision = TaskPublicDecisionV1::new(
            root.site,
            TaskPublicObservationV1::Strategic { observation },
            root.candidates,
        )
        .expect("map overlay decision");
        let selected = public_deterministic_task_decision_v1(&decision).expect("stateless choice");
        assert!(matches!(
            selected_task_strategic_action(&decision, &selected.choice),
            TaskStrategicActionV1::Cancel {
                site: PlannerDecisionSite::Map
            }
        ));

        let reward = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Reward,
            PlannerDecisionSite::Reward,
            crate::ai::planner_core::PlannerDecisionContext::Reward {
                pending_rewards: Vec::new(),
            },
            &[(CardId::Strike, 0), (CardId::Defend, 0)],
            vec![TaskStrategicActionV1::Proceed {
                site: PlannerDecisionSite::Reward,
            }],
            (34, 75),
            66,
        );
        let view = TaskActorViewV1 {
            task: crate::agent::task::TaskSpecV1::ironclad_a0_act_three_fail_closed(),
            budget: TaskBudgetV1 {
                max_world_transitions: 10,
                max_reloads: 0,
            }
            .into(),
            committed_history: Default::default(),
            legal_knowledge: Default::default(),
            node_memory: Some(crate::agent::task::NodeMemoryV1 {
                scope: crate::agent::task::TaskScopeOrdinalV1(0),
                entry_site: crate::agent::task::TaskDecisionSiteV1::Reward,
                attempts: vec![crate::agent::task::NodeAttemptV1 {
                    attempt: crate::agent::task::TaskAttemptOrdinalV1(0),
                    root: reward.clone(),
                    steps: vec![crate::agent::task::TaskPublicStepV1 {
                        source: reward,
                        choice: TaskPublicChoiceV1::one(0),
                        observed_boundary: TaskObservedBoundaryV1::Decision {
                            decision: decision.clone(),
                        },
                    }],
                    status: crate::agent::task::NodeAttemptStatusV1::InProgress,
                }],
            }),
            reload_permission: None,
            boundary: TaskActorBoundaryV1::Decision {
                decision: decision.clone(),
            },
        };
        let mut actor = PublicDeterministicTaskActorV1::default();
        let TaskActorIntentV1::Act { choice } = actor.next_intent(&view) else {
            panic!("history-aware actor should leave completed rewards");
        };
        assert!(matches!(
            selected_task_strategic_action(&decision, &choice),
            TaskStrategicActionV1::ChooseRouteNode { x: 4, y: 5, .. }
        ));

        let pending_reward = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Reward,
            PlannerDecisionSite::Reward,
            crate::ai::planner_core::PlannerDecisionContext::Reward {
                pending_rewards: vec![crate::ai::planner_core::PlannerRewardDescriptor::Gold {
                    amount: 12,
                }],
            },
            &[(CardId::Strike, 0), (CardId::Defend, 0)],
            vec![TaskStrategicActionV1::ClaimReward {
                reward_item_index: 0,
                reward: crate::ai::planner_core::PlannerRewardDescriptor::Gold { amount: 12 },
            }],
            (34, 75),
            66,
        );
        let mut view = view;
        let attempt = &mut view.node_memory.as_mut().unwrap().attempts[0];
        attempt.root = pending_reward.clone();
        attempt.steps[0].source = pending_reward;
        let TaskActorIntentV1::Act { choice } = actor.next_intent(&view) else {
            panic!("actor should keep access to pending rewards");
        };
        assert!(matches!(
            selected_task_strategic_action(&decision, &choice),
            TaskStrategicActionV1::Cancel {
                site: PlannerDecisionSite::Map
            }
        ));
    }

    #[test]
    fn many_suffix_paths_cannot_hide_one_recovery_outlet() {
        let node = |x, y, room: RoomType, edges: &[(i32, i32)]| PlannerPublicMapNode {
            x,
            y,
            room: room.into(),
            has_emerald_key: false,
            edges: edges
                .iter()
                .map(|(destination_x, destination_y)| PlannerPublicMapEdge {
                    destination_x: *destination_x,
                    destination_y: *destination_y,
                })
                .collect(),
        };
        let mut nodes = vec![
            node(2, 0, RoomType::ShopRoom, &[(0, 1), (3, 1)]),
            node(0, 1, RoomType::MonsterRoom, &[(0, 2), (1, 2)]),
            node(3, 1, RoomType::RestRoom, &[(3, 14)]),
        ];
        for y in 2..=13 {
            let edges = if y == 13 {
                vec![(0, 14)]
            } else {
                vec![(0, y + 1), (1, y + 1)]
            };
            nodes.push(node(0, y, RoomType::MonsterRoom, &edges));
            nodes.push(node(1, y, RoomType::MonsterRoom, &edges));
        }
        nodes.push(node(0, 14, RoomType::MonsterRoomBoss, &[]));
        nodes.push(node(3, 14, RoomType::MonsterRoomBoss, &[]));
        let map = PlannerPublicMap {
            current_x: -1,
            current_y: -1,
            boss: None,
            nodes,
        };
        let pressure = visible_route_pressure(&map, 2, 0, true);
        assert_eq!(pressure.min_damage_rooms_before_recovery, 0);
        assert!(pressure.recovery_before_damage_reachable);
        assert_eq!(pressure.min_elites, 0);
        assert_eq!(pressure.max_elites, 0);
    }

    #[test]
    fn public_baseline_does_not_expect_rest_healing_from_blocking_relics() {
        for relic_id in [RelicId::CoffeeDripper, RelicId::MarkOfTheBloom] {
            let mut session = RunControlSession::new(RunControlConfig::default());
            session
                .run_state
                .relics
                .push(crate::content::relics::RelicState::new(relic_id));
            let boundary = LearningEnvV3::from_session(session)
                .observe()
                .expect("fresh strategic boundary");
            let decision = LearningModelDecisionV1::from_boundary(&boundary)
                .expect("public learning decision");
            let LearningModelObservationV1::Strategic(observation) = decision.observation else {
                panic!("expected strategic observation");
            };
            let view = StrategicBaselineViewV1::from_learning(observation);
            assert!(!view.can_recover_at_rest, "{relic_id:?}");
        }
    }

    #[test]
    fn public_event_costs_preserve_current_hp_before_small_max_hp_loss() {
        use crate::ai::planner_core::PlannerEventOptionPublicSemanticsV1;
        use crate::state::events::EventId;

        let lose_hp = PlannerEventOptionPublicSemanticsV1 {
            hp_delta: -20,
            terminal: true,
            ..Default::default()
        };
        let lose_max_hp = PlannerEventOptionPublicSemanticsV1 {
            max_hp_delta: -6,
            terminal: true,
            ..Default::default()
        };
        let decision = strategic_task_decision(
            crate::agent::task::TaskDecisionSiteV1::Event,
            PlannerDecisionSite::Event,
            crate::ai::planner_core::PlannerDecisionContext::Event {
                event: EventId::GoldenIdol,
                screen: 1,
                face_trader_result: None,
            },
            &[],
            vec![TaskStrategicActionV1::Proceed {
                site: PlannerDecisionSite::Event,
            }],
            (58, 80),
            0,
        );
        let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
            panic!("expected strategic observation");
        };
        let view = StrategicBaselineViewV1::from_task(observation);
        assert!(
            event_action_score(
                EventId::GoldenIdol,
                1,
                EventActionKind::Decline,
                &lose_max_hp,
                &view
            ) > event_action_score(
                EventId::GoldenIdol,
                1,
                EventActionKind::Fight,
                &lose_hp,
                &view
            )
        );
    }
}
