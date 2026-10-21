use super::types::{BossPressureObjective, CollectorStartSignals};

pub(super) fn collector_objectives(
    start: &CollectorStartSignals,
    phase: &'static str,
) -> Vec<BossPressureObjective> {
    vec![
        minion_pressure_objective(start),
        pre_debuff_stability_objective(start, phase),
        boss_hp_progress_objective(start, phase),
    ]
}

fn minion_pressure_objective(start: &CollectorStartSignals) -> BossPressureObjective {
    if start.torch_heads_alive == 0 {
        objective(
            "reduce_minion_pressure",
            "satisfied",
            "no_torch_heads_alive",
        )
    } else if start.torch_heads_alive >= 2 || start.visible_incoming_damage >= 18 {
        objective(
            "reduce_minion_pressure",
            "violated",
            "two_torch_heads_or_high_incoming",
        )
    } else {
        objective("reduce_minion_pressure", "watch", "one_torch_head_alive")
    }
}

fn pre_debuff_stability_objective(
    start: &CollectorStartSignals,
    phase: &'static str,
) -> BossPressureObjective {
    if phase != "opening_spawn" && phase != "pre_mega_debuff" {
        return objective(
            "pre_mega_debuff_stability",
            "unknown",
            "already_past_debuff_window",
        );
    }
    if start.player_hp_percent <= 40 || start.visible_incoming_damage >= start.player_hp / 2 {
        objective(
            "pre_mega_debuff_stability",
            "violated",
            "low_hp_or_large_visible_attack_before_debuff",
        )
    } else {
        objective(
            "pre_mega_debuff_stability",
            "watch",
            "debuff_window_not_resolved",
        )
    }
}

fn boss_hp_progress_objective(
    start: &CollectorStartSignals,
    phase: &'static str,
) -> BossPressureObjective {
    if phase == "opening_spawn" {
        return objective("boss_hp_progress", "unknown", "too_early_to_measure");
    }
    if start.collector_hp_percent >= 75 {
        objective(
            "boss_hp_progress",
            "violated",
            "slow_boss_damage_before_or_after_debuff",
        )
    } else {
        objective("boss_hp_progress", "watch", "boss_damage_progress_visible")
    }
}

fn objective(
    tag: &'static str,
    status: &'static str,
    reason: &'static str,
) -> BossPressureObjective {
    BossPressureObjective {
        tag,
        status,
        reason,
    }
}
