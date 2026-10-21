import json
from pathlib import Path

import pytest

from tools.expert.curated import CuratedStrategy, _digest, render_curated
from tools.expert.decision import DecisionReading, render_decision

REPOSITORY = Path(__file__).parents[3]
FIXTURE = Path(__file__).parent / "fixtures/public-card-reward.json"
PACKAGE = REPOSITORY / "reference/expert/curated/ironclad-card-reward.json"


def edited_fixture(tmp_path, change, *, resign=False):
    document = json.loads(FIXTURE.read_text(encoding="utf-8"))
    change(document["payload"])
    if resign:
        document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "decision.json"
    path.write_text(json.dumps(document, ensure_ascii=False), encoding="utf-8")
    return path


def test_rust_produced_export_reads_all_candidates_without_raw_corpus():
    decision = DecisionReading(FIXTURE)
    view = decision.view()
    candidates = view["content"]["candidates"]
    assert len(candidates) == 7
    assert len({row["candidate_id"] for row in candidates}) == 7
    assert [row["action"]["upgrades"] for row in candidates[:2]] == [0, 1]
    assert [row["action"]["kind"] for row in candidates[2:]] == [
        "skip_card_reward",
        "singing_bowl",
        "discard_run_potion",
        "take_card",
        "take_card",
    ]
    text = render_decision(view)
    assert "DarkShackles+1" in text and "Unknown" in text
    assert "deployable_policy" in text
    assert view["public_snapshot_id"] not in text and decision.revision not in text
    assert "cards" not in view["content"]
    assert len(text) < 1000


def test_views_keep_exact_card_variants_map_edges_and_unknown_history():
    reading = DecisionReading(FIXTURE)
    deck = reading.view("deck")["content"]
    assert [(card["ordinal"], card["upgrades"]) for card in deck] == [(0, 0), (1, 1)]
    topology = reading.view("map")["content"]
    assert len(topology["nodes"][0]["edges"]) == 2
    assert topology["nodes"][2]["room"] == "question_mark"
    assert topology["boss"] is None
    history = reading.view("history")
    assert history["content"]["recent_combat_hp_loss"] is None
    assert "Unknown" in render_decision(history)
    # A caller cannot mutate the retained reading through a returned view.
    deck[0]["upgrades"] = 99
    assert reading.view("deck")["content"][0]["upgrades"] == 0


def test_changed_export_is_not_silently_read_as_current(tmp_path):
    path = edited_fixture(
        tmp_path, lambda payload: payload["decision"]["candidates"].reverse()
    )
    with pytest.raises(ValueError, match="bytes changed"):
        DecisionReading(path)
    path.write_bytes(FIXTURE.read_bytes())
    reading = DecisionReading(path)
    path.write_bytes(path.read_bytes() + b"\n")
    with pytest.raises(ValueError, match="changed"):
        reading.view()


@pytest.mark.parametrize(
    "change",
    [
        lambda payload: payload["decision"]["information"]["candidate_surface"][
            "ordered_candidate_ids"
        ].pop(),
        lambda payload: payload["decision"]["candidates"][0]["action"].update(
            kind="unknown_future_action"
        ),
        lambda payload: payload["decision"]["candidates"][0]["action"].update(
            extra_unobserved_effect=1
        ),
        lambda payload: payload["decision"]["information"]["observation"].update(
            schema_name="TaskStrategicPublicObservationV99"
        ),
    ],
)
def test_invalid_contracts_fail_instead_of_omitting_options(tmp_path, change):
    # Re-signing isolates shape validation from accidental-byte integrity.
    path = edited_fixture(tmp_path, change, resign=True)
    with pytest.raises(ValueError):
        DecisionReading(path)


def test_knowledge_checks_scope_without_inferring_applicability(tmp_path):
    reading = DecisionReading(FIXTURE)
    package = CuratedStrategy(PACKAGE)
    result = reading.knowledge(package, ["future-value", "skip-density"])
    assert result["applicability"] == "unassessed"
    assert len(render_curated(result)) < 800
    assert len(reading.knowledge(package, [])["entries"]) == 8
    path = edited_fixture(
        tmp_path,
        lambda payload: payload["decision"]["observation"]["observation"]["run"].update(
            player_class="silent"
        ),
        resign=True,
    )
    with pytest.raises(ValueError, match="scope"):
        DecisionReading(path).knowledge(package, [])


