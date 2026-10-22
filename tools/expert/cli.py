from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from typing import Any

from tools.presentation import Presentation


def _print_json(value: Any) -> None:
    print(json.dumps(value, ensure_ascii=False, indent=2))


def _render_asset_refs(value: dict[str, Any], view: Presentation) -> None:
    for asset in value.get("asset_refs", []):
        relation = asset.get("relation", view.text("expert", "cli.sourceAsset"))
        alt = f" | {asset['alt']}" if asset.get("alt") else ""
        print(view.text("expert", "cli.asset", path=asset.get("path"), relation=relation, alt=alt))


def _render_entity(result: dict[str, Any], *, locale="en") -> None:
    view = Presentation(locale)
    text = lambda key, **values: view.text("expert", "cli." + key, **values)
    entity = result["entity"]
    print(text("entityTitle", id=entity["id"], name=entity.get("canonical_name")))
    print(text("aliases", aliases=", ".join(entity.get("aliases", []))))
    for field in ("direct_reviewed_passages", "direct_native_elements", "direct_source_media_transcriptions",
                  "direct_unreviewed_literal_matches", "related_reviewed_passages", "direct_reviewed_claims", "related_reviewed_claims"):
        values = result[field]
        print(f"\n{text(field)} ({len(values)})")
        for value in values:
            if field.endswith("claims"):
                print(f"  - {value['id']}: {value.get('statement', '')}")
                caveats = value.get("caveats", [])
                if caveats:
                    print(text("caveats", value=view.text("expert", "separator").join(caveats)))
            elif field == "direct_native_elements":
                label = value.get("authored_label") or {}
                heading = " > ".join(value.get("heading_path", []))
                print(f"  - {value.get('source_id')} | {value.get('path')}:{value.get('line_number')} | {heading}")
                print(text("authoredLabel", label=label.get("text", "[missing]"), entity=label.get("resolution", "unknown"), topic=label.get("topic_resolution", "unknown")))
                print(f"    {value.get('excerpt', '')}")
                _render_asset_refs(value, view)
            elif field == "direct_source_media_transcriptions":
                observation, transcription, scope = value["observation"], value["transcription"], value["scope"]
                print(text("tierRow", source=value["source_id"], path=value["asset"]["path"], tier=observation["tier_label"]))
                print(text("tierObservation", label=observation["source_label"], tier=repr(observation["tier_label"])))
                print(f"    {transcription['method']} | {transcription['review_state']} | coverage={scope['coverage']} | image_complete={str(scope['image_complete']).lower()}")
                print(text("annotation", path=value["path"], line=value["line_number"], sha=value["asset"]["sha256"]))
            else:
                record = value.get("record") or {}
                print(text("record", source=value.get("source_id"), locator=value.get("locator") or record.get("locator"), id=value.get("record_id") or record.get("id")))
                print(f"    {value.get('excerpt', '')}")
                _render_asset_refs(value, view)
    print("\n" + text("boundary", value=result["boundary"]))


