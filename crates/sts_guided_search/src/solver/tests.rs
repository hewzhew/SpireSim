use super::*;
use sts_core::content::{
    cards::CardId,
    monsters::EnemyId,
    potions::{Potion, PotionId},
};
use sts_core::runtime::combat::CombatCard;
use sts_core::state::core::{EngineState, HandSelectReason, PendingChoice};

pub(super) fn root(hand: &[CardId], hp: i32) -> CombatPosition {
    let mut combat = sts_core::test_support::blank_test_combat();
    let mut monster = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    monster.current_hp = hp;
    monster.max_hp = hp;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = hand
        .iter()
        .enumerate()
        .map(|(i, &id)| CombatCard::for_scenario(id, i as u32 + 1))
        .collect();
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

fn run(root: CombatPosition, work: usize) -> CombatSolver {
    let mut s = CombatSolver::new(
        root,
        SolverConfig {
            max_width: 16,
            ..Default::default()
        },
    )
    .unwrap();
    s.advance(work, Duration::from_secs(30)).unwrap();
    s
}

#[test]
fn display_prefers_recovered_gold_before_action_count_at_equal_hp() {
    use sts_core::state::rewards::RewardItem;
    let won = run(root(&[CardId::Strike], 6), 2000).plans[0].clone();
    let mut escaped = won.clone();
    escaped.outcome.combat_mugged = true;
    escaped.outcome.persistent.unrecovered_stolen_gold = 60;
    escaped.outcome.final_gold = 143;
    escaped.outcome.pending_rewards.clear();

    let mut recovered = won;
    recovered.inputs.push(ClientInput::EndTurn);
    recovered.outcome.final_gold = 143;
    recovered.outcome.pending_rewards = vec![
        RewardItem::StolenGold { amount: 60 },
        RewardItem::Gold { amount: 20 },
    ];
    let mut plans = vec![escaped.clone(), recovered.clone()];
    sort_plans_for_display(&mut plans);
    assert_eq!(plans.len(), 2, "display order cannot remove an alternative");
    assert!(!plans[0].outcome.combat_mugged);
    assert!(plans[0].inputs.len() > plans[1].inputs.len());

    // Normal gold rewards also precede action count; holding or claiming the
    // same amount must not change the order.
    escaped.outcome.combat_mugged = false;
    escaped.outcome.persistent.unrecovered_stolen_gold = 0;
    let mut plans = vec![escaped.clone(), recovered.clone()];
    sort_plans_for_display(&mut plans);
    assert_eq!(plans[0].outcome.pending_rewards.len(), 2);
    recovered.outcome.final_gold += 80;
    recovered.outcome.pending_rewards.clear();
    let mut plans = vec![escaped, recovered];
    sort_plans_for_display(&mut plans);
    assert_eq!(plans[0].outcome.final_gold, 223);
}

#[test]
fn repaired_wins_receive_charged_resumable_discard_cleanup() {
    let mut source = root(&[CardId::Strike], 6);
    source.combat.entities.potions = vec![
        Some(Potion::new(PotionId::DexterityPotion, 1)),
        Some(Potion::new(PotionId::PowerPotion, 2)),
    ];
    let inputs = vec![
        ClientInput::DiscardPotion(0),
        ClientInput::DiscardPotion(1),
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
    ];
    let expected = crate::shuffle_order::replay(&source, &inputs)
        .unwrap()
        .pop()
        .unwrap();
    let origin = SolverPlanOrigin {
        width: 4,
        card_only: false,
        recovery: false,
        observed_at_work: 0,
        pass_search_work: 0,
        shuffle: None,
        order_edit: None,
        order_family: None,
        potion_edit: Some(PotionEditOrigin {
            source_found_at_work: 0,
            source_inputs: inputs.clone(),
            input_index: 0,
            inserted: None,
        }),
    };
    let mut solver = CombatSolver::new(
        source.clone(),
        SolverConfig {
            refine_card_order: false,
            ..Default::default()
        },
    )
    .unwrap();
    solver.replay_refinement_candidate(inputs, expected, origin);
    for _ in 0..100 {
        let before = solver.stats.work();
        solver.advance(1, Duration::from_secs(1)).unwrap();
        assert!(solver.stats.work() <= before + 1);
        assert_eq!(
            solver.stats.search_transitions, 0,
            "cleanup must precede unrelated search"
        );
        if let Some(plan) = solver
            .plans()
            .iter()
            .find(|p| p.outcome.total_potion_expenditures == 0)
        {
            assert_eq!(plan.inputs.len(), 1);
            assert_eq!(
                plan.successor.combat.entities.potions,
                source.combat.entities.potions
            );
            assert_eq!(
                crate::shuffle_order::replay(&source, &plan.inputs)
                    .unwrap()
                    .last(),
                Some(&plan.successor)
            );
            assert!(solver.stats.plan_edit_work > 0 && solver.stats.replay_transitions > 3);
            return;
        }
    }
    panic!("repaired winner retained a removable discard");
}

#[test]
fn card_order_edits_rebind_instances_and_replay_an_earlier_kill() {
    let mut source = root(&[CardId::Strike, CardId::Bash], 17);
    source.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 20)].into();
    let limits = CombatStepLimits {
        max_engine_steps: 512,
        deadline: None,
    };
    let mut current = source.clone();
    let mut inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        ClientInput::EndTurn,
    ];
    for input in &inputs {
        assert!(EngineCombatStepper.is_legal_action(&current, input));
        current = EngineCombatStepper
            .apply_to_stable(&current, input.clone(), limits)
            .position;
    }
    let strike = current
        .combat
        .zones
        .hand
        .iter()
        .position(|c| c.id == CardId::Strike)
        .unwrap();
    let input = ClientInput::PlayCard {
        card_index: strike,
        target: Some(1),
    };
    current = EngineCombatStepper
        .apply_to_stable(&current, input.clone(), limits)
        .position;
    inputs.push(input);
    let outcome = materialize_replay_checked_combat_outcome_v1(&source, &inputs, &current).unwrap();
    assert!(outcome.final_hp < source.combat.entities.player.current_hp);
    let plan = SolverPlan {
        inputs,
        outcome,
        successor: current,
        found_at_work: 4,
        min_stable_hp: 1,
        origin: SolverPlanOrigin {
            width: 4,
            card_only: true,
            recovery: false,
            observed_at_work: 0,
            pass_search_work: 0,
            shuffle: None,
            potion_edit: None,
            order_edit: None,
            order_family: None,
        },
    };
    let mut job = refinement::PlanEditJob::new(source.clone(), plan, Some(0), false, true);
    let mut stats = SolverStats::default();
    let mut improved = false;
    let mut done = false;
    for _ in 0..50 {
        let work = stats.work();
        match job.advance(limits, &mut stats).unwrap() {
            refinement::Event::Win {
                inputs,
                expected,
                origin,
            } => {
                let edit = origin.order_edit.unwrap();
                assert_eq!((edit.start, edit.middle, edit.end), (0, 1, 2));
                assert_eq!(
                    inputs.len(),
                    2,
                    "the earlier kill truncates the old next turn"
                );
                assert_eq!(
                    inputs[0],
                    ClientInput::PlayCard {
                        card_index: 1,
                        target: Some(1)
                    }
                );
                let replay = crate::shuffle_order::replay(&source, &inputs).unwrap();
                assert_eq!(replay.last(), Some(&expected));
                assert!(EngineCombatStepper.is_exact_victory(&expected));
                assert_eq!(
                    expected.combat.entities.player.current_hp,
                    source.combat.entities.player.current_hp
                );
                improved = true;
            }
            refinement::Event::Done => {
                done = true;
                break;
            }
            refinement::Event::TimedOut => panic!("no deadline"),
            refinement::Event::Progress => {}
        }
        assert!(stats.work() <= work + 1);
    }
    assert!(done && improved);
    assert_eq!(
        stats.plan_edit_candidates, 1,
        "EndTurn must separate card swaps"
    );
}

#[test]
fn card_order_edits_keep_selection_with_its_card_and_reject_changed_domains() {
    let source = root(&[CardId::Armaments, CardId::Strike, CardId::Defend], 9);
    let limits = CombatStepLimits {
        max_engine_steps: 512,
        deadline: None,
    };
    let mut current = source.clone();
    let mut inputs = vec![ClientInput::PlayCard {
        card_index: 0,
        target: None,
    }];
    current = EngineCombatStepper
        .apply_to_stable(&current, inputs[0].clone(), limits)
        .position;
    let select = ClientInput::SubmitSelection(
        sts_core::state::selection::SelectionResolution::card_lineages(
            sts_core::state::selection::SelectionScope::Hand,
            [current.combat.zones.hand[0].lineage],
        ),
    );
    assert!(EngineCombatStepper.is_legal_action(&current, &select));
    current = EngineCombatStepper
        .apply_to_stable(&current, select.clone(), limits)
        .position;
    inputs.push(select);
    let strike = ClientInput::PlayCard {
        card_index: current
            .combat
            .zones
            .hand
            .iter()
            .position(|c| c.id == CardId::Strike)
            .unwrap(),
        target: Some(1),
    };
    assert!(EngineCombatStepper.is_legal_action(&current, &strike));
    current = EngineCombatStepper
        .apply_to_stable(&current, strike.clone(), limits)
        .position;
    inputs.push(strike);
    let outcome = materialize_replay_checked_combat_outcome_v1(&source, &inputs, &current).unwrap();
    let plan = SolverPlan {
        inputs,
        outcome,
        successor: current,
        found_at_work: 3,
        min_stable_hp: source.combat.entities.player.current_hp,
        origin: SolverPlanOrigin {
            width: 4,
            card_only: true,
            recovery: false,
            observed_at_work: 0,
            pass_search_work: 0,
            shuffle: None,
            potion_edit: None,
            order_edit: None,
            order_family: None,
        },
    };
    let mut job = refinement::PlanEditJob::new(source, plan, Some(0), false, true);
    let mut stats = SolverStats::default();
    let mut done = false;
    for _ in 0..30 {
        match job.advance(limits, &mut stats).unwrap() {
            refinement::Event::Done => {
                done = true;
                break;
            }
            refinement::Event::Win { .. } => {
                panic!("playing Strike first loses the upgrade target")
            }
            refinement::Event::TimedOut => panic!("no deadline"),
            refinement::Event::Progress => {}
        }
    }
    assert!(done);
    assert_eq!(
        stats.plan_edit_candidates, 1,
        "selection must move with Armaments, never as its own action"
    );
    assert_eq!(stats.plan_edit_rejected, 1);
}

