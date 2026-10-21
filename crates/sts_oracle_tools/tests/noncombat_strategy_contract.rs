use sts_oracle_runtime::ai::noncombat_strategy_v1::{
    build_run_strategy_snapshot_from_run_state_v2, threat_coverage_from_run_state_v1,
    StrategyCapabilityCoverageV1, StrategyCapabilityInputKindV1, StrategyCapabilityKindV1,
    StrategyDeckFormationNeedV1, StrategyThreatProfileV1, StrategyThreatSourceRecordV1,
    StrategyThreatSourceV1, StrategyThreatTagV1,
};
use sts_oracle_runtime::content::cards::CardId;
use sts_oracle_runtime::runtime::combat::CombatCard;
use sts_oracle_runtime::state::run::RunState;

fn one_threat(
    source: StrategyThreatSourceV1,
    subject: &str,
    tag: StrategyThreatTagV1,
) -> StrategyThreatProfileV1 {
    StrategyThreatProfileV1 {
        current_act: 1,
        tags: vec![tag],
        sources: vec![StrategyThreatSourceRecordV1 {
            source_act: 1,
            tag,
            source,
            subject: subject.to_string(),
            evidence: format!("{subject} requires {tag:?}"),
        }],
        ..StrategyThreatProfileV1::default()
    }
}

#[test]
fn exact_deck_aoe_source_closes_three_sentries_aoe_gap() {
    let threats = one_threat(
        StrategyThreatSourceV1::ActEliteEncounter,
        "ThreeSentries",
        StrategyThreatTagV1::AoEValuable,
    );
    let starter = RunState::new(1, 0, false, "Ironclad");
    let starter_coverage = threat_coverage_from_run_state_v1(&starter, &threats);
    assert!(starter_coverage.has_gap(
        StrategyThreatSourceV1::ActEliteEncounter,
        StrategyThreatTagV1::AoEValuable
    ));

    let mut with_whirlwind = starter;
    with_whirlwind
        .master_deck
        .push(CombatCard::for_scenario(CardId::Whirlwind, 99_001));
    let patched_coverage = threat_coverage_from_run_state_v1(&with_whirlwind, &threats);
    assert!(!patched_coverage.has_gap(
        StrategyThreatSourceV1::ActEliteEncounter,
        StrategyThreatTagV1::AoEValuable
    ));
}

#[test]
fn exact_deck_strength_down_closes_high_incoming_gap() {
    let threats = one_threat(
        StrategyThreatSourceV1::ActBoss,
        "TheChamp",
        StrategyThreatTagV1::HighIncomingDamage,
    );
    let starter = RunState::new(1, 0, false, "Ironclad");
    let starter_coverage = threat_coverage_from_run_state_v1(&starter, &threats);
    assert!(starter_coverage.has_gap(
        StrategyThreatSourceV1::ActBoss,
        StrategyThreatTagV1::HighIncomingDamage
    ));

    let mut with_shockwave = starter;
    with_shockwave
        .master_deck
        .push(CombatCard::for_scenario(CardId::Shockwave, 99_002));
    let patched_coverage = threat_coverage_from_run_state_v1(&with_shockwave, &threats);
    assert!(!patched_coverage.has_gap(
        StrategyThreatSourceV1::ActBoss,
        StrategyThreatTagV1::HighIncomingDamage
    ));
}

#[test]
fn low_impact_attack_addition_does_not_clear_frontload_formation_need() {
    for card in [CardId::Anger, CardId::PerfectedStrike] {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.add_card_to_deck(card);
        let formation = build_run_strategy_snapshot_from_run_state_v2(&run).formation_summary();

        assert!(
            formation
                .needs
                .contains(&StrategyDeckFormationNeedV1::Frontload),
            "{card:?} only moves the aggregate damage fact from 42 to 48"
        );
    }
}

#[test]
fn incomplete_exhaust_package_does_not_claim_long_fight_coverage() {
    let threats = one_threat(
        StrategyThreatSourceV1::ActBoss,
        "TheChamp",
        StrategyThreatTagV1::LongFightScaling,
    );
    let mut generator_only = RunState::new(1, 0, false, "Ironclad");
    generator_only.add_card_to_deck(CardId::TrueGrit);
    let incomplete = threat_coverage_from_run_state_v1(&generator_only, &threats);
    assert!(incomplete.has_gap(
        StrategyThreatSourceV1::ActBoss,
        StrategyThreatTagV1::LongFightScaling
    ));

    generator_only.add_card_to_deck(CardId::DarkEmbrace);
    let complete = threat_coverage_from_run_state_v1(&generator_only, &threats);
    assert!(!complete.has_gap(
        StrategyThreatSourceV1::ActBoss,
        StrategyThreatTagV1::LongFightScaling
    ));
    assert_eq!(
        complete
            .capability(StrategyCapabilityKindV1::LongFightScaling)
            .expect("long-fight capability")
            .coverage,
        StrategyCapabilityCoverageV1::Strong,
        "a repeatable converter with starter fuel and an exhaust payoff is a mature engine"
    );
}

