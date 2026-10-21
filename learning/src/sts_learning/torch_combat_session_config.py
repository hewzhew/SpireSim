"""Typed bridge, algorithm, and resource configuration for combat sessions."""

from __future__ import annotations

import copy
import operator
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field

from .combat_experience import CombatExperienceLimits
from .combat_objective import CombatWinObjectiveConfig
from .combat_potion_lane import (
    CombatPotionLane,
    CombatPotionLaneError,
    normalize_combat_potion_slots,
)
from .combat_rollout import COMBAT_ROLLOUT_VALUE_HEAD_WIDTH
from .manifest_catalog import BehaviorManifestCatalogLimits
from .semantic_concat import SemanticBatchConcatLimits
from .torch_checkpoints import TorchCheckpointLimits
from .torch_policy import RaggedCategoricalPolicyConfig, RaggedScorerConfig
from .torch_provenance import AdamTrainingConfig


class TorchCombatSessionError(RuntimeError):
    """A combat session profile or owner graph is invalid."""


@dataclass(frozen=True)
class CombatSessionBridge:
    """Opaque production-root loader plus the exact bridge semantic schema."""

    combat_roots_from_artifact: Callable[..., object]
    semantic_schema: Mapping[str, object]

    def __post_init__(self) -> None:
        if not callable(self.combat_roots_from_artifact):
            raise TorchCombatSessionError(
                "combat bridge artifact loader must be callable"
            )
        if not isinstance(self.semantic_schema, Mapping):
            raise TorchCombatSessionError(
                "combat bridge semantic_schema must be a mapping"
            )
        object.__setattr__(
            self,
            "semantic_schema",
            copy.deepcopy(dict(self.semantic_schema)),
        )

    @classmethod
    def installed(cls) -> CombatSessionBridge:
        """Load the separately installed Rust bridge only on explicit request."""

        try:
            from sts_learning_bridge import LearningBatchEnv, semantic_schema
        except ImportError as error:
            raise TorchCombatSessionError(
                "standalone learning bridge wheel is not installed"
            ) from error
        loader = getattr(
            LearningBatchEnv,
            "from_combat_root_artifact_bytes",
            None,
        )
        if not callable(loader):
            raise TorchCombatSessionError(
                "installed learning bridge is stale"
            )
        return cls(
            combat_roots_from_artifact=loader,
            semantic_schema=semantic_schema(),
        )


@dataclass(frozen=True)
class CombatWinSessionProfile:
    """Exact maintained model and optimizer profile for same-root wins."""

    scorer: RaggedScorerConfig = RaggedScorerConfig()
    behavior: RaggedCategoricalPolicyConfig = RaggedCategoricalPolicyConfig()
    optimizer: AdamTrainingConfig = AdamTrainingConfig()
    objective: CombatWinObjectiveConfig = CombatWinObjectiveConfig()
    device_type: str = "cpu"

    def __post_init__(self) -> None:
        if not isinstance(self.scorer, RaggedScorerConfig):
            raise TorchCombatSessionError("combat session scorer must be typed")
        if self.scorer.relation_layers == 0:
            raise TorchCombatSessionError(
                "combat session requires a relation-aware scorer"
            )
        if not isinstance(self.behavior, RaggedCategoricalPolicyConfig):
            raise TorchCombatSessionError("combat session behavior must be typed")
        if not isinstance(self.optimizer, AdamTrainingConfig):
            raise TorchCombatSessionError("combat session optimizer must be typed")
        if not isinstance(self.objective, CombatWinObjectiveConfig):
            raise TorchCombatSessionError("combat session objective must be typed")
        expected_value_width = (
            COMBAT_ROLLOUT_VALUE_HEAD_WIDTH
            if self.objective.policy_update.uses_value_baseline
            else 1
        )
        if (
            self.scorer.value_head
            != self.objective.policy_update.uses_value_baseline
            or self.scorer.value_head_width != expected_value_width
        ):
            raise TorchCombatSessionError(
                "combat session scorer value profile disagrees with the objective"
            )
        if self.device_type != "cpu":
            raise TorchCombatSessionError(
                "the first maintained combat session supports only cpu"
            )


