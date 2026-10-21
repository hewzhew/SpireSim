from copy import deepcopy

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from sts_learning.torch_policy import (
    RaggedCandidateScorer, RaggedFamilyLogits, RaggedScorerConfig, TorchPolicyError,
    family_target_greedy_logits, ragged_acceptable_set_cross_entropy, ragged_acceptable_family_cross_entropy,
    load_scorer_warm_start,
)
from semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture


def test_family_only_loss_does_not_teach_within_family_targets_or_dilute_with_all_allowed_rows():
    values = torch.tensor([1., 2., 7., -2., 5., 9.], requires_grad=True)
    targets = torch.tensor([80., -90., -92., 200., 30., 60., 90.], requires_grad=True)
    families = RaggedFamilyLogits(values, torch.tensor([0, 3, 5, 6]),
                                 torch.tensor([0, 1, 1, 2, 3, 4, 5]), targets)
    logits = family_target_greedy_logits(families, torch.tensor([0, 4, 6, 7]))
    loss = ragged_acceptable_family_cross_entropy(logits, [(1,), (0, 1), None])
    expected = torch.logsumexp(values[:3], 0) - values[1]
    torch.testing.assert_close(loss, expected)
    # Changing the acceptable target within the same family leaves this task unchanged.
    torch.testing.assert_close(loss, ragged_acceptable_family_cross_entropy(logits, [(2,), (0, 1), None]))
    loss.backward()
    assert values.grad[1] < 0 and torch.all(values.grad[[0, 2]] > 0)
    assert torch.equal(values.grad[3:], torch.zeros(3))
    assert targets.grad is None


def test_family_only_loss_requires_real_contrast_and_valid_complete_menu_mapping():
    families = RaggedFamilyLogits(torch.tensor([1., 2.]), torch.tensor([0, 2]),
                                 torch.tensor([0, 0, 1]), torch.tensor([1., 3., 0.]))
    logits = family_target_greedy_logits(families, torch.tensor([0, 3]))
    for targets in ([(0, 2)], [None]):
        with pytest.raises(TorchPolicyError, match="informative row"):
            ragged_acceptable_family_cross_entropy(logits, targets)
    for targets in ([(True,)], [(0, 0)], [(3,)], []):
        with pytest.raises(TorchPolicyError):
            ragged_acceptable_family_cross_entropy(logits, targets)


def test_family_choice_is_independent_of_target_score_offsets_and_count_penalty():
    families = RaggedFamilyLogits(torch.tensor([1., 2., 0.]), torch.tensor([0, 3]),
                                 torch.tensor([0, 1, 1, 2]), torch.tensor([80., -90., -92., 200.]))
    original = family_target_greedy_logits(families, torch.tensor([0, 4]))
    assert original.greedy_ordinals() == [1]
    torch.testing.assert_close(original.values, torch.tensor([1., 2., 0., 0.]))
    shifted = RaggedFamilyLogits(families.values, families.row_splits, families.candidate_families,
                                torch.tensor([0., 910., 908., -1000.]))
    torch.testing.assert_close(original.values, family_target_greedy_logits(shifted, original.row_splits).values)
    extended = RaggedFamilyLogits(families.values, families.row_splits,
                                 torch.tensor([0, 1, 1, 2, 1, 1]),
                                 torch.tensor([80., -90., -92., 200., -999., -999.]))
    assert family_target_greedy_logits(extended, torch.tensor([0, 6])).greedy_ordinals() == [1]


def test_best_target_preserves_family_score_with_large_target_offset():
    families = RaggedFamilyLogits(torch.tensor([1., 2.]), torch.tensor([0, 2]),
                                 torch.tensor([0, 1, 1]), torch.tensor([1e10, -1e10, -1e10]))
    logits = family_target_greedy_logits(families, torch.tensor([0, 3]))
    assert torch.equal(logits.values, torch.tensor([1., 2., 2.]))
    assert logits.greedy_ordinals() == [1]


@pytest.mark.parametrize("seed", [0, 1, 37])
@pytest.mark.parametrize("pooling", ["vector-max", "candidate-score-max"])
def test_multi_target_family_pooling_is_exact_under_candidate_reversal(seed, pooling):
    torch.manual_seed(seed)
    scorer = RaggedCandidateScorer.from_bridge_schema(
        semantic_schema_fixture(), RaggedScorerConfig(hidden_dim=12, relation_layers=1,
            family_field=1, family_pooling=pooling))
    batch = semantic_batch_fixture()
    # The second decision has three different candidate states in one family.
    batch["semantic"]["categorical"]["value"][4:] = 1
    reversed_batch = deepcopy(batch)
    reversal = np.array([1, 0, 4, 3, 2])
    reversed_batch["semantic"]["candidate_token_indices"] = (
        batch["semantic"]["candidate_token_indices"][reversal])
    original, changed = scorer(batch), scorer(reversed_batch)
    assert torch.equal(changed.families.values, original.families.values)
    assert torch.equal(changed.families.target_values, original.families.target_values[reversal])
    assert torch.equal(changed.values, original.values[reversal])


