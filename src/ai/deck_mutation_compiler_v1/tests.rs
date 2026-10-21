use crate::ai::deck_mutation_compiler_v1::{
    best_duplicate_target_for_shop_v1, compile_deck_mutation_decision_v1,
    deck_removal_target_snapshots_v1, render_compiled_deck_mutation_decision_v1,
    DeckMutationCompilerRequestV1, DeckMutationPlanRoleV1, DeckMutationRemovalBenefitTierV1,
    DeckMutationTargetLossTierV1, DuplicateTargetRoleV1, TransformRandomAdditionBandV1,
    TransformVarianceRiskV1,
};
use crate::ai::upgrade_planner_v1::{
    upgrade_candidate_for_deck_index_v1, upgrade_candidate_score_hint_v1,
};
use crate::content::cards::CardId;
use crate::content::relics::{RelicId, RelicState};
use crate::runtime::combat::CombatCard;
use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::events::EventId;
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

#[test]
fn removal_snapshots_preserve_redundant_functional_loss_tier() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.master_deck = vec![
        CombatCard::for_scenario(CardId::Flex, 1),
        CombatCard::for_scenario(CardId::Flex, 2),
    ];

    let snapshots = deck_removal_target_snapshots_v1(&run);

    assert_eq!(snapshots.len(), 2);
    assert!(snapshots.iter().all(|snapshot| {
        snapshot.target_loss.tier == DeckMutationTargetLossTierV1::RedundantFunctional
    }));
}

#[test]
fn removal_snapshots_keep_benefit_separate_from_function_loss() {
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.master_deck = vec![
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::Writhe, 2),
    ];
    run.relics.push(RelicState::new(RelicId::DuVuDoll));

    let snapshots = deck_removal_target_snapshots_v1(&run);
    let strike = snapshots
        .iter()
        .find(|snapshot| snapshot.card == CardId::Strike)
        .expect("Strike snapshot");
    let writhe = snapshots
        .iter()
        .find(|snapshot| snapshot.card == CardId::Writhe)
        .expect("Writhe snapshot");

    assert_eq!(
        strike.removal_benefit.tier,
        DeckMutationRemovalBenefitTierV1::LowValueCard
    );
    assert_eq!(
        writhe.removal_benefit.tier,
        DeckMutationRemovalBenefitTierV1::ForcedOpeningObstruction
    );
    assert_eq!(
        writhe.target_loss.tier,
        DeckMutationTargetLossTierV1::LowValue
    );
    assert!(writhe
        .removal_benefit
        .compensating_signals
        .iter()
        .any(|signal| signal == "du_vu_doll_strength"));
}

#[test]
fn compiler_marks_functional_purge_inspect_only_when_low_value_targets_exist() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state
        .master_deck
        .push(CombatCard::for_scenario(CardId::TrueGrit, 99));
    let choice = choice(RunPendingChoiceReason::Purge, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(12),
    );

    assert!(decision.inspect_only_plans.iter().any(|plan| plan
        .step
        .cards
        .iter()
        .any(|card| card.card == CardId::TrueGrit)));
    assert!(!decision.branch_active_plans.iter().any(|plan| plan
        .step
        .cards
        .iter()
        .any(|card| card.card == CardId::TrueGrit)));
}

#[test]
fn compiler_keeps_low_value_purge_targets_branch_active() {
    let run_state = RunState::new(1, 0, false, "Ironclad");
    let choice = choice(RunPendingChoiceReason::Purge, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(12),
    );

    let commands = decision
        .branch_active_plans
        .iter()
        .map(|plan| (plan.step.command.as_str(), plan.step.cards[0].card))
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        vec![("select 0", CardId::Strike), ("select 5", CardId::Defend),]
    );
    assert!(!decision.branch_active_plans.iter().any(|plan| plan
        .step
        .cards
        .iter()
        .any(|card| card.card == CardId::Bash)));
}

#[test]
fn compiler_execute_one_selects_evaluated_executable_plan() {
    let run_state = RunState::new(1, 0, false, "Ironclad");
    let choice = choice(RunPendingChoiceReason::PurgeNonBottled, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_execute_one(),
    );
    let selected = decision.selected_plan.expect("selected plan");

    assert_eq!(selected.step.command, "select 0");
    assert!(selected.allowed_consumers.execute_autopilot);
    assert!(decision
        .candidate_plans
        .iter()
        .any(|candidate| candidate.plan_id == selected.plan_id));
}