#[test]
fn potion_edits_replay_insertions_removals_and_reject_changed_choices() {
    let mut source = root(&[CardId::Defend], 6);
    source.combat.entities.player.current_hp = 40;
    source.combat.entities.potions = vec![
        Some(Potion::new(PotionId::StrengthPotion, 1)),
        Some(Potion::new(PotionId::BlockPotion, 2)),
        Some(Potion::new(PotionId::ColorlessPotion, 3)),
    ];
    source.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 20)].into();
    let limits = CombatStepLimits {
        max_engine_steps: 512,
        deadline: None,
    };
    let mut current = source.clone();
    let mut inputs = vec![
        ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::EndTurn,
    ];
    for input in &inputs {
        current = EngineCombatStepper
            .apply_to_stable(&current, input.clone(), limits)
            .position;
    }
    let strike = current
        .combat
        .zones
        .hand
        .iter()
        .position(|c| c.id == CardId::Strike)
        .unwrap();
    let input = ClientInput::PlayCard {
        card_index: strike,
        target: Some(1),
    };
    current = EngineCombatStepper
        .apply_to_stable(&current, input.clone(), limits)
        .position;
    inputs.push(input);
    assert!(EngineCombatStepper.is_exact_victory(&current));
    let outcome = materialize_replay_checked_combat_outcome_v1(&source, &inputs, &current).unwrap();
    let source_hp = outcome.final_hp;
    assert!(source_hp < 40);
    let plan = SolverPlan {
        inputs,
        outcome,
        successor: current,
        found_at_work: 4,
        min_stable_hp: source_hp,
        origin: SolverPlanOrigin {
            width: 4,
            card_only: false,
            recovery: false,
            observed_at_work: 0,
            pass_search_work: 0,
            shuffle: None,
            potion_edit: None,
            order_edit: None,
            order_family: None,
        },
    };
    let mut job = refinement::PlanEditJob::new(source.clone(), plan, None, true, false);
    let mut stats = SolverStats::default();
    let mut removed = false;
    let mut blocked = false;
    let mut completed = false;
    for _ in 0..200 {
        let before = stats.work();
        let event = job.advance(limits, &mut stats).unwrap();
        assert!(stats.work() <= before + 1);
        match event {
            refinement::Event::Win {
                inputs,
                expected,
                origin,
            } => {
                let mut replay = source.clone();
                for input in &inputs {
                    assert!(EngineCombatStepper.is_legal_action(&replay, input));
                    replay = EngineCombatStepper
                        .apply_to_stable(&replay, input.clone(), limits)
                        .position;
                }
                assert_eq!(replay, expected);
                let outcome =
                    materialize_replay_checked_combat_outcome_v1(&source, &inputs, &replay)
                        .unwrap();
                let edit = origin.potion_edit.unwrap();
                removed |= edit.inserted.is_none()
                    && outcome.total_potion_expenditures == 0
                    && outcome.final_hp == source_hp;
                blocked |= edit.inserted
                    == Some(ClientInput::UsePotion {
                        potion_index: 1,
                        target: None,
                    })
                    && outcome.final_hp == 40;
                assert!(
                    !inputs.iter().any(|i| matches!(
                        i,
                        ClientInput::UsePotion {
                            potion_index: 2,
                            ..
                        }
                    )),
                    "a new discovery choice cannot inherit the old positional suffix"
                );
            }
            refinement::Event::Done => {
                completed = true;
                break;
            }
            refinement::Event::TimedOut => panic!("no deadline"),
            refinement::Event::Progress => {}
        }
    }
    assert!(completed && removed && blocked);
    assert!(stats.plan_edit_rejected > 0);
}

#[test]
fn draw_into_lethal_survives_an_immediate_enrage_penalty() {
    let mut source = root(
        &[
            CardId::BattleTrance,
            CardId::Defend,
            CardId::Defend,
            CardId::Strike,
            CardId::Strike,
        ],
        54,
    );
    source.combat.entities.player.current_hp = 1;
    source.combat.turn.energy = 2;
    let mut finisher = CombatCard::for_scenario(CardId::FiendFire, 103);
    sts_core::content::cards::upgrade_card_once_java(&mut finisher);
    source.combat.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::Wound, 101),
        CombatCard::for_scenario(CardId::Wound, 102),
        finisher,
    ]
    .into();
    // Test the mechanism, not a monster-name exception: this attacker has the
    // same persistent skill-triggered strength gain as Gremlin Nob.
    sts_core::content::powers::store::set_powers_for(
        &mut source.combat,
        1,
        vec![sts_core::runtime::combat::Power {
            power_type: sts_core::content::powers::PowerId::Anger,
            amount: 3,
            instance_id: None,
            extra_data: 0,
            payload: Default::default(),
            just_applied: false,
        }],
    );
    let turn = source.combat.turn.turn_count;
    let solved = run(source, 2_000);
    assert!(solved.plans().iter().any(|plan| {
        plan.successor.combat.turn.turn_count == turn && plan.outcome.final_hp > 0
            && plan.inputs.len() == 2
    }), "draw plus finisher must get a continuation before the incoming-damage heuristic rejects it");
}

#[test]
fn equivalent_starters_do_not_displace_distinct_plays_targets_or_consumables() {
    let mut source = root(
        &[
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::TwinStrike,
            CardId::TwinStrike,
        ],
        30,
    );
    source.combat.zones.hand[2].upgrades = 1;
    source.combat.zones.hand[5].free_to_play_once = true;
    // Only adjacent copies of the same instance are exact duplicates.
    source.combat.zones.hand[1] = source.combat.zones.hand[0].clone();
    source.combat.zones.hand[4] = source.combat.zones.hand[3].clone();
    let mut second = source.combat.entities.monsters[0].clone();
    second.id = 2;
    source.combat.entities.monsters.push(second);
    source.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 1))];
    let space = CombatSearchSpace::default();
    let state = CombatSearchState::new(source.clone()).unwrap();
    let mut pass = Pass::new(Node::root(state), 4, false);
    let mut stats = SolverStats::default();
    assert!(pass
        .prepare(&space, &SolverConfig::default(), &mut stats, None)
        .unwrap());
    let expansion = pass.expansion.as_ref().unwrap();
    let inputs = expansion
        .branches
        .iter()
        .map(|&i| expansion.choices.input(i).unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(stats.equivalent_actions_skipped, 3);
    for target in [Some(1), Some(2)] {
        for card_index in [0, 2, 6, 7] {
            assert!(inputs.contains(&ClientInput::PlayCard { card_index, target }));
        }
        assert!(!inputs.contains(&ClientInput::PlayCard {
            card_index: 1,
            target
        }));
        assert!(inputs.contains(&ClientInput::UsePotion {
            potion_index: 0,
            target
        }));
    }
    for card_index in [3, 5] {
        assert!(inputs.contains(&ClientInput::PlayCard {
            card_index,
            target: None
        }));
    }
    assert!(!inputs.contains(&ClientInput::PlayCard {
        card_index: 4,
        target: None
    }));
    assert!(inputs.contains(&ClientInput::EndTurn));
    assert!(inputs.contains(&ClientInput::DiscardPotion(0)));
    assert!(inputs
        .iter()
        .all(|i| EngineCombatStepper.is_legal_action(&source, i)));
}

#[test]
fn equivalent_starter_reduction_keeps_replayed_resource_alternatives() {
    let mut source = root(&[CardId::Strike, CardId::Strike, CardId::Strike], 18);
    source.combat.zones.hand[1] = source.combat.zones.hand[0].clone();
    source.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 1))];
    let mut reduced = CombatSolver::new(
        source.clone(),
        SolverConfig {
            max_width: 4,
            ..Default::default()
        },
    )
    .unwrap();
    let mut full = CombatSolver::new(
        source,
        SolverConfig {
            max_width: 4,
            collapse_equivalent_actions: false,
            ..Default::default()
        },
    )
    .unwrap();
    for solver in [&mut full, &mut reduced] {
        solver.advance(4000, Duration::from_secs(30)).unwrap();
        assert!(solver
            .plans()
            .iter()
            .any(|p| p.outcome.total_potion_expenditures == 0));
    }
    assert!(reduced.stats.equivalent_actions_skipped > 0);
    assert_eq!(full.stats.equivalent_actions_skipped, 0);
    for (a, b) in [(&full, &reduced), (&reduced, &full)] {
        for plan in a.plans() {
            assert!(b.plans().iter().any(|other| other
                .outcome
                .has_same_materialized_resources_v1(&plan.outcome)));
        }
    }
}

