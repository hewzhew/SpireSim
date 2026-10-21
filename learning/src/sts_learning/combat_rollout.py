"""Decision-local combat transitions with independent return-to-go axes."""

from __future__ import annotations

import math
import operator
from dataclasses import dataclass
from enum import IntEnum

from .combat_experience import (
    CombatDecisionProgress,
    CombatExperienceError,
    CompletedCombatGroupExperience,
)
from .combat_outcomes import CombatTerminalKind, CombatTerminalOutcome
from .policy import BehaviorManifestId, SelectionProbability


class CombatRolloutError(ValueError):
    """A complete combat group cannot form exact chronological transitions."""


class CombatRolloutAxis(IntEnum):
    """Fixed semantic meaning of one combat reward/value column."""

    WIN = 0
    PLAYER_HP_CHANGE = 1
    ENEMY_HP_CHANGE = 2


COMBAT_ROLLOUT_VALUE_HEAD_WIDTH = len(CombatRolloutAxis)


@dataclass(frozen=True)
class CombatRolloutRow:
    """One action-aligned transition and three uncombined return axes."""

    sequence_index: int
    progress: CombatDecisionProgress
    next_progress: CombatDecisionProgress | None
    terminal_outcome: CombatTerminalOutcome | None
    selected_ordinal: int
    selection_probability: SelectionProbability
    win_reward: float
    player_hp_change_reward: float
    enemy_hp_change_reward: float
    win_return_to_go: float
    player_hp_change_return_to_go: float
    enemy_hp_change_return_to_go: float

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "sequence_index",
            _integer(self.sequence_index, "sequence_index", minimum=0),
        )
        if not isinstance(self.progress, CombatDecisionProgress):
            raise CombatRolloutError("combat rollout progress must be typed")
        if (self.next_progress is None) == (self.terminal_outcome is None):
            raise CombatRolloutError(
                "combat rollout row requires exactly one next boundary"
            )
        if self.next_progress is not None:
            if not isinstance(self.next_progress, CombatDecisionProgress):
                raise CombatRolloutError("next combat progress must be typed")
            if self.next_progress.replicate_index != self.progress.replicate_index:
                raise CombatRolloutError("next combat progress changed replicate")
            if len(self.next_progress.potion_ids) != len(self.progress.potion_ids):
                raise CombatRolloutError("combat potion slot count changed")
        if self.terminal_outcome is not None:
            if not isinstance(self.terminal_outcome, CombatTerminalOutcome):
                raise CombatRolloutError("terminal combat outcome must be typed")
            if self.terminal_outcome.replicate_index != self.progress.replicate_index:
                raise CombatRolloutError("terminal combat outcome changed replicate")
            if len(self.terminal_outcome.final_potion_ids) != len(
                self.progress.potion_ids
            ):
                raise CombatRolloutError("terminal potion slot count changed")
        object.__setattr__(
            self,
            "selected_ordinal",
            _integer(self.selected_ordinal, "selected_ordinal", minimum=0),
        )
        if not isinstance(self.selection_probability, SelectionProbability):
            raise CombatRolloutError(
                "combat rollout selection probability must be typed"
            )
        for name in (
            "win_reward",
            "player_hp_change_reward",
            "enemy_hp_change_reward",
            "win_return_to_go",
            "player_hp_change_return_to_go",
            "enemy_hp_change_return_to_go",
        ):
            value = float(getattr(self, name))
            if not math.isfinite(value):
                raise CombatRolloutError(f"{name} must be finite")
            object.__setattr__(self, name, value)

    @property
    def terminal(self) -> bool:
        return self.terminal_outcome is not None

    @property
    def terminal_kind(self) -> CombatTerminalKind | None:
        return (
            None
            if self.terminal_outcome is None
            else self.terminal_outcome.terminal_kind
        )

    @property
    def after_player_hp(self) -> int:
        if self.next_progress is not None:
            return self.next_progress.player_hp
        assert self.terminal_outcome is not None
        return self.terminal_outcome.final_hp

    @property
    def after_enemy_hp(self) -> int:
        if self.next_progress is not None:
            return self.next_progress.enemy_hp
        assert self.terminal_outcome is not None
        return self.terminal_outcome.enemy_final_hp

    @property
    def after_potion_uuids(self) -> tuple[int | None, ...] | None:
        return (
            None
            if self.next_progress is None
            else self.next_progress.potion_uuids
        )

    @property
    def after_potion_ids(self) -> tuple[str | None, ...]:
        if self.next_progress is not None:
            return self.next_progress.potion_ids
        assert self.terminal_outcome is not None
        return self.terminal_outcome.final_potion_ids

    def reward(self, axis: CombatRolloutAxis) -> float:
        if not isinstance(axis, CombatRolloutAxis):
            raise CombatRolloutError("combat rollout reward axis must be typed")
        return (
            self.win_reward
            if axis is CombatRolloutAxis.WIN
            else self.player_hp_change_reward
            if axis is CombatRolloutAxis.PLAYER_HP_CHANGE
            else self.enemy_hp_change_reward
        )

    def return_to_go(self, axis: CombatRolloutAxis) -> float:
        if not isinstance(axis, CombatRolloutAxis):
            raise CombatRolloutError("combat rollout return axis must be typed")
        return (
            self.win_return_to_go
            if axis is CombatRolloutAxis.WIN
            else self.player_hp_change_return_to_go
            if axis is CombatRolloutAxis.PLAYER_HP_CHANGE
            else self.enemy_hp_change_return_to_go
        )


