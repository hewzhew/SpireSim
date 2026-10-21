from __future__ import annotations

from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest

pytest.importorskip("torch")

from learning.tests.semantic_fixtures import (
    semantic_batch_fixture,
    semantic_schema_fixture,
)
from learning.tests.torch_combat_fixtures import OneRoundCombatGroup
from sts_learning.combat_objective import CombatWinObjectiveConfig
from sts_learning.combat_potion_lane import CombatPotionLane
from sts_learning.torch_combat_recovery_session import (
    CombatWinRecoverySessionFactory,
)
from sts_learning.torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinBatchSessionConfig,
    CombatWinSessionProfile,
)


# This fixture deliberately crosses semantic sampling, exact ordinal replay,
# opaque root capture, potion-lane rebinding, and one optimizer step. Testing
# those owners separately would not catch the lane contamination this contract
# was added to prevent.
class _RecoveryHandle:
    def __init__(self, transition: int, source_root: tuple[str, str]) -> None:
        self.root_id = f"{transition + 3:02x}" * 32
        self.exact_combat_state_hash = f"{transition + 9:02x}" * 32
        self.source_root_id, self.source_exact_combat_state_hash = source_root
        self.source_replicate_index = 0
        self.spawned_slots: list[tuple[int, ...] | None] = []

    def spawn_group(
        self,
        replicate_count: int,
        potion_slots: tuple[int, ...] | None = None,
    ) -> OneRoundCombatGroup:
        assert replicate_count == 2
        self.spawned_slots.append(potion_slots)
        return OneRoundCombatGroup(
            self.root_id,
            self.exact_combat_state_hash,
            (True, False),
            potion_slots=potion_slots,
        )


class _ReplayableGroup:
    def __init__(
        self,
        root_id: str,
        state_hash: str,
        replicate_count: int,
        potion_slots: tuple[int, ...] | None,
        *,
        has_win: bool,
    ) -> None:
        self.root_id = root_id
        self.exact_combat_state_hash = state_hash
        self.replicate_count = replicate_count
        self.potion_slots = potion_slots
        self.has_win = has_win
        self.terminal_count = 0
        self.ready = False
        self.transition = 0
        self.recovery_handles: list[_RecoveryHandle] = []

    def capture_recovery_root(self, replicate_index: int) -> _RecoveryHandle:
        assert self.replicate_count == 1
        assert replicate_index == 0 and not self.ready and not self.terminal_count
        handle = _RecoveryHandle(
            self.transition,
            (self.root_id, self.exact_combat_state_hash),
        )
        self.recovery_handles.append(handle)
        return handle

    def decision_batch(self, *, semantic: bool) -> dict[str, object]:
        assert not self.ready and not self.terminal_count
        if not semantic:
            assert self.replicate_count == 1
            return {"slot_indices": (0,), "candidate_counts": (3,)}
        assert self.replicate_count == 2
        batch = semantic_batch_fixture()
        batch["slot_indices"] = np.asarray([0, 1], dtype=np.uint64)
        return batch

    def decision_progress(self) -> list[SimpleNamespace]:
        assert not self.ready and not self.terminal_count
        return [
            SimpleNamespace(
                replicate_index=replicate_index,
                turn=self.transition + 1,
                player_hp=80 - 15 * self.transition,
                player_max_hp=80,
                enemy_hp=40 - 15 * self.transition,
                enemy_max_hp=40,
                potion_uuids=(101 + replicate_index,),
                potion_ids=("FearPotion",),
            )
            for replicate_index in range(self.replicate_count)
        ]

    def choose(self, ordinals: list[int]) -> None:
        assert not self.ready and len(ordinals) == self.replicate_count
        self.ready = True

    def step(self) -> dict[str, object]:
        assert self.ready and not self.terminal_count
        self.ready = False
        self.transition += 1
        terminal = self.transition == 3
        if terminal:
            self.terminal_count = self.replicate_count
        if self.replicate_count == 1:
            won = (self.has_win,)
            final_hp = (30 if self.has_win else 0,)
            potion_use = (1 if self.has_win else 0,)
        else:
            won = (False, self.has_win)
            final_hp = (0, 30 if self.has_win else 0)
            potion_use = (0, 1 if self.has_win else 0)
        row_count = self.replicate_count if terminal else 0
        return {
            "root_id": self.root_id,
            "exact_combat_state_hash": self.exact_combat_state_hash,
            "terminal_slot_indices": np.arange(row_count, dtype=np.uint64),
            "terminal_kind": np.asarray(
                [0 if row else 1 for row in won] if terminal else [],
                dtype=np.uint8,
            ),
            "terminal_won": np.asarray(won if terminal else [], dtype=np.bool_),
            "terminal_start_hp": np.asarray(
                [80] * row_count,
                dtype=np.int32,
            ),
            "terminal_final_hp": np.asarray(
                final_hp if terminal else [],
                dtype=np.int32,
            ),
            "terminal_final_max_hp": np.asarray(
                [80] * row_count,
                dtype=np.int32,
            ),
            "terminal_final_gold": np.asarray(
                [99] * row_count,
                dtype=np.int32,
            ),
            "terminal_hp_loss": np.asarray(
                [80 - hp for hp in final_hp] if terminal else [],
                dtype=np.int32,
            ),
            "terminal_enemy_start_hp": np.asarray(
                [40] * row_count,
                dtype=np.int32,
            ),
            "terminal_enemy_final_hp": np.asarray(
                [0] * row_count,
                dtype=np.int32,
            ),
            "terminal_turns": np.asarray([3] * row_count, dtype=np.uint32),
            "terminal_potions_used": np.asarray(
                potion_use if terminal else [],
                dtype=np.uint32,
            ),
            "terminal_potions_discarded": np.asarray(
                [0] * row_count,
                dtype=np.uint32,
            ),
            "terminal_cards_played": np.asarray(
                [8] * row_count,
                dtype=np.uint32,
            ),
            "terminal_potion_ids": (
                tuple((None,) for _ in range(row_count)) if terminal else ()
            ),
        }


