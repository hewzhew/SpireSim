from copy import deepcopy
import json
from itertools import permutations

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from learning.tests.semantic_fixtures import semantic_schema_fixture
from sts_learning.semantic_batch import SemanticBatchError, select_semantic_decision_rows
from sts_learning.semantic_information_state import semantic_information_state_id
from sts_learning.torch_policy import RaggedCandidateScorer, RaggedScorerConfig, TorchPolicyError


def _batch():
    return dict(
        slot_indices=np.array([0], dtype=np.uint64), phase=np.array([1], dtype=np.uint8),
        candidate_counts=np.array([2], dtype=np.uint64),
        candidate_row_splits=np.array([0, 2], dtype=np.uint64),
        semantic=dict(
            schema_version=2, completeness=np.array([1], dtype=np.uint8),
            token=dict(row_splits=np.array([0, 6], dtype=np.uint64),
                       kind=np.array([0, 1, 2, 1, 2, 2], dtype=np.uint16),
                       scope=np.array([0, 1, 1, 2, 2, 0], dtype=np.uint32)),
            categorical=dict(token_indices=np.array([1, 3], dtype=np.uint64),
                             field=np.array([1, 1], dtype=np.uint16),
                             value=np.array([0, 1], dtype=np.int64)),
            scalar=dict(token_indices=np.array([0, 2, 4, 5], dtype=np.uint64),
                        field=np.zeros(4, dtype=np.uint16),
                        value=np.array([72, 3, 5, 14], dtype=np.float32)),
            relation=dict(source_token_indices=np.array([0, 1, 1, 0, 3, 3, 0], dtype=np.uint64),
                          relation=np.array([0, 1, 1, 0, 1, 1, 1], dtype=np.uint16),
                          target_token_indices=np.array([1, 2, 5, 3, 4, 5, 5], dtype=np.uint64)),
            candidate_token_indices=np.array([1, 3], dtype=np.uint64)))


def _with_duplicate(batch):
    result = deepcopy(batch)
    s = result["semantic"]
    s["token"]["kind"] = np.append(s["token"]["kind"], [1, 2]).astype(np.uint16)
    s["token"]["scope"] = np.append(s["token"]["scope"], [3, 3]).astype(np.uint32)
    s["token"]["row_splits"][-1] += 2
    c = s["categorical"]
    for column, value in (("token_indices", 6), ("field", 1), ("value", 0)):
        c[column] = np.append(c[column], value).astype(c[column].dtype)
    a = s["scalar"]
    for column, value in (("token_indices", 7), ("field", 0), ("value", 3)):
        a[column] = np.append(a[column], value).astype(a[column].dtype)
    r = s["relation"]
    for column, values in (("source_token_indices", [0, 6, 6]), ("relation", [0, 1, 1]),
                           ("target_token_indices", [6, 7, 5])):
        r[column] = np.append(r[column], values).astype(r[column].dtype)
    s["candidate_token_indices"] = np.append(s["candidate_token_indices"], 6).astype(np.uint64)
    result["candidate_counts"] += 1
    result["candidate_row_splits"][-1] += 1
    return result


def _model(family=False, layers=2, pooling="vector-max", observation_pooling="mean"):
    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(20261011)
        m = RaggedCandidateScorer.from_bridge_schema(
            semantic_schema_fixture(), RaggedScorerConfig(hidden_dim=24, relation_layers=layers,
                                                         family_field=1 if family else None,
                                                         family_pooling=pooling,
                                                         observation_pooling=observation_pooling))
        with torch.no_grad():
            m.scalar_weight.weight.normal_(0, 0.1)
    return m.eval()


@pytest.mark.parametrize("family, pooling", [(False, "vector-max"),
                                             (True, "vector-max"),
                                             (True, "candidate-score-max")])
@pytest.mark.parametrize("layers", [0, 2])
def test_duplicate_candidate_preserves_observation_and_existing_scores(family, layers, pooling):
    m = _model(family, layers, pooling)
    a, b = _batch(), _with_duplicate(_batch())
    select_semantic_decision_rows(b, [0])
    x, xs = m._forward_encoded(a)
    y, ys = m._forward_encoded(b)
    torch.testing.assert_close(xs, ys, rtol=1e-6, atol=1e-6)
    torch.testing.assert_close(x.values, y.values[:2], rtol=1e-6, atol=1e-6)
    torch.testing.assert_close(y.values[0], y.values[2], rtol=1e-6, atol=1e-6)


@pytest.mark.parametrize("observation_pooling", ["mean", "candidate-attention"])
def test_private_payload_changes_its_score_without_rewriting_observation_or_sibling(observation_pooling):
    m = _model(observation_pooling=observation_pooling)
    a, b = _batch(), deepcopy(_batch())
    b["semantic"]["scalar"]["value"][1] = 999
    x, xs = m._forward_encoded(a)
    y, ys = m._forward_encoded(b)
    torch.testing.assert_close(xs, ys, rtol=0, atol=0)
    torch.testing.assert_close(x.values[1], y.values[1], rtol=0, atol=0)
    assert abs(float(x.values[0].detach() - y.values[0].detach())) > 1e-5


def test_observed_condition_still_changes_shared_state_and_candidate_scores():
    m = _model()
    a, b = _batch(), deepcopy(_batch())
    b["semantic"]["scalar"]["value"][0] = 10
    x, xs = m._forward_encoded(a)
    y, ys = m._forward_encoded(b)
    assert float((xs-ys).abs().max().detach()) > 1e-5
    assert float((x.values-y.values).abs().max().detach()) > 1e-5


