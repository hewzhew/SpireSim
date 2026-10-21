from __future__ import annotations

from dataclasses import replace

import pytest

from learning.tests.torch_combat_fixtures import combat_group_experience_fixture
from sts_learning import (
    BehaviorManifestId,
    CombatRolloutError,
    CombatRolloutAxis,
    CombatTerminalKind,
    build_complete_combat_rollout,
)


MANIFEST = BehaviorManifestId(b"r" * 32)


def test_rollout_attributes_adjacent_changes_and_future_returns() -> None:
    experience = combat_group_experience_fixture(MANIFEST, wins=(True, True))

    rollout = build_complete_combat_rollout(experience)
    first, second = rollout.replicates

    assert rollout.decision_count == 3
    assert tuple(batch.replicate_indices for batch in rollout.batches) == (
        (0, 1),
        (1,),
    )
    assert rollout.batches[0].returns_to_go(CombatRolloutAxis.WIN) == (1.0, 1.0)
    assert rollout.batches[1].returns_to_go(
        CombatRolloutAxis.PLAYER_HP_CHANGE
    ) == pytest.approx((-65 / 80,))
    assert tuple(row.sequence_index for row in first.rows) == (0,)
    assert tuple(row.sequence_index for row in second.rows) == (0, 1)
    assert first.rows[0].player_hp_change_reward == pytest.approx(-10 / 80)
    assert first.rows[0].enemy_hp_change_reward == pytest.approx(1.0)
    assert second.rows[0].player_hp_change_reward == pytest.approx(-5 / 80)
    assert second.rows[1].player_hp_change_reward == pytest.approx(-65 / 80)
    assert second.rows[0].player_hp_change_return_to_go == pytest.approx(-70 / 80)
    assert second.rows[1].player_hp_change_return_to_go == pytest.approx(-65 / 80)
    assert second.rows[0].enemy_hp_change_reward == pytest.approx(15 / 40)
    assert second.rows[1].enemy_hp_change_reward == pytest.approx(25 / 40)
    assert second.rows[0].enemy_hp_change_return_to_go == pytest.approx(1.0)
    assert tuple(row.win_reward for row in second.rows) == (0.0, 1.0)
    assert tuple(row.win_return_to_go for row in second.rows) == (1.0, 1.0)


def test_rollout_keeps_potion_identity_and_marks_terminal_uuid_unknown() -> None:
    experience = combat_group_experience_fixture(
        MANIFEST,
        wins=(True, False),
        potions_used=(0, 1),
    )

    second = build_complete_combat_rollout(experience).replicates[1]

    assert second.rows[0].after_potion_uuids == (101, 102)
    assert second.rows[0].after_potion_ids == ("FearPotion", "GamblersBrew")
    terminal = second.rows[-1]
    assert terminal.terminal
    assert terminal.terminal_kind is CombatTerminalKind.LOSS
    assert terminal.after_potion_uuids is None
    assert terminal.after_potion_ids == (None, "GamblersBrew")
    assert all(row.win_return_to_go == 0.0 for row in second.rows)


def test_unchanged_selection_prefix_receives_zero_state_change() -> None:
    experience = combat_group_experience_fixture(MANIFEST, wins=(True, True))
    first, second = experience.batches
    second = replace(
        second,
        decision_progress=(first.decision_progress[1],),
    )
    experience = replace(experience, batches=(first, second))

    second_rollout = build_complete_combat_rollout(experience).replicates[1]

    prefix = second_rollout.rows[0]
    assert prefix.player_hp_change_reward == 0.0
    assert prefix.enemy_hp_change_reward == 0.0
    assert prefix.progress.potion_uuids == prefix.after_potion_uuids
    assert prefix.progress.potion_ids == prefix.after_potion_ids


def test_rollout_uses_root_public_max_hp_not_original_combat_start_hp() -> None:
    experience = combat_group_experience_fixture(
        MANIFEST,
        wins=(True, True),
        final_hps=(40, 30),
    )
    first_batch, second_batch = experience.batches
    first_batch = replace(
        first_batch,
        decision_progress=(
            replace(first_batch.decision_progress[0], player_hp=50),
            replace(first_batch.decision_progress[1], player_hp=50),
        ),
    )
    second_batch = replace(
        second_batch,
        decision_progress=(
            replace(second_batch.decision_progress[0], player_hp=45),
        ),
    )
    experience = replace(experience, batches=(first_batch, second_batch))

    rollout = build_complete_combat_rollout(experience)

    assert rollout.replicates[0].rows[0].player_hp_change_return_to_go == pytest.approx(
        -10 / 80
    )
    assert rollout.replicates[1].rows[0].player_hp_change_return_to_go == pytest.approx(
        -20 / 80
    )


def test_rollout_rejects_root_enemy_hp_mismatch() -> None:
    experience = combat_group_experience_fixture(MANIFEST, wins=(True, False))
    first, second = experience.batches
    first = replace(
        first,
        decision_progress=(
            replace(first.decision_progress[0], enemy_hp=39),
            first.decision_progress[1],
        ),
    )
    experience = replace(experience, batches=(first, second))

    with pytest.raises(CombatRolloutError, match="enemy HP disagrees"):
        build_complete_combat_rollout(experience)
