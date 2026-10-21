use serde::{Deserialize, Serialize};

use crate::agent::information::action::{
    PublicCombatActionSurfaceV1, PublicCombatAtomicActionV1, PublicCombatIndexedChoiceCandidateV1,
    PublicCombatIndexedChoiceReasonV1, PublicCombatSelectionFamilyV1,
};
use crate::agent::information::combat::InformationAccessV1;
use crate::agent::information::effect::{
    public_combat_atomic_action_effect_v1, public_combat_indexed_choice_effect_v1,
    public_combat_selection_effect_v1, PublicCombatActionEffectEnvelopeV1,
    PublicCombatEffectProjectionErrorV1,
};
use crate::agent::information::run::PublicCombatRunContextV1;
use crate::agent::information::state::PublicCombatStateV1;
use crate::ai::planner_core::{
    stable_planner_id, CandidateSetCompleteness, LegalCandidateSet, PlannerCardObservation,
    PlannerDecisionContext, PlannerDecisionSite, PlannerEventOptionPublicSemanticsV1,
    PlannerMechanicsManifest, PlannerObservation, PlannerPublicHistory, PlannerPublicMap,
    PlannerQuestionMarkOutcome, PlannerRelicObservation, PlannerRewardDescriptor, PlannerRunGoal,
    PlannerRunScalars, PublicCandidateSurfaceKindV1, PublicCandidateSurfaceReferenceV1,
    PublicDecisionDomainV1, PublicHistorySnapshotReferenceV1, PublicInformationSnapshotV1,
    PublicObservationReferenceV1, PublicObservationScopeV1,
};
use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::sim::combat_action_surface::CombatIndexedChoiceInputEncodingV2;
use crate::state::events::{EventActionKind, EventId};
use crate::state::selection::{SelectionReason, SelectionScope};

use super::TaskDecisionSiteV1;

const STRATEGIC_OBSERVATION_SCHEMA: &str = "TaskStrategicPublicObservationV3";
const COMBAT_OBSERVATION_SCHEMA: &str = "TaskCombatPublicObservationV1";
const HISTORY_SCHEMA: &str = "TaskPublicHistorySnapshotV1";
const CANDIDATE_SURFACE_SCHEMA: &str = "TaskPublicCandidateSurfaceV1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum TaskPublicObservationV1 {
    Strategic {
        observation: TaskStrategicObservationV1,
    },
    Combat {
        run_context: PublicCombatRunContextV1,
        observation: PublicCombatStateV1,
    },
}

impl TaskPublicObservationV1 {
    fn domain(&self) -> PublicDecisionDomainV1 {
        match self {
            Self::Strategic { .. } => PublicDecisionDomainV1::Strategic,
            Self::Combat { .. } => PublicDecisionDomainV1::Combat,
        }
    }

    fn scope(&self) -> PublicObservationScopeV1 {
        match self {
            Self::Strategic { .. } => PublicObservationScopeV1::StrategicRunDecision,
            Self::Combat { run_context, .. } if run_context.is_available() => {
                PublicObservationScopeV1::CombatDecisionWithRunContinuation
            }
            Self::Combat { .. } => PublicObservationScopeV1::CombatDecisionOnly,
        }
    }

