"""Exact in-process PyTorch behavior binding and explicit persistence."""

from __future__ import annotations

import operator
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, replace
from enum import Enum

import torch

from .manifests import (
    BehaviorManifest,
    BehaviorManifestRegistry,
    BehaviorManifestTemplate,
    BehaviorRuleBinding,
    GREEDY_BEHAVIOR_RULE_V1,
    ManifestArtifactId,
    combat_anchored_greedy_strategic_sampled_rule_v1,
    combat_greedy_strategic_sampled_rule_v1,
)
from .manifest_catalog import (
    BoundedBehaviorManifestCatalog,
    PreparedBehaviorManifest,
)
from .decision_progress import DecisionProgressProvider, DecisionRunProgress
from .policy import (
    DETERMINISTIC_SELECTION,
    BatchPolicyChoice,
    BehaviorManifestId,
)
from .torch_checkpoints import (
    BoundedTorchCheckpointStore,
    PreparedTorchCheckpoint,
)
from .torch_policy import (
    RaggedCandidateLogits,
    RaggedCandidateScorer,
    RaggedCategoricalPolicyConfig,
    categorical_probability_rows,
    sample_ragged_categorical,
    sample_ragged_categorical_rows,
    score_categorical_behavior,
)


class TorchBehaviorError(RuntimeError):
    """A shadow model cannot be safely bound, promoted, or published."""


class FrozenDecisionRule(str, Enum):
    """How typed combat rows are selected from one exact scorer."""

    SAMPLED = "sampled"
    GREEDY = "greedy"


@dataclass(frozen=True)
class TorchBehaviorBinding:
    """Exact model/provenance identity, independent of durable publication."""

    manifest_id: BehaviorManifestId
    manifest: BehaviorManifest
    checkpoint_id: ManifestArtifactId

    def __post_init__(self) -> None:
        if not isinstance(self.manifest_id, BehaviorManifestId):
            raise TorchBehaviorError("behavior manifest id must be typed")
        if not isinstance(self.manifest, BehaviorManifest):
            raise TorchBehaviorError("behavior manifest must be typed")
        if not isinstance(self.checkpoint_id, ManifestArtifactId):
            raise TorchBehaviorError("behavior checkpoint id must be typed")
        if self.manifest_id != self.manifest.identity:
            raise TorchBehaviorError("behavior manifest id conflicts with content")
        if self.manifest.model_checkpoint != self.checkpoint_id:
            raise TorchBehaviorError("behavior checkpoint conflicts with manifest")


@dataclass(frozen=True)
class TorchBehaviorPublication(TorchBehaviorBinding):
    """A behavior binding committed to checkpoint and manifest stores."""


@dataclass(frozen=True)
class TorchBehaviorPublicationPreview:
    """Non-authoritative exact identity and capacity facts for one preview."""

    manifest_id: BehaviorManifestId
    checkpoint_id: ManifestArtifactId
    training_step: int
    checkpoint_payload_bytes: int
    manifest_payload_bytes: int
    requires_novel_capacity: bool


@dataclass(frozen=True)
class _PreparedTorchBehaviorPublication:
    checkpoint: PreparedTorchCheckpoint
    durable_manifest: PreparedBehaviorManifest
    binding: TorchBehaviorBinding


class TorchBehaviorPublisher:
    """Publish checkpoint then manifest; returning is the visibility boundary."""

    def __init__(
        self,
        store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
        registry: BehaviorManifestRegistry,
        template: BehaviorManifestTemplate,
    ) -> None:
        if not isinstance(store, BoundedTorchCheckpointStore):
            raise TorchBehaviorError("publisher requires a checkpoint store")
        if not isinstance(registry, BehaviorManifestRegistry):
            raise TorchBehaviorError("publisher requires a behavior manifest registry")
        if not isinstance(catalog, BoundedBehaviorManifestCatalog):
            raise TorchBehaviorError("publisher requires a durable manifest catalog")
        if not isinstance(template, BehaviorManifestTemplate):
            raise TorchBehaviorError("publisher requires a behavior manifest template")
        self.store = store
        self.catalog = catalog
        self.registry = registry
        self.template = template

    def publish(
        self,
        scorer: RaggedCandidateScorer,
        *,
        training_step: int,
    ) -> TorchBehaviorPublication:
        prepared = self._prepare(scorer, training_step=training_step)
        return self._commit(prepared)

    def publish_exact(
        self,
        scorer: RaggedCandidateScorer,
        binding: TorchBehaviorBinding,
    ) -> TorchBehaviorPublication:
        """Durably publish one already-active exact in-memory binding."""

        if not isinstance(binding, TorchBehaviorBinding):
            raise TorchBehaviorError("exact publication requires a behavior binding")
        prepared = self._prepare(
            scorer,
            training_step=binding.manifest.training_step,
        )
        if (
            prepared.binding.manifest_id != binding.manifest_id
            or prepared.binding.manifest != binding.manifest
            or prepared.binding.checkpoint_id != binding.checkpoint_id
        ):
            raise TorchBehaviorError(
                "active scorer no longer matches its behavior binding"
            )
        return self._commit(prepared)

    def bind_live(
        self,
        scorer: RaggedCandidateScorer,
        *,
        training_step: int,
    ) -> TorchBehaviorBinding:
        """Compute an exact behavior identity without writing durable state."""

        checkpoint, binding = self._bind(scorer, training_step=training_step)
        if checkpoint.artifact_id != binding.checkpoint_id:
            raise TorchBehaviorError("live behavior checkpoint identity changed")
        return binding

    def _commit(
        self,
        prepared: _PreparedTorchBehaviorPublication,
    ) -> TorchBehaviorPublication:
        self._preview(prepared, novel=False)
        binding = prepared.binding
        manifest = binding.manifest
        checkpoint_id = self.store.commit(prepared.checkpoint)
        if checkpoint_id != manifest.model_checkpoint:
            raise TorchBehaviorError("checkpoint store committed a different identity")
        durable_id = self.catalog.commit(prepared.durable_manifest)
        if durable_id != binding.manifest_id:
            raise TorchBehaviorError("manifest catalog committed a different identity")
        registered_id = self.registry.register(manifest, claimed_id=durable_id)
        if registered_id != binding.manifest_id:
            raise TorchBehaviorError("manifest registry committed a different identity")
        return TorchBehaviorPublication(
            binding.manifest_id,
            binding.manifest,
            binding.checkpoint_id,
        )

    def preview(
        self,
        scorer: RaggedCandidateScorer,
        *,
        training_step: int,
    ) -> TorchBehaviorPublicationPreview:
        """Verify an exact publication without mutating any owner."""

        prepared = self._prepare(scorer, training_step=training_step)
        self._preview(prepared, novel=False)
        return _publication_preview(prepared, novel=False)

    def preview_novel(
        self,
        scorer: RaggedCandidateScorer,
        *,
        training_step: int,
    ) -> TorchBehaviorPublicationPreview:
        """Reserve capacity for one new same-shape generation without mutation."""

        prepared = self._prepare(scorer, training_step=training_step)
        self._preview(prepared, novel=True)
        return _publication_preview(prepared, novel=True)

    def _prepare(
        self,
        scorer: RaggedCandidateScorer,
        *,
        training_step: int,
    ) -> _PreparedTorchBehaviorPublication:
        checkpoint, binding = self._bind(scorer, training_step=training_step)
        durable_manifest = self.catalog.prepare(binding.manifest)
        return _PreparedTorchBehaviorPublication(
            checkpoint=checkpoint,
            durable_manifest=durable_manifest,
            binding=binding,
        )

    def _bind(
        self,
        scorer: RaggedCandidateScorer,
        *,
        training_step: int,
    ) -> tuple[PreparedTorchCheckpoint, TorchBehaviorBinding]:
        if not isinstance(scorer, RaggedCandidateScorer):
            raise TorchBehaviorError("publisher requires a RaggedCandidateScorer")
        if scorer.schema.version != self.template.semantic_schema_version:
            raise TorchBehaviorError(
                "scorer schema version does not match behavior manifest template"
            )
        checkpoint = self.store.prepare(scorer)
        manifest = self.template.bind(
            checkpoint.artifact_id,
            training_step=training_step,
        )
        binding = TorchBehaviorBinding(
            manifest.identity,
            manifest,
            checkpoint.artifact_id,
        )
        return checkpoint, binding

    def _preview(
        self,
        prepared: _PreparedTorchBehaviorPublication,
        *,
        novel: bool,
    ) -> None:
        if novel:
            self.store.preview_novel_commit(prepared.checkpoint)
            self.catalog.preview_novel_commit(prepared.durable_manifest)
            self.registry.preview_novel_registration()
            return
        binding = prepared.binding
        self.store.preview_commit(prepared.checkpoint)
        self.catalog.preview_commit(prepared.durable_manifest)
        self.registry.preview_registration(
            binding.manifest,
            claimed_id=binding.manifest_id,
        )


