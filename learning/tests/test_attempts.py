from __future__ import annotations

import unittest

from learning.tests.policy_fixtures import (
    BEHAVIOR_MANIFEST_ID,
    UPDATED_BEHAVIOR_MANIFEST_ID,
)
from learning.tests.semantic_fixtures import semantic_batch_fixture
from sts_learning import (
    AttemptAssemblyDelivery,
    AttemptAssemblyError,
    AttemptAssemblyLimits,
    AttemptDropReason,
    AttemptFragment,
    AttemptKey,
    BehaviorManifestId,
    BoundedAttemptAssembler,
    DecisionExperienceBatch,
    ExperienceSegment,
    PreparedDecisionBatch,
    RecoverySlotSnapshot,
    RecoverySlotStatus,
    SelectionProbability,
    SegmentCloseReason,
    TerminalAttemptOutcome,
    TerminalAttemptRecord,
    iter_payload_arrays,
)


def _snapshot(slot: int, *, generation: int = 0) -> RecoverySlotSnapshot:
    return RecoverySlotSnapshot(
        slot_index=slot,
        episode_seed=1_000 + slot,
        episode_generation=generation,
        attempt_index=1,
        recoveries_used=0,
        status=RecoverySlotStatus.ACTIVE,
        pending_terminal=None,
    )


def _experience_batch(
    *,
    generations: tuple[int, int] = (0, 0),
    behavior_manifest_id: BehaviorManifestId = BEHAVIOR_MANIFEST_ID,
    selection_probabilities: tuple[SelectionProbability, SelectionProbability] = (
        SelectionProbability.known(1.0),
        SelectionProbability.known(1.0),
    ),
) -> DecisionExperienceBatch:
    prepared = PreparedDecisionBatch.capture(
        semantic_batch_fixture(),
        [
            _snapshot(4, generation=generations[0]),
            _snapshot(9, generation=generations[1]),
        ],
    )
    return DecisionExperienceBatch.from_prepared(
        prepared,
        [1, 2],
        selection_probabilities,
        behavior_manifest_id,
    )


def _terminal(lineage) -> TerminalAttemptRecord:
    key = lineage.key
    return TerminalAttemptRecord(
        episode_seed=key.episode_seed,
        episode_generation=key.episode_generation,
        attempt_index=key.attempt_index,
        recoveries_used=lineage.recoveries_used,
        terminal=TerminalAttemptOutcome(
            slot_index=key.slot_index,
            terminal_reward=1,
            terminal_act=3,
            terminal_floor=40,
            terminal_hp=20,
            terminal_max_hp=80,
            terminal_gold=50,
        ),
    )


def _segment(
    sequence_index: int,
    batches: tuple[DecisionExperienceBatch, ...],
    *,
    terminal_keys: tuple[AttemptKey, ...] = (),
) -> ExperienceSegment:
    lineages = {}
    for batch in batches:
        for lineage in batch.lineages:
            lineages.setdefault(lineage.key, lineage)
    attempts = tuple(
        AttemptFragment(
            lineage=lineage,
            terminal=_terminal(lineage)
            if lineage.key in terminal_keys
            else None,
        )
        for lineage in lineages.values()
    )
    return ExperienceSegment(
        sequence_index=sequence_index,
        close_reason=SegmentCloseReason.EXPLICIT_FLUSH,
        batches=batches,
        attempts=attempts,
        decision_count=sum(batch.decision_count for batch in batches),
        payload_bytes=sum(batch.payload_bytes for batch in batches),
    )


class RecordingAttemptSink:
    def __init__(self) -> None:
        self.deliveries: list[AttemptAssemblyDelivery] = []
        self.fail = False

    def __call__(self, delivery: AttemptAssemblyDelivery) -> None:
        if self.fail:
            raise RuntimeError("sink failed")
        self.deliveries.append(delivery)


