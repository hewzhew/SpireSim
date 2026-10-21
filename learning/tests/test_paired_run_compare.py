from __future__ import annotations

import copy
from pathlib import Path

import pytest

pytest.importorskip("torch")

from sts_learning.evaluate_run import RunPotionLane
from sts_learning.paired_run_compare import (
    PairedRunComparisonConfig,
    PairedRunComparisonError,
    RunComparisonScope,
    compare_completed_run_evaluations,
    run_paired_run_comparison,
)


def _seed(
    seed: int,
    *,
    won: bool,
    floor: int,
    hp: int,
) -> dict[str, object]:
    return {
        "seed": seed,
        "slot_index": 0,
        "combat_count": 3,
        "hp_loss_sum": 40,
        "last_act": 1,
        "last_floor": floor,
        "last_hp": hp,
        "last_max_hp": 75,
        "last_gold": 120,
        "last_potion_ids": (None, None),
        "terminal_reward": 1 if won else -1,
        "open_combat": False,
        "potion_identity_losses": (),
        "potion_identity_gains": (),
    }


def _summary(
    manifest_byte: str,
    seeds: list[dict[str, object]],
) -> dict[str, object]:
    wins = sum(seed["terminal_reward"] == 1 for seed in seeds)
    return {
        "schema": "sts-learning-run-held-out-evaluation-v10",
        "kind": "completed",
        "target_reached": True,
        "behavior_manifest_id": manifest_byte * 64,
        "execution_behavior_manifest_id": manifest_byte * 64,
        "behavior_checkpoint_id": manifest_byte.upper() * 64,
        "execution_model_definition_id": "a" * 64,
        "execution_model_config_id": "b" * 64,
        "execution_behavior_rule_implementation_id": "c" * 64,
        "execution_behavior_rule_configuration_id": "d" * 64,
        "execution_semantic_schema_id": "e" * 64,
        "execution_semantic_schema_version": 7,
        "execution_scope": "publication_sampled",
        "execution_strategic_source_manifest_id": manifest_byte * 64,
        "execution_combat_anchor_manifest_id": None,
        "execution_combat_anchor_checkpoint_id": None,
        "execution_combat_anchor_model_definition_id": None,
        "execution_combat_anchor_model_config_id": None,
        "execution_combat_anchor_semantic_schema_id": None,
        "execution_combat_anchor_semantic_schema_version": None,
        "behavior_seed": 501,
        "ascension_level": 20,
        "held_out_seed_start": 1000,
        "held_out_seed_end": 1020,
        "seed_partition_held_out_numerator": 1,
        "seed_partition_denominator": 10,
        "requested_combat_potion_lane": "never",
        "combat_potion_lane": "never",
        "slot_count": 1,
        "terminal_attempt_target": len(seeds),
        "terminal_attempts": len(seeds),
        "max_batch_steps": 4096,
        "victories": wins,
        "defeats": len(seeds) - wins,
        "terminal_floor_sum": sum(int(seed["last_floor"]) for seed in seeds),
        "batch_steps": 100,
        "combat_seed_summaries": seeds,
    }


def test_paired_run_comparison_preserves_seed_outcomes_and_raw_axes() -> None:
    baseline = _summary(
        "1",
        [
            _seed(1001, won=False, floor=6, hp=0),
            _seed(1011, won=True, floor=57, hp=20),
        ],
    )
    candidate = _summary(
        "2",
        [
            _seed(1001, won=True, floor=57, hp=10),
            _seed(1011, won=True, floor=57, hp=25),
        ],
    )

    comparison = compare_completed_run_evaluations(baseline, candidate)

    assert comparison["kind"] == "complete"
    assert comparison["contract"]["terminal_seeds"] == (1001, 1011)
    assert comparison["contract"]["policy_rng_scope"] == (
        "same_initial_stream_per_behavior_path_dependent_consumption"
    )
    assert comparison["aggregate"]["victory_delta"] == 1
    assert comparison["aggregate"]["loss_to_win_seeds"] == 1
    assert comparison["aggregate"]["win_to_loss_seeds"] == 0
    assert comparison["aggregate"]["terminal_floor_sum_delta"] == 51
    assert comparison["aggregate"]["common_victory_final_hp_delta"] == 5
    assert comparison["seeds"][0]["axes"]["floor"]["delta"] == 51
    assert comparison["seeds"][1]["axes"]["hp"]["delta"] == 5
    assert "better" not in comparison
    assert len(comparison["contract_id"]) == 64
    assert len(comparison["comparison_id"]) == 64


