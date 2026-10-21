use super::*;
use crate::path_search::{admission::AdmittedChoices, GroupedNeutralPrior};
use crate::ResourceRequirement;
use sts_core::{
    content::{
        cards::CardId,
        monsters::EnemyId,
        potions::{Potion, PotionId},
    },
    runtime::combat::CombatCard,
    sim::combat::CombatPosition,
    state::core::EngineState,
};

fn root() -> CombatPosition {
    let mut c = sts_core::test_support::blank_test_combat();
    c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
    c.zones.hand = [CardId::Strike, CardId::Defend, CardId::Strike]
        .iter()
        .enumerate()
        .map(|(i, c)| CombatCard::for_scenario(*c, i as u32 + 1))
        .collect();
    c.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 11)), None];
    CombatPosition::new(EngineState::CombatPlayerTurn, c)
}
fn goal() -> ResourceRequirement {
    ResourceRequirement {
        min_final_hp: 20,
        preserve_starting_potion_slots: 0,
    }
}
fn weights() -> LinearWeights {
    LinearWeights {
        schema: SCHEMA,
        group: vec![0.0; FEATURE_COUNT],
        member: vec![0.0; FEATURE_COUNT],
    }
}

#[test]
fn zero_model_matches_neutral_over_all_concrete_admitted_choices() {
    let r = root();
    let s = CombatSearchState::new(r.clone()).unwrap();
    let g = goal();
    let context = PathContext {
        root: &r,
        goal: &g,
        spent_originals: 0,
    };
    let branches = AdmittedChoices::new(&r, &s, &g).unwrap().branches();
    let encoded = encode(&s, context, &branches).unwrap();
    let model = FrozenLinearPrior::new(weights()).unwrap();
    assert_eq!(
        model.score(&encoded).unwrap(),
        GroupedNeutralPrior
            .log_probabilities(&s, context, &branches)
            .unwrap()
    );
    assert_eq!(
        model.score(&encoded).unwrap(),
        model.log_probabilities(&s, context, &branches).unwrap()
    );
    let loaded: LinearInput =
        serde_json::from_slice(&serde_json::to_vec(&encoded).unwrap()).unwrap();
    assert_eq!(loaded.groups, encoded.groups);
    for (a, b) in loaded.rows.iter().zip(&encoded.rows) {
        assert_eq!(a.len(), b.len());
        for ((i, x), (j, y)) in a.iter().zip(b) {
            assert_eq!(i, j);
            assert!((x - y).abs() < 1e-14);
        }
    }
}

#[test]
fn encoder_sees_goal_and_original_bottle_relation_but_not_uuid() {
    let r = root();
    let s = CombatSearchState::new(r.clone()).unwrap();
    let g = goal();
    let branches = AdmittedChoices::new(&r, &s, &g).unwrap().branches();
    let f = |r: &CombatPosition, s: &CombatSearchState, g: &ResourceRequirement| {
        encode(
            s,
            PathContext {
                root: r,
                goal: g,
                spent_originals: 0,
            },
            &branches,
        )
        .unwrap()
    };
    let baseline = f(&r, &s, &g);
    let mut renamed = r.clone();
    renamed.combat.entities.potions[0].as_mut().unwrap().uuid = 99;
    let state = CombatSearchState::new(renamed.clone()).unwrap();
    assert_eq!(baseline, f(&renamed, &state, &g));
    // Same displayed potion, now a replacement: relation to original changes.
    assert_ne!(baseline, f(&r, &state, &g));
    let mut high = g.clone();
    high.min_final_hp = 60;
    assert_ne!(baseline, f(&r, &s, &high));
    high = g.clone();
    high.preserve_starting_potion_slots = 1;
    assert_ne!(baseline, f(&r, &s, &high));
}

