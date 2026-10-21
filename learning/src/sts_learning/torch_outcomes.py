"""On-policy policy objectives over exact terminal experience."""

from __future__ import annotations

import math
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Protocol

import torch
from torch import Tensor

from .credit_assignment import (
    CreditAssignmentError,
    matched_episode_floor_context_leave_one_out_advantages,
    matched_floor_context_leave_one_out_advantages,
    matched_floor_leave_one_out_advantages,
)
from .combat_experience import (
    CombatDecisionExperienceBatch,
    CompletedCombatGroupExperience,
)
from .combat_objective import (
    CombatAllLossAxis,
    CombatAllWinAxis,
    CombatPolicyUpdateConfig,
    CombatPolicyUpdateRule,
    CombatWinObjectiveConfig,
)
from .combat_outcomes import CombatTerminalKind
from .combat_rollout import (
    COMBAT_ROLLOUT_VALUE_HEAD_WIDTH,
    CombatRolloutAxis,
    CombatRolloutError,
    build_complete_combat_rollout,
)
from .manifests import BehaviorManifestRegistry, BehaviorRuleBinding
from .policy import BehaviorManifestId, SelectionProbability
from .public_trajectory import (
    PublicAttemptTrajectoryV1,
    PublicTrajectoryDecisionV1,
)
from .run_rollout import (
    RunRolloutError,
    build_complete_run_rollout,
    decision_progress_potential,
)
from .semantic_concat import (
    SemanticBatchConcatLimits,
    concatenate_semantic_decision_batches,
    plan_semantic_decision_batch_chunks,
)
from .semantic_batch import select_semantic_decision_rows
from .terminal_returns import (
    FloorProgressReturnConfig,
    RunDecisionScope,
    RunPolicyUpdateConfig,
    RunPolicyUpdateRule,
    TerminalAdvantageMode,
    floor_progress_terminal_return,
    terminal_return_advantages,
)
from .torch_policy import (
    RaggedActorCriticOutput,
    RaggedCandidateLogits,
    RaggedCandidateScorer,
    RaggedMultiActorCriticOutput,
    RaggedCategoricalPolicyConfig,
)


class TorchOutcomeError(ValueError):
    """Complete outcome experience does not satisfy the policy objective."""


class CandidatePolicyScorer(Protocol):
    def __call__(self, decision_batch: Mapping[str, object]) -> RaggedCandidateLogits: ...


@dataclass(frozen=True)
class OnPolicyTerminalLoss:
    """One differentiable policy loss plus exact behavior provenance."""

    value: Tensor
    attempt_count: int
    decision_count: int
    actor_decision_count: int
    behavior_manifest_ids: tuple[tuple[BehaviorManifestId, ...], ...]
    selection_probabilities: tuple[tuple[SelectionProbability, ...], ...]
    approximate_kl: float
    clip_fraction: float
    entropy: float
    value_loss: float
    value_clip_fraction: float
    explained_variance: float | None
    actor_advantages: tuple[float, ...]
    critic_predictions: tuple[float, ...] | None
    value_diagnostics: RunValueDiagnostics | None


@dataclass(frozen=True)
class RunSignalSummary:
    """Compact decision signal statistics under the declared actor/value weights."""

    decision_count: int
    negative_decisions: int
    zero_decisions: int
    positive_decisions: int
    negative_weight: float
    zero_weight: float
    positive_weight: float
    weighted_mean: float
    weighted_standard_deviation: float
    minimum: float
    maximum: float


@dataclass(frozen=True)
class RunValueDiagnostics:
    """Frozen rollout diagnostics for one whole-run actor-critic objective."""

    pre_normalization_actor_advantage: RunSignalSummary | None
    actor_advantage: RunSignalSummary | None
    advantage_normalization_applied: bool
    advantage_normalization_sign_changes: int
    advantage_normalization_positive_from_nonpositive: int
    advantage_normalization_negative_from_nonnegative: int
    critic_prediction: RunSignalSummary
    return_to_go_target: RunSignalSummary
    critic_residual: RunSignalSummary
    actor_decision_count: int
    forced_decision_count: int
    explained_variance: float | None


@dataclass(frozen=True)
class OnPolicyCombatWinLoss:
    """One win-first combat loss without an HP/potion exchange rate.

    A root with mixed wins and losses uses only win advantage.  An all-win
    root may use the configured all-win terminal-HP advantage, so solved early
    combats can keep learning resource preservation. An exact all-loss root may
    use enemy-HP progress only when explicitly configured. Potion retention
    remains evidence only.
    """

    value: Tensor
    group_count: int
    signal_group_count: int
    win_signal_group_count: int
    terminal_hp_signal_group_count: int
    enemy_hp_progress_signal_group_count: int
    replicate_count: int
    decision_count: int
    behavior_manifest_ids: tuple[BehaviorManifestId, ...]
    selection_probabilities: tuple[tuple[SelectionProbability, ...], ...]
    approximate_kl: float
    clip_fraction: float
    entropy: float
    value_loss: float
    actor_advantages: tuple[float, ...]


@dataclass(frozen=True)
class _CombatPolicyLoss:
    value: Tensor
    diagnostic_weight: float
    approximate_kl_weighted_sum: float
    clipped_weight: float
    entropy_weighted_sum: float
    value_loss: float
    actor_advantages: tuple[float, ...]


@dataclass(frozen=True)
class _RunPolicyLoss:
    value: Tensor
    approximate_kl: float
    clip_fraction: float
    entropy: float
    value_loss: float
    value_clip_fraction: float
    explained_variance: float | None
    actor_decision_count: int
    actor_advantages: tuple[float, ...]
    critic_predictions: tuple[float, ...] | None
    value_diagnostics: RunValueDiagnostics | None


