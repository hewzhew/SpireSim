import copy
from types import SimpleNamespace

import numpy as np

import pytest
torch = pytest.importorskip("torch")

from tools.guided_search.potion_timing import HP_SCALE, intervention_targets, objective, encode_source
from tools.guided_search.potion_timing_trial import (
    declared_groups, query_selection, node_replay_cases, timing_query_indices, battle_timing_selection,
)


def reports():
    source = {"combat": {"entities": {"potions": [{"id": "BlockPotion", "uuid": 1}, None]}}}
    common = {"source": "snapshot", "producer": "declared-fresh", "position": source}
    rows = []
    for limit, prefix in [(0, []), (0, [{"UsePotion": {"potion_index": 0, "target": None}}]), (None, [])]:
        rows.append({"case": dict(common, max_potion_uses=limit, prefix=prefix),
                     "root": source, "config": {"max_potion_uses": limit},
                     "implementation_sha256": "current", "information": "full_state_oracle", "plans": []})
    return copy.deepcopy(rows)


def plan(hp, spent):
    return {"outcome": {"final_hp": hp, "total_potion_expenditures": spent}}


def test_flexible_card_only_tactic_is_shared_instead_of_becoming_potion_credit():
    never, now, flexible = reports()
    never["plans"] = [plan(30, 0)]
    now["plans"] = [plan(34, 0)]
    flexible["plans"] = [plan(37, 0), plan(42, 1)]
    _, _, gains, observed = intervention_targets(never, now, flexible)
    assert gains == pytest.approx((-3 / HP_SCALE, 5 / HP_SCALE))
    assert observed == (True, True)


def test_post_prefix_zero_uses_is_not_a_retained_potion_and_known_now_remains_available():
    never, now, flexible = reports()
    never["plans"] = [plan(30, 0)]
    now["plans"] = [plan(42, 0)]
    flexible["plans"] = [plan(34, 1)]
    _, _, gains, observed = intervention_targets(never, now, flexible)
    assert gains == pytest.approx((12 / HP_SCALE, 12 / HP_SCALE))
    assert observed == (True, True)


def test_missing_now_continuation_does_not_teach_death_or_zero_gain():
    never, now, flexible = reports()
    flexible["plans"] = [plan(22, 0), plan(35, 1)]
    _, _, gains, observed = intervention_targets(never, now, flexible)
    assert observed == (False, True)
    predicted = torch.ones((1, 2), requires_grad=True)
    objective(predicted, torch.tensor([gains]), torch.tensor([observed])).backward()
    assert predicted.grad[0, 0] == 0
    assert predicted.grad[0, 1] != 0


def test_no_retained_potion_witness_does_not_invent_a_baseline():
    never, now, flexible = reports()
    now["plans"] = [plan(30, 0)]
    flexible["plans"] = [plan(35, 1)]
    _, _, gains, observed = intervention_targets(never, now, flexible)
    assert observed == (False, False)
    predicted = torch.ones((1, 2), requires_grad=True)
    objective(predicted, torch.tensor([gains]), torch.tensor([observed])).backward()
    assert predicted.grad.abs().sum() == 0


@pytest.mark.parametrize("mutation", ["source", "prefix", "teacher", "constraint", "potion"])
def test_interventions_cannot_mix_other_sources_or_tasks(mutation):
    never, now, flexible = reports()
    if mutation == "source":
        now["case"]["position"] = {"other": True}
    elif mutation == "prefix":
        now["case"]["prefix"] = []
    elif mutation == "teacher":
        flexible["implementation_sha256"] = "older"
    elif mutation == "constraint":
        flexible["config"]["max_potion_uses"] = 0
    else:
        never["case"]["position"]["combat"]["entities"]["potions"][0]["id"] = "PowerPotion"
    with pytest.raises(ValueError):
        intervention_targets(never, now, flexible)


def test_query_budget_does_not_claim_an_unselected_now_witness_as_flexible_result():
    triples = [reports() for _ in range(4)]
    for i, (never, now, flexible) in enumerate(triples):
        for report in (never, now, flexible):
            report["case"]["label"] = f"evaluation/{i}/never"
            report["milestones"] = [{"stats": dict.fromkeys(("search_transitions", "replay_transitions",
                "shuffle_work", "plan_edit_work", "order_family_work", "result_backup_work"), 0)}]
        never["plans"] = [plan(30, 0)]
        now["plans"] = [plan(50 if i == 0 else 30, 0)]
        flexible["plans"] = [plan(45 if i == 0 else 30, 1)]
    columns = {"entity_ids": np.array([[1, 0], [3, 0], [6, 0]]), "entity_values": np.zeros((3, 32))}
    rows = [SimpleNamespace(position=SimpleNamespace(columns=columns), potion_row=2)] * 4
    prediction = np.array([[20, 20], [0, 0], [0, 0], [0, 0]])
    now, timing = query_selection(triples, rows, prediction)
    assert now["observed_hp_gain_captured"] == 20
    assert timing["observed_hp_gain_captured"] == 15


