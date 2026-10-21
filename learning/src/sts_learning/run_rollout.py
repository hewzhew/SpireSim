"""Typed complete-attempt rollouts with decision-local returns.

The reverse advantage recurrence is adapted from Stable-Baselines3 2.9.0's
``RolloutBuffer.compute_returns_and_advantage`` (MIT license).  This version
operates on complete, ragged Slay the Spire attempts: episodes are never
bootstrapped and action time is intentionally undiscounted. Policy terms sum
within each attempt and average across attempts; auxiliary value errors keep
equal total weight per attempt. See ``THIRD_PARTY_NOTICES.md``.
"""

from __future__ import annotations

import math
from collections.abc import Sequence
from dataclasses import dataclass
from numbers import Real

from .decision_progress import DecisionRunProgress
from .experience import DecisionLineage
from .policy import BehaviorManifestId, SelectionProbability
from .public_trajectory import PublicAttemptTrajectoryV1
from .recovery import TerminalAttemptRecord
from .terminal_returns import (
    FloorProgressReturnConfig,
    floor_progress_terminal_return,
)


class RunRolloutError(ValueError):
    """Complete-attempt experience cannot form one chronological rollout."""


@dataclass(frozen=True)
class RunRolloutConfig:
    """Fixed first-stage whole-run return profile.

    A game decision is not a stable unit of elapsed time: one row may play a
    card while another chooses a route or shop purchase.  Discounting by row
    count would therefore invent a time scale.  The first migration keeps the
    exact Monte-Carlo profile ``gamma = gae_lambda = 1``.
    """

    gamma: float = 1.0
    gae_lambda: float = 1.0

    def __post_init__(self) -> None:
        gamma = _finite_float(self.gamma, "gamma")
        gae_lambda = _finite_float(self.gae_lambda, "gae_lambda")
        if gamma != 1.0 or gae_lambda != 1.0:
            raise RunRolloutError(
                "whole-run rollout currently requires gamma=1 and gae_lambda=1"
            )
        object.__setattr__(self, "gamma", gamma)
        object.__setattr__(self, "gae_lambda", gae_lambda)


@dataclass(frozen=True)
class RunRolloutRow:
    """One exact policy row plus its post-decision reward and return."""

    batch_index: int
    lineage: DecisionLineage
    progress: DecisionRunProgress
    behavior_manifest_id: BehaviorManifestId
    selected_ordinal: int
    selection_probability: SelectionProbability
    candidate_count: int
    reward: float
    return_to_go: float
    actor_eligible: bool
    actor_weight: float
    value_weight: float


@dataclass(frozen=True)
class RunAttemptRollout:
    """Chronological rows for one terminal attempt.

    ``prefix_reward`` is progress that happened before the first retained
    policy row.  It participates in conservation of the historical terminal
    objective, but never enters an action return or actor weight.
    """

    lineage: DecisionLineage
    terminal: TerminalAttemptRecord
    prefix_reward: float
    terminal_adjustment: float
    terminal_return: float
    rows: tuple[RunRolloutRow, ...]

    @property
    def total_reward(self) -> float:
        return self.prefix_reward + math.fsum(row.reward for row in self.rows)


@dataclass(frozen=True)
class CompleteRunRollout:
    """A bounded complete-attempt batch without tensor or optimizer ownership."""

    attempts: tuple[RunAttemptRollout, ...]
    decision_count: int
    actor_decision_count: int


@dataclass(frozen=True)
class EvaluatedRunRollout:
    """GAE/return columns aligned to a typed complete rollout."""

    rollout: CompleteRunRollout
    value_predictions: tuple[tuple[float, ...], ...]
    advantages: tuple[tuple[float, ...], ...]
    returns: tuple[tuple[float, ...], ...]


