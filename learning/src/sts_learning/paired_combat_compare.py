"""Run and compare two frozen combat behaviors on identical exact roots."""

from __future__ import annotations

import argparse
import hashlib
import json
import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

from .combat_potion_lane import (
    CombatPotionLane,
    CombatPotionLaneError,
    normalize_combat_potion_slots,
)
from .evaluate_combat import (
    CombatEvaluationCommandConfig,
    run_combat_evaluation,
)
from .torch_behavior import FrozenDecisionRule
from .torch_combat_session_config import CombatSessionBridge


PAIRED_COMBAT_SCHEMA = "sts-learning-paired-combat-comparison-v2"


class PairedCombatComparisonError(RuntimeError):
    """Two combat evaluations are incomplete, mismatched, or not comparable."""


@dataclass(frozen=True)
class PairedCombatComparisonConfig:
    artifact: Path
    baseline_behavior: Path
    candidate_behavior: Path
    output: Path
    root_count: int
    replicate_count: int
    behavior_seed_base: int
    decision_rule: FrozenDecisionRule = FrozenDecisionRule.GREEDY
    potion_lane: CombatPotionLane = CombatPotionLane.NEVER
    potion_slots: tuple[int, ...] = ()

    def __post_init__(self) -> None:
        artifact = Path(self.artifact).resolve()
        baseline = Path(self.baseline_behavior).resolve()
        candidate = Path(self.candidate_behavior).resolve()
        output = Path(self.output).resolve()
        if not artifact.is_file():
            raise PairedCombatComparisonError(
                "paired combat artifact is not a file"
            )
        if not baseline.is_dir() or not candidate.is_dir():
            raise PairedCombatComparisonError(
                "paired combat behaviors must be directories"
            )
        if baseline == candidate:
            raise PairedCombatComparisonError(
                "paired combat comparison requires distinct behavior directories"
            )
        if output.exists() and (not output.is_dir() or any(output.iterdir())):
            raise PairedCombatComparisonError(
                "paired combat output must be absent or empty"
            )
        if any(
            output == behavior or behavior in output.parents
            for behavior in (baseline, candidate)
        ):
            raise PairedCombatComparisonError(
                "paired combat output must stay outside behavior directories"
            )
        root_count = _positive(self.root_count, "root_count")
        replicate_count = _positive(self.replicate_count, "replicate_count")
        if replicate_count < 2:
            raise PairedCombatComparisonError(
                "paired combat comparison requires at least two replicates"
            )
        behavior_seed_base = _seed(self.behavior_seed_base, "behavior_seed_base")
        if behavior_seed_base + root_count * replicate_count > 1 << 63:
            raise PairedCombatComparisonError(
                "paired combat behavior seeds must stay below 2^63"
            )
        if not isinstance(self.decision_rule, FrozenDecisionRule):
            raise PairedCombatComparisonError(
                "paired combat decision_rule must be typed"
            )
        if not isinstance(self.potion_lane, CombatPotionLane):
            raise PairedCombatComparisonError(
                "paired combat potion lane must be typed"
            )
        try:
            potion_slots = normalize_combat_potion_slots(
                self.potion_lane,
                self.potion_slots,
            )
        except CombatPotionLaneError as error:
            raise PairedCombatComparisonError(str(error)) from error
        object.__setattr__(self, "artifact", artifact)
        object.__setattr__(self, "baseline_behavior", baseline)
        object.__setattr__(self, "candidate_behavior", candidate)
        object.__setattr__(self, "output", output)
        object.__setattr__(self, "root_count", root_count)
        object.__setattr__(self, "replicate_count", replicate_count)
        object.__setattr__(self, "behavior_seed_base", behavior_seed_base)
        object.__setattr__(self, "potion_slots", potion_slots)

    @property
    def behavior_seeds(self) -> tuple[tuple[int, ...], ...]:
        """Explicit root-major policy RNG seed matrix."""

        return tuple(
            tuple(
                self.behavior_seed_base
                + root_index * self.replicate_count
                + replicate_index
                for replicate_index in range(self.replicate_count)
            )
            for root_index in range(self.root_count)
        )


