"""Install, normalize, index, and query verified MediaWiki exports."""

from __future__ import annotations

from collections import Counter
import gzip
import json
import os
from pathlib import Path
import shutil
import sqlite3
from typing import Any, Iterator

from .mediawiki_records import (
    NORMALIZED_SCHEMA,
    VerifiedBundle,
    canonical_json,
    extract_section_outline,
    file_sha256,
    iter_pages,
    normalize_page,
    plain_text_from_wikitext,
    utc_now,
    verify_bundle,
)


SOURCE_SCHEMA = "external-research-mediawiki-source-v1"
CORPUS_SCHEMA = "mediawiki-normalized-corpus-v1"
INDEX_SCHEMA = "mediawiki-fts-index-v1"
PAGE_SECTION_OUTLINE_SCHEMA = "mediawiki-page-section-outline-v1"


def _fts_match_query(query: str, raw_query: bool) -> str:
    if raw_query:
        return query
    escaped_query = query.replace('"', '""')
    return f'"{escaped_query}"'


def _write_json_atomic(path: Path, value: Any) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("w", encoding="utf-8", newline="\n") as output:
        output.write(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True))
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    os.replace(temporary, path)


def _relative_path(path: Path, root: Path) -> str:
    return path.resolve().relative_to(root.resolve()).as_posix()


def _namespace_manifest(siteinfo: dict[str, Any]) -> list[dict[str, Any]]:
    namespaces = siteinfo.get("namespaces", {})
    values = namespaces if isinstance(namespaces, list) else list(namespaces.values())
    result = []
    for namespace in values:
        if not isinstance(namespace, dict) or not isinstance(namespace.get("id"), int):
            continue
        if namespace["id"] < 0:
            continue
        result.append(
            {
                "id": namespace["id"],
                "name": namespace.get("name", namespace.get("*", "")),
                "canonical": namespace.get("canonical"),
                "content": bool(namespace.get("content", False)),
                "case": namespace.get("case"),
            }
        )
    return sorted(result, key=lambda namespace: namespace["id"])


def install_browser_bundle(
    *,
    bundle: Path,
    library_root: Path,
    capture_name: str,
) -> Path:
    verified = verify_bundle(bundle)
    source_id = verified.header["source_id"]
    source_root = library_root / source_id
    capture_dir = source_root / "captures" / capture_name
    if capture_dir.exists():
        raise FileExistsError(f"fresh capture path already exists: {capture_dir}")
    capture_dir.mkdir(parents=True)
    suffix = ".jsonl.gz" if bundle.suffix.casefold() == ".gz" else ".jsonl"
    installed = capture_dir / f"export{suffix}"
    with bundle.open("rb") as source, installed.open("xb") as target:
        shutil.copyfileobj(source, target, length=1024 * 1024)
    installed_verified = verify_bundle(installed)
    manifest = {
        "schema": "mediawiki-capture-v1",
        "source_id": source_id,
        "api_url": verified.header.get("api_url"),
        "transport": verified.header.get("transport", "browser-mediawiki-api"),
        "bundle": {
            "path": installed.name,
            "sha256": installed_verified.sha256,
            "page_count": installed_verified.page_count,
            "namespace_counts": installed_verified.namespace_counts,
            "acquisition_warning_pages": installed_verified.acquisition_warning_pages,
            "acquisition_error_pages": installed_verified.acquisition_error_pages,
        },
        "completed_at": verified.trailer.get("completed_at"),
    }
    _write_json_atomic(capture_dir / "manifest.json", manifest)
    return installed


