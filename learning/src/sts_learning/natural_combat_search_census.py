"""Equal-work search-improvement census over untouched natural combat roots."""

from __future__ import annotations

import argparse
import hashlib
import json
import operator
import time
from collections import Counter
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

from .combat_search_distillation_candidate import (
    recover_combat_search_distillation_candidate,
)
from .combat_root_artifacts import load_combat_root_source, read_combat_root_artifact
from .exact_successor_search import (
    paired_search_comparison,
    run_equal_work_successor_search_root,
    select_complete_search_proposal,
)
from .published_combat_behavior import recover_published_combat_behavior
from .policy import BehaviorManifestId
from .torch_behavior import FrozenGreedyTorchPolicy
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits
from .torch_policy import GreedyTorchPolicy


class NaturalCombatSearchCensusError(RuntimeError):
    """A natural-root search surface or result was malformed."""


@dataclass(frozen=True)
class NaturalCombatSearchAnchor:
    """One exact frozen scorer identity used for residual search."""

    policy: FrozenGreedyTorchPolicy | GreedyTorchPolicy
    manifest_id: BehaviorManifestId
    receipt: Mapping[str, object]


def recover_natural_combat_search_anchor(
    *,
    behavior: Path | None,
    candidate: Path | None,
    bridge: CombatSessionBridge,
    limits: CombatWinSessionLimits,
    policy_seed: int,
) -> NaturalCombatSearchAnchor:
    """Recover exactly one published or explicitly unqualified greedy anchor."""

    if (behavior is None) == (candidate is None):
        raise NaturalCombatSearchCensusError(
            "exactly one published behavior or experimental candidate is required"
        )
    if behavior is not None:
        behavior = Path(behavior).resolve()
        published = recover_published_combat_behavior(
            behavior,
            bridge,
            limits,
            behavior_seeds=(_seed(policy_seed, "policy_seed"),),
        )
        policy = FrozenGreedyTorchPolicy.from_behavior(published.policies[0])
        return NaturalCombatSearchAnchor(
            policy=policy,
            manifest_id=published.manifest_id,
            receipt={
                "kind": "published_combat_behavior",
                "artifact": str(behavior),
                "manifest_id": published.manifest_id.digest.hex(),
            },
        )

    assert candidate is not None
    candidate = Path(candidate).resolve()
    restored = recover_combat_search_distillation_candidate(
        candidate,
        bridge,
        limits,
    )
    return NaturalCombatSearchAnchor(
        policy=GreedyTorchPolicy(restored.scorer, restored.manifest_id),
        manifest_id=restored.manifest_id,
        receipt={
            "kind": "experimental_search_distillation_candidate",
            "artifact": str(candidate),
            "candidate_id": restored.candidate_id,
            "manifest_id": restored.manifest_id.digest.hex(),
            "source_manifest_id": restored.source_manifest_id.digest.hex(),
            "loss": restored.loss,
        },
    )


