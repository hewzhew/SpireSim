from __future__ import annotations

import numpy as np


def semantic_schema_fixture(
    *,
    identity_residual_fields: tuple[int, ...] = (),
) -> dict[str, object]:
    return {
        "version": 2,
        "token_kind": {"Observation": 0, "Candidate": 1, "Entity": 2},
        "categorical_field": {"Kind": 0, "Flag": 1},
        "scalar_field": {"Amount": 0},
        "relation_kind": {"HasCandidate": 0, "Targets": 1},
        "categorical_vocabulary_size": {0: 3, 1: 2},
        "identity_residual_categorical_fields": {
            field: 1 for field in identity_residual_fields
        },
    }


def semantic_batch_fixture(*, dense_mask: bool = False) -> dict[str, object]:
    batch: dict[str, object] = {
        "slot_indices": np.array([4, 9], dtype=np.uint64),
        "phase": np.array([1, 2], dtype=np.uint8),
        "candidate_counts": np.array([2, 3], dtype=np.uint64),
        "candidate_row_splits": np.array([0, 2, 5], dtype=np.uint64),
        "semantic": {
            "schema_version": 2,
            "completeness": np.array([1, 1], dtype=np.uint8),
            "token": {
                "row_splits": np.array([0, 4, 9], dtype=np.uint64),
                "kind": np.array([0, 1, 1, 2, 0, 1, 1, 1, 2], dtype=np.uint16),
                "scope": np.array([0, 1, 2, 0, 0, 1, 2, 3, 0], dtype=np.uint32),
            },
            "categorical": {
                "token_indices": np.array([0, 1, 2, 4, 5, 6, 7], dtype=np.uint64),
                "field": np.array([0, 1, 1, 0, 1, 1, 1], dtype=np.uint16),
                "value": np.array([2, 0, 1, 1, 1, 0, 1], dtype=np.int64),
            },
            "scalar": {
                "token_indices": np.array([0, 3, 4, 8], dtype=np.uint64),
                "field": np.array([0, 0, 0, 0], dtype=np.uint16),
                "value": np.array([0.5, -1.0, 2.0, 0.25], dtype=np.float32),
            },
            "relation": {
                "source_token_indices": np.array(
                    [0, 0, 1, 4, 4, 4, 5], dtype=np.uint64
                ),
                "relation": np.array([0, 0, 1, 0, 0, 0, 1], dtype=np.uint16),
                "target_token_indices": np.array(
                    [1, 2, 3, 5, 6, 7, 8], dtype=np.uint64
                ),
            },
            "candidate_token_indices": np.array([1, 2, 5, 6, 7], dtype=np.uint64),
        },
    }
    if dense_mask:
        batch["dense_action_mask"] = np.array(
            [[True, True, False], [True, True, True]],
            dtype=np.bool_,
        )
    return batch