def normalize_export(
    *,
    bundle: Path,
    source_root: Path,
    replace: bool = False,
) -> Path:
    verified = verify_bundle(bundle)
    source_id = verified.header["source_id"]
    if source_root.name != source_id:
        raise ValueError(
            f"source root name {source_root.name!r} does not match bundle source_id {source_id!r}"
        )
    corpus_dir = source_root / "corpus"
    output = corpus_dir / "pages.jsonl.gz"
    manifest_path = corpus_dir / "manifest.json"
    if not replace and (output.exists() or manifest_path.exists()):
        raise FileExistsError(f"canonical corpus already exists below {corpus_dir}")
    corpus_dir.mkdir(parents=True, exist_ok=True)
    temporary = corpus_dir / "pages.jsonl.gz.tmp"
    siteinfo = verified.header["siteinfo"]
    namespace_counts: Counter[str] = Counter()
    page_count = 0
    warning_page_count = 0
    error_page_count = 0
    with gzip.open(temporary, "wt", encoding="utf-8", newline="\n", compresslevel=6) as target:
        for page in iter_pages(bundle):
            normalized = normalize_page(page, source_id=source_id, siteinfo=siteinfo)
            target.write(canonical_json(normalized))
            target.write("\n")
            namespace_counts[str(normalized["namespace_id"])] += 1
            page_count += 1
            warning_page_count += bool(normalized["acquisition_warnings"])
            error_page_count += bool(normalized["acquisition_errors"])
    if page_count != verified.page_count or dict(namespace_counts) != verified.namespace_counts:
        temporary.unlink(missing_ok=True)
        raise ValueError("normalized corpus counts diverged from verified bundle")
    os.replace(temporary, output)
    corpus_manifest = {
        "schema": CORPUS_SCHEMA,
        "source_id": source_id,
        "built_at": utc_now(),
        "input_bundle": {
            "path": _relative_path(bundle, source_root),
            "sha256": verified.sha256,
        },
        "pages": {
            "path": output.name,
            "sha256": file_sha256(output),
            "page_count": page_count,
            "namespace_counts": dict(namespace_counts),
        },
        "acquisition": {
            "warning_pages": warning_page_count,
            "error_pages": error_page_count,
        },
    }
    _write_json_atomic(manifest_path, corpus_manifest)

    general = siteinfo.get("general", {})
    statistics = siteinfo.get("statistics", {})
    rights = siteinfo.get("rightsinfo", {})
    source_manifest = {
        "schema": SOURCE_SCHEMA,
        "id": source_id,
        "provider": "mediawiki",
        "status": (
            "captured_complete_inventory_with_declared_text_gaps"
            if error_page_count
            else "captured_complete_text_with_metadata_warnings"
            if warning_page_count
            else "captured_complete_text"
        ),
        "source": {
            "api_url": verified.header.get("api_url"),
            "site_name": general.get("sitename"),
            "language": general.get("lang"),
            "generator": general.get("generator"),
            "rights": rights,
        },
        "remote_statistics_at_capture": statistics,
        "namespaces": _namespace_manifest(siteinfo),
        "canonical_corpus": corpus_manifest["pages"],
        "canonical_capture": corpus_manifest["input_bundle"],
        "acquisition": corpus_manifest["acquisition"],
        "scope": verified.header.get("request_contract"),
        "evidence_boundary": (
            "Wiki mechanics/reference text. It is not an expert consensus, policy label, "
            "or proof that a strategic choice is optimal."
        ),
    }
    _write_json_atomic(source_root / "source.json", source_manifest)
    return output


def iter_normalized_pages(path: Path) -> Iterator[dict[str, Any]]:
    with gzip.open(path, "rt", encoding="utf-8", newline="") as source:
        for line_number, line in enumerate(source, start=1):
            if not line.strip():
                continue
            record = json.loads(line)
            if record.get("schema") != NORMALIZED_SCHEMA:
                raise ValueError(f"{path}:{line_number}: unsupported normalized page schema")
            yield record


def discover_sources(library_root: Path) -> list[tuple[Path, dict[str, Any]]]:
    sources: list[tuple[Path, dict[str, Any]]] = []
    if not library_root.is_dir():
        return sources
    for manifest_path in sorted(library_root.glob("*/source.json")):
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if manifest.get("schema") != SOURCE_SCHEMA:
            continue
        source_root = manifest_path.parent
        corpus_path = source_root / "corpus" / manifest["canonical_corpus"]["path"]
        if file_sha256(corpus_path) != manifest["canonical_corpus"]["sha256"]:
            raise ValueError(f"canonical corpus hash mismatch: {corpus_path}")
        sources.append((source_root, manifest))
    return sources


