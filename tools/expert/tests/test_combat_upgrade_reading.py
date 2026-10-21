"""Combat upgrade scope must be visible without implying permanent deck value."""
import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-combat-upgrades.json"


def edited(tmp_path, change):
    document = json.loads(FIXTURE.read_bytes())
    change(document["payload"]["deck_mechanics"]["cards"][0]["facts"]["current"]["definition"]["reviewed_effects"])
    document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "constructed-upgrade-reading.json"
    path.write_text(json.dumps(document), encoding="utf-8")
    return path


def test_armaments_upgrade_difference_survives_unchanged_numbers():
    reading = DecisionReading(FIXTURE)
    before = FIXTURE.read_bytes()
    view = reading.view("mechanics", card_ordinals=[0])
    facts = view["content"]["cards"][0]["facts"]
    assert facts["current"]["persistent_block"] == facts["next_upgrade"]["persistent_block"] == 5
    text = render_decision(view)
    assert "升级一张可升级牌" in text and "升级当前手牌内全部可升级牌" in text
    assert "效果结算时" in text and "不能取消" in text
    assert "不永久升级主牌组" in text and "Searing Blow 可继续升级" in text
    assert "已展示属性不变" not in text and len(text) < 1800
    assert "Apotheosis" not in text and reading.revision not in text
    assert len(reading.view()["content"]["candidates"]) == 3
    assert FIXTURE.read_bytes() == before


def test_apotheosis_scope_cost_and_owned_candidate_stay_bound():
    reading = DecisionReading(FIXTURE)
    view = reading.view("mechanics", candidate_ordinals=[1])
    candidate = view["content"]["candidates"][0]
    assert candidate["action"]["kind"] == "smith"
    assert candidate["owned_card"]["card"]["ordinal"] == 2
    text = render_decision(view)
    assert "手牌、抽牌堆、弃牌堆和消耗堆" in text
    assert "出牌目标：无显式目标" in text
    assert "当前持久费用 2" in text and "下次升级 +1：当前持久费用 1" in text
    assert "之后才生成或进入范围的牌不自动获得升级" in text
    assert "Armaments" not in text and "不永久升级主牌组" in text
    upgraded = render_decision(reading.view("mechanics", card_ordinals=[1]))
    assert "全部可升级牌" in upgraded and "无下一合法卡牌升级" in upgraded


def test_old_record_does_not_reconstruct_upgrade_effect_from_card_name(tmp_path):
    document = json.loads(FIXTURE.read_bytes())
    facts = document["payload"]["deck_mechanics"]["cards"][0]["facts"]
    for field in ("current", "next_upgrade"):
        effects = facts[field]["definition"]["reviewed_effects"]
        effects.pop("combat_upgrade_scope")
        effects.pop("combat_upgrade_persistence")
    document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "constructed-old-upgrade-reading.json"
    path.write_text(json.dumps(document), encoding="utf-8")
    text = render_decision(DecisionReading(path).view("mechanics", card_ordinals=[0]))
    assert "Unknown" in text
    assert "升级一张可升级牌" not in text and "全部可升级牌" not in text


@pytest.mark.parametrize("change", [
    lambda e: e.update(combat_upgrade_scope="master_deck"),
    lambda e: e.update(combat_upgrade_scope=True),
    lambda e: e.update(combat_upgrade_persistence="permanent"),
    lambda e: e.pop("combat_upgrade_persistence"),
    lambda e: e.pop("combat_upgrade_scope"),
])
def test_upgrade_reading_rejects_unknown_scope_or_incomplete_persistence(tmp_path, change):
    with pytest.raises(ValueError, match="combat upgrade"):
        DecisionReading(edited(tmp_path, change))


def test_cli_can_pair_scope_comparison_with_resource_knowledge(monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("mechanics reading must not retrieve original guides")

    monkeypatch.setattr(cli, "ExpertKnowledgeOwner", forbidden)
    assert cli.main([
        "curated", "--decision", str(FIXTURE), "--view", "mechanics", "--candidate", "0",
        "--package", "reference/expert/curated/ironclad-run-planning.json", "--entry", "hp-recovery",
    ]) == 0
    text = capsys.readouterr().out
    assert "升级一张可升级牌" in text and "全部可升级牌" in text
    assert "hp-recovery" in text and "Apotheosis" not in text
