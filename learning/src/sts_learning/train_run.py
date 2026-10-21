"""Bounded whole-run on-policy training from a published combat warm start."""

from __future__ import annotations

import argparse
import json
import operator
import time
from dataclasses import dataclass, replace
from pathlib import Path
from typing import TextIO

from .credit_assignment import (
    CreditAssignmentComparison,
    DecisionCreditDistribution,
)
from .driver import BatchEnvironment
from .evaluation import (
    HeldOutEvaluationResult,
    HeldOutEvaluationSpec,
    evaluate_held_out_behavior,
)
from .combat_potion_lane import CombatPotionLane
from .evaluate_run import RunPotionLane, resolve_run_potion_lane
from .published_combat_behavior import (
    PublishedCombatBehavior,
    recover_published_combat_behavior,
)
from .published_run_behavior import (
    RUN_TRAINING_SCHEMA,
    PublishedRunBehavior,
    recover_published_run_behavior,
)
from .run_sampling import (
    EpisodeRootRetryCurriculum,
    RunSamplingMode,
)
from .run_resource_trace import (
    ResourceTracingEnvironmentFactory,
    RunCombatResourceTransition,
    RunResourceTrace,
)
from .seeds import SeedPartition, SeedSchedule
from .terminal_returns import (
    OnPolicyObjectiveConfig,
    RunDecisionScope,
    RunPolicyUpdateConfig,
    RunPolicyUpdateRule,
    TerminalAdvantageMode,
)
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinSessionLimits,
)
from .torch_generation import CategoricalGenerationAdvanceResult
from .torch_behavior import FrozenCombatAnchor, FrozenDecisionRule
from .torch_outcomes import RunSignalSummary
from .torch_session import (
    CategoricalOnlineSession,
    CategoricalOnlineSessionFactory,
    NoRecoveryCurriculum,
)
from .torch_session_config import (
    CategoricalOnlineProfile,
    CategoricalOnlineSessionConfig,
    CategoricalSessionBridge,
    CategoricalSessionLimits,
    TorchSessionError,
    normalize_torch_device_type,
)
from .torch_policy import (
    RaggedScorerConfig,
    TorchPolicyError,
    require_matching_actor_state,
)
from .torch_training import RunPolicyTrainingResult


class RunTrainingCommandError(RuntimeError):
    """A bounded whole-run training command is malformed or incomplete."""


_ADVANTAGE_MODE_ARGUMENTS = {
    "auto": None,
    "raw-return": TerminalAdvantageMode.RAW_RETURN,
    "leave-one-out": TerminalAdvantageMode.LEAVE_ONE_OUT,
    "matched-floor": TerminalAdvantageMode.MATCHED_FLOOR_LEAVE_ONE_OUT,
    "matched-floor-context": (
        TerminalAdvantageMode.MATCHED_FLOOR_CONTEXT_LEAVE_ONE_OUT
    ),
    "matched-episode-floor-context": (
        TerminalAdvantageMode.MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT
    ),
    "decision-local-gae": TerminalAdvantageMode.DECISION_LOCAL_GAE,
}

_RUN_POLICY_UPDATE_ARGUMENTS = {
    "reinforce": RunPolicyUpdateConfig(),
    "ppo-clip-value": RunPolicyUpdateConfig.ppo_clip_value(),
    "critic-calibration": RunPolicyUpdateConfig.critic_calibration(),
}

_RUN_ADVANTAGE_NORMALIZATION_ARGUMENTS = ("auto", "on", "off")


def _resolve_run_policy_update(
    update_name: str,
    normalization: str,
    *,
    critic_fit_steps: int = 256,
    value_prediction_target: str = "return_to_go",
) -> RunPolicyUpdateConfig:
    try:
        update = _RUN_POLICY_UPDATE_ARGUMENTS[update_name]
    except KeyError as error:
        raise RunTrainingCommandError(
            f"unknown run policy update {update_name!r}"
        ) from error
    if update.rule is RunPolicyUpdateRule.CRITIC_CALIBRATION:
        try:
            update = RunPolicyUpdateConfig.critic_calibration(
                steps=critic_fit_steps
            )
        except (TypeError, ValueError) as error:
            raise RunTrainingCommandError(str(error)) from error
    try:
        update = replace(update, value_prediction_target=value_prediction_target)
    except (TypeError, ValueError) as error:
        raise RunTrainingCommandError(str(error)) from error
    if normalization == "auto":
        return update
    if normalization not in _RUN_ADVANTAGE_NORMALIZATION_ARGUMENTS:
        raise RunTrainingCommandError(
            f"unknown run advantage normalization {normalization!r}"
        )
    normalize = normalization == "on"
    if update.rule is RunPolicyUpdateRule.CRITIC_CALIBRATION:
        raise RunTrainingCommandError(
            "critic calibration has no actor advantage normalization"
        )
    if normalize and not update.uses_value_baseline:
        raise RunTrainingCommandError(
            "run advantage normalization requires value PPO"
        )
    return replace(update, normalize_advantage=normalize)


