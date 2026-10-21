use super::*;
use crate::path_search::{GroupedNeutralPrior, PathBudget, PathSearch};
use sts_core::content::{
    cards::CardId,
    monsters::EnemyId,
    potions::{Potion, PotionId},
};
use sts_core::runtime::combat::CombatCard;
use sts_core::sim::combat::{CombatStepper, EngineCombatStepper};
use sts_core::state::core::{EngineState, HandSelectReason, PendingChoice};
use sts_core::state::selection::{SelectionResolution, SelectionScope};

fn root(hand: &[CardId], enemy_hp: i32) -> CombatPosition {
    let mut c = sts_core::test_support::blank_test_combat();
    let mut m = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    m.current_hp = enemy_hp;
    m.max_hp = enemy_hp;
    c.entities.monsters = vec![m];
    c.zones.hand = hand
        .iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(*id, i as u32 + 1))
        .collect();
    CombatPosition::new(EngineState::CombatPlayerTurn, c)
}
fn goal(hp: i32, slots: u64) -> ResourceRequirement {
    ResourceRequirement {
        min_final_hp: hp,
        preserve_starting_potion_slots: slots,
    }
}
fn limits() -> RouteLimits {
    RouteLimits {
        max_decisions: 100,
        step: CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    }
}
fn play(index: usize) -> ClientInput {
    ClientInput::PlayCard {
        card_index: index,
        target: Some(1),
    }
}
fn finish(root: &CombatPosition, inputs: &[ClientInput]) -> CombatPosition {
    let mut p = root.clone();
    for input in inputs {
        assert!(EngineCombatStepper.is_legal_action(&p, input));
        let step = EngineCombatStepper.apply_to_stable(&p, input.clone(), limits().step);
        assert!(!step.truncated && !step.timed_out);
        p = step.position;
    }
    assert!(EngineCombatStepper.is_exact_victory(&p));
    p
}
fn selection(indices: &[u32]) -> ClientInput {
    ClientInput::SubmitSelection(SelectionResolution::card_lineages(
        SelectionScope::Hand,
        sts_core::test_support::scenario_lineages(indices.iter().copied()),
    ))
}

#[test]
fn ordered_selection_reconstructs_duplicate_instances_without_permutation_search() {
    let mut r = root(
        &[
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Strike,
        ],
        6,
    );
    r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2, 3, 4]),
        min_cards: 1,
        max_cards: 3,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let inputs = vec![selection(&[3, 1]), play(1)];
    let end = finish(&r, &inputs);
    let route = reconstruct_route(&r, &goal(1, 0), &inputs, &end, limits()).unwrap();
    assert_eq!(route.decisions.len(), 5);
    // Inspect the typed virtual selection prefix, not a shorter raw-input count.
    let mut inspected = PathSearch::with_order(
        r.clone(),
        goal(1, 0),
        GroupedNeutralPrior,
        crate::path_search::PathOrder::RelativePrior,
    )
    .unwrap();
    inspected
        .advance(PathBudget {
            work: 2000,
            max_nodes: 1000,
            max_pending_edges: 5000,
            max_depth: 3,
            max_engine_steps_per_input: 512,
            deadline: None,
        })
        .unwrap();
    let review = inspected.inspect_route(&route).unwrap();
    assert_eq!(review.reached_decisions, 3);
    assert_eq!(review.next_input_index, Some(0));
    assert_eq!(
        review.progress,
        crate::path_search::inspection::ReferenceProgress::DepthDeferred
    );
    let chosen: Vec<_> = route
        .decisions
        .iter()
        .map(|d| d.branches[d.selected].clone())
        .collect();
    assert!(matches!(chosen[0], CombatBranch::BeginSelection(_)));
    assert_eq!(
        &chosen[1..4],
        &[
            CombatBranch::SelectMember(2),
            CombatBranch::SelectMember(0),
            CombatBranch::FinishSelection
        ]
    );
    assert_eq!(
        route.decisions[3].state.selected_members(),
        Some([2, 0].as_slice())
    );
    assert_eq!(
        route.decisions[3]
            .observation
            .assembling
            .as_ref()
            .unwrap()
            .selected,
        [2, 0]
    );
    assert_eq!(route.decisions[1].choices.len(), 4);
    let first = AdmittedChoices::new(&r, &route.decisions[0].state, &goal(1, 0)).unwrap();
    for invalid in [
        selection(&[1, 1]),
        selection(&[99]),
        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Grid,
            sts_core::test_support::scenario_lineages([1]),
        )),
    ] {
        assert!(first.input_path(&invalid).is_err());
    }
    assert!(reconstruct_route(
        &r,
        &goal(1, 0),
        &inputs,
        &end,
        RouteLimits {
            max_decisions: 4,
            ..limits()
        }
    )
    .is_err());
}

