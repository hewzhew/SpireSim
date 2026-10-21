import dataclasses
import hashlib
import json
from unittest.mock import Mock, patch

import pytest

torch = pytest.importorskip("torch")

from learning.tests.semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture
from sts_learning.fixed_combat import FixedCombatConfig, FixedCombatStep
from sts_learning.fixed_combat_environment import FixedCombatEnvironment, InstalledFixedCombatRuntime, canonical_json
from sts_learning.policy import BehaviorManifestId
from sts_learning.strategic_rollout import (
    ModelSite, StrategicRunController, load_scoped_model, main, run_rollout,
)
from sts_learning.torch_checkpoints import encode_state_dict
from sts_learning.torch_policy import RaggedCandidateScorer, RaggedScorerConfig
from learning.tests.test_strategic_rollout import RecordingModel
from learning.tests.strategic_scope_fixtures import (
    add_fixture_run_axes, fitted_summary, interface_scope_contract, interface_scopes,
)


@pytest.fixture
def saved_model(tmp_path):
    schema = add_fixture_run_axes(semantic_schema_fixture())
    config = RaggedScorerConfig(hidden_dim=12)
    scorer = RaggedCandidateScorer.from_bridge_schema(schema, config)
    payload = encode_state_dict(scorer.state_dict(), max_bytes=1024*1024)
    root = tmp_path/"model"
    root.mkdir()
    contract = {
        "schema": "sts-scoped-decision-clone-v1", "semantic_schema": schema,
        "scorer": dataclasses.asdict(config), "decision_architecture": "flat",
        "model_sites": ["campfire"], "training_seeds": [1], "evaluation_seeds": [2],
        "applicability": interface_scope_contract(schema, ["campfire"]),
        "training": {"fit_margin": 0.5},
        "initialization": {"source_training_seeds": [3], "source_evaluation_seeds": [4]},
    }
    (root/"contract.json").write_text(json.dumps(contract))
    (root/"summary.json").write_text(json.dumps(fitted_summary(hashlib.sha256(payload).hexdigest())))
    (root/"model.ststorch").write_bytes(payload)
    return root, schema, scorer


def test_frozen_loader_keeps_exact_logits_identity_provenance_and_rng(saved_model):
    root, schema, scorer = saved_model
    original = {p.name: p.read_bytes() for p in root.iterdir()}
    rng = torch.get_rng_state().clone()
    policy, source = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CAMPFIRE,))
    assert torch.equal(torch.get_rng_state(), rng)
    assert not policy.scorer.training
    assert all(not parameter.requires_grad for parameter in policy.scorer.parameters())
    with torch.inference_mode():
        assert torch.equal(scorer(semantic_batch_fixture()).values,
                           policy.scorer(semantic_batch_fixture()).values)
    assert policy.choose(semantic_batch_fixture()).behavior_manifest_id == policy.behavior_manifest_id
    assert source["manifest"]["source_seeds"] == {"training_seeds": [1, 3], "evaluation_seeds": [2, 4]}
    assert source["behavior_id"] == hashlib.sha256(canonical_json(source["manifest"]).encode()).hexdigest()
    assert source["manifest"]["model_sites"] == ["campfire"]
    assert original == {p.name: p.read_bytes() for p in root.iterdir()}
    again, _ = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CAMPFIRE,))
    assert again.behavior_manifest_id == policy.behavior_manifest_id


@pytest.mark.parametrize("family", [False, True])
def test_legacy_contract_without_pooling_keeps_original_vector_semantics(saved_model, family):
    root, schema, scorer = saved_model
    contract = json.loads((root/"contract.json").read_text())
    if family:
        schema["categorical_field"]["ActionKind"] = schema["categorical_field"].pop("Flag")
        config = dataclasses.replace(scorer.config, family_field=1)
        scorer = RaggedCandidateScorer.from_bridge_schema(schema, config)
        payload = encode_state_dict(scorer.state_dict(), max_bytes=1024*1024)
        contract.update(semantic_schema=schema, scorer=dataclasses.asdict(config),
                        decision_architecture="family-target",
                        applicability=interface_scope_contract(schema, ["campfire"]))
        (root/"model.ststorch").write_bytes(payload)
        (root/"summary.json").write_text(json.dumps(fitted_summary(hashlib.sha256(payload).hexdigest())))
    contract["scorer"].pop("family_pooling")
    contract["scorer"].pop("observation_pooling")
    (root/"contract.json").write_text(json.dumps(contract))
    policy, source = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CAMPFIRE,))
    assert policy.scorer.config.family_pooling == "vector-max"
    assert source["manifest"]["family_pooling"] == "vector-max"
    assert policy.scorer.config.observation_pooling == "mean"
    with torch.inference_mode():
        assert torch.equal(scorer(semantic_batch_fixture()).values,
                           policy.scorer(semantic_batch_fixture()).values)


