from __future__ import annotations

import importlib.util
import unittest
from dataclasses import replace

from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.torch_combat_fixtures import OneRoundCombatGroup


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    from sts_learning import CombatWinObjectiveConfig
    from sts_learning.torch_combat_census import CombatWinSignalCensusRunner
    from sts_learning.torch_combat_census import CombatWinSignalCensusResult
    from sts_learning.torch_combat_session_config import (
        CombatSessionBridge,
        CombatWinSessionConfig,
        CombatWinSessionProfile,
        TorchCombatSessionError,
    )
    from sts_learning.torch_policy import RaggedScorerConfig


class _IndexedCombatRootSource:
    def __init__(self) -> None:
        self.specifications = (
            ("12" * 32, "ab" * 32, (True, False)),
            ("34" * 32, "cd" * 32, (True, True)),
        )
        self.calls: list[int] = []

    def combat_group(self, slot_index: int, replicate_count: int):
        if replicate_count != 2:
            raise AssertionError("census changed the replicate count")
        self.calls.append(slot_index)
        return OneRoundCombatGroup(*self.specifications[slot_index])


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class CombatWinSignalCensusRunnerTests(unittest.TestCase):
    def test_distinct_roots_run_from_equal_models_and_form_one_census(self) -> None:
        source = _IndexedCombatRootSource()
        loader_calls = 0

        def loader(payload, **kwargs):
            nonlocal loader_calls
            loader_calls += 1
            return source

        runner = CombatWinSignalCensusRunner(
            CombatSessionBridge(
                combat_roots_from_artifact=loader,
                semantic_schema=semantic_schema_fixture(),
            ),
            CombatWinSessionConfig(
                expected_roots=2,
                replicate_count=2,
                profile=CombatWinSessionProfile(
                    scorer=RaggedScorerConfig(hidden_dim=4, relation_layers=1),
                ),
            ),
            max_roots=2,
        )

        result = runner.run_from_artifact_bytes(
            b"opaque-two-root-artifact",
            model_seed=41,
            behavior_seeds=(92, 93),
        )

        self.assertEqual(loader_calls, 1)
        self.assertEqual(source.calls, [0, 1])
        self.assertEqual(result.census.group_count, 2)
        self.assertEqual(result.census.replicate_count, 4)
        self.assertEqual(result.census.win.signal_group_count, 1)
        self.assertEqual(result.frontier.survival_frontier_slots, (0,))
        self.assertEqual(result.frontier.resource_frontier_slots, ())
        self.assertEqual(result.frontier.training_slots, (0,))
        self.assertEqual(result.frontier.rescue_slots, ())
        self.assertEqual(result.frontier.solved_slots, (1,))
        self.assertEqual(
            tuple(generation.root_id for generation in result.generations),
            ("12" * 32, "34" * 32),
        )
        self.assertEqual(
            result.generations[0].active_manifest_id_before,
            result.generations[1].active_manifest_id_before,
        )
        with self.assertRaisesRegex(TorchCombatSessionError, "one-group"):
            CombatWinSignalCensusResult(
                result.generations,
                result.census,
                replace(
                    result.frontier,
                    objective_config=CombatWinObjectiveConfig(
                        groups_per_update=2,
                    ),
                ),
            )

    def test_root_and_behavior_seed_bounds_fail_before_loading(self) -> None:
        loader_calls = 0

        def loader(payload, **kwargs):
            nonlocal loader_calls
            loader_calls += 1
            return _IndexedCombatRootSource()

        bridge = CombatSessionBridge(
            combat_roots_from_artifact=loader,
            semantic_schema=semantic_schema_fixture(),
        )
        config = CombatWinSessionConfig(expected_roots=2, replicate_count=2)
        with self.assertRaisesRegex(TorchCombatSessionError, "exceed"):
            CombatWinSignalCensusRunner(bridge, config, max_roots=1)

        runner = CombatWinSignalCensusRunner(bridge, config, max_roots=2)
        with self.assertRaisesRegex(TorchCombatSessionError, "one behavior seed"):
            runner.run_from_artifact_bytes(
                b"opaque-two-root-artifact",
                model_seed=41,
                behavior_seeds=(92,),
            )
        with self.assertRaisesRegex(TorchCombatSessionError, "not bool"):
            runner.run_from_artifact_bytes(
                b"opaque-two-root-artifact",
                model_seed=41,
                behavior_seeds=(92, True),
            )
        with self.assertRaisesRegex(TorchCombatSessionError, "distinct"):
            runner.run_from_artifact_bytes(
                b"opaque-two-root-artifact",
                model_seed=41,
                behavior_seeds=(92, 92),
            )
        self.assertEqual(loader_calls, 0)


if __name__ == "__main__":
    unittest.main()
