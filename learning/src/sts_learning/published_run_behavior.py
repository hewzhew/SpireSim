"""Strict recovery of one completed whole-run training publication."""

from __future__ import annotations

import json
import math
import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, replace
from numbers import Real
from pathlib import Path

from .combat_potion_lane import CombatPotionLane
from .manifests import BehaviorManifestId, ManifestArtifactId, ManifestArtifactKind
from .run_sampling import RunSamplingMode
from .seeds import SeedPartition, SeedSchedule
from .terminal_returns import (
    OnPolicyObjectiveConfig,
    RunDecisionScope,
    RunPolicyUpdateConfig,
    RunPolicyUpdateRule,
    TerminalAdvantageMode,
)
from .torch_behavior import (
    CheckpointedCategoricalTorchPolicy,
    FrozenCombatGreedyTorchPolicy,
    FrozenDecisionRule,
)
from .torch_session import CategoricalOnlineSessionFactory, NoRecoveryCurriculum
from .torch_session_config import (
    CategoricalOnlineProfile,
    CategoricalOnlineSessionConfig,
    CategoricalSessionBridge,
    CategoricalSessionLimits,
    TorchSessionError,
    normalize_torch_device_type,
)
from .torch_policy import RaggedScorerConfig


RUN_TRAINING_SCHEMA = "sts-learning-run-training-v6"
_LEGACY_RUN_TRAINING_SCHEMAS = frozenset(
    {
        "sts-learning-run-training-v3",
        "sts-learning-run-training-v4",
        "sts-learning-run-training-v5",
    }
)
_MAX_TRAINING_JOURNAL_BYTES = 16 * 1024 * 1024


class PublishedRunBehaviorError(RuntimeError):
    """A durable whole-run behavior is incomplete or has conflicting provenance."""


