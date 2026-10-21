from __future__ import annotations

import hashlib
import json

import numpy as np
import pytest

pytest.importorskip("torch")

from learning.tests.semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture
from sts_learning.seeds import SeedPartition, SeedPartitionSpec
from sts_learning.semantic_batch import select_semantic_decision_rows
from sts_learning.semantic_concat import SemanticBatchConcatLimits
from sts_learning.strategic_behavior_cloning import (
    StrategicBehaviorCloningError,
    split_scoped_strategic_preferences,
    train_scoped_strategic_preferences,
)
from sts_learning.strategic_demonstrations import (
    ScopedStrategicSampleRecorder,
    StrategicDemonstrationError,
    load_scoped_strategic_preferences,
    save_scoped_strategic_preferences,
)
from sts_learning.torch_policy import RaggedCandidateLogits, ragged_resolved_pairwise_loss


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


def _annotation(source=0, *, preferred=(0, 2), disfavored=(1,)):
    return dict(source_index=source, row_index=0, preferred=list(preferred),
                disfavored=list(disfavored), rationale="Two acceptable alternatives to B.")


def _overlay(tmp_path, roots, annotations=None, *, name="preferences.json"):
    path = tmp_path / name
    save_scoped_strategic_preferences(
        path, roots, annotations if annotations is not None else [_annotation()],
        reviewer="test reviewer", references=[], protocol={"scope": "partial comparisons"})
    return path


def test_partial_preferences_preserve_sources_multiple_answers_and_unknown(tmp_path):
    roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204),
             _source(tmp_path, 2026101205, committed=False)]
    before = {path: path.read_bytes() for root in roots for path in root.iterdir()}
    path = _overlay(tmp_path, roots)
    corpus = load_scoped_strategic_preferences(path)
    assert corpus.rows[0].preferred_pairs == ((0, 1), (2, 1))
    assert corpus.rows[1].preferred_pairs == ()
    assert corpus.rows[2].preferred_pairs == ()
    assert corpus.overlay["rows"][1]["unknown"] == [0, 1, 2]
    assert all(path.read_bytes() == contents for path, contents in before.items())
    assert corpus.overlay_sha256 == hashlib.sha256(path.read_bytes()).hexdigest()
    assert corpus.overlay["rows"][0]["preferred"] == [0, 2]
    assert corpus.rows[0].episode_seed == 2026101200
    split = split_scoped_strategic_preferences(
        corpus, SeedPartitionSpec(), SemanticBatchConcatLimits(max_rows=4, max_input_array_bytes=1048576))
    assert [row.episode_seed for row in split[SeedPartition.TRAINING].rows] == [2026101200]
    assert [row.episode_seed for row in split[SeedPartition.HELD_OUT].rows] == [2026101204]
    with pytest.raises(FileExistsError):
        _overlay(tmp_path, roots)


@pytest.mark.parametrize("change, message", [
    (lambda label: label.update(preferred=[0, 1]), "conflict"),
    (lambda label: label.update(preferred=[0, 0]), "duplicate preference candidate"),
    (lambda label: label.update(preferred=[3]), "candidate ordinal"),
    (lambda label: label.update(preferred=[True]), "candidate ordinal"),
    (lambda label: label.update(preferred=[0.0]), "candidate ordinal"),
    (lambda label: label.update(unknown=[0]), "conflict"),
    (lambda label: label.update(preferred=[0]), "omit candidates"),
    (lambda label: label.update(source_token="different"), "row source mismatch"),
    (lambda label: label.update(row_sha256="different"), "row source mismatch"),
    (lambda label: label.update(rationale=""), "rationale"),
])
def test_loader_rejects_conflicts_missing_coverage_and_source_mismatch(tmp_path, change, message):
    path = _overlay(tmp_path, [_source(tmp_path, 2026101200)])
    payload = json.loads(path.read_text())
    change(payload["rows"][0])
    path.write_text(json.dumps(payload))
    with pytest.raises(StrategicDemonstrationError, match=message):
        load_scoped_strategic_preferences(path)


def test_pending_labels_duplicate_sources_and_changed_manifests_are_rejected(tmp_path):
    pending = _source(tmp_path, 2026101201, committed=False)
    with pytest.raises(StrategicDemonstrationError, match="uncommitted"):
        _overlay(tmp_path, [pending])
    assert not (tmp_path / "preferences.json").exists()
    root = _source(tmp_path, 2026101200)
    with pytest.raises(StrategicDemonstrationError, match="repeats a source"):
        _overlay(tmp_path, [root, root])
    path = _overlay(tmp_path, [root])
    manifest = root / "manifest.json"
    manifest.write_bytes(manifest.read_bytes() + b" ")
    with pytest.raises(StrategicDemonstrationError, match="manifest hash"):
        load_scoped_strategic_preferences(path)


def test_mixed_producers_and_duplicate_annotations_are_rejected(tmp_path):
    roots = [_source(tmp_path, 2026101200),
             _source(tmp_path, 2026101204, producer={"baseline": "different"})]
    with pytest.raises(StrategicDemonstrationError, match="producers or schemas differ"):
        _overlay(tmp_path, roots)
    with pytest.raises(StrategicDemonstrationError, match="duplicate preference annotation"):
        _overlay(tmp_path, roots[:1], [_annotation(), _annotation()])


