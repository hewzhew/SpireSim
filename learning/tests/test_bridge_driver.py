from __future__ import annotations

import unittest
from collections.abc import Callable, Mapping

from learning.tests.policy_fixtures import BEHAVIOR_MANIFEST_ID
from sts_learning import (
    AttemptAssemblyDelivery,
    AttemptAssemblyLimits,
    BatchPolicyChoice,
    BoundedAttemptAssembler,
    ExperienceLimits,
    ExperienceSegment,
    ExperienceSegmentBuffer,
    HeldOutEvaluationSpec,
    OnlineBatchDriver,
    RecoveryPlan,
    RecoverySlotSnapshot,
    SeedPartition,
    SeedSchedule,
    TerminalAccountingBatch,
    initialize_population,
    iter_payload_arrays,
    evaluate_held_out_behavior,
)

try:
    from sts_learning_bridge import LearningBatchEnv
except ImportError:
    LearningBatchEnv = None  # type: ignore[assignment,misc]


class FirstLegalPolicy:
    behavior_manifest_id = BEHAVIOR_MANIFEST_ID

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        return BatchPolicyChoice.deterministic(
            [0] * len(decision_batch["slot_indices"]),  # type: ignore[arg-type]
            BEHAVIOR_MANIFEST_ID,
        )


class NoRecoveryCurriculum:
    def plan_recovery(
        self,
        accounting: TerminalAccountingBatch,
        snapshots: tuple[RecoverySlotSnapshot, ...],
    ) -> RecoveryPlan:
        return RecoveryPlan()


class CountingSegmentSink:
    def __init__(
        self,
        limits: ExperienceLimits,
        downstream: Callable[[ExperienceSegment], None] | None = None,
    ) -> None:
        self.limits = limits
        self.downstream = downstream
        self.segments = 0
        self.decisions = 0
        self.payload_bytes = 0
        self.terminal_attempts = 0

    def __call__(self, segment: ExperienceSegment) -> None:
        assert segment.decision_count <= self.limits.max_decisions
        assert segment.payload_bytes <= self.limits.max_payload_bytes
        for batch in segment.batches:
            assert all(
                not array.flags.writeable
                for array in iter_payload_arrays(batch.payload)
            )
        self.segments += 1
        self.decisions += segment.decision_count
        self.payload_bytes += segment.payload_bytes
        self.terminal_attempts += sum(
            fragment.terminal is not None for fragment in segment.attempts
        )
        if self.downstream is not None:
            self.downstream(segment)


class CountingAttemptSink:
    def __init__(self) -> None:
        self.completed = 0
        self.dropped = 0

    def __call__(self, delivery: AttemptAssemblyDelivery) -> None:
        self.completed += len(delivery.completed)
        self.dropped += len(delivery.dropped)


@unittest.skipIf(LearningBatchEnv is None, "standalone bridge wheel is not installed")
class BridgeDriverIntegrationTests(unittest.TestCase):
    def test_real_bridge_held_out_evaluation_repeats_exact_prefix(self) -> None:
        assert LearningBatchEnv is not None
        schedule = SeedSchedule(SeedPartition.HELD_OUT, next_candidate=23)
        spec = HeldOutEvaluationSpec(
            slot_count=1,
            terminal_attempt_target=1,
            max_batch_steps=160,
        )

        first = evaluate_held_out_behavior(
            lambda seeds: LearningBatchEnv(seeds, 20),
            FirstLegalPolicy(),
            schedule=schedule,
            spec=spec,
        )
        second = evaluate_held_out_behavior(
            lambda seeds: LearningBatchEnv(seeds, 20),
            FirstLegalPolicy(),
            schedule=schedule,
            spec=spec,
        )

        self.assertTrue(first.complete)
        self.assertTrue(second.complete)
        self.assertEqual(first.schedule_end, second.schedule_end)
        self.assertEqual(first.run.summary.batch_steps, second.run.summary.batch_steps)
        self.assertEqual(first.run.summary.victories, second.run.summary.victories)
        self.assertEqual(first.run.summary.defeats, second.run.summary.defeats)

    def test_real_bridge_runs_bounded_continuous_population(self) -> None:
        assert LearningBatchEnv is not None
        population = initialize_population(
            lambda seeds: LearningBatchEnv(seeds, 20),
            slot_count=3,
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            max_recoveries_per_episode=0,
        )
        limits = ExperienceLimits(
            max_decisions=48,
            max_payload_bytes=16 * 1024 * 1024,
        )
        attempt_sink = CountingAttemptSink()
        assembler = BoundedAttemptAssembler(
            AttemptAssemblyLimits(
                max_open_attempts=3,
                max_decisions_per_attempt=1_024,
                max_payload_bytes_per_attempt=64 * 1024 * 1024,
            ),
            attempt_sink,
        )
        sink = CountingSegmentSink(limits, assembler)
        driver = OnlineBatchDriver(
            population,
            policy=FirstLegalPolicy(),
            curriculum=NoRecoveryCurriculum(),
            experience_buffer=ExperienceSegmentBuffer(limits),
            experience_sink=sink,
        )

        summary = driver.run(batch_steps=160)
        flushed = driver.flush_experience()

        self.assertEqual(summary.batch_steps, 160)
        self.assertEqual(summary.slot_steps, 480)
        self.assertGreater(summary.terminal_attempts, 0)
        self.assertEqual(summary.victories + summary.defeats, summary.terminal_attempts)
        self.assertEqual(summary.terminal_attempts, summary.completed_episodes)
        self.assertEqual(summary.recoveries, 0)
        self.assertEqual(summary.active_slots, 3)
        self.assertGreater(summary.emitted_experience_segments, 0)
        self.assertLessEqual(
            summary.open_experience_decisions,
            limits.max_decisions,
        )
        self.assertLessEqual(
            summary.open_experience_payload_bytes,
            limits.max_payload_bytes,
        )
        self.assertIsNotNone(flushed)
        self.assertEqual(sink.segments, summary.emitted_experience_segments + 1)
        self.assertGreater(sink.decisions, summary.emitted_experience_decisions)
        self.assertGreater(sink.terminal_attempts, 0)
        self.assertEqual(attempt_sink.completed, summary.terminal_attempts)
        self.assertEqual(attempt_sink.dropped, 0)
        self.assertEqual(assembler.snapshot.completed_attempts, summary.terminal_attempts)
        self.assertLessEqual(assembler.snapshot.open_attempts, 3)
        self.assertLessEqual(
            assembler.snapshot.retained_decisions,
            assembler.limits.maximum_retained_decisions,
        )
        self.assertLessEqual(
            assembler.snapshot.retained_payload_bytes,
            assembler.limits.maximum_retained_payload_bytes,
        )


if __name__ == "__main__":
    unittest.main()
