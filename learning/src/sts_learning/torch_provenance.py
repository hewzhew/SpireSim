"""Canonical provenance for the maintained categorical PyTorch baseline."""

from __future__ import annotations

import math
import operator
import struct
from collections.abc import Iterable, Mapping
from dataclasses import dataclass

import torch

from .combat_objective import (
    CombatAllLossAxis,
    CombatPolicyUpdateRule,
    CombatWinObjectiveConfig,
)
from .manifests import (
    BehaviorRuleBinding,
    BehaviorManifestTemplate,
    GREEDY_BEHAVIOR_RULE_V1,
    ManifestArtifactId,
    ManifestArtifactKind,
)
from .torch_policy import (
    RaggedCategoricalPolicyConfig,
    RaggedScorerConfig,
    SemanticSchemaDimensions,
)
from .terminal_returns import (
    OnPolicyObjectiveConfig,
    RunPolicyUpdateRule,
)


class TorchProvenanceError(ValueError):
    """A runtime profile cannot be represented by exact canonical provenance."""


_MODEL_DEFINITION_VERSION = 1
_MODEL_CONFIG_VERSION = 1
_ACTOR_CRITIC_MODEL_DEFINITION_VERSION = 2
_ACTOR_CRITIC_MODEL_CONFIG_VERSION = 2
_MULTI_ACTOR_CRITIC_MODEL_DEFINITION_VERSION = 3
_MULTI_ACTOR_CRITIC_MODEL_CONFIG_VERSION = 3
_SEMANTIC_SCHEMA_ENCODING_VERSION = 1
_OPTIMIZER_CONFIG_VERSION = 1
_TRAINER_IMPLEMENTATION_VERSION = 5
_TERMINAL_RETURN_CONFIG_VERSION = 1
_RUN_VALUE_PPO_TRAINER_IMPLEMENTATION_VERSION = 5
_RUN_VALUE_PPO_OBJECTIVE_VERSION = 4
_RUN_CRITIC_CALIBRATION_TRAINER_IMPLEMENTATION_VERSION = 1
_RUN_CRITIC_CALIBRATION_OBJECTIVE_VERSION = 1
_COMBAT_WIN_TRAINER_IMPLEMENTATION_VERSION = 3
_COMBAT_WIN_OBJECTIVE_VERSION = 3
_COMBAT_PPO_TRAINER_IMPLEMENTATION_VERSION = 4
_COMBAT_PPO_OBJECTIVE_VERSION = 4
_COMBAT_VALUE_PPO_TRAINER_IMPLEMENTATION_VERSION = 2
_COMBAT_VALUE_PPO_OBJECTIVE_VERSION = 2
_COMBAT_ALL_LOSS_WIN_TRAINER_IMPLEMENTATION_VERSION = 4
_COMBAT_ALL_LOSS_WIN_OBJECTIVE_VERSION = 4
_COMBAT_ALL_LOSS_PPO_TRAINER_IMPLEMENTATION_VERSION = 5
_COMBAT_ALL_LOSS_PPO_OBJECTIVE_VERSION = 5
_COMBAT_ALL_LOSS_VALUE_PPO_TRAINER_IMPLEMENTATION_VERSION = 3
COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS = (
    "ragged_cross_entropy_on_strict_proposal_else_frozen_baseline"
)
COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS = (
    "strict_proposal_cross_entropy_plus_retained_forward_kl_v1"
)
_COMBAT_ALL_LOSS_VALUE_PPO_OBJECTIVE_VERSION = 3
_MAX_SCHEMA_BYTES = 1 << 20
_MAX_SCHEMA_DEPTH = 16
_MAX_SCHEMA_ITEMS = 100_000


