"""Optional PyTorch policy components over the bridge-owned semantic graph.

This module is deliberately not imported by :mod:`sts_learning`.  Importing the
ordinary caller package therefore continues to require only NumPy; callers that
want this scorer opt into PyTorch and import this module explicitly.
"""

from __future__ import annotations

import math
import operator
import struct
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from numbers import Real

import numpy as np
import torch
from torch import Tensor, nn

from .manifests import (
    BehaviorRuleBinding,
    ManifestArtifactId,
    ManifestArtifactKind,
)
from .policy import (
    BatchPolicyChoice,
    BehaviorManifestId,
    SelectionProbability,
)


class TorchPolicyError(ValueError):
    """A schema, semantic batch, or ragged target violated the model contract."""


@dataclass(frozen=True)
class SemanticSchemaDimensions:
    """Numeric model dimensions derived from one bridge ``semantic_schema``."""

    version: int
    token_kind_size: int
    categorical_field_size: int
    scalar_field_size: int
    relation_kind_size: int
    valid_token_kinds: tuple[bool, ...]
    valid_categorical_fields: tuple[bool, ...]
    valid_scalar_fields: tuple[bool, ...]
    valid_relation_kinds: tuple[bool, ...]
    categorical_offsets: tuple[int, ...]
    categorical_vocabulary_sizes: tuple[int, ...]
    categorical_vocabulary_size: int
    identity_residual_categorical_fields: tuple[int, ...]

    @classmethod
    def from_bridge_schema(
        cls,
        schema: Mapping[str, object],
    ) -> SemanticSchemaDimensions:
        """Validate and compact dimensions without copying enum names."""

        version = _non_negative_integer(_required(schema, "version"), "schema version")
        token_kind_size, valid_token_kinds = _enum_dimension(schema, "token_kind")
        categorical_field_size, valid_categorical_fields = _enum_dimension(
            schema,
            "categorical_field",
        )
        scalar_field_size, valid_scalar_fields = _enum_dimension(schema, "scalar_field")
        relation_kind_size, valid_relation_kinds = _enum_dimension(
            schema,
            "relation_kind",
        )

        raw_sizes = _mapping(
            _required(schema, "categorical_vocabulary_size"),
            "categorical_vocabulary_size",
        )
        sizes_by_field: dict[int, int] = {}
        for raw_field, raw_size in raw_sizes.items():
            field = _non_negative_integer(raw_field, "categorical vocabulary field")
            size = _positive_integer(raw_size, f"categorical field {field} vocabulary")
            if field in sizes_by_field:
                raise TorchPolicyError(
                    f"categorical vocabulary repeats numeric field {field}"
                )
            sizes_by_field[field] = size
        expected_fields = {
            field for field, valid in enumerate(valid_categorical_fields) if valid
        }
        if set(sizes_by_field) != expected_fields:
            raise TorchPolicyError(
                "categorical vocabulary fields do not match categorical_field ids"
            )

        raw_identity_fields = _mapping(
            _required(schema, "identity_residual_categorical_fields"),
            "identity_residual_categorical_fields",
        )
        if any(
            type(enabled) is not int or enabled != 1
            for enabled in raw_identity_fields.values()
        ):
            raise TorchPolicyError(
                "identity_residual_categorical_fields values must be integer one"
            )
        identity_fields = tuple(
            _non_negative_integer(field, "identity residual categorical field")
            for field in raw_identity_fields.keys()
        )
        if len(set(identity_fields)) != len(identity_fields):
            raise TorchPolicyError(
                "identity_residual_categorical_fields repeats a field"
            )
        if not set(identity_fields).issubset(expected_fields):
            raise TorchPolicyError(
                "identity_residual_categorical_fields contains an unknown field"
            )

        offsets: list[int] = []
        sizes: list[int] = []
        next_offset = 0
        for field in range(categorical_field_size):
            offsets.append(next_offset)
            size = sizes_by_field.get(field, 0)
            sizes.append(size)
            next_offset += size

        return cls(
            version=version,
            token_kind_size=token_kind_size,
            categorical_field_size=categorical_field_size,
            scalar_field_size=scalar_field_size,
            relation_kind_size=relation_kind_size,
            valid_token_kinds=valid_token_kinds,
            valid_categorical_fields=valid_categorical_fields,
            valid_scalar_fields=valid_scalar_fields,
            valid_relation_kinds=valid_relation_kinds,
            categorical_offsets=tuple(offsets),
            categorical_vocabulary_sizes=tuple(sizes),
            categorical_vocabulary_size=next_offset,
            identity_residual_categorical_fields=identity_fields,
        )


@dataclass(frozen=True)
class RaggedScorerConfig:
    """Small backend configuration; it contains no game feature dictionary."""

    hidden_dim: int = 64
    relation_layers: int = 2
    value_head: bool = False
    value_head_width: int = 1
    family_field: int | None = None
    # Missing in old contracts means vector-max, preserving fitted semantics.
    family_pooling: str = "vector-max"
    # Candidate queries read shared observation tokens only. Legacy contracts
    # retain mean pooling and have no attention parameters.
    observation_pooling: str = "mean"

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "hidden_dim",
            _positive_integer(self.hidden_dim, "hidden_dim"),
        )
        object.__setattr__(
            self,
            "relation_layers",
            _non_negative_integer(self.relation_layers, "relation_layers"),
        )
        if type(self.value_head) is not bool:
            raise TorchPolicyError("value_head must be bool")
        object.__setattr__(
            self,
            "value_head_width",
            _positive_integer(self.value_head_width, "value_head_width"),
        )
        if self.value_head_width > 64:
            raise TorchPolicyError("value_head_width must not exceed 64")
        if not self.value_head and self.value_head_width != 1:
            raise TorchPolicyError(
                "value_head_width must be one when the value head is disabled"
            )
        if self.family_field is not None:
            object.__setattr__(self, "family_field",
                               _non_negative_integer(self.family_field, "family_field"))
        if (not isinstance(self.family_pooling, str) or
                self.family_pooling not in {"vector-max", "candidate-score-max"}):
            raise TorchPolicyError("family_pooling must be vector-max or candidate-score-max")
        if self.family_field is None and self.family_pooling != "vector-max":
            raise TorchPolicyError("candidate-score-max requires family_field")
        if (not isinstance(self.observation_pooling, str) or
                self.observation_pooling not in {"mean", "candidate-attention"}):
            raise TorchPolicyError("observation_pooling must be mean or candidate-attention")
        if self.observation_pooling != "mean" and self.family_field is not None:
            raise TorchPolicyError("candidate-attention currently requires flat scoring")


@dataclass(frozen=True)
class RaggedFamilyLogits:
    """Separate family scores and within-family target scores, without labels."""

    values: Tensor
    row_splits: Tensor
    candidate_families: Tensor
    target_values: Tensor


