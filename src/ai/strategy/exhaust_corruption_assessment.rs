use serde::Serialize;

use crate::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CombatEvent, Mechanic, PlayEffect, TriggeredEffect,
};
use crate::content::cards::{get_card_definition, CardId, CardType};
use crate::runtime::combat::CombatCard;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExhaustCorruptionState {
    Absent,
    Seeded,
    SourceOnly,
    PayoffOnly,
    SupportedButSlow,
    EngineOnline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExhaustCorruptionRisk {
    CorruptionWithoutExhaustPayoff,
    PayoffWithoutFuel,
    TooSlowToSetup,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExhaustCorruptionAssessment {
    pub state: ExhaustCorruptionState,
    pub has_corruption: bool,
    pub exhaust_source_count: u8,
    pub exhaust_payoff_count: u8,
    pub skill_count: u8,
    pub access_count: u8,
    pub stabilizer_count: u8,
    pub risks: Vec<ExhaustCorruptionRisk>,
}

pub fn assess_exhaust_corruption(deck: &[CombatCard]) -> ExhaustCorruptionAssessment {
    let mut counts = ExhaustCorruptionCounts::default();
    for card in deck {
        counts.add_card(card);
    }

    let mut risks = Vec::new();
    if counts.has_corruption && counts.exhaust_payoff_count == 0 {
        risks.push(ExhaustCorruptionRisk::CorruptionWithoutExhaustPayoff);
    }
    if counts.exhaust_payoff_count > 0 && !counts.has_corruption && counts.exhaust_source_count <= 1
    {
        risks.push(ExhaustCorruptionRisk::PayoffWithoutFuel);
    }
    if counts.has_corruption
        && counts.exhaust_payoff_count > 0
        && counts.access_count == 0
        && counts.skill_count < 6
    {
        risks.push(ExhaustCorruptionRisk::TooSlowToSetup);
    }

    ExhaustCorruptionAssessment {
        state: exhaust_corruption_state(&counts),
        has_corruption: counts.has_corruption,
        exhaust_source_count: counts.exhaust_source_count,
        exhaust_payoff_count: counts.exhaust_payoff_count,
        skill_count: counts.skill_count,
        access_count: counts.access_count,
        stabilizer_count: counts.stabilizer_count,
        risks,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ExhaustCorruptionCounts {
    has_corruption: bool,
    exhaust_source_count: u8,
    exhaust_payoff_count: u8,
    skill_count: u8,
    access_count: u8,
    stabilizer_count: u8,
}

impl ExhaustCorruptionCounts {
    fn add_card(&mut self, card: &CombatCard) {
        let definition = card_definition_with_upgrades(card.id, card.upgrades);
        if definition.card == CardId::Corruption {
            self.has_corruption = true;
            self.exhaust_source_count = self.exhaust_source_count.saturating_add(2);
        }
        if get_card_definition(card.id).card_type == CardType::Skill {
            self.skill_count = self.skill_count.saturating_add(1);
        }
        for effect in &definition.play_effects {
            match *effect {
                PlayEffect::EmitEvent(CombatEvent::CardExhausted)
                | PlayEffect::PlayTopCardAndExhaust => {
                    self.exhaust_source_count = self.exhaust_source_count.saturating_add(1)
                }
                PlayEffect::Provide(Mechanic::CardDraw | Mechanic::Energy) => {
                    self.access_count = self.access_count.saturating_add(1)
                }
                PlayEffect::Provide(
                    Mechanic::Block
                    | Mechanic::Weak
                    | Mechanic::EnemyStrengthDown
                    | Mechanic::TemporaryEnemyStrengthDown,
                ) => self.stabilizer_count = self.stabilizer_count.saturating_add(1),
                PlayEffect::FrontloadDamage
                | PlayEffect::AreaDamage
                | PlayEffect::DamageUses(_)
                | PlayEffect::DamageScalesWith(_)
                | PlayEffect::Provide(_)
                | PlayEffect::EmitEvent(_)
                | PlayEffect::ExhaustsSelf
                | PlayEffect::RunReward(_)
                | PlayEffect::RecoverCurrentHp
                | PlayEffect::CostReducedByHpLossThisCombat
                | PlayEffect::CombatUpgradeSingle
                | PlayEffect::CombatUpgradeAll
                | PlayEffect::AddCombatDeckClutter => {}
            }
        }
        for handler in &definition.event_handlers {
            if handler.on == CombatEvent::CardExhausted
                && matches!(
                    handler.effect,
                    TriggeredEffect::Provide(Mechanic::Block | Mechanic::CardDraw)
                )
            {
                self.exhaust_payoff_count = self.exhaust_payoff_count.saturating_add(1);
            }
        }
    }
}

fn exhaust_corruption_state(counts: &ExhaustCorruptionCounts) -> ExhaustCorruptionState {
    match (
        counts.has_corruption,
        counts.exhaust_source_count > 0,
        counts.exhaust_payoff_count > 0,
    ) {
        (false, false, false) => ExhaustCorruptionState::Absent,
        (true, _, false) => ExhaustCorruptionState::Seeded,
        (false, true, false) => ExhaustCorruptionState::SourceOnly,
        (false, _, true) => ExhaustCorruptionState::PayoffOnly,
        (true, _, true) if counts.skill_count >= 6 && counts.access_count > 0 => {
            ExhaustCorruptionState::EngineOnline
        }
        (true, _, true) => ExhaustCorruptionState::SupportedButSlow,
    }
}