def compare_completed_combat_evaluations(
    baseline: Mapping[str, object],
    candidate: Mapping[str, object],
) -> dict[str, object]:
    """Produce raw paired differences without declaring either behavior better."""

    left = _mapping(baseline, "baseline evaluation")
    right = _mapping(candidate, "candidate evaluation")
    for name, summary in (("baseline", left), ("candidate", right)):
        if summary.get("kind") != "completed":
            raise PairedCombatComparisonError(
                f"{name} combat evaluation is not complete"
            )
        if summary.get("schema") is None:
            raise PairedCombatComparisonError(
                f"{name} combat evaluation omitted its schema"
            )
    for field in (
        "schema",
        "artifact_sha256",
        "root_count",
        "replicate_count",
        "decision_rule",
        "potion_lane",
        "potion_slots",
        "behavior_seed_scope",
        "behavior_seeds",
    ):
        if _canonical(left.get(field)) != _canonical(right.get(field)):
            raise PairedCombatComparisonError(
                f"paired combat evaluations disagree on {field}"
            )
    try:
        decision_rule = FrozenDecisionRule(left.get("decision_rule"))
    except ValueError as error:
        raise PairedCombatComparisonError(
            "paired combat comparison has an unknown decision rule"
        ) from error
    if left.get("behavior_seed_scope") != "root_replicate_independent":
        raise PairedCombatComparisonError(
            "paired combat comparison requires root-replicate RNG ownership"
        )
    behavior_seeds = _seed_matrix(
        left.get("behavior_seeds"),
        root_count=_positive(left.get("root_count"), "root_count"),
        replicate_count=_positive(
            left.get("replicate_count"),
            "replicate_count",
        ),
    )
    baseline_manifest = _sha256(
        left.get("behavior_manifest_id"),
        "baseline behavior_manifest_id",
    )
    candidate_manifest = _sha256(
        right.get("behavior_manifest_id"),
        "candidate behavior_manifest_id",
    )
    if baseline_manifest == candidate_manifest:
        raise PairedCombatComparisonError(
            "paired combat evaluations use the same behavior manifest"
        )
    baseline_roots = _sequence(left.get("roots"), "baseline roots")
    candidate_roots = _sequence(right.get("roots"), "candidate roots")
    if len(baseline_roots) != len(candidate_roots):
        raise PairedCombatComparisonError(
            "paired combat evaluations have different root counts"
        )

    root_comparisons = tuple(
        _compare_root(root_index, baseline_root, candidate_root)
        for root_index, (baseline_root, candidate_root) in enumerate(
            zip(baseline_roots, candidate_roots, strict=True)
        )
    )
    contract = {
        "schema": "sts-learning-paired-combat-contract-v2",
        "artifact_sha256": left["artifact_sha256"],
        "root_count": left["root_count"],
        "replicate_count": left["replicate_count"],
        "decision_rule": left["decision_rule"],
        "potion_lane": left["potion_lane"],
        "potion_slots": left["potion_slots"],
        "behavior_seeds": behavior_seeds,
        "seed_scope": (
            "root_replicate_independent_policy_rng"
            if decision_rule is FrozenDecisionRule.SAMPLED
            else "root_replicate_declared_greedy_policy_consumes_no_rng"
        ),
        "baseline_manifest_sha256": baseline_manifest,
        "candidate_manifest_sha256": candidate_manifest,
        "baseline_checkpoint_sha256": _sha256(
            left.get("behavior_checkpoint_id"),
            "baseline behavior_checkpoint_id",
        ),
        "candidate_checkpoint_sha256": _sha256(
            right.get("behavior_checkpoint_id"),
            "candidate behavior_checkpoint_id",
        ),
    }
    contract_id = _content_sha256(contract)
    aggregate = _aggregate_comparison(left, right, root_comparisons)
    comparison_payload = {
        "contract_id": contract_id,
        "roots": root_comparisons,
        "aggregate": aggregate,
    }
    return {
        "schema": PAIRED_COMBAT_SCHEMA,
        "kind": "complete",
        "contract_id": contract_id,
        "comparison_id": _content_sha256(comparison_payload),
        "contract": contract,
        "baseline_evaluation_manifest_id": _sha256(
            left.get("evaluation_manifest_id"),
            "baseline evaluation_manifest_id",
        ),
        "candidate_evaluation_manifest_id": _sha256(
            right.get("evaluation_manifest_id"),
            "candidate evaluation_manifest_id",
        ),
        "aggregate": aggregate,
        "roots": root_comparisons,
    }