fn played_node(source: &CombatPosition, card_index: usize, target: Option<usize>) -> Node {
    let input = ClientInput::PlayCard { card_index, target };
    assert!(EngineCombatStepper.is_legal_action(source, &input));
    let step = EngineCombatStepper.apply_to_stable(
        source,
        input.clone(),
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(!step.truncated && !step.timed_out);
    Node {
        state: CombatSearchState::new(step.position).unwrap(),
        card_baseline: Arc::new(retention::CardBaseline::new(&source.combat)),
        potion_budget: Node::root(CombatSearchState::new(source.clone()).unwrap()).potion_budget,
        trace: Some(Arc::new(Trace {
            expenditures: CombatInputExpendituresV1::from_inputs(std::slice::from_ref(&input)),
            previous: None,
            input,
        })),
        input_count: 1,
    }
}

#[test]
fn preparation_credit_tracks_root_relative_persistent_card_gains() {
    let mut source = root(&[CardId::Apotheosis, CardId::Defend, CardId::BodySlam], 80);
    source.combat.zones.hand[0].upgrades = 1;
    let mut upgraded = CombatCard::for_scenario(CardId::Strike, 20);
    upgraded.upgrades = 1;
    source.combat.zones.draw_pile = vec![upgraded].into();
    let before = Node::root(CombatSearchState::new(source.clone()).unwrap());
    assert_eq!(retention::card_improvements(&before), 0);
    let after = played_node(&source, 0, None);
    assert!(retention::card_improvements(&after) > 0);
    assert!(retention::scores(&after)[2] > retention::scores(&before)[2]);
    for lane in [0, 1, 3] {
        assert!(retention::scores(&after)[lane] < retention::scores(&before)[lane]);
    }
    let defended_hp = |position: &CombatPosition, card_index| {
        crate::shuffle_order::replay(
            position,
            &[
                ClientInput::PlayCard {
                    card_index,
                    target: None,
                },
                ClientInput::EndTurn,
            ],
        )
        .unwrap()
        .last()
        .unwrap()
        .combat
        .entities
        .player
        .current_hp
    };
    let protection_gain = defended_hp(after.state.position(), 0) - defended_hp(&source, 1);
    assert_eq!(protection_gain, 3);
    assert_eq!(
        retention::scores(&after)[4] - retention::scores(&before)[4],
        i64::from(protection_gain) * 10,
        "upgrading a payable hand Defend changes its actual protection"
    );
    assert!(
        retention::evaluate(&after, None).durable > retention::evaluate(&before, None).durable,
        "persistent upgrades distinguish equally protected preparations"
    );

    let mut position = after.state.position().clone();
    // Rendering and one-turn discounts must not be mistaken for new persistent
    // improvements, and moving a card between cycling piles must preserve them.
    position.combat.zones.hand[0].base_block_mut = 100;
    position.combat.zones.hand[1].cost_for_turn = Some(0);
    let strike = position.combat.zones.draw_pile.iter_mut().next().unwrap();
    strike.cost_for_turn = Some(0);
    strike.base_damage_mut = 100;
    let card = position.combat.zones.hand.remove(0);
    position.combat.zones.discard_pile.push(card);
    let changed = Node {
        state: CombatSearchState::new(position.clone()).unwrap(),
        ..after.clone()
    };
    assert_eq!(
        retention::card_improvements(&changed),
        retention::card_improvements(&after)
    );
    let mut discounted = position.clone();
    discounted
        .combat
        .zones
        .draw_pile
        .iter_mut()
        .next()
        .unwrap()
        .cost_modifier = -1;
    let discounted = Node {
        state: CombatSearchState::new(discounted).unwrap(),
        ..after.clone()
    };
    assert!(retention::card_improvements(&discounted) > retention::card_improvements(&after));
    assert_eq!(
        retention::scores(&discounted)[4],
        retention::scores(&changed)[4],
        "a future card's persistent improvement cannot buy immediate protection"
    );
    position
        .combat
        .zones
        .exhaust_pile
        .extend(std::mem::take(&mut position.combat.zones.discard_pile));
    position
        .combat
        .zones
        .exhaust_pile
        .extend(std::mem::take(&mut position.combat.zones.hand));
    let exhausted = Node {
        state: CombatSearchState::new(position).unwrap(),
        ..after
    };
    assert_eq!(
        retention::card_improvements(&exhausted),
        0,
        "spent improvements and the already-upgraded root Strike earn no credit"
    );
}

#[test]
fn sampled_prefixes_replay_without_changing_subsequent_search() {
    let source = root(&[CardId::Strike, CardId::Defend, CardId::ShrugItOff], 40);
    let config = SolverConfig {
        max_width: 8,
        ..Default::default()
    };
    let mut sampled = CombatSolver::new(source.clone(), config.clone()).unwrap();
    let mut control = CombatSolver::new(source.clone(), config).unwrap();
    sampled.advance(400, Duration::from_secs(30)).unwrap();
    control.advance(400, Duration::from_secs(30)).unwrap();
    let before = serde_json::to_value(sampled.progress()).unwrap();
    let candidates = sampled.sample_prefixes(12);
    assert!(!candidates.is_empty());
    assert!(candidates.len() <= 12);
    assert_eq!(
        serde_json::to_value(&candidates).unwrap(),
        serde_json::to_value(sampled.sample_prefixes(12)).unwrap()
    );
    assert_eq!(serde_json::to_value(sampled.progress()).unwrap(), before);
    for candidate in &candidates {
        assert!(!candidate.inputs.is_empty());
        let task = crate::continuation::ValidatedContinuationTask::new(
            crate::continuation::ContinuationTaskSpec {
                source_producer: "unit-test solver".into(),
                group: "same root".into(),
                origin: source.clone(),
                prefix: candidate.inputs.clone(),
                requirement: crate::continuation::ResourceRequirement {
                    min_final_hp: 1,
                    preserve_starting_potion_slots: 0,
                },
            },
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        )
        .unwrap();
        assert_eq!(task.anchor(), &candidate.successor);
    }
    sampled.advance(600, Duration::from_secs(30)).unwrap();
    control.advance(600, Duration::from_secs(30)).unwrap();
    assert_eq!(
        serde_json::to_value(sampled.stats()).unwrap(),
        serde_json::to_value(control.stats()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(sampled.plans()).unwrap(),
        serde_json::to_value(control.plans()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(sampled.sample_prefixes(12)).unwrap(),
        serde_json::to_value(control.sample_prefixes(12)).unwrap()
    );
}

#[test]
fn prefix_sampling_skips_virtual_choices_and_deduplicates_only_input_chains() {
    let source = root(&[CardId::Strike, CardId::Strike], 30);
    let first = played_node(&source, 0, Some(1));
    let second = played_node(&source, 1, Some(1));
    let mut solver = CombatSolver::new(source, SolverConfig::default()).unwrap();
    // The same chain in several queues is one task; distinct instance choices
    // remain two tasks even when their immediate damage is equal.
    let pass = &mut solver.beams[0].passes[0];
    pass.next = vec![first.clone(), second];
    pass.ended = vec![first];
    let mut selected = root(&[CardId::Defend], 30);
    selected.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([1]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let virtual_state = CombatSearchSpace::default()
        .virtual_successor(
            &CombatSearchState::new(selected).unwrap(),
            &CombatBranch::BeginSelection(0),
        )
        .unwrap();
    let mut assembling = pass.next[0].clone();
    assembling.state = virtual_state;
    pass.pending
        .push_back(actions::PendingAction::new(assembling));
    assert!(solver.sample_prefixes(0).is_empty());
    assert_eq!(solver.sample_prefixes(1).len(), 1);
    let sampled = solver.sample_prefixes(20);
    assert_eq!(sampled.len(), 2);
    assert_ne!(sampled[0].inputs, sampled[1].inputs);
    assert!(sampled
        .iter()
        .all(|p| p.pool != SolverPrefixPool::PendingChoice));
}

#[test]
fn prefix_sampling_keeps_real_pending_choices_before_virtual_assembly() {
    let source = root(&[CardId::BurningPact, CardId::Strike, CardId::Defend], 30);
    let choice = played_node(&source, 0, None);
    assert!(matches!(
        choice.state.position().engine,
        EngineState::PendingChoice(_)
    ));
    let mut assembly = choice.clone();
    assembly.state = CombatSearchSpace::default()
        .virtual_successor(&choice.state, &CombatBranch::BeginSelection(0))
        .unwrap();
    let mut solver = CombatSolver::new(source.clone(), SolverConfig::default()).unwrap();
    solver.beams[0].passes[0].pending.extend(
        [assembly, choice.clone()]
            .into_iter()
            .map(actions::PendingAction::new),
    );
    let candidates = solver.sample_prefixes(8);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].successor, *choice.state.position());
    let input = &candidates[0].inputs[0];
    let replay = EngineCombatStepper.apply_to_stable(
        &source,
        input.clone(),
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(!replay.truncated && !replay.timed_out);
    assert_eq!(replay.position, candidates[0].successor);
}

#[test]
fn retention_partitions_without_erasing_exact_states() {
    let source = root(
        &[
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
        ],
        100,
    );
    let mut nodes = (0..3)
        .map(|i| played_node(&source, i, Some(1)))
        .collect::<Vec<_>>();
    let defense = played_node(&source, 3, None);
    nodes.push(defense.clone());
    let (kept, omitted) = retention::partition(
        nodes.clone(),
        2,
        &mut SolverStats::default(),
        false,
        false,
        None,
        None,
    );
    assert_eq!((kept.len(), omitted.len()), (2, 2));
    for original in &nodes {
        assert_eq!(
            kept.iter()
                .chain(&omitted)
                .filter(|n| n.state == original.state)
                .count(),
            1
        );
    }
    assert_ne!(omitted[0].state, omitted[1].state);
}

fn assert_retention_matches_linear_membership(pool: Vec<Node>) {
    for effects in [false, true] {
        // Reference contract: first complete-state occurrence wins. Its trace
        // and scheduling hint must survive, even if a later trace is shorter.
        let mut unique: Vec<Node> = vec![];
        let mut duplicates = 0;
        for node in &pool {
            if unique.iter().any(|n| n.state == node.state) {
                duplicates += 1;
            } else {
                unique.push(node.clone());
            }
        }
        for width in [1, 4, 16, 128] {
            let mut actual_stats = SolverStats::default();
            let mut expected_stats = SolverStats::default();
            let actual = retention::partition(
                pool.clone(),
                width,
                &mut actual_stats,
                effects,
                false,
                None,
                None,
            );
            let expected = retention::partition(
                unique.clone(),
                width,
                &mut expected_stats,
                effects,
                false,
                None,
                None,
            );
            assert_eq!(actual_stats.exact_duplicates, duplicates);
            assert_eq!(actual_stats.beam_dropped, expected_stats.beam_dropped);
            for (a, b) in [(actual.0, expected.0), (actual.1, expected.1)] {
                assert_eq!(a.len(), b.len());
                for (a, b) in a.iter().zip(&b) {
                    assert_eq!(a.state, b.state);
                    assert_eq!(a.inputs(), b.inputs());
                    assert_eq!(a.input_count, b.input_count);
                }
            }
        }
    }
}

#[test]
fn indexed_retention_checks_collisions_and_preserves_first_occurrence() {
    let source = root(&[CardId::Strike, CardId::Defend], 30);
    let mut pool = vec![];
    for i in 0..48 {
        let mut node = played_node(&source, 1, None);
        let mut position = node.state.position().clone();
        // These fields are intentionally absent from the partial lookup key.
        position.combat.entities.monsters[0].current_hp -= i % 3;
        position.combat.entities.player.max_hp += i % 4;
        node.state = CombatSearchState::new(position).unwrap();
        if i >= 24 {
            node.trace = None;
            node.input_count = 0;
        }
        pool.push(node);
    }
    assert!(pool.iter().all(|n| n.index_key() == pool[0].index_key()));
    assert_ne!(pool[0].state, pool[1].state);
    assert_retention_matches_linear_membership(pool);
}

#[test]
fn indexed_retention_preserves_selection_assembly() {
    let mut source = root(&[CardId::Strike, CardId::Defend], 30);
    source.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2]),
        min_cards: 1,
        max_cards: 2,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let state = CombatSearchState::new(source).unwrap();
    let space = CombatSearchSpace::default();
    let prefix = space
        .virtual_successor(&state, &CombatBranch::BeginSelection(0))
        .unwrap();
    let mut pool = vec![];
    for order in [vec![], vec![0], vec![1], vec![0, 1], vec![1, 0]] {
        let mut selected = prefix.clone();
        for member in order {
            selected = space
                .virtual_successor(&selected, &CombatBranch::SelectMember(member))
                .unwrap();
        }
        for _ in 0..2 {
            pool.push(Node::root(selected.clone()));
        }
    }
    assert_retention_matches_linear_membership(pool);
}

#[test]
fn retain_order_reduction_preserves_every_member_set_and_ordered_reasons() {
    fn enumerate(source: CombatPosition, collapse: bool) -> (Vec<CombatSearchState>, SolverStats) {
        let space = CombatSearchSpace::default();
        let config = SolverConfig {
            collapse_equivalent_actions: collapse,
            ..Default::default()
        };
        let first = Node::root(CombatSearchState::new(source).unwrap());
        let mut pass = Pass::new(first.clone(), 4, true);
        let mut stack = vec![first];
        let mut results = vec![];
        let mut stats = SolverStats::default();
        while let Some(node) = stack.pop() {
            let expansion = pass
                .prepare_node(node, &space, &config, &mut stats)
                .unwrap();
            for branch in expansion.branches {
                let input = expansion.choices.input(branch).unwrap();
                let Transition::Settled { state, .. } = expansion
                    .choices
                    .transition(
                        branch,
                        CombatStepLimits {
                            max_engine_steps: 512,
                            deadline: None,
                        },
                    )
                    .unwrap()
                else {
                    panic!("interrupted")
                };
                if input.is_some() {
                    results.push(state);
                } else {
                    stack.push(Node::root(state));
                }
            }
        }
        (results, stats)
    }
    for (reason, min_cards) in [
        (HandSelectReason::Retain, 0),
        (HandSelectReason::Retain, 2),
        (HandSelectReason::Exhaust, 0),
        (HandSelectReason::Discard, 0),
        (HandSelectReason::PutOnDrawPile, 0),
    ] {
        let mut source = root(
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Wound,
            ],
            100,
        );
        source.combat.zones.hand[1].upgrades = 1;
        source.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: source.combat.zones.hand.iter().map(|c| c.lineage).collect(),
            min_cards,
            max_cards: 2,
            can_cancel: false,
            reason,
        });
        let (original, _) = enumerate(source.clone(), false);
        let (reduced, stats) = enumerate(source, true);
        assert_eq!(original.len(), if min_cards == 0 { 17 } else { 12 });
        if reason == HandSelectReason::Retain {
            assert_eq!(
                reduced.len(),
                if min_cards == 0 { 11 } else { 6 },
                "all empty/single/pair sets, with no repeated permutations"
            );
            assert!(stats.equivalent_actions_skipped > 0);
        } else {
            assert_eq!(reduced.len(), original.len());
            assert_eq!(stats.equivalent_actions_skipped, 0);
        }
        assert!(original.iter().all(|state| reduced.contains(state)));
        assert!(reduced.iter().all(|state| original.contains(state)));
    }
}