@dataclass(frozen=True)
class CombatReplicateRollout:
    """Chronological transitions for one exact same-root replicate."""

    replicate_index: int
    outcome: CombatTerminalOutcome
    rows: tuple[CombatRolloutRow, ...]

    def __post_init__(self) -> None:
        replicate_index = _integer(
            self.replicate_index,
            "replicate_index",
            minimum=0,
        )
        if not isinstance(self.outcome, CombatTerminalOutcome):
            raise CombatRolloutError("combat rollout outcome must be typed")
        if self.outcome.replicate_index != replicate_index:
            raise CombatRolloutError("combat rollout outcome changed replicate")
        rows = tuple(self.rows)
        if not rows or not all(isinstance(row, CombatRolloutRow) for row in rows):
            raise CombatRolloutError("combat replicate requires typed rollout rows")
        if any(row.progress.replicate_index != replicate_index for row in rows):
            raise CombatRolloutError("combat rollout rows changed replicate")
        sequence_indices = tuple(row.sequence_index for row in rows)
        if any(
            later <= earlier
            for earlier, later in zip(
                sequence_indices,
                sequence_indices[1:],
            )
        ):
            raise CombatRolloutError(
                "combat replicate sequence indices are not chronological"
            )
        if any(row.terminal for row in rows[:-1]) or not rows[-1].terminal:
            raise CombatRolloutError(
                "only the final combat rollout row may be terminal"
            )
        object.__setattr__(self, "replicate_index", replicate_index)
        object.__setattr__(self, "rows", rows)


@dataclass(frozen=True)
class CombatRolloutBatch:
    """Rollout rows in one original semantic model-call order."""

    sequence_index: int
    replicate_indices: tuple[int, ...]
    rows: tuple[CombatRolloutRow, ...]

    def __post_init__(self) -> None:
        sequence_index = _integer(
            self.sequence_index,
            "sequence_index",
            minimum=0,
        )
        replicate_indices = tuple(
            _integer(value, "replicate_index", minimum=0)
            for value in self.replicate_indices
        )
        rows = tuple(self.rows)
        if not rows or not all(isinstance(row, CombatRolloutRow) for row in rows):
            raise CombatRolloutError("combat rollout batch requires typed rows")
        if len(replicate_indices) != len(rows):
            raise CombatRolloutError("combat rollout batch rows are misaligned")
        if len(set(replicate_indices)) != len(replicate_indices):
            raise CombatRolloutError("combat rollout batch repeats a replicate")
        if any(row.sequence_index != sequence_index for row in rows):
            raise CombatRolloutError("combat rollout batch changed sequence")
        if tuple(row.progress.replicate_index for row in rows) != replicate_indices:
            raise CombatRolloutError("combat rollout batch changed replicate order")
        object.__setattr__(self, "sequence_index", sequence_index)
        object.__setattr__(self, "replicate_indices", replicate_indices)
        object.__setattr__(self, "rows", rows)

    def rewards(self, axis: CombatRolloutAxis) -> tuple[float, ...]:
        return tuple(row.reward(axis) for row in self.rows)

    def returns_to_go(self, axis: CombatRolloutAxis) -> tuple[float, ...]:
        return tuple(row.return_to_go(axis) for row in self.rows)


