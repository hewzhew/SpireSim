"""Explicit persistence for an unqualified combat-search distillation scorer."""

from __future__ import annotations

import hashlib
import json
import math
import operator
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path

from .manifest_catalog import BoundedBehaviorManifestCatalog
from .manifests import BehaviorManifestRegistry, ManifestArtifactId, ManifestArtifactKind
from .policy import BehaviorManifestId
from .torch_behavior import TorchBehaviorPublisher
from .torch_checkpoints import BoundedTorchCheckpointStore
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits
from .torch_policy import RaggedCandidateScorer, RaggedScorerConfig
from .torch_provenance import (
    AdamTrainingConfig,
    COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS,
    COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
    combat_search_distillation_manifest_template,
)


COMBAT_SEARCH_DISTILLATION_CANDIDATE_SCHEMA = (
    "sts-learning-combat-search-distillation-candidate-v1"
)
COMBAT_SEARCH_DISTILLATION_CANDIDATE_FILENAME = "candidate.json"
_MAX_CANDIDATE_RECEIPT_BYTES = 64 * 1024


class CombatSearchDistillationCandidateError(RuntimeError):
    """An experimental candidate is incomplete, mutable, or incompatible."""


@dataclass(frozen=True)
class CombatSearchDistillationCandidate:
    """One explicitly unqualified scorer restored from exact durable identities."""

    root: Path
    candidate_id: str
    source_manifest_id: BehaviorManifestId
    manifest_id: BehaviorManifestId
    checkpoint_id: ManifestArtifactId
    training_corpus_sha256: str
    training_root_count: int
    training_proposal_count: int
    epochs: int
    learning_rate: float
    max_grad_norm: float
    loss: str
    scorer: RaggedCandidateScorer

    def __post_init__(self) -> None:
        if not self.root.is_absolute():
            raise CombatSearchDistillationCandidateError(
                "candidate root must be absolute"
            )
        _sha256(self.candidate_id, "candidate_id")
        _sha256(self.training_corpus_sha256, "training_corpus_sha256")
        if not isinstance(self.source_manifest_id, BehaviorManifestId):
            raise CombatSearchDistillationCandidateError(
                "candidate source manifest id must be typed"
            )
        if not isinstance(self.manifest_id, BehaviorManifestId):
            raise CombatSearchDistillationCandidateError(
                "candidate manifest id must be typed"
            )
        if (
            not isinstance(self.checkpoint_id, ManifestArtifactId)
            or self.checkpoint_id.kind is not ManifestArtifactKind.MODEL_CHECKPOINT
        ):
            raise CombatSearchDistillationCandidateError(
                "candidate checkpoint id must identify a model checkpoint"
            )
        _positive(self.training_root_count, "training_root_count")
        proposals = _nonnegative(
            self.training_proposal_count,
            "training_proposal_count",
        )
        if proposals > self.training_root_count:
            raise CombatSearchDistillationCandidateError(
                "training proposal count exceeds root count"
            )
        _positive(self.epochs, "epochs")
        _positive_float(self.learning_rate, "learning_rate")
        _positive_float(self.max_grad_norm, "max_grad_norm")
        _loss(self.loss)
        if not isinstance(self.scorer, RaggedCandidateScorer):
            raise CombatSearchDistillationCandidateError(
                "candidate did not restore the maintained scorer"
            )
        if self.scorer.training or any(
            parameter.requires_grad for parameter in self.scorer.parameters()
        ):
            raise CombatSearchDistillationCandidateError(
                "restored candidate scorer must be frozen"
            )


