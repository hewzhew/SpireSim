from __future__ import annotations

import json
from pathlib import Path
from typing import Any
import unicodedata


def _key(value: str) -> str:
    """Normalize spelling separators for exact subject lookup."""
    folded = unicodedata.normalize("NFKC", value).casefold()
    return "".join(character for character in folded if not character.isspace() and character not in "_-'’")


def _read(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def _read_page(path: Path, repository: Path) -> dict[str, Any]:
    """Read a subject page by its headings; the prose and source links stay editable."""
    lines = path.read_text(encoding="utf-8").splitlines()
    title = next((line[2:].strip() for line in lines if line.startswith("# ")), None)
    if not title:
        raise ValueError(f"missing subject title: {path}")
    sections: dict[str, list[str]] = {}
    heading = None
    for line in lines:
        if line.startswith("## "):
            heading = line[3:].strip()
            sections.setdefault(heading, [])
        elif heading is not None and line.strip():
            sections[heading].append(line.strip())
    kinds = {
        "当前收益": "immediate_value",
        "长期代价": "long_term_cost",
        "取舍条件": "decision_conditions",
    }
    return {
        "subject": title,
        "path": str(path.relative_to(repository)).replace("\\", "/"),
        "notes": [
            {"kind": kind, "text": " ".join(sections[heading])}
            for heading, kind in kinds.items()
            if sections.get(heading)
        ],
        "sources": sections.get("来源摘记", []),
    }


def build(repository: Path) -> dict[str, Any]:
    """Index subject pages and the remaining cleaned source blocks."""
    data = repository / "reference/strategy"
    subjects: dict[str, str] = {}
    passages: dict[str, dict[str, Any]] = {}
    by_subject: dict[str, list[str]] = {}
    for path in sorted((data / "corpus").glob("*.json")):
        document = _read(path)
        source = document["source"]
        for block in document["blocks"]:
            origin = block.get("origin", document["origin"])
            passage_id = f"{source['id']}:{block['key']}"
            if passage_id in passages:
                raise ValueError(f"duplicate block: {passage_id}")
            passages[passage_id] = {
                "id": passage_id,
                "source_id": source["id"],
                "source_title": source["title"],
                "author": block.get("author", source["author"]),
                "url": source["url"],
                "path": str(path.relative_to(repository)).replace("\\", "/"),
                "block_key": block["key"],
                "kind": block["kind"],
                "title": block["title"],
                "record_id": origin["record_id"],
                "locator": origin["locator"],
                "text": block["text"],
            }
            for subject in block["subjects"]:
                key = _key(subject)
                subjects.setdefault(key, subject)
                by_subject.setdefault(key, []).append(passage_id)
    pages = {}
    for path in sorted((data / "pages").glob("*.md")):
        page = _read_page(path, repository)
        key = _key(page["subject"])
        subjects[key] = page["subject"]
        pages[key] = page
    metadata: dict[str, dict[str, Any]] = {}
    aliases: dict[str, str] = {key: key for key in subjects}
    for item in _read(data / "subject_metadata.json"):
        canonical = _key(item["subject"])
        if canonical not in subjects:
            continue
        metadata[canonical] = item
        for alias in item["aliases"]:
            key = _key(alias)
            if key in aliases and aliases[key] != canonical:
                raise ValueError(f"ambiguous alias: {alias}")
            aliases[key] = canonical
    return {
        "subjects": subjects,
        "passages": passages,
        "by_subject": by_subject,
        "aliases": aliases,
        "metadata": metadata,
        "pages": pages,
    }


def lookup(repository: Path, term: str) -> dict[str, Any]:
    index = build(repository)
    canonical = index["aliases"].get(_key(term))
    if canonical is None:
        raise ValueError(f"unknown subject: {term}")
    extra = index["metadata"].get(canonical, {})
    if canonical in index["pages"]:
        page = index["pages"][canonical]
        return {
            "subject": {"name": extra.get("subject", page["subject"]), "mechanics": extra.get("mechanics")},
            "passages": [],
            "notes": page["notes"],
            "source_lines": page["sources"],
            "source_page": page["path"],
        }
    passages = [index["passages"][pid] for pid in sorted(index["by_subject"][canonical])]
    return {
        "subject": {"name": extra.get("subject", index["subjects"][canonical]), "mechanics": extra.get("mechanics")},
        "passages": passages,
        "notes": [],
    }


def list_subjects(repository: Path) -> list[dict[str, Any]]:
    index = build(repository)
    return sorted(
        (
            {
                "name": index["metadata"].get(key, {}).get("subject", name),
                "passage_count": (
                    len(index["pages"][key]["sources"])
                    if key in index["pages"]
                    else len(index["by_subject"][key])
                ),
            }
            for key, name in index["subjects"].items()
        ),
        key=lambda item: item["name"].casefold(),
    )
