"""Bounded semantic experience segments with exact attempt lineage."""

from __future__ import annotations

import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from enum import Enum

from .decision_rows import (
    DecisionRowError,
    PreparedDecisionRows,
    iter_payload_arrays,
    normalize_decision_choice,
    normalize_integer_sequence,
)
from .decision_progress import DecisionRunProgress
from .policy import BehaviorManifestId, SelectionProbability
from .recovery import (
    RecoverySlotSnapshot,
    RecoverySlotStatus,
    TerminalAttemptRecord,
)
from .semantic_batch import SemanticBatchError, select_semantic_decision_rows


class ExperienceError(ValueError):
    """An experience batch or segment transition is invalid."""


class SegmentCloseReason(Enum):
    EXPLICIT_FLUSH = "explicit_flush"
    DECISION_LIMIT = "decision_limit"
    PAYLOAD_BYTE_LIMIT = "payload_byte_limit"
    DECISION_AND_PAYLOAD_LIMIT = "decision_and_payload_limit"


@dataclass(frozen=True, order=True)
class AttemptKey:
    slot_index: int
    episode_seed: int
    episode_generation: int
    attempt_index: int


@dataclass(frozen=True)
class DecisionLineage:
    key: AttemptKey
    recoveries_used: int

    @classmethod
    def from_snapshot(cls, snapshot: RecoverySlotSnapshot) -> DecisionLineage:
        if not isinstance(snapshot, RecoverySlotSnapshot):
            raise ExperienceError("lineage must come from a RecoverySlotSnapshot")
        if snapshot.status is not RecoverySlotStatus.ACTIVE:
            raise ExperienceError("decision lineage requires an active slot snapshot")
        if snapshot.pending_terminal is not None:
            raise ExperienceError("active decision lineage cannot have a pending terminal")
        slot_index = _nonnegative_integer(snapshot.slot_index, "slot_index")
        episode_seed = _bounded_seed(snapshot.episode_seed)
        episode_generation = _nonnegative_integer(
            snapshot.episode_generation,
            "episode_generation",
        )
        attempt_index = _positive_integer(snapshot.attempt_index, "attempt_index")
        recoveries_used = _nonnegative_integer(
            snapshot.recoveries_used,
            "recoveries_used",
        )
        if attempt_index != recoveries_used + 1:
            raise ExperienceError("attempt index must equal recoveries used plus one")
        return cls(
            key=AttemptKey(
                slot_index=slot_index,
                episode_seed=episode_seed,
                episode_generation=episode_generation,
                attempt_index=attempt_index,
            ),
            recoveries_used=recoveries_used,
        )


@dataclass(frozen=True)
class ExperienceLimits:
    max_decisions: int
    max_payload_bytes: int

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "max_decisions",
            _positive_integer(self.max_decisions, "max_decisions"),
        )
        object.__setattr__(
            self,
            "max_payload_bytes",
            _positive_integer(self.max_payload_bytes, "max_payload_bytes"),
        )


