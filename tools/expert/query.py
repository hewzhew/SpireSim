from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

from .store import ExpertReferenceStore, owned_path


EXPERT_LITERAL_SEARCH_SCHEMA = "sts-expert-literal-search-v1"
EXPERT_ENTITY_EVIDENCE_SCHEMA = "sts-expert-entity-evidence-v1"
EXPERT_KNOWLEDGE_COVERAGE_SCHEMA = "sts-expert-knowledge-coverage-v1"


def _one_line(text: object) -> str:
    return re.sub(r"\s+", " ", str(text or "")).strip()


class ExpertKnowledgeOwner:
    """In-process attributed expert lookup over local files."""

    def __init__(self, repository: Path) -> None:
        self._store = ExpertReferenceStore(repository)
        self.repository = self._store.repository
        self.sources = self._store.sources

    def _refresh(self) -> None:
        if self._store.has_changed():
            self._store = ExpertReferenceStore(self.repository)
            self.sources = self._store.sources

    def _source_contexts(self, source_ids: list[str]) -> list[dict[str, Any]]:
        contexts = []
        for source_id in dict.fromkeys(source_ids):
            catalog = self._store.catalog_by_id.get(source_id)
            if catalog is None:
                continue
            context = {
                "source_id": source_id,
                **{
                    field: catalog[field]
                    for field in (
                        "title",
                        "author",
                        "published_at",
                        "language",
                        "kind",
                        "status",
                        "scope_note",
                        "credibility",
                    )
                    if isinstance(catalog.get(field), str) and catalog[field]
                },
            }
            contexts.append(context)
        return contexts

    def _record_asset_refs(
        self, source_id: str, record: dict[str, Any] | None
    ) -> list[dict[str, str]]:
        if record is None:
            return []
        refs = record.get("asset_refs", [])
        if not refs:
            return []
        if not isinstance(refs, list):
            raise ValueError(f"expert record asset_refs are invalid: {source_id}")
        source = self._store.require_source(source_id)
        declared_assets = {
            asset.get("path"): asset
            for asset in source.manifest.get("assets", [])
            if isinstance(asset, dict) and isinstance(asset.get("path"), str)
        }
        resolved = []
        for ref in refs:
            if not isinstance(ref, dict):
                raise ValueError(f"expert record asset ref is invalid: {source_id}")
            relative = ref.get("path")
            declared = declared_assets.get(relative)
            if not isinstance(relative, str) or declared is None:
                continue
            path = owned_path(source.root, relative, must_exist=False)
            if not path.is_file():
                continue
            item = {
                field: ref[field]
                for field in ("kind", "alt", "relation")
                if isinstance(ref.get(field), str) and ref[field]
            }
            item["path"] = self._store.display_path(path)
            media_type = declared.get("media_type")
            if isinstance(media_type, str) and media_type:
                item["media_type"] = media_type
            resolved.append(item)
        return resolved

    def _with_record_assets(
        self,
        payload: dict[str, Any],
        source_id: str,
        record: dict[str, Any] | None,
    ) -> dict[str, Any]:
        asset_refs = self._record_asset_refs(source_id, record)
        if asset_refs:
            payload["asset_refs"] = asset_refs
        return payload

    def _media_annotation_evidence(
        self,
        row: dict[str, Any],
    ) -> dict[str, Any]:
        source = self._store.require_source(row["source_id"])
        origin_path, origin_line = self._store.media_annotation_origin(row["id"])
        asset = row["asset"]
        declared = next(
            item
            for item in source.manifest.get("assets", [])
            if isinstance(item, dict) and item.get("path") == asset["path"]
        )
        payload = {
            "evidence_kind": "source_media_transcription",
            "id": row["id"],
            "source_id": row["source_id"],
            "path": self._store.display_path(origin_path),
            "line_number": origin_line,
            "annotation_kind": row["annotation_kind"],
            "entity_ids": list(row["entity_ids"]),
            "observation": dict(row["observation"]),
            "selector": dict(row["selector"]),
            "asset": {
                "path": self._store.display_path(owned_path(source.root, asset["path"])),
                "sha256": asset["sha256"],
                **(
                    {"media_type": declared["media_type"]}
                    if isinstance(declared.get("media_type"), str)
                    and declared["media_type"]
                    else {}
                ),
            },
            "transcription": dict(row["transcription"]),
            "scope": dict(row["scope"]),
        }
        return payload

    def literal_search(
        self,
        query: str,
        *,
        source_id: str | None = None,
        max_results: int = 40,
        max_characters: int | None = None,
        max_line_characters: int = 1_200,
    ) -> dict[str, Any]:
        return self.literal_search_any(
            [query],
            source_id=source_id,
            max_results=max_results,
            max_characters=max_characters,
            max_line_characters=max_line_characters,
        )

    def literal_search_any(
        self,
        queries: list[str],
        *,
        source_id: str | None = None,
        max_results: int = 40,
        max_characters: int | None = None,
        max_line_characters: int = 1_200,
    ) -> dict[str, Any]:
        normalized_queries = []
        seen_queries = set()
        for query in queries:
            normalized = query.strip()
            folded = normalized.casefold()
            if normalized and folded not in seen_queries:
                normalized_queries.append(normalized)
                seen_queries.add(folded)
        if not normalized_queries:
            raise ValueError("expert queries must not be empty")
        if max_results < 1 or max_line_characters < 1:
            raise ValueError("expert search bounds must be positive")
        if max_characters is not None and max_characters < 1:
            raise ValueError("expert character budget must be positive")
        self._refresh()
        if source_id is not None:
            self._store.require_source(source_id)
        needles = [query.casefold() for query in normalized_queries]
        matches = []
        total_matches = 0
        characters = 0
        for document in self._store.documents:
            if source_id is not None:
                if document.evidence_kind == "tracked_entity_index":
                    continue
                if document.evidence_kind == "tracked_expert_note":
                    if document.source_id != source_id:
                        continue
                elif document.source_id not in {None, source_id}:
                    continue
            for line_number, line in enumerate(document.lines, start=1):
                folded_line = line.casefold()
                if not any(needle in folded_line for needle in needles):
                    continue
                if (
                    source_id is not None
                    and document.source_id is None
                    and source_id.casefold() not in line.casefold()
                ):
                    continue
                total_matches += 1
                excerpt = line
                line_truncated = len(excerpt) > max_line_characters
                if line_truncated:
                    excerpt = excerpt[: max_line_characters - 1].rstrip() + "…"
                result = {
                    "source_id": document.source_id,
                    "evidence_kind": document.evidence_kind,
                    "path": document.display_path,
                    "line_number": line_number,
                    "text": excerpt,
                    "line_truncated": line_truncated,
                }
                serialized_length = len(json.dumps(result, ensure_ascii=False))
                if len(matches) >= max_results:
                    continue
                if (
                    max_characters is not None
                    and characters + serialized_length > max_characters
                ):
                    continue
                matches.append(result)
                characters += serialized_length
        return {
            "schema": EXPERT_LITERAL_SEARCH_SCHEMA,
            "query": normalized_queries[0],
            "queries": normalized_queries,
            "source_id": source_id,
            "total_match_count": total_matches,
            "returned_match_count": len(matches),
            "truncated": len(matches) < total_matches,
            "matches": matches,
            "source_contexts": self._source_contexts(
                [
                    match["source_id"]
                    for match in matches
                    if isinstance(match.get("source_id"), str)
                ]
            ),
        }

    def _resolve_entity(self, name: str) -> dict[str, Any]:
        entity_id = self._store.entity_aliases.get(name.casefold())
        if entity_id is None:
            raise ValueError(
                f"unknown reference entity or alias {name!r}; inspect reference/entities.jsonl"
            )
        return self._store.entities_by_id[entity_id]

    def _adjacent_entity_ids(self, entity: dict[str, Any]) -> set[str]:
        adjacent = {
            edge["entity_id"]
            for edge in entity.get("related", [])
            if edge.get("entity_id") in self._store.entities_by_id
        }
        for candidate in self._store.entities:
            if any(
                edge.get("entity_id") == entity["id"]
                for edge in candidate.get("related", [])
            ):
                adjacent.add(candidate["id"])
        return adjacent

    @staticmethod
    def _excerpt(
        record: dict[str, Any], anchors: list[str], maximum_length: int = 420
    ) -> str:
        text = _one_line(record.get("text"))
        if not text:
            return "[record has no text]"
        folded = text.casefold()
        positions = [
            folded.find(anchor.casefold())
            for anchor in anchors
            if isinstance(anchor, str) and anchor and folded.find(anchor.casefold()) >= 0
        ]
        match_index = min(positions) if positions else 0
        start = max(0, match_index - 100)
        excerpt = text[start : start + maximum_length]
        if start > 0:
            excerpt = f"…{excerpt}"
        if start + maximum_length < len(text):
            excerpt = f"{excerpt}…"
        return excerpt

    def entity_evidence(
        self, name: str, *, source_id: str | None = None
    ) -> dict[str, Any]:
        self._refresh()
        entity = self._resolve_entity(name)
        adjacent = self._adjacent_entity_ids(entity)
        sources = (
            {source_id: self._store.require_source(source_id)}
            if source_id is not None
            else self.sources
        )
        passages = [
            passage
            for passage in self._store.passages
            if source_id is None or passage.get("source_id") == source_id
        ]
        claims = [
            claim
            for claim in self._store.claims
            if source_id is None or source_id in claim.get("source_ids", [])
        ]
        native_elements = [
            element
            for element in self._store.native_elements
            if source_id is None or element["source_id"] == source_id
        ]
        record_maps = {
            item_id: {str(record.get("id")): record for record in source.records}
            for item_id, source in sources.items()
        }

        def passage_evidence(passage: dict[str, Any]) -> dict[str, Any]:
            record_id = str(passage.get("record", {}).get("id"))
            record = record_maps.get(passage["source_id"], {}).get(record_id)
            origin_path, origin_line = self._store.passage_origin(passage["id"])
            return self._with_record_assets(
                {
                    "passage_id": passage["id"],
                    "source_id": passage["source_id"],
                    "evidence_kind": "reviewed_expert_passage",
                    "path": self._store.display_path(origin_path),
                    "line_number": origin_line,
                    "record": passage.get("record"),
                    "entity_ids": passage.get("entity_ids", []),
                    "anchors": passage.get("anchors", []),
                    "tags": passage.get("tags", []),
                    "summary": passage.get("summary", ""),
                    "excerpt": (
                        self._excerpt(record, passage.get("anchors", []))
                        if record is not None
                        else "[local canonical record unavailable]"
                    ),
                },
                passage["source_id"],
                record,
            )

        direct_passages = [
            passage_evidence(passage)
            for passage in passages
            if entity["id"] in passage.get("entity_ids", [])
        ]
        direct_native_elements = []
        for element in native_elements:
            if entity["id"] not in element.get("entity_ids", []):
                continue
            record = record_maps.get(element["source_id"], {}).get(
                str(element.get("record_id"))
            )
            direct_native_elements.append(
                self._with_record_assets(
                    {
                        "element_id": element["id"],
                        "source_id": element["source_id"],
                        "evidence_kind": "native_structured_source_element",
                        "path": self._store.display_path(
                            self.sources[element["source_id"]].text_path
                        ),
                        "line_number": element["source_span"]["start_line"],
                        "record": {
                            "id": element.get("record_id"),
                            "locator": record.get("locator") if record else None,
                        },
                        "entity_ids": element.get("entity_ids", []),
                        "heading_path": element.get("heading_path", []),
                        "authored_label": element.get("authored_label"),
                        "binding": (
                            "source-authored label deterministically resolved against "
                            "installed aliases and reviewed record context"
                        ),
                        "excerpt": _one_line(element.get("text")),
                    },
                    element["source_id"],
                    record,
                )
            )
        direct_media_transcriptions = [
            self._media_annotation_evidence(annotation)
            for annotation in self._store.media_annotation_rows
            if entity["id"] in annotation.get("entity_ids", [])
            and (source_id is None or annotation["source_id"] == source_id)
        ]
        related_passages = [
            passage_evidence(passage)
            for passage in passages
            if entity["id"] not in passage.get("entity_ids", [])
            and adjacent.intersection(passage.get("entity_ids", []))
        ]
        indexed_direct = {
            (passage["source_id"], str(passage.get("record", {}).get("id")))
            for passage in passages
            if entity["id"] in passage.get("entity_ids", [])
        }
        indexed_direct.update(
            (element["source_id"], str(element.get("record_id")))
            for element in native_elements
            if entity["id"] in element.get("entity_ids", [])
        )
        aliases = [
            value
            for value in [
                entity["id"],
                entity.get("canonical_name"),
                *entity.get("aliases", []),
            ]
            if isinstance(value, str) and value
        ]
        unreviewed = []
        for item_id, source in sources.items():
            for record in source.records:
                text = str(record.get("text", ""))
                folded = text.casefold()
                matched_aliases = [alias for alias in aliases if alias.casefold() in folded]
                record_id = str(record.get("id"))
                if matched_aliases and (item_id, record_id) not in indexed_direct:
                    unreviewed.append(
                        {
                            "source_id": item_id,
                            "record_id": record_id,
                            "locator": record.get("floor") or record.get("locator"),
                            "matched_aliases": matched_aliases,
                            "excerpt": self._excerpt(record, matched_aliases),
                        }
                    )
        direct_claims = [
            claim
            for claim in claims
            if entity["id"] in claim.get("entity_ids", [])
        ]
        related_claims = [
            claim
            for claim in claims
            if entity["id"] not in claim.get("entity_ids", [])
            and adjacent.intersection(claim.get("entity_ids", []))
        ]
        return {
            "schema": EXPERT_ENTITY_EVIDENCE_SCHEMA,
            "entity": entity,
            "adjacent_entity_ids": sorted(adjacent),
            "source_id": source_id,
            "direct_reviewed_passages": direct_passages,
            "direct_native_elements": direct_native_elements,
            "direct_source_media_transcriptions": direct_media_transcriptions,
            "source_contexts": self._source_contexts(
                [
                    *(passage["source_id"] for passage in direct_passages),
                    *(element["source_id"] for element in direct_native_elements),
                    *(
                        transcription["source_id"]
                        for transcription in direct_media_transcriptions
                    ),
                ]
            ),
            "direct_unreviewed_literal_matches": unreviewed,
            "related_reviewed_passages": related_passages,
            "direct_reviewed_claims": direct_claims,
            "related_reviewed_claims": related_claims,
            "boundary": (
                "related card/mechanic evidence is context, and a source media "
                "transcription is a scoped observation rather than authored prose "
                "or a reviewed strategic judgment"
            ),
        }

    def coverage(self) -> dict[str, Any]:
        self._refresh()
        reviewed_entity_ids = {
            entity_id
            for passage in self._store.passages
            for entity_id in passage.get("entity_ids", [])
        }
        reviewed_entity_ids.update(
            entity_id
            for claim in self._store.claims
            for entity_id in claim.get("entity_ids", [])
        )
        native_source_labeled_entity_ids = {
            entity_id
            for element in self._store.native_elements
            for entity_id in element.get("entity_ids", [])
        }
        source_media_transcription_entity_ids = {
            entity_id
            for annotation in self._store.media_annotation_rows
            for entity_id in annotation.get("entity_ids", [])
        }
        substantive_entity_ids = (
            reviewed_entity_ids
            | native_source_labeled_entity_ids
            | source_media_transcription_entity_ids
        )
        outline_entity_ids = {
            entity_id
            for outline in self._store.outline_rows
            for entity_id in outline.get("entity_ids", [])
        }
        direct_entity_ids = substantive_entity_ids | outline_entity_ids
        entity_evidence_coverage = []
        for kind in sorted(
            {
                str(entity.get("kind"))
                for entity in self._store.entities
                if entity.get("kind")
            }
        ):
            entity_ids = {
                entity["id"]
                for entity in self._store.entities
                if entity.get("kind") == kind
            }
            direct = entity_ids & direct_entity_ids
            substantive = entity_ids & substantive_entity_ids
            entity_evidence_coverage.append(
                {
                    "kind": kind,
                    "entity_count": len(entity_ids),
                    "direct_evidence_entity_count": len(direct),
                    "substantive_evidence_entity_count": len(substantive),
                    "reviewed_passage_or_claim_entity_count": len(
                        entity_ids & reviewed_entity_ids
                    ),
                    "native_source_labeled_entity_count": len(
                        entity_ids & native_source_labeled_entity_ids
                    ),
                    "source_media_transcription_entity_count": len(
                        entity_ids & source_media_transcription_entity_ids
                    ),
                    "outline_only_entity_count": len(direct - substantive),
                    "uncovered_entity_count": len(entity_ids - direct),
                }
            )
        sources = []
        for source_id, catalog in sorted(self._store.catalog_by_id.items()):
            source = self.sources.get(source_id)
            if source is None:
                sources.append(
                    {
                        "source_id": source_id,
                        "catalog_status": catalog.get("status"),
                        "coverage_status": "catalog_only",
                        "record_count": None,
                        "covered_record_count": 0,
                        "uncovered_record_count": None,
                    }
                )
                continue
            record_ids = {str(record.get("id")) for record in source.records}
            entity_record_ids = {
                str(row["record"]["id"])
                for index in self._store.reviewed_indices
                if index.source_id == source_id
                for row in index.rows
            }
            outline_record_ids = {
                str(row["record"]["id"])
                for outline in self._store.reviewed_outlines
                if outline.source_id == source_id
                for row in outline.rows
            }
            covered = entity_record_ids | outline_record_ids
            uncovered = sorted(record_ids - covered)
            unexpected = sorted(covered - record_ids)
            overlap = sorted(entity_record_ids & outline_record_ids)
            media_annotation_count = sum(
                1
                for annotation in self._store.media_annotation_rows
                if annotation["source_id"] == source_id
            )
            complete = not uncovered and not unexpected and not overlap
            sources.append(
                {
                    "source_id": source_id,
                    "catalog_status": catalog.get("status"),
                    "coverage_status": (
                        "complete_structured" if complete else "local_unstructured"
                    ),
                    "record_count": len(record_ids),
                    "entity_index_record_count": len(entity_record_ids),
                    "outline_record_count": len(outline_record_ids),
                    "media_annotation_count": media_annotation_count,
                    "covered_record_count": len(covered & record_ids),
                    "uncovered_record_count": len(uncovered),
                    "uncovered_record_ids": uncovered,
                    "unexpected_record_ids": unexpected,
                    "overlapping_record_ids": overlap,
                }
            )
        result = {
            "schema": EXPERT_KNOWLEDGE_COVERAGE_SCHEMA,
            "entity_catalog_status": self._store.game_entity_catalog_status,
            "simulator_entity_count": self._store.game_entity_count,
            "simulator_entity_alias_count": self._store.game_entity_alias_count,
            "ambiguous_entity_alias_count": len(
                self._store.ambiguous_entity_aliases
            ),
            "source_identifier_count": len(self._store.catalog_by_id),
            "entity_evidence_coverage": entity_evidence_coverage,
            "topic_count": len(self._store.topics),
            "entity_count": len(self._store.entities),
            "claim_count": len(self._store.claims),
            "passage_count": len(self._store.passages),
            "native_element_count": len(self._store.native_elements),
            "media_annotation_count": len(self._store.media_annotation_rows),
            "sources": sources,
        }
        return result

    def list_topics(self) -> list[dict[str, Any]]:
        self._refresh()
        return [dict(topic) for topic in self._store.topics]

    def list_entities(self, *, kind: str | None = None) -> list[dict[str, Any]]:
        self._refresh()
        return [
            dict(entity)
            for entity in self._store.entities
            if kind is None or entity.get("kind") == kind
        ]

    def source_outline(
        self, source_id: str, *, topic_id: str | None = None
    ) -> list[dict[str, Any]]:
        self._refresh()
        source = self._store.require_source(source_id)
        if topic_id is not None and topic_id not in self._store.topics_by_id:
            raise ValueError(f"unknown expert topic {topic_id!r}; list topics first")
        records = {str(record.get("id")): record for record in source.records}
        rows = []
        for outline in self._store.reviewed_outlines:
            if outline.source_id != source_id:
                continue
            for row in outline.rows:
                if topic_id is not None and topic_id not in row.get("topic_ids", []):
                    continue
                origin_path, origin_line = self._store.outline_origin(row["id"])
                rows.append(
                    self._with_record_assets(
                        {
                            **row,
                            "path": self._store.display_path(origin_path),
                            "line_number": origin_line,
                        },
                        source_id,
                        records.get(str(row["record"]["id"])),
                    )
                )
        return rows

    def list_sources(self) -> list[dict[str, Any]]:
        self._refresh()
        result = []
        for source_id, catalog in sorted(self._store.catalog_by_id.items()):
            source = self.sources.get(source_id)
            result.append(
                {
                    "id": source_id,
                    "provider": source.manifest.get("provider") if source else None,
                    "status": (
                        source.manifest.get("status") if source else catalog.get("status")
                    ),
                    "title": (
                        source.manifest.get("source", {}).get("title")
                        if source else catalog.get("title")
                    ),
                    "text_path": (
                        self._store.display_path(source.text_path) if source else None
                    ),
                    "url": catalog.get("url"),
                }
            )
        return result

    def show_source(self, source_id: str) -> dict[str, Any]:
        self._refresh()
        catalog = self._store.catalog_by_id.get(source_id)
        if catalog is None:
            raise ValueError(f"unknown expert source {source_id!r}")
        source = self.sources.get(source_id)
        if source is None:
            return {**catalog, "availability": "catalog_only"}
        return {
            **source.manifest,
            "resolved_source_root": str(source.root),
            "resolved_corpus_text": str(source.text_path),
            "resolved_corpus_records": str(source.records_path),
        }
