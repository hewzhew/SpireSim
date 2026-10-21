import copy
import json
from unittest.mock import Mock, patch

import pytest

from learning.tests.strategic_scope_fixtures import interface_scope_contract
from learning.tests.test_strategic_rollout import RecordingModel
from sts_learning.fixed_combat import FixedCombatConfig
from sts_learning.fixed_combat_environment import FixedCombatEnvironment, InstalledFixedCombatRuntime
from sts_learning.strategic_model_scope import DECISION_CHOICE_TARGET, StrategicModelApplicability, require_training_fit
from sts_learning.strategic_rollout import ModelSite, StrategicModelOutOfScope, StrategicRunController, run_rollout


def _fit_contract(*, preferred=True):
    contract = {"training": {"fit_margin": 0.5}}
    metrics = {"supervised_rows": 1, "minimum_margin": None,
               "choices": {"adoptable": 1, "outside_set": 0, "unknown": 1}}
    if preferred:
        contract.update(choice_target=DECISION_CHOICE_TARGET, preferred_training_rows=1)
        metrics.update(preferred_rows=1, minimum_recommendation_margin=1., recommendations={
            "recommended": 1, "secondary_acceptable": 0, "outside_set": 0, "unknown": 1})
    return contract, {"stop_reason": "training_fit", "final_training": metrics}


def test_fit_gate_preserves_legacy_acceptability_and_requires_declared_recommendations():
    for preferred in (False, True):
        contract, summary = _fit_contract(preferred=preferred)
        require_training_fit(contract, summary)


@pytest.mark.parametrize("failure", ["secondary", "margin", "nan", "missing_margin", "preferred_count",
    "unknown_count", "missing_target", "unknown_target", "null_target", "missing_preferred_count"])
def test_fit_gate_cannot_accept_secondary_choices_or_downgrade_preferred_contract(failure):
    contract, summary = _fit_contract()
    metrics = summary["final_training"]
    if failure == "secondary":
        metrics["recommendations"].update(recommended=0, secondary_acceptable=1)
    elif failure == "margin": metrics["minimum_recommendation_margin"] = 0.1
    elif failure == "nan": metrics["minimum_recommendation_margin"] = float("nan")
    elif failure == "missing_margin": del metrics["minimum_recommendation_margin"]
    elif failure == "preferred_count": metrics["preferred_rows"] = 0
    elif failure == "unknown_count": metrics["recommendations"]["unknown"] = 0
    elif failure == "missing_target": del contract["choice_target"]
    elif failure == "unknown_target": contract["choice_target"] = "guessed-rank"
    elif failure == "null_target": contract["choice_target"] = None
    else: del contract["preferred_training_rows"]
    with pytest.raises(ValueError, match="training fit"):
        require_training_fit(contract, summary)


@pytest.fixture
def decision():
    native = pytest.importorskip("sts_learning_bridge")
    schema = native.semantic_schema()
    loadout = dict(seed=2026101107, ascension_level=20, current_hp=43, max_hp=75,
                   gold=117, boss="Hexaghost", cards=[{"id": "Bash"}], relics=["BurningBlood"])
    env = native.LearningBatchEnv.constructed_card_rewards(json.dumps([
        dict(loadout=loadout, floor=3, offers=[{"id": "SeeingRed"}])]))
    return schema, env.decision_batch(semantic=True)


def _scope(schema, *, change=None):
    value = interface_scope_contract(schema, ["card_reward"], acts=[1])
    if change:
        change(value["sites"]["card_reward"])
    return StrategicModelApplicability.from_contract(value, schema=schema, model_sites=["card_reward"])


def test_complete_surface_is_eligible_only_as_declared_and_declaration_is_immutable(decision):
    schema, batch = decision
    value = interface_scope_contract(schema, ["card_reward"])
    scope = StrategicModelApplicability.from_contract(value, schema=schema, model_sites=["card_reward"])
    assert scope.issues("card_reward", batch) == ([],)
    value["sites"]["card_reward"]["acts"] = [4]
    scope.contract()["sites"]["card_reward"]["acts"] = [4]
    assert scope.issues("card_reward", batch) == ([],)


