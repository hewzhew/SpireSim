"""Owned relic navigation is bounded, exact, and separate from action selection."""
import json
from pathlib import Path

import pytest

from tools.expert.decision import DecisionReading, render_decision

ROOT = Path(__file__).parents[3]
FIXTURE = Path(__file__).parent / "fixtures/public-gold-gain-reading.json"
RELIC_FIXTURES = Path(__file__).parent / "fixtures/public-relic-readings.json"


def owned_sundial(tmp_path):
    document = next(row["document"] for row in json.loads(RELIC_FIXTURES.read_bytes())
                    if row["case"] == "owned")
    path = tmp_path / "constructed-owned-sundial.json"
    path.write_text(json.dumps(document, ensure_ascii=False), encoding="utf-8")
    return DecisionReading(path)


def test_owned_relic_directory_keeps_order_without_loading_every_mechanic_into_context():
    reading = DecisionReading(FIXTURE)
    view = reading.view("relics")
    assert view["content"] == [
        {"ordinal": 0, "relic": "BloodyIdol"},
        {"ordinal": 1, "relic": "QuestionCard"},
        {"ordinal": 2, "relic": "PrayerWheel"},
    ]
    text = render_decision(view)
    assert '[1] 问号牌' in text and "--relic" in text
    assert "counter=" not in text and "可选张数" not in text and "额外生成" not in text
    assert len(text) < 350


def test_selected_relics_preserve_requested_order_and_do_not_become_candidates():
    before = FIXTURE.read_bytes()
    reading = DecisionReading(FIXTURE)
    view = reading.view("mechanics", relic_ordinals=[2, 1])
    assert [row["ordinal"] for row in view["content"]["relics"]] == [2, 1]
    text = render_decision(view)
    assert text.index("[relic 2]") < text.index("[relic 1]")
    assert "额外生成 1 份" in text and "可选张数 +1" in text
    assert "BloodyIdol" not in text and "Ectoplasm" not in text and "take" not in text
    assert view["selected_relics"] is True and view["selected_candidates"] is False
    assert view["public_snapshot_id"] not in text and reading.revision not in text
    assert len(reading.view()["content"]["candidates"]) == 4
    assert FIXTURE.read_bytes() == before


def test_observed_counter_is_distinct_from_definition_initial_counter_and_returned_data_is_detached(tmp_path):
    reading = owned_sundial(tmp_path)
    rows = reading.view("relics")["content"]
    ordinal = next(row["ordinal"] for row in rows if row["relic"] == "Sundial")
    view = reading.view("mechanics", relic_ordinals=[ordinal])
    relic = view["content"]["relics"][0]
    assert relic["observed"]["counter"] == 2
    assert relic["facts"]["shuffle_energy"]["initial_counter"] == 0
    text = render_decision(view)
    assert "计数 2" in text and "定义初始计数 0" in text
    relic["observed"]["counter"] = 999
    assert reading.view("mechanics", relic_ordinals=[ordinal])["content"]["relics"][0]["observed"]["counter"] == 2


@pytest.mark.parametrize("selection", [[], [True], [-1], [3], [0, 0]])
def test_invalid_relic_ordinals_fail_closed(selection):
    with pytest.raises(ValueError):
        DecisionReading(FIXTURE).view("mechanics", relic_ordinals=selection)


@pytest.mark.parametrize("part,extra", [
    ("relics", {}), ("overview", {}),
    ("mechanics", {"candidate_ordinals": [0]}),
    ("mechanics", {"card_ordinals": [0]}),
])
def test_relic_ordinals_cannot_mix_selection_domains(part, extra):
    with pytest.raises(ValueError):
        DecisionReading(FIXTURE).view(part, relic_ordinals=[0], **extra)


def test_changed_export_invalidates_a_previous_inventory_selection(tmp_path):
    path = tmp_path / "reading.json"
    path.write_bytes(FIXTURE.read_bytes())
    reading = DecisionReading(path)
    reading.view("relics")
    path.write_bytes(path.read_bytes() + b"\n")
    with pytest.raises(ValueError):
        reading.view("mechanics", relic_ordinals=[1])


def test_renderer_rejects_an_older_derived_view_contract():
    view = DecisionReading(FIXTURE).view("relics")
    view["schema"] = "sts-public-decision-reading-v1"
    with pytest.raises(ValueError, match="schema"):
        render_decision(view)


def test_cli_selects_owned_relics_through_shared_reader_without_raw_query(monkeypatch, capsys):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("owned relic reading must not query original sources")

    monkeypatch.setattr("tools.expert.query.ExpertKnowledgeOwner", forbidden)
    assert cli.main(["curated", "--decision", str(FIXTURE), "--view", "mechanics", "--relic", "2",
                     "--package", str(ROOT / "reference/expert/curated/ironclad-run-planning.json"),
                     "--entry", "replan-after-relic"]) == 0
    text = capsys.readouterr().out
    assert '[relic 2] 转经轮' in text and "replan-after-relic" in text
    assert "[relic 1]" not in text and "[relic 0]" not in text
    assert cli.main(["curated", "--relic", "0"]) == 1
    assert cli.main(["curated", "--decision", str(FIXTURE), "--view", "mechanics",
                     "--relic", "0", "--candidate", "0"]) == 1