@dataclass(frozen=True)
class CombatWinSessionLimits:
    """Mandatory root-import, experience, concat, and store bounds."""

    owner_capacity: int = 16
    max_artifact_bytes: int = 16 * 1024 * 1024
    experience: CombatExperienceLimits = CombatExperienceLimits(
        max_decisions=4_096,
        max_payload_bytes=64 * 1024 * 1024,
        max_model_rounds=4_096,
        max_transitions=4_096,
    )
    concat: SemanticBatchConcatLimits = SemanticBatchConcatLimits(
        max_rows=4_096,
        max_input_array_bytes=64 * 1024 * 1024,
    )
    max_checkpoint_bytes: int = 16 * 1024 * 1024

    def __post_init__(self) -> None:
        for name in (
            "owner_capacity",
            "max_artifact_bytes",
            "max_checkpoint_bytes",
        ):
            object.__setattr__(
                self,
                name,
                _positive_integer(getattr(self, name), name),
            )
        if not isinstance(self.experience, CombatExperienceLimits):
            raise TorchCombatSessionError(
                "combat session experience limits must be typed"
            )
        if not isinstance(self.concat, SemanticBatchConcatLimits):
            raise TorchCombatSessionError(
                "combat session concat limits must be typed"
            )

    @property
    def checkpoint_store(self) -> TorchCheckpointLimits:
        return TorchCheckpointLimits(
            max_checkpoints=self.owner_capacity,
            max_bytes_per_checkpoint=self.max_checkpoint_bytes,
            max_total_bytes=self.owner_capacity * self.max_checkpoint_bytes,
        )

    @property
    def manifest_catalog(self) -> BehaviorManifestCatalogLimits:
        per_manifest = 4 * 1024
        return BehaviorManifestCatalogLimits(
            max_manifests=self.owner_capacity,
            max_bytes_per_manifest=per_manifest,
            max_total_bytes=self.owner_capacity * per_manifest,
        )


@dataclass(frozen=True)
class CombatWinSessionConfig:
    """Artifact selection, replicate count, algorithm, and resource bounds."""

    expected_roots: int = 1
    root_slot_index: int = 0
    replicate_count: int = 8
    profile: CombatWinSessionProfile = field(
        default_factory=CombatWinSessionProfile
    )
    limits: CombatWinSessionLimits = field(
        default_factory=CombatWinSessionLimits
    )

    def __post_init__(self) -> None:
        expected = _positive_integer(self.expected_roots, "expected_roots")
        slot = _nonnegative_integer(self.root_slot_index, "root_slot_index")
        replicates = _positive_integer(self.replicate_count, "replicate_count")
        if slot >= expected:
            raise TorchCombatSessionError(
                "root_slot_index must be below expected_roots"
            )
        if replicates < 2:
            raise TorchCombatSessionError(
                "combat session requires at least two replicates"
            )
        if not isinstance(self.profile, CombatWinSessionProfile):
            raise TorchCombatSessionError("combat session profile must be typed")
        if self.profile.objective.groups_per_update != 1:
            raise TorchCombatSessionError(
                "fixed-root combat session requires one group per update"
            )
        if not isinstance(self.limits, CombatWinSessionLimits):
            raise TorchCombatSessionError("combat session limits must be typed")
        object.__setattr__(self, "expected_roots", expected)
        object.__setattr__(self, "root_slot_index", slot)
        object.__setattr__(self, "replicate_count", replicates)


@dataclass(frozen=True)
class CombatWinBatchSessionConfig:
    """Exact frontier delivery width, algorithm, and resource bounds."""

    expected_roots: int
    max_roots: int
    replicate_count: int = 8
    profile: CombatWinSessionProfile = field(
        default_factory=CombatWinSessionProfile
    )
    limits: CombatWinSessionLimits = field(
        default_factory=CombatWinSessionLimits
    )
    potion_lane: CombatPotionLane = CombatPotionLane.ALL
    potion_slots: tuple[int, ...] = ()

    def __post_init__(self) -> None:
        expected = _positive_integer(self.expected_roots, "expected_roots")
        root_bound = _positive_integer(self.max_roots, "max_roots")
        replicates = _positive_integer(self.replicate_count, "replicate_count")
        if expected > root_bound:
            raise TorchCombatSessionError(
                "combat batch session expected roots exceed max_roots"
            )
        if replicates < 2:
            raise TorchCombatSessionError(
                "combat batch session requires at least two replicates"
            )
        if not isinstance(self.profile, CombatWinSessionProfile):
            raise TorchCombatSessionError(
                "combat batch session profile must be typed"
            )
        if self.profile.objective.groups_per_update != expected:
            raise TorchCombatSessionError(
                "combat batch roots must equal groups_per_update"
            )
        if not isinstance(self.limits, CombatWinSessionLimits):
            raise TorchCombatSessionError(
                "combat batch session limits must be typed"
            )
        if not isinstance(self.potion_lane, CombatPotionLane):
            raise TorchCombatSessionError(
                "combat batch session potion_lane must be typed"
            )
        try:
            potion_slots = normalize_combat_potion_slots(
                self.potion_lane,
                self.potion_slots,
            )
        except CombatPotionLaneError as error:
            raise TorchCombatSessionError(str(error)) from error
        object.__setattr__(self, "expected_roots", expected)
        object.__setattr__(self, "max_roots", root_bound)
        object.__setattr__(self, "replicate_count", replicates)
        object.__setattr__(self, "potion_slots", potion_slots)


def _positive_integer(value: object, name: str) -> int:
    normalized = _nonnegative_integer(value, name)
    if normalized == 0:
        raise TorchCombatSessionError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchCombatSessionError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchCombatSessionError(f"{name} must be an integer") from error
    if normalized < 0:
        raise TorchCombatSessionError(f"{name} must be non-negative")
    return normalized
