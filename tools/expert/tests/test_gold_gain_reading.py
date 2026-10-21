"""Gold restrictions are read selectively without spending the observed balance."""
import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-gold-gain-reading.json"


def edited(tmp_path, change, relic="Ectoplasm"):
    doc = json.loads(FIXTURE.read_bytes())
    definition = next(row for row in doc["payload"]["relic_mechanics"]["definitions"]
                      if row["relic"] == relic)
    change(definition)
    doc["payload_sha256"] = _digest(doc["payload"])
    path = tmp_path / "constructed-gold-gain-reading.json"
    path.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    return path


def test_gold_rule_preserves_owned_balance_spending_and_trigger_scope():
    before = FIXTURE.read_bytes()
    reading = DecisionReading(FIXTURE)
    text = render_decision(reading.view("mechanics", candidate_ordinals=[0]))
    assert "阻止获得金币" in text and "盗贼返还" in text
    assert "已有金币保留" in text and "仍可花费或被盗" in text
    assert "未获得金币时不触发" in text and "血腥神像的治疗" in text
    assert "每回合基础能量 +1" in text
    assert "BustedCrown" not in text and "CursedKey" not in text
    assert len(text) < 650
    assert "阻止获得金币" not in render_decision(reading.view())
    assert {row["relic"] for row in reading.view("relics")["content"]} == {"BloodyIdol", "QuestionCard", "PrayerWheel"}
    assert FIXTURE.read_bytes() == before


def test_missing_gold_rule_is_not_inferred_from_ectoplasm_name(tmp_path):
    reading = DecisionReading(edited(tmp_path, lambda d: d.pop("gold_gain_rule")))
    text = render_decision(reading.view("mechanics", candidate_ordinals=[0]))
    assert "每回合基础能量 +1" in text and "阻止获得金币" not in text


def test_owned_idol_reads_per_gain_healing_without_predicting_income_or_changing_hp():
    before = FIXTURE.read_bytes()
    reading = DecisionReading(FIXTURE)
    view = reading.view("mechanics", relic_ordinals=[0])
    assert view["content"]["relics"][0]["facts"]["gold_gain_healing"] == 5
    text = render_decision(view)
    assert "立即请求治疗 5 HP" in text and "一次收入触发一次" in text
    assert "不按金币数量乘算" in text and "灵体阻止收入时不触发" in text
    assert "治疗修正与禁止治疗" in text and "尚未发生的收入" in text
    assert "可选张数" not in text and "每回合基础能量" not in text
    assert len(text) < 400
    assert "治疗 5 HP" not in render_decision(reading.view())
    assert "治疗 5 HP" not in render_decision(reading.view("relics"))
    assert FIXTURE.read_bytes() == before


def test_missing_idol_healing_is_unknown_instead_of_inferred_from_name(tmp_path):
    reading = DecisionReading(edited(tmp_path, lambda d: d.pop("gold_gain_healing"), "BloodyIdol"))
    text = render_decision(reading.view("mechanics", relic_ordinals=[0]))
    assert "Unknown" in text and "治疗 5 HP" not in text


@pytest.mark.parametrize("amount", [True, 0, -5, 1.5, 2**31])
def test_invalid_gold_gain_healing_amount_fails_closed(tmp_path, amount):
    with pytest.raises(ValueError):
        DecisionReading(edited(tmp_path, lambda d: d.update(gold_gain_healing=amount), "BloodyIdol"))


@pytest.mark.parametrize("rule", [True, "prevent_spending", "clear_owned_gold", {"amount": 0}])
def test_unsupported_gold_rules_fail_closed(tmp_path, rule):
    with pytest.raises(ValueError, match="unsupported gold-gain rule"):
        DecisionReading(edited(tmp_path, lambda d: d.update(gold_gain_rule=rule)))
