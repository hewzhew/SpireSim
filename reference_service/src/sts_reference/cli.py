from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .models import (
    CitationLocator,
    CitationRecord,
    DecisionResearchResponse,
    DecisionResearchRequest,
    DiagnosticResponse,
    EntityEvidence,
    EntityResearchResponse,
    EntityResearchRequest,
    EvidenceLane,
    GameplayEntityRef,
)
from .research import ResearchService


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="sts-reference")
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    subparsers = parser.add_subparsers(dest="command", required=True)

    entity = subparsers.add_parser("entity", help="research one exact gameplay entity")
    entity.add_argument("--kind", required=True)
    entity.add_argument("--id", required=True)
    entity.add_argument("--display-name")
    entity.add_argument(
        "--json", action="store_true", help="print complete machine-readable evidence"
    )

    decision = subparsers.add_parser(
        "decision", help="research one complete typed decision JSON request"
    )
    decision.add_argument("request", type=Path)
    decision.add_argument(
        "--json", action="store_true", help="print complete machine-readable evidence"
    )

    citation = subparsers.add_parser(
        "citation", help="resolve one CitationLocator JSON object"
    )
    citation.add_argument("locator", type=Path)
    citation.add_argument(
        "--json", action="store_true", help="print complete machine-readable citation"
    )

    diagnose = subparsers.add_parser("diagnose", help="inspect per-lane coverage")
    diagnose.add_argument("--kind", required=True)
    diagnose.add_argument("--id", required=True)
    diagnose.add_argument("--display-name")
    diagnose.add_argument(
        "--json", action="store_true", help="print complete machine-readable diagnostics"
    )

    return parser


def _read_json(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def _one_line(value: object, limit: int) -> str:
    text = " ".join(str(value or "").split())
    return text if len(text) <= limit else text[: limit - 1].rstrip() + "…"


def _lane_error(lane: EvidenceLane) -> str:
    error = lane.evidence.get("error") or lane.evidence.get("message")
    if not error:
        entities = lane.evidence.get("entities", [])
        if isinstance(entities, list):
            error = next(
                (
                    row.get("error")
                    for row in entities
                    if isinstance(row, dict) and row.get("error")
                ),
                None,
            )
    return str(error or "unknown error")


def _lane_lines(
    label: str, lane: EvidenceLane, *, max_rows: int, width: int
) -> list[str]:
    rows = lane.evidence.get(
        "strategy_passages" if lane.evidence_class == "wiki_strategy" else "matches", []
    )
    if not isinstance(rows, list):
        rows = []
    lines = [f"{label}: {lane.status}" + (f" ({len(rows)})" if rows else "")]
    if lane.status == "error":
        lines.append(f"  {_one_line(_lane_error(lane), width)}")
    for row in rows[:max_rows]:
        if not isinstance(row, dict):
            continue
        source = _one_line(row.get("source_id") or "unknown source", 80)
        location = row.get("record") if isinstance(row.get("record"), dict) else {}
        place = (
            location.get("locator")
            or row.get("section_title")
            or row.get("page_title")
            or row.get("title")
        )
        origin = f"{source} / {_one_line(place, 80)}" if place else source
        passage = row.get("text") or row.get("snippet") or ""
        note = (
            " [source excerpt shortened]"
            if row.get("line_truncated") or row.get("truncated")
            else ""
        )
        lines.append(f"  {origin}: {_one_line(passage, width)}{note}")
    hidden = len(rows) - max_rows
    if hidden > 0:
        lines.append(f"  … {hidden} more match(es); use --json for all evidence")
    elif lane.truncated:
        lines.append("  Source results truncated; use --json for details")
    return lines


def _entity_lines(evidence: EntityEvidence, *, decision: bool = False) -> list[str]:
    lines = [evidence.entity.entity_key]
    if evidence.entity.simulator_facts:
        lines.append(f"Simulator facts: {_one_line(evidence.entity.simulator_facts, 240)}")
    # Expert reading comes first in the interactive view. The complete typed lanes
    # and citation locators remain unchanged in --json output.
    lines.extend(
        _lane_lines(
            "Expert", evidence.expert_strategy, max_rows=1 if decision else 2, width=520
        )
    )
    lines.extend(
        _lane_lines("Wiki strategy", evidence.wiki_strategy, max_rows=1, width=360)
    )
    lines.extend(
        _lane_lines(
            "Wiki reference", evidence.wiki_reference,
            max_rows=2 if decision else 3, width=280,
        )
    )
    return lines


def _display(command: str, result: object) -> str:
    if command == "entity":
        assert isinstance(result, EntityResearchResponse)
        lines = _entity_lines(result.result)
    elif command == "decision":
        assert isinstance(result, DecisionResearchResponse)
        entities = {row.entity.entity_key: row for row in result.candidate_entities}
        lines = [
            f"Decision: {result.decision_site} ({len(result.candidate_coverage)} candidates)"
        ]
        shown: set[str] = set()
        for candidate in result.candidate_coverage:
            lines.append(f"\n{_one_line(candidate.label or candidate.candidate_id, 120)}")
            if not candidate.entity_keys:
                lines.append(f"  {_one_line(candidate.entityless_reason, 240)}")
            for key in candidate.entity_keys:
                if key in entities:
                    if key in shown:
                        lines.append(f"{key} (shown above)")
                    else:
                        lines.extend(_entity_lines(entities[key], decision=True))
                        shown.add(key)
        if result.contextual_entities:
            lines.append("\nContextual entities:")
            for evidence in result.contextual_entities:
                lines.extend(_entity_lines(evidence, decision=True))
    elif command == "citation":
        assert isinstance(result, CitationRecord)
        lines = [f"{result.entity_key} | {result.evidence_class} | {result.source_id}"]
        if result.title:
            lines.append(result.title)
        if result.url:
            lines.append(result.url)
        lines.append(result.text)
    else:
        assert isinstance(result, DiagnosticResponse)
        lines = [result.entity.entity_key]
        for lane, status in result.lane_statuses.items():
            lines.append(f"{lane}: {status}")
            detail = result.details.get(lane, {})
            error = detail.get("error") or detail.get("message")
            if error:
                lines.append(f"  {_one_line(error, 280)}")
    if command != "citation":
        lines.append("Complete evidence: repeat with --json")
    return "\n".join(lines)


def main() -> int:
    arguments = _parser().parse_args()
    service = ResearchService(arguments.repository)
    try:
        if arguments.command == "entity":
            result = service.research_entity(
                EntityResearchRequest(
                    entity=GameplayEntityRef(
                        kind=arguments.kind,
                        stable_id=arguments.id,
                        display_name=arguments.display_name,
                    ),
                )
            )
        elif arguments.command == "decision":
            result = service.research_decision(
                DecisionResearchRequest.model_validate(_read_json(arguments.request))
            )
        elif arguments.command == "citation":
            result = service.citation(
                CitationLocator.model_validate(_read_json(arguments.locator))
            )
        else:
            result = service.diagnose(
                GameplayEntityRef(
                    kind=arguments.kind,
                    stable_id=arguments.id,
                    display_name=arguments.display_name,
                ),
            )
        if arguments.json:
            print(result.model_dump_json(by_alias=True, indent=2))
        else:
            print(_display(arguments.command, result))
        return 0
    except (OSError, ValueError, KeyError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