@dataclass(frozen=True)
class CompleteCombatGroupRollout:
    """Independent decision-local axes for every replicate in one exact group."""

    root_id: str
    exact_combat_state_hash: str
    behavior_manifest_id: BehaviorManifestId
    replicates: tuple[CombatReplicateRollout, ...]
    batches: tuple[CombatRolloutBatch, ...]
    decision_count: int

    def __post_init__(self) -> None:
        if not isinstance(self.behavior_manifest_id, BehaviorManifestId):
            raise CombatRolloutError("combat rollout manifest identity must be typed")
        replicates = tuple(self.replicates)
        if len(replicates) < 2 or not all(
            isinstance(replicate, CombatReplicateRollout)
            for replicate in replicates
        ):
            raise CombatRolloutError(
                "complete combat rollout requires typed same-root replicates"
            )
        if tuple(row.replicate_index for row in replicates) != tuple(
            range(len(replicates))
        ):
            raise CombatRolloutError(
                "complete combat rollout replicates must be contiguous"
            )
        batches = tuple(self.batches)
        if not batches or not all(
            isinstance(batch, CombatRolloutBatch) for batch in batches
        ):
            raise CombatRolloutError(
                "complete combat rollout requires typed model-call batches"
            )
        if tuple(batch.sequence_index for batch in batches) != tuple(
            range(len(batches))
        ):
            raise CombatRolloutError("combat rollout batch sequence is not contiguous")
        decision_count = _integer(
            self.decision_count,
            "decision_count",
            minimum=1,
        )
        if decision_count != sum(len(row.rows) for row in replicates):
            raise CombatRolloutError("combat rollout decision count is misaligned")
        if decision_count != sum(len(batch.rows) for batch in batches):
            raise CombatRolloutError("combat rollout batch count is misaligned")
        replicate_keys = {
            (row.sequence_index, row.progress.replicate_index)
            for replicate in replicates
            for row in replicate.rows
        }
        batch_keys = {
            (row.sequence_index, row.progress.replicate_index)
            for batch in batches
            for row in batch.rows
        }
        if batch_keys != replicate_keys or len(batch_keys) != decision_count:
            raise CombatRolloutError(
                "combat rollout batch and replicate projections disagree"
            )
        object.__setattr__(self, "replicates", replicates)
        object.__setattr__(self, "batches", batches)
        object.__setattr__(self, "decision_count", decision_count)


def build_complete_combat_rollout(
    experience: CompletedCombatGroupExperience,
) -> CompleteCombatGroupRollout:
    """Decompose each decision into exact adjacent public-state changes.

    The three numeric axes remain independent. Win is sparse at the terminal
    transition. HP change is normalized by root max HP; enemy change is
    normalized by exact root enemy HP. Potion identities are retained only as
    typed before/after facts and never converted into a scalar value.
    """

    if not isinstance(experience, CompletedCombatGroupExperience):
        raise CombatRolloutError("combat rollout requires complete experience")
    replicates = tuple(
        _build_replicate_rollout(experience, outcome)
        for outcome in experience.outcomes.outcomes
    )
    row_by_key = {
        (row.sequence_index, row.progress.replicate_index): row
        for replicate in replicates
        for row in replicate.rows
    }
    batches = tuple(
        CombatRolloutBatch(
            sequence_index=batch.sequence_index,
            replicate_indices=batch.replicate_indices,
            rows=tuple(
                row_by_key[(batch.sequence_index, replicate_index)]
                for replicate_index in batch.replicate_indices
            ),
        )
        for batch in experience.batches
    )
    rollout = CompleteCombatGroupRollout(
        root_id=experience.root_id,
        exact_combat_state_hash=experience.exact_combat_state_hash,
        behavior_manifest_id=experience.behavior_manifest_id,
        replicates=replicates,
        batches=batches,
        decision_count=experience.decision_count,
    )
    if rollout.decision_count != experience.decision_count:
        raise CombatRolloutError("combat rollout lost retained decisions")
    return rollout


