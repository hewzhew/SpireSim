"""Non-authoritative diagnostics for whole-run decision credit targets."""

from __future__ import annotations

import math
from collections.abc import Callable, Hashable, Sequence
from dataclasses import dataclass

from .decision_progress import DecisionRunProgress
from .public_trajectory import PublicAttemptTrajectoryV1
from .terminal_returns import (
    FloorProgressReturnConfig,
    floor_progress_terminal_return,
)


class CreditAssignmentError(ValueError):
    """Complete attempts cannot produce aligned decision credit evidence."""


@dataclass(frozen=True)
class DecisionCreditDistribution:
    """Compact sign and range summary for one decision-target definition."""

    decision_count: int
    negative: int
    zero: int
    positive: int
    minimum: float
    maximum: float
    mean: float


@dataclass(frozen=True)
class DecisionFloorCreditComparison:
    """Target comparison for decisions observed at one public run floor."""

    floor: int
    terminal_broadcast: DecisionCreditDistribution
    remaining_progress: DecisionCreditDistribution
    matched_floor_advantage: DecisionCreditDistribution


@dataclass(frozen=True)
class DecisionScopeCreditComparison:
    """Target comparison for combat or strategic decision rows."""

    is_combat: bool
    terminal_broadcast: DecisionCreditDistribution
    remaining_progress: DecisionCreditDistribution
    matched_floor_advantage: DecisionCreditDistribution


@dataclass(frozen=True)
class DecisionStrategicContextCreditComparison:
    """Target comparison for one typed strategic decision context."""

    context_kind: int
    strategic_scope_weight: float
    matched_floor_strategic_weighted_target: float
    matched_floor_context_strategic_weighted_target: float
    terminal_broadcast: DecisionCreditDistribution
    remaining_progress: DecisionCreditDistribution
    matched_floor_advantage: DecisionCreditDistribution
    matched_floor_context_advantage: DecisionCreditDistribution


@dataclass(frozen=True)
class EpisodeRootCreditComparison:
    """Learning-potential evidence for retries from one exact episode root."""

    episode_seed: int
    episode_generation: int
    attempt_count: int
    terminal_floor_min: int
    terminal_floor_max: int
    terminal_floor_mean: float
    matched_episode_floor_context_advantage: DecisionCreditDistribution
    strategic_matched_episode_floor_context_advantage: (
        DecisionCreditDistribution | None
    )


@dataclass(frozen=True)
class CreditAssignmentComparison:
    """Current terminal broadcast beside a decision-local progress target."""

    attempt_count: int
    terminal_broadcast: DecisionCreditDistribution
    remaining_progress: DecisionCreditDistribution
    matched_floor_advantage: DecisionCreditDistribution
    matched_floor_context_advantage: DecisionCreditDistribution
    matched_episode_floor_context_advantage: DecisionCreditDistribution
    by_decision_floor: tuple[DecisionFloorCreditComparison, ...]
    by_combat_scope: tuple[DecisionScopeCreditComparison, ...]
    by_strategic_context: tuple[DecisionStrategicContextCreditComparison, ...]
    by_episode_root: tuple[EpisodeRootCreditComparison, ...]


@dataclass(frozen=True)
class _DecisionCreditRow:
    episode_seed: int
    episode_generation: int
    act: int
    floor: int
    is_combat: bool
    strategic_context_kind: int | None
    terminal_broadcast: float
    remaining_progress: float