def test_same_declared_seed_cannot_cross_training_and_evaluation():
    cases = [{"label": f"{partition}/73/{arm}"} for partition in ("train", "evaluation")
             for arm in ("never", "now", "flexible")]
    with pytest.raises(ValueError, match="disjoint"):
        declared_groups(cases)


def test_replayed_winning_suffix_supplies_baseline_without_rediscovery():
    never, now, flexible = reports()
    never["plan_consequences"] = [{"frames": [{"boundary": {"kind": "victory"},
        "resources": plan(37, 0)["outcome"]}]}]
    now["plans"] = [plan(40, 0)]
    _, _, gains, observed = intervention_targets(never, now, flexible)
    assert observed == (True, True)
    assert gains == pytest.approx((3 / HP_SCALE, 3 / HP_SCALE))


@pytest.mark.parametrize("boundary", ["player_control", "loss", "escape"])
def test_nonvictory_replay_cannot_be_used_as_winning_baseline(boundary):
    never, now, flexible = reports()
    never["plan_consequences"] = [{"frames": [{"boundary": {"kind": boundary},
        "resources": plan(70, 0)["outcome"]}]}]
    now["plans"] = [plan(40, 0)]
    assert intervention_targets(never, now, flexible)[3] == (False, False)


def test_related_nodes_share_partition_and_duplicate_node_is_rejected():
    cases = [{"label": f"{partition}/{seed}/{node}/{arm}"}
             for partition, seed, node in [("train", 73, 0), ("train", 73, 6), ("evaluation", 211, 0)]
             for arm in ("never", "now", "flexible")]
    assert len(declared_groups(cases)) == 3
    with pytest.raises(ValueError, match="duplicate"):
        declared_groups(cases + cases[:3])
    cases[-3:] = [{"label": f"evaluation/73/9/{arm}"} for arm in ("never", "now", "flexible")]
    with pytest.raises(ValueError, match="disjoint"):
        declared_groups(cases)


def test_source_prefix_and_known_suffix_remain_the_original_exact_inputs():
    inputs = [{"PlayCard": {"card_index": 0, "target": 1}}, "EndTurn",
              {"PlayCard": {"card_index": 1, "target": 1}}]
    report = {"case": {"label": "train/73"}, "config": {"max_potion_uses": 0},
              "plans": [dict(plan(60, 0), inputs=inputs)], "root": {"exact": True},
              "implementation_sha256": "current"}
    nodes = node_replay_cases(report, 2)
    assert [n["label"] for n in nodes] == ["train/73/0", "train/73/2"]
    for node in nodes:
        assert node["prefix"] + node["plan_proposals"][0]["inputs"] == inputs
        assert node["position"] is report["root"]


def test_inference_encoding_needs_no_teacher_outcomes():
    import json
    bridge = pytest.importorskip("sts_learning_bridge")
    spec = {"seed": 73, "encounter": "JawWorm", "act": 1, "room": "MonsterRoom", "hp": 68,
            "deck": [["Strike", 0]] * 5, "relics": [], "potions": ["BlockPotion"], "max_potion_uses": 0}
    env = bridge.GuidedCombatBatch.from_specifications([spec], 128)
    root = json.loads(env.evidence_json())["episodes"][0]["root"]
    row = encode_source(bridge, root)
    assert row.observed == (False, False)
    assert row.position.columns["entity_ids"][row.potion_row, 0] == 6


def test_timing_nominations_are_unique_and_one_battle_is_one_endpoint():
    triples, rows, labels = [], [], []
    for i, (hp, incoming) in enumerate([(40, 1), (55, 20), (33, 0), (30, 0)]):
        triple = reports()
        for arm, report in zip(("never", "now", "flexible"), triple):
            report["case"]["label"] = f"evaluation/73/{i}/{arm}"
            report["plans"] = [plan(hp if arm == "now" else 30, 0)]
            report["milestones"] = [{"stats": dict.fromkeys(("search_transitions", "replay_transitions",
                "shuffle_work", "plan_edit_work", "order_family_work", "result_backup_work"), 0)}]
        triples.append(triple)
        labels.append(triple[0]["case"]["label"])
        values = np.zeros((3, 32))
        values[1, 13], values[2, 2] = incoming / 50., 12 / 20.
        columns = {"entity_ids": np.array([[1, 0], [3, 0], [6, 0]]), "entity_values": values}
        rows.append(SimpleNamespace(position=SimpleNamespace(columns=columns), potion_row=2))
    prediction = np.array([[9., 0.], [1., 0.], [8., 0.], [0., 0.]])
    nominations = timing_query_indices(labels, rows, prediction)
    assert nominations[0][-1] == [1, 0]
    result = battle_timing_selection(triples, rows, prediction)
    assert len(result) == 1
    assert result[0]["observed_hp_gains"] == [25, 10, 25, 25]
    prediction[1, 0] = 10  # Both nominate the same node: spend no duplicate query.
    assert timing_query_indices(labels, rows, prediction)[0][-1] == [1, 0]
