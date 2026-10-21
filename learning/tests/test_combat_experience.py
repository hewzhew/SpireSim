from __future__ import annotations

import numpy as np
import pytest

from sts_learning import (
    BatchPolicyChoice,
    BehaviorManifestId,
    BoundedCombatGroupExperience,
    CombatExperienceError,
    CombatExperienceLimits,
    CombatGroupDriver,
    CombatDecisionProgress,
    SelectionProbability,
)

from learning.tests.torch_combat_fixtures import combat_group_experience_fixture


ROOT_ID = "12" * 32
COMBAT_HASH = "ab" * 32
MANIFEST = BehaviorManifestId(b"\x01" * 32)


def _decision_batch() -> dict[str, object]:
    return {
        "slot_indices": np.asarray([0, 1], dtype=np.uint64),
        "phase": np.asarray([1, 1], dtype=np.uint8),
        "candidate_counts": np.asarray([2, 2], dtype=np.uint64),
        "candidate_row_splits": np.asarray([0, 2, 4], dtype=np.uint64),
        "semantic": {"fixture": np.asarray([10, 20], dtype=np.int32)},
    }


def _decision_progress(
    replicate_indices: tuple[int, ...] = (0, 1),
) -> tuple[CombatDecisionProgress, ...]:
    return tuple(
        CombatDecisionProgress(
            replicate_index=replicate_index,
            turn=1,
            player_hp=80,
            player_max_hp=80,
            enemy_hp=40,
            enemy_max_hp=40,
            potion_uuids=(100 + replicate_index,),
            potion_ids=("FearPotion",),
        )
        for replicate_index in replicate_indices
    )


class _Policy:
    def choose(self, decision_batch) -> BatchPolicyChoice:
        assert tuple(decision_batch["candidate_counts"]) == (2, 2)
        return BatchPolicyChoice.create(
            (0, 1),
            MANIFEST,
            (SelectionProbability.known(0.5),) * 2,
        )


class _OneStepCombatGroup:
    root_id = ROOT_ID
    exact_combat_state_hash = COMBAT_HASH
    replicate_count = 2

    def __init__(self) -> None:
        self.terminal_count = 0
        self.ready = False
        self.choose_calls = 0

    def decision_batch(self, *, semantic: bool) -> dict[str, object]:
        assert semantic
        return _decision_batch()

    def decision_progress(self) -> tuple[CombatDecisionProgress, ...]:
        return _decision_progress()

    def choose(self, ordinals: list[int]) -> None:
        assert ordinals == [0, 1]
        self.choose_calls += 1
        self.ready = True

    def step(self) -> dict[str, object]:
        assert self.ready
        self.terminal_count = 2
        return {
            "root_id": ROOT_ID,
            "exact_combat_state_hash": COMBAT_HASH,
            "terminal_slot_indices": np.asarray([0, 1], dtype=np.uint64),
            "terminal_kind": np.asarray([0, 1], dtype=np.uint8),
            "terminal_won": np.asarray([True, False], dtype=np.bool_),
            "terminal_start_hp": np.asarray([80, 80], dtype=np.int32),
            "terminal_final_hp": np.asarray([70, 0], dtype=np.int32),
            "terminal_final_max_hp": np.asarray([80, 80], dtype=np.int32),
            "terminal_final_gold": np.asarray([99, 99], dtype=np.int32),
            "terminal_hp_loss": np.asarray([10, 80], dtype=np.int32),
            "terminal_enemy_start_hp": np.asarray([40, 40], dtype=np.int32),
            "terminal_enemy_final_hp": np.asarray([0, 20], dtype=np.int32),
            "terminal_turns": np.asarray([3, 5], dtype=np.uint32),
            "terminal_potions_used": np.asarray([0, 1], dtype=np.uint32),
            "terminal_potions_discarded": np.asarray([0, 0], dtype=np.uint32),
            "terminal_cards_played": np.asarray([8, 12], dtype=np.uint32),
            "terminal_potion_ids": (
                ("FearPotion",),
                (None,),
            ),
        }


