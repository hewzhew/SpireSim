from __future__ import annotations

import hashlib
import json

import numpy as np
import pytest

torch = pytest.importorskip("torch")

from learning.tests.semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture
from sts_learning.seeds import SeedPartition, SeedPartitionSpec
from sts_learning.semantic_batch import select_semantic_decision_rows
from sts_learning.semantic_concat import SemanticBatchConcatLimits
from sts_learning.strategic_behavior_cloning import (
    _decision_metrics,
    _decision_choice_metrics,
    _decision_fit_reached,
    StrategicBehaviorCloningError,
    StrategicDecisionFitConfig,
    split_scoped_strategic_decisions,
    train_scoped_strategic_decisions,
)
from sts_learning.strategic_demonstrations import (
    ScopedStrategicSampleRecorder,
    StrategicDemonstrationError,
    load_scoped_strategic_decisions,
    save_scoped_strategic_decisions,
)
from sts_learning.torch_policy import (
    RaggedCandidateLogits,
    RaggedFamilyLogits,
    TorchPolicyError,
    ragged_acceptable_set_cross_entropy,
    ragged_cross_entropy,
    family_target_greedy_logits,
)


def _source(tmp_path, seed, *, committed=True, producer=None):
    batch = select_semantic_decision_rows(semantic_batch_fixture(), (1,))
    batch.update(decision_token_ids=[f"token-{seed}"], public_snapshot_ids=[f"snapshot-{seed}"],
                 decision_steps=np.array([3], dtype=np.uint64))
    decision = {"slot": 9, "source_token": f"token-{seed}", "public_snapshot": f"snapshot-{seed}",
                "ordinal": 1, "owner": "baseline", "context": {"seed": seed, "act": 1, "floor": 3},
                "audit": {"decision_site": "card_reward", "candidates": [
                    {"kind": "take_card", "card": "A"},
                    {"kind": "take_card", "card": "B"},
                    {"kind": "skip_card_reward"}]}}
    recorder = ScopedStrategicSampleRecorder()
    recorder(batch, [decision])
    if committed:
        recorder.commit([decision])
    root = tmp_path / str(seed)
    recorder.save(root, schema=semantic_schema_fixture(), producer=producer or {"baseline": "test"},
                  terminal_outcome=None)
    return root


def _annotation(source=0, *, acceptable=(0, 2)):
    return {"source_index": source, "row_index": 0,
            "acceptable": list(acceptable) if acceptable is not None else None,
            "rationale": "Teacher would adopt either A or skip on the entire surface."}


def _overlay(tmp_path, roots, annotations=None, *, name="decisions.json", protocol=None):
    path = tmp_path / name
    save_scoped_strategic_decisions(
        path, roots, annotations if annotations is not None else [_annotation()],
        author="Codex test teacher", revision="v1", references=[],
        protocol=protocol or {"scope": "complete decisions"})
    return path


def test_decisions_round_trip_keeps_complete_sets_separate_from_adopted_actions(tmp_path):
    roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204),
             _source(tmp_path, 2026101205, committed=False)]
    before = {path: path.read_bytes() for root in roots for path in root.iterdir()}
    path = _overlay(tmp_path, roots)
    corpus = load_scoped_strategic_decisions(path)
    assert corpus.rows[0].acceptable_ordinals == (0, 2)
    assert corpus.rows[0].preferred_ordinals is None
    assert corpus.rows[0].choice_target_ordinals == (0, 2)
    assert corpus.overlay["schema"] == "sts-scoped-strategic-decisions-v1"
    assert all("preferred" not in row for row in corpus.overlay["rows"])
    assert corpus.rows[1].acceptable_ordinals is None
    assert corpus.rows[2].acceptable_ordinals is None
    assert corpus.overlay["author"] == "Codex test teacher"
    assert corpus.overlay["revision"] == "v1"
    assert corpus.overlay_sha256 == hashlib.sha256(path.read_bytes()).hexdigest()
    split = split_scoped_strategic_decisions(
        corpus, SeedPartitionSpec(), SemanticBatchConcatLimits(max_rows=4, max_input_array_bytes=1048576))
    assert split[SeedPartition.TRAINING].acceptable_ordinals == ((0, 2),)
    assert split[SeedPartition.HELD_OUT].acceptable_ordinals == (None,)
    assert all(path.read_bytes() == contents for path, contents in before.items())
    with pytest.raises(FileExistsError):
        _overlay(tmp_path, roots)


def test_full_decision_preferred_subset_round_trip_keeps_acceptability_and_unknown(tmp_path):
    roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204)]
    path = _overlay(tmp_path, roots, [dict(_annotation(), preferred=[2])])
    corpus = load_scoped_strategic_decisions(path)
    assert corpus.overlay["schema"] == "sts-scoped-strategic-decisions-v2"
    assert corpus.rows[0].acceptable_ordinals == (0, 2)
    assert corpus.rows[0].preferred_ordinals == (2,)
    assert corpus.rows[0].choice_target_ordinals == (2,)
    assert corpus.rows[1].choice_target_ordinals is None
    assert corpus.overlay["rows"][1]["preferred"] is None