#[test]
fn model_rejects_invalid_schema_weights_and_sparse_domains() {
    let mut bad = weights();
    bad.group[0] = f64::NAN;
    assert!(FrozenLinearPrior::new(bad).is_err());
    let mut bad = weights();
    bad.schema += 1;
    assert!(FrozenLinearPrior::new(bad).is_err());
    let model = FrozenLinearPrior::new(weights()).unwrap();
    for groups in [vec![], vec![1], vec![100]] {
        assert!(model
            .score(&LinearInput {
                schema: SCHEMA,
                groups,
                rows: vec![vec![(0, 1.0)]]
            })
            .is_err());
    }
}

#[test]
fn two_level_scores_match_explicit_group_means_and_member_probabilities() {
    let mut w = weights();
    w.group[0] = 2.0;
    w.member[0] = 1.0;
    let input = LinearInput {
        schema: SCHEMA,
        groups: vec![0, 0, 1],
        rows: vec![vec![(0, 1.0)], vec![(0, 3.0)], vec![(0, 0.0)]],
    };
    let logs = FrozenLinearPrior::new(w).unwrap().score(&input).unwrap();
    let group = log_softmax(&[4.0, 0.0]).unwrap();
    let member = log_softmax(&[1.0, 3.0]).unwrap();
    assert_eq!(
        logs,
        vec![group[0] + member[0], group[0] + member[1], group[1]]
    );
}

#[test]
fn awakened_form_and_first_turn_are_observed_not_defaulted() {
    let mut r = root();
    r.combat.entities.monsters = vec![sts_core::test_support::planned_monster(
        EnemyId::AwakenedOne,
        1,
    )];
    let s = CombatSearchState::new(r.clone()).unwrap();
    let g = goal();
    let b = AdmittedChoices::new(&r, &s, &g).unwrap().branches();
    let baseline = encode(
        &s,
        PathContext {
            root: &r,
            goal: &g,
            spent_originals: 0,
        },
        &b,
    )
    .unwrap();
    let mut second = r.clone();
    second.combat.entities.monsters[0].awakened_one.form1 = false;
    let s = CombatSearchState::new(second).unwrap();
    assert!(
        baseline
            != encode(
                &s,
                PathContext {
                    root: &r,
                    goal: &g,
                    spent_originals: 0
                },
                &b
            )
            .unwrap()
    );
    let mut missing = r.clone();
    missing.combat.entities.monsters[0]
        .awakened_one
        .protocol_seeded = false;
    let s = CombatSearchState::new(missing).unwrap();
    assert!(encode(
        &s,
        PathContext {
            root: &r,
            goal: &g,
            spent_originals: 0
        },
        &b
    )
    .is_err());
}

#[test]
fn frozen_learned_prior_keeps_queue_identical_across_budget_slices() {
    use crate::path_search::{PathBudget, PathSearch};
    let r = root();
    let g = goal();
    let mut w = weights();
    for i in 0..FEATURE_COUNT {
        w.group[i] = (i % 7) as f64 * 0.002;
        w.member[i] = (i % 5) as f64 * -0.003;
    }
    let mut whole = PathSearch::new(
        r.clone(),
        g.clone(),
        FrozenLinearPrior::new(w.clone()).unwrap(),
    )
    .unwrap();
    let mut sliced = PathSearch::new(r, g, FrozenLinearPrior::new(w).unwrap()).unwrap();
    let budget = |work| PathBudget {
        work,
        max_nodes: 1000,
        max_pending_edges: 5000,
        max_depth: 40,
        max_engine_steps_per_input: 512,
        deadline: None,
    };
    whole.advance(budget(200)).unwrap();
    for _ in 0..200 {
        sliced.advance(budget(1)).unwrap();
    }
    assert_eq!(whole.stats(), sliced.stats());
    assert_eq!(whole.pending_edges(), sliced.pending_edges());
    assert!(
        serde_json::to_value(whole.plans()).unwrap()
            == serde_json::to_value(sliced.plans()).unwrap()
    );
}
