use super::*;

fn evidence(s: &DistanceSearch, node: usize) -> NodeOutcomeEvidence {
    s.data_with_outcomes().rows[node]
        .outcome_evidence
        .clone()
        .unwrap()
}

fn reach_victory(s: &mut DistanceSearch) -> usize {
    for _ in 0..100 {
        service(s, 1, 0.);
        if let Some(node) = s
            .nodes
            .iter()
            .position(|n| EngineCombatStepper.is_exact_victory(n.state.position()))
        {
            return node;
        }
    }
    panic!("fixture did not reach its one-input victory");
}

#[test]
fn outcome_export_is_optional_read_only_and_distinguishes_pending_replay() {
    let mut s = search(root(&[CardId::Strike], 6));
    let first = evidence(&s, 0);
    assert_eq!(first.combat_terminal, None);
    assert_eq!(first.observed_terminal_outcome, None);
    assert!(!first.terminal_replay_verified);
    // Stop after the real transition, before the separately charged replay.
    let terminal = reach_victory(&mut s);
    let before = s.stats().clone();
    let pending = evidence(&s, terminal);
    assert_eq!(pending.combat_terminal, Some(true));
    assert_eq!(pending.terminal_satisfies_query, Some(true));
    assert!(pending.observed_terminal_outcome.is_some());
    assert!(!pending.terminal_replay_verified);
    assert!(s.plans().is_empty());
    assert_eq!(s.stats(), &before);

    assert_eq!(s.advance(budget(1)).unwrap(), DistanceStop::WorkBudget);
    let checked = evidence(&s, terminal);
    assert!(checked.terminal_replay_verified);
    assert_eq!(
        checked.observed_terminal_outcome.as_ref(),
        Some(&s.plans()[0].outcome)
    );
    // Known continuation at a source does not make that source a terminal.
    assert_eq!(s.data().rows[0].witnessed_remaining_inputs, Some(1));
    assert_eq!(evidence(&s, 0).combat_terminal, None);
    assert!(!evidence(&s, 0).terminal_replay_verified);

    let before = s.stats().clone();
    let plain = serde_json::to_value(s.data()).unwrap();
    let mut rich = serde_json::to_value(s.data_with_outcomes()).unwrap();
    for row in rich["rows"].as_array_mut().unwrap() {
        assert!(row
            .as_object_mut()
            .unwrap()
            .remove("outcome_evidence")
            .is_some());
    }
    assert_eq!(plain, rich);
    assert_eq!(s.stats(), &before);
}

#[test]
fn insufficient_hp_victory_is_not_death_or_an_open_state_failure_label() {
    let mut s = search(root(&[CardId::Strike], 6));
    s.goal.min_final_hp = s.root.combat.entities.player.max_hp + 1;
    let terminal = reach_victory(&mut s);
    let e = evidence(&s, terminal);
    assert_eq!(e.combat_terminal, Some(true));
    assert_eq!(e.terminal_satisfies_query, Some(false));
    assert!(!e.required_original_spent);
    assert!(!e.terminal_replay_verified);
    assert!(e.observed_terminal_outcome.unwrap().final_hp > 0);
    assert_eq!(evidence(&s, 0).terminal_satisfies_query, None);
}

