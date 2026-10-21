"""Run and aggregate equal-work Rust successor search over opaque chance roots."""

from __future__ import annotations

import hashlib
import json
import statistics
import subprocess
from collections import Counter
from collections.abc import Mapping, Sequence
from pathlib import Path


class ExactSuccessorSearchError(RuntimeError):
    """A Rust successor corpus was incomplete, misaligned, or malformed."""


def run_equal_work_successor_search(
    *,
    oracle_binary: Path,
    artifact: Path,
    root_count: int,
    candidate_count: int,
    output_dir: Path,
    solve_work_per_candidate: int,
    candidate_jobs: int,
    max_artifact_bytes: int,
    no_potions: bool = False,
) -> dict[str, object]:
    """Search every model action at every exact root and retain typed evidence."""

    oracle_binary = Path(oracle_binary).resolve()
    artifact = Path(artifact).resolve()
    output_dir = Path(output_dir).resolve()
    if not oracle_binary.is_file():
        raise ExactSuccessorSearchError("oracle binary does not exist")
    if not artifact.is_file():
        raise ExactSuccessorSearchError("chance root artifact does not exist")
    for value, name in (
        (root_count, "root_count"),
        (candidate_count, "candidate_count"),
        (solve_work_per_candidate, "solve_work_per_candidate"),
        (candidate_jobs, "candidate_jobs"),
        (max_artifact_bytes, "max_artifact_bytes"),
    ):
        if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
            raise ExactSuccessorSearchError(f"{name} must be a positive integer")
    if output_dir.exists():
        raise ExactSuccessorSearchError("successor search output must be a fresh directory")
    if type(no_potions) is not bool:
        raise ExactSuccessorSearchError("no_potions must be bool")
    output_dir.mkdir(parents=True)

    artifact_payload = artifact.read_bytes()
    if not artifact_payload or len(artifact_payload) > max_artifact_bytes:
        raise ExactSuccessorSearchError("chance root artifact violates its byte bound")
    roots = []
    for root_slot in range(root_count):
        corpus_path = output_dir / f"root-{root_slot:03}.json"
        roots.append(
            _run_root_successor_corpus(
                oracle_binary=oracle_binary,
                artifact=artifact,
                expected_roots=root_count,
                root_slot=root_slot,
                candidate_count=candidate_count,
                corpus_path=corpus_path,
                solve_work_per_candidate=solve_work_per_candidate,
                candidate_jobs=candidate_jobs,
                max_artifact_bytes=max_artifact_bytes,
                no_potions=no_potions,
            )
        )
    return {
        "schema": "sts-learning-equal-work-successor-search-v1",
        "engine": "rust_local_turn_graph_budget_or_exhaustion_v1",
        "artifact": str(artifact),
        "artifact_sha256": hashlib.sha256(artifact_payload).hexdigest(),
        "root_count": root_count,
        "candidate_count": candidate_count,
        "solve_work_per_candidate": solve_work_per_candidate,
        "candidate_jobs": candidate_jobs,
        "legacy_v2_teacher_enabled": False,
        "no_potions": no_potions,
        "roots": tuple(roots),
        "actions": _aggregate_actions(roots, candidate_count),
    }


def run_equal_work_successor_search_root(
    *,
    oracle_binary: Path,
    artifact: Path,
    expected_roots: int,
    root_slot: int,
    candidate_count: int,
    output_dir: Path,
    solve_work_per_candidate: int,
    candidate_jobs: int,
    max_artifact_bytes: int,
    no_potions: bool = False,
) -> dict[str, object]:
    """Search one selected slot of a heterogeneous natural-root artifact."""

    oracle_binary = Path(oracle_binary).resolve()
    artifact = Path(artifact).resolve()
    output_dir = Path(output_dir).resolve()
    if not oracle_binary.is_file():
        raise ExactSuccessorSearchError("oracle binary does not exist")
    if not artifact.is_file():
        raise ExactSuccessorSearchError("chance root artifact does not exist")
    for value, name in (
        (expected_roots, "expected_roots"),
        (candidate_count, "candidate_count"),
        (solve_work_per_candidate, "solve_work_per_candidate"),
        (candidate_jobs, "candidate_jobs"),
        (max_artifact_bytes, "max_artifact_bytes"),
    ):
        if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
            raise ExactSuccessorSearchError(f"{name} must be a positive integer")
    if isinstance(root_slot, bool) or not isinstance(root_slot, int):
        raise ExactSuccessorSearchError("root_slot must be an integer")
    if not 0 <= root_slot < expected_roots:
        raise ExactSuccessorSearchError("root_slot is outside the source artifact")
    if output_dir.exists():
        raise ExactSuccessorSearchError("successor search output must be a fresh directory")
    if type(no_potions) is not bool:
        raise ExactSuccessorSearchError("no_potions must be bool")
    output_dir.mkdir(parents=True)
    artifact_payload = artifact.read_bytes()
    if not artifact_payload or len(artifact_payload) > max_artifact_bytes:
        raise ExactSuccessorSearchError("chance root artifact violates its byte bound")
    root = _run_root_successor_corpus(
        oracle_binary=oracle_binary,
        artifact=artifact,
        expected_roots=expected_roots,
        root_slot=root_slot,
        candidate_count=candidate_count,
        corpus_path=output_dir / f"root-{root_slot:03}.json",
        solve_work_per_candidate=solve_work_per_candidate,
        candidate_jobs=candidate_jobs,
        max_artifact_bytes=max_artifact_bytes,
        no_potions=no_potions,
    )
    return {
        "schema": "sts-learning-equal-work-successor-search-v1",
        "engine": "rust_local_turn_graph_budget_or_exhaustion_v1",
        "artifact": str(artifact),
        "artifact_sha256": hashlib.sha256(artifact_payload).hexdigest(),
        "expected_root_count": expected_roots,
        "searched_root_slots": (root_slot,),
        "root_count": 1,
        "candidate_count": candidate_count,
        "solve_work_per_candidate": solve_work_per_candidate,
        "candidate_jobs": candidate_jobs,
        "legacy_v2_teacher_enabled": False,
        "no_potions": no_potions,
        "roots": (root,),
        "actions": _aggregate_actions((root,), candidate_count),
    }


