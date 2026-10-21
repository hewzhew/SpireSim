use super::*;
use sts_combat_outcome::agent::{
    belief::IndependentStreamsCombatBeliefSamplerV1,
    information::run::{PublicCombatRunContextGapV1, PublicCombatRunContextV1},
};
use sts_core::{
    content::{cards::CardId, monsters::factory::EncounterId},
    runtime::combat::CombatCard,
    sim::combat::{CombatPosition, EngineCombatStepper},
    state::map::node::RoomType,
};

fn position() -> CombatPosition {
    let mut position = crate::local::ConstructedCombat {
        seed: 73,
        encounter: EncounterId::JawWorm,
        act: 1,
        room: RoomType::MonsterRoom,
        hp: 40,
        deck: vec![
            (CardId::FlashOfSteel, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
        ],
        relics: vec![],
        potions: vec![],
        max_potion_uses: None,
    }
    .build()
    .unwrap();
    let combat = &mut position.combat;
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::FlashOfSteel, 1)];
    combat.zones.draw_pile = [CardId::Strike, CardId::Defend]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 2))
        .collect();
    combat.entities.monsters[0].current_hp = 9;
    combat.zones.discard_pile.clear();
    position
}

fn environment(position: &CombatPosition) -> CombatBeliefEnvironmentV1 {
    CombatBeliefEnvironmentV1::from_sampler(
        PublicCombatRunContextV1::Unavailable {
            reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
        },
        position,
        &IndependentStreamsCombatBeliefSamplerV1::new((1..=8).collect())
            .with_draw_order_knowledge(1_000),
    )
    .unwrap()
}

fn config() -> LookaheadConfig {
    LookaheadConfig {
        depth: 2,
        width: 4,
        work: 4_000,
        max_engine_steps: 512,
        selection_limit: 16,
        allowed_potion_slots: vec![],
        rejuvenation: None,
    }
}

fn score(boundary: &CombatPublicBoundaryV1) -> f64 {
    match boundary {
        CombatPublicBoundaryV1::Terminal {
            outcome: CombatTerminal::Win,
            observation,
            ..
        } => 10_000.0 + observation.player.hp as f64,
        CombatPublicBoundaryV1::Terminal { .. } => -10_000.0,
        CombatPublicBoundaryV1::Decision { decision } => {
            let c = &decision.observation;
            c.player.hp as f64 * 10.0 + c.player.block as f64 * 3.0
                - c.monsters
                    .iter()
                    .map(|m| m.hp.max(0) as f64 * 10.0)
                    .sum::<f64>()
        }
    }
}