#[test]
fn critical_bonfire_offer_trades_a_redundant_uncommon_for_full_recovery() {
    for event_id in [EventId::BonfireElementals, EventId::BonfireSpirits] {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.act_num = 3;
        run_state.floor_num = 36;
        run_state.current_hp = 12;
        run_state.max_hp = 80;
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::DarkEmbrace, 2),
            CombatCard::for_scenario(CardId::DarkEmbrace, 3),
        ];
        let choice = RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 1,
            reason: RunPendingChoiceReason::PurgeNonBottled,
            source: DomainEventSource::Event(event_id),
            return_state: Box::new(EngineState::EventRoom),
        };

        let decision = compile_deck_mutation_decision_v1(
            &run_state,
            &choice,
            DeckMutationCompilerRequestV1::committed_forced_execute_one(),
        );
        let selected = decision.selected_plan.expect("bonfire sacrifice target");

        assert_eq!(selected.step.cards[0].card, CardId::DarkEmbrace);
        assert_eq!(selected.role, DeckMutationPlanRoleV1::PolicyPreferred);
        assert!(selected.reasons.iter().any(|reason| {
            reason.contains("bonfire_offer_crosses_survival_reserve")
                && reason.contains("projected_hp=80")
        }));
    }
}

#[test]
fn material_bonfire_recovery_outranks_a_common_cleanup_above_reserve() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.act_num = 3;
    run_state.floor_num = 36;
    run_state.current_hp = 30;
    run_state.max_hp = 85;
    run_state.master_deck = vec![
        CombatCard::for_scenario(CardId::Cleave, 1),
        CombatCard::for_scenario(CardId::Uppercut, 2),
        CombatCard::for_scenario(CardId::Uppercut, 3),
        CombatCard::for_scenario(CardId::FiendFire, 4),
    ];
    let choice = RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason: RunPendingChoiceReason::PurgeNonBottled,
        source: DomainEventSource::Event(EventId::BonfireElementals),
        return_state: Box::new(EngineState::EventRoom),
    };

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );
    let selected = decision
        .selected_plan
        .as_ref()
        .expect("bonfire sacrifice target");

    assert_eq!(
        selected.step.cards[0].card,
        CardId::Uppercut,
        "{}",
        render_compiled_deck_mutation_decision_v1(&decision)
    );
    assert_eq!(selected.role, DeckMutationPlanRoleV1::PolicyPreferred);
    assert!(decision
        .candidate_plans
        .iter()
        .find(|candidate| candidate.step.cards[0].card == CardId::FiendFire)
        .is_some_and(|candidate| {
            candidate.step.cards[0].target_loss.tier == DeckMutationTargetLossTierV1::CoreFunctional
                && candidate.role != DeckMutationPlanRoleV1::PolicyPreferred
        }));
}

#[test]
fn healthy_bonfire_offer_keeps_the_ordinary_low_loss_removal_order() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.current_hp = 70;
    run_state.max_hp = 80;
    run_state.master_deck = vec![
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::DarkEmbrace, 2),
        CombatCard::for_scenario(CardId::DarkEmbrace, 3),
    ];
    let choice = RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason: RunPendingChoiceReason::PurgeNonBottled,
        source: DomainEventSource::Event(EventId::BonfireElementals),
        return_state: Box::new(EngineState::EventRoom),
    };

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );

    assert_eq!(
        decision.selected_plan.unwrap().step.cards[0].card,
        CardId::Strike
    );
}

#[test]
fn bonfire_recovery_order_respects_mark_of_the_bloom() {
    use crate::content::relics::{RelicId, RelicState};

    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.current_hp = 12;
    run_state.max_hp = 80;
    run_state.master_deck = vec![
        CombatCard::for_scenario(CardId::Strike, 1),
        CombatCard::for_scenario(CardId::DarkEmbrace, 2),
        CombatCard::for_scenario(CardId::DarkEmbrace, 3),
    ];
    run_state
        .relics
        .push(RelicState::new(RelicId::MarkOfTheBloom));
    let choice = RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason: RunPendingChoiceReason::PurgeNonBottled,
        source: DomainEventSource::Event(EventId::BonfireElementals),
        return_state: Box::new(EngineState::EventRoom),
    };

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );

    assert_eq!(
        decision.selected_plan.unwrap().step.cards[0].card,
        CardId::Strike
    );
}

