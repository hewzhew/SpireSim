import copy
import gzip
import json

import pytest

from tools.guided_search.combat_quality import (
    attainment, compare_observations, project_episode, report, review_trial, validate_requirements,
)


def episode(hp=31, potions=(None, "ExplosivePotion"), kind="win"):
    def position(health, slots):
        return dict(engine="test", combat=dict(
            entities=dict(player=dict(current_hp=health, max_hp=75, gold=99, energy_master=3,
                relics=[dict(id="BurningBlood"), dict(id="HappyFlower", counter=1)]),
                potions=[dict(id=p, uuid=i) if p else None for i,p in enumerate(slots)],
                monsters=[dict(enemy_id="Lagavulin")]),
            meta=dict(master_deck_snapshot=[dict(id="RitualDagger", lineage="master_deck:1", misc=15)],
                      meta_changes=[]),
            runtime=dict(pending_rewards=[], combat_mugged=False, combat_smoked=False), rng=dict(counter=1)))
    return dict(seed=71, root=position(60, ("FearPotion", "ExplosivePotion")),
        successor=position(hp,potions), outcome=dict(seed=71, starting_hp=60, hp=hp,
            potions=list(potions), potion_uses=1, kind=kind, win_replayed=kind=="win"))


def row(e, case="a", **context):
    q=project_episode(e)
    return dict(id=case, kind=q["kind"], quality=q,
                quality_context=dict(encounter="Lagavulin", hp=60, **context), initial_enemies=["Lagavulin"])


def test_equal_win_rewards_do_not_hide_large_hp_difference():
    before,after=row(episode(66)),row(episode(31))
    req=[dict(id="leave45",min_hp=45,reserve={"ExplosivePotion":1})]
    assert attainment([before],req)[0]["met"]==1
    assert attainment([after],req)[0]["missed"]==1
    result=compare_observations(before["quality"],after["quality"])
    assert result["relation"]=="less_hp_same_other_recorded_resources"
    assert result["hp_change"]==-35 and result["continuation_value"]=="unassessed"


def test_inventory_requirements_use_types_multiplicity_and_outgoing_inventory():
    r=row(episode(60,("FirePotion","DexterityPotion")))
    qs=[dict(id="twofire",min_hp=30,reserve={"FirePotion":2}),
        dict(id="fire",min_hp=30,reserve={"FirePotion":1}),
        dict(id="original",min_hp=30,reserve="starting_inventory"),
        dict(id="free",min_hp=30,reserve="unrestricted")]
    assert [x["met"] for x in attainment([r],qs)]==[0,1,0,1]
    # Regenerating a consumed potion still meets an outgoing type requirement;
    # explicit use counts and potion instance IDs are not the objective.
    e=episode(60,("FearPotion","ExplosivePotion"))
    e["outcome"]["potion_uses"]=2
    e["successor"]["combat"]["entities"]["potions"][0]["uuid"]=100
    assert attainment([row(e)],qs)[2]["met"]==1


def test_death_and_unfinished_attempts_remain_in_every_requirement_denominator():
    rs=[row(episode(50)),row(episode(0,kind="loss")),
        row(episode(60,kind="unknown_DecisionBudget")),dict(kind="unknown_not_attempted")]
    req=[dict(id="hp40",min_hp=40,reserve="unrestricted")]
    result=attainment(rs,req)[0]
    assert (result["cases"],result["met"],result["missed"],result["unresolved"])==(4,1,1,2)
    assert result["observed_fraction_bounds"]==[.25,.75]


@pytest.mark.parametrize("field",["relic", "permanent_card", "reward", "gold", "max_hp"])
def test_other_successor_changes_prevent_hp_only_comparison(field):
    a=episode(66)
    b=episode(31)
    c=b["successor"]["combat"]
    if field=="relic":
        c["entities"]["player"]["relics"][1]["counter"]=2
    elif field=="permanent_card":
        c["meta"]["meta_changes"]=[dict(ModifyCardMisc=dict(card_lineage="master_deck:1",amount=5))]
    elif field=="reward":
        c["runtime"]["pending_rewards"]=[dict(Gold=20)]
    else:
        c["entities"]["player"][field]+=3
    result=compare_observations(project_episode(a),project_episode(b))
    assert result["relation"]=="multiple_resource_changes"
    assert len(result["changed"])==2