def run_natural_combat_search_census(
    *,
    artifact: Path,
    expected_roots: int,
    behavior: Path | None,
    candidate: Path | None,
    oracle_binary: Path,
    output_dir: Path,
    policy_seed: int,
    solve_work_per_candidate: int,
    candidate_jobs: int,
    max_artifact_bytes: int,
) -> dict[str, object]:
    """Challenge the frozen policy once on every declared natural root."""

    expected_roots = _positive(expected_roots, "expected_roots")
    policy_seed = _seed(policy_seed, "policy_seed")
    solve_work_per_candidate = _positive(
        solve_work_per_candidate,
        "solve_work_per_candidate",
    )
    candidate_jobs = _positive(candidate_jobs, "candidate_jobs")
    max_artifact_bytes = _positive(max_artifact_bytes, "max_artifact_bytes")
    artifact = Path(artifact).resolve()
    behavior = None if behavior is None else Path(behavior).resolve()
    candidate = None if candidate is None else Path(candidate).resolve()
    oracle_binary = Path(oracle_binary).resolve()
    output_dir = Path(output_dir).resolve()
    if output_dir.exists():
        raise NaturalCombatSearchCensusError("output directory must be fresh")
    output_dir.mkdir(parents=True)

    limits = CombatWinSessionLimits(max_artifact_bytes=max_artifact_bytes)
    bridge = CombatSessionBridge.installed()
    payload = read_combat_root_artifact(artifact, max_bytes=max_artifact_bytes)
    source = load_combat_root_source(
        bridge,
        payload,
        expected_roots=expected_roots,
        max_bytes=max_artifact_bytes,
    )
    anchor = recover_natural_combat_search_anchor(
        behavior=behavior,
        candidate=candidate,
        bridge=bridge,
        limits=limits,
        policy_seed=policy_seed,
    )
    policy = anchor.policy

    started = time.perf_counter()
    roots: list[dict[str, object]] = []
    proposal_reasons: Counter[str] = Counter()
    for root_slot in range(expected_roots):
        group = source.combat_group(root_slot, 1, potion_slots=[])
        audit_raw = group.combat_decision_audit_json(0)
        if not isinstance(audit_raw, str):
            raise NaturalCombatSearchCensusError(
                f"natural root {root_slot} lacks a combat decision audit"
            )
        audit = json.loads(audit_raw)
        candidates = audit.get("candidates")
        if not isinstance(candidates, list) or not candidates:
            raise NaturalCombatSearchCensusError(
                f"natural root {root_slot} exposes no typed candidates"
            )
        choice = policy.choose(group.decision_batch(semantic=True))
        if len(choice.ordinals) != 1:
            raise NaturalCombatSearchCensusError(
                f"natural root {root_slot} policy decision is not singular"
            )
        baseline_ordinal = operator.index(choice.ordinals[0])
        if not 0 <= baseline_ordinal < len(candidates):
            raise NaturalCombatSearchCensusError(
                f"natural root {root_slot} baseline is outside its candidate surface"
            )

        search = run_equal_work_successor_search_root(
            oracle_binary=oracle_binary,
            artifact=artifact,
            expected_roots=expected_roots,
            root_slot=root_slot,
            candidate_count=len(candidates),
            output_dir=output_dir / f"root-{root_slot:03}",
            solve_work_per_candidate=solve_work_per_candidate,
            candidate_jobs=candidate_jobs,
            max_artifact_bytes=max_artifact_bytes,
            no_potions=True,
        )
        proposal_ordinal, proposal_reason = select_complete_search_proposal(
            search,
            baseline_ordinal=baseline_ordinal,
        )
        proposal_reasons[proposal_reason] += 1
        roots.append(
            {
                "root_slot": root_slot,
                "root": _root_audit(source, root_slot),
                "candidate_count": len(candidates),
                "candidates": candidates,
                "baseline_ordinal": baseline_ordinal,
                "baseline_candidate": candidates[baseline_ordinal],
                "proposal_ordinal": proposal_ordinal,
                "proposal_candidate": (
                    None if proposal_ordinal is None else candidates[proposal_ordinal]
                ),
                "proposal_reason": proposal_reason,
                "search": search,
                "paired": (
                    None
                    if proposal_ordinal is None
                    else paired_search_comparison(
                        search,
                        candidate_ordinal=proposal_ordinal,
                        baseline_ordinal=baseline_ordinal,
                    )
                ),
            }
        )

    manifest = {
        "schema": "sts-learning-natural-combat-search-census-v1",
        "teacher_valid": False,
        "claim": "realized_natural_root_search_improvement_census_only",
        "qualification_limit": (
            "search observes each realized private future; policy value must be proven "
            "by a later held-out natural-root update experiment"
        ),
        "source": {
            "artifact": str(artifact),
            "artifact_sha256": hashlib.sha256(payload).hexdigest(),
            "root_count": expected_roots,
            "selection_rule": "every_declared_root_once_in_artifact_order",
            "outcome_filter": "none",
            "encounter_filter": "none",
        },
        "behavior_manifest_id": anchor.manifest_id.digest.hex(),
        "anchor": anchor.receipt,
        "policy_rule": "frozen_greedy",
        "policy_seed": policy_seed,
        "search": {
            "engine": "rust_local_turn_graph_budget_or_exhaustion_v1",
            "solve_work_per_candidate": solve_work_per_candidate,
            "candidate_jobs": candidate_jobs,
            "legacy_v2_teacher_enabled": False,
            "potion_lane": "never",
        },
        "proposal_rule": (
            "strict_exact_win_count_then_winning_final_hp_over_frozen_anchor"
        ),
        "proposal_count": sum(
            root["proposal_ordinal"] is not None for root in roots
        ),
        "no_proposal_count": sum(
            root["proposal_ordinal"] is None for root in roots
        ),
        "proposal_reason_counts": dict(sorted(proposal_reasons.items())),
        "elapsed_seconds": time.perf_counter() - started,
        "roots": roots,
    }
    manifest_path = output_dir / "manifest.json"
    with manifest_path.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(manifest, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    receipt = {
        key: manifest[key]
        for key in (
            "schema",
            "teacher_valid",
            "proposal_count",
            "no_proposal_count",
            "proposal_reason_counts",
            "elapsed_seconds",
        )
    }
    receipt["artifact"] = str(manifest_path)
    print(json.dumps(receipt, separators=(",", ":"), sort_keys=True), flush=True)
    return manifest


def _root_audit(source: object, root_slot: int) -> dict[str, object]:
    audit = source.combat_root_audit(root_slot)
    return {
        "seed": audit.seed,
        "act": audit.act,
        "floor": audit.floor,
        "ascension_level": audit.ascension_level,
        "hp": audit.hp,
        "max_hp": audit.max_hp,
        "encounter_id": audit.encounter_id,
        "monster_ids": audit.monster_ids,
        "master_deck_cards": audit.master_deck_cards,
        "relic_ids": audit.relic_ids,
        "potion_ids": audit.potion_ids,
    }


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise NaturalCombatSearchCensusError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise NaturalCombatSearchCensusError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise NaturalCombatSearchCensusError(f"{name} must be positive")
    return normalized


def _seed(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise NaturalCombatSearchCensusError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise NaturalCombatSearchCensusError(f"{name} must be an integer") from error
    if not 0 <= normalized < 1 << 64:
        raise NaturalCombatSearchCensusError(f"{name} must be in 0..2^64-1")
    return normalized


def _parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run equal-work search once on every natural combat root"
    )
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--root-count", type=int, required=True)
    anchor = parser.add_mutually_exclusive_group(required=True)
    anchor.add_argument("--behavior", type=Path)
    anchor.add_argument("--candidate", type=Path)
    parser.add_argument("--oracle-binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--policy-seed", type=int, default=2026081201)
    parser.add_argument("--solve-work-per-candidate", type=int, default=5_000)
    parser.add_argument("--candidate-jobs", type=int, default=4)
    parser.add_argument("--max-artifact-bytes", type=int, default=16 * 1024 * 1024)
    args = parser.parse_args(argv)
    for name in ("artifact", "oracle_binary"):
        path = getattr(args, name).resolve()
        if not path.is_file():
            parser.error(f"--{name.replace('_', '-')} must be an existing file")
        setattr(args, name, path)
    for name in ("behavior", "candidate"):
        path = getattr(args, name)
        if path is None:
            continue
        path = path.resolve()
        if not path.is_dir():
            parser.error(f"--{name} must be an existing directory")
        setattr(args, name, path)
    args.output_dir = args.output_dir.resolve()
    if args.output_dir.exists():
        parser.error("--output-dir must be fresh")
    if not args.output_dir.parent.is_dir():
        parser.error("--output-dir parent must exist")
    return args


def main(argv: Sequence[str] | None = None) -> int:
    args = _parse_args(argv)
    run_natural_combat_search_census(
        artifact=args.artifact,
        expected_roots=args.root_count,
        behavior=args.behavior,
        candidate=args.candidate,
        oracle_binary=args.oracle_binary,
        output_dir=args.output_dir,
        policy_seed=args.policy_seed,
        solve_work_per_candidate=args.solve_work_per_candidate,
        candidate_jobs=args.candidate_jobs,
        max_artifact_bytes=args.max_artifact_bytes,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