#[test]
fn retain_order_reduction_is_charged_resumable_and_replay_exact() {
    let mut source = root(&[CardId::Strike, CardId::Defend, CardId::Wound], 6);
    source.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: source.combat.zones.hand.iter().map(|c| c.lineage).collect(),
        min_cards: 1,
        max_cards: 2,
        can_cancel: false,
        reason: HandSelectReason::Retain,
    });
    let config = SolverConfig {
        max_width: 4,
        max_potion_uses: Some(0),
        ..Default::default()
    };
    let mut whole = CombatSolver::new(source.clone(), config.clone()).unwrap();
    let mut chunks = CombatSolver::new(source.clone(), config).unwrap();
    whole.advance(1000, Duration::from_secs(30)).unwrap();
    for _ in 0..1000 {
        let before = chunks.stats.work();
        chunks.advance(1, Duration::from_secs(30)).unwrap();
        assert!(chunks.stats.work() <= before + 1);
    }
    assert!(!whole.plans.is_empty());
    assert!(whole.stats.equivalent_actions_skipped > 0);
    assert_eq!(
        serde_json::to_value(&whole.stats).unwrap(),
        serde_json::to_value(&chunks.stats).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&whole.plans).unwrap(),
        serde_json::to_value(&chunks.plans).unwrap()
    );
    for plan in whole.plans() {
        assert_eq!(
            crate::shuffle_order::replay(&source, &plan.inputs)
                .unwrap()
                .last(),
            Some(&plan.successor)
        );
    }
}

#[test]
fn deferred_node_resumes_its_exact_prefix_and_replays_from_original_root() {
    let source = root(&[CardId::Strike, CardId::Defend], 6);
    let saved = played_node(&source, 1, None);
    let mut solver = CombatSolver::new(
        source.clone(),
        SolverConfig {
            initial_width: 4,
            max_width: 4,
            // Check the recovered prefix before optional order polishing can
            // shorten its verified win by moving the lethal Strike first.
            refine_card_order: false,
            ..Default::default()
        },
    )
    .unwrap();
    let pass = &mut solver.beams[0].passes[0];
    pass.active.clear();
    pass.defer([saved.clone()], &mut solver.stats);
    solver.advance(1, Duration::from_secs(30)).unwrap();
    let pass = &solver.beams[0].passes[0];
    assert_eq!(solver.stats.resumed_nodes, 1);
    assert_eq!(pass.expansion.as_ref().unwrap().node.state, saved.state);
    assert_eq!(
        pass.expansion.as_ref().unwrap().node.inputs(),
        saved.inputs()
    );
    solver.advance(100, Duration::from_secs(30)).unwrap();
    let plan = &solver.plans()[0];
    assert_eq!(plan.inputs[0], saved.inputs()[0]);
    assert!(plan.origin.recovery);
    assert!(EngineCombatStepper.is_exact_victory(&plan.successor));
    assert_eq!(solver.root(), &source);
    assert!(solver.stats.replay_transitions >= plan.inputs.len());
}

#[test]
fn deferred_storage_is_bounded_and_preserves_partial_selection() {
    let mut source = root(&[CardId::Strike, CardId::Defend], 6);
    source.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let state = CombatSearchState::new(source).unwrap();
    let space = CombatSearchSpace::default();
    let selection = space
        .virtual_successor(&state, &CombatBranch::BeginSelection(0))
        .unwrap();
    let selection = space
        .virtual_successor(&selection, &CombatBranch::SelectMember(1))
        .unwrap();
    let saved = Node::root(selection.clone());
    let mut pass = Pass::new(Node::root(state), 1, false);
    pass.active.clear();
    let mut stats = SolverStats::default();
    pass.defer([saved.clone(), saved.clone()], &mut stats);
    assert_eq!(
        pass.deferred.len(),
        1,
        "complete-state duplicates do not consume reserve slots"
    );
    for hp in 10..30 {
        let mut r = root(&[CardId::Strike], hp);
        r.combat.entities.player.current_hp = hp;
        pass.defer([Node::root(CombatSearchState::new(r).unwrap())], &mut stats);
    }
    assert_eq!(pass.deferred.len(), 8);
    assert_eq!(stats.deferred_limit_dropped, 13);
    let config = SolverConfig {
        initial_width: 1,
        max_width: 1,
        ..Default::default()
    };
    assert!(pass.prepare(&space, &config, &mut stats, None).unwrap());
    assert_eq!(pass.expansion.as_ref().unwrap().node.state, selection);
    assert_eq!(
        pass.expansion
            .as_ref()
            .unwrap()
            .node
            .state
            .selected_members(),
        Some(&[1][..])
    );
    assert_eq!(pass.deferred.len(), 7);
    assert_eq!(stats.resumed_nodes, 1);
}

#[test]
fn reserve_index_checks_complete_states_when_keys_collide() {
    let source = root(&[CardId::Strike], 20);
    let make = |position| Node::root(CombatSearchState::new(position).unwrap());
    let first = make(source.clone());
    let mut changed = source;
    changed.combat.entities.monsters[0].current_hp = 19;
    let second = make(changed);
    assert_eq!(first.index_key(), second.index_key());
    assert_ne!(first.state, second.state);
    let mut pass = Pass::new(first.clone(), 4, false);
    let mut stats = SolverStats::default();
    pass.defer([first.clone(), second.clone(), first.clone()], &mut stats);
    assert_eq!(pass.deferred.len(), 2);
    assert_eq!(pass.pop_deferred().unwrap().state, first.state);
    assert_eq!(pass.deferred_index.values().map(Vec::len).sum::<usize>(), 1);
    assert_eq!(pass.pop_deferred().unwrap().state, second.state);
    assert!(pass.deferred_index.is_empty());
    pass.defer([first], &mut stats);
    assert_eq!(
        pass.deferred.len(),
        1,
        "a removed index entry cannot suppress a later continuation"
    );
}

#[test]
fn complete_results_keep_exact_potion_reserves_and_replay() {
    let mut r = root(&[], 5);
    r.combat.entities.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 1)),
        Some(Potion::new(PotionId::ExplosivePotion, 2)),
    ];
    let s = run(r.clone(), 2000);
    assert_eq!(s.root(), &r);
    assert!(s
        .plans()
        .iter()
        .any(|p| p.outcome.starting_potion_expenditure_slots == 1));
    assert!(s
        .plans()
        .iter()
        .any(|p| p.outcome.starting_potion_expenditure_slots == 2));
    for p in s.plans() {
        assert!(EngineCombatStepper.is_exact_victory(&p.successor));
        assert_eq!(p.inputs.len(), 1);
        assert_eq!(p.outcome.final_hp, r.combat.entities.player.current_hp);
    }
}

#[test]
fn winning_is_not_a_stop_condition_and_wider_work_keeps_better_hp() {
    let r = root(&[CardId::Offering, CardId::Strike], 6);
    let initial_hp = r.combat.entities.player.current_hp;
    let s = run(r, 1000);
    assert!(s.stats.observed_wins > 1);
    assert!(s.stats.inferior_wins_skipped > 0);
    assert!(s.stats.replayed_wins < s.stats.observed_wins);
    assert!(s.stats.search_transitions > s.plans[0].found_at_work);
    assert_eq!(s.plans[0].outcome.final_hp, initial_hp);
    assert_eq!(s.plans[0].inputs.len(), 1);
}

#[test]
fn retained_trace_expenditures_match_materialized_inputs_across_resumption() {
    let mut r = root(&[CardId::Strike, CardId::Strike, CardId::Defend], 25);
    r.combat.entities.potions = vec![
        Some(Potion::new(PotionId::ColorlessPotion, 1)),
        Some(Potion::new(PotionId::FirePotion, 2)),
    ];
    let mut solver = CombatSolver::new(
        r,
        SolverConfig {
            max_width: 8,
            ..Default::default()
        },
    )
    .unwrap();
    let mut seen = [false; 3];
    for _ in 0..40 {
        solver.advance(25, Duration::from_secs(30)).unwrap();
        for pass in solver.beams.iter().flat_map(|beam| &beam.passes) {
            for node in pass
                .active
                .iter()
                .chain(&pass.next)
                .chain(pass.pending_nodes())
                .chain(&pass.ended)
                .chain(&pass.deferred)
                .chain(pass.expansion.iter().map(|e| &e.node))
            {
                let inputs = node.inputs();
                assert_eq!(
                    node.expenditures(),
                    CombatInputExpendituresV1::from_inputs(&inputs)
                );
                assert_eq!(node.input_count, inputs.len());
                for input in inputs {
                    match input {
                        ClientInput::DiscardPotion(_) => seen[0] = true,
                        ClientInput::UsePotion { .. } => seen[1] = true,
                        ClientInput::SubmitDiscoverChoice(_) => seen[2] = true,
                        _ => {}
                    }
                }
            }
        }
    }
    assert_eq!(
        seen, [true; 3],
        "exercise discard, use and resolved selection prefixes"
    );
}

#[test]
fn plan_origin_survives_paused_replay_and_progress_reads_do_not_advance() {
    let source = root(&[CardId::Strike, CardId::Defend], 6);
    let mut solver = CombatSolver::new(source, SolverConfig::default()).unwrap();
    for _ in 0..20 {
        solver.advance(1, Duration::from_secs(30)).unwrap();
        if solver.replay.is_some() {
            break;
        }
    }
    let replay = solver
        .replay
        .as_ref()
        .expect("a selected endpoint awaits replay");
    let expected = serde_json::to_value(&replay.origin).unwrap();
    let origin_work = replay.origin.observed_at_work;
    let remaining = replay.inputs.len() - replay.index;
    assert_eq!(replay.index, 0);
    assert_eq!(origin_work, solver.stats.work());
    assert!(
        solver.plans().is_empty(),
        "observation is not a published plan"
    );
    let before = serde_json::to_value(solver.stats()).unwrap();
    let progress = serde_json::to_value(solver.progress()).unwrap();
    assert_eq!(progress, serde_json::to_value(solver.progress()).unwrap());
    assert_eq!(before, serde_json::to_value(solver.stats()).unwrap());
    solver.advance(remaining, Duration::from_secs(30)).unwrap();
    let plan = &solver.plans()[0];
    assert_eq!(serde_json::to_value(&plan.origin).unwrap(), expected);
    assert_eq!(plan.found_at_work, origin_work + plan.inputs.len());
    assert!(EngineCombatStepper.is_exact_victory(&plan.successor));
}

#[test]
fn budget_chunks_resume_expansion_and_replay_identically() {
    let mut r = root(&[CardId::Strike, CardId::Strike, CardId::Defend], 25);
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 1))];
    let config = SolverConfig {
        max_width: 8,
        preserve_card_compositions: true,
        ..Default::default()
    };
    let mut whole = CombatSolver::new(r.clone(), config.clone()).unwrap();
    whole.advance(1000, Duration::from_secs(30)).unwrap();
    let mut chunks = CombatSolver::new(r, config).unwrap();
    for _ in 0..1000 {
        chunks.advance(1, Duration::from_secs(30)).unwrap();
    }
    assert_eq!(
        serde_json::to_value(whole.stats()).unwrap(),
        serde_json::to_value(chunks.stats()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(whole.plans()).unwrap(),
        serde_json::to_value(chunks.plans()).unwrap()
    );
    assert!(chunks.stats.card_only_transitions > 0);
    assert!(chunks.stats.card_only_transitions < chunks.stats.search_transitions);
    assert!(chunks.stats.plan_edit_work > 0);
    assert!(chunks.stats.work() <= 1000);
    let progress = chunks.progress();
    assert_eq!(
        progress.iter().map(|p| p.search_work).sum::<usize>(),
        chunks.stats.search_transitions
    );
    assert_eq!(
        progress
            .iter()
            .filter(|p| p.card_only)
            .map(|p| p.search_work)
            .sum::<usize>(),
        chunks.stats.card_only_transitions
    );
    assert_eq!(
        serde_json::to_value(progress).unwrap(),
        serde_json::to_value(whole.progress()).unwrap()
    );
    let before = chunks.stats().work();
    chunks.advance(500, Duration::ZERO).unwrap();
    assert_eq!(before, chunks.stats().work());
}

