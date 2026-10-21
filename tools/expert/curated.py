"""Selective reading of curated strategy; source auditing is explicitly offline.

No game state, natural-language query parser, raw-source loader, or policy engine
is involved in ordinary reading. Applicability remains an analyst judgment.
"""

from __future__ import annotations

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re
from typing import Any, Mapping

# Scope vocabulary of the strategic TaskDecisionSiteV1 public contract. This
# describes where a package may be consulted, never entry applicability.
_STRATEGIC_SITES = frozenset(
    {
        "map",
        "card_reward",
        "neow",
        "event",
        "shop",
        "campfire",
        "boss_relic",
        "reward",
        "run_choice",
        "treasure",
    }
)


def _digest(value: Any) -> str:
    return hashlib.sha256(
        json.dumps(
            value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
        ).encode("utf-8")
    ).hexdigest()


def _unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate curated key: {key}")
        result[key] = value
    return result


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def _text(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def _texts(value: Any) -> bool:
    return isinstance(value, list) and bool(value) and all(_text(v) for v in value)


class CuratedStrategy:
    def __init__(self, path: Path):
        self.path = path.resolve(strict=True)
        self._bytes = self.path.read_bytes()
        self._data = json.loads(self._bytes, object_pairs_hook=_unique_object)
        try:
            self._validate()
        except (KeyError, TypeError, AttributeError) as error:
            raise ValueError("invalid curated strategy structure") from error
        self._entries = {entry["id"]: entry for entry in self._data["entries"]}

    def _validate(self) -> None:
        data = self._data
        _require(
            data["schema"] in {"sts-curated-strategy-v1", "sts-curated-strategy-v2"},
            "unsupported curated schema",
        )
        _require(_text(data["id"]) and _text(data["title"]), "missing curated identity")
        scope = data["scope"]
        _require(_text(scope["character"]), "missing curated character scope")
        if data["schema"] == "sts-curated-strategy-v1":
            _require(
                "decision_sites" not in scope and _text(scope["decision_site"]),
                "invalid single-site curated scope",
            )
            self._decision_sites = frozenset({scope["decision_site"]})
        else:
            sites = scope["decision_sites"]
            _require(
                "decision_site" not in scope
                and scope["character"] in {"Ironclad", "Silent", "Defect", "Watcher"}
                and _texts(sites)
                and len(set(sites)) == len(sites)
                and set(sites) <= _STRATEGIC_SITES,
                "invalid strategic curated scope",
            )
            self._decision_sites = frozenset(sites)
        _require(_texts(data["boundaries"]), "missing evidence boundaries")
        _require(
            _text(data["review"]["method"])
            and _text(data["review"]["reviewer"]),
            "missing review description",
        )
        for sid, source in data["sources"].items():
            _require(bool(re.fullmatch(r"[a-z0-9-]+", sid)), "invalid source ID")
            for key in (
                "title",
                "author",
                "published_at",
                "scope_note",
                "curator_scope_note",
            ):
                _require(_text(source[key]), f"missing source {key}: {sid}")
        for aid, assertion in data["assertions"].items():
            _require(
                assertion["source_id"] in data["sources"],
                f"unknown assertion source: {aid}",
            )
            _require(
                all(
                    _text(assertion[key]) for key in ("record_id", "locator", "summary")
                ),
                f"incomplete assertion: {aid}",
            )
        ids = set()
        _require(
            isinstance(data["entries"], list) and bool(data["entries"]),
            "missing curated entries",
        )
        for entry in data["entries"]:
            eid = entry["id"]
            _require(
                bool(re.fullmatch(r"[a-z0-9-]+", eid)) and eid not in ids,
                "invalid or duplicate entry ID",
            )
            ids.add(eid)
            _require(
                entry["kind"] in {"decision_method", "conditional_experience"},
                f"invalid strategy kind: {eid}",
            )
            for field in ("title", "principle", "check", "unless"):
                _require(
                    _text(entry[field]) and len(entry[field]) <= 180,
                    f"missing or unbounded {field}: {eid}",
                )
            for field in ("reasoning", "disagreements", "assertions"):
                _require(_texts(entry[field]), f"missing {field}: {eid}")
            _require(
                all(aid in data["assertions"] for aid in entry["assertions"]),
                f"unknown assertion: {eid}",
            )
            _require(bool(entry["inputs"]), f"missing inputs: {eid}")
            input_ids = set()
            for item in entry["inputs"]:
                _require(
                    item["kind"] in {"observable", "judgment"},
                    f"invalid input kind: {eid}",
                )
                _require(
                    _text(item["id"])
                    and item["id"] not in input_ids
                    and _text(item["description"]),
                    f"invalid input: {eid}",
                )
                input_ids.add(item["id"])

    def _refresh(self) -> None:
        if self.path.read_bytes() != self._bytes:
            self.__init__(self.path)

    def catalog(self) -> dict[str, Any]:
        self._refresh()
        return deepcopy(
            {
                "schema": "sts-curated-catalog-v1",
                "package": self._data["id"],
                "scope": self._data["scope"],
                "entries": [
                    {"id": entry["id"], "title": entry["title"]}
                    for entry in self._data["entries"]
                ],
            }
        )

    def supports_decision(self, character: str | None, site: str) -> bool:
        """Check declared package scope, without selecting or assessing entries."""
        self._refresh()
        return (
            character == self._data["scope"]["character"]
            and site in self._decision_sites
        )

    def read(
        self,
        entry_ids: list[str],
        *,
        detail: str = "brief",
        known: Mapping[str, str] | None = None,
    ) -> dict[str, Any]:
        """Read exact IDs in caller order; no ranking or automatic applicability.

        `known` is an explicit caller assertion that a previous reading is still
        in context. Receipts include detail and all cited curated dependencies.
        Omitting it always returns content, including after context compaction.
        """
        self._refresh()
        _require(
            detail in {"brief", "reasoning", "provenance"}, "unknown reading detail"
        )
        _require(
            bool(entry_ids) and len(set(entry_ids)) == len(entry_ids),
            "entry IDs must be nonempty and distinct",
        )
        _require(
            all(eid in self._entries for eid in entry_ids),
            "unknown curated entry ID; inspect the catalog",
        )
        readings = []
        for eid in entry_ids:
            entry = self._entries[eid]
            assertions = {
                aid: self._data["assertions"][aid] for aid in entry["assertions"]
            }
            sources = {
                a["source_id"]: self._data["sources"][a["source_id"]]
                for a in assertions.values()
            }
            revision = _digest(
                {
                    "schema": "sts-curated-reading-v1",
                    "package_schema": self._data["schema"],
                    "package": self._data["id"],
                    "entry": entry,
                    "assertions": assertions,
                    "sources": sources,
                    "detail": detail,
                    "scope": self._data["scope"],
                    "review": self._data["review"],
                    "boundaries": self._data["boundaries"],
                }
            )
            reading = {"id": eid, "revision": revision, "status": "unchanged"}
            if (known or {}).get(eid) != revision:
                reading["status"] = "content"
                reading["content"] = {
                    key: entry[key]
                    for key in (
                        "title",
                        "kind",
                        "principle",
                        "check",
                        "unless",
                        "disagreements",
                    )
                }
                if detail in {"reasoning", "provenance"}:
                    reading["content"].update(
                        inputs=entry["inputs"], reasoning=entry["reasoning"]
                    )
                if detail == "provenance":
                    reading["content"].update(
                        assertions=assertions,
                        sources=sources,
                        review=self._data["review"],
                    )
            readings.append(reading)
        return deepcopy(
            {
                "schema": "sts-curated-reading-v1",
                "package": self._data["id"],
                "scope": self._data["scope"],
                "detail": detail,
                "applicability": "unassessed",
                "evidence_class": "curated_expert_synthesis",
                "readings": readings,
            }
        )


def render_curated(document: dict[str, Any]) -> str:
    if document["schema"] == "sts-curated-catalog-v1":
        return "\n".join(f"{row['id']} · {row['title']}" for row in document["entries"])
    lines = []
    for reading in document["readings"]:
        if reading["status"] == "unchanged":
            lines.append(f"{reading['id']} · 已读内容无变化")
            continue
        row = reading["content"]
        lines.extend(
            [
                f"{reading['id']} · {row['title']}",
                row["principle"],
                f"检查：{row['check']}",
                f"改判：{row['unless']}",
            ]
        )
        if document["detail"] != "brief":
            lines.append("来源取舍：" + "；".join(row["disagreements"]))
        for item in row.get("inputs", []):
            label = "可观察" if item["kind"] == "observable" else "需判断"
            lines.append(f"  {label}：{item['description']}")
        lines.extend(f"  推导：{text}" for text in row.get("reasoning", []))
        for aid, assertion in row.get("assertions", {}).items():
            source = row["sources"][assertion["source_id"]]
            lines.append(
                f"  {aid} · {source['author']} · {assertion['locator']}：{assertion['summary']}"
            )
        for source in row.get("sources", {}).values():
            lines.append(
                f"  来源范围：{source['title']} ({source['published_at']})；{source['scope_note']}；{source['curator_scope_note']}"
            )
        lines.append("")
    return "\n".join(lines).rstrip()
