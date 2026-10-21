from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace

import pytest

pytest.importorskip("torch")

from learning.tests.driver_fixtures import (  # noqa: E402
    NumpyFakeBatchEnv,
    NumpyWinningBatchEnv,
)
from learning.tests.run_training_fixtures import published_behavior  # noqa: E402
from sts_learning import RunDecisionScope  # noqa: E402
from sts_learning.evaluate_run import (  # noqa: E402
    RunEvaluationCommandConfig,
    run_run_evaluation,
)
from sts_learning.published_run_behavior import (  # noqa: E402
    recover_published_run_behavior,
)
from sts_learning.torch_behavior import (  # noqa: E402
    FrozenCombatGreedyTorchPolicy,
    FrozenDecisionRule,
)
from sts_learning.train_run import (  # noqa: E402
    RunTrainingCommandConfig,
    RunTrainingCommandError,
    run_run_training,
)


class _CombatThenStrategicWinningEnv(NumpyWinningBatchEnv):
    """One combat transition followed by one strategic terminal decision."""

    def __init__(self, seeds: list[int]) -> None:
        super().__init__(seeds)
        self._strategic = [False] * len(seeds)

    def step(self) -> dict[str, object]:
        active = [slot for slot, terminal in enumerate(self.terminal) if not terminal]
        if all(not self._strategic[slot] for slot in active):
            self._terminal_plans.insert(0, {})
            result = NumpyFakeBatchEnv.step(self)
            for slot in active:
                self._strategic[slot] = True
            return result
        result = super().step()
        for slot in active:
            self._strategic[slot] = False
        return result

    def reset_slots(self, slot_indices: list[int], seeds: list[int]) -> None:
        super().reset_slots(slot_indices, seeds)
        for slot in slot_indices:
            self._strategic[slot] = False

    def public_run_contexts(self) -> list[tuple[int, SimpleNamespace]]:
        rows = []
        for slot, terminal in enumerate(self.terminal):
            strategic = self._strategic[slot] and not terminal
            rows.append(
                (
                    slot,
                    SimpleNamespace(
                        boundary_kind=2 if terminal else (3 if strategic else 1),
                        is_combat=not terminal and not strategic,
                        is_terminal=terminal,
                        strategic_context_kind=2 if strategic else None,
                        seed=self.seeds[slot],
                        act=1,
                        floor=2,
                        hp=20 if terminal else 80,
                        max_hp=80,
                        gold=50,
                        potion_ids=[],
                        encounter_id=(
                            "JawWorm" if not terminal and not strategic else None
                        ),
                        monster_ids=(
                            ["JawWorm"] if not terminal and not strategic else []
                        ),
                    ),
                )
            )
        return rows


def test_run_training_uses_greedy_combat_and_trains_only_strategic_rows(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)

    def environment(
        seeds: list[int],
        ascension_level: int,
    ) -> _CombatThenStrategicWinningEnv:
        assert ascension_level == 20
        return _CombatThenStrategicWinningEnv(seeds)

    run_bridge = replace(
        run_bridge,
        environment=environment,
        environment_without_combat_potions=environment,
        environment_from_checkpoint=(
            _CombatThenStrategicWinningEnv.from_checkpoint_bytes
        ),
    )
    with pytest.raises(
        RunTrainingCommandError,
        match="requires strategic decision scope",
    ):
        RunTrainingCommandConfig(
            warm_start_behavior=behavior,
            output=tmp_path / "invalid-mixed-run-training",
            slot_count=1,
            generations=1,
            attempts_per_update=1,
            max_batch_steps_per_generation=2,
            model_seed=43,
            behavior_seed=94,
            training_seed_start=0,
            evaluation_attempts=1,
            evaluation_max_batch_steps=2,
            evaluation_behavior_seed=501,
            held_out_seed_start=1000,
            ascension_level=20,
            combat_decision_rule=FrozenDecisionRule.GREEDY,
        )

    output = tmp_path / "mixed-run-training"
    summary = run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=behavior,
            output=output,
            slot_count=1,
            generations=1,
            attempts_per_update=1,
            max_batch_steps_per_generation=2,
            model_seed=43,
            behavior_seed=94,
            training_seed_start=0,
            evaluation_attempts=1,
            evaluation_max_batch_steps=2,
            evaluation_behavior_seed=501,
            held_out_seed_start=1000,
            ascension_level=20,
            decision_scope=RunDecisionScope.STRATEGIC,
            combat_decision_rule=FrozenDecisionRule.GREEDY,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert summary["combat_decision_rule"] == "greedy"
    assert summary["combat_anchor_manifest_id"] is not None
    assert summary["combat_anchor_checkpoint_id"] is not None
    assert summary["combat_anchor_scorer"] == {
        "hidden_dim": 64,
        "relation_layers": 2,
        "value_head": False,
        "value_head_width": 1,
    }
    assert summary["held_out_target_reached"] is True
    records = tuple(
        json.loads(line)
        for line in (output / "training.jsonl").read_text(
            encoding="utf-8"
        ).splitlines()
    )
    assert records[0]["decision_scope"] == "strategic"
    assert records[0]["combat_decision_rule"] == "greedy"
    assert records[0]["device_type"] == "cpu"
    assert records[0]["schema"] == "sts-learning-run-training-v6"
    assert records[0]["combat_anchor_manifest_id"] == (
        summary["combat_anchor_manifest_id"]
    )
    assert records[-1]["combat_anchor_checkpoint_id"] == (
        summary["combat_anchor_checkpoint_id"]
    )
    recovered = recover_published_run_behavior(output, run_bridge, (777,))
    assert recovered.training_device_type == "cpu"
    assert recovered.training_combat_decision_rule is FrozenDecisionRule.GREEDY
    assert isinstance(recovered.policies[0], FrozenCombatGreedyTorchPolicy)
    assert recovered.combat_anchor_manifest_id is not None
    assert recovered.combat_anchor_checkpoint_id is not None
    assert recovered.policies[0].combat_anchor is not None
    assert recovered.policies[0].combat_anchor.manifest_id == (
        recovered.combat_anchor_manifest_id
    )
    evaluation = run_run_evaluation(
        RunEvaluationCommandConfig(
            behavior=output,
            output=tmp_path / "mixed-run-reevaluation",
            slot_count=1,
            terminal_attempts=1,
            max_batch_steps=2,
            behavior_seed=778,
            ascension_level=20,
            held_out_seed_start=2000,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )
    assert evaluation["behavior_run_combat_anchor_manifest_id"] == (
        recovered.combat_anchor_manifest_id.digest.hex()
    )
    assert evaluation["behavior_run_combat_anchor_checkpoint_id"] == (
        recovered.combat_anchor_checkpoint_id.digest.hex()
    )
    assert evaluation["behavior_run_combat_anchor_scorer"] == (
        summary["combat_anchor_scorer"]
    )
