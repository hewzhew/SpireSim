"""Visible-relic reading must not confuse observation with action permission."""

import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-relic-readings.json"
PACKAGE = Path(__file__).parents[3] / "reference/expert/curated/ironclad-run-planning.json"


def document(case="shop"):
    return next(row["document"] for row in json.loads(FIXTURE.read_bytes()) if row["case"] == case)


def write_document(tmp_path, doc):
    doc["payload_sha256"] = _digest(doc["payload"])
    path = tmp_path / "relic-reading.json"
    path.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    return path


def test_unaffordable_visible_relic_can_be_read_without_inventing_buy_action(tmp_path):
    original = FIXTURE.read_bytes()
    reading = DecisionReading(write_document(tmp_path, document()))
    inventory = reading.view("context")["content"]["public"]["inventory"]
    assert inventory["relics"][0]["relic"] == "Sundial"
    assert inventory["relics"][0]["affordable"] is False
    assert inventory["relics"][0]["purchase_status"]["reason"]["kind"] == "not_enough_gold"
    context = render_decision(reading.view("context"))
    assert "Sundial" in context and "271" in context and "not_enough_gold" in context
    assert "恰好到 3" in context and "能量 +2" in context
    assert "计数跨战斗保留" in context and "不是每回合或购买时加费" in context
    assert "是否可购买/领取仍看原库存与候选" in context
    candidates = reading.view()["content"]["candidates"]
    assert len(candidates) == 3 and candidates[-1]["action"]["kind"] == "leave_shop"
    assert all(row["action"].get("relic") != "Sundial" for row in candidates)
    bronze = render_decision(reading.view("mechanics", candidate_ordinals=[0]))
    sling = render_decision(reading.view("mechanics", candidate_ordinals=[1]))
    assert "每场战斗开始" in bronze and "荆棘 3" in bronze and "被格挡" in bronze
    assert "仅精英战斗开始" in sling and "Boss 身份本身不满足" in sling and "力量 +2" in sling
    assert "buy Bronze Scales" not in sling
    assert max(len(bronze), len(sling)) < 700 and len(context) < 1300
    assert FIXTURE.read_bytes() == original


def test_owned_relic_counter_stays_observed_not_replaced_by_definition_initial_value(tmp_path):
    reading = DecisionReading(write_document(tmp_path, document("owned")))
    content = reading.view("relics")["content"]
    selected = reading.view("mechanics", relic_ordinals=[row["ordinal"] for row in content])
    sundial = next(row for row in selected["content"]["relics"] if row["observed"]["relic"] == "Sundial")
    assert sundial["observed"]["counter"] == 2 and sundial["facts"]["shuffle_energy"]["initial_counter"] == 0
    text = render_decision(selected)
    assert "计数 2" in text and "定义初始计数 0" in text
    assert "治疗 6 HP" in text and "需玩家存活" in text and "治疗修正与禁止治疗" in text
    assert "洗牌" not in render_decision(reading.view())
    assert "战斗胜利" not in render_decision(reading.view())


def test_reward_boss_and_owned_views_share_definitions_with_explicit_unknowns(tmp_path):
    reward = DecisionReading(write_document(tmp_path, document("reward")))
    text = render_decision(reward.view("mechanics"))
    assert "仅精英" in text and "具体遗物效果 Unknown" in text
    assert "奖励项 1" in render_decision(reward.view("context"))
    boss = DecisionReading(write_document(tmp_path, document("boss")))
    text = render_decision(boss.view("mechanics", candidate_ordinals=[0]))
    assert "治疗 12 HP" in text and "获取时效果" in text
    blood = next(row["ordinal"] for row in boss.view("relics")["content"] if row["relic"] == "BurningBlood")
    assert "治疗 6 HP" in render_decision(boss.view("mechanics", relic_ordinals=[blood]))


@pytest.mark.parametrize("change", [
    lambda catalog: catalog.update(public_snapshot_id="unbound"),
    lambda catalog: catalog.update(coverage="complete"),
    lambda catalog: catalog["definitions"].pop(),
    lambda catalog: catalog["definitions"].append(catalog["definitions"][0]),
    lambda catalog: catalog["definitions"][0].update(relic="SneckoEye"),
    lambda catalog: catalog["definitions"][0]["combat_victory_healing"].update(base_hp=True),
    lambda catalog: catalog["definitions"][0]["combat_victory_healing"].update(requires_player_alive="true"),
    lambda catalog: catalog["definitions"][1]["shuffle_energy"].update(shuffle_trigger_at=0),
    lambda catalog: catalog["definitions"][1]["shuffle_energy"].update(energy=-1),
    lambda catalog: catalog["definitions"][1]["shuffle_energy"].update(initial_counter=3),
    lambda catalog: catalog["definitions"][1]["shuffle_energy"].pop("counter_persists_between_combats"),
    lambda catalog: catalog["definitions"][2]["battle_start_power"].update(scope=[]),
    lambda catalog: catalog["definitions"][3]["battle_start_power"].update(scope="boss_combat"),
    lambda catalog: catalog["definitions"][3]["battle_start_power"].update(power="immediate_damage"),
    lambda catalog: catalog["definitions"][3]["battle_start_power"].update(amount=True),
])
def test_malformed_or_unobserved_relic_definitions_fail_closed(tmp_path, change):
    doc = document()
    change(doc["payload"]["relic_mechanics"])
    with pytest.raises(ValueError):
        DecisionReading(write_document(tmp_path, doc))


def test_catalog_order_is_not_identity_and_missing_legacy_definitions_stay_unknown(tmp_path):
    doc = document()
    doc["payload"]["relic_mechanics"]["definitions"].reverse()
    reading = DecisionReading(write_document(tmp_path, doc))
    assert "荆棘 3" in render_decision(reading.view("mechanics", candidate_ordinals=[0]))
    del doc["payload"]["relic_mechanics"]
    legacy = DecisionReading(write_document(tmp_path, doc))
    for view in [legacy.view("mechanics"), legacy.view("context"),
                 legacy.view("mechanics", relic_ordinals=[0])]:
        text = render_decision(view)
        assert "遗物机制 Unknown" in text
        assert "荆棘 3" not in text and "恰好到 3" not in text and "治疗 6 HP" not in text
    assert len(legacy.view()["content"]["candidates"]) == 3


def test_cli_reads_public_inventory_with_curated_shop_and_route_knowledge(tmp_path, monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("visible relic reading must not query raw sources")

    monkeypatch.setattr(cli, "ExpertKnowledgeOwner", forbidden)
    path = write_document(tmp_path, document())
    assert cli.main(["curated", "--decision", str(path), "--view", "context",
                     "--package", str(PACKAGE), "--entry", "shop-tradeoffs", "--entry", "route-options"]) == 0
    text = capsys.readouterr().out
    assert "Sundial" in text and "恰好到 3" in text and "路线收益与退路" in text
    assert "potion-reserve" not in text and "buy Sundial" not in text
