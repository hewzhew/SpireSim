"""Capacity-aware learning for complete-turn prefix proposal recall.

A replay-verified prefix is positive carriage evidence. Other generated
prefixes remain unknown: this module never assigns them terminal or strategy
labels. The objective only asks the verified prefix to cross the finite
retention cutoff when a layer is wider than its retained frontier.
"""

from __future__ import annotations

import math
import operator
from collections.abc import Sequence

import torch
from torch import Tensor

from .torch_policy import RaggedCandidateLogits, TorchPolicyError


def verified_positive_topk_survival_loss(
    logits: RaggedCandidateLogits,
    positive_ordinals: Sequence[int | None],
    retained_counts: Sequence[int],
    *,
    margin: float = 1.0,
) -> Tensor:
    """Require each verified positive to survive its row's finite capacity.

    Rows at or below capacity impose no ordering because every candidate is
    retained already. ``None`` means that the verified turn has already become
    a completed endpoint, so the residual pending layer carries no positive
    constraint. Wider rows compare the positive only with the K-th highest
    unknown score: unknown candidates are not mutually ordered or relabelled
    as losses.
    """

    if not isinstance(logits, RaggedCandidateLogits):
        raise TorchPolicyError("prefix survival loss requires ragged candidate logits")
    normalized_margin = _finite_non_negative_float(margin, "margin")
    if len(positive_ordinals) != logits.row_count:
        raise TorchPolicyError("positive ordinals must align to candidate rows")
    if len(retained_counts) != logits.row_count:
        raise TorchPolicyError("retained counts must align to candidate rows")

    losses: list[Tensor] = []
    splits = logits.row_splits.detach().cpu().tolist()
    for row, (start, end) in enumerate(zip(splits[:-1], splits[1:], strict=True)):
        candidate_count = end - start
        if candidate_count <= 0:
            raise TorchPolicyError("prefix survival rows must contain candidates")
        retained_count = _positive_index(
            retained_counts[row], f"row {row} retained count"
        )
        if retained_count > candidate_count:
            raise TorchPolicyError(
                f"row {row} retained count exceeds its candidate count"
            )
        positive = positive_ordinals[row]
        if positive is None:
            continue
        positive_ordinal = _non_negative_index(positive, f"row {row} positive ordinal")
        if positive_ordinal >= candidate_count:
            raise TorchPolicyError(f"row {row} positive ordinal is out of range")
        if candidate_count <= retained_count:
            continue

        row_values = logits.values[start:end]
        positive_score = row_values[positive_ordinal]
        unknown_scores = torch.cat(
            (row_values[:positive_ordinal], row_values[positive_ordinal + 1 :])
        )
        cutoff = torch.topk(
            unknown_scores,
            k=retained_count,
            largest=True,
            sorted=True,
        ).values[-1]
        losses.append(torch.relu(cutoff + normalized_margin - positive_score))

    if not losses:
        return logits.values.sum() * 0.0
    return torch.stack(losses).mean()


def topk_prefix_reservations(
    logits: RaggedCandidateLogits,
    reservation_counts: Sequence[int],
) -> list[list[int]]:
    """Return stable observation-local model reservations for each row."""

    if not isinstance(logits, RaggedCandidateLogits):
        raise TorchPolicyError("prefix reservations require ragged candidate logits")
    if len(reservation_counts) != logits.row_count:
        raise TorchPolicyError("reservation counts must align to candidate rows")
    reservations: list[list[int]] = []
    splits = logits.row_splits.detach().cpu().tolist()
    for row, (start, end) in enumerate(zip(splits[:-1], splits[1:], strict=True)):
        candidate_count = end - start
        count = _non_negative_index(
            reservation_counts[row], f"row {row} reservation count"
        )
        if count > candidate_count:
            raise TorchPolicyError(
                f"row {row} reservation count exceeds its candidate count"
            )
        if count == 0:
            reservations.append([])
            continue
        # Stable tie handling keeps the lower observation-local ordinal first.
        values = logits.values[start:end].detach().cpu().tolist()
        ordered = sorted(
            range(candidate_count), key=lambda ordinal: (-values[ordinal], ordinal)
        )
        reservations.append(ordered[:count])
    return reservations


def _finite_non_negative_float(value: object, name: str) -> float:
    if isinstance(value, bool):
        raise TorchPolicyError(f"{name} must be a finite non-negative number")
    try:
        normalized = float(value)
    except (TypeError, ValueError) as error:
        raise TorchPolicyError(
            f"{name} must be a finite non-negative number"
        ) from error
    if not math.isfinite(normalized) or normalized < 0.0:
        raise TorchPolicyError(f"{name} must be a finite non-negative number")
    return normalized


def _non_negative_index(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchPolicyError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchPolicyError(f"{name} must be an integer") from error
    if normalized < 0:
        raise TorchPolicyError(f"{name} must be non-negative")
    return normalized


def _positive_index(value: object, name: str) -> int:
    normalized = _non_negative_index(value, name)
    if normalized == 0:
        raise TorchPolicyError(f"{name} must be positive")
    return normalized
