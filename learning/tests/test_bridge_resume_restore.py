from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

from learning.tests.driver_fixtures import NoRecovery
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
    OnlineBatchDriver,
    OnPolicyObjectiveConfig,
    ResumeStoreLimits,
    SeedPartition,
    SeedSchedule,
    SemanticBatchConcatLimits,
    initialize_population,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
try:
    from sts_learning_bridge import (
        LearningBatchEnv,
        LearningCheckpointBatch,
        semantic_schema,
    )
except ImportError:
    LearningBatchEnv = None  # type: ignore[assignment,misc]
    LearningCheckpointBatch = None  # type: ignore[assignment,misc]
    semantic_schema = None  # type: ignore[assignment]

if _TORCH_AVAILABLE:
    import torch

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
    from sts_learning.torch_provenance import categorical_trainer_implementation
    from sts_learning.torch_resume_publication import (
        CategoricalGenerationResumePublisher,
        CategoricalResumePayloadLimits,
    )
    from sts_learning.torch_resume_restore import (
        CategoricalGenerationResumeRestorer,
        CategoricalResumeRestoreConfig,
        CategoricalResumeRestoreFactories,
    )
    from sts_learning.torch_training import SynchronousPolicyTrainer


@unittest.skipUnless(
    _TORCH_AVAILABLE and LearningBatchEnv is not None,
    "optional PyTorch dependency or standalone bridge wheel is not installed",
)
class RealBridgeCategoricalResumeRestorerTests(unittest.TestCase):
    def test_environment_and_bank_restore_from_one_manifest(self) -> None:
        assert LearningBatchEnv is not None
        assert LearningCheckpointBatch is not None
        assert semantic_schema is not None
        with tempfile.TemporaryDirectory() as root_text:
            root = Path(root_text)
            schema = semantic_schema()
            scorer_config = RaggedScorerConfig(hidden_dim=4, relation_layers=0)
            behavior_config = RaggedCategoricalPolicyConfig(temperature=0.8)
            return_config = FloorProgressReturnConfig()
            objective_config = OnPolicyObjectiveConfig(
                terminal_return=return_config,
                attempts_per_update=1,
            )

            def scorer_factory():
                return RaggedCandidateScorer.from_bridge_schema(
                    schema,
                    scorer_config,
                )

            checkpoint_limits = TorchCheckpointLimits(
                max_checkpoints=2,
                max_bytes_per_checkpoint=2 * 1024 * 1024,
                max_total_bytes=4 * 1024 * 1024,
            )
            catalog_limits = BehaviorManifestCatalogLimits(
                max_manifests=2,
                max_bytes_per_manifest=1024,
                max_total_bytes=2 * 1024,
            )

            def controller_factory(generator):
                return CategoricalTorchBehaviorController(
                    TorchBehaviorPublisher(
                        BoundedTorchCheckpointStore(
                            root / "behavior-checkpoints",
                            checkpoint_limits,
                        ),
                        BoundedBehaviorManifestCatalog(
                            root / "behavior-manifests",
                            catalog_limits,
                        ),
                        BehaviorManifestRegistry(capacity=2),
                        behavior_manifest_template_fixture(
                            semantic_schema_version=int(schema["version"]),
                            behavior_rule=behavior_config.behavior_rule,
                            trainer_implementation=categorical_trainer_implementation(
                                objective_config,
                            ),
                        ),
                    ),
                    scorer_factory,
                    behavior_config,
                    generator,
                )

            torch.manual_seed(57)
            shadow = scorer_factory()
            controller = controller_factory(torch.Generator().manual_seed(94))
            controller.promote_live(shadow, training_step=0)
            concat_limits = SemanticBatchConcatLimits(
                max_rows=64,
                max_input_array_bytes=4 * 1024 * 1024,
            )

            def optimizer_factory(scorer):
                return torch.optim.Adam(scorer.parameters(), lr=0.001)

            trainer = SynchronousPolicyTrainer(
                shadow,
                optimizer_factory(shadow),
                controller.publisher.registry,
                concat_limits,
                behavior_config,
                objective_config,
            )
            attempt_limits = AttemptAssemblyLimits(
                max_open_attempts=1,
                max_decisions_per_attempt=256,
                max_payload_bytes_per_attempt=8 * 1024 * 1024,
            )
            experience_limits = ExperienceLimits(
                max_decisions=16,
                max_payload_bytes=4 * 1024 * 1024,
            )
            attempt_update_limits = AttemptUpdateBatchLimits(
                max_decisions_per_update=256,
                max_payload_bytes_per_update=8 * 1024 * 1024,
            )
            update_batcher = BoundedAttemptUpdateBatcher(
                objective_config.attempts_per_update,
                attempt_update_limits,
                trainer,
            )
            assembler = BoundedAttemptAssembler(attempt_limits, update_batcher)
            population = initialize_population(
                lambda seeds: LearningBatchEnv(seeds, 20),
                slot_count=1,
                schedule=SeedSchedule(SeedPartition.TRAINING),
                max_recoveries_per_episode=0,
            )
            driver = OnlineBatchDriver(
                population,
                policy=controller,
                curriculum=NoRecovery(),
                experience_buffer=ExperienceSegmentBuffer(experience_limits),
                experience_sink=assembler,
            )
            runner = BoundedCategoricalGenerationRunner(
                driver,
                assembler,
                update_batcher,
                trainer,
                controller,
                shadow,
                optimizer_steps_per_generation=1,
            )
            payload_limits = CategoricalResumePayloadLimits(
                max_environment_bytes=16 * 1024 * 1024,
                max_episode_root_bank_bytes=16 * 1024 * 1024,
                max_shadow_model_bytes=1024 * 1024,
                max_optimizer_bytes=1024 * 1024,
                max_generator_bytes=1024 * 1024,
                max_metadata_bytes=1024 * 1024,
            )
            resume_store = BoundedResumeStore(
                root / "resume",
                ResumeStoreLimits(
                    max_components=6,
                    max_bytes_per_component=16 * 1024 * 1024,
                    max_total_component_bytes=36 * 1024 * 1024,
                    max_manifests=1,
                    max_bytes_per_manifest=1024,
                    max_total_manifest_bytes=1024,
                ),
            )
            publication = CategoricalGenerationResumePublisher(
                resume_store,
                payload_limits,
            ).publish(runner)
            restored = CategoricalGenerationResumeRestorer(
                resume_store,
                CategoricalResumeRestoreConfig(
                    factories=CategoricalResumeRestoreFactories(
                        environment_from_checkpoint=(
                            LearningBatchEnv.from_checkpoint_bytes
                        ),
                        checkpoint_bank_from_checkpoint=(
                            LearningCheckpointBatch.from_checkpoint_bytes
                        ),
                        shadow_scorer=scorer_factory,
                        optimizer=optimizer_factory,
                        controller=controller_factory,
                    ),
                    curriculum=NoRecovery(),
                    experience_limits=experience_limits,
                    attempt_limits=attempt_limits,
                    attempt_update_limits=attempt_update_limits,
                    concat_limits=concat_limits,
                    objective=objective_config,
                    payload_limits=payload_limits,
                    expected_generator_device_type="cpu",
                ),
            ).restore(publication.manifest_id)

            self.assertEqual(restored.boundary, publication.boundary)
            self.assertEqual(
                restored.runner.driver.env.checkpoint_bytes(
                    max_bytes=payload_limits.max_environment_bytes
                ),
                runner.driver.env.checkpoint_bytes(
                    max_bytes=payload_limits.max_environment_bytes
                ),
            )
            self.assertEqual(
                restored.runner.driver.checkpoint_bank.checkpoint_bytes(
                    max_bytes=payload_limits.max_episode_root_bank_bytes
                ),
                runner.driver.checkpoint_bank.checkpoint_bytes(
                    max_bytes=payload_limits.max_episode_root_bank_bytes
                ),
            )


if __name__ == "__main__":
    unittest.main()
