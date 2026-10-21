from __future__ import annotations

import unittest
from dataclasses import replace

import numpy as np

from learning.tests.policy_fixtures import BEHAVIOR_MANIFEST_ID
from learning.tests.semantic_fixtures import semantic_batch_fixture
from sts_learning import (
    DETERMINISTIC_SELECTION,
    DecisionRunProgress,
    DecisionExperienceBatch,
    ExperienceError,
    ExperienceLimits,
    ExperienceSegmentBuffer,
    PreparedDecisionBatch,
    PublicDecisionSnapshot,
    RecoverySlotSnapshot,
    RecoverySlotStatus,
    SegmentCloseReason,
    SelectionProbability,
    TerminalAttemptOutcome,
    TerminalAttemptRecord,
    iter_payload_arrays,
)


def deterministic(count: int) -> tuple[SelectionProbability, ...]:
    return (DETERMINISTIC_SELECTION,) * count


def decision_batch(
    slots: tuple[int, ...] = (0, 1),
    candidate_counts: tuple[int, ...] = (2, 3),
) -> dict[str, object]:
    candidate_splits = [0]
    for count in candidate_counts:
        candidate_splits.append(candidate_splits[-1] + count)
    token_splits = np.arange(len(slots) + 1, dtype=np.uint64) * 2
    return {
        "slot_indices": np.array(slots, dtype=np.uint64),
        "phase": np.zeros(len(slots), dtype=np.uint8),
        "candidate_counts": np.array(candidate_counts, dtype=np.uint64),
        "candidate_row_splits": np.array(candidate_splits, dtype=np.uint64),
        "semantic": {
            "schema_version": 2,
            "completeness": np.ones(len(slots), dtype=np.uint8),
            "token": {
                "row_splits": token_splits,
                "kind": np.arange(len(slots) * 2, dtype=np.uint16),
            },
            "candidate_token_indices": np.arange(
                sum(candidate_counts),
                dtype=np.uint64,
            ),
        },
    }


def snapshot(
    slot: int,
    *,
    seed: int | None = None,
    generation: int = 0,
    attempt: int = 1,
    recoveries: int = 0,
) -> RecoverySlotSnapshot:
    return RecoverySlotSnapshot(
        slot_index=slot,
        episode_seed=100 + slot if seed is None else seed,
        episode_generation=generation,
        attempt_index=attempt,
        recoveries_used=recoveries,
        status=RecoverySlotStatus.ACTIVE,
        pending_terminal=None,
    )


def terminal(
    slot: int,
    *,
    seed: int | None = None,
    generation: int = 0,
    attempt: int = 1,
    recoveries: int = 0,
    reward: int = 1,
) -> TerminalAttemptRecord:
    return TerminalAttemptRecord(
        episode_seed=100 + slot if seed is None else seed,
        episode_generation=generation,
        attempt_index=attempt,
        recoveries_used=recoveries,
        terminal=TerminalAttemptOutcome(
            slot_index=slot,
            terminal_reward=reward,
            terminal_act=3,
            terminal_floor=40,
            terminal_hp=20 if reward == 1 else 0,
            terminal_max_hp=80,
            terminal_gold=50,
        ),
    )


def public_snapshot(
    *,
    phase: int,
    is_combat: bool,
    candidate_count: int,
) -> PublicDecisionSnapshot:
    return PublicDecisionSnapshot(
        phase=phase,
        is_combat=is_combat,
        snapshot_id=f"snapshot-{phase}-{candidate_count}",
        observation_id=f"observation-{phase}-{candidate_count}",
        history_snapshot_id=f"history-{phase}-{candidate_count}",
        candidate_surface_id=f"surface-{phase}-{candidate_count}",
        candidate_ids=tuple(
            f"candidate-{phase}-{ordinal}" for ordinal in range(candidate_count)
        ),
    )


