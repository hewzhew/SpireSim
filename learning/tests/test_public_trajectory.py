from __future__ import annotations

from dataclasses import fields, replace

import pytest

from learning.tests.policy_fixtures import BEHAVIOR_MANIFEST_ID
from learning.tests.semantic_fixtures import semantic_batch_fixture
from sts_learning import (
    AttemptKey,
    CompletedAttemptExperience,
    DecisionExperienceBatch,
    DecisionLineage,
    DecisionRunProgress,
    PreparedDecisionBatch,
    PublicDecisionSnapshot,
    PublicTrajectoryDecisionV1,
    PublicTrajectoryError,
    RecoverySlotSnapshot,
    RecoverySlotStatus,
    SelectionProbability,
    TerminalAttemptOutcome,
    TerminalAttemptRecord,
    build_public_attempt_trajectory,
    iter_payload_arrays,
    select_semantic_decision_rows,
)


def _snapshot(snapshot_id: str) -> PublicDecisionSnapshot:
    return PublicDecisionSnapshot(
        phase=1,
        is_combat=True,
        snapshot_id=snapshot_id,
        observation_id=f"observation-{snapshot_id}",
        history_snapshot_id=f"history-{snapshot_id}",
        candidate_surface_id=f"surface-{snapshot_id}",
        candidate_ids=(f"{snapshot_id}-candidate-0", f"{snapshot_id}-candidate-1"),
    )


def _attempt(*, reward: int = 1) -> CompletedAttemptExperience:
    lineage = DecisionLineage(
        key=AttemptKey(
            slot_index=4,
            episode_seed=104,
            episode_generation=0,
            attempt_index=1,
        ),
        recoveries_used=0,
    )
    recovery_snapshot = RecoverySlotSnapshot(
        slot_index=4,
        episode_seed=104,
        episode_generation=0,
        attempt_index=1,
        recoveries_used=0,
        status=RecoverySlotStatus.ACTIVE,
        pending_terminal=None,
    )
    payload = select_semantic_decision_rows(semantic_batch_fixture(), [0])
    batches = []
    for index, probability in enumerate((0.25, 0.75)):
        prepared = PreparedDecisionBatch.capture(
            payload,
            [recovery_snapshot],
            [
                DecisionRunProgress(
                    episode_seed=104,
                    act=1,
                    floor=3,
                    is_combat=True,
                    strategic_context_kind=None,
                    public_snapshot=_snapshot(f"decision-{index}"),
                )
            ],
        )
        batches.append(
            DecisionExperienceBatch.from_prepared(
                prepared,
                [1 - index],
                [SelectionProbability.known(probability)],
                BEHAVIOR_MANIFEST_ID,
            )
        )
    terminal = TerminalAttemptRecord(
        episode_seed=104,
        episode_generation=0,
        attempt_index=1,
        recoveries_used=0,
        terminal=TerminalAttemptOutcome(
            slot_index=4,
            terminal_reward=reward,
            terminal_act=1,
            terminal_floor=3,
            terminal_hp=20 if reward == 1 else 0,
            terminal_max_hp=80,
            terminal_gold=50,
        ),
    )
    return CompletedAttemptExperience(
        lineage=lineage,
        batches=tuple(batches),
        terminal=terminal,
        decision_count=len(batches),
        payload_bytes=sum(batch.payload_bytes for batch in batches),
    )


def test_raw_public_trajectory_preserves_behavior_and_sparse_environment_reward() -> None:
    attempt = _attempt(reward=1)

    trajectory = build_public_attempt_trajectory(attempt)

    assert trajectory.lineage == attempt.lineage
    assert trajectory.terminal == attempt.terminal
    assert tuple(row.chronological_index for row in trajectory.decisions) == (0, 1)
    assert tuple(row.public_snapshot.snapshot_id for row in trajectory.decisions) == (
        "decision-0",
        "decision-1",
    )
    assert tuple(row.run_progress for row in trajectory.decisions) == tuple(
        batch.run_progress[0]  # type: ignore[index]
        for batch in attempt.batches
    )
    assert tuple(row.selected_ordinal for row in trajectory.decisions) == (1, 0)
    assert tuple(
        row.selection_probability.value for row in trajectory.decisions
    ) == (0.25, 0.75)
    assert all(
        row.behavior_manifest_id == BEHAVIOR_MANIFEST_ID
        for row in trajectory.decisions
    )
    assert tuple(row.environment_reward for row in trajectory.decisions) == (0, 1)
    assert tuple(row.terminated for row in trajectory.decisions) == (False, True)
    assert trajectory.decisions[0].selected_candidate_id.endswith("candidate-1")
    assert trajectory.decisions[1].selected_candidate_id.endswith("candidate-0")
    assert all(
        not array.flags.writeable
        for row in trajectory.decisions
        for array in iter_payload_arrays(row.semantic_payload)
    )

    field_names = {field.name for field in fields(PublicTrajectoryDecisionV1)}
    assert "return_to_go" not in field_names
    assert "advantage" not in field_names
    assert "teacher" not in field_names


def test_raw_public_trajectory_keeps_defeat_only_on_the_terminal_transition() -> None:
    trajectory = build_public_attempt_trajectory(_attempt(reward=-1))

    assert tuple(row.environment_reward for row in trajectory.decisions) == (0, -1)
    assert tuple(row.terminated for row in trajectory.decisions) == (False, True)


def test_raw_public_trajectory_rejects_missing_or_misaligned_public_snapshot() -> None:
    attempt = _attempt()
    first = attempt.batches[0]
    assert first.run_progress is not None
    missing = replace(
        first,
        run_progress=(replace(first.run_progress[0], public_snapshot=None),),
    )

    with pytest.raises(PublicTrajectoryError, match="sanitized snapshot"):
        build_public_attempt_trajectory(
            replace(attempt, batches=(missing, attempt.batches[1]))
        )

    public_snapshot = first.run_progress[0].public_snapshot
    assert public_snapshot is not None
    misaligned = replace(
        first,
        run_progress=(
            replace(
                first.run_progress[0],
                public_snapshot=replace(
                    public_snapshot,
                    candidate_ids=("candidate-only",),
                ),
            ),
        ),
    )
    with pytest.raises(PublicTrajectoryError, match="candidates"):
        build_public_attempt_trajectory(
            replace(attempt, batches=(misaligned, attempt.batches[1]))
        )

    trajectory = build_public_attempt_trajectory(attempt)
    with pytest.raises(PublicTrajectoryError, match="progress and snapshot"):
        replace(
            trajectory.decisions[0],
            public_snapshot=trajectory.decisions[1].public_snapshot,
        )
