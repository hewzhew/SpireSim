import numpy as np
import pytest
import torch
from retention_learning import (Dataset, RetentionNetwork, SCHEMA, OBJECTIVE, RESPONSE_OBJECTIVE,
                                fit, mechanism_columns, response_losses, training_targets)


def dataset():
    rows = [
        dict(features=[0.], terminal=None, observed_return=-1., children=[1, 2]),
        dict(features=[1.], terminal=-1., observed_return=-1., children=None),
        dict(features=[2.], terminal=None, observed_return=None, children=[3]),
        dict(features=[3.], terminal=.8, observed_return=.8, children=None),
        dict(features=[4.], terminal=None, observed_return=None, children=None),
    ]
    return Dataset([dict(case=dict(label="one-source"), data=dict(schema=SCHEMA, objective=OBJECTIVE, feature_names=["hp"], rows=rows))])


def test_frozen_backups_use_alternatives_without_labeling_unfinished_as_loss():
    data = dataset()
    initial = data.targets()
    assert initial[0] == pytest.approx(.8) and initial[2] == pytest.approx(.8)
    assert np.isnan(initial[4])
    targets = data.targets([0., 1., .6, -1., .7])
    assert targets[0] == pytest.approx(.8)  # witnessed route beats the frozen prediction
    assert targets[1] == -1.  # real terminal overrides optimistic prediction
    assert targets[2] == pytest.approx(.8)
    assert np.isnan(targets[4])


def test_witnessed_backups_use_partial_traces_cycles_and_exact_source_indices():
    rows = [
        dict(features=[0.], terminal=None, observed_return=-1., children=None),
        dict(features=[1.], terminal=None, observed_return=None, children=None, origin=[0, "real-branch"]),
        dict(features=[2.], terminal=None, observed_return=-1., children=None),
        dict(features=[3.], terminal=.8, observed_return=.8, children=None),
        dict(features=[0.], terminal=None, observed_return=None, children=[5]),
        dict(features=[0.], terminal=None, observed_return=None, children=[4]),
    ]
    data = dict(schema=SCHEMA, objective=OBJECTIVE, feature_names=["hp"], rows=rows,
                traces=[dict(rows=[1, 2, 1, 2, 3])])
    other = dict(schema=SCHEMA, objective=OBJECTIVE, feature_names=["hp"],
                 rows=[dict(features=[0.], terminal=None, observed_return=None, children=None)])
    dataset = Dataset([dict(case=dict(label="one"), data=data), dict(case=dict(label="two"), data=other)])
    assert dataset.lower[:4] == pytest.approx([.8] * 4)
    assert np.isnan(dataset.targets()[:3]).all()  # partial edges do not prove optimality
    assert dataset.targets()[3] == pytest.approx(.8)
    assert np.isnan(dataset.targets()[4:]).all()  # no feature merge, cycle or source leakage
    assert dataset.backup_stats["loss_to_win"] == 2
    assert dataset.backup_stats["newly_connected"] == 1
    data["traces"][0]["rows"].append(0)
    with pytest.raises(ValueError, match="terminal"):
        Dataset([dict(case=dict(label="bad"), data=data)])


def test_bad_rollout_is_not_bad_state_and_complete_action_sets_tighten_bounds():
    rows = [
        dict(features=[1.], terminal=None, observed_return=-1., children=[1, 2]),
        dict(features=[1.], terminal=-1., observed_return=-1., children=None),
        dict(features=[.6], terminal=None, observed_return=None, children=None),
        dict(features=[1.], terminal=None, observed_return=-1., children=[1]),
    ]
    document = dict(case=dict(label="bounds"), data=dict(schema=SCHEMA, objective=OBJECTIVE,
        feature_names=["terminal_hp_ceiling"], rows=rows))
    data = Dataset([document])
    assert data.upper == pytest.approx([.6, -1., .6, -1.])
    assert np.isnan(data.targets()[0]) and data.targets()[3] == -1.
    assert data.targets([1., 1., 1., 1.])[0] == pytest.approx(.6)
    prediction = torch.tensor([.4, .4, .4, .4], requires_grad=True)
    point = torch.from_numpy(data.targets())
    target = training_targets(prediction, point, torch.from_numpy(data.lower), torch.from_numpy(data.upper))
    torch.nn.functional.smooth_l1_loss(prediction, target, reduction="sum").backward()
    assert prediction.grad.tolist() == pytest.approx([0., 1., 0., 1.])
    rows[0]["observed_return"] = .8
    with pytest.raises(ValueError, match="exceeds"):
        Dataset([document])