@dataclass(frozen=True)
class PublishedRunBehavior:
    """One verified run-trained policy fork per requested RNG stream."""

    manifest_id: BehaviorManifestId
    checkpoint_id: ManifestArtifactId
    training_step: int
    training_ascension_level: int
    training_potion_lane: CombatPotionLane
    training_sampling_mode: RunSamplingMode
    training_episode_root_attempts: int | None
    training_device_type: str
    training_combat_decision_rule: FrozenDecisionRule
    combat_anchor_manifest_id: BehaviorManifestId | None
    combat_anchor_checkpoint_id: ManifestArtifactId | None
    combat_anchor_scorer: RaggedScorerConfig | None
    objective: OnPolicyObjectiveConfig
    critic_initialization_manifest_id: BehaviorManifestId | None
    critic_initialization_checkpoint_id: ManifestArtifactId | None
    critic_initialization_training_step: int | None
    critic_initialization_policy_update: RunPolicyUpdateRule | None
    policies: tuple[
        CheckpointedCategoricalTorchPolicy | FrozenCombatGreedyTorchPolicy,
        ...,
    ]

    def __post_init__(self) -> None:
        if not isinstance(self.manifest_id, BehaviorManifestId):
            raise PublishedRunBehaviorError("run behavior manifest id must be typed")
        if not isinstance(self.checkpoint_id, ManifestArtifactId):
            raise PublishedRunBehaviorError("run behavior checkpoint id must be typed")
        if self.checkpoint_id.kind is not ManifestArtifactKind.MODEL_CHECKPOINT:
            raise PublishedRunBehaviorError("run behavior checkpoint has the wrong kind")
        _nonnegative(self.training_step, "training_step")
        _ascension(self.training_ascension_level, "training_ascension_level")
        if not isinstance(self.training_potion_lane, CombatPotionLane):
            raise PublishedRunBehaviorError("run behavior potion lane must be typed")
        if self.training_potion_lane is CombatPotionLane.ROOT_SLOTS:
            raise PublishedRunBehaviorError("whole-run training cannot publish root slots")
        if not isinstance(self.training_sampling_mode, RunSamplingMode):
            raise PublishedRunBehaviorError("run behavior sampling mode must be typed")
        try:
            normalize_torch_device_type(self.training_device_type)
        except TorchSessionError as error:
            raise PublishedRunBehaviorError(str(error)) from error
        if not isinstance(
            self.training_combat_decision_rule,
            FrozenDecisionRule,
        ):
            raise PublishedRunBehaviorError(
                "run behavior combat decision rule must be typed"
            )
        anchor_values = (
            self.combat_anchor_manifest_id,
            self.combat_anchor_checkpoint_id,
            self.combat_anchor_scorer,
        )
        if self.training_combat_decision_rule is FrozenDecisionRule.GREEDY:
            if any(value is None for value in anchor_values):
                raise PublishedRunBehaviorError(
                    "greedy run behavior requires a complete combat anchor"
                )
        elif any(value is not None for value in anchor_values):
            raise PublishedRunBehaviorError(
                "sampled run behavior cannot publish a combat anchor"
            )
        if (
            self.combat_anchor_manifest_id is not None
            and not isinstance(
                self.combat_anchor_manifest_id,
                BehaviorManifestId,
            )
        ):
            raise PublishedRunBehaviorError(
                "run combat anchor manifest id must be typed"
            )
        if self.combat_anchor_checkpoint_id is not None:
            if not isinstance(
                self.combat_anchor_checkpoint_id,
                ManifestArtifactId,
            ):
                raise PublishedRunBehaviorError(
                    "run combat anchor checkpoint id must be typed"
                )
            if (
                self.combat_anchor_checkpoint_id.kind
                is not ManifestArtifactKind.MODEL_CHECKPOINT
            ):
                raise PublishedRunBehaviorError(
                    "run combat anchor checkpoint has the wrong kind"
                )
        if (
            self.combat_anchor_scorer is not None
            and not isinstance(self.combat_anchor_scorer, RaggedScorerConfig)
        ):
            raise PublishedRunBehaviorError(
                "run combat anchor scorer must be typed"
            )
        if not isinstance(self.objective, OnPolicyObjectiveConfig):
            raise PublishedRunBehaviorError("run behavior objective must be typed")
        critic_initialization = (
            self.critic_initialization_manifest_id,
            self.critic_initialization_checkpoint_id,
            self.critic_initialization_training_step,
            self.critic_initialization_policy_update,
        )
        if any(value is None for value in critic_initialization) and not all(
            value is None for value in critic_initialization
        ):
            raise PublishedRunBehaviorError(
                "run critic initialization provenance is incomplete"
            )
        if self.critic_initialization_manifest_id is not None:
            if not isinstance(
                self.critic_initialization_manifest_id,
                BehaviorManifestId,
            ):
                raise PublishedRunBehaviorError(
                    "run critic initialization manifest id must be typed"
                )
            checkpoint = self.critic_initialization_checkpoint_id
            if (
                not isinstance(checkpoint, ManifestArtifactId)
                or checkpoint.kind is not ManifestArtifactKind.MODEL_CHECKPOINT
            ):
                raise PublishedRunBehaviorError(
                    "run critic initialization checkpoint id must be typed"
                )
            _nonnegative(
                self.critic_initialization_training_step,
                "critic_initialization_training_step",
            )
            if (
                self.critic_initialization_policy_update
                is not RunPolicyUpdateRule.CRITIC_CALIBRATION
            ):
                raise PublishedRunBehaviorError(
                    "run critic initialization must name critic calibration"
                )
            if (
                self.objective.policy_update.rule
                is not RunPolicyUpdateRule.PPO_CLIP_VALUE
            ):
                raise PublishedRunBehaviorError(
                    "only run PPO can consume critic initialization"
                )
        expected_attempts = _episode_root_attempts(
            self.training_episode_root_attempts,
            self.training_sampling_mode,
        )
        if expected_attempts != self.training_episode_root_attempts:
            raise AssertionError("normalized episode-root attempt count changed")
        if (
            expected_attempts is not None
            and expected_attempts > self.objective.attempts_per_update
        ):
            raise PublishedRunBehaviorError(
                "episode-root attempts exceed the training update"
            )
        if not self.policies or not all(
            isinstance(
                policy,
                (
                    CheckpointedCategoricalTorchPolicy,
                    FrozenCombatGreedyTorchPolicy,
                ),
            )
            for policy in self.policies
        ):
            raise PublishedRunBehaviorError("run behavior policies must be checkpointed")
        if any(
            policy.behavior_manifest_id != self.manifest_id
            for policy in self.policies
        ):
            raise PublishedRunBehaviorError("run behavior policy identity changed")
        if self.combat_anchor_manifest_id is not None and any(
            not isinstance(policy, FrozenCombatGreedyTorchPolicy)
            or policy.combat_anchor is None
            or policy.combat_anchor.manifest_id
            != self.combat_anchor_manifest_id
            or policy.combat_anchor.checkpoint_id
            != self.combat_anchor_checkpoint_id
            or policy.combat_anchor.scorer.config
            != self.combat_anchor_scorer
            for policy in self.policies
        ):
            raise PublishedRunBehaviorError(
                "run behavior policy combat anchor changed"
            )


