from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

from learning.tests.semantic_fixtures import (
    semantic_batch_fixture,
    semantic_schema_fixture,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_checkpoints import (
        BoundedTorchCheckpointStore,
        TorchCheckpointError,
        TorchCheckpointLimits,
    )
    from sts_learning.torch_policy import RaggedCandidateScorer, RaggedScorerConfig


def _limits(
    *,
    checkpoints: int = 2,
    per_checkpoint: int = 2 * 1024 * 1024,
    total: int = 4 * 1024 * 1024,
):
    return TorchCheckpointLimits(
        max_checkpoints=checkpoints,
        max_bytes_per_checkpoint=per_checkpoint,
        max_total_bytes=total,
    )


def _scorer(*, hidden_dim: int = 8):
    return RaggedCandidateScorer.from_bridge_schema(
        semantic_schema_fixture(),
        RaggedScorerConfig(hidden_dim=hidden_dim, relation_layers=1),
    )


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class BoundedTorchCheckpointStoreTests(unittest.TestCase):
    def test_deterministic_checkpoint_restores_identical_logits_after_restart(self) -> None:
        torch.manual_seed(12)
        scorer = _scorer()
        expected = scorer(semantic_batch_fixture()).values.detach().clone()

        with tempfile.TemporaryDirectory() as root:
            store = BoundedTorchCheckpointStore(root, _limits())
            first = store.prepare(scorer)
            second = store.prepare(scorer)
            self.assertEqual(first.artifact_id, second.artifact_id)
            store.commit(first)
            store.commit(second)
            self.assertEqual(store.snapshot.checkpoints, 1)

            with torch.no_grad():
                next(scorer.parameters()).add_(10.0)
            restarted = BoundedTorchCheckpointStore(root, _limits())
            restored = restarted.materialize(first.artifact_id, _scorer)
            actual = restored(semantic_batch_fixture()).values.detach()

            torch.testing.assert_close(actual, expected)
            self.assertEqual(
                restarted.prepare(restored).artifact_id,
                first.artifact_id,
            )

    def test_capacity_never_evicts_an_existing_checkpoint(self) -> None:
        torch.manual_seed(13)
        scorer = _scorer()
        with tempfile.TemporaryDirectory() as root:
            store = BoundedTorchCheckpointStore(
                root,
                _limits(checkpoints=1, total=2 * 1024 * 1024),
            )
            first = store.prepare(scorer)
            store.commit(first)
            with torch.no_grad():
                next(scorer.parameters()).add_(1.0)
            updated = store.prepare(scorer)

            self.assertNotEqual(first.artifact_id, updated.artifact_id)
            with self.assertRaisesRegex(TorchCheckpointError, "capacity"):
                store.commit(updated)
            self.assertEqual(store.snapshot.checkpoints, 1)
            self.assertEqual(store.commit(first), first.artifact_id)
            with self.assertRaisesRegex(TorchCheckpointError, "capacity"):
                store.preview_novel_commit(first)

    def test_per_checkpoint_limit_fails_before_any_file_is_published(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            store = BoundedTorchCheckpointStore(
                root,
                _limits(checkpoints=1, per_checkpoint=128, total=128),
            )

            with self.assertRaisesRegex(TorchCheckpointError, "byte limit"):
                store.prepare(_scorer())
            self.assertEqual(tuple(Path(root).iterdir()), ())

    def test_corruption_and_unowned_files_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            store = BoundedTorchCheckpointStore(root, _limits())
            prepared = store.prepare(_scorer())
            store.commit(prepared)
            path = next(Path(root).glob("*.ststorch"))
            path.write_bytes(path.read_bytes() + b"corrupt")

            with self.assertRaisesRegex(TorchCheckpointError, "changed"):
                store.materialize(prepared.artifact_id, _scorer)
            with self.assertRaisesRegex(TorchCheckpointError, "corrupt"):
                BoundedTorchCheckpointStore(root, _limits())

        with tempfile.TemporaryDirectory() as root:
            Path(root, ".pending-crash.tmp").write_bytes(b"partial")
            with self.assertRaisesRegex(TorchCheckpointError, "unexpected file"):
                BoundedTorchCheckpointStore(root, _limits())

    def test_incompatible_factory_cannot_partly_mutate_an_existing_model(self) -> None:
        torch.manual_seed(14)
        scorer = _scorer()
        untouched = _scorer(hidden_dim=9)
        before = tuple(tensor.detach().clone() for tensor in untouched.state_dict().values())
        with tempfile.TemporaryDirectory() as root:
            store = BoundedTorchCheckpointStore(root, _limits())
            artifact_id = store.commit(store.prepare(scorer))

            with self.assertRaisesRegex(TorchCheckpointError, "shape"):
                store.materialize(artifact_id, lambda: untouched)
            for expected, actual in zip(
                before,
                untouched.state_dict().values(),
                strict=True,
            ):
                torch.testing.assert_close(actual, expected)


if __name__ == "__main__":
    unittest.main()