def test_knowledge_receipt_does_not_assert_live_or_current_applicability():
    reading = DecisionReading(FIXTURE)
    package = CuratedStrategy(PACKAGE)
    first = reading.knowledge(package, ["energy"])["readings"][0]
    result = reading.knowledge(package, ["energy"], known={"energy": first["revision"]})
    assert result["readings"][0]["status"] == "unchanged"
    assert result["applicability"] == "unassessed"
    assert (
        reading.view()["freshness"] == "detached_export_not_checked_against_live_head"
    )


def test_public_views_do_not_echo_seed_bearing_session_names(tmp_path):
    def change(payload):
        payload["source"] = {
            "kind": "run_play_head",
            "session_id": "seed-987654321-private",
            "head_revision": 2,
            "archive_node_id": "internal-node",
            "evidence_identity": payload["producer"]["evidence_identity"],
        }

    reading = DecisionReading(edited_fixture(tmp_path, change, resign=True))
    view = reading.view()
    assert view["source"] == {"kind": "run_play_head", "head_revision": 2}
    assert "987654321" not in json.dumps(view)


def test_cli_selective_knowledge_does_not_repeat_context_or_load_query_owner(
    tmp_path, monkeypatch, capsys
):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("raw query owner is not part of this reading")

    monkeypatch.setattr(cli, "ExpertKnowledgeOwner", forbidden)
    isolated_package = tmp_path / "package.json"
    isolated_package.write_bytes(PACKAGE.read_bytes())
    isolated_decision = tmp_path / "decision.json"
    isolated_decision.write_bytes(FIXTURE.read_bytes())
    base = [
        "--repository",
        str(tmp_path),
        "curated",
        "--package",
        str(isolated_package),
        "--decision",
        str(isolated_decision),
    ]
    assert cli.main(base) == 0
    overview = capsys.readouterr().out
    assert "HP 22/80" in overview and "future-value" in overview
    assert cli.main(base + ["--entry", "future-value"]) == 0
    selective = capsys.readouterr().out
    assert "HP 22/80" not in selective and "改判：" in selective
    assert cli.main(base + ["--view", "map", "--entry", "near-threat"]) == 0
    assert "question_mark" in capsys.readouterr().out


def test_mechanics_bind_variants_and_select_without_repeating_the_whole_scene():
    reading = DecisionReading(FIXTURE)
    selected = reading.view("mechanics", candidate_ordinals=[1, 0, 5, 6])
    rows = selected["content"]["candidates"]
    assert [row["ordinal"] for row in rows] == [1, 0, 5, 6]
    upgraded, base, breath, escape = [row["facts"] for row in rows]
    assert upgraded["current"]["upgrades"] == 1
    assert upgraded["next_upgrade"] is None
    assert base["current"]["reviewed_effects"]["enemy_strength_reduction"] == {
        "amount": 9,
        "restoration": "shackled_at_owner_turn_end",
    }
    assert breath["current"]["reviewed_effects"]["unconditional_draw_request"] == 1
    assert breath["next_upgrade"]["reviewed_effects"]["unconditional_draw_request"] == 2
    # Conditional block stays a definition field, not an unconditional gain.
    assert escape["current"]["definition_numbers"]["block"] == 3
    assert (
        escape["current"]["reviewed_effects"]["base_block_delivery"]
        == "if_this_draw_history_contains_skill"
    )
    assert escape["current"]["reviewed_effects"]["end_of_turn_block_each_turn"] is None
    text = render_decision(selected)
    assert "Unknown" in text and "Shackled" in text and "Artifact" in text
    assert "技能牌" in text
    assert "HP 22/80" not in text and reading.revision not in text
    assert len(text) < 2300
    base["current"]["upgrades"] = 99
    assert (
        reading.view("mechanics", candidate_ordinals=[0])["content"]["candidates"][0][
            "facts"
        ]["current"]["upgrades"]
        == 0
    )


def test_non_card_actions_remain_readable_without_inventing_card_facts():
    reading = DecisionReading(FIXTURE)
    rows = reading.view("mechanics")["content"]["candidates"]
    assert len(rows) == 7
    assert [row["facts"] for row in rows[2:5]] == [None, None, None]
    for selected in ([], [-1], [7], [0, 0], [True]):
        with pytest.raises(ValueError, match="ordinal"):
            reading.view("mechanics", candidate_ordinals=selected)
    with pytest.raises(ValueError, match="mechanics view"):
        reading.view("deck", candidate_ordinals=[0])