@pytest.mark.parametrize("change, message", [
    (lambda row: row.update(preferred=[]), "nonempty subset"),
    (lambda row: row.update(preferred=[1]), "nonempty subset"),
    (lambda row: row.update(preferred=[2, 2]), "duplicate preferred decision candidate"),
    (lambda row: row.update(preferred=[3]), "candidate ordinal"),
    (lambda row: row.update(preferred=[True]), "candidate ordinal"),
    (lambda row: row.update(preferred=[2.0]), "candidate ordinal"),
    (lambda row: row.update(acceptable=None), "nonempty subset"),
    (lambda row: row.pop("preferred"), "overlay schema"),
])
def test_preferred_targets_require_a_complete_known_acceptable_set(tmp_path, change, message):
    path = _overlay(tmp_path, [_source(tmp_path, 2026101200)], [dict(_annotation(), preferred=[2])])
    payload = json.loads(path.read_text())
    change(payload["rows"][0])
    path.write_text(json.dumps(payload))
    with pytest.raises(StrategicDemonstrationError, match=message):
        load_scoped_strategic_decisions(path)


def test_legacy_decision_schema_cannot_silently_ignore_a_preferred_target(tmp_path):
    path = _overlay(tmp_path, [_source(tmp_path, 2026101200)])
    payload = json.loads(path.read_text())
    payload["rows"][0]["preferred"] = [2]
    path.write_text(json.dumps(payload))
    with pytest.raises(StrategicDemonstrationError, match="overlay schema"):
        load_scoped_strategic_decisions(path)


def test_secondary_acceptable_is_visible_and_cannot_qualify_recommendation_fit(tmp_path):
    corpus = load_scoped_strategic_decisions(_overlay(tmp_path, [_source(tmp_path, 2026101200)],
        [dict(_annotation(acceptable=(0, 1, 2)), preferred=[2])]))
    partition = split_scoped_strategic_decisions(
        corpus, SeedPartitionSpec(), SemanticBatchConcatLimits(4, 1048576))[SeedPartition.TRAINING]
    metrics = _decision_choice_metrics(partition, [0], [5., 0., 1.], [0, 3])
    assert metrics["choices"]["adoptable"] == 1
    assert metrics["minimum_margin"] is None  # Every legal choice is acceptable.
    assert metrics["recommendations"]["secondary_acceptable"] == 1
    assert metrics["minimum_recommendation_margin"] == -4
    assert not _decision_fit_reached(metrics, StrategicDecisionFitConfig())
    tied = _decision_choice_metrics(partition, [2], [1., 0., 1.], [0, 3])
    assert tied["recommendations"]["recommended"] == 1
    assert not _decision_fit_reached(tied, StrategicDecisionFitConfig(fit_margin=0.5))
    fitted = _decision_choice_metrics(partition, [2], [1., 0., 2.], [0, 3])
    assert _decision_fit_reached(fitted, StrategicDecisionFitConfig(fit_margin=0.5))


def test_explicit_recommendations_are_not_inflated_by_acceptable_fallback_rows(tmp_path):
    from types import SimpleNamespace
    roots = [_source(tmp_path, seed) for seed in range(2026101200, 2026101204)]
    annotations = [dict(_annotation(0, acceptable=(0, 1, 2)), preferred=[2]),
                   _annotation(1), _annotation(2, acceptable=None), _annotation(3)]
    corpus = load_scoped_strategic_decisions(_overlay(tmp_path, roots, annotations))
    partition = SimpleNamespace(rows=corpus.rows)
    metrics = _decision_choice_metrics(partition, [0, 2, 0, 1],
        [5., 0., 1., 0., 0., 2., 1., 0., 0., 0., 2., 0.], [0, 3, 6, 9, 12])
    assert metrics["preferred_rows"] == 1
    assert metrics["target_basis_counts"] == {
        "declared_preferred": 1, "acceptable_fallback": 2, "unknown": 1}
    assert metrics["recommendations"] == {
        "recommended": 1, "secondary_acceptable": 1, "outside_set": 1, "unknown": 1}
    assert metrics["explicit_preferred_choices"] == {
        "recommended": 0, "secondary_acceptable": 1, "outside_set": 0}
    assert metrics["minimum_explicit_preferred_margin"] == -4
    assert [row["target_basis"] for row in metrics["predictions"]] == [
        "declared_preferred", "acceptable_fallback", "unknown", "acceptable_fallback"]
    assert not _decision_fit_reached(metrics, StrategicDecisionFitConfig())


