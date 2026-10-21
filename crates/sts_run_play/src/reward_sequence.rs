//! An ordered selection of current reward occurrences, prepared by the existing
//! exact line owner. Only gold/potions followed by opening a card reward qualify.
use std::collections::{HashMap, HashSet};

use sts_oracle_learning::agent::task::{
    TaskPublicCandidateV1, TaskPublicChoiceV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskStrategicActionV1, TaskStrategicObservationV1,
};
use sts_oracle_learning::ai::planner_core::{PlannerDecisionContext, PlannerRewardDescriptor};
use sts_oracle_learning::content::potions::PotionId;

pub(crate) struct RewardSequenceV1 {
    rewards: Vec<PlannerRewardDescriptor>,
    indices: Vec<usize>,
    potions: HashMap<PotionId, usize>,
}

impl RewardSequenceV1 {
    pub fn compile(
        root: &TaskPublicDecisionV1,
        choices: &[TaskPublicChoiceV1],
    ) -> Result<Self, String> {
        if choices.is_empty() || choices.len() > 8 {
            return Err("Reward sequence requires 1..8 choices".into());
        }
        let observation = strategic(root)?;
        let PlannerDecisionContext::Reward { pending_rewards } = &observation.context else {
            return Err("Reward sequence requires the current rewards page".into());
        };
        let mut indices = Vec::new();
        let mut seen = HashSet::new();
        for (step, choice) in choices.iter().enumerate() {
            root.validate_choice(choice).map_err(|e| e.to_string())?;
            let [ordinal] = choice.ordinal_path.as_slice() else {
                return Err("Reward sequence requires individual reward choices".into());
            };
            let action = action(&root.candidates[*ordinal])?;
            let index = if step + 1 == choices.len() {
                let TaskStrategicActionV1::OpenCardReward { reward_item_index } = action else {
                    return Err("Reward sequence must end by opening a card reward".into());
                };
                if !matches!(
                    pending_rewards.get(*reward_item_index),
                    Some(PlannerRewardDescriptor::CardReward { .. })
                ) {
                    return Err("Card reward occurrence is unavailable".into());
                }
                *reward_item_index
            } else {
                let TaskStrategicActionV1::ClaimReward {
                    reward_item_index,
                    reward,
                } = action
                else {
                    return Err("Only gold and potion claims may precede the card reward".into());
                };
                if !matches!(
                    reward,
                    PlannerRewardDescriptor::Gold { .. }
                        | PlannerRewardDescriptor::StolenGold { .. }
                        | PlannerRewardDescriptor::Potion { .. }
                ) || pending_rewards.get(*reward_item_index) != Some(reward)
                {
                    return Err(
                        "Reward sequence only accepts current gold and potion occurrences".into(),
                    );
                }
                *reward_item_index
            };
            if !seen.insert(index) {
                return Err("Reward sequence repeats a reward occurrence".into());
            }
            indices.push(index);
        }
        Ok(Self {
            rewards: pending_rewards.clone(),
            indices,
            potions: potion_counts(observation),
        })
    }

    pub fn len(&self) -> usize {
        self.indices.len()
    }

    // Reindex only after proving that the preceding selected occurrences were
    // removed and the entire remaining reward inventory is unchanged.
    fn current_index(&self, step: usize) -> usize {
        let original = self.indices[step];
        original
            - self.indices[..step]
                .iter()
                .filter(|i| **i < original)
                .count()
    }

    pub fn resolve(
        &self,
        step: usize,
        decision: &TaskPublicDecisionV1,
    ) -> Result<TaskPublicChoiceV1, String> {
        let observation = strategic(decision)?;
        self.check_potions(step, observation)?;
        let expected = self.remaining_rewards(step);
        if observation.context
            != (PlannerDecisionContext::Reward {
                pending_rewards: expected,
            })
        {
            return Err("Reward inventory changed during the selected sequence".into());
        }
        let index = self.current_index(step);
        let expected_action = if step + 1 == self.len() {
            TaskStrategicActionV1::OpenCardReward {
                reward_item_index: index,
            }
        } else {
            TaskStrategicActionV1::ClaimReward {
                reward_item_index: index,
                reward: self.rewards[self.indices[step]].clone(),
            }
        };
        let matches: Vec<_> = decision.candidates.iter().enumerate()
            .filter(|(_, candidate)| matches!(candidate, TaskPublicCandidateV1::Strategic { action } if *action == expected_action))
            .map(|(ordinal, _)| TaskPublicChoiceV1::one(ordinal)).collect();
        match matches.as_slice() {
            [choice] => Ok(choice.clone()),
            _ => Err("Selected reward is unavailable or ambiguous; sequence not committed".into()),
        }
    }

