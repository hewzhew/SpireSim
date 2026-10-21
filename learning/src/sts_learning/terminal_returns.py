"""Typed terminal returns for sparse whole-run policy training."""

from __future__ import annotations

import math
import operator
from collections.abc import Sequence
from dataclasses import dataclass
from enum import IntEnum
from numbers import Real

from .recovery import TerminalAttemptRecord


class TerminalReturnError(ValueError):
    """A terminal outcome or return profile is malformed."""


def _finite_float(value: object, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, Real):
        raise TerminalReturnError(f"{name} must be a real number")
    normalized = float(value)
    if not math.isfinite(normalized):
        raise TerminalReturnError(f"{name} must be finite")
    return normalized


def _optional_positive_float(value: object, name: str) -> float | None:
    if value is None:
        return None
    normalized = _finite_float(value, name)
    if normalized <= 0.0:
        raise TerminalReturnError(f"{name} must be positive when present")
    return normalized


class TerminalAdvantageMode(IntEnum):
    """How one complete-attempt batch turns terminal returns into advantages."""

    RAW_RETURN = 0
    LEAVE_ONE_OUT = 1
    MATCHED_FLOOR_LEAVE_ONE_OUT = 2
    MATCHED_FLOOR_CONTEXT_LEAVE_ONE_OUT = 3
    MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT = 4
    DECISION_LOCAL_GAE = 5


class RunDecisionScope(IntEnum):
    """Which whole-run decision rows receive the terminal objective."""

    ALL = 0
    STRATEGIC = 1


class RunPolicyUpdateRule(IntEnum):
    """How one frozen whole-run attempt batch changes the policy."""

    REINFORCE = 0
    PPO_CLIP_VALUE = 1
    CRITIC_CALIBRATION = 2


@dataclass(frozen=True)
class RunPolicyUpdateConfig:
    """Bounded policy optimization over one frozen whole-run batch."""

    rule: RunPolicyUpdateRule = RunPolicyUpdateRule.REINFORCE
    epochs: int = 1
    clip_coefficient: float = 0.2
    entropy_coefficient: float = 0.0
    max_grad_norm: float | None = None
    target_kl: float | None = None
    value_loss_coefficient: float = 0.0
    normalize_advantage: bool = False
    value_clip_coefficient: float | None = None
    value_prediction_target: str = "return_to_go"

    @classmethod
    def ppo_clip_value(cls) -> RunPolicyUpdateConfig:
        return cls(
            rule=RunPolicyUpdateRule.PPO_CLIP_VALUE,
            epochs=4,
            entropy_coefficient=0.01,
            max_grad_norm=0.5,
            target_kl=0.02,
            value_loss_coefficient=0.5,
            normalize_advantage=True,
            value_clip_coefficient=0.2,
        )

    @classmethod
    def critic_calibration(cls, *, steps: int = 256) -> RunPolicyUpdateConfig:
        """Fit only the value head while leaving every actor tensor fixed."""

        return cls(
            rule=RunPolicyUpdateRule.CRITIC_CALIBRATION,
            epochs=steps,
            value_loss_coefficient=1.0,
        )

    @property
    def uses_value_baseline(self) -> bool:
        return self.rule is not RunPolicyUpdateRule.REINFORCE

    @property
    def updates_actor(self) -> bool:
        return self.rule is not RunPolicyUpdateRule.CRITIC_CALIBRATION

    def __post_init__(self) -> None:
        if not isinstance(self.rule, RunPolicyUpdateRule):
            raise TerminalReturnError("run policy update rule must be typed")
        if isinstance(self.epochs, bool):
            raise TerminalReturnError("run policy update epochs must be an integer")
        try:
            epochs = operator.index(self.epochs)
        except TypeError as error:
            raise TerminalReturnError(
                "run policy update epochs must be an integer"
            ) from error
        if epochs <= 0:
            raise TerminalReturnError("run policy update epochs must be positive")
        maximum_epochs = (
            1024
            if self.rule is RunPolicyUpdateRule.CRITIC_CALIBRATION
            else 64
        )
        if epochs > maximum_epochs:
            raise TerminalReturnError(
                f"run policy update epochs must not exceed {maximum_epochs}"
            )
        clip = _finite_float(self.clip_coefficient, "clip_coefficient")
        entropy = _finite_float(
            self.entropy_coefficient,
            "entropy_coefficient",
        )
        value_loss = _finite_float(
            self.value_loss_coefficient,
            "value_loss_coefficient",
        )
        if type(self.normalize_advantage) is not bool:
            raise TerminalReturnError("normalize_advantage must be bool")
        if self.value_prediction_target not in ("return_to_go", "terminal_score"):
            raise TerminalReturnError("unknown run value prediction target")
        max_grad_norm = _optional_positive_float(
            self.max_grad_norm,
            "max_grad_norm",
        )
        target_kl = _optional_positive_float(self.target_kl, "target_kl")
        value_clip = _optional_positive_float(
            self.value_clip_coefficient,
            "value_clip_coefficient",
        )
        if not 0.0 < clip < 1.0:
            raise TerminalReturnError("clip_coefficient must be in (0, 1)")
        if entropy < 0.0:
            raise TerminalReturnError("entropy_coefficient must be non-negative")
        if value_loss < 0.0:
            raise TerminalReturnError("value_loss_coefficient must be non-negative")
        if self.rule is RunPolicyUpdateRule.REINFORCE and (
            epochs != 1
            or entropy != 0.0
            or max_grad_norm is not None
            or target_kl is not None
            or value_loss != 0.0
            or self.normalize_advantage
            or value_clip is not None
            or self.value_prediction_target != "return_to_go"
        ):
            raise TerminalReturnError(
                "run REINFORCE requires one epoch and no PPO regularization"
            )
        if (
            self.uses_value_baseline and value_loss <= 0.0
        ):
            raise TerminalReturnError(
                "run value training requires a positive value_loss_coefficient"
            )
        if self.rule is RunPolicyUpdateRule.CRITIC_CALIBRATION and (
            entropy != 0.0
            or target_kl is not None
            or self.normalize_advantage
        ):
            raise TerminalReturnError(
                "run critic calibration cannot configure actor regularization"
            )
        object.__setattr__(self, "epochs", epochs)
        object.__setattr__(self, "clip_coefficient", clip)
        object.__setattr__(self, "entropy_coefficient", entropy)
        object.__setattr__(self, "max_grad_norm", max_grad_norm)
        object.__setattr__(self, "target_kl", target_kl)
        object.__setattr__(self, "value_loss_coefficient", value_loss)
        object.__setattr__(self, "value_clip_coefficient", value_clip)