def run_paired_combat_comparison(
    config: PairedCombatComparisonConfig,
    *,
    bridge: CombatSessionBridge | None = None,
    print_completion: bool = True,
) -> dict[str, object]:
    """Run both immutable evaluations and publish their exact paired differences."""

    if not isinstance(config, PairedCombatComparisonConfig):
        raise PairedCombatComparisonError(
            "paired combat comparison config must be typed"
        )
    config.output.mkdir(parents=True, exist_ok=True)
    baseline_summary = run_combat_evaluation(
        CombatEvaluationCommandConfig(
            artifact=config.artifact,
            behavior=config.baseline_behavior,
            output=config.output / "baseline",
            root_count=config.root_count,
            replicate_count=config.replicate_count,
            behavior_seed_base=config.behavior_seed_base,
            decision_rule=config.decision_rule,
            potion_lane=config.potion_lane,
            potion_slots=config.potion_slots,
        ),
        bridge=bridge,
        print_completion=False,
    )
    candidate_summary = run_combat_evaluation(
        CombatEvaluationCommandConfig(
            artifact=config.artifact,
            behavior=config.candidate_behavior,
            output=config.output / "candidate",
            root_count=config.root_count,
            replicate_count=config.replicate_count,
            behavior_seed_base=config.behavior_seed_base,
            decision_rule=config.decision_rule,
            potion_lane=config.potion_lane,
            potion_slots=config.potion_slots,
        ),
        bridge=bridge,
        print_completion=False,
    )
    comparison = compare_completed_combat_evaluations(
        baseline_summary,
        candidate_summary,
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
                    "baseline_wins": aggregate["baseline_wins"],
                    "candidate_wins": aggregate["candidate_wins"],
                    "win_delta": aggregate["win_delta"],
                    "regressed_replicates": aggregate["regressed_replicates"],
                    "improved_replicates": aggregate["improved_replicates"],
                },
                separators=(",", ":"),
                sort_keys=True,
            ),
            flush=True,
        )
    return comparison


def _compare_root(
    root_index: int,
    baseline_value: object,
    candidate_value: object,
) -> dict[str, object]:
    baseline = _mapping(baseline_value, f"baseline root {root_index}")
    candidate = _mapping(candidate_value, f"candidate root {root_index}")
    for field in ("slot_index", "root_id", "exact_combat_state_hash", "context"):
        if _canonical(baseline.get(field)) != _canonical(candidate.get(field)):
            raise PairedCombatComparisonError(
                f"paired combat root {root_index} disagrees on {field}"
            )
    baseline_outcomes = _sequence(
        baseline.get("outcomes"),
        f"baseline root {root_index} outcomes",
    )
    candidate_outcomes = _sequence(
        candidate.get("outcomes"),
        f"candidate root {root_index} outcomes",
    )
    if len(baseline_outcomes) != len(candidate_outcomes):
        raise PairedCombatComparisonError(
            f"paired combat root {root_index} has different replicate counts"
        )
    outcomes = tuple(
        _compare_outcome(root_index, replicate_index, left, right)
        for replicate_index, (left, right) in enumerate(
            zip(baseline_outcomes, candidate_outcomes, strict=True)
        )
    )
    return {
        "slot_index": baseline["slot_index"],
        "root_id": baseline["root_id"],
        "exact_combat_state_hash": baseline["exact_combat_state_hash"],
        "context": baseline["context"],
        "baseline_wins": _integer(baseline.get("wins"), "baseline root wins"),
        "candidate_wins": _integer(candidate.get("wins"), "candidate root wins"),
        "win_delta": (
            _integer(candidate.get("wins"), "candidate root wins")
            - _integer(baseline.get("wins"), "baseline root wins")
        ),
        "baseline_final_hp_sum": _integer(
            baseline.get("final_hp_sum"),
            "baseline root final_hp_sum",
        ),
        "candidate_final_hp_sum": _integer(
            candidate.get("final_hp_sum"),
            "candidate root final_hp_sum",
        ),
        "final_hp_sum_delta": (
            _integer(candidate.get("final_hp_sum"), "candidate root final_hp_sum")
            - _integer(baseline.get("final_hp_sum"), "baseline root final_hp_sum")
        ),
        "baseline_enemy_final_hp_sum": _integer(
            baseline.get("enemy_final_hp_sum"),
            "baseline root enemy_final_hp_sum",
        ),
        "candidate_enemy_final_hp_sum": _integer(
            candidate.get("enemy_final_hp_sum"),
            "candidate root enemy_final_hp_sum",
        ),
        "enemy_final_hp_sum_delta": (
            _integer(
                candidate.get("enemy_final_hp_sum"),
                "candidate root enemy_final_hp_sum",
            )
            - _integer(
                baseline.get("enemy_final_hp_sum"),
                "baseline root enemy_final_hp_sum",
            )
        ),
        "improved_replicates": sum(
            outcome["win_delta"] > 0 for outcome in outcomes
        ),
        "regressed_replicates": sum(
            outcome["win_delta"] < 0 for outcome in outcomes
        ),
        "outcomes": outcomes,
    }


