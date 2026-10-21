use crate::ai::strategy::boss_damage_plan::{
    assess_boss_damage_plan_v1, BossDamagePlanEngineReliabilityV1, BossDamagePlanReadinessV1,
};
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::runtime::combat::{CombatCard, CombatState};

use super::awakened_one_signals::{card_labels, AwakenedOneDeckSignals};
use super::types::{
    BossMatchupClaimConfidence, BossMatchupClaimScope, BossMatchupClaimStatus,
    BossMatchupEvidenceClaim, BossMatchupEvidenceFrame, BossMatchupInputSummary,
    BossMatchupPolicyConsumability, BossMatchupShadowPressureKindV1, BossMatchupShadowPressureV1,
};

pub fn awakened_one_evidence_frame(combat: &CombatState) -> Option<BossMatchupEvidenceFrame> {
    if !is_awakened_one_case(combat) {
        return None;
    }
    let signals = AwakenedOneDeckSignals::from_combat(combat);
    Some(awakened_one_evidence_frame_from_signals(signals))
}

pub fn awakened_one_evidence_frame_from_deck(
    deck: Vec<CombatCard>,
    energy: u8,
    has_runic_dome: bool,
) -> BossMatchupEvidenceFrame {
    awakened_one_evidence_frame_from_signals(AwakenedOneDeckSignals::from_deck(
        deck,
        energy,
        has_runic_dome,
    ))
}

pub fn awakened_one_cultist_cleanup_shadow_pressure_v1(
    deck: &[CombatCard],
) -> Option<BossMatchupShadowPressureV1> {
    let signals = AwakenedOneDeckSignals::from_deck(deck.to_vec(), 3, false);
    signals.aoe.is_empty().then(|| BossMatchupShadowPressureV1 {
        kind: BossMatchupShadowPressureKindV1::AwakenedCultistCleanup,
        evidence: vec![
            "Awakened One starts with two scaling Cultists".to_string(),
            "current deck has no static AOE evidence for the Cultist deadline".to_string(),
        ],
    })
}

pub fn is_awakened_one_case(combat: &CombatState) -> bool {
    combat
        .entities
        .monsters
        .iter()
        .any(|monster| monster.enemy_id == EnemyId::AwakenedOne)
}

fn awakened_one_evidence_frame_from_signals(
    signals: AwakenedOneDeckSignals,
) -> BossMatchupEvidenceFrame {
    let claims = awakened_one_claims(&signals);
    BossMatchupEvidenceFrame {
        schema: "boss_matchup_evidence_frame_v0",
        contract: "domain_boss_matchup_claims_no_report_or_policy_side_effects",
        boss: "AwakenedOne",
        input: BossMatchupInputSummary {
            deck_size: signals.deck.len(),
            energy: signals.energy,
            has_runic_dome: signals.has_runic_dome,
            deck: card_labels(&signals.deck),
        },
        claims,
    }
}

fn awakened_one_claims(signals: &AwakenedOneDeckSignals) -> Vec<BossMatchupEvidenceClaim> {
    vec![
        damage_scaling_claim(signals),
        defensive_scaling_claim(signals),
        cultist_deadline_claim(signals),
        phase2_dark_echo_claim(signals),
        power_penalty_claim(signals),
        deck_clean_not_sufficient_claim(signals),
        generic_block_package_claim(signals),
        deck_access_claim(signals),
        mitigation_pressure_claim(signals),
        defensive_engine_pressure_claim(signals),
        acceleration_pressure_claim(signals),
        scaling_multiplier_pressure_claim(signals),
    ]
}

fn claim(
    id: &'static str,
    status: BossMatchupClaimStatus,
    support: Vec<String>,
    counterevidence: Vec<String>,
    unknown: Vec<String>,
    policy_consumability: BossMatchupPolicyConsumability,
) -> BossMatchupEvidenceClaim {
    BossMatchupEvidenceClaim {
        id,
        status,
        support,
        counterevidence,
        unknown,
        scope: BossMatchupClaimScope::StaticOnly,
        confidence: BossMatchupClaimConfidence::Provisional,
        policy_consumability,
    }
}

