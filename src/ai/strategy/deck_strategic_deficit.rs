use serde::Serialize;

use crate::ai::analysis::card_semantics::{card_definition_with_upgrades, PlayEffect};
use crate::ai::strategy::boss_damage_plan::{
    assess_boss_damage_plan_v1, BossDamagePlanEngineReliabilityV1, BossDamagePlanFactsV1,
    BossDamagePlanReadinessV1,
};
use crate::ai::strategy::deck_construction_pressure::{
    assess_deck_construction_pressure, DeckConstructionContext, PressureLevel,
};
use crate::ai::strategy::deck_role_inventory::DeckRoleInventory;
use crate::ai::strategy::exhaust_corruption_assessment::{
    assess_exhaust_corruption, ExhaustCorruptionAssessment, ExhaustCorruptionRisk,
    ExhaustCorruptionState,
};
use crate::ai::strategy::run_strategic_facts::RunStrategicFacts;
use crate::content::cards::{get_card_definition, CardId, CardType};
use crate::runtime::combat::CombatCard;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicDeficitLevel {
    Missing,
    Thin,
    Adequate,
    Surplus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicBurdenLevel {
    Clean,
    Watch,
    Heavy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicPackageEvidence {
    ExhaustEngine,
    StrengthScaling,
    BlockEngine,
    #[serde(rename = "aoe_package")]
    AoEPackage,
    DrawEngine,
    EnergyEngine,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategicRisk {
    TooManyLowImpactAttacks,
    ApparentFrontloadLowQuality,
    #[serde(rename = "shallow_aoe_for_multi_enemy")]
    ShallowAoEForMultiEnemy,
    HighStarterBurden,
    TooManyConditionalPayoffs,
    NoEnablerForExploiters,
    DeckTooThickForAccess,
    OpeningHandPollution,
    SevereCurseBurden,
    ReliesOnPowers,
    ReliesOnLowImpactCardSpam,
    CorruptionWithoutExhaustPayoff,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeckStrategicDeficit {
    pub frontload_damage: StrategicDeficitLevel,
    pub aoe_or_minion_control: StrategicDeficitLevel,
    pub block_or_mitigation: StrategicDeficitLevel,
    pub boss_scaling_plan: StrategicDeficitLevel,
    pub deck_access: StrategicDeficitLevel,
    pub energy_or_playability: StrategicDeficitLevel,
    pub deck_burden: StrategicBurdenLevel,
    pub exhaust_corruption: ExhaustCorruptionAssessment,
    pub package_evidence: Vec<StrategicPackageEvidence>,
    pub risks: Vec<StrategicRisk>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeckStrategicDeficitSummary {
    pub frontload_damage: StrategicDeficitLevel,
    pub aoe_or_minion_control: StrategicDeficitLevel,
    pub block_or_mitigation: StrategicDeficitLevel,
    pub boss_scaling_plan: StrategicDeficitLevel,
    pub deck_access: StrategicDeficitLevel,
    pub energy_or_playability: StrategicDeficitLevel,
    pub deck_burden: StrategicBurdenLevel,
    pub too_many_low_impact_attacks: bool,
    pub opening_hand_pollution: bool,
    pub severe_curse_burden: bool,
}

impl DeckStrategicDeficit {
    pub fn summary(&self) -> DeckStrategicDeficitSummary {
        DeckStrategicDeficitSummary {
            frontload_damage: self.frontload_damage,
            aoe_or_minion_control: self.aoe_or_minion_control,
            block_or_mitigation: self.block_or_mitigation,
            boss_scaling_plan: self.boss_scaling_plan,
            deck_access: self.deck_access,
            energy_or_playability: self.energy_or_playability,
            deck_burden: self.deck_burden,
            too_many_low_impact_attacks: self
                .risks
                .contains(&StrategicRisk::TooManyLowImpactAttacks),
            opening_hand_pollution: self.risks.contains(&StrategicRisk::OpeningHandPollution),
            severe_curse_burden: self.risks.contains(&StrategicRisk::SevereCurseBurden),
        }
    }
}

pub fn assess_deck_strategic_deficit(
    deck: &[CombatCard],
    facts: RunStrategicFacts,
) -> DeckStrategicDeficit {
    let inventory = DeckRoleInventory::from_deck(deck);
    let counts = StrategicCounts::from_deck(deck, facts);
    let construction =
        assess_deck_construction_pressure(deck, DeckConstructionContext { act: counts.act });
    let exhaust_corruption = assess_exhaust_corruption(deck);
    let frontload_units = calibrated_frontload_units(&inventory, &counts);
    let block_or_mitigation_units = block_or_mitigation_units(&inventory, &counts);
    let package_evidence = package_evidence(&inventory, &counts, &exhaust_corruption);
    let boss_damage_plan = assess_boss_damage_plan_v1(deck);
    let boss_scaling_plan = boss_scaling_level(&boss_damage_plan);
    let deck_access = access_level(construction.card_flow.level);
    let energy_or_playability = energy_level(&inventory, &counts);
    let risks = risks(&inventory, &counts, deck_access, &exhaust_corruption);

    DeckStrategicDeficit {
        frontload_damage: frontload_level(frontload_units, counts.act),
        aoe_or_minion_control: aoe_or_minion_level(&inventory, &counts),
        block_or_mitigation: block_or_mitigation_level(block_or_mitigation_units, counts.act),
        boss_scaling_plan,
        deck_access,
        energy_or_playability,
        deck_burden: burden_level(&counts, deck_access),
        exhaust_corruption,
        package_evidence,
        risks,
    }
}

pub fn assess_deck_strategic_deficit_summary(
    deck: &[CombatCard],
    facts: RunStrategicFacts,
) -> DeckStrategicDeficitSummary {
    assess_deck_strategic_deficit(deck, facts).summary()
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct StrategicCounts {
    deck_size: usize,
    act: u8,
    curse_count: usize,
    starter_basic_count: usize,
    power_count: u8,
    strike_count: u8,
    defend_count: u8,
    iron_wave_count: u8,
    writhe_count: u8,
    normality_count: u8,
    severe_curse_count: u8,
    low_impact_attacks: u8,
    conditional_payoffs: u8,
    status_clutter_sources: u8,
    has_energy_relic: bool,
    has_corruption: bool,
    has_dark_embrace: bool,
}

impl StrategicCounts {
    fn from_deck(deck: &[CombatCard], facts: RunStrategicFacts) -> Self {
        let mut counts = Self {
            deck_size: deck.len(),
            act: facts.entering_act,
            curse_count: facts.curse_count,
            starter_basic_count: facts.starter_basic_count,
            has_energy_relic: facts.has_energy_relic,
            ..Self::default()
        };
        for card in deck {
            counts.add_card(card);
        }
        counts
    }

    fn add_card(&mut self, card: &CombatCard) {
        let definition = get_card_definition(card.id);
        if definition.card_type == CardType::Power {
            self.power_count += 1;
        }
        if card.id == CardId::Strike {
            self.strike_count += 1;
        }
        if card.id == CardId::Defend {
            self.defend_count += 1;
        }
        if card.id == CardId::IronWave {
            self.iron_wave_count += 1;
        }
        if card.id == CardId::Writhe {
            self.writhe_count += 1;
        }
        if card.id == CardId::Normality {
            self.normality_count += 1;
        }
        if is_severe_curse(card.id) {
            self.severe_curse_count += 1;
        }
        if is_low_impact_attack(card.id) {
            self.low_impact_attacks += 1;
        }
        if is_conditional_payoff(card.id) {
            self.conditional_payoffs += 1;
        }
        if card.id == CardId::Corruption {
            self.has_corruption = true;
        }
        if card.id == CardId::DarkEmbrace {
            self.has_dark_embrace = true;
        }
        let semantics = card_definition_with_upgrades(card.id, card.upgrades);
        for effect in semantics.play_effects {
            if effect == PlayEffect::AddCombatDeckClutter {
                self.status_clutter_sources += 1;
            }
        }
    }
}

fn package_evidence(
    inventory: &DeckRoleInventory,
    counts: &StrategicCounts,
    exhaust_corruption: &ExhaustCorruptionAssessment,
) -> Vec<StrategicPackageEvidence> {
    let mut evidence = Vec::new();
    if matches!(
        exhaust_corruption.state,
        ExhaustCorruptionState::SupportedButSlow | ExhaustCorruptionState::EngineOnline
    ) || (counts.has_dark_embrace && inventory.exhaust_stream_units >= 1)
    {
        evidence.push(StrategicPackageEvidence::ExhaustEngine);
    }
    if has_persistent_strength_source(inventory) {
        evidence.push(StrategicPackageEvidence::StrengthScaling);
    }
    if inventory.block_units >= 3 && inventory.block_payoff_units > 0 {
        evidence.push(StrategicPackageEvidence::BlockEngine);
    }
    if inventory.aoe_units > 0 {
        evidence.push(StrategicPackageEvidence::AoEPackage);
    }
    if inventory.draw_units >= 2 || (counts.has_dark_embrace && inventory.exhaust_stream_units > 0)
    {
        evidence.push(StrategicPackageEvidence::DrawEngine);
    }
    if counts.has_energy_relic || inventory.energy_units > 0 || counts.has_corruption {
        evidence.push(StrategicPackageEvidence::EnergyEngine);
    }
    evidence
}

fn risks(
    inventory: &DeckRoleInventory,
    counts: &StrategicCounts,
    deck_access: StrategicDeficitLevel,
    exhaust_corruption: &ExhaustCorruptionAssessment,
) -> Vec<StrategicRisk> {
    let mut risks = Vec::new();
    if counts.low_impact_attacks >= 4 {
        risks.push(StrategicRisk::TooManyLowImpactAttacks);
    }
    if counts.low_impact_attacks >= 4 && counts.act >= 2 {
        risks.push(StrategicRisk::ApparentFrontloadLowQuality);
    }
    if counts.act >= 2 && inventory.aoe_units >= 2 && inventory.strong_aoe_units == 0 {
        risks.push(StrategicRisk::ShallowAoEForMultiEnemy);
    }
    if high_starter_burden(counts) {
        risks.push(StrategicRisk::HighStarterBurden);
    }
    if counts.conditional_payoffs >= 3 {
        risks.push(StrategicRisk::TooManyConditionalPayoffs);
    }
    if unsupported_payoffs(inventory, counts) {
        risks.push(StrategicRisk::NoEnablerForExploiters);
    }
    if counts.deck_size >= 26
        && matches!(
            deck_access,
            StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
        )
    {
        risks.push(StrategicRisk::DeckTooThickForAccess);
    }
    if counts.writhe_count > 0 {
        risks.push(StrategicRisk::OpeningHandPollution);
    }
    if counts.severe_curse_count > 0 {
        risks.push(StrategicRisk::SevereCurseBurden);
    }
    if counts.power_count >= 4 {
        risks.push(StrategicRisk::ReliesOnPowers);
    }
    if exhaust_corruption
        .risks
        .contains(&ExhaustCorruptionRisk::CorruptionWithoutExhaustPayoff)
    {
        risks.push(StrategicRisk::CorruptionWithoutExhaustPayoff);
    }
    if inventory.frontload_units >= 5
        && counts.low_impact_attacks >= 4
        && matches!(
            deck_access,
            StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
        )
    {
        risks.push(StrategicRisk::ReliesOnLowImpactCardSpam);
    }
    risks
}

fn calibrated_frontload_units(inventory: &DeckRoleInventory, counts: &StrategicCounts) -> u8 {
    inventory
        .frontload_units
        .saturating_add(starter_strike_frontload_credit(counts))
}

fn starter_strike_frontload_credit(counts: &StrategicCounts) -> u8 {
    if counts.act <= 1 {
        counts.strike_count.min(2)
    } else {
        counts.strike_count.min(1)
    }
}

fn aoe_or_minion_level(
    inventory: &DeckRoleInventory,
    counts: &StrategicCounts,
) -> StrategicDeficitLevel {
    if counts.act >= 2 && inventory.aoe_units >= 2 && inventory.strong_aoe_units == 0 {
        StrategicDeficitLevel::Thin
    } else {
        unit_level(inventory.aoe_units, 1, 4)
    }
}

fn unsupported_payoffs(inventory: &DeckRoleInventory, counts: &StrategicCounts) -> bool {
    (inventory.block_payoff_units > 0 && inventory.block_units <= 1)
        || (inventory.strength_payoff_units > 0 && !has_persistent_strength_source(inventory))
        || (counts.conditional_payoffs > 1
            && !has_persistent_strength_source(inventory)
            && inventory.block_units <= 1
            && counts.status_clutter_sources == 0)
}

fn has_persistent_strength_source(inventory: &DeckRoleInventory) -> bool {
    inventory.strength_source_units > 0 || inventory.conditional_strength_source_units > 0
}

fn block_or_mitigation_units(inventory: &DeckRoleInventory, counts: &StrategicCounts) -> u8 {
    let raw_nonstarter = inventory
        .block_units
        .saturating_add(inventory.cycle_block_units)
        .saturating_add(inventory.mitigation_units);
    let strong = raw_nonstarter.saturating_sub(counts.iron_wave_count);
    let low_quality_cap = counts
        .defend_count
        .saturating_add(counts.iron_wave_count)
        .min(2);
    strong.saturating_add(low_quality_cap)
}

fn block_or_mitigation_level(units: u8, act: u8) -> StrategicDeficitLevel {
    let surplus_at = if act >= 3 { 16 } else { 10 };
    unit_level(units, 2, surplus_at)
}

fn frontload_level(units: u8, act: u8) -> StrategicDeficitLevel {
    if act <= 2 {
        unit_level(units, 2, 8)
    } else {
        unit_level(units, 3, 8)
    }
}

fn boss_scaling_level(plan: &BossDamagePlanFactsV1) -> StrategicDeficitLevel {
    match (plan.readiness, plan.engine_reliability) {
        (BossDamagePlanReadinessV1::Engine, BossDamagePlanEngineReliabilityV1::Established) => {
            StrategicDeficitLevel::Adequate
        }
        (BossDamagePlanReadinessV1::Engine, _) => StrategicDeficitLevel::Thin,
        (BossDamagePlanReadinessV1::Support, _) => StrategicDeficitLevel::Thin,
        (BossDamagePlanReadinessV1::Fragment | BossDamagePlanReadinessV1::Missing, _) => {
            StrategicDeficitLevel::Missing
        }
    }
}

fn access_level(card_flow: PressureLevel) -> StrategicDeficitLevel {
    match card_flow {
        PressureLevel::Open => StrategicDeficitLevel::Missing,
        PressureLevel::Thin => StrategicDeficitLevel::Thin,
        PressureLevel::Present => StrategicDeficitLevel::Adequate,
    }
}

fn energy_level(inventory: &DeckRoleInventory, counts: &StrategicCounts) -> StrategicDeficitLevel {
    if counts.has_energy_relic || counts.has_corruption || inventory.energy_units >= 2 {
        StrategicDeficitLevel::Adequate
    } else if inventory.energy_units == 1 || counts.act <= 2 {
        StrategicDeficitLevel::Thin
    } else {
        StrategicDeficitLevel::Missing
    }
}

fn burden_level(counts: &StrategicCounts, access: StrategicDeficitLevel) -> StrategicBurdenLevel {
    if counts.curse_count >= 2
        || counts.normality_count > 0
        || counts.severe_curse_count >= 2
        || (counts.writhe_count > 0 && counts.deck_size >= 18)
        || high_starter_burden(counts)
        || counts.deck_size >= 34
        || (counts.deck_size >= 30
            && matches!(
                access,
                StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
            ))
    {
        StrategicBurdenLevel::Heavy
    } else if counts.curse_count > 0
        || counts.severe_curse_count > 0
        || counts.starter_basic_count >= 6
        || counts.deck_size >= 25
        || (counts.deck_size >= 22
            && matches!(
                access,
                StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
            ))
    {
        StrategicBurdenLevel::Watch
    } else {
        StrategicBurdenLevel::Clean
    }
}

fn high_starter_burden(counts: &StrategicCounts) -> bool {
    (counts.act >= 2 && counts.starter_basic_count >= 8 && counts.deck_size >= 18)
        || (counts.act >= 3 && counts.starter_basic_count >= 6)
}

fn unit_level(units: u8, thin_under: u8, surplus_at: u8) -> StrategicDeficitLevel {
    if units == 0 {
        StrategicDeficitLevel::Missing
    } else if units <= thin_under {
        StrategicDeficitLevel::Thin
    } else if units < surplus_at {
        StrategicDeficitLevel::Adequate
    } else {
        StrategicDeficitLevel::Surplus
    }
}

fn is_low_impact_attack(card: CardId) -> bool {
    matches!(
        card,
        CardId::Strike
            | CardId::SwiftStrike
            | CardId::TwinStrike
            | CardId::WildStrike
            | CardId::RecklessCharge
            | CardId::SwordBoomerang
            | CardId::Clash
    )
}

fn is_conditional_payoff(card: CardId) -> bool {
    matches!(
        card,
        CardId::BodySlam
            | CardId::Entrench
            | CardId::Barricade
            | CardId::Juggernaut
            | CardId::HeavyBlade
            | CardId::LimitBreak
            | CardId::Pummel
            | CardId::FireBreathing
            | CardId::Evolve
    )
}

fn is_severe_curse(card: CardId) -> bool {
    matches!(
        card,
        CardId::Writhe | CardId::Normality | CardId::Regret | CardId::Pain | CardId::Parasite
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    fn act3_facts() -> RunStrategicFacts {
        RunStrategicFacts {
            entering_act: 3,
            starter_basic_count: 0,
            curse_count: 0,
            has_energy_relic: false,
            has_runic_pyramid: false,
        }
    }

    fn act2_facts() -> RunStrategicFacts {
        RunStrategicFacts {
            entering_act: 2,
            starter_basic_count: 7,
            curse_count: 0,
            has_energy_relic: false,
            has_runic_pyramid: false,
        }
    }

    fn first_cycle_access_deck() -> Vec<CombatCard> {
        [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Cleave,
            CardId::Cleave,
            CardId::Inflame,
            CardId::LimitBreak,
            CardId::Pummel,
            CardId::Uppercut,
            CardId::Disarm,
            CardId::BurningPact,
            CardId::PommelStrike,
            CardId::ShrugItOff,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, id)| card(id, index as u32 + 1))
        .collect()
    }

    #[test]
    fn repeatable_strength_growth_is_an_adequate_boss_damage_engine() {
        let deficit = assess_deck_strategic_deficit(
            &[
                card(CardId::Strike, 1),
                card(CardId::Defend, 2),
                card(CardId::Bash, 3),
                card(CardId::DemonForm, 4),
            ],
            act3_facts(),
        );

        assert_eq!(deficit.boss_scaling_plan, StrategicDeficitLevel::Adequate);
        assert!(deficit
            .package_evidence
            .contains(&StrategicPackageEvidence::StrengthScaling));
    }

    #[test]
    fn strength_multiplier_alone_is_not_a_strength_source_package() {
        let deficit = assess_deck_strategic_deficit(
            &[card(CardId::LimitBreak, 1), card(CardId::Pummel, 2)],
            act3_facts(),
        );

        assert_eq!(deficit.boss_scaling_plan, StrategicDeficitLevel::Missing);
        assert!(!deficit
            .package_evidence
            .contains(&StrategicPackageEvidence::StrengthScaling));
    }

    #[test]
    fn temporary_strength_does_not_become_a_boss_scaling_package() {
        let deficit = assess_deck_strategic_deficit(
            &[card(CardId::Flex, 1), card(CardId::Pummel, 2)],
            act3_facts(),
        );

        assert_eq!(deficit.boss_scaling_plan, StrategicDeficitLevel::Missing);
        assert!(!deficit
            .package_evidence
            .contains(&StrategicPackageEvidence::StrengthScaling));
    }

    #[test]
    fn exhaust_draw_without_growth_or_block_payoff_is_only_a_thin_plan() {
        let deficit = assess_deck_strategic_deficit(
            &[
                card(CardId::DarkEmbrace, 1),
                card(CardId::TrueGrit, 2),
                card(CardId::FiendFire, 3),
                card(CardId::Whirlwind, 4),
            ],
            act3_facts(),
        );

        assert_eq!(deficit.boss_scaling_plan, StrategicDeficitLevel::Thin);
        assert!(deficit
            .package_evidence
            .contains(&StrategicPackageEvidence::ExhaustEngine));
        assert!(!deficit
            .package_evidence
            .contains(&StrategicPackageEvidence::StrengthScaling));
        assert!(!deficit
            .package_evidence
            .contains(&StrategicPackageEvidence::BlockEngine));
    }

    #[test]
    fn one_real_draw_source_plus_cantrips_is_thin_first_cycle_access() {
        let deficit = assess_deck_strategic_deficit(&first_cycle_access_deck(), act2_facts());

        assert_eq!(deficit.deck_access, StrategicDeficitLevel::Thin);
    }

    #[test]
    fn second_real_draw_source_makes_first_cycle_access_adequate() {
        let mut deck = first_cycle_access_deck();
        deck.push(card(CardId::BattleTrance, 100));

        let deficit = assess_deck_strategic_deficit(&deck, act2_facts());

        assert_eq!(deficit.deck_access, StrategicDeficitLevel::Adequate);
    }

    #[test]
    fn repeated_weak_aoe_remains_thin_for_act2_multi_enemy_pressure() {
        let deck = [CardId::Cleave, CardId::Cleave, CardId::Cleave]
            .into_iter()
            .enumerate()
            .map(|(index, id)| card(id, index as u32 + 1))
            .collect::<Vec<_>>();

        let deficit = assess_deck_strategic_deficit(&deck, act2_facts());

        assert_eq!(deficit.aoe_or_minion_control, StrategicDeficitLevel::Thin);
        assert!(deficit
            .risks
            .contains(&StrategicRisk::ShallowAoEForMultiEnemy));
    }

    #[test]
    fn act2_starter_defends_alone_remain_thin() {
        let deck = [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, id)| card(id, index as u32 + 1))
        .collect::<Vec<_>>();

        let deficit = assess_deck_strategic_deficit(&deck, act2_facts());

        assert_eq!(deficit.block_or_mitigation, StrategicDeficitLevel::Thin);
    }

    #[test]
    fn act2_real_block_access_can_close_starter_defense_gap() {
        let deck = [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::ShrugItOff,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, id)| card(id, index as u32 + 1))
        .collect::<Vec<_>>();

        let deficit = assess_deck_strategic_deficit(&deck, act2_facts());

        assert_eq!(deficit.block_or_mitigation, StrategicDeficitLevel::Adequate);
    }
}