@dataclass(frozen=True)
class RunTrainingCommandConfig:
    warm_start_behavior: Path
    output: Path
    slot_count: int
    generations: int
    attempts_per_update: int
    max_batch_steps_per_generation: int
    model_seed: int
    behavior_seed: int
    training_seed_start: int
    evaluation_attempts: int
    evaluation_max_batch_steps: int
    evaluation_behavior_seed: int
    held_out_seed_start: int
    ascension_level: int
    advantage_mode: TerminalAdvantageMode | None = None
    decision_scope: RunDecisionScope = RunDecisionScope.ALL
    combat_decision_rule: FrozenDecisionRule = FrozenDecisionRule.SAMPLED
    policy_update: RunPolicyUpdateConfig = RunPolicyUpdateConfig()
    sampling_mode: RunSamplingMode = RunSamplingMode.INDEPENDENT_COHORTS
    episode_root_attempts: int | None = None
    potion_lane: RunPotionLane = RunPotionLane.TRAINED
    device_type: str = "cpu"
    critic_initialization_behavior: Path | None = None

    def __post_init__(self) -> None:
        try:
            device_type = normalize_torch_device_type(self.device_type)
        except TorchSessionError as error:
            raise RunTrainingCommandError(str(error)) from error
        object.__setattr__(self, "device_type", device_type)
        behavior = Path(self.warm_start_behavior).resolve()
        output = Path(self.output).resolve()
        if not behavior.is_dir():
            raise RunTrainingCommandError(
                "run training warm-start behavior is not a directory"
            )
        if output.exists() and (
            not output.is_dir() or any(output.iterdir())
        ):
            raise RunTrainingCommandError(
                "run training output must be absent or empty"
            )
        if output == behavior or behavior in output.parents:
            raise RunTrainingCommandError(
                "run training output must stay outside the warm-start behavior"
            )
        object.__setattr__(self, "warm_start_behavior", behavior)
        object.__setattr__(self, "output", output)
        if not isinstance(self.policy_update, RunPolicyUpdateConfig):
            raise RunTrainingCommandError(
                "run training policy update must be typed"
            )
        critic_initialization = self.critic_initialization_behavior
        if critic_initialization is not None:
            critic_initialization = Path(critic_initialization).resolve()
            if not critic_initialization.is_dir():
                raise RunTrainingCommandError(
                    "critic initialization behavior is not a directory"
                )
            if critic_initialization == behavior:
                raise RunTrainingCommandError(
                    "critic initialization must be distinct from the combat warm start"
                )
            if (
                output == critic_initialization
                or critic_initialization in output.parents
            ):
                raise RunTrainingCommandError(
                    "run training output must stay outside critic initialization"
                )
            if self.policy_update.rule is not RunPolicyUpdateRule.PPO_CLIP_VALUE:
                raise RunTrainingCommandError(
                    "critic initialization is accepted only by actor-critic PPO"
                )
            object.__setattr__(
                self,
                "critic_initialization_behavior",
                critic_initialization,
            )
        advantage_mode = self.advantage_mode
        if advantage_mode is None:
            advantage_mode = (
                TerminalAdvantageMode.DECISION_LOCAL_GAE
                if self.policy_update.uses_value_baseline
                else TerminalAdvantageMode.RAW_RETURN
            )
            object.__setattr__(self, "advantage_mode", advantage_mode)
        if not isinstance(advantage_mode, TerminalAdvantageMode):
            raise RunTrainingCommandError(
                "run training advantage mode must be typed"
            )
        if not isinstance(self.decision_scope, RunDecisionScope):
            raise RunTrainingCommandError(
                "run training decision scope must be typed"
            )
        if not isinstance(self.combat_decision_rule, FrozenDecisionRule):
            raise RunTrainingCommandError(
                "run training combat decision rule must be typed"
            )
        if (
            self.combat_decision_rule is FrozenDecisionRule.GREEDY
            and self.decision_scope is not RunDecisionScope.STRATEGIC
        ):
            raise RunTrainingCommandError(
                "combat-greedy run training requires strategic decision scope"
            )
        if not isinstance(self.sampling_mode, RunSamplingMode):
            raise RunTrainingCommandError(
                "run training sampling mode must be typed"
            )
        if not isinstance(self.potion_lane, RunPotionLane):
            raise RunTrainingCommandError(
                "run training potion lane must be typed"
            )
        for name in (
            "slot_count",
            "attempts_per_update",
            "max_batch_steps_per_generation",
            "evaluation_attempts",
            "evaluation_max_batch_steps",
        ):
            object.__setattr__(self, name, _positive(getattr(self, name), name))
        if self.attempts_per_update % self.slot_count != 0:
            raise RunTrainingCommandError(
                "attempts_per_update must contain complete slot cohorts"
            )
        paired_mode = (
            TerminalAdvantageMode.MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT
        )
        if self.sampling_mode is RunSamplingMode.EPISODE_ROOT_RETRIES:
            if self.episode_root_attempts is None:
                raise RunTrainingCommandError(
                    "episode-root retries require episode_root_attempts"
                )
            episode_root_attempts = _positive(
                self.episode_root_attempts,
                "episode_root_attempts",
            )
            if episode_root_attempts < 2:
                raise RunTrainingCommandError(
                    "episode_root_attempts must be at least two"
                )
            if episode_root_attempts > self.attempts_per_update:
                raise RunTrainingCommandError(
                    "episode_root_attempts cannot exceed attempts_per_update"
                )
            object.__setattr__(
                self,
                "episode_root_attempts",
                episode_root_attempts,
            )
            if self.slot_count != 1:
                raise RunTrainingCommandError(
                    "episode-root retry sampling requires slot_count=1"
                )
            if self.advantage_mode is not paired_mode:
                raise RunTrainingCommandError(
                    "episode-root retries require episode-matched credit"
                )
        else:
            if self.episode_root_attempts is not None:
                raise RunTrainingCommandError(
                    "episode_root_attempts require episode-root retries"
                )
            if self.advantage_mode is paired_mode:
                raise RunTrainingCommandError(
                    "episode-matched credit requires episode-root retries"
                )
        object.__setattr__(
            self,
            "generations",
            _nonnegative(self.generations, "generations"),
        )
        for name in (
            "model_seed",
            "behavior_seed",
            "training_seed_start",
            "evaluation_behavior_seed",
            "held_out_seed_start",
        ):
            object.__setattr__(self, name, _seed(getattr(self, name), name))
        ascension_level = _nonnegative(self.ascension_level, "ascension_level")
        if ascension_level > 20:
            raise RunTrainingCommandError("ascension_level must be at most 20")
        object.__setattr__(self, "ascension_level", ascension_level)


