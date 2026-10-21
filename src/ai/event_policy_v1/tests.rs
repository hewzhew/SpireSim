use crate::ai::event_policy_v1::{
    build_event_decision_context_v1, compile_event_plan_candidates_v1,
    compile_event_plan_status_v1, plan_event_decision_v1, select_event_plan_candidate_v1,
    EventCandidateTierV1, EventDecisionShapeV1, EventInformationModeV1, EventOracleOutcomeV1,
    EventPlanCompileStatusV1, EventPlanIdV1, EventPlanRewardV1, EventPlanRiskModelV1,
    EventPlanUnsupportedShapeV1, EventPolicyActionV1, EventPolicyClassV1, EventPolicyConfigV1,
    RepeatablePaidMenuShapeV1,
};
use crate::ai::random_upgrade_opportunity_v1::{
    evaluate_random_upgrade_opportunity_v1, ProbabilityBucketV1, RandomUpgradeSourceV1,
    RandomUpgradeVerdictV1,
};
use crate::content::cards::CardId;
use crate::runtime::combat::CombatCard;
use crate::state::events::{
    EventActionKind, EventChoiceMeta, EventEffect, EventId, EventOption, EventOptionSemantics,
    EventOptionTransition, EventState,
};
use crate::state::run::RunState;
use crate::{content::relics::RelicId, content::relics::RelicState};

#[test]
fn event_context_classifies_free_known_benefit() {
    let run = RunState::new(1, 0, false, "Ironclad");
    let context = build_event_decision_context_v1(
        &run,
        EventId::GoldenShrine,
        vec![
            option(
                "[Pray] Gain 100 Gold.",
                EventActionKind::Gain,
                vec![EventEffect::GainGold(100)],
                EventOptionTransition::AdvanceScreen,
            ),
            option(
                "[Desecrate] Gain 275 Gold. Become Cursed - Regret.",
                EventActionKind::Gain,
                vec![
                    EventEffect::GainGold(275),
                    EventEffect::ObtainCurse {
                        count: 1,
                        kind: crate::state::events::EventCardKind::Specific(
                            crate::content::cards::CardId::Regret,
                        ),
                    },
                ],
                EventOptionTransition::AdvanceScreen,
            ),
            option(
                "[Leave]",
                EventActionKind::Leave,
                vec![],
                EventOptionTransition::Complete,
            ),
        ],
    );

    assert_eq!(
        context.candidates[0].class,
        EventPolicyClassV1::FreeKnownBenefit
    );
}

#[test]
fn event_context_classifies_leave_option_without_deciding_policy() {
    let run = RunState::new(1, 0, false, "Ironclad");
    let context = build_event_decision_context_v1(
        &run,
        EventId::Addict,
        vec![
            option(
                "[Pay] Lose 85 Gold. Obtain a random Relic.",
                EventActionKind::Trade,
                vec![
                    EventEffect::LoseGold(85),
                    EventEffect::ObtainRelic {
                        count: 1,
                        kind: crate::state::events::EventRelicKind::RandomRelic,
                    },
                ],
                EventOptionTransition::AdvanceScreen,
            ),
            option(
                "[Rob] Obtain a random Relic. Become Cursed - Shame.",
                EventActionKind::Trade,
                vec![
                    EventEffect::ObtainRelic {
                        count: 1,
                        kind: crate::state::events::EventRelicKind::RandomRelic,
                    },
                    EventEffect::ObtainCurse {
                        count: 1,
                        kind: crate::state::events::EventCardKind::Specific(
                            crate::content::cards::CardId::Shame,
                        ),
                    },
                ],
                EventOptionTransition::AdvanceScreen,
            ),
            option(
                "[Leave]",
                EventActionKind::Leave,
                vec![],
                EventOptionTransition::Complete,
            ),
        ],
    );

    assert_eq!(context.candidates[2].class, EventPolicyClassV1::SafeExit);
}

