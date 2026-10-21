from __future__ import annotations

import unittest

from sts_learning import (
    RecoveryLedger,
    SeedPartition,
    SeedPartitionSpec,
    SeedSchedule,
    SeedScheduleError,
    TerminalAttemptOutcome,
    TerminalStepBatch,
    reset_scheduled_checkpointed_with_accounting,
    reset_scheduled_with_accounting,
)


def terminal_batch(*rows: tuple[int, int]) -> TerminalStepBatch:
    return TerminalStepBatch(
        tuple(
            TerminalAttemptOutcome(
                slot_index=slot,
                terminal_reward=reward,
                terminal_act=3,
                terminal_floor=40,
                terminal_hp=10 if reward == 1 else 0,
                terminal_max_hp=80,
                terminal_gold=50,
            )
            for slot, reward in rows
        )
    )


class FakeResetEnv:
    def __init__(self, *, fail: bool = False, checkpoints: object = None) -> None:
        self.fail = fail
        self.checkpoints = checkpoints
        self.calls: list[tuple[list[int], list[int]]] = []

    def reset_slots(self, slot_indices: list[int], seeds: list[int]) -> None:
        self.calls.append((slot_indices, seeds))
        if self.fail:
            raise RuntimeError("reset failed")

    def reset_slots_checkpointed(
        self,
        slot_indices: list[int],
        seeds: list[int],
    ) -> object:
        self.reset_slots(slot_indices, seeds)
        return self.checkpoints


