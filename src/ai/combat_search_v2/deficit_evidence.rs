use serde::Serialize;

use super::line_lab::{CombatLineLabReport, CombatLineLabTurnPoolLineSummary};
use super::SearchTerminalLabel;

#[derive(Clone, Debug, Serialize)]
pub struct CombatDeficitEvidenceReport {
    pub schema: &'static str,
    pub flags: Vec<CombatDeficitEvidenceFlag>,
    pub observations: CombatDeficitEvidenceObservations,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatDeficitEvidenceFlag {
    BossScalingPressure,
    AoeOrMinionControlPressure,
    SurvivalPressure,
    DeckAccessOrSetupPressure,
    PotionDependentMargin,
    SearchStillAmbiguous,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatDeficitEvidenceObservations {
    pub turn_pool_lanes: usize,
    pub turn_pool_deadline_hit: bool,
    pub win_lanes: usize,
    pub all_lanes_loss: bool,
    pub best_lane: Option<&'static str>,
    pub best_terminal: Option<SearchTerminalLabel>,
    pub best_final_hp: Option<i32>,
    pub best_total_enemy_hp: Option<i32>,
    pub best_living_enemy_count: Option<usize>,
    pub best_potions_used: Option<u32>,
    pub lowest_enemy_hp: Option<i32>,
    pub highest_final_hp: Option<i32>,
    pub max_powers_played: Option<u32>,
}

pub fn derive_combat_deficit_evidence(report: &CombatLineLabReport) -> CombatDeficitEvidenceReport {
    let lanes = report
        .turn_pool
        .as_ref()
        .map(|pool| pool.lanes.as_slice())
        .unwrap_or(&[]);
    let best = report
        .turn_pool
        .as_ref()
        .and_then(|pool| pool.best.as_ref());
    let deadline_hit = report
        .turn_pool
        .as_ref()
        .is_some_and(|pool| pool.deadline_hit);
    let win_lanes = lanes
        .iter()
        .filter(|line| line.terminal == SearchTerminalLabel::Win)
        .count();
    let all_lanes_loss = !lanes.is_empty()
        && lanes
            .iter()
            .all(|line| line.terminal == SearchTerminalLabel::Loss);
    let lowest_enemy_hp = lanes.iter().map(|line| line.total_enemy_hp).min();
    let highest_final_hp = lanes.iter().map(|line| line.final_hp).max();
    let max_powers_played = lanes.iter().map(|line| line.powers_played).max();
    let observations = CombatDeficitEvidenceObservations {
        turn_pool_lanes: lanes.len(),
        turn_pool_deadline_hit: deadline_hit,
        win_lanes,
        all_lanes_loss,
        best_lane: best.map(|line| line.lane),
        best_terminal: best.map(|line| line.terminal),
        best_final_hp: best.map(|line| line.final_hp),
        best_total_enemy_hp: best.map(|line| line.total_enemy_hp),
        best_living_enemy_count: best.map(|line| line.living_enemy_count),
        best_potions_used: best.map(|line| line.potions_used),
        lowest_enemy_hp,
        highest_final_hp,
        max_powers_played,
    };
    let mut flags = Vec::new();
    push_unique(
        &mut flags,
        evidence_from_ambiguity(lanes, deadline_hit, best).as_slice(),
    );
    push_unique(&mut flags, evidence_from_losses(lanes, best).as_slice());
    push_unique(&mut flags, evidence_from_lane_contrast(lanes).as_slice());
    CombatDeficitEvidenceReport {
        schema: "combat_deficit_evidence_v0",
        flags,
        observations,
    }
}

fn evidence_from_ambiguity(
    lanes: &[CombatLineLabTurnPoolLineSummary],
    deadline_hit: bool,
    best: Option<&CombatLineLabTurnPoolLineSummary>,
) -> Vec<CombatDeficitEvidenceFlag> {
    if lanes.is_empty() || deadline_hit || best.is_none() {
        vec![CombatDeficitEvidenceFlag::SearchStillAmbiguous]
    } else {
        Vec::new()
    }
}

fn evidence_from_losses(
    lanes: &[CombatLineLabTurnPoolLineSummary],
    best: Option<&CombatLineLabTurnPoolLineSummary>,
) -> Vec<CombatDeficitEvidenceFlag> {
    if lanes.is_empty()
        || lanes
            .iter()
            .any(|line| line.terminal == SearchTerminalLabel::Win)
    {
        return Vec::new();
    }
    let mut flags = Vec::new();
    if lanes.iter().all(|line| line.living_enemy_count > 1) {
        flags.push(CombatDeficitEvidenceFlag::AoeOrMinionControlPressure);
    }
    if let Some(best) = best {
        if best.living_enemy_count == 1 && best.total_enemy_hp >= 80 {
            flags.push(CombatDeficitEvidenceFlag::BossScalingPressure);
        }
        if best.living_enemy_count == 1 && best.total_enemy_hp <= 60 {
            flags.push(CombatDeficitEvidenceFlag::SurvivalPressure);
        }
        if best.potions_used > 0 && best.total_enemy_hp <= 80 {
            flags.push(CombatDeficitEvidenceFlag::PotionDependentMargin);
        }
    }
    flags
}

fn evidence_from_lane_contrast(
    lanes: &[CombatLineLabTurnPoolLineSummary],
) -> Vec<CombatDeficitEvidenceFlag> {
    let Some(setup) = line_for_lane(lanes, "setup") else {
        return Vec::new();
    };
    let comparison_enemy_hp = ["damage", "power_delay", "survival"]
        .iter()
        .filter_map(|lane| line_for_lane(lanes, lane))
        .map(|line| line.total_enemy_hp)
        .min();
    if setup.powers_played >= 2
        && comparison_enemy_hp.is_some_and(|enemy_hp| setup.total_enemy_hp + 50 <= enemy_hp)
    {
        vec![CombatDeficitEvidenceFlag::DeckAccessOrSetupPressure]
    } else {
        Vec::new()
    }
}

fn line_for_lane<'a>(
    lanes: &'a [CombatLineLabTurnPoolLineSummary],
    lane: &str,
) -> Option<&'a CombatLineLabTurnPoolLineSummary> {
    lanes.iter().find(|line| line.lane == lane)
}

fn push_unique<T: Copy + Eq>(target: &mut Vec<T>, values: &[T]) {
    for value in values {
        if !target.contains(value) {
            target.push(*value);
        }
    }
}