def _publication_preview(
    prepared: _PreparedTorchBehaviorPublication,
    *,
    novel: bool,
) -> TorchBehaviorPublicationPreview:
    binding = prepared.binding
    return TorchBehaviorPublicationPreview(
        manifest_id=binding.manifest_id,
        checkpoint_id=binding.checkpoint_id,
        training_step=binding.manifest.training_step,
        checkpoint_payload_bytes=prepared.checkpoint.payload_bytes,
        manifest_payload_bytes=prepared.durable_manifest.payload_bytes,
        requires_novel_capacity=novel,
    )


_PROMOTION_TOKEN = object()


class CheckpointedGreedyTorchPolicy:
    """Frozen behavior scorer materialized only from a registered publication."""

    def __init__(
        self,
        scorer: RaggedCandidateScorer,
        binding: TorchBehaviorBinding,
        *,
        _token: object,
    ) -> None:
        if _token is not _PROMOTION_TOKEN:
            raise TorchBehaviorError("behavior policy must be created through promote")
        if not isinstance(binding, TorchBehaviorBinding):
            raise TorchBehaviorError("behavior policy requires an exact binding")
        self._scorer = scorer
        self.binding = binding

    @classmethod
    def promote(
        cls,
        publication: TorchBehaviorPublication,
        store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
        registry: BehaviorManifestRegistry,
        scorer_factory: Callable[[], RaggedCandidateScorer],
    ) -> CheckpointedGreedyTorchPolicy:
        model = _promote_frozen_scorer(
            publication,
            store,
            catalog,
            registry,
            scorer_factory,
            expected_rule=GREEDY_BEHAVIOR_RULE_V1,
            rule_name="greedy candidate rule",
        )
        return cls(model, publication, _token=_PROMOTION_TOKEN)

    @classmethod
    def recover(
        cls,
        manifest_id: BehaviorManifestId,
        store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
        registry: BehaviorManifestRegistry,
        scorer_factory: Callable[[], RaggedCandidateScorer],
    ) -> CheckpointedGreedyTorchPolicy:
        """Recover from durable owners without a prior in-memory publication."""

        model, publication = _recover_frozen_scorer(
            manifest_id,
            store,
            catalog,
            registry,
            scorer_factory,
            expected_rule=GREEDY_BEHAVIOR_RULE_V1,
            rule_name="greedy candidate rule",
        )
        return cls(model, publication, _token=_PROMOTION_TOKEN)

    @property
    def behavior_manifest_id(self) -> BehaviorManifestId:
        return self.binding.manifest_id

    def score(self, decision_batch: Mapping[str, object]) -> RaggedCandidateLogits:
        with torch.inference_mode():
            return self._scorer(decision_batch)

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        ordinals = self.score(decision_batch).greedy_ordinals()
        return BatchPolicyChoice.deterministic(ordinals, self.behavior_manifest_id)


