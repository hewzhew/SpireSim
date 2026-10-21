"""Bounded whole-run evaluation of one published frozen behavior."""

from __future__ import annotations

import argparse
import json
import operator
from dataclasses import dataclass
from enum import Enum
from pathlib import Path

from .combat_potion_lane import CombatPotionLane
from .evaluation import (
    HeldOutEvaluationResult,
    HeldOutEvaluationSpec,
    evaluate_held_out_behavior,
)
from .published_combat_behavior import (
    PublishedCombatBehavior,
    recover_published_combat_behavior,
)
from .published_run_behavior import (
    PublishedRunBehavior,
    is_run_training_publication,
    recover_published_run_behavior,
)
from .run_resource_trace import (
    ResourceTracingEnvironmentFactory,
    RunCombatResourceTransition,
    RunResourceTrace,
    RunSeedResourceSummary,
)
from .seeds import SeedPartition, SeedSchedule
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinSessionLimits,
)
from .torch_behavior import (
    FrozenCategoricalTorchPolicy,
    FrozenCombatAnchor,
    FrozenCombatGreedyTorchPolicy,
)
from .torch_session_config import CategoricalSessionBridge


RUN_EVALUATION_SCHEMA = "sts-learning-run-held-out-evaluation-v10"


class RunEvaluationCommandError(RuntimeError):
    """A bounded whole-run evaluation command is malformed."""


class RunPotionLane(Enum):
    """How a whole-run evaluation selects its combat potion surface."""

    TRAINED = "trained"
    ALL = "all"
    NEVER = "never"


@dataclass(frozen=True)
class RunEvaluationCommandConfig:
    behavior: Path
    output: Path
    slot_count: int
    terminal_attempts: int
    max_batch_steps: int
    behavior_seed: int
    ascension_level: int
    held_out_seed_start: int = 0
    potion_lane: RunPotionLane = RunPotionLane.TRAINED

    def __post_init__(self) -> None:
        behavior = Path(self.behavior).resolve()
        output = Path(self.output).resolve()
        if not behavior.is_dir():
            raise RunEvaluationCommandError(
                "published behavior is not a directory"
            )
        if output.exists() and (
            not output.is_dir() or any(output.iterdir())
        ):
            raise RunEvaluationCommandError(
                "run evaluation output must be absent or empty"
            )
        if output == behavior or behavior in output.parents:
            raise RunEvaluationCommandError(
                "run evaluation output must stay outside the behavior directory"
            )
        object.__setattr__(self, "behavior", behavior)
        object.__setattr__(self, "output", output)
        object.__setattr__(
            self,
            "slot_count",
            _positive(self.slot_count, "slot_count"),
        )
        if self.slot_count != 1:
            raise RunEvaluationCommandError(
                "exact whole-run evaluation requires slot_count=1"
            )
        object.__setattr__(
            self,
            "terminal_attempts",
            _positive(self.terminal_attempts, "terminal_attempts"),
        )
        object.__setattr__(
            self,
            "max_batch_steps",
            _positive(self.max_batch_steps, "max_batch_steps"),
        )
        object.__setattr__(
            self,
            "behavior_seed",
            _seed(self.behavior_seed, "behavior_seed"),
        )
        ascension_level = _seed(self.ascension_level, "ascension_level")
        if ascension_level > 20:
            raise RunEvaluationCommandError("ascension_level must be at most 20")
        object.__setattr__(self, "ascension_level", ascension_level)
        object.__setattr__(
            self,
            "held_out_seed_start",
            _seed(self.held_out_seed_start, "held_out_seed_start"),
        )
        if not isinstance(self.potion_lane, RunPotionLane):
            raise RunEvaluationCommandError(
                "run evaluation potion lane must be typed"
            )


def run_run_evaluation(
    config: RunEvaluationCommandConfig,
    *,
    combat_bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
) -> dict[str, object]:
    """Run a frozen behavior over complete held-out episodes without recovery."""

    if not isinstance(config, RunEvaluationCommandConfig):
        raise RunEvaluationCommandError(
            "run evaluation config must be typed"
        )
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
        raise RunEvaluationCommandError(
            "run evaluation combat bridge must be typed"
        )
    if not isinstance(active_run_bridge, CategoricalSessionBridge):
        raise RunEvaluationCommandError(
            "run evaluation environment bridge must be typed"
        )
    if active_combat_bridge.semantic_schema != active_run_bridge.semantic_schema:
        raise RunEvaluationCommandError(
            "combat behavior and run environment semantic schemas differ"
        )

    recovered: PublishedCombatBehavior | PublishedRunBehavior
    if is_run_training_publication(config.behavior):
        recovered = recover_published_run_behavior(
            config.behavior,
            active_run_bridge,
            (config.behavior_seed,),
        )
    else:
        recovered = recover_published_combat_behavior(
            config.behavior,
            active_combat_bridge,
            CombatWinSessionLimits(),
            (config.behavior_seed,),
        )
    return run_recovered_run_evaluation(
        config,
        recovered,
        recovered.policies[0],
        run_bridge=active_run_bridge,
    )