@pytest.mark.parametrize("architecture", ["flat", "family-target"])
def test_neural_fit_learns_preferred_subset_without_using_evaluation_preference(tmp_path, architecture):
    from unittest.mock import patch
    fixture = semantic_schema_fixture()
    fixture["categorical_field"]["ActionKind"] = fixture["categorical_field"].pop("Flag")
    with patch(__name__ + ".semantic_schema_fixture", return_value=fixture):
        roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204)]
    paths = [_overlay(tmp_path, roots, [dict(_annotation(acceptable=(0, 1, 2)), preferred=[2]),
        dict(_annotation(1, acceptable=(0, 1, 2)), preferred=[preferred])], name=f"preferred-{preferred}.json")
        for preferred in (0, 2)]
    config = StrategicDecisionFitConfig(max_updates=128, check_every=4, learning_rate=0.01)
    results = [train_scoped_strategic_decisions(path, tmp_path/f"{architecture}-{i}", config=config,
        decision_architecture=architecture) for i, path in enumerate(paths)]
    a, b = results
    assert a["stop_reason"] == "training_fit"
    assert a["final_training"]["recommendations"]["recommended"] == 1
    assert a["final_training"]["predictions"][0]["ordinal"] == 2
    assert a["final_training"]["minimum_margin"] is None
    assert a["final_training"]["minimum_recommendation_margin"] >= config.fit_margin
    assert a["model_sha256"] == b["model_sha256"]
    assert a["training_trace"] == b["training_trace"]
    assert a["final_evaluation"]["recommendations"]["secondary_acceptable"] == 1
    assert b["final_evaluation"]["recommendations"]["recommended"] == 1


@pytest.mark.parametrize("change, message", [
    (lambda label: label.update(acceptable=[]), "nonempty"),
    (lambda label: label.update(acceptable=[0, 0]), "duplicate decision candidate"),
    (lambda label: label.update(acceptable=[3]), "candidate ordinal"),
    (lambda label: label.update(acceptable=[True]), "candidate ordinal"),
    (lambda label: label.update(acceptable=[0.0]), "candidate ordinal"),
    (lambda label: label.update(source_token="different"), "row source mismatch"),
    (lambda label: label.update(row_sha256="different"), "row source mismatch"),
    (lambda label: label.update(rationale=""), "rationale"),
    (lambda label: label.pop("acceptable"), "explicit acceptable"),
])
def test_decision_loader_rejects_malformed_or_unbound_targets(tmp_path, change, message):
    path = _overlay(tmp_path, [_source(tmp_path, 2026101200)])
    payload = json.loads(path.read_text())
    change(payload["rows"][0])
    path.write_text(json.dumps(payload))
    with pytest.raises(StrategicDemonstrationError, match=message):
        load_scoped_strategic_decisions(path)


def test_partial_preferences_cannot_be_silently_promoted_to_decisions(tmp_path):
    root = _source(tmp_path, 2026101200)
    label = dict(source_index=0, row_index=0, preferred=[0], disfavored=[1], rationale="A beats B.")
    with pytest.raises(StrategicDemonstrationError, match="explicit acceptable"):
        _overlay(tmp_path, [root], [label])
    path = _overlay(tmp_path, [root])
    payload = json.loads(path.read_text())
    payload["schema"] = "sts-scoped-strategic-preferences-v1"
    path.write_text(json.dumps(payload))
    with pytest.raises(StrategicDemonstrationError, match="unsupported scoped decision"):
        load_scoped_strategic_decisions(path)


def test_explicit_public_observation_targets_train_without_an_adopted_action(tmp_path):
    root = _source(tmp_path, 2026101200, committed=False)
    manifest_path = root / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["rows"][0]["decision"].update(ordinal=None, owner="observation_only")
    manifest["rows"][0]["label_source"] = "observation_only"
    manifest_path.write_text(json.dumps(manifest))
    held_out = _source(tmp_path, 2026101204, committed=False)
    annotations = [dict(_annotation(index), basis="public_observation") for index in range(2)]
    path = _overlay(tmp_path, [root, held_out], annotations)
    corpus = load_scoped_strategic_decisions(path)
    assert not corpus.rows[0].committed
    assert corpus.rows[0].supervision_basis == "public_observation"
    assert corpus.rows[0].eligible_for_supervision
    before = manifest_path.read_bytes()
    result = train_scoped_strategic_decisions(path, tmp_path / "observation-clone",
        config=StrategicDecisionFitConfig(max_updates=64, check_every=4, learning_rate=0.01))
    assert result["stop_reason"] == "training_fit"
    assert result["final_training"]["choices"]["adoptable"] == 1
    contract = json.loads((tmp_path / "observation-clone/contract.json").read_text())
    assert contract["pre_adoption_supervised_rows"] == 2
    assert contract["excluded_uncommitted_rows"] == 0
    assert contract["training_seeds"] == [2026101200]
    assert contract["evaluation_seeds"] == [2026101204]
    assert manifest_path.read_bytes() == before