#[test]
fn ordered_selection_completes_and_only_materialized_inputs_are_replayed() {
    let mut r = root(&[CardId::Strike, CardId::Defend], 6);
    r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2]),
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let s = run(r.clone(), 1000);
    assert!(!s.plans.is_empty());
    assert!(matches!(
        s.plans[0].inputs[0],
        ClientInput::SubmitSelection(_)
    ));
    assert_eq!(s.plans[0].inputs.len(), 2);
    let mut watched = CombatSolver::new(r, s.config.clone()).unwrap();
    watched
        .audit_reference(&s.plans[0].inputs, &s.plans[0].successor)
        .unwrap();
    watched.advance(1000, Duration::from_secs(30)).unwrap();
    assert_eq!(
        serde_json::to_value(s.plans()).unwrap(),
        serde_json::to_value(watched.plans()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(s.stats()).unwrap(),
        serde_json::to_value(watched.stats()).unwrap()
    );
    let audit = watched.audit().unwrap();
    assert!(audit
        .reference
        .iter()
        .any(|s| matches!(s.branch, Some(CombatBranch::SelectMember(_)))));
    let selected = audit
        .reference
        .iter()
        .find(|s| matches!(s.branch, Some(CombatBranch::SelectMember(_))))
        .unwrap();
    let ClientInput::SubmitSelection(input) = &s.plans[0].inputs[0] else {
        unreachable!()
    };
    assert_eq!(selected.inputs_completed, 0);
    assert!(matches!(
        selected.selection.as_ref().unwrap().selected.as_slice(),
        [crate::selection::SelectionMember::Card(lineage)]
            if input.selected_card_lineages() == [*lineage]
    ));
    assert!(audit.passes.iter().any(|p| p
        .work
        .pending_choice_work
        .get("hand/exhaust")
        .copied()
        .unwrap_or(0)
        > 0));
}

#[test]
fn forced_boundaries_are_charged_before_scoring_but_optional_actions_remain_decisions() {
    for max_cards in [1, 2] {
        let mut r = root(&[CardId::Strike, CardId::Defend], 6);
        r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: sts_core::test_support::scenario_lineages([1, 2]),
            min_cards: 1,
            max_cards,
            can_cancel: false,
            reason: HandSelectReason::Exhaust,
        });
        let config = SolverConfig {
            initial_width: 1,
            max_width: 1,
            ..Default::default()
        };
        let mut solver = CombatSolver::new(r.clone(), config.clone()).unwrap();
        solver.advance(1, Duration::from_secs(30)).unwrap();
        assert_eq!(solver.stats.work(), 1);
        let pass = &solver.beams[0].passes[0];
        assert!(pass.next.is_empty(), "BeginSelection is internal assembly");
        let started = pass.pending_nodes().next().unwrap();
        assert_eq!(started.state.selected_members(), Some(&[][..]));
        assert_eq!(started.state.position(), &r);
        assert!(started.inputs().is_empty());
        solver.advance(1, Duration::from_secs(30)).unwrap();
        assert_eq!(solver.stats.work(), 2);
        let pass = &solver.beams[0].passes[0];
        if max_cards == 2 {
            let partial = pass.pending_nodes().next().unwrap();
            assert_eq!(partial.state.selected_members(), Some(&[0][..]));
            let choices = solver.space.prepare_choices(&partial.state).unwrap();
            assert_eq!(
                choices.branches(),
                &[CombatBranch::FinishSelection, CombatBranch::SelectMember(1)]
            );
        }
        assert!(
            pass.next.is_empty(),
            "virtual assembly must bypass scalar retention"
        );
        assert_eq!(
            pass.pending.len() + usize::from(pass.resolving.is_some()),
            1
        );
        let selected = pass.pending_nodes().next().unwrap();
        assert_eq!(selected.state.selected_members(), Some(&[0][..]));
        assert_eq!(selected.state.position(), &r);
        assert!(selected.inputs().is_empty());
        assert_eq!(
            solver.advance(1, Duration::ZERO).unwrap(),
            SolverStop::TimeBudget
        );
        assert_eq!(solver.stats.work(), 2, "deadline cannot submit for free");
        solver.advance(1, Duration::from_secs(30)).unwrap();
        assert_eq!(solver.stats.work(), 3);
        let pass = &solver.beams[0].passes[0];
        let completed = pass
            .next
            .iter()
            .chain(pass.pending_nodes())
            .find(|node| node.input_count == 1 && node.state.selected_members().is_none())
            .expect("the completed choice remains available, including in a narrow-action cursor");
        assert_eq!(completed.state.selected_members(), None);
        assert_eq!(
            completed.state.position().combat.zones.hand[0].id,
            CardId::Defend
        );
        assert_eq!(
            completed.state.position().combat.zones.exhaust_pile[0].lineage,
            r.combat.zones.hand[0].lineage
        );
        assert_eq!(completed.inputs().len(), 1);
        let choices = solver.space.prepare_choices(&completed.state).unwrap();
        assert_eq!(choices.branches().len(), 2);
        assert!(choices
            .branches()
            .iter()
            .any(|branch| matches!(branch, CombatBranch::Input(ClientInput::EndTurn))));
        assert!(choices
            .branches()
            .iter()
            .any(|branch| matches!(branch, CombatBranch::Input(ClientInput::PlayCard { .. }))));
        assert_eq!(
            crate::shuffle_order::replay(&r, &completed.inputs())
                .unwrap()
                .last(),
            Some(completed.state.position())
        );

        // Both the remaining sibling choice and ordinary work survive pauses.
        let mut whole = CombatSolver::new(r.clone(), config.clone()).unwrap();
        whole.advance(128, Duration::from_secs(30)).unwrap();
        let mut chunks = CombatSolver::new(r, config).unwrap();
        for _ in 0..128 {
            let before = chunks.stats.work();
            chunks.advance(1, Duration::from_secs(30)).unwrap();
            assert!(chunks.stats.work() <= before + 1);
        }
        assert_eq!(
            serde_json::to_value(whole.stats()).unwrap(),
            serde_json::to_value(chunks.stats()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(whole.progress()).unwrap(),
            serde_json::to_value(chunks.progress()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(whole.plans()).unwrap(),
            serde_json::to_value(chunks.plans()).unwrap()
        );
    }

    // Completing the first phase is a transient score boundary when the only
    // admitted continuation is EndTurn. That input is real work, keeps its
    // original trace, and reaches the normal next-turn pool after rebirth.
    let mut r = root(&[CardId::Strike], 1);
    r.combat.entities.monsters[0] =
        sts_core::test_support::planned_monster(EnemyId::AwakenedOne, 1);
    r.combat.entities.monsters[0].id = 1;
    r.combat.entities.monsters[0].current_hp = 1;
    r.combat.entities.monsters[0].max_hp = 320;
    sts_core::content::powers::store::set_powers_for(
        &mut r.combat,
        1,
        vec![sts_core::runtime::combat::Power {
            power_type: sts_core::content::powers::PowerId::Unawakened,
            amount: -1,
            instance_id: None,
            extra_data: 0,
            payload: Default::default(),
            just_applied: false,
        }],
    );
    r.combat.turn.energy = 1;
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    let config = SolverConfig {
        initial_width: 1,
        max_width: 1,
        ..Default::default()
    };
    let mut solver = CombatSolver::new(r.clone(), config.clone()).unwrap();
    let pass = &mut solver.beams[0].passes[0];
    pass.active.clear();
    let mut expansion = pass
        .prepare_node(
            Node::root(CombatSearchState::new(r.clone()).unwrap()),
            &solver.space,
            &config,
            &mut solver.stats,
        )
        .unwrap();
    expansion.branches.retain(|&i| {
        matches!(
            expansion.choices.branches()[i],
            CombatBranch::Input(ClientInput::PlayCard { card_index: 0, .. })
        )
    });
    assert_eq!(expansion.branches.len(), 1);
    pass.expansion = Some(expansion);
    solver.advance(1, Duration::from_secs(30)).unwrap();
    assert_eq!(solver.stats.work(), 1);
    let pass = &solver.beams[0].passes[0];
    assert!(pass.next.is_empty() && pass.ended.is_empty());
    assert_eq!(pass.pending.len(), 1);
    let boundary = pass.pending_nodes().next().unwrap().clone();
    assert_eq!(boundary.state.position().combat.turn.energy, 0);
    assert!(boundary.state.position().combat.entities.monsters[0].half_dead);
    assert_eq!(boundary.inputs().len(), 1);
    assert_eq!(
        solver.advance(1, Duration::ZERO).unwrap(),
        SolverStop::TimeBudget
    );
    assert_eq!(solver.stats.work(), 1, "forced EndTurn cannot run for free");
    solver.advance(1, Duration::from_secs(30)).unwrap();
    assert_eq!(solver.stats.work(), 2);
    let next = &solver.beams[0].passes[0].ended[0];
    assert_eq!(next.inputs().last(), Some(&ClientInput::EndTurn));
    assert!(next.state.position().combat.turn.turn_count > r.combat.turn.turn_count);
    assert!(!next.state.position().combat.entities.monsters[0].half_dead);
    assert_eq!(
        next.state.position().combat.entities.monsters[0].current_hp,
        320
    );
    assert_eq!(
        crate::shuffle_order::replay(&r, &next.inputs())
            .unwrap()
            .last(),
        Some(next.state.position())
    );

    let space = CombatSearchSpace::default();
    assert!(actions::forced_end_turn(&boundary, &space, false, None).unwrap());
    let mut optional = boundary.state.position().clone();
    optional.combat.entities.potions = vec![Some(Potion::new(PotionId::SwiftPotion, 9))];
    let optional = Node::root(CombatSearchState::new(optional).unwrap());
    assert!(!actions::forced_end_turn(&optional, &space, false, None).unwrap());
    assert!(actions::forced_end_turn(&optional, &space, true, None).unwrap());
    assert!(actions::forced_end_turn(&optional, &space, false, Some(0)).unwrap());
    let mut still_playable = boundary.state.position().clone();
    let mut defend = CombatCard::for_scenario(CardId::Defend, 102);
    defend.cost_for_turn = Some(0);
    still_playable.combat.zones.hand = vec![defend];
    let still_playable = Node::root(CombatSearchState::new(still_playable).unwrap());
    assert!(EngineCombatStepper.is_legal_action(
        still_playable.state.position(),
        &ClientInput::PlayCard {
            card_index: 0,
            target: None
        }
    ));
    assert!(!actions::forced_end_turn(&still_playable, &space, true, None).unwrap());
    let mut live_enemy = boundary.state.position().clone();
    let mut other = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    other.id = 2;
    live_enemy.combat.entities.monsters.push(other);
    let live_enemy = Node::root(CombatSearchState::new(live_enemy).unwrap());
    assert!(!actions::forced_end_turn(&live_enemy, &space, true, None).unwrap());
}

