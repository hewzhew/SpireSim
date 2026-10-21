"""Synchronous optimizer owner for same-root win-first combat learning."""

from __future__ import annotations

import math
import time
from collections.abc import Sequence
from dataclasses import dataclass
from enum import IntEnum

import torch

from .combat_experience import CompletedCombatGroupExperience
from .combat_objective import (
    CombatAllLossAxis,
    CombatAllWinAxis,
    CombatWinObjectiveConfig,
)
from .manifests import BehaviorManifestRegistry
from .policy import BehaviorManifestId
from .semantic_concat import SemanticBatchConcatLimits
from .torch_outcomes import (
    CandidatePolicyScorer,
    OnPolicyCombatWinLoss,
    _backward_on_policy_combat_win_loss,
)
from .torch_policy import RaggedCategoricalPolicyConfig


class TorchCombatTrainingError(RuntimeError):
    """A win-first combat update cannot safely commit."""


class CombatWinTrainingStatus(IntEnum):
    NO_OBJECTIVE_SIGNAL = 0
    ZERO_POLICY_GRADIENT = 1
    OPTIMIZER_STEP = 2


@dataclass(frozen=True)
class CombatWinTrainingResult:
    status: CombatWinTrainingStatus
    all_win_axis: CombatAllWinAxis
    all_loss_axis: CombatAllLossAxis
    group_count: int
    signal_group_count: int
    win_signal_group_count: int
    terminal_hp_signal_group_count: int
    enemy_hp_progress_signal_group_count: int
    replicate_count: int
    decision_count: int
    loss: float
    optimizer_steps_applied: int
    optimizer_steps_after: int
    approximate_kl: float
    clip_fraction: float
    entropy: float
    value_loss: float

    @property
    def updated(self) -> bool:
        return self.status is CombatWinTrainingStatus.OPTIMIZER_STEP


@dataclass(frozen=True)
class SynchronousCombatWinTrainerSnapshot:
    all_win_axis: CombatAllWinAxis
    all_loss_axis: CombatAllLossAxis
    deliveries: int
    optimizer_steps: int
    completed_groups: int
    signal_groups: int
    win_signal_groups: int
    terminal_hp_signal_groups: int
    enemy_hp_progress_signal_groups: int
    no_update_deliveries: int
    trained_replicates: int
    trained_decisions: int
    last_loss: float | None
    last_status: CombatWinTrainingStatus | None
    last_behavior_manifest_ids: tuple[BehaviorManifestId, ...] | None
    total_training_seconds: float
    last_training_seconds: float | None
    poisoned: bool


