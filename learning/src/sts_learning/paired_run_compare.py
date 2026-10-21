"""Run and compare two frozen behaviors on identical complete-run seeds."""

from __future__ import annotations

import argparse
import hashlib
import json
import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from enum import Enum
from pathlib import Path

from .evaluate_run import (
    RunEvaluationCommandConfig,
    RunEvaluationCommandError,
    RunPotionLane,
    run_recovered_run_evaluation,
    run_run_evaluation,
)
from .published_combat_behavior import recover_published_combat_behavior
from .torch_behavior import FrozenCombatAnchor, FrozenCombatGreedyTorchPolicy
from .torch_combat_session_config import CombatSessionBridge
from .torch_combat_session_config import CombatWinSessionLimits
from .torch_session_config import CategoricalSessionBridge


PAIRED_RUN_SCHEMA = "sts-learning-paired-run-comparison-v2"


class PairedRunComparisonError(RuntimeError):
    """Two whole-run evaluations are incomplete, mismatched, or incomparable."""


class RunComparisonScope(str, Enum):
    FULL_BEHAVIOR = "full_behavior"
    COMBAT_ANCHOR_ONLY = "combat_anchor_only"


@dataclass(frozen=True)
class PairedRunComparisonConfig:
    baseline_behavior: Path
    candidate_behavior: Path
    output: Path
    terminal_attempts: int
    max_batch_steps: int
    behavior_seed: int
    ascension_level: int
    held_out_seed_start: int = 0
    potion_lane: RunPotionLane = RunPotionLane.NEVER
    strategic_behavior: Path | None = None

    def __post_init__(self) -> None:
        baseline = Path(self.baseline_behavior).resolve()
        candidate = Path(self.candidate_behavior).resolve()
        output = Path(self.output).resolve()
        if not baseline.is_dir() or not candidate.is_dir():
            raise PairedRunComparisonError(
                "paired run behaviors must be directories"
            )
        strategic = (
            None
            if self.strategic_behavior is None
            else Path(self.strategic_behavior).resolve()
        )
        if strategic is not None and not strategic.is_dir():
            raise PairedRunComparisonError(
                "paired run strategic behavior must be a directory"
            )
        if baseline == candidate:
            raise PairedRunComparisonError(
                "paired run comparison requires distinct behavior directories"
            )
        if output.exists() and (not output.is_dir() or any(output.iterdir())):
            raise PairedRunComparisonError(
                "paired run output must be absent or empty"
            )
        behavior_directories = (
            (baseline, candidate)
            if strategic is None
            else (baseline, candidate, strategic)
        )
        if any(
            output == behavior or behavior in output.parents
            for behavior in behavior_directories
        ):
            raise PairedRunComparisonError(
                "paired run output must stay outside behavior directories"
            )
        if not isinstance(self.potion_lane, RunPotionLane):
            raise PairedRunComparisonError(
                "paired run potion lane must be typed"
            )
        probe = RunEvaluationCommandConfig(
            behavior=baseline,
            output=output / "baseline",
            slot_count=1,
            terminal_attempts=self.terminal_attempts,
            max_batch_steps=self.max_batch_steps,
            behavior_seed=self.behavior_seed,
            ascension_level=self.ascension_level,
            held_out_seed_start=self.held_out_seed_start,
            potion_lane=self.potion_lane,
        )
        object.__setattr__(self, "baseline_behavior", baseline)
        object.__setattr__(self, "candidate_behavior", candidate)
        object.__setattr__(self, "output", output)
        object.__setattr__(self, "strategic_behavior", strategic)
        object.__setattr__(self, "terminal_attempts", probe.terminal_attempts)
        object.__setattr__(self, "max_batch_steps", probe.max_batch_steps)
        object.__setattr__(self, "behavior_seed", probe.behavior_seed)
        object.__setattr__(self, "ascension_level", probe.ascension_level)
        object.__setattr__(
            self,
            "held_out_seed_start",
            probe.held_out_seed_start,
        )

    @property
    def comparison_scope(self) -> RunComparisonScope:
        return (
            RunComparisonScope.FULL_BEHAVIOR
            if self.strategic_behavior is None
            else RunComparisonScope.COMBAT_ANCHOR_ONLY
        )