#[test]
fn smoke_escape_and_budget_exhaustion_are_not_victories() {
    let mut r = root(&[], 50);
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 1))];
    let s = run(r, 500);
    assert!(s.plans.is_empty());
    assert!(s.stats.observed_losses > 0);
    assert!(s.stats.observed_escapes > 0);
}

#[test]
fn passive_fairy_consumption_cannot_bypass_the_resource_limit() {
    let mut r = root(&[], 6);
    r.combat.entities.player.current_hp = 1;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    let unrestricted = run(r.clone(), 1000);
    assert!(!unrestricted.plans.is_empty());
    let mut limited = CombatSolver::new(
        r,
        SolverConfig {
            max_potion_uses: Some(0),
            max_width: 16,
            ..Default::default()
        },
    )
    .unwrap();
    limited.advance(1000, Duration::from_secs(30)).unwrap();
    assert!(limited.plans.is_empty());
    assert!(limited.stats.resource_limit_states_skipped > 0);
    assert_eq!(limited.stats.resource_limit_wins_skipped, 0);
    assert_eq!(
        limited.stats.observed_wins, 0,
        "do not finish forbidden-Fairy continuations"
    );
}

#[test]
fn audit_observes_without_guiding_and_explicit_resource_limits_prune_before_search() {
    let mut source = root(&[CardId::Strike, CardId::Strike, CardId::Defend], 26);
    source.combat.entities.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 1)),
        Some(Potion::new(PotionId::FirePotion, 2)),
    ];
    let config = SolverConfig {
        max_width: 8,
        max_potion_uses: Some(1),
        ..Default::default()
    };
    let mut control = CombatSolver::new(source.clone(), config.clone()).unwrap();
    control.advance(4000, Duration::from_secs(30)).unwrap();
    let witness = control.plans()[0].clone();
    let mut watched = CombatSolver::new(source.clone(), config.clone()).unwrap();
    let mut wrong = witness.successor.clone();
    wrong.combat.entities.player.current_hp -= 1;
    assert!(watched.audit_reference(&witness.inputs, &wrong).is_err());
    assert!(watched.audit().is_none());
    watched
        .audit_reference(&witness.inputs, &witness.successor)
        .unwrap();
    for _ in 0..160 {
        watched.advance(25, Duration::from_secs(30)).unwrap();
        let before = serde_json::to_value(watched.audit()).unwrap();
        assert_eq!(before, serde_json::to_value(watched.audit()).unwrap());
        for pass in watched.beams.iter().flat_map(|b| &b.passes) {
            for node in pass
                .active
                .iter()
                .chain(&pass.next)
                .chain(pass.pending_nodes())
                .chain(&pass.ended)
                .chain(&pass.deferred)
                .chain(pass.expansion.iter().map(|e| &e.node))
            {
                assert!(node.expenditures().explicit_count() <= 1);
                assert!(!node.exceeds_potion_budget());
            }
        }
    }
    assert_eq!(
        serde_json::to_value(control.plans()).unwrap(),
        serde_json::to_value(watched.plans()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(control.stats()).unwrap(),
        serde_json::to_value(watched.stats()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(control.progress()).unwrap(),
        serde_json::to_value(watched.progress()).unwrap()
    );
    let audit = watched.audit().unwrap();
    assert_eq!(
        audit
            .passes
            .iter()
            .map(|p| p.work.transitions)
            .sum::<usize>(),
        watched.stats.search_transitions
    );
    assert!(
        audit.passes.iter().any(|p| p.work.discard_potion > 0),
        "a first discard is still admissible"
    );
    assert!(audit
        .passes
        .iter()
        .all(|p| p.work.from_over_explicit_limit == 0));
    assert!(watched.stats.resource_limit_actions_skipped > 0);

    // An audit must reject a supplied line outside the same resource contract.
    let inputs = vec![
        ClientInput::DiscardPotion(1),
        ClientInput::UsePotion {
            potion_index: 0,
            target: Some(1),
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
    ];
    let expected = crate::shuffle_order::replay(&source, &inputs)
        .unwrap()
        .pop()
        .unwrap();
    let mut constrained = CombatSolver::new(source, config).unwrap();
    assert!(constrained
        .audit_reference(&inputs, &expected)
        .unwrap_err()
        .contains("expenditure constraint"));
    assert!(constrained.audit().is_none());
}

#[test]
fn turn_quota_reaches_later_turns_before_exhausting_combinations() {
    let r = root(&[CardId::Anger; 10], 999);
    let mut s = CombatSolver::new(
        r,
        SolverConfig {
            transitions_per_width: 2,
            max_width: 4,
            ..Default::default()
        },
    )
    .unwrap();
    s.advance(300, Duration::from_secs(30)).unwrap();
    assert!(s.stats.turn_budget_dropped > 0);
    assert!(s.stats.deepest_turn >= 3);
}

#[test]
fn quota_audit_observes_source_turns_without_changing_search() {
    let source = root(&[CardId::Anger; 10], 60);
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1)
        };
        10
    ];
    let win = crate::shuffle_order::replay(&source, &inputs)
        .unwrap()
        .pop()
        .unwrap();
    let config = SolverConfig {
        max_width: 4,
        transitions_per_width: 2,
        ..Default::default()
    };
    let mut plain = CombatSolver::new(source.clone(), config.clone()).unwrap();
    let mut observed = CombatSolver::new(source, config).unwrap();
    observed.audit_reference(&inputs, &win).unwrap();
    plain.advance(750, Duration::from_secs(30)).unwrap();
    observed.advance(750, Duration::from_secs(30)).unwrap();
    assert_eq!(
        serde_json::to_value(plain.stats()).unwrap(),
        serde_json::to_value(observed.stats()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(plain.plans()).unwrap(),
        serde_json::to_value(observed.plans()).unwrap()
    );
    let audit = observed.audit().unwrap();
    assert!(audit.passes.iter().any(|p| !p.quotas.is_empty()));
    for pass in audit.passes {
        assert_eq!(
            pass.work.transitions_by_turn.values().sum::<usize>(),
            pass.work.transitions
        );
        assert!(pass.quotas.len() <= 32);
        assert!(pass
            .quotas
            .iter()
            .all(|q| q.reserve_size + q.reserve_cursors <= pass.width * 8));
    }
}

#[test]
fn nob_defend_is_worth_playing_when_next_turn_already_kills() {
    use crate::local::ConstructedCombat;
    use sts_core::content::monsters::factory::EncounterId;
    use sts_core::state::map::node::RoomType;
    let r = ConstructedCombat {
        seed: 17,
        encounter: EncounterId::GremlinNob,
        act: 1,
        room: RoomType::MonsterRoomElite,
        hp: 68,
        deck: vec![(CardId::Strike, 0); 10],
        relics: vec![],
        potions: vec![],
        max_potion_uses: None,
    }
    .build()
    .unwrap();
    let step = EngineCombatStepper.apply_to_stable(
        &r,
        ClientInput::EndTurn,
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(!step.truncated && !step.timed_out);
    let mut r = step.position;
    // Explicit mechanism intervention after the natural Bellow transition.
    r.combat.entities.monsters[0].current_hp = 12;
    r.combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 101),
        CombatCard::for_scenario(CardId::Defend, 102),
    ];
    r.combat.zones.draw_pile = (0..5)
        .map(|i| CombatCard::for_scenario(CardId::Strike, 201 + i))
        .collect::<Vec<_>>()
        .into();
    let s = run(r.clone(), 2000);
    assert_eq!(s.plans[0].outcome.final_hp, 68); // 6 actual damage, then Burning Blood.
    let mut p = r.clone();
    let mut defended = false;
    for input in &s.plans[0].inputs {
        if matches!(input, ClientInput::EndTurn) {
            break;
        }
        if let ClientInput::PlayCard { card_index, .. } = input {
            defended |= p.combat.zones.hand[*card_index].id == CardId::Defend;
        }
        p = EngineCombatStepper
            .apply_to_stable(
                &p,
                input.clone(),
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            )
            .position;
    }
    assert!(defended);
    r.combat.entities.monsters[0].current_hp = 6;
    let immediate = run(r, 2000);
    assert_eq!(immediate.plans[0].inputs.len(), 1);
}

#[test]
fn optional_potion_branches_do_not_crowd_out_the_nob_card_route() {
    use crate::local::ConstructedCombat;
    use sts_core::content::monsters::factory::EncounterId;
    use sts_core::state::map::node::RoomType;
    let mut deck = vec![(CardId::Strike, 0); 5];
    deck.extend(vec![(CardId::Defend, 0); 4]);
    deck.extend([
        (CardId::Bash, 0),
        (CardId::AscendersBane, 0),
        (CardId::Bludgeon, 0),
        (CardId::ShrugItOff, 0),
        (CardId::Cleave, 0),
    ]);
    let mut spec = ConstructedCombat {
        seed: 997,
        encounter: EncounterId::GremlinNob,
        act: 1,
        room: RoomType::MonsterRoomElite,
        hp: 68,
        deck,
        relics: vec![],
        potions: vec![],
        max_potion_uses: None,
    };
    let empty = spec.build().unwrap();
    let mut baseline = CombatSolver::new(empty.clone(), SolverConfig::default()).unwrap();
    baseline.advance(2_000, Duration::from_secs(30)).unwrap();
    let witness = baseline.plans()[0].clone();
    assert_eq!(witness.outcome.final_hp, 43);
    assert!(
        witness.found_at_work <= 2_000,
        "recovery must preserve the early primary witness"
    );
    baseline.advance(48_000, Duration::from_secs(30)).unwrap();
    assert!(baseline.plans().iter().any(|p| p.outcome.final_hp >= 43));
    for potion in [
        PotionId::FirePotion,
        PotionId::BlockPotion,
        PotionId::StrengthPotion,
    ] {
        spec.potions = vec![potion];
        let populated = spec.build().unwrap();
        let mut without_inventory = populated.clone();
        without_inventory.combat.entities.potions = empty.combat.entities.potions.clone();
        assert_eq!(without_inventory, empty);
        // First establish that the old card route really remains available.
        // These are inert carried bottles, not passive Fairy interventions.
        let mut replay = populated.clone();
        for input in &witness.inputs {
            assert!(EngineCombatStepper.is_legal_action(&replay, input));
            let step = EngineCombatStepper.apply_to_stable(
                &replay,
                input.clone(),
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            replay = step.position;
        }
        assert!(EngineCombatStepper.is_exact_victory(&replay));
        assert_eq!(replay.combat.entities.player.current_hp, 43);
        assert_eq!(
            replay.combat.entities.potions,
            populated.combat.entities.potions
        );
        let mut solver = CombatSolver::new(populated, SolverConfig::default()).unwrap();
        solver.advance(4_000, Duration::from_secs(30)).unwrap();
        assert!(
            solver
                .plans()
                .iter()
                .any(|p| p.outcome.total_potion_expenditures == 0
                    && p.outcome.final_hp >= 43
                    && p.found_at_work <= 4_000),
            "{potion:?} crowded out the known card-only route"
        );
        solver.advance(46_000, Duration::from_secs(30)).unwrap();
        assert!(solver
            .plans()
            .iter()
            .any(|p| p.outcome.total_potion_expenditures == 0 && p.outcome.final_hp >= 43));
    }
}

#[test]
fn width_balancing_reaches_the_awakened_one_potion_route_within_budget() {
    use crate::local::ConstructedCombat;
    use sts_core::content::monsters::factory::EncounterId;
    use sts_core::content::relics::RelicId;
    use sts_core::state::map::node::RoomType;
    let r = ConstructedCombat {
        seed: 73,
        encounter: EncounterId::AwakenedOne,
        act: 3,
        room: RoomType::MonsterRoomBoss,
        hp: 75,
        deck: vec![
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::AscendersBane, 0),
            (CardId::DemonForm, 1),
            (CardId::PommelStrike, 1),
            (CardId::ShrugItOff, 1),
            (CardId::ShrugItOff, 0),
            (CardId::Impervious, 0),
            (CardId::Disarm, 1),
            (CardId::Reaper, 0),
            (CardId::HeavyBlade, 1),
            (CardId::LimitBreak, 1),
            (CardId::Offering, 0),
            (CardId::Armaments, 1),
            (CardId::TrueGrit, 1),
        ],
        relics: vec![
            RelicId::CoffeeDripper,
            RelicId::BagOfPreparation,
            RelicId::Vajra,
        ],
        potions: vec![PotionId::DuplicationPotion, PotionId::SwiftPotion],
        max_potion_uses: None,
    }
    .build()
    .unwrap();
    let mut solver = CombatSolver::new(r, SolverConfig::default()).unwrap();
    solver.advance(50_000, Duration::from_secs(30)).unwrap();
    assert!(solver
        .plans()
        .iter()
        .any(|p| p.outcome.final_hp >= 10 && EngineCombatStepper.is_exact_victory(&p.successor)));
    assert!(solver.stats.card_only_transitions > 0);
    assert_eq!(solver.stats.work(), 50_000);
}