#[test]
fn event_policy_does_not_treat_attack_as_safe_exit() {
    let run = RunState::new(1, 0, false, "Ironclad");
    let context = build_event_decision_context_v1(
        &run,
        EventId::WeMeetAgain,
        vec![
            option(
                "[Give Gold] Lose 124 Gold. Obtain a Relic.",
                EventActionKind::Trade,
                vec![
                    EventEffect::LoseGold(124),
                    EventEffect::ObtainRelic {
                        count: 1,
                        kind: crate::state::events::EventRelicKind::RandomRelic,
                    },
                ],
                EventOptionTransition::AdvanceScreen,
            ),
            option(
                "[Give Card] Give Clothesline. Obtain a Relic.",
                EventActionKind::Trade,
                vec![
                    EventEffect::RemoveCard {
                        count: 1,
                        target_lineage: Some(crate::test_support::scenario_lineage(42)),
                        kind: crate::state::events::EventCardKind::Specific(
                            crate::content::cards::CardId::Clothesline,
                        ),
                    },
                    EventEffect::ObtainRelic {
                        count: 1,
                        kind: crate::state::events::EventRelicKind::RandomRelic,
                    },
                ],
                EventOptionTransition::AdvanceScreen,
            ),
            option(
                "[Attack]",
                EventActionKind::Decline,
                vec![],
                EventOptionTransition::AdvanceScreen,
            ),
        ],
    );

    let decision = plan_event_decision_v1(&context, &EventPolicyConfigV1::default());

    assert!(matches!(decision.action, EventPolicyActionV1::Stop { .. }));
}

#[test]
fn event_policy_stops_for_neow() {
    let run = RunState::new(1, 0, false, "Ironclad");
    let context = build_event_decision_context_v1(
        &run,
        EventId::Neow,
        vec![option(
            "Obtain a random rare card.",
            EventActionKind::Gain,
            vec![EventEffect::ObtainCard {
                count: 1,
                kind: crate::state::events::EventCardKind::RandomClassRare,
            }],
            EventOptionTransition::AdvanceScreen,
        )],
    );

    let decision = plan_event_decision_v1(&context, &EventPolicyConfigV1::default());

    assert!(matches!(decision.action, EventPolicyActionV1::Stop { .. }));
}

#[test]
fn event_policy_takes_max_hp_for_hp_when_health_buffer_is_safe() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.current_hp = 74;
    run.max_hp = 80;
    run.event_state = Some(EventState::new(EventId::ForgottenAltar));
    let options = crate::content::events::forgotten_altar::get_options(
        &run,
        run.event_state.as_ref().unwrap(),
    );
    let context = build_event_decision_context_v1(&run, EventId::ForgottenAltar, options);

    let decision = plan_event_decision_v1(&context, &EventPolicyConfigV1::default());

    assert!(matches!(
        decision.action,
        EventPolicyActionV1::Pick { index: 1, .. }
    ));
}

#[test]
fn event_policy_stops_max_hp_for_hp_when_health_buffer_is_low() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.current_hp = 24;
    run.max_hp = 80;
    run.event_state = Some(EventState::new(EventId::ForgottenAltar));
    let options = crate::content::events::forgotten_altar::get_options(
        &run,
        run.event_state.as_ref().unwrap(),
    );
    let context = build_event_decision_context_v1(&run, EventId::ForgottenAltar, options);

    let decision = plan_event_decision_v1(&context, &EventPolicyConfigV1::default());

    assert!(matches!(decision.action, EventPolicyActionV1::Stop { .. }));
}

#[test]
fn winding_halls_with_mark_of_the_bloom_prefers_max_hp_loss_over_blocked_heal_curse() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.current_hp = 79;
    run.max_hp = 90;
    run.relics.push(RelicState::new(RelicId::MarkOfTheBloom));
    let mut event_state = EventState::new(EventId::WindingHalls);
    event_state.current_screen = 1;
    run.event_state = Some(event_state);
    let options =
        crate::content::events::winding_halls::get_options(&run, run.event_state.as_ref().unwrap());
    let context = build_event_decision_context_v1(&run, EventId::WindingHalls, options);

    let decision = plan_event_decision_v1(&context, &EventPolicyConfigV1::default());

    assert!(matches!(
        decision.action,
        EventPolicyActionV1::Pick { index: 2, .. }
    ));
}

#[test]
fn mind_bloom_remember_exposes_mark_of_the_bloom_healing_lock_risk() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.current_hp = 58;
    run.max_hp = 80;
    run.floor_num = 37;
    run.event_state = Some(EventState::new(EventId::MindBloom));
    let options =
        crate::content::events::mind_bloom::get_options(&run, run.event_state.as_ref().unwrap());
    let context = build_event_decision_context_v1(&run, EventId::MindBloom, options);

    let remember = context
        .candidates
        .iter()
        .find(|candidate| candidate.label.contains("[Remember]"))
        .expect("Mind Bloom Remember option should be visible");

    assert!(remember.obtains_mark_of_the_bloom);
    assert!(
        remember
            .risks
            .iter()
            .any(|risk| risk.contains("disables all future healing")),
        "Remember should expose the Mark of the Bloom healing lock risk"
    );
    assert_eq!(remember.evaluation.tier, EventCandidateTierV1::Avoid);
}