@dataclass(frozen=True)
class PreparedDecisionBatch:
    """Policy-independent frozen copy prepared before model inference."""

    payload: Mapping[str, object]
    lineages: tuple[DecisionLineage, ...]
    candidate_counts: tuple[int, ...]
    decision_count: int
    payload_bytes: int
    run_progress: tuple[DecisionRunProgress, ...] | None = None

    @classmethod
    def capture(
        cls,
        decision_batch: Mapping[str, object],
        snapshots: Sequence[RecoverySlotSnapshot],
        run_progress: Sequence[DecisionRunProgress] | None = None,
    ) -> PreparedDecisionBatch:
        try:
            rows = PreparedDecisionRows.capture(decision_batch)
        except DecisionRowError as error:
            raise ExperienceError(str(error)) from error
        normalized_snapshots = tuple(snapshots)
        if len(normalized_snapshots) != rows.decision_count:
            raise ExperienceError(
                f"received {len(normalized_snapshots)} snapshots for "
                f"{rows.decision_count} rows"
            )
        lineages = tuple(
            DecisionLineage.from_snapshot(snapshot)
            for snapshot in normalized_snapshots
        )
        for slot, lineage in zip(rows.slot_indices, lineages, strict=True):
            if lineage.key.slot_index != slot:
                raise ExperienceError(
                    f"decision slot {slot} is aligned to snapshot slot "
                    f"{lineage.key.slot_index}"
                )
        normalized_progress = None
        if run_progress is not None:
            normalized_progress = tuple(run_progress)
            if len(normalized_progress) != rows.decision_count:
                raise ExperienceError(
                    f"received {len(normalized_progress)} progress rows for "
                    f"{rows.decision_count} decisions"
                )
            if not all(
                isinstance(progress, DecisionRunProgress)
                for progress in normalized_progress
            ):
                raise ExperienceError(
                    "decision progress rows must be DecisionRunProgress values"
                )
            phases: tuple[int, ...] | None = None
            for row_index, (progress, lineage, candidate_count) in enumerate(
                zip(
                    normalized_progress,
                    lineages,
                    rows.candidate_counts,
                    strict=True,
                )
            ):
                if progress.episode_seed != lineage.key.episode_seed:
                    raise ExperienceError(
                        "decision progress seed does not match attempt lineage"
                    )
                public_snapshot = progress.public_snapshot
                if public_snapshot is None:
                    continue
                if phases is None:
                    try:
                        phases = normalize_integer_sequence(
                            rows.payload["phase"],
                            "phase",
                        )
                    except (KeyError, DecisionRowError) as error:
                        raise ExperienceError(
                            "decision payload phase column is malformed"
                        ) from error
                    if len(phases) != rows.decision_count:
                        raise ExperienceError(
                            "decision payload phase rows are misaligned"
                        )
                if public_snapshot.phase != phases[row_index]:
                    raise ExperienceError(
                        "public decision snapshot phase disagrees with decision payload"
                    )
                if len(public_snapshot.candidate_ids) != candidate_count:
                    raise ExperienceError(
                        "public decision snapshot candidates disagree with decision payload"
                    )
        return cls(
            payload=rows.payload,
            lineages=lineages,
            candidate_counts=rows.candidate_counts,
            decision_count=rows.decision_count,
            payload_bytes=rows.payload_bytes,
            run_progress=normalized_progress,
        )


@dataclass(frozen=True)
class DecisionExperienceBatch:
    payload: Mapping[str, object]
    lineages: tuple[DecisionLineage, ...]
    selected_ordinals: tuple[int, ...]
    selection_probabilities: tuple[SelectionProbability, ...]
    behavior_manifest_id: BehaviorManifestId
    decision_count: int
    payload_bytes: int
    run_progress: tuple[DecisionRunProgress, ...] | None = None

    @classmethod
    def from_prepared(
        cls,
        prepared: PreparedDecisionBatch,
        selected_ordinals: Sequence[int],
        selection_probabilities: Sequence[SelectionProbability],
        behavior_manifest_id: BehaviorManifestId,
    ) -> DecisionExperienceBatch:
        if not isinstance(prepared, PreparedDecisionBatch):
            raise ExperienceError("experience input must be a PreparedDecisionBatch")
        try:
            rows = PreparedDecisionRows(
                payload=prepared.payload,
                slot_indices=tuple(
                    lineage.key.slot_index for lineage in prepared.lineages
                ),
                candidate_counts=prepared.candidate_counts,
                decision_count=prepared.decision_count,
                payload_bytes=prepared.payload_bytes,
            )
            ordinals, probabilities = normalize_decision_choice(
                rows,
                selected_ordinals,
                selection_probabilities,
                behavior_manifest_id,
            )
        except DecisionRowError as error:
            raise ExperienceError(str(error)) from error
        return cls(
            payload=prepared.payload,
            lineages=prepared.lineages,
            selected_ordinals=ordinals,
            selection_probabilities=probabilities,
            behavior_manifest_id=behavior_manifest_id,
            decision_count=prepared.decision_count,
            payload_bytes=prepared.payload_bytes,
            run_progress=prepared.run_progress,
        )

    def select_rows(self, row_indices: Sequence[int]) -> DecisionExperienceBatch:
        """Own and freeze an exact row subset for attempt-local retention."""

        rows = normalize_integer_sequence(row_indices, "row indices")
        try:
            selected = select_semantic_decision_rows(self.payload, rows)
        except SemanticBatchError as error:
            raise ExperienceError("cannot select semantic decision rows") from error
        try:
            selected_rows = PreparedDecisionRows.capture(selected)
        except DecisionRowError as error:
            raise ExperienceError("cannot freeze selected decision rows") from error
        return DecisionExperienceBatch(
            payload=selected_rows.payload,
            lineages=tuple(self.lineages[row] for row in rows),
            selected_ordinals=tuple(self.selected_ordinals[row] for row in rows),
            selection_probabilities=tuple(
                self.selection_probabilities[row] for row in rows
            ),
            behavior_manifest_id=self.behavior_manifest_id,
            decision_count=len(rows),
            payload_bytes=selected_rows.payload_bytes,
            run_progress=(
                None
                if self.run_progress is None
                else tuple(self.run_progress[row] for row in rows)
            ),
        )


