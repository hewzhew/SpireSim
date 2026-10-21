"""Deterministic seed partitioning and immutable batch reset plans."""

from __future__ import annotations

import hashlib
import operator
from dataclasses import dataclass, replace
from enum import Enum
from typing import Protocol, Sequence

from .recovery import (
    EpisodeResetTarget,
    RecoveryLedger,
    RecoveryMode,
    reset_with_accounting,
)


_U64_LIMIT = 1 << 64
_PARTITION_PERSON = b"sts-seed-v1"


class SeedScheduleError(ValueError):
    """A seed partition or schedule request is invalid."""


class SeedPartition(Enum):
    TRAINING = "training"
    HELD_OUT = "held_out"


class CheckpointedEpisodeResetTarget(Protocol):
    def reset_slots_checkpointed(
        self,
        slot_indices: list[int],
        seeds: list[int],
    ) -> object: ...


@dataclass(frozen=True)
class SeedPartitionSpec:
    """Stable seed-only split applied before any derived episode attempts."""

    held_out_numerator: int = 1
    denominator: int = 10

    def __post_init__(self) -> None:
        numerator = operator.index(self.held_out_numerator)
        denominator = operator.index(self.denominator)
        if not 0 < denominator <= _U64_LIMIT:
            raise SeedScheduleError("denominator must be in 1..2^64")
        if not 0 <= numerator <= denominator:
            raise SeedScheduleError(
                "held_out_numerator must be in 0..denominator"
            )
        object.__setattr__(self, "held_out_numerator", numerator)
        object.__setattr__(self, "denominator", denominator)

    def classify(self, seed: int) -> SeedPartition:
        normalized = _normalize_seed(seed)
        digest = hashlib.blake2b(
            normalized.to_bytes(8, "little"),
            digest_size=8,
            person=_PARTITION_PERSON,
        ).digest()
        bucket = int.from_bytes(digest, "little") % self.denominator
        if bucket < self.held_out_numerator:
            return SeedPartition.HELD_OUT
        return SeedPartition.TRAINING


@dataclass(frozen=True)
class SeedResetBatch:
    slot_indices: tuple[int, ...]
    seeds: tuple[int, ...]
    partition: SeedPartition


@dataclass(frozen=True)
class SeedSchedule:
    """An immutable cursor over seeds belonging to exactly one partition."""

    partition: SeedPartition
    spec: SeedPartitionSpec = SeedPartitionSpec()
    next_candidate: int = 0

    def __post_init__(self) -> None:
        if not isinstance(self.partition, SeedPartition):
            raise SeedScheduleError("partition must be a SeedPartition")
        if not isinstance(self.spec, SeedPartitionSpec):
            raise SeedScheduleError("spec must be a SeedPartitionSpec")
        candidate = operator.index(self.next_candidate)
        if not 0 <= candidate <= _U64_LIMIT:
            raise SeedScheduleError("next_candidate must be in 0..2^64")
        if (
            self.partition is SeedPartition.HELD_OUT
            and self.spec.held_out_numerator == 0
        ):
            raise SeedScheduleError("held-out partition is empty")
        if (
            self.partition is SeedPartition.TRAINING
            and self.spec.held_out_numerator == self.spec.denominator
        ):
            raise SeedScheduleError("training partition is empty")
        object.__setattr__(self, "next_candidate", candidate)

    def plan(
        self, slot_indices: Sequence[int]
    ) -> tuple[SeedResetBatch, SeedSchedule]:
        slots = _normalize_slots(slot_indices)
        seeds: list[int] = []
        candidate = self.next_candidate
        while len(seeds) < len(slots):
            if candidate == _U64_LIMIT:
                raise SeedScheduleError("seed schedule is exhausted")
            if self.spec.classify(candidate) is self.partition:
                seeds.append(candidate)
            candidate += 1
        batch = SeedResetBatch(
            slot_indices=slots,
            seeds=tuple(seeds),
            partition=self.partition,
        )
        return batch, replace(self, next_candidate=candidate)


def reset_scheduled_with_accounting(
    env: EpisodeResetTarget,
    slot_indices: Sequence[int],
    ledger: RecoveryLedger,
    schedule: SeedSchedule,
) -> tuple[SeedResetBatch, SeedSchedule]:
    """Atomically reset one planned batch and return the advanced schedule."""

    _validate_ledger_partition(ledger, schedule)
    batch, next_schedule = schedule.plan(slot_indices)
    reset_with_accounting(
        env,
        batch.slot_indices,
        batch.seeds,
        ledger,
    )
    return batch, next_schedule


def reset_scheduled_checkpointed_with_accounting(
    env: CheckpointedEpisodeResetTarget,
    slot_indices: Sequence[int],
    ledger: RecoveryLedger,
    schedule: SeedSchedule,
) -> tuple[SeedResetBatch, SeedSchedule, object]:
    """Atomically reset slots and return their exact new root checkpoints."""

    _validate_ledger_partition(ledger, schedule)
    batch, next_schedule = schedule.plan(slot_indices)
    ticket = ledger.prepare_reset(batch.slot_indices, batch.seeds)
    checkpoints = env.reset_slots_checkpointed(
        list(ticket.slot_indices),
        list(ticket.new_seeds),
    )
    ledger.commit_reset(ticket)
    return batch, next_schedule, checkpoints


def _validate_ledger_partition(
    ledger: RecoveryLedger,
    schedule: SeedSchedule,
) -> None:
    expected_partition = (
        SeedPartition.TRAINING
        if ledger.mode is RecoveryMode.TRAINING
        else SeedPartition.HELD_OUT
    )
    if schedule.partition is not expected_partition:
        raise SeedScheduleError(
            f"{ledger.mode.value} ledger requires {expected_partition.value} seeds"
        )


def _normalize_seed(seed: int) -> int:
    normalized = operator.index(seed)
    if not 0 <= normalized < _U64_LIMIT:
        raise SeedScheduleError("seed must be in 0..2^64-1")
    return normalized


def _normalize_slots(slot_indices: Sequence[int]) -> tuple[int, ...]:
    slots = tuple(operator.index(slot) for slot in slot_indices)
    if len(set(slots)) != len(slots):
        raise SeedScheduleError("slot batch contains duplicate indices")
    if any(slot < 0 for slot in slots):
        raise SeedScheduleError("slot indices must be non-negative")
    return slots
