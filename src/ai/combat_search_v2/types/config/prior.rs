use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct CombatSearchV2RootActionPrior {
    scores_by_state: Arc<HashMap<String, HashMap<String, f64>>>,
    duplicate_hint_count: usize,
}

impl CombatSearchV2RootActionPrior {
    pub fn from_scores(scores_by_state: HashMap<String, HashMap<String, f64>>) -> Self {
        Self::from_scores_with_duplicate_count(scores_by_state, 0)
    }

    pub fn from_scores_with_duplicate_count(
        scores_by_state: HashMap<String, HashMap<String, f64>>,
        duplicate_hint_count: usize,
    ) -> Self {
        Self {
            scores_by_state: Arc::new(scores_by_state),
            duplicate_hint_count,
        }
    }

    pub fn score(&self, exact_state_hash: &str, action_key: &str) -> Option<f64> {
        self.scores_by_state
            .get(exact_state_hash)
            .and_then(|scores| scores.get(action_key))
            .copied()
    }

    pub fn is_empty(&self) -> bool {
        self.scores_by_state.is_empty()
    }

    pub fn duplicate_hint_count(&self) -> usize {
        self.duplicate_hint_count
    }
}

#[derive(Clone, Debug, Default)]
pub struct CombatSearchV2TurnPlanPrior {
    scores_by_state: Arc<HashMap<String, HashMap<String, f64>>>,
    duplicate_hint_count: usize,
}

impl CombatSearchV2TurnPlanPrior {
    pub fn from_scores(scores_by_state: HashMap<String, HashMap<String, f64>>) -> Self {
        Self::from_scores_with_duplicate_count(scores_by_state, 0)
    }

    pub fn from_plan_scores<I, J>(scores_by_state: I) -> Self
    where
        I: IntoIterator<Item = (String, J)>,
        J: IntoIterator<Item = (Vec<String>, f64)>,
    {
        let mut keyed_scores = HashMap::new();
        for (state_hash, plan_scores) in scores_by_state {
            let mut state_scores = HashMap::new();
            for (action_keys, score) in plan_scores {
                if score.is_finite() {
                    state_scores.insert(turn_plan_action_sequence_key(&action_keys), score);
                }
            }
            keyed_scores.insert(state_hash, state_scores);
        }
        Self::from_scores(keyed_scores)
    }

    pub fn from_scores_with_duplicate_count(
        scores_by_state: HashMap<String, HashMap<String, f64>>,
        duplicate_hint_count: usize,
    ) -> Self {
        Self {
            scores_by_state: Arc::new(scores_by_state),
            duplicate_hint_count,
        }
    }

    pub fn score_for_action_keys(
        &self,
        exact_state_hash: &str,
        action_keys: &[String],
    ) -> Option<f64> {
        self.scores_by_state
            .get(exact_state_hash)
            .and_then(|scores| scores.get(&turn_plan_action_sequence_key(action_keys)))
            .copied()
    }

    pub fn has_hints_for_state(&self, exact_state_hash: &str) -> bool {
        self.scores_by_state
            .get(exact_state_hash)
            .is_some_and(|scores| !scores.is_empty())
    }

    pub fn is_empty(&self) -> bool {
        self.scores_by_state.is_empty()
    }

    pub fn duplicate_hint_count(&self) -> usize {
        self.duplicate_hint_count
    }
}

pub fn turn_plan_action_sequence_key(action_keys: &[String]) -> String {
    action_keys.join("\u{1f}")
}