@dataclass(frozen=True)
class AdamTrainingConfig:
    """Exact Adam configuration used by the online policy trainer."""

    learning_rate: float = 1e-3
    beta1: float = 0.9
    beta2: float = 0.999
    epsilon: float = 1e-8
    weight_decay: float = 0.0
    amsgrad: bool = False

    def __post_init__(self) -> None:
        learning_rate = _finite_float(self.learning_rate, "learning_rate")
        beta1 = _finite_float(self.beta1, "beta1")
        beta2 = _finite_float(self.beta2, "beta2")
        epsilon = _finite_float(self.epsilon, "epsilon")
        weight_decay = _finite_float(self.weight_decay, "weight_decay")
        if learning_rate <= 0.0:
            raise TorchProvenanceError("learning_rate must be positive")
        if not 0.0 <= beta1 < 1.0 or not 0.0 <= beta2 < 1.0:
            raise TorchProvenanceError("Adam beta values must be in [0, 1)")
        if epsilon <= 0.0:
            raise TorchProvenanceError("epsilon must be positive")
        if weight_decay < 0.0:
            raise TorchProvenanceError("weight_decay must be non-negative")
        if type(self.amsgrad) is not bool:
            raise TorchProvenanceError("amsgrad must be bool")
        object.__setattr__(self, "learning_rate", learning_rate)
        object.__setattr__(self, "beta1", beta1)
        object.__setattr__(self, "beta2", beta2)
        object.__setattr__(self, "epsilon", epsilon)
        object.__setattr__(self, "weight_decay", weight_decay)

    def create(self, parameters: Iterable[torch.Tensor]) -> torch.optim.Adam:
        """Create the optimizer whose implementation choices are all explicit."""

        return torch.optim.Adam(
            parameters,
            lr=self.learning_rate,
            betas=(self.beta1, self.beta2),
            eps=self.epsilon,
            weight_decay=self.weight_decay,
            amsgrad=self.amsgrad,
            foreach=False,
            maximize=False,
            capturable=False,
            differentiable=False,
            fused=False,
        )

    @property
    def artifact_id(self) -> ManifestArtifactId:
        flags = 1 if self.amsgrad else 0
        content = (
            b"STS-ADAM-CONFIG\x00"
            + struct.pack(
                ">I5dB",
                _OPTIMIZER_CONFIG_VERSION,
                self.learning_rate,
                self.beta1,
                self.beta2,
                self.epsilon,
                self.weight_decay,
                flags,
            )
            + _runtime_version_bytes()
        )
        return ManifestArtifactId.from_content(
            ManifestArtifactKind.OPTIMIZER_CONFIG,
            content,
        )


def categorical_training_manifest_template(
    semantic_schema: Mapping[str, object],
    scorer_config: RaggedScorerConfig,
    behavior_config: RaggedCategoricalPolicyConfig,
    optimizer_config: AdamTrainingConfig,
    objective_config: OnPolicyObjectiveConfig,
    *,
    device_type: str,
    behavior_rule: BehaviorRuleBinding | None = None,
) -> BehaviorManifestTemplate:
    """Bind the exact maintained model, schema, optimizer, and trainer profile."""

    if not isinstance(objective_config, OnPolicyObjectiveConfig):
        raise TorchProvenanceError("objective_config must be typed")
    return _categorical_manifest_template(
        semantic_schema,
        scorer_config,
        behavior_config,
        optimizer_config,
        categorical_trainer_implementation(objective_config),
        device_type=device_type,
        behavior_rule=behavior_rule,
    )


def combat_win_training_manifest_template(
    semantic_schema: Mapping[str, object],
    scorer_config: RaggedScorerConfig,
    behavior_config: RaggedCategoricalPolicyConfig,
    optimizer_config: AdamTrainingConfig,
    objective_config: CombatWinObjectiveConfig,
    *,
    device_type: str,
) -> BehaviorManifestTemplate:
    """Bind the same scorer stack to the distinct same-root win trainer."""

    if not isinstance(objective_config, CombatWinObjectiveConfig):
        raise TorchProvenanceError("combat objective_config must be typed")
    return _categorical_manifest_template(
        semantic_schema,
        scorer_config,
        behavior_config,
        optimizer_config,
        combat_win_trainer_implementation(objective_config),
        device_type=device_type,
    )


