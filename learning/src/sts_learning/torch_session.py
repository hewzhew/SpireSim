"""Compact new/restore boundary for one categorical online learning session."""

from __future__ import annotations

import operator
from dataclasses import dataclass
from pathlib import Path

import torch

from .attempts import BoundedAttemptAssembler
from .attempt_batching import BoundedAttemptUpdateBatcher
from .driver import (
    BatchCurriculum,
    OnlineBatchDriver,
    RecoveryPlan,
    initialize_population,
)
from .decision_progress import BridgeDecisionProgressProvider
from .experience import ExperienceSegmentBuffer
from .manifest_catalog import (
    BoundedBehaviorManifestCatalog,
)
from .manifests import BehaviorManifestRegistry
from .policy import BehaviorManifestId
from .resume_store import BoundedResumeStore, ResumeManifestId
from .torch_behavior import (
    CategoricalTorchBehaviorController,
    CheckpointedCategoricalTorchPolicy,
    FrozenCombatAnchor,
    FrozenCombatGreedyTorchPolicy,
    FrozenDecisionRule,
    TorchBehaviorPublisher,
)
from .torch_checkpoints import BoundedTorchCheckpointStore
from .torch_generation import (
    BoundedCategoricalGenerationRunner,
    CategoricalGenerationAdvanceResult,
)
from .torch_policy import (
    RaggedCandidateScorer,
    TorchPolicyError,
    load_scorer_warm_start,
)
from .torch_provenance import categorical_training_manifest_template
from .torch_resume_publication import (
    CategoricalGenerationResumePublisher,
    CategoricalResumePublication,
)
from .torch_resume_restore import (
    CategoricalGenerationResumeRestorer,
    CategoricalResumeRestoreConfig,
    CategoricalResumeRestoreFactories,
)
from .torch_session_config import (
    CategoricalOnlineSessionConfig,
    CategoricalSessionBridge,
    TorchSessionError,
)
from .torch_training import SynchronousPolicyTrainer


class NoRecoveryCurriculum:
    """Explicit zero-recovery baseline curriculum."""

    def plan_recovery(self, accounting, snapshots) -> RecoveryPlan:
        return RecoveryPlan()


@dataclass(frozen=True)
class CategoricalTrainingAdvanceResult:
    """Aggregate-only result for one bounded multi-generation continuation."""

    active_manifest_id_before: BehaviorManifestId
    active_manifest_id_after: BehaviorManifestId
    active_training_step_before: int
    active_training_step_after: int
    target_generations: int
    completed_generations: int
    batch_step_limit_per_generation: int
    batch_steps: int
    terminal_attempts: int
    terminal_flushes: int
    optimizer_steps_before: int
    optimizer_steps_after: int

    @property
    def complete(self) -> bool:
        return self.completed_generations == self.target_generations

    @property
    def step_limit_reached(self) -> bool:
        return not self.complete