def decision_progress_potential(
    progress: DecisionRunProgress,
    return_config: FloorProgressReturnConfig,
) -> float:
    """Already earned public progress, excluded from an action's future return."""

    if not isinstance(progress, DecisionRunProgress) or not isinstance(
        return_config, FloorProgressReturnConfig
    ):
        raise RunRolloutError("progress potential requires typed public progress")
    if return_config.metric == "act_completion":
        if not 1 <= progress.act <= 4:
            raise RunRolloutError("act completion requires act 1..4")
        return (progress.act - 1) / 4.0
    return (
        2.0 * min(progress.floor, return_config.target_floor - 1)
        / return_config.target_floor
    )


def build_complete_run_rollout(
    attempts: Sequence[PublicAttemptTrajectoryV1],
    return_config: FloorProgressReturnConfig,
) -> CompleteRunRollout:
    """Decompose terminal progress into exact decision-local rewards.

    Floor progress is credited to the action whose transition reaches the next
    observed floor.  Repeated same-floor decisions receive zero progress.
    Defeat contributes ``-1`` at the terminal transition; victory contributes
    the exact adjustment needed to keep the historical reserved ``+1`` return.
    """

    normalized = tuple(attempts)
    if not normalized:
        raise RunRolloutError("run rollout requires complete attempts")
    if not isinstance(return_config, FloorProgressReturnConfig):
        raise RunRolloutError("run rollout requires a floor-progress config")
    if not all(
        isinstance(attempt, PublicAttemptTrajectoryV1) for attempt in normalized
    ):
        raise RunRolloutError("run rollout accepts only public attempt trajectories")

    attempt_count = len(normalized)
    rollouts: list[RunAttemptRollout] = []
    decision_count = 0
    actor_decision_count = 0
    for attempt in normalized:
        base_rows = _attempt_rows(attempt)
        value_weight = 1.0 / (attempt_count * len(base_rows))
        # The likelihood of a trajectory is the product of its action
        # probabilities: its score gradient is their SUM. Dividing by the
        # realized trajectory length changes the expected-return objective.
        actor_weight = 1.0 / attempt_count

        ceiling = return_config.target_floor - 1
        effective_floors = tuple(
            min(row[1].floor, ceiling) for row in base_rows
        )
        terminal_floor = attempt.terminal.terminal.terminal_floor
        if terminal_floor < base_rows[-1][1].floor:
            raise RunRolloutError(
                "terminal floor precedes the final retained decision floor"
            )
        effective_terminal_floor = min(terminal_floor, ceiling)
        prefix_reward = decision_progress_potential(base_rows[0][1], return_config)
        terminal_adjustment = _terminal_adjustment(
            attempt.terminal,
            effective_terminal_floor,
            return_config,
        )

        rewards: list[float] = []
        for row_index, effective_floor in enumerate(effective_floors):
            next_floor = (
                effective_terminal_floor
                if row_index + 1 == len(effective_floors)
                else effective_floors[row_index + 1]
            )
            reward = 2.0 * (next_floor - effective_floor) / return_config.target_floor
            if row_index + 1 == len(effective_floors):
                reward += terminal_adjustment
            rewards.append(reward)

        if return_config.metric == "act_completion":
            acts = [row[1].act for row in base_rows]
            final_act = attempt.terminal.terminal.terminal_act
            if any(not 1 <= act <= 4 for act in [*acts, final_act]):
                raise RunRolloutError("act completion requires act 1..4")
            if any(right < left for left, right in zip(acts, [*acts[1:], final_act])):
                raise RunRolloutError("run act regressed within an attempt")
            terminal_adjustment = float(attempt.terminal.terminal_reward == 1) / 4.0
            rewards = [(right - left) / 4.0 for left, right in zip(acts, [*acts[1:], final_act])]
            rewards[-1] += terminal_adjustment

        return_to_go = _reverse_returns(rewards)
        rows = tuple(
            RunRolloutRow(
                batch_index=batch_index,
                lineage=lineage,
                progress=progress,
                behavior_manifest_id=manifest_id,
                selected_ordinal=selected_ordinal,
                selection_probability=selection_probability,
                candidate_count=candidate_count,
                reward=reward,
                return_to_go=remaining_return,
                actor_eligible=candidate_count > 1,
                actor_weight=(actor_weight if candidate_count > 1 else 0.0),
                value_weight=value_weight,
            )
            for (
                batch_index,
                progress,
                lineage,
                manifest_id,
                candidate_count,
                selected_ordinal,
                selection_probability,
            ), reward, remaining_return in zip(
                base_rows,
                rewards,
                return_to_go,
                strict=True,
            )
        )
        terminal_return = floor_progress_terminal_return(
            attempt.terminal,
            return_config,
        )
        rollout = RunAttemptRollout(
            lineage=attempt.lineage,
            terminal=attempt.terminal,
            prefix_reward=prefix_reward,
            terminal_adjustment=terminal_adjustment,
            terminal_return=terminal_return,
            rows=rows,
        )
        if not math.isclose(
            rollout.total_reward,
            terminal_return,
            rel_tol=0.0,
            abs_tol=1e-12,
        ):
            raise RunRolloutError(
                "decision-local rewards do not conserve the terminal return"
            )
        rollouts.append(rollout)
        decision_count += len(rows)
        actor_decision_count += sum(row.actor_eligible for row in rows)

    return CompleteRunRollout(
        attempts=tuple(rollouts),
        decision_count=decision_count,
        actor_decision_count=actor_decision_count,
    )