def on_policy_terminal_loss(
    scorer: CandidatePolicyScorer,
    attempts: Sequence[PublicAttemptTrajectoryV1],
    registry: BehaviorManifestRegistry,
    concat_limits: SemanticBatchConcatLimits,
    policy_config: RaggedCategoricalPolicyConfig,
    return_config: FloorProgressReturnConfig,
    advantage_mode: TerminalAdvantageMode,
    decision_scope: RunDecisionScope = RunDecisionScope.ALL,
    *,
    expected_behavior_rule: BehaviorRuleBinding | None = None,
    update_config: RunPolicyUpdateConfig = RunPolicyUpdateConfig(),
    require_matching_propensities: bool = True,
    fixed_actor_advantages: Sequence[float] | None = None,
    fixed_value_predictions: Sequence[float] | None = None,
) -> OnPolicyTerminalLoss:
    """Apply one exact REINFORCE or decision-local GAE-PPO objective.

    Policy terms sum within an attempt and average across attempts, as in
    the likelihood-ratio gradient of expected terminal return. Auxiliary
    value errors average within and across attempts. A single-candidate
    decision has no actor gradient; value PPO still trains its critic row.
    """

    if not callable(scorer):
        raise TorchOutcomeError("candidate policy scorer must be callable")
    if not isinstance(registry, BehaviorManifestRegistry):
        raise TorchOutcomeError("policy objective requires a behavior manifest registry")
    if not isinstance(concat_limits, SemanticBatchConcatLimits):
        raise TorchOutcomeError("policy objective requires semantic concat limits")
    if not isinstance(policy_config, RaggedCategoricalPolicyConfig):
        raise TorchOutcomeError("policy objective requires categorical policy config")
    if not isinstance(return_config, FloorProgressReturnConfig):
        raise TorchOutcomeError("policy objective requires terminal return config")
    if not isinstance(advantage_mode, TerminalAdvantageMode):
        raise TorchOutcomeError("policy objective requires typed advantage mode")
    if not isinstance(decision_scope, RunDecisionScope):
        raise TorchOutcomeError("policy objective requires typed decision scope")
    if expected_behavior_rule is None:
        expected_behavior_rule = policy_config.behavior_rule
    if not isinstance(expected_behavior_rule, BehaviorRuleBinding):
        raise TorchOutcomeError("policy objective behavior rule must be typed")
    if not isinstance(update_config, RunPolicyUpdateConfig):
        raise TorchOutcomeError("policy objective requires typed run update config")
    if type(require_matching_propensities) is not bool:
        raise TorchOutcomeError("run policy propensity check must be bool")
    if (
        update_config.uses_value_baseline
        and advantage_mode is not TerminalAdvantageMode.DECISION_LOCAL_GAE
    ):
        raise TorchOutcomeError("run value PPO requires decision-local GAE advantage")
    if fixed_actor_advantages is not None and not update_config.uses_value_baseline:
        raise TorchOutcomeError("fixed run advantages require a value baseline")
    if fixed_value_predictions is not None and not update_config.uses_value_baseline:
        raise TorchOutcomeError("fixed run values require a value baseline")
    normalized = tuple(attempts)
    if not normalized:
        raise TorchOutcomeError("policy objective requires at least one complete attempt")
    if not all(
        isinstance(attempt, PublicAttemptTrajectoryV1) for attempt in normalized
    ):
        raise TorchOutcomeError(
            "policy objective accepts only public attempt trajectories"
        )
    rollout = None
    if update_config.uses_value_baseline:
        try:
            rollout = build_complete_run_rollout(normalized, return_config)
        except RunRolloutError as error:
            raise TorchOutcomeError(str(error)) from error
    matched_advantages = None
    advantages = None
    if update_config.uses_value_baseline:
        pass
    elif advantage_mode in (
        TerminalAdvantageMode.MATCHED_FLOOR_LEAVE_ONE_OUT,
        TerminalAdvantageMode.MATCHED_FLOOR_CONTEXT_LEAVE_ONE_OUT,
        TerminalAdvantageMode.MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT,
    ):
        try:
            if advantage_mode is TerminalAdvantageMode.MATCHED_FLOOR_LEAVE_ONE_OUT:
                matched_advantages = matched_floor_leave_one_out_advantages(
                    normalized,
                    return_config,
                )
            elif (
                advantage_mode
                is TerminalAdvantageMode.MATCHED_FLOOR_CONTEXT_LEAVE_ONE_OUT
            ):
                matched_advantages = matched_floor_context_leave_one_out_advantages(
                    normalized,
                    return_config,
                )
            else:
                matched_advantages = (
                    matched_episode_floor_context_leave_one_out_advantages(
                        normalized,
                        return_config,
                    )
                )
        except CreditAssignmentError as error:
            raise TorchOutcomeError(str(error)) from error
    else:
        advantages = terminal_return_advantages(
            tuple(
                floor_progress_terminal_return(attempt.terminal, return_config)
                for attempt in normalized
            ),
            advantage_mode,
        )

    behavior_ids: list[tuple[BehaviorManifestId, ...]] = []
    probability_evidence: list[tuple[SelectionProbability, ...]] = []
    payloads: list[Mapping[str, object]] = []
    selected_ordinals: list[int] = []
    targets: list[float] = []
    value_weights: list[float] = []
    actor_weights: list[float] = []
    value_prediction_offsets: list[float] = []
    terminal_score_targets: list[float] = []
    total_decisions = 0
    for attempt_index, attempt in enumerate(normalized):
        retained_decisions = len(attempt.decisions)
        if retained_decisions <= 0:
            raise TorchOutcomeError("public attempt has no retained decisions")

        scoped_decisions_with_indices = tuple(
            (decision_index, decision)
            for decision_index, decision in enumerate(attempt.decisions)
            if _decision_in_scope(decision, decision_scope)
        )
        scoped_decisions = len(scoped_decisions_with_indices)
        if scoped_decisions == 0:
            raise TorchOutcomeError(
                "public attempt has no decisions in the configured scope"
            )
        attempt_behavior_ids: list[BehaviorManifestId] = []
        attempt_probabilities: list[SelectionProbability] = []
        for decision_index, decision in scoped_decisions_with_indices:
            try:
                manifest = registry.resolve(decision.behavior_manifest_id)
            except ValueError as error:
                raise TorchOutcomeError(
                    "public attempt references an unknown behavior manifest"
                ) from error
            if manifest.behavior_rule != expected_behavior_rule:
                raise TorchOutcomeError(
                    "public attempt behavior rule conflicts with policy config"
                )
            attempt_behavior_ids.append(decision.behavior_manifest_id)
            payloads.append(decision.semantic_payload)
            selected_ordinals.append(decision.selected_ordinal)
            attempt_probabilities.append(decision.selection_probability)
            if rollout is not None:
                rollout_row = rollout.attempts[attempt_index].rows[decision_index]
                batch_targets = (rollout_row.return_to_go,)
            elif matched_advantages is None:
                assert advantages is not None
                batch_targets = (advantages[attempt_index],)
            else:
                original_targets = matched_advantages[attempt_index][decision_index]
                if len(original_targets) != 1:
                    raise TorchOutcomeError(
                        "matched target is not public-decision aligned"
                    )
                batch_targets = original_targets
            targets.extend(batch_targets)
            if update_config.value_prediction_target == "terminal_score":
                assert rollout is not None
                value_prediction_offsets.append(
                    decision_progress_potential(decision.run_progress, return_config)
                )
                terminal_score_targets.append(rollout.attempts[attempt_index].terminal_return)
            value_weights.append(1.0 / (len(normalized) * scoped_decisions))
            if rollout is None:
                actor_weights.append(1.0 / len(normalized))
            else:
                actor_weights.append(rollout_row.actor_weight)
        behavior_ids.append(tuple(attempt_behavior_ids))
        probability_evidence.append(tuple(attempt_probabilities))
        total_decisions += scoped_decisions

    policy_loss = _run_policy_weighted_loss(
        scorer=scorer,
        payloads=payloads,
        selected_ordinals=selected_ordinals,
        selection_probabilities=tuple(
            probability
            for attempt_probabilities in probability_evidence
            for probability in attempt_probabilities
        ),
        targets=targets,
        value_weights=value_weights,
        actor_weights=actor_weights,
        concat_limits=concat_limits,
        policy_config=policy_config,
        update_config=update_config,
        require_matching_propensities=require_matching_propensities,
        fixed_actor_advantages=fixed_actor_advantages,
        fixed_value_predictions=fixed_value_predictions,
        value_prediction_offsets=(value_prediction_offsets or None),
        terminal_score_targets=(terminal_score_targets or None),
    )
    return OnPolicyTerminalLoss(
        value=policy_loss.value,
        attempt_count=len(normalized),
        decision_count=total_decisions,
        actor_decision_count=policy_loss.actor_decision_count,
        behavior_manifest_ids=tuple(behavior_ids),
        selection_probabilities=tuple(probability_evidence),
        approximate_kl=policy_loss.approximate_kl,
        clip_fraction=policy_loss.clip_fraction,
        entropy=policy_loss.entropy,
        value_loss=policy_loss.value_loss,
        value_clip_fraction=policy_loss.value_clip_fraction,
        explained_variance=policy_loss.explained_variance,
        actor_advantages=policy_loss.actor_advantages,
        critic_predictions=policy_loss.critic_predictions,
        value_diagnostics=policy_loss.value_diagnostics,
    )


def on_policy_combat_win_loss(
    scorer: CandidatePolicyScorer,
    groups: Sequence[CompletedCombatGroupExperience],
    registry: BehaviorManifestRegistry,
    concat_limits: SemanticBatchConcatLimits,
    policy_config: RaggedCategoricalPolicyConfig,
    objective_config: CombatWinObjectiveConfig,
    *,
    require_matching_propensities: bool = True,
    fixed_actor_advantages: Sequence[float] | None = None,
) -> OnPolicyCombatWinLoss:
    """Construct one differentiable combat objective through bounded batches."""

    return _on_policy_combat_win_loss(
        scorer,
        groups,
        registry,
        concat_limits,
        policy_config,
        objective_config,
        require_matching_propensities=require_matching_propensities,
        fixed_actor_advantages=fixed_actor_advantages,
        backward_microbatches=False,
    )


