"""Synchronous on-policy optimizer sink for complete-attempt delivery."""

from __future__ import annotations

import math
import time
from dataclasses import dataclass

import torch

from .attempts import AttemptAssemblyDelivery, DroppedAttemptExperience
from .credit_assignment import (
    CreditAssignmentComparison,
    CreditAssignmentError,
    compare_credit_assignment,
)
from .manifests import BehaviorManifestRegistry, BehaviorRuleBinding
from .policy import BehaviorManifestId, SelectionProbability
from .public_trajectory import (
    PublicTrajectoryError,
    build_public_attempt_trajectory,
)
from .semantic_concat import SemanticBatchConcatLimits
from .terminal_returns import OnPolicyObjectiveConfig
from .torch_outcomes import (
    CandidatePolicyScorer,
    RunValueDiagnostics,
    on_policy_terminal_loss,
)
from .torch_policy import (
    RaggedCandidateScorer,
    RaggedCategoricalPolicyConfig,
    TorchPolicyError,
    configure_critic_only_training,
)


class TorchTrainingError(RuntimeError):
    """A synchronous training delivery cannot safely commit."""


@dataclass(frozen=True)
class SynchronousPolicyTrainerSnapshot:
    deliveries: int
    optimizer_steps: int
    completed_attempts: int
    dropped_attempts: int
    trained_decisions: int
    last_loss: float | None
    last_behavior_manifest_ids: tuple[tuple[BehaviorManifestId, ...], ...] | None
    last_selection_probabilities: (
        tuple[tuple[SelectionProbability, ...], ...] | None
    )
    total_training_seconds: float
    last_training_seconds: float | None
    poisoned: bool


@dataclass(frozen=True)
class RunPolicyTrainingResult:
    """Diagnostics for one committed complete-attempt policy update."""

    loss: float
    optimizer_steps_applied: int
    optimizer_steps_after: int
    decision_count: int
    actor_decision_count: int
    actor_update_applied: bool
    approximate_kl: float
    clip_fraction: float
    entropy: float
    value_loss: float
    value_clip_fraction: float
    explained_variance: float | None
    gradient_norm: float
    rollout_value_diagnostics: RunValueDiagnostics | None