def _run_root_successor_corpus(
    *,
    oracle_binary: Path,
    artifact: Path,
    expected_roots: int,
    root_slot: int,
    candidate_count: int,
    corpus_path: Path,
    solve_work_per_candidate: int,
    candidate_jobs: int,
    max_artifact_bytes: int,
    no_potions: bool,
) -> dict[str, object]:
    command = [
        str(oracle_binary),
        "--canonical-oracle",
        "build-action-successor-corpus",
        "--artifact",
        str(artifact),
        "--expected-roots",
        str(expected_roots),
        "--root-slot",
        str(root_slot),
        "--output",
        str(corpus_path),
        "--solve-work-per-candidate",
        str(solve_work_per_candidate),
        "--candidate-jobs",
        str(candidate_jobs),
        "--v2-teacher-wall-ms-per-candidate",
        "0",
        "--max-artifact-bytes",
        str(max_artifact_bytes),
    ]
    if no_potions:
        command.append("--no-potions")
    completed = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout).strip().splitlines()[-8:]
        raise ExactSuccessorSearchError(
            "Rust successor search failed: " + " | ".join(detail)
        )
    return _read_root_corpus(
        corpus_path,
        root_slot=root_slot,
        candidate_count=candidate_count,
        solve_work_per_candidate=solve_work_per_candidate,
    )


def select_complete_search_proposal(
    result: Mapping[str, object],
    *,
    baseline_ordinal: int,
) -> tuple[int | None, str]:
    """Choose only a resolved action strictly better than the frozen baseline."""

    actions = _mapping_sequence(result, "actions")
    if not 0 <= baseline_ordinal < len(actions):
        raise ExactSuccessorSearchError("baseline ordinal is outside the search surface")
    if any(int(action["budget_unknown_count"]) for action in actions):
        return None, "incomplete_equal_work_search"
    if not actions or max(int(action["exact_win_count"]) for action in actions) == 0:
        return None, "no_action_has_an_exact_win"
    selected = max(
        actions,
        key=lambda action: (
            int(action["exact_win_count"]),
            int(action["winning_final_hp_sum"]),
            -int(action["ordinal"]),
        ),
    )
    baseline = actions[baseline_ordinal]
    selected_quality = (
        int(selected["exact_win_count"]),
        int(selected["winning_final_hp_sum"]),
    )
    baseline_quality = (
        int(baseline["exact_win_count"]),
        int(baseline["winning_final_hp_sum"]),
    )
    if selected_quality <= baseline_quality:
        return None, "no_search_improvement_over_baseline"
    return int(selected["ordinal"]), "exact_win_count_then_winning_final_hp"


def paired_search_comparison(
    result: Mapping[str, object],
    *,
    candidate_ordinal: int,
    baseline_ordinal: int,
) -> dict[str, object]:
    """Compare two searched actions on the same ordered exact chance roots."""

    pairs = []
    for root in _mapping_sequence(result, "roots"):
        candidates = {
            int(candidate["ordinal"]): candidate
            for candidate in _mapping_sequence(root, "candidates")
        }
        candidate = candidates[candidate_ordinal]
        baseline = candidates[baseline_ordinal]
        if "budget_unknown" in {candidate["kind"], baseline["kind"]}:
            pairs.append({"root_slot": root["root_slot"], "status": "unknown"})
            continue
        candidate_won = candidate["kind"] == "exact_win"
        baseline_won = baseline["kind"] == "exact_win"
        both_win = candidate_won and baseline_won
        pairs.append(
            {
                "root_slot": root["root_slot"],
                "status": "paired",
                "candidate_won": candidate_won,
                "baseline_won": baseline_won,
                "win_delta": int(candidate_won) - int(baseline_won),
                "both_win_final_hp_delta": (
                    int(candidate["final_hp"]) - int(baseline["final_hp"])
                    if both_win
                    else None
                ),
            }
        )
    completed = [pair for pair in pairs if pair["status"] == "paired"]
    hp_deltas = [
        float(pair["both_win_final_hp_delta"])
        for pair in completed
        if pair["both_win_final_hp_delta"] is not None
    ]
    win_deltas = [float(pair["win_delta"]) for pair in completed]
    return {
        "candidate_ordinal": candidate_ordinal,
        "baseline_ordinal": baseline_ordinal,
        "paired_count": len(completed),
        "unknown_count": len(pairs) - len(completed),
        "mean_win_delta": statistics.fmean(win_deltas) if win_deltas else None,
        "mean_both_win_final_hp_delta": (
            statistics.fmean(hp_deltas) if hp_deltas else None
        ),
        "pairs": tuple(pairs),
    }


