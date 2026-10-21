"""Run-planning synthesis, versioned scope, and offline strategic integration."""

from collections import defaultdict
from copy import deepcopy
import json
from pathlib import Path

import pytest

from tools.expert.curated import CuratedStrategy, _digest, render_curated
from tools.expert.decision import DecisionReading

REPOSITORY = Path(__file__).parents[3]
DIRECTORY = REPOSITORY / "reference/expert/curated"
PACKAGE = DIRECTORY / "ironclad-run-planning.json"
FIXTURES = json.loads(
    (Path(__file__).parent / "fixtures/public-strategic-sites.json").read_bytes()
)["cases"]
FIXTURES["card_reward"] = json.loads(
    (Path(__file__).parent / "fixtures/public-card-reward.json").read_bytes()
)


def package_copy(tmp_path, change=None):
    data = json.loads(PACKAGE.read_bytes())
    if change:
        change(data)
    path = tmp_path / "package.json"
    path.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    return path


def decision_copy(tmp_path, name):
    path = tmp_path / "decision.json"
    path.write_text(json.dumps(FIXTURES[name], ensure_ascii=False), encoding="utf-8")
    return DecisionReading(path)


@pytest.mark.parametrize("name", FIXTURES)
def test_declared_sites_can_consult_one_package_without_corpus_or_game(tmp_path, name):
    package = CuratedStrategy(package_copy(tmp_path))
    reading = decision_copy(tmp_path, name)
    catalog = reading.knowledge(package, [])
    assert reading.site in catalog["scope"]["decision_sites"]
    assert package.supports_decision("Ironclad", reading.site)
    result = reading.knowledge(package, ["route-options", "hp-recovery"])
    assert result["applicability"] == "unassessed"
    assert [row["id"] for row in result["readings"]] == ["route-options", "hp-recovery"]
    assert not (tmp_path / ".oracle-lab").exists()


@pytest.mark.parametrize(
    "sites",
    [
        [],
        "map shop",
        ["map|shop"],
        ["route"],
        ["combat"],
        ["map", "map"],
        [None],
        [["map"]],
    ],
)
def test_scope_rejects_ambiguous_unknown_or_combat_sites(tmp_path, sites):
    path = package_copy(
        tmp_path, lambda data: data["scope"].update(decision_sites=sites)
    )
    with pytest.raises(ValueError, match="scope"):
        CuratedStrategy(path)


@pytest.mark.parametrize("character", ["ironclad", "Ironclad|Silent", "all", "Unknown"])
def test_v2_scope_uses_explicit_character_identity(tmp_path, character):
    path = package_copy(
        tmp_path, lambda data: data["scope"].update(character=character)
    )
    with pytest.raises(ValueError, match="scope"):
        CuratedStrategy(path)


def test_single_and_multiple_site_schemas_do_not_mix(tmp_path):
    path = package_copy(
        tmp_path, lambda data: data["scope"].update(decision_site="map")
    )
    with pytest.raises(ValueError, match="scope"):
        CuratedStrategy(path)
    data = json.loads(path.read_bytes())
    data["schema"] = "sts-curated-strategy-v1"
    path.write_text(json.dumps(data), encoding="utf-8")
    with pytest.raises(ValueError, match="scope"):
        CuratedStrategy(path)
    legacy = CuratedStrategy(DIRECTORY / "ironclad-card-reward.json")
    assert legacy.supports_decision("Ironclad", "card_reward")
    assert not legacy.supports_decision("Ironclad", "map")
    assert "decision_site" in legacy.catalog()["scope"]
    assert "decision_sites" not in legacy.catalog()["scope"]


def test_scope_change_invalidates_receipt_and_does_not_certify_new_context(tmp_path):
    path = package_copy(tmp_path)
    package = CuratedStrategy(path)
    first = decision_copy(tmp_path, "map").knowledge(package, ["potion-reserve"])[
        "readings"
    ][0]
    known = {first["id"]: first["revision"]}
    # An explicit retained-content receipt may be used at another declared site;
    # only the text is unchanged, never its applicability or the current action.
    result = decision_copy(tmp_path, "reward").knowledge(
        package, ["potion-reserve"], known=known
    )
    assert result["readings"][0]["status"] == "unchanged"
    assert result["applicability"] == "unassessed"
    assert package.read(["potion-reserve"])["readings"][0]["status"] == "content"
    data = json.loads(path.read_bytes())
    data["scope"]["decision_sites"].remove("map")
    path.write_text(json.dumps(data), encoding="utf-8")
    assert package.supports_decision("Ironclad", "reward")
    assert (
        package.read(["potion-reserve"], known=known)["readings"][0]["status"]
        == "content"
    )
    with pytest.raises(ValueError, match="scope"):
        decision_copy(tmp_path, "map").knowledge(package, [])


