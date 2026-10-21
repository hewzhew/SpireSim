use serde::{Deserialize, Serialize};
use sts_oracle_runtime::ai::boss_matchup::{
    awakened_one_evidence_frame, awakened_one_evidence_frame_from_deck,
    boss_matchup_static_conclusion_from_risk_tags, boss_matchup_static_risk_summary_v0,
    is_awakened_one_case, BossMatchupEvidenceClaim, BossMatchupEvidenceFrame,
};
use sts_oracle_runtime::content::cards::CardId;
use sts_oracle_runtime::content::monsters::EnemyId;
use sts_oracle_runtime::eval::combat_case::{CombatCase, CombatCasePathStep};
use sts_oracle_runtime::runtime::combat::{CombatCard, CombatState};

use super::counterfactual_hp::CounterfactualHpProbe;

#[derive(Clone, Serialize)]
pub(crate) struct StaticBossMatchupAuditV0 {
    pub(super) schema: &'static str,
    pub(super) contract: &'static str,
    pub(super) boss: &'static str,
    pub(super) start: AwakenedOneStartEvidence,
    pub(super) claims: Vec<AwakenedOneEvidenceClaim>,
    pub(super) risk_tags: Vec<&'static str>,
    pub(super) conclusion: &'static str,
}

#[derive(Clone, Serialize)]
pub(crate) struct AwakenedOnePathAuditV0 {
    pub(super) schema: &'static str,
    pub(super) contract: &'static str,
    pub(super) boss: &'static str,
    pub(super) first_known_boss_alarm: Option<AwakenedOnePathAlarm>,
    pub(super) first_retrospective_alarm: Option<AwakenedOnePathAlarm>,
    pub(super) steps: Vec<AwakenedOnePathAuditStep>,
}

#[derive(Clone, Serialize)]
pub(super) struct AwakenedOnePathAlarm {
    pub(super) path_index: usize,
    pub(super) label: String,
    pub(super) previous_label: Option<String>,
    pub(super) state_point: &'static str,
    pub(super) act: u8,
    pub(super) floor: i32,
    pub(super) hp: i32,
    pub(super) max_hp: i32,
    pub(super) deck_size: usize,
    pub(super) risk_tags: Vec<&'static str>,
    pub(super) conclusion: &'static str,
}

