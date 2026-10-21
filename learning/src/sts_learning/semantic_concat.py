"""Bounded concatenation of validated bridge-owned semantic decision rows."""

from __future__ import annotations

import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass

import numpy as np

from .semantic_batch import SemanticBatchError, select_semantic_decision_rows


@dataclass(frozen=True)
class SemanticBatchConcatLimits:
    max_rows: int
    max_input_array_bytes: int

    def __post_init__(self) -> None:
        object.__setattr__(self, "max_rows", _positive(self.max_rows, "max_rows"))
        object.__setattr__(
            self,
            "max_input_array_bytes",
            _positive(self.max_input_array_bytes, "max_input_array_bytes"),
        )

    @property
    def maximum_additional_array_bytes(self) -> int:
        """Conservative cap for validation copies, output, and split temporaries."""

        return 3 * self.max_input_array_bytes


@dataclass(frozen=True)
class SemanticBatchConcatChunk:
    """One contiguous input span that fits one semantic materialization."""

    start_batch_index: int
    stop_batch_index: int
    row_count: int
    input_array_bytes: int


def plan_semantic_decision_batch_chunks(
    decision_batches: Sequence[Mapping[str, object]],
    limits: SemanticBatchConcatLimits,
) -> tuple[SemanticBatchConcatChunk, ...]:
    """Partition compatible batches without weakening per-materialization bounds."""

    if not isinstance(limits, SemanticBatchConcatLimits):
        raise SemanticBatchError("semantic concatenation limits must be typed")
    try:
        batches = tuple(decision_batches)
    except TypeError as error:
        raise SemanticBatchError("decision_batches must be a sequence") from error
    if not batches:
        raise SemanticBatchError("at least one semantic decision batch is required")

    chunks: list[SemanticBatchConcatChunk] = []
    chunk_start = 0
    chunk_rows = 0
    chunk_bytes = 0
    compatibility_signature: tuple[object, ...] | None = None
    for batch_index, batch in enumerate(batches):
        if not isinstance(batch, Mapping):
            raise SemanticBatchError("each semantic decision batch must be a mapping")
        slots = batch.get("slot_indices")
        if not isinstance(slots, np.ndarray) or slots.ndim != 1:
            raise SemanticBatchError("decision_batch.slot_indices must be a vector")
        batch_rows = int(slots.size)
        batch_bytes = _array_bytes(batch, set())
        if batch_rows > limits.max_rows:
            raise SemanticBatchError(
                "one semantic decision batch exceeds its row limit"
            )
        if batch_bytes > limits.max_input_array_bytes:
            raise SemanticBatchError(
                "one semantic decision batch exceeds its input array byte limit"
            )

        signature = _compatibility_signature(batch)
        if compatibility_signature is None:
            compatibility_signature = signature
        elif signature != compatibility_signature:
            raise SemanticBatchError("semantic batches use incompatible schemas")

        if chunk_rows and (
            chunk_rows + batch_rows > limits.max_rows
            or chunk_bytes + batch_bytes > limits.max_input_array_bytes
        ):
            chunks.append(
                SemanticBatchConcatChunk(
                    start_batch_index=chunk_start,
                    stop_batch_index=batch_index,
                    row_count=chunk_rows,
                    input_array_bytes=chunk_bytes,
                )
            )
            chunk_start = batch_index
            chunk_rows = 0
            chunk_bytes = 0
        chunk_rows += batch_rows
        chunk_bytes += batch_bytes

    chunks.append(
        SemanticBatchConcatChunk(
            start_batch_index=chunk_start,
            stop_batch_index=len(batches),
            row_count=chunk_rows,
            input_array_bytes=chunk_bytes,
        )
    )
    return tuple(chunks)


