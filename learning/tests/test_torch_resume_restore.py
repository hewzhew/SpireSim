from __future__ import annotations

import importlib.util
import tempfile
import unittest
from unittest.mock import patch
from dataclasses import replace
from pathlib import Path

from learning.tests.driver_fixtures import (
    FakeCheckpointBatch,
    NoRecovery,
    NumpyWinningBatchEnv,
)
from learning.tests.torch_outcome_fixtures import (
    behavior_manifest_template_fixture,
)
from sts_learning import (
    AttemptAssemblyLimits,
    AttemptUpdateBatchLimits,
    BehaviorManifestCatalogLimits,
    BehaviorManifestRegistry,
    BoundedAttemptAssembler,
    BoundedAttemptUpdateBatcher,
    BoundedBehaviorManifestCatalog,
    BoundedResumeStore,
    ExperienceLimits,
    ExperienceSegmentBuffer,
    FloorProgressReturnConfig,
    ManifestArtifactId,
    ManifestArtifactKind,
    OnlineBatchDriver,
    OnPolicyObjectiveConfig,
    ResumeStoreLimits,
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
    from sts_learning.torch_generation import BoundedCategoricalGenerationRunner
    from sts_learning.torch_policy import (
        RaggedCandidateScorer,
        RaggedCategoricalPolicyConfig,
        RaggedScorerConfig,
    )
    from sts_learning.torch_resume import (
        encode_generator_state,
        encode_optimizer_state,
        encode_shadow_model_state,
    )
    from sts_learning.torch_provenance import categorical_trainer_implementation
    from sts_learning.torch_resume_publication import (
        CategoricalGenerationResumePublisher,
        CategoricalResumePayloadLimits,
    )
    from sts_learning.torch_resume_restore import (
        CategoricalGenerationResumeRestorer,
        CategoricalResumeRestoreConfig,
        CategoricalResumeRestoreFactories,
        TorchResumeRestoreError,
    )
    from sts_learning.torch_training import SynchronousPolicyTrainer


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class CategoricalGenerationResumeRestorerTests(unittest.TestCase):
    def test_fresh_restore_reproduces_the_next_generation_exactly(self) -> None:
        # The full owner chain is intentional: this protects process continuation,
        # not an individual component codec already covered by narrower tests.
        with tempfile.TemporaryDirectory() as root:
            fixture = _ResumeFixture(Path(root))
            initial = fixture.initial_runner()
            initial_resume = fixture.resume_publisher.publish(initial)

            baseline = fixture.restorer.restore(initial_resume.manifest_id).runner
            split = fixture.restorer.restore(initial_resume.manifest_id).runner
            baseline_result = baseline.advance(max_batch_steps=1)

            split_resume = fixture.resume_publisher.publish(split)
            resumed = fixture.restorer.restore(split_resume.manifest_id).runner
            resumed_result = resumed.advance(max_batch_steps=1)

            self.assertTrue(baseline_result.promoted)
            self.assertTrue(resumed_result.promoted)
            assert baseline_result.promotion is not None
            assert resumed_result.promotion is not None
            self.assertEqual(
                resumed_result.promotion.manifest_id,
                baseline_result.promotion.manifest_id,
            )

            baseline_env = baseline.driver.env
            resumed_env = resumed.driver.env
            assert isinstance(baseline_env, NumpyWinningBatchEnv)
            assert isinstance(resumed_env, NumpyWinningBatchEnv)
            self.assertEqual(resumed_env.choose_calls, baseline_env.choose_calls)
            self.assertEqual(
                resumed.trainer.snapshot.last_selection_probabilities,
                baseline.trainer.snapshot.last_selection_probabilities,
            )
            self.assertEqual(
                resumed.trainer.snapshot.last_loss,
                baseline.trainer.snapshot.last_loss,
            )
            self.assertEqual(
                encode_shadow_model_state(resumed.shadow_scorer, max_bytes=2**20),
                encode_shadow_model_state(baseline.shadow_scorer, max_bytes=2**20),
            )
            self.assertEqual(
                encode_optimizer_state(resumed.trainer.optimizer, max_bytes=2**20),
                encode_optimizer_state(baseline.trainer.optimizer, max_bytes=2**20),
            )
            self.assertEqual(
                encode_generator_state(resumed.controller.generator, max_bytes=2**20),
                encode_generator_state(baseline.controller.generator, max_bytes=2**20),
            )
            self.assertEqual(
                resumed.driver.env.checkpoint_bytes(max_bytes=1024),
                baseline.driver.env.checkpoint_bytes(max_bytes=1024),
            )
            self.assertEqual(
                resumed.driver.checkpoint_bank.checkpoint_bytes(max_bytes=1024),
                baseline.driver.checkpoint_bank.checkpoint_bytes(max_bytes=1024),
            )

            baseline_boundary = baseline.require_resume_boundary()
            resumed_boundary = resumed.require_resume_boundary()
            self.assertEqual(resumed_boundary.driver, baseline_boundary.driver)
            self.assertEqual(resumed_boundary.assembler, baseline_boundary.assembler)
            self.assertEqual(resumed_boundary.controller, baseline_boundary.controller)
            self.assertEqual(
                resumed_boundary.trainer.optimizer_steps,
                baseline_boundary.trainer.optimizer_steps,
            )
            self.assertEqual(
                resumed_boundary.trainer.last_behavior_manifest_ids,
                baseline_boundary.trainer.last_behavior_manifest_ids,
            )

    def test_slot_mismatched_environment_is_never_exposed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            fixture = _ResumeFixture(Path(root))
            publication = fixture.resume_publisher.publish(fixture.initial_runner())

            def wrong_slot_count(payload, *, expected_slots, max_bytes):
                NumpyWinningBatchEnv.from_checkpoint_bytes(
                    payload,
                    expected_slots=expected_slots,
                    max_bytes=max_bytes,
                )
                return NumpyWinningBatchEnv([1, 2])

            factories = replace(
                fixture.restore_config.factories,
                environment_from_checkpoint=wrong_slot_count,
            )
            config = replace(fixture.restore_config, factories=factories)
            restorer = CategoricalGenerationResumeRestorer(
                fixture.resume_store,
                config,
            )

            with self.assertRaisesRegex(
                TorchResumeRestoreError,
                "slot counts differ",
            ):
                restorer.restore(publication.manifest_id)

    def test_runtime_provenance_mismatch_is_never_exposed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            fixture = _ResumeFixture(Path(root))
            publication = fixture.resume_publisher.publish(fixture.initial_runner())

            def wrong_provenance(generator):
                controller = fixture.controller(generator)
                controller.publisher.template = replace(
                    controller.publisher.template,
                    optimizer_config=ManifestArtifactId(
                        ManifestArtifactKind.OPTIMIZER_CONFIG,
                        b"x" * 32,
                    ),
                )
                return controller

            factories = replace(
                fixture.restore_config.factories,
                controller=wrong_provenance,
            )
            restorer = CategoricalGenerationResumeRestorer(
                fixture.resume_store,
                replace(fixture.restore_config, factories=factories),
            )

            with self.assertRaisesRegex(
                TorchResumeRestoreError,
                "runtime provenance",
            ):
                restorer.restore(publication.manifest_id)

    def test_terminal_return_mismatch_is_never_exposed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            fixture = _ResumeFixture(Path(root))
            publication = fixture.resume_publisher.publish(fixture.initial_runner())
            restorer = CategoricalGenerationResumeRestorer(
                fixture.resume_store,
                replace(
                    fixture.restore_config,
                    objective=replace(
                        fixture.objective_config,
                        terminal_return=FloorProgressReturnConfig(target_floor=51),
                    ),
                ),
            )

            with self.assertRaisesRegex(
                TorchResumeRestoreError,
                "trainer configuration",
            ):
                restorer.restore(publication.manifest_id)

    def test_legacy_episode_mean_optimizer_session_cannot_resume_as_episode_sum(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            with patch("sts_learning.torch_provenance._TRAINER_IMPLEMENTATION_VERSION", 4):
                fixture = _ResumeFixture(Path(root))
                publication = fixture.resume_publisher.publish(fixture.initial_runner())
            restorer = CategoricalGenerationResumeRestorer(fixture.resume_store, fixture.restore_config)
            with self.assertRaisesRegex(TorchResumeRestoreError, "runtime provenance"):
                restorer.restore(publication.manifest_id)

    def test_attempts_per_update_mismatch_is_never_exposed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            fixture = _ResumeFixture(Path(root))
            publication = fixture.resume_publisher.publish(fixture.initial_runner())
            restorer = CategoricalGenerationResumeRestorer(
                fixture.resume_store,
                replace(
                    fixture.restore_config,
                    objective=replace(
                        fixture.objective_config,
                        attempts_per_update=2,
                    ),
                ),
            )

            with self.assertRaisesRegex(
                TorchResumeRestoreError,
                "trainer configuration",
            ):
                restorer.restore(publication.manifest_id)


class _ResumeFixture:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.behavior_root = root / "behavior"
        self.behavior_root.mkdir()
        self.behavior_config = RaggedCategoricalPolicyConfig(temperature=0.8)
        self.return_config = FloorProgressReturnConfig()
        self.objective_config = OnPolicyObjectiveConfig(
            terminal_return=self.return_config,
            attempts_per_update=1,
        )
        self.checkpoint_limits = TorchCheckpointLimits(
            max_checkpoints=3,
            max_bytes_per_checkpoint=2 * 1024 * 1024,
            max_total_bytes=6 * 1024 * 1024,
        )
        self.catalog_limits = BehaviorManifestCatalogLimits(
            max_manifests=3,
            max_bytes_per_manifest=1024,
            max_total_bytes=3 * 1024,
        )
        self.experience_limits = ExperienceLimits(
            max_decisions=8,
            max_payload_bytes=1024 * 1024,
        )
        self.attempt_limits = AttemptAssemblyLimits(
            max_open_attempts=1,
            max_decisions_per_attempt=64,
            max_payload_bytes_per_attempt=1024 * 1024,
        )
        self.attempt_update_limits = AttemptUpdateBatchLimits(
            max_decisions_per_update=64,
            max_payload_bytes_per_update=1024 * 1024,
        )
        self.concat_limits = SemanticBatchConcatLimits(
            max_rows=64,
            max_input_array_bytes=1024 * 1024,
        )
        self.payload_limits = CategoricalResumePayloadLimits(
            max_environment_bytes=1024,
            max_episode_root_bank_bytes=1024,
            max_shadow_model_bytes=1024 * 1024,
            max_optimizer_bytes=1024 * 1024,
            max_generator_bytes=1024 * 1024,
            max_metadata_bytes=1024 * 1024,
        )
        self.resume_store = BoundedResumeStore(
            root / "resume",
            ResumeStoreLimits(
                max_components=18,
                max_bytes_per_component=2 * 1024 * 1024,
                max_total_component_bytes=24 * 1024 * 1024,
                max_manifests=3,
                max_bytes_per_manifest=1024,
                max_total_manifest_bytes=3 * 1024,
            ),
        )
        self.resume_publisher = CategoricalGenerationResumePublisher(
            self.resume_store,
            self.payload_limits,
        )
        self.restore_config = CategoricalResumeRestoreConfig(
            factories=CategoricalResumeRestoreFactories(
                environment_from_checkpoint=(
                    NumpyWinningBatchEnv.from_checkpoint_bytes
                ),
                checkpoint_bank_from_checkpoint=(
                    FakeCheckpointBatch.from_checkpoint_bytes
                ),
                shadow_scorer=self.scorer,
                optimizer=self.optimizer,
                controller=self.controller,
            ),
            curriculum=NoRecovery(),
            experience_limits=self.experience_limits,
            attempt_limits=self.attempt_limits,
            attempt_update_limits=self.attempt_update_limits,
            concat_limits=self.concat_limits,
            objective=self.objective_config,
            payload_limits=self.payload_limits,
            expected_generator_device_type="cpu",
        )
        self.restorer = CategoricalGenerationResumeRestorer(
            self.resume_store,
            self.restore_config,
        )

    def scorer(self):
        return RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(hidden_dim=4, relation_layers=0),
        )

    def optimizer(self, scorer):
        return torch.optim.Adam(scorer.parameters(), lr=0.001)

    def controller(self, generator):
        store = BoundedTorchCheckpointStore(
            self.behavior_root / "checkpoints",
            self.checkpoint_limits,
        )
        catalog = BoundedBehaviorManifestCatalog(
            self.behavior_root / "manifests",
            self.catalog_limits,
        )
        registry = BehaviorManifestRegistry(capacity=3)
        return CategoricalTorchBehaviorController(
            TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(
                    behavior_rule=self.behavior_config.behavior_rule,
                    trainer_implementation=categorical_trainer_implementation(
                        self.objective_config,
                    ),
                ),
            ),
            self.scorer,
            self.behavior_config,
            generator,
        )

    def initial_runner(self):
        torch.manual_seed(1203)
        shadow = self.scorer()
        controller = self.controller(torch.Generator().manual_seed(94))
        controller.promote_live(shadow, training_step=0)
        trainer = SynchronousPolicyTrainer(
            shadow,
            self.optimizer(shadow),
            controller.publisher.registry,
            self.concat_limits,
            self.behavior_config,
            self.objective_config,
        )
        update_batcher = BoundedAttemptUpdateBatcher(
            self.objective_config.attempts_per_update,
            self.attempt_update_limits,
            trainer,
        )
        assembler = BoundedAttemptAssembler(self.attempt_limits, update_batcher)
        population = initialize_population(
            NumpyWinningBatchEnv,
            slot_count=1,
            schedule=SeedSchedule(SeedPartition.TRAINING),
            max_recoveries_per_episode=0,
        )
        driver = OnlineBatchDriver(
            population,
            policy=controller,
            curriculum=NoRecovery(),
            experience_buffer=ExperienceSegmentBuffer(self.experience_limits),
            experience_sink=assembler,
        )
        return BoundedCategoricalGenerationRunner(
            driver,
            assembler,
            update_batcher,
            trainer,
            controller,
            shadow,
            optimizer_steps_per_generation=1,
        )


if __name__ == "__main__":
    unittest.main()