@pytest.mark.parametrize("failure, message", [
    ("hash", "weights are not verified"), ("schema", "semantic schema"),
    ("architecture", "architecture"), ("scope", "scope"),
    ("seed", "seed provenance"), ("roundtrip", "weights are not verified"),
    ("shape", "weights are incompatible"), ("nan", "must be finite"),
    ("unfit", "training fit"), ("gap", "training fit"),
    ("missing_applicability", "applicability"),
])
def test_loader_rejects_invalid_sources(saved_model, failure, message):
    root, schema, scorer = saved_model
    contract = json.loads((root/"contract.json").read_text())
    summary = json.loads((root/"summary.json").read_text())
    if failure == "hash":
        (root/"model.ststorch").write_bytes(b"changed weights")
    elif failure == "schema":
        contract["semantic_schema"]["version"] += 1
    elif failure == "architecture":
        contract["decision_architecture"] = "family-target"
    elif failure == "scope":
        contract["model_sites"] = ["card_reward"]
    elif failure == "seed":
        contract["initialization"]["source_training_seeds"] = [True]
    elif failure == "roundtrip":
        summary["validation"]["model_round_trip"] = False
    elif failure == "unfit":
        summary["stop_reason"] = "max_updates"
    elif failure == "gap":
        summary["final_training"]["minimum_margin"] = 0.1
    elif failure == "missing_applicability":
        contract.pop("applicability")
    else:
        state = scorer.state_dict()
        name = next(iter(state))
        if failure == "shape":
            state.pop(name)
        else:
            state[name].flatten()[0] = float("nan")
        payload = encode_state_dict(state, max_bytes=1024*1024)
        (root/"model.ststorch").write_bytes(payload)
        summary["model_sha256"] = hashlib.sha256(payload).hexdigest()
    (root/"contract.json").write_text(json.dumps(contract))
    (root/"summary.json").write_text(json.dumps(summary))
    with pytest.raises(ValueError, match=message):
        load_scoped_model(root, schema=schema, model_sites=(ModelSite.CAMPFIRE,))


@pytest.mark.parametrize("sites", [(), ("campfire",), (ModelSite.CAMPFIRE,)*2])
def test_saved_model_requires_explicit_distinct_sites(saved_model, sites):
    root, schema, _ = saved_model
    with pytest.raises(ValueError, match="distinct typed"):
        load_scoped_model(root, schema=schema, model_sites=sites)


@pytest.mark.parametrize("flags", [["--model-directory", "model"], ["--model-sites", "campfire"]])
def test_cli_and_api_reject_partial_model_configuration_before_creating_runtime(tmp_path, flags):
    with patch("sts_learning.strategic_rollout.InstalledFixedCombatRuntime") as runtime:
        with pytest.raises(SystemExit) as error:
            main(["--output", str(tmp_path/"out"), "--seed", "17", *flags])
        assert error.value.code == 2
        kwargs = {"model_directory": "model"} if "--model-directory" in flags else {
            "model_sites": (ModelSite.CAMPFIRE,)}
        with pytest.raises(ValueError, match="supplied together"):
            run_rollout(tmp_path/"out", seed=17, **kwargs)
        runtime.assert_not_called()
    assert not (tmp_path/"out").exists()