def compare_credit_assignment(
    attempts: Sequence[PublicAttemptTrajectoryV1],
    config: FloorProgressReturnConfig,
) -> CreditAssignmentComparison:
    """Compare target distributions without changing the optimizer objective."""

    normalized, aligned = _aligned_credit_rows(attempts, config)
    matched_aligned = _matched_floor_advantages(aligned)
    context_matched_aligned = _matched_floor_context_advantages(aligned)
    episode_context_matched_aligned = (
        _matched_episode_floor_context_advantages(aligned)
    )
    broadcast = [
        row.terminal_broadcast
        for attempt in aligned
        for batch in attempt
        for row in batch
    ]
    remaining = [
        row.remaining_progress
        for attempt in aligned
        for batch in attempt
        for row in batch
    ]
    matched = [
        value
        for attempt in matched_aligned
        for batch in attempt
        for value in batch
    ]
    context_matched_values = [
        value
        for attempt in context_matched_aligned
        for batch in attempt
        for value in batch
    ]
    episode_context_matched_values = [
        value
        for attempt in episode_context_matched_aligned
        for batch in attempt
        for value in batch
    ]
    by_episode_root: dict[
        tuple[int, int],
        tuple[list[int], list[float], list[float]],
    ] = {}
    for attempt, attempt_rows, attempt_advantages in zip(
        normalized,
        aligned,
        episode_context_matched_aligned,
        strict=True,
    ):
        key = (
            attempt.lineage.key.episode_seed,
            attempt.lineage.key.episode_generation,
        )
        floors, all_advantages, strategic_advantages = by_episode_root.setdefault(
            key,
            ([], [], []),
        )
        floors.append(attempt.terminal.terminal.terminal_floor)
        for batch_rows, batch_advantages in zip(
            attempt_rows,
            attempt_advantages,
            strict=True,
        ):
            all_advantages.extend(batch_advantages)
            strategic_advantages.extend(
                advantage
                for row, advantage in zip(
                    batch_rows,
                    batch_advantages,
                    strict=True,
                )
                if not row.is_combat
            )
    by_floor: dict[int, tuple[list[float], list[float], list[float]]] = {}
    by_scope: dict[bool, tuple[list[float], list[float], list[float]]] = {}
    by_context: dict[
        int,
        tuple[list[float], list[float], list[float], list[float]],
    ] = {}
    context_objective: dict[int, list[float]] = {}
    for attempt_rows, attempt_advantages, attempt_context_advantages in zip(
        aligned,
        matched_aligned,
        context_matched_aligned,
        strict=True,
    ):
        strategic_decisions = sum(
            not row.is_combat for batch in attempt_rows for row in batch
        )
        strategic_row_weight = (
            0.0
            if strategic_decisions == 0
            else 1.0 / (len(aligned) * strategic_decisions)
        )
        for batch_rows, batch_advantages, batch_context_advantages in zip(
            attempt_rows,
            attempt_advantages,
            attempt_context_advantages,
            strict=True,
        ):
            for row, advantage, context_advantage in zip(
                batch_rows,
                batch_advantages,
                batch_context_advantages,
                strict=True,
            ):
                floor_broadcast, floor_remaining, floor_matched = by_floor.setdefault(
                    row.floor,
                    ([], [], []),
                )
                floor_broadcast.append(row.terminal_broadcast)
                floor_remaining.append(row.remaining_progress)
                floor_matched.append(advantage)
                scope_broadcast, scope_remaining, scope_matched = by_scope.setdefault(
                    row.is_combat,
                    ([], [], []),
                )
                scope_broadcast.append(row.terminal_broadcast)
                scope_remaining.append(row.remaining_progress)
                scope_matched.append(advantage)
                if not row.is_combat:
                    context = row.strategic_context_kind
                    if context is None:
                        raise CreditAssignmentError(
                            "strategic credit row is missing its context kind"
                        )
                    (
                        context_broadcast,
                        context_remaining,
                        context_floor_matched,
                        context_context_matched,
                    ) = by_context.setdefault(context, ([], [], [], []))
                    context_broadcast.append(row.terminal_broadcast)
                    context_remaining.append(row.remaining_progress)
                    context_floor_matched.append(advantage)
                    context_context_matched.append(context_advantage)
                    objective = context_objective.setdefault(
                        context,
                        [0.0, 0.0, 0.0],
                    )
                    objective[0] += strategic_row_weight
                    objective[1] += strategic_row_weight * advantage
                    objective[2] += strategic_row_weight * context_advantage

    return CreditAssignmentComparison(
        attempt_count=len(normalized),
        terminal_broadcast=_distribution(broadcast),
        remaining_progress=_distribution(remaining),
        matched_floor_advantage=_distribution(matched),
        matched_floor_context_advantage=_distribution(context_matched_values),
        matched_episode_floor_context_advantage=_distribution(
            episode_context_matched_values
        ),
        by_decision_floor=tuple(
            DecisionFloorCreditComparison(
                floor=floor,
                terminal_broadcast=_distribution(values[0]),
                remaining_progress=_distribution(values[1]),
                matched_floor_advantage=_distribution(values[2]),
            )
            for floor, values in sorted(by_floor.items())
        ),
        by_combat_scope=tuple(
            DecisionScopeCreditComparison(
                is_combat=is_combat,
                terminal_broadcast=_distribution(values[0]),
                remaining_progress=_distribution(values[1]),
                matched_floor_advantage=_distribution(values[2]),
            )
            for is_combat, values in sorted(by_scope.items())
        ),
        by_strategic_context=tuple(
            DecisionStrategicContextCreditComparison(
                context_kind=context_kind,
                strategic_scope_weight=context_objective[context_kind][0],
                matched_floor_strategic_weighted_target=(
                    context_objective[context_kind][1]
                ),
                matched_floor_context_strategic_weighted_target=(
                    context_objective[context_kind][2]
                ),
                terminal_broadcast=_distribution(values[0]),
                remaining_progress=_distribution(values[1]),
                matched_floor_advantage=_distribution(values[2]),
                matched_floor_context_advantage=_distribution(values[3]),
            )
            for context_kind, values in sorted(by_context.items())
        ),
        by_episode_root=tuple(
            EpisodeRootCreditComparison(
                episode_seed=episode_seed,
                episode_generation=episode_generation,
                attempt_count=len(values[0]),
                terminal_floor_min=min(values[0]),
                terminal_floor_max=max(values[0]),
                terminal_floor_mean=math.fsum(values[0]) / len(values[0]),
                matched_episode_floor_context_advantage=_distribution(values[1]),
                strategic_matched_episode_floor_context_advantage=(
                    _distribution(values[2]) if values[2] else None
                ),
            )
            for (episode_seed, episode_generation), values in sorted(
                by_episode_root.items()
            )
        ),
    )


