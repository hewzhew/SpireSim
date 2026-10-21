use std::{cell::RefCell, collections::BTreeMap};

use serde::{Deserialize, Serialize};

use crate::ai::combat_state_key::combat_exact_state_hash_v2;
use crate::content::potions::PotionId;

use super::combat_line_executor::drawn_cards_from_action_result;
use super::combat_line_trace::{
    combat_automation_opportunity_state_v1, combat_automation_step_state_v1,
};
use super::oracle_run_explorer::run_session_fingerprint_v3;
use super::{
    CombatAutomationOpportunityStateV1, DecisionCandidateKey, RunCombatResolutionBoundaryV1,
    RunCombatResolutionV1, RunControlConfig, RunControlSession, RunDecisionBoundaryV1,
    RunDecisionTransactionV1, RunProgressJournalV1, RunProgressStepV1,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExactRunProgressReplayReportV1 {
    pub seed: u64,
    pub ascension: u8,
    pub journal_entries: usize,
    pub decisions: usize,
    pub forced_transitions: usize,
    pub combat_resolutions: usize,
    pub combat_actions: usize,
    pub final_fingerprint: String,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub engine_state: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WitnessPolicyDecisionAuditV1 {
    pub journal_entry: usize,
    pub decision_ordinal: usize,
    pub act: u8,
    pub floor: i32,
    pub boundary: String,
    pub location: String,
    pub chosen_candidate_id: String,
    pub chosen_label: String,
    pub recorded_chosen_key: Option<DecisionCandidateKey>,
    pub current_chosen_key: Option<DecisionCandidateKey>,
    pub owner_rank: Option<usize>,
    pub owner_candidate_count: usize,
    pub owner_first_candidate_id: Option<String>,
    pub owner_first_label: Option<String>,
    pub current_owner_first_key: Option<DecisionCandidateKey>,
    pub owner_first_relation: RunWitnessOwnerFirstRelationV1,
    pub resources: RunWitnessResourceSnapshotV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunWitnessOwnerFirstRelationV1 {
    ExactCandidate,
    SamePotionKindDiscard,
    DifferentCandidate,
    TypedIdentityUnavailable,
    NoOwnerCandidate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct RunWitnessPotionSnapshotV1 {
    pub id: PotionId,
    pub uuid: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunWitnessResourceSnapshotV1 {
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub deck_size: usize,
    pub potions: Vec<Option<RunWitnessPotionSnapshotV1>>,
}

pub const RUN_WITNESS_JOURNAL_FINGERPRINT_ALGORITHM_V1: &str =
    "blake2b_256_canonical_run_progress_journal_v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunWitnessLineIdentityV1 {
    pub seed: u64,
    pub ascension: u8,
    pub journal_entries: usize,
    pub journal_fingerprint_algorithm: String,
    pub journal_fingerprint: String,
    pub final_session_fingerprint: String,
    pub final_resources: RunWitnessResourceSnapshotV1,
    pub final_engine_state: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunWitnessCombatRootOriginV1 {
    JournalEntryBefore,
    FinalActiveCombat,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunWitnessCombatRootIdentityV1 {
    pub origin: RunWitnessCombatRootOriginV1,
    pub journal_entry: Option<usize>,
    pub journal_prefix_entries: usize,
    pub journal_prefix_fingerprint: String,
    pub run_session_fingerprint: String,
    pub root_exact_state_hash: String,
    pub boundary: Option<String>,
    pub resources: RunWitnessResourceSnapshotV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExactRunWitnessIdentityReportV1 {
    pub replay: ExactRunProgressReplayReportV1,
    pub line_identity: RunWitnessLineIdentityV1,
    pub combat_roots: Vec<RunWitnessCombatRootIdentityV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExactRunWitnessCombatRootCensusV1 {
    pub replay: Option<ExactRunProgressReplayReportV1>,
    pub line_identity: Option<RunWitnessLineIdentityV1>,
    pub replay_error: Option<String>,
    pub combat_roots: Vec<RunWitnessCombatRootIdentityV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunWitnessStrategicDecisionV1 {
    pub journal_entry: usize,
    pub act: u8,
    pub floor: i32,
    pub boundary: String,
    pub chosen_label: String,
    pub key: DecisionCandidateKey,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunWitnessCombatTimelineEntryV1 {
    pub journal_entry: usize,
    pub act: u8,
    pub floor: i32,
    pub encounter: String,
    pub resolution_kind: String,
    pub source: String,
    pub root_identity: RunWitnessCombatRootIdentityV1,
    pub action_count: usize,
    pub hp_before: i32,
    pub minimum_combat_hp: i32,
    pub hp_after: i32,
    pub peak_hp_loss: i32,
    pub net_hp_change: i32,
    pub potions_before: Vec<Option<RunWitnessPotionSnapshotV1>>,
    pub potions_after: Vec<Option<RunWitnessPotionSnapshotV1>>,
    pub preceding_strategic_decisions: Vec<RunWitnessStrategicDecisionV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunWitnessRecoveryPivotV1 {
    pub journal_entry: usize,
    pub act: u8,
    pub floor: i32,
    pub boundary: String,
    pub chosen_label: Option<String>,
    pub hp_before: i32,
    pub hp_after: i32,
    pub hp_recovered: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunWitnessFullHpResetV1 {
    pub journal_entry: usize,
    pub boundary: String,
    pub chosen_label: Option<String>,
    pub before: RunWitnessResourceSnapshotV1,
    pub after: RunWitnessResourceSnapshotV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunWitnessCurrentHpEpochV1 {
    pub last_full_hp_reset: Option<RunWitnessFullHpResetV1>,
    pub start: RunWitnessResourceSnapshotV1,
    pub current: RunWitnessResourceSnapshotV1,
    pub net_hp_change: i32,
    pub combat_timeline: Vec<RunWitnessCombatTimelineEntryV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExactRunWitnessDiagnosisReportV1 {
    pub replay: ExactRunProgressReplayReportV1,
    pub line_identity: RunWitnessLineIdentityV1,
    pub policy: ExactRunWitnessPolicyAuditReportV1,
    pub combat_timeline: Vec<RunWitnessCombatTimelineEntryV1>,
    pub current_combat_root: Option<RunWitnessCombatRootIdentityV1>,
    pub highest_peak_hp_loss_combats: Vec<RunWitnessCombatTimelineEntryV1>,
    pub lowest_post_combat_hp_combats: Vec<RunWitnessCombatTimelineEntryV1>,
    pub recovery_pivots: Vec<RunWitnessRecoveryPivotV1>,
    pub current_hp_epoch: RunWitnessCurrentHpEpochV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExactRunWitnessPolicyAuditReportV1 {
    pub replay: ExactRunProgressReplayReportV1,
    pub decisions_with_owner_preferences: usize,
    pub decisions_without_owner_preferences: usize,
    pub rank_zero_agreements: usize,
    pub nonzero_rank_choices: usize,
    pub choices_absent_from_owner_preferences: usize,
    pub discrepancy_sum: usize,
    pub max_owner_rank: Option<usize>,
    pub same_potion_kind_discard_choices: usize,
    pub first_divergence: Option<WitnessPolicyDecisionAuditV1>,
    pub first_unclassified_divergence: Option<WitnessPolicyDecisionAuditV1>,
    pub divergences: Vec<WitnessPolicyDecisionAuditV1>,
    pub combat_sources: BTreeMap<String, usize>,
}

/// Re-executes a committed run journal from the canonical initial state and
/// verifies every recorded decision/combat boundary plus the final normalized
/// session fingerprint. This is deliberately independent of owner policy and
/// search: a saved witness is accepted only when its exact recorded actions
/// still produce the saved terminal state.
pub fn exact_replay_run_progress_journal_v1(
    seed: u64,
    ascension: u8,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
) -> Result<ExactRunProgressReplayReportV1, String> {
    exact_replay_run_progress_journal_observed_v1(
        seed,
        ascension,
        journal,
        expected_final,
        |_, _, _| Ok(()),
        |_, _, _| Ok(()),
        |_, _, _| Ok(()),
    )
}

pub fn run_progress_journal_fingerprint_v1(journal: &RunProgressJournalV1) -> String {
    crate::eval::fingerprint::hash_serializable(journal)
}

pub fn run_progress_journal_prefix_fingerprint_v1(
    journal: &RunProgressJournalV1,
    prefix_entries: usize,
) -> Result<String, String> {
    if prefix_entries > journal.len() {
        return Err(format!(
            "journal prefix length {prefix_entries} exceeds journal length {}",
            journal.len()
        ));
    }
    let prefix =
        RunProgressJournalV1::from_committed_steps(journal.entries()[..prefix_entries].to_vec())?;
    Ok(run_progress_journal_fingerprint_v1(&prefix))
}

pub fn exact_replay_run_progress_journal_identity_v1(
    seed: u64,
    ascension: u8,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
) -> Result<ExactRunWitnessIdentityReportV1, String> {
    let census =
        exact_census_run_progress_journal_combat_roots_v1(seed, ascension, journal, expected_final);
    let replay = census.replay.ok_or_else(|| {
        census.replay_error.unwrap_or_else(|| {
            "run witness combat-root census produced no replay result".to_string()
        })
    })?;
    let line_identity = census.line_identity.ok_or_else(|| {
        "run witness combat-root census produced no line identity after exact replay".to_string()
    })?;
    Ok(ExactRunWitnessIdentityReportV1 {
        replay,
        line_identity,
        combat_roots: census.combat_roots,
    })
}

pub fn exact_census_run_progress_journal_combat_roots_v1(
    seed: u64,
    ascension: u8,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
) -> ExactRunWitnessCombatRootCensusV1 {
    let combat_roots = RefCell::new(Vec::new());
    let replay = exact_replay_run_progress_journal_observed_v1(
        seed,
        ascension,
        journal,
        expected_final,
        |entry_index, entry, session| {
            if let RunProgressStepV1::CombatResolution(record) = entry {
                combat_roots
                    .borrow_mut()
                    .push(run_witness_combat_root_identity_v1(
                        journal,
                        entry_index,
                        record,
                        session,
                    )?);
            }
            Ok(())
        },
        |_, _, _| Ok(()),
        |_, _, _| Ok(()),
    );
    let mut combat_roots = combat_roots.into_inner();
    let (replay, line_identity, replay_error) = match replay {
        Ok(replay) => {
            match run_witness_final_active_combat_root_identity_v1(journal, expected_final) {
                Ok(current) => {
                    combat_roots.extend(current);
                    let line_identity =
                        run_witness_line_identity_v1(&replay, journal, expected_final);
                    (Some(replay), Some(line_identity), None)
                }
                Err(error) => (None, None, Some(error)),
            }
        }
        Err(error) => (None, None, Some(error)),
    };
    ExactRunWitnessCombatRootCensusV1 {
        replay,
        line_identity,
        replay_error,
        combat_roots,
    }
}

/// Replays a committed journal up to, but not including, one entry and
/// returns the exact session at that historical boundary.
///
/// This is the supported extraction path for historical combat cases. It
/// validates every preceding decision and combat instead of asking tooling to
/// edit continuation JSON or reconstruct persistent journal nodes.
pub fn exact_replay_run_progress_journal_prefix_v1(
    seed: u64,
    ascension: u8,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
    stop_before_entry: usize,
) -> Result<RunControlSession, String> {
    if stop_before_entry > journal.len() {
        return Err(format!(
            "journal prefix entry {stop_before_entry} exceeds journal length {}",
            journal.len()
        ));
    }
    let mut session = canonical_replay_session(seed, ascension, expected_final);
    let mut counters = ExactReplayCountersV1::default();
    let mut before_decision =
        |_: usize, _: &RunControlSession, _: &RunDecisionTransactionV1| Ok(());
    for (entry_index, entry) in journal.entries().iter().take(stop_before_entry).enumerate() {
        apply_exact_progress_entry_v1(
            entry_index,
            entry,
            &mut session,
            &mut counters,
            &mut before_decision,
        )?;
    }
    Ok(session)
}

/// Replays an exact witness and compares every committed non-combat choice
/// against the ordering produced by the current owner implementation.
///
/// The owner remains read-only: its ordering neither mutates the replay nor
/// decides whether the historical witness is valid. An absent rank means that
/// the current owner did not include the committed action in its preference
/// list; it is reported separately instead of being converted into an
/// arbitrary discrepancy score.
pub fn exact_audit_run_progress_journal_policy_v1<F>(
    seed: u64,
    ascension: u8,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
    decision_order: F,
) -> Result<ExactRunWitnessPolicyAuditReportV1, String>
where
    F: FnMut(&RunControlSession) -> Vec<String>,
{
    exact_audit_run_progress_journal_policy_observed_v1(
        seed,
        ascension,
        journal,
        expected_final,
        decision_order,
        |_, _, _| Ok(()),
        |_, _, _| Ok(()),
    )
}

/// Replays one exact run witness once and returns the compact typed pivots
/// needed to choose a bounded counterfactual. This is diagnostic only: it
/// neither changes owner policy nor assigns causal credit to a divergence.
pub fn exact_diagnose_run_progress_journal_v1<F>(
    seed: u64,
    ascension: u8,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
    decision_order: F,
    max_pivots: usize,
) -> Result<ExactRunWitnessDiagnosisReportV1, String>
where
    F: FnMut(&RunControlSession) -> Vec<String>,
{
    let initial = canonical_replay_session(seed, ascension, expected_final);
    let mut before_resources = run_witness_resource_snapshot_v1(&initial);
    let mut hp_epoch_start = before_resources.clone();
    let mut last_full_hp_reset = None;
    let mut hp_epoch_combats = Vec::new();
    let mut strategic_decisions_since_combat = Vec::new();
    let mut combat_timeline = Vec::new();
    let mut recovery_pivots = Vec::new();
    let combat_root_identities = RefCell::new(BTreeMap::new());

    let policy = exact_audit_run_progress_journal_policy_observed_v1(
        seed,
        ascension,
        journal,
        expected_final,
        decision_order,
        |entry_index, entry, session| {
            if let RunProgressStepV1::CombatResolution(record) = entry {
                combat_root_identities.borrow_mut().insert(
                    entry_index,
                    run_witness_combat_root_identity_v1(journal, entry_index, record, session)?,
                );
            }
            Ok(())
        },
        |entry_index, entry, session| {
            let after_resources = run_witness_resource_snapshot_v1(session);
            let full_hp_reset = establishes_full_hp_reset_v1(&before_resources, &after_resources);
            if after_resources.current_hp > before_resources.current_hp {
                let (boundary, chosen_label) = progress_entry_label_v1(entry);
                recovery_pivots.push(RunWitnessRecoveryPivotV1 {
                    journal_entry: entry_index,
                    act: before_resources.act,
                    floor: before_resources.floor,
                    boundary,
                    chosen_label,
                    hp_before: before_resources.current_hp,
                    hp_after: after_resources.current_hp,
                    hp_recovered: after_resources.current_hp - before_resources.current_hp,
                });
            }

            match entry {
                RunProgressStepV1::Decision(record) => {
                    if let Some(decision) =
                        strategic_decision_v1(entry_index, &before_resources, record)
                    {
                        strategic_decisions_since_combat.push(decision);
                    }
                }
                RunProgressStepV1::CombatResolution(record) => {
                    let root_identity = combat_root_identities
                        .borrow()
                        .get(&entry_index)
                        .cloned()
                        .ok_or_else(|| {
                            format!(
                                "journal entry {entry_index} combat root identity was not captured"
                            )
                        })?;
                    let minimum_combat_hp = record
                        .trajectory
                        .actions
                        .iter()
                        .filter_map(|action| {
                            action.combat_after.as_ref().map(|state| state.player_hp)
                        })
                        .fold(
                            before_resources.current_hp.min(after_resources.current_hp),
                            i32::min,
                        );
                    let combat = RunWitnessCombatTimelineEntryV1 {
                        journal_entry: entry_index,
                        act: before_resources.act,
                        floor: before_resources.floor,
                        encounter: combat_encounter_label_v1(record),
                        resolution_kind: format!("{:?}", record.kind),
                        source: record.trajectory.source.label().to_string(),
                        root_identity,
                        action_count: record.trajectory.action_count,
                        hp_before: before_resources.current_hp,
                        minimum_combat_hp,
                        hp_after: after_resources.current_hp,
                        peak_hp_loss: (before_resources.current_hp - minimum_combat_hp).max(0),
                        net_hp_change: after_resources.current_hp - before_resources.current_hp,
                        potions_before: before_resources.potions.clone(),
                        potions_after: after_resources.potions.clone(),
                        preceding_strategic_decisions: std::mem::take(
                            &mut strategic_decisions_since_combat,
                        ),
                    };
                    combat_timeline.push(combat.clone());
                    if !full_hp_reset {
                        hp_epoch_combats.push(combat);
                    }
                }
                RunProgressStepV1::ForcedTransition(_) | RunProgressStepV1::Stop(_) => {}
            }
            if full_hp_reset {
                let (boundary, chosen_label) = progress_entry_label_v1(entry);
                last_full_hp_reset = Some(RunWitnessFullHpResetV1 {
                    journal_entry: entry_index,
                    boundary,
                    chosen_label,
                    before: before_resources.clone(),
                    after: after_resources.clone(),
                });
                hp_epoch_start = after_resources.clone();
                hp_epoch_combats.clear();
            }
            before_resources = after_resources;
            Ok(())
        },
    )?;

    let pivot_limit = max_pivots.max(1);
    let mut highest_peak_hp_loss_combats = combat_timeline.clone();
    highest_peak_hp_loss_combats.sort_by(|left, right| {
        right
            .peak_hp_loss
            .cmp(&left.peak_hp_loss)
            .then_with(|| left.journal_entry.cmp(&right.journal_entry))
    });
    highest_peak_hp_loss_combats.truncate(pivot_limit);

    let mut lowest_post_combat_hp_combats = combat_timeline.clone();
    lowest_post_combat_hp_combats.sort_by(|left, right| {
        left.hp_after
            .cmp(&right.hp_after)
            .then_with(|| left.journal_entry.cmp(&right.journal_entry))
    });
    lowest_post_combat_hp_combats.truncate(pivot_limit);

    recovery_pivots.sort_by(|left, right| {
        right
            .hp_recovered
            .cmp(&left.hp_recovered)
            .then_with(|| left.journal_entry.cmp(&right.journal_entry))
    });
    recovery_pivots.truncate(pivot_limit);

    let current_hp_epoch = RunWitnessCurrentHpEpochV1 {
        last_full_hp_reset,
        net_hp_change: before_resources.current_hp - hp_epoch_start.current_hp,
        start: hp_epoch_start,
        current: before_resources,
        combat_timeline: hp_epoch_combats,
    };
    let line_identity = run_witness_line_identity_v1(&policy.replay, journal, expected_final);
    let current_combat_root =
        run_witness_final_active_combat_root_identity_v1(journal, expected_final)?;

    Ok(ExactRunWitnessDiagnosisReportV1 {
        replay: policy.replay.clone(),
        line_identity,
        policy,
        combat_timeline,
        current_combat_root,
        highest_peak_hp_loss_combats,
        lowest_post_combat_hp_combats,
        recovery_pivots,
        current_hp_epoch,
    })
}

fn exact_audit_run_progress_journal_policy_observed_v1<B, F, G>(
    seed: u64,
    ascension: u8,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
    mut decision_order: F,
    before_entry: B,
    after_entry: G,
) -> Result<ExactRunWitnessPolicyAuditReportV1, String>
where
    B: FnMut(usize, &RunProgressStepV1, &RunControlSession) -> Result<(), String>,
    F: FnMut(&RunControlSession) -> Vec<String>,
    G: FnMut(usize, &RunProgressStepV1, &RunControlSession) -> Result<(), String>,
{
    let mut decision_audits = Vec::new();
    let replay = exact_replay_run_progress_journal_observed_v1(
        seed,
        ascension,
        journal,
        expected_final,
        before_entry,
        |entry_index, session, record| {
            let owner_order = decision_order(session);
            let selected_id = record.selection.candidate_id.clone();
            let owner_rank = owner_order
                .iter()
                .position(|candidate_id| candidate_id == &selected_id);
            let selected_candidate = record
                .before
                .candidates
                .iter()
                .find(|candidate| candidate.candidate_id == selected_id);
            let chosen_label = selected_candidate
                .map(|candidate| candidate.label.clone())
                .unwrap_or_else(|| selected_id.clone());
            let recorded_chosen_key =
                selected_candidate.and_then(|candidate| candidate.key.clone());
            let owner_first_candidate_id = owner_order.first().cloned();
            let recorded_owner_first_candidate =
                owner_first_candidate_id.as_ref().and_then(|candidate_id| {
                    record
                        .before
                        .candidates
                        .iter()
                        .find(|candidate| &candidate.candidate_id == candidate_id)
                });
            let owner_first_label =
                recorded_owner_first_candidate.map(|candidate| candidate.label.clone());
            let current_surface = super::build_decision_surface(session);
            let current_chosen_key = current_surface
                .view
                .candidates
                .iter()
                .find(|candidate| candidate.id == selected_id)
                .and_then(|candidate| candidate.key.clone());
            let current_owner_first_key =
                owner_first_candidate_id.as_ref().and_then(|candidate_id| {
                    current_surface
                        .view
                        .candidates
                        .iter()
                        .find(|candidate| &candidate.id == candidate_id)
                        .and_then(|candidate| candidate.key.clone())
                });
            let owner_first_relation = classify_owner_first_relation_v1(
                &selected_id,
                current_chosen_key.as_ref(),
                owner_first_candidate_id.as_deref(),
                current_owner_first_key.as_ref(),
            );
            decision_audits.push(WitnessPolicyDecisionAuditV1 {
                journal_entry: entry_index,
                decision_ordinal: decision_audits.len(),
                act: session.run_state.act_num,
                floor: session.run_state.floor_num,
                boundary: record.before.title.clone(),
                location: record.before.location.clone(),
                chosen_candidate_id: selected_id,
                chosen_label,
                recorded_chosen_key,
                current_chosen_key,
                owner_rank,
                owner_candidate_count: owner_order.len(),
                owner_first_candidate_id,
                owner_first_label,
                current_owner_first_key,
                owner_first_relation,
                resources: run_witness_resource_snapshot_v1(session),
            });
            Ok(())
        },
        after_entry,
    )?;

    let decisions_with_owner_preferences = decision_audits
        .iter()
        .filter(|audit| audit.owner_candidate_count > 0)
        .count();
    let decisions_without_owner_preferences = decision_audits
        .len()
        .saturating_sub(decisions_with_owner_preferences);
    let rank_zero_agreements = decision_audits
        .iter()
        .filter(|audit| audit.owner_rank == Some(0))
        .count();
    let nonzero_rank_choices = decision_audits
        .iter()
        .filter(|audit| audit.owner_rank.is_some_and(|rank| rank > 0))
        .count();
    let choices_absent_from_owner_preferences = decision_audits
        .iter()
        .filter(|audit| audit.owner_candidate_count > 0 && audit.owner_rank.is_none())
        .count();
    let discrepancy_sum = decision_audits
        .iter()
        .filter_map(|audit| audit.owner_rank)
        .sum();
    let max_owner_rank = decision_audits
        .iter()
        .filter_map(|audit| audit.owner_rank)
        .max();
    let same_potion_kind_discard_choices = decision_audits
        .iter()
        .filter(|audit| {
            audit.owner_first_relation == RunWitnessOwnerFirstRelationV1::SamePotionKindDiscard
        })
        .count();
    let divergences = decision_audits
        .into_iter()
        .filter(|audit| {
            audit.owner_candidate_count > 0
                && (audit.owner_rank.is_none() || audit.owner_rank.is_some_and(|rank| rank > 0))
        })
        .collect::<Vec<_>>();
    let first_divergence = divergences.first().cloned();
    let first_unclassified_divergence = divergences
        .iter()
        .find(|audit| {
            audit.owner_first_relation != RunWitnessOwnerFirstRelationV1::SamePotionKindDiscard
        })
        .cloned();
    let mut combat_sources = BTreeMap::new();
    for entry in journal.entries() {
        if let RunProgressStepV1::CombatResolution(record) = entry {
            *combat_sources
                .entry(record.trajectory.source.label().to_string())
                .or_insert(0) += 1;
        }
    }

    Ok(ExactRunWitnessPolicyAuditReportV1 {
        replay,
        decisions_with_owner_preferences,
        decisions_without_owner_preferences,
        rank_zero_agreements,
        nonzero_rank_choices,
        choices_absent_from_owner_preferences,
        discrepancy_sum,
        max_owner_rank,
        same_potion_kind_discard_choices,
        first_divergence,
        first_unclassified_divergence,
        divergences,
        combat_sources,
    })
}

fn classify_owner_first_relation_v1(
    chosen_candidate_id: &str,
    chosen_key: Option<&DecisionCandidateKey>,
    owner_first_candidate_id: Option<&str>,
    owner_first_key: Option<&DecisionCandidateKey>,
) -> RunWitnessOwnerFirstRelationV1 {
    let Some(owner_first_candidate_id) = owner_first_candidate_id else {
        return RunWitnessOwnerFirstRelationV1::NoOwnerCandidate;
    };
    if chosen_candidate_id == owner_first_candidate_id {
        return RunWitnessOwnerFirstRelationV1::ExactCandidate;
    }
    if matches!(
        (chosen_key, owner_first_key),
        (
            Some(DecisionCandidateKey::RunPotionDiscard {
                potion: chosen_potion,
                ..
            }),
            Some(DecisionCandidateKey::RunPotionDiscard {
                potion: owner_potion,
                ..
            })
        ) if chosen_potion == owner_potion
    ) {
        return RunWitnessOwnerFirstRelationV1::SamePotionKindDiscard;
    }
    if chosen_key.is_none() || owner_first_key.is_none() {
        return RunWitnessOwnerFirstRelationV1::TypedIdentityUnavailable;
    }
    RunWitnessOwnerFirstRelationV1::DifferentCandidate
}

fn run_witness_resource_snapshot_v1(session: &RunControlSession) -> RunWitnessResourceSnapshotV1 {
    RunWitnessResourceSnapshotV1 {
        act: session.run_state.act_num,
        floor: session.run_state.floor_num,
        current_hp: session.run_state.current_hp,
        max_hp: session.run_state.max_hp,
        gold: session.run_state.gold,
        deck_size: session.run_state.master_deck.len(),
        potions: session
            .run_state
            .potions
            .iter()
            .map(|slot| {
                slot.as_ref().map(|potion| RunWitnessPotionSnapshotV1 {
                    id: potion.id,
                    uuid: potion.uuid,
                })
            })
            .collect(),
    }
}

fn run_witness_line_identity_v1(
    replay: &ExactRunProgressReplayReportV1,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
) -> RunWitnessLineIdentityV1 {
    RunWitnessLineIdentityV1 {
        seed: replay.seed,
        ascension: replay.ascension,
        journal_entries: replay.journal_entries,
        journal_fingerprint_algorithm: RUN_WITNESS_JOURNAL_FINGERPRINT_ALGORITHM_V1.to_string(),
        journal_fingerprint: run_progress_journal_fingerprint_v1(journal),
        final_session_fingerprint: replay.final_fingerprint.clone(),
        final_resources: run_witness_resource_snapshot_v1(expected_final),
        final_engine_state: replay.engine_state.clone(),
    }
}

fn run_witness_combat_root_identity_v1(
    journal: &RunProgressJournalV1,
    journal_entry: usize,
    record: &RunCombatResolutionV1,
    session: &RunControlSession,
) -> Result<RunWitnessCombatRootIdentityV1, String> {
    if !record.before.active_combat {
        return Err(format!(
            "journal entry {journal_entry} combat record has an inactive before-boundary"
        ));
    }
    run_witness_active_combat_root_identity_v1(
        journal,
        journal_entry,
        RunWitnessCombatRootOriginV1::JournalEntryBefore,
        Some(journal_entry),
        Some(record.before.location.clone()),
        session,
    )
}

fn run_witness_final_active_combat_root_identity_v1(
    journal: &RunProgressJournalV1,
    session: &RunControlSession,
) -> Result<Option<RunWitnessCombatRootIdentityV1>, String> {
    if session.active_combat.is_none() {
        return Ok(None);
    }
    run_witness_active_combat_root_identity_v1(
        journal,
        journal.len(),
        RunWitnessCombatRootOriginV1::FinalActiveCombat,
        None,
        None,
        session,
    )
    .map(Some)
}

fn run_witness_active_combat_root_identity_v1(
    journal: &RunProgressJournalV1,
    journal_prefix_entries: usize,
    origin: RunWitnessCombatRootOriginV1,
    journal_entry: Option<usize>,
    boundary: Option<String>,
    session: &RunControlSession,
) -> Result<RunWitnessCombatRootIdentityV1, String> {
    let active = session.active_combat.as_ref().ok_or_else(|| {
        format!("combat root at journal prefix {journal_prefix_entries} has no active combat")
    })?;
    Ok(RunWitnessCombatRootIdentityV1 {
        origin,
        journal_entry,
        journal_prefix_entries,
        journal_prefix_fingerprint: run_progress_journal_prefix_fingerprint_v1(
            journal,
            journal_prefix_entries,
        )?,
        run_session_fingerprint: run_session_fingerprint_v3(session),
        root_exact_state_hash: combat_exact_state_hash_v2(
            &active.engine_state,
            &active.combat_state,
        ),
        boundary,
        resources: run_witness_resource_snapshot_v1(session),
    })
}

fn establishes_full_hp_reset_v1(
    before: &RunWitnessResourceSnapshotV1,
    after: &RunWitnessResourceSnapshotV1,
) -> bool {
    before.current_hp < before.max_hp
        && after.current_hp == after.max_hp
        && after.current_hp > before.current_hp
}

fn strategic_decision_v1(
    journal_entry: usize,
    resources: &RunWitnessResourceSnapshotV1,
    record: &RunDecisionTransactionV1,
) -> Option<RunWitnessStrategicDecisionV1> {
    let selected = record
        .before
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == record.selection.candidate_id)?;
    let key = selected.key.clone()?;
    if !is_strategic_decision_key_v1(&key) {
        return None;
    }
    Some(RunWitnessStrategicDecisionV1 {
        journal_entry,
        act: resources.act,
        floor: resources.floor,
        boundary: record.before.title.clone(),
        chosen_label: selected.label.clone(),
        key,
    })
}

fn is_strategic_decision_key_v1(key: &DecisionCandidateKey) -> bool {
    matches!(
        key,
        DecisionCandidateKey::RouteSelect { .. }
            | DecisionCandidateKey::RunPotionUse { .. }
            | DecisionCandidateKey::RunPotionDiscard { .. }
            | DecisionCandidateKey::EventOption { .. }
            | DecisionCandidateKey::CardRewardPick { .. }
            | DecisionCandidateKey::CardRewardSingingBowl { .. }
            | DecisionCandidateKey::CardRewardSkip { .. }
            | DecisionCandidateKey::BossRelicPick { .. }
            | DecisionCandidateKey::BossRelicSkip
            | DecisionCandidateKey::CampfireRest
            | DecisionCandidateKey::CampfireSmith { .. }
            | DecisionCandidateKey::CampfireDig
            | DecisionCandidateKey::CampfireLift
            | DecisionCandidateKey::CampfireToke { .. }
            | DecisionCandidateKey::CampfireRecall
            | DecisionCandidateKey::ShopPurgeCard { .. }
            | DecisionCandidateKey::ShopBuyCard { .. }
            | DecisionCandidateKey::ShopBuyRelic { .. }
            | DecisionCandidateKey::ShopBuyPotion { .. }
    )
}

fn progress_entry_label_v1(entry: &RunProgressStepV1) -> (String, Option<String>) {
    match entry {
        RunProgressStepV1::Decision(record) => {
            let chosen_label = record
                .before
                .candidates
                .iter()
                .find(|candidate| candidate.candidate_id == record.selection.candidate_id)
                .map(|candidate| candidate.label.clone());
            (record.before.title.clone(), chosen_label)
        }
        RunProgressStepV1::ForcedTransition(record) => (
            record.before.title.clone(),
            Some(format!("{:?}", record.kind)),
        ),
        RunProgressStepV1::CombatResolution(record) => (
            record.before.title.clone(),
            Some(record.trajectory.source.label().to_string()),
        ),
        RunProgressStepV1::Stop(record) => ("Stop".to_string(), Some(record.reason.clone())),
    }
}

fn combat_encounter_label_v1(record: &RunCombatResolutionV1) -> String {
    let labels = record
        .trajectory
        .actions
        .iter()
        .find_map(|action| action.combat_after.as_ref())
        .map(|state| {
            state
                .monsters
                .iter()
                .map(|monster| monster.label.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if labels.is_empty() {
        record.before.title.clone()
    } else {
        labels.join(" + ")
    }
}

/// Replaces one committed combat trajectory while preserving the surrounding
/// strategic history. The replacement is accepted only when its exact combat
/// boundaries match and the entire resulting journal still replays to the
/// original final session fingerprint.
pub fn splice_exact_combat_resolution_v1(
    seed: u64,
    ascension: u8,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
    journal_entry: usize,
    replacement: &RunCombatResolutionV1,
) -> Result<(RunProgressJournalV1, ExactRunProgressReplayReportV1), String> {
    let original = journal
        .entries()
        .get(journal_entry)
        .and_then(RunProgressStepV1::as_combat_resolution)
        .ok_or_else(|| {
            format!("journal entry {journal_entry} is not a committed combat resolution")
        })?;
    if !combat_boundaries_match(&replacement.before, &original.before) {
        return Err(format!(
            "replacement combat before-boundary does not match journal entry {journal_entry}"
        ));
    }
    if !combat_boundaries_match(&replacement.after, &original.after) {
        return Err(format!(
            "replacement combat after-boundary does not match journal entry {journal_entry}"
        ));
    }

    let mut entries = journal.entries().to_vec();
    entries[journal_entry] = RunProgressStepV1::CombatResolution(replacement.clone());
    let journal = RunProgressJournalV1::from_committed_steps(entries)?;
    let replay = exact_replay_run_progress_journal_v1(seed, ascension, &journal, expected_final)?;
    Ok((journal, replay))
}

fn exact_replay_run_progress_journal_observed_v1<B, F, G>(
    seed: u64,
    ascension: u8,
    journal: &RunProgressJournalV1,
    expected_final: &RunControlSession,
    mut before_entry: B,
    mut before_decision: F,
    mut after_entry: G,
) -> Result<ExactRunProgressReplayReportV1, String>
where
    B: FnMut(usize, &RunProgressStepV1, &RunControlSession) -> Result<(), String>,
    F: FnMut(usize, &RunControlSession, &RunDecisionTransactionV1) -> Result<(), String>,
    G: FnMut(usize, &RunProgressStepV1, &RunControlSession) -> Result<(), String>,
{
    let mut session = canonical_replay_session(seed, ascension, expected_final);
    let mut counters = ExactReplayCountersV1::default();

    for (entry_index, entry) in journal.entries().iter().enumerate() {
        before_entry(entry_index, entry, &session)?;
        apply_exact_progress_entry_v1(
            entry_index,
            entry,
            &mut session,
            &mut counters,
            &mut before_decision,
        )?;
        session.preserve_recent_combat_attrition_availability_from(expected_final);
        after_entry(entry_index, entry, &session)?;
    }

    let final_fingerprint = run_session_fingerprint_v3(&session);
    let expected_fingerprint = run_session_fingerprint_v3(expected_final);
    if final_fingerprint != expected_fingerprint {
        return Err(format!(
            "journal replay final fingerprint mismatch: expected {expected_fingerprint}, got {final_fingerprint}; recent_combat_attrition expected {:?}, got {:?}",
            expected_final.recent_combat_attrition(),
            session.recent_combat_attrition(),
        ));
    }

    Ok(ExactRunProgressReplayReportV1 {
        seed,
        ascension,
        journal_entries: journal.len(),
        decisions: counters.decisions,
        forced_transitions: counters.forced_transitions,
        combat_resolutions: counters.combat_resolutions,
        combat_actions: counters.combat_actions,
        final_fingerprint,
        act: session.run_state.act_num,
        floor: session.run_state.floor_num,
        current_hp: session.run_state.current_hp,
        max_hp: session.run_state.max_hp,
        engine_state: session.engine_state.kind_v1().to_string(),
    })
}

#[derive(Default)]
struct ExactReplayCountersV1 {
    decisions: usize,
    forced_transitions: usize,
    combat_resolutions: usize,
    combat_actions: usize,
}

fn canonical_replay_session(
    seed: u64,
    ascension: u8,
    expected_final: &RunControlSession,
) -> RunControlSession {
    RunControlSession::new(RunControlConfig {
        seed,
        ascension_level: ascension,
        final_act: false,
        player_class: expected_final.run_state.player_class,
        reward_automation: expected_final.reward_automation.clone(),
        ..RunControlConfig::default()
    })
}

fn apply_exact_progress_entry_v1<F>(
    entry_index: usize,
    entry: &RunProgressStepV1,
    session: &mut RunControlSession,
    counters: &mut ExactReplayCountersV1,
    before_decision: &mut F,
) -> Result<(), String>
where
    F: FnMut(usize, &RunControlSession, &RunDecisionTransactionV1) -> Result<(), String>,
{
    match entry {
        RunProgressStepV1::Decision(record) => {
            let actual_before = RunDecisionBoundaryV1::capture(session);
            if !decision_boundaries_match(&actual_before, &record.before) {
                return Err(format!(
                    "journal entry {entry_index} decision before-boundary mismatch: expected {:?}, got {:?}",
                    record.before,
                    actual_before,
                ));
            }
            before_decision(entry_index, session, record)?;
            session
                .apply_decision_action(record.action.clone())
                .map_err(|error| {
                    format!("journal entry {entry_index} decision replay failed: {error}")
                })?;
            let actual_after = RunDecisionBoundaryV1::capture(session);
            if !decision_boundaries_match(&actual_after, &record.after) {
                return Err(format!(
                    "journal entry {entry_index} decision after-boundary mismatch: expected {:?}, got {:?}",
                    record.after,
                    actual_after,
                ));
            }
            counters.decisions = counters.decisions.saturating_add(1);
        }
        RunProgressStepV1::ForcedTransition(record) => {
            let actual_before = RunDecisionBoundaryV1::capture(session);
            if !decision_boundaries_match(&actual_before, &record.before) {
                return Err(format!(
                    "journal entry {entry_index} forced-transition before-boundary mismatch"
                ));
            }
            session
                .apply_forced_transition(record.kind)
                .map_err(|error| {
                    format!("journal entry {entry_index} forced-transition replay failed: {error}")
                })?;
            let actual_after = RunDecisionBoundaryV1::capture(session);
            if !decision_boundaries_match(&actual_after, &record.after) {
                return Err(format!(
                    "journal entry {entry_index} forced-transition after-boundary mismatch"
                ));
            }
            counters.forced_transitions = counters.forced_transitions.saturating_add(1);
        }
        RunProgressStepV1::CombatResolution(record) => {
            let actual_before = RunCombatResolutionBoundaryV1::capture(session);
            if !combat_boundaries_match(&actual_before, &record.before) {
                return Err(format!(
                    "journal entry {entry_index} combat before-boundary mismatch: expected '{} @ {}', got '{} @ {}'",
                    record.before.title,
                    record.before.location,
                    actual_before.title,
                    actual_before.location,
                ));
            }
            session.mark_current_combat_search_resolved();
            for (action_index, action) in record.trajectory.actions.iter().enumerate() {
                let opportunity = combat_automation_opportunity_state_v1(session);
                if opportunity != action.opportunity_before {
                    return Err(format!(
                        "journal entry {entry_index} combat action {action_index} opportunity mismatch: {}",
                        combat_opportunity_mismatch_summary(
                            action.opportunity_before.as_ref(),
                            opportunity.as_ref(),
                        )
                    ));
                }
                let outcome = session
                    .apply_combat_resolution_input(action.input.clone())
                    .map_err(|error| {
                        format!(
                            "journal entry {entry_index} combat action {action_index} replay failed: {error}"
                        )
                    })?;
                let drawn_cards = drawn_cards_from_action_result(outcome.action_result.as_ref());
                if drawn_cards != action.drawn_cards {
                    return Err(format!(
                        "journal entry {entry_index} combat action {action_index} drawn-card mismatch"
                    ));
                }
                let combat_after = combat_automation_step_state_v1(session);
                if combat_after != action.combat_after {
                    return Err(format!(
                        "journal entry {entry_index} combat action {action_index} successor mismatch"
                    ));
                }
                counters.combat_actions = counters.combat_actions.saturating_add(1);
            }
            let actual_after = RunCombatResolutionBoundaryV1::capture(session);
            if !combat_boundaries_match(&actual_after, &record.after) {
                return Err(format!(
                    "journal entry {entry_index} combat after-boundary mismatch: expected {:?}, got {:?}",
                    record.after,
                    actual_after,
                ));
            }
            counters.combat_resolutions = counters.combat_resolutions.saturating_add(1);
        }
        RunProgressStepV1::Stop(_) => {
            return Err(format!(
                "journal entry {entry_index} contains a non-committed Stop record"
            ));
        }
    }
    Ok(())
}

fn combat_boundaries_match(
    actual: &RunCombatResolutionBoundaryV1,
    expected: &RunCombatResolutionBoundaryV1,
) -> bool {
    // combat_sequence is diagnostic bookkeeping and is deliberately removed
    // by the canonical run-session fingerprint. Historical search/import
    // paths can observe the same exact combat with a different counter while
    // preserving every game-semantic state transition.
    actual.decision_step == expected.decision_step
        && actual.title == expected.title
        && actual.location == expected.location
        && actual.active_combat == expected.active_combat
}

fn decision_boundaries_match(
    actual: &RunDecisionBoundaryV1,
    expected: &RunDecisionBoundaryV1,
) -> bool {
    // The visible candidate surface may grow when a newer build exposes an
    // additional legal action (for example explicit potion discard). The
    // journal owns the action that was actually committed; replay validates
    // that action against the current exact state. Requiring every unrelated
    // visible candidate to remain byte-identical would reject valid witnesses
    // for a presentation/schema change rather than a game-state divergence.
    actual.decision_step == expected.decision_step
        && actual.title == expected.title
        && actual.location == expected.location
}

fn combat_opportunity_mismatch_summary(
    expected: Option<&CombatAutomationOpportunityStateV1>,
    actual: Option<&CombatAutomationOpportunityStateV1>,
) -> String {
    let mut differences = serde_json::Map::new();
    match (expected, actual) {
        (Some(expected), Some(actual)) => {
            macro_rules! record_difference {
                ($field:ident) => {
                    if expected.$field != actual.$field {
                        differences.insert(
                            stringify!($field).to_owned(),
                            serde_json::json!({
                                "expected": &expected.$field,
                                "actual": &actual.$field,
                            }),
                        );
                    }
                };
            }
            record_difference!(turn);
            record_difference!(energy);
            record_difference!(hand);
            record_difference!(potions);
            record_difference!(playable_card_lineages);
            record_difference!(usable_potion_uuids);
        }
        (expected, actual) => {
            differences.insert(
                "presence".to_owned(),
                serde_json::json!({
                    "expected": expected.is_some(),
                    "actual": actual.is_some(),
                }),
            );
        }
    }
    serde_json::to_string(&differences)
        .unwrap_or_else(|error| format!("failed to encode typed mismatch: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_opportunity_mismatch_reports_only_changed_typed_fields() {
        let expected = CombatAutomationOpportunityStateV1 {
            turn: 2,
            energy: 3,
            hand: Vec::new(),
            potions: Vec::new(),
            playable_card_lineages: Vec::new(),
            usable_potion_uuids: Vec::new(),
        };
        let actual = CombatAutomationOpportunityStateV1 {
            energy: 2,
            ..expected.clone()
        };

        let summary = combat_opportunity_mismatch_summary(Some(&expected), Some(&actual));
        let summary = serde_json::from_str::<serde_json::Value>(&summary)
            .expect("typed mismatch should be valid JSON");
        assert_eq!(summary["energy"]["expected"], 3);
        assert_eq!(summary["energy"]["actual"], 2);
        assert!(summary.get("turn").is_none());
        assert!(summary.get("hand").is_none());
    }

    #[test]
    fn owner_relation_classifies_same_kind_potion_discards_without_collapsing_other_potions() {
        let older_fear = DecisionCandidateKey::RunPotionDiscard {
            slot: 0,
            potion: PotionId::FearPotion,
            uuid: 10,
        };
        let newer_fear = DecisionCandidateKey::RunPotionDiscard {
            slot: 1,
            potion: PotionId::FearPotion,
            uuid: 20,
        };
        let fire = DecisionCandidateKey::RunPotionDiscard {
            slot: 2,
            potion: PotionId::FirePotion,
            uuid: 30,
        };

        assert_eq!(
            classify_owner_first_relation_v1(
                "discard-potion-0",
                Some(&older_fear),
                Some("discard-potion-1"),
                Some(&newer_fear),
            ),
            RunWitnessOwnerFirstRelationV1::SamePotionKindDiscard
        );
        assert_eq!(
            classify_owner_first_relation_v1(
                "discard-potion-0",
                Some(&older_fear),
                Some("discard-potion-2"),
                Some(&fire),
            ),
            RunWitnessOwnerFirstRelationV1::DifferentCandidate
        );
        assert_eq!(
            classify_owner_first_relation_v1("leave", None, Some("leave"), None),
            RunWitnessOwnerFirstRelationV1::ExactCandidate
        );
        assert_eq!(
            classify_owner_first_relation_v1(
                "discard-potion-0",
                None,
                Some("discard-potion-1"),
                Some(&newer_fear),
            ),
            RunWitnessOwnerFirstRelationV1::TypedIdentityUnavailable
        );
    }

    #[test]
    fn empty_witness_policy_audit_is_exact_and_search_free() {
        let seed = 20260713006;
        let config = RunControlConfig {
            seed,
            ascension_level: 0,
            ..RunControlConfig::default()
        };
        let expected_final = RunControlSession::new(config);
        let report = exact_audit_run_progress_journal_policy_v1(
            seed,
            0,
            &RunProgressJournalV1::default(),
            &expected_final,
            |_| panic!("an empty journal must not ask the owner for an ordering"),
        )
        .unwrap();

        assert_eq!(report.replay.journal_entries, 0);
        assert_eq!(report.replay.combat_resolutions, 0);
        assert_eq!(report.decisions_with_owner_preferences, 0);
        assert!(report.divergences.is_empty());
        assert!(report.combat_sources.is_empty());
    }

    #[test]
    fn empty_witness_identity_binds_the_journal_and_final_session() {
        let seed = 20260713006;
        let journal = RunProgressJournalV1::default();
        let expected_final = RunControlSession::new(RunControlConfig {
            seed,
            ascension_level: 0,
            ..RunControlConfig::default()
        });
        let report =
            exact_replay_run_progress_journal_identity_v1(seed, 0, &journal, &expected_final)
                .unwrap();

        assert_eq!(report.line_identity.seed, seed);
        assert_eq!(report.line_identity.ascension, 0);
        assert_eq!(report.line_identity.journal_entries, 0);
        assert_eq!(
            report.line_identity.journal_fingerprint,
            run_progress_journal_fingerprint_v1(&journal)
        );
        assert_eq!(
            report.line_identity.final_session_fingerprint,
            report.replay.final_fingerprint
        );
        assert!(report.combat_roots.is_empty());
    }

    #[test]
    fn empty_witness_diagnosis_is_compact_exact_and_search_free() {
        let seed = 20260713006;
        let expected_final = RunControlSession::new(RunControlConfig {
            seed,
            ascension_level: 0,
            ..RunControlConfig::default()
        });
        let report = exact_diagnose_run_progress_journal_v1(
            seed,
            0,
            &RunProgressJournalV1::default(),
            &expected_final,
            |_| panic!("an empty diagnosis must not ask the owner for an ordering"),
            5,
        )
        .unwrap();

        assert_eq!(report.replay.journal_entries, 0);
        assert_eq!(report.line_identity.journal_entries, 0);
        assert_eq!(
            report.line_identity.final_session_fingerprint,
            report.replay.final_fingerprint
        );
        assert!(report.policy.first_divergence.is_none());
        assert!(report.combat_timeline.is_empty());
        assert!(report.highest_peak_hp_loss_combats.is_empty());
        assert!(report.lowest_post_combat_hp_combats.is_empty());
        assert!(report.recovery_pivots.is_empty());
        assert!(report.current_hp_epoch.last_full_hp_reset.is_none());
        assert_eq!(report.current_hp_epoch.net_hp_change, 0);
        assert_eq!(
            report.current_hp_epoch.start,
            report.current_hp_epoch.current
        );
        assert!(report.current_hp_epoch.combat_timeline.is_empty());
    }

    #[test]
    fn full_hp_reset_breaks_prior_damage_lineage_only_when_hp_reaches_the_cap() {
        let snapshot = |current_hp, max_hp| RunWitnessResourceSnapshotV1 {
            act: 2,
            floor: 32,
            current_hp,
            max_hp,
            gold: 0,
            deck_size: 0,
            potions: Vec::new(),
        };

        assert!(establishes_full_hp_reset_v1(
            &snapshot(8, 80),
            &snapshot(80, 80)
        ));
        assert!(!establishes_full_hp_reset_v1(
            &snapshot(8, 80),
            &snapshot(42, 80)
        ));
        assert!(!establishes_full_hp_reset_v1(
            &snapshot(80, 80),
            &snapshot(80, 80)
        ));
    }
}
