from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

import numpy as np

from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.torch_combat_fixtures import (
    COMBAT_HASH,
    ROOT_ID,
    ExactCombatRootSource,
)
from sts_learning import (
    CombatExperienceLimits,
    CombatWinObjectiveConfig,
    SemanticBatchConcatLimits,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
_BRIDGE_AVAILABLE = importlib.util.find_spec("sts_learning_bridge") is not None
if _TORCH_AVAILABLE:
    from sts_learning.torch_combat_session import CombatWinSessionFactory
    from sts_learning.torch_combat_session_config import (
        CombatSessionBridge,
        CombatWinSessionConfig,
        CombatWinSessionLimits,
        CombatWinSessionProfile,
        TorchCombatSessionError,
    )
    from sts_learning.torch_combat_training import CombatWinTrainingStatus
    from sts_learning.torch_policy import (
        RaggedCategoricalPolicyConfig,
        RaggedScorerConfig,
    )


ARTIFACT = b"opaque-production-combat-root"


class _ArtifactLoader:
    def __init__(self, source: ExactCombatRootSource) -> None:
        self.source = source
        self.calls: list[tuple[bytes, int, int]] = []
        self.fail_once = False

    def __call__(self, payload, *, expected_roots, max_bytes):
        self.calls.append((payload, expected_roots, max_bytes))
        if self.fail_once:
            self.fail_once = False
            raise ValueError("invalid artifact")
        return self.source


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class CombatWinSessionTests(unittest.TestCase):
    def test_artifact_session_advances_and_publishes_only_explicitly(self) -> None:
        source = ExactCombatRootSource(((ROOT_ID, COMBAT_HASH, (True, False)),))
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            artifact = root_path / "root.bin"
            artifact.write_bytes(ARTIFACT)
            factory, loader = _factory(root_path / "session", source)
            session = factory.new_from_artifact_file(
                artifact,
                model_seed=41,
                behavior_seed=92,
            )
            active_before = session.active_behavior_manifest_id
            self.assertEqual(session.artifact_byte_count, len(ARTIFACT))
            self.assertEqual(
                loader.calls,
                [(ARTIFACT, 1, factory.config.limits.max_artifact_bytes)],
            )
            self.assertEqual(_files(factory.root), ())

            result = session.advance()

            self.assertTrue(result.promoted)
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
            with self.assertRaisesRegex(TorchCombatSessionError, "unused"):
                factory.new_from_artifact_file(
                    artifact,
                    model_seed=41,
                    behavior_seed=92,
                )

    def test_file_size_limit_rejects_before_reading_or_importing(self) -> None:
        source = ExactCombatRootSource(((ROOT_ID, COMBAT_HASH, (True, False)),))
        with tempfile.TemporaryDirectory() as root:
            root_path = Path(root)
            factory, loader = _factory(
                root_path / "session",
                source,
                max_artifact_bytes=4,
            )
            artifact = root_path / "root.bin"
            artifact.write_bytes(b"12345")

            with self.assertRaisesRegex(TorchCombatSessionError, "byte limit"):
                factory.new_from_artifact_file(
                    artifact,
                    model_seed=41,
                    behavior_seed=92,
                )

            self.assertEqual(loader.calls, [])
            self.assertEqual(_files(factory.root), ())

    def test_failed_import_keeps_the_unused_root_retryable(self) -> None:
        source = ExactCombatRootSource(((ROOT_ID, COMBAT_HASH, (True, False)),))
        with tempfile.TemporaryDirectory() as root:
            factory, loader = _factory(Path(root), source)
            loader.fail_once = True

            with self.assertRaisesRegex(TorchCombatSessionError, "import failed"):
                factory.new_from_artifact_bytes(
                    ARTIFACT,
                    model_seed=41,
                    behavior_seed=92,
                )
            self.assertEqual(_files(Path(root)), ())

            session = factory.new_from_artifact_bytes(
                ARTIFACT,
                model_seed=41,
                behavior_seed=92,
            )

            self.assertEqual(session.artifact_byte_count, len(ARTIFACT))
            self.assertEqual(len(loader.calls), 2)

    def test_config_rejects_ambiguous_root_and_objective_shapes(self) -> None:
        with self.assertRaisesRegex(TorchCombatSessionError, "below expected"):
            CombatWinSessionConfig(expected_roots=1, root_slot_index=1)
        with self.assertRaisesRegex(TorchCombatSessionError, "two replicates"):
            CombatWinSessionConfig(replicate_count=1)
        with self.assertRaisesRegex(TorchCombatSessionError, "one group"):
            CombatWinSessionConfig(
                profile=CombatWinSessionProfile(
                    objective=CombatWinObjectiveConfig(groups_per_update=2)
                )
            )
        with self.assertRaisesRegex(TorchCombatSessionError, "relation-aware"):
            CombatWinSessionProfile(
                scorer=RaggedScorerConfig(hidden_dim=4, relation_layers=0)
            )


@unittest.skipUnless(
    _TORCH_AVAILABLE and _BRIDGE_AVAILABLE,
    "optional PyTorch dependency or standalone bridge wheel is not installed",
)
class InstalledCombatSessionBridgeTests(unittest.TestCase):
    def test_installed_bridge_exposes_the_opaque_artifact_loader(self) -> None:
        bridge = CombatSessionBridge.installed()

        self.assertTrue(callable(bridge.combat_roots_from_artifact))
        self.assertGreater(int(bridge.semantic_schema["version"]), 0)

    def test_compact_session_runs_one_real_bridge_combat_group(self) -> None:
        from sts_learning_bridge import semantic_schema

        source = _first_real_combat_root()
        with tempfile.TemporaryDirectory() as root:
            session = CombatWinSessionFactory(
                Path(root),
                CombatSessionBridge(
                    combat_roots_from_artifact=lambda payload, **kwargs: source,
                    semantic_schema=semantic_schema(),
                ),
                CombatWinSessionConfig(
                    replicate_count=2,
                    profile=CombatWinSessionProfile(
                        scorer=RaggedScorerConfig(
                            hidden_dim=4,
                            relation_layers=1,
                        ),
                    ),
                ),
            ).new_from_artifact_bytes(
                ARTIFACT,
                model_seed=41,
                behavior_seed=92,
            )

            result = session.advance()

            self.assertEqual(result.replicate_count, 2)
            self.assertEqual(result.wins + result.losses, 2)
            self.assertGreater(result.model_rounds, 0)
            self.assertGreater(result.transitions, 0)
            self.assertIn(0, dict(source.combat_root_contexts()))

    def test_real_bridge_rebases_one_explicit_combat_replicate(self) -> None:
        source = _first_real_combat_root()
        group = source.combat_group(0, 1)
        original = (group.root_id, group.exact_combat_state_hash)
        while not group.ready:
            decision = group.decision_batch(semantic=False)
            group.choose([0] * len(decision["slot_indices"]))
        group.step()

        recovery = group.capture_recovery_root(0)
        self.assertEqual(
            (recovery.source_root_id, recovery.source_exact_combat_state_hash),
            original,
        )
        self.assertEqual(recovery.source_replicate_index, 0)
        self.assertNotEqual(
            (recovery.root_id, recovery.exact_combat_state_hash),
            original,
        )
        recovered_group = recovery.spawn_group(2)
        self.assertEqual(recovered_group.replicate_count, 2)
        self.assertEqual(recovered_group.root_id, recovery.root_id)


if _TORCH_AVAILABLE:

    def _factory(
        root: Path,
        source: ExactCombatRootSource,
        *,
        max_artifact_bytes: int = 1024,
    ) -> tuple[CombatWinSessionFactory, _ArtifactLoader]:
        loader = _ArtifactLoader(source)
        factory = CombatWinSessionFactory(
            root,
            CombatSessionBridge(
                combat_roots_from_artifact=loader,
                semantic_schema=semantic_schema_fixture(),
            ),
            CombatWinSessionConfig(
                replicate_count=2,
                profile=CombatWinSessionProfile(
                    scorer=RaggedScorerConfig(
                        hidden_dim=4,
                        relation_layers=1,
                    ),
                    behavior=RaggedCategoricalPolicyConfig(temperature=0.8),
                ),
                limits=CombatWinSessionLimits(
                    owner_capacity=2,
                    max_artifact_bytes=max_artifact_bytes,
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


if _TORCH_AVAILABLE and _BRIDGE_AVAILABLE:

    def _first_real_combat_root():
        from sts_learning_bridge import LearningBatchEnv, PHASE_COMBAT_ROOT

        source = LearningBatchEnv([11], 20)
        for _ in range(32):
            if not source.ready:
                batch = source.decision_batch(semantic=False)
                if np.any(batch["phase"] == PHASE_COMBAT_ROOT):
                    return source
            source.choose(
                list(batch["decision_token_ids"]),
                [0] * len(batch["slot_indices"]),
            )
            if source.ready:
                source.step()
            if source.terminal_count:
                break
        raise AssertionError("fixture seed did not reach a combat root")


def _files(root: Path) -> tuple[Path, ...]:
    return tuple(sorted(path for path in root.rglob("*") if path.is_file()))


if __name__ == "__main__":
    unittest.main()
