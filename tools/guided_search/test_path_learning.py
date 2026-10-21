import copy
import numpy as np
import pytest

from path_learning import FEATURE_COUNT, FEATURE_SCHEMA, PreferenceDataset, RouteDataset, fit, segment_log_softmax


def route(root, key, rows, selected=0, hp=20):
    return {"schema":1,"root_id":root,"family":root,"route_id":key,
        "goal":{"min_final_hp":hp,"preserve_starting_potion_slots":0},
        "decisions":[{"features":{"schema":FEATURE_SCHEMA,"groups":[0,0,1],"rows":rows},"selected":selected}]}


def examples():
    a = route("a","a1",[[(0,1.)],[(0,-.5)],[(1,2.)]])
    b = route("b","b1",[[(1,.4)],[(0,2.)],[(1,1.)]],selected=2)
    b["decisions"] *= 2
    other = route("a","a2",[[(0,3.)],[(1,2.)],[(1,-1.)]],selected=1)
    q = copy.deepcopy(a); q["goal"]["min_final_hp"] = 30
    return [a,other,q,b]


def test_whole_route_gradient_matches_finite_differences():
    data = RouteDataset(examples())
    w = np.zeros((2,FEATURE_COUNT)); w[:, :2] = [[.17,-.3],[.25,.14]]
    loss, gradient, logs, active = data.evaluate(w)
    for head in range(2):
        for feature in range(2):
            plus,minus = w.copy(),w.copy()
            plus[head,feature] += 1e-5; minus[head,feature] -= 1e-5
            numeric = (data.evaluate(plus)[0]-data.evaluate(minus)[0])/2e-5
            assert gradient[head,feature] == pytest.approx(numeric,abs=2e-8)
    # Two roots; a has two queries, b one. Min alternatives is inside query.
    bounds = np.exp(data.log_cost-logs)
    expected = ((min(bounds[0],bounds[1])+bounds[2])/2 + bounds[3])/2
    assert loss == pytest.approx(np.log(expected))


def test_duplicates_do_not_reweight_roots_queries_or_routes():
    records = examples(); w=np.zeros((2,FEATURE_COUNT))
    a = RouteDataset(records); b = RouteDataset(records+[copy.deepcopy(records[0])]*4)
    assert len(a.records)==len(b.records)
    assert a.evaluate(w)[0] == b.evaluate(w)[0]
    np.testing.assert_array_equal(a.evaluate(w)[1],b.evaluate(w)[1])


def test_complete_cost_includes_forced_steps_and_not_step_average():
    record=examples()[0]
    record["decisions"].append({"features":{"schema":FEATURE_SCHEMA,"groups":[0],"rows":[[]]},"selected":0})
    d=RouteDataset([record]); loss,_,logs,_=d.evaluate(np.zeros((2,FEATURE_COUNT)))
    assert logs[0]==pytest.approx(np.log(.25))
    assert loss==pytest.approx(np.log(2/.25))


def test_training_changes_preferences_and_keeps_small_probabilities_finite():
    data=RouteDataset(examples()); weights,history=fit(data,30,.03)
    assert data.evaluate(weights)[0] < history[0]["log_mean_route_bound"]
    logs=segment_log_softmax(np.array([1000.,0.]),np.array([0,0]))
    np.testing.assert_array_equal(logs,[0.,-1000.])


def test_invalid_or_missing_data_is_not_silently_used():
    with pytest.raises(ValueError): RouteDataset([])
    records=examples(); bad=copy.deepcopy(records[0]); bad["decisions"][0]["selected"]=99
    with pytest.raises(ValueError): RouteDataset([bad])
    bad=copy.deepcopy(records[0]); bad["decisions"][0]["features"]["rows"][0]=[[0,np.nan]]
    with pytest.raises(ValueError): RouteDataset([bad])
    bad=copy.deepcopy(records[0]); bad["family"]="another-seed"
    with pytest.raises(ValueError): RouteDataset(records+[bad])


def preference_examples():
    a, other, q, b = examples()
    a2 = copy.deepcopy(a); a2["route_id"] = "a2"; a2["decisions"][0]["selected"] = 2
    q2 = copy.deepcopy(q); q2["route_id"] = "a2"; q2["decisions"][0]["selected"] = 1
    b2 = copy.deepcopy(b); b2["route_id"] = "b2"; b2["decisions"][0]["selected"] = 0
    records = [a, a2, q, q2, b, b2]
    for r in records:
        r.update(evidence_kind="observed_complete_victory", goal_satisfied=False)
    pairs = [{"root_id":a["root_id"], "goal":a["goal"],
              "better_route_id":a["route_id"], "worse_route_id":b["route_id"]}
             for a,b in zip(records[::2],records[1::2])]
    return records, pairs