def combat_search_distillation_manifest_template(
    semantic_schema: Mapping[str, object],
    scorer_config: RaggedScorerConfig,
    optimizer_config: AdamTrainingConfig,
    *,
    epochs: int,
    max_grad_norm: float,
    loss: str,
    device_type: str,
) -> BehaviorManifestTemplate:
    """Bind the experimental search-distillation scorer and greedy rule exactly."""

    if isinstance(epochs, bool):
        raise TorchProvenanceError("distillation epochs must be an integer")
    try:
        normalized_epochs = operator.index(epochs)
    except TypeError as error:
        raise TorchProvenanceError(
            "distillation epochs must be an integer"
        ) from error
    if normalized_epochs <= 0:
        raise TorchProvenanceError("distillation epochs must be positive")
    normalized_grad_norm = _finite_float(max_grad_norm, "max_grad_norm")
    if normalized_grad_norm <= 0.0:
        raise TorchProvenanceError("max_grad_norm must be positive")
    if loss == COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS:
        trainer_version = 1
        loss_identity = (
            b"RAGGED-CROSS-ENTROPY-STRICT-PROPOSAL-ELSE-FROZEN-BASELINE\x00"
        )
    elif loss == COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS:
        trainer_version = 2
        loss_identity = (
            b"STRICT-PROPOSAL-CROSS-ENTROPY-PLUS-RETAINED-FORWARD-KL-V1\x00"
        )
    else:
        raise TorchProvenanceError("unsupported combat search distillation loss")
    trainer = ManifestArtifactId.from_content(
        ManifestArtifactKind.TRAINER_IMPLEMENTATION,
        b"STS-COMBAT-SEARCH-TRAJECTORY-DISTILLATION\x00"
        + struct.pack(
            ">IQd",
            trainer_version,
            normalized_epochs,
            normalized_grad_norm,
        )
        + loss_identity
        + _runtime_version_bytes(),
    )
    return _categorical_manifest_template(
        semantic_schema,
        scorer_config,
        RaggedCategoricalPolicyConfig(),
        optimizer_config,
        trainer,
        device_type=device_type,
        behavior_rule=GREEDY_BEHAVIOR_RULE_V1,
    )