def test_campfire_only_owner_gets_complete_candidates_without_bootstrap_outputs():
    pytest.importorskip("sts_learning_bridge")
    runtime = InstalledFixedCombatRuntime(FixedCombatConfig(
        backend="whole_combat", generation_work=100_000, wall_ms=3_000, atomic_nodes=None))
    env = runtime.environment([2026103102], 20)
    model = RecordingModel()
    scopes = interface_scopes(runtime.schema, (ModelSite.CAMPFIRE,))
    controller = StrategicRunController(env, model_policy=model, model_sites=(ModelSite.CAMPFIRE,),
                                        model_applicability=scopes)
    audits = []
    for _ in range(60):
        audit = json.loads(env.strategic_decision_audit_json(0))
        if audit["decision_site"] == "campfire":
            break
        result, decisions, _, _ = controller.advance()
        assert result.advanced
        for decision in decisions:
            audit = decision["audit"]
            if audit is not None:
                assert decision["owner"] == "baseline"
    else:
        pytest.fail("did not reach a campfire")
    assert model.batches == []
    actual_step = env.fixed_step
    env.fixed_step = Mock(return_value=FixedCombatStep(None, {"combats": []}, 0))
    result, decisions, _, _ = controller.advance()
    assert not result.advanced
    audits = [decision["audit"] for decision in decisions]
    assert all(decision["owner"] == "model" for decision in decisions)
    assert all(audit["decision_site"] == "campfire" for audit in audits)
    assert len(model.batches) == len(audits) == 1
    batch = model.batches[0]
    assert int(batch["candidate_counts"][0]) == len(audits[0]["candidates"])
    assert not any(key.startswith("legality_bootstrap_") for key in batch)
    assert controller.advance()[1] == decisions
    assert len(model.batches) == 1
    payload = env.checkpoint_bytes(max_bytes=64*1024*1024)
    restored_env = runtime.environment_from_checkpoint(payload, expected_slots=1, max_bytes=64*1024*1024)
    restored = StrategicRunController.from_memory(restored_env, controller.memory(),
        model_policy=model, model_sites=(ModelSite.CAMPFIRE,), model_applicability=scopes)
    env.fixed_step = actual_step
    assert controller.advance()[0].advanced
    assert restored.advance()[0].advanced
    assert len(model.batches) == 1
    assert env.checkpoint_bytes(max_bytes=64*1024*1024) == restored_env.checkpoint_bytes(max_bytes=64*1024*1024)
    committed_memory = controller.memory()
    assert committed_memory["model_slots"] == []
    assert committed_memory["model_slot_sites"] == {}
    StrategicRunController.from_memory(restored_env, committed_memory, model_policy=model,
        model_sites=(ModelSite.CAMPFIRE,), model_applicability=scopes)


def test_rollout_records_adopted_model_source_and_keeps_other_sites_on_baseline(tmp_path):
    native = pytest.importorskip("sts_learning_bridge")
    schema = native.semantic_schema()
    config = RaggedScorerConfig(hidden_dim=12, family_field=schema["categorical_field"]["ActionKind"])
    scorer = RaggedCandidateScorer.from_bridge_schema(schema, config)
    root = tmp_path/"source"
    root.mkdir()
    payload = encode_state_dict(scorer.state_dict(), max_bytes=1024*1024)
    (root/"model.ststorch").write_bytes(payload)
    (root/"contract.json").write_text(json.dumps({
        "schema": "sts-scoped-decision-clone-v1", "semantic_schema": schema,
        "scorer": dataclasses.asdict(config), "decision_architecture": "family-target",
        "model_sites": ["campfire"], "training_seeds": [], "evaluation_seeds": [],
        "applicability": interface_scope_contract(schema, ["campfire"]), "training": {"fit_margin": 0.5}}))
    (root/"summary.json").write_text(json.dumps(fitted_summary(hashlib.sha256(payload).hexdigest())))
    result = run_rollout(tmp_path/"run", seed=17, max_steps=1,
        model_directory=root, model_sites=(ModelSite.CAMPFIRE,), progress=lambda _: None)
    contract = json.loads((tmp_path/"run"/"contract.json").read_text())
    assert contract["controller"]["model_sites"] == ["campfire"]
    source = contract["model_source"]
    assert source["manifest"]["model_sha256"] == hashlib.sha256(payload).hexdigest()
    assert source["behavior_id"] == contract["controller"]["model_id"]
    assert result["committed_model_decisions"] == 0
    event = json.loads((tmp_path/"run"/"steps.jsonl").read_text())
    assert all(decision["owner"] == "baseline" for decision in event["decisions"])


