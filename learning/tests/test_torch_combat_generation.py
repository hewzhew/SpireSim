from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.torch_combat_fixtures import (
    COMBAT_HASH,
    ROOT_ID,
    ExactCombatRootSource,
)
from sts_learning import (
    BehaviorManifestCatalogLimits,
    BehaviorManifestRegistry,
    BoundedBehaviorManifestCatalog,
    CombatExperienceLimits,
    CombatWinObjectiveConfig,
    SemanticBatchConcatLimits,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_behavior import (
        CategoricalTorchBehaviorController,
        TorchBehaviorError,
        TorchBehaviorPublisher,
    )
    from sts_learning.torch_checkpoints import (
        BoundedTorchCheckpointStore,
        TorchCheckpointLimits,
    )
    from sts_learning.torch_combat_generation import (
        BoundedCombatWinGenerationRunner,
        TorchCombatGenerationError,
    )
    from sts_learning.torch_combat_training import (
        CombatWinTrainingStatus,
        SynchronousCombatWinTrainer,
    )
    from sts_learning.torch_policy import (
        RaggedCandidateScorer,
        RaggedCategoricalPolicyConfig,
        RaggedScorerConfig,
    )
    from sts_learning.torch_provenance import (
        AdamTrainingConfig,
        combat_win_training_manifest_template,
    )


OTHER_ROOT_ID = "34" * 32
OTHER_COMBAT_HASH = "cd" * 32
LIMITS = CombatExperienceLimits(
    max_decisions=8,
    max_payload_bytes=1024 * 1024,
    max_model_rounds=4,
    max_transitions=4,
)
CONCAT_LIMITS = SemanticBatchConcatLimits(
    max_rows=8,
    max_input_array_bytes=1024 * 1024,
)


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class BoundedCombatWinGenerationRunnerTests(unittest.TestCase):
    def test_win_signal_steps_once_then_promotes_immediately(self) -> None:
        source = ExactCombatRootSource(((ROOT_ID, COMBAT_HASH, (True, False)),))
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(Path(root), source)
            before = tuple(
                parameter.detach().clone()
                for parameter in owners.shadow.parameters()
            )

            result = owners.runner.advance()

            self.assertTrue(result.promoted)
            self.assertEqual(
                result.training.status,
                CombatWinTrainingStatus.OPTIMIZER_STEP,
            )
            self.assertEqual(result.training.optimizer_steps_after, 1)
            self.assertEqual(result.signals.root_id, ROOT_ID)
            self.assertEqual(result.signals.exact_combat_state_hash, COMBAT_HASH)
            self.assertTrue(result.signals.win.has_signal)
            self.assertEqual(result.signals.replicate_count, 2)
            self.assertEqual(source.call_count, 1)
            self.assertEqual(owners.controller.snapshot.active_training_step, 1)
            self.assertEqual(owners.controller.snapshot.successful_promotions, 2)
            self.assertFalse(owners.runner.pending_promotion)
            self.assertTrue(
                any(
                    not torch.equal(parameter.detach(), prior)
                    for parameter, prior in zip(
                        owners.shadow.parameters(),
                        before,
                        strict=True,
                    )
                )
            )

    def test_no_win_signal_keeps_the_current_behavior(self) -> None:
        source = ExactCombatRootSource(((ROOT_ID, COMBAT_HASH, (True, True)),))
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(Path(root), source)
            active_before = owners.controller.snapshot.active_manifest_id

            result = owners.runner.advance()

            self.assertFalse(result.promoted)
            self.assertEqual(
                result.training.status,
                CombatWinTrainingStatus.NO_OBJECTIVE_SIGNAL,
            )
            self.assertFalse(result.signals.win.has_signal)
            self.assertEqual(owners.trainer.snapshot.optimizer_steps, 0)
            self.assertEqual(
                owners.controller.snapshot.active_manifest_id,
                active_before,
            )
            self.assertEqual(owners.controller.snapshot.active_training_step, 0)

    def test_failed_promotion_retries_without_replaying_the_group(self) -> None:
        source = ExactCombatRootSource(((ROOT_ID, COMBAT_HASH, (True, False)),))
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(Path(root), source)
            original = owners.publisher.bind_live
            calls = 0

            def fail_once(scorer, *, training_step):
                nonlocal calls
                calls += 1
                if calls == 1:
                    raise TorchBehaviorError("temporary promotion failure")
                return original(scorer, training_step=training_step)

            with patch.object(owners.publisher, "bind_live", side_effect=fail_once):
                with self.assertRaisesRegex(TorchBehaviorError, "temporary"):
                    owners.runner.advance()
                self.assertTrue(owners.runner.pending_promotion)
                self.assertEqual(owners.trainer.snapshot.optimizer_steps, 1)
                self.assertEqual(owners.controller.snapshot.active_training_step, 0)
                self.assertEqual(source.call_count, 1)

                result = owners.runner.advance()

            self.assertTrue(result.promoted)
            self.assertFalse(owners.runner.pending_promotion)
            self.assertEqual(source.call_count, 1)
            self.assertEqual(owners.trainer.snapshot.deliveries, 1)
            self.assertEqual(owners.controller.snapshot.active_training_step, 1)

    def test_changed_root_fails_before_policy_or_optimizer_mutation(self) -> None:
        source = ExactCombatRootSource(
            (
                (ROOT_ID, COMBAT_HASH, (True, True)),
                (OTHER_ROOT_ID, OTHER_COMBAT_HASH, (True, False)),
            )
        )
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(Path(root), source)
            first = owners.runner.advance()
            self.assertFalse(first.promoted)

            with self.assertRaisesRegex(TorchCombatGenerationError, "changed"):
                owners.runner.advance()

            self.assertEqual(source.call_count, 2)
            self.assertEqual(source.groups[1].choose_calls, 0)
            self.assertEqual(owners.trainer.snapshot.deliveries, 1)
            self.assertEqual(owners.trainer.snapshot.optimizer_steps, 0)
            self.assertEqual(owners.controller.snapshot.active_training_step, 0)


if _TORCH_AVAILABLE:

    class _CombatOwners:
        def __init__(self, root: Path, source: ExactCombatRootSource) -> None:
            schema = semantic_schema_fixture()
            scorer_config = RaggedScorerConfig(hidden_dim=4, relation_layers=0)
            behavior_config = RaggedCategoricalPolicyConfig(temperature=0.8)
            optimizer_config = AdamTrainingConfig(learning_rate=0.002)
            objective_config = CombatWinObjectiveConfig(groups_per_update=1)
            self.shadow = RaggedCandidateScorer.from_bridge_schema(
                schema,
                scorer_config,
            )
            registry = BehaviorManifestRegistry(capacity=1)
            self.publisher = TorchBehaviorPublisher(
                BoundedTorchCheckpointStore(
                    root / "checkpoints",
                    TorchCheckpointLimits(
                        max_checkpoints=1,
                        max_bytes_per_checkpoint=2 * 1024 * 1024,
                        max_total_bytes=2 * 1024 * 1024,
                    ),
                ),
                BoundedBehaviorManifestCatalog(
                    root / "manifests",
                    BehaviorManifestCatalogLimits(
                        max_manifests=1,
                        max_bytes_per_manifest=1024,
                        max_total_bytes=1024,
                    ),
                ),
                registry,
                combat_win_training_manifest_template(
                    schema,
                    scorer_config,
                    behavior_config,
                    optimizer_config,
                    objective_config,
                    device_type="cpu",
                ),
            )

            def scorer_factory() -> RaggedCandidateScorer:
                return RaggedCandidateScorer.from_bridge_schema(
                    schema,
                    scorer_config,
                )

            self.controller = CategoricalTorchBehaviorController(
                self.publisher,
                scorer_factory,
                behavior_config,
                torch.Generator().manual_seed(451),
            )
            self.controller.promote_live(self.shadow, training_step=0)
            self.trainer = SynchronousCombatWinTrainer(
                self.shadow,
                optimizer_config.create(self.shadow.parameters()),
                registry,
                CONCAT_LIMITS,
                behavior_config,
                objective_config,
            )
            self.runner = BoundedCombatWinGenerationRunner(
                source,
                slot_index=0,
                replicate_count=2,
                limits=LIMITS,
                trainer=self.trainer,
                controller=self.controller,
                shadow_scorer=self.shadow,
            )


    def _owners(root: Path, source: ExactCombatRootSource) -> _CombatOwners:
        return _CombatOwners(root, source)


if __name__ == "__main__":
    unittest.main()
