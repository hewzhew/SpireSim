use super::*;
use sts_oracle_learning::agent::task::TaskPublicChoiceV1;

fn commands(reading: &RunPlayDecisionReadingV1) -> (RunPlayBoundChoiceV1, Vec<TaskPublicChoiceV1>) {
    let choices: Vec<_> = reading
        .options
        .iter()
        .filter_map(|o| match &o.candidate {
            TaskPublicCandidateV1::Strategic {
                action:
                    TaskStrategicActionV1::ClaimReward { .. }
                    | TaskStrategicActionV1::OpenCardReward { .. },
            } => Some(o.choice.clone()),
            _ => None,
        })
        .collect();
    (
        RunPlayBoundChoiceV1 {
            source: reading.source.clone().unwrap(),
            choice: choices[0].clone(),
        },
        choices[1..].to_vec(),
    )
}

#[test]
fn reward_sequence_reindexes_after_claims_and_reopens_at_the_unselected_card() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "reward_sequence");
    let before = game.head().clone();
    let reading = game.decision_reading_v1(&[], None).unwrap();
    assert!(reading.can_collect_rewards);
    let (first, following) = commands(&reading);
    game.choose_reward_sequence_v1(&first, &following).unwrap();
    let after = game.decision_reading_v1(&[], None).unwrap();
    assert!(!after.can_collect_rewards);
    assert!(after.view.contains("Headbutt"));
    let session = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(session.run_state.gold, 110);
    assert_eq!(
        session.run_state.master_deck,
        fixture("reward_sequence").run_state.master_deck
    );
    assert_eq!(
        session.run_state.potions[0].as_ref().unwrap().id,
        PotionId::AncientPotion
    );
    let EngineState::RewardScreen(reward) = session.engine_state else {
        panic!("expected card reward")
    };
    assert_eq!(reward.pending_card_reward_index, Some(0));
    assert!(reward.pending_card_choice.is_some());
    assert_eq!(game.head().head_revision, before.head_revision + 1);
    let receipt = after.last_choice.unwrap();
    assert_eq!(receipt.choices.len(), 3);
    assert_ne!(
        receipt.choices[1], following[0],
        "the potion's original menu ordinal cannot be reused"
    );
    let committed = game.head().clone();
    assert!(game.choose_reward_sequence_v1(&first, &following).is_err());
    assert_eq!(*game.head(), committed);
    drop(game);
    let mut reopened =
        RunPlayControllerV1::open(RunPlayPathsV1::new(dir.0.join("reward_sequence"))).unwrap();
    assert_eq!(*reopened.head(), committed);
    assert_eq!(
        reopened
            .decision_reading_v1(&[], None)
            .unwrap()
            .last_choice
            .unwrap(),
        receipt
    );
}

#[test]
fn reward_sequence_rejects_an_uncollectable_later_potion_without_publishing_the_prefix() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "reward_sequence_crowded");
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let (first, following) = commands(&reading);
    assert_eq!(
        following.len(),
        3,
        "both potions are initially offered as legal claims"
    );
    let before = game.head().clone();
    assert!(game
        .choose_reward_sequence_v1(&first, &following)
        .unwrap_err()
        .contains("unavailable"));
    assert_eq!(*game.head(), before);
    assert_eq!(game.decision_reading_v1(&[], None).unwrap(), reading);
    game.choose_bound_v1(&first).unwrap();
    let after_single = game.head().clone();
    assert!(game
        .choose_reward_sequence_v1(&first, &following)
        .unwrap_err()
        .contains("Decision changed"));
    assert_eq!(*game.head(), after_single);
}