@dataclass(frozen=True)
class RaggedCandidateLogits:
    """Flat candidate logits with their unchanged decision-row boundaries."""

    values: Tensor
    row_splits: Tensor
    families: RaggedFamilyLogits | None = None

    def __post_init__(self) -> None:
        if self.values.ndim != 1:
            raise TorchPolicyError("candidate logits must be one-dimensional")
        if self.row_splits.ndim != 1 or self.row_splits.numel() == 0:
            raise TorchPolicyError("candidate row_splits must be a non-empty vector")
        if self.row_splits.dtype is not torch.long:
            raise TorchPolicyError("candidate row_splits must use torch.long")
        if int(self.row_splits[0].item()) != 0:
            raise TorchPolicyError("candidate row_splits must start at zero")
        if int(self.row_splits[-1].item()) != self.values.numel():
            raise TorchPolicyError("candidate row_splits do not end at logits length")
        if self.families is not None:
            families = self.families
            if not isinstance(families, RaggedFamilyLogits):
                raise TorchPolicyError("families must be typed ragged family logits")
            if families.values.ndim != 1 or families.target_values.shape != self.values.shape:
                raise TorchPolicyError("family/target logits must be aligned vectors")
            if families.candidate_families.shape != self.values.shape:
                raise TorchPolicyError("candidate families must align to candidates")
            if any(t.device != self.values.device for t in (
                    families.values, families.row_splits, families.target_values, families.candidate_families)):
                raise TorchPolicyError("family logits must share the candidate device")
            if families.row_splits.dtype is not torch.long or families.candidate_families.dtype is not torch.long:
                raise TorchPolicyError("family indices must use torch.long")
            _validate_splits(families.row_splits, self.row_count, families.values.numel(),
                             "family", require_non_empty=True)
            _validate_range(families.candidate_families, families.values.numel(), "candidate family")
            if not torch.equal(_row_ids(families.row_splits)[families.candidate_families],
                               _row_ids(self.row_splits)):
                raise TorchPolicyError("a candidate family escapes its decision row")
            if torch.bincount(families.candidate_families, minlength=families.values.numel()).eq(0).any():
                raise TorchPolicyError("every family must contain a candidate")

    @property
    def row_count(self) -> int:
        return self.row_splits.numel() - 1

    def greedy_ordinals(self) -> list[int]:
        """Return observation-local argmax ordinals for a driver policy call."""

        ordinals: list[int] = []
        splits = self.row_splits.detach().cpu().tolist()
        for start, end in zip(splits[:-1], splits[1:], strict=True):
            if start == end:
                raise TorchPolicyError("cannot choose from an empty candidate row")
            ordinal = int(torch.argmax(self.values[start:end]).item())
            ordinals.append(ordinal)
        return ordinals


def family_target_greedy_logits(
    families: RaggedFamilyLogits, row_splits: Tensor,
) -> RaggedCandidateLogits:
    """Keep every legal target while making full argmax choose family then target."""
    RaggedCandidateLogits(families.target_values, row_splits, families)
    maximum = torch.full_like(families.values, -torch.inf).scatter_reduce(
        0, families.candidate_families, families.target_values, reduce="amax", include_self=True)
    relative_targets = families.target_values - maximum[families.candidate_families]
    values = families.values[families.candidate_families] + relative_targets
    return RaggedCandidateLogits(values, row_splits, families)


@dataclass(frozen=True)
class RaggedActorCriticOutput:
    """Candidate policy logits and one scalar value per decision row."""

    logits: RaggedCandidateLogits
    row_values: Tensor

    def __post_init__(self) -> None:
        if not isinstance(self.logits, RaggedCandidateLogits):
            raise TorchPolicyError("actor-critic logits must be ragged")
        if self.row_values.ndim != 1:
            raise TorchPolicyError("actor-critic values must be one-dimensional")
        if self.row_values.numel() != self.logits.row_count:
            raise TorchPolicyError("actor-critic values must align to decision rows")


@dataclass(frozen=True)
class RaggedMultiActorCriticOutput:
    """Candidate logits and fixed-semantic value columns per decision row."""

    logits: RaggedCandidateLogits
    row_values: Tensor

    def __post_init__(self) -> None:
        if not isinstance(self.logits, RaggedCandidateLogits):
            raise TorchPolicyError("multi actor-critic logits must be ragged")
        if self.row_values.ndim != 2 or self.row_values.shape[1] <= 0:
            raise TorchPolicyError(
                "multi actor-critic values must be a non-empty matrix"
            )
        if self.row_values.shape[0] != self.logits.row_count:
            raise TorchPolicyError(
                "multi actor-critic values must align to decision rows"
            )


_RAGGED_CATEGORICAL_RULE_V1 = ManifestArtifactId.from_content(
    ManifestArtifactKind.BEHAVIOR_RULE,
    b"sts_learning.ragged_categorical_inverse_cdf\x00v1",
)


@dataclass(frozen=True)
class RaggedCategoricalPolicyConfig:
    """Temperature-scaled categorical behavior with canonical provenance."""

    temperature: float = 1.0
    canonical_row_scoring: bool = False

    def __post_init__(self) -> None:
        if type(self.canonical_row_scoring) is not bool:
            raise TorchPolicyError("canonical_row_scoring must be bool")
        if isinstance(self.temperature, bool) or not isinstance(self.temperature, Real):
            raise TorchPolicyError("categorical temperature must be a real number")
        normalized = float(self.temperature)
        if not math.isfinite(normalized) or normalized <= 0.0:
            raise TorchPolicyError("categorical temperature must be finite and positive")
        object.__setattr__(self, "temperature", normalized)

    @property
    def behavior_rule(self) -> BehaviorRuleBinding:
        payload = b"sts_learning.ragged_categorical_temperature\x00v1" + struct.pack(
            ">d",
            self.temperature,
        )
        if self.canonical_row_scoring:
            payload += b"SINGLE-ROW-SCORING-V1\x00"
        return BehaviorRuleBinding(
            implementation=_RAGGED_CATEGORICAL_RULE_V1,
            configuration=ManifestArtifactId.from_content(
                ManifestArtifactKind.BEHAVIOR_RULE_CONFIG,
                payload,
            ),
        )


@dataclass(frozen=True)
class RaggedCategoricalSample:
    """Aligned sampled ordinals and their probabilities at selection time."""

    ordinals: tuple[int, ...]
    selection_probabilities: tuple[SelectionProbability, ...]


def sample_ragged_categorical(
    logits: RaggedCandidateLogits,
    config: RaggedCategoricalPolicyConfig,
    generator: torch.Generator,
) -> RaggedCategoricalSample:
    """Sample every legal ragged row from one caller-owned random stream."""

    if not isinstance(logits, RaggedCandidateLogits):
        raise TorchPolicyError("categorical sampling requires RaggedCandidateLogits")
    if not isinstance(config, RaggedCategoricalPolicyConfig):
        raise TorchPolicyError("categorical sampling requires typed config")
    if not isinstance(generator, torch.Generator):
        raise TorchPolicyError("categorical sampling requires a caller-owned generator")
    if generator is torch.default_generator:
        raise TorchPolicyError("categorical sampling refuses the global generator")
    generator_device = torch.device(generator.device)
    if generator_device.type != logits.values.device.type:
        raise TorchPolicyError(
            "categorical generator device type must match candidate logits"
        )
    probability_rows = categorical_probability_rows(logits, config)

    uniforms = torch.rand(
        (len(probability_rows),),
        dtype=torch.float64,
        device=logits.values.device,
        generator=generator,
    )
    return _sample_probability_rows(probability_rows, uniforms)


