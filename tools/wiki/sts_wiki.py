"""Maintained CLI for MediaWiki acquisition, verification, and corpus work."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from .entity_graph import rebuild_entity_graph
from .entity_query import (
    candidate_audit,
    compact_entity_results,
    entity_graph_inventory,
    mechanics_scope_inventory,
    query_conflicts,
    query_entities,
    query_entity_sections,
    query_identity_reviews,
    query_issues,
    query_unmatched,
    search_entity_pages,
    search_bound_entity_sections,
    search_entity_mechanics,
)
from .simulator_catalog import simulator_catalog_summary
from .mediawiki_direct import (
    DirectMediaWikiClient,
    MediaWikiApiError,
    acquire_export,
    probe_summary,
)
from .mediawiki_records import BundleError, iter_pages, verify_bundle
from .wiki_corpus import (
    index_inventory,
    install_browser_bundle,
    list_pages,
    normalize_export,
    read_data_table,
    read_page,
    read_page_sections,
    rebuild_index,
    search_index,
    structured_data_inventory,
)


DEFAULT_LIBRARY_ROOT = Path(".oracle-lab/external-research/sources")
DEFAULT_USER_AGENT = "sts-simulator-wiki-research/0.1 (local research corpus)"
DEFAULT_WIKI_DATABASE = Path(".oracle-lab/external-research/wiki.sqlite3")
DEFAULT_ENTITY_DATABASE = Path(".oracle-lab/external-research/entities.sqlite3")
DEFAULT_ENTITY_REVIEW_FILE = Path("reference/wiki/entity_identity_reviews.json")


def _namespace_ids(value: str) -> list[int]:
    try:
        return sorted({int(item) for item in value.split(",") if item.strip()})
    except ValueError as error:
        raise argparse.ArgumentTypeError("namespaces must be comma-separated integers") from error


def _client(arguments: argparse.Namespace) -> DirectMediaWikiClient:
    return DirectMediaWikiClient(
        api_url=arguments.api_url,
        user_agent=arguments.user_agent,
        delay_seconds=arguments.delay_ms / 1000.0,
        timeout_seconds=arguments.timeout_seconds,
        max_retries=arguments.max_retries,
    )


def command_probe(arguments: argparse.Namespace) -> int:
    print(json.dumps(probe_summary(_client(arguments), arguments.api_url), ensure_ascii=False, indent=2))
    return 0


def command_crawl(arguments: argparse.Namespace) -> int:
    source_root = arguments.library_root / arguments.source_id
    bundle = acquire_export(
        transport=_client(arguments),
        source_id=arguments.source_id,
        api_url=arguments.api_url,
        source_root=source_root,
        namespace_ids=arguments.namespaces,
        capture_name=arguments.capture_name,
        resume=arguments.resume,
        max_pages=arguments.max_pages,
    )
    verified = verify_bundle(bundle)
    print(
        json.dumps(
            {
                "bundle": str(bundle.resolve()),
                "page_count": verified.page_count,
                "namespace_counts": verified.namespace_counts,
                "acquisition_warning_pages": verified.acquisition_warning_pages,
                "acquisition_error_pages": verified.acquisition_error_pages,
                "sha256": verified.sha256,
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_verify(arguments: argparse.Namespace) -> int:
    verified = verify_bundle(arguments.bundle)
    print(
        json.dumps(
            {
                "bundle": str(arguments.bundle.resolve()),
                "source_id": verified.header["source_id"],
                "page_count": verified.page_count,
                "namespace_counts": verified.namespace_counts,
                "acquisition_warning_pages": verified.acquisition_warning_pages,
                "acquisition_error_pages": verified.acquisition_error_pages,
                "sha256": verified.sha256,
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_issues(arguments: argparse.Namespace) -> int:
    verified = verify_bundle(arguments.bundle)
    pages = []
    for page in iter_pages(arguments.bundle):
        warnings = page.get("acquisition_warnings", [])
        errors = page.get("acquisition_errors", [])
        if not warnings and not errors:
            continue
        pages.append(
            {
                "page_id": page["pageid"],
                "namespace_id": page["ns"],
                "title": page["title"],
                "warnings": warnings,
                "errors": errors,
            }
        )
    print(
        json.dumps(
            {
                "source_id": verified.header["source_id"],
                "warning_pages": verified.acquisition_warning_pages,
                "error_pages": verified.acquisition_error_pages,
                "pages": pages[: arguments.limit],
                "truncated": len(pages) > arguments.limit,
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_import_browser(arguments: argparse.Namespace) -> int:
    installed = install_browser_bundle(
        bundle=arguments.bundle,
        library_root=arguments.library_root,
        capture_name=arguments.capture_name,
    )
    print(json.dumps({"installed_bundle": str(installed.resolve())}, ensure_ascii=False, indent=2))
    return 0


def command_normalize(arguments: argparse.Namespace) -> int:
    output = normalize_export(
        bundle=arguments.bundle,
        source_root=arguments.source_root,
        replace=arguments.replace,
    )
    print(json.dumps({"normalized_pages": str(output.resolve())}, ensure_ascii=False, indent=2))
    return 0


def command_rebuild_index(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            rebuild_index(library_root=arguments.library_root, database=arguments.database),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_search(arguments: argparse.Namespace) -> int:
    results = search_index(
        database=arguments.database,
        query=arguments.query,
        source_id=arguments.source_id,
        language=arguments.language,
        namespace_id=arguments.namespace,
        content_only=arguments.content_only,
        raw_query=arguments.raw_query,
        limit=arguments.limit,
    )
    print(json.dumps(results, ensure_ascii=False, indent=2))
    return 0


def command_inventory(arguments: argparse.Namespace) -> int:
    inventory = index_inventory(database=arguments.database)
    if arguments.source_id:
        inventory = [item for item in inventory if item["source_id"] == arguments.source_id]
    print(
        json.dumps(
            inventory,
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_structure(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            structured_data_inventory(
                database=arguments.database,
                source_id=arguments.source_id,
                namespace_id=arguments.namespace,
                issue_limit=arguments.issue_limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_list_pages(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            list_pages(
                database=arguments.database,
                source_id=arguments.source_id,
                namespace_id=arguments.namespace,
                limit=arguments.limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_page(arguments: argparse.Namespace) -> int:
    page = read_page(
        database=arguments.database,
        source_id=arguments.source_id,
        title=arguments.title,
        metadata_only=arguments.metadata_only,
    )
    if page is None:
        print("error: page was not found", file=sys.stderr)
        return 1
    print(json.dumps(page, ensure_ascii=False, indent=2))
    return 0


def command_page_sections(arguments: argparse.Namespace) -> int:
    page = read_page_sections(
        database=arguments.database,
        source_id=arguments.source_id,
        title=arguments.title,
    )
    if page is None:
        print("error: page was not found", file=sys.stderr)
        return 1
    print(json.dumps(page, ensure_ascii=False, indent=2))
    return 0


def command_table(arguments: argparse.Namespace) -> int:
    columns = [column.strip() for column in arguments.columns.split(",") if column.strip()]
    print(
        json.dumps(
            read_data_table(
                database=arguments.database,
                source_id=arguments.source_id,
                title=arguments.title,
                columns=columns or None,
                offset=arguments.offset,
                limit=arguments.limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_simulator_entities(arguments: argparse.Namespace) -> int:
    print(json.dumps(simulator_catalog_summary(arguments.repository), ensure_ascii=False, indent=2))
    return 0


def command_rebuild_entities(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            rebuild_entity_graph(
                repository=arguments.repository,
                wiki_database=arguments.wiki_database,
                output_database=arguments.database,
                review_file=arguments.reviews,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_inventory(arguments: argparse.Namespace) -> int:
    print(json.dumps(entity_graph_inventory(arguments.database), ensure_ascii=False, indent=2))
    return 0


def command_mechanics_scope_inventory(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            mechanics_scope_inventory(
                entity_database=arguments.entity_database,
                wiki_database=arguments.wiki_database,
                kind=arguments.kind,
                example_limit=arguments.example_limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity(arguments: argparse.Namespace) -> int:
    results = query_entities(
        database=arguments.database,
        kind=arguments.kind,
        stable_id=arguments.stable_id,
        query=arguments.query,
    )
    if not arguments.full_evidence:
        results = compact_entity_results(results)
    print(
        json.dumps(
            results,
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_page_search(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            search_entity_pages(
                entity_database=arguments.entity_database,
                wiki_database=arguments.wiki_database,
                kind=arguments.kind,
                stable_id=arguments.stable_id,
                query=arguments.query,
                include_related=arguments.include_related,
                raw_query=arguments.raw_query,
                limit=arguments.limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_mechanics_search(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            search_entity_mechanics(
                entity_database=arguments.entity_database,
                wiki_database=arguments.wiki_database,
                kind=arguments.kind,
                stable_id=arguments.stable_id,
                query=arguments.query,
                include_related=arguments.include_related,
                raw_query=arguments.raw_query,
                limit=arguments.limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_sections(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            query_entity_sections(
                entity_database=arguments.entity_database,
                wiki_database=arguments.wiki_database,
                kind=arguments.kind,
                stable_id=arguments.stable_id,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_section_search(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            search_bound_entity_sections(
                entity_database=arguments.entity_database,
                wiki_database=arguments.wiki_database,
                kind=arguments.kind,
                stable_id=arguments.stable_id,
                query=arguments.query,
                raw_query=arguments.raw_query,
                limit=arguments.limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_unmatched(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            query_unmatched(
                database=arguments.database,
                kind=arguments.kind,
                side=arguments.side,
                candidate_state=arguments.candidate_state,
                limit=arguments.limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_conflicts(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            query_conflicts(database=arguments.database, limit=arguments.limit),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_issues(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            query_issues(database=arguments.database, limit=arguments.limit),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_reviews(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            query_identity_reviews(
                database=arguments.database,
                decision=arguments.decision,
                limit=arguments.limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def command_entity_candidate_audit(arguments: argparse.Namespace) -> int:
    print(
        json.dumps(
            candidate_audit(
                database=arguments.database,
                kind=arguments.kind,
                details=arguments.details,
                limit=arguments.limit,
            ),
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


def _add_direct_options(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--api-url", required=True)
    parser.add_argument("--user-agent", default=DEFAULT_USER_AGENT)
    parser.add_argument("--delay-ms", type=int, default=600)
    parser.add_argument("--timeout-seconds", type=float, default=45.0)
    parser.add_argument("--max-retries", type=int, default=6)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Acquire and verify complete, provenance-preserving MediaWiki text exports."
    )
    commands = parser.add_subparsers(dest="command", required=True)

    probe = commands.add_parser("probe", help="read source siteinfo without writing a capture")
    _add_direct_options(probe)
    probe.set_defaults(handler=command_probe)

    crawl = commands.add_parser("crawl", help="create or resume a direct API capture")
    _add_direct_options(crawl)
    crawl.add_argument("--source-id", required=True)
    crawl.add_argument("--library-root", type=Path, default=DEFAULT_LIBRARY_ROOT)
    crawl.add_argument("--capture-name")
    crawl.add_argument("--namespaces", type=_namespace_ids)
    crawl.add_argument("--resume", action="store_true")
    crawl.add_argument(
        "--max-pages",
        type=int,
        help="diagnostic early stop; leaves a resumable partial capture",
    )
    crawl.set_defaults(handler=command_crawl)

    verify = commands.add_parser("verify", help="verify one completed export bundle")
    verify.add_argument("--bundle", type=Path, required=True)
    verify.set_defaults(handler=command_verify)

    issues = commands.add_parser(
        "issues", help="list declared acquisition warnings and errors in a verified bundle"
    )
    issues.add_argument("--bundle", type=Path, required=True)
    issues.add_argument("--limit", type=int, default=100)
    issues.set_defaults(handler=command_issues)

    import_browser = commands.add_parser(
        "import-browser", help="verify and install one browser-exported bundle"
    )
    import_browser.add_argument("--bundle", type=Path, required=True)
    import_browser.add_argument("--library-root", type=Path, default=DEFAULT_LIBRARY_ROOT)
    import_browser.add_argument("--capture-name", required=True)
    import_browser.set_defaults(handler=command_import_browser)

    normalize = commands.add_parser("normalize", help="build one canonical normalized corpus")
    normalize.add_argument("--bundle", type=Path, required=True)
    normalize.add_argument("--source-root", type=Path, required=True)
    normalize.add_argument("--replace", action="store_true")
    normalize.set_defaults(handler=command_normalize)

    rebuild = commands.add_parser("rebuild-index", help="atomically rebuild the cross-source FTS index")
    rebuild.add_argument("--library-root", type=Path, default=DEFAULT_LIBRARY_ROOT)
    rebuild.add_argument(
        "--database",
        type=Path,
        default=Path(".oracle-lab/external-research/wiki.sqlite3"),
    )
    rebuild.set_defaults(handler=command_rebuild_index)

    search = commands.add_parser("search", help="query the provenance-preserving Wiki FTS index")
    search.add_argument("query")
    search.add_argument(
        "--database",
        type=Path,
        default=Path(".oracle-lab/external-research/wiki.sqlite3"),
    )
    search.add_argument("--source-id")
    search.add_argument("--language")
    search.add_argument("--namespace", type=int)
    search.add_argument("--content-only", action="store_true")
    search.add_argument("--raw-query", action="store_true")
    search.add_argument("--limit", type=int, default=20)
    search.set_defaults(handler=command_search)

    inventory = commands.add_parser("inventory", help="list installed sources and namespace counts")
    inventory.add_argument(
        "--database",
        type=Path,
        default=Path(".oracle-lab/external-research/wiki.sqlite3"),
    )
    inventory.add_argument("--source-id")
    inventory.set_defaults(handler=command_inventory)

    structure = commands.add_parser(
        "structure", help="summarize machine-readable Wiki Data pages without dumping bodies"
    )
    structure.add_argument("--source-id", required=True)
    structure.add_argument("--namespace", type=int, default=3500)
    structure.add_argument("--issue-limit", type=int, default=10)
    structure.add_argument(
        "--database",
        type=Path,
        default=Path(".oracle-lab/external-research/wiki.sqlite3"),
    )
    structure.set_defaults(handler=command_structure)

    pages = commands.add_parser("list-pages", help="list exact page identities without ad-hoc SQL")
    pages.add_argument("--source-id", required=True)
    pages.add_argument("--namespace", type=int)
    pages.add_argument("--limit", type=int, default=100)
    pages.add_argument(
        "--database",
        type=Path,
        default=Path(".oracle-lab/external-research/wiki.sqlite3"),
    )
    pages.set_defaults(handler=command_list_pages)

    page = commands.add_parser("page", help="read one exact normalized Wiki page")
    page.add_argument("--source-id", required=True)
    page.add_argument("--title", required=True)
    page.add_argument(
        "--metadata-only",
        action="store_true",
        help="omit page bodies and internal links while retaining identity metadata",
    )
    page.add_argument(
        "--database",
        type=Path,
        default=Path(".oracle-lab/external-research/wiki.sqlite3"),
    )
    page.set_defaults(handler=command_page)

    page_sections = commands.add_parser(
        "page-sections", help="read the explicit wikitext heading outline without page bodies"
    )
    page_sections.add_argument("--source-id", required=True)
    page_sections.add_argument("--title", required=True)
    page_sections.add_argument(
        "--database", type=Path, default=DEFAULT_WIKI_DATABASE
    )
    page_sections.set_defaults(handler=command_page_sections)

    table = commands.add_parser("table", help="read selected rows from one JSON tabular page")
    table.add_argument("--source-id", required=True)
    table.add_argument("--title", required=True)
    table.add_argument("--columns", default="")
    table.add_argument("--offset", type=int, default=0)
    table.add_argument("--limit", type=int, default=20)
    table.add_argument(
        "--database",
        type=Path,
        default=Path(".oracle-lab/external-research/wiki.sqlite3"),
    )
    table.set_defaults(handler=command_table)

    simulator_entities = commands.add_parser(
        "simulator-entities", help="audit stable simulator identities without compiling Rust"
    )
    simulator_entities.add_argument("--repository", type=Path, default=Path("."))
    simulator_entities.set_defaults(handler=command_simulator_entities)

    rebuild_entities = commands.add_parser(
        "rebuild-entities", help="atomically rebuild the auditable simulator-to-Wiki graph"
    )
    rebuild_entities.add_argument("--repository", type=Path, default=Path("."))
    rebuild_entities.add_argument("--wiki-database", type=Path, default=DEFAULT_WIKI_DATABASE)
    rebuild_entities.add_argument("--database", type=Path, default=DEFAULT_ENTITY_DATABASE)
    rebuild_entities.add_argument("--reviews", type=Path, default=DEFAULT_ENTITY_REVIEW_FILE)
    rebuild_entities.set_defaults(handler=command_rebuild_entities)

    entity_inventory = commands.add_parser(
        "entity-inventory", help="report entity graph coverage and unresolved counts"
    )
    entity_inventory.add_argument("--database", type=Path, default=DEFAULT_ENTITY_DATABASE)
    entity_inventory.set_defaults(handler=command_entity_inventory)

    mechanics_inventory = commands.add_parser(
        "mechanics-scope-inventory",
        help="summarize safe page/section mechanics coverage without page bodies",
    )
    mechanics_inventory.add_argument(
        "--kind", choices=["card", "potion", "relic", "enemy", "encounter", "event"]
    )
    mechanics_inventory.add_argument("--example-limit", type=int, default=20)
    mechanics_inventory.add_argument(
        "--entity-database", type=Path, default=DEFAULT_ENTITY_DATABASE
    )
    mechanics_inventory.add_argument(
        "--wiki-database", type=Path, default=DEFAULT_WIKI_DATABASE
    )
    mechanics_inventory.set_defaults(handler=command_mechanics_scope_inventory)

    entity = commands.add_parser("entity", help="query one simulator entity and its source edges")
    entity.add_argument("--kind", choices=["card", "potion", "relic", "enemy", "encounter", "event"])
    identity = entity.add_mutually_exclusive_group(required=True)
    identity.add_argument("--id", dest="stable_id")
    identity.add_argument("--query")
    entity.add_argument("--database", type=Path, default=DEFAULT_ENTITY_DATABASE)
    entity.add_argument("--full-evidence", action="store_true")
    entity.set_defaults(handler=command_entity)

    entity_page_search = commands.add_parser(
        "entity-page-search",
        help="search accepted pages without section-level attribution",
    )
    entity_page_search.add_argument("query")
    entity_page_search.add_argument(
        "--kind",
        required=True,
        choices=["card", "potion", "relic", "enemy", "encounter", "event"],
    )
    entity_page_search.add_argument("--id", dest="stable_id", required=True)
    entity_page_search.add_argument(
        "--include-related",
        action="store_true",
        help="for encounters, also search accepted pages of related enemy members",
    )
    entity_page_search.add_argument("--raw-query", action="store_true")
    entity_page_search.add_argument("--limit", type=int, default=20)
    entity_page_search.add_argument(
        "--entity-database", type=Path, default=DEFAULT_ENTITY_DATABASE
    )
    entity_page_search.add_argument(
        "--wiki-database", type=Path, default=DEFAULT_WIKI_DATABASE
    )
    entity_page_search.set_defaults(handler=command_entity_page_search)

    entity_search = commands.add_parser(
        "entity-search",
        help="search mechanics attributable to exact entity page or section scopes",
    )
    entity_search.add_argument("query")
    entity_search.add_argument(
        "--kind",
        required=True,
        choices=["card", "potion", "relic", "enemy", "encounter", "event"],
    )
    entity_search.add_argument("--id", dest="stable_id", required=True)
    entity_search.add_argument(
        "--include-related",
        action="store_true",
        help="for encounters, also search attributable initial-member scopes",
    )
    entity_search.add_argument("--raw-query", action="store_true")
    entity_search.add_argument("--limit", type=int, default=20)
    entity_search.add_argument(
        "--entity-database", type=Path, default=DEFAULT_ENTITY_DATABASE
    )
    entity_search.add_argument(
        "--wiki-database", type=Path, default=DEFAULT_WIKI_DATABASE
    )
    entity_search.set_defaults(handler=command_entity_mechanics_search)

    entity_sections = commands.add_parser(
        "entity-sections",
        help="audit exact explicit-heading bindings for one simulator entity",
    )
    entity_sections.add_argument(
        "--kind",
        required=True,
        choices=["card", "potion", "relic", "enemy", "encounter", "event"],
    )
    entity_sections.add_argument("--id", dest="stable_id", required=True)
    entity_sections.add_argument(
        "--entity-database", type=Path, default=DEFAULT_ENTITY_DATABASE
    )
    entity_sections.add_argument(
        "--wiki-database", type=Path, default=DEFAULT_WIKI_DATABASE
    )
    entity_sections.set_defaults(handler=command_entity_sections)

    entity_section_search = commands.add_parser(
        "entity-section-search",
        help="search only uniquely bound explicit sections for one simulator entity",
    )
    entity_section_search.add_argument("query")
    entity_section_search.add_argument(
        "--kind",
        required=True,
        choices=["card", "potion", "relic", "enemy", "encounter", "event"],
    )
    entity_section_search.add_argument("--id", dest="stable_id", required=True)
    entity_section_search.add_argument("--raw-query", action="store_true")
    entity_section_search.add_argument("--limit", type=int, default=20)
    entity_section_search.add_argument(
        "--entity-database", type=Path, default=DEFAULT_ENTITY_DATABASE
    )
    entity_section_search.add_argument(
        "--wiki-database", type=Path, default=DEFAULT_WIKI_DATABASE
    )
    entity_section_search.set_defaults(handler=command_entity_section_search)

    unmatched = commands.add_parser(
        "entity-unmatched", help="list first-class unmatched simulator entities or typed Wiki pages"
    )
    unmatched.add_argument("--side", choices=["simulator", "wiki"], default="simulator")
    unmatched.add_argument("--kind", choices=["card", "potion", "relic", "enemy", "encounter", "event"])
    unmatched.add_argument("--candidate-state", choices=["any", "some", "none"], default="any")
    unmatched.add_argument("--limit", type=int, default=50)
    unmatched.add_argument("--database", type=Path, default=DEFAULT_ENTITY_DATABASE)
    unmatched.set_defaults(handler=command_entity_unmatched)

    conflicts = commands.add_parser("entity-conflicts", help="list graph conflicts requiring audit")
    conflicts.add_argument("--limit", type=int, default=50)
    conflicts.add_argument("--database", type=Path, default=DEFAULT_ENTITY_DATABASE)
    conflicts.set_defaults(handler=command_entity_conflicts)

    issues = commands.add_parser("entity-issues", help="list non-conflicting source coverage issues")
    issues.add_argument("--limit", type=int, default=50)
    issues.add_argument("--database", type=Path, default=DEFAULT_ENTITY_DATABASE)
    issues.set_defaults(handler=command_entity_issues)

    reviews = commands.add_parser(
        "entity-reviews", help="list revision-bound entity identity review decisions"
    )
    reviews.add_argument("--decision", choices=["accept", "reject", "defer"])
    reviews.add_argument("--limit", type=int, default=50)
    reviews.add_argument("--database", type=Path, default=DEFAULT_ENTITY_DATABASE)
    reviews.set_defaults(handler=command_entity_reviews)

    candidate_audit_parser = commands.add_parser(
        "entity-candidate-audit",
        help="audit normalized candidates by distinct structured record without accepting them",
    )
    candidate_audit_parser.add_argument(
        "--kind", choices=["card", "potion", "relic", "enemy", "encounter", "event"]
    )
    candidate_audit_parser.add_argument("--details", action="store_true")
    candidate_audit_parser.add_argument("--limit", type=int, default=50)
    candidate_audit_parser.add_argument("--database", type=Path, default=DEFAULT_ENTITY_DATABASE)
    candidate_audit_parser.set_defaults(handler=command_entity_candidate_audit)
    return parser


def main() -> int:
    arguments = build_parser().parse_args()
    try:
        return arguments.handler(arguments)
    except (BundleError, MediaWikiApiError, FileExistsError, OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