def _limits(*, payload_bytes: int = 1_000_000) -> CombatExperienceLimits:
    return CombatExperienceLimits(
        max_decisions=16,
        max_payload_bytes=payload_bytes,
        max_model_rounds=8,
        max_transitions=8,
    )


def test_driver_returns_complete_aligned_group_experience() -> None:
    env = _OneStepCombatGroup()

    result = CombatGroupDriver(env, _Policy(), _limits()).run()

    assert env.choose_calls == 1
    assert result.model_rounds == 1
    assert result.transitions == 1
    assert result.experience.root_id == ROOT_ID
    assert result.experience.behavior_manifest_id == MANIFEST
    assert result.experience.decision_count == 2
    assert result.experience.batches[0].replicate_indices == (0, 1)
    assert result.experience.batches[0].decision_progress == _decision_progress()
    assert result.experience.batches[0].selected_ordinals == (0, 1)
    assert result.experience.grouped_advantages().win == (1.0, -1.0)
    projected = result.experience.decision_advantages()
    assert len(projected) == 1
    assert projected[0].replicate_indices == (0, 1)
    assert projected[0].win == (1.0, -1.0)
    assert projected[0].terminal_hp == (0.875, -0.875)
    assert projected[0].enemy_hp_progress == (0.5, -0.5)
    assert projected[0].potion_retention == (1.0, -1.0)
    summary = result.experience.signal_summary()
    assert summary.replicate_count == 2
    assert summary.decision_count == 2
    assert summary.win.replicate_count == 2
    assert summary.win.decision_count == 2
    assert summary.terminal_hp.replicate_count == 2
    assert summary.enemy_hp_progress.replicate_count == 2
    assert summary.potion_retention.replicate_count == 2


def test_payload_limit_rejects_before_environment_mutation() -> None:
    env = _OneStepCombatGroup()

    with pytest.raises(CombatExperienceError):
        CombatGroupDriver(env, _Policy(), _limits(payload_bytes=1)).run()

    assert env.choose_calls == 0


def test_group_rejects_mixed_behavior_manifests_before_second_commit() -> None:
    collector = BoundedCombatGroupExperience(
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        replicate_count=2,
        limits=_limits(),
    )
    prepared = collector.prepare(_decision_batch())
    first = collector.bind_choice(
        prepared,
        _Policy().choose(prepared.payload),
        _decision_progress(),
    )
    collector.commit(first)
    foreign_choice = BatchPolicyChoice.create(
        (0, 1),
        BehaviorManifestId(b"\x02" * 32),
        (SelectionProbability.known(0.5),) * 2,
    )

    with pytest.raises(CombatExperienceError):
        collector.bind_choice(prepared, foreign_choice, _decision_progress())

    assert collector.decision_count == 2


def test_progress_must_match_decision_row_order_before_commit() -> None:
    collector = BoundedCombatGroupExperience(
        root_id=ROOT_ID,
        exact_combat_state_hash=COMBAT_HASH,
        replicate_count=2,
        limits=_limits(),
    )
    prepared = collector.prepare(_decision_batch())

    with pytest.raises(CombatExperienceError, match="progress order"):
        collector.bind_choice(
            prepared,
            _Policy().choose(prepared.payload),
            tuple(reversed(_decision_progress())),
        )

    assert collector.decision_count == 0


def test_completed_group_reconstructs_each_replicate_chronologically() -> None:
    experience = combat_group_experience_fixture(MANIFEST, wins=(True, True))

    first = experience.chronological_steps(0)
    second = experience.chronological_steps(1)

    assert tuple(step.sequence_index for step in first) == (0,)
    assert tuple(step.sequence_index for step in second) == (0, 1)
    assert tuple(step.progress.turn for step in second) == (1, 2)
    assert tuple(step.progress.player_hp for step in second) == (80, 75)
    assert tuple(step.selected_ordinal for step in second) == (0, 0)
    assert all(
        isinstance(step.selection_probability, SelectionProbability)
        for step in second
    )
