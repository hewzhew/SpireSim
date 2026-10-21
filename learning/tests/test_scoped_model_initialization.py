import json
from unittest.mock import patch

import pytest

torch = pytest.importorskip("torch")

from learning.tests.test_frozen_family_refinement import family_source
from learning.tests.test_scoped_strategic_decisions import _annotation, _overlay, _source
from sts_learning.strategic_behavior_cloning import (
    StrategicBehaviorCloningError, StrategicDecisionFitConfig, main, train_scoped_strategic_decisions,
)
from sts_learning.torch_checkpoints import decode_state_dict


def test_full_initialization_updates_encoder_and_repairs_a_changed_family(tmp_path, family_source):
    roots, source, config, _ = family_source
    original = {p.name: p.read_bytes() for p in source.iterdir()}
    overlay = _overlay(tmp_path, roots, [_annotation(acceptable=(1,))], name="changed.json")
    output = tmp_path / "updated"
    result = train_scoped_strategic_decisions(overlay, output, hidden_dim=12, config=config,
        decision_architecture="family-target", initial_model=source)
    assert result["stop_reason"] == "training_fit"
    assert result["final_training"]["choices"]["adoptable"] == 1
    assert "frozen_encoder_state_unchanged" not in result["validation"]
    assert "frozen_family_state_unchanged" not in result["validation"]
    before = decode_state_dict(original["model.ststorch"])
    after = decode_state_dict((output / "model.ststorch").read_bytes())
    assert any(not torch.equal(value, after[name]) for name, value in before.items()
               if not name.startswith(("scorer.", "family_head.")))
    assert original == {p.name: p.read_bytes() for p in source.iterdir()}
    contract = json.loads((output / "contract.json").read_text())
    assert contract["model_seed"] is None
    assert contract["initialization"]["kind"] == "full-model-initialization"
    assert contract["initialization"]["source_training_seeds"] == [2026101200]
    assert contract["initialization"]["source_evaluation_seeds"] == [2026101204]


def test_full_initialization_reads_exact_weights_before_any_update(tmp_path, family_source):
    roots, source, config, _ = family_source
    overlay = _overlay(tmp_path, roots, name="unchanged.json")
    result = train_scoped_strategic_decisions(overlay, tmp_path / "initialized", hidden_dim=12,
        config=config, decision_architecture="family-target", initial_model=source, model_seed=99)
    assert result["updates"] == 0
    assert result["model_sha256"] == json.loads((source / "summary.json").read_text())["model_sha256"]


@pytest.mark.parametrize("pooling", ["mean", "candidate-attention"])
def test_flat_initialization_preserves_weights_and_keeps_fit_gate_after_budget_miss(tmp_path, pooling):
    roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204)]
    overlay = _overlay(tmp_path, roots)
    source = tmp_path / "flat-source"
    config = StrategicDecisionFitConfig(max_updates=1, fit_margin=1000)
    original = train_scoped_strategic_decisions(overlay, source, hidden_dim=12, config=config,
        observation_pooling=pooling)
    assert original["stop_reason"] == "max_updates"
    initialized = train_scoped_strategic_decisions(overlay, tmp_path / "flat-initialized",
        hidden_dim=12, config=config, initial_model=source, model_seed=99, observation_pooling=pooling)
    assert initialized["initial_training"] == original["final_training"]
    assert initialized["stop_reason"] == "max_updates"
    contract = json.loads((tmp_path / "flat-initialized/contract.json").read_text())
    assert contract["decision_architecture"] == "flat"
    assert contract["scorer"]["observation_pooling"] == pooling
    assert contract["model_seed"] is None
    assert contract["initialization"]["model_sha256"] == original["model_sha256"]
    assert contract["initialization"]["source_training_seeds"] == [2026101200]
    from sts_learning.strategic_model_scope import require_training_fit
    with pytest.raises(ValueError, match="training fit"):
        require_training_fit(contract, initialized)
    with pytest.raises(StrategicBehaviorCloningError, match="scorer contract"):
        train_scoped_strategic_decisions(overlay, tmp_path / "changed-pooling", hidden_dim=12,
            config=config, initial_model=source,
            observation_pooling="candidate-attention" if pooling == "mean" else "mean")


def test_full_initialization_ignores_development_targets_and_carries_ancestry(tmp_path, family_source):
    roots, source, config, _ = family_source
    a = _overlay(tmp_path, roots, [_annotation(acceptable=(1,)), _annotation(1, acceptable=(0,))], name="a.json")
    b = _overlay(tmp_path, roots, [_annotation(acceptable=(1,)), _annotation(1, acceptable=(1,))], name="b.json")
    results = [train_scoped_strategic_decisions(path, tmp_path / name, hidden_dim=12, config=config,
        decision_architecture="family-target", initial_model=source) for path, name in ((a, "a"), (b, "b"))]
    assert results[0]["model_sha256"] == results[1]["model_sha256"]
    assert results[0]["training_trace"] == results[1]["training_trace"]
    train_scoped_strategic_decisions(a, tmp_path / "descendant", hidden_dim=12, config=config,
        decision_architecture="family-target", initial_model=tmp_path / "a")
    inherited = json.loads((tmp_path / "descendant/contract.json").read_text())["initialization"]
    assert inherited["source_training_seeds"] == [2026101200]
    assert inherited["source_evaluation_seeds"] == [2026101204]


@pytest.mark.parametrize("failure", ["source-hash", "source-contract", "reused-evaluation"])
def test_full_initialization_rejects_unbound_sources_before_output(tmp_path, family_source, failure):
    roots, source, config, _ = family_source
    if failure == "source-hash":
        payload = bytearray((source / "model.ststorch").read_bytes())
        payload[-1] ^= 1
        (source / "model.ststorch").write_bytes(payload)
    elif failure == "source-contract":
        contract = json.loads((source / "contract.json").read_text())
        contract["scorer"]["hidden_dim"] = 99
        (source / "contract.json").write_text(json.dumps(contract))
    overlay = _overlay(tmp_path, roots, [_annotation(acceptable=(1,))], name="bad.json")
    with pytest.raises(StrategicBehaviorCloningError):
        train_scoped_strategic_decisions(overlay, tmp_path / "rejected", hidden_dim=12, config=config,
            decision_architecture="family-target", initial_model=source,
            evaluation_role="held_out" if failure == "reused-evaluation" else "development")
    assert not (tmp_path / "rejected").exists()


@pytest.mark.parametrize("frozen", ["frozen_family_model", "frozen_encoder_model"])
def test_full_initialization_is_explicit_and_exclusive(tmp_path, frozen):
    with pytest.raises(StrategicBehaviorCloningError, match="family-target"):
        train_scoped_strategic_decisions(tmp_path / "unused.json", tmp_path / "unused",
                                        **{frozen: tmp_path / "source"})
    with pytest.raises(StrategicBehaviorCloningError, match="mutually exclusive"):
        train_scoped_strategic_decisions(tmp_path / "unused.json", tmp_path / "unused",
            decision_architecture="family-target", initial_model=tmp_path / "a", **{frozen: tmp_path / "b"})
    flag = "--" + frozen.replace("_", "-")
    with patch("sys.argv", ["fit", "--decisions", "unused.json", "--output", "unused",
                            "--initial-model", "a", flag, "b"]):
        with pytest.raises(SystemExit):
            main()
    with patch("sys.argv", ["fit", "--samples", "unused", "--output", "unused", "--initial-model", "a"]):
        with pytest.raises(SystemExit):
            main()
