from __future__ import annotations

import pytest
import torch

from learning.tests.semantic_fixtures import semantic_batch_fixture
from sts_learning import (
    complete_turn_completed_endpoint_decision_batch,
    complete_turn_prefix_decision_batch,
)
from sts_learning.complete_turn_prefix_learning import (
    topk_prefix_reservations,
    verified_positive_topk_survival_loss,
)
from sts_learning.semantic_batch import (
    SemanticBatchError,
    select_semantic_decision_rows,
)
from sts_learning.torch_policy import RaggedCandidateLogits, TorchPolicyError


def _logits(
    rows: list[list[float]], *, requires_grad: bool = False
) -> RaggedCandidateLogits:
    values = torch.tensor(
        [value for row in rows for value in row],
        dtype=torch.float32,
        requires_grad=requires_grad,
    )
    splits = [0]
    for row in rows:
        splits.append(splits[-1] + len(row))
    return RaggedCandidateLogits(values, torch.tensor(splits, dtype=torch.long))


def _prefix_layer_batch() -> dict[str, object]:
    batch = select_semantic_decision_rows(semantic_batch_fixture(), (0,))
    batch.update(
        {
            "atomic_depth": 3,
            "max_retained_states": 32,
            "nodes_expanded": 7,
            "nodes_generated": 19,
        }
    )
    return batch


def _completed_endpoint_batch() -> dict[str, object]:
    batch = select_semantic_decision_rows(semantic_batch_fixture(), (0,))
    batch["max_retained_states"] = 24
    return batch


def test_prefix_projection_produces_the_standard_semantic_envelope() -> None:
    projected = complete_turn_prefix_decision_batch(_prefix_layer_batch())

    assert set(projected) == {
        "slot_indices",
        "phase",
        "candidate_counts",
        "candidate_row_splits",
        "semantic",
    }
    assert projected["slot_indices"].tolist() == [4]
    assert projected["phase"].tolist() == [1]


def test_prefix_projection_rejects_unknown_control_metadata() -> None:
    batch = _prefix_layer_batch()
    batch["unreviewed_control_hint"] = 1

    with pytest.raises(SemanticBatchError, match="unsupported fields"):
        complete_turn_prefix_decision_batch(batch)


def test_prefix_projection_rejects_invalid_control_metadata() -> None:
    batch = _prefix_layer_batch()
    batch["max_retained_states"] = 0

    with pytest.raises(SemanticBatchError, match="must be positive"):
        complete_turn_prefix_decision_batch(batch)


def test_completed_endpoint_projection_is_a_standard_semantic_envelope() -> None:
    projected = complete_turn_completed_endpoint_decision_batch(
        _completed_endpoint_batch()
    )

    assert set(projected) == {
        "slot_indices",
        "phase",
        "candidate_counts",
        "candidate_row_splits",
        "semantic",
    }


def test_completed_endpoint_projection_rejects_prefix_control_fields() -> None:
    batch = _completed_endpoint_batch()
    batch["atomic_depth"] = 3

    with pytest.raises(SemanticBatchError, match="unsupported fields"):
        complete_turn_completed_endpoint_decision_batch(batch)


def test_survival_loss_only_crosses_the_finite_retention_cutoff() -> None:
    logits = _logits([[0.2, 0.9, 0.8, 0.7]], requires_grad=True)

    loss = verified_positive_topk_survival_loss(
        logits,
        positive_ordinals=[0],
        retained_counts=[2],
        margin=0.1,
    )
    loss.backward()

    assert loss.item() == pytest.approx(0.7)
    assert logits.values.grad is not None
    assert logits.values.grad.tolist() == pytest.approx([-1.0, 0.0, 1.0, 0.0])


def test_positive_already_above_cutoff_and_rows_below_capacity_are_zero() -> None:
    logits = _logits([[1.0, 0.4, 0.3], [-5.0, 8.0]], requires_grad=True)

    loss = verified_positive_topk_survival_loss(
        logits,
        positive_ordinals=[0, 0],
        retained_counts=[2, 2],
        margin=0.1,
    )
    loss.backward()

    assert loss.item() == 0.0
    assert logits.values.grad is not None
    assert logits.values.grad.tolist() == [0.0] * 5


def test_completed_endpoint_row_adds_no_pending_prefix_constraint() -> None:
    logits = _logits([[4.0, 3.0], [0.0, 9.0, 8.0]])

    loss = verified_positive_topk_survival_loss(
        logits,
        positive_ordinals=[None, 0],
        retained_counts=[1, 1],
        margin=0.5,
    )

    assert loss.item() == pytest.approx(9.5)


def test_reservations_are_topk_with_stable_ordinal_ties() -> None:
    logits = _logits([[0.5, 0.9, 0.9, -1.0], [2.0]])

    assert topk_prefix_reservations(logits, [2, 0]) == [[1, 2], []]


@pytest.mark.parametrize(
    "positives, retained, message",
    [
        ([0], [0], "must be positive"),
        ([3], [1], "out of range"),
        ([0], [4], "exceeds"),
        ([], [1], "align"),
    ],
)
def test_survival_loss_rejects_misaligned_capacity_evidence(
    positives: list[int], retained: list[int], message: str
) -> None:
    with pytest.raises(TorchPolicyError, match=message):
        verified_positive_topk_survival_loss(
            _logits([[0.0, 1.0, 2.0]]),
            positive_ordinals=positives,
            retained_counts=retained,
        )
