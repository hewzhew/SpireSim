"""Target semantics tests; these arithmetic cases are not gameplay evidence."""
import copy
import numpy as np
import pytest
import torch

from distance_learning import DistanceDataset, DistanceNetwork, model_revision, from_features, fit, _target_losses, _fact_strata
from entity_learning import pack


@pytest.mark.parametrize("boundary_estimates", [False, True])
def test_forced_probe_boundaries_cross_the_bridge_and_replay_real_inputs(boundary_estimates):
    import json
    from distance_learning import solve
    bridge = pytest.importorskip("sts_learning_bridge")
    spec = dict(seed=73, encounter="JawWorm", act=1, room="MonsterRoom", hp=68,
                deck=[("Bludgeon", 0)], relics=[], potions=[], max_potion_uses=None)
    env = bridge.GuidedCombatBatch.from_specifications([spec], 32)
    evidence = json.loads(env.evidence_json())
    task = dict(source_producer="constructed bridge unit fixture", group="bridge-unit",
                origin=evidence["episodes"][0]["root"], prefix=[],
                requirement=dict(min_final_hp=1, preserve_starting_potion_slots=0))
    for options in ({}, dict(probe_immediate=True)):
        with pytest.raises(ValueError, match="require immediate probes and forced advancement"):
            solve(task, None, horizon=8, probe_forced_boundaries=True, **options)
    with pytest.raises(ValueError, match="boundary estimates require forced probe boundaries"):
        solve(task, None, horizon=8, estimate_probe_boundaries=True)
    report, data = solve(task, None, horizon=8, work=3000, max_nodes=400, max_depth=20,
                         advance_forced=True, probe_immediate=True,
                         probe_forced_boundaries=True, estimate_probe_boundaries=boundary_estimates,
                         export_data=True, include_outcomes=True)
    assert report["limits"]["probe_forced_boundaries"] is True
    assert report["limits"]["estimate_probe_boundaries"] is boundary_estimates
    assert sum(report["stats"][k] for k in
               ("prepared", "transitions", "replay_attempts", "evaluated_states")) <= 3000
    assert 0 < report["stats"]["probe_forced_transitions"] <= report["stats"]["immediate_transitions"]
    assert report["plans"]
    assert len(data["rows"]) <= 400
    assert report["outcome_export_seconds"] >= 0
    evidence = [r["outcome_evidence"] for r in data["rows"]]
    assert all(e["observed_terminal_outcome"] is None for e in evidence if e["combat_terminal"] is None)
    checked = [e for e in evidence if e["terminal_replay_verified"]]
    assert checked and all(e["terminal_satisfies_query"] for e in checked)
    for plan in report["plans"]:
        assert plan["outcome"] in [e["observed_terminal_outcome"] for e in checked]
    for plan in report["plans"]:
        checked = json.loads(bridge.distance_route_data(json.dumps(task), json.dumps(plan["inputs"]),
                                                       json.dumps(plan["successor"]), 8, 10.))
        assert checked["reference_inputs"] == plan["inputs"]
        assert checked["goal"] == task["requirement"]


def features(hp=1.0):
    v=[[0.]*32 for _ in range(2)]
    v[0][0]=hp
    return dict(ids=[[1,0],[16,0]],values=v,refs=[[-1,-1,-1],[0,-1,-1]],
                action_ids=[[2,0]],actions=[[0.]*4],action_refs=[[-1,-1,-1]])


def dataset(complete=True, upper=9, cost=1):
    rows = [dict(features=features(),children=[(1,cost)],complete=complete,lower=0,upper=upper),
            dict(features=features(.5),children=[],complete=False,lower=0,upper=9)]
    return DistanceDataset([dict(group="one-root",data=dict(schema=1,horizon=8,root={},rows=rows))])


def test_partial_children_and_missing_prediction_are_not_failure_labels():
    d=dataset(False)
    targets,kinds=d.targets([2,8])
    assert np.isnan(targets).all()
    assert kinds==["unknown","unknown"]
    d=dataset(True)
    targets,kinds=d.targets([2,np.nan])
    assert np.isnan(targets[0]) and kinds[0]=="unknown"


def test_facts_only_does_not_fit_successor_predictions_or_lose_intervals():
    d=dataset(True)
    targets,kinds=d.targets([2,1],bootstrap=False)
    assert np.isnan(targets).all() and kinds==["unknown","unknown"]
    d.rows[0]["upper"]=5
    targets,kinds=d.targets([2,1],bootstrap=False)
    assert np.isnan(targets[0]) and kinds[0]=="interval_only"
    d.rows[0]["lower"]=5
    targets,kinds=d.targets([2,1],bootstrap=False)
    assert targets[0]==5 and kinds[0]=="fact"


