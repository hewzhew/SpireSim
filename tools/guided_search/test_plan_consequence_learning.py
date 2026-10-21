import copy

import pytest
import torch

from plan_consequence_learning import Dataset, OBJECTIVE, PlanNetwork, metrics, validate_split
from retention_learning import SCHEMA


def doc(seed=1):
    names = ["context"] + [f"hand/{i}/present" for i in range(10)]
    rows = []
    for i, (hp, strength) in enumerate(((-.1, .4), (-.2, 0.))):
        rows.append(dict(sample_sha256=f"{seed}-{i}", source_sha256=f"root-{seed}", endpoint_sha256=f"end-{i}",
            horizon=2, inputs=["EndTurn"], plan=[[1., 0.], [0., 1.]][::1 if i == 0 else -1],
            features=[.5] + [0.] * 10, targets=[hp, strength]))
    return dict(schema=1, feature_schema=SCHEMA, objective=OBJECTIVE, information="full_state_oracle_concrete_plan",
        feature_names=names, token_names=["play", "end"], target_names=["hp", "strength"], target_scales=[75., 10.],
        producer_sha256="frozen", case=dict(label=f"case-{seed}", group=f"seed-{seed}", spec=dict(seed=seed, deck=[])),
        rows=rows, collection=dict(exact_local_replay_ms=1.))


def test_targets_keep_joint_tradeoff_and_do_not_make_per_dimension_maxima():
    data = Dataset([doc()])
    assert data.y.tolist()[0] == pytest.approx([-.1, .4])
    assert data.y.tolist()[1] == pytest.approx([-.2, 0.])
    assert len(data.case_indices[0]) == 2
    result = metrics(data, data.y.clone())
    assert result["same_action_bag_different_order"]["sources"] == 1
    assert result["same_action_bag_different_order"]["centered_mae"]["hp"] == 0.


def test_padded_actions_cannot_change_sequence_or_bag_prediction():
    torch.manual_seed(1)
    data = Dataset([doc()])
    extra = torch.ones(2, 4, 2) * 99
    extended = torch.cat((data.plan, extra), dim=1)
    for mode in ("ordered", "bag"):
        network = PlanNetwork(data.names, 2, 2, mode)
        assert torch.allclose(network.encode_plan(data.plan, data.lengths), network.encode_plan(extended, data.lengths))
        encoded = network.encode_plan(data.plan, data.lengths)
        if mode == "bag":
            assert torch.allclose(encoded[0], encoded[1])
        else:
            assert not torch.allclose(encoded[0], encoded[1])


def test_source_only_control_has_no_access_to_plan_or_outcome():
    data = Dataset([doc()])
    network = PlanNetwork(data.names, 2, 2, "state_only")
    y = network(data.x, data.plan, data.lengths, data.horizon)
    assert torch.allclose(y[0], y[1])
    data.y.fill_(999.)
    assert torch.equal(y, network(data.x, data.plan * 99, data.lengths, data.horizon))


def test_split_and_producer_contracts_prevent_hidden_seed_overlap():
    train, evaluation = Dataset([doc()]), Dataset([doc(2)])
    validate_split(train, evaluation)
    bad = doc(2); bad["case"]["spec"]["seed"] = 1
    with pytest.raises(ValueError, match="source/seed"):
        validate_split(train, Dataset([bad]))
    bad = doc(2); bad["producer_sha256"] = "other"
    with pytest.raises(ValueError, match="contracts differ"):
        validate_split(train, Dataset([bad]))


def test_bad_or_duplicate_plan_evidence_is_rejected():
    original = doc()
    for key, value in (("plan", [[float("nan"), 0.]]), ("horizon", 3), ("targets", [0.]), ("inputs", [])):
        bad = copy.deepcopy(original); bad["rows"][0][key] = value
        with pytest.raises(ValueError):
            Dataset([bad])
    bad = copy.deepcopy(original); bad["rows"][1]["sample_sha256"] = bad["rows"][0]["sample_sha256"]
    with pytest.raises(ValueError, match="provenance"):
        Dataset([bad])
