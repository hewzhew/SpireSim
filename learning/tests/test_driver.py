from __future__ import annotations

import unittest

from learning.tests.driver_fixtures import (
    ArrayFirstPolicy,
    FakeBatchEnv,
    FirstAttemptRecovery,
    InvalidPolicy,
    MisalignedProbabilityPolicy,
    NoRecovery,
    NumpyFakeBatchEnv,
    OneRejectedChoiceEnv,
    RecordingPolicy,
    UntypedPolicy,
    UntypedProbabilityPolicy,
)
from sts_learning import (
    BatchDriverError,
    ExperienceLimits,
    ExperienceSegment,
    ExperienceSegmentBuffer,
    OnlineBatchDriver,
    SeedPartition,
    SeedPartitionSpec,
    SeedSchedule,
    initialize_population,
)


class BatchDriverTests(unittest.TestCase):
    def test_initial_population_uses_one_seed_plan_for_every_owner(self) -> None:
        schedule = SeedSchedule(
            SeedPartition.TRAINING,
            SeedPartitionSpec(held_out_numerator=1, denominator=4),
        )
        created: list[FakeBatchEnv] = []

        def factory(seeds: list[int]) -> FakeBatchEnv:
            env = FakeBatchEnv(seeds)
            created.append(env)
            return env

        population = initialize_population(
            factory,
            slot_count=3,
            schedule=schedule,
            max_recoveries_per_episode=2,
        )

        self.assertEqual(len(created), 1)
        env = created[0]
        self.assertEqual(
            tuple(env.seeds),
            tuple(snapshot.episode_seed for snapshot in population.ledger.snapshots()),
        )
        self.assertEqual(len(population.checkpoint_bank), 3)
        self.assertGreater(population.schedule.next_candidate, schedule.next_candidate)

    def test_driver_batches_policy_and_resolves_recovery_completion_and_reset(self) -> None:
        envs: list[FakeBatchEnv] = []

        def factory(seeds: list[int]) -> FakeBatchEnv:
            env = FakeBatchEnv(
                seeds,
                terminal_plans=({0: -1, 1: 1}, {0: -1}),
            )
            envs.append(env)
            return env

        population = initialize_population(
            factory,
            slot_count=2,
            schedule=SeedSchedule(SeedPartition.TRAINING),
            max_recoveries_per_episode=1,
        )
        initial_seeds = tuple(envs[0].seeds)
        policy = RecordingPolicy()
        curriculum = FirstAttemptRecovery()
        driver = OnlineBatchDriver(
            population,
            policy=policy,
            curriculum=curriculum,
        )

        summary = driver.run(batch_steps=2)

        env = envs[0]
        self.assertEqual(policy.batch_sizes, [2, 2, 2, 2])
        self.assertEqual(curriculum.calls, 2)
        self.assertEqual(env.restore_calls, [[0]])
        self.assertEqual([call[0] for call in env.reset_calls], [[1], [0]])
        self.assertEqual(summary.batch_steps, 2)
        self.assertEqual(summary.slot_steps, 4)
        self.assertEqual(summary.decision_rounds, 4)
        self.assertEqual(summary.terminal_attempts, 3)
        self.assertEqual(summary.victories, 1)
        self.assertEqual(summary.defeats, 2)
        self.assertEqual(summary.terminal_progress.attempts, 3)
        self.assertEqual(summary.terminal_progress.floor_sum, 120)
        self.assertEqual(summary.terminal_progress.min_floor, 40)
        self.assertEqual(summary.terminal_progress.max_floor, 40)
        self.assertEqual(summary.terminal_progress.mean_floor, 40.0)
        self.assertEqual(summary.terminal_progress.floor_counts, ((40, 3),))
        self.assertEqual(summary.terminal_progress.act_counts, ((3, 3),))
        self.assertEqual(summary.victories + summary.defeats, summary.terminal_attempts)
        self.assertEqual(summary.completed_episodes, 2)
        self.assertEqual(summary.recoveries, 1)
        self.assertEqual(summary.active_slots, 2)
        self.assertGreater(summary.steps_per_second, 0.0)
        self.assertEqual(driver.ledger.snapshot(0).episode_generation, 1)
        self.assertEqual(driver.ledger.snapshot(1).episode_generation, 1)
        self.assertNotEqual(tuple(env.seeds), initial_seeds)

    def test_driver_can_park_and_refill_one_complete_slot_cohort(self) -> None:
        envs: list[FakeBatchEnv] = []

        def factory(seeds: list[int]) -> FakeBatchEnv:
            env = FakeBatchEnv(
                seeds,
                terminal_plans=({0: 1}, {1: -1}),
            )
            envs.append(env)
            return env

        population = initialize_population(
            factory,
            slot_count=2,
            schedule=SeedSchedule(SeedPartition.TRAINING),
            max_recoveries_per_episode=0,
        )
        driver = OnlineBatchDriver(
            population,
            policy=RecordingPolicy(),
            curriculum=NoRecovery(),
        )

        first = driver.advance(refill_completed=False)
        second = driver.advance(refill_completed=False)

        env = envs[0]
        self.assertEqual(len(first.completed_episodes), 1)
        self.assertEqual(len(second.completed_episodes), 1)
        self.assertEqual(env.terminal_count, 2)
        self.assertEqual(env.reset_calls, [])
        self.assertEqual(driver.refill_completed(), (0, 1))
        self.assertEqual(env.terminal_count, 0)
        self.assertEqual([call[0] for call in env.reset_calls], [[0, 1]])
        self.assertEqual(
            tuple(snapshot.episode_generation for snapshot in driver.ledger.snapshots()),
            (1, 1),
        )

    def test_terminal_target_run_stops_after_an_atomic_multi_terminal_step(self) -> None:
        population = initialize_population(
            lambda seeds: FakeBatchEnv(
                seeds,
                terminal_plans=({0: 1, 1: -1}, {0: 1}),
            ),
            slot_count=2,
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            max_recoveries_per_episode=0,
        )
        driver = OnlineBatchDriver(
            population,
            policy=RecordingPolicy(),
            curriculum=NoRecovery(),
        )

        result = driver.run_until_terminal_attempts(
            terminal_attempts=1,
            max_batch_steps=5,
        )

        self.assertTrue(result.target_reached)
        self.assertFalse(result.step_limit_reached)
        self.assertEqual(result.summary.batch_steps, 1)
        self.assertEqual(result.summary.terminal_attempts, 2)
        self.assertEqual(result.summary.victories, 1)
        self.assertEqual(result.summary.defeats, 1)

    def test_terminal_target_run_reports_limit_and_continues_from_same_driver(self) -> None:
        population = initialize_population(
            lambda seeds: FakeBatchEnv(
                seeds,
                terminal_plans=({}, {0: 1}),
            ),
            slot_count=1,
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            max_recoveries_per_episode=0,
        )
        driver = OnlineBatchDriver(
            population,
            policy=RecordingPolicy(),
            curriculum=NoRecovery(),
        )

        exhausted = driver.run_until_terminal_attempts(
            terminal_attempts=1,
            max_batch_steps=1,
        )
        reached = driver.run_until_terminal_attempts(
            terminal_attempts=1,
            max_batch_steps=1,
        )

        self.assertFalse(exhausted.target_reached)
        self.assertTrue(exhausted.step_limit_reached)
        self.assertEqual(exhausted.summary.terminal_attempts, 0)
        self.assertEqual(exhausted.summary.victories, 0)
        self.assertEqual(exhausted.summary.defeats, 0)
        self.assertTrue(reached.target_reached)
        self.assertEqual(reached.summary.batch_steps, 1)
        self.assertEqual(reached.summary.terminal_attempts, 1)
        self.assertEqual(reached.summary.victories, 1)
        self.assertEqual(reached.summary.defeats, 0)

    def test_zero_terminal_target_mutates_nothing_and_parameters_are_typed(self) -> None:
        population = initialize_population(
            FakeBatchEnv,
            slot_count=1,
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            max_recoveries_per_episode=0,
        )
        policy = RecordingPolicy()
        driver = OnlineBatchDriver(
            population,
            policy=policy,
            curriculum=NoRecovery(),
        )

        result = driver.run_until_terminal_attempts(
            terminal_attempts=0,
            max_batch_steps=5,
        )

        self.assertTrue(result.target_reached)
        self.assertFalse(result.step_limit_reached)
        self.assertEqual(result.summary.batch_steps, 0)
        self.assertEqual(result.summary.terminal_progress.attempts, 0)
        self.assertIsNone(result.summary.terminal_progress.mean_floor)
        self.assertIsNone(result.summary.terminal_progress.min_floor)
        self.assertIsNone(result.summary.terminal_progress.max_floor)
        self.assertEqual(result.summary.terminal_progress.floor_counts, ())
        self.assertEqual(result.summary.terminal_progress.act_counts, ())
        self.assertEqual(policy.batch_sizes, [])
        for arguments, message in (
            ({"terminal_attempts": -1, "max_batch_steps": 1}, "non-negative"),
            ({"terminal_attempts": 1, "max_batch_steps": True}, "not bool"),
        ):
            with self.subTest(arguments=arguments):
                with self.assertRaisesRegex(BatchDriverError, message):
                    driver.run_until_terminal_attempts(**arguments)

    def test_invalid_policy_is_rejected_before_environment_mutation(self) -> None:
        envs: list[FakeBatchEnv] = []

        def factory(seeds: list[int]) -> FakeBatchEnv:
            env = FakeBatchEnv(seeds)
            envs.append(env)
            return env

        population = initialize_population(
            factory,
            slot_count=2,
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            max_recoveries_per_episode=0,
        )
        driver = OnlineBatchDriver(
            population,
            policy=InvalidPolicy(),
            curriculum=NoRecovery(),
        )

        with self.assertRaisesRegex(BatchDriverError, "outside"):
            driver.advance()
        self.assertEqual(envs[0].choose_calls, [])
        self.assertEqual(envs[0].terminal_count, 0)

    def test_untyped_policy_result_is_rejected_before_environment_mutation(self) -> None:
        population = initialize_population(
            FakeBatchEnv,
            slot_count=1,
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            max_recoveries_per_episode=0,
        )
        driver = OnlineBatchDriver(
            population,
            policy=UntypedPolicy(),  # type: ignore[arg-type]
            curriculum=NoRecovery(),
        )

        with self.assertRaisesRegex(BatchDriverError, "BatchPolicyChoice"):
            driver.advance()
        self.assertEqual(driver.env.choose_calls, [])  # type: ignore[attr-defined]

    def test_invalid_probability_evidence_is_rejected_before_environment_mutation(
        self,
    ) -> None:
        for policy, message in (
            (MisalignedProbabilityPolicy(), "one value per row"),
            (UntypedProbabilityPolicy(), "must be typed"),
        ):
            with self.subTest(policy=type(policy).__name__):
                population = initialize_population(
                    FakeBatchEnv,
                    slot_count=2,
                    schedule=SeedSchedule(SeedPartition.HELD_OUT),
                    max_recoveries_per_episode=0,
                )
                driver = OnlineBatchDriver(
                    population,
                    policy=policy,
                    curriculum=NoRecovery(),
                )

                with self.assertRaisesRegex(BatchDriverError, message):
                    driver.advance()
                self.assertEqual(driver.env.choose_calls, [])  # type: ignore[attr-defined]

    def test_population_rejects_environment_slot_mismatch(self) -> None:
        with self.assertRaisesRegex(BatchDriverError, "created 1 slots"):
            initialize_population(
                lambda seeds: FakeBatchEnv(seeds[:-1]),
                slot_count=2,
                schedule=SeedSchedule(SeedPartition.TRAINING),
                max_recoveries_per_episode=0,
            )

    def test_driver_consumes_bounded_segments_without_retaining_a_queue(self) -> None:
        envs: list[NumpyFakeBatchEnv] = []

        def factory(seeds: list[int]) -> NumpyFakeBatchEnv:
            env = NumpyFakeBatchEnv(
                seeds,
                terminal_plans=({0: 1, 1: -1},),
            )
            envs.append(env)
            return env

        population = initialize_population(
            factory,
            slot_count=2,
            schedule=SeedSchedule(SeedPartition.HELD_OUT),
            max_recoveries_per_episode=0,
        )
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=2,
                max_payload_bytes=1_000_000,
            )
        )
        consumed: list[ExperienceSegment] = []
        driver = OnlineBatchDriver(
            population,
            policy=ArrayFirstPolicy(),
            curriculum=NoRecovery(),
            experience_buffer=buffer,
            experience_sink=consumed.append,
        )

        result = driver.advance()

        self.assertEqual(result.emitted_experience_segments, 1)
        self.assertEqual(result.emitted_experience_decisions, 2)
        self.assertEqual(len(consumed), 1)
        self.assertTrue(consumed[0].censored)
        self.assertEqual(buffer.decision_count, 2)
        final = driver.flush_experience()
        assert final is not None
        self.assertEqual(len(consumed), 2)
        self.assertFalse(final.censored)
        self.assertTrue(all(fragment.terminal for fragment in final.attempts))
        self.assertTrue(buffer.empty)

    def test_experience_buffer_and_sink_must_be_paired(self) -> None:
        population = initialize_population(
            lambda seeds: FakeBatchEnv(seeds),
            slot_count=1,
            schedule=SeedSchedule(SeedPartition.TRAINING),
            max_recoveries_per_episode=0,
        )
        with self.assertRaisesRegex(BatchDriverError, "configured together"):
            OnlineBatchDriver(
                population,
                policy=RecordingPolicy(),
                curriculum=NoRecovery(),
                experience_buffer=ExperienceSegmentBuffer(
                    ExperienceLimits(
                        max_decisions=1,
                        max_payload_bytes=1024,
                    )
                ),
            )

    def test_failing_experience_sink_stops_before_the_current_choice(self) -> None:
        envs: list[NumpyFakeBatchEnv] = []

        def factory(seeds: list[int]) -> NumpyFakeBatchEnv:
            env = NumpyFakeBatchEnv(seeds)
            envs.append(env)
            return env

        population = initialize_population(
            factory,
            slot_count=2,
            schedule=SeedSchedule(SeedPartition.TRAINING),
            max_recoveries_per_episode=0,
        )

        def fail_sink(segment: ExperienceSegment) -> None:
            raise RuntimeError("sink failed")

        driver = OnlineBatchDriver(
            population,
            policy=ArrayFirstPolicy(),
            curriculum=NoRecovery(),
            experience_buffer=ExperienceSegmentBuffer(
                ExperienceLimits(
                    max_decisions=2,
                    max_payload_bytes=1_000_000,
                )
            ),
            experience_sink=fail_sink,
        )

        with self.assertRaisesRegex(RuntimeError, "sink failed"):
            driver.advance()
        self.assertEqual(len(envs[0].choose_calls), 1)
        with self.assertRaisesRegex(BatchDriverError, "previously failed"):
            driver.advance()

    def test_rejected_choice_is_not_committed_as_experience(self) -> None:
        envs: list[OneRejectedChoiceEnv] = []

        def factory(seeds: list[int]) -> OneRejectedChoiceEnv:
            env = OneRejectedChoiceEnv(seeds)
            envs.append(env)
            return env

        population = initialize_population(
            factory,
            slot_count=2,
            schedule=SeedSchedule(SeedPartition.TRAINING),
            max_recoveries_per_episode=0,
        )
        buffer = ExperienceSegmentBuffer(
            ExperienceLimits(
                max_decisions=2,
                max_payload_bytes=1_000_000,
            )
        )
        consumed: list[ExperienceSegment] = []
        driver = OnlineBatchDriver(
            population,
            policy=ArrayFirstPolicy(),
            curriculum=NoRecovery(),
            experience_buffer=buffer,
            experience_sink=consumed.append,
        )

        with self.assertRaisesRegex(RuntimeError, "choice rejected"):
            driver.advance()
        self.assertEqual(len(consumed), 1)
        self.assertTrue(buffer.empty)

        result = driver.advance()
        self.assertEqual(result.emitted_experience_segments, 0)
        self.assertEqual(buffer.decision_count, 2)
        self.assertEqual(len(envs[0].choose_calls), 2)


if __name__ == "__main__":
    unittest.main()