#[test]
fn reconstructed_route_agrees_with_runtime_choices_and_path_probability() {
    let r = root(&[CardId::Strike, CardId::Defend, CardId::Strike], 12);
    let g = goal(1, 0);
    let mut search = PathSearch::new(r.clone(), g.clone(), GroupedNeutralPrior).unwrap();
    search
        .advance(PathBudget {
            work: 150,
            max_nodes: 1000,
            max_pending_edges: 5000,
            max_depth: 40,
            max_engine_steps_per_input: 512,
            deadline: None,
        })
        .unwrap();
    assert!(!search.plans().is_empty());
    for plan in search.plans() {
        let route = reconstruct_route(&r, &g, &plan.inputs, &plan.successor, limits()).unwrap();
        assert_eq!(route.outcome, plan.outcome);
        assert_eq!(route.decisions.len(), plan.steps.len());
        assert!(
            (route.log_probability(&GroupedNeutralPrior).unwrap()
                - plan.steps.iter().map(|s| s.log_probability).sum::<f64>())
            .abs()
                < 1e-10
        );
        for (decision, step) in route.decisions.iter().zip(&plan.steps) {
            assert_eq!(decision.branches[decision.selected], step.branch);
            assert_eq!(decision.branches.len(), decision.choices.len());
            assert_eq!(decision.groups.len(), decision.choices.len());
        }
        let d = &route.decisions[0];
        let strikes: Vec<_> = d
            .branches
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                matches!(
                    b,
                    CombatBranch::Input(ClientInput::PlayCard {
                        card_index: 0 | 2,
                        ..
                    })
                )
            })
            .collect();
        assert_eq!(strikes.len(), 2);
        assert_eq!(d.groups[strikes[0].0], d.groups[strikes[1].0]);
    }
}

#[test]
fn resource_admission_and_prefix_ledger_keep_original_bottles_distinct() {
    let mut r = root(&[CardId::Strike], 6);
    r.combat.entities.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 10)),
        Some(Potion::new(PotionId::FirePotion, 11)),
    ];
    let inputs = vec![ClientInput::DiscardPotion(1), play(0)];
    let end = finish(&r, &inputs);
    let route = reconstruct_route(&r, &goal(1, 1), &inputs, &end, limits()).unwrap();
    assert_eq!(route.decisions[0].original_potion_slots, [1, 2]);
    assert_eq!(route.decisions[1].original_potion_slots, [1, 0]);
    assert_eq!(route.decisions[1].spent_originals_before, 2);
    assert_eq!(
        route.decisions[1].expenditures_before,
        CombatInputExpendituresV1::from_inputs(&inputs[..1])
    );
    for d in &route.decisions {
        assert!(!d.branches.iter().any(|b| matches!(
            b,
            CombatBranch::Input(
                ClientInput::DiscardPotion(0)
                    | ClientInput::UsePotion {
                        potion_index: 0,
                        ..
                    }
            )
        )));
    }
    assert!(reconstruct_route(&r, &goal(1, 3), &inputs, &end, limits()).is_err());
}

