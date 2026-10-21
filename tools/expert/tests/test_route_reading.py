"""Consume Rust route presentations without rebuilding graph semantics in Python."""

from copy import deepcopy
import json
from pathlib import Path

import pytest

from tools.expert.curated import _digest
from tools.expert.decision import DecisionReading, render_decision

FIXTURE = Path(__file__).parent / "fixtures/public-route-readings.json"


def reading(tmp_path, name, change=None):
    document = deepcopy(json.loads(FIXTURE.read_bytes())["cases"][name])
    if change:
        change(document["payload"])
        document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "decision.json"
    path.write_text(json.dumps(document, ensure_ascii=False), encoding="utf-8")
    return DecisionReading(path)


@pytest.mark.parametrize("name", ["entrance", "flight", "unknown_end", "cycle"])
def test_route_views_keep_all_candidates_and_require_no_raw_corpus(tmp_path, name):
    decision = reading(tmp_path, name)
    before = decision.path.read_bytes()
    view = decision.view()
    rows = view["content"]["candidates"]
    expected = json.loads(FIXTURE.read_bytes())["cases"][name]["payload"]["decision"]
    assert [row["candidate_id"] for row in rows] == expected["information"][
        "candidate_surface"
    ]["ordered_candidate_ids"]
    assert [row["action"] for row in rows] == [
        row["action"] for row in expected["candidates"]
    ]
    assert len(render_decision(view)) < 1100
    for i in range(len(rows)):
        detail = decision.view("candidates", candidate_ordinals=[i])
        assert detail["content"][0]["candidate_id"] == rows[i]["candidate_id"]
        render_decision(detail)
    assert decision.path.read_bytes() == before
    assert not (tmp_path / ".oracle-lab").exists()


def test_entrance_detail_preserves_order_and_merge_without_inventing_a_best_route(
    tmp_path,
):
    decision = reading(tmp_path, "entrance")
    detail = render_decision(decision.view("candidates", candidate_ordinals=[0]))
    assert "(0,1) Rest → (0,2) Elite" in detail
    assert "(2,1) Elite → (2,2) Rest" in detail
    assert detail.count("(1,3) Shop") == 2  # A shared endpoint, not two distinct shops.
    assert (
        "(3,0)" not in detail
    )  # Disconnected current alternative remains in overview.
    summary = decision.view()["content"]["candidates"][0]["label"]
    assert "(0,1)" in summary and "(2,1)" in summary
    assert "(1,3)" not in summary  # That shop is after both first landmarks.
    assert len(decision.view()["content"]["candidates"]) == 3  # Cancel is retained too.


def test_flight_target_does_not_expand_unavailable_future_flight_edges(tmp_path):
    decision = reading(tmp_path, "flight")
    detail = decision.view("candidates", candidate_ordinals=[0])["content"][0]
    assert detail["action"]["flight"] is True
    assert "(6,1) Elite → (6,2) Shop" in detail["label"]
    assert "(0,1)" not in detail["label"]
    assert decision.view()["content"]["candidates"][1]["action"]["flight"] is False


def test_unexpanded_endpoints_and_invalid_topology_remain_visible(tmp_path):
    detail = render_decision(
        reading(tmp_path, "unknown_end").view("candidates", candidate_ordinals=[0])
    )
    assert "(3,16) Unknown" in detail
    cyclic = reading(tmp_path, "cycle")
    assert "unknown" in render_decision(cyclic.view())
    assert len(cyclic.view()["content"]["candidates"]) == 1
    # The complete public map remains inspectable even when its summary fails.
    assert len(cyclic.view("map")["content"]["nodes"]) == 2


@pytest.mark.parametrize(
    "change",
    [
        lambda graph: graph["nodes"][0]["edges"].reverse(),
        lambda graph: graph["nodes"][1].update(room="elite"),
        lambda graph: graph["nodes"][1].update(has_emerald_key=True),
        lambda graph: graph["nodes"][0]["edges"].pop(),
    ],
)
def test_route_description_is_bound_to_full_public_topology(tmp_path, change):
    def mutate(payload):
        change(payload["decision"]["observation"]["observation"]["public_map"])

    with pytest.raises(ValueError, match="observation"):
        reading(tmp_path, "entrance", mutate)


def test_cli_expands_only_the_requested_route_without_a_package(tmp_path, capsys):
    from tools.expert import cli

    decision = reading(tmp_path, "entrance")
    assert (
        cli.main(
            [
                "--repository",
                str(tmp_path),
                "curated",
                "--decision",
                str(decision.path),
                "--view",
                "candidates",
                "--candidate",
                "1",
                "--json",
            ]
        )
        == 0
    )
    result = json.loads(capsys.readouterr().out)
    assert result["knowledge"] is None
    rows = result["decision"]["content"]
    assert [row["ordinal"] for row in rows] == [1]
    assert "(3,1) Shop" in rows[0]["label"]
    assert "(1,0)" not in rows[0]["label"]
