from __future__ import annotations

import unittest

from sts_learning import (
    RecoveryLedger,
    RecoveryProtocolError,
    RecoverySlotStatus,
    TerminalAttemptOutcome,
    TerminalStepBatch,
    reset_with_accounting,
    restore_with_accounting,
)


def terminal(
    slot_index: int,
    reward: int,
    *,
    act: int = 2,
    floor: int = 20,
    hp: int = 0,
) -> TerminalAttemptOutcome:
    return TerminalAttemptOutcome(
        slot_index=slot_index,
        terminal_reward=reward,
        terminal_act=act,
        terminal_floor=floor,
        terminal_hp=hp,
        terminal_max_hp=80,
        terminal_gold=50,
    )


def terminal_batch(*attempts: TerminalAttemptOutcome) -> TerminalStepBatch:
    return TerminalStepBatch(attempts)


class FakeRestoreEnv:
    def __init__(self, *, fail: bool = False) -> None:
        self.fail = fail
        self.calls: list[tuple[list[int], object]] = []

    def restore_slots(self, slot_indices: list[int], checkpoints: object) -> None:
        self.calls.append((slot_indices, checkpoints))
        if self.fail:
            raise RuntimeError("restore failed")


class FakeResetEnv:
    def __init__(self, *, fail: bool = False) -> None:
        self.fail = fail
        self.calls: list[tuple[list[int], list[int]]] = []

    def reset_slots(self, slot_indices: list[int], seeds: list[int]) -> None:
        self.calls.append((slot_indices, seeds))
        if self.fail:
            raise RuntimeError("reset failed")


