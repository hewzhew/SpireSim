use super::*;
use sts_core::content::{cards::CardId, monsters::factory::EncounterId};
use sts_core::state::map::node::RoomType;

fn spec() -> ConstructedCombat {
    ConstructedCombat {
        seed: 97,
        encounter: EncounterId::JawWorm,
        act: 1,
        room: RoomType::MonsterRoom,
        hp: 60,
        deck: vec![(CardId::Strike, 0); 10],
        relics: vec![],
        potions: vec![],
        max_potion_uses: Some(0),
    }
}

#[test]
fn demonstration_restarts_at_a_student_pending_choice_and_rejects_partial_assembly() {
    for card in [CardId::Armaments, CardId::TrueGrit] {
        let mut s = spec();
        s.deck = vec![
            (card, u8::from(card == CardId::TrueGrit)),
            (CardId::SearingBlow, 10),
            (CardId::Strike, 0),
        ];
        let space = CombatSearchSpace::with_potion_limit(0);
        let state = CombatSearchState::new(s.build().unwrap()).unwrap();
        let node = Node::new(&space, state).unwrap();
        let p = node.encode(&space).unwrap();
        let play = p
            .action_ids
            .iter()
            .enumerate()
            .find(|(i, id)| {
                id[0] == 1
                    && p.ids[p.action_refs[*i][0] as usize][1] == card.stable_ordinal() as i64
            })
            .unwrap()
            .0;
        let Transition::Settled { state, .. } = node.choices.transition(play, limits()).unwrap()
        else {
            panic!()
        };
        let pending = Node::new(&space, state).unwrap();
        let expected = pending.encode(&space).unwrap();
        let (positions, choices, value) = demonstrate(&s, 6000, &[play]).unwrap().unwrap();
        assert_eq!(positions[0], expected);
        assert!(positions
            .iter()
            .any(|p| p.action_ids.iter().any(|a| a[0] == 6)));
        assert_eq!(positions.len(), choices.len());
        assert_eq!(value, 0.94);
        let begin = expected.action_ids.iter().position(|a| a[0] == 5).unwrap();
        assert!(demonstrate(&s, 6000, &[play, begin])
            .unwrap_err()
            .contains("unfinished virtual"));
        assert!(demonstrate(&s, 6000, &[usize::MAX]).is_err());
    }
}

#[test]
fn demonstration_prefix_keeps_spent_potion_allowance() {
    use sts_core::content::potions::PotionId;
    let mut s = spec();
    s.deck = vec![(CardId::SearingBlow, 10)];
    s.potions = vec![PotionId::BlockPotion, PotionId::BlockPotion];
    s.max_potion_uses = Some(1);
    let space = CombatSearchSpace::with_potion_limit(1);
    let node = Node::new(&space, CombatSearchState::new(s.build().unwrap()).unwrap()).unwrap();
    let p = node.encode(&space).unwrap();
    let drink = p.action_ids.iter().position(|id| id[0] == 3).unwrap();
    let (positions, _, value) = demonstrate(&s, 256, &[drink]).unwrap().unwrap();
    assert!(positions
        .iter()
        .all(|p| p.action_ids.iter().all(|id| id[0] != 3)));
    assert_eq!(value, 0.94);
}

fn drive(batch: &mut PolicySearchBatch) {
    loop {
        let f = batch.evaluations().unwrap();
        if f.slots.is_empty() {
            break;
        }
        let weights = f
            .positions
            .iter()
            .map(|p| vec![1.; p.actions.len()])
            .collect::<Vec<_>>();
        batch
            .evaluate(f.revision, &weights, &vec![0.5; f.slots.len()])
            .unwrap();
    }
}