class SeedScheduleTests(unittest.TestCase):
    def test_partition_is_stable_and_seed_only(self) -> None:
        spec = SeedPartitionSpec(held_out_numerator=1, denominator=4)

        self.assertEqual(
            [spec.classify(seed) for seed in range(8)],
            [
                SeedPartition.TRAINING,
                SeedPartition.TRAINING,
                SeedPartition.TRAINING,
                SeedPartition.HELD_OUT,
                SeedPartition.TRAINING,
                SeedPartition.HELD_OUT,
                SeedPartition.TRAINING,
                SeedPartition.TRAINING,
            ],
        )

    def test_partition_schedules_are_disjoint_and_repeatable(self) -> None:
        spec = SeedPartitionSpec(held_out_numerator=1, denominator=5)
        training = SeedSchedule(SeedPartition.TRAINING, spec)
        held_out = SeedSchedule(SeedPartition.HELD_OUT, spec)

        training_batch, training_next = training.plan([0, 2, 4, 6])
        held_out_batch, held_out_next = held_out.plan([1, 3, 5, 7])
        repeated, repeated_next = training.plan([0, 2, 4, 6])

        self.assertEqual(training_batch, repeated)
        self.assertEqual(training_next, repeated_next)
        self.assertTrue(set(training_batch.seeds).isdisjoint(held_out_batch.seeds))
        self.assertTrue(
            all(spec.classify(seed) is SeedPartition.TRAINING for seed in training_batch.seeds)
        )
        self.assertTrue(
            all(spec.classify(seed) is SeedPartition.HELD_OUT for seed in held_out_batch.seeds)
        )
        self.assertGreater(training_next.next_candidate, training.next_candidate)
        self.assertGreater(held_out_next.next_candidate, held_out.next_candidate)

    def test_failed_reset_consumes_neither_generation_nor_schedule(self) -> None:
        spec = SeedPartitionSpec(held_out_numerator=1, denominator=3)
        initial_batch, schedule = SeedSchedule(SeedPartition.HELD_OUT, spec).plan(
            [0, 1]
        )
        ledger = RecoveryLedger.held_out(initial_batch.seeds)
        ledger.record_terminal(terminal_batch((0, 1), (1, -1)))
        ledger.complete_defeats([1])
        failing = FakeResetEnv(fail=True)

        with self.assertRaisesRegex(RuntimeError, "reset failed"):
            reset_scheduled_with_accounting(failing, [0, 1], ledger, schedule)
        self.assertEqual(ledger.snapshot(0).episode_generation, 0)
        self.assertEqual(ledger.snapshot(1).episode_generation, 0)
        self.assertEqual(ledger.snapshot(0).episode_seed, initial_batch.seeds[0])
        self.assertEqual(ledger.snapshot(1).episode_seed, initial_batch.seeds[1])

        working = FakeResetEnv()
        batch, next_schedule = reset_scheduled_with_accounting(
            working, [0, 1], ledger, schedule
        )
        self.assertEqual(working.calls, [([0, 1], list(batch.seeds))])
        self.assertEqual(ledger.snapshot(0).episode_generation, 1)
        self.assertEqual(ledger.snapshot(1).episode_generation, 1)
        self.assertEqual(ledger.snapshot(0).episode_seed, batch.seeds[0])
        self.assertEqual(ledger.snapshot(1).episode_seed, batch.seeds[1])
        self.assertTrue(set(initial_batch.seeds).isdisjoint(batch.seeds))
        self.assertGreater(next_schedule.next_candidate, schedule.next_candidate)

    def test_ledger_mode_must_match_seed_partition(self) -> None:
        initial_batch, _ = SeedSchedule(SeedPartition.HELD_OUT).plan([0])
        ledger = RecoveryLedger.held_out(initial_batch.seeds)
        ledger.record_terminal(terminal_batch((0, 1)))
        env = FakeResetEnv()

        with self.assertRaisesRegex(SeedScheduleError, "requires held_out seeds"):
            reset_scheduled_with_accounting(
                env,
                [0],
                ledger,
                SeedSchedule(SeedPartition.TRAINING),
            )
        self.assertEqual(env.calls, [])
        self.assertEqual(ledger.snapshot(0).episode_generation, 0)

    def test_checkpointed_reset_commits_only_with_exact_new_roots(self) -> None:
        initial, schedule = SeedSchedule(SeedPartition.TRAINING).plan([0, 1])
        ledger = RecoveryLedger.training(
            initial.seeds,
            max_recoveries_per_episode=0,
        )
        ledger.record_terminal(terminal_batch((0, 1), (1, -1)))
        ledger.complete_defeats([1])
        failing = FakeResetEnv(fail=True)

        with self.assertRaisesRegex(RuntimeError, "reset failed"):
            reset_scheduled_checkpointed_with_accounting(
                failing,
                [0, 1],
                ledger,
                schedule,
            )
        self.assertEqual(ledger.snapshot(0).episode_generation, 0)
        self.assertEqual(ledger.snapshot(1).episode_generation, 0)

        roots = object()
        working = FakeResetEnv(checkpoints=roots)
        batch, next_schedule, returned = (
            reset_scheduled_checkpointed_with_accounting(
                working,
                [0, 1],
                ledger,
                schedule,
            )
        )
        self.assertIs(returned, roots)
        self.assertEqual(working.calls, [([0, 1], list(batch.seeds))])
        self.assertEqual(ledger.snapshot(0).episode_generation, 1)
        self.assertEqual(ledger.snapshot(1).episode_generation, 1)
        self.assertGreater(next_schedule.next_candidate, schedule.next_candidate)

    def test_invalid_or_empty_schedules_fail_before_scanning(self) -> None:
        with self.assertRaisesRegex(SeedScheduleError, "must be a SeedPartition"):
            SeedSchedule("training")  # type: ignore[arg-type]
        with self.assertRaisesRegex(SeedScheduleError, "held-out partition is empty"):
            SeedSchedule(
                SeedPartition.HELD_OUT,
                SeedPartitionSpec(held_out_numerator=0, denominator=1),
            )
        with self.assertRaisesRegex(SeedScheduleError, "training partition is empty"):
            SeedSchedule(
                SeedPartition.TRAINING,
                SeedPartitionSpec(held_out_numerator=1, denominator=1),
            )
        with self.assertRaisesRegex(SeedScheduleError, "duplicate"):
            SeedSchedule(SeedPartition.TRAINING).plan([0, 0])


if __name__ == "__main__":
    unittest.main()