class SynchronousPolicyTrainer:
    """Train once per on-policy delivery and retain no experience payload."""

    def __init__(
        self,
        scorer: CandidatePolicyScorer,
        optimizer: torch.optim.Optimizer,
        registry: BehaviorManifestRegistry,
        concat_limits: SemanticBatchConcatLimits,
        policy_config: RaggedCategoricalPolicyConfig,
        objective_config: OnPolicyObjectiveConfig,
        *,
        behavior_rule: BehaviorRuleBinding | None = None,
        resume_snapshot: SynchronousPolicyTrainerSnapshot | None = None,
    ) -> None:
        if not callable(scorer):
            raise TorchTrainingError("candidate policy scorer must be callable")
        if not isinstance(optimizer, torch.optim.Optimizer):
            raise TorchTrainingError("optimizer must be a torch Optimizer")
        if not isinstance(registry, BehaviorManifestRegistry):
            raise TorchTrainingError("trainer requires a behavior manifest registry")
        if not isinstance(concat_limits, SemanticBatchConcatLimits):
            raise TorchTrainingError("trainer requires semantic concat limits")
        if not isinstance(policy_config, RaggedCategoricalPolicyConfig):
            raise TorchTrainingError("trainer requires categorical policy config")
        if not isinstance(objective_config, OnPolicyObjectiveConfig):
            raise TorchTrainingError("trainer requires on-policy objective config")
        if behavior_rule is None:
            behavior_rule = policy_config.behavior_rule
        if not isinstance(behavior_rule, BehaviorRuleBinding):
            raise TorchTrainingError("trainer behavior rule must be typed")
        self.scorer = scorer
        self.optimizer = optimizer
        self.registry = registry
        self.concat_limits = concat_limits
        self.policy_config = policy_config
        self.behavior_rule = behavior_rule
        self.objective_config = objective_config
        restored = _validated_resume_snapshot(resume_snapshot)
        self._deliveries = restored.deliveries
        self._optimizer_steps = restored.optimizer_steps
        self._completed_attempts = restored.completed_attempts
        self._dropped_attempts = restored.dropped_attempts
        self._trained_decisions = restored.trained_decisions
        self._last_loss = restored.last_loss
        self._last_behavior_manifest_ids = restored.last_behavior_manifest_ids
        self._last_selection_probabilities = restored.last_selection_probabilities
        self._total_training_seconds = restored.total_training_seconds
        self._last_training_seconds = restored.last_training_seconds
        self._last_credit_assignment: CreditAssignmentComparison | None = None
        self._last_result: RunPolicyTrainingResult | None = None
        self._poisoned = False
        if not objective_config.policy_update.updates_actor:
            if not isinstance(scorer, RaggedCandidateScorer):
                raise TorchTrainingError(
                    "critic calibration requires the maintained ragged scorer"
                )
            try:
                configure_critic_only_training(scorer)
            except TorchPolicyError as error:
                raise TorchTrainingError(str(error)) from error

    @property
    def snapshot(self) -> SynchronousPolicyTrainerSnapshot:
        return SynchronousPolicyTrainerSnapshot(
            deliveries=self._deliveries,
            optimizer_steps=self._optimizer_steps,
            completed_attempts=self._completed_attempts,
            dropped_attempts=self._dropped_attempts,
            trained_decisions=self._trained_decisions,
            last_loss=self._last_loss,
            last_behavior_manifest_ids=self._last_behavior_manifest_ids,
            last_selection_probabilities=self._last_selection_probabilities,
            total_training_seconds=self._total_training_seconds,
            last_training_seconds=self._last_training_seconds,
            poisoned=self._poisoned,
        )

    @property
    def last_credit_assignment(self) -> CreditAssignmentComparison | None:
        """Return the latest non-authoritative target comparison, if available."""

        return self._last_credit_assignment

    @property
    def last_result(self) -> RunPolicyTrainingResult | None:
        """Return diagnostics for the latest live update, if one occurred."""

        return self._last_result

    def __call__(self, delivery: AttemptAssemblyDelivery) -> None:
        if self._poisoned:
            raise TorchTrainingError("trainer is poisoned after an optimizer failure")
        if not isinstance(delivery, AttemptAssemblyDelivery):
            raise TorchTrainingError("trainer requires AttemptAssemblyDelivery input")
        if not all(
            isinstance(attempt, DroppedAttemptExperience)
            for attempt in delivery.dropped
        ):
            raise TorchTrainingError("dropped delivery rows are malformed")

        completed_count = len(delivery.completed)
        dropped_count = len(delivery.dropped)
        if completed_count == 0:
            self._deliveries += 1
            self._dropped_attempts += dropped_count
            self._last_result = None
            return
        if completed_count != self.objective_config.attempts_per_update:
            raise TorchTrainingError(
                "training delivery must contain exactly attempts_per_update "
                "completed attempts"
            )

        try:
            trajectories = tuple(
                build_public_attempt_trajectory(attempt)
                for attempt in delivery.completed
            )
        except PublicTrajectoryError as error:
            raise TorchTrainingError(str(error)) from error
        try:
            credit_assignment = compare_credit_assignment(
                trajectories,
                self.objective_config.terminal_return,
            )
        except CreditAssignmentError as error:
            raise TorchTrainingError(str(error)) from error

        training_started = time.perf_counter()
        objective = on_policy_terminal_loss(
            self.scorer,
            trajectories,
            self.registry,
            self.concat_limits,
            self.policy_config,
            self.objective_config.terminal_return,
            self.objective_config.advantage_mode,
            self.objective_config.decision_scope,
            expected_behavior_rule=self.behavior_rule,
            update_config=self.objective_config.policy_update,
        )
        if objective.value.ndim != 0 or not objective.value.requires_grad:
            raise TorchTrainingError(
                "realized outcome objective must be a differentiable scalar"
            )

        optimizer_steps_applied = 0
        gradient_norm = 0.0
        fixed_actor_advantages = (
            objective.actor_advantages
            if self.objective_config.policy_update.uses_value_baseline
            else None
        )
        fixed_value_predictions = (
            objective.critic_predictions
            if self.objective_config.policy_update.uses_value_baseline
            else None
        )
        rollout_value_diagnostics = objective.value_diagnostics
        try:
            for epoch in range(self.objective_config.policy_update.epochs):
                if epoch > 0:
                    objective = on_policy_terminal_loss(
                        self.scorer,
                        trajectories,
                        self.registry,
                        self.concat_limits,
                        self.policy_config,
                        self.objective_config.terminal_return,
                        self.objective_config.advantage_mode,
                        self.objective_config.decision_scope,
                        expected_behavior_rule=self.behavior_rule,
                        update_config=self.objective_config.policy_update,
                        require_matching_propensities=False,
                        fixed_actor_advantages=fixed_actor_advantages,
                        fixed_value_predictions=fixed_value_predictions,
                    )
                    target_kl = self.objective_config.policy_update.target_kl
                    if target_kl is not None and objective.approximate_kl > target_kl:
                        break

                self.optimizer.zero_grad(set_to_none=True)
                objective.value.backward()
                parameters = tuple(
                    parameter
                    for group in self.optimizer.param_groups
                    for parameter in group["params"]
                    if parameter.grad is not None
                )
                if not parameters:
                    raise TorchTrainingError("optimizer received no gradients")
                gradients = tuple(parameter.grad for parameter in parameters)
                if not all(
                    gradient is not None
                    and bool(torch.all(torch.isfinite(gradient)))
                    for gradient in gradients
                ):
                    raise TorchTrainingError("optimizer gradients must be finite")
                max_grad_norm = self.objective_config.policy_update.max_grad_norm
                measured_norm = torch.nn.utils.clip_grad_norm_(
                    parameters,
                    float("inf") if max_grad_norm is None else max_grad_norm,
                )
                if not bool(torch.isfinite(measured_norm)):
                    raise TorchTrainingError("optimizer gradient norm must be finite")
                gradient_norm = float(measured_norm.detach().item())
                self.optimizer.step()
                optimizer_steps_applied += 1
                self._optimizer_steps += 1
        except Exception:
            self._poisoned = True
            raise

        if optimizer_steps_applied <= 0:
            self._poisoned = True
            raise TorchTrainingError("policy update committed no optimizer step")

        loss = float(objective.value.detach().item())
        if not math.isfinite(loss):
            self._poisoned = True
            raise TorchTrainingError("committed optimizer loss must be finite")
        self._deliveries += 1
        self._completed_attempts += completed_count
        self._dropped_attempts += dropped_count
        self._trained_decisions += (
            objective.actor_decision_count * optimizer_steps_applied
        )
        self._last_loss = loss
        self._last_behavior_manifest_ids = objective.behavior_manifest_ids
        self._last_selection_probabilities = objective.selection_probabilities
        self._last_credit_assignment = credit_assignment
        self._last_result = RunPolicyTrainingResult(
            loss=loss,
            optimizer_steps_applied=optimizer_steps_applied,
            optimizer_steps_after=self._optimizer_steps,
            decision_count=objective.decision_count,
            actor_decision_count=objective.actor_decision_count,
            actor_update_applied=(
                self.objective_config.policy_update.updates_actor
            ),
            approximate_kl=objective.approximate_kl,
            clip_fraction=objective.clip_fraction,
            entropy=objective.entropy,
            value_loss=objective.value_loss,
            value_clip_fraction=objective.value_clip_fraction,
            explained_variance=objective.explained_variance,
            gradient_norm=gradient_norm,
            rollout_value_diagnostics=rollout_value_diagnostics,
        )
        elapsed = time.perf_counter() - training_started
        self._total_training_seconds += elapsed
        self._last_training_seconds = elapsed