def test_paired_run_comparison_rejects_rng_rule_or_terminal_seed_drift() -> None:
    baseline = _summary("1", [_seed(1001, won=False, floor=6, hp=0)])
    candidate = _summary("2", [_seed(1001, won=True, floor=57, hp=10)])

    wrong_rule = copy.deepcopy(candidate)
    wrong_rule["execution_behavior_rule_configuration_id"] = "f" * 64
    with pytest.raises(PairedRunComparisonError, match="behavior_rule"):
        compare_completed_run_evaluations(baseline, wrong_rule)

    wrong_seed = copy.deepcopy(candidate)
    wrong_seed["combat_seed_summaries"][0]["seed"] = 1011
    with pytest.raises(PairedRunComparisonError, match="different terminal seeds"):
        compare_completed_run_evaluations(baseline, wrong_seed)


def test_scoped_run_comparison_changes_only_the_combat_anchor() -> None:
    baseline = _summary("1", [_seed(1001, won=False, floor=3, hp=0)])
    candidate = _summary("2", [_seed(1001, won=False, floor=5, hp=0)])
    strategic_manifest = "9" * 64
    for summary, anchor_byte in ((baseline, "3"), (candidate, "4")):
        summary["behavior_manifest_id"] = strategic_manifest
        summary["execution_scope"] = (
            "combat_anchor_greedy_strategic_source_sampled"
        )
        summary["execution_strategic_source_manifest_id"] = strategic_manifest
        summary["execution_combat_anchor_manifest_id"] = anchor_byte * 64
        summary["execution_combat_anchor_checkpoint_id"] = anchor_byte.upper() * 64
        summary["execution_combat_anchor_model_definition_id"] = "a" * 64
        summary["execution_combat_anchor_model_config_id"] = "b" * 64
        summary["execution_combat_anchor_semantic_schema_id"] = "e" * 64
        summary["execution_combat_anchor_semantic_schema_version"] = 7
        summary["execution_behavior_rule_configuration_id"] = anchor_byte * 64

    comparison = compare_completed_run_evaluations(
        baseline,
        candidate,
        scope=RunComparisonScope.COMBAT_ANCHOR_ONLY,
    )

    assert comparison["contract"]["comparison_scope"] == "combat_anchor_only"
    assert comparison["contract"]["strategic_source_manifest_sha256"] == (
        strategic_manifest
    )
    assert comparison["contract"]["baseline_combat_anchor_manifest_sha256"] == (
        "3" * 64
    )
    assert comparison["contract"]["candidate_combat_anchor_manifest_sha256"] == (
        "4" * 64
    )
    assert comparison["contract"]["policy_rng_scope"] == (
        "same_initial_strategic_stream_combat_greedy_consumes_no_rng"
    )
    assert comparison["aggregate"]["terminal_floor_sum_delta"] == 2


def test_paired_run_command_retains_both_evaluations(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    baseline_behavior = tmp_path / "baseline-behavior"
    candidate_behavior = tmp_path / "candidate-behavior"
    baseline_behavior.mkdir()
    candidate_behavior.mkdir()
    baseline = _summary("1", [_seed(1001, won=False, floor=6, hp=0)])
    candidate = _summary("2", [_seed(1001, won=True, floor=57, hp=10)])
    calls: list[object] = []

    def fake_run(config, **_kwargs):
        calls.append(config)
        config.output.mkdir(parents=True)
        (config.output / "evaluation.json").write_text("{}\n", encoding="utf-8")
        return baseline if len(calls) == 1 else candidate

    monkeypatch.setattr(
        "sts_learning.paired_run_compare.run_run_evaluation",
        fake_run,
    )
    output = tmp_path / "comparison"
    result = run_paired_run_comparison(
        PairedRunComparisonConfig(
            baseline_behavior=baseline_behavior,
            candidate_behavior=candidate_behavior,
            output=output,
            terminal_attempts=1,
            max_batch_steps=4096,
            behavior_seed=501,
            ascension_level=20,
            held_out_seed_start=1000,
            potion_lane=RunPotionLane.NEVER,
        ),
        combat_bridge=object(),
        run_bridge=object(),
        print_completion=False,
    )

    assert len(calls) == 2
    assert result["aggregate"]["loss_to_win_seeds"] == 1
    assert result["baseline_evaluation"] == "baseline/evaluation.json"
    assert result["candidate_evaluation"] == "candidate/evaluation.json"
    assert (output / "baseline" / "evaluation.json").is_file()
    assert (output / "candidate" / "evaluation.json").is_file()
    assert (output / "paired-comparison.json").is_file()
