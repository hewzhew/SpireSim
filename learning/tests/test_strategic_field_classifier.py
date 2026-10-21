"""Field classifier boundaries and honest backend integration."""
from copy import deepcopy
from dataclasses import replace
import hashlib
import json

import numpy as np
import pytest

pytest.importorskip("lightgbm")
pytest.importorskip("torch")  # Existing corpus/fit owner uses Torch for other backends.

from learning.tests.semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture
from learning.tests.strategic_scope_fixtures import add_fixture_run_axes, interface_scope_contract
from sts_learning.policy import BehaviorManifestId
from sts_learning.semantic_batch import select_semantic_decision_rows
from sts_learning.strategic_behavior_cloning import (
    StrategicBehaviorCloningError, StrategicFieldFitConfig, _field_candidate_relevance,
    train_scoped_field_classifier,
)
from sts_learning.strategic_demonstrations import (
    ScopedStrategicDecisionRow, ScopedStrategicSampleRecorder, save_scoped_strategic_decisions,
)
from sts_learning.strategic_field_policy import (
    CARD_CLASS_READOUT, CARD_UPGRADE_READOUT, FIELD_CHOICE_TARGET_RELEVANCE, FIELD_GRADED_RELEVANCE,
    FIELD_READOUT, LOCAL_CARD_READOUT, StrategicFieldEncoder,
)
from sts_learning.strategic_rollout import ModelSite, load_scoped_model


def _record(tmp_path, schema, seed, *, extra_field=False):
    batch = select_semantic_decision_rows(semantic_batch_fixture(), [1])
    batch.update(decision_token_ids=[f"token-{seed}"], public_snapshot_ids=[f"snapshot-{seed}"],
                 decision_steps=np.array([3], dtype=np.uint64))
    sem = batch["semantic"]
    sem["token"]["kind"][0] = schema["token_kind"]["Run"]
    scalar = sem["scalar"]
    numbers = [1, 3, 20] + ([0] if extra_field else [])
    fields = [schema["scalar_field"][n] for n in ("Act", "Floor", "AscensionLevel")]
    fields += [schema["scalar_field"]["OnlyInEvaluation"]] if extra_field else []
    scalar["token_indices"] = np.concatenate([scalar["token_indices"], np.zeros(len(fields), dtype=np.uint64)])
    scalar["field"] = np.concatenate([scalar["field"], np.asarray(fields, dtype=np.uint16)])
    scalar["value"] = np.concatenate([scalar["value"], np.asarray(numbers, dtype=np.float32)])
    owned = len(sem["token"]["kind"])-1
    sem["token"]["kind"][owned] = schema["token_kind"]["Card"]
    categorical = sem["categorical"]
    for key, values in (("token_indices", [owned, owned]),
                        ("field", [schema["categorical_field"]["CardId"], schema["categorical_field"]["CardCanUpgrade"]]),
                        ("value", [int(extra_field), 1])):
        categorical[key] = np.concatenate([categorical[key], np.asarray(values, dtype=categorical[key].dtype)])
    for key, values in (("token_indices", [owned, owned]),
                        ("field", [schema["categorical_field"]["CardType"], schema["categorical_field"]["CardRarity"]]),
                        ("value", [schema["card_type"]["Attack"], schema["card_rarity"]["Basic"]])):
        categorical[key] = np.concatenate([categorical[key], np.asarray(values, dtype=categorical[key].dtype)])
    for key, value in (("token_indices", owned), ("field", schema["scalar_field"]["CardUpgrades"]), ("value", 0)):
        scalar[key] = np.append(scalar[key], np.array([value], dtype=scalar[key].dtype))
    decision = dict(slot=9, source_token=f"token-{seed}", public_snapshot=f"snapshot-{seed}",
                    ordinal=1, owner="baseline", context=dict(seed=seed, act=1, floor=3),
                    audit=dict(decision_site="card_reward", candidates=[
                        dict(kind="take_card", card="A"), dict(kind="take_card", card="B"),
                        dict(kind="skip_card_reward")]))
    recorder = ScopedStrategicSampleRecorder()
    recorder(batch, [decision])
    recorder.commit([decision])
    root = tmp_path/str(seed)
    recorder.save(root, schema=schema, producer={"test": "synthetic interface"}, terminal_outcome=None)
    return root, batch


@pytest.fixture
def field_inputs(tmp_path):
    schema = add_fixture_run_axes(semantic_schema_fixture())
    schema["scalar_field"]["OnlyInEvaluation"] = 4
    schema["scalar_field"]["CardUpgrades"] = 5
    schema["token_kind"]["Card"] = 4
    schema["token_kind"]["OfferedCard"] = 5
    schema["categorical_field"].update(CardId=2, CardCanUpgrade=3, CardType=4, CardRarity=5)
    schema["categorical_vocabulary_size"].update({2: 3, 3: 2, 4: 2, 5: 3})
    schema["card_type"] = {"Attack": 0, "Skill": 1}
    schema["card_rarity"] = {"Basic": 2, "Common": 1, "Rare": 0}
    first, batch = _record(tmp_path, schema, 2026101200)
    second, _ = _record(tmp_path, schema, 2026101204, extra_field=True)
    return schema, [first, second], batch


def _overlay(tmp_path, inputs, *, name="decisions.json", evaluation=(1,), declared=True):
    schema, roots, _ = inputs
    path = tmp_path/name
    rows = [dict(source_index=0, row_index=0, acceptable=[0, 2], rationale="Both A and skip acceptable.")]
    rows.append(dict(source_index=1, row_index=0, acceptable=None if evaluation is None else list(evaluation),
                     rationale="Synthetic development label, distinct from training."))
    protocol = {"scope": "synthetic interface"}
    if declared:
        protocol["applicability"] = interface_scope_contract(schema, ["card_reward"])
    save_scoped_strategic_decisions(path, roots, rows, author="Codex test teacher", revision="v1",
                                    references=[], protocol=protocol)
    return path


@pytest.fixture(params=(("binary", FIELD_READOUT), ("lambdarank", FIELD_READOUT),
                        ("binary", CARD_UPGRADE_READOUT), ("lambdarank", CARD_UPGRADE_READOUT),
                        ("binary", CARD_CLASS_READOUT), ("lambdarank", CARD_CLASS_READOUT)))