@pytest.mark.parametrize("field,value,code", [
    ("Act", 2, "run_axis_outside_scope"), ("Floor", 101, "run_axis_outside_scope"),
    ("AscensionLevel", 21, "run_axis_outside_scope"), ("Act", 1.5, "unobservable_run_axis"),
])
def test_run_axes_and_unknown_are_checked_from_shared_run_tokens(decision, field, value, code):
    schema, batch = decision
    batch = copy.deepcopy(batch)
    table = batch["semantic"]["scalar"]
    mask = table["field"] == schema["scalar_field"][field]
    table["value"][mask] = value
    assert {"code": code, "field": field, **({"observed": value} if code.startswith("run_axis") else {})} in (
        _scope(schema).issues("card_reward", batch)[0])


def test_missing_axis_and_incomplete_input_cannot_be_scored_as_supported(decision):
    schema, batch = decision
    batch = copy.deepcopy(batch)
    table = batch["semantic"]["scalar"]
    keep = table["field"] != schema["scalar_field"]["Floor"]
    for key in table:
        table[key] = table[key][keep]
    batch["semantic"]["completeness"][0] = schema["completeness"]["NotEncoded"]
    issues = _scope(schema).issues("card_reward", batch)[0]
    assert {"code": "unobservable_run_axis", "field": "Floor"} in issues
    assert {"code": "incomplete_semantic_observation"} in issues


def test_private_candidate_mechanic_and_action_identity_are_in_the_boundary(decision):
    schema, batch = decision
    gain = schema["card_mechanic_role"]["GainEnergy"]
    scope = _scope(schema, change=lambda s: s["categorical_domains"]["CardMechanicRole"].remove(gain))
    assert {"code": "categorical_domain_outside_scope", "field": "CardMechanicRole", "observed": [gain]} in (
        scope.issues("card_reward", batch)[0])
    # An identity appearing only on the action payload still uses CardId's domain.
    batch = copy.deepcopy(batch)
    table = batch["semantic"]["categorical"]
    field = schema["categorical_field"]["ActionCardId"]
    number = int(table["value"][table["field"] == field][0])
    keep = ~((table["field"] == schema["categorical_field"]["CardId"]) & (table["value"] == number))
    for key in table:
        table[key] = table[key][keep]
    scope = _scope(schema, change=lambda s: s["categorical_domains"]["CardId"].remove(number))
    assert {"code": "categorical_domain_outside_scope", "field": "CardId", "observed": [number]} in (
        scope.issues("card_reward", batch)[0])


def test_additional_action_domain_can_narrow_but_never_widen_identity_scope(decision):
    schema, batch = decision
    scope = _scope(schema, change=lambda s: s["categorical_domains"].update(ActionCardId=[]))
    assert scope.issues("card_reward", batch)[0]


def test_owned_identity_scope_is_separate_from_offered_and_generated_identities(decision):
    schema, batch = decision
    table = batch["semantic"]["categorical"]
    kinds = batch["semantic"]["token"]["kind"]
    card_field = schema["categorical_field"]["CardId"]
    owned = {int(number) for index, field, number in zip(
        table["token_indices"], table["field"], table["value"], strict=True)
        if field == card_field and kinds[index] == schema["token_kind"]["Card"]}
    assert len(owned) == 1
    scope = _scope(schema, change=lambda s: s.update(
        observation_categorical_domains={"Card": {"CardId": sorted(owned)}}))
    # SeeingRed is a legal offer, but not in the declared owned-card domain.
    assert scope.issues("card_reward", batch) == ([],)
    changed = copy.deepcopy(batch)
    table = changed["semantic"]["categorical"]
    offered = next(int(number) for index, field, number in zip(
        table["token_indices"], table["field"], table["value"], strict=True)
        if field == card_field and kinds[index] == schema["token_kind"]["OfferedCard"])
    for i, (index, field) in enumerate(zip(table["token_indices"], table["field"], strict=True)):
        if field == card_field and kinds[index] == schema["token_kind"]["Card"]:
            table["value"][i] = offered
    assert {"code": "observation_categorical_domain_outside_scope", "token_kind": "Card",
            "field": "CardId", "observed": [offered]} in scope.issues("card_reward", changed)[0]


def test_observation_domain_is_a_direct_field_constraint_and_cannot_widen_global_domain(decision):
    schema, batch = decision
    table = batch["semantic"]["categorical"]
    offered = int(table["value"][table["field"] == schema["categorical_field"]["ActionCardId"]][0])
    scope = _scope(schema, change=lambda s: (
        s["categorical_domains"]["CardId"].remove(offered),
        s.update(observation_categorical_domains={"OfferedCard": {"CardId": [offered]}})))
    assert {"code": "categorical_domain_outside_scope", "field": "CardId", "observed": [offered]} in (
        scope.issues("card_reward", batch)[0])


