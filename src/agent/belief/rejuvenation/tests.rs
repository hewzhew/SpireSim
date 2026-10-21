use super::*;
use crate::agent::belief::{CombatPublicBoundaryV1, IndependentStreamsCombatBeliefSamplerV1};
use crate::agent::information::{
    action::{PublicCombatActionChoiceV1 as Choice, PublicCombatAtomicActionV1 as Atomic},
    run::{PublicCombatRunContextGapV1, PublicCombatRunContextV1},
};
use crate::content::{
    cards::CardId,
    monsters::EnemyId,
    relics::{RelicId, RelicState},
};
use crate::runtime::combat::{CombatCard, DrawCardKind, Intent};
use crate::sim::combat::{CombatStepLimits, CombatStepResult, CombatTerminal, EngineCombatStepper};
use crate::state::core::{ClientInput, EngineState};

fn source(hand: &[CardId]) -> CombatPosition {
    let mut combat = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.current_hp = 100;
    monster.max_hp = 100;
    let id = monster.id;
    combat.entities.monsters.push(monster);
    combat.set_monster_protocol_visible_intent(
        id,
        Intent::Attack {
            damage: 11,
            hits: 1,
        },
    );
    combat.zones.hand = hand
        .iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(*id, i as u32 + 1))
        .collect();
    combat.zones.draw_pile = [CardId::Strike, CardId::Defend, CardId::Bash, CardId::Cleave]
        .iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(*id, i as u32 + 20))
        .collect();
    combat.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Carnage, 40)].into();
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

fn env(source: &CombatPosition, seeds: Vec<u64>) -> CombatBeliefEnvironmentV1 {
    CombatBeliefEnvironmentV1::from_sampler(
        PublicCombatRunContextV1::Unavailable {
            reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
        },
        source,
        &IndependentStreamsCombatBeliefSamplerV1::new(seeds).with_draw_order_knowledge(1000),
    )
    .unwrap()
}

fn play(env: &CombatBeliefEnvironmentV1, card: CardId) -> Choice {
    let CombatPublicBoundaryV1::Decision { decision } = env.public_history().current() else {
        panic!()
    };
    Choice::Atomic {
        action_ordinal: decision
            .actions
            .atomic_actions
            .iter()
            .position(|a| match a {
                Atomic::PlayCard { hand_index, .. } => {
                    decision.observation.cards.hand.cards[*hand_index].card_id == card
                }
                _ => false,
            })
            .unwrap(),
    }
}

fn settings() -> HistoryRejuvenationConfig {
    HistoryRejuvenationConfig {
        target_particles: 24,
        moves_per_particle: 12,
        seed: 334,
    }
}

#[test]
fn uncovered_public_successor_can_be_recovered_without_the_execution_position() {
    let source = source(&[CardId::FlashOfSteel]);
    let initial = env(&source, vec![73]);
    let execution = env(&source, vec![101]).particles()[0]
        .private_position()
        .clone();
    let action = play(&initial, CardId::FlashOfSteel);
    let (observed_history, _) = initial
        .public_history()
        .advance_exact(&execution, action.clone(), &EngineCombatStepper, 512)
        .unwrap();
    let observed = observed_history.entries().last().unwrap();
    assert!(initial
        .step(action, &EngineCombatStepper, 512)
        .unwrap()
        .iter()
        .all(|branch| !branch.matches_observation(observed)));

    let config = ObservedSuccessorRescueConfig {
        attempts_per_donor: 512,
        moves_per_attempt: 4,
        max_retained: 16,
        seed: 31337,
    };
    assert!(initial
        .rescue_observed_successor(
            observed,
            &config,
            config.work_bound(&initial).unwrap() - 1,
            std::time::Duration::from_secs(5),
            &EngineCombatStepper,
            512,
        )
        .unwrap_err()
        .contains("insufficient work"));
    let (recovered, report) = initial
        .rescue_observed_successor(
            observed,
            &config,
            config.work_bound(&initial).unwrap(),
            std::time::Duration::from_secs(5),
            &EngineCombatStepper,
            512,
        )
        .unwrap();
    let recovered = recovered.expect("public-history-compatible successor proposal");
    assert_eq!(
        recovered.public_history().current(),
        observed_history.current()
    );
    assert!(report.compatible_proposals > 0);
    assert!(report.first_compatible_proposal.is_some());
    assert!(report.replay_transitions <= config.work_bound(&initial).unwrap());
    assert!(matches!(
        recovered.particles()[0].origin(),
        CombatBeliefParticleOriginV1::ObservedSuccessorRescue { .. }
    ));
    assert_eq!(
        initial.particles()[0].origin(),
        CombatBeliefParticleOriginV1::DrawOrderConditionedIndependentStreams { particle_seed: 73 }
    );
}