#[test]
fn fairy_cutoff_is_distinct_from_defeat_and_does_not_fabricate_final_resources() {
    use sts_core::content::potions::{Potion, PotionId};
    let mut r = root(&[], 6);
    r.combat.entities.player.current_hp = 1;
    for fairy in [false, true] {
        let mut r = r.clone();
        if fairy {
            r.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
        }
        let mut s = DistanceSearch::new(
            r,
            ResourceRequirement {
                min_final_hp: 1,
                preserve_starting_potion_slots: u64::from(fairy),
            },
            8,
            "test-model".into(),
            16,
        )
        .unwrap();
        service(&mut s, 10, 0.);
        // EndTurn is the only admitted choice (Fairy cannot be actively used).
        s.advance(budget(1)).unwrap();
        let e = evidence(&s, 1);
        assert_eq!(e.required_original_spent, fairy);
        assert_eq!(e.spent_starting_potion_slots, u64::from(fairy));
        assert!(!e.terminal_replay_verified);
        assert!(!e.combat_smoked);
        if fairy {
            assert_eq!(e.combat_terminal, None);
            assert_eq!(e.terminal_satisfies_query, None);
            assert_eq!(e.observed_terminal_outcome, None);
        } else {
            assert_eq!(e.combat_terminal, Some(false));
            assert_eq!(e.terminal_satisfies_query, Some(false));
            assert!(e.observed_terminal_outcome.unwrap().final_hp <= 0);
        }
    }
}

#[test]
fn beyond_horizon_witness_preserves_terminal_resources_without_labelling_source() {
    let mut s = DistanceSearch::new(
        root(&[CardId::Strike, CardId::Strike, CardId::Strike], 18),
        ResourceRequirement {
            min_final_hp: 1,
            preserve_starting_potion_slots: 0,
        },
        1,
        "test-model".into(),
        16,
    )
    .unwrap();
    for _ in 0..100 {
        service(&mut s, 100, 2.);
        if !s.plans().is_empty() {
            break;
        }
    }
    assert_eq!(s.plans()[0].inputs.len(), 3);
    let d = s.data_with_outcomes();
    assert_eq!(d.rows[0].upper, d.horizon + 1);
    assert_eq!(d.rows[0].witnessed_remaining_inputs, Some(3));
    assert_eq!(evidence(&s, 0).combat_terminal, None);
    let checked: Vec<_> = d
        .rows
        .iter()
        .filter_map(|r| r.outcome_evidence.as_ref())
        .filter(|e| e.terminal_replay_verified)
        .collect();
    assert!(!checked.is_empty());
    assert!(checked
        .iter()
        .all(|e| e.observed_terminal_outcome.as_ref().unwrap().final_hp > 0));
}

#[test]
fn verified_but_dominated_routes_remain_terminal_evidence() {
    let mut s = search(root(&[CardId::Hemokinesis, CardId::Strike], 6));
    for _ in 0..100 {
        let stop = service(&mut s, 100, 0.);
        if matches!(
            stop,
            DistanceStop::NoOpenWork | DistanceStop::NodeLimit | DistanceStop::DepthLimit
        ) {
            break;
        }
    }
    assert!(!s.plans().is_empty());
    let data = s.data_with_outcomes();
    let checked: Vec<_> = data
        .rows
        .iter()
        .filter_map(|r| r.outcome_evidence.as_ref())
        .filter(|e| e.terminal_replay_verified)
        .collect();
    assert_eq!(checked.len(), s.stats().verified_wins);
    assert!(checked.len() > s.plans().len());
    assert!(checked.iter().any(|e| s.plans().iter().any(|p| p
        .outcome
        .strictly_dominates(e.observed_terminal_outcome.as_ref().unwrap()))));
}

#[test]
fn smoke_escape_is_visible_and_never_a_victory() {
    use sts_core::content::potions::{Potion, PotionId};
    let mut r = root(&[], 200);
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 1))];
    let mut s = search(r);
    for _ in 0..100 {
        service(&mut s, 1, 0.);
        let data = s.data_with_outcomes();
        if let Some(e) = data
            .rows
            .iter()
            .filter_map(|r| r.outcome_evidence.as_ref())
            .find(|e| e.combat_smoked)
        {
            assert_eq!(e.combat_terminal, Some(false));
            assert_eq!(e.terminal_satisfies_query, Some(false));
            assert!(!e.terminal_replay_verified);
            assert!(e.observed_terminal_outcome.as_ref().unwrap().final_hp > 0);
            return;
        }
    }
    panic!("fixture did not reach Smoke escape");
}