@pytest.mark.parametrize(
    "change",
    [
        lambda payload: payload["card_mechanics"].update(
            public_snapshot_id="another-observation"
        ),
        lambda payload: payload["card_mechanics"].update(coverage="complete"),
        lambda payload: payload["card_mechanics"]["candidates"].pop(),
        lambda payload: payload["card_mechanics"]["candidates"].append(
            payload["card_mechanics"]["candidates"][0]
        ),
        lambda payload: payload["card_mechanics"]["candidates"][0].update(
            candidate_id=payload["decision"]["information"]["candidate_surface"][
                "ordered_candidate_ids"
            ][2]
        ),
        lambda payload: payload["card_mechanics"]["candidates"][0]["facts"][
            "current"
        ].update(upgrades=1),
        lambda payload: payload["card_mechanics"]["candidates"][0]["facts"][
            "next_upgrade"
        ].update(card="Disarm"),
        lambda payload: payload["card_mechanics"]["candidates"][0]["facts"]["current"][
            "reviewed_effects"
        ].update(unknown_future_effect=42),
        lambda payload: payload["card_mechanics"]["candidates"][0]["facts"]["current"][
            "reviewed_effects"
        ].update(base_block_delivery="guessed_unconditional"),
        lambda payload: payload["card_mechanics"]["candidates"][0]["facts"]["current"][
            "reviewed_effects"
        ]["enemy_strength_reduction"].update(restoration="guessed_permanent"),
    ],
)
def test_mechanics_reject_bad_bindings_and_unrendered_contracts(tmp_path, change):
    with pytest.raises(ValueError):
        DecisionReading(edited_fixture(tmp_path, change, resign=True))


def test_mechanics_bind_by_exact_id_even_if_fact_rows_are_reordered(tmp_path):
    reading = DecisionReading(
        edited_fixture(
            tmp_path,
            lambda payload: payload["card_mechanics"]["candidates"].reverse(),
            resign=True,
        )
    )
    rows = reading.view("mechanics", candidate_ordinals=[0, 1])["content"]["candidates"]
    assert [row["facts"]["current"]["upgrades"] for row in rows] == [0, 1]


def test_v1_remains_readable_as_evidence_without_guessed_mechanics(tmp_path):
    document = json.loads(FIXTURE.read_text(encoding="utf-8"))
    document["schema"] = "sts-public-decision-export-v1"
    del document["payload"]["card_mechanics"]
    document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "v1.json"
    path.write_text(json.dumps(document, ensure_ascii=False), encoding="utf-8")
    reading = DecisionReading(path)
    assert len(reading.view()["content"]["candidates"]) == 7
    with pytest.raises(ValueError, match="no card mechanics"):
        reading.view("mechanics")


def test_missing_block_delivery_stays_unknown_in_older_reading(tmp_path):
    def remove_delivery(payload):
        for row in payload["card_mechanics"]["candidates"]:
            for version in ("current", "next_upgrade"):
                facts = row["facts"][version]
                if facts is not None:
                    facts["reviewed_effects"].pop("base_block_delivery", None)

    reading = DecisionReading(edited_fixture(tmp_path, remove_delivery, resign=True))
    view = reading.view("mechanics", candidate_ordinals=[6])
    assert (
        view["content"]["candidates"][0]["facts"]["current"]["reviewed_effects"].get(
            "base_block_delivery"
        )
        is None
    )
    text = render_decision(view)
    assert "Unknown" in text and "出牌时请求一次基础格挡" not in text


def test_cli_mechanics_is_selective_and_does_not_repeat_knowledge_catalog(capsys):
    from tools.expert import cli

    base = [
        "curated",
        "--decision",
        str(FIXTURE),
        "--view",
        "mechanics",
        "--candidate",
        "5",
    ]
    assert cli.main(base) == 0
    text = capsys.readouterr().out
    assert "DeepBreath" in text and "DarkShackles" not in text
    assert "future-value" not in text and "HP 22/80" not in text
    assert cli.main(base + ["--entry", "draw-access"]) == 0
    assert "改判：" in capsys.readouterr().out


def test_mechanics_only_view_does_not_require_a_knowledge_package(tmp_path, capsys):
    from tools.expert import cli

    assert (
        cli.main(
            [
                "--repository",
                str(tmp_path),
                "curated",
                "--decision",
                str(FIXTURE),
                "--view",
                "mechanics",
                "--candidate",
                "0",
            ]
        )
        == 0
    )
    assert "DarkShackles" in capsys.readouterr().out
