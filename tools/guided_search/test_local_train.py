import copy
import gzip
import json
import time

import numpy as np
import pytest
torch = pytest.importorskip("torch")
bridge = pytest.importorskip("sts_learning_bridge")
if not hasattr(bridge, "GuidedCombatBatch"):
    pytest.skip("rebuild the optional bridge for local starter tests", allow_module_level=True)
GuidedCombatBatch = bridge.GuidedCombatBatch
from tools.guided_search.local_train import Config, StarterNetwork, prepare, rollout, update
from tools.guided_search.local_compare import search, repeated_policy
from tools.guided_search.local_train import run as run_training
from tools.guided_search.local_compare import run as run_comparison
from tools.guided_search.local_compare import verify_direct_regression
from tools.guided_search.local_compare import EvaluationPanel


def test_numerical_arrays_bind_all_instances_and_exclude_control_identity():
    env = GuidedCombatBatch([(17, "JawWorm"), (29, "Cultist")], 128)
    frame = env.decision_batch()
    assert frame["context"].shape == (2, 48)
    assert frame["cards"].shape == (2, 11, 20)
    assert frame["context"].dtype == np.float32
    data = prepare(frame)
    changed = copy.deepcopy(frame)
    changed["revision"] += 999
    changed["slots"][:] = 999
    for key, value in prepare(changed).items():
        torch.testing.assert_close(value, data[key])
    before = env.evidence_json()
    with pytest.raises(ValueError):
        env.advance(frame["revision"], [0, 999])
    assert env.evidence_json() == before
    env.advance(frame["revision"], [0, 0])
    with pytest.raises(ValueError):
        env.advance(frame["revision"], [0, 0])
    assert frame["revision"] != env.decision_batch()["revision"]


def test_unknown_episode_retains_evidence_and_cannot_update(tmp_path):
    config = Config(max_decisions=1, hidden=16, layers=1)
    model = StarterNetwork(config).eval()
    before = copy.deepcopy(model.state_dict())
    with pytest.raises(RuntimeError, match="incomplete combat cohort"):
        rollout(model, [(17, "JawWorm")], config, torch.Generator().manual_seed(7),
                tmp_path / "cut", time.monotonic() + 30)
    result = json.loads((tmp_path / "cut/outcomes.json").read_text())
    assert result["outcomes"][0]["kind"].startswith("unknown")
    assert (tmp_path / "cut/evidence.json.gz").is_file()
    for key, value in model.state_dict().items():
        torch.testing.assert_close(value, before[key])


def test_complete_fresh_rollouts_train_on_real_slot_aligned_returns(tmp_path):
    torch.set_num_threads(1)
    torch.manual_seed(117)
    config = Config(hidden=16, layers=1)
    model = StarterNetwork(config).eval()
    cases = [(17, "JawWorm"), (29, "Cultist")]
    records, outcomes, _ = rollout(model, cases, config, torch.Generator().manual_seed(51),
                                   tmp_path / "complete", time.monotonic() + 30)
    assert {o["slot"] for o in outcomes} == {0, 1}
    assert all(o["kind"] in ("win", "loss") and o["turn"] > 1 for o in outcomes)
    before = copy.deepcopy(model.state_dict())
    metrics = update(model, torch.optim.Adam(model.parameters(), lr=.001), records, outcomes[::-1], config)
    assert metrics["rows"] == sum(len(r["slots"]) for r in records)
    assert any(not torch.equal(value, before[key]) for key, value in model.state_dict().items())
    assert (tmp_path / "complete/experience.npz").is_file()
    with pytest.raises(ValueError, match="behavior probabilities changed"):
        update(model, torch.optim.Adam(model.parameters()), records, outcomes, config)


def test_seed_groups_must_be_disjoint_and_model_initializations_explicit():
    with pytest.raises(ValueError, match="overlap"):
        Config(train_seed_start=17, eval_seed_start=17).validate()
    with pytest.raises(ValueError, match="distinct"):
        Config(model_seeds=(1, 1)).validate()
    with pytest.raises(ValueError, match="nonempty"):
        Config(model_seeds=()).validate()


def test_invalid_numeric_domains_are_rejected_without_truncation():
    frame = GuidedCombatBatch([(17, "JawWorm")], 128).decision_batch()
    frame["action_cards"][0] = 11
    with pytest.raises(ValueError, match="invalid action domain"):
        prepare(frame)


def test_search_binding_preserves_low_hp_and_budget_unknown(tmp_path):
    config = Config(starting_hp=24, hidden=16, layers=1)
    env = GuidedCombatBatch([(17, "JawWorm")], 128, starting_hp=24, search_services=1)
    assert env.decision_batch()["context"][0, 0] == np.float32(24 / 75)
    model = StarterNetwork(config).eval()
    before = copy.deepcopy(model.state_dict())
    outcomes, timing = search(model, [(17, "JawWorm")], config, tmp_path / "search",
                              time.monotonic() + 30, 1, "policy_value")
    assert outcomes[0]["kind"].startswith("unknown")
    assert outcomes[0]["services"] == 1
    assert timing["cohort_wall"] > 0
    for key, value in model.state_dict().items():
        torch.testing.assert_close(value, before[key])
    with np.load(tmp_path / "search/guidance.npz") as guidance:
        assert guidance["slots"].tolist() == [0]
        assert guidance["values"].shape == (1,)