class FrozenCategoricalTorchPolicy:
    """Frozen exact behavior sampled through one injected random stream."""

    def __init__(
        self,
        scorer: RaggedCandidateScorer,
        binding: TorchBehaviorBinding,
        config: RaggedCategoricalPolicyConfig,
        generator: torch.Generator,
        *,
        _token: object,
    ) -> None:
        if _token is not _PROMOTION_TOKEN:
            raise TorchBehaviorError("behavior policy must be created through promote")
        if not isinstance(binding, TorchBehaviorBinding):
            raise TorchBehaviorError("behavior policy requires an exact binding")
        _validate_categorical_inputs(config, generator)
        _require_generator_device(scorer, generator)
        self._scorer = scorer
        self.binding = binding
        self.config = config
        self.generator = generator

    @classmethod
    def promote(
        cls,
        publication: TorchBehaviorPublication,
        store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
        registry: BehaviorManifestRegistry,
        scorer_factory: Callable[[], RaggedCandidateScorer],
        config: RaggedCategoricalPolicyConfig,
        generator: torch.Generator,
    ) -> FrozenCategoricalTorchPolicy:
        _validate_categorical_inputs(config, generator)
        model = _promote_frozen_scorer(
            publication,
            store,
            catalog,
            registry,
            scorer_factory,
            expected_rule=config.behavior_rule,
            rule_name="categorical candidate rule",
            model_validator=lambda scorer: _require_generator_device(
                scorer,
                generator,
            ),
        )
        return cls(
            model,
            publication,
            config,
            generator,
            _token=_PROMOTION_TOKEN,
        )

    @classmethod
    def recover(
        cls,
        manifest_id: BehaviorManifestId,
        store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
        registry: BehaviorManifestRegistry,
        scorer_factory: Callable[[], RaggedCandidateScorer],
        config: RaggedCategoricalPolicyConfig,
        generator: torch.Generator,
    ) -> FrozenCategoricalTorchPolicy:
        _validate_categorical_inputs(config, generator)
        model, publication = _recover_frozen_scorer(
            manifest_id,
            store,
            catalog,
            registry,
            scorer_factory,
            expected_rule=config.behavior_rule,
            rule_name="categorical candidate rule",
            model_validator=lambda scorer: _require_generator_device(
                scorer,
                generator,
            ),
        )
        return cls(
            model,
            publication,
            config,
            generator,
            _token=_PROMOTION_TOKEN,
        )

    @property
    def behavior_manifest_id(self) -> BehaviorManifestId:
        return self.binding.manifest_id

    @property
    def frozen_scorer(self) -> RaggedCandidateScorer:
        return self._scorer

    def fork(self, generator: torch.Generator) -> FrozenCategoricalTorchPolicy:
        """Share immutable behavior while giving one caller an independent RNG."""

        _validate_categorical_inputs(self.config, generator)
        _require_generator_device(self._scorer, generator)
        return FrozenCategoricalTorchPolicy(
            self._scorer,
            self.binding,
            self.config,
            generator,
            _token=_PROMOTION_TOKEN,
        )

    def score(self, decision_batch: Mapping[str, object]) -> RaggedCandidateLogits:
        with torch.inference_mode():
            return score_categorical_behavior(self._scorer, decision_batch, self.config)

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        sample = sample_ragged_categorical(
            self.score(decision_batch),
            self.config,
            self.generator,
        )
        return BatchPolicyChoice.create(
            sample.ordinals,
            self.behavior_manifest_id,
            sample.selection_probabilities,
        )


class FrozenReplicateCategoricalTorchPolicy:
    """One frozen scorer with an independent RNG stream per replicate slot."""

    def __init__(
        self,
        scorer: RaggedCandidateScorer,
        binding: TorchBehaviorBinding,
        config: RaggedCategoricalPolicyConfig,
        generators: Sequence[torch.Generator],
        *,
        _token: object,
    ) -> None:
        if _token is not _PROMOTION_TOKEN:
            raise TorchBehaviorError(
                "replicate policy must derive from frozen behavior"
            )
        if not isinstance(binding, TorchBehaviorBinding):
            raise TorchBehaviorError(
                "replicate policy requires an exact binding"
            )
        streams = tuple(generators)
        if not streams:
            raise TorchBehaviorError(
                "replicate policy requires at least one RNG stream"
            )
        if not all(isinstance(generator, torch.Generator) for generator in streams):
            raise TorchBehaviorError(
                "replicate policy requires typed RNG streams"
            )
        if len({id(generator) for generator in streams}) != len(streams):
            raise TorchBehaviorError(
                "replicate policy requires independent RNG streams"
            )
        for generator in streams:
            _validate_categorical_inputs(config, generator)
            _require_generator_device(scorer, generator)
        self._scorer = scorer
        self.binding = binding
        self.config = config
        self.generators = streams

    @classmethod
    def from_categorical(
        cls,
        policy: FrozenCategoricalTorchPolicy,
        generators: Sequence[torch.Generator],
    ) -> FrozenReplicateCategoricalTorchPolicy:
        if not isinstance(policy, FrozenCategoricalTorchPolicy):
            raise TorchBehaviorError(
                "replicate policy requires frozen categorical behavior"
            )
        streams = tuple(generators)
        if not streams or streams[0] is not policy.generator:
            raise TorchBehaviorError(
                "replicate policy must retain its recovered first RNG stream"
            )
        return cls(
            policy.frozen_scorer,
            policy.binding,
            policy.config,
            streams,
            _token=_PROMOTION_TOKEN,
        )

    @property
    def behavior_manifest_id(self) -> BehaviorManifestId:
        return self.binding.manifest_id

    def score(self, decision_batch: Mapping[str, object]) -> RaggedCandidateLogits:
        with torch.inference_mode():
            return score_categorical_behavior(self._scorer, decision_batch, self.config)

    def predict_row_values(self, decision_batch: Mapping[str, object]) -> tuple[float, ...] | None:
        """Read the frozen value head for an evaluation receipt, without sampling."""
        if self._scorer.value_head is None:
            return None
        with torch.inference_mode():
            values = self._scorer.actor_critic(decision_batch).row_values
        return tuple(float(value) for value in values.detach().cpu().tolist())

    def candidate_probability_rows(self, decision_batch: Mapping[str, object]) -> tuple[tuple[float, ...], ...]:
        """Read the frozen actor's full sampling distribution without changing RNG."""
        rows = categorical_probability_rows(self.score(decision_batch), self.config)
        return tuple(tuple(float(value) for value in row.detach().cpu().tolist()) for row in rows)

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        slots = _decision_slots(decision_batch)
        if any(slot >= len(self.generators) for slot in slots):
            raise TorchBehaviorError(
                "decision slot exceeds replicate RNG stream count"
            )
        sample = sample_ragged_categorical_rows(
            self.score(decision_batch),
            self.config,
            tuple(self.generators[slot] for slot in slots),
        )
        return BatchPolicyChoice.create(
            sample.ordinals,
            self.behavior_manifest_id,
            sample.selection_probabilities,
        )