def _compare_outcome(
    root_index: int,
    replicate_index: int,
    baseline_value: object,
    candidate_value: object,
) -> dict[str, object]:
    baseline = _mapping(
        baseline_value,
        f"baseline root {root_index} replicate {replicate_index}",
    )
    candidate = _mapping(
        candidate_value,
        f"candidate root {root_index} replicate {replicate_index}",
    )
    if (
        _integer(baseline.get("replicate_index"), "baseline replicate index")
        != replicate_index
        or _integer(candidate.get("replicate_index"), "candidate replicate index")
        != replicate_index
    ):
        raise PairedCombatComparisonError(
            f"paired combat root {root_index} replicate identity is misaligned"
        )
    baseline_won = _boolean(baseline.get("won"), "baseline won")
    candidate_won = _boolean(candidate.get("won"), "candidate won")
    numeric_axes = (
        "terminal_kind",
        "final_hp",
        "hp_loss",
        "enemy_final_hp",
        "final_gold",
        "gold_delta",
        "potions_used",
        "potions_discarded",
        "turns",
        "cards_played",
    )
    axes = {
        field: {
            "baseline": _integer(baseline.get(field), f"baseline {field}"),
            "candidate": _integer(candidate.get(field), f"candidate {field}"),
            "delta": (
                _integer(candidate.get(field), f"candidate {field}")
                - _integer(baseline.get(field), f"baseline {field}")
            ),
        }
        for field in numeric_axes
    }
    return {
        "replicate_index": replicate_index,
        "baseline_won": baseline_won,
        "candidate_won": candidate_won,
        "win_delta": int(candidate_won) - int(baseline_won),
        "axes": axes,
        "baseline_final_potion_ids": baseline.get("final_potion_ids"),
        "candidate_final_potion_ids": candidate.get("final_potion_ids"),
        "baseline_lost_potion_ids": baseline.get("lost_potion_ids"),
        "candidate_lost_potion_ids": candidate.get("lost_potion_ids"),
        "baseline_gained_potion_ids": baseline.get("gained_potion_ids"),
        "candidate_gained_potion_ids": candidate.get("gained_potion_ids"),
    }


def _aggregate_comparison(
    baseline: Mapping[str, object],
    candidate: Mapping[str, object],
    roots: Sequence[Mapping[str, object]],
) -> dict[str, object]:
    baseline_wins = _integer(baseline.get("wins"), "baseline wins")
    candidate_wins = _integer(candidate.get("wins"), "candidate wins")
    baseline_final_hp = _integer(
        baseline.get("final_hp_sum"),
        "baseline final_hp_sum",
    )
    candidate_final_hp = _integer(
        candidate.get("final_hp_sum"),
        "candidate final_hp_sum",
    )
    baseline_enemy_hp = _integer(
        baseline.get("enemy_final_hp_sum"),
        "baseline enemy_final_hp_sum",
    )
    candidate_enemy_hp = _integer(
        candidate.get("enemy_final_hp_sum"),
        "candidate enemy_final_hp_sum",
    )
    return {
        "baseline_wins": baseline_wins,
        "candidate_wins": candidate_wins,
        "win_delta": candidate_wins - baseline_wins,
        "baseline_final_hp_sum": baseline_final_hp,
        "candidate_final_hp_sum": candidate_final_hp,
        "final_hp_sum_delta": candidate_final_hp - baseline_final_hp,
        "baseline_enemy_final_hp_sum": baseline_enemy_hp,
        "candidate_enemy_final_hp_sum": candidate_enemy_hp,
        "enemy_final_hp_sum_delta": candidate_enemy_hp - baseline_enemy_hp,
        "improved_replicates": sum(
            _integer(root.get("improved_replicates"), "improved_replicates")
            for root in roots
        ),
        "regressed_replicates": sum(
            _integer(root.get("regressed_replicates"), "regressed_replicates")
            for root in roots
        ),
        "improved_roots": sum(
            _integer(root.get("win_delta"), "root win_delta") > 0
            for root in roots
        ),
        "regressed_roots": sum(
            _integer(root.get("win_delta"), "root win_delta") < 0
            for root in roots
        ),
    }