def _recover_critic_initialization(
    config: RunTrainingCommandConfig,
    warm_start: PublishedCombatBehavior,
    combat_anchor: FrozenCombatAnchor | None,
    potion_lane: CombatPotionLane,
    run_bridge: CategoricalSessionBridge,
) -> PublishedRunBehavior | None:
    source = config.critic_initialization_behavior
    if source is None:
        return None
    recovered = recover_published_run_behavior(
        source,
        run_bridge,
        (config.behavior_seed,),
    )
    if (
        recovered.objective.policy_update.rule
        is not RunPolicyUpdateRule.CRITIC_CALIBRATION
    ):
        raise RunTrainingCommandError(
            "critic initialization was not published by critic calibration"
        )
    if recovered.training_ascension_level != config.ascension_level:
        raise RunTrainingCommandError(
            "critic initialization ascension differs from actor training"
        )
    if recovered.training_potion_lane is not potion_lane:
        raise RunTrainingCommandError(
            "critic initialization potion lane differs from actor training"
        )
    if recovered.training_sampling_mode is not RunSamplingMode.INDEPENDENT_COHORTS:
        raise RunTrainingCommandError(
            "critic initialization requires independent calibration cohorts"
        )
    if recovered.training_combat_decision_rule is not config.combat_decision_rule:
        raise RunTrainingCommandError(
            "critic initialization combat rule differs from actor training"
        )
    if recovered.objective.decision_scope is not config.decision_scope:
        raise RunTrainingCommandError(
            "critic initialization decision scope differs from actor training"
        )
    if recovered.objective.policy_update.value_prediction_target != config.policy_update.value_prediction_target:
        raise RunTrainingCommandError("critic initialization value target differs from actor training")
    if combat_anchor is None:
        if recovered.combat_anchor_manifest_id is not None:
            raise RunTrainingCommandError(
                "sampled critic initialization unexpectedly owns a combat anchor"
            )
    elif (
        recovered.combat_anchor_manifest_id != combat_anchor.manifest_id
        or recovered.combat_anchor_checkpoint_id != combat_anchor.checkpoint_id
        or recovered.combat_anchor_scorer != combat_anchor.scorer.config
    ):
        raise RunTrainingCommandError(
            "critic initialization changed the immutable combat anchor"
        )
    scorer = recovered.policies[0].frozen_scorer
    if not scorer.config.value_head or scorer.config.value_head_width != 1:
        raise RunTrainingCommandError(
            "critic initialization does not contain one scalar value head"
        )
    try:
        require_matching_actor_state(
            warm_start.policies[0].frozen_scorer,
            scorer,
        )
    except TorchPolicyError as error:
        raise RunTrainingCommandError(str(error)) from error
    return recovered