def sample_ragged_categorical_rows(
    logits: RaggedCandidateLogits,
    config: RaggedCategoricalPolicyConfig,
    generators: Sequence[torch.Generator],
) -> RaggedCategoricalSample:
    """Sample each ragged row from its own caller-owned random stream."""

    if not isinstance(logits, RaggedCandidateLogits):
        raise TorchPolicyError("categorical sampling requires RaggedCandidateLogits")
    if not isinstance(config, RaggedCategoricalPolicyConfig):
        raise TorchPolicyError("categorical sampling requires typed config")
    row_generators = tuple(generators)
    if len(row_generators) != logits.row_count:
        raise TorchPolicyError(
            "categorical row generators must align to decision rows"
        )
    if not all(isinstance(generator, torch.Generator) for generator in row_generators):
        raise TorchPolicyError(
            "categorical row sampling requires caller-owned generators"
        )
    if any(generator is torch.default_generator for generator in row_generators):
        raise TorchPolicyError("categorical sampling refuses the global generator")
    if len({id(generator) for generator in row_generators}) != len(row_generators):
        raise TorchPolicyError(
            "categorical row sampling requires independent generators"
        )
    logits_device = logits.values.device.type
    if any(
        torch.device(generator.device).type != logits_device
        for generator in row_generators
    ):
        raise TorchPolicyError(
            "categorical generator device type must match candidate logits"
        )

    probability_rows = categorical_probability_rows(logits, config)
    uniforms = tuple(
        torch.rand(
            (),
            dtype=torch.float64,
            device=logits.values.device,
            generator=generator,
        )
        for generator in row_generators
    )
    return _sample_probability_rows(probability_rows, uniforms)


def categorical_probability_rows(
    logits: RaggedCandidateLogits,
    config: RaggedCategoricalPolicyConfig,
) -> tuple[Tensor, ...]:
    """Return each legal row's actual sampling distribution without drawing RNG."""
    if not isinstance(logits, RaggedCandidateLogits):
        raise TorchPolicyError("categorical probabilities require RaggedCandidateLogits")
    if not isinstance(config, RaggedCategoricalPolicyConfig):
        raise TorchPolicyError("categorical probabilities require typed config")
    if not bool(torch.all(torch.isfinite(logits.values))):
        raise TorchPolicyError("categorical candidate logits must be finite")

    splits = logits.row_splits.detach().cpu().tolist()
    probability_rows: list[Tensor] = []
    for start, end in zip(splits[:-1], splits[1:], strict=True):
        if start >= end:
            raise TorchPolicyError(
                "categorical sampling requires non-empty increasing rows"
            )
        row = logits.values[start:end].detach().to(dtype=torch.float64)
        probabilities = torch.softmax(row / config.temperature, dim=0)
        if not bool(torch.all(torch.isfinite(probabilities))):
            raise TorchPolicyError("categorical probabilities must be finite")
        if not bool(torch.any(probabilities > 0.0)):
            raise TorchPolicyError("categorical row has no positive probability")
        probability_rows.append(probabilities)
    return tuple(probability_rows)


def _sample_probability_rows(
    probability_rows: Sequence[Tensor],
    uniforms: Sequence[Tensor] | Tensor,
) -> RaggedCategoricalSample:
    ordinals: list[int] = []
    selected_probabilities: list[SelectionProbability] = []
    for probabilities, uniform in zip(
        probability_rows,
        uniforms,
        strict=True,
    ):
        cumulative = torch.cumsum(probabilities, dim=0)
        cumulative[-1] = 1.0
        ordinal = int(torch.searchsorted(cumulative, uniform, right=True).item())
        probability = float(probabilities[ordinal].item())
        ordinals.append(ordinal)
        selected_probabilities.append(SelectionProbability.known(probability))
    return RaggedCategoricalSample(
        ordinals=tuple(ordinals),
        selection_probabilities=tuple(selected_probabilities),
    )


class _RelationLayer(nn.Module):
    def __init__(self, hidden_dim: int, relation_kind_size: int) -> None:
        super().__init__()
        self.relation = nn.Embedding(relation_kind_size, hidden_dim)
        self.outgoing = nn.Linear(hidden_dim, hidden_dim, bias=False)
        self.incoming = nn.Linear(hidden_dim, hidden_dim, bias=False)
        self.project = nn.Linear(hidden_dim, hidden_dim)
        self.norm = nn.LayerNorm(hidden_dim)

    def forward(
        self,
        state: Tensor,
        source: Tensor,
        relation: Tensor,
        target: Tensor,
        scopes: Tensor,
    ) -> Tensor:
        if source.numel() == 0:
            return state
        relation_state = self.relation(relation)
        same_scope = scopes[source] == scopes[target]
        outgoing = same_scope | (scopes[target] == 0)
        incoming = same_scope | (scopes[source] == 0)
        messages = torch.zeros_like(state)
        messages.index_add_(
            0,
            source[outgoing],
            self.outgoing(state[target[outgoing]]) + relation_state[outgoing],
        )
        messages.index_add_(
            0,
            target[incoming],
            self.incoming(state[source[incoming]]) + relation_state[incoming],
        )
        degree = torch.zeros(
            state.shape[0],
            dtype=state.dtype,
            device=state.device,
        )
        ones = torch.ones(source.shape[0], dtype=state.dtype, device=state.device)
        degree.index_add_(0, source[outgoing], ones[outgoing])
        degree.index_add_(0, target[incoming], ones[incoming])
        messages = messages / degree.clamp_min(1.0).sqrt().unsqueeze(1)
        updated = self.norm(state + torch.nn.functional.silu(self.project(messages)))
        return torch.where((degree > 0).unsqueeze(1), updated, state)


