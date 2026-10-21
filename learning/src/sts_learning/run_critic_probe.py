"""Actor-neutral fixed-trajectory probe for whole-run critic learnability."""

from __future__ import annotations

import argparse
import json
import math
import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import torch

from .combat_potion_lane import CombatPotionLane
from .evaluate_run import RunPotionLane, resolve_run_potion_lane
from .evaluation import (
    HeldOutEvaluationSpec,
    evaluate_held_out_behavior_with_public_trajectories,
)
from .public_trajectory import PublicAttemptTrajectoryV1, PublicTrajectoryDecisionV1
from .published_run_behavior import (
    PublishedRunBehavior,
    is_run_training_publication,
    recover_published_run_behavior,
)
from .run_rollout import build_complete_run_rollout, decision_progress_potential
from .seeds import SeedPartition, SeedSchedule
from .semantic_concat import concatenate_semantic_decision_batches
from .terminal_returns import FloorProgressReturnConfig
from .torch_policy import (
    RaggedCandidateScorer,
    configure_critic_only_training,
    load_scorer_warm_start,
    require_matching_actor_state,
)
from .torch_session_config import (
    CategoricalSessionBridge,
    CategoricalSessionLimits,
)


RUN_CRITIC_PROBE_SCHEMA = "sts-learning-run-critic-probe-v2"


class RunCriticProbeError(RuntimeError):
    """A fixed-trajectory critic probe cannot produce comparable evidence."""


@dataclass(frozen=True)
class RunCriticProbeConfig:
    behavior: Path
    output: Path
    ascension_level: int
    train_attempts: int = 24
    held_out_attempts: int = 8
    max_batch_steps: int = 32_768
    behavior_seed: int = 10_000
    held_out_seed_start: int = 1_000_000
    head_fit_steps: int = 256
    head_fit_learning_rate: float = 1e-3
    model_seed: int = 0
    potion_lane: RunPotionLane = RunPotionLane.TRAINED

    def __post_init__(self) -> None:
        behavior = Path(self.behavior).resolve()
        output = Path(self.output).resolve()
        if not behavior.is_dir():
            raise RunCriticProbeError("probe behavior is not a directory")
        if not is_run_training_publication(behavior):
            raise RunCriticProbeError(
                "run critic probe requires a completed run-training publication"
            )
        if output.exists() and (not output.is_dir() or any(output.iterdir())):
            raise RunCriticProbeError("probe output must be absent or empty")
        if output == behavior or behavior in output.parents:
            raise RunCriticProbeError(
                "probe output must stay outside the behavior directory"
            )
        object.__setattr__(self, "behavior", behavior)
        object.__setattr__(self, "output", output)
        ascension = _integer(self.ascension_level, "ascension_level", minimum=0)
        if ascension > 20:
            raise RunCriticProbeError("ascension_level must be at most 20")
        object.__setattr__(self, "ascension_level", ascension)
        for name in ("train_attempts", "held_out_attempts", "max_batch_steps"):
            object.__setattr__(
                self,
                name,
                _integer(getattr(self, name), name, minimum=1),
            )
        object.__setattr__(
            self,
            "head_fit_steps",
            _integer(self.head_fit_steps, "head_fit_steps", minimum=1),
        )
        if self.train_attempts + self.held_out_attempts > 64:
            raise RunCriticProbeError(
                "fixed critic probe supports at most 64 complete attempts"
            )
        for name in ("behavior_seed", "held_out_seed_start", "model_seed"):
            value = _integer(getattr(self, name), name, minimum=0)
            if value >= 1 << 63:
                raise RunCriticProbeError(f"{name} must be below 2^63")
            object.__setattr__(self, name, value)
        learning_rate = float(self.head_fit_learning_rate)
        if not math.isfinite(learning_rate) or learning_rate <= 0.0:
            raise RunCriticProbeError(
                "head_fit_learning_rate must be finite and positive"
            )
        object.__setattr__(self, "head_fit_learning_rate", learning_rate)
        if not isinstance(self.potion_lane, RunPotionLane):
            raise RunCriticProbeError("probe potion_lane must be typed")

    @property
    def total_attempts(self) -> int:
        return self.train_attempts + self.held_out_attempts


@dataclass(frozen=True)
class _ProbeRow:
    attempt_index: int
    floor: int
    context_kind: int
    target: float
    decision: PublicTrajectoryDecisionV1


