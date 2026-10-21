import copy
import json
import time

import numpy as np
import pytest

torch = pytest.importorskip("torch")
bridge = pytest.importorskip("sts_learning_bridge")
if not hasattr(bridge, "GuidedCombatBatch"):
    pytest.skip("rebuild the optional bridge for tactical tests", allow_module_level=True)
from tools.guided_search.local_train import Config, cases, network, prepare, rollout, update
from tools.guided_search.local_compare import search
from tools.guided_search.local_compare import EvaluationPanel, repeated_policy, run as compare
from tools.guided_search.local_train import run as train


def frame():
    return bridge.GuidedCombatBatch([(17, "ThreeLouse"), (29, "JawWorm")], 128,
                                    task="draw_select").decision_batch()


def test_entities_targets_and_padding_remain_distinct_and_control_free():
    b = frame()
    data = prepare(b)
    assert data["cards"].shape == (2, 16, 24)
    assert data["monsters_mask"].sum(1).tolist() == [3, 1]
    assert data["cards_mask"].sum(1).tolist() == [15, 15]
    by_card = {}
    for r in b["action_refs"][:b["action_splits"][1]]:
        if r[1] >= 0:
            by_card.setdefault(r[0], []).append(r[1])
    assert by_card and all(targets == [0, 1, 2] for targets in by_card.values())
    b["slots"][:] = 123456
    b["revision"] += 100
    for key, value in prepare(b).items():
        torch.testing.assert_close(value, data[key])
    torch.set_num_threads(1)
    model = network(Config(task="draw_select", hidden=16, layers=1)).eval()
    changed = copy.deepcopy(data)
    changed["cards"][~data["cards_mask"]] = 100
    changed["monsters"][~data["monsters_mask"]] = -100
    with torch.no_grad():
        a, v = model(data)
        b, w = model(changed)
    torch.testing.assert_close(a, b)
    torch.testing.assert_close(v, w)


def test_entity_bounds_and_schema_mismatch_fail_instead_of_truncating():
    b = frame()
    b["action_refs"][0, 1] = 3
    with pytest.raises(ValueError, match="absent entity"):
        prepare(b)
    b = frame()
    b["card_splits"][1] = 17
    with pytest.raises(ValueError, match="capacity"):
        prepare(b)
    with pytest.raises(ValueError, match="outside"):
        bridge.GuidedCombatBatch([(17, "ThreeLouse")], 128)
    with pytest.raises(ValueError, match="1..32"):
        Config(task="draw_select", train_seed_count=33).validate()


def test_tactical_rollouts_learn_only_from_complete_actual_episodes(tmp_path):
    torch.set_num_threads(1)
    torch.manual_seed(117)
    config = Config(task="draw_select", starting_hp=24, hidden=16, layers=1)
    model = network(config).eval()
    records, outcomes, _ = rollout(model, cases(range(31, 33), config.task), config,
        torch.Generator().manual_seed(51), tmp_path / "train", time.monotonic() + 30)
    assert len(outcomes) == 8
    assert all(o["kind"] in ("win", "loss") for o in outcomes)
    assert any((r["actions"][:, :, 3] > 0).any() for r in records), "must actually encounter selection members"
    before = copy.deepcopy(model.state_dict())
    update(model, torch.optim.Adam(model.parameters(), lr=.001), records, outcomes, config)
    assert any(not torch.equal(v, before[k]) for k, v in model.state_dict().items())
    result, _ = search(model, [(31, "ThreeLouse")], config, tmp_path / "search",
                       time.monotonic() + 30, 128, "policy_value", 64)
    assert result[0]["kind"] == "win" or result[0]["kind"].startswith("unknown")
    if result[0]["kind"] == "win":
        assert result[0]["win_replayed"]
    assert result[0]["services"] <= 128


def test_tactical_rejected_batch_never_partially_advances():
    env = bridge.GuidedCombatBatch([(17, "ThreeLouse"), (29, "JawWorm")], 128, task="draw_select")
    b = env.decision_batch()
    before = env.evidence_json()
    with pytest.raises(ValueError, match="outside its source"):
        env.advance(b["revision"], [0, 999])
    assert env.evidence_json() == before
    env.advance(b["revision"], [0, 0])
    with pytest.raises(ValueError, match="stale"):
        env.advance(b["revision"], [0, 0])
    assert json.loads(before)["schema"] == 2