#[test]
fn terminal_edges_keep_receiving_visits_and_real_action_advances() {
    let mut batch = PolicySearchBatch::new(&[spec()], 32, 100).unwrap();
    let mut root = spec().build().unwrap();
    root.combat.entities.monsters[0].current_hp = 1;
    batch.episodes[0].tree = Tree::new(
        CombatSearchSpace::with_potion_limit(0),
        CombatSearchState::new(root).unwrap(),
    )
    .unwrap();
    drive(&mut batch);
    let d = batch.decisions().unwrap();
    assert_eq!(d.visits[0].iter().sum::<u32>(), 32);
    let choice = d.visits[0]
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| **v)
        .unwrap()
        .0;
    assert!(
        d.visits[0][choice] > 1,
        "winning terminal cannot close after one backup"
    );
    assert_eq!(d.frame.positions[0].action_ids[choice][0], 1);
    let value = d.terminal_values[0][choice].unwrap();
    assert!(value > 0.5);
    assert_eq!(d.terminal_visits[0][choice], d.visits[0][choice]);
    assert!((d.qvalues[0][choice].unwrap() - value).abs() < 1e-12);
    assert!((d.priors[0].iter().sum::<f64>() - 1.).abs() < 1e-12);
    assert_eq!(d.root_values, vec![0.5]);
    assert_eq!(
        d.regularization[0],
        PUCT_EXPLORATION * 33_f64.sqrt() / (32 + d.visits[0].len()) as f64
    );
    batch.advance(d.frame.revision, &[choice]).unwrap();
    let outcome = &batch.outcomes()[0];
    assert_eq!(outcome.kind, "win");
    assert!(outcome.value.unwrap() > 0.5);
    assert_eq!(batch.episodes[0].inputs.len(), 1);
    assert_eq!(batch.episodes[0].decisions, 1);
}

#[test]
fn model_prior_changes_finite_budget_allocation_and_batches_validate_atomically() {
    let mut batch = PolicySearchBatch::new(&[spec(), spec()], 1, 1).unwrap();
    let f = batch.evaluations().unwrap();
    let n = f.positions[0].actions.len();
    let mut a = vec![0.; n];
    a[0] = 1.;
    let mut b = vec![0.; n];
    b[n - 1] = 1.;
    assert!(batch
        .evaluate(f.revision, &[a.clone(), vec![]], &[0.5, 0.5])
        .is_err());
    assert!(batch
        .episodes
        .iter()
        .all(|ep| ep.tree.nodes[0].prediction.is_none()));
    batch.evaluate(f.revision, &[a, b], &[0.5, 0.5]).unwrap();
    drive(&mut batch);
    let d = batch.decisions().unwrap();
    assert_eq!(d.visits[0][0], 1);
    assert_eq!(d.visits[1][n - 1], 1);
    assert_eq!(d.qvalues[0][0], Some(0.5));
    assert_eq!(d.qvalues[0][n - 1], None);
    assert!(d.terminal_visits[0].iter().all(|&n| n == 0));
    assert!(d.terminal_values[0].iter().all(Option::is_none));
    assert!(batch.advance(f.revision, &[0, n - 1]).is_err());
    batch.advance(d.frame.revision, &[0, n - 1]).unwrap();
    assert_eq!(batch.outcomes()[0].kind, "truncated");
    assert_eq!(
        batch.outcomes()[0].value,
        None,
        "decision cap is not a death"
    );
}

#[test]
fn upgrade_and_exhaust_choices_are_executed_through_the_same_actor() {
    let mut s = spec();
    s.deck = vec![
        (CardId::Armaments, 0),
        (CardId::TrueGrit, 1),
        (CardId::Bash, 0),
        (CardId::Strike, 0),
        (CardId::Defend, 0),
    ];
    let mut batch = PolicySearchBatch::new(&[s], 1, 100).unwrap();
    let mut choose = |kind: i64, card: Option<CardId>| {
        let f = batch.evaluations().unwrap();
        let p = &f.positions[0];
        let choice = p
            .action_ids
            .iter()
            .enumerate()
            .find(|(i, id)| {
                if id[0] != kind {
                    return false;
                }
                card.is_none_or(|card| {
                    let mut row = p.action_refs[*i][0] as usize;
                    if kind == 6 {
                        row = p.refs[row][1] as usize;
                    }
                    p.ids[row] == [2, card.stable_ordinal() as i64]
                })
            })
            .unwrap()
            .0;
        let mut weights = vec![0.; p.actions.len()];
        weights[choice] = 1.;
        batch.evaluate(f.revision, &[weights], &[0.5]).unwrap();
        drive(&mut batch);
        let d = batch.decisions().unwrap();
        assert_eq!(d.visits[0][choice], 1);
        batch.advance(d.frame.revision, &[choice]).unwrap();
    };
    choose(1, Some(CardId::Armaments));
    choose(5, None);
    choose(6, Some(CardId::Bash));
    choose(7, None);
    choose(1, Some(CardId::TrueGrit));
    choose(5, None);
    choose(6, Some(CardId::Strike));
    choose(7, None);
    let ep = &batch.episodes[0];
    let zones = &ep.tree.nodes[0].state.position().combat.zones;
    assert!(zones
        .hand
        .iter()
        .any(|c| c.id == CardId::Bash && c.upgrades == 1));
    assert!(zones.exhaust_pile.iter().any(|c| c.id == CardId::Strike));
    assert_eq!(ep.inputs.len(), 4); // two plays and two completed selections
    assert_eq!(ep.decisions, 8); // virtual decisions are retained for learning
}

