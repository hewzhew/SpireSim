import json
import sys
from pathlib import Path

from sts_reference.cli import main
from sts_reference.models import EntityResearchRequest, GameplayEntityRef
from sts_reference.research import ResearchService


REPOSITORY = Path(__file__).parents[2]


def run_cli(monkeypatch, capsys, *args: str) -> str:
    monkeypatch.setattr(
        sys, "argv", ["sts-reference", "--repository", str(REPOSITORY), *args]
    )
    assert main() == 0
    return capsys.readouterr().out


def test_entity_default_reads_passages_without_dumping_evidence(monkeypatch, capsys) -> None:
    output = run_cli(monkeypatch, capsys, "entity", "--kind", "card", "--id", "Pummel")

    assert "card:Pummel" in output
    assert "连续拳 定位：力量收益卡" in output
    assert "tieba-10072740344 / floor 74" in output
    assert "Wiki reference: matched (3)" in output
    assert "Complete evidence: repeat with --json" in output
    assert "revision_sha1" not in output
    assert len(output.splitlines()) < 20


def test_entity_json_preserves_complete_typed_result(monkeypatch, capsys) -> None:
    output = run_cli(
        monkeypatch, capsys, "entity", "--kind", "card", "--id", "Pummel", "--json"
    )
    result = json.loads(output)

    assert result["schema"] == "sts-reference-entity-response-v2"
    assert result["result"]["expert_strategy"]["citations"]
    assert result["result"]["wiki_reference"]["evidence"]["matches"][0]["revision_sha1"]


def test_missing_entity_shows_source_error(monkeypatch, capsys) -> None:
    output = run_cli(
        monkeypatch, capsys, "entity", "--kind", "card", "--id", "NoSuchCard"
    )

    assert "Wiki reference: error" in output
    assert "simulator entity was not found: card:NoSuchCard" in output
    assert "unknown error" not in output


def test_decision_default_keeps_candidates_and_compact_readings(
    monkeypatch, capsys, tmp_path: Path
) -> None:
    request = {
        "schema": "sts-reference-decision-request-v3",
        "decision_site": "card_reward",
        "public_context": {},
        "candidates": [
            {
                "candidate_id": "pummel",
                "typed_candidate": {"kind": "take_card", "card": "Pummel"},
                "entities": [{"kind": "card", "stable_id": "Pummel"}],
            },
            {
                "candidate_id": "skip",
                "typed_candidate": {"kind": "skip_card_reward"},
                "entities": [],
                "entityless_reason": "no card taken",
            },
        ],
    }
    path = tmp_path / "decision.json"
    path.write_text(json.dumps(request), encoding="utf-8")

    output = run_cli(monkeypatch, capsys, "decision", str(path))

    assert "Decision: card_reward (2 candidates)" in output
    assert "card:Pummel" in output
    assert "连续拳 定位：力量收益卡" in output
    assert "skip\n  no card taken" in output
    assert len(output.splitlines()) < 25


def test_citation_default_shows_full_requested_passage(
    monkeypatch, capsys, tmp_path: Path
) -> None:
    service = ResearchService(REPOSITORY)
    locator = service.research_entity(
        EntityResearchRequest(
            entity=GameplayEntityRef(kind="card", stable_id="Pummel")
        )
    ).result.expert_strategy.citations[0]
    path = tmp_path / "citation.json"
    path.write_text(locator.model_dump_json(), encoding="utf-8")

    output = run_cli(monkeypatch, capsys, "citation", str(path))

    assert "card:Pummel | expert_strategy | tieba-10072740344" in output
    assert service.citation(locator).text in output
    assert "citation_id" not in output
