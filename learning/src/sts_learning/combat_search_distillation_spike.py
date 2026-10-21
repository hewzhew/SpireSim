"""Distill searched combat decision trajectories and run full-combat checks."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import operator
import statistics
import time
from collections.abc import Mapping, Sequence
from pathlib import Path

import torch

from .combat_driver import CombatGroupDriver
from .combat_experience import CombatExperienceLimits
from .combat_root_artifacts import load_combat_root_source, read_combat_root_artifact
from .combat_search_distillation_candidate import (
    publish_combat_search_distillation_candidate,
    recover_combat_search_distillation_candidate,
)
from .policy import BatchPolicyChoice, BehaviorManifestId
from .published_combat_behavior import recover_compatible_combat_scorer
from .semantic_concat import concatenate_semantic_decision_batches
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits
from .torch_policy import (
    RaggedCandidateLogits,
    RaggedCandidateScorer,
    ragged_cross_entropy,
)
from .torch_provenance import (
    AdamTrainingConfig,
    COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS,
    COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
)


class CombatSearchDistillationError(RuntimeError):
    """Search evidence, semantic rows, or the bounded update was inconsistent."""


def run_combat_search_distillation_spike(
    *,
    training_pairs: Sequence[tuple[Path, Path]],
    held_out_pairs: Sequence[tuple[Path, Path]],
    behavior: Path,
    output: Path,
    candidate_output: Path | None = None,
    epochs: int,
    learning_rate: float,
    max_grad_norm: float,
    max_artifact_bytes: int,
) -> dict[str, object]:
    """Fit searched trajectory rows and evaluate disjoint complete combats."""

    epochs = _positive(epochs, "epochs")
    learning_rate = _positive_float(learning_rate, "learning_rate")
    max_grad_norm = _positive_float(max_grad_norm, "max_grad_norm")
    max_artifact_bytes = _positive(max_artifact_bytes, "max_artifact_bytes")
    output = Path(output).resolve()
    if output.exists() or not output.parent.is_dir():
        raise CombatSearchDistillationError(
            "output must be a fresh file below an existing directory"
        )
    candidate_output = (
        None if candidate_output is None else Path(candidate_output).resolve()
    )
    if candidate_output == output:
        raise CombatSearchDistillationError(
            "candidate output and result output must be distinct"
        )
    if candidate_output is not None and (
        candidate_output.exists() or not candidate_output.parent.is_dir()
    ):
        raise CombatSearchDistillationError(
            "candidate output must be fresh below an existing directory"
        )
    if not training_pairs or not held_out_pairs:
        raise CombatSearchDistillationError(
            "distillation requires training and held-out corpus pairs"
        )

    bridge = CombatSessionBridge.installed()
    limits = CombatWinSessionLimits(max_artifact_bytes=max_artifact_bytes)
    training = load_combat_search_distillation_partition(
        training_pairs,
        bridge=bridge,
        limits=limits,
        max_artifact_bytes=max_artifact_bytes,
        require_unique_seeds=False,
    )
    held_out = load_combat_search_distillation_partition(
        held_out_pairs,
        bridge=bridge,
        limits=limits,
        max_artifact_bytes=max_artifact_bytes,
        require_unique_seeds=True,
    )
    overlap = set(training["seeds"]) & set(held_out["seeds"])
    if overlap:
        raise CombatSearchDistillationError(
            "training and held-out natural roots share run seeds"
        )
    if not training["proposal_records"] or not held_out["proposal_records"]:
        raise CombatSearchDistillationError(
            "both partitions require at least one strict search proposal"
        )

    warm_start = recover_compatible_combat_scorer(
        behavior,
        bridge,
        limits,
    )
    expected_anchor_id = warm_start.source_manifest_id.digest.hex()
    for name, partition in (("training", training), ("held-out", held_out)):
        if partition["search_anchor_manifest_id"] != expected_anchor_id:
            raise CombatSearchDistillationError(
                f"{name} search evidence and warm-start behavior disagree"
            )
    anchor = warm_start.scorer
    if anchor.training or any(parameter.requires_grad for parameter in anchor.parameters()):
        raise CombatSearchDistillationError("recovered anchor must be frozen")

    started = time.perf_counter()
    initial_training = combat_search_distillation_partition_metrics(
        anchor, training, limits
    )
    initial_held_out = combat_search_distillation_partition_metrics(
        anchor, held_out, limits
    )
    rollout_limits = CombatExperienceLimits(
        max_decisions=2_048,
        max_payload_bytes=64 * 1024 * 1024,
        max_model_rounds=1_024,
        max_transitions=4_096,
    )
    initial_full_combat = _full_combat_metrics(
        anchor,
        held_out,
        warm_start.source_manifest_id,
        rollout_limits,
    )
    scorer, optimizer_config, update_losses, gradient_norms, loss_components = (
        fit_combat_search_distillation_scorer(
            anchor,
            training,
            limits,
            epochs=epochs,
            learning_rate=learning_rate,
            max_grad_norm=max_grad_norm,
            loss=COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
        )
    )

    final_training = combat_search_distillation_partition_metrics(
        scorer, training, limits
    )
    final_held_out = combat_search_distillation_partition_metrics(
        scorer, held_out, limits
    )
    candidate_receipt: dict[str, object] | None = None
    recovered_candidate = None
    if candidate_output is None:
        candidate_manifest_id = BehaviorManifestId(
            hashlib.sha256(
                warm_start.source_manifest_id.digest
                + b"combat-search-trajectory-distillation-spike-v3"
                + str(output).encode("utf-8")
            ).digest()
        )
    else:
        candidate_receipt = publish_combat_search_distillation_candidate(
            candidate_output,
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
            learning_rate=optimizer_config.learning_rate,
            max_grad_norm=max_grad_norm,
            loss=COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
        )
        recovered_candidate = recover_combat_search_distillation_candidate(
            candidate_output,
            bridge,
            limits,
        )
        candidate_manifest_id = recovered_candidate.manifest_id
    final_full_combat = _full_combat_metrics(
        scorer,
        held_out,
        candidate_manifest_id,
        rollout_limits,
    )
    candidate_summary = None
    if recovered_candidate is not None and candidate_receipt is not None:
        reload_parity = _candidate_reload_parity(
            scorer,
            recovered_candidate.scorer,
            training,
            held_out,
            candidate_manifest_id,
            limits,
            rollout_limits,
            final_full_combat,
        )
        candidate_summary = {
            "artifact": str(candidate_output),
            "candidate_id": recovered_candidate.candidate_id,
            "manifest_id": recovered_candidate.manifest_id.digest.hex(),
            "checkpoint_id": recovered_candidate.checkpoint_id.digest.hex(),
            "status": candidate_receipt["status"],
            "teacher_valid": False,
            "production_eligible": False,
            "model_published": False,
            "reload_parity": reload_parity,
        }
    full_combat_delta = _compare_full_combat_metrics(
        initial_full_combat,
        final_full_combat,
    )
    result = {
        "schema": "sts-learning-combat-search-distillation-spike-v3",
        "teacher_valid": False,
        "model_published": False,
        "claim": "bounded_search_trajectory_full_combat_feasibility_only",
        "training_source_manifest_id": warm_start.source_manifest_id.digest.hex(),
        "candidate": candidate_summary,
        "training": combat_search_distillation_partition_receipt(training),
        "held_out": combat_search_distillation_partition_receipt(held_out),
        "seed_overlap": tuple(sorted(overlap)),
        "config": {
            "epochs": epochs,
            "learning_rate": learning_rate,
            "max_grad_norm": max_grad_norm,
            "optimizer": "fresh_adam",
            "loss": COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
            "potion_lane": "never",
            "held_out_full_combat_replicates_per_root": 2,
            "held_out_full_combat_rule": "frozen_greedy_model_only_no_search_suffix",
        },
        "updates": {
            "losses": tuple(update_losses),
            "gradient_norms": tuple(gradient_norms),
            "loss_components": loss_components,
        },
        "initial": {
            "training": initial_training,
            "held_out": initial_held_out,
            "held_out_full_combat": initial_full_combat,
        },
        "final": {
            "training": final_training,
            "held_out": final_held_out,
            "held_out_full_combat": final_full_combat,
        },
        "held_out_delta": {
            "proposal_correct": (
                final_held_out["proposal_correct"]
                - initial_held_out["proposal_correct"]
            ),
            "proposal_cross_entropy": (
                final_held_out["proposal_cross_entropy"]
                - initial_held_out["proposal_cross_entropy"]
            ),
            "mean_both_win_hp_vs_baseline": _optional_delta(
                final_held_out["mean_both_win_hp_vs_baseline"],
                initial_held_out["mean_both_win_hp_vs_baseline"],
            ),
            "regressed_root_count": (
                final_held_out["regressed_root_count"]
                - initial_held_out["regressed_root_count"]
            ),
        },
        "held_out_full_combat_delta": full_combat_delta,
        "elapsed_seconds": time.perf_counter() - started,
    }
    with output.open("x", encoding="utf-8", newline="\n") as destination:
        json.dump(result, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    receipt = {
        "schema": result["schema"],
        "teacher_valid": False,
        "model_published": False,
        "candidate": candidate_summary,
        "training_proposals": result["training"]["proposal_count"],
        "held_out_proposals": result["held_out"]["proposal_count"],
        "initial_held_out": _without_rows(initial_held_out),
        "final_held_out": _without_rows(final_held_out),
        "held_out_delta": result["held_out_delta"],
        "initial_held_out_full_combat": _without_rows(initial_full_combat),
        "final_held_out_full_combat": _without_rows(final_full_combat),
        "held_out_full_combat_delta": _without_rows(full_combat_delta),
        "artifact": str(output),
    }
    print(json.dumps(receipt, separators=(",", ":"), sort_keys=True), flush=True)
    return result


def load_combat_search_distillation_partition(
    pairs: Sequence[tuple[Path, Path]],
    *,
    bridge: CombatSessionBridge,
    limits: CombatWinSessionLimits,
    max_artifact_bytes: int,
    require_unique_seeds: bool,
) -> dict[str, object]:
    records: list[dict[str, object]] = []
    sources: list[dict[str, object]] = []
    seeds: list[int] = []
    root_ids: list[str] = []
    record_index_by_root_id: dict[str, int] = {}
    duplicate_root_occurrence_count = 0
    search_anchor_manifest_ids: set[str] = set()
    for artifact_raw, manifest_raw in pairs:
        artifact = Path(artifact_raw).resolve()
        manifest_path = Path(manifest_raw).resolve()
        payload = read_combat_root_artifact(
            artifact,
            max_bytes=max_artifact_bytes,
        )
        manifest, manifest_digest = _read_manifest(
            manifest_path,
            max_bytes=max_artifact_bytes,
        )
        search_anchor_manifest_ids.add(
            _sha256_text(
                manifest.get("behavior_manifest_id"),
                "search behavior_manifest_id",
            )
        )
        source_receipt = _mapping(manifest.get("source"), "source")
        root_count = _positive(source_receipt.get("root_count"), "root_count")
        digest = hashlib.sha256(payload).hexdigest()
        if source_receipt.get("artifact_sha256") != digest:
            raise CombatSearchDistillationError(
                "search manifest and opaque root artifact digest disagree"
            )
        search_config = _mapping(manifest.get("search"), "search")
        if search_config.get("potion_lane") != "never":
            raise CombatSearchDistillationError(
                "distillation only accepts no-potion search evidence"
            )
        source = load_combat_root_source(
            bridge,
            payload,
            expected_roots=root_count,
            max_bytes=max_artifact_bytes,
        )
        roots = _mapping_sequence(manifest, "roots")
        if len(roots) != root_count:
            raise CombatSearchDistillationError(
                "search manifest does not cover every declared natural root"
            )
        for expected_slot, root in enumerate(roots):
            slot = operator.index(root.get("root_slot"))
            if slot != expected_slot:
                raise CombatSearchDistillationError(
                    "search roots are not in exact artifact order"
                )
            group = source.combat_group(slot, 1, potion_slots=[])
            batch = group.decision_batch(semantic=True)
            counts = tuple(operator.index(value) for value in batch["candidate_counts"])
            if counts != (operator.index(root.get("candidate_count")),):
                raise CombatSearchDistillationError(
                    "search manifest and semantic candidate count disagree"
                )
            audit = _mapping(root.get("root"), "root")
            seed = _seed(audit.get("seed"), "root seed")
            root_id = str(group.root_id)
            record = {
                "batch": batch,
                "source_owner": source,
                "source_slot": slot,
                "root_id": root_id,
                "exact_combat_state_hash": str(group.exact_combat_state_hash),
                "seed": seed,
                "ascension_level": operator.index(audit.get("ascension_level")),
                "encounter_id": audit.get("encounter_id"),
                "baseline_ordinal": operator.index(root.get("baseline_ordinal")),
                "proposal_ordinal": (
                    None
                    if root.get("proposal_ordinal") is None
                    else operator.index(root.get("proposal_ordinal"))
                ),
                "actions": _mapping_sequence(
                    _mapping(root.get("search"), "root search"),
                    "actions",
                ),
            }
            existing_index = record_index_by_root_id.get(root_id)
            if existing_index is not None:
                if not _same_exact_search_record(records[existing_index], record):
                    raise CombatSearchDistillationError(
                        "one exact root has conflicting search evidence"
                    )
                duplicate_root_occurrence_count += 1
                continue
            record_index_by_root_id[root_id] = len(records)
            records.append(record)
            seeds.append(seed)
            root_ids.append(root_id)
        sources.append(
            {
                "artifact": str(artifact),
                "artifact_sha256": digest,
                "search_manifest": str(manifest_path),
                "search_manifest_sha256": manifest_digest,
                "root_count": root_count,
            }
        )
    if len(search_anchor_manifest_ids) != 1:
        raise CombatSearchDistillationError(
            "one partition mixes different search anchor manifests"
        )
    if require_unique_seeds and len(set(seeds)) != len(seeds):
        raise CombatSearchDistillationError("one partition repeats a run seed")
    proposal_records = tuple(
        record for record in records if record["proposal_ordinal"] is not None
    )
    return {
        "records": tuple(records),
        "proposal_records": proposal_records,
        "sources": tuple(sources),
        "seeds": tuple(seeds),
        "root_ids": tuple(root_ids),
        "search_anchor_manifest_id": next(iter(search_anchor_manifest_ids)),
        "duplicate_root_occurrence_count": duplicate_root_occurrence_count,
    }


def fit_combat_search_distillation_scorer(
    anchor: RaggedCandidateScorer,
    training: Mapping[str, object],
    limits: CombatWinSessionLimits,
    *,
    epochs: int,
    learning_rate: float,
    max_grad_norm: float,
    loss: str = COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
) -> tuple[
    RaggedCandidateScorer,
    AdamTrainingConfig,
    tuple[float, ...],
    tuple[float, ...],
    tuple[dict[str, float], ...],
]:
    """Apply one explicit bounded search-distillation objective."""

    epochs = _positive(epochs, "epochs")
    learning_rate = _positive_float(learning_rate, "learning_rate")
    max_grad_norm = _positive_float(max_grad_norm, "max_grad_norm")
    if loss not in {
        COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS,
        COMBAT_SEARCH_DISTILLATION_PROPOSAL_KL_LOSS,
    }:
        raise CombatSearchDistillationError(
            "unsupported combat search distillation loss"
        )
    if not isinstance(anchor, RaggedCandidateScorer):
        raise CombatSearchDistillationError(
            "distillation requires a maintained anchor scorer"
        )
    if anchor.training or any(
        parameter.requires_grad for parameter in anchor.parameters()
    ):
        raise CombatSearchDistillationError(
            "distillation anchor must be frozen"
        )
    scorer = copy.deepcopy(anchor)
    scorer.requires_grad_(True)
    scorer.train()
    optimizer_config = AdamTrainingConfig(learning_rate=learning_rate)
    optimizer = optimizer_config.create(scorer.parameters())
    proposal_batch, proposal_targets = _proposal_batch(training, limits)
    retained_batch = _retained_batch(training, limits)
    retained_anchor_logits: RaggedCandidateLogits | None = None
    if retained_batch is not None:
        with torch.inference_mode():
            retained_anchor_logits = anchor(retained_batch)
        retained_anchor_logits = RaggedCandidateLogits(
            values=retained_anchor_logits.values.detach().clone(),
            row_splits=retained_anchor_logits.row_splits.detach().clone(),
        )
    legacy_batch: Mapping[str, object] | None = None
    legacy_targets: tuple[int, ...] | None = None
    if loss == COMBAT_SEARCH_DISTILLATION_LEGACY_LOSS:
        legacy_batch, legacy_targets = combat_search_distillation_policy_batch(
            training,
            limits,
        )
    update_losses: list[float] = []
    gradient_norms: list[float] = []
    loss_components: list[dict[str, float]] = []
    for _ in range(epochs):
        optimizer.zero_grad(set_to_none=True)
        if legacy_batch is not None and legacy_targets is not None:
            policy_cross_entropy = ragged_cross_entropy(
                scorer(legacy_batch),
                legacy_targets,
            )
            proposal_cross_entropy = ragged_cross_entropy(
                scorer(proposal_batch),
                proposal_targets,
            )
            retained_forward_kl = policy_cross_entropy.new_zeros(())
            objective = policy_cross_entropy
        else:
            policy_cross_entropy = None
            proposal_cross_entropy = ragged_cross_entropy(
                scorer(proposal_batch),
                proposal_targets,
            )
            if retained_batch is None or retained_anchor_logits is None:
                retained_forward_kl = proposal_cross_entropy.new_zeros(())
            else:
                retained_forward_kl = _ragged_forward_kl(
                    retained_anchor_logits,
                    scorer(retained_batch),
                )
            objective = proposal_cross_entropy + retained_forward_kl
        if not bool(torch.isfinite(objective)):
            raise CombatSearchDistillationError(
                "distillation loss is not finite"
            )
        objective.backward()
        gradients = tuple(
            parameter.grad
            for parameter in scorer.parameters()
            if parameter.grad is not None
        )
        if not gradients or any(
            not bool(torch.all(torch.isfinite(gradient)))
            for gradient in gradients
        ):
            raise CombatSearchDistillationError(
                "distillation gradients are missing or non-finite"
            )
        norm = torch.nn.utils.clip_grad_norm_(
            tuple(scorer.parameters()),
            max_grad_norm,
        )
        if not bool(torch.isfinite(norm)):
            raise CombatSearchDistillationError(
                "distillation gradient norm is not finite"
            )
        optimizer.step()
        update_losses.append(float(objective.detach().cpu().item()))
        gradient_norms.append(float(norm.detach().cpu().item()))
        with torch.no_grad():
            proposal_after = ragged_cross_entropy(
                scorer(proposal_batch),
                proposal_targets,
            )
            if retained_batch is None or retained_anchor_logits is None:
                retained_after = proposal_after.new_zeros(())
            else:
                retained_after = _ragged_forward_kl(
                    retained_anchor_logits,
                    scorer(retained_batch),
                )
        components = {
            "proposal_cross_entropy_before_step": float(
                proposal_cross_entropy.detach().cpu().item()
            ),
            "proposal_cross_entropy_after_step": float(
                proposal_after.detach().cpu().item()
            ),
            "retained_forward_kl_before_step": float(
                retained_forward_kl.detach().cpu().item()
            ),
            "retained_forward_kl_after_step": float(
                retained_after.detach().cpu().item()
            ),
        }
        if policy_cross_entropy is not None:
            components["legacy_policy_cross_entropy_before_step"] = float(
                policy_cross_entropy.detach().cpu().item()
            )
        loss_components.append(components)
    scorer.eval()
    scorer.requires_grad_(False)
    return (
        scorer,
        optimizer_config,
        tuple(update_losses),
        tuple(gradient_norms),
        tuple(loss_components),
    )


def combat_search_distillation_partition_metrics(
    scorer: RaggedCandidateScorer,
    partition: Mapping[str, object],
    limits: CombatWinSessionLimits,
) -> dict[str, object]:
    records = tuple(partition["records"])
    all_batch = concatenate_semantic_decision_batches(
        [record["batch"] for record in records],
        limits.concat,
    )
    scorer.eval()
    with torch.inference_mode():
        predictions = tuple(scorer(all_batch).greedy_ordinals())
    proposal_batch, proposal_targets = _proposal_batch(partition, limits)
    with torch.inference_mode():
        proposal_logits = scorer(proposal_batch)
        proposal_loss = ragged_cross_entropy(proposal_logits, proposal_targets)

    improved = 0
    equal = 0
    regressed = 0
    unknown = 0
    hp_deltas: list[float] = []
    best_hp_regrets: list[float] = []
    rows: list[dict[str, object]] = []
    for record, predicted in zip(records, predictions, strict=True):
        actions = tuple(record["actions"])
        baseline = operator.index(record["baseline_ordinal"])
        chosen = operator.index(predicted)
        if not 0 <= chosen < len(actions):
            raise CombatSearchDistillationError(
                "scorer selected outside the searched candidate surface"
            )
        chosen_quality = _action_quality(actions[chosen])
        baseline_quality = _action_quality(actions[baseline])
        best_ordinal = max(
            range(len(actions)),
            key=lambda ordinal: (*_action_quality(actions[ordinal]), -ordinal),
        )
        best_quality = _action_quality(actions[best_ordinal])
        if chosen_quality is None or baseline_quality is None:
            status = "unknown"
            unknown += 1
        elif chosen_quality > baseline_quality:
            status = "improved"
            improved += 1
        elif chosen_quality == baseline_quality:
            status = "equal"
            equal += 1
        else:
            status = "regressed"
            regressed += 1
        hp_delta = _both_win_hp_delta(actions[chosen], actions[baseline])
        if hp_delta is not None:
            hp_deltas.append(float(hp_delta))
        if chosen_quality is not None and best_quality is not None:
            if chosen_quality[0] == best_quality[0] == 1:
                best_hp_regrets.append(float(best_quality[1] - chosen_quality[1]))
        rows.append(
            {
                "seed": record["seed"],
                "ascension_level": record["ascension_level"],
                "encounter_id": record["encounter_id"],
                "baseline_ordinal": baseline,
                "proposal_ordinal": record["proposal_ordinal"],
                "predicted_ordinal": chosen,
                "best_search_ordinal": best_ordinal,
                "status_vs_baseline": status,
                "both_win_hp_delta_vs_baseline": hp_delta,
            }
        )
    proposal_predictions = tuple(proposal_logits.greedy_ordinals())
    proposal_correct = sum(
        prediction == target
        for prediction, target in zip(
            proposal_predictions,
            proposal_targets,
            strict=True,
        )
    )
    return {
        "root_count": len(records),
        "proposal_count": len(proposal_targets),
        "proposal_correct": proposal_correct,
        "proposal_agreement": proposal_correct / len(proposal_targets),
        "proposal_cross_entropy": float(proposal_loss.detach().cpu().item()),
        "improved_root_count": improved,
        "equal_root_count": equal,
        "regressed_root_count": regressed,
        "unknown_root_count": unknown,
        "mean_both_win_hp_vs_baseline": (
            statistics.fmean(hp_deltas) if hp_deltas else None
        ),
        "mean_best_search_hp_regret": (
            statistics.fmean(best_hp_regrets) if best_hp_regrets else None
        ),
        "rows": tuple(rows),
    }


def _proposal_batch(
    partition: Mapping[str, object],
    limits: CombatWinSessionLimits,
) -> tuple[Mapping[str, object], tuple[int, ...]]:
    records = tuple(partition["proposal_records"])
    if not records:
        raise CombatSearchDistillationError("partition has no proposal rows")
    return (
        concatenate_semantic_decision_batches(
            [record["batch"] for record in records],
            limits.concat,
        ),
        tuple(operator.index(record["proposal_ordinal"]) for record in records),
    )


def _retained_batch(
    partition: Mapping[str, object],
    limits: CombatWinSessionLimits,
) -> Mapping[str, object] | None:
    records = tuple(
        record
        for record in partition["records"]
        if record["proposal_ordinal"] is None
    )
    if not records:
        return None
    return concatenate_semantic_decision_batches(
        [record["batch"] for record in records],
        limits.concat,
    )


def _ragged_forward_kl(
    teacher: RaggedCandidateLogits,
    student: RaggedCandidateLogits,
) -> torch.Tensor:
    if not torch.equal(teacher.row_splits, student.row_splits):
        raise CombatSearchDistillationError(
            "retained teacher and candidate rows are misaligned"
        )
    if not bool(torch.all(torch.isfinite(teacher.values))) or not bool(
        torch.all(torch.isfinite(student.values))
    ):
        raise CombatSearchDistillationError(
            "retained teacher and candidate logits must be finite"
        )
    row_losses: list[torch.Tensor] = []
    splits = teacher.row_splits.detach().cpu().tolist()
    for start, end in zip(splits[:-1], splits[1:], strict=True):
        teacher_log_probabilities = torch.log_softmax(
            teacher.values[start:end],
            dim=0,
        )
        student_log_probabilities = torch.log_softmax(
            student.values[start:end],
            dim=0,
        )
        teacher_probabilities = teacher_log_probabilities.exp()
        row_losses.append(
            torch.sum(
                teacher_probabilities
                * (teacher_log_probabilities - student_log_probabilities)
            )
        )
    if not row_losses:
        raise CombatSearchDistillationError(
            "retained KL requires at least one decision row"
        )
    return torch.stack(row_losses).mean()


def combat_search_distillation_policy_batch(
    partition: Mapping[str, object],
    limits: CombatWinSessionLimits,
) -> tuple[Mapping[str, object], tuple[int, ...]]:
    records = tuple(partition["records"])
    if not records:
        raise CombatSearchDistillationError("partition has no natural roots")
    return (
        concatenate_semantic_decision_batches(
            [record["batch"] for record in records],
            limits.concat,
        ),
        tuple(
            operator.index(
                record["baseline_ordinal"]
                if record["proposal_ordinal"] is None
                else record["proposal_ordinal"]
            )
            for record in records
        ),
    )


class _TracingGreedyPolicy:
    """Greedy scorer adapter retaining only exact selected ordinal calls."""

    def __init__(
        self,
        scorer: RaggedCandidateScorer,
        behavior_manifest_id: BehaviorManifestId,
    ) -> None:
        self.scorer = scorer
        self.behavior_manifest_id = behavior_manifest_id
        self.calls: list[tuple[int, ...]] = []

    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice:
        with torch.inference_mode():
            ordinals = tuple(self.scorer(decision_batch).greedy_ordinals())
        self.calls.append(ordinals)
        return BatchPolicyChoice.deterministic(
            ordinals,
            self.behavior_manifest_id,
        )


def _full_combat_metrics(
    scorer: RaggedCandidateScorer,
    partition: Mapping[str, object],
    behavior_manifest_id: BehaviorManifestId,
    limits: CombatExperienceLimits,
) -> dict[str, object]:
    scorer.eval()
    rows: list[dict[str, object]] = []
    replicate_count = 2
    for record in partition["records"]:
        policy = _TracingGreedyPolicy(scorer, behavior_manifest_id)
        source = record["source_owner"]
        env = source.combat_group(
            operator.index(record["source_slot"]),
            replicate_count,
            potion_slots=[],
        )
        run = CombatGroupDriver(env, policy, limits).run()
        outcomes = tuple(run.experience.outcomes.outcomes)
        wins = sum(outcome.won for outcome in outcomes)
        final_hps = tuple(outcome.final_hp for outcome in outcomes)
        winning_hps = tuple(
            outcome.final_hp for outcome in outcomes if outcome.won
        )
        trace_payload = json.dumps(
            policy.calls,
            separators=(",", ":"),
        ).encode("utf-8")
        rows.append(
            {
                "root_id": record["root_id"],
                "seed": record["seed"],
                "ascension_level": record["ascension_level"],
                "encounter_id": record["encounter_id"],
                "win_count": wins,
                "loss_count": replicate_count - wins,
                "final_hp_sum": sum(final_hps),
                "mean_final_hp": statistics.fmean(final_hps),
                "mean_winning_final_hp": (
                    statistics.fmean(winning_hps) if winning_hps else None
                ),
                "outcomes": tuple(
                    {
                        "won": bool(outcome.won),
                        "final_hp": outcome.final_hp,
                    }
                    for outcome in outcomes
                ),
                "decision_count": run.experience.decision_count,
                "model_rounds": run.model_rounds,
                "transitions": run.transitions,
                "greedy_action_call_count": len(policy.calls),
                "greedy_action_ordinal_count": sum(
                    len(call) for call in policy.calls
                ),
                "greedy_action_trace_sha256": hashlib.sha256(
                    trace_payload
                ).hexdigest(),
            }
        )
    result = _summarize_full_combat_rows(rows, replicate_count)
    result["by_ascension"] = {
        str(ascension): _summarize_full_combat_rows(
            [row for row in rows if row["ascension_level"] == ascension],
            replicate_count,
        )
        for ascension in sorted({row["ascension_level"] for row in rows})
    }
    result["rows"] = tuple(rows)
    return result


def _summarize_full_combat_rows(
    rows: Sequence[Mapping[str, object]],
    replicate_count: int,
) -> dict[str, object]:
    wins = sum(operator.index(row["win_count"]) for row in rows)
    final_hps = [float(row["mean_final_hp"]) for row in rows]
    winning_hps = [
        float(row["mean_winning_final_hp"])
        for row in rows
        if row["mean_winning_final_hp"] is not None
    ]
    return {
        "root_count": len(rows),
        "replicates_per_root": replicate_count,
        "win_count": wins,
        "loss_count": len(rows) * replicate_count - wins,
        "win_rate": wins / (len(rows) * replicate_count),
        "all_win_root_count": sum(
            operator.index(row["win_count"]) == replicate_count for row in rows
        ),
        "mean_final_hp": statistics.fmean(final_hps),
        "mean_winning_final_hp": (
            statistics.fmean(winning_hps) if winning_hps else None
        ),
    }


def _compare_full_combat_metrics(
    baseline: Mapping[str, object],
    candidate: Mapping[str, object],
) -> dict[str, object]:
    baseline_rows = {
        str(row["root_id"]): row for row in baseline["rows"]
    }
    candidate_rows = {
        str(row["root_id"]): row for row in candidate["rows"]
    }
    if baseline_rows.keys() != candidate_rows.keys():
        raise CombatSearchDistillationError(
            "full-combat evaluations do not cover the same exact roots"
        )
    improved = 0
    equal = 0
    regressed = 0
    both_win_hp_deltas: list[float] = []
    rows: list[dict[str, object]] = []
    replicate_count = operator.index(baseline["replicates_per_root"])
    for root_id, baseline_row in baseline_rows.items():
        candidate_row = candidate_rows[root_id]
        baseline_quality = (
            operator.index(baseline_row["win_count"]),
            operator.index(baseline_row["final_hp_sum"]),
        )
        candidate_quality = (
            operator.index(candidate_row["win_count"]),
            operator.index(candidate_row["final_hp_sum"]),
        )
        if candidate_quality > baseline_quality:
            status = "improved"
            improved += 1
        elif candidate_quality == baseline_quality:
            status = "equal"
            equal += 1
        else:
            status = "regressed"
            regressed += 1
        hp_delta = None
        if baseline_quality[0] == candidate_quality[0] == replicate_count:
            hp_delta = (
                candidate_quality[1] - baseline_quality[1]
            ) / replicate_count
            both_win_hp_deltas.append(float(hp_delta))
        rows.append(
            {
                "root_id": root_id,
                "seed": baseline_row["seed"],
                "ascension_level": baseline_row["ascension_level"],
                "encounter_id": baseline_row["encounter_id"],
                "baseline_win_count": baseline_quality[0],
                "candidate_win_count": candidate_quality[0],
                "both_win_final_hp_delta": hp_delta,
                "status": status,
            }
        )
    return {
        "root_count": len(rows),
        "improved_root_count": improved,
        "equal_root_count": equal,
        "regressed_root_count": regressed,
        "win_count_delta": (
            operator.index(candidate["win_count"])
            - operator.index(baseline["win_count"])
        ),
        "mean_both_win_final_hp_delta": (
            statistics.fmean(both_win_hp_deltas)
            if both_win_hp_deltas
            else None
        ),
        "rows": tuple(rows),
    }


def _candidate_reload_parity(
    original: RaggedCandidateScorer,
    reloaded: RaggedCandidateScorer,
    training: Mapping[str, object],
    held_out: Mapping[str, object],
    manifest_id: BehaviorManifestId,
    limits: CombatWinSessionLimits,
    rollout_limits: CombatExperienceLimits,
    original_full_combat: Mapping[str, object],
) -> dict[str, object]:
    logit_partitions = {
        "training": combat_search_distillation_logit_parity(
            original,
            reloaded,
            training,
            limits,
        ),
        "held_out": combat_search_distillation_logit_parity(
            original,
            reloaded,
            held_out,
            limits,
        ),
    }
    reloaded_full_combat = _full_combat_metrics(
        reloaded,
        held_out,
        manifest_id,
        rollout_limits,
    )
    original_rows = tuple(original_full_combat["rows"])
    reloaded_rows = tuple(reloaded_full_combat["rows"])
    action_keys = (
        "root_id",
        "greedy_action_call_count",
        "greedy_action_ordinal_count",
        "greedy_action_trace_sha256",
    )
    outcome_keys = (
        "root_id",
        "outcomes",
        "decision_count",
        "model_rounds",
        "transitions",
    )
    action_projection = tuple(
        tuple(row[key] for key in action_keys) for row in original_rows
    )
    reloaded_action_projection = tuple(
        tuple(row[key] for key in action_keys) for row in reloaded_rows
    )
    outcome_projection = tuple(
        tuple(row[key] for key in outcome_keys) for row in original_rows
    )
    reloaded_outcome_projection = tuple(
        tuple(row[key] for key in outcome_keys) for row in reloaded_rows
    )
    logits_equal = all(
        partition["logits_equal"] and partition["greedy_ordinals_equal"]
        for partition in logit_partitions.values()
    )
    greedy_actions_equal = action_projection == reloaded_action_projection
    full_combat_outcomes_equal = outcome_projection == reloaded_outcome_projection
    if not logits_equal:
        raise CombatSearchDistillationError(
            "reloaded candidate changed logits or entry greedy actions"
        )
    if not greedy_actions_equal:
        raise CombatSearchDistillationError(
            "reloaded candidate changed complete-combat greedy actions"
        )
    if not full_combat_outcomes_equal:
        raise CombatSearchDistillationError(
            "reloaded candidate changed complete-combat outcomes"
        )
    return {
        "verified": True,
        "logits": logit_partitions,
        "greedy_actions_equal": True,
        "full_combat_outcomes_equal": True,
        "full_combat_root_count": len(original_rows),
    }


def combat_search_distillation_logit_parity(
    original: RaggedCandidateScorer,
    reloaded: RaggedCandidateScorer,
    partition: Mapping[str, object],
    limits: CombatWinSessionLimits,
) -> dict[str, object]:
    records = tuple(partition["records"])
    batch = concatenate_semantic_decision_batches(
        [record["batch"] for record in records],
        limits.concat,
    )
    with torch.inference_mode():
        original_logits = original(batch)
        reloaded_logits = reloaded(batch)
    row_splits_equal = torch.equal(
        original_logits.row_splits,
        reloaded_logits.row_splits,
    )
    values_equal = torch.equal(original_logits.values, reloaded_logits.values)
    original_ordinals = tuple(original_logits.greedy_ordinals())
    reloaded_ordinals = tuple(reloaded_logits.greedy_ordinals())
    return {
        "row_count": len(records),
        "candidate_count": original_logits.values.numel(),
        "logits_equal": row_splits_equal and values_equal,
        "greedy_ordinals_equal": original_ordinals == reloaded_ordinals,
        "original_logits_sha256": _tensor_sha256(original_logits.values),
        "reloaded_logits_sha256": _tensor_sha256(reloaded_logits.values),
    }


def _tensor_sha256(value: torch.Tensor) -> str:
    tensor = value.detach().cpu().contiguous()
    header = (
        f"{tensor.dtype}|{tuple(tensor.shape)}|".encode("ascii")
    )
    return hashlib.sha256(header + tensor.numpy().tobytes()).hexdigest()


def combat_search_distillation_corpus_sha256(
    partition: Mapping[str, object],
) -> str:
    payload = {
        "search_anchor_manifest_id": partition["search_anchor_manifest_id"],
        "duplicate_root_occurrence_count": partition[
            "duplicate_root_occurrence_count"
        ],
        "sources": tuple(
            {
                "artifact_sha256": source["artifact_sha256"],
                "search_manifest_sha256": source["search_manifest_sha256"],
                "root_count": source["root_count"],
            }
            for source in partition["sources"]
        ),
        "roots": tuple(
            {
                "root_id": record["root_id"],
                "baseline_ordinal": record["baseline_ordinal"],
                "proposal_ordinal": record["proposal_ordinal"],
            }
            for record in partition["records"]
        ),
    }
    encoded = json.dumps(
        payload,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _action_quality(action: Mapping[str, object]) -> tuple[int, int] | None:
    if operator.index(action.get("budget_unknown_count")):
        return None
    return (
        operator.index(action.get("exact_win_count")),
        operator.index(action.get("winning_final_hp_sum")),
    )


def _both_win_hp_delta(
    chosen: Mapping[str, object],
    baseline: Mapping[str, object],
) -> int | None:
    if (
        operator.index(chosen.get("exact_win_count")) != 1
        or operator.index(baseline.get("exact_win_count")) != 1
    ):
        return None
    return operator.index(chosen.get("winning_final_hp_sum")) - operator.index(
        baseline.get("winning_final_hp_sum")
    )


def combat_search_distillation_partition_receipt(
    partition: Mapping[str, object],
) -> dict[str, object]:
    records = tuple(partition["records"])
    return {
        "root_count": len(records),
        "unique_run_seed_count": len(set(partition["seeds"])),
        "proposal_count": len(partition["proposal_records"]),
        "search_anchor_manifest_id": partition["search_anchor_manifest_id"],
        "duplicate_root_occurrence_count": partition[
            "duplicate_root_occurrence_count"
        ],
        "seeds": partition["seeds"],
        "ascension_counts": {
            str(ascension): sum(
                record["ascension_level"] == ascension for record in records
            )
            for ascension in sorted({record["ascension_level"] for record in records})
        },
        "sources": partition["sources"],
    }


def _without_rows(value: Mapping[str, object]) -> dict[str, object]:
    return {key: item for key, item in value.items() if key != "rows"}


def _read_manifest(
    path: Path,
    *,
    max_bytes: int,
) -> tuple[Mapping[str, object], str]:
    if not path.is_file():
        raise CombatSearchDistillationError("search manifest does not exist")
    try:
        content = path.read_bytes()
        if len(content) > max_bytes:
            raise CombatSearchDistillationError(
                "search manifest exceeds its byte limit"
            )
        payload = json.loads(content)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise CombatSearchDistillationError("cannot read search manifest") from error
    if not isinstance(payload, Mapping):
        raise CombatSearchDistillationError("search manifest must be an object")
    if (
        payload.get("schema") != "sts-learning-natural-combat-search-census-v1"
        or payload.get("teacher_valid") is not False
    ):
        raise CombatSearchDistillationError("unsupported natural search manifest")
    return payload, hashlib.sha256(content).hexdigest()


def _same_exact_search_record(
    left: Mapping[str, object],
    right: Mapping[str, object],
) -> bool:
    for key in (
        "root_id",
        "exact_combat_state_hash",
        "seed",
        "ascension_level",
        "encounter_id",
        "baseline_ordinal",
        "proposal_ordinal",
        "actions",
    ):
        if left[key] != right[key]:
            return False
    return True


def _sha256_text(value: object, name: str) -> str:
    if not isinstance(value, str) or len(value) != 64:
        raise CombatSearchDistillationError(f"{name} must be a sha256 hex digest")
    try:
        bytes.fromhex(value)
    except ValueError as error:
        raise CombatSearchDistillationError(
            f"{name} must be a sha256 hex digest"
        ) from error
    return value.lower()


def _mapping(value: object, name: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise CombatSearchDistillationError(f"{name} must be an object")
    return value


def _mapping_sequence(
    source: Mapping[str, object],
    key: str,
) -> tuple[Mapping[str, object], ...]:
    value = source.get(key)
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)):
        raise CombatSearchDistillationError(f"{key} must be a sequence")
    return tuple(_mapping(item, key) for item in value)


def _optional_delta(final: object, initial: object) -> float | None:
    if final is None or initial is None:
        return None
    return float(final) - float(initial)


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatSearchDistillationError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatSearchDistillationError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise CombatSearchDistillationError(f"{name} must be positive")
    return normalized


def _seed(value: object, name: str) -> int:
    normalized = _positive(value, name)
    if normalized >= 1 << 64:
        raise CombatSearchDistillationError(f"{name} must be below 2^64")
    return normalized


def _positive_float(value: object, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise CombatSearchDistillationError(f"{name} must be numeric")
    normalized = float(value)
    if not math.isfinite(normalized) or normalized <= 0.0:
        raise CombatSearchDistillationError(f"{name} must be finite and positive")
    return normalized


def _pair_arguments(
    artifacts: Sequence[Path],
    manifests: Sequence[Path],
    name: str,
) -> tuple[tuple[Path, Path], ...]:
    if len(artifacts) != len(manifests) or not artifacts:
        raise CombatSearchDistillationError(
            f"{name} artifacts and search manifests must be non-empty and aligned"
        )
    return tuple(zip(artifacts, manifests, strict=True))


def _parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Fit strict search-trajectory proposals and evaluate complete held-out "
            "combats, optionally retaining an unqualified candidate"
        )
    )
    parser.add_argument("--training-artifact", type=Path, action="append", required=True)
    parser.add_argument("--training-search", type=Path, action="append", required=True)
    parser.add_argument("--held-out-artifact", type=Path, action="append", required=True)
    parser.add_argument("--held-out-search", type=Path, action="append", required=True)
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--candidate-output", type=Path)
    parser.add_argument("--epochs", type=int, default=1)
    parser.add_argument("--learning-rate", type=float, default=3e-4)
    parser.add_argument("--max-grad-norm", type=float, default=1.0)
    parser.add_argument("--max-artifact-bytes", type=int, default=16 * 1024 * 1024)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = _parse_args(argv)
    run_combat_search_distillation_spike(
        training_pairs=_pair_arguments(
            args.training_artifact,
            args.training_search,
            "training",
        ),
        held_out_pairs=_pair_arguments(
            args.held_out_artifact,
            args.held_out_search,
            "held-out",
        ),
        behavior=args.behavior,
        output=args.output,
        candidate_output=args.candidate_output,
        epochs=args.epochs,
        learning_rate=args.learning_rate,
        max_grad_norm=args.max_grad_norm,
        max_artifact_bytes=args.max_artifact_bytes,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
