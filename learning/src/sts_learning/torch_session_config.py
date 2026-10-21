"""Typed bridge, algorithm, and resource configuration for torch sessions."""

from __future__ import annotations

import copy
import operator
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field

import torch

from .attempts import AttemptAssemblyLimits
from .attempt_batching import AttemptUpdateBatchLimits
from .driver import BatchEnvironment
from .experience import ExperienceLimits
from .manifest_catalog import BehaviorManifestCatalogLimits
from .resume_store import ResumeStoreLimits
from .seeds import SeedPartition, SeedSchedule
from .semantic_concat import SemanticBatchConcatLimits
from .terminal_returns import OnPolicyObjectiveConfig, RunDecisionScope
from .manifests import (
    BehaviorRuleBinding,
    combat_anchored_greedy_strategic_sampled_rule_v1,
    combat_greedy_strategic_sampled_rule_v1,
)
from .policy import BehaviorManifestId
from .torch_behavior import FrozenDecisionRule
from .torch_checkpoints import TorchCheckpointLimits
from .torch_policy import RaggedCategoricalPolicyConfig, RaggedScorerConfig
from .torch_provenance import AdamTrainingConfig
from .torch_resume_publication import CategoricalResumePayloadLimits
from .torch_resume_restore import CheckpointBankDecoder, EnvironmentCheckpointDecoder


class TorchSessionError(RuntimeError):
    """A categorical session profile or owner graph is invalid."""


def normalize_torch_device_type(value: object) -> str:
    """Return one maintained torch device type and fail before session setup."""

    if not isinstance(value, str):
        raise TorchSessionError("session device_type must be a string")
    device_type = value.strip().lower()
    if device_type not in {"cpu", "cuda"}:
        raise TorchSessionError(
            "session device_type must be either 'cpu' or 'cuda'"
        )
    if device_type == "cuda" and not torch.cuda.is_available():
        raise TorchSessionError(
            "session device_type requests cuda, but torch.cuda is unavailable"
        )
    return device_type


@dataclass(frozen=True)
class CategoricalSessionBridge:
    """The bridge callables and exact semantic schema used by a session."""

    environment: Callable[[list[int], int], BatchEnvironment]
    environment_without_combat_potions: Callable[[list[int], int], BatchEnvironment]
    environment_from_checkpoint: EnvironmentCheckpointDecoder
    checkpoint_bank_from_checkpoint: CheckpointBankDecoder
    semantic_schema: Mapping[str, object]

    def __post_init__(self) -> None:
        for name in (
            "environment",
            "environment_without_combat_potions",
            "environment_from_checkpoint",
            "checkpoint_bank_from_checkpoint",
        ):
            if not callable(getattr(self, name)):
                raise TorchSessionError(f"bridge {name} must be callable")
        if not isinstance(self.semantic_schema, Mapping):
            raise TorchSessionError("bridge semantic_schema must be a mapping")
        object.__setattr__(
            self,
            "semantic_schema",
            copy.deepcopy(dict(self.semantic_schema)),
        )

    @classmethod
    def installed(cls) -> CategoricalSessionBridge:
        """Load the separately installed Rust bridge only on explicit request."""

        try:
            from sts_learning_bridge import (
                LearningBatchEnv,
                LearningCheckpointBatch,
                semantic_schema,
            )
        except ImportError as error:
            raise TorchSessionError(
                "standalone learning bridge wheel is not installed"
            ) from error
        return cls(
            environment=LearningBatchEnv,
            environment_without_combat_potions=(
                LearningBatchEnv.without_combat_potions
            ),
            environment_from_checkpoint=(
                LearningBatchEnv.from_checkpoint_bytes
            ),
            checkpoint_bank_from_checkpoint=(
                LearningCheckpointBatch.from_checkpoint_bytes
            ),
            semantic_schema=semantic_schema(),
        )