@dataclass(frozen=True)
class AttemptFragment:
    lineage: DecisionLineage
    terminal: TerminalAttemptRecord | None

    @property
    def censored(self) -> bool:
        return self.terminal is None


@dataclass(frozen=True)
class ExperienceSegment:
    sequence_index: int
    close_reason: SegmentCloseReason
    batches: tuple[DecisionExperienceBatch, ...]
    attempts: tuple[AttemptFragment, ...]
    decision_count: int
    payload_bytes: int

    @property
    def censored(self) -> bool:
        return any(attempt.censored for attempt in self.attempts)


class ExperienceSegmentBuffer:
    """One bounded mutable segment; sealed segments are caller-consumed."""

    def __init__(
        self,
        limits: ExperienceLimits,
        *,
        next_sequence_index: int = 0,
    ) -> None:
        if not isinstance(limits, ExperienceLimits):
            raise ExperienceError("limits must be ExperienceLimits")
        self.limits = limits
        self._next_sequence_index = _nonnegative_integer(
            next_sequence_index,
            "next_sequence_index",
        )
        self._batches: list[DecisionExperienceBatch] = []
        self._decision_count = 0
        self._payload_bytes = 0
        self._lineages: dict[AttemptKey, DecisionLineage] = {}
        self._terminals: dict[AttemptKey, TerminalAttemptRecord] = {}

    @property
    def decision_count(self) -> int:
        return self._decision_count

    @property
    def payload_bytes(self) -> int:
        return self._payload_bytes

    @property
    def empty(self) -> bool:
        return not self._lineages

    @property
    def next_sequence_index(self) -> int:
        """Return the next durable segment identity without exposing payloads."""

        return self._next_sequence_index

    def prepare(
        self,
        decision_batch: Mapping[str, object],
        snapshots: Sequence[RecoverySlotSnapshot],
        run_progress: Sequence[DecisionRunProgress] | None = None,
    ) -> PreparedDecisionBatch:
        prepared = PreparedDecisionBatch.capture(
            decision_batch,
            snapshots,
            run_progress,
        )
        if prepared.decision_count > self.limits.max_decisions:
            raise ExperienceError(
                f"one batch has {prepared.decision_count} decisions, exceeding "
                f"segment limit {self.limits.max_decisions}"
            )
        if prepared.payload_bytes > self.limits.max_payload_bytes:
            raise ExperienceError(
                f"one batch has {prepared.payload_bytes} payload bytes, exceeding "
                f"segment limit {self.limits.max_payload_bytes}"
            )
        return prepared

    def record(
        self,
        prepared: PreparedDecisionBatch,
        selected_ordinals: Sequence[int],
        selection_probabilities: Sequence[SelectionProbability],
        behavior_manifest_id: BehaviorManifestId,
    ) -> tuple[ExperienceSegment, ...]:
        batch = DecisionExperienceBatch.from_prepared(
            prepared,
            selected_ordinals,
            selection_probabilities,
            behavior_manifest_id,
        )
        emitted = self.rotate_before(batch)
        self.commit(batch)
        return emitted

    def rotate_before(
        self,
        batch: DecisionExperienceBatch,
    ) -> tuple[ExperienceSegment, ...]:
        """Seal the old segment, if needed, without admitting this batch."""

        self._validate_batch(batch)
        decision_overflow, payload_overflow = self._would_overflow(batch)
        if self._batches and (decision_overflow or payload_overflow):
            return (
                self._seal(_limit_reason(decision_overflow, payload_overflow)),
            )
        return ()

    def commit(self, batch: DecisionExperienceBatch) -> None:
        """Admit one already-applied choice after any required rotation."""

        self._validate_batch(batch)
        decision_overflow, payload_overflow = self._would_overflow(batch)
        if decision_overflow or payload_overflow:
            raise ExperienceError("experience batch requires rotation before commit")
        self._append(batch)

    def _validate_batch(self, batch: DecisionExperienceBatch) -> None:
        if not isinstance(batch, DecisionExperienceBatch):
            raise ExperienceError("experience input must be DecisionExperienceBatch")
        if not isinstance(batch.behavior_manifest_id, BehaviorManifestId):
            raise ExperienceError("experience batch has no behavior manifest identity")
        if batch.decision_count != len(batch.lineages):
            raise ExperienceError("experience batch lineage rows are misaligned")
        if batch.decision_count != len(batch.selected_ordinals):
            raise ExperienceError("experience batch ordinal rows are misaligned")
        if batch.decision_count != len(batch.selection_probabilities):
            raise ExperienceError(
                "experience batch selection probability rows are misaligned"
            )
        if (
            batch.run_progress is not None
            and batch.decision_count != len(batch.run_progress)
        ):
            raise ExperienceError("experience batch progress rows are misaligned")
        if batch.run_progress is not None and not all(
            isinstance(progress, DecisionRunProgress)
            for progress in batch.run_progress
        ):
            raise ExperienceError(
                "experience batch progress rows must be DecisionRunProgress values"
            )
        if not all(
            isinstance(probability, SelectionProbability)
            for probability in batch.selection_probabilities
        ):
            raise ExperienceError(
                "experience batch selection probabilities must be typed"
            )
        if batch.decision_count > self.limits.max_decisions:
            raise ExperienceError("prepared batch exceeds the decision limit")
        if batch.payload_bytes > self.limits.max_payload_bytes:
            raise ExperienceError("prepared batch exceeds the payload byte limit")

    def _would_overflow(
        self,
        batch: DecisionExperienceBatch,
    ) -> tuple[bool, bool]:
        return (
            self._decision_count + batch.decision_count
            > self.limits.max_decisions,
            self._payload_bytes + batch.payload_bytes
            > self.limits.max_payload_bytes,
        )

    def record_terminals(
        self,
        attempts: Sequence[TerminalAttemptRecord],
        *,
        continuation_lineages: Sequence[DecisionLineage] = (),
    ) -> None:
        records = tuple(attempts)
        if not all(isinstance(record, TerminalAttemptRecord) for record in records):
            raise ExperienceError(
                "terminal experience must contain TerminalAttemptRecord values"
            )
        prepared: list[tuple[AttemptKey, TerminalAttemptRecord]] = []
        continuations = {}
        for lineage in continuation_lineages:
            if not isinstance(lineage, DecisionLineage) or lineage.key in continuations:
                raise ExperienceError("terminal continuations require distinct typed lineages")
            continuations[lineage.key] = lineage
        seen = set()
        for record in records:
            key = _terminal_key(record)
            if key in seen:
                raise ExperienceError("terminal batch contains duplicate attempt lineage")
            seen.add(key)
            lineage = self._lineages.get(key, continuations.get(key))
            if lineage is None:
                raise ExperienceError("terminal attempt is absent from the open segment")
            if lineage.recoveries_used != record.recoveries_used:
                raise ExperienceError("terminal recovery count does not match its decisions")
            if key in self._terminals:
                raise ExperienceError("terminal attempt was already recorded")
            prepared.append((key, record))
        for key, _ in prepared:
            self._lineages.setdefault(key, continuations.get(key))
        self._terminals.update(prepared)

    def flush(self) -> ExperienceSegment | None:
        if not self._lineages:
            return None
        return self._seal(SegmentCloseReason.EXPLICIT_FLUSH)

    def _append(self, batch: DecisionExperienceBatch) -> None:
        for lineage in batch.lineages:
            previous = self._lineages.get(lineage.key)
            if previous is not None and previous != lineage:
                raise ExperienceError("attempt lineage changed within one segment")
        self._batches.append(batch)
        self._decision_count += batch.decision_count
        self._payload_bytes += batch.payload_bytes
        for lineage in batch.lineages:
            self._lineages.setdefault(lineage.key, lineage)
        if self._decision_count > self.limits.max_decisions:
            raise AssertionError("experience decision bound was exceeded")
        if self._payload_bytes > self.limits.max_payload_bytes:
            raise AssertionError("experience payload byte bound was exceeded")

    def _seal(self, reason: SegmentCloseReason) -> ExperienceSegment:
        if not self._lineages:
            raise ExperienceError("cannot seal an empty experience segment")
        attempts = tuple(
            AttemptFragment(
                lineage=lineage,
                terminal=self._terminals.get(key),
            )
            for key, lineage in self._lineages.items()
        )
        segment = ExperienceSegment(
            sequence_index=self._next_sequence_index,
            close_reason=reason,
            batches=tuple(self._batches),
            attempts=attempts,
            decision_count=self._decision_count,
            payload_bytes=self._payload_bytes,
        )
        self._next_sequence_index += 1
        self._batches = []
        self._decision_count = 0
        self._payload_bytes = 0
        self._lineages = {}
        self._terminals = {}
        return segment


