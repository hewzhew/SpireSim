use serde::Serialize;
use sts_oracle_runtime::ai::combat_search_v2::SearchTerminalLabel;

#[derive(Serialize)]
pub(crate) struct BossPressureLensReport {
    pub(super) schema: &'static str,
    pub(super) boss: &'static str,
    pub(super) phase: &'static str,
    pub(super) start: CollectorStartSignals,
    pub(super) tags: Vec<&'static str>,
    pub(super) objectives: Vec<BossPressureObjective>,
    pub(super) potion_permission: BossPotionPermission,
    pub(super) line_reviews: Vec<BossLineReview>,
}

#[derive(Serialize)]
pub(super) struct CollectorStartSignals {
    pub(super) turn: u32,
    pub(super) player_hp: i32,
    pub(super) player_max_hp: i32,
    pub(super) player_hp_percent: i32,
    pub(super) collector_hp: i32,
    pub(super) collector_max_hp: i32,
    pub(super) collector_hp_percent: i32,
    pub(super) torch_heads_alive: usize,
    pub(super) visible_incoming_damage: i32,
}

#[derive(Serialize)]
pub(super) struct BossPressureObjective {
    pub(super) tag: &'static str,
    pub(super) status: &'static str,
    pub(super) reason: &'static str,
}

#[derive(Serialize)]
pub(super) struct BossPotionPermission {
    pub(super) level: &'static str,
    pub(super) reason: &'static str,
}

#[derive(Serialize)]
pub(super) struct BossLineReview {
    pub(super) source: String,
    pub(super) terminal: SearchTerminalLabel,
    pub(super) final_hp: Option<i32>,
    pub(super) hp_loss: Option<i32>,
    pub(super) turns: Option<u32>,
    pub(super) potions_used: Option<u32>,
    pub(super) tags: Vec<&'static str>,
}