fn damage_scaling_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    let plan = assess_boss_damage_plan_v1(&signals.deck);
    let single_slow = plan.readiness == BossDamagePlanReadinessV1::Engine
        && signals.damage_scaling.len() == 1
        && signals.damage_scaling[0].id == CardId::DemonForm;
    match plan.readiness {
        BossDamagePlanReadinessV1::Engine => claim(
            "damage_scaling_present",
            if single_slow {
                BossMatchupClaimStatus::SingleSlowSource
            } else if plan.engine_reliability == BossDamagePlanEngineReliabilityV1::Fragile {
                BossMatchupClaimStatus::WeakSupported
            } else {
                BossMatchupClaimStatus::Supported
            },
            plan.active_plan_labels(),
            if single_slow {
                vec!["Demon Form is slow and must be survived into value".to_string()]
            } else {
                vec![]
            },
            vec![],
            BossMatchupPolicyConsumability::HumanOnly,
        ),
        BossDamagePlanReadinessV1::Support => claim(
            "damage_scaling_present",
            BossMatchupClaimStatus::WeakSupported,
            plan.active_plan_labels(),
            vec!["support pieces do not establish a repeatable boss damage engine".to_string()],
            vec![],
            BossMatchupPolicyConsumability::HumanOnly,
        ),
        BossDamagePlanReadinessV1::Fragment | BossDamagePlanReadinessV1::Missing => claim(
            "damage_scaling_present",
            BossMatchupClaimStatus::Unsupported,
            vec![],
            vec!["no complete repeatable boss damage engine in deck".to_string()],
            vec![],
            BossMatchupPolicyConsumability::HumanOnly,
        ),
    }
}

