use sts_oracle_runtime::ai::combat_search_v2::SearchTerminalLabel;

use super::types::BossLineReview;

pub(super) fn aggregate_line_tags(line_reviews: &[BossLineReview]) -> Vec<&'static str> {
    let mut tags = Vec::new();
    if line_reviews
        .iter()
        .any(|line| line.tags.contains(&"no_win_left_multi_target_pressure"))
    {
        tags.push("collector_lines_leave_multi_target_pressure");
    }
    if line_reviews
        .iter()
        .any(|line| line.tags.contains(&"no_win_boss_hp_still_high"))
    {
        tags.push("collector_lines_leave_boss_hp_high");
    }
    if line_reviews
        .iter()
        .any(|line| line.tags.contains(&"failed_after_debuff_window"))
    {
        tags.push("collector_lines_fail_after_debuff_window");
    }
    if line_reviews
        .iter()
        .any(|line| line.tags.contains(&"dirty_win_low_hp"))
    {
        tags.push("collector_dirty_win_only");
    }
    tags
}

pub(super) fn line_quality_tags(
    terminal: SearchTerminalLabel,
    final_hp: Option<i32>,
    turns: Option<u32>,
    potions_used: Option<u32>,
    living_enemy_count: usize,
    total_enemy_hp: i32,
) -> Vec<&'static str> {
    let mut tags = Vec::new();
    if terminal == SearchTerminalLabel::Win && final_hp.is_some_and(|hp| hp <= 10) {
        tags.push("dirty_win_low_hp");
    }
    if terminal == SearchTerminalLabel::Win
        && final_hp.is_some_and(|hp| hp <= 20)
        && potions_used.unwrap_or(0) > 0
    {
        tags.push("potion_rescue_desperate");
    }
    if terminal != SearchTerminalLabel::Win && living_enemy_count >= 2 {
        tags.push("no_win_left_multi_target_pressure");
    }
    if terminal != SearchTerminalLabel::Win && total_enemy_hp >= 80 {
        tags.push("no_win_boss_hp_still_high");
    }
    if terminal != SearchTerminalLabel::Win && turns.unwrap_or(0) >= 4 {
        tags.push("failed_after_debuff_window");
    }
    tags
}
