"""Compact diagnostics for independent same-root combat advantage axes."""

from __future__ import annotations

import operator
from collections.abc import Sequence
from dataclasses import dataclass

from .combat_outcomes import (
    CombatGroupedAdvantages,
    CombatOutcomeError,
    combat_advantage_has_signal,
    validate_combat_digest,
)


class CombatSignalError(ValueError):
    """A combat signal summary is malformed or misaligned."""


@dataclass(frozen=True)
class CombatAxisSignalSummary:
    """Nonzero same-root support at outcome and retained-decision granularity."""

    replicate_count: int
    decision_count: int

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "replicate_count",
            _nonnegative_integer(self.replicate_count, "replicate_count"),
        )
        object.__setattr__(
            self,
            "decision_count",
            _nonnegative_integer(self.decision_count, "decision_count"),
        )
        if (self.replicate_count == 0) != (self.decision_count == 0):
            raise CombatSignalError(
                "combat axis replicate and decision signal must agree on presence"
            )

    @property
    def has_signal(self) -> bool:
        return self.replicate_count > 0


@dataclass(frozen=True)
class CombatGroupSignalSummary:
    """Compact independent-axis signal census for one completed exact root."""

    root_id: str
    exact_combat_state_hash: str
    replicate_count: int
    decision_count: int
    win: CombatAxisSignalSummary
    terminal_hp: CombatAxisSignalSummary
    enemy_hp_progress: CombatAxisSignalSummary
    potion_retention: CombatAxisSignalSummary

    def __post_init__(self) -> None:
        _validate_root(self.root_id, self.exact_combat_state_hash)
        replicate_count = _positive_integer(self.replicate_count, "replicate_count")
        decision_count = _positive_integer(self.decision_count, "decision_count")
        for axis in (
            self.win,
            self.terminal_hp,
            self.enemy_hp_progress,
            self.potion_retention,
        ):
            if not isinstance(axis, CombatAxisSignalSummary):
                raise CombatSignalError(
                    "combat signal summary requires typed axis summaries"
                )
            if axis.replicate_count > replicate_count:
                raise CombatSignalError(
                    "combat axis signal exceeds the replicate count"
                )
            if axis.decision_count > decision_count:
                raise CombatSignalError(
                    "combat axis signal exceeds the decision count"
                )
        object.__setattr__(self, "replicate_count", replicate_count)
        object.__setattr__(self, "decision_count", decision_count)


@dataclass(frozen=True)
class CombatAxisSignalCensus:
    signal_group_count: int
    signal_replicate_count: int
    signal_decision_count: int

    def __post_init__(self) -> None:
        for name in (
            "signal_group_count",
            "signal_replicate_count",
            "signal_decision_count",
        ):
            object.__setattr__(
                self,
                name,
                _nonnegative_integer(getattr(self, name), name),
            )


@dataclass(frozen=True)
class CombatSignalCensus:
    """Bounded aggregate over distinct exact combat roots."""

    group_count: int
    replicate_count: int
    decision_count: int
    win: CombatAxisSignalCensus
    terminal_hp: CombatAxisSignalCensus
    enemy_hp_progress: CombatAxisSignalCensus
    potion_retention: CombatAxisSignalCensus

    def __post_init__(self) -> None:
        group_count = _positive_integer(self.group_count, "group_count")
        replicate_count = _positive_integer(self.replicate_count, "replicate_count")
        decision_count = _positive_integer(self.decision_count, "decision_count")
        for axis in (
            self.win,
            self.terminal_hp,
            self.enemy_hp_progress,
            self.potion_retention,
        ):
            if not isinstance(axis, CombatAxisSignalCensus):
                raise CombatSignalError(
                    "combat signal census requires typed axis counts"
                )
            if axis.signal_group_count > group_count:
                raise CombatSignalError("axis signal groups exceed census groups")
            if axis.signal_replicate_count > replicate_count:
                raise CombatSignalError(
                    "axis signal replicates exceed census replicates"
                )
            if axis.signal_decision_count > decision_count:
                raise CombatSignalError(
                    "axis signal decisions exceed census decisions"
                )
        object.__setattr__(self, "group_count", group_count)
        object.__setattr__(self, "replicate_count", replicate_count)
        object.__setattr__(self, "decision_count", decision_count)


