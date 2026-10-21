"""Read-only queries over a rebuilt simulator-to-Wiki entity graph."""

from __future__ import annotations

import json
from pathlib import Path
import sqlite3
from typing import Any
import unicodedata

from .entity_reviews import (
    IDENTITY_REVIEW_REPORT_SCHEMA,
    record_key as identity_record_key,
    structured_record,
)
from .mediawiki_records import file_sha256
from .wiki_corpus import (
    read_page_sections,
    search_index,
    search_scope_references,
    search_section_references,
)

TITLE_CANDIDATE_EVIDENCE = "normalized_title_candidate"
CANDIDATE_AUDIT_SCHEMA = "sts-wiki-entity-candidate-audit-v1"
ENTITY_SEARCH_SCHEMA = "sts-wiki-entity-search-v1"
ENTITY_SECTION_BINDINGS_SCHEMA = "sts-wiki-entity-section-bindings-v1"
ENTITY_SECTION_SEARCH_SCHEMA = "sts-wiki-entity-section-search-v1"
ENTITY_MECHANICS_SEARCH_SCHEMA = "sts-wiki-entity-mechanics-search-v1"
MECHANICS_SCOPE_INVENTORY_SCHEMA = "sts-wiki-mechanics-scope-inventory-v1"
ENCOUNTER_MEMBER_RELATION = "encounter_initial_possible_member"
SECTION_IDENTITY_FIELDS = {"NAME", "name", "name_en", "name_zhs", "english_title"}


