"""Expand verified natural-root search wins into multi-decision search corpora."""

from __future__ import annotations

import argparse
import hashlib
import json
import operator
import subprocess
import time
from collections.abc import Mapping, Sequence
from pathlib import Path

from .natural_combat_search_census import (
    recover_natural_combat_search_anchor,
    run_natural_combat_search_census,
)
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits


class CombatSearchTrajectoryCensusError(RuntimeError):
    """A natural-root witness or derived recovery corpus was inconsistent."""


def run_combat_search_trajectory_census(
    *,
    artifact: Path,
    expected_roots: int,
    search_manifest: Path,
    root_slots: Sequence[int],
    behavior: Path | None,
    candidate: Path | None,
    oracle_binary: Path,
    output_dir: Path,
    max_recovery_roots: int,
    solve_work_per_candidate: int,
    candidate_jobs: int,
    policy_seed: int,
    max_artifact_bytes: int,
) -> dict[str, object]:
    """Replay complete search wins and independently search their suffix roots."""

    expected_roots = _positive(expected_roots, "expected_roots")
    max_recovery_roots = _positive(max_recovery_roots, "max_recovery_roots")
    solve_work_per_candidate = _positive(
        solve_work_per_candidate,
        "solve_work_per_candidate",
    )
    candidate_jobs = _positive(candidate_jobs, "candidate_jobs")
    max_artifact_bytes = _positive(max_artifact_bytes, "max_artifact_bytes")
    slots = tuple(operator.index(slot) for slot in root_slots)
    if not slots or len(set(slots)) != len(slots):
        raise CombatSearchTrajectoryCensusError(
            "root_slots must be a non-empty unique sequence"
        )
    if any(not 0 <= slot < expected_roots for slot in slots):
        raise CombatSearchTrajectoryCensusError(
            "root_slots contain an index outside expected_roots"
        )

    artifact = Path(artifact).resolve()
    search_manifest = Path(search_manifest).resolve()
    if (behavior is None) == (candidate is None):
        raise CombatSearchTrajectoryCensusError(
            "exactly one published behavior or experimental candidate is required"
        )
    behavior = None if behavior is None else Path(behavior).resolve()
    candidate = None if candidate is None else Path(candidate).resolve()
    oracle_binary = Path(oracle_binary).resolve()
    output_dir = Path(output_dir).resolve()
    for path, name in (
        (artifact, "artifact"),
        (search_manifest, "search_manifest"),
        (oracle_binary, "oracle_binary"),
    ):
        if not path.is_file():
            raise CombatSearchTrajectoryCensusError(f"{name} must be a file")
    for path, name in ((behavior, "behavior"), (candidate, "candidate")):
        if path is not None and not path.is_dir():
            raise CombatSearchTrajectoryCensusError(f"{name} must be a directory")
    if output_dir.exists() or not output_dir.parent.is_dir():
        raise CombatSearchTrajectoryCensusError(
            "output_dir must be fresh below an existing directory"
        )

    artifact_payload = artifact.read_bytes()
    if not artifact_payload or len(artifact_payload) > max_artifact_bytes:
        raise CombatSearchTrajectoryCensusError(
            "source artifact violates its byte bound"
        )
    source_digest = hashlib.sha256(artifact_payload).hexdigest()
    manifest = _read_json_object(search_manifest, "search manifest")
    if (
        manifest.get("schema") != "sts-learning-natural-combat-search-census-v1"
        or manifest.get("teacher_valid") is not False
    ):
        raise CombatSearchTrajectoryCensusError(
            "unsupported natural-root search manifest"
        )
    source = _mapping(manifest.get("source"), "search source")
    if (
        source.get("artifact_sha256") != source_digest
        or operator.index(source.get("root_count")) != expected_roots
    ):
        raise CombatSearchTrajectoryCensusError(
            "search manifest does not match the source root artifact"
        )
    search_config = _mapping(manifest.get("search"), "search config")
    if search_config.get("potion_lane") != "never":
        raise CombatSearchTrajectoryCensusError(
            "trajectory census requires no-potion source search"
        )
    roots = _mapping_sequence(manifest, "roots")
    if len(roots) != expected_roots:
        raise CombatSearchTrajectoryCensusError(
            "search manifest does not cover every source root"
        )
    anchor = recover_natural_combat_search_anchor(
        behavior=behavior,
        candidate=candidate,
        bridge=CombatSessionBridge.installed(),
        limits=CombatWinSessionLimits(max_artifact_bytes=max_artifact_bytes),
        policy_seed=policy_seed,
    )
    if manifest.get("behavior_manifest_id") != anchor.manifest_id.digest.hex():
        raise CombatSearchTrajectoryCensusError(
            "source search manifest and suffix-search anchor disagree"
        )

    output_dir.mkdir()
    started = time.perf_counter()
    trajectories: list[dict[str, object]] = []
    for trajectory_index, root_slot in enumerate(slots):
        root = roots[root_slot]
        if operator.index(root.get("root_slot")) != root_slot:
            raise CombatSearchTrajectoryCensusError(
                "search roots are not in exact artifact order"
            )
        proposal_raw = root.get("proposal_ordinal")
        if proposal_raw is None:
            raise CombatSearchTrajectoryCensusError(
                f"source root {root_slot} has no strict search proposal"
            )
        proposal_ordinal = operator.index(proposal_raw)
        root_search = _mapping(root.get("search"), "root search")
        search_roots = _mapping_sequence(root_search, "roots")
        if len(search_roots) != 1:
            raise CombatSearchTrajectoryCensusError(
                "source root search must contain exactly one root"
            )
        corpus_path = Path(str(search_roots[0].get("corpus"))).resolve()
        corpus = _read_json_object(corpus_path, "successor corpus")
        if (
            corpus.get("schema_name") != "ActionSuccessorReanalysisCorpusV2"
            or corpus.get("schema_version") != 2
            or _mapping(corpus.get("config"), "corpus config").get("no_potions")
            is not True
        ):
            raise CombatSearchTrajectoryCensusError(
                "source successor corpus has the wrong contract"
            )
        candidates = {
            operator.index(candidate.get("learning_candidate_ordinal")): candidate
            for candidate in _mapping_sequence(corpus, "candidates")
        }
        proposal_candidate = candidates.get(proposal_ordinal)
        if proposal_candidate is None:
            raise CombatSearchTrajectoryCensusError(
                "proposal ordinal is absent from its successor corpus"
            )
        evidence = _mapping(
            proposal_candidate.get("evidence"),
            "proposal evidence",
        )
        if evidence.get("kind") != "exact_win":
            raise CombatSearchTrajectoryCensusError(
                "strict proposal lacks a complete exact-win witness"
            )

        trajectory_dir = output_dir / f"trajectory-{trajectory_index:03}"
        trajectory_dir.mkdir()
        recovery_path = trajectory_dir / "recovery-roots.bin"
        recovery_receipt = _run_oracle_json(
            oracle_binary,
            (
                "learning-root",
                "recover-search",
                "--artifact",
                str(artifact),
                "--expected-roots",
                str(expected_roots),
                "--root-slot",
                str(root_slot),
                "--corpus",
                str(corpus_path),
                "--candidate-ordinal",
                str(proposal_ordinal),
                "--output",
                str(recovery_path),
                "--max-roots",
                str(max_recovery_roots),
                "--max-bytes",
                str(max_artifact_bytes),
            ),
        )
        recovered_roots = recovery_receipt.get("roots")
        if not isinstance(recovered_roots, list) or not recovered_roots:
            raise CombatSearchTrajectoryCensusError(
                "recovery command exported no suffix roots"
            )
        recovered_root_count = len(recovered_roots)
        suffix_search_dir = trajectory_dir / "search"
        suffix_search = run_natural_combat_search_census(
            artifact=recovery_path,
            expected_roots=recovered_root_count,
            behavior=behavior,
            candidate=candidate,
            oracle_binary=oracle_binary,
            output_dir=suffix_search_dir,
            policy_seed=policy_seed,
            solve_work_per_candidate=solve_work_per_candidate,
            candidate_jobs=candidate_jobs,
            max_artifact_bytes=max_artifact_bytes,
        )
        trajectories.append(
            {
                "source_root_slot": root_slot,
                "source_root": root.get("root"),
                "source_proposal_ordinal": proposal_ordinal,
                "source_win_final_hp": evidence.get("final_hp"),
                "source_action_count": operator.index(
                    recovery_receipt.get("supplied_action_count")
                ),
                "source_corpus": str(corpus_path),
                "source_identity": recovery_receipt.get("source_identity"),
                "recovery_artifact": str(recovery_path),
                "recovery_root_count": recovered_root_count,
                "recovery_roots": recovered_roots,
                "search_manifest": str(suffix_search_dir / "manifest.json"),
                "proposal_count": suffix_search["proposal_count"],
                "no_proposal_count": suffix_search["no_proposal_count"],
            }
        )

    result = {
        "schema": "sts-learning-combat-search-trajectory-census-v2",
        "teacher_valid": False,
        "claim": "verified_artifact_native_winning_trajectory_suffix_search_census_only",
        "source": {
            "artifact": str(artifact),
            "artifact_sha256": source_digest,
            "root_count": expected_roots,
            "search_manifest": str(search_manifest),
            "selected_root_slots": slots,
            "anchor": anchor.receipt,
        },
        "config": {
            "max_recovery_roots": max_recovery_roots,
            "solve_work_per_candidate": solve_work_per_candidate,
            "candidate_jobs": candidate_jobs,
            "policy_seed": policy_seed,
            "potion_lane": "never",
            "recovery_protocol": "opaque_artifact_exact_win_corpus_v1",
        },
        "trajectory_count": len(trajectories),
        "recovery_root_count": sum(
            operator.index(item["recovery_root_count"]) for item in trajectories
        ),
        "proposal_count": sum(
            operator.index(item["proposal_count"]) for item in trajectories
        ),
        "trajectories": trajectories,
        "elapsed_seconds": time.perf_counter() - started,
    }
    manifest_path = output_dir / "manifest.json"
    _write_json(manifest_path, result)
    print(
        json.dumps(
            {
                "schema": result["schema"],
                "teacher_valid": False,
                "trajectory_count": result["trajectory_count"],
                "recovery_root_count": result["recovery_root_count"],
                "proposal_count": result["proposal_count"],
                "artifact": str(manifest_path),
            },
            separators=(",", ":"),
            sort_keys=True,
        ),
        flush=True,
    )
    return result


