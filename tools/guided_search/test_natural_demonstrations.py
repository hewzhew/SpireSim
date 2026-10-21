import hashlib
import json

import pytest
import torch

from entity_learning import EntityNetwork, pack
from natural_demonstrations import load


def write_dataset(tmp_path):
    # Constructed file-format fixture, not evidence of a natural run.
    action = {"Combat": {"Input": "EndTurn"}}
    producer = {"contract": "natural_assisted_demo_v1", "entity_schema": 4,
                "labels": "chosen_actions_only", "plan": {"actor": "codex_assisted",
                "information": "oracle_observation_no_search_no_sl",
                "purpose": "demonstration_development", "seeds": [1]}}
    row = {"episode": 0, "before": "a", "after": "b", "selected": 0, "action": action,
           "options": [{"action": action}], "entities": {
               "ids": [[1, 0]], "values": [[0.] * 32], "refs": [[-1] * 3],
               "action_ids": [[1, 0]], "actions": [[0.] * 4], "action_refs": [[-1] * 3]}}
    (tmp_path / "producer.json").write_text(json.dumps(producer))
    (tmp_path / "trajectory.jsonl").write_text(json.dumps(row) + "\n")
    receipt = {"replay_verified": True, "choices": 1, "episodes": [{"episode": 0, "seed": 1, "steps": 1}]}
    for name, suffix in (("producer", ".json"), ("trajectory", ".jsonl")):
        receipt[name + "_sha256"] = hashlib.sha256((tmp_path / (name + suffix)).read_bytes()).hexdigest()
    (tmp_path / "verification.json").write_text(json.dumps(receipt))
    return row


def test_verified_choices_pack_as_numerical_inputs(tmp_path):
    write_dataset(tmp_path)
    examples = load(tmp_path)
    assert len(examples) == 1 and examples[0].seed == 1
    data = pack([e.position for e in examples])
    model = EntityNetwork(hidden=16, layers=1)
    logits, value = model(data)
    assert torch.isfinite(logits[data["mask"]]).all()
    assert torch.isfinite(value).all()
    # Imitation targets carry no invented behavior-policy probability.
    assert not hasattr(examples[0], "behavior_probability")


@pytest.mark.parametrize("failure", ["tampered", "missing", "execution", "unverified"])
def test_invalid_receipts_cannot_supply_training_rows(tmp_path, failure):
    write_dataset(tmp_path)
    if failure == "tampered":
        with (tmp_path / "trajectory.jsonl").open("a") as f:
            f.write("{}\n")
    elif failure == "missing":
        (tmp_path / "verification.json").unlink()
    elif failure == "execution":
        (tmp_path / "execution-failure.json").write_text("{}")
    else:
        p = tmp_path / "verification.json"
        v = json.loads(p.read_text())
        v["replay_verified"] = False
        p.write_text(json.dumps(v))
    with pytest.raises((ValueError, FileNotFoundError)):
        load(tmp_path)


def test_supplied_segment_is_not_natural_imitation_data(tmp_path):
    write_dataset(tmp_path)
    path = tmp_path / "producer.json"
    producer = json.loads(path.read_text())
    producer["contract"] = "supplied_segment_trial_v1"
    producer["labels"] = "none_resource_experiment"
    producer["plan"]["purpose"] = "resource_consequence_development"
    path.write_text(json.dumps(producer))
    receipt_path = tmp_path / "verification.json"
    receipt = json.loads(receipt_path.read_text())
    receipt["producer_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
    receipt_path.write_text(json.dumps(receipt))
    with pytest.raises(ValueError, match="unadmitted"):
        load(tmp_path)
