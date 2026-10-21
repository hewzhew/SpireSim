from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

import pytest

pytest.importorskip("torch")

from learning.tests.driver_fixtures import (  # noqa: E402
    NumpyFakeBatchEnv,
    NumpyWinningBatchEnv,
)
from learning.tests.run_training_fixtures import (  # noqa: E402
    published_behavior,
)
from sts_learning.evaluate_run import (  # noqa: E402
    RunEvaluationCommandConfig,
    run_run_evaluation,
)
from sts_learning.run_sampling import RunSamplingMode  # noqa: E402
from sts_learning.terminal_returns import TerminalAdvantageMode  # noqa: E402
from sts_learning.train_run import (  # noqa: E402
    RunTrainingCommandConfig,
    RunTrainingCommandError,
    run_run_training,
)


class _NumpyLosingBatchEnv(NumpyWinningBatchEnv):
    def step(self) -> dict[str, object]:
        active = [
            slot for slot, terminal in enumerate(self.terminal) if not terminal
        ]
        self._terminal_plans.insert(0, {slot: -1 for slot in active})
        return NumpyFakeBatchEnv.step(self)


def _command_config(
    root: Path,
    *,
    slot_count: int = 1,
    advantage_mode: TerminalAdvantageMode = TerminalAdvantageMode.RAW_RETURN,
    sampling_mode: RunSamplingMode = RunSamplingMode.INDEPENDENT_COHORTS,
    episode_root_attempts: int | None = None,
) -> RunTrainingCommandConfig:
    behavior = root / "behavior"
    behavior.mkdir(exist_ok=True)
    return RunTrainingCommandConfig(
        warm_start_behavior=behavior,
        output=root / "output",
        slot_count=slot_count,
        generations=1,
        attempts_per_update=2,
        max_batch_steps_per_generation=10,
        model_seed=1,
        behavior_seed=2,
        training_seed_start=3,
        evaluation_attempts=1,
        evaluation_max_batch_steps=10,
        evaluation_behavior_seed=4,
        held_out_seed_start=5,
        ascension_level=20,
        advantage_mode=advantage_mode,
        sampling_mode=sampling_mode,
        episode_root_attempts=episode_root_attempts,
    )


def test_run_training_binds_episode_credit_and_attempt_cap_to_retries(
    tmp_path: Path,
) -> None:
    paired = TerminalAdvantageMode.MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT

    with pytest.raises(RunTrainingCommandError, match="requires episode-root"):
        _command_config(tmp_path, advantage_mode=paired)
    with pytest.raises(RunTrainingCommandError, match="episode_root_attempts"):
        _command_config(
            tmp_path,
            sampling_mode=RunSamplingMode.EPISODE_ROOT_RETRIES,
        )
    with pytest.raises(RunTrainingCommandError, match="require episode-matched"):
        _command_config(
            tmp_path,
            sampling_mode=RunSamplingMode.EPISODE_ROOT_RETRIES,
            episode_root_attempts=2,
        )
    with pytest.raises(RunTrainingCommandError, match="slot_count=1"):
        _command_config(
            tmp_path,
            slot_count=2,
            advantage_mode=paired,
            sampling_mode=RunSamplingMode.EPISODE_ROOT_RETRIES,
            episode_root_attempts=2,
        )
    with pytest.raises(RunTrainingCommandError, match="require episode-root"):
        _command_config(tmp_path, episode_root_attempts=2)
    with pytest.raises(RunTrainingCommandError, match="at least two"):
        _command_config(
            tmp_path,
            advantage_mode=paired,
            sampling_mode=RunSamplingMode.EPISODE_ROOT_RETRIES,
            episode_root_attempts=1,
        )
    with pytest.raises(RunTrainingCommandError, match="cannot exceed"):
        _command_config(
            tmp_path,
            advantage_mode=paired,
            sampling_mode=RunSamplingMode.EPISODE_ROOT_RETRIES,
            episode_root_attempts=3,
        )

    retry = _command_config(
        tmp_path,
        advantage_mode=paired,
        sampling_mode=RunSamplingMode.EPISODE_ROOT_RETRIES,
        episode_root_attempts=2,
    )
    assert retry.episode_root_attempts == 2


def test_run_training_samples_bounded_pairs_from_multiple_roots(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    populations: list[_NumpyLosingBatchEnv] = []

    def losing_environment(
        seeds: list[int],
        ascension_level: int,
    ) -> _NumpyLosingBatchEnv:
        assert ascension_level == 20
        environment = _NumpyLosingBatchEnv(seeds)
        populations.append(environment)
        return environment

    run_bridge = replace(
        run_bridge,
        environment=losing_environment,
        environment_without_combat_potions=losing_environment,
        environment_from_checkpoint=(
            _NumpyLosingBatchEnv.from_checkpoint_bytes
        ),
    )
    output = tmp_path / "paired-root-training"
    summary = run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=behavior,
            output=output,
            slot_count=1,
            generations=1,
            attempts_per_update=4,
            max_batch_steps_per_generation=16,
            model_seed=43,
            behavior_seed=94,
            training_seed_start=0,
            evaluation_attempts=1,
            evaluation_max_batch_steps=2,
            evaluation_behavior_seed=501,
            held_out_seed_start=1000,
            ascension_level=20,
            advantage_mode=(
                TerminalAdvantageMode.MATCHED_EPISODE_FLOOR_CONTEXT_LEAVE_ONE_OUT
            ),
            sampling_mode=RunSamplingMode.EPISODE_ROOT_RETRIES,
            episode_root_attempts=2,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    records = tuple(
        json.loads(line)
        for line in (output / "training.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    )
    assert records[0]["episode_root_attempts"] == 2
    assert records[1]["terminal_attempts"] == 4
    assert records[1]["sampled_episodes"] == 2
    assert records[1]["recoveries"] == 2
    assert summary["episode_root_attempts"] == 2
    assert populations[0].restore_calls == [[0], [0]]

    reevaluation = run_run_evaluation(
        RunEvaluationCommandConfig(
            behavior=output,
            output=tmp_path / "paired-root-reevaluation",
            slot_count=1,
            terminal_attempts=1,
            max_batch_steps=2,
            behavior_seed=777,
            ascension_level=20,
            held_out_seed_start=2000,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )
    assert reevaluation["behavior_run_sampling_mode"] == "episode-root-retries"
    assert reevaluation["behavior_run_episode_root_attempts"] == 2