def test_supervision_summary_separates_witnesses_from_point_targets():
    data = Dataset([dict(case=dict(label="response"), data=response_data())])
    stats = data.supervision_summary()
    decisions = stats["player_decisions"]
    assert decisions["rows"] == 6
    assert decisions["grounded_point_rows"] == 1
    assert decisions["frozen_backup_point_rows"] == 0
    assert decisions["interval_only_rows"] == 5
    # The root has a completed losing route, but alternatives remain open.
    assert decisions["loss_only_continuation_rows"] == 2
    assert decisions["no_completed_continuation_rows"] == 4
    assert decisions["complete_action_rows"] == 4
    assert stats["all"]["grounded_point_rows"] == 2
    assert "player_decisions" not in dataset().supervision_summary()


def test_supervision_metrics_use_frozen_targets_and_current_estimates_separately():
    data = Dataset([dict(case=dict(label="response"), data=response_data())])
    frozen = np.array([.1, -1., .4, .3, .2, -1., .1], dtype=np.float32)
    fitted = np.array([.4, -.2, -.8, 0., .9, -1., .2], dtype=np.float32)
    stats = data.supervision_summary(frozen, estimates=fitted)["player_decisions"]
    assert stats["grounded_point_rows"] == 1
    assert stats["frozen_backup_point_rows"] == 3
    assert stats["interval_only_rows"] == 2
    assert stats["grounded_point_mae"] == pytest.approx(.2)
    assert stats["frozen_backup_point_mae"] == pytest.approx(.2)
    assert stats["active_interval_constraints"] == 1
    assert stats["inactive_interval_constraints"] == 1
    assert data.supervision_summary(estimates=fitted)["player_decisions"]["frozen_backup_point_rows"] == 0
    assert np.isnan(data.targets()[0])  # reading metrics creates no target
    with pytest.raises(ValueError, match="finite and match rows"):
        data.supervision_summary([float("nan")] * len(data.x))
    with pytest.raises(ValueError, match="finite and match rows"):
        data.supervision_summary(estimates=[0.])


def test_mechanism_ablation_survives_export_and_bad_edges_fail():
    names = ["hp", "guardian/open", "enemy/SharpHide", "zone/0/numbers/3"]
    model = RetentionNetwork(len(names), 2)
    masked = mechanism_columns(names)
    exported = model.export(names, masked)
    assert torch.tensor(exported["hidden_weights"])[:, masked].count_nonzero() == 0
    document = dict(case=dict(label="bad"), data=dict(schema=SCHEMA, objective=OBJECTIVE,
        feature_names=["hp"], rows=[dict(features=[0.], terminal=None, observed_return=None, children=[1])]))
    with pytest.raises(ValueError):
        Dataset([document])


def response_data():
    def row(turn, ceiling, children=None, terminal=None):
        return dict(features=[turn, 0., 0., ceiling], terminal=terminal,
                    observed_return=terminal, children=children)

    return dict(schema=SCHEMA, objective=OBJECTIVE,
        feature_names=["turn", "pending", "assembling", "terminal_hp_ceiling"],
        rows=[row(1, 1., [1, 2, 3]), row(1, 1., [4]), row(1, 1., [5]),
              row(2, .6, [6]), row(2, .8), row(1, 0., terminal=-1.), row(3, .5)])


def test_response_targets_keep_turn_boundaries_and_terminal_healing_distinct():
    data = response_data()
    targets = response_losses(data)
    assert targets[:4] == pytest.approx([.2, .2, 2., .1])
    assert np.isnan(targets[4:]).all()
    # The terminal feature ceiling would add BurningBlood again; use the actual
    # terminal result, not a second heal. This is resource loss, not raw damage.
    data["rows"][5].update(features=[1, 1., 0., .98], terminal=.9, observed_return=.9)
    targets = response_losses(data)
    assert targets[0] == pytest.approx(.1) and targets[2] == pytest.approx(.1)


