import json
from pathlib import Path

import pytest

from tools.expert.curated import CuratedStrategy, render_curated

REPOSITORY = Path(__file__).parents[3]
SAMPLE = REPOSITORY / "reference/expert/curated/ironclad-card-reward.json"


def write_sample(tmp_path, edit=None):
    data = json.loads(SAMPLE.read_text(encoding="utf-8"))
    if edit:
        edit(data)
    path = tmp_path / "sample.json"
    path.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    return path


def test_read_requires_only_curated_package_and_retains_limits(tmp_path):
    package = CuratedStrategy(write_sample(tmp_path))
    result = package.read(["future-value", "skip-density"])
    assert result["applicability"] == "unassessed"
    for row in result["readings"]:
        content = row["content"]
        assert content["check"] and content["unless"] and content["disagreements"]
        assert "sources" not in content and "assertions" not in content
    assert "跳过" in render_curated(result)
    assert len(render_curated(result)) < 900
    assert "当前局面适用性待判断" not in render_curated(result)
    assert "来源取舍" not in render_curated(result)
    assert not (tmp_path / ".oracle-lab").exists()


def test_exact_ids_do_not_parse_questions_or_card_names():
    package = CuratedStrategy(SAMPLE)
    for ids in (
        ["黑暗之拥该不该拿"],
        ["DarkEmbrace"],
        ["future-value", "future-value"],
        [],
    ):
        with pytest.raises(ValueError):
            package.read(ids)


def test_expansion_separates_observations_judgments_and_source_scope():
    result = CuratedStrategy(SAMPLE).read(["exhaust-startup"], detail="provenance")
    row = result["readings"][0]["content"]
    assert {item["kind"] for item in row["inputs"]} == {"observable", "judgment"}
    assert row["reasoning"] and row["assertions"]
    assert row["review"]["human_review"] == "not_recorded"
    assert row["review"]["gameplay_validation"] == "not_performed"
    for assertion in row["assertions"].values():
        assert assertion["record_id"] and assertion["locator"]
        assert row["sources"][assertion["source_id"]]["curator_scope_note"]
    assert "来源取舍：" in render_curated(result)


def test_repeated_reading_is_suppressed_only_with_explicit_matching_receipt(tmp_path):
    path = write_sample(tmp_path)
    package = CuratedStrategy(path)
    first = package.read(["future-value"])["readings"][0]
    known = {first["id"]: first["revision"]}
    assert (
        package.read(["future-value"], known=known)["readings"][0]["status"]
        == "unchanged"
    )
    assert package.read(["future-value"])["readings"][0]["status"] == "content"
    assert (
        package.read(["future-value"], detail="reasoning", known=known)["readings"][0][
            "status"
        ]
        == "content"
    )

    data = json.loads(path.read_text(encoding="utf-8"))
    data["sources"]["tieba-10072740344"]["curator_scope_note"] += " 新发现的适用限制。"
    path.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    assert (
        package.read(["future-value"], known=known)["readings"][0][
            "status"
        ]
        == "content"
    )


@pytest.mark.parametrize(
    "field,value",
    [("unless", ""), ("assertions", ["missing"]), ("kind", "simulator_fact")],
)
def test_missing_conditions_broken_provenance_and_evidence_promotion_are_rejected(
    tmp_path, field, value
):
    path = write_sample(
        tmp_path, lambda data: data["entries"][0].update({field: value})
    )
    with pytest.raises(ValueError):
        CuratedStrategy(path)


def test_duplicate_json_keys_are_rejected(tmp_path):
    path = tmp_path / "duplicate.json"
    path.write_text('{"schema":"one","schema":"two"}', encoding="utf-8")
    with pytest.raises(ValueError, match="duplicate"):
        CuratedStrategy(path)


def test_constructed_cases_preserve_caveats_with_a_small_reading_budget():
    cases = json.loads(
        (Path(__file__).parent / "fixtures/card-reward-cases.json").read_text(encoding="utf-8")
    )
    package = CuratedStrategy(SAMPLE)
    for case in cases["cases"]:
        reading = package.read(case["reading_ids"])
        assert len(render_curated(reading)) < 1200, case["id"]
        assert reading["applicability"] == "unassessed"
        assert all(
            row["content"]["unless"] and row["content"]["disagreements"]
            for row in reading["readings"]
        )


def test_cli_curated_read_never_constructs_natural_query_owner(
    tmp_path, monkeypatch, capsys
):
    from tools.expert import cli

    def forbidden(*args, **kwargs):
        raise AssertionError("the old query owner must not be loaded")

    monkeypatch.setattr("tools.expert.query.ExpertKnowledgeOwner", forbidden)
    path = write_sample(tmp_path)
    assert (
        cli.main(
            [
                "--repository",
                str(tmp_path),
                "curated",
                "--package",
                str(path),
                "--entry",
                "energy",
            ]
        )
        == 0
    )
    output = capsys.readouterr().out
    assert "费用与可用手牌一起看" in output and "改判：" in output
