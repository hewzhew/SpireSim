from __future__ import annotations

import copy

import numpy as np
import pytest

pytest.importorskip("torch")

from learning.tests.semantic_fixtures import semantic_schema_fixture
from learning.tests.torch_outcome_fixtures import (
    completed_attempt_fixture,
    decision_batch_fixture,
    public_attempt_trajectory_fixture,
    with_run_progress_fixture,
)
from sts_learning import BehaviorManifestId, FloorProgressReturnConfig
from sts_learning.run_critic_probe import (
    _ProbeDataset,
    _ProbeRow,
    _build_dataset,
    _fit_head_only_probe,
    _fit_weighted_ridge,
    _matched_concordance,
    _metrics,
    _score_scorer,
)
from sts_learning.torch_policy import (
    RaggedCandidateScorer,
    RaggedScorerConfig,
    require_matching_actor_state,
)
from sts_learning.torch_session_config import CategoricalSessionLimits


def _strategic_attempt(*, slot: int, reward: int, act: int = 1):
    batch = with_run_progress_fixture(
        decision_batch_fixture(
            slot=slot,
            semantic_row=slot % 2,
            selected_ordinal=0,
            manifest_id=BehaviorManifestId(b"\x71" * 32),
        ),
        act=act,
        floor=2 + slot,
        is_combat=False,
        strategic_context_kind=1 + slot % 2,
    )
    return public_attempt_trajectory_fixture(
        completed_attempt_fixture(
            slot=slot,
            batches=(batch,),
            reward=reward,
        )
    )


def test_published_terminal_score_probe_uses_source_return_profile_and_units():
    dataset = _build_dataset((_strategic_attempt(slot=0, reward=1, act=2),),
                             FloorProgressReturnConfig(target_floor=54, metric="act_completion"))
    scorer = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture(),
        RaggedScorerConfig(value_head=True))
    import torch
    with torch.no_grad(): scorer.value_head[-1].bias.fill_(.5)
    values = _score_scorer(scorer, dataset, CategoricalSessionLimits(),
                           value_prediction_target="terminal_score")
    assert values.tolist() == [.25]
    assert dataset.targets.tolist() == [.5]


def test_head_only_probe_reuses_fixed_rows_without_changing_actor() -> None:
    schema = semantic_schema_fixture()
    dataset = _build_dataset(
        (
            _strategic_attempt(slot=0, reward=-1),
            _strategic_attempt(slot=1, reward=1),
        )
    )
    source = RaggedCandidateScorer.from_bridge_schema(
        schema,
        RaggedScorerConfig(value_head=True),
    )
    source_before = copy.deepcopy(source)

    fitted, (train_predictions, _held_out, initial_loss, final_loss) = (
        _fit_head_only_probe(
            source,
            schema,
            dataset,
            dataset,
            CategoricalSessionLimits(),
            steps=32,
            learning_rate=1e-3,
            model_seed=17,
        )
    )

    require_matching_actor_state(source_before, source)
    require_matching_actor_state(source, fitted)
    assert final_loss < initial_loss
    assert final_loss == pytest.approx(
        0.5
        * np.sum(
            np.square(train_predictions - dataset.targets) * dataset.weights
        )
    )
    for name, tensor in source_before.state_dict().items():
        assert np.array_equal(
            tensor.detach().cpu().numpy(),
            source.state_dict()[name].detach().cpu().numpy(),
        )


def test_weighted_public_baseline_and_metrics_use_held_out_rows() -> None:
    train_features = np.asarray([[0.0], [1.0], [2.0]], dtype=np.float64)
    train_targets = np.asarray([-1.0, 0.0, 1.0], dtype=np.float64)
    train_weights = np.asarray([0.25, 0.25, 0.5], dtype=np.float64)
    held_out_features = np.asarray([[3.0], [4.0]], dtype=np.float64)

    train_predictions, held_out_predictions = _fit_weighted_ridge(
        train_features,
        train_targets,
        train_weights,
        held_out_features,
    )

    assert train_predictions.shape == train_targets.shape
    assert held_out_predictions.shape == (2,)
    assert train_predictions[2] > train_predictions[1] > train_predictions[0]

    dataset = _build_dataset(
        (
            _strategic_attempt(slot=0, reward=-1),
            _strategic_attempt(slot=1, reward=1),
        )
    )
    constant = np.full(len(dataset.rows), np.mean(dataset.targets))
    metrics = _metrics(dataset, constant)
    assert metrics["explained_variance"] == pytest.approx(0.0)
    assert metrics["prediction_standard_deviation"] == pytest.approx(0.0)


def test_matched_concordance_aggregates_rows_and_counts_prediction_ties_half() -> None:
    decisions = _build_dataset(
        (
            _strategic_attempt(slot=0, reward=-1),
            _strategic_attempt(slot=1, reward=1),
        )
    ).rows
    dataset = _ProbeDataset(
        attempts=(),
        rows=(
            _ProbeRow(0, 2, 1, 1.0, decisions[0].decision),
            _ProbeRow(0, 2, 1, 1.0, decisions[0].decision),
            _ProbeRow(1, 2, 1, 0.0, decisions[1].decision),
            _ProbeRow(2, 2, 1, -1.0, decisions[1].decision),
        ),
    )

    result = _matched_concordance(
        dataset,
        np.asarray([1.0, 1.0, 0.0, 1.0], dtype=np.float64),
    )

    assert result["aggregated_attempt_floor_context_groups"] == 3
    assert result["comparable_attempt_pairs"] == 3
    assert result["comparable_group_pairs"] == 3
    assert result["concordant"] == 1
    assert result["discordant"] == 1
    assert result["tied_prediction"] == 1
    assert result["rate"] == pytest.approx(0.5)
    assert result["pooled_rate"] == pytest.approx(0.5)

    tied = _matched_concordance(dataset, np.zeros(4, dtype=np.float64))
    assert tied["rate"] == pytest.approx(0.5)
    assert tied["non_tie_rate"] is None
