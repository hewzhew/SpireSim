use super::*;

#[test]
fn rollout_priority_prefers_evaluated_terminal_win() {
    let unresolved = RolloutNodeEstimate::unevaluated();
    let mut win = RolloutNodeEstimate::unevaluated();
    win.evaluated = true;
    win.terminal = SearchTerminalLabel::Win;
    win.final_hp = 3;

    assert!(rollout_priority_value(&win) > rollout_priority_value(&unresolved));
}

#[test]
fn rollout_priority_prefers_higher_hp_after_terminal_rank() {
    let low = terminal_win_with_hp(10);
    let high = terminal_win_with_hp(20);

    assert!(rollout_priority_value(&high) > rollout_priority_value(&low));
}

#[test]
fn rollout_priority_prefers_unresolved_stable_progress_over_extra_hp_stall() {
    let progress = unresolved_estimate(40, 20, 30);
    let stalled = unresolved_estimate(55, 25, 180);

    assert!(rollout_priority_value(&progress) > rollout_priority_value(&stalled));
}

#[test]
fn rollout_priority_prefers_survival_margin_when_unresolved_state_is_critical() {
    let safer = unresolved_estimate(12, 5, 120);
    let riskier_progress = unresolved_estimate(20, 1, 20);

    assert!(rollout_priority_value(&safer) > rollout_priority_value(&riskier_progress));
}

#[test]
fn rollout_priority_does_not_rank_simulated_loss_above_unresolved_estimate() {
    let mut loss = RolloutNodeEstimate::unevaluated();
    loss.evaluated = true;
    loss.terminal = SearchTerminalLabel::Loss;

    let unresolved = unresolved_estimate(1, -10, 1);

    assert!(rollout_priority_value(&unresolved) > rollout_priority_value(&loss));
}

#[test]
fn rollout_priority_gives_unevaluated_live_state_urgency_over_simulated_loss() {
    let unevaluated = RolloutNodeEstimate::unevaluated();
    let mut simulated_loss = RolloutNodeEstimate::unevaluated();
    simulated_loss.evaluated = true;
    simulated_loss.terminal = SearchTerminalLabel::Loss;

    assert!(rollout_priority_value(&unevaluated) > rollout_priority_value(&simulated_loss));
}

#[test]
fn rollout_priority_prefers_progress_over_safer_margin_between_simulated_losses() {
    let mut safer_stall = RolloutNodeEstimate::unevaluated();
    safer_stall.evaluated = true;
    safer_stall.terminal = SearchTerminalLabel::Loss;
    safer_stall.final_hp = 0;
    safer_stall.survival_margin = -1;
    safer_stall.phase_adjusted_enemy_effort = 240;

    let mut closer_race = safer_stall.clone();
    closer_race.survival_margin = -20;
    closer_race.phase_adjusted_enemy_effort = 20;

    assert!(rollout_priority_value(&closer_race) > rollout_priority_value(&safer_stall));
}

#[test]
fn rollout_priority_uses_phase_adjusted_enemy_effort_for_unresolved_states() {
    let mut lower_effort = RolloutNodeEstimate::unevaluated();
    lower_effort.evaluated = true;
    lower_effort.terminal = SearchTerminalLabel::Unresolved;
    lower_effort.final_hp = 40;
    lower_effort.phase_adjusted_enemy_effort = 30;

    let mut higher_effort = lower_effort.clone();
    higher_effort.phase_adjusted_enemy_effort = 50;

    assert!(rollout_priority_value(&lower_effort) > rollout_priority_value(&higher_effort));
}

#[test]
fn rollout_priority_penalizes_unresolved_high_fanout_pending_choices() {
    let mut low_fanout = RolloutNodeEstimate::unevaluated();
    low_fanout.evaluated = true;
    low_fanout.terminal = SearchTerminalLabel::Unresolved;
    low_fanout.final_hp = 40;
    low_fanout.phase_adjusted_enemy_effort = 30;
    low_fanout.pending_choice_estimated_action_fanout = 4;

    let mut high_fanout = low_fanout.clone();
    high_fanout.high_fanout_pending_choice = true;
    high_fanout.pending_choice_estimated_action_fanout = 128;

    assert!(rollout_priority_value(&low_fanout) > rollout_priority_value(&high_fanout));
}

#[test]
fn rollout_priority_uses_mechanics_pressure_for_unresolved_states() {
    let mut lower_pressure = RolloutNodeEstimate::unevaluated();
    lower_pressure.evaluated = true;
    lower_pressure.terminal = SearchTerminalLabel::Unresolved;
    lower_pressure.final_hp = 40;
    lower_pressure.phase_adjusted_enemy_effort = 30;

    let mut higher_pressure = lower_pressure.clone();
    higher_pressure.gremlin_nob_anger_amount_total = 3;

    assert!(rollout_priority_value(&lower_pressure) > rollout_priority_value(&higher_pressure));
}

fn terminal_win_with_hp(final_hp: i32) -> RolloutNodeEstimate {
    let mut estimate = RolloutNodeEstimate::unevaluated();
    estimate.evaluated = true;
    estimate.terminal = SearchTerminalLabel::Win;
    estimate.final_hp = final_hp;
    estimate
}

fn unresolved_estimate(
    final_hp: i32,
    survival_margin: i32,
    phase_adjusted_enemy_effort: i32,
) -> RolloutNodeEstimate {
    let mut estimate = RolloutNodeEstimate::unevaluated();
    estimate.evaluated = true;
    estimate.terminal = SearchTerminalLabel::Unresolved;
    estimate.final_hp = final_hp;
    estimate.survival_margin = survival_margin;
    estimate.phase_adjusted_enemy_effort = phase_adjusted_enemy_effort;
    estimate
}
