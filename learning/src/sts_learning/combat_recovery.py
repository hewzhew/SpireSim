"""Exact replay-derived recovery roots for sparse combat objectives."""

from __future__ import annotations

import operator
from collections import deque
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from typing import Protocol

from .combat_experience import CompletedCombatGroupExperience
from .combat_outcomes import (
    CombatOutcomeError,
    CombatTerminalOutcome,
    CombatTerminalStepBatch,
    validate_combat_digest,
)
from .policy import BehaviorManifestId


class CombatRecoveryError(RuntimeError):
    """A recovery root lost exact replay or lineage guarantees."""


class _CombatRecoveryHandle(Protocol):
    root_id: str
    exact_combat_state_hash: str
    source_root_id: str
    source_exact_combat_state_hash: str
    source_replicate_index: int

    def spawn_group(
        self,
        replicate_count: int,
        potion_slots: Sequence[int] | None = None,
    ): ...


@dataclass(frozen=True)
class CombatRecoveryRoot:
    """One exact root derived from a replay-verified winning trajectory."""

    root_id: str
    exact_combat_state_hash: str
    source_root_id: str
    source_exact_combat_state_hash: str
    teacher_replicate_index: int
    transitions_to_terminal: int
    teacher_outcome: CombatTerminalOutcome
    _handle: _CombatRecoveryHandle = field(repr=False, compare=False)

    def __post_init__(self) -> None:
        for value, name in (
            (self.root_id, "root_id"),
            (self.exact_combat_state_hash, "exact_combat_state_hash"),
            (self.source_root_id, "source_root_id"),
            (
                self.source_exact_combat_state_hash,
                "source_exact_combat_state_hash",
            ),
        ):
            _validate_digest(value, name)
        teacher = _nonnegative_integer(
            self.teacher_replicate_index,
            "teacher_replicate_index",
        )
        distance = _positive_integer(
            self.transitions_to_terminal,
            "transitions_to_terminal",
        )
        if not isinstance(self.teacher_outcome, CombatTerminalOutcome):
            raise CombatRecoveryError("recovery root requires a typed teacher outcome")
        if not self.teacher_outcome.won:
            raise CombatRecoveryError("recovery root teacher must be a verified win")
        if self.teacher_outcome.replicate_index != teacher:
            raise CombatRecoveryError("recovery root teacher replicate is misaligned")
        expected = (
            self.root_id,
            self.exact_combat_state_hash,
            self.source_root_id,
            self.source_exact_combat_state_hash,
        )
        observed = (
            getattr(self._handle, "root_id", None),
            getattr(self._handle, "exact_combat_state_hash", None),
            getattr(self._handle, "source_root_id", None),
            getattr(self._handle, "source_exact_combat_state_hash", None),
        )
        if observed != expected:
            raise CombatRecoveryError("opaque recovery handle disagrees with typed lineage")
        if getattr(self._handle, "source_replicate_index", None) != 0:
            raise CombatRecoveryError("replayed recovery handle lost single-slot lineage")
        object.__setattr__(self, "teacher_replicate_index", teacher)
        object.__setattr__(self, "transitions_to_terminal", distance)

    def spawn_group(
        self,
        replicate_count: int,
        potion_slots: Sequence[int] | None = None,
    ):
        count = _positive_integer(replicate_count, "replicate_count")
        slots = (
            None
            if potion_slots is None
            else tuple(
                _nonnegative_integer(slot, f"potion_slots[{index}]")
                for index, slot in enumerate(potion_slots)
            )
        )
        if slots is not None and len(set(slots)) != len(slots):
            raise CombatRecoveryError("recovery potion slots must be distinct")
        group = self._handle.spawn_group(count, slots)
        if (
            getattr(group, "root_id", None),
            getattr(group, "exact_combat_state_hash", None),
            getattr(group, "replicate_count", None),
        ) != (self.root_id, self.exact_combat_state_hash, count):
            raise CombatRecoveryError("spawned recovery group changed its exact root")
        return group