class BoundedAttemptAssemblerTests(unittest.TestCase):
    def test_censored_segments_join_into_exact_completed_attempts(self) -> None:
        sink = RecordingAttemptSink()
        assembler = BoundedAttemptAssembler(
            AttemptAssemblyLimits(
                max_open_attempts=2,
                max_decisions_per_attempt=8,
                max_payload_bytes_per_attempt=8 * 1024 * 1024,
            ),
            sink,
        )
        first = _experience_batch()
        second = _experience_batch(
            behavior_manifest_id=UPDATED_BEHAVIOR_MANIFEST_ID,
            selection_probabilities=(
                SelectionProbability.known(0.25),
                SelectionProbability.unknown(),
            ),
        )
        third = _experience_batch().select_rows([1])

        assembler(_segment(0, (first,)))
        assembler(_segment(1, (second,), terminal_keys=(first.lineages[0].key,)))
        assembler(_segment(2, (third,), terminal_keys=(third.lineages[0].key,)))

        self.assertEqual(len(sink.deliveries), 2)
        first_completed = sink.deliveries[0].completed[0]
        second_completed = sink.deliveries[1].completed[0]
        self.assertEqual(first_completed.lineage.key.slot_index, 4)
        self.assertEqual(first_completed.decision_count, 2)
        self.assertEqual(len(first_completed.batches), 2)
        self.assertEqual(
            tuple(batch.behavior_manifest_id for batch in first_completed.batches),
            (BEHAVIOR_MANIFEST_ID, UPDATED_BEHAVIOR_MANIFEST_ID),
        )
        self.assertEqual(
            tuple(
                probability.value
                for batch in first_completed.batches
                for probability in batch.selection_probabilities
            ),
            (1.0, 0.25),
        )
        self.assertTrue(
            all(
                int(batch.payload["slot_indices"][0]) == 4
                for batch in first_completed.batches
            )
        )
        self.assertTrue(
            all(
                not array.flags.writeable
                for batch in first_completed.batches
                for array in iter_payload_arrays(batch.payload)
            )
        )
        self.assertEqual(second_completed.lineage.key.slot_index, 9)
        self.assertEqual(second_completed.decision_count, 3)
        self.assertEqual(
            tuple(
                probability.value
                for batch in second_completed.batches
                for probability in batch.selection_probabilities
            ),
            (1.0, None, 1.0),
        )
        self.assertEqual(assembler.snapshot.open_attempts, 0)
        self.assertEqual(assembler.snapshot.completed_attempts, 2)

    def test_terminal_continuation_closes_only_a_previously_sampled_attempt(self) -> None:
        first = _experience_batch().select_rows([0])
        lineage = first.lineages[0]
        continuation = ExperienceSegment(sequence_index=1,
            close_reason=SegmentCloseReason.EXPLICIT_FLUSH, batches=(),
            attempts=(AttemptFragment(lineage, _terminal(lineage)),), decision_count=0, payload_bytes=0)
        sink = RecordingAttemptSink()
        assembler = BoundedAttemptAssembler(AttemptAssemblyLimits(
            max_open_attempts=2, max_decisions_per_attempt=8, max_payload_bytes_per_attempt=8*1024*1024), sink)
        assembler(_segment(0, (first,)))
        assembler(continuation)
        self.assertEqual(sink.deliveries[0].completed[0].decision_count, 1)
        retained = sink.deliveries[0].completed[0].batches
        self.assertEqual(len(retained), 1)
        self.assertEqual(retained[0].selected_ordinals, first.selected_ordinals)
        self.assertEqual(retained[0].selection_probabilities, first.selection_probabilities)
        self.assertEqual(assembler.snapshot.open_attempts, 0)
        fresh = BoundedAttemptAssembler(assembler.limits, RecordingAttemptSink())
        from dataclasses import replace
        with self.assertRaisesRegex(AttemptAssemblyError, "previously sampled"):
            fresh(replace(continuation, sequence_index=0))
        self.assertEqual(fresh.snapshot.next_sequence_index, 0)

    def test_decision_limit_releases_payload_and_reports_drop_at_terminal(self) -> None:
        sink = RecordingAttemptSink()
        assembler = BoundedAttemptAssembler(
            AttemptAssemblyLimits(
                max_open_attempts=1,
                max_decisions_per_attempt=1,
                max_payload_bytes_per_attempt=8 * 1024 * 1024,
            ),
            sink,
        )
        first = _experience_batch().select_rows([0])
        second = _experience_batch().select_rows([0])

        assembler(_segment(0, (first,)))
        assembler(_segment(1, (second,), terminal_keys=(first.lineages[0].key,)))

        dropped = sink.deliveries[0].dropped[0]
        self.assertEqual(dropped.reason, AttemptDropReason.DECISION_LIMIT)
        self.assertEqual(dropped.decision_count_at_drop, 2)
        self.assertGreater(dropped.payload_bytes_at_drop, first.payload_bytes)
        self.assertEqual(assembler.snapshot.retained_payload_bytes, 0)
        self.assertEqual(assembler.snapshot.dropped_attempts, 1)

    def test_payload_limit_marks_open_attempt_without_retaining_arrays(self) -> None:
        sink = RecordingAttemptSink()
        assembler = BoundedAttemptAssembler(
            AttemptAssemblyLimits(
                max_open_attempts=1,
                max_decisions_per_attempt=8,
                max_payload_bytes_per_attempt=1,
            ),
            sink,
        )
        batch = _experience_batch().select_rows([0])

        assembler(_segment(0, (batch,)))

        snapshot = assembler.snapshot
        self.assertEqual(snapshot.open_attempts, 1)
        self.assertEqual(snapshot.dropped_open_attempts, 1)
        self.assertEqual(snapshot.retained_decisions, 0)
        self.assertEqual(snapshot.retained_payload_bytes, 0)

        assembler(_segment(1, (batch,), terminal_keys=(batch.lineages[0].key,)))
        self.assertEqual(
            sink.deliveries[0].dropped[0].reason,
            AttemptDropReason.PAYLOAD_BYTE_LIMIT,
        )

    def test_all_terminals_from_one_segment_share_one_sink_delivery(self) -> None:
        sink = RecordingAttemptSink()
        assembler = BoundedAttemptAssembler(
            AttemptAssemblyLimits(
                max_open_attempts=2,
                max_decisions_per_attempt=8,
                max_payload_bytes_per_attempt=8 * 1024 * 1024,
            ),
            sink,
        )
        batch = _experience_batch()

        assembler(
            _segment(
                0,
                (batch,),
                terminal_keys=tuple(lineage.key for lineage in batch.lineages),
            )
        )

        self.assertEqual(len(sink.deliveries), 1)
        self.assertEqual(len(sink.deliveries[0].completed), 2)
        self.assertEqual(assembler.snapshot.completed_attempts, 2)

    def test_sink_failure_commits_neither_sequence_nor_completion(self) -> None:
        sink = RecordingAttemptSink()
        sink.fail = True
        assembler = BoundedAttemptAssembler(
            AttemptAssemblyLimits(
                max_open_attempts=1,
                max_decisions_per_attempt=8,
                max_payload_bytes_per_attempt=8 * 1024 * 1024,
            ),
            sink,
        )
        batch = _experience_batch().select_rows([0])
        segment = _segment(0, (batch,), terminal_keys=(batch.lineages[0].key,))

        with self.assertRaisesRegex(RuntimeError, "sink failed"):
            assembler(segment)

        self.assertEqual(assembler.snapshot.next_sequence_index, 0)
        self.assertEqual(assembler.snapshot.completed_attempts, 0)
        sink.fail = False
        assembler(segment)
        self.assertEqual(assembler.snapshot.next_sequence_index, 1)
        self.assertEqual(assembler.snapshot.completed_attempts, 1)

        resumed = BoundedAttemptAssembler(
            assembler.limits,
            sink,
            resume_snapshot=assembler.snapshot,
        )
        self.assertEqual(resumed.snapshot, assembler.snapshot)

    def test_sequence_and_open_attempt_limits_fail_before_mutation(self) -> None:
        sink = RecordingAttemptSink()
        assembler = BoundedAttemptAssembler(
            AttemptAssemblyLimits(
                max_open_attempts=1,
                max_decisions_per_attempt=8,
                max_payload_bytes_per_attempt=8 * 1024 * 1024,
            ),
            sink,
        )

        with self.assertRaisesRegex(AttemptAssemblyError, "sequence"):
            assembler(_segment(1, (_experience_batch().select_rows([0]),)))
        with self.assertRaisesRegex(AttemptAssemblyError, "max_open_attempts"):
            assembler(_segment(0, (_experience_batch(),)))

        self.assertEqual(assembler.snapshot.next_sequence_index, 0)
        self.assertEqual(assembler.snapshot.open_attempts, 0)

    def test_terminal_and_replacement_generation_share_one_bounded_segment(self) -> None:
        sink = RecordingAttemptSink()
        assembler = BoundedAttemptAssembler(
            AttemptAssemblyLimits(
                max_open_attempts=1,
                max_decisions_per_attempt=8,
                max_payload_bytes_per_attempt=8 * 1024 * 1024,
            ),
            sink,
        )
        old_first = _experience_batch().select_rows([0])
        old_final = _experience_batch().select_rows([0])
        replacement = _experience_batch(generations=(1, 0)).select_rows([0])

        assembler(_segment(0, (old_first,)))
        assembler(
            _segment(
                1,
                (old_final, replacement),
                terminal_keys=(old_first.lineages[0].key,),
            )
        )

        self.assertEqual(assembler.snapshot.open_attempts, 1)
        self.assertEqual(assembler.snapshot.completed_attempts, 1)
        self.assertEqual(sink.deliveries[0].completed[0].decision_count, 2)


if __name__ == "__main__":
    unittest.main()