@dataclass(frozen=True)
class FloorProgressReturnConfig:
    """Run return: historical floor progress or actual completed acts.

    Act completion counts transitions to the next act plus a final victory;
    walking farther inside an act gives no additional reward.
    """

    target_floor: int = 52
    metric: str = "floor_progress"

    def __post_init__(self) -> None:
        if self.metric not in ("floor_progress", "act_completion"):
            raise TerminalReturnError("unknown run return metric")
        if isinstance(self.target_floor, bool):
            raise TerminalReturnError("target_floor must be an integer, not bool")
        try:
            target = operator.index(self.target_floor)
        except TypeError as error:
            raise TerminalReturnError("target_floor must be an integer") from error
        if target < 2:
            raise TerminalReturnError("target_floor must be at least two")
        object.__setattr__(self, "target_floor", target)


@dataclass(frozen=True)
class OnPolicyObjectiveConfig:
    """Exact terminal objective and complete-attempt update size."""

    terminal_return: FloorProgressReturnConfig = FloorProgressReturnConfig()
    attempts_per_update: int = 8
    advantage_mode: TerminalAdvantageMode = TerminalAdvantageMode.RAW_RETURN
    decision_scope: RunDecisionScope = RunDecisionScope.ALL
    policy_update: RunPolicyUpdateConfig = RunPolicyUpdateConfig()

    def __post_init__(self) -> None:
        if not isinstance(self.terminal_return, FloorProgressReturnConfig):
            raise TerminalReturnError("terminal_return must be typed")
        if isinstance(self.attempts_per_update, bool):
            raise TerminalReturnError(
                "attempts_per_update must be an integer, not bool"
            )
        try:
            attempts = operator.index(self.attempts_per_update)
        except TypeError as error:
            raise TerminalReturnError(
                "attempts_per_update must be an integer"
            ) from error
        if attempts <= 0:
            raise TerminalReturnError("attempts_per_update must be positive")
        if not isinstance(self.advantage_mode, TerminalAdvantageMode):
            raise TerminalReturnError(
                "advantage_mode must be TerminalAdvantageMode"
            )
        if not isinstance(self.decision_scope, RunDecisionScope):
            raise TerminalReturnError("decision_scope must be RunDecisionScope")
        if not isinstance(self.policy_update, RunPolicyUpdateConfig):
            raise TerminalReturnError("policy_update must be RunPolicyUpdateConfig")
        if (
            self.policy_update.uses_value_baseline
            and self.advantage_mode is not TerminalAdvantageMode.DECISION_LOCAL_GAE
        ):
            raise TerminalReturnError(
                "run value PPO requires decision-local GAE advantage"
            )
        if (
            not self.policy_update.uses_value_baseline
            and self.advantage_mode is TerminalAdvantageMode.DECISION_LOCAL_GAE
        ):
            raise TerminalReturnError(
                "decision-local GAE advantage requires run value PPO"
            )
        if (
            self.advantage_mode
            in (
                TerminalAdvantageMode.LEAVE_ONE_OUT,
                TerminalAdvantageMode.MATCHED_FLOOR_LEAVE_ONE_OUT,
                TerminalAdvantageMode.MATCHED_FLOOR_CONTEXT_LEAVE_ONE_OUT,
                TerminalAdvantageMode.MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT,
            )
            and attempts < 2
        ):
            raise TerminalReturnError(
                "leave-one-out advantage requires at least two attempts per update"
            )
        object.__setattr__(self, "attempts_per_update", attempts)