def test_observation_domain_does_not_apply_to_private_payloads_or_change_legacy_scope(decision):
    schema, batch = decision
    scope = _scope(schema, change=lambda s: s.update(
        observation_categorical_domains={"Candidate": {"ActionCardId": []}}))
    assert scope.issues("card_reward", batch) == ([],)
    assert "observation_categorical_domains" not in _scope(schema).contract()["sites"]["card_reward"]


def test_owned_card_domain_does_not_turn_its_creation_request_into_inventory():
    native = pytest.importorskip("sts_learning_bridge")
    schema = native.semantic_schema()
    env = native.LearningBatchEnv.constructed_card_rewards(json.dumps([dict(
        loadout=dict(seed=17, ascension_level=20, current_hp=75, max_hp=75,
            boss="Hexaghost", cards=[dict(id="WildStrike")], relics=[]),
        floor=3, offers=[dict(id="Anger")])]))
    batch = env.decision_batch(semantic=True)
    cats = batch["semantic"]["categorical"]
    kinds = batch["semantic"]["token"]["kind"]
    owned = {int(number) for index, field, number in zip(
        cats["token_indices"], cats["field"], cats["value"], strict=True)
        if field == schema["categorical_field"]["CardId"] and kinds[index] == schema["token_kind"]["Card"]}
    status = set(map(int, cats["value"][cats["field"] == schema["categorical_field"]["CardStatusCreationCardId"]]))
    assert owned and status and not owned & status
    scope = _scope(schema, change=lambda s: s.update(
        observation_categorical_domains={"Card": {"CardId": sorted(owned)}}))
    assert scope.issues("card_reward", batch) == ([],)


@pytest.mark.parametrize("domains", [None, [], {"InventedToken": {"CardId": []}},
    {"Card": {}}, {"Card": []}, {"Card": {"InventedField": []}},
    {"Card": {"CardId": [True]}}, {"Card": {"CardId": [9999]}},
    {"Card": {"CardId": [1, 1]}}])
def test_invalid_observation_domains_are_rejected(decision, domains):
    schema, _ = decision
    with pytest.raises(ValueError, match="applicability"):
        _scope(schema, change=lambda s: s.update(observation_categorical_domains=domains))


def test_generated_status_identity_uses_card_domain_without_being_owned():
    native = pytest.importorskip("sts_learning_bridge")
    schema = native.semantic_schema()
    env = native.LearningBatchEnv.constructed_card_rewards(json.dumps([dict(
        loadout=dict(seed=17, ascension_level=20, current_hp=75, max_hp=75,
            boss="Hexaghost", cards=[dict(id="Bash")], relics=[]),
        floor=3, offers=[dict(id="WildStrike")])]))
    batch = env.decision_batch(semantic=True)
    categories = batch["semantic"]["categorical"]
    status, = set(map(int, categories["value"][categories["field"] ==
        schema["categorical_field"]["CardStatusCreationCardId"]]))
    # Wound exists only as a creation request, not as an owned/offered card.
    assert status not in categories["value"][categories["field"] ==
        schema["categorical_field"]["CardId"]]
    scope = _scope(schema, change=lambda s: s["categorical_domains"]["CardId"].remove(status))
    assert {"code": "categorical_domain_outside_scope", "field": "CardId",
            "observed": [status]} in scope.issues("card_reward", batch)[0]
    # An explicit alias domain may narrow, but cannot widen the base domain.
    scope = _scope(schema, change=lambda s: (
        s["categorical_domains"]["CardId"].remove(status),
        s["categorical_domains"].update(CardStatusCreationCardId=[status])))
    assert scope.issues("card_reward", batch)[0]


@pytest.mark.parametrize("change", [
    lambda s: s.update(acts=[True]), lambda s: s.update(acts=[1, 1]),
    lambda s: s.update(floor_range=[10, 3]), lambda s: s.update(ascension_range=[0, 21]),
    lambda s: s["categorical_domains"].pop("CardMechanicRole"),
    lambda s: s["categorical_domains"].update(CardMechanicRole=[999]),
    lambda s: s["categorical_domains"].update(ImaginedMechanic=[]),
])
def test_invalid_declarations_are_rejected(decision, change):
    schema, _ = decision
    with pytest.raises(ValueError, match="applicability"):
        _scope(schema, change=change)