#[test]
fn passive_use_rejected_but_intermediate_hp_below_goal_is_allowed() {
    let mut r = root(&[], 6);
    r.combat.entities.player.current_hp = 1;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    let inputs = vec![ClientInput::EndTurn, play(0)];
    let end = finish(&r, &inputs);
    let route = reconstruct_route(&r, &goal(1, 0), &inputs, &end, limits()).unwrap();
    assert_eq!(route.outcome.starting_potion_expenditure_slots, 1);
    assert_eq!(route.decisions[1].spent_originals_before, 1);
    assert!(reconstruct_route(&r, &goal(1, 1), &inputs, &end, limits()).is_err());

    let mut r = root(&[CardId::Reaper, CardId::Strike], 6);
    r.combat.entities.player.current_hp = 5;
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        play(0),
    ];
    let end = finish(&r, &inputs);
    assert!(reconstruct_route(&r, &goal(9, 0), &inputs, &end, limits()).is_ok());
    assert!(reconstruct_route(&r, &goal(75, 0), &inputs, &end, limits()).is_err());
    let observed = reconstruct_observed_victory(&r, &goal(75, 0), &inputs, &end, limits()).unwrap();
    assert!(!observed.goal.accepts(&observed.outcome));
}

#[test]
fn preferences_require_exact_roots_and_compare_specific_bottles_without_pricing() {
    let mut r = root(&[CardId::Strike], 6);
    r.combat.entities.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 10)),
        Some(Potion::new(PotionId::FirePotion, 11)),
    ];
    let g = goal(100, 0); // Neither route is qualified; relative resources still compare.
    let observed = |inputs: Vec<ClientInput>| {
        let end = finish(&r, &inputs);
        reconstruct_observed_victory(&r, &g, &inputs, &end, limits()).unwrap()
    };
    let retained = observed(vec![play(0)]);
    let first = observed(vec![ClientInput::DiscardPotion(0), play(0)]);
    let second = observed(vec![ClientInput::DiscardPotion(1), play(0)]);
    assert_eq!(
        resource_preference(&retained, &first).unwrap(),
        Some(std::cmp::Ordering::Greater)
    );
    assert_eq!(
        resource_preference(&first, &retained).unwrap(),
        Some(std::cmp::Ordering::Less)
    );
    assert_eq!(resource_preference(&first, &second).unwrap(), None);
    assert_eq!(resource_preference(&retained, &retained).unwrap(), None);
    let mut other = retained.clone();
    other.goal.min_final_hp += 1;
    assert!(resource_preference(&retained, &other).is_err());
    other = retained.clone();
    other.root.combat.entities.player.gold += 1;
    assert!(resource_preference(&retained, &other).is_err());
    assert!(reconstruct_observed_victory(
        &r,
        &goal(100, 1),
        &first.inputs,
        &first.successor,
        limits()
    )
    .is_err());
}

#[test]
fn observed_evidence_rejects_passive_spending_and_incomplete_or_drifted_routes() {
    let mut r = root(&[], 6);
    r.combat.entities.player.current_hp = 1;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    let inputs = vec![ClientInput::EndTurn, play(0)];
    let end = finish(&r, &inputs);
    assert!(reconstruct_observed_victory(&r, &goal(100, 1), &inputs, &end, limits()).is_err());
    assert!(reconstruct_observed_victory(&r, &goal(100, 0), &inputs[..1], &end, limits()).is_err());
    let mut drifted = end.clone();
    drifted.combat.entities.player.current_hp += 1;
    assert!(reconstruct_observed_victory(&r, &goal(100, 0), &inputs, &drifted, limits()).is_err());
}

