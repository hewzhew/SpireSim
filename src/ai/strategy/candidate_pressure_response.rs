use serde::{Deserialize, Serialize};

use crate::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CombatEvent, InstalledRule, Mechanic, PayoffRequirement,
    PlayEffect, TriggeredEffect,
};
use crate::ai::strategy::package_transition::PackageKind;
use crate::ai::strategy::pressure_assessment::PressureAxis;
use crate::ai::strategy::reward_admission::{RewardAdmission, RewardAdmissionReason};
use crate::content::cards::CardId;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyCommitmentKind {
    ExhaustEngine,
    SelfDamageEngine,
    StrengthScaling,
    BlockEngine,
    UpgradeAccess,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CandidatePressureResponse {
    pub axes: Vec<PressureAxis>,
    pub opens_commitments: Vec<StrategyCommitmentKind>,
    pub supports_commitments: Vec<StrategyCommitmentKind>,
    pub repeatable_self_damage_supply: bool,
}

pub fn assess_candidate_pressure_response(
    card: Option<(CardId, u8)>,
    admission: &RewardAdmission,
) -> CandidatePressureResponse {
    let mut response = CandidatePressureResponse::default();
    for reason in &admission.reasons {
        match reason {
            RewardAdmissionReason::FrontloadDamage
            | RewardAdmissionReason::Provides(Mechanic::Vulnerable) => {
                push_unique(&mut response.axes, PressureAxis::ResolutionTempo)
            }
            RewardAdmissionReason::AreaDamage => {
                push_unique(&mut response.axes, PressureAxis::MultiTargetControl)
            }
            RewardAdmissionReason::Provides(
                Mechanic::Block
                | Mechanic::Weak
                | Mechanic::EnemyStrengthDown
                | Mechanic::TemporaryEnemyStrengthDown,
            ) => push_unique(&mut response.axes, PressureAxis::DelayCapacity),
            RewardAdmissionReason::Provides(Mechanic::CardDraw | Mechanic::Energy)
            | RewardAdmissionReason::CombatUpgrade => {
                push_unique(&mut response.axes, PressureAxis::Deployability)
            }
            RewardAdmissionReason::Provides(Mechanic::Strength | Mechanic::StrengthMultiplier)
            | RewardAdmissionReason::DamageScalesWith(_)
            | RewardAdmissionReason::Installs(_) => {
                push_unique(&mut response.axes, PressureAxis::GrowthHorizon)
            }
            RewardAdmissionReason::Supports(package) => {
                if let Some(kind) = commitment_for_package(*package) {
                    push_unique(&mut response.supports_commitments, kind);
                }
            }
            _ => {}
        }
    }

    if let Some((card, upgrades)) = card {
        let definition = card_definition_with_upgrades(card, upgrades);
        if definition
            .installed_rules
            .contains(&InstalledRule::SkillCardsCostZeroAndExhaust)
        {
            push_unique(
                &mut response.opens_commitments,
                StrategyCommitmentKind::ExhaustEngine,
            );
            push_unique(&mut response.axes, PressureAxis::GrowthHorizon);
        }
        if definition
            .payoff_requirements
            .contains(&PayoffRequirement::WantsEventStream(
                CombatEvent::CardSelfDamage,
            ))
        {
            push_unique(
                &mut response.opens_commitments,
                StrategyCommitmentKind::SelfDamageEngine,
            );
            push_unique(&mut response.axes, PressureAxis::GrowthHorizon);
        }
        let emits_direct_self_damage = definition
            .play_effects
            .contains(&PlayEffect::EmitEvent(CombatEvent::CardSelfDamage));
        let emits_triggered_self_damage = definition
            .event_handlers
            .iter()
            .any(|handler| handler.effect == TriggeredEffect::LoseHpFromCard);
        if emits_direct_self_damage || emits_triggered_self_damage {
            push_unique(
                &mut response.supports_commitments,
                StrategyCommitmentKind::SelfDamageEngine,
            );
        }
        response.repeatable_self_damage_supply = emits_triggered_self_damage
            || (emits_direct_self_damage
                && !definition.play_effects.contains(&PlayEffect::ExhaustsSelf));
    }

    response.axes.sort();
    response.opens_commitments.sort();
    response.supports_commitments.sort();
    response
}

fn commitment_for_package(package: PackageKind) -> Option<StrategyCommitmentKind> {
    match package {
        PackageKind::Strength => Some(StrategyCommitmentKind::StrengthScaling),
        PackageKind::Exhaust => Some(StrategyCommitmentKind::ExhaustEngine),
        PackageKind::SelfDamage => Some(StrategyCommitmentKind::SelfDamageEngine),
        PackageKind::Block => Some(StrategyCommitmentKind::BlockEngine),
    }
}

fn push_unique<T: Copy + Eq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::strategy::reward_admission::assess_reward_admission;

    #[test]
    fn shockwave_exposes_tempo_and_delay_responses() {
        let admission = assess_reward_admission(&[], CardId::Shockwave);
        let response = assess_candidate_pressure_response(Some((CardId::Shockwave, 0)), &admission);

        assert!(response.axes.contains(&PressureAxis::ResolutionTempo));
        assert!(response.axes.contains(&PressureAxis::DelayCapacity));
    }

    #[test]
    fn corruption_opens_an_exhaust_commitment_without_card_id_rules() {
        let admission = assess_reward_admission(&[], CardId::Corruption);
        let response =
            assess_candidate_pressure_response(Some((CardId::Corruption, 0)), &admission);

        assert!(response.axes.contains(&PressureAxis::GrowthHorizon));
        assert!(response
            .opens_commitments
            .contains(&StrategyCommitmentKind::ExhaustEngine));
    }

    #[test]
    fn rupture_opens_self_damage_commitment_from_semantic_requirement() {
        let admission = assess_reward_admission(&[CardId::Offering], CardId::Rupture);
        let response = assess_candidate_pressure_response(Some((CardId::Rupture, 0)), &admission);

        assert!(response
            .opens_commitments
            .contains(&StrategyCommitmentKind::SelfDamageEngine));
    }

    #[test]
    fn offering_supports_self_damage_but_does_not_claim_repeatability() {
        let admission = assess_reward_admission(&[CardId::Rupture], CardId::Offering);
        let response = assess_candidate_pressure_response(Some((CardId::Offering, 0)), &admission);

        assert!(response
            .supports_commitments
            .contains(&StrategyCommitmentKind::SelfDamageEngine));
        assert!(!response.repeatable_self_damage_supply);
    }

    #[test]
    fn recurring_power_supports_repeatable_self_damage_from_handlers() {
        let admission = assess_reward_admission(&[CardId::Rupture], CardId::Brutality);
        let response = assess_candidate_pressure_response(Some((CardId::Brutality, 0)), &admission);

        assert!(response
            .supports_commitments
            .contains(&StrategyCommitmentKind::SelfDamageEngine));
        assert!(response.repeatable_self_damage_supply);
    }
}
