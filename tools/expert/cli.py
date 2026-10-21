from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from typing import Any

from .query import ExpertKnowledgeOwner


def _print_json(value: Any) -> None:
    print(json.dumps(value, ensure_ascii=False, indent=2))


def _render_asset_refs(value: dict[str, Any]) -> None:
    for asset in value.get("asset_refs", []):
        relation = asset.get("relation", "source asset")
        alt = f" · {asset['alt']}" if asset.get("alt") else ""
        print(f"    asset: {asset.get('path')} · {relation}{alt}")


def _render_entity(result: dict[str, Any]) -> None:
    entity = result["entity"]
    print(f"Entity: {entity['id']} ({entity.get('canonical_name')})")
    print(f"Aliases: {', '.join(entity.get('aliases', []))}")
    groups = (
        ("DIRECT REVIEWED PASSAGES", "direct_reviewed_passages"),
        ("DIRECT SOURCE-LABELED ELEMENTS", "direct_native_elements"),
        (
            "DIRECT SOURCE MEDIA TRANSCRIPTIONS",
            "direct_source_media_transcriptions",
        ),
        ("DIRECT UNREVIEWED LITERAL MATCHES", "direct_unreviewed_literal_matches"),
        ("RELATED REVIEWED PASSAGES", "related_reviewed_passages"),
        ("DIRECT REVIEWED CLAIMS", "direct_reviewed_claims"),
        ("RELATED REVIEWED CLAIMS", "related_reviewed_claims"),
    )
    for label, field in groups:
        values = result[field]
        print(f"\n{label} ({len(values)})")
        for value in values:
            if field.endswith("claims"):
                print(f"  - {value['id']}: {value.get('statement', '')}")
                caveats = value.get("caveats", [])
                if caveats:
                    print(f"    caveats: {'; '.join(caveats)}")
            elif field == "direct_native_elements":
                label = value.get("authored_label") or {}
                heading = " > ".join(value.get("heading_path", []))
                print(
                    f"  - {value.get('source_id')} · {value.get('path')}:"
                    f"{value.get('line_number')} · {heading}"
                )
                print(
                    f"    authored label: {label.get('text', '[missing]')} · "
                    f"entity_binding={label.get('resolution', 'unknown')} · "
                    f"topic_binding={label.get('topic_resolution', 'unknown')}"
                )
                print(f"    {value.get('excerpt', '')}")
                _render_asset_refs(value)
            elif field == "direct_source_media_transcriptions":
                observation = value["observation"]
                transcription = value["transcription"]
                scope = value["scope"]
                print(
                    f"  - {value['source_id']} · {value['asset']['path']} · "
                    f"tier row {observation['tier_label']}"
                )
                print(
                    f"    {observation['source_label']} appears in the source tier row "
                    f"{observation['tier_label']!r}."
                )
                print(
                    f"    {transcription['method']} · {transcription['review_state']} · "
                    f"coverage={scope['coverage']} · image_complete="
                    f"{str(scope['image_complete']).lower()}"
                )
                print(
                    f"    annotation: {value['path']}:{value['line_number']} · "
                    f"sha256={value['asset']['sha256']}"
                )
            else:
                source_id = value.get("source_id")
                record = value.get("record") or {}
                record_id = value.get("record_id") or record.get("id")
                locator = value.get("locator") or record.get("locator")
                print(f"  - {source_id} · {locator} · record {record_id}")
                print(f"    {value.get('excerpt', '')}")
                _render_asset_refs(value)
    print(f"\nBoundary: {result['boundary']}.")