class RaggedCandidateScorer(nn.Module):
    """Score every candidate in one sparse, ragged semantic graph batch."""

    def __init__(
        self,
        schema: SemanticSchemaDimensions,
        config: RaggedScorerConfig | None = None,
    ) -> None:
        super().__init__()
        self.schema = schema
        self.config = config or RaggedScorerConfig()
        hidden_dim = self.config.hidden_dim

        self.token_kind = nn.Embedding(schema.token_kind_size, hidden_dim)
        self.categorical_value = nn.Embedding(
            schema.categorical_vocabulary_size,
            hidden_dim,
        )
        with torch.no_grad():
            for field in schema.identity_residual_categorical_fields:
                start = schema.categorical_offsets[field]
                end = start + schema.categorical_vocabulary_sizes[field]
                self.categorical_value.weight[start:end].zero_()
        self.scalar_bias = nn.Embedding(schema.scalar_field_size, hidden_dim)
        self.scalar_weight = nn.Embedding(schema.scalar_field_size, hidden_dim)
        # A field observed only at zero cannot train its slope. Keep that
        # unused numerical direction neutral when a nonzero value first appears.
        nn.init.zeros_(self.scalar_weight.weight)
        self.relation_layers = nn.ModuleList(
            _RelationLayer(hidden_dim, schema.relation_kind_size)
            for _ in range(self.config.relation_layers)
        )
        self.scorer = nn.Sequential(
            nn.Linear(hidden_dim * 2, hidden_dim),
            nn.SiLU(),
            nn.Linear(hidden_dim, 1),
        )
        self.observation_query = self.observation_key = None
        if self.config.observation_pooling == "candidate-attention":
            self.observation_query = nn.Linear(hidden_dim, hidden_dim, bias=False)
            self.observation_key = nn.Linear(hidden_dim, hidden_dim, bias=False)
        self.family_head = None
        if self.config.family_field is not None:
            field = self.config.family_field
            if field >= schema.categorical_field_size or not schema.valid_categorical_fields[field]:
                raise TorchPolicyError("family_field is outside the categorical schema")
            self.family_head = nn.Sequential(
                nn.Linear(hidden_dim * 2, hidden_dim), nn.SiLU(), nn.Linear(hidden_dim, 1))
        self.value_head = None
        if self.config.value_head:
            self.value_head = nn.Sequential(
                nn.Linear(hidden_dim, hidden_dim),
                nn.SiLU(),
                nn.Linear(hidden_dim, self.config.value_head_width),
            )
            final = self.value_head[-1]
            assert isinstance(final, nn.Linear)
            nn.init.zeros_(final.weight)
            nn.init.zeros_(final.bias)
        self.register_buffer(
            "_categorical_offsets",
            torch.tensor(schema.categorical_offsets, dtype=torch.long),
            persistent=False,
        )
        self.register_buffer(
            "_categorical_sizes",
            torch.tensor(schema.categorical_vocabulary_sizes, dtype=torch.long),
            persistent=False,
        )
        for name, values in (
            ("_valid_token_kinds", schema.valid_token_kinds),
            ("_valid_categorical_fields", schema.valid_categorical_fields),
            ("_valid_scalar_fields", schema.valid_scalar_fields),
            ("_valid_relation_kinds", schema.valid_relation_kinds),
        ):
            self.register_buffer(
                name,
                torch.tensor(values, dtype=torch.bool),
                persistent=False,
            )

    @classmethod
    def from_bridge_schema(
        cls,
        schema: Mapping[str, object],
        config: RaggedScorerConfig | None = None,
    ) -> RaggedCandidateScorer:
        return cls(SemanticSchemaDimensions.from_bridge_schema(schema), config)

    def forward(self, decision_batch: Mapping[str, object]) -> RaggedCandidateLogits:
        logits, _ = self._forward_encoded(decision_batch)
        return logits

    def actor_critic(
        self,
        decision_batch: Mapping[str, object],
    ) -> RaggedActorCriticOutput:
        if self.value_head is None:
            raise TorchPolicyError("actor-critic output requires a value head")
        if self.config.value_head_width != 1:
            raise TorchPolicyError(
                "scalar actor-critic output requires value_head_width one"
            )
        logits, row_state = self._forward_encoded(decision_batch)
        return RaggedActorCriticOutput(
            logits=logits,
            row_values=self.value_head(row_state).squeeze(1),
        )

    def actor_critic_multi(
        self,
        decision_batch: Mapping[str, object],
    ) -> RaggedMultiActorCriticOutput:
        if self.value_head is None:
            raise TorchPolicyError("multi actor-critic output requires a value head")
        logits, row_state = self._forward_encoded(decision_batch)
        return RaggedMultiActorCriticOutput(
            logits=logits,
            row_values=self.value_head(row_state),
        )

    def _forward_encoded(
        self,
        decision_batch: Mapping[str, object],
    ) -> tuple[RaggedCandidateLogits, Tensor]:
        device = self.token_kind.weight.device
        semantic = _mapping(_required(decision_batch, "semantic"), "semantic")
        batch_version = _non_negative_integer(
            _required(semantic, "schema_version"),
            "semantic schema_version",
        )
        if batch_version != self.schema.version:
            raise TorchPolicyError(
                f"semantic schema version {batch_version} does not match model "
                f"version {self.schema.version}"
            )

        slot_indices = _index_tensor(
            _required(decision_batch, "slot_indices"),
            device,
            "slot_indices",
        )
        row_count = slot_indices.numel()
        token = _mapping(_required(semantic, "token"), "semantic.token")
        token_kind = _index_tensor(
            _required(token, "kind"),
            device,
            "semantic.token.kind",
        )
        scopes = _index_tensor(_required(token, "scope"), device, "token scope")
        if scopes.shape != token_kind.shape:
            raise TorchPolicyError("token scopes must align to token kinds")
        token_splits = _index_tensor(
            _required(token, "row_splits"),
            device,
            "semantic.token.row_splits",
        )
        candidate_splits = _index_tensor(
            _required(decision_batch, "candidate_row_splits"),
            device,
            "candidate_row_splits",
        )
        candidate_tokens = _index_tensor(
            _required(semantic, "candidate_token_indices"),
            device,
            "semantic.candidate_token_indices",
        )
        _validate_splits(token_splits, row_count, token_kind.numel(), "token")
        _validate_splits(
            candidate_splits,
            row_count,
            candidate_tokens.numel(),
            "candidate",
            require_non_empty=True,
        )
        _validate_schema_ids(token_kind, self._valid_token_kinds, "token kind")
        _validate_range(candidate_tokens, token_kind.numel(), "candidate token")

        candidate_counts = _index_tensor(
            _required(decision_batch, "candidate_counts"),
            device,
            "candidate_counts",
        )
        if candidate_counts.shape != (row_count,) or not torch.equal(
            candidate_counts,
            candidate_splits[1:] - candidate_splits[:-1],
        ):
            raise TorchPolicyError("candidate_counts disagree with candidate_row_splits")

        token_row_ids = _row_ids(token_splits)
        candidate_row_ids = _row_ids(candidate_splits)
        if not torch.equal(token_row_ids[candidate_tokens], candidate_row_ids):
            raise TorchPolicyError("a candidate token escapes its decision row")
        if (bool(torch.any(scopes < 0))
                or bool(torch.any(scopes > candidate_counts[token_row_ids]))):
            raise TorchPolicyError("candidate scope is outside its decision row")
        # Scope IDs are row-local topology and are never embedded as features.
        scope_offsets = candidate_splits[:-1] + torch.arange(row_count, device=device)
        groups = scope_offsets[token_row_ids] + scopes
        candidate_groups = groups[candidate_tokens]
        if (bool(torch.any(scopes[candidate_tokens] == 0))
                or torch.unique(candidate_groups).numel() != candidate_tokens.numel()):
            raise TorchPolicyError("each candidate must own one distinct positive scope")
        shared = scopes == 0
        shared_counts = torch.bincount(token_row_ids[shared], minlength=row_count)
        if bool(torch.any(shared_counts == 0)):
            raise TorchPolicyError("each decision row requires shared observation tokens")
        owners = torch.full((candidate_tokens.numel() + row_count,), -1,
                            dtype=torch.long, device=device)
        owners[candidate_groups] = torch.arange(candidate_tokens.numel(), device=device)
        token_owners = owners[groups]
        if bool(torch.any(token_owners[~shared] < 0)):
            raise TorchPolicyError("candidate payload has no scope owner")

        state = self.token_kind(token_kind)
        state = state + self._categorical_state(semantic, token_kind.numel(), device)
        state = state + self._scalar_state(semantic, token_kind.numel(), device)

        relation_table = _mapping(
            _required(semantic, "relation"),
            "semantic.relation",
        )
        source = _index_tensor(
            _required(relation_table, "source_token_indices"),
            device,
            "semantic.relation.source_token_indices",
        )
        relation = _index_tensor(
            _required(relation_table, "relation"),
            device,
            "semantic.relation.relation",
        )
        target = _index_tensor(
            _required(relation_table, "target_token_indices"),
            device,
            "semantic.relation.target_token_indices",
        )
        _validate_aligned((source, relation, target), "relation")
        _validate_range(source, token_kind.numel(), "relation source token")
        _validate_range(target, token_kind.numel(), "relation target token")
        _validate_schema_ids(relation, self._valid_relation_kinds, "relation kind")
        if source.numel() and not torch.equal(token_row_ids[source], token_row_ids[target]):
            raise TorchPolicyError("a semantic relation escapes its decision row")
        if bool(torch.any((scopes[source] > 0) & (scopes[target] > 0)
                          & (scopes[source] != scopes[target]))):
            raise TorchPolicyError("a semantic relation crosses candidate scopes")
        for layer in self.relation_layers:
            state = layer(state, source, relation, target, scopes)

        row_sum = torch.zeros(
            (row_count, self.config.hidden_dim),
            dtype=state.dtype,
            device=device,
        )
        row_sum.index_add_(0, token_row_ids[shared], state[shared])
        row_state = row_sum / shared_counts.to(state.dtype).unsqueeze(1)
        payload_sum = torch.zeros((candidate_tokens.numel(), self.config.hidden_dim),
                                  dtype=state.dtype, device=device)
        payload_sum.index_add_(0, token_owners[~shared], state[~shared])
        payload_counts = torch.bincount(token_owners[~shared], minlength=candidate_tokens.numel())
        candidate_state = state[candidate_tokens] + payload_sum / payload_counts.to(state.dtype).unsqueeze(1)
        candidate_observation = row_state[candidate_row_ids]
        if self.observation_query is not None:
            # The query may contain this candidate's preview, but keys and
            # values are current observations in the same row. No candidate
            # payload enters another candidate or the value-head summary.
            candidate_observation = candidate_observation + self._attend_observation(
                candidate_state, state[shared], token_row_ids[shared],
                candidate_counts, candidate_splits)
        score_inputs = torch.cat(
            (candidate_state, candidate_observation),
            dim=1,
        )
        # Canonical token order fixes both floating-point pooling order and
        # position-dependent batched linear kernels. Indices are not features;
        # scores are returned in the caller's original candidate order.
        scoring_order = torch.argsort(candidate_tokens) if self.family_head is not None else None
        values = self.scorer(score_inputs if scoring_order is None else score_inputs[scoring_order]).squeeze(1)
        if scoring_order is not None:
            values = values[torch.argsort(scoring_order)]
        families = None
        if self.family_head is not None:
            table = _mapping(_required(semantic, "categorical"), "semantic.categorical")
            fields = _index_tensor(table["field"], device, "categorical field")
            indices = _index_tensor(table["token_indices"], device, "categorical token")
            feature_values = _index_tensor(table["value"], device, "categorical value")
            selected = fields == self.config.family_field
            counts = torch.zeros(token_kind.numel(), dtype=torch.long, device=device)
            counts.index_add_(0, indices[selected], torch.ones_like(indices[selected]))
            if not torch.all(counts[candidate_tokens] == 1):
                raise TorchPolicyError("each candidate requires exactly one family_field value")
            token_families = torch.zeros_like(counts)
            token_families[indices[selected]] = feature_values[selected]
            width = self.schema.categorical_vocabulary_sizes[self.config.family_field]
            keys, candidate_families = torch.unique(
                candidate_row_ids * width + token_families[candidate_tokens],
                sorted=True, return_inverse=True)
            family_rows = keys // width
            family_count = keys.numel()
            if self.config.family_pooling == "candidate-score-max":
                # Score real representations before reduction: favorable
                # coordinates from different targets cannot form a new target.
                candidate_family_values = self.family_head(score_inputs[scoring_order]).squeeze(1)
                candidate_family_values = candidate_family_values[torch.argsort(scoring_order)]
                family_values = torch.full((family_count,), -torch.inf,
                                           dtype=state.dtype, device=device).scatter_reduce(
                    0, candidate_families, candidate_family_values, reduce="amax", include_self=True)
            else:
                # Preserve the original explicit research architecture.
                family_state = torch.full((family_count, self.config.hidden_dim), -torch.inf,
                                          dtype=state.dtype, device=device).scatter_reduce(
                    0, candidate_families.unsqueeze(1).expand_as(candidate_state),
                    candidate_state, reduce="amax", include_self=True)
                family_values = self.family_head(torch.cat((family_state, row_state[family_rows]), dim=1)).squeeze(1)
            family_splits = torch.cat((torch.zeros(1, dtype=torch.long, device=device),
                                      torch.bincount(family_rows, minlength=row_count).cumsum(0)))
            families = RaggedFamilyLogits(family_values, family_splits, candidate_families, values)
            # Each family's best target has score equal to its family score.
            # Full legal argmax chooses a family, then its best target. This
            # conversion uses no labels or target-count probability penalty.
            return family_target_greedy_logits(families, candidate_splits), row_state
        return (
            RaggedCandidateLogits(values=values, row_splits=candidate_splits, families=families),
            row_state,
        )

    def _attend_observation(self, candidates, shared_state, shared_rows,
                            candidate_counts, candidate_splits):
        """Segmented attention without a dense cross-row candidate/token mask."""
        assert self.observation_query is not None and self.observation_key is not None
        counts = candidate_counts[shared_rows]
        starts = counts.cumsum(0) - counts
        shared_indices = torch.repeat_interleave(
            torch.arange(shared_state.shape[0], device=shared_state.device), counts)
        relative = torch.arange(shared_indices.numel(), device=shared_state.device) - torch.repeat_interleave(starts, counts)
        candidate_indices = candidate_splits[shared_rows[shared_indices]] + relative
        query, key = self.observation_query(candidates), self.observation_key(shared_state)
        scores = (query[candidate_indices] * key[shared_indices]).sum(1) / math.sqrt(self.config.hidden_dim)
        maxima = torch.full((candidates.shape[0],), -torch.inf,
                            dtype=scores.dtype, device=scores.device).scatter_reduce(
            0, candidate_indices, scores, reduce="amax", include_self=True)
        weights = (scores - maxima[candidate_indices]).exp()
        denominators = torch.zeros_like(maxima).index_add(0, candidate_indices, weights)
        weights = weights / denominators[candidate_indices]
        result = torch.zeros_like(candidates)
        result.index_add_(0, candidate_indices, shared_state[shared_indices] * weights.unsqueeze(1))
        return result

    def _categorical_state(
        self,
        semantic: Mapping[str, object],
        token_count: int,
        device: torch.device,
    ) -> Tensor:
        table = _mapping(_required(semantic, "categorical"), "semantic.categorical")
        token_indices = _index_tensor(
            _required(table, "token_indices"),
            device,
            "semantic.categorical.token_indices",
        )
        field = _index_tensor(
            _required(table, "field"),
            device,
            "semantic.categorical.field",
        )
        value = _index_tensor(
            _required(table, "value"),
            device,
            "semantic.categorical.value",
        )
        _validate_aligned((token_indices, field, value), "categorical")
        _validate_range(token_indices, token_count, "categorical token")
        _validate_schema_ids(
            field,
            self._valid_categorical_fields,
            "categorical field",
        )
        if value.numel() and bool(torch.any(value >= self._categorical_sizes[field])):
            raise TorchPolicyError("categorical value exceeds its schema vocabulary")
        encoded = self.categorical_value(self._categorical_offsets[field] + value)
        return _mean_by_token(encoded, token_indices, token_count)

    def _scalar_state(
        self,
        semantic: Mapping[str, object],
        token_count: int,
        device: torch.device,
    ) -> Tensor:
        table = _mapping(_required(semantic, "scalar"), "semantic.scalar")
        token_indices = _index_tensor(
            _required(table, "token_indices"),
            device,
            "semantic.scalar.token_indices",
        )
        field = _index_tensor(
            _required(table, "field"),
            device,
            "semantic.scalar.field",
        )
        value = _float_tensor(
            _required(table, "value"),
            device,
            "semantic.scalar.value",
        )
        _validate_aligned((token_indices, field, value), "scalar")
        _validate_range(token_indices, token_count, "scalar token")
        _validate_schema_ids(field, self._valid_scalar_fields, "scalar field")
        if not bool(torch.all(torch.isfinite(value))):
            raise TorchPolicyError("scalar values must be finite")
        encoded = self.scalar_bias(field) + self.scalar_weight(field) * value.unsqueeze(1)
        return _mean_by_token(encoded, token_indices, token_count)


