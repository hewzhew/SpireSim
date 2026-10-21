"""Evidence/learning arithmetic checks, not natural combat quality evidence."""
import copy
import numpy as np
import pytest
import torch

from resource_learning import (hp_intervals, ResourceDataset, ResourceNetwork,
    interval_loss, model_revision, state_input, fit)
from entity_learning import pack


def features(actions=2, requirement=40, hp=68):
    values = [[0.]*32 for _ in range(2)]
    values[0][0] = hp/75
    values[1][:4] = [requirement/75, 0., 1., 68/75]
    return dict(ids=[[1,0],[16,0]],values=values,refs=[[-1,-1,-1],[0,-1,-1]],
        action_ids=[[2,0]]*actions,actions=[[0.]*4 for _ in range(actions)],action_refs=[[-1,-1,-1]]*actions)


def row(parent=None, *, hp=None, won=None, checked=False, complete=False, children=(), actions=2,
        spent=0, cutoff=False, keep=0, min_hp=1, smoked=False):
    o = None if won is None else dict(final_hp=hp,starting_potion_expenditure_slots=spent)
    return dict(parent=parent,branch=None,features=None if won is not None or cutoff else features(actions),
        children=list(children),complete=complete,lower=0,upper=97,
        witnessed_remaining_inputs=0 if checked else None,
        outcome_evidence=dict(combat_terminal=won,combat_smoked=smoked,
            spent_starting_potion_slots=spent,required_original_spent=cutoff,
            observed_terminal_outcome=o,terminal_replay_verified=checked,
            terminal_satisfies_query=None if won is None else won and hp>=min_hp and not bool(keep&spent)))


def graph(rows, *, min_hp=1, keep=0):
    return dict(schema=1,root={},goal=dict(min_final_hp=min_hp,preserve_starting_potion_slots=keep),rows=rows)


def test_witness_is_a_lower_bound_and_unknown_successor_keeps_upper_open():
    d=graph([row(complete=True,children=[(1,1),(2,0)]),row(0,hp=48,won=True,checked=True),row(0)])
    result=hp_intervals(d,75)
    assert result.tolist()==[[48,75],[48,48],[0,75]]
    # Distance caps, witnessed input counts and current HP cannot close a branch.
    d['rows'][0].update(lower=97,upper=97,witnessed_remaining_inputs=1)
    d['rows'][2]['features']['values'][0][0]=0.01
    np.testing.assert_array_equal(result,hp_intervals(d,75))


def test_partial_source_never_claims_full_resource_coverage():
    d=graph([row(complete=False,children=[(1,1)]),row(0,hp=48,won=True,checked=True)])
    assert hp_intervals(d,75)[0].tolist()==[48,75]
    d['rows'][0]['complete']=True
    with pytest.raises(ValueError,match='coverage'): hp_intervals(d,75)
    d['rows'][0]['features']=features(1)
    assert hp_intervals(d,75)[0].tolist()==[48,48]


def test_unreplayed_terminal_does_not_supply_a_verified_positive():
    d=graph([row(complete=True,children=[(1,1),(2,1)]),
             row(0,hp=48,won=True,checked=True),row(0,hp=60,won=True)])
    assert hp_intervals(d,75).tolist()==[[48,60],[48,48],[0,60]]
    d['rows'][2]['outcome_evidence']['terminal_replay_verified']=True
    d['rows'][2]['witnessed_remaining_inputs']=0
    assert hp_intervals(d,75)[0].tolist()==[60,60]


def test_failure_cutoff_and_top_bucket_have_explicit_meanings():
    assert hp_intervals(graph([row(hp=0,won=False)]),75).tolist()==[[0,0]]
    assert hp_intervals(graph([row(hp=68,won=False,smoked=True)]),75).tolist()==[[0,0]]
    d=graph([row(spent=1,cutoff=True,keep=1)],keep=1)
    assert hp_intervals(d,75).tolist()==[[0,0]]
    assert hp_intervals(graph([row(hp=101,won=True,checked=True)]),75).tolist()==[[75,75]]


