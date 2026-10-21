"""Seed-isolated supervised initialization from production strategic labels."""

from __future__ import annotations

import copy
import argparse
import hashlib
import json
import math
import operator
import time
from collections import Counter
from collections.abc import Mapping
from dataclasses import asdict, dataclass
from pathlib import Path

import numpy as np
import torch

from .seeds import SeedPartition, SeedPartitionSpec
from .semantic_batch import select_semantic_decision_rows
from .strategic_model_scope import DECISION_CHOICE_TARGET, StrategicModelApplicability
from .semantic_concat import (
    SemanticBatchConcatLimits,
    concatenate_semantic_decision_batches,
)
from .strategic_demonstrations import StrategicDemonstrationCorpus
from .strategic_demonstrations import load_scoped_strategic_samples
from .strategic_demonstrations import (
    ScopedStrategicDecisionRow,
    ScopedStrategicDecisions,
    ScopedStrategicPreferenceRow,
    ScopedStrategicPreferences,
    load_scoped_strategic_decisions,
    load_scoped_strategic_preferences,
)
from .torch_policy import (
    RaggedCandidateScorer,
    RaggedScorerConfig,
    configure_policy_heads_only_training,
    configure_target_only_training,
    ragged_acceptable_family_cross_entropy,
    ragged_acceptable_set_cross_entropy,
    ragged_cross_entropy,
    ragged_resolved_pairwise_loss,
)


class StrategicBehaviorCloningError(RuntimeError):
    """A demonstration split or supervised update is not exact and bounded."""


@dataclass(frozen=True)
class StrategicDemonstrationPartition:
    decision_batch: Mapping[str, object]
    target_ordinals: tuple[int, ...]
    episode_seeds: tuple[int, ...]
    context_kinds: tuple[int, ...]

    def __post_init__(self) -> None:
        row_count = len(self.target_ordinals)
        if row_count == 0:
            raise StrategicBehaviorCloningError(
                "demonstration partition must contain at least one row"
            )
        if not (
            len(self.episode_seeds) == row_count
            and len(self.context_kinds) == row_count
        ):
            raise StrategicBehaviorCloningError(
                "demonstration partition metadata is misaligned"
            )
        candidate_counts = _integer_vector(
            self.decision_batch,
            "candidate_counts",
        )
        if candidate_counts.size != row_count:
            raise StrategicBehaviorCloningError(
                "demonstration partition decision rows are misaligned"
            )
        if any(
            target < 0 or target >= int(candidate_counts[index])
            for index, target in enumerate(self.target_ordinals)
        ):
            raise StrategicBehaviorCloningError(
                "demonstration target is outside its candidate row"
            )

    @property
    def row_count(self) -> int:
        return len(self.target_ordinals)

    @property
    def seed_count(self) -> int:
        return len(set(self.episode_seeds))

    @property
    def context_counts(self) -> dict[int, int]:
        return dict(sorted(Counter(self.context_kinds).items()))


@dataclass(frozen=True)
class StrategicDemonstrationSplit:
    training: StrategicDemonstrationPartition
    held_out: StrategicDemonstrationPartition
    partition_spec: SeedPartitionSpec

    def __post_init__(self) -> None:
        if not isinstance(self.partition_spec, SeedPartitionSpec):
            raise StrategicBehaviorCloningError("partition_spec must be typed")
        overlap = set(self.training.episode_seeds) & set(self.held_out.episode_seeds)
        if overlap:
            raise StrategicBehaviorCloningError(
                "training and held-out demonstrations share a run seed"
            )


@dataclass(frozen=True)
class StrategicBehaviorCloningConfig:
    epochs: int = 16
    learning_rate: float = 3e-4
    max_grad_norm: float = 1.0
    device_type: str = "cpu"
    batch_rows: int | None = None
    shuffle_seed: int = 0

    def __post_init__(self) -> None:
        object.__setattr__(self, "epochs", _positive(self.epochs, "epochs"))
        if self.batch_rows is not None:
            object.__setattr__(self, "batch_rows", _positive(self.batch_rows, "batch_rows"))
        if type(self.shuffle_seed) is not int or not 0 <= self.shuffle_seed < 2**63:
            raise StrategicBehaviorCloningError("shuffle_seed must be an integer in 0..2^63")
        for name in ("learning_rate", "max_grad_norm"):
            value = getattr(self, name)
            if isinstance(value, bool) or not isinstance(value, (int, float)):
                raise StrategicBehaviorCloningError(f"{name} must be numeric")
            normalized = float(value)
            if not math.isfinite(normalized) or normalized <= 0.0:
                raise StrategicBehaviorCloningError(
                    f"{name} must be finite and positive"
                )
            object.__setattr__(self, name, normalized)
        if self.device_type not in {"cpu", "cuda"}:
            raise StrategicBehaviorCloningError(
                "device_type must be either cpu or cuda"
            )
        if self.device_type == "cuda" and not torch.cuda.is_available():
            raise StrategicBehaviorCloningError("CUDA was requested but is unavailable")


@dataclass(frozen=True)
class StrategicContextAgreement:
    context_kind: int
    rows: int
    correct: int

    @property
    def agreement(self) -> float:
        return self.correct / self.rows


@dataclass(frozen=True)
class StrategicAgreement:
    rows: int
    correct: int
    cross_entropy: float
    contexts: tuple[StrategicContextAgreement, ...]

    @property
    def agreement(self) -> float:
        return self.correct / self.rows


@dataclass(frozen=True)
class StrategicBehaviorCloneResult:
    scorer: RaggedCandidateScorer
    initial_training: StrategicAgreement
    initial_held_out: StrategicAgreement
    final_training: StrategicAgreement
    final_held_out: StrategicAgreement
    epochs: int
    elapsed_seconds: float
    optimizer_steps: int


def split_strategic_demonstrations(
    corpus: StrategicDemonstrationCorpus,
    partition_spec: SeedPartitionSpec,
    concat_limits: SemanticBatchConcatLimits,
) -> StrategicDemonstrationSplit:
    """Split by original run seed before concatenating any semantic rows."""

    if not isinstance(corpus, StrategicDemonstrationCorpus):
        raise StrategicBehaviorCloningError("demonstration corpus must be typed")
    if not isinstance(partition_spec, SeedPartitionSpec):
        raise StrategicBehaviorCloningError("partition_spec must be typed")
    if not isinstance(concat_limits, SemanticBatchConcatLimits):
        raise StrategicBehaviorCloningError("concat_limits must be typed")

    batches: dict[SeedPartition, list[Mapping[str, object]]] = {
        SeedPartition.TRAINING: [],
        SeedPartition.HELD_OUT: [],
    }
    targets: dict[SeedPartition, list[int]] = {
        SeedPartition.TRAINING: [],
        SeedPartition.HELD_OUT: [],
    }
    seeds: dict[SeedPartition, list[int]] = {
        SeedPartition.TRAINING: [],
        SeedPartition.HELD_OUT: [],
    }
    contexts: dict[SeedPartition, list[int]] = {
        SeedPartition.TRAINING: [],
        SeedPartition.HELD_OUT: [],
    }
    for batch in corpus.batches:
        for partition in SeedPartition:
            rows = tuple(
                index
                for index, seed in enumerate(batch.episode_seeds)
                if partition_spec.classify(seed) is partition
            )
            if not rows:
                continue
            batches[partition].append(
                select_semantic_decision_rows(batch.decision_batch, rows)
            )
            targets[partition].extend(batch.target_ordinals[row] for row in rows)
            seeds[partition].extend(batch.episode_seeds[row] for row in rows)
            contexts[partition].extend(batch.context_kinds[row] for row in rows)

    partitioned_rows = sum(len(values) for values in targets.values())
    if partitioned_rows != corpus.teacher_rows:
        raise StrategicBehaviorCloningError(
            "demonstration corpus teacher-row accounting is inconsistent"
        )

    partitions: dict[SeedPartition, StrategicDemonstrationPartition] = {}
    for partition in SeedPartition:
        if not batches[partition]:
            raise StrategicBehaviorCloningError(
                f"demonstration corpus has no {partition.value} teacher rows"
            )
        partitions[partition] = StrategicDemonstrationPartition(
            decision_batch=concatenate_semantic_decision_batches(
                batches[partition],
                concat_limits,
            ),
            target_ordinals=tuple(targets[partition]),
            episode_seeds=tuple(seeds[partition]),
            context_kinds=tuple(contexts[partition]),
        )
    return StrategicDemonstrationSplit(
        training=partitions[SeedPartition.TRAINING],
        held_out=partitions[SeedPartition.HELD_OUT],
        partition_spec=partition_spec,
    )


def train_strategic_behavior_clone(
    combat_anchor: RaggedCandidateScorer,
    demonstrations: StrategicDemonstrationSplit,
    config: StrategicBehaviorCloningConfig,
) -> StrategicBehaviorCloneResult:
    """Train a copied scorer; the caller's combat anchor remains immutable."""

    if not isinstance(combat_anchor, RaggedCandidateScorer):
        raise StrategicBehaviorCloningError("combat_anchor must be a scorer")
    if combat_anchor.training or any(
        parameter.requires_grad for parameter in combat_anchor.parameters()
    ):
        raise StrategicBehaviorCloningError("combat_anchor must be frozen")
    if not isinstance(demonstrations, StrategicDemonstrationSplit):
        raise StrategicBehaviorCloningError("demonstrations must be a typed split")
    if not isinstance(config, StrategicBehaviorCloningConfig):
        raise StrategicBehaviorCloningError("config must be typed")

    started = time.perf_counter()
    initial_training = _agreement(combat_anchor, demonstrations.training, config.batch_rows)
    initial_held_out = _agreement(combat_anchor, demonstrations.held_out, config.batch_rows)
    generator = torch.Generator().manual_seed(config.shuffle_seed)
    def training_batches(_epoch):
        order = torch.randperm(demonstrations.training.row_count, generator=generator).tolist()
        for start in range(0, len(order), config.batch_rows):
            rows = order[start:start+config.batch_rows]
            targets = tuple(demonstrations.training.target_ordinals[row] for row in rows)
            yield select_semantic_decision_rows(demonstrations.training.decision_batch, rows), (
                lambda logits, targets=targets: ragged_cross_entropy(logits, targets))
    scorer = _fit_supervised_scorer(
        combat_anchor, demonstrations.training.decision_batch,
        lambda logits: ragged_cross_entropy(logits, demonstrations.training.target_ordinals),
        config,
        training_batches=training_batches if config.batch_rows is not None else None,
    )
    return StrategicBehaviorCloneResult(
        scorer=scorer,
        initial_training=initial_training,
        initial_held_out=initial_held_out,
        final_training=_agreement(scorer, demonstrations.training, config.batch_rows),
        final_held_out=_agreement(scorer, demonstrations.held_out, config.batch_rows),
        epochs=config.epochs,
        elapsed_seconds=time.perf_counter() - started,
        optimizer_steps=config.epochs * (math.ceil(demonstrations.training.row_count/config.batch_rows)
                                         if config.batch_rows is not None else 1),
    )


