from __future__ import annotations

import unittest

import numpy as np

from learning.tests.semantic_fixtures import semantic_batch_fixture
from sts_learning import (
    SemanticBatchError,
    iter_payload_arrays,
    select_semantic_decision_rows,
)


class SemanticBatchSelectionTests(unittest.TestCase):
    def test_selection_reorders_rows_and_compacts_all_token_indices(self) -> None:
        source = semantic_batch_fixture(dense_mask=True)

        selected = select_semantic_decision_rows(source, [1, 0])
        semantic = selected["semantic"]

        np.testing.assert_array_equal(selected["slot_indices"], [9, 4])
        np.testing.assert_array_equal(selected["phase"], [2, 1])
        np.testing.assert_array_equal(selected["candidate_counts"], [3, 2])
        np.testing.assert_array_equal(selected["candidate_row_splits"], [0, 3, 5])
        np.testing.assert_array_equal(
            selected["dense_action_mask"],
            [[True, True, True], [True, True, False]],
        )
        np.testing.assert_array_equal(
            semantic["token"]["row_splits"],
            [0, 5, 9],
        )
        np.testing.assert_array_equal(
            semantic["candidate_token_indices"],
            [1, 2, 3, 6, 7],
        )
        np.testing.assert_array_equal(
            semantic["categorical"]["token_indices"],
            [0, 1, 2, 3, 5, 6, 7],
        )
        np.testing.assert_array_equal(
            semantic["scalar"]["token_indices"],
            [0, 4, 5, 8],
        )
        np.testing.assert_array_equal(
            semantic["relation"]["source_token_indices"],
            [0, 0, 0, 1, 5, 5, 6],
        )
        np.testing.assert_array_equal(
            semantic["relation"]["target_token_indices"],
            [1, 2, 3, 4, 6, 7, 8],
        )
        np.testing.assert_array_equal(source["slot_indices"], [4, 9])

    def test_single_row_selection_preserves_dtypes_and_exact_local_graph(self) -> None:
        source = semantic_batch_fixture()

        selected = select_semantic_decision_rows(source, [1])
        semantic = selected["semantic"]

        self.assertEqual(
            selected["candidate_row_splits"].dtype,
            source["candidate_row_splits"].dtype,
        )
        self.assertEqual(
            semantic["token"]["kind"].dtype,
            source["semantic"]["token"]["kind"].dtype,
        )
        np.testing.assert_array_equal(semantic["token"]["row_splits"], [0, 5])
        np.testing.assert_array_equal(
            semantic["candidate_token_indices"],
            [1, 2, 3],
        )
        np.testing.assert_array_equal(
            semantic["relation"]["source_token_indices"],
            [0, 0, 0, 1],
        )
        np.testing.assert_array_equal(
            semantic["relation"]["target_token_indices"],
            [1, 2, 3, 4],
        )

    def test_duplicate_or_out_of_range_rows_are_rejected(self) -> None:
        batch = semantic_batch_fixture()

        with self.assertRaisesRegex(SemanticBatchError, "duplicates"):
            select_semantic_decision_rows(batch, [0, 0])
        with self.assertRaisesRegex(SemanticBatchError, "outside"):
            select_semantic_decision_rows(batch, [2])

    def test_cross_row_relation_is_rejected_before_selection(self) -> None:
        batch = semantic_batch_fixture()
        relation = batch["semantic"]["relation"]
        relation["target_token_indices"][0] = 5

        with self.assertRaisesRegex(SemanticBatchError, "relation escapes"):
            select_semantic_decision_rows(batch, [1])

    def test_unknown_bridge_field_fails_closed(self) -> None:
        batch = semantic_batch_fixture()
        batch["future_field"] = np.array([1, 2], dtype=np.uint8)

        with self.assertRaisesRegex(SemanticBatchError, "unsupported fields"):
            select_semantic_decision_rows(batch, [0])

    def test_frozen_experience_payload_can_be_selected_without_mutation(self) -> None:
        batch = semantic_batch_fixture()
        for array in iter_payload_arrays(batch):
            array.setflags(write=False)

        selected = select_semantic_decision_rows(batch, [0])

        self.assertTrue(all(not array.flags.writeable for array in iter_payload_arrays(batch)))
        self.assertTrue(all(array.flags.writeable for array in iter_payload_arrays(selected)))
        np.testing.assert_array_equal(selected["slot_indices"], [4])


if __name__ == "__main__":
    unittest.main()