def field_model(tmp_path, field_inputs, request):
    overlay = _overlay(tmp_path, field_inputs)
    root = tmp_path/"model"
    summary = train_scoped_field_classifier(overlay, root,
        config=StrategicFieldFitConfig(max_updates=64, check_every=4, learning_rate=0.2, min_data_in_bin=1,
                                      objective=request.param[0], readout=request.param[1]))
    assert summary["stop_reason"] == "training_fit"
    return root, field_inputs[0], field_inputs[2], summary


@pytest.mark.parametrize("objective", ["binary", "lambdarank"])
@pytest.mark.parametrize("readout", [FIELD_READOUT, CARD_UPGRADE_READOUT, CARD_CLASS_READOUT])
def test_field_fit_uses_training_labels_and_vocabulary_only(tmp_path, field_inputs, objective, readout):
    a = _overlay(tmp_path, field_inputs, name="first.json", evaluation=None)
    b = _overlay(tmp_path, field_inputs, name="second.json", evaluation=(0, 2))
    config = StrategicFieldFitConfig(max_updates=64, check_every=4, learning_rate=0.2, min_data_in_bin=1, objective=objective, readout=readout)
    first = train_scoped_field_classifier(a, tmp_path/"first", config=config)
    second = train_scoped_field_classifier(b, tmp_path/"second", config=config)
    assert first["model_sha256"] == second["model_sha256"]
    assert first["training_trace"] == second["training_trace"]
    assert first["final_evaluation"]["choices"]["unknown"] == 1
    assert second["final_evaluation"]["choices"]["adoptable"] == 1
    assert not any("OnlyInEvaluation" in name for name in json.loads((tmp_path/"first/feature-names.json").read_text()))
    assert not any(name.startswith("observation.card_identity.Card.1.") for name in json.loads((tmp_path/"first/feature-names.json").read_text()))
    assert first["ignored_columns"]["training"] == []
    assert any("OnlyInEvaluation" in name for name in first["ignored_columns"]["evaluation"])
    assert first["ignored_columns"] == second["ignored_columns"]
    assert first["evaluation_support"]["supervised_training_rows"] == 1
    assert first["evaluation_support"]["reference_role"] == "current-supervised-training-partition"
    assert first["evaluation_support"]["evaluation_rows"] == 1
    cohort = first["final_evaluation"]["predictions"][0]["readout_support"]
    assert second["final_evaluation"]["predictions"][0]["readout_support"] == cohort
    assert first["evaluation_support"]["cohorts"][cohort]["status"] == {"unknown": 1}
    assert second["evaluation_support"]["cohorts"][cohort]["status"] == {"adoptable": 1}
    if readout in {CARD_UPGRADE_READOUT, CARD_CLASS_READOUT}:
        assert any(name.startswith("observation.card_identity.Card.1.")
                   for name in first["ignored_columns"]["evaluation"])
    assert not (tmp_path/"first/model.ststorch").exists()


def test_field_loader_preserves_scores_identity_and_sources(field_model):
    root, schema, batch, summary = field_model
    before = {path.name: path.read_bytes() for path in root.iterdir()}
    policy, source = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    choice = policy.choose(batch)
    assert choice.ordinals[0] in (0, 2)
    assert choice.behavior_manifest_id == policy.behavior_manifest_id
    assert source["manifest"]["source_seeds"] == {"training_seeds": [2026101200], "evaluation_seeds": [2026101204]}
    assert source["manifest"]["decision_architecture"] == "field-membership"
    assert source["manifest"]["field_readout"] == json.loads((root/"contract.json").read_text())["field_readout"]
    assert summary["validation"]["candidate_reversal_max_logit_error"] == 0
    again, _ = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    assert again.behavior_manifest_id == policy.behavior_manifest_id
    assert np.array_equal(again.scores(batch).values, policy.scores(batch).values)
    assert before == {path.name: path.read_bytes() for path in root.iterdir()}


def _support_encoder():
    schema = add_fixture_run_axes(semantic_schema_fixture())
    names = sorted(("observation.scalar.Run.Floor.sum", "observation.scalar.Run.Floor.count",
                    "candidate.scalar.Candidate.Amount.sum", "candidate.scalar.Candidate.Amount.count"))
    return StrategicFieldEncoder(schema, names)


def _support_fact(floor, amount):
    return {"observation.scalar.Run.Floor.sum": floor, "observation.scalar.Run.Floor.count": 1,
            "candidate.scalar.Candidate.Amount.sum": amount, "candidate.scalar.Candidate.Amount.count": 1}


def test_support_distinguishes_reused_candidates_new_candidates_and_new_observation():
    encoder = _support_encoder()
    train = [_support_fact(3, 1), _support_fact(3, 2)]
    evaluation = [*reversed(train), _support_fact(3, 1), _support_fact(3, 4), _support_fact(4, 1)]
    report, cohorts = encoder.evaluation_support(train, [0, 2], evaluation, [0, 2, 4, 5], training_rows=[0])
    assert cohorts == ("all_candidates_seen", "known_observation_new_candidates", "new_observation")
    assert report["evaluation_distinct_read_observations"] == 2
    assert report["cohorts"]["all_candidates_seen"] == {
        "rows": 1, "distinct_read_observations": 1, "full_observation_seen_rows": 1}


def test_support_reports_ignored_fact_changes_without_claiming_full_observation_seen():
    encoder = _support_encoder()
    train = [_support_fact(3, 1)]
    evaluation = [{**train[0], "observation.scalar.Run.Act.sum": 2}]
    report, cohorts = encoder.evaluation_support(train, [0, 1], evaluation, [0, 1], training_rows=[0])
    assert cohorts == ("all_candidates_seen",)
    assert report["cohorts"]["all_candidates_seen"]["full_observation_seen_rows"] == 0
    assert np.array_equal(encoder.transform(train).toarray(), encoder.transform(evaluation).toarray())


def test_support_matches_sparse_zero_semantics_and_retains_presence_counts():
    encoder = _support_encoder()
    train = [_support_fact(3, 0)]
    missing_sum = {key: value for key, value in train[0].items() if not key.endswith("Amount.sum")}
    missing_count = {key: value for key, value in missing_sum.items() if not key.endswith("Amount.count")}
    report, cohorts = encoder.evaluation_support(train, [0, 1], [missing_sum, missing_count],
                                                 [0, 1, 2], training_rows=[0])
    assert cohorts == ("all_candidates_seen", "known_observation_new_candidates")
    assert np.array_equal(encoder.transform(train).toarray(), encoder.transform([missing_sum]).toarray())
    assert report["cohorts"]["known_observation_new_candidates"]["rows"] == 1


