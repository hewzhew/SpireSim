"""Capture the first meaningful greedy-policy divergence on exact combat roots."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import operator
from collections import Counter
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

import torch

from .combat_potion_lane import CombatPotionLane
from .combat_decision_audit import (
    CombatDecisionAuditError,
    read_combat_decision_audit,
)
from .combat_root_artifacts import (
    load_combat_root_source,
    normalize_combat_root_artifact,
    read_combat_root_artifact,
)
from .combat_root_audit import read_combat_root_audits
from .combat_search_distillation_candidate import (
    recover_combat_search_distillation_candidate,
)
from .fixed_combat_policy_audit import (
    FixedCombatPolicyAuditError,
    FixedPolicyIdentity,
    compare_fixed_combat_decision,
)
from .policy import BehaviorManifestId
from .published_combat_behavior import recover_compatible_combat_scorer
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits
from .torch_policy import RaggedCandidateLogits, RaggedCandidateScorer


COLLECTION_SCHEMA = "sts-learning-combat-policy-divergence-collection-v1"


class CombatPolicyDivergenceCollectionError(RuntimeError):
    """The requested exact-root divergence collection is invalid or incomplete."""


@dataclass(frozen=True)
class CombatPolicyDivergenceInput:
    artifact: Path
    root_count: int

    def __post_init__(self) -> None:
        artifact = Path(self.artifact).resolve()
        if not artifact.is_file():
            raise CombatPolicyDivergenceCollectionError(
                "divergence input artifact is not a file"
            )
        object.__setattr__(self, "artifact", artifact)
        object.__setattr__(self, "root_count", _positive(self.root_count, "root_count"))


@dataclass(frozen=True)
class CombatPolicyDivergenceCollectionConfig:
    inputs: tuple[CombatPolicyDivergenceInput, ...]
    baseline_behavior: Path
    candidate: Path
    output: Path
    max_decisions_per_root: int = 4_096
    max_captures: int = 64
    max_artifact_bytes: int = 16 * 1024 * 1024

    def __post_init__(self) -> None:
        inputs = tuple(self.inputs)
        if not inputs or not all(
            isinstance(item, CombatPolicyDivergenceInput) for item in inputs
        ):
            raise CombatPolicyDivergenceCollectionError(
                "divergence collection requires typed input artifacts"
            )
        artifacts = tuple(item.artifact for item in inputs)
        if len(set(artifacts)) != len(artifacts):
            raise CombatPolicyDivergenceCollectionError(
                "divergence collection repeats an input artifact"
            )
        baseline = Path(self.baseline_behavior).resolve()
        candidate = Path(self.candidate).resolve()
        output = Path(self.output).resolve()
        if not baseline.is_dir() or not candidate.is_dir():
            raise CombatPolicyDivergenceCollectionError(
                "divergence policies must be existing directories"
            )
        if baseline == candidate:
            raise CombatPolicyDivergenceCollectionError(
                "divergence collection requires distinct policy directories"
            )
        if output.exists() or not output.parent.is_dir():
            raise CombatPolicyDivergenceCollectionError(
                "divergence output must be fresh below an existing directory"
            )
        if any(output == root or root in output.parents for root in (baseline, candidate)):
            raise CombatPolicyDivergenceCollectionError(
                "divergence output must stay outside policy directories"
            )
        object.__setattr__(self, "inputs", inputs)
        object.__setattr__(self, "baseline_behavior", baseline)
        object.__setattr__(self, "candidate", candidate)
        object.__setattr__(self, "output", output)
        object.__setattr__(
            self,
            "max_decisions_per_root",
            _positive(self.max_decisions_per_root, "max_decisions_per_root"),
        )
        object.__setattr__(self, "max_captures", _positive(self.max_captures, "max_captures"))
        object.__setattr__(
            self,
            "max_artifact_bytes",
            _positive(self.max_artifact_bytes, "max_artifact_bytes"),
        )


class _FrozenGreedyScoringPolicy:
    """Expose a frozen scorer through the fixed-audit scoring protocol."""

    def __init__(
        self,
        scorer: RaggedCandidateScorer,
        behavior_manifest_id: BehaviorManifestId,
    ) -> None:
        if not isinstance(scorer, RaggedCandidateScorer):
            raise CombatPolicyDivergenceCollectionError(
                "divergence scoring requires the maintained scorer"
            )
        if not isinstance(behavior_manifest_id, BehaviorManifestId):
            raise CombatPolicyDivergenceCollectionError(
                "divergence scoring requires a typed manifest identity"
            )
        self.scorer = scorer
        self.behavior_manifest_id = behavior_manifest_id

    def score(self, batch: Mapping[str, object]) -> RaggedCandidateLogits:
        with torch.inference_mode():
            return self.scorer(batch)


def run_combat_policy_divergence_collection(
    config: CombatPolicyDivergenceCollectionConfig,
    *,
    bridge: CombatSessionBridge | None = None,
    print_completion: bool = True,
) -> dict[str, object]:
    """Follow shared greedy actions and capture each first relevant divergence."""

    if not isinstance(config, CombatPolicyDivergenceCollectionConfig):
        raise CombatPolicyDivergenceCollectionError(
            "divergence collection config must be typed"
        )
    active_bridge = bridge if bridge is not None else CombatSessionBridge.installed()
    if not isinstance(active_bridge, CombatSessionBridge):
        raise CombatPolicyDivergenceCollectionError(
            "divergence collection bridge must be typed"
        )
    limits = CombatWinSessionLimits(max_artifact_bytes=config.max_artifact_bytes)
    baseline = recover_compatible_combat_scorer(
        config.baseline_behavior,
        active_bridge,
        limits,
    )
    candidate = recover_combat_search_distillation_candidate(
        config.candidate,
        active_bridge,
        limits,
    )
    if candidate.source_manifest_id != baseline.source_manifest_id:
        raise CombatPolicyDivergenceCollectionError(
            "candidate and baseline do not share the same source manifest"
        )
    baseline_identity = FixedPolicyIdentity(
        manifest_id=baseline.source_manifest_id,
        checkpoint_id=baseline.checkpoint_id,
        training_step=baseline.training_step,
        temperature=1.0,
    )
    candidate_identity = FixedPolicyIdentity(
        manifest_id=candidate.manifest_id,
        checkpoint_id=candidate.checkpoint_id,
        training_step=candidate.epochs,
        temperature=1.0,
    )
    baseline_policy = _FrozenGreedyScoringPolicy(
        baseline.scorer,
        baseline.source_manifest_id,
    )
    candidate_policy = _FrozenGreedyScoringPolicy(
        candidate.scorer,
        candidate.manifest_id,
    )

    rows: list[dict[str, object]] = []
    captured_payloads: list[bytes] = []
    merger: Callable[..., object] | None = None
    for input_index, source_input in enumerate(config.inputs):
        artifact = read_combat_root_artifact(
            source_input.artifact,
            max_bytes=config.max_artifact_bytes,
        )
        artifact_sha256 = hashlib.sha256(artifact).hexdigest()
        source = load_combat_root_source(
            active_bridge,
            artifact,
            expected_roots=source_input.root_count,
            max_bytes=config.max_artifact_bytes,
        )
        if merger is None:
            merger = getattr(source, "merge_combat_root_artifact_bytes", None)
            if not callable(merger):
                raise CombatPolicyDivergenceCollectionError(
                    "installed bridge does not expose opaque root merging"
                )
        audits = read_combat_root_audits(
            source,
            tuple(range(source_input.root_count)),
        )
        for root_slot, root_audit in enumerate(audits):
            if len(captured_payloads) >= config.max_captures:
                rows.append(
                    {
                        "input_index": input_index,
                        "root_slot": root_slot,
                        "status": "capture_limit_not_inspected",
                        "artifact": str(source_input.artifact),
                        "artifact_sha256": artifact_sha256,
                        "root_audit": root_audit.as_mapping(),
                    }
                )
                continue
            try:
                group = source.combat_group(root_slot, 1, potion_slots=[])
            except Exception as error:
                raise CombatPolicyDivergenceCollectionError(
                    f"divergence combat group construction failed: {error}"
                ) from error
            row, captured = _inspect_first_policy_divergence(
                group,
                baseline_policy,
                candidate_policy,
                baseline_identity=baseline_identity,
                candidate_identity=candidate_identity,
                artifact_sha256=artifact_sha256,
                expected_roots=source_input.root_count,
                root_slot=root_slot,
                root_audit=root_audit.as_mapping(),
                max_decisions=config.max_decisions_per_root,
                max_artifact_bytes=config.max_artifact_bytes,
            )
            row.update(
                {
                    "input_index": input_index,
                    "artifact": str(source_input.artifact),
                }
            )
            if captured is not None:
                row["captured_root_slot"] = len(captured_payloads)
                captured_payloads.append(captured)
            rows.append(row)

    combined: bytes | None = None
    combined_sha256: str | None = None
    if captured_payloads:
        assert callable(merger)
        try:
            merged = merger(
                captured_payloads,
                max_bytes=config.max_artifact_bytes,
            )
        except Exception as error:
            raise CombatPolicyDivergenceCollectionError(
                f"captured divergence root merge failed: {error}"
            ) from error
        combined = normalize_combat_root_artifact(
            merged,
            max_bytes=config.max_artifact_bytes,
        )
        combined_sha256 = hashlib.sha256(combined).hexdigest()

    status_counts = Counter(str(row["status"]) for row in rows)
    category_counts = Counter(
        str(row["divergence_category"])
        for row in rows
        if row.get("divergence_category") is not None
    )
    result: dict[str, object] = {
        "schema": COLLECTION_SCHEMA,
        "claim": "first_shared-greedy-policy-divergence-capture_only",
        "teacher_valid": False,
        "production_eligible": False,
        "potion_lane": CombatPotionLane.NEVER.value,
        "baseline_behavior": str(config.baseline_behavior),
        "baseline_manifest_id": baseline.source_manifest_id.digest.hex(),
        "candidate": str(config.candidate),
        "candidate_id": candidate.candidate_id,
        "candidate_manifest_id": candidate.manifest_id.digest.hex(),
        "inputs": tuple(
            {"artifact": str(item.artifact), "root_count": item.root_count}
            for item in config.inputs
        ),
        "input_root_count": sum(item.root_count for item in config.inputs),
        "captured_root_count": len(captured_payloads),
        "captured_artifact": (
            "divergence-roots.bin" if combined is not None else None
        ),
        "captured_artifact_sha256": combined_sha256,
        "max_decisions_per_root": config.max_decisions_per_root,
        "max_captures": config.max_captures,
        "status_counts": dict(sorted(status_counts.items())),
        "divergence_category_counts": dict(sorted(category_counts.items())),
        "rows": tuple(rows),
    }
    result["collection_id"] = _content_sha256(result)

    config.output.mkdir()
    if combined is not None:
        with (config.output / "divergence-roots.bin").open("xb") as destination:
            destination.write(combined)
    manifest_path = config.output / "manifest.json"
    with manifest_path.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(result, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    if print_completion:
        print(
            json.dumps(
                {
                    "schema": COLLECTION_SCHEMA,
                    "manifest": str(manifest_path),
                    "input_root_count": result["input_root_count"],
                    "captured_root_count": result["captured_root_count"],
                    "status_counts": result["status_counts"],
                    "divergence_category_counts": result[
                        "divergence_category_counts"
                    ],
                },
                separators=(",", ":"),
                sort_keys=True,
            ),
            flush=True,
        )
    return result


def _inspect_first_policy_divergence(
    group: object,
    baseline_policy: object,
    candidate_policy: object,
    *,
    baseline_identity: FixedPolicyIdentity,
    candidate_identity: FixedPolicyIdentity,
    artifact_sha256: str,
    expected_roots: int,
    root_slot: int,
    root_audit: Mapping[str, object],
    max_decisions: int,
    max_artifact_bytes: int,
) -> tuple[dict[str, object], bytes | None]:
    """Inspect one root without allowing the policies to enter different states."""

    source_root_id = _nonempty_text(getattr(group, "root_id", None), "root_id")
    source_exact_hash = _sha256(
        getattr(group, "exact_combat_state_hash", None),
        "exact_combat_state_hash",
    )
    shared_trace: list[dict[str, object]] = []
    enclosing_root: object | None = None
    for decision_index in range(_positive(max_decisions, "max_decisions")):
        try:
            decision = read_combat_decision_audit(group, 0)
        except CombatDecisionAuditError as error:
            raise CombatPolicyDivergenceCollectionError(str(error)) from error
        if decision is None:
            raise CombatPolicyDivergenceCollectionError(
                "combat group lost its auditable decision before termination"
            )
        if decision.phase == "combat_root":
            enclosing_root = _capture_exact_boundary(
                group,
                source_root_id=source_root_id,
                source_exact_hash=source_exact_hash,
            )
        elif enclosing_root is None:
            raise CombatPolicyDivergenceCollectionError(
                "combat selection has no enclosing exact combat root"
            )
        assert enclosing_root is not None
        try:
            comparison = compare_fixed_combat_decision(
                group,
                baseline_policy,
                candidate_policy,
                baseline_identity=baseline_identity,
                candidate_identity=candidate_identity,
                artifact_sha256=artifact_sha256,
                expected_roots=expected_roots,
                root_slot=root_slot,
                root_audit=root_audit,
                potion_lane=CombatPotionLane.NEVER,
                decision_root_id=_nonempty_text(
                    getattr(enclosing_root, "root_id", None),
                    "decision_root_id",
                ),
                decision_exact_combat_state_hash=_sha256(
                    getattr(enclosing_root, "exact_combat_state_hash", None),
                    "decision_exact_combat_state_hash",
                ),
                replay_prefix=shared_trace,
            )
        except FixedCombatPolicyAuditError as error:
            raise CombatPolicyDivergenceCollectionError(str(error)) from error
        baseline_ordinal = comparison.baseline.top_ordinal
        candidate_ordinal = comparison.candidate.top_ordinal
        if baseline_ordinal != candidate_ordinal:
            baseline_semantics = comparison.candidates[baseline_ordinal].semantics
            candidate_semantics = comparison.candidates[candidate_ordinal].semantics
            category = _relevant_divergence_category(
                baseline_semantics,
                candidate_semantics,
            )
            row: dict[str, object] = {
                "root_slot": root_slot,
                "artifact_sha256": artifact_sha256,
                "source_root_id": source_root_id,
                "source_exact_combat_state_hash": source_exact_hash,
                "shared_decision_count": len(shared_trace),
                "decision_index": decision_index,
                "decision_state_identity_kind": (
                    "exact_combat_root"
                    if decision.phase == "combat_root"
                    else "enclosing_exact_combat_root_plus_selection_prefix"
                ),
                "status": (
                    "captured_first_divergence"
                    if category is not None
                    else "rejected_first_divergence"
                ),
                "divergence_category": category,
                "audit": comparison.as_mapping(),
            }
            if category is None:
                return row, None
            if decision.phase != "combat_root":
                raise CombatPolicyDivergenceCollectionError(
                    "a relevant combat divergence escaped the exact-root phase"
                )
            payload_source = getattr(
                enclosing_root,
                "combat_root_artifact_bytes",
                None,
            )
            if not callable(payload_source):
                raise CombatPolicyDivergenceCollectionError(
                    "captured divergence root cannot export an opaque artifact"
                )
            try:
                payload = payload_source(max_bytes=max_artifact_bytes)
            except Exception as error:
                raise CombatPolicyDivergenceCollectionError(
                    f"captured divergence root export failed: {error}"
                ) from error
            normalized = normalize_combat_root_artifact(
                payload,
                max_bytes=max_artifact_bytes,
            )
            row["single_root_artifact_sha256"] = hashlib.sha256(normalized).hexdigest()
            return row, normalized

        chosen = comparison.candidates[baseline_ordinal]
        shared_trace.append(
            {
                "decision_index": decision_index,
                "decision_id": comparison.decision_id,
                "decision_root_id": comparison.decision_root_id,
                "decision_exact_combat_state_hash": (
                    comparison.decision_exact_combat_state_hash
                ),
                "phase": comparison.decision.phase,
                "selection_prefix": comparison.decision.selection_prefix,
                "selected_ordinal": baseline_ordinal,
                "selected_candidate_id": chosen.candidate_id,
                "selected_candidate": chosen.semantics,
                "baseline_top_two_logit_margin": (
                    comparison.baseline.top_two_logit_margin
                ),
                "candidate_top_two_logit_margin": (
                    comparison.candidate.top_two_logit_margin
                ),
            }
        )
        choose = getattr(group, "choose", None)
        if not callable(choose):
            raise CombatPolicyDivergenceCollectionError(
                "combat group does not expose choose()"
            )
        try:
            choose([baseline_ordinal])
        except Exception as error:
            raise CombatPolicyDivergenceCollectionError(
                f"shared policy action was rejected: {error}"
            ) from error
        if bool(getattr(group, "ready", False)):
            step = getattr(group, "step", None)
            if not callable(step):
                raise CombatPolicyDivergenceCollectionError(
                    "ready combat group does not expose step()"
                )
            try:
                step()
            except Exception as error:
                raise CombatPolicyDivergenceCollectionError(
                    f"shared policy action failed: {error}"
                ) from error
            if _nonnegative(getattr(group, "terminal_count", None), "terminal_count"):
                return (
                    {
                        "root_slot": root_slot,
                        "artifact_sha256": artifact_sha256,
                        "source_root_id": source_root_id,
                        "source_exact_combat_state_hash": source_exact_hash,
                        "shared_decision_count": len(shared_trace),
                        "status": "shared_terminal_without_divergence",
                        "divergence_category": None,
                        "shared_trace": tuple(shared_trace),
                    },
                    None,
                )
    return (
        {
            "root_slot": root_slot,
            "artifact_sha256": artifact_sha256,
            "source_root_id": source_root_id,
            "source_exact_combat_state_hash": source_exact_hash,
            "shared_decision_count": len(shared_trace),
            "status": "decision_limit_without_divergence",
            "divergence_category": None,
            "shared_trace": tuple(shared_trace),
        },
        None,
    )


def _capture_exact_boundary(
    group: object,
    *,
    source_root_id: str,
    source_exact_hash: str,
) -> object:
    capture = getattr(group, "capture_recovery_root", None)
    if not callable(capture):
        raise CombatPolicyDivergenceCollectionError(
            "combat group cannot capture its current exact boundary"
        )
    try:
        recovery = capture(0)
    except Exception as error:
        raise CombatPolicyDivergenceCollectionError(
            f"combat exact-boundary capture failed: {error}"
        ) from error
    lineage = (
        _nonempty_text(getattr(recovery, "source_root_id", None), "source_root_id"),
        _sha256(
            getattr(recovery, "source_exact_combat_state_hash", None),
            "source_exact_combat_state_hash",
        ),
    )
    if lineage != (source_root_id, source_exact_hash):
        raise CombatPolicyDivergenceCollectionError(
            "captured divergence boundary changed its exact source lineage"
        )
    return recovery


def _relevant_divergence_category(
    baseline: Mapping[str, object],
    candidate: Mapping[str, object],
) -> str | None:
    if _same_card_profile_different_target(baseline, candidate):
        return "same_card_profile_different_target"
    if _is_damaging_card_action(baseline) and _is_damaging_card_action(candidate):
        return "damaging_card_vs_damaging_card"
    return None


def _same_card_profile_different_target(
    baseline: Mapping[str, object],
    candidate: Mapping[str, object],
) -> bool:
    if baseline.get("kind") != "play_card" or candidate.get("kind") != "play_card":
        return False
    baseline_target = _mapping_or_none(baseline.get("target"))
    candidate_target = _mapping_or_none(candidate.get("target"))
    if baseline_target is None or candidate_target is None:
        return False
    if _target_identity(baseline_target) == _target_identity(candidate_target):
        return False
    baseline_card = _mapping_or_none(baseline.get("card"))
    candidate_card = _mapping_or_none(candidate.get("card"))
    return baseline_card is not None and baseline_card == candidate_card


def _is_damaging_card_action(candidate: Mapping[str, object]) -> bool:
    if candidate.get("kind") != "play_card":
        return False
    target = _mapping_or_none(candidate.get("target"))
    card = _mapping_or_none(candidate.get("card"))
    if target is None or card is None:
        return False
    monster_index = _nonnegative_or_none(target.get("monster_index"))
    damage_by_monster = card.get("damage_by_monster_order")
    if (
        monster_index is not None
        and isinstance(damage_by_monster, Sequence)
        and not isinstance(damage_by_monster, (str, bytes))
        and monster_index < len(damage_by_monster)
    ):
        damage = _finite_number_or_none(damage_by_monster[monster_index])
        if damage is not None:
            return damage > 0.0
    damage = _finite_number_or_none(card.get("current_damage"))
    return damage is not None and damage > 0.0


def _target_identity(target: Mapping[str, object]) -> tuple[object, object]:
    return (target.get("monster_index"), target.get("slot"))


def _mapping_or_none(value: object) -> Mapping[str, object] | None:
    return value if isinstance(value, Mapping) else None


def _nonnegative_or_none(value: object) -> int | None:
    try:
        return _nonnegative(value, "value")
    except CombatPolicyDivergenceCollectionError:
        return None


def _finite_number_or_none(value: object) -> float | None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    normalized = float(value)
    return normalized if math.isfinite(normalized) else None


def _content_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, separators=(",", ":"), sort_keys=True).encode("utf-8")
    ).hexdigest()


def _sha256(value: object, name: str) -> str:
    if not isinstance(value, str) or len(value) != 64 or value != value.lower():
        raise CombatPolicyDivergenceCollectionError(
            f"{name} must be a lowercase SHA-256 digest"
        )
    try:
        bytes.fromhex(value)
    except ValueError as error:
        raise CombatPolicyDivergenceCollectionError(
            f"{name} must be a lowercase SHA-256 digest"
        ) from error
    return value


def _nonempty_text(value: object, name: str) -> str:
    if not isinstance(value, str) or not value:
        raise CombatPolicyDivergenceCollectionError(
            f"{name} must be non-empty text"
        )
    return value


def _positive(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized == 0:
        raise CombatPolicyDivergenceCollectionError(f"{name} must be positive")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatPolicyDivergenceCollectionError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatPolicyDivergenceCollectionError(
            f"{name} must be an integer"
        ) from error
    if normalized < 0:
        raise CombatPolicyDivergenceCollectionError(
            f"{name} must be non-negative"
        )
    return normalized


def _parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact", action="append", type=Path, required=True)
    parser.add_argument("--root-count", action="append", type=int, required=True)
    parser.add_argument("--baseline-behavior", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--max-decisions-per-root", type=int, default=4_096)
    parser.add_argument("--max-captures", type=int, default=64)
    parser.add_argument("--max-artifact-bytes", type=int, default=16 * 1024 * 1024)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parse_args(argv)
    if len(arguments.artifact) != len(arguments.root_count):
        raise CombatPolicyDivergenceCollectionError(
            "--artifact and --root-count must be supplied in aligned pairs"
        )
    run_combat_policy_divergence_collection(
        CombatPolicyDivergenceCollectionConfig(
            inputs=tuple(
                CombatPolicyDivergenceInput(artifact, root_count)
                for artifact, root_count in zip(
                    arguments.artifact,
                    arguments.root_count,
                    strict=True,
                )
            ),
            baseline_behavior=arguments.baseline_behavior,
            candidate=arguments.candidate,
            output=arguments.output_dir,
            max_decisions_per_root=arguments.max_decisions_per_root,
            max_captures=arguments.max_captures,
            max_artifact_bytes=arguments.max_artifact_bytes,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