class CheckpointedCategoricalTorchPolicy(FrozenCategoricalTorchPolicy):
    """Frozen behavior whose binding was verified through durable stores."""


@dataclass(frozen=True)
class FrozenCombatAnchor:
    """One immutable scorer used only for combat argmax decisions."""

    scorer: RaggedCandidateScorer
    binding: TorchBehaviorBinding

    def __post_init__(self) -> None:
        if not isinstance(self.scorer, RaggedCandidateScorer):
            raise TorchBehaviorError("combat anchor requires a frozen scorer")
        if not isinstance(self.binding, TorchBehaviorBinding):
            raise TorchBehaviorError("combat anchor requires an exact binding")
        if self.scorer.schema.version != self.binding.manifest.semantic_schema_version:
            raise TorchBehaviorError("combat anchor semantic schema changed")
        if self.scorer.training or any(
            parameter.requires_grad for parameter in self.scorer.parameters()
        ):
            raise TorchBehaviorError("combat anchor scorer must be immutable")

    @classmethod
    def from_behavior(
        cls,
        policy: FrozenCategoricalTorchPolicy,
    ) -> FrozenCombatAnchor:
        if not isinstance(policy, FrozenCategoricalTorchPolicy):
            raise TorchBehaviorError(
                "combat anchor requires frozen categorical behavior"
            )
        return cls(policy.frozen_scorer, policy.binding)

    @classmethod
    def recover(
        cls,
        manifest_id: BehaviorManifestId,
        store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
        scorer_factory: Callable[[], RaggedCandidateScorer],
    ) -> FrozenCombatAnchor:
        if not isinstance(manifest_id, BehaviorManifestId):
            raise TorchBehaviorError("combat anchor manifest id must be typed")
        if not isinstance(store, BoundedTorchCheckpointStore):
            raise TorchBehaviorError("combat anchor requires a checkpoint store")
        if not isinstance(catalog, BoundedBehaviorManifestCatalog):
            raise TorchBehaviorError("combat anchor requires a manifest catalog")
        try:
            manifest = catalog.resolve(manifest_id)
        except RuntimeError as error:
            raise TorchBehaviorError(
                "durable combat anchor manifest is unavailable"
            ) from error
        publication = TorchBehaviorPublication(
            manifest_id,
            manifest,
            manifest.model_checkpoint,
        )
        model = store.materialize(publication.checkpoint_id, scorer_factory)
        return cls(
            _freeze_scorer(model, publication, None),
            publication,
        )

    @property
    def manifest_id(self) -> BehaviorManifestId:
        return self.binding.manifest_id

    @property
    def checkpoint_id(self) -> ManifestArtifactId:
        return self.binding.checkpoint_id


class FrozenGreedyTorchPolicy:
    """Exact greedy decision rule derived from a frozen categorical scorer."""

    def __init__(
        self,
        scorer: RaggedCandidateScorer,
        binding: TorchBehaviorBinding,
        source_manifest_id: BehaviorManifestId,
        *,
        _token: object,
    ) -> None:
        if _token is not _PROMOTION_TOKEN:
            raise TorchBehaviorError(
                "greedy evaluation policy must derive from frozen behavior"
            )
        if not isinstance(binding, TorchBehaviorBinding):
            raise TorchBehaviorError("greedy evaluation requires an exact binding")
        if not isinstance(source_manifest_id, BehaviorManifestId):
            raise TorchBehaviorError(
                "greedy evaluation requires a source behavior manifest"
            )
        _require_behavior_rule(
            binding.manifest,
            GREEDY_BEHAVIOR_RULE_V1,
            "greedy candidate rule",
        )
        self._scorer = scorer
        self.binding = binding
        self.source_manifest_id = source_manifest_id

    @classmethod
    def from_categorical(
        cls,
        policy: FrozenCategoricalTorchPolicy,
    ) -> FrozenGreedyTorchPolicy:
        if not isinstance(policy, FrozenCategoricalTorchPolicy):
            raise TorchBehaviorError(
                "greedy evaluation requires frozen categorical behavior"
            )
        return cls.from_behavior(policy)

    @classmethod
    def from_behavior(
        cls,
        policy: FrozenCategoricalTorchPolicy | FrozenCombatGreedyTorchPolicy,
    ) -> FrozenGreedyTorchPolicy:
        if not isinstance(
            policy,
            (FrozenCategoricalTorchPolicy, FrozenCombatGreedyTorchPolicy),
        ):
            raise TorchBehaviorError(
                "greedy evaluation requires frozen torch behavior"
            )
        source_scorer = policy.frozen_scorer
        source_binding = policy.binding
        if (
            isinstance(policy, FrozenCombatGreedyTorchPolicy)
            and policy.combat_anchor is not None
        ):
            source_scorer = policy.combat_anchor.scorer
            source_binding = policy.combat_anchor.binding
        manifest = replace(
            source_binding.manifest,
            behavior_rule=GREEDY_BEHAVIOR_RULE_V1,
        )
        binding = TorchBehaviorBinding(
            manifest.identity,
            manifest,
            source_binding.checkpoint_id,
        )
        return cls(
            source_scorer,
            binding,
            policy.behavior_manifest_id,
            _token=_PROMOTION_TOKEN,
        )

    @property
    def behavior_manifest_id(self) -> BehaviorManifestId:
        return self.binding.manifest_id

    def score(self, decision_batch: Mapping[str, object]) -> RaggedCandidateLogits:
        with torch.inference_mode():
            return self._scorer(decision_batch)

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        ordinals = self.score(decision_batch).greedy_ordinals()
        return BatchPolicyChoice.deterministic(
            ordinals,
            self.behavior_manifest_id,
        )


