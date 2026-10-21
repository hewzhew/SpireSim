use super::*;
use crate::ai::strategy::deck_admission::DeckAdmissionContext;
use crate::ai::strategy::reward_admission::{
    assess_reward_admission_from_master_deck, skip_reward_admission,
};
use crate::ai::strategy::run_strategic_facts::RunStrategicFacts;
use crate::content::monsters::factory::EncounterId;
use crate::runtime::combat::CombatCard;

#[test]
fn starter_attack_credit_does_not_veto_needed_act_one_damage_rewards() {
    let mut cards = vec![CardId::AscendersBane];
    cards.extend([CardId::Strike; 5]);
    cards.extend([CardId::Defend; 4]);
    cards.extend([CardId::Bash, CardId::Armaments]);
    for (hp, candidate) in [(71, CardId::Anger), (80, CardId::Hemokinesis)] {
        let mut context = reward_context_with_act_and_hp(&cards, 1, hp, 80);
        context.deck_plan = context
            .deck_plan
            .with_boss_key(Some(EncounterId::Hexaghost));
        assert_eq!(
            context.deck_plan.construction.frontload,
            FrontloadPressure::NeedMore
        );
        assert_eq!(
            context.deck_plan.strategic_deficit.frontload_damage,
            StrategicDeficitLevel::Adequate
        );
        let admission = assess_reward_admission_from_master_deck(&test_deck(&cards), candidate, 0);
        let evaluation = evaluate_decision_candidate(
            context,
            DecisionCandidateKind::CardRewardPick {
                card: candidate,
                upgrades: 0,
            },
            Some(&admission),
        );
        let acquisition = assess_card_acquisition(
            AcquisitionContext::reward(context.deck_plan),
            candidate,
            0,
            &admission,
        );
        assert!(evaluate_deck_construction_contract(&acquisition).allows_acquisition());
        assert!(
            !evaluation
                .adjudication
                .caps
                .iter()
                .any(|cap| cap.source == CandidateLaneCapSource::Acquisition),
            "{evaluation:?}"
        );
        cards.extend([CardId::PowerThrough, CardId::Headbutt]);
    }
}

#[test]
fn accepted_first_persistent_mitigation_is_not_vetoed_by_coarse_deck_burden() {
    // Deck from the current automated A20 run's F37 reward. The baseline
    // skipped Disarm despite having no persistent enemy attack reduction.
    let cards = [
        (CardId::AscendersBane, 0),
        (CardId::Strike, 0),
        (CardId::Strike, 0),
        (CardId::Defend, 0),
        (CardId::Defend, 0),
        (CardId::Defend, 0),
        (CardId::Defend, 0),
        (CardId::Bash, 0),
        (CardId::DemonForm, 0),
        (CardId::Cleave, 0),
        (CardId::BattleTrance, 1),
        (CardId::TwinStrike, 1),
        (CardId::ShrugItOff, 1),
        (CardId::Reaper, 0),
        (CardId::BurningPact, 1),
        (CardId::Flex, 1),
        (CardId::Headbutt, 0),
    ];
    let deck = cards
        .iter()
        .enumerate()
        .map(|(i, &(id, upgrades))| {
            let mut card = CombatCard::for_scenario(id, i as u32 + 1);
            for _ in 0..upgrades {
                crate::content::cards::upgrade_card_once_java(&mut card);
            }
            card
        })
        .collect::<Vec<_>>();
    let context = DecisionPipelineContext::reward(
        DeckPlanSnapshot::from_deck(
            &deck,
            DeckAdmissionContext {
                act: 3,
                current_hp: 72,
                max_hp: 72,
            },
            RunStrategicFacts {
                entering_act: 3,
                starter_basic_count: 6,
                curse_count: 1,
                has_energy_relic: true,
                has_runic_pyramid: false,
            },
        )
        .with_boss_key(Some(EncounterId::DonuAndDeca)),
    );
    let admission = assess_reward_admission_from_master_deck(&deck, CardId::Disarm, 0);
    let acquisition = assess_card_acquisition(
        AcquisitionContext::reward(context.deck_plan),
        CardId::Disarm,
        0,
        &admission,
    );
    assert_eq!(
        evaluate_deck_construction_contract(&acquisition).verdict,
        AcquisitionPolicyVerdict::ContextTake,
        "{acquisition:?}"
    );
    let evaluation = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::CardRewardPick {
            card: CardId::Disarm,
            upgrades: 0,
        },
        Some(&admission),
    );
    assert_eq!(evaluation.lane, CandidateLane::Mainline, "{evaluation:?}");
    assert!(evaluation.auto_expands(), "{evaluation:?}");
    let skip = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::CardRewardSkip,
        Some(&skip_reward_admission()),
    );
    assert!(evaluation.auto_order_key(true) < skip.auto_order_key(true));
}

fn shop_context(cards: &[CardId]) -> DecisionPipelineContext {
    shop_context_with_gold_and_hp(cards, 999, 70, 80)
}

fn shop_context_with_hp(cards: &[CardId], current_hp: i32, max_hp: i32) -> DecisionPipelineContext {
    shop_context_with_gold_and_hp(cards, 999, current_hp, max_hp)
}

fn shop_context_with_gold_and_hp(
    cards: &[CardId],
    gold: i32,
    current_hp: i32,
    max_hp: i32,
) -> DecisionPipelineContext {
    let deck: Vec<_> = cards
        .iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(*card, index as u32 + 1))
        .collect();
    DecisionPipelineContext::shop(
        DeckPlanSnapshot::from_deck(
            &deck,
            DeckAdmissionContext {
                act: 2,
                current_hp,
                max_hp,
            },
            RunStrategicFacts {
                entering_act: 3,
                starter_basic_count: 0,
                curse_count: 0,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        ),
        gold,
    )
}

fn reward_context_with_act(cards: &[CardId], act: u8) -> DecisionPipelineContext {
    reward_context_with_act_and_hp(cards, act, 70, 80)
}