fn defensive_scaling_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if signals.defensive_scaling_or_mitigation.is_empty() {
        claim(
            "defensive_scaling_or_mitigation_present",
            BossMatchupClaimStatus::Unsupported,
            vec![],
            vec![
                "no Disarm / Shockwave / Impervious / Power Through / Feel No Pain / Second Wind / Barricade evidence".to_string(),
                format!(
                    "generic block cards do not establish boss-grade defensive scaling: {}",
                    card_labels(&signals.generic_block).join(", ")
                ),
            ],
            vec![],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    } else {
        claim(
            "defensive_scaling_or_mitigation_present",
            BossMatchupClaimStatus::Supported,
            card_labels(&signals.defensive_scaling_or_mitigation),
            vec![],
            vec![],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    }
}

fn cultist_deadline_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if signals.aoe.is_empty() {
        claim(
            "cultist_deadline_plan",
            BossMatchupClaimStatus::Unsupported,
            vec![],
            vec!["no AOE evidence for early Cultist cleanup".to_string()],
            vec![
                "single-target sequencing may still kill Cultists but is not evidenced here"
                    .to_string(),
            ],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    } else {
        claim(
            "cultist_deadline_plan",
            BossMatchupClaimStatus::WeakSupported,
            card_labels(&signals.aoe),
            vec![
                "AOE presence does not prove Cultists are killed before scaling pressure"
                    .to_string(),
            ],
            vec!["actual Cultist death turns require line replay evidence".to_string()],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    }
}

fn phase2_dark_echo_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if !signals.defensive_scaling_or_mitigation.is_empty() {
        claim(
            "phase2_dark_echo_plan",
            BossMatchupClaimStatus::Supported,
            card_labels(
                &signals
                    .defensive_scaling_or_mitigation
                    .iter()
                    .chain(signals.big_block.iter())
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            vec![],
            vec![],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    } else if !signals.big_block.is_empty() {
        claim(
            "phase2_dark_echo_plan",
            BossMatchupClaimStatus::WeakSupported,
            card_labels(&signals.big_block),
            vec![
                "single-use big block does not prove it is drawn and playable on the phase-2 Dark Echo turn"
                    .to_string(),
            ],
            vec![
                "search/replay evidence is needed to prove the transition turn is covered"
                    .to_string(),
            ],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    } else {
        claim(
            "phase2_dark_echo_plan",
            BossMatchupClaimStatus::Unsupported,
            vec![],
            vec![
                "no obvious big block, mitigation, or block engine for the phase-2 Dark Echo turn"
                    .to_string(),
            ],
            vec!["search/replay evidence could still show a specific transition line".to_string()],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    }
}

fn power_penalty_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if signals.powers.is_empty() {
        claim(
            "awakened_one_power_penalty_exposure",
            BossMatchupClaimStatus::NotPresent,
            vec![],
            vec!["no Power cards in deck".to_string()],
            vec![],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    } else {
        claim(
            "awakened_one_power_penalty_exposure",
            BossMatchupClaimStatus::Supported,
            card_labels(&signals.powers),
            power_counterevidence(signals),
            vec!["actual Power timing requires replay evidence".to_string()],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    }
}

fn deck_clean_not_sufficient_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    let mut support = Vec::new();
    if signals.deck.len() <= 18 {
        support.push(format!("small deck_size={}", signals.deck.len()));
    }
    if signals.curses.is_empty() {
        support.push("no curse burden".to_string());
    }
    claim(
        "clean_deck_does_not_imply_boss_plan_sufficient",
        if support.is_empty() {
            BossMatchupClaimStatus::Unknown
        } else {
            BossMatchupClaimStatus::Supported
        },
        support,
        vec![
            "deck cleanliness is separate from defensive scaling, mitigation, and phase-transition planning".to_string(),
        ],
        vec![],
        BossMatchupPolicyConsumability::HumanOnly,
    )
}

fn generic_block_package_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if signals.generic_block.is_empty() {
        claim(
            "generic_block_package",
            BossMatchupClaimStatus::NotPresent,
            vec![],
            vec!["no generic block package evidence".to_string()],
            vec![],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    } else {
        claim(
            "generic_block_package",
            BossMatchupClaimStatus::WeakSupported,
            card_labels(&signals.generic_block),
            vec!["generic block does not prove boss-grade repeatable survival".to_string()],
            vec!["draw timing and phase transition coverage require replay evidence".to_string()],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    }
}

fn deck_access_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if signals.access.is_empty() {
        claim(
            "deck_access_or_acceleration",
            BossMatchupClaimStatus::Unsupported,
            vec![],
            vec!["no access / acceleration evidence in deck".to_string()],
            vec![],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    } else if signals.premium_access.is_empty() {
        claim(
            "deck_access_or_acceleration",
            BossMatchupClaimStatus::WeakSupported,
            card_labels(&signals.access),
            vec!["partial access may still be too slow for Demon Form to matter".to_string()],
            vec!["actual key-card setup turn requires replay evidence".to_string()],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    } else {
        claim(
            "deck_access_or_acceleration",
            BossMatchupClaimStatus::Supported,
            card_labels(&signals.access),
            vec![],
            vec!["actual key-card setup turn requires replay evidence".to_string()],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    }
}

fn mitigation_pressure_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if signals.mitigation_or_strength_down.is_empty() {
        claim(
            "mitigation_or_strength_down",
            BossMatchupClaimStatus::Unsupported,
            vec![],
            vec!["no Disarm / Shockwave / Intimidate / Uppercut+ evidence".to_string()],
            vec![],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    } else {
        claim(
            "mitigation_or_strength_down",
            BossMatchupClaimStatus::Supported,
            card_labels(&signals.mitigation_or_strength_down),
            vec![],
            vec!["actual mitigation timing requires replay evidence".to_string()],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    }
}

fn defensive_engine_pressure_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if signals.defensive_engine_or_repeatable_block.is_empty() {
        claim(
            "defensive_engine_or_repeatable_block",
            BossMatchupClaimStatus::Unsupported,
            vec![],
            vec![
                "no Feel No Pain / Second Wind / Corruption / Dark Embrace / Barricade evidence"
                    .to_string(),
            ],
            vec![],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    } else {
        claim(
            "defensive_engine_or_repeatable_block",
            BossMatchupClaimStatus::Supported,
            card_labels(&signals.defensive_engine_or_repeatable_block),
            vec![],
            vec!["engine deployability requires combat replay evidence".to_string()],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    }
}

fn acceleration_pressure_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if !signals.premium_access.is_empty() {
        claim(
            "acceleration_for_slow_scaling",
            BossMatchupClaimStatus::Supported,
            card_labels(&signals.premium_access),
            vec![],
            vec!["actual Demon Form setup turn requires replay evidence".to_string()],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    } else if !signals.access.is_empty() {
        claim(
            "acceleration_for_slow_scaling",
            BossMatchupClaimStatus::Uncertain,
            card_labels(&signals.access),
            vec!["partial access may still be too slow for Demon Form to matter".to_string()],
            vec!["actual Demon Form setup turn requires replay evidence".to_string()],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    } else {
        claim(
            "acceleration_for_slow_scaling",
            BossMatchupClaimStatus::Unsupported,
            vec![],
            vec![
                "no Offering / Battle Trance / Burning Pact / Pommel Strike / Shrug It Off evidence"
                    .to_string(),
            ],
            vec![],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    }
}

fn scaling_multiplier_pressure_claim(signals: &AwakenedOneDeckSignals) -> BossMatchupEvidenceClaim {
    if signals.scaling_multiplier.is_empty() {
        claim(
            "damage_scaling_multiplier",
            BossMatchupClaimStatus::Unsupported,
            vec![],
            vec!["no Limit Break evidence".to_string()],
            vec![],
            BossMatchupPolicyConsumability::ShadowPressure,
        )
    } else {
        claim(
            "damage_scaling_multiplier",
            BossMatchupClaimStatus::Supported,
            card_labels(&signals.scaling_multiplier),
            vec![],
            vec!["actual scaling payoff requires replay evidence".to_string()],
            BossMatchupPolicyConsumability::HumanOnly,
        )
    }
}

fn power_counterevidence(signals: &AwakenedOneDeckSignals) -> Vec<String> {
    let mut items = Vec::new();
    if signals.powers.iter().any(|card| card.id == CardId::Rupture)
        && signals.self_damage.is_empty()
    {
        items.push(
            "Rupture has no stable self-damage engine and may be Burning Pact fuel".to_string(),
        );
    }
    if signals
        .powers
        .iter()
        .any(|card| card.id == CardId::DemonForm)
    {
        items.push("Demon Form is valuable scaling but triggers Curiosity and is slow".to_string());
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_aoe_exposes_cultist_cleanup_shadow_pressure() {
        let deck = vec![
            CombatCard::for_scenario(CardId::Bash, 1),
            CombatCard::for_scenario(CardId::ShrugItOff, 2),
        ];

        let pressure = awakened_one_cultist_cleanup_shadow_pressure_v1(&deck)
            .expect("a deck without AOE should expose the Cultist cleanup pressure");

        assert_eq!(
            pressure.kind,
            BossMatchupShadowPressureKindV1::AwakenedCultistCleanup
        );
    }

    #[test]
    fn existing_aoe_retires_cultist_cleanup_shadow_pressure() {
        let deck = vec![
            CombatCard::for_scenario(CardId::Bash, 1),
            CombatCard::for_scenario(CardId::Whirlwind, 2),
        ];

        assert!(awakened_one_cultist_cleanup_shadow_pressure_v1(&deck).is_none());
    }

    #[test]
    fn spot_weakness_is_a_conditional_source_not_a_scaling_multiplier() {
        let frame = awakened_one_evidence_frame_from_deck(
            vec![CombatCard::for_scenario(CardId::SpotWeakness, 1)],
            4,
            false,
        );
        let damage = frame
            .claims
            .iter()
            .find(|claim| claim.id == "damage_scaling_present")
            .expect("damage scaling claim");
        let multiplier = frame
            .claims
            .iter()
            .find(|claim| claim.id == "damage_scaling_multiplier")
            .expect("scaling multiplier claim");

        assert_eq!(damage.status, BossMatchupClaimStatus::WeakSupported);
        assert_eq!(damage.support, vec!["repeatable_strength_growth"]);
        assert_eq!(multiplier.status, BossMatchupClaimStatus::Unsupported);
        assert!(multiplier.support.is_empty());
    }
}
