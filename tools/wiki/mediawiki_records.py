"""Typed records and normalization for complete MediaWiki text exports.

Acquisition transports deliberately live elsewhere.  This module accepts the
same JSONL bundle whether it came from a direct HTTP client, a browser
userscript, or a future official dump adapter.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from datetime import datetime, timezone
import gzip
import hashlib
import html
import json
from pathlib import Path
import re
from typing import Any, Iterable, Iterator, TextIO


BUNDLE_SCHEMA = "mediawiki-text-export-v1"
NORMALIZED_SCHEMA = "mediawiki-normalized-page-v1"


class BundleError(ValueError):
    """Raised when an export is incomplete, conflicting, or malformed."""


@dataclass(frozen=True)
class VerifiedBundle:
    path: Path
    header: dict[str, Any]
    trailer: dict[str, Any]
    page_count: int
    namespace_counts: dict[str, int]
    acquisition_warning_pages: int
    acquisition_error_pages: int
    sha256: str


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def page_ids_sha256(page_ids: Iterable[int]) -> str:
    payload = "".join(f"{page_id}\n" for page_id in sorted(page_ids)).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _open_text(path: Path) -> TextIO:
    if path.suffix.casefold() == ".gz":
        return gzip.open(path, "rt", encoding="utf-8", newline="")
    return path.open("r", encoding="utf-8", newline="")


def iter_records(path: Path) -> Iterator[tuple[int, dict[str, Any]]]:
    with _open_text(path) as source:
        for line_number, line in enumerate(source, start=1):
            if not line.strip():
                continue
            try:
                record = json.loads(line)
            except json.JSONDecodeError as error:
                raise BundleError(f"{path}:{line_number}: invalid JSON: {error}") from error
            if not isinstance(record, dict):
                raise BundleError(f"{path}:{line_number}: record must be an object")
            yield line_number, record


def verify_bundle(path: Path) -> VerifiedBundle:
    header: dict[str, Any] | None = None
    trailer: dict[str, Any] | None = None
    page_ids: set[int] = set()
    titles: set[tuple[int, str]] = set()
    namespace_counts: Counter[str] = Counter()
    acquisition_warning_pages = 0
    acquisition_error_pages = 0
    saw_trailer = False

    for line_number, record in iter_records(path):
        kind = record.get("kind")
        if header is None:
            if kind != "header" or record.get("schema") != BUNDLE_SCHEMA:
                raise BundleError(
                    f"{path}:{line_number}: first record must be a {BUNDLE_SCHEMA} header"
                )
            header = record
            namespace_ids = record.get("namespace_ids")
            if not isinstance(namespace_ids, list) or not all(
                isinstance(namespace_id, int) and namespace_id >= 0
                for namespace_id in namespace_ids
            ):
                raise BundleError(f"{path}:{line_number}: invalid namespace_ids")
            if namespace_ids != sorted(set(namespace_ids)):
                raise BundleError(
                    f"{path}:{line_number}: namespace_ids must be sorted and unique"
                )
            continue

        if saw_trailer:
            raise BundleError(f"{path}:{line_number}: record appears after complete trailer")

        if kind == "page":
            page = record.get("page")
            if not isinstance(page, dict):
                raise BundleError(f"{path}:{line_number}: page record lacks page object")
            page_id = page.get("pageid")
            namespace_id = page.get("ns")
            title = page.get("title")
            if not isinstance(page_id, int) or page_id < 0:
                raise BundleError(f"{path}:{line_number}: invalid pageid")
            if not isinstance(namespace_id, int) or namespace_id < 0:
                raise BundleError(f"{path}:{line_number}: invalid namespace id")
            if not isinstance(title, str) or not title.strip():
                raise BundleError(f"{path}:{line_number}: invalid page title")
            if namespace_id not in header["namespace_ids"]:
                raise BundleError(
                    f"{path}:{line_number}: page namespace {namespace_id} was not requested"
                )
            if page_id in page_ids:
                raise BundleError(f"{path}:{line_number}: duplicate pageid {page_id}")
            # MediaWiki's common `case=first-letter` policy does not make the
            # remainder of a title case-insensitive.  A redirect named
            # `Colorless cards` may coexist with `Colorless Cards`.
            title_key = (namespace_id, title)
            if title_key in titles:
                raise BundleError(
                    f"{path}:{line_number}: duplicate namespace/title {namespace_id}:{title}"
                )
            page_ids.add(page_id)
            titles.add(title_key)
            namespace_counts[str(namespace_id)] += 1
            for field in ("acquisition_warnings", "acquisition_errors"):
                issues = page.get(field, [])
                if not isinstance(issues, list) or not all(
                    isinstance(issue, dict)
                    and isinstance(issue.get("stage"), str)
                    and isinstance(issue.get("code"), str)
                    and isinstance(issue.get("message"), str)
                    for issue in issues
                ):
                    raise BundleError(f"{path}:{line_number}: invalid {field}")
            acquisition_warning_pages += bool(page.get("acquisition_warnings"))
            acquisition_error_pages += bool(page.get("acquisition_errors"))
            continue

        if kind == "complete":
            trailer = record
            saw_trailer = True
            continue

        raise BundleError(f"{path}:{line_number}: unknown record kind {kind!r}")

    if header is None:
        raise BundleError(f"{path}: export is empty")
    if trailer is None:
        raise BundleError(f"{path}: export has no complete trailer")
    if trailer.get("schema") != BUNDLE_SCHEMA:
        raise BundleError(f"{path}: trailer schema does not match header")
    if trailer.get("source_id") != header.get("source_id"):
        raise BundleError(f"{path}: trailer source_id does not match header")
    if trailer.get("page_count") != len(page_ids):
        raise BundleError(
            f"{path}: trailer page_count={trailer.get('page_count')} but read {len(page_ids)}"
        )
    expected_counts = {str(key): int(value) for key, value in namespace_counts.items()}
    if trailer.get("namespace_counts") != expected_counts:
        raise BundleError(
            f"{path}: trailer namespace_counts do not match observed pages"
        )
    observed_digest = page_ids_sha256(page_ids)
    if trailer.get("page_ids_sha256") != observed_digest:
        raise BundleError(f"{path}: trailer page_ids_sha256 does not match observed pages")
    for field, observed in (
        ("acquisition_warning_pages", acquisition_warning_pages),
        ("acquisition_error_pages", acquisition_error_pages),
    ):
        if field in trailer and trailer[field] != observed:
            raise BundleError(f"{path}: trailer {field} does not match observed pages")

    return VerifiedBundle(
        path=path,
        header=header,
        trailer=trailer,
        page_count=len(page_ids),
        namespace_counts=expected_counts,
        acquisition_warning_pages=acquisition_warning_pages,
        acquisition_error_pages=acquisition_error_pages,
        sha256=file_sha256(path),
    )


def iter_pages(path: Path) -> Iterator[dict[str, Any]]:
    for _, record in iter_records(path):
        if record.get("kind") == "page":
            yield record["page"]


def _revision_content(page: dict[str, Any]) -> tuple[str, dict[str, Any]]:
    revisions = page.get("revisions")
    if not isinstance(revisions, list) or not revisions:
        return "", {}
    revision = revisions[0]
    if not isinstance(revision, dict):
        return "", {}
    slots = revision.get("slots")
    if isinstance(slots, dict) and isinstance(slots.get("main"), dict):
        slot = slots["main"]
        content = slot.get("content", slot.get("*", ""))
        return (content if isinstance(content, str) else ""), revision
    content = revision.get("content", revision.get("*", ""))
    return (content if isinstance(content, str) else ""), revision


_COMMENT_RE = re.compile(r"<!--.*?-->", re.DOTALL)
_REF_RE = re.compile(r"<ref\b[^>]*>.*?</ref\s*>|<ref\b[^>]*/\s*>", re.I | re.DOTALL)
_TAG_RE = re.compile(r"<[^>]+>")
_EXTERNAL_LINK_RE = re.compile(r"\[(?:https?|steam)://[^\s\]]+(?:\s+([^\]]+))?\]", re.I)
_INTERNAL_LINK_RE = re.compile(r"\[\[([^\[\]]+?)\]\]")
_TEMPLATE_RE = re.compile(r"\{\{([^{}]+)\}\}", re.DOTALL)
_HEADING_RE = re.compile(r"^\s*=+\s*(.*?)\s*=+\s*$", re.MULTILINE)
_SECTION_HEADING_RE = re.compile(
    r"^\s*(?P<marks>={1,6})\s*(?P<title>.*?)\s*(?P=marks)\s*$", re.MULTILINE
)
_REDIRECT_RE = re.compile(
    r"^\s*#(?:redirect|重定向)\s*:?\s*\[\[([^\]|#]+)", re.I
)

_DISPLAY_TEMPLATE_NAMES = {
    "c",
    "card",
    "card_link",
    "character_link",
    "e",
    "event",
    "event_link",
    "icon",
    "kw",
    "keyword",
    "keyword_link",
    "m",
    "monster",
    "monster_link",
    "orb_link",
    "p",
    "potion",
    "potion_link",
    "power_link",
    "querylink",
    "r",
    "relic",
    "relic_link",
    "stance_link",
}


def _namespace_names(siteinfo: dict[str, Any], namespace_id: int) -> set[str]:
    names: set[str] = set()
    namespaces = siteinfo.get("namespaces", {})
    if isinstance(namespaces, list):
        entries = namespaces
    elif isinstance(namespaces, dict):
        entries = list(namespaces.values())
    else:
        entries = []
    for entry in entries:
        if not isinstance(entry, dict) or entry.get("id") != namespace_id:
            continue
        for key in ("name", "canonical", "*"):
            value = entry.get(key)
            if isinstance(value, str) and value:
                names.add(value.casefold())
    for alias in siteinfo.get("namespacealiases", []):
        if isinstance(alias, dict) and alias.get("id") == namespace_id:
            value = alias.get("alias", alias.get("*"))
            if isinstance(value, str) and value:
                names.add(value.casefold())
    return names


def _split_link(value: str) -> tuple[str, str]:
    parts = value.split("|", 1)
    target = parts[0].strip()
    label = parts[1].strip() if len(parts) == 2 else target
    return target, label


def extract_internal_links(wikitext: str) -> list[str]:
    links: set[str] = set()
    for match in _INTERNAL_LINK_RE.finditer(wikitext):
        target, _ = _split_link(match.group(1))
        target = target.split("#", 1)[0].strip()
        if target:
            links.add(target)
    return sorted(links, key=str.casefold)


def extract_prefixed_links(wikitext: str, prefixes: set[str]) -> list[str]:
    values: set[str] = set()
    for target in extract_internal_links(wikitext):
        prefix, separator, remainder = target.partition(":")
        if separator and prefix.casefold() in prefixes and remainder.strip():
            values.add(remainder.strip())
    return sorted(values, key=str.casefold)


def extract_templates(wikitext: str) -> list[str]:
    templates: set[str] = set()
    for match in re.finditer(r"\{\{\s*([^{}|\n]+)", wikitext):
        name = match.group(1).strip()
        if name and not name.startswith("#"):
            templates.add(name)
    return sorted(templates, key=str.casefold)


def plain_text_from_wikitext(wikitext: str) -> str:
    text = _COMMENT_RE.sub(" ", wikitext)
    text = _REF_RE.sub(" ", text)
    # Remove innermost templates repeatedly.  This is intentionally a search
    # projection, not a claim to implement the MediaWiki parser.
    def replace_template(match: re.Match[str]) -> str:
        parts = [part.strip() for part in match.group(1).split("|")]
        if not parts:
            return " "
        if parts[0].casefold() not in _DISPLAY_TEMPLATE_NAMES:
            return " "
        positional = [part for part in parts[1:] if part and "=" not in part]
        return positional[-1] if positional else " "

    for _ in range(8):
        updated = _TEMPLATE_RE.sub(replace_template, text)
        if updated == text:
            break
        text = updated

    def replace_internal(match: re.Match[str]) -> str:
        target, label = _split_link(match.group(1))
        if target.partition(":")[0].casefold() in {
            "category",
            "分类",
            "file",
            "文件",
            "image",
        }:
            return " "
        return label

    text = _INTERNAL_LINK_RE.sub(replace_internal, text)
    text = _EXTERNAL_LINK_RE.sub(lambda match: match.group(1) or " ", text)
    text = _HEADING_RE.sub(lambda match: f"\n{match.group(1)}\n", text)
    text = _TAG_RE.sub(" ", text)
    text = html.unescape(text)
    text = re.sub(r"'{2,5}", "", text)
    text = re.sub(r"[ \t\f\v]+", " ", text)
    text = re.sub(r"\n\s*\n+", "\n\n", text)
    return text.strip()


def extract_section_outline(wikitext: str) -> dict[str, Any]:
    """Return revision-local locators for explicit wikitext headings only."""
    matches = list(_SECTION_HEADING_RE.finditer(wikitext))
    sections: list[dict[str, Any]] = []
    stack: list[dict[str, Any]] = []
    for position, match in enumerate(matches):
        level = len(match.group("marks"))
        raw_title = match.group("title").strip()
        projected_title = plain_text_from_wikitext(raw_title)
        title = projected_title or raw_title
        while stack and stack[-1]["level"] >= level:
            stack.pop()
        own_body_end = (
            matches[position + 1].start()
            if position + 1 < len(matches)
            else len(wikitext)
        )
        subtree_end = len(wikitext)
        for later in matches[position + 1 :]:
            if len(later.group("marks")) <= level:
                subtree_end = later.start()
                break
        own_body = wikitext[match.end() : own_body_end]
        item = {
            "document_order": position + 1,
            "level": level,
            "title": title,
            "raw_title": raw_title,
            "title_projection_fallback": not bool(projected_title),
            "path": [ancestor["title"] for ancestor in stack] + [title],
            "parent_document_order": stack[-1]["document_order"] if stack else None,
            "heading_start": match.start(),
            "body_start": match.end(),
            "own_body_end": own_body_end,
            "subtree_end": subtree_end,
            "own_plain_text_length": len(plain_text_from_wikitext(own_body)),
        }
        sections.append(item)
        stack.append(item)
    lead_end = matches[0].start() if matches else len(wikitext)
    return {
        "extraction": "explicit_wikitext_headings_only",
        "lead_plain_text_length": len(
            plain_text_from_wikitext(wikitext[:lead_end])
        ),
        "section_count": len(sections),
        "sections": sections,
    }


def normalize_page(
    page: dict[str, Any], *, source_id: str, siteinfo: dict[str, Any]
) -> dict[str, Any]:
    wikitext, revision = _revision_content(page)
    category_names = _namespace_names(siteinfo, 14) | {"category", "分类"}
    template_names = _namespace_names(siteinfo, 10) | {"template", "模板"}
    redirect = _REDIRECT_RE.search(wikitext)
    internal_links = extract_internal_links(wikitext)

    return {
        "schema": NORMALIZED_SCHEMA,
        "source_id": source_id,
        "page_id": page["pageid"],
        "namespace_id": page["ns"],
        "title": page["title"],
        "url": page.get("fullurl") or page.get("canonicalurl"),
        "revision_id": revision.get("revid", page.get("lastrevid")),
        "revision_parent_id": revision.get("parentid"),
        "revision_timestamp": revision.get("timestamp"),
        "revision_sha1": revision.get("sha1"),
        "revision_size": revision.get("size", page.get("length")),
        "content_model": (
            revision.get("slots", {}).get("main", {}).get("contentmodel")
            if isinstance(revision.get("slots"), dict)
            else revision.get("contentmodel", page.get("contentmodel"))
        ),
        "redirect_target": redirect.group(1).strip() if redirect else None,
        "categories": extract_prefixed_links(wikitext, category_names),
        "templates": sorted(
            set(extract_templates(wikitext))
            | set(extract_prefixed_links(wikitext, template_names)),
            key=str.casefold,
        ),
        "internal_links": internal_links,
        "plain_text": "\n\n".join(
            part for part in (page["title"], plain_text_from_wikitext(wikitext)) if part
        ),
        "wikitext": wikitext,
        "acquisition_warnings": (
            page.get("acquisition_warnings")
            if isinstance(page.get("acquisition_warnings"), list)
            else []
        ),
        "acquisition_errors": (
            page.get("acquisition_errors")
            if isinstance(page.get("acquisition_errors"), list)
            else []
        ),
    }