#[test]
fn leaf_values_change_allocation_with_identical_policy_priors() {
    let mut visits = Vec::new();
    for favored in [0, 1] {
        let mut batch = PolicySearchBatch::new(&[spec()], 16, 100).unwrap();
        loop {
            let f = batch.evaluations().unwrap();
            if f.slots.is_empty() {
                break;
            }
            let path = &batch.episodes[0].tree.pending.as_ref().unwrap().1;
            let value = path
                .first()
                .map_or(0.5, |(_, edge)| if *edge == favored { 0.9 } else { 0.1 });
            batch
                .evaluate(
                    f.revision,
                    &[vec![1.; f.positions[0].actions.len()]],
                    &[value],
                )
                .unwrap();
        }
        let d = batch.decisions().unwrap();
        visits.push(d.visits[0].clone());
    }
    assert!(visits[0][0] > visits[1][0]);
    assert!(visits[1][1] > visits[0][1]);
}

#[test]
fn queried_leaf_continuations_produce_real_returns_without_advancing_parent() {
    let mut s = spec();
    s.deck = vec![(CardId::SearingBlow, 10)];
    let mut parent = PolicySearchBatch::new(&[s], 16, 100).unwrap();
    let f = parent.evaluations().unwrap();
    assert!(parent.leaf_rollouts(f.revision, &[0, 0], 1, 100).is_err());
    let mut rollout = parent.leaf_rollouts(f.revision, &[0], 1, 100).unwrap();
    let rf = rollout.evaluations().unwrap();
    assert_eq!(rf.positions[0], f.positions[0]);
    assert_ne!(rf.revision, f.revision);
    let play = rf.positions[0]
        .action_ids
        .iter()
        .position(|id| id[0] == 1)
        .unwrap();
    assert!(rollout.advance(f.revision, &[play]).is_err());
    let mut weights = vec![0.; rf.positions[0].actions.len()];
    weights[play] = 1.;
    rollout.evaluate(rf.revision, &[weights], &[0.5]).unwrap();
    drive(&mut rollout);
    let d = rollout.decisions().unwrap();
    rollout.advance(d.frame.revision, &[play]).unwrap();
    assert!(rollout.evaluations().unwrap().slots.is_empty());
    let outcomes = rollout.outcomes();
    assert_eq!(outcomes[0].kind, "win");
    assert_eq!(outcomes[0].hp, 66);
    assert_eq!(outcomes[0].value, Some(0.94));
    assert_eq!(outcomes[0].search_transitions, 1);
    let unchanged = parent.evaluations().unwrap();
    assert_eq!(unchanged.revision, f.revision);
    assert_eq!(unchanged.positions, f.positions);
    assert_eq!(parent.outcomes()[0].decisions, 0);
    parent
        .evaluate(
            f.revision,
            &[vec![1.; f.positions[0].actions.len()]],
            &[0.5],
        )
        .unwrap();
    assert!(parent.leaf_rollouts(f.revision, &[0], 1, 100).is_err());
}

#[test]
fn continuation_cap_is_not_a_terminal_failure() {
    let mut parent = PolicySearchBatch::new(&[spec()], 4, 100).unwrap();
    let f = parent.evaluations().unwrap();
    let mut rollout = parent.leaf_rollouts(f.revision, &[0], 1, 1).unwrap();
    let rf = rollout.evaluations().unwrap();
    let end = rf.positions[0]
        .action_ids
        .iter()
        .position(|id| id[0] == 2)
        .unwrap();
    let mut weights = vec![0.; rf.positions[0].actions.len()];
    weights[end] = 1.;
    rollout.evaluate(rf.revision, &[weights], &[0.5]).unwrap();
    drive(&mut rollout);
    let d = rollout.decisions().unwrap();
    rollout.advance(d.frame.revision, &[end]).unwrap();
    assert!(rollout.evaluations().unwrap().slots.is_empty());
    assert_eq!(rollout.outcomes()[0].kind, "truncated");
    assert!(rollout.outcomes()[0].value.is_none());
}

