from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

import pytest

pytest.importorskip("torch")

from learning.tests.driver_fixtures import NumpyWinningBatchEnv
from learning.tests.run_training_fixtures import published_behavior
from sts_learning.combat_potion_lane import CombatPotionLane
from sts_learning.evaluate_run import (
    RunEvaluationCommandConfig,
    RunPotionLane,
    run_run_evaluation,
)
from sts_learning.evaluate_run_potions import (
    RunPotionComparisonCommandConfig,
    run_run_potion_comparison,
)
from sts_learning.published_combat_behavior import recover_published_combat_behavior
from sts_learning.published_run_behavior import (
    PublishedRunBehaviorError,
    is_run_training_publication,
    recover_published_run_behavior,
)
from sts_learning.train_run import (
    RunTrainingCommandError,
    RunTrainingCommandConfig,
    _resolve_run_policy_update,
    run_run_training,
)
from sts_learning import (
    RunPolicyUpdateConfig,
    RunPolicyUpdateRule,
    TerminalAdvantageMode,
)
from sts_learning.torch_combat_session_config import CombatWinSessionLimits
from sts_learning.torch_policy import require_matching_actor_state


def test_run_advantage_normalization_override_is_typed() -> None:
    assert _resolve_run_policy_update("ppo-clip-value", "auto",
        value_prediction_target="terminal_score").value_prediction_target == "terminal_score"
    with pytest.raises(RunTrainingCommandError, match="REINFORCE"):
        _resolve_run_policy_update("reinforce", "auto", value_prediction_target="terminal_score")
    automatic = _resolve_run_policy_update("ppo-clip-value", "auto")
    enabled = _resolve_run_policy_update("ppo-clip-value", "on")
    disabled = _resolve_run_policy_update("ppo-clip-value", "off")

    assert automatic.normalize_advantage is True
    assert enabled.normalize_advantage is True
    assert disabled.normalize_advantage is False
    assert disabled.rule is automatic.rule
    assert disabled.epochs == automatic.epochs
    with pytest.raises(RunTrainingCommandError, match="requires value PPO"):
        _resolve_run_policy_update("reinforce", "on")
    with pytest.raises(RunTrainingCommandError, match="unknown"):
        _resolve_run_policy_update("ppo-clip-value", "sideways")
    calibration = _resolve_run_policy_update("critic-calibration", "auto")
    assert calibration.updates_actor is False
    assert calibration.epochs == 256
    assert calibration.max_grad_norm is None
    assert calibration.value_clip_coefficient is None
    assert calibration.value_loss_coefficient == pytest.approx(1.0)
    assert (
        _resolve_run_policy_update(
            "critic-calibration",
            "auto",
            critic_fit_steps=32,
        ).epochs
        == 32
    )
    with pytest.raises(RunTrainingCommandError, match="no actor"):
        _resolve_run_policy_update("critic-calibration", "off")