def _validated_resume_snapshot(
    snapshot: SynchronousPolicyTrainerSnapshot | None,
) -> SynchronousPolicyTrainerSnapshot:
    if snapshot is None:
        return SynchronousPolicyTrainerSnapshot(
            deliveries=0,
            optimizer_steps=0,
            completed_attempts=0,
            dropped_attempts=0,
            trained_decisions=0,
            last_loss=None,
            last_behavior_manifest_ids=None,
            last_selection_probabilities=None,
            total_training_seconds=0.0,
            last_training_seconds=None,
            poisoned=False,
        )
    if not isinstance(snapshot, SynchronousPolicyTrainerSnapshot):
        raise TorchTrainingError("trainer resume snapshot must be typed")
    if snapshot.poisoned:
        raise TorchTrainingError("cannot resume a poisoned trainer")
    for name in (
        "deliveries",
        "optimizer_steps",
        "completed_attempts",
        "dropped_attempts",
        "trained_decisions",
    ):
        value = getattr(snapshot, name)
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise TorchTrainingError(f"trainer resume {name} must be non-negative")
    if snapshot.last_loss is not None and not math.isfinite(snapshot.last_loss):
        raise TorchTrainingError("trainer resume loss must be finite")
    if (
        not math.isfinite(snapshot.total_training_seconds)
        or snapshot.total_training_seconds < 0.0
    ):
        raise TorchTrainingError("trainer resume total time must be finite")
    if snapshot.last_training_seconds is not None and (
        not math.isfinite(snapshot.last_training_seconds)
        or snapshot.last_training_seconds < 0.0
    ):
        raise TorchTrainingError("trainer resume last time must be finite")
    if snapshot.optimizer_steps == 0:
        if (
            snapshot.last_loss is not None
            or snapshot.last_behavior_manifest_ids is not None
            or snapshot.last_selection_probabilities is not None
            or snapshot.last_training_seconds is not None
        ):
            raise TorchTrainingError(
                "trainer without optimizer steps cannot have last-training evidence"
            )
    else:
        if (
            snapshot.last_loss is None
            or snapshot.last_behavior_manifest_ids is None
            or snapshot.last_selection_probabilities is None
            or snapshot.last_training_seconds is None
        ):
            raise TorchTrainingError(
                "trainer optimizer steps require last-training evidence"
            )
        if len(snapshot.last_behavior_manifest_ids) != len(
            snapshot.last_selection_probabilities
        ):
            raise TorchTrainingError("trainer resume evidence attempts are misaligned")
        for manifest_ids, probabilities in zip(
            snapshot.last_behavior_manifest_ids,
            snapshot.last_selection_probabilities,
            strict=True,
        ):
            if len(manifest_ids) != len(probabilities):
                raise TorchTrainingError(
                    "trainer resume evidence decisions are misaligned"
                )
            if not all(
                isinstance(manifest_id, BehaviorManifestId)
                for manifest_id in manifest_ids
            ):
                raise TorchTrainingError("trainer resume manifest ids are malformed")
            if not all(
                isinstance(probability, SelectionProbability)
                for probability in probabilities
            ):
                raise TorchTrainingError(
                    "trainer resume selection probabilities are malformed"
                )
    return snapshot
