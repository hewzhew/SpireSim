"""Row algebra for the bridge-owned sparse semantic decision batch."""

from __future__ import annotations

import operator
from collections.abc import Mapping, Sequence

import numpy as np


class SemanticBatchError(ValueError):
    """A semantic batch or requested row selection violated the bridge schema."""


def select_semantic_decision_rows(
    decision_batch: Mapping[str, object],
    row_indices: Sequence[int],
) -> dict[str, object]:
    """Copy selected rows and compact every token-indexed semantic table.

    Rows are returned in the requested order. Numeric feature ids remain
    uninterpreted; only the bridge's structural table contract is understood.
    """

    batch = _mapping(decision_batch, "decision_batch")
    _exact_keys(
        batch,
        required={
            "slot_indices",
            "phase",
            "candidate_counts",
            "candidate_row_splits",
            "semantic",
        },
        optional={
            "dense_action_mask",
            "decision_token_ids",
            "public_snapshot_ids",
            "decision_steps",
        },
        name="decision_batch",
    )
    slots = _integer_array(batch, "slot_indices", "decision_batch")
    phases = _integer_array(batch, "phase", "decision_batch")
    candidate_counts = _integer_array(
        batch,
        "candidate_counts",
        "decision_batch",
    )
    row_count = slots.size
    _require_shape(phases, (row_count,), "phase")
    _require_shape(candidate_counts, (row_count,), "candidate_counts")
    tokenized = {
        "decision_token_ids",
        "public_snapshot_ids",
        "decision_steps",
    }.intersection(batch)
    if tokenized and len(tokenized) != 3:
        raise SemanticBatchError("V3 decision lineage columns must appear together")
    decision_steps = None
    decision_token_ids = None
    public_snapshot_ids = None
    if tokenized:
        decision_steps = _integer_array(batch, "decision_steps", "decision_batch")
        _require_shape(decision_steps, (row_count,), "decision_steps")
        decision_token_ids = _string_rows(
            batch,
            "decision_token_ids",
            row_count,
            "decision_batch",
        )
        public_snapshot_ids = _string_rows(
            batch,
            "public_snapshot_ids",
            row_count,
            "decision_batch",
        )
    rows = _normalize_rows(row_indices, row_count)

    candidate_splits = _integer_array(
        batch,
        "candidate_row_splits",
        "decision_batch",
    )
    candidate_bounds = _split_bounds(
        candidate_splits,
        row_count,
        "candidate",
        require_non_empty=True,
    )
    split_counts = tuple(end - start for start, end in candidate_bounds)
    if tuple(map(int, candidate_counts)) != split_counts:
        raise SemanticBatchError(
            "candidate_counts disagree with candidate_row_splits"
        )

    semantic = _mapping(_required(batch, "semantic", "decision_batch"), "semantic")
    _exact_keys(
        semantic,
        required={
            "schema_version",
            "completeness",
            "token",
            "categorical",
            "scalar",
            "relation",
            "candidate_token_indices",
        },
        optional=set(),
        name="semantic",
    )
    completeness = _integer_array(semantic, "completeness", "semantic")
    _require_shape(completeness, (row_count,), "semantic.completeness")

    token = _mapping(_required(semantic, "token", "semantic"), "semantic.token")
    _exact_keys(
        token,
        required={"row_splits", "kind", "scope"},
        optional=set(),
        name="semantic.token",
    )
    token_kind = _integer_array(token, "kind", "semantic.token")
    token_scope = _integer_array(token, "scope", "semantic.token")
    _require_shape(token_scope, token_kind.shape, "semantic.token.scope")
    token_splits = _integer_array(token, "row_splits", "semantic.token")
    token_bounds = _split_bounds(
        token_splits,
        row_count,
        "token",
        flat_count=token_kind.size,
        require_non_empty=True,
    )

    candidate_tokens = _integer_array(
        semantic,
        "candidate_token_indices",
        "semantic",
    )
    if candidate_splits[-1] != candidate_tokens.size:
        raise SemanticBatchError(
            "candidate_row_splits do not end at candidate_token_indices length"
        )
    _validate_candidate_rows(candidate_tokens, candidate_bounds, token_bounds)
    _validate_token_scopes(token_scope, candidate_tokens, candidate_bounds, token_bounds)

    categorical = _parallel_token_table(
        semantic,
        "categorical",
        token_kind.size,
    )
    scalar = _parallel_token_table(semantic, "scalar", token_kind.size)
    relation = _relation_table(semantic, token_bounds, token_kind.size)
    source_scope = token_scope[relation["source_token_indices"]]
    target_scope = token_scope[relation["target_token_indices"]]
    if np.any((source_scope > 0) & (target_scope > 0) & (source_scope != target_scope)):
        raise SemanticBatchError("semantic relation crosses candidate scopes")

    token_lengths = [token_bounds[row][1] - token_bounds[row][0] for row in rows]
    candidate_lengths = [
        candidate_bounds[row][1] - candidate_bounds[row][0] for row in rows
    ]
    new_token_splits = _build_splits(token_lengths, token_splits.dtype)
    new_candidate_splits = _build_splits(candidate_lengths, candidate_splits.dtype)
    row_offsets = tuple(
        (
            token_bounds[row][0],
            token_bounds[row][1],
            int(new_token_splits[new_row]),
        )
        for new_row, row in enumerate(rows)
    )

    result: dict[str, object] = {
        "slot_indices": slots[list(rows)].copy(),
        "phase": phases[list(rows)].copy(),
        "candidate_counts": candidate_counts[list(rows)].copy(),
        "candidate_row_splits": new_candidate_splits,
        "semantic": {
            "schema_version": _required(semantic, "schema_version", "semantic"),
            "completeness": completeness[list(rows)].copy(),
            "token": {
                "row_splits": new_token_splits,
                "kind": _concatenate_row_slices(token_kind, token_bounds, rows),
                "scope": _concatenate_row_slices(token_scope, token_bounds, rows),
            },
            "categorical": _select_parallel_token_table(categorical, row_offsets),
            "scalar": _select_parallel_token_table(scalar, row_offsets),
            "relation": _select_relation_table(relation, row_offsets),
            "candidate_token_indices": _select_candidate_tokens(
                candidate_tokens,
                candidate_bounds,
                rows,
                row_offsets,
            ),
        },
    }
    if decision_token_ids is not None:
        assert public_snapshot_ids is not None
        assert decision_steps is not None
        result["decision_token_ids"] = [decision_token_ids[row] for row in rows]
        result["public_snapshot_ids"] = [public_snapshot_ids[row] for row in rows]
        result["decision_steps"] = decision_steps[list(rows)].copy()
    if "dense_action_mask" in batch:
        dense_mask = _array(batch, "dense_action_mask", "decision_batch", ndim=2)
        if dense_mask.shape[0] != row_count:
            raise SemanticBatchError(
                "dense_action_mask row count disagrees with slot_indices"
            )
        result["dense_action_mask"] = dense_mask[list(rows)].copy()
    return result


