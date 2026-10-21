from copy import deepcopy

import pytest

torch = pytest.importorskip("torch")

from learning.tests.semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture
from learning.tests.test_candidate_scope import _batch, _model, _with_duplicate
from sts_learning.semantic_batch import select_semantic_decision_rows
from sts_learning.torch_policy import RaggedCandidateScorer, RaggedScorerConfig, TorchPolicyError, load_scorer_warm_start


def test_attention_matches_independent_dense_rows_and_has_finite_gradients():
    torch.manual_seed(39)
    scorer = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture(),
        RaggedScorerConfig(hidden_dim=8, observation_pooling="candidate-attention"))
    queries = torch.randn(5, 8, requires_grad=True)
    observations = torch.randn(7, 8, requires_grad=True)
    rows = torch.tensor([0, 0, 0, 1, 1, 1, 1])
    counts, splits = torch.tensor([2, 3]), torch.tensor([0, 2, 5])
    actual = scorer._attend_observation(queries, observations, rows, counts, splits)
    expected = []
    for row, (start, end) in enumerate([(0, 2), (2, 5)]):
        values = observations[rows == row]
        weights = (scorer.observation_query(queries[start:end]) @ scorer.observation_key(values).T / 8**0.5).softmax(1)
        expected.append(weights @ values)
    torch.testing.assert_close(actual, torch.cat(expected), rtol=1e-6, atol=1e-6)
    actual.square().sum().backward()
    for value in [queries.grad, observations.grad, scorer.observation_query.weight.grad, scorer.observation_key.weight.grad]:
        assert value is not None and bool(torch.isfinite(value).all())
        assert bool(torch.any(value != 0))


def test_attention_scores_do_not_read_other_rows():
    scorer = _model(observation_pooling="candidate-attention")
    batch = semantic_batch_fixture()
    original = scorer(batch).values
    for row, (start, end) in enumerate([(0, 2), (2, 5)]):
        selected = select_semantic_decision_rows(batch, [row])
        torch.testing.assert_close(scorer(selected).values, original[start:end], rtol=1e-6, atol=1e-6)
    changed = deepcopy(batch)
    changed["semantic"]["scalar"]["value"][2:] += 999
    torch.testing.assert_close(scorer(changed).values[:2], original[:2], rtol=0, atol=0)


def test_attention_duplicate_and_candidate_order_leave_existing_scores_and_critic_unchanged():
    scorer = _model(observation_pooling="candidate-attention")
    batch = _batch()
    original, shared = scorer._forward_encoded(batch)
    duplicate, duplicate_shared = scorer._forward_encoded(_with_duplicate(batch))
    torch.testing.assert_close(shared, duplicate_shared, rtol=1e-6, atol=1e-6)
    torch.testing.assert_close(original.values, duplicate.values[:2], rtol=1e-6, atol=1e-6)
    torch.testing.assert_close(duplicate.values[0], duplicate.values[2], rtol=1e-6, atol=1e-6)
    reversed_batch = deepcopy(batch)
    reversed_batch["semantic"]["candidate_token_indices"] = batch["semantic"]["candidate_token_indices"][::-1].copy()
    torch.testing.assert_close(scorer(reversed_batch).values, original.values.flip(0), rtol=1e-6, atol=1e-6)


def test_attention_configuration_cannot_reinterpret_mean_weights():
    mean = _model()
    attention = _model(observation_pooling="candidate-attention")
    for target, source in [(mean, attention), (attention, mean)]:
        with pytest.raises(TorchPolicyError, match="observation_pooling"):
            load_scorer_warm_start(target, source)
    with pytest.raises(TorchPolicyError, match="flat scoring"):
        RaggedScorerConfig(family_field=1, observation_pooling="candidate-attention")
    for invalid in [None, [], "automatic"]:
        with pytest.raises(TorchPolicyError, match="observation_pooling"):
            RaggedScorerConfig(observation_pooling=invalid)