def test_separate_site_models_route_mixed_rows_and_retain_owner_through_restore():
    native = pytest.importorskip("sts_learning_bridge")
    runtime = InstalledFixedCombatRuntime(FixedCombatConfig(
        backend="whole_combat", generation_work=1, wall_ms=1, atomic_nodes=None), final_act=True)
    loadouts = [dict(seed=seed, ascension_level=20, current_hp=71, max_hp=80,
                    boss="Hexaghost", cards=[{"id": "Bash"}], relics=[]) for seed in (17, 18)]
    pool = native.LearningBatchEnv.constructed_card_rewards(json.dumps([
        dict(loadout=loadout, floor=1, offers=[{"id": "Anger"}]) for loadout in loadouts]))
    campfires = native.LearningBatchEnv.constructed_campfires(json.dumps(loadouts))
    pool.restore_slot(1, campfires.checkpoint_slot(1))
    env = FixedCombatEnvironment(pool, runtime.config, runtime.binding)
    env.baseline_manifest = runtime.baseline_manifest
    reward_model, campfire_model = RecordingModel(), RecordingModel()
    campfire_model.behavior_manifest_id = BehaviorManifestId(b"c"*32)
    policies = {ModelSite.CARD_REWARD: reward_model, ModelSite.CAMPFIRE: campfire_model}
    scopes = interface_scopes(runtime.schema, tuple(policies))
    controller = StrategicRunController(env, model_policy=policies, model_applicability=scopes)
    env.fixed_step = Mock(return_value=FixedCombatStep(None, {"combats": []}, 0))
    result, decisions, _, _ = controller.advance()
    assert not result.advanced
    for model, site, slot in ((reward_model, "card_reward", 0), (campfire_model, "campfire", 1)):
        assert len(model.batches) == 1
        assert model.batches[0]["slot_indices"].tolist() == [slot]
        assert not any(k.startswith("legality_bootstrap_") for k in model.batches[0])
        selected = [d for d in decisions if d["slot"] == slot]
        assert len(selected) == 1
        assert selected[0]["audit"]["decision_site"] == site
        assert selected[0]["model_behavior_id"] == model.behavior_manifest_id.digest.hex()
    payload = env.checkpoint_bytes(max_bytes=64*1024*1024)
    restored_env = runtime.environment_from_checkpoint(payload, expected_slots=2, max_bytes=64*1024*1024)
    restored = StrategicRunController.from_memory(restored_env, controller.memory(), model_policy=policies,
                                                  model_applicability=scopes)
    restored_env.fixed_step = Mock(return_value=FixedCombatStep(None, {"combats": []}, 0))
    assert canonical_json(restored.advance()[1]) == canonical_json(decisions)
    assert all(len(model.batches) == 1 for model in policies.values())
    with pytest.raises(ValueError, match="policy binding mismatch"):
        StrategicRunController.from_memory(restored_env, controller.memory(), model_policy={
            ModelSite.CARD_REWARD: campfire_model, ModelSite.CAMPFIRE: reward_model}, model_applicability=scopes)
    broken = dict(controller.memory(), model_slot_sites={"0": "campfire"})
    with pytest.raises(ValueError, match="model-site ownership mismatch"):
        StrategicRunController.from_memory(restored_env, broken, model_policy=policies, model_applicability=scopes)
    swapped = dict(controller.memory(), model_slot_sites={"0": "campfire", "1": "card_reward"})
    with pytest.raises(ValueError, match="pending model ownership mismatch"):
        StrategicRunController.from_memory(restored_env, swapped, model_policy=policies, model_applicability=scopes)


@pytest.mark.parametrize("flags", [
    ["--site-model", "campfire=one", "--site-model", "campfire=two"],
    ["--site-model", "campfire"], ["--site-model", "route=one"],
    ["--site-model", "campfire=one", "--model-directory", "two", "--model-sites", "campfire"],
])
def test_cli_rejects_ambiguous_site_models_before_loading(flags, tmp_path):
    with patch("sts_learning.strategic_rollout.InstalledFixedCombatRuntime") as runtime:
        with pytest.raises(SystemExit):
            main(["--output", str(tmp_path/"out"), "--seed", "17", *flags])
        runtime.assert_not_called()


def test_rollout_binds_each_saved_model_to_its_site(tmp_path):
    pytest.importorskip("sts_learning_bridge")
    paths = {ModelSite.CAMPFIRE: tmp_path/"camp", ModelSite.CARD_REWARD: tmp_path/"reward"}

    def load(directory, *, schema, model_sites):
        site, = model_sites
        assert directory == paths[site]
        model = RecordingModel()
        model.behavior_manifest_id = BehaviorManifestId(bytes([len(site.value)])*32)
        return model, {"behavior_id": model.behavior_manifest_id.digest.hex(), "site": site.value,
                       "manifest": {"applicability": interface_scope_contract(schema, [site])}}

    with patch("sts_learning.strategic_rollout.load_scoped_model", side_effect=load):
        run_rollout(tmp_path/"run", seed=17, max_steps=1,
                    site_model_directories=paths, progress=lambda _: None)
    contract = json.loads((tmp_path/"run/contract.json").read_text())
    assert set(contract["model_source"]) == {site.value for site in paths}
    for site in paths:
        assert contract["controller"]["site_model_ids"][site.value] == contract["model_source"][site.value]["behavior_id"]