def test_scope_numbers_are_topology_and_payload_ownership_is_semantic():
    a, renamed = _batch(), deepcopy(_batch())
    scopes = renamed["semantic"]["token"]["scope"]
    renamed["semantic"]["token"]["scope"] = np.where(scopes == 1, 2, np.where(scopes == 2, 1, 0)).astype(np.uint32)
    m = _model()
    torch.testing.assert_close(m(a).values, m(renamed).values, rtol=0, atol=0)
    assert semantic_information_state_id(a, 0) == semantic_information_state_id(renamed, 0)
    changed = deepcopy(a)
    changed["semantic"]["token"]["scope"][2] = 0
    assert semantic_information_state_id(a, 0) != semantic_information_state_id(changed, 0)


def test_cross_candidate_edge_and_missing_owner_are_rejected():
    m = _model()
    invalid = _batch()
    invalid["semantic"]["relation"]["target_token_indices"][1] = 4
    with pytest.raises(TorchPolicyError, match="crosses candidate scopes"):
        m(invalid)
    with pytest.raises(SemanticBatchError, match="crosses candidate scopes"):
        select_semantic_decision_rows(invalid, [0])
    invalid = _batch()
    invalid["semantic"]["token"]["scope"][1] = 2
    with pytest.raises(TorchPolicyError, match="distinct positive scope"):
        m(invalid)
    with pytest.raises(SemanticBatchError, match="scope ownership"):
        select_semantic_decision_rows(invalid, [0])


def test_isolated_observation_is_unchanged_when_private_edges_appear():
    m = _model()
    a, b = _batch(), deepcopy(_batch())
    for batch in (a, b):
        table = batch["semantic"]["relation"]
        for column in table:
            table[column] = table[column][:0]
    b["semantic"]["relation"] = dict(
        source_token_indices=np.array([1], dtype=np.uint64),
        relation=np.array([1], dtype=np.uint16),
        target_token_indices=np.array([2], dtype=np.uint64))
    _, xs = m._forward_encoded(a)
    _, ys = m._forward_encoded(b)
    torch.testing.assert_close(xs, ys, rtol=0, atol=0)


def test_native_campfire_owned_cards_and_upgrade_previews_have_separate_scopes():
    bridge = pytest.importorskip("sts_learning_bridge")
    schema = bridge.semantic_schema()
    recipe = dict(seed=2026101101, ascension_level=20, current_hp=40, max_hp=75, gold=217,
                  boss="Hexaghost", relics=["BurningBlood"],
                  cards=[dict(id="Strike"), dict(id="Defend"), dict(id="DemonForm"),
                         dict(id="LimitBreak"), dict(id="HeavyBlade")])
    env = bridge.LearningBatchEnv.constructed_campfires(json.dumps([recipe]))
    a = env.decision_batch(semantic=True)
    selected = select_semantic_decision_rows(a, [0])
    tokens = selected["semantic"]["token"]
    owned = tokens["kind"] == schema["token_kind"]["Card"]
    previews = tokens["kind"] == schema["token_kind"]["UpgradedCard"]
    assert owned.sum() == len(recipe["cards"])
    assert np.all(tokens["scope"][owned] == 0)
    assert previews.sum() == len(recipe["cards"])
    assert np.all(tokens["scope"][previews] > 0)
    assert len(set(tokens["scope"][previews])) == previews.sum()
    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(20261011)
        m = RaggedCandidateScorer.from_bridge_schema(schema).eval()
    x = m(selected)
    assert bool(torch.isfinite(x.values).all())
    assert len(x.values) == int(a["candidate_counts"][0])
    # This is an interface/ownership check, without labels or adopted actions.


def test_native_reward_offer_order_is_an_execution_address_not_a_policy_feature():
    bridge = pytest.importorskip("sts_learning_bridge")
    schema = bridge.semantic_schema()
    loadout = dict(seed=2026101121, ascension_level=20, current_hp=72, max_hp=80,
        boss="Hexaghost", cards=[dict(id="Bash"), dict(id="PowerThrough"), dict(id="Headbutt")],
        relics=["BurningBlood"])
    recipes = [dict(loadout=loadout, floor=1, offers=[dict(id=card) for card in order])
               for order in permutations(["Headbutt", "Flex", "Hemokinesis"])]
    env = bridge.LearningBatchEnv.constructed_card_rewards(json.dumps(recipes))
    batch = env.decision_batch(semantic=True)
    table = batch["semantic"]["scalar"]
    assert not np.any(table["field"] == schema["scalar_field"]["ActionOptionIndex"])
    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(20261011)
        scorer = RaggedCandidateScorer.from_bridge_schema(schema, RaggedScorerConfig(
            hidden_dim=12, relation_layers=2, family_field=schema["categorical_field"]["ActionKind"])).eval()
    identities, scores = [], []
    for row in range(len(recipes)):
        one = select_semantic_decision_rows(batch, [row])
        identities.append(semantic_information_state_id(one, 0))
        audit = json.loads(env.strategic_decision_audit_json(row))
        with torch.inference_mode():
            logits = scorer(one).values.tolist()
        scores.append({(candidate["kind"], candidate.get("card")): value
                       for candidate, value in zip(audit["candidates"], logits, strict=True)})
        assert [candidate["option_index"] for candidate in audit["candidates"]
                if candidate["kind"] == "take_card"] == [0, 1, 2]
    assert len(set(identities)) == 1
    assert all(score == pytest.approx(scores[0], rel=0, abs=1e-6) for score in scores)
    assert all(max(score, key=score.get) == max(scores[0], key=scores[0].get) for score in scores)