#[test]
fn transform_compiler_keeps_low_value_targets_branchable_without_autopilot_execution() {
    let run_state = RunState::new(1, 0, false, "Ironclad");
    let choice = choice(RunPendingChoiceReason::TransformNonBottled, 1);

    let branch_decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(12),
    );

    let strike_transform = branch_decision
        .branch_active_plans
        .iter()
        .find(|plan| plan.step.cards[0].card == CardId::Strike)
        .expect("starter Strike transform should stay branchable");
    assert_eq!(
        strike_transform.step.cards[0]
            .transform
            .random_addition_band,
        TransformRandomAdditionBandV1::LikelyBetterThanTarget
    );
    assert_eq!(
        strike_transform.step.cards[0].transform.variance_risk,
        TransformVarianceRiskV1::Medium
    );
    assert!(!strike_transform.allowed_consumers.execute_autopilot);
    assert!(strike_transform
        .reasons
        .iter()
        .any(|reason| reason.contains("transform_random_addition_band")));

    let execute_decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_execute_one(),
    );
    assert!(
        execute_decision.selected_plan.is_none(),
        "transform is a random deck mutation and should not share purge's autopilot gate"
    );
}

#[test]
fn committed_forced_transform_selects_least_bad_legal_target() {
    let run_state = RunState::new(1, 0, false, "Ironclad");
    let choice = choice(RunPendingChoiceReason::TransformNonBottled, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );
    let selected = decision
        .selected_plan
        .expect("committed forced transform should select a legal target");

    assert_eq!(selected.step.cards[0].card, CardId::Strike);
    assert!(selected
        .reasons
        .iter()
        .any(|reason| reason == "commitment_mode=CommittedForced"));
    assert!(selected
        .risks
        .iter()
        .any(|risk| risk == "committed_forced_selected_non_autopilot_target"));
}

#[test]
fn transform_compiler_marks_functional_targets_as_high_variance() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.add_card_to_deck(CardId::TrueGrit);
    let choice = choice(RunPendingChoiceReason::TransformNonBottled, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(12),
    );
    let true_grit = decision
        .candidate_plans
        .iter()
        .find(|plan| plan.step.cards[0].card == CardId::TrueGrit)
        .expect("True Grit transform candidate should exist");

    assert_eq!(
        true_grit.step.cards[0].transform.random_addition_band,
        TransformRandomAdditionBandV1::Mixed
    );
    assert_eq!(
        true_grit.step.cards[0].transform.variance_risk,
        TransformVarianceRiskV1::High
    );
    assert!(matches!(
        true_grit.role,
        DeckMutationPlanRoleV1::InspectOnly | DeckMutationPlanRoleV1::RiskyExploration
    ));
    assert!(true_grit
        .risks
        .iter()
        .any(|risk| risk == "transform_variance_risk=High"));
}

#[test]
fn compiler_render_exposes_active_and_inspect_only_plan_groups() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state
        .master_deck
        .push(CombatCard::for_scenario(CardId::TrueGrit, 99));
    let choice = choice(RunPendingChoiceReason::Purge, 1);
    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(12),
    );

    let rendered = render_compiled_deck_mutation_decision_v1(&decision);

    assert!(rendered.contains("Deck mutation evidence:"));
    assert!(rendered.contains("execution: head="));
    assert!(rendered.contains("scheduler: branch_active:"));
    assert!(rendered.contains("candidate_pool: inspect_only:"));
    assert!(rendered.contains("True Grit"));
    assert!(rendered.contains("role=InspectOnly"));
}

#[test]
fn compiler_exposes_target_loss_for_functional_mutation_targets() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state
        .master_deck
        .push(CombatCard::for_scenario(CardId::BurningPact, 90));
    run_state
        .master_deck
        .push(CombatCard::for_scenario(CardId::TrueGrit, 91));
    run_state
        .master_deck
        .push(CombatCard::for_scenario(CardId::TrueGrit, 92));
    let choice = choice(RunPendingChoiceReason::Purge, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(16),
    );

    let burning_pact = decision
        .candidate_plans
        .iter()
        .find(|plan| plan.step.cards[0].card == CardId::BurningPact)
        .expect("Burning Pact candidate");
    let true_grit = decision
        .candidate_plans
        .iter()
        .find(|plan| plan.step.cards[0].card == CardId::TrueGrit)
        .expect("True Grit candidate");

    assert_eq!(
        burning_pact.step.cards[0].target_loss.tier,
        DeckMutationTargetLossTierV1::CoreFunctional
    );
    assert_eq!(
        true_grit.step.cards[0].target_loss.tier,
        DeckMutationTargetLossTierV1::Functional
    );
    assert!(
        true_grit.score_hint > burning_pact.score_hint,
        "redundant functional target should be ranked before singleton core functional target"
    );
    assert!(burning_pact
        .reasons
        .iter()
        .any(|reason| reason.contains("target_loss=CoreFunctional")));
}

