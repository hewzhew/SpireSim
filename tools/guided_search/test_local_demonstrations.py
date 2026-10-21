import copy
import gzip
import json
import time

import numpy as np
import pytest

torch = pytest.importorskip("torch")
bridge = pytest.importorskip("sts_learning_bridge")
if not hasattr(bridge.GuidedCombatBatch, "demonstration"):
    pytest.skip("rebuild bridge for numerical demonstrations", allow_module_level=True)

from tools.guided_search.local_demonstrations import (
    export_demonstrations, imitate, imitate_groups, load_demonstrations, validate_rescue_sources,
)
from tools.guided_search.local_train import Config, network, prepare, rollout, run as train
from tools.guided_search.local_compare import search, run as compare
from tools.guided_search.local_improve import Improvement, run as improve


def successful_selection():
    deck = [("BurningPact", 0), ("Strike", 1), ("Strike", 1), ("Headbutt", 1), ("PommelStrike", 1)]
    env = bridge.GuidedCombatBatch([(17, "ThreeLouse")], 128, starting_hp=75, task="draw_select",
                                  search_services=256, policy_rollout_steps=128, deck=deck)
    first = True
    while len((f := env.decision_batch())["slots"]):
        cards, actions, refs = f["cards"], f["actions"], f["action_refs"]
        def play(kind):
            return next((i for i, (a, r) in enumerate(zip(actions, refs))
                         if a[0] == 1 and r[0] >= 0 and cards[r[0], kind] == 1), None)
        if first:
            choice, first = play(4), False
        else:
            selections = [i for i, a in enumerate(actions) if np.any(a[2:5] == 1)]
            choice = selections[-1] if selections else next((a for k in (5, 6, 0)
                                                            if (a := play(k)) is not None), None)
            if choice is None:
                choice = next(i for i, a in enumerate(actions) if a[1] == 1)
        weights = np.zeros(len(actions))
        weights[choice] = 1
        env.guide(f["revision"], [weights.tolist()], [None])
    assert json.loads(env.outcomes_json())[0]["kind"] == "win"
    return env


def save_evidence(env, directory):
    directory.mkdir()
    with gzip.open(directory / "evidence.json.gz", "xt", encoding="utf-8") as f:
        f.write(env.evidence_json())
    export_demonstrations(env, directory)


def test_source_bound_win_path_uses_same_numeric_schema_and_no_live_revision(tmp_path):
    env = successful_selection()
    before = env.evidence_json()
    frame = env.demonstration(0)
    assert "revision" not in frame
    assert set(frame) - {"choices", "kind"} == set(env.decision_batch()) - {"revision"}
    data = prepare(frame)
    chosen = data["actions"][torch.arange(len(frame["choices"])), frame["choices"].astype(np.int64)]
    assert all(chosen[:, k].any() for k in (2, 3, 4))
    assert (data["monsters_mask"].sum(1) == 3).all()
    save_evidence(env, tmp_path / "demo")
    loaded = load_demonstrations(tmp_path / "demo", Config(task="draw_select"))
    assert "logp" not in loaded and "old_value" not in loaded
    for key in data:
        torch.testing.assert_close(loaded[key], data[key])
    assert env.evidence_json() == before
    assert loaded["choices"].tolist() == json.loads(before)["episodes"][0]["decision_ordinals"]
    foreign = bridge.GuidedCombatBatch([(18, "ThreeLouse")], 128, task="draw_select")
    with pytest.raises(ValueError, match="differs from the live"):
        export_demonstrations(foreign, tmp_path / "demo")