def publish_combat_search_distillation_candidate(
    root: str | Path,
    scorer: RaggedCandidateScorer,
    bridge: CombatSessionBridge,
    limits: CombatWinSessionLimits,
    *,
    source_manifest_id: BehaviorManifestId,
    training_corpus_sha256: str,
    training_root_count: int,
    training_proposal_count: int,
    epochs: int,
    learning_rate: float,
    max_grad_norm: float,
    loss: str = COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
) -> dict[str, object]:
    """Write one fresh candidate root without creating a production publication."""

    candidate_root = Path(root).resolve()
    if candidate_root.exists() or not candidate_root.parent.is_dir():
        raise CombatSearchDistillationCandidateError(
            "candidate root must be fresh below an existing directory"
        )
    if not isinstance(scorer, RaggedCandidateScorer):
        raise CombatSearchDistillationCandidateError(
            "candidate publication requires a maintained scorer"
        )
    if not isinstance(bridge, CombatSessionBridge):
        raise CombatSearchDistillationCandidateError(
            "candidate publication requires a typed bridge"
        )
    if not isinstance(limits, CombatWinSessionLimits):
        raise CombatSearchDistillationCandidateError(
            "candidate publication requires typed limits"
        )
    if not isinstance(source_manifest_id, BehaviorManifestId):
        raise CombatSearchDistillationCandidateError(
            "candidate publication requires a typed source manifest id"
        )
    corpus_digest = _sha256(training_corpus_sha256, "training_corpus_sha256")
    root_count = _positive(training_root_count, "training_root_count")
    proposal_count = _nonnegative(
        training_proposal_count,
        "training_proposal_count",
    )
    if proposal_count > root_count:
        raise CombatSearchDistillationCandidateError(
            "training proposal count exceeds root count"
        )
    normalized_epochs = _positive(epochs, "epochs")
    normalized_learning_rate = _positive_float(learning_rate, "learning_rate")
    normalized_grad_norm = _positive_float(max_grad_norm, "max_grad_norm")
    normalized_loss = _loss(loss)
    optimizer = AdamTrainingConfig(learning_rate=normalized_learning_rate)
    template = combat_search_distillation_manifest_template(
        bridge.semantic_schema,
        scorer.config,
        optimizer,
        epochs=normalized_epochs,
        max_grad_norm=normalized_grad_norm,
        loss=normalized_loss,
        device_type="cpu",
    )

    candidate_root.mkdir()
    store = BoundedTorchCheckpointStore(
        candidate_root / "behavior-checkpoints",
        limits.checkpoint_store,
    )
    catalog = BoundedBehaviorManifestCatalog(
        candidate_root / "behavior-manifests",
        limits.manifest_catalog,
    )
    publication = TorchBehaviorPublisher(
        store,
        catalog,
        BehaviorManifestRegistry(capacity=1),
        template,
    ).publish(scorer, training_step=normalized_epochs)
    payload: dict[str, object] = {
        "schema": COMBAT_SEARCH_DISTILLATION_CANDIDATE_SCHEMA,
        "status": "experimental_unqualified",
        "teacher_valid": False,
        "production_eligible": False,
        "model_published": False,
        "source_manifest_id": source_manifest_id.digest.hex(),
        "manifest_id": publication.manifest_id.digest.hex(),
        "checkpoint_id": publication.checkpoint_id.digest.hex(),
        "training_corpus_sha256": corpus_digest,
        "training_root_count": root_count,
        "training_proposal_count": proposal_count,
        "scorer": _scorer_config_payload(scorer.config),
        "optimizer": {
            "kind": "adam",
            "learning_rate": optimizer.learning_rate,
            "beta1": optimizer.beta1,
            "beta2": optimizer.beta2,
            "epsilon": optimizer.epsilon,
            "weight_decay": optimizer.weight_decay,
            "amsgrad": optimizer.amsgrad,
        },
        "training": {
            "epochs": normalized_epochs,
            "max_grad_norm": normalized_grad_norm,
            "loss": normalized_loss,
        },
    }
    payload["candidate_id"] = _candidate_identity(payload)
    receipt = candidate_root / COMBAT_SEARCH_DISTILLATION_CANDIDATE_FILENAME
    try:
        with receipt.open("x", encoding="utf-8", newline="\n") as destination:
            json.dump(payload, destination, separators=(",", ":"), sort_keys=True)
            destination.write("\n")
    except OSError as error:
        raise CombatSearchDistillationCandidateError(
            "candidate receipt could not be committed"
        ) from error
    return payload