    fn schema_name(&self) -> &'static str {
        match self {
            Self::Strategic { .. } => STRATEGIC_OBSERVATION_SCHEMA,
            Self::Combat { .. } => COMBAT_OBSERVATION_SCHEMA,
        }
    }

    fn schema_version(&self) -> u32 {
        match self {
            Self::Strategic { .. } => 3,
            Self::Combat { .. } => 1,
        }
    }

    fn validate(&self, site: TaskDecisionSiteV1) -> Result<(), String> {
        if self.domain() != site.public_domain() {
            return Err("task public observation domain disagrees with decision site".to_string());
        }
        match self {
            Self::Strategic { observation }
                if TaskDecisionSiteV1::from(observation.decision_site) != site =>
            {
                Err("strategic public observation site disagrees with task site".to_string())
            }
            Self::Combat { observation, .. }
                if observation.information_access != InformationAccessV1::Public =>
            {
                Err("task combat observation must use public information access".to_string())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStrategicObservationV1 {
    pub mechanics: PlannerMechanicsManifest,
    pub run_goal: PlannerRunGoal,
    pub decision_site: PlannerDecisionSite,
    pub run: PlannerRunScalars,
    pub cards: Vec<TaskStrategicCardV1>,
    pub relics: Vec<PlannerRelicObservation>,
    pub potions: Vec<TaskStrategicPotionSlotV1>,
    pub public_map: PlannerPublicMap,
    pub context: PlannerDecisionContext,
    pub public_history: PlannerPublicHistory,
}

impl TaskStrategicObservationV1 {
    fn from_planner(observation: &PlannerObservation) -> Result<Self, String> {
        let bottle_bindings = observation
            .relics
            .iter()
            .filter(|relic| {
                matches!(
                    relic.relic,
                    RelicId::BottledFlame | RelicId::BottledLightning | RelicId::BottledTornado
                )
            })
            .map(|relic| {
                let mut state = crate::content::relics::RelicState::new(relic.relic);
                state.amount = relic.amount;
                crate::content::relics::persistent_state::persistent_relic_bound_card_lineage_v1(
                    &state,
                )
                .map(|lineage| (relic.relic, lineage))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            mechanics: observation.mechanics.clone(),
            run_goal: observation.run_goal,
            decision_site: observation.decision_site,
            run: observation.run.clone(),
            cards: observation
                .cards
                .iter()
                .enumerate()
                .map(|(ordinal, card)| {
                    let mut public = TaskStrategicCardV1::from_planner(ordinal, card);
                    public.bottled_by = bottle_bindings.iter().find_map(|(relic, lineage)| {
                        (*lineage == Some(card.card_lineage)).then_some(*relic)
                    });
                    public
                })
                .collect(),
            relics: observation.relics.clone(),
            potions: observation
                .potions
                .iter()
                .map(|slot| TaskStrategicPotionSlotV1 {
                    slot: slot.slot,
                    potion: slot.potion.as_ref().map(|potion| TaskStrategicPotionV1 {
                        potion: potion.potion,
                        can_use: potion.can_use,
                        can_discard: potion.can_discard,
                        requires_target: potion.requires_target,
                    }),
                })
                .collect(),
            public_map: observation.public_map.clone(),
            context: observation.context.clone(),
            public_history: observation.public_history.clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStrategicCardV1 {
    pub ordinal: usize,
    pub card: CardId,
    pub upgrades: u8,
    pub misc_value: i32,
    pub base_damage_override: Option<i32>,
    pub base_block_override: Option<i32>,
    pub cost_modifier: i8,
    /// Public bottle marker, projected from the owning relic's typed lineage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottled_by: Option<RelicId>,
}

impl TaskStrategicCardV1 {
    fn from_planner(ordinal: usize, card: &PlannerCardObservation) -> Self {
        Self {
            ordinal,
            card: card.card,
            upgrades: card.upgrades,
            misc_value: card.misc_value,
            base_damage_override: card.base_damage_override,
            base_block_override: card.base_block_override,
            cost_modifier: card.cost_modifier,
            bottled_by: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStrategicPotionSlotV1 {
    pub slot: usize,
    pub potion: Option<TaskStrategicPotionV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStrategicPotionV1 {
    pub potion: PotionId,
    pub can_use: bool,
    pub can_discard: bool,
    pub requires_target: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum TaskPublicCandidateV1 {
    Strategic {
        action: TaskStrategicActionV1,
    },
    CombatAtomic {
        action: PublicCombatAtomicActionV1,
    },
    CombatIndexedChoice {
        action: PublicCombatAtomicActionV1,
        input_encoding: CombatIndexedChoiceInputEncodingV2,
        reason: PublicCombatIndexedChoiceReasonV1,
        candidate: PublicCombatIndexedChoiceCandidateV1,
    },
    CombatSelectionFamily {
        family: PublicCombatSelectionFamilyV1,
    },
}

impl TaskPublicCandidateV1 {
    fn domain(&self) -> PublicDecisionDomainV1 {
        match self {
            Self::Strategic { .. } => PublicDecisionDomainV1::Strategic,
            Self::CombatAtomic { .. }
            | Self::CombatIndexedChoice { .. }
            | Self::CombatSelectionFamily { .. } => PublicDecisionDomainV1::Combat,
        }
    }

    pub(super) fn combat_effect_v1(
        &self,
        observation: &PublicCombatStateV1,
    ) -> Result<PublicCombatActionEffectEnvelopeV1, PublicCombatEffectProjectionErrorV1> {
        match self {
            Self::CombatAtomic { action } => {
                public_combat_atomic_action_effect_v1(observation, action)
            }
            Self::CombatIndexedChoice {
                input_encoding,
                reason,
                candidate,
                ..
            } => public_combat_indexed_choice_effect_v1(
                observation,
                *input_encoding,
                reason,
                *candidate,
            ),
            Self::CombatSelectionFamily { family } => {
                public_combat_selection_effect_v1(observation, family)
            }
            Self::Strategic { .. } => Err(PublicCombatEffectProjectionErrorV1::NotCombatCandidate),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskStrategicActionV1 {
    ChooseRouteNode {
        x: i32,
        y: i32,
        flight: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        question_mark_outcomes: Vec<PlannerQuestionMarkOutcome>,
    },
    ChooseEventOption {
        event: EventId,
        screen: usize,
        option_index: usize,
        action: EventActionKind,
        public_semantics: PlannerEventOptionPublicSemanticsV1,
    },
    TakeCard {
        reward_item_index: Option<usize>,
        option_index: usize,
        card: CardId,
        upgrades: u8,
    },
    OpenCardReward {
        reward_item_index: usize,
    },
    SingingBowl {
        reward_item_index: Option<usize>,
    },
    SkipCardReward {
        reward_item_index: usize,
    },
    ClaimReward {
        reward_item_index: usize,
        reward: PlannerRewardDescriptor,
    },
    Rest,
    Smith {
        card_ordinal: usize,
        card: CardId,
        upgrades: u8,
    },
    Dig,
    Lift,
    Toke {
        card_ordinal: usize,
        card: CardId,
        upgrades: u8,
    },
    Recall,
    BuyCard {
        shop_slot: usize,
        card: CardId,
        upgrades: u8,
        price: i32,
    },
    BuyRelic {
        shop_slot: usize,
        relic: RelicId,
        price: i32,
    },
    BuyPotion {
        shop_slot: usize,
        potion: PotionId,
        price: i32,
    },
    UseRunPotion {
        slot: usize,
        potion: PotionId,
    },
    DiscardRunPotion {
        slot: usize,
        potion: PotionId,
    },
    RemoveCard {
        card_ordinal: usize,
        card: CardId,
        upgrades: u8,
        price: i32,
    },
    OpenPendingRewards,
    LeaveShop,
    TakeBossRelic {
        option_index: usize,
        relic: RelicId,
    },
    SkipBossRelic,
    BeginRunCardSelection {
        scope: SelectionScope,
        reason: SelectionReason,
        min_choices: usize,
        max_choices: usize,
        selectable_card_ordinals: Vec<usize>,
    },
    SubmitRunSelection {
        scope: SelectionScope,
        selected_card_ordinals: Vec<usize>,
    },
    OpenChest,
    Proceed {
        site: PlannerDecisionSite,
    },
    Cancel {
        site: PlannerDecisionSite,
    },
}

pub fn task_public_strategic_decision_v1(
    observation: &PlannerObservation,
    candidates: &LegalCandidateSet,
) -> Result<super::TaskPublicDecisionV1, String> {
    if candidates.observation_id != observation.observation_id
        || candidates.site != observation.decision_site
    {
        return Err("strategic candidate surface belongs to another observation".to_string());
    }
    if !matches!(
        candidates.completeness,
        CandidateSetCompleteness::Complete { .. }
    ) {
        return Err("task strategic decision requires a complete candidate surface".to_string());
    }
    let public_candidates = candidates
        .candidates
        .iter()
        .map(|candidate| {
            if candidate.mechanics != observation.mechanics {
                return Err("strategic candidate mechanics disagree with observation".to_string());
            }
            Ok(TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::from_planner_action_v1(
                    &observation.cards,
                    &candidate.action,
                )?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    super::TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::from(observation.decision_site),
        TaskPublicObservationV1::Strategic {
            observation: TaskStrategicObservationV1::from_planner(observation)?,
        },
        public_candidates,
    )
}

pub fn task_public_combat_decision_v1(
    run_context: PublicCombatRunContextV1,
    observation: PublicCombatStateV1,
    actions: &PublicCombatActionSurfaceV1,
) -> Result<super::TaskPublicDecisionV1, String> {
    let mut candidates = actions
        .atomic_actions
        .iter()
        .cloned()
        .map(|action| match action {
            PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index } => {
                let indexed = actions.indexed_choice.as_ref().ok_or_else(|| {
                    "indexed combat action lacks its public choice surface".to_string()
                })?;
                let candidate = indexed
                    .candidates
                    .get(choice_index)
                    .copied()
                    .ok_or_else(|| {
                        "indexed combat action points outside its public choice surface".to_string()
                    })?;
                Ok(TaskPublicCandidateV1::CombatIndexedChoice {
                    action: PublicCombatAtomicActionV1::SubmitIndexedChoice { choice_index },
                    input_encoding: indexed.input_encoding,
                    reason: indexed.reason.clone(),
                    candidate,
                })
            }
            action => Ok(TaskPublicCandidateV1::CombatAtomic { action }),
        })
        .collect::<Result<Vec<_>, String>>()?;
    candidates.extend(
        actions
            .selection_families
            .iter()
            .cloned()
            .map(|family| TaskPublicCandidateV1::CombatSelectionFamily { family }),
    );
    super::TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::Combat,
        TaskPublicObservationV1::Combat {
            run_context,
            observation,
        },
        candidates,
    )
}

pub(super) fn build_public_information_snapshot_v1(
    site: TaskDecisionSiteV1,
    observation: &TaskPublicObservationV1,
    candidates: &[TaskPublicCandidateV1],
) -> Result<PublicInformationSnapshotV1, String> {
    observation.validate(site)?;
    if candidates.is_empty() {
        return Err("task public decision has no candidates".to_string());
    }
    if candidates
        .iter()
        .any(|candidate| candidate.domain() != observation.domain())
    {
        return Err("task public candidate domain disagrees with observation".to_string());
    }
    let observation_reference = PublicObservationReferenceV1::from_sanitized_payload(
        observation.schema_name(),
        observation.schema_version(),
        observation.scope(),
        observation,
    )?;
    let candidate_ids = candidates
        .iter()
        .enumerate()
        .map(|(ordinal, candidate)| {
            stable_planner_id(
                "task_public_candidate_v1",
                &(
                    observation_reference.observation_id.as_str(),
                    ordinal,
                    candidate,
                ),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let candidate_surface = PublicCandidateSurfaceReferenceV1::from_candidate_ids(
        CANDIDATE_SURFACE_SCHEMA,
        1,
        PublicCandidateSurfaceKindV1::DeployablePolicy,
        candidate_ids,
    )?;
    let history_snapshot = match observation {
        TaskPublicObservationV1::Strategic { observation } => {
            PublicHistorySnapshotReferenceV1::from_sanitized_payload(
                HISTORY_SCHEMA,
                1,
                &observation.public_history,
            )?
        }
        TaskPublicObservationV1::Combat { observation, .. } => {
            let histories = observation
                .monsters
                .iter()
                .map(|monster| {
                    (
                        monster.slot,
                        &monster.executed_moves,
                        &monster.public_counters,
                    )
                })
                .collect::<Vec<_>>();
            PublicHistorySnapshotReferenceV1::from_sanitized_payload(
                HISTORY_SCHEMA,
                1,
                &(
                    observation.turn.turn_count,
                    &observation.turn.counters,
                    histories,
                ),
            )?
        }
    };
    PublicInformationSnapshotV1::new(
        observation.domain(),
        observation_reference,
        history_snapshot,
        candidate_surface,
    )
}