def _categorical_manifest_template(
    semantic_schema: Mapping[str, object],
    scorer_config: RaggedScorerConfig,
    behavior_config: RaggedCategoricalPolicyConfig,
    optimizer_config: AdamTrainingConfig,
    trainer_implementation: ManifestArtifactId,
    *,
    device_type: str,
    behavior_rule: BehaviorRuleBinding | None = None,
) -> BehaviorManifestTemplate:
    if not isinstance(scorer_config, RaggedScorerConfig):
        raise TorchProvenanceError("scorer_config must be typed")
    if not isinstance(behavior_config, RaggedCategoricalPolicyConfig):
        raise TorchProvenanceError("behavior_config must be typed")
    if not isinstance(optimizer_config, AdamTrainingConfig):
        raise TorchProvenanceError("optimizer_config must be typed")
    if not isinstance(trainer_implementation, ManifestArtifactId):
        raise TorchProvenanceError("trainer_implementation must be typed")
    if behavior_rule is not None and not isinstance(
        behavior_rule,
        BehaviorRuleBinding,
    ):
        raise TorchProvenanceError("behavior_rule must be typed")
    if type(device_type) is not str or not device_type:
        raise TorchProvenanceError("device_type must be a non-empty string")
    try:
        dimensions = SemanticSchemaDimensions.from_bridge_schema(semantic_schema)
    except (TypeError, ValueError) as error:
        raise TorchProvenanceError(str(error)) from error

    runtime = _runtime_version_bytes()
    model_definition_version = (
        _MULTI_ACTOR_CRITIC_MODEL_DEFINITION_VERSION
        if scorer_config.value_head and scorer_config.value_head_width > 1
        else _ACTOR_CRITIC_MODEL_DEFINITION_VERSION
        if scorer_config.value_head
        else _MODEL_DEFINITION_VERSION
    )
    model_definition = ManifestArtifactId.from_content(
        ManifestArtifactKind.MODEL_DEFINITION,
        b"STS-RAGGED-CANDIDATE-SCORER\x00"
        + struct.pack(">I", model_definition_version)
        + runtime,
    )
    encoded_device = device_type.encode("utf-8")
    if len(encoded_device) > 255:
        raise TorchProvenanceError("device_type is too large")
    if scorer_config.value_head and scorer_config.value_head_width > 1:
        encoded_model_config = struct.pack(
            ">IQQQB",
            _MULTI_ACTOR_CRITIC_MODEL_CONFIG_VERSION,
            scorer_config.hidden_dim,
            scorer_config.relation_layers,
            scorer_config.value_head_width,
            len(encoded_device),
        )
    elif scorer_config.value_head:
        encoded_model_config = struct.pack(
            ">IQQBB",
            _ACTOR_CRITIC_MODEL_CONFIG_VERSION,
            scorer_config.hidden_dim,
            scorer_config.relation_layers,
            1,
            len(encoded_device),
        )
    else:
        encoded_model_config = struct.pack(
            ">IQQB",
            _MODEL_CONFIG_VERSION,
            scorer_config.hidden_dim,
            scorer_config.relation_layers,
            len(encoded_device),
        )
    model_config = ManifestArtifactId.from_content(
        ManifestArtifactKind.MODEL_CONFIG,
        b"STS-RAGGED-SCORER-CONFIG\x00"
        + encoded_model_config
        + encoded_device,
    )
    schema_content = (
        b"STS-SEMANTIC-SCHEMA\x00"
        + struct.pack(">I", _SEMANTIC_SCHEMA_ENCODING_VERSION)
        + _canonical_schema_bytes(semantic_schema)
    )
    semantic_schema_id = ManifestArtifactId.from_content(
        ManifestArtifactKind.SEMANTIC_SCHEMA,
        schema_content,
    )
    return BehaviorManifestTemplate(
        model_definition=model_definition,
        model_config=model_config,
        behavior_rule=(
            behavior_config.behavior_rule
            if behavior_rule is None
            else behavior_rule
        ),
        semantic_schema=semantic_schema_id,
        optimizer_config=optimizer_config.artifact_id,
        trainer_implementation=trainer_implementation,
        semantic_schema_version=dimensions.version,
    )