def recover_combat_search_distillation_candidate(
    root: str | Path,
    bridge: CombatSessionBridge,
    limits: CombatWinSessionLimits,
) -> CombatSearchDistillationCandidate:
    """Explicitly restore one candidate while preserving production rejection."""

    candidate_root = Path(root).resolve()
    if not candidate_root.is_dir():
        raise CombatSearchDistillationCandidateError(
            "combat-search candidate is not a directory"
        )
    if not isinstance(bridge, CombatSessionBridge):
        raise CombatSearchDistillationCandidateError(
            "candidate recovery requires a typed bridge"
        )
    if not isinstance(limits, CombatWinSessionLimits):
        raise CombatSearchDistillationCandidateError(
            "candidate recovery requires typed limits"
        )
    if (candidate_root / "training.jsonl").exists():
        raise CombatSearchDistillationCandidateError(
            "experimental candidate must not contain a production training journal"
        )
    payload = _read_candidate_receipt(
        candidate_root / COMBAT_SEARCH_DISTILLATION_CANDIDATE_FILENAME
    )
    if payload.get("schema") != COMBAT_SEARCH_DISTILLATION_CANDIDATE_SCHEMA:
        raise CombatSearchDistillationCandidateError(
            "unsupported combat-search candidate schema"
        )
    if (
        payload.get("status") != "experimental_unqualified"
        or payload.get("teacher_valid") is not False
        or payload.get("production_eligible") is not False
        or payload.get("model_published") is not False
    ):
        raise CombatSearchDistillationCandidateError(
            "combat-search candidate claims unsupported authority"
        )
    claimed_candidate_id = _sha256(payload.get("candidate_id"), "candidate_id")
    identity_payload = dict(payload)
    del identity_payload["candidate_id"]
    if _candidate_identity(identity_payload) != claimed_candidate_id:
        raise CombatSearchDistillationCandidateError(
            "candidate receipt identity does not match its content"
        )

    source_manifest_id = _manifest_id(
        payload.get("source_manifest_id"),
        "source_manifest_id",
    )
    manifest_id = _manifest_id(payload.get("manifest_id"), "manifest_id")
    checkpoint_id = ManifestArtifactId(
        ManifestArtifactKind.MODEL_CHECKPOINT,
        bytes.fromhex(_sha256(payload.get("checkpoint_id"), "checkpoint_id")),
    )
    corpus_digest = _sha256(
        payload.get("training_corpus_sha256"),
        "training_corpus_sha256",
    )
    root_count = _positive(payload.get("training_root_count"), "training_root_count")
    proposal_count = _nonnegative(
        payload.get("training_proposal_count"),
        "training_proposal_count",
    )
    if proposal_count > root_count:
        raise CombatSearchDistillationCandidateError(
            "training proposal count exceeds root count"
        )
    scorer_config = _scorer_config(_mapping(payload.get("scorer"), "scorer"))
    optimizer_payload = _mapping(payload.get("optimizer"), "optimizer")
    if optimizer_payload.get("kind") != "adam":
        raise CombatSearchDistillationCandidateError(
            "candidate optimizer kind is unsupported"
        )
    optimizer = AdamTrainingConfig(
        learning_rate=_positive_float(
            optimizer_payload.get("learning_rate"),
            "learning_rate",
        ),
        beta1=_float(optimizer_payload.get("beta1"), "beta1"),
        beta2=_float(optimizer_payload.get("beta2"), "beta2"),
        epsilon=_positive_float(optimizer_payload.get("epsilon"), "epsilon"),
        weight_decay=_nonnegative_float(
            optimizer_payload.get("weight_decay"),
            "weight_decay",
        ),
        amsgrad=_bool(optimizer_payload.get("amsgrad"), "amsgrad"),
    )
    training = _mapping(payload.get("training"), "training")
    epochs = _positive(training.get("epochs"), "epochs")
    max_grad_norm = _positive_float(
        training.get("max_grad_norm"),
        "max_grad_norm",
    )
    loss = _loss(training.get("loss"))

    store = BoundedTorchCheckpointStore(
        candidate_root / "behavior-checkpoints",
        limits.checkpoint_store,
    )
    catalog = BoundedBehaviorManifestCatalog(
        candidate_root / "behavior-manifests",
        limits.manifest_catalog,
    )
    if store.snapshot.checkpoints != 1 or catalog.snapshot.manifests != 1:
        raise CombatSearchDistillationCandidateError(
            "candidate must contain exactly one checkpoint and manifest"
        )
    if catalog.manifest_ids != (manifest_id,):
        raise CombatSearchDistillationCandidateError(
            "candidate receipt and durable manifest identity disagree"
        )
    manifest = catalog.resolve(manifest_id)
    expected = combat_search_distillation_manifest_template(
        bridge.semantic_schema,
        scorer_config,
        optimizer,
        epochs=epochs,
        max_grad_norm=max_grad_norm,
        loss=loss,
        device_type="cpu",
    ).bind(checkpoint_id, training_step=epochs)
    if manifest != expected:
        raise CombatSearchDistillationCandidateError(
            "candidate manifest does not match the installed distillation profile"
        )

    def scorer_factory() -> RaggedCandidateScorer:
        return RaggedCandidateScorer.from_bridge_schema(
            bridge.semantic_schema,
            scorer_config,
        ).to("cpu")

    try:
        scorer = store.materialize(checkpoint_id, scorer_factory)
    except RuntimeError as error:
        raise CombatSearchDistillationCandidateError(
            "candidate checkpoint cannot initialize the maintained scorer"
        ) from error
    if not isinstance(scorer, RaggedCandidateScorer):
        raise CombatSearchDistillationCandidateError(
            "candidate factory returned the wrong model type"
        )
    scorer.eval()
    scorer.requires_grad_(False)
    return CombatSearchDistillationCandidate(
        root=candidate_root,
        candidate_id=claimed_candidate_id,
        source_manifest_id=source_manifest_id,
        manifest_id=manifest_id,
        checkpoint_id=checkpoint_id,
        training_corpus_sha256=corpus_digest,
        training_root_count=root_count,
        training_proposal_count=proposal_count,
        epochs=epochs,
        learning_rate=optimizer.learning_rate,
        max_grad_norm=max_grad_norm,
        loss=loss,
        scorer=scorer,
    )


