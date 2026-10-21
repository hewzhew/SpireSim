"""Strictly bounded assembly of complete attempt-local experience."""

from __future__ import annotations

import operator
from dataclasses import dataclass
from enum import Enum
from typing import Protocol

from .experience import (
    AttemptFragment,
    AttemptKey,
    DecisionExperienceBatch,
    DecisionLineage,
    ExperienceSegment,
)
from .policy import SelectionProbability
from .recovery import TerminalAttemptRecord


class AttemptAssemblyError(ValueError):
    """An experience stream violated bounded attempt assembly."""


class AttemptDropReason(Enum):
    DECISION_LIMIT = "decision_limit"
    PAYLOAD_BYTE_LIMIT = "payload_byte_limit"
    DECISION_AND_PAYLOAD_LIMIT = "decision_and_payload_limit"


@dataclass(frozen=True)
class AttemptAssemblyLimits:
    max_open_attempts: int
    max_decisions_per_attempt: int
    max_payload_bytes_per_attempt: int

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "max_open_attempts",
            _positive_integer(self.max_open_attempts, "max_open_attempts"),
        )
        object.__setattr__(
            self,
            "max_decisions_per_attempt",
            _positive_integer(
                self.max_decisions_per_attempt,
                "max_decisions_per_attempt",
            ),
        )
        object.__setattr__(
            self,
            "max_payload_bytes_per_attempt",
            _positive_integer(
                self.max_payload_bytes_per_attempt,
                "max_payload_bytes_per_attempt",
            ),
        )

    @property
    def maximum_retained_payload_bytes(self) -> int:
        return self.max_open_attempts * self.max_payload_bytes_per_attempt

    @property
    def maximum_retained_decisions(self) -> int:
        return self.max_open_attempts * self.max_decisions_per_attempt


@dataclass(frozen=True)
class CompletedAttemptExperience:
    lineage: DecisionLineage
    batches: tuple[DecisionExperienceBatch, ...]
    terminal: TerminalAttemptRecord
    decision_count: int
    payload_bytes: int


@dataclass(frozen=True)
class DroppedAttemptExperience:
    lineage: DecisionLineage
    terminal: TerminalAttemptRecord
    reason: AttemptDropReason
    decision_count_at_drop: int
    payload_bytes_at_drop: int


@dataclass(frozen=True)
class AttemptAssemblyDelivery:
    completed: tuple[CompletedAttemptExperience, ...]
    dropped: tuple[DroppedAttemptExperience, ...]


class CompletedAttemptSink(Protocol):
    """Synchronous all-terminal delivery; the assembler keeps no queue."""

    def __call__(self, delivery: AttemptAssemblyDelivery) -> None: ...


@dataclass(frozen=True)
class AttemptAssemblerSnapshot:
    next_sequence_index: int
    open_attempts: int
    dropped_open_attempts: int
    retained_decisions: int
    retained_payload_bytes: int
    completed_attempts: int
    dropped_attempts: int


@dataclass(frozen=True)
class _OpenAttempt:
    lineage: DecisionLineage
    batches: tuple[DecisionExperienceBatch, ...] = ()
    decision_count: int = 0
    payload_bytes: int = 0
    drop_reason: AttemptDropReason | None = None
    decision_count_at_drop: int = 0
    payload_bytes_at_drop: int = 0

    @property
    def dropped(self) -> bool:
        return self.drop_reason is not None