#[test]
fn reward_sequence_rejects_duplicate_out_of_scope_and_early_card_choices() {
    let dir = TestDirectory::new();
    let mut game = controller(&dir.0, "reward_sequence");
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let (first, following) = commands(&reading);
    let before = game.head().clone();
    for tail in [
        vec![],
        vec![first.choice.clone(), following[1].clone()],
        vec![following[1].clone(), following[0].clone()],
        vec![TaskPublicChoiceV1::one(999), following[1].clone()],
        vec![first.choice.clone(); 8],
    ] {
        assert!(game.choose_reward_sequence_v1(&first, &tail).is_err());
        assert_eq!(*game.head(), before);
    }
    let leave = reading.options.last().unwrap();
    let unsupported = RunPlayBoundChoiceV1 {
        source: first.source.clone(),
        choice: leave.choice.clone(),
    };
    assert!(game
        .choose_reward_sequence_v1(&unsupported, &[following[1].clone()])
        .is_err());
    assert_eq!(*game.head(), before);
    let open_only = RunPlayBoundChoiceV1 {
        source: first.source,
        choice: following[1].clone(),
    };
    game.choose_reward_sequence_v1(&open_only, &[]).unwrap();
    let session = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(session.run_state.gold, 99);
    assert!(session.run_state.potions.iter().all(Option::is_none));
}

#[test]
fn reward_sequence_keeps_duplicate_occurrences_and_runs_gold_relic_effects() {
    let dir = TestDirectory::new();
    let mut session = fixture("reward_sequence");
    session.run_state.current_hp = 40;
    session
        .run_state
        .relics
        .push(RelicState::new(RelicId::BloodyIdol));
    let EngineState::RewardScreen(rewards) = &mut session.engine_state else {
        unreachable!()
    };
    rewards.items.insert(0, RewardItem::Gold { amount: 11 });
    rewards.items.insert(
        3,
        RewardItem::Potion {
            potion_id: PotionId::AncientPotion,
        },
    );
    let mut game = controller_with_session(&dir.0, "duplicate_rewards", &session);
    let reading = game.decision_reading_v1(&[], None).unwrap();
    let (_, all) = commands(&reading);
    let first = RunPlayBoundChoiceV1 {
        source: reading.source.unwrap(),
        choice: all[0].clone(),
    };
    // Choose the second gold and second potion. Identical unselected items stay.
    game.choose_reward_sequence_v1(&first, &[all[2].clone(), all[3].clone()])
        .unwrap();
    let actual = game
        .current_external_exact_checkpoint_v1()
        .unwrap()
        .into_session()
        .unwrap();
    assert_eq!(actual.run_state.gold, 110);
    assert_eq!(actual.run_state.current_hp, 45);
    let EngineState::RewardScreen(rewards) = actual.engine_state else {
        unreachable!()
    };
    assert_eq!(rewards.items[0], RewardItem::Gold { amount: 11 });
    assert_eq!(
        rewards.items[1],
        RewardItem::Potion {
            potion_id: PotionId::AncientPotion
        }
    );
    assert_eq!(rewards.pending_card_reward_index, Some(2));
}

#[test]
fn reward_sequence_rejects_inventory_drift_and_a_potion_that_was_not_obtained() {
    let session = fixture("reward_sequence");
    let dir = TestDirectory::new();
    let mut game = controller_with_session(&dir.0, "resolver_checks", &session);
    let reading = game
        .decision_reading_v1(&[DecisionDetailV1::State], None)
        .unwrap();
    let (first, following) = commands(&reading);
    let choices = [
        first.choice.clone(),
        following[0].clone(),
        following[1].clone(),
    ];
    let public = reading.state.unwrap();
    let plan = crate::reward_sequence::RewardSequenceV1::compile(&public, &choices).unwrap();
    let mut changed = public.clone();
    if let TaskPublicObservationV1::Strategic { observation } = &mut changed.observation {
        if let PlannerDecisionContext::Reward { pending_rewards } = &mut observation.context {
            pending_rewards.swap(0, 1);
        }
    }
    assert!(plan
        .resolve(0, &changed)
        .unwrap_err()
        .contains("inventory changed"));
    // A consumed claim without the potion (e.g. an acquisition blocker) cannot
    // qualify as the requested collection, even if the menu removed the item.
    assert!(plan
        .resolve(2, &public)
        .unwrap_err()
        .contains("not obtained"));
    assert!(plan.check_endpoint(&public).is_err());
}
