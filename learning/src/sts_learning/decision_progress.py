"""Typed run progress captured at one policy decision boundary."""

from __future__ import annotations

import operator
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Protocol


class DecisionProgressError(ValueError):
    """A decision-time public run context is malformed or misaligned."""


@dataclass(frozen=True)
class PublicDecisionSnapshot:
    """Sanitized Rust-owned identity aligned to one model decision row."""

    phase: int
    is_combat: bool
    snapshot_id: str
    observation_id: str
    history_snapshot_id: str
    candidate_surface_id: str
    candidate_ids: tuple[str, ...]

    def __post_init__(self) -> None:
        object.__setattr__(self, "phase", _integer(self.phase, "phase", minimum=0))
        if type(self.is_combat) is not bool:
            raise DecisionProgressError("public decision is_combat must be bool")
        for name in (
            "snapshot_id",
            "observation_id",
            "history_snapshot_id",
            "candidate_surface_id",
        ):
            object.__setattr__(self, name, _nonempty_text(getattr(self, name), name))
        try:
            candidate_ids = tuple(self.candidate_ids)
        except TypeError as error:
            raise DecisionProgressError(
                "public decision candidate_ids must be a sequence"
            ) from error
        if not candidate_ids:
            raise DecisionProgressError(
                "public decision candidate surface must not be empty"
            )
        if not all(isinstance(value, str) and value.strip() for value in candidate_ids):
            raise DecisionProgressError(
                "public decision candidate ids must be non-empty text"
            )
        if len(set(candidate_ids)) != len(candidate_ids):
            raise DecisionProgressError("public decision candidate ids repeat")
        object.__setattr__(self, "candidate_ids", candidate_ids)


@dataclass(frozen=True)
class DecisionRunProgress:
    """Minimal public run position aligned to one semantic decision row."""

    episode_seed: int
    act: int
    floor: int
    is_combat: bool
    strategic_context_kind: int | None
    public_snapshot: PublicDecisionSnapshot | None = None

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "episode_seed",
            _integer(self.episode_seed, "episode_seed", minimum=0),
        )
        object.__setattr__(self, "act", _integer(self.act, "act", minimum=0))
        object.__setattr__(self, "floor", _integer(self.floor, "floor", minimum=0))
        if type(self.is_combat) is not bool:
            raise DecisionProgressError("is_combat must be bool")
        context = self.strategic_context_kind
        if self.is_combat:
            if context is not None:
                raise DecisionProgressError(
                    "combat decision cannot carry a strategic context kind"
                )
        elif context is None:
            raise DecisionProgressError(
                "strategic decision requires a strategic context kind"
            )
        else:
            object.__setattr__(
                self,
                "strategic_context_kind",
                _integer(context, "strategic_context_kind", minimum=1),
            )
        if self.public_snapshot is not None:
            if not isinstance(self.public_snapshot, PublicDecisionSnapshot):
                raise DecisionProgressError(
                    "decision progress public_snapshot must be typed"
                )
            if self.public_snapshot.is_combat != self.is_combat:
                raise DecisionProgressError(
                    "public decision snapshot combat domain disagrees with progress"
                )


class DecisionProgressProvider(Protocol):
    """Caller-owned source of public progress for selected environment slots."""

    def capture(
        self,
        slot_indices: Sequence[int],
    ) -> tuple[DecisionRunProgress, ...]: ...


