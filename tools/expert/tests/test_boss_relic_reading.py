"""Boss-resource tradeoffs use explicit mechanics without taking decision ownership."""
import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-boss-relic-resources.json"
PACKAGE = Path(__file__).parents[3] / "reference/expert/curated/ironclad-run-planning.json"


def changed_document(tmp_path, change):
    doc = json.loads(FIXTURE.read_bytes())
    change(doc["payload"]["relic_mechanics"]["definitions"])
    doc["payload_sha256"] = _digest(doc["payload"])
    path = tmp_path / "constructed-boss-relic-reading.json"
    path.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    return path


def test_candidates_keep_distinct_costs_and_do_not_imply_current_ownership():
    original = FIXTURE.read_bytes()
    reading = DecisionReading(FIXTURE)
    candidates = reading.view()["content"]["candidates"]
    assert len(candidates) == 4
    assert candidates[-1]["action"]["kind"] == "skip_boss_relic"
    views = [render_decision(reading.view("mechanics", candidate_ordinals=[i])) for i in range(3)]
    for text in views:
        assert "每回合基础能量 +1" in text and "获取当下立即" in text
        assert len(text) < 750 and "counter=" not in text
    assert "禁用营火的休息选项" in views[0] and "其他治疗来源" in views[0]
    assert "阻止获得新药水" in views[1] and "已有药水仍可使用" in views[1]
    assert "每回合最多打出 6 张牌" in views[2] and "零费牌也计数" in views[2]
    assert "Sozu" not in views[0] and "VelvetChoker" not in views[1]
    owned = reading.view("relics")["content"]
    assert {row["relic"] for row in owned} == {"BurningBlood", "FusionHammer"}
    hammer = next(row["ordinal"] for row in owned if row["relic"] == "FusionHammer")
    text = render_decision(reading.view("mechanics", relic_ordinals=[hammer]))
    assert "禁用营火的升级选项" in text and "其他升级来源" in text
    assert "禁用营火的休息选项" not in text
    assert FIXTURE.read_bytes() == original


def test_legacy_record_does_not_infer_constraints_from_relic_names(tmp_path):
    def omit(rows):
        for row in rows:
            for field in ("energy_per_turn_delta", "campfire_restriction", "potion_acquisition_rule",
                          "card_play_limit_per_turn"):
                row.pop(field, None)

    reading = DecisionReading(changed_document(tmp_path, omit))
    text = render_decision(reading.view("mechanics", candidate_ordinals=[0, 1, 2]))
    assert "具体遗物效果 Unknown" in text
    assert "基础能量 +1" not in text and "阻止获得新药水" not in text
    assert "禁用营火" not in text and "最多打出" not in text
    assert len(reading.view()["content"]["candidates"]) == 4


@pytest.mark.parametrize("field,value", [
    ("energy_per_turn_delta", True),
    ("energy_per_turn_delta", -1),
    ("energy_per_turn_delta", 256),
    ("campfire_restriction", "all_healing"),
    ("campfire_restriction", []),
    ("potion_acquisition_rule", {"prevents_new_potions": True}),
    ("potion_acquisition_rule", {"prevents_new_potions": 1, "owned_potions_remain_usable": True}),
    ("card_play_limit_per_turn", 0),
    ("card_play_limit_per_turn", True),
])
def test_malformed_resource_rules_fail_closed(tmp_path, field, value):
    with pytest.raises(ValueError):
        DecisionReading(changed_document(tmp_path, lambda rows: rows[0].update({field: value})))


def test_resource_reading_combines_selected_knowledge_without_raw_guide_queries(monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("boss resource reading must not load raw guide queries")

    monkeypatch.setattr("tools.expert.query.ExpertKnowledgeOwner", forbidden)
    assert cli.main(["curated", "--decision", str(FIXTURE), "--view", "mechanics", "--candidate", "1",
                     "--package", str(PACKAGE), "--entry", "replan-after-relic", "--entry", "potion-reserve"]) == 0
    text = capsys.readouterr().out
    assert "Sozu" in text and "已有药水仍可使用" in text
    assert "replan-after-relic" in text and "potion-reserve" in text
    assert "改判：" in text
    assert "CoffeeDripper" not in text and "card_play_limit_per_turn" not in text