def matched_floor_leave_one_out_advantages(
    attempts: Sequence[PublicAttemptTrajectoryV1],
    config: FloorProgressReturnConfig,
) -> tuple[tuple[tuple[float, ...], ...], ...]:
    """Return attempt/batch/row-aligned advantages from matched run floors."""

    _, aligned = _aligned_credit_rows(attempts, config)
    return _matched_floor_advantages(aligned)


def matched_floor_context_leave_one_out_advantages(
    attempts: Sequence[PublicAttemptTrajectoryV1],
    config: FloorProgressReturnConfig,
) -> tuple[tuple[tuple[float, ...], ...], ...]:
    """Return aligned advantages matched by floor and typed decision context."""

    _, aligned = _aligned_credit_rows(attempts, config)
    return _matched_floor_context_advantages(aligned)


def matched_episode_floor_context_leave_one_out_advantages(
    attempts: Sequence[PublicAttemptTrajectoryV1],
    config: FloorProgressReturnConfig,
) -> tuple[tuple[tuple[float, ...], ...], ...]:
    """Return aligned advantages matched by episode, floor, and context."""

    _, aligned = _aligned_credit_rows(attempts, config)
    return _matched_episode_floor_context_advantages(aligned)


def _aligned_credit_rows(
    attempts: Sequence[PublicAttemptTrajectoryV1],
    config: FloorProgressReturnConfig,
) -> tuple[
    tuple[PublicAttemptTrajectoryV1, ...],
    tuple[tuple[tuple[_DecisionCreditRow, ...], ...], ...],
]:
    normalized = tuple(attempts)
    if not normalized:
        raise CreditAssignmentError("credit comparison requires complete attempts")
    if not all(
        isinstance(attempt, PublicAttemptTrajectoryV1)
        for attempt in normalized
    ):
        raise CreditAssignmentError(
            "credit comparison accepts only public attempt trajectories"
        )
    if not isinstance(config, FloorProgressReturnConfig):
        raise CreditAssignmentError("credit comparison requires a floor return config")

    aligned: list[tuple[tuple[_DecisionCreditRow, ...], ...]] = []
    for attempt in normalized:
        attempt_broadcast = floor_progress_terminal_return(attempt.terminal, config)
        attempt_batches: list[tuple[_DecisionCreditRow, ...]] = []
        for decision in attempt.decisions:
            progress = decision.run_progress
            batch_rows = (
                _DecisionCreditRow(
                    episode_seed=progress.episode_seed,
                    episode_generation=attempt.lineage.key.episode_generation,
                    act=progress.act,
                    floor=progress.floor,
                    is_combat=progress.is_combat,
                    strategic_context_kind=progress.strategic_context_kind,
                    terminal_broadcast=attempt_broadcast,
                    remaining_progress=remaining_floor_progress_return(
                        attempt,
                        progress,
                        config,
                    ),
                ),
            )
            if any(
                row.episode_seed != attempt.lineage.key.episode_seed
                for row in batch_rows
            ):
                raise CreditAssignmentError(
                    "decision-time seed disagrees with attempt lineage"
                )
            attempt_batches.append(batch_rows)
        if len(attempt_batches) != len(attempt.decisions):
            raise CreditAssignmentError(
                "public attempt progress rows disagree with its decision count"
            )
        aligned.append(tuple(attempt_batches))
    return normalized, tuple(aligned)


def _matched_floor_advantages(
    aligned: tuple[tuple[tuple[_DecisionCreditRow, ...], ...], ...],
) -> tuple[tuple[tuple[float, ...], ...], ...]:
    return _matched_group_advantages(aligned, lambda row: (row.act, row.floor))