def _canonical(value: object) -> bytes:
    try:
        return json.dumps(
            value,
            ensure_ascii=True,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise PairedCombatComparisonError(
            "paired combat value is not canonical JSON"
        ) from error


def _content_sha256(value: object) -> str:
    return hashlib.sha256(_canonical(value)).hexdigest()


def _mapping(value: object, name: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise PairedCombatComparisonError(f"{name} must be a mapping")
    return value


def _sequence(value: object, name: str) -> tuple[object, ...]:
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise PairedCombatComparisonError(f"{name} must be a sequence")
    return tuple(value)


def _sha256(value: object, name: str) -> str:
    if not isinstance(value, str) or len(value) != 64:
        raise PairedCombatComparisonError(f"{name} must be lowercase SHA-256 text")
    try:
        bytes.fromhex(value)
    except ValueError as error:
        raise PairedCombatComparisonError(
            f"{name} must be lowercase SHA-256 text"
        ) from error
    if value != value.lower():
        raise PairedCombatComparisonError(f"{name} must be lowercase SHA-256 text")
    return value


def _boolean(value: object, name: str) -> bool:
    if not isinstance(value, bool):
        raise PairedCombatComparisonError(f"{name} must be boolean")
    return value


def _positive(value: object, name: str) -> int:
    normalized = _integer(value, name)
    if normalized <= 0:
        raise PairedCombatComparisonError(f"{name} must be positive")
    return normalized


def _integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise PairedCombatComparisonError(f"{name} must be an integer, not bool")
    try:
        return operator.index(value)
    except TypeError as error:
        raise PairedCombatComparisonError(f"{name} must be an integer") from error


def _seed(value: object, name: str) -> int:
    normalized = _integer(value, name)
    if normalized < 0 or normalized >= 1 << 63:
        raise PairedCombatComparisonError(f"{name} must be in 0..2^63")
    return normalized


def _seed_matrix(
    value: object,
    *,
    root_count: int,
    replicate_count: int,
) -> tuple[tuple[int, ...], ...]:
    rows = _sequence(value, "behavior_seeds")
    if len(rows) != root_count:
        raise PairedCombatComparisonError(
            "paired combat behavior seed roots are misaligned"
        )
    normalized = tuple(
        tuple(
            _seed(seed, f"behavior_seeds[{root_index}][{replicate_index}]")
            for replicate_index, seed in enumerate(
                _sequence(row, f"behavior_seeds[{root_index}]")
            )
        )
        for root_index, row in enumerate(rows)
    )
    if any(len(row) != replicate_count for row in normalized):
        raise PairedCombatComparisonError(
            "paired combat behavior seed replicates are misaligned"
        )
    flat = tuple(seed for row in normalized for seed in row)
    if len(set(flat)) != len(flat):
        raise PairedCombatComparisonError(
            "paired combat behavior RNG seeds must be distinct"
        )
    return normalized


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Compare two frozen behaviors on identical exact combat roots.",
    )
    parser.add_argument("--artifact", required=True, type=Path)
    parser.add_argument("--baseline-behavior", required=True, type=Path)
    parser.add_argument("--candidate-behavior", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--roots", required=True, type=int)
    parser.add_argument("--replicates", required=True, type=int)
    parser.add_argument("--behavior-seed-base", required=True, type=int)
    parser.add_argument(
        "--decision-rule",
        choices=tuple(rule.value for rule in FrozenDecisionRule),
        default=FrozenDecisionRule.GREEDY.value,
    )
    parser.add_argument(
        "--potion-lane",
        choices=tuple(lane.value for lane in CombatPotionLane),
        default=CombatPotionLane.NEVER.value,
    )
    parser.add_argument("--potion-slot", action="append", default=[], type=int)
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    try:
        config = PairedCombatComparisonConfig(
            artifact=arguments.artifact,
            baseline_behavior=arguments.baseline_behavior,
            candidate_behavior=arguments.candidate_behavior,
            output=arguments.output,
            root_count=arguments.roots,
            replicate_count=arguments.replicates,
            behavior_seed_base=arguments.behavior_seed_base,
            decision_rule=FrozenDecisionRule(arguments.decision_rule),
            potion_lane=CombatPotionLane(arguments.potion_lane),
            potion_slots=tuple(arguments.potion_slot),
        )
        run_paired_combat_comparison(config)
    except (PairedCombatComparisonError, OSError, ValueError) as error:
        raise SystemExit(str(error)) from error
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