def categorical_trainer_implementation(
    objective_config: OnPolicyObjectiveConfig,
) -> ManifestArtifactId:
    """Bind the exact objective, return, and attempts-per-update contract."""

    if not isinstance(objective_config, OnPolicyObjectiveConfig):
        raise TorchProvenanceError("objective_config must be typed")
    update = objective_config.policy_update
    if update.rule is RunPolicyUpdateRule.REINFORCE:
        return ManifestArtifactId.from_content(
            ManifestArtifactKind.TRAINER_IMPLEMENTATION,
            b"STS-SYNCHRONOUS-TERMINAL-POLICY-TRAINER\x00"
            + objective_config.terminal_return.metric.encode("ascii") + b"\x00"
            + struct.pack(">I", _TRAINER_IMPLEMENTATION_VERSION)
            + b"STS-FLOOR-PROGRESS-RETURN\x00"
            + struct.pack(
                ">IQQBB",
                _TERMINAL_RETURN_CONFIG_VERSION,
                objective_config.terminal_return.target_floor,
                objective_config.attempts_per_update,
                int(objective_config.advantage_mode),
                int(objective_config.decision_scope),
            )
            + _runtime_version_bytes(),
        )
    max_grad_norm = update.max_grad_norm
    target_kl = update.target_kl
    value_clip = update.value_clip_coefficient
    critic_calibration = update.rule is RunPolicyUpdateRule.CRITIC_CALIBRATION
    objective_version = (
        _RUN_CRITIC_CALIBRATION_OBJECTIVE_VERSION
        if critic_calibration
        else _RUN_VALUE_PPO_OBJECTIVE_VERSION
    )
    objective_encoding = struct.pack(
        ">IQQBBBBQdddBdBdBddd",
        objective_version,
        objective_config.terminal_return.target_floor,
        objective_config.attempts_per_update,
        int(objective_config.advantage_mode),
        int(objective_config.decision_scope),
        int(update.rule),
        int(update.normalize_advantage),
        update.epochs,
        update.clip_coefficient,
        update.entropy_coefficient,
        update.value_loss_coefficient,
        int(max_grad_norm is not None),
        0.0 if max_grad_norm is None else max_grad_norm,
        int(target_kl is not None),
        0.0 if target_kl is None else target_kl,
        int(value_clip is not None),
        0.0 if value_clip is None else value_clip,
        1.0,
        1.0,
    )
    if update.value_prediction_target == "terminal_score":
        # The new head target and its exact public-potential conversion have
        # their own contract, even though model tensor shapes are unchanged.
        objective_encoding += b"TERMINAL-SCORE-HEAD-PUBLIC-PROGRESS-POTENTIAL-V1\x00"
    if critic_calibration:
        trainer_marker = b"STS-SYNCHRONOUS-RUN-CRITIC-CALIBRATION-TRAINER\x00"
        trainer_version = _RUN_CRITIC_CALIBRATION_TRAINER_IMPLEMENTATION_VERSION
        objective_marker = b"STS-DECISION-LOCAL-FLOOR-RETURN-VALUE-HEAD-ONLY\x00"
    else:
        trainer_marker = b"STS-SYNCHRONOUS-RUN-DECISION-LOCAL-GAE-PPO-TRAINER\x00"
        trainer_version = _RUN_VALUE_PPO_TRAINER_IMPLEMENTATION_VERSION
        objective_marker = b"STS-DECISION-LOCAL-FLOOR-RETURN-GAE\x00"
    return ManifestArtifactId.from_content(
        ManifestArtifactKind.TRAINER_IMPLEMENTATION,
        trainer_marker
        + objective_config.terminal_return.metric.encode("ascii") + b"\x00"
        + struct.pack(">I", trainer_version)
        + objective_marker
        + objective_encoding
        + _runtime_version_bytes(),
    )