def _read_root_corpus(
    path: Path,
    *,
    root_slot: int,
    candidate_count: int,
    solve_work_per_candidate: int,
) -> dict[str, object]:
    try:
        corpus = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ExactSuccessorSearchError("cannot read Rust successor corpus") from error
    if (
        corpus.get("schema_name") != "ActionSuccessorReanalysisCorpusV2"
        or corpus.get("schema_version") != 2
        or corpus.get("source_mode") != "opaque_combat_root_artifact"
    ):
        raise ExactSuccessorSearchError("Rust successor corpus has the wrong identity")
    learning_surface = corpus.get("learning_surface")
    if not isinstance(learning_surface, Mapping) or not learning_surface.get("complete"):
        raise ExactSuccessorSearchError("Rust successor corpus does not cover the model surface")
    raw_candidates = corpus.get("candidates")
    if not isinstance(raw_candidates, list) or len(raw_candidates) != candidate_count:
        raise ExactSuccessorSearchError("Rust successor candidate count changed")
    candidates: dict[int, dict[str, object]] = {}
    for raw in raw_candidates:
        if not isinstance(raw, Mapping):
            raise ExactSuccessorSearchError("Rust successor candidate is malformed")
        ordinal = raw.get("learning_candidate_ordinal")
        evidence = raw.get("evidence")
        if not isinstance(ordinal, int) or not isinstance(evidence, Mapping):
            raise ExactSuccessorSearchError("Rust successor candidate is not model-aligned")
        if ordinal in candidates:
            raise ExactSuccessorSearchError("Rust successor candidate ordinal repeats")
        kind = evidence.get("kind")
        if kind not in {
            "exact_win",
            "exact_refutation",
            "exact_terminal_non_win",
            "budget_unknown",
        }:
            raise ExactSuccessorSearchError("Rust successor evidence kind is unsupported")
        search_cost = evidence.get("search_cost")
        generation_work = (
            None
            if search_cost is None
            else int(_mapping(search_cost, "search_cost")["generation_work"])
        )
        if generation_work is not None and generation_work > solve_work_per_candidate:
            raise ExactSuccessorSearchError("Rust successor exceeded its declared work")
        candidates[ordinal] = {
            "ordinal": ordinal,
            "kind": kind,
            "final_hp": int(evidence["final_hp"]) if kind == "exact_win" else None,
            "generation_work": generation_work,
            "source": evidence.get("source"),
        }
    if set(candidates) != set(range(candidate_count)):
        raise ExactSuccessorSearchError("Rust successor ordinals do not cover the model surface")
    return {
        "root_slot": root_slot,
        "root_exact_state_hash": corpus.get("root_exact_state_hash"),
        "corpus": str(path),
        "candidates": tuple(candidates[index] for index in range(candidate_count)),
    }


def _aggregate_actions(
    roots: Sequence[Mapping[str, object]],
    candidate_count: int,
) -> tuple[dict[str, object], ...]:
    actions = []
    for ordinal in range(candidate_count):
        evidence = [_mapping_sequence(root, "candidates")[ordinal] for root in roots]
        kinds = Counter(str(row["kind"]) for row in evidence)
        final_hp = [int(row["final_hp"]) for row in evidence if row["kind"] == "exact_win"]
        actions.append(
            {
                "ordinal": ordinal,
                "exact_win_count": kinds["exact_win"],
                "exact_refutation_count": kinds["exact_refutation"],
                "exact_terminal_non_win_count": kinds["exact_terminal_non_win"],
                "budget_unknown_count": kinds["budget_unknown"],
                "winning_final_hp_sum": sum(final_hp),
                "mean_winning_final_hp": (
                    statistics.fmean(final_hp) if final_hp else None
                ),
                "min_winning_final_hp": min(final_hp, default=None),
                "max_winning_final_hp": max(final_hp, default=None),
            }
        )
    return tuple(actions)


def _mapping(source: object, name: str) -> Mapping[str, object]:
    if not isinstance(source, Mapping):
        raise ExactSuccessorSearchError(f"{name} must be a mapping")
    return source


def _mapping_sequence(
    source: Mapping[str, object],
    key: str,
) -> tuple[Mapping[str, object], ...]:
    value = source.get(key)
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise ExactSuccessorSearchError(f"{key} must be a sequence")
    return tuple(_mapping(item, key) for item in value)