def summarize_combat_group_signals(
    *,
    root_id: str,
    exact_combat_state_hash: str,
    grouped: CombatGroupedAdvantages,
    decision_count: int,
    win_decisions: Sequence[float],
    terminal_hp_decisions: Sequence[float],
    enemy_hp_progress_decisions: Sequence[float],
    potion_retention_decisions: Sequence[float],
) -> CombatGroupSignalSummary:
    """Summarize aligned axes without retaining semantic decision payloads."""

    if not isinstance(grouped, CombatGroupedAdvantages):
        raise CombatSignalError("signal summary requires CombatGroupedAdvantages")
    normalized_decision_count = _positive_integer(decision_count, "decision_count")
    decision_axes = tuple(
        tuple(values)
        for values in (
            win_decisions,
            terminal_hp_decisions,
            enemy_hp_progress_decisions,
            potion_retention_decisions,
        )
    )
    if any(len(values) != normalized_decision_count for values in decision_axes):
        raise CombatSignalError("combat decision signal columns are misaligned")
    replicate_count = len(grouped.win)
    return CombatGroupSignalSummary(
        root_id=root_id,
        exact_combat_state_hash=exact_combat_state_hash,
        replicate_count=replicate_count,
        decision_count=normalized_decision_count,
        win=_axis_summary(grouped.win, decision_axes[0]),
        terminal_hp=_axis_summary(grouped.terminal_hp, decision_axes[1]),
        enemy_hp_progress=_axis_summary(
            grouped.enemy_hp_progress,
            decision_axes[2],
        ),
        potion_retention=_axis_summary(
            grouped.potion_retention,
            decision_axes[3],
        ),
    )


def build_combat_signal_census(
    summaries: Sequence[CombatGroupSignalSummary],
    *,
    max_groups: int,
) -> CombatSignalCensus:
    """Aggregate distinct roots without retaining their semantic payloads."""

    bound = _positive_integer(max_groups, "max_groups")
    normalized = tuple(summaries)
    if not normalized:
        raise CombatSignalError("combat signal census requires at least one group")
    if len(normalized) > bound:
        raise CombatSignalError("combat signal census exceeds max_groups")
    if not all(isinstance(item, CombatGroupSignalSummary) for item in normalized):
        raise CombatSignalError("combat signal census requires typed group summaries")
    identities = tuple(
        (item.root_id, item.exact_combat_state_hash) for item in normalized
    )
    if len(set(identities)) != len(identities):
        raise CombatSignalError("combat signal census repeats an exact root")
    return CombatSignalCensus(
        group_count=len(normalized),
        replicate_count=sum(item.replicate_count for item in normalized),
        decision_count=sum(item.decision_count for item in normalized),
        win=_axis_census(tuple(item.win for item in normalized)),
        terminal_hp=_axis_census(tuple(item.terminal_hp for item in normalized)),
        enemy_hp_progress=_axis_census(
            tuple(item.enemy_hp_progress for item in normalized)
        ),
        potion_retention=_axis_census(
            tuple(item.potion_retention for item in normalized)
        ),
    )


def _axis_summary(
    replicate_values: Sequence[float],
    decision_values: Sequence[float],
) -> CombatAxisSignalSummary:
    return CombatAxisSignalSummary(
        replicate_count=sum(
            combat_advantage_has_signal(value) for value in replicate_values
        ),
        decision_count=sum(
            combat_advantage_has_signal(value) for value in decision_values
        ),
    )


def _axis_census(
    summaries: Sequence[CombatAxisSignalSummary],
) -> CombatAxisSignalCensus:
    return CombatAxisSignalCensus(
        signal_group_count=sum(summary.has_signal for summary in summaries),
        signal_replicate_count=sum(
            summary.replicate_count for summary in summaries
        ),
        signal_decision_count=sum(summary.decision_count for summary in summaries),
    )


def _positive_integer(value: object, name: str) -> int:
    normalized = _nonnegative_integer(value, name)
    if normalized == 0:
        raise CombatSignalError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatSignalError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatSignalError(f"{name} must be an integer") from error
    if normalized < 0:
        raise CombatSignalError(f"{name} must be non-negative")
    return normalized


def _validate_root(root_id: object, exact_combat_state_hash: object) -> None:
    try:
        validate_combat_digest(root_id, "root_id")
        validate_combat_digest(
            exact_combat_state_hash,
            "exact_combat_state_hash",
        )
    except CombatOutcomeError as error:
        raise CombatSignalError(str(error)) from error
