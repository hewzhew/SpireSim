import json

import numpy as np
import pytest

pytest.importorskip("sts_learning_bridge")
from tools.guided_search.constructed_demonstrations import Collection, load, verify, sha
from tools.guided_search.entity_learning import positions
from tools.guided_search.natural_imitation import read_material, seed_weights


def plan():
    base = dict(seed=993, encounter="LargeSlime", act=1, room="MonsterRoom", hp=68,
        deck=[["Strike", 0]] * 5 + [["Defend", 0]] * 4 + [["Bash", 0], ["AscendersBane", 0]],
        relics=[], potions=[], max_potion_uses=None)
    other = dict(base, deck=base["deck"] + [["ShrugItOff", 0]])
    return dict(actor="codex_assisted", information="oracle_observation_no_search_no_sl",
        purpose="demonstration_development", max_decisions=2, max_seconds=30,
        cases=[dict(id=str(i), source="test fixture", family="starter", spec=spec) for i, spec in enumerate((base, other))])


def source(directory):
    return dict(kind="constructed_assisted", directory=str(directory), source_manifest=str(directory / "source.json"),
        source_manifest_sha256=sha(directory / "source.json"),
        sha256={n: sha(directory / n) for n in ("producer.json", "trajectory.jsonl", "verification.json")})


def test_structured_reading_aligns_with_numeric_actions_and_stale_requests_do_not_act(tmp_path):
    c = Collection(plan(), tmp_path / "collection")
    before = c.envs[0].evidence_json()
    r = c.observe(0)
    frame = c.envs[0].decision_batch()
    p, = positions(frame)
    assert r["source"] == frame["revision"]
    assert len(r["reading"]["choices"]) == len(p.columns["actions"])
    for i, choice in enumerate(r["reading"]["choices"]):
        if choice["kind"] == "play":
            assert p.columns["action_ids"][i, 0] == 1
            card_row = p.columns["action_refs"][i, 0]
            assert round(p.columns["entity_values"][card_row, 6] * 32) == choice["hand"]
            break
    else:
        pytest.fail("fixture has no play action")
    with pytest.raises(ValueError, match="stale"):
        c.choose(0, r["source"] + 1, i, "rejected stale request")
    with pytest.raises(ValueError, match="outside"):
        c.choose(0, r["source"], 999, "rejected absent action")
    assert c.envs[0].evidence_json() == before
    c.choose(0, r["source"], i, "attributed fixture action")
    receipt = c.finish()
    assert receipt["choices"] == 1 and len(receipt["episodes"]) == 2
    assert receipt["episodes"][1]["choices"] == 0
    assert receipt["episodes"][1]["outcome"] is None
    records = load(c.directory)
    assert len(records) == 1 and not hasattr(records[0], "logp")
    assert verify(c.directory) == receipt
    material, seeds = read_material([source(c.directory)])
    assert len(material) == 1 and seeds == {993}
    unrelated = c.directory / "unrelated-source.json"
    manifest = json.loads((c.directory / "source.json").read_text())
    unrelated.write_text(json.dumps(dict(manifest, unrelated=True)))
    wrong_source = dict(source(c.directory), source_manifest=str(unrelated), source_manifest_sha256=sha(unrelated))
    with pytest.raises(ValueError, match="differs from its producer"):
        read_material([wrong_source])
    unmarked = source(c.directory)
    del unmarked["kind"]
    with pytest.raises(ValueError, match="unadmitted"):
        read_material([unmarked])


def test_losses_and_incomplete_cases_replay_without_success_filter(tmp_path):
    p = plan()
    p["cases"][0]["spec"] = dict(p["cases"][0]["spec"], encounter="JawWorm", hp=1)
    c = Collection(p, tmp_path / "collection")
    for _ in range(2):
        r = c.observe(0)
        if r["reading"] is None:
            break
        selected = next(i for i, a in enumerate(r["reading"]["choices"]) if a["kind"] == "end_turn")
        c.choose(0, r["source"], selected, "intentional loss fixture")
    receipt = c.finish()
    assert receipt["episodes"][0]["outcome"]["kind"] == "loss"
    assert receipt["episodes"][1]["choices"] == 0
    records = load(c.directory)
    assert len(records) == receipt["choices"] > 0


def test_pair_members_share_seed_weight_and_tampering_invalidates_receipt(tmp_path):
    c = Collection(plan(), tmp_path / "collection")
    for ep, repeats in [(0, 2), (1, 1)]:
        for _ in range(repeats):
            r = c.observe(ep)
            end = next(i for i, a in enumerate(r["reading"]["choices"]) if a["kind"] == "end_turn")
            c.choose(ep, r["source"], end, "explicit fixture choice")
    c.finish()
    records = load(c.directory)
    np.testing.assert_allclose(seed_weights(records), [1/3] * 3)
    with pytest.raises(ValueError, match="duplicate run seed"):
        read_material([source(c.directory)] * 2)
    path = c.directory / "trajectory.jsonl"
    rows = path.read_text().splitlines()
    row = json.loads(rows[0])
    row["selected"] += 1
    rows[0] = json.dumps(row)
    path.write_text("\n".join(rows) + "\n")
    with pytest.raises(ValueError, match="receipt"):
        load(c.directory)
    with pytest.raises(ValueError, match="replay"):
        verify(c.directory)


def test_failed_native_step_seals_collection(tmp_path):
    c = Collection(plan(), tmp_path / "collection")
    r = c.observe(0)
    class Broken:
        def __getattr__(self, name):
            return getattr(original, name)
        def advance(self, *args):
            raise RuntimeError("native fixture failure")
    original = c.envs[0]
    c.envs[0] = Broken()
    with pytest.raises(RuntimeError, match="fixture"):
        c.choose(0, r["source"], 0, "will fail")
    assert (c.directory / "execution-failure.json").exists()
    with pytest.raises(ValueError, match="closed"):
        c.finish()
    with pytest.raises(ValueError, match="failed collection"):
        load(c.directory)


def test_virtual_selection_steps_keep_actual_inputs_and_replay(tmp_path):
    p = plan()
    p["max_decisions"] = 8
    p["cases"] = [p["cases"][0]]
    p["cases"][0]["spec"]["deck"] = [["BurningPact", 0], ["Strike", 0], ["Defend", 0]]
    c = Collection(p, tmp_path / "collection")
    r = c.observe(0)
    play = next(i for i, a in enumerate(r["reading"]["choices"]) if a["kind"] == "play"
                and r["reading"]["combat"]["hand"][a["hand"]]["card"] == "BurningPact")
    r = c.choose(0, r["source"], play, "exercise selection source")
    for kind in ("begin_selection", "select_member", "finish_selection"):
        choice = next(i for i, a in enumerate(r["reading"]["choices"]) if a["kind"] == kind)
        r = c.choose(0, r["source"], choice, "exercise " + kind)
    receipt = c.finish()
    rows = [json.loads(s) for s in (c.directory / "trajectory.jsonl").read_text().splitlines()]
    assert receipt["choices"] == len(load(c.directory)) == 4
    assert rows[1]["inputs"] == rows[2]["inputs"] == []
    assert rows[3]["inputs"]