def load_scorer_warm_start(
    target: RaggedCandidateScorer,
    source: RaggedCandidateScorer,
    *,
    actor_only: bool = False,
) -> None:
    """Copy a compatible scorer, optionally resetting objective-local value state."""

    if not isinstance(target, RaggedCandidateScorer) or not isinstance(
        source,
        RaggedCandidateScorer,
    ):
        raise TorchPolicyError("scorer warm start requires typed scorers")
    if type(actor_only) is not bool:
        raise TorchPolicyError("scorer warm-start actor_only must be bool")
    if target.config.family_field != source.config.family_field:
        raise TorchPolicyError("warm-start family_field must match")
    if target.config.family_pooling != source.config.family_pooling:
        raise TorchPolicyError("warm-start family_pooling must match")
    if target.config.observation_pooling != source.config.observation_pooling:
        raise TorchPolicyError("warm-start observation_pooling must match")
    try:
        same_value_profile = (
            target.config.value_head == source.config.value_head
            and target.config.value_head_width == source.config.value_head_width
        )
        if not actor_only and same_value_profile:
            target.load_state_dict(source.state_dict(), strict=True)
            return
        actor_state = {
            key: value
            for key, value in source.state_dict().items()
            if not key.startswith("value_head.")
        }
        incompatible = target.load_state_dict(actor_state, strict=False)
        expected_missing = {
            key
            for key in target.state_dict()
            if key.startswith("value_head.")
        }
        if (
            incompatible.unexpected_keys
            or set(incompatible.missing_keys) != expected_missing
        ):
            raise RuntimeError("actor-only warm start changed shared policy keys")
    except RuntimeError as error:
        raise TorchPolicyError(
            "warm-start scorer is incompatible with the maintained profile"
        ) from error