def test_evidence_mismatch_missing_fields_and_foreign_edges_are_rejected():
    d=graph([row(complete=True,children=[(1,1),(2,1)]),row(0,hp=48,won=True,checked=True),row(0)])
    mutations=[lambda d:d['rows'][0].pop('outcome_evidence'),
        lambda d:d['rows'][0].update(children=[(1,1),(1,1)]),
        lambda d:d['rows'][2].update(parent=1),
        lambda d:d['rows'][1]['outcome_evidence'].update(terminal_satisfies_query=False),
        lambda d:d['rows'][2]['outcome_evidence'].update(terminal_replay_verified=True),
        lambda d:d['rows'][2]['outcome_evidence'].update(required_original_spent=True)]
    for edit in mutations:
        bad=copy.deepcopy(d); edit(bad)
        with pytest.raises(ValueError): hp_intervals(bad,75)


def test_input_transform_removes_collector_threshold_only_and_never_mutates_source():
    a=features(); before=copy.deepcopy(a); b=features(requirement=1)
    pa,pb=state_input(a),state_input(b)
    assert a==before
    for name in pa.columns: np.testing.assert_array_equal(pa.columns[name],pb.columns[name])
    assert pa.columns['entity_values'][1,0]==0
    np.testing.assert_array_equal(pa.columns['entity_values'][1,1:],np.asarray(a['values'][1][1:],dtype=np.float32))
    assert all(not c.flags.writeable for c in pa.columns.values())


def test_interval_loss_does_not_regress_to_witness_or_penalize_unknown():
    # All allowed mass at48 or60 satisfies the same lower-bound evidence.
    logits=torch.full((3,76),-1000.,dtype=torch.float64)
    logits[0,48]=0; logits[1,60]=0; logits[2,0]=0; logits.requires_grad_()
    loss=interval_loss(logits,torch.tensor([[48,75],[48,75],[0,75]]))
    assert torch.equal(loss,torch.zeros(3,dtype=torch.float64))
    loss.sum().backward(); assert torch.equal(logits.grad,torch.zeros_like(logits))
    wrong=torch.zeros((1,76),requires_grad=True)
    interval_loss(wrong,torch.tensor([[48,75]])).sum().backward()
    assert wrong.grad[0,0]>0 and wrong.grad[0,60]<0
    for bounds in ([[49,48]], [[-1,75]], [[0,76]]):
        with pytest.raises(ValueError): interval_loss(wrong,torch.tensor(bounds))


def test_network_tails_are_monotone_and_outcomes_never_become_inputs():
    torch.manual_seed(7)
    model=ResourceNetwork(75).eval()
    data=pack([state_input(features())],kind_count=18)
    tails=model.tails(data)
    assert tails.shape==(1,75) and torch.isfinite(tails).all()
    assert (tails[:,1:]<=tails[:,:-1]).all() and (tails>=0).all() and (tails<=1).all()
    assert model_revision(model)==model_revision(copy.deepcopy(model))
    a=graph([row(complete=True,children=[(1,1),(2,1)]),row(0,hp=48,won=True,checked=True),row(0)])
    b=copy.deepcopy(a); b['rows'][1]['outcome_evidence']['observed_terminal_outcome']['final_hp']=60
    x=ResourceDataset([dict(group='source',data=a)],75)
    y=ResourceDataset([dict(group='source',data=b)],75)
    assert x.intervals[0,0]==48 and y.intervals[0,0]==60
    for left,right in zip(x.items,y.items):
        for name in left.columns: np.testing.assert_array_equal(left.columns[name],right.columns[name])


def test_unknown_is_not_sampled_and_duplicate_queries_do_not_gain_weight():
    d=graph([row()]); docs=[dict(group='source',data=d)]
    dataset=ResourceDataset(docs,75)
    assert not dataset.eligible.any()
    with pytest.raises(ValueError,match='no factual'): fit(ResourceNetwork(75),dataset,steps=1)
    with pytest.raises(ValueError,match='duplicate'): ResourceDataset(docs*2,75)
    d=graph([row(complete=True,children=[(1,1),(2,1)]),row(0,hp=48,won=True,checked=True),row(0)])
    dataset=ResourceDataset([dict(group='source',data=d)],75)
    result=fit(ResourceNetwork(75),dataset,steps=2,batch_size=2)
    assert result['eligible_rows']==1 and result['unique_rows_seen']==1 and result['steps']==2