#[test]
fn leaf_source_reconstructs_an_unfinished_exhaust_selection() {
    let mut s = spec();
    s.deck = vec![
        (CardId::TrueGrit, 1),
        (CardId::Strike, 0),
        (CardId::Defend, 0),
    ];
    let mut parent = PolicySearchBatch::new(&[s], 1, 100).unwrap();
    for kind in [1, 5] {
        let f = parent.evaluations().unwrap();
        let p = &f.positions[0];
        let choice = p
            .action_ids
            .iter()
            .enumerate()
            .find(|(i, id)| {
                id[0] == kind
                    && (kind != 1
                        || p.ids[p.action_refs[*i][0] as usize][1]
                            == CardId::TrueGrit.stable_ordinal() as i64)
            })
            .unwrap()
            .0;
        let mut weights = vec![0.; p.actions.len()];
        weights[choice] = 1.;
        parent.evaluate(f.revision, &[weights], &[0.5]).unwrap();
        drive(&mut parent);
        let d = parent.decisions().unwrap();
        parent.advance(d.frame.revision, &[choice]).unwrap();
    }
    let f = parent.evaluations().unwrap();
    parent
        .evaluate(
            f.revision,
            &[vec![1.; f.positions[0].actions.len()]],
            &[0.5],
        )
        .unwrap();
    let leaf = parent.evaluations().unwrap();
    let mut fork = parent.leaf_rollouts(leaf.revision, &[0], 1, 100).unwrap();
    let evidence = fork.evidence();
    let source = &evidence["sources"][0];
    let s: ConstructedCombat = serde_json::from_value(source["specification"].clone()).unwrap();
    let inputs: Vec<ClientInput> = serde_json::from_value(source["parent_inputs"].clone()).unwrap();
    assert_eq!(inputs.len(), 1);
    let selection: Option<(usize, Vec<usize>)> =
        serde_json::from_value(source["root_selection"].clone()).unwrap();
    assert!(selection.is_some());
    let mut position = s.build().unwrap();
    use sts_core::sim::combat::{CombatStepper, EngineCombatStepper};
    for input in inputs {
        position = EngineCombatStepper
            .apply_to_stable(&position, input, limits())
            .position;
    }
    let space = CombatSearchSpace::with_potion_limit(0);
    let mut state = CombatSearchState::new(position).unwrap();
    let (family, members) = selection.unwrap();
    let mut path = vec![crate::CombatBranch::BeginSelection(family)];
    path.extend(members.into_iter().map(crate::CombatBranch::SelectMember));
    let search: Vec<crate::CombatBranch> =
        serde_json::from_value(source["search_branches"].clone()).unwrap();
    assert!(!search.is_empty());
    path.extend(search);
    for branch in path {
        state = match space.transition(&state, &branch, limits()).unwrap() {
            Transition::Settled { state, .. } => state,
            _ => panic!("source reconstruction interrupted"),
        };
    }
    let reconstructed = Node::new(&space, state).unwrap().encode(&space).unwrap();
    assert_eq!(reconstructed, leaf.positions[0]);
    assert_eq!(fork.evaluations().unwrap().positions[0], leaf.positions[0]);
}

#[test]
fn leaf_continuation_search_can_reject_the_greedy_policy_action() {
    let mut s = spec();
    s.deck = vec![(CardId::SearingBlow, 10)];
    let mut parent = PolicySearchBatch::new(&[s], 32, 100).unwrap();
    let source = parent.evaluations().unwrap();
    let mut child = parent
        .leaf_rollouts(source.revision, &[0], 32, 100)
        .unwrap();
    loop {
        let f = child.evaluations().unwrap();
        if f.slots.is_empty() {
            break;
        }
        let weights = f
            .positions
            .iter()
            .map(|p| {
                p.action_ids
                    .iter()
                    .map(|id| if id[0] == 2 { 0.8 } else { 0.2 })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        child
            .evaluate(f.revision, &weights, &vec![0.; f.slots.len()])
            .unwrap();
    }
    let d = child.decisions().unwrap();
    let choice = d.visits[0]
        .iter()
        .enumerate()
        .max_by_key(|(_, n)| **n)
        .unwrap()
        .0;
    assert_eq!(d.frame.positions[0].action_ids[choice][0], 1);
    child.advance(d.frame.revision, &[choice]).unwrap();
    assert_eq!(child.outcomes()[0].kind, "win");
    assert_eq!(parent.outcomes()[0].decisions, 0);
}