def _terminal_key(record: TerminalAttemptRecord) -> AttemptKey:
    return AttemptKey(
        slot_index=_nonnegative_integer(record.slot_index, "terminal slot_index"),
        episode_seed=_bounded_seed(record.episode_seed),
        episode_generation=_nonnegative_integer(
            record.episode_generation,
            "terminal episode_generation",
        ),
        attempt_index=_positive_integer(
            record.attempt_index,
            "terminal attempt_index",
        ),
    )


def _limit_reason(
    decision_overflow: bool,
    payload_overflow: bool,
) -> SegmentCloseReason:
    if decision_overflow and payload_overflow:
        return SegmentCloseReason.DECISION_AND_PAYLOAD_LIMIT
    if decision_overflow:
        return SegmentCloseReason.DECISION_LIMIT
    if payload_overflow:
        return SegmentCloseReason.PAYLOAD_BYTE_LIMIT
    raise AssertionError("segment limit reason requires an exceeded bound")


def _positive_integer(value: int, name: str) -> int:
    if isinstance(value, bool):
        raise ExperienceError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise ExperienceError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise ExperienceError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: int, name: str) -> int:
    if isinstance(value, bool):
        raise ExperienceError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise ExperienceError(f"{name} must be an integer") from error
    if normalized < 0:
        raise ExperienceError(f"{name} must be non-negative")
    return normalized


def _bounded_seed(value: int) -> int:
    seed = _nonnegative_integer(value, "episode_seed")
    if seed >= 1 << 64:
        raise ExperienceError("episode_seed must be in 0..2^64-1")
    return seed
