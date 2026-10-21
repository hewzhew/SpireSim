use serde::Serialize;

use super::types::{BossMatchupClaimStatus, BossMatchupEvidenceClaim, BossMatchupEvidenceFrame};

#[derive(Clone, Debug, Serialize)]
pub struct BossMatchupRiskSummaryV0 {
    pub risk_tags: Vec<&'static str>,
    pub conclusion: &'static str,
}

pub fn boss_matchup_static_risk_summary_v0(
    frame: &BossMatchupEvidenceFrame,
) -> BossMatchupRiskSummaryV0 {
    let risk_tags = boss_matchup_static_risk_tags(&frame.claims);
    let conclusion = boss_matchup_static_conclusion_from_risk_tags(&risk_tags);
    BossMatchupRiskSummaryV0 {
        risk_tags,
        conclusion,
    }
}

pub fn boss_matchup_static_conclusion_from_risk_tags(risk_tags: &[&'static str]) -> &'static str {
    if risk_tags.contains(&"missing_defensive_scaling_or_mitigation") {
        "boss_plan_thin_with_missing_survival_plan"
    } else {
        "awakened_one_boss_plan_needs_review"
    }
}

fn boss_matchup_static_risk_tags(claims: &[BossMatchupEvidenceClaim]) -> Vec<&'static str> {
    let mut tags = Vec::new();
    for claim in claims {
        match (claim.id, claim.status) {
            ("damage_scaling_present", BossMatchupClaimStatus::SingleSlowSource) => {
                tags.push("single_slow_damage_scaling_source")
            }
            ("defensive_scaling_or_mitigation_present", BossMatchupClaimStatus::Unsupported) => {
                tags.push("missing_defensive_scaling_or_mitigation")
            }
            (
                "cultist_deadline_plan",
                BossMatchupClaimStatus::Unsupported | BossMatchupClaimStatus::WeakSupported,
            ) => tags.push("cultist_cleanup_deadline_uncertain"),
            ("phase2_dark_echo_plan", BossMatchupClaimStatus::Unsupported) => {
                tags.push("phase2_dark_echo_plan_missing")
            }
            ("phase2_dark_echo_plan", BossMatchupClaimStatus::WeakSupported) => {
                tags.push("phase2_dark_echo_plan_uncertain")
            }
            ("awakened_one_power_penalty_exposure", BossMatchupClaimStatus::Supported) => {
                tags.push("awakened_one_power_penalty_exposure")
            }
            _ => {}
        }
    }
    tags.sort();
    tags.dedup();
    tags
}