def test_support_excludes_unknown_training_rows_and_is_menu_order_independent():
    encoder = _support_encoder()
    train = [_support_fact(3, 1), _support_fact(4, 1)]
    evaluation = [train[1], train[0]]
    report, cohorts = encoder.evaluation_support(train, [0, 1, 2], evaluation, [0, 1, 2], training_rows=[0])
    assert cohorts == ("new_observation", "all_candidates_seen")
    assert report["supervised_training_rows"] == 1
    _, with_unknown = encoder.evaluation_support(train, [0, 1, 2], evaluation, [0, 1, 2], training_rows=[0, 1])
    assert with_unknown == ("all_candidates_seen", "all_candidates_seen")


@pytest.mark.parametrize("indices", [[-1], [1], [0, 0], [True]])
def test_support_rejects_invalid_supervised_row_indices(indices):
    encoder = _support_encoder()
    facts = [_support_fact(3, 1)]
    with pytest.raises(ValueError, match="supervised training rows"):
        encoder.evaluation_support(facts, [0, 1], facts, [0, 1], training_rows=indices)


@pytest.mark.parametrize("splits", [[1, 2], [0, 2], [0, 0, 1], [0.0, 1.0]])
def test_support_rejects_misaligned_decision_boundaries(splits):
    encoder = _support_encoder()
    facts = [_support_fact(3, 1)]
    with pytest.raises(ValueError, match="aligned nonempty decisions"):
        encoder.evaluation_support(facts, splits, facts, [0, 1], training_rows=[0])


def test_support_handles_empty_evaluation_without_inventing_a_cohort():
    report, cohorts = _support_encoder().evaluation_support([_support_fact(3, 1)], [0, 1],
                                                            [], [0], training_rows=[0])
    assert report["evaluation_rows"] == 0
    assert report["cohorts"] == {} and cohorts == ()


def test_support_rejects_observation_changes_inside_one_decision():
    facts = [_support_fact(3, 1), _support_fact(4, 2)]
    with pytest.raises(ValueError, match="shared observation facts"):
        _support_encoder().evaluation_support(facts, [0, 2], facts, [0, 2], training_rows=[0])


def test_support_requires_frozen_vocabulary():
    facts = [_support_fact(3, 1)]
    encoder = StrategicFieldEncoder(add_fixture_run_axes(semantic_schema_fixture()))
    with pytest.raises(ValueError, match="frozen vocabulary"):
        encoder.evaluation_support(facts, [0, 1], facts, [0, 1], training_rows=[0])


def test_candidate_payload_and_order_cannot_pollute_other_candidates(field_model):
    root, schema, batch, _ = field_model
    policy, _ = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    scores = policy.scores(batch).values
    changed = deepcopy(batch)
    changed["semantic"]["categorical"]["value"][2] = 1  # Own payload of ordinal1 only.
    assert np.array_equal(policy.scores(changed).values[[0, 2]], scores[[0, 2]])
    reordered = deepcopy(batch)
    reordered["semantic"]["candidate_token_indices"] = reordered["semantic"]["candidate_token_indices"][[2, 0, 1]]
    assert np.array_equal(policy.scores(reordered).values, scores[[2, 0, 1]])
    changed_addresses = deepcopy(batch)
    changed_addresses["slot_indices"][:] = 123
    changed_addresses["decision_steps"][:] = 99
    assert np.array_equal(policy.scores(changed_addresses).values, scores)


def test_missing_scalar_differs_from_supplied_zero_and_token_order_is_stable():
    batch = semantic_batch_fixture()
    encoder = StrategicFieldEncoder(semantic_schema_fixture())
    before, _ = encoder.candidate_features(batch)
    changed = deepcopy(batch)
    table = changed["semantic"]["scalar"]
    for key, number in (("token_indices", 1), ("field", 0), ("value", 0)):
        table[key] = np.append(table[key], np.array([number], dtype=table[key].dtype))
    after, _ = encoder.candidate_features(changed)
    assert "candidate.scalar.Candidate.Amount.count" not in before[0]
    assert after[0]["candidate.scalar.Candidate.Amount.count"] == 1
    assert after[0]["candidate.scalar.Candidate.Amount.sum"] == 0
    assert after[1:] == before[1:]
    for name in ("categorical", "scalar", "relation"):
        changed["semantic"][name] = {key: values[::-1] for key, values in changed["semantic"][name].items()}
    assert encoder.candidate_features(changed)[0] == after


@pytest.mark.parametrize("failure, message", [
    ("weights", "weights are not verified"), ("vocabulary", "vocabulary is not verified"),
    ("unknown_feature", "unknown feature"), ("count", "feature count"),
    ("readout", "architecture differs"), ("unfit", "training fit"),
    ("schema", "semantic schema"), ("scope", "scope"), ("seed", "seed provenance"),
    ("roundtrip", "weights are not verified"), ("gap", "training fit"),
    ("objective", "weights are incompatible"),
    ("objective_contract", "objective contract differs"),
    ("readout_contract", "readout contract differs"),
    ("foreign_keyed_feature", "unknown feature"),
])
def test_field_loader_shares_adoption_gates(field_model, failure, message):
    root, schema, _, _ = field_model
    contract = json.loads((root/"contract.json").read_text())
    summary = json.loads((root/"summary.json").read_text())
    if failure == "weights":
        (root/"model.lgb").write_bytes(b"changed weights")
    elif failure == "vocabulary":
        (root/"feature-names.json").write_text("[]")
    elif failure == "unknown_feature":
        (root/"feature-names.json").write_text(json.dumps(["observation.scalar.Run.HiddenSeed.sum"]))
        digest = hashlib.sha256((root/"feature-names.json").read_bytes()).hexdigest()
        contract["feature_names_sha256"] = summary["feature_names_sha256"] = digest
    elif failure == "count":
        contract["feature_count"] += 1
    elif failure == "readout":
        contract["field_readout"] = "different"
    elif failure == "unfit":
        summary["stop_reason"] = "max_updates"
    elif failure == "schema":
        contract["semantic_schema"]["version"] += 1
    elif failure == "scope":
        contract["model_sites"] = ["campfire"]
    elif failure == "seed":
        contract["training_seeds"] = [True]
    elif failure == "roundtrip":
        summary["validation"]["model_round_trip"] = False
    elif failure == "gap":
        summary["final_training"]["minimum_margin"] = 0.1
    elif failure in {"objective", "objective_contract"}:
        contract["field_objective"] = "lambdarank" if contract.get("field_objective")=="binary" else "binary"
        if failure == "objective": contract["training"]["objective"] = contract["field_objective"]
    elif failure == "readout_contract":
        contract["training"]["readout"] = CARD_UPGRADE_READOUT if contract["field_readout"]==FIELD_READOUT else FIELD_READOUT
    elif failure == "foreign_keyed_feature":
        (root/"feature-names.json").write_text(json.dumps(["candidate.card_identity.Card.0.token_count"]))
        digest = hashlib.sha256((root/"feature-names.json").read_bytes()).hexdigest()
        contract["feature_names_sha256"] = summary["feature_names_sha256"] = digest
    (root/"contract.json").write_text(json.dumps(contract))
    (root/"summary.json").write_text(json.dumps(summary))
    with pytest.raises(ValueError, match=message):
        load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))


