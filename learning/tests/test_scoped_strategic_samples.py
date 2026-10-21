from __future__ import annotations

import json

import numpy as np
import pytest

pytest.importorskip("torch")

from learning.tests.semantic_fixtures import semantic_batch_fixture, semantic_schema_fixture
from sts_learning.semantic_batch import select_semantic_decision_rows
from sts_learning.strategic_demonstrations import (
    ScopedStrategicSampleRecorder, StrategicDemonstrationError, load_scoped_strategic_samples,
)


def sample(seed, *, committed=True):
    batch = select_semantic_decision_rows(semantic_batch_fixture(), (0,))
    batch.update(decision_token_ids=[f"token-{seed}"], public_snapshot_ids=[f"snapshot-{seed}"],
                 decision_steps=np.array([3], dtype=np.uint64))
    decision = {"slot": 4, "source_token": f"token-{seed}", "public_snapshot": f"snapshot-{seed}",
                "ordinal": 1, "owner": "baseline", "context": {"seed": seed, "act": 1, "floor": 3},
                "audit": {"decision_site": "card_reward", "candidates": [{"kind": "take_card"},
                                                                            {"kind": "skip_card_reward"}]}}
    recorder = ScopedStrategicSampleRecorder()
    recorder(batch, [decision])
    if committed:
        recorder.commit([decision])
    return recorder, batch


def test_public_numeric_round_trip_keeps_commit_and_label_authority_separate(tmp_path):
    recorder, original = sample(2026101200)
    root = tmp_path/"samples"
    recorder.save(root, schema=semantic_schema_fixture(), producer={"baseline": "test"}, terminal_outcome=None)
    manifest, rows = load_scoped_strategic_samples(root)
    batch, record = rows[0]
    assert manifest["semantic_schema"] == semantic_schema_fixture()
    assert manifest["terminal_outcome"] is None
    assert record["committed"] is True
    assert record["expert_target"] is None
    assert record["label_source"] == "public_baseline"
    np.testing.assert_array_equal(batch["semantic"]["scalar"]["value"],
                                  original["semantic"]["scalar"]["value"])
    assert batch["semantic"]["scalar"]["value"].dtype == np.float32
    assert batch["decision_token_ids"] == original["decision_token_ids"]
    with pytest.raises(FileExistsError):
        recorder.save(root, schema=semantic_schema_fixture(), producer={}, terminal_outcome=None)


def test_uncommitted_sample_retained_without_training_target_and_hash_checked(tmp_path):
    recorder, _ = sample(2026101200, committed=False)
    root = tmp_path/"samples"
    recorder.save(root, schema=semantic_schema_fixture(), producer={}, terminal_outcome=None)
    manifest, rows = load_scoped_strategic_samples(root)
    assert manifest["summary"]["committed_rows"] == 0
    assert rows[0][1]["committed"] is False
    assert rows[0][1]["expert_target"] is None
    path = root/"row-000000.npz"
    path.write_bytes(path.read_bytes()+b"corruption")
    with pytest.raises(StrategicDemonstrationError, match="hash mismatch"):
        load_scoped_strategic_samples(root)


def test_scoped_recorder_rejects_target_leakage_and_misaligned_source():
    recorder, batch = sample(2026101200)
    decision = recorder.rows[0][1]
    with pytest.raises(StrategicDemonstrationError, match="model inputs"):
        recorder(dict(batch, legality_bootstrap_ordinals=[1]), [decision])
    wrong = dict(decision, source_token="other")
    with pytest.raises(StrategicDemonstrationError, match="source token"):
        recorder(batch, [wrong])


def test_all_site_recorder_preserves_symbolic_ordinal_and_excludes_routines(tmp_path):
    original, batch = sample(2026101200, committed=False)
    symbolic = dict(original.rows[0][1], audit=None)
    recorder = ScopedStrategicSampleRecorder(sites=None, include_symbolic=True,
                                            routine_resolver=lambda _: {})
    recorder(batch, [symbolic])
    recorder.commit([symbolic])
    root = tmp_path/"symbolic"
    recorder.save(root, schema=semantic_schema_fixture(), producer={}, terminal_outcome=None)
    manifest, rows = load_scoped_strategic_samples(root)
    assert manifest["schema"] == "sts-scoped-strategic-samples-v3"
    assert manifest["summary"]["sites"] == {"symbolic": 1}
    assert rows[0][1]["committed"]
    assert rows[0][1]["decision"]["ordinal"] == 1
    assert rows[0][1]["decision"]["audit"] is None
    forced = ScopedStrategicSampleRecorder(sites=None, include_symbolic=True,
                                          routine_resolver=lambda _: {0: 1})
    forced(batch, [symbolic])
    forced.commit([symbolic])
    assert not forced.rows
    assert forced.summary()["routine_rows_excluded"] == 1


