use super::{EnemyPhaseTransitionHint, ProjectedMonsterDamage};

pub(super) const SPLIT_TRIGGER_RISK_PER_DEBT_HP: i32 = 3;
pub(super) const GUARDIAN_MODE_SHIFT_TRIGGER_RISK: i32 = 40;
pub(super) const LAGAVULIN_WAKE_RISK: i32 = 80;
pub(super) const CHAMP_THRESHOLD_TRIGGER_RISK: i32 = 160;
pub(super) const CHAMP_THRESHOLD_DEBT_RISK_PER_HP: i32 = 8;

const SPLIT_MOVE_ID: u8 = 3;

pub(super) fn observe_split_transition(
    hint: &mut EnemyPhaseTransitionHint,
    projected: &ProjectedMonsterDamage,
) {
    if !projected.split_power || projected.large_slime_split_already_triggered {
        return;
    }
    if projected.planned_move_id == SPLIT_MOVE_ID {
        return;
    }
    let threshold = projected.max_hp.saturating_div(2);
    if projected.current_hp > threshold
        && projected.projected_hp <= threshold
        && projected.projected_hp > 0
    {
        hint.split_trigger_count += 1;
        hint.split_debt_hp = hint.split_debt_hp.saturating_add(projected.projected_hp);
    }
}

pub(super) fn observe_guardian_transition(
    hint: &mut EnemyPhaseTransitionHint,
    projected: &ProjectedMonsterDamage,
) {
    if !projected.guardian_open || projected.guardian_close_up_triggered || projected.hp_loss <= 0 {
        return;
    }
    let Some(remaining) = projected.guardian_mode_shift_remaining else {
        return;
    };
    hint.guardian_min_threshold_remaining_before_hit = Some(
        hint.guardian_min_threshold_remaining_before_hit
            .map_or(remaining, |old| old.min(remaining)),
    );
    if projected.hp_loss >= remaining.max(0) {
        hint.guardian_mode_shift_trigger_count += 1;
    }
}

pub(super) fn observe_lagavulin_transition(
    hint: &mut EnemyPhaseTransitionHint,
    projected: &ProjectedMonsterDamage,
) {
    if projected.lagavulin_sleeping && projected.hp_loss > 0 {
        hint.lagavulin_wake_risk_count += 1;
    }
}

pub(super) fn observe_champ_threshold_transition(
    hint: &mut EnemyPhaseTransitionHint,
    projected: &ProjectedMonsterDamage,
) {
    if !projected.champ_threshold_pending || projected.hp_loss <= 0 {
        return;
    }
    let threshold = projected.max_hp.saturating_div(2);
    if projected.current_hp > threshold
        && projected.projected_hp <= threshold
        && projected.projected_hp > 0
    {
        hint.champ_threshold_trigger_count += 1;
        hint.champ_threshold_debt_hp = hint.champ_threshold_debt_hp.saturating_add(
            threshold
                .saturating_sub(projected.projected_hp)
                .saturating_add(1),
        );
    }
}
