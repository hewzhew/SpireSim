"""Pyramid explains ordinary discard separately from card Retain and Ethereal."""
import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-snecko-reading.json"


def edited(tmp_path, change):
    doc = json.loads(FIXTURE.read_bytes())
    definition = next(row for row in doc["payload"]["relic_mechanics"]["definitions"]
                      if row["relic"] == "RunicPyramid")
    change(definition)
    doc["payload_sha256"] = _digest(doc["payload"])
    path = tmp_path / "constructed-pyramid-reading.json"
    path.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    return path


def test_pyramid_reading_is_selective_and_keeps_retain_and_hand_capacity_distinct():
    before = FIXTURE.read_bytes()
    reading = DecisionReading(FIXTURE)
    text = render_decision(reading.view("mechanics", candidate_ordinals=[2]))
    assert "跳过通常的回合末弃牌" in text and "不赋予保留标记" in text
    assert "不因此触发保留效果或确立基础的减费" in text
    assert "未保留的虚无牌仍会消耗" in text and "主动弃牌仍可发生" in text
    assert "占用手牌容量" in text and "不增加起手抽牌数" in text
    assert "SneckoEye" not in text and "Sozu" not in text
    assert len(text) < 650
    assert "确立" not in render_decision(reading.view())
    assert FIXTURE.read_bytes() == before


def test_missing_pyramid_rule_stays_unknown_without_inference_from_relic_name(tmp_path):
    reading = DecisionReading(edited(tmp_path, lambda d: d.pop("end_turn_discard_rule")))
    text = render_decision(reading.view("mechanics", candidate_ordinals=[2]))
    assert "具体遗物效果 Unknown" in text and "跳过通常" not in text


@pytest.mark.parametrize("rule", [True, "retain_all_cards", {"skip": True}])
def test_unreviewed_or_malformed_discard_rules_fail_closed(tmp_path, rule):
    with pytest.raises(ValueError, match="unsupported end-turn discard rule"):
        DecisionReading(edited(tmp_path, lambda d: d.update(end_turn_discard_rule=rule)))