def test_unsupported_target_basis_is_rejected_before_publishing(tmp_path):
    annotation = dict(_annotation(), basis="search_win_oracle")
    with pytest.raises(StrategicDemonstrationError, match="supervision basis"):
        _overlay(tmp_path, [_source(tmp_path, 2026101200)], [annotation])
    assert not (tmp_path / "decisions.json").exists()


def test_decisions_reject_pending_labels_duplicate_sources_and_manifest_changes(tmp_path):
    pending = _source(tmp_path, 2026101201, committed=False)
    with pytest.raises(StrategicDemonstrationError, match="uncommitted"):
        _overlay(tmp_path, [pending])
    root = _source(tmp_path, 2026101200)
    with pytest.raises(StrategicDemonstrationError, match="repeats a source"):
        _overlay(tmp_path, [root, root])
    with pytest.raises(StrategicDemonstrationError, match="duplicate decision annotation"):
        _overlay(tmp_path, [root], [_annotation(), _annotation()])
    other = _source(tmp_path, 2026101204, producer={"baseline": "different"})
    with pytest.raises(StrategicDemonstrationError, match="producers or schemas differ"):
        _overlay(tmp_path, [root, other])
    path = _overlay(tmp_path, [root])
    manifest = root / "manifest.json"
    manifest.write_bytes(manifest.read_bytes() + b" ")
    with pytest.raises(StrategicDemonstrationError, match="manifest hash"):
        load_scoped_strategic_decisions(path)


def _compatible_sources(tmp_path):
    native_digest = "a" * 64
    roots = [_source(tmp_path, 2026101200, producer={"native_sha256": native_digest,
                                                  "source_kind": "constructed fixture"}),
             _source(tmp_path, 2026101204, producer={"executor_binding": {
                 "native_sha256": native_digest}, "source_kind": "run fixture"})]
    sampling_digest = json.loads((roots[0] / "manifest.json").read_text())["sampling_source_sha256"]
    compatibility = {"schema": "sts-scoped-source-compatibility-v1",
                     "native_sha256": native_digest,
                     "rationale": "Same numerical producers; keep fixture origins separately.",
                     "sources": [{"source_index": index, "origin": "fixture",
                                  "sampling_source_sha256": sampling_digest,
                                  "input_access": "public_semantic_only",
                                  "continuation_access": "constructed_transition",
                                  "rationale": "Test fixture, not a natural game."}
                                 for index in range(2)]}
    return roots, {"scope": "complete decisions", "source_compatibility": compatibility}


def test_explicit_compatible_sources_keep_origins_and_partitions_in_trained_contract(tmp_path):
    roots, protocol = _compatible_sources(tmp_path)
    before = {path: path.read_bytes() for root in roots for path in root.iterdir()}
    path = _overlay(tmp_path, roots, [_annotation(), _annotation(1)], protocol=protocol)
    corpus = load_scoped_strategic_decisions(path)
    assert corpus.producer["schema"] == "sts-scoped-multi-source-producer-v1"
    for index, root in enumerate(roots):
        manifest = json.loads((root / "manifest.json").read_text())
        record = corpus.producer["sources"][index]
        assert record["producer"] == manifest["producer"]
        assert record["manifest_sha256"] == corpus.overlay["sources"][index]["manifest_sha256"]
        assert record["permissions"] == protocol["source_compatibility"]["sources"][index]
    result = train_scoped_strategic_decisions(path, tmp_path / "mixed-clone",
        config=StrategicDecisionFitConfig(max_updates=64, check_every=4, learning_rate=0.01))
    assert result["stop_reason"] == "training_fit"
    contract = json.loads((tmp_path / "mixed-clone/contract.json").read_text())
    assert contract["producer"] == corpus.producer
    assert contract["training_seeds"] == [2026101200]
    assert contract["evaluation_seeds"] == [2026101204]
    assert all(path.read_bytes() == contents for path, contents in before.items())


def test_preferences_use_the_same_explicit_source_compatibility_contract(tmp_path):
    from sts_learning.strategic_demonstrations import (
        load_scoped_strategic_preferences, save_scoped_strategic_preferences,
    )
    roots, protocol = _compatible_sources(tmp_path)
    path = tmp_path / "preferences.json"
    save_scoped_strategic_preferences(path, roots,
        [{"source_index": index, "row_index": 0, "preferred": [0], "disfavored": [1],
          "rationale": "A is preferred to B; skip remains Unknown."} for index in range(2)],
        reviewer="Codex test teacher", references=[], protocol=protocol)
    corpus = load_scoped_strategic_preferences(path)
    assert corpus.producer["sources"][1]["producer"]["source_kind"] == "run fixture"
    assert corpus.rows[0].preferred_ordinals == (0,)
    assert corpus.rows[1].disfavored_ordinals == (1,)