def recover_published_run_behavior(
    behavior_root: str | Path,
    bridge: CategoricalSessionBridge,
    behavior_seeds: Sequence[int],
) -> PublishedRunBehavior:
    """Verify run-training boundaries and materialize immutable policies."""

    root = Path(behavior_root).resolve()
    if not root.is_dir():
        raise PublishedRunBehaviorError("published run behavior is not a directory")
    if not isinstance(bridge, CategoricalSessionBridge):
        raise PublishedRunBehaviorError("run behavior recovery requires a typed bridge")
    seeds = tuple(
        _seed(seed, f"behavior_seeds[{index}]")
        for index, seed in enumerate(behavior_seeds)
    )
    if not seeds or len(set(seeds)) != len(seeds):
        raise PublishedRunBehaviorError("run behavior requires distinct RNG seeds")

    configuration, completed = _training_boundary_records(root / "training.jsonl")
    policy_update = _run_policy_update(configuration)
    objective = OnPolicyObjectiveConfig(
        attempts_per_update=_positive(
            configuration.get("attempts_per_update"),
            "attempts_per_update",
        ),
        advantage_mode=_enum_name(
            TerminalAdvantageMode,
            configuration.get("advantage_mode"),
            "advantage_mode",
        ),
        decision_scope=_enum_name(
            RunDecisionScope,
            configuration.get("decision_scope", "all"),
            "decision_scope",
        ),
        policy_update=policy_update,
    )
    completed_update = completed.get("run_policy_update", "reinforce")
    if completed_update != policy_update.rule.name.lower():
        raise PublishedRunBehaviorError(
            "run policy update changed across publication"
        )
    completed_normalization = completed.get(
        "run_policy_normalize_advantage",
        False,
    )
    if completed_normalization != policy_update.normalize_advantage:
        raise PublishedRunBehaviorError(
            "run advantage normalization changed across publication"
        )
    if completed.get("run_policy_value_prediction_target", "return_to_go") != policy_update.value_prediction_target:
        raise PublishedRunBehaviorError("run value target changed across publication")
    training_step = _nonnegative(
        completed.get("optimizer_steps"),
        "optimizer_steps",
    )
    manifest_id = BehaviorManifestId(
        _digest(
            completed.get("active_behavior_manifest_id"),
            "active_behavior_manifest_id",
        )
    )
    checkpoint_id = ManifestArtifactId(
        ManifestArtifactKind.MODEL_CHECKPOINT,
        _digest(
            completed.get("active_behavior_checkpoint_id"),
            "active_behavior_checkpoint_id",
        ),
    )
    potion_lane = _potion_lane(completed.get("run_potion_lane"))
    if _potion_lane(configuration.get("run_potion_lane")) is not potion_lane:
        raise PublishedRunBehaviorError("run potion lane changed across publication")
    training_ascension_level = _ascension(
        configuration.get("ascension_level"),
        "ascension_level",
    )
    if (
        _ascension(completed.get("ascension_level"), "completed ascension_level")
        != training_ascension_level
    ):
        raise PublishedRunBehaviorError(
            "run ascension level changed across publication"
        )
    try:
        training_device_type = normalize_torch_device_type(
            configuration.get("device_type", "cpu")
        )
        completed_device_type = normalize_torch_device_type(
            completed.get("device_type", "cpu")
        )
    except TorchSessionError as error:
        raise PublishedRunBehaviorError(str(error)) from error
    if completed_device_type != training_device_type:
        raise PublishedRunBehaviorError(
            "run device type changed across publication"
        )
    configuration_sampling_mode = _sampling_mode(
        configuration.get(
            "sampling_mode",
            RunSamplingMode.INDEPENDENT_COHORTS.value,
        )
    )
    completed_sampling_mode = _sampling_mode(
        completed.get(
            "sampling_mode",
            RunSamplingMode.INDEPENDENT_COHORTS.value,
        )
    )
    if completed_sampling_mode is not configuration_sampling_mode:
        raise PublishedRunBehaviorError(
            "run sampling mode changed across publication"
        )
    configuration_episode_root_attempts = _episode_root_attempts(
        configuration.get("episode_root_attempts"),
        configuration_sampling_mode,
    )
    completed_episode_root_attempts = _episode_root_attempts(
        completed.get("episode_root_attempts"),
        completed_sampling_mode,
    )
    if completed_episode_root_attempts != configuration_episode_root_attempts:
        raise PublishedRunBehaviorError(
            "episode-root attempt cap changed across publication"
        )
    combat_decision_rule = _combat_decision_rule(
        configuration.get("combat_decision_rule")
    )
    if (
        _combat_decision_rule(completed.get("combat_decision_rule"))
        is not combat_decision_rule
    ):
        raise PublishedRunBehaviorError(
            "combat decision rule changed across publication"
        )
    combat_anchor = _combat_anchor(
        configuration,
        combat_decision_rule,
        "configuration",
    )
    if (
        _combat_anchor(completed, combat_decision_rule, "completed")
        != combat_anchor
    ):
        raise PublishedRunBehaviorError(
            "combat anchor changed across publication"
        )
    critic_initialization = _critic_initialization(
        configuration,
        "configuration",
    )
    if (
        _critic_initialization(completed, "completed")
        != critic_initialization
    ):
        raise PublishedRunBehaviorError(
            "critic initialization changed across publication"
        )
    (
        combat_anchor_manifest_id,
        combat_anchor_checkpoint_id,
        combat_anchor_scorer,
    ) = combat_anchor
    if combat_anchor_manifest_id is not None:
        if combat_anchor_manifest_id.digest != _digest(
            configuration.get("warm_start_manifest_id"),
            "warm_start_manifest_id",
        ):
            raise PublishedRunBehaviorError(
                "run combat anchor is not the verified warm-start manifest"
            )
        if combat_anchor_checkpoint_id is None:
            raise AssertionError("complete combat anchor lost its checkpoint")
        if combat_anchor_checkpoint_id.digest != _digest(
            configuration.get("warm_start_checkpoint_id"),
            "warm_start_checkpoint_id",
        ):
            raise PublishedRunBehaviorError(
                "run combat anchor is not the verified warm-start checkpoint"
            )

    profile = replace(
        CategoricalOnlineProfile(),
        scorer=RaggedScorerConfig(
            value_head=policy_update.uses_value_baseline,
        ),
        objective=objective,
        combat_decision_rule=combat_decision_rule,
        combat_anchor_manifest_id=combat_anchor_manifest_id,
        combat_anchor_scorer=combat_anchor_scorer,
        device_type=training_device_type,
    )
    limits = replace(
        CategoricalSessionLimits(),
        owner_capacity=max(
            16,
            _nonnegative(configuration.get("generations"), "generations")
            + 2,
        ),
    )
    session_config = CategoricalOnlineSessionConfig(
        ascension_level=training_ascension_level,
        schedule=SeedSchedule(
            SeedPartition.TRAINING,
            next_candidate=_seed(
                configuration.get("training_seed_start"),
                "training_seed_start",
            ),
        ),
        slot_count=_positive(configuration.get("slot_count"), "slot_count"),
        max_recoveries_per_episode=0,
        profile=profile,
        limits=limits,
    )
    factory = CategoricalOnlineSessionFactory(
        root,
        bridge,
        session_config,
        NoRecoveryCurriculum(),
    )
    policies = tuple(
        factory.recover_behavior(manifest_id, behavior_seed=seed)
        for seed in seeds
    )
    manifest = policies[0].binding.manifest
    if manifest.model_checkpoint != checkpoint_id:
        raise PublishedRunBehaviorError(
            "run journal and durable checkpoint identity disagree"
        )
    if manifest.training_step != training_step:
        raise PublishedRunBehaviorError(
            "run journal and durable manifest training step disagree"
        )
    return PublishedRunBehavior(
        manifest_id=manifest_id,
        checkpoint_id=checkpoint_id,
        training_step=training_step,
        training_ascension_level=training_ascension_level,
        training_potion_lane=potion_lane,
        training_sampling_mode=configuration_sampling_mode,
        training_episode_root_attempts=(
            configuration_episode_root_attempts
        ),
        training_device_type=training_device_type,
        training_combat_decision_rule=combat_decision_rule,
        combat_anchor_manifest_id=combat_anchor_manifest_id,
        combat_anchor_checkpoint_id=combat_anchor_checkpoint_id,
        combat_anchor_scorer=combat_anchor_scorer,
        objective=objective,
        critic_initialization_manifest_id=critic_initialization[0],
        critic_initialization_checkpoint_id=critic_initialization[1],
        critic_initialization_training_step=critic_initialization[2],
        critic_initialization_policy_update=critic_initialization[3],
        policies=policies,
    )