def _backward_on_policy_combat_win_loss(
    scorer: CandidatePolicyScorer,
    groups: Sequence[CompletedCombatGroupExperience],
    registry: BehaviorManifestRegistry,
    concat_limits: SemanticBatchConcatLimits,
    policy_config: RaggedCategoricalPolicyConfig,
    objective_config: CombatWinObjectiveConfig,
    *,
    require_matching_propensities: bool = True,
    fixed_actor_advantages: Sequence[float] | None = None,
) -> OnPolicyCombatWinLoss:
    """Backpropagate bounded microbatches without retaining all forward graphs."""

    return _on_policy_combat_win_loss(
        scorer,
        groups,
        registry,
        concat_limits,
        policy_config,
        objective_config,
        require_matching_propensities=require_matching_propensities,
        fixed_actor_advantages=fixed_actor_advantages,
        backward_microbatches=True,
    )


def _on_policy_combat_win_loss(
    scorer: CandidatePolicyScorer,
    groups: Sequence[CompletedCombatGroupExperience],
    registry: BehaviorManifestRegistry,
    concat_limits: SemanticBatchConcatLimits,
    policy_config: RaggedCategoricalPolicyConfig,
    objective_config: CombatWinObjectiveConfig,
    *,
    require_matching_propensities: bool,
    fixed_actor_advantages: Sequence[float] | None,
    backward_microbatches: bool,
) -> OnPolicyCombatWinLoss:
    """Apply same-root win-first advantages with a typed all-win fallback.

    Every group has equal total weight. Inside a group, every replicate has
    equal total weight regardless of combat length, and its weight is split
    equally across only that replicate's retained decisions. A group with any
    win variation uses only win advantage. An all-win group uses terminal HP
    only when configured and that axis varies. An all-loss group uses enemy-HP
    progress only when explicitly configured, every terminal is an exact loss,
    and that axis varies. Potion retention is deliberately absent, so there is
    no exchange rate among the selected axes and potions.
    """

    if not callable(scorer):
        raise TorchOutcomeError("candidate policy scorer must be callable")
    if not isinstance(registry, BehaviorManifestRegistry):
        raise TorchOutcomeError(
            "combat win objective requires a behavior manifest registry"
        )
    if not isinstance(concat_limits, SemanticBatchConcatLimits):
        raise TorchOutcomeError(
            "combat win objective requires semantic concat limits"
        )
    if not isinstance(policy_config, RaggedCategoricalPolicyConfig):
        raise TorchOutcomeError(
            "combat win objective requires categorical policy config"
        )
    if not isinstance(objective_config, CombatWinObjectiveConfig):
        raise TorchOutcomeError("combat win objective requires typed objective config")
    if type(require_matching_propensities) is not bool:
        raise TorchOutcomeError(
            "combat win objective propensity check must be bool"
        )
    if type(backward_microbatches) is not bool:
        raise TorchOutcomeError(
            "combat win objective backward_microbatches must be bool"
        )
    if (
        fixed_actor_advantages is not None
        and not objective_config.policy_update.uses_value_baseline
    ):
        raise TorchOutcomeError(
            "fixed combat advantages require a value baseline"
        )
    normalized = tuple(groups)
    if not normalized:
        raise TorchOutcomeError(
            "combat win objective requires at least one complete group"
        )

    seen_roots: set[tuple[str, str]] = set()
    behavior_ids: list[BehaviorManifestId] = []
    probability_evidence: list[tuple[SelectionProbability, ...]] = []
    payloads: list[Mapping[str, object]] = []
    selected_ordinals: list[int] = []
    targets: list[float] = []
    weights: list[float] = []
    value_head_indices: list[int] = []
    total_replicates = 0
    total_decisions = 0
    win_signal_groups = 0
    terminal_hp_signal_groups = 0
    enemy_hp_progress_signal_groups = 0

    for group in normalized:
        if not isinstance(group, CompletedCombatGroupExperience):
            raise TorchOutcomeError("combat win objective accepts only complete groups")
        root = (group.root_id, group.exact_combat_state_hash)
        if root in seen_roots:
            raise TorchOutcomeError("combat win objective repeats an exact root")
        seen_roots.add(root)
        try:
            manifest = registry.resolve(group.behavior_manifest_id)
        except ValueError as error:
            raise TorchOutcomeError(
                "combat group references an unknown behavior manifest"
            ) from error
        if manifest.behavior_rule != policy_config.behavior_rule:
            raise TorchOutcomeError(
                "combat group behavior rule conflicts with policy config"
            )

        advantages = group.grouped_advantages()
        if advantages.win_has_signal:
            selected_advantages = advantages.win
            selected_rollout_axis = CombatRolloutAxis.WIN
            selected_returns = tuple(
                1.0 if outcome.won else 0.0
                for outcome in group.outcomes.outcomes
            )
            win_signal_groups += 1
        elif (
            objective_config.all_win_axis is CombatAllWinAxis.TERMINAL_HP
            and all(outcome.won for outcome in group.outcomes.outcomes)
            and advantages.terminal_hp_has_signal
        ):
            selected_advantages = advantages.terminal_hp
            selected_rollout_axis = CombatRolloutAxis.PLAYER_HP_CHANGE
            start_hp = group.outcomes.outcomes[0].start_hp
            selected_returns = tuple(
                outcome.final_hp / start_hp
                for outcome in group.outcomes.outcomes
            )
            terminal_hp_signal_groups += 1
        elif (
            objective_config.all_loss_axis
            is CombatAllLossAxis.ENEMY_HP_PROGRESS
            and all(
                outcome.terminal_kind is CombatTerminalKind.LOSS
                for outcome in group.outcomes.outcomes
            )
            and advantages.enemy_hp_progress_has_signal
        ):
            selected_advantages = advantages.enemy_hp_progress
            selected_rollout_axis = CombatRolloutAxis.ENEMY_HP_CHANGE
            selected_returns = tuple(
                1.0 - outcome.enemy_final_hp / outcome.enemy_start_hp
                for outcome in group.outcomes.outcomes
            )
            enemy_hp_progress_signal_groups += 1
        else:
            selected_advantages = advantages.win
            selected_rollout_axis = CombatRolloutAxis.WIN
            selected_returns = tuple(
                1.0 if outcome.won else 0.0
                for outcome in group.outcomes.outcomes
            )
        replicate_count = len(group.outcomes.outcomes)
        decision_counts = [0] * replicate_count
        group_probabilities: list[SelectionProbability] = []
        for batch in group.batches:
            _validate_combat_batch(batch)
            for replicate_index in batch.replicate_indices:
                decision_counts[replicate_index] += 1
        if any(count == 0 for count in decision_counts):
            raise TorchOutcomeError(
                "combat win objective requires a retained decision for every replicate"
            )

        rollout_batches = None
        if objective_config.policy_update.uses_value_baseline:
            try:
                rollout_batches = build_complete_combat_rollout(group).batches
            except CombatRolloutError as error:
                raise TorchOutcomeError(str(error)) from error

        for batch_index, batch in enumerate(group.batches):
            payloads.append(batch.payload)
            selected_ordinals.extend(batch.selected_ordinals)
            group_probabilities.extend(batch.selection_probabilities)
            rollout_batch = (
                None if rollout_batches is None else rollout_batches[batch_index]
            )
            for row_index, replicate_index in enumerate(batch.replicate_indices):
                targets.append(
                    rollout_batch.rows[row_index].return_to_go(
                        selected_rollout_axis
                    )
                    if objective_config.policy_update.uses_value_baseline
                    else selected_advantages[replicate_index]
                )
                weights.append(
                    1.0
                    / (
                        len(normalized)
                        * replicate_count
                        * decision_counts[replicate_index]
                    )
                )
                value_head_indices.append(int(selected_rollout_axis))
        behavior_ids.append(group.behavior_manifest_id)
        probability_evidence.append(tuple(group_probabilities))
        total_replicates += replicate_count
        total_decisions += group.decision_count

    policy_loss = _combat_policy_weighted_loss(
        scorer=scorer,
        payloads=payloads,
        selected_ordinals=selected_ordinals,
        selection_probabilities=tuple(
            probability
            for group_probabilities in probability_evidence
            for probability in group_probabilities
        ),
        targets=targets,
        weights=weights,
        value_head_indices=value_head_indices,
        concat_limits=concat_limits,
        policy_config=policy_config,
        update_config=objective_config.policy_update,
        require_matching_propensities=require_matching_propensities,
        fixed_actor_advantages=fixed_actor_advantages,
        backward_microbatches=backward_microbatches,
    )
    return OnPolicyCombatWinLoss(
        value=policy_loss.value,
        group_count=len(normalized),
        signal_group_count=(
            win_signal_groups
            + terminal_hp_signal_groups
            + enemy_hp_progress_signal_groups
        ),
        win_signal_group_count=win_signal_groups,
        terminal_hp_signal_group_count=terminal_hp_signal_groups,
        enemy_hp_progress_signal_group_count=(
            enemy_hp_progress_signal_groups
        ),
        replicate_count=total_replicates,
        decision_count=total_decisions,
        behavior_manifest_ids=tuple(behavior_ids),
        selection_probabilities=tuple(probability_evidence),
        approximate_kl=(
            policy_loss.approximate_kl_weighted_sum
            / policy_loss.diagnostic_weight
        ),
        clip_fraction=policy_loss.clipped_weight / policy_loss.diagnostic_weight,
        entropy=(
            policy_loss.entropy_weighted_sum / policy_loss.diagnostic_weight
        ),
        value_loss=policy_loss.value_loss,
        actor_advantages=policy_loss.actor_advantages,
    )


