"""Selective shop reads retain sequence, trigger eligibility, and pile changes."""
import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-shop-card-secondary.json"
FIELDS = ("enemy_debuffs_in_monster_order", "hp_loss_strength_power",
          "draw_pile_skill_to_hand", "copy_to_discard_after_damage")


def test_shop_comparison_preserves_secondary_effects_without_choosing_actions():
    before = FIXTURE.read_bytes()
    reading = DecisionReading(FIXTURE)
    anger, shockwave, rupture, secret = [
        render_decision(reading.view("mechanics", candidate_ordinals=[i])) for i in (0, 2, 4, 6)
    ]
    assert "虚弱 3 层 → 易伤 3 层" in shockwave and "虚弱 5 层 → 易伤 5 层" in shockwave
    assert '一层 人工制品 阻止第一个减益' in shockwave
    assert "能力 1 层" in rupture and "能力 2 层" in rupture
    assert "收益不乘失血量" in rupture and "敌人伤害不触发" in rupture
    assert '多份 自燃 合并为一次失血事件' in rupture
    assert "完全阻止则不触发" in rupture and "打出当下不加力" in rupture
    assert "从抽牌堆直接取 1 张现有技能牌" in secret
    assert "打出时抽牌堆必须有技能牌" in secret and "效果结算时重新检查" in secret
    assert '无法抽牌 不阻止' in secret and "满手则放入弃牌堆" in secret
    assert "保留当时费用，不自动免费" in secret and "不能取消" in secret
    assert "消耗/虚无/固有：是/否/否" in secret and "消耗/虚无/固有：否/否/否" in secret
    assert "伤害之后排队生成 1 张自身副本到弃牌堆" in anger
    assert "仅在本场战斗内增加牌量" in anger and "可能阻止复制结算" in anger
    assert '操控现实' in anger and "不复制临时显示伤害" in anger
    assert all(len(text) < 1500 for text in (anger, shockwave, rupture, secret))
    assert "buy Rupture" not in secret and "buy Shockwave" not in anger
    assert len(reading.view()["content"]["candidates"]) == 8
    assert FIXTURE.read_bytes() == before


def write_document(tmp_path, document):
    document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "constructed-secondary-mutation.json"
    path.write_text(json.dumps(document), encoding="utf-8")
    return path


@pytest.mark.parametrize("value", [[], {}, [{"debuff": "frail", "amount": 3}],
    [{"debuff": "weak", "amount": True}], [{"debuff": "weak", "amount": 0}],
    [{"debuff": "weak", "amount": 3, "guaranteed": True}]])
def test_invalid_ordered_effects_fail_closed(tmp_path, value):
    document = json.loads(FIXTURE.read_bytes())
    document["payload"]["card_mechanics"]["candidates"][2]["facts"]["current"]["reviewed_effects"][FIELDS[0]] = value
    with pytest.raises(ValueError):
        DecisionReading(write_document(tmp_path, document))


@pytest.mark.parametrize("field,value", [("hp_loss_strength_power", True),
    ("hp_loss_strength_power", 0), ("draw_pile_skill_to_hand", 2),
    ("draw_pile_skill_to_hand", []), ("copy_to_discard_after_damage", True),
    ("copy_to_discard_after_damage", 2)])
def test_invalid_secondary_amounts_fail_closed(tmp_path, field, value):
    document = json.loads(FIXTURE.read_bytes())
    document["payload"]["card_mechanics"]["candidates"][0]["facts"]["current"]["reviewed_effects"][field] = value
    with pytest.raises(ValueError):
        DecisionReading(write_document(tmp_path, document))


def test_missing_old_fields_stay_unknown_instead_of_lookup_by_card_name(tmp_path):
    document = json.loads(FIXTURE.read_bytes())
    for row in document["payload"]["card_mechanics"]["candidates"]:
        for definition in (row["facts"]["current"], row["facts"]["next_upgrade"]):
            for field in FIELDS:
                definition["reviewed_effects"].pop(field, None)
    text = render_decision(DecisionReading(write_document(tmp_path, document)).view("mechanics"))
    assert "Unknown" in text and "Secret Technique" in text
    assert "从抽牌堆直接取" not in text and "收益不乘失血量" not in text
    assert "伤害之后排队生成" not in text and "虚弱 3 层 →" not in text


def test_cli_combines_shop_knowledge_with_selected_cards_without_raw_query(monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("curated reading must not query original strategy text")

    monkeypatch.setattr("tools.expert.query.ExpertKnowledgeOwner", forbidden)
    assert cli.main(["curated", "--decision", str(FIXTURE), "--view", "mechanics",
        "--candidate", "2", "--candidate", "4", "--package", "reference/expert/curated/ironclad-run-planning.json",
        "--entry", "shop-tradeoffs"]) == 0
    text = capsys.readouterr().out
    assert "虚弱 3 层 → 易伤 3 层" in text and "收益不乘失血量" in text
    assert "shop-tradeoffs" in text and "buy Anger" not in text
