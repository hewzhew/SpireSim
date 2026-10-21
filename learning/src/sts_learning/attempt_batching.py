"""Strictly bounded live-only attempt batches for one optimizer update."""

from __future__ import annotations

import operator
from dataclasses import dataclass

from .attempts import (
    AttemptAssemblyDelivery,
    CompletedAttemptExperience,
    CompletedAttemptSink,
    DroppedAttemptExperience,
)
from .policy import BehaviorManifestId


class AttemptUpdateBatchError(RuntimeError):
    """A complete-attempt stream cannot form one exact on-policy update."""


@dataclass(frozen=True)
class AttemptUpdateBatchLimits:
    """Hard retained decision and payload bounds for one update batch."""

    max_decisions_per_update: int
    max_payload_bytes_per_update: int

    def __post_init__(self) -> None:
        for name in (
            "max_decisions_per_update",
            "max_payload_bytes_per_update",
        ):
            object.__setattr__(self, name, _positive_integer(getattr(self, name), name))


class BoundedAttemptUpdateBatcher:
    """Collect one exact same-behavior attempt batch before training once."""

    def __init__(
        self,
        attempts_per_update: int,
        limits: AttemptUpdateBatchLimits,
        sink: CompletedAttemptSink,
    ) -> None:
        self.attempts_per_update = _positive_integer(
            attempts_per_update,
            "attempts_per_update",
        )
        if not isinstance(limits, AttemptUpdateBatchLimits):
            raise AttemptUpdateBatchError("limits must be AttemptUpdateBatchLimits")
        if not callable(sink):
            raise AttemptUpdateBatchError("attempt update sink must be callable")
        self.limits = limits
        self._sink = sink
        self._pending: tuple[CompletedAttemptExperience, ...] = ()
        self._pending_decisions = 0
        self._pending_payload_bytes = 0
        self._last_update_payload_bytes: int | None = None
        self._pending_behavior_manifest_id: BehaviorManifestId | None = None
        self._poisoned = False

    @property
    def update_sink(self) -> CompletedAttemptSink:
        return self._sink

    @property
    def pending_attempts(self) -> int:
        return len(self._pending)

    @property
    def pending_decisions(self) -> int:
        return self._pending_decisions

    @property
    def pending_payload_bytes(self) -> int:
        return self._pending_payload_bytes

    @property
    def last_update_payload_bytes(self) -> int | None:
        """Size of the most recent successfully delivered update; diagnostic only."""
        return self._last_update_payload_bytes

    @property
    def pending_behavior_manifest_id(self) -> BehaviorManifestId | None:
        return self._pending_behavior_manifest_id

    @property
    def poisoned(self) -> bool:
        return self._poisoned

    def require_quiescent(self) -> None:
        """Reject durable publication unless this live-only owner is empty."""

        if self._poisoned:
            raise AttemptUpdateBatchError("attempt update batcher is poisoned")
        if (
            self._pending
            or self._pending_decisions != 0
            or self._pending_payload_bytes != 0
            or self._pending_behavior_manifest_id is not None
        ):
            raise AttemptUpdateBatchError(
                "attempt update batcher contains pending live-only payload"
            )

    def __call__(self, delivery: AttemptAssemblyDelivery) -> None:
        if self._poisoned:
            raise AttemptUpdateBatchError("attempt update batcher is poisoned")
        completed, dropped = _validated_delivery(delivery)
        if not completed and not dropped:
            raise AttemptUpdateBatchError("attempt update delivery is empty")

        pending_keys = {attempt.lineage.key for attempt in self._pending}
        incoming_keys = [attempt.lineage.key for attempt in completed]
        if len(set(incoming_keys)) != len(incoming_keys):
            raise AttemptUpdateBatchError("attempt update delivery repeats a lineage")
        if pending_keys.intersection(incoming_keys):
            raise AttemptUpdateBatchError(
                "attempt update batch repeats a pending lineage"
            )
        dropped_keys = [attempt.lineage.key for attempt in dropped]
        if len(set(dropped_keys)) != len(dropped_keys):
            raise AttemptUpdateBatchError(
                "attempt update delivery repeats a dropped lineage"
            )
        if set(incoming_keys).intersection(dropped_keys):
            raise AttemptUpdateBatchError("one lineage is both completed and dropped")

        incoming_manifest_id = _single_behavior_manifest_id(completed)
        next_count = len(self._pending) + len(completed)
        if next_count > self.attempts_per_update:
            raise AttemptUpdateBatchError(
                "attempt update delivery exceeds the exact attempts_per_update target"
            )
        if (
            incoming_manifest_id is not None
            and self._pending_behavior_manifest_id is not None
            and incoming_manifest_id != self._pending_behavior_manifest_id
        ):
            raise AttemptUpdateBatchError(
                "attempt update batch mixes behavior manifest identities"
            )

        next_decisions = self._pending_decisions + sum(
            attempt.decision_count for attempt in completed
        )
        next_payload_bytes = self._pending_payload_bytes + sum(
            attempt.payload_bytes for attempt in completed
        )
        if next_decisions > self.limits.max_decisions_per_update:
            raise AttemptUpdateBatchError(
                "attempt update batch exceeds max_decisions_per_update"
            )
        if next_payload_bytes > self.limits.max_payload_bytes_per_update:
            raise AttemptUpdateBatchError(
                "attempt update batch exceeds max_payload_bytes_per_update: "
                f"required={next_payload_bytes}, limit={self.limits.max_payload_bytes_per_update}, "
                f"attempts={next_count}, decisions={next_decisions}"
            )
        next_pending = self._pending + completed

        ready = next_count == self.attempts_per_update
        sink_delivery = None
        if ready:
            sink_delivery = AttemptAssemblyDelivery(
                completed=next_pending,
                dropped=dropped,
            )
        elif dropped:
            sink_delivery = AttemptAssemblyDelivery(completed=(), dropped=dropped)

        if sink_delivery is not None:
            try:
                self._sink(sink_delivery)
            except Exception:
                self._clear_pending()
                self._poisoned = True
                raise

        if ready:
            self._last_update_payload_bytes = next_payload_bytes
            self._clear_pending()
        else:
            self._pending = next_pending
            self._pending_decisions = next_decisions
            self._pending_payload_bytes = next_payload_bytes
            if incoming_manifest_id is not None:
                self._pending_behavior_manifest_id = incoming_manifest_id

    def _clear_pending(self) -> None:
        self._pending = ()
        self._pending_decisions = 0
        self._pending_payload_bytes = 0
        self._pending_behavior_manifest_id = None


