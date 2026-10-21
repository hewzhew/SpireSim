from __future__ import annotations

from collections.abc import Mapping, Sequence
from types import SimpleNamespace

import numpy as np

from learning.tests.policy_fixtures import BEHAVIOR_MANIFEST_ID
from sts_learning import (
    BatchPolicyChoice,
    DETERMINISTIC_SELECTION,
    RecoveryPlan,
    RecoverySlotSnapshot,
    TerminalAccountingBatch,
)


class FakeCheckpointBatch:
    def __init__(self, checkpoints: dict[int, tuple[int, int]]) -> None:
        self.checkpoints = dict(checkpoints)

    def __len__(self) -> int:
        return len(self.checkpoints)

    def select(self, slot_indices: list[int]) -> FakeCheckpointBatch:
        if len(set(slot_indices)) != len(slot_indices):
            raise ValueError("duplicate checkpoint selection")
        return FakeCheckpointBatch(
            {slot: self.checkpoints[slot] for slot in slot_indices}
        )

    def updated(self, replacements: FakeCheckpointBatch) -> FakeCheckpointBatch:
        if not replacements.checkpoints.keys() <= self.checkpoints.keys():
            raise ValueError("replacement slot is missing")
        updated = dict(self.checkpoints)
        updated.update(replacements.checkpoints)
        return FakeCheckpointBatch(updated)

    def checkpoint_bytes(self, *, max_bytes: int) -> bytes:
        payload = b"FAKE-BANK\x00" + b"".join(
            slot.to_bytes(8, "big")
            + seed.to_bytes(8, "big")
            + generation.to_bytes(8, "big")
            for slot, (seed, generation) in sorted(self.checkpoints.items())
        )
        if len(payload) > max_bytes:
            raise ValueError("fake checkpoint bank exceeds byte limit")
        return payload

    @classmethod
    def from_checkpoint_bytes(
        cls,
        payload: bytes,
        *,
        expected_slot_indices: list[int],
        max_bytes: int,
    ) -> FakeCheckpointBatch:
        if not isinstance(payload, bytes) or len(payload) > max_bytes:
            raise ValueError("fake checkpoint bank exceeds byte limit")
        header = b"FAKE-BANK\x00"
        if not payload.startswith(header):
            raise ValueError("fake checkpoint bank header is invalid")
        body = payload[len(header) :]
        if len(body) != 24 * len(expected_slot_indices):
            raise ValueError("fake checkpoint bank slot count differs")
        checkpoints: dict[int, tuple[int, int]] = {}
        for offset, expected_slot in enumerate(expected_slot_indices):
            row = body[offset * 24 : (offset + 1) * 24]
            slot = int.from_bytes(row[0:8], "big")
            if slot != expected_slot:
                raise ValueError("fake checkpoint bank slot identity differs")
            checkpoints[slot] = (
                int.from_bytes(row[8:16], "big"),
                int.from_bytes(row[16:24], "big"),
            )
        return cls(checkpoints)