def compare_completed_run_evaluations(
    baseline: Mapping[str, object],
    candidate: Mapping[str, object],
    *,
    scope: RunComparisonScope = RunComparisonScope.FULL_BEHAVIOR,
) -> dict[str, object]:
    """Produce per-seed raw differences without declaring a better policy."""

    left = _mapping(baseline, "baseline evaluation")
    right = _mapping(candidate, "candidate evaluation")
    if not isinstance(scope, RunComparisonScope):
        raise PairedRunComparisonError("paired run comparison scope must be typed")
    for name, summary in (("baseline", left), ("candidate", right)):
        if summary.get("kind") != "completed" or summary.get("target_reached") is not True:
            raise PairedRunComparisonError(
                f"{name} run evaluation is not complete"
            )
    shared_fields = (
        "schema",
        "execution_model_definition_id",
        "execution_model_config_id",
        "execution_behavior_rule_implementation_id",
        "execution_semantic_schema_id",
        "execution_semantic_schema_version",
        "behavior_seed",
        "ascension_level",
        "held_out_seed_start",
        "held_out_seed_end",
        "seed_partition_held_out_numerator",
        "seed_partition_denominator",
        "requested_combat_potion_lane",
        "combat_potion_lane",
        "slot_count",
        "terminal_attempt_target",
        "max_batch_steps",
    )
    for field in shared_fields:
        if _canonical(left.get(field)) != _canonical(right.get(field)):
            raise PairedRunComparisonError(
                f"paired run evaluations disagree on {field}"
            )
    if _integer(left.get("slot_count"), "slot_count") != 1:
        raise PairedRunComparisonError(
            "paired run comparison requires one-slot evaluations"
        )
    baseline_manifest = _sha256(
        left.get("execution_behavior_manifest_id"),
        "baseline execution_behavior_manifest_id",
    )
    candidate_manifest = _sha256(
        right.get("execution_behavior_manifest_id"),
        "candidate execution_behavior_manifest_id",
    )
    if baseline_manifest == candidate_manifest:
        raise PairedRunComparisonError(
            "paired run evaluations use the same execution behavior manifest"
        )
    baseline_anchor: str | None = None
    candidate_anchor: str | None = None
    if scope is RunComparisonScope.FULL_BEHAVIOR:
        if _canonical(left.get("execution_behavior_rule_configuration_id")) != (
            _canonical(right.get("execution_behavior_rule_configuration_id"))
        ):
            raise PairedRunComparisonError(
                "paired run evaluations disagree on "
                "execution_behavior_rule_configuration_id"
            )
    else:
        _validate_scoped_combat_pair(left, right)
        baseline_anchor = _sha256(
            left.get("execution_combat_anchor_manifest_id"),
            "baseline execution_combat_anchor_manifest_id",
        )
        candidate_anchor = _sha256(
            right.get("execution_combat_anchor_manifest_id"),
            "candidate execution_combat_anchor_manifest_id",
        )
        if baseline_anchor == candidate_anchor:
            raise PairedRunComparisonError(
                "scoped run comparison requires distinct combat anchors"
            )

    baseline_seeds = _terminal_seeds(left, "baseline")
    candidate_seeds = _terminal_seeds(right, "candidate")
    if set(baseline_seeds) != set(candidate_seeds):
        raise PairedRunComparisonError(
            "paired run evaluations completed different terminal seeds"
        )
    seed_comparisons = tuple(
        _compare_seed(seed, baseline_seeds[seed], candidate_seeds[seed])
        for seed in sorted(baseline_seeds)
    )
    aggregate = _aggregate(left, right, seed_comparisons)
    contract = {
        "schema": "sts-learning-paired-run-contract-v2",
        "comparison_scope": scope.value,
        "ascension_level": left["ascension_level"],
        "held_out_seed_start": left["held_out_seed_start"],
        "held_out_seed_end": left["held_out_seed_end"],
        "seed_partition_held_out_numerator": left[
            "seed_partition_held_out_numerator"
        ],
        "seed_partition_denominator": left["seed_partition_denominator"],
        "terminal_attempt_target": left["terminal_attempt_target"],
        "max_batch_steps": left["max_batch_steps"],
        "potion_lane": left["combat_potion_lane"],
        "policy_rng_seed": left["behavior_seed"],
        "policy_rng_scope": (
            "same_initial_stream_per_behavior_path_dependent_consumption"
            if scope is RunComparisonScope.FULL_BEHAVIOR
            else "same_initial_strategic_stream_combat_greedy_consumes_no_rng"
        ),
        "execution_model_definition_id": left[
            "execution_model_definition_id"
        ],
        "execution_model_config_id": left["execution_model_config_id"],
        "execution_behavior_rule_implementation_id": left[
            "execution_behavior_rule_implementation_id"
        ],
        "baseline_execution_behavior_rule_configuration_id": left[
            "execution_behavior_rule_configuration_id"
        ],
        "candidate_execution_behavior_rule_configuration_id": right[
            "execution_behavior_rule_configuration_id"
        ],
        "execution_semantic_schema_id": left["execution_semantic_schema_id"],
        "execution_semantic_schema_version": left[
            "execution_semantic_schema_version"
        ],
        "baseline_manifest_sha256": baseline_manifest,
        "candidate_manifest_sha256": candidate_manifest,
        "strategic_source_manifest_sha256": (
            None
            if scope is RunComparisonScope.FULL_BEHAVIOR
            else _sha256(
                left.get("execution_strategic_source_manifest_id"),
                "execution_strategic_source_manifest_id",
            )
        ),
        "baseline_combat_anchor_manifest_sha256": baseline_anchor,
        "candidate_combat_anchor_manifest_sha256": candidate_anchor,
        "terminal_seeds": tuple(sorted(baseline_seeds)),
    }
    contract_id = _content_sha256(contract)
    comparison_payload = {
        "contract_id": contract_id,
        "aggregate": aggregate,
        "seeds": seed_comparisons,
    }
    return {
        "schema": PAIRED_RUN_SCHEMA,
        "kind": "complete",
        "contract_id": contract_id,
        "comparison_id": _content_sha256(comparison_payload),
        "contract": contract,
        "baseline_evaluation_id": _content_sha256(left),
        "candidate_evaluation_id": _content_sha256(right),
        "aggregate": aggregate,
        "seeds": seed_comparisons,
    }