#[test]
fn forced_purge_preserves_the_last_multiplier_of_a_live_strength_package() {
    let mut run_state = RunState::new(20260713004, 0, false, "Ironclad");
    run_state.act_num = 3;
    run_state.master_deck = [
        (CardId::Bash, 1),
        (CardId::Reaper, 1),
        (CardId::Headbutt, 0),
        (CardId::Disarm, 0),
        (CardId::Bloodletting, 0),
        (CardId::SpotWeakness, 1),
        (CardId::Clothesline, 1),
        (CardId::Offering, 0),
        (CardId::Barricade, 0),
        (CardId::SearingBlow, 1),
        (CardId::SecondWind, 0),
        (CardId::WildStrike, 1),
        (CardId::LimitBreak, 0),
        (CardId::Reaper, 1),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, upgrades))| {
        let mut card = CombatCard::for_scenario(id, index as u32 + 1);
        card.upgrades = upgrades;
        card
    })
    .collect();

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice(RunPendingChoiceReason::PurgeNonBottled, 1),
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );

    let limit_break = decision
        .candidate_plans
        .iter()
        .find(|plan| plan.step.cards[0].card == CardId::LimitBreak)
        .expect("Limit Break purge candidate");
    assert_eq!(
        limit_break.step.cards[0].target_loss.tier,
        DeckMutationTargetLossTierV1::CoreFunctional,
        "the last multiplier paired with a real strength source is a live package component"
    );
    assert!(limit_break.step.cards[0]
        .target_loss
        .signals
        .iter()
        .any(|signal| signal == "breaks_live_strength_package"));
    assert_ne!(
        decision.selected_plan.unwrap().step.cards[0].card,
        CardId::LimitBreak,
        "a forced purge should sacrifice an ordinary functional card before breaking live scaling"
    );
}

#[test]
fn forced_purge_preserves_the_last_high_quality_block_chunk() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.master_deck = vec![
        CombatCard::for_scenario(CardId::PowerThrough, 1),
        CombatCard::for_scenario(CardId::Headbutt, 2),
    ];

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice(RunPendingChoiceReason::PurgeNonBottled, 1),
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );
    let power_through = decision
        .candidate_plans
        .iter()
        .find(|plan| plan.step.cards[0].card == CardId::PowerThrough)
        .expect("Power Through purge candidate");

    assert_eq!(
        power_through.step.cards[0].target_loss.tier,
        DeckMutationTargetLossTierV1::CoreFunctional
    );
    assert!(power_through.step.cards[0]
        .target_loss
        .signals
        .iter()
        .any(|signal| signal == "removes_last_high_quality_block"));
    assert_eq!(
        decision.selected_plan.unwrap().step.cards[0].card,
        CardId::Headbutt
    );
}

#[test]
fn equal_mutation_candidates_use_numeric_deck_index_not_command_text() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.master_deck = (0..12)
        .map(|index| {
            let id = match index {
                2 => CardId::Headbutt,
                11 => CardId::LimitBreak,
                _ => CardId::BurningPact,
            };
            CombatCard::for_scenario(id, index as u32 + 1)
        })
        .collect();

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice(RunPendingChoiceReason::PurgeNonBottled, 1),
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );
    let headbutt_rank = decision
        .candidate_plans
        .iter()
        .position(|plan| plan.step.cards[0].card == CardId::Headbutt)
        .expect("Headbutt candidate");
    let limit_break_rank = decision
        .candidate_plans
        .iter()
        .position(|plan| plan.step.cards[0].card == CardId::LimitBreak)
        .expect("Limit Break candidate");

    assert!(
        headbutt_rank < limit_break_rank,
        "index 2 must sort before index 11 when the semantic scores are equal"
    );
}

#[test]
fn upgrade_compiler_scores_targets_from_upgrade_planner() {
    let run_state = RunState::new(1, 0, false, "Ironclad");
    let bash_index = run_state
        .master_deck
        .iter()
        .position(|card| card.id == CardId::Bash)
        .expect("starter deck should contain Bash");
    let choice = choice(RunPendingChoiceReason::Upgrade, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(12),
    );
    let bash_plan = decision
        .candidate_plans
        .iter()
        .find(|plan| plan.step.cards[0].deck_index == bash_index)
        .expect("Bash upgrade candidate");
    let upgrade_candidate =
        upgrade_candidate_for_deck_index_v1(&run_state, bash_index).expect("planner candidate");

    assert_eq!(
        bash_plan.score_hint,
        upgrade_candidate_score_hint_v1(&upgrade_candidate),
        "deck mutation upgrade score must be sourced from UpgradePlanner"
    );
    assert!(
        bash_plan
            .reasons
            .iter()
            .any(|reason| reason.starts_with("upgrade_plan: ")),
        "upgrade candidates should expose UpgradePlanner evidence"
    );
}

