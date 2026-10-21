"""Typed configuration for same-root combat learning objectives."""

from __future__ import annotations

import math
import operator
from dataclasses import dataclass, field
from enum import IntEnum
from numbers import Real


class CombatObjectiveError(ValueError):
    """A combat objective configuration is malformed."""


class CombatAllWinAxis(IntEnum):
    """Optional learning axis after every same-root replicate wins."""

    NONE = 0
    TERMINAL_HP = 1


class CombatAllLossAxis(IntEnum):
    """Optional learning axis when every same-root replicate truly loses."""

    NONE = 0
    ENEMY_HP_PROGRESS = 1


class CombatPolicyUpdateRule(IntEnum):
    """How one frozen combat batch changes the policy."""

    REINFORCE = 0
    PPO_CLIP = 1
    PPO_CLIP_VALUE = 2


@dataclass(frozen=True)
class CombatPolicyUpdateConfig:
    """Bounded policy optimization over one frozen-behavior combat batch."""

    rule: CombatPolicyUpdateRule = CombatPolicyUpdateRule.REINFORCE
    epochs: int = 1
    clip_coefficient: float = 0.2
    entropy_coefficient: float = 0.0
    max_grad_norm: float | None = None
    target_kl: float | None = None
    value_loss_coefficient: float = 0.0

    @classmethod
    def ppo_clip(cls) -> CombatPolicyUpdateConfig:
        return cls(
            rule=CombatPolicyUpdateRule.PPO_CLIP,
            epochs=4,
            clip_coefficient=0.2,
            entropy_coefficient=0.01,
            max_grad_norm=0.5,
            target_kl=0.02,
        )

    @classmethod
    def ppo_clip_value(cls) -> CombatPolicyUpdateConfig:
        return cls(
            rule=CombatPolicyUpdateRule.PPO_CLIP_VALUE,
            epochs=4,
            clip_coefficient=0.2,
            entropy_coefficient=0.01,
            max_grad_norm=0.5,
            target_kl=0.02,
            value_loss_coefficient=0.5,
        )

    @property
    def uses_value_baseline(self) -> bool:
        return self.rule is CombatPolicyUpdateRule.PPO_CLIP_VALUE

    def __post_init__(self) -> None:
        if not isinstance(self.rule, CombatPolicyUpdateRule):
            raise CombatObjectiveError(
                "combat policy update rule must be CombatPolicyUpdateRule"
            )
        epochs = _positive_integer(self.epochs, "policy update epochs")
        if epochs > 64:
            raise CombatObjectiveError("policy update epochs must not exceed 64")
        clip = _finite_float(self.clip_coefficient, "clip_coefficient")
        entropy = _finite_float(
            self.entropy_coefficient,
            "entropy_coefficient",
        )
        max_grad_norm = _optional_positive_float(
            self.max_grad_norm,
            "max_grad_norm",
        )
        target_kl = _optional_positive_float(self.target_kl, "target_kl")
        value_loss_coefficient = _finite_float(
            self.value_loss_coefficient,
            "value_loss_coefficient",
        )
        if clip <= 0.0 or clip >= 1.0:
            raise CombatObjectiveError("clip_coefficient must be in (0, 1)")
        if entropy < 0.0:
            raise CombatObjectiveError("entropy_coefficient must be non-negative")
        if value_loss_coefficient < 0.0:
            raise CombatObjectiveError(
                "value_loss_coefficient must be non-negative"
            )
        if self.rule is CombatPolicyUpdateRule.REINFORCE and (
            epochs != 1
            or entropy != 0.0
            or max_grad_norm is not None
            or target_kl is not None
            or value_loss_coefficient != 0.0
        ):
            raise CombatObjectiveError(
                "REINFORCE requires one epoch and no PPO optimization controls"
            )
        if self.rule is CombatPolicyUpdateRule.PPO_CLIP and (
            value_loss_coefficient != 0.0
        ):
            raise CombatObjectiveError(
                "policy-only PPO requires zero value_loss_coefficient"
            )
        if self.rule is CombatPolicyUpdateRule.PPO_CLIP_VALUE and (
            value_loss_coefficient <= 0.0
        ):
            raise CombatObjectiveError(
                "value PPO requires a positive value_loss_coefficient"
            )
        object.__setattr__(self, "epochs", epochs)
        object.__setattr__(self, "clip_coefficient", clip)
        object.__setattr__(self, "entropy_coefficient", entropy)
        object.__setattr__(self, "max_grad_norm", max_grad_norm)
        object.__setattr__(self, "target_kl", target_kl)
        object.__setattr__(
            self,
            "value_loss_coefficient",
            value_loss_coefficient,
        )


@dataclass(frozen=True)
class CombatWinObjectiveConfig:
    """Exact width and typed fallback axes of the combat objective."""

    groups_per_update: int = 1
    all_win_axis: CombatAllWinAxis = CombatAllWinAxis.TERMINAL_HP
    all_loss_axis: CombatAllLossAxis = CombatAllLossAxis.NONE
    policy_update: CombatPolicyUpdateConfig = field(
        default_factory=CombatPolicyUpdateConfig
    )

    def __post_init__(self) -> None:
        value = self.groups_per_update
        if isinstance(value, bool):
            raise CombatObjectiveError("groups_per_update must be an integer, not bool")
        try:
            normalized = operator.index(value)
        except TypeError as error:
            raise CombatObjectiveError(
                "groups_per_update must be an integer"
            ) from error
        if normalized <= 0:
            raise CombatObjectiveError("groups_per_update must be positive")
        if not isinstance(self.all_win_axis, CombatAllWinAxis):
            raise CombatObjectiveError("all_win_axis must be CombatAllWinAxis")
        if not isinstance(self.all_loss_axis, CombatAllLossAxis):
            raise CombatObjectiveError("all_loss_axis must be CombatAllLossAxis")
        if not isinstance(self.policy_update, CombatPolicyUpdateConfig):
            raise CombatObjectiveError(
                "policy_update must be CombatPolicyUpdateConfig"
            )
        object.__setattr__(self, "groups_per_update", normalized)


def _positive_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatObjectiveError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatObjectiveError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise CombatObjectiveError(f"{name} must be positive")
    return normalized


def _finite_float(value: object, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, Real):
        raise CombatObjectiveError(f"{name} must be a real number")
    normalized = float(value)
    if not math.isfinite(normalized):
        raise CombatObjectiveError(f"{name} must be finite")
    return normalized


def _optional_positive_float(value: object, name: str) -> float | None:
    if value is None:
        return None
    normalized = _finite_float(value, name)
    if normalized <= 0.0:
        raise CombatObjectiveError(f"{name} must be positive")
    return normalized
