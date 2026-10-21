from __future__ import annotations

from dataclasses import replace

import numpy as np
import pytest

from sts_learning import (
    CombatGroupedAdvantages,
    CombatGroupOutcomeAccumulator,
    CombatOutcomeError,
    CombatTerminalKind,
    CombatTerminalOutcome,
    CombatTerminalStepBatch,
    CompletedCombatGroup,
)
ROOT_ID = "12" * 32
COMBAT_HASH = "ab" * 32
WIN_KIND = 0
LOSS_KIND = 1


def _outcome(
    replicate: int,
    *,
    kind: int = WIN_KIND,
    final_hp: int = 80,
    potions_used: int = 0,
    enemy_final_hp: int | None = None,
) -> CombatTerminalOutcome:
    if enemy_final_hp is None:
        enemy_final_hp = 0 if kind == WIN_KIND else 20
    return CombatTerminalOutcome(
        replicate_index=replicate,
        terminal_kind=kind,
        won=kind == WIN_KIND,
        start_hp=80,
        final_hp=final_hp,
        final_max_hp=80,
        final_gold=99,
        hp_loss=max(80 - final_hp, 0),
        enemy_start_hp=40,
        enemy_final_hp=enemy_final_hp,
        turns=3,
        potions_used=potions_used,
        potions_discarded=0,
        cards_played=8,
        final_potion_ids=("FearPotion",),
    )


def _bridge_step(replicates: list[int]) -> dict[str, object]:
    rows = len(replicates)
    return {
        "root_id": ROOT_ID,
        "exact_combat_state_hash": COMBAT_HASH,
        "terminal_slot_indices": np.asarray(replicates, dtype=np.uint64),
        "terminal_kind": np.full(rows, WIN_KIND, dtype=np.uint8),
        "terminal_won": np.ones(rows, dtype=np.bool_),
        "terminal_start_hp": np.full(rows, 80, dtype=np.int32),
        "terminal_final_hp": np.asarray(
            [80 - 10 * replicate for replicate in replicates], dtype=np.int32
        ),
        "terminal_final_max_hp": np.full(rows, 80, dtype=np.int32),
        "terminal_final_gold": np.full(rows, 99, dtype=np.int32),
        "terminal_hp_loss": np.asarray(
            [10 * replicate for replicate in replicates], dtype=np.int32
        ),
        "terminal_enemy_start_hp": np.full(rows, 40, dtype=np.int32),
        "terminal_enemy_final_hp": np.zeros(rows, dtype=np.int32),
        "terminal_turns": np.full(rows, 3, dtype=np.uint32),
        "terminal_potions_used": np.zeros(rows, dtype=np.uint32),
        "terminal_potions_discarded": np.zeros(rows, dtype=np.uint32),
        "terminal_cards_played": np.full(rows, 8, dtype=np.uint32),
        "terminal_potion_ids": tuple(("FearPotion",) for _ in range(rows)),
    }


def test_bridge_terminal_columns_copy_into_typed_rows() -> None:
    batch = CombatTerminalStepBatch.from_bridge_step(
        _bridge_step([0, 2]),
        replicate_count=3,
    )

    assert tuple(row.replicate_index for row in batch.outcomes) == (0, 2)
    assert tuple(row.final_hp for row in batch.outcomes) == (80, 60)
    assert all(row.won for row in batch.outcomes)
    assert all(
        row.terminal_kind is CombatTerminalKind.WIN for row in batch.outcomes
    )


def test_terminal_kind_is_closed_and_must_agree_with_win_fact() -> None:
    with pytest.raises(CombatOutcomeError, match="unsupported"):
        replace(_outcome(0), terminal_kind=9)
    with pytest.raises(CombatOutcomeError, match="disagrees"):
        replace(_outcome(0), terminal_kind=CombatTerminalKind.LOSS)


def test_accumulator_rejects_wrong_identity_and_duplicate_atomically() -> None:
    accumulator = CombatGroupOutcomeAccumulator(
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        replicate_count=3,
    )
    accumulator.record(
        CombatTerminalStepBatch(ROOT_ID, COMBAT_HASH, (_outcome(0),))
    )

    invalid_batches = (
        (
            "different root id",
            CombatTerminalStepBatch("34" * 32, COMBAT_HASH, (_outcome(1),)),
        ),
        (
            "different exact state",
            CombatTerminalStepBatch(ROOT_ID, "cd" * 32, (_outcome(1),)),
        ),
        (
            "duplicate after a new row",
            CombatTerminalStepBatch(
                ROOT_ID,
                COMBAT_HASH,
                (_outcome(1), _outcome(0)),
            ),
        ),
    )
    for label, batch in invalid_batches:
        with pytest.raises(CombatOutcomeError):
            accumulator.record(batch)
        assert accumulator.terminal_count == 1, label

    with pytest.raises(CombatOutcomeError):
        accumulator.finish()


def test_grouped_axes_remain_independent_and_sibling_relative() -> None:
    all_wins = CompletedCombatGroup(
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        outcomes=(
            _outcome(0, final_hp=80),
            _outcome(1, final_hp=60, potions_used=1),
            _outcome(2, final_hp=40),
        ),
    ).grouped_advantages()

    assert all_wins.win == (0.0, 0.0, 0.0)
    assert np.allclose(all_wins.terminal_hp, (0.375, 0.0, -0.375))
    assert all_wins.enemy_hp_progress == (0.0, 0.0, 0.0)
    assert all_wins.potion_retention == (0.5, -1.0, 0.5)
    assert not all_wins.win_has_signal
    assert all_wins.terminal_hp_has_signal
    assert all_wins.potion_retention_has_signal

    mixed_result = CompletedCombatGroup(
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        outcomes=(
            _outcome(0, kind=WIN_KIND),
            _outcome(1, kind=LOSS_KIND, final_hp=0),
        ),
    ).grouped_advantages()

    assert mixed_result.win == (1.0, -1.0)
    assert mixed_result.win_has_signal

    all_losses = CompletedCombatGroup(
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        outcomes=(
            _outcome(0, kind=LOSS_KIND, final_hp=0, enemy_final_hp=30),
            _outcome(1, kind=LOSS_KIND, final_hp=0, enemy_final_hp=10),
        ),
    ).grouped_advantages()
    assert all_losses.enemy_hp_progress == (-0.5, 0.5)
    assert all_losses.enemy_hp_progress_has_signal


def test_same_root_group_rejects_mismatched_start_hp() -> None:
    second = CombatTerminalOutcome(
        replicate_index=1,
        terminal_kind=WIN_KIND,
        won=True,
        start_hp=79,
        final_hp=79,
        final_max_hp=80,
        final_gold=99,
        hp_loss=0,
        enemy_start_hp=40,
        enemy_final_hp=0,
        turns=3,
        potions_used=0,
        potions_discarded=0,
        cards_played=8,
        final_potion_ids=("FearPotion",),
    )

    with pytest.raises(CombatOutcomeError):
        CompletedCombatGroup(
            root_id=ROOT_ID,
            exact_combat_state_hash=COMBAT_HASH,
            outcomes=(_outcome(0), second),
        )


def test_grouped_signal_ignores_floating_point_residue() -> None:
    advantages = CombatGroupedAdvantages(
        win=(1.0e-16, -1.0e-16),
        terminal_hp=(0.0, 0.0),
        enemy_hp_progress=(0.0, 0.0),
        potion_retention=(0.0, 0.0),
    )

    assert not advantages.win_has_signal