class FakeBatchEnv:
    def __init__(
        self,
        seeds: list[int],
        *,
        terminal_plans: Sequence[Mapping[int, int]] = (),
    ) -> None:
        self.seeds = list(seeds)
        self.generations = [0] * len(seeds)
        self.terminal = [False] * len(seeds)
        self._ready = False
        self._round = 0
        self._terminal_plans = list(terminal_plans)
        self.choose_calls: list[list[int]] = []
        self.restore_calls: list[list[int]] = []
        self.reset_calls: list[tuple[list[int], list[int]]] = []

    @property
    def slot_count(self) -> int:
        return len(self.seeds)

    @property
    def terminal_count(self) -> int:
        return sum(self.terminal)

    @property
    def ready(self) -> bool:
        return self._ready

    def decision_batch(self, *, semantic: bool = False) -> dict[str, object]:
        slots = [slot for slot, terminal in enumerate(self.terminal) if not terminal]
        return {
            "slot_indices": slots,
            "candidate_counts": [2] * len(slots),
            "decision_token_ids": self._decision_token_ids(slots),
            "semantic": {"complete": semantic},
        }

    def choose(self, decision_token_ids: list[str], ordinals: list[int]) -> None:
        slots = [slot for slot, terminal in enumerate(self.terminal) if not terminal]
        expected = self._decision_token_ids(slots)
        if decision_token_ids != expected:
            raise ValueError("stale fake decision token ids")
        self.choose_calls.append(list(ordinals))
        self._round += 1
        self._ready = self._round == 2

    def _decision_token_ids(self, slots: Sequence[int]) -> list[str]:
        return [
            f"fake-token-{self.seeds[slot]}-{self.generations[slot]}-{self._round}-{slot}"
            for slot in slots
        ]

    def step(self) -> dict[str, object]:
        if not self._ready:
            raise ValueError("not ready")
        active = [slot for slot, terminal in enumerate(self.terminal) if not terminal]
        plan = self._terminal_plans.pop(0) if self._terminal_plans else {}
        terminal_slots = [slot for slot in active if slot in plan]
        rewards = [plan[slot] for slot in terminal_slots]
        for slot in terminal_slots:
            self.terminal[slot] = True
        self._ready = False
        self._round = 0
        return {
            "slot_indices": active,
            "reward": [plan.get(slot, 0) for slot in active],
            "terminated": [slot in plan for slot in active],
            "terminal_slot_indices": terminal_slots,
            "terminal_reward": rewards,
            "terminal_act": [3] * len(terminal_slots),
            "terminal_floor": [40] * len(terminal_slots),
            "terminal_hp": [20 if reward == 1 else 0 for reward in rewards],
            "terminal_max_hp": [80] * len(terminal_slots),
            "terminal_gold": [50] * len(terminal_slots),
        }

    def checkpoint_slots(self, slot_indices: list[int]) -> FakeCheckpointBatch:
        return FakeCheckpointBatch(
            {
                slot: (self.seeds[slot], self.generations[slot])
                for slot in slot_indices
            }
        )

    def checkpoint_bytes(self, *, max_bytes: int) -> bytes:
        payload = b"FAKE-ENV\x00" + b"".join(
            seed.to_bytes(8, "big") + generation.to_bytes(8, "big")
            for seed, generation in zip(self.seeds, self.generations, strict=True)
        )
        if len(payload) > max_bytes:
            raise ValueError("fake environment checkpoint exceeds byte limit")
        return payload

    @classmethod
    def from_checkpoint_bytes(
        cls,
        payload: bytes,
        *,
        expected_slots: int,
        max_bytes: int,
    ) -> FakeBatchEnv:
        if not isinstance(payload, bytes) or len(payload) > max_bytes:
            raise ValueError("fake environment checkpoint exceeds byte limit")
        header = b"FAKE-ENV\x00"
        if not payload.startswith(header):
            raise ValueError("fake environment checkpoint header is invalid")
        body = payload[len(header) :]
        if len(body) != 16 * expected_slots:
            raise ValueError("fake environment checkpoint slot count differs")
        seeds = [
            int.from_bytes(body[offset * 16 : offset * 16 + 8], "big")
            for offset in range(expected_slots)
        ]
        generations = [
            int.from_bytes(body[offset * 16 + 8 : (offset + 1) * 16], "big")
            for offset in range(expected_slots)
        ]
        restored = cls(seeds)
        restored.generations = generations
        return restored

    def restore_slots(
        self,
        slot_indices: list[int],
        checkpoints: object,
    ) -> None:
        assert isinstance(checkpoints, FakeCheckpointBatch)
        self.restore_calls.append(list(slot_indices))
        for slot in slot_indices:
            seed, generation = checkpoints.checkpoints[slot]
            self.seeds[slot] = seed
            self.generations[slot] = generation
            self.terminal[slot] = False
        self._ready = False
        self._round = 0

    def reset_slots(self, slot_indices: list[int], seeds: list[int]) -> None:
        self.reset_calls.append((list(slot_indices), list(seeds)))
        for slot, seed in zip(slot_indices, seeds, strict=True):
            if not self.terminal[slot]:
                raise ValueError("reset target is not terminal")
            self.seeds[slot] = seed
            self.generations[slot] += 1
            self.terminal[slot] = False
        self._ready = False
        self._round = 0

    def reset_slots_checkpointed(
        self,
        slot_indices: list[int],
        seeds: list[int],
    ) -> FakeCheckpointBatch:
        self.reset_slots(slot_indices, seeds)
        return self.checkpoint_slots(slot_indices)