def _fit_supervised_scorer(initial, decision_batch, loss_function, config, *, after_update=None,
                           target_only=False, heads_only=False, training_batches=None):
    if type(target_only) is not bool or type(heads_only) is not bool:
        raise StrategicBehaviorCloningError("training scope flags must be bool")
    if target_only and heads_only:
        raise StrategicBehaviorCloningError("training scopes are mutually exclusive")
    scorer = copy.deepcopy(initial).to(config.device_type)
    scorer.requires_grad_(True)
    if target_only:
        configure_target_only_training(scorer)
    elif heads_only:
        configure_policy_heads_only_training(scorer)
    parameters = tuple(p for p in scorer.parameters() if p.requires_grad)
    optimizer = torch.optim.Adam(parameters, lr=config.learning_rate)
    for update in range(1, config.epochs + 1):
        batches = ((decision_batch, loss_function),) if training_batches is None else training_batches(update)
        for inputs, batch_loss in batches:
            scorer.train()
            optimizer.zero_grad(set_to_none=True)
            loss = batch_loss(scorer(inputs))
            if not bool(torch.isfinite(loss)):
                raise StrategicBehaviorCloningError("behavior cloning loss is not finite")
            loss.backward()
            gradients = tuple(
                parameter.grad for parameter in scorer.parameters() if parameter.grad is not None
            )
            if not gradients or any(not bool(torch.all(torch.isfinite(gradient))) for gradient in gradients):
                raise StrategicBehaviorCloningError("behavior cloning gradients are missing or non-finite")
            norm = torch.nn.utils.clip_grad_norm_(parameters, config.max_grad_norm)
            if not bool(torch.isfinite(norm)):
                raise StrategicBehaviorCloningError("behavior cloning gradient norm is not finite")
            optimizer.step()
        if after_update is not None and after_update(scorer, update):
            break

    scorer.eval()
    scorer.requires_grad_(False)
    return scorer


@dataclass(frozen=True)
class StrategicDecisionFitConfig:
    max_updates: int = 256
    learning_rate: float = 0.003
    check_every: int = 16
    fit_margin: float = 0.5
    max_grad_norm: float = 1.0
    device_type: str = "cpu"

    def __post_init__(self):
        optimizer = StrategicBehaviorCloningConfig(
            epochs=self.max_updates, learning_rate=self.learning_rate,
            max_grad_norm=self.max_grad_norm, device_type=self.device_type)
        object.__setattr__(self, "max_updates", optimizer.epochs)
        object.__setattr__(self, "learning_rate", optimizer.learning_rate)
        object.__setattr__(self, "max_grad_norm", optimizer.max_grad_norm)
        object.__setattr__(self, "check_every", _positive(self.check_every, "check_every"))
        if (isinstance(self.fit_margin, bool) or not isinstance(self.fit_margin, (int, float))
                or not math.isfinite(self.fit_margin) or self.fit_margin < 0):
            raise StrategicBehaviorCloningError("fit_margin must be finite and nonnegative")
        object.__setattr__(self, "fit_margin", float(self.fit_margin))


@dataclass(frozen=True)
class StrategicDecisionPartition:
    decision_batch: Mapping[str, object]
    rows: tuple[ScopedStrategicDecisionRow, ...]

    @property
    def acceptable_ordinals(self):
        return tuple(row.acceptable_ordinals for row in self.rows)

    @property
    def choice_target_ordinals(self):
        return tuple(row.choice_target_ordinals for row in self.rows)


def split_scoped_strategic_decisions(corpus, partition_spec, concat_limits):
    if not isinstance(corpus, ScopedStrategicDecisions):
        raise StrategicBehaviorCloningError("decisions must be a typed corpus")
    if not isinstance(partition_spec, SeedPartitionSpec):
        raise StrategicBehaviorCloningError("partition_spec must be typed")
    partitions = {}
    for partition in SeedPartition:
        rows = tuple(row for row in corpus.rows
                     if row.eligible_for_supervision and partition_spec.classify(row.episode_seed) is partition)
        if not rows:
            continue
        partitions[partition] = StrategicDecisionPartition(
            concatenate_semantic_decision_batches([row.decision_batch for row in rows], concat_limits), rows)
    return partitions


def _decision_metrics(scorer, partition):
    scorer.eval()
    targets = partition.acceptable_ordinals
    with torch.inference_mode():
        logits = scorer(partition.decision_batch)
        loss = (float(ragged_acceptable_set_cross_entropy(logits, targets).cpu().item())
                if any(target is not None for target in targets) else None)
        recommendation_loss = (float(ragged_acceptable_set_cross_entropy(
            logits, partition.choice_target_ordinals).cpu().item()) if loss is not None else None)
        predicted = logits.greedy_ordinals()
        values = logits.values.cpu().tolist()
        splits = logits.row_splits.cpu().tolist()
    families = None if logits.families is None else logits.families.candidate_families.cpu().tolist()
    return _decision_choice_metrics(partition, predicted, values, splits, loss=loss,
                                    recommendation_loss=recommendation_loss, families=families)


def _decision_choice_metrics(partition, predicted, values, splits, *, loss=None,
                             recommendation_loss=None, families=None):
    """Backend-independent whole-choice agreement; Unknown stays unsupervised."""
    choices = Counter({"adoptable": 0, "outside_set": 0, "unknown": 0})
    family_choices = Counter({"adoptable": 0, "outside_set": 0, "unknown": 0})
    target_choices = Counter({"adoptable": 0, "outside_set": 0, "unknown": 0})
    recommendations = Counter({"recommended": 0, "secondary_acceptable": 0, "outside_set": 0, "unknown": 0})
    explicit_preferred_choices = Counter({"recommended": 0, "secondary_acceptable": 0, "outside_set": 0})
    target_basis_counts = Counter({"declared_preferred": 0, "acceptable_fallback": 0, "unknown": 0})
    signal = Counter({"unknown": 0, "informative": 0, "all_candidates_allowed": 0})
    family_signal = Counter({"unknown": 0, "informative": 0, "all_families_allowed": 0})
    recommendation_errors = Counter({"wrong_family": 0, "wrong_target_within_allowed_family": 0})
    recommendation_margins = []
    explicit_preferred_margins = []
    contexts, predictions, margins = {}, [], []
    for index, (row, ordinal) in enumerate(zip(partition.rows, predicted, strict=True)):
        acceptable = row.acceptable_ordinals
        status = ("unknown" if acceptable is None else
                  "adoptable" if ordinal in acceptable else "outside_set")
        choices[status] += 1
        target = row.choice_target_ordinals
        signal["unknown" if target is None else "all_candidates_allowed"
               if len(target) == splits[index + 1] - splits[index] else "informative"] += 1
        target_basis = ("unknown" if target is None else "declared_preferred"
                        if row.preferred_ordinals is not None else "acceptable_fallback")
        target_basis_counts[target_basis] += 1
        recommendation_status = ("unknown" if target is None else "recommended" if ordinal in target
                                 else "secondary_acceptable" if ordinal in acceptable else "outside_set")
        recommendations[recommendation_status] += 1
        if row.preferred_ordinals is not None:
            explicit_preferred_choices[recommendation_status] += 1
        context = contexts.setdefault(row.context_kind,
                                      {"rows": 0, "adoptable": 0, "outside_set": 0, "unknown": 0})
        context["rows"] += 1
        context[status] += 1
        margin = None
        recommendation_margin = None
        if acceptable is not None:
            scores = values[splits[index]:splits[index + 1]]
            other_scores = [score for i, score in enumerate(scores) if i not in acceptable]
            if other_scores:
                margin = max(scores[i] for i in acceptable) - max(other_scores)
                margins.append(margin)
            alternatives = [score for i, score in enumerate(scores) if i not in target]
            if alternatives:
                recommendation_margin = max(scores[i] for i in target) - max(alternatives)
                recommendation_margins.append(recommendation_margin)
                if row.preferred_ordinals is not None:
                    explicit_preferred_margins.append(recommendation_margin)
        predictions.append({"source_index": row.source_index, "row_index": row.row_index,
                            "seed": row.episode_seed, "context_kind": row.context_kind,
                            "ordinal": ordinal, "status": status, "margin": margin,
                            "target_basis": target_basis,
                            "recommendation_status": recommendation_status,
                            "recommendation_margin": recommendation_margin})
        if families is not None:
            membership = families[splits[index]:splits[index + 1]]
            target_families = None if target is None else {membership[i] for i in target}
            family_signal["unknown" if target is None else "all_families_allowed"
                          if target_families == set(membership) else "informative"] += 1
            if target is not None and ordinal not in target:
                recommendation_errors["wrong_family" if membership[ordinal] not in target_families
                                      else "wrong_target_within_allowed_family"] += 1
            family_status = ("unknown" if acceptable is None else
                             "adoptable" if membership[ordinal] in {membership[i] for i in acceptable}
                             else "outside_set")
            target_status = status if family_status == "adoptable" else "unknown"
            family_choices[family_status] += 1
            target_choices[target_status] += 1
            predictions[-1].update(family_status=family_status, target_status=target_status)
    result = {"rows": len(partition.rows), "supervised_rows": choices["adoptable"] + choices["outside_set"],
            "set_cross_entropy": loss, "choices": dict(choices), "contexts": contexts,
            "minimum_margin": min(margins) if margins else None, "predictions": predictions}
    result.update(recommendations=dict(recommendations),
                  preferred_rows=sum(row.preferred_ordinals is not None for row in partition.rows),
                  target_basis_counts=dict(target_basis_counts),
                  explicit_preferred_choices=dict(explicit_preferred_choices),
                  minimum_explicit_preferred_margin=(min(explicit_preferred_margins)
                                                    if explicit_preferred_margins else None),
                  recommendation_set_cross_entropy=recommendation_loss,
                  minimum_recommendation_margin=min(recommendation_margins) if recommendation_margins else None)
    result["choice_supervision_signal"] = dict(signal)
    if families is not None:
        result.update(family_choices=dict(family_choices), target_choices=dict(target_choices),
                      family_supervision_signal=dict(family_signal),
                      recommendation_error_components=dict(recommendation_errors))
    return result