def test_reviewed_sampling_versions_can_differ_without_rewriting_producer_identity(tmp_path):
    roots, protocol = _compatible_sources(tmp_path)
    manifest_path = roots[1] / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["sampling_source_sha256"] = "b" * 64
    manifest_path.write_text(json.dumps(manifest))
    permission = protocol["source_compatibility"]["sources"][1]
    permission.update(sampling_source_sha256="b" * 64,
                      rationale="A separately reviewed fixture sampling version with the same schema.")
    path = _overlay(tmp_path, roots, protocol=protocol)
    corpus = load_scoped_strategic_decisions(path)
    assert corpus.producer["sources"][1]["permissions"]["sampling_source_sha256"] == "b" * 64


@pytest.mark.parametrize("change, message", [
    (lambda declaration: declaration.pop("native_sha256"), "requires native_sha256"),
    (lambda declaration: declaration.update(native_sha256="b" * 64), "numerical producers differ"),
    (lambda declaration: declaration["sources"][1].update(sampling_source_sha256="c" * 64), "numerical producers differ"),
    (lambda declaration: declaration["sources"][1].pop("sampling_source_sha256"), "require sampling_source_sha256"),
    (lambda declaration: declaration.update(rationale=""), "explicitly justified"),
    (lambda declaration: declaration["sources"].pop(), "cover every source"),
    (lambda declaration: declaration["sources"][1].update(source_index=0), "permissions"),
    (lambda declaration: declaration["sources"][1].update(input_access="hidden_state"), "permissions"),
    (lambda declaration: declaration["sources"][1].pop("continuation_access"), "permissions"),
])
def test_source_compatibility_cannot_bypass_producer_and_permission_checks(tmp_path, change, message):
    roots, protocol = _compatible_sources(tmp_path)
    change(protocol["source_compatibility"])
    with pytest.raises(StrategicDemonstrationError, match=message):
        _overlay(tmp_path, roots, protocol=protocol)
    assert not (tmp_path / "decisions.json").exists()


@pytest.mark.parametrize("field", ["semantic_schema", "sampling_source_sha256", "producer"])
def test_explicit_compatibility_rechecks_every_source_at_load(tmp_path, field):
    roots, protocol = _compatible_sources(tmp_path)
    path = _overlay(tmp_path, roots, protocol=protocol)
    manifest_path = roots[1] / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    if field == "semantic_schema":
        manifest[field]["scalar_field"]["extra"] = 99
        message = "producers or schemas differ"
    elif field == "producer":
        manifest[field]["native_sha256"] = "b" * 64  # Conflicts with its executor binding.
        message = "numerical producers differ"
    else:
        manifest[field] = "b" * 64
        message = "numerical producers differ"
    manifest_path.write_text(json.dumps(manifest))
    overlay = json.loads(path.read_text())
    overlay["sources"][1]["manifest_sha256"] = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    path.write_text(json.dumps(overlay))
    with pytest.raises(StrategicDemonstrationError, match=message):
        load_scoped_strategic_decisions(path)


def test_set_loss_matches_single_answer_ce_and_constrains_every_competitor():
    values = torch.tensor([5.0, -5.0, 100.0, 1.0, 2.0], requires_grad=True)
    logits = RaggedCandidateLogits(values, torch.tensor([0, 3, 5]))
    assert torch.allclose(ragged_acceptable_set_cross_entropy(logits, [(0,), (1,)]),
                          ragged_cross_entropy(logits, [0, 1]))
    loss = ragged_acceptable_set_cross_entropy(logits, [(0,), None])
    loss.backward()
    assert values.grad[0] < 0
    assert values.grad[2] > 0  # The old pair loss did not constrain this top action.
    assert torch.equal(values.grad[3:], torch.zeros(2))
    assert logits.greedy_ordinals() == [2, 1]  # Labels do not filter inference.


def test_multiple_answers_are_unordered_and_rows_have_equal_weight():
    values = torch.tensor([2.0, 3.0, 0.0, 1.0, 2.0])
    logits = RaggedCandidateLogits(values, torch.tensor([0, 3, 5]))
    first = ragged_acceptable_set_cross_entropy(logits, [(0, 1), (1,)])
    second = ragged_acceptable_set_cross_entropy(logits, [(1, 0), (1,)])
    expected = (torch.logsumexp(values[:3], 0) - torch.logsumexp(values[:2], 0)
                + torch.logsumexp(values[3:], 0) - values[4]) / 2
    assert torch.equal(first, second)
    assert torch.allclose(first, expected)
    assert ragged_acceptable_set_cross_entropy(logits, [(0, 1, 2), None]).item() == 0