def _build_replicate_rollout(
    experience: CompletedCombatGroupExperience,
    outcome: CombatTerminalOutcome,
) -> CombatReplicateRollout:
    try:
        steps = experience.chronological_steps(outcome.replicate_index)
    except CombatExperienceError as error:
        raise CombatRolloutError(str(error)) from error
    first = steps[0].progress
    if first.enemy_hp != outcome.enemy_start_hp:
        raise CombatRolloutError(
            "first decision enemy HP disagrees with the exact combat root"
        )
    root_player_max_hp = first.player_max_hp
    root_enemy_hp = outcome.enemy_start_hp
    if any(
        len(step.progress.potion_ids) != len(outcome.final_potion_ids)
        for step in steps
    ):
        raise CombatRolloutError("combat potion slot count changed across rollout")

    next_progress = tuple(
        steps[row_index + 1].progress
        if row_index + 1 < len(steps)
        else None
        for row_index in range(len(steps))
    )
    win_rewards = tuple(
        1.0 if row_index + 1 == len(steps) and outcome.won else 0.0
        for row_index in range(len(steps))
    )
    hp_rewards = tuple(
        (
            (next_row.player_hp if next_row is not None else outcome.final_hp)
            - step.progress.player_hp
        )
        / root_player_max_hp
        for step, next_row in zip(steps, next_progress, strict=True)
    )
    enemy_rewards = tuple(
        (
            step.progress.enemy_hp
            - (
                next_row.enemy_hp
                if next_row is not None
                else outcome.enemy_final_hp
            )
        )
        / root_enemy_hp
        for step, next_row in zip(steps, next_progress, strict=True)
    )
    win_returns = _reverse_returns(win_rewards)
    hp_returns = _reverse_returns(hp_rewards)
    enemy_returns = _reverse_returns(enemy_rewards)

    rows = tuple(
        CombatRolloutRow(
            sequence_index=step.sequence_index,
            progress=step.progress,
            next_progress=next_row,
            terminal_outcome=(outcome if next_row is None else None),
            selected_ordinal=step.selected_ordinal,
            selection_probability=step.selection_probability,
            win_reward=win_reward,
            player_hp_change_reward=hp_reward,
            enemy_hp_change_reward=enemy_reward,
            win_return_to_go=win_return,
            player_hp_change_return_to_go=hp_return,
            enemy_hp_change_return_to_go=enemy_return,
        )
        for (
            step,
            next_row,
            win_reward,
            hp_reward,
            enemy_reward,
            win_return,
            hp_return,
            enemy_return,
        ) in zip(
            steps,
            next_progress,
            win_rewards,
            hp_rewards,
            enemy_rewards,
            win_returns,
            hp_returns,
            enemy_returns,
            strict=True,
        )
    )
    _require_close(
        math.fsum(win_rewards),
        1.0 if outcome.won else 0.0,
        "win reward conservation",
    )
    _require_close(
        math.fsum(hp_rewards),
        (outcome.final_hp - first.player_hp) / root_player_max_hp,
        "player HP reward conservation",
    )
    _require_close(
        math.fsum(enemy_rewards),
        (outcome.enemy_start_hp - outcome.enemy_final_hp) / root_enemy_hp,
        "enemy HP reward conservation",
    )
    return CombatReplicateRollout(
        replicate_index=outcome.replicate_index,
        outcome=outcome,
        rows=rows,
    )


def _reverse_returns(rewards: tuple[float, ...]) -> tuple[float, ...]:
    returns = [0.0] * len(rewards)
    remaining = 0.0
    for index in range(len(rewards) - 1, -1, -1):
        remaining = rewards[index] + remaining
        returns[index] = remaining
    return tuple(returns)


def _integer(value: object, name: str, *, minimum: int) -> int:
    if isinstance(value, bool):
        raise CombatRolloutError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatRolloutError(f"{name} must be an integer") from error
    if normalized < minimum:
        raise CombatRolloutError(f"{name} must be at least {minimum}")
    return normalized


def _require_close(actual: float, expected: float, name: str) -> None:
    if not math.isclose(actual, expected, rel_tol=0.0, abs_tol=1e-12):
        raise CombatRolloutError(f"{name} failed")