def rebuild_index(*, library_root: Path, database: Path) -> dict[str, Any]:
    sources = discover_sources(library_root)
    database.parent.mkdir(parents=True, exist_ok=True)
    temporary = database.with_suffix(database.suffix + ".tmp")
    temporary.unlink(missing_ok=True)
    connection = sqlite3.connect(temporary)
    try:
        connection.executescript(
            """
            PRAGMA journal_mode=OFF;
            PRAGMA synchronous=FULL;
            CREATE TABLE metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE sources(
                source_id TEXT PRIMARY KEY,
                site_name TEXT,
                language TEXT,
                api_url TEXT,
                page_count INTEGER NOT NULL,
                corpus_sha256 TEXT NOT NULL,
                status TEXT NOT NULL,
                acquisition_warning_pages INTEGER NOT NULL,
                acquisition_error_pages INTEGER NOT NULL
            );
            CREATE TABLE namespaces(
                source_id TEXT NOT NULL,
                namespace_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                canonical TEXT,
                is_content INTEGER NOT NULL,
                case_rule TEXT,
                PRIMARY KEY(source_id, namespace_id),
                FOREIGN KEY(source_id) REFERENCES sources(source_id)
            );
            CREATE TABLE pages(
                row_id INTEGER PRIMARY KEY,
                source_id TEXT NOT NULL,
                page_id INTEGER NOT NULL,
                namespace_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                url TEXT,
                revision_id INTEGER,
                revision_timestamp TEXT,
                revision_sha1 TEXT,
                content_model TEXT,
                redirect_target TEXT,
                plain_text TEXT NOT NULL,
                wikitext TEXT NOT NULL,
                acquisition_warnings_json TEXT NOT NULL,
                acquisition_errors_json TEXT NOT NULL,
                UNIQUE(source_id, page_id),
                FOREIGN KEY(source_id) REFERENCES sources(source_id)
            );
            CREATE TABLE categories(
                source_id TEXT NOT NULL,
                page_id INTEGER NOT NULL,
                category TEXT NOT NULL,
                PRIMARY KEY(source_id, page_id, category)
            );
            CREATE TABLE links(
                source_id TEXT NOT NULL,
                page_id INTEGER NOT NULL,
                target_title TEXT NOT NULL,
                PRIMARY KEY(source_id, page_id, target_title)
            );
            CREATE TABLE templates(
                source_id TEXT NOT NULL,
                page_id INTEGER NOT NULL,
                template TEXT NOT NULL,
                PRIMARY KEY(source_id, page_id, template)
            );
            CREATE VIRTUAL TABLE pages_fts USING fts5(
                source_id UNINDEXED,
                page_id UNINDEXED,
                title,
                plain_text,
                wikitext,
                tokenize='unicode61 remove_diacritics 2'
            );
            """
        )
        total_pages = 0
        for source_root, manifest in sources:
            source_id = manifest["id"]
            corpus = manifest["canonical_corpus"]
            source = manifest["source"]
            connection.execute(
                "INSERT INTO sources VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    source_id,
                    source.get("site_name"),
                    source.get("language"),
                    source.get("api_url"),
                    corpus["page_count"],
                    corpus["sha256"],
                    manifest.get("status", "unknown"),
                    manifest.get("acquisition", {}).get("warning_pages", 0),
                    manifest.get("acquisition", {}).get("error_pages", 0),
                ),
            )
            connection.executemany(
                "INSERT INTO namespaces VALUES (?,?,?,?,?,?)",
                (
                    (
                        source_id,
                        namespace["id"],
                        namespace.get("name") or "",
                        namespace.get("canonical"),
                        1 if namespace.get("content") else 0,
                        namespace.get("case"),
                    )
                    for namespace in manifest.get("namespaces", [])
                ),
            )
            corpus_path = source_root / "corpus" / corpus["path"]
            for page in iter_normalized_pages(corpus_path):
                cursor = connection.execute(
                    """
                    INSERT INTO pages(
                        source_id,page_id,namespace_id,title,url,revision_id,
                        revision_timestamp,revision_sha1,content_model,redirect_target,
                        plain_text,wikitext,acquisition_warnings_json,acquisition_errors_json
                    ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                    """,
                    (
                        source_id,
                        page["page_id"],
                        page["namespace_id"],
                        page["title"],
                        page["url"],
                        page["revision_id"],
                        page["revision_timestamp"],
                        page["revision_sha1"],
                        page["content_model"],
                        page["redirect_target"],
                        page["plain_text"],
                        page["wikitext"],
                        json.dumps(page.get("acquisition_warnings", []), ensure_ascii=False),
                        json.dumps(page.get("acquisition_errors", []), ensure_ascii=False),
                    ),
                )
                row_id = cursor.lastrowid
                connection.execute(
                    "INSERT INTO pages_fts(rowid,source_id,page_id,title,plain_text,wikitext) VALUES (?,?,?,?,?,?)",
                    (
                        row_id,
                        source_id,
                        page["page_id"],
                        page["title"],
                        page["plain_text"],
                        page["wikitext"],
                    ),
                )
                connection.executemany(
                    "INSERT INTO categories VALUES (?,?,?)",
                    ((source_id, page["page_id"], value) for value in page["categories"]),
                )
                connection.executemany(
                    "INSERT INTO links VALUES (?,?,?)",
                    ((source_id, page["page_id"], value) for value in page["internal_links"]),
                )
                connection.executemany(
                    "INSERT INTO templates VALUES (?,?,?)",
                    ((source_id, page["page_id"], value) for value in page["templates"]),
                )
                total_pages += 1
        connection.execute(
            "INSERT INTO metadata VALUES (?, ?)", ("schema", INDEX_SCHEMA)
        )
        connection.execute(
            "INSERT INTO metadata VALUES (?, ?)", ("built_at", utc_now())
        )
        connection.commit()
        quick_check = connection.execute("PRAGMA quick_check").fetchone()[0]
        if quick_check != "ok":
            raise ValueError(f"SQLite quick_check failed: {quick_check}")
    finally:
        connection.close()
    os.replace(temporary, database)
    return {
        "database": str(database.resolve()),
        "source_count": len(sources),
        "page_count": total_pages,
        "sha256": file_sha256(database),
    }


