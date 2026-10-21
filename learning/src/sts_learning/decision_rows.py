"""Policy-neutral capture and validation for bridge semantic decision rows."""

from __future__ import annotations

import operator
import sys
from collections.abc import Iterable, Iterator, Mapping, Sequence
from dataclasses import dataclass
from types import MappingProxyType

import numpy as np

from .policy import BehaviorManifestId, SelectionProbability


class DecisionRowError(ValueError):
    """A semantic decision payload or aligned policy choice is malformed."""


@dataclass(frozen=True)
class PreparedDecisionRows:
    """Policy-independent immutable rows with conservative retained-byte cost."""

    payload: Mapping[str, object]
    slot_indices: tuple[int, ...]
    candidate_counts: tuple[int, ...]
    decision_count: int
    payload_bytes: int

    @classmethod
    def capture(cls, decision_batch: Mapping[str, object]) -> PreparedDecisionRows:
        if not isinstance(decision_batch, Mapping):
            raise DecisionRowError("decision batch must be a mapping")
        slots = _integer_column(decision_batch, "slot_indices")
        counts = _integer_column(decision_batch, "candidate_counts")
        if not slots:
            raise DecisionRowError("decision batch must contain a decision row")
        if len(slots) != len(counts):
            raise DecisionRowError(
                "decision slot and candidate columns are misaligned"
            )
        if len(set(slots)) != len(slots):
            raise DecisionRowError("decision batch contains duplicate slots")
        if any(count <= 0 for count in counts):
            raise DecisionRowError("every decision row must have a legal candidate")
        payload, payload_bytes = freeze_decision_payload(
            decision_batch,
            "decision_batch",
        )
        if not isinstance(payload, Mapping):
            raise AssertionError("frozen decision payload is not a mapping")
        return cls(
            payload=payload,
            slot_indices=slots,
            candidate_counts=counts,
            decision_count=len(slots),
            payload_bytes=payload_bytes,
        )


def normalize_decision_choice(
    prepared: PreparedDecisionRows,
    selected_ordinals: Sequence[int],
    selection_probabilities: Sequence[SelectionProbability],
    behavior_manifest_id: BehaviorManifestId,
) -> tuple[tuple[int, ...], tuple[SelectionProbability, ...]]:
    """Validate one policy choice against the exact rows seen by that policy."""

    if not isinstance(prepared, PreparedDecisionRows):
        raise DecisionRowError("choice requires PreparedDecisionRows")
    ordinals = normalize_integer_sequence(selected_ordinals, "selected ordinals")
    try:
        probabilities = tuple(selection_probabilities)
    except TypeError as error:
        raise DecisionRowError(
            "selection probabilities must be a sequence"
        ) from error
    if not isinstance(behavior_manifest_id, BehaviorManifestId):
        raise DecisionRowError("decision choice requires a BehaviorManifestId")
    if len(ordinals) != prepared.decision_count:
        raise DecisionRowError(
            f"received {len(ordinals)} ordinals for "
            f"{prepared.decision_count} decision rows"
        )
    if len(probabilities) != prepared.decision_count:
        raise DecisionRowError(
            "selection probabilities must contain one value per decision row"
        )
    if not all(
        isinstance(probability, SelectionProbability) for probability in probabilities
    ):
        raise DecisionRowError(
            "selection probabilities must be typed SelectionProbability values"
        )
    for row, (ordinal, count) in enumerate(
        zip(ordinals, prepared.candidate_counts, strict=True)
    ):
        if not 0 <= ordinal < count:
            raise DecisionRowError(
                f"row {row} candidate ordinal {ordinal} is outside 0..{count}"
            )
    return ordinals, probabilities


def freeze_decision_payload(value: object, path: str) -> tuple[object, int]:
    """Recursively copy a supported decision payload and account retained bytes."""

    if isinstance(value, np.ndarray):
        if value.dtype.hasobject:
            raise DecisionRowError(f"{path} contains an object array")
        copied = np.array(value, copy=True, order="C", subok=False)
        copied.setflags(write=False)
        return copied, sys.getsizeof(copied)
    if isinstance(value, Mapping):
        frozen: dict[str, object] = {}
        payload_bytes = 0
        for key, child in value.items():
            if not isinstance(key, str):
                raise DecisionRowError(f"{path} contains a non-string mapping key")
            frozen_child, child_bytes = freeze_decision_payload(child, f"{path}.{key}")
            frozen[key] = frozen_child
            payload_bytes += sys.getsizeof(key) + child_bytes
        proxy = MappingProxyType(frozen)
        payload_bytes += sys.getsizeof(frozen) + sys.getsizeof(proxy)
        return proxy, payload_bytes
    if isinstance(value, str):
        if not value:
            raise DecisionRowError(f"{path} contains an empty string")
        return value, sys.getsizeof(value)
    if isinstance(value, Sequence) and not isinstance(value, (str, bytes)):
        frozen_items = []
        payload_bytes = 0
        for index, child in enumerate(value):
            frozen_child, child_bytes = freeze_decision_payload(
                child,
                f"{path}[{index}]",
            )
            frozen_items.append(frozen_child)
            payload_bytes += child_bytes
        frozen_sequence = tuple(frozen_items)
        return frozen_sequence, payload_bytes + sys.getsizeof(frozen_sequence)
    if isinstance(value, bool):
        return value, sys.getsizeof(value)
    try:
        normalized = operator.index(value)
        return normalized, sys.getsizeof(normalized)
    except TypeError as error:
        raise DecisionRowError(
            f"{path} contains unsupported value {type(value).__name__}"
        ) from error


def iter_payload_arrays(value: object) -> Iterator[np.ndarray]:
    """Yield every NumPy buffer retained by a frozen decision payload."""

    if isinstance(value, np.ndarray):
        yield value
    elif isinstance(value, Mapping):
        for child in value.values():
            yield from iter_payload_arrays(child)


def _integer_column(mapping: Mapping[str, object], name: str) -> tuple[int, ...]:
    try:
        raw = mapping[name]
    except KeyError as error:
        raise DecisionRowError(f"decision batch is missing {name}") from error
    return normalize_integer_sequence(raw, name)


def normalize_integer_sequence(raw: object, name: str) -> tuple[int, ...]:
    """Normalize a non-text iterable without accepting booleans as integers."""

    if isinstance(raw, (str, bytes)) or not isinstance(raw, Iterable):
        raise DecisionRowError(f"{name} must be an iterable of integers")
    normalized = []
    for value in raw:
        if isinstance(value, bool):
            raise DecisionRowError(f"{name} must not contain bool")
        try:
            normalized.append(operator.index(value))
        except TypeError as error:
            raise DecisionRowError(f"{name} must contain only integers") from error
    return tuple(normalized)