def test_foreign_character_cannot_use_package_even_with_retained_receipt(tmp_path):
    package = CuratedStrategy(package_copy(tmp_path))
    old = package.read(["route-options"])["readings"][0]
    document = deepcopy(FIXTURES["card_reward"])
    document["payload"]["decision"]["observation"]["observation"]["run"][
        "player_class"
    ] = "silent"
    document["payload_sha256"] = _digest(document["payload"])
    path = tmp_path / "silent.json"
    path.write_text(json.dumps(document), encoding="utf-8")
    with pytest.raises(ValueError, match="scope"):
        DecisionReading(path).knowledge(
            package, ["route-options"], known={old["id"]: old["revision"]}
        )
    assert not package.supports_decision(None, "map")


def test_contrast_cases_keep_conditional_reading_within_budget():
    document = json.loads((Path(__file__).parent / "fixtures/run-planning-cases.json").read_bytes())
    package = CuratedStrategy(PACKAGE)
    pairs = defaultdict(list)
    covered = set()
    for case in document["cases"]:
        assert package.supports_decision("Ironclad", case["decision_site"])
        result = package.read(case["reading_ids"])
        assert len(render_curated(result)) < 1200, case["id"]
        assert case["unknowns"] and case["analyst_hypothesis"] and case["application"]
        assert result["applicability"] == "unassessed"
        for reading in result["readings"]:
            content = reading["content"]
            assert content["check"] and content["unless"] and content["disagreements"]
            assert "sources" not in content
        covered.update(case["reading_ids"])
        if pair := case.get("pair"):
            pairs[pair].append(case)
    assert covered == {entry["id"] for entry in package.catalog()["entries"]}
    for cases in pairs.values():
        assert len(cases) == 2
        a, b = cases
        assert a["candidates"] == b["candidates"]
        assert a["public_context"] != b["public_context"]
        assert a["application"] != b["application"]


def test_missing_corpus_does_not_block_curated_reading(tmp_path):
    package = CuratedStrategy(package_copy(tmp_path))
    result = package.read(["replan-after-relic"], detail="provenance")
    content = result["readings"][0]["content"]
    assert {row["kind"] for row in content["inputs"]} == {"observable", "judgment"}
    assert content["review"]["gameplay_validation"] == "not_performed"
    assert content["review"]["human_review"] == "not_recorded"
    assert all(source["curator_scope_note"] for source in content["sources"].values())


def test_cli_explicit_package_is_selective_and_never_constructs_query_owner(
    tmp_path, monkeypatch, capsys
):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("natural-query owner must not be loaded")

    monkeypatch.setattr(cli, "ExpertKnowledgeOwner", forbidden)
    package = package_copy(tmp_path)
    decision = decision_copy(tmp_path, "map")
    before = decision.path.read_bytes()
    assert (
        cli.main(
            [
                "--repository",
                str(tmp_path),
                "curated",
                "--package",
                str(package),
                "--decision",
                str(decision.path),
                "--entry",
                "route-options",
                "--json",
            ]
        )
        == 0
    )
    output = json.loads(capsys.readouterr().out)
    assert (
        output["decision"] is None
    )  # A knowledge-only request does not repeat the map.
    assert [row["id"] for row in output["knowledge"]["readings"]] == ["route-options"]
    assert decision.path.read_bytes() == before
    assert (
        cli.main(
            [
                "--repository",
                str(tmp_path),
                "curated",
                "--decision",
                str(decision.path),
                "--json",
            ]
        )
        == 0
    )
    output = json.loads(capsys.readouterr().out)
    assert output["knowledge"] is None  # No automatic per-site package loading.
