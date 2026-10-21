"""Compare one reloaded search-distillation candidate on fresh natural combats."""

from __future__ import annotations

import argparse
import hashlib
import json
import operator
import statistics
import time
from collections.abc import Mapping, Sequence
from pathlib import Path

from .combat_driver import CombatGroupDriver
from .combat_experience import CombatExperienceLimits
from .combat_root_artifacts import load_combat_root_source, read_combat_root_artifact
from .combat_root_audit import read_combat_root_audits
from .combat_search_distillation_candidate import (
    recover_combat_search_distillation_candidate,
)
from .policy import BehaviorManifestId
from .published_combat_behavior import recover_compatible_combat_scorer
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits
from .torch_policy import GreedyTorchPolicy, RaggedCandidateScorer


class CombatSearchCandidateEvaluationError(RuntimeError):
    """A fresh-root candidate comparison was incomplete or inconsistent."""


def run_combat_search_candidate_evaluation(
    *,
    artifact: Path,
    root_count: int,
    baseline_behavior: Path,
    candidate: Path,
    output: Path,
    replicates: int,
    max_artifact_bytes: int,
    max_experience_payload_bytes: int,
) -> dict[str, object]:
    """Let baseline and candidate each play every exact root without search."""

    roots = _positive(root_count, "root_count")
    replicate_count = _positive(replicates, "replicates")
    if replicate_count < 2:
        raise CombatSearchCandidateEvaluationError(
            "candidate evaluation requires at least two replicates"
        )
    artifact_limit = _positive(max_artifact_bytes, "max_artifact_bytes")
    experience_payload_limit = _positive(
        max_experience_payload_bytes,
        "max_experience_payload_bytes",
    )
    artifact_path = Path(artifact).resolve()
    output_path = Path(output).resolve()
    if output_path.exists() or not output_path.parent.is_dir():
        raise CombatSearchCandidateEvaluationError(
            "evaluation output must be a fresh file below an existing directory"
        )
    bridge = CombatSessionBridge.installed()
    limits = CombatWinSessionLimits(max_artifact_bytes=artifact_limit)
    payload = read_combat_root_artifact(
        artifact_path,
        max_bytes=artifact_limit,
    )
    source = load_combat_root_source(
        bridge,
        payload,
        expected_roots=roots,
        max_bytes=artifact_limit,
    )
    audits = read_combat_root_audits(source, tuple(range(roots)))
    baseline = recover_compatible_combat_scorer(
        baseline_behavior,
        bridge,
        limits,
    )
    restored = recover_combat_search_distillation_candidate(
        candidate,
        bridge,
        limits,
    )
    if restored.source_manifest_id != baseline.source_manifest_id:
        raise CombatSearchCandidateEvaluationError(
            "candidate and frozen baseline do not share the same source manifest"
        )
    rollout_limits = CombatExperienceLimits(
        max_decisions=4_096,
        max_payload_bytes=experience_payload_limit,
        max_model_rounds=2_048,
        max_transitions=8_192,
    )
    started = time.perf_counter()
    rows: list[dict[str, object]] = []
    for slot, audit in enumerate(audits):
        try:
            baseline_result = _play_root(
                source,
                slot,
                replicate_count,
                baseline.scorer,
                baseline.source_manifest_id,
                rollout_limits,
            )
        except Exception as error:
            raise CombatSearchCandidateEvaluationError(
                f"baseline evaluation failed at root slot {slot}: {error}"
            ) from error
        try:
            candidate_result = _play_root(
                source,
                slot,
                replicate_count,
                restored.scorer,
                restored.manifest_id,
                rollout_limits,
            )
        except Exception as error:
            raise CombatSearchCandidateEvaluationError(
                f"candidate evaluation failed at root slot {slot}: {error}"
            ) from error
        if (
            baseline_result["root_id"] != candidate_result["root_id"]
            or baseline_result["exact_combat_state_hash"]
            != candidate_result["exact_combat_state_hash"]
        ):
            raise CombatSearchCandidateEvaluationError(
                "baseline and candidate did not play the same exact root"
            )
        baseline_quality = _quality(baseline_result)
        candidate_quality = _quality(candidate_result)
        if candidate_quality > baseline_quality:
            status = "improved"
        elif candidate_quality == baseline_quality:
            status = "equal"
        else:
            status = "regressed"
        rows.append(
            {
                "slot": slot,
                "audit": audit.as_mapping(),
                "root_id": baseline_result["root_id"],
                "exact_combat_state_hash": baseline_result[
                    "exact_combat_state_hash"
                ],
                "baseline": baseline_result,
                "candidate": candidate_result,
                "status": status,
                "both_all_win_final_hp_delta": (
                    candidate_quality[1] - baseline_quality[1]
                    if baseline_quality[0]
                    == candidate_quality[0]
                    == replicate_count
                    else None
                ),
            }
        )
    result = {
        "schema": "sts-learning-combat-search-candidate-evaluation-v1",
        "claim": "fresh_natural_roots_full_combat_comparison_only",
        "teacher_valid": False,
        "model_published": False,
        "production_eligible": False,
        "artifact": str(artifact_path),
        "artifact_sha256": hashlib.sha256(payload).hexdigest(),
        "root_count": roots,
        "replicates_per_root": replicate_count,
        "max_experience_payload_bytes": experience_payload_limit,
        "baseline_behavior": str(Path(baseline_behavior).resolve()),
        "baseline_manifest_id": baseline.source_manifest_id.digest.hex(),
        "candidate": str(Path(candidate).resolve()),
        "candidate_id": restored.candidate_id,
        "candidate_manifest_id": restored.manifest_id.digest.hex(),
        "candidate_checkpoint_id": restored.checkpoint_id.digest.hex(),
        "baseline": _summarize(rows, "baseline", replicate_count),
        "candidate_result": _summarize(rows, "candidate", replicate_count),
        "comparison": _comparison(rows, replicate_count),
        "by_ascension": {
            str(ascension): {
                "baseline": _summarize(
                    [
                        row
                        for row in rows
                        if row["audit"]["ascension_level"] == ascension
                    ],
                    "baseline",
                    replicate_count,
                ),
                "candidate": _summarize(
                    [
                        row
                        for row in rows
                        if row["audit"]["ascension_level"] == ascension
                    ],
                    "candidate",
                    replicate_count,
                ),
            }
            for ascension in sorted(
                {row["audit"]["ascension_level"] for row in rows}
            )
        },
        "rows": tuple(rows),
        "elapsed_seconds": time.perf_counter() - started,
    }
    with output_path.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(result, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    receipt = {
        "schema": result["schema"],
        "artifact": str(output_path),
        "root_count": roots,
        "replicates_per_root": replicate_count,
        "max_experience_payload_bytes": experience_payload_limit,
        "baseline": result["baseline"],
        "candidate": result["candidate_result"],
        "comparison": result["comparison"],
    }
    print(json.dumps(receipt, separators=(",", ":"), sort_keys=True), flush=True)
    return result


def _play_root(
    source: object,
    slot: int,
    replicates: int,
    scorer: RaggedCandidateScorer,
    manifest_id: BehaviorManifestId,
    limits: CombatExperienceLimits,
) -> dict[str, object]:
    env = source.combat_group(slot, replicates, potion_slots=[])
    run = CombatGroupDriver(
        env,
        GreedyTorchPolicy(scorer, manifest_id),
        limits,
    ).run()
    outcomes = tuple(run.experience.outcomes.outcomes)
    if len(outcomes) != replicates:
        raise CombatSearchCandidateEvaluationError(
            "combat evaluation did not terminate every replicate"
        )
    if any(outcome.potions_used or outcome.potions_discarded for outcome in outcomes):
        raise CombatSearchCandidateEvaluationError(
            "no-potion candidate evaluation observed a potion action"
        )
    return {
        "root_id": str(env.root_id),
        "exact_combat_state_hash": str(env.exact_combat_state_hash),
        "win_count": sum(outcome.won for outcome in outcomes),
        "loss_count": sum(not outcome.won for outcome in outcomes),
        "final_hp_sum": sum(outcome.final_hp for outcome in outcomes),
        "outcomes": tuple(
            {
                "replicate_index": outcome.replicate_index,
                "terminal_kind": outcome.terminal_kind.name.lower(),
                "won": outcome.won,
                "start_hp": outcome.start_hp,
                "final_hp": outcome.final_hp,
                "hp_loss": outcome.hp_loss,
                "enemy_start_hp": outcome.enemy_start_hp,
                "enemy_final_hp": outcome.enemy_final_hp,
                "turns": outcome.turns,
                "cards_played": outcome.cards_played,
            }
            for outcome in outcomes
        ),
        "decision_count": run.experience.decision_count,
        "model_rounds": run.model_rounds,
        "transitions": run.transitions,
    }


def _quality(result: Mapping[str, object]) -> tuple[int, int]:
    return (
        operator.index(result["win_count"]),
        operator.index(result["final_hp_sum"]),
    )


def _summarize(
    rows: Sequence[Mapping[str, object]],
    key: str,
    replicates: int,
) -> dict[str, object]:
    results = tuple(_mapping(row[key], key) for row in rows)
    wins = sum(operator.index(result["win_count"]) for result in results)
    final_hp_sums = [operator.index(result["final_hp_sum"]) for result in results]
    return {
        "root_count": len(results),
        "win_count": wins,
        "loss_count": len(results) * replicates - wins,
        "win_rate": wins / (len(results) * replicates),
        "all_win_root_count": sum(
            operator.index(result["win_count"]) == replicates
            for result in results
        ),
        "mean_final_hp": statistics.fmean(final_hp_sums) / replicates,
    }


def _comparison(
    rows: Sequence[Mapping[str, object]],
    replicates: int,
) -> dict[str, object]:
    both_win_hp_deltas = [
        operator.index(row["both_all_win_final_hp_delta"]) / replicates
        for row in rows
        if row["both_all_win_final_hp_delta"] is not None
    ]
    baseline_wins = sum(
        operator.index(_mapping(row["baseline"], "baseline")["win_count"])
        for row in rows
    )
    candidate_wins = sum(
        operator.index(_mapping(row["candidate"], "candidate")["win_count"])
        for row in rows
    )
    return {
        "improved_root_count": sum(row["status"] == "improved" for row in rows),
        "equal_root_count": sum(row["status"] == "equal" for row in rows),
        "regressed_root_count": sum(row["status"] == "regressed" for row in rows),
        "win_count_delta": candidate_wins - baseline_wins,
        "mean_both_all_win_final_hp_delta": (
            statistics.fmean(both_win_hp_deltas)
            if both_win_hp_deltas
            else None
        ),
    }


def _mapping(value: object, name: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise CombatSearchCandidateEvaluationError(f"{name} must be an object")
    return value


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatSearchCandidateEvaluationError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatSearchCandidateEvaluationError(
            f"{name} must be an integer"
        ) from error
    if normalized <= 0:
        raise CombatSearchCandidateEvaluationError(f"{name} must be positive")
    return normalized


def _parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--roots", type=int, required=True)
    parser.add_argument("--baseline-behavior", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--replicates", type=int, default=2)
    parser.add_argument("--max-artifact-bytes", type=int, default=16 * 1024 * 1024)
    parser.add_argument(
        "--max-experience-payload-bytes",
        type=int,
        default=64 * 1024 * 1024,
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parse_args(argv)
    run_combat_search_candidate_evaluation(
        artifact=arguments.artifact,
        root_count=arguments.roots,
        baseline_behavior=arguments.baseline_behavior,
        candidate=arguments.candidate,
        output=arguments.output,
        replicates=arguments.replicates,
        max_artifact_bytes=arguments.max_artifact_bytes,
        max_experience_payload_bytes=arguments.max_experience_payload_bytes,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