@dataclass(frozen=True)
class CombatRecoveryPlan:
    """Bounded terminal-nearest roots from one exact winning replay."""

    source_root_id: str
    source_exact_combat_state_hash: str
    teacher_behavior_manifest_id: BehaviorManifestId
    teacher_replicate_index: int
    transition_count: int
    roots: tuple[CombatRecoveryRoot, ...]

    def __post_init__(self) -> None:
        _validate_digest(self.source_root_id, "source_root_id")
        _validate_digest(
            self.source_exact_combat_state_hash,
            "source_exact_combat_state_hash",
        )
        if not isinstance(self.teacher_behavior_manifest_id, BehaviorManifestId):
            raise CombatRecoveryError("recovery plan requires teacher behavior identity")
        teacher = _nonnegative_integer(
            self.teacher_replicate_index,
            "teacher_replicate_index",
        )
        transitions = _positive_integer(self.transition_count, "transition_count")
        roots = tuple(self.roots)
        if not roots or not all(isinstance(root, CombatRecoveryRoot) for root in roots):
            raise CombatRecoveryError("recovery plan requires at least one typed root")
        distances = tuple(root.transitions_to_terminal for root in roots)
        if distances != tuple(range(1, len(roots) + 1)):
            raise CombatRecoveryError(
                "recovery roots must be ordered from terminal-nearest outward"
            )
        if distances[-1] > transitions:
            raise CombatRecoveryError("recovery root lies before the replay root")
        if any(
            root.source_root_id != self.source_root_id
            or root.source_exact_combat_state_hash
            != self.source_exact_combat_state_hash
            or root.teacher_replicate_index != teacher
            for root in roots
        ):
            raise CombatRecoveryError("recovery plan roots disagree on source lineage")
        object.__setattr__(self, "teacher_replicate_index", teacher)
        object.__setattr__(self, "transition_count", transitions)
        object.__setattr__(self, "roots", roots)

    @property
    def root_count(self) -> int:
        return len(self.roots)


class CombatRecoveryRootSource:
    """Identity-checking trainer source over one bounded recovery plan."""

    def __init__(self, plan: CombatRecoveryPlan) -> None:
        if not isinstance(plan, CombatRecoveryPlan):
            raise CombatRecoveryError("recovery root source requires a typed plan")
        self.plan = plan

    @property
    def root_count(self) -> int:
        return self.plan.root_count

    def combat_group(
        self,
        slot_index: int,
        replicate_count: int,
        potion_slots: Sequence[int] | None = None,
    ):
        slot = _nonnegative_integer(slot_index, "slot_index")
        if slot >= self.root_count:
            raise CombatRecoveryError("recovery root slot is out of range")
        return self.plan.roots[slot].spawn_group(replicate_count, potion_slots)


def replay_winning_recovery_roots(
    source: object,
    *,
    slot_index: int,
    experience: CompletedCombatGroupExperience,
    teacher_replicate_index: int,
    max_roots: int,
) -> CombatRecoveryPlan:
    """Replay one observed win and retain only its terminal-nearest root window.

    The recorded ordinals are replay evidence, never supervised action labels.
    A fresh exact group must reproduce the complete typed terminal outcome
    before any captured root is returned.
    """

    if not callable(getattr(source, "combat_group", None)):
        raise CombatRecoveryError("recovery replay requires a combat-root source")
    if not isinstance(experience, CompletedCombatGroupExperience):
        raise CombatRecoveryError("recovery replay requires completed combat experience")
    slot = _nonnegative_integer(slot_index, "slot_index")
    teacher = _nonnegative_integer(
        teacher_replicate_index,
        "teacher_replicate_index",
    )
    window = _positive_integer(max_roots, "max_roots")
    try:
        teacher_outcome = experience.outcomes.outcomes[teacher]
    except IndexError as error:
        raise CombatRecoveryError("teacher replicate is outside the completed group") from error
    if teacher_outcome.replicate_index != teacher:
        raise CombatRecoveryError("completed outcomes are not replicate-aligned")
    if not teacher_outcome.won:
        raise CombatRecoveryError("recovery replay requires an observed winning replicate")

    ordinals = tuple(
        batch.selected_ordinals[batch.replicate_indices.index(teacher)]
        for batch in experience.batches
        if teacher in batch.replicate_indices
    )
    if not ordinals:
        raise CombatRecoveryError("winning teacher has no recorded decisions")

    group = source.combat_group(slot, 1)
    if (
        getattr(group, "root_id", None),
        getattr(group, "exact_combat_state_hash", None),
        getattr(group, "replicate_count", None),
        getattr(group, "terminal_count", None),
    ) != (
        experience.root_id,
        experience.exact_combat_state_hash,
        1,
        0,
    ):
        raise CombatRecoveryError("replay source changed the teacher's exact root")
    if not callable(getattr(group, "capture_recovery_root", None)):
        raise CombatRecoveryError("combat bridge lacks explicit recovery-root capture")

    retained: deque[tuple[int, _CombatRecoveryHandle]] = deque(maxlen=window)
    ordinal_cursor = 0
    transitions = 0
    replayed_outcome: CombatTerminalOutcome | None = None
    while group.terminal_count == 0:
        retained.append((transitions, group.capture_recovery_root(0)))
        while not group.ready:
            if ordinal_cursor >= len(ordinals):
                raise CombatRecoveryError("teacher decisions ended before combat terminal")
            decision = group.decision_batch(semantic=False)
            _validate_single_replay_row(decision)
            ordinal = ordinals[ordinal_cursor]
            candidate_count = _single_integer_column(decision, "candidate_counts")
            if ordinal >= candidate_count:
                raise CombatRecoveryError("teacher ordinal is illegal during exact replay")
            group.choose([ordinal])
            ordinal_cursor += 1
        try:
            terminal = CombatTerminalStepBatch.from_bridge_step(
                group.step(),
                replicate_count=1,
            )
        except CombatOutcomeError as error:
            raise CombatRecoveryError(str(error)) from error
        transitions += 1
        if terminal.outcomes:
            if len(terminal.outcomes) != 1:
                raise CombatRecoveryError("single replay produced multiple terminal rows")
            replayed_outcome = terminal.outcomes[0]

    if ordinal_cursor != len(ordinals):
        raise CombatRecoveryError("teacher decisions continue beyond replay terminal")
    if replayed_outcome is None or not _same_terminal_facts(
        replayed_outcome,
        teacher_outcome,
    ):
        raise CombatRecoveryError("teacher win did not reproduce its typed terminal outcome")

    roots = tuple(
        CombatRecoveryRoot(
            root_id=handle.root_id,
            exact_combat_state_hash=handle.exact_combat_state_hash,
            source_root_id=experience.root_id,
            source_exact_combat_state_hash=experience.exact_combat_state_hash,
            teacher_replicate_index=teacher,
            transitions_to_terminal=transitions - transition_index,
            teacher_outcome=teacher_outcome,
            _handle=handle,
        )
        for transition_index, handle in reversed(retained)
    )
    return CombatRecoveryPlan(
        source_root_id=experience.root_id,
        source_exact_combat_state_hash=experience.exact_combat_state_hash,
        teacher_behavior_manifest_id=experience.behavior_manifest_id,
        teacher_replicate_index=teacher,
        transition_count=transitions,
        roots=roots,
    )