def run_paired_run_comparison(
    config: PairedRunComparisonConfig,
    *,
    combat_bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
    print_completion: bool = True,
) -> dict[str, object]:
    """Run two one-slot evaluations and publish exact per-seed differences."""

    if not isinstance(config, PairedRunComparisonConfig):
        raise PairedRunComparisonError(
            "paired run comparison config must be typed"
        )
    active_combat_bridge = combat_bridge or CombatSessionBridge.installed()
    active_run_bridge = run_bridge or CategoricalSessionBridge.installed()
    config.output.mkdir(parents=True, exist_ok=True)
    summaries = (
        _run_full_behavior_pair(
            config,
            active_combat_bridge,
            active_run_bridge,
        )
        if config.comparison_scope is RunComparisonScope.FULL_BEHAVIOR
        else _run_scoped_combat_pair(
            config,
            active_combat_bridge,
            active_run_bridge,
        )
    )
    comparison = compare_completed_run_evaluations(
        summaries["baseline"],
        summaries["candidate"],
        scope=config.comparison_scope,
    )
    comparison["baseline_evaluation"] = "baseline/evaluation.json"
    comparison["candidate_evaluation"] = "candidate/evaluation.json"
    destination = config.output / "paired-comparison.json"
    with destination.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(comparison, output, separators=(",", ":"), sort_keys=True)
        output.write("\n")
    if print_completion:
        aggregate = _mapping(comparison["aggregate"], "paired aggregate")
        print(
            json.dumps(
                {
                    "comparison": str(destination),
                    "comparison_id": comparison["comparison_id"],
                    "baseline_victories": aggregate["baseline_victories"],
                    "candidate_victories": aggregate["candidate_victories"],
                    "victory_delta": aggregate["victory_delta"],
                    "loss_to_win_seeds": aggregate["loss_to_win_seeds"],
                    "win_to_loss_seeds": aggregate["win_to_loss_seeds"],
                    "terminal_floor_sum_delta": aggregate[
                        "terminal_floor_sum_delta"
                    ],
                },
                separators=(",", ":"),
                sort_keys=True,
            ),
            flush=True,
        )
    return comparison