def _validated_delivery(
    delivery: AttemptAssemblyDelivery,
) -> tuple[
    tuple[CompletedAttemptExperience, ...],
    tuple[DroppedAttemptExperience, ...],
]:
    if not isinstance(delivery, AttemptAssemblyDelivery):
        raise AttemptUpdateBatchError(
            "attempt update batcher requires AttemptAssemblyDelivery input"
        )
    if not all(
        isinstance(attempt, CompletedAttemptExperience)
        for attempt in delivery.completed
    ):
        raise AttemptUpdateBatchError("completed attempt delivery rows are malformed")
    if not all(
        isinstance(attempt, DroppedAttemptExperience) for attempt in delivery.dropped
    ):
        raise AttemptUpdateBatchError("dropped attempt delivery rows are malformed")
    for attempt in delivery.completed:
        if not attempt.batches:
            raise AttemptUpdateBatchError("completed attempt has no policy decisions")
        if attempt.decision_count != sum(
            batch.decision_count for batch in attempt.batches
        ):
            raise AttemptUpdateBatchError(
                "completed attempt decision count is inconsistent"
            )
        if attempt.payload_bytes != sum(
            batch.payload_bytes for batch in attempt.batches
        ):
            raise AttemptUpdateBatchError(
                "completed attempt payload bytes are inconsistent"
            )
    return delivery.completed, delivery.dropped


def _single_behavior_manifest_id(
    attempts: tuple[CompletedAttemptExperience, ...],
) -> BehaviorManifestId | None:
    manifest_ids = {
        batch.behavior_manifest_id
        for attempt in attempts
        for batch in attempt.batches
    }
    if len(manifest_ids) > 1:
        raise AttemptUpdateBatchError(
            "attempt update delivery mixes behavior manifest identities"
        )
    return next(iter(manifest_ids), None)


def _positive_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise AttemptUpdateBatchError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise AttemptUpdateBatchError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise AttemptUpdateBatchError(f"{name} must be positive")
    return normalized
