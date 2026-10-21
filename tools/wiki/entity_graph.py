"""Build and query an auditable simulator-to-Wiki entity graph."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import sqlite3
from typing import Any, Iterable
import unicodedata

from .entity_reviews import (
    IDENTITY_REVIEW_SCHEMA,
    load_identity_reviews,
    record_key,
    structured_record,
)
from .simulator_catalog import (
    SimulatorEntity,
    build_simulator_catalog,
    build_simulator_relations,
    file_sha256,
)


ENTITY_GRAPH_SCHEMA = "sts-wiki-entity-graph-v5"
HUIJI_SOURCE_ID = "slay-the-spire-huiji-zh"
WIKIGG_SOURCE_ID = "slaythespire-wikigg-en"
STRUCTURED_IDENTITY_FIELDS = (
    "id",
    "NAME",
    "page",
    "card_id",
    "name",
    "name_en",
    "name_zhs",
)
EVENT_INTERNAL_NAME_PATTERN = re.compile(
    r"^\*\s*(?:This event is internally called|Internally called|"
    r"The internal name of this event is)\s+"
    r"(?:<code>)?([A-Za-z][A-Za-z0-9_]*)(?:</code>)?",
    flags=re.IGNORECASE,
)
HUIJI_EVENT_ENGLISH_NAME_PATTERN = re.compile(
    r"^\s*\|\s*英文名\s*=\s*([^\r\n|}]+?)\s*$",
    flags=re.MULTILINE,
)


def _normalized_identity(value: str | None) -> str | None:
    if not value:
        return None
    normalized = unicodedata.normalize("NFKC", value).casefold()
    token = "".join(character for character in normalized if character.isalnum())
    return token or None


def _page_kind(templates: Iterable[str], categories: Iterable[str]) -> tuple[str | None, list[str]]:
    template_signals = [
        unicodedata.normalize("NFKC", value).replace("_", " ").casefold()
        for value in templates
    ]
    category_signals = [
        unicodedata.normalize("NFKC", value).replace("_", " ").casefold()
        for value in categories
    ]
    signals = [*template_signals, *category_signals]
    template_matches: set[str] = set()
    for signal in template_signals:
        if (
            "infobox" in signal and "card" in signal
        ) or signal == "cardautobox" or signal.endswith(" card navbox"):
            template_matches.add("card")
        if "infobox" in signal and "relic" in signal:
            template_matches.add("relic")
        if "infobox" in signal and "potion" in signal:
            template_matches.add("potion")
        if "infobox" in signal and any(word in signal for word in ("monster", "enemy")):
            template_matches.add("enemy")
        if "infobox" in signal and "event" in signal:
            template_matches.add("event")
        if signal in {"event header", "事件"}:
            template_matches.add("event")
    category_kinds = {
        "card": {"card", "cards", "卡牌"},
        "relic": {"relic", "relics", "遗物"},
        "potion": {"potion", "potions", "药水"},
        "enemy": {"enemy", "enemies", "monster", "monsters", "怪物"},
        "event": {"event", "events", "事件"},
    }
    category_matches: set[str] = set()
    for kind, exact_signals in category_kinds.items():
        if any(signal in exact_signals for signal in category_signals):
            category_matches.add(kind)
    if len(category_matches) == 1:
        return next(iter(category_matches)), sorted(signals)
    if len(category_matches) > 1 or any(
        signal in {"disambiguation pages", "消歧义"} for signal in category_signals
    ):
        return None, sorted(signals)
    return (
        next(iter(template_matches)) if len(template_matches) == 1 else None,
        sorted(signals),
    )


def _normalized_title_candidates(title: str, kind: str) -> tuple[str, ...]:
    normalized_title = unicodedata.normalize("NFKC", title)
    candidates = {_normalized_identity(normalized_title)}
    suffix = re.fullmatch(r"(.*?)\s*\(([^()]*)\)\s*", normalized_title)
    if suffix and _normalized_identity(suffix.group(2)) == _normalized_identity(kind):
        candidates.add(_normalized_identity(suffix.group(1)))
    return tuple(sorted(candidate for candidate in candidates if candidate))


def _load_wiki_pages(connection: sqlite3.Connection) -> list[dict[str, Any]]:
    connection.row_factory = sqlite3.Row
    template_map: dict[tuple[str, int], list[str]] = {}
    category_map: dict[tuple[str, int], list[str]] = {}
    for row in connection.execute("SELECT source_id,page_id,template FROM templates"):
        template_map.setdefault((row["source_id"], row["page_id"]), []).append(row["template"])
    for row in connection.execute("SELECT source_id,page_id,category FROM categories"):
        category_map.setdefault((row["source_id"], row["page_id"]), []).append(row["category"])
    pages = []
    for row in connection.execute(
        """
        SELECT p.source_id,p.page_id,p.namespace_id,p.title,p.url,p.revision_id,
               p.revision_timestamp,p.revision_sha1,p.redirect_target,s.language
        FROM pages p JOIN sources s ON s.source_id=p.source_id
        WHERE p.namespace_id=0
        ORDER BY p.source_id,p.page_id
        """
    ):
        item = dict(row)
        templates = template_map.get((item["source_id"], item["page_id"]), [])
        categories = category_map.get((item["source_id"], item["page_id"]), [])
        kind, signals = _page_kind(templates, categories)
        item["kind"] = kind
        item["signals"] = signals
        pages.append(item)
    return pages


def _read_tabular_rows(
    connection: sqlite3.Connection, source_id: str, title: str
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    connection.row_factory = sqlite3.Row
    page = connection.execute(
        """
        SELECT page_id,title,revision_id,revision_timestamp,revision_sha1,wikitext
        FROM pages WHERE source_id=? AND title=?
        """,
        (source_id, title),
    ).fetchone()
    if page is None:
        raise ValueError(f"required structured Wiki page was not found: {source_id}:{title}")
    value = json.loads(page["wikitext"])
    fields = value.get("schema", {}).get("fields")
    data = value.get("data")
    if not isinstance(fields, list) or not isinstance(data, list):
        raise ValueError(f"structured Wiki page is not tabular: {source_id}:{title}")
    names = [field.get("name") if isinstance(field, dict) else None for field in fields]
    if not all(isinstance(name, str) and name for name in names):
        raise ValueError(f"structured Wiki page has invalid fields: {source_id}:{title}")
    rows = []
    for row_index, row in enumerate(data):
        if not isinstance(row, list) or len(row) != len(names):
            raise ValueError(f"structured Wiki row {row_index} does not match schema: {title}")
        item = {name: row[index] for index, name in enumerate(names)}
        item["_row_index"] = row_index
        rows.append(item)
    provenance = {key: page[key] for key in page.keys() if key != "wikitext"}
    provenance["source_id"] = source_id
    return provenance, rows


def _read_json_records(
    connection: sqlite3.Connection, source_id: str, title_prefix: str, content_model: str
) -> list[tuple[dict[str, Any], dict[str, Any]]]:
    connection.row_factory = sqlite3.Row
    records = []
    for page in connection.execute(
        """
        SELECT page_id,title,revision_id,revision_timestamp,revision_sha1,wikitext
        FROM pages
        WHERE source_id=? AND title LIKE ? AND content_model=?
        ORDER BY title
        """,
        (source_id, f"{title_prefix}%", content_model),
    ):
        try:
            value = json.loads(page["wikitext"])
        except json.JSONDecodeError as error:
            raise ValueError(f"structured Wiki JSON is invalid: {source_id}:{page['title']}: {error}") from error
        if not isinstance(value, dict):
            raise ValueError(f"structured Wiki JSON root is not an object: {source_id}:{page['title']}")
        provenance = {key: page[key] for key in page.keys() if key != "wikitext"}
        provenance["source_id"] = source_id
        records.append((provenance, value))
    return records


def _read_event_identity_claims(
    connection: sqlite3.Connection, typed_pages: list[dict[str, Any]]
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    connection.row_factory = sqlite3.Row
    claims: list[dict[str, Any]] = []
    diagnostics: list[dict[str, Any]] = []
    for page in typed_pages:
        if page["source_id"] != WIKIGG_SOURCE_ID or page["kind"] != "event":
            continue
        record = connection.execute(
            """
            SELECT source_id,page_id,title,revision_id,revision_timestamp,revision_sha1,wikitext
            FROM pages WHERE source_id=? AND page_id=?
            """,
            (page["source_id"], page["page_id"]),
        ).fetchone()
        page_claims = []
        for line_number, line in enumerate(record["wikitext"].splitlines(), start=1):
            match = EVENT_INTERNAL_NAME_PATTERN.match(line.strip())
            if match:
                page_claims.append(
                    {
                        "source_id": record["source_id"],
                        "page_id": record["page_id"],
                        "title": record["title"],
                        "revision_id": record["revision_id"],
                        "revision_timestamp": record["revision_timestamp"],
                        "revision_sha1": record["revision_sha1"],
                        "claim_line": line_number,
                        "internal_name": match.group(1),
                    }
                )
        if len(page_claims) > 1:
            diagnostics.append(
                {
                    "issue_type": "multiple_event_internal_name_claims",
                    "details_json": _json(
                        {
                            "source_id": record["source_id"],
                            "page_id": record["page_id"],
                            "title": record["title"],
                            "revision_id": record["revision_id"],
                            "claims": [claim["internal_name"] for claim in page_claims],
                        }
                    ),
                }
            )
        elif page_claims:
            claims.extend(page_claims)
    return claims, diagnostics


def _read_huiji_event_english_names(
    connection: sqlite3.Connection, typed_pages: list[dict[str, Any]]
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    connection.row_factory = sqlite3.Row
    translations: list[dict[str, Any]] = []
    diagnostics: list[dict[str, Any]] = []
    for page in typed_pages:
        if page["source_id"] != HUIJI_SOURCE_ID or page["kind"] != "event":
            continue
        record = connection.execute(
            """
            SELECT source_id,page_id,title,revision_id,revision_timestamp,revision_sha1,wikitext
            FROM pages WHERE source_id=? AND page_id=?
            """,
            (page["source_id"], page["page_id"]),
        ).fetchone()
        names = list(
            dict.fromkeys(
                value.strip()
                for value in HUIJI_EVENT_ENGLISH_NAME_PATTERN.findall(record["wikitext"])
                if value.strip()
            )
        )
        if len(names) != 1:
            diagnostics.append(
                {
                    "issue_type": "huiji_event_english_name_cardinality",
                    "details_json": _json(
                        {
                            "source_id": record["source_id"],
                            "page_id": record["page_id"],
                            "title": record["title"],
                            "revision_id": record["revision_id"],
                            "english_names": names,
                        }
                    ),
                }
            )
            continue
        translations.append(
            {
                "source_id": record["source_id"],
                "page_id": record["page_id"],
                "title": record["title"],
                "revision_id": record["revision_id"],
                "revision_timestamp": record["revision_timestamp"],
                "revision_sha1": record["revision_sha1"],
                "english_name": names[0],
            }
        )
    return translations, diagnostics


def _json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def rebuild_entity_graph(
    *,
    repository: Path,
    wiki_database: Path,
    output_database: Path,
    review_file: Path | None = None,
) -> dict[str, Any]:
    entities, source_hashes = build_simulator_catalog(repository)
    relations = build_simulator_relations(repository)
    review_bundle = (
        load_identity_reviews(review_file)
        if review_file is not None
        else {
            "schema": IDENTITY_REVIEW_SCHEMA,
            "project": "",
            "reviews": [],
            "path": "",
            "sha256": "",
        }
    )
    if review_file is not None:
        try:
            review_bundle["path"] = review_file.resolve().relative_to(
                repository.resolve()
            ).as_posix()
        except ValueError:
            review_bundle["path"] = str(review_file.resolve())
    wiki_uri = f"file:{wiki_database.resolve().as_posix()}?mode=ro"
    wiki = sqlite3.connect(wiki_uri, uri=True)
    wiki.row_factory = sqlite3.Row
    try:
        pages = _load_wiki_pages(wiki)
        typed_pages = [page for page in pages if page["kind"]]
        event_claims, event_claim_diagnostics = _read_event_identity_claims(
            wiki, typed_pages
        )
        huiji_event_names, huiji_event_diagnostics = _read_huiji_event_english_names(
            wiki, typed_pages
        )
        table_specs = {
            "card": "Data:Card data.tabx",
            "relic": "Data:Relics.tabx",
            "potion": "Data:Potions.tabx",
            "enemy": "Data:Monsters.tabx",
        }
        structured: dict[str, tuple[dict[str, Any], list[dict[str, Any]]]] = {
            kind: _read_tabular_rows(wiki, HUIJI_SOURCE_ID, title)
            for kind, title in table_specs.items()
        }
        card_json_records = _read_json_records(
            wiki, HUIJI_SOURCE_ID, "Data:Card/", "BSON"
        )
        review_source_pages: dict[tuple[str, int], dict[str, Any] | None] = {}
        for review in review_bundle["reviews"]:
            source_key = (
                review["record"]["record_source_id"],
                review["record"]["record_page_id"],
            )
            if source_key in review_source_pages:
                continue
            source_page = wiki.execute(
                """
                SELECT source_id,page_id,title,revision_id
                FROM pages WHERE source_id=? AND page_id=?
                """,
                source_key,
            ).fetchone()
            review_source_pages[source_key] = (
                dict(source_page) if source_page is not None else None
            )
        wiki_sources = [dict(row) for row in wiki.execute("SELECT * FROM sources ORDER BY source_id")]
        wiki_schema = wiki.execute("SELECT value FROM metadata WHERE key='schema'").fetchone()[0]
    finally:
        wiki.close()

    entity_by_key = {entity.entity_key: entity for entity in entities}
    page_by_identity = {(page["source_id"], page["title"]): page for page in pages}
    page_by_key = {(page["source_id"], page["page_id"]): page for page in pages}
    english_sources = sorted(
        source["source_id"] for source in wiki_sources if (source.get("language") or "").startswith("en")
    )
    edges: dict[tuple[str, str, int, str, str], dict[str, Any]] = {}
    conflicts: list[dict[str, Any]] = []
    issues: list[dict[str, Any]] = [
        *event_claim_diagnostics,
        *huiji_event_diagnostics,
    ]

    def resolve_typed_page(
        source_id: str, title: str, expected_kind: str
    ) -> tuple[dict[str, Any] | None, list[str]]:
        chain: list[str] = []
        seen: set[str] = set()
        current_title = title
        while current_title not in seen:
            seen.add(current_title)
            page = page_by_identity.get((source_id, current_title))
            if page is None:
                return None, chain
            chain.append(current_title)
            if page["kind"] == expected_kind:
                return page, chain
            target = page.get("redirect_target")
            if not isinstance(target, str) or not target:
                return None, chain
            current_title = target.split("#", 1)[0]
        return None, chain

    def add_edge(
        entity: SimulatorEntity,
        page: dict[str, Any],
        *,
        status: str,
        evidence_type: str,
        evidence: dict[str, Any],
    ) -> None:
        key = (entity.entity_key, page["source_id"], page["page_id"], status, evidence_type)
        edges[key] = {
            "entity_key": entity.entity_key,
            "source_id": page["source_id"],
            "page_id": page["page_id"],
            "status": status,
            "evidence_type": evidence_type,
            "evidence_json": _json(evidence),
        }

    def add_structured_targets(
        entity: SimulatorEntity,
        row: dict[str, Any],
        provenance: dict[str, Any],
        *,
        target_title: str | None,
        english_title: str | None,
        status: str,
        evidence_type: str,
        matched_field: str,
    ) -> None:
        record_identity = {
            field: row[field]
            for field in STRUCTURED_IDENTITY_FIELDS
            if field in row and row[field] is not None
        }
        evidence = {
            "record_source_id": provenance["source_id"],
            "record_page_id": provenance["page_id"],
            "record_page_title": provenance["title"],
            "record_revision_id": provenance["revision_id"],
            "record_locator": {"row_index": row["_row_index"]},
            "matched_field": matched_field,
            "matched_value": row.get(matched_field),
            "record_identity": record_identity,
        }
        if target_title:
            target, chain = resolve_typed_page(HUIJI_SOURCE_ID, target_title, entity.kind)
            if target:
                target_evidence = {**evidence, "redirect_chain": chain}
                add_edge(
                    entity,
                    target,
                    status=status,
                    evidence_type=evidence_type,
                    evidence=target_evidence,
                )
            elif status == "accepted":
                issues.append(
                    {
                        "entity_key": entity.entity_key,
                        "issue_type": "structured_target_missing_or_untyped",
                        "details_json": _json({**evidence, "target_title": target_title}),
                    }
                )
        if english_title:
            for source_id in english_sources:
                target, chain = resolve_typed_page(source_id, english_title, entity.kind)
                if target:
                    add_edge(
                        entity,
                        target,
                        status=status,
                        evidence_type=evidence_type,
                        evidence={**evidence, "redirect_chain": chain},
                    )

    huiji_events_by_english_name: dict[str, list[dict[str, Any]]] = {}
    for translation in huiji_event_names:
        huiji_events_by_english_name.setdefault(translation["english_name"], []).append(
            translation
        )
    for english_name, translations in huiji_events_by_english_name.items():
        if len(translations) > 1:
            conflicts.append(
                {
                    "conflict_type": "duplicate_huiji_event_english_name",
                    "details_json": _json(
                        {
                            "english_name": english_name,
                            "pages": [
                                {
                                    "page_id": translation["page_id"],
                                    "title": translation["title"],
                                    "revision_id": translation["revision_id"],
                                }
                                for translation in translations
                            ],
                        }
                    ),
                }
            )

    event_claims_by_internal_name: dict[str, list[dict[str, Any]]] = {}
    for claim in event_claims:
        event_claims_by_internal_name.setdefault(claim["internal_name"], []).append(claim)
    for internal_name, claims in event_claims_by_internal_name.items():
        if len(claims) > 1:
            conflicts.append(
                {
                    "conflict_type": "duplicate_event_internal_name_claim",
                    "details_json": _json(
                        {
                            "internal_name": internal_name,
                            "pages": [
                                {
                                    "page_id": claim["page_id"],
                                    "title": claim["title"],
                                    "revision_id": claim["revision_id"],
                                }
                                for claim in claims
                            ],
                        }
                    ),
                }
            )

    def add_event_claim_targets(
        entity: SimulatorEntity, claim: dict[str, Any], *, status: str
    ) -> None:
        evidence_type = (
            "wikigg_event_internal_name_exact"
            if status == "accepted"
            else "normalized_event_internal_name_candidate"
        )
        evidence = {
            "record_source_id": claim["source_id"],
            "record_page_id": claim["page_id"],
            "record_page_title": claim["title"],
            "record_revision_id": claim["revision_id"],
            "record_locator": {"claim_line": claim["claim_line"]},
            "matched_field": "internal_name",
            "matched_value": claim["internal_name"],
            "record_identity": {
                "internal_name": claim["internal_name"],
                "english_title": claim["title"],
            },
        }
        for source_id in english_sources:
            target, chain = resolve_typed_page(source_id, claim["title"], "event")
            if target:
                add_edge(
                    entity,
                    target,
                    status=status,
                    evidence_type=evidence_type,
                    evidence={**evidence, "redirect_chain": chain},
                )
        translations = huiji_events_by_english_name.get(claim["title"], [])
        if len(translations) == 1:
            translation = translations[0]
            target = page_by_identity[(translation["source_id"], translation["title"])]
            add_edge(
                entity,
                target,
                status=status,
                evidence_type=evidence_type,
                evidence={
                    **evidence,
                    "translation_evidence": {
                        "source_id": translation["source_id"],
                        "page_id": translation["page_id"],
                        "revision_id": translation["revision_id"],
                        "matched_field": "英文名",
                        "matched_value": translation["english_name"],
                    },
                },
            )

    for entity in entities:
        if entity.kind == "event":
            anchors = {
                value
                for value in (
                    entity.stable_id,
                    entity.external_id,
                    entity.display_name,
                    *entity.external_aliases,
                )
                if value
            }
            normalized_anchors = {
                token for token in map(_normalized_identity, anchors) if token
            }
            for claim in event_claims:
                if len(event_claims_by_internal_name[claim["internal_name"]]) != 1:
                    continue
                if claim["internal_name"] in anchors:
                    add_event_claim_targets(entity, claim, status="accepted")
                elif _normalized_identity(claim["internal_name"]) in normalized_anchors:
                    add_event_claim_targets(entity, claim, status="candidate")
        if entity.kind == "card":
            card_table_provenance, card_table_rows = structured["card"]
            for record_provenance, record in card_json_records:
                record_id = record.get("id")
                if not isinstance(record_id, str) or record_id != entity.external_id:
                    continue
                chinese_name = record.get("NAME")
                table_matches = [
                    row
                    for row in card_table_rows
                    if isinstance(chinese_name, str) and row.get("page") == chinese_name
                ]
                english_name = table_matches[0].get("name_en") if len(table_matches) == 1 else None
                if len(table_matches) > 1:
                    conflicts.append(
                        {
                            "entity_key": entity.entity_key,
                            "conflict_type": "duplicate_card_json_to_tabular_join",
                            "details_json": _json(
                                {
                                    "json_page_id": record_provenance["page_id"],
                                    "tabular_page_id": card_table_provenance["page_id"],
                                    "tabular_rows": [row["_row_index"] for row in table_matches],
                                }
                            ),
                        }
                    )
                record_with_row = {**record, "_row_index": 0}
                add_structured_targets(
                    entity,
                    record_with_row,
                    record_provenance,
                    target_title=chinese_name if isinstance(chinese_name, str) else None,
                    english_title=english_name if isinstance(english_name, str) else None,
                    status="accepted",
                    evidence_type="huiji_card_json_id_exact",
                    matched_field="id",
                )
        if entity.kind in structured:
            provenance, rows = structured[entity.kind]
            exact: list[tuple[dict[str, Any], str]] = []
            candidate: list[tuple[dict[str, Any], str]] = []
            for row in rows:
                fields: list[tuple[str, Any]]
                if entity.kind == "card":
                    fields = [("card_id", row.get("card_id"))]
                elif entity.kind == "enemy":
                    fields = [("name_en", row.get("name_en"))]
                else:
                    fields = [("id", row.get("id"))]
                anchors = [
                    entity.stable_id,
                    entity.external_id,
                    entity.display_name,
                    *entity.external_aliases,
                ]
                for field, value in fields:
                    if not isinstance(value, str):
                        continue
                    if value in anchors:
                        exact.append((row, field))
                    elif _normalized_identity(value) in {
                        token for token in map(_normalized_identity, anchors) if token
                    }:
                        candidate.append((row, field))
            if entity.kind != "enemy" and len(exact) > 1:
                conflicts.append(
                    {
                        "entity_key": entity.entity_key,
                        "conflict_type": "duplicate_exact_structured_anchor",
                        "details_json": _json(
                            [{"row_index": row["_row_index"], "field": field} for row, field in exact]
                        ),
                    }
                )
            for row, field in exact:
                target_title = (
                    row.get("page")
                    if entity.kind == "card"
                    else row.get("name_zhs")
                    if entity.kind in {"relic", "enemy"}
                    else row.get("name")
                )
                english_title = row.get("name_en") if entity.kind != "potion" else entity.display_name
                add_structured_targets(
                    entity,
                    row,
                    provenance,
                    target_title=target_title if isinstance(target_title, str) else None,
                    english_title=english_title if isinstance(english_title, str) else None,
                    status="accepted",
                    evidence_type="huiji_structured_exact",
                    matched_field=field,
                )
            for row, field in candidate:
                target_title = (
                    row.get("page")
                    if entity.kind == "card"
                    else row.get("name_zhs")
                    if entity.kind in {"relic", "enemy"}
                    else row.get("name")
                )
                english_title = row.get("name_en") if entity.kind != "potion" else entity.display_name
                add_structured_targets(
                    entity,
                    row,
                    provenance,
                    target_title=target_title if isinstance(target_title, str) else None,
                    english_title=english_title if isinstance(english_title, str) else None,
                    status="candidate",
                    evidence_type="normalized_structured_candidate",
                    matched_field=field,
                )

        intrinsic_titles = {
            value
            for value in (entity.stable_id, entity.display_name)
            if value
        }
        external_id_titles = {entity.external_id} if entity.external_id else set()
        external_alias_titles = set(entity.external_aliases)
        normalized_titles = {
            token
            for token in map(
                _normalized_identity,
                (entity.stable_id, entity.display_name, entity.external_id, *entity.external_aliases),
            )
            if token
        }
        for page in typed_pages:
            if page["kind"] != entity.kind:
                continue
            if page["title"] in intrinsic_titles:
                add_edge(
                    entity,
                    page,
                    status="accepted",
                    evidence_type="simulator_identity_title_exact",
                    evidence={"matched_value": page["title"]},
                )
            elif page["title"] in external_id_titles:
                add_edge(
                    entity,
                    page,
                    status="accepted",
                    evidence_type="simulator_external_id_title_exact",
                    evidence={"matched_value": page["title"]},
                )
            elif page["title"] in external_alias_titles:
                add_edge(
                    entity,
                    page,
                    status="accepted",
                    evidence_type="simulator_external_alias_title_exact",
                    evidence={"matched_value": page["title"]},
                )
            else:
                normalized_page_title = _normalized_identity(page["title"])
                matched_normalizations = normalized_titles.intersection(
                    _normalized_title_candidates(page["title"], entity.kind)
                )
                if not matched_normalizations:
                    continue
                evidence: dict[str, Any] = {
                    "record_source_id": page["source_id"],
                    "record_page_id": page["page_id"],
                    "record_page_title": page["title"],
                    "record_revision_id": page["revision_id"],
                    "record_locator": {"field": "title"},
                    "matched_field": (
                        "title"
                        if normalized_page_title in matched_normalizations
                        else "title_without_kind_suffix"
                    ),
                    "matched_value": page["title"],
                    "record_identity": {
                        "kind": page["kind"],
                        "title": page["title"],
                    },
                }
                add_edge(
                    entity,
                    page,
                    status="candidate",
                    evidence_type="normalized_title_candidate",
                    evidence=evidence,
                )

    candidate_records: dict[
        tuple[str, tuple[Any, ...]], list[dict[str, Any]]
    ] = {}
    candidate_record_owners: dict[tuple[Any, ...], set[str]] = {}
    accepted_records: set[tuple[str, tuple[Any, ...]]] = set()
    record_descriptors: dict[tuple[str, tuple[Any, ...]], dict[str, Any]] = {}
    for edge in edges.values():
        evidence = json.loads(edge["evidence_json"])
        if "record_identity" not in evidence:
            continue
        record = structured_record(evidence)
        identity_key = record_key(record)
        entity_record_key = (edge["entity_key"], identity_key)
        record_descriptors[entity_record_key] = record
        if edge["status"] == "candidate":
            candidate_records.setdefault(entity_record_key, []).append(edge)
            candidate_record_owners.setdefault(identity_key, set()).add(edge["entity_key"])
        else:
            accepted_records.add(entity_record_key)

    review_results: list[dict[str, Any]] = []
    for review in review_bundle["reviews"]:
        entity_key = review["entity_key"]
        if entity_key not in entity_by_key:
            raise ValueError(
                f"identity review references unknown simulator entity: {entity_key}"
            )
        identity_key = record_key(review["record"])
        entity_record_key = (entity_key, identity_key)
        matched_edges = candidate_records.get(entity_record_key, [])
        if entity_record_key in accepted_records:
            raise ValueError(
                f"identity review for {entity_key} is redundant because the same "
                "record is already accepted"
            )
        if not matched_edges:
            source_key = (
                review["record"]["record_source_id"],
                review["record"]["record_page_id"],
            )
            source_page = review_source_pages[source_key]
            if source_page is None:
                raise ValueError(
                    f"identity review for {entity_key} is stale: source page "
                    f"{source_key} no longer exists"
                )
            if source_page["revision_id"] != review["record"]["record_revision_id"]:
                raise ValueError(
                    f"identity review for {entity_key} is stale: source page revision "
                    f"{review['record']['record_revision_id']} is now "
                    f"{source_page['revision_id']}"
                )
            if source_page["title"] != review["record"]["record_page_title"]:
                raise ValueError(
                    f"identity review for {entity_key} is stale: source page title "
                    f"{review['record']['record_page_title']!r} is now "
                    f"{source_page['title']!r}"
                )
            same_page_records = [
                record
                for (candidate_entity, _), record in record_descriptors.items()
                if candidate_entity == entity_key
                and record["record_source_id"] == review["record"]["record_source_id"]
                and record["record_page_id"] == review["record"]["record_page_id"]
            ]
            if same_page_records:
                raise ValueError(
                    f"identity review for {entity_key} no longer matches the current "
                    "record locator or identity"
                )
            owners = candidate_record_owners.get(identity_key, set())
            if owners:
                raise ValueError(
                    f"identity review record for {entity_key} belongs to different "
                    f"candidate entities: {sorted(owners)}"
                )
            raise ValueError(
                f"identity review for {entity_key} does not match an existing "
                "structured candidate record"
            )
        owners = candidate_record_owners[identity_key]
        if owners != {entity_key}:
            raise ValueError(
                f"identity review for {entity_key} is ambiguous across candidate "
                f"entities: {sorted(owners)}"
            )
        unique_pages: dict[tuple[str, int], dict[str, Any]] = {}
        for edge in matched_edges:
            page_key = (edge["source_id"], edge["page_id"])
            prior = unique_pages.get(page_key)
            if prior is not None and prior["evidence_json"] != edge["evidence_json"]:
                raise ValueError(
                    f"identity review for {entity_key} has ambiguous evidence for "
                    f"candidate page {page_key}"
                )
            unique_pages[page_key] = edge

        applied_count = 0
        if review["decision"] == "accept":
            for page_key, candidate_edge in sorted(unique_pages.items()):
                candidate_evidence = json.loads(candidate_edge["evidence_json"])
                add_edge(
                    entity_by_key[entity_key],
                    page_by_key[page_key],
                    status="accepted",
                    evidence_type="reviewed_identity_override",
                    evidence={
                        **candidate_evidence,
                        "candidate_evidence_type": candidate_edge["evidence_type"],
                        "identity_review": {
                            "project": review_bundle["project"],
                            "decision": review["decision"],
                            "rationale": review["rationale"],
                            "reviewer": review["reviewer"],
                            "reviewed_on": review["reviewed_on"],
                        },
                    },
                )
                applied_count += 1
        review_results.append(
            {
                **review,
                "candidate_page_count": len(unique_pages),
                "applied_accepted_edge_count": applied_count,
            }
        )

    accepted_event_page_owners: dict[tuple[str, int], set[str]] = {}
    for edge in edges.values():
        if (
            edge["status"] == "accepted"
            and entity_by_key[edge["entity_key"]].kind == "event"
        ):
            accepted_event_page_owners.setdefault(
                (edge["source_id"], edge["page_id"]), set()
            ).add(edge["entity_key"])
    for page_key, owners in sorted(accepted_event_page_owners.items()):
        if len(owners) <= 1:
            continue
        page = page_by_key[page_key]
        conflicts.append(
            {
                "conflict_type": "typed_event_page_has_multiple_active_owners",
                "details_json": _json(
                    {
                        "source_id": page["source_id"],
                        "page_id": page["page_id"],
                        "title": page["title"],
                        "revision_id": page["revision_id"],
                        "entity_keys": sorted(owners),
                    }
                ),
            }
        )

    accepted_entity_keys = {
        edge["entity_key"] for edge in edges.values() if edge["status"] == "accepted"
    }
    accepted_pages = {
        (edge["source_id"], edge["page_id"])
        for edge in edges.values()
        if edge["status"] == "accepted"
    }
    unmatched_entities = [entity for entity in entities if entity.entity_key not in accepted_entity_keys]
    unmatched_pages = [
        page for page in typed_pages if (page["source_id"], page["page_id"]) not in accepted_pages
    ]

    output_database.parent.mkdir(parents=True, exist_ok=True)
    temporary = output_database.with_suffix(output_database.suffix + ".tmp")
    temporary.unlink(missing_ok=True)
    output = sqlite3.connect(temporary)
    try:
        output.executescript(
            """
            PRAGMA journal_mode=OFF;
            PRAGMA synchronous=FULL;
            CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE simulator_sources(path TEXT PRIMARY KEY,sha256 TEXT NOT NULL);
            CREATE TABLE simulator_entities(
                entity_key TEXT PRIMARY KEY,kind TEXT NOT NULL,stable_id TEXT NOT NULL,
                display_name TEXT,external_id TEXT,external_aliases_json TEXT NOT NULL,
                source_path TEXT NOT NULL,source_line INTEGER NOT NULL
            );
            CREATE TABLE simulator_relations(
                relation_id INTEGER PRIMARY KEY,relation_type TEXT NOT NULL,
                source_entity_key TEXT NOT NULL,target_entity_key TEXT NOT NULL,
                source_path TEXT NOT NULL,source_line INTEGER NOT NULL,
                UNIQUE(relation_type,source_entity_key,target_entity_key)
            );
            CREATE TABLE wiki_sources(
                source_id TEXT PRIMARY KEY,site_name TEXT,language TEXT,api_url TEXT,
                page_count INTEGER NOT NULL,corpus_sha256 TEXT NOT NULL,status TEXT NOT NULL,
                acquisition_warning_pages INTEGER NOT NULL,acquisition_error_pages INTEGER NOT NULL
            );
            CREATE TABLE wiki_pages(
                source_id TEXT NOT NULL,page_id INTEGER NOT NULL,kind TEXT NOT NULL,title TEXT NOT NULL,
                url TEXT,revision_id INTEGER,revision_timestamp TEXT,revision_sha1 TEXT,
                redirect_target TEXT,signals_json TEXT NOT NULL,
                PRIMARY KEY(source_id,page_id)
            );
            CREATE TABLE edges(
                edge_id INTEGER PRIMARY KEY,entity_key TEXT NOT NULL,source_id TEXT NOT NULL,
                page_id INTEGER NOT NULL,status TEXT NOT NULL CHECK(status IN ('accepted','candidate')),
                evidence_type TEXT NOT NULL,evidence_json TEXT NOT NULL,
                UNIQUE(entity_key,source_id,page_id,status,evidence_type)
            );
            CREATE TABLE conflicts(
                conflict_id INTEGER PRIMARY KEY,entity_key TEXT,conflict_type TEXT NOT NULL,
                details_json TEXT NOT NULL
            );
            CREATE TABLE issues(
                issue_id INTEGER PRIMARY KEY,entity_key TEXT,issue_type TEXT NOT NULL,
                details_json TEXT NOT NULL
            );
            CREATE TABLE identity_reviews(
                review_id INTEGER PRIMARY KEY,entity_key TEXT NOT NULL,
                decision TEXT NOT NULL CHECK(decision IN ('accept','reject','defer')),
                record_source_id TEXT NOT NULL,record_page_id INTEGER NOT NULL,
                record_page_title TEXT NOT NULL,record_revision_id INTEGER NOT NULL,
                record_locator_json TEXT NOT NULL,record_identity_json TEXT NOT NULL,
                rationale TEXT NOT NULL,reviewer TEXT NOT NULL,reviewed_on TEXT NOT NULL,
                candidate_page_count INTEGER NOT NULL,
                applied_accepted_edge_count INTEGER NOT NULL
            );
            CREATE TABLE unmatched_simulator_entities(
                entity_key TEXT PRIMARY KEY,reason TEXT NOT NULL
            );
            CREATE TABLE unmatched_wiki_pages(
                source_id TEXT NOT NULL,page_id INTEGER NOT NULL,reason TEXT NOT NULL,
                PRIMARY KEY(source_id,page_id)
            );
            CREATE INDEX edges_by_entity ON edges(entity_key,status);
            CREATE INDEX edges_by_page ON edges(source_id,page_id,status);
            CREATE INDEX entities_by_kind_id ON simulator_entities(kind,stable_id);
            CREATE INDEX relations_by_source
                ON simulator_relations(source_entity_key,relation_type);
            CREATE INDEX relations_by_target
                ON simulator_relations(target_entity_key,relation_type);
            """
        )
        output.executemany("INSERT INTO metadata VALUES (?,?)", [
            ("schema", ENTITY_GRAPH_SCHEMA),
            ("wiki_schema", wiki_schema),
            ("wiki_database_sha256", file_sha256(wiki_database)),
            ("identity_review_schema", review_bundle["schema"]),
            ("identity_review_project", review_bundle["project"]),
            ("identity_review_file", review_bundle["path"]),
            ("identity_review_file_sha256", review_bundle["sha256"]),
        ])
        output.executemany("INSERT INTO simulator_sources VALUES (?,?)", source_hashes.items())
        output.executemany(
            "INSERT INTO simulator_entities VALUES (?,?,?,?,?,?,?,?)",
            [
                (
                    entity.entity_key,
                    entity.kind,
                    entity.stable_id,
                    entity.display_name,
                    entity.external_id,
                    _json(entity.external_aliases),
                    entity.source_path,
                    entity.source_line,
                )
                for entity in entities
            ],
        )
        output.executemany(
            """
            INSERT INTO simulator_relations(
                relation_type,source_entity_key,target_entity_key,source_path,source_line
            ) VALUES (?,?,?,?,?)
            """,
            [
                (
                    relation.relation_type,
                    relation.source_entity_key,
                    relation.target_entity_key,
                    relation.source_path,
                    relation.source_line,
                )
                for relation in relations
            ],
        )
        output.executemany(
            "INSERT INTO wiki_sources VALUES (?,?,?,?,?,?,?,?,?)",
            [
                (
                    source["source_id"], source["site_name"], source["language"], source["api_url"],
                    source["page_count"], source["corpus_sha256"], source["status"],
                    source["acquisition_warning_pages"], source["acquisition_error_pages"],
                )
                for source in wiki_sources
            ],
        )
        output.executemany(
            "INSERT INTO wiki_pages VALUES (?,?,?,?,?,?,?,?,?,?)",
            [
                (
                    page["source_id"], page["page_id"], page["kind"], page["title"], page["url"],
                    page["revision_id"], page["revision_timestamp"], page["revision_sha1"],
                    page["redirect_target"], _json(page["signals"]),
                )
                for page in typed_pages
            ],
        )
        output.executemany(
            "INSERT INTO edges(entity_key,source_id,page_id,status,evidence_type,evidence_json) VALUES (?,?,?,?,?,?)",
            [
                (edge["entity_key"], edge["source_id"], edge["page_id"], edge["status"], edge["evidence_type"], edge["evidence_json"])
                for edge in edges.values()
            ],
        )
        output.executemany(
            "INSERT INTO conflicts(entity_key,conflict_type,details_json) VALUES (?,?,?)",
            [(item.get("entity_key"), item["conflict_type"], item["details_json"]) for item in conflicts],
        )
        output.executemany(
            "INSERT INTO issues(entity_key,issue_type,details_json) VALUES (?,?,?)",
            [(item.get("entity_key"), item["issue_type"], item["details_json"]) for item in issues],
        )
        output.executemany(
            """
            INSERT INTO identity_reviews(
                entity_key,decision,record_source_id,record_page_id,record_page_title,
                record_revision_id,record_locator_json,record_identity_json,rationale,
                reviewer,reviewed_on,candidate_page_count,applied_accepted_edge_count
            ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
            """,
            [
                (
                    item["entity_key"], item["decision"],
                    item["record"]["record_source_id"],
                    item["record"]["record_page_id"],
                    item["record"]["record_page_title"],
                    item["record"]["record_revision_id"],
                    _json(item["record"]["record_locator"]),
                    _json(item["record"]["record_identity"]),
                    item["rationale"], item["reviewer"], item["reviewed_on"],
                    item["candidate_page_count"], item["applied_accepted_edge_count"],
                )
                for item in review_results
            ],
        )
        output.executemany(
            "INSERT INTO unmatched_simulator_entities VALUES (?,?)",
            [(entity.entity_key, "no accepted source-page edge") for entity in unmatched_entities],
        )
        output.executemany(
            "INSERT INTO unmatched_wiki_pages VALUES (?,?,?)",
            [(page["source_id"], page["page_id"], "no accepted simulator-entity edge") for page in unmatched_pages],
        )
        output.commit()
        if output.execute("PRAGMA quick_check").fetchone()[0] != "ok":
            raise ValueError("entity graph SQLite quick_check failed")
    finally:
        output.close()
    os.replace(temporary, output_database)

    kind_summary: dict[str, dict[str, int]] = {}
    for entity in entities:
        item = kind_summary.setdefault(entity.kind, {"entities": 0, "accepted": 0, "unmatched": 0})
        item["entities"] += 1
        if entity.entity_key in accepted_entity_keys:
            item["accepted"] += 1
        else:
            item["unmatched"] += 1
    relation_summary: dict[str, dict[str, Any]] = {}
    for relation in relations:
        item = relation_summary.setdefault(
            relation.relation_type,
            {"count": 0, "source_entities": set(), "target_entities": set()},
        )
        item["count"] += 1
        item["source_entities"].add(relation.source_entity_key)
        item["target_entities"].add(relation.target_entity_key)
    for item in relation_summary.values():
        item["source_entity_count"] = len(item.pop("source_entities"))
        item["target_entity_count"] = len(item.pop("target_entities"))
    return {
        "schema": ENTITY_GRAPH_SCHEMA,
        "database": str(output_database.resolve()),
        "sha256": file_sha256(output_database),
        "simulator_entity_count": len(entities),
        "simulator_relation_count": len(relations),
        "typed_wiki_page_count": len(typed_pages),
        "accepted_edge_count": sum(edge["status"] == "accepted" for edge in edges.values()),
        "candidate_edge_count": sum(edge["status"] == "candidate" for edge in edges.values()),
        "conflict_count": len(conflicts),
        "issue_count": len(issues),
        "identity_review_count": len(review_results),
        "reviewed_accepted_edge_count": sum(
            item["applied_accepted_edge_count"] for item in review_results
        ),
        "unmatched_simulator_entity_count": len(unmatched_entities),
        "unmatched_typed_wiki_page_count": len(unmatched_pages),
        "kinds": kind_summary,
        "relations": relation_summary,
    }