fn reward_context_with_act_and_hp(
    cards: &[CardId],
    act: u8,
    current_hp: i32,
    max_hp: i32,
) -> DecisionPipelineContext {
    let deck = test_deck(cards);
    DecisionPipelineContext::reward(DeckPlanSnapshot::from_deck(
        &deck,
        DeckAdmissionContext {
            act,
            current_hp,
            max_hp,
        },
        RunStrategicFacts {
            entering_act: act,
            starter_basic_count: deck
                .iter()
                .filter(|card| matches!(card.id, CardId::Strike | CardId::Defend))
                .count(),
            curse_count: 0,
            has_energy_relic: false,
            has_runic_pyramid: false,
        },
    ))
}

fn reward_context_with_act_boss(
    cards: &[CardId],
    act: u8,
    boss: EncounterId,
) -> DecisionPipelineContext {
    let deck = test_deck(cards);
    DecisionPipelineContext::reward(
        DeckPlanSnapshot::from_deck(
            &deck,
            DeckAdmissionContext {
                act,
                current_hp: 70,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: act,
                starter_basic_count: deck
                    .iter()
                    .filter(|card| matches!(card.id, CardId::Strike | CardId::Defend))
                    .count(),
                curse_count: 0,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        )
        .with_boss_key(Some(boss)),
    )
}

fn reward_card_with_act(
    cards: &[CardId],
    candidate: CardId,
    upgrades: u8,
    act: u8,
) -> CandidateEvaluation {
    let deck = test_deck(cards);
    let context = reward_context_with_act(cards, act);
    let admission = assess_reward_admission_from_master_deck(&deck, candidate, upgrades);
    evaluate_decision_candidate(
        context,
        DecisionCandidateKind::CardRewardPick {
            card: candidate,
            upgrades,
        },
        Some(&admission),
    )
}

fn reward_card_with_act_and_hp(
    cards: &[CardId],
    candidate: CardId,
    upgrades: u8,
    act: u8,
    current_hp: i32,
    max_hp: i32,
) -> CandidateEvaluation {
    let deck = test_deck(cards);
    let context = reward_context_with_act_and_hp(cards, act, current_hp, max_hp);
    let admission = assess_reward_admission_from_master_deck(&deck, candidate, upgrades);
    evaluate_decision_candidate(
        context,
        DecisionCandidateKind::CardRewardPick {
            card: candidate,
            upgrades,
        },
        Some(&admission),
    )
}

fn reward_card_with_act_boss(
    cards: &[CardId],
    candidate: CardId,
    upgrades: u8,
    act: u8,
    boss: EncounterId,
) -> CandidateEvaluation {
    let deck = test_deck(cards);
    let context = reward_context_with_act_boss(cards, act, boss);
    let admission = assess_reward_admission_from_master_deck(&deck, candidate, upgrades);
    evaluate_decision_candidate(
        context,
        DecisionCandidateKind::CardRewardPick {
            card: candidate,
            upgrades,
        },
        Some(&admission),
    )
}

fn shop_relic(context: DecisionPipelineContext, relic: RelicId) -> CandidateEvaluation {
    evaluate_decision_candidate(
        context,
        DecisionCandidateKind::ShopBuyRelic { relic, price: 150 },
        None,
    )
}

fn shop_card(cards: &[CardId], candidate: CardId) -> CandidateEvaluation {
    shop_card_with_upgrades(cards, candidate, 0)
}

fn shop_card_with_upgrades(
    cards: &[CardId],
    candidate: CardId,
    upgrades: u8,
) -> CandidateEvaluation {
    let deck = test_deck(cards);
    let context = shop_context(cards);
    shop_card_in_context(context, &deck, candidate, upgrades)
}

fn shop_card_in_context(
    context: DecisionPipelineContext,
    deck: &[CombatCard],
    candidate: CardId,
    upgrades: u8,
) -> CandidateEvaluation {
    shop_card_in_context_with_price(context, deck, candidate, upgrades, 80)
}

fn shop_card_in_context_with_price(
    context: DecisionPipelineContext,
    deck: &[CombatCard],
    candidate: CardId,
    upgrades: u8,
    price: i32,
) -> CandidateEvaluation {
    let admission = assess_reward_admission_from_master_deck(deck, candidate, upgrades);
    evaluate_decision_candidate(
        context,
        DecisionCandidateKind::ShopBuyCard {
            card: candidate,
            upgrades,
            price,
        },
        Some(&admission),
    )
}

fn test_deck(cards: &[CardId]) -> Vec<CombatCard> {
    cards
        .iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(*card, index as u32 + 1))
        .collect()
}

fn act2_collector_pressure_deck() -> Vec<CardId> {
    vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Armaments,
        CardId::Cleave,
        CardId::IronWave,
        CardId::Shockwave,
        CardId::BattleTrance,
        CardId::Whirlwind,
        CardId::ShrugItOff,
        CardId::Inflame,
        CardId::ShrugItOff,
    ]
}

fn act1_low_margin_reward_deck() -> Vec<CardId> {
    vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Immolate,
        CardId::IronWave,
        CardId::Cleave,
        CardId::ShrugItOff,
        CardId::PommelStrike,
        CardId::Bloodletting,
    ]
}

fn low_hp_heavy_burden_deck() -> Vec<CardId> {
    vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::PommelStrike,
        CardId::ShrugItOff,
        CardId::Armaments,
        CardId::Cleave,
        CardId::Cleave,
        CardId::Rupture,
        CardId::Apparition,
        CardId::Apparition,
        CardId::Apparition,
        CardId::Apparition,
        CardId::Apparition,
        CardId::Hemokinesis,
        CardId::ShrugItOff,
        CardId::Offering,
    ]
}

fn seed_20260713003_act3_floor42_card_ids() -> Vec<CardId> {
    vec![
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Clothesline,
        CardId::BattleTrance,
        CardId::Armaments,
        CardId::FlameBarrier,
        CardId::Cleave,
        CardId::PowerThrough,
        CardId::FlameBarrier,
        CardId::Brutality,
        CardId::GhostlyArmor,
        CardId::DemonForm,
        CardId::BodySlam,
        CardId::Headbutt,
        CardId::ShrugItOff,
        CardId::SeeingRed,
        CardId::HeavyBlade,
        CardId::Flex,
        CardId::IronWave,
    ]
}