def _validate_single_replay_row(decision: Mapping[str, object]) -> None:
    if not isinstance(decision, Mapping):
        raise CombatRecoveryError("replay decision batch must be a mapping")
    try:
        slots = tuple(
            _integer(value, "slot_index") for value in decision["slot_indices"]
        )
    except (KeyError, TypeError) as error:
        raise CombatRecoveryError("replay decision is missing slot_indices") from error
    if slots != (0,):
        raise CombatRecoveryError("single replay decision lost replicate alignment")


def _single_integer_column(decision: Mapping[str, object], name: str) -> int:
    try:
        values = tuple(decision[name])
    except (KeyError, TypeError) as error:
        raise CombatRecoveryError(f"replay decision is missing {name}") from error
    if len(values) != 1:
        raise CombatRecoveryError(f"replay decision {name} must contain one row")
    value = _integer(values[0], name)
    if value <= 0:
        raise CombatRecoveryError(f"replay decision {name} must be positive")
    return value


def _same_terminal_facts(
    replayed: CombatTerminalOutcome,
    teacher: CombatTerminalOutcome,
) -> bool:
    return (
        replayed.terminal_kind,
        replayed.won,
        replayed.start_hp,
        replayed.final_hp,
        replayed.hp_loss,
        replayed.turns,
        replayed.potions_used,
        replayed.potions_discarded,
        replayed.cards_played,
    ) == (
        teacher.terminal_kind,
        teacher.won,
        teacher.start_hp,
        teacher.final_hp,
        teacher.hp_loss,
        teacher.turns,
        teacher.potions_used,
        teacher.potions_discarded,
        teacher.cards_played,
    )


def _validate_digest(value: object, name: str) -> None:
    try:
        validate_combat_digest(value, name)
    except CombatOutcomeError as error:
        raise CombatRecoveryError(str(error)) from error


def _positive_integer(value: object, name: str) -> int:
    normalized = _integer(value, name)
    if normalized <= 0:
        raise CombatRecoveryError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    normalized = _integer(value, name)
    if normalized < 0:
        raise CombatRecoveryError(f"{name} must be non-negative")
    return normalized


def _integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatRecoveryError(f"{name} must be an integer, not bool")
    try:
        return operator.index(value)
    except TypeError as error:
        raise CombatRecoveryError(f"{name} must be an integer") from error