@dataclass(frozen=True)
class _ProbeDataset:
    attempts: tuple[PublicAttemptTrajectoryV1, ...]
    rows: tuple[_ProbeRow, ...]
    return_config: FloorProgressReturnConfig = FloorProgressReturnConfig()

    @property
    def weights(self) -> np.ndarray:
        counts: dict[int, int] = {}
        for row in self.rows:
            counts[row.attempt_index] = counts.get(row.attempt_index, 0) + 1
        return np.asarray(
            [1.0 / (len(counts) * counts[row.attempt_index]) for row in self.rows],
            dtype=np.float64,
        )

    @property
    def targets(self) -> np.ndarray:
        return np.asarray([row.target for row in self.rows], dtype=np.float64)

    @property
    def payloads(self) -> tuple[Mapping[str, object], ...]:
        return tuple(row.decision.semantic_payload for row in self.rows)


def run_run_critic_probe(
    config: RunCriticProbeConfig,
    *,
    run_bridge: CategoricalSessionBridge | None = None,
) -> dict[str, object]:
    """Collect one immutable cohort and compare critic signal surfaces."""

    if not isinstance(config, RunCriticProbeConfig):
        raise RunCriticProbeError("run critic probe config must be typed")
    active_bridge = (
        run_bridge if run_bridge is not None else CategoricalSessionBridge.installed()
    )
    if not isinstance(active_bridge, CategoricalSessionBridge):
        raise RunCriticProbeError("run critic probe bridge must be typed")
    recovered = recover_published_run_behavior(
        config.behavior,
        active_bridge,
        (config.behavior_seed,),
    )
    _require_scalar_critic(recovered)
    if recovered.training_ascension_level != config.ascension_level:
        raise RunCriticProbeError(
            "probe ascension differs from the published run behavior"
        )
    potion_lane = resolve_run_potion_lane(config.potion_lane, recovered)
    environment_constructor = (
        active_bridge.environment
        if potion_lane is CombatPotionLane.ALL
        else active_bridge.environment_without_combat_potions
    )
    limits = CategoricalSessionLimits()
    collected = evaluate_held_out_behavior_with_public_trajectories(
        lambda seeds: environment_constructor(seeds, config.ascension_level),
        recovered.policies[0],
        schedule=SeedSchedule(
            SeedPartition.HELD_OUT,
            next_candidate=config.held_out_seed_start,
        ),
        spec=HeldOutEvaluationSpec(
            slot_count=1,
            terminal_attempt_target=config.total_attempts,
            max_batch_steps=config.max_batch_steps,
        ),
        experience_limits=limits.experience,
        attempt_limits=limits.attempts,
    )
    if not collected.evaluation.complete:
        raise RunCriticProbeError(
            "critic probe hit its batch-step limit before the fixed cohort completed"
        )
    trajectories = collected.trajectories
    training = _build_dataset(trajectories[: config.train_attempts], recovered.objective.terminal_return)
    held_out = _build_dataset(trajectories[config.train_attempts :], recovered.objective.terminal_return)
    if len(training.rows) + len(held_out.rows) > limits.concat.max_rows:
        raise RunCriticProbeError(
            "fixed critic probe decision rows exceed one bounded semantic batch"
        )

    source_scorer = recovered.policies[0].frozen_scorer
    target = recovered.objective.policy_update.value_prediction_target
    published_train = _score_scorer(source_scorer, training, limits, value_prediction_target=target)
    published_held_out = _score_scorer(source_scorer, held_out, limits, value_prediction_target=target)

    constant_value = _weighted_mean(training.targets, training.weights)
    constant_train = np.full(len(training.rows), constant_value, dtype=np.float64)
    constant_held_out = np.full(len(held_out.rows), constant_value, dtype=np.float64)

    feature_names, train_features, held_out_features = _public_feature_matrices(
        training,
        held_out,
        active_bridge.semantic_schema,
    )
    linear_train, linear_held_out = _fit_weighted_ridge(
        train_features,
        training.targets,
        training.weights,
        held_out_features,
    )

    head_scorer, head_fit = _fit_head_only_probe(
        source_scorer,
        active_bridge.semantic_schema,
        training,
        held_out,
        limits,
        steps=config.head_fit_steps,
        learning_rate=config.head_fit_learning_rate,
        model_seed=config.model_seed,
    )
    require_matching_actor_state(source_scorer, head_scorer)

    result = {
        "schema": RUN_CRITIC_PROBE_SCHEMA,
        "terminal_return": {"metric": training.return_config.metric,
                            "target_floor": training.return_config.target_floor},
        "published_value_prediction_target": target,
        "kind": "completed",
        "behavior": str(config.behavior),
        "behavior_manifest_id": recovered.manifest_id.digest.hex(),
        "behavior_checkpoint_id": recovered.checkpoint_id.digest.hex(),
        "behavior_training_step": recovered.training_step,
        "ascension_level": config.ascension_level,
        "combat_potion_lane": potion_lane.value,
        "behavior_seed": config.behavior_seed,
        "held_out_seed_start": config.held_out_seed_start,
        "held_out_seed_end": collected.evaluation.schedule_end.next_candidate,
        "train_attempts": config.train_attempts,
        "held_out_attempts": config.held_out_attempts,
        "train_decisions": len(training.rows),
        "held_out_decisions": len(held_out.rows),
        "terminal_floor_counts": (
            collected.evaluation.run.summary.terminal_progress.floor_counts
        ),
        "victories": collected.evaluation.run.summary.victories,
        "defeats": collected.evaluation.run.summary.defeats,
        "public_linear_features": feature_names,
        "actor_tensors_unchanged": True,
        "head_fit_steps": config.head_fit_steps,
        "head_fit_learning_rate": config.head_fit_learning_rate,
        "probes": {
            "constant": _probe_pair(
                training,
                constant_train,
                held_out,
                constant_held_out,
            ),
            "public_linear": _probe_pair(
                training,
                linear_train,
                held_out,
                linear_held_out,
            ),
            "published_critic": _probe_pair(
                training,
                published_train,
                held_out,
                published_held_out,
            ),
            "head_only_fit": {
                **_probe_pair(
                    training,
                    head_fit[0],
                    held_out,
                    head_fit[1],
                ),
                "initial_train_loss": head_fit[2],
                "final_train_loss": head_fit[3],
            },
        },
    }
    config.output.mkdir(parents=True, exist_ok=True)
    with (config.output / "challenge.json").open(
        "x",
        encoding="utf-8",
        newline="\n",
    ) as destination:
        json.dump(result, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    held = result["probes"]
    assert isinstance(held, dict)
    print(
        "run_critic_probe_complete=true "
        f"attempts={config.total_attempts} "
        f"decisions={len(training.rows) + len(held_out.rows)} "
        f"victories={result['victories']} defeats={result['defeats']} "
        f"constant_ev={_held_out_ev(held['constant'])} "
        f"public_linear_ev={_held_out_ev(held['public_linear'])} "
        f"published_ev={_held_out_ev(held['published_critic'])} "
        f"head_only_ev={_held_out_ev(held['head_only_fit'])} "
        f"output={config.output}",
        flush=True,
    )
    return result


def _require_scalar_critic(recovered: PublishedRunBehavior) -> None:
    scorer = recovered.policies[0].frozen_scorer
    if not scorer.config.value_head or scorer.config.value_head_width != 1:
        raise RunCriticProbeError(
            "run critic probe requires a publication with one scalar value head"
        )


def _build_dataset(
    attempts: Sequence[PublicAttemptTrajectoryV1],
    return_config: FloorProgressReturnConfig = FloorProgressReturnConfig(),
) -> _ProbeDataset:
    normalized = tuple(attempts)
    if not normalized:
        raise RunCriticProbeError("critic probe dataset must contain attempts")
    rollout = build_complete_run_rollout(normalized, return_config)
    rows: list[_ProbeRow] = []
    for attempt_index, attempt in enumerate(normalized):
        for decision_index, decision in enumerate(attempt.decisions):
            if decision.run_progress.is_combat:
                continue
            context = decision.run_progress.strategic_context_kind
            if context is None:
                raise RunCriticProbeError(
                    "strategic critic row lost its typed context"
                )
            rows.append(
                _ProbeRow(
                    attempt_index=attempt_index,
                    floor=decision.run_progress.floor,
                    context_kind=context,
                    target=rollout.attempts[attempt_index]
                    .rows[decision_index]
                    .return_to_go,
                    decision=decision,
                )
            )
    if not rows:
        raise RunCriticProbeError(
            "critic probe dataset contains no strategic decisions"
        )
    if len({row.attempt_index for row in rows}) != len(normalized):
        raise RunCriticProbeError(
            "every critic probe attempt must contain a strategic decision"
        )
    return _ProbeDataset(attempts=normalized, rows=tuple(rows), return_config=return_config)


def _score_scorer(
    scorer: RaggedCandidateScorer,
    dataset: _ProbeDataset,
    limits: CategoricalSessionLimits,
    *, value_prediction_target: str = "return_to_go",
) -> np.ndarray:
    combined = concatenate_semantic_decision_batches(
        dataset.payloads,
        limits.concat,
    )
    with torch.no_grad():
        values = scorer.actor_critic(combined).row_values
        if value_prediction_target == "terminal_score":
            values = values - torch.as_tensor([decision_progress_potential(
                row.decision.run_progress, dataset.return_config) for row in dataset.rows],
                dtype=values.dtype, device=values.device)
        elif value_prediction_target != "return_to_go":
            raise RunCriticProbeError("unknown published value prediction target")
    return values.detach().cpu().numpy().astype(np.float64, copy=False)


def _fit_head_only_probe(
    source: RaggedCandidateScorer,
    schema: Mapping[str, object],
    training: _ProbeDataset,
    held_out: _ProbeDataset,
    limits: CategoricalSessionLimits,
    *,
    steps: int,
    learning_rate: float,
    model_seed: int,
) -> tuple[RaggedCandidateScorer, tuple[np.ndarray, np.ndarray, float, float]]:
    device = source.token_kind.weight.device
    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(model_seed)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            schema,
            source.config,
        ).to(device)
    load_scorer_warm_start(scorer, source, actor_only=True)
    require_matching_actor_state(source, scorer)
    configure_critic_only_training(scorer)
    parameters = tuple(
        parameter for parameter in scorer.parameters() if parameter.requires_grad
    )
    optimizer = torch.optim.Adam(parameters, lr=learning_rate, foreach=False)
    combined = concatenate_semantic_decision_batches(
        training.payloads,
        limits.concat,
    )
    targets = torch.as_tensor(
        training.targets,
        dtype=scorer.token_kind.weight.dtype,
        device=device,
    )
    weights = torch.as_tensor(
        training.weights,
        dtype=targets.dtype,
        device=device,
    )
    initial_loss: float | None = None
    for _step in range(steps):
        predictions = scorer.actor_critic(combined).row_values
        loss = 0.5 * torch.sum((predictions - targets).square() * weights)
        if initial_loss is None:
            initial_loss = float(loss.detach().item())
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        optimizer.step()
    with torch.no_grad():
        final_predictions = scorer.actor_critic(combined).row_values
        final_loss = float(
            (0.5 * torch.sum((final_predictions - targets).square() * weights))
            .detach()
            .item()
        )
    require_matching_actor_state(source, scorer)
    return scorer, (
        _score_scorer(scorer, training, limits),
        _score_scorer(scorer, held_out, limits),
        float(initial_loss if initial_loss is not None else math.nan),
        final_loss,
    )