class FrozenCombatGreedyTorchPolicy:
    """Use argmax for typed combat rows and source sampling elsewhere."""

    def __init__(
        self,
        scorer: RaggedCandidateScorer,
        binding: TorchBehaviorBinding,
        config: RaggedCategoricalPolicyConfig,
        generator: torch.Generator,
        progress_provider: DecisionProgressProvider | None,
        source_manifest_id: BehaviorManifestId,
        combat_anchor: FrozenCombatAnchor | None = None,
        *,
        _token: object,
    ) -> None:
        if _token is not _PROMOTION_TOKEN:
            raise TorchBehaviorError(
                "combat-scoped greedy policy must derive from frozen behavior"
            )
        if not isinstance(scorer, RaggedCandidateScorer):
            raise TorchBehaviorError(
                "combat-scoped greedy policy requires a frozen scorer"
            )
        if not isinstance(binding, TorchBehaviorBinding):
            raise TorchBehaviorError(
                "combat-scoped greedy policy requires an exact binding"
            )
        _validate_categorical_inputs(config, generator)
        _require_generator_device(scorer, generator)
        if progress_provider is not None and not callable(
            getattr(progress_provider, "capture", None)
        ):
            raise TorchBehaviorError(
                "combat-scoped greedy policy requires typed decision progress"
            )
        if combat_anchor is not None and not isinstance(
            combat_anchor,
            FrozenCombatAnchor,
        ):
            raise TorchBehaviorError("combat anchor must be typed")
        expected_rule = _combat_scoped_behavior_rule(
            config.behavior_rule,
            combat_anchor,
        )
        _require_behavior_rule(
            binding.manifest,
            expected_rule,
            "combat-scoped greedy candidate rule",
        )
        if not isinstance(source_manifest_id, BehaviorManifestId):
            raise TorchBehaviorError(
                "combat-scoped greedy policy requires a source manifest identity"
            )
        source_manifest = replace(
            binding.manifest,
            behavior_rule=config.behavior_rule,
        )
        if source_manifest.identity != source_manifest_id:
            raise TorchBehaviorError(
                "combat-scoped greedy source manifest identity changed"
            )
        self._scorer = scorer
        self.binding = binding
        self.config = config
        self.generator = generator
        self.progress_provider = progress_provider
        self.source_manifest_id = source_manifest_id
        self.combat_anchor = combat_anchor

    @classmethod
    def from_categorical(
        cls,
        policy: FrozenCategoricalTorchPolicy,
        progress_provider: DecisionProgressProvider | None,
        combat_anchor: FrozenCombatAnchor | None = None,
    ) -> FrozenCombatGreedyTorchPolicy:
        if not isinstance(policy, FrozenCategoricalTorchPolicy):
            raise TorchBehaviorError(
                "combat-scoped greedy policy requires frozen categorical behavior"
            )
        if policy.binding.manifest.behavior_rule != policy.config.behavior_rule:
            raise TorchBehaviorError(
                "categorical policy rule conflicts with its sampled configuration"
            )
        if combat_anchor is not None:
            if not isinstance(combat_anchor, FrozenCombatAnchor):
                raise TorchBehaviorError("combat anchor must be typed")
            source_manifest = policy.binding.manifest
            anchor_manifest = combat_anchor.binding.manifest
            for field in (
                "model_definition",
                "model_config",
                "semantic_schema",
                "semantic_schema_version",
            ):
                if getattr(source_manifest, field) != getattr(anchor_manifest, field):
                    raise TorchBehaviorError(
                        f"combat anchor {field} differs from strategic scorer"
                    )
            if anchor_manifest.behavior_rule != policy.config.behavior_rule:
                raise TorchBehaviorError(
                    "combat anchor behavior rule differs from strategic scorer"
                )
        manifest = replace(
            policy.binding.manifest,
            behavior_rule=_combat_scoped_behavior_rule(
                policy.binding.manifest.behavior_rule,
                combat_anchor,
            ),
        )
        binding = TorchBehaviorBinding(
            manifest.identity,
            manifest,
            policy.binding.checkpoint_id,
        )
        return cls(
            policy.frozen_scorer,
            binding,
            policy.config,
            policy.generator,
            progress_provider,
            policy.behavior_manifest_id,
            combat_anchor,
            _token=_PROMOTION_TOKEN,
        )

    @classmethod
    def recover(
        cls,
        manifest_id: BehaviorManifestId,
        store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
        registry: BehaviorManifestRegistry,
        scorer_factory: Callable[[], RaggedCandidateScorer],
        config: RaggedCategoricalPolicyConfig,
        generator: torch.Generator,
        progress_provider: DecisionProgressProvider | None = None,
        combat_anchor: FrozenCombatAnchor | None = None,
    ) -> FrozenCombatGreedyTorchPolicy:
        """Recover the exact mixed rule; progress may be bound by its run env."""

        _validate_categorical_inputs(config, generator)
        expected_rule = _combat_scoped_behavior_rule(
            config.behavior_rule,
            combat_anchor,
        )
        model, publication = _recover_frozen_scorer(
            manifest_id,
            store,
            catalog,
            registry,
            scorer_factory,
            expected_rule=expected_rule,
            rule_name="combat-scoped greedy candidate rule",
            model_validator=lambda scorer: _require_generator_device(
                scorer,
                generator,
            ),
        )
        source_manifest = replace(
            publication.manifest,
            behavior_rule=config.behavior_rule,
        )
        return cls(
            model,
            publication,
            config,
            generator,
            progress_provider,
            source_manifest.identity,
            combat_anchor,
            _token=_PROMOTION_TOKEN,
        )

    @property
    def behavior_manifest_id(self) -> BehaviorManifestId:
        return self.binding.manifest_id

    @property
    def frozen_scorer(self) -> RaggedCandidateScorer:
        return self._scorer

    def fork(
        self,
        generator: torch.Generator,
        *,
        progress_provider: DecisionProgressProvider | None = None,
    ) -> FrozenCombatGreedyTorchPolicy:
        """Share immutable weights with a fresh RNG and optional run context."""

        provider = (
            self.progress_provider
            if progress_provider is None
            else progress_provider
        )
        return FrozenCombatGreedyTorchPolicy(
            self._scorer,
            self.binding,
            self.config,
            generator,
            provider,
            self.source_manifest_id,
            self.combat_anchor,
            _token=_PROMOTION_TOKEN,
        )

    def bind_progress_provider(
        self,
        progress_provider: DecisionProgressProvider,
    ) -> FrozenCombatGreedyTorchPolicy:
        """Return one environment-bound view without changing behavior identity."""

        return FrozenCombatGreedyTorchPolicy(
            self._scorer,
            self.binding,
            self.config,
            self.generator,
            progress_provider,
            self.source_manifest_id,
            self.combat_anchor,
            _token=_PROMOTION_TOKEN,
        )

    def bind_combat_only(self) -> FrozenCombatGreedyTorchPolicy:
        """Bind an explicit combat-episode domain with no strategic rows."""

        return self.bind_progress_provider(_AllCombatDecisionProgressProvider())

    @property
    def is_combat_only(self) -> bool:
        return isinstance(
            self.progress_provider,
            _AllCombatDecisionProgressProvider,
        )

    def score(self, decision_batch: Mapping[str, object]) -> RaggedCandidateLogits:
        with torch.inference_mode():
            return score_categorical_behavior(self._scorer, decision_batch, self.config)

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        if self.progress_provider is None:
            raise TorchBehaviorError(
                "combat-scoped greedy policy is not bound to run progress"
            )
        slots = _decision_slots(decision_batch)
        progress = tuple(self.progress_provider.capture(slots))
        if len(progress) != len(slots):
            raise TorchBehaviorError(
                "combat-scoped greedy progress rows are misaligned"
            )
        has_combat = any(row.is_combat for row in progress)
        has_strategic = any(not row.is_combat for row in progress)
        shared_logits = (
            self.score(decision_batch)
            if self.combat_anchor is None
            else None
        )
        combat_logits = (
            shared_logits
            if self.combat_anchor is None
            else (
                _score_frozen(self.combat_anchor.scorer, decision_batch)
                if has_combat
                else None
            )
        )
        greedy_ordinals = (
            None if combat_logits is None else combat_logits.greedy_ordinals()
        )
        if greedy_ordinals is not None and len(greedy_ordinals) != len(progress):
            raise TorchBehaviorError(
                "combat-scoped greedy logits are misaligned"
            )
        strategic_logits = (
            shared_logits
            if self.combat_anchor is None
            else (
                self.score(decision_batch)
                if has_strategic
                else None
            )
        )
        sampled = (
            None
            if strategic_logits is None
            else sample_ragged_categorical(
                strategic_logits,
                self.config,
                self.generator,
            )
        )
        ordinals: list[int] = []
        probabilities = []
        for index, row in enumerate(progress):
            if row.is_combat:
                if greedy_ordinals is None:
                    raise AssertionError("combat argmax result is missing")
                ordinals.append(greedy_ordinals[index])
                probabilities.append(DETERMINISTIC_SELECTION)
            else:
                if sampled is None:
                    raise AssertionError("strategic sampling result is missing")
                ordinals.append(sampled.ordinals[index])
                probabilities.append(sampled.selection_probabilities[index])
        return BatchPolicyChoice.create(
            ordinals,
            self.behavior_manifest_id,
            probabilities,
        )