def _read_only(database: Path) -> sqlite3.Connection:
    connection = sqlite3.connect(f"file:{database.resolve().as_posix()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    return connection


def _accepted_entity_keys_by_page(
    database: Path, page_references: list[tuple[str, int]] | None
) -> dict[tuple[str, int], list[str]]:
    if page_references == []:
        return {}
    page_clause = ""
    parameters: list[Any] = []
    if page_references is not None:
        clauses = " OR ".join(
            "(x.source_id=? AND x.page_id=?)" for _ in page_references
        )
        page_clause = f" AND ({clauses})"
        parameters = [value for reference in page_references for value in reference]
    connection = _read_only(database)
    try:
        rows = connection.execute(
            f"""
            SELECT DISTINCT x.source_id,x.page_id,x.entity_key
            FROM edges x
            WHERE x.status='accepted'{page_clause}
            ORDER BY x.source_id,x.page_id,x.entity_key
            """,
            parameters,
        )

        accepted: dict[tuple[str, int], list[str]] = {}
        for row in rows:
            accepted.setdefault((row["source_id"], row["page_id"]), []).append(
                row["entity_key"]
            )
        return accepted
    finally:
        connection.close()


def _require_matching_wiki_database(
    entity_database: Path, wiki_database: Path
) -> None:
    connection = _read_only(entity_database)
    try:
        wiki_hash_row = connection.execute(
            "SELECT value FROM metadata WHERE key='wiki_database_sha256'"
        ).fetchone()
    finally:
        connection.close()
    if wiki_hash_row is None:
        raise ValueError(
            "entity graph does not record its Wiki database identity; "
            "run `.\\wiki.ps1 rebuild-entities`"
        )
    if file_sha256(wiki_database) != wiki_hash_row[0]:
        raise ValueError(
            "entity graph was built from a different Wiki database; "
            "run `.\\wiki.ps1 rebuild-entities`"
        )


def _section_identity_key(value: str) -> str:
    return " ".join(unicodedata.normalize("NFKC", value).split()).casefold()


def entity_graph_inventory(database: Path) -> dict[str, Any]:
    connection = _read_only(database)
    try:
        schema = connection.execute("SELECT value FROM metadata WHERE key='schema'").fetchone()[0]
        kinds = []
        for row in connection.execute(
            """
            SELECT e.kind,COUNT(*) AS entities,
                   SUM(CASE WHEN u.entity_key IS NULL THEN 1 ELSE 0 END) AS accepted,
                   SUM(CASE WHEN u.entity_key IS NOT NULL THEN 1 ELSE 0 END) AS unmatched
            FROM simulator_entities e
            LEFT JOIN unmatched_simulator_entities u ON u.entity_key=e.entity_key
            GROUP BY e.kind ORDER BY e.kind
            """
        ):
            item = dict(row)
            item["candidate_only_entities"] = connection.execute(
                """
                SELECT COUNT(DISTINCT e.entity_key)
                FROM unmatched_simulator_entities u
                JOIN simulator_entities e ON e.entity_key=u.entity_key
                JOIN edges x ON x.entity_key=e.entity_key
                WHERE e.kind=? AND x.status='candidate'
                """,
                (item["kind"],),
            ).fetchone()[0]
            kinds.append(item)
        return {
            "schema": schema,
            "kinds": kinds,
            "simulator_relations": [
                dict(row)
                for row in connection.execute(
                    """
                    SELECT relation_type,COUNT(*) AS relation_count,
                           COUNT(DISTINCT source_entity_key) AS source_entity_count,
                           COUNT(DISTINCT target_entity_key) AS target_entity_count
                    FROM simulator_relations
                    GROUP BY relation_type ORDER BY relation_type
                    """
                )
            ],
            "accepted_edges": connection.execute(
                "SELECT COUNT(*) FROM edges WHERE status='accepted'"
            ).fetchone()[0],
            "candidate_edges": connection.execute(
                "SELECT COUNT(*) FROM edges WHERE status='candidate'"
            ).fetchone()[0],
            "conflicts": connection.execute("SELECT COUNT(*) FROM conflicts").fetchone()[0],
            "issues": connection.execute("SELECT COUNT(*) FROM issues").fetchone()[0],
            "unmatched_typed_wiki_pages": connection.execute(
                "SELECT COUNT(*) FROM unmatched_wiki_pages"
            ).fetchone()[0],
        }
    finally:
        connection.close()


def query_entities(
    *, database: Path, kind: str | None, stable_id: str | None, query: str | None
) -> list[dict[str, Any]]:
    connection = _read_only(database)
    try:
        clauses: list[str] = []
        parameters: list[Any] = []
        if kind:
            clauses.append("e.kind=?")
            parameters.append(kind)
        if stable_id:
            clauses.append("e.stable_id=?")
            parameters.append(stable_id)
        if query:
            clauses.append(
                "(e.entity_key=? OR e.stable_id=? OR e.display_name=? OR e.external_id=? "
                "OR EXISTS (SELECT 1 FROM json_each(e.external_aliases_json) a WHERE a.value=?) "
                "OR EXISTS (SELECT 1 FROM edges x JOIN wiki_pages p "
                "ON p.source_id=x.source_id AND p.page_id=x.page_id "
                "WHERE x.entity_key=e.entity_key AND p.title=?))"
            )
            parameters.extend([query] * 6)
        where = f"WHERE {' AND '.join(clauses)}" if clauses else ""
        entities = [
            dict(row)
            for row in connection.execute(
                f"SELECT e.* FROM simulator_entities e {where} ORDER BY e.kind,e.stable_id",
                parameters,
            )
        ]
        for entity in entities:
            entity["external_aliases"] = json.loads(entity.pop("external_aliases_json"))
        for entity in entities:
            entity["edges"] = [
                {**dict(row), "evidence": json.loads(row["evidence_json"])}
                for row in connection.execute(
                    """
                    SELECT x.status,x.evidence_type,x.evidence_json,p.source_id,p.page_id,p.title,
                           p.url,p.revision_id,p.revision_timestamp,p.revision_sha1
                    FROM edges x JOIN wiki_pages p
                      ON p.source_id=x.source_id AND p.page_id=x.page_id
                    WHERE x.entity_key=?
                    ORDER BY x.status,p.source_id,p.title,x.evidence_type
                    """,
                    (entity["entity_key"],),
                )
            ]
            for edge in entity["edges"]:
                edge.pop("evidence_json", None)
            entity["relations"] = []
            for row in connection.execute(
                """
                SELECT r.relation_type,r.source_path,r.source_line,
                       target.entity_key,target.kind,target.stable_id,
                       target.display_name,target.external_id,target.external_aliases_json
                FROM simulator_relations r
                JOIN simulator_entities target ON target.entity_key=r.target_entity_key
                WHERE r.source_entity_key=?
                ORDER BY r.relation_type,target.kind,target.stable_id
                """,
                (entity["entity_key"],),
            ):
                relation = dict(row)
                relation["target"] = {
                    "entity_key": relation.pop("entity_key"),
                    "kind": relation.pop("kind"),
                    "stable_id": relation.pop("stable_id"),
                    "display_name": relation.pop("display_name"),
                    "external_id": relation.pop("external_id"),
                    "external_aliases": json.loads(
                        relation.pop("external_aliases_json")
                    ),
                }
                relation["target"]["accepted_pages"] = [
                    dict(page)
                    for page in connection.execute(
                        """
                        SELECT DISTINCT p.source_id,p.page_id,p.title,p.url,p.revision_id,
                                        p.revision_timestamp,p.revision_sha1
                        FROM edges x JOIN wiki_pages p
                          ON p.source_id=x.source_id AND p.page_id=x.page_id
                        WHERE x.entity_key=? AND x.status='accepted'
                        ORDER BY p.source_id,p.title
                        """,
                        (relation["target"]["entity_key"],),
                    )
                ]
                entity["relations"].append(relation)
        return entities
    finally:
        connection.close()


def simulator_entity_identity_aliases(
    *,
    database: Path,
    expected_entity_identities: dict[
        str,
        tuple[str, str, str | None, str | None, tuple[str, ...]],
    ],
) -> dict[str, tuple[str, ...]]:
    """Load exact identity aliases without exposing Wiki prose to another owner."""
    connection = _read_only(database)
    try:
        rows = list(
            connection.execute(
                "SELECT entity_key,kind,stable_id,display_name,external_id,"
                "external_aliases_json FROM simulator_entities"
            )
        )
        stored_entity_identities = {
            str(row["entity_key"]): (
                str(row["kind"]),
                str(row["stable_id"]),
                row["display_name"],
                row["external_id"],
                tuple(json.loads(row["external_aliases_json"])),
            )
            for row in rows
        }
        if stored_entity_identities.keys() != expected_entity_identities.keys():
            raise ValueError(
                "Wiki entity graph does not contain the current complete simulator "
                "entity set; rebuild entities.sqlite3"
            )
        changed_identities = sorted(
            entity_key
            for entity_key, identity in expected_entity_identities.items()
            if stored_entity_identities[entity_key] != identity
        )
        if changed_identities:
            preview = ", ".join(changed_identities[:5])
            suffix = "" if len(changed_identities) <= 5 else ", ..."
            raise ValueError(
                "Wiki entity graph simulator identities differ for "
                f"{preview}{suffix}; rebuild entities.sqlite3"
            )
        values: dict[str, list[str]] = {}

        def add(entity_key: str, value: object) -> None:
            if not isinstance(value, str) or not value.strip():
                return
            normalized = value.strip()
            current = values.setdefault(entity_key, [])
            if normalized.casefold() not in {item.casefold() for item in current}:
                current.append(normalized)

        for row in rows:
            entity_key = str(row["entity_key"])
            add(entity_key, row["stable_id"])
            add(entity_key, row["display_name"])
            add(entity_key, row["external_id"])
            for alias in json.loads(row["external_aliases_json"]):
                add(entity_key, alias)
        for row in connection.execute(
            """
            SELECT x.entity_key,p.title,x.evidence_json
            FROM edges x
            JOIN wiki_pages p ON p.source_id=x.source_id AND p.page_id=x.page_id
            WHERE x.status='accepted'
            ORDER BY x.entity_key,p.source_id,p.title,x.evidence_type
            """
        ):
            entity_key = str(row["entity_key"])
            add(entity_key, row["title"])
            evidence = json.loads(row["evidence_json"])
            add(entity_key, evidence.get("matched_value"))
            identity = evidence.get("record_identity", {})
            if isinstance(identity, dict):
                for value in identity.values():
                    add(entity_key, value)
            redirects = evidence.get("redirect_chain", [])
            if isinstance(redirects, list):
                for value in redirects:
                    add(entity_key, value)
        return {entity_key: tuple(aliases) for entity_key, aliases in values.items()}
    finally:
        connection.close()


def compact_entity_results(entities: list[dict[str, Any]]) -> list[dict[str, Any]]:
    compact = []
    for entity in entities:
        item = {key: value for key, value in entity.items() if key != "edges"}
        grouped: dict[tuple[str, int], dict[str, Any]] = {}
        for edge in entity.get("edges", []):
            key = (edge["source_id"], edge["page_id"])
            summary = grouped.setdefault(
                key,
                {
                    "source_id": edge["source_id"],
                    "page_id": edge["page_id"],
                    "title": edge["title"],
                    "url": edge["url"],
                    "revision_id": edge["revision_id"],
                    "revision_timestamp": edge["revision_timestamp"],
                    "revision_sha1": edge["revision_sha1"],
                    "statuses": set(),
                    "evidence_types": set(),
                },
            )
            summary["statuses"].add(edge["status"])
            summary["evidence_types"].add(edge["evidence_type"])
        item["pages"] = []
        for summary in grouped.values():
            summary["statuses"] = sorted(summary["statuses"])
            summary["evidence_types"] = sorted(summary["evidence_types"])
            item["pages"].append(summary)
        compact.append(item)
    return compact


def search_entity_pages(
    *,
    entity_database: Path,
    wiki_database: Path,
    kind: str,
    stable_id: str,
    query: str,
    include_related: bool = False,
    raw_query: bool = False,
    limit: int = 20,
) -> dict[str, Any]:
    """Search only accepted pages owned by one exact simulator entity."""
    if limit < 1:
        raise ValueError("--limit must be at least 1")
    if include_related and kind != "encounter":
        raise ValueError("--include-related is only valid for encounter entities")

    _require_matching_wiki_database(entity_database, wiki_database)

    entities = query_entities(
        database=entity_database,
        kind=kind,
        stable_id=stable_id,
        query=None,
    )
    if not entities:
        raise ValueError(f"simulator entity was not found: {kind}:{stable_id}")
    if len(entities) != 1:
        raise ValueError(f"simulator entity identity is not unique: {kind}:{stable_id}")
    entity = entities[0]

    scoped_pages: dict[tuple[str, int], dict[str, Any]] = {}

    def add_scoped_page(page: dict[str, Any], binding: dict[str, Any]) -> None:
        key = (page["source_id"], page["page_id"])
        scoped_page = scoped_pages.setdefault(
            key,
            {
                **{
                    field: page[field]
                    for field in (
                        "source_id",
                        "page_id",
                        "title",
                        "url",
                        "revision_id",
                        "revision_timestamp",
                        "revision_sha1",
                    )
                },
                "scope_bindings": [],
            },
        )
        if binding not in scoped_page["scope_bindings"]:
            scoped_page["scope_bindings"].append(binding)

    for edge in entity["edges"]:
        if edge["status"] != "accepted":
            continue
        add_scoped_page(
            edge,
            {
                "scope": "direct",
                "entity_key": entity["entity_key"],
                "kind": entity["kind"],
                "stable_id": entity["stable_id"],
                "display_name": entity["display_name"],
                "relation_type": None,
            },
        )

    if include_related:
        for relation in entity["relations"]:
            if relation["relation_type"] != ENCOUNTER_MEMBER_RELATION:
                continue
            target = relation["target"]
            for page in target["accepted_pages"]:
                add_scoped_page(
                    page,
                    {
                        "scope": "related",
                        "entity_key": target["entity_key"],
                        "kind": target["kind"],
                        "stable_id": target["stable_id"],
                        "display_name": target["display_name"],
                        "relation_type": relation["relation_type"],
                    },
                )

    for page in scoped_pages.values():
        page["scope_bindings"].sort(
            key=lambda binding: (
                0 if binding["scope"] == "direct" else 1,
                binding["entity_key"],
                binding["relation_type"] or "",
            )
        )

    accepted_entity_keys = _accepted_entity_keys_by_page(
        entity_database, list(scoped_pages)
    )
    for key, page in scoped_pages.items():
        page["page_accepted_entity_keys"] = accepted_entity_keys.get(key, [])
        page["shared_entity_page"] = len(page["page_accepted_entity_keys"]) > 1

    pages = sorted(
        scoped_pages.values(),
        key=lambda page: (
            0
            if any(binding["scope"] == "direct" for binding in page["scope_bindings"])
            else 1,
            page["source_id"],
            page["title"],
        ),
    )
    matches = search_index(
        database=wiki_database,
        query=query,
        page_references=list(scoped_pages),
        raw_query=raw_query,
        limit=limit,
    )
    for match in matches:
        scoped_page = scoped_pages[(match["source_id"], match["page_id"])]
        match["scope_bindings"] = scoped_page["scope_bindings"]
        match["page_accepted_entity_keys"] = scoped_page[
            "page_accepted_entity_keys"
        ]
        match["shared_entity_page"] = scoped_page["shared_entity_page"]

    return {
        "schema": ENTITY_SEARCH_SCHEMA,
        "entity": {
            field: entity[field]
            for field in ("entity_key", "kind", "stable_id", "display_name")
        },
        "query": {
            "text": query,
            "mode": "raw_fts" if raw_query else "literal_phrase",
            "include_related": include_related,
            "limit": limit,
        },
        "eligible_page_count": len(pages),
        "eligible_pages": pages,
        "match_count": len(matches),
        "matches": matches,
    }


def _entity_section_report(
    *,
    entity: dict[str, Any],
    entity_database: Path,
    wiki_database: Path,
    accepted_entity_keys: dict[tuple[str, int], list[str]] | None = None,
    outline_cache: dict[tuple[str, int, str], dict[str, Any]] | None = None,
    shared_pages_only: bool = False,
) -> dict[str, Any]:
    common_aliases: list[dict[str, Any]] = []

    def add_alias(
        aliases: list[dict[str, Any]], value: Any, source: str, field: str
    ) -> None:
        if not isinstance(value, str) or not value.strip():
            return
        alias = {"value": value.strip(), "source": source, "field": field}
        if alias not in aliases:
            aliases.append(alias)

    add_alias(
        common_aliases,
        entity["display_name"],
        "simulator",
        "display_name",
    )
    add_alias(common_aliases, entity["external_id"], "simulator", "external_id")
    for alias in entity["external_aliases"]:
        add_alias(common_aliases, alias, "simulator", "external_alias")

    pages: dict[tuple[str, int], dict[str, Any]] = {}
    for edge in entity["edges"]:
        if edge["status"] != "accepted":
            continue
        key = (edge["source_id"], edge["page_id"])
        page = pages.setdefault(
            key,
            {
                field: edge[field]
                for field in (
                    "source_id",
                    "page_id",
                    "title",
                    "url",
                    "revision_id",
                    "revision_timestamp",
                    "revision_sha1",
                )
            }
            | {"identity_aliases": list(common_aliases)},
        )
        record_identity = edge["evidence"].get("record_identity")
        if isinstance(record_identity, dict):
            for field in sorted(SECTION_IDENTITY_FIELDS):
                add_alias(
                    page["identity_aliases"],
                    record_identity.get(field),
                    edge["evidence_type"],
                    f"record_identity.{field}",
                )

    if accepted_entity_keys is None:
        accepted_entity_keys = _accepted_entity_keys_by_page(
            entity_database, list(pages)
        )
    if outline_cache is None:
        outline_cache = {}
    results = []
    for key, page in sorted(pages.items()):
        page_entity_keys = accepted_entity_keys.get(key, [])
        if shared_pages_only and page_entity_keys == [entity["entity_key"]]:
            results.append(
                {
                    **page,
                    "page_accepted_entity_keys": page_entity_keys,
                    "shared_entity_page": False,
                    "outline_extraction": None,
                    "section_count": None,
                    "binding_status": "not_required",
                    "matched_sections": [],
                }
            )
            continue
        outline_key = (page["source_id"], page["page_id"], page["revision_sha1"])
        outline = outline_cache.get(outline_key)
        if outline is None:
            outline = read_page_sections(
                database=wiki_database,
                source_id=page["source_id"],
                title=page["title"],
            )
            if outline is not None:
                outline_cache[outline_key] = outline
        if outline is None:
            raise ValueError(
                f"accepted Wiki page was not found: {page['source_id']}:{page['title']}"
            )
        aliases_by_key: dict[str, list[dict[str, Any]]] = {}
        for alias in page["identity_aliases"]:
            aliases_by_key.setdefault(_section_identity_key(alias["value"]), []).append(
                alias
            )
        matches = []
        for section in outline["sections"]:
            matched_aliases = aliases_by_key.get(
                _section_identity_key(section["title"]), []
            )
            if matched_aliases:
                matches.append({**section, "matched_aliases": matched_aliases})
        results.append(
            {
                **page,
                "page_accepted_entity_keys": page_entity_keys,
                "shared_entity_page": len(page_entity_keys) > 1,
                "outline_extraction": outline["extraction"],
                "section_count": outline["section_count"],
                "binding_status": (
                    "unique" if len(matches) == 1 else "none" if not matches else "ambiguous"
                ),
                "matched_sections": matches,
            }
        )

    return {
        "schema": ENTITY_SECTION_BINDINGS_SCHEMA,
        "match_rule": "nfkc_casefold_whitespace_exact",
        "entity": {
            field: entity[field]
            for field in ("entity_key", "kind", "stable_id", "display_name")
        },
        "accepted_page_count": len(results),
        "unique_binding_count": sum(
            result["binding_status"] == "unique" for result in results
        ),
        "pages": results,
    }


def query_entity_sections(
    *,
    entity_database: Path,
    wiki_database: Path,
    kind: str,
    stable_id: str,
) -> dict[str, Any]:
    """Find exact explicit-heading bindings for one simulator entity."""
    _require_matching_wiki_database(entity_database, wiki_database)
    entities = query_entities(
        database=entity_database,
        kind=kind,
        stable_id=stable_id,
        query=None,
    )
    if not entities:
        raise ValueError(f"simulator entity was not found: {kind}:{stable_id}")
    if len(entities) != 1:
        raise ValueError(f"simulator entity identity is not unique: {kind}:{stable_id}")
    return _entity_section_report(
        entity=entities[0],
        entity_database=entity_database,
        wiki_database=wiki_database,
    )


def search_bound_entity_sections(
    *,
    entity_database: Path,
    wiki_database: Path,
    kind: str,
    stable_id: str,
    query: str,
    raw_query: bool = False,
    limit: int = 20,
) -> dict[str, Any]:
    bindings = query_entity_sections(
        entity_database=entity_database,
        wiki_database=wiki_database,
        kind=kind,
        stable_id=stable_id,
    )
    eligible_sections = []
    excluded_pages = []
    for page in bindings["pages"]:
        if page["binding_status"] != "unique":
            excluded_pages.append(
                {
                    field: page[field]
                    for field in (
                        "source_id",
                        "page_id",
                        "title",
                        "revision_id",
                        "revision_sha1",
                        "binding_status",
                    )
                }
            )
            continue
        section = page["matched_sections"][0]
        eligible_sections.append(
            {
                "source_id": page["source_id"],
                "page_id": page["page_id"],
                "page_title": page["title"],
                "url": page["url"],
                "revision_id": page["revision_id"],
                "revision_timestamp": page["revision_timestamp"],
                "revision_sha1": page["revision_sha1"],
                "shared_entity_page": page["shared_entity_page"],
                "page_accepted_entity_keys": page["page_accepted_entity_keys"],
                "section_document_order": section["document_order"],
                "section_level": section["level"],
                "section_title": section["title"],
                "raw_section_title": section["raw_title"],
                "section_path": section["path"],
                "heading_start": section["heading_start"],
                "body_start": section["body_start"],
                "own_body_end": section["own_body_end"],
                "subtree_end": section["subtree_end"],
                "own_plain_text_length": section["own_plain_text_length"],
                "matched_aliases": section["matched_aliases"],
            }
        )
    matches = search_section_references(
        database=wiki_database,
        query=query,
        section_references=eligible_sections,
        raw_query=raw_query,
        limit=limit,
    )
    return {
        "schema": ENTITY_SECTION_SEARCH_SCHEMA,
        "entity": bindings["entity"],
        "match_rule": bindings["match_rule"],
        "query": {
            "text": query,
            "mode": "raw_fts" if raw_query else "literal_phrase",
            "limit": limit,
        },
        "eligible_section_count": len(eligible_sections),
        "eligible_sections": eligible_sections,
        "excluded_page_count": len(excluded_pages),
        "excluded_pages": excluded_pages,
        "match_count": len(matches),
        "matches": matches,
    }


def _search_entity_mechanics_prepared(
    *,
    entity_database: Path,
    wiki_database: Path,
    kind: str,
    stable_id: str,
    query: str,
    include_related: bool = False,
    raw_query: bool = False,
    limit: int = 20,
    entities_by_identity: dict[tuple[str, str], dict[str, Any]],
    accepted_entity_keys: dict[tuple[str, int], list[str]],
    outline_cache: dict[tuple[str, int, str], dict[str, Any]],
) -> dict[str, Any]:
    if include_related and kind != "encounter":
        raise ValueError("--include-related is only valid for encounter entities")
    root = entities_by_identity.get((kind, stable_id))
    if root is None:
        raise ValueError(f"simulator entity was not found: {kind}:{stable_id}")

    target_bindings: list[tuple[dict[str, Any], dict[str, Any]]] = [
        (
            root,
            {
                "scope": "direct",
                "entity_key": root["entity_key"],
                "kind": root["kind"],
                "stable_id": root["stable_id"],
                "display_name": root["display_name"],
                "relation_type": None,
            },
        )
    ]
    if include_related:
        for relation in root["relations"]:
            if relation["relation_type"] != ENCOUNTER_MEMBER_RELATION:
                continue
            target = relation["target"]
            target_bindings.append(
                (
                    target,
                    {
                        "scope": "related",
                        "entity_key": target["entity_key"],
                        "kind": target["kind"],
                        "stable_id": target["stable_id"],
                        "display_name": target["display_name"],
                        "relation_type": relation["relation_type"],
                    },
                )
            )

    scope_map: dict[tuple[Any, ...], dict[str, Any]] = {}
    excluded_pages = []
    reports: dict[str, dict[str, Any]] = {}
    for target, binding in target_bindings:
        report = reports.get(target["entity_key"])
        if report is None:
            target_identity = (target["kind"], target["stable_id"])
            full_entity = entities_by_identity.get(target_identity)
            if full_entity is None:
                matches = query_entities(
                    database=entity_database,
                    kind=target["kind"],
                    stable_id=target["stable_id"],
                    query=None,
                )
                if len(matches) != 1:
                    raise ValueError(
                        "related simulator entity identity is not unique: "
                        f"{target['entity_key']}"
                    )
                full_entity = matches[0]
                entities_by_identity[target_identity] = full_entity
            report = _entity_section_report(
                entity=full_entity,
                entity_database=entity_database,
                wiki_database=wiki_database,
                accepted_entity_keys=accepted_entity_keys,
                outline_cache=outline_cache,
                shared_pages_only=True,
            )
            reports[target["entity_key"]] = report
        for page in report["pages"]:
            page_identity = {
                "source_id": page["source_id"],
                "page_id": page["page_id"],
                "page_title": page["title"],
                "url": page["url"],
                "revision_id": page["revision_id"],
                "revision_timestamp": page["revision_timestamp"],
                "revision_sha1": page["revision_sha1"],
                "shared_entity_page": page["shared_entity_page"],
                "page_accepted_entity_keys": page["page_accepted_entity_keys"],
            }
            if page["page_accepted_entity_keys"] == [target["entity_key"]]:
                scope_key = ("page", page["source_id"], page["page_id"])
                reference = {
                    **page_identity,
                    "scope_type": "page",
                    "scope_bindings": [],
                }
            elif page["binding_status"] == "unique":
                section = page["matched_sections"][0]
                scope_key = (
                    "section",
                    page["source_id"],
                    page["page_id"],
                    section["document_order"],
                )
                reference = {
                    **page_identity,
                    "scope_type": "section",
                    "section_document_order": section["document_order"],
                    "section_level": section["level"],
                    "section_title": section["title"],
                    "raw_section_title": section["raw_title"],
                    "section_path": section["path"],
                    "heading_start": section["heading_start"],
                    "body_start": section["body_start"],
                    "own_body_end": section["own_body_end"],
                    "subtree_end": section["subtree_end"],
                    "own_plain_text_length": section["own_plain_text_length"],
                    "matched_aliases": section["matched_aliases"],
                    "scope_bindings": [],
                }
            else:
                excluded_pages.append(
                    {
                        **page_identity,
                        "scope_binding": binding,
                        "binding_status": page["binding_status"],
                        "reason": "shared_page_without_unique_section",
                    }
                )
                continue
            stored = scope_map.setdefault(scope_key, reference)
            if binding not in stored["scope_bindings"]:
                stored["scope_bindings"].append(binding)

    scopes = sorted(
        scope_map.values(),
        key=lambda scope: (
            scope["source_id"],
            scope["page_title"],
            0 if scope["scope_type"] == "page" else 1,
            scope.get("section_document_order", 0),
        ),
    )
    matches = search_scope_references(
        database=wiki_database,
        query=query,
        scope_references=scopes,
        raw_query=raw_query,
        limit=limit,
    )
    return {
        "schema": ENTITY_MECHANICS_SEARCH_SCHEMA,
        "scope_rule": "single_entity_page_or_unique_exact_section",
        "entity": {
            field: root[field]
            for field in ("entity_key", "kind", "stable_id", "display_name")
        },
        "query": {
            "text": query,
            "mode": "raw_fts" if raw_query else "literal_phrase",
            "include_related": include_related,
            "limit": limit,
        },
        "eligible_scope_count": len(scopes),
        "eligible_scopes": scopes,
        "excluded_page_count": len(excluded_pages),
        "excluded_pages": excluded_pages,
        "match_count": len(matches),
        "matches": matches,
    }


def _database_identity(path: Path) -> tuple[int, int, int, int]:
    status = path.stat()
    return (status.st_dev, status.st_ino, status.st_size, status.st_mtime_ns)


class AttributedMechanicsOwner:
    """Long-lived exact-entity mechanics lookup over one immutable DB pair."""

    def __init__(self, *, entity_database: Path, wiki_database: Path) -> None:
        self._entity_database = entity_database.resolve(strict=True)
        self._wiki_database = wiki_database.resolve(strict=True)
        initial_identity = self._current_database_identity()
        _require_matching_wiki_database(
            self._entity_database, self._wiki_database
        )
        entities = query_entities(
            database=self._entity_database,
            kind=None,
            stable_id=None,
            query=None,
        )
        self._entities_by_identity = {
            (entity["kind"], entity["stable_id"]): entity for entity in entities
        }
        if len(self._entities_by_identity) != len(entities):
            raise ValueError("simulator entity kind/stable ID identity is not unique")
        self._accepted_entity_keys = _accepted_entity_keys_by_page(
            self._entity_database, None
        )
        self._outline_cache: dict[tuple[str, int, str], dict[str, Any]] = {}
        final_identity = self._current_database_identity()
        if final_identity != initial_identity:
            raise ValueError(
                "Wiki knowledge databases changed while the owner was opening"
            )
        self._database_identity = final_identity

    def _current_database_identity(
        self,
    ) -> tuple[tuple[int, int, int, int], tuple[int, int, int, int]]:
        return (
            _database_identity(self._entity_database),
            _database_identity(self._wiki_database),
        )

    def _require_unchanged_databases(self) -> None:
        if self._current_database_identity() != self._database_identity:
            raise ValueError(
                "Wiki knowledge databases changed; create a new mechanics owner"
            )

    def search(
        self,
        *,
        kind: str,
        stable_id: str,
        query: str,
        include_related: bool = False,
        raw_query: bool = False,
        limit: int = 20,
    ) -> dict[str, Any]:
        self._require_unchanged_databases()
        result = _search_entity_mechanics_prepared(
            entity_database=self._entity_database,
            wiki_database=self._wiki_database,
            kind=kind,
            stable_id=stable_id,
            query=query,
            include_related=include_related,
            raw_query=raw_query,
            limit=limit,
            entities_by_identity=self._entities_by_identity,
            accepted_entity_keys=self._accepted_entity_keys,
            outline_cache=self._outline_cache,
        )
        self._require_unchanged_databases()
        return result


def search_entity_mechanics(
    *,
    entity_database: Path,
    wiki_database: Path,
    kind: str,
    stable_id: str,
    query: str,
    include_related: bool = False,
    raw_query: bool = False,
    limit: int = 20,
) -> dict[str, Any]:
    """Search only page or section scopes attributable to exact entities."""
    _require_matching_wiki_database(entity_database, wiki_database)
    roots = query_entities(
        database=entity_database,
        kind=kind,
        stable_id=stable_id,
        query=None,
    )
    if not roots:
        raise ValueError(f"simulator entity was not found: {kind}:{stable_id}")
    if len(roots) != 1:
        raise ValueError(f"simulator entity identity is not unique: {kind}:{stable_id}")
    root = roots[0]
    return _search_entity_mechanics_prepared(
        entity_database=entity_database,
        wiki_database=wiki_database,
        kind=kind,
        stable_id=stable_id,
        query=query,
        include_related=include_related,
        raw_query=raw_query,
        limit=limit,
        entities_by_identity={(kind, stable_id): root},
        accepted_entity_keys=_accepted_entity_keys_by_page(entity_database, None),
        outline_cache={},
    )


def mechanics_scope_inventory(
    *,
    entity_database: Path,
    wiki_database: Path,
    kind: str | None = None,
    example_limit: int = 20,
) -> dict[str, Any]:
    """Summarize attributable mechanics scopes without returning page bodies."""
    if example_limit < 0:
        raise ValueError("--example-limit must be non-negative")
    _require_matching_wiki_database(entity_database, wiki_database)
    entities = query_entities(
        database=entity_database,
        kind=kind,
        stable_id=None,
        query=None,
    )
    accepted_entity_keys = _accepted_entity_keys_by_page(entity_database, None)
    outline_cache: dict[tuple[str, int, str], dict[str, Any]] = {}

    metric_names = (
        "entity_count",
        "accepted_entity_pages",
        "safe_page_scopes",
        "safe_section_scopes",
        "excluded_shared_none",
        "excluded_shared_ambiguous",
        "excluded_ownership_mismatch",
        "entities_with_safe_scope",
        "entities_without_accepted_pages",
        "entities_with_accepted_but_no_safe_scope",
        "entities_with_partial_exclusions",
    )

    def empty_metrics() -> dict[str, int]:
        return {name: 0 for name in metric_names}

    totals = empty_metrics()
    kinds: dict[str, dict[str, int]] = {}
    sources: dict[str, dict[str, int]] = {}
    reports: dict[str, dict[str, Any]] = {}
    scope_summaries: dict[str, dict[str, int]] = {}
    examples: dict[str, list[dict[str, Any]]] = {
        "entities_without_accepted_pages": [],
        "entities_with_accepted_but_no_safe_scope": [],
        "excluded_pages": [],
        "encounters_with_uncovered_members": [],
    }

    def append_example(category: str, value: dict[str, Any]) -> None:
        if len(examples[category]) < example_limit:
            examples[category].append(value)

    def classify_report(
        report: dict[str, Any], *, include_sources: bool = True
    ) -> tuple[dict[str, int], list[dict[str, Any]]]:
        entity_key = report["entity"]["entity_key"]
        summary = {
            "accepted_entity_pages": len(report["pages"]),
            "safe_page_scopes": 0,
            "safe_section_scopes": 0,
            "excluded_shared_none": 0,
            "excluded_shared_ambiguous": 0,
            "excluded_ownership_mismatch": 0,
        }
        excluded = []
        for page in report["pages"]:
            if page["page_accepted_entity_keys"] == [entity_key]:
                classification = "safe_page_scopes"
            elif entity_key not in page["page_accepted_entity_keys"]:
                classification = "excluded_ownership_mismatch"
            elif page["binding_status"] == "unique":
                classification = "safe_section_scopes"
            elif page["binding_status"] == "ambiguous":
                classification = "excluded_shared_ambiguous"
            else:
                classification = "excluded_shared_none"
            summary[classification] += 1
            if include_sources:
                source = sources.setdefault(
                    page["source_id"],
                    {
                        "accepted_entity_pages": 0,
                        "safe_page_scopes": 0,
                        "safe_section_scopes": 0,
                        "excluded_shared_none": 0,
                        "excluded_shared_ambiguous": 0,
                        "excluded_ownership_mismatch": 0,
                    },
                )
                source["accepted_entity_pages"] += 1
                source[classification] += 1
            if classification.startswith("excluded_"):
                excluded.append(
                    {
                        "entity_key": entity_key,
                        "source_id": page["source_id"],
                        "page_id": page["page_id"],
                        "title": page["title"],
                        "revision_id": page["revision_id"],
                        "page_accepted_entity_keys": page[
                            "page_accepted_entity_keys"
                        ],
                        "binding_status": page["binding_status"],
                        "classification": classification,
                    }
                )
        return summary, excluded

    for entity in entities:
        report = _entity_section_report(
            entity=entity,
            entity_database=entity_database,
            wiki_database=wiki_database,
            accepted_entity_keys=accepted_entity_keys,
            outline_cache=outline_cache,
            shared_pages_only=True,
        )
        reports[entity["entity_key"]] = report
        scope_summary, excluded = classify_report(report)
        scope_summaries[entity["entity_key"]] = scope_summary
        safe_scope_count = (
            scope_summary["safe_page_scopes"]
            + scope_summary["safe_section_scopes"]
        )
        entity_metrics = {
            **scope_summary,
            "entity_count": 1,
            "entities_with_safe_scope": int(safe_scope_count > 0),
            "entities_without_accepted_pages": int(
                scope_summary["accepted_entity_pages"] == 0
            ),
            "entities_with_accepted_but_no_safe_scope": int(
                scope_summary["accepted_entity_pages"] > 0 and safe_scope_count == 0
            ),
            "entities_with_partial_exclusions": int(
                safe_scope_count > 0 and bool(excluded)
            ),
        }
        kind_metrics = kinds.setdefault(entity["kind"], empty_metrics())
        for name in metric_names:
            totals[name] += entity_metrics[name]
            kind_metrics[name] += entity_metrics[name]
        if entity_metrics["entities_without_accepted_pages"]:
            append_example(
                "entities_without_accepted_pages",
                {
                    "entity_key": entity["entity_key"],
                    "display_name": entity["display_name"],
                },
            )
        if entity_metrics["entities_with_accepted_but_no_safe_scope"]:
            append_example(
                "entities_with_accepted_but_no_safe_scope",
                {
                    "entity_key": entity["entity_key"],
                    "display_name": entity["display_name"],
                    "accepted_page_count": scope_summary["accepted_entity_pages"],
                },
            )
        for item in excluded:
            append_example("excluded_pages", item)

    encounter_summary = {
        "encounter_count": 0,
        "initial_member_links": 0,
        "covered_initial_member_links": 0,
        "encounters_with_all_members_covered": 0,
        "encounters_with_partial_member_coverage": 0,
        "encounters_with_no_members_covered": 0,
        "encounters_without_initial_members": 0,
    }
    if kind in (None, "encounter"):
        for encounter in (entity for entity in entities if entity["kind"] == "encounter"):
            encounter_summary["encounter_count"] += 1
            member_keys = sorted(
                {
                    relation["target"]["entity_key"]
                    for relation in encounter["relations"]
                    if relation["relation_type"] == ENCOUNTER_MEMBER_RELATION
                }
            )
            if not member_keys:
                encounter_summary["encounters_without_initial_members"] += 1
                continue
            covered = []
            for member_key in member_keys:
                if member_key not in reports:
                    target = next(
                        relation["target"]
                        for relation in encounter["relations"]
                        if relation["target"]["entity_key"] == member_key
                    )
                    matches = query_entities(
                        database=entity_database,
                        kind=target["kind"],
                        stable_id=target["stable_id"],
                        query=None,
                    )
                    if len(matches) != 1:
                        raise ValueError(
                            f"encounter member identity is not unique: {member_key}"
                        )
                    reports[member_key] = _entity_section_report(
                        entity=matches[0],
                        entity_database=entity_database,
                        wiki_database=wiki_database,
                        accepted_entity_keys=accepted_entity_keys,
                        outline_cache=outline_cache,
                        shared_pages_only=True,
                    )
                    scope_summaries[member_key], _ = classify_report(
                        reports[member_key], include_sources=False
                    )
                member_summary = scope_summaries[member_key]
                has_scope = (
                    member_summary["safe_page_scopes"]
                    + member_summary["safe_section_scopes"]
                    > 0
                )
                covered.append(has_scope)
            encounter_summary["initial_member_links"] += len(member_keys)
            encounter_summary["covered_initial_member_links"] += sum(covered)
            if all(covered):
                encounter_summary["encounters_with_all_members_covered"] += 1
            elif any(covered):
                encounter_summary["encounters_with_partial_member_coverage"] += 1
            else:
                encounter_summary["encounters_with_no_members_covered"] += 1
            if not all(covered):
                append_example(
                    "encounters_with_uncovered_members",
                    {
                        "entity_key": encounter["entity_key"],
                        "member_entity_keys": member_keys,
                        "uncovered_member_entity_keys": [
                            member_key
                            for member_key, is_covered in zip(member_keys, covered)
                            if not is_covered
                        ],
                    },
                )

    return {
        "schema": MECHANICS_SCOPE_INVENTORY_SCHEMA,
        "scope_rule": "single_entity_page_or_unique_exact_section",
        "filter": {"kind": kind, "example_limit": example_limit},
        "totals": totals,
        "kinds": [
            {"kind": item_kind, **metrics}
            for item_kind, metrics in sorted(kinds.items())
        ],
        "sources": [
            {"source_id": source_id, **metrics}
            for source_id, metrics in sorted(sources.items())
        ],
        "encounters": encounter_summary,
        "distinct_page_outlines_read": len(outline_cache),
        "examples": examples,
    }


def query_unmatched(
    *, database: Path, kind: str | None, side: str, candidate_state: str, limit: int
) -> list[dict[str, Any]]:
    if side == "wiki" and candidate_state != "any":
        raise ValueError("--candidate-state applies only to --side simulator")
    connection = _read_only(database)
    try:
        if side == "simulator":
            clauses = ["1=1"]
            parameters: list[Any] = []
            if kind:
                clauses.append("e.kind=?")
                parameters.append(kind)
            if candidate_state == "some":
                clauses.append(
                    "EXISTS (SELECT 1 FROM edges x WHERE x.entity_key=e.entity_key AND x.status='candidate')"
                )
            elif candidate_state == "none":
                clauses.append(
                    "NOT EXISTS (SELECT 1 FROM edges x WHERE x.entity_key=e.entity_key AND x.status='candidate')"
                )
            parameters.append(limit)
            rows = connection.execute(
                f"""
                SELECT e.*,u.reason FROM unmatched_simulator_entities u
                JOIN simulator_entities e ON e.entity_key=u.entity_key
                WHERE {' AND '.join(clauses)} ORDER BY e.kind,e.stable_id LIMIT ?
                """,
                parameters,
            )
        else:
            clauses = ["1=1"]
            parameters = []
            if kind:
                clauses.append("p.kind=?")
                parameters.append(kind)
            parameters.append(limit)
            rows = connection.execute(
                f"""
                SELECT p.*,u.reason FROM unmatched_wiki_pages u
                JOIN wiki_pages p ON p.source_id=u.source_id AND p.page_id=u.page_id
                WHERE {' AND '.join(clauses)} ORDER BY p.kind,p.source_id,p.title LIMIT ?
                """,
                parameters,
            )
        result = [dict(row) for row in rows]
        if side == "simulator":
            for item in result:
                item["external_aliases"] = json.loads(item.pop("external_aliases_json"))
                candidates = connection.execute(
                    """
                    SELECT DISTINCT p.source_id,p.page_id,p.title
                    FROM edges x JOIN wiki_pages p
                      ON p.source_id=x.source_id AND p.page_id=x.page_id
                    WHERE x.entity_key=? AND x.status='candidate'
                    ORDER BY p.source_id,p.title
                    """,
                    (item["entity_key"],),
                ).fetchall()
                item["candidate_page_count"] = len(candidates)
                item["candidate_pages"] = [dict(row) for row in candidates[:5]]
                item["candidate_pages_truncated"] = len(candidates) > 5
        return result
    finally:
        connection.close()


def _query_diagnostics(database: Path, table: str, id_column: str, limit: int) -> list[dict[str, Any]]:
    if (table, id_column) not in {("conflicts", "conflict_id"), ("issues", "issue_id")}:
        raise ValueError("unsupported entity diagnostic table")
    connection = _read_only(database)
    try:
        rows = []
        for row in connection.execute(
            f"SELECT * FROM {table} ORDER BY {id_column} LIMIT ?", (limit,)
        ):
            item = dict(row)
            item["details"] = json.loads(item.pop("details_json"))
            rows.append(item)
        return rows
    finally:
        connection.close()


def query_conflicts(*, database: Path, limit: int) -> list[dict[str, Any]]:
    return _query_diagnostics(database, "conflicts", "conflict_id", limit)


def query_issues(*, database: Path, limit: int) -> list[dict[str, Any]]:
    return _query_diagnostics(database, "issues", "issue_id", limit)


def query_identity_reviews(
    *, database: Path, decision: str | None, limit: int
) -> dict[str, Any]:
    if limit < 0:
        raise ValueError("--limit must be non-negative")
    connection = _read_only(database)
    try:
        metadata = {
            row["key"]: row["value"]
            for row in connection.execute(
                "SELECT key,value FROM metadata WHERE key LIKE 'identity_review_%'"
            )
        }
        counts = {
            row["decision"]: row["count"]
            for row in connection.execute(
                "SELECT decision,COUNT(*) AS count FROM identity_reviews GROUP BY decision"
            )
        }
        parameters: list[Any] = []
        where = ""
        if decision:
            where = "WHERE r.decision=?"
            parameters.append(decision)
        parameters.append(limit)
        reviews = []
        for row in connection.execute(
            f"""
            SELECT r.*,e.kind,e.stable_id
            FROM identity_reviews r
            JOIN simulator_entities e ON e.entity_key=r.entity_key
            {where}
            ORDER BY r.review_id LIMIT ?
            """,
            parameters,
        ):
            item = dict(row)
            item["record"] = {
                "record_source_id": item.pop("record_source_id"),
                "record_page_id": item.pop("record_page_id"),
                "record_page_title": item.pop("record_page_title"),
                "record_revision_id": item.pop("record_revision_id"),
                "record_locator": json.loads(item.pop("record_locator_json")),
                "record_identity": json.loads(item.pop("record_identity_json")),
            }
            reviews.append(item)
        return {
            "schema": IDENTITY_REVIEW_REPORT_SCHEMA,
            "graph_schema": connection.execute(
                "SELECT value FROM metadata WHERE key='schema'"
            ).fetchone()[0],
            "review_schema": metadata.get("identity_review_schema"),
            "project": metadata.get("identity_review_project"),
            "review_file": metadata.get("identity_review_file"),
            "review_file_sha256": metadata.get("identity_review_file_sha256"),
            "counts": {
                "total": sum(counts.values()),
                "accept": counts.get("accept", 0),
                "reject": counts.get("reject", 0),
                "defer": counts.get("defer", 0),
                "applied_accepted_edges": connection.execute(
                    "SELECT COALESCE(SUM(applied_accepted_edge_count),0) FROM identity_reviews"
                ).fetchone()[0],
            },
            "reviews": reviews,
            "limit": limit,
        }
    finally:
        connection.close()


def candidate_audit(
    *, database: Path, kind: str | None, details: bool, limit: int
) -> dict[str, Any]:
    if limit < 0:
        raise ValueError("--limit must be non-negative")
    connection = _read_only(database)
    try:
        kinds = (
            [kind]
            if kind
            else [
                row[0]
                for row in connection.execute(
                    "SELECT DISTINCT kind FROM simulator_entities ORDER BY kind"
                )
            ]
        )
        reports = []
        for entity_kind in kinds:
            entities = [
                dict(row)
                for row in connection.execute(
                    """
                    SELECT e.* FROM unmatched_simulator_entities u
                    JOIN simulator_entities e ON e.entity_key=u.entity_key
                    WHERE e.kind=? ORDER BY e.stable_id
                    """,
                    (entity_kind,),
                )
            ]
            entity_records: dict[str, dict[tuple[Any, ...], dict[str, Any]]] = {
                entity["entity_key"]: {} for entity in entities
            }
            entity_pages: dict[str, dict[tuple[str, int], dict[str, Any]]] = {
                entity["entity_key"]: {} for entity in entities
            }
            title_candidate_entities: set[str] = set()
            for row in connection.execute(
                """
                SELECT x.entity_key,x.evidence_type,x.evidence_json,
                       p.source_id,p.page_id,p.title
                FROM edges x
                JOIN unmatched_simulator_entities u ON u.entity_key=x.entity_key
                JOIN simulator_entities e ON e.entity_key=x.entity_key
                JOIN wiki_pages p ON p.source_id=x.source_id AND p.page_id=x.page_id
                WHERE e.kind=? AND x.status='candidate'
                ORDER BY x.entity_key,x.evidence_type,p.source_id,p.title
                """,
                (entity_kind,),
            ):
                entity_key = row["entity_key"]
                page = {
                    "source_id": row["source_id"],
                    "page_id": row["page_id"],
                    "title": row["title"],
                }
                entity_pages[entity_key][(row["source_id"], row["page_id"])] = page
                evidence = json.loads(row["evidence_json"])
                if row["evidence_type"] == TITLE_CANDIDATE_EVIDENCE:
                    title_candidate_entities.add(entity_key)
                elif "record_identity" in evidence:
                    record = structured_record(evidence)
                    candidate_record_key = identity_record_key(record)
                    record_entry = entity_records[entity_key].setdefault(
                        candidate_record_key, {**record, "pages": {}}
                    )
                    record_entry["pages"][(row["source_id"], row["page_id"])] = page

            record_entities: dict[tuple[Any, ...], set[str]] = {}
            record_values: dict[tuple[Any, ...], dict[str, Any]] = {}
            for entity_key, records in entity_records.items():
                for record_key, record in records.items():
                    record_entities.setdefault(record_key, set()).add(entity_key)
                    shared_record = record_values.setdefault(
                        record_key,
                        {**record, "pages": {}},
                    )
                    shared_record["pages"].update(record["pages"])

            unique_entities = [
                entity
                for entity in entities
                if len(entity_records[entity["entity_key"]]) == 1
                and len(
                    record_entities[next(iter(entity_records[entity["entity_key"]]))]
                )
                == 1
            ]
            multiple_record_entities = [
                entity for entity in entities if len(entity_records[entity["entity_key"]]) > 1
            ]
            colliding_records = [
                record_key for record_key, owners in record_entities.items() if len(owners) > 1
            ]
            title_only_entities = [
                entity
                for entity in entities
                if not entity_records[entity["entity_key"]]
                and entity["entity_key"] in title_candidate_entities
            ]
            no_candidate_entities = [
                entity for entity in entities if not entity_pages[entity["entity_key"]]
            ]
            report: dict[str, Any] = {
                "kind": entity_kind,
                "unmatched_entities": len(entities),
                "with_structured_candidates": sum(
                    bool(entity_records[entity["entity_key"]]) for entity in entities
                ),
                "unique_one_entity_one_record_candidates": len(unique_entities),
                "entities_with_multiple_structured_records": len(multiple_record_entities),
                "structured_records_with_multiple_entities": len(colliding_records),
                "title_only_candidates": len(title_only_entities),
                "no_candidates": len(no_candidate_entities),
            }
            if details:
                def entity_summary(entity: dict[str, Any]) -> dict[str, Any]:
                    return {
                        "entity_key": entity["entity_key"],
                        "stable_id": entity["stable_id"],
                        "display_name": entity["display_name"],
                        "external_id": entity["external_id"],
                        "external_aliases": json.loads(entity["external_aliases_json"]),
                    }

                def record_summary(record_key: tuple[Any, ...]) -> dict[str, Any]:
                    record = record_values[record_key]
                    return {
                        key: value
                        for key, value in record.items()
                        if key != "pages"
                    } | {
                        "pages": sorted(
                            record["pages"].values(),
                            key=lambda page: (page["source_id"], page["title"]),
                        )
                    }

                report["details"] = {
                    "unique_proposals": [
                        {
                            **entity_summary(entity),
                            "record": record_summary(
                                next(iter(entity_records[entity["entity_key"]]))
                            ),
                        }
                        for entity in unique_entities[:limit]
                    ],
                    "entities_with_multiple_records": [
                        {
                            **entity_summary(entity),
                            "records": [
                                record_summary(record_key)
                                for record_key in entity_records[entity["entity_key"]]
                            ],
                        }
                        for entity in multiple_record_entities[:limit]
                    ],
                    "records_with_multiple_entities": [
                        {
                            "record": record_summary(record_key),
                            "entity_keys": sorted(record_entities[record_key]),
                        }
                        for record_key in colliding_records[:limit]
                    ],
                    "title_only": [entity_summary(entity) for entity in title_only_entities[:limit]],
                    "no_candidates": [
                        entity_summary(entity) for entity in no_candidate_entities[:limit]
                    ],
                    "limit": limit,
                }
            reports.append(report)
        return {"schema": CANDIDATE_AUDIT_SCHEMA, "kinds": reports}
    finally:
        connection.close()