def _matched_floor_context_advantages(
    aligned: tuple[tuple[tuple[_DecisionCreditRow, ...], ...], ...],
) -> tuple[tuple[tuple[float, ...], ...], ...]:
    return _matched_group_advantages(
        aligned,
        lambda row: (
            row.act,
            row.floor,
            row.is_combat,
            row.strategic_context_kind,
        ),
    )


def _matched_episode_floor_context_advantages(
    aligned: tuple[tuple[tuple[_DecisionCreditRow, ...], ...], ...],
) -> tuple[tuple[tuple[float, ...], ...], ...]:
    return _matched_group_advantages(
        aligned,
        lambda row: (
            row.episode_seed,
            row.episode_generation,
            row.act,
            row.floor,
            row.is_combat,
            row.strategic_context_kind,
        ),
    )


def _matched_group_advantages(
    aligned: tuple[tuple[tuple[_DecisionCreditRow, ...], ...], ...],
    group_key: Callable[[_DecisionCreditRow], Hashable],
) -> tuple[tuple[tuple[float, ...], ...], ...]:
    attempt_group_returns: dict[Hashable, dict[int, float]] = {}
    for attempt_index, attempt in enumerate(aligned):
        for batch in attempt:
            for row in batch:
                group = group_key(row)
                previous = attempt_group_returns.setdefault(
                    group,
                    {},
                ).setdefault(attempt_index, row.remaining_progress)
                if previous != row.remaining_progress:
                    raise CreditAssignmentError(
                        "one attempt has conflicting targets in one matched group"
                    )

    group_advantages: dict[tuple[int, Hashable], float] = {}
    for group, attempt_values in attempt_group_returns.items():
        if len(attempt_values) == 1:
            attempt_index = next(iter(attempt_values))
            group_advantages[(attempt_index, group)] = 0.0
            continue
        values = tuple(attempt_values.values())
        mean = math.fsum(values) / len(values)
        scale = len(values) / (len(values) - 1)
        for attempt_index, value in attempt_values.items():
            group_advantages[(attempt_index, group)] = scale * (value - mean)

    return tuple(
        tuple(
            tuple(
                group_advantages[(attempt_index, group_key(row))]
                for row in batch
            )
            for batch in attempt
        )
        for attempt_index, attempt in enumerate(aligned)
    )


def remaining_floor_progress_return(
    attempt: PublicAttemptTrajectoryV1,
    progress: DecisionRunProgress,
    config: FloorProgressReturnConfig,
) -> float:
    """Measure terminal progress relative to the decision's remaining horizon.

    Victory retains the reserved ``+1`` target. A defeat at the same effective
    floor as the decision is ``-1``; advancing through all remaining configured
    floors approaches, but never reaches, ``+1``.
    """

    if not isinstance(attempt, PublicAttemptTrajectoryV1):
        raise CreditAssignmentError(
            "remaining progress requires a public attempt trajectory"
        )
    if not isinstance(progress, DecisionRunProgress):
        raise CreditAssignmentError("remaining progress requires typed run progress")
    if not isinstance(config, FloorProgressReturnConfig):
        raise CreditAssignmentError("remaining progress requires a floor return config")
    if progress.episode_seed != attempt.lineage.key.episode_seed:
        raise CreditAssignmentError("decision progress seed changed within one attempt")
    if config.metric == "act_completion":
        return (floor_progress_terminal_return(attempt.terminal, config)
                - (progress.act - 1) / 4.0)
    if attempt.terminal.terminal_reward == 1:
        return 1.0

    ceiling = config.target_floor - 1
    start = min(progress.floor, ceiling)
    end = min(attempt.terminal.terminal.terminal_floor, ceiling)
    if end < start:
        raise CreditAssignmentError(
            "terminal floor precedes a retained decision floor"
        )
    remaining_horizon = config.target_floor - start
    return -1.0 + (2.0 * (end - start) / remaining_horizon)


def _distribution(values: Sequence[float]) -> DecisionCreditDistribution:
    normalized = tuple(float(value) for value in values)
    if not normalized or not all(math.isfinite(value) for value in normalized):
        raise CreditAssignmentError("credit distribution requires finite targets")
    negative = sum(value < 0.0 for value in normalized)
    zero = sum(value == 0.0 for value in normalized)
    positive = len(normalized) - negative - zero
    return DecisionCreditDistribution(
        decision_count=len(normalized),
        negative=negative,
        zero=zero,
        positive=positive,
        minimum=min(normalized),
        maximum=max(normalized),
        mean=math.fsum(normalized) / len(normalized),
    )