def is_run_training_publication(behavior_root: str | Path) -> bool:
    """Classify only the first bounded journal record, without fallback guessing."""

    root = Path(behavior_root).resolve()
    journal = root / "training.jsonl"
    if not journal.is_file() or journal.stat().st_size > _MAX_TRAINING_JOURNAL_BYTES:
        return False
    try:
        with journal.open("r", encoding="utf-8") as source:
            for line in source:
                if line.strip():
                    value = json.loads(line)
                    return (
                        isinstance(value, dict)
                        and value.get("schema")
                        in {RUN_TRAINING_SCHEMA, *_LEGACY_RUN_TRAINING_SCHEMAS}
                        and value.get("kind") == "configuration"
                    )
    except (OSError, json.JSONDecodeError):
        return False
    return False


def _training_boundary_records(
    journal: Path,
) -> tuple[dict[str, object], dict[str, object]]:
    try:
        size = journal.stat().st_size
    except OSError as error:
        raise PublishedRunBehaviorError("run training journal is unavailable") from error
    if size <= 0 or size > _MAX_TRAINING_JOURNAL_BYTES:
        raise PublishedRunBehaviorError("run training journal size is invalid")
    first: tuple[int, str] | None = None
    last: tuple[int, str] | None = None
    try:
        with journal.open("r", encoding="utf-8") as source:
            for line_number, line in enumerate(source, start=1):
                if not line.strip():
                    continue
                if first is None:
                    first = (line_number, line)
                last = (line_number, line)
    except OSError as error:
        raise PublishedRunBehaviorError("run training journal could not be read") from error
    if first is None or last is None:
        raise PublishedRunBehaviorError("run training journal is empty")
    configuration = _journal_record(*first)
    completed = _journal_record(*last)
    if (
        configuration.get("schema") in _LEGACY_RUN_TRAINING_SCHEMAS
        or completed.get("schema") in _LEGACY_RUN_TRAINING_SCHEMAS
    ):
        raise PublishedRunBehaviorError(
            "run training publication predates the current explicit run provenance"
        )
    if (
        configuration.get("schema") != RUN_TRAINING_SCHEMA
        or configuration.get("kind") != "configuration"
        or completed.get("schema") != RUN_TRAINING_SCHEMA
        or completed.get("kind") != "completed"
    ):
        raise PublishedRunBehaviorError(
            "run training journal lacks exact configuration/completion boundaries"
        )
    return configuration, completed