def run_run_training(
    config: RunTrainingCommandConfig,
    *,
    combat_bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
) -> dict[str, object]:
    """Train complete runs, publish once, then evaluate one held-out prefix."""

    if not isinstance(config, RunTrainingCommandConfig):
        raise RunTrainingCommandError("run training config must be typed")
    active_combat_bridge = (
        combat_bridge
        if combat_bridge is not None
        else CombatSessionBridge.installed()
    )
    active_run_bridge = (
        run_bridge
        if run_bridge is not None
        else CategoricalSessionBridge.installed()
    )
    if not isinstance(active_combat_bridge, CombatSessionBridge):
        raise RunTrainingCommandError("run training combat bridge must be typed")
    if not isinstance(active_run_bridge, CategoricalSessionBridge):
        raise RunTrainingCommandError(
            "run training environment bridge must be typed"
        )
    if active_combat_bridge.semantic_schema != active_run_bridge.semantic_schema:
        raise RunTrainingCommandError(
            "combat warm start and run environment semantic schemas differ"
        )
    warm_start = recover_published_combat_behavior(
        config.warm_start_behavior,
        active_combat_bridge,
        CombatWinSessionLimits(),
        (config.behavior_seed,),
    )
    combat_anchor = (
        FrozenCombatAnchor.from_behavior(warm_start.policies[0])
        if config.combat_decision_rule is FrozenDecisionRule.GREEDY
        else None
    )
    potion_lane = resolve_run_potion_lane(config.potion_lane, warm_start)
    critic_initialization = _recover_critic_initialization(
        config,
        warm_start,
        combat_anchor,
        potion_lane,
        active_run_bridge,
    )
    base_training_run_bridge = _bridge_for_potion_lane(
        active_run_bridge,
        potion_lane,
    )
    training_resource_factory = ResourceTracingEnvironmentFactory(
        lambda seeds: base_training_run_bridge.environment(
            seeds,
            config.ascension_level,
        )
    )

    def traced_training_environment(
        seeds: list[int],
        ascension_level: int,
    ) -> BatchEnvironment:
        if ascension_level != config.ascension_level:
            raise RunTrainingCommandError(
                "training environment ascension changed after configuration"
            )
        return training_resource_factory(seeds)

    training_run_bridge = replace(
        base_training_run_bridge,
        environment=traced_training_environment,
    )
    profile = replace(
        CategoricalOnlineProfile(),
        scorer=RaggedScorerConfig(
            value_head=config.policy_update.uses_value_baseline,
        ),
        objective=OnPolicyObjectiveConfig(
            attempts_per_update=config.attempts_per_update,
            advantage_mode=config.advantage_mode,
            decision_scope=config.decision_scope,
            policy_update=config.policy_update,
        ),
        combat_decision_rule=config.combat_decision_rule,
        combat_anchor_manifest_id=(
            None if combat_anchor is None else combat_anchor.manifest_id
        ),
        combat_anchor_scorer=(
            None if combat_anchor is None else combat_anchor.scorer.config
        ),
        device_type=config.device_type,
    )
    limits = replace(
        CategoricalSessionLimits(),
        owner_capacity=max(16, config.generations + 2),
    )
    retry_sampling = (
        config.sampling_mode is RunSamplingMode.EPISODE_ROOT_RETRIES
    )
    if retry_sampling:
        episode_root_attempts = config.episode_root_attempts
        if episode_root_attempts is None:
            raise AssertionError("validated retry sampling lost its attempt cap")
        recovery_budget = episode_root_attempts - 1
        curriculum = EpisodeRootRetryCurriculum(
            config.attempts_per_update,
            episode_root_attempts,
        )
    else:
        recovery_budget = 0
        curriculum = NoRecoveryCurriculum()
    session_config = CategoricalOnlineSessionConfig(
        ascension_level=config.ascension_level,
        schedule=SeedSchedule(
            SeedPartition.TRAINING,
            next_candidate=config.training_seed_start,
        ),
        slot_count=config.slot_count,
        max_recoveries_per_episode=recovery_budget,
        profile=profile,
        limits=limits,
    )
    factory = CategoricalOnlineSessionFactory(
        config.output,
        training_run_bridge,
        session_config,
        curriculum,
    )
    session = factory.new(
        model_seed=config.model_seed,
        behavior_seed=config.behavior_seed,
        initial_scorer=(
            warm_start.policies[0].frozen_scorer
            if critic_initialization is None
            else critic_initialization.policies[0].frozen_scorer
        ),
        initial_scorer_actor_only=critic_initialization is None,
        combat_anchor=combat_anchor,
    )
    config.output.mkdir(parents=True, exist_ok=True)
    journal_path = config.output / "training.jsonl"
    with journal_path.open("x", encoding="utf-8", newline="\n") as journal:
        _write(
            journal,
            _configuration(
                config,
                warm_start,
                combat_anchor,
                critic_initialization,
                potion_lane,
            ),
        )
        for generation in range(config.generations):
            started = time.perf_counter()
            result = session.advance_generation(
                max_batch_steps=config.max_batch_steps_per_generation
            )
            elapsed = time.perf_counter() - started
            training_resources = training_resource_factory.trace
            _write(
                journal,
                _generation(
                    generation,
                    result,
                    session,
                    elapsed,
                    training_resources,
                ),
            )
            progress = result.terminal_progress
            credit = session.runner.trainer.last_credit_assignment
            print(
                f"run_generation={generation} promoted={str(result.promoted).lower()} "
                f"attempts={result.terminal_attempts} "
                f"victories={result.terminal_victories} "
                f"defeats={result.terminal_defeats} "
                f"episodes={result.sampled_episodes} "
                f"recoveries={result.recoveries} "
                f"floor_sum={progress.floor_sum} "
                f"floor_counts={_counts(progress.floor_counts)} "
                f"combat_rule={config.combat_decision_rule.value} "
                f"batch_steps={result.batch_steps}/"
                f"{config.max_batch_steps_per_generation} "
                f"loss={_optional_float(session.runner.trainer.snapshot.last_loss)} "
                f"update={_training_result_line(session)} "
                "early_resources_cumulative="
                f"{_early_resource_line(training_resources)} "
                f"credit={_credit_line(credit)} "
                f"credit_floors={_credit_floor_line(credit)} "
                f"credit_scopes={_credit_scope_line(credit)} "
                f"credit_contexts={_credit_context_line(credit)} "
                f"credit_roots={_credit_root_line(credit)} "
                f"seconds={elapsed:.3f}",
                flush=True,
            )
            if not result.promoted:
                raise RunTrainingCommandError(
                    "run training hit its batch-step bound before one update; "
                    "the partial live update was not published"
                )

        publication = session.runner.controller.publish_active()
        active_manifest_id = publication.manifest_id
        evaluation_policy = factory.recover_behavior(
            active_manifest_id,
            behavior_seed=config.evaluation_behavior_seed,
        )
        held_out_resource_factory = ResourceTracingEnvironmentFactory(
            lambda seeds: base_training_run_bridge.environment(
                seeds,
                config.ascension_level,
            )
        )
        evaluation = evaluate_held_out_behavior(
            held_out_resource_factory,
            evaluation_policy,
            schedule=SeedSchedule(
                SeedPartition.HELD_OUT,
                next_candidate=config.held_out_seed_start,
            ),
            spec=HeldOutEvaluationSpec(
                slot_count=1,
                terminal_attempt_target=config.evaluation_attempts,
                max_batch_steps=config.evaluation_max_batch_steps,
            ),
        )
        summary = _summary(
            config,
            warm_start,
            combat_anchor,
            critic_initialization,
            session,
            publication.checkpoint_id.digest.hex(),
            evaluation,
            potion_lane,
            training_resource_factory.trace,
            held_out_resource_factory.trace,
        )
        _write(journal, summary)

    with (config.output / "summary.json").open(
        "x",
        encoding="utf-8",
        newline="\n",
    ) as destination:
        json.dump(summary, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    run = evaluation.run.summary
    print(
        "run_training_complete=true "
        f"ascension={config.ascension_level} "
        f"potion_lane={potion_lane.value} "
        f"potion_lane_request={config.potion_lane.value} "
        f"sampling_mode={config.sampling_mode.value} "
        f"episode_root_attempts={config.episode_root_attempts or 'none'} "
        f"generations={config.generations} "
        f"optimizer_steps={session.runner.trainer.snapshot.optimizer_steps} "
        f"held_out_attempts={run.terminal_attempts}/"
        f"{config.evaluation_attempts} "
        f"held_out_victories={run.victories} "
        f"held_out_floor_sum={run.terminal_progress.floor_sum} "
        f"held_out_floor_counts={_counts(run.terminal_progress.floor_counts)} "
        f"combat_rule={config.combat_decision_rule.value} "
        f"held_out_early={_early_resource_line(held_out_resource_factory.trace)} "
        f"run_policy_update={config.policy_update.rule.name.lower()} "
        f"output={config.output}",
        flush=True,
    )
    return summary


def _configuration(
    config: RunTrainingCommandConfig,
    warm_start: PublishedCombatBehavior,
    combat_anchor: FrozenCombatAnchor | None,
    critic_initialization: PublishedRunBehavior | None,
    potion_lane: CombatPotionLane,
) -> dict[str, object]:
    return {
        "schema": RUN_TRAINING_SCHEMA,
        "kind": "configuration",
        "warm_start_behavior": str(config.warm_start_behavior),
        "warm_start_manifest_id": warm_start.manifest_id.digest.hex(),
        "warm_start_checkpoint_id": warm_start.checkpoint_id.digest.hex(),
        "warm_start_training_step": warm_start.training_step,
        "warm_start_artifact_sha256": warm_start.training_artifact_sha256,
        "warm_start_potion_lane": warm_start.training_potion_lane.value,
        "critic_initialization_behavior": (
            None
            if config.critic_initialization_behavior is None
            else str(config.critic_initialization_behavior)
        ),
        **_critic_initialization_record(critic_initialization),
        "requested_run_potion_lane": config.potion_lane.value,
        "run_potion_lane": potion_lane.value,
        "sampling_mode": config.sampling_mode.value,
        "episode_root_attempts": config.episode_root_attempts,
        "slot_count": config.slot_count,
        "device_type": config.device_type,
        "ascension_level": config.ascension_level,
        "generations": config.generations,
        "attempts_per_update": config.attempts_per_update,
        "advantage_mode": config.advantage_mode.name.lower(),
        "decision_scope": config.decision_scope.name.lower(),
        "combat_decision_rule": config.combat_decision_rule.value,
        "combat_anchor_manifest_id": (
            None
            if combat_anchor is None
            else combat_anchor.manifest_id.digest.hex()
        ),
        "combat_anchor_checkpoint_id": (
            None
            if combat_anchor is None
            else combat_anchor.checkpoint_id.digest.hex()
        ),
        "combat_anchor_scorer": (
            None
            if combat_anchor is None
            else _scorer_config(combat_anchor.scorer.config)
        ),
        "run_policy_update": config.policy_update.rule.name.lower(),
        "run_policy_value_prediction_target": config.policy_update.value_prediction_target,
        "run_policy_epochs": config.policy_update.epochs,
        "run_policy_clip_coefficient": config.policy_update.clip_coefficient,
        "run_policy_entropy_coefficient": (
            config.policy_update.entropy_coefficient
        ),
        "run_policy_max_grad_norm": config.policy_update.max_grad_norm,
        "run_policy_target_kl": config.policy_update.target_kl,
        "run_policy_value_loss_coefficient": (
            config.policy_update.value_loss_coefficient
        ),
        "run_policy_value_clip_coefficient": (
            config.policy_update.value_clip_coefficient
        ),
        "run_policy_normalize_advantage": (
            config.policy_update.normalize_advantage
        ),
        "max_batch_steps_per_generation": (
            config.max_batch_steps_per_generation
        ),
        "model_seed": config.model_seed,
        "behavior_seed": config.behavior_seed,
        "training_seed_start": config.training_seed_start,
        "evaluation_attempts": config.evaluation_attempts,
        "evaluation_max_batch_steps": config.evaluation_max_batch_steps,
        "evaluation_behavior_seed": config.evaluation_behavior_seed,
        "held_out_seed_start": config.held_out_seed_start,
    }


def _generation(
    generation: int,
    result: CategoricalGenerationAdvanceResult,
    session: CategoricalOnlineSession,
    elapsed: float,
    resources: RunResourceTrace,
) -> dict[str, object]:
    progress = result.terminal_progress
    trainer = session.runner.trainer.snapshot
    training = session.runner.trainer.last_result
    return {
        "schema": RUN_TRAINING_SCHEMA,
        "kind": "generation",
        "generation": generation,
        "promoted": result.promoted,
        "active_training_step_before": result.active_training_step_before,
        "optimizer_steps_after": result.optimizer_steps_after,
        "batch_steps": result.batch_steps,
        "terminal_attempts": result.terminal_attempts,
        "victories": result.terminal_victories,
        "defeats": result.terminal_defeats,
        "sampled_episodes": result.sampled_episodes,
        "recoveries": result.recoveries,
        "terminal_floor_sum": progress.floor_sum,
        "terminal_floor_counts": progress.floor_counts,
        "terminal_act_counts": progress.act_counts,
        "loss": trainer.last_loss,
        "optimizer_steps_applied": (
            None if training is None else training.optimizer_steps_applied
        ),
        "approximate_kl": (
            None if training is None else training.approximate_kl
        ),
        "clip_fraction": None if training is None else training.clip_fraction,
        "entropy": None if training is None else training.entropy,
        "value_loss": None if training is None else training.value_loss,
        "value_clip_fraction": (
            None if training is None else training.value_clip_fraction
        ),
        "explained_variance": (
            None if training is None else training.explained_variance
        ),
        "actor_decisions": (
            None if training is None else training.actor_decision_count
        ),
        "actor_update_applied": (
            None if training is None else training.actor_update_applied
        ),
        "gradient_norm": None if training is None else training.gradient_norm,
        "rollout_value_diagnostics": _rollout_value_diagnostics(training),
        "trained_decisions": trainer.trained_decisions,
        "training_resources_cumulative": _resource_aggregate(resources),
        "credit_assignment": _credit_assignment(
            session.runner.trainer.last_credit_assignment
        ),
        "elapsed_seconds": elapsed,
    }


def _rollout_value_diagnostics(
    training: RunPolicyTrainingResult | None,
) -> dict[str, object] | None:
    diagnostics = (
        None if training is None else training.rollout_value_diagnostics
    )
    if diagnostics is None:
        return None

    def signal(summary: RunSignalSummary) -> dict[str, object]:
        return {
            "decision_count": summary.decision_count,
            "negative_decisions": summary.negative_decisions,
            "zero_decisions": summary.zero_decisions,
            "positive_decisions": summary.positive_decisions,
            "negative_weight": summary.negative_weight,
            "zero_weight": summary.zero_weight,
            "positive_weight": summary.positive_weight,
            "weighted_mean": summary.weighted_mean,
            "weighted_standard_deviation": summary.weighted_standard_deviation,
            "minimum": summary.minimum,
            "maximum": summary.maximum,
        }

    return {
        "weighting": "attempt_equal",
        "optimization_target": "decision_local_return_to_go",
        "advantage_estimator": "gae",
        "gamma": 1.0,
        "gae_lambda": 1.0,
        "actor_mask": "multiple_candidates",
        "critic_residual_convention": "return_to_go_target_minus_prediction",
        "pre_normalization_actor_advantage": (
            None
            if diagnostics.pre_normalization_actor_advantage is None
            else signal(diagnostics.pre_normalization_actor_advantage)
        ),
        "actor_advantage": (
            None
            if diagnostics.actor_advantage is None
            else signal(diagnostics.actor_advantage)
        ),
        "advantage_normalization_applied": (
            diagnostics.advantage_normalization_applied
        ),
        "advantage_normalization_sign_changes": (
            diagnostics.advantage_normalization_sign_changes
        ),
        "advantage_normalization_positive_from_nonpositive": (
            diagnostics.advantage_normalization_positive_from_nonpositive
        ),
        "advantage_normalization_negative_from_nonnegative": (
            diagnostics.advantage_normalization_negative_from_nonnegative
        ),
        "critic_prediction": signal(diagnostics.critic_prediction),
        "return_to_go_target": signal(diagnostics.return_to_go_target),
        "critic_residual": signal(diagnostics.critic_residual),
        "actor_decisions": diagnostics.actor_decision_count,
        "forced_decisions": diagnostics.forced_decision_count,
        "explained_variance": diagnostics.explained_variance,
    }


def _summary(
    config: RunTrainingCommandConfig,
    warm_start: PublishedCombatBehavior,
    combat_anchor: FrozenCombatAnchor | None,
    critic_initialization: PublishedRunBehavior | None,
    session: CategoricalOnlineSession,
    checkpoint_id: str,
    evaluation: HeldOutEvaluationResult,
    potion_lane: CombatPotionLane,
    training_resources: RunResourceTrace,
    held_out_resources: RunResourceTrace,
) -> dict[str, object]:
    run = evaluation.run.summary
    progress = run.terminal_progress
    return {
        "schema": RUN_TRAINING_SCHEMA,
        "kind": "completed",
        "warm_start_manifest_id": warm_start.manifest_id.digest.hex(),
        "warm_start_checkpoint_id": warm_start.checkpoint_id.digest.hex(),
        **_critic_initialization_record(critic_initialization),
        "active_behavior_manifest_id": (
            session.active_behavior_manifest_id.digest.hex()
        ),
        "active_behavior_checkpoint_id": checkpoint_id,
        "requested_run_potion_lane": config.potion_lane.value,
        "run_potion_lane": potion_lane.value,
        "sampling_mode": config.sampling_mode.value,
        "episode_root_attempts": config.episode_root_attempts,
        "device_type": config.device_type,
        "ascension_level": config.ascension_level,
        "combat_decision_rule": config.combat_decision_rule.value,
        "combat_anchor_manifest_id": (
            None
            if combat_anchor is None
            else combat_anchor.manifest_id.digest.hex()
        ),
        "combat_anchor_checkpoint_id": (
            None
            if combat_anchor is None
            else combat_anchor.checkpoint_id.digest.hex()
        ),
        "combat_anchor_scorer": (
            None
            if combat_anchor is None
            else _scorer_config(combat_anchor.scorer.config)
        ),
        "run_policy_update": config.policy_update.rule.name.lower(),
        "run_policy_value_prediction_target": config.policy_update.value_prediction_target,
        "run_policy_normalize_advantage": (
            config.policy_update.normalize_advantage
        ),
        "generations": config.generations,
        "optimizer_steps": session.runner.trainer.snapshot.optimizer_steps,
        "training_resources_cumulative": _resource_aggregate(training_resources),
        "held_out_resources": _resource_aggregate(held_out_resources),
        "held_out_target_reached": evaluation.complete,
        "held_out_step_limit_reached": evaluation.step_limit_reached,
        "held_out_attempts": run.terminal_attempts,
        "held_out_victories": run.victories,
        "held_out_defeats": run.defeats,
        "held_out_floor_sum": progress.floor_sum,
        "held_out_floor_counts": progress.floor_counts,
        "held_out_act_counts": progress.act_counts,
        "held_out_batch_steps": run.batch_steps,
        "held_out_slot_count": 1,
        "held_out_seed_end": evaluation.schedule_end.next_candidate,
    }


def _critic_initialization_record(
    source: PublishedRunBehavior | None,
) -> dict[str, object]:
    return {
        "critic_initialization_manifest_id": (
            None if source is None else source.manifest_id.digest.hex()
        ),
        "critic_initialization_checkpoint_id": (
            None if source is None else source.checkpoint_id.digest.hex()
        ),
        "critic_initialization_training_step": (
            None if source is None else source.training_step
        ),
        "critic_initialization_policy_update": (
            None
            if source is None
            else source.objective.policy_update.rule.name.lower()
        ),
    }


def _scorer_config(config: RaggedScorerConfig) -> dict[str, object]:
    return {
        "hidden_dim": config.hidden_dim,
        "relation_layers": config.relation_layers,
        "value_head": config.value_head,
        "value_head_width": config.value_head_width,
    }


def _write(journal: TextIO, value: dict[str, object]) -> None:
    journal.write(json.dumps(value, separators=(",", ":"), sort_keys=True))
    journal.write("\n")
    journal.flush()


def _bridge_for_potion_lane(
    bridge: CategoricalSessionBridge,
    lane: CombatPotionLane,
) -> CategoricalSessionBridge:
    if lane is CombatPotionLane.ALL:
        return bridge
    if lane is not CombatPotionLane.NEVER:
        raise RunTrainingCommandError(
            "whole-run training supports only all or never potion lanes"
        )
    return replace(
        bridge,
        environment=bridge.environment_without_combat_potions,
        environment_from_checkpoint=_reject_no_potion_resume,
    )


def _reject_no_potion_resume(*_args: object, **_kwargs: object) -> object:
    raise RunTrainingCommandError(
        "no-potion whole-run training does not support resume checkpoints"
    )


def _positive(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized == 0:
        raise RunTrainingCommandError(f"{name} must be positive")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise RunTrainingCommandError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise RunTrainingCommandError(f"{name} must be an integer") from error
    if normalized < 0:
        raise RunTrainingCommandError(f"{name} must be non-negative")
    return normalized


def _seed(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized >= 1 << 63:
        raise RunTrainingCommandError(f"{name} must be below 2^63")
    return normalized


def _counts(values: tuple[tuple[int, int], ...]) -> str:
    return ",".join(f"{key}:{count}" for key, count in values) or "none"


def _optional_float(value: float | None) -> str:
    return "none" if value is None else f"{value:.9g}"


def _training_result_line(session: CategoricalOnlineSession) -> str:
    result = session.runner.trainer.last_result
    if result is None:
        return "none"
    return (
        f"steps:{result.optimizer_steps_applied};"
        f"actor_update:{str(result.actor_update_applied).lower()};"
        f"kl:{result.approximate_kl:.4g};"
        f"clip:{result.clip_fraction:.4g};"
        f"value_clip:{result.value_clip_fraction:.4g};"
        f"entropy:{result.entropy:.4g};"
        f"value:{result.value_loss:.4g};"
        f"grad:{result.gradient_norm:.4g}"
    )


def _early_resource_line(trace: RunResourceTrace) -> str:
    rows = _resource_aggregate(trace)["early_combat_ordinals"]
    assert isinstance(rows, list)
    if not rows:
        return "none"
    return ",".join(
        f"{row['ordinal']}:{row['count']}@"
        f"{row['end_hp_sum']}/{row['end_max_hp_sum']}"
        f"#{row['below_75_percent_count']}/{row['below_50_percent_count']}"
        for row in rows
    )


def _resource_aggregate(trace: RunResourceTrace) -> dict[str, object]:
    by_seed: dict[int, list[RunCombatResourceTransition]] = {}
    for transition in trace.combat_transitions:
        by_seed.setdefault(transition.start.seed, []).append(transition)
    ordinal_rows: list[dict[str, int]] = []
    for ordinal in range(1, 5):
        selected = tuple(
            transitions[ordinal - 1]
            for transitions in by_seed.values()
            if len(transitions) >= ordinal
        )
        if not selected:
            continue
        ordinal_rows.append(
            {
                "ordinal": ordinal,
                "count": len(selected),
                "start_hp_sum": sum(row.start.hp for row in selected),
                "start_max_hp_sum": sum(row.start.max_hp for row in selected),
                "end_hp_sum": sum(row.end.hp for row in selected),
                "end_max_hp_sum": sum(row.end.max_hp for row in selected),
                "net_hp_loss_sum": sum(row.hp_loss for row in selected),
                "below_75_percent_count": sum(
                    row.end.hp * 4 < row.end.max_hp * 3 for row in selected
                ),
                "below_50_percent_count": sum(
                    row.end.hp * 2 < row.end.max_hp for row in selected
                ),
            }
        )
    act_one = tuple(
        transition
        for transition in trace.combat_transitions
        if transition.start.act == 1
    )
    return {
        "attempt_count": len(trace.episode_endpoints),
        "combat_count": len(trace.combat_transitions),
        "net_combat_hp_loss_sum": trace.hp_loss_sum,
        "act_one_combat_count": len(act_one),
        "act_one_net_hp_loss_sum": sum(row.hp_loss for row in act_one),
        "potion_identity_losses": trace.potion_identity_losses,
        "potion_identity_gains": trace.potion_identity_gains,
        "early_combat_ordinals": ordinal_rows,
    }


def _credit_line(comparison: CreditAssignmentComparison | None) -> str:
    if comparison is None:
        return "unavailable"
    terminal = comparison.terminal_broadcast
    local = comparison.remaining_progress
    matched = comparison.matched_floor_advantage
    context = comparison.matched_floor_context_advantage
    episode_context = comparison.matched_episode_floor_context_advantage
    return (
        f"broadcast:{terminal.negative}/{terminal.zero}/{terminal.positive}"
        f"@{terminal.mean:.4f};"
        f"local:{local.negative}/{local.zero}/{local.positive}@{local.mean:.4f};"
        f"matched:{matched.negative}/{matched.zero}/{matched.positive}"
        f"@{matched.mean:.4f};"
        f"context:{context.negative}/{context.zero}/{context.positive}"
        f"@{context.mean:.4f};"
        f"episode_context:{episode_context.negative}/"
        f"{episode_context.zero}/{episode_context.positive}"
        f"@{episode_context.mean:.4f}"
    )


def _credit_floor_line(comparison: CreditAssignmentComparison | None) -> str:
    if comparison is None:
        return "unavailable"
    return ",".join(
        f"{row.floor}:{row.remaining_progress.decision_count}"
        f"@{row.terminal_broadcast.mean:.4f}>{row.remaining_progress.mean:.4f}"
        f"#{row.matched_floor_advantage.negative}/"
        f"{row.matched_floor_advantage.zero}/"
        f"{row.matched_floor_advantage.positive}"
        for row in comparison.by_decision_floor
    )


def _credit_scope_line(comparison: CreditAssignmentComparison | None) -> str:
    if comparison is None:
        return "unavailable"
    return ",".join(
        f"{'combat' if row.is_combat else 'strategic'}:"
        f"{row.remaining_progress.decision_count}"
        f"#{row.matched_floor_advantage.negative}/"
        f"{row.matched_floor_advantage.zero}/"
        f"{row.matched_floor_advantage.positive}"
        for row in comparison.by_combat_scope
    )


def _credit_context_line(comparison: CreditAssignmentComparison | None) -> str:
    if comparison is None:
        return "unavailable"
    return ",".join(
        f"{row.context_kind}:{row.remaining_progress.decision_count}"
        f"#{row.matched_floor_advantage.negative}/"
        f"{row.matched_floor_advantage.zero}/"
        f"{row.matched_floor_advantage.positive}"
        f"~{row.matched_floor_context_advantage.negative}/"
        f"{row.matched_floor_context_advantage.zero}/"
        f"{row.matched_floor_context_advantage.positive}"
        f"@{row.strategic_scope_weight:.4f}"
        f">{row.matched_floor_strategic_weighted_target:.4f}"
        f">{row.matched_floor_context_strategic_weighted_target:.4f}"
        for row in comparison.by_strategic_context
    )


def _credit_root_line(comparison: CreditAssignmentComparison | None) -> str:
    if comparison is None:
        return "unavailable"
    return ",".join(
        f"{row.episode_seed}:{row.episode_generation}:"
        f"{row.attempt_count}@{row.terminal_floor_min}-{row.terminal_floor_max}"
        f"#{_optional_credit_signs(row.strategic_matched_episode_floor_context_advantage)}"
        for row in comparison.by_episode_root
    )


def _optional_credit_signs(
    distribution: DecisionCreditDistribution | None,
) -> str:
    if distribution is None:
        return "none"
    return (
        f"{distribution.negative}/{distribution.zero}/{distribution.positive}"
    )


def _credit_assignment(
    comparison: CreditAssignmentComparison | None,
) -> dict[str, object] | None:
    if comparison is None:
        return None
    return {
        "attempt_count": comparison.attempt_count,
        "terminal_broadcast": _credit_distribution(
            comparison.terminal_broadcast
        ),
        "remaining_progress": _credit_distribution(
            comparison.remaining_progress
        ),
        "matched_floor_advantage": _credit_distribution(
            comparison.matched_floor_advantage
        ),
        "matched_floor_context_advantage": _credit_distribution(
            comparison.matched_floor_context_advantage
        ),
        "matched_episode_floor_context_advantage": _credit_distribution(
            comparison.matched_episode_floor_context_advantage
        ),
        "by_decision_floor": [
            {
                "floor": row.floor,
                "terminal_broadcast": _credit_distribution(
                    row.terminal_broadcast
                ),
                "remaining_progress": _credit_distribution(
                    row.remaining_progress
                ),
                "matched_floor_advantage": _credit_distribution(
                    row.matched_floor_advantage
                ),
            }
            for row in comparison.by_decision_floor
        ],
        "by_combat_scope": [
            {
                "is_combat": row.is_combat,
                "terminal_broadcast": _credit_distribution(
                    row.terminal_broadcast
                ),
                "remaining_progress": _credit_distribution(
                    row.remaining_progress
                ),
                "matched_floor_advantage": _credit_distribution(
                    row.matched_floor_advantage
                ),
            }
            for row in comparison.by_combat_scope
        ],
        "by_strategic_context": [
            {
                "context_kind": row.context_kind,
                "strategic_scope_weight": row.strategic_scope_weight,
                "matched_floor_strategic_weighted_target": (
                    row.matched_floor_strategic_weighted_target
                ),
                "matched_floor_context_strategic_weighted_target": (
                    row.matched_floor_context_strategic_weighted_target
                ),
                "terminal_broadcast": _credit_distribution(
                    row.terminal_broadcast
                ),
                "remaining_progress": _credit_distribution(
                    row.remaining_progress
                ),
                "matched_floor_advantage": _credit_distribution(
                    row.matched_floor_advantage
                ),
                "matched_floor_context_advantage": _credit_distribution(
                    row.matched_floor_context_advantage
                ),
            }
            for row in comparison.by_strategic_context
        ],
        "by_episode_root": [
            {
                "episode_seed": row.episode_seed,
                "episode_generation": row.episode_generation,
                "attempt_count": row.attempt_count,
                "terminal_floor_min": row.terminal_floor_min,
                "terminal_floor_max": row.terminal_floor_max,
                "terminal_floor_mean": row.terminal_floor_mean,
                "matched_episode_floor_context_advantage": (
                    _credit_distribution(
                        row.matched_episode_floor_context_advantage
                    )
                ),
                "strategic_matched_episode_floor_context_advantage": (
                    None
                    if row.strategic_matched_episode_floor_context_advantage is None
                    else _credit_distribution(
                        row.strategic_matched_episode_floor_context_advantage
                    )
                ),
            }
            for row in comparison.by_episode_root
        ],
    }


def _credit_distribution(
    distribution: DecisionCreditDistribution,
) -> dict[str, int | float]:
    return {
        "decision_count": distribution.decision_count,
        "negative": distribution.negative,
        "zero": distribution.zero,
        "positive": distribution.positive,
        "minimum": distribution.minimum,
        "maximum": distribution.maximum,
        "mean": distribution.mean,
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Warm-start bounded whole-run on-policy training from one "
            "published combat behavior."
        ),
    )
    parser.add_argument("--warm-start-behavior", type=Path, required=True)
    parser.add_argument(
        "--critic-initialization-behavior",
        type=Path,
        help=(
            "verified critic-calibration publication whose value head seeds "
            "actor-critic PPO"
        ),
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--slots", type=int, default=4)
    parser.add_argument("--generations", type=int, default=1)
    parser.add_argument("--attempts-per-update", type=int, default=8)
    parser.add_argument("--max-batch-steps", type=int, default=4096)
    parser.add_argument("--model-seed", type=int, default=0)
    parser.add_argument("--behavior-seed", type=int, default=90_000)
    parser.add_argument("--training-seed-start", type=int, default=0)
    parser.add_argument("--evaluation-attempts", type=int, default=16)
    parser.add_argument("--evaluation-max-batch-steps", type=int, default=4096)
    parser.add_argument("--evaluation-behavior-seed", type=int, default=100_000)
    parser.add_argument("--held-out-seed-start", type=int, default=1_000_000)
    parser.add_argument(
        "--ascension",
        type=int,
        choices=range(21),
        required=True,
    )
    parser.add_argument(
        "--advantage-mode",
        choices=tuple(_ADVANTAGE_MODE_ARGUMENTS),
        default="auto",
    )
    parser.add_argument(
        "--decision-scope",
        choices=("all", "strategic"),
        default="all",
    )
    parser.add_argument(
        "--combat-decision-rule",
        choices=tuple(rule.value for rule in FrozenDecisionRule),
        default=FrozenDecisionRule.SAMPLED.value,
    )
    parser.add_argument(
        "--run-policy-update",
        choices=tuple(_RUN_POLICY_UPDATE_ARGUMENTS),
        default="reinforce",
    )
    parser.add_argument(
        "--run-advantage-normalization",
        choices=_RUN_ADVANTAGE_NORMALIZATION_ARGUMENTS,
        default="auto",
        help=(
            "override value-PPO advantage normalization; auto preserves the "
            "selected update profile"
        ),
    )
    parser.add_argument("--run-value-prediction-target", choices=("return_to_go", "terminal_score"),
                        default="return_to_go")
    parser.add_argument(
        "--critic-fit-steps",
        type=int,
        default=256,
        help=(
            "fixed-cohort supervised value-head steps used only by "
            "critic-calibration"
        ),
    )
    parser.add_argument(
        "--sampling-mode",
        choices=tuple(mode.value for mode in RunSamplingMode),
        default=RunSamplingMode.INDEPENDENT_COHORTS.value,
    )
    parser.add_argument("--episode-root-attempts", type=int)
    parser.add_argument(
        "--device",
        choices=("cpu", "cuda"),
        default="cpu",
    )
    parser.add_argument(
        "--potion-lane",
        choices=tuple(lane.value for lane in RunPotionLane),
        default=RunPotionLane.TRAINED.value,
    )
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=arguments.warm_start_behavior,
            output=arguments.output,
            slot_count=arguments.slots,
            generations=arguments.generations,
            attempts_per_update=arguments.attempts_per_update,
            max_batch_steps_per_generation=arguments.max_batch_steps,
            model_seed=arguments.model_seed,
            behavior_seed=arguments.behavior_seed,
            training_seed_start=arguments.training_seed_start,
            evaluation_attempts=arguments.evaluation_attempts,
            evaluation_max_batch_steps=arguments.evaluation_max_batch_steps,
            evaluation_behavior_seed=arguments.evaluation_behavior_seed,
            held_out_seed_start=arguments.held_out_seed_start,
            ascension_level=arguments.ascension,
            advantage_mode=_ADVANTAGE_MODE_ARGUMENTS[arguments.advantage_mode],
            decision_scope=(
                RunDecisionScope.ALL
                if arguments.decision_scope == "all"
                else RunDecisionScope.STRATEGIC
            ),
            combat_decision_rule=FrozenDecisionRule(
                arguments.combat_decision_rule
            ),
            policy_update=_resolve_run_policy_update(
                arguments.run_policy_update,
                arguments.run_advantage_normalization,
                critic_fit_steps=arguments.critic_fit_steps,
                value_prediction_target=arguments.run_value_prediction_target,
            ),
            sampling_mode=RunSamplingMode(arguments.sampling_mode),
            episode_root_attempts=arguments.episode_root_attempts,
            potion_lane=RunPotionLane(arguments.potion_lane),
            device_type=arguments.device,
            critic_initialization_behavior=(
                arguments.critic_initialization_behavior
            ),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