#[test]
fn second_wind_engine_is_mature_before_a_duplicate_converter_copy() {
    let threats = one_threat(
        StrategyThreatSourceV1::ActBoss,
        "TheChamp",
        StrategyThreatTagV1::LongFightScaling,
    );
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.add_card_to_deck(CardId::PowerThrough);
    run.add_card_to_deck(CardId::SecondWind);
    run.add_card_to_deck(CardId::DarkEmbrace);

    let supported = threat_coverage_from_run_state_v1(&run, &threats);
    assert_eq!(
        supported
            .capability(StrategyCapabilityKindV1::LongFightScaling)
            .expect("long-fight capability")
            .coverage,
        StrategyCapabilityCoverageV1::Strong
    );

    run.add_card_to_deck(CardId::SecondWind);
    let duplicate = threat_coverage_from_run_state_v1(&run, &threats);
    let long_fight = duplicate
        .capability(StrategyCapabilityKindV1::LongFightScaling)
        .expect("long-fight capability");
    let input = |kind| {
        long_fight
            .inputs
            .iter()
            .find(|input| input.input == kind)
            .map(|input| input.value)
    };

    assert_eq!(long_fight.coverage, StrategyCapabilityCoverageV1::Strong);
    assert_eq!(
        input(StrategyCapabilityInputKindV1::OneShotExhaustGenerators),
        Some(0)
    );
    assert_eq!(
        input(StrategyCapabilityInputKindV1::RepeatableExhaustConverters),
        Some(2)
    );
    assert_eq!(
        input(StrategyCapabilityInputKindV1::GeneratedExhaustFuelPerCycle),
        Some(2)
    );
}

#[test]
fn second_wind_does_not_count_attacks_as_exhaust_fuel() {
    let threats = one_threat(
        StrategyThreatSourceV1::ActBoss,
        "TheChamp",
        StrategyThreatTagV1::LongFightScaling,
    );
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.master_deck = [
        CardId::Strike,
        CardId::Strike,
        CardId::Strike,
        CardId::SecondWind,
        CardId::DarkEmbrace,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
    .collect();

    let coverage = threat_coverage_from_run_state_v1(&run, &threats);
    let long_fight = coverage
        .capability(StrategyCapabilityKindV1::LongFightScaling)
        .expect("long-fight capability");
    let fuel = long_fight
        .inputs
        .iter()
        .find(|input| input.input == StrategyCapabilityInputKindV1::ExhaustFuelCards)
        .map(|input| input.value);

    assert_eq!(fuel, Some(0));
    assert_eq!(long_fight.coverage, StrategyCapabilityCoverageV1::Supported);
}

#[test]
fn duplicate_one_shot_exhaust_generators_do_not_mature_long_fight_engine() {
    let threats = one_threat(
        StrategyThreatSourceV1::ActBoss,
        "TheChamp",
        StrategyThreatTagV1::LongFightScaling,
    );
    let mut run = RunState::new(1, 0, false, "Ironclad");
    run.add_card_to_deck(CardId::PowerThrough);
    run.add_card_to_deck(CardId::FiendFire);
    run.add_card_to_deck(CardId::FiendFire);
    run.add_card_to_deck(CardId::DarkEmbrace);

    let coverage = threat_coverage_from_run_state_v1(&run, &threats);
    let long_fight = coverage
        .capability(StrategyCapabilityKindV1::LongFightScaling)
        .expect("long-fight capability");
    let input = |kind| {
        long_fight
            .inputs
            .iter()
            .find(|input| input.input == kind)
            .map(|input| input.value)
    };

    assert_eq!(long_fight.coverage, StrategyCapabilityCoverageV1::Supported);
    assert_eq!(
        input(StrategyCapabilityInputKindV1::OneShotExhaustGenerators),
        Some(2)
    );
    assert_eq!(
        input(StrategyCapabilityInputKindV1::RepeatableExhaustConverters),
        Some(0)
    );
}

#[test]
fn temporary_strength_does_not_claim_long_fight_coverage() {
    let threats = one_threat(
        StrategyThreatSourceV1::ActBoss,
        "TheChamp",
        StrategyThreatTagV1::LongFightScaling,
    );
    let mut with_flex = RunState::new(1, 0, false, "Ironclad");
    with_flex.add_card_to_deck(CardId::Flex);
    let coverage = threat_coverage_from_run_state_v1(&with_flex, &threats);
    assert!(coverage.has_gap(
        StrategyThreatSourceV1::ActBoss,
        StrategyThreatTagV1::LongFightScaling
    ));
}

#[test]
fn artifact_coverage_requires_multiple_independent_debuff_applications() {
    let threats = one_threat(
        StrategyThreatSourceV1::ActBoss,
        "Automaton",
        StrategyThreatTagV1::ArtifactBlocksDebuff,
    );
    let starter = RunState::new(1, 0, false, "Ironclad");
    let starter_coverage = threat_coverage_from_run_state_v1(&starter, &threats);
    assert!(starter_coverage.has_gap(
        StrategyThreatSourceV1::ActBoss,
        StrategyThreatTagV1::ArtifactBlocksDebuff
    ));

    let mut with_shockwave = starter;
    with_shockwave.add_card_to_deck(CardId::Shockwave);
    let patched_coverage = threat_coverage_from_run_state_v1(&with_shockwave, &threats);
    assert!(!patched_coverage.has_gap(
        StrategyThreatSourceV1::ActBoss,
        StrategyThreatTagV1::ArtifactBlocksDebuff
    ));
}
