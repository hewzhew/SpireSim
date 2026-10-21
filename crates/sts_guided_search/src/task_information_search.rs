//! Bounded policy search over task-public decisions and caller-supplied
//! whole-run scenarios. This is a research core, not a posterior sampler or a
//! deployed actor. Scenario provenance and leaf values belong to the caller.

use sts_combat_outcome::agent::task::{
    GameOutcomeV1, TaskPublicChoiceV1, TaskPublicDecisionV1, TaskPublicStepV1,
};

#[derive(Clone, Debug, PartialEq)]
pub enum PublicBoundary {
    Decision(TaskPublicDecisionV1),
    Terminal(GameOutcomeV1),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PublicPolicyHistoryStep {
    Recorded(TaskPublicStepV1),
    Hypothetical {
        source: TaskPublicDecisionV1,
        choice: TaskPublicChoiceV1,
        observed: PublicBoundary,
    },
}

/// An exact scenario may carry hidden state. Neither action enumeration nor
/// leaf evaluation receives it; they see only the common public history.
/// Deployment requires an independently justified public-history scenario
/// distribution. A single true checkpoint is not such a distribution.
pub trait ScenarioModel {
    type Scenario: Clone;

    fn observe(&self, scenario: &Self::Scenario) -> Result<PublicBoundary, String>;