#[test]
fn observed_successor_rescue_bounds_retained_candidates_without_shortening_the_schedule() {
    let initial = env(&source(&[CardId::Strike]), vec![73]);
    let action = play(&initial, CardId::Strike);
    let execution = initial.particles()[0].private_position();
    let (observed_history, _) = initial
        .public_history()
        .advance_exact(execution, action, &EngineCombatStepper, 512)
        .unwrap();
    let observed = observed_history.entries().last().unwrap();
    let config = ObservedSuccessorRescueConfig {
        attempts_per_donor: 20,
        moves_per_attempt: 2,
        max_retained: 3,
        seed: 29,
    };
    let run = || {
        initial
            .rescue_observed_successor(
                observed,
                &config,
                config.work_bound(&initial).unwrap(),
                std::time::Duration::from_secs(5),
                &EngineCombatStepper,
                512,
            )
            .unwrap()
    };
    let (first, report) = run();
    let first = first.unwrap();
    assert_eq!(report.proposals, 20);
    assert_eq!(report.compatible_proposals, 20);
    assert_eq!(report.retained_proposals, 3);
    assert_eq!(first.particles().len(), 3);
    let (second, repeated) = run();
    assert_eq!(report, repeated);
    assert_eq!(first.particles(), second.unwrap().particles());
}

#[test]
fn unseen_futures_change_while_past_draws_and_the_source_remain_intact() {
    let initial = env(
        &source(&[CardId::FlashOfSteel, CardId::ShrugItOff]),
        vec![73],
    );
    let action = play(&initial, CardId::FlashOfSteel);
    let after = initial
        .step(action, &EngineCombatStepper, 512)
        .unwrap()
        .pop()
        .unwrap()
        .into_environment();
    let before = after.particles()[0].clone();
    let (revised, report) = after
        .rejuvenate_history(&settings(), 10000, &EngineCombatStepper, 512)
        .unwrap();
    assert_eq!(after.particles()[0], before);
    assert_eq!(revised.public_history(), after.public_history());
    assert!(report.accepted > 0 && report.contradicted_history > 0);
    assert!(
        report.distinct_draw_kind_orders >= 3,
        "unseen orders, not just different RNG: {report:?}"
    );
    assert!(report.replay_transitions <= settings().work_bound(&after).unwrap());
    for particle in revised.particles() {
        let mut work = 0;
        let replay = replay_root(
            particle.reconstruction_root.as_ref().unwrap(),
            after.public_history(),
            &EngineCombatStepper,
            512,
            &mut work,
        )
        .unwrap()
        .unwrap();
        assert_eq!(&replay, particle.private_position());
    }
    assert!(
        revised
            .step(
                play(&revised, CardId::ShrugItOff),
                &EngineCombatStepper,
                512
            )
            .unwrap()
            .len()
            >= 2
    );
    let (again, repeated) = after
        .rejuvenate_history(&settings(), 10000, &EngineCombatStepper, 512)
        .unwrap();
    assert_eq!(report, repeated);
    assert_eq!(revised.particles(), again.particles());
}

#[test]
fn headbutt_memory_survives_root_mutations_and_the_next_draw() {
    let initial = env(
        &source(&[CardId::Headbutt, CardId::ShrugItOff, CardId::ShrugItOff]),
        vec![73],
    );
    let after = initial
        .step(play(&initial, CardId::Headbutt), &EngineCombatStepper, 512)
        .unwrap()
        .pop()
        .unwrap()
        .into_environment();
    let (revised, report) = after
        .rejuvenate_history(&settings(), 10000, &EngineCombatStepper, 512)
        .unwrap();
    let known = DrawCardKind::from(&CombatCard::for_scenario(CardId::Carnage, 40));
    assert_eq!(
        revised
            .public_history()
            .draw_order_knowledge()
            .unwrap()
            .known_prefix(),
        &[known]
    );
    assert!(report.distinct_draw_kind_orders > 1);
    let mut branches = revised
        .step(
            play(&revised, CardId::ShrugItOff),
            &EngineCombatStepper,
            512,
        )
        .unwrap();
    assert_eq!(branches.len(), 1);
    let after_known = branches.pop().unwrap().into_environment();
    assert_eq!(
        after_known.particles()[0]
            .private_position()
            .combat
            .zones
            .hand
            .last()
            .unwrap()
            .id,
        CardId::Carnage
    );
    assert!(
        after_known
            .step(
                play(&after_known, CardId::ShrugItOff),
                &EngineCombatStepper,
                512
            )
            .unwrap()
            .len()
            > 1
    );
}