def test_family_and_target_set_loss_teaches_separate_questions_and_excludes_unknown_rows():
    values = torch.tensor([1., 2., 5., 0.], requires_grad=True)
    targets = torch.tensor([80., -90., -92., 10., -50.], requires_grad=True)
    families = RaggedFamilyLogits(values, torch.tensor([0, 2, 4]),
                                 torch.tensor([0, 1, 1, 2, 3]), targets)
    logits = family_target_greedy_logits(families, torch.tensor([0, 3, 5]))
    loss = ragged_acceptable_set_cross_entropy(logits, [(1,), None])
    expected = torch.logsumexp(values[:2], 0) - values[1]
    expected += torch.logsumexp(targets[1:3], 0) - targets[1]
    torch.testing.assert_close(loss, expected)
    loss.backward()
    assert values.grad[0] > 0 and values.grad[1] < 0
    assert targets.grad[1] < 0 and targets.grad[2] > 0
    torch.testing.assert_close(values.grad[2:], torch.zeros(2))
    torch.testing.assert_close(targets.grad[[0, 3, 4]], torch.zeros(3))


def test_co_acceptable_families_do_not_rank_each_other_and_targets_remain_unordered():
    families = RaggedFamilyLogits(torch.tensor([1., 2.]), torch.tensor([0, 2]),
                                 torch.tensor([0, 1, 1]), torch.tensor([7., -3., 5.]))
    logits = family_target_greedy_logits(families, torch.tensor([0, 3]))
    torch.testing.assert_close(ragged_acceptable_set_cross_entropy(logits, [(0, 1, 2)]), torch.tensor(0.))


def test_multiple_acceptable_families_average_their_own_target_losses():
    values = torch.tensor([1., 2., 4.], requires_grad=True)
    targets = torch.tensor([3., 5., 0., 7., 8.], requires_grad=True)
    families = RaggedFamilyLogits(values, torch.tensor([0, 3]),
                                 torch.tensor([0, 0, 1, 1, 2]), targets)
    logits = family_target_greedy_logits(families, torch.tensor([0, 5]))
    loss = ragged_acceptable_set_cross_entropy(logits, [(0, 2, 3)])
    expected = torch.logsumexp(values, 0) - torch.logsumexp(values[:2], 0)
    expected += (torch.logsumexp(targets[:2], 0) - targets[0]) / 2
    torch.testing.assert_close(loss, expected)
    loss.backward()
    assert torch.all(values.grad[:2] < 0) and values.grad[2] > 0
    assert targets.grad[0] < 0 and targets.grad[1] > 0
    assert torch.equal(targets.grad[2:], torch.zeros(3))


@pytest.mark.parametrize("membership", [torch.tensor([0, 1, 2]), torch.tensor([0, 0, 0]),
                                       torch.tensor([-1, 0, 1]), torch.tensor([0, 1])])
def test_family_composition_rejects_unbound_or_missing_groups(membership):
    families = RaggedFamilyLogits(torch.tensor([1., 2.]), torch.tensor([0, 2]),
                                 membership, torch.tensor([1., 2., 3.]))
    with pytest.raises(TorchPolicyError):
        family_target_greedy_logits(families, torch.tensor([0, 3]))


def test_family_groups_cannot_cross_decision_rows():
    families = RaggedFamilyLogits(torch.tensor([1., 2.]), torch.tensor([0, 1, 2]),
                                 torch.tensor([1, 0]), torch.tensor([1., 2.]))
    with pytest.raises(TorchPolicyError, match="escapes"):
        family_target_greedy_logits(families, torch.tensor([0, 1, 2]))


def test_scorer_groups_only_declared_candidate_field_and_preserves_candidate_order():
    torch.manual_seed(1)
    scorer = RaggedCandidateScorer.from_bridge_schema(
        semantic_schema_fixture(), RaggedScorerConfig(hidden_dim=12, relation_layers=1, family_field=1))
    batch = semantic_batch_fixture()
    logits = scorer(batch)
    assert logits.families is not None
    assert logits.families.candidate_families.tolist() == [0, 1, 3, 2, 3]
    assert logits.families.row_splits.tolist() == [0, 2, 4]
    reordered = deepcopy(batch)
    reordered["semantic"]["candidate_token_indices"] = np.array([2, 1, 7, 6, 5], dtype=np.uint64)
    changed = scorer(reordered)
    torch.testing.assert_close(changed.values, logits.values[[1, 0, 4, 3, 2]])
    restored = RaggedCandidateScorer(scorer.schema, scorer.config)
    restored.load_state_dict(scorer.state_dict(), strict=True)
    torch.testing.assert_close(restored(batch).values, logits.values)