def _render_coverage(result: dict[str, Any]) -> None:
    print(
        "Expert library: "
        f"entity_catalog={result.get('entity_catalog_status', 'reference_only')} "
        f"simulator_entities={result.get('simulator_entity_count', 0)} "
        f"topics={result['topic_count']} entities={result['entity_count']} "
        f"claims={result['claim_count']} passages={result['passage_count']} "
        f"native_elements={result.get('native_element_count', 0)} "
        f"media_annotations={result.get('media_annotation_count', 0)} "
        f"source_ids={result.get('source_identifier_count', 0)}"
    )
    for item in result.get("entity_evidence_coverage", []):
        print(
            f"  {item['kind']}: direct={item['direct_evidence_entity_count']}/"
            f"{item['entity_count']} substantive="
            f"{item['substantive_evidence_entity_count']} reviewed="
            f"{item['reviewed_passage_or_claim_entity_count']} native_label="
            f"{item['native_source_labeled_entity_count']} media_transcribed="
            f"{item.get('source_media_transcription_entity_count', 0)} outline_only="
            f"{item['outline_only_entity_count']} uncovered="
            f"{item['uncovered_entity_count']}"
        )
    for source in result["sources"]:
        if source["coverage_status"] == "catalog_only":
            print(
                f"  - {source['source_id']}: catalog_only "
                f"({source.get('catalog_status')})"
            )
            continue
        print(
            f"  - {source['source_id']}: {source['coverage_status']} "
            f"records={source['covered_record_count']}/{source['record_count']} "
            f"entity_index={source['entity_index_record_count']} "
            f"outline={source['outline_record_count']} "
            f"media_annotations={source.get('media_annotation_count', 0)}"
        )
        if source["uncovered_record_ids"]:
            print(f"    uncovered: {', '.join(source['uncovered_record_ids'])}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="query the local expert reference owner"
    )
    parser.add_argument("--repository", type=Path, default=Path("."))
    commands = parser.add_subparsers(dest="command", required=True)

    search = commands.add_parser("search", help="literal attributed expert search")
    search.add_argument("--query", required=True)
    search.add_argument("--source")
    search.add_argument("--max-results", type=int, default=40)

    entity = commands.add_parser("entity", help="structured entity evidence")
    entity.add_argument("--name", required=True)
    entity.add_argument("--source")
    entity.add_argument("--json", action="store_true")

    topics = commands.add_parser("list-topics", help="list structured expert topics")
    topics.add_argument("--json", action="store_true")
    entities = commands.add_parser("list-entities", help="list structured entities")
    entities.add_argument("--kind")
    entities.add_argument("--json", action="store_true")
    outline = commands.add_parser("outline", help="show one source's reviewed outline")
    outline.add_argument("--source", required=True)
    outline.add_argument("--topic")
    outline.add_argument("--json", action="store_true")
    coverage = commands.add_parser("coverage", help="show structured source coverage")
    coverage.add_argument("--json", action="store_true")
    commands.add_parser("list-sources", help="list cataloged expert sources")
    show = commands.add_parser("show-source", help="show one expert source")
    show.add_argument("--source", required=True)
    curated = commands.add_parser(
        "curated", help="read curated strategy by exact entry ID, without a question"
    )
    curated.add_argument(
        "--package",
        type=Path,
        help="explicit knowledge package; the card-reward package is the default only for card rewards or standalone knowledge reading",
    )
    curated.add_argument("--entry", action="append", default=[])
    curated.add_argument(
        "--decision",
        type=Path,
        help="detached typed public export from run-play show --public-output",
    )
    curated.add_argument(
        "--view",
        choices=(
            "overview",
            "deck",
            "map",
            "relics",
            "history",
            "candidates",
            "mechanics",
            "context",
        ),
    )
    curated.add_argument(
        "--candidate",
        action="append",
        type=int,
        help="reading ordinal for --view candidates or mechanics; repeat to select several",
    )
    curated.add_argument("--card", action="append", type=int,
                         help="public deck ordinal for --view mechanics; repeat to compare owned cards")
    curated.add_argument("--relic", action="append", type=int,
                         help="owned relic ordinal for --view mechanics; repeat to compare owned relics")
    curated.add_argument(
        "--detail", choices=("brief", "reasoning", "provenance"), default="brief"
    )
    curated.add_argument("--json", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    arguments = build_parser().parse_args(argv)
    try:
        if arguments.command == "curated":
            from .curated import CuratedStrategy, render_curated

            package_path = arguments.repository / (
                arguments.package
                or Path("reference/expert/curated/ironclad-card-reward.json")
            )
            if arguments.view and not arguments.decision:
                raise ValueError("--view requires a public --decision export")
            if any(selection is not None for selection in (arguments.candidate, arguments.card, arguments.relic)):
                if not arguments.decision or not arguments.view:
                    raise ValueError("reading selectors require --decision and an explicit --view")
            if arguments.decision:
                from .decision import DecisionReading, render_decision

                decision = DecisionReading(arguments.repository / arguments.decision)
                knowledge = (
                    decision.knowledge(
                        CuratedStrategy(package_path),
                        arguments.entry,
                        detail=arguments.detail,
                    )
                    if arguments.entry
                    or (
                        arguments.view is None
                        and (
                            arguments.package is not None
                            or decision.site == "card_reward"
                        )
                    )
                    else None
                )
                view = (
                    decision.view(
                        arguments.view or "overview",
                        candidate_ordinals=arguments.candidate,
                        card_ordinals=arguments.card,
                        relic_ordinals=arguments.relic,
                    )
                    if arguments.view or not arguments.entry
                    else None
                )
                result = {
                    "schema": "sts-curated-decision-reading-v1",
                    "export_revision": decision.revision,
                    "decision": view,
                    "knowledge": knowledge,
                }
                if arguments.json:
                    _print_json(result)
                else:
                    if view:
                        print(render_decision(view))
                    if knowledge is not None:
                        print(render_curated(knowledge))
                return 0
            package = CuratedStrategy(package_path)
            result = (
                package.read(arguments.entry, detail=arguments.detail)
                if arguments.entry
                else package.catalog()
            )
            if arguments.json:
                _print_json(result)
            else:
                print(render_curated(result))
            return 0
        owner = ExpertKnowledgeOwner(arguments.repository)
        if arguments.command == "search":
            result = owner.literal_search(
                arguments.query,
                source_id=arguments.source,
                max_results=arguments.max_results,
            )
            for match in result["matches"]:
                print(f"{match['path']}:{match['line_number']}:{match['text']}")
            if result["truncated"]:
                print(
                    "Showing the first "
                    f"{result['returned_match_count']} of {result['total_match_count']} "
                    "literal matches. Increase -MaxResults for a wider result set.",
                    file=sys.stderr,
                )
            return 0
        if arguments.command == "entity":
            result = owner.entity_evidence(arguments.name, source_id=arguments.source)
            if arguments.json:
                _print_json(result)
            else:
                _render_entity(result)
            return 0
        if arguments.command == "list-topics":
            values = owner.list_topics()
            if arguments.json:
                _print_json(values)
            else:
                for value in values:
                    print(
                        f"{value['id']}\t{value['canonical_name']}\t"
                        f"{', '.join(value.get('aliases', []))}"
                    )
            return 0
        if arguments.command == "list-entities":
            values = owner.list_entities(kind=arguments.kind)
            if arguments.json:
                _print_json(values)
            else:
                for value in values:
                    print(
                        f"{value['id']}\t{value['canonical_name']}\t"
                        f"{', '.join(value.get('aliases', []))}"
                    )
            return 0
        if arguments.command == "outline":
            values = owner.source_outline(arguments.source, topic_id=arguments.topic)
            if arguments.json:
                _print_json(values)
            else:
                for value in values:
                    print(
                        f"{value['record'].get('locator')}\t{value['kind']}\t"
                        f"{value['title']}\t{value['summary']}"
                    )
            return 0
        if arguments.command == "coverage":
            result = owner.coverage()
            if arguments.json:
                _print_json(result)
            else:
                _render_coverage(result)
            return 0
        if arguments.command == "list-sources":
            _print_json(owner.list_sources())
            return 0
        if arguments.command == "show-source":
            _print_json(owner.show_source(arguments.source))
            return 0
    except (OSError, ValueError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
