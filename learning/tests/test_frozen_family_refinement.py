import json
from unittest.mock import patch

import pytest

torch = pytest.importorskip("torch")

from learning.tests.test_scoped_strategic_decisions import _annotation, _overlay, _source
from learning.tests.semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture
from sts_learning.strategic_behavior_cloning import (
    StrategicBehaviorCloningError, StrategicDecisionFitConfig,
    train_scoped_strategic_decisions,
)
from sts_learning.strategic_demonstrations import load_scoped_strategic_samples
from sts_learning.torch_checkpoints import decode_state_dict
from sts_learning.torch_policy import (
    RaggedCandidateScorer, RaggedScorerConfig, TorchPolicyError,
    configure_target_only_training,
)


@pytest.fixture
def family_source(tmp_path):
    schema = semantic_schema_fixture()
    schema["categorical_field"]["ActionKind"] = schema["categorical_field"].pop("Flag")
    with patch(_source.__module__+".semantic_schema_fixture", return_value=schema):
        roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204)]
    overlay = _overlay(tmp_path, roots)
    config = StrategicDecisionFitConfig(max_updates=128, check_every=4, learning_rate=0.01)
    base = tmp_path/"base"
    result = train_scoped_strategic_decisions(overlay, base, hidden_dim=12, config=config,
                                             decision_architecture="family-target")
    assert result["stop_reason"] == "training_fit"
    return roots, base, config, schema


def test_target_refinement_preserves_every_family_parameter_and_exact_scores(tmp_path, family_source):
    roots, base, config, schema = family_source
    original_files = {p.name: p.read_bytes() for p in base.iterdir()}
    overlay = _overlay(tmp_path, roots, [_annotation(acceptable=(2,))], name="refine.json")
    out = tmp_path/"refined"
    result = train_scoped_strategic_decisions(overlay, out, hidden_dim=12, config=config,
        decision_architecture="family-target", frozen_family_model=base)
    assert result["stop_reason"] == "training_fit"
    assert result["final_training"]["choices"]["adoptable"] == 1
    assert result["validation"]["frozen_family_state_unchanged"]
    before = decode_state_dict(original_files["model.ststorch"])
    after = decode_state_dict((out/"model.ststorch").read_bytes())
    assert all(torch.equal(value, after[key]) for key,value in before.items() if not key.startswith("scorer."))
    assert any(not torch.equal(value, after[key]) for key,value in before.items() if key.startswith("scorer."))
    scorers = []
    for state in (before, after):
        scorer = RaggedCandidateScorer.from_bridge_schema(schema, RaggedScorerConfig(hidden_dim=12, family_field=1))
        scorer.load_state_dict(state, strict=True)
        scorer.eval().requires_grad_(False)
        scorers.append(scorer)
    for root in roots:
        _, rows = load_scoped_strategic_samples(root)
        with torch.inference_mode():
            a, b = [scorer(rows[0][0]) for scorer in scorers]
        assert torch.equal(a.families.values, b.families.values)
        assert torch.equal(a.families.candidate_families, b.families.candidate_families)
    assert original_files == {p.name: p.read_bytes() for p in base.iterdir()}
    contract = json.loads((out/"contract.json").read_text())
    assert contract["model_seed"] is None
    assert contract["initialization"]["kind"] == "frozen-family-target-refinement"
    assert contract["initialization"]["model_sha256"] == json.loads(original_files["summary.json"])["model_sha256"]


def test_refinement_weights_do_not_depend_on_evaluation_targets(tmp_path, family_source):
    roots, base, config, _ = family_source
    a = _overlay(tmp_path, roots, [_annotation(acceptable=(2,)), _annotation(1, acceptable=(0,))], name="a.json")
    b = _overlay(tmp_path, roots, [_annotation(acceptable=(2,)), _annotation(1, acceptable=(1,))], name="b.json")
    results = [train_scoped_strategic_decisions(path, tmp_path/name, hidden_dim=12, config=config,
        decision_architecture="family-target", frozen_family_model=base) for path,name in ((a,"a"),(b,"b"))]
    assert results[0]["model_sha256"] == results[1]["model_sha256"]
    assert results[0]["training_trace"] == results[1]["training_trace"]


