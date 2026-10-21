use serde::{Deserialize, Serialize};

use super::types::{DecisionSiteKindV1, NonCombatDecisionRecordV1, PolicySelectionStatusV1};
use super::validation::{
    render_noncombat_decision_record_validation_errors, validate_noncombat_decision_record_v1,
};

pub const NONCOMBAT_OUTCOME_ATTACHMENT_SCHEMA_NAME: &str = "NonCombatOutcomeAttachmentV1";
pub const NONCOMBAT_OUTCOME_ATTACHMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NonCombatOutcomeAttachmentV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub label_role: String,
    pub trainable_as_action_label: bool,
    pub policy_quality_claim: bool,
    pub site: DecisionSiteKindV1,
    pub decision_record_hash: String,
    pub window: NonCombatOutcomeWindowV1,
    pub before: NonCombatOutcomeSnapshotV1,
    pub after: NonCombatOutcomeSnapshotV1,
    pub metrics: NonCombatOutcomeMetricsV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_reward: Option<CardRewardOutcomeAttachmentV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardRewardOutcomeAttachmentV1 {
    pub selected_candidate_id: String,
    pub picked_card_label: String,
    pub floor_reached_after_decision: i32,
    pub next_combat_hp_loss: Option<i32>,
    pub hp_before_next_elite: Option<i32>,
    pub hp_after_next_elite: Option<i32>,
    pub hp_before_boss: Option<i32>,
    pub picked_card_drawn_count: Option<u32>,
    pub picked_card_played_count: Option<u32>,
    pub picked_card_upgraded_before_boss: Option<bool>,
    pub picked_card_removed_later: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardRewardOutcomeObservationV1 {
    pub picked_card_drawn_count: Option<u32>,
    pub picked_card_played_count: Option<u32>,
    pub picked_card_upgraded_before_boss: Option<bool>,
    pub picked_card_removed_later: Option<bool>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NonCombatOutcomeWindowV1 {
    AfterOneFloor,
    AfterThreeFloors,
    BeforeNextElite,
    AfterNextElite,
    BeforeBoss,
    AfterBoss,
    Manual,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NonCombatOutcomeSnapshotV1 {
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub deck_size: usize,
    pub relic_count: usize,
    pub potion_count: usize,
    pub combats_completed: u32,
    pub elites_completed: u32,
    pub bosses_completed: u32,
    pub run_terminal: Option<NonCombatRunTerminalV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NonCombatRunTerminalV1 {
    Victory,
    Loss,
    Abandoned,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NonCombatOutcomeMetricsV1 {
    pub act_delta: i32,
    pub floor_delta: i32,
    pub hp_delta: i32,
    pub max_hp_delta: i32,
    pub gold_delta: i32,
    pub deck_size_delta: i32,
    pub relic_count_delta: i32,
    pub potion_count_delta: i32,
    pub combats_completed_delta: i32,
    pub elites_completed_delta: i32,
    pub bosses_completed_delta: i32,
    pub terminal_changed: bool,
}

pub fn attach_noncombat_outcome_v1(
    record: &NonCombatDecisionRecordV1,
    window: NonCombatOutcomeWindowV1,
    before: NonCombatOutcomeSnapshotV1,
    after: NonCombatOutcomeSnapshotV1,
) -> Result<NonCombatOutcomeAttachmentV1, String> {
    attach_noncombat_outcome_with_card_reward_observation_v1(
        record,
        window,
        before,
        after,
        CardRewardOutcomeObservationV1::default(),
    )
}

pub fn attach_noncombat_outcome_with_card_reward_observation_v1(
    record: &NonCombatDecisionRecordV1,
    window: NonCombatOutcomeWindowV1,
    before: NonCombatOutcomeSnapshotV1,
    after: NonCombatOutcomeSnapshotV1,
    card_reward_observation: CardRewardOutcomeObservationV1,
) -> Result<NonCombatOutcomeAttachmentV1, String> {
    validate_noncombat_decision_record_v1(record).map_err(|errors| {
        format!(
            "cannot attach outcome to invalid NonCombatDecisionRecordV1: {}",
            render_noncombat_decision_record_validation_errors(&errors)
        )
    })?;

    let metrics = outcome_metrics(&before, &after);
    let card_reward = card_reward_outcome(
        record,
        window,
        &before,
        &after,
        &metrics,
        card_reward_observation,
    );
    Ok(NonCombatOutcomeAttachmentV1 {
        schema_name: NONCOMBAT_OUTCOME_ATTACHMENT_SCHEMA_NAME.to_string(),
        schema_version: NONCOMBAT_OUTCOME_ATTACHMENT_SCHEMA_VERSION,
        label_role: "diagnostic_not_teacher_label".to_string(),
        trainable_as_action_label: false,
        policy_quality_claim: false,
        site: record.site,
        decision_record_hash: decision_record_hash(record)?,
        window,
        metrics,
        before,
        after,
        card_reward,
    })
}

fn card_reward_outcome(
    record: &NonCombatDecisionRecordV1,
    window: NonCombatOutcomeWindowV1,
    before: &NonCombatOutcomeSnapshotV1,
    after: &NonCombatOutcomeSnapshotV1,
    metrics: &NonCombatOutcomeMetricsV1,
    observation: CardRewardOutcomeObservationV1,
) -> Option<CardRewardOutcomeAttachmentV1> {
    if record.site != DecisionSiteKindV1::CardReward {
        return None;
    }
    if record.selection.status != PolicySelectionStatusV1::Selected {
        return None;
    }
    let selected_candidate_id = record.selection.selected_candidate_id.clone()?;
    let picked_card_label = record
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == selected_candidate_id)
        .map(|candidate| candidate.label.clone())
        .unwrap_or_else(|| selected_candidate_id.clone());

    Some(CardRewardOutcomeAttachmentV1 {
        selected_candidate_id,
        picked_card_label,
        floor_reached_after_decision: after.floor,
        next_combat_hp_loss: (matches!(window, NonCombatOutcomeWindowV1::AfterOneFloor)
            && metrics.combats_completed_delta > 0)
            .then_some((before.current_hp - after.current_hp).max(0)),
        hp_before_next_elite: matches!(window, NonCombatOutcomeWindowV1::BeforeNextElite)
            .then_some(after.current_hp),
        hp_after_next_elite: (matches!(window, NonCombatOutcomeWindowV1::AfterNextElite)
            && metrics.elites_completed_delta > 0)
            .then_some(after.current_hp),
        hp_before_boss: None,
        picked_card_drawn_count: observation.picked_card_drawn_count,
        picked_card_played_count: observation.picked_card_played_count,
        picked_card_upgraded_before_boss: observation.picked_card_upgraded_before_boss,
        picked_card_removed_later: observation.picked_card_removed_later,
    })
}

fn outcome_metrics(
    before: &NonCombatOutcomeSnapshotV1,
    after: &NonCombatOutcomeSnapshotV1,
) -> NonCombatOutcomeMetricsV1 {
    NonCombatOutcomeMetricsV1 {
        act_delta: i32::from(after.act) - i32::from(before.act),
        floor_delta: after.floor - before.floor,
        hp_delta: after.current_hp - before.current_hp,
        max_hp_delta: after.max_hp - before.max_hp,
        gold_delta: after.gold - before.gold,
        deck_size_delta: after.deck_size as i32 - before.deck_size as i32,
        relic_count_delta: after.relic_count as i32 - before.relic_count as i32,
        potion_count_delta: after.potion_count as i32 - before.potion_count as i32,
        combats_completed_delta: after.combats_completed as i32 - before.combats_completed as i32,
        elites_completed_delta: after.elites_completed as i32 - before.elites_completed as i32,
        bosses_completed_delta: after.bosses_completed as i32 - before.bosses_completed as i32,
        terminal_changed: before.run_terminal != after.run_terminal,
    }
}

fn decision_record_hash(record: &NonCombatDecisionRecordV1) -> Result<String, String> {
    noncombat_decision_record_hash_v1(record)
}

pub fn noncombat_decision_record_hash_v1(
    record: &NonCombatDecisionRecordV1,
) -> Result<String, String> {
    let bytes = serde_json::to_vec(record)
        .map_err(|err| format!("failed to serialize decision record for hashing: {err}"))?;
    Ok(super::hash::hash_bytes(&bytes))
}