def test_facts_only_fit_never_runs_a_target_network(monkeypatch):
    d=dataset(True,upper=5)
    def forbidden(*args,**kwargs):
        raise AssertionError("facts-only fitting must not generate guessed targets")
    monkeypatch.setattr(d,"values",forbidden)
    h=fit(DistanceNetwork(8),d,updates=1,epochs=None,steps_per_update=2,
          batch_size=2,bootstrap=False)[0]
    assert h["target_revision"] is None and h["bootstrap"] is False
    assert h["target_kinds"]=={"interval_only":1,"unknown":1}
    assert h["optimizer_steps"]==2 and h["fit_examples"]==4
    with pytest.raises(ValueError,match="fitting schedule"):
        fit(DistanceNetwork(8),d,bootstrap="false")


@pytest.mark.parametrize("bootstrap", [True,False])
def test_fit_rejects_mismatched_horizons_before_prediction_or_training(bootstrap,monkeypatch):
    d=dataset(True,upper=5)
    m=DistanceNetwork(9)
    def forbidden(*args,**kwargs):
        raise AssertionError("range validation must precede prediction or training")
    monkeypatch.setattr(d,"values",forbidden)
    monkeypatch.setattr(m,"forward",forbidden)
    with pytest.raises(ValueError,match="model/data range mismatch"):
        fit(m,d,bootstrap=bootstrap)


def test_fact_sampling_preserves_source_and_class_mass_when_occurrences_repeat():
    d=dataset(False,upper=2)
    d.rows[1].update(lower=9,upper=9)
    d.rows.extend([dict(d.rows[1]) for _ in range(3)])
    d.rows.append(dict(d.rows[0]))
    d.item_rows=np.arange(6)
    d.groups=["a"]*5+["b"]
    strata,p=_fact_strata(d,list(range(6)))
    assert strata==[("a","finite_upper")]+[("a","capped")]*4+[("b","finite_upper")]
    assert p==pytest.approx([.25]+[.0625]*4+[.5])
    d.rows[1].update(lower=1,upper=9)
    strata,p=_fact_strata(d,list(range(6)))
    assert sum(p)==pytest.approx(1)
    assert p[1]==pytest.approx(1/6) and strata[1]==("a","lower_only")
    d.rows[1].update(lower=0,upper=9)
    with pytest.raises(ValueError,match="unknown interval"):
        _fact_strata(d,list(range(6)))


def test_source_fact_fit_records_draws_and_rejects_unsupported_schedules():
    d=dataset(False,upper=2)
    d.rows[1].update(lower=9,upper=9)
    h=fit(DistanceNetwork(8),d,updates=1,epochs=None,steps_per_update=2,
          batch_size=4,bootstrap=False,sampling="source_fact")[0]
    assert h["sampling"]=="source_fact" and h["fit_examples"]==8
    assert sum(s["draws"] for s in h["fact_sampling"])==8
    assert all(s["expected_probability"]==.5 for s in h["fact_sampling"])
    for options in ({},dict(bootstrap=False)):
        with pytest.raises(ValueError,match="requires fixed-step"):
            fit(DistanceNetwork(8),d,sampling="source_fact",**options)


def test_source_fact_bootstrap_adds_successor_targets_without_admitting_guesses_alone(monkeypatch):
    rows=[dict(features=features(),children=[(1,1)],complete=True,lower=1,upper=5),
          dict(features=features(.5),children=[(2,0)],complete=True,lower=0,upper=9),
          dict(features=features(.2),children=[],complete=False,lower=0,upper=9)]
    d=DistanceDataset([dict(group="root",data=dict(schema=1,horizon=8,root={},rows=rows))])
    calls=[]
    def values(model,device,batch_size):
        calls.append(model_revision(model))
        return np.array([2.,3.,4.])
    monkeypatch.setattr(d,"values",values)
    h=fit(DistanceNetwork(8),d,updates=1,epochs=None,steps_per_update=2,
          batch_size=4,bootstrap=True,sampling="source_fact")[0]
    assert calls==[h["target_revision"]] and h["bootstrap"] is True
    assert h["target_kinds"]=={"bootstrap":2,"unknown":1}
    assert h["eligible_feature_states"]==1 and h["fit_target_kinds"]=={"bootstrap":1}
    assert h["fact_sampling"]==[dict(group="root",kind="finite_upper",eligible=1,expected_probability=1.,draws=8)]


def test_witnessed_length_only_constrains_upper_and_virtual_edges_cost_zero():
    d=dataset(False,upper=5)
    for value in (2,7,np.nan):
        targets,kinds=d.targets([value,8])
        assert np.isnan(targets[0]) and kinds[0]=="interval_only"
    d=dataset(True,cost=0)
    targets,kinds=d.targets([2,4])
    assert targets[0]==4 and kinds[0]=="bootstrap"
    d=dataset(True,cost=1)
    assert d.targets([2,4])[0][0]==5