def _public_feature_matrices(
    training: _ProbeDataset,
    held_out: _ProbeDataset,
    schema: Mapping[str, object],
) -> tuple[tuple[str, ...], np.ndarray, np.ndarray]:
    context_schema = _schema_mapping(schema, "context_kind")
    context_ids = tuple(sorted({_schema_id(value) for value in context_schema.values()}))
    numeric_names = (
        "act",
        "floor",
        "hp_ratio",
        "gold",
        "deck_size",
        "relic_count",
        "potion_slots",
        "map_nodes",
        "candidate_count",
    )
    names = numeric_names + tuple(f"context_{value}" for value in context_ids)
    train_raw = np.asarray(
        [_public_feature_row(row, schema, context_ids) for row in training.rows],
        dtype=np.float64,
    )
    held_raw = np.asarray(
        [_public_feature_row(row, schema, context_ids) for row in held_out.rows],
        dtype=np.float64,
    )
    numeric_count = len(numeric_names)
    mean = np.sum(
        train_raw[:, :numeric_count] * training.weights[:, None],
        axis=0,
    )
    variance = np.sum(
        np.square(train_raw[:, :numeric_count] - mean)
        * training.weights[:, None],
        axis=0,
    )
    scale = np.sqrt(np.maximum(variance, 1e-12))
    train_raw[:, :numeric_count] = (
        train_raw[:, :numeric_count] - mean
    ) / scale
    held_raw[:, :numeric_count] = (
        held_raw[:, :numeric_count] - mean
    ) / scale
    return names, train_raw, held_raw


