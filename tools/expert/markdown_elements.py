from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import Counter
from pathlib import Path
from typing import Any, Iterable

import markdown_it
from markdown_it import MarkdownIt
from markdown_it.token import Token

from .store import ExpertReferenceStore, sha256


DOCUMENT_SCHEMA = "expert-document-elements-v1"
_LINE_LOCATOR = re.compile(r"^lines\s+(\d+)-(\d+)$", re.IGNORECASE)
_TRAILING_COLON = re.compile(r"\s*[:：]\s*$")
_LEADING_COLON = re.compile(r"^\s*[:：]")
_MULTI_LABEL_SEPARATOR = re.compile(r"\s*/\s*")
_TRAILING_AUTHORED_QUALIFIER = re.compile(r"\s+\((?:good|bad)\)\s*$", re.IGNORECASE)


def _inline_text(tokens: Iterable[Token]) -> str:
    parts: list[str] = []
    for token in tokens:
        if token.type in {"text", "code_inline"}:
            parts.append(token.content)
        elif token.type in {"softbreak", "hardbreak"}:
            parts.append("\n")
        elif token.type == "image":
            parts.append(_inline_text(token.children or []))
        elif token.children:
            parts.append(_inline_text(token.children))
    return "".join(parts)


def _authored_label(inline: Token | None) -> str | None:
    children = list(inline.children or []) if inline is not None else []
    while children and children[0].type == "text" and not children[0].content:
        children.pop(0)
    if not children or children[0].type != "strong_open":
        return None
    depth = 0
    label_tokens: list[Token] = []
    closing_index = None
    for index, token in enumerate(children):
        if token.type == "strong_open":
            depth += 1
            if index > 0:
                label_tokens.append(token)
        elif token.type == "strong_close":
            depth -= 1
            if depth == 0:
                closing_index = index
                break
            label_tokens.append(token)
        elif depth > 0:
            label_tokens.append(token)
    if closing_index is None:
        return None
    label = _inline_text(label_tokens).strip()
    colon_in_label = bool(_TRAILING_COLON.search(label))
    colon_after_label = (
        closing_index + 1 < len(children)
        and children[closing_index + 1].type == "text"
        and bool(_LEADING_COLON.search(children[closing_index + 1].content))
    )
    if not (colon_in_label or colon_after_label):
        return None
    label = _TRAILING_COLON.sub("", label).strip()
    return label or None


def _resolve_label(label: str, aliases: dict[str, str]) -> tuple[list[str], str]:
    direct = aliases.get(label.casefold())
    if direct is not None:
        return [direct], "exact_alias"
    unqualified = _TRAILING_AUTHORED_QUALIFIER.sub("", label).strip()
    if unqualified != label:
        direct = aliases.get(unqualified.casefold())
        if direct is not None:
            return [direct], "exact_alias_after_authored_qualifier"
    components = [item.strip() for item in _MULTI_LABEL_SEPARATOR.split(label)]
    if len(components) > 1:
        resolved = [aliases.get(item.casefold()) for item in components]
        if all(item is not None for item in resolved):
            return list(dict.fromkeys(str(item) for item in resolved)), "split_exact_alias"
    return [], "unresolved"


def _resolve_ambiguous_label_in_kind(
    label: str,
    *,
    ambiguous_aliases: dict[str, list[str]],
    entities_by_id: dict[str, dict[str, Any]],
    entity_kind: str | None,
) -> tuple[list[str], str]:
    if entity_kind is None:
        return [], "unresolved"
    variants = [(label, "exact_ambiguous_alias_in_reviewed_record_context")]
    unqualified = _TRAILING_AUTHORED_QUALIFIER.sub("", label).strip()
    if unqualified != label:
        variants.append(
            (
                unqualified,
                "exact_ambiguous_alias_after_authored_qualifier_in_reviewed_record_context",
            )
        )
    for value, resolution in variants:
        owners = ambiguous_aliases.get(value.casefold(), [])
        matching = [
            entity_id
            for entity_id in owners
            if entities_by_id[entity_id].get("kind") == entity_kind
        ]
        if len(matching) == 1:
            return matching, resolution
    return [], "unresolved"


def _record_ranges(records: Iterable[dict[str, Any]]) -> tuple[list[tuple[int, int, str]], dict[str, str]]:
    ranges = []
    titles = {}
    for record in records:
        record_id = str(record.get("id", ""))
        locator = str(record.get("locator", ""))
        match = _LINE_LOCATOR.fullmatch(locator)
        if record_id and match:
            ranges.append((int(match.group(1)), int(match.group(2)), record_id))
        title = str(record.get("title", "")).strip().casefold()
        if title and record_id:
            titles[title] = record_id
    return ranges, titles


def _record_id_for(
    *,
    start_line: int,
    kind: str,
    text: str,
    ranges: list[tuple[int, int, str]],
    title_records: dict[str, str],
) -> str | None:
    if kind == "heading":
        titled = title_records.get(text.strip().casefold())
        if titled is not None:
            return titled
    for first, last, record_id in ranges:
        if first <= start_line <= last:
            return record_id
    return None


