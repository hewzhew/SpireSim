use super::*;
use sts_core::{
    content::{
        cards::CardId,
        monsters::EnemyId,
        potions::{Potion, PotionId},
    },
    runtime::combat::CombatCard,
    state::core::EngineState,
};

fn limits() -> CombatStepLimits {
    CombatStepLimits {
        max_engine_steps: 512,
        deadline: None,
    }
}

fn root(hand: &[CardId], monster_hp: i32) -> CombatPosition {
    let mut combat = sts_core::test_support::blank_test_combat();
    let mut monster = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    monster.current_hp = monster_hp;
    monster.max_hp = monster_hp;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = hand
        .iter()
        .enumerate()
        .map(|(i, &id)| CombatCard::for_scenario(id, i as u32 + 1))
        .collect();
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

fn strike() -> ClientInput {
    ClientInput::PlayCard {
        card_index: 0,
        target: Some(1),
    }
}

fn spec(origin: CombatPosition, prefix: Vec<ClientInput>) -> ContinuationTaskSpec {
    ContinuationTaskSpec {
        source_producer: "constructed unit fixture".into(),
        group: "fixture-family".into(),
        origin,
        prefix,
        requirement: ResourceRequirement {
            min_final_hp: 1,
            preserve_starting_potion_slots: 0,
        },
    }
}

fn experiment(spec: ContinuationTaskSpec) -> ContinuationExperiment {
    ContinuationExperiment::new(
        ValidatedContinuationTask::new(spec, limits()).unwrap(),
        SolverConfig {
            max_width: 4,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn prefix_validation_rejects_illegality_interruption_and_terminal_anchor() {
    let origin = root(&[CardId::Strike], 6);
    let saved = origin.clone();
    let bad = ClientInput::PlayCard {
        card_index: 9,
        target: Some(1),
    };
    assert!(ValidatedContinuationTask::new(spec(origin.clone(), vec![bad]), limits()).is_err());
    assert!(
        ValidatedContinuationTask::new(spec(origin.clone(), vec![strike()]), limits()).is_err()
    );
    let deadline = CombatStepLimits {
        deadline: Some(Instant::now()),
        ..limits()
    };
    assert!(
        ValidatedContinuationTask::new(spec(origin.clone(), vec![strike()]), deadline).is_err()
    );
    assert_eq!(origin, saved);
}

#[test]
fn supplied_reference_must_reproduce_full_victory() {
    let origin = root(&[CardId::Strike], 6);
    let (won, steps) = checked_step(&origin, &strike(), limits()).unwrap();
    let (states, replay_steps) =
        validate_reference_trajectory(&origin, &[strike()], &won, limits()).unwrap();
    assert_eq!(states, [origin.clone(), won.clone()]);
    assert_eq!(replay_steps, steps);
    let mut changed = won.clone();
    changed.combat.entities.player.current_hp -= 1;
    assert!(validate_reference_trajectory(&origin, &[strike()], &changed, limits()).is_err());
    assert!(validate_reference_trajectory(&origin, &[], &origin, limits()).is_err());
}

#[test]
fn verification_uses_the_grant_and_resumes_before_publishing() {
    let origin = root(&[CardId::Strike, CardId::Strike], 12);
    let mut run = experiment(spec(origin.clone(), vec![strike()]));
    for _ in 0..200 {
        let receipt = run.advance(1, Duration::from_secs(10)).unwrap();
        assert_eq!(receipt.used_work, 1);
        if receipt.verification_pending {
            break;
        }
    }
    assert!(run.replay.is_some());
    assert!(run.witnesses().is_empty());
    let before = run.work();
    assert_eq!(run.advance(100, Duration::ZERO).unwrap().used_work, 0);
    assert_eq!(run.work(), before);
    let receipt = run.advance(1, Duration::from_secs(10)).unwrap();
    assert!(receipt.verification_pending);
    assert_eq!(receipt.cumulative_origin_replay_work, 1);
    assert!(!receipt.target_witness_observed);
    let receipt = run.advance(1, Duration::from_secs(10)).unwrap();
    assert_eq!(receipt.used_work, 1);
    assert_eq!(receipt.cumulative_origin_replay_work, 2);
    assert!(receipt.target_witness_observed);
    assert_eq!(
        receipt.cumulative_work,
        receipt.cumulative_executor_work + receipt.cumulative_origin_replay_work
    );
    assert_eq!(run.witnesses()[0].inputs, [strike(), strike()]);
    assert_eq!(run.task().spec().origin, origin);
    assert_eq!(run.task().validation_work(), 1);
}

#[test]
fn chunking_preserves_results_and_total_work() {
    let task = spec(
        root(&[CardId::Offering, CardId::Strike, CardId::Defend], 6),
        vec![],
    );
    let mut whole = experiment(task.clone());
    let mut chunks = experiment(task);
    whole.advance(300, Duration::from_secs(30)).unwrap();
    for _ in 0..30 {
        chunks.advance(10, Duration::from_secs(30)).unwrap();
    }
    assert!(!whole.witnesses().is_empty());
    assert_eq!(whole.work(), chunks.work());
    assert_eq!(
        serde_json::to_value(whole.witnesses()).unwrap(),
        serde_json::to_value(chunks.witnesses()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(whole.solver.stats()).unwrap(),
        serde_json::to_value(chunks.solver.stats()).unwrap()
    );
}

#[test]
fn prefix_expenditure_stays_origin_relative_and_concrete() {
    let mut origin = root(&[CardId::Strike], 26);
    origin.combat.entities.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 1)),
        Some(Potion::new(PotionId::BlockPotion, 2)),
    ];
    let prefix = vec![ClientInput::UsePotion {
        potion_index: 0,
        target: Some(1),
    }];
    let mut task = spec(origin, prefix);
    task.requirement.preserve_starting_potion_slots = 2;
    let mut run = experiment(task.clone());
    run.advance(500, Duration::from_secs(30)).unwrap();
    let won = run
        .witnesses()
        .iter()
        .find(|w| w.satisfies_requirement)
        .unwrap();
    assert_eq!(won.outcome.starting_potion_expenditure_slots, 1);
    assert_eq!(won.outcome.total_potion_expenditures, 1);
    assert!(run
        .solver
        .plans()
        .iter()
        .any(|p| p.outcome.total_potion_expenditures == 0));
    task.requirement.preserve_starting_potion_slots = 1;
    let mut spent = experiment(task);
    spent.advance(500, Duration::from_secs(30)).unwrap();
    assert!(!spent.witnesses().is_empty());
    assert!(spent.witnesses().iter().all(|w| !w.satisfies_requirement));
    assert!(!spent.receipts().last().unwrap().target_witness_observed);
}

#[test]
fn task_input_roundtrips_past_only_and_keeps_family_group() {
    let origin = root(&[CardId::Strike, CardId::Strike], 12);
    let a = ValidatedContinuationTask::new(spec(origin.clone(), vec![]), limits()).unwrap();
    let b = ValidatedContinuationTask::new(spec(origin, vec![strike()]), limits()).unwrap();
    assert_eq!(a.spec().group, b.spec().group);
    assert_ne!(a.id(), b.id());
    let bytes = serde_json::to_vec(b.spec()).unwrap();
    let loaded: ContinuationTaskSpec = serde_json::from_slice(&bytes).unwrap();
    let loaded = ValidatedContinuationTask::new(loaded, limits()).unwrap();
    assert_eq!(loaded.anchor(), b.anchor());
    assert_eq!(loaded.id(), b.id());
    let mut json = serde_json::to_value(b.spec()).unwrap();
    json["reference_suffix"] = serde_json::json!([strike()]);
    assert!(serde_json::from_value::<ContinuationTaskSpec>(json).is_err());
}

#[test]
fn absent_witness_and_impossible_hp_requirement_are_not_negative_labels() {
    let mut task = spec(root(&[CardId::Strike], 6), vec![]);
    task.requirement.min_final_hp = 1000;
    let mut run = experiment(task);
    let receipt = run.advance(0, Duration::from_secs(10)).unwrap();
    assert_eq!(receipt.stop, ContinuationStop::WorkBudget);
    assert_eq!(receipt.used_work, 0);
    assert!(!receipt.target_witness_observed);
    run.advance(100, Duration::from_secs(30)).unwrap();
    assert!(!run.witnesses().is_empty());
    assert!(!run.receipts().last().unwrap().target_witness_observed);
}

#[test]
fn replay_error_stops_session_without_publishing_a_witness() {
    let mut run = experiment(spec(root(&[CardId::Strike], 6), vec![]));
    for _ in 0..100 {
        run.advance(1, Duration::from_secs(10)).unwrap();
        if run.replay.is_some() {
            break;
        }
    }
    run.replay
        .as_mut()
        .unwrap()
        .expected
        .combat
        .entities
        .player
        .current_hp -= 1;
    assert!(run.advance(1, Duration::from_secs(10)).is_err());
    assert!(run.witnesses().is_empty());
    assert!(run.advance(100, Duration::from_secs(10)).is_err());
}