@pytest.mark.parametrize("failure", ["wrong-family", "insufficient-margin", "source-hash", "source-contract"])
def test_refinement_rejects_unrepairable_or_unbound_sources_before_creating_output(tmp_path, family_source, failure):
    roots, base, config, _ = family_source
    target = (1,) if failure == "wrong-family" else (2,)
    if failure == "insufficient-margin":
        config = StrategicDecisionFitConfig(max_updates=16, check_every=4, fit_margin=1000)
    if failure == "source-hash":
        payload = bytearray((base/"model.ststorch").read_bytes())
        payload[-1] ^= 1
        (base/"model.ststorch").write_bytes(payload)
    if failure == "source-contract":
        contract = json.loads((base/"contract.json").read_text())
        contract["scorer"]["family_field"] = 0
        (base/"contract.json").write_text(json.dumps(contract))
    overlay = _overlay(tmp_path, roots, [_annotation(acceptable=target)], name="bad.json")
    with pytest.raises(StrategicBehaviorCloningError):
        train_scoped_strategic_decisions(overlay, tmp_path/"rejected", hidden_dim=12, config=config,
            decision_architecture="family-target", frozen_family_model=base)
    assert not (tmp_path/"rejected").exists()


def test_target_only_gradients_and_rejection_of_flat_scorer():
    schema = semantic_schema_fixture()
    scorer = RaggedCandidateScorer.from_bridge_schema(schema,
        RaggedScorerConfig(hidden_dim=12, family_field=1, value_head=True))
    configure_target_only_training(scorer)
    assert all(p.requires_grad == name.startswith("scorer.") for name,p in scorer.named_parameters())
    logits = scorer(semantic_batch_fixture())
    logits.values.sum().backward()
    assert all(p.grad is None for name,p in scorer.named_parameters() if not name.startswith("scorer."))
    assert any(p.grad is not None for name,p in scorer.named_parameters() if name.startswith("scorer."))
    with pytest.raises(TorchPolicyError, match="family-target"):
        configure_target_only_training(RaggedCandidateScorer.from_bridge_schema(schema))


def test_frozen_source_requires_family_target_mode(tmp_path):
    with pytest.raises(StrategicBehaviorCloningError, match="family-target"):
        train_scoped_strategic_decisions(tmp_path/"unused.json", tmp_path/"unused",
                                        frozen_family_model=tmp_path/"unused-source")


def test_frozen_families_cannot_ignore_preferred_family_when_all_choices_are_acceptable(tmp_path, family_source):
    roots, base, config, _ = family_source
    overlay = _overlay(tmp_path, roots, [dict(_annotation(acceptable=(0, 1, 2)), preferred=[1])],
                       name="preferred-other-family.json")
    with pytest.raises(StrategicBehaviorCloningError, match="family"):
        train_scoped_strategic_decisions(overlay, tmp_path/"rejected-preference", hidden_dim=12,
            config=config, decision_architecture="family-target", frozen_family_model=base)
    assert not (tmp_path/"rejected-preference").exists()


def test_refinement_rejects_changed_pooling_before_creating_output(tmp_path, family_source):
    roots, base, config, _ = family_source
    overlay = _overlay(tmp_path, roots, [_annotation(acceptable=(2,))], name="changed-pooling.json")
    with pytest.raises(StrategicBehaviorCloningError, match="scorer contract differs"):
        train_scoped_strategic_decisions(overlay, tmp_path/"rejected", hidden_dim=12, config=config,
            decision_architecture="family-target", family_pooling="candidate-score-max",
            frozen_family_model=base)
    assert not (tmp_path/"rejected").exists()


def test_refinement_cannot_relabel_source_evaluation_as_fresh_held_out(tmp_path, family_source):
    roots, base, config, _ = family_source
    overlay = _overlay(tmp_path, roots, [_annotation(acceptable=(2,))], name="not-fresh.json")
    with pytest.raises(StrategicBehaviorCloningError, match="previously evaluated"):
        train_scoped_strategic_decisions(overlay, tmp_path/"rejected", hidden_dim=12, config=config,
            decision_architecture="family-target", frozen_family_model=base, evaluation_role="held_out")
    assert not (tmp_path/"rejected").exists()


def test_refinement_keeps_ancestor_seed_provenance(tmp_path, family_source):
    roots, base, config, _ = family_source
    overlay = _overlay(tmp_path, roots, [_annotation(acceptable=(2,))], name="next.json")
    previous = base
    for name in ("one", "two"):
        out = tmp_path/name
        train_scoped_strategic_decisions(overlay, out, hidden_dim=12, config=config,
            decision_architecture="family-target", frozen_family_model=previous)
        initialization = json.loads((out/"contract.json").read_text())["initialization"]
        assert initialization["source_training_seeds"] == [2026101200]
        assert initialization["source_evaluation_seeds"] == [2026101204]
        previous = out