def test_explicit_public_observation_preferences_include_pending_inputs_without_commit(tmp_path):
    roots = [_source(tmp_path, 2026101200, committed=False),
             _source(tmp_path, 2026101204, committed=False)]
    path = _overlay(tmp_path, roots, [dict(_annotation(index), basis="public_observation")
                                     for index in range(2)])
    corpus = load_scoped_strategic_preferences(path)
    assert all(not row.committed and row.eligible_for_supervision for row in corpus.rows)
    result = train_scoped_strategic_preferences(path, tmp_path / "pending-preferences", epochs=4)
    assert result["final_training"]["rows"] == 1
    assert result["final_held_out"]["rows"] == 1
    contract = json.loads((tmp_path / "pending-preferences/contract.json").read_text())
    assert contract["pre_adoption_supervised_rows"] == 2
    assert contract["excluded_uncommitted_rows"] == 0
    assert all(not record["committed"] for root in roots
               for record in json.loads((root / "manifest.json").read_text())["rows"])


def test_training_uses_only_training_pairs_and_keeps_unknown_separate(tmp_path):
    roots = [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204),
             _source(tmp_path, 2026101205, committed=False)]
    first = _overlay(tmp_path, roots)
    second = _overlay(tmp_path, roots, [_annotation(), _annotation(1, preferred=(1,), disfavored=(0,))],
                      name="changed-held-out.json")
    before = {path: path.read_bytes() for root in roots for path in root.iterdir()}
    a = train_scoped_strategic_preferences(first, tmp_path / "first-clone", epochs=4)
    b = train_scoped_strategic_preferences(second, tmp_path / "second-clone", epochs=4)
    assert a["model_sha256"] == b["model_sha256"]
    assert a["final_training"]["pairwise_loss"] < a["initial_training"]["pairwise_loss"]
    assert a["final_held_out"]["choices"] == {"preferred": 0, "disfavored": 0, "unknown": 1}
    assert a["final_held_out"]["pairwise_loss"] is None
    assert a["final_training"]["resolved_pairs"] == 2
    assert a["validation"] == {"model_round_trip": True, "candidate_reversal_max_logit_error": 0.0}
    contract = json.loads((tmp_path / "first-clone/contract.json").read_text())
    assert contract["training_seeds"] == [2026101200]
    assert contract["held_out_seeds"] == [2026101204]
    assert contract["excluded_uncommitted_rows"] == 1
    assert all(path.read_bytes() == contents for path, contents in before.items())


def test_all_unknown_training_is_not_fabricated_into_targets(tmp_path):
    path = _overlay(tmp_path, [_source(tmp_path, 2026101200), _source(tmp_path, 2026101204)], [])
    with pytest.raises(StrategicBehaviorCloningError, match="at least one resolved preference"):
        train_scoped_strategic_preferences(path, tmp_path / "clone")
    assert not (tmp_path / "clone").exists()


def test_worst_pair_objective_is_not_diluted_by_duplicate_easy_choices():
    import torch

    # A preferred upgrade, a wrongly top-scored rest, easy basic upgrades,
    # and an unresolved candidate. Adding easy comparisons must not weaken the
    # rest/upgrade constraint, nor label the unresolved candidate.
    values = torch.tensor([0.0, 2.0, -3.0, -3.0, 5.0], requires_grad=True)
    logits = RaggedCandidateLogits(values, torch.tensor([0, 5]))
    loss = ragged_resolved_pairwise_loss(logits, [[(0, 1), (0, 2)]], row_reduction="max")
    repeated = ragged_resolved_pairwise_loss(logits, [[(0, 1), (0, 2), (0, 3)]], row_reduction="max")
    assert torch.equal(loss, repeated)
    loss.backward()
    assert values.grad[0] < 0
    assert values.grad[1] > 0
    assert torch.equal(values.grad[2:], torch.zeros(3))
    mean = ragged_resolved_pairwise_loss(logits, [[(0, 1), (0, 2), (0, 3)]])
    assert mean < loss


def test_worst_pair_loss_does_not_rank_multiple_reasonable_answers():
    import torch

    first = RaggedCandidateLogits(torch.tensor([2.0, 3.0, 0.0]), torch.tensor([0, 3]))
    swapped = RaggedCandidateLogits(torch.tensor([3.0, 2.0, 0.0]), torch.tensor([0, 3]))
    pairs = [[(0, 2), (1, 2)]]
    assert torch.equal(ragged_resolved_pairwise_loss(first, pairs, row_reduction="max"),
                       ragged_resolved_pairwise_loss(swapped, pairs, row_reduction="max"))


def test_satisfied_partial_preferences_do_not_certify_global_choice():
    import torch

    # Every taught comparison is satisfied. The complete action surface still
    # selects the unreviewed candidate, so a preference fit is not a policy fit.
    values = torch.tensor([5.0, -5.0, 100.0], requires_grad=True)
    logits = RaggedCandidateLogits(values, torch.tensor([0, 3]))
    loss = ragged_resolved_pairwise_loss(logits, [[(0, 1)]], row_reduction="max")
    assert loss.item() < 0.0001
    assert logits.greedy_ordinals() == [2]
    loss.backward()
    assert values.grad[2].item() == 0.0
