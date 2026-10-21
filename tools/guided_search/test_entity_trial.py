import copy
import json
import pytest

pytest.importorskip("torch")
pytest.importorskip("sts_learning_bridge")
from tools.guided_search.entity_trial import SCHEMA, summarize, validate
from tools.guided_search import entity_trial as trial


def plan():
    case = {"family":"draw", "source":"explicit constructed recipe", "spec":{
        "seed":1, "deck":[["Strike",0],["Defend",0]], "relics":[], "potions":[]}}
    evaluation = copy.deepcopy(case)
    evaluation["spec"]["seed"] = 100
    evaluation["group"] = "new_seed"
    return dict(schema=SCHEMA, partition="development", question="Does direct RL improve?",
        decision_rule="Stop on input gaps; compare complete outcomes before/after.",
        training=[[case]], evaluation=[evaluation], model_seeds=[7], hidden=16, layers=1,
        max_seconds=30, cohort_seconds=10, learning_rate=.001, gradient_limit=1,
        max_decisions=32, minibatch_size=8, value_weight=.5, entropy_weight=.01)


def test_trial_partitions_reject_seed_leakage_and_renamed_or_reordered_training_recipes():
    p = plan()
    validate(p)
    p["evaluation"][0]["spec"]["seed"] = 1
    with pytest.raises(ValueError, match="overlap"):
        validate(p)
    p = plan()
    p["evaluation"][0]["family"] = "pretend_new"
    p["evaluation"][0]["spec"]["deck"].reverse()
    p["evaluation"][0]["group"] = "new_recipe"
    with pytest.raises(ValueError, match="repeats"):
        validate(p)


def test_trial_reporting_does_not_fold_unknown_into_losses():
    rows = [dict(kind=k, hp=10, decisions=3, engine_steps=5, replay_engine_steps=0)
            for k in ("win", "loss", "unknown_decision_limit")]
    result = summarize(rows)
    assert (result["win"], result["loss"], result["unknown"]) == (1, 1, 1)
    assert result["episodes"] == 3


def warm_source(tmp_path):
    p = plan()
    original = copy.deepcopy(p)
    original["evaluation"][0]["spec"]["seed"] = 900
    tmp_path.mkdir()
    trial.dump(tmp_path / "plan.json", original)
    trial.dump(tmp_path / "summary.json", dict(complete=True, models=[dict(seed=7, complete=True)]))
    trial.dump(tmp_path / "material.json", dict(labels="chosen_actions_only", all_source_seeds=[800]))
    sources = (*trial.INPUT_SOURCES, "crates/sts_guided_search/src/local.rs", "bindings/python_learning/src/guided.rs",
               "tools/guided_search/entity_learning.py")
    trial.dump(tmp_path / "producer.json", dict(sources={s:(trial.ROOT / s).read_text(encoding="utf-8") for s in sources},
               native_sha256=trial.sha(trial.native.__file__), torch=trial.torch.__version__))
    (tmp_path / "model-7").mkdir()
    model = trial.EntityNetwork(p["hidden"], p["layers"]).eval()
    trial.torch.save(model.state_dict(), tmp_path / "model-7/final.pt")
    p["warm_start"] = dict(directory=str(tmp_path), kind="attributed_imitation", value_initialization="zero_logit",
        sha256={n:trial.sha(tmp_path / n) for n in ("plan.json", "producer.json", "summary.json", "material.json", "model-7/final.pt")})
    return p, model


def test_warm_start_pins_source_seeds_numerical_semantics_and_weights(tmp_path):
    p, model = warm_source(tmp_path / "source")
    states = trial.warm_start_states(p)
    assert all(trial.torch.equal(states[7][k], v) for k, v in model.state_dict().items())
    collision = copy.deepcopy(p)
    collision["training"][0][0]["spec"]["seed"] = 800
    with pytest.raises(ValueError, match="overlap"):
        trial.warm_start_states(collision)
    source = tmp_path / "source/producer.json"
    producer = json.loads(source.read_text())
    key = "tools/guided_search/entity_learning.py"
    producer["sources"][key] += "\n# collector-only change\n"
    trial.dump(source, producer)
    with pytest.raises(ValueError, match="evidence changed"):
        trial.warm_start_states(p)
    p["warm_start"]["sha256"]["producer.json"] = trial.sha(source)
    trial.warm_start_states(p)
    producer["sources"][key] = producer["sources"][key].replace("self.value(context)", "self.value(context + 1)")
    trial.dump(source, producer)
    p["warm_start"]["sha256"]["producer.json"] = trial.sha(source)
    with pytest.raises(ValueError, match="network contract"):
        trial.warm_start_states(p)


def test_warm_feedback_trial_keeps_policy_resets_value_and_learns_real_outcomes(tmp_path):
    p, original = warm_source(tmp_path / "source")
    for case in [p["training"][0][0], p["evaluation"][0]]:
        case["spec"].update(encounter="JawWorm", act=1, room="MonsterRoom", hp=68,
                            deck=[["Strike", 0]] * 3, max_potion_uses=None)
    p.update(max_decisions=128, training_behavior_seed=41, evaluation_behavior_seed=43,
             sampling_rng="per_case", max_policy_kl=.02)
    report = trial.run(p, tmp_path / "trial")
    assert report["complete"]
    result = report["models"][0]
    assert len(result["updates"]) == 1 and result["updates"][0]["accepted"]
    initial = trial.torch.load(tmp_path / "trial/model-7/initial.pt", weights_only=True)
    final = trial.torch.load(tmp_path / "trial/model-7/final.pt", weights_only=True)
    for key, t in initial.items():
        assert trial.torch.equal(t, trial.torch.zeros_like(t) if key.startswith("value.") else original.state_dict()[key])
    assert any(not trial.torch.equal(initial[k], v) for k, v in final.items() if k.startswith("value."))
    assert result["final"]["rows"][0]["initial_enemies"] == ["JawWorm"]
    assert result["final"]["all"]["unknown"] == 0
