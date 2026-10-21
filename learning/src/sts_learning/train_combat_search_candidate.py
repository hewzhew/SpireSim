"""Fit and retain one unqualified combat-search candidate from training rows only."""

from __future__ import annotations

import argparse
import json
from collections.abc import Sequence
from pathlib import Path

from .combat_search_distillation_candidate import (
    publish_combat_search_distillation_candidate,
    recover_combat_search_distillation_candidate,
)
from .combat_search_distillation_spike import (
    CombatSearchDistillationError,
    combat_search_distillation_corpus_sha256,
    combat_search_distillation_logit_parity,
    combat_search_distillation_partition_metrics,
    combat_search_distillation_partition_receipt,
    fit_combat_search_distillation_scorer,
    load_combat_search_distillation_partition,
)
from .published_combat_behavior import recover_compatible_combat_scorer
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits
from .torch_provenance import COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS


class TrainCombatSearchCandidateError(RuntimeError):
    """The training corpus or explicit candidate publication was inconsistent."""


def run_train_combat_search_candidate(
    *,
    training_pairs: Sequence[tuple[Path, Path]],
    behavior: Path,
    candidate_output: Path,
    output: Path,
    epochs: int,
    learning_rate: float,
    max_grad_norm: float,
    max_artifact_bytes: int,
) -> dict[str, object]:
    """Train once from fixed rows, publish a candidate, and verify exact reload."""

    candidate_path = Path(candidate_output).resolve()
    result_path = Path(output).resolve()
    if result_path.exists() or not result_path.parent.is_dir():
        raise TrainCombatSearchCandidateError(
            "training result must be a fresh file below an existing directory"
        )
    if candidate_path.exists() or not candidate_path.parent.is_dir():
        raise TrainCombatSearchCandidateError(
            "candidate output must be fresh below an existing directory"
        )
    if candidate_path == result_path:
        raise TrainCombatSearchCandidateError(
            "candidate and training result outputs must differ"
        )
    if not training_pairs:
        raise TrainCombatSearchCandidateError(
            "candidate training requires at least one corpus pair"
        )

    bridge = CombatSessionBridge.installed()
    limits = CombatWinSessionLimits(max_artifact_bytes=max_artifact_bytes)
    try:
        training = load_combat_search_distillation_partition(
            training_pairs,
            bridge=bridge,
            limits=limits,
            max_artifact_bytes=max_artifact_bytes,
            require_unique_seeds=False,
        )
        if not training["proposal_records"]:
            raise TrainCombatSearchCandidateError(
                "candidate training requires at least one strict proposal"
            )
        warm_start = recover_compatible_combat_scorer(
            behavior,
            bridge,
            limits,
        )
        if (
            training["search_anchor_manifest_id"]
            != warm_start.source_manifest_id.digest.hex()
        ):
            raise TrainCombatSearchCandidateError(
                "training search evidence and warm-start behavior disagree"
            )
        initial = combat_search_distillation_partition_metrics(
            warm_start.scorer,
            training,
            limits,
        )
        scorer, optimizer, losses, gradient_norms, loss_components = (
            fit_combat_search_distillation_scorer(
                warm_start.scorer,
                training,
                limits,
                epochs=epochs,
                learning_rate=learning_rate,
                max_grad_norm=max_grad_norm,
                loss=COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
            )
        )
        final = combat_search_distillation_partition_metrics(
            scorer,
            training,
            limits,
        )
        candidate_receipt = publish_combat_search_distillation_candidate(
            candidate_path,
            scorer,
            bridge,
            limits,
            source_manifest_id=warm_start.source_manifest_id,
            training_corpus_sha256=combat_search_distillation_corpus_sha256(
                training
            ),
            training_root_count=len(training["records"]),
            training_proposal_count=len(training["proposal_records"]),
            epochs=epochs,
            learning_rate=optimizer.learning_rate,
            max_grad_norm=max_grad_norm,
            loss=COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
        )
        restored = recover_combat_search_distillation_candidate(
            candidate_path,
            bridge,
            limits,
        )
        reload_parity = combat_search_distillation_logit_parity(
            scorer,
            restored.scorer,
            training,
            limits,
        )
    except CombatSearchDistillationError as error:
        raise TrainCombatSearchCandidateError(str(error)) from error
    if not (
        reload_parity["logits_equal"]
        and reload_parity["greedy_ordinals_equal"]
    ):
        raise TrainCombatSearchCandidateError(
            "reloaded candidate changed training logits or greedy actions"
        )

    result = {
        "schema": "sts-learning-combat-search-candidate-training-v1",
        "claim": "bounded_training_rows_candidate_only",
        "teacher_valid": False,
        "model_published": False,
        "production_eligible": False,
        "source_manifest_id": warm_start.source_manifest_id.digest.hex(),
        "training": combat_search_distillation_partition_receipt(training),
        "config": {
            "epochs": epochs,
            "learning_rate": optimizer.learning_rate,
            "max_grad_norm": max_grad_norm,
            "loss": COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
        },
        "updates": {
            "losses": losses,
            "gradient_norms": gradient_norms,
            "loss_components": loss_components,
        },
        "initial": initial,
        "final": final,
        "candidate": {
            "artifact": str(candidate_path),
            "candidate_id": candidate_receipt["candidate_id"],
            "manifest_id": restored.manifest_id.digest.hex(),
            "checkpoint_id": restored.checkpoint_id.digest.hex(),
            "status": "experimental_unqualified",
            "reload_parity": reload_parity,
        },
    }
    with result_path.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(result, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    print(
        json.dumps(
            {
                "schema": result["schema"],
                "artifact": str(result_path),
                "candidate": result["candidate"],
                "training_roots": result["training"]["root_count"],
                "training_proposals": result["training"]["proposal_count"],
                "initial": _without_rows(initial),
                "final": _without_rows(final),
            },
            separators=(",", ":"),
            sort_keys=True,
        ),
        flush=True,
    )
    return result


def _without_rows(value: dict[str, object]) -> dict[str, object]:
    return {key: item for key, item in value.items() if key != "rows"}


def _pair_arguments(
    artifacts: Sequence[Path],
    manifests: Sequence[Path],
) -> tuple[tuple[Path, Path], ...]:
    if len(artifacts) != len(manifests) or not artifacts:
        raise TrainCombatSearchCandidateError(
            "training artifacts and search manifests must be non-empty and aligned"
        )
    return tuple(zip(artifacts, manifests, strict=True))


def _parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--training-artifact", type=Path, action="append", required=True)
    parser.add_argument("--training-search", type=Path, action="append", required=True)
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--candidate-output", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--epochs", type=int, default=1)
    parser.add_argument("--learning-rate", type=float, default=3e-4)
    parser.add_argument("--max-grad-norm", type=float, default=1.0)
    parser.add_argument("--max-artifact-bytes", type=int, default=16 * 1024 * 1024)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parse_args(argv)
    run_train_combat_search_candidate(
        training_pairs=_pair_arguments(
            arguments.training_artifact,
            arguments.training_search,
        ),
        behavior=arguments.behavior,
        candidate_output=arguments.candidate_output,
        output=arguments.output,
        epochs=arguments.epochs,
        learning_rate=arguments.learning_rate,
        max_grad_norm=arguments.max_grad_norm,
        max_artifact_bytes=arguments.max_artifact_bytes,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