class _AllCombatDecisionProgressProvider:
    """Typed adapter used only by exact combat-episode evaluators."""

    def capture(self, slot_indices) -> tuple[DecisionRunProgress, ...]:
        return tuple(
            DecisionRunProgress(0, 0, 0, True, None)
            for _slot in slot_indices
        )


def _score_frozen(
    scorer: RaggedCandidateScorer,
    decision_batch: Mapping[str, object],
) -> RaggedCandidateLogits:
    with torch.inference_mode():
        return scorer(decision_batch)


def _combat_scoped_behavior_rule(
    sampled_rule: BehaviorRuleBinding,
    combat_anchor: FrozenCombatAnchor | None,
) -> BehaviorRuleBinding:
    if combat_anchor is None:
        return combat_greedy_strategic_sampled_rule_v1(sampled_rule)
    return combat_anchored_greedy_strategic_sampled_rule_v1(
        sampled_rule,
        combat_anchor.manifest_id,
    )


@dataclass(frozen=True)
class CategoricalTorchBehaviorControllerSnapshot:
    active_manifest_id: BehaviorManifestId | None
    active_training_step: int | None
    successful_promotions: int


class CategoricalTorchBehaviorController:
    """Atomically switch one live sampled or combat-scoped behavior."""

    def __init__(
        self,
        publisher: TorchBehaviorPublisher,
        scorer_factory: Callable[[], RaggedCandidateScorer],
        config: RaggedCategoricalPolicyConfig,
        generator: torch.Generator,
        *,
        combat_decision_rule: FrozenDecisionRule = FrozenDecisionRule.SAMPLED,
        progress_provider: DecisionProgressProvider | None = None,
        combat_anchor: FrozenCombatAnchor | None = None,
    ) -> None:
        if not isinstance(publisher, TorchBehaviorPublisher):
            raise TorchBehaviorError("controller requires a behavior publisher")
        if not callable(scorer_factory):
            raise TorchBehaviorError("controller requires a scorer factory")
        _validate_categorical_inputs(config, generator)
        if not isinstance(combat_decision_rule, FrozenDecisionRule):
            raise TorchBehaviorError(
                "controller combat decision rule must be typed"
            )
        if progress_provider is not None and not callable(
            getattr(progress_provider, "capture", None)
        ):
            raise TorchBehaviorError(
                "controller decision progress provider must be typed"
            )
        if combat_anchor is not None and not isinstance(
            combat_anchor,
            FrozenCombatAnchor,
        ):
            raise TorchBehaviorError("controller combat anchor must be typed")
        if (
            combat_anchor is not None
            and combat_decision_rule is not FrozenDecisionRule.GREEDY
        ):
            raise TorchBehaviorError(
                "controller combat anchor requires greedy combat decisions"
            )
        behavior_rule = (
            config.behavior_rule
            if combat_decision_rule is FrozenDecisionRule.SAMPLED
            else _combat_scoped_behavior_rule(
                config.behavior_rule,
                combat_anchor,
            )
        )
        if publisher.template.behavior_rule != behavior_rule:
            raise TorchBehaviorError(
                "controller behavior rule conflicts with publisher provenance"
            )
        self.publisher = publisher
        self.scorer_factory = scorer_factory
        self.config = config
        self.generator = generator
        self.combat_decision_rule = combat_decision_rule
        self.behavior_rule = behavior_rule
        self.combat_anchor = combat_anchor
        self._progress_provider = progress_provider
        self._policy: (
            FrozenCategoricalTorchPolicy
            | FrozenCombatGreedyTorchPolicy
            | None
        ) = None
        self._successful_promotions = 0

    def bind_progress_provider(
        self,
        progress_provider: DecisionProgressProvider,
    ) -> None:
        """Bind typed run context before activating a mixed behavior."""

        if not callable(getattr(progress_provider, "capture", None)):
            raise TorchBehaviorError(
                "controller decision progress provider must be typed"
            )
        if self._policy is not None:
            raise TorchBehaviorError(
                "controller progress must be bound before behavior activation"
            )
        self._progress_provider = progress_provider

    @property
    def snapshot(self) -> CategoricalTorchBehaviorControllerSnapshot:
        binding = self._policy.binding if self._policy is not None else None
        return CategoricalTorchBehaviorControllerSnapshot(
            active_manifest_id=(
                binding.manifest_id if binding is not None else None
            ),
            active_training_step=(
                binding.manifest.training_step if binding is not None else None
            ),
            successful_promotions=self._successful_promotions,
        )

    def promote_live(
        self,
        scorer: RaggedCandidateScorer,
        *,
        training_step: int,
    ) -> TorchBehaviorBinding:
        """Freeze and activate a model without writing durable artifacts."""

        step = _training_step(training_step)
        current = self.snapshot.active_training_step
        if current is not None and step <= current:
            raise TorchBehaviorError(
                "controller training step must increase across promotions"
            )
        frozen = _clone_frozen_scorer(scorer, self.scorer_factory)
        binding = self.publisher.bind_live(frozen, training_step=step)
        _require_behavior_rule(
            binding.manifest,
            self.behavior_rule,
            "configured candidate rule",
        )
        policy = self._policy_for_binding(frozen, binding, self.generator)
        registered_id = self.publisher.registry.replace_active(
            self.snapshot.active_manifest_id,
            binding.manifest,
        )
        if registered_id != binding.manifest_id:
            raise TorchBehaviorError(
                "live registry committed a different behavior identity"
            )
        self._policy = policy
        self._successful_promotions += 1
        return binding

    def publish_active(self) -> TorchBehaviorPublication:
        """Durably publish the current frozen behavior without switching it."""

        if self._policy is None:
            raise TorchBehaviorError("categorical behavior controller is inactive")
        return self.publisher.publish_exact(
            self._policy.frozen_scorer,
            self._policy.binding,
        )

    def fork_active(
        self,
        generator: torch.Generator,
    ) -> FrozenCategoricalTorchPolicy | FrozenCombatGreedyTorchPolicy:
        """Bind the active frozen scorer to one independent random stream."""

        if self._policy is None:
            raise TorchBehaviorError("categorical behavior controller is inactive")
        _validate_categorical_inputs(self.config, generator)
        _require_generator_device(self._policy.frozen_scorer, generator)
        return self._policy_for_binding(
            self._policy.frozen_scorer,
            self._policy.binding,
            generator,
        )

    def recover_and_promote(
        self,
        manifest_id: BehaviorManifestId,
        *,
        successful_promotions: int = 1,
    ) -> TorchBehaviorPublication:
        if self._policy is not None:
            raise TorchBehaviorError(
                "controller recovery requires an inactive behavior slot"
            )
        if isinstance(successful_promotions, bool):
            raise TorchBehaviorError("successful promotions must be an integer")
        try:
            promotion_count = operator.index(successful_promotions)
        except TypeError as error:
            raise TorchBehaviorError("successful promotions must be an integer") from error
        if promotion_count <= 0:
            raise TorchBehaviorError("successful promotions must be positive")
        if self.combat_decision_rule is FrozenDecisionRule.SAMPLED:
            policy = CheckpointedCategoricalTorchPolicy.recover(
                manifest_id,
                self.publisher.store,
                self.publisher.catalog,
                self.publisher.registry,
                self.scorer_factory,
                self.config,
                self.generator,
            )
        else:
            policy = FrozenCombatGreedyTorchPolicy.recover(
                manifest_id,
                self.publisher.store,
                self.publisher.catalog,
                self.publisher.registry,
                self.scorer_factory,
                self.config,
                self.generator,
                self._progress_provider,
                self.combat_anchor,
            )
        self._policy = policy
        self._successful_promotions = promotion_count
        if not isinstance(policy.binding, TorchBehaviorPublication):
            raise TorchBehaviorError("recovered behavior is not durably published")
        return policy.binding

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        if self._policy is None:
            raise TorchBehaviorError("categorical behavior controller is inactive")
        return self._policy.choose(decision_batch)

    def _policy_for_binding(
        self,
        scorer: RaggedCandidateScorer,
        binding: TorchBehaviorBinding,
        generator: torch.Generator,
    ) -> FrozenCategoricalTorchPolicy | FrozenCombatGreedyTorchPolicy:
        if self.combat_decision_rule is FrozenDecisionRule.SAMPLED:
            return FrozenCategoricalTorchPolicy(
                scorer,
                binding,
                self.config,
                generator,
                _token=_PROMOTION_TOKEN,
            )
        if self._progress_provider is None:
            raise TorchBehaviorError(
                "combat-scoped greedy controller requires decision progress"
            )
        source_manifest = replace(
            binding.manifest,
            behavior_rule=self.config.behavior_rule,
        )
        return FrozenCombatGreedyTorchPolicy(
            scorer,
            binding,
            self.config,
            generator,
            self._progress_provider,
            source_manifest.identity,
            self.combat_anchor,
            _token=_PROMOTION_TOKEN,
        )