class RecoveryLedgerTests(unittest.TestCase):
    def test_training_lifecycle_reports_attempts_and_zero_recovery(self) -> None:
        ledger = RecoveryLedger.training([100, 101], max_recoveries_per_episode=2)

        first_defeat = terminal(0, -1, floor=17)
        victory = terminal(1, 1, act=3, floor=51, hp=24)
        accounting = ledger.record_terminal(terminal_batch(first_defeat, victory))
        victories = accounting.completed_episodes
        self.assertEqual(accounting.attempts[0].episode_seed, 100)
        self.assertEqual(accounting.attempts[0].attempt_index, 1)
        self.assertEqual(accounting.attempts[0].terminal, first_defeat)
        self.assertEqual(accounting.attempts[1].episode_seed, 101)
        self.assertEqual(len(victories), 1)
        self.assertEqual(victories[0].slot_index, 1)
        self.assertEqual(victories[0].episode_seed, 101)
        self.assertEqual(victories[0].terminal, victory)
        self.assertTrue(victories[0].zero_recovery)

        first = ledger.prepare_recovery([0])
        events = ledger.commit_recovery(first)
        self.assertEqual(events[0].attempt_index, 2)
        self.assertEqual(events[0].recoveries_used, 1)
        self.assertEqual(events[0].episode_seed, 100)
        second = ledger.record_terminal(
            terminal_batch(terminal(0, -1, floor=30))
        )
        self.assertEqual(second.attempts[0].episode_seed, 100)
        self.assertEqual(second.attempts[0].attempt_index, 2)
        self.assertEqual(second.attempts[0].recoveries_used, 1)
        ledger.commit_recovery(ledger.prepare_recovery([0]))
        final_defeat = terminal(0, -1, act=3, floor=44)
        third = ledger.record_terminal(terminal_batch(final_defeat))
        self.assertEqual(third.attempts[0].episode_seed, 100)
        self.assertEqual(third.attempts[0].attempt_index, 3)
        self.assertEqual(third.attempts[0].recoveries_used, 2)
        with self.assertRaisesRegex(RecoveryProtocolError, "exhausted"):
            ledger.prepare_recovery([0])

        defeats = ledger.complete_defeats([0])
        self.assertEqual(defeats[0].attempts, 3)
        self.assertEqual(defeats[0].recoveries_used, 2)
        self.assertEqual(defeats[0].terminal, final_defeat)
        self.assertFalse(defeats[0].zero_recovery)
        reset = ledger.prepare_reset([0, 1], [200, 201])
        ledger.commit_reset(reset)
        self.assertEqual(ledger.snapshot(0).episode_generation, 1)
        self.assertEqual(ledger.snapshot(0).episode_seed, 200)
        self.assertEqual(ledger.snapshot(0).status, RecoverySlotStatus.ACTIVE)
        next_victory = ledger.record_terminal(
            terminal_batch(terminal(0, 1, act=3, floor=51, hp=30))
        ).completed_episodes[0]
        self.assertEqual(next_victory.episode_seed, 200)
        self.assertEqual(next_victory.episode_generation, 1)

    def test_held_out_mode_cannot_prepare_recovery(self) -> None:
        ledger = RecoveryLedger.held_out([300])
        ledger.record_terminal(terminal_batch(terminal(0, -1)))

        with self.assertRaisesRegex(RecoveryProtocolError, "forbids recovery"):
            ledger.prepare_recovery([0])
        outcome = ledger.complete_defeats([0])[0]
        self.assertTrue(outcome.zero_recovery)
        self.assertEqual(outcome.attempts, 1)

    def test_active_resume_snapshots_restore_exact_lineage(self) -> None:
        ledger = RecoveryLedger.training([100, (1 << 64) - 1], max_recoveries_per_episode=2)
        ledger.record_terminal(terminal_batch(terminal(0, -1)))
        ledger.commit_recovery(ledger.prepare_recovery([0]))
        snapshots = ledger.snapshots()

        restored = RecoveryLedger.from_active_snapshots(
            snapshots,
            mode=ledger.mode,
            max_recoveries_per_episode=ledger.max_recoveries_per_episode,
        )
        self.assertEqual(restored.snapshots(), snapshots)

        pending = RecoveryLedger.training([5], max_recoveries_per_episode=1)
        pending.record_terminal(terminal_batch(terminal(0, -1)))
        with self.assertRaisesRegex(RecoveryProtocolError, "unfinished"):
            RecoveryLedger.from_active_snapshots(
                pending.snapshots(),
                mode=pending.mode,
                max_recoveries_per_episode=1,
            )

    def test_inactive_terminal_batch_does_not_mutate_valid_prefix(self) -> None:
        ledger = RecoveryLedger.training([400, 401], max_recoveries_per_episode=1)
        ledger.record_terminal(terminal_batch(terminal(1, 1, hp=10)))

        with self.assertRaisesRegex(RecoveryProtocolError, "slot 1 is not active"):
            ledger.record_terminal(
                terminal_batch(terminal(0, -1), terminal(1, -1))
            )
        self.assertEqual(
            ledger.snapshot(0).status,
            RecoverySlotStatus.ACTIVE,
        )
        self.assertIsNone(ledger.snapshot(0).pending_terminal)

    def test_restore_failure_keeps_defeat_pending_before_commit(self) -> None:
        ledger = RecoveryLedger.training([500], max_recoveries_per_episode=1)
        pending = terminal(0, -1, floor=22)
        ledger.record_terminal(terminal_batch(pending))
        checkpoints = object()
        failing = FakeRestoreEnv(fail=True)

        with self.assertRaisesRegex(RuntimeError, "restore failed"):
            restore_with_accounting(failing, [0], checkpoints, ledger)
        self.assertEqual(ledger.snapshot(0).status, RecoverySlotStatus.DEFEAT_PENDING)
        self.assertEqual(ledger.snapshot(0).recoveries_used, 0)
        self.assertEqual(ledger.snapshot(0).pending_terminal, pending)

        working = FakeRestoreEnv()
        events = restore_with_accounting(working, [0], checkpoints, ledger)
        self.assertEqual(working.calls, [([0], checkpoints)])
        self.assertEqual(events[0].recoveries_used, 1)
        self.assertEqual(ledger.snapshot(0).status, RecoverySlotStatus.ACTIVE)
        self.assertIsNone(ledger.snapshot(0).pending_terminal)

    def test_complete_defeat_batch_rejects_invalid_suffix_without_mutating_prefix(self) -> None:
        ledger = RecoveryLedger.training([600, 601], max_recoveries_per_episode=1)
        first = terminal(0, -1, floor=18)
        ledger.record_terminal(
            terminal_batch(first, terminal(1, -1, floor=19))
        )
        ledger.commit_recovery(ledger.prepare_recovery([1]))

        with self.assertRaisesRegex(RecoveryProtocolError, "slot 1 has no pending"):
            ledger.complete_defeats([0, 1])
        self.assertEqual(
            ledger.snapshot(0).status,
            RecoverySlotStatus.DEFEAT_PENDING,
        )
        self.assertEqual(ledger.snapshot(0).pending_terminal, first)

    def test_reset_failure_keeps_completed_generations_before_commit(self) -> None:
        ledger = RecoveryLedger.held_out([700, 701])
        ledger.record_terminal(
            terminal_batch(terminal(0, 1, hp=20), terminal(1, -1))
        )
        ledger.complete_defeats([1])
        failing = FakeResetEnv(fail=True)

        with self.assertRaisesRegex(RuntimeError, "reset failed"):
            reset_with_accounting(failing, [0, 1], [101, 102], ledger)
        self.assertEqual(ledger.snapshot(0).episode_generation, 0)
        self.assertEqual(ledger.snapshot(1).episode_generation, 0)
        self.assertEqual(ledger.snapshot(0).episode_seed, 700)
        self.assertEqual(ledger.snapshot(1).episode_seed, 701)

        working = FakeResetEnv()
        reset_with_accounting(working, [0, 1], [101, 102], ledger)
        self.assertEqual(working.calls, [([0, 1], [101, 102])])
        self.assertEqual(ledger.snapshot(0).episode_generation, 1)
        self.assertEqual(ledger.snapshot(0).episode_seed, 101)
        self.assertEqual(ledger.snapshot(1).episode_seed, 102)
        self.assertEqual(ledger.snapshot(1).status, RecoverySlotStatus.ACTIVE)

    def test_invalid_seed_fails_before_environment_reset_or_lineage_mutation(self) -> None:
        with self.assertRaisesRegex(RecoveryProtocolError, "not bool"):
            RecoveryLedger.held_out([True])

        ledger = RecoveryLedger.held_out([800])
        ledger.record_terminal(terminal_batch(terminal(0, 1, hp=20)))
        env = FakeResetEnv()

        with self.assertRaisesRegex(RecoveryProtocolError, r"0..2\^64-1"):
            reset_with_accounting(env, [0], [1 << 64], ledger)
        self.assertEqual(env.calls, [])
        self.assertEqual(ledger.snapshot(0).episode_seed, 800)
        self.assertEqual(ledger.snapshot(0).episode_generation, 0)
        self.assertEqual(
            ledger.snapshot(0).status,
            RecoverySlotStatus.VICTORY_COMPLETE,
        )


if __name__ == "__main__":
    unittest.main()