def _stable_element_id(
    source_id: str,
    record_id: str | None,
    kind: str,
    markdown: str,
    occurrence: int,
) -> str:
    material = "\0".join((record_id or "root", kind, markdown, str(occurrence)))
    suffix = hashlib.sha256(material.encode("utf-8")).hexdigest()[:16]
    return f"{source_id}-element-{suffix}"


def _safe_asset_ref(
    *,
    source_root: Path,
    text_path: Path,
    source_url: str,
    alt: str,
    declared_assets: dict[str, dict[str, Any]],
) -> dict[str, str] | None:
    if not source_url or "://" in source_url or source_url.startswith("data:"):
        return None
    resolved = (text_path.parent / source_url).resolve(strict=False)
    if not resolved.is_relative_to(source_root):
        return None
    relative = resolved.relative_to(source_root).as_posix()
    declared = declared_assets.get(relative)
    if declared is None or not resolved.is_file():
        return None
    result = {
        "kind": "image",
        "path": relative,
        "relation": "inline_source_asset",
    }
    if alt:
        result["alt"] = alt
    media_type = declared.get("media_type")
    if isinstance(media_type, str) and media_type:
        result["media_type"] = media_type
    return result


def build_markdown_elements(repository: Path, source_id: str) -> dict[str, Any]:
    repository = repository.resolve(strict=True)
    store = ExpertReferenceStore(
        repository,
        rebuilding_derived_sources=frozenset({source_id}),
    )
    source = store.require_source(source_id)
    manifest = source.manifest
    canonical_text = manifest["canonical_corpus"]["text"]
    text = source.text_path.read_text(encoding="utf-8")
    lines = text.splitlines()
    parser = MarkdownIt("commonmark").enable("table")
    tokens = parser.parse(text)
    record_ranges, title_records = _record_ranges(source.records)
    record_entity_kind_hints = store.reviewed_record_entity_kind_hints(source_id)
    declared_assets = {
        item["path"]: item
        for item in manifest.get("assets", [])
        if isinstance(item, dict) and isinstance(item.get("path"), str)
    }

    heading_stack: list[tuple[int, str, str]] = []
    elements: list[dict[str, Any]] = []
    content_occurrences: Counter[tuple[str | None, str, str]] = Counter()

    def add_element(
        *,
        kind: str,
        token: Token,
        plain_text: str,
        parent_id: str | None,
        heading_path: list[str],
        authored_label: dict[str, Any] | None = None,
        asset_refs: list[dict[str, str]] | None = None,
    ) -> dict[str, Any] | None:
        if token.map is None:
            return None
        first = token.map[0] + 1
        last = token.map[1]
        markdown = "\n".join(lines[token.map[0] : token.map[1]])
        record_id = _record_id_for(
            start_line=first,
            kind=kind,
            text=plain_text,
            ranges=record_ranges,
            title_records=title_records,
        )
        occurrence_key = (record_id, kind, markdown)
        occurrence = content_occurrences[occurrence_key]
        content_occurrences[occurrence_key] += 1
        element = {
            "id": _stable_element_id(
                source_id, record_id, kind, markdown, occurrence
            ),
            "ordinal": len(elements),
            "kind": kind,
            "record_id": record_id,
            "parent_id": parent_id,
            "heading_path": heading_path,
            "source_span": {"start_line": first, "end_line": last},
            "text": plain_text.strip(),
            "markdown": markdown,
            "entity_ids": (
                authored_label.get("entity_ids", []) if authored_label else []
            ),
            "topic_ids": (
                authored_label.get("topic_ids", []) if authored_label else []
            ),
        }
        if authored_label is not None:
            element["authored_label"] = authored_label
        if asset_refs:
            element["asset_refs"] = asset_refs
        elements.append(element)
        return element

    index = 0
    while index < len(tokens):
        token = tokens[index]
        next_token = tokens[index + 1] if index + 1 < len(tokens) else None
        if token.type == "heading_open" and token.map is not None:
            level = int(token.tag[1:])
            heading_text = (
                _inline_text(next_token.children or [])
                if next_token is not None and next_token.type == "inline"
                else ""
            ).strip()
            while heading_stack and heading_stack[-1][0] >= level:
                heading_stack.pop()
            parent_id = heading_stack[-1][2] if heading_stack else None
            heading_path = [item[1] for item in heading_stack] + [heading_text]
            element = add_element(
                kind="heading",
                token=token,
                plain_text=heading_text,
                parent_id=parent_id,
                heading_path=heading_path,
            )
            if element is not None:
                heading_stack.append((level, heading_text, element["id"]))
        elif token.type == "paragraph_open" and token.map is not None:
            inline = next_token if next_token is not None and next_token.type == "inline" else None
            plain_text = _inline_text(inline.children or []) if inline is not None else ""
            label = _authored_label(inline)
            label_record = None
            if label is not None:
                entity_ids, resolution = _resolve_label(label, store.entity_aliases)
                record_id = _record_id_for(
                    start_line=token.map[0] + 1,
                    kind="paragraph",
                    text=plain_text,
                    ranges=record_ranges,
                    title_records=title_records,
                )
                resolution_context = None
                if not entity_ids:
                    entity_kind = record_entity_kind_hints.get(record_id or "")
                    entity_ids, resolution = _resolve_ambiguous_label_in_kind(
                        label,
                        ambiguous_aliases=store.ambiguous_entity_aliases,
                        entities_by_id=store.entities_by_id,
                        entity_kind=entity_kind,
                    )
                    if entity_ids:
                        resolution_context = {
                            "basis": "reviewed_source_outline_decisions",
                            "record_id": record_id,
                            "entity_kind": entity_kind,
                        }
                topic_ids, topic_resolution = _resolve_label(
                    label,
                    store.topic_aliases,
                )
                label_record = {
                    "text": label,
                    "resolution": resolution,
                    "entity_ids": entity_ids,
                    "topic_resolution": topic_resolution,
                    "topic_ids": topic_ids,
                }
                if resolution_context is not None:
                    label_record["resolution_context"] = resolution_context
            parent_id = heading_stack[-1][2] if heading_stack else None
            heading_path = [item[1] for item in heading_stack]
            paragraph = add_element(
                kind="paragraph",
                token=token,
                plain_text=plain_text,
                parent_id=parent_id,
                heading_path=heading_path,
                authored_label=label_record,
            )
            if inline is not None and paragraph is not None:
                for child in inline.children or []:
                    if child.type != "image":
                        continue
                    asset = _safe_asset_ref(
                        source_root=source.root,
                        text_path=source.text_path,
                        source_url=child.attrGet("src") or "",
                        alt=_inline_text(child.children or []).strip(),
                        declared_assets=declared_assets,
                    )
                    if asset is not None:
                        add_element(
                            kind="image",
                            token=token,
                            plain_text=asset.get("alt", ""),
                            parent_id=paragraph["id"],
                            heading_path=heading_path,
                            asset_refs=[asset],
                        )
        elif token.type == "table_open" and token.map is not None:
            parent_id = heading_stack[-1][2] if heading_stack else None
            add_element(
                kind="table",
                token=token,
                plain_text="\n".join(lines[token.map[0] : token.map[1]]),
                parent_id=parent_id,
                heading_path=[item[1] for item in heading_stack],
            )
        elif token.type in {"fence", "code_block"} and token.map is not None:
            parent_id = heading_stack[-1][2] if heading_stack else None
            add_element(
                kind="code",
                token=token,
                plain_text=token.content,
                parent_id=parent_id,
                heading_path=[item[1] for item in heading_stack],
            )
        index += 1

    unresolved_labels = [
        element["authored_label"]["text"]
        for element in elements
        if element.get("authored_label", {}).get("resolution") == "unresolved"
        and element.get("authored_label", {}).get("topic_resolution")
        == "unresolved"
    ]
    return {
        "schema": DOCUMENT_SCHEMA,
        "source_id": source_id,
        "input": {
            "path": canonical_text["path"],
            "sha256": sha256(source.text_path),
        },
        "parser": {
            "name": "markdown-it-py",
            "version": markdown_it.__version__,
            "preset": "commonmark",
            "enabled_rules": ["table"],
        },
        "entity_catalog": {
            "status": store.game_entity_catalog_status,
            "resolved_entity_count": len(store.entities),
        },
        "statistics": {
            "element_count": len(elements),
            "kind_counts": dict(sorted(Counter(item["kind"] for item in elements).items())),
            "authored_label_count": sum("authored_label" in item for item in elements),
            "resolved_authored_label_count": sum(bool(item["entity_ids"]) for item in elements),
            "resolved_authored_topic_label_count": sum(
                bool(item["topic_ids"]) for item in elements
            ),
            "unresolved_authored_labels": unresolved_labels,
        },
        "elements": elements,
    }


def write_markdown_elements(
    repository: Path,
    source_id: str,
    output: Path | None = None,
) -> dict[str, Any]:
    repository = repository.resolve(strict=True)
    source_root = (
        repository
        / ".oracle-lab"
        / "external-research"
        / "sources"
        / source_id
    ).resolve(strict=True)
    output_path = (
        output.resolve()
        if output is not None
        else source_root / "corpus" / "elements.json"
    )
    if not output_path.is_relative_to(source_root):
        raise ValueError("Markdown element output must stay inside the source directory")
    document = build_markdown_elements(repository, source_id)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(document, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return {
        "source_id": source_id,
        "output": str(output_path),
        "sha256": sha256(output_path),
        **document["statistics"],
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="build a provenance-preserving element tree for one admitted Markdown source"
    )
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    parser.add_argument("--source", required=True)
    parser.add_argument("--output", type=Path)
    return parser


def main(argv: list[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    summary = write_markdown_elements(
        arguments.repository,
        arguments.source,
        arguments.output,
    )
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