def test_relative_preference_gradient_and_root_query_weighting():
    records,pairs = preference_examples(); data = PreferenceDataset(records,pairs)
    w = np.zeros((2,FEATURE_COUNT)); w[:,:2]=[[.17,-.3],[.25,.14]]
    loss,gradient,costs,_ = data.evaluate(w)
    np.testing.assert_allclose(data.pair_weights, [.25,.25,.5])
    assert loss == pytest.approx(sum(weight*np.logaddexp(0,costs[a]-costs[b])
                                    for (a,b),weight in zip(data.pairs,data.pair_weights)))
    for head in range(2):
        for feature in range(2):
            plus,minus = w.copy(),w.copy()
            plus[head,feature]+=1e-5; minus[head,feature]-=1e-5
            numeric=(data.evaluate(plus)[0]-data.evaluate(minus)[0])/2e-5
            assert gradient[head,feature]==pytest.approx(numeric,abs=2e-8)
    # Direct loop matches segmented cost; forced decisions contribute zero.
    logs=data.forward(w)[2]
    expected=np.zeros(len(records))
    for d,r in enumerate(data.decision_route):
        expected[r]+=max(logs[data.action_decision==d])-logs[data.selected_actions[d]]
    np.testing.assert_allclose(costs,expected)


def test_preference_duplicates_and_queries_do_not_add_weight():
    records,pairs=preference_examples(); w=np.zeros((2,FEATURE_COUNT))
    a=PreferenceDataset(records,pairs)
    b=PreferenceDataset(records+[copy.deepcopy(records[0])]*4,pairs+pairs[:1]*5)
    np.testing.assert_array_equal(a.evaluate(w)[1],b.evaluate(w)[1])
    assert a.evaluate(w)[0]==b.evaluate(w)[0]
    # Add another distinct route/pair in one query; that query's total stays 1/4.
    third=copy.deepcopy(records[0]); third["route_id"]="third"
    pair=copy.deepcopy(pairs[0]); pair["worse_route_id"]="third"
    c=PreferenceDataset(records+[third],pairs+[pair])
    assert c.pair_weights.sum()==pytest.approx(1)
    np.testing.assert_allclose(sorted(c.pair_weights),[.125,.125,.25,.5])


def test_preference_ties_forced_steps_and_initial_model_are_deterministic():
    records,pairs=preference_examples()
    for r in records:
        r["decisions"].append({"features":{"schema":FEATURE_SCHEMA,"groups":[0],"rows":[[]]},"selected":0})
    data=PreferenceDataset(records,pairs); w=np.zeros((2,FEATURE_COUNT))
    logs=data.forward(w)[2]; costs,winners=data.relative_costs(logs)
    for d,winner in enumerate(winners):
        candidates=np.flatnonzero(data.action_decision==d)
        assert winner==candidates[np.argmax(logs[candidates])]
    w[:,:2]=[[.1,.2],[-.3,.1]]; saved=w.copy()
    trained,history=fit(data,30,.003,w)
    np.testing.assert_array_equal(saved,w)
    assert history[0][data.metric]==data.evaluate(w)[0]
    assert data.evaluate(trained)[0]<data.evaluate(w)[0]
    # Extreme finite logits use stable softplus and softmax derivatives.
    extreme=w*10000
    assert np.isfinite(data.evaluate(extreme)[0])
    assert np.isfinite(data.evaluate(extreme)[1]).all()


def test_preference_evidence_never_silently_enters_positive_learning():
    records,pairs=preference_examples()
    with pytest.raises(ValueError): RouteDataset(records)
    records[0]["goal_satisfied"]=True
    with pytest.raises(ValueError): RouteDataset([records[0]])
    with pytest.raises(ValueError): PreferenceDataset(records,[])
    bad=copy.deepcopy(pairs[0]); bad["root_id"]="elsewhere"
    with pytest.raises(ValueError): PreferenceDataset(records,[bad]+pairs[1:])
    bad=copy.deepcopy(pairs[0]); bad["goal"]["min_final_hp"]=99
    with pytest.raises(ValueError): PreferenceDataset(records,[bad]+pairs[1:])
    bad=copy.deepcopy(pairs[0]); bad["better_route_id"],bad["worse_route_id"]=bad["worse_route_id"],bad["better_route_id"]
    with pytest.raises(ValueError): PreferenceDataset(records,pairs+[bad])
    with pytest.raises(ValueError): PreferenceDataset(records,pairs[:1])