def test_root_identity_rng_and_unknown_do_not_become_dominance_labels():
    a=project_episode(episode())
    b=copy.deepcopy(a)
    b["rng_sha256"]="different"
    assert compare_observations(a,b)["rng_changed"]
    b["root_sha256"]="different"
    with pytest.raises(ValueError,match="same source"):
        compare_observations(a,b)
    b=project_episode(episode(kind="unknown_DecisionBudget"))
    assert compare_observations(a,b)["relation"]=="unresolved"
    b["contract"]="future_contract"
    with pytest.raises(ValueError,match="contract"):
        attainment([dict(quality=b)],[dict(id="x",min_hp=1,reserve="unrestricted")])


def test_resource_projection_rejects_inconsistent_evidence_and_is_owned():
    e=episode()
    q=project_episode(e)
    q["observed_end"]["relics"][1]["counter"]=99
    assert e["successor"]["combat"]["entities"]["player"]["relics"][1]["counter"]==1
    e["outcome"]["hp"]+=1
    with pytest.raises(ValueError,match="disagrees"):
        project_episode(e)
    e=episode()
    e["outcome"]["win_replayed"]=False
    with pytest.raises(ValueError,match="replay"):
        project_episode(e)


def test_different_loadouts_and_actual_enemy_compositions_do_not_share_a_quality_group():
    a=row(episode(),deck=[["Strike",0]])
    b=row(episode(),case="b",deck=[["Bludgeon",1]])
    c=copy.deepcopy(a)
    c.update(id="c",initial_enemies=["AcidSlimeL"])
    result=report([a,b,c],[])
    assert len(result["groups"])==3


@pytest.mark.parametrize("requirement",[
    dict(id="bad",min_hp=-1,reserve="unrestricted"),
    dict(id="bad",min_hp=True,reserve="unrestricted"),
    dict(id="bad",min_hp=1,reserve="one_potion"),
    dict(id="bad",min_hp=1,reserve={"FirePotion":0}),
])
def test_invalid_requirements_are_rejected(requirement):
    with pytest.raises(ValueError):
        validate_requirements([requirement])


def test_read_only_review_keeps_missing_endpoint_and_checks_source(tmp_path):
    source=tmp_path/"trial"
    source.mkdir()
    spec=dict(seed=71,encounter="Lagavulin",hp=60)
    plan=dict(schema=4,partition="development",model_seeds=[1],evaluation_batch_size=16,
              evaluation=[dict(id="case",family="test",spec=spec)])
    e=episode()
    e["construction"]=spec
    record=dict(id="case",**e["outcome"])
    summary=dict(complete=False,models=[dict(seed=1,initial=dict(rows=[record]))])
    for name,data in [("plan.json",plan),("summary.json",summary),("producer.json",dict(test=True))]:
        (source/name).write_text(json.dumps(data))
    batch=source/"model-1/initial/batch-0000"
    batch.mkdir(parents=True)
    with gzip.open(batch/"evidence.json.gz","wt") as f:
        json.dump(dict(schema=4,poisoned=False,episodes=[e]),f)
    req=[dict(id="hp45",min_hp=45,reserve="unrestricted")]
    result=review_trial(source,tmp_path/"review",req)
    assert result["complete"] and result["models"][0]["relations"]=={"unresolved":1}
    after=json.loads((tmp_path/"review/model-1-final.json").read_text())
    assert after["groups"][0]["attainment"][0]["unresolved"]==1
    record["hp"]+=1
    (source/"summary.json").write_text(json.dumps(summary))
    with pytest.raises(ValueError,match="differs"):
        review_trial(source,tmp_path/"bad-review",req)
    assert not json.loads((tmp_path/"bad-review/review.json").read_text())["complete"]
    record["hp"]-=1
    summary["models"][0]["initial"]["rows"].append(copy.deepcopy(record))
    (source/"summary.json").write_text(json.dumps(summary))
    with pytest.raises(ValueError,match="case accounting"):
        review_trial(source,tmp_path/"duplicate-review",req)