#[test]
fn reward_low_margin_filler_cannot_enter_mainline_after_basic_roles_exist() {
    let deck = act1_low_margin_reward_deck();

    let iron_wave = reward_card_with_act(&deck, CardId::IronWave, 0, 1);

    assert_eq!(iron_wave.adjudication.raw_lane, CandidateLane::Mainline);
    assert_eq!(iron_wave.adjudication.final_lane, CandidateLane::Probe);
    assert!(iron_wave
        .adjudication
        .caps
        .iter()
        .any(|cap| cap.source == CandidateLaneCapSource::Acquisition));
    assert_ne!(
        iron_wave.lane,
        CandidateLane::Mainline,
        "low-margin filler should not be promoted by stacked weak evidence: {:?}",
        iron_wave.scores
    );
}

#[test]
fn guardian_clothesline_can_clear_acquisition_cap_as_first_weak_answer() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
    ];

    let clothesline =
        reward_card_with_act_boss(&deck, CardId::Clothesline, 0, 1, EncounterId::TheGuardian);

    assert_eq!(clothesline.adjudication.raw_lane, CandidateLane::Mainline);
    assert_eq!(
        clothesline.adjudication.final_lane,
        CandidateLane::Mainline,
        "known Guardian survival evidence must not be discarded by the generic filler cap: {:?}",
        clothesline
    );
}

#[test]
fn guardian_flame_barrier_can_clear_acquisition_cap_as_first_substantial_block() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::BattleTrance,
        CardId::Armaments,
    ];

    let flame_barrier =
        reward_card_with_act_boss(&deck, CardId::FlameBarrier, 0, 1, EncounterId::TheGuardian);

    assert_eq!(flame_barrier.adjudication.raw_lane, CandidateLane::Mainline);
    assert_eq!(
        flame_barrier.adjudication.final_lane,
        CandidateLane::Mainline,
        "the first substantial Guardian block answer must remain executable: {:?}",
        flame_barrier
    );
}

#[test]
fn low_hp_survival_stabilizer_can_clear_low_margin_acquisition_cap() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::BattleTrance,
        CardId::Armaments,
        CardId::Cleave,
        CardId::Headbutt,
    ];

    let iron_wave = reward_card_with_act_and_hp(&deck, CardId::IronWave, 0, 2, 42, 80);

    assert_eq!(iron_wave.adjudication.raw_lane, CandidateLane::Mainline);
    assert_eq!(
        iron_wave.adjudication.final_lane,
        CandidateLane::Mainline,
        "actual HP pressure must make an immediate survival stabilizer executable: {:?}",
        iron_wave
    );
}

#[test]
fn reward_skip_sorts_before_probe_when_no_mainline_take_exists() {
    let deck = act1_low_margin_reward_deck();
    let iron_wave = reward_card_with_act(&deck, CardId::IronWave, 0, 1);
    let thunderclap = reward_card_with_act(&deck, CardId::ThunderClap, 0, 1);
    let skip = evaluate_decision_candidate(
        reward_context_with_act(&deck, 1),
        DecisionCandidateKind::CardRewardSkip,
        Some(&crate::ai::strategy::reward_admission::skip_reward_admission()),
    );

    assert_eq!(iron_wave.lane, CandidateLane::Probe);
    assert_eq!(thunderclap.lane, CandidateLane::Probe);
    assert_eq!(skip.lane, CandidateLane::Skip);
    assert!(
        skip.order_key(false) < iron_wave.order_key(false),
        "skip should outrank probe when there is no mainline take"
    );
    assert!(
        skip.order_key(false) < thunderclap.order_key(false),
        "skip should outrank every probe filler when there is no mainline take"
    );
}

#[test]
fn reward_probe_filler_is_inspect_only_not_auto_expandable() {
    let deck = act1_low_margin_reward_deck();
    let iron_wave = reward_card_with_act(&deck, CardId::IronWave, 0, 1);

    assert_eq!(iron_wave.lane, CandidateLane::Probe);
    assert_eq!(
        iron_wave.inspect_only_reason(),
        Some("card reward pick is below mainline"),
        "probe reward picks may be visible for review, but must not auto-expand"
    );
}

#[test]
fn reward_low_margin_filler_does_not_mainline_for_only_soft_gap_contact() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Bash,
        CardId::Immolate,
        CardId::Cleave,
        CardId::PommelStrike,
    ];

    let iron_wave = reward_card_with_act(&deck, CardId::IronWave, 0, 1);

    assert_ne!(
        iron_wave.lane,
        CandidateLane::Mainline,
        "low-margin filler should not mainline just because it touches a soft gap: {:?}",
        iron_wave.scores
    );
}

#[test]
fn status_digest_removes_hard_rejection_without_auto_taking_second_wild_strike() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::WildStrike,
        CardId::Evolve,
    ];

    let wild_strike = reward_card_with_act(&deck, CardId::WildStrike, 0, 1);
    let skip = evaluate_decision_candidate(
        reward_context_with_act(&deck, 1),
        DecisionCandidateKind::CardRewardSkip,
        Some(&crate::ai::strategy::reward_admission::skip_reward_admission()),
    );

    assert_eq!(wild_strike.adjudication.raw_lane, CandidateLane::Probe);
    assert_eq!(wild_strike.lane, CandidateLane::Probe);
    assert!(wild_strike
        .scores
        .iter()
        .any(|score| score.by == "status-liability-digested"));
    assert!(skip.order_key(false) < wild_strike.order_key(false));
}