#[test]
fn search_chooses_once_before_drawing_then_uses_each_public_observation() {
    let environment = environment(&position());
    let report = lookahead(
        &environment,
        &config(),
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    let CombatPublicBoundaryV1::Decision { decision } = &report.policy.boundary else {
        panic!()
    };
    let Some(Choice::Atomic { action_ordinal }) = report.policy.action else {
        panic!()
    };
    assert!(matches!(
        decision.actions.atomic_actions[action_ordinal],
        Atomic::PlayCard { hand_index: 0, .. }
    ));
    assert_eq!(report.policy.branches.len(), 2);
    for branch in &report.policy.branches {
        let CombatPublicBoundaryV1::Decision { decision } = &branch.continuation.boundary else {
            panic!()
        };
        let Some(Choice::Atomic { action_ordinal }) = branch.continuation.action else {
            panic!()
        };
        let Atomic::PlayCard { hand_index, .. } = decision.actions.atomic_actions[action_ordinal]
        else {
            panic!()
        };
        let drawn = decision.observation.cards.hand.cards[hand_index].card_id;
        assert!(matches!(drawn, CardId::Strike | CardId::Defend));
        assert!(!branch.draw_pile_changes.is_empty());
    }
    let expected = report
        .policy
        .branches
        .iter()
        .map(|b| b.probability * b.continuation.value_estimate)
        .sum::<f64>();
    assert!((report.policy.value_estimate - expected).abs() < 1e-9);
    let leaves = &report.policy.leaves;
    assert!((leaves.win_mass + leaves.loss_mass + leaves.unfinished_mass - 1.0).abs() < 1e-9);
    assert!(leaves.win_mass > 0.0 && leaves.win_mass < 1.0);
    assert!(replay_policy(&environment, &report.policy, &EngineCombatStepper, 512).unwrap() > 8);
}

#[test]
fn private_source_permutation_does_not_change_the_public_policy() {
    let first = position();
    let mut second = first.clone();
    second.combat.zones.draw_pile.reverse();
    let left = lookahead(
        &environment(&first),
        &config(),
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    let right = lookahead(
        &environment(&second),
        &config(),
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&left).unwrap(),
        serde_json::to_value(&right).unwrap()
    );
}

#[test]
fn insufficient_work_returns_unknown_instead_of_favoring_the_first_action() {
    let environment = environment(&position());
    let mut config = config();
    config.work = 1;
    let report = lookahead(
        &environment,
        &config,
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    assert_eq!(report.policy.stop, Some(LeafStop::Work));
    assert!(report.policy.action.is_none() && report.alternatives.is_empty());
    assert_eq!(report.policy.leaves.unfinished_mass, 1.0);
    assert_eq!(report.stats.transitions, 0);
    config.work = 45;
    let report = lookahead(
        &environment,
        &config,
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    assert!(report.stats.transitions <= config.work);
    assert!(report.policy.leaves.unfinished_mass > 0.0);
}

#[test]
fn timeout_and_nonfinite_estimates_do_not_publish_partial_comparisons() {
    let environment = environment(&position());
    let report = lookahead(
        &environment,
        &config(),
        Duration::ZERO,
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    assert_eq!(report.policy.stop, Some(LeafStop::Time));
    assert!(report.policy.action.is_none());
    assert!(lookahead(
        &environment,
        &config(),
        Duration::from_secs(1),
        &|_| f64::NAN,
        &EngineCombatStepper
    )
    .is_err());
}

#[test]
fn duplicate_representatives_and_selection_enumeration_are_bounded() {
    let mut source = position();
    source.combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::Defend, 2),
        CombatCard::for_scenario(CardId::Strike, 3),
    ];
    let environment = environment(&source);
    let CombatPublicBoundaryV1::Decision { decision } = environment.public_history().current()
    else {
        panic!()
    };
    let (actions, omitted, _) = candidates(decision, &config());
    assert_eq!(omitted, 1);
    assert_eq!(actions.len(), decision.actions.atomic_actions.len() - 1);
    let mut selected = Vec::new();
    selection_prefixes(
        &(0..10).collect::<Vec<_>>(),
        2,
        5,
        7,
        &mut vec![],
        &mut selected,
    );
    assert_eq!(selected.len(), 7);
    assert!(selected.iter().all(|s| (2..=5).contains(&s.len())));
}

#[test]
fn replay_rejects_changed_branch_probabilities_and_observations() {
    let environment = environment(&position());
    let mut report = lookahead(
        &environment,
        &config(),
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    report.policy.branches[0].probability += 0.1;
    assert!(replay_policy(&environment, &report.policy, &EngineCombatStepper, 512).is_err());
}

#[test]
fn smoke_escape_is_not_reported_as_a_victory() {
    use sts_core::content::potions::{Potion, PotionId};
    let mut source = position();
    source.combat.entities.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 99))];
    let environment = environment(&source);
    let mut config = config();
    config.allowed_potion_slots = vec![0];
    let error = lookahead(
        &environment,
        &config,
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .err()
    .expect("escape classification must fail explicitly");
    assert!(error.contains("escaped combat"), "{error}");
}

#[test]
fn search_plans_headbutt_selection_and_known_draw_as_one_continuation() {
    let mut source = position();
    source.combat.entities.monsters[0].current_hp = 28;
    source.combat.turn.energy = 4;
    source.combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Headbutt, 10),
        CombatCard::for_scenario(CardId::ShrugItOff, 11),
    ];
    source.combat.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::Defend, 20),
        CombatCard::for_scenario(CardId::Burn, 21),
    ]
    .into();
    source.combat.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Carnage, 30),
        CombatCard::for_scenario(CardId::Strike, 31),
    ]
    .into();
    let environment = environment(&source);
    let mut config = config();
    config.depth = 4;
    config.work = 20_000;
    let report = lookahead(
        &environment,
        &config,
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    assert_eq!(report.policy.leaves.win_mass, 1.0);
    let mut node = &report.policy;
    let mut seen_selection = false;
    while node.action.is_some() {
        seen_selection |= matches!(node.action, Some(Choice::Selection { .. }));
        assert_eq!(
            node.branches.len(),
            1,
            "Headbutt's known draw does not reveal hidden order"
        );
        node = &node.branches[0].continuation;
    }
    assert!(seen_selection);
    assert_eq!(node.stop, Some(LeafStop::Terminal));
    assert!(replay_policy(&environment, &report.policy, &EngineCombatStepper, 512).unwrap() >= 32);
}

