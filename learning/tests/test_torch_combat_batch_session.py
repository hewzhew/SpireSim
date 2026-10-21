from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.torch_combat_fixtures import OneRoundCombatGroup
from sts_learning import (
    CombatExperienceLimits,
    CombatWinObjectiveConfig,
    SemanticBatchConcatLimits,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_combat_batch_session import (
        CombatWinBatchSessionFactory,
    )
    from sts_learning.torch_combat_session_config import (
        CombatSessionBridge,
        CombatWinBatchSessionConfig,
        CombatWinSessionLimits,
        CombatWinSessionProfile,
        TorchCombatSessionError,
    )
    from sts_learning.torch_combat_training import CombatWinTrainingStatus
    from sts_learning.torch_policy import (
        RaggedCategoricalPolicyConfig,
        RaggedCandidateScorer,
        RaggedScorerConfig,
    )


ARTIFACT = b"opaque-production-combat-root-batch"
ROOTS = (
    ("12" * 32, "ab" * 32, (True, False)),
    ("34" * 32, "cd" * 32, (True, True)),
)


class _IndexedCombatRootSource:
    def __init__(self) -> None:
        self.calls: list[int] = []

    def combat_group(self, slot_index: int, replicate_count: int):
        if replicate_count != 2:
            raise AssertionError("batch session changed the replicate count")
        self.calls.append(slot_index)
        return OneRoundCombatGroup(*ROOTS[slot_index])


class _ArtifactLoader:
    def __init__(self, source: _IndexedCombatRootSource) -> None:
        self.source = source
        self.calls: list[tuple[bytes, int, int]] = []

    def __call__(self, payload, *, expected_roots, max_bytes):
        self.calls.append((payload, expected_roots, max_bytes))
        return self.source


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class CombatWinBatchSessionTests(unittest.TestCase):
    def test_initial_scorer_is_copied_into_an_independent_trainable_shadow(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as root:
            source = _IndexedCombatRootSource()
            factory, _ = _factory(Path(root) / "session", source)
            initial = RaggedCandidateScorer.from_bridge_schema(
                semantic_schema_fixture(),
                factory.config.profile.scorer,
            )
            with torch.no_grad():
                for parameter in initial.parameters():
                    parameter.fill_(0.125)

            session = factory.new_from_artifact_bytes(
                ARTIFACT,
                model_seed=41,
                behavior_seeds=(92, 93),
                initial_scorer=initial,
            )

            shadow = session.runner.shadow_scorer
            for initial_value, shadow_value in zip(
                initial.state_dict().values(),
                shadow.state_dict().values(),
                strict=True,
            ):
                self.assertTrue(torch.equal(initial_value, shadow_value))
                self.assertNotEqual(initial_value.data_ptr(), shadow_value.data_ptr())

    def test_artifact_loads_once_and_one_batch_update_publishes_explicitly(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            source = _IndexedCombatRootSource()
            factory, loader = _factory(root_path / "session", source)
            session = factory.new_from_artifact_bytes(
                ARTIFACT,
                model_seed=41,
                behavior_seeds=(92, 93),
            )
            active_before = session.active_behavior_manifest_id

            result = session.advance()

            self.assertEqual(
                loader.calls,
                [
                    (
                        ARTIFACT,
                        2,
                        factory.config.limits.max_artifact_bytes,
                    )
                ],
            )
            self.assertEqual(source.calls, [0, 1])
            self.assertEqual(len(result.roots), 2)
            self.assertEqual(result.training.group_count, 2)
            self.assertEqual(
                result.training.status,
                CombatWinTrainingStatus.OPTIMIZER_STEP,
            )
            self.assertNotEqual(
                session.active_behavior_manifest_id,
                active_before,
            )
            self.assertEqual(_files(factory.root), ())

            publication = session.publish_active_behavior()

            self.assertEqual(
                publication.manifest_id,
                session.active_behavior_manifest_id,
            )
            self.assertEqual(len(_files(factory.root)), 2)

    def test_behavior_seed_shape_fails_before_artifact_import(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            source = _IndexedCombatRootSource()
            factory, loader = _factory(Path(root), source)

            with self.assertRaisesRegex(
                TorchCombatSessionError,
                "one behavior seed per root",
            ):
                factory.new_from_artifact_bytes(
                    ARTIFACT,
                    model_seed=41,
                    behavior_seeds=(92,),
                )
            with self.assertRaisesRegex(
                TorchCombatSessionError,
                "distinct behavior seeds",
            ):
                factory.new_from_artifact_bytes(
                    ARTIFACT,
                    model_seed=41,
                    behavior_seeds=(92, 92),
                )

            self.assertEqual(loader.calls, [])
            self.assertEqual(_files(factory.root), ())

    def test_config_allows_one_frontier_root_and_requires_exact_delivery_width(
        self,
    ) -> None:
        one_root = CombatWinBatchSessionConfig(expected_roots=1, max_roots=2)
        self.assertEqual(one_root.expected_roots, 1)
        with self.assertRaisesRegex(TorchCombatSessionError, "groups_per_update"):
            CombatWinBatchSessionConfig(expected_roots=2, max_roots=2)
        with self.assertRaisesRegex(TorchCombatSessionError, "exceed max_roots"):
            CombatWinBatchSessionConfig(expected_roots=3, max_roots=2)


if _TORCH_AVAILABLE:

    def _factory(
        root: Path,
        source: _IndexedCombatRootSource,
    ) -> tuple[CombatWinBatchSessionFactory, _ArtifactLoader]:
        loader = _ArtifactLoader(source)
        profile = CombatWinSessionProfile(
            scorer=RaggedScorerConfig(hidden_dim=4, relation_layers=1),
            behavior=RaggedCategoricalPolicyConfig(temperature=0.8),
            objective=CombatWinObjectiveConfig(groups_per_update=2),
        )
        factory = CombatWinBatchSessionFactory(
            root,
            CombatSessionBridge(
                combat_roots_from_artifact=loader,
                semantic_schema=semantic_schema_fixture(),
            ),
            CombatWinBatchSessionConfig(
                expected_roots=2,
                max_roots=2,
                replicate_count=2,
                profile=profile,
                limits=CombatWinSessionLimits(
                    owner_capacity=2,
                    max_artifact_bytes=1024,
                    experience=CombatExperienceLimits(
                        max_decisions=8,
                        max_payload_bytes=1024 * 1024,
                        max_model_rounds=4,
                        max_transitions=4,
                    ),
                    concat=SemanticBatchConcatLimits(
                        max_rows=8,
                        max_input_array_bytes=1024 * 1024,
                    ),
                    max_checkpoint_bytes=2 * 1024 * 1024,
                ),
            ),
        )
        return factory, loader


def _files(root: Path) -> tuple[Path, ...]:
    return tuple(sorted(path for path in root.rglob("*") if path.is_file()))


if __name__ == "__main__":
    unittest.main()