#[test]
fn shop_boss_scaling_repair_remains_mainline() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Clothesline,
        CardId::BattleTrance,
        CardId::Armaments,
        CardId::FlameBarrier,
        CardId::Cleave,
        CardId::PowerThrough,
        CardId::FlameBarrier,
        CardId::Brutality,
        CardId::GhostlyArmor,
    ];
    let deck = test_deck(&cards);
    let context_at_entry = DecisionPipelineContext::shop(
        DeckPlanSnapshot::from_deck(
            &deck,
            DeckAdmissionContext {
                act: 2,
                current_hp: 75,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: 3,
                starter_basic_count: 6,
                curse_count: 0,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        )
        .with_boss_key(Some(EncounterId::TheChamp)),
        313,
    );
    let demon_form =
        shop_card_in_context_with_price(context_at_entry, &deck, CardId::DemonForm, 0, 139);
    assert_eq!(demon_form.lane, CandidateLane::Mainline);

    let context_after_cleanup =
        DecisionPipelineContext::shop_with_purge_reserve(context_at_entry.deck_plan, 213, None);
    let demon_after_cleanup =
        shop_card_in_context_with_price(context_after_cleanup, &deck, CardId::DemonForm, 0, 139);
    assert_eq!(
        demon_after_cleanup.lane,
        CandidateLane::Mainline,
        "demon_after_cleanup={demon_after_cleanup:#?}"
    );
    assert!(demon_after_cleanup.auto_expands());
}

#[test]
fn low_hp_shop_scaling_repair_remains_below_mainline() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::PommelStrike,
        CardId::ShrugItOff,
        CardId::Cleave,
    ];
    let deck = test_deck(&cards);
    let context = DecisionPipelineContext::shop(
        DeckPlanSnapshot::from_deck(
            &deck,
            DeckAdmissionContext {
                act: 2,
                current_hp: 12,
                max_hp: 39,
            },
            RunStrategicFacts {
                entering_act: 3,
                starter_basic_count: 7,
                curse_count: 0,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        )
        .with_boss_key(Some(EncounterId::TheChamp)),
        213,
    );

    let demon_form = shop_card_in_context_with_price(context, &deck, CardId::DemonForm, 0, 139);

    assert_ne!(demon_form.lane, CandidateLane::Mainline);
}

#[test]
fn shop_power_potion_scores_as_premium_discovery_potion() {
    let context = shop_context(&act1_low_margin_reward_deck());
    let power_potion = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::ShopBuyPotion {
            potion: PotionId::PowerPotion,
            price: 78,
        },
        None,
    );
    let attack_potion = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::ShopBuyPotion {
            potion: PotionId::AttackPotion,
            price: 51,
        },
        None,
    );
    let block_potion = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::ShopBuyPotion {
            potion: PotionId::BlockPotion,
            price: 51,
        },
        None,
    );

    assert!(
            power_potion.total_score() > attack_potion.total_score()
                && attack_potion.total_score() > block_potion.total_score(),
            "Power Potion should be premium discovery, while Attack Potion remains useful access: power={:?} attack={:?} block={:?}",
            power_potion.scores,
            attack_potion.scores,
            block_potion.scores
        );
}

#[test]
fn survival_pressure_prefers_energy_over_explosive_potion() {
    let context = shop_context_with_hp(&act1_low_margin_reward_deck(), 12, 39);
    assert!(context.deck_plan.survival_pressure());
    let explosive = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::ShopBuyPotion {
            potion: PotionId::ExplosivePotion,
            price: 50,
        },
        None,
    );
    let energy = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::ShopBuyPotion {
            potion: PotionId::EnergyPotion,
            price: 50,
        },
        None,
    );
    let shop_potion_score = |evaluation: &CandidateEvaluation| {
        evaluation
            .scores
            .iter()
            .find(|component| component.by == "shop-potion")
            .expect("shop potion evaluation should expose its score")
            .value
    };

    let explosive_score = shop_potion_score(&explosive);
    let energy_score = shop_potion_score(&energy);
    assert!(
            energy_score > explosive_score,
            "survival pressure should prefer reusable emergency access over fixed AoE: energy={energy_score} explosive={explosive_score}"
        );
}

#[test]
fn unsupported_rupture_shop_purchase_is_not_mainline() {
    let rupture = shop_card(
        &[CardId::Strike, CardId::Defend, CardId::Bash],
        CardId::Rupture,
    );

    assert_ne!(rupture.lane, CandidateLane::Mainline);
}

#[test]
fn low_hp_pure_block_survives_heavy_burden_lane_cap() {
    let deck = low_hp_heavy_burden_deck();
    let flame_barrier = reward_card_with_act_and_hp(&deck, CardId::FlameBarrier, 1, 3, 12, 39);

    assert_eq!(
        flame_barrier.lane,
        CandidateLane::Mainline,
        "flame_barrier={flame_barrier:#?}"
    );
    assert!(!flame_barrier
        .adjudication
        .caps
        .iter()
        .any(|cap| cap.source == CandidateLaneCapSource::Strategic));
}

#[test]
fn acute_survival_distinguishes_deterministic_burst_from_a_fuel_backed_block_engine() {
    let deck = seed_20260713003_act3_floor42_card_ids();

    let impervious = reward_card_with_act_and_hp(&deck, CardId::Impervious, 1, 3, 22, 82);
    let second_wind = reward_card_with_act_and_hp(&deck, CardId::SecondWind, 1, 3, 22, 82);
    let ghostly_armor = reward_card_with_act_and_hp(&deck, CardId::GhostlyArmor, 0, 3, 22, 82);
    let healthy_impervious = reward_card_with_act_and_hp(&deck, CardId::Impervious, 1, 3, 70, 82);

    assert_eq!(
        impervious.lane,
        CandidateLane::Mainline,
        "a deterministic 40-block reward must be executable at 22/82 HP: {impervious:#?}"
    );
    assert!(impervious.scores.iter().any(|component| {
        component.by == "acute-survival-block-density" && component.value > 0
    }));
    assert!(acute_survival_block_density(CardId::Impervious, 1) > 0);
    assert_eq!(acute_survival_block_density(CardId::SecondWind, 1), 0);
    assert_eq!(acute_survival_block_density(CardId::GhostlyArmor, 0), 0);
    assert_eq!(acute_survival_block_density(CardId::PowerThrough, 0), 0);
    assert!(acute_survival_block_density(CardId::PowerThrough, 1) > 0);
    assert_eq!(
        second_wind.lane,
        CandidateLane::Mainline,
        "the fixture has enough non-attack fuel for a real Second Wind engine: {second_wind:#?}"
    );
    assert!(second_wind
        .scores
        .iter()
        .any(|component| component.by == "fuel-backed-second-wind-block-plan"));
    assert!(!second_wind
        .scores
        .iter()
        .any(|component| component.by == "acute-survival-block-density"));
    assert_ne!(
        ghostly_armor.lane,
        CandidateLane::Mainline,
        "ordinary solid block must not inherit burst-block credit: {ghostly_armor:#?}"
    );
    assert!(!ghostly_armor
        .scores
        .iter()
        .any(|component| component.by == "acute-survival-block-density"));
    assert!(!healthy_impervious
        .scores
        .iter()
        .any(|component| component.by == "acute-survival-block-density"));
}

