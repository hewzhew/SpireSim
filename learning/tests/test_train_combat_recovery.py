from __future__ import annotations

import json
from pathlib import Path

import pytest

pytest.importorskip("torch")

from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.test_torch_combat_recovery_session import _ReplayableSource
from sts_learning.combat_objective import CombatPolicyUpdateConfig
from sts_learning.combat_potion_lane import CombatPotionLane
from sts_learning.published_combat_behavior import recover_published_combat_behavior
from sts_learning.torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinSessionLimits,
)
from sts_learning.torch_provenance import AdamTrainingConfig
from sts_learning.train_combat_recovery import (
    run_combat_recovery_training,
)
from sts_learning.train_combat import CombatTrainingCommandConfig


def test_recovery_training_journal_is_a_recoverable_combat_publication(
    tmp_path: Path,
) -> None:
    artifact = tmp_path / "source.bin"
    artifact.write_bytes(b"opaque-root")
    output = tmp_path / "training"
    source = _ReplayableSource(expected_slot=1)
    bridge = CombatSessionBridge(
        combat_roots_from_artifact=lambda payload, **_: source,
        semantic_schema=semantic_schema_fixture(),
    )

    summary = run_combat_recovery_training(
        CombatTrainingCommandConfig(
            artifact=artifact,
            output=output,
            root_count=2,
            replicate_count=2,
            updates=1,
            model_seed=7,
            behavior_seed_base=11,
            potion_lane=CombatPotionLane.ROOT_SLOTS,
            potion_slots=(0,),
            policy_update=CombatPolicyUpdateConfig.ppo_clip_value(),
            optimizer=AdamTrainingConfig(learning_rate=2.5e-4),
        ),
        source_expected_roots=2,
        source_root_slot=1,
        bridge=bridge,
    )

    records = tuple(
        json.loads(line)
        for line in (output / "training.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    )
    assert records[0]["curriculum"] == "verified-win-terminal-nearest"
    assert records[0]["teacher_replicate_index"] == 1
    assert records[0]["root_count"] == 2
    assert records[0]["source_artifact_root_count"] == 2
    assert records[0]["source_root_slot"] == 1
    assert records[0]["policy_update_rule"] == "PPO_CLIP_VALUE"
    assert records[0]["policy_value_loss_coefficient"] == 0.5
    assert records[0]["optimizer_learning_rate"] == 2.5e-4
    assert 1 <= records[1]["optimizer_steps_applied"] <= 4
    assert records[1]["value_loss"] > 0.0
    assert records[-1]["final_manifest_id"] == summary["final_manifest_id"]
    assert summary["source_wins"] == 1
    assert summary["teacher_final_hp"] == 30

    recovered = recover_published_combat_behavior(
        output,
        bridge,
        CombatWinSessionLimits(),
        (99,),
    )
    assert recovered.training_root_count == 2
    assert recovered.training_potion_lane is CombatPotionLane.ROOT_SLOTS
    assert recovered.training_potion_slots == (0,)