def search_index(
    *,
    database: Path,
    query: str,
    page_references: list[tuple[str, int]] | None = None,
    source_id: str | None = None,
    language: str | None = None,
    namespace_id: int | None = None,
    content_only: bool = False,
    raw_query: bool = False,
    limit: int = 20,
) -> list[dict[str, Any]]:
    if page_references is not None:
        page_references = sorted(set(page_references))
        if not page_references:
            return []
    connection = sqlite3.connect(f"file:{database.resolve().as_posix()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        match_query = _fts_match_query(query, raw_query)
        clauses = ["pages_fts MATCH ?"]
        parameters: list[Any] = [match_query]
        if page_references is not None:
            clauses.append(
                "("
                + " OR ".join(
                    "(p.source_id = ? AND p.page_id = ?)" for _ in page_references
                )
                + ")"
            )
            for page_source_id, page_id in page_references:
                parameters.extend((page_source_id, page_id))
        if source_id:
            clauses.append("p.source_id = ?")
            parameters.append(source_id)
        if language:
            clauses.append("s.language = ?")
            parameters.append(language)
        if namespace_id is not None:
            clauses.append("p.namespace_id = ?")
            parameters.append(namespace_id)
        if content_only:
            clauses.append("COALESCE(n.is_content, 0) = 1")
        parameters.append(limit)
        rows = connection.execute(
            f"""
            SELECT p.source_id,p.page_id,p.namespace_id,p.title,p.url,
                   p.revision_id,p.revision_timestamp,p.revision_sha1,p.redirect_target,
                   s.language,n.name AS namespace_name,n.canonical AS namespace_canonical,
                   COALESCE(n.is_content,0) AS namespace_is_content,
                   snippet(pages_fts, 3, '[', ']', ' … ', 32) AS snippet,
                   bm25(pages_fts, 4.0, 1.0, 0.25) AS rank
            FROM pages_fts
            JOIN pages p ON p.row_id = pages_fts.rowid
            JOIN sources s ON s.source_id = p.source_id
            LEFT JOIN namespaces n
              ON n.source_id = p.source_id AND n.namespace_id = p.namespace_id
            WHERE {' AND '.join(clauses)}
            ORDER BY rank, p.source_id, p.page_id
            LIMIT ?
            """,
            parameters,
        ).fetchall()
        return [dict(row) for row in rows]
    finally:
        connection.close()


def search_scope_references(
    *,
    database: Path,
    query: str,
    scope_references: list[dict[str, Any]],
    raw_query: bool = False,
    limit: int = 20,
) -> list[dict[str, Any]]:
    """Search revision-verified whole pages or explicit section subtrees."""
    if limit < 1:
        raise ValueError("--limit must be at least 1")
    if not scope_references:
        return []

    source = sqlite3.connect(f"file:{database.resolve().as_posix()}?mode=ro", uri=True)
    source.row_factory = sqlite3.Row
    documents: list[tuple[dict[str, Any], str, list[str], str, str]] = []
    try:
        for reference in scope_references:
            page = source.execute(
                """
                SELECT p.source_id,p.page_id,p.title,p.revision_id,p.revision_sha1,p.wikitext
                FROM pages p WHERE p.source_id=? AND p.page_id=?
                """,
                (reference["source_id"], reference["page_id"]),
            ).fetchone()
            if page is None:
                raise ValueError(
                    "corpus scope page was not found: "
                    f"{reference['source_id']}:{reference['page_id']}"
                )
            expected_page_identity = {
                "title": reference["page_title"],
                "revision_id": reference["revision_id"],
                "revision_sha1": reference["revision_sha1"],
            }
            for field, expected in expected_page_identity.items():
                if page[field] != expected:
                    raise ValueError(
                        "corpus scope revision mismatch: "
                        f"{reference['source_id']}:{reference['page_id']}:{field}"
                    )
            if reference["scope_type"] == "page":
                scope_title = reference["page_title"]
                scope_path = [reference["page_title"]]
                wikitext = page["wikitext"]
            elif reference["scope_type"] == "section":
                outline = extract_section_outline(page["wikitext"])
                section = next(
                    (
                        item
                        for item in outline["sections"]
                        if item["document_order"]
                        == reference["section_document_order"]
                    ),
                    None,
                )
                if section is None or any(
                    section[field] != reference[reference_field]
                    for field, reference_field in (
                        ("title", "section_title"),
                        ("path", "section_path"),
                        ("heading_start", "heading_start"),
                        ("subtree_end", "subtree_end"),
                    )
                ):
                    raise ValueError(
                        "section reference locator mismatch: "
                        f"{reference['source_id']}:{reference['page_id']}:"
                        f"{reference['section_document_order']}"
                    )
                scope_title = reference["section_title"]
                scope_path = reference["section_path"]
                wikitext = page["wikitext"][
                    section["heading_start"] : section["subtree_end"]
                ]
            else:
                raise ValueError(
                    f"unknown corpus search scope type: {reference['scope_type']}"
                )
            documents.append(
                (
                    reference,
                    scope_title,
                    scope_path,
                    plain_text_from_wikitext(wikitext),
                    wikitext,
                )
            )
    finally:
        source.close()

    search = sqlite3.connect(":memory:")
    search.row_factory = sqlite3.Row
    try:
        search.execute(
            """
            CREATE VIRTUAL TABLE section_fts USING fts5(
                reference_index UNINDEXED,title,path,plain_text,wikitext,
                tokenize='unicode61 remove_diacritics 2'
            )
            """
        )
        search.executemany(
            "INSERT INTO section_fts VALUES (?,?,?,?,?)",
            (
                (
                    index,
                    scope_title,
                    " > ".join(scope_path),
                    plain_text,
                    wikitext,
                )
                for index, (
                    reference,
                    scope_title,
                    scope_path,
                    plain_text,
                    wikitext,
                ) in enumerate(documents)
            ),
        )
        rows = search.execute(
            """
            SELECT reference_index,
                   snippet(section_fts,3,'[',']',' … ',32) AS snippet,
                   bm25(section_fts,4.0,2.0,1.0,0.25) AS rank
            FROM section_fts WHERE section_fts MATCH ?
            ORDER BY rank,reference_index LIMIT ?
            """,
            (_fts_match_query(query, raw_query), limit),
        ).fetchall()
        return [
            {
                **scope_references[row["reference_index"]],
                "snippet": row["snippet"],
                "rank": row["rank"],
            }
            for row in rows
        ]
    finally:
        search.close()


def search_section_references(
    *,
    database: Path,
    query: str,
    section_references: list[dict[str, Any]],
    raw_query: bool = False,
    limit: int = 20,
) -> list[dict[str, Any]]:
    """Search revision-verified explicit section subtrees without returning bodies."""
    return search_scope_references(
        database=database,
        query=query,
        scope_references=[
            {**reference, "scope_type": "section"}
            for reference in section_references
        ],
        raw_query=raw_query,
        limit=limit,
    )


def index_inventory(*, database: Path) -> list[dict[str, Any]]:
    connection = sqlite3.connect(f"file:{database.resolve().as_posix()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        sources = []
        for source in connection.execute(
            """
            SELECT source_id,site_name,language,api_url,page_count,corpus_sha256,
                   status,acquisition_warning_pages,acquisition_error_pages
            FROM sources ORDER BY source_id
            """
        ):
            item = dict(source)
            item["namespace_counts"] = {
                str(row[0]): row[1]
                for row in connection.execute(
                    "SELECT namespace_id,COUNT(*) FROM pages WHERE source_id=? GROUP BY namespace_id ORDER BY namespace_id",
                    (item["source_id"],),
                )
            }
            item["namespaces"] = [
                {
                    "id": row[0],
                    "name": row[1],
                    "canonical": row[2],
                    "content": bool(row[3]),
                    "page_count": row[4],
                }
                for row in connection.execute(
                    """
                    SELECT n.namespace_id,n.name,n.canonical,n.is_content,COUNT(p.page_id)
                    FROM namespaces n
                    LEFT JOIN pages p
                      ON p.source_id=n.source_id AND p.namespace_id=n.namespace_id
                    WHERE n.source_id=?
                    GROUP BY n.namespace_id,n.name,n.canonical,n.is_content
                    ORDER BY n.namespace_id
                    """,
                    (item["source_id"],),
                )
            ]
            sources.append(item)
        return sources
    finally:
        connection.close()


def structured_data_inventory(
    *, database: Path, source_id: str, namespace_id: int = 3500, issue_limit: int = 10
) -> dict[str, Any]:
    """Summarize machine-readable MediaWiki Data pages without dumping their bodies."""

    if issue_limit < 0:
        raise ValueError("issue_limit must be non-negative")

    connection = sqlite3.connect(f"file:{database.resolve().as_posix()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        rows = connection.execute(
            """
            SELECT page_id,title,revision_id,content_model,wikitext
            FROM pages
            WHERE source_id=? AND namespace_id=?
            ORDER BY title
            """,
            (source_id, namespace_id),
        ).fetchall()
    finally:
        connection.close()

    families: dict[str, dict[str, Any]] = {}
    parse_issues: list[dict[str, Any]] = []
    for row in rows:
        suffix = row["title"].split(":", 1)[-1]
        if "/" in suffix:
            family = suffix.split("/", 1)[0]
        elif " data." in suffix.casefold():
            family = suffix[: suffix.casefold().index(" data.")]
        else:
            family = suffix.rsplit(".", 1)[0]
        item = families.setdefault(
            family,
            {
                "family": family,
                "page_count": 0,
                "content_models": {},
                "json_page_count": 0,
                "non_json_page_count": 0,
                "json_object_keys": set(),
                "tabular_fields": set(),
                "tabular_row_count": 0,
                "sample_titles": [],
            },
        )
        item["page_count"] += 1
        model = row["content_model"] or ""
        item["content_models"][model] = item["content_models"].get(model, 0) + 1
        if len(item["sample_titles"]) < 3:
            item["sample_titles"].append(row["title"])
        body = row["wikitext"].lstrip() if isinstance(row["wikitext"], str) else ""
        normalized_model = model.casefold()
        expects_json = normalized_model in {"bson", "json", "tabular"} or "表格" in model
        if not expects_json:
            item["non_json_page_count"] += 1
            continue
        try:
            value = json.loads(body)
        except (json.JSONDecodeError, TypeError) as error:
            parse_issues.append(
                {
                    "page_id": row["page_id"],
                    "title": row["title"],
                    "revision_id": row["revision_id"],
                    "error": str(error),
                }
            )
            continue
        item["json_page_count"] += 1
        if isinstance(value, dict):
            item["json_object_keys"].update(value.keys())
            schema = value.get("schema")
            if isinstance(schema, dict) and isinstance(schema.get("fields"), list):
                for field in schema["fields"]:
                    if isinstance(field, dict) and isinstance(field.get("name"), str):
                        item["tabular_fields"].add(field["name"])
            data = value.get("data")
            if isinstance(data, list):
                item["tabular_row_count"] += len(data)

    serialized_families = []
    for item in families.values():
        item["content_models"] = dict(sorted(item["content_models"].items()))
        item["json_object_keys"] = sorted(item["json_object_keys"])
        item["tabular_fields"] = sorted(item["tabular_fields"])
        serialized_families.append(item)
    return {
        "source_id": source_id,
        "namespace_id": namespace_id,
        "page_count": len(rows),
        "families": serialized_families,
        "parse_issue_count": len(parse_issues),
        "parse_issues": parse_issues[:issue_limit],
        "parse_issues_truncated": len(parse_issues) > issue_limit,
    }


def list_pages(
    *,
    database: Path,
    source_id: str,
    namespace_id: int | None = None,
    limit: int = 100,
) -> list[dict[str, Any]]:
    connection = sqlite3.connect(f"file:{database.resolve().as_posix()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        clauses = ["source_id = ?"]
        parameters: list[Any] = [source_id]
        if namespace_id is not None:
            clauses.append("namespace_id = ?")
            parameters.append(namespace_id)
        parameters.append(limit)
        return [
            dict(row)
            for row in connection.execute(
                f"""
                SELECT source_id,page_id,namespace_id,title,url,revision_id,
                       revision_timestamp,redirect_target
                FROM pages
                WHERE {' AND '.join(clauses)}
                ORDER BY namespace_id,title
                LIMIT ?
                """,
                parameters,
            )
        ]
    finally:
        connection.close()


def read_page(
    *, database: Path, source_id: str, title: str, metadata_only: bool = False
) -> dict[str, Any] | None:
    connection = sqlite3.connect(f"file:{database.resolve().as_posix()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        body_columns = (
            "length(plain_text) AS plain_text_length,"
            "length(wikitext) AS wikitext_length"
            if metadata_only
            else "plain_text,wikitext"
        )
        row = connection.execute(
            f"""
            SELECT source_id,page_id,namespace_id,title,url,revision_id,
                   revision_timestamp,revision_sha1,content_model,redirect_target,
                   {body_columns},acquisition_warnings_json,acquisition_errors_json
            FROM pages
            WHERE source_id = ? AND title = ?
            """,
            (source_id, title),
        ).fetchone()
        if row is None:
            return None
        page = dict(row)
        page["acquisition_warnings"] = json.loads(page.pop("acquisition_warnings_json"))
        page["acquisition_errors"] = json.loads(page.pop("acquisition_errors_json"))
        for table, key in (("categories", "categories"), ("templates", "templates")):
            column = "category" if table == "categories" else "template"
            page[key] = [
                value[0]
                for value in connection.execute(
                    f"SELECT {column} FROM {table} WHERE source_id=? AND page_id=? ORDER BY {column}",
                    (source_id, page["page_id"]),
                )
            ]
        if metadata_only:
            page["internal_link_count"] = connection.execute(
                "SELECT COUNT(*) FROM links WHERE source_id=? AND page_id=?",
                (source_id, page["page_id"]),
            ).fetchone()[0]
        else:
            page["internal_links"] = [
                value[0]
                for value in connection.execute(
                    """
                    SELECT target_title FROM links
                    WHERE source_id=? AND page_id=? ORDER BY target_title
                    """,
                    (source_id, page["page_id"]),
                )
            ]
        return page
    finally:
        connection.close()


def read_page_sections(
    *,
    database: Path,
    source_id: str,
    title: str,
    expected_revision_id: int | None = None,
    expected_revision_sha1: str | None = None,
    include_plain_text: bool = False,
) -> dict[str, Any] | None:
    connection = sqlite3.connect(f"file:{database.resolve().as_posix()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        row = connection.execute(
            """
            SELECT source_id,page_id,namespace_id,title,url,revision_id,
                   revision_timestamp,revision_sha1,content_model,redirect_target,wikitext
            FROM pages WHERE source_id=? AND title=?
            """,
            (source_id, title),
        ).fetchone()
    finally:
        connection.close()
    if row is None:
        return None
    page = dict(row)
    if (
        expected_revision_id is not None
        and page["revision_id"] != expected_revision_id
    ):
        raise ValueError(
            "Wiki page revision changed while reading sections: "
            f"{source_id}:{title}:revision_id"
        )
    if (
        expected_revision_sha1 is not None
        and page["revision_sha1"] != expected_revision_sha1
    ):
        raise ValueError(
            "Wiki page revision changed while reading sections: "
            f"{source_id}:{title}:revision_sha1"
        )
    wikitext = page.pop("wikitext")
    outline = extract_section_outline(wikitext)
    if include_plain_text:
        for section in outline["sections"]:
            section["own_plain_text"] = plain_text_from_wikitext(
                wikitext[section["body_start"] : section["own_body_end"]]
            )
            section["subtree_plain_text"] = plain_text_from_wikitext(
                wikitext[section["body_start"] : section["subtree_end"]]
            )
    return {
        "schema": PAGE_SECTION_OUTLINE_SCHEMA,
        **page,
        **outline,
    }


def read_data_table(
    *,
    database: Path,
    source_id: str,
    title: str,
    columns: list[str] | None = None,
    offset: int = 0,
    limit: int = 20,
) -> dict[str, Any]:
    """Read selected rows from one JSON tabular page with source provenance."""

    if offset < 0 or limit < 0:
        raise ValueError("table offset and limit must be non-negative")

    connection = sqlite3.connect(f"file:{database.resolve().as_posix()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        page = connection.execute(
            """
            SELECT page_id,title,revision_id,revision_timestamp,revision_sha1,wikitext
            FROM pages WHERE source_id=? AND title=?
            """,
            (source_id, title),
        ).fetchone()
    finally:
        connection.close()
    if page is None:
        raise ValueError(f"page was not found: {source_id}:{title}")
    try:
        value = json.loads(page["wikitext"])
    except json.JSONDecodeError as error:
        raise ValueError(f"page is not valid JSON: {source_id}:{title}: {error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"tabular page root is not an object: {source_id}:{title}")
    schema = value.get("schema")
    fields = schema.get("fields") if isinstance(schema, dict) else None
    data = value.get("data")
    if not isinstance(fields, list) or not isinstance(data, list):
        raise ValueError(f"page does not contain tabular schema/data: {source_id}:{title}")
    field_names = [
        field.get("name") if isinstance(field, dict) else None for field in fields
    ]
    if not all(isinstance(name, str) and name for name in field_names):
        raise ValueError(f"tabular page has invalid field names: {source_id}:{title}")
    selected = columns or list(field_names)
    missing = [column for column in selected if column not in field_names]
    if missing:
        raise ValueError(
            f"tabular columns were not found in {source_id}:{title}: {', '.join(missing)}"
        )
    indices = [field_names.index(column) for column in selected]
    end = min(len(data), offset + limit)
    output_rows = []
    for row_number, row in enumerate(data[offset:end], start=offset):
        if not isinstance(row, list) or len(row) != len(field_names):
            raise ValueError(
                f"tabular row {row_number} does not match schema in {source_id}:{title}"
            )
        output_rows.append({column: row[index] for column, index in zip(selected, indices)})
    return {
        "source_id": source_id,
        "page_id": page["page_id"],
        "title": title,
        "revision_id": page["revision_id"],
        "revision_timestamp": page["revision_timestamp"],
        "revision_sha1": page["revision_sha1"],
        "fields": field_names,
        "selected_columns": selected,
        "row_count": len(data),
        "offset": offset,
        "rows": output_rows,
        "truncated": end < len(data),
    }
