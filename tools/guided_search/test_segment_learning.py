from dataclasses import replace
import json

import numpy as np
import pytest
import torch
import sts_learning_bridge as bridge

from tools.guided_search.entity_learning import CaseSampling, EntityNetwork
from tools.guided_search.segment_learning import Cohort, SegmentNetwork, collect, pack, position, update


def spec(seed=921101, hp=60, target=70, *, reserve=(), enemy="GremlinNob", cards=5):
    # Deliberately artificial mechanism/integration sentinel, not learning material.
    return {"seed": seed, "recipe": {"hp": hp, "deck": [["SearingBlow", 10]] * cards,
        "relics": ["HappyFlower"], "potions": ["FirePotion"],
        "route": [{"encounter": "Cultist", "room": "MonsterRoom"},
                  {"encounter": enemy, "room": "MonsterRoomElite"}]},
        "requirement": {"min_hp": target, "reserve": list(reserve)}, "between_rooms": "gold_only"}


def env(**kwargs):
    return bridge.GuidedSegmentTask(spec(**kwargs), 80)


def play(e):
    f = e.decision()
    choice = int(np.flatnonzero(f["combat"]["action_ids"][:, 0] == 1)[0])
    e.advance(f["revision"], choice)


def test_native_source_and_final_resource_target_survive_intermission():
    e = env(reserve=[{"potion": "FirePotion", "count": 1}])
    f = e.decision()
    assert f["segment_schema"] == 1 and "schema" not in f
    assert e.attained() is None
    play(e)
    r = json.loads(e.reading_json())
    assert r["results"][0]["exit_hp"] == 66
    assert e.attained() is None
    with pytest.raises(ValueError, match="stale"):
        e.advance(f["revision"], 0)
    play(e)
    assert e.decision() is None
    assert e.attained() is True
    e.verify()
    evidence = json.loads(e.evidence_json())
    assert evidence["attained"] is True
    assert evidence["resources"]["hp"] == 72
    assert evidence["resources"]["potions"][0]["id"] == "FirePotion"
    assert len(evidence["successor"]["results"]) == 2
    assert any(s["action"].get("Run", {}).get("ClaimReward") == 0
               for s in evidence["transitions"] if isinstance(s["action"].get("Run"), dict))


def test_context_distinguishes_identical_current_combat_and_is_padding_independent():
    torch.manual_seed(19)
    a, b, c = env(target=10), env(target=70, enemy="Lagavulin"), env(cards=8)
    pa, pb, pc = [position(e.decision()) for e in (a, b, c)]
    for name in pa.combat.columns:
        assert np.array_equal(pa.combat.columns[name], pb.combat.columns[name])
    assert not np.array_equal(pa.context["context_values"], pb.context["context_values"])
    model = SegmentNetwork(hidden=16, layers=1).eval()
    with torch.no_grad():
        la, va = model(pack([pa]))
        lb, vb = model(pack([pb]))
        mixed, values = model(pack([pc, pb, pa]))
    count = la.shape[1]
    assert torch.allclose(la[0], mixed[2, :count], atol=2e-6)
    assert torch.allclose(va[0], values[2], atol=2e-6)
    assert not torch.allclose(la.softmax(-1), lb.softmax(-1), atol=1e-7, rtol=0)
    assert not torch.allclose(va, vb, atol=1e-7, rtol=0)
    with pytest.raises(ValueError, match="attention budget"):
        pack([pa, pc], max_attention_cells=1)
    with pytest.raises(RuntimeError):
        model.load_state_dict(EntityNetwork(hidden=16, layers=1).state_dict())


@pytest.mark.parametrize("failure", ["schema", "refs", "values", "ids"])
def test_invalid_context_is_rejected_without_clipping(failure):
    f = env().decision()
    if failure == "schema":
        f["segment_schema"] = 4
    elif failure == "refs":
        f["context_refs"][0, 0] = len(f["context_ids"])
    elif failure == "values":
        f["context_values"][0, 0] = np.nan
    else:
        f["context_ids"][0, 0] = 7
    with pytest.raises(ValueError):
        position(f)


def test_whole_segment_sampled_update_uses_actual_conditioned_outcomes():
    torch.set_num_threads(1)
    torch.manual_seed(20)
    model = SegmentNetwork(hidden=16, layers=1)
    environments = [env(seed=921102, target=1), env(seed=921103, target=999)]
    cohort = collect(environments, model, CaseSampling([31, 37]), max_batches=80, wall_seconds=20)
    assert len(cohort.outcomes) == 2
    assert all(o["verified"] and type(o["attained"]) is bool for o in cohort.outcomes)
    assert cohort.outcomes[1]["attained"] is False
    assert all(json.loads(e.evidence_json())["attained"] == o["attained"]
               for e, o in zip(environments, cohort.outcomes))
    optimizer = torch.optim.Adam(model.parameters(), lr=1e-4)
    before = {k: v.clone() for k, v in model.state_dict().items()}
    # Invalid evidence at a later minibatch must not leave a partial update.
    bad = list(cohort.records)
    bad[-1] = replace(bad[-1], logp=bad[-1].logp + 1.)
    with pytest.raises(ValueError, match="behavior"):
        update(model, optimizer, Cohort(bad, cohort.outcomes), batch_size=1)
    assert all(torch.equal(v, model.state_dict()[k]) for k, v in before.items())
    assert not optimizer.state
    # Keeping the action probabilities does not permit changing the assigned task.
    wrong_task = json.loads(json.dumps(cohort.outcomes))
    wrong_task[0]["specification"]["requirement"]["min_hp"] += 1
    with pytest.raises(ValueError, match="assigned task"):
        update(model, optimizer, Cohort(cohort.records, wrong_task))
    with pytest.raises(ValueError, match="source order"):
        update(model, optimizer, Cohort(cohort.records + [cohort.records[0]], cohort.outcomes))
    invalid_choice = [replace(cohort.records[0], choice=-1), *cohort.records[1:]]
    with pytest.raises(ValueError, match="action outside"):
        update(model, optimizer, Cohort(invalid_choice, cohort.outcomes))
    assert all(torch.equal(v, model.state_dict()[k]) for k, v in before.items())
    assert not optimizer.state
    metrics = update(model, optimizer, cohort, batch_size=2)
    assert metrics["episodes"] == 2 and np.isfinite(metrics["loss"])
    assert any(not torch.equal(v, model.state_dict()[k]) for k, v in before.items())
    assert model.context_facts.weight.grad is not None
    assert torch.count_nonzero(model.context_facts.weight.grad) > 0


def test_unknown_does_not_produce_a_negative_training_target():
    torch.manual_seed(21)
    e = bridge.GuidedSegmentTask(spec(), 1) # Budget consumed on the first map entry.
    assert e.decision() is None and e.attained() is None
    model = SegmentNetwork(hidden=16, layers=1)
    cohort = collect([e], model, CaseSampling([41]), max_batches=1)
    assert cohort.outcomes[0]["attained"] is None
    optimizer = torch.optim.Adam(model.parameters())
    with pytest.raises(ValueError, match="Unknown"):
        update(model, optimizer, cohort)
    assert not optimizer.state