@dataclass(frozen=True)
class CategoricalOnlineProfile:
    """Algorithm configuration shared by new and restored online sessions."""

    scorer: RaggedScorerConfig = RaggedScorerConfig()
    behavior: RaggedCategoricalPolicyConfig = RaggedCategoricalPolicyConfig()
    optimizer: AdamTrainingConfig = AdamTrainingConfig()
    objective: OnPolicyObjectiveConfig = OnPolicyObjectiveConfig()
    combat_decision_rule: FrozenDecisionRule = FrozenDecisionRule.SAMPLED
    combat_anchor_manifest_id: BehaviorManifestId | None = None
    combat_anchor_scorer: RaggedScorerConfig | None = None
    optimizer_steps_per_generation: int = 1
    device_type: str = "cpu"

    def __post_init__(self) -> None:
        if not isinstance(self.scorer, RaggedScorerConfig):
            raise TorchSessionError("session scorer config must be typed")
        if self.scorer.relation_layers == 0:
            raise TorchSessionError(
                "online session requires a relation-aware scorer"
            )
        if not isinstance(self.behavior, RaggedCategoricalPolicyConfig):
            raise TorchSessionError("session behavior config must be typed")
        if not isinstance(self.optimizer, AdamTrainingConfig):
            raise TorchSessionError("session optimizer config must be typed")
        if not isinstance(self.objective, OnPolicyObjectiveConfig):
            raise TorchSessionError("session objective config must be typed")
        if not isinstance(self.combat_decision_rule, FrozenDecisionRule):
            raise TorchSessionError(
                "session combat decision rule must be typed"
            )
        anchor_id = self.combat_anchor_manifest_id
        anchor_scorer = self.combat_anchor_scorer
        if (anchor_id is None) != (anchor_scorer is None):
            raise TorchSessionError(
                "session combat anchor identity and scorer must appear together"
            )
        if anchor_id is not None and not isinstance(
            anchor_id,
            BehaviorManifestId,
        ):
            raise TorchSessionError("session combat anchor id must be typed")
        if anchor_scorer is not None and not isinstance(
            anchor_scorer,
            RaggedScorerConfig,
        ):
            raise TorchSessionError("session combat anchor scorer must be typed")
        if (
            anchor_id is not None
            and self.combat_decision_rule is not FrozenDecisionRule.GREEDY
        ):
            raise TorchSessionError(
                "session combat anchor requires greedy combat decisions"
            )
        if (
            self.combat_decision_rule is FrozenDecisionRule.GREEDY
            and self.objective.decision_scope is not RunDecisionScope.STRATEGIC
        ):
            raise TorchSessionError(
                "combat-greedy training requires strategic decision scope"
            )
        if (
            self.scorer.value_head
            != self.objective.policy_update.uses_value_baseline
        ):
            raise TorchSessionError(
                "session scorer value head disagrees with the run update rule"
            )
        optimizer_steps = _positive_integer(
            self.optimizer_steps_per_generation,
            "optimizer_steps_per_generation",
        )
        if optimizer_steps != 1:
            raise TorchSessionError(
                "on-policy session requires exactly one optimizer step per generation"
            )
        object.__setattr__(self, "optimizer_steps_per_generation", optimizer_steps)
        object.__setattr__(
            self,
            "device_type",
            normalize_torch_device_type(self.device_type),
        )

    @property
    def behavior_rule(self) -> BehaviorRuleBinding:
        if self.combat_decision_rule is FrozenDecisionRule.SAMPLED:
            return self.behavior.behavior_rule
        if self.combat_anchor_manifest_id is not None:
            return combat_anchored_greedy_strategic_sampled_rule_v1(
                self.behavior.behavior_rule,
                self.combat_anchor_manifest_id,
            )
        return combat_greedy_strategic_sampled_rule_v1(
            self.behavior.behavior_rule
        )