@pytest.mark.parametrize("failure", ["kind", "category", "scalar", "relation", "ownership", "nan"])
def test_field_readout_rejects_unknown_domains_and_invalid_ownership(failure):
    batch = semantic_batch_fixture()
    sem = batch["semantic"]
    if failure == "kind": sem["token"]["kind"][0] = 99
    elif failure == "category": sem["categorical"]["value"][0] = 99
    elif failure == "scalar": sem["scalar"]["field"][0] = 99
    elif failure == "relation": sem["relation"]["relation"][0] = 99
    elif failure == "ownership": sem["token"]["scope"][3] = 2
    else: sem["scalar"]["value"][0] = float("nan")
    with pytest.raises(ValueError):
        StrategicFieldEncoder(semantic_schema_fixture()).candidate_features(batch)


def test_offline_field_fit_without_applicability_cannot_be_deployed(tmp_path, field_inputs):
    overlay = _overlay(tmp_path, field_inputs, declared=False)
    root = tmp_path/"offline"
    train_scoped_field_classifier(overlay, root, config=StrategicFieldFitConfig(learning_rate=0.2, min_data_in_bin=1))
    with pytest.raises(ValueError, match="applicability"):
        load_scoped_model(root, schema=field_inputs[0], model_sites=(ModelSite.CARD_REWARD,))


@pytest.mark.parametrize("objective,relevance", [
    ("binary", FIELD_CHOICE_TARGET_RELEVANCE),
    ("lambdarank", FIELD_CHOICE_TARGET_RELEVANCE),
    ("lambdarank", FIELD_GRADED_RELEVANCE),
])
def test_field_preferred_subset_controls_fit_and_development_cannot_choose_weights(tmp_path, field_inputs, objective, relevance):
    schema, roots, batch = field_inputs
    overlays = []
    for preferred in (0, 1):
        overlay = tmp_path/f"preferred-{preferred}.json"
        save_scoped_strategic_decisions(overlay, roots, [
            dict(source_index=0, row_index=0, acceptable=[0, 1, 2], preferred=[1], rationale="Prefer B."),
            dict(source_index=1, row_index=0, acceptable=[0, 1, 2], preferred=[preferred],
                 rationale="Development preference does not teach training.")],
            author="Codex test teacher", revision="v2", references=[],
            protocol={"applicability": interface_scope_contract(schema, ["card_reward"])})
        overlays.append(overlay)
    config = StrategicFieldFitConfig(max_updates=64, check_every=4, learning_rate=0.2,
                                    min_data_in_bin=1, objective=objective, relevance=relevance)
    a, b = [train_scoped_field_classifier(path, tmp_path/f"preferred-model-{i}", config=config)
            for i, path in enumerate(overlays)]
    assert a["stop_reason"] == "training_fit"
    assert a["final_training"]["minimum_margin"] is None
    assert a["final_training"]["minimum_recommendation_margin"] >= config.fit_margin
    assert a["final_training"]["recommendations"]["recommended"] == 1
    assert a["model_sha256"] == b["model_sha256"]
    assert a["training_trace"] == b["training_trace"]
    assert a["final_evaluation"]["recommendations"]["secondary_acceptable"] == 1
    assert b["final_evaluation"]["recommendations"]["recommended"] == 1
    root = tmp_path/"preferred-model-0"
    policy, source = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    assert policy.choose(batch).ordinals == (1,)
    assert source["manifest"]["choice_target"] == "preferred-if-declared-else-acceptable-v1"
    assert source["manifest"]["field_relevance"] == relevance
    contract = json.loads((root/"contract.json").read_text())
    assert contract["preferred_training_rows"] == 1
    if relevance == FIELD_GRADED_RELEVANCE:
        assert contract["booster_parameters"]["label_gain"] == [0, 1, 2]
        with pytest.raises(StrategicBehaviorCloningError, match="source relevance differs"):
            train_scoped_field_classifier(overlays[0], tmp_path/"wrong-relevance", frozen_field_model=root,
                config=StrategicFieldFitConfig(objective=objective, relevance=FIELD_CHOICE_TARGET_RELEVANCE))
        assert not (tmp_path/"wrong-relevance").exists()
    # Requalify the identical weights against another preferred subset of the
    # same complete acceptable set. Acceptability still fits; recommendation does not.
    changed = json.loads(overlays[0].read_text())
    changed["rows"][0]["preferred"] = [0]
    replacement = tmp_path/"other-preferred.json"
    replacement.write_text(json.dumps(changed))
    frozen = tmp_path/"preferred-frozen"
    result = train_scoped_field_classifier(replacement, frozen, frozen_field_model=root)
    assert result["updates"] == 0 and result["stop_reason"] == "frozen_unfit"
    assert result["final_training"]["choices"]["adoptable"] == 1
    assert result["final_training"]["recommendations"]["secondary_acceptable"] == 1
    with pytest.raises(ValueError, match="training fit"):
        load_scoped_model(frozen, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    # Even a forged stop reason cannot bypass the shared fit gate.
    summary = json.loads((frozen/"summary.json").read_text())
    summary["stop_reason"] = "training_fit"
    (frozen/"summary.json").write_text(json.dumps(summary))
    with pytest.raises(ValueError, match="training fit"):
        load_scoped_model(frozen, schema=schema, model_sites=(ModelSite.CARD_REWARD,))


def test_field_relevance_preserves_secondary_acceptability_and_unknown():
    row = ScopedStrategicDecisionRow({}, 0, 0, 0, 0, False, (1, 2), "Teacher order.",
        preferred_ordinals=(2,))
    assert [_field_candidate_relevance(row, i, FIELD_CHOICE_TARGET_RELEVANCE) for i in range(3)] == [0, 0, 1]
    assert [_field_candidate_relevance(row, i, FIELD_GRADED_RELEVANCE) for i in range(3)] == [0, 1, 2]
    fallback = replace(row, preferred_ordinals=None)
    assert [_field_candidate_relevance(fallback, i, FIELD_GRADED_RELEVANCE) for i in range(3)] == [0, 2, 2]
    for relevance in (FIELD_CHOICE_TARGET_RELEVANCE, FIELD_GRADED_RELEVANCE):
        with pytest.raises(StrategicBehaviorCloningError, match="unknown decisions"):
            _field_candidate_relevance(replace(row, acceptable_ordinals=None, preferred_ordinals=None), 0, relevance)


def test_legacy_field_relevance_keeps_scores(field_model):
    root, schema, batch, _ = field_model
    policy, _ = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    scores = policy.scores(batch).values.copy()
    contract = json.loads((root/"contract.json").read_text())
    del contract["field_relevance"]
    del contract["training"]["relevance"]
    (root/"contract.json").write_text(json.dumps(contract))
    restored, source = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    assert np.array_equal(scores, restored.scores(batch).values)
    assert "field_relevance" not in source["manifest"]


@pytest.mark.parametrize("failure", ["unknown", "wrong_type", "contract_mismatch"])
def test_field_loader_rejects_incompatible_relevance(field_model, failure):
    root, schema, _, _ = field_model
    contract = json.loads((root/"contract.json").read_text())
    if failure == "contract_mismatch":
        contract["training"]["relevance"] = FIELD_GRADED_RELEVANCE
    else:
        contract["field_relevance"] = "unknown" if failure == "unknown" else True
    (root/"contract.json").write_text(json.dumps(contract))
    with pytest.raises(ValueError, match="relevance"):
        load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))