def test_recorder_capacity_is_checked_before_retaining_another_row():
    original, batch = sample(2026101200, committed=False)
    recorder = ScopedStrategicSampleRecorder(max_rows=1)
    recorder(batch, [original.rows[0][1]])
    with pytest.raises(StrategicDemonstrationError, match="capacity"):
        recorder(batch, [original.rows[0][1]])
    assert len(recorder.rows) == 1


def test_unchosen_observations_have_no_adopted_action_or_baseline_target(tmp_path):
    original, batch = sample(2026101200, committed=False)
    observation = dict(original.rows[0][1], ordinal=None, owner="observation_only")
    recorder = ScopedStrategicSampleRecorder()
    recorder(batch, [observation])
    with pytest.raises(StrategicDemonstrationError, match="cannot be committed"):
        recorder.commit([observation])
    root = tmp_path / "observations"
    recorder.save(root, schema=semantic_schema_fixture(), producer={"baseline": "test"}, terminal_outcome=None)
    manifest, rows = load_scoped_strategic_samples(root)
    assert manifest["schema"] == "sts-scoped-strategic-samples-v2"
    assert manifest["summary"]["observation_only_rows"] == 1
    record = rows[0][1]
    assert record["decision"]["ordinal"] is None
    assert record["decision"]["owner"] == "observation_only"
    assert not record["committed"]
    assert record["label_source"] == "observation_only"
    assert record["expert_target"] is None
    assert "adopted" not in record
    from sts_learning.strategic_behavior_cloning import clone_scoped_baseline_samples, StrategicBehaviorCloningError
    with pytest.raises(StrategicBehaviorCloningError, match="committed"):
        clone_scoped_baseline_samples([root], tmp_path / "clone")
    assert not (tmp_path / "clone").exists()


def test_existing_v1_sources_load_without_rewriting_their_manifest(tmp_path):
    recorder, _ = sample(2026101200)
    root = tmp_path / "legacy"
    recorder.save(root, schema=semantic_schema_fixture(), producer={}, terminal_outcome=None)
    path = root / "manifest.json"
    manifest = json.loads(path.read_text())
    manifest["schema"] = "sts-scoped-strategic-samples-v1"
    for record in manifest["rows"]:
        record["adopted"] = record.pop("decision")
    path.write_text(json.dumps(manifest))
    before = path.read_bytes()
    loaded, rows = load_scoped_strategic_samples(root)
    assert loaded["schema"] == "sts-scoped-strategic-samples-v1"
    assert rows[0][1]["decision"]["ordinal"] == 1
    assert "adopted" not in rows[0][1]
    assert path.read_bytes() == before


@pytest.mark.parametrize("change", [
    lambda record: record.update(committed=True),
    lambda record: record["decision"].update(owner="baseline"),
])
def test_unchosen_observations_cannot_claim_choice_ownership_or_commit(tmp_path, change):
    original, batch = sample(2026101200, committed=False)
    recorder = ScopedStrategicSampleRecorder()
    recorder(batch, [dict(original.rows[0][1], ordinal=None, owner="observation_only")])
    root = tmp_path / "observations"
    recorder.save(root, schema=semantic_schema_fixture(), producer={}, terminal_outcome=None)
    path = root / "manifest.json"
    manifest = json.loads(path.read_text())
    change(manifest["rows"][0])
    path.write_text(json.dumps(manifest))
    with pytest.raises(StrategicDemonstrationError, match="uncommitted v2 observation"):
        load_scoped_strategic_samples(root)


def test_baseline_clone_is_seed_separated_and_uncommitted_rows_are_excluded(tmp_path):
    from sts_learning.strategic_behavior_cloning import clone_scoped_baseline_samples
    roots = []
    for seed in (2026101200, 2026101204):
        recorder, _ = sample(seed)
        pending, _ = sample(seed+1, committed=False)
        recorder.rows.extend(pending.rows)
        root = tmp_path/str(seed)
        recorder.save(root, schema=semantic_schema_fixture(), producer={"baseline": "test"}, terminal_outcome=None)
        roots.append(root)
    output = tmp_path/"clone"
    result = clone_scoped_baseline_samples(roots, output, epochs=4)
    contract = json.loads((output/"contract.json").read_text())
    assert contract["training_seeds"] == [2026101200]
    assert contract["held_out_seeds"] == [2026101204]
    assert contract["excluded_uncommitted_rows"] == 2
    assert result["final_training"]["rows"] == 1
    assert result["final_held_out"]["rows"] == 1
    assert (output/"model.ststorch").is_file()
    with pytest.raises(FileExistsError):
        clone_scoped_baseline_samples(roots, output, epochs=4)