#[test]
fn duplicate_shop_target_requires_premium_duplicate_role() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");

    assert!(
        best_duplicate_target_for_shop_v1(&run_state).is_none(),
        "starter deck should not make Dolly's Mirror a high-impact purchase"
    );

    run_state.add_card_to_deck(CardId::Offering);
    let target = best_duplicate_target_for_shop_v1(&run_state).expect("premium duplicate target");

    assert_eq!(target.card, CardId::Offering);
    assert_eq!(target.role, DuplicateTargetRoleV1::SetupAccelerator);
    assert!(target.premium);
    assert!(target.priority >= 760);
}

#[test]
fn duplicate_compiler_exposes_target_role_evidence() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.add_card_to_deck(CardId::Offering);
    run_state.add_card_to_deck(CardId::ShrugItOff);
    let choice = choice(RunPendingChoiceReason::Duplicate, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(4),
    );
    let offering = decision
        .branch_active_plans
        .iter()
        .find(|plan| plan.step.cards[0].card == CardId::Offering)
        .expect("Offering duplicate candidate should remain visible");

    assert!(offering
        .reasons
        .iter()
        .any(|reason| reason == "duplicate_role=setup_accelerator"));
    assert!(offering
        .reasons
        .iter()
        .any(|reason| reason == "duplicate_premium=true"));
    assert!(
        decision
            .candidate_plans
            .iter()
            .find(|plan| plan.step.cards[0].card == CardId::ShrugItOff)
            .is_some_and(|plan| plan.risks.iter().any(|risk| {
                risk == "duplicate_good_reward_card_but_not_premium_mirror_target"
            })),
        "ordinary good reward cards should be explainable as non-premium mirror targets"
    );
}

#[test]
fn bottle_flame_compiler_does_not_rank_starter_strike_above_real_opening_target() {
    let run_state = RunState::new(1, 0, false, "Ironclad");
    let choice = choice(RunPendingChoiceReason::BottleFlame, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(3),
    );

    let first = decision
        .branch_active_plans
        .first()
        .expect("bottle target option");
    assert_eq!(first.step.cards[0].card, CardId::Bash);
    assert!(first
        .reasons
        .iter()
        .any(|reason| reason.contains("opening_hand_target_verdict")));
}

#[test]
fn bottle_compiler_keeps_best_bad_target_branchable_when_no_clean_target_exists() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.master_deck.retain(|card| card.id != CardId::Bash);
    let choice = choice(RunPendingChoiceReason::BottleFlame, 1);

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice,
        DeckMutationCompilerRequestV1::optional_branch_top_k(3),
    );

    assert!(
        !decision.branch_active_plans.is_empty(),
        "forced bottle choice should keep a risky fallback branch when every target is bad"
    );
    assert_eq!(
        decision.branch_active_plans[0].role,
        DeckMutationPlanRoleV1::RiskyExploration
    );
    assert!(decision.branch_active_plans[0]
        .risks
        .iter()
        .any(|risk| risk.contains("opening-hand debt")));
}

#[test]
fn execute_one_multi_selection_never_materializes_the_combination_domain() {
    let mut run_state = RunState::new(1, 0, false, "Ironclad");
    run_state.master_deck = (0..60)
        .map(|uuid| {
            let mut card = CombatCard::for_scenario(CardId::Strike, uuid);
            card.upgrades = uuid as u8;
            card
        })
        .collect();

    let decision = compile_deck_mutation_decision_v1(
        &run_state,
        &choice(RunPendingChoiceReason::TransformUpgraded, 3),
        DeckMutationCompilerRequestV1::committed_forced_execute_one(),
    );

    assert_eq!(decision.candidate_plans.len(), 1);
    assert_eq!(decision.selected_plan.unwrap().step.deck_indices.len(), 3);
}

fn choice(reason: RunPendingChoiceReason, count: usize) -> RunPendingChoiceState {
    RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: count,
        max_choices: count,
        reason,
        source: crate::state::selection::DomainEventSource::Selection(reason.into()),
        return_state: Box::new(EngineState::MapNavigation),
    }
}