class BoundedAttemptAssembler:
    """Consume ordered segments and deliver only complete bounded attempts."""

    def __init__(
        self,
        limits: AttemptAssemblyLimits,
        sink: CompletedAttemptSink,
        *,
        resume_snapshot: AttemptAssemblerSnapshot | None = None,
    ) -> None:
        if not isinstance(limits, AttemptAssemblyLimits):
            raise AttemptAssemblyError("limits must be AttemptAssemblyLimits")
        if not callable(sink):
            raise AttemptAssemblyError("completed attempt sink must be callable")
        self.limits = limits
        self._sink = sink
        if resume_snapshot is None:
            next_sequence_index = 0
            completed_attempts = 0
            dropped_attempts = 0
        else:
            if not isinstance(resume_snapshot, AttemptAssemblerSnapshot):
                raise AttemptAssemblyError("resume snapshot must be typed")
            if (
                resume_snapshot.open_attempts != 0
                or resume_snapshot.dropped_open_attempts != 0
                or resume_snapshot.retained_decisions != 0
                or resume_snapshot.retained_payload_bytes != 0
            ):
                raise AttemptAssemblyError(
                    "resume snapshot contains open attempt state"
                )
            next_sequence_index = _nonnegative_integer(
                resume_snapshot.next_sequence_index,
                "resume next_sequence_index",
            )
            completed_attempts = _nonnegative_integer(
                resume_snapshot.completed_attempts,
                "resume completed_attempts",
            )
            dropped_attempts = _nonnegative_integer(
                resume_snapshot.dropped_attempts,
                "resume dropped_attempts",
            )
        self._next_sequence_index = next_sequence_index
        self._open: dict[AttemptKey, _OpenAttempt] = {}
        self._completed_attempts = completed_attempts
        self._dropped_attempts = dropped_attempts

    @property
    def completed_attempt_sink(self) -> CompletedAttemptSink:
        """Return the exact synchronous owner wired after attempt assembly."""

        return self._sink

    @property
    def snapshot(self) -> AttemptAssemblerSnapshot:
        retained = tuple(state for state in self._open.values() if not state.dropped)
        return AttemptAssemblerSnapshot(
            next_sequence_index=self._next_sequence_index,
            open_attempts=len(self._open),
            dropped_open_attempts=sum(state.dropped for state in self._open.values()),
            retained_decisions=sum(state.decision_count for state in retained),
            retained_payload_bytes=sum(state.payload_bytes for state in retained),
            completed_attempts=self._completed_attempts,
            dropped_attempts=self._dropped_attempts,
        )

    def __call__(self, segment: ExperienceSegment) -> None:
        if not isinstance(segment, ExperienceSegment):
            raise AttemptAssemblyError("attempt assembler requires ExperienceSegment input")
        if segment.sequence_index != self._next_sequence_index:
            raise AttemptAssemblyError(
                f"experience segment sequence {segment.sequence_index} does not match "
                f"expected {self._next_sequence_index}"
            )

        fragments = _validate_segment(segment)
        decision_keys = {lineage.key for batch in segment.batches for lineage in batch.lineages}
        for key in set(fragments) - decision_keys:
            if key not in self._open or fragments[key].terminal is None:
                raise AttemptAssemblyError("terminal continuation has no previously sampled open attempt")
        tentative = dict(self._open)
        new_keys = set(fragments) - set(tentative)
        terminal_keys = {
            key for key, fragment in fragments.items() if fragment.terminal is not None
        }
        remaining_keys = (set(tentative) | new_keys) - terminal_keys
        if len(remaining_keys) > self.limits.max_open_attempts:
            raise AttemptAssemblyError(
                "experience stream exceeds max_open_attempts after terminal closure"
            )
        for key in new_keys:
            tentative[key] = _OpenAttempt(lineage=fragments[key].lineage)
        for key, fragment in fragments.items():
            if tentative[key].lineage != fragment.lineage:
                raise AttemptAssemblyError(
                    f"attempt lineage changed while {key} remained open"
                )

        for batch in segment.batches:
            rows_by_key = _batch_rows_by_attempt(batch)
            for key, rows in rows_by_key.items():
                state = tentative[key]
                if state.dropped:
                    continue
                selected = batch.select_rows(rows)
                decision_count = state.decision_count + selected.decision_count
                payload_bytes = state.payload_bytes + selected.payload_bytes
                reason = _drop_reason(
                    decision_count > self.limits.max_decisions_per_attempt,
                    payload_bytes > self.limits.max_payload_bytes_per_attempt,
                )
                if reason is None:
                    tentative[key] = _OpenAttempt(
                        lineage=state.lineage,
                        batches=state.batches + (selected,),
                        decision_count=decision_count,
                        payload_bytes=payload_bytes,
                    )
                else:
                    tentative[key] = _OpenAttempt(
                        lineage=state.lineage,
                        drop_reason=reason,
                        decision_count_at_drop=decision_count,
                        payload_bytes_at_drop=payload_bytes,
                    )

        completed: list[CompletedAttemptExperience] = []
        dropped: list[DroppedAttemptExperience] = []
        for key, fragment in fragments.items():
            if fragment.terminal is None:
                continue
            state = tentative.pop(key)
            if state.drop_reason is None:
                completed.append(
                    CompletedAttemptExperience(
                        lineage=state.lineage,
                        batches=state.batches,
                        terminal=fragment.terminal,
                        decision_count=state.decision_count,
                        payload_bytes=state.payload_bytes,
                    )
                )
            else:
                dropped.append(
                    DroppedAttemptExperience(
                        lineage=state.lineage,
                        terminal=fragment.terminal,
                        reason=state.drop_reason,
                        decision_count_at_drop=state.decision_count_at_drop,
                        payload_bytes_at_drop=state.payload_bytes_at_drop,
                    )
                )

        delivery = AttemptAssemblyDelivery(
            completed=tuple(completed),
            dropped=tuple(dropped),
        )
        if delivery.completed or delivery.dropped:
            self._sink(delivery)

        self._open = tentative
        self._next_sequence_index += 1
        self._completed_attempts += len(completed)
        self._dropped_attempts += len(dropped)