def test_repeated_defeats_are_failed_attempts_not_a_proven_root_loss(tmp_path):
    class EndTurn(torch.nn.Module):
        def forward(self, data):
            logits = torch.full(data["mask"].shape, -torch.inf)
            logits[(data["indices"] == 0) & data["mask"]] = 0
            return logits, torch.zeros(len(logits))
    results, _ = repeated_policy(EndTurn(), [(17, "JawWorm")], Config(starting_hp=1),
                                 tmp_path / "repeat", time.monotonic() + 30, 2, 17)
    assert results[0]["kind"] == "unknown_attempt_limit"
    assert results[0]["attempts"] == 2 and results[0]["services"] == 2
    assert results[0]["hp"] <= 0


def test_completed_study_loads_frozen_weights_and_requires_same_simulator(tmp_path):
    config = Config(train_seed_start=4333, train_seed_count=1, eval_seed_start=7555,
                    eval_seed_count=1, model_seeds=(7,), generations=1,
                    hidden=16, layers=1, starting_hp=24, evaluation_partition="development")
    training = tmp_path / "training"
    run_training(config, training)
    weights = (training / "model-7/model-1.npz").read_bytes()
    run_comparison(training, tmp_path / "comparison", budgets=(1,), repeats=1, max_wall_seconds=30)
    result = json.loads((tmp_path / "comparison/result.json").read_text())
    assert result["status"] == "complete" and len(result["evaluation"]) == 5
    assert (training / "model-7/model-1.npz").read_bytes() == weights
    for row in result["evaluation"]:
        assert row["episodes"] == row["wins"] + row["losses"] + row["unknown"] == 2
    run_comparison(training, tmp_path / "regression", budgets=(1,), repeats=1,
                   rollout_steps=(128,), regression_baseline=tmp_path / "comparison")
    check = json.loads((tmp_path / "regression/regression.json").read_text())
    assert check[0]["episodes"] == 2
    assert len(json.loads((tmp_path / "regression/result.json").read_text())["evaluation"]) == 7
    run_comparison(training, tmp_path / "final", budgets=(1,), repeats=1,
                   rollout_steps=(128,), regression_baseline=tmp_path / "comparison",
                   evaluation_panel=EvaluationPanel(8444, 2, "final"))
    final_contract = json.loads((tmp_path / "final/contract.json").read_text())
    assert final_contract["partition"] == "final"
    assert {c[0] for c in final_contract["cases"]} == {8444, 8445}
    assert {c[0] for c in final_contract["regression_cases"]} == {7555}
    for row in json.loads((tmp_path / "final/result.json").read_text())["evaluation"]:
        assert row["episodes"] == 4
        assert {o["seed"] for o in row["outcomes"]} == {8444, 8445}
    # A real successor change must stop cross-binary comparison before search.
    evidence_path = tmp_path / "comparison/model-7/greedy/evidence.json.gz"
    with gzip.open(evidence_path, "rt", encoding="utf-8") as f:
        evidence = json.load(f)
    evidence["episodes"][0]["successor"] = None
    with gzip.open(evidence_path, "wt", encoding="utf-8") as f:
        json.dump(evidence, f)
    with pytest.raises(ValueError, match="trajectory differs"):
        verify_direct_regression(tmp_path / "regression/model-7/greedy", evidence_path.parent)
    with pytest.raises(ValueError, match="trajectory differs"):
        run_comparison(training, tmp_path / "stopped", budgets=(1,), repeats=1,
                       regression_baseline=tmp_path / "comparison",
                       evaluation_panel=EvaluationPanel(8444, 2, "final"))
    assert (tmp_path / "stopped/failure.json").is_file()
    assert not (tmp_path / "stopped/neutral-1").exists()
    assert not (tmp_path / "stopped/model-7/greedy").exists()
    manifest = json.loads((training / "manifest.json").read_text())
    manifest["binary_sha256"] = {}
    (training / "manifest.json").write_text(json.dumps(manifest))
    with pytest.raises(ValueError, match="same native binary"):
        run_comparison(training, tmp_path / "mismatch", budgets=(1,), repeats=1)
    assert not (tmp_path / "mismatch").exists()


def test_new_seed_panel_rejects_source_overlap_and_invalid_domains():
    config = Config()
    for start in (config.train_seed_start, config.eval_seed_start):
        with pytest.raises(ValueError, match="overlaps"):
            EvaluationPanel(start, 2, "final").validate(config)
    for panel in (EvaluationPanel(-1, 2, "final"), EvaluationPanel(2**64-1, 2, "final"),
                  EvaluationPanel(9000, 65, "final"), EvaluationPanel(9000, 0, "final"),
                  EvaluationPanel(True, 2, "final"), EvaluationPanel(9000, 2, "unknown")):
        with pytest.raises(ValueError, match="invalid evaluation"):
            panel.validate(config)
    EvaluationPanel(3000000, 32, "final").validate(config)