@pytest.mark.parametrize("restriction", ["mechanic", "observation_identity"])
def test_mixed_cohort_stops_before_any_scoring_choice_or_fallback(restriction):
    native = pytest.importorskip("sts_learning_bridge")
    runtime = InstalledFixedCombatRuntime(FixedCombatConfig(
        backend="whole_combat", generation_work=1, wall_ms=1, atomic_nodes=None))
    recipes = []
    for seed, card in ((17, "Anger"), (18, "SeeingRed")):
        owned = [dict(id="Bash")]
        if restriction == "observation_identity" and seed == 18:
            owned.append(dict(id="SeeingRed"))
        recipes.append(dict(loadout=dict(seed=seed, ascension_level=20, current_hp=75, max_hp=75,
            boss="Hexaghost", cards=owned, relics=[]), floor=3, offers=[{"id": card}]))
    pool = native.LearningBatchEnv.constructed_card_rewards(json.dumps(recipes))
    env = FixedCombatEnvironment(pool, runtime.config, runtime.binding)
    env.baseline_manifest = runtime.baseline_manifest
    model = RecordingModel()
    gain = runtime.schema["card_mechanic_role"]["GainEnergy"]
    if restriction == "mechanic":
        scope = _scope(runtime.schema, change=lambda s: s["categorical_domains"]["CardMechanicRole"].remove(gain))
    else:
        batch = pool.decision_batch(semantic=True)
        cats = batch["semantic"]["categorical"]
        kinds = batch["semantic"]["token"]["kind"]
        first_end = batch["semantic"]["token"]["row_splits"][1]
        first_owned = sorted({int(number) for index, field, number in zip(
            cats["token_indices"], cats["field"], cats["value"], strict=True)
            if index < first_end and field == runtime.schema["categorical_field"]["CardId"]
            and kinds[index] == runtime.schema["token_kind"]["Card"]})
        scope = _scope(runtime.schema, change=lambda s: s.update(
            observation_categorical_domains={"Card": {"CardId": first_owned}}))
    controller = StrategicRunController(env, model_policy=model, model_sites=(ModelSite.CARD_REWARD,),
        model_applicability={ModelSite.CARD_REWARD: scope})
    before = env.checkpoint_bytes(max_bytes=64*1024*1024)
    env.choose = Mock(wraps=env.choose)
    env.fixed_step = Mock(wraps=env.fixed_step)
    with pytest.raises(StrategicModelOutOfScope) as caught:
        controller.advance()
    assert caught.value.prepared.slot_indices == (1,)
    assert len(caught.value.decisions) == 1
    assert caught.value.decisions[0]["adopted"] is False
    assert not any(key.startswith("legality_bootstrap_") for key in caught.value.prepared.payload)
    assert model.batches == []
    env.choose.assert_not_called()
    env.fixed_step.assert_not_called()
    assert controller.memory()["pending_decisions"] == []
    assert env.checkpoint_bytes(max_bytes=64*1024*1024) == before


def test_real_rollout_persists_scope_interruption_endpoint_and_unadopted_public_input(tmp_path):
    native = pytest.importorskip("sts_learning_bridge")
    model = RecordingModel()
    declaration = interface_scope_contract(native.semantic_schema(), ["card_reward"], acts=[4])
    source = {"manifest": {"applicability": declaration}}
    with patch("sts_learning.strategic_rollout.load_scoped_model", return_value=(model, source)):
        result = run_rollout(tmp_path/"run", seed=17, max_steps=30, capture_samples=True,
            model_directory=tmp_path/"test-source", model_sites=(ModelSite.CARD_REWARD,),
            executor=FixedCombatConfig(backend="whole_combat", generation_work=100_000, wall_ms=3_000, atomic_nodes=None),
            progress=lambda _: None)
    assert result["stop"] == "model_out_of_scope"
    assert not result["goal_completed"]
    assert result["terminal_outcome"] is None
    assert result["committed_model_decisions"] == 0
    assert model.batches == []
    root = tmp_path/"run"
    report = json.loads((root/"out-of-scope.json").read_text())
    assert report["adopted"] is False
    assert all(not decision["adopted"] for decision in report["decisions"])
    assert report["input"]["decision_token_ids"] == [report["decisions"][0]["source_token"]]
    assert not any(key.startswith("legality_bootstrap_") for key in report["input"])
    assert (root/"endpoint.bin").stat().st_size > 0
    memory = json.loads((root/"controller-memory.json").read_text())
    assert all(d["owner"] != "model" for d in memory["pending_decisions"])
    assert json.loads((root/"summary.json").read_text())["stop"] == "model_out_of_scope"
    from sts_learning.strategic_demonstrations import load_scoped_strategic_samples
    manifest, rows = load_scoped_strategic_samples(root / "samples")
    assert manifest["summary"]["observation_only_rows"] == 1
    observations = [record for _, record in rows if record["decision"]["ordinal"] is None]
    assert len(observations) == 1
    assert observations[0]["decision"]["source_token"] == report["decisions"][0]["source_token"]
    assert observations[0]["decision"]["audit"] == report["decisions"][0]["audit"]
    assert not observations[0]["committed"]