def _validate_segment(segment: ExperienceSegment) -> dict[AttemptKey, AttemptFragment]:
    if segment.decision_count != sum(batch.decision_count for batch in segment.batches):
        raise AttemptAssemblyError("segment decision_count disagrees with its batches")
    if segment.payload_bytes != sum(batch.payload_bytes for batch in segment.batches):
        raise AttemptAssemblyError("segment payload_bytes disagree with its batches")

    fragments: dict[AttemptKey, AttemptFragment] = {}
    for fragment in segment.attempts:
        if not isinstance(fragment, AttemptFragment):
            raise AttemptAssemblyError("segment attempts must be AttemptFragment values")
        key = fragment.lineage.key
        if key in fragments:
            raise AttemptAssemblyError("segment repeats an attempt fragment")
        if fragment.terminal is not None:
            terminal_key = _terminal_key(fragment.terminal)
            if terminal_key != key:
                raise AttemptAssemblyError("terminal record does not match attempt lineage")
            if fragment.terminal.recoveries_used != fragment.lineage.recoveries_used:
                raise AttemptAssemblyError(
                    "terminal recovery count does not match attempt lineage"
                )
        fragments[key] = fragment

    batch_keys: set[AttemptKey] = set()
    for batch in segment.batches:
        if not isinstance(batch, DecisionExperienceBatch):
            raise AttemptAssemblyError(
                "segment batches must be DecisionExperienceBatch values"
            )
        if batch.decision_count != len(batch.lineages):
            raise AttemptAssemblyError("decision batch lineage rows are misaligned")
        if batch.decision_count != len(batch.selected_ordinals):
            raise AttemptAssemblyError("decision batch ordinal rows are misaligned")
        if batch.decision_count != len(batch.selection_probabilities):
            raise AttemptAssemblyError(
                "decision batch selection probability rows are misaligned"
            )
        if not all(
            isinstance(probability, SelectionProbability)
            for probability in batch.selection_probabilities
        ):
            raise AttemptAssemblyError(
                "decision batch selection probabilities must be typed"
            )
        batch_keys.update(lineage.key for lineage in batch.lineages)
    if not batch_keys <= set(fragments):
        raise AttemptAssemblyError(
            "segment attempts do not exactly cover its decision batch lineages"
        )
    return fragments


def _batch_rows_by_attempt(
    batch: DecisionExperienceBatch,
) -> dict[AttemptKey, tuple[int, ...]]:
    grouped: dict[AttemptKey, list[int]] = {}
    for row, lineage in enumerate(batch.lineages):
        grouped.setdefault(lineage.key, []).append(row)
    return {key: tuple(rows) for key, rows in grouped.items()}


def _terminal_key(record: TerminalAttemptRecord) -> AttemptKey:
    return AttemptKey(
        slot_index=record.slot_index,
        episode_seed=record.episode_seed,
        episode_generation=record.episode_generation,
        attempt_index=record.attempt_index,
    )


def _drop_reason(
    decision_overflow: bool,
    payload_overflow: bool,
) -> AttemptDropReason | None:
    if decision_overflow and payload_overflow:
        return AttemptDropReason.DECISION_AND_PAYLOAD_LIMIT
    if decision_overflow:
        return AttemptDropReason.DECISION_LIMIT
    if payload_overflow:
        return AttemptDropReason.PAYLOAD_BYTE_LIMIT
    return None


def _positive_integer(value: object, name: str) -> int:
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise AttemptAssemblyError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise AttemptAssemblyError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise AttemptAssemblyError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise AttemptAssemblyError(f"{name} must be an integer") from error
    if normalized < 0:
        raise AttemptAssemblyError(f"{name} must be non-negative")
    return normalized
