use serde::Serialize;
use sts_oracle_runtime::runtime::combat::{CardLineageId, CombatCard};
use sts_oracle_runtime::state::core::ClientInput;

#[derive(Serialize)]
pub(crate) struct KeyCardLifecycleReport {
    pub(super) schema: &'static str,
    pub(super) contract: &'static str,
    pub(super) basis_line: &'static str,
    pub(super) witness_action_count: Option<usize>,
    pub(super) replayed_actions: usize,
    pub(super) truncated_by_preview: bool,
    pub(super) truncated: bool,
    pub(super) timed_out: bool,
    pub(super) tracked_cards: Vec<KeyCardLifecycle>,
}

#[derive(Serialize)]
pub(super) struct KeyCardLifecycle {
    pub(super) card: String,
    pub(super) lineage: CardLineageId,
    pub(super) upgrades: u8,
    pub(super) reason: KeyCardReason,
    pub(super) initial_zone: CardZoneLabel,
    pub(super) first_seen_zone: CardZoneAtStep,
    pub(super) first_play: Option<KeyCardPlay>,
    pub(super) final_zone: CardZoneAtStep,
    pub(super) played_in_replay: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KeyCardReason {
    StrengthScaling,
    ExhaustEngine,
}

#[derive(Clone)]
pub(crate) struct KeyCardTarget {
    pub(crate) card: CombatCard,
    pub(crate) reason: KeyCardReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CardZoneLabel {
    MasterOnly,
    Hand,
    Draw,
    Discard,
    Exhaust,
    Limbo,
    Queued,
    Missing,
}

#[derive(Clone, Serialize)]
pub(super) struct CardZoneAtStep {
    pub(super) step_index: usize,
    pub(super) zone: CardZoneLabel,
}

#[derive(Serialize)]
pub(super) struct KeyCardPlay {
    pub(super) step_index: usize,
    pub(super) action_key: String,
    pub(super) input: ClientInput,
}

pub(super) struct TrackedKeyCard {
    pub(super) card: CombatCard,
    pub(super) reason: KeyCardReason,
    pub(super) initial_zone: CardZoneLabel,
    pub(super) first_seen_zone: CardZoneAtStep,
    pub(super) first_play: Option<KeyCardPlay>,
}