class ExperienceSegmentTests(unittest.TestCase):
    def test_capture_copies_only_read_only_array_payload_with_exact_lineage(self) -> None:
        source = decision_batch()
        source_arrays = tuple(iter_payload_arrays(source))
        array_bytes = sum(array.nbytes for array in source_arrays)

        prepared = PreparedDecisionBatch.capture(
            source,
            [snapshot(0), snapshot(1)],
        )
        frozen_arrays = tuple(iter_payload_arrays(prepared.payload))
        source["slot_indices"][0] = 9  # type: ignore[index]
        source["semantic"]["token"]["kind"][0] = 99  # type: ignore[index]

        self.assertEqual(prepared.decision_count, 2)
        self.assertGreater(prepared.payload_bytes, array_bytes)
        self.assertEqual(prepared.lineages[0].key.episode_seed, 100)
        self.assertEqual(prepared.lineages[1].key.slot_index, 1)
        self.assertEqual(int(prepared.payload["slot_indices"][0]), 0)  # type: ignore[index]
        self.assertEqual(
            int(prepared.payload["semantic"]["token"]["kind"][0]),  # type: ignore[index]
            0,
        )
        self.assertTrue(frozen_arrays)
        self.assertTrue(all(not array.flags.writeable for array in frozen_arrays))
        with self.assertRaises(ValueError):
            prepared.payload["slot_indices"][0] = 7  # type: ignore[index]

    def test_capture_aligns_typed_run_progress_without_parsing_semantics(self) -> None:
        prepared = PreparedDecisionBatch.capture(
            semantic_batch_fixture(),
            [snapshot(4), snapshot(9)],
            [
                DecisionRunProgress(
                    episode_seed=104,
                    act=1,
                    floor=3,
                    is_combat=False,
                    strategic_context_kind=1,
                ),
                DecisionRunProgress(
                    episode_seed=109,
                    act=2,
                    floor=19,
                    is_combat=True,
                    strategic_context_kind=None,
                ),
            ],
        )
        batch = DecisionExperienceBatch.from_prepared(
            prepared,
            [0, 1],
            deterministic(2),
            BEHAVIOR_MANIFEST_ID,
        )

        assert batch.run_progress == prepared.run_progress
        selected = batch.select_rows([1])
        assert selected.run_progress == (
            DecisionRunProgress(
                episode_seed=109,
                act=2,
                floor=19,
                is_combat=True,
                strategic_context_kind=None,
            ),
        )

        with self.assertRaisesRegex(ExperienceError, "progress seed"):
            PreparedDecisionBatch.capture(
                decision_batch((0,), (1,)),
                [snapshot(0)],
                [
                    DecisionRunProgress(
                        episode_seed=999,
                        act=1,
                        floor=3,
                        is_combat=False,
                        strategic_context_kind=1,
                    )
                ],
            )

    def test_capture_aligns_public_snapshot_phase_and_candidate_surface(self) -> None:
        first = DecisionRunProgress(
            episode_seed=104,
            act=1,
            floor=3,
            is_combat=True,
            strategic_context_kind=None,
            public_snapshot=public_snapshot(
                phase=1,
                is_combat=True,
                candidate_count=2,
            ),
        )
        second = DecisionRunProgress(
            episode_seed=109,
            act=1,
            floor=4,
            is_combat=False,
            strategic_context_kind=2,
            public_snapshot=public_snapshot(
                phase=2,
                is_combat=False,
                candidate_count=3,
            ),
        )

        prepared = PreparedDecisionBatch.capture(
            semantic_batch_fixture(),
            [snapshot(4), snapshot(9)],
            [first, second],
        )
        self.assertEqual(prepared.run_progress, (first, second))

        with self.assertRaisesRegex(ExperienceError, "phase"):
            PreparedDecisionBatch.capture(
                semantic_batch_fixture(),
                [snapshot(4), snapshot(9)],
                [
                    replace(
                        first,
                        public_snapshot=replace(first.public_snapshot, phase=0),
                    ),
                    second,
                ],
            )
        with self.assertRaisesRegex(ExperienceError, "candidates"):
            PreparedDecisionBatch.capture(
                semantic_batch_fixture(),
                [snapshot(4), snapshot(9)],
                [
                    replace(
                        first,
                        public_snapshot=replace(
                            first.public_snapshot,
                            candidate_ids=("candidate-only",),
                        ),
                    ),
                    second,
                ],
            )

    def test_capture_rejects_misaligned_lineage_and_object_arrays(self) -> None:
        with self.assertRaisesRegex(ExperienceError, "snapshots"):
            PreparedDecisionBatch.capture(decision_batch(), [snapshot(0)])
        with self.assertRaisesRegex(ExperienceError, "snapshot slot 1"):
            PreparedDecisionBatch.capture(
                decision_batch(),
                [snapshot(1), snapshot(0)],
            )
        invalid = decision_batch((0,), (1,))
        invalid["semantic"] = {"bad": np.array([object()], dtype=object)}
        with self.assertRaisesRegex(ExperienceError, "object array"):
            PreparedDecisionBatch.capture(invalid, [snapshot(0)])
        with self.assertRaisesRegex(ExperienceError, "duplicate slots"):
            PreparedDecisionBatch.capture(
                decision_batch((0, 0), (1, 1)),
                [snapshot(0), snapshot(0)],
            )
        inactive = replace(
            snapshot(0),
            status=RecoverySlotStatus.VICTORY_COMPLETE,
        )
        with self.assertRaisesRegex(ExperienceError, "active"):
            PreparedDecisionBatch.capture(
                decision_batch((0,), (1,)),
                [inactive],
            )

    def test_decision_limit_rotates_before_adding_the_next_batch(self) -> None:
        first = PreparedDecisionBatch.capture(
            decision_batch(),
            [snapshot(0), snapshot(1)],
        )
        second = PreparedDecisionBatch.capture(
            decision_batch((0,), (2,)),
            [snapshot(0)],
        )
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=2,
                max_payload_bytes=first.payload_bytes + second.payload_bytes,
            )
        )

        self.assertEqual(
            buffer.record(first, [0, 1], deterministic(2), BEHAVIOR_MANIFEST_ID),
            (),
        )
        emitted = buffer.record(second, [1], deterministic(1), BEHAVIOR_MANIFEST_ID)

        self.assertEqual(len(emitted), 1)
        self.assertEqual(emitted[0].close_reason, SegmentCloseReason.DECISION_LIMIT)
        self.assertEqual(emitted[0].decision_count, 2)
        self.assertTrue(emitted[0].censored)
        self.assertEqual(
            emitted[0].batches[0].behavior_manifest_id,
            BEHAVIOR_MANIFEST_ID,
        )
        self.assertEqual(buffer.decision_count, 1)
        self.assertEqual(buffer.payload_bytes, second.payload_bytes)

    def test_payload_limit_and_oversized_single_batch_are_explicit(self) -> None:
        prepared = PreparedDecisionBatch.capture(
            decision_batch((0,), (2,)),
            [snapshot(0)],
        )
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=10,
                max_payload_bytes=prepared.payload_bytes,
            )
        )
        buffer.record(prepared, [0], deterministic(1), BEHAVIOR_MANIFEST_ID)

        emitted = buffer.record(
            prepared,
            [1],
            deterministic(1),
            BEHAVIOR_MANIFEST_ID,
        )

        self.assertEqual(
            emitted[0].close_reason,
            SegmentCloseReason.PAYLOAD_BYTE_LIMIT,
        )
        self.assertLessEqual(
            emitted[0].payload_bytes,
            buffer.limits.max_payload_bytes,
        )
        too_small = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=10,
                max_payload_bytes=prepared.payload_bytes - 1,
            )
        )
        with self.assertRaisesRegex(ExperienceError, "one batch"):
            too_small.prepare(decision_batch((0,), (2,)), [snapshot(0)])
        self.assertTrue(too_small.empty)

    def test_scalar_mapping_metadata_counts_toward_the_byte_limit(self) -> None:
        payload = decision_batch((0,), (1,))
        payload["semantic"] = {
            "schema_version": 2,
            **{f"field_{index}": index for index in range(100)},
        }
        prepared = PreparedDecisionBatch.capture(payload, [snapshot(0)])
        array_bytes = sum(
            array.nbytes for array in iter_payload_arrays(prepared.payload)
        )

        self.assertGreater(prepared.payload_bytes, array_bytes + 1_000)
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=1,
                max_payload_bytes=prepared.payload_bytes - 1,
            )
        )
        with self.assertRaisesRegex(ExperienceError, "one batch"):
            buffer.prepare(payload, [snapshot(0)])

    def test_probability_evidence_is_typed_aligned_and_preserved_by_selection(
        self,
    ) -> None:
        prepared = PreparedDecisionBatch.capture(
            semantic_batch_fixture(),
            [snapshot(4), snapshot(9)],
        )
        batch = DecisionExperienceBatch.from_prepared(
            prepared,
            [0, 1],
            (SelectionProbability.known(0.25), SelectionProbability.unknown()),
            BEHAVIOR_MANIFEST_ID,
        )

        selected = batch.select_rows([1, 0])

        self.assertEqual(
            tuple(probability.value for probability in selected.selection_probabilities),
            (None, 0.25),
        )
        with self.assertRaisesRegex(ExperienceError, "sequence"):
            DecisionExperienceBatch.from_prepared(
                prepared,
                [0, 1],
                None,  # type: ignore[arg-type]
                BEHAVIOR_MANIFEST_ID,
            )
        with self.assertRaisesRegex(ExperienceError, "one value per decision"):
            DecisionExperienceBatch.from_prepared(
                prepared,
                [0, 1],
                (DETERMINISTIC_SELECTION,),
                BEHAVIOR_MANIFEST_ID,
            )
        with self.assertRaisesRegex(ExperienceError, "typed"):
            DecisionExperienceBatch.from_prepared(
                prepared,
                [0, 1],
                (1.0, 1.0),  # type: ignore[arg-type]
                BEHAVIOR_MANIFEST_ID,
            )
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=2,
                max_payload_bytes=prepared.payload_bytes,
            )
        )
        with self.assertRaisesRegex(ExperienceError, "must be typed"):
            buffer.rotate_before(
                replace(
                    batch,
                    selection_probabilities=(1.0, 1.0),  # type: ignore[arg-type]
                )
            )

    def test_flush_marks_only_unfinished_attempts_censored(self) -> None:
        prepared = PreparedDecisionBatch.capture(
            decision_batch(),
            [snapshot(0), snapshot(1)],
        )
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=4,
                max_payload_bytes=prepared.payload_bytes * 2,
            )
        )
        buffer.record(
            prepared,
            [0, 1],
            deterministic(2),
            BEHAVIOR_MANIFEST_ID,
        )
        first_terminal = terminal(0)
        buffer.record_terminals([first_terminal])

        segment = buffer.flush()

        assert segment is not None
        self.assertEqual(segment.close_reason, SegmentCloseReason.EXPLICIT_FLUSH)
        by_slot = {
            fragment.lineage.key.slot_index: fragment
            for fragment in segment.attempts
        }
        self.assertEqual(by_slot[0].terminal, first_terminal)
        self.assertFalse(by_slot[0].censored)
        self.assertIsNone(by_slot[1].terminal)
        self.assertTrue(by_slot[1].censored)
        self.assertTrue(segment.censored)
        self.assertTrue(buffer.empty)

    def test_recovery_attempts_never_share_one_lineage(self) -> None:
        first = PreparedDecisionBatch.capture(
            decision_batch((0,), (2,)),
            [snapshot(0, attempt=1, recoveries=0)],
        )
        second = PreparedDecisionBatch.capture(
            decision_batch((0,), (2,)),
            [snapshot(0, attempt=2, recoveries=1)],
        )
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=4,
                max_payload_bytes=first.payload_bytes + second.payload_bytes,
            )
        )
        buffer.record(first, [0], deterministic(1), BEHAVIOR_MANIFEST_ID)
        first_terminal = terminal(0, attempt=1, recoveries=0, reward=-1)
        buffer.record_terminals([first_terminal])
        buffer.record(second, [1], deterministic(1), BEHAVIOR_MANIFEST_ID)

        segment = buffer.flush()

        assert segment is not None
        self.assertEqual(len(segment.attempts), 2)
        by_attempt = {
            fragment.lineage.key.attempt_index: fragment
            for fragment in segment.attempts
        }
        self.assertEqual(by_attempt[1].terminal, first_terminal)
        self.assertFalse(by_attempt[1].censored)
        self.assertTrue(by_attempt[2].censored)

    def test_terminal_after_only_routine_steps_retains_no_fabricated_decision(self) -> None:
        from sts_learning.experience import DecisionLineage
        buffer = ExperienceSegmentBuffer(ExperienceLimits(max_decisions=2, max_payload_bytes=4096))
        lineage = DecisionLineage.from_snapshot(snapshot(0))
        buffer.record_terminals([terminal(0)], continuation_lineages=[lineage])
        self.assertFalse(buffer.empty)
        segment = buffer.flush()
        self.assertEqual(segment.batches, ())
        self.assertEqual(segment.decision_count, 0)
        self.assertEqual(segment.payload_bytes, 0)
        self.assertEqual(segment.attempts[0].lineage, lineage)
        self.assertEqual(segment.attempts[0].terminal, terminal(0))
        self.assertTrue(buffer.empty)

    def test_terminal_validation_is_atomic(self) -> None:
        prepared = PreparedDecisionBatch.capture(
            decision_batch((0,), (2,)),
            [snapshot(0)],
        )
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=2,
                max_payload_bytes=prepared.payload_bytes * 2,
            )
        )
        buffer.record(prepared, [0], deterministic(1), BEHAVIOR_MANIFEST_ID)
        valid = terminal(0)

        with self.assertRaisesRegex(ExperienceError, "absent"):
            buffer.record_terminals([valid, terminal(1)])
        buffer.record_terminals([valid])
        with self.assertRaisesRegex(ExperienceError, "already"):
            buffer.record_terminals([valid])

    def test_empty_resume_preserves_next_segment_identity(self) -> None:
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(max_decisions=2, max_payload_bytes=1024),
            next_sequence_index=7,
        )
        self.assertTrue(buffer.empty)
        self.assertEqual(buffer.next_sequence_index, 7)


if __name__ == "__main__":
    unittest.main()
