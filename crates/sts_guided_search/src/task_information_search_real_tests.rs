//! A mechanism diagnostic over a natural Rust run. Its one true exact state is
//! deliberately not a public-history belief or a deployable policy input.

use super::*;
use sts_combat_outcome::agent::information::{
    action::{
        project_public_combat_actions_v1, resolve_public_combat_action_v1,
        PublicCombatActionChoiceV1,
    },
    run::{PublicCombatRunContextGapV1, PublicCombatRunContextV1},
    state::public_combat_state_v1,
};
use sts_combat_outcome::agent::task::task_public_combat_decision_v1;
use sts_combat_outcome::agent::{
    information::action::PublicCombatAtomicActionV1,
    task::{
        TaskDecisionSiteV1, TaskPublicCandidateV1, TaskPublicObservationV1, TaskStrategicActionV1,
        TaskWorldExplorationV1, TaskWorldV1,
    },
};
use sts_core::{
    content::{cards::CardId, monsters::factory::EncounterId},
    runtime::combat::CombatCard,
    sim::combat::{
        CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
    },
    state::map::node::RoomType,
};
use sts_oracle_learning::eval::run_control::ExactBranchingTaskWorldV1;

#[derive(Clone)]
struct ExactScenario {
    world: ExactBranchingTaskWorldV1,
    boundary: PublicBoundary,
}

struct ExactDiagnosticModel;

impl ScenarioModel for ExactDiagnosticModel {
    type Scenario = ExactScenario;

    fn observe(&self, scenario: &ExactScenario) -> Result<PublicBoundary, String> {
        Ok(scenario.boundary.clone())
    }

    fn choices(
        &self,
        _history: &[PublicPolicyHistoryStep],
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskPublicChoiceV1>, String> {
        decision
            .candidates
            .iter()
            .enumerate()
            .map(|(ordinal, candidate)| match candidate {
                TaskPublicCandidateV1::CombatSelectionFamily { .. }
                | TaskPublicCandidateV1::Strategic {
                    action: TaskStrategicActionV1::BeginRunCardSelection { .. },
                } => Err("exact diagnostic has not expanded a selection family".into()),
                _ => Ok(TaskPublicChoiceV1::one(ordinal)),
            })
            .collect()
    }

    fn step(
        &self,
        scenario: &ExactScenario,
        choice: &TaskPublicChoiceV1,
    ) -> Result<Vec<WeightedScenario<ExactScenario>>, String> {
        let mut next = scenario.clone();
        next.boundary = match next.world.explore_choice(choice)? {
            TaskWorldExplorationV1::Decision(decision) => PublicBoundary::Decision(decision),
            TaskWorldExplorationV1::ExploredScopeExit(
                sts_combat_outcome::agent::task::TaskExploredScopeExitV1::GameTerminal {
                    game_outcome,
                },
            ) => PublicBoundary::Terminal(game_outcome),
            other => return Err(format!("unexpected exact diagnostic boundary: {other:?}")),
        };
        Ok(vec![WeightedScenario {
            scenario: next,
            mass: 1.0,
        }])
    }
}

fn advance(scenario: &mut ExactScenario, choice: TaskPublicChoiceV1) {
    *scenario = ExactDiagnosticModel
        .step(scenario, &choice)
        .expect("real Rust transition")
        .remove(0)
        .scenario;
}

fn decision(scenario: &ExactScenario) -> &TaskPublicDecisionV1 {
    let PublicBoundary::Decision(decision) = &scenario.boundary else {
        panic!("expected a public decision")
    };
    decision
}

fn first_atomic_choice(decision: &TaskPublicDecisionV1) -> TaskPublicChoiceV1 {
    for (ordinal, candidate) in decision.candidates.iter().enumerate() {
        if let TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::BeginRunCardSelection {
                    min_choices,
                    selectable_card_ordinals,
                    ..
                },
        } = candidate
        {
            if selectable_card_ordinals.len() >= *min_choices {
                let mut ordinal_path = vec![ordinal];
                ordinal_path.extend(0..*min_choices);
                return TaskPublicChoiceV1 { ordinal_path };
            }
        } else if !matches!(
            candidate,
            TaskPublicCandidateV1::CombatSelectionFamily { .. }
        ) {
            return TaskPublicChoiceV1::one(ordinal);
        }
    }
    panic!("no usable public choice")
}

