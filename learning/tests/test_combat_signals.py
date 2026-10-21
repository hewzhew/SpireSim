from __future__ import annotations

import pytest

from sts_learning import (
    CombatAxisSignalSummary,
    CombatGroupSignalSummary,
    CombatSignalError,
    build_combat_signal_census,
)


def _summary(root_byte: str, *, hp_signal: bool) -> CombatGroupSignalSummary:
    return CombatGroupSignalSummary(
        root_id=root_byte * 64,
        exact_combat_state_hash=("a" if root_byte != "a" else "b") * 64,
        replicate_count=4,
        decision_count=20,
        win=CombatAxisSignalSummary(0, 0),
        terminal_hp=CombatAxisSignalSummary(
            4 if hp_signal else 0,
            20 if hp_signal else 0,
        ),
        enemy_hp_progress=CombatAxisSignalSummary(0, 0),
        potion_retention=CombatAxisSignalSummary(0, 0),
    )


def test_census_aggregates_distinct_roots_without_payloads() -> None:
    census = build_combat_signal_census(
        (_summary("1", hp_signal=True), _summary("2", hp_signal=False)),
        max_groups=2,
    )

    assert census.group_count == 2
    assert census.replicate_count == 8
    assert census.decision_count == 40
    assert census.win.signal_group_count == 0
    assert census.terminal_hp.signal_group_count == 1
    assert census.terminal_hp.signal_replicate_count == 4
    assert census.terminal_hp.signal_decision_count == 20
    assert census.enemy_hp_progress.signal_group_count == 0


def test_census_rejects_duplicate_roots_and_group_overflow() -> None:
    summary = _summary("1", hp_signal=True)
    for summaries, bound in (
        ((summary, summary), 2),
        ((summary,), 0),
    ):
        with pytest.raises(CombatSignalError):
            build_combat_signal_census(summaries, max_groups=bound)


def test_axis_signal_rejects_decisions_without_a_signal_replicate() -> None:
    with pytest.raises(CombatSignalError):
        CombatAxisSignalSummary(0, 1)
