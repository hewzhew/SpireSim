from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from sts_learning.resume_store import (
    BoundedResumeStore,
    ResumeComponentKind,
    ResumeStoreError,
    ResumeStoreLimits,
)


def _limits(*, max_components: int = 12) -> ResumeStoreLimits:
    return ResumeStoreLimits(
        max_components=max_components,
        max_bytes_per_component=4096,
        max_total_component_bytes=48 * 1024,
        max_manifests=2,
        max_bytes_per_manifest=1024,
        max_total_manifest_bytes=2048,
    )


def _payloads(marker: bytes = b"v1") -> dict[ResumeComponentKind, bytes]:
    return {
        kind: b"component:" + bytes([int(kind)]) + b":" + marker
        for kind in ResumeComponentKind
    }


class BoundedResumeStoreTests(unittest.TestCase):
    def test_components_publish_before_one_reopenable_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            store = BoundedResumeStore(root, _limits())
            prepared = store.prepare(_payloads())
            self.assertEqual(store.snapshot.components, 0)
            self.assertEqual(store.snapshot.manifests, 0)
            self.assertEqual(store.preview_commit(prepared), prepared.manifest_id)
            self.assertEqual(store.snapshot.components, 0)

            manifest_id = store.commit(prepared)
            self.assertEqual(store.snapshot.components, 6)
            self.assertEqual(store.snapshot.manifests, 1)
            self.assertEqual(store.resolve(manifest_id), _payloads())

            reopened = BoundedResumeStore(root, _limits())
            self.assertEqual(reopened.manifest_ids, (manifest_id,))
            self.assertEqual(reopened.resolve(manifest_id), _payloads())
            self.assertEqual(reopened.commit(reopened.prepare(_payloads())), manifest_id)
            self.assertEqual(reopened.snapshot.components, 6)
            self.assertEqual(reopened.snapshot.manifests, 1)

    def test_batch_capacity_fails_before_any_component_or_manifest_commit(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            store = BoundedResumeStore(root, _limits(max_components=5))
            prepared = store.prepare(_payloads())

            with self.assertRaisesRegex(ResumeStoreError, "capacity"):
                store.commit(prepared)
            self.assertEqual(store.snapshot.components, 0)
            self.assertEqual(store.snapshot.manifests, 0)

    def test_missing_component_and_corrupt_reopen_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            store = BoundedResumeStore(root, _limits())
            manifest_id = store.commit(store.prepare(_payloads()))
            component_path = next((Path(root) / "components").iterdir())
            component_path.write_bytes(component_path.read_bytes() + b"corrupt")

            with self.assertRaisesRegex(ResumeStoreError, "corrupt"):
                BoundedResumeStore(root, _limits())
            self.assertEqual(store.manifest_ids, (manifest_id,))

    def test_publication_requires_every_typed_component(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            store = BoundedResumeStore(root, _limits())
            incomplete = _payloads()
            del incomplete[ResumeComponentKind.OPTIMIZER]
            with self.assertRaisesRegex(ResumeStoreError, "every component"):
                store.prepare(incomplete)


if __name__ == "__main__":
    unittest.main()