#[test]
fn low_hp_redundant_rupture_cannot_enter_mainline() {
    let deck = low_hp_heavy_burden_deck();
    let rupture = reward_card_with_act_and_hp(&deck, CardId::Rupture, 1, 3, 13, 39);

    assert_ne!(rupture.lane, CandidateLane::Mainline);
}

#[test]
fn low_hp_setup_only_scaling_is_capped_below_mainline() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::PommelStrike,
        CardId::ShrugItOff,
        CardId::Cleave,
    ];
    let demon_form = reward_card_with_act_and_hp(&deck, CardId::DemonForm, 0, 3, 12, 39);

    assert_ne!(demon_form.lane, CandidateLane::Mainline);
    assert!(demon_form.adjudication.caps.iter().any(|cap| {
        cap.source == CandidateLaneCapSource::Strategic && cap.cap == LaneCap::ProbeOnly
    }));
}

#[test]
fn reward_prefers_supported_corruption_engine_over_feed_run_reward_before_act2_boss() {
    let cards = vec![
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Immolate,
        CardId::Cleave,
        CardId::ShrugItOff,
        CardId::PommelStrike,
        CardId::Bloodletting,
        CardId::Armaments,
        CardId::Impervious,
        CardId::SpotWeakness,
    ];

    let feed = reward_card_with_act(&cards, CardId::Feed, 0, 2);
    let corruption = reward_card_with_act(&cards, CardId::Corruption, 0, 2);

    assert!(
            corruption.order_key(false) < feed.order_key(false),
            "supported Corruption engine should outrank Feed run reward before Act2 boss: corruption={:?} feed={:?}",
            corruption.scores,
            feed.scores
        );
}

#[test]
fn seed006_single_controlled_exhaust_supports_but_does_not_complete_a_payoff_engine() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Berserk,
        CardId::ShrugItOff,
        CardId::Clothesline,
        CardId::PommelStrike,
        CardId::BattleTrance,
        CardId::Cleave,
        CardId::TrueGrit,
        CardId::Uppercut,
    ];

    let dark_embrace =
        reward_card_with_act_boss(&cards, CardId::DarkEmbrace, 1, 2, EncounterId::TheChamp);
    let burning_pact =
        reward_card_with_act_boss(&cards, CardId::BurningPact, 1, 2, EncounterId::TheChamp);

    assert_eq!(dark_embrace.lane, CandidateLane::Probe);
    assert!(dark_embrace.scores.iter().any(|component| {
        component.by == "boss-limited-exhaust-synergy" && component.value > 0
    }));
    assert_eq!(burning_pact.lane, CandidateLane::Mainline);
    assert!(
            burning_pact.order_key(true) < dark_embrace.order_key(true),
            "one True Grit is limited exhaust access; another draw-and-exhaust source remains executable while the first payoff stays speculative: dark_embrace={dark_embrace:#?} burning_pact={burning_pact:#?}"
        );
}

#[test]
fn first_real_draw_stays_mainline_in_thin_cantrip_deck() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Armaments,
        CardId::PommelStrike,
        CardId::ShrugItOff,
        CardId::Feed,
        CardId::Corruption,
        CardId::Cleave,
        CardId::Cleave,
    ];

    let battle_trance = reward_card_with_act(&cards, CardId::BattleTrance, 0, 3);
    let cleave = reward_card_with_act(&cards, CardId::Cleave, 0, 3);

    assert_eq!(
        battle_trance.lane,
        CandidateLane::Mainline,
        "battle_trance={battle_trance:#?}"
    );
    assert!(battle_trance.auto_expands());
    assert!(
            battle_trance.order_key(true) < cleave.order_key(true),
            "first real draw should outrank another Cleave: battle_trance={battle_trance:#?} cleave={cleave:#?}"
        );
}

#[test]
fn repeated_weak_aoe_cannot_enter_mainline_as_act2_gap_repair() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::BattleTrance,
        CardId::Armaments,
        CardId::PommelStrike,
        CardId::Cleave,
        CardId::Cleave,
    ];

    let cleave = reward_card_with_act(&cards, CardId::Cleave, 0, 2);
    let whirlwind = reward_card_with_act(&cards, CardId::Whirlwind, 0, 2);

    assert_ne!(cleave.lane, CandidateLane::Mainline, "cleave={cleave:#?}");
    assert!(!cleave.auto_expands(), "cleave={cleave:#?}");
    assert!(
        !cleave
            .scores
            .iter()
            .any(|score| score.by == "strategic-aoe-gap"),
        "cleave={cleave:#?}"
    );
    assert_eq!(whirlwind.lane, CandidateLane::Mainline);
    assert!(whirlwind.auto_expands(), "whirlwind={whirlwind:#?}");
}