def _run_full_behavior_pair(
    config: PairedRunComparisonConfig,
    combat_bridge: CombatSessionBridge,
    run_bridge: CategoricalSessionBridge,
) -> dict[str, dict[str, object]]:
    summaries: dict[str, dict[str, object]] = {}
    for name, behavior in (
        ("baseline", config.baseline_behavior),
        ("candidate", config.candidate_behavior),
    ):
        summaries[name] = run_run_evaluation(
            _evaluation_config(config, behavior, name),
            combat_bridge=combat_bridge,
            run_bridge=run_bridge,
        )
    return summaries


def _run_scoped_combat_pair(
    config: PairedRunComparisonConfig,
    combat_bridge: CombatSessionBridge,
    run_bridge: CategoricalSessionBridge,
) -> dict[str, dict[str, object]]:
    if config.strategic_behavior is None:
        raise AssertionError("scoped comparison lost its strategic behavior")
    if not isinstance(combat_bridge, CombatSessionBridge):
        raise PairedRunComparisonError(
            "scoped run comparison combat bridge must be typed"
        )
    if not isinstance(run_bridge, CategoricalSessionBridge):
        raise PairedRunComparisonError(
            "scoped run comparison environment bridge must be typed"
        )
    if combat_bridge.semantic_schema != run_bridge.semantic_schema:
        raise PairedRunComparisonError(
            "scoped combat behavior and run environment schemas differ"
        )
    limits = CombatWinSessionLimits()
    summaries: dict[str, dict[str, object]] = {}
    for name, anchor_behavior in (
        ("baseline", config.baseline_behavior),
        ("candidate", config.candidate_behavior),
    ):
        strategic = recover_published_combat_behavior(
            config.strategic_behavior,
            combat_bridge,
            limits,
            (config.behavior_seed,),
        )
        anchor_source = recover_published_combat_behavior(
            anchor_behavior,
            combat_bridge,
            limits,
            (config.behavior_seed,),
        )
        anchor = FrozenCombatAnchor.from_behavior(anchor_source.policies[0])
        policy = FrozenCombatGreedyTorchPolicy.from_categorical(
            strategic.policies[0],
            None,
            anchor,
        )
        summaries[name] = run_recovered_run_evaluation(
            _evaluation_config(config, config.strategic_behavior, name),
            strategic,
            policy,
            run_bridge=run_bridge,
        )
    return summaries


def _evaluation_config(
    config: PairedRunComparisonConfig,
    behavior: Path,
    name: str,
) -> RunEvaluationCommandConfig:
    return RunEvaluationCommandConfig(
        behavior=behavior,
        output=config.output / name,
        slot_count=1,
        terminal_attempts=config.terminal_attempts,
        max_batch_steps=config.max_batch_steps,
        behavior_seed=config.behavior_seed,
        ascension_level=config.ascension_level,
        held_out_seed_start=config.held_out_seed_start,
        potion_lane=config.potion_lane,
    )


def _validate_scoped_combat_pair(
    baseline: Mapping[str, object],
    candidate: Mapping[str, object],
) -> None:
    expected_scope = "combat_anchor_greedy_strategic_source_sampled"
    for side, summary in (("baseline", baseline), ("candidate", candidate)):
        if summary.get("execution_scope") != expected_scope:
            raise PairedRunComparisonError(
                f"{side} run evaluation is not combat-anchor scoped"
            )
    for field in (
        "behavior_manifest_id",
        "execution_strategic_source_manifest_id",
        "execution_combat_anchor_model_definition_id",
        "execution_combat_anchor_model_config_id",
        "execution_combat_anchor_semantic_schema_id",
        "execution_combat_anchor_semantic_schema_version",
    ):
        if _canonical(baseline.get(field)) != _canonical(candidate.get(field)):
            raise PairedRunComparisonError(
                f"scoped run evaluations disagree on {field}"
            )


