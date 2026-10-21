from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

from sts_learning import AttemptUpdateBatchLimits, OnPolicyObjectiveConfig


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
_BRIDGE_AVAILABLE = importlib.util.find_spec("sts_learning_bridge") is not None

if _TORCH_AVAILABLE:
    from sts_learning.torch_policy import (
        RaggedCategoricalPolicyConfig,
        RaggedScorerConfig,
    )
    from sts_learning.torch_session import (
        CategoricalOnlineSessionFactory,
        NoRecoveryCurriculum,
    )
    from sts_learning.torch_session_config import (
        CategoricalOnlineProfile,
        CategoricalOnlineSessionConfig,
        CategoricalSessionBridge,
        CategoricalSessionLimits,
    )


@unittest.skipUnless(
    _TORCH_AVAILABLE and _BRIDGE_AVAILABLE,
    "optional PyTorch dependency or standalone bridge wheel is not installed",
)
class RealBridgeCategoricalOnlineSessionTests(unittest.TestCase):
    def test_two_generations_continue_across_a_fresh_restore(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            factory = CategoricalOnlineSessionFactory(
                Path(root),
                CategoricalSessionBridge.installed(),
                CategoricalOnlineSessionConfig(
                    ascension_level=20,
                    profile=CategoricalOnlineProfile(
                        scorer=RaggedScorerConfig(
                            hidden_dim=8,
                            relation_layers=1,
                        ),
                        behavior=RaggedCategoricalPolicyConfig(
                            temperature=0.8
                        ),
                        objective=OnPolicyObjectiveConfig(
                            attempts_per_update=1,
                        ),
                        optimizer_steps_per_generation=1,
                    ),
                    limits=CategoricalSessionLimits(
                        owner_capacity=4,
                        attempt_updates=AttemptUpdateBatchLimits(
                            max_decisions_per_update=4_096,
                            max_payload_bytes_per_update=64 * 1024 * 1024,
                        ),
                    ),
                ),
                NoRecoveryCurriculum(),
            )
            initial = factory.new(model_seed=43, behavior_seed=94)
            initial_resume = initial.publish()

            generation_zero = factory.restore(initial_resume.manifest_id)
            generation_one = generation_zero.advance_generation(
                max_batch_steps=256
            )
            self.assertTrue(generation_one.promoted)
            generation_one_resume = generation_zero.publish()

            restored = factory.restore(generation_one_resume.manifest_id)
            generation_two = restored.advance_generation(max_batch_steps=256)

            self.assertTrue(generation_two.promoted)
            self.assertEqual(
                generation_one.optimizer_steps_after,
                1,
            )
            self.assertEqual(
                generation_two.optimizer_steps_before,
                1,
            )
            self.assertEqual(
                generation_two.optimizer_steps_after,
                2,
            )
            self.assertEqual(
                restored.runner.controller.snapshot.active_training_step,
                2,
            )
            self.assertEqual(
                restored.runner.controller.snapshot.successful_promotions,
                3,
            )


if __name__ == "__main__":
    unittest.main()
