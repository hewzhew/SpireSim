"""Generated reward choices and independently generated rows stay distinct."""
import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-gold-gain-reading.json"


def edited(tmp_path, relic, change):
    doc = json.loads(FIXTURE.read_bytes())
    definition = next(row for row in doc["payload"]["relic_mechanics"]["definitions"]
                      if row["relic"] == relic)
    change(definition)
    doc["payload_sha256"] = _digest(doc["payload"])
    path = tmp_path / "constructed-reward-width-reading.json"
    path.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    return path


def test_crown_only_expands_its_choice_count_and_the_eligible_reward_sources():
    before = FIXTURE.read_bytes()
    reading = DecisionReading(FIXTURE)
    text = render_decision(reading.view("mechanics", candidate_ordinals=[1]))
    assert "可选张数 -2" in text and "多个相关遗物的修正相加" in text
    assert "奖励份数不因此变化" in text
    assert "无色奖励、捕梦网与星系仪" in text
    assert "不改变商店卡牌库存、发现或药水的三选一数量" in text
    assert "每回合基础能量 +1" in text
    assert "Ectoplasm" not in text and "CursedKey" not in text and "阻止获得金币" not in text
    assert len(text) < 650
    assert "可选张数" not in render_decision(reading.view())
    assert FIXTURE.read_bytes() == before


def test_owned_question_card_and_prayer_wheel_have_different_effect_units():
    text = render_decision(DecisionReading(FIXTURE).view("mechanics", relic_ordinals=[1, 2]))
    assert "可选张数 +1" in text and "额外生成 1 份" in text
    assert "精英、Boss 与事件房不适用" in text
    assert "每份独立生成，分别选择或跳过" in text
    assert "每份的可选张数仍受破损王冠、问号牌等修正" in text
    assert "可选张数 -2" not in text


def test_missing_choice_rule_does_not_infer_crown_penalty(tmp_path):
    reading = DecisionReading(edited(tmp_path, "BustedCrown", lambda d: d.pop("card_reward_choice_delta")))
    text = render_decision(reading.view("mechanics", candidate_ordinals=[1]))
    assert "可选张数" not in text and "每回合基础能量 +1" in text


@pytest.mark.parametrize("relic,field,value", [
    ("BustedCrown", "card_reward_choice_delta", True),
    ("BustedCrown", "card_reward_choice_delta", -2.0),
    ("BustedCrown", "card_reward_choice_delta", 0),
    ("BustedCrown", "card_reward_choice_delta", -(2**31)-1),
    ("PrayerWheel", "additional_normal_monster_card_reward_rows", True),
    ("PrayerWheel", "additional_normal_monster_card_reward_rows", 0),
    ("PrayerWheel", "additional_normal_monster_card_reward_rows", 256),
])
def test_malformed_reward_counts_fail_closed(tmp_path, relic, field, value):
    with pytest.raises(ValueError):
        DecisionReading(edited(tmp_path, relic, lambda d: d.update({field: value})))