#[derive(Clone, Serialize)]
pub(super) struct AwakenedOnePathAuditStep {
    pub(super) path_index: usize,
    pub(super) label: String,
    pub(super) previous_label: Option<String>,
    pub(super) state_point: &'static str,
    pub(super) act: u8,
    pub(super) floor: i32,
    pub(super) hp: i32,
    pub(super) max_hp: i32,
    pub(super) deck_size: usize,
    pub(super) deck: Vec<String>,
    pub(super) risk_tags: Vec<&'static str>,
    pub(super) conclusion: &'static str,
    pub(super) known_boss_policy_scope: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct AwakenedOneFailureEvidenceFrame {
    pub(super) schema: &'static str,
    pub(super) contract: &'static str,
    pub(super) boss: &'static str,
    pub(super) start: AwakenedOneStartEvidence,
    pub(super) claims: Vec<AwakenedOneEvidenceClaim>,
    pub(super) risk_tags: Vec<&'static str>,
    pub(super) conclusion: &'static str,
}

#[derive(Clone, Serialize)]
pub(super) struct AwakenedOneStartEvidence {
    pub(super) turn: u32,
    pub(super) player_hp: i32,
    pub(super) player_max_hp: i32,
    pub(super) deck_size: usize,
    pub(super) power_cards: Vec<String>,
    pub(super) cultists_alive: usize,
    pub(super) awakened_hp: Option<i32>,
    pub(super) awakened_max_hp: Option<i32>,
    pub(super) total_enemy_hp: i32,
}

#[derive(Clone, Serialize)]
pub(super) struct AwakenedOneEvidenceClaim {
    pub(super) claim: &'static str,
    pub(super) status: &'static str,
    pub(super) support: Vec<String>,
    pub(super) counterevidence: Vec<String>,
    pub(super) unknown: Vec<String>,
}

#[derive(Deserialize)]
struct PathStateSnapshot {
    act: u8,
    floor: i32,
    hp: i32,
    max_hp: i32,
    deck_size: usize,
    #[serde(default)]
    deck: Vec<PathCardSnapshot>,
}

#[derive(Deserialize)]
struct PathCardSnapshot {
    id: CardId,
    lineage: sts_oracle_runtime::runtime::combat::CardLineageId,
    #[serde(default)]
    upgrades: u8,
}

pub(super) fn awakened_one_failure_evidence(
    case: &CombatCase,
    hp_probe: Option<&CounterfactualHpProbe>,
) -> Option<AwakenedOneFailureEvidenceFrame> {
    let static_audit = static_boss_matchup_audit_v0(case)?;
    let mut claims = static_audit.claims.clone();
    let mut risk_tags = static_audit.risk_tags.clone();
    if let Some(probe) = hp_probe {
        let full_hp_claim = full_hp_probe_claim(probe);
        if full_hp_claim.status == "supports_not_low_hp_only" {
            risk_tags.push("full_hp_no_win_found");
        }
        claims.push(full_hp_claim);
    } else {
        claims.push(AwakenedOneEvidenceClaim {
            claim: "full_hp_counterfactual_probe",
            status: "unknown",
            support: vec![],
            counterevidence: vec![],
            unknown: vec!["counterfactual_hp_probe was not run".to_string()],
        });
    }

    risk_tags.sort();
    risk_tags.dedup();
    let conclusion = failure_conclusion_from_risk_tags(&risk_tags);
    Some(AwakenedOneFailureEvidenceFrame {
        schema: "awakened_one_failure_evidence_frame_v0",
        contract: "review_only_boss_plan_claims_with_support_counterevidence_unknown_no_runner_policy_change",
        boss: "AwakenedOne",
        start: static_audit.start,
        claims,
        conclusion,
        risk_tags,
    })
}

pub(super) fn awakened_one_path_audit_v0(case: &CombatCase) -> Option<AwakenedOnePathAuditV0> {
    if !is_awakened_one_case(&case.core.position.combat) {
        return None;
    }

    let mut steps = Vec::new();
    for (path_index, step) in case.path.iter().enumerate() {
        let previous_label = path_index
            .checked_sub(1)
            .and_then(|previous| case.path.get(previous))
            .map(|previous| previous.label.clone());
        if let Some(audit_step) = path_audit_step(path_index, previous_label, step) {
            steps.push(audit_step);
        }
    }
    if steps.is_empty() {
        return None;
    }

    let first_known_boss_alarm = steps
        .iter()
        .find(|step| step.known_boss_policy_scope && is_alarm_step(step))
        .map(path_alarm_from_step);
    let first_retrospective_alarm = steps
        .iter()
        .find(|step| is_retrospective_alarm_step(step))
        .map(path_alarm_from_step);

    Some(AwakenedOnePathAuditV0 {
        schema: "awakened_one_path_audit_v0",
        contract: "review_only_replay_of_static_boss_plan_claims_on_recorded_path_states_no_runner_policy_change",
        boss: "AwakenedOne",
        first_known_boss_alarm,
        first_retrospective_alarm,
        steps,
    })
}

pub(super) fn static_boss_matchup_audit_v0(case: &CombatCase) -> Option<StaticBossMatchupAuditV0> {
    let frame = awakened_one_evidence_frame(&case.core.position.combat)?;
    let claims = evidence_claims_from_frame(&frame);
    let risk_summary = boss_matchup_static_risk_summary_v0(&frame);
    Some(StaticBossMatchupAuditV0 {
        schema: "static_boss_matchup_audit_v0",
        contract:
            "shadow_static_boss_plan_claims_from_boss_deck_relic_potion_energy_no_combat_outcome",
        boss: "AwakenedOne",
        start: start_evidence(&case.core.position.combat, &frame),
        claims,
        risk_tags: risk_summary.risk_tags,
        conclusion: risk_summary.conclusion,
    })
}

fn path_audit_step(
    path_index: usize,
    previous_label: Option<String>,
    step: &CombatCasePathStep,
) -> Option<AwakenedOnePathAuditStep> {
    let state = step.state_before.as_ref()?;
    let snapshot: PathStateSnapshot = serde_json::from_value(state.clone()).ok()?;
    if snapshot.deck.is_empty() {
        return None;
    }
    let frame =
        awakened_one_evidence_frame_from_deck(path_deck_to_combat_cards(&snapshot.deck), 0, false);
    let risk_summary = boss_matchup_static_risk_summary_v0(&frame);
    Some(AwakenedOnePathAuditStep {
        path_index,
        label: step.label.clone(),
        previous_label,
        state_point: "before_decision",
        act: snapshot.act,
        floor: snapshot.floor,
        hp: snapshot.hp,
        max_hp: snapshot.max_hp,
        deck_size: snapshot.deck_size,
        deck: frame.input.deck,
        risk_tags: risk_summary.risk_tags,
        conclusion: risk_summary.conclusion,
        known_boss_policy_scope: snapshot.act >= 3,
    })
}

fn path_deck_to_combat_cards(cards: &[PathCardSnapshot]) -> Vec<CombatCard> {
    cards
        .iter()
        .map(|card| {
            let mut combat_card = CombatCard::with_lineage(card.id, card.lineage);
            combat_card.upgrades = card.upgrades;
            combat_card
        })
        .collect()
}

fn is_alarm_step(step: &AwakenedOnePathAuditStep) -> bool {
    step.risk_tags
        .contains(&"missing_defensive_scaling_or_mitigation")
        && step.risk_tags.contains(&"phase2_dark_echo_plan_missing")
}

fn is_retrospective_alarm_step(step: &AwakenedOnePathAuditStep) -> bool {
    is_alarm_step(step)
        && step.risk_tags.iter().any(|tag| {
            *tag == "single_slow_damage_scaling_source"
                || *tag == "awakened_one_power_penalty_exposure"
        })
}

fn path_alarm_from_step(step: &AwakenedOnePathAuditStep) -> AwakenedOnePathAlarm {
    AwakenedOnePathAlarm {
        path_index: step.path_index,
        label: step.label.clone(),
        previous_label: step.previous_label.clone(),
        state_point: step.state_point,
        act: step.act,
        floor: step.floor,
        hp: step.hp,
        max_hp: step.max_hp,
        deck_size: step.deck_size,
        risk_tags: step.risk_tags.clone(),
        conclusion: step.conclusion,
    }
}

fn evidence_claims_from_frame(frame: &BossMatchupEvidenceFrame) -> Vec<AwakenedOneEvidenceClaim> {
    frame.claims.iter().map(evidence_claim_from_core).collect()
}

fn evidence_claim_from_core(claim: &BossMatchupEvidenceClaim) -> AwakenedOneEvidenceClaim {
    AwakenedOneEvidenceClaim {
        claim: claim.id,
        status: claim.status.as_str(),
        support: claim.support.clone(),
        counterevidence: claim.counterevidence.clone(),
        unknown: claim.unknown.clone(),
    }
}

fn full_hp_probe_claim(probe: &CounterfactualHpProbe) -> AwakenedOneEvidenceClaim {
    let mut support = vec![
        format!("hp_probe_classification={}", probe.classification_label()),
        format!("original_hp={}/{}", probe.original_hp(), probe.max_hp()),
    ];
    if let Some(enemy_hp) = probe.full_hp_best_progress_enemy_hp() {
        support.push(format!("full_hp_best_enemy_hp_remaining={enemy_hp}"));
    }
    if let Some(turns) = probe.full_hp_best_progress_turns() {
        support.push(format!("full_hp_best_progress_turns={turns}"));
    }
    AwakenedOneEvidenceClaim {
        claim: "full_hp_counterfactual_probe",
        status: if probe.full_hp_complete_win() == Some(false) && !probe.any_complete_win() {
            "supports_not_low_hp_only"
        } else if probe.any_complete_win() {
            "counterfactual_win_found"
        } else {
            "unknown"
        },
        support,
        counterevidence: vec![],
        unknown: vec!["review lanes are not a proof of unwinnability".to_string()],
    }
}

fn failure_conclusion_from_risk_tags(risk_tags: &[&'static str]) -> &'static str {
    if risk_tags.contains(&"full_hp_no_win_found")
        && risk_tags.contains(&"missing_defensive_scaling_or_mitigation")
    {
        "likely_boss_plan_insufficient_not_low_hp_only"
    } else {
        boss_matchup_static_conclusion_from_risk_tags(risk_tags)
    }
}

fn start_evidence(
    combat: &CombatState,
    frame: &BossMatchupEvidenceFrame,
) -> AwakenedOneStartEvidence {
    let awakened = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.enemy_id == EnemyId::AwakenedOne);
    AwakenedOneStartEvidence {
        turn: combat.turn.turn_count,
        player_hp: combat.entities.player.current_hp,
        player_max_hp: combat.entities.player.max_hp,
        deck_size: frame.input.deck_size,
        power_cards: frame
            .claims
            .iter()
            .find(|claim| claim.id == "awakened_one_power_penalty_exposure")
            .map(|claim| claim.support.clone())
            .unwrap_or_default(),
        cultists_alive: combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.enemy_id == EnemyId::Cultist)
            .filter(|monster| monster.is_alive_for_action())
            .count(),
        awakened_hp: awakened.map(|monster| monster.current_hp),
        awakened_max_hp: awakened.map(|monster| monster.max_hp),
        total_enemy_hp: combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .map(|monster| monster.current_hp.max(0) + monster.block.max(0))
            .sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_step_with_tags(tags: Vec<&'static str>) -> AwakenedOnePathAuditStep {
        AwakenedOnePathAuditStep {
            path_index: 0,
            label: "test".to_string(),
            previous_label: None,
            state_point: "before_decision",
            act: 2,
            floor: 26,
            hp: 47,
            max_hp: 99,
            deck_size: 17,
            deck: Vec::new(),
            risk_tags: tags,
            conclusion: "boss_plan_thin_with_missing_survival_plan",
            known_boss_policy_scope: false,
        }
    }

    #[test]
    fn full_hp_risk_requires_counterfactual_hp_evidence() {
        let tags = vec!["missing_defensive_scaling_or_mitigation"];

        assert!(!tags.contains(&"full_hp_no_win_found"));
        assert_eq!(
            failure_conclusion_from_risk_tags(&tags),
            "boss_plan_thin_with_missing_survival_plan"
        );
    }

    #[test]
    fn expected_awakened_one_case_tags_imply_low_hp_is_not_enough() {
        let tags = vec![
            "awakened_one_power_penalty_exposure",
            "cultist_cleanup_deadline_uncertain",
            "full_hp_no_win_found",
            "missing_defensive_scaling_or_mitigation",
            "phase2_dark_echo_plan_missing",
            "single_slow_damage_scaling_source",
        ];

        for expected in [
            "awakened_one_power_penalty_exposure",
            "cultist_cleanup_deadline_uncertain",
            "full_hp_no_win_found",
            "missing_defensive_scaling_or_mitigation",
            "phase2_dark_echo_plan_missing",
            "single_slow_damage_scaling_source",
        ] {
            assert!(tags.contains(&expected), "missing {expected}");
        }
        assert_eq!(
            failure_conclusion_from_risk_tags(&tags),
            "likely_boss_plan_insufficient_not_low_hp_only"
        );
    }

    #[test]
    fn flame_barrier_is_big_block_not_defensive_scaling_for_awakened_one() {
        let frame = awakened_one_evidence_frame_from_deck(
            vec![CombatCard::for_scenario(CardId::FlameBarrier, 1)],
            0,
            false,
        );
        let claims = evidence_claims_from_frame(&frame);

        let defensive = claims
            .iter()
            .find(|claim| claim.claim == "defensive_scaling_or_mitigation_present")
            .cloned()
            .expect("defensive claim");
        assert_eq!(defensive.status, "unsupported");
        assert!(defensive
            .counterevidence
            .iter()
            .any(|line| line.contains("generic block cards do not establish")));

        let dark_echo = claims
            .iter()
            .find(|claim| claim.claim == "phase2_dark_echo_plan")
            .cloned()
            .expect("dark echo claim");
        assert_eq!(dark_echo.status, "weak_supported");
        assert_eq!(dark_echo.support, vec!["Flame Barrier+0"]);

        let risk_summary = boss_matchup_static_risk_summary_v0(&frame);
        assert!(risk_summary
            .risk_tags
            .contains(&"missing_defensive_scaling_or_mitigation"));
        assert!(risk_summary
            .risk_tags
            .contains(&"phase2_dark_echo_plan_uncertain"));
    }

    #[test]
    fn retrospective_path_alarm_requires_scaling_context() {
        let starter_only = path_step_with_tags(vec![
            "cultist_cleanup_deadline_uncertain",
            "missing_defensive_scaling_or_mitigation",
            "phase2_dark_echo_plan_missing",
        ]);
        assert!(!is_retrospective_alarm_step(&starter_only));

        let slow_scaling = path_step_with_tags(vec![
            "cultist_cleanup_deadline_uncertain",
            "missing_defensive_scaling_or_mitigation",
            "phase2_dark_echo_plan_missing",
            "single_slow_damage_scaling_source",
        ]);
        assert!(is_retrospective_alarm_step(&slow_scaling));
    }
}