def concatenate_semantic_decision_batches(
    decision_batches: Sequence[Mapping[str, object]],
    limits: SemanticBatchConcatLimits,
) -> dict[str, object]:
    """Validate, copy, and combine rows without interpreting feature ids."""

    if not isinstance(limits, SemanticBatchConcatLimits):
        raise SemanticBatchError("semantic concatenation limits must be typed")
    try:
        batches = tuple(decision_batches)
    except TypeError as error:
        raise SemanticBatchError("decision_batches must be a sequence") from error
    if not batches:
        raise SemanticBatchError("at least one semantic decision batch is required")

    total_rows = 0
    total_array_bytes = 0
    normalized: list[dict[str, object]] = []
    for batch in batches:
        if not isinstance(batch, Mapping):
            raise SemanticBatchError("each semantic decision batch must be a mapping")
        slots = batch.get("slot_indices")
        if not isinstance(slots, np.ndarray) or slots.ndim != 1:
            raise SemanticBatchError("decision_batch.slot_indices must be a vector")
        total_rows += slots.size
        if total_rows > limits.max_rows:
            raise SemanticBatchError("semantic concatenation exceeds its row limit")
        total_array_bytes += _array_bytes(batch, set())
        if total_array_bytes > limits.max_input_array_bytes:
            raise SemanticBatchError(
                "semantic concatenation exceeds its input array byte limit"
            )
        normalized.append(
            select_semantic_decision_rows(batch, tuple(range(slots.size)))
        )

    semantic_tables = [_mapping(batch["semantic"], "semantic") for batch in normalized]
    schema_versions = tuple(
        _non_negative(table["schema_version"], "semantic.schema_version")
        for table in semantic_tables
    )
    schema_version = schema_versions[0]
    if any(version != schema_version for version in schema_versions[1:]):
        raise SemanticBatchError("semantic batches use different schema versions")

    dense_presence = tuple("dense_action_mask" in batch for batch in normalized)
    if any(present != dense_presence[0] for present in dense_presence[1:]):
        raise SemanticBatchError("semantic batches disagree on dense_action_mask")

    token_counts = [
        _array(_mapping(table["token"], "semantic.token")["kind"], "token kind").size
        for table in semantic_tables
    ]
    token_offsets = _prefix_offsets(token_counts)

    result: dict[str, object] = {
        "slot_indices": _concat_column(normalized, "slot_indices"),
        "phase": _concat_column(normalized, "phase"),
        "candidate_counts": _concat_column(normalized, "candidate_counts"),
        "candidate_row_splits": _concat_splits(
            [_array(batch["candidate_row_splits"], "candidate_row_splits") for batch in normalized],
            "candidate_row_splits",
        ),
        "semantic": {
            "schema_version": schema_version,
            "completeness": _concat_nested_column(
                semantic_tables,
                "completeness",
                "semantic.completeness",
            ),
            "token": {
                "row_splits": _concat_splits(
                    [
                        _array(
                            _mapping(table["token"], "semantic.token")["row_splits"],
                            "semantic.token.row_splits",
                        )
                        for table in semantic_tables
                    ],
                    "semantic.token.row_splits",
                ),
                "kind": _concat_nested_mapping_column(
                    semantic_tables,
                    "token",
                    "kind",
                    "semantic.token.kind",
                ),
                "scope": _concat_nested_mapping_column(
                    semantic_tables,
                    "token",
                    "scope",
                    "semantic.token.scope",
                ),
            },
            "categorical": _concat_parallel_table(
                semantic_tables,
                "categorical",
                token_offsets,
            ),
            "scalar": _concat_parallel_table(
                semantic_tables,
                "scalar",
                token_offsets,
            ),
            "relation": _concat_relation_table(semantic_tables, token_offsets),
            "candidate_token_indices": _concat_offset_column(
                [
                    _array(table["candidate_token_indices"], "candidate_token_indices")
                    for table in semantic_tables
                ],
                token_offsets,
                "semantic.candidate_token_indices",
            ),
        },
    }
    if dense_presence[0]:
        masks = [
            _array(batch["dense_action_mask"], "dense_action_mask", ndim=2)
            for batch in normalized
        ]
        widths = {mask.shape[1] for mask in masks}
        if len(widths) != 1:
            raise SemanticBatchError("dense_action_mask widths do not match")
        result["dense_action_mask"] = _concatenate(masks, "dense_action_mask", axis=0)
    return result