def terminal_return_advantages(
    returns: Sequence[float],
    mode: TerminalAdvantageMode,
) -> tuple[float, ...]:
    """Convert one independent attempt batch into typed policy advantages."""

    if not isinstance(mode, TerminalAdvantageMode):
        raise TerminalReturnError("advantage mode must be TerminalAdvantageMode")
    normalized: list[float] = []
    for value in returns:
        if isinstance(value, bool) or not isinstance(value, Real):
            raise TerminalReturnError("terminal returns must be real numbers")
        number = float(value)
        if not math.isfinite(number):
            raise TerminalReturnError("terminal returns must be finite")
        normalized.append(number)
    if not normalized:
        raise TerminalReturnError("advantage calculation requires terminal returns")
    if mode is TerminalAdvantageMode.RAW_RETURN:
        return tuple(normalized)
    if mode in (
        TerminalAdvantageMode.MATCHED_FLOOR_LEAVE_ONE_OUT,
        TerminalAdvantageMode.MATCHED_FLOOR_CONTEXT_LEAVE_ONE_OUT,
        TerminalAdvantageMode.MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT,
        TerminalAdvantageMode.DECISION_LOCAL_GAE,
    ):
        raise TerminalReturnError(
            "selected advantage requires complete decision-time rollout data"
        )
    if len(normalized) < 2:
        raise TerminalReturnError(
            "leave-one-out advantage requires at least two terminal returns"
        )
    batch_mean = math.fsum(normalized) / len(normalized)
    scale = len(normalized) / (len(normalized) - 1)
    return tuple(scale * (value - batch_mean) for value in normalized)


def floor_progress_terminal_return(
    attempt: TerminalAttemptRecord,
    config: FloorProgressReturnConfig,
) -> float:
    """Return ``1`` for victory and a bounded floor signal for defeat."""

    if not isinstance(attempt, TerminalAttemptRecord):
        raise TerminalReturnError("terminal return requires a terminal attempt record")
    if not isinstance(config, FloorProgressReturnConfig):
        raise TerminalReturnError("terminal return requires a floor-progress config")
    if config.metric == "act_completion":
        act = attempt.terminal.terminal_act
        if not 1 <= act <= 4:
            raise TerminalReturnError("act completion requires terminal act 1..4")
        return (act - 1 + int(attempt.terminal_reward == 1)) / 4.0
    if attempt.terminal_reward == 1:
        return 1.0
    capped_floor = min(attempt.terminal.terminal_floor, config.target_floor - 1)
    return -1.0 + (2.0 * capped_floor / config.target_floor)