class BridgeDecisionProgressProvider:
    """Adapt the bridge's compact public contexts without inspecting semantics."""

    def __init__(self, environment: object) -> None:
        source = getattr(environment, "public_run_contexts", None)
        if not callable(source):
            raise DecisionProgressError(
                "decision progress environment does not expose public_run_contexts()"
            )
        self._source = source
        snapshot_source = getattr(environment, "public_information_snapshots", None)
        if not callable(snapshot_source):
            raise DecisionProgressError(
                "decision progress environment does not expose "
                "public_information_snapshots()"
            )
        self._snapshot_source = snapshot_source

    def capture(
        self,
        slot_indices: Sequence[int],
    ) -> tuple[DecisionRunProgress, ...]:
        slots = tuple(
            _integer(slot, "decision progress slot", minimum=0)
            for slot in slot_indices
        )
        if len(set(slots)) != len(slots):
            raise DecisionProgressError("decision progress slots contain duplicates")
        requested = set(slots)
        rows = self._source()
        if not isinstance(rows, Sequence) or isinstance(rows, (str, bytes)):
            raise DecisionProgressError(
                "public_run_contexts() must return a sequence"
            )
        public_snapshots = self._capture_public_snapshots(requested)
        contexts: dict[int, DecisionRunProgress] = {}
        for row in rows:
            if not isinstance(row, Sequence) or isinstance(row, (str, bytes)):
                raise DecisionProgressError("public run context row must be a pair")
            if len(row) != 2:
                raise DecisionProgressError(
                    "public run context row must contain two values"
                )
            slot = _integer(row[0], "public run context slot", minimum=0)
            if slot not in requested:
                continue
            if slot in contexts:
                raise DecisionProgressError("public run contexts repeat a slot")
            view = row[1]
            contexts[slot] = DecisionRunProgress(
                episode_seed=_attribute(view, "seed"),
                act=_attribute(view, "act"),
                floor=_attribute(view, "floor"),
                is_combat=_attribute(view, "is_combat"),
                strategic_context_kind=_attribute(view, "strategic_context_kind"),
                public_snapshot=public_snapshots[slot],
            )
        try:
            return tuple(contexts[slot] for slot in slots)
        except KeyError as error:
            raise DecisionProgressError(
                f"decision slot {error.args[0]} has no public run context"
            ) from error

    def _capture_public_snapshots(
        self,
        requested: set[int],
    ) -> dict[int, PublicDecisionSnapshot]:
        rows = self._snapshot_source()
        if not isinstance(rows, Sequence) or isinstance(rows, (str, bytes)):
            raise DecisionProgressError(
                "public_information_snapshots() must return a sequence"
            )
        snapshots: dict[int, PublicDecisionSnapshot] = {}
        for row in rows:
            if not isinstance(row, Sequence) or isinstance(row, (str, bytes)):
                raise DecisionProgressError("public information snapshot row must be a pair")
            if len(row) != 2:
                raise DecisionProgressError(
                    "public information snapshot row must contain two values"
                )
            slot = _integer(row[0], "public information snapshot slot", minimum=0)
            if slot not in requested:
                continue
            if slot in snapshots:
                raise DecisionProgressError(
                    "public information snapshots repeat a slot"
                )
            view = row[1]
            snapshots[slot] = PublicDecisionSnapshot(
                phase=_attribute(view, "phase"),
                is_combat=_attribute(view, "is_combat"),
                snapshot_id=_attribute(view, "snapshot_id"),
                observation_id=_attribute(view, "observation_id"),
                history_snapshot_id=_attribute(view, "history_snapshot_id"),
                candidate_surface_id=_attribute(view, "candidate_surface_id"),
                candidate_ids=tuple(_attribute(view, "candidate_ids")),
            )
        missing = requested - set(snapshots)
        if missing:
            raise DecisionProgressError(
                f"decision slots {sorted(missing)} have no public information snapshots"
            )
        return snapshots


def _attribute(source: object, name: str) -> object:
    try:
        return getattr(source, name)
    except AttributeError as error:
        raise DecisionProgressError(
            f"public run context is missing {name}"
        ) from error


def _nonempty_text(value: object, name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise DecisionProgressError(f"{name} must be non-empty text")
    return value


def _integer(value: object, name: str, *, minimum: int) -> int:
    if isinstance(value, bool):
        raise DecisionProgressError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise DecisionProgressError(f"{name} must be an integer") from error
    if normalized < minimum:
        raise DecisionProgressError(f"{name} must be at least {minimum}")
    return normalized
