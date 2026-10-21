from __future__ import annotations

from dataclasses import replace

import pytest

from learning.tests.torch_outcome_fixtures import (
    behavior_manifest_fixture,
    completed_attempt_fixture,
    decision_batch_fixture,
    public_attempt_trajectory_fixture,
    with_run_progress_fixture,
)
from sts_learning import (
    CompletedAttemptExperience,
    CreditAssignmentError,
    FloorProgressReturnConfig,
    compare_credit_assignment,
    matched_episode_floor_context_leave_one_out_advantages,
    matched_floor_context_leave_one_out_advantages,
    matched_floor_leave_one_out_advantages,
    remaining_floor_progress_return,
)


def _completed_attempt(
    *,
    reward: int,
    terminal_floor: int,
    decision_floors: tuple[int, ...],
    episode_seed: int = 100,
    attempt_index: int = 1,
    decision_acts: tuple[int, ...] | None = None,
):
    manifest_id = behavior_manifest_fixture().identity
    batches = tuple(
        with_run_progress_fixture(
            decision_batch_fixture(
                slot=0,
                semantic_row=index % 2,
                selected_ordinal=0,
                manifest_id=manifest_id,
            ),
            act=1 if decision_acts is None else decision_acts[index],
            floor=floor,
            is_combat=(index % 2 == 0),
            strategic_context_kind=None if index % 2 == 0 else 3,
            identity_suffix=f"decision-{index}",
        )
        for index, floor in enumerate(decision_floors)
    )
    lineage = replace(
        batches[0].lineages[0],
        key=replace(
            batches[0].lineages[0].key,
            episode_seed=episode_seed,
            attempt_index=attempt_index,
        ),
        recoveries_used=attempt_index - 1,
    )
    batches = tuple(
        replace(
            batch,
            lineages=(lineage,),
            run_progress=(
                replace(batch.run_progress[0], episode_seed=episode_seed),
            ),
        )
        for batch in batches
    )
    attempt = completed_attempt_fixture(slot=0, batches=batches, reward=reward)
    return replace(
        attempt,
        terminal=replace(
            attempt.terminal,
            terminal=replace(
                attempt.terminal.terminal,
                terminal_floor=terminal_floor,
            ),
        ),
    )


def _attempt(**kwargs):
    return public_attempt_trajectory_fixture(_completed_attempt(**kwargs))


def test_act_completion_separates_same_floor_on_either_side_of_act_transition() -> None:
    attempt = _attempt(reward=-1, terminal_floor=20, decision_floors=(17, 17),
                       decision_acts=(1, 2))
    attempt = replace(attempt, terminal=replace(attempt.terminal,
        terminal=replace(attempt.terminal.terminal, terminal_act=2)))
    config = FloorProgressReturnConfig(metric="act_completion")
    assert tuple(remaining_floor_progress_return(attempt, row.run_progress, config)
                 for row in attempt.decisions) == (.25, 0)
    comparison = compare_credit_assignment((attempt,), config)
    assert comparison.remaining_progress.maximum == .25
    assert comparison.remaining_progress.minimum == 0


def test_remaining_progress_assigns_stronger_blame_near_a_defeat() -> None:
    attempt = _attempt(reward=-1, terminal_floor=20, decision_floors=(0, 10, 20))
    config = FloorProgressReturnConfig(target_floor=52)

    targets = tuple(
        remaining_floor_progress_return(attempt, decision.run_progress, config)
        for decision in attempt.decisions
    )

    assert targets == pytest.approx((-12 / 52, -22 / 42, -1.0))
    assert targets[0] > targets[1] > targets[2]