@pytest.mark.parametrize("kwargs", [{"max_updates": 0}, {"min_data_in_leaf": True},
                                    {"num_leaves": 1}, {"max_depth": 0}, {"fit_margin": float("nan")},
                                    {"objective": "unknown"}, {"readout": "unknown"}, {"readout": True},
                                    {"relevance": "unknown"}, {"relevance": True},
                                    {"relevance": FIELD_GRADED_RELEVANCE}])
def test_field_fit_requires_explicit_bounded_valid_parameters(kwargs):
    with pytest.raises(StrategicBehaviorCloningError):
        StrategicFieldFitConfig(**kwargs)


def _upgrade_identity_fixture():
    schema = semantic_schema_fixture()
    schema["token_kind"]["Card"] = 3
    schema["categorical_field"].update(CardId=2, CardCanUpgrade=3)
    schema["categorical_vocabulary_size"].update({2: 3, 3: 2})
    schema["scalar_field"]["CardUpgrades"] = 1
    batch = select_semantic_decision_rows(semantic_batch_fixture(), [0])
    sem = batch["semantic"]
    sem["token"]["kind"][[0, 3]] = 3
    for key, values in (("token_indices", [0, 3, 0, 3, 1]), ("field", [2, 2, 3, 3, 2]),
                        ("value", [0, 1, 1, 0, 2])):
        table = sem["categorical"]
        table[key] = np.concatenate([table[key], np.asarray(values, dtype=table[key].dtype)])
    for key, values in (("token_indices", [0, 3]), ("field", [1, 1]), ("value", [0, 1])):
        table = sem["scalar"]
        table[key] = np.concatenate([table[key], np.asarray(values, dtype=table[key].dtype)])
    return schema, batch


def test_upgrade_readout_resolves_identity_collisions_without_reading_other_candidates():
    schema, batch = _upgrade_identity_fixture()
    changed = deepcopy(batch)
    changed["semantic"]["scalar"]["value"][-2:] = [1, 0]
    changed["semantic"]["categorical"]["value"][-3:-1] = [0, 1]
    generic = StrategicFieldEncoder(schema)
    assert generic.candidate_features(batch)[0] == generic.candidate_features(changed)[0]
    encoder = StrategicFieldEncoder(schema, readout=CARD_UPGRADE_READOUT)
    before, _ = encoder.candidate_features(batch)
    after, _ = encoder.candidate_features(changed)
    key = "observation.card_identity.Card.0.scalar.CardUpgrades.sum"
    assert before[0][key] == 0 and after[0][key] == 1
    assert not any("card_identity.Card.2." in name for row in before for name in row)
    assert not any(name.startswith("candidate.card_identity") for row in before for name in row)
    changed = deepcopy(batch)
    changed["semantic"]["categorical"]["value"][-1] = 0  # Candidate's explicit identity.
    assert encoder.candidate_features(changed)[0][1] == before[1]
    for section in ("categorical", "scalar", "relation"):
        changed["semantic"][section] = {k: v[::-1] for k, v in changed["semantic"][section].items()}
    assert encoder.candidate_features(changed)[0][1] == before[1]


def _shared_catalog_fixture():
    schema, batch = _upgrade_identity_fixture()
    schema["token_kind"]["OfferedCard"] = 4
    sem = batch["semantic"]
    for key, values in (("kind", [4, 4]), ("scope", [0, 1])):
        sem["token"][key] = np.concatenate([sem["token"][key], np.asarray(values, dtype=sem["token"][key].dtype)])
    sem["token"]["row_splits"][-1] += 2
    for key, values in (("token_indices", [4, 5]), ("field", [2, 2]), ("value", [2, 1])):
        table = sem["categorical"]
        table[key] = np.concatenate([table[key], np.asarray(values, dtype=table[key].dtype)])
    for key, values in (("token_indices", [4, 5]), ("field", [0, 0]), ("value", [99, 7])):
        table = sem["scalar"]
        table[key] = np.concatenate([table[key], np.asarray(values, dtype=table[key].dtype)])
    return schema, batch


