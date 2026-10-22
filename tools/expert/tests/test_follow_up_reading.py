"""Delayed effects needed to compare a naturally encountered reward trio."""

import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-card-reward-follow-up.json"


def test_selected_comparison_retains_delayed_payoffs_and_draw_obligation():
    reading = DecisionReading(FIXTURE)
    before = FIXTURE.read_bytes()
    combust, warcry, embrace = [
        render_decision(reading.view("mechanics", candidate_ordinals=[i])) for i in range(3)
    ]
    assert "每个敌人伤害 5" in combust and "每个敌人伤害 7" in combust
    assert "自失血 1" in combust and "两种数值分别叠加" in combust
    assert "先请求合计失血" in combust and "打出当下不结算" in combust
    assert "不按玩家力量/虚弱调整" in combust and '撕裂' in combust
    assert "抽牌请求 1" in warcry and "抽牌请求 2" in warcry
    assert "放回抽牌堆顶" in warcry and '无法抽牌 阻止抽牌也不免除' in warcry
    assert "全部手牌选择" in warcry and "不能取消" in warcry
    assert "手牌为空则跳过" in warcry and "仅一张时自动放回" in warcry
    assert "每有一张牌被消耗" in embrace and "打出当下不直接抽牌" in embrace
    assert '无法抽牌、手牌上限' in embrace
    assert "基础费用 2" in embrace and "基础费用 1" in embrace
    assert all(len(text) < 1300 for text in (combust, warcry, embrace))
    assert "take Warcry" not in combust and "take Combust" not in embrace
    assert reading.view()["content"]["candidates"][-1]["action"]["kind"] == "skip_card_reward"
    assert FIXTURE.read_bytes() == before


def write_document(tmp_path, document):
    document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "constructed-follow-up-mutation.json"
    path.write_text(json.dumps(document), encoding="utf-8")
    return path


@pytest.mark.parametrize("change", [
    lambda effects: effects["end_of_turn_hp_loss_and_damage"].update(hp_loss_per_application=True),
    lambda effects: effects["end_of_turn_hp_loss_and_damage"].update(damage_per_enemy_per_application=-1),
    lambda effects: effects["end_of_turn_hp_loss_and_damage"].pop("hp_loss_per_application"),
    lambda effects: effects["end_of_turn_hp_loss_and_damage"].update(immediate_damage=True),
    lambda effects: effects.update(exhaust_draw_power=True),
    lambda effects: effects.update(exhaust_draw_power=0),
    lambda effects: effects.update(hand_topdeck_after_draw=2),
    lambda effects: effects.update(hand_topdeck_after_draw=[]),
])
def test_invalid_or_unrendered_follow_up_facts_fail_closed(tmp_path, change):
    document = json.loads(FIXTURE.read_bytes())
    change(document["payload"]["card_mechanics"]["candidates"][0]["facts"]["current"]["reviewed_effects"])
    with pytest.raises(ValueError):
        DecisionReading(write_document(tmp_path, document))


def test_missing_old_follow_up_fields_remain_unknown(tmp_path):
    document = json.loads(FIXTURE.read_bytes())
    for row in document["payload"]["card_mechanics"]["candidates"]:
        for definition in (row["facts"]["current"], row["facts"]["next_upgrade"]):
            if definition is not None:
                for key in ("end_of_turn_hp_loss_and_damage", "exhaust_draw_power", "hand_topdeck_after_draw"):
                    definition["reviewed_effects"].pop(key, None)
    reading = DecisionReading(write_document(tmp_path, document))
    text = render_decision(reading.view("mechanics"))
    assert "Unknown" in text and "take Combust" in text
    assert "先请求合计失血" not in text and "消耗抽牌能力" not in text
    assert "放回抽牌堆顶" not in text


def test_cli_reads_reward_mechanics_and_selected_knowledge_without_source_search(monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("curated comparison must not query raw strategy")

    monkeypatch.setattr("tools.expert.query.ExpertKnowledgeOwner", forbidden)
    assert cli.main(["curated", "--decision", str(FIXTURE), "--view", "mechanics",
                     "--candidate", "2", "--entry", "exhaust-startup"]) == 0
    text = capsys.readouterr().out
    assert "消耗抽牌能力" in text and "启动稳定性仍需判断" in text
    assert "take Combust" not in text and "take Warcry" not in text
