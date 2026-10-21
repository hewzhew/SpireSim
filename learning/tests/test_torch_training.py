from __future__ import annotations

import importlib.util
import unittest

from learning.tests.torch_outcome_fixtures import (
    behavior_manifest_fixture,
    completed_attempt_fixture,
    decision_batch_fixture,
    with_run_progress_fixture,
)
from sts_learning import (
    BehaviorManifestRegistry,
    FloorProgressReturnConfig,
    OnPolicyObjectiveConfig,
    SelectionProbability,
    SemanticBatchConcatLimits,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from sts_learning import AttemptAssemblyDelivery
    from sts_learning.torch_policy import RaggedCandidateLogits
    from sts_learning.torch_policy import RaggedCategoricalPolicyConfig
    from sts_learning.torch_training import (
        SynchronousPolicyTrainer,
        TorchTrainingError,
    )
    POLICY_CONFIG = RaggedCategoricalPolicyConfig()
else:
    POLICY_CONFIG = None


CONCAT_LIMITS = SemanticBatchConcatLimits(
    max_rows=16,
    max_input_array_bytes=1024 * 1024,
)
RETURN_CONFIG = FloorProgressReturnConfig()
OBJECTIVE_CONFIG = OnPolicyObjectiveConfig(
    terminal_return=RETURN_CONFIG,
    attempts_per_update=1,
)


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class SynchronousPolicyTrainerTests(unittest.TestCase):
    def test_delivery_updates_once_without_retaining_attempt_payload(self) -> None:
        registry = BehaviorManifestRegistry(capacity=1)
        manifest_id = registry.register(
            behavior_manifest_fixture(behavior_rule=POLICY_CONFIG.behavior_rule)
        )
        parameter = torch.nn.Parameter(torch.tensor([0.0, 0.0]))

        def scorer(payload):
            return RaggedCandidateLogits(
                values=parameter,
                row_splits=torch.tensor([0, 2]),
            )

        trainer = SynchronousPolicyTrainer(
            scorer,
            torch.optim.SGD([parameter], lr=0.1),
            registry,
            CONCAT_LIMITS,
            POLICY_CONFIG,
            OBJECTIVE_CONFIG,
        )
        batch = with_run_progress_fixture(
            decision_batch_fixture(
                slot=1,
                semantic_row=0,
                selected_ordinal=0,
                manifest_id=manifest_id,
                selection_probability=SelectionProbability.known(0.5),
            ),
            act=1,
            floor=12,
            is_combat=True,
            strategic_context_kind=None,
        )
        delivery = AttemptAssemblyDelivery(
            completed=(
                completed_attempt_fixture(slot=1, batches=(batch,), reward=1),
            ),
            dropped=(),
        )

        trainer(delivery)

        self.assertAlmostEqual(parameter[0].item(), 0.05, places=6)
        self.assertAlmostEqual(parameter[1].item(), -0.05, places=6)
        self.assertEqual(trainer.snapshot.deliveries, 1)
        self.assertEqual(trainer.snapshot.optimizer_steps, 1)
        self.assertEqual(trainer.snapshot.completed_attempts, 1)
        self.assertEqual(trainer.snapshot.trained_decisions, 1)
        self.assertAlmostEqual(trainer.snapshot.last_loss, 0.693147, places=6)
        self.assertEqual(
            trainer.snapshot.last_behavior_manifest_ids,
            ((manifest_id,),),
        )
        self.assertEqual(
            trainer.snapshot.last_selection_probabilities,
            ((SelectionProbability.known(0.5),),),
        )
        self.assertGreater(trainer.snapshot.total_training_seconds, 0.0)
        self.assertGreater(trainer.snapshot.last_training_seconds, 0.0)
        assert trainer.last_credit_assignment is not None
        self.assertEqual(trainer.last_credit_assignment.attempt_count, 1)
        self.assertEqual(trainer.last_credit_assignment.remaining_progress.positive, 1)

        restored_parameter = torch.nn.Parameter(torch.tensor([0.05, -0.05]))

        def restored_scorer(payload):
            return RaggedCandidateLogits(
                values=restored_parameter,
                row_splits=torch.tensor([0, 2]),
            )

        restored = SynchronousPolicyTrainer(
            restored_scorer,
            torch.optim.SGD([restored_parameter], lr=0.1),
            registry,
            CONCAT_LIMITS,
            POLICY_CONFIG,
            OBJECTIVE_CONFIG,
            resume_snapshot=trainer.snapshot,
        )
        self.assertEqual(restored.snapshot, trainer.snapshot)

    def test_unknown_manifest_fails_before_optimizer_and_can_be_retried(self) -> None:
        registry = BehaviorManifestRegistry(capacity=1)
        manifest = behavior_manifest_fixture(behavior_rule=POLICY_CONFIG.behavior_rule)
        parameter = torch.nn.Parameter(torch.tensor([0.0, 0.0]))

        def scorer(payload):
            return RaggedCandidateLogits(
                values=parameter,
                row_splits=torch.tensor([0, 2]),
            )

        trainer = SynchronousPolicyTrainer(
            scorer,
            torch.optim.SGD([parameter], lr=0.1),
            registry,
            CONCAT_LIMITS,
            POLICY_CONFIG,
            OBJECTIVE_CONFIG,
        )
        batch = with_run_progress_fixture(
            decision_batch_fixture(
                slot=1,
                semantic_row=0,
                selected_ordinal=0,
                manifest_id=manifest.identity,
                selection_probability=SelectionProbability.known(0.5),
            ),
            act=1,
            floor=0,
            is_combat=True,
            strategic_context_kind=None,
        )
        delivery = AttemptAssemblyDelivery(
            completed=(
                completed_attempt_fixture(slot=1, batches=(batch,), reward=1),
            ),
            dropped=(),
        )

        with self.assertRaisesRegex(ValueError, "unknown behavior"):
            trainer(delivery)
        self.assertEqual(parameter[0].item(), 0.0)
        self.assertFalse(trainer.snapshot.poisoned)
        self.assertEqual(trainer.snapshot.deliveries, 0)

        registry.register(manifest)
        trainer(delivery)
        self.assertEqual(trainer.snapshot.optimizer_steps, 1)

    def test_optimizer_failure_poison_stops_retrying_mutable_state(self) -> None:
        registry = BehaviorManifestRegistry(capacity=1)
        manifest_id = registry.register(
            behavior_manifest_fixture(behavior_rule=POLICY_CONFIG.behavior_rule)
        )
        parameter = torch.nn.Parameter(torch.tensor([0.0, 0.0]))

        def scorer(payload):
            return RaggedCandidateLogits(
                values=parameter,
                row_splits=torch.tensor([0, 2]),
            )

        class FailingSgd(torch.optim.SGD):
            def step(self, closure=None):
                raise RuntimeError("optimizer failed")

        trainer = SynchronousPolicyTrainer(
            scorer,
            FailingSgd([parameter], lr=0.1),
            registry,
            CONCAT_LIMITS,
            POLICY_CONFIG,
            OBJECTIVE_CONFIG,
        )
        batch = with_run_progress_fixture(
            decision_batch_fixture(
                slot=1,
                semantic_row=0,
                selected_ordinal=0,
                manifest_id=manifest_id,
                selection_probability=SelectionProbability.known(0.5),
            ),
            act=1,
            floor=0,
            is_combat=True,
            strategic_context_kind=None,
        )
        delivery = AttemptAssemblyDelivery(
            completed=(
                completed_attempt_fixture(slot=1, batches=(batch,), reward=1),
            ),
            dropped=(),
        )

        with self.assertRaisesRegex(RuntimeError, "optimizer failed"):
            trainer(delivery)
        self.assertTrue(trainer.snapshot.poisoned)
        with self.assertRaisesRegex(TorchTrainingError, "poisoned"):
            trainer(delivery)


if __name__ == "__main__":
    unittest.main()
