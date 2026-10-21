use serde::Serialize;
use sts_oracle_runtime::state::core::ClientInput;

#[derive(Serialize)]
pub(crate) struct ChampPhaseAudit {
    pub(super) schema: &'static str,
    pub(super) contract: &'static str,
    pub(super) basis_line: &'static str,
    pub(super) witness_action_count: Option<usize>,
    pub(super) replayed_actions: usize,
    pub(super) truncated_by_preview: bool,
    pub(super) truncated: bool,
    pub(super) timed_out: bool,
    pub(super) initial_snapshot: ChampPhaseSnapshot,
    pub(super) first_below_half_hp: Option<ChampHpCrossing>,
    pub(super) split_trigger: Option<ChampSplitTrigger>,
    pub(super) post_split_snapshot: Option<ChampPhaseSnapshot>,
    pub(super) resources_before_split: ChampResourceTiming,
    pub(super) flags: Vec<ChampPhaseAuditFlag>,
    pub(super) verdict: ChampPhaseAuditVerdict,
}

#[derive(Clone, Serialize)]
pub(super) struct ChampPhaseSnapshot {
    pub(super) step_index: usize,
    pub(super) player_hp: i32,
    pub(super) player_max_hp: i32,
    pub(super) player_block: i32,
    pub(super) champ_hp: i32,
    pub(super) champ_max_hp: i32,
    pub(super) champ_block: i32,
    pub(super) champ_strength: i32,
    pub(super) champ_weak: i32,
    pub(super) champ_vulnerable: i32,
    pub(super) champ_threshold_reached: bool,
    pub(super) champ_move_id: u8,
    pub(super) total_enemy_hp: i32,
    pub(super) living_enemy_count: usize,
}

#[derive(Serialize)]
pub(super) struct ChampSplitTrigger {
    pub(super) step_index: usize,
    pub(super) action_key: String,
    pub(super) input: ClientInput,
    pub(super) before: ChampPhaseSnapshot,
    pub(super) after: ChampPhaseSnapshot,
}

#[derive(Serialize)]
pub(super) struct ChampHpCrossing {
    pub(super) step_index: usize,
    pub(super) action_key: String,
    pub(super) input: ClientInput,
    pub(super) before_champ_hp: i32,
    pub(super) after_champ_hp: i32,
}

#[derive(Default, Serialize)]
pub(super) struct ChampResourceTiming {
    pub(super) disarm_used_before_split: bool,
    pub(super) disarm_step: Option<usize>,
    pub(super) fear_potion_used_before_split: bool,
    pub(super) fear_potion_step: Option<usize>,
    pub(super) strength_potion_used_before_split: bool,
    pub(super) strength_potion_step: Option<usize>,
    pub(super) steroid_potion_used_before_split: bool,
    pub(super) steroid_potion_step: Option<usize>,
    pub(super) forge_potion_used_before_split: bool,
    pub(super) forge_potion_step: Option<usize>,
    pub(super) potions_used_before_split: u32,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ChampPhaseAuditFlag {
    NoSplitReached,
    SplitObserved,
    DisarmSpentBeforeSplit,
    FearPotionSpentBeforeSplit,
    BurstPotionSpentBeforeSplit,
    SplitWithLowHp,
    SplitWithChampHpStillHigh,
    ReplayTruncated,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ChampPhaseAuditVerdict {
    NoSplitReached,
    SplitObserved,
    ResourceSpentBeforeSplit,
    SplitWithLowHp,
    Unclear,
}

pub(super) struct ChampPhaseReplay {
    pub(super) witness_action_count: Option<usize>,
    pub(super) replayed_actions: usize,
    pub(super) truncated_by_preview: bool,
    pub(super) truncated: bool,
    pub(super) timed_out: bool,
    pub(super) initial_snapshot: ChampPhaseSnapshot,
    pub(super) first_below_half_hp: Option<ChampHpCrossing>,
    pub(super) split_trigger: Option<ChampSplitTrigger>,
    pub(super) post_split_snapshot: Option<ChampPhaseSnapshot>,
    pub(super) resources_before_split: ChampResourceTiming,
}
