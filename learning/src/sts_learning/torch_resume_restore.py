"""Restore one complete categorical generation owner graph from a manifest."""

from __future__ import annotations

import operator
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import Protocol

import torch

from .attempts import AttemptAssemblyLimits, BoundedAttemptAssembler
from .attempt_batching import (
    AttemptUpdateBatchLimits,
    BoundedAttemptUpdateBatcher,
)
from .driver import (
    BatchCurriculum,
    BatchEnvironment,
    CheckpointBatch,
    InitialPopulation,
    OnlineBatchDriver,
)
from .decision_progress import BridgeDecisionProgressProvider
from .experience import ExperienceLimits, ExperienceSegmentBuffer
from .recovery import RecoveryLedger
from .resume_store import (
    BoundedResumeStore,
    ResumeComponentKind,
    ResumeManifestId,
)
from .semantic_concat import SemanticBatchConcatLimits
from .terminal_returns import OnPolicyObjectiveConfig
from .torch_behavior import (
    CategoricalTorchBehaviorController,
    TorchBehaviorPublication,
)
from .torch_generation import (
    BoundedCategoricalGenerationRunner,
    CategoricalGenerationResumeBoundary,
)
from .torch_policy import RaggedCandidateScorer
from .torch_provenance import categorical_trainer_implementation
from .torch_resume import (
    hydrate_fresh_optimizer,
    materialize_generator_state,
    materialize_shadow_model_state,
)
from .torch_resume_metadata import decode_generation_resume_state
from .torch_resume_publication import CategoricalResumePayloadLimits
from .torch_training import SynchronousPolicyTrainer


class TorchResumeRestoreError(RuntimeError):
    """A durable manifest could not produce one complete fresh owner graph."""


class EnvironmentCheckpointDecoder(Protocol):
    """Fresh environment decoder implemented by the standalone bridge."""

    def __call__(
        self,
        payload: bytes,
        *,
        expected_slots: int,
        max_bytes: int,
    ) -> BatchEnvironment: ...


class CheckpointBankDecoder(Protocol):
    """Fresh episode-root bank decoder implemented by the standalone bridge."""

    def __call__(
        self,
        payload: bytes,
        *,
        expected_slot_indices: list[int],
        max_bytes: int,
    ) -> CheckpointBatch: ...


@dataclass(frozen=True)
class CategoricalResumeRestoreFactories:
    """Factories for every process-local owner excluded from the manifest."""

    environment_from_checkpoint: EnvironmentCheckpointDecoder
    checkpoint_bank_from_checkpoint: CheckpointBankDecoder
    shadow_scorer: Callable[[], RaggedCandidateScorer]
    optimizer: Callable[[RaggedCandidateScorer], torch.optim.Optimizer]
    controller: Callable[
        [torch.Generator],
        CategoricalTorchBehaviorController,
    ]

    def __post_init__(self) -> None:
        for name in (
            "environment_from_checkpoint",
            "checkpoint_bank_from_checkpoint",
            "shadow_scorer",
            "optimizer",
            "controller",
        ):
            if not callable(getattr(self, name)):
                raise TorchResumeRestoreError(f"{name} must be callable")


@dataclass(frozen=True)
class CategoricalResumeRestoreConfig:
    """Typed runtime configuration that must be supplied again after restart."""

    factories: CategoricalResumeRestoreFactories
    curriculum: BatchCurriculum
    experience_limits: ExperienceLimits
    attempt_limits: AttemptAssemblyLimits
    attempt_update_limits: AttemptUpdateBatchLimits
    concat_limits: SemanticBatchConcatLimits
    objective: OnPolicyObjectiveConfig
    payload_limits: CategoricalResumePayloadLimits
    expected_generator_device_type: str
    max_decision_rounds_per_step: int = 256

    def __post_init__(self) -> None:
        if not isinstance(self.factories, CategoricalResumeRestoreFactories):
            raise TorchResumeRestoreError("resume factories must be typed")
        if not callable(getattr(self.curriculum, "plan_recovery", None)):
            raise TorchResumeRestoreError("resume curriculum must plan recovery")
        for name, expected in (
            ("experience_limits", ExperienceLimits),
            ("attempt_limits", AttemptAssemblyLimits),
            ("attempt_update_limits", AttemptUpdateBatchLimits),
            ("concat_limits", SemanticBatchConcatLimits),
            ("objective", OnPolicyObjectiveConfig),
            ("payload_limits", CategoricalResumePayloadLimits),
        ):
            if not isinstance(getattr(self, name), expected):
                raise TorchResumeRestoreError(f"{name} must be typed")
        device = self.expected_generator_device_type
        if type(device) is not str or not device:
            raise TorchResumeRestoreError(
                "expected_generator_device_type must be a non-empty string"
            )
        object.__setattr__(
            self,
            "max_decision_rounds_per_step",
            _positive_integer(
                self.max_decision_rounds_per_step,
                "max_decision_rounds_per_step",
            ),
        )