def _string_rows(
    mapping: Mapping[object, object],
    key: str,
    row_count: int,
    name: str,
) -> tuple[str, ...]:
    raw = _required(mapping, key, name)
    if isinstance(raw, (str, bytes)) or not isinstance(raw, Sequence):
        raise SemanticBatchError(f"{name}.{key} must be a sequence of strings")
    values = tuple(raw)
    if len(values) != row_count or not all(
        isinstance(value, str) and value for value in values
    ):
        raise SemanticBatchError(f"{name}.{key} is malformed or misaligned")
    return values


def _parallel_token_table(
    semantic: Mapping[object, object],
    key: str,
    token_count: int,
) -> dict[str, np.ndarray]:
    name = f"semantic.{key}"
    table = _mapping(_required(semantic, key, "semantic"), name)
    _exact_keys(
        table,
        required={"token_indices", "field", "value"},
        optional=set(),
        name=name,
    )
    token_indices = _integer_array(table, "token_indices", name)
    field = _integer_array(table, "field", name)
    value = _array(table, "value", name, ndim=1)
    _require_aligned((token_indices, field, value), name)
    _validate_index_range(token_indices, token_count, f"{name}.token_indices")
    return {"token_indices": token_indices, "field": field, "value": value}


def _relation_table(
    semantic: Mapping[object, object],
    token_bounds: tuple[tuple[int, int], ...],
    token_count: int,
) -> dict[str, np.ndarray]:
    name = "semantic.relation"
    table = _mapping(_required(semantic, "relation", "semantic"), name)
    _exact_keys(
        table,
        required={"source_token_indices", "relation", "target_token_indices"},
        optional=set(),
        name=name,
    )
    source = _integer_array(table, "source_token_indices", name)
    relation = _integer_array(table, "relation", name)
    target = _integer_array(table, "target_token_indices", name)
    _require_aligned((source, relation, target), name)
    _validate_index_range(source, token_count, f"{name}.source_token_indices")
    _validate_index_range(target, token_count, f"{name}.target_token_indices")
    if source.size:
        token_rows = np.empty(token_count, dtype=np.int64)
        for row, (start, end) in enumerate(token_bounds):
            token_rows[start:end] = row
        if np.any(token_rows[source] != token_rows[target]):
            raise SemanticBatchError("a semantic relation escapes its decision row")
    return {
        "source_token_indices": source,
        "relation": relation,
        "target_token_indices": target,
    }