def _journal_record(line_number: int, line: str) -> dict[str, object]:
    try:
        value = json.loads(line)
    except json.JSONDecodeError as error:
        raise PublishedRunBehaviorError(
            f"run training journal line {line_number} is invalid JSON"
        ) from error
    if not isinstance(value, dict):
        raise PublishedRunBehaviorError(
            f"run training journal line {line_number} is not an object"
        )
    return value


def _enum_name(enum_type, value: object, name: str):
    if not isinstance(value, str):
        raise PublishedRunBehaviorError(f"{name} must be text")
    try:
        return enum_type[value.upper()]
    except KeyError as error:
        raise PublishedRunBehaviorError(f"{name} is unsupported") from error


def _potion_lane(value: object) -> CombatPotionLane:
    try:
        lane = CombatPotionLane(value)
    except (TypeError, ValueError) as error:
        raise PublishedRunBehaviorError("run_potion_lane is unsupported") from error
    if lane is CombatPotionLane.ROOT_SLOTS:
        raise PublishedRunBehaviorError("whole-run publication cannot use root slots")
    return lane


def _sampling_mode(value: object) -> RunSamplingMode:
    try:
        return RunSamplingMode(value)
    except (TypeError, ValueError) as error:
        raise PublishedRunBehaviorError(
            "run sampling mode is unsupported"
        ) from error