class NumpyFakeBatchEnv(FakeBatchEnv):
    public_snapshot_phase = 0

    def decision_batch(self, *, semantic: bool = False) -> dict[str, object]:
        raw = super().decision_batch(semantic=semantic)
        slots = np.array(raw["slot_indices"], dtype=np.uint64)
        counts = np.array(raw["candidate_counts"], dtype=np.uint64)
        splits = np.zeros(len(slots) + 1, dtype=np.uint64)
        splits[1:] = np.cumsum(counts)
        return {
            "slot_indices": slots,
            "phase": np.zeros(len(slots), dtype=np.uint8),
            "candidate_counts": counts,
            "candidate_row_splits": splits,
            "decision_token_ids": list(raw["decision_token_ids"]),
            "public_snapshot_ids": [
                f"fake-public-{token_id}" for token_id in raw["decision_token_ids"]
            ],
            "decision_steps": np.full(len(slots), self._round, dtype=np.uint64),
            "semantic": {
                "schema_version": 2,
                "completeness": np.ones(len(slots), dtype=np.uint8),
                "token": {
                    "row_splits": np.arange(
                        len(slots) + 1,
                        dtype=np.uint64,
                    ),
                    "kind": np.zeros(len(slots), dtype=np.uint16),
                },
                "candidate_token_indices": np.arange(
                    int(splits[-1]),
                    dtype=np.uint64,
                ),
            },
        }

    def public_information_snapshots(self) -> list[tuple[int, SimpleNamespace]]:
        context_source = getattr(self, "public_run_contexts", None)
        if not callable(context_source):
            raise ValueError("fake environment has no public run contexts")
        contexts = {slot: view for slot, view in context_source()}
        rows = []
        for slot, terminal in enumerate(self.terminal):
            if terminal:
                continue
            view = contexts[slot]
            identity = (
                f"fake-{self.seeds[slot]}-{self.generations[slot]}-"
                f"{self._round}-{slot}-{self.public_snapshot_phase}"
            )
            rows.append(
                (
                    slot,
                    SimpleNamespace(
                        phase=self.public_snapshot_phase,
                        is_combat=view.is_combat,
                        snapshot_id=f"snapshot-{identity}",
                        observation_id=f"observation-{identity}",
                        history_snapshot_id=f"history-{identity}",
                        candidate_surface_id=f"surface-{identity}",
                        candidate_ids=(
                            f"candidate-{identity}-0",
                            f"candidate-{identity}-1",
                        ),
                    ),
                )
            )
        return rows


class NumpyWinningBatchEnv(NumpyFakeBatchEnv):
    """Deterministic terminal fixture with no hidden future plan to serialize."""

    def decision_batch(self, *, semantic: bool = False) -> dict[str, object]:
        batch = super().decision_batch(semantic=semantic)
        candidate_splits = batch["candidate_row_splits"]
        assert isinstance(candidate_splits, np.ndarray)
        semantic_batch = batch["semantic"]
        assert isinstance(semantic_batch, dict)
        token_splits = candidate_splits + np.arange(len(candidate_splits), dtype=np.uint64)
        semantic_batch["token"] = {
            "row_splits": token_splits,
            "kind": np.zeros(int(token_splits[-1]), dtype=np.uint16),
            "scope": np.concatenate([
                np.arange(int(count) + 1, dtype=np.uint64)
                for count in batch["candidate_counts"]
            ]),
        }
        semantic_batch["candidate_token_indices"] = np.concatenate([
            np.arange(int(start) + 1, int(end), dtype=np.uint64)
            for start, end in zip(token_splits[:-1], token_splits[1:], strict=True)
        ])
        semantic_batch["categorical"] = {
            "token_indices": np.array([], dtype=np.uint64),
            "field": np.array([], dtype=np.uint16),
            "value": np.array([], dtype=np.int64),
        }
        semantic_batch["scalar"] = {
            "token_indices": np.array([], dtype=np.uint64),
            "field": np.array([], dtype=np.uint16),
            "value": np.array([], dtype=np.float32),
        }
        semantic_batch["relation"] = {
            "source_token_indices": np.array([], dtype=np.uint64),
            "relation": np.array([], dtype=np.uint16),
            "target_token_indices": np.array([], dtype=np.uint64),
        }
        return batch

    def step(self) -> dict[str, object]:
        active = [slot for slot, terminal in enumerate(self.terminal) if not terminal]
        self._terminal_plans.insert(0, {slot: 1 for slot in active})
        return super().step()

    def public_run_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        return [
            (
                slot,
                SimpleNamespace(
                    boundary_kind=2 if terminal else 1,
                    is_combat=not terminal,
                    is_terminal=terminal,
                    strategic_context_kind=None,
                    seed=self.seeds[slot],
                    act=3,
                    floor=40,
                    hp=20 if terminal else 80,
                    max_hp=80,
                    gold=50,
                    potion_ids=[],
                    encounter_id=None if terminal else "JawWorm",
                    monster_ids=[] if terminal else ["JawWorm"],
                ),
            )
            for slot, terminal in enumerate(self.terminal)
        ]