def _read_candidate_receipt(path: Path) -> dict[str, object]:
    if not path.is_file():
        raise CombatSearchDistillationCandidateError(
            "candidate is missing candidate.json"
        )
    try:
        if path.stat().st_size > _MAX_CANDIDATE_RECEIPT_BYTES:
            raise CombatSearchDistillationCandidateError(
                "candidate receipt exceeds its byte limit"
            )
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CombatSearchDistillationCandidateError(
            "candidate receipt could not be read"
        ) from error
    if not isinstance(value, dict):
        raise CombatSearchDistillationCandidateError(
            "candidate receipt must be an object"
        )
    return value


def _candidate_identity(payload: Mapping[str, object]) -> str:
    content = json.dumps(
        payload,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(content).hexdigest()


def _scorer_config_payload(config: RaggedScorerConfig) -> dict[str, object]:
    return {
        "hidden_dim": config.hidden_dim,
        "relation_layers": config.relation_layers,
        "value_head": config.value_head,
        "value_head_width": config.value_head_width,
    }


def _scorer_config(payload: Mapping[str, object]) -> RaggedScorerConfig:
    return RaggedScorerConfig(
        hidden_dim=_positive(payload.get("hidden_dim"), "hidden_dim"),
        relation_layers=_nonnegative(
            payload.get("relation_layers"),
            "relation_layers",
        ),
        value_head=_bool(payload.get("value_head"), "value_head"),
        value_head_width=_positive(
            payload.get("value_head_width"),
            "value_head_width",
        ),
    )


def _mapping(value: object, name: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise CombatSearchDistillationCandidateError(f"{name} must be an object")
    return value


def _manifest_id(value: object, name: str) -> BehaviorManifestId:
    return BehaviorManifestId(bytes.fromhex(_sha256(value, name)))


def _sha256(value: object, name: str) -> str:
    if not isinstance(value, str) or len(value) != 64 or value != value.lower():
        raise CombatSearchDistillationCandidateError(
            f"{name} must be a lowercase SHA-256 digest"
        )
    try:
        bytes.fromhex(value)
    except ValueError as error:
        raise CombatSearchDistillationCandidateError(
            f"{name} must be a lowercase SHA-256 digest"
        ) from error
    return value


def _positive(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized == 0:
        raise CombatSearchDistillationCandidateError(f"{name} must be positive")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatSearchDistillationCandidateError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatSearchDistillationCandidateError(
            f"{name} must be an integer"
        ) from error
    if normalized < 0:
        raise CombatSearchDistillationCandidateError(
            f"{name} must be non-negative"
        )
    return normalized


def _float(value: object, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise CombatSearchDistillationCandidateError(f"{name} must be numeric")
    normalized = float(value)
    if not math.isfinite(normalized):
        raise CombatSearchDistillationCandidateError(f"{name} must be finite")
    return normalized


def _positive_float(value: object, name: str) -> float:
    normalized = _float(value, name)
    if normalized <= 0.0:
        raise CombatSearchDistillationCandidateError(f"{name} must be positive")
    return normalized


def _nonnegative_float(value: object, name: str) -> float:
    normalized = _float(value, name)
    if normalized < 0.0:
        raise CombatSearchDistillationCandidateError(
            f"{name} must be non-negative"
        )
    return normalized


def _loss(value: object) -> str:
    if value not in {
        COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS,
        COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
    }:
        raise CombatSearchDistillationCandidateError(
            "candidate loss contract is unsupported"
        )
    assert isinstance(value, str)
    return value


def _bool(value: object, name: str) -> bool:
    if type(value) is not bool:
        raise CombatSearchDistillationCandidateError(f"{name} must be bool")
    return value
