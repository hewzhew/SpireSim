"""Cross-language public-reading checks over explicitly constructed Rust exports."""

from copy import deepcopy
import json
from pathlib import Path

import pytest

from tools.expert.curated import CuratedStrategy, _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-strategic-sites.json"
CASES = json.loads(FIXTURE.read_bytes())["cases"]
PACKAGE = (
    Path(__file__).parents[3] / "reference/expert/curated/ironclad-card-reward.json"
)


def read_case(tmp_path, name, change=None):
    document = deepcopy(CASES[name])
    if change:
        change(document["payload"])
        document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / f"{name}.json"
    path.write_text(json.dumps(document, ensure_ascii=False), encoding="utf-8")
    return DecisionReading(path)


@pytest.mark.parametrize("name", CASES)
def test_each_strategic_site_preserves_all_candidates_without_loading_knowledge(
    tmp_path, name
):
    reading = read_case(tmp_path, name)
    before = reading.path.read_bytes()
    candidates = reading.view()["content"]["candidates"]
    expected = CASES[name]["payload"]["decision"]
    assert [row["candidate_id"] for row in candidates] == expected["information"][
        "candidate_surface"
    ]["ordered_candidate_ids"]
    assert [row["action"] for row in candidates] == [
        row["action"] for row in expected["candidates"]
    ]
    text = render_decision(reading.view())
    assert len(text) < 2500
    assert reading.revision not in text
    assert "detached_export" in reading.view()["freshness"]
    for part in [
        "context",
        "deck",
        "map",
        "relics",
        "history",
        "candidates",
        "mechanics",
    ]:
        render_decision(reading.view(part))
    assert reading.path.read_bytes() == before


def test_full_potion_slots_keep_visible_reward_separate_from_claim_actions(tmp_path):
    reading = read_case(tmp_path, "reward")
    context = reading.view("context")["content"]["public"]
    assert context["pending_rewards"][2] == {"kind": "potion", "potion": "FirePotion"}
    actions = [row["action"] for row in reading.view("candidates")["content"]]
    assert any(row["kind"] == "discard_run_potion" for row in actions)
    assert not any(
        row["kind"] == "claim_reward" and row["reward"]["kind"] == "potion"
        for row in actions
    )
    assert "Fire Potion" in render_decision(reading.view("context"))
    empty = read_case(tmp_path, "empty_reward")
    assert empty.view("context")["content"]["public"].get("pending_rewards", []) == []
    assert [row["action"]["kind"] for row in empty.view("candidates")["content"]] == [
        "proceed"
    ]


def test_shop_inventory_includes_unaffordable_and_blocked_offers_without_inventing_actions(
    tmp_path,
):
    reading = read_case(tmp_path, "shop")
    view = reading.view("context")
    inventory = view["content"]["public"]["inventory"]
    assert inventory["cards"][1]["card"] == "Corruption"
    assert inventory["cards"][1]["affordable"] is False
    assert (
        inventory["potions"][0]["purchase_status"]["reason"]["kind"]
        == "no_empty_potion_slot"
    )
    rendered = render_decision(view)
    assert "Corruption" in rendered and "200" in rendered and "Fire Potion" in rendered
    actions = [row["action"] for row in reading.view("candidates")["content"]]
    assert not any(
        row["kind"] == "buy_card" and row["card"] == "Corruption" for row in actions
    )
    assert not any(row["kind"] == "buy_potion" for row in actions)
    mechanics = reading.view("mechanics", candidate_ordinals=[0])["content"][
        "candidates"
    ][0]
    assert (
        mechanics["facts"]["current"]["card"]
        == mechanics["action"]["card"]
        == "DarkShackles"
    )
    assert mechanics["action"]["price"] == 50
    assert "Dark Shackles" in render_decision(
        reading.view("mechanics", candidate_ordinals=[0])
    )
    assert (
        reading.view("mechanics", candidate_ordinals=[1])["content"]["candidates"][0][
            "facts"
        ]
        is None
    )


def test_map_only_uses_public_node_outcome_possibilities(tmp_path):
    reading = read_case(tmp_path, "map")
    action = reading.view("candidates")["content"][1]["action"]
    assert action["question_mark_outcomes"] == ["event", "monster"]
    assert "QuestionMark" in render_decision(reading.view())
    assert reading.view("map")["content"]["boss"] is None


def test_selecting_candidate_details_preserves_domain_and_unknown_effects(tmp_path):
    reading = read_case(tmp_path, "run_choice")
    detail = reading.view("candidates", candidate_ordinals=[0])
    assert detail["selected_candidates"] is True
    action = detail["content"][0]["action"]
    assert action["selectable_card_ordinals"] == [0, 1]
    assert action["min_choices"] == action["max_choices"] == 1
    assert "Strike+" in render_decision(detail)
    assert "Strike+" not in render_decision(reading.view())
    event = read_case(tmp_path, "event")
    assert "Unknown" in render_decision(event.view())
    event_detail = event.view("candidates", candidate_ordinals=[0])
    assert "unavailable" in render_decision(event_detail)
    for selected in [[], [True], [-1], [99], [0, 0]]:
        with pytest.raises(ValueError, match="ordinal"):
            reading.view("candidates", candidate_ordinals=selected)


@pytest.mark.parametrize(
    "change",
    [
        lambda payload: payload["strategic_reading"].update(public_snapshot_id="other"),
        lambda payload: payload["strategic_reading"].update(decision_site="map"),
        lambda payload: payload["strategic_reading"]["candidates"].pop(),
        lambda payload: payload["strategic_reading"]["candidates"].append(
            payload["strategic_reading"]["candidates"][0]
        ),
        lambda payload: payload["strategic_reading"]["candidates"][0].update(
            candidate_id="foreign"
        ),
        lambda payload: payload["strategic_reading"]["candidates"][0].update(brief=""),
        lambda payload: payload["decision"]["candidates"][0]["action"].update(price=0),
        lambda payload: payload["decision"]["observation"]["observation"][
            "context"
        ].update(purge_cost=0),
        lambda payload: payload.update(strategic_reading=None),
        lambda payload: payload["decision"]["observation"]["observation"].update(
            context={"kind": "reward"}
        ),
    ],
)
def test_presentation_cannot_drift_from_its_typed_decision(tmp_path, change):
    with pytest.raises(ValueError):
        read_case(tmp_path, "shop", change)


def test_presentation_joins_by_identity_not_file_row_order(tmp_path):
    original = read_case(tmp_path, "reward").view()
    reordered = read_case(
        tmp_path,
        "reward",
        lambda payload: payload["strategic_reading"]["candidates"].reverse(),
    ).view()
    assert original["content"] == reordered["content"]


def test_default_non_card_view_is_independent_of_card_knowledge_and_raw_query(
    tmp_path, monkeypatch, capsys
):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError(
            "this public view needs no source query or knowledge package"
        )

    monkeypatch.setattr(cli, "ExpertKnowledgeOwner", forbidden)
    reading = read_case(tmp_path, "shop")
    base = ["--repository", str(tmp_path), "curated", "--decision", str(reading.path)]
    assert cli.main(base) == 0
    assert "Dark Shackles" in capsys.readouterr().out
    assert cli.main(base + ["--view", "context"]) == 0
    assert "Corruption" in capsys.readouterr().out
    assert cli.main(base + ["--view", "candidates", "--candidate", "2"]) == 0
    detail = capsys.readouterr().out
    assert "Strike" in detail and "Dark Shackles" not in detail
    with pytest.raises(ValueError, match="scope"):
        reading.knowledge(CuratedStrategy(PACKAGE), [])