def test_imitation_supervises_policy_only_and_rejects_wrong_authority(tmp_path):
    torch.set_num_threads(1)
    config = Config(task="draw_select", hidden=16, layers=1)
    save_evidence(successful_selection(), tmp_path / "demo")
    data = load_demonstrations(tmp_path / "demo", config)
    model = network(config).eval()
    before = copy.deepcopy(model.state_dict())
    metrics = imitate(model, torch.optim.Adam(model.parameters(), lr=.001), data, config)
    assert metrics["updated"] and metrics["episodes"] == 1
    assert model.value.weight.grad is None and model.value.bias.grad is None
    assert torch.equal(before["value.weight"], model.value.weight)
    assert any(not torch.equal(v, before[k]) for k, v in model.state_dict().items())
    with pytest.raises(ValueError, match="behavior-policy"):
        imitate(model, None, {**data, "logp": torch.zeros(len(data["choices"]))}, config)
    broken = copy.deepcopy(data)
    broken["choices"][0] = data["mask"].shape[1]
    with pytest.raises(ValueError, match="outside its source"):
        imitate(model, None, broken, config)
    assert not imitate(model, None, None, config)["updated"]
    path = tmp_path / "demo/demonstrations.npz"
    path.write_bytes(path.read_bytes() + b"changed")
    with pytest.raises(ValueError, match="binding changed"):
        load_demonstrations(tmp_path / "demo", config)


def test_unknown_or_loss_cannot_become_positive_demonstrations(tmp_path):
    env = bridge.GuidedCombatBatch([(17, "JawWorm")], 128, starting_hp=1)
    with pytest.raises(ValueError, match="replayed victory"):
        env.demonstration(0)
    frame = env.decision_batch()
    env.advance(frame["revision"], [np.where(frame["action_cards"] == -1)[0].item()])
    assert json.loads(env.outcomes_json())[0]["kind"] == "loss"
    with pytest.raises(ValueError, match="replayed victory"):
        env.demonstration(0)
    save_evidence(env, tmp_path / "loss")
    assert load_demonstrations(tmp_path / "loss", Config()) is None
    config = Config(starting_hp=1, hidden=16, layers=1)
    search(network(config).eval(), [(17, "JawWorm")], config, tmp_path / "unknown",
           time.monotonic() + 30, 1, "policy", 128, demonstrations=True)
    assert load_demonstrations(tmp_path / "unknown", config) is None


def test_improvement_starts_identical_arms_and_finishes_all_declared_evaluations(tmp_path):
    config = Config(train_seed_start=31, train_seed_count=1, eval_seed_start=41, eval_seed_count=1,
                    model_seeds=(117,), generations=1, hidden=16, layers=1,
                    evaluation_partition="development")
    train(config, tmp_path / "training")
    compare(tmp_path / "training", tmp_path / "baseline", budgets=(1,), repeats=1)
    source = tmp_path / "training/model-117/model-1.npz"
    before = source.read_bytes()
    settings = Improvement(51, 1, 61, 1, generations=1, search_services=128, max_wall_seconds=60)
    improve(tmp_path / "training", tmp_path / "improve", settings, tmp_path / "baseline")
    result = json.loads((tmp_path / "improve/result.json").read_text())
    assert result["status"] == "complete"
    assert len(result["evaluation"]) == 6 and len(result["training"]) == 2
    assert all({o["seed"] for o in r["outcomes"]} == {61} for r in result["evaluation"])
    assert source.read_bytes() == before
    for arm in ("direct_rl", "search_imitation"):
        with np.load(source) as a, np.load(tmp_path / f"improve/model-117/{arm}/model-0.npz") as b:
            for key in a.files:
                np.testing.assert_array_equal(a[key], b[key])
    assert json.loads((tmp_path / "improve/regression.json").read_text())[0]["episodes"] == 2
    with pytest.raises(ValueError, match="overlap"):
        Improvement(51, 2, 52, 2).validate(config)
    with pytest.raises(ValueError, match="overlaps"):
        Improvement(31, 1, 61, 1).validate(config)


