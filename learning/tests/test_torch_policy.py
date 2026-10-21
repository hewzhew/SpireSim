from __future__ import annotations

import importlib.util
import math
import unittest
from copy import deepcopy

from learning.tests.policy_fixtures import BEHAVIOR_MANIFEST_ID
from learning.tests.semantic_fixtures import (
    semantic_batch_fixture,
    semantic_schema_fixture,
)
from sts_learning import select_semantic_decision_rows


_TORCH_AVAILABLE = importlib.util.find_spec("torch") is not None
if _TORCH_AVAILABLE:
    import torch

    from sts_learning.torch_policy import (
        GreedyTorchPolicy,
        RaggedCandidateLogits,
        RaggedCandidateScorer,
        RaggedCategoricalPolicyConfig,
        RaggedScorerConfig,
        SemanticSchemaDimensions,
        TorchPolicyError,
        configure_critic_only_training,
        load_scorer_warm_start,
        ragged_cross_entropy,
        ragged_frozen_greedy_trust_hinge,
        ragged_resolved_pairwise_loss,
        require_matching_actor_state,
        sample_ragged_categorical,
        sample_ragged_categorical_rows,
    )

try:
    from sts_learning_bridge import LearningBatchEnv, semantic_schema
except ImportError:
    LearningBatchEnv = None  # type: ignore[assignment,misc]
    semantic_schema = None  # type: ignore[assignment]


