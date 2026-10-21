from __future__ import annotations

from dataclasses import dataclass

import pytest

from sts_learning import (
    BridgeDecisionProgressProvider,
    DecisionProgressError,
    DecisionRunProgress,
    PublicDecisionSnapshot,
)


@dataclass(frozen=True)
class _View:
    seed: int
    act: int
    floor: int
    is_combat: bool
    strategic_context_kind: int | None


@dataclass(frozen=True)
class _SnapshotView:
    phase: int
    is_combat: bool
    snapshot_id: str
    observation_id: str
    history_snapshot_id: str
    candidate_surface_id: str
    candidate_ids: tuple[str, ...]


class _Environment:
    def __init__(self, rows: object, snapshot_rows: object) -> None:
        self.rows = rows
        self.snapshot_rows = snapshot_rows

    def public_run_contexts(self) -> object:
        return self.rows

    def public_information_snapshots(self) -> object:
        return self.snapshot_rows


def _snapshot(slot: int, *, phase: int, is_combat: bool) -> _SnapshotView:
    return _SnapshotView(
        phase=phase,
        is_combat=is_combat,
        snapshot_id=f"snapshot-{slot}",
        observation_id=f"observation-{slot}",
        history_snapshot_id=f"history-{slot}",
        candidate_surface_id=f"surface-{slot}",
        candidate_ids=(f"candidate-{slot}-0", f"candidate-{slot}-1"),
    )


def _public_snapshot(slot: int, *, phase: int, is_combat: bool) -> PublicDecisionSnapshot:
    view = _snapshot(slot, phase=phase, is_combat=is_combat)
    return PublicDecisionSnapshot(
        phase=view.phase,
        is_combat=view.is_combat,
        snapshot_id=view.snapshot_id,
        observation_id=view.observation_id,
        history_snapshot_id=view.history_snapshot_id,
        candidate_surface_id=view.candidate_surface_id,
        candidate_ids=view.candidate_ids,
    )


def test_bridge_provider_returns_requested_slots_in_decision_order() -> None:
    provider = BridgeDecisionProgressProvider(
        _Environment(
            [
                (
                    0,
                    _View(
                        seed=101,
                        act=1,
                        floor=3,
                        is_combat=False,
                        strategic_context_kind=3,
                    ),
                ),
                (
                    1,
                    _View(
                        seed=202,
                        act=2,
                        floor=21,
                        is_combat=True,
                        strategic_context_kind=None,
                    ),
                ),
                (2, object()),
            ],
            [
                (0, _snapshot(0, phase=0, is_combat=False)),
                (1, _snapshot(1, phase=1, is_combat=True)),
            ],
        )
    )

    assert provider.capture((1, 0)) == (
        DecisionRunProgress(
            episode_seed=202,
            act=2,
            floor=21,
            is_combat=True,
            strategic_context_kind=None,
            public_snapshot=_public_snapshot(1, phase=1, is_combat=True),
        ),
        DecisionRunProgress(
            episode_seed=101,
            act=1,
            floor=3,
            is_combat=False,
            strategic_context_kind=3,
            public_snapshot=_public_snapshot(0, phase=0, is_combat=False),
        ),
    )


@pytest.mark.parametrize(
    ("rows", "message"),
    [
        (
            [(0, _View(101, 1, 3, False, 3)), (0, _View(101, 1, 3, False, 3))],
            "repeat",
        ),
        ([(0, _View(101, 1, 3, False, 3))], "slot 1"),
        ([(0, object())], "missing seed"),
    ],
)
def test_bridge_provider_rejects_malformed_or_missing_contexts(
    rows: object,
    message: str,
) -> None:
    requested_slot = 1 if message == "slot 1" else 0
    provider = BridgeDecisionProgressProvider(
        _Environment(
            rows,
            [
                (
                    requested_slot,
                    _snapshot(requested_slot, phase=0, is_combat=False),
                )
            ],
        )
    )

    with pytest.raises(DecisionProgressError, match=message):
        provider.capture((requested_slot,))


def test_bridge_provider_requires_snapshot_for_every_requested_decision() -> None:
    provider = BridgeDecisionProgressProvider(
        _Environment(
            [(0, _View(101, 1, 3, False, 3))],
            [],
        )
    )

    with pytest.raises(DecisionProgressError, match="no public information snapshots"):
        provider.capture((0,))


def test_bridge_provider_requires_the_typed_bridge_surface() -> None:
    with pytest.raises(DecisionProgressError, match="public_run_contexts"):
        BridgeDecisionProgressProvider(object())