class CategoricalOnlineSession:
    """One live runner with explicit progress and durable publication."""

    def __init__(
        self,
        runner: BoundedCategoricalGenerationRunner,
        resume_publisher: CategoricalGenerationResumePublisher,
    ) -> None:
        if not isinstance(runner, BoundedCategoricalGenerationRunner):
            raise TorchSessionError("session requires a generation runner")
        if not isinstance(resume_publisher, CategoricalGenerationResumePublisher):
            raise TorchSessionError("session requires a resume publisher")
        self.runner = runner
        self.resume_publisher = resume_publisher

    @property
    def active_behavior_manifest_id(self) -> BehaviorManifestId:
        active = self.runner.controller.snapshot.active_manifest_id
        if active is None:
            raise TorchSessionError("session has no active behavior")
        return active

    def publish(self) -> CategoricalResumePublication:
        return self.resume_publisher.publish(self.runner)

    def advance_generation(
        self,
        *,
        max_batch_steps: int,
    ) -> CategoricalGenerationAdvanceResult:
        """Advance live training without implicitly writing a checkpoint."""

        return self.runner.advance(max_batch_steps=max_batch_steps)

    def advance_generations(
        self,
        *,
        generations: int,
        max_batch_steps_per_generation: int,
    ) -> CategoricalTrainingAdvanceResult:
        """Advance whole live generations until the first bounded incomplete one."""

        target = _non_negative_integer(generations, "generations")
        step_limit = _non_negative_integer(
            max_batch_steps_per_generation,
            "max_batch_steps_per_generation",
        )
        controller_before = self.runner.controller.snapshot
        trainer_before = self.runner.trainer.snapshot
        manifest_before = controller_before.active_manifest_id
        training_step_before = controller_before.active_training_step
        if manifest_before is None or training_step_before is None:
            raise TorchSessionError("session has no active behavior")

        completed = 0
        batch_steps = 0
        terminal_attempts = 0
        terminal_flushes = 0
        while completed < target:
            result = self.advance_generation(max_batch_steps=step_limit)
            batch_steps += result.batch_steps
            terminal_attempts += result.terminal_attempts
            terminal_flushes += result.terminal_flushes
            if not result.promoted:
                break
            completed += 1

        controller_after = self.runner.controller.snapshot
        trainer_after = self.runner.trainer.snapshot
        manifest_after = controller_after.active_manifest_id
        training_step_after = controller_after.active_training_step
        if manifest_after is None or training_step_after is None:
            raise TorchSessionError("session lost its active behavior")
        if (
            controller_after.successful_promotions
            - controller_before.successful_promotions
            != completed
        ):
            raise TorchSessionError(
                "multi-generation promotion count disagrees with completed generations"
            )
        return CategoricalTrainingAdvanceResult(
            active_manifest_id_before=manifest_before,
            active_manifest_id_after=manifest_after,
            active_training_step_before=training_step_before,
            active_training_step_after=training_step_after,
            target_generations=target,
            completed_generations=completed,
            batch_step_limit_per_generation=step_limit,
            batch_steps=batch_steps,
            terminal_attempts=terminal_attempts,
            terminal_flushes=terminal_flushes,
            optimizer_steps_before=trainer_before.optimizer_steps,
            optimizer_steps_after=trainer_after.optimizer_steps,
        )