@unittest.skipUnless(_TORCH_AVAILABLE, "optional PyTorch dependency is not installed")
class TorchPolicyTests(unittest.TestCase):
    def test_schema_dimensions_come_only_from_bridge_schema(self) -> None:
        dimensions = SemanticSchemaDimensions.from_bridge_schema(
            semantic_schema_fixture()
        )

        self.assertEqual(dimensions.token_kind_size, 3)
        self.assertEqual(dimensions.categorical_field_size, 2)
        self.assertEqual(dimensions.categorical_offsets, (0, 3))
        self.assertEqual(dimensions.categorical_vocabulary_size, 5)

    def test_identity_residual_vocabularies_start_at_mechanical_fallback(self) -> None:
        torch.manual_seed(17)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(identity_residual_fields=(1,))
        )

        weights = scorer.categorical_value.weight.detach()
        self.assertTrue(bool(torch.any(weights[0:3] != 0)))
        torch.testing.assert_close(weights[3:5], torch.zeros_like(weights[3:5]))

    def test_identity_residual_fields_must_name_categorical_vocabularies(self) -> None:
        with self.assertRaisesRegex(TorchPolicyError, "unknown field"):
            SemanticSchemaDimensions.from_bridge_schema(
                semantic_schema_fixture(identity_residual_fields=(2,))
            )

    def test_unobserved_scalar_values_start_neutral_and_observed_values_can_learn(self) -> None:
        torch.manual_seed(7)
        scorer = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture())
        self.assertTrue(torch.equal(scorer.scalar_weight.weight, torch.zeros_like(scorer.scalar_weight.weight)))
        batch = deepcopy(semantic_batch_fixture())
        table = batch["semantic"]["scalar"]
        table["value"][table["field"] == 0] = 0
        optimizer = torch.optim.SGD(scorer.parameters(), lr=0.05)
        for _ in range(3):
            optimizer.zero_grad()
            ragged_cross_entropy(scorer(batch), [1, 2]).backward()
            optimizer.step()
        self.assertTrue(torch.equal(scorer.scalar_weight.weight[0], torch.zeros_like(scorer.scalar_weight.weight[0])))
        unseen = deepcopy(batch)
        unseen["semantic"]["scalar"]["value"][table["field"] == 0] = 217
        scorer.eval()
        with torch.inference_mode():
            self.assertTrue(torch.equal(scorer(batch).values, scorer(unseen).values))
        for _ in range(3):
            optimizer.zero_grad()
            ragged_cross_entropy(scorer(semantic_batch_fixture()), [1, 2]).backward()
            optimizer.step()
        self.assertTrue(bool(torch.any(scorer.scalar_weight.weight[0] != 0)))

    def test_ragged_logits_loss_and_parameter_update(self) -> None:
        assert _TORCH_AVAILABLE
        torch.manual_seed(7)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(hidden_dim=24, relation_layers=1),
        )
        optimizer = torch.optim.SGD(scorer.parameters(), lr=0.05)

        logits = scorer(semantic_batch_fixture())
        self.assertEqual(tuple(logits.values.shape), (5,))
        self.assertEqual(logits.row_splits.tolist(), [0, 2, 5])
        self.assertTrue(bool(torch.all(torch.isfinite(logits.values))))
        self.assertEqual(len(logits.greedy_ordinals()), 2)

        loss = ragged_cross_entropy(logits, [1, 2])
        self.assertTrue(bool(torch.isfinite(loss)))
        before = scorer.scorer[-1].weight.detach().clone()
        optimizer.zero_grad()
        loss.backward()
        self.assertTrue(
            any(
                parameter.grad is not None
                and bool(torch.all(torch.isfinite(parameter.grad)))
                for parameter in scorer.parameters()
            )
        )
        optimizer.step()
        self.assertFalse(torch.equal(before, scorer.scorer[-1].weight.detach()))

    def test_resolved_pairwise_loss_leaves_unknown_candidates_unlabeled(self) -> None:
        values = torch.tensor([0.0, 7.0, 0.0, 3.0, -1.0], requires_grad=True)
        logits = RaggedCandidateLogits(
            values=values,
            row_splits=torch.tensor([0, 3, 5]),
        )

        loss = ragged_resolved_pairwise_loss(logits, [[(0, 2)], [(0, 1)]])
        expected = (
            torch.nn.functional.softplus(torch.tensor(0.0))
            + torch.nn.functional.softplus(torch.tensor(-4.0))
        ) / 2.0
        torch.testing.assert_close(loss, expected)
        loss.backward()
        self.assertEqual(values.grad[1].item(), 0.0)

    def test_resolved_pairwise_loss_does_not_order_co_winners(self) -> None:
        first = RaggedCandidateLogits(
            values=torch.tensor([2.0, -1.0, 0.0]),
            row_splits=torch.tensor([0, 3]),
        )
        swapped = RaggedCandidateLogits(
            values=torch.tensor([-1.0, 2.0, 0.0]),
            row_splits=torch.tensor([0, 3]),
        )
        pairs = [[(0, 2), (1, 2)]]

        torch.testing.assert_close(
            ragged_resolved_pairwise_loss(first, pairs),
            ragged_resolved_pairwise_loss(swapped, pairs),
        )

    def test_resolved_pairwise_loss_rejects_missing_or_invalid_evidence(self) -> None:
        logits = RaggedCandidateLogits(
            values=torch.tensor([0.0, 0.0]),
            row_splits=torch.tensor([0, 2]),
        )

        with self.assertRaisesRegex(TorchPolicyError, "at least one"):
            ragged_resolved_pairwise_loss(logits, [[]])
        with self.assertRaisesRegex(TorchPolicyError, "outside"):
            ragged_resolved_pairwise_loss(logits, [[(0, 2)]])
        with self.assertRaisesRegex(TorchPolicyError, "itself"):
            ragged_resolved_pairwise_loss(logits, [[(1, 1)]])

    def test_frozen_greedy_trust_hinge_activates_only_at_threatened_rows(self) -> None:
        values = torch.tensor([2.0, 0.0, 1.0, 1.4], requires_grad=True)
        logits = RaggedCandidateLogits(
            values=values,
            row_splits=torch.tensor([0, 2, 4]),
        )

        loss = ragged_frozen_greedy_trust_hinge(
            logits,
            [0, 0],
            margin=0.1,
        )

        torch.testing.assert_close(loss, torch.tensor(0.25))
        loss.backward()
        torch.testing.assert_close(values.grad, torch.tensor([0.0, 0.0, -0.5, 0.5]))

    def test_frozen_greedy_trust_hinge_validates_alignment_and_margin(self) -> None:
        logits = RaggedCandidateLogits(
            values=torch.tensor([0.0, 0.0]),
            row_splits=torch.tensor([0, 2]),
        )

        with self.assertRaisesRegex(TorchPolicyError, "match"):
            ragged_frozen_greedy_trust_hinge(logits, [])
        with self.assertRaisesRegex(TorchPolicyError, "outside"):
            ragged_frozen_greedy_trust_hinge(logits, [2])
        with self.assertRaisesRegex(TorchPolicyError, "non-negative"):
            ragged_frozen_greedy_trust_hinge(logits, [0], margin=-0.1)

    def test_actor_critic_head_starts_neutral_and_aligns_to_rows(self) -> None:
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(
                hidden_dim=24,
                relation_layers=1,
                value_head=True,
            ),
        )
        batch = semantic_batch_fixture()

        output = scorer.actor_critic(batch)

        torch.testing.assert_close(output.logits.values, scorer(batch).values)
        self.assertEqual(tuple(output.row_values.shape), (2,))
        torch.testing.assert_close(output.row_values, torch.zeros(2))
        output.row_values.sum().backward()
        assert scorer.value_head is not None
        final = scorer.value_head[-1]
        assert isinstance(final, torch.nn.Linear)
        self.assertTrue(bool(torch.any(final.bias.grad != 0)))

    def test_multi_actor_critic_has_fixed_columns_and_actor_only_warm_start(
        self,
    ) -> None:
        schema = semantic_schema_fixture()
        source = RaggedCandidateScorer.from_bridge_schema(
            schema,
            RaggedScorerConfig(
                hidden_dim=24,
                relation_layers=1,
                value_head=True,
            ),
        )
        target = RaggedCandidateScorer.from_bridge_schema(
            schema,
            RaggedScorerConfig(
                hidden_dim=24,
                relation_layers=1,
                value_head=True,
                value_head_width=3,
            ),
        )

        load_scorer_warm_start(target, source)
        output = target.actor_critic_multi(semantic_batch_fixture())

        self.assertEqual(tuple(output.row_values.shape), (2, 3))
        torch.testing.assert_close(output.row_values, torch.zeros(2, 3))
        with self.assertRaisesRegex(TorchPolicyError, "width one"):
            target.actor_critic(semantic_batch_fixture())
        for key, value in source.state_dict().items():
            if not key.startswith("value_head."):
                torch.testing.assert_close(target.state_dict()[key], value)

    def test_critic_only_scope_freezes_and_audits_every_actor_tensor(self) -> None:
        schema = semantic_schema_fixture()
        source = RaggedCandidateScorer.from_bridge_schema(
            schema,
            RaggedScorerConfig(hidden_dim=24, relation_layers=1),
        )
        calibrated = RaggedCandidateScorer.from_bridge_schema(
            schema,
            RaggedScorerConfig(
                hidden_dim=24,
                relation_layers=1,
                value_head=True,
            ),
        )
        load_scorer_warm_start(calibrated, source, actor_only=True)
        require_matching_actor_state(source, calibrated)

        configure_critic_only_training(calibrated)
        for name, parameter in calibrated.named_parameters():
            self.assertEqual(
                parameter.requires_grad,
                name.startswith("value_head."),
            )

        with torch.no_grad():
            calibrated.scorer[-1].bias.add_(1.0)
        with self.assertRaisesRegex(TorchPolicyError, "changed actor tensor"):
            require_matching_actor_state(source, calibrated)

    def test_value_head_width_rejects_ambiguous_profiles(self) -> None:
        with self.assertRaisesRegex(TorchPolicyError, "disabled"):
            RaggedScorerConfig(value_head_width=3)
        with self.assertRaisesRegex(TorchPolicyError, "must not exceed"):
            RaggedScorerConfig(value_head=True, value_head_width=65)

    def test_cross_row_relation_is_rejected(self) -> None:
        scorer = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture())
        batch = semantic_batch_fixture()
        relation = batch["semantic"]["relation"]  # type: ignore[index]
        relation["target_token_indices"][0] = 5  # type: ignore[index]

        with self.assertRaisesRegex(TorchPolicyError, "relation escapes"):
            scorer(batch)

    def test_row_selection_preserves_logits_without_cross_row_leakage(self) -> None:
        torch.manual_seed(9)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(),
            RaggedScorerConfig(hidden_dim=20, relation_layers=2),
        )
        batch = semantic_batch_fixture()

        original = scorer(batch)
        selected = scorer(select_semantic_decision_rows(batch, [1, 0]))
        expected = torch.cat((original.values[2:5], original.values[0:2]))

        self.assertEqual(selected.row_splits.tolist(), [0, 3, 5])
        torch.testing.assert_close(selected.values, expected)

    def test_categorical_config_has_canonical_rule_identity(self) -> None:
        first = RaggedCategoricalPolicyConfig(temperature=0.5)
        same = RaggedCategoricalPolicyConfig(temperature=0.5)
        different = RaggedCategoricalPolicyConfig(temperature=1.0)

        self.assertEqual(first.behavior_rule, same.behavior_rule)
        self.assertNotEqual(first.behavior_rule, different.behavior_rule)
        for invalid in (0.0, -1.0, float("nan"), float("inf")):
            with self.assertRaisesRegex(TorchPolicyError, "finite and positive"):
                RaggedCategoricalPolicyConfig(temperature=invalid)
        with self.assertRaisesRegex(TorchPolicyError, "real number"):
            RaggedCategoricalPolicyConfig(temperature="1")  # type: ignore[arg-type]

    def test_categorical_sampling_is_ragged_reproducible_and_rng_local(self) -> None:
        logits = RaggedCandidateLogits(
            values=torch.tensor([0.0, math.log(3.0), 0.0, 0.0, 0.0]),
            row_splits=torch.tensor([0, 2, 5]),
        )
        first_generator = torch.Generator().manual_seed(1234)
        second_generator = torch.Generator().manual_seed(1234)
        global_state = torch.random.get_rng_state().clone()

        first = sample_ragged_categorical(
            logits,
            RaggedCategoricalPolicyConfig(),
            first_generator,
        )
        second = sample_ragged_categorical(
            logits,
            RaggedCategoricalPolicyConfig(),
            second_generator,
        )

        self.assertEqual(first, second)
        self.assertTrue(torch.equal(torch.random.get_rng_state(), global_state))
        self.assertIn(first.ordinals[0], (0, 1))
        self.assertIn(first.ordinals[1], (0, 1, 2))
        self.assertAlmostEqual(
            first.selection_probabilities[0].value,
            (0.25, 0.75)[first.ordinals[0]],
        )
        self.assertAlmostEqual(
            first.selection_probabilities[1].value,
            1.0 / 3.0,
        )

    def test_row_local_generators_isolate_divergent_replicates(self) -> None:
        two_rows = RaggedCandidateLogits(
            values=torch.tensor([0.0, 0.0, 0.0, 0.0]),
            row_splits=torch.tensor([0, 2, 4]),
        )
        one_row = RaggedCandidateLogits(
            values=torch.tensor([0.0, 0.0]),
            row_splits=torch.tensor([0, 2]),
        )
        first_stream = torch.Generator().manual_seed(111)
        second_stream = torch.Generator().manual_seed(222)
        reference = torch.Generator().manual_seed(222)

        first_round = sample_ragged_categorical_rows(
            two_rows,
            RaggedCategoricalPolicyConfig(),
            (first_stream, second_stream),
        )
        first_stream_after_round = first_stream.get_state().clone()
        second_round = sample_ragged_categorical_rows(
            one_row,
            RaggedCategoricalPolicyConfig(),
            (second_stream,),
        )
        reference_first = sample_ragged_categorical(
            one_row,
            RaggedCategoricalPolicyConfig(),
            reference,
        )
        reference_second = sample_ragged_categorical(
            one_row,
            RaggedCategoricalPolicyConfig(),
            reference,
        )

        self.assertEqual(first_round.ordinals[1], reference_first.ordinals[0])
        self.assertEqual(second_round, reference_second)
        self.assertTrue(
            torch.equal(first_stream.get_state(), first_stream_after_round)
        )

    def test_categorical_sampling_validates_all_rows_before_consuming_rng(self) -> None:
        generator = torch.Generator().manual_seed(9)
        state = generator.get_state().clone()
        invalid = RaggedCandidateLogits(
            values=torch.tensor([0.0]),
            row_splits=torch.tensor([0, 2, 1]),
        )

        with self.assertRaisesRegex(TorchPolicyError, "non-empty increasing"):
            sample_ragged_categorical(
                invalid,
                RaggedCategoricalPolicyConfig(),
                generator,
            )
        self.assertTrue(torch.equal(generator.get_state(), state))

        non_finite = RaggedCandidateLogits(
            values=torch.tensor([float("nan")]),
            row_splits=torch.tensor([0, 1]),
        )
        with self.assertRaisesRegex(TorchPolicyError, "must be finite"):
            sample_ragged_categorical(
                non_finite,
                RaggedCategoricalPolicyConfig(),
                generator,
            )
        self.assertTrue(torch.equal(generator.get_state(), state))
        with self.assertRaisesRegex(TorchPolicyError, "global generator"):
            sample_ragged_categorical(
                RaggedCandidateLogits(
                    values=torch.tensor([0.0]),
                    row_splits=torch.tensor([0, 1]),
                ),
                RaggedCategoricalPolicyConfig(),
                torch.default_generator,
            )