def test_local_card_readout_omits_shared_catalog_and_retains_owned_and_local_payloads():
    schema, batch = _shared_catalog_fixture()
    local = StrategicFieldEncoder(schema, readout=LOCAL_CARD_READOUT)
    legacy = StrategicFieldEncoder(schema, readout=CARD_UPGRADE_READOUT)
    before, _ = local.candidate_features(batch)
    assert before[0]["observation.card_identity.Card.0.scalar.CardUpgrades.sum"] == 0
    assert before[0]["candidate.scalar.OfferedCard.Amount.sum"] == 7
    assert not any(name.startswith(("observation.scalar.OfferedCard.", "observation.category.OfferedCard."))
                   or name == "observation.token_count.OfferedCard" for row in before for name in row)
    changed = deepcopy(batch)
    changed["semantic"]["scalar"]["value"][-2] = -10
    changed["semantic"]["categorical"]["value"][-2] = 0
    assert local.candidate_features(changed)[0] == before
    assert legacy.candidate_features(changed)[0] != legacy.candidate_features(batch)[0]
    changed["semantic"]["scalar"]["value"][-1] = 8
    after, _ = local.candidate_features(changed)
    assert after[0]["candidate.scalar.OfferedCard.Amount.sum"] == 8
    assert after[1] == before[1]


def _card_class_fixture():
    schema, batch = _shared_catalog_fixture()
    schema["categorical_field"].update(CardType=4, CardRarity=5)
    schema["categorical_vocabulary_size"].update({4: 2, 5: 3})
    schema["card_type"] = {"Attack": 0, "Skill": 1}
    schema["card_rarity"] = {"Rare": 0, "Common": 1, "Basic": 2}
    for key, values in (("token_indices", [0, 3, 0, 3, 4, 4, 5, 5]),
                        ("field", [4, 4, 5, 5, 4, 5, 4, 5]),
                        ("value", [0, 0, 2, 0, 0, 0, 0, 0])):
        table = batch["semantic"]["categorical"]
        table[key] = np.concatenate([table[key], np.asarray(values, dtype=table[key].dtype)])
    return schema, batch


def test_owned_card_class_retains_same_card_scalar_associations():
    schema, batch = _card_class_fixture()
    changed = deepcopy(batch)
    changed["semantic"]["scalar"]["value"][:2] = [-1.0, 0.5]
    generic = StrategicFieldEncoder(schema, readout=LOCAL_CARD_READOUT)
    assert generic.candidate_features(batch)[0] == generic.candidate_features(changed)[0]
    encoder = StrategicFieldEncoder(schema, readout=CARD_CLASS_READOUT)
    before, _ = encoder.candidate_features(batch)
    after, _ = encoder.candidate_features(changed)
    assert before[0]["observation.card_class.Card.0.1.token_count"] == 1
    assert before[0]["observation.card_class.Card.0.0.token_count"] == 1
    # Basic's schema value is2, not an assumed stable0. Scalar requests remain
    # joined to the owning card's type and Basic membership.
    key = "observation.card_class.Card.0.1.scalar.Amount.sum"
    assert before[0][key] == 0.5 and after[0][key] == -1.0
    assert before[0]["candidate.scalar.OfferedCard.Amount.sum"] == 7
    assert not any("OfferedCard" in name for row in before for name in row if name.startswith("observation."))
    for section in ("categorical", "scalar", "relation"):
        changed["semantic"][section] = {k:v[::-1] for k,v in changed["semantic"][section].items()}
    assert encoder.candidate_features(changed)[0] == after


def test_owned_card_class_excludes_local_candidate_cards_and_shared_offers():
    schema, batch = _card_class_fixture()
    encoder = StrategicFieldEncoder(schema, readout=CARD_CLASS_READOUT)
    before, _ = encoder.candidate_features(batch)
    changed = deepcopy(batch)
    changed["semantic"]["token"]["kind"][5] = schema["token_kind"]["Card"]
    changed["semantic"]["scalar"]["value"][-2:] = [1000, 2000]
    after, _ = encoder.candidate_features(changed)
    assert {k:v for k,v in after[0].items() if k.startswith("observation.card_class.")} == {
        k:v for k,v in before[0].items() if k.startswith("observation.card_class.")}
    assert not any(name.startswith("candidate.card_class.") for row in after for name in row)


@pytest.mark.parametrize("failure", ["missing", "duplicate", "missing_schema", "no_basic"])
def test_owned_card_class_requires_explicit_owner_fields(failure):
    schema, batch = _card_class_fixture()
    if failure == "missing_schema":
        del schema["categorical_field"]["CardType"]
        del schema["categorical_vocabulary_size"][4]
    elif failure == "no_basic":
        schema["card_rarity"]["Special"] = schema["card_rarity"].pop("Basic")
    elif failure == "missing":
        table = batch["semantic"]["categorical"]
        keep = ~((table["token_indices"] == 0) & (table["field"] == 4))
        batch["semantic"]["categorical"] = {key:value[keep] for key,value in table.items()}
    else:
        table = batch["semantic"]["categorical"]
        for key, value in (("token_indices", 0), ("field", 4), ("value", 0)):
            table[key] = np.append(table[key], np.array([value], dtype=table[key].dtype))
    with pytest.raises(ValueError, match="card class"):
        StrategicFieldEncoder(schema, readout=CARD_CLASS_READOUT).candidate_features(batch)


@pytest.mark.parametrize("feature", ["observation.card_class.Card.0.2.token_count",
    "observation.card_class.Card.99.1.scalar.Amount.sum",
    "candidate.card_class.Card.0.1.token_count",
    "observation.card_class.Card.0.1.scalar.NotKnown.sum"])
def test_owned_card_class_rejects_invalid_vocabulary(feature):
    schema, _ = _card_class_fixture()
    with pytest.raises(ValueError, match="card class feature"):
        StrategicFieldEncoder(schema, [feature], readout=CARD_CLASS_READOUT)


@pytest.mark.parametrize("feature", ["observation.token_count.OfferedCard",
                                   "observation.category.OfferedCard.CardId.2",
                                   "observation.scalar.OfferedCard.Amount.sum"])
def test_local_card_readout_rejects_shared_catalog_vocabulary(feature):
    schema, _ = _shared_catalog_fixture()
    with pytest.raises(ValueError, match="shared offered card catalog"):
        StrategicFieldEncoder(schema, [feature], readout=LOCAL_CARD_READOUT)


