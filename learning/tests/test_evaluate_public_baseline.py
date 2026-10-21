from __future__ import annotations

from pathlib import Path

import numpy as np

from sts_learning.evaluate_public_baseline import (
    PublicBaselineEvaluationConfig,
    run_public_baseline_evaluation,
)

from learning.tests.driver_fixtures import NumpyWinningBatchEnv


class _BaselineWinningEnv(NumpyWinningBatchEnv):
    def __init__(self, seeds: list[int], ascension_level: int) -> None:
        assert ascension_level == 0
        super().__init__(seeds)

    def decision_batch(
        self, *, legality_bootstrap: bool = False
    ) -> dict[str, object]:
        assert legality_bootstrap
        batch = super().decision_batch()
        slots = batch["slot_indices"]
        assert isinstance(slots, np.ndarray)
        batch["legality_bootstrap_available"] = np.ones(slots.size, dtype=np.bool_)
        batch["legality_bootstrap_ordinals"] = np.zeros(slots.size, dtype=np.uint64)
        return batch

    def step(self) -> dict[str, object]:
        step = super().step()
        for key in (
            "terminal_slot_indices",
            "terminal_reward",
            "terminal_act",
            "terminal_floor",
            "terminal_hp",
            "terminal_max_hp",
            "terminal_gold",
        ):
            step[key] = np.asarray(step[key], dtype=np.int64)
        return step


def test_public_baseline_panel_runs_complete_deterministic_episodes(tmp_path: Path) -> None:
    output = tmp_path / "baseline"
    summary = run_public_baseline_evaluation(
        PublicBaselineEvaluationConfig(
            output=output,
            ascension_level=0,
            seed_start=10,
            attempts=3,
            slot_count=2,
        ),
        environment_factory=_BaselineWinningEnv,
    )

    assert summary["baseline_name"] == "public_legality_bootstrap"
    assert summary["baseline_revision"] == 0
    assert summary["baseline_manifest"] is None
    assert summary["victories"] == 3
    assert summary["defeats"] == 0
    assert summary["censored"] == 0
    assert [row["seed"] for row in summary["outcomes"]] == [10, 11, 12]
    assert (output / "evaluation.json").is_file()


def test_step_cap_retains_every_seed_without_counting_censoring_as_defeat(tmp_path: Path) -> None:
    class NeverTerminal(_BaselineWinningEnv):
        def step(self) -> dict[str, object]:
            return {
                name: np.asarray([], dtype=np.int64)
                for name in (
                    "terminal_slot_indices", "terminal_reward", "terminal_act",
                    "terminal_floor", "terminal_hp", "terminal_max_hp", "terminal_gold",
                )
            }

    summary = run_public_baseline_evaluation(
        PublicBaselineEvaluationConfig(
            output=tmp_path / "censored", ascension_level=0, seed_start=10,
            attempts=3, slot_count=2, max_batch_steps_per_cohort=1,
        ),
        environment_factory=NeverTerminal,
    )
    assert summary["completed"] == 0
    assert summary["censored"] == 3
    assert summary["defeats"] == 0
    assert summary["floor_min"] is None
    assert [row["seed"] for row in summary["outcomes"]] == [10, 11, 12]
    assert all(row["censored_reason"] == "step_limit" for row in summary["outcomes"])