@pytest.mark.parametrize("targets", [[()], [[0, 0]], [[3]], [[-1]], [[0.0]], [[True]], [None], []])
def test_set_loss_rejects_invalid_targets(targets):
    logits = RaggedCandidateLogits(torch.tensor([1., 2., 3.]), torch.tensor([0, 3]))
    with pytest.raises(TorchPolicyError):
        ragged_acceptable_set_cross_entropy(logits, targets)


def test_complete_fit_stop_and_weights_ignore_evaluation_labels(tmp_path):
    roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204),
             _source(tmp_path, 2026101205, committed=False)]
    first = _overlay(tmp_path, roots)
    second = _overlay(tmp_path, roots, [_annotation(), _annotation(1, acceptable=(1,))],
                      name="changed-evaluation.json")
    config = StrategicDecisionFitConfig(max_updates=64, check_every=4, learning_rate=0.01)
    before = {path: path.read_bytes() for root in roots for path in root.iterdir()}
    a = train_scoped_strategic_decisions(first, tmp_path / "first-clone", config=config)
    b = train_scoped_strategic_decisions(second, tmp_path / "second-clone", config=config)
    assert a["model_sha256"] == b["model_sha256"]
    assert a["training_trace"] == b["training_trace"]
    assert a["stop_reason"] == "training_fit"
    assert a["updates"] <= 64
    assert a["final_training"]["choices"] == {"adoptable": 1, "outside_set": 0, "unknown": 0}
    assert a["final_training"]["minimum_margin"] >= 0.5
    assert a["final_evaluation"]["choices"] == {"adoptable": 0, "outside_set": 0, "unknown": 1}
    assert a["validation"] == {"model_round_trip": True, "candidate_reversal_max_logit_error": 0.0}
    contract = json.loads((tmp_path / "first-clone/contract.json").read_text())
    assert contract["training_seeds"] == [2026101200]
    assert contract["evaluation_seeds"] == [2026101204]
    assert contract["model_sites"] == ["card_reward"]
    assert contract["applicability"] is None  # Offline fit does not authorize live use.
    assert contract["excluded_uncommitted_rows"] == 1
    assert all(path.read_bytes() == contents for path, contents in before.items())


def test_unknown_training_has_no_fabricated_target_and_budget_stop_is_explicit(tmp_path):
    roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204)]
    unknown = _overlay(tmp_path, roots, [], name="unknown.json")
    with pytest.raises(StrategicBehaviorCloningError, match="complete decision target"):
        train_scoped_strategic_decisions(unknown, tmp_path / "unknown-clone")
    assert not (tmp_path / "unknown-clone").exists()
    path = _overlay(tmp_path, roots)
    result = train_scoped_strategic_decisions(path, tmp_path / "bounded-clone",
        config=StrategicDecisionFitConfig(max_updates=1, fit_margin=1e6))
    assert result["updates"] == 1
    assert result["stop_reason"] == "max_updates"


def test_fitting_does_not_require_evaluation_labels_or_a_separate_teacher(tmp_path):
    path = _overlay(tmp_path, [_source(tmp_path, 2026101200)])
    result = train_scoped_strategic_decisions(path, tmp_path / "train-only",
        config=StrategicDecisionFitConfig(max_updates=64, learning_rate=0.01, check_every=4))
    assert result["stop_reason"] == "training_fit"
    assert result["final_evaluation"] is None
    assert result["evaluation_role"] == "not_provided"
    assert result["validation"]["model_round_trip"] is True


@pytest.mark.parametrize("kwargs", [
    {"max_updates": 0}, {"max_updates": True}, {"check_every": 0},
    {"learning_rate": 0}, {"learning_rate": float("nan")},
    {"fit_margin": -1}, {"fit_margin": float("inf")}, {"fit_margin": True},
])
def test_fit_config_rejects_ambiguous_or_unbounded_values(kwargs):
    with pytest.raises(StrategicBehaviorCloningError):
        StrategicDecisionFitConfig(**kwargs)


@pytest.mark.parametrize("extra", [["--epochs", "4"], ["--preference-reduction", "mean"]])
def test_complete_action_cli_does_not_silently_ignore_other_objective_options(monkeypatch, extra):
    import sys
    from sts_learning.strategic_behavior_cloning import main

    monkeypatch.setattr(sys, "argv", ["clone", "--decisions", "unused.json", "--output", "unused", *extra])
    with pytest.raises(SystemExit) as error:
        main()
    assert error.value.code == 2