class OneRejectedChoiceEnv(NumpyFakeBatchEnv):
    def __init__(self, seeds: list[int]) -> None:
        super().__init__(seeds)
        self.rejected = False

    def choose(self, decision_token_ids: list[str], ordinals: list[int]) -> None:
        if len(self.choose_calls) == 1 and not self.rejected:
            self.rejected = True
            raise RuntimeError("choice rejected")
        super().choose(decision_token_ids, ordinals)


class RecordingPolicy:
    def __init__(self) -> None:
        self.behavior_manifest_id = BEHAVIOR_MANIFEST_ID
        self.batch_sizes: list[int] = []

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        slots = decision_batch["slot_indices"]
        assert isinstance(slots, Sequence)
        self.batch_sizes.append(len(slots))
        return BatchPolicyChoice.deterministic(
            [0] * len(slots),
            BEHAVIOR_MANIFEST_ID,
        )


class FirstAttemptRecovery:
    def __init__(self) -> None:
        self.calls = 0

    def plan_recovery(
        self,
        accounting: TerminalAccountingBatch,
        snapshots: tuple[RecoverySlotSnapshot, ...],
    ) -> RecoveryPlan:
        self.calls += 1
        rewards = {
            attempt.slot_index: attempt.terminal_reward
            for attempt in accounting.attempts
        }
        return RecoveryPlan(
            tuple(
                snapshot.slot_index
                for snapshot in snapshots
                if rewards[snapshot.slot_index] == -1
                and snapshot.attempt_index == 1
            )
        )


class InvalidPolicy:
    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        return BatchPolicyChoice.deterministic(
            [2] * len(decision_batch["slot_indices"]),  # type: ignore[arg-type]
            BEHAVIOR_MANIFEST_ID,
        )


class ArrayFirstPolicy:
    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        return BatchPolicyChoice.deterministic(
            [0] * len(decision_batch["slot_indices"]),  # type: ignore[arg-type]
            BEHAVIOR_MANIFEST_ID,
        )


class UntypedPolicy:
    def choose(self, decision_batch: Mapping[str, object]) -> Sequence[int]:
        return [0] * len(decision_batch["slot_indices"])  # type: ignore[arg-type]


def _forged_choice(
    row_count: int,
    probabilities: tuple[object, ...],
) -> BatchPolicyChoice:
    choice = object.__new__(BatchPolicyChoice)
    object.__setattr__(choice, "ordinals", (0,) * row_count)
    object.__setattr__(choice, "behavior_manifest_id", BEHAVIOR_MANIFEST_ID)
    object.__setattr__(choice, "selection_probabilities", probabilities)
    return choice


class MisalignedProbabilityPolicy:
    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        row_count = len(decision_batch["slot_indices"])  # type: ignore[arg-type]
        return _forged_choice(row_count, (DETERMINISTIC_SELECTION,))


class UntypedProbabilityPolicy:
    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        row_count = len(decision_batch["slot_indices"])  # type: ignore[arg-type]
        return _forged_choice(row_count, (1.0,) * row_count)


class NoRecovery:
    def plan_recovery(
        self,
        accounting: TerminalAccountingBatch,
        snapshots: tuple[RecoverySlotSnapshot, ...],
    ) -> RecoveryPlan:
        return RecoveryPlan()