def _decision_action_families(batch, field):
    """Read exact native ActionKind values; candidate addresses are not features."""
    table = batch["semantic"]["categorical"]
    selected = table["field"] == field
    values = {}
    for token, value in zip(table["token_indices"][selected], table["value"][selected], strict=True):
        if int(token) in values:
            raise StrategicBehaviorCloningError("candidate action family repeats on one token")
        values[int(token)] = int(value)
    try:
        return tuple(values[int(token)] for token in batch["semantic"]["candidate_token_indices"])
    except KeyError as error:
        raise StrategicBehaviorCloningError("each candidate requires its native action family") from error


def _informative_family_rows(partition, family_codes):
    splits = partition.decision_batch["candidate_row_splits"]
    return tuple(i for i, row in enumerate(partition.rows) if row.choice_target_ordinals is not None
                 and {family_codes[int(splits[i]) + ordinal] for ordinal in row.choice_target_ordinals}
                     != set(family_codes[int(splits[i]):int(splits[i + 1])]))


def _family_decision_metrics(scorer, partition, field):
    """Family predictions are component outputs, never selected action ordinals."""
    codes = _decision_action_families(partition.decision_batch, field)
    scorer.eval()
    with torch.inference_mode():
        logits = scorer(partition.decision_batch)
        if logits.families is None:
            raise StrategicBehaviorCloningError("family task requires family logits")
        family = logits.families
        group_scores = family.values.cpu().tolist()
        membership = family.candidate_families.cpu().tolist()
        splits = logits.row_splits.cpu().tolist()
        group_splits = family.row_splits.cpu().tolist()
        loss = (float(ragged_acceptable_family_cross_entropy(logits, partition.choice_target_ordinals).cpu())
                if _informative_family_rows(partition, codes) else None)
    counts = Counter({"recommended": 0, "outside_set": 0, "uninformative": 0, "unknown": 0})
    predictions, margins = [], []
    for index, row in enumerate(partition.rows):
        start, end = splits[index:index + 2]
        group_start, group_end = group_splits[index:index + 2]
        selected = max(range(group_start, group_end), key=lambda group: group_scores[group])
        code = next(codes[i] for i in range(start, end) if membership[i] == selected)
        target = row.choice_target_ordinals
        allowed = None if target is None else {membership[start + ordinal] for ordinal in target}
        excluded = [] if allowed is None else [group for group in range(group_start, group_end) if group not in allowed]
        status = ("unknown" if allowed is None else "uninformative" if not excluded else
                  "recommended" if selected in allowed else "outside_set")
        counts[status] += 1
        margin = (max(group_scores[group] for group in allowed) - max(group_scores[group] for group in excluded)
                  if excluded else None)
        if margin is not None:
            margins.append(margin)
        predictions.append({"source_index": row.source_index, "row_index": row.row_index,
                            "seed": row.episode_seed, "family": code, "status": status, "margin": margin})
    return {"rows": len(partition.rows), "supervised_rows": counts["recommended"] + counts["outside_set"],
            "recommendations": dict(counts), "minimum_recommendation_margin": min(margins) if margins else None,
            "family_set_cross_entropy": loss, "predictions": predictions,
            "scope": "action-family classification only; no within-family action agreement"}


def _decision_fit_reached(metrics, config):
    return (metrics["supervised_rows"] > 0
            and metrics["recommendations"]["recommended"] == metrics["supervised_rows"]
            and (metrics["minimum_recommendation_margin"] is None
                 or metrics["minimum_recommendation_margin"] >= config.fit_margin))


def _decision_applicability(corpus, training, supervised, trained_sites):
    applicability = corpus.overlay["protocol"].get("applicability")
    if applicability is None:
        return None
    try:
        declared = StrategicModelApplicability.from_contract(
            applicability, schema=corpus.semantic_schema, model_sites=trained_sites)
        for i in supervised:
            row = training.rows[i]
            site = "card_reward" if row.context_kind == 0 else "campfire"
            if site in declared.contract()["sites"] and any(declared.issues(site, row.decision_batch)):
                raise ValueError("taught training row lies outside model applicability")
        return declared.contract()
    except ValueError as error:
        raise StrategicBehaviorCloningError(str(error)) from error


def _decision_source_contract(corpus, overlay_path, training, evaluation, partition_spec,
                              trained_sites, applicability, evaluation_role, source_owners):
    return {
        "overlay": str(Path(overlay_path).resolve()), "overlay_sha256": corpus.overlay_sha256,
        "sources": corpus.overlay["sources"], "author": corpus.overlay["author"],
        "label_revision": corpus.overlay["revision"], "annotation_protocol": corpus.overlay["protocol"],
        "producer": corpus.producer, "semantic_schema": corpus.semantic_schema,
        "partition": asdict(partition_spec), "model_sites": trained_sites, "applicability": applicability,
        "training_seeds": sorted({row.episode_seed for row in training.rows}),
        "evaluation_seeds": sorted({row.episode_seed for row in evaluation.rows}) if evaluation else [],
        "evaluation_role": evaluation_role if evaluation else "not_provided",
        "excluded_uncommitted_rows": sum(not row.eligible_for_supervision for row in corpus.rows),
        "pre_adoption_supervised_rows": sum(not row.committed and row.acceptable_ordinals is not None
                                            for row in corpus.rows),
        "excluded_unknown_training_rows": sum(row.acceptable_ordinals is None for row in training.rows),
        "choice_target": DECISION_CHOICE_TARGET,
        "preferred_training_rows": sum(row.preferred_ordinals is not None for row in training.rows),
        "context_codes": {"card_reward": 0, "campfire": 1},
        "inference": "full legal argmax",
        "stop": "training argmax in preferred set when declared, otherwise acceptable set; minimum choice-target margin reached",
        "scope": "offline named-teacher complete-action imitation; not RL, win rate or A20H qualification",
        "learning_source_sha256": {name: hashlib.sha256((Path(__file__).parent/name).read_bytes()).hexdigest()
                                   for name in source_owners},
    }