def test_group_loss_preserves_separate_episode_identity_and_explicit_mass():
    class Policy(torch.nn.Module):
        def __init__(self):
            super().__init__()
            self.logits = torch.nn.Parameter(torch.zeros(2))
        def forward(self, data):
            return self.logits.expand(len(data["slots"]), -1), torch.zeros(len(data["slots"]))
    def data(choice, rows):
        return {"slots": torch.zeros(rows, dtype=torch.int64),
                "choices": torch.full((rows,), choice, dtype=torch.int64),
                "mask": torch.ones((rows, 2), dtype=torch.bool)}
    a, b = data(0, 1), data(1, 3)
    model = Policy()
    optimizer = torch.optim.SGD(model.parameters(), lr=.1)
    metrics = imitate_groups(model, optimizer, [(a, 1), (b, 1)], Config())
    torch.testing.assert_close(model.logits, torch.zeros(2))
    assert metrics["episodes"] == 2 and metrics["rows"] == 4
    assert metrics["policy_loss"] == pytest.approx(np.log(2))
    imitate_groups(model, optimizer, [(a, 1), (b, 3)], Config())
    torch.testing.assert_close(model.logits, torch.tensor([-.025, .025]))
    before = model.logits.detach().clone()
    broken = {**b, "choices": torch.full((3,), 2)}
    with pytest.raises(ValueError, match="outside its source"):
        imitate_groups(model, optimizer, [(a, 1), (broken, 1)], Config())
    torch.testing.assert_close(model.logits, before)
    for groups in ([], [(None, 1)], [(a, 0)], [(a, -1)], [(a, np.nan)], [(a, float("inf"))]):
        with pytest.raises(ValueError, match="positive finite"):
            imitate_groups(model, optimizer, groups, Config())


def test_rescues_require_same_root_and_actual_greedy_loss(tmp_path):
    class ConstructedPolicy(torch.nn.Module):
        def forward(self, data):
            # Test sentinel: attack the single Cultist, deliberately pass against
            # multiple enemies, so search slot 0 must bind to direct slot 1.
            actions = data["actions"]
            logits = actions[:, :, 5] * 100 + actions[:, :, 2:5].sum(-1) * 20 + actions[:, :, 1]
            multiple = data["monsters_mask"].sum(1) > 1
            logits[multiple] = actions[multiple, :, 1]
            logits = logits.masked_fill(~data["mask"], -torch.inf)
            return logits, torch.zeros(len(logits))
    config = Config(task="draw_select", starting_hp=75)
    deck = [("BurningPact", 0), ("Strike", 1), ("Strike", 1), ("Headbutt", 1), ("PommelStrike", 1)]
    _, outcomes, _ = rollout(ConstructedPolicy(), [(11, "Cultist"), (17, "ThreeLouse")], config, torch.Generator(),
                            tmp_path / "direct", time.monotonic() + 30, "greedy", deck=deck,
                            demonstrations=True)
    assert [o["kind"] for o in outcomes] == ["win", "loss"]
    direct_demo = load_demonstrations(tmp_path / "direct", config)
    assert direct_demo["slots"].unique().tolist() == [0]
    assert "logp" not in direct_demo and "old_value" not in direct_demo
    save_evidence(successful_selection(), tmp_path / "search")
    receipt = validate_rescue_sources(tmp_path / "direct", tmp_path / "search")
    assert receipt["mapping"] == [{"search_slot": 0, "direct_slot": 1, "kind": "win"}]
    path = tmp_path / "direct/evidence.json.gz"
    with gzip.open(path, "rt") as f:
        direct = json.load(f)
    changed = copy.deepcopy(direct)
    changed["episodes"][1]["root"]["combat"]["entities"]["player"]["current_hp"] -= 1
    with gzip.open(path, "wt") as f:
        json.dump(changed, f)
    with pytest.raises(ValueError, match="exact failed direct root"):
        validate_rescue_sources(tmp_path / "direct", tmp_path / "search")
    direct["episodes"][1]["outcome"]["kind"] = "unknown_decision_limit"
    with gzip.open(path, "wt") as f:
        json.dump(direct, f)
    with pytest.raises(ValueError, match="unresolved direct root"):
        validate_rescue_sources(tmp_path / "direct", tmp_path / "search")