@pytest.mark.parametrize("pooling", ["vector-max", "candidate-score-max"])
def test_family_target_fit_contract_and_evaluation_labels_do_not_select_weights(tmp_path, pooling):
    # Keep the source fixture small; give its synthetic categorical field the
    # ActionKind name before recording immutable rows.
    from unittest.mock import patch
    fixture = semantic_schema_fixture()
    fixture["categorical_field"]["ActionKind"] = fixture["categorical_field"].pop("Flag")
    with patch(__name__ + ".semantic_schema_fixture", return_value=fixture):
        roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204)]
    first = _overlay(tmp_path, roots)
    second = _overlay(tmp_path, roots, [_annotation(), _annotation(1, acceptable=(1,))],
                      name="changed-evaluation.json")
    config = StrategicDecisionFitConfig(max_updates=64, check_every=4, learning_rate=0.01)
    a = train_scoped_strategic_decisions(first, tmp_path / "family-first", config=config,
                                       decision_architecture="family-target", family_pooling=pooling)
    b = train_scoped_strategic_decisions(second, tmp_path / "family-second", config=config,
                                       decision_architecture="family-target", family_pooling=pooling)
    assert a["model_sha256"] == b["model_sha256"]
    assert a["training_trace"] == b["training_trace"]
    assert a["stop_reason"] == "training_fit"
    assert a["final_training"]["family_choices"]["adoptable"] == 1
    assert a["final_training"]["target_choices"]["adoptable"] == 1
    assert a["validation"]["model_round_trip"]
    contract = json.loads((tmp_path / "family-first/contract.json").read_text())
    assert contract["decision_architecture"] == "family-target"
    assert contract["scorer"]["family_field"] == 1
    assert contract["scorer"]["family_pooling"] == pooling


def test_family_only_training_keeps_target_choices_unlearned_and_cannot_load_as_player(tmp_path):
    from unittest.mock import patch
    from sts_learning.strategic_rollout import load_scoped_model, ModelSite
    fixture = semantic_schema_fixture()
    fixture["categorical_field"]["ActionKind"] = fixture["categorical_field"].pop("Flag")
    with patch(__name__ + ".semantic_schema_fixture", return_value=fixture):
        roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204)]
    # Targets 0 and 2 are different actions with the same native ActionKind.
    first = _overlay(tmp_path, roots, [_annotation(acceptable=(0,)), _annotation(1, acceptable=(0,))])
    second = _overlay(tmp_path, roots, [_annotation(acceptable=(2,)), _annotation(1, acceptable=(1,))],
                      name="different-target-and-evaluation.json")
    config = StrategicDecisionFitConfig(max_updates=64, check_every=4, learning_rate=0.01)
    results = [train_scoped_strategic_decisions(path, tmp_path/f"family-only-{i}", config=config,
        decision_architecture="family-target", family_pooling="candidate-score-max",
        supervision_level="action-family") for i,path in enumerate((first,second))]
    a,b = results
    assert a["model_sha256"] == b["model_sha256"]
    assert a["training_trace"] == b["training_trace"]
    assert a["stop_reason"] == "training_fit"
    assert a["final_training"]["supervised_rows"] == 1
    assert a["final_training"]["recommendations"]["recommended"] == 1
    assert a["final_evaluation"]["recommendations"]["recommended"] == 1
    assert b["final_evaluation"]["recommendations"]["outside_set"] == 1
    assert all("ordinal" not in prediction for prediction in a["final_training"]["predictions"])
    contract = json.loads((tmp_path/"family-only-0/contract.json").read_text())
    assert contract["schema"] == "sts-scoped-family-clone-v1"
    assert contract["informative_training_rows"] == 1
    assert contract["stop"] == ("training family argmax in derived target set on every informative row; "
                                "minimum family margin reached")
    with pytest.raises(ValueError, match="decision contract differs"):
        load_scoped_model(tmp_path/"family-only-0", schema=fixture, model_sites=[ModelSite.CARD_REWARD])
    for source_argument in ("initial_model", "frozen_family_model", "frozen_encoder_model"):
        with pytest.raises(StrategicBehaviorCloningError, match="refinement source schema or scorer contract differs"):
            train_scoped_strategic_decisions(first, tmp_path/source_argument, config=config,
                decision_architecture="family-target", family_pooling="candidate-score-max",
                **{source_argument: tmp_path/"family-only-0"})
        assert not (tmp_path/source_argument).exists()


def test_family_only_training_rejects_rows_without_a_family_contrast(tmp_path):
    from unittest.mock import patch
    fixture = semantic_schema_fixture()
    fixture["categorical_field"]["ActionKind"] = fixture["categorical_field"].pop("Flag")
    with patch(__name__ + ".semantic_schema_fixture", return_value=fixture):
        root = _source(tmp_path, 2026101200)
    path = _overlay(tmp_path, [root], [_annotation(acceptable=(0, 1, 2))])
    with pytest.raises(StrategicBehaviorCloningError, match="informative family target"):
        train_scoped_strategic_decisions(path, tmp_path/"no-contrast", decision_architecture="family-target",
                                        supervision_level="action-family")
    assert not (tmp_path/"no-contrast").exists()


