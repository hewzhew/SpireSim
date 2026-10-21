from __future__ import annotations

import importlib.util
import unittest

import numpy as np

from learning.tests.semantic_fixtures import (
    semantic_batch_fixture,
    semantic_schema_fixture,
)
from sts_learning import (
    SemanticBatchConcatLimits,
    SemanticBatchError,
    concatenate_semantic_decision_batches,
    plan_semantic_decision_batch_chunks,
    select_semantic_decision_rows,
)


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_policy import RaggedCandidateScorer, RaggedScorerConfig


def _limits(*, rows: int = 16, array_bytes: int = 1024 * 1024):
    return SemanticBatchConcatLimits(
        max_rows=rows,
        max_input_array_bytes=array_bytes,
    )


class SemanticBatchConcatTests(unittest.TestCase):
    def test_chunk_plan_keeps_global_order_and_checks_cross_chunk_schema(self) -> None:
        row = select_semantic_decision_rows(semantic_batch_fixture(), [0])
        chunks = plan_semantic_decision_batch_chunks(
            (row, row, row),
            _limits(rows=2),
        )

        self.assertEqual(
            tuple((chunk.start_batch_index, chunk.stop_batch_index) for chunk in chunks),
            ((0, 2), (2, 3)),
        )
        self.assertEqual(tuple(chunk.row_count for chunk in chunks), (2, 1))

        incompatible = select_semantic_decision_rows(
            semantic_batch_fixture(),
            [1],
        )
        incompatible["phase"] = incompatible["phase"].astype(np.int64)
        with self.assertRaisesRegex(SemanticBatchError, "incompatible schemas"):
            plan_semantic_decision_batch_chunks(
                (row, incompatible),
                _limits(rows=1),
            )

    def test_rows_tokens_relations_and_candidates_are_reindexed_in_order(self) -> None:
        original = semantic_batch_fixture()
        second_row = select_semantic_decision_rows(original, [1])
        first_row = select_semantic_decision_rows(original, [0])

        combined = concatenate_semantic_decision_batches(
            (second_row, first_row),
            _limits(),
        )

        self.assertEqual(combined["slot_indices"].tolist(), [9, 4])
        self.assertEqual(combined["candidate_row_splits"].tolist(), [0, 3, 5])
        semantic = combined["semantic"]
        self.assertEqual(semantic["token"]["row_splits"].tolist(), [0, 5, 9])
        self.assertEqual(semantic["candidate_token_indices"].tolist(), [1, 2, 3, 6, 7])
        source = semantic["relation"]["source_token_indices"]
        target = semantic["relation"]["target_token_indices"]
        self.assertTrue(bool(np.all(source < 9)))
        self.assertTrue(bool(np.all(target < 9)))

    def test_repeated_temporal_rows_are_allowed_but_limits_are_mandatory(self) -> None:
        row = select_semantic_decision_rows(semantic_batch_fixture(), [0])
        combined = concatenate_semantic_decision_batches((row, row), _limits())

        self.assertEqual(combined["slot_indices"].tolist(), [4, 4])
        with self.assertRaisesRegex(SemanticBatchError, "row limit"):
            concatenate_semantic_decision_batches((row, row), _limits(rows=1))
        with self.assertRaisesRegex(SemanticBatchError, "array byte limit"):
            concatenate_semantic_decision_batches(
                (row,),
                _limits(array_bytes=1),
            )

    def test_schema_dtype_and_optional_dense_mask_mismatches_fail_closed(self) -> None:
        first = select_semantic_decision_rows(semantic_batch_fixture(), [0])
        schema_mismatch = select_semantic_decision_rows(
            semantic_batch_fixture(),
            [1],
        )
        schema_mismatch["semantic"]["schema_version"] = 99
        with self.assertRaisesRegex(SemanticBatchError, "schema versions"):
            concatenate_semantic_decision_batches((first, schema_mismatch), _limits())

        dtype_mismatch = select_semantic_decision_rows(
            semantic_batch_fixture(),
            [1],
        )
        dtype_mismatch["phase"] = dtype_mismatch["phase"].astype(np.int64)
        with self.assertRaisesRegex(SemanticBatchError, "dtypes"):
            concatenate_semantic_decision_batches((first, dtype_mismatch), _limits())

        dense = select_semantic_decision_rows(
            semantic_batch_fixture(dense_mask=True),
            [1],
        )
        with self.assertRaisesRegex(SemanticBatchError, "dense_action_mask"):
            concatenate_semantic_decision_batches((first, dense), _limits())

    def test_structurally_invalid_input_is_rejected_before_concatenation(self) -> None:
        invalid = semantic_batch_fixture()
        invalid["semantic"]["relation"]["target_token_indices"][0] = 5

        with self.assertRaisesRegex(SemanticBatchError, "relation escapes"):
            concatenate_semantic_decision_batches((invalid,), _limits())


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class SemanticBatchConcatTorchTests(unittest.TestCase):
    def test_combined_logits_equal_individual_logits_with_one_model_call(self) -> None:
        torch.manual_seed(41)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(hidden_dim=12, relation_layers=1),
        )
        original = semantic_batch_fixture()
        parts = (
            select_semantic_decision_rows(original, [1]),
            select_semantic_decision_rows(original, [0]),
        )

        expected = torch.cat(tuple(scorer(part).values for part in parts))
        combined = concatenate_semantic_decision_batches(parts, _limits())
        actual = scorer(combined)

        self.assertEqual(actual.row_splits.tolist(), [0, 3, 5])
        torch.testing.assert_close(actual.values, expected)


if __name__ == "__main__":
    unittest.main()
