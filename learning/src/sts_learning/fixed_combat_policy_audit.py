"""Compare two frozen policies at one unchanged exact combat decision."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

import torch

from .combat_decision_audit import (
    CombatDecisionAudit,
    CombatDecisionAuditError,
    read_combat_decision_audit,
)
from .combat_potion_lane import (
    CombatPotionLane,
    CombatPotionLaneError,
    CombatPotionLaneRootSource,
    normalize_combat_potion_slots,
)
from .combat_root_artifacts import (
    load_combat_root_source,
    read_combat_root_artifact,
)
from .combat_root_audit import CombatRootAuditError, read_combat_root_audit
from .manifests import ManifestArtifactId, ManifestArtifactKind
from .policy import BehaviorManifestId
from .published_combat_behavior import recover_published_combat_behavior
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinSessionLimits,
)
from .torch_policy import RaggedCandidateLogits, RaggedCategoricalPolicyConfig


AUDIT_SCHEMA = "sts-learning-fixed-combat-policy-audit-v1"


class FixedCombatPolicyAuditError(RuntimeError):
    """A fixed-decision comparison is malformed, mutable, or misaligned."""


@dataclass(frozen=True)
class FixedPolicyIdentity:
    manifest_id: BehaviorManifestId
    checkpoint_id: ManifestArtifactId
    training_step: int
    temperature: float

    def __post_init__(self) -> None:
        if not isinstance(self.manifest_id, BehaviorManifestId):
            raise FixedCombatPolicyAuditError(
                "fixed policy identity requires a typed manifest id"
            )
        if not isinstance(self.checkpoint_id, ManifestArtifactId):
            raise FixedCombatPolicyAuditError(
                "fixed policy identity requires a typed checkpoint id"
            )
        if self.checkpoint_id.kind is not ManifestArtifactKind.MODEL_CHECKPOINT:
            raise FixedCombatPolicyAuditError(
                "fixed policy identity checkpoint has the wrong kind"
            )
        object.__setattr__(
            self,
            "training_step",
            _nonnegative(self.training_step, "training_step"),
        )
        if isinstance(self.temperature, bool) or not isinstance(
            self.temperature,
            (int, float),
        ):
            raise FixedCombatPolicyAuditError(
                "fixed policy temperature must be a real number"
            )
        temperature = float(self.temperature)
        if not math.isfinite(temperature) or temperature <= 0.0:
            raise FixedCombatPolicyAuditError(
                "fixed policy temperature must be finite and positive"
            )
        object.__setattr__(self, "temperature", temperature)

    def as_mapping(self) -> dict[str, object]:
        return {
            "manifest_sha256": self.manifest_id.digest.hex(),
            "checkpoint_sha256": self.checkpoint_id.digest.hex(),
            "training_step": self.training_step,
            "temperature": self.temperature,
        }


@dataclass(frozen=True)
class FixedPolicyDecisionScores:
    identity: FixedPolicyIdentity
    logits: tuple[float, ...]
    probabilities: tuple[float, ...]
    ranks: tuple[int, ...]
    top_ordinal: int
    top_two_logit_margin: float | None

    def as_mapping(self) -> dict[str, object]:
        return {
            **self.identity.as_mapping(),
            "top_ordinal": self.top_ordinal,
            "top_two_logit_margin": self.top_two_logit_margin,
        }


@dataclass(frozen=True)
class FixedCombatCandidateComparison:
    candidate_id: str
    ordinal: int
    semantics: dict[str, object]
    baseline_logit: float
    baseline_probability: float
    baseline_rank: int
    candidate_logit: float
    candidate_probability: float
    candidate_rank: int

    def as_mapping(self) -> dict[str, object]:
        return {
            "candidate_id": self.candidate_id,
            "ordinal": self.ordinal,
            "semantics": self.semantics,
            "baseline": {
                "raw_logit": self.baseline_logit,
                "normalized_probability": self.baseline_probability,
                "rank": self.baseline_rank,
            },
            "candidate": {
                "raw_logit": self.candidate_logit,
                "normalized_probability": self.candidate_probability,
                "rank": self.candidate_rank,
            },
            "probability_delta": (
                self.candidate_probability - self.baseline_probability
            ),
            "rank_delta": self.candidate_rank - self.baseline_rank,
        }


@dataclass(frozen=True)
class FixedCombatPolicyAuditResult:
    audit_id: str
    decision_id: str
    artifact_sha256: str
    expected_roots: int
    root_slot: int
    source_root_id: str
    source_exact_combat_state_hash: str
    decision_root_id: str
    decision_exact_combat_state_hash: str
    semantic_schema_version: int
    potion_lane: CombatPotionLane
    potion_slots: tuple[int, ...]
    replay_prefix: tuple[dict[str, object], ...]
    decision: CombatDecisionAudit
    root_audit: Mapping[str, object]
    baseline: FixedPolicyDecisionScores
    candidate: FixedPolicyDecisionScores
    candidates: tuple[FixedCombatCandidateComparison, ...]

    def as_mapping(self) -> dict[str, object]:
        return {
            "schema": AUDIT_SCHEMA,
            "audit_id": self.audit_id,
            "decision_id": self.decision_id,
            "artifact_sha256": self.artifact_sha256,
            "expected_roots": self.expected_roots,
            "root_slot": self.root_slot,
            "source_root_id": self.source_root_id,
            "source_exact_combat_state_hash": self.source_exact_combat_state_hash,
            "decision_root_id": self.decision_root_id,
            "decision_exact_combat_state_hash": (
                self.decision_exact_combat_state_hash
            ),
            "semantic_schema_version": self.semantic_schema_version,
            "potion_lane": self.potion_lane.value,
            "potion_slots": self.potion_slots,
            "replay_prefix": self.replay_prefix,
            "phase": self.decision.phase,
            "selection_prefix": self.decision.selection_prefix,
            "root_audit": dict(self.root_audit),
            "baseline": self.baseline.as_mapping(),
            "candidate": self.candidate.as_mapping(),
            "candidates": tuple(item.as_mapping() for item in self.candidates),
        }


@dataclass(frozen=True)
class FixedCombatPolicyAuditConfig:
    artifact: Path
    baseline_behavior: Path
    candidate_behavior: Path
    output: Path
    expected_roots: int
    root_slot: int
    potion_lane: CombatPotionLane = CombatPotionLane.NEVER
    potion_slots: tuple[int, ...] = ()
    decision_ordinals: tuple[int, ...] = ()

    def __post_init__(self) -> None:
        artifact = Path(self.artifact).resolve()
        baseline = Path(self.baseline_behavior).resolve()
        candidate = Path(self.candidate_behavior).resolve()
        output = Path(self.output).resolve()
        if not artifact.is_file():
            raise FixedCombatPolicyAuditError(
                "fixed combat audit artifact is not a file"
            )
        if not baseline.is_dir() or not candidate.is_dir():
            raise FixedCombatPolicyAuditError(
                "fixed combat audit behaviors must be directories"
            )
        if baseline == candidate:
            raise FixedCombatPolicyAuditError(
                "fixed combat audit requires distinct behavior directories"
            )
        if output.exists() and (not output.is_dir() or any(output.iterdir())):
            raise FixedCombatPolicyAuditError(
                "fixed combat audit output must be absent or empty"
            )
        if any(
            output == behavior or behavior in output.parents
            for behavior in (baseline, candidate)
        ):
            raise FixedCombatPolicyAuditError(
                "fixed combat audit output must stay outside behavior directories"
            )
        expected_roots = _positive(self.expected_roots, "expected_roots")
        root_slot = _nonnegative(self.root_slot, "root_slot")
        if root_slot >= expected_roots:
            raise FixedCombatPolicyAuditError(
                "fixed combat audit root_slot must be below expected_roots"
            )
        if not isinstance(self.potion_lane, CombatPotionLane):
            raise FixedCombatPolicyAuditError(
                "fixed combat audit potion lane must be typed"
            )
        try:
            potion_slots = normalize_combat_potion_slots(
                self.potion_lane,
                self.potion_slots,
            )
        except CombatPotionLaneError as error:
            raise FixedCombatPolicyAuditError(str(error)) from error
        object.__setattr__(self, "artifact", artifact)
        object.__setattr__(self, "baseline_behavior", baseline)
        object.__setattr__(self, "candidate_behavior", candidate)
        object.__setattr__(self, "output", output)
        object.__setattr__(self, "expected_roots", expected_roots)
        object.__setattr__(self, "root_slot", root_slot)
        object.__setattr__(self, "potion_slots", potion_slots)
        object.__setattr__(
            self,
            "decision_ordinals",
            tuple(
                _nonnegative(ordinal, f"decision_ordinals[{index}]")
                for index, ordinal in enumerate(self.decision_ordinals)
            ),
        )


def compare_fixed_combat_decision(
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
    potion_lane: CombatPotionLane,
    potion_slots: Sequence[int] = (),
    decision_root_id: str | None = None,
    decision_exact_combat_state_hash: str | None = None,
    replay_prefix: Sequence[Mapping[str, object]] = (),
) -> FixedCombatPolicyAuditResult:
    """Score both policies on one bridge-owned, unchanged semantic batch."""

    if baseline_identity.manifest_id == candidate_identity.manifest_id:
        raise FixedCombatPolicyAuditError(
            "fixed combat audit requires distinct behavior manifests"
        )
    expected_roots = _positive(expected_roots, "expected_roots")
    root_slot = _nonnegative(root_slot, "root_slot")
    if root_slot >= expected_roots:
        raise FixedCombatPolicyAuditError(
            "fixed combat audit root_slot must be below expected_roots"
        )
    artifact_sha256 = _sha256(artifact_sha256, "artifact_sha256")
    if not isinstance(root_audit, Mapping):
        raise FixedCombatPolicyAuditError("fixed combat root audit must be a mapping")
    if not isinstance(potion_lane, CombatPotionLane):
        raise FixedCombatPolicyAuditError("fixed combat potion lane must be typed")
    try:
        normalized_slots = normalize_combat_potion_slots(potion_lane, potion_slots)
    except CombatPotionLaneError as error:
        raise FixedCombatPolicyAuditError(str(error)) from error

    try:
        decision = read_combat_decision_audit(group, 0)
    except CombatDecisionAuditError as error:
        raise FixedCombatPolicyAuditError(str(error)) from error
    if decision is None:
        raise FixedCombatPolicyAuditError(
            "fixed combat group is not at an auditable decision"
        )
    batch_source = getattr(group, "decision_batch", None)
    if not callable(batch_source):
        raise FixedCombatPolicyAuditError(
            "fixed combat group does not expose decision_batch()"
        )
    try:
        batch = batch_source(semantic=True)
    except Exception as error:
        raise FixedCombatPolicyAuditError(
            f"fixed combat semantic batch failed: {error}"
        ) from error
    if not isinstance(batch, Mapping):
        raise FixedCombatPolicyAuditError(
            "fixed combat semantic batch must be a mapping"
        )
    candidate_count = _single_candidate_count(batch)
    if candidate_count != len(decision.candidates):
        raise FixedCombatPolicyAuditError(
            "typed combat audit candidates disagree with the model batch"
        )
    semantic = batch.get("semantic")
    if not isinstance(semantic, Mapping):
        raise FixedCombatPolicyAuditError(
            "fixed combat batch omitted semantic input"
        )
    schema_version = _nonnegative(
        semantic.get("schema_version"),
        "semantic_schema_version",
    )
    source_root_id = _nonempty_text(
        getattr(group, "root_id", None),
        "source_root_id",
    )
    source_exact_hash = _sha256(
        getattr(group, "exact_combat_state_hash", None),
        "source_exact_combat_state_hash",
    )
    normalized_decision_root_id = _nonempty_text(
        source_root_id if decision_root_id is None else decision_root_id,
        "decision_root_id",
    )
    normalized_decision_exact_hash = _sha256(
        source_exact_hash
        if decision_exact_combat_state_hash is None
        else decision_exact_combat_state_hash,
        "decision_exact_combat_state_hash",
    )
    normalized_replay_prefix = tuple(dict(step) for step in replay_prefix)

    decision_payload = {
        "schema": "sts-learning-fixed-combat-decision-identity-v1",
        "source_root_id": source_root_id,
        "decision_root_id": normalized_decision_root_id,
        "decision_exact_combat_state_hash": normalized_decision_exact_hash,
        "semantic_schema_version": schema_version,
        "potion_lane": potion_lane.value,
        "potion_slots": normalized_slots,
        "replay_prefix": normalized_replay_prefix,
        "phase": decision.phase,
        "selection_prefix": decision.selection_prefix,
        "candidates": decision.candidates,
    }
    decision_id = _content_sha256(decision_payload)
    baseline_scores = _score_policy(
        baseline_policy,
        baseline_identity,
        batch,
        candidate_count,
    )
    candidate_scores = _score_policy(
        candidate_policy,
        candidate_identity,
        batch,
        candidate_count,
    )
    candidates = tuple(
        FixedCombatCandidateComparison(
            candidate_id=_content_sha256(
                {
                    "schema": "sts-learning-fixed-combat-candidate-identity-v1",
                    "decision_id": decision_id,
                    "ordinal": ordinal,
                    "semantics": semantics,
                }
            ),
            ordinal=ordinal,
            semantics=dict(semantics),
            baseline_logit=baseline_scores.logits[ordinal],
            baseline_probability=baseline_scores.probabilities[ordinal],
            baseline_rank=baseline_scores.ranks[ordinal],
            candidate_logit=candidate_scores.logits[ordinal],
            candidate_probability=candidate_scores.probabilities[ordinal],
            candidate_rank=candidate_scores.ranks[ordinal],
        )
        for ordinal, semantics in enumerate(decision.candidates)
    )
    result_payload = {
        "schema": AUDIT_SCHEMA,
        "decision_id": decision_id,
        "artifact_sha256": artifact_sha256,
        "expected_roots": expected_roots,
        "root_slot": root_slot,
        "baseline_manifest_sha256": baseline_identity.manifest_id.digest.hex(),
        "candidate_manifest_sha256": candidate_identity.manifest_id.digest.hex(),
    }
    return FixedCombatPolicyAuditResult(
        audit_id=_content_sha256(result_payload),
        decision_id=decision_id,
        artifact_sha256=artifact_sha256,
        expected_roots=expected_roots,
        root_slot=root_slot,
        source_root_id=source_root_id,
        source_exact_combat_state_hash=source_exact_hash,
        decision_root_id=normalized_decision_root_id,
        decision_exact_combat_state_hash=normalized_decision_exact_hash,
        semantic_schema_version=schema_version,
        potion_lane=potion_lane,
        potion_slots=normalized_slots,
        replay_prefix=normalized_replay_prefix,
        decision=decision,
        root_audit=dict(root_audit),
        baseline=baseline_scores,
        candidate=candidate_scores,
        candidates=candidates,
    )


def run_fixed_combat_policy_audit(
    config: FixedCombatPolicyAuditConfig,
    *,
    bridge: CombatSessionBridge | None = None,
    print_completion: bool = True,
) -> dict[str, object]:
    """Recover two publications and write one fresh fixed-root comparison."""

    if not isinstance(config, FixedCombatPolicyAuditConfig):
        raise FixedCombatPolicyAuditError(
            "fixed combat audit config must be typed"
        )
    active_bridge = bridge if bridge is not None else CombatSessionBridge.installed()
    if not isinstance(active_bridge, CombatSessionBridge):
        raise FixedCombatPolicyAuditError("fixed combat audit bridge must be typed")
    limits = CombatWinSessionLimits()
    artifact = read_combat_root_artifact(
        config.artifact,
        max_bytes=limits.max_artifact_bytes,
    )
    artifact_sha256 = hashlib.sha256(artifact).hexdigest()
    source = load_combat_root_source(
        active_bridge,
        artifact,
        expected_roots=config.expected_roots,
        max_bytes=limits.max_artifact_bytes,
    )
    try:
        root_audit = read_combat_root_audit(source, config.root_slot)
    except CombatRootAuditError as error:
        raise FixedCombatPolicyAuditError(str(error)) from error
    lane_source = CombatPotionLaneRootSource(
        source,
        config.potion_lane,
        config.potion_slots,
    )
    try:
        group = lane_source.combat_group(config.root_slot, 1)
    except Exception as error:
        raise FixedCombatPolicyAuditError(
            f"fixed combat group construction failed: {error}"
        ) from error
    replay_prefix = _replay_decision_prefix(group, config.decision_ordinals)
    if config.decision_ordinals:
        capture = getattr(group, "capture_recovery_root", None)
        if not callable(capture):
            raise FixedCombatPolicyAuditError(
                "fixed combat group cannot identify the replayed exact decision"
            )
        try:
            decision_root = capture(0)
        except Exception as error:
            raise FixedCombatPolicyAuditError(
                f"fixed combat decision identity capture failed: {error}"
            ) from error
        decision_root_id = _nonempty_text(
            getattr(decision_root, "root_id", None),
            "decision_root_id",
        )
        decision_exact_hash = _sha256(
            getattr(decision_root, "exact_combat_state_hash", None),
            "decision_exact_combat_state_hash",
        )
    else:
        decision_root_id = _nonempty_text(
            getattr(group, "root_id", None),
            "decision_root_id",
        )
        decision_exact_hash = _sha256(
            getattr(group, "exact_combat_state_hash", None),
            "decision_exact_combat_state_hash",
        )
    baseline = recover_published_combat_behavior(
        config.baseline_behavior,
        active_bridge,
        limits,
        (0,),
    )
    candidate = recover_published_combat_behavior(
        config.candidate_behavior,
        active_bridge,
        limits,
        (0,),
    )
    baseline_policy = baseline.policies[0]
    candidate_policy = candidate.policies[0]
    result = compare_fixed_combat_decision(
        group,
        baseline_policy,
        candidate_policy,
        baseline_identity=FixedPolicyIdentity(
            manifest_id=baseline.manifest_id,
            checkpoint_id=baseline.checkpoint_id,
            training_step=baseline.training_step,
            temperature=_policy_temperature(baseline_policy),
        ),
        candidate_identity=FixedPolicyIdentity(
            manifest_id=candidate.manifest_id,
            checkpoint_id=candidate.checkpoint_id,
            training_step=candidate.training_step,
            temperature=_policy_temperature(candidate_policy),
        ),
        artifact_sha256=artifact_sha256,
        expected_roots=config.expected_roots,
        root_slot=config.root_slot,
        root_audit=root_audit.as_mapping(),
        potion_lane=config.potion_lane,
        potion_slots=config.potion_slots,
        decision_root_id=decision_root_id,
        decision_exact_combat_state_hash=decision_exact_hash,
        replay_prefix=replay_prefix,
    )
    summary = result.as_mapping()
    config.output.mkdir(parents=True, exist_ok=True)
    audit_path = config.output / "policy-audit.json"
    with audit_path.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(summary, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    if print_completion:
        print(
            json.dumps(
                {
                    "audit": str(audit_path),
                    "audit_id": result.audit_id,
                    "decision_root_id": result.decision_root_id,
                    "baseline_top_ordinal": result.baseline.top_ordinal,
                    "candidate_top_ordinal": result.candidate.top_ordinal,
                },
                separators=(",", ":"),
                sort_keys=True,
            ),
            flush=True,
        )
    return summary


def _replay_decision_prefix(
    group: object,
    ordinals: Sequence[int],
) -> tuple[dict[str, object], ...]:
    """Replay an explicit model-decision ordinal stream to a new exact boundary."""

    trace: list[dict[str, object]] = []
    for decision_index, raw_ordinal in enumerate(ordinals):
        ordinal = _nonnegative(raw_ordinal, f"decision_ordinals[{decision_index}]")
        try:
            audit = read_combat_decision_audit(group, 0)
        except CombatDecisionAuditError as error:
            raise FixedCombatPolicyAuditError(str(error)) from error
        if audit is None:
            raise FixedCombatPolicyAuditError(
                f"decision prefix {decision_index} reached no auditable candidate surface"
            )
        if ordinal >= len(audit.candidates):
            raise FixedCombatPolicyAuditError(
                f"decision prefix ordinal {ordinal} exceeds candidate count "
                f"{len(audit.candidates)} at index {decision_index}"
            )
        trace.append(
            {
                "decision_index": decision_index,
                "phase": audit.phase,
                "selection_prefix": audit.selection_prefix,
                "selected_ordinal": ordinal,
                "selected_candidate": audit.candidates[ordinal],
            }
        )
        choose = getattr(group, "choose", None)
        if not callable(choose):
            raise FixedCombatPolicyAuditError(
                "fixed combat group does not expose choose()"
            )
        try:
            choose([ordinal])
        except Exception as error:
            raise FixedCombatPolicyAuditError(
                f"decision prefix {decision_index} was rejected: {error}"
            ) from error
        if bool(getattr(group, "ready", False)):
            step = getattr(group, "step", None)
            if not callable(step):
                raise FixedCombatPolicyAuditError(
                    "fixed combat group does not expose step()"
                )
            try:
                step()
            except Exception as error:
                raise FixedCombatPolicyAuditError(
                    f"decision prefix action {decision_index} failed: {error}"
                ) from error
            if _nonnegative(
                getattr(group, "terminal_count", None),
                "terminal_count",
            ):
                raise FixedCombatPolicyAuditError(
                    "decision prefix reached a terminal combat"
                )
    if ordinals:
        try:
            final = read_combat_decision_audit(group, 0)
        except CombatDecisionAuditError as error:
            raise FixedCombatPolicyAuditError(str(error)) from error
        if final is None or final.phase != "combat_root":
            raise FixedCombatPolicyAuditError(
                "decision prefix must end at an undecoded exact combat boundary"
            )
    return tuple(trace)


def _score_policy(
    policy: object,
    identity: FixedPolicyIdentity,
    batch: Mapping[str, object],
    candidate_count: int,
) -> FixedPolicyDecisionScores:
    manifest = getattr(policy, "behavior_manifest_id", None)
    if manifest != identity.manifest_id:
        raise FixedCombatPolicyAuditError(
            "fixed policy object changed its declared manifest identity"
        )
    score = getattr(policy, "score", None)
    if not callable(score):
        raise FixedCombatPolicyAuditError("fixed policy does not expose score()")
    try:
        logits = score(batch)
    except Exception as error:
        raise FixedCombatPolicyAuditError(f"fixed policy scoring failed: {error}") from error
    if not isinstance(logits, RaggedCandidateLogits):
        raise FixedCombatPolicyAuditError(
            "fixed policy score did not return ragged candidate logits"
        )
    splits = tuple(int(value) for value in logits.row_splits.detach().cpu().tolist())
    if splits != (0, candidate_count):
        raise FixedCombatPolicyAuditError(
            "fixed policy logits do not describe exactly one candidate row"
        )
    values_tensor = logits.values.detach().to(dtype=torch.float64, device="cpu")
    if not bool(torch.all(torch.isfinite(values_tensor))):
        raise FixedCombatPolicyAuditError("fixed policy logits must be finite")
    probabilities_tensor = torch.softmax(
        values_tensor / identity.temperature,
        dim=0,
    )
    values = tuple(float(value) for value in values_tensor.tolist())
    probabilities = tuple(float(value) for value in probabilities_tensor.tolist())
    ordering = sorted(range(candidate_count), key=lambda ordinal: (-values[ordinal], ordinal))
    ranks = [0] * candidate_count
    for rank, ordinal in enumerate(ordering, start=1):
        ranks[ordinal] = rank
    margin = None if candidate_count == 1 else values[ordering[0]] - values[ordering[1]]
    return FixedPolicyDecisionScores(
        identity=identity,
        logits=values,
        probabilities=probabilities,
        ranks=tuple(ranks),
        top_ordinal=ordering[0],
        top_two_logit_margin=margin,
    )


def _single_candidate_count(batch: Mapping[str, object]) -> int:
    raw = batch.get("candidate_counts")
    try:
        values = tuple(raw)  # type: ignore[arg-type]
    except TypeError as error:
        raise FixedCombatPolicyAuditError(
            "fixed combat candidate counts must be a vector"
        ) from error
    if len(values) != 1:
        raise FixedCombatPolicyAuditError(
            "fixed combat audit requires exactly one decision row"
        )
    return _positive(values[0], "candidate_count")


def _policy_temperature(policy: object) -> float:
    config = getattr(policy, "config", None)
    if not isinstance(config, RaggedCategoricalPolicyConfig):
        raise FixedCombatPolicyAuditError(
            "fixed policy omitted its categorical rule configuration"
        )
    return config.temperature


def _content_sha256(value: object) -> str:
    try:
        encoded = json.dumps(
            value,
            ensure_ascii=True,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise FixedCombatPolicyAuditError(
            "fixed combat identity payload is not canonical JSON"
        ) from error
    return hashlib.sha256(encoded).hexdigest()


def _sha256(value: object, name: str) -> str:
    if not isinstance(value, str) or len(value) != 64:
        raise FixedCombatPolicyAuditError(f"{name} must be lowercase SHA-256 text")
    try:
        bytes.fromhex(value)
    except ValueError as error:
        raise FixedCombatPolicyAuditError(
            f"{name} must be lowercase SHA-256 text"
        ) from error
    if value != value.lower():
        raise FixedCombatPolicyAuditError(f"{name} must be lowercase SHA-256 text")
    return value


def _nonempty_text(value: object, name: str) -> str:
    if not isinstance(value, str) or not value:
        raise FixedCombatPolicyAuditError(f"{name} must be non-empty text")
    return value


def _positive(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized == 0:
        raise FixedCombatPolicyAuditError(f"{name} must be positive")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise FixedCombatPolicyAuditError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise FixedCombatPolicyAuditError(f"{name} must be an integer") from error
    if normalized < 0:
        raise FixedCombatPolicyAuditError(f"{name} must be non-negative")
    return normalized


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Compare two frozen policies at one exact combat root decision.",
    )
    parser.add_argument("--artifact", required=True, type=Path)
    parser.add_argument("--baseline-behavior", required=True, type=Path)
    parser.add_argument("--candidate-behavior", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--roots", required=True, type=int)
    parser.add_argument("--root-slot", required=True, type=int)
    parser.add_argument("--decision-ordinal", action="append", default=[], type=int)
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
        config = FixedCombatPolicyAuditConfig(
            artifact=arguments.artifact,
            baseline_behavior=arguments.baseline_behavior,
            candidate_behavior=arguments.candidate_behavior,
            output=arguments.output,
            expected_roots=arguments.roots,
            root_slot=arguments.root_slot,
            potion_lane=CombatPotionLane(arguments.potion_lane),
            potion_slots=tuple(arguments.potion_slot),
            decision_ordinals=tuple(arguments.decision_ordinal),
        )
        run_fixed_combat_policy_audit(config)
    except (FixedCombatPolicyAuditError, OSError, ValueError) as error:
        raise SystemExit(str(error)) from error
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
