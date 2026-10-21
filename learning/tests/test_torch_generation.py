from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

from learning.tests.driver_fixtures import (
    NoRecovery,
    NumpyFakeBatchEnv,
    NumpyWinningBatchEnv,
)
from learning.tests.torch_outcome_fixtures import (
    behavior_manifest_template_fixture,
    decision_batch_fixture,
)
from sts_learning import (
    AttemptAssemblyLimits,
    AttemptUpdateBatchLimits,
    BehaviorManifestCatalogLimits,
    BehaviorManifestRegistry,
    BridgeDecisionProgressProvider,
    BoundedAttemptAssembler,
    BoundedAttemptUpdateBatcher,
    BoundedBehaviorManifestCatalog,
    ExperienceLimits,
    ExperienceSegmentBuffer,
    EpisodeRootRetryCurriculum,
    FloorProgressReturnConfig,
    OnlineBatchDriver,
    OnPolicyObjectiveConfig,
    SeedPartition,
    SeedSchedule,
    SemanticBatchConcatLimits,
    initialize_population,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None

if _TORCH_AVAILABLE:
    import torch

    from learning.tests.semantic_fixtures import semantic_schema_fixture
    from sts_learning.torch_behavior import (
        CategoricalTorchBehaviorController,
        TorchBehaviorPublisher,
    )
    from sts_learning.torch_checkpoints import (
        BoundedTorchCheckpointStore,
        TorchCheckpointLimits,
    )
    from sts_learning.torch_generation import (
        BoundedCategoricalGenerationRunner,
        TorchGenerationError,
    )
    from sts_learning.torch_policy import (
        RaggedCandidateScorer,
        RaggedCategoricalPolicyConfig,
        RaggedScorerConfig,
    )
    from sts_learning.torch_resume_metadata import (
        decode_generation_resume_state,
        encode_generation_resume_state,
    )
    from sts_learning.torch_provenance import categorical_trainer_implementation
    from sts_learning.resume_store import (
        BoundedResumeStore,
        ResumeComponentKind,
        ResumeStoreLimits,
    )
    from sts_learning.torch_resume_publication import (
        CategoricalGenerationResumePublisher,
        CategoricalResumePayloadLimits,
    )
    from sts_learning.torch_training import SynchronousPolicyTrainer


class _NumpyScheduledBatchEnv(NumpyWinningBatchEnv):
    """Full semantic fixture that honors caller-provided terminal plans."""

    def step(self) -> dict[str, object]:
        return NumpyFakeBatchEnv.step(self)


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class BoundedCategoricalGenerationRunnerTests(unittest.TestCase):
    def test_miswired_generation_owners_fail_before_environment_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            driver, assembler, batcher, trainer, controller, shadow = _components(
                Path(root)
            )
            disconnected = BoundedAttemptAssembler(
                _attempt_limits(),
                batcher,
            )

            with self.assertRaisesRegex(TorchGenerationError, "not wired"):
                BoundedCategoricalGenerationRunner(
                    driver,
                    disconnected,
                    batcher,
                    trainer,
                    controller,
                    shadow,
                    optimizer_steps_per_generation=1,
                )

            self.assertEqual(driver.env.choose_calls, [])  # type: ignore[attr-defined]
            self.assertIs(assembler.completed_attempt_sink, batcher)

    def test_generation_optimizer_must_own_the_exact_shadow_parameters(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            other = _scorer()
            driver, assembler, batcher, trainer, controller, shadow = _components(
                Path(root),
                optimizer_model=other,
            )

            with self.assertRaisesRegex(
                TorchGenerationError,
                "exactly the model parameters",
            ):
                BoundedCategoricalGenerationRunner(
                    driver,
                    assembler,
                    batcher,
                    trainer,
                    controller,
                    shadow,
                    optimizer_steps_per_generation=1,
                )

            self.assertEqual(driver.env.choose_calls, [])  # type: ignore[attr-defined]

    def test_generation_return_config_must_match_active_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            driver, assembler, batcher, trainer, controller, shadow = _components(
                Path(root)
            )
            trainer.objective_config = OnPolicyObjectiveConfig(
                terminal_return=FloorProgressReturnConfig(target_floor=51),
                attempts_per_update=1,
            )

            with self.assertRaisesRegex(TorchGenerationError, "trainer implementation"):
                BoundedCategoricalGenerationRunner(
                    driver,
                    assembler,
                    batcher,
                    trainer,
                    controller,
                    shadow,
                    optimizer_steps_per_generation=1,
                )

            self.assertEqual(driver.env.choose_calls, [])  # type: ignore[attr-defined]

    def test_generation_target_and_step_limit_require_typed_counts(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            driver, assembler, batcher, trainer, controller, shadow = _components(
                Path(root)
            )
            with self.assertRaisesRegex(TorchGenerationError, "positive"):
                BoundedCategoricalGenerationRunner(
                    driver,
                    assembler,
                    batcher,
                    trainer,
                    controller,
                    shadow,
                    optimizer_steps_per_generation=0,
                )
            with self.assertRaisesRegex(TorchGenerationError, "exactly one"):
                BoundedCategoricalGenerationRunner(
                    driver,
                    assembler,
                    batcher,
                    trainer,
                    controller,
                    shadow,
                    optimizer_steps_per_generation=2,
                )
            runner = BoundedCategoricalGenerationRunner(
                driver,
                assembler,
                batcher,
                trainer,
                controller,
                shadow,
                optimizer_steps_per_generation=1,
            )

            with self.assertRaisesRegex(TorchGenerationError, "not bool"):
                runner.advance(max_batch_steps=True)

            driver.policy = object()  # type: ignore[assignment]
            with self.assertRaisesRegex(TorchGenerationError, "driver policy"):
                runner.advance(max_batch_steps=0)

            self.assertEqual(driver.env.choose_calls, [])  # type: ignore[attr-defined]

    def test_live_generations_do_not_consume_durable_owner_capacity(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            driver, assembler, batcher, trainer, controller, shadow = _components(
                Path(root),
                owner_capacity=1,
                environment=NumpyWinningBatchEnv,
            )
            runner = BoundedCategoricalGenerationRunner(
                driver,
                assembler,
                batcher,
                trainer,
                controller,
                shadow,
                optimizer_steps_per_generation=1,
            )

            first = runner.advance(max_batch_steps=1)
            second = runner.advance(max_batch_steps=1)

            self.assertTrue(first.promoted)
            self.assertTrue(second.promoted)
            self.assertEqual(trainer.snapshot.optimizer_steps, 2)
            self.assertEqual(controller.snapshot.active_training_step, 2)
            self.assertEqual(
                controller.publisher.registry.snapshot.registered_manifests,
                1,
            )
            self.assertEqual(controller.publisher.store.snapshot.checkpoints, 0)
            self.assertEqual(controller.publisher.catalog.snapshot.manifests, 0)

    def test_multi_slot_generations_promote_only_between_complete_cohorts(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            driver, assembler, batcher, trainer, controller, shadow = _components(
                Path(root),
                slot_count=2,
                attempts_per_update=4,
                environment=lambda seeds: _NumpyScheduledBatchEnv(
                    seeds,
                    terminal_plans=(
                        {0: -1},
                        {1: -1},
                        {0: -1, 1: -1},
                        {1: -1},
                        {0: -1},
                        {0: -1, 1: -1},
                    ),
                ),
            )
            runner = BoundedCategoricalGenerationRunner(
                driver,
                assembler,
                batcher,
                trainer,
                controller,
                shadow,
                optimizer_steps_per_generation=1,
            )

            first = runner.advance(max_batch_steps=3)
            second = runner.advance(max_batch_steps=3)

            self.assertTrue(first.promoted)
            self.assertTrue(second.promoted)
            self.assertEqual(first.terminal_attempts, 4)
            self.assertEqual(second.terminal_attempts, 4)
            self.assertEqual(trainer.snapshot.optimizer_steps, 2)
            self.assertEqual(controller.snapshot.active_training_step, 2)
            self.assertEqual(driver.env.terminal_count, 0)
            self.assertEqual(assembler.snapshot.open_attempts, 0)
            self.assertEqual(batcher.pending_attempts, 0)
            self.assertEqual(
                [slots for slots, _ in driver.env.reset_calls],  # type: ignore[attr-defined]
                [[0, 1], [0, 1], [0, 1], [0, 1]],
            )

    def test_single_slot_retry_generation_closes_after_an_early_victory(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            curriculum = EpisodeRootRetryCurriculum(
                attempts_per_update=3,
                attempts_per_episode=2,
            )
            driver, assembler, batcher, trainer, controller, shadow = _components(
                Path(root),
                attempts_per_update=3,
                max_recoveries_per_episode=1,
                curriculum=curriculum,
                environment=lambda seeds: _NumpyScheduledBatchEnv(
                    seeds,
                    terminal_plans=({0: 1}, {0: -1}, {0: -1}),
                ),
            )
            runner = BoundedCategoricalGenerationRunner(
                driver,
                assembler,
                batcher,
                trainer,
                controller,
                shadow,
                optimizer_steps_per_generation=1,
            )

            result = runner.advance(max_batch_steps=3)

            self.assertTrue(result.promoted)
            self.assertEqual(result.terminal_attempts, 3)
            self.assertEqual(result.sampled_episodes, 2)
            self.assertEqual(result.recoveries, 1)
            self.assertEqual(curriculum.attempts_in_update, 0)
            self.assertEqual(driver.env.restore_calls, [[0]])  # type: ignore[attr-defined]
            self.assertEqual(driver.env.terminal_count, 0)
            self.assertEqual(assembler.snapshot.open_attempts, 0)
            self.assertEqual(batcher.pending_attempts, 0)

    def test_resume_boundary_requires_flushed_and_closed_experience(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            driver, assembler, batcher, trainer, controller, shadow = _components(
                Path(root)
            )
            runner = BoundedCategoricalGenerationRunner(
                driver,
                assembler,
                batcher,
                trainer,
                controller,
                shadow,
                optimizer_steps_per_generation=1,
            )

            boundary = runner.require_resume_boundary()
            self.assertEqual(boundary.driver.slot_count, 1)
            self.assertEqual(boundary.driver.checkpoint_slots, 1)
            self.assertEqual(boundary.assembler.open_attempts, 0)
            self.assertEqual(boundary.trainer.optimizer_steps, 0)
            self.assertEqual(boundary.controller.active_training_step, 0)
            payload = encode_generation_resume_state(
                boundary,
                optimizer_steps_per_generation=runner.optimizer_steps_per_generation,
                max_bytes=1024 * 1024,
            )
            restored = decode_generation_resume_state(
                payload,
                max_bytes=1024 * 1024,
            )
            self.assertEqual(restored.boundary, boundary)
            self.assertEqual(restored.optimizer_steps_per_generation, 1)

            manifest_id = controller.snapshot.active_manifest_id
            assert manifest_id is not None
            buffer = driver._experience_buffer
            assert buffer is not None
            buffer.commit(
                decision_batch_fixture(
                    slot=0,
                    semantic_row=0,
                    selected_ordinal=0,
                    manifest_id=manifest_id,
                )
            )
            with self.assertRaisesRegex(TorchGenerationError, "flushed"):
                runner.require_resume_boundary()

            driver.flush_experience()
            with self.assertRaisesRegex(TorchGenerationError, "open attempt"):
                runner.require_resume_boundary()

    def test_safe_generation_publishes_all_components_and_manifest_last(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            (Path(root) / "behavior").mkdir()
            driver, assembler, batcher, trainer, controller, shadow = _components(
                Path(root) / "behavior"
            )
            runner = BoundedCategoricalGenerationRunner(
                driver,
                assembler,
                batcher,
                trainer,
                controller,
                shadow,
                optimizer_steps_per_generation=1,
            )
            store = BoundedResumeStore(
                Path(root) / "resume",
                ResumeStoreLimits(
                    max_components=6,
                    max_bytes_per_component=2 * 1024 * 1024,
                    max_total_component_bytes=12 * 1024 * 1024,
                    max_manifests=1,
                    max_bytes_per_manifest=1024,
                    max_total_manifest_bytes=1024,
                ),
            )
            publisher = CategoricalGenerationResumePublisher(
                store,
                CategoricalResumePayloadLimits(
                    max_environment_bytes=1024,
                    max_episode_root_bank_bytes=1024,
                    max_shadow_model_bytes=1024 * 1024,
                    max_optimizer_bytes=1024 * 1024,
                    max_generator_bytes=1024 * 1024,
                    max_metadata_bytes=1024 * 1024,
                ),
            )

            publication = publisher.publish(runner)
            self.assertEqual(store.snapshot.components, 6)
            self.assertEqual(store.snapshot.manifests, 1)
            resolved = store.resolve(publication.manifest_id)
            self.assertEqual(set(resolved), set(ResumeComponentKind))
            self.assertTrue(
                resolved[ResumeComponentKind.ENVIRONMENT].startswith(b"FAKE-ENV")
            )
            self.assertTrue(
                resolved[ResumeComponentKind.EPISODE_ROOT_BANK].startswith(
                    b"FAKE-BANK"
                )
            )
            self.assertEqual(publisher.publish(runner).manifest_id, publication.manifest_id)


def _components(
    root: Path,
    *,
    optimizer_model=None,
    owner_capacity: int = 2,
    environment=NumpyFakeBatchEnv,
    slot_count: int = 1,
    attempts_per_update: int = 1,
    max_recoveries_per_episode: int = 0,
    curriculum=None,
):
    shadow = _scorer()
    behavior_config = RaggedCategoricalPolicyConfig(temperature=0.8)
    return_config = FloorProgressReturnConfig()
    objective_config = OnPolicyObjectiveConfig(
        terminal_return=return_config,
        attempts_per_update=attempts_per_update,
    )
    registry = BehaviorManifestRegistry(capacity=owner_capacity)
    store = BoundedTorchCheckpointStore(
        root / "checkpoints",
        TorchCheckpointLimits(
            max_checkpoints=owner_capacity,
            max_bytes_per_checkpoint=2 * 1024 * 1024,
            max_total_bytes=owner_capacity * 2 * 1024 * 1024,
        ),
    )
    catalog = BoundedBehaviorManifestCatalog(
        root / "manifests",
        BehaviorManifestCatalogLimits(
            max_manifests=owner_capacity,
            max_bytes_per_manifest=1024,
            max_total_bytes=owner_capacity * 1024,
        ),
    )

    def scorer_factory():
        return _scorer()

    controller = CategoricalTorchBehaviorController(
        TorchBehaviorPublisher(
            store,
            catalog,
            registry,
            behavior_manifest_template_fixture(
                behavior_rule=behavior_config.behavior_rule,
                trainer_implementation=categorical_trainer_implementation(
                    objective_config,
                ),
            ),
        ),
        scorer_factory,
        behavior_config,
        torch.Generator().manual_seed(94),
    )
    controller.promote_live(shadow, training_step=0)
    optimizer_owner = shadow if optimizer_model is None else optimizer_model
    trainer = SynchronousPolicyTrainer(
        shadow,
        torch.optim.SGD(optimizer_owner.parameters(), lr=0.001),
        registry,
        SemanticBatchConcatLimits(
            max_rows=64,
            max_input_array_bytes=1024 * 1024,
        ),
        behavior_config,
        objective_config,
    )
    batcher = BoundedAttemptUpdateBatcher(
        objective_config.attempts_per_update,
        AttemptUpdateBatchLimits(
            max_decisions_per_update=64,
            max_payload_bytes_per_update=1024 * 1024,
        ),
        trainer,
    )
    assembler = BoundedAttemptAssembler(_attempt_limits(), batcher)
    population = initialize_population(
        environment,
        slot_count=slot_count,
        schedule=SeedSchedule(
            SeedPartition.TRAINING
            if max_recoveries_per_episode
            else SeedPartition.HELD_OUT
        ),
        max_recoveries_per_episode=max_recoveries_per_episode,
    )
    progress_provider = (
        BridgeDecisionProgressProvider(population.env)
        if callable(getattr(population.env, "public_run_contexts", None))
        else None
    )
    driver = OnlineBatchDriver(
        population,
        policy=controller,
        curriculum=NoRecovery() if curriculum is None else curriculum,
        experience_buffer=ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=8,
                max_payload_bytes=1024 * 1024,
            )
        ),
        experience_sink=assembler,
        decision_progress_provider=progress_provider,
    )
    return driver, assembler, batcher, trainer, controller, shadow


def _attempt_limits() -> AttemptAssemblyLimits:
    return AttemptAssemblyLimits(
        max_open_attempts=8,
        max_decisions_per_attempt=64,
        max_payload_bytes_per_attempt=1024 * 1024,
    )


def _scorer():
    return RaggedCandidateScorer.from_bridge_schema(
        semantic_schema_fixture(),
        RaggedScorerConfig(hidden_dim=4, relation_layers=0),
    )


if __name__ == "__main__":
    unittest.main()