#[test]
fn known_collector_minion_control_outranks_first_real_draw() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Armaments,
        CardId::PommelStrike,
        CardId::ShrugItOff,
        CardId::Feed,
        CardId::Cleave,
        CardId::Corruption,
    ];

    let cleave = reward_card_with_act_boss(&cards, CardId::Cleave, 0, 2, EncounterId::Collector);
    let battle_trance =
        reward_card_with_act_boss(&cards, CardId::BattleTrance, 1, 2, EncounterId::Collector);

    assert_eq!(cleave.lane, CandidateLane::Mainline, "cleave={cleave:#?}");
    assert_eq!(
        battle_trance.lane,
        CandidateLane::Mainline,
        "battle_trance={battle_trance:#?}"
    );
    assert!(
            cleave.order_key(true) < battle_trance.order_key(true),
            "known Collector minion control should win this comparison: cleave={cleave:#?} battle_trance={battle_trance:#?}"
        );
}

#[test]
fn seed006_f35_dark_shackles_is_recognized_as_boss_survival_bridge() {
    let cards = [
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Berserk,
        CardId::Clothesline,
        CardId::Feed,
        CardId::BattleTrance,
        CardId::Armaments,
        CardId::ShrugItOff,
        CardId::MasterOfStrategy,
        CardId::Inflame,
    ];
    let mut deck = test_deck(&cards);
    for card in &mut deck {
        if matches!(
            card.id,
            CardId::Bash
                | CardId::Clothesline
                | CardId::BattleTrance
                | CardId::Armaments
                | CardId::ShrugItOff
                | CardId::MasterOfStrategy
                | CardId::Inflame
        ) {
            card.upgrades = 1;
        }
    }
    let context = DecisionPipelineContext::shop(
        DeckPlanSnapshot::from_deck(
            &deck,
            DeckAdmissionContext {
                act: 3,
                current_hp: 110,
                max_hp: 110,
            },
            RunStrategicFacts {
                entering_act: 3,
                starter_basic_count: 6,
                curse_count: 0,
                has_energy_relic: true,
                has_runic_pyramid: true,
            },
        )
        .with_boss_key(Some(EncounterId::AwakenedOne)),
        180,
    );

    let shackles = shop_card_in_context_with_price(context, &deck, CardId::DarkShackles, 1, 78);

    assert_eq!(shackles.lane, CandidateLane::Mainline, "{shackles:#?}");
    assert!(shackles
        .scores
        .iter()
        .any(|score| score.by == "awakened-one-temporary-strength-timed-bridge"));
}

#[test]
fn reward_automaton_context_keeps_shockwave_as_boss_support() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Immolate,
        CardId::Cleave,
        CardId::ShrugItOff,
        CardId::PommelStrike,
        CardId::Bloodletting,
        CardId::FiendFire,
        CardId::SpotWeakness,
        CardId::SpotWeakness,
        CardId::Offering,
    ];

    let shockwave =
        reward_card_with_act_boss(&cards, CardId::Shockwave, 0, 2, EncounterId::Automaton);

    assert_eq!(
        shockwave.lane,
        CandidateLane::Mainline,
        "shockwave={shockwave:#?}"
    );
    assert!(shockwave.auto_expands(), "shockwave={shockwave:?}");
    assert!(
        shockwave
            .scores
            .iter()
            .any(|score| score.by == "automaton-artifact-debuff-window"),
        "shockwave scores should expose Automaton boss support: {:?}",
        shockwave.scores
    );
}

#[test]
fn reward_awakened_one_context_promotes_true_survival_repair() {
    let cards = vec![
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Headbutt,
        CardId::Armaments,
        CardId::BurningPact,
        CardId::Whirlwind,
        CardId::Feed,
        CardId::ShrugItOff,
        CardId::Cleave,
        CardId::DemonForm,
        CardId::Rupture,
        CardId::Feed,
    ];

    let disarm = reward_card_with_act_boss(&cards, CardId::Disarm, 0, 3, EncounterId::AwakenedOne);

    assert_eq!(disarm.lane, CandidateLane::Mainline);
    assert!(disarm.auto_expands(), "disarm={disarm:?}");
    assert!(
        disarm
            .scores
            .iter()
            .any(|score| score.by == "awakened-one-strength-down-survival"),
        "Disarm should expose Awakened One survival repair: {:?}",
        disarm.scores
    );
}

#[test]
fn reward_awakened_one_context_keeps_generic_block_access_distinct_from_survival_repair() {
    let cards = vec![
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Headbutt,
        CardId::Armaments,
        CardId::BurningPact,
        CardId::Whirlwind,
        CardId::Feed,
        CardId::ShrugItOff,
        CardId::Cleave,
        CardId::DemonForm,
        CardId::Rupture,
        CardId::Feed,
    ];

    let shrug =
        reward_card_with_act_boss(&cards, CardId::ShrugItOff, 1, 3, EncounterId::AwakenedOne);

    assert!(
        shrug
            .scores
            .iter()
            .any(|score| score.by == "awakened-one-generic-block-access"),
        "Shrug+ may receive generic block/access support: {:?}",
        shrug.scores
    );
    assert!(
        !shrug.scores.iter().any(|score| matches!(
            score.by,
            "awakened-one-strength-down-survival"
                | "awakened-one-weak-strength-down-survival"
                | "awakened-one-dark-echo-block-plan"
                | "awakened-one-repeatable-block-plan"
        )),
        "Shrug+ should not masquerade as a boss survival repair: {:?}",
        shrug.scores
    );
}

#[test]
fn reward_awakened_one_context_admits_first_supported_strength_payoff() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Clothesline,
        CardId::BattleTrance,
        CardId::Armaments,
        CardId::FlameBarrier,
        CardId::Cleave,
        CardId::PowerThrough,
        CardId::FlameBarrier,
        CardId::Brutality,
        CardId::GhostlyArmor,
        CardId::DemonForm,
        CardId::BodySlam,
        CardId::Headbutt,
        CardId::ShrugItOff,
        CardId::SeeingRed,
    ];

    let heavy_blade =
        reward_card_with_act_boss(&cards, CardId::HeavyBlade, 0, 3, EncounterId::AwakenedOne);
    let sword_boomerang = reward_card_with_act_boss(
        &cards,
        CardId::SwordBoomerang,
        0,
        3,
        EncounterId::AwakenedOne,
    );

    assert_eq!(heavy_blade.lane, CandidateLane::Mainline);
    assert!(heavy_blade.auto_expands(), "heavy_blade={heavy_blade:#?}");
    assert!(!heavy_blade.adjudication.caps.iter().any(|cap| {
        cap.source == CandidateLaneCapSource::Acquisition && cap.cap == LaneCap::ProbeOnly
    }));
    assert_ne!(
        sword_boomerang.lane,
        CandidateLane::Mainline,
        "low-margin payoff should remain disciplined: {sword_boomerang:#?}"
    );
}

