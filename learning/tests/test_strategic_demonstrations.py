from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import numpy as np
import pytest

pytest.importorskip("torch")

from learning.tests.driver_fixtures import FakeBatchEnv, NumpyWinningBatchEnv
from learning.tests.run_training_fixtures import published_behavior
from sts_learning.strategic_demonstrations import (
    CombatAnchorMode,
    CombatRetryCoverageConfig,
    StrategicDemonstrationConfig,
    collect_strategic_demonstrations,
)
from sts_learning.torch_session_config import CategoricalSessionBridge
from sts_learning_bridge import PHASE_COMBAT_ROOT, PHASE_STRATEGIC_ROOT


class _ProductionStrategicWinningEnv(NumpyWinningBatchEnv):
    def decision_batch(
        self,
        *,
        semantic: bool = False,
        legality_bootstrap: bool = False,
    ) -> dict[str, object]:
        assert semantic
        assert legality_bootstrap
        batch = super().decision_batch(semantic=True)
        slots = batch["slot_indices"]
        assert isinstance(slots, np.ndarray)
        batch["phase"] = np.full(
            slots.size,
            PHASE_STRATEGIC_ROOT,
            dtype=np.uint8,
        )
        batch["legality_bootstrap_available"] = np.ones(
            slots.size,
            dtype=np.bool_,
        )
        batch["legality_bootstrap_ordinals"] = np.ones(
            slots.size,
            dtype=np.uint64,
        )
        return batch

    def public_run_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        rows = super().public_run_contexts()
        for _, view in rows:
            view.is_combat = False
            view.strategic_context_kind = 7
        return rows

    def step(self) -> dict[str, object]:
        step = super().step()
        for key in (
            "terminal_slot_indices",
            "terminal_reward",
            "terminal_act",
            "terminal_floor",
            "terminal_hp",
            "terminal_max_hp",
        ):
            step[key] = np.asarray(step[key], dtype=np.int64)
        return step


class _RetryCombatEnv(NumpyWinningBatchEnv):
    public_snapshot_phase = PHASE_COMBAT_ROOT

    def __init__(self, seeds: list[int]) -> None:
        super().__init__(seeds)
        self.losses_remaining = 1

    def decision_batch(
        self,
        *,
        semantic: bool = False,
        legality_bootstrap: bool = False,
    ) -> dict[str, object]:
        assert semantic
        assert legality_bootstrap
        batch = super().decision_batch(semantic=True)
        slots = batch["slot_indices"]
        assert isinstance(slots, np.ndarray)
        batch["phase"] = np.full(
            slots.size,
            PHASE_COMBAT_ROOT,
            dtype=np.uint8,
        )
        batch["legality_bootstrap_available"] = np.zeros(
            slots.size,
            dtype=np.bool_,
        )
        batch["legality_bootstrap_ordinals"] = np.zeros(
            slots.size,
            dtype=np.uint64,
        )
        return batch

    def step(self) -> dict[str, object]:
        active = [slot for slot, terminal in enumerate(self.terminal) if not terminal]
        reward = -1 if self.losses_remaining else 1
        self.losses_remaining = max(0, self.losses_remaining - 1)
        self._terminal_plans.insert(0, {slot: reward for slot in active})
        step = FakeBatchEnv.step(self)
        for key in (
            "terminal_slot_indices",
            "terminal_reward",
            "terminal_act",
            "terminal_floor",
            "terminal_hp",
            "terminal_max_hp",
        ):
            step[key] = np.asarray(step[key], dtype=np.int64)
        return step


def test_collector_retains_only_same_frame_production_teacher_rows(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, base_run_bridge = published_behavior(tmp_path)
    environments: list[_ProductionStrategicWinningEnv] = []

    def environment(seeds: list[int], ascension_level: int):
        assert ascension_level == 20
        created = _ProductionStrategicWinningEnv(seeds)
        environments.append(created)
        return created

    run_bridge = CategoricalSessionBridge(
        environment=environment,
        environment_without_combat_potions=environment,
        environment_from_checkpoint=base_run_bridge.environment_from_checkpoint,
        checkpoint_bank_from_checkpoint=base_run_bridge.checkpoint_bank_from_checkpoint,
        semantic_schema=base_run_bridge.semantic_schema,
    )
    corpus = collect_strategic_demonstrations(
        StrategicDemonstrationConfig(
            behavior=behavior,
            ascension_level=20,
            training_seed_start=40_000,
            run_count=2,
            slot_count=2,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert corpus.completed_runs == 2
    assert corpus.victories == 2
    assert corpus.defeats == 0
    assert corpus.teacher_rows == 4
    assert corpus.context_counts == {7: 4}
    assert corpus.combat_rows == 0
    assert corpus.strategic_selection_rows == 0
    assert corpus.unavailable_strategic_root_rows == 0
    assert corpus.combat_anchor_mode is CombatAnchorMode.STRICT_PUBLICATION
    assert corpus.combat_anchor_provenance_mismatches == ()
    assert corpus.terminal_episode_seeds == (40_000, 40_001)
    assert corpus.terminal_rewards == (1, 1)
    assert corpus.terminal_floor_counts == {(3, 40): 2}
    assert corpus.combat_retries == 0
    assert corpus.rescued_combats == 0
    assert corpus.terminal_combat_retries == (0, 0)
    assert len(environments) == 1
    assert environments[0].choose_calls == [[1, 1], [1, 1]]


def test_combat_retry_coverage_restores_only_the_combat_root(tmp_path: Path) -> None:
    behavior, combat_bridge, base_run_bridge = published_behavior(tmp_path)
    environments: list[_RetryCombatEnv] = []

    def environment(seeds: list[int], ascension_level: int):
        created = _RetryCombatEnv(seeds)
        environments.append(created)
        return created

    run_bridge = CategoricalSessionBridge(
        environment=environment,
        environment_without_combat_potions=environment,
        environment_from_checkpoint=base_run_bridge.environment_from_checkpoint,
        checkpoint_bank_from_checkpoint=base_run_bridge.checkpoint_bank_from_checkpoint,
        semantic_schema=base_run_bridge.semantic_schema,
    )
    corpus = collect_strategic_demonstrations(
        StrategicDemonstrationConfig(
            behavior=behavior,
            ascension_level=20,
            training_seed_start=50_000,
            run_count=1,
            slot_count=1,
            combat_retry_coverage=CombatRetryCoverageConfig(
                max_retries_per_combat=1,
                sampling_seed=17,
            ),
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert corpus.completed_runs == 1
    assert corpus.victories == 1
    assert corpus.defeats == 0
    assert corpus.combat_retries == 1
    assert corpus.rescued_combats == 1
    assert corpus.terminal_combat_retries == (1,)
    assert corpus.terminal_episode_seeds == (50_000,)
    assert len(environments) == 1
    assert environments[0].restore_calls == [[0]]
