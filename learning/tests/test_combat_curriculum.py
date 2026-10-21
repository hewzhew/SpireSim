from __future__ import annotations

import unittest
from dataclasses import replace

from sts_learning import (
    CombatAllLossAxis,
    CombatAllWinAxis,
    CombatAxisSignalSummary,
    CombatCurriculumError,
    CombatFrontierRootSource,
    CombatGroupSignalSummary,
    CombatRootCompetenceEvidence,
    CombatWinObjectiveConfig,
    build_combat_frontier_plan,
)


REPLICATES = 4
DECISIONS = 8


def _evidence(
    source_slot: int,
    wins: int,
    *,
    hp_signal: bool = False,
    enemy_signal: bool = False,
    unresolved: int = 0,
) -> CombatRootCompetenceEvidence:
    root_id = format(source_slot + 1, "x") * 64
    combat_hash = format(source_slot + 9, "x") * 64
    mixed = 0 < wins < REPLICATES
    signals = CombatGroupSignalSummary(
        root_id=root_id,
        exact_combat_state_hash=combat_hash,
        replicate_count=REPLICATES,
        decision_count=DECISIONS,
        win=CombatAxisSignalSummary(
            REPLICATES if mixed else 0,
            DECISIONS if mixed else 0,
        ),
        terminal_hp=CombatAxisSignalSummary(
            REPLICATES if hp_signal else 0,
            DECISIONS if hp_signal else 0,
        ),
        enemy_hp_progress=CombatAxisSignalSummary(
            REPLICATES if enemy_signal else 0,
            DECISIONS if enemy_signal else 0,
        ),
        potion_retention=CombatAxisSignalSummary(0, 0),
    )
    return CombatRootCompetenceEvidence(
        source_slot=source_slot,
        root_id=root_id,
        exact_combat_state_hash=combat_hash,
        replicate_count=REPLICATES,
        wins=wins,
        losses=REPLICATES - wins - unresolved,
        unresolved=unresolved,
        signals=signals,
    )


class _Group:
    def __init__(self, root_id: str, combat_hash: str) -> None:
        self.root_id = root_id
        self.exact_combat_state_hash = combat_hash


class _Source:
    def __init__(self, roots: tuple[CombatRootCompetenceEvidence, ...]) -> None:
        self.roots = roots
        self.calls: list[tuple[int, int]] = []
        self.changed_slot: int | None = None

    def combat_group(self, slot_index: int, replicate_count: int) -> _Group:
        self.calls.append((slot_index, replicate_count))
        root = self.roots[slot_index]
        root_id = "f" * 64 if slot_index == self.changed_slot else root.root_id
        return _Group(root_id, root.exact_combat_state_hash)


class CombatFrontierPlanTests(unittest.TestCase):
    def setUp(self) -> None:
        self.roots = (
            _evidence(0, 0),
            _evidence(1, 2),
            _evidence(2, 4, hp_signal=True),
            _evidence(3, 4),
        )

    def test_hp_objective_partitions_survival_resource_rescue_and_solved(self) -> None:
        plan = build_combat_frontier_plan(
            self.roots,
            CombatWinObjectiveConfig(
                groups_per_update=2,
                all_win_axis=CombatAllWinAxis.TERMINAL_HP,
            ),
            max_roots=4,
        )

        self.assertEqual(plan.survival_frontier_slots, (1,))
        self.assertEqual(plan.resource_frontier_slots, (2,))
        self.assertEqual(plan.training_slots, (1, 2))
        self.assertEqual(
            plan.training_objective_config(),
            CombatWinObjectiveConfig(
                groups_per_update=2,
                all_win_axis=CombatAllWinAxis.TERMINAL_HP,
            ),
        )
        self.assertEqual(plan.rescue_slots, (0,))
        self.assertEqual(plan.solved_slots, (3,))

    def test_win_only_objective_treats_all_win_roots_as_solved(self) -> None:
        plan = build_combat_frontier_plan(
            self.roots,
            CombatWinObjectiveConfig(
                all_win_axis=CombatAllWinAxis.NONE,
            ),
            max_roots=4,
        )

        self.assertEqual(plan.training_slots, (1,))
        self.assertEqual(plan.rescue_slots, (0,))
        self.assertEqual(plan.solved_slots, (2, 3))

    def test_all_loss_axis_admits_exact_losses_but_not_unresolved_roots(self) -> None:
        roots = (
            _evidence(0, 0, enemy_signal=True),
            _evidence(1, 0, enemy_signal=True, unresolved=1),
        )
        objective = CombatWinObjectiveConfig(
            all_loss_axis=CombatAllLossAxis.ENEMY_HP_PROGRESS,
        )

        plan = build_combat_frontier_plan(roots, objective, max_roots=2)

        self.assertEqual(plan.survival_frontier_slots, (0,))
        self.assertEqual(plan.rescue_slots, (1,))
        self.assertEqual(plan.training_slots, (0,))
        self.assertIs(
            plan.training_objective_config().all_loss_axis,
            CombatAllLossAxis.ENEMY_HP_PROGRESS,
        )

    def test_plan_without_frontier_cannot_construct_training_surface(self) -> None:
        roots = (self.roots[0], self.roots[3])
        plan = build_combat_frontier_plan(
            roots,
            CombatWinObjectiveConfig(
                all_win_axis=CombatAllWinAxis.NONE,
            ),
            max_roots=2,
        )

        self.assertEqual(plan.training_slots, ())
        with self.assertRaisesRegex(CombatCurriculumError, "no trainable roots"):
            plan.training_objective_config()
        with self.assertRaisesRegex(CombatCurriculumError, "no trainable roots"):
            CombatFrontierRootSource(_Source(roots), plan)

    def test_frontier_source_cannot_route_rescue_or_solved_roots(self) -> None:
        plan = build_combat_frontier_plan(
            self.roots,
            CombatWinObjectiveConfig(groups_per_update=2),
            max_roots=4,
        )
        source = _Source(self.roots)
        selected = CombatFrontierRootSource(source, plan)

        first = selected.combat_group(0, 16)
        second = selected.combat_group(1, 16)

        self.assertEqual(selected.root_count, 2)
        self.assertEqual(source.calls, [(1, 16), (2, 16)])
        self.assertEqual(first.root_id, self.roots[1].root_id)
        self.assertEqual(second.root_id, self.roots[2].root_id)
        with self.assertRaisesRegex(CombatCurriculumError, "out of range"):
            selected.combat_group(2, 16)

    def test_frontier_source_rechecks_exact_identity(self) -> None:
        plan = build_combat_frontier_plan(
            self.roots,
            CombatWinObjectiveConfig(groups_per_update=2),
            max_roots=4,
        )
        source = _Source(self.roots)
        source.changed_slot = 1
        selected = CombatFrontierRootSource(source, plan)

        with self.assertRaisesRegex(CombatCurriculumError, "changed an exact root"):
            selected.combat_group(0, 16)

    def test_malformed_or_repeated_evidence_fails_closed(self) -> None:
        with self.assertRaisesRegex(CombatCurriculumError, "terminal-HP"):
            _evidence(0, 0, hp_signal=True)
        with self.assertRaisesRegex(CombatCurriculumError, "repeats a source slot"):
            build_combat_frontier_plan(
                (self.roots[0], replace(self.roots[1], source_slot=0)),
                CombatWinObjectiveConfig(),
                max_roots=2,
            )
        with self.assertRaisesRegex(CombatCurriculumError, "max_roots"):
            build_combat_frontier_plan(
                self.roots,
                CombatWinObjectiveConfig(),
                max_roots=3,
            )


if __name__ == "__main__":
    unittest.main()