def _terminal_seeds(
    summary: Mapping[str, object],
    side: str,
) -> dict[int, Mapping[str, object]]:
    rows = _sequence(summary.get("combat_seed_summaries"), f"{side} seed summaries")
    terminal: dict[int, Mapping[str, object]] = {}
    for index, value in enumerate(rows):
        row = _mapping(value, f"{side} seed summary {index}")
        reward = row.get("terminal_reward")
        if reward is None:
            continue
        _terminal_reward(reward, f"{side} terminal reward")
        seed = _integer(row.get("seed"), f"{side} terminal seed")
        if seed in terminal:
            raise PairedRunComparisonError(
                f"{side} run evaluation repeats terminal seed {seed}"
            )
        terminal[seed] = row
    attempts = _integer(summary.get("terminal_attempts"), f"{side} attempts")
    if len(terminal) != attempts:
        raise PairedRunComparisonError(
            f"{side} run evaluation has incomplete terminal seed evidence"
        )
    return terminal


def _compare_seed(
    seed: int,
    baseline: Mapping[str, object],
    candidate: Mapping[str, object],
) -> dict[str, object]:
    left_reward = _terminal_reward(
        baseline.get("terminal_reward"),
        "baseline terminal reward",
    )
    right_reward = _terminal_reward(
        candidate.get("terminal_reward"),
        "candidate terminal reward",
    )
    axes = {
        name: _axis(
            _integer(baseline.get(field), f"baseline {field}"),
            _integer(candidate.get(field), f"candidate {field}"),
        )
        for name, field in (
            ("act", "last_act"),
            ("floor", "last_floor"),
            ("hp", "last_hp"),
            ("max_hp", "last_max_hp"),
            ("gold", "last_gold"),
            ("combat_count", "combat_count"),
            ("combat_hp_loss", "hp_loss_sum"),
        )
    }
    return {
        "seed": seed,
        "baseline_won": left_reward == 1,
        "candidate_won": right_reward == 1,
        "win_delta": int(right_reward == 1) - int(left_reward == 1),
        "axes": axes,
        "baseline_last_potion_ids": baseline.get("last_potion_ids"),
        "candidate_last_potion_ids": candidate.get("last_potion_ids"),
        "baseline_potion_identity_losses": baseline.get(
            "potion_identity_losses"
        ),
        "candidate_potion_identity_losses": candidate.get(
            "potion_identity_losses"
        ),
        "baseline_potion_identity_gains": baseline.get(
            "potion_identity_gains"
        ),
        "candidate_potion_identity_gains": candidate.get(
            "potion_identity_gains"
        ),
    }


def _aggregate(
    baseline: Mapping[str, object],
    candidate: Mapping[str, object],
    seeds: Sequence[Mapping[str, object]],
) -> dict[str, object]:
    floor_deltas = tuple(
        _integer(seed["axes"]["floor"]["delta"], "floor delta")
        for seed in seeds
    )
    common_wins = tuple(
        seed
        for seed in seeds
        if seed["baseline_won"] and seed["candidate_won"]
    )
    return {
        "terminal_seeds": len(seeds),
        "baseline_victories": _integer(
            baseline.get("victories"),
            "baseline victories",
        ),
        "candidate_victories": _integer(
            candidate.get("victories"),
            "candidate victories",
        ),
        "victory_delta": (
            _integer(candidate.get("victories"), "candidate victories")
            - _integer(baseline.get("victories"), "baseline victories")
        ),
        "loss_to_win_seeds": sum(seed["win_delta"] > 0 for seed in seeds),
        "win_to_loss_seeds": sum(seed["win_delta"] < 0 for seed in seeds),
        "candidate_deeper_seeds": sum(delta > 0 for delta in floor_deltas),
        "same_terminal_floor_seeds": sum(delta == 0 for delta in floor_deltas),
        "candidate_shallower_seeds": sum(delta < 0 for delta in floor_deltas),
        "baseline_terminal_floor_sum": _integer(
            baseline.get("terminal_floor_sum"),
            "baseline terminal floor sum",
        ),
        "candidate_terminal_floor_sum": _integer(
            candidate.get("terminal_floor_sum"),
            "candidate terminal floor sum",
        ),
        "terminal_floor_sum_delta": (
            _integer(
                candidate.get("terminal_floor_sum"),
                "candidate terminal floor sum",
            )
            - _integer(
                baseline.get("terminal_floor_sum"),
                "baseline terminal floor sum",
            )
        ),
        "common_victory_seeds": len(common_wins),
        "common_victory_final_hp_delta": sum(
            _integer(seed["axes"]["hp"]["delta"], "common victory HP delta")
            for seed in common_wins
        ),
        "baseline_batch_steps": _integer(
            baseline.get("batch_steps"),
            "baseline batch steps",
        ),
        "candidate_batch_steps": _integer(
            candidate.get("batch_steps"),
            "candidate batch steps",
        ),
    }