#[test]
fn random_upgrade_opportunity_detects_high_debt_density_with_safe_hp_cost() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.current_hp = 72;
    run.max_hp = 80;
    run.master_deck.clear();
    run.master_deck
        .push(CombatCard::for_scenario(CardId::TrueGrit, 1));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Armaments, 2));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Bash, 3));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Strike, 4));

    let plan = evaluate_random_upgrade_opportunity_v1(
        &run,
        RandomUpgradeSourceV1::ShiningLight {
            hp_cost: 16,
            upgrade_count: 2,
        },
    );

    assert_eq!(plan.eligible_count, 4);
    assert!(plan.hit_distribution.important_or_better_targets >= 2);
    assert!(
        plan.hit_distribution.p_hit_at_least_one_important_or_better >= ProbabilityBucketV1::Medium
    );
    assert!(matches!(
        plan.verdict,
        RandomUpgradeVerdictV1::EnterClean | RandomUpgradeVerdictV1::EnterRisky
    ));
}

#[test]
fn shining_light_uses_random_upgrade_opportunity_instead_of_default_leave() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.current_hp = 72;
    run.max_hp = 80;
    run.event_state = Some(EventState::new(EventId::ShiningLight));
    run.master_deck.clear();
    run.master_deck
        .push(CombatCard::for_scenario(CardId::TrueGrit, 1));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Armaments, 2));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Bash, 3));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Strike, 4));
    let options =
        crate::content::events::shining_light::get_options(&run, run.event_state.as_ref().unwrap());

    let context = build_event_decision_context_v1(&run, EventId::ShiningLight, options);
    let enter = context
        .candidates
        .iter()
        .find(|candidate| candidate.label.contains("[Enter the Light]"))
        .expect("enter candidate");

    assert_eq!(enter.class, EventPolicyClassV1::RandomUpgradeOpportunity);
    assert!(enter.evaluation.score > 120);
    assert!(enter
        .evaluation
        .reasons
        .iter()
        .any(|reason| reason.contains("random upgrade opportunity")));

    let decision = plan_event_decision_v1(&context, &EventPolicyConfigV1::default());
    assert!(matches!(
        decision.action,
        EventPolicyActionV1::Pick { index: 0, .. }
    ));
}

#[test]
fn random_upgrade_opportunity_is_scoped_to_shining_light_event() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.current_hp = 72;
    run.max_hp = 80;
    run.master_deck.clear();
    run.master_deck
        .push(CombatCard::for_scenario(CardId::TrueGrit, 1));
    run.master_deck
        .push(CombatCard::for_scenario(CardId::Armaments, 2));

    let context = build_event_decision_context_v1(
        &run,
        EventId::MindBloom,
        vec![option(
            "[Test] Lose HP. Upgrade a card.",
            EventActionKind::Special,
            vec![
                EventEffect::LoseHp(16),
                EventEffect::UpgradeCard { count: 1 },
            ],
            EventOptionTransition::AdvanceScreen,
        )],
    );

    assert_eq!(
        context.candidates[0].class,
        EventPolicyClassV1::SelectionOrDeckMutation
    );
    assert!(context.candidates[0].random_upgrade_opportunity.is_none());
}

#[test]
fn cursed_tome_plan_compiler_projects_public_plans_and_effective_costs() {
    let mut run = RunState::new(1, 15, false, "Ironclad");
    run.current_hp = 80;
    run.max_hp = 80;
    run.event_state = Some(EventState::new(EventId::CursedTome));
    run.relics.push(RelicState::new(RelicId::TungstenRod));

    let plans = compile_event_plan_candidates_v1(&run, EventInformationModeV1::PublicOnly);

    let leave = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::LeaveNow)
        .expect("Cursed Tome should expose immediate leave");
    assert_eq!(leave.cost.effective_hp_loss, 0);

    let stop = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::CursedTomeReadThenStop)
        .expect("Cursed Tome should expose read then stop");
    assert_eq!(
        stop.steps
            .iter()
            .map(|step| (step.screen, step.choice_index))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0), (3, 0), (4, 1)]
    );
    assert_eq!(stop.cost.nominal_hp_loss, 9);
    assert_eq!(stop.cost.effective_hp_loss, 5);

    let take = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::CursedTomeReadAndTakeBook)
        .expect("Cursed Tome should expose read and take book");
    assert_eq!(
        take.steps
            .iter()
            .map(|step| (step.screen, step.choice_index))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]
    );
    assert_eq!(take.cost.nominal_hp_loss, 21);
    assert_eq!(take.cost.effective_hp_loss, 17);
    assert!(matches!(
        take.reward,
        EventPlanRewardV1::RandomBookRelic { observed: None }
    ));
    assert!(take.oracle_evidence.is_none());
}