def _decision_in_scope(
    decision: PublicTrajectoryDecisionV1,
    scope: RunDecisionScope,
) -> bool:
    if not isinstance(decision, PublicTrajectoryDecisionV1):
        raise TorchOutcomeError("run objective decision must be public and typed")
    if scope is RunDecisionScope.ALL:
        return True
    return not decision.public_snapshot.is_combat


def _validate_combat_batch(batch: object) -> None:
    if not isinstance(batch, CombatDecisionExperienceBatch):
        raise TorchOutcomeError(
            "complete combat groups must contain CombatDecisionExperienceBatch values"
        )
    if batch.decision_count != len(batch.replicate_indices):
        raise TorchOutcomeError("combat decision batch replicates are misaligned")
    if batch.decision_count != len(batch.selected_ordinals):
        raise TorchOutcomeError("combat decision batch ordinals are misaligned")
    if batch.decision_count != len(batch.selection_probabilities):
        raise TorchOutcomeError(
            "combat decision batch selection probabilities are misaligned"
        )
    if not all(
        isinstance(probability, SelectionProbability)
        for probability in batch.selection_probabilities
    ):
        raise TorchOutcomeError(
            "combat decision batch selection probabilities must be typed"
        )


def _on_policy_weighted_loss(
    *,
    scorer: CandidatePolicyScorer,
    payloads: Sequence[Mapping[str, object]],
    selected_ordinals: Sequence[int],
    selection_probabilities: Sequence[SelectionProbability],
    targets: Sequence[float],
    weights: Sequence[float],
    concat_limits: SemanticBatchConcatLimits,
    policy_config: RaggedCategoricalPolicyConfig,
    objective_name: str,
) -> Tensor:
    decision_count = len(selected_ordinals)
    if not (
        len(selection_probabilities)
        == len(targets)
        == len(weights)
        == decision_count
        > 0
    ):
        raise TorchOutcomeError(f"{objective_name} objective rows are misaligned")
    if not all(math.isfinite(target) for target in targets):
        raise TorchOutcomeError(f"{objective_name} objective targets must be finite")
    if not all(math.isfinite(weight) and weight > 0.0 for weight in weights):
        raise TorchOutcomeError(f"{objective_name} objective weights must be positive")

    combined = concatenate_semantic_decision_batches(payloads, concat_limits)
    logits = scorer(combined)
    if not isinstance(logits, RaggedCandidateLogits):
        raise TorchOutcomeError(
            "candidate policy scorer must return RaggedCandidateLogits"
        )
    selected_log_probabilities = _selected_log_probabilities(
        logits,
        selected_ordinals,
        policy_config,
    )
    _require_matching_propensities(
        logits,
        selected_ordinals,
        selection_probabilities,
        policy_config,
        scorer=scorer, decision_batch=combined,
    )
    target_tensor = torch.as_tensor(
        targets,
        dtype=selected_log_probabilities.dtype,
        device=selected_log_probabilities.device,
    )
    weight_tensor = torch.as_tensor(
        weights,
        dtype=selected_log_probabilities.dtype,
        device=selected_log_probabilities.device,
    )
    terms = -target_tensor * selected_log_probabilities
    if not bool(torch.all(torch.isfinite(terms))):
        raise TorchOutcomeError(f"{objective_name} policy loss terms must be finite")
    return torch.sum(terms * weight_tensor)