def _concat_parallel_table(
    semantic_tables: list[Mapping[object, object]],
    table_name: str,
    token_offsets: tuple[int, ...],
) -> dict[str, np.ndarray]:
    tables = [
        _mapping(table[table_name], f"semantic.{table_name}")
        for table in semantic_tables
    ]
    return {
        "token_indices": _concat_offset_column(
            [_array(table["token_indices"], "token_indices") for table in tables],
            token_offsets,
            f"semantic.{table_name}.token_indices",
        ),
        "field": _concatenate(
            [_array(table["field"], "field") for table in tables],
            f"semantic.{table_name}.field",
        ),
        "value": _concatenate(
            [_array(table["value"], "value") for table in tables],
            f"semantic.{table_name}.value",
        ),
    }


def _concat_relation_table(
    semantic_tables: list[Mapping[object, object]],
    token_offsets: tuple[int, ...],
) -> dict[str, np.ndarray]:
    tables = [
        _mapping(table["relation"], "semantic.relation")
        for table in semantic_tables
    ]
    return {
        "source_token_indices": _concat_offset_column(
            [_array(table["source_token_indices"], "source") for table in tables],
            token_offsets,
            "semantic.relation.source_token_indices",
        ),
        "relation": _concatenate(
            [_array(table["relation"], "relation") for table in tables],
            "semantic.relation.relation",
        ),
        "target_token_indices": _concat_offset_column(
            [_array(table["target_token_indices"], "target") for table in tables],
            token_offsets,
            "semantic.relation.target_token_indices",
        ),
    }


def _concat_column(batches: list[dict[str, object]], key: str) -> np.ndarray:
    return _concatenate([_array(batch[key], key) for batch in batches], key)


def _concat_nested_column(
    mappings: list[Mapping[object, object]],
    key: str,
    name: str,
) -> np.ndarray:
    return _concatenate([_array(mapping[key], name) for mapping in mappings], name)


def _concat_nested_mapping_column(
    mappings: list[Mapping[object, object]],
    nested: str,
    key: str,
    name: str,
) -> np.ndarray:
    return _concatenate(
        [_array(_mapping(mapping[nested], nested)[key], name) for mapping in mappings],
        name,
    )


def _concat_splits(parts: list[np.ndarray], name: str) -> np.ndarray:
    _same_dtype(parts, name)
    lengths = [np.diff(part) for part in parts]
    flat_lengths = _concatenate(lengths, name)
    dtype = parts[0].dtype
    total = sum(int(length) for length in flat_lengths)
    _require_representable(total, dtype, name)
    result = np.empty(flat_lengths.size + 1, dtype=dtype)
    result[0] = 0
    if flat_lengths.size:
        np.cumsum(flat_lengths, dtype=dtype, out=result[1:])
    return result


def _concat_offset_column(
    parts: list[np.ndarray],
    offsets: tuple[int, ...],
    name: str,
) -> np.ndarray:
    _same_dtype(parts, name)
    result = _concatenate(parts, name)
    position = 0
    for part, offset in zip(parts, offsets, strict=True):
        if part.size:
            _require_representable(int(part.max()) + offset, part.dtype, name)
            end = position + part.size
            result[position:end] += np.asarray(offset, dtype=part.dtype)
            position = end
    return result


def _concatenate(parts: list[np.ndarray], name: str, *, axis: int = 0) -> np.ndarray:
    _same_dtype(parts, name)
    if not parts:
        raise SemanticBatchError(f"{name} has no arrays to concatenate")
    return np.concatenate(parts, axis=axis)


def _same_dtype(parts: list[np.ndarray], name: str) -> None:
    if not parts or len({part.dtype for part in parts}) != 1:
        raise SemanticBatchError(f"{name} dtypes do not match")


def _prefix_offsets(lengths: list[int]) -> tuple[int, ...]:
    offsets = []
    total = 0
    for length in lengths:
        offsets.append(total)
        total += length
    return tuple(offsets)


