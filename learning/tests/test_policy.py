from __future__ import annotations

import unittest

import numpy as np

from learning.tests.policy_fixtures import BEHAVIOR_MANIFEST_ID
from sts_learning import (
    BatchPolicyChoice,
    BehaviorManifestId,
    PolicyChoiceError,
    SelectionProbability,
)


class PolicyChoiceTests(unittest.TestCase):
    def test_manifest_identity_requires_one_immutable_sha256_digest(self) -> None:
        with self.assertRaisesRegex(PolicyChoiceError, "immutable bytes"):
            BehaviorManifestId(bytearray(32))  # type: ignore[arg-type]
        with self.assertRaisesRegex(PolicyChoiceError, "32 bytes"):
            BehaviorManifestId(b"short")

    def test_batch_choice_normalizes_integer_ordinals_and_preserves_identity(self) -> None:
        choice = BatchPolicyChoice.create(
            [np.uint64(2), 0],
            BEHAVIOR_MANIFEST_ID,
            (SelectionProbability.known(0.25), SelectionProbability.unknown()),
        )

        self.assertEqual(choice.ordinals, (2, 0))
        self.assertEqual(choice.behavior_manifest_id, BEHAVIOR_MANIFEST_ID)
        self.assertEqual(choice.selection_probabilities[0].value, 0.25)
        self.assertIsNone(choice.selection_probabilities[1].value)

    def test_batch_choice_rejects_non_integer_ordinals(self) -> None:
        with self.assertRaisesRegex(PolicyChoiceError, "ordinal must be an integer"):
            BatchPolicyChoice.create(
                [0.5],  # type: ignore[list-item]
                BEHAVIOR_MANIFEST_ID,
                (SelectionProbability.known(1.0),),
            )

    def test_selection_probability_is_typed_aligned_and_bounded(self) -> None:
        deterministic = BatchPolicyChoice.deterministic(
            [0, 1],
            BEHAVIOR_MANIFEST_ID,
        )
        self.assertEqual(
            tuple(item.value for item in deterministic.selection_probabilities),
            (1.0, 1.0),
        )
        for invalid in (0.0, -0.1, 1.1, float("nan"), float("inf")):
            with self.assertRaisesRegex(PolicyChoiceError, r"\(0, 1\]"):
                SelectionProbability.known(invalid)
        with self.assertRaisesRegex(PolicyChoiceError, "not bool or text"):
            SelectionProbability.known("0.5")  # type: ignore[arg-type]
        with self.assertRaisesRegex(PolicyChoiceError, "sequence"):
            BatchPolicyChoice.create(
                [0],
                BEHAVIOR_MANIFEST_ID,
                None,  # type: ignore[arg-type]
            )
        with self.assertRaisesRegex(PolicyChoiceError, "one value per ordinal"):
            BatchPolicyChoice.create(
                [0, 1],
                BEHAVIOR_MANIFEST_ID,
                (SelectionProbability.known(1.0),),
            )
        with self.assertRaisesRegex(PolicyChoiceError, "typed"):
            BatchPolicyChoice.create(
                [0],
                BEHAVIOR_MANIFEST_ID,
                (1.0,),  # type: ignore[arg-type]
            )


if __name__ == "__main__":
    unittest.main()
