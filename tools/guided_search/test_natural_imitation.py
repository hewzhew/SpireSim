import copy
from dataclasses import replace
import json

import numpy as np
import pytest
import torch

from tools.guided_search.entity_learning import EntityNetwork
from tools.guided_search.natural_demonstrations import load
from tools.guided_search.natural_imitation import (
    INPUT_SOURCES, ROOT, evaluate, imitate, read_material, seed_weights, sha, summarize, validate,
)
from test_natural_demonstrations import write_dataset


def plan():
    return dict(schema=4, partition="development", information="oracle_observation_no_search_no_sl",
        sampling="equal_run_seed_full_pass", evaluation_policy="sampled_temperature_1",
        question="Does imitation transfer to complete combats?", decision_rule="Report all seeds.",
        evaluation=[dict(id="test", family="constructed", source="explicit test recipe", spec=dict(seed=10))],
        model_seeds=[1, 2], max_seconds=30, cohort_seconds=10, learning_rate=.001, gradient_limit=1.,
        epochs=2, minibatch_size=8, evaluation_batch_size=4, max_decisions=32, hidden=16, layers=1,
        shuffle_seed=23, evaluation_behavior_seed=45)


def example(tmp_path):
    write_dataset(tmp_path)
    p = load(tmp_path)[0].position
    # Two distinct legal action kinds make the imitation gradient observable.
    columns = dict(p.columns, action_ids=np.array([[1, 0], [2, 0]]),
                   actions=np.zeros((2, 4), dtype=np.float32), action_refs=np.full((2, 3), -1))
    return replace(load(tmp_path)[0], position=replace(p, columns=columns))


def test_material_pins_input_sources_and_does_not_count_reproduced_runs_twice(tmp_path):
    write_dataset(tmp_path)
    manifest = tmp_path / "source.json"
    manifest.write_text(json.dumps({"files":[dict(path=p, sha256=sha(ROOT / p)) for p in INPUT_SOURCES]}))
    source = dict(directory=str(tmp_path), source_manifest=str(manifest), source_manifest_sha256=sha(manifest),
                  sha256={p:sha(tmp_path / p) for p in ("producer.json", "trajectory.jsonl", "verification.json")})
    records, seeds = read_material([source])
    assert len(records) == 1 and seeds == {1}
    with pytest.raises(ValueError, match="duplicate run seed"):
        read_material([source, source])
    data = json.loads(manifest.read_text())
    data["files"][0]["sha256"] = "changed"
    manifest.write_text(json.dumps(data))
    source["source_manifest_sha256"] = sha(manifest)
    with pytest.raises(ValueError, match="compatibility"):
        read_material([source])


def test_source_seed_partitions_and_duplicate_evaluation_ids_are_rejected():
    p = plan()
    validate(p, {1, 2, 3})
    with pytest.raises(ValueError, match="overlap"):
        validate(p, {10})
    p["evaluation"].append(copy.deepcopy(p["evaluation"][0]))
    with pytest.raises(ValueError, match="duplicate"):
        validate(p, {1})


def test_constructed_sources_require_explicit_seed_group_weighting():
    p = plan()
    p["demonstrations"] = [dict(kind="constructed_assisted")]
    with pytest.raises(ValueError, match="seed-group"):
        validate(p, {1})
    p["sampling"] = "equal_seed_group_full_pass"
    validate(p, {1})


def test_evaluation_rng_contract_rejects_unknown_modes_and_seed_overflow():
    p = plan()
    p['evaluation_rng'] = 'per_case'
    validate(p, {1})
    p['evaluation_rng'] = 'unqualified'
    with pytest.raises(ValueError, match='RNG'):
        validate(p, {1})
    p['evaluation_rng'] = 'per_case'
    p['evaluation_behavior_seed'] = 2**64
    with pytest.raises(ValueError, match='overflow'):
        validate(p, {1})


def test_independent_case_evaluation_preserves_actual_trajectories_across_batch_sizes(tmp_path):
    pytest.importorskip('sts_learning_bridge')
    torch.set_num_threads(1)
    torch.manual_seed(27)
    p = plan()
    p.update(evaluation_rng='per_case', max_decisions=48)
    cases = []
    for seed, deck, enemy in [(11, [('Strike', 0)] * 5, 'JawWorm'),
                              (13, [('Defend', 0)] * 2, 'Cultist'),
                              (17, [('Strike', 0)] * 3 + [('Bash', 0)], 'TwoLouse')]:
        cases.append(dict(id=str(seed), family='diagnostic', spec=dict(seed=seed, encounter=enemy,
            act=1, room='MonsterRoom', hp=10, deck=deck, relics=[], potions=[], max_potion_uses=None)))
    model = EntityNetwork(hidden=16, layers=1).eval()
    separate = evaluate(model, cases, dict(p, evaluation_batch_size=1), tmp_path/'separate', lambda: 30.)
    together = evaluate(model, cases, dict(p, evaluation_batch_size=3), tmp_path/'together', lambda: 30.)
    assert separate['evaluation_rng'] == together['evaluation_rng'] == 'per_case'
    assert separate['all'] == together['all']
    import gzip
    with gzip.open(tmp_path/'together/batch-0000/evidence.json.gz', 'rt') as f:
        mixed = json.load(f)['episodes']
    assert len({e['outcome']['decisions'] for e in mixed}) > 1
    for i, actual in enumerate(mixed):
        with gzip.open(tmp_path/f'separate/batch-{i:04}/evidence.json.gz', 'rt') as f:
            single, = json.load(f)['episodes']
        for key in ('root', 'inputs', 'successor', 'decision_ordinals'):
            assert single[key] == actual[key]
        assert dict(single['outcome'], slot=i) == actual['outcome']