def score_categorical_behavior(
    scorer: RaggedCandidateScorer,
    decision_batch: Mapping[str, object],
    config: RaggedCategoricalPolicyConfig,
) -> RaggedCandidateLogits:
    """Score each inference row in a reproducible float32 kernel shape.

    Sampling and native execution remain batched. Training may combine rows;
    its strict single-row propensity replay now reproduces this declared
    behavior independently of the original cohort's size.
    """
    if not config.canonical_row_scoring or len(decision_batch["slot_indices"]) == 1:
        return scorer(decision_batch)
    from .semantic_batch import select_semantic_decision_rows
    rows = [scorer(select_semantic_decision_rows(decision_batch, (row,)))
            for row in range(len(decision_batch["slot_indices"]))]
    values = torch.cat([row.values for row in rows])
    splits = torch.tensor([0, *np.cumsum([row.values.numel() for row in rows])],
                          dtype=torch.long, device=values.device)
    families = None
    if rows[0].families is not None:
        group = [row.families for row in rows]
        counts = [family.values.numel() for family in group]
        offsets = [0, *np.cumsum(counts)]
        families = RaggedFamilyLogits(
            torch.cat([family.values for family in group]),
            torch.tensor(offsets, dtype=torch.long, device=values.device),
            torch.cat([family.candidate_families + offset
                       for family, offset in zip(group, offsets[:-1], strict=True)]),
            torch.cat([family.target_values for family in group]),
        )
    return RaggedCandidateLogits(values, splits, families)


def configure_critic_only_training(scorer: RaggedCandidateScorer) -> None:
    """Freeze the complete actor path and leave only the scalar value head live."""

    if not isinstance(scorer, RaggedCandidateScorer):
        raise TorchPolicyError("critic-only training requires a typed scorer")
    if scorer.value_head is None or scorer.config.value_head_width != 1:
        raise TorchPolicyError("critic-only training requires one scalar value head")
    value_parameters = 0
    for name, parameter in scorer.named_parameters():
        is_value_parameter = name.startswith("value_head.")
        parameter.requires_grad_(is_value_parameter)
        value_parameters += int(is_value_parameter)
    if value_parameters == 0:
        raise TorchPolicyError("critic-only training found no value parameters")


def configure_target_only_training(scorer: RaggedCandidateScorer) -> None:
    """Refine targets while preserving the complete graph and family-score path."""
    if not isinstance(scorer, RaggedCandidateScorer) or scorer.family_head is None:
        raise TorchPolicyError("target-only training requires a family-target scorer")
    for name, parameter in scorer.named_parameters():
        parameter.requires_grad_(name.startswith("scorer."))


def configure_policy_heads_only_training(scorer: RaggedCandidateScorer) -> None:
    """Keep the encoded representation fixed while learning both policy heads."""
    if not isinstance(scorer, RaggedCandidateScorer) or scorer.family_head is None:
        raise TorchPolicyError("policy-head training requires a family-target scorer")
    for name, parameter in scorer.named_parameters():
        parameter.requires_grad_(name.startswith(("scorer.", "family_head.")))


def require_matching_actor_state(
    left: RaggedCandidateScorer,
    right: RaggedCandidateScorer,
) -> None:
    """Reject a critic initialization that changed any actor-owned tensor."""

    if not isinstance(left, RaggedCandidateScorer) or not isinstance(
        right,
        RaggedCandidateScorer,
    ):
        raise TorchPolicyError("actor-state comparison requires typed scorers")
    if (
        left.schema != right.schema
        or left.config.hidden_dim != right.config.hidden_dim
        or left.config.relation_layers != right.config.relation_layers
    ):
        raise TorchPolicyError("critic initialization changed the actor definition")
    left_state = {
        name: tensor
        for name, tensor in left.state_dict().items()
        if not name.startswith("value_head.")
    }
    right_state = {
        name: tensor
        for name, tensor in right.state_dict().items()
        if not name.startswith("value_head.")
    }
    if left_state.keys() != right_state.keys():
        raise TorchPolicyError("critic initialization changed actor state keys")
    for name in left_state:
        left_tensor = left_state[name].detach().cpu()
        right_tensor = right_state[name].detach().cpu()
        if not torch.equal(left_tensor, right_tensor):
            raise TorchPolicyError(
                f"critic initialization changed actor tensor {name!r}"
            )


class GreedyTorchPolicy:
    """Driver adapter that performs one batched scorer call per decision round."""

    def __init__(
        self,
        scorer: RaggedCandidateScorer,
        behavior_manifest_id: BehaviorManifestId,
    ) -> None:
        if not isinstance(behavior_manifest_id, BehaviorManifestId):
            raise TorchPolicyError("greedy policy requires a BehaviorManifestId")
        self.scorer = scorer
        self.behavior_manifest_id = behavior_manifest_id

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        with torch.inference_mode():
            ordinals = self.scorer(decision_batch).greedy_ordinals()
        return BatchPolicyChoice.deterministic(ordinals, self.behavior_manifest_id)