def train_scoped_strategic_decisions(overlay_path, output, *, model_seed=20261010,
                                    hidden_dim=32, config=None, evaluation_role="development",
                                    decision_architecture="flat", frozen_family_model=None,
                                    frozen_encoder_model=None, initial_model=None,
                                    family_pooling="vector-max", observation_pooling="mean",
                                    supervision_level="complete-action"):
    """Explicit task fit; only taught training targets can stop updates."""
    output = Path(output).resolve()
    if output.exists():
        raise FileExistsError(output)
    config = StrategicDecisionFitConfig() if config is None else config
    if not isinstance(config, StrategicDecisionFitConfig):
        raise StrategicBehaviorCloningError("decision fit config must be typed")
    if evaluation_role not in {"development", "held_out"}:
        raise StrategicBehaviorCloningError("evaluation_role must be development or held_out")
    if decision_architecture not in {"flat", "family-target"}:
        raise StrategicBehaviorCloningError("decision_architecture must be flat or family-target")
    if supervision_level not in {"complete-action", "action-family"}:
        raise StrategicBehaviorCloningError("invalid supervision_level")
    family_only = supervision_level == "action-family"
    if family_only and (decision_architecture != "family-target" or any(
            source is not None for source in (frozen_family_model, frozen_encoder_model, initial_model))):
        raise StrategicBehaviorCloningError("family-only task requires fresh family-target initialization")
    if (not isinstance(family_pooling, str) or
            family_pooling not in {"vector-max", "candidate-score-max"}):
        raise StrategicBehaviorCloningError("invalid family_pooling")
    if decision_architecture == "flat" and family_pooling != "vector-max":
        raise StrategicBehaviorCloningError("candidate-score-max requires family-target")
    if (not isinstance(observation_pooling, str) or
            observation_pooling not in {"mean", "candidate-attention"}):
        raise StrategicBehaviorCloningError("invalid observation_pooling")
    if decision_architecture != "flat" and observation_pooling != "mean":
        raise StrategicBehaviorCloningError("candidate-attention currently requires flat scoring")
    if sum(source is not None for source in (frozen_family_model, frozen_encoder_model, initial_model)) > 1:
        raise StrategicBehaviorCloningError("refinement sources are mutually exclusive")
    source_model = next((source for source in (frozen_family_model, frozen_encoder_model, initial_model)
                         if source is not None), None)
    if (frozen_family_model is not None or frozen_encoder_model is not None) and decision_architecture != "family-target":
        raise StrategicBehaviorCloningError("frozen scoped refinement requires family-target")
    corpus = load_scoped_strategic_decisions(overlay_path)
    partition_spec = SeedPartitionSpec()
    partitions = split_scoped_strategic_decisions(
        corpus, partition_spec,
        SemanticBatchConcatLimits(max_rows=4096, max_input_array_bytes=64*1024*1024))
    training = partitions.get(SeedPartition.TRAINING)
    evaluation = partitions.get(SeedPartition.HELD_OUT)
    if training is None:
        raise StrategicBehaviorCloningError("training requires at least one complete decision target")
    family_field = None
    if decision_architecture == "family-target":
        family_field = corpus.semantic_schema["categorical_field"].get("ActionKind")
        if family_field is None:
            raise StrategicBehaviorCloningError("family-target requires bridge ActionKind")
    supervised = tuple(i for i, row in enumerate(training.rows) if row.acceptable_ordinals is not None)
    if family_only:
        supervised = _informative_family_rows(training, _decision_action_families(training.decision_batch, family_field))
    if not supervised:
        raise StrategicBehaviorCloningError("training requires at least one informative family target" if family_only
                                           else "training requires at least one complete decision target")
    trained_sites = sorted({"card_reward" if training.rows[i].context_kind == 0 else "campfire"
                            for i in supervised})
    applicability = _decision_applicability(corpus, training, supervised, trained_sites)
    scorer_config = RaggedScorerConfig(hidden_dim=hidden_dim, relation_layers=2,
        family_field=family_field, family_pooling=family_pooling,
        observation_pooling=observation_pooling)
    torch.set_num_threads(2)
    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(model_seed)
        initial = RaggedCandidateScorer.from_bridge_schema(corpus.semantic_schema, config=scorer_config)
    initialization = {"kind": "fresh", "model_seed": model_seed}
    if source_model is not None:
        initialization = _load_scoped_refinement_initialization(
            initial, corpus.semantic_schema, scorer_config, source_model,
            freeze_family=frozen_family_model is not None,
            freeze_encoder=frozen_encoder_model is not None)
        evaluation_seeds = {row.episode_seed for row in evaluation.rows} if evaluation else set()
        if evaluation_seeds.intersection(initialization["source_training_seeds"]):
            raise StrategicBehaviorCloningError("source model trained on a current evaluation seed")
        if evaluation_role == "held_out" and evaluation_seeds.intersection(initialization["source_evaluation_seeds"]):
            raise StrategicBehaviorCloningError("source model previously evaluated these seeds; use development")
    initial.to(config.device_type).eval().requires_grad_(False)
    if frozen_family_model is not None:
        _require_frozen_families_fit(initial, training, config.fit_margin)
    trainable_prefixes = ("scorer.",) if frozen_family_model is not None else ("scorer.", "family_head.")
    frozen_state = {key: value.detach().cpu().clone() for key, value in initial.state_dict().items()
                    if not key.startswith(trainable_prefixes)} if (
                        frozen_family_model is not None or frozen_encoder_model is not None) else None
    output.mkdir(parents=True, exist_ok=False)
    contract = {**_decision_source_contract(corpus, overlay_path, training, evaluation, partition_spec,
                trained_sites, applicability, evaluation_role, (
                    "strategic_behavior_cloning.py", "strategic_demonstrations.py", "torch_policy.py",
                    "semantic_batch.py", "semantic_concat.py", "strategic_model_scope.py")),
                "schema": "sts-scoped-decision-clone-v1",
                "model_seed": model_seed if source_model is None else None,
                "scorer": asdict(scorer_config), "training": asdict(config),
                "initialization": initialization,
                "decision_architecture": decision_architecture,
                "loss": ("mean across supervised rows of logsumexp(all) - logsumexp(choice-target)"
                         if family_field is None else
                         "mean across supervised rows of choice-target-family set loss + mean within-family choice-target set loss"),
                "inference": ("full legal argmax" if family_field is None else
                              "full legal argmax of family score + target score - best target score in family"),
                }
    if family_only:
        contract.update(schema="sts-scoped-family-clone-v1", supervision_level=supervision_level,
                        choice_target="ActionKind sets derived from declared preferred or acceptable actions",
                        loss="family set cross entropy on rows that exclude at least one offered family",
                        inference="ActionKind classification; no action ordinal or within-family ranking",
                        informative_training_rows=len(supervised),
                        excluded_uninformative_training_rows=sum(row.choice_target_ordinals is not None
                                                                 for row in training.rows) - len(supervised),
                        stop="training family argmax in derived target set on every informative row; minimum family margin reached",
                        scope="Offline learned action-family component; cannot be loaded as a complete-action policy")
    (output/"contract.json").write_text(json.dumps(contract, indent=2) + "\n", encoding="utf-8")
    task_metrics = (lambda scorer, partition: _family_decision_metrics(scorer, partition, family_field)) if family_only else _decision_metrics
    initial_training = task_metrics(initial, training)
    trace = [{"update": 0, **{k: v for k, v in initial_training.items() if k not in {"predictions", "contexts"}}}]

    def check_fit(scorer, update):
        if update % config.check_every and update != config.max_updates:
            return False
        metrics = task_metrics(scorer, training)
        trace.append({"update": update, **{k: v for k, v in metrics.items() if k not in {"predictions", "contexts"}}})
        return _decision_fit_reached(metrics, config)

    batch = select_semantic_decision_rows(training.decision_batch, supervised)
    targets = tuple(training.choice_target_ordinals[i] for i in supervised)
    optimizer_config = StrategicBehaviorCloningConfig(
        epochs=config.max_updates, learning_rate=config.learning_rate,
        max_grad_norm=config.max_grad_norm, device_type=config.device_type)
    started = time.perf_counter()
    scorer = (initial if _decision_fit_reached(initial_training, config) else
              _fit_supervised_scorer(initial, batch,
                  lambda logits: (ragged_acceptable_family_cross_entropy(logits, targets) if family_only
                                  else ragged_acceptable_set_cross_entropy(logits, targets)),
                  optimizer_config, after_update=check_fit,
                  target_only=frozen_family_model is not None,
                  heads_only=frozen_encoder_model is not None))
    elapsed = time.perf_counter() - started
    final_training = task_metrics(scorer, training)
    if frozen_state is not None and any(
            not torch.equal(value, scorer.state_dict()[key].detach().cpu())
            for key, value in frozen_state.items()):
        raise StrategicBehaviorCloningError("scoped refinement changed frozen state")
    # Evaluation is first scored after the train-only stopping decision.
    model_hash, permutation_error = _save_scoped_scorer(
        scorer, corpus.semantic_schema, scorer_config,
        evaluation.decision_batch if evaluation else training.decision_batch, output)
    summary = {"initial_training": initial_training, "final_training": final_training,
               "final_evaluation": task_metrics(scorer, evaluation) if evaluation else None,
               "evaluation_role": contract["evaluation_role"],
               "stop_reason": "training_fit" if _decision_fit_reached(final_training, config) else "max_updates",
               "updates": trace[-1]["update"], "training_trace": trace, "elapsed_seconds": elapsed,
               "parameters": sum(parameter.numel() for parameter in scorer.parameters()),
               "model_sha256": model_hash,
               "validation": {"model_round_trip": True, "candidate_reversal_max_logit_error": permutation_error},
               "scope": contract["scope"]}
    if frozen_state is not None:
        check = "frozen_family_state_unchanged" if frozen_family_model is not None else "frozen_encoder_state_unchanged"
        summary["validation"][check] = True
    (output/"summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    return summary


@dataclass(frozen=True)
class StrategicFieldFitConfig:
    max_updates: int = 1024
    check_every: int = 32
    fit_margin: float = 0.5
    learning_rate: float = 0.05
    num_leaves: int = 31
    max_depth: int = 6
    min_data_in_leaf: int = 1
    min_data_in_bin: int = 3
    objective: str = "binary"
    readout: str = "owned-field-statistics-v1"
    relevance: str = "choice-target-v1"

    def __post_init__(self):
        fit = StrategicDecisionFitConfig(max_updates=self.max_updates, check_every=self.check_every,
                                        fit_margin=self.fit_margin, learning_rate=self.learning_rate)
        for name in ("max_updates", "check_every", "fit_margin", "learning_rate"):
            object.__setattr__(self, name, getattr(fit, name))
        for name in ("num_leaves", "max_depth", "min_data_in_leaf", "min_data_in_bin"):
            object.__setattr__(self, name, _positive(getattr(self, name), name))
        if self.num_leaves < 2:
            raise StrategicBehaviorCloningError("num_leaves must be at least two")
        if not isinstance(self.objective, str) or self.objective not in {"binary", "lambdarank"}:
            raise StrategicBehaviorCloningError("field objective must be binary or lambdarank")
        from .strategic_field_policy import FIELD_READOUTS, validate_field_relevance
        if not isinstance(self.readout, str) or self.readout not in FIELD_READOUTS:
            raise StrategicBehaviorCloningError("field readout differs")
        try:
            validate_field_relevance(self.relevance, self.objective)
        except ValueError as error:
            raise StrategicBehaviorCloningError(str(error)) from error


def _field_candidate_relevance(row, ordinal, relevance):
    from .strategic_field_policy import FIELD_CHOICE_TARGET_RELEVANCE, FIELD_GRADED_RELEVANCE
    if row.acceptable_ordinals is None:
        raise StrategicBehaviorCloningError("unknown decisions supply no relevance labels")
    if relevance == FIELD_CHOICE_TARGET_RELEVANCE:
        return int(ordinal in row.choice_target_ordinals)
    if relevance == FIELD_GRADED_RELEVANCE:
        return (2 if ordinal in row.choice_target_ordinals else
                1 if ordinal in row.acceptable_ordinals else 0)
    raise StrategicBehaviorCloningError("field relevance differs")


def train_scoped_field_classifier(overlay_path, output, *, model_seed=20261010,
                                  config=None, evaluation_role="development", frozen_field_model=None):
    """Candidate membership supervision with the same whole-decision fit gate.

    No unknown/partial labels become candidate negatives. The vocabulary and
    stopping decision see supervised training rows only; evaluation is scored
    once fitting stops. A verified frozen source is qualified against the whole
    new corpus without updates or vocabulary growth, under a new bound contract.
    Saved trees are a distinct backend, not Torch weights.
    """
    import lightgbm as lgb
    from .policy import BehaviorManifestId
    from .strategic_field_policy import (
        FIELD_CHOICE_TARGET_RELEVANCE, FIELD_GRADED_RELEVANCE,
        GreedyStrategicFieldPolicy, StrategicFieldEncoder, field_readout_limits,
    )
    output = Path(output).resolve()
    if output.exists():
        raise FileExistsError(output)
    if config is not None and not isinstance(config, StrategicFieldFitConfig):
        raise StrategicBehaviorCloningError("field fit config must be typed")
    if evaluation_role not in {"development", "held_out"}:
        raise StrategicBehaviorCloningError("evaluation_role must be development or held_out")
    if type(model_seed) is not int or not 0 <= model_seed < 2**31:
        raise StrategicBehaviorCloningError("field model_seed requires an integer in [0,2**31)")
    corpus = load_scoped_strategic_decisions(overlay_path)
    spec = SeedPartitionSpec()
    partitions = split_scoped_strategic_decisions(
        corpus, spec, SemanticBatchConcatLimits(max_rows=4096, max_input_array_bytes=64*1024*1024))
    training, evaluation = partitions.get(SeedPartition.TRAINING), partitions.get(SeedPartition.HELD_OUT)
    supervised = (() if training is None else
                  tuple(i for i, row in enumerate(training.rows) if row.acceptable_ordinals is not None))
    if not supervised:
        raise StrategicBehaviorCloningError("training requires at least one complete decision target")
    sites = sorted({"card_reward" if training.rows[i].context_kind == 0 else "campfire" for i in supervised})
    applicability = _decision_applicability(corpus, training, supervised, sites)
    frozen = None
    if frozen_field_model is not None:
        if applicability is None:
            raise StrategicBehaviorCloningError("frozen qualification requires explicit applicability")
        frozen = _load_frozen_field_source(frozen_field_model, output, corpus.semantic_schema, sites)
        policy, source_contract, source_files, initialization = frozen
        if config is None:
            config = StrategicFieldFitConfig(fit_margin=source_contract["training"]["fit_margin"],
                                            objective=policy.objective, readout=policy.encoder.readout,
                                            relevance=source_contract.get("field_relevance", FIELD_CHOICE_TARGET_RELEVANCE))
        if config.objective != policy.objective or config.readout != policy.encoder.readout:
            raise StrategicBehaviorCloningError("frozen source objective or readout differs")
        if config.relevance != source_contract.get("field_relevance", FIELD_CHOICE_TARGET_RELEVANCE):
            raise StrategicBehaviorCloningError("frozen source relevance differs")
        evaluation_seeds = {row.episode_seed for row in evaluation.rows} if evaluation else set()
        if evaluation_seeds.intersection(initialization["source_training_seeds"]):
            raise StrategicBehaviorCloningError("source model trained on a current evaluation seed")
        if evaluation_role == "held_out" and evaluation_seeds.intersection(initialization["source_evaluation_seeds"]):
            raise StrategicBehaviorCloningError("source model previously evaluated these seeds; use development")
    else:
        config = StrategicFieldFitConfig() if config is None else config
        initialization = {"kind": "fresh", "model_seed": model_seed}
    readout = policy.encoder if frozen else StrategicFieldEncoder(corpus.semantic_schema, readout=config.readout)
    features, splits = readout.candidate_features(training.decision_batch)
    indices = [j for i in supervised for j in range(int(splits[i]), int(splits[i+1]))]
    names = list(readout.feature_names) if frozen else sorted({name for i in indices for name in features[i]})
    encoder = StrategicFieldEncoder(corpus.semantic_schema, names, readout=config.readout)
    x = encoder.transform(features)
    targets = np.array([_field_candidate_relevance(training.rows[i], j-int(splits[i]), config.relevance)
                        for i in supervised for j in range(int(splits[i]), int(splits[i+1]))])
    params = dict(objective=config.objective, verbosity=-1, learning_rate=config.learning_rate,
                  num_leaves=config.num_leaves, max_depth=config.max_depth,
                  min_data_in_leaf=config.min_data_in_leaf, feature_pre_filter=False,
                  min_data_in_bin=config.min_data_in_bin,
                  num_threads=2, seed=model_seed, deterministic=True, force_col_wise=True)
    if config.relevance == FIELD_GRADED_RELEVANCE:
        params["label_gain"] = [0, 1, 2]
    if frozen:
        loss = "no optimization; complete corpus argmax and choice-target margin qualification"
    elif config.objective == "binary":
        loss = "binary membership of each candidate in the teacher choice-target set"
    elif config.relevance == FIELD_GRADED_RELEVANCE:
        loss = ("lambdarank over each full legal group, choice-target relevance2, "
                "secondary acceptable relevance1, outside-set relevance0; linear label gains")
    else:
        loss = "lambdarank over each full legal group, choice-target relevance1 and other relevance0"
    contract = {
        **_decision_source_contract(corpus, overlay_path, training, evaluation, spec, sites,
                                   applicability, evaluation_role, (
            "strategic_behavior_cloning.py", "strategic_demonstrations.py", "strategic_field_policy.py",
            "semantic_batch.py", "semantic_concat.py", "strategic_model_scope.py")),
        "schema": "sts-scoped-field-classifier-v1", "decision_architecture": "field-membership",
        "model_seed": None if frozen else model_seed, "initialization": initialization,
        "training": ({"mode": "frozen-qualification", "fit_margin": config.fit_margin,
                      "objective": config.objective, "readout": config.readout,
                      "relevance": config.relevance} if frozen else asdict(config)),
        "booster_parameters": None if frozen else params,
        "field_readout": config.readout, "field_objective": config.objective,
        "field_relevance": config.relevance, "feature_count": len(names),
        "loss": loss,
        "readout_limits": field_readout_limits(config.readout),
        "lightgbm_version": lgb.__version__,
    }
    output.mkdir(parents=True, exist_ok=False)
    (output/"feature-names.json").write_bytes(source_files["feature-names.json"] if frozen else
                                              (json.dumps(names)+"\n").encode("utf-8"))
    vocabulary_hash = hashlib.sha256((output/"feature-names.json").read_bytes()).hexdigest()
    contract["feature_names_sha256"] = vocabulary_hash
    (output/"contract.json").write_text(json.dumps(contract, indent=2)+"\n", encoding="utf-8")
    if frozen:
        model = policy.model
    else:
        data = lgb.Dataset(x[indices], label=targets, feature_name=[f"f{i}" for i in range(len(names))],
                           free_raw_data=False, params=params,
                           group=([int(training.decision_batch["candidate_counts"][i]) for i in supervised]
                                  if config.objective == "lambdarank" else None))
        model = lgb.Booster(params=params, train_set=data)

    def metrics(model, partition, matrix, bounds):
        scores = np.asarray(model.predict(matrix, raw_score=True, num_threads=2))
        if scores.shape != (int(bounds[-1]),) or not np.isfinite(scores).all():
            raise StrategicBehaviorCloningError("field classifier produced invalid scores")
        predicted = tuple(int(scores[int(a):int(b)].argmax()) for a, b in zip(bounds[:-1], bounds[1:], strict=True))
        action_field = corpus.semantic_schema["categorical_field"].get("ActionKind")
        return _decision_choice_metrics(partition, predicted, scores.tolist(), list(map(int, bounds)),
            families=None if action_field is None else _decision_action_families(partition.decision_batch, action_field))

    trace = []
    started = time.perf_counter()
    for update in (() if frozen else range(1, config.max_updates+1)):
        model.update()
        if update % config.check_every and update != config.max_updates:
            continue
        current = metrics(model, training, x, splits)
        trace.append({"update": update, **{k: v for k, v in current.items() if k not in {"predictions", "contexts"}}})
        if _decision_fit_reached(current, config):
            break
    final_training = metrics(model, training, x, splits)
    elapsed = time.perf_counter() - started
    if frozen:
        trace.append({"update": 0, **{k: v for k, v in final_training.items() if k not in {"predictions", "contexts"}}})
    payload = (source_files["model.lgb"] if frozen else
               model.model_to_string(num_iteration=model.current_iteration()).encode("utf-8"))
    (output/"model.lgb").write_bytes(payload)
    restored = lgb.Booster(model_str=payload.decode("utf-8"))
    policy = GreedyStrategicFieldPolicy(restored, encoder, BehaviorManifestId(bytes(32)), objective=config.objective)
    check_batch = evaluation.decision_batch if evaluation else training.decision_batch
    check_features, check_splits = encoder.candidate_features(check_batch)
    check_matrix = encoder.transform(check_features)
    expected = np.asarray(model.predict(check_matrix, raw_score=True, num_threads=2))
    actual = policy.scores(check_batch).values
    if not np.array_equal(expected, actual):
        raise StrategicBehaviorCloningError("saved field model does not reproduce evaluation scores")
    permuted = select_semantic_decision_rows(check_batch, range(len(check_batch["candidate_counts"])))
    reversal = np.concatenate([np.arange(int(b)-1, int(a)-1, -1) for a, b in zip(check_splits[:-1], check_splits[1:], strict=True)])
    permuted["semantic"]["candidate_token_indices"] = permuted["semantic"]["candidate_token_indices"][reversal]
    reversed_scores = policy.scores(permuted).values
    if not np.array_equal(reversed_scores, actual[reversal]):
        raise StrategicBehaviorCloningError("candidate permutation changed field scores")
    final_evaluation = metrics(restored, evaluation, check_matrix, check_splits) if evaluation else None
    summary = {
        "final_training": final_training, "final_evaluation": final_evaluation,
        "evaluation_role": contract["evaluation_role"],
        "stop_reason": ("training_fit" if _decision_fit_reached(final_training, config) else
                        "frozen_unfit" if frozen else "max_updates"),
        "updates": trace[-1]["update"], "training_trace": trace, "elapsed_seconds": elapsed,
        "model_sha256": hashlib.sha256(payload).hexdigest(), "feature_names_sha256": vocabulary_hash,
        "validation": {"model_round_trip": True, "candidate_reversal_max_logit_error": 0.0},
        "scope": contract["scope"],
    }
    if frozen:
        source_root = Path(initialization["root"])
        if any((source_root/name).read_bytes() != value for name, value in source_files.items()):
            raise StrategicBehaviorCloningError("frozen qualification source changed during verification")
        summary["validation"]["frozen_source_unchanged"] = True
    known_columns = set(encoder.feature_names)
    summary["ignored_columns"] = {
        "training": sorted({name for row in features for name in row if name not in known_columns}),
        "evaluation": sorted({name for row in check_features for name in row if name not in known_columns})
                      if evaluation else [],
    }
    if evaluation:
        support, cohorts = encoder.evaluation_support(
            features, splits, check_features, check_splits, training_rows=supervised)
        support["reference_role"] = ("current-supervised-qualification-partition-no-updates" if frozen else
                                     "current-supervised-training-partition")
        for prediction, cohort in zip(final_evaluation["predictions"], cohorts, strict=True):
            prediction["readout_support"] = cohort
        for cohort, counts in support["cohorts"].items():
            selected = [prediction for prediction in final_evaluation["predictions"]
                        if prediction["readout_support"] == cohort]
            for field in ("status", "recommendation_status"):
                counts[field] = dict(Counter(prediction[field] for prediction in selected))
        summary["evaluation_support"] = support
    else:
        summary["evaluation_support"] = None
    (output/"summary.json").write_text(json.dumps(summary, indent=2)+"\n", encoding="utf-8")
    return summary


def _load_frozen_field_source(root, output, schema, sites):
    """Use the deployment loader; qualification creates a new artifact only."""
    from .strategic_rollout import ModelSite, load_scoped_model
    from .strategic_field_policy import GreedyStrategicFieldPolicy

    root = Path(root).resolve()
    if output == root or output.is_relative_to(root):
        raise StrategicBehaviorCloningError("qualification output must be outside the source model directory")
    files = {name: (root/name).read_bytes() for name in ("contract.json", "summary.json")}
    source_contract = json.loads(files["contract.json"])
    if not isinstance(source_contract, dict) or source_contract.get("schema") != "sts-scoped-field-classifier-v1":
        raise StrategicBehaviorCloningError("frozen field qualification requires a verified field policy")
    files.update({name: (root/name).read_bytes() for name in ("feature-names.json", "model.lgb")})
    try:
        policy, source = load_scoped_model(root, schema=schema, model_sites=tuple(ModelSite(site) for site in sites))
    except ValueError as error:
        raise StrategicBehaviorCloningError(str(error)) from error
    if not isinstance(policy, GreedyStrategicFieldPolicy):
        raise StrategicBehaviorCloningError("frozen field qualification requires a verified field policy")
    if any((root/name).read_bytes() != value for name, value in files.items()):
        raise StrategicBehaviorCloningError("frozen qualification source changed during loading")
    initialization = {
        "kind": "frozen-field-qualification", "root": str(root),
        **{"source_"+key: value for key, value in source["manifest"]["source_seeds"].items()},
        "model_sha256": hashlib.sha256(files["model.lgb"]).hexdigest(),
        "feature_names_sha256": hashlib.sha256(files["feature-names.json"]).hexdigest(),
        "contract_sha256": hashlib.sha256(files["contract.json"]).hexdigest(),
        "summary_sha256": hashlib.sha256(files["summary.json"]).hexdigest(),
        "behavior_id": source["behavior_id"], "manifest": source["manifest"],
        "trainable_parameters": "none; weights and vocabulary bytes copied unchanged",
    }
    return policy, source_contract, files, initialization


def _load_scoped_refinement_initialization(scorer, schema, scorer_config, root, *, freeze_family, freeze_encoder):
    from .torch_checkpoints import decode_state_dict

    root = Path(root).resolve()
    contract_bytes = (root/"contract.json").read_bytes()
    summary_bytes = (root/"summary.json").read_bytes()
    contract, summary = json.loads(contract_bytes), json.loads(summary_bytes)
    source_config = contract.get("scorer") if isinstance(contract, dict) else None
    if isinstance(source_config, dict):
        source_config = {"family_pooling": "vector-max", "observation_pooling": "mean", **source_config}
    if (not isinstance(contract, dict) or not isinstance(summary, dict)
            or contract.get("schema") != "sts-scoped-decision-clone-v1"
            or contract.get("decision_architecture") != (
                "family-target" if scorer_config.family_field is not None else "flat")
            or contract.get("semantic_schema") != json.loads(json.dumps(schema))
            or source_config != asdict(scorer_config)):
        raise StrategicBehaviorCloningError("refinement source schema or scorer contract differs")
    payload = (root/"model.ststorch").read_bytes()
    model_sha = hashlib.sha256(payload).hexdigest()
    if (summary.get("model_sha256") != model_sha
            or summary.get("validation", {}).get("model_round_trip") is not True):
        raise StrategicBehaviorCloningError("refinement source weights are not verified")
    ancestor = contract.get("initialization", {})
    if not isinstance(ancestor, dict):
        raise StrategicBehaviorCloningError("refinement source initialization is malformed")
    source_seeds = {}
    for key in ("training_seeds", "evaluation_seeds"):
        direct, inherited = contract.get(key), ancestor.get("source_"+key, [])
        if (not isinstance(direct, list) or not isinstance(inherited, list)
                or any(type(seed) is not int or not 0 <= seed < 2**64 for seed in direct+inherited)):
            raise StrategicBehaviorCloningError("refinement source seed provenance is malformed")
        source_seeds["source_"+key] = sorted(set(direct+inherited))
    try:
        scorer.load_state_dict(decode_state_dict(payload), strict=True)
    except (RuntimeError, ValueError) as error:
        raise StrategicBehaviorCloningError("refinement source weights are incompatible") from error
    if freeze_family:
        kind = "frozen-family-target-refinement"
        trainable = "scorer.* only; graph, family head and value head remain frozen"
    elif freeze_encoder:
        kind = "frozen-encoder-policy-head-refinement"
        trainable = "scorer.* and family_head.* only; graph and value head remain frozen"
    else:
        kind = "full-model-initialization"
        trainable = "complete graph encoder and policy heads; no freezing constraint"
    return {"kind": kind, "root": str(root), **source_seeds,
            "model_sha256": model_sha, "contract_sha256": hashlib.sha256(contract_bytes).hexdigest(),
            "summary_sha256": hashlib.sha256(summary_bytes).hexdigest(),
            "trainable_parameters": trainable}


def _require_frozen_families_fit(scorer, partition, margin):
    """Target-only updates cannot repair an incompatible family decision."""
    with torch.inference_mode():
        logits = scorer(partition.decision_batch)
    f = logits.families
    if f is None:
        raise StrategicBehaviorCloningError("frozen-family refinement requires family logits")
    if not bool(torch.isfinite(f.values).all()):
        raise StrategicBehaviorCloningError("frozen family source scores are not finite")
    splits = logits.row_splits.tolist()
    for row, acceptable in enumerate(partition.choice_target_ordinals):
        if acceptable is None:
            continue
        start, end = splits[row:row+2]
        accepted_groups = set(f.candidate_families[start:end][list(acceptable)].tolist())
        group_start, group_end = f.row_splits[row:row+2].tolist()
        other = [g for g in range(group_start, group_end) if g not in accepted_groups]
        if other:
            gap = float(f.values[list(accepted_groups)].max() - f.values[other].max())
            if gap < margin:
                raise StrategicBehaviorCloningError(
                    f"frozen family cannot meet training row {row} margin; observed {gap}, required {margin}")


def _agreement(
    scorer: RaggedCandidateScorer,
    partition: StrategicDemonstrationPartition,
    batch_rows: int | None = None,
) -> StrategicAgreement:
    scorer.eval()
    predicted = []
    total_loss = 0.0
    with torch.inference_mode():
        size = batch_rows or partition.row_count
        for start in range(0, partition.row_count, size):
            stop = min(start+size, partition.row_count)
            inputs = (partition.decision_batch if size >= partition.row_count else
                      select_semantic_decision_rows(partition.decision_batch, range(start, stop)))
            logits = scorer(inputs)
            loss = ragged_cross_entropy(logits, partition.target_ordinals[start:stop])
            predicted.extend(logits.greedy_ordinals())
            total_loss += float(loss.detach().cpu().item())*(stop-start)
    correct = tuple(
        predicted[index] == target
        for index, target in enumerate(partition.target_ordinals)
    )
    context_counts: Counter[int] = Counter()
    context_correct: Counter[int] = Counter()
    for context, matched in zip(partition.context_kinds, correct, strict=True):
        context_counts[context] += 1
        context_correct[context] += int(matched)
    return StrategicAgreement(
        rows=partition.row_count,
        correct=sum(correct),
        cross_entropy=total_loss/partition.row_count,
        contexts=tuple(
            StrategicContextAgreement(
                context_kind=context,
                rows=context_counts[context],
                correct=context_correct[context],
            )
            for context in sorted(context_counts)
        ),
    )


def _integer_vector(source: Mapping[str, object], key: str) -> np.ndarray:
    value = source.get(key)
    if not isinstance(value, np.ndarray) or value.ndim != 1:
        raise StrategicBehaviorCloningError(f"{key} must be a NumPy vector")
    if value.dtype.kind not in "iu":
        raise StrategicBehaviorCloningError(f"{key} must contain integers")
    return value


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise StrategicBehaviorCloningError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise StrategicBehaviorCloningError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise StrategicBehaviorCloningError(f"{name} must be positive")
    return normalized


def clone_scoped_baseline_samples(sample_roots, output, *, model_seed=20261010,
                                  hidden_dim=32, epochs=16):
    """Fresh mechanical BC smoke; baseline agreement is not strategic quality.

    No old weights, combat anchor, environment or teacher service is loaded.
    All input roots must identify the same public baseline and numerical schema.
    """
    output = Path(output).resolve()
    if output.exists():
        raise FileExistsError(output)
    partition_spec = SeedPartitionSpec()
    partitions = {partition: [] for partition in SeedPartition}
    targets = {partition: [] for partition in SeedPartition}
    seeds = {partition: [] for partition in SeedPartition}
    contexts = {partition: [] for partition in SeedPartition}
    source_rows = []
    seen = set()
    schema = producer = None
    excluded = 0
    for root in sample_roots:
        root = Path(root).resolve()
        manifest, rows = load_scoped_strategic_samples(root)
        if schema is None:
            schema, producer = manifest["semantic_schema"], manifest["producer"]
        if schema != manifest["semantic_schema"] or producer != manifest["producer"]:
            raise StrategicBehaviorCloningError("sample producers or schemas differ")
        source_rows.append({"root": str(root), "manifest_sha256": hashlib.sha256(
            (root/"manifest.json").read_bytes()).hexdigest()})
        for batch, record in rows:
            if not record["committed"]:
                excluded += 1
                continue
            decision = record["decision"]
            if record["label_source"] != "public_baseline" or decision["owner"] != "baseline":
                raise StrategicBehaviorCloningError("smoke labels must come from the public baseline")
            site = decision["audit"]["decision_site"]
            if site not in ("card_reward", "campfire"):
                raise StrategicBehaviorCloningError("sample escapes the declared model scope")
            seed = decision["context"]["seed"]
            identity = (seed, decision["source_token"])
            if identity in seen:
                raise StrategicBehaviorCloningError("sample repeats a source decision")
            seen.add(identity)
            partition = partition_spec.classify(seed)
            partitions[partition].append(batch)
            targets[partition].append(decision["ordinal"])
            seeds[partition].append(seed)
            contexts[partition].append(0 if site == "card_reward" else 1)
    materialized = {}
    for partition in SeedPartition:
        if not partitions[partition]:
            raise StrategicBehaviorCloningError(f"no committed {partition.value} sample rows")
        materialized[partition] = StrategicDemonstrationPartition(
            concatenate_semantic_decision_batches(partitions[partition],
                SemanticBatchConcatLimits(max_rows=4096, max_input_array_bytes=64*1024*1024)),
            tuple(targets[partition]), tuple(seeds[partition]), tuple(contexts[partition]))
    split = StrategicDemonstrationSplit(materialized[SeedPartition.TRAINING],
                                       materialized[SeedPartition.HELD_OUT], partition_spec)
    config = StrategicBehaviorCloningConfig(epochs=epochs)
    scorer_config = RaggedScorerConfig(hidden_dim=hidden_dim, relation_layers=2)
    torch.set_num_threads(2)
    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(model_seed)
        initial = RaggedCandidateScorer.from_bridge_schema(schema, config=scorer_config)
    initial.eval().requires_grad_(False)
    output.mkdir(parents=True, exist_ok=False)
    contract = {"schema": "sts-scoped-baseline-clone-v1", "sources": source_rows,
                "producer": producer, "semantic_schema": schema,
                "model_seed": model_seed, "scorer": asdict(scorer_config), "training": asdict(config),
                "partition": asdict(partition_spec), "model_sites": ["card_reward", "campfire"],
                "training_seeds": sorted(set(split.training.episode_seeds)),
                "held_out_seeds": sorted(set(split.held_out.episode_seeds)),
                "excluded_uncommitted_rows": excluded,
                "context_codes": {"card_reward": 0, "campfire": 1},
                "scope": "mechanical baseline imitation smoke; not expert supervision, RL or A20H qualification",
                "learning_source_sha256": {name: hashlib.sha256(
                    (Path(__file__).parent/name).read_bytes()).hexdigest() for name in (
                        "strategic_behavior_cloning.py", "strategic_demonstrations.py",
                        "torch_policy.py", "semantic_batch.py", "semantic_concat.py")}}
    (output/"contract.json").write_text(json.dumps(contract, indent=2)+"\n", encoding="utf-8")
    result = train_strategic_behavior_clone(initial, split, config)
    model_hash, permutation_error = _save_scoped_scorer(
        result.scorer, schema, scorer_config, split.held_out.decision_batch, output)
    summary = {"initial_training": asdict(result.initial_training),
               "initial_held_out": asdict(result.initial_held_out),
               "final_training": asdict(result.final_training),
               "final_held_out": asdict(result.final_held_out),
               "epochs": result.epochs, "elapsed_seconds": result.elapsed_seconds,
               "parameters": sum(parameter.numel() for parameter in result.scorer.parameters()),
               "model_sha256": model_hash,
               "validation": {"model_round_trip": True,
                              "candidate_reversal_max_logit_error": permutation_error},
               "scope": contract["scope"]}
    (output/"summary.json").write_text(json.dumps(summary, indent=2)+"\n", encoding="utf-8")
    return summary


def _save_scoped_scorer(scorer, schema, scorer_config, held_out, output):
    from .torch_checkpoints import encode_state_dict, decode_state_dict

    payload = encode_state_dict(scorer.state_dict(), max_bytes=64*1024*1024)
    (output/"model.ststorch").write_bytes(payload)
    with torch.random.fork_rng(devices=[]):
        reloaded = RaggedCandidateScorer.from_bridge_schema(schema, config=scorer_config)
    reloaded.load_state_dict(decode_state_dict((output/"model.ststorch").read_bytes()), strict=True)
    reloaded.to(next(scorer.parameters()).device).eval().requires_grad_(False)
    permuted = select_semantic_decision_rows(held_out, tuple(range(len(held_out["candidate_counts"]))))
    splits = held_out["candidate_row_splits"]
    reversal = np.concatenate([np.arange(int(end)-1, int(start)-1, -1)
                              for start, end in zip(splits[:-1], splits[1:])])
    permuted["semantic"]["candidate_token_indices"] = (
        permuted["semantic"]["candidate_token_indices"][reversal])
    with torch.inference_mode():
        expected = scorer(held_out).values
        restored = reloaded(held_out).values
        reversed_scores = reloaded(permuted).values
    if not torch.equal(expected, restored):
        raise StrategicBehaviorCloningError("saved model does not reproduce held-out scores")
    permutation_error = float(torch.max(torch.abs(reversed_scores-restored[reversal])).item())
    if permutation_error != 0.0:
        raise StrategicBehaviorCloningError("candidate permutation changed semantic scores")
    return hashlib.sha256(payload).hexdigest(), permutation_error


@dataclass(frozen=True)
class StrategicPreferencePartition:
    decision_batch: Mapping[str, object]
    rows: tuple[ScopedStrategicPreferenceRow, ...]

    @property
    def preferred_pairs(self):
        return tuple(row.preferred_pairs for row in self.rows)


def split_scoped_strategic_preferences(corpus, partition_spec, concat_limits):
    if not isinstance(corpus, ScopedStrategicPreferences):
        raise StrategicBehaviorCloningError("preferences must be a typed corpus")
    if not isinstance(partition_spec, SeedPartitionSpec):
        raise StrategicBehaviorCloningError("partition_spec must be typed")
    partitions = {}
    for partition in SeedPartition:
        rows = tuple(row for row in corpus.rows
                     if row.eligible_for_supervision and partition_spec.classify(row.episode_seed) is partition)
        if not rows:
            raise StrategicBehaviorCloningError(f"no committed {partition.value} preference rows")
        partitions[partition] = StrategicPreferencePartition(
            concatenate_semantic_decision_batches([row.decision_batch for row in rows], concat_limits),
            rows,
        )
    return partitions


def _preference_metrics(scorer, partition, *, row_reduction="max"):
    scorer.eval()
    pairs = partition.preferred_pairs
    with torch.inference_mode():
        logits = scorer(partition.decision_batch)
        loss = (float(ragged_resolved_pairwise_loss(logits, pairs, row_reduction=row_reduction).cpu().item())
                if any(pairs) else None)
        predicted = logits.greedy_ordinals()
        values = logits.values.cpu().tolist()
        splits = logits.row_splits.cpu().tolist()
    choices = Counter({"preferred": 0, "disfavored": 0, "unknown": 0})
    contexts = {}
    predictions = []
    pair_count = ordered = ties = 0
    row_agreement = []
    for index, (row, ordinal) in enumerate(zip(partition.rows, predicted, strict=True)):
        status = ("preferred" if ordinal in row.preferred_ordinals else
                  "disfavored" if ordinal in row.disfavored_ordinals else "unknown")
        choices[status] += 1
        context = contexts.setdefault(row.context_kind,
                                      {"rows": 0, "preferred": 0, "disfavored": 0, "unknown": 0})
        context["rows"] += 1
        context[status] += 1
        predictions.append({"source_index": row.source_index, "row_index": row.row_index,
                            "seed": row.episode_seed, "context_kind": row.context_kind,
                            "ordinal": ordinal, "status": status})
        start = splits[index]
        comparisons = [(values[start + good], values[start + bad]) for good, bad in row.preferred_pairs]
        if comparisons:
            matched = sum(good > bad for good, bad in comparisons)
            ordered += matched
            ties += sum(good == bad for good, bad in comparisons)
            pair_count += len(comparisons)
            row_agreement.append(matched / len(comparisons))
    return {"rows": len(partition.rows), "supervised_rows": len(row_agreement),
            "resolved_pairs": pair_count, "ordered_pairs": ordered, "tied_pairs": ties,
            "row_mean_pair_agreement": (sum(row_agreement) / len(row_agreement) if row_agreement else None),
            "pairwise_loss": loss, "choices": dict(choices), "contexts": contexts,
            "predictions": predictions}


def train_scoped_strategic_preferences(overlay_path, output, *, model_seed=20261010,
                                      hidden_dim=32, epochs=16, row_reduction="max"):
    """Fresh partial-preference smoke; held-out labels never supply gradients."""
    output = Path(output).resolve()
    if output.exists():
        raise FileExistsError(output)
    if row_reduction not in {"mean", "max"}:
        raise StrategicBehaviorCloningError("preference row_reduction must be mean or max")
    corpus = load_scoped_strategic_preferences(overlay_path)
    partition_spec = SeedPartitionSpec()
    partitions = split_scoped_strategic_preferences(
        corpus, partition_spec,
        SemanticBatchConcatLimits(max_rows=4096, max_input_array_bytes=64*1024*1024))
    training = partitions[SeedPartition.TRAINING]
    held_out = partitions[SeedPartition.HELD_OUT]
    supervised_rows = tuple(index for index, pairs in enumerate(training.preferred_pairs) if pairs)
    if not supervised_rows:
        raise StrategicBehaviorCloningError("training requires at least one resolved preference")
    config = StrategicBehaviorCloningConfig(epochs=epochs)
    scorer_config = RaggedScorerConfig(hidden_dim=hidden_dim, relation_layers=2)
    torch.set_num_threads(2)
    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(model_seed)
        initial = RaggedCandidateScorer.from_bridge_schema(corpus.semantic_schema, config=scorer_config)
    initial.eval().requires_grad_(False)
    output.mkdir(parents=True, exist_ok=False)
    contract = {"schema": "sts-scoped-preference-clone-v1",
                "overlay": str(Path(overlay_path).resolve()), "overlay_sha256": corpus.overlay_sha256,
                "sources": corpus.overlay["sources"], "reviewer": corpus.overlay["reviewer"],
                "annotation_protocol": corpus.overlay["protocol"],
                "producer": corpus.producer, "semantic_schema": corpus.semantic_schema,
                "model_seed": model_seed, "scorer": asdict(scorer_config), "training": asdict(config),
                "partition": asdict(partition_spec), "model_sites": ["card_reward", "campfire"],
                "training_seeds": sorted({row.episode_seed for row in training.rows}),
                "held_out_seeds": sorted({row.episode_seed for row in held_out.rows}),
                "excluded_uncommitted_rows": sum(not row.eligible_for_supervision for row in corpus.rows),
                "pre_adoption_supervised_rows": sum(not row.committed and bool(row.preferred_pairs)
                                                    for row in corpus.rows),
                "context_codes": {"card_reward": 0, "campfire": 1},
                "loss": f"{row_reduction} within row, then mean across rows of softplus(disfavored - preferred)",
                "row_reduction": row_reduction,
                "scope": "offline reviewer-preference smoke; not original expert labels, RL, win rate or A20H qualification",
                "learning_source_sha256": {name: hashlib.sha256(
                    (Path(__file__).parent/name).read_bytes()).hexdigest() for name in (
                        "strategic_behavior_cloning.py", "strategic_demonstrations.py",
                        "torch_policy.py", "semantic_batch.py", "semantic_concat.py")}}
    (output/"contract.json").write_text(json.dumps(contract, indent=2)+"\n", encoding="utf-8")
    initial_training = _preference_metrics(initial, training, row_reduction=row_reduction)
    initial_held_out = _preference_metrics(initial, held_out, row_reduction=row_reduction)
    batch = select_semantic_decision_rows(training.decision_batch, supervised_rows)
    pairs = tuple(training.preferred_pairs[index] for index in supervised_rows)
    started = time.perf_counter()
    scorer = _fit_supervised_scorer(initial, batch,
                                  lambda logits: ragged_resolved_pairwise_loss(
                                      logits, pairs, row_reduction=row_reduction), config)
    elapsed = time.perf_counter() - started
    model_hash, permutation_error = _save_scoped_scorer(
        scorer, corpus.semantic_schema, scorer_config, held_out.decision_batch, output)
    summary = {"initial_training": initial_training, "initial_held_out": initial_held_out,
               "final_training": _preference_metrics(scorer, training, row_reduction=row_reduction),
               "final_held_out": _preference_metrics(scorer, held_out, row_reduction=row_reduction),
               "epochs": epochs, "elapsed_seconds": elapsed,
               "parameters": sum(parameter.numel() for parameter in scorer.parameters()),
               "model_sha256": model_hash,
               "validation": {"model_round_trip": True,
                              "candidate_reversal_max_logit_error": permutation_error},
               "scope": contract["scope"]}
    (output/"summary.json").write_text(json.dumps(summary, indent=2)+"\n", encoding="utf-8")
    return summary


def main():
    parser = argparse.ArgumentParser(description="Offline scoped baseline, partial preference or complete-action fit")
    labels = parser.add_mutually_exclusive_group(required=True)
    labels.add_argument("--samples", type=Path, nargs="+")
    labels.add_argument("--preferences", type=Path)
    labels.add_argument("--decisions", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--epochs", type=int, help="Baseline or partial-preference update count (default: 16)")
    parser.add_argument("--hidden-dim", type=int, default=32)
    parser.add_argument("--model-seed", type=int)
    parser.add_argument("--preference-reduction", choices=("mean", "max"))
    parser.add_argument("--max-updates", type=int, help="Decision fit update bound (default: 256)")
    parser.add_argument("--check-every", type=int, help="Decision fit check interval (default: 16)")
    parser.add_argument("--learning-rate", type=float, help="Decision learning rate (default: 0.003)")
    parser.add_argument("--fit-margin", type=float, help="Selected supervision task's training stopping margin (default: 0.5)")
    parser.add_argument("--evaluation-role", choices=("development", "held_out"))
    parser.add_argument("--decision-architecture", choices=("flat", "family-target", "field-membership"))
    parser.add_argument("--supervision-level", choices=("complete-action", "action-family"),
                        help="action-family learns only native action kinds, not target choices")
    parser.add_argument("--field-num-leaves", type=int)
    parser.add_argument("--field-max-depth", type=int)
    parser.add_argument("--field-min-data-in-leaf", type=int)
    parser.add_argument("--field-min-data-in-bin", type=int)
    parser.add_argument("--field-objective", choices=("binary", "lambdarank"))
    from .strategic_field_policy import FIELD_READOUTS, FIELD_RELEVANCES
    parser.add_argument("--field-readout", choices=FIELD_READOUTS)
    parser.add_argument("--field-relevance", choices=FIELD_RELEVANCES)
    parser.add_argument("--family-pooling", choices=("vector-max", "candidate-score-max"))
    parser.add_argument("--observation-pooling", choices=("mean", "candidate-attention"))
    sources = parser.add_mutually_exclusive_group()
    sources.add_argument("--frozen-family-model", type=Path)
    sources.add_argument("--frozen-encoder-model", type=Path)
    sources.add_argument("--initial-model", type=Path)
    sources.add_argument("--frozen-field-model", type=Path,
                         help="Qualify verified field weights and vocabulary on the new complete corpus, without updates")
    args = parser.parse_args()
    fit_options = {name: getattr(args, name) for name in
                   ("max_updates", "check_every", "learning_rate", "fit_margin")
                   if getattr(args, name) is not None}
    field_options = {name: getattr(args, "field_"+name) for name in ("num_leaves", "max_depth", "min_data_in_leaf", "min_data_in_bin", "objective", "readout", "relevance")
                     if getattr(args, "field_"+name) is not None}
    if field_options and (not args.decisions or args.decision_architecture != "field-membership"):
        parser.error("field classifier options require --decisions --decision-architecture field-membership")
    if args.supervision_level is not None and (not args.decisions or args.decision_architecture == "field-membership"):
        parser.error("--supervision-level requires a neural --decisions task")
    if args.frozen_field_model is not None:
        if not args.decisions or args.decision_architecture != "field-membership":
            parser.error("--frozen-field-model requires --decisions --decision-architecture field-membership")
        if fit_options or field_options or args.model_seed is not None:
            parser.error("frozen field qualification inherits source readout, objective and fit margin; it uses no optimizer options")
    if args.decision_architecture == "field-membership" and (
            args.frozen_family_model or args.frozen_encoder_model or args.initial_model
            or args.family_pooling or args.observation_pooling or args.hidden_dim != 32):
        parser.error("field-membership uses field configuration")
    if not args.decisions and (fit_options or args.evaluation_role is not None
                              or args.decision_architecture is not None or args.frozen_family_model is not None
                              or args.frozen_encoder_model is not None or args.initial_model is not None
                              or args.family_pooling is not None or args.observation_pooling is not None):
        parser.error("complete-action fit options require --decisions")
    if args.decisions and args.epochs is not None:
        parser.error("--decisions uses --max-updates and train-only fit stopping, not --epochs")
    if not args.preferences and args.preference_reduction is not None:
        parser.error("--preference-reduction requires --preferences")
    if args.decisions:
        result = (train_scoped_field_classifier(
            args.decisions, args.output, model_seed=args.model_seed if args.model_seed is not None else 20261010,
            evaluation_role=args.evaluation_role or "development",
            frozen_field_model=args.frozen_field_model,
            config=None if args.frozen_field_model else StrategicFieldFitConfig(**fit_options, **field_options))
            if args.decision_architecture == "field-membership" else train_scoped_strategic_decisions(
            args.decisions, args.output, model_seed=args.model_seed if args.model_seed is not None else 20261010, hidden_dim=args.hidden_dim,
            evaluation_role=args.evaluation_role or "development",
            decision_architecture=args.decision_architecture or "flat",
            family_pooling=args.family_pooling or "vector-max",
            observation_pooling=args.observation_pooling or "mean",
            supervision_level=args.supervision_level or "complete-action",
            frozen_family_model=args.frozen_family_model,
            frozen_encoder_model=args.frozen_encoder_model,
            initial_model=args.initial_model,
            config=StrategicDecisionFitConfig(**fit_options)))
    else:
        train = train_scoped_strategic_preferences if args.preferences else clone_scoped_baseline_samples
        result = train(args.preferences or args.samples, args.output, model_seed=args.model_seed if args.model_seed is not None else 20261010,
                       hidden_dim=args.hidden_dim, epochs=args.epochs if args.epochs is not None else 16,
                       **({"row_reduction": args.preference_reduction or "max"} if args.preferences else {}))
    print(json.dumps({key: {k: v for k, v in value.items() if k != "predictions"}
                           if isinstance(value, dict) else value for key, value in result.items()
                      if key != "training_trace"}))


if __name__ == "__main__":
    main()