def test_interval_loss_uses_current_prediction_and_has_no_internal_preference():
    prediction=torch.tensor([1.,2.,3.,5.,6.,3.,3.],requires_grad=True)
    # Even a stale or absent point estimate cannot anchor an interval example.
    targets=torch.tensor([float('nan'),1.,4.,2.,float('nan'),2.,2.])
    loss=_target_losses(prediction,targets,torch.full((7,),2.),torch.full((7,),5.),
                        torch.tensor([True]*5+[False]*2))
    torch.testing.assert_close(loss,torch.tensor([.5,0.,0.,0.,.5,.5,.5]))
    loss.sum().backward()
    torch.testing.assert_close(prediction.grad,torch.tensor([-1.,0.,0.,0.,1.,1.,1.]))


def test_fit_keeps_interval_only_rows_without_fitting_unknowns():
    torch.manual_seed(0)
    d=dataset(False,upper=5)
    model=DistanceNetwork(8)
    history=fit(model,d,updates=1,epochs=1,batch_size=1)
    assert history[0]["fit_target_kinds"]=={"interval_only":1}
    assert history[0]["target_kinds"]=={"interval_only":1,"unknown":1}
    assert np.isfinite(history[0]["mean_fit_loss"])
    assert history[0]["optimizer_steps"]==history[0]["fit_examples"]==1


def test_fixed_steps_match_optimizer_work_across_data_sizes_and_remain_reproducible():
    torch.manual_seed(0)
    model=DistanceNetwork(8)
    small=dataset(False,upper=5)
    rows=[dict(features=features(i/40),children=[],complete=False,lower=2,upper=5) for i in range(40)]
    # Unconstrained missing evidence remains ineligible even under sampling.
    rows.append(dict(features=features(.2),children=[],complete=False,lower=0,upper=9))
    large=DistanceDataset([dict(group="one-root",data=dict(schema=1,horizon=8,root={},rows=rows))])
    results=[]
    for d in (small,large,large):
        candidate=copy.deepcopy(model)
        history=fit(candidate,d,updates=2,epochs=None,steps_per_update=3,batch_size=4,seed=7)
        for h in history:
            assert h["optimizer_steps"]==3 and h["fit_examples"]==12
            assert h["fit_unique_states"]<=min(12,h["eligible_feature_states"])
            assert h["fit_target_kinds"]=={"interval_only":h["fit_unique_states"]}
        results.append((model_revision(candidate),history))
    assert results[1]==results[2]
    assert results[1][1][0]["eligible_feature_states"]==40


@pytest.mark.parametrize("schedule",[
    {"epochs":None}, {"steps_per_update":2}, {"epochs":None,"steps_per_update":0},
    {"epochs":None,"steps_per_update":True}, {"epochs":1.5}, {"updates":False},
])
def test_fit_rejects_ambiguous_or_invalid_schedules(schedule):
    with pytest.raises(ValueError,match="fitting schedule"):
        fit(DistanceNetwork(8),dataset(),**schedule)


def test_terminal_facts_override_predictions_without_contaminating_unknowns():
    d=dataset()
    d.rows[1]["lower"]=d.rows[1]["upper"]=9
    assert d.targets([2,1])[0][0]==9
    d.rows[1]["lower"]=d.rows[1]["upper"]=0
    assert d.targets([2,8])[0][0]==1
    with pytest.raises(ValueError): d.targets([2,np.inf])
    with pytest.raises(ValueError): d.targets([2,10])


def test_resource_entities_and_state_features_affect_distance_with_no_actor():
    torch.manual_seed(0)
    model=DistanceNetwork(8).eval()
    x=pack([from_features(features()),from_features(features(.1))],kind_count=18)
    prediction=model(x)
    assert prediction.shape==(2,)
    assert (prediction>0).all() and (prediction<9).all()
    assert prediction[0]!=prediction[1]
    assert not hasattr(model,"policy")
    revision=model_revision(model)
    assert model_revision(copy.deepcopy(model))==revision
    with torch.no_grad(): model.value.bias.add_(.1)
    assert model_revision(model)!=revision


def test_invalid_fact_intervals_and_foreign_edges_are_rejected():
    d=dataset()
    for edit in (dict(lower=8,upper=7),dict(children=[(0,1)]),dict(children=[(1,2)])):
        rows=copy.deepcopy(d.rows)
        rows[0].update(edit)
        with pytest.raises(ValueError):
            DistanceDataset([dict(group="one-root",data=dict(schema=1,horizon=8,root={},rows=rows))])
