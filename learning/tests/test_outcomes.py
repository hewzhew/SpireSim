from __future__ import annotations

import unittest
from dataclasses import replace

import numpy as np

from sts_learning import (
    FloorProgressReturnConfig,
    OnPolicyObjectiveConfig,
    RunPolicyUpdateConfig,
    RunPolicyUpdateRule,
    TerminalAdvantageMode,
    TerminalAttemptOutcome,
    TerminalBatchError,
    TerminalReturnError,
    TerminalStepBatch,
    terminal_return_advantages,
)


def bridge_step(**overrides: object) -> dict[str, object]:
    step: dict[str, object] = {
        "terminal_slot_indices": [2, 0],
        "terminal_reward": [-1, 1],
        "terminal_act": [2, 3],
        "terminal_floor": [28, 51],
        "terminal_hp": [0, 37],
        "terminal_max_hp": [84, 90],
        "terminal_gold": [42, 101],
        "unrelated_large_tensor": object(),
    }
    step.update(overrides)
    return step


class TerminalStepBatchTests(unittest.TestCase):
    def test_numpy_bridge_columns_are_copied_to_plain_integers(self) -> None:
        step = bridge_step(
            terminal_slot_indices=np.array([2, 0], dtype=np.uint64),
            terminal_reward=np.array([-1, 1], dtype=np.int8),
            terminal_act=np.array([2, 3], dtype=np.uint8),
            terminal_floor=np.array([28, 51], dtype=np.int32),
            terminal_hp=np.array([0, 37], dtype=np.int32),
            terminal_max_hp=np.array([84, 90], dtype=np.int32),
            terminal_gold=np.array([42, 101], dtype=np.int32),
        )

        batch = TerminalStepBatch.from_bridge_step(step, slot_count=3)

        self.assertEqual(batch.slot_indices, (2, 0))
        self.assertTrue(
            all(
                type(value) is int
                for value in batch.attempts[0].__dict__.values()
            )
        )

    def test_bridge_step_copies_only_compact_aligned_terminal_rows(self) -> None:
        step = bridge_step()

        batch = TerminalStepBatch.from_bridge_step(step, slot_count=3)

        self.assertEqual(batch.slot_indices, (2, 0))
        self.assertEqual(batch.attempts[0].terminal_floor, 28)
        self.assertEqual(batch.attempts[1].terminal_hp, 37)
        self.assertNotIn(step["unrelated_large_tensor"], batch.__dict__.values())

    def test_missing_or_misaligned_columns_fail_before_producing_a_batch(self) -> None:
        missing = bridge_step()
        del missing["terminal_gold"]
        with self.assertRaisesRegex(TerminalBatchError, "missing terminal_gold"):
            TerminalStepBatch.from_bridge_step(missing, slot_count=3)

        with self.assertRaisesRegex(TerminalBatchError, "has 1 rows, expected 2"):
            TerminalStepBatch.from_bridge_step(
                bridge_step(terminal_hp=[0]),
                slot_count=3,
            )

    def test_batch_is_bounded_by_pool_and_rejects_duplicate_or_invalid_rows(self) -> None:
        with self.assertRaisesRegex(TerminalBatchError, "outside 0..2"):
            TerminalStepBatch.from_bridge_step(bridge_step(), slot_count=2)
        with self.assertRaisesRegex(TerminalBatchError, "duplicate slots"):
            TerminalStepBatch.from_bridge_step(
                bridge_step(terminal_slot_indices=[0, 0]),
                slot_count=3,
            )
        with self.assertRaisesRegex(TerminalBatchError, "must be -1 or 1"):
            TerminalStepBatch.from_bridge_step(
                bridge_step(terminal_reward=[0, 1]),
                slot_count=3,
            )

    def test_direct_rows_reject_boolean_and_non_integral_values(self) -> None:
        with self.assertRaisesRegex(TerminalBatchError, "not bool"):
            TerminalAttemptOutcome(True, 1, 3, 51, 20, 80, 50)
        with self.assertRaisesRegex(TerminalBatchError, "must be an integer"):
            TerminalAttemptOutcome(0, 1, 3, 51.5, 20, 80, 50)