fn natural_near_victory() -> (ExactScenario, TaskPublicChoiceV1) {
    let world = ExactBranchingTaskWorldV1::new_ironclad_a0(1).expect("natural Rust run");
    let boundary = PublicBoundary::Decision(world.initial_scope().expect("root").public_root);
    let mut scenario = ExactScenario { world, boundary };
    for _ in 0..16 {
        if decision(&scenario).site == TaskDecisionSiteV1::Map {
            break;
        }
        let choice = first_atomic_choice(decision(&scenario));
        advance(&mut scenario, choice);
    }
    assert_eq!(decision(&scenario).site, TaskDecisionSiteV1::Map);
    advance(&mut scenario, TaskPublicChoiceV1::one(0));
    assert_eq!(decision(&scenario).site, TaskDecisionSiteV1::Combat);

    for _ in 0..80 {
        let current = decision(&scenario);
        assert_eq!(current.site, TaskDecisionSiteV1::Combat);
        for (ordinal, candidate) in current.candidates.iter().enumerate() {
            if !matches!(
                candidate,
                TaskPublicCandidateV1::CombatAtomic {
                    action: PublicCombatAtomicActionV1::PlayCard { .. }
                }
            ) {
                continue;
            }
            let choice = TaskPublicChoiceV1::one(ordinal);
            let successor = ExactDiagnosticModel
                .step(&scenario, &choice)
                .expect("probe exact transition")
                .remove(0)
                .scenario;
            if matches!(
                successor.boundary,
                PublicBoundary::Decision(ref next) if next.site != TaskDecisionSiteV1::Combat
            ) {
                return (scenario, choice);
            }
        }
        let TaskPublicObservationV1::Combat { observation, .. } = &current.observation else {
            unreachable!()
        };
        let choice = current
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(ordinal, candidate)| {
                let TaskPublicCandidateV1::CombatAtomic {
                    action:
                        PublicCombatAtomicActionV1::PlayCard {
                            hand_index,
                            target_monster_index,
                        },
                } = candidate
                else {
                    return None;
                };
                let card = observation.cards.hand.cards.get(*hand_index)?;
                let damage = target_monster_index
                    .and_then(|target| card.damage_by_monster_order.get(target).copied())
                    .unwrap_or(card.current_damage);
                (damage > 0).then_some((damage, ordinal))
            })
            .max_by_key(|(damage, _)| *damage)
            .map(|(_, ordinal)| TaskPublicChoiceV1::one(ordinal))
            .or_else(|| {
                current
                    .candidates
                    .iter()
                    .position(|candidate| {
                        matches!(
                            candidate,
                            TaskPublicCandidateV1::CombatAtomic {
                                action: PublicCombatAtomicActionV1::EndTurn
                            }
                        )
                    })
                    .map(TaskPublicChoiceV1::one)
            })
            .expect("combat can progress");
        advance(&mut scenario, choice);
    }
    panic!("natural first combat did not reach a lethal public choice")
}

fn visible_site_value(
    _history: &[PublicPolicyHistoryStep],
    boundary: &PublicBoundary,
) -> Option<f64> {
    Some(match boundary {
        PublicBoundary::Decision(decision) if decision.site == TaskDecisionSiteV1::Map => 3.0,
        PublicBoundary::Decision(decision) if decision.site != TaskDecisionSiteV1::Combat => 2.0,
        PublicBoundary::Decision(_) => 0.0,
        PublicBoundary::Terminal(_) => -1.0,
    })
}