def test_run_evaluation_uses_frozen_combat_behavior_without_recovery(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    capsys.readouterr()

    output = tmp_path / "run-evaluation"
    summary = run_run_evaluation(
        RunEvaluationCommandConfig(
            behavior=behavior,
            output=output,
            slot_count=1,
            terminal_attempts=2,
            max_batch_steps=2,
            behavior_seed=501,
            ascension_level=20,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert summary["schema"] == "sts-learning-run-held-out-evaluation-v10"
    assert summary["execution_behavior_manifest_id"] == summary[
        "behavior_manifest_id"
    ]
    assert len(summary["execution_model_definition_id"]) == 64
    assert len(summary["execution_model_config_id"]) == 64
    assert len(summary["execution_behavior_rule_implementation_id"]) == 64
    assert len(summary["execution_behavior_rule_configuration_id"]) == 64
    assert len(summary["execution_semantic_schema_id"]) == 64
    assert summary["execution_semantic_schema_version"] > 0
    assert summary["execution_scope"] == "publication_sampled"
    assert summary["execution_strategic_source_manifest_id"] == summary[
        "behavior_manifest_id"
    ]
    assert summary["execution_combat_anchor_manifest_id"] is None
    assert summary["seed_partition_held_out_numerator"] == 1
    assert summary["seed_partition_denominator"] == 10
    assert summary["ascension_level"] == 20
    assert summary["behavior_training_kind"] == "combat"
    assert summary["behavior_training_all_loss_axis"] == "none"
    assert summary["behavior_run_sampling_mode"] is None
    assert summary["behavior_run_episode_root_attempts"] is None
    assert summary["behavior_run_combat_decision_rule"] is None
    assert summary["behavior_run_combat_anchor_manifest_id"] is None
    assert summary["behavior_run_combat_anchor_checkpoint_id"] is None
    assert summary["behavior_run_combat_anchor_scorer"] is None
    assert summary["combat_potion_lane"] == "all"
    assert summary["requested_combat_potion_lane"] == "trained"
    assert summary["kind"] == "completed"
    assert summary["target_reached"] is True
    assert summary["step_limit_reached"] is False
    assert summary["terminal_attempts"] == 2
    assert summary["victories"] == 2
    assert summary["defeats"] == 0
    assert summary["terminal_floor_sum"] == 80
    assert summary["min_terminal_floor"] == 40
    assert summary["max_terminal_floor"] == 40
    assert summary["terminal_floor_counts"] == ((40, 2),)
    assert summary["terminal_act_counts"] == ((3, 2),)
    assert summary["combat_transition_count"] == 2
    assert {
        (transition["encounter_id"], transition["monster_ids"])
        for transition in summary["combat_transitions"]
    } == {("JawWorm", ("JawWorm",))}
    assert summary["combat_hp_loss_sum"] == 120
    assert summary["combat_potion_identity_losses"] == ()
    assert summary["combat_potion_identity_gains"] == ()
    assert summary["open_combat_count"] == 0
    assert len(summary["combat_seed_summaries"]) == 2
    assert summary["recoveries"] == 0
    assert (output / "evaluation.json").is_file()
    stdout = capsys.readouterr().out
    assert (
        "run_evaluation_complete=true ascension=20 potion_lane=all "
        "potion_lane_request=trained "
        "target_reached=true attempts=2/2 "
        "victories=2 defeats=0 floor_sum=80 floor_min=40 floor_max=40 "
        "floor_counts=40:2 act_counts=3:2"
    ) in stdout


def test_run_evaluation_selects_the_no_combat_potion_environment(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    created: list[tuple[int, ...]] = []

    def without_combat_potions(
        seeds: list[int],
        ascension_level: int,
    ) -> NumpyWinningBatchEnv:
        assert ascension_level == 20
        created.append(tuple(seeds))
        return NumpyWinningBatchEnv(seeds)

    run_bridge = replace(
        run_bridge,
        environment_without_combat_potions=without_combat_potions,
    )
    summary = run_run_evaluation(
        RunEvaluationCommandConfig(
            behavior=behavior,
            output=tmp_path / "run-evaluation-never",
            slot_count=1,
            terminal_attempts=2,
            max_batch_steps=2,
            behavior_seed=501,
            ascension_level=20,
            potion_lane=RunPotionLane.NEVER,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert len(created) == 1
    assert len(created[0]) == 1
    assert summary["combat_potion_lane"] == "never"
    assert summary["combat_potion_identity_losses"] == ()


def test_run_potion_comparison_pairs_terminal_seeds(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)

    summary = run_run_potion_comparison(
        RunPotionComparisonCommandConfig(
            behavior=behavior,
            output=tmp_path / "run-potion-comparison",
            terminal_attempts=2,
            max_batch_steps=2,
            behavior_seed=501,
            ascension_level=20,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert summary["kind"] == "completed"
    assert tuple(
        lane["terminal_floor_sum"] for lane in summary["lanes"]
    ) == (80, 80)
    assert summary["comparison"] == {
        "paired_terminal_seeds": 2,
        "all_only_terminal_seeds": (),
        "never_only_terminal_seeds": (),
        "never_deeper": 0,
        "same_floor": 2,
        "never_shallower": 0,
        "paired_floor_delta_never_minus_all": 0,
    }


def test_run_training_warm_starts_publishes_and_evaluates(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    capsys.readouterr()

    output = tmp_path / "run-training"
    summary = run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=behavior,
            output=output,
            slot_count=2,
            generations=1,
            attempts_per_update=2,
            max_batch_steps_per_generation=1,
            model_seed=43,
            behavior_seed=94,
            training_seed_start=0,
            evaluation_attempts=2,
            evaluation_max_batch_steps=2,
            evaluation_behavior_seed=501,
            held_out_seed_start=1000,
            ascension_level=20,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert summary["kind"] == "completed"
    assert summary["generations"] == 1
    assert summary["optimizer_steps"] == 1
    assert summary["episode_root_attempts"] is None
    assert summary["held_out_target_reached"] is True
    assert summary["held_out_attempts"] == 2
    assert summary["held_out_victories"] == 2
    assert summary["held_out_floor_sum"] == 80
    assert summary["held_out_floor_counts"] == ((40, 2),)
    assert summary["training_resources_cumulative"]["combat_count"] == 2
    assert summary["training_resources_cumulative"]["early_combat_ordinals"][0][
        "count"
    ] == 2
    assert summary["held_out_resources"]["combat_count"] == 2
    assert (output / "summary.json").is_file()
    records = tuple(
        json.loads(line)
        for line in (output / "training.jsonl").read_text(
            encoding="utf-8"
        ).splitlines()
    )
    assert tuple(record["kind"] for record in records) == (
        "configuration",
        "generation",
        "completed",
    )
    assert records[0]["advantage_mode"] == "raw_return"
    assert records[0]["decision_scope"] == "all"
    assert records[0]["sampling_mode"] == "independent-cohorts"
    assert records[0]["episode_root_attempts"] is None
    assert records[0]["requested_run_potion_lane"] == "trained"
    assert records[0]["run_potion_lane"] == "all"
    stdout = capsys.readouterr().out
    assert (
        "run_generation=0 promoted=true attempts=2 victories=2 defeats=0 "
        "episodes=2 recoveries=0 floor_sum=80 floor_counts=40:2"
    ) in stdout
    assert (
        "run_training_complete=true ascension=20 potion_lane=all "
        "potion_lane_request=trained sampling_mode=independent-cohorts "
        "episode_root_attempts=none "
        "generations=1 optimizer_steps=1 "
        "held_out_attempts=2/2 held_out_victories=2 "
        "held_out_floor_sum=80 held_out_floor_counts=40:2"
    ) in stdout

    reevaluation = run_run_evaluation(
        RunEvaluationCommandConfig(
            behavior=output,
            output=tmp_path / "run-training-reevaluation",
            slot_count=1,
            terminal_attempts=2,
            max_batch_steps=2,
            behavior_seed=777,
            ascension_level=20,
            held_out_seed_start=2000,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )
    assert reevaluation["behavior_training_kind"] == "run"
    assert reevaluation["behavior_run_sampling_mode"] == "independent-cohorts"
    assert reevaluation["behavior_run_episode_root_attempts"] is None
    assert reevaluation["behavior_run_combat_decision_rule"] == "sampled"
    assert reevaluation["behavior_run_objective"] == {
        "attempts_per_update": 2,
        "advantage_mode": "raw_return",
        "decision_scope": "all",
        "policy_update": "reinforce",
        "normalize_advantage": False,
        "value_clip_coefficient": None,
    }
    assert reevaluation["terminal_attempts"] == 2
    assert is_run_training_publication(output)

    v2_records = tuple(
        record | {"schema": "sts-learning-run-training-v2"}
        for record in records
    )
    (output / "training.jsonl").write_text(
        "".join(
            json.dumps(record, separators=(",", ":"), sort_keys=True) + "\n"
            for record in v2_records
        ),
        encoding="utf-8",
        newline="\n",
    )
    assert not is_run_training_publication(output)
    with pytest.raises(
        PublishedRunBehaviorError,
        match="exact configuration/completion",
    ):
        recover_published_run_behavior(output, run_bridge, (777,))

    v5_records = tuple(
        record | {"schema": "sts-learning-run-training-v5"}
        for record in records
    )
    (output / "training.jsonl").write_text(
        "".join(
            json.dumps(record, separators=(",", ":"), sort_keys=True) + "\n"
            for record in v5_records
        ),
        encoding="utf-8",
        newline="\n",
    )
    assert is_run_training_publication(output)
    with pytest.raises(
        PublishedRunBehaviorError,
        match="current explicit run provenance",
    ):
        recover_published_run_behavior(output, run_bridge, (777,))

    (output / "training.jsonl").write_text(
        "".join(
            json.dumps(record, separators=(",", ":"), sort_keys=True) + "\n"
            for record in records
        ),
        encoding="utf-8",
        newline="\n",
    )
    records[-1]["sampling_mode"] = "episode-root-retries"
    (output / "training.jsonl").write_text(
        "".join(
            json.dumps(record, separators=(",", ":"), sort_keys=True) + "\n"
            for record in records
        ),
        encoding="utf-8",
        newline="\n",
    )
    with pytest.raises(PublishedRunBehaviorError, match="sampling mode changed"):
        recover_published_run_behavior(output, run_bridge, (777,))


@pytest.mark.parametrize("value_target", ("return_to_go", "terminal_score"))
def test_run_value_ppo_warm_starts_actor_and_publishes_diagnostics(
    tmp_path: Path,
    value_target: str,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    output = tmp_path / "run-value-training"

    summary = run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=behavior,
            output=output,
            slot_count=2,
            generations=1,
            attempts_per_update=2,
            max_batch_steps_per_generation=1,
            model_seed=43,
            behavior_seed=94,
            training_seed_start=0,
            evaluation_attempts=2,
            evaluation_max_batch_steps=2,
            evaluation_behavior_seed=501,
            held_out_seed_start=1000,
            ascension_level=20,
            policy_update=replace(RunPolicyUpdateConfig.ppo_clip_value(),
                                  value_prediction_target=value_target),
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    records = tuple(
        json.loads(line)
        for line in (output / "training.jsonl").read_text(
            encoding="utf-8"
        ).splitlines()
    )
    generation = records[1]
    assert summary["run_policy_update"] == "ppo_clip_value"
    assert summary["run_policy_value_prediction_target"] == value_target
    assert summary["run_policy_normalize_advantage"] is True
    assert 1 <= summary["optimizer_steps"] <= 4
    assert generation["optimizer_steps_applied"] == summary["optimizer_steps"]
    assert generation["value_loss"] > 0.0
    assert generation["value_clip_fraction"] >= 0.0
    assert generation["gradient_norm"] > 0.0
    assert generation["approximate_kl"] >= 0.0
    rollout = generation["rollout_value_diagnostics"]
    assert rollout["weighting"] == "attempt_equal"
    assert rollout["optimization_target"] == "decision_local_return_to_go"
    assert rollout["advantage_estimator"] == "gae"
    assert rollout["gamma"] == 1.0
    assert rollout["gae_lambda"] == 1.0
    assert rollout["actor_mask"] == "multiple_candidates"
    assert (
        rollout["critic_residual_convention"]
        == "return_to_go_target_minus_prediction"
    )
    offset = 0.0 if value_target == "return_to_go" else 20.0 / 13.0
    assert rollout["critic_prediction"]["weighted_mean"] == pytest.approx(-offset)
    assert rollout["pre_normalization_actor_advantage"] is not None
    assert rollout["actor_advantage"]["zero_weight"] == pytest.approx(1.0)
    assert rollout["advantage_normalization_applied"] is True
    assert rollout["advantage_normalization_sign_changes"] >= 0
    assert rollout["advantage_normalization_positive_from_nonpositive"] >= 0
    assert rollout["advantage_normalization_negative_from_nonnegative"] >= 0
    assert rollout["critic_residual"]["weighted_mean"] == pytest.approx(
        rollout["return_to_go_target"]["weighted_mean"] + offset
    )
    assert rollout["return_to_go_target"]["weighted_mean"] == pytest.approx(
        -7.0 / 13.0
    )
    assert rollout["actor_decisions"] == generation["actor_decisions"]
    assert rollout["forced_decisions"] == 0

    recovered = recover_published_run_behavior(output, run_bridge, (777,))
    assert recovered.objective.policy_update.uses_value_baseline
    assert recovered.objective.policy_update.value_prediction_target == value_target
    assert (
        recovered.objective.advantage_mode
        is TerminalAdvantageMode.DECISION_LOCAL_GAE
    )
    assert recovered.policies[0].frozen_scorer.config.value_head
    records[-1]["run_policy_value_prediction_target"] = (
        "terminal_score" if value_target == "return_to_go" else "return_to_go")
    (output / "training.jsonl").write_text("".join(json.dumps(record) + "\n" for record in records),
                                           encoding="utf-8")
    with pytest.raises(PublishedRunBehaviorError, match="value target changed"):
        recover_published_run_behavior(output, run_bridge, (777,))


def test_run_critic_calibration_is_actor_neutral_and_seeds_fresh_ppo(
    tmp_path: Path,
) -> None:
    # This crosses both durable publications so recovery cannot skip the exact
    # actor-state audit between calibration and the fresh PPO session.
    behavior, combat_bridge, run_bridge = published_behavior(tmp_path)
    calibration_output = tmp_path / "run-critic-calibration"

    calibration_summary = run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=behavior,
            output=calibration_output,
            slot_count=2,
            generations=1,
            attempts_per_update=2,
            max_batch_steps_per_generation=1,
            model_seed=43,
            behavior_seed=94,
            training_seed_start=0,
            evaluation_attempts=2,
            evaluation_max_batch_steps=2,
            evaluation_behavior_seed=501,
            held_out_seed_start=1000,
            ascension_level=20,
            policy_update=RunPolicyUpdateConfig.critic_calibration(steps=4),
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )
    records = tuple(
        json.loads(line)
        for line in (calibration_output / "training.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    )
    generation = records[1]
    assert calibration_summary["run_policy_update"] == "critic_calibration"
    assert generation["actor_update_applied"] is False
    assert generation["actor_decisions"] == 0
    assert generation["trained_decisions"] == 0
    assert generation["value_loss"] > 0.0

    calibration = recover_published_run_behavior(
        calibration_output,
        run_bridge,
        (777,),
    )
    assert (
        calibration.objective.policy_update.rule
        is RunPolicyUpdateRule.CRITIC_CALIBRATION
    )
    assert calibration.critic_initialization_manifest_id is None
    combat_behavior = recover_published_combat_behavior(
        behavior,
        combat_bridge,
        CombatWinSessionLimits(),
        (777,),
    )
    require_matching_actor_state(
        combat_behavior.policies[0].frozen_scorer,
        calibration.policies[0].frozen_scorer,
    )

    actor_output = tmp_path / "run-ppo-from-calibrated-critic"
    actor_summary = run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=behavior,
            critic_initialization_behavior=calibration_output,
            output=actor_output,
            slot_count=2,
            generations=1,
            attempts_per_update=2,
            max_batch_steps_per_generation=1,
            model_seed=43,
            behavior_seed=194,
            training_seed_start=2000,
            evaluation_attempts=2,
            evaluation_max_batch_steps=2,
            evaluation_behavior_seed=601,
            held_out_seed_start=3000,
            ascension_level=20,
            policy_update=RunPolicyUpdateConfig.ppo_clip_value(),
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )
    assert actor_summary["critic_initialization_manifest_id"] == (
        calibration.manifest_id.digest.hex()
    )
    assert actor_summary["critic_initialization_checkpoint_id"] == (
        calibration.checkpoint_id.digest.hex()
    )
    actor_records = tuple(
        json.loads(line)
        for line in (actor_output / "training.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    )
    assert actor_records[0]["critic_initialization_behavior"] == str(
        calibration_output.resolve()
    )
    assert actor_records[1]["actor_update_applied"] is True
    recovered_actor = recover_published_run_behavior(
        actor_output,
        run_bridge,
        (888,),
    )
    assert recovered_actor.critic_initialization_manifest_id == (
        calibration.manifest_id
    )
    assert recovered_actor.critic_initialization_checkpoint_id == (
        calibration.checkpoint_id
    )
    assert (
        recovered_actor.critic_initialization_policy_update
        is RunPolicyUpdateRule.CRITIC_CALIBRATION
    )


def test_run_training_inherits_the_warm_start_potion_lane(
    tmp_path: Path,
) -> None:
    behavior, combat_bridge, run_bridge = published_behavior(
        tmp_path,
        potion_lane=CombatPotionLane.NEVER,
    )
    no_potion_populations: list[tuple[int, ...]] = []

    def without_combat_potions(
        seeds: list[int],
        ascension_level: int,
    ) -> NumpyWinningBatchEnv:
        assert ascension_level == 20
        no_potion_populations.append(tuple(seeds))
        return NumpyWinningBatchEnv(seeds)

    run_bridge = replace(
        run_bridge,
        environment_without_combat_potions=without_combat_potions,
    )
    summary = run_run_training(
        RunTrainingCommandConfig(
            warm_start_behavior=behavior,
            output=tmp_path / "run-training-never",
            slot_count=1,
            generations=0,
            attempts_per_update=1,
            max_batch_steps_per_generation=1,
            model_seed=43,
            behavior_seed=94,
            training_seed_start=0,
            evaluation_attempts=1,
            evaluation_max_batch_steps=1,
            evaluation_behavior_seed=501,
            held_out_seed_start=1000,
            ascension_level=20,
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )

    assert len(no_potion_populations) == 2
    assert summary["requested_run_potion_lane"] == "trained"
    assert summary["run_potion_lane"] == "never"