def compute_complete_run_gae(
    rollout: CompleteRunRollout,
    value_predictions: Sequence[Sequence[float]],
    config: RunRolloutConfig | None = None,
) -> EvaluatedRunRollout:
    """Compute terminal, non-bootstrapped GAE and lambda returns.

    This is the SB3 reverse recurrence specialized to independently completed
    attempts.  With the maintained ``gamma=lambda=1`` profile, returns equal
    the exact decision-local Monte-Carlo return-to-go regardless of critic
    prediction.
    """

    if not isinstance(rollout, CompleteRunRollout):
        raise RunRolloutError("GAE requires a typed complete run rollout")
    if config is None:
        config = RunRolloutConfig()
    if not isinstance(config, RunRolloutConfig):
        raise RunRolloutError("GAE requires a typed rollout config")
    try:
        prediction_attempts = tuple(tuple(values) for values in value_predictions)
    except TypeError as error:
        raise RunRolloutError("value predictions must be attempt-aligned") from error
    if len(prediction_attempts) != len(rollout.attempts):
        raise RunRolloutError("value predictions disagree with rollout attempts")

    normalized_predictions: list[tuple[float, ...]] = []
    advantages: list[tuple[float, ...]] = []
    returns: list[tuple[float, ...]] = []
    for attempt, raw_values in zip(
        rollout.attempts,
        prediction_attempts,
        strict=True,
    ):
        if len(raw_values) != len(attempt.rows):
            raise RunRolloutError("value predictions disagree with rollout rows")
        values = tuple(
            _finite_float(value, "value prediction") for value in raw_values
        )
        attempt_advantages = [0.0] * len(attempt.rows)
        last_gae_lambda = 0.0
        for row_index in range(len(attempt.rows) - 1, -1, -1):
            terminal = row_index + 1 == len(attempt.rows)
            next_non_terminal = 0.0 if terminal else 1.0
            next_value = 0.0 if terminal else values[row_index + 1]
            delta = (
                attempt.rows[row_index].reward
                + config.gamma * next_value * next_non_terminal
                - values[row_index]
            )
            last_gae_lambda = delta + (
                config.gamma
                * config.gae_lambda
                * next_non_terminal
                * last_gae_lambda
            )
            attempt_advantages[row_index] = last_gae_lambda
        attempt_returns = tuple(
            advantage + value
            for advantage, value in zip(
                attempt_advantages,
                values,
                strict=True,
            )
        )
        if not all(
            math.isclose(
                actual,
                row.return_to_go,
                rel_tol=0.0,
                abs_tol=1e-12,
            )
            for actual, row in zip(attempt_returns, attempt.rows, strict=True)
        ):
            raise RunRolloutError("Monte-Carlo GAE disagrees with return-to-go")
        normalized_predictions.append(values)
        advantages.append(tuple(attempt_advantages))
        returns.append(attempt_returns)

    return EvaluatedRunRollout(
        rollout=rollout,
        value_predictions=tuple(normalized_predictions),
        advantages=tuple(advantages),
        returns=tuple(returns),
    )