def _clone_frozen_scorer(
    scorer: RaggedCandidateScorer,
    scorer_factory: Callable[[], RaggedCandidateScorer],
) -> RaggedCandidateScorer:
    """Copy a shadow scorer into an independent in-process behavior model."""

    if not isinstance(scorer, RaggedCandidateScorer):
        raise TorchBehaviorError("live promotion requires a RaggedCandidateScorer")
    model = scorer_factory()
    if not isinstance(model, RaggedCandidateScorer):
        raise TorchBehaviorError(
            "behavior scorer factory did not create a RaggedCandidateScorer"
        )
    if model.schema.version != scorer.schema.version:
        raise TorchBehaviorError(
            "behavior scorer factory returned a different schema version"
        )
    try:
        model.load_state_dict(scorer.state_dict(), strict=True)
    except RuntimeError as error:
        raise TorchBehaviorError(
            "behavior scorer factory is incompatible with the shadow model"
        ) from error
    model.eval()
    model.requires_grad_(False)
    return model


def _promote_frozen_scorer(
    publication: TorchBehaviorPublication,
    store: BoundedTorchCheckpointStore,
    catalog: BoundedBehaviorManifestCatalog,
    registry: BehaviorManifestRegistry,
    scorer_factory: Callable[[], RaggedCandidateScorer],
    *,
    expected_rule: BehaviorRuleBinding,
    rule_name: str,
    model_validator: Callable[[RaggedCandidateScorer], None] | None = None,
) -> RaggedCandidateScorer:
    if not isinstance(publication, TorchBehaviorPublication):
        raise TorchBehaviorError("promotion requires a typed publication")
    if not isinstance(store, BoundedTorchCheckpointStore):
        raise TorchBehaviorError("promotion requires a checkpoint store")
    if not isinstance(catalog, BoundedBehaviorManifestCatalog):
        raise TorchBehaviorError("promotion requires a durable manifest catalog")
    if not isinstance(registry, BehaviorManifestRegistry):
        raise TorchBehaviorError("promotion requires a manifest registry")
    try:
        durable = catalog.resolve(publication.manifest_id)
        if durable != publication.manifest:
            raise TorchBehaviorError(
                "durable manifest does not match behavior publication"
            )
        registry.require_exact(publication.manifest_id, publication.manifest)
    except ValueError as error:
        raise TorchBehaviorError(
            "publication is not registered for behavior promotion"
        ) from error
    _require_behavior_rule(publication.manifest, expected_rule, rule_name)
    model = store.materialize(publication.checkpoint_id, scorer_factory)
    return _freeze_scorer(model, publication, model_validator)