#[test]
fn replenishment_inside_hypothetical_observation_branches_replays_with_its_budget() {
    let environment = environment(&position());
    let mut config = config();
    config.work = 20_000;
    config.rejuvenation = Some(HistoryRejuvenationConfig {
        target_particles: 8,
        moves_per_particle: 4,
        seed: 337,
    });
    let report = lookahead(
        &environment,
        &config,
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    assert!(
        report.policy.rejuvenation.is_none(),
        "root population is already full"
    );
    assert!(report
        .policy
        .branches
        .iter()
        .any(|b| b.continuation.rejuvenation.is_some()));
    assert!(report.stats.reconstruction_transitions > 0);
    assert!(report.stats.transitions <= config.work);
    assert!(replay_policy(&environment, &report.policy, &EngineCombatStepper, 512).unwrap() > 8);
    let mut changed = report.policy;
    changed
        .branches
        .iter_mut()
        .find_map(|b| b.continuation.rejuvenation.as_mut())
        .unwrap()
        .report
        .accepted += 1;
    assert!(
        replay_policy(&environment, &changed, &EngineCombatStepper, 512)
            .unwrap_err()
            .contains("rejuvenation receipt")
    );
}

#[test]
fn insufficient_reconstruction_budget_does_not_silently_expand_a_singleton() {
    let initial = environment(&position());
    let particle = initial.particles()[0].clone();
    let small =
        CombatBeliefEnvironmentV1::from_particles(initial.public_history().clone(), vec![particle])
            .unwrap();
    let after = small
        .step(
            Choice::Atomic { action_ordinal: 0 },
            &EngineCombatStepper,
            512,
        )
        .unwrap()
        .pop()
        .unwrap()
        .into_environment();
    let mut config = config();
    config.work = 1;
    config.rejuvenation = Some(HistoryRejuvenationConfig {
        target_particles: 8,
        moves_per_particle: 4,
        seed: 337,
    });
    let report = lookahead(
        &after,
        &config,
        Duration::from_secs(10),
        &score,
        &EngineCombatStepper,
    )
    .unwrap();
    assert_eq!(report.policy.stop, Some(LeafStop::PopulationWork));
    assert!(report.policy.action.is_none());
    assert_eq!(report.stats.transitions, 0);
}

#[test]
fn independent_execution_matches_visible_draws_and_stops_outside_population() {
    let initial = environment(&position());
    let left = initial.particles()[0].clone();
    let right = initial
        .particles()
        .iter()
        .find(|p| {
            p.private_position().combat.zones.draw_pile[0].id
                != left.private_position().combat.zones.draw_pile[0].id
        })
        .unwrap();
    // Deliberately exclude the execution world's outcome from this fixture's
    // population. No private execution state enters the action selector.
    let candidates =
        CombatBeliefEnvironmentV1::from_particles(initial.public_history().clone(), vec![left])
            .unwrap();
    let action = Choice::Atomic { action_ordinal: 0 };
    let branches = candidates
        .step(action.clone(), &EngineCombatStepper, 512)
        .unwrap();
    let before = right.private_position().clone();
    let (history, endpoint) = initial
        .public_history()
        .advance_exact(&before, action, &EngineCombatStepper, 512)
        .unwrap();
    assert_eq!(right.private_position(), &before);
    assert_ne!(endpoint, before);
    let observed = history.entries().last().unwrap();
    assert!(branches.iter().all(|b| !b.matches_observation(observed)));
    assert_eq!(
        candidates.particles().len(),
        1,
        "missing actual outcome is never injected as a candidate"
    );
}
