from __future__ import annotations

import unittest

from learning.tests.torch_outcome_fixtures import (
    completed_attempt_fixture,
    decision_batch_fixture,
)
from sts_learning import (
    AttemptAssemblyDelivery,
    AttemptUpdateBatchError,
    AttemptUpdateBatchLimits,
    BehaviorManifestId,
    BoundedAttemptUpdateBatcher,
)


class BoundedAttemptUpdateBatcherTests(unittest.TestCase):
    def test_exact_target_delivers_once_and_smaller_prefix_does_not_train(self) -> None:
        deliveries: list[AttemptAssemblyDelivery] = []
        batcher = BoundedAttemptUpdateBatcher(2, _limits(), deliveries.append)

        batcher(_delivery(slot=0))

        self.assertEqual(deliveries, [])
        self.assertEqual(batcher.pending_attempts, 1)
        self.assertIsNone(batcher.last_update_payload_bytes)

        batcher(_delivery(slot=1))

        self.assertEqual(len(deliveries), 1)
        self.assertEqual(len(deliveries[0].completed), 2)
        self.assertEqual(batcher.pending_attempts, 0)
        self.assertEqual(batcher.last_update_payload_bytes,
                         sum(attempt.payload_bytes for attempt in deliveries[0].completed))
        batcher.require_quiescent()

    def test_mixed_manifest_and_overfull_delivery_fail_before_sink(self) -> None:
        deliveries: list[AttemptAssemblyDelivery] = []
        batcher = BoundedAttemptUpdateBatcher(2, _limits(), deliveries.append)
        batcher(_delivery(slot=0, manifest_byte=1))

        with self.assertRaisesRegex(AttemptUpdateBatchError, "mixes behavior"):
            batcher(_delivery(slot=1, manifest_byte=2))
        self.assertEqual(deliveries, [])
        self.assertEqual(batcher.pending_attempts, 1)

        overfull = BoundedAttemptUpdateBatcher(2, _limits(), deliveries.append)
        with self.assertRaisesRegex(AttemptUpdateBatchError, "exceeds the exact"):
            overfull(
                AttemptAssemblyDelivery(
                    completed=tuple(
                        _delivery(slot=slot).completed[0] for slot in range(3)
                    ),
                    dropped=(),
                )
            )
        self.assertEqual(overfull.pending_attempts, 0)

    def test_decision_and_payload_limits_reject_without_retaining_input(self) -> None:
        limits = AttemptUpdateBatchLimits(
            max_decisions_per_update=1,
            max_payload_bytes_per_update=1,
        )
        batcher = BoundedAttemptUpdateBatcher(2, limits, lambda delivery: None)
        batcher(_delivery(slot=0))

        with self.assertRaisesRegex(AttemptUpdateBatchError, "max_decisions"):
            batcher(_delivery(slot=1))
        self.assertEqual(batcher.pending_attempts, 1)
        self.assertEqual(batcher.pending_decisions, 1)

    def test_pending_payload_is_not_quiescent(self) -> None:
        batcher = BoundedAttemptUpdateBatcher(2, _limits(), lambda delivery: None)
        batcher(_delivery(slot=0))

        with self.assertRaisesRegex(AttemptUpdateBatchError, "pending"):
            batcher.require_quiescent()

    def test_sink_failure_releases_payload_and_poisons_owner(self) -> None:
        def fail(delivery: AttemptAssemblyDelivery) -> None:
            raise RuntimeError("sink failed")

        batcher = BoundedAttemptUpdateBatcher(2, _limits(), fail)
        batcher(_delivery(slot=0))

        with self.assertRaisesRegex(RuntimeError, "sink failed"):
            batcher(_delivery(slot=1))
        self.assertTrue(batcher.poisoned)
        self.assertEqual(batcher.pending_attempts, 0)
        with self.assertRaisesRegex(AttemptUpdateBatchError, "poisoned"):
            batcher(_delivery(slot=1))


def _limits() -> AttemptUpdateBatchLimits:
    return AttemptUpdateBatchLimits(
        max_decisions_per_update=8,
        max_payload_bytes_per_update=8,
    )


def _delivery(*, slot: int, manifest_byte: int = 1) -> AttemptAssemblyDelivery:
    batch = decision_batch_fixture(
        slot=slot,
        semantic_row=0,
        selected_ordinal=0,
        manifest_id=BehaviorManifestId(bytes([manifest_byte]) * 32),
    )
    return AttemptAssemblyDelivery(
        completed=(
            completed_attempt_fixture(slot=slot, batches=(batch,), reward=1),
        ),
        dropped=(),
    )


if __name__ == "__main__":
    unittest.main()
