use serde::Serialize;

use crate::runtime::combat::CombatState;

use super::awakened_one::awakened_one_evidence_frame;
use super::types::{BossMatchupClaimStatus, BossMatchupEvidenceClaim, BossMatchupEvidenceFrame};

#[derive(Clone, Debug, Serialize)]
pub struct BossMatchupAcquisitionPressureV0 {
    pub schema: &'static str,
    pub contract: &'static str,
    pub boss: &'static str,
    pub start: BossMatchupAcquisitionStart,
    pub pressure_tags: Vec<&'static str>,
    pub already_present: Vec<BossMatchupPressureClaim>,
    pub missing_or_uncertain: Vec<BossMatchupPressureClaim>,
    pub conclusion: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct BossMatchupAcquisitionStart {
    pub deck_size: usize,
    pub energy: u8,
    pub has_runic_dome: bool,
    pub deck: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BossMatchupPressureClaim {
    pub capability: &'static str,
    pub status: &'static str,
    pub support: Vec<String>,
    pub details: Vec<&'static str>,
}

pub fn boss_matchup_acquisition_pressure_v0(
    combat: &CombatState,
) -> Option<BossMatchupAcquisitionPressureV0> {
    let frame = awakened_one_evidence_frame(combat)?;
    Some(boss_matchup_acquisition_pressure_v0_from_frame(&frame))
}

fn boss_matchup_acquisition_pressure_v0_from_frame(
    frame: &BossMatchupEvidenceFrame,
) -> BossMatchupAcquisitionPressureV0 {
    let already_present = present_claims(frame);
    let missing_or_uncertain = pressure_claims(frame);
    let pressure_tags = pressure_tags(frame, &missing_or_uncertain);
    BossMatchupAcquisitionPressureV0 {
        schema: "boss_matchup_acquisition_pressure_v0",
        contract: "shadow_boss_matchup_acquisition_pressure_no_reward_shop_runner_policy_change",
        boss: "AwakenedOne",
        start: BossMatchupAcquisitionStart {
            deck_size: frame.input.deck_size,
            energy: frame.input.energy,
            has_runic_dome: frame.input.has_runic_dome,
            deck: frame.input.deck.clone(),
        },
        conclusion: conclusion_from_pressure_tags(&pressure_tags),
        pressure_tags,
        already_present,
        missing_or_uncertain,
    }
}

fn present_claims(frame: &BossMatchupEvidenceFrame) -> Vec<BossMatchupPressureClaim> {
    [
        present_from_claim(frame, "damage_scaling_present", "slow_damage_scaling"),
        present_from_claim(frame, "cultist_deadline_plan", "cultist_cleanup_tools"),
        present_from_claim(frame, "generic_block_package", "generic_block_package"),
        present_from_claim(
            frame,
            "deck_access_or_acceleration",
            "deck_access_or_acceleration",
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn present_from_claim(
    frame: &BossMatchupEvidenceFrame,
    claim_id: &'static str,
    capability: &'static str,
) -> Option<BossMatchupPressureClaim> {
    let claim = frame.claims.iter().find(|claim| claim.id == claim_id)?;
    if claim.support.is_empty() {
        return None;
    }
    Some(BossMatchupPressureClaim {
        capability,
        status: present_status(claim),
        support: claim.support.clone(),
        details: vec!["presence evidence only; deployability still requires combat replay"],
    })
}

fn present_status(claim: &BossMatchupEvidenceClaim) -> &'static str {
    match (claim.id, claim.status) {
        ("damage_scaling_present", BossMatchupClaimStatus::SingleSlowSource) => "present",
        ("cultist_deadline_plan", BossMatchupClaimStatus::WeakSupported) => "partial",
        ("generic_block_package", BossMatchupClaimStatus::WeakSupported) => {
            "present_but_not_boss_grade"
        }
        ("deck_access_or_acceleration", BossMatchupClaimStatus::WeakSupported) => "partial",
        (_, BossMatchupClaimStatus::Supported) => "present",
        (_, BossMatchupClaimStatus::WeakSupported) => "partial",
        _ => "present",
    }
}

fn pressure_claims(frame: &BossMatchupEvidenceFrame) -> Vec<BossMatchupPressureClaim> {
    [
        pressure_from_claim_with_capability(frame, "damage_scaling_present", "damage_scaling_plan"),
        pressure_from_claim(frame, "mitigation_or_strength_down"),
        pressure_from_claim(frame, "defensive_engine_or_repeatable_block"),
        pressure_from_claim_with_capability(
            frame,
            "phase2_dark_echo_plan",
            "burst_block_for_dark_echo",
        ),
        pressure_from_claim(frame, "acceleration_for_slow_scaling"),
        pressure_from_claim_with_capability(
            frame,
            "cultist_deadline_plan",
            "cultist_cleanup_consistency",
        ),
        pressure_from_claim(frame, "damage_scaling_multiplier"),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn pressure_from_claim(
    frame: &BossMatchupEvidenceFrame,
    claim_id: &'static str,
) -> Option<BossMatchupPressureClaim> {
    pressure_from_claim_with_capability(frame, claim_id, claim_id)
}

fn pressure_from_claim_with_capability(
    frame: &BossMatchupEvidenceFrame,
    claim_id: &'static str,
    capability: &'static str,
) -> Option<BossMatchupPressureClaim> {
    let claim = frame.claims.iter().find(|claim| claim.id == claim_id)?;
    Some(BossMatchupPressureClaim {
        capability,
        status: pressure_status(claim),
        support: claim.support.clone(),
        details: pressure_details(claim),
    })
}

fn pressure_status(claim: &BossMatchupEvidenceClaim) -> &'static str {
    match claim.status {
        BossMatchupClaimStatus::Supported => "present",
        BossMatchupClaimStatus::WeakSupported | BossMatchupClaimStatus::Uncertain => "uncertain",
        BossMatchupClaimStatus::Unsupported | BossMatchupClaimStatus::NotPresent => "missing",
        BossMatchupClaimStatus::Unknown => "unknown",
        BossMatchupClaimStatus::SingleSlowSource => "present",
    }
}

fn pressure_details(claim: &BossMatchupEvidenceClaim) -> Vec<&'static str> {
    match (claim.id, claim.status) {
        ("phase2_dark_echo_plan", BossMatchupClaimStatus::WeakSupported) => {
            vec!["single draw-dependent block card does not prove Dark Echo is covered"]
        }
        ("cultist_deadline_plan", BossMatchupClaimStatus::WeakSupported) => {
            vec!["non-premium AOE does not prove Cultists die before scaling pressure"]
        }
        ("acceleration_for_slow_scaling", BossMatchupClaimStatus::Uncertain) => {
            vec!["partial access may still be too slow for Demon Form to matter"]
        }
        _ if claim.status == BossMatchupClaimStatus::Supported => {
            vec!["timing and payoff still require combat replay"]
        }
        _ => vec![],
    }
}

fn pressure_tags(
    frame: &BossMatchupEvidenceFrame,
    claims: &[BossMatchupPressureClaim],
) -> Vec<&'static str> {
    let mut tags = Vec::new();
    let has_single_slow_damage_scaling = frame.claims.iter().any(|claim| {
        claim.id == "damage_scaling_present"
            && claim.status == BossMatchupClaimStatus::SingleSlowSource
    });
    if frame.input.has_runic_dome {
        tags.push("runic_dome_increases_survival_pressure");
    }
    if has_single_slow_damage_scaling {
        tags.push("slow_scaling_needs_acceleration");
    }
    for claim in claims {
        match (claim.capability, claim.status) {
            ("damage_scaling_plan", "missing") => tags.push("missing_damage_scaling_plan"),
            ("damage_scaling_plan", "uncertain") => tags.push("thin_damage_scaling_plan"),
            ("mitigation_or_strength_down", "missing") => {
                tags.push("missing_mitigation_or_strength_down")
            }
            ("defensive_engine_or_repeatable_block", "missing") => {
                tags.push("missing_defensive_engine_or_repeatable_block")
            }
            ("burst_block_for_dark_echo", "missing") => tags.push("dark_echo_burst_block_missing"),
            ("burst_block_for_dark_echo", "uncertain") => {
                tags.push("dark_echo_burst_block_uncertain")
            }
            ("cultist_cleanup_consistency", "missing" | "uncertain") => {
                tags.push("cultist_cleanup_deadline_uncertain")
            }
            ("damage_scaling_multiplier", "missing") if has_single_slow_damage_scaling => {
                tags.push("missing_scaling_multiplier_for_slow_damage_source")
            }
            _ => {}
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

fn conclusion_from_pressure_tags(tags: &[&'static str]) -> &'static str {
    let damage_plan_open =
        tags.contains(&"missing_damage_scaling_plan") || tags.contains(&"thin_damage_scaling_plan");
    if damage_plan_open && tags.contains(&"missing_defensive_engine_or_repeatable_block") {
        "boss_matchup_acquisition_pressure_damage_and_survival"
    } else if damage_plan_open {
        "boss_matchup_acquisition_pressure_damage_plan_first"
    } else if tags.contains(&"missing_mitigation_or_strength_down")
        && tags.contains(&"missing_defensive_engine_or_repeatable_block")
    {
        "boss_matchup_acquisition_pressure_survival_first"
    } else if tags.contains(&"slow_scaling_needs_acceleration") {
        "boss_matchup_acquisition_pressure_setup_access_first"
    } else {
        "boss_matchup_acquisition_pressure_review"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::boss_matchup::awakened_one_evidence_frame_from_deck;
    use crate::content::cards::CardId;
    use crate::runtime::combat::CombatCard;

    fn card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    fn upgraded(id: CardId, uuid: u32) -> CombatCard {
        let mut card = CombatCard::for_scenario(id, uuid);
        card.upgrades = 1;
        card
    }

    #[test]
    fn current_like_deck_reports_survival_pressure() {
        let report = boss_matchup_acquisition_pressure_v0_from_deck(
            vec![
                card(CardId::DemonForm, 1),
                card(CardId::Whirlwind, 2),
                upgraded(CardId::Cleave, 3),
                upgraded(CardId::BurningPact, 4),
                upgraded(CardId::ShrugItOff, 5),
                upgraded(CardId::ShrugItOff, 6),
                upgraded(CardId::FlameBarrier, 7),
                upgraded(CardId::Defend, 8),
            ],
            true,
        );

        assert_eq!(
            report.conclusion,
            "boss_matchup_acquisition_pressure_survival_first"
        );
        for expected in [
            "missing_mitigation_or_strength_down",
            "missing_defensive_engine_or_repeatable_block",
            "dark_echo_burst_block_uncertain",
            "slow_scaling_needs_acceleration",
            "cultist_cleanup_deadline_uncertain",
            "runic_dome_increases_survival_pressure",
        ] {
            assert!(report.pressure_tags.contains(&expected), "{expected}");
        }
    }

    #[test]
    fn flame_barrier_alone_does_not_count_as_dark_echo_plan() {
        let report = boss_matchup_acquisition_pressure_v0_from_deck(
            vec![
                card(CardId::DemonForm, 1),
                upgraded(CardId::FlameBarrier, 2),
            ],
            false,
        );

        assert!(report
            .pressure_tags
            .contains(&"dark_echo_burst_block_uncertain"));
        assert!(report
            .pressure_tags
            .contains(&"missing_mitigation_or_strength_down"));
        assert!(report
            .pressure_tags
            .contains(&"missing_defensive_engine_or_repeatable_block"));
    }

    #[test]
    fn multiplier_and_exhaust_draw_do_not_invent_damage_or_defensive_engines() {
        let report = boss_matchup_acquisition_pressure_v0_from_deck(
            vec![
                card(CardId::LimitBreak, 1),
                card(CardId::DarkEmbrace, 2),
                card(CardId::Whirlwind, 3),
            ],
            false,
        );

        assert!(report
            .pressure_tags
            .contains(&"missing_damage_scaling_plan"));
        assert!(report
            .pressure_tags
            .contains(&"missing_defensive_engine_or_repeatable_block"));
        assert_eq!(
            report.conclusion,
            "boss_matchup_acquisition_pressure_damage_and_survival"
        );
    }

    #[test]
    fn exhaust_compression_support_keeps_damage_plan_pressure_open() {
        let report = boss_matchup_acquisition_pressure_v0_from_deck(
            vec![card(CardId::DarkEmbrace, 1), card(CardId::TrueGrit, 2)],
            false,
        );

        assert!(report.pressure_tags.contains(&"thin_damage_scaling_plan"));
        assert_eq!(
            report.conclusion,
            "boss_matchup_acquisition_pressure_damage_and_survival"
        );
    }

    fn boss_matchup_acquisition_pressure_v0_from_deck(
        deck: Vec<CombatCard>,
        has_runic_dome: bool,
    ) -> BossMatchupAcquisitionPressureV0 {
        let frame = awakened_one_evidence_frame_from_deck(deck, 4, has_runic_dome);
        let already_present = present_claims(&frame);
        let missing_or_uncertain = pressure_claims(&frame);
        let pressure_tags = pressure_tags(&frame, &missing_or_uncertain);
        BossMatchupAcquisitionPressureV0 {
            schema: "test",
            contract: "test",
            boss: "AwakenedOne",
            start: BossMatchupAcquisitionStart {
                deck_size: frame.input.deck_size,
                energy: frame.input.energy,
                has_runic_dome: frame.input.has_runic_dome,
                deck: frame.input.deck,
            },
            conclusion: conclusion_from_pressure_tags(&pressure_tags),
            pressure_tags,
            already_present,
            missing_or_uncertain,
        }
    }
}