def _run_policy_weighted_loss(
    *,
    scorer: CandidatePolicyScorer,
    payloads: Sequence[Mapping[str, object]],
    selected_ordinals: Sequence[int],
    selection_probabilities: Sequence[SelectionProbability],
    targets: Sequence[float],
    value_weights: Sequence[float],
    actor_weights: Sequence[float],
    concat_limits: SemanticBatchConcatLimits,
    policy_config: RaggedCategoricalPolicyConfig,
    update_config: RunPolicyUpdateConfig,
    require_matching_propensities: bool,
    fixed_actor_advantages: Sequence[float] | None,
    fixed_value_predictions: Sequence[float] | None,
    value_prediction_offsets: Sequence[float] | None = None,
    terminal_score_targets: Sequence[float] | None = None,
) -> _RunPolicyLoss:
    if update_config.rule is RunPolicyUpdateRule.REINFORCE:
        if not require_matching_propensities:
            raise TorchOutcomeError("run REINFORCE cannot reuse an updated batch")
        return _RunPolicyLoss(
            value=_on_policy_weighted_loss(
                scorer=scorer,
                payloads=payloads,
                selected_ordinals=selected_ordinals,
                selection_probabilities=selection_probabilities,
                targets=targets,
                weights=actor_weights,
                concat_limits=concat_limits,
                policy_config=policy_config,
                objective_name="terminal",
            ),
            approximate_kl=0.0,
            clip_fraction=0.0,
            entropy=0.0,
            value_loss=0.0,
            value_clip_fraction=0.0,
            explained_variance=None,
            actor_decision_count=len(selected_ordinals),
            actor_advantages=tuple(float(target) for target in targets),
            critic_predictions=None,
            value_diagnostics=None,
        )

    decision_count = len(selected_ordinals)
    if not (
        len(selection_probabilities)
        == len(targets)
        == len(value_weights)
        == len(actor_weights)
        == decision_count
        > 0
    ):
        raise TorchOutcomeError("run PPO objective rows are misaligned")
    if not all(math.isfinite(target) for target in targets):
        raise TorchOutcomeError("run PPO objective targets must be finite")
    if not all(
        math.isfinite(weight) and weight > 0.0 for weight in value_weights
    ):
        raise TorchOutcomeError("run PPO value weights must be positive")
    if not all(
        math.isfinite(weight) and weight >= 0.0 for weight in actor_weights
    ):
        raise TorchOutcomeError("run PPO actor weights must be non-negative")
    recorded_log_probabilities: list[float] = []
    for evidence in selection_probabilities:
        if not isinstance(evidence, SelectionProbability):
            raise TorchOutcomeError("run PPO probabilities must be typed")
        value = evidence.value
        if value is None or not math.isfinite(value) or not 0.0 < value <= 1.0:
            raise TorchOutcomeError(
                "run PPO requires positive recorded probabilities"
            )
        recorded_log_probabilities.append(math.log(value))

    combined = concatenate_semantic_decision_batches(payloads, concat_limits)
    actor_critic = getattr(scorer, "actor_critic", None)
    if not callable(actor_critic):
        raise TorchOutcomeError("run value PPO requires an actor-critic scorer")
    actor_critic_output = actor_critic(combined)
    if not isinstance(actor_critic_output, RaggedActorCriticOutput):
        raise TorchOutcomeError("actor-critic scorer returned an invalid output")
    logits = actor_critic_output.logits
    predicted_values = actor_critic_output.row_values
    terminal_score_predictions = predicted_values
    if update_config.value_prediction_target == "terminal_score":
        if (
            value_prediction_offsets is None
            or len(value_prediction_offsets) != decision_count
            or not all(math.isfinite(value) for value in value_prediction_offsets)
            or terminal_score_targets is None
            or len(terminal_score_targets) != decision_count
            or not all(math.isfinite(value) for value in terminal_score_targets)
            or not all(
                math.isclose(target + offset, score, rel_tol=0.0, abs_tol=1e-12)
                for target, offset, score in zip(
                    targets, value_prediction_offsets, terminal_score_targets, strict=True
                )
            )
        ):
            raise TorchOutcomeError("terminal-score values require aligned public progress")
        # The head predicts the final score. Translate it into the existing
        # return-to-go units before advantage, clipping and diagnostics. This
        # known state baseline changes neither rewards nor the terminal goal.
        predicted_values = predicted_values - torch.as_tensor(
            value_prediction_offsets,
            dtype=predicted_values.dtype,
            device=predicted_values.device,
        )
    elif value_prediction_offsets is not None or terminal_score_targets is not None:
        raise TorchOutcomeError("return-to-go values cannot have progress offsets")
    selected_log_probabilities = _selected_log_probabilities(
        logits,
        selected_ordinals,
        policy_config,
    )
    if require_matching_propensities:
        _require_matching_propensities(
            logits,
            selected_ordinals,
            selection_probabilities,
            policy_config,
            scorer=scorer, decision_batch=combined,
        )
    old_log_probability_tensor = torch.as_tensor(
        recorded_log_probabilities,
        dtype=selected_log_probabilities.dtype,
        device=selected_log_probabilities.device,
    )
    target_tensor = torch.as_tensor(
        targets,
        dtype=selected_log_probabilities.dtype,
        device=selected_log_probabilities.device,
    )
    value_weight_tensor = torch.as_tensor(
        value_weights,
        dtype=selected_log_probabilities.dtype,
        device=selected_log_probabilities.device,
    )
    actor_weight_tensor = torch.as_tensor(
        actor_weights,
        dtype=selected_log_probabilities.dtype,
        device=selected_log_probabilities.device,
    )
    if predicted_values.shape != target_tensor.shape:
        raise TorchOutcomeError("run critic values are misaligned")
    pre_normalization_actor_advantages: Tensor | None = None
    advantage_normalization_applied = False
    value_error = predicted_values - target_tensor
    if terminal_score_targets is not None:
        # Compute in head units, rather than subtracting two rounded progress
        # translations. Identical final scores must retain identical signals.
        value_error = terminal_score_predictions - torch.as_tensor(
            terminal_score_targets, dtype=target_tensor.dtype, device=target_tensor.device
        )
    if fixed_actor_advantages is None:
        actor_advantages = -value_error.detach()
        pre_normalization_actor_advantages = actor_advantages
        actor_weight = torch.sum(actor_weight_tensor)
        if update_config.normalize_advantage:
            if bool(actor_weight > 0.0):
                advantage_normalization_applied = True
                eligible_advantages = actor_advantages[actor_weight_tensor > 0.0]
                advantage_mean = (
                    torch.sum(actor_advantages * actor_weight_tensor) / actor_weight
                )
                advantage_variance = torch.sum(
                    (actor_advantages - advantage_mean).square()
                    * actor_weight_tensor
                ) / actor_weight
                actor_advantages = (
                    actor_advantages - advantage_mean
                ) / torch.clamp_min(torch.sqrt(advantage_variance), 1e-8)
                if bool(torch.all(eligible_advantages == eligible_advantages[0])):
                    # Weighted float32 summation can round the mean of a
                    # constant signal. It must not manufacture an actor update.
                    actor_advantages = torch.zeros_like(actor_advantages)
        actor_advantages = torch.where(
            actor_weight_tensor > 0.0,
            actor_advantages,
            torch.zeros_like(actor_advantages),
        )
    else:
        if (
            len(fixed_actor_advantages) != decision_count
            or not all(math.isfinite(value) for value in fixed_actor_advantages)
        ):
            raise TorchOutcomeError(
                "fixed run advantages are misaligned or non-finite"
            )
        actor_advantages = torch.as_tensor(
            fixed_actor_advantages,
            dtype=target_tensor.dtype,
            device=target_tensor.device,
        )
        if bool(
            torch.any(
                (actor_weight_tensor == 0.0) & (actor_advantages != 0.0)
            )
        ):
            raise TorchOutcomeError("forced run rows must have zero fixed advantage")
    if fixed_value_predictions is None:
        old_values = predicted_values.detach()
    else:
        if (
            len(fixed_value_predictions) != decision_count
            or not all(math.isfinite(value) for value in fixed_value_predictions)
        ):
            raise TorchOutcomeError(
                "fixed run value predictions are misaligned or non-finite"
            )
        old_values = torch.as_tensor(
            fixed_value_predictions,
            dtype=target_tensor.dtype,
            device=target_tensor.device,
        )
    value_losses = value_error.square()
    value_clip_fraction = torch.zeros(
        (),
        dtype=target_tensor.dtype,
        device=target_tensor.device,
    )
    value_clip = update_config.value_clip_coefficient
    if value_clip is not None:
        clipped_values = old_values + torch.clamp(
            predicted_values - old_values,
            -value_clip,
            value_clip,
        )
        value_losses = torch.maximum(
            value_losses,
            (clipped_values - target_tensor).square(),
        )
        value_clip_fraction = torch.sum(
            ((predicted_values - old_values).abs() > value_clip).to(
                value_weight_tensor.dtype
            )
            * value_weight_tensor
        ) / torch.sum(value_weight_tensor)
    value_loss = 0.5 * torch.sum(value_losses * value_weight_tensor)
    log_ratio = selected_log_probabilities - old_log_probability_tensor
    ratio = torch.exp(log_ratio)
    clipped_ratio = torch.clamp(
        ratio,
        1.0 - update_config.clip_coefficient,
        1.0 + update_config.clip_coefficient,
    )
    policy_terms = torch.maximum(
        -actor_advantages * ratio,
        -actor_advantages * clipped_ratio,
    )
    entropies = _ragged_entropies(logits, policy_config)
    actor_loss = torch.sum(policy_terms * actor_weight_tensor) - (
        update_config.entropy_coefficient
        * torch.sum(entropies * actor_weight_tensor)
    )
    loss = update_config.value_loss_coefficient * value_loss
    if update_config.updates_actor:
        loss = actor_loss + loss
    actor_weight = torch.sum(actor_weight_tensor)
    if bool(actor_weight > 0.0):
        approximate_kl = torch.sum(
            ((ratio - 1.0) - log_ratio) * actor_weight_tensor
        ) / actor_weight
        clip_fraction = torch.sum(
            ((ratio - 1.0).abs() > update_config.clip_coefficient).to(
                actor_weight_tensor.dtype
            )
            * actor_weight_tensor
        ) / actor_weight
        mean_entropy = torch.sum(entropies * actor_weight_tensor) / actor_weight
    else:
        approximate_kl = torch.zeros_like(loss)
        clip_fraction = torch.zeros_like(loss)
        mean_entropy = torch.zeros_like(loss)
    explained_variance = _weighted_explained_variance(
        target_tensor.detach(),
        predicted_values.detach(),
        value_weight_tensor,
    )
    diagnostics = (
        loss,
        actor_loss,
        approximate_kl,
        clip_fraction,
        mean_entropy,
        value_loss,
        value_clip_fraction,
    )
    if not all(bool(torch.all(torch.isfinite(value))) for value in diagnostics):
        raise TorchOutcomeError("run PPO objective must be finite")
    detached_advantages = tuple(
        float(value) for value in actor_advantages.detach().cpu().tolist()
    )
    detached_predictions = tuple(
        float(value) for value in predicted_values.detach().cpu().tolist()
    )
    value_diagnostics = None
    if fixed_actor_advantages is None:
        assert pre_normalization_actor_advantages is not None
        detached_pre_normalization_advantages = tuple(
            float(value)
            for value in pre_normalization_actor_advantages.detach().cpu().tolist()
        )
        detached_targets = tuple(
            float(value) for value in target_tensor.detach().cpu().tolist()
        )
        detached_residuals = tuple(
            target - prediction
            for target, prediction in zip(
                detached_targets,
                detached_predictions,
                strict=True,
            )
        )
        value_diagnostics = RunValueDiagnostics(
            pre_normalization_actor_advantage=_optional_actor_signal_summary(
                detached_pre_normalization_advantages,
                actor_weights,
            ),
            actor_advantage=_optional_actor_signal_summary(
                detached_advantages,
                actor_weights,
            ),
            advantage_normalization_applied=advantage_normalization_applied,
            advantage_normalization_sign_changes=_eligible_sign_changes(
                detached_pre_normalization_advantages,
                detached_advantages,
                actor_weights,
            ),
            advantage_normalization_positive_from_nonpositive=(
                _eligible_direction_changes(
                    detached_pre_normalization_advantages,
                    detached_advantages,
                    actor_weights,
                    positive=True,
                )
            ),
            advantage_normalization_negative_from_nonnegative=(
                _eligible_direction_changes(
                    detached_pre_normalization_advantages,
                    detached_advantages,
                    actor_weights,
                    positive=False,
                )
            ),
            critic_prediction=_run_signal_summary(
                detached_predictions,
                value_weights,
            ),
            return_to_go_target=_run_signal_summary(
                detached_targets,
                value_weights,
            ),
            critic_residual=_run_signal_summary(
                detached_residuals,
                value_weights,
            ),
            actor_decision_count=sum(weight > 0.0 for weight in actor_weights),
            forced_decision_count=sum(weight == 0.0 for weight in actor_weights),
            explained_variance=explained_variance,
        )
    return _RunPolicyLoss(
        value=loss,
        approximate_kl=float(approximate_kl.detach().item()),
        clip_fraction=float(clip_fraction.detach().item()),
        entropy=float(mean_entropy.detach().item()),
        value_loss=float(value_loss.detach().item()),
        value_clip_fraction=float(value_clip_fraction.detach().item()),
        explained_variance=explained_variance,
        actor_decision_count=(
            sum(weight > 0.0 for weight in actor_weights)
            if update_config.updates_actor
            else 0
        ),
        actor_advantages=detached_advantages,
        critic_predictions=detached_predictions,
        value_diagnostics=value_diagnostics,
    )