class CategoricalOnlineSessionFactory:
    """Own all repetitive new/restore wiring below one experiment root."""

    def __init__(
        self,
        root: str | Path,
        bridge: CategoricalSessionBridge,
        config: CategoricalOnlineSessionConfig,
        curriculum: BatchCurriculum,
    ) -> None:
        if not isinstance(bridge, CategoricalSessionBridge):
            raise TorchSessionError("session bridge must be typed")
        if not isinstance(config, CategoricalOnlineSessionConfig):
            raise TorchSessionError("session config must be typed")
        if not callable(getattr(curriculum, "plan_recovery", None)):
            raise TorchSessionError("session curriculum must plan recovery")
        if (
            isinstance(curriculum, NoRecoveryCurriculum)
            and config.max_recoveries_per_episode != 0
        ):
            raise TorchSessionError(
                "zero-recovery curriculum requires a zero recovery budget"
            )
        self.root = Path(root).resolve()
        if self.root.exists() and not self.root.is_dir():
            raise TorchSessionError("session root is not a directory")
        self.root.mkdir(parents=True, exist_ok=True)
        self.bridge = bridge
        self.config = config
        self.curriculum = curriculum
        profile = config.profile
        self.template = categorical_training_manifest_template(
            bridge.semantic_schema,
            profile.scorer,
            profile.behavior,
            profile.optimizer,
            profile.objective,
            device_type=profile.device_type,
            behavior_rule=profile.behavior_rule,
        )

    def new(
        self,
        *,
        model_seed: int,
        behavior_seed: int,
        initial_scorer: RaggedCandidateScorer | None = None,
        initial_scorer_actor_only: bool = False,
        combat_anchor: FrozenCombatAnchor | None = None,
    ) -> CategoricalOnlineSession:
        """Create generation zero only in an unused experiment root."""

        model_seed = _torch_seed(model_seed, "model_seed")
        behavior_seed = _torch_seed(behavior_seed, "behavior_seed")
        if any(self.root.iterdir()):
            raise TorchSessionError("new session requires an unused experiment root")
        if initial_scorer is not None and not isinstance(
            initial_scorer,
            RaggedCandidateScorer,
        ):
            raise TorchSessionError(
                "session initial_scorer must be a RaggedCandidateScorer"
            )
        if type(initial_scorer_actor_only) is not bool:
            raise TorchSessionError("initial_scorer_actor_only must be bool")
        expected_anchor_id = self.config.profile.combat_anchor_manifest_id
        if (combat_anchor is None) != (expected_anchor_id is None):
            raise TorchSessionError(
                "session combat anchor conflicts with its profile"
            )
        if combat_anchor is not None:
            if not isinstance(combat_anchor, FrozenCombatAnchor):
                raise TorchSessionError("session combat anchor must be typed")
            if combat_anchor.manifest_id != expected_anchor_id:
                raise TorchSessionError(
                    "session combat anchor manifest identity changed"
                )
            if (
                combat_anchor.scorer.config
                != self.config.profile.combat_anchor_scorer
            ):
                raise TorchSessionError(
                    "session combat anchor scorer configuration changed"
                )
        checkpoint_store, catalog = self._behavior_stores()
        resume_store = self._resume_store()
        if (
            checkpoint_store.snapshot.checkpoints != 0
            or catalog.snapshot.manifests != 0
            or resume_store.snapshot.components != 0
            or resume_store.snapshot.manifests != 0
        ):
            raise TorchSessionError("new session requires an unused experiment root")
        with torch.random.fork_rng(devices=[]):
            torch.manual_seed(model_seed)
            shadow = self._scorer()
        if initial_scorer is not None:
            try:
                load_scorer_warm_start(
                    shadow,
                    initial_scorer,
                    actor_only=initial_scorer_actor_only,
                )
            except TorchPolicyError as error:
                raise TorchSessionError(
                    "session initial scorer is incompatible with the maintained profile"
                ) from error
        population = initialize_population(
            lambda seeds: self.bridge.environment(
                seeds,
                self.config.ascension_level,
            ),
            slot_count=self.config.slot_count,
            schedule=self.config.schedule,
            max_recoveries_per_episode=(
                self.config.max_recoveries_per_episode
            ),
        )
        registry = BehaviorManifestRegistry(
            capacity=self.config.limits.owner_capacity
        )
        controller = self._controller(
            torch.Generator(
                device=self.config.profile.device_type
            ).manual_seed(behavior_seed),
            checkpoint_store,
            catalog,
            registry,
            combat_anchor=combat_anchor,
        )
        controller.bind_progress_provider(
            BridgeDecisionProgressProvider(population.env)
        )
        optimizer = self.config.profile.optimizer.create(shadow.parameters())
        trainer = SynchronousPolicyTrainer(
            shadow,
            optimizer,
            registry,
            self.config.limits.concat,
            self.config.profile.behavior,
            self.config.profile.objective,
            behavior_rule=self.config.profile.behavior_rule,
        )
        update_batcher = BoundedAttemptUpdateBatcher(
            self.config.profile.objective.attempts_per_update,
            self.config.limits.attempt_updates,
            trainer,
        )
        assembler = BoundedAttemptAssembler(
            self.config.limits.attempts,
            update_batcher,
        )
        controller.promote_live(shadow, training_step=0)
        driver = OnlineBatchDriver(
            population,
            policy=controller,
            curriculum=self.curriculum,
            max_decision_rounds_per_step=(
                self.config.max_decision_rounds_per_step
            ),
            experience_buffer=ExperienceSegmentBuffer(
                self.config.limits.experience
            ),
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
            optimizer_steps_per_generation=(
                self.config.profile.optimizer_steps_per_generation
            ),
        )
        if combat_anchor is not None:
            self._import_combat_anchor(
                combat_anchor,
                checkpoint_store,
                catalog,
            )
        return self._session(runner, resume_store)

    def restore(self, manifest_id: ResumeManifestId) -> CategoricalOnlineSession:
        """Reopen stores and rebuild one complete fresh runner."""

        resume_store = self._resume_store()
        restored = CategoricalGenerationResumeRestorer(
            resume_store,
            CategoricalResumeRestoreConfig(
                factories=CategoricalResumeRestoreFactories(
                    environment_from_checkpoint=(
                        self.bridge.environment_from_checkpoint
                    ),
                    checkpoint_bank_from_checkpoint=(
                        self.bridge.checkpoint_bank_from_checkpoint
                    ),
                    shadow_scorer=self._scorer,
                    optimizer=lambda scorer: self.config.profile.optimizer.create(
                        scorer.parameters()
                    ),
                    controller=self._fresh_controller,
                ),
                curriculum=self.curriculum,
                experience_limits=self.config.limits.experience,
                attempt_limits=self.config.limits.attempts,
                attempt_update_limits=self.config.limits.attempt_updates,
                concat_limits=self.config.limits.concat,
                objective=self.config.profile.objective,
                payload_limits=self.config.limits.resume_payloads,
                expected_generator_device_type=(
                    self.config.profile.device_type
                ),
                max_decision_rounds_per_step=(
                    self.config.max_decision_rounds_per_step
                ),
            ),
        ).restore(manifest_id)
        driver_state = restored.boundary.driver
        if driver_state.slot_count != self.config.slot_count:
            raise TorchSessionError(
                "restored slot_count conflicts with the session config"
            )
        if (
            driver_state.max_recoveries_per_episode
            != self.config.max_recoveries_per_episode
        ):
            raise TorchSessionError(
                "restored recovery budget conflicts with the session config"
            )
        if (
            driver_state.schedule.partition is not self.config.schedule.partition
            or driver_state.schedule.spec != self.config.schedule.spec
        ):
            raise TorchSessionError(
                "restored seed partition conflicts with the session config"
            )
        return self._session(restored.runner, resume_store)

    def recover_behavior(
        self,
        manifest_id: BehaviorManifestId,
        *,
        behavior_seed: int,
    ) -> CheckpointedCategoricalTorchPolicy | FrozenCombatGreedyTorchPolicy:
        """Materialize one frozen behavior for held-out evaluation."""

        seed = _torch_seed(behavior_seed, "behavior_seed")
        checkpoint_store, catalog = self._behavior_stores()
        generator = torch.Generator(
            device=self.config.profile.device_type
        ).manual_seed(seed)
        if (
            self.config.profile.combat_decision_rule
            is FrozenDecisionRule.SAMPLED
        ):
            return CheckpointedCategoricalTorchPolicy.recover(
                manifest_id,
                checkpoint_store,
                catalog,
                BehaviorManifestRegistry(capacity=1),
                self._scorer,
                self.config.profile.behavior,
                generator,
            )
        return FrozenCombatGreedyTorchPolicy.recover(
            manifest_id,
            checkpoint_store,
            catalog,
            BehaviorManifestRegistry(capacity=1),
            self._scorer,
            self.config.profile.behavior,
            generator,
            combat_anchor=self._recover_combat_anchor(
                checkpoint_store,
                catalog,
            ),
        )

    def _scorer(self) -> RaggedCandidateScorer:
        return RaggedCandidateScorer.from_bridge_schema(
            self.bridge.semantic_schema,
            self.config.profile.scorer,
        ).to(self.config.profile.device_type)

    def _fresh_controller(
        self,
        generator: torch.Generator,
    ) -> CategoricalTorchBehaviorController:
        checkpoint_store, catalog = self._behavior_stores()
        registry = BehaviorManifestRegistry(
            capacity=self.config.limits.owner_capacity
        )
        return self._controller(
            generator,
            checkpoint_store,
            catalog,
            registry,
            combat_anchor=self._recover_combat_anchor(
                checkpoint_store,
                catalog,
            ),
        )

    def _controller(
        self,
        generator: torch.Generator,
        checkpoint_store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
        registry: BehaviorManifestRegistry,
        *,
        combat_anchor: FrozenCombatAnchor | None = None,
    ) -> CategoricalTorchBehaviorController:
        return CategoricalTorchBehaviorController(
            TorchBehaviorPublisher(
                checkpoint_store,
                catalog,
                registry,
                self.template,
            ),
            self._scorer,
            self.config.profile.behavior,
            generator,
            combat_decision_rule=(
                self.config.profile.combat_decision_rule
            ),
            combat_anchor=combat_anchor,
        )

    def _import_combat_anchor(
        self,
        combat_anchor: FrozenCombatAnchor,
        checkpoint_store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
    ) -> None:
        prepared_checkpoint = checkpoint_store.prepare(combat_anchor.scorer)
        if prepared_checkpoint.artifact_id != combat_anchor.checkpoint_id:
            raise TorchSessionError("combat anchor checkpoint identity changed")
        prepared_manifest = catalog.prepare(combat_anchor.binding.manifest)
        if prepared_manifest.manifest_id != combat_anchor.manifest_id:
            raise TorchSessionError("combat anchor manifest identity changed")
        checkpoint_store.preview_commit(prepared_checkpoint)
        catalog.preview_commit(prepared_manifest)
        if (
            checkpoint_store.commit(prepared_checkpoint)
            != combat_anchor.checkpoint_id
        ):
            raise TorchSessionError("combat anchor checkpoint commit changed identity")
        if catalog.commit(prepared_manifest) != combat_anchor.manifest_id:
            raise TorchSessionError("combat anchor manifest commit changed identity")

    def _recover_combat_anchor(
        self,
        checkpoint_store: BoundedTorchCheckpointStore,
        catalog: BoundedBehaviorManifestCatalog,
    ) -> FrozenCombatAnchor | None:
        manifest_id = self.config.profile.combat_anchor_manifest_id
        if manifest_id is None:
            return None
        return FrozenCombatAnchor.recover(
            manifest_id,
            checkpoint_store,
            catalog,
            self._combat_anchor_scorer,
        )

    def _combat_anchor_scorer(self) -> RaggedCandidateScorer:
        config = self.config.profile.combat_anchor_scorer
        if config is None:
            raise TorchSessionError("session has no combat anchor scorer config")
        return RaggedCandidateScorer.from_bridge_schema(
            self.bridge.semantic_schema,
            config,
        ).to(self.config.profile.device_type)

    def _behavior_stores(
        self,
    ) -> tuple[BoundedTorchCheckpointStore, BoundedBehaviorManifestCatalog]:
        limits = self.config.limits
        return (
            BoundedTorchCheckpointStore(
                self.root / "behavior-checkpoints",
                limits.checkpoint_store,
            ),
            BoundedBehaviorManifestCatalog(
                self.root / "behavior-manifests",
                limits.manifest_catalog,
            ),
        )

    def _resume_store(self) -> BoundedResumeStore:
        return BoundedResumeStore(
            self.root / "resume",
            self.config.limits.resume_store,
        )

    def _session(
        self,
        runner: BoundedCategoricalGenerationRunner,
        resume_store: BoundedResumeStore,
    ) -> CategoricalOnlineSession:
        return CategoricalOnlineSession(
            runner,
            CategoricalGenerationResumePublisher(
                resume_store,
                self.config.limits.resume_payloads,
            ),
        )


def _torch_seed(value: object, name: str) -> int:
    normalized = _non_negative_integer(value, name)
    if normalized >= 1 << 63:
        raise TorchSessionError(f"{name} must be below 2^63")
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