def ragged_cross_entropy(
    logits: RaggedCandidateLogits,
    target_ordinals: Sequence[int] | Tensor,
) -> Tensor:
    """Mean row-wise cross entropy without padding the candidate surface."""

    targets = torch.as_tensor(
        target_ordinals,
        dtype=torch.long,
        device=logits.values.device,
    )
    if targets.ndim != 1 or targets.numel() != logits.row_count:
        raise TorchPolicyError("target ordinals must contain one value per row")
    lengths = logits.row_splits[1:] - logits.row_splits[:-1]
    if bool(torch.any(targets < 0)) or bool(torch.any(targets >= lengths)):
        raise TorchPolicyError("target ordinal is outside its candidate row")

    row_ids = _row_ids(logits.row_splits)
    row_max = torch.full(
        (logits.row_count,),
        -torch.inf,
        dtype=logits.values.dtype,
        device=logits.values.device,
    )
    row_max.scatter_reduce_(0, row_ids, logits.values.detach(), reduce="amax")
    exponentials = torch.exp(logits.values - row_max[row_ids])
    row_sum = torch.zeros_like(row_max)
    row_sum.index_add_(0, row_ids, exponentials)
    log_normalizer = row_max + torch.log(row_sum)
    target_indices = logits.row_splits[:-1] + targets
    return (log_normalizer - logits.values[target_indices]).mean()


def _acceptable_set_rows(logits, acceptable_ordinals):
    """Shared validation for complete targets and derived family supervision."""
    if len(acceptable_ordinals) != logits.row_count:
        raise TorchPolicyError("acceptable sets must match candidate-logit rows")
    splits = logits.row_splits.detach().cpu().tolist()
    for row, (start, end, targets) in enumerate(zip(splits[:-1], splits[1:], acceptable_ordinals, strict=True)):
        if targets is None:
            continue
        if not isinstance(targets, (list, tuple)) or not targets:
            raise TorchPolicyError("acceptable set must be a nonempty sequence or None")
        if any(isinstance(value, (bool, np.bool_)) for value in targets):
            raise TorchPolicyError("acceptable ordinal must be an integer, not a boolean")
        ordinals = tuple(_non_negative_integer(value, "acceptable ordinal") for value in targets)
        if len(set(ordinals)) != len(ordinals):
            raise TorchPolicyError("acceptable set repeats a candidate ordinal")
        if any(value >= end - start for value in ordinals):
            raise TorchPolicyError(f"acceptable row {row} ordinal is outside its candidate row")
        yield row, start, end, ordinals


def ragged_acceptable_family_cross_entropy(logits, acceptable_ordinals):
    """Learn only ActionKind membership; within-family targets receive no loss.

    Unknown rows and rows accepting every offered family provide no family
    contrast and are excluded from the mean, rather than diluting its gradient.
    The complete candidate surface still determines every offered family.
    """
    if logits.families is None:
        raise TorchPolicyError("family-only supervision requires family logits")
    families, losses = logits.families, []
    for row, start, end, ordinals in _acceptable_set_rows(logits, acceptable_ordinals):
        group_start, group_end = families.row_splits[row:row + 2].tolist()
        accepted = torch.unique(families.candidate_families[start:end][list(ordinals)], sorted=True)
        if accepted.numel() == group_end - group_start:
            continue
        losses.append(torch.logsumexp(families.values[group_start:group_end], dim=0)
                      - torch.logsumexp(families.values[accepted], dim=0))
    if not losses:
        raise TorchPolicyError("family-only loss requires at least one informative row")
    return torch.stack(losses).mean()


def ragged_acceptable_set_cross_entropy(
    logits: RaggedCandidateLogits,
    acceptable_ordinals: Sequence[Sequence[int] | None],
) -> Tensor:
    """Fit a teacher's adoptable set against the whole legal candidate row.

    Each nonempty set is independently authored as a complete decision target.
    ``None`` excludes the entire row from supervision. The loss is
    logsumexp(all candidates) - logsumexp(acceptable candidates), averaged over
    supervised rows. With separate family logits it is family set loss plus
    mean target set loss inside each acceptable family. It does not rank
    acceptable alternatives within a family against each other. Unknown rows
    provide no family or target supervision.
    """
    losses: list[Tensor] = []
    for row, start, end, ordinals in _acceptable_set_rows(logits, acceptable_ordinals):
        values = logits.values[start:end]
        if logits.families is None:
            losses.append(torch.logsumexp(values, dim=0) - torch.logsumexp(values[list(ordinals)], dim=0))
        else:
            families = logits.families
            group_start, group_end = families.row_splits[row:row + 2].tolist()
            membership = families.candidate_families[start:end]
            accepted_groups = torch.unique(membership[list(ordinals)], sorted=True)
            family_loss = (torch.logsumexp(families.values[group_start:group_end], dim=0)
                           - torch.logsumexp(families.values[accepted_groups], dim=0))
            target_losses = []
            for group in accepted_groups.tolist():
                taught = [ordinal for ordinal in ordinals if int(membership[ordinal]) == group]
                target_values = families.target_values[start:end]
                target_losses.append(torch.logsumexp(target_values[membership == group], dim=0)
                                     - torch.logsumexp(target_values[taught], dim=0))
            losses.append(family_loss + torch.stack(target_losses).mean())
    if not losses:
        raise TorchPolicyError("acceptable-set loss requires at least one supervised row")
    return torch.stack(losses).mean()


def ragged_resolved_pairwise_loss(
    logits: RaggedCandidateLogits,
    preferred_pairs: Sequence[Sequence[tuple[int, int]]],
    *,
    row_reduction: str = "mean",
) -> Tensor:
    """Rank only explicitly resolved candidate pairs within each ragged row.

    Every tuple is ``(preferred_ordinal, disfavored_ordinal)``. Candidates that
    do not appear in a pair are unknown and receive no gradient. Rows contribute
    equally regardless of how many resolved pairs they contain. ``mean`` gives
    each pair equal weight within its row. ``max`` focuses on the worst resolved
    comparison, so duplicate easy alternatives cannot dilute an important one.
    """

    if row_reduction not in {"mean", "max"}:
        raise TorchPolicyError("resolved pairwise row_reduction must be mean or max")
    if len(preferred_pairs) != logits.row_count:
        raise TorchPolicyError(
            "resolved preference rows must match candidate-logit rows"
        )
    splits = logits.row_splits.detach().cpu().tolist()
    row_losses: list[Tensor] = []
    for row, (start, end, pairs) in enumerate(
        zip(splits[:-1], splits[1:], preferred_pairs, strict=True)
    ):
        candidate_count = end - start
        pair_losses: list[Tensor] = []
        for pair_index, pair in enumerate(pairs):
            if len(pair) != 2:
                raise TorchPolicyError(
                    f"resolved preference row {row} pair {pair_index} must contain two ordinals"
                )
            preferred = _non_negative_integer(
                pair[0], f"resolved preference row {row} preferred ordinal"
            )
            disfavored = _non_negative_integer(
                pair[1], f"resolved preference row {row} disfavored ordinal"
            )
            if preferred >= candidate_count or disfavored >= candidate_count:
                raise TorchPolicyError(
                    f"resolved preference row {row} ordinal is outside its candidate row"
                )
            if preferred == disfavored:
                raise TorchPolicyError(
                    f"resolved preference row {row} cannot compare one candidate with itself"
                )
            pair_losses.append(
                torch.nn.functional.softplus(
                    logits.values[start + disfavored]
                    - logits.values[start + preferred]
                )
            )
        if pair_losses:
            stacked = torch.stack(pair_losses)
            row_losses.append(stacked.mean() if row_reduction == "mean" else stacked.amax())
    if not row_losses:
        raise TorchPolicyError("resolved pairwise loss requires at least one preference pair")
    return torch.stack(row_losses).mean()