class _ReplayableSource:
    root = ("12" * 32, "ab" * 32)

    def __init__(self, *, has_win: bool = True, expected_slot: int = 0) -> None:
        self.has_win = has_win
        self.expected_slot = expected_slot
        self.calls: list[tuple[int, int, tuple[int, ...] | None]] = []
        self.groups: list[_ReplayableGroup] = []

    def combat_group(
        self,
        slot_index: int,
        replicate_count: int,
        potion_slots: tuple[int, ...] | None = None,
    ) -> _ReplayableGroup:
        assert slot_index == self.expected_slot
        normalized_slots = None if potion_slots is None else tuple(potion_slots)
        self.calls.append((slot_index, replicate_count, normalized_slots))
        group = _ReplayableGroup(
            *self.root,
            replicate_count,
            normalized_slots,
            has_win=self.has_win,
        )
        self.groups.append(group)
        return group


def _config() -> CombatWinBatchSessionConfig:
    profile = replace(
        CombatWinSessionProfile(),
        objective=CombatWinObjectiveConfig(groups_per_update=2),
    )
    return CombatWinBatchSessionConfig(
        expected_roots=2,
        max_roots=2,
        replicate_count=2,
        profile=profile,
        potion_lane=CombatPotionLane.ROOT_SLOTS,
        potion_slots=(0,),
    )


def test_verified_source_win_trains_terminal_nearest_roots_under_same_lane(
    tmp_path: Path,
) -> None:
    source = _ReplayableSource()
    bridge = CombatSessionBridge(
        combat_roots_from_artifact=lambda payload, **_: source,
        semantic_schema=semantic_schema_fixture(),
    )
    session = CombatWinRecoverySessionFactory(
        tmp_path / "session",
        bridge,
        _config(),
    ).new_from_artifact_bytes(
        b"opaque-root",
        model_seed=7,
        source_behavior_seed=11,
        recovery_behavior_seeds=(12, 13),
    )

    assert source.calls == [(0, 2, (0,)), (0, 1, (0,))]
    assert session.discovery.wins == 1
    assert session.discovery.teacher_replicate_index == 1
    assert session.discovery.teacher_final_hp == 30
    assert tuple(
        root.transitions_to_terminal for root in session.plan.roots
    ) == (1, 2)

    result = session.advance()

    assert result.promoted
    assert result.training.signal_group_count == 2
    replay_handles = source.groups[1].recovery_handles
    assert tuple(handle.spawned_slots for handle in replay_handles[-2:]) == (
        [(0,)],
        [(0,)],
    )


def test_all_loss_source_cannot_create_a_recovery_curriculum(tmp_path: Path) -> None:
    source = _ReplayableSource(has_win=False)
    bridge = CombatSessionBridge(
        combat_roots_from_artifact=lambda payload, **_: source,
        semantic_schema=semantic_schema_fixture(),
    )

    with pytest.raises(RuntimeError, match="no verified winning replicate"):
        CombatWinRecoverySessionFactory(
            tmp_path / "session",
            bridge,
            _config(),
        ).new_from_artifact_bytes(
            b"opaque-root",
            model_seed=7,
            source_behavior_seed=11,
            recovery_behavior_seeds=(12, 13),
        )


def test_recovery_session_selects_one_root_from_a_multi_root_artifact(
    tmp_path: Path,
) -> None:
    source = _ReplayableSource(expected_slot=1)
    imported: list[int] = []

    def load_source(payload: bytes, *, expected_roots: int, **_: object):
        imported.append(expected_roots)
        return source

    bridge = CombatSessionBridge(
        combat_roots_from_artifact=load_source,
        semantic_schema=semantic_schema_fixture(),
    )
    session = CombatWinRecoverySessionFactory(
        tmp_path / "session",
        bridge,
        _config(),
        source_expected_roots=2,
        source_root_slot=1,
    ).new_from_artifact_bytes(
        b"opaque-roots",
        model_seed=7,
        source_behavior_seed=11,
        recovery_behavior_seeds=(12, 13),
    )

    assert imported == [2]
    assert source.calls == [(1, 2, (0,)), (1, 1, (0,))]
    assert session.discovery.source_artifact_root_count == 2
    assert session.discovery.source_root_slot == 1