def run_recovered_run_evaluation(
    config: RunEvaluationCommandConfig,
    recovered: PublishedCombatBehavior | PublishedRunBehavior,
    policy: FrozenCategoricalTorchPolicy | FrozenCombatGreedyTorchPolicy,
    *,
    run_bridge: CategoricalSessionBridge,
) -> dict[str, object]:
    """Evaluate one already verified immutable policy over complete runs."""

    if not isinstance(config, RunEvaluationCommandConfig):
        raise RunEvaluationCommandError("run evaluation config must be typed")
    if not isinstance(recovered, (PublishedCombatBehavior, PublishedRunBehavior)):
        raise RunEvaluationCommandError(
            "run evaluation recovered behavior must be typed"
        )
    if not isinstance(
        policy,
        (FrozenCategoricalTorchPolicy, FrozenCombatGreedyTorchPolicy),
    ):
        raise RunEvaluationCommandError("run evaluation policy must be frozen")
    if not isinstance(run_bridge, CategoricalSessionBridge):
        raise RunEvaluationCommandError(
            "run evaluation environment bridge must be typed"
        )
    if (
        isinstance(policy, FrozenCategoricalTorchPolicy)
        and policy.behavior_manifest_id != recovered.manifest_id
    ):
        raise RunEvaluationCommandError(
            "run evaluation sampled policy changed source behavior"
        )
    if (
        isinstance(policy, FrozenCombatGreedyTorchPolicy)
        and policy.binding.manifest_id != recovered.manifest_id
        and policy.source_manifest_id != recovered.manifest_id
    ):
        raise RunEvaluationCommandError(
            "run evaluation scoped policy changed strategic source behavior"
        )

    potion_lane = resolve_run_potion_lane(config.potion_lane, recovered)
    schedule = SeedSchedule(
        SeedPartition.HELD_OUT,
        next_candidate=config.held_out_seed_start,
    )
    environment_constructor = (
        run_bridge.environment
        if potion_lane is CombatPotionLane.ALL
        else run_bridge.environment_without_combat_potions
    )
    resource_factory = ResourceTracingEnvironmentFactory(
        lambda seeds: environment_constructor(seeds, config.ascension_level)
    )
    result = evaluate_held_out_behavior(
        resource_factory,
        policy,
        schedule=schedule,
        spec=HeldOutEvaluationSpec(
            slot_count=config.slot_count,
            terminal_attempt_target=config.terminal_attempts,
            max_batch_steps=config.max_batch_steps,
        ),
    )
    resource_trace = resource_factory.trace
    summary = _summary(
        config,
        recovered,
        result,
        resource_trace,
        potion_lane,
        policy,
    )
    config.output.mkdir(parents=True, exist_ok=True)
    with (config.output / "evaluation.json").open(
        "x",
        encoding="utf-8",
        newline="\n",
    ) as destination:
        json.dump(summary, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    run = result.run.summary
    print(
        "run_evaluation_complete=true "
        f"ascension={config.ascension_level} "
        f"potion_lane={potion_lane.value} "
        f"potion_lane_request={config.potion_lane.value} "
        f"target_reached={str(result.complete).lower()} "
        f"attempts={run.terminal_attempts}/{config.terminal_attempts} "
        f"victories={run.victories} defeats={run.defeats} "
        f"floor_sum={run.terminal_progress.floor_sum} "
        f"floor_min={_optional(run.terminal_progress.min_floor)} "
        f"floor_max={_optional(run.terminal_progress.max_floor)} "
        f"floor_counts={_counts(run.terminal_progress.floor_counts)} "
        f"act_counts={_counts(run.terminal_progress.act_counts)} "
        f"combats={len(resource_trace.combat_transitions)} "
        f"combat_hp_loss={resource_trace.hp_loss_sum} "
        f"open_combats={resource_trace.open_combat_count} "
        f"batch_steps={run.batch_steps}/{config.max_batch_steps} "
        f"slot_steps={run.slot_steps} seconds={run.elapsed_seconds:.3f} "
        f"output={config.output}",
        flush=True,
    )
    for seed in resource_trace.seed_summaries:
        print(
            "run_resource_seed="
            f"{seed.seed} combats={seed.combat_count} "
            f"hp_loss={seed.hp_loss_sum} "
            f"last=A{seed.last_act}F{seed.last_floor} "
            f"hp={seed.last_hp}/{seed.last_max_hp} gold={seed.last_gold} "
            f"terminal={_optional(seed.terminal_reward)} "
            f"open_combat={str(seed.open_combat).lower()} "
            f"potions={_potions(seed.last_potion_ids)} "
            f"lost={_identity_counts(seed.potion_identity_losses)} "
            f"gained={_identity_counts(seed.potion_identity_gains)}",
            flush=True,
        )
    return summary


def _summary(
    config: RunEvaluationCommandConfig,
    recovered: PublishedCombatBehavior | PublishedRunBehavior,
    result: HeldOutEvaluationResult,
    resource_trace: RunResourceTrace,
    potion_lane: CombatPotionLane,
    policy: FrozenCategoricalTorchPolicy | FrozenCombatGreedyTorchPolicy,
) -> dict[str, object]:
    run = result.run.summary
    progress = run.terminal_progress
    combat_trained = isinstance(recovered, PublishedCombatBehavior)
    execution_manifest = policy.binding.manifest
    execution_anchor: FrozenCombatAnchor | None = (
        policy.combat_anchor
        if isinstance(policy, FrozenCombatGreedyTorchPolicy)
        else None
    )
    execution_scope = (
        "combat_anchor_greedy_strategic_source_sampled"
        if execution_anchor is not None
        else (
            "combat_greedy_strategic_source_sampled"
            if isinstance(policy, FrozenCombatGreedyTorchPolicy)
            else "publication_sampled"
        )
    )
    return {
        "schema": RUN_EVALUATION_SCHEMA,
        "kind": "completed" if result.complete else "step-limit",
        "behavior": str(config.behavior),
        "behavior_manifest_id": recovered.manifest_id.digest.hex(),
        "execution_behavior_manifest_id": (
            result.behavior_manifest_id.digest.hex()
        ),
        "behavior_checkpoint_id": recovered.checkpoint_id.digest.hex(),
        "execution_model_definition_id": (
            execution_manifest.model_definition.digest.hex()
        ),
        "execution_model_config_id": execution_manifest.model_config.digest.hex(),
        "execution_behavior_rule_implementation_id": (
            execution_manifest.behavior_rule.implementation.digest.hex()
        ),
        "execution_behavior_rule_configuration_id": (
            execution_manifest.behavior_rule.configuration.digest.hex()
        ),
        "execution_semantic_schema_id": (
            execution_manifest.semantic_schema.digest.hex()
        ),
        "execution_semantic_schema_version": (
            execution_manifest.semantic_schema_version
        ),
        "execution_scope": execution_scope,
        "execution_strategic_source_manifest_id": (
            policy.source_manifest_id.digest.hex()
            if isinstance(policy, FrozenCombatGreedyTorchPolicy)
            else policy.behavior_manifest_id.digest.hex()
        ),
        "execution_combat_anchor_manifest_id": (
            None
            if execution_anchor is None
            else execution_anchor.manifest_id.digest.hex()
        ),
        "execution_combat_anchor_checkpoint_id": (
            None
            if execution_anchor is None
            else execution_anchor.checkpoint_id.digest.hex()
        ),
        "execution_combat_anchor_model_definition_id": (
            None
            if execution_anchor is None
            else execution_anchor.binding.manifest.model_definition.digest.hex()
        ),
        "execution_combat_anchor_model_config_id": (
            None
            if execution_anchor is None
            else execution_anchor.binding.manifest.model_config.digest.hex()
        ),
        "execution_combat_anchor_semantic_schema_id": (
            None
            if execution_anchor is None
            else execution_anchor.binding.manifest.semantic_schema.digest.hex()
        ),
        "execution_combat_anchor_semantic_schema_version": (
            None
            if execution_anchor is None
            else execution_anchor.binding.manifest.semantic_schema_version
        ),
        "behavior_training_step": recovered.training_step,
        "behavior_training_kind": "combat" if combat_trained else "run",
        "behavior_training_ascension_level": (
            None if combat_trained else recovered.training_ascension_level
        ),
        "behavior_training_root_count": (
            recovered.training_root_count if combat_trained else None
        ),
        "behavior_training_artifact_sha256": (
            recovered.training_artifact_sha256 if combat_trained else None
        ),
        "behavior_training_all_loss_axis": (
            recovered.training_all_loss_axis.name.lower()
            if combat_trained
            else None
        ),
        "behavior_training_potion_lane": recovered.training_potion_lane.value,
        "behavior_training_potion_slots": (
            recovered.training_potion_slots if combat_trained else ()
        ),
        "behavior_run_sampling_mode": (
            None
            if combat_trained
            else recovered.training_sampling_mode.value
        ),
        "behavior_run_episode_root_attempts": (
            None
            if combat_trained
            else recovered.training_episode_root_attempts
        ),
        "behavior_run_combat_decision_rule": (
            None
            if combat_trained
            else recovered.training_combat_decision_rule.value
        ),
        "behavior_run_combat_anchor_manifest_id": (
            None
            if combat_trained or recovered.combat_anchor_manifest_id is None
            else recovered.combat_anchor_manifest_id.digest.hex()
        ),
        "behavior_run_combat_anchor_checkpoint_id": (
            None
            if combat_trained or recovered.combat_anchor_checkpoint_id is None
            else recovered.combat_anchor_checkpoint_id.digest.hex()
        ),
        "behavior_run_combat_anchor_scorer": (
            None
            if combat_trained or recovered.combat_anchor_scorer is None
            else {
                "hidden_dim": recovered.combat_anchor_scorer.hidden_dim,
                "relation_layers": (
                    recovered.combat_anchor_scorer.relation_layers
                ),
                "value_head": recovered.combat_anchor_scorer.value_head,
                "value_head_width": (
                    recovered.combat_anchor_scorer.value_head_width
                ),
            }
        ),
        "behavior_run_critic_initialization_manifest_id": (
            None
            if combat_trained
            or recovered.critic_initialization_manifest_id is None
            else recovered.critic_initialization_manifest_id.digest.hex()
        ),
        "behavior_run_critic_initialization_checkpoint_id": (
            None
            if combat_trained
            or recovered.critic_initialization_checkpoint_id is None
            else recovered.critic_initialization_checkpoint_id.digest.hex()
        ),
        "behavior_run_critic_initialization_training_step": (
            None
            if combat_trained
            else recovered.critic_initialization_training_step
        ),
        "behavior_run_critic_initialization_policy_update": (
            None
            if combat_trained
            or recovered.critic_initialization_policy_update is None
            else recovered.critic_initialization_policy_update.name.lower()
        ),
        "behavior_run_objective": (
            None
            if combat_trained
            else {
                "attempts_per_update": recovered.objective.attempts_per_update,
                "advantage_mode": recovered.objective.advantage_mode.name.lower(),
                "decision_scope": recovered.objective.decision_scope.name.lower(),
                "policy_update": (
                    recovered.objective.policy_update.rule.name.lower()
                ),
                "normalize_advantage": (
                    recovered.objective.policy_update.normalize_advantage
                ),
                "value_clip_coefficient": (
                    recovered.objective.policy_update.value_clip_coefficient
                ),
            }
        ),
        "behavior_seed": config.behavior_seed,
        "ascension_level": config.ascension_level,
        "held_out_seed_start": config.held_out_seed_start,
        "seed_partition_held_out_numerator": (
            result.schedule_start.spec.held_out_numerator
        ),
        "seed_partition_denominator": result.schedule_start.spec.denominator,
        "requested_combat_potion_lane": config.potion_lane.value,
        "combat_potion_lane": potion_lane.value,
        "held_out_seed_end": result.schedule_end.next_candidate,
        "slot_count": config.slot_count,
        "terminal_attempt_target": config.terminal_attempts,
        "max_batch_steps": config.max_batch_steps,
        "target_reached": result.complete,
        "step_limit_reached": result.step_limit_reached,
        "terminal_attempts": run.terminal_attempts,
        "victories": run.victories,
        "defeats": run.defeats,
        "terminal_floor_sum": progress.floor_sum,
        "min_terminal_floor": progress.min_floor,
        "max_terminal_floor": progress.max_floor,
        "terminal_floor_counts": progress.floor_counts,
        "terminal_act_counts": progress.act_counts,
        "combat_transitions": tuple(
            _combat_transition(transition)
            for transition in resource_trace.combat_transitions
        ),
        "combat_transition_count": len(resource_trace.combat_transitions),
        "combat_hp_loss_sum": resource_trace.hp_loss_sum,
        "combat_potion_identity_losses": resource_trace.potion_identity_losses,
        "combat_potion_identity_gains": resource_trace.potion_identity_gains,
        "open_combat_count": resource_trace.open_combat_count,
        "combat_seed_summaries": tuple(
            _seed_resource_summary(seed)
            for seed in resource_trace.seed_summaries
        ),
        "batch_steps": run.batch_steps,
        "slot_steps": run.slot_steps,
        "decision_rounds": run.decision_rounds,
        "completed_episodes": run.completed_episodes,
        "recoveries": run.recoveries,
        "elapsed_seconds": run.elapsed_seconds,
    }


def _combat_transition(
    transition: RunCombatResourceTransition,
) -> dict[str, object]:
    return {
        "slot_index": transition.start.slot_index,
        "seed": transition.start.seed,
        "encounter_id": transition.start.encounter_id,
        "monster_ids": transition.start.monster_ids,
        "start_act": transition.start.act,
        "start_floor": transition.start.floor,
        "end_act": transition.end.act,
        "end_floor": transition.end.floor,
        "start_hp": transition.start.hp,
        "end_hp": transition.end.hp,
        "hp_loss": transition.hp_loss,
        "start_max_hp": transition.start.max_hp,
        "end_max_hp": transition.end.max_hp,
        "start_gold": transition.start.gold,
        "end_gold": transition.end.gold,
        "start_potion_ids": transition.start.potion_ids,
        "end_potion_ids": transition.end.potion_ids,
        "end_boundary_kind": transition.end.boundary_kind,
        "terminal_reward": transition.terminal_reward,
    }


def _seed_resource_summary(seed: RunSeedResourceSummary) -> dict[str, object]:
    return {
        "seed": seed.seed,
        "slot_index": seed.slot_index,
        "combat_count": seed.combat_count,
        "hp_loss_sum": seed.hp_loss_sum,
        "last_act": seed.last_act,
        "last_floor": seed.last_floor,
        "last_hp": seed.last_hp,
        "last_max_hp": seed.last_max_hp,
        "last_gold": seed.last_gold,
        "last_potion_ids": seed.last_potion_ids,
        "terminal_reward": seed.terminal_reward,
        "open_combat": seed.open_combat,
        "potion_identity_losses": seed.potion_identity_losses,
        "potion_identity_gains": seed.potion_identity_gains,
    }


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise RunEvaluationCommandError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise RunEvaluationCommandError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise RunEvaluationCommandError(f"{name} must be positive")
    return normalized


def resolve_run_potion_lane(
    requested: RunPotionLane,
    recovered: PublishedCombatBehavior | PublishedRunBehavior,
) -> CombatPotionLane:
    if requested is RunPotionLane.TRAINED:
        lane = recovered.training_potion_lane
        if lane is CombatPotionLane.ROOT_SLOTS:
            raise RunEvaluationCommandError(
                "a root-slots-trained behavior requires an explicit whole-run "
                "all or never potion lane"
            )
        return lane
    return CombatPotionLane(requested.value)


def _seed(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise RunEvaluationCommandError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise RunEvaluationCommandError(f"{name} must be an integer") from error
    if not 0 <= normalized < 1 << 63:
        raise RunEvaluationCommandError(f"{name} must be in [0, 2^63)")
    return normalized


def _optional(value: int | None) -> str:
    return "none" if value is None else str(value)


def _counts(values: tuple[tuple[int, int], ...]) -> str:
    return ",".join(f"{key}:{count}" for key, count in values) or "none"


def _identity_counts(values: tuple[tuple[str, int], ...]) -> str:
    return ",".join(f"{key}:{count}" for key, count in values) or "none"


def _potions(values: tuple[str | None, ...]) -> str:
    return ",".join(value or "empty" for value in values) or "none"


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Evaluate one published frozen scorer over complete "
            "held-out runs without recovery."
        ),
    )
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--slots", type=int, default=1)
    parser.add_argument("--attempts", type=int, default=8)
    parser.add_argument("--max-batch-steps", type=int, default=4096)
    parser.add_argument("--behavior-seed", type=int, default=10_000)
    parser.add_argument(
        "--ascension",
        type=int,
        choices=range(21),
        required=True,
    )
    parser.add_argument("--held-out-seed-start", type=int, default=0)
    parser.add_argument(
        "--potion-lane",
        choices=tuple(lane.value for lane in RunPotionLane),
        default=RunPotionLane.TRAINED.value,
    )
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    run_run_evaluation(
        RunEvaluationCommandConfig(
            behavior=arguments.behavior,
            output=arguments.output,
            slot_count=arguments.slots,
            terminal_attempts=arguments.attempts,
            max_batch_steps=arguments.max_batch_steps,
            behavior_seed=arguments.behavior_seed,
            ascension_level=arguments.ascension,
            held_out_seed_start=arguments.held_out_seed_start,
            potion_lane=RunPotionLane(arguments.potion_lane),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
