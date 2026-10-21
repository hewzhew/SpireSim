use super::*;
use sts_core::content::{
    cards::CardId,
    monsters::EnemyId,
    potions::{Potion, PotionId},
};
use sts_core::runtime::combat::CombatCard;
use sts_core::state::core::{HandSelectReason, PendingChoice};

fn source() -> CombatSearchState {
    let mut c = sts_core::test_support::blank_test_combat();
    c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
    c.zones.hand = [CardId::Strike, CardId::Defend, CardId::BurningPact]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 1))
        .collect();
    c.entities.potions = vec![
        Some(Potion::new(PotionId::ColorlessPotion, 1)),
        Some(Potion::new(PotionId::FirePotion, 2)),
    ];
    CombatSearchState::new(CombatPosition::new(EngineState::CombatPlayerTurn, c)).unwrap()
}

fn equal_transition(
    a: Transition<CombatSearchState>,
    b: Transition<CombatSearchState>,
) -> Option<CombatSearchState> {
    match (a, b) {
        (
            Transition::Settled {
                state: a,
                engine_steps: ae,
            },
            Transition::Settled {
                state: b,
                engine_steps: be,
            },
        ) => {
            assert_eq!(a, b);
            assert_eq!(ae, be);
            Some(a)
        }
        (
            Transition::Interrupted {
                cause: a,
                engine_steps: ae,
            },
            Transition::Interrupted {
                cause: b,
                engine_steps: be,
            },
        ) => {
            assert_eq!(a, b);
            assert_eq!(ae, be);
            None
        }
        _ => panic!("prepared and unbound transition disagree"),
    }
}

#[test]
fn prepared_choices_preserve_inputs_and_successors_through_selection_assembly() {
    let space = CombatSearchSpace::default();
    let ordinary = source();
    let mut select = ordinary.position().clone();
    select.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2, 3]),
        min_cards: 1,
        max_cards: 2,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let mut pending = vec![(ordinary, 0), (CombatSearchState::new(select).unwrap(), 0)];
    let mut selected = false;
    let mut submitted = false;
    while let Some((state, depth)) = pending.pop() {
        let prepared = space.prepare_choices(&state).unwrap();
        assert_eq!(
            prepared.branches(),
            space
                .choices(&state)
                .unwrap()
                .iter()
                .map(|c| c.action.clone())
                .collect::<Vec<_>>()
        );
        for (i, branch) in prepared.branches().iter().enumerate() {
            let input = prepared.input(i).unwrap();
            assert_eq!(input, space.input(&state, branch).unwrap());
            // Check the frozen enumeration's admission against the simulator,
            // independently of the transition path shared by both adapters.
            if let Some(input) = &input {
                assert!(EngineCombatStepper.is_legal_action(state.position(), input));
            }
            selected |= matches!(branch, CombatBranch::SelectMember(_));
            submitted |= matches!(input, Some(ClientInput::SubmitSelection(_)));
            let limits = CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            };
            let next = equal_transition(
                space.transition(&state, branch, limits).unwrap(),
                prepared.transition(i, limits).unwrap(),
            )
            .unwrap();
            // Follow virtual assembly and physical inputs that open choices.
            // Do not enumerate a whole combat in this contract test.
            if depth < 4
                && (input.is_none()
                    || matches!(next.position().engine, EngineState::PendingChoice(_)))
            {
                pending.push((next, depth + 1));
            }
        }
    }
    assert!(selected && submitted);
}

#[test]
fn prepared_choices_keep_the_original_source_and_still_obey_limits() {
    let space = CombatSearchSpace::default();
    let original = source();
    let prepared = space.prepare_choices(&original).unwrap();
    let i = prepared
        .branches()
        .iter()
        .position(|b| *b == CombatBranch::Input(ClientInput::EndTurn))
        .unwrap();
    let branch = &prepared.branches()[i];
    let expired = CombatStepLimits {
        max_engine_steps: 512,
        deadline: Some(Instant::now()),
    };
    assert!(equal_transition(
        space.transition(&original, branch, expired).unwrap(),
        prepared.transition(i, expired).unwrap()
    )
    .is_none());
    let tiny = CombatStepLimits {
        max_engine_steps: 1,
        deadline: None,
    };
    assert!(equal_transition(
        space.transition(&original, branch, tiny).unwrap(),
        prepared.transition(i, tiny).unwrap()
    )
    .is_none());
    let full = CombatStepLimits {
        max_engine_steps: 512,
        deadline: None,
    };
    let advanced = equal_transition(
        space.transition(&original, branch, full).unwrap(),
        prepared.transition(i, full).unwrap(),
    )
    .unwrap();
    assert_ne!(advanced, original);
    // An independently advanced state cannot replace the captured source.
    equal_transition(
        space.transition(&original, branch, full).unwrap(),
        prepared.transition(i, full).unwrap(),
    );
    assert!(prepared
        .transition(prepared.branches().len(), full)
        .is_err());
    assert!(space
        .transition(
            &original,
            &CombatBranch::Input(ClientInput::PlayCard {
                card_index: 999,
                target: None
            }),
            full
        )
        .is_err());
}

#[test]
fn prepared_choices_preserve_potion_budget_and_terminal_membership() {
    let mut state = source();
    state.potion_uses = 1;
    let limited = CombatSearchSpace::with_potion_limit(1)
        .prepare_choices(&state)
        .unwrap();
    assert!(!limited
        .branches()
        .iter()
        .any(|b| matches!(b, CombatBranch::Input(ClientInput::UsePotion { .. }))));
    assert!(limited
        .branches()
        .iter()
        .any(|b| matches!(b, CombatBranch::Input(ClientInput::DiscardPotion(_)))));
    let mut final_hit = state.position().clone();
    final_hit.combat.entities.monsters[0].current_hp = 1;
    let ended = EngineCombatStepper.apply_to_stable(
        &final_hit,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(EngineCombatStepper.is_exact_victory(&ended.position));
    let terminal = CombatSearchState::new(ended.position).unwrap();
    assert!(CombatSearchSpace::default()
        .prepare_choices(&terminal)
        .unwrap()
        .branches()
        .is_empty());
}

#[test]
fn prepared_legality_does_not_compute_factorial_selection_weights() {
    // Stress the legal domain independently of normal hand limits. No gameplay
    // claim: the legacy distribution over all ordered completions overflows.
    let mut p = source().position().clone();
    p.combat.zones.hand = (1..=180)
        .map(|i| sts_core::runtime::combat::CombatCard::for_scenario(CardId::Defend, i))
        .collect();
    p.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages(1..=180),
        min_cards: 1,
        max_cards: 180,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let state = CombatSearchState::new(p).unwrap();
    let space = CombatSearchSpace::default();
    assert!(matches!(space.choices(&state), Err(error) if error.contains("numeric range")));
    let first = space.prepare_choices(&state).unwrap();
    let begin = first
        .branches()
        .iter()
        .position(|b| matches!(b, CombatBranch::BeginSelection(_)))
        .unwrap();
    let Transition::Settled { state, .. } = first
        .transition(
            begin,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        )
        .unwrap()
    else {
        panic!("selection assembly must settle");
    };
    assert_eq!(space.prepare_choices(&state).unwrap().branches().len(), 180);
}
