import copy

import pytest

pytest.importorskip("torch")
from tools.guided_search.mechanism_eval import (
    TRAINING_SOURCES, integrate, validate_case, verify_training_sources,
)


def decision(edges):
    n = len(edges)
    return dict(kind="decision", hp=10, edges=edges, choices=[{}] * n, inputs=[{}] * n,
        input=dict(ids=[[1, 0]], values=[[0.] * 32], refs=[[-1] * 3],
                   action_ids=[[2, 0]] * n, actions=[[0.] * 4] * n, action_refs=[[-1] * 3] * n))


def leaf(kind, hp=0):
    return dict(kind=kind, hp=hp, edges=[], choices=[], inputs=[])


def graph(nodes, closed=True):
    return dict(nodes=nodes, closed=closed,
                terminals_replayed=sum(n["kind"] in ("win", "loss") for n in nodes))


def test_full_policy_integration_is_distinct_from_root_preference_and_reachability():
    case = graph([decision([1, 4]), decision([2, 3]), leaf("win", 20), leaf("loss"), leaf("win", 5)])
    validate_case(case)
    result = integrate(case, {0:[.8, .2], 1:[.25, .75]})
    assert result["win_probability"] == pytest.approx(.4)
    assert result["expected_hp_given_win"] == pytest.approx(12.5)
    assert result["greedy_outcome"] == "loss"
    assert result["best_winning_hp"] == 20
    assert result["root_reachable_win"] == [True, True]
    assert result["necessary_win_actions"] is None


def test_cuts_keep_probability_bounds_and_cannot_be_losing_labels():
    # Shared cutoff is referenced by a later decision, as in Rust's node cap.
    case = graph([decision([1, 2]), leaf("unknown_budget"), decision([1, 3]), leaf("win", 6)], False)
    validate_case(case)
    result = integrate(case, {0:[.4, .6], 2:[.5, .5]})
    assert result["win_probability"] is None
    assert result["win_probability_bounds"] == pytest.approx([.3, 1.])
    assert result["unknown_probability"] == pytest.approx(.7)
    assert result["known_loss_probability"] == 0
    assert result["best_reachable_win"] is None
    assert result["necessary_win_actions"] is None
    assert result["expected_hp_given_win"] is None


def test_necessary_action_uses_reachable_victory_not_hp_or_a_forced_all_loss_label():
    case = graph([decision([1, 2]), leaf("loss"), leaf("win", 1)])
    validate_case(case)
    result = integrate(case, {0:[.9, .1]})
    assert result["necessary_win_actions"] == [1]
    assert result["necessary_action_mass"] == .1
    case["nodes"][2] = leaf("loss")
    result = integrate(case, {0:[.9, .1]})
    assert result["necessary_win_actions"] is None
    assert result["best_reachable_win"] is False


@pytest.mark.parametrize("damage,match", [
    (lambda c: c["nodes"][0]["edges"].__setitem__(0, 0), "non-forward"),
    (lambda c: c["nodes"][0]["edges"].__setitem__(0, 99), "outside"),
    (lambda c: c["nodes"][0]["inputs"].pop(), "incomplete"),
    (lambda c: c["nodes"][0]["input"]["actions"].pop(), "invalid"),
    (lambda c: c.update(closed=False), "closure"),
    (lambda c: c.update(terminals_replayed=0), "replay"),
    (lambda c: c["nodes"].append(leaf("loss")), "unreachable"),
])
def test_corrupt_graph_rejected(damage, match):
    case = graph([decision([1, 2]), leaf("loss"), leaf("win", 10)])
    damage(case)
    with pytest.raises(ValueError, match=match):
        validate_case(case)


@pytest.mark.parametrize("policy", [{}, {0:[1.]}, {0:[-.1, 1.1]}, {0:[.2, .2]}, {0:[float("nan"), 0.]}])
def test_missing_or_invalid_policy_rejected(policy):
    case = graph([decision([1, 2]), leaf("loss"), leaf("win", 10)])
    with pytest.raises(ValueError):
        integrate(case, policy)


def test_missing_and_changed_encoder_sources_reject_frozen_weights(tmp_path):
    sources = {name:"source" for name in TRAINING_SOURCES}
    for name, text in sources.items():
        p = tmp_path / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")
    producer = dict(sources=sources)
    verify_training_sources(producer, tmp_path)
    missing = copy.deepcopy(producer)
    missing["sources"].pop(TRAINING_SOURCES[0])
    with pytest.raises(ValueError, match="training source differs"):
        verify_training_sources(missing, tmp_path)
    (tmp_path / TRAINING_SOURCES[0]).write_text("new meaning", encoding="utf-8")
    with pytest.raises(ValueError, match="training source differs"):
        verify_training_sources(producer, tmp_path)
