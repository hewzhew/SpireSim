"""Validated model-envelope projection for complete-turn prefix layers."""

from __future__ import annotations

import operator
from collections.abc import Mapping

import numpy as np

from .semantic_batch import SemanticBatchError, select_semantic_decision_rows


_DECISION_REQUIRED_FIELDS = {
    "slot_indices",
    "phase",
    "candidate_counts",
    "candidate_row_splits",
    "semantic",
}
_DECISION_OPTIONAL_FIELDS = {
    "dense_action_mask",
    "decision_token_ids",
    "public_snapshot_ids",
    "decision_steps",
}
_PREFIX_LAYER_FIELDS = {
    "atomic_depth",
    "max_retained_states",
    "nodes_expanded",
    "nodes_generated",
}
_COMPLETED_ENDPOINT_LAYER_FIELDS = {"max_retained_states"}


def complete_turn_prefix_decision_batch(
    prefix_layer_batch: Mapping[str, object],
) -> dict[str, object]:
    """Validate a bridge prefix observation and project its model envelope.

    Prefix control metadata remains available to the caller, while the return
    value contains exactly the standard semantic decision fields accepted by
    row selection, concatenation, and candidate scorers. Unknown top-level
    fields are rejected rather than silently discarded.
    """

    decision_batch = _validated_single_row_reservation_batch(
        prefix_layer_batch,
        control_fields=_PREFIX_LAYER_FIELDS,
        context="complete-turn prefix layer",
    )
    _non_negative_index(prefix_layer_batch["atomic_depth"], "atomic_depth")
    _positive_index(
        prefix_layer_batch["max_retained_states"], "max_retained_states"
    )
    _non_negative_index(prefix_layer_batch["nodes_expanded"], "nodes_expanded")
    _non_negative_index(prefix_layer_batch["nodes_generated"], "nodes_generated")

    return decision_batch


def complete_turn_completed_endpoint_decision_batch(
    completed_endpoint_batch: Mapping[str, object],
) -> dict[str, object]:
    """Project a completed-endpoint reservation layer to a scorer envelope.

    These candidates have finished generation but have not yet entered the
    bounded executable surface. Scores therefore reserve capacity; they do not
    select an action for execution.
    """

    decision_batch = _validated_single_row_reservation_batch(
        completed_endpoint_batch,
        control_fields=_COMPLETED_ENDPOINT_LAYER_FIELDS,
        context="complete-turn completed endpoint layer",
    )
    _positive_index(
        completed_endpoint_batch["max_retained_states"], "max_retained_states"
    )
    return decision_batch


def _validated_single_row_reservation_batch(
    batch: Mapping[str, object],
    *,
    control_fields: set[str],
    context: str,
) -> dict[str, object]:
    if not isinstance(batch, Mapping):
        raise SemanticBatchError(f"{context} batch must be a mapping")
    keys = set(batch)
    required = _DECISION_REQUIRED_FIELDS | control_fields
    missing = required - keys
    unsupported = keys - required - _DECISION_OPTIONAL_FIELDS
    if missing:
        raise SemanticBatchError(
            f"{context} batch is missing fields: " + ", ".join(sorted(missing))
        )
    if unsupported:
        raise SemanticBatchError(
            f"{context} batch has unsupported fields: "
            + ", ".join(sorted(map(str, unsupported)))
        )
    slots = batch["slot_indices"]
    if not isinstance(slots, np.ndarray) or slots.shape != (1,):
        raise SemanticBatchError(f"{context} slot_indices must contain exactly one row")
    decision_batch = {
        key: batch[key]
        for key in _DECISION_REQUIRED_FIELDS | _DECISION_OPTIONAL_FIELDS
        if key in batch
    }
    return select_semantic_decision_rows(decision_batch, (0,))


def _non_negative_index(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise SemanticBatchError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise SemanticBatchError(f"{name} must be an integer") from error
    if normalized < 0:
        raise SemanticBatchError(f"{name} must be non-negative")
    return normalized


def _positive_index(value: object, name: str) -> int:
    normalized = _non_negative_index(value, name)
    if normalized == 0:
        raise SemanticBatchError(f"{name} must be positive")
    return normalized
