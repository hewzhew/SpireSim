use super::types::{BossPotionPermission, CollectorStartSignals};

pub(super) fn collector_phase(turn: u32) -> &'static str {
    match turn {
        0 | 1 => "opening_spawn",
        2..=4 => "pre_mega_debuff",
        _ => "post_mega_debuff_or_late",
    }
}

pub(super) fn collector_potion_permission(
    start: &CollectorStartSignals,
    phase: &'static str,
) -> BossPotionPermission {
    if (phase == "opening_spawn" || phase == "pre_mega_debuff")
        && (start.player_hp_percent <= 40
            || start.visible_incoming_damage >= 20
            || start.torch_heads_alive >= 2)
    {
        BossPotionPermission {
            level: "allow",
            reason: "prevent_collector_early_collapse",
        }
    } else {
        BossPotionPermission {
            level: "no_special_permission",
            reason: "collector_pressure_not_yet_emergency",
        }
    }
}

pub(super) fn collector_tags(
    start: &CollectorStartSignals,
    phase: &'static str,
) -> Vec<&'static str> {
    let mut tags = Vec::new();
    if start.torch_heads_alive >= 2 || start.visible_incoming_damage >= 18 {
        tags.push("collector_minion_pressure_high");
    }
    if (phase == "opening_spawn" || phase == "pre_mega_debuff") && start.player_hp_percent <= 40 {
        tags.push("collector_pre_debuff_collapse_risk");
    }
    if phase != "opening_spawn" && start.collector_hp_percent >= 75 {
        tags.push("collector_boss_progress_slow");
    }
    tags
}