def _run_signal_summary(
    values: Sequence[float],
    weights: Sequence[float],
) -> RunSignalSummary:
    """Summarize rows under the supplied actor or auxiliary value weights."""

    normalized_values = tuple(float(value) for value in values)
    normalized_weights = tuple(float(weight) for weight in weights)
    if (
        len(normalized_values) != len(normalized_weights)
        or not normalized_values
        or not all(math.isfinite(value) for value in normalized_values)
        or not all(
            math.isfinite(weight) and weight > 0.0
            for weight in normalized_weights
        )
    ):
        raise TorchOutcomeError("run value diagnostics are misaligned or non-finite")
    total_weight = math.fsum(normalized_weights)
    if not math.isfinite(total_weight) or total_weight <= 0.0:
        raise TorchOutcomeError("run value diagnostic weight must be positive")
    scaled_weights = tuple(weight / total_weight for weight in normalized_weights)
    weighted_mean = math.fsum(
        value * weight
        for value, weight in zip(
            normalized_values,
            scaled_weights,
            strict=True,
        )
    )
    weighted_variance = math.fsum(
        weight * (value - weighted_mean) ** 2
        for value, weight in zip(
            normalized_values,
            scaled_weights,
            strict=True,
        )
    )
    negative = tuple(value < 0.0 for value in normalized_values)
    zero = tuple(value == 0.0 for value in normalized_values)
    positive = tuple(value > 0.0 for value in normalized_values)
    return RunSignalSummary(
        decision_count=len(normalized_values),
        negative_decisions=sum(negative),
        zero_decisions=sum(zero),
        positive_decisions=sum(positive),
        negative_weight=math.fsum(
            weight
            for weight, selected in zip(scaled_weights, negative, strict=True)
            if selected
        ),
        zero_weight=math.fsum(
            weight
            for weight, selected in zip(scaled_weights, zero, strict=True)
            if selected
        ),
        positive_weight=math.fsum(
            weight
            for weight, selected in zip(scaled_weights, positive, strict=True)
            if selected
        ),
        weighted_mean=weighted_mean,
        weighted_standard_deviation=math.sqrt(max(0.0, weighted_variance)),
        minimum=min(normalized_values),
        maximum=max(normalized_values),
    )


def _optional_actor_signal_summary(
    values: Sequence[float],
    weights: Sequence[float],
) -> RunSignalSummary | None:
    eligible = tuple(
        (value, weight)
        for value, weight in zip(values, weights, strict=True)
        if weight > 0.0
    )
    if not eligible:
        return None
    return _run_signal_summary(
        tuple(value for value, _weight in eligible),
        tuple(weight for _value, weight in eligible),
    )


def _eligible_sign_changes(
    before: Sequence[float],
    after: Sequence[float],
    weights: Sequence[float],
) -> int:
    if not (len(before) == len(after) == len(weights)):
        raise TorchOutcomeError("run actor signal comparison is misaligned")
    return sum(
        _signal_sign(before_value) != _signal_sign(after_value)
        for before_value, after_value, weight in zip(
            before,
            after,
            weights,
            strict=True,
        )
        if weight > 0.0
    )


def _eligible_direction_changes(
    before: Sequence[float],
    after: Sequence[float],
    weights: Sequence[float],
    *,
    positive: bool,
) -> int:
    if not (len(before) == len(after) == len(weights)):
        raise TorchOutcomeError("run actor direction comparison is misaligned")
    if type(positive) is not bool:
        raise TorchOutcomeError("run actor direction must be bool")
    return sum(
        (
            before_value <= 0.0 and after_value > 0.0
            if positive
            else before_value >= 0.0 and after_value < 0.0
        )
        for before_value, after_value, weight in zip(
            before,
            after,
            weights,
            strict=True,
        )
        if weight > 0.0
    )


def _signal_sign(value: float) -> int:
    if not math.isfinite(value):
        raise TorchOutcomeError("run actor signal must be finite")
    return int(value > 0.0) - int(value < 0.0)


def _weighted_explained_variance(
    targets: Tensor,
    predictions: Tensor,
    weights: Tensor,
) -> float | None:
    """Return SB3-style explained variance under attempt-equal row weights."""

    total_weight = torch.sum(weights)
    target_mean = torch.sum(targets * weights) / total_weight
    target_variance = (
        torch.sum((targets - target_mean).square() * weights) / total_weight
    )
    if bool(target_variance <= 1e-12):
        return None
    residual = targets - predictions
    residual_mean = torch.sum(residual * weights) / total_weight
    residual_variance = (
        torch.sum((residual - residual_mean).square() * weights) / total_weight
    )
    explained = 1.0 - residual_variance / target_variance
    if not bool(torch.isfinite(explained)):
        raise TorchOutcomeError("run explained variance must be finite")
    return float(explained.detach().item())