def test_full_pass_gradient_is_equal_seed_weighted_and_independent_of_packing(tmp_path):
    torch.set_num_threads(1)
    torch.manual_seed(17)
    e = example(tmp_path)
    examples = [e, e, e, replace(e, seed=2, selected=1)]
    np.testing.assert_allclose(seed_weights(examples), [1/6, 1/6, 1/6, 1/2])
    first = EntityNetwork(hidden=16, layers=1).eval()
    second = copy.deepcopy(first)
    value_before = copy.deepcopy(first.value.state_dict())
    a = imitate(first, examples, minibatch_size=1,
                optimizer=torch.optim.SGD(first.parameters(), lr=.05))
    b = imitate(second, examples, minibatch_size=4, order=np.array([3, 2, 1, 0]),
                optimizer=torch.optim.SGD(second.parameters(), lr=.05))
    assert a["loss"] == pytest.approx(b["loss"], abs=1e-6)
    for x, y in zip(first.parameters(), second.parameters()):
        torch.testing.assert_close(x, y, atol=2e-7, rtol=1e-5)
    for name, value in first.value.state_dict().items():
        torch.testing.assert_close(value, value_before[name], atol=0, rtol=0)
    assert all(p.grad is None for p in first.value.parameters())


def test_learning_increases_demonstrated_action_likelihood_and_invalid_target_cannot_step(tmp_path):
    torch.set_num_threads(1)
    torch.manual_seed(23)
    e = example(tmp_path)
    model = EntityNetwork(hidden=16, layers=1).eval()
    optimizer = torch.optim.Adam(model.parameters(), lr=.003)
    before = imitate(model, [e], minibatch_size=1)
    for _ in range(4):
        imitate(model, [e], minibatch_size=1, optimizer=optimizer)
    after = imitate(model, [e], minibatch_size=1)
    assert after["target_probability"] > before["target_probability"]
    state = copy.deepcopy(model.state_dict())
    with pytest.raises(ValueError, match="outside"):
        imitate(model, [e, replace(e, selected=3)], minibatch_size=1, optimizer=optimizer)
    for name, value in model.state_dict().items():
        torch.testing.assert_close(value, state[name], atol=0, rtol=0)


def test_resources_and_unknown_stay_separate():
    rows = [dict(kind="win", hp=20, starting_hp=30, potion_uses=1, potions=[None, "FirePotion"]),
            dict(kind="loss"), dict(kind="unknown_decision_limit")]
    s = summarize(rows)
    assert (s["cases"], s["win"], s["loss"], s["unknown"]) == (3, 1, 1, 1)
    assert s["mean_winning_net_hp_cost"] == 10
    assert s["mean_winning_potions_remaining"] == 1
    assert s["mean_winning_potion_uses"] == 1


def test_budget_interruption_keeps_full_declared_evaluation_denominator(tmp_path):
    pytest.importorskip("sts_learning_bridge")
    p = plan()
    cases = [dict(id=str(i), family="test", spec=dict(seed=i, encounter="JawWorm")) for i in range(3)]
    def expired():
        raise TimeoutError("test deadline")
    with pytest.raises(TimeoutError):
        evaluate(None, cases, p, tmp_path / "incomplete", expired)
    report = json.loads((tmp_path / "incomplete/summary.json").read_text())
    assert report["complete"] is False
    assert report["all"]["cases"] == report["all"]["unknown"] == 3
    assert report["all"]["loss"] == 0
    assert report["enemy_compositions"] == []
    assert report["unidentified_enemies"]["cases"] == 3


def test_evaluation_reports_actual_pool_members_and_keeps_duplicate_monsters(tmp_path):
    pytest.importorskip("sts_learning_bridge")
    torch.set_num_threads(1)
    torch.manual_seed(9)
    p = plan()
    p["max_decisions"] = 1
    cases = []
    for seed, encounter in [(3689424591, "LargeSlime"), (4283847167, "LargeSlime"), (73, "ThreeSentries")]:
        spec = dict(seed=seed, encounter=encounter, act=1, room="MonsterRoom", hp=68,
                    deck=[("Strike", 0)] * 5 + [("Defend", 0)] * 4 + [("Bash", 0), ("AscendersBane", 0)],
                    relics=[], potions=[], max_potion_uses=None)
        cases.append(dict(id=str(seed), family="test", spec=spec))
    report = evaluate(EntityNetwork(hidden=16, layers=1).eval(), cases, p, tmp_path / "evaluation", lambda: 30.)
    assert report["complete"] is True
    assert [r["initial_enemies"] for r in report["rows"]] == [
        ["SpikeSlimeL"], ["AcidSlimeL"], ["Sentry", "Sentry", "Sentry"]]
    assert report["encounters"]["LargeSlime"]["cases"] == 2
    groups = report["enemy_compositions"]
    assert [g["enemies"] for g in groups] == [["AcidSlimeL"], ["Sentry"] * 3, ["SpikeSlimeL"]]
    assert all(g["cases"] == g["unknown"] == 1 for g in groups)
    assert report["unidentified_enemies"]["cases"] == 0
    assert sum(g["cases"] for g in groups) == report["all"]["cases"]