def test_local_card_readout_requires_offered_card_schema():
    schema, _ = _upgrade_identity_fixture()
    with pytest.raises(ValueError, match="typed offered card"):
        StrategicFieldEncoder(schema, readout=LOCAL_CARD_READOUT)


def test_local_card_readout_fits_restores_and_qualifies_without_changing_readout(tmp_path, field_inputs):
    overlay = _overlay(tmp_path, field_inputs)
    root = tmp_path/"local"
    config = StrategicFieldFitConfig(max_updates=64, check_every=4, learning_rate=0.2,
                                    min_data_in_bin=1, objective="lambdarank", readout=LOCAL_CARD_READOUT)
    result = train_scoped_field_classifier(overlay, root, config=config)
    assert result["stop_reason"] == "training_fit"
    schema, _, batch = field_inputs
    policy, source = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    assert policy.encoder.readout == LOCAL_CARD_READOUT
    assert source["manifest"]["field_readout"] == LOCAL_CARD_READOUT
    frozen = tmp_path/"frozen"
    assert train_scoped_field_classifier(overlay, frozen, frozen_field_model=root)["updates"] == 0
    restored, _ = load_scoped_model(frozen, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    assert np.array_equal(policy.scores(batch).values, restored.scores(batch).values)
    assert (root/"model.lgb").read_bytes() == (frozen/"model.lgb").read_bytes()


def test_native_local_card_readout_keeps_common_candidates_independent_of_other_offers():
    bridge = pytest.importorskip("sts_learning_bridge")
    schema = bridge.semantic_schema()
    deck = ([{"id": "AscendersBane"}, {"id": "Bash"}]
            + [{"id": "Strike"} for _ in range(5)] + [{"id": "Defend"} for _ in range(4)]
            + [{"id": "Carnage"}, {"id": "ShrugItOff"}])
    recipes = [dict(loadout=dict(seed=902, ascension_level=20, current_hp=70, max_hp=80, gold=120,
                                boss="TheGuardian", cards=deck, relics=["BurningBlood"]), floor=5,
                    offers=[{"id": "SeeingRed"}, {"id": "TrueGrit"}, {"id": other}])
               for other in ("BodySlam", "Clash", "Entrench", "LimitBreak")]
    env = bridge.LearningBatchEnv.constructed_card_rewards(json.dumps(recipes))
    batch = env.decision_batch(semantic=True)
    local, splits = StrategicFieldEncoder(schema, readout=LOCAL_CARD_READOUT).candidate_features(batch)
    legacy, _ = StrategicFieldEncoder(schema, readout=CARD_UPGRADE_READOUT).candidate_features(batch)
    for start in splits[:-1]:
        assert [local[int(start)+i] for i in (0, 1, 3)] == [local[i] for i in (0, 1, 3)]
    assert legacy[0] != legacy[int(splits[1])]


@pytest.mark.parametrize("failure", ["missing", "duplicate"])
def test_upgrade_readout_requires_one_explicit_owned_identity(failure):
    schema, batch = _upgrade_identity_fixture()
    table = batch["semantic"]["categorical"]
    index = int(np.flatnonzero((table["token_indices"]==0) & (table["field"]==2))[0])
    if failure == "missing":
        for key in table: table[key] = np.delete(table[key], index)
    else:
        for key in table: table[key] = np.append(table[key], table[key][index:index+1])
    with pytest.raises(ValueError, match="single explicit identity"):
        StrategicFieldEncoder(schema, readout=CARD_UPGRADE_READOUT).candidate_features(batch)


def test_old_readout_contract_without_training_readout_retains_original_meaning(field_model):
    root, schema, batch, _ = field_model
    contract = json.loads((root/"contract.json").read_text())
    if contract["field_readout"] != FIELD_READOUT:
        return
    original, _ = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    del contract["training"]["readout"]
    (root/"contract.json").write_text(json.dumps(contract))
    restored, _ = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    assert np.array_equal(original.scores(batch).values, restored.scores(batch).values)


@pytest.mark.parametrize("extra", [["--initial-model", "source"], ["--observation-pooling", "mean"],
                                    ["--family-pooling", "vector-max"], ["--hidden-dim", "64"]])
def test_field_cli_rejects_neural_configuration_before_reading_inputs(monkeypatch, extra):
    import sys
    from sts_learning.strategic_behavior_cloning import main
    monkeypatch.setattr(sys, "argv", ["clone", "--decisions", "unused.json", "--output", "unused",
                                     "--decision-architecture", "field-membership", *extra])
    with pytest.raises(SystemExit) as error:
        main()
    assert error.value.code == 2


def test_frozen_field_qualification_preserves_bytes_and_checks_new_training(tmp_path, field_model, field_inputs, monkeypatch):
    import lightgbm as lgb
    from sts_learning.seeds import SeedPartition, SeedPartitionSpec
    root, schema, batch, _ = field_model
    before = {p.name: p.read_bytes() for p in root.iterdir()}
    source_policy, source = load_scoped_model(root, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    seed = next(s for s in range(2026101250, 2026101300) if SeedPartitionSpec().classify(s) is SeedPartition.TRAINING)
    extra_root, _ = _record(tmp_path, schema, seed, extra_field=True)
    overlay = tmp_path/"qualification.json"
    rows = [dict(source_index=i, row_index=0, acceptable=[0, 2] if i != 1 else None,
                 rationale="Full teacher set; development Unknown teaches no target.") for i in range(3)]
    save_scoped_strategic_decisions(overlay, [*field_inputs[1], extra_root], rows,
        author="Codex test teacher", revision="v1", references=[],
        protocol={"applicability": interface_scope_contract(schema, ["card_reward"])})
    def reject_update(*args, **kwargs):
        pytest.fail("frozen qualification called Booster.update")
    monkeypatch.setattr(lgb.Booster, "update", reject_update)
    output = tmp_path/"qualified"
    summary = train_scoped_field_classifier(overlay, output, frozen_field_model=root)
    assert summary["updates"] == 0 and summary["stop_reason"] == "training_fit"
    assert summary["final_training"]["supervised_rows"] == 2
    assert summary["evaluation_support"]["reference_role"] == "current-supervised-qualification-partition-no-updates"
    assert summary["final_training"]["choices"]["adoptable"] == 2
    assert summary["final_evaluation"]["choices"]["unknown"] == 1
    assert any("OnlyInEvaluation" in name for name in summary["ignored_columns"]["training"])
    assert all((output/name).read_bytes() == before[name] for name in ("model.lgb", "feature-names.json"))
    assert before == {p.name: p.read_bytes() for p in root.iterdir()}
    policy, provenance = load_scoped_model(output, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    assert np.array_equal(policy.scores(batch).values, source_policy.scores(batch).values)
    assert policy.behavior_manifest_id != source_policy.behavior_manifest_id
    assert provenance["manifest"]["source_seeds"] == {
        "training_seeds": sorted([2026101200, seed]), "evaluation_seeds": [2026101204]}
    contract = json.loads((output/"contract.json").read_text())
    ancestor = contract["initialization"]
    for name in ("model_sha256", "feature_names_sha256", "contract_sha256", "summary_sha256"):
        assert ancestor[name] == source["manifest"][name]
    assert ancestor["behavior_id"] == source["behavior_id"]
    assert contract["model_seed"] is None and "max_updates" not in contract["training"]
    assert contract["booster_parameters"] is None
    chained = tmp_path/"chained"
    train_scoped_field_classifier(overlay, chained, frozen_field_model=output)
    _, chained_source = load_scoped_model(chained, schema=schema, model_sites=(ModelSite.CARD_REWARD,))
    assert chained_source["manifest"]["source_seeds"] == provenance["manifest"]["source_seeds"]
    # Chained qualification retains the original evaluation provenance.
    with pytest.raises(StrategicBehaviorCloningError, match="previously evaluated"):
        train_scoped_field_classifier(overlay, tmp_path/"held-out", frozen_field_model=output, evaluation_role="held_out")


@pytest.mark.parametrize("failure", ["new_target", "higher_margin"])
def test_frozen_field_qualification_retains_unfit_artifact(tmp_path, field_model, field_inputs, failure):
    root, schema, _, _ = field_model
    overlay = _overlay(tmp_path, field_inputs, name="changed.json")
    if failure == "new_target":
        document = json.loads(overlay.read_text())
        document["rows"][0]["acceptable"] = [1]
        overlay.write_text(json.dumps(document))
        config = None
    else:
        contract = json.loads((root/"contract.json").read_text())
        config = StrategicFieldFitConfig(fit_margin=100, objective=contract["field_objective"], readout=contract["field_readout"])
    output = tmp_path/"unfit-qualified"
    summary = train_scoped_field_classifier(overlay, output, frozen_field_model=root, config=config)
    assert summary["updates"] == 0 and summary["stop_reason"] == "frozen_unfit"
    with pytest.raises(ValueError, match="training fit"):
        load_scoped_model(output, schema=schema, model_sites=(ModelSite.CARD_REWARD,))


@pytest.mark.parametrize("failure", ["unfit", "weights", "objective", "readout", "applicability", "descendant"])
def test_frozen_field_qualification_rejects_invalid_source_and_request(tmp_path, field_model, field_inputs, failure):
    root, _, _, _ = field_model
    overlay = _overlay(tmp_path, field_inputs, name="new.json", declared=failure != "applicability")
    output, config = tmp_path/"new-model", None
    if failure == "unfit":
        summary = json.loads((root/"summary.json").read_text())
        summary["stop_reason"] = "max_updates"
        (root/"summary.json").write_text(json.dumps(summary))
    elif failure == "weights":
        (root/"model.lgb").write_bytes(b"bad")
    elif failure in {"objective", "readout"}:
        contract = json.loads((root/"contract.json").read_text())
        config = StrategicFieldFitConfig(
            objective=("lambdarank" if contract["field_objective"] == "binary" else "binary")
                      if failure == "objective" else contract["field_objective"],
            readout=(CARD_UPGRADE_READOUT if contract["field_readout"] == FIELD_READOUT else FIELD_READOUT)
                    if failure == "readout" else contract["field_readout"])
    elif failure == "descendant":
        output = root/"qualification"
    with pytest.raises(StrategicBehaviorCloningError):
        train_scoped_field_classifier(overlay, output, frozen_field_model=root, config=config)
    assert not output.exists()


@pytest.mark.parametrize("failure", ["source_weight_digest", "source_contract_digest", "updates", "unchanged", "source_seeds", "mode"])
def test_frozen_field_loader_checks_bound_qualification_provenance(tmp_path, field_model, field_inputs, failure):
    root, schema, _, _ = field_model
    overlay = _overlay(tmp_path, field_inputs, name="qualified.json")
    output = tmp_path/"qualified"
    train_scoped_field_classifier(overlay, output, frozen_field_model=root)
    contract = json.loads((output/"contract.json").read_text())
    summary = json.loads((output/"summary.json").read_text())
    if failure == "source_weight_digest": contract["initialization"]["model_sha256"] = "0"*64
    elif failure == "source_contract_digest": contract["initialization"]["contract_sha256"] = "0"*64
    elif failure == "updates": summary["updates"] = 1
    elif failure == "unchanged": summary["validation"]["frozen_source_unchanged"] = False
    elif failure == "source_seeds": contract["initialization"]["source_training_seeds"] = []
    else: del contract["training"]["mode"]
    (output/"contract.json").write_text(json.dumps(contract))
    (output/"summary.json").write_text(json.dumps(summary))
    with pytest.raises(ValueError, match="qualification provenance"):
        load_scoped_model(output, schema=schema, model_sites=(ModelSite.CARD_REWARD,))


@pytest.mark.parametrize("extra", [[], ["--initial-model", "source"], ["--max-updates", "1"],
                                    ["--field-readout", CARD_UPGRADE_READOUT], ["--model-seed", "1"],
                                    ["--field-relevance", FIELD_GRADED_RELEVANCE]])
def test_frozen_field_cli_rejects_invalid_combinations_before_inputs(monkeypatch, extra):
    import sys
    from sts_learning.strategic_behavior_cloning import main
    arguments = ["clone", "--decisions", "unused", "--output", "unused", "--frozen-field-model", "source"]
    if extra: arguments.extend(["--decision-architecture", "field-membership", *extra])
    monkeypatch.setattr(sys, "argv", arguments)
    with pytest.raises(SystemExit) as error:
        main()
    assert error.value.code == 2