    fn remaining_rewards(&self, completed: usize) -> Vec<PlannerRewardDescriptor> {
        self.rewards
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.indices[..completed].contains(i))
            .map(|(_, r)| r.clone())
            .collect()
    }

    pub fn check_endpoint(&self, decision: &TaskPublicDecisionV1) -> Result<(), String> {
        let observation = strategic(decision)?;
        self.check_potions(self.len() - 1, observation)?;
        if observation.context
            != (PlannerDecisionContext::CardReward {
                reward_item_index: Some(self.current_index(self.len() - 1)),
                pending_rewards: self.remaining_rewards(self.len() - 1),
            })
        {
            return Err("Reward sequence did not reach its selected card reward".into());
        }
        Ok(())
    }

    fn check_potions(
        &self,
        completed: usize,
        observation: &TaskStrategicObservationV1,
    ) -> Result<(), String> {
        let mut expected = self.potions.clone();
        for index in &self.indices[..completed] {
            if let PlannerRewardDescriptor::Potion { potion } = self.rewards[*index] {
                *expected.entry(potion).or_default() += 1;
            }
        }
        if potion_counts(observation) != expected {
            return Err("Selected potion was not obtained; sequence not committed".into());
        }
        Ok(())
    }
}

fn strategic(decision: &TaskPublicDecisionV1) -> Result<&TaskStrategicObservationV1, String> {
    match &decision.observation {
        TaskPublicObservationV1::Strategic { observation } => Ok(observation),
        _ => Err("Reward sequence reached an unexpected combat decision".into()),
    }
}

fn action(candidate: &TaskPublicCandidateV1) -> Result<&TaskStrategicActionV1, String> {
    match candidate {
        TaskPublicCandidateV1::Strategic { action } => Ok(action),
        _ => Err("Reward sequence requires reward actions".into()),
    }
}

fn potion_counts(observation: &TaskStrategicObservationV1) -> HashMap<PotionId, usize> {
    let mut counts = HashMap::new();
    for potion in observation.potions.iter().filter_map(|s| s.potion) {
        *counts.entry(potion.potion).or_default() += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_learning::agent::task::TaskDecisionSiteV1;

    #[test]
    fn reward_sequence_preserves_the_entire_pending_inventory_when_card_reward_opens() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tools/expert/tests/fixtures/public-strategic-sites.json"
        ))
        .unwrap();
        let retained: TaskPublicDecisionV1 =
            serde_json::from_value(fixture["cases"]["reward"]["payload"]["decision"].clone())
                .unwrap();
        let root = TaskPublicDecisionV1::new(
            TaskDecisionSiteV1::Reward,
            retained.observation,
            retained.candidates,
        )
        .unwrap();
        let sequence = RewardSequenceV1::compile(
            &root,
            &[TaskPublicChoiceV1::one(0), TaskPublicChoiceV1::one(1)],
        )
        .unwrap();
        let mut observation = strategic(&root).unwrap().clone();
        observation.decision_site =
            sts_oracle_learning::ai::planner_core::PlannerDecisionSite::CardReward;
        observation.context = PlannerDecisionContext::CardReward {
            reward_item_index: Some(0),
            pending_rewards: sequence.remaining_rewards(1),
        };
        let decision = |observation| {
            TaskPublicDecisionV1::new(
                TaskDecisionSiteV1::CardReward,
                TaskPublicObservationV1::Strategic { observation },
                vec![TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::SkipCardReward {
                        reward_item_index: 0,
                    },
                }],
            )
            .unwrap()
        };
        assert!(sequence
            .check_endpoint(&decision(observation.clone()))
            .is_ok());
        let PlannerDecisionContext::CardReward {
            pending_rewards, ..
        } = &mut observation.context
        else {
            unreachable!()
        };
        pending_rewards.pop(); // Unselected rewards must not silently disappear.
        assert!(sequence.check_endpoint(&decision(observation)).is_err());
    }
}
