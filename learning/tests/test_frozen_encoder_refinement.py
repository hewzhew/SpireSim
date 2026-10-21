import json
from unittest.mock import patch

import pytest

torch = pytest.importorskip("torch")

from learning.tests.test_frozen_family_refinement import family_source
from learning.tests.test_scoped_strategic_decisions import _annotation, _overlay
from learning.tests.semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture
from sts_learning.strategic_behavior_cloning import (
    StrategicBehaviorCloningError, main, train_scoped_strategic_decisions,
)
from sts_learning.torch_checkpoints import decode_state_dict
from sts_learning.torch_policy import (
    RaggedCandidateScorer, RaggedScorerConfig, TorchPolicyError,
    configure_policy_heads_only_training,
)


@pytest.mark.parametrize("target, changed_prefix", [((1,), "family_head."), ((2,), "scorer.")])
def test_head_refinement_repairs_family_or_target_without_changing_encoder(
    tmp_path, family_source, target, changed_prefix,
):
    roots, source, config, _ = family_source
    before_files = {p.name: p.read_bytes() for p in source.iterdir()}
    overlay = _overlay(tmp_path, roots, [_annotation(acceptable=target)], name="heads.json")
    output = tmp_path / "heads"
    summary = train_scoped_strategic_decisions(overlay, output, hidden_dim=12, config=config,
        decision_architecture="family-target", frozen_encoder_model=source)
    assert summary["stop_reason"] == "training_fit"
    assert summary["final_training"]["choices"]["adoptable"] == 1
    assert summary["validation"]["frozen_encoder_state_unchanged"]
    assert summary["validation"]["candidate_reversal_max_logit_error"] == 0.0
    before = decode_state_dict(before_files["model.ststorch"])
    after = decode_state_dict((output / "model.ststorch").read_bytes())
    assert all(torch.equal(value, after[name]) for name, value in before.items()
               if not name.startswith(("scorer.", "family_head.")))
    assert any(not torch.equal(value, after[name]) for name, value in before.items()
               if name.startswith(changed_prefix))
    assert before_files == {p.name: p.read_bytes() for p in source.iterdir()}
    contract = json.loads((output / "contract.json").read_text())
    assert contract["model_seed"] is None
    assert contract["initialization"]["kind"] == "frozen-encoder-policy-head-refinement"
    assert contract["initialization"]["source_training_seeds"] == [2026101200]
    assert contract["initialization"]["source_evaluation_seeds"] == [2026101204]


def test_head_refinement_is_independent_of_development_targets(tmp_path, family_source):
    roots, source, config, _ = family_source
    a = _overlay(tmp_path, roots, [_annotation(acceptable=(1,)), _annotation(1, acceptable=(0,))], name="a.json")
    b = _overlay(tmp_path, roots, [_annotation(acceptable=(1,)), _annotation(1, acceptable=(1,))], name="b.json")
    results = [train_scoped_strategic_decisions(path, tmp_path / name, hidden_dim=12, config=config,
        decision_architecture="family-target", frozen_encoder_model=source) for path, name in ((a, "a"), (b, "b"))]
    assert results[0]["model_sha256"] == results[1]["model_sha256"]
    assert results[0]["training_trace"] == results[1]["training_trace"]


def test_policy_heads_only_gradients_leave_encoder_and_value_frozen():
    schema = semantic_schema_fixture()
    scorer = RaggedCandidateScorer.from_bridge_schema(schema,
        RaggedScorerConfig(hidden_dim=12, family_field=1, value_head=True))
    configure_policy_heads_only_training(scorer)
    assert all(p.requires_grad == name.startswith(("scorer.", "family_head."))
               for name, p in scorer.named_parameters())
    logits = scorer(semantic_batch_fixture())
    (logits.families.values.sum() + logits.families.target_values.sum()).backward()
    assert all(p.grad is None for name, p in scorer.named_parameters()
               if not name.startswith(("scorer.", "family_head.")))
    for prefix in ("scorer.", "family_head."):
        assert any(p.grad is not None for name, p in scorer.named_parameters() if name.startswith(prefix))
    with pytest.raises(TorchPolicyError, match="family-target"):
        configure_policy_heads_only_training(RaggedCandidateScorer.from_bridge_schema(schema))


def test_encoder_refinement_requires_explicit_compatible_scope(tmp_path):
    with pytest.raises(StrategicBehaviorCloningError, match="family-target"):
        train_scoped_strategic_decisions(tmp_path / "unused.json", tmp_path / "unused",
                                        frozen_encoder_model=tmp_path / "source")
    with pytest.raises(StrategicBehaviorCloningError, match="mutually exclusive"):
        train_scoped_strategic_decisions(tmp_path / "unused.json", tmp_path / "unused",
            decision_architecture="family-target", frozen_encoder_model=tmp_path / "a",
            frozen_family_model=tmp_path / "b")
    with patch("sys.argv", ["fit", "--decisions", "unused.json", "--output", "unused",
                            "--frozen-encoder-model", "a", "--frozen-family-model", "b"]):
        with pytest.raises(SystemExit) as error:
            main()
        assert error.value.code == 2


@pytest.mark.parametrize("failure", ["source-hash", "reused-evaluation"])
def test_encoder_refinement_rejects_bad_provenance_before_output(tmp_path, family_source, failure):
    roots, source, config, _ = family_source
    role = "held_out" if failure == "reused-evaluation" else "development"
    if failure == "source-hash":
        payload = bytearray((source / "model.ststorch").read_bytes())
        payload[-1] ^= 1
        (source / "model.ststorch").write_bytes(payload)
    overlay = _overlay(tmp_path, roots, [_annotation(acceptable=(1,))], name="bad-heads.json")
    with pytest.raises(StrategicBehaviorCloningError):
        train_scoped_strategic_decisions(overlay, tmp_path / "rejected", hidden_dim=12, config=config,
            decision_architecture="family-target", frozen_encoder_model=source, evaluation_role=role)
    assert not (tmp_path / "rejected").exists()