def combat_win_trainer_implementation(
    objective_config: CombatWinObjectiveConfig,
) -> ManifestArtifactId:
    """Bind same-root win-first/HP-fallback learning and update width."""

    if not isinstance(objective_config, CombatWinObjectiveConfig):
        raise TorchProvenanceError("combat objective_config must be typed")
    update = objective_config.policy_update
    all_loss_enabled = (
        objective_config.all_loss_axis is not CombatAllLossAxis.NONE
    )
    if update.rule is CombatPolicyUpdateRule.REINFORCE:
        if all_loss_enabled:
            return ManifestArtifactId.from_content(
                ManifestArtifactKind.TRAINER_IMPLEMENTATION,
                b"STS-SYNCHRONOUS-COMBAT-WIN-FIRST-POLICY-TRAINER\x00"
                + struct.pack(
                    ">I",
                    _COMBAT_ALL_LOSS_WIN_TRAINER_IMPLEMENTATION_VERSION,
                )
                + b"STS-SAME-ROOT-WIN-FIRST-TYPED-FALLBACK-AXES\x00"
                + struct.pack(
                    ">IQBB",
                    _COMBAT_ALL_LOSS_WIN_OBJECTIVE_VERSION,
                    objective_config.groups_per_update,
                    int(objective_config.all_win_axis),
                    int(objective_config.all_loss_axis),
                )
                + _runtime_version_bytes(),
            )
        return ManifestArtifactId.from_content(
            ManifestArtifactKind.TRAINER_IMPLEMENTATION,
            b"STS-SYNCHRONOUS-COMBAT-WIN-FIRST-POLICY-TRAINER\x00"
            + struct.pack(">I", _COMBAT_WIN_TRAINER_IMPLEMENTATION_VERSION)
            + b"STS-SAME-ROOT-WIN-FIRST-OPTIONAL-ALL-WIN-AXIS\x00"
            + struct.pack(
                ">IQB",
                _COMBAT_WIN_OBJECTIVE_VERSION,
                objective_config.groups_per_update,
                int(objective_config.all_win_axis),
            )
            + _runtime_version_bytes(),
        )
    if update.rule is CombatPolicyUpdateRule.PPO_CLIP_VALUE:
        max_grad_norm = update.max_grad_norm
        target_kl = update.target_kl
        if all_loss_enabled:
            implementation_version = (
                _COMBAT_ALL_LOSS_VALUE_PPO_TRAINER_IMPLEMENTATION_VERSION
            )
            objective_version = _COMBAT_ALL_LOSS_VALUE_PPO_OBJECTIVE_VERSION
            objective_marker = (
                b"STS-SAME-ROOT-WIN-FIRST-PPO-CLIP-VALUE-DECISION-LOCAL-"
                b"MULTI-HEAD-TYPED-FALLBACK-AXES\x00"
            )
            objective_encoding = struct.pack(
                ">IQBBBQdddBdBd",
                objective_version,
                objective_config.groups_per_update,
                int(objective_config.all_win_axis),
                int(objective_config.all_loss_axis),
                int(update.rule),
                update.epochs,
                update.clip_coefficient,
                update.entropy_coefficient,
                update.value_loss_coefficient,
                int(max_grad_norm is not None),
                0.0 if max_grad_norm is None else max_grad_norm,
                int(target_kl is not None),
                0.0 if target_kl is None else target_kl,
            )
        else:
            implementation_version = _COMBAT_VALUE_PPO_TRAINER_IMPLEMENTATION_VERSION
            objective_marker = (
                b"STS-SAME-ROOT-WIN-FIRST-PPO-CLIP-VALUE-DECISION-LOCAL-"
                b"MULTI-HEAD\x00"
            )
            objective_encoding = struct.pack(
                ">IQBBQdddBdBd",
                _COMBAT_VALUE_PPO_OBJECTIVE_VERSION,
                objective_config.groups_per_update,
                int(objective_config.all_win_axis),
                int(update.rule),
                update.epochs,
                update.clip_coefficient,
                update.entropy_coefficient,
                update.value_loss_coefficient,
                int(max_grad_norm is not None),
                0.0 if max_grad_norm is None else max_grad_norm,
                int(target_kl is not None),
                0.0 if target_kl is None else target_kl,
            )
        return ManifestArtifactId.from_content(
            ManifestArtifactKind.TRAINER_IMPLEMENTATION,
            b"STS-SYNCHRONOUS-COMBAT-PPO-CLIP-VALUE-TRAINER\x00"
            + struct.pack(">I", implementation_version)
            + objective_marker
            + objective_encoding
            + _runtime_version_bytes(),
        )
    max_grad_norm = update.max_grad_norm
    target_kl = update.target_kl
    if all_loss_enabled:
        implementation_version = (
            _COMBAT_ALL_LOSS_PPO_TRAINER_IMPLEMENTATION_VERSION
        )
        objective_marker = b"STS-SAME-ROOT-WIN-FIRST-PPO-CLIP-TYPED-FALLBACK-AXES\x00"
        objective_encoding = struct.pack(
            ">IQBBBQddBdBd",
            _COMBAT_ALL_LOSS_PPO_OBJECTIVE_VERSION,
            objective_config.groups_per_update,
            int(objective_config.all_win_axis),
            int(objective_config.all_loss_axis),
            int(update.rule),
            update.epochs,
            update.clip_coefficient,
            update.entropy_coefficient,
            int(max_grad_norm is not None),
            0.0 if max_grad_norm is None else max_grad_norm,
            int(target_kl is not None),
            0.0 if target_kl is None else target_kl,
        )
    else:
        implementation_version = _COMBAT_PPO_TRAINER_IMPLEMENTATION_VERSION
        objective_marker = b"STS-SAME-ROOT-WIN-FIRST-PPO-CLIP\x00"
        objective_encoding = struct.pack(
            ">IQBBQddBdBd",
            _COMBAT_PPO_OBJECTIVE_VERSION,
            objective_config.groups_per_update,
            int(objective_config.all_win_axis),
            int(update.rule),
            update.epochs,
            update.clip_coefficient,
            update.entropy_coefficient,
            int(max_grad_norm is not None),
            0.0 if max_grad_norm is None else max_grad_norm,
            int(target_kl is not None),
            0.0 if target_kl is None else target_kl,
        )
    return ManifestArtifactId.from_content(
        ManifestArtifactKind.TRAINER_IMPLEMENTATION,
        b"STS-SYNCHRONOUS-COMBAT-PPO-CLIP-POLICY-TRAINER\x00"
        + struct.pack(">I", implementation_version)
        + objective_marker
        + objective_encoding
        + _runtime_version_bytes(),
    )


