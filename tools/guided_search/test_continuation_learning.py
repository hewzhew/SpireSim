import copy

import pytest
import torch

from continuation_learning import Dataset, OBJECTIVE, SEARCH_OBJECTIVE, WITNESS_OBJECTIVE, metrics, validate_split
from retention_learning import SCHEMA


def document(group="seed-1"):
    return dict(schema=1, feature_schema=SCHEMA, objective=OBJECTIVE,
        feature_names=["terminal_hp_ceiling", "signal"],
        continuation=dict(policy="RetainedCombatTacticalPolicyV1", selection="argmax_first_tie",
                          executable_sha256="frozen-producer", max_inputs=256),
        case=dict(label=group, group=group, spec=dict(seed=int(group.rsplit("-", 1)[-1]))),
        traces=[dict(inputs=["EndTurn"], terminal_return=value) for value in (-1, .8, None)],
        rows=[dict(features=[1., 0.], trace=i, offset=0) for i in range(3)])


def test_actual_loss_is_a_policy_return_and_censored_is_not_a_loss():
    data = Dataset([document()])
    assert data.y.tolist() == pytest.approx([-1., .8])
    assert data.censored == 1
    # Equal encodings retain different executed returns, with no best-of merge.
    assert data.x[0].tolist() == data.x[1].tolist()


def test_aggregate_error_cannot_hide_pending_decision_errors():
    doc = document()
    doc["feature_names"][1] = "pending"
    doc["rows"][0]["features"][1] = 1.
    doc["rows"][2]["features"][1] = 1.  # Censored rows supply no evaluation target.
    data = Dataset([doc])
    result = metrics(data, torch.tensor([0., .8]))
    assert result["case_mean_mae"] == pytest.approx(.5)
    assert result["decision_phases"]["pending"] == dict(
        rows=1, cases=1, winning_rows=0, case_mean_mae=1.)
    assert result["decision_phases"]["player_turn"]["case_mean_mae"] == pytest.approx(0.)


def test_first_three_public_turns_exclude_the_fourth_zero_based_counter():
    doc = document()
    doc["feature_names"][1] = "turn"
    doc["traces"] = [dict(inputs=["EndTurn"],terminal_return=0.) for _ in range(4)]
    doc["rows"] = [dict(features=[1.,i/20.],trace=i,offset=0) for i in range(4)]
    result = metrics(Dataset([doc]),torch.tensor([0.,0.,0.,1.]))
    assert result["case_mean_mae"] == pytest.approx(.25)
    assert result["first_three_turns_case_mae"] == pytest.approx(0.)


def test_split_keeps_seed_interventions_together_and_frozen_actor_identical():
    train = Dataset([document()])
    with pytest.raises(ValueError, match="source/seed"):
        validate_split(train, Dataset([document()]))
    held_out = document("seed-2")
    validate_split(train, Dataset([held_out]))
    disguised = copy.deepcopy(held_out)
    disguised["case"]["spec"]["seed"] = 1
    with pytest.raises(ValueError, match="source/seed"):
        validate_split(train, Dataset([disguised]))
    held_out["continuation"]["executable_sha256"] = "different-producer"
    with pytest.raises(ValueError, match="contracts differ"):
        validate_split(train, Dataset([held_out]))


def test_iterations_cannot_silently_mix_different_teacher_weights():
    first, next_round = document(), document("seed-2")
    for doc, digest in ((first, "teacher-a"), (next_round, "teacher-b")):
        doc["continuation"]["solver_config"] = dict(resource_model=dict(sha256=digest))
    with pytest.raises(ValueError, match="inconsistent continuation contract"):
        Dataset([first, next_round])
    with pytest.raises(ValueError, match="contracts differ"):
        validate_split(Dataset([first]), Dataset([next_round]))


def test_bad_return_or_row_cannot_silently_enter_training():
    original = document()
    for trace in (-.5, 1.1, float("nan")):
        bad = copy.deepcopy(original)
        bad["traces"][0]["terminal_return"] = trace
        with pytest.raises(ValueError, match="terminal return"):
            Dataset([bad])
    bad = copy.deepcopy(original)
    bad["rows"][0]["offset"] = 2
    with pytest.raises(ValueError, match="provenance"):
        Dataset([bad])
    bad = copy.deepcopy(original)
    bad["rows"][1]["features"][0] = .5
    with pytest.raises(ValueError, match="ceiling"):
        Dataset([bad])


def test_search_targets_require_a_fresh_invocation_at_each_row():
    data = document()
    data["objective"] = SEARCH_OBJECTIVE
    data["continuation"]["policy"] = "CombatSolverPlusTacticalControlV1"
    for trace in data["traces"]:
        trace["tactical_control"] = dict(terminal_return=trace["terminal_return"])
        trace["search"] = dict(stop="WorkBudget")
    Dataset([data])
    data["rows"][0]["offset"] = 1
    data["traces"][0]["inputs"].append("EndTurn")
    with pytest.raises(ValueError, match="independent source"):
        Dataset([data])


def test_search_miss_needs_an_executed_control_and_cannot_invent_a_loss():
    data = document()
    data["objective"] = SEARCH_OBJECTIVE
    with pytest.raises(ValueError, match="search/control evidence"):
        Dataset([data])
    for trace in data["traces"]:
        trace["tactical_control"] = dict(terminal_return=trace["terminal_return"])
        trace["search"] = dict(stop="WorkBudget")
    data["traces"][1]["terminal_return"] = -1.
    with pytest.raises(ValueError, match="worse than its executed control"):
        Dataset([data])


def test_replayed_winning_control_cannot_be_labeled_as_death_or_silently_dropped():
    data=document()
    data["objective"]=WITNESS_OBJECTIVE
    data["traces"]=data["traces"][1:2]
    data["rows"]=data["rows"][1:2]
    data["rows"][0]["trace"]=0
    trace=data["traces"][0]
    trace["tactical_control"]=dict(terminal_return=-1.)
    trace["search"]=dict(stop="WorkBudget",selected_witness_control=True,selected_solver_plan=False)
    trace["witness_control"]=dict(source_prefix=[],inputs=["EndTurn"],terminal_return=.8)
    assert Dataset([data]).y.tolist()==pytest.approx([.8])
    trace["terminal_return"]=-1.
    with pytest.raises(ValueError,match="worse than its replayed winning control"):
        Dataset([data])
    trace["terminal_return"]=.8
    trace["witness_control"]=None
    with pytest.raises(ValueError,match="selection evidence"):
        Dataset([data])