@dataclass(frozen=True)
class CategoricalSessionLimits:
    """One bounded local profile for memory and immutable store growth."""

    owner_capacity: int = 16
    experience: ExperienceLimits = ExperienceLimits(
        max_decisions=64,
        max_payload_bytes=16 * 1024 * 1024,
    )
    attempts: AttemptAssemblyLimits = AttemptAssemblyLimits(
        max_open_attempts=8,
        max_decisions_per_attempt=4_096,
        max_payload_bytes_per_attempt=64 * 1024 * 1024,
    )
    attempt_updates: AttemptUpdateBatchLimits = AttemptUpdateBatchLimits(
        max_decisions_per_update=4_096,
        max_payload_bytes_per_update=64 * 1024 * 1024,
    )
    concat: SemanticBatchConcatLimits = SemanticBatchConcatLimits(
        max_rows=4_096,
        max_input_array_bytes=64 * 1024 * 1024,
    )
    resume_payloads: CategoricalResumePayloadLimits = (
        CategoricalResumePayloadLimits(
            max_environment_bytes=64 * 1024 * 1024,
            max_episode_root_bank_bytes=64 * 1024 * 1024,
            max_shadow_model_bytes=16 * 1024 * 1024,
            max_optimizer_bytes=64 * 1024 * 1024,
            max_generator_bytes=1024 * 1024,
            max_metadata_bytes=4 * 1024 * 1024,
        )
    )

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "owner_capacity",
            _positive_integer(self.owner_capacity, "owner_capacity"),
        )
        for name, expected in (
            ("experience", ExperienceLimits),
            ("attempts", AttemptAssemblyLimits),
            ("attempt_updates", AttemptUpdateBatchLimits),
            ("concat", SemanticBatchConcatLimits),
            ("resume_payloads", CategoricalResumePayloadLimits),
        ):
            if not isinstance(getattr(self, name), expected):
                raise TorchSessionError(f"session {name} limits must be typed")

    @property
    def checkpoint_store(self) -> TorchCheckpointLimits:
        per_checkpoint = self.resume_payloads.max_shadow_model_bytes
        return TorchCheckpointLimits(
            max_checkpoints=self.owner_capacity,
            max_bytes_per_checkpoint=per_checkpoint,
            max_total_bytes=self.owner_capacity * per_checkpoint,
        )

    @property
    def manifest_catalog(self) -> BehaviorManifestCatalogLimits:
        per_manifest = 4 * 1024
        return BehaviorManifestCatalogLimits(
            max_manifests=self.owner_capacity,
            max_bytes_per_manifest=per_manifest,
            max_total_bytes=self.owner_capacity * per_manifest,
        )

    @property
    def resume_store(self) -> ResumeStoreLimits:
        payloads = self.resume_payloads
        component_limits = (
            payloads.max_environment_bytes,
            payloads.max_episode_root_bank_bytes,
            payloads.max_shadow_model_bytes,
            payloads.max_optimizer_bytes,
            payloads.max_generator_bytes,
            payloads.max_metadata_bytes,
        )
        manifest_bytes = 4 * 1024
        return ResumeStoreLimits(
            max_components=6 * self.owner_capacity,
            max_bytes_per_component=max(component_limits),
            max_total_component_bytes=(
                self.owner_capacity * sum(component_limits)
            ),
            max_manifests=self.owner_capacity,
            max_bytes_per_manifest=manifest_bytes,
            max_total_manifest_bytes=self.owner_capacity * manifest_bytes,
        )


@dataclass(frozen=True)
class CategoricalOnlineSessionConfig:
    """Population, algorithm, and bounded-resource configuration."""

    ascension_level: int
    schedule: SeedSchedule = field(
        default_factory=lambda: SeedSchedule(SeedPartition.TRAINING)
    )
    slot_count: int = 1
    max_recoveries_per_episode: int = 0
    max_decision_rounds_per_step: int = 256
    profile: CategoricalOnlineProfile = field(
        default_factory=CategoricalOnlineProfile
    )
    limits: CategoricalSessionLimits = field(
        default_factory=CategoricalSessionLimits
    )

    def __post_init__(self) -> None:
        ascension_level = _non_negative_integer(
            self.ascension_level,
            "ascension_level",
        )
        if ascension_level > 20:
            raise TorchSessionError("ascension_level must be at most 20")
        object.__setattr__(self, "ascension_level", ascension_level)
        if not isinstance(self.schedule, SeedSchedule):
            raise TorchSessionError("session schedule must be typed")
        if self.schedule.partition is not SeedPartition.TRAINING:
            raise TorchSessionError(
                "online training requires the training seed partition"
            )
        object.__setattr__(
            self,
            "slot_count",
            _positive_integer(self.slot_count, "slot_count"),
        )
        object.__setattr__(
            self,
            "max_recoveries_per_episode",
            _non_negative_integer(
                self.max_recoveries_per_episode,
                "max_recoveries_per_episode",
            ),
        )
        object.__setattr__(
            self,
            "max_decision_rounds_per_step",
            _positive_integer(
                self.max_decision_rounds_per_step,
                "max_decision_rounds_per_step",
            ),
        )
        if not isinstance(self.profile, CategoricalOnlineProfile):
            raise TorchSessionError("session profile must be typed")
        if not isinstance(self.limits, CategoricalSessionLimits):
            raise TorchSessionError("session limits must be typed")


def _positive_integer(value: object, name: str) -> int:
    normalized = _non_negative_integer(value, name)
    if normalized == 0:
        raise TorchSessionError(f"{name} must be positive")
    return normalized


def _non_negative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchSessionError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchSessionError(f"{name} must be an integer") from error
    if normalized < 0:
        raise TorchSessionError(f"{name} must be non-negative")
    return normalized