@pytest.mark.parametrize("outside", [False, "act", "observation_identity"])
def test_training_binds_teacher_declaration_and_rejects_own_rows_outside_it(tmp_path, outside):
    # Constructed single-decision interface test; not strategic teacher quality
    # evidence or a natural trajectory. The skip is actually committed natively.
    native = pytest.importorskip("sts_learning_bridge")
    from sts_learning.strategic_behavior_cloning import (
        StrategicBehaviorCloningError, StrategicDecisionFitConfig, train_scoped_strategic_decisions,
    )
    from sts_learning.strategic_demonstrations import ScopedStrategicSampleRecorder, save_scoped_strategic_decisions
    from sts_learning.strategic_rollout import load_scoped_model
    schema = native.semantic_schema()
    loadout = dict(seed=2026101200, ascension_level=20, current_hp=43, max_hp=75,
                   boss="Hexaghost", cards=[{"id": "Bash"}], relics=[])
    env = native.LearningBatchEnv.constructed_card_rewards(json.dumps([
        dict(loadout=loadout, floor=3, offers=[{"id": "SeeingRed"}])]))
    batch = env.decision_batch(semantic=True)
    audit = json.loads(env.strategic_decision_audit_json(0))
    ordinal = next(i for i, candidate in enumerate(audit["candidates"]) if candidate["kind"] == "skip_card_reward")
    decision = {"slot": 0, "source_token": batch["decision_token_ids"][0],
        "public_snapshot": batch["public_snapshot_ids"][0], "ordinal": ordinal, "owner": "baseline",
        "context": {"seed": 2026101200, "act": 1, "floor": 3}, "audit": audit}
    recorder = ScopedStrategicSampleRecorder()
    recorder(batch, [decision])
    env.choose(batch["decision_token_ids"], [ordinal])
    env.step()
    recorder.commit([decision])
    root, overlay = tmp_path/"sample", tmp_path/"teacher.json"
    recorder.save(root, schema=schema, producer={"scope": "constructed-interface-test"}, terminal_outcome=None)
    declaration = interface_scope_contract(schema, ["card_reward"], acts=[4] if outside == "act" else [1])
    declaration["sites"]["card_reward"]["observation_categorical_domains"] = {
        "Card": {"CardId": [] if outside == "observation_identity" else
            list(range(schema["categorical_vocabulary_size"][schema["categorical_field"]["CardId"]]))}}
    save_scoped_strategic_decisions(overlay, [root], [{"source_index": 0, "row_index": 0,
        "acceptable": [ordinal], "rationale": "Interface fixture explicitly teaches the committed skip."}],
        author="Codex test teacher", revision="v1", references=[], protocol={"applicability": declaration})
    output = tmp_path/"model"
    if outside:
        with pytest.raises(StrategicBehaviorCloningError, match="outside model applicability"):
            train_scoped_strategic_decisions(overlay, output)
        assert not output.exists()
    else:
        result = train_scoped_strategic_decisions(overlay, output, hidden_dim=12,
            decision_architecture="family-target",
            config=StrategicDecisionFitConfig(max_updates=128, learning_rate=0.01, check_every=4))
        assert result["stop_reason"] == "training_fit"
        contract = json.loads((output/"contract.json").read_text())
        assert contract["model_sites"] == ["card_reward"]
        assert contract["applicability"] == declaration
        _, source = load_scoped_model(output, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
        assert source["manifest"]["applicability"] == declaration
        with pytest.raises(ValueError, match="saved model scope"):
            load_scoped_model(output, schema=schema, model_sites=(ModelSite.CAMPFIRE,))