def _combat_policy_weighted_loss(
    *,
    scorer: CandidatePolicyScorer,
    payloads: Sequence[Mapping[str, object]],
    selected_ordinals: Sequence[int],
    selection_probabilities: Sequence[SelectionProbability],
    targets: Sequence[float],
    weights: Sequence[float],
    value_head_indices: Sequence[int],
    concat_limits: SemanticBatchConcatLimits,
    policy_config: RaggedCategoricalPolicyConfig,
    update_config: CombatPolicyUpdateConfig,
    require_matching_propensities: bool,
    fixed_actor_advantages: Sequence[float] | None,
    backward_microbatches: bool,
) -> _CombatPolicyLoss:
    """Evaluate one objective through bounded, contiguous semantic batches."""

    normalized_payloads = tuple(payloads)
    chunks = plan_semantic_decision_batch_chunks(
        normalized_payloads,
        concat_limits,
    )
    decision_count = len(selected_ordinals)
    if sum(chunk.row_count for chunk in chunks) != decision_count:
        raise TorchOutcomeError(
            "combat policy payload rows are misaligned with objective rows"
        )
    if fixed_actor_advantages is not None and (
        len(fixed_actor_advantages) != decision_count
        or not all(math.isfinite(value) for value in fixed_actor_advantages)
    ):
        raise TorchOutcomeError(
            "fixed combat advantages are misaligned or non-finite"
        )

    parts: list[_CombatPolicyLoss] = []
    row_start = 0
    for chunk in chunks:
        row_stop = row_start + chunk.row_count
        fixed_chunk = (
            None
            if fixed_actor_advantages is None
            else fixed_actor_advantages[row_start:row_stop]
        )
        part = _combat_policy_microbatch_loss(
            scorer=scorer,
            payloads=normalized_payloads[
                chunk.start_batch_index : chunk.stop_batch_index
            ],
            selected_ordinals=selected_ordinals[row_start:row_stop],
            selection_probabilities=selection_probabilities[row_start:row_stop],
            targets=targets[row_start:row_stop],
            weights=weights[row_start:row_stop],
            value_head_indices=value_head_indices[row_start:row_stop],
            concat_limits=concat_limits,
            policy_config=policy_config,
            update_config=update_config,
            require_matching_propensities=require_matching_propensities,
            fixed_actor_advantages=fixed_chunk,
        )
        if backward_microbatches:
            part.value.backward()
            part = _CombatPolicyLoss(
                value=part.value.detach(),
                diagnostic_weight=part.diagnostic_weight,
                approximate_kl_weighted_sum=(
                    part.approximate_kl_weighted_sum
                ),
                clipped_weight=part.clipped_weight,
                entropy_weighted_sum=part.entropy_weighted_sum,
                value_loss=part.value_loss,
                actor_advantages=part.actor_advantages,
            )
        parts.append(part)
        row_start = row_stop

    value = parts[0].value
    for part in parts[1:]:
        value = value + part.value
    diagnostic_weight = sum(part.diagnostic_weight for part in parts)
    if not math.isfinite(diagnostic_weight) or diagnostic_weight <= 0.0:
        raise TorchOutcomeError("combat policy diagnostic weight must be positive")
    approximate_kl_weighted_sum = sum(
        part.approximate_kl_weighted_sum for part in parts
    )
    clipped_weight = sum(part.clipped_weight for part in parts)
    entropy_weighted_sum = sum(part.entropy_weighted_sum for part in parts)
    value_loss = sum(part.value_loss for part in parts)
    diagnostics = (
        approximate_kl_weighted_sum,
        clipped_weight,
        entropy_weighted_sum,
        value_loss,
    )
    if not all(math.isfinite(value) for value in diagnostics):
        raise TorchOutcomeError("combat PPO accumulated diagnostics must be finite")
    return _CombatPolicyLoss(
        value=value,
        diagnostic_weight=diagnostic_weight,
        approximate_kl_weighted_sum=approximate_kl_weighted_sum,
        clipped_weight=clipped_weight,
        entropy_weighted_sum=entropy_weighted_sum,
        value_loss=value_loss,
        actor_advantages=tuple(
            advantage
            for part in parts
            for advantage in part.actor_advantages
        ),
    )


def _combat_policy_microbatch_loss(
    *,
    scorer: CandidatePolicyScorer,
    payloads: Sequence[Mapping[str, object]],
    selected_ordinals: Sequence[int],
    selection_probabilities: Sequence[SelectionProbability],
    targets: Sequence[float],
    weights: Sequence[float],
    value_head_indices: Sequence[int],
    concat_limits: SemanticBatchConcatLimits,
    policy_config: RaggedCategoricalPolicyConfig,
    update_config: CombatPolicyUpdateConfig,
    require_matching_propensities: bool,
    fixed_actor_advantages: Sequence[float] | None,
) -> _CombatPolicyLoss:
    if not isinstance(update_config, CombatPolicyUpdateConfig):
        raise TorchOutcomeError("combat policy update config must be typed")
    if update_config.rule is CombatPolicyUpdateRule.REINFORCE:
        if not require_matching_propensities:
            raise TorchOutcomeError("REINFORCE cannot reuse an updated policy batch")
        return _CombatPolicyLoss(
            value=_on_policy_weighted_loss(
                scorer=scorer,
                payloads=payloads,
                selected_ordinals=selected_ordinals,
                selection_probabilities=selection_probabilities,
                targets=targets,
                weights=weights,
                concat_limits=concat_limits,
                policy_config=policy_config,
                objective_name="combat win-first",
            ),
            diagnostic_weight=float(sum(weights)),
            approximate_kl_weighted_sum=0.0,
            clipped_weight=0.0,
            entropy_weighted_sum=0.0,
            value_loss=0.0,
            actor_advantages=tuple(float(target) for target in targets),
        )

    decision_count = len(selected_ordinals)
    if not (
        len(selection_probabilities)
        == len(targets)
        == len(weights)
        == len(value_head_indices)
        == decision_count
        > 0
    ):
        raise TorchOutcomeError("combat PPO objective rows are misaligned")
    if not all(math.isfinite(target) for target in targets):
        raise TorchOutcomeError("combat PPO objective targets must be finite")
    if not all(math.isfinite(weight) and weight > 0.0 for weight in weights):
        raise TorchOutcomeError("combat PPO objective weights must be positive")
    recorded_log_probabilities: list[float] = []
    for evidence in selection_probabilities:
        if not isinstance(evidence, SelectionProbability):
            raise TorchOutcomeError(
                "combat PPO objective probabilities must be typed"
            )
        value = evidence.value
        if value is None or not math.isfinite(value) or not 0.0 < value <= 1.0:
            raise TorchOutcomeError(
                "combat PPO objective requires positive recorded probabilities"
            )
        recorded_log_probabilities.append(math.log(value))

    combined = concatenate_semantic_decision_batches(payloads, concat_limits)
    predicted_values: Tensor | None = None
    if update_config.uses_value_baseline:
        actor_critic_multi = getattr(scorer, "actor_critic_multi", None)
        if not callable(actor_critic_multi):
            raise TorchOutcomeError(
                "combat value PPO requires a multi actor-critic scorer"
            )
        actor_critic_output = actor_critic_multi(combined)
        if not isinstance(actor_critic_output, RaggedMultiActorCriticOutput):
            raise TorchOutcomeError(
                "multi actor-critic scorer returned an invalid output"
            )
        logits = actor_critic_output.logits
        all_predicted_values = actor_critic_output.row_values
        if all_predicted_values.shape[1] != COMBAT_ROLLOUT_VALUE_HEAD_WIDTH:
            raise TorchOutcomeError(
                "combat value PPO requires exactly three fixed value columns"
            )
        value_head_tensor = torch.as_tensor(
            value_head_indices,
            dtype=torch.long,
            device=all_predicted_values.device,
        )
        if bool(
            torch.any(value_head_tensor < 0)
            or torch.any(value_head_tensor >= all_predicted_values.shape[1])
        ):
            raise TorchOutcomeError("combat value head index is outside the scorer")
        predicted_values = all_predicted_values[
            torch.arange(
                decision_count,
                dtype=torch.long,
                device=all_predicted_values.device,
            ),
            value_head_tensor,
        ]
    else:
        logits = scorer(combined)
    if not isinstance(logits, RaggedCandidateLogits):
        raise TorchOutcomeError(
            "candidate policy scorer must return RaggedCandidateLogits"
        )
    selected_log_probabilities = _selected_log_probabilities(
        logits,
        selected_ordinals,
        policy_config,
    )
    if require_matching_propensities:
        _require_matching_propensities(
            logits,
            selected_ordinals,
            selection_probabilities,
            policy_config,
            scorer=scorer, decision_batch=combined,
        )
    old_log_probability_tensor = torch.as_tensor(
        recorded_log_probabilities,
        dtype=selected_log_probabilities.dtype,
        device=selected_log_probabilities.device,
    )
    target_tensor = torch.as_tensor(
        targets,
        dtype=selected_log_probabilities.dtype,
        device=selected_log_probabilities.device,
    )
    weight_tensor = torch.as_tensor(
        weights,
        dtype=selected_log_probabilities.dtype,
        device=selected_log_probabilities.device,
    )
    actor_advantages = target_tensor
    value_loss = torch.zeros(
        (),
        dtype=target_tensor.dtype,
        device=target_tensor.device,
    )
    if predicted_values is not None:
        if predicted_values.shape != target_tensor.shape:
            raise TorchOutcomeError(
                "combat critic values are misaligned"
            )
        if fixed_actor_advantages is None:
            actor_advantages = target_tensor - predicted_values.detach()
        else:
            if (
                len(fixed_actor_advantages) != decision_count
                or not all(
                    math.isfinite(value) for value in fixed_actor_advantages
                )
            ):
                raise TorchOutcomeError(
                    "fixed combat advantages are misaligned or non-finite"
                )
            actor_advantages = torch.as_tensor(
                fixed_actor_advantages,
                dtype=target_tensor.dtype,
                device=target_tensor.device,
            )
        value_error = predicted_values - target_tensor
        value_loss = 0.5 * torch.sum(value_error.square() * weight_tensor)
    log_ratio = selected_log_probabilities - old_log_probability_tensor
    ratio = torch.exp(log_ratio)
    clipped_ratio = torch.clamp(
        ratio,
        1.0 - update_config.clip_coefficient,
        1.0 + update_config.clip_coefficient,
    )
    policy_terms = torch.maximum(
        -actor_advantages * ratio,
        -actor_advantages * clipped_ratio,
    )
    entropies = _ragged_entropies(logits, policy_config)
    loss = torch.sum(policy_terms * weight_tensor) - (
        update_config.entropy_coefficient
        * torch.sum(entropies * weight_tensor)
    ) + update_config.value_loss_coefficient * value_loss
    diagnostic_weight = torch.sum(weight_tensor)
    approximate_kl_weighted_sum = torch.sum(
        ((ratio - 1.0) - log_ratio) * weight_tensor
    )
    clipped_weight = torch.sum(
        ((ratio - 1.0).abs() > update_config.clip_coefficient).to(
            weight_tensor.dtype
        )
        * weight_tensor
    )
    entropy_weighted_sum = torch.sum(entropies * weight_tensor)
    diagnostics = (
        loss,
        diagnostic_weight,
        approximate_kl_weighted_sum,
        clipped_weight,
        entropy_weighted_sum,
        value_loss,
    )
    if not all(bool(torch.all(torch.isfinite(value))) for value in diagnostics):
        raise TorchOutcomeError("combat PPO objective must be finite")
    return _CombatPolicyLoss(
        value=loss,
        diagnostic_weight=float(diagnostic_weight.detach().item()),
        approximate_kl_weighted_sum=float(
            approximate_kl_weighted_sum.detach().item()
        ),
        clipped_weight=float(clipped_weight.detach().item()),
        entropy_weighted_sum=float(entropy_weighted_sum.detach().item()),
        value_loss=float(value_loss.detach().item()),
        actor_advantages=tuple(
            float(value)
            for value in actor_advantages.detach().cpu().tolist()
        ),
    )


