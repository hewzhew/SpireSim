from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.torch_combat_fixtures import OneRoundCombatGroup
from sts_learning import (
    BehaviorManifestCatalogLimits,
    BehaviorManifestRegistry,
    BoundedBehaviorManifestCatalog,
    CombatAllLossAxis,
    CombatAllWinAxis,
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
    from sts_learning.torch_combat_batch_generation import (
        BoundedCombatWinBatchGenerationRunner,
        TorchCombatBatchGenerationError,
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


ROOTS = (
    ("12" * 32, "ab" * 32),
    ("34" * 32, "cd" * 32),
)
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


class _IndexedCombatRootSource:
    def __init__(
        self,
        wins: tuple[tuple[bool, bool], tuple[bool, bool]],
        *,
        final_hps: tuple[tuple[int, int] | None, tuple[int, int] | None] = (
            None,
            None,
        ),
        enemy_final_hps: tuple[
            tuple[int, int] | None,
            tuple[int, int] | None,
        ] = (None, None),
        terminal_kinds: tuple[
            tuple[int, int] | None,
            tuple[int, int] | None,
        ] = (None, None),
        repeated_root: bool = False,
        fail_slot: int | None = None,
    ) -> None:
        self.wins = wins
        self.final_hps = final_hps
        self.enemy_final_hps = enemy_final_hps
        self.terminal_kinds = terminal_kinds
        self.repeated_root = repeated_root
        self.fail_slot = fail_slot
        self.calls: list[int] = []
        self.groups: list[OneRoundCombatGroup] = []

    def combat_group(self, slot_index: int, replicate_count: int):
        if replicate_count != 2:
            raise AssertionError("batch generation changed the replicate count")
        self.calls.append(slot_index)
        if slot_index == self.fail_slot:
            raise RuntimeError("declared source failure")
        root = ROOTS[0] if self.repeated_root else ROOTS[slot_index]
        group = OneRoundCombatGroup(
            *root,
            self.wins[slot_index],
            final_hps=self.final_hps[slot_index],
            enemy_final_hps=self.enemy_final_hps[slot_index],
            terminal_kinds=self.terminal_kinds[slot_index],
        )
        self.groups.append(group)
        return group


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class BoundedCombatWinBatchGenerationRunnerTests(unittest.TestCase):
    def test_distinct_roots_train_once_then_promote_once(self) -> None:
        source = _IndexedCombatRootSource(((True, False), (True, True)))
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(Path(root), source)
            controller_rng_before = owners.controller.generator.get_state().clone()
            root_rng_before = tuple(
                generator.get_state().clone()
                for generator in owners.behavior_generators
            )

            result = owners.runner.advance()

            self.assertTrue(result.promoted)
            self.assertEqual(source.calls, [0, 1])
            self.assertEqual(len(result.roots), 2)
            self.assertEqual(result.training.group_count, 2)
            self.assertEqual(result.training.signal_group_count, 1)
            self.assertEqual(
                result.training.status,
                CombatWinTrainingStatus.OPTIMIZER_STEP,
            )
            self.assertEqual(owners.trainer.snapshot.deliveries, 1)
            self.assertEqual(owners.trainer.snapshot.optimizer_steps, 1)
            self.assertEqual(owners.controller.snapshot.active_training_step, 1)
            self.assertEqual(owners.controller.snapshot.successful_promotions, 2)
            self.assertTrue(
                torch.equal(
                    owners.controller.generator.get_state(),
                    controller_rng_before,
                )
            )
            self.assertTrue(
                all(
                    not torch.equal(generator.get_state(), before)
                    for generator, before in zip(
                        owners.behavior_generators,
                        root_rng_before,
                        strict=True,
                    )
                )
            )

    def test_no_win_signal_keeps_one_unchanged_behavior(self) -> None:
        source = _IndexedCombatRootSource(((True, True), (False, False)))
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(Path(root), source)
            active_before = owners.controller.snapshot

            result = owners.runner.advance()

            self.assertFalse(result.promoted)
            self.assertEqual(
                result.training.status,
                CombatWinTrainingStatus.NO_OBJECTIVE_SIGNAL,
            )
            self.assertEqual(result.training.signal_group_count, 0)
            self.assertEqual(owners.controller.snapshot, active_before)
            self.assertEqual(owners.trainer.snapshot.optimizer_steps, 0)

    def test_win_only_axis_ignores_eligible_hp_signal_consistently(self) -> None:
        source = _IndexedCombatRootSource(
            ((True, True), (False, False)),
            final_hps=((70, 50), None),
        )
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(
                Path(root),
                source,
                all_win_axis=CombatAllWinAxis.NONE,
            )

            result = owners.runner.advance()

            self.assertFalse(result.promoted)
            self.assertIs(result.training.all_win_axis, CombatAllWinAxis.NONE)
            self.assertEqual(result.training.signal_group_count, 0)
            self.assertEqual(result.training.terminal_hp_signal_group_count, 0)

    def test_all_loss_axis_counts_only_exact_losses_across_the_batch(self) -> None:
        source = _IndexedCombatRootSource(
            ((False, False), (False, False)),
            enemy_final_hps=((30, 10), (30, 10)),
            terminal_kinds=((1, 1), (2, 2)),
        )
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(
                Path(root),
                source,
                all_loss_axis=CombatAllLossAxis.ENEMY_HP_PROGRESS,
            )

            result = owners.runner.advance()

            self.assertIs(
                result.training.all_loss_axis,
                CombatAllLossAxis.ENEMY_HP_PROGRESS,
            )
            self.assertEqual(result.training.signal_group_count, 1)
            self.assertEqual(
                result.training.enemy_hp_progress_signal_group_count,
                1,
            )
            self.assertEqual(
                (result.roots[0].losses, result.roots[0].unresolved),
                (2, 0),
            )
            self.assertEqual(
                (result.roots[1].losses, result.roots[1].unresolved),
                (0, 2),
            )
            self.assertEqual(
                owners.trainer.snapshot.enemy_hp_progress_signal_groups,
                1,
            )

    def test_collection_failure_restores_rng_and_skips_training(self) -> None:
        source = _IndexedCombatRootSource(
            ((True, False), (True, False)),
            fail_slot=1,
        )
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(Path(root), source)
            states = tuple(
                generator.get_state().clone()
                for generator in owners.behavior_generators
            )
            parameters = tuple(
                parameter.detach().clone()
                for parameter in owners.shadow.parameters()
            )

            with self.assertRaisesRegex(RuntimeError, "declared source failure"):
                owners.runner.advance()

            self.assertEqual(source.calls, [0, 1])
            self.assertEqual(owners.trainer.snapshot.deliveries, 0)
            self.assertEqual(owners.controller.snapshot.active_training_step, 0)
            for generator, state in zip(
                owners.behavior_generators,
                states,
                strict=True,
            ):
                self.assertTrue(torch.equal(generator.get_state(), state))
            for parameter, before in zip(
                owners.shadow.parameters(),
                parameters,
                strict=True,
            ):
                torch.testing.assert_close(parameter.detach(), before)

    def test_repeated_root_fails_before_second_root_play_or_training(self) -> None:
        source = _IndexedCombatRootSource(
            ((True, False), (True, False)),
            repeated_root=True,
        )
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(Path(root), source)

            with self.assertRaisesRegex(
                TorchCombatBatchGenerationError,
                "repeated an exact root",
            ):
                owners.runner.advance()

            self.assertEqual(source.calls, [0, 1])
            self.assertEqual(source.groups[0].choose_calls, 1)
            self.assertEqual(source.groups[1].choose_calls, 0)
            self.assertEqual(owners.trainer.snapshot.deliveries, 0)

    def test_failed_promotion_retries_without_recollecting_or_retraining(self) -> None:
        source = _IndexedCombatRootSource(((True, False), (True, True)))
        with tempfile.TemporaryDirectory() as root:
            owners = _owners(Path(root), source)
            original = owners.publisher.bind_live
            calls = 0

            def fail_once(scorer, *, training_step):
                nonlocal calls
                calls += 1
                if calls == 1:
                    raise TorchBehaviorError("temporary batch promotion failure")
                return original(scorer, training_step=training_step)

            with patch.object(owners.publisher, "bind_live", side_effect=fail_once):
                with self.assertRaisesRegex(TorchBehaviorError, "temporary batch"):
                    owners.runner.advance()
                self.assertTrue(owners.runner.pending_promotion)
                self.assertEqual(source.calls, [0, 1])
                self.assertEqual(owners.trainer.snapshot.deliveries, 1)

                result = owners.runner.advance()

            self.assertTrue(result.promoted)
            self.assertFalse(owners.runner.pending_promotion)
            self.assertEqual(source.calls, [0, 1])
            self.assertEqual(owners.trainer.snapshot.deliveries, 1)
            self.assertEqual(owners.controller.snapshot.active_training_step, 1)


if _TORCH_AVAILABLE:

    class _CombatBatchOwners:
        def __init__(
            self,
            root: Path,
            source: _IndexedCombatRootSource,
            *,
            all_win_axis: CombatAllWinAxis = CombatAllWinAxis.TERMINAL_HP,
            all_loss_axis: CombatAllLossAxis = CombatAllLossAxis.NONE,
        ) -> None:
            schema = semantic_schema_fixture()
            scorer_config = RaggedScorerConfig(hidden_dim=4, relation_layers=0)
            behavior_config = RaggedCategoricalPolicyConfig(temperature=0.8)
            optimizer_config = AdamTrainingConfig(learning_rate=0.002)
            objective_config = CombatWinObjectiveConfig(
                groups_per_update=2,
                all_win_axis=all_win_axis,
                all_loss_axis=all_loss_axis,
            )
            self.shadow = RaggedCandidateScorer.from_bridge_schema(
                schema,
                scorer_config,
            )
            registry = BehaviorManifestRegistry(capacity=2)
            self.publisher = TorchBehaviorPublisher(
                BoundedTorchCheckpointStore(
                    root / "checkpoints",
                    TorchCheckpointLimits(
                        max_checkpoints=2,
                        max_bytes_per_checkpoint=2 * 1024 * 1024,
                        max_total_bytes=4 * 1024 * 1024,
                    ),
                ),
                BoundedBehaviorManifestCatalog(
                    root / "manifests",
                    BehaviorManifestCatalogLimits(
                        max_manifests=2,
                        max_bytes_per_manifest=1024,
                        max_total_bytes=2 * 1024,
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
            self.behavior_generators = (
                torch.Generator().manual_seed(101),
                torch.Generator().manual_seed(102),
            )
            self.runner = BoundedCombatWinBatchGenerationRunner(
                source,
                slot_indices=(0, 1),
                replicate_count=2,
                behavior_generators=self.behavior_generators,
                max_roots=2,
                limits=LIMITS,
                trainer=self.trainer,
                controller=self.controller,
                shadow_scorer=self.shadow,
            )


    def _owners(
        root: Path,
        source: _IndexedCombatRootSource,
        *,
        all_win_axis: CombatAllWinAxis = CombatAllWinAxis.TERMINAL_HP,
        all_loss_axis: CombatAllLossAxis = CombatAllLossAxis.NONE,
    ) -> _CombatBatchOwners:
        return _CombatBatchOwners(
            root,
            source,
            all_win_axis=all_win_axis,
            all_loss_axis=all_loss_axis,
        )


if __name__ == "__main__":
    unittest.main()