#[test]
fn long_lived_narrow_search_cannot_starve_the_wider_search() {
    let mut r = root(&[CardId::Strike, CardId::Strike, CardId::Defend], 10_000);
    r.combat.entities.player.current_hp = 10_000;
    r.combat.entities.player.max_hp = 10_000;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 1))];
    let mut solver = CombatSolver::new(
        r,
        SolverConfig {
            max_width: 8,
            transitions_per_width: 2,
            ..Default::default()
        },
    )
    .unwrap();
    // Reproduce an allowed scheduler state: the all-action beam has already
    // widened while card-only exploration is still in a long narrow pass.
    solver.beams[0].passes[0].width = 8;
    for _ in 0..16 {
        solver.advance(1, Duration::from_secs(30)).unwrap();
    }
    assert!(solver.stats.card_only_transitions > 0);
    assert!(solver.stats.search_transitions > solver.stats.card_only_transitions);
    assert!(!solver.is_complete());
}

#[test]
fn wider_passes_start_without_losing_or_restarting_narrow_frontiers() {
    let mut r = root(&[CardId::Strike, CardId::Defend, CardId::Defend], 10_000);
    r.combat.entities.player.current_hp = 10_000;
    r.combat.entities.player.max_hp = 10_000;
    let config = SolverConfig {
        max_width: 16,
        transitions_per_width: 2,
        ..Default::default()
    };
    let mut shared = CombatSolver::new(r.clone(), config.clone()).unwrap();
    shared.advance(160, Duration::from_secs(30)).unwrap();
    let passes = &shared.beams[0].passes;
    assert_eq!(
        passes.iter().map(|p| p.width).collect::<Vec<_>>(),
        [4, 8, 16]
    );
    assert!(passes.iter().all(|p| !p.done && p.search_work > 0));
    assert_eq!(
        passes.iter().map(|p| p.search_work).sum::<usize>() + shared.stats.result_backup_work,
        160
    );
    let previous = passes.iter().map(|p| p.search_work).collect::<Vec<_>>();
    shared.advance(90, Duration::from_secs(30)).unwrap();
    assert_eq!(
        shared.beams[0].passes.len(),
        3,
        "bounded by configured widths"
    );
    for (pass, work) in shared.beams[0].passes.iter().zip(previous) {
        assert!(pass.search_work > work, "each live pass continues");
    }
    assert!(!shared.is_complete());
    // Shared future-boundary evidence may change a pass's retained choices.
    // Every resumed frontier still carries a replay-exact original-root path.
    for pass in &shared.beams[0].passes {
        for node in pass
            .active
            .iter()
            .chain(&pass.next)
            .chain(&pass.ended)
            .chain(&pass.deferred)
        {
            if node.state.selected_members().is_none() && node.input_count > 0 {
                assert_eq!(
                    crate::shuffle_order::replay(&r, &node.inputs())
                        .unwrap()
                        .last(),
                    Some(node.state.position())
                );
            }
        }
    }
    fn frontier(pass: &Pass) -> serde_json::Value {
        let node = |n: &Node| {
            serde_json::json!({
                "position": n.state.position(), "inputs": n.inputs(),
                "selection": n.state.selected_members(), "uses": n.state.potion_uses(),
            })
        };
        serde_json::json!({
            "turn_work": pass.turn_work,
            "active": pass.active.iter().map(&node).collect::<Vec<_>>(),
            "next": pass.next.iter().map(&node).collect::<Vec<_>>(),
            "pending": pass.pending_nodes().map(&node).collect::<Vec<_>>(),
            "ended": pass.ended.iter().map(&node).collect::<Vec<_>>(),
            "deferred": pass.deferred.iter().map(&node).collect::<Vec<_>>(),
            "deferred_cursors": pass.deferred_cursors.iter().map(|c| serde_json::json!({
                "nodes": c.nodes().map(&node).collect::<Vec<_>>(), "remaining": c.remaining_branches(),
            })).collect::<Vec<_>>(),
            "primary_done": pass.primary_done,
            "expansion": pass.expansion.as_ref().map(|e| serde_json::json!({
                "node": node(&e.node), "index": e.index, "branches": format!("{:?}", e.branches),
            })),
        })
    }
    let mut chunked = CombatSolver::new(r, config).unwrap();
    for _ in 0..250 {
        chunked.advance(1, Duration::from_secs(30)).unwrap();
    }
    assert_eq!(
        serde_json::to_value(shared.stats()).unwrap(),
        serde_json::to_value(chunked.stats()).unwrap()
    );
    assert_eq!(shared.beams[0].passes.len(), chunked.beams[0].passes.len());
    for (whole, chunk) in shared.beams[0].passes.iter().zip(&chunked.beams[0].passes) {
        assert_eq!(frontier(whole), frontier(chunk));
    }
}

#[test]
fn narrow_card_choice_uses_real_targets_and_remaining_resource_permissions() {
    let admits = |position: CombatPosition, card_only, budget| {
        actions::narrow_card_choice(
            &Node::root(CombatSearchState::new(position).unwrap()).with_potion_limit(budget),
            &CombatSearchSpace::default(),
            card_only,
            budget,
        )
        .unwrap()
    };
    let single = root(&[CardId::Expertise], 20);
    assert!(admits(single.clone(), false, Some(0)));
    assert!(admits(root(&[CardId::Strike], 20), false, Some(0)));
    assert!(!admits(
        root(&[CardId::Expertise, CardId::Strike], 20),
        false,
        Some(0)
    ));
    assert!(!admits(root(&[CardId::Wound], 20), false, Some(0)));
    let mut targets = root(&[CardId::Strike], 20);
    let mut other = targets.combat.entities.monsters[0].clone();
    other.id = 2;
    other.slot = 1;
    targets.combat.entities.monsters.push(other);
    assert!(!admits(targets, false, Some(0)));
    let mut bottled = single;
    bottled.combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 7))];
    assert!(!admits(bottled.clone(), false, Some(1)));
    assert!(admits(bottled.clone(), false, Some(0)));
    assert!(admits(bottled, true, Some(1)));
}

#[test]
fn narrow_card_refill_materializes_both_branches_and_resumes_exactly() {
    let mut source = root(&[CardId::Strike, CardId::Expertise], 22);
    source.combat.entities.player.current_hp = 1;
    source.combat.turn.energy = 3;
    source.combat.zones.draw_pile = [CardId::GlassKnife, CardId::Wound, CardId::Wound]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, 100 + i as u32))
        .collect();
    let inputs = vec![
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
    ];
    let expected = crate::shuffle_order::replay(&source, &inputs)
        .unwrap()
        .pop()
        .unwrap();
    assert!(EngineCombatStepper.is_exact_victory(&expected));
    let config = SolverConfig {
        max_width: 4,
        max_potion_uses: Some(0),
        ..Default::default()
    };
    let mut whole = CombatSolver::new(source.clone(), config.clone()).unwrap();
    let mut chunks = CombatSolver::new(source, config).unwrap();
    whole.audit_reference(&inputs, &expected).unwrap();
    chunks.audit_reference(&inputs, &expected).unwrap();
    whole.advance(1000, Duration::from_secs(30)).unwrap();
    for _ in 0..1000 {
        let before = chunks.stats.work();
        chunks.advance(1, Duration::from_secs(30)).unwrap();
        assert!(chunks.stats.work() <= before + 1);
        for pass in chunks.beams.iter().flat_map(|beam| &beam.passes) {
            assert!(pass.pending.len() + usize::from(pass.resolving.is_some()) <= pass.width * 8);
        }
    }
    assert!(!whole.plans().is_empty());
    assert!(whole.stats.narrow_card_choices > 0);
    let audit = whole.audit().unwrap();
    assert!(audit
        .passes
        .iter()
        .any(|pass| pass.steps[1].events.contains_key("narrow_card_choice")));
    assert!(audit
        .passes
        .iter()
        .any(|pass| pass.steps[2].events.contains_key("expanded")));
    // The source also generated an EndTurn branch; the refill was never forced.
    assert!(whole.stats.observed_losses > 0);
    assert_eq!(
        serde_json::to_value(whole.stats()).unwrap(),
        serde_json::to_value(chunks.stats()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(whole.plans()).unwrap(),
        serde_json::to_value(chunks.plans()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(whole.audit()).unwrap(),
        serde_json::to_value(chunks.audit()).unwrap()
    );
}

#[test]
fn narrow_card_cycle_still_obeys_turn_and_total_work_quotas() {
    let mut source = root(&[CardId::Finesse], 9999);
    source.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Finesse, 100)].into();
    let mut solver = CombatSolver::new(
        source,
        SolverConfig {
            max_width: 4,
            max_potion_uses: Some(0),
            transitions_per_width: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        solver.advance(200, Duration::from_secs(30)).unwrap(),
        SolverStop::WorkBudget
    );
    assert_eq!(solver.stats.work(), 200);
    assert!(solver.stats.narrow_card_choices > 0);
    assert!(solver.stats.turn_budget_dropped > 0);
    assert!(solver.stats.turn_layers > 0);
}