def _public_feature_row(
    row: _ProbeRow,
    schema: Mapping[str, object],
    context_ids: tuple[int, ...],
) -> tuple[float, ...]:
    payload = row.decision.semantic_payload
    semantic = _mapping(payload.get("semantic"), "semantic")
    token = _mapping(semantic.get("token"), "semantic.token")
    token_kind = np.asarray(token.get("kind"))
    scalar = _mapping(semantic.get("scalar"), "semantic.scalar")
    scalar_fields = np.asarray(scalar.get("field"))
    scalar_values = np.asarray(scalar.get("value"), dtype=np.float64)
    categorical = _mapping(semantic.get("categorical"), "semantic.categorical")
    categorical_fields = np.asarray(categorical.get("field"))
    categorical_values = np.asarray(categorical.get("value"))
    scalar_schema = _schema_mapping(schema, "scalar_field")
    categorical_schema = _schema_mapping(schema, "categorical_field")
    token_schema = _schema_mapping(schema, "token_kind")

    act = _one_integral_scalar(
        scalar_fields,
        scalar_values,
        _named_schema_id(scalar_schema, "Act"),
        "Act",
    )
    floor = _one_integral_scalar(
        scalar_fields,
        scalar_values,
        _named_schema_id(scalar_schema, "Floor"),
        "Floor",
    )
    context_kind = _one_categorical(
        categorical_fields,
        categorical_values,
        _named_schema_id(categorical_schema, "ContextKind"),
        "ContextKind",
    )
    progress = row.decision.run_progress
    if (
        act != progress.act
        or floor != progress.floor
        or context_kind != row.context_kind
    ):
        raise RunCriticProbeError(
            "public model features disagree with decision progress provenance"
        )
    current_hp = _one_scalar(
        scalar_fields,
        scalar_values,
        _named_schema_id(scalar_schema, "CurrentHp"),
        "CurrentHp",
    )
    max_hp = _one_scalar(
        scalar_fields,
        scalar_values,
        _named_schema_id(scalar_schema, "MaxHp"),
        "MaxHp",
    )
    gold = _one_scalar(
        scalar_fields,
        scalar_values,
        _named_schema_id(scalar_schema, "Gold"),
        "Gold",
    )
    counts = tuple(
        float(np.count_nonzero(token_kind == _named_schema_id(token_schema, name)))
        for name in ("Card", "Relic", "PotionSlot", "MapNode")
    )
    candidates = np.asarray(payload.get("candidate_counts"))
    if candidates.shape != (1,):
        raise RunCriticProbeError(
            "critic probe public payload must contain one candidate count"
        )
    return (
        float(act),
        float(floor),
        current_hp / max(max_hp, 1.0),
        gold,
        *counts,
        float(candidates[0]),
        *(float(context_kind == value) for value in context_ids),
    )


