use sts_oracle_runtime::sim::combat::CombatPosition;

use super::focus::CombatReviewFocus;

#[path = "champ_phase/replay.rs"]
mod replay;
#[path = "champ_phase/resources.rs"]
mod resources;
#[path = "champ_phase/snapshot.rs"]
mod snapshot;
#[path = "champ_phase/types.rs"]
mod types;
#[path = "champ_phase/verdict.rs"]
mod verdict;

pub(super) use types::ChampPhaseAudit;

use replay::replay_champ_phase_focus;
use verdict::{champ_phase_flags, champ_phase_verdict};

pub(super) fn champ_phase_audit(
    root: &CombatPosition,
    focus: &CombatReviewFocus,
) -> Option<ChampPhaseAudit> {
    let replay = replay_champ_phase_focus(root, focus)?;
    let interrupted = replay.truncated || replay.timed_out || replay.truncated_by_preview;
    let flags = champ_phase_flags(
        replay.split_trigger.as_ref(),
        replay.post_split_snapshot.as_ref(),
        &replay.resources_before_split,
        interrupted,
    );
    let verdict = champ_phase_verdict(&flags, replay.split_trigger.is_some(), interrupted);

    Some(ChampPhaseAudit {
        schema: "champ_phase_audit_v0",
        contract: "exact_replay_timing_snapshot_only_no_search_policy_change_no_strategy_verdict",
        basis_line: focus.selected_review,
        witness_action_count: replay.witness_action_count,
        replayed_actions: replay.replayed_actions,
        truncated_by_preview: replay.truncated_by_preview,
        truncated: replay.truncated,
        timed_out: replay.timed_out,
        initial_snapshot: replay.initial_snapshot,
        first_below_half_hp: replay.first_below_half_hp,
        split_trigger: replay.split_trigger,
        post_split_snapshot: replay.post_split_snapshot,
        resources_before_split: replay.resources_before_split,
        flags,
        verdict,
    })
}
