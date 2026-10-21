use serde::{Deserialize, Serialize};

use super::{TaskPrivilegedSupportProvenanceV1, TaskPublicChoiceV1, TaskPublicDecisionV1};
use crate::ai::planner_core::PlannerRewardDescriptor;
use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::persistent_state::{
    persistent_relic_states_equivalent_for_resource_dominance_v1,
    persistent_relic_states_strictly_dominate_within_resource_equivalence_v1,
    post_combat_persistent_relic_states_equal_v1,
};
use crate::content::relics::{RelicId, RelicState};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskResearchPlanAttemptExitV1 {
    Witness,
    /// A complete witness was retained while improvement search hit its budget.
    WitnessAtWorkLimit,
    WitnessAtWallLimit,
    GenerationWorkExhausted,
    ExpandedNodeBudgetExhausted,
    WallTimeExhausted,
    AllowanceExhausted,
    DeadlineReached,
    PortfolioCompleteWithoutWitness,
    SearchFrontierExhaustedWithoutWitness,
    ActionPrefixBudgetExhausted,
    ActionSurfaceIncomplete,
    FrontierOpenWithoutWitness,
    MilestoneCandidates,
}

impl TaskResearchPlanAttemptExitV1 {
    pub fn has_witness(self) -> bool {
        matches!(
            self,
            Self::Witness | Self::WitnessAtWorkLimit | Self::WitnessAtWallLimit
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskResearchPlanWorkUnitV1 {
    TurnGraphGenerationWork,
    CompleteTurnProposalExpandedBoundaries,
    AtomicExpandedNodes,
    MilestoneClosureGenerationWork,
    /// Settled search transitions plus the solver's independent witness replay.
    WholeCombatTransitions,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchPlanLaneAttemptV1 {
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub work_unit: TaskResearchPlanWorkUnitV1,
    pub work_consumed: u64,
    pub exit: TaskResearchPlanAttemptExitV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchPlanAttemptV1 {
    /// Unique replay-verified executions. Search-lane discovery order is kept
    /// separately in `lanes`; one execution may be supported by several lanes.
    pub candidates: Vec<TaskResearchPlanCandidateV1>,
    /// Search lanes in execution order.
    pub lanes: Vec<TaskResearchPlanLaneAttemptV1>,
}

impl TaskResearchPlanAttemptV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.lanes.is_empty() {
            return Err("research plan attempt contains no search lane".to_string());
        }
        let mut lane_provenance = std::collections::BTreeSet::new();
        for lane in &self.lanes {
            lane.provenance.validate()?;
            if !lane_provenance.insert(lane.provenance.clone()) {
                return Err("research plan attempt contains duplicate producer lanes".to_string());
            }
        }
        for (candidate_ordinal, candidate) in self.candidates.iter().enumerate() {
            candidate.validate()?;
            if self.candidates[..candidate_ordinal]
                .iter()
                .any(|prior| prior.plan == candidate.plan)
            {
                return Err("research plan attempt contains duplicate execution plans".to_string());
            }
            for provenance in &candidate.supporting_provenances {
                let matching_lanes = self
                    .lanes
                    .iter()
                    .filter(|lane| lane.exit.has_witness() && lane.provenance == *provenance)
                    .count();
                if matching_lanes != 1 {
                    return Err(
                        "research plan support does not have exactly one matching witness lane"
                            .to_string(),
                    );
                }
            }
        }
        if self.lanes.iter().any(|lane| {
            let matching_candidates = self
                .candidates
                .iter()
                .filter(|candidate| candidate.supporting_provenances.contains(&lane.provenance))
                .count();
            lane.exit.has_witness() != (matching_candidates != 0)
        }) {
            return Err("witness lane did not deliver a research plan".to_string());
        }
        Ok(())
    }

    /// The scheduler exit reported by the last serviced producer lane.
    ///
    /// This is lane-order evidence, not the portfolio's aggregate result: a
    /// later challenger may miss after an earlier producer already delivered
    /// a complete witness.
    pub fn last_lane_exit(&self) -> TaskResearchPlanAttemptExitV1 {
        self.lanes
            .last()
            .expect("validated research plan attempt has a lane")
            .exit
    }

    /// Aggregate complete-plan result for a multi-producer attempt.
    ///
    /// Validation binds every retained candidate to a `Witness` lane. Once at
    /// least one such candidate exists, no later lane-local miss can turn the
    /// portfolio back into a no-witness result. Exact producer exits remain
    /// available through `lanes` and `last_lane_exit`.
    pub fn summary_exit(&self) -> TaskResearchPlanAttemptExitV1 {
        if self.candidates.is_empty() {
            self.last_lane_exit()
        } else {
            TaskResearchPlanAttemptExitV1::Witness
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchPlanPersistentOutcomeV1 {
    pub max_hp: i32,
    pub recoverable_gold_delta: i32,
    pub unrecovered_stolen_gold: i32,
    pub ritual_dagger_value: i32,
    pub genetic_algorithm_value: i32,
    pub external_burden_count: i32,
}

impl TaskResearchPlanPersistentOutcomeV1 {
    fn dominates_or_equals(self, other: Self) -> bool {
        self.max_hp >= other.max_hp
            && self.recoverable_gold_delta >= other.recoverable_gold_delta
            && self.unrecovered_stolen_gold <= other.unrecovered_stolen_gold
            && self.ritual_dagger_value >= other.ritual_dagger_value
            && self.genetic_algorithm_value >= other.genetic_algorithm_value
            && self.external_burden_count <= other.external_burden_count
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchPlanGeneratedPotionV1 {
    pub slot: usize,
    pub potion_id: PotionId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskResearchPlanMetaChangeV1 {
    AddCardToMasterDeck {
        card_id: CardId,
    },
    ModifyCardMisc {
        master_deck_ordinal: usize,
        amount: i32,
    },
    UpgradeMasterDeckCard {
        master_deck_ordinal: usize,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskResearchPlanPendingRewardV1 {
    Gold {
        amount: i32,
    },
    StolenGold {
        amount: i32,
    },
    Card {
        cards: Vec<TaskResearchPlanRewardCardV1>,
    },
    Relic {
        relic_id: RelicId,
    },
    Potion {
        potion_id: PotionId,
    },
    EmeraldKey,
    SapphireKey,
}

impl TaskResearchPlanPendingRewardV1 {
    pub fn from_planner_descriptor_v1(reward: &PlannerRewardDescriptor) -> Self {
        match reward {
            PlannerRewardDescriptor::Gold { amount } => Self::Gold { amount: *amount },
            PlannerRewardDescriptor::StolenGold { amount } => Self::StolenGold { amount: *amount },
            PlannerRewardDescriptor::CardReward { cards } => Self::Card {
                cards: cards
                    .iter()
                    .map(|card| TaskResearchPlanRewardCardV1 {
                        card_id: card.card,
                        upgrades: card.upgrades,
                    })
                    .collect(),
            },
            PlannerRewardDescriptor::Relic { relic } => Self::Relic { relic_id: *relic },
            PlannerRewardDescriptor::Potion { potion } => Self::Potion { potion_id: *potion },
            PlannerRewardDescriptor::EmeraldKey => Self::EmeraldKey,
            PlannerRewardDescriptor::SapphireKey => Self::SapphireKey,
        }
    }

    pub fn matches_planner_descriptor_v1(&self, reward: &PlannerRewardDescriptor) -> bool {
        self == &Self::from_planner_descriptor_v1(reward)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchPlanRewardCardV1 {
    pub card_id: CardId,
    pub upgrades: u8,
}

/// Public, already-materialized facts at the replay proof's exact successor
/// boundary. Mandatory handoffs are included: for example, defeating the
/// first A20 Act 3 boss may enter the second boss and trigger Pantograph before
/// `final_hp` is observed. The underlying combat-terminal resources remain a
/// separate tactical fact. This type deliberately carries no checkpoint, RNG
/// state, UUID, or private action.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchPlanOutcomeV1 {
    pub final_hp: i32,
    pub final_gold: i32,
    pub persistent: TaskResearchPlanPersistentOutcomeV1,
    pub consumed_entry_potion_slots: Vec<usize>,
    pub total_potions_consumed: u32,
    pub generated_potions: Vec<TaskResearchPlanGeneratedPotionV1>,
    pub final_relics: Vec<RelicState>,
    pub meta_changes: Vec<TaskResearchPlanMetaChangeV1>,
    pub pending_rewards: Vec<TaskResearchPlanPendingRewardV1>,
    pub combat_mugged: bool,
}

impl TaskResearchPlanOutcomeV1 {
    /// Policy comparison across a rewarded victory and a mugged exit. All
    /// materialized resources must still satisfy the strict checks, with at
    /// least one improvement. The original exit facts and strict frontier are
    /// unchanged; an exit label alone supplies no resource preference.
    pub fn rewarded_exit_strictly_dominates_v1(&self, mugged: &Self) -> bool {
        if self.combat_mugged || !mugged.combat_mugged {
            return false;
        }
        let mut comparable_mugged = mugged.clone();
        comparable_mugged.combat_mugged = false;
        self.strictly_dominates(&comparable_mugged)
    }

    pub fn strictly_dominates(&self, other: &Self) -> bool {
        self.strictly_dominates_with_relic_equivalence_v1(
            other,
            post_combat_persistent_relic_states_equal_v1(&self.final_relics, &other.final_relics),
            false,
        )
    }

    pub fn strictly_dominates_with_bounded_relic_progress_equivalence_v1(
        &self,
        other: &Self,
    ) -> bool {
        self.strictly_dominates_with_relic_equivalence_v1(
            other,
            persistent_relic_states_equivalent_for_resource_dominance_v1(
                &self.final_relics,
                &other.final_relics,
            ),
            persistent_relic_states_strictly_dominate_within_resource_equivalence_v1(
                &self.final_relics,
                &other.final_relics,
            ),
        )
    }

    fn strictly_dominates_with_relic_equivalence_v1(
        &self,
        other: &Self,
        final_relics_equivalent: bool,
        relic_progress_strictly_better: bool,
    ) -> bool {
        let entry_potion_expenditure_is_subset = self
            .consumed_entry_potion_slots
            .iter()
            .all(|slot| other.consumed_entry_potion_slots.contains(slot));
        let claimable_gold = self.claimable_gold_v1();
        let other_claimable_gold = other.claimable_gold_v1();
        let no_unmodeled_persistent_difference = self.generated_potions == other.generated_potions
            && final_relics_equivalent
            && self.meta_changes == other.meta_changes
            && same_non_gold_pending_rewards_v1(&self.pending_rewards, &other.pending_rewards)
            && self.combat_mugged == other.combat_mugged;
        self.final_hp >= other.final_hp
            && claimable_gold >= other_claimable_gold
            && self.persistent.dominates_or_equals(other.persistent)
            && entry_potion_expenditure_is_subset
            && self.total_potions_consumed <= other.total_potions_consumed
            && no_unmodeled_persistent_difference
            && (self.final_hp > other.final_hp
                || claimable_gold > other_claimable_gold
                || self.persistent != other.persistent
                || self.consumed_entry_potion_slots != other.consumed_entry_potion_slots
                || self.total_potions_consumed < other.total_potions_consumed
                || relic_progress_strictly_better)
    }

    /// Compares deterministic post-combat wealth without treating gold that is
    /// already in the run state differently from gold waiting in the reward
    /// surface. Non-gold rewards remain exact typed facts.
    pub fn claimable_gold_and_pending_rewards_equivalent_v1(&self, other: &Self) -> bool {
        self.claimable_gold_v1() == other.claimable_gold_v1()
            && same_non_gold_pending_rewards_v1(&self.pending_rewards, &other.pending_rewards)
    }

    pub fn claimable_gold_v1(&self) -> i64 {
        let prevents_gain = self
            .final_relics
            .iter()
            .any(|relic| crate::content::relics::mechanics::relic_prevents_gold_gain(relic.id));
        let golden_idol = self
            .final_relics
            .iter()
            .any(|relic| relic.id == RelicId::GoldenIdol);
        i64::from(self.final_gold)
            + self
                .pending_rewards
                .iter()
                .map(|reward| {
                    // Combat rewards store base Gold; the bonus is paid on claim.
                    // Recovered theft is a separate reward with no Idol bonus.
                    let amount = match reward {
                        TaskResearchPlanPendingRewardV1::Gold { amount } if golden_idol => {
                            crate::content::relics::golden_idol::apply_reward_gold_bonus(*amount)
                        }
                        TaskResearchPlanPendingRewardV1::Gold { amount }
                        | TaskResearchPlanPendingRewardV1::StolenGold { amount } => *amount,
                        _ => 0,
                    };
                    if prevents_gain && amount > 0 {
                        0
                    } else {
                        i64::from(amount)
                    }
                })
                .sum::<i64>()
    }

    fn validate(&self) -> Result<(), String> {
        if self
            .consumed_entry_potion_slots
            .windows(2)
            .any(|slots| slots[0] >= slots[1])
        {
            return Err("research plan outcome potion slots must be sorted and unique".to_string());
        }
        if self
            .generated_potions
            .windows(2)
            .any(|potions| potions[0].slot >= potions[1].slot)
        {
            return Err(
                "research plan outcome generated potion slots must be sorted and unique"
                    .to_string(),
            );
        }
        Ok(())
    }
}

fn same_non_gold_pending_rewards_v1(
    left: &[TaskResearchPlanPendingRewardV1],
    right: &[TaskResearchPlanPendingRewardV1],
) -> bool {
    left.iter()
        .filter(|reward| {
            !matches!(
                reward,
                TaskResearchPlanPendingRewardV1::Gold { .. }
                    | TaskResearchPlanPendingRewardV1::StolenGold { .. }
            )
        })
        .eq(right.iter().filter(|reward| {
            !matches!(
                reward,
                TaskResearchPlanPendingRewardV1::Gold { .. }
                    | TaskResearchPlanPendingRewardV1::StolenGold { .. }
            )
        }))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchPlanCandidateV1 {
    pub plan: TaskResearchPlanV1,
    pub outcome: TaskResearchPlanOutcomeV1,
    /// Search-request satisfaction is evidence about this line, never an
    /// execution instruction or a run-level resource exchange rate.
    pub satisfies_requested_quality: bool,
    /// Search producers that independently delivered this exact execution.
    /// This evidence is deliberately outside `plan`: producer discovery does
    /// not participate in executable identity.
    pub supporting_provenances: Vec<TaskPrivilegedSupportProvenanceV1>,
}

/// Returns the candidate ordinals that survive strict outcome dominance.
///
/// This is a fact projection only: incomparable resource outcomes remain in
/// the frontier, and producer order is never interpreted as a preference.
pub fn task_research_plan_non_dominated_candidate_ordinals_v1(
    candidates: &[TaskResearchPlanCandidateV1],
) -> Vec<usize> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(candidate_ordinal, candidate)| {
            (!candidates.iter().any(|other| {
                !std::ptr::eq(candidate, other)
                    && other.outcome.strictly_dominates(&candidate.outcome)
            }))
            .then_some(candidate_ordinal)
        })
        .collect()
}

/// Returns the unique shortest public plan only when every candidate in the
/// supplied fact frontier has the exact same materialized run outcome.
///
/// This is an operational canonicalization fact, not a run-resource score.
/// Equal-length plans remain ambiguous instead of inheriting producer order.
pub fn task_research_plan_unique_shortest_exact_outcome_candidate_v1(
    candidates: &[TaskResearchPlanCandidateV1],
    non_dominated_candidate_ordinals: &[usize],
) -> Option<usize> {
    if non_dominated_candidate_ordinals.len() < 2 {
        return None;
    }
    let first = candidates.get(*non_dominated_candidate_ordinals.first()?)?;
    if non_dominated_candidate_ordinals.iter().any(|ordinal| {
        candidates
            .get(*ordinal)
            .is_none_or(|candidate| candidate.outcome != first.outcome)
    }) {
        return None;
    }
    let shortest_steps = non_dominated_candidate_ordinals
        .iter()
        .filter_map(|ordinal| candidates.get(*ordinal))
        .map(|candidate| candidate.plan.steps.len())
        .min()?;
    let mut shortest = non_dominated_candidate_ordinals
        .iter()
        .copied()
        .filter(|ordinal| {
            candidates
                .get(*ordinal)
                .is_some_and(|candidate| candidate.plan.steps.len() == shortest_steps)
        });
    let selected = shortest.next()?;
    shortest.next().is_none().then_some(selected)
}

impl TaskResearchPlanCandidateV1 {
    /// Strict resource dominance at a fully settled run boundary, respecting
    /// the caller's requested quality and protected persistent relic states.
    pub fn resource_dominates_v1(&self, other: &Self) -> bool {
        (!other.satisfies_requested_quality || self.satisfies_requested_quality)
            && self
                .outcome
                .strictly_dominates_with_bounded_relic_progress_equivalence_v1(&other.outcome)
    }

    fn validate(&self) -> Result<(), String> {
        self.outcome.validate()?;
        if self.supporting_provenances.is_empty() {
            return Err("research plan candidate contains no supporting provenance".to_string());
        }
        for provenance in &self.supporting_provenances {
            provenance.validate()?;
        }
        if self
            .supporting_provenances
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err(
                "research plan candidate supporting provenances must be sorted and unique"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchPlanStepV1 {
    pub source_public_snapshot_id: String,
    pub choice: TaskPublicChoiceV1,
}

/// A privileged planner's exact witness translated into the public ordinal
/// language. Exact inputs, checkpoints, RNG state, and private identities are
/// deliberately absent. Every step remains bound to the public snapshot on
/// which its ordinal path is interpreted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchPlanV1 {
    pub root_public_snapshot_id: String,
    pub steps: Vec<TaskResearchPlanStepV1>,
}

impl TaskResearchPlanV1 {
    pub fn validate_for_root(&self, root: &TaskPublicDecisionV1) -> Result<(), String> {
        if self.root_public_snapshot_id != root.information.snapshot_id {
            return Err("research plan belongs to another public root".to_string());
        }
        if self.steps.is_empty() {
            return Err("research plan contains no public steps".to_string());
        }
        if self.steps[0].source_public_snapshot_id != self.root_public_snapshot_id {
            return Err("research plan first step does not begin at its public root".to_string());
        }
        root.validate_choice(&self.steps[0].choice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::TaskPrivilegedSupportAccessV1;

    fn provenance(name: &str) -> TaskPrivilegedSupportProvenanceV1 {
        TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: name.to_string(),
            producer_revision: 1,
        }
    }

    fn lane(name: &str, exit: TaskResearchPlanAttemptExitV1) -> TaskResearchPlanLaneAttemptV1 {
        TaskResearchPlanLaneAttemptV1 {
            provenance: provenance(name),
            work_unit: TaskResearchPlanWorkUnitV1::AtomicExpandedNodes,
            work_consumed: 1,
            exit,
        }
    }

    fn plan(name: &str) -> TaskResearchPlanV1 {
        TaskResearchPlanV1 {
            root_public_snapshot_id: "root".to_string(),
            steps: vec![TaskResearchPlanStepV1 {
                source_public_snapshot_id: "root".to_string(),
                choice: TaskPublicChoiceV1::one(if name == "alternate" { 1 } else { 0 }),
            }],
        }
    }

    fn candidate(name: &str) -> TaskResearchPlanCandidateV1 {
        TaskResearchPlanCandidateV1 {
            plan: plan(name),
            outcome: TaskResearchPlanOutcomeV1 {
                final_hp: 10,
                final_gold: 0,
                persistent: TaskResearchPlanPersistentOutcomeV1 {
                    max_hp: 80,
                    recoverable_gold_delta: 0,
                    unrecovered_stolen_gold: 0,
                    ritual_dagger_value: 0,
                    genetic_algorithm_value: 0,
                    external_burden_count: 0,
                },
                consumed_entry_potion_slots: Vec::new(),
                total_potions_consumed: 0,
                generated_potions: Vec::new(),
                final_relics: Vec::new(),
                meta_changes: Vec::new(),
                pending_rewards: Vec::new(),
                combat_mugged: false,
            },
            satisfies_requested_quality: false,
            supporting_provenances: vec![provenance(name)],
        }
    }

    #[test]
    fn rewarded_exit_compares_resources_without_rewriting_exit_facts() {
        let mut rewarded = candidate("rewarded");
        rewarded.outcome.final_gold = 100;
        rewarded.outcome.pending_rewards = vec![
            TaskResearchPlanPendingRewardV1::StolenGold { amount: 40 },
            TaskResearchPlanPendingRewardV1::Gold { amount: 10 },
        ];
        let mut mugged = rewarded.clone();
        mugged.outcome.pending_rewards.clear();
        mugged.outcome.combat_mugged = true;
        mugged.outcome.persistent.recoverable_gold_delta = -40;
        mugged.outcome.persistent.unrecovered_stolen_gold = 40;
        let before = mugged.clone();
        assert!(rewarded
            .outcome
            .rewarded_exit_strictly_dominates_v1(&mugged.outcome));
        assert!(!rewarded.outcome.strictly_dominates(&mugged.outcome));
        assert!(!mugged
            .outcome
            .rewarded_exit_strictly_dominates_v1(&rewarded.outcome));
        assert_eq!(mugged, before);
        assert_eq!(
            task_research_plan_non_dominated_candidate_ordinals_v1(&[rewarded, mugged]),
            vec![0, 1]
        );
    }

    #[test]
    fn rewarded_exit_requires_resource_improvement_and_keeps_tradeoffs() {
        let rewarded = candidate("rewarded");
        let mut mugged = rewarded.clone();
        mugged.outcome.combat_mugged = true;
        assert!(!rewarded
            .outcome
            .rewarded_exit_strictly_dominates_v1(&mugged.outcome));
        let mut richer = rewarded.outcome.clone();
        richer.pending_rewards = vec![TaskResearchPlanPendingRewardV1::Gold { amount: 20 }];
        assert!(richer.rewarded_exit_strictly_dominates_v1(&mugged.outcome));
        for difference in 0..8 {
            let mut alternative = mugged.outcome.clone();
            match difference {
                0 => alternative.final_hp += 1,
                1 => alternative.final_gold += 21,
                2 => alternative.persistent.ritual_dagger_value += 1,
                3 => alternative
                    .pending_rewards
                    .push(TaskResearchPlanPendingRewardV1::EmeraldKey),
                4 => alternative
                    .final_relics
                    .push(RelicState::new(RelicId::IncenseBurner)),
                5 => alternative.meta_changes.push(
                    TaskResearchPlanMetaChangeV1::AddCardToMasterDeck {
                        card_id: CardId::Bash,
                    },
                ),
                6 => alternative
                    .generated_potions
                    .push(TaskResearchPlanGeneratedPotionV1 {
                        slot: 0,
                        potion_id: PotionId::BlockPotion,
                    }),
                _ => {
                    richer.consumed_entry_potion_slots = vec![0];
                    richer.total_potions_consumed = 1;
                }
            }
            assert!(!richer.rewarded_exit_strictly_dominates_v1(&alternative));
        }
    }

    #[test]
    fn attempt_requires_at_least_one_search_lane() {
        let attempt = TaskResearchPlanAttemptV1 {
            candidates: Vec::new(),
            lanes: Vec::new(),
        };

        assert!(attempt.validate().is_err());
    }

    #[test]
    fn an_earlier_candidate_lane_may_coexist_with_a_challenger_miss() {
        let attempt = TaskResearchPlanAttemptV1 {
            candidates: vec![candidate("resident")],
            lanes: vec![
                lane("resident", TaskResearchPlanAttemptExitV1::Witness),
                lane(
                    "challenger",
                    TaskResearchPlanAttemptExitV1::ExpandedNodeBudgetExhausted,
                ),
            ],
        };

        assert!(attempt.validate().is_ok());
        assert_eq!(
            attempt.last_lane_exit(),
            TaskResearchPlanAttemptExitV1::ExpandedNodeBudgetExhausted
        );
        assert_eq!(
            attempt.summary_exit(),
            TaskResearchPlanAttemptExitV1::Witness
        );
    }

    #[test]
    fn delivered_candidate_must_match_a_witness_lane() {
        let attempt = TaskResearchPlanAttemptV1 {
            candidates: vec![candidate("resident")],
            lanes: vec![lane("challenger", TaskResearchPlanAttemptExitV1::Witness)],
        };

        assert!(attempt.validate().is_err());
    }

    #[test]
    fn witness_lane_cannot_omit_its_plan() {
        let attempt = TaskResearchPlanAttemptV1 {
            candidates: Vec::new(),
            lanes: vec![lane("resident", TaskResearchPlanAttemptExitV1::Witness)],
        };

        assert!(attempt.validate().is_err());
    }

    #[test]
    fn witness_cannot_be_returned_without_a_candidate() {
        let attempt = TaskResearchPlanAttemptV1 {
            candidates: Vec::new(),
            lanes: vec![lane("resident", TaskResearchPlanAttemptExitV1::Witness)],
        };

        assert!(attempt.validate().is_err());
    }

    #[test]
    fn multiple_witness_lanes_each_retain_one_candidate_without_selecting() {
        let mut challenger = candidate("challenger");
        challenger.plan = plan("alternate");
        let attempt = TaskResearchPlanAttemptV1 {
            candidates: vec![candidate("resident"), challenger],
            lanes: vec![
                lane("resident", TaskResearchPlanAttemptExitV1::Witness),
                lane("challenger", TaskResearchPlanAttemptExitV1::Witness),
            ],
        };

        assert!(attempt.validate().is_ok());
    }

    #[test]
    fn one_witness_lane_may_retain_multiple_frontier_candidates() {
        let mut alternate = candidate("resident");
        alternate.plan = plan("alternate");
        let attempt = TaskResearchPlanAttemptV1 {
            candidates: vec![candidate("resident"), alternate],
            lanes: vec![lane("resident", TaskResearchPlanAttemptExitV1::Witness)],
        };

        assert!(attempt.validate().is_ok());
    }

    #[test]
    fn one_execution_may_retain_multiple_supporting_witness_lanes() {
        let mut shared = candidate("resident");
        shared.supporting_provenances.push(provenance("challenger"));
        shared.supporting_provenances.sort();
        let attempt = TaskResearchPlanAttemptV1 {
            candidates: vec![shared],
            lanes: vec![
                lane("resident", TaskResearchPlanAttemptExitV1::Witness),
                lane("challenger", TaskResearchPlanAttemptExitV1::Witness),
            ],
        };

        assert!(attempt.validate().is_ok());
    }

    #[test]
    fn duplicate_execution_candidates_are_rejected() {
        let attempt = TaskResearchPlanAttemptV1 {
            candidates: vec![candidate("resident"), candidate("challenger")],
            lanes: vec![
                lane("resident", TaskResearchPlanAttemptExitV1::Witness),
                lane("challenger", TaskResearchPlanAttemptExitV1::Witness),
            ],
        };

        assert_eq!(
            attempt.validate(),
            Err("research plan attempt contains duplicate execution plans".to_string())
        );
    }

    #[test]
    fn requested_quality_does_not_override_an_incomparable_resource_tradeoff() {
        let mut conserving = candidate("resident");
        conserving.outcome.final_hp = 31;
        let mut zero_loss = candidate("challenger");
        zero_loss.outcome.final_hp = 74;
        zero_loss.outcome.consumed_entry_potion_slots = vec![0];
        zero_loss.outcome.total_potions_consumed = 1;
        zero_loss.satisfies_requested_quality = true;

        assert!(!conserving.outcome.strictly_dominates(&zero_loss.outcome));
        assert!(!zero_loss.outcome.strictly_dominates(&conserving.outcome));
    }

    #[test]
    fn fact_frontier_removes_dominated_lines_but_keeps_resource_tradeoffs() {
        let dominated = candidate("dominated");
        let mut conserving = candidate("conserving");
        conserving.outcome.final_hp = 20;
        let mut potion_line = candidate("potion-line");
        potion_line.outcome.final_hp = 30;
        potion_line.outcome.consumed_entry_potion_slots = vec![0];
        potion_line.outcome.total_potions_consumed = 1;

        assert_eq!(
            task_research_plan_non_dominated_candidate_ordinals_v1(&[
                dominated,
                conserving,
                potion_line,
            ]),
            vec![1, 2]
        );
    }

    #[test]
    fn claimable_gold_applies_combat_bonus_once_and_preserves_theft_identity() {
        let mut outcome = candidate("gold").outcome;
        outcome.final_gold = 326;
        outcome.final_relics = vec![RelicState::new(RelicId::GoldenIdol)];
        outcome.pending_rewards = vec![
            TaskResearchPlanPendingRewardV1::StolenGold { amount: 40 },
            TaskResearchPlanPendingRewardV1::Gold { amount: 18 },
        ];
        assert_eq!(outcome.claimable_gold_v1(), 389);
        outcome.final_gold = 389;
        outcome.pending_rewards.clear();
        assert_eq!(outcome.claimable_gold_v1(), 389);

        outcome.final_gold = 262;
        outcome.pending_rewards = vec![TaskResearchPlanPendingRewardV1::Gold { amount: 76 }];
        assert_eq!(outcome.claimable_gold_v1(), 357);
        outcome
            .final_relics
            .push(RelicState::new(RelicId::Ectoplasm));
        outcome
            .pending_rewards
            .push(TaskResearchPlanPendingRewardV1::StolenGold { amount: 40 });
        assert_eq!(outcome.claimable_gold_v1(), 262);
    }

    #[test]
    fn dominance_normalizes_gold_already_materialized_as_a_pending_reward() {
        let mut healthier = candidate("healthier");
        healthier.outcome.final_hp = 69;
        healthier.outcome.final_gold = 42;
        healthier.outcome.pending_rewards =
            vec![TaskResearchPlanPendingRewardV1::StolenGold { amount: 30 }];
        let mut lower_hp = candidate("lower-hp");
        lower_hp.outcome.final_hp = 61;
        lower_hp.outcome.final_gold = 57;
        lower_hp.outcome.pending_rewards =
            vec![TaskResearchPlanPendingRewardV1::StolenGold { amount: 15 }];

        assert!(healthier.outcome.strictly_dominates(&lower_hp.outcome));
        assert!(!lower_hp.outcome.strictly_dominates(&healthier.outcome));
        assert!(healthier
            .outcome
            .claimable_gold_and_pending_rewards_equivalent_v1(&lower_hp.outcome));
    }

    #[test]
    fn dominance_preserves_a_real_hp_against_claimable_gold_tradeoff() {
        let mut healthier = candidate("healthier");
        healthier.outcome.final_hp = 69;
        healthier.outcome.final_gold = 42;
        healthier.outcome.pending_rewards =
            vec![TaskResearchPlanPendingRewardV1::StolenGold { amount: 15 }];
        let mut richer = candidate("richer");
        richer.outcome.final_hp = 61;
        richer.outcome.final_gold = 57;
        richer.outcome.pending_rewards =
            vec![TaskResearchPlanPendingRewardV1::StolenGold { amount: 15 }];

        assert!(!healthier.outcome.strictly_dominates(&richer.outcome));
        assert!(!richer.outcome.strictly_dominates(&healthier.outcome));
    }

    #[test]
    fn pending_reward_projection_preserves_typed_public_identity() {
        let rewards = [
            PlannerRewardDescriptor::Gold { amount: 30 },
            PlannerRewardDescriptor::Relic {
                relic: RelicId::RegalPillow,
            },
            PlannerRewardDescriptor::Potion {
                potion: PotionId::HeartOfIron,
            },
            PlannerRewardDescriptor::CardReward {
                cards: vec![crate::ai::planner_core::PlannerOfferedCard {
                    card: CardId::TrueGrit,
                    upgrades: 1,
                }],
            },
        ];

        for reward in rewards {
            let projected = TaskResearchPlanPendingRewardV1::from_planner_descriptor_v1(&reward);
            assert!(projected.matches_planner_descriptor_v1(&reward));
        }
    }
}