def _attempt_rows(
    attempt: PublicAttemptTrajectoryV1,
) -> tuple[
    tuple[
        int,
        DecisionRunProgress,
        DecisionLineage,
        BehaviorManifestId,
        int,
        int,
        SelectionProbability,
    ],
    ...,
]:
    if not attempt.decisions:
        raise RunRolloutError("public attempt has no policy decisions")
    terminal = attempt.terminal
    key = attempt.lineage.key
    if (
        terminal.slot_index != key.slot_index
        or terminal.episode_seed != key.episode_seed
        or terminal.episode_generation != key.episode_generation
        or terminal.attempt_index != key.attempt_index
        or terminal.recoveries_used != attempt.lineage.recoveries_used
    ):
        raise RunRolloutError("terminal record disagrees with attempt lineage")

    rows = []
    previous_floor = -1
    previous_act = -1
    for batch_index, decision in enumerate(attempt.decisions):
        if decision.chronological_index != batch_index:
            raise RunRolloutError("public decisions are not chronological")
        if decision.lineage != attempt.lineage:
            raise RunRolloutError("decision row disagrees with attempt lineage")
        progress = decision.run_progress
        if progress.episode_seed != key.episode_seed:
            raise RunRolloutError("decision progress seed changed within one attempt")
        if progress.floor < previous_floor:
            raise RunRolloutError("decision floors are not chronological")
        if progress.act < previous_act:
            raise RunRolloutError("decision acts are not chronological")
        previous_floor = progress.floor
        previous_act = progress.act
        candidate_count = len(decision.public_snapshot.candidate_ids)
        if candidate_count <= 0:
            raise RunRolloutError("decision row must have a legal candidate")
        selected_ordinal = decision.selected_ordinal
        if not 0 <= selected_ordinal < candidate_count:
            raise RunRolloutError("selected ordinal is outside the ragged candidate row")
        selection_probability = decision.selection_probability
        if not isinstance(selection_probability, SelectionProbability):
            raise RunRolloutError("selection probability must be typed")
        if not isinstance(decision.behavior_manifest_id, BehaviorManifestId):
            raise RunRolloutError("behavior manifest identity must be typed")
        rows.append(
            (
                batch_index,
                progress,
                decision.lineage,
                decision.behavior_manifest_id,
                candidate_count,
                selected_ordinal,
                selection_probability,
            )
        )
    if attempt.terminal.terminal.terminal_act < previous_act:
        raise RunRolloutError("terminal act precedes the final decision act")
    return tuple(rows)


def _terminal_adjustment(
    terminal: TerminalAttemptRecord,
    effective_terminal_floor: int,
    config: FloorProgressReturnConfig,
) -> float:
    if terminal.terminal_reward == -1:
        return -1.0
    return -1.0 + (
        2.0
        * (config.target_floor - effective_terminal_floor)
        / config.target_floor
    )


def _reverse_returns(rewards: Sequence[float]) -> tuple[float, ...]:
    remaining = 0.0
    reversed_returns = []
    for reward in reversed(tuple(rewards)):
        remaining = float(reward) + remaining
        reversed_returns.append(remaining)
    return tuple(reversed(reversed_returns))


def _finite_float(value: object, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, Real):
        raise RunRolloutError(f"{name} must be a real number")
    normalized = float(value)
    if not math.isfinite(normalized):
        raise RunRolloutError(f"{name} must be finite")
    return normalized