def _recover_frozen_scorer(
    manifest_id: BehaviorManifestId,
    store: BoundedTorchCheckpointStore,
    catalog: BoundedBehaviorManifestCatalog,
    registry: BehaviorManifestRegistry,
    scorer_factory: Callable[[], RaggedCandidateScorer],
    *,
    expected_rule: BehaviorRuleBinding,
    rule_name: str,
    model_validator: Callable[[RaggedCandidateScorer], None] | None = None,
) -> tuple[RaggedCandidateScorer, TorchBehaviorPublication]:
    if not isinstance(manifest_id, BehaviorManifestId):
        raise TorchBehaviorError("recovery manifest id must be typed")
    if not isinstance(store, BoundedTorchCheckpointStore):
        raise TorchBehaviorError("recovery requires a checkpoint store")
    if not isinstance(catalog, BoundedBehaviorManifestCatalog):
        raise TorchBehaviorError("recovery requires a durable manifest catalog")
    if not isinstance(registry, BehaviorManifestRegistry):
        raise TorchBehaviorError("recovery requires a manifest registry")
    try:
        manifest = catalog.resolve(manifest_id)
    except RuntimeError as error:
        raise TorchBehaviorError("durable behavior manifest is unavailable") from error
    _require_behavior_rule(manifest, expected_rule, rule_name)
    publication = TorchBehaviorPublication(
        manifest_id=manifest_id,
        manifest=manifest,
        checkpoint_id=manifest.model_checkpoint,
    )
    model = store.materialize(publication.checkpoint_id, scorer_factory)
    frozen = _freeze_scorer(model, publication, model_validator)
    registry.preview_registration(manifest, claimed_id=manifest_id)
    registry.register(manifest, claimed_id=manifest_id)
    return frozen, publication


def _freeze_scorer(
    model: torch.nn.Module,
    publication: TorchBehaviorPublication,
    validator: Callable[[RaggedCandidateScorer], None] | None,
) -> RaggedCandidateScorer:
    if not isinstance(model, RaggedCandidateScorer):
        raise TorchBehaviorError(
            "checkpoint factory did not create a RaggedCandidateScorer"
        )
    if model.schema.version != publication.manifest.semantic_schema_version:
        raise TorchBehaviorError(
            "restored scorer schema version does not match publication"
        )
    model.eval()
    model.requires_grad_(False)
    if validator is not None:
        validator(model)
    return model


def _require_behavior_rule(
    manifest: BehaviorManifest,
    expected: BehaviorRuleBinding,
    name: str,
) -> None:
    if manifest.behavior_rule != expected:
        raise TorchBehaviorError(
            f"behavior manifest is not bound to the {name}"
        )


def _validate_categorical_inputs(
    config: RaggedCategoricalPolicyConfig,
    generator: torch.Generator,
) -> None:
    if not isinstance(config, RaggedCategoricalPolicyConfig):
        raise TorchBehaviorError("categorical policy requires typed config")
    if not isinstance(generator, torch.Generator):
        raise TorchBehaviorError("categorical policy requires an injected generator")
    if generator is torch.default_generator:
        raise TorchBehaviorError("categorical policy refuses the global generator")


def _require_generator_device(
    scorer: RaggedCandidateScorer,
    generator: torch.Generator,
) -> None:
    model_device = next(scorer.parameters()).device
    generator_device = torch.device(generator.device)
    if generator_device.type != model_device.type:
        raise TorchBehaviorError(
            "categorical generator device type must match the restored scorer"
        )


def _decision_slots(decision_batch: Mapping[str, object]) -> tuple[int, ...]:
    try:
        raw_slots = decision_batch["slot_indices"]
        values = tuple(raw_slots)  # type: ignore[arg-type]
    except (KeyError, TypeError) as error:
        raise TorchBehaviorError(
            "slot-bound policy requires decision slot indices"
        ) from error
    slots: list[int] = []
    for value in values:
        if isinstance(value, bool):
            raise TorchBehaviorError("decision slot index must be an integer")
        try:
            slot = operator.index(value)
        except TypeError as error:
            raise TorchBehaviorError(
                "decision slot index must be an integer"
            ) from error
        if slot < 0:
            raise TorchBehaviorError("decision slot index must be non-negative")
        slots.append(slot)
    if not slots:
        raise TorchBehaviorError(
            "slot-bound policy requires at least one decision row"
        )
    if len(set(slots)) != len(slots):
        raise TorchBehaviorError(
            "slot-bound policy decision slots contain duplicates"
        )
    return tuple(slots)


def _training_step(value: object) -> int:
    if isinstance(value, bool):
        raise TorchBehaviorError("training step must be an integer")
    try:
        step = operator.index(value)
    except TypeError as error:
        raise TorchBehaviorError("training step must be an integer") from error
    if step < 0:
        raise TorchBehaviorError("training step must be non-negative")
    return step