#[test]
fn route_relative_cost_matches_each_runtime_decision() {
    use crate::path_search::linear::{FrozenLinearPrior, LinearWeights, FEATURE_COUNT, SCHEMA};
    let r = root(&[CardId::Strike, CardId::Strike], 12);
    let inputs = vec![play(0), play(0)];
    let end = finish(&r, &inputs);
    let route = reconstruct_route(&r, &goal(1, 0), &inputs, &end, limits()).unwrap();
    let prior = FrozenLinearPrior::new(LinearWeights {
        schema: SCHEMA,
        group: (0..FEATURE_COUNT).map(|i| (i % 7) as f64 * 0.001).collect(),
        member: (0..FEATURE_COUNT)
            .map(|i| (i % 5) as f64 * -0.002)
            .collect(),
    })
    .unwrap();
    let mut expected = 0.0;
    for d in &route.decisions {
        let logs = prior
            .log_probabilities(
                &d.state,
                PathContext {
                    root: &route.root,
                    goal: &route.goal,
                    spent_originals: d.spent_originals_before,
                },
                &d.branches,
            )
            .unwrap();
        expected += logs.iter().copied().fold(f64::NEG_INFINITY, f64::max) - logs[d.selected];
    }
    assert!((route.relative_prior_cost(&prior).unwrap() - expected).abs() < 1e-12);
}

#[test]
fn no_positive_examples_for_truncation_drift_illegality_or_interruption() {
    let r = root(&[CardId::Strike, CardId::Strike], 12);
    let inputs = vec![play(0), play(0)];
    let end = finish(&r, &inputs);
    let g = goal(1, 0);
    assert!(reconstruct_route(&r, &g, &inputs[..1], &end, limits()).is_err());
    assert!(reconstruct_route(&r, &g, &[play(99)], &end, limits()).is_err());
    let mut changed = end.clone();
    changed.combat.entities.player.current_hp -= 1;
    assert!(reconstruct_route(&r, &g, &inputs, &changed, limits()).is_err());
    assert!(reconstruct_route(
        &r,
        &g,
        &inputs,
        &end,
        RouteLimits {
            step: CombatStepLimits {
                max_engine_steps: 1,
                deadline: None
            },
            ..limits()
        }
    )
    .is_err());
    assert!(reconstruct_route(
        &r,
        &g,
        &inputs,
        &end,
        RouteLimits {
            step: CombatStepLimits {
                deadline: Some(Instant::now()),
                ..limits().step
            },
            ..limits()
        }
    )
    .is_err());
    let mut late = inputs.clone();
    late.push(ClientInput::EndTurn);
    assert!(reconstruct_route(&r, &g, &late, &end, limits()).is_err());
    assert!(reconstruct_route(&end, &g, &inputs, &end, limits()).is_err());
}

#[test]
fn scry_reconstruction_uses_current_domain_indices_and_preserves_empty_selection() {
    let mut r = root(&[CardId::Strike], 6);
    r.combat.zones.draw_pile = (101..105)
        .map(|i| CombatCard::for_scenario(CardId::Defend, i))
        .collect::<Vec<_>>()
        .into();
    r.engine = EngineState::PendingChoice(PendingChoice::ScrySelect {
        cards: vec![CardId::Defend; 4],
        card_lineages: sts_core::test_support::scenario_lineages(101..105),
    });
    for selected in [vec![3, 1], vec![]] {
        let inputs = vec![ClientInput::SubmitScryDiscard(selected.clone()), play(0)];
        let end = finish(&r, &inputs);
        let route = reconstruct_route(&r, &goal(1, 0), &inputs, &end, limits()).unwrap();
        let members: Vec<_> = route
            .decisions
            .iter()
            .filter_map(|d| match d.branches[d.selected] {
                CombatBranch::SelectMember(i) => Some(i),
                _ => None,
            })
            .collect();
        assert_eq!(members, selected);
        let original = AdmittedChoices::new(&r, &route.decisions[0].state, &goal(1, 0)).unwrap();
        assert!(original
            .input_path(&ClientInput::SubmitScryDiscard(vec![0, 0]))
            .is_err());
        assert!(original
            .input_path(&ClientInput::SubmitScryDiscard(vec![4]))
            .is_err());
    }
    // A stale scry source cannot be repaired by matching duplicate card names.
    r.combat.zones.draw_pile.pop();
    let state = CombatSearchState::new(r.clone()).unwrap();
    let original = AdmittedChoices::new(&r, &state, &goal(1, 0)).unwrap();
    assert!(original
        .input_path(&ClientInput::SubmitScryDiscard(vec![3]))
        .is_err());
}