def test_incomplete_or_closed_response_graphs_do_not_supply_exact_targets():
    data = response_data()
    data["rows"][1]["children"] = None
    targets = response_losses(data)
    assert np.isnan(targets[:2]).all() and targets[2] == 2.
    data["rows"][1]["children"] = [1]
    targets = response_losses(data)
    assert np.isnan(targets[1])  # a closed graph is not proof of death
    assert targets[0] == pytest.approx(.4)  # best reachable terminating response
    # New-turn forced choices must finish before this is a response boundary.
    data["rows"][3]["features"][1] = 1.
    data["rows"][3]["children"] = None
    assert np.isnan(response_losses(data)[0])
    data["rows"][3]["children"] = [6]
    data["rows"][6]["features"][0] = 2.
    assert response_losses(data)[0] == pytest.approx(.5)


def test_response_targets_use_case_local_identity_and_reject_impossible_gain():
    import copy
    first = response_data()
    second = copy.deepcopy(first)
    second["rows"][4]["features"][3] = .7
    data = Dataset([dict(case=dict(label="one"), data=first), dict(case=dict(label="two"), data=second)])
    assert data.response_losses[[0, 7]] == pytest.approx([.2, .3])
    first["rows"][4]["features"][3] = 1.1
    with pytest.raises(ValueError, match="physical resource ceiling"):
        response_losses(first)


def test_auxiliary_training_preserves_deployed_format_and_initialization():
    torch.manual_seed(1)
    plain = RetentionNetwork(4, 2).export(response_data()["feature_names"])
    torch.manual_seed(1)
    auxiliary = RetentionNetwork(4, 2, response_head=True).export(response_data()["feature_names"])
    assert auxiliary == plain
    data = Dataset([dict(case=dict(label="one"), data=response_data())])
    model, _, history, head = fit(data, steps=2, backup_rounds=1, response_weight=1.)
    assert model.keys() == plain.keys() and model["objective"] == OBJECTIVE
    assert head["objective"] == RESPONSE_OBJECTIVE
    assert head["hidden_weights"] == model["hidden_weights"]
    assert np.isfinite(history[-1]["response_mae"])
    assert history[0]["supervision"]["player_decisions"]["frozen_backup_point_rows"] == 0
    assert history[1]["supervision"]["player_decisions"]["frozen_backup_point_rows"] == 3
    with pytest.raises(ValueError, match="no complete response"):
        fit(dataset(), steps=1, response_weight=1.)


def test_separate_response_data_does_not_need_to_replace_terminal_graph():
    terminal = response_data()
    for row in terminal["rows"]:
        row["children"] = None
    data = Dataset([dict(case=dict(label="terminal"), data=terminal)])
    auxiliary = Dataset([dict(case=dict(label="auxiliary"), data=response_data())])
    assert not np.isfinite(data.response_losses).any()
    model, _, history, _ = fit(data, steps=2, backup_rounds=0,
        response_weight=1., response_data=auxiliary)
    assert model["objective"] == OBJECTIVE and np.isfinite(history[0]["response_mae"])
    assert data.children == [None] * len(data.x)
    with pytest.raises(ValueError, match="feature schemas"):
        fit(data, response_weight=1., response_data=dataset())
    with pytest.raises(ValueError, match="positive response weight"):
        fit(data, response_data=auxiliary)


def test_shared_hand_encoder_preserves_padding_and_exported_masks():
    names = ['hp'] + [f'hand/{i}/{n}' for i in range(10)
                      for n in ('present', 'damage_per_hit', 'mode_remaining')]
    model = RetentionNetwork(len(names), hidden=1, hand_layout=(1, 3, 1))
    with torch.no_grad():
        model.cards.weight[:] = torch.tensor([[0., 1., 2.]])
        model.cards.bias[:] = .1
        model.hidden.weight[:] = torch.tensor([[0., 1., 1.]])
        model.hidden.bias[:] = 0
        model.output.weight[:] = 1
        model.output.bias[:] = 0
    x = torch.zeros(1, len(names))
    x[0, 1:7] = torch.tensor([1., .2, .1, 1., .4, .1])
    masked = mechanism_columns(names)
    x[:, masked] = 0
    assert model(x).item() == pytest.approx(.58)
    exported = model.export(names, masked)
    encoder = exported['hand_encoder']
    assert encoder['weights'] == [[0., 1., 0.]]
    assert encoder['start'] == 1 and encoder['width'] == 3
    assert len(exported['hidden_weights'][0]) == 3
    with pytest.raises(ValueError, match='preserve presence'):
        model.export(names, [1])
    with pytest.raises(ValueError, match='instance features'):
        fit(dataset(), hand_hidden=16)