def _select_parallel_token_table(
    table: Mapping[str, np.ndarray],
    row_offsets: tuple[tuple[int, int, int], ...],
) -> dict[str, np.ndarray]:
    old_indices = table["token_indices"]
    masks = [
        (old_indices >= old_start) & (old_indices < old_end)
        for old_start, old_end, _ in row_offsets
    ]
    new_indices = [
        old_indices[mask] - old_start + new_start
        for mask, (old_start, _, new_start) in zip(
            masks,
            row_offsets,
            strict=True,
        )
    ]
    return {
        "token_indices": _concatenate(new_indices, old_indices),
        "field": _concatenate([table["field"][mask] for mask in masks], table["field"]),
        "value": _concatenate([table["value"][mask] for mask in masks], table["value"]),
    }


def _select_relation_table(
    table: Mapping[str, np.ndarray],
    row_offsets: tuple[tuple[int, int, int], ...],
) -> dict[str, np.ndarray]:
    old_source = table["source_token_indices"]
    masks = [
        (old_source >= old_start) & (old_source < old_end)
        for old_start, old_end, _ in row_offsets
    ]
    new_source = [
        old_source[mask] - old_start + new_start
        for mask, (old_start, _, new_start) in zip(
            masks,
            row_offsets,
            strict=True,
        )
    ]
    new_target = [
        table["target_token_indices"][mask] - old_start + new_start
        for mask, (old_start, _, new_start) in zip(
            masks,
            row_offsets,
            strict=True,
        )
    ]
    return {
        "source_token_indices": _concatenate(new_source, old_source),
        "relation": _concatenate(
            [table["relation"][mask] for mask in masks],
            table["relation"],
        ),
        "target_token_indices": _concatenate(
            new_target,
            table["target_token_indices"],
        ),
    }


def _select_candidate_tokens(
    candidate_tokens: np.ndarray,
    candidate_bounds: tuple[tuple[int, int], ...],
    rows: tuple[int, ...],
    row_offsets: tuple[tuple[int, int, int], ...],
) -> np.ndarray:
    parts = []
    for row, (old_token_start, _, new_token_start) in zip(
        rows,
        row_offsets,
        strict=True,
    ):
        candidate_start, candidate_end = candidate_bounds[row]
        parts.append(
            candidate_tokens[candidate_start:candidate_end]
            - old_token_start
            + new_token_start
        )
    return _concatenate(parts, candidate_tokens)


def _concatenate_row_slices(
    values: np.ndarray,
    bounds: tuple[tuple[int, int], ...],
    rows: tuple[int, ...],
) -> np.ndarray:
    return _concatenate([values[slice(*bounds[row])] for row in rows], values)


def _concatenate(parts: list[np.ndarray], template: np.ndarray) -> np.ndarray:
    if not parts or all(part.size == 0 for part in parts):
        return np.empty((0,), dtype=template.dtype)
    return np.concatenate(parts).astype(template.dtype, copy=False)


def _validate_candidate_rows(
    candidate_tokens: np.ndarray,
    candidate_bounds: tuple[tuple[int, int], ...],
    token_bounds: tuple[tuple[int, int], ...],
) -> None:
    for row, ((candidate_start, candidate_end), (token_start, token_end)) in enumerate(
        zip(candidate_bounds, token_bounds, strict=True)
    ):
        row_candidates = candidate_tokens[candidate_start:candidate_end]
        if np.any(row_candidates < token_start) or np.any(row_candidates >= token_end):
            raise SemanticBatchError(
                f"candidate token escapes decision row {row}"
            )


def _validate_token_scopes(token_scope, candidate_tokens, candidate_bounds, token_bounds):
    """Row-local ownership IDs are topology, not categorical model features."""
    for row, ((candidate_start, candidate_end), (token_start, token_end)) in enumerate(
        zip(candidate_bounds, token_bounds, strict=True)
    ):
        scopes = token_scope[token_start:token_end]
        count = candidate_end - candidate_start
        owners = token_scope[candidate_tokens[candidate_start:candidate_end]]
        if (np.any(scopes < 0) or np.any(scopes > count)
                or not np.any(scopes == 0)
                or not np.array_equal(np.sort(owners), np.arange(1, count + 1))):
            raise SemanticBatchError(f"invalid candidate scope ownership in row {row}")