#[test]
fn reward_supported_block_engine_admits_zero_cost_block_payoff() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::BurningPact,
        CardId::PowerThrough,
        CardId::SecondWind,
        CardId::FeelNoPain,
    ];

    let body_slam = reward_card_with_act_boss(&deck, CardId::BodySlam, 1, 2, EncounterId::TheChamp);

    assert_eq!(
            body_slam.lane,
            CandidateLane::Mainline,
            "a zero-cost block payoff should be actionable once the deck has a real block engine: {body_slam:#?}"
        );
}

#[test]
fn reward_starter_block_does_not_claim_supported_body_slam_package() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
    ];

    let body_slam =
        reward_card_with_act_boss(&deck, CardId::BodySlam, 1, 1, EncounterId::TheGuardian);

    assert_ne!(
        body_slam.lane,
        CandidateLane::Mainline,
        "starter Defends alone must not certify a supported block-payoff package: {body_slam:#?}"
    );
}

#[test]
fn reward_supported_exhaust_conversion_preserves_fiend_fire_independent_value() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::BurningPact,
        CardId::PowerThrough,
        CardId::Inflame,
        CardId::Offering,
        CardId::SecondWind,
        CardId::FeelNoPain,
        CardId::Disarm,
        CardId::BodySlam,
        CardId::PommelStrike,
        CardId::HeavyBlade,
        CardId::BurningPact,
    ];

    let fiend_fire =
        reward_card_with_act_boss(&deck, CardId::FiendFire, 0, 2, EncounterId::TheChamp);

    assert_eq!(
            fiend_fire.lane,
            CandidateLane::Mainline,
            "supported exhaust conversion must survive an unrelated strength-payoff fragility check: {fiend_fire:#?}"
        );
}

#[test]
fn reward_fiend_fire_without_exhaust_payoff_is_not_promoted_by_conversion_support() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Immolate,
        CardId::Cleave,
        CardId::PommelStrike,
        CardId::BattleTrance,
        CardId::Inflame,
        CardId::HeavyBlade,
    ];

    let fiend_fire =
        reward_card_with_act_boss(&deck, CardId::FiendFire, 0, 2, EncounterId::TheChamp);

    assert_ne!(
        fiend_fire.lane,
        CandidateLane::Mainline,
        "hand exhaust alone must not impersonate a supported exhaust conversion: {fiend_fire:#?}"
    );
}

#[test]
fn shop_paper_frog_beats_chemical_x_without_x_cost_payoff() {
    let context = shop_context(&[
        CardId::Shockwave,
        CardId::Uppercut,
        CardId::Cleave,
        CardId::Cleave,
    ]);

    let chemical_x = shop_relic(context, RelicId::ChemicalX);
    let paper_frog = shop_relic(context, RelicId::PaperFrog);

    assert!(
            paper_frog.total_score() > chemical_x.total_score(),
            "Paper Frog should beat dead Chemical X with vulnerable/AoE support: Paper Frog={:?}, Chemical X={:?}",
            paper_frog.scores,
            chemical_x.scores
        );
    assert_ne!(chemical_x.lane, CandidateLane::Mainline);
}

#[test]
fn shop_chemical_x_stays_mainline_with_x_cost_payoff() {
    let context = shop_context(&[
        CardId::Shockwave,
        CardId::Uppercut,
        CardId::Cleave,
        CardId::Whirlwind,
    ]);

    let chemical_x = shop_relic(context, RelicId::ChemicalX);

    assert_eq!(chemical_x.lane, CandidateLane::Mainline);
}

#[test]
fn shop_rejects_ordinary_unupgraded_transition_card_without_gap() {
    let deck = act2_collector_pressure_deck();

    let clothesline = shop_card(&deck, CardId::Clothesline);
    let iron_wave = shop_card(&deck, CardId::IronWave);

    assert_eq!(
        clothesline.inspect_only_reason(),
        Some("shop card has no acquisition policy support"),
        "clothesline={clothesline:#?}"
    );
    assert_eq!(
        iron_wave.inspect_only_reason(),
        Some("shop card has no acquisition policy support")
    );
}

#[test]
fn shop_keeps_premium_access_card_eligible() {
    let deck = act2_collector_pressure_deck();

    let master_of_strategy = shop_card(&deck, CardId::MasterOfStrategy);

    assert_ne!(
        master_of_strategy.inspect_only_reason(),
        Some("shop card fails acquisition discipline")
    );
}

#[test]
fn shop_keeps_second_stable_source_for_live_strength_multiplier_package() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Cleave,
        CardId::Cleave,
        CardId::Inflame,
        CardId::LimitBreak,
        CardId::HeavyBlade,
        CardId::BurningPact,
        CardId::BattleTrance,
        CardId::Armaments,
        CardId::Disarm,
    ];
    let deck = test_deck(&cards);
    let context = shop_context_with_gold_and_hp(&cards, 152, 47, 79);

    let inflame = shop_card_in_context_with_price(context, &deck, CardId::Inflame, 0, 81);

    assert!(inflame.auto_expands(), "inflame={inflame:?}");
    assert!(
        !inflame
            .adjudication
            .caps
            .iter()
            .any(|cap| cap.source == CandidateLaneCapSource::Acquisition),
        "inflame={inflame:?}"
    );
}