def _require_representable(value: int, dtype: np.dtype[object], name: str) -> None:
    if not np.issubdtype(dtype, np.integer):
        raise SemanticBatchError(f"{name} must use an integer dtype")
    bounds = np.iinfo(dtype)
    if not bounds.min <= value <= bounds.max:
        raise SemanticBatchError(f"{name} overflows its integer dtype")


def _array(value: object, name: str, *, ndim: int = 1) -> np.ndarray:
    if not isinstance(value, np.ndarray) or value.ndim != ndim:
        raise SemanticBatchError(f"{name} must be a {ndim}-dimensional NumPy array")
    return value


def _mapping(value: object, name: str) -> Mapping[object, object]:
    if not isinstance(value, Mapping):
        raise SemanticBatchError(f"{name} must be a mapping")
    return value


def _compatibility_signature(batch: Mapping[str, object]) -> tuple[object, ...]:
    """Return the fields that one combined semantic model batch must share."""

    semantic = _mapping(batch.get("semantic"), "semantic")
    token = _mapping(semantic.get("token"), "semantic.token")
    categorical = _mapping(
        semantic.get("categorical"),
        "semantic.categorical",
    )
    scalar = _mapping(semantic.get("scalar"), "semantic.scalar")
    relation = _mapping(semantic.get("relation"), "semantic.relation")
    arrays = (
        _array(batch.get("slot_indices"), "slot_indices"),
        _array(batch.get("phase"), "phase"),
        _array(batch.get("candidate_counts"), "candidate_counts"),
        _array(batch.get("candidate_row_splits"), "candidate_row_splits"),
        _array(semantic.get("completeness"), "semantic.completeness"),
        _array(token.get("row_splits"), "semantic.token.row_splits"),
        _array(token.get("kind"), "semantic.token.kind"),
        _array(
            categorical.get("token_indices"),
            "semantic.categorical.token_indices",
        ),
        _array(categorical.get("field"), "semantic.categorical.field"),
        _array(categorical.get("value"), "semantic.categorical.value"),
        _array(scalar.get("token_indices"), "semantic.scalar.token_indices"),
        _array(scalar.get("field"), "semantic.scalar.field"),
        _array(scalar.get("value"), "semantic.scalar.value"),
        _array(
            relation.get("source_token_indices"),
            "semantic.relation.source_token_indices",
        ),
        _array(relation.get("relation"), "semantic.relation.relation"),
        _array(
            relation.get("target_token_indices"),
            "semantic.relation.target_token_indices",
        ),
        _array(
            semantic.get("candidate_token_indices"),
            "semantic.candidate_token_indices",
        ),
    )
    dense_mask = batch.get("dense_action_mask")
    if dense_mask is None:
        dense_signature: tuple[object, ...] = (False,)
    else:
        dense = _array(dense_mask, "dense_action_mask", ndim=2)
        dense_signature = (True, dense.dtype.str, dense.shape[1])
    return (
        _non_negative(semantic.get("schema_version"), "semantic.schema_version"),
        tuple(array.dtype.str for array in arrays),
        dense_signature,
    )


def _array_bytes(value: object, seen_mappings: set[int]) -> int:
    if isinstance(value, np.ndarray):
        return value.nbytes
    if isinstance(value, Mapping):
        identity = id(value)
        if identity in seen_mappings:
            raise SemanticBatchError("semantic batch contains a mapping cycle")
        seen_mappings.add(identity)
        try:
            return sum(_array_bytes(item, seen_mappings) for item in value.values())
        finally:
            seen_mappings.remove(identity)
    return 0


def _positive(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise SemanticBatchError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise SemanticBatchError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise SemanticBatchError(f"{name} must be positive")
    return normalized


def _non_negative(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise SemanticBatchError(f"{name} must be an integer")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise SemanticBatchError(f"{name} must be an integer") from error
    if normalized < 0:
        raise SemanticBatchError(f"{name} must be non-negative")
    return normalized