def _normalize_rows(row_indices: Sequence[int], row_count: int) -> tuple[int, ...]:
    try:
        raw_rows = tuple(row_indices)
    except TypeError as error:
        raise SemanticBatchError("row_indices must be a sequence") from error
    if not raw_rows:
        raise SemanticBatchError("at least one decision row must be selected")
    rows: list[int] = []
    for raw in raw_rows:
        try:
            row = operator.index(raw)
        except TypeError as error:
            raise SemanticBatchError("row index must be an integer") from error
        if not 0 <= row < row_count:
            raise SemanticBatchError(f"row index {row} is outside 0..{row_count}")
        rows.append(row)
    if len(set(rows)) != len(rows):
        raise SemanticBatchError("row selection contains duplicates")
    return tuple(rows)


def _split_bounds(
    splits: np.ndarray,
    row_count: int,
    name: str,
    *,
    flat_count: int | None = None,
    require_non_empty: bool = False,
) -> tuple[tuple[int, int], ...]:
    _require_shape(splits, (row_count + 1,), f"{name}_row_splits")
    if int(splits[0]) != 0:
        raise SemanticBatchError(f"{name}_row_splits must start at zero")
    bounds = tuple(
        (int(start), int(end))
        for start, end in zip(splits[:-1], splits[1:], strict=True)
    )
    if any(end < start for start, end in bounds):
        raise SemanticBatchError(f"{name}_row_splits are not monotonic")
    if require_non_empty and any(end == start for start, end in bounds):
        raise SemanticBatchError(f"{name} rows must not be empty")
    if flat_count is not None and int(splits[-1]) != flat_count:
        raise SemanticBatchError(
            f"{name}_row_splits do not end at the flat table length"
        )
    return bounds


def _build_splits(lengths: list[int], dtype: np.dtype[object]) -> np.ndarray:
    result = np.empty(len(lengths) + 1, dtype=dtype)
    result[0] = 0
    total = 0
    for index, length in enumerate(lengths, start=1):
        total += length
        result[index] = total
    return result


def _integer_array(
    mapping: Mapping[object, object],
    key: str,
    name: str,
) -> np.ndarray:
    result = _array(mapping, key, name, ndim=1)
    if not np.issubdtype(result.dtype, np.integer):
        raise SemanticBatchError(f"{name}.{key} must have an integer dtype")
    return result


def _array(
    mapping: Mapping[object, object],
    key: str,
    name: str,
    *,
    ndim: int,
) -> np.ndarray:
    value = _required(mapping, key, name)
    if not isinstance(value, np.ndarray):
        raise SemanticBatchError(f"{name}.{key} must be a NumPy array")
    if value.ndim != ndim:
        raise SemanticBatchError(f"{name}.{key} must have {ndim} dimensions")
    return value


def _validate_index_range(values: np.ndarray, upper_bound: int, name: str) -> None:
    if values.size and (np.any(values < 0) or np.any(values >= upper_bound)):
        raise SemanticBatchError(f"{name} is outside 0..{upper_bound}")


def _require_aligned(values: tuple[np.ndarray, ...], name: str) -> None:
    if len({value.shape for value in values}) != 1:
        raise SemanticBatchError(f"{name} columns are misaligned")


def _require_shape(value: np.ndarray, shape: tuple[int, ...], name: str) -> None:
    if value.shape != shape:
        raise SemanticBatchError(f"{name} has shape {value.shape}, expected {shape}")


def _mapping(value: object, name: str) -> Mapping[object, object]:
    if not isinstance(value, Mapping):
        raise SemanticBatchError(f"{name} must be a mapping")
    return value


def _required(mapping: Mapping[object, object], key: str, name: str) -> object:
    try:
        return mapping[key]
    except KeyError as error:
        raise SemanticBatchError(f"{name} is missing required field {key}") from error


def _exact_keys(
    mapping: Mapping[object, object],
    *,
    required: set[str],
    optional: set[str],
    name: str,
) -> None:
    keys = set(mapping)
    missing = required - keys
    unexpected = keys - required - optional
    if missing:
        raise SemanticBatchError(
            f"{name} is missing fields: {', '.join(sorted(missing))}"
        )
    if unexpected:
        raise SemanticBatchError(
            f"{name} has unsupported fields: {', '.join(sorted(map(str, unexpected)))}"
        )
