"""Selective energy planning reads produced by Rust, without source search."""

import json
from pathlib import Path

import pytest

from tools.expert.curated import CuratedStrategy, _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-card-reward-energy.json"
PACKAGE = Path(__file__).parents[3] / "reference/expert/curated/ironclad-card-reward.json"


def selected(reading, card):
    rows = reading.view()["content"]["candidates"]
    ordinal = next(row["ordinal"] for row in rows if row["action"].get("card") == card)
    return reading.view("mechanics", candidate_ordinals=[ordinal])


def test_energy_timing_payment_and_upgrade_survive_selective_reading():
    reading = DecisionReading(FIXTURE)
    before = FIXTURE.read_bytes()
    texts = {
        card: render_decision(selected(reading, card))
        for card in ("Offering", "SeeingRed", "Bloodletting", "Berserk", "Sentinel",
                     "Dropkick", "Adrenaline", "Outmaneuver", "Turbo", "DoubleEnergy")
    }
    assert "加费请求 +2" in texts["Offering"]
    assert "HP 损失请求 6" in texts["Offering"]
    assert "抽牌请求 3" in texts["Offering"] and "抽牌请求 5" in texts["Offering"]
    assert "基础费用 1" in texts["SeeingRed"] and "基础费用 0" in texts["SeeingRed"]
    assert "未减去打出费用" in texts["SeeingRed"]
    assert "加费请求 +2" in texts["Bloodletting"] and "加费请求 +3" in texts["Bloodletting"]
    assert "HP 损失请求 3" in texts["Bloodletting"]
    assert "后续玩家回合开始" in texts["Berserk"] and "打出当下不加费" in texts["Berserk"]
    assert "对玩家施加易伤 2" in texts["Berserk"] and "对玩家施加易伤 1" in texts["Berserk"]
    assert "自身被消耗时" in texts["Sentinel"] and "普通打出不触发" in texts["Sentinel"]
    assert "请求 +2" in texts["Sentinel"] and "请求 +3" in texts["Sentinel"]
    assert "效果结算时" in texts["Dropkick"] and "有易伤" in texts["Dropkick"]
    assert "非易伤目标仍可打出" in texts["Dropkick"]
    assert "请求 +1" in texts["Adrenaline"] and "请求 +2" in texts["Adrenaline"]
    assert "抽牌请求 2" in texts["Adrenaline"]
    assert "下次玩家能量补充时" in texts["Outmaneuver"] and "触发后移除" in texts["Outmaneuver"]
    assert '虚空 ×1 → 弃牌堆' in texts["Turbo"] and "带虚无属性" in texts["Turbo"]
    assert "Unknown" in texts["DoubleEnergy"] and "加费请求" not in texts["DoubleEnergy"]
    assert all(len(text) < 1100 for text in texts.values())
    assert "take Berserk" not in texts["Offering"]
    assert reading.view()["content"]["candidates"][-1]["action"]["kind"] == "skip_card_reward"
    knowledge = reading.knowledge(CuratedStrategy(PACKAGE), ["energy"])
    assert [row["id"] for row in knowledge["readings"]] == ["energy"]
    assert knowledge["applicability"] == "unassessed"
    assert FIXTURE.read_bytes() == before


def write_document(tmp_path, document):
    document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "constructed-energy-mutation.json"
    path.write_text(json.dumps(document), encoding="utf-8")
    return path


@pytest.mark.parametrize("change", [
    lambda effects: effects["energy_gain"].update(amount=True),
    lambda effects: effects["energy_gain"].update(amount=-1),
    lambda effects: effects["energy_gain"].update(amount=0),
    lambda effects: effects["energy_gain"].update(delivery="immediate_net_profit"),
    lambda effects: effects["energy_gain"].update(delivery=[]),
    lambda effects: effects["energy_gain"].pop("delivery"),
    lambda effects: effects["energy_gain"].update(net_gain=3),
    lambda effects: effects.update(player_vulnerable_on_play=True),
    lambda effects: effects.update(status_injections_on_play=[]),
    lambda effects: effects.update(status_injections_on_play=[{
        "card": "Void", "count": 1, "destination": "future_reward", "persistence": "ethereal",
    }]),
])
def test_invalid_energy_or_unrendered_cost_facts_are_rejected(tmp_path, change):
    document = json.loads(FIXTURE.read_bytes())
    change(document["payload"]["card_mechanics"]["candidates"][0]["facts"]["current"]["reviewed_effects"])
    with pytest.raises(ValueError):
        DecisionReading(write_document(tmp_path, document))


def test_old_missing_energy_fields_stay_unknown_without_name_lookup(tmp_path):
    document = json.loads(FIXTURE.read_bytes())
    for row in document["payload"]["card_mechanics"]["candidates"]:
        for variant in (row["facts"]["current"], row["facts"]["next_upgrade"]):
            if variant is not None:
                for field in ("energy_gain", "player_vulnerable_on_play", "status_injections_on_play"):
                    variant["reviewed_effects"].pop(field, None)
    reading = DecisionReading(write_document(tmp_path, document))
    text = render_decision(selected(reading, "Offering"))
    assert "Unknown" in text and "HP 损失请求 6" in text
    assert "加费请求" not in text


def test_cli_combines_selected_mechanics_with_curated_energy_without_raw_query(monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("a curated public reading must not search raw sources")

    monkeypatch.setattr("tools.expert.query.ExpertKnowledgeOwner", forbidden)
    assert cli.main(["curated", "--decision", str(FIXTURE), "--view", "mechanics",
                     "--candidate", "1", "--entry", "energy"]) == 0
    text = capsys.readouterr().out
    assert "加费请求 +2" in text and "基础费用 1" in text and "基础费用 0" in text
    assert "加费需要有值得支付的动作" in text
    assert "take Offering" not in text and "take Sentinel" not in text
