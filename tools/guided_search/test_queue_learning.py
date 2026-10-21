import copy

import numpy as np
import pytest
from scipy.special import logsumexp

from path_learning import FEATURE_COUNT, fit
from queue_learning import QueueDataset


def sample(root="a", snapshot="one", hp=20):
    def node(parent, choice, groups, rows):
        return {"parent":parent, "incoming_choice":choice,
                "features":{"schema":1, "groups":groups, "rows":rows}}
    return {"schema":1, "evidence_kind":"witnessed_queue", "root_id":root,
        "family":root, "snapshot_id":snapshot,
        "goal":{"min_final_hp":hp, "preserve_starting_potion_slots":0},
        "nodes":[
            node(None,None,[0,0,1],[[[0,1.]],[[1,2.]],[[0,-.5]]]),
            node(0,0,[0,1],[[[0,.3]],[[1,-.8]]]),
            node(1,1,[0,1],[[[1,1.2]],[[0,.4]]]),
            node(0,1,[0,0,1],[[[1,.1]],[[0,.8]],[[1,-.2]]])],
        "edges":[
            {"parent":0,"choice":2,"depth":1,"sequence":5,"witnesses":[]},
            {"parent":1,"choice":0,"depth":2,"sequence":7,"witnesses":[]},
            {"parent":2,"choice":0,"depth":3,"sequence":12,"witnesses":[0]},
            {"parent":2,"choice":1,"depth":3,"sequence":13,"witnesses":[]},
            {"parent":3,"choice":2,"depth":2,"sequence":9,"witnesses":[1]}]}


def test_shared_ancestry_costs_and_listwise_gradient():
    d=QueueDataset([sample()]); w=np.zeros((2,FEATURE_COUNT))
    w[:,:2]=[[.17,-.3],[.25,.14]]
    loss,gradient,costs,ranks=d.evaluate(w)
    logs=d.forward(w)[2]
    # Independent path sum; shared ancestors occur in every descendant's cost.
    expected=[]
    for edge in d.records[0]["edges"]:
        n,c=edge["parent"],edge["choice"]; total=0.
        while True:
            segment=logs[d.action_offsets[n]:d.action_offsets[n+1]]
            total+=max(segment)-segment[c]
            p=d.records[0]["nodes"][n]
            n,c=p["parent"],p["incoming_choice"]
            if n is None: break
        expected.append(total)
    np.testing.assert_allclose(costs,expected,atol=1e-14)
    assert loss==pytest.approx(logsumexp(-costs)-logsumexp(-costs[d.witnessed]))
    for head in range(2):
        for f in range(2):
            a,b=w.copy(),w.copy(); a[head,f]+=1e-5; b[head,f]-=1e-5
            numeric=(d.evaluate(a)[0]-d.evaluate(b)[0])/2e-5
            assert gradient[head,f]==pytest.approx(numeric,abs=2e-8)


def test_root_query_snapshot_weighting_and_deduplication():
    records=[sample(),sample(snapshot="two"),sample(hp=30),sample(root="b")]
    d=QueueDataset(records); w=np.zeros((2,FEATURE_COUNT))
    np.testing.assert_allclose(d.sample_weights,[.125,.125,.25,.5])
    duplicate=QueueDataset(records+[copy.deepcopy(records[0])]*4)
    assert len(duplicate.records)==4
    np.testing.assert_array_equal(d.evaluate(w)[1],duplicate.evaluate(w)[1])
    bad=copy.deepcopy(records[0]); bad["edges"][0]["witnesses"]=[2]
    with pytest.raises(ValueError,match="inconsistent duplicate"):
        QueueDataset(records+[bad])


def test_queue_rank_uses_depth_then_unsigned_sequence_and_not_cost_scale():
    d=QueueDataset([sample()])
    # All tied: deeper edges first, then smaller insertion sequence.
    assert d.ranks(np.zeros(5)).tolist()==[1]
    d.sequences[2]=2**64-1
    assert d.ranks(np.zeros(5)).tolist()==[2]
    costs=np.array([0.,1.,2.,2.,1.])
    assert d.ranks(costs).tolist()==[3]
    np.testing.assert_array_equal(d.ranks(costs),d.ranks(costs*.01))
    assert d.checkpoint_key(.1,np.array([2]))>d.checkpoint_key(9.,np.array([1]))


def test_training_keeps_initial_weights_and_checkpoints_rank_before_loss():
    d=QueueDataset([sample()]); initial=np.zeros((2,FEATURE_COUNT))
    initial[:,:2]=[[.17,-.3],[.25,.14]]; saved=initial.copy()
    trained,history=fit(d,30,.003,initial)
    np.testing.assert_array_equal(initial,saved)
    before=d.evaluate(initial); after=d.evaluate(trained)
    assert d.checkpoint_key(after[0],after[3])<=d.checkpoint_key(before[0],before[3])
    assert "mean_sampled_queue_rank" in history[0]
    assert np.isfinite(d.evaluate(initial*10000)[0])


def test_equal_mean_ranks_cannot_choose_a_worse_loss_due_to_float_roundoff():
    d=QueueDataset([sample(root=str(i)) for i in range(5)])
    keys=[d.checkpoint_key(.5,np.array([1+(j==i) for j in range(5)])) for i in range(5)]
    assert len(set(keys))==1
    assert keys[0]>d.checkpoint_key(.4,np.array([2,1,1,1,1]))


@pytest.mark.parametrize("kind",["cycle","choice","edge","duplicate","sequence","depth","empty_positive","unused"])
def test_invalid_tree_and_missing_witnesses_are_rejected(kind):
    r=sample()
    if kind=="cycle": r["nodes"][1]["parent"]=1
    if kind=="choice": r["nodes"][1]["incoming_choice"]=99
    if kind=="edge": r["edges"][0]["parent"]=-1
    if kind=="duplicate": r["edges"].append(copy.deepcopy(r["edges"][0]))
    if kind=="sequence": r["edges"][0]["sequence"]=[]
    if kind=="depth": r["edges"][0]["depth"]=3
    if kind=="empty_positive":
        for e in r["edges"]: e["witnesses"]=[]
    if kind=="unused": r["edges"].pop()
    with pytest.raises(ValueError): QueueDataset([r])