def _canonical_schema_bytes(schema: Mapping[str, object]) -> bytes:
    if not isinstance(schema, Mapping):
        raise TorchProvenanceError("semantic schema must be a mapping")
    payload = _encode_schema_value(schema, depth=0)
    if len(payload) > _MAX_SCHEMA_BYTES:
        raise TorchProvenanceError("semantic schema exceeds its byte limit")
    return payload


def _encode_schema_value(value: object, *, depth: int) -> bytes:
    if depth > _MAX_SCHEMA_DEPTH:
        raise TorchProvenanceError("semantic schema exceeds its nesting limit")
    if type(value) is int:
        try:
            normalized = operator.index(value)
            return b"I" + struct.pack(">q", normalized)
        except (TypeError, struct.error) as error:
            raise TorchProvenanceError(
                "semantic schema integer is outside signed 64-bit range"
            ) from error
    if type(value) is str:
        encoded = value.encode("utf-8")
        if len(encoded) > _MAX_SCHEMA_BYTES:
            raise TorchProvenanceError("semantic schema string is too large")
        return b"S" + struct.pack(">I", len(encoded)) + encoded
    if isinstance(value, Mapping):
        if len(value) > _MAX_SCHEMA_ITEMS:
            raise TorchProvenanceError("semantic schema has too many entries")
        rows: list[tuple[bytes, bytes]] = []
        encoded_bytes = 5
        for key, item in value.items():
            if type(key) not in (int, str):
                raise TorchProvenanceError(
                    "semantic schema keys must be integers or strings"
                )
            encoded_key = _encode_schema_value(key, depth=depth + 1)
            encoded_value = _encode_schema_value(item, depth=depth + 1)
            encoded_bytes += len(encoded_key) + len(encoded_value)
            if encoded_bytes > _MAX_SCHEMA_BYTES:
                raise TorchProvenanceError(
                    "semantic schema exceeds its byte limit"
                )
            rows.append((encoded_key, encoded_value))
        rows.sort(key=lambda row: row[0])
        return (
            b"M"
            + struct.pack(">I", len(rows))
            + b"".join(key + item for key, item in rows)
        )
    raise TorchProvenanceError(
        f"semantic schema contains unsupported {type(value).__name__} value"
    )


def _runtime_version_bytes() -> bytes:
    encoded = str(torch.__version__).encode("utf-8")
    if len(encoded) > 255:
        raise TorchProvenanceError("PyTorch version string is too large")
    return struct.pack(">B", len(encoded)) + encoded


def _finite_float(value: object, name: str) -> float:
    if isinstance(value, bool):
        raise TorchProvenanceError(f"{name} must be a real number")
    try:
        normalized = float(value)
    except (TypeError, ValueError) as error:
        raise TorchProvenanceError(f"{name} must be a real number") from error
    if not math.isfinite(normalized):
        raise TorchProvenanceError(f"{name} must be finite")
    return normalized