@dataclass(frozen=True)
class RestoredCategoricalGeneration:
    """One fully validated fresh runner and its exact durable provenance."""

    manifest_id: ResumeManifestId
    runner: BoundedCategoricalGenerationRunner
    active_behavior: TorchBehaviorPublication
    boundary: CategoricalGenerationResumeBoundary


class CategoricalGenerationResumeRestorer:
    """Resolve six components and expose owners only after end-to-end validation."""

    def __init__(
        self,
        store: BoundedResumeStore,
        config: CategoricalResumeRestoreConfig,
    ) -> None:
        if not isinstance(store, BoundedResumeStore):
            raise TorchResumeRestoreError("resume restorer requires a resume store")
        if not isinstance(config, CategoricalResumeRestoreConfig):
            raise TorchResumeRestoreError("resume restore config must be typed")
        self.store = store
        self.config = config

    def restore(
        self,
        manifest_id: ResumeManifestId,
    ) -> RestoredCategoricalGeneration:
        """Build owners and return them only after matching the saved metadata."""

        if not isinstance(manifest_id, ResumeManifestId):
            raise TorchResumeRestoreError("resume manifest identity must be typed")
        try:
            return self._restore(manifest_id)
        except TorchResumeRestoreError:
            raise
        except Exception as error:
            raise TorchResumeRestoreError(str(error)) from error

    def _restore(
        self,
        manifest_id: ResumeManifestId,
    ) -> RestoredCategoricalGeneration:
        payloads = self.store.resolve(manifest_id)
        limits = self.config.payload_limits
        saved = decode_generation_resume_state(
            _component(payloads, ResumeComponentKind.GENERATION_METADATA),
            max_bytes=limits.max_metadata_bytes,
        )
        driver_state = saved.boundary.driver
        slot_indices = list(range(driver_state.slot_count))

        environment = self.config.factories.environment_from_checkpoint(
            _component(payloads, ResumeComponentKind.ENVIRONMENT),
            expected_slots=driver_state.slot_count,
            max_bytes=limits.max_environment_bytes,
        )
        checkpoint_bank = self.config.factories.checkpoint_bank_from_checkpoint(
            _component(payloads, ResumeComponentKind.EPISODE_ROOT_BANK),
            expected_slot_indices=slot_indices,
            max_bytes=limits.max_episode_root_bank_bytes,
        )
        shadow = materialize_shadow_model_state(
            _component(payloads, ResumeComponentKind.SHADOW_MODEL),
            self.config.factories.shadow_scorer,
            max_bytes=limits.max_shadow_model_bytes,
        )
        if not isinstance(shadow, RaggedCandidateScorer):
            raise TorchResumeRestoreError(
                "shadow scorer factory did not create a RaggedCandidateScorer"
            )
        optimizer = self.config.factories.optimizer(shadow)
        hydrate_fresh_optimizer(
            optimizer,
            _component(payloads, ResumeComponentKind.OPTIMIZER),
            max_bytes=limits.max_optimizer_bytes,
        )
        generator = materialize_generator_state(
            _component(payloads, ResumeComponentKind.CATEGORICAL_GENERATOR),
            expected_device_type=self.config.expected_generator_device_type,
            max_bytes=limits.max_generator_bytes,
        )
        controller = self.config.factories.controller(generator)
        if not isinstance(controller, CategoricalTorchBehaviorController):
            raise TorchResumeRestoreError(
                "controller factory did not create a categorical controller"
            )
        if controller.generator is not generator:
            raise TorchResumeRestoreError(
                "restored controller does not own the restored generator"
            )
        controller.bind_progress_provider(
            BridgeDecisionProgressProvider(environment)
        )
        controller_state = saved.boundary.controller
        active_manifest_id = controller_state.active_manifest_id
        if active_manifest_id is None:
            raise TorchResumeRestoreError("resume metadata has no active behavior")
        active_behavior = controller.recover_and_promote(
            active_manifest_id,
            successful_promotions=controller_state.successful_promotions,
        )
        expected_behavior = controller.publisher.template.bind(
            active_behavior.checkpoint_id,
            training_step=active_behavior.manifest.training_step,
        )
        if expected_behavior != active_behavior.manifest:
            raise TorchResumeRestoreError(
                "restored behavior manifest conflicts with runtime provenance"
            )
        if (
            active_behavior.manifest.trainer_implementation
            != categorical_trainer_implementation(
                self.config.objective,
            )
        ):
            raise TorchResumeRestoreError(
                "restored behavior conflicts with trainer configuration"
            )

        registry = controller.publisher.registry
        trainer = SynchronousPolicyTrainer(
            shadow,
            optimizer,
            registry,
            self.config.concat_limits,
            controller.config,
            self.config.objective,
            behavior_rule=controller.behavior_rule,
            resume_snapshot=saved.boundary.trainer,
        )
        update_batcher = BoundedAttemptUpdateBatcher(
            self.config.objective.attempts_per_update,
            self.config.attempt_update_limits,
            trainer,
        )
        assembler = BoundedAttemptAssembler(
            self.config.attempt_limits,
            update_batcher,
            resume_snapshot=saved.boundary.assembler,
        )
        sequence_index = driver_state.experience_next_sequence_index
        if sequence_index is None:
            raise TorchResumeRestoreError(
                "generation resume has no experience sequence index"
            )
        experience_buffer = ExperienceSegmentBuffer(
            self.config.experience_limits,
            next_sequence_index=sequence_index,
        )
        ledger = RecoveryLedger.from_active_snapshots(
            driver_state.ledger_snapshots,
            mode=driver_state.recovery_mode,
            max_recoveries_per_episode=driver_state.max_recoveries_per_episode,
        )
        population = InitialPopulation(
            env=environment,
            ledger=ledger,
            schedule=driver_state.schedule,
            checkpoint_bank=checkpoint_bank,
        )
        driver = OnlineBatchDriver(
            population,
            policy=controller,
            curriculum=self.config.curriculum,
            max_decision_rounds_per_step=(
                self.config.max_decision_rounds_per_step
            ),
            experience_buffer=experience_buffer,
            experience_sink=assembler,
            decision_progress_provider=BridgeDecisionProgressProvider(
                population.env
            ),
        )
        runner = BoundedCategoricalGenerationRunner(
            driver,
            assembler,
            update_batcher,
            trainer,
            controller,
            shadow,
            optimizer_steps_per_generation=saved.optimizer_steps_per_generation,
        )
        restored_boundary = runner.require_resume_boundary()
        if restored_boundary != saved.boundary:
            raise TorchResumeRestoreError(
                "restored owner graph does not reproduce the saved boundary"
            )
        return RestoredCategoricalGeneration(
            manifest_id=manifest_id,
            runner=runner,
            active_behavior=active_behavior,
            boundary=restored_boundary,
        )


def _component(
    payloads: Mapping[ResumeComponentKind, bytes],
    kind: ResumeComponentKind,
) -> bytes:
    try:
        payload = payloads[kind]
    except KeyError as error:
        raise TorchResumeRestoreError(
            f"resume manifest is missing {kind.name.lower()}"
        ) from error
    if not isinstance(payload, bytes):
        raise TorchResumeRestoreError(
            f"resume component {kind.name.lower()} is not immutable bytes"
        )
    return payload


def _positive_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchResumeRestoreError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchResumeRestoreError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise TorchResumeRestoreError(f"{name} must be positive")
    return normalized
