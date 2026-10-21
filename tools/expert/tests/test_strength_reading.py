"""Strength conditions and schedules from the Rust public export owner."""

import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-card-reward-strength.json"


def test_selected_strength_reading_keeps_conditions_schedule_and_upgrade():
    reading = DecisionReading(FIXTURE)
    rows = reading.view("mechanics")["content"]["candidates"]
    by_card = {row["facts"]["current"]["card"]: row for row in rows}

    def text_for(card):
        return render_decision(reading.view("mechanics", candidate_ordinals=[by_card[card]["ordinal"]]))

    spot = text_for("SpotWeakness")
    assert "效果结算时" in spot and "零伤害攻击" in spot
    assert "非攻击意图仍可打出但不加力" in spot
    assert "力量 +3" in spot and "力量 +4" in spot
    assert "每个玩家回合开始" not in spot and "take Disarm" not in spot
    assert len(spot) < 800
    inflame = text_for("Inflame")
    assert "力量 +2" in inflame and "力量 +3" in inflame and "不安排回合末回收" in inflame
    flex = text_for("Flex")
    assert "力量 +2" in flex and "力量 +4" in flex
    assert "另请求同量 LoseStrength" in flex and "Artifact" in flex and "提前清除" in flex
    demon = text_for("DemonForm")
    assert "每个玩家回合开始抽牌后" in demon and "打出当下不直接加力" in demon
    assert "力量 +2" in demon and "力量 +3" in demon
    assert "减力请求 2" in text_for("Disarm")
    assert "抽牌请求 1" in text_for("ShrugItOff")


def write_document(tmp_path, document):
    document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "constructed-mutation.json"
    path.write_text(json.dumps(document), encoding="utf-8")
    return path


@pytest.mark.parametrize("change", [
    lambda gain: gain.update(amount=True),
    lambda gain: gain.update(amount=0),
    lambda gain: gain.update(amount=-3),
    lambda gain: gain.update(delivery="permanent_unconditional"),
    lambda gain: gain.update(delivery=[]),
    lambda gain: gain.pop("delivery"),
    lambda gain: gain.update(unreviewed_damage_multiplier=2),
])
def test_invalid_or_unrendered_strength_facts_are_rejected(tmp_path, change):
    document = json.loads(FIXTURE.read_text(encoding="utf-8"))
    gain = document["payload"]["card_mechanics"]["candidates"][1]["facts"]["current"]["reviewed_effects"]["player_strength_gain"]
    change(gain)
    with pytest.raises(ValueError):
        DecisionReading(write_document(tmp_path, document))


def test_missing_old_strength_field_stays_unknown_without_card_lookup(tmp_path):
    document = json.loads(FIXTURE.read_text(encoding="utf-8"))
    for row in document["payload"]["card_mechanics"]["candidates"]:
        for variant in (row["facts"]["current"], row["facts"]["next_upgrade"]):
            if variant is not None:
                variant["reviewed_effects"].pop("player_strength_gain", None)
    reading = DecisionReading(write_document(tmp_path, document))
    text = render_decision(reading.view("mechanics", candidate_ordinals=[1]))
    assert "Unknown" in text and "Spot Weakness" in text
    assert "力量 +3" not in text and "零伤害攻击" not in text