def _axis(baseline: int, candidate: int) -> dict[str, int]:
    return {
        "baseline": baseline,
        "candidate": candidate,
        "delta": candidate - baseline,
    }


def _mapping(value: object, name: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise PairedRunComparisonError(f"{name} must be a mapping")
    return value


def _sequence(value: object, name: str) -> tuple[object, ...]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise PairedRunComparisonError(f"{name} must be a sequence")
    return tuple(value)


def _integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise PairedRunComparisonError(f"{name} must be an integer")
    try:
        return operator.index(value)
    except TypeError as error:
        raise PairedRunComparisonError(f"{name} must be an integer") from error


def _terminal_reward(value: object, name: str) -> int:
    reward = _integer(value, name)
    if reward not in (-1, 1):
        raise PairedRunComparisonError(f"{name} must be -1 or 1")
    return reward


def _sha256(value: object, name: str) -> str:
    if not isinstance(value, str) or len(value) != 64:
        raise PairedRunComparisonError(f"{name} must be a SHA-256 hex digest")
    try:
        bytes.fromhex(value)
    except ValueError as error:
        raise PairedRunComparisonError(
            f"{name} must be a SHA-256 hex digest"
        ) from error
    return value.lower()


def _canonical(value: object) -> str:
    return json.dumps(value, separators=(",", ":"), sort_keys=True)


def _content_sha256(value: object) -> str:
    return hashlib.sha256(_canonical(value).encode("utf-8")).hexdigest()


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Compare two frozen behaviors over exactly paired complete runs."
        ),
    )
    parser.add_argument("--baseline-behavior", type=Path, required=True)
    parser.add_argument("--candidate-behavior", type=Path, required=True)
    parser.add_argument("--strategic-behavior", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--attempts", type=int, default=8)
    parser.add_argument("--max-batch-steps", type=int, default=4096)
    parser.add_argument("--behavior-seed", type=int, default=10_000)
    parser.add_argument("--ascension", type=int, choices=range(21), required=True)
    parser.add_argument("--held-out-seed-start", type=int, default=0)
    parser.add_argument(
        "--potion-lane",
        choices=tuple(lane.value for lane in RunPotionLane),
        default=RunPotionLane.NEVER.value,
    )
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    try:
        run_paired_run_comparison(
            PairedRunComparisonConfig(
                baseline_behavior=arguments.baseline_behavior,
                candidate_behavior=arguments.candidate_behavior,
                output=arguments.output,
                terminal_attempts=arguments.attempts,
                max_batch_steps=arguments.max_batch_steps,
                behavior_seed=arguments.behavior_seed,
                ascension_level=arguments.ascension,
                held_out_seed_start=arguments.held_out_seed_start,
                potion_lane=RunPotionLane(arguments.potion_lane),
                strategic_behavior=arguments.strategic_behavior,
            )
        )
    except (PairedRunComparisonError, RunEvaluationCommandError) as error:
        raise SystemExit(str(error)) from error
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
