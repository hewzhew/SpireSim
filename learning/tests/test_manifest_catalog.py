from __future__ import annotations

import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

from learning.tests.torch_outcome_fixtures import behavior_manifest_fixture
from sts_learning import (
    BehaviorManifestCatalogError,
    BehaviorManifestCatalogLimits,
    BehaviorManifestRegistry,
    BehaviorManifestId,
    BoundedBehaviorManifestCatalog,
)


def _limits(*, manifests: int = 2) -> BehaviorManifestCatalogLimits:
    return BehaviorManifestCatalogLimits(
        max_manifests=manifests,
        max_bytes_per_manifest=1024,
        max_total_bytes=manifests * 1024,
    )


class BehaviorManifestCatalogTests(unittest.TestCase):
    def test_reopen_resolves_exact_manifest_and_atomically_hydrates_registry(self) -> None:
        first = behavior_manifest_fixture()
        second = replace(first, training_step=1)
        with tempfile.TemporaryDirectory() as root:
            catalog = BoundedBehaviorManifestCatalog(root, _limits())
            first_id = catalog.commit(catalog.prepare(first))
            second_id = catalog.commit(catalog.prepare(second))
            reopened = BoundedBehaviorManifestCatalog(root, _limits())
            registry = BehaviorManifestRegistry(capacity=2)

            hydrated = reopened.hydrate_registry(registry)

            self.assertEqual(set(hydrated), {first_id, second_id})
            self.assertEqual(reopened.resolve(first_id), first)
            self.assertEqual(registry.resolve(second_id), second)
            self.assertEqual(reopened.snapshot.manifests, 2)

    def test_hydration_capacity_failure_does_not_partly_fill_registry(self) -> None:
        first = behavior_manifest_fixture()
        second = replace(first, training_step=1)
        with tempfile.TemporaryDirectory() as root:
            catalog = BoundedBehaviorManifestCatalog(root, _limits())
            catalog.commit(catalog.prepare(first))
            catalog.commit(catalog.prepare(second))
            registry = BehaviorManifestRegistry(capacity=1)

            with self.assertRaisesRegex(ValueError, "capacity"):
                catalog.hydrate_registry(registry)
            self.assertEqual(registry.snapshot.registered_manifests, 0)

    def test_capacity_corruption_and_partial_files_fail_closed(self) -> None:
        first = behavior_manifest_fixture()
        second = replace(first, training_step=1)
        with tempfile.TemporaryDirectory() as root:
            catalog = BoundedBehaviorManifestCatalog(root, _limits(manifests=1))
            catalog.commit(catalog.prepare(first))
            with self.assertRaisesRegex(BehaviorManifestCatalogError, "capacity"):
                catalog.commit(catalog.prepare(second))
            self.assertEqual(catalog.snapshot.manifests, 1)
            self.assertEqual(
                catalog.preview_commit(catalog.prepare(first)),
                first.identity,
            )
            with self.assertRaisesRegex(BehaviorManifestCatalogError, "capacity"):
                catalog.preview_novel_commit(catalog.prepare(first))

            path = next(Path(root).glob("*.stsmanifest"))
            path.write_bytes(path.read_bytes() + b"corrupt")
            with self.assertRaisesRegex(BehaviorManifestCatalogError, "corrupt"):
                BoundedBehaviorManifestCatalog(root, _limits(manifests=1))

        with tempfile.TemporaryDirectory() as root:
            Path(root, ".pending-crash.tmp").write_bytes(b"partial")
            with self.assertRaisesRegex(BehaviorManifestCatalogError, "unexpected file"):
                BoundedBehaviorManifestCatalog(root, _limits())

    def test_unknown_manifest_id_is_never_guessed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            catalog = BoundedBehaviorManifestCatalog(root, _limits())
            with self.assertRaisesRegex(
                BehaviorManifestCatalogError,
                "unknown durable",
            ):
                catalog.resolve(BehaviorManifestId(b"\xff" * 32))


if __name__ == "__main__":
    unittest.main()