class SynchronousCombatWinTrainer:
    """Optimize one frozen-behavior batch and retain no combat payload."""

    def __init__(
        self,
        scorer: CandidatePolicyScorer,
        optimizer: torch.optim.Optimizer,
        registry: BehaviorManifestRegistry,
        concat_limits: SemanticBatchConcatLimits,
        policy_config: RaggedCategoricalPolicyConfig,
        objective_config: CombatWinObjectiveConfig,
    ) -> None:
        if not callable(scorer):
            raise TorchCombatTrainingError("candidate policy scorer must be callable")
        if not isinstance(optimizer, torch.optim.Optimizer):
            raise TorchCombatTrainingError("optimizer must be a torch Optimizer")
        if not isinstance(registry, BehaviorManifestRegistry):
            raise TorchCombatTrainingError(
                "combat trainer requires a behavior manifest registry"
            )
        if not isinstance(concat_limits, SemanticBatchConcatLimits):
            raise TorchCombatTrainingError(
                "combat trainer requires semantic concat limits"
            )
        if not isinstance(policy_config, RaggedCategoricalPolicyConfig):
            raise TorchCombatTrainingError(
                "combat trainer requires categorical policy config"
            )
        if not isinstance(objective_config, CombatWinObjectiveConfig):
            raise TorchCombatTrainingError(
                "combat trainer requires combat objective config"
            )
        self.scorer = scorer
        self.optimizer = optimizer
        self.registry = registry
        self.concat_limits = concat_limits
        self.policy_config = policy_config
        self.objective_config = objective_config
        self._deliveries = 0
        self._optimizer_steps = 0
        self._completed_groups = 0
        self._signal_groups = 0
        self._win_signal_groups = 0
        self._terminal_hp_signal_groups = 0
        self._enemy_hp_progress_signal_groups = 0
        self._no_update_deliveries = 0
        self._trained_replicates = 0
        self._trained_decisions = 0
        self._last_loss: float | None = None
        self._last_status: CombatWinTrainingStatus | None = None
        self._last_behavior_manifest_ids: tuple[BehaviorManifestId, ...] | None = None
        self._total_training_seconds = 0.0
        self._last_training_seconds: float | None = None
        self._poisoned = False

    @property
    def snapshot(self) -> SynchronousCombatWinTrainerSnapshot:
        return SynchronousCombatWinTrainerSnapshot(
            all_win_axis=self.objective_config.all_win_axis,
            all_loss_axis=self.objective_config.all_loss_axis,
            deliveries=self._deliveries,
            optimizer_steps=self._optimizer_steps,
            completed_groups=self._completed_groups,
            signal_groups=self._signal_groups,
            win_signal_groups=self._win_signal_groups,
            terminal_hp_signal_groups=self._terminal_hp_signal_groups,
            enemy_hp_progress_signal_groups=(
                self._enemy_hp_progress_signal_groups
            ),
            no_update_deliveries=self._no_update_deliveries,
            trained_replicates=self._trained_replicates,
            trained_decisions=self._trained_decisions,
            last_loss=self._last_loss,
            last_status=self._last_status,
            last_behavior_manifest_ids=self._last_behavior_manifest_ids,
            total_training_seconds=self._total_training_seconds,
            last_training_seconds=self._last_training_seconds,
            poisoned=self._poisoned,
        )

    def train(
        self,
        groups: Sequence[CompletedCombatGroupExperience],
    ) -> CombatWinTrainingResult:
        if self._poisoned:
            raise TorchCombatTrainingError(
                "combat trainer is poisoned after an optimizer failure"
            )
        normalized = tuple(groups)
        if len(normalized) != self.objective_config.groups_per_update:
            raise TorchCombatTrainingError(
                "combat training delivery must contain exactly groups_per_update groups"
            )

        started = time.perf_counter()
        optimizer_steps_applied = 0
        fixed_actor_advantages: tuple[float, ...] | None = None
        objective: OnPolicyCombatWinLoss | None = None
        loss = 0.0
        try:
            for epoch in range(self.objective_config.policy_update.epochs):
                self.optimizer.zero_grad(set_to_none=True)
                objective = _backward_on_policy_combat_win_loss(
                    self.scorer,
                    normalized,
                    self.registry,
                    self.concat_limits,
                    self.policy_config,
                    self.objective_config,
                    require_matching_propensities=epoch == 0,
                    fixed_actor_advantages=fixed_actor_advantages,
                )
                if objective.value.ndim != 0:
                    raise TorchCombatTrainingError(
                        "combat win-first objective must be a scalar"
                    )
                loss = float(objective.value.detach().item())
                if not math.isfinite(loss):
                    raise TorchCombatTrainingError(
                        "combat win-first loss must be finite"
                    )
                if epoch == 0:
                    if objective.signal_group_count == 0:
                        self.optimizer.zero_grad(set_to_none=True)
                        return self._finish(
                            objective,
                            loss,
                            CombatWinTrainingStatus.NO_OBJECTIVE_SIGNAL,
                            started,
                            optimizer_steps_applied=0,
                        )
                    if self.objective_config.policy_update.uses_value_baseline:
                        fixed_actor_advantages = objective.actor_advantages
                else:
                    target_kl = self.objective_config.policy_update.target_kl
                    if (
                        target_kl is not None
                        and objective.approximate_kl > target_kl
                    ):
                        self.optimizer.zero_grad(set_to_none=True)
                        break

                gradients = tuple(
                    parameter.grad
                    for group in self.optimizer.param_groups
                    for parameter in group["params"]
                    if parameter.grad is not None
                )
                if not gradients:
                    raise TorchCombatTrainingError("optimizer received no gradients")
                if not all(
                    bool(torch.all(torch.isfinite(gradient)))
                    for gradient in gradients
                ):
                    raise TorchCombatTrainingError(
                        "optimizer gradients must be finite"
                    )
                if not any(bool(torch.any(gradient != 0)) for gradient in gradients):
                    self.optimizer.zero_grad(set_to_none=True)
                    if optimizer_steps_applied == 0:
                        return self._finish(
                            objective,
                            loss,
                            CombatWinTrainingStatus.ZERO_POLICY_GRADIENT,
                            started,
                            optimizer_steps_applied=0,
                        )
                    break
                max_grad_norm = self.objective_config.policy_update.max_grad_norm
                if max_grad_norm is not None:
                    gradient_norm = torch.nn.utils.clip_grad_norm_(
                        tuple(
                            parameter
                            for group in self.optimizer.param_groups
                            for parameter in group["params"]
                            if parameter.grad is not None
                        ),
                        max_grad_norm,
                    )
                    if not bool(torch.isfinite(gradient_norm)):
                        raise TorchCombatTrainingError(
                            "optimizer gradient norm must be finite"
                        )
                self.optimizer.step()
                optimizer_steps_applied += 1
                self._optimizer_steps += 1
        except Exception:
            self.optimizer.zero_grad(set_to_none=True)
            self._poisoned = True
            raise

        if objective is None:
            raise TorchCombatTrainingError("combat policy update has no epochs")

        self._trained_replicates += (
            objective.replicate_count * optimizer_steps_applied
        )
        self._trained_decisions += objective.decision_count * optimizer_steps_applied
        return self._finish(
            objective,
            loss,
            CombatWinTrainingStatus.OPTIMIZER_STEP,
            started,
            optimizer_steps_applied=optimizer_steps_applied,
        )

    def _finish(
        self,
        objective: OnPolicyCombatWinLoss,
        loss: float,
        status: CombatWinTrainingStatus,
        started: float,
        *,
        optimizer_steps_applied: int,
    ) -> CombatWinTrainingResult:
        elapsed = time.perf_counter() - started
        self._deliveries += 1
        self._completed_groups += objective.group_count
        self._signal_groups += objective.signal_group_count
        self._win_signal_groups += objective.win_signal_group_count
        self._terminal_hp_signal_groups += objective.terminal_hp_signal_group_count
        self._enemy_hp_progress_signal_groups += (
            objective.enemy_hp_progress_signal_group_count
        )
        self._no_update_deliveries += int(
            status is not CombatWinTrainingStatus.OPTIMIZER_STEP
        )
        self._last_loss = loss
        self._last_status = status
        self._last_behavior_manifest_ids = objective.behavior_manifest_ids
        self._total_training_seconds += elapsed
        self._last_training_seconds = elapsed
        return CombatWinTrainingResult(
            status=status,
            all_win_axis=self.objective_config.all_win_axis,
            all_loss_axis=self.objective_config.all_loss_axis,
            group_count=objective.group_count,
            signal_group_count=objective.signal_group_count,
            win_signal_group_count=objective.win_signal_group_count,
            terminal_hp_signal_group_count=objective.terminal_hp_signal_group_count,
            enemy_hp_progress_signal_group_count=(
                objective.enemy_hp_progress_signal_group_count
            ),
            replicate_count=objective.replicate_count,
            decision_count=objective.decision_count,
            loss=loss,
            optimizer_steps_applied=optimizer_steps_applied,
            optimizer_steps_after=self._optimizer_steps,
            approximate_kl=objective.approximate_kl,
            clip_fraction=objective.clip_fraction,
            entropy=objective.entropy,
            value_loss=objective.value_loss,
        )
