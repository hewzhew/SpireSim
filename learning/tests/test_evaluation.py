from __future__ import annotations

import unittest
from collections.abc import Mapping, Sequence
from dataclasses import replace

from learning.tests.driver_fixtures import (
    FakeBatchEnv,
    NumpyWinningBatchEnv,
    RecordingPolicy,
)
from learning.tests.policy_fixtures import BEHAVIOR_MANIFEST_ID
from sts_learning import (
    BatchPolicyChoice,
    BehaviorManifestId,
    AttemptAssemblyLimits,
    ExperienceLimits,
    HeldOutEvaluationDelta,
    HeldOutEvaluationError,
    HeldOutEvaluationSpec,
    PairedHeldOutEvaluationResult,
    PairedHeldOutEvaluationSpec,
    SeedPartition,
    SeedSchedule,
    evaluate_held_out_behavior,
    evaluate_held_out_behavior_with_public_trajectories,
    evaluate_paired_held_out_behaviors,
)


class FixedOrdinalPolicy:
    def __init__(self, manifest_id: BehaviorManifestId, ordinal: int) -> None:
        self.behavior_manifest_id = manifest_id
        self.ordinal = ordinal

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        slots = decision_batch["slot_indices"]
        assert isinstance(slots, Sequence)
        return BatchPolicyChoice.deterministic(
            [self.ordinal] * len(slots),
            self.behavior_manifest_id,
        )


class NumpyFirstPolicy:
    behavior_manifest_id = BEHAVIOR_MANIFEST_ID

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        return BatchPolicyChoice.deterministic(
            [0] * len(decision_batch["slot_indices"]),  # type: ignore[arg-type]
            self.behavior_manifest_id,
        )


class OrdinalOutcomeEnv(FakeBatchEnv):
    def step(self) -> dict[str, object]:
        ordinal = self.choose_calls[-1][0]
        self._terminal_plans.insert(0, {0: 1 if ordinal == 1 else -1})
        return super().step()


class OnlyOrdinalOneTerminatesEnv(FakeBatchEnv):
    def step(self) -> dict[str, object]:
        ordinal = self.choose_calls[-1][0]
        self._terminal_plans.insert(0, {0: 1} if ordinal == 1 else {})
        return super().step()


