from __future__ import annotations

import unittest

from sts_learning import (
    CombatAllLossAxis,
    CombatAllWinAxis,
    CombatObjectiveError,
    CombatWinObjectiveConfig,
)


class CombatWinObjectiveConfigTests(unittest.TestCase):
    def test_default_uses_terminal_hp_after_wins_are_solved(self) -> None:
        config = CombatWinObjectiveConfig()

        self.assertEqual(config.groups_per_update, 1)
        self.assertIs(config.all_win_axis, CombatAllWinAxis.TERMINAL_HP)
        self.assertIs(config.all_loss_axis, CombatAllLossAxis.NONE)

    def test_all_win_axis_requires_the_typed_enum(self) -> None:
        for value in (1, "terminal_hp", None):
            with self.subTest(value=value):
                with self.assertRaisesRegex(CombatObjectiveError, "all_win_axis"):
                    CombatWinObjectiveConfig(all_win_axis=value)  # type: ignore[arg-type]

    def test_all_loss_axis_requires_the_typed_enum(self) -> None:
        for value in (1, "enemy_hp_progress", None):
            with self.subTest(value=value):
                with self.assertRaisesRegex(CombatObjectiveError, "all_loss_axis"):
                    CombatWinObjectiveConfig(all_loss_axis=value)  # type: ignore[arg-type]


if __name__ == "__main__":
    unittest.main()