#[test]
fn exact_diagnostic_crosses_natural_combat_into_real_reward_and_map() {
    let (mut scenario, lethal) = natural_near_victory();
    let root = decision(&scenario).clone();
    let outcome = search(
        &ExactDiagnosticModel,
        ScenarioSet {
            basis: ScenarioBasis::PrivilegedExactDiagnostic,
            source: "Rust seed 1, diagnostic greedy prefix and exact lethal probe".into(),
            samples: vec![WeightedScenario {
                scenario: scenario.clone(),
                mass: 1.0,
            }],
        },
        &[],
        SearchBudget {
            max_decisions: 2,
            max_transitions: 1_000,
        },
        &visible_site_value,
    );
    let SearchOutcome::Complete(report) = outcome else {
        panic!("real combat diagnostic did not complete: {outcome:?}");
    };
    assert_eq!(
        report.scenario_basis,
        ScenarioBasis::PrivilegedExactDiagnostic
    );
    assert_eq!(report.alternatives.len(), root.candidates.len());
    assert_eq!(report.policy.choice, Some(lethal.clone()));
    assert!(matches!(
        report.policy.branches[0].continuation.boundary,
        PublicBoundary::Decision(ref next) if next.site != TaskDecisionSiteV1::Combat
    ));
    assert!(report.policy.branches[0].continuation.choice.is_some());

    advance(&mut scenario, lethal);
    assert_ne!(decision(&scenario).site, TaskDecisionSiteV1::Combat);
    let mut saw_reward = false;
    let mut saw_map_successor_in_search = false;
    for _ in 0..16 {
        let current = decision(&scenario);
        if current.site == TaskDecisionSiteV1::Map {
            break;
        }
        assert!(matches!(
            current.site,
            TaskDecisionSiteV1::Reward | TaskDecisionSiteV1::CardReward
        ));
        saw_reward = true;
        let next = search(
            &ExactDiagnosticModel,
            ScenarioSet {
                basis: ScenarioBasis::PrivilegedExactDiagnostic,
                source: "natural Rust seed 1, post-combat exact diagnostic".into(),
                samples: vec![WeightedScenario {
                    scenario: scenario.clone(),
                    mass: 1.0,
                }],
            },
            &[],
            SearchBudget {
                max_decisions: 1,
                max_transitions: 100,
            },
            &visible_site_value,
        );
        let SearchOutcome::Complete(report) = next else {
            panic!("real reward diagnostic did not complete: {next:?}");
        };
        saw_map_successor_in_search |= report.policy.branches.iter().any(|branch| {
            matches!(
                branch.observed,
                PublicBoundary::Decision(ref next) if next.site == TaskDecisionSiteV1::Map
            )
        });
        let choice = current
            .candidates
            .iter()
            .enumerate()
            .find_map(|(ordinal, candidate)| {
                matches!(
                    candidate,
                    TaskPublicCandidateV1::Strategic {
                        action: TaskStrategicActionV1::Proceed { .. }
                            | TaskStrategicActionV1::SkipCardReward { .. }
                    }
                )
                .then_some(TaskPublicChoiceV1::one(ordinal))
            })
            .unwrap_or_else(|| first_atomic_choice(current));
        advance(&mut scenario, choice);
    }
    assert!(saw_reward);
    assert!(saw_map_successor_in_search);
    assert_eq!(decision(&scenario).site, TaskDecisionSiteV1::Map);
}

#[derive(Clone)]
struct ConstructedDrawScenario {
    position: CombatPosition,
    boundary: PublicBoundary,
}

struct ConstructedDrawModel;

fn constructed_draw_boundary(position: &CombatPosition) -> Result<PublicBoundary, String> {
    match EngineCombatStepper.terminal(position) {
        CombatTerminal::Win | CombatTerminal::Loss => {
            Err("constructed draw fixture unexpectedly reached combat terminal".into())
        }
        CombatTerminal::Unresolved => {
            let projection = project_public_combat_actions_v1(&position.engine, &position.combat)
                .map_err(|error| error.to_string())?;
            Ok(PublicBoundary::Decision(task_public_combat_decision_v1(
                PublicCombatRunContextV1::Unavailable {
                    reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
                },
                public_combat_state_v1(&position.combat),
                &projection.public,
            )?))
        }
    }
}

impl ScenarioModel for ConstructedDrawModel {
    type Scenario = ConstructedDrawScenario;

    fn observe(&self, scenario: &Self::Scenario) -> Result<PublicBoundary, String> {
        Ok(scenario.boundary.clone())
    }

    fn choices(
        &self,
        _history: &[PublicPolicyHistoryStep],
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskPublicChoiceV1>, String> {
        decision
            .candidates
            .iter()
            .enumerate()
            .map(|(ordinal, candidate)| match candidate {
                TaskPublicCandidateV1::CombatAtomic { .. } => Ok(TaskPublicChoiceV1::one(ordinal)),
                other => Err(format!(
                    "constructed draw fixture has unsupported choice: {other:?}"
                )),
            })
            .collect()
    }

    fn step(
        &self,
        scenario: &Self::Scenario,
        choice: &TaskPublicChoiceV1,
    ) -> Result<Vec<WeightedScenario<Self::Scenario>>, String> {
        let projection =
            project_public_combat_actions_v1(&scenario.position.engine, &scenario.position.combat)
                .map_err(|error| error.to_string())?;
        let action_ordinal = *choice
            .ordinal_path
            .first()
            .ok_or("constructed combat choice has no action ordinal")?;
        let input = resolve_public_combat_action_v1(
            &projection,
            &PublicCombatActionChoiceV1::Atomic { action_ordinal },
        )
        .map_err(|error| error.to_string())?;
        let result = EngineCombatStepper.apply_to_stable(
            &scenario.position,
            input,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        if result.truncated || result.timed_out {
            return Err("constructed combat transition did not settle".into());
        }
        let boundary = constructed_draw_boundary(&result.position)?;
        Ok(vec![WeightedScenario {
            scenario: ConstructedDrawScenario {
                position: result.position,
                boundary,
            },
            mass: 1.0,
        }])
    }
}

fn constructed_draw_scenarios() -> Vec<WeightedScenario<ConstructedDrawScenario>> {
    let mut position = crate::local::ConstructedCombat {
        seed: 73,
        encounter: EncounterId::JawWorm,
        act: 1,
        room: RoomType::MonsterRoom,
        hp: 40,
        deck: vec![
            (CardId::FlashOfSteel, 0),
            (CardId::Strike, 0),
            (CardId::Burn, 0),
        ],
        relics: vec![],
        potions: vec![],
        max_potion_uses: None,
    }
    .build()
    .expect("constructed real combat");
    position.combat.zones.hand = vec![CombatCard::for_scenario(CardId::FlashOfSteel, 1)];
    position.combat.zones.draw_pile = [CardId::Strike, CardId::Burn]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32 + 2))
        .collect();
    position.combat.zones.discard_pile.clear();
    position.combat.entities.monsters[0].current_hp = 20;
    let mut other = position.clone();
    other.combat.zones.draw_pile.reverse();
    [position, other]
        .into_iter()
        .map(|position| WeightedScenario {
            scenario: ConstructedDrawScenario {
                boundary: constructed_draw_boundary(&position).expect("public combat boundary"),
                position,
            },
            mass: 0.5,
        })
        .collect()
}