def _render_coverage(result: dict[str, Any], *, locale="en") -> None:
    view = Presentation(locale)
    print(
        view.text("expert", "cli.library")
        + f"entity_catalog={result.get('entity_catalog_status', 'reference_only')} "
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
            print(view.text("expert", "cli.uncovered", ids=", ".join(source["uncovered_record_ids"])))


def build_parser(locale="zh-CN") -> argparse.ArgumentParser:
    view = Presentation(locale)
    text = lambda key: view.text("expert", "cli." + key)

    class LocalizedHelpFormatter(argparse.HelpFormatter):
        def add_usage(self, usage, actions, groups, prefix=None):
            return super().add_usage(usage, actions, groups, prefix or text("usage"))

    parser = argparse.ArgumentParser(
        description=text("description"), formatter_class=LocalizedHelpFormatter
    )
    parser.add_argument("--locale", choices=("en", "zh-CN"), default="zh-CN", help=text("locale"))
    parser.add_argument("--repository", type=Path, default=Path("."))
    commands = parser.add_subparsers(dest="command", required=True, title=text("commands"))

    search = commands.add_parser("search", help=text("search"))
    search.add_argument("--query", required=True)
    search.add_argument("--source")
    search.add_argument("--max-results", type=int, default=40)

    entity = commands.add_parser("entity", help=text("entity"))
    entity.add_argument("--name", required=True)
    entity.add_argument("--source")
    entity.add_argument("--json", action="store_true")

    topics = commands.add_parser("list-topics", help=text("list-topics"))
    topics.add_argument("--json", action="store_true")
    entities = commands.add_parser("list-entities", help=text("list-entities"))
    entities.add_argument("--kind")
    entities.add_argument("--json", action="store_true")
    outline = commands.add_parser("outline", help=text("outline"))
    outline.add_argument("--source", required=True)
    outline.add_argument("--topic")
    outline.add_argument("--json", action="store_true")
    coverage = commands.add_parser("coverage", help=text("coverage"))
    coverage.add_argument("--json", action="store_true")
    commands.add_parser("list-sources", help=text("list-sources"))
    show = commands.add_parser("show-source", help=text("show-source"))
    show.add_argument("--source", required=True)
    curated = commands.add_parser(
        "curated", help=text("curated")
    )
    curated.add_argument(
        "--package",
        type=Path,
        help=text("package"),
    )
    curated.add_argument("--entry", action="append", default=[])
    curated.add_argument(
        "--decision",
        type=Path,
        help=text("decision"),
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
        help=text("candidate"),
    )
    curated.add_argument("--card", action="append", type=int,
                         help=text("card"))
    curated.add_argument("--relic", action="append", type=int,
                         help=text("relic"))
    curated.add_argument(
        "--detail", choices=("brief", "reasoning", "provenance"), default="brief"
    )
    curated.add_argument("--json", action="store_true")
    for child in commands.choices.values():
        child.add_argument("--locale", choices=("en", "zh-CN"), default=argparse.SUPPRESS, help=text("locale"))
    for target in [parser, *commands.choices.values()]:
        target.formatter_class = LocalizedHelpFormatter
        target._positionals.title = text("positionals")
        target._optionals.title = text("options")
        next(action for action in target._actions if action.dest == "help").help = text("help")
    return parser


def main(argv: list[str] | None = None) -> int:
    language_parser = argparse.ArgumentParser(add_help=False)
    language_parser.add_argument("--locale", choices=("en", "zh-CN"), default="zh-CN")
    language, _ = language_parser.parse_known_args(argv)
    arguments = build_parser(language.locale).parse_args(argv)
    text = lambda key, **values: Presentation(arguments.locale).text("expert", "cli." + key, **values)
    try:
        if arguments.command == "curated":
            from .curated import CuratedStrategy, render_curated

            package_path = arguments.repository / (
                arguments.package
                or Path("reference/expert/curated/ironclad-card-reward.json")
            )
            if arguments.view and not arguments.decision:
                raise ValueError(text("requiresDecision"))
            if any(selection is not None for selection in (arguments.candidate, arguments.card, arguments.relic)):
                if not arguments.decision or not arguments.view:
                    raise ValueError(text("requiresView"))
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
                        print(render_decision(view, locale=arguments.locale))
                    if knowledge is not None:
                        print(render_curated(knowledge, locale=arguments.locale))
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
                print(render_curated(result, locale=arguments.locale))
            return 0
        from .query import ExpertKnowledgeOwner

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
                    text("searchTruncated", returned=result["returned_match_count"], total=result["total_match_count"]),
                    file=sys.stderr,
                )
            return 0
        if arguments.command == "entity":
            result = owner.entity_evidence(arguments.name, source_id=arguments.source, locale=arguments.locale)
            if arguments.json:
                _print_json(result)
            else:
                _render_entity(result, locale=arguments.locale)
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
                _render_coverage(result, locale=arguments.locale)
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
