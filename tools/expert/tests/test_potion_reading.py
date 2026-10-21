"""Selective potion planning from typed facts, without raw-source retrieval."""

import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-potion-readings.json"
PACKAGE = Path(__file__).parents[3] / "reference/expert/curated/ironclad-run-planning.json"


def document(case="shop"):
    return next(row["document"] for row in json.loads(FIXTURE.read_bytes()) if row["case"] == case)


def write_document(tmp_path, doc, name="potion-reading.json"):
    doc["payload_sha256"] = _digest(doc["payload"])
    path = tmp_path / name
    path.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    return path


def test_shop_potion_effects_retain_units_timing_and_candidate_meaning(tmp_path):
    original = FIXTURE.read_bytes()
    reading = DecisionReading(write_document(tmp_path, document()))
    text = [render_decision(reading.view("mechanics", candidate_ordinals=[i])) for i in range(4)]
    assert "丢弃，不会触发使用效果" in text[0]
    assert "荆棘 3" in text[0] and "不立即造成伤害" in text[0]
    assert "最大 HP 的 20%" in text[1] and "不保证至少 1 HP" in text[1]
    assert "治疗修正与禁止治疗" in text[1] and "允许战外使用" in text[1]
    assert "购买，效果须之后使用" in text[1]
    assert "任意数量" in text[2] and "允许选零张" in text[2]
    assert "取消选择不退还" in text[2] and "永久牌组" in text[2]
    assert "虚弱 3" in text[3] and "Artifact" in text[3]
    assert "需指定敌人目标" in text[3]
    assert all(len(value) < 800 for value in text)
    assert "buy Elixir" not in text[1]
    assert "不允许战外主动使用" in text[2]
    assert reading.view()["content"]["candidates"][-1]["action"]["kind"] == "leave_shop"
    # Mixed views preserve all current typed surfaces without item-name lookup.
    mixed = render_decision(reading.view("mechanics", candidate_ordinals=[1, 4, 5]))
    assert "最大 HP 的 20%" in mixed and "请求 1 段" in mixed
    assert "伤害之后排队生成 1 张自身副本" in mixed
    assert "buy Bronze Scales" in mixed and "每场战斗开始请求荆棘 3" in mixed
    assert "定义字段" not in text[1]
    assert FIXTURE.read_bytes() == original


def test_reward_discovery_keeps_choice_contract_and_unknown_effects(tmp_path):
    reading = DecisionReading(write_document(tmp_path, document("reward")))
    power, colorless, fairy = [render_decision(reading.view("mechanics", candidate_ordinals=[i])) for i in range(3)]
    assert "领取，效果须之后使用" in power
    assert "能力池发现 3 选一" in power and "本回合费用设为 0" in power and "可以跳过" in power
    assert "无色池发现 3 选一" in colorless and "不能跳过" in colorless
    assert "具体使用效果 Unknown" in fairy and "自动触发" in fairy


def test_use_and_discard_of_same_potion_keep_distinct_action_bindings(tmp_path):
    doc = document("owned")
    doc["payload"]["potion_mechanics"]["candidates"].reverse()
    reading = DecisionReading(write_document(tmp_path, doc))
    rows = reading.view("mechanics", candidate_ordinals=[0, 1])["content"]["candidates"]
    assert rows[0]["candidate_id"] != rows[1]["candidate_id"]
    assert rows[0]["potion_facts"] == rows[1]["potion_facts"]
    use = render_decision(reading.view("mechanics", candidate_ordinals=[0]))
    discard = render_decision(reading.view("mechanics", candidate_ordinals=[1]))
    assert "本候选是使用" in use and "丢弃，不会触发使用效果" in discard


@pytest.mark.parametrize("change", [
    lambda surface: surface.update(public_snapshot_id="wrong_snapshot"),
    lambda surface: surface.update(coverage="complete"),
    lambda surface: surface["candidates"].pop(),
    lambda surface: surface["candidates"].append(surface["candidates"][0]),
    lambda surface: surface["candidates"][0].update(candidate_id="unknown"),
    lambda surface: surface["candidates"][0]["facts"].update(potion="BloodPotion"),
    lambda surface: surface["candidates"][0]["facts"].update(requires_target=1),
    lambda surface: surface["candidates"][1]["facts"]["reviewed_use"].update(percent=True),
    lambda surface: surface["candidates"][1]["facts"]["reviewed_use"].update(percent=-1),
    lambda surface: surface["candidates"][1]["facts"]["reviewed_use"].pop("percent"),
    lambda surface: surface["candidates"][2]["facts"]["reviewed_use"].update(refund_on_cancel=True),
    lambda surface: surface["candidates"][2]["facts"]["reviewed_use"].update(kind="exhaust_permanent_deck"),
])
def test_malformed_or_misbound_facts_fail_closed(tmp_path, change):
    doc = document()
    change(doc["payload"]["potion_mechanics"])
    with pytest.raises(ValueError):
        DecisionReading(write_document(tmp_path, doc))


@pytest.mark.parametrize("change", [
    lambda discovery: discovery.update(pool=[]),
    lambda discovery: discovery.update(offered_choices=True),
    lambda discovery: discovery.update(base_selected_card_copies=0),
    lambda discovery: discovery.update(can_skip="true"),
])
def test_invalid_discovery_facts_are_rejected(tmp_path, change):
    doc = document("reward")
    change(doc["payload"]["potion_mechanics"]["candidates"][0]["facts"]["card_discovery"])
    with pytest.raises(ValueError):
        DecisionReading(write_document(tmp_path, doc))


def test_missing_legacy_surface_is_unknown_without_name_lookup(tmp_path):
    doc = document()
    del doc["payload"]["potion_mechanics"]
    reading = DecisionReading(write_document(tmp_path, doc))
    text = render_decision(reading.view("mechanics", candidate_ordinals=[1]))
    assert "药水使用机制 Unknown" in text and "20%" not in text
    assert len(reading.view()["content"]["candidates"]) == 7


def test_cli_compares_selected_potions_with_curated_resources_without_raw_search(tmp_path, monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("potion planning must not query raw strategies")

    monkeypatch.setattr(cli, "ExpertKnowledgeOwner", forbidden)
    path = write_document(tmp_path, document())
    assert cli.main(["curated", "--decision", str(path), "--view", "mechanics", "--candidate", "1",
                     "--candidate", "2", "--package", str(PACKAGE), "--entry", "shop-tradeoffs",
                     "--entry", "potion-reserve"]) == 0
    text = capsys.readouterr().out
    assert "20%" in text and "允许选零张" in text and "为难点留药" in text
    assert "buy Anger" not in text and "readiness-horizon" not in text
