from __future__ import annotations

import unittest

from learning.tests.torch_combat_fixtures import (
    COMBAT_HASH,
    ROOT_ID,
    combat_group_experience_fixture,
)
from sts_learning import (
    BehaviorManifestId,
    CombatRecoveryError,
    CombatRecoveryRootSource,
    replay_winning_recovery_roots,
)


class _SpawnedGroup:
    def __init__(self, root_id: str, combat_hash: str, replicate_count: int) -> None:
        self.root_id = root_id
        self.exact_combat_state_hash = combat_hash
        self.replicate_count = replicate_count


class _RecoveryHandle:
    def __init__(self, transition: int) -> None:
        self.root_id = f"{transition + 3:02x}" * 32
        self.exact_combat_state_hash = f"{transition + 9:02x}" * 32
        self.source_root_id = ROOT_ID
        self.source_exact_combat_state_hash = COMBAT_HASH
        self.source_replicate_index = 0

    def spawn_group(
        self,
        replicate_count: int,
        potion_slots: tuple[int, ...] | None = None,
    ) -> _SpawnedGroup:
        if potion_slots not in (None, (0,)):
            raise AssertionError("recovery received an unexpected potion lane")
        return _SpawnedGroup(
            self.root_id,
            self.exact_combat_state_hash,
            replicate_count,
        )


class _TwoTransitionWinningReplay:
    root_id = ROOT_ID
    exact_combat_state_hash = COMBAT_HASH
    replicate_count = 1

    def __init__(self) -> None:
        self.terminal_count = 0
        self.ready = False
        self.transition = 0
        self.chosen: list[int] = []

    def capture_recovery_root(self, replicate_index: int) -> _RecoveryHandle:
        if replicate_index != 0 or self.ready or self.terminal_count:
            raise AssertionError("recovery capture happened outside a root boundary")
        return _RecoveryHandle(self.transition)

    def decision_batch(self, *, semantic: bool) -> dict[str, object]:
        if semantic or self.ready or self.terminal_count:
            raise AssertionError("replay requested the wrong decision surface")
        return {"slot_indices": (0,), "candidate_counts": (1,)}

    def choose(self, ordinals: list[int]) -> None:
        if ordinals != [0] or self.ready:
            raise AssertionError("replay selected a different action")
        self.chosen.append(ordinals[0])
        self.ready = True

    def step(self) -> dict[str, object]:
        if not self.ready:
            raise AssertionError("replay stepped before choosing")
        self.ready = False
        self.transition += 1
        terminal = self.transition == 2
        if terminal:
            self.terminal_count = 1
        return {
            "root_id": ROOT_ID,
            "exact_combat_state_hash": COMBAT_HASH,
            "terminal_slot_indices": (0,) if terminal else (),
            "terminal_kind": (0,) if terminal else (),
            "terminal_won": (True,) if terminal else (),
            "terminal_start_hp": (80,) if terminal else (),
            "terminal_final_hp": (10,) if terminal else (),
            "terminal_final_max_hp": (80,) if terminal else (),
            "terminal_final_gold": (99,) if terminal else (),
            "terminal_hp_loss": (70,) if terminal else (),
            "terminal_enemy_start_hp": (40,) if terminal else (),
            "terminal_enemy_final_hp": (0,) if terminal else (),
            "terminal_turns": (3,) if terminal else (),
            "terminal_potions_used": (1,) if terminal else (),
            "terminal_potions_discarded": (0,) if terminal else (),
            "terminal_cards_played": (8,) if terminal else (),
            "terminal_potion_ids": ((None,),) if terminal else (),
        }


class _ReplaySource:
    def __init__(self) -> None:
        self.groups: list[_TwoTransitionWinningReplay] = []

    def combat_group(
        self,
        slot_index: int,
        replicate_count: int,
    ) -> _TwoTransitionWinningReplay:
        if slot_index != 4 or replicate_count != 1:
            raise AssertionError("replay source received different bounds")
        group = _TwoTransitionWinningReplay()
        self.groups.append(group)
        return group


class CombatRecoveryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.manifest_id = BehaviorManifestId(b"r" * 32)

    def test_exact_winning_replay_keeps_only_typed_terminal_nearest_roots(self) -> None:
        experience = combat_group_experience_fixture(
            self.manifest_id,
            wins=(False, True),
        )
        source = _ReplaySource()

        plan = replay_winning_recovery_roots(
            source,
            slot_index=4,
            experience=experience,
            teacher_replicate_index=1,
            max_roots=2,
        )

        self.assertEqual(source.groups[0].chosen, [0, 0])
        self.assertEqual(plan.transition_count, 2)
        self.assertEqual(plan.teacher_replicate_index, 1)
        self.assertEqual(
            tuple(root.transitions_to_terminal for root in plan.roots),
            (1, 2),
        )
        selected = CombatRecoveryRootSource(plan)
        self.assertEqual(selected.root_count, 2)
        spawned = selected.combat_group(0, 4)
        self.assertEqual(spawned.replicate_count, 4)
        self.assertEqual(spawned.root_id, plan.roots[0].root_id)
        selected.combat_group(0, 4, (0,))

    def test_loss_cannot_be_promoted_to_a_recovery_teacher(self) -> None:
        experience = combat_group_experience_fixture(
            self.manifest_id,
            wins=(False, True),
        )

        with self.assertRaisesRegex(CombatRecoveryError, "observed winning"):
            replay_winning_recovery_roots(
                _ReplaySource(),
                slot_index=4,
                experience=experience,
                teacher_replicate_index=0,
                max_roots=2,
            )


if __name__ == "__main__":
    unittest.main()