#[test]
fn reward_keeps_first_stable_strength_source_out_of_payoff_saturation() {
    let cards = vec![
        CardId::HeavyBlade,
        CardId::Reaper,
        CardId::BattleTrance,
        CardId::FlameBarrier,
    ];

    let inflame = reward_card_with_act(&cards, CardId::Inflame, 1, 3);

    assert_eq!(inflame.lane, CandidateLane::Mainline, "inflame={inflame:?}");
    assert!(
        !inflame.adjudication.caps.iter().any(|cap| matches!(
            cap.source,
            CandidateLaneCapSource::RoleSaturation | CandidateLaneCapSource::Acquisition
        )),
        "inflame={inflame:?}"
    );
}

#[test]
fn shop_rejects_act2_ordinary_cards_that_only_pad_adequate_roles() {
    let deck = act2_collector_pressure_deck();

    let clothesline = shop_card(&deck, CardId::Clothesline);
    let spot_weakness = shop_card(&deck, CardId::SpotWeakness);

    assert_eq!(
        clothesline.inspect_only_reason(),
        Some("shop card has no acquisition policy support"),
        "clothesline={clothesline:#?}"
    );
    assert_eq!(
        spot_weakness.inspect_only_reason(),
        Some("shop card has no acquisition policy support"),
        "spot_weakness={spot_weakness:#?}"
    );
}

#[test]
fn shop_low_hp_does_not_turn_ordinary_cards_into_emergency_buys() {
    let cards = act2_collector_pressure_deck();
    let deck = test_deck(&cards);
    let context = shop_context_with_hp(&cards, 24, 90);

    let shrug = shop_card_in_context(context, &deck, CardId::ShrugItOff, 0);
    let clothesline = shop_card_in_context(context, &deck, CardId::Clothesline, 0);

    assert_eq!(
        shrug.inspect_only_reason(),
        Some("shop card has no acquisition policy support")
    );
    assert_eq!(
        clothesline.inspect_only_reason(),
        Some("shop card has no acquisition policy support")
    );
}

#[test]
fn act2_second_wind_sees_gap_behind_starter_defends() {
    let deck = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Headbutt,
        CardId::Cleave,
        CardId::Offering,
        CardId::Offering,
    ];

    let master_deck = test_deck(&deck);
    let context = DecisionPipelineContext::reward(DeckPlanSnapshot::from_deck(
        &master_deck,
        DeckAdmissionContext {
            act: 2,
            current_hp: 52,
            max_hp: 74,
        },
        RunStrategicFacts {
            entering_act: 3,
            starter_basic_count: 7,
            curse_count: 0,
            has_energy_relic: false,
            has_runic_pyramid: false,
        },
    ));
    let admission = assess_reward_admission_from_master_deck(&master_deck, CardId::SecondWind, 0);
    let second_wind = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::CardRewardPick {
            card: CardId::SecondWind,
            upgrades: 0,
        },
        Some(&admission),
    );

    assert!(
        !second_wind
            .scores
            .iter()
            .any(|score| score.by == "strategic-survival-gap"),
        "Second Wind needs its fuel-aware assessment, not an unconditional Block bonus: {:?}",
        second_wind.scores
    );
    assert!(
        second_wind
            .scores
            .iter()
            .any(|score| score.by == "fuel-backed-second-wind-block-plan"),
        "four or more real non-attacks should expose the executable Second Wind block plan: {:?}",
        second_wind.scores
    );
}

#[test]
fn second_wind_without_non_attack_fuel_does_not_claim_a_block_plan() {
    let master_deck = test_deck(&[
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Bash,
        CardId::Defend,
    ]);
    let context = DecisionPipelineContext::reward(DeckPlanSnapshot::from_deck(
        &master_deck,
        DeckAdmissionContext {
            act: 1,
            current_hp: 70,
            max_hp: 80,
        },
        RunStrategicFacts {
            entering_act: 2,
            starter_basic_count: 6,
            curse_count: 0,
            has_energy_relic: false,
            has_runic_pyramid: false,
        },
    ));
    let admission = assess_reward_admission_from_master_deck(&master_deck, CardId::SecondWind, 0);
    let second_wind = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::CardRewardPick {
            card: CardId::SecondWind,
            upgrades: 0,
        },
        Some(&admission),
    );

    assert!(
            !second_wind
                .scores
                .iter()
                .any(|score| score.by == "fuel-backed-second-wind-block-plan"),
            "conditional mass exhaust must not be presented as a complete block plan without fuel: {:?}",
            second_wind.scores
        );
}

#[test]
fn cheap_survival_access_shop_admission_reaches_mainline_lane() {
    let cards = vec![
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Berserk,
    ];
    let deck = test_deck(&cards);
    let plan = DeckPlanSnapshot::from_deck(
        &deck,
        DeckAdmissionContext {
            act: 1,
            current_hp: 80,
            max_hp: 80,
        },
        RunStrategicFacts {
            entering_act: 2,
            starter_basic_count: 8,
            curse_count: 0,
            has_energy_relic: false,
            has_runic_pyramid: false,
        },
    );
    let context = DecisionPipelineContext::shop_with_purge_reserve(plan, 43, None);

    let shrug = shop_card_in_context_with_price(context, &deck, CardId::ShrugItOff, 0, 25);

    assert_eq!(shrug.inspect_only_reason(), None, "shrug={shrug:#?}");
    assert_eq!(shrug.lane, CandidateLane::Mainline, "shrug={shrug:#?}");
}

#[test]
fn early_shop_does_not_treat_strike_purge_as_frontload_gain() {
    let cards = [
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Defend,
        CardId::Bash,
        CardId::Disarm,
    ];
    let context = DecisionPipelineContext::shop(reward_context_with_act(&cards, 1).deck_plan, 117);
    assert_eq!(
        context.deck_plan.construction.frontload,
        FrontloadPressure::NeedMore
    );

    let strike = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::ShopPurge {
            target: CleanupTarget::StarterStrike,
        },
        None,
    );
    let curse = evaluate_decision_candidate(
        context,
        DecisionCandidateKind::ShopPurge {
            target: CleanupTarget::Curse,
        },
        None,
    );
    assert_eq!(strike.lane, CandidateLane::Reject, "{strike:#?}");
    assert_eq!(curse.lane, CandidateLane::Mainline, "{curse:#?}");
}