def _selected_log_probabilities(
    logits: RaggedCandidateLogits,
    selected_ordinals: Sequence[int],
    config: RaggedCategoricalPolicyConfig,
) -> Tensor:
    ordinals = torch.as_tensor(
        tuple(selected_ordinals),
        dtype=torch.long,
        device=logits.values.device,
    )
    if ordinals.ndim != 1 or ordinals.numel() != logits.row_count:
        raise TorchOutcomeError("selected ordinals must contain one value per row")
    lengths = logits.row_splits[1:] - logits.row_splits[:-1]
    if bool(torch.any(ordinals < 0)) or bool(torch.any(ordinals >= lengths)):
        raise TorchOutcomeError("selected ordinal is outside its candidate row")
    row_ids = torch.repeat_interleave(
        torch.arange(
            logits.row_count,
            dtype=torch.long,
            device=logits.values.device,
        ),
        lengths,
    )
    scaled = logits.values / config.temperature
    row_max = torch.full(
        (logits.row_count,),
        -torch.inf,
        dtype=scaled.dtype,
        device=scaled.device,
    )
    row_max.scatter_reduce_(0, row_ids, scaled.detach(), reduce="amax")
    shifted = scaled - row_max[row_ids]
    row_sum = torch.zeros(
        logits.row_count,
        dtype=scaled.dtype,
        device=scaled.device,
    )
    row_sum.index_add_(0, row_ids, torch.exp(shifted))
    log_probabilities = shifted - torch.log(row_sum[row_ids])
    return log_probabilities[logits.row_splits[:-1] + ordinals]


def _ragged_entropies(
    logits: RaggedCandidateLogits,
    config: RaggedCategoricalPolicyConfig,
) -> Tensor:
    lengths = logits.row_splits[1:] - logits.row_splits[:-1]
    row_ids = torch.repeat_interleave(
        torch.arange(
            logits.row_count,
            dtype=torch.long,
            device=logits.values.device,
        ),
        lengths,
    )
    scaled = logits.values / config.temperature
    row_max = torch.full(
        (logits.row_count,),
        -torch.inf,
        dtype=scaled.dtype,
        device=scaled.device,
    )
    row_max.scatter_reduce_(0, row_ids, scaled.detach(), reduce="amax")
    shifted = scaled - row_max[row_ids]
    row_sum = torch.zeros(
        logits.row_count,
        dtype=scaled.dtype,
        device=scaled.device,
    )
    row_sum.index_add_(0, row_ids, torch.exp(shifted))
    log_probabilities = shifted - torch.log(row_sum[row_ids])
    probabilities = torch.exp(log_probabilities)
    entropies = torch.zeros_like(row_sum)
    entropies.index_add_(
        0,
        row_ids,
        -(probabilities * log_probabilities),
    )
    return entropies


def _require_matching_propensities(
    logits: RaggedCandidateLogits,
    selected_ordinals: Sequence[int],
    recorded: Sequence[SelectionProbability],
    config: RaggedCategoricalPolicyConfig,
    *, scorer=None, decision_batch=None,
) -> None:
    if len(recorded) != logits.row_count:
        raise TorchOutcomeError("selection probabilities must contain one value per row")
    splits = logits.row_splits.detach().cpu().tolist()
    for row, (start, end, ordinal, evidence) in enumerate(
        zip(
            splits[:-1],
            splits[1:],
            selected_ordinals,
            recorded,
            strict=True,
        )
    ):
        if evidence.value is None:
            raise TorchOutcomeError(
                "on-policy objective requires known selection probabilities"
            )
        probabilities = torch.softmax(
            logits.values[start:end].detach().to(dtype=torch.float64)
            / config.temperature,
            dim=0,
        )
        expected = float(probabilities[ordinal].item())
        if not math.isclose(
            evidence.value,
            expected,
            rel_tol=1e-6,
            abs_tol=1e-8,
        ):
            # Float32 linear kernels can round differently when rollout rows
            # are concatenated for training. Only a small discrepancy may be
            # retried, using the same maintained scorer and exact single row.
            # The original propensity tolerance still applies to that replay.
            if (isinstance(scorer, RaggedCandidateScorer) and decision_batch is not None
                    and logits.values.dtype == torch.float32
                    and math.isclose(evidence.value, expected, rel_tol=1e-5, abs_tol=1e-8)):
                with torch.no_grad():
                    replay = scorer(select_semantic_decision_rows(decision_batch, (row,)))
                    if replay.row_count != 1 or replay.values.numel() != end-start:
                        raise TorchOutcomeError("propensity replay changed the candidate surface")
                    replay_probability = float(torch.softmax(
                        replay.values.detach().to(dtype=torch.float64)/config.temperature,
                        dim=0)[ordinal].item())
                if math.isclose(evidence.value, replay_probability, rel_tol=1e-6, abs_tol=1e-8):
                    continue
            raise TorchOutcomeError(
                f"selection probability at row {row} is off-policy: recorded={evidence.value:.12g}, recomputed={expected:.12g}"
            )
