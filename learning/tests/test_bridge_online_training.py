from __future__ import annotations

import importlib.util
import tempfile
import unittest
import warnings
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
    BridgeDecisionProgressProvider,
    ExperienceLimits,
    ExperienceSegmentBuffer,
    FloorProgressReturnConfig,
    HeldOutEvaluationSpec,
    OnlineBatchDriver,
    OnPolicyObjectiveConfig,
    PairedHeldOutEvaluationSpec,
    SemanticBatchConcatLimits,
    SeedPartition,
    SeedSchedule,
    initialize_population,
    evaluate_paired_held_out_behaviors,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
try:
    from sts_learning_bridge import LearningBatchEnv, semantic_schema
except ImportError:
    LearningBatchEnv = None  # type: ignore[assignment,misc]
    semantic_schema = None  # type: ignore[assignment]

if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_behavior import (
        CategoricalTorchBehaviorController,
        CheckpointedCategoricalTorchPolicy,
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
    from sts_learning.torch_training import SynchronousPolicyTrainer


@unittest.skipUnless(
    _TORCH_AVAILABLE and LearningBatchEnv is not None,
    "optional PyTorch dependency or standalone bridge wheel is not installed",
)
class RealBridgeOnlineTrainingTests(unittest.TestCase):
    def test_categorical_behavior_promotes_consecutive_bounded_generations(self) -> None:
        assert LearningBatchEnv is not None
        assert semantic_schema is not None
        # This is deliberately end-to-end: publication, bridge decoding,
        # bounded retention, attempt closure, and training are one contract.
        schema = semantic_schema()
        scorer_config = RaggedScorerConfig(hidden_dim=8, relation_layers=0)
        behavior_config = RaggedCategoricalPolicyConfig(temperature=0.8)
        return_config = FloorProgressReturnConfig()
        objective_config = OnPolicyObjectiveConfig(
            terminal_return=return_config,
            attempts_per_update=1,
        )

        def scorer_factory():
            return RaggedCandidateScorer.from_bridge_schema(schema, scorer_config)

        torch.manual_seed(43)
        shadow = scorer_factory()
        registry = BehaviorManifestRegistry(capacity=3)
        with tempfile.TemporaryDirectory() as root:
            store = BoundedTorchCheckpointStore(
                Path(root, "checkpoints"),
                TorchCheckpointLimits(
                    max_checkpoints=3,
                    max_bytes_per_checkpoint=2 * 1024 * 1024,
                    max_total_bytes=6 * 1024 * 1024,
                ),
            )
            catalog = BoundedBehaviorManifestCatalog(
                Path(root, "manifests"),
                BehaviorManifestCatalogLimits(
                    max_manifests=3,
                    max_bytes_per_manifest=1024,
                    max_total_bytes=3 * 1024,
                ),
            )
            publisher = TorchBehaviorPublisher(
                store,
                catalog,
                registry,
                behavior_manifest_template_fixture(
                    semantic_schema_version=int(schema["version"]),
                    behavior_rule=behavior_config.behavior_rule,
                    trainer_implementation=categorical_trainer_implementation(
                        objective_config,
                    ),
                ),
            )
            behavior_generator = torch.Generator().manual_seed(94)
            controller = CategoricalTorchBehaviorController(
                publisher,
                scorer_factory,
                behavior_config,
                behavior_generator,
            )
            controller.promote_live(
                shadow,
                training_step=0,
            )
            generation_zero = controller.publish_active()
            trainer = SynchronousPolicyTrainer(
                shadow,
                torch.optim.SGD(shadow.parameters(), lr=0.001),
                registry,
                SemanticBatchConcatLimits(
                    max_rows=1_024,
                    max_input_array_bytes=32 * 1024 * 1024,
                ),
                behavior_config,
                objective_config,
            )
            update_batcher = BoundedAttemptUpdateBatcher(
                objective_config.attempts_per_update,
                AttemptUpdateBatchLimits(
                    max_decisions_per_update=1_024,
                    max_payload_bytes_per_update=32 * 1024 * 1024,
                ),
                trainer,
            )
            assembler = BoundedAttemptAssembler(
                AttemptAssemblyLimits(
                    max_open_attempts=1,
                    max_decisions_per_attempt=1_024,
                    max_payload_bytes_per_attempt=32 * 1024 * 1024,
                ),
                update_batcher,
            )
            population = initialize_population(
                lambda seeds: LearningBatchEnv(seeds, 20),
                slot_count=1,
                schedule=SeedSchedule(SeedPartition.HELD_OUT),
                max_recoveries_per_episode=0,
            )
            progress_provider = BridgeDecisionProgressProvider(population.env)
            driver = OnlineBatchDriver(
                population,
                policy=controller,
                curriculum=NoRecovery(),
                experience_buffer=ExperienceSegmentBuffer(
                    ExperienceLimits(
                        max_decisions=16,
                        max_payload_bytes=4 * 1024 * 1024,
                    )
                ),
                experience_sink=assembler,
                decision_progress_provider=progress_provider,
            )
            generation_runner = BoundedCategoricalGenerationRunner(
                driver,
                assembler,
                update_batcher,
                trainer,
                controller,
                shadow,
                optimizer_steps_per_generation=1,
            )

            with warnings.catch_warnings():
                warnings.simplefilter("error")
                partial_generation = generation_runner.advance(max_batch_steps=1)
                self.assertFalse(partial_generation.promoted)
                self.assertTrue(partial_generation.step_limit_reached)
                self.assertEqual(partial_generation.optimizer_steps_after, 0)
                self.assertEqual(
                    controller.snapshot.active_manifest_id,
                    generation_zero.manifest_id,
                )
                promoted_generation = generation_runner.advance(max_batch_steps=159)
                self.assertTrue(promoted_generation.promoted)
                self.assertEqual(
                    promoted_generation.active_manifest_id_before,
                    generation_zero.manifest_id,
                )
                self.assertEqual(promoted_generation.active_training_step_before, 0)
                self.assertEqual(promoted_generation.optimizer_steps_before, 0)
                self.assertEqual(promoted_generation.optimizer_steps_after, 1)
                self.assertEqual(
                    promoted_generation.promotion_target_training_step,
                    1,
                )
                self.assertEqual(promoted_generation.terminal_attempts, 1)
                self.assertEqual(promoted_generation.terminal_flushes, 1)
                generation_one = promoted_generation.promotion
                assert generation_one is not None
                generation_after_first = driver.ledger.snapshot(0).episode_generation
                first_training = trainer.snapshot
                first_manifest_evidence = first_training.last_behavior_manifest_ids
                first_probability_evidence = first_training.last_selection_probabilities

                self.assertEqual(first_training.optimizer_steps, 1)
                self.assertIsNotNone(first_manifest_evidence)
                self.assertIsNotNone(first_probability_evidence)
                assert first_manifest_evidence is not None
                assert first_probability_evidence is not None
                self.assertTrue(
                    all(
                        manifest_id == generation_zero.manifest_id
                        for attempt in first_manifest_evidence
                        for manifest_id in attempt
                    )
                )
                self.assertTrue(
                    all(
                        probability.value is not None
                        for attempt in first_probability_evidence
                        for probability in attempt
                    )
                )

                second_generation = generation_runner.advance(
                    max_batch_steps=(
                        160
                        - partial_generation.batch_steps
                        - promoted_generation.batch_steps
                    ),
                )
                self.assertTrue(second_generation.promoted)
                self.assertEqual(
                    second_generation.active_manifest_id_before,
                    generation_one.manifest_id,
                )
                self.assertEqual(second_generation.active_training_step_before, 1)
                self.assertEqual(second_generation.optimizer_steps_before, 1)
                self.assertEqual(second_generation.optimizer_steps_after, 2)
                self.assertEqual(
                    second_generation.promotion_target_training_step,
                    2,
                )
                self.assertEqual(second_generation.terminal_attempts, 1)
                self.assertEqual(second_generation.terminal_flushes, 1)
                generation_two = second_generation.promotion
                assert generation_two is not None
                self.assertLessEqual(
                    partial_generation.batch_steps
                    + promoted_generation.batch_steps
                    + second_generation.batch_steps,
                    160,
                )

            second_training = trainer.snapshot
            second_manifest_evidence = second_training.last_behavior_manifest_ids
            second_probability_evidence = second_training.last_selection_probabilities

            self.assertNotEqual(generation_zero.manifest_id, generation_one.manifest_id)
            self.assertNotEqual(generation_one.manifest_id, generation_two.manifest_id)
            self.assertEqual(generation_two.manifest.training_step, 2)
            self.assertEqual(
                driver.ledger.snapshot(0).episode_generation,
                generation_after_first + 1,
            )
            self.assertEqual(
                first_manifest_evidence,
                ((generation_zero.manifest_id,) * len(first_manifest_evidence[0]),),
            )
            self.assertEqual(second_training.optimizer_steps, 2)
            self.assertIsNotNone(second_manifest_evidence)
            self.assertIsNotNone(second_probability_evidence)
            assert second_manifest_evidence is not None
            assert second_probability_evidence is not None
            self.assertTrue(
                all(
                    manifest_id == generation_one.manifest_id
                    for attempt in second_manifest_evidence
                    for manifest_id in attempt
                )
            )
            self.assertTrue(
                all(
                    probability.value is not None
                    for attempt in second_probability_evidence
                    for probability in attempt
                )
            )
            self.assertEqual(
                controller.snapshot.active_manifest_id,
                generation_two.manifest_id,
            )
            self.assertEqual(controller.snapshot.active_training_step, 2)
            self.assertEqual(controller.snapshot.successful_promotions, 3)
            generation_two_publication = controller.publish_active()
            self.assertEqual(store.snapshot.checkpoints, 2)
            self.assertEqual(catalog.snapshot.manifests, 2)
            self.assertEqual(registry.snapshot.registered_manifests, 1)

            recovered_generator = torch.Generator()
            recovered_generator.set_state(behavior_generator.get_state())
            recovered = CategoricalTorchBehaviorController(
                TorchBehaviorPublisher(
                    store,
                    catalog,
                    BehaviorManifestRegistry(capacity=1),
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
                recovered_generator,
            )
            recovered_publication = recovered.recover_and_promote(
                generation_two.manifest_id
            )
            next_decision = driver.env.decision_batch(semantic=True)

            self.assertEqual(recovered_publication, generation_two_publication)
            self.assertEqual(
                controller.choose(next_decision),
                recovered.choose(next_decision),
            )

            evaluation_schedule = SeedSchedule(
                SeedPartition.HELD_OUT,
                next_candidate=107,
            )
            evaluation_spec = HeldOutEvaluationSpec(
                slot_count=1,
                terminal_attempt_target=1,
                max_batch_steps=160,
            )
            generation_zero_generator = torch.Generator().manual_seed(501)
            generation_two_generator = torch.Generator().manual_seed(501)
            generation_zero_policy = CheckpointedCategoricalTorchPolicy.recover(
                generation_zero.manifest_id,
                store,
                catalog,
                BehaviorManifestRegistry(capacity=1),
                scorer_factory,
                behavior_config,
                generation_zero_generator,
            )
            generation_two_policy = CheckpointedCategoricalTorchPolicy.recover(
                generation_two.manifest_id,
                store,
                catalog,
                BehaviorManifestRegistry(capacity=1),
                scorer_factory,
                behavior_config,
                generation_two_generator,
            )
            self.assertTrue(
                torch.equal(
                    generation_zero_generator.get_state(),
                    generation_two_generator.get_state(),
                )
            )

            held_out_comparison = evaluate_paired_held_out_behaviors(
                lambda seeds: LearningBatchEnv(seeds, 20),
                generation_zero_policy,
                generation_two_policy,
                spec=PairedHeldOutEvaluationSpec(
                    schedule=evaluation_schedule,
                    evaluation=evaluation_spec,
                ),
            )
            generation_zero_evaluation = held_out_comparison.left
            generation_two_evaluation = held_out_comparison.right

            self.assertTrue(held_out_comparison.comparable)
            self.assertTrue(generation_zero_evaluation.complete)
            self.assertTrue(generation_two_evaluation.complete)
            self.assertEqual(
                generation_zero_evaluation.behavior_manifest_id,
                generation_zero.manifest_id,
            )
            self.assertEqual(
                generation_two_evaluation.behavior_manifest_id,
                generation_two.manifest_id,
            )
            self.assertEqual(
                generation_zero_evaluation.schedule_start,
                generation_two_evaluation.schedule_start,
            )
            self.assertEqual(generation_zero_evaluation.run.summary.recoveries, 0)
            self.assertEqual(generation_two_evaluation.run.summary.recoveries, 0)
            self.assertEqual(
                generation_zero_evaluation.run.summary.victories
                + generation_zero_evaluation.run.summary.defeats,
                generation_zero_evaluation.run.summary.terminal_attempts,
            )
            self.assertEqual(
                generation_two_evaluation.run.summary.victories
                + generation_two_evaluation.run.summary.defeats,
                generation_two_evaluation.run.summary.terminal_attempts,
            )

        self.assertEqual(assembler.snapshot.completed_attempts, 2)
        self.assertEqual(assembler.snapshot.dropped_attempts, 0)
        self.assertTrue(
            any(
                probability.value != 1.0
                for evidence in (
                    first_probability_evidence,
                    second_probability_evidence,
                )
                for attempt in evidence
                for probability in attempt
            )
        )
        self.assertFalse(trainer.snapshot.poisoned)


if __name__ == "__main__":
    unittest.main()