#[test]
fn cursed_tome_oracle_peek_records_book_without_polluting_real_rng() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.current_hp = 80;
    run.max_hp = 80;
    run.event_state = Some(EventState::new(EventId::CursedTome));
    let misc_counter_before = run.rng_pool.misc_rng.counter;

    let plans =
        compile_event_plan_candidates_v1(&run, EventInformationModeV1::CounterfactualOracle);

    assert_eq!(run.rng_pool.misc_rng.counter, misc_counter_before);
    assert_eq!(
        run.event_state.as_ref().unwrap().current_screen,
        0,
        "oracle peek must not advance the real event state"
    );

    let take = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::CursedTomeReadAndTakeBook)
        .expect("Cursed Tome should expose read and take book");
    let Some(oracle) = &take.oracle_evidence else {
        panic!("oracle mode should attach observed book evidence");
    };
    assert_eq!(oracle.event_id, EventId::CursedTome);
    assert!(!oracle.committed);
    assert_eq!(oracle.misc_rng_delta_if_committed, 1);
    assert!(matches!(
        take.reward,
        EventPlanRewardV1::RandomBookRelic { observed: Some(_) }
    ));
}

#[test]
fn cursed_tome_plan_selector_takes_book_when_effective_hp_buffer_is_safe() {
    let mut run = RunState::new(1, 15, false, "Ironclad");
    run.current_hp = 80;
    run.max_hp = 80;
    run.event_state = Some(EventState::new(EventId::CursedTome));

    let selected = select_event_plan_candidate_v1(
        &run,
        EventInformationModeV1::CounterfactualOracle,
        &EventPolicyConfigV1::default(),
    )
    .expect("Cursed Tome should have a selected plan");

    assert_eq!(selected.plan_id, EventPlanIdV1::CursedTomeReadAndTakeBook);
    assert!(selected.oracle_evidence.is_some());
}

#[test]
fn cursed_tome_plan_selector_leaves_before_reading_when_take_buffer_is_unsafe() {
    let mut run = RunState::new(1, 15, false, "Ironclad");
    run.current_hp = 25;
    run.max_hp = 80;
    run.event_state = Some(EventState::new(EventId::CursedTome));

    let selected = select_event_plan_candidate_v1(
        &run,
        EventInformationModeV1::PublicOnly,
        &EventPolicyConfigV1::default(),
    )
    .expect("Cursed Tome should have a selected plan");

    assert_eq!(selected.plan_id, EventPlanIdV1::LeaveNow);
}

#[test]
fn unsupported_repeatable_paid_shape_is_explicit_not_empty_fallback() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    let mut event_state = EventState::new(EventId::BigFish);
    event_state.internal_state = crate::content::events::big_fish::init_big_fish_state(&run);
    run.event_state = Some(event_state);
    let status = compile_event_plan_status_v1(
        &run,
        &EventDecisionShapeV1::RepeatablePaidMenu(RepeatablePaidMenuShapeV1 {
            exit_index: 1,
            exit_cost_hp: 0,
            paid_option_indices: vec![0],
        }),
        EventInformationModeV1::PublicOnly,
    );

    assert!(matches!(
        status,
        EventPlanCompileStatusV1::UnsupportedShape {
            event_id: EventId::BigFish,
            shape: EventPlanUnsupportedShapeV1::RepeatablePaidMenu,
        }
    ));
}

#[test]
fn scrap_ooze_plan_exposes_repeated_gamble_as_optional_elite_like_risk() {
    let mut run = RunState::new(1, 15, false, "Ironclad");
    run.current_hp = 72;
    run.max_hp = 80;
    run.event_state = Some(EventState::new(EventId::ScrapOoze));

    let plans = compile_event_plan_candidates_v1(&run, EventInformationModeV1::PublicOnly);
    let reach = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::ScrapOozeReachInOnce)
        .expect("Scrap Ooze should expose a reach-in plan");

    assert_eq!(
        reach
            .steps
            .iter()
            .map(|step| (step.screen, step.choice_index))
            .collect::<Vec<_>>(),
        vec![(0, 0)]
    );
    assert_eq!(reach.cost.nominal_hp_loss, 5);
    assert_eq!(reach.cost.effective_hp_loss, 5);
    assert!(matches!(reach.reward, EventPlanRewardV1::RandomRelic));
    assert!(matches!(
        &reach.risk_model,
        EventPlanRiskModelV1::RepeatedGamble {
            current_success_chance_percent: 25,
            current_effective_hp_loss: 5,
            next_effective_hp_loss: 6,
            treat_as_optional_elite: true,
            ..
        }
    ));
}