@pytest.mark.parametrize("edit", ["missing", "duplicate"])
def test_scorer_requires_one_declared_family_value_per_candidate(edit):
    scorer = RaggedCandidateScorer.from_bridge_schema(
        semantic_schema_fixture(), RaggedScorerConfig(hidden_dim=12, family_field=1))
    batch = semantic_batch_fixture()
    table = batch["semantic"]["categorical"]
    if edit == "missing":
        for key in table:
            table[key] = np.delete(table[key], 1)
    else:
        for key in table:
            table[key] = np.append(table[key], table[key][1])
    with pytest.raises(TorchPolicyError, match="exactly one"):
        scorer(batch)


def test_warm_start_rejects_different_family_contracts():
    schema = semantic_schema_fixture()
    a = RaggedCandidateScorer.from_bridge_schema(schema, RaggedScorerConfig(hidden_dim=12, family_field=1))
    b = RaggedCandidateScorer.from_bridge_schema(schema, RaggedScorerConfig(hidden_dim=12))
    with pytest.raises(TorchPolicyError, match="family_field"):
        load_scorer_warm_start(a, b)


@pytest.mark.parametrize("value", [None, True, {}, "unknown"])
def test_family_pooling_rejects_unknown_or_unbound_modes(value):
    with pytest.raises(TorchPolicyError, match="family_pooling"):
        RaggedScorerConfig(family_field=1, family_pooling=value)
    with pytest.raises(TorchPolicyError, match="requires family_field"):
        RaggedScorerConfig(family_pooling="candidate-score-max")


def test_scalar_pool_cannot_invent_positive_family_from_negative_real_candidates():
    torch.manual_seed(37)
    legacy = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture(),
        RaggedScorerConfig(hidden_dim=12, relation_layers=1, family_field=1))
    scalar = RaggedCandidateScorer.from_bridge_schema(semantic_schema_fixture(),
        RaggedScorerConfig(hidden_dim=12, relation_layers=1, family_field=1,
            family_pooling="candidate-score-max"))
    scalar.load_state_dict(legacy.state_dict())
    batch = semantic_batch_fixture()
    batch["semantic"]["categorical"]["value"][4:] = 1
    captured = []
    hook = legacy.scorer.register_forward_pre_hook(lambda module, args: captured.append(args[0].detach()))
    legacy(batch)
    hook.remove()
    states = captured[0][2:, :12]
    # Select two real feature coordinates whose maxima belong to different
    # candidates. A vector maximum can combine both, but no real target can.
    pairs = [(i, j) for i in range(12) for j in range(i+1, 12)
             if float(states[:, i].max() + states[:, j].max() -
                      (states[:, i] + states[:, j]).max()) > 1e-5]
    assert pairs, "fixture must contain distinct candidate feature tradeoffs"
    i, j = pairs[0]
    actual_max = (states[:, i] + states[:, j]).max()
    invented = states[:, i].max() + states[:, j].max()
    cutoff = (actual_max + invented) / 2
    for scorer in (legacy, scalar):
        scorer.family_head = torch.nn.Linear(24, 1)
        with torch.no_grad():
            scorer.family_head.weight.zero_()
            scorer.family_head.weight[0, i] = 1
            scorer.family_head.weight[0, j] = 1
            scorer.family_head.bias.fill_(-cutoff)
    with torch.inference_mode():
        assert float(legacy(batch).families.values[-1]) > 0
        assert torch.all(scalar.family_head(captured[0][2:]).squeeze(1) < 0)
        assert float(scalar(batch).families.values[-1]) < 0


def test_warm_start_cannot_reinterpret_pooling_with_same_shaped_weights():
    schema = semantic_schema_fixture()
    original = RaggedCandidateScorer.from_bridge_schema(schema,
        RaggedScorerConfig(hidden_dim=12, family_field=1))
    changed = RaggedCandidateScorer.from_bridge_schema(schema,
        RaggedScorerConfig(hidden_dim=12, family_field=1, family_pooling="candidate-score-max"))
    with pytest.raises(TorchPolicyError, match="family_pooling"):
        load_scorer_warm_start(changed, original)