def _combat_decision_rule(value: object) -> FrozenDecisionRule:
    try:
        return FrozenDecisionRule(value)
    except (TypeError, ValueError) as error:
        raise PublishedRunBehaviorError(
            "combat decision rule is unsupported"
        ) from error


def _combat_anchor(
    record: Mapping[str, object],
    decision_rule: FrozenDecisionRule,
    name: str,
) -> tuple[
    BehaviorManifestId | None,
    ManifestArtifactId | None,
    RaggedScorerConfig | None,
]:
    raw_manifest = record.get("combat_anchor_manifest_id")
    raw_checkpoint = record.get("combat_anchor_checkpoint_id")
    raw_scorer = record.get("combat_anchor_scorer")
    raw_values = (raw_manifest, raw_checkpoint, raw_scorer)
    if decision_rule is FrozenDecisionRule.SAMPLED:
        if any(value is not None for value in raw_values):
            raise PublishedRunBehaviorError(
                f"{name} sampled behavior cannot contain a combat anchor"
            )
        return None, None, None
    if any(value is None for value in raw_values):
        raise PublishedRunBehaviorError(
            f"{name} greedy behavior requires a complete combat anchor"
        )
    return (
        BehaviorManifestId(
            _digest(raw_manifest, f"{name} combat_anchor_manifest_id")
        ),
        ManifestArtifactId(
            ManifestArtifactKind.MODEL_CHECKPOINT,
            _digest(raw_checkpoint, f"{name} combat_anchor_checkpoint_id"),
        ),
        _scorer_config(raw_scorer, f"{name} combat_anchor_scorer"),
    )


def _scorer_config(value: object, name: str) -> RaggedScorerConfig:
    if not isinstance(value, Mapping):
        raise PublishedRunBehaviorError(f"{name} must be an object")
    expected = {"hidden_dim", "relation_layers", "value_head"}
    extended = expected | {"value_head_width"}
    if set(value) not in (expected, extended):
        raise PublishedRunBehaviorError(f"{name} fields changed")
    value_head = value.get("value_head")
    if type(value_head) is not bool:
        raise PublishedRunBehaviorError(f"{name}.value_head must be bool")
    return RaggedScorerConfig(
        hidden_dim=_positive(value.get("hidden_dim"), f"{name}.hidden_dim"),
        relation_layers=_nonnegative(
            value.get("relation_layers"),
            f"{name}.relation_layers",
        ),
        value_head=value_head,
        value_head_width=_positive(
            value.get("value_head_width", 1),
            f"{name}.value_head_width",
        ),
    )


def _critic_initialization(
    record: Mapping[str, object],
    name: str,
) -> tuple[
    BehaviorManifestId | None,
    ManifestArtifactId | None,
    int | None,
    RunPolicyUpdateRule | None,
]:
    raw_manifest = record.get("critic_initialization_manifest_id")
    raw_checkpoint = record.get("critic_initialization_checkpoint_id")
    raw_step = record.get("critic_initialization_training_step")
    raw_update = record.get("critic_initialization_policy_update")
    values = (raw_manifest, raw_checkpoint, raw_step, raw_update)
    if all(value is None for value in values):
        return None, None, None, None
    if any(value is None for value in values):
        raise PublishedRunBehaviorError(
            f"{name} critic initialization provenance is incomplete"
        )
    if raw_update != RunPolicyUpdateRule.CRITIC_CALIBRATION.name.lower():
        raise PublishedRunBehaviorError(
            f"{name} critic initialization update is unsupported"
        )
    return (
        BehaviorManifestId(
            _digest(raw_manifest, f"{name} critic_initialization_manifest_id")
        ),
        ManifestArtifactId(
            ManifestArtifactKind.MODEL_CHECKPOINT,
            _digest(
                raw_checkpoint,
                f"{name} critic_initialization_checkpoint_id",
            ),
        ),
        _nonnegative(raw_step, f"{name} critic_initialization_training_step"),
        RunPolicyUpdateRule.CRITIC_CALIBRATION,
    )


