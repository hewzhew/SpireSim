"""Owned-instance readings must preserve upgrade reversals and exact public binding."""
import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-owned-card-instances.json"


def edited(tmp_path, mutate):
    document = json.loads(FIXTURE.read_bytes())
    mutate(document["payload"])
    document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "constructed-owned-reading-mutation.json"
    path.write_text(json.dumps(document), encoding="utf-8")
    return path


def test_same_named_cards_keep_opposite_upgrade_cost_changes_and_selection_identity():
    reading = DecisionReading(FIXTURE)
    before = FIXTURE.read_bytes()
    view = reading.view("mechanics", card_ordinals=[1, 0])
    cards = view["content"]["cards"]
    assert [row["card"]["ordinal"] for row in cards] == [1, 0]
    assert [row["facts"]["current"]["persistent_cost"] for row in cards] == [3, 0]
    assert [row["facts"]["next_upgrade"]["persistent_cost"] for row in cards] == [2, 2]
    text = render_decision(view)
    assert '[deck 1] 腐化' in text and '[deck 0] 腐化' in text
    assert "当前持久费用 0" in text and "下次升级 +1：当前持久费用 2" in text
    assert "Strike" not in text and "营火/遗物副作用" in text
    assert "Unknown" in text and reading.revision not in text and len(text) < 1600
    candidate = reading.view("mechanics", candidate_ordinals=[0])["content"]["candidates"][0]
    assert candidate["action"]["kind"] == "smith"
    assert candidate["owned_card"] == cards[1]
    assert len(reading.view()["content"]["candidates"]) == 10
    assert FIXTURE.read_bytes() == before


def test_persistent_growth_repeat_upgrade_and_dynamic_effects_survive_short_reads():
    reading = DecisionReading(FIXTURE)
    text = render_decision(reading.view("mechanics", card_ordinals=[2, 3, 4]))
    assert "实例字段：伤害 20 / 格挡 0" in text
    assert "实例字段：伤害 23 / 格挡 0" in text
    assert "实例字段：伤害 0 / 格挡 17" in text
    assert "实例字段：伤害 0 / 格挡 20" in text
    assert "下次升级 +6：实例字段：伤害 51" in text
    text = render_decision(reading.view("mechanics", card_ordinals=[5, 6, 8]))
    assert "实例字段：伤害 42" in text and "格挡 31" in text
    assert "misc=42" in text and "misc=31" in text
    assert "当前持久费用 X" in text and "按有效 X 请求次数" in text
    secret = render_decision(reading.view("mechanics", card_ordinals=[7]))
    assert "消耗/虚无/固有：是/否/否" in secret and "消耗/虚无/固有：否/否/否" in secret
    assert "效果结算时重新检查" in secret
    assert "无下一合法卡牌升级" in render_decision(reading.view("mechanics", card_ordinals=[9]))


def test_cli_combines_selected_owned_instance_with_campfire_knowledge(monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("owned reading must not query original sources")

    monkeypatch.setattr("tools.expert.query.ExpertKnowledgeOwner", forbidden)
    assert cli.main(["curated", "--decision", str(FIXTURE), "--view", "mechanics", "--card", "0",
                     "--package", "reference/expert/curated/ironclad-run-planning.json", "--entry", "hp-recovery"]) == 0
    text = capsys.readouterr().out
    assert "当前持久费用 0" in text and "当前持久费用 2" in text and "hp-recovery" in text
    assert "[deck 1]" not in text and "Strik" not in text
    assert cli.main(["curated", "--decision", str(FIXTURE), "--view", "mechanics", "--card", "0", "--candidate", "1"]) == 1


def test_old_exports_do_not_reconstruct_missing_instance_mechanics(tmp_path):
    reading = DecisionReading(edited(tmp_path, lambda p: p.pop("deck_mechanics")))
    text = render_decision(reading.view("mechanics", card_ordinals=[0]))
    assert "Unknown" in text and '腐化' in text and "当前持久费用" not in text
    candidate = render_decision(reading.view("mechanics", candidate_ordinals=[0]))
    assert "Unknown" in candidate and "当前持久费用" not in candidate


@pytest.mark.parametrize("selection", [[], [True], [-1], [10], [0, 0]])
def test_card_selection_rejects_invalid_or_ambiguous_ordinals(selection):
    reading = DecisionReading(FIXTURE)
    with pytest.raises(ValueError):
        reading.view("mechanics", card_ordinals=selection)


@pytest.mark.parametrize("change", [
    lambda p: p["deck_mechanics"].update(public_snapshot_id="other"),
    lambda p: p["deck_mechanics"]["cards"].pop(),
    lambda p: p["deck_mechanics"]["cards"].append(p["deck_mechanics"]["cards"][0]),
    lambda p: p["deck_mechanics"]["cards"][0].update(ordinal=True),
    lambda p: p["deck_mechanics"]["cards"][0].update(card_sha256=p["deck_mechanics"]["cards"][1]["card_sha256"]),
    lambda p: p["deck_mechanics"]["cards"][0]["facts"]["current"].update(persistent_cost=True),
    lambda p: p["deck_mechanics"]["cards"][0]["facts"]["current"]["definition"].update(card="Strike"),
    lambda p: p["deck_mechanics"]["cards"][0]["facts"]["next_upgrade"]["definition"].update(upgrades=2),
    lambda p: p["decision"]["observation"]["observation"]["cards"][0].update(cost_modifier=0),
])
def test_readings_reject_missing_or_cross_bound_instance_evidence(tmp_path, change):
    with pytest.raises(ValueError):
        DecisionReading(edited(tmp_path, change))


def test_candidate_cannot_borrow_a_different_deck_card_reading(tmp_path):
    def mutate(payload):
        action = payload["decision"]["candidates"][0]["action"]
        action["card_ordinal"] = 2
        payload["strategic_reading"]["candidates"][0]["candidate_sha256"] = _digest(payload["decision"]["candidates"][0])

    with pytest.raises(ValueError, match="candidate differs"):
        DecisionReading(edited(tmp_path, mutate)).view("mechanics", candidate_ordinals=[0])