def ragged_frozen_greedy_trust_hinge(
    logits: RaggedCandidateLogits,
    trusted_ordinals: Sequence[int],
    *,
    margin: float = 0.0,
) -> Tensor:
    """Keep frozen greedy choices on caller-declared induced states.

    This is a policy-preservation regularizer, not outcome evidence about the
    unchosen candidates. Each row is penalized only when another candidate
    approaches or exceeds its frozen choice by the requested margin.
    """

    if not isinstance(logits, RaggedCandidateLogits):
        raise TorchPolicyError("frozen greedy trust requires ragged candidate logits")
    if isinstance(margin, bool) or not isinstance(margin, Real):
        raise TorchPolicyError("frozen greedy trust margin must be a real number")
    normalized_margin = float(margin)
    if not math.isfinite(normalized_margin) or normalized_margin < 0.0:
        raise TorchPolicyError(
            "frozen greedy trust margin must be finite and non-negative"
        )
    if len(trusted_ordinals) != logits.row_count:
        raise TorchPolicyError(
            "frozen greedy trust ordinals must match candidate-logit rows"
        )

    splits = logits.row_splits.detach().cpu().tolist()
    row_losses: list[Tensor] = []
    for row, (start, end, ordinal) in enumerate(
        zip(splits[:-1], splits[1:], trusted_ordinals, strict=True)
    ):
        candidate_count = end - start
        trusted = _non_negative_integer(
            ordinal, f"frozen greedy trust row {row} ordinal"
        )
        if trusted >= candidate_count:
            raise TorchPolicyError(
                f"frozen greedy trust row {row} ordinal is outside its candidate row"
            )
        row_values = logits.values[start:end]
        if candidate_count == 1:
            row_losses.append(row_values[trusted] * 0.0)
            continue
        alternatives = torch.cat(
            (row_values[:trusted], row_values[trusted + 1 :])
        )
        row_losses.append(
            torch.relu(
                alternatives.max()
                - row_values[trusted]
                + normalized_margin
            )
        )
    return torch.stack(row_losses).mean()


def _mean_by_token(values: Tensor, token_indices: Tensor, token_count: int) -> Tensor:
    result = torch.zeros(
        (token_count, values.shape[1]),
        dtype=values.dtype,
        device=values.device,
    )
    if token_indices.numel() == 0:
        return result
    result.index_add_(0, token_indices, values)
    counts = torch.zeros(token_count, dtype=values.dtype, device=values.device)
    counts.index_add_(
        0,
        token_indices,
        torch.ones(token_indices.shape[0], dtype=values.dtype, device=values.device),
    )
    return result / counts.clamp_min(1.0).unsqueeze(1)


def _row_ids(row_splits: Tensor) -> Tensor:
    lengths = row_splits[1:] - row_splits[:-1]
    return torch.repeat_interleave(
        torch.arange(lengths.numel(), device=row_splits.device),
        lengths,
    )


def _validate_splits(
    splits: Tensor,
    row_count: int,
    flat_count: int,
    name: str,
    *,
    require_non_empty: bool = False,
) -> None:
    if splits.shape != (row_count + 1,):
        raise TorchPolicyError(f"{name} row_splits have the wrong length")
    if int(splits[0].item()) != 0 or int(splits[-1].item()) != flat_count:
        raise TorchPolicyError(f"{name} row_splits do not bound the flat table")
    lengths = splits[1:] - splits[:-1]
    if bool(torch.any(lengths < 0)):
        raise TorchPolicyError(f"{name} row_splits are not monotonic")
    if require_non_empty and bool(torch.any(lengths == 0)):
        raise TorchPolicyError(f"{name} rows must not be empty")


def _validate_range(values: Tensor, upper_bound: int, name: str) -> None:
    if values.numel() and (
        bool(torch.any(values < 0)) or bool(torch.any(values >= upper_bound))
    ):
        raise TorchPolicyError(f"{name} is outside 0..{upper_bound}")


def _validate_schema_ids(values: Tensor, valid: Tensor, name: str) -> None:
    _validate_range(values, valid.numel(), name)
    if values.numel() and not bool(torch.all(valid[values])):
        raise TorchPolicyError(f"{name} uses a reserved schema id")


def _validate_aligned(values: tuple[Tensor, ...], name: str) -> None:
    if any(value.ndim != 1 for value in values):
        raise TorchPolicyError(f"{name} columns must be one-dimensional")
    if len({value.numel() for value in values}) != 1:
        raise TorchPolicyError(f"{name} columns are misaligned")


def _index_tensor(value: object, device: torch.device, name: str) -> Tensor:
    try:
        result = torch.as_tensor(
            _writable_numpy_copy(value),
            dtype=torch.long,
            device=device,
        )
    except (TypeError, ValueError, RuntimeError, OverflowError) as error:
        raise TorchPolicyError(f"{name} is not an integer tensor") from error
    if result.ndim != 1:
        raise TorchPolicyError(f"{name} must be one-dimensional")
    return result


def _float_tensor(value: object, device: torch.device, name: str) -> Tensor:
    try:
        result = torch.as_tensor(
            _writable_numpy_copy(value),
            dtype=torch.float32,
            device=device,
        )
    except (TypeError, ValueError, RuntimeError, OverflowError) as error:
        raise TorchPolicyError(f"{name} is not a float tensor") from error
    if result.ndim != 1:
        raise TorchPolicyError(f"{name} must be one-dimensional")
    return result


def _writable_numpy_copy(value: object) -> object:
    if isinstance(value, np.ndarray) and not value.flags.writeable:
        return value.copy()
    return value


def _enum_dimension(
    schema: Mapping[str, object],
    key: str,
) -> tuple[int, tuple[bool, ...]]:
    values = _mapping(_required(schema, key), key).values()
    numeric = {_non_negative_integer(value, f"{key} id") for value in values}
    if not numeric:
        raise TorchPolicyError(f"{key} must not be empty")
    dimension = max(numeric) + 1
    valid = tuple(index in numeric for index in range(dimension))
    return dimension, valid


def _mapping(value: object, name: str) -> Mapping[object, object]:
    if not isinstance(value, Mapping):
        raise TorchPolicyError(f"{name} must be a mapping")
    return value


def _required(mapping: Mapping[object, object], key: str) -> object:
    try:
        return mapping[key]
    except KeyError as error:
        raise TorchPolicyError(f"missing required field {key}") from error


def _non_negative_integer(value: object, name: str) -> int:
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchPolicyError(f"{name} must be an integer") from error
    if normalized < 0:
        raise TorchPolicyError(f"{name} must be non-negative")
    return normalized


def _positive_integer(value: object, name: str) -> int:
    normalized = _non_negative_integer(value, name)
    if normalized == 0:
        raise TorchPolicyError(f"{name} must be positive")
    return normalized