class OnPolicyObjectiveConfigTests(unittest.TestCase):
    def test_objective_owns_typed_return_and_positive_update_size(self) -> None:
        terminal_return = FloorProgressReturnConfig(target_floor=52)

        objective = OnPolicyObjectiveConfig(
            terminal_return=terminal_return,
            attempts_per_update=4,
        )

        self.assertIs(objective.terminal_return, terminal_return)
        self.assertEqual(objective.attempts_per_update, 4)
        self.assertIs(
            objective.policy_update.rule,
            RunPolicyUpdateRule.REINFORCE,
        )
        self.assertIs(
            objective.advantage_mode,
            TerminalAdvantageMode.RAW_RETURN,
        )
        with self.assertRaisesRegex(TerminalReturnError, "must be typed"):
            OnPolicyObjectiveConfig(terminal_return=object())
        for invalid in (True, 0, -1, 1.5):
            with self.subTest(invalid=invalid):
                with self.assertRaises(TerminalReturnError):
                    OnPolicyObjectiveConfig(attempts_per_update=invalid)

    def test_leave_one_out_uses_only_other_attempt_returns(self) -> None:
        returns = (-0.8, -0.6, 1.0)

        self.assertEqual(
            terminal_return_advantages(
                returns,
                TerminalAdvantageMode.RAW_RETURN,
            ),
            returns,
        )
        advantages = terminal_return_advantages(
            returns,
            TerminalAdvantageMode.LEAVE_ONE_OUT,
        )

        for actual, expected in zip(advantages, (-1.0, -0.7, 1.7), strict=True):
            self.assertAlmostEqual(actual, expected)
        self.assertAlmostEqual(sum(advantages), 0.0)
        with self.assertRaisesRegex(TerminalReturnError, "at least two"):
            OnPolicyObjectiveConfig(
                attempts_per_update=1,
                advantage_mode=TerminalAdvantageMode.LEAVE_ONE_OUT,
            )
        for matched_mode in (
            TerminalAdvantageMode.MATCHED_FLOOR_LEAVE_ONE_OUT,
            TerminalAdvantageMode.MATCHED_FLOOR_CONTEXT_LEAVE_ONE_OUT,
            TerminalAdvantageMode.MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT,
        ):
            with self.subTest(matched_mode=matched_mode):
                with self.assertRaisesRegex(TerminalReturnError, "at least two"):
                    OnPolicyObjectiveConfig(
                        attempts_per_update=1,
                        advantage_mode=matched_mode,
                    )

    def test_value_ppo_is_explicit_and_rejects_ambiguous_advantage_modes(self) -> None:
        update = RunPolicyUpdateConfig.ppo_clip_value()
        with self.assertRaisesRegex(ValueError, "unknown run value"):
            replace(update, value_prediction_target="unknown")
        with self.assertRaisesRegex(ValueError, "REINFORCE"):
            RunPolicyUpdateConfig(value_prediction_target="terminal_score")

        objective = OnPolicyObjectiveConfig(
            advantage_mode=TerminalAdvantageMode.DECISION_LOCAL_GAE,
            policy_update=update,
        )

        self.assertIs(objective.policy_update, update)
        self.assertTrue(objective.policy_update.uses_value_baseline)
        self.assertEqual(objective.policy_update.epochs, 4)
        self.assertTrue(objective.policy_update.normalize_advantage)
        self.assertEqual(objective.policy_update.value_clip_coefficient, 0.2)
        with self.assertRaisesRegex(TerminalReturnError, "decision-local"):
            OnPolicyObjectiveConfig(
                attempts_per_update=2,
                advantage_mode=TerminalAdvantageMode.LEAVE_ONE_OUT,
                policy_update=update,
            )

    def test_critic_calibration_owns_a_larger_unclipped_supervised_budget(self) -> None:
        update = RunPolicyUpdateConfig.critic_calibration()

        self.assertEqual(update.epochs, 256)
        self.assertIsNone(update.value_clip_coefficient)
        self.assertIsNone(update.max_grad_norm)
        self.assertEqual(update.value_loss_coefficient, 1.0)
        with self.assertRaisesRegex(TerminalReturnError, "1024"):
            RunPolicyUpdateConfig.critic_calibration(steps=1025)
        with self.assertRaisesRegex(TerminalReturnError, "64"):
            replace(RunPolicyUpdateConfig.ppo_clip_value(), epochs=65)


if __name__ == "__main__":
    unittest.main()