def _run_oracle_json(binary: Path, arguments: Sequence[str]) -> Mapping[str, object]:
    completed = subprocess.run(
        [str(binary), "--canonical-oracle", *arguments],
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout).strip().splitlines()[-8:]
        raise CombatSearchTrajectoryCensusError(
            "oracle command failed: " + " | ".join(detail)
        )
    lines = [line for line in completed.stdout.splitlines() if line.strip()]
    if not lines:
        raise CombatSearchTrajectoryCensusError("oracle command returned no receipt")
    try:
        receipt = json.loads(lines[-1])
    except json.JSONDecodeError as error:
        raise CombatSearchTrajectoryCensusError(
            "oracle command returned a malformed receipt"
        ) from error
    if not isinstance(receipt, Mapping):
        raise CombatSearchTrajectoryCensusError(
            "oracle command receipt must be an object"
        )
    return receipt


def _read_json_object(path: Path, name: str) -> Mapping[str, object]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CombatSearchTrajectoryCensusError(f"cannot read {name}") from error
    if not isinstance(payload, Mapping):
        raise CombatSearchTrajectoryCensusError(f"{name} must be an object")
    return payload


def _write_json(path: Path, payload: object) -> None:
    with path.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(payload, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")


def _mapping(value: object, name: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise CombatSearchTrajectoryCensusError(f"{name} must be an object")
    return value


def _mapping_sequence(
    source: Mapping[str, object],
    key: str,
) -> tuple[Mapping[str, object], ...]:
    value = source.get(key)
    if not isinstance(value, list) or not all(isinstance(item, Mapping) for item in value):
        raise CombatSearchTrajectoryCensusError(f"{key} must be an object array")
    return tuple(value)


def _positive(value: object, name: str) -> int:
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatSearchTrajectoryCensusError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise CombatSearchTrajectoryCensusError(f"{name} must be positive")
    return normalized


def _parse_args(argv: Sequence[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--root-count", type=int, required=True)
    parser.add_argument("--search-manifest", type=Path, required=True)
    parser.add_argument("--root-slot", type=int, action="append", required=True)
    anchor = parser.add_mutually_exclusive_group(required=True)
    anchor.add_argument("--behavior", type=Path)
    anchor.add_argument("--candidate", type=Path)
    parser.add_argument("--oracle-binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--max-recovery-roots", type=int, default=8)
    parser.add_argument("--solve-work-per-candidate", type=int, default=5_000)
    parser.add_argument("--candidate-jobs", type=int, default=4)
    parser.add_argument("--policy-seed", type=int, default=2026081201)
    parser.add_argument("--max-artifact-bytes", type=int, default=16 * 1024 * 1024)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = _parse_args(argv)
    run_combat_search_trajectory_census(
        artifact=args.artifact,
        expected_roots=args.root_count,
        search_manifest=args.search_manifest,
        root_slots=args.root_slot,
        behavior=args.behavior,
        candidate=args.candidate,
        oracle_binary=args.oracle_binary,
        output_dir=args.output_dir,
        max_recovery_roots=args.max_recovery_roots,
        solve_work_per_candidate=args.solve_work_per_candidate,
        candidate_jobs=args.candidate_jobs,
        policy_seed=args.policy_seed,
        max_artifact_bytes=args.max_artifact_bytes,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