#[test]
fn frozen_eye_preserves_the_public_permutation() {
    let mut position = source(&[CardId::FlashOfSteel]);
    position
        .combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::FrozenEye));
    let initial = env(&position, vec![73]);
    let (revised, report) = initial
        .rejuvenate_history(&settings(), 0, &EngineCombatStepper, 512)
        .unwrap();
    assert_eq!(report.distinct_draw_kind_orders, 1);
    assert!(report.contradicted_history > 0);
    assert!(revised
        .particles()
        .iter()
        .all(|p| p.private_position().combat.zones.draw_pile == position.combat.zones.draw_pile));
}

#[test]
fn donor_mass_is_preserved_and_insufficient_work_does_not_change_population() {
    let initial = env(&source(&[CardId::FlashOfSteel]), vec![73, 74]);
    let mut particles = initial.particles().to_vec();
    particles[0].set_probability_mass(0.8);
    particles[1].set_probability_mass(0.2);
    let initial =
        CombatBeliefEnvironmentV1::from_particles(initial.public_history().clone(), particles)
            .unwrap();
    let config = HistoryRejuvenationConfig {
        target_particles: 5,
        ..settings()
    };
    let (revised, report) = initial
        .rejuvenate_history(&config, 0, &EngineCombatStepper, 512)
        .unwrap();
    assert_eq!(report.output_particles, 4);
    let weights = revised
        .particles()
        .iter()
        .map(|p| p.probability_mass())
        .collect::<Vec<_>>();
    assert_eq!(weights, [0.4, 0.4, 0.1, 0.1]);
    let after = initial
        .step(
            play(&initial, CardId::FlashOfSteel),
            &EngineCombatStepper,
            512,
        )
        .unwrap()
        .pop()
        .unwrap()
        .into_environment();
    let before = after.particles().to_vec();
    assert!(after
        .rejuvenate_history(
            &config,
            config.work_bound(&after).unwrap() - 1,
            &EngineCombatStepper,
            512
        )
        .unwrap_err()
        .contains("insufficient work"));
    assert_eq!(after.particles(), before);
}

#[test]
fn later_boundary_samples_cannot_claim_a_reconstruction_root() {
    let initial = env(&source(&[CardId::FlashOfSteel]), vec![73]);
    let after = initial
        .step(
            play(&initial, CardId::FlashOfSteel),
            &EngineCombatStepper,
            512,
        )
        .unwrap()
        .pop()
        .unwrap()
        .into_environment();
    let resampled = CombatBeliefEnvironmentV1::from_history_and_sampler(
        after.public_history().clone(),
        after.particles()[0].private_position(),
        &IndependentStreamsCombatBeliefSamplerV1::new(vec![75]).with_draw_order_knowledge(1000),
    )
    .unwrap();
    assert!(resampled
        .rejuvenate_history(&settings(), 10000, &EngineCombatStepper, 512)
        .unwrap_err()
        .contains("reconstruction root"));
}

#[test]
fn same_current_boundary_does_not_erase_conflicting_intermediate_observations() {
    let initial = env(&source(&[CardId::FlashOfSteel]), vec![73]);
    let after = initial
        .step(
            play(&initial, CardId::FlashOfSteel),
            &EngineCombatStepper,
            512,
        )
        .unwrap()
        .pop()
        .unwrap()
        .into_environment();
    let mut tampered = serde_json::to_value(after.public_history()).unwrap();
    tampered["entries"][1]["draw_pile_changes"] = serde_json::json!([]);
    let history: CombatPublicHistoryV1 = serde_json::from_value(tampered).unwrap();
    assert_eq!(history.current(), after.public_history().current());
    let mut work = 0;
    assert!(replay_root(
        initial.particles()[0].private_position(),
        &history,
        &EngineCombatStepper,
        512,
        &mut work
    )
    .unwrap()
    .is_none());
}

#[test]
fn observation_failures_are_not_counted_as_contradictory_candidates() {
    struct NoObserver;
    impl CombatStepper for NoObserver {
        fn atomic_actions(&self, p: &CombatPosition) -> Vec<ClientInput> {
            EngineCombatStepper.atomic_actions(p)
        }
        fn apply_to_stable(
            &self,
            p: &CombatPosition,
            a: ClientInput,
            l: CombatStepLimits,
        ) -> CombatStepResult {
            EngineCombatStepper.apply_to_stable(p, a, l)
        }
        fn terminal(&self, p: &CombatPosition) -> CombatTerminal {
            EngineCombatStepper.terminal(p)
        }
    }
    let initial = env(&source(&[CardId::FlashOfSteel]), vec![73]);
    let after = initial
        .step(
            play(&initial, CardId::FlashOfSteel),
            &EngineCombatStepper,
            512,
        )
        .unwrap()
        .pop()
        .unwrap()
        .into_environment();
    assert!(after
        .rejuvenate_history(&settings(), 10000, &NoObserver, 512)
        .unwrap_err()
        .contains("observations"));
}
