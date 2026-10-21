"""Snecko reading separates extra draw from the conditional Confusion hook."""
import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-snecko-reading.json"
PACKAGE = Path(__file__).parents[3] / "reference/expert/curated/ironclad-run-planning.json"


def edited(tmp_path, change):
    doc = json.loads(FIXTURE.read_bytes())
    definition = next(row for row in doc["payload"]["relic_mechanics"]["definitions"]
                      if row["relic"] == "SneckoEye")
    change(definition)
    doc["payload_sha256"] = _digest(doc["payload"])
    path = tmp_path / "constructed-snecko-reading.json"
    path.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    return path


def test_snecko_reading_preserves_eligibility_timing_and_equal_roll_branch():
    before = FIXTURE.read_bytes()
    reading = DecisionReading(FIXTURE)
    text = render_decision(reading.view("mechanics", candidate_ordinals=[0]))
    assert "每回合起手抽牌数 +2" in text and "手牌上限" in text
    assert "战斗前请求施加混乱" in text and "只有混乱成功存在时" in text
    assert "非负战斗费用" in text and "0 到 3（含两端）" in text
    assert "状态与诅咒不统一排除" in text and "负费用不掷" in text
    assert "相同时保留已有本回合费用" in text and "清除免费打出一次标记" in text
    assert "不永久改写主牌组" in text and "再次符合条件抽到时会重掷" in text
    assert "每回合基础能量" not in text and "Sozu" not in text
    assert len(text) < 1100
    candidates = reading.view()["content"]["candidates"]
    assert len(candidates) == 4 and candidates[-1]["action"]["kind"] == "skip_boss_relic"
    assert "混乱" not in render_decision(reading.view())
    assert FIXTURE.read_bytes() == before


def test_missing_old_fields_stay_unknown_without_reconstructing_snecko(tmp_path):
    def omit(definition):
        definition.pop("additional_cards_drawn_each_turn")
        definition.pop("pre_battle_confusion")

    reading = DecisionReading(edited(tmp_path, omit))
    text = render_decision(reading.view("mechanics", candidate_ordinals=[0]))
    assert "具体遗物效果 Unknown" in text
    assert "混乱" not in text and "起手抽牌数" not in text


def test_confusion_fact_alone_does_not_supply_missing_draw_amount(tmp_path):
    reading = DecisionReading(edited(tmp_path, lambda d: d.pop("additional_cards_drawn_each_turn")))
    text = render_decision(reading.view("mechanics", candidate_ordinals=[0]))
    assert "战斗前请求施加混乱" in text and "起手抽牌" not in text


@pytest.mark.parametrize("change", [
    lambda d: d.update(additional_cards_drawn_each_turn=True),
    lambda d: d["pre_battle_confusion"].update(minimum_cost=True),
    lambda d: d["pre_battle_confusion"].update(maximum_cost=-1),
    lambda d: d["pre_battle_confusion"].update(minimum_cost=4),
    lambda d: d["pre_battle_confusion"].update(eligible_cost="exclude_status_and_curse"),
    lambda d: d["pre_battle_confusion"].update(turn_cost_update="always"),
    lambda d: d["pre_battle_confusion"].update(clears_free_to_play_once=1),
    lambda d: d["pre_battle_confusion"].pop("eligible_cost"),
])
def test_malformed_or_unreviewed_confusion_rules_fail_closed(tmp_path, change):
    with pytest.raises(ValueError):
        DecisionReading(edited(tmp_path, change))


def test_snecko_mechanics_and_replanning_knowledge_remain_separate(monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("Snecko reading must not load a natural-language query owner")

    monkeypatch.setattr(cli, "ExpertKnowledgeOwner", forbidden)
    assert cli.main(["curated", "--decision", str(FIXTURE), "--view", "mechanics", "--candidate", "0",
                     "--package", str(PACKAGE), "--entry", "replan-after-relic"]) == 0
    text = capsys.readouterr().out
    assert "非负战斗费用" in text and "replan-after-relic" in text
    assert "改判：" in text and "来源取舍" not in text
    assert "Sozu" not in text and "RunicPyramid" not in text