def test_family_only_training_requires_native_action_kind(tmp_path):
    path = _overlay(tmp_path, [_source(tmp_path, 2026101200)], [_annotation(acceptable=(0,))])
    with pytest.raises(StrategicBehaviorCloningError, match="requires bridge ActionKind"):
        train_scoped_strategic_decisions(path, tmp_path/"missing-family", decision_architecture="family-target",
                                        supervision_level="action-family")
    assert not (tmp_path/"missing-family").exists()


@pytest.mark.parametrize("kwargs", [{"decision_architecture": "flat"},
    {"decision_architecture": "family-target", "initial_model": "unused"},
    {"decision_architecture": "family-target", "frozen_family_model": "unused"},
    {"decision_architecture": "family-target", "frozen_encoder_model": "unused"}])
def test_family_only_training_rejects_other_task_initializations(tmp_path, kwargs):
    with pytest.raises(StrategicBehaviorCloningError, match="requires fresh family-target"):
        train_scoped_strategic_decisions("unused", tmp_path/"invalid-task",
                                        supervision_level="action-family", **kwargs)
    assert not (tmp_path/"invalid-task").exists()


def test_complete_action_metrics_distinguish_informative_from_trivial_targets(tmp_path):
    from sts_learning.strategic_behavior_cloning import StrategicDecisionPartition
    from dataclasses import replace
    corpus = load_scoped_strategic_decisions(_overlay(tmp_path, [_source(tmp_path, 2026101200)]))
    row = corpus.rows[0]
    rows = (replace(row, acceptable_ordinals=None), replace(row, acceptable_ordinals=(0, 1, 2)),
            replace(row, acceptable_ordinals=(0, 2)), replace(row, acceptable_ordinals=(0, 1, 2), preferred_ordinals=(2,)))
    partition = StrategicDecisionPartition({}, rows)
    result = _decision_choice_metrics(partition, [0, 0, 0, 2], [3., 2., 1.]*4, [0, 3, 6, 9, 12])
    assert result["choice_supervision_signal"] == {"unknown": 1, "informative": 2, "all_candidates_allowed": 1}


def test_complete_action_metrics_separate_family_errors_from_target_errors(tmp_path):
    from sts_learning.strategic_behavior_cloning import StrategicDecisionPartition
    from dataclasses import replace
    corpus = load_scoped_strategic_decisions(_overlay(tmp_path, [_source(tmp_path, 2026101200)]))
    row = corpus.rows[0]
    rows = (replace(row, acceptable_ordinals=(0,)), replace(row, acceptable_ordinals=(0,)),
            replace(row, acceptable_ordinals=(0, 2)), replace(row, acceptable_ordinals=None))
    result = _decision_choice_metrics(StrategicDecisionPartition({}, rows), [1, 2, 0, 1],
        [3., 2., 1.]*4, [0, 3, 6, 9, 12], families=[7, 7, 9]*4)
    assert result["recommendation_error_components"] == {"wrong_family": 1, "wrong_target_within_allowed_family": 1}
    assert result["family_supervision_signal"] == {"unknown": 1, "informative": 2, "all_families_allowed": 1}


@pytest.mark.parametrize("family_values, target_values, expected_family, expected_target", [
    ([2., 0.], [1., 9., 0.], "adoptable", "outside_set"),
    ([0., 2.], [1., 9., 0.], "outside_set", "unknown"),
])
def test_family_metrics_keep_wrong_targets_and_wrong_families_visible(
        tmp_path, family_values, target_values, expected_family, expected_target):
    from unittest.mock import Mock
    corpus = load_scoped_strategic_decisions(_overlay(
        tmp_path, [_source(tmp_path, 2026101200)], [_annotation(acceptable=(0,))]))
    partition = split_scoped_strategic_decisions(
        corpus, SeedPartitionSpec(), SemanticBatchConcatLimits(4, 1048576))[SeedPartition.TRAINING]
    families = RaggedFamilyLogits(torch.tensor(family_values), torch.tensor([0, 2]),
                                 torch.tensor([0, 0, 1]), torch.tensor(target_values))
    scorer = Mock(return_value=family_target_greedy_logits(families, torch.tensor([0, 3])))
    metrics = _decision_metrics(scorer, partition)
    assert metrics["choices"]["outside_set"] == 1
    assert metrics["family_choices"][expected_family] == 1
    assert metrics["target_choices"][expected_target] == 1
    assert metrics["predictions"][0]["family_status"] == expected_family
    assert metrics["predictions"][0]["target_status"] == expected_target


@pytest.mark.parametrize("extra", [["--decision-architecture", "family-target"],
                                   ["--family-pooling", "candidate-score-max"]])
def test_family_architecture_cli_requires_complete_decision_supervision(monkeypatch, extra):
    import sys
    from sts_learning.strategic_behavior_cloning import main
    monkeypatch.setattr(sys, "argv", ["clone", "--samples", "unused", "--output", "unused", *extra])
    with pytest.raises(SystemExit) as error:
        main()
    assert error.value.code == 2