#[test]
fn scrap_ooze_oracle_reports_attempts_until_success_without_polluting_real_state() {
    let mut run = RunState::new(1, 15, false, "Ironclad");
    run.current_hp = 72;
    run.max_hp = 80;
    run.event_state = Some(EventState::new(EventId::ScrapOoze));
    let hp_before = run.current_hp;
    let relic_count_before = run.relics.len();
    let misc_counter_before = run.rng_pool.misc_rng.counter;

    let plans =
        compile_event_plan_candidates_v1(&run, EventInformationModeV1::CounterfactualOracle);

    assert_eq!(run.current_hp, hp_before);
    assert_eq!(run.relics.len(), relic_count_before);
    assert_eq!(run.rng_pool.misc_rng.counter, misc_counter_before);

    let reach = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::ScrapOozeReachInOnce)
        .expect("Scrap Ooze should expose a reach-in plan");
    let Some(oracle) = &reach.oracle_evidence else {
        panic!("oracle mode should attach Scrap Ooze outcome evidence");
    };
    assert_eq!(oracle.event_id, EventId::ScrapOoze);
    assert!(!oracle.committed);
    match &oracle.outcome {
        EventOracleOutcomeV1::ScrapOoze {
            attempts_until_success,
            failed_attempts_before_stop,
            effective_hp_loss_if_committed,
            ..
        } => {
            assert!(
                attempts_until_success.is_some() || *failed_attempts_before_stop > 0,
                "oracle should either find the success attempt or report known failed attempts"
            );
            assert!(*effective_hp_loss_if_committed >= 0);
        }
        other => panic!("unexpected oracle outcome: {other:?}"),
    }
}

#[test]
fn dead_adventurer_plan_exposes_search_as_optional_elite_risk() {
    let mut run = RunState::new(1, 15, false, "Ironclad");
    run.current_hp = 72;
    run.max_hp = 80;
    let mut event_state = EventState::new(EventId::DeadAdventurer);
    event_state.internal_state = dead_adventurer_internal_state_for_test(0, 35, [0, 1, 2], 2);
    run.event_state = Some(event_state);

    let plans = compile_event_plan_candidates_v1(&run, EventInformationModeV1::PublicOnly);
    let search = plans
        .iter()
        .find(|plan| plan.plan_id == EventPlanIdV1::DeadAdventurerSearchAsOptionalElite)
        .expect("Dead Adventurer should expose a search plan");

    assert_eq!(
        search
            .steps
            .iter()
            .map(|step| (step.screen, step.choice_index))
            .collect::<Vec<_>>(),
        vec![(0, 0)]
    );
    assert!(matches!(
        &search.risk_model,
        EventPlanRiskModelV1::OptionalEliteLike {
            fight_chance_percent: 35,
            reward_already_obtained: false,
            encounter: Some(encounter),
            ..
        } if encounter.encounter_id == crate::content::monsters::factory::EncounterId::LagavulinEvent
            && encounter.publicly_revealed
            && encounter.starts_awake
    ));
}

#[test]
fn dead_adventurer_selector_leaves_after_relic_reward_was_obtained() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.current_hp = 72;
    run.max_hp = 80;
    let mut event_state = EventState::new(EventId::DeadAdventurer);
    event_state.internal_state = dead_adventurer_internal_state_for_test(1, 50, [2, 0, 1], 0);
    run.event_state = Some(event_state);

    let selected = select_event_plan_candidate_v1(
        &run,
        EventInformationModeV1::PublicOnly,
        &EventPolicyConfigV1::default(),
    )
    .expect("Dead Adventurer should have a selected plan");

    assert_eq!(selected.plan_id, EventPlanIdV1::DeadAdventurerLeaveNow);
}

fn dead_adventurer_internal_state_for_test(
    num_rewards: i32,
    encounter_chance: i32,
    reward_types: [i32; 3],
    enemy: i32,
) -> i32 {
    (num_rewards & 0xF)
        | ((encounter_chance & 0xFF) << 4)
        | ((reward_types[0] & 0x3) << 12)
        | ((reward_types[1] & 0x3) << 14)
        | ((reward_types[2] & 0x3) << 16)
        | ((enemy & 0x3) << 18)
}

fn option(
    label: &'static str,
    action: EventActionKind,
    effects: Vec<EventEffect>,
    transition: EventOptionTransition,
) -> EventOption {
    EventOption::new(
        EventChoiceMeta::new(label),
        EventOptionSemantics {
            action,
            effects,
            constraints: Vec::new(),
            transition,
            repeatable: false,
            terminal: transition == EventOptionTransition::Complete,
        },
    )
}