#[test]
fn setup_actions_survive_long_enough_to_realize_a_followup() {
    let mut missed = vec![];
    let scenarios = [
        (CardId::DoubleTap, 4, 64, false),
        (CardId::BattleTrance, 3, 32, true),
        (CardId::BurningPact, 4, 32, true),
    ];
    for (setup, energy, hp, draw_finisher) in scenarios {
        let mut hand = vec![setup];
        if !draw_finisher {
            hand.push(CardId::Bludgeon);
        }
        hand.extend([CardId::Strike; 4]);
        let mut r = root(&hand, hp);
        r.combat.entities.player.current_hp = 1;
        r.combat.turn.energy = energy;
        if draw_finisher {
            r.combat.zones.draw_pile = vec![
                CombatCard::for_scenario(CardId::Bludgeon, 101),
                CombatCard::for_scenario(CardId::Wound, 102),
                CombatCard::for_scenario(CardId::Wound, 103),
            ]
            .into();
        }
        // Independent witness establishes feasibility before testing retention.
        // These are explicit mechanism interventions, not natural-run samples.
        let mut replay = r.clone();
        for id in [setup, CardId::Bludgeon] {
            if matches!(
                replay.engine,
                EngineState::PendingChoice(PendingChoice::HandSelect { .. })
            ) {
                use sts_core::state::selection::{SelectionResolution, SelectionScope};
                let lineage = replay
                    .combat
                    .zones
                    .hand
                    .iter()
                    .find(|c| c.id == CardId::Strike)
                    .unwrap()
                    .lineage;
                let input = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                    SelectionScope::Hand,
                    [lineage],
                ));
                assert!(EngineCombatStepper.is_legal_action(&replay, &input));
                let step = EngineCombatStepper.apply_to_stable(
                    &replay,
                    input,
                    CombatStepLimits {
                        max_engine_steps: 512,
                        deadline: None,
                    },
                );
                assert!(!step.truncated && !step.timed_out);
                replay = step.position;
            }
            let card_index = replay
                .combat
                .zones
                .hand
                .iter()
                .position(|c| c.id == id)
                .unwrap();
            let input = ClientInput::PlayCard {
                card_index,
                target: (id == CardId::Bludgeon).then_some(1),
            };
            assert!(EngineCombatStepper.is_legal_action(&replay, &input));
            let step = EngineCombatStepper.apply_to_stable(
                &replay,
                input,
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            replay = step.position;
        }
        assert!(
            EngineCombatStepper.is_exact_victory(&replay),
            "{setup:?} witness"
        );
        let mut solver = CombatSolver::new(
            r,
            SolverConfig {
                initial_width: 4,
                max_width: 4,
                diversify_decision_contexts: true,
                complete_actions: true,
                ..Default::default()
            },
        )
        .unwrap();
        solver.advance(2000, Duration::from_secs(30)).unwrap();
        if solver.plans().is_empty() {
            missed.push(setup);
        }
    }
    assert!(
        missed.is_empty(),
        "missed replayed setup routes: {missed:?}"
    );
}

#[test]
fn completed_actions_resume_at_single_work_boundaries() {
    for follow_preparations in [false, true] {
        check_action_continuations_resume(follow_preparations);
    }
}

fn check_action_continuations_resume(follow_preparations: bool) {
    let mut r = root(&[CardId::BurningPact, CardId::Strike, CardId::Strike], 32);
    r.combat.entities.player.current_hp = 1;
    r.combat.turn.energy = 4;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::GamblersBrew, 7))];
    r.combat.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::Bludgeon, 101),
        CombatCard::for_scenario(CardId::Wound, 102),
    ]
    .into();
    let config = SolverConfig {
        max_width: 4,
        diversify_decision_contexts: !follow_preparations,
        complete_actions: !follow_preparations,
        follow_preparations,
        ..Default::default()
    };
    let mut whole = CombatSolver::new(r.clone(), config.clone()).unwrap();
    whole.advance(500, Duration::from_secs(30)).unwrap();
    let mut chunks = CombatSolver::new(r.clone(), config).unwrap();
    let witness = whole
        .plans()
        .first()
        .expect("setup or potion unlocks the finisher");
    chunks
        .audit_reference(&witness.inputs, &witness.successor)
        .unwrap();
    let mut saw_pending = false;
    let mut saw_replay = false;
    let mut continued_before_choices_exhausted = false;
    for _ in 0..500 {
        let before = chunks.stats.work();
        chunks.advance(1, Duration::from_secs(30)).unwrap();
        assert!(chunks.stats.work() <= before + 1);
        saw_pending |= chunks
            .beams
            .iter()
            .flat_map(|b| &b.passes)
            .any(|p| !p.pending.is_empty());
        saw_replay |= chunks.replay.is_some();
        continued_before_choices_exhausted |=
            chunks.beams.iter().flat_map(|b| &b.passes).any(|p| {
                !p.pending.is_empty()
                    && p.expansion.as_ref().is_some_and(|e| {
                        !actions::unfinished(&e.node)
                            && e.node
                                .inputs()
                                .iter()
                                .any(|i| matches!(i, ClientInput::SubmitSelection(_)))
                    })
            });
        for pass in chunks.beams.iter().flat_map(|b| &b.passes) {
            assert!(pass.pending.len() + usize::from(pass.resolving.is_some()) <= pass.width * 8);
        }
    }
    assert!(saw_pending && saw_replay);
    assert!(
        continued_before_choices_exhausted,
        "completed choices must get a followup before every other selection finishes"
    );
    assert!(!whole.plans().is_empty());
    assert_eq!(
        serde_json::to_value(whole.stats()).unwrap(),
        serde_json::to_value(chunks.stats()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(whole.plans()).unwrap(),
        serde_json::to_value(chunks.plans()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(whole.progress()).unwrap(),
        serde_json::to_value(chunks.progress()).unwrap()
    );
    let audit = chunks.audit().unwrap();
    for reason in ["hand/gambling_chip", "hand/exhaust"] {
        assert!(audit.passes.iter().any(|p| p
            .work
            .pending_choice_work
            .get(reason)
            .copied()
            .unwrap_or(0)
            > 0));
    }
    assert!(chunks.stats.completed_action_alternatives > 0);
    if follow_preparations {
        assert!(chunks.stats.followup_transitions > 0);
        assert!(chunks.stats.preparation_opportunities > 0);
    } else {
        assert_eq!(chunks.stats.preparation_opportunities, 0);
        assert!(chunks.stats.narrow_card_choices > 0);
        assert!(chunks.stats.followup_transitions > 0);
    }
    for plan in chunks.plans() {
        assert_eq!(
            crate::shuffle_order::replay(chunks.root(), &plan.inputs)
                .unwrap()
                .last(),
            Some(&plan.successor)
        );
    }
}

#[test]
fn start_of_turn_selection_receives_the_new_turn_quota() {
    let r = root(
        &[CardId::ToolsOfTheTrade, CardId::Strike, CardId::Defend],
        100,
    );
    let setup = EngineCombatStepper.apply_to_stable(
        &r,
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(!setup.truncated && !setup.timed_out);
    let r = setup.position;
    let next = EngineCombatStepper.apply_to_stable(
        &r,
        ClientInput::EndTurn,
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    assert!(!next.truncated && !next.timed_out);
    assert!(next.position.combat.turn.turn_count > r.combat.turn.turn_count);
    assert!(matches!(
        next.position.engine,
        EngineState::PendingChoice(_)
    ));

    for complete_actions in [false, true] {
        let mut solver = CombatSolver::new(
            r.clone(),
            SolverConfig {
                max_width: 4,
                transitions_per_width: 1,
                complete_actions,
                ..Default::default()
            },
        )
        .unwrap();
        // EndTurn is the first scheduled branch. Its choice must cross the turn
        // layer, rather than bypass the new quota via the within-action queue.
        solver.advance(1, Duration::from_secs(30)).unwrap();
        assert!(solver.beams[0].passes[0].pending.is_empty());
        assert_eq!(solver.beams[0].passes[0].ended.len(), 1);
        assert_eq!(
            solver.beams[0].passes[0].ended[0].state.position(),
            &next.position
        );
        for _ in 0..32 {
            solver.advance(1, Duration::from_secs(30)).unwrap();
            if solver.stats.turn_layers > 0 {
                break;
            }
        }
        assert_eq!(solver.stats.turn_layers, 1);
        assert_eq!(solver.beams[0].passes[0].turn_work, 1);
        assert!(solver.stats.turn_budget_dropped > 0);
    }
}

#[test]
fn reserve_work_and_primary_work_in_the_other_potion_family_both_receive_service() {
    let mut source = root(&[CardId::Strike, CardId::Defend], 10_000);
    source.combat.entities.player.current_hp = 10_000;
    source.combat.entities.player.max_hp = 10_000;
    source.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 1))];
    let saved = played_node(&source, 1, None);
    let mut solver = CombatSolver::new(
        source,
        SolverConfig {
            initial_width: 4,
            max_width: 4,
            ..Default::default()
        },
    )
    .unwrap();
    let first = &mut solver.beams[0].passes[0];
    // Allowed scheduler state: this primary pass has ended; the other family
    // still has original work. Keep a real, source-bound prefix in reserve.
    first.active.clear();
    first.primary_done = true;
    first.defer([saved.clone()], &mut solver.stats);
    solver.advance(20, Duration::from_secs(30)).unwrap();
    assert_eq!(solver.stats.work(), 20);
    assert!(solver.stats.card_only_transitions > 0);
    assert!(solver.stats.resumed_nodes > 0);
    assert!(solver.stats.recovery_transitions > 0);
    assert!(!solver.beams[1].passes[0].primary_done);
}

#[test]
fn automatic_shuffle_work_resumes_within_the_same_budget_and_replays_every_plan() {
    let mut source = root(&[CardId::Strike, CardId::Defend, CardId::Anger], 24);
    source.combat.zones.draw_pile = [CardId::Strike, CardId::Strike, CardId::Defend]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 20))
        .collect();
    source.combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 1))];
    let config = SolverConfig {
        max_width: 8,
        max_potion_uses: Some(0),
        shuffle_continuation_work: 32,
        ..Default::default()
    };
    let mut whole = CombatSolver::new(source.clone(), config.clone()).unwrap();
    let mut chunks = CombatSolver::new(source.clone(), config).unwrap();
    whole.advance(6000, Duration::from_secs(30)).unwrap();
    let mut saw_child = false;
    for _ in 0..6000 {
        let before = chunks.stats.work();
        chunks.advance(1, Duration::from_secs(30)).unwrap();
        assert!(chunks.stats.work() <= before + 1);
        let state = serde_json::to_value(chunks.stats()).unwrap();
        let _ = chunks.shuffle_progress();
        let _ = chunks.progress();
        chunks.advance(0, Duration::from_secs(30)).unwrap();
        chunks.advance(1, Duration::ZERO).unwrap();
        assert_eq!(serde_json::to_value(chunks.stats()).unwrap(), state);
        if let Some(child) = chunks.shuffle_job.as_ref().and_then(|j| j.child_for_test()) {
            saw_child = true;
            assert_eq!(child.stats.shuffle_work, 0);
            assert_eq!(
                chunks.shuffle_progress().unwrap().remaining_orders,
                0,
                "every cheap proposal must be visited before expensive continuation"
            );
        }
        assert_eq!(
            chunks.stats.shuffle_work,
            chunks.stats.shuffle_source_work
                + chunks.stats.shuffle_prefix_work
                + chunks.stats.shuffle_suffix_work
                + chunks.stats.shuffle_continuation_work
        );
    }
    assert!(
        whole.stats.shuffle_candidates > 0,
        "an automatically found win must produce real mapped candidates"
    );
    assert!(whole.stats.shuffle_observed_wins > 0);
    assert!(whole.stats.shuffle_work > 0);
    assert!(whole.stats.shuffle_continuations_queued > 0);
    assert!(saw_child);
    assert_eq!(
        serde_json::to_value(whole.stats()).unwrap(),
        serde_json::to_value(chunks.stats()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(whole.plans()).unwrap(),
        serde_json::to_value(chunks.plans()).unwrap()
    );
    assert_eq!(whole.root(), &source);
    for plan in whole.plans() {
        let replay = crate::shuffle_order::replay(&source, &plan.inputs).unwrap();
        assert_eq!(replay.last(), Some(&plan.successor));
        assert_eq!(plan.outcome.total_potion_expenditures, 0);
        assert_eq!(
            plan.successor.combat.entities.potions,
            source.combat.entities.potions
        );
    }
}