def _fit_weighted_ridge(
    train_features: np.ndarray,
    train_targets: np.ndarray,
    train_weights: np.ndarray,
    held_out_features: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    train_design = np.column_stack(
        (np.ones(train_features.shape[0], dtype=np.float64), train_features)
    )
    held_design = np.column_stack(
        (np.ones(held_out_features.shape[0], dtype=np.float64), held_out_features)
    )
    weighted = train_design * train_weights[:, None]
    penalty = np.eye(train_design.shape[1], dtype=np.float64) * 1e-3
    penalty[0, 0] = 0.0
    coefficients = np.linalg.pinv(train_design.T @ weighted + penalty) @ (
        train_design.T @ (train_weights * train_targets)
    )
    return train_design @ coefficients, held_design @ coefficients


def _probe_pair(
    training: _ProbeDataset,
    train_predictions: np.ndarray,
    held_out: _ProbeDataset,
    held_out_predictions: np.ndarray,
) -> dict[str, object]:
    return {
        "train": _metrics(training, train_predictions),
        "held_out": _metrics(held_out, held_out_predictions),
    }


def _metrics(
    dataset: _ProbeDataset,
    predictions: np.ndarray,
) -> dict[str, object]:
    targets = dataset.targets
    weights = dataset.weights
    values = np.asarray(predictions, dtype=np.float64)
    if values.shape != targets.shape or not np.all(np.isfinite(values)):
        raise RunCriticProbeError("critic probe predictions are misaligned or non-finite")
    target_mean = _weighted_mean(targets, weights)
    prediction_mean = _weighted_mean(values, weights)
    target_variance = _weighted_mean((targets - target_mean) ** 2, weights)
    prediction_variance = _weighted_mean(
        (values - prediction_mean) ** 2,
        weights,
    )
    residuals = targets - values
    residual_mean = _weighted_mean(residuals, weights)
    residual_variance = _weighted_mean(
        (residuals - residual_mean) ** 2,
        weights,
    )
    mse = _weighted_mean(residuals**2, weights)
    explained_variance = (
        None
        if target_variance <= 1e-12
        else 1.0 - residual_variance / target_variance
    )
    attempt_mses = []
    for attempt_index in sorted({row.attempt_index for row in dataset.rows}):
        selected = np.asarray(
            [row.attempt_index == attempt_index for row in dataset.rows],
            dtype=bool,
        )
        attempt_mses.append(float(np.mean(residuals[selected] ** 2)))
    return {
        "attempts": len(attempt_mses),
        "decisions": len(dataset.rows),
        "target_mean": target_mean,
        "target_standard_deviation": math.sqrt(target_variance),
        "prediction_mean": prediction_mean,
        "prediction_standard_deviation": math.sqrt(prediction_variance),
        "prediction_to_target_sd_ratio": (
            None
            if target_variance <= 1e-12
            else math.sqrt(prediction_variance / target_variance)
        ),
        "residual_mean": residual_mean,
        "residual_mse": mse,
        "explained_variance": explained_variance,
        "attempt_mse_mean": float(np.mean(attempt_mses)),
        "attempt_mse_median": float(np.median(attempt_mses)),
        "matched_floor_context_concordance": _matched_concordance(
            dataset,
            values,
        ),
    }


def _matched_concordance(
    dataset: _ProbeDataset,
    predictions: np.ndarray,
) -> dict[str, int | float | None]:
    grouped: dict[tuple[int, int, int], tuple[float, float]] = {}
    grouped_indices: dict[tuple[int, int, int], list[int]] = {}
    for index, row in enumerate(dataset.rows):
        grouped_indices.setdefault(
            (row.attempt_index, row.floor, row.context_kind),
            [],
        ).append(index)
    for key, indices in grouped_indices.items():
        grouped[key] = (
            float(np.mean([dataset.rows[index].target for index in indices])),
            float(np.mean(predictions[indices])),
        )

    by_attempt: dict[int, dict[tuple[int, int], tuple[float, float]]] = {}
    for (attempt, floor, context), values in grouped.items():
        by_attempt.setdefault(attempt, {})[(floor, context)] = values

    comparable_groups = 0
    concordant = 0
    discordant = 0
    tied_prediction = 0
    attempt_pair_rates: list[float] = []
    attempts = sorted(by_attempt)
    for left_offset, left_attempt in enumerate(attempts):
        for right_attempt in attempts[left_offset + 1 :]:
            left_groups = by_attempt[left_attempt]
            right_groups = by_attempt[right_attempt]
            pair_scores: list[float] = []
            for key in sorted(set(left_groups).intersection(right_groups)):
                left_target, left_prediction = left_groups[key]
                right_target, right_prediction = right_groups[key]
                target_delta = left_target - right_target
                if abs(target_delta) <= 1e-12:
                    continue
                prediction_delta = left_prediction - right_prediction
                comparable_groups += 1
                if abs(prediction_delta) <= 1e-12:
                    tied_prediction += 1
                    pair_scores.append(0.5)
                elif target_delta * prediction_delta > 0.0:
                    concordant += 1
                    pair_scores.append(1.0)
                else:
                    discordant += 1
                    pair_scores.append(0.0)
            if pair_scores:
                attempt_pair_rates.append(float(np.mean(pair_scores)))
    return {
        "aggregated_attempt_floor_context_groups": len(grouped),
        "comparable_attempt_pairs": len(attempt_pair_rates),
        "comparable_group_pairs": comparable_groups,
        "concordant": concordant,
        "discordant": discordant,
        "tied_prediction": tied_prediction,
        "rate": (
            None
            if not attempt_pair_rates
            else float(np.mean(attempt_pair_rates))
        ),
        "pooled_rate": (
            None
            if comparable_groups == 0
            else (concordant + 0.5 * tied_prediction) / comparable_groups
        ),
        "non_tie_rate": (
            None
            if concordant + discordant == 0
            else concordant / (concordant + discordant)
        ),
    }


def _weighted_mean(values: np.ndarray, weights: np.ndarray) -> float:
    return float(np.sum(values * weights) / np.sum(weights))


def _one_scalar(
    fields: np.ndarray,
    values: np.ndarray,
    field_id: int,
    name: str,
) -> float:
    selected = values[fields == field_id]
    if selected.shape != (1,):
        raise RunCriticProbeError(
            f"strategic public payload must contain one {name} scalar"
        )
    return float(selected[0])


def _one_integral_scalar(
    fields: np.ndarray,
    values: np.ndarray,
    field_id: int,
    name: str,
) -> int:
    value = _one_scalar(fields, values, field_id, name)
    if not math.isfinite(value) or not value.is_integer() or value < 0.0:
        raise RunCriticProbeError(
            f"strategic public payload {name} must be a nonnegative integer"
        )
    return int(value)


def _one_categorical(
    fields: np.ndarray,
    values: np.ndarray,
    field_id: int,
    name: str,
) -> int:
    selected = values[fields == field_id]
    if selected.shape != (1,):
        raise RunCriticProbeError(
            f"strategic public payload must contain one {name} categorical"
        )
    return _integer(selected[0], name, minimum=0)


def _schema_mapping(schema: Mapping[str, object], name: str) -> Mapping[str, object]:
    return _mapping(schema.get(name), f"semantic schema {name}")


def _named_schema_id(schema: Mapping[str, object], name: str) -> int:
    try:
        return _schema_id(schema[name])
    except KeyError as error:
        raise RunCriticProbeError(
            f"semantic schema has no {name!r} entry"
        ) from error


def _schema_id(value: object) -> int:
    return _integer(value, "semantic schema id", minimum=0)


def _mapping(value: object, name: str) -> Mapping[str, object]:
    if not isinstance(value, Mapping):
        raise RunCriticProbeError(f"{name} must be a mapping")
    return value


def _held_out_ev(probe: object) -> str:
    if not isinstance(probe, Mapping):
        return "none"
    held_out = probe.get("held_out")
    if not isinstance(held_out, Mapping):
        return "none"
    value = held_out.get("explained_variance")
    return "none" if value is None else f"{float(value):.6f}"


def _integer(value: object, name: str, *, minimum: int) -> int:
    if isinstance(value, bool):
        raise RunCriticProbeError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise RunCriticProbeError(f"{name} must be an integer") from error
    if normalized < minimum:
        raise RunCriticProbeError(f"{name} must be at least {minimum}")
    return normalized


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Collect one actor-neutral fixed configured-ascension cohort and "
            "compare run-critic "
            "state discrimination without publishing a behavior."
        )
    )
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--ascension", type=int, choices=range(21), required=True)
    parser.add_argument("--train-attempts", type=int, default=24)
    parser.add_argument("--held-out-attempts", type=int, default=8)
    parser.add_argument("--max-batch-steps", type=int, default=32_768)
    parser.add_argument("--behavior-seed", type=int, default=10_000)
    parser.add_argument("--held-out-seed-start", type=int, default=1_000_000)
    parser.add_argument("--head-fit-steps", type=int, default=256)
    parser.add_argument("--head-fit-learning-rate", type=float, default=1e-3)
    parser.add_argument("--model-seed", type=int, default=0)
    parser.add_argument(
        "--potion-lane",
        choices=tuple(lane.value for lane in RunPotionLane),
        default=RunPotionLane.TRAINED.value,
    )
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    run_run_critic_probe(
        RunCriticProbeConfig(
            behavior=arguments.behavior,
            output=arguments.output,
            ascension_level=arguments.ascension,
            train_attempts=arguments.train_attempts,
            held_out_attempts=arguments.held_out_attempts,
            max_batch_steps=arguments.max_batch_steps,
            behavior_seed=arguments.behavior_seed,
            held_out_seed_start=arguments.held_out_seed_start,
            head_fit_steps=arguments.head_fit_steps,
            head_fit_learning_rate=arguments.head_fit_learning_rate,
            model_seed=arguments.model_seed,
            potion_lane=RunPotionLane(arguments.potion_lane),
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