@unittest.skipUnless(
    _TORCH_AVAILABLE and LearningBatchEnv is not None,
    "optional PyTorch dependency or standalone bridge wheel is not installed",
)
class RealBridgeTorchPolicyTests(unittest.TestCase):
    def test_real_semantic_batch_trains_without_padding(self) -> None:
        assert LearningBatchEnv is not None
        assert semantic_schema is not None
        env = LearningBatchEnv([11, 12, 13], 20)
        batch = env.decision_batch(semantic=True)
        scorer = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema(),
            RaggedScorerConfig(hidden_dim=16, relation_layers=1),
        )

        logits = scorer(batch)
        targets = torch.zeros(len(batch["slot_indices"]), dtype=torch.long)
        loss = ragged_cross_entropy(logits, targets)
        loss.backward()

        selected_batch = select_semantic_decision_rows(batch, [2, 0])
        selected_logits = scorer(selected_batch)
        original_splits = logits.row_splits.tolist()
        expected = torch.cat(
            tuple(
                logits.values[original_splits[row] : original_splits[row + 1]]
                for row in (2, 0)
            )
        )

        self.assertEqual(logits.row_splits.tolist(), batch["candidate_row_splits"].tolist())
        torch.testing.assert_close(selected_logits.values, expected)
        self.assertTrue(bool(torch.isfinite(loss)))
        choice = GreedyTorchPolicy(scorer, BEHAVIOR_MANIFEST_ID).choose(batch)
        self.assertEqual(choice.behavior_manifest_id, BEHAVIOR_MANIFEST_ID)
        self.assertEqual(len(choice.ordinals), len(batch["slot_indices"]))
        self.assertTrue(
            all(
                0 <= ordinal < int(count)
                for ordinal, count in zip(
                    choice.ordinals,
                    batch["candidate_counts"],
                    strict=True,
                )
            )
        )
        env.choose(list(batch["decision_token_ids"]), list(choice.ordinals))
        self.assertTrue(env.ready)


if __name__ == "__main__":
    unittest.main()