fn constructed_draw_value(
    _history: &[PublicPolicyHistoryStep],
    boundary: &PublicBoundary,
) -> Option<f64> {
    match boundary {
        PublicBoundary::Terminal(_) => None,
        PublicBoundary::Decision(decision) => {
            let TaskPublicObservationV1::Combat { observation, .. } = &decision.observation else {
                return None;
            };
            Some(
                observation.player.hp as f64 * 100.0 + observation.player.block as f64 * 2.0
                    - observation
                        .monsters
                        .iter()
                        .map(|monster| monster.hp.max(0) as f64 * 2.0)
                        .sum::<f64>(),
            )
        }
    }
}

#[test]
fn constructed_real_draw_uses_one_pre_draw_choice_then_publicly_diverges() {
    let samples = constructed_draw_scenarios();
    assert_eq!(samples[0].scenario.boundary, samples[1].scenario.boundary);
    let outcome = search(
        &ConstructedDrawModel,
        ScenarioSet {
            basis: ScenarioBasis::Constructed,
            source: "two constructed real Rust combats with opposite hidden draw order".into(),
            samples,
        },
        &[],
        SearchBudget {
            max_decisions: 2,
            max_transitions: 100,
        },
        &constructed_draw_value,
    );
    let SearchOutcome::Complete(report) = outcome else {
        panic!("constructed draw search did not complete: {outcome:?}");
    };
    assert_eq!(report.scenario_basis, ScenarioBasis::Constructed);
    let PublicBoundary::Decision(root) = &report.policy.boundary else {
        panic!("expected combat root");
    };
    assert!(matches!(
        root.candidates[report.policy.choice.as_ref().unwrap().ordinal_path[0]],
        TaskPublicCandidateV1::CombatAtomic {
            action: PublicCombatAtomicActionV1::PlayCard { hand_index: 0, .. }
        }
    ));
    assert_eq!(report.policy.branches.len(), 2);
    for branch in &report.policy.branches {
        assert!((branch.scenario_mass - 0.5).abs() < 1e-9);
        let PublicBoundary::Decision(next) = &branch.continuation.boundary else {
            panic!("expected observed draw");
        };
        let TaskPublicObservationV1::Combat { observation, .. } = &next.observation else {
            panic!("expected combat observation");
        };
        let drawn = observation.cards.hand.cards[0].card_id;
        let chosen = &next.candidates[branch.continuation.choice.as_ref().unwrap().ordinal_path[0]];
        match drawn {
            CardId::Strike => assert!(matches!(
                chosen,
                TaskPublicCandidateV1::CombatAtomic {
                    action: PublicCombatAtomicActionV1::PlayCard { hand_index: 0, .. }
                }
            )),
            CardId::Burn => assert!(matches!(
                chosen,
                TaskPublicCandidateV1::CombatAtomic {
                    action: PublicCombatAtomicActionV1::EndTurn
                }
            )),
            other => panic!("unexpected drawn card: {other:?}"),
        }
    }

    let mut reversed_samples = constructed_draw_scenarios();
    reversed_samples.reverse();
    let reversed = search(
        &ConstructedDrawModel,
        ScenarioSet {
            basis: ScenarioBasis::Constructed,
            source: "same constructed combats in reverse sample order".into(),
            samples: reversed_samples,
        },
        &[],
        SearchBudget {
            max_decisions: 2,
            max_transitions: 100,
        },
        &constructed_draw_value,
    );
    let SearchOutcome::Complete(reversed) = reversed else {
        panic!("reversed draw search did not complete: {reversed:?}");
    };
    assert_eq!(reversed.policy.choice, report.policy.choice);
    assert_eq!(reversed.policy.value, report.policy.value);
}