def test_custom_deck_padding_and_exact_recipe_reach_all_search_attempts(tmp_path):
    deck = [("Strike", 1), ("Strike", 0), ("Defend", 1), ("PommelStrike", 1),
            ("ShrugItOff", 1), ("AscendersBane", 0)]
    env = bridge.GuidedCombatBatch([(31, "Cultist")], 128, task="draw_select", deck=deck)
    b = env.decision_batch()
    data = prepare(b)
    assert data["cards_mask"].sum().item() == 6
    assert json.loads(env.evidence_json())["episodes"][0]["deck_recipe"] == [list(c) for c in deck]
    torch.set_num_threads(1)
    config = Config(task="draw_select", starting_hp=1, max_decisions=128, hidden=16, layers=1)
    model = network(config).eval()
    repeated_policy(model, [(31, "Cultist")], config, tmp_path / "repeated",
                    time.monotonic() + 30, 2, 117, deck=deck)
    import gzip
    for path in (tmp_path / "repeated").glob("attempt-*/evidence.json.gz"):
        with gzip.open(path, "rt") as f:
            evidence = json.load(f)
        assert evidence["episodes"][0]["deck_recipe"] == [list(c) for c in deck]
    result, _ = search(model, [(31, "Cultist")], config, tmp_path / "search",
                       time.monotonic() + 30, 2, "policy_value", 128, deck=deck)
    assert result[0]["kind"].startswith("unknown")
    with gzip.open(tmp_path / "search/evidence.json.gz", "rt") as f:
        assert json.load(f)["episodes"][0]["deck_recipe"] == [list(c) for c in deck]


def test_new_deck_evaluation_keeps_regression_on_original_training_recipe(tmp_path):
    config = Config(task="draw_select", starting_hp=68, train_seed_start=31, train_seed_count=1,
                    eval_seed_start=41, eval_seed_count=1, model_seeds=(117,), generations=1,
                    hidden=16, layers=1, evaluation_partition="development")
    train(config, tmp_path / "training")
    compare(tmp_path / "training", tmp_path / "baseline", budgets=(1,), repeats=1)
    panel = EvaluationPanel(**json.loads('{"seed_start":51,"seed_count":1,"partition":"development","deck":[["Strike",1],["Defend",0],["Bash",1]]}'))
    compare(tmp_path / "training", tmp_path / "changed", budgets=(1,), repeats=1,
            regression_baseline=tmp_path / "baseline", evaluation_panel=panel)
    import gzip
    with gzip.open(tmp_path / "changed/model-117/regression/evidence.json.gz", "rt") as f:
        assert all(e["deck_recipe"] is None for e in json.load(f)["episodes"])
    with gzip.open(tmp_path / "changed/model-117/greedy/evidence.json.gz", "rt") as f:
        assert all(e["deck_recipe"] == [list(c) for c in panel.deck] for e in json.load(f)["episodes"])
    contract = json.loads((tmp_path / "changed/contract.json").read_text())
    assert "deck" not in contract["config"]
    assert contract["evaluation_panel"]["deck"] == [list(c) for c in panel.deck]
    assert len(json.loads((tmp_path / "changed/regression.json").read_text())) == 1
    with pytest.raises(ValueError, match="requires draw_select"):
        panel.validate(Config())


def test_native_deck_accepts_json_pairs_and_rejects_malformed_entries():
    recipe = [["Strike", 1], ["Defend", 0], ["AscendersBane", 0]]
    json_env = bridge.GuidedCombatBatch([(31, "Cultist")], 128, task="draw_select", deck=recipe)
    tuple_env = bridge.GuidedCombatBatch([(31, "Cultist")], 128, task="draw_select", deck=[tuple(c) for c in recipe])
    assert json_env.evidence_json() == tuple_env.evidence_json()
    for entry in [["Strike"], ["Strike", 0, "extra"], {0:"Strike", 1:0}]:
        with pytest.raises(ValueError, match="pair"):
            bridge.GuidedCombatBatch([(31, "Cultist")], 128, task="draw_select", deck=[entry])