def test_credit_comparison_keeps_victory_reserved_and_groups_decision_floors() -> None:
    defeat = _attempt(reward=-1, terminal_floor=20, decision_floors=(0, 10, 20))
    victory = _attempt(reward=1, terminal_floor=55, decision_floors=(10, 40))

    comparison = compare_credit_assignment(
        (defeat, victory),
        FloorProgressReturnConfig(target_floor=52),
    )

    assert comparison.attempt_count == 2
    assert comparison.terminal_broadcast.decision_count == 5
    assert comparison.terminal_broadcast.positive == 2
    assert comparison.remaining_progress.positive == 2
    assert comparison.matched_floor_advantage.negative == 1
    assert comparison.matched_floor_advantage.zero == 3
    assert comparison.matched_floor_advantage.positive == 1
    assert comparison.matched_floor_context_advantage.zero == 5
    assert tuple(
        (row.is_combat, row.remaining_progress.decision_count)
        for row in comparison.by_combat_scope
    ) == ((False, 2), (True, 3))
    assert tuple(
        (row.context_kind, row.remaining_progress.decision_count)
        for row in comparison.by_strategic_context
    ) == ((3, 2),)
    assert comparison.by_strategic_context[0].strategic_scope_weight == 1.0
    assert (
        comparison.by_strategic_context[0].matched_floor_strategic_weighted_target
        < 0.0
    )
    assert (
        comparison.by_strategic_context[
            0
        ].matched_floor_context_strategic_weighted_target
        == 0.0
    )
    assert tuple(item.floor for item in comparison.by_decision_floor) == (0, 10, 20, 40)
    assert comparison.by_decision_floor[-1].remaining_progress.minimum == 1.0

    aligned = matched_floor_leave_one_out_advantages(
        (defeat, victory),
        FloorProgressReturnConfig(target_floor=52),
    )
    assert tuple(len(attempt) for attempt in aligned) == (3, 2)
    assert aligned[0][0] == (0.0,)
    assert aligned[0][1][0] < 0.0
    assert aligned[1][0][0] > 0.0

    context_aligned = matched_floor_context_leave_one_out_advantages(
        (defeat, victory),
        FloorProgressReturnConfig(target_floor=52),
    )
    assert all(
        advantage == 0.0
        for attempt in context_aligned
        for batch in attempt
        for advantage in batch
    )


def test_credit_comparison_rejects_raw_or_impossible_progress() -> None:
    missing = _completed_attempt(
        reward=-1,
        terminal_floor=20,
        decision_floors=(0,),
    )
    assert isinstance(missing, CompletedAttemptExperience)
    missing = replace(
        missing,
        batches=(replace(missing.batches[0], run_progress=None),),
    )
    with pytest.raises(CreditAssignmentError, match="public attempt trajectories"):
        compare_credit_assignment((missing,), FloorProgressReturnConfig())

    impossible = _attempt(reward=-1, terminal_floor=4, decision_floors=(5,))
    with pytest.raises(CreditAssignmentError, match="precedes"):
        compare_credit_assignment((impossible,), FloorProgressReturnConfig())


def test_episode_matched_credit_uses_only_retries_from_the_same_root() -> None:
    first = _attempt(
        reward=-1,
        terminal_floor=10,
        decision_floors=(0,),
        episode_seed=10,
        attempt_index=1,
    )
    retry = _attempt(
        reward=-1,
        terminal_floor=20,
        decision_floors=(0,),
        episode_seed=10,
        attempt_index=2,
    )
    other_root = _attempt(
        reward=-1,
        terminal_floor=30,
        decision_floors=(0,),
        episode_seed=20,
        attempt_index=1,
    )

    aligned = matched_episode_floor_context_leave_one_out_advantages(
        (first, retry, other_root),
        FloorProgressReturnConfig(target_floor=52),
    )

    assert aligned[0][0][0] < 0.0
    assert aligned[1][0][0] > 0.0
    assert aligned[2][0][0] == 0.0

    comparison = compare_credit_assignment(
        (first, retry, other_root),
        FloorProgressReturnConfig(target_floor=52),
    )
    assert tuple(
        (
            row.episode_seed,
            row.attempt_count,
            row.terminal_floor_min,
            row.terminal_floor_max,
        )
        for row in comparison.by_episode_root
    ) == ((10, 2, 10, 20), (20, 1, 30, 30))
    first_root = comparison.by_episode_root[0]
    other = comparison.by_episode_root[1]
    assert first_root.matched_episode_floor_context_advantage.negative == 1
    assert first_root.matched_episode_floor_context_advantage.positive == 1
    assert other.matched_episode_floor_context_advantage.zero == 1
    assert all(
        row.strategic_matched_episode_floor_context_advantage is None
        for row in comparison.by_episode_root
    )