class HeldOutEvaluationTests(unittest.TestCase):
    def test_trajectory_evaluation_retains_only_complete_public_attempts(self) -> None:
        result = evaluate_held_out_behavior_with_public_trajectories(
            NumpyWinningBatchEnv,
            NumpyFirstPolicy(),
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            spec=HeldOutEvaluationSpec(
                slot_count=1,
                terminal_attempt_target=2,
                max_batch_steps=2,
            ),
            experience_limits=ExperienceLimits(
                max_decisions=16,
                max_payload_bytes=1024 * 1024,
            ),
            attempt_limits=AttemptAssemblyLimits(
                max_open_attempts=1,
                max_decisions_per_attempt=16,
                max_payload_bytes_per_attempt=1024 * 1024,
            ),
        )

        self.assertTrue(result.evaluation.complete)
        self.assertEqual(len(result.trajectories), 2)
        self.assertEqual(
            tuple(row.terminal.terminal_reward for row in result.trajectories),
            (1, 1),
        )
        self.assertTrue(
            all(
                decision.public_snapshot.is_combat
                for trajectory in result.trajectories
                for decision in trajectory.decisions
            )
        )

    def test_trajectory_evaluation_retains_atomic_multi_slot_overshoot(self) -> None:
        result = evaluate_held_out_behavior_with_public_trajectories(
            NumpyWinningBatchEnv,
            NumpyFirstPolicy(),
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            spec=HeldOutEvaluationSpec(
                slot_count=2,
                terminal_attempt_target=1,
                max_batch_steps=1,
            ),
            experience_limits=ExperienceLimits(
                max_decisions=16,
                max_payload_bytes=1024 * 1024,
            ),
            attempt_limits=AttemptAssemblyLimits(
                max_open_attempts=2,
                max_decisions_per_attempt=16,
                max_payload_bytes_per_attempt=1024 * 1024,
            ),
        )

        self.assertEqual(result.evaluation.run.summary.terminal_attempts, 2)
        self.assertEqual(len(result.trajectories), 2)

    def test_evaluation_reports_atomic_multi_terminal_outcomes(self) -> None:
        created: list[FakeBatchEnv] = []

        def factory(seeds: list[int]) -> FakeBatchEnv:
            env = FakeBatchEnv(
                seeds,
                terminal_plans=({0: 1, 1: -1}, {0: 1}),
            )
            created.append(env)
            return env

        schedule = SeedSchedule(SeedPartition.HELD_OUT)
        result = evaluate_held_out_behavior(
            factory,
            RecordingPolicy(),
            schedule=schedule,
            spec=HeldOutEvaluationSpec(
                slot_count=2,
                terminal_attempt_target=1,
                max_batch_steps=5,
            ),
        )

        self.assertTrue(result.complete)
        self.assertFalse(result.step_limit_reached)
        self.assertEqual(result.run.summary.batch_steps, 1)
        self.assertEqual(result.run.summary.terminal_attempts, 2)
        self.assertEqual(result.run.summary.victories, 1)
        self.assertEqual(result.run.summary.defeats, 1)
        self.assertEqual(result.run.summary.terminal_progress.floor_sum, 80)
        self.assertEqual(result.run.summary.terminal_progress.mean_floor, 40.0)
        self.assertEqual(
            result.run.summary.terminal_progress.floor_counts,
            ((40, 2),),
        )
        self.assertEqual(result.run.summary.terminal_progress.act_counts, ((3, 2),))
        self.assertEqual(result.run.summary.recoveries, 0)
        self.assertEqual(result.behavior_manifest_id, BEHAVIOR_MANIFEST_ID)
        self.assertEqual(created[0].restore_calls, [])
        self.assertEqual(result.schedule_start, schedule)
        self.assertGreater(result.schedule_end.next_candidate, schedule.next_candidate)

    def test_paired_evaluation_reports_only_typed_arithmetic_delta(self) -> None:
        left_manifest = BehaviorManifestId(b"\x81" * 32)
        right_manifest = BehaviorManifestId(b"\x82" * 32)
        schedule = SeedSchedule(SeedPartition.HELD_OUT, next_candidate=31)
        result = evaluate_paired_held_out_behaviors(
            OrdinalOutcomeEnv,
            FixedOrdinalPolicy(left_manifest, 0),
            FixedOrdinalPolicy(right_manifest, 1),
            spec=PairedHeldOutEvaluationSpec(
                schedule=schedule,
                evaluation=HeldOutEvaluationSpec(
                    slot_count=1,
                    terminal_attempt_target=1,
                    max_batch_steps=1,
                ),
            ),
        )

        self.assertTrue(result.comparable)
        self.assertEqual(result.left.behavior_manifest_id, left_manifest)
        self.assertEqual(result.right.behavior_manifest_id, right_manifest)
        self.assertEqual(result.left.schedule_start, schedule)
        self.assertEqual(result.right.schedule_start, schedule)
        self.assertTrue(result.left.complete)
        self.assertTrue(result.right.complete)
        self.assertEqual(result.left.run.summary.victories, 0)
        self.assertEqual(result.left.run.summary.defeats, 1)
        self.assertEqual(result.right.run.summary.victories, 1)
        self.assertEqual(result.right.run.summary.defeats, 0)
        self.assertEqual(
            result.right_minus_left,
            HeldOutEvaluationDelta(
                terminal_attempts=0,
                victories=1,
                defeats=-1,
                terminal_floor_sum=0,
                batch_steps=0,
            ),
        )

    def test_paired_evaluation_marks_asymmetric_budget_exhaustion_incomparable(
        self,
    ) -> None:
        result = evaluate_paired_held_out_behaviors(
            OnlyOrdinalOneTerminatesEnv,
            FixedOrdinalPolicy(BehaviorManifestId(b"\x83" * 32), 0),
            FixedOrdinalPolicy(BehaviorManifestId(b"\x84" * 32), 1),
            spec=PairedHeldOutEvaluationSpec(
                schedule=SeedSchedule(SeedPartition.HELD_OUT),
                evaluation=HeldOutEvaluationSpec(
                    slot_count=1,
                    terminal_attempt_target=1,
                    max_batch_steps=1,
                ),
            ),
        )

        self.assertFalse(result.comparable)
        self.assertFalse(result.left.complete)
        self.assertTrue(result.left.step_limit_reached)
        self.assertTrue(result.right.complete)
        self.assertFalse(result.right.step_limit_reached)

    def test_paired_evaluation_rejects_duplicate_identity_before_environment(
        self,
    ) -> None:
        factory_calls = 0

        def factory(seeds: list[int]) -> FakeBatchEnv:
            nonlocal factory_calls
            factory_calls += 1
            return FakeBatchEnv(seeds)

        manifest_id = BehaviorManifestId(b"\x85" * 32)
        with self.assertRaisesRegex(HeldOutEvaluationError, "distinct behavior"):
            evaluate_paired_held_out_behaviors(
                factory,
                FixedOrdinalPolicy(manifest_id, 0),
                FixedOrdinalPolicy(manifest_id, 1),
                spec=PairedHeldOutEvaluationSpec(
                    schedule=SeedSchedule(SeedPartition.HELD_OUT),
                    evaluation=HeldOutEvaluationSpec(
                        slot_count=1,
                        terminal_attempt_target=1,
                        max_batch_steps=1,
                    ),
                ),
            )

        self.assertEqual(factory_calls, 0)

    def test_paired_evaluation_keeps_manifest_lock_on_each_side(self) -> None:
        left_manifest = BehaviorManifestId(b"\x86" * 32)
        right_manifest = BehaviorManifestId(b"\x87" * 32)
        mixed_manifest = BehaviorManifestId(b"\x88" * 32)

        class SwitchingPolicy:
            behavior_manifest_id = right_manifest

            def __init__(self) -> None:
                self.calls = 0

            def choose(self, decision_batch):
                self.calls += 1
                manifest_id = right_manifest if self.calls == 1 else mixed_manifest
                return BatchPolicyChoice.deterministic(
                    [1] * len(decision_batch["slot_indices"]),
                    manifest_id,
                )

        created: list[OrdinalOutcomeEnv] = []

        def factory(seeds: list[int]) -> OrdinalOutcomeEnv:
            env = OrdinalOutcomeEnv(seeds)
            created.append(env)
            return env

        with self.assertRaisesRegex(HeldOutEvaluationError, "changed behavior"):
            evaluate_paired_held_out_behaviors(
                factory,
                FixedOrdinalPolicy(left_manifest, 0),
                SwitchingPolicy(),
                spec=PairedHeldOutEvaluationSpec(
                    schedule=SeedSchedule(SeedPartition.HELD_OUT),
                    evaluation=HeldOutEvaluationSpec(
                        slot_count=1,
                        terminal_attempt_target=1,
                        max_batch_steps=1,
                    ),
                ),
            )

        self.assertEqual(len(created), 2)
        self.assertEqual(len(created[1].choose_calls), 1)
        self.assertEqual(created[1].reset_calls, [])

    def test_paired_result_rejects_schedule_or_run_spec_mismatch(self) -> None:
        result = evaluate_paired_held_out_behaviors(
            OrdinalOutcomeEnv,
            FixedOrdinalPolicy(BehaviorManifestId(b"\x89" * 32), 0),
            FixedOrdinalPolicy(BehaviorManifestId(b"\x8a" * 32), 1),
            spec=PairedHeldOutEvaluationSpec(
                schedule=SeedSchedule(SeedPartition.HELD_OUT),
                evaluation=HeldOutEvaluationSpec(
                    slot_count=1,
                    terminal_attempt_target=1,
                    max_batch_steps=1,
                ),
            ),
        )

        with self.assertRaisesRegex(HeldOutEvaluationError, "seed schedules"):
            PairedHeldOutEvaluationResult(
                left=result.left,
                right=replace(
                    result.right,
                    schedule_start=SeedSchedule(
                        SeedPartition.HELD_OUT,
                        next_candidate=3,
                    ),
                ),
            )
        with self.assertRaisesRegex(HeldOutEvaluationError, "terminal targets"):
            PairedHeldOutEvaluationResult(
                left=result.left,
                right=replace(
                    result.right,
                    run=replace(
                        result.right.run,
                        terminal_attempt_target=2,
                    ),
                ),
            )
        with self.assertRaisesRegex(HeldOutEvaluationError, "slot counts"):
            PairedHeldOutEvaluationResult(
                left=result.left,
                right=replace(
                    result.right,
                    run=replace(
                        result.right.run,
                        summary=replace(
                            result.right.run.summary,
                            active_slots=2,
                        ),
                    ),
                ),
            )

    def test_same_schedule_repeats_the_same_seed_and_outcome_prefix(self) -> None:
        created_seeds: list[tuple[int, ...]] = []

        def factory(seeds: list[int]) -> FakeBatchEnv:
            created_seeds.append(tuple(seeds))
            return FakeBatchEnv(seeds, terminal_plans=({0: -1},))

        schedule = SeedSchedule(SeedPartition.HELD_OUT, next_candidate=17)
        spec = HeldOutEvaluationSpec(
            slot_count=1,
            terminal_attempt_target=1,
            max_batch_steps=3,
        )
        first = evaluate_held_out_behavior(
            factory,
            RecordingPolicy(),
            schedule=schedule,
            spec=spec,
        )
        second = evaluate_held_out_behavior(
            factory,
            RecordingPolicy(),
            schedule=schedule,
            spec=spec,
        )

        self.assertEqual(created_seeds[0], created_seeds[1])
        self.assertEqual(first.schedule_end, second.schedule_end)
        self.assertEqual(first.run.summary.batch_steps, second.run.summary.batch_steps)
        self.assertEqual(first.run.summary.victories, second.run.summary.victories)
        self.assertEqual(first.run.summary.defeats, second.run.summary.defeats)

    def test_budget_exhaustion_and_training_schedule_are_explicit(self) -> None:
        exhausted = evaluate_held_out_behavior(
            FakeBatchEnv,
            RecordingPolicy(),
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            spec=HeldOutEvaluationSpec(
                slot_count=1,
                terminal_attempt_target=1,
                max_batch_steps=0,
            ),
        )

        self.assertFalse(exhausted.complete)
        self.assertTrue(exhausted.step_limit_reached)
        self.assertEqual(exhausted.run.summary.terminal_attempts, 0)
        with self.assertRaisesRegex(HeldOutEvaluationError, "held-out"):
            evaluate_held_out_behavior(
                FakeBatchEnv,
                RecordingPolicy(),
                schedule=SeedSchedule(SeedPartition.TRAINING),
                spec=HeldOutEvaluationSpec(
                    slot_count=1,
                    terminal_attempt_target=1,
                    max_batch_steps=1,
                ),
            )

    def test_policy_cannot_mix_behavior_manifests_inside_one_evaluation(self) -> None:
        alternative = BehaviorManifestId(b"\x91" * 32)

        class SwitchingPolicy:
            behavior_manifest_id = BEHAVIOR_MANIFEST_ID

            def __init__(self) -> None:
                self.calls = 0

            def choose(self, decision_batch):
                self.calls += 1
                manifest_id = (
                    self.behavior_manifest_id if self.calls == 1 else alternative
                )
                return BatchPolicyChoice.deterministic(
                    [0] * len(decision_batch["slot_indices"]),
                    manifest_id,
                )

        with self.assertRaisesRegex(HeldOutEvaluationError, "changed behavior"):
            evaluate_held_out_behavior(
                FakeBatchEnv,
                SwitchingPolicy(),
                schedule=SeedSchedule(SeedPartition.HELD_OUT),
                spec=HeldOutEvaluationSpec(
                    slot_count=1,
                    terminal_attempt_target=1,
                    max_batch_steps=1,
                ),
            )


if __name__ == "__main__":
    unittest.main()