    /// Return complete, source-bound public choices for this experiment.
    /// A selection family must be expanded to complete ordinal paths here.
    fn choices(
        &self,
        history: &[PublicPolicyHistoryStep],
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskPublicChoiceV1>, String>;

    /// Conditional masses must be finite, positive, and sum to one. A model
    /// gap is an error, not an invitation to silently omit a successor.
    fn step(
        &self,
        scenario: &Self::Scenario,
        choice: &TaskPublicChoiceV1,
    ) -> Result<Vec<WeightedScenario<Self::Scenario>>, String>;
}

#[derive(Clone, Debug)]
pub struct WeightedScenario<S> {
    pub scenario: S,
    pub mass: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioBasis {
    Constructed,
    PublicHistoryHypothesis,
    PrivilegedExactDiagnostic,
}

#[derive(Clone, Debug)]
pub struct ScenarioSet<S> {
    pub basis: ScenarioBasis,
    pub source: String,
    pub samples: Vec<WeightedScenario<S>>,
}

#[derive(Clone, Copy, Debug)]
pub struct SearchBudget {
    pub max_decisions: usize,
    pub max_transitions: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyBranch {
    pub observed: PublicBoundary,
    /// Fraction of the current scenario mass under the caller's assumptions.
    pub scenario_mass: f64,
    pub continuation: Policy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    pub boundary: PublicBoundary,
    /// Scenario-weighted caller value, not a calibrated win probability.
    pub value: f64,
    pub choice: Option<TaskPublicChoiceV1>,
    pub branches: Vec<PolicyBranch>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RootAlternative {
    pub choice: TaskPublicChoiceV1,
    pub value: f64,
}

/// Complete only over the supplied scenario model, action enumeration, and
/// decision horizon. It is not a whole-run optimality or probability claim.
#[derive(Clone, Debug, PartialEq)]
pub struct CompleteSearch {
    pub scenario_basis: ScenarioBasis,
    pub scenario_source: String,
    pub policy: Policy,
    pub alternatives: Vec<RootAlternative>,
    pub scenario_transitions: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SearchOutcome {
    Complete(CompleteSearch),
    Unknown {
        scenario_basis: ScenarioBasis,
        scenario_source: String,
        reason: String,
        scenario_transitions: usize,
    },
}

struct Search<'a, M, E>
where
    M: ScenarioModel,
    E: Fn(&[PublicPolicyHistoryStep], &PublicBoundary) -> Option<f64>,
{
    model: &'a M,
    evaluate: &'a E,
    budget: SearchBudget,
    transitions: usize,
}

struct SolvedNode {
    policy: Policy,
    alternatives: Vec<RootAlternative>,
}

/// Enumerates an observation-contingent policy through one shared budget.
/// If any candidate cannot be evaluated, no recommendation is published.
/// The caller may then use its declared public fallback policy.
pub fn search<M, E>(
    model: &M,
    scenario_set: ScenarioSet<M::Scenario>,
    public_history: &[TaskPublicStepV1],
    budget: SearchBudget,
    evaluate: &E,
) -> SearchOutcome
where
    M: ScenarioModel,
    E: Fn(&[PublicPolicyHistoryStep], &PublicBoundary) -> Option<f64>,
{
    let ScenarioSet {
        basis,
        source,
        samples,
    } = scenario_set;
    let mut search = Search {
        model,
        evaluate,
        budget,
        transitions: 0,
    };
    let history = public_history
        .iter()
        .cloned()
        .map(PublicPolicyHistoryStep::Recorded)
        .collect::<Vec<_>>();
    let result = if source.trim().is_empty() {
        Err("scenario source is empty".into())
    } else {
        search
            .validate_scenarios(&samples)
            .and_then(|_| search.solve(samples, &history, budget.max_decisions))
    };
    match result {
        Ok(solved) => SearchOutcome::Complete(CompleteSearch {
            scenario_basis: basis,
            scenario_source: source,
            policy: solved.policy,
            alternatives: solved.alternatives,
            scenario_transitions: search.transitions,
        }),
        Err(reason) => SearchOutcome::Unknown {
            scenario_basis: basis,
            scenario_source: source,
            reason,
            scenario_transitions: search.transitions,
        },
    }
}

impl<M, E> Search<'_, M, E>
where
    M: ScenarioModel,
    E: Fn(&[PublicPolicyHistoryStep], &PublicBoundary) -> Option<f64>,
{
    fn validate_scenarios(
        &self,
        scenarios: &[WeightedScenario<M::Scenario>],
    ) -> Result<(), String> {
        if scenarios.is_empty() {
            return Err("no public-history scenarios supplied".into());
        }
        if scenarios
            .iter()
            .any(|sample| !sample.mass.is_finite() || sample.mass <= 0.0)
        {
            return Err("scenario masses must be finite and positive".into());
        }
        let total: f64 = scenarios.iter().map(|sample| sample.mass).sum();
        if !total.is_finite() || total <= 0.0 {
            return Err("scenario mass overflow".into());
        }
        Ok(())
    }

    fn solve(
        &mut self,
        scenarios: Vec<WeightedScenario<M::Scenario>>,
        history: &[PublicPolicyHistoryStep],
        remaining_decisions: usize,
    ) -> Result<SolvedNode, String> {
        let first = self.model.observe(&scenarios[0].scenario)?;
        for sample in scenarios.iter().skip(1) {
            if self.model.observe(&sample.scenario)? != first {
                return Err(
                    "scenarios at one information node disagree on the public boundary".into(),
                );
            }
        }
        let PublicBoundary::Decision(decision) = &first else {
            return self.leaf(history, first);
        };
        decision.validate()?;
        if remaining_decisions == 0 {
            return self.leaf(history, first);
        }
        let choices = self.model.choices(history, decision)?;
        if choices.is_empty() {
            return Err("public action enumeration is empty".into());
        }
        let mut seen = Vec::new();
        for choice in &choices {
            decision.validate_choice(choice)?;
            if seen.contains(choice) {
                return Err("public action enumeration repeats a choice".into());
            }
            seen.push(choice.clone());
        }
        if (0..decision.candidates.len()).any(|ordinal| {
            !choices
                .iter()
                .any(|choice| choice.ordinal_path[0] == ordinal)
        }) {
            return Err("public action enumeration omits a candidate family".into());
        }
        let mut alternatives = Vec::with_capacity(choices.len());
        let mut best: Option<Policy> = None;
        for choice in choices {
            let mut groups: Vec<(PublicBoundary, Vec<WeightedScenario<M::Scenario>>)> = Vec::new();
            for sample in &scenarios {
                if self.transitions >= self.budget.max_transitions {
                    return Err("scenario transition budget exhausted".into());
                }
                self.transitions += 1;
                let successors = self.model.step(&sample.scenario, &choice)?;
                self.validate_conditional(&successors)?;
                for successor in successors {
                    let boundary = self.model.observe(&successor.scenario)?;
                    if let PublicBoundary::Decision(next) = &boundary {
                        next.validate()?;
                    }
                    let weighted = WeightedScenario {
                        scenario: successor.scenario,
                        mass: sample.mass * successor.mass,
                    };
                    if !weighted.mass.is_finite() || weighted.mass <= 0.0 {
                        return Err("successor mass overflow or underflow".into());
                    }
                    if let Some((_, members)) =
                        groups.iter_mut().find(|(known, _)| *known == boundary)
                    {
                        members.push(weighted);
                    } else {
                        groups.push((boundary, vec![weighted]));
                    }
                }
            }
            let total_mass: f64 = groups
                .iter()
                .flat_map(|(_, group)| group.iter().map(|sample| sample.mass))
                .sum();
            let mut branches = Vec::with_capacity(groups.len());
            let mut value = 0.0;
            for (observed, group) in groups {
                let branch_mass: f64 = group.iter().map(|sample| sample.mass).sum();
                let mut next_history = history.to_vec();
                next_history.push(PublicPolicyHistoryStep::Hypothetical {
                    source: decision.clone(),
                    choice: choice.clone(),
                    observed: observed.clone(),
                });
                let continuation = self
                    .solve(group, &next_history, remaining_decisions - 1)?
                    .policy;
                let scenario_mass = branch_mass / total_mass;
                value += scenario_mass * continuation.value;
                branches.push(PolicyBranch {
                    observed,
                    scenario_mass,
                    continuation,
                });
            }
            if !value.is_finite() {
                return Err("policy evaluation is not finite".into());
            }
            alternatives.push(RootAlternative {
                choice: choice.clone(),
                value,
            });
            if best.as_ref().is_none_or(|old| value > old.value) {
                best = Some(Policy {
                    boundary: first.clone(),
                    value,
                    choice: Some(choice),
                    branches,
                });
            }
        }
        Ok(SolvedNode {
            policy: best.expect("nonempty public action enumeration"),
            alternatives,
        })
    }

    fn leaf(
        &self,
        history: &[PublicPolicyHistoryStep],
        boundary: PublicBoundary,
    ) -> Result<SolvedNode, String> {
        let value = (self.evaluate)(history, &boundary)
            .filter(|value| value.is_finite())
            .ok_or_else(|| "leaf value is unknown".to_string())?;
        Ok(SolvedNode {
            policy: Policy {
                boundary,
                value,
                choice: None,
                branches: Vec::new(),
            },
            alternatives: Vec::new(),
        })
    }

    fn validate_conditional(
        &self,
        successors: &[WeightedScenario<M::Scenario>],
    ) -> Result<(), String> {
        self.validate_scenarios(successors)?;
        let sum: f64 = successors.iter().map(|sample| sample.mass).sum();
        if (sum - 1.0).abs() > 1e-9 {
            return Err("scenario transition omits or overcounts conditional mass".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_combat_outcome::agent::{
        information::{
            action::{project_public_combat_actions_v1, PublicCombatAtomicActionV1},
            run::{PublicCombatRunContextGapV1, PublicCombatRunContextV1},
            state::public_combat_state_v1,
        },
        task::{
            task_public_combat_decision_v1, TaskDecisionSiteV1, TaskPublicCandidateV1,
            TaskPublicObservationV1, TaskStrategicActionV1, TaskStrategicCardV1,
            TaskStrategicObservationV1, TaskStrategicPotionSlotV1, TaskStrategicPotionV1,
        },
    };
    use sts_core::{
        ai::planner_core::{
            PlannerDecisionContext, PlannerDecisionSite, PlannerMechanicsManifest,
            PlannerPlayerClass, PlannerPublicHistory, PlannerPublicMap, PlannerRunGoal,
            PlannerRunScalars,
        },
        content::{
            cards::CardId,
            monsters::EnemyId,
            potions::{Potion, PotionId},
        },
        state::EngineState,
        test_support::{blank_test_combat, test_monster},
    };

    #[derive(Clone, Copy)]
    enum Stage {
        Combat,
        Reward,
        Map,
        Terminal(GameOutcomeV1),
    }

    #[derive(Clone)]
    struct ToyScenario {
        hidden_route: usize,
        stage: Stage,
        spent_potion: bool,
        took_card: bool,
    }

    struct ToyModel {
        combat: TaskPublicDecisionV1,
        reveal_reward: bool,
        potion_ordinal: usize,
        omit_last_choice: bool,
    }

    impl ToyModel {
        fn new(reveal_reward: bool) -> Self {
            let mut combat = blank_test_combat();
            combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
            combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 41))];
            let actions = project_public_combat_actions_v1(&EngineState::CombatPlayerTurn, &combat)
                .expect("public combat actions");
            let decision = task_public_combat_decision_v1(
                PublicCombatRunContextV1::Unavailable {
                    reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
                },
                public_combat_state_v1(&combat),
                &actions.public,
            )
            .expect("public combat decision");
            let potion_ordinal = decision
                .candidates
                .iter()
                .position(|candidate| {
                    matches!(
                        candidate,
                        TaskPublicCandidateV1::CombatAtomic {
                            action: PublicCombatAtomicActionV1::UsePotion { .. }
                        }
                    )
                })
                .expect("potion action");
            assert!(decision.candidates.len() >= 2, "controlled combat choice");
            Self {
                combat: decision,
                reveal_reward,
                potion_ordinal,
                omit_last_choice: false,
            }
        }

        fn scenario(hidden_route: usize) -> WeightedScenario<ToyScenario> {
            WeightedScenario {
                scenario: ToyScenario {
                    hidden_route,
                    stage: Stage::Combat,
                    spent_potion: false,
                    took_card: false,
                },
                mass: 0.5,
            }
        }

        fn strategic_decision(&self, state: &ToyScenario) -> TaskPublicDecisionV1 {
            let reward_card = if self.reveal_reward && state.hidden_route == 1 {
                CardId::Defend
            } else {
                CardId::Strike
            };
            let (site, planner_site, context, candidates, floor) = match state.stage {
                Stage::Reward => (
                    TaskDecisionSiteV1::CardReward,
                    PlannerDecisionSite::CardReward,
                    PlannerDecisionContext::CardReward {
                        reward_item_index: Some(0),
                        pending_rewards: Vec::new(),
                    },
                    vec![
                        TaskPublicCandidateV1::Strategic {
                            action: TaskStrategicActionV1::TakeCard {
                                reward_item_index: Some(0),
                                option_index: 0,
                                card: reward_card,
                                upgrades: 0,
                            },
                        },
                        TaskPublicCandidateV1::Strategic {
                            action: TaskStrategicActionV1::SkipCardReward {
                                reward_item_index: 0,
                            },
                        },
                    ],
                    1,
                ),
                Stage::Map => (
                    TaskDecisionSiteV1::Map,
                    PlannerDecisionSite::Map,
                    PlannerDecisionContext::Map { overlay: false },
                    (0..2)
                        .map(|x| TaskPublicCandidateV1::Strategic {
                            action: TaskStrategicActionV1::ChooseRouteNode {
                                x,
                                y: 1,
                                flight: false,
                                question_mark_outcomes: Vec::new(),
                            },
                        })
                        .collect(),
                    2,
                ),
                _ => panic!("not a strategic fixture state"),
            };
            let visible_card = (state.took_card && self.reveal_reward).then_some(reward_card);
            TaskPublicDecisionV1::new(
                site,
                TaskPublicObservationV1::Strategic {
                    observation: TaskStrategicObservationV1 {
                        mechanics: PlannerMechanicsManifest {
                            mechanics_id: "controlled-task-scenarios".into(),
                            mechanics_version: 1,
                        },
                        run_goal: PlannerRunGoal::HeartVictory,
                        decision_site: planner_site,
                        run: PlannerRunScalars {
                            player_class: PlannerPlayerClass::Ironclad,
                            ascension_level: 20,
                            act: 1,
                            floor,
                            current_hp: 50,
                            max_hp: 75,
                            gold: 99,
                            keys: [false; 3],
                            potion_capacity: 3,
                            pending_act_entry: None,
                        },
                        cards: visible_card
                            .into_iter()
                            .map(|card| TaskStrategicCardV1 {
                                ordinal: 0,
                                card,
                                upgrades: 0,
                                misc_value: 0,
                                base_damage_override: None,
                                base_block_override: None,
                                cost_modifier: 0,
                                bottled_by: None,
                            })
                            .collect(),
                        relics: Vec::new(),
                        potions: vec![TaskStrategicPotionSlotV1 {
                            slot: 0,
                            potion: (!state.spent_potion).then_some(TaskStrategicPotionV1 {
                                potion: PotionId::FirePotion,
                                can_use: false,
                                can_discard: true,
                                requires_target: true,
                            }),
                        }],
                        public_map: PlannerPublicMap {
                            current_x: 0,
                            current_y: floor - 1,
                            boss: None,
                            nodes: Vec::new(),
                        },
                        context,
                        public_history: PlannerPublicHistory {
                            shop_purge_count: 0,
                            recent_combat_hp_loss: Some(0),
                            recent_combat_turns: Some(1),
                            recent_combat_potions_used: Some(u32::from(state.spent_potion)),
                        },
                    },
                },
                candidates,
            )
            .expect("strategic decision")
        }
    }

    impl ScenarioModel for ToyModel {
        type Scenario = ToyScenario;

        fn observe(&self, scenario: &ToyScenario) -> Result<PublicBoundary, String> {
            Ok(match scenario.stage {
                Stage::Combat => PublicBoundary::Decision(self.combat.clone()),
                Stage::Reward | Stage::Map => {
                    PublicBoundary::Decision(self.strategic_decision(scenario))
                }
                Stage::Terminal(outcome) => PublicBoundary::Terminal(outcome),
            })
        }

        fn choices(
            &self,
            _history: &[PublicPolicyHistoryStep],
            decision: &TaskPublicDecisionV1,
        ) -> Result<Vec<TaskPublicChoiceV1>, String> {
            let count = decision.candidates.len() - usize::from(self.omit_last_choice);
            Ok((0..count).map(TaskPublicChoiceV1::one).collect())
        }

        fn step(
            &self,
            scenario: &ToyScenario,
            choice: &TaskPublicChoiceV1,
        ) -> Result<Vec<WeightedScenario<ToyScenario>>, String> {
            let mut next = scenario.clone();
            let ordinal = choice.ordinal_path[0];
            match scenario.stage {
                Stage::Combat => {
                    next.spent_potion = ordinal == self.potion_ordinal;
                    next.stage = if next.spent_potion {
                        Stage::Reward
                    } else {
                        Stage::Terminal(GameOutcomeV1::Defeat)
                    };
                }
                Stage::Reward => {
                    next.took_card = ordinal == 0;
                    next.stage = Stage::Map;
                }
                Stage::Map => {
                    next.stage =
                        Stage::Terminal(if next.took_card && ordinal == next.hidden_route {
                            GameOutcomeV1::HeartVictory
                        } else {
                            GameOutcomeV1::Defeat
                        });
                }
                Stage::Terminal(_) => return Err("terminal scenario cannot step".into()),
            }
            Ok(vec![WeightedScenario {
                scenario: next,
                mass: 1.0,
            }])
        }
    }

    fn terminal_heart_value(
        history: &[PublicPolicyHistoryStep],
        boundary: &PublicBoundary,
    ) -> Option<f64> {
        if matches!(boundary, PublicBoundary::Terminal(_)) {
            assert!(matches!(
                history.last(),
                Some(PublicPolicyHistoryStep::Hypothetical {
                    observed: PublicBoundary::Terminal(_),
                    ..
                })
            ));
        }
        match boundary {
            PublicBoundary::Terminal(GameOutcomeV1::HeartVictory) => Some(1.0),
            PublicBoundary::Terminal(GameOutcomeV1::Defeat) => Some(0.0),
            _ => None,
        }
    }

    fn run(reveal_reward: bool, max_transitions: usize, reverse: bool) -> SearchOutcome {
        let model = ToyModel::new(reveal_reward);
        let mut scenarios = vec![ToyModel::scenario(0), ToyModel::scenario(1)];
        if reverse {
            scenarios.reverse();
        }
        search(
            &model,
            ScenarioSet {
                basis: ScenarioBasis::Constructed,
                source: "controlled two-world task fixture".into(),
                samples: scenarios,
            },
            &[],
            SearchBudget {
                max_decisions: 3,
                max_transitions,
            },
            &terminal_heart_value,
        )
    }

    #[test]
    fn one_policy_crosses_combat_reward_and_map_after_observing_the_reward() {
        let SearchOutcome::Complete(report) = run(true, 100, false) else {
            panic!("controlled policy should finish");
        };
        assert_eq!(report.policy.value, 1.0);
        assert_eq!(report.scenario_basis, ScenarioBasis::Constructed);
        assert!(
            matches!(report.policy.boundary, PublicBoundary::Decision(ref decision) if decision.site == TaskDecisionSiteV1::Combat)
        );
        let PublicBoundary::Decision(combat) = &report.policy.boundary else {
            unreachable!();
        };
        assert!(matches!(
            &combat.candidates[report.policy.choice.as_ref().unwrap().ordinal_path[0]],
            TaskPublicCandidateV1::CombatAtomic {
                action: PublicCombatAtomicActionV1::UsePotion { .. }
            }
        ));
        assert_eq!(
            report.policy.branches.len(),
            2,
            "reward reveals two public branches"
        );
        let route_choices = report
            .policy
            .branches
            .iter()
            .map(|reward_branch| {
                let reward = &reward_branch.continuation;
                assert!(matches!(reward.boundary, PublicBoundary::Decision(ref decision) if decision.site == TaskDecisionSiteV1::CardReward));
                let map = &reward.branches[0].continuation;
                assert!(matches!(map.boundary, PublicBoundary::Decision(ref decision) if decision.site == TaskDecisionSiteV1::Map));
                map.choice.as_ref().unwrap().ordinal_path[0]
            })
            .collect::<Vec<_>>();
        assert_eq!(route_choices, vec![0, 1]);
    }

    #[test]
    fn hidden_worlds_without_a_new_observation_cannot_choose_separate_routes() {
        let SearchOutcome::Complete(report) = run(false, 100, false) else {
            panic!("controlled policy should finish");
        };
        assert_eq!(report.policy.value, 0.5);
        assert_eq!(report.policy.branches.len(), 1);
        let SearchOutcome::Complete(reversed) = run(false, 100, true) else {
            panic!("reordered scenarios should finish");
        };
        assert_eq!(reversed.policy.value, report.policy.value);
        assert_eq!(reversed.policy.choice, report.policy.choice);
    }

    #[test]
    fn insufficient_work_is_unknown_without_a_partial_recommendation() {
        assert!(matches!(
            run(true, 2, false),
            SearchOutcome::Unknown {
                scenario_transitions: 2,
                ..
            }
        ));
    }

    #[test]
    fn horizon_without_a_public_leaf_value_is_unknown() {
        let model = ToyModel::new(true);
        assert!(matches!(
            search(
                &model,
                ScenarioSet {
                    basis: ScenarioBasis::Constructed,
                    source: "controlled two-world task fixture".into(),
                    samples: vec![ToyModel::scenario(0), ToyModel::scenario(1)],
                },
                &[],
                SearchBudget {
                    max_decisions: 2,
                    max_transitions: 100,
                },
                &terminal_heart_value,
            ),
            SearchOutcome::Unknown { reason, .. } if reason == "leaf value is unknown"
        ));
    }

    #[test]
    fn omitted_public_candidate_family_is_unknown() {
        let mut model = ToyModel::new(true);
        model.omit_last_choice = true;
        assert!(matches!(
            search(
                &model,
                ScenarioSet {
                    basis: ScenarioBasis::Constructed,
                    source: "controlled two-world task fixture".into(),
                    samples: vec![ToyModel::scenario(0), ToyModel::scenario(1)],
                },
                &[],
                SearchBudget {
                    max_decisions: 3,
                    max_transitions: 100,
                },
                &terminal_heart_value,
            ),
            SearchOutcome::Unknown { reason, .. }
                if reason == "public action enumeration omits a candidate family"
        ));
    }
}

#[cfg(test)]
#[path = "task_information_search_real_tests.rs"]
mod real_tests;