def _run_policy_update(configuration: Mapping[str, object]) -> RunPolicyUpdateConfig:
    value = configuration.get("run_policy_update", "reinforce")
    if value == "reinforce":
        return RunPolicyUpdateConfig(value_prediction_target=configuration.get(
            "run_policy_value_prediction_target", "return_to_go"))
    if value not in {"ppo_clip_value", "critic_calibration"}:
        raise PublishedRunBehaviorError("run policy update is unsupported")
    return RunPolicyUpdateConfig(
        rule=(
            RunPolicyUpdateRule.PPO_CLIP_VALUE
            if value == "ppo_clip_value"
            else RunPolicyUpdateRule.CRITIC_CALIBRATION
        ),
        epochs=_positive(configuration.get("run_policy_epochs"), "run_policy_epochs"),
        clip_coefficient=_finite_float(
            configuration.get("run_policy_clip_coefficient"),
            "run_policy_clip_coefficient",
        ),
        entropy_coefficient=_finite_float(
            configuration.get("run_policy_entropy_coefficient"),
            "run_policy_entropy_coefficient",
        ),
        max_grad_norm=_optional_float(
            configuration.get("run_policy_max_grad_norm"),
            "run_policy_max_grad_norm",
        ),
        target_kl=_optional_float(
            configuration.get("run_policy_target_kl"),
            "run_policy_target_kl",
        ),
        value_loss_coefficient=_finite_float(
            configuration.get("run_policy_value_loss_coefficient"),
            "run_policy_value_loss_coefficient",
        ),
        normalize_advantage=_boolean(
            configuration.get("run_policy_normalize_advantage", False),
            "run_policy_normalize_advantage",
        ),
        value_clip_coefficient=_optional_float(
            configuration.get("run_policy_value_clip_coefficient"),
            "run_policy_value_clip_coefficient",
        ),
        value_prediction_target=configuration.get("run_policy_value_prediction_target", "return_to_go"),
    )


def _boolean(value: object, name: str) -> bool:
    if type(value) is not bool:
        raise PublishedRunBehaviorError(f"{name} must be boolean")
    return value


def _episode_root_attempts(
    value: object,
    mode: RunSamplingMode,
) -> int | None:
    if mode is RunSamplingMode.INDEPENDENT_COHORTS:
        if value is not None:
            raise PublishedRunBehaviorError(
                "independent sampling cannot carry episode-root attempts"
            )
        return None
    attempts = _positive(value, "episode_root_attempts")
    if attempts < 2:
        raise PublishedRunBehaviorError(
            "episode_root_attempts must be at least two"
        )
    return attempts


def _digest(value: object, name: str) -> bytes:
    if not isinstance(value, str) or len(value) != 64 or value != value.lower():
        raise PublishedRunBehaviorError(f"{name} must be a lowercase SHA-256 digest")
    try:
        return bytes.fromhex(value)
    except ValueError as error:
        raise PublishedRunBehaviorError(f"{name} must be a lowercase SHA-256 digest") from error


def _positive(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized == 0:
        raise PublishedRunBehaviorError(f"{name} must be positive")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise PublishedRunBehaviorError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise PublishedRunBehaviorError(f"{name} must be an integer") from error
    if normalized < 0:
        raise PublishedRunBehaviorError(f"{name} must be non-negative")
    return normalized


def _ascension(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized > 20:
        raise PublishedRunBehaviorError(f"{name} must be at most 20")
    return normalized


def _seed(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized >= 1 << 63:
        raise PublishedRunBehaviorError(f"{name} must be below 2^63")
    return normalized


def _finite_float(value: object, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, Real):
        raise PublishedRunBehaviorError(f"{name} must be a real number")
    normalized = float(value)
    if not math.isfinite(normalized):
        raise PublishedRunBehaviorError(f"{name} must be finite")
    return normalized


def _optional_float(value: object, name: str) -> float | None:
    if value is None:
        return None
    return _finite_float(value, name)
