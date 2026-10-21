from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .game_entities import load_game_entity_catalog


EXPERT_SOURCE_SCHEMA = "external-research-source-v1"
EXPERT_SOURCE_ENTITY_RECORD_SCHEMA = "expert-source-entity-record-v1"
EXPERT_SOURCE_OUTLINE_RECORD_SCHEMA = "expert-source-outline-record-v1"
EXPERT_SOURCE_MEDIA_ANNOTATION_SCHEMA = "expert-source-media-annotation-v1"
EXPERT_TOPIC_SCHEMA = "expert-topic-v1"
EXPERT_DOCUMENT_ELEMENTS_SCHEMA = "expert-document-elements-v1"
REVIEWED_DECISION_ENTITY_KINDS = {
    "boss relic": "relic",
    "card reward": "card",
    "event": "event",
    "potion": "potion",
}


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows = []
    for line_number, line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        if not line.strip():
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f"invalid JSONL at {path}:{line_number}: {error}") from error
        if not isinstance(value, dict):
            raise ValueError(f"JSONL row is not an object at {path}:{line_number}")
        rows.append(value)
    return rows


def owned_path(root: Path, relative: str, *, must_exist: bool = True) -> Path:
    if not isinstance(relative, str) or not relative:
        raise ValueError("source artifact path must be non-empty")
    candidate = (root / relative).resolve(strict=must_exist)
    if not candidate.is_relative_to(root):
        raise ValueError(f"source path escapes its source directory: {relative}")
    return candidate


def file_identity(path: Path) -> tuple[int, int, int, int]:
    status = path.stat()
    return (status.st_dev, status.st_ino, status.st_size, status.st_mtime_ns)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


@dataclass(frozen=True)
class ExpertSource:
    source_id: str
    root: Path
    manifest_path: Path
    manifest: dict[str, Any]
    text_path: Path
    records_path: Path
    records: tuple[dict[str, Any], ...]
    elements_path: Path | None
    elements: tuple[dict[str, Any], ...]


@dataclass(frozen=True)
class SearchDocument:
    path: Path
    display_path: str
    evidence_kind: str
    source_id: str | None
    lines: tuple[str, ...]


@dataclass(frozen=True)
class ExpertReviewedIndex:
    source_id: str
    path: Path
    rows: tuple[dict[str, Any], ...]


@dataclass(frozen=True)
class ExpertReviewedOutline:
    source_id: str
    path: Path
    rows: tuple[dict[str, Any], ...]


@dataclass(frozen=True)
class ExpertMediaAnnotations:
    source_id: str
    path: Path
    rows: tuple[dict[str, Any], ...]


class ExpertReferenceStore:
    """One loaded view of the local expert evidence."""

    def __init__(
        self,
        repository: Path,
        *,
        rebuilding_derived_sources: frozenset[str] = frozenset(),
    ) -> None:
        self.repository = repository.resolve(strict=True)
        self._rebuilding_derived_sources = rebuilding_derived_sources
        self.expert_root = self.repository / "reference" / "expert"
        self.catalog_path = self.expert_root / "sources.jsonl"
        self.passages_path = self.expert_root / "passages.jsonl"
        self.claims_path = self.expert_root / "claims.jsonl"
        self.topics_path = self.expert_root / "topics.jsonl"
        self.entities_path = self.repository / "reference" / "entities.jsonl"
        self.notes_root = self.expert_root / "notes"

        self.catalog = read_jsonl(self.catalog_path)
        self.catalog_by_id = self._unique_by_id(self.catalog, "expert source catalog")
        self.sources = self._load_sources()
        game_catalog = load_game_entity_catalog(self.repository)
        self.game_entity_catalog_status = game_catalog.status
        self.game_entity_count = len(game_catalog.entities)
        self.game_entity_alias_count = game_catalog.alias_count
        self._game_entity_paths = game_catalog.tracked_paths
        self.topics = read_jsonl(self.topics_path)
        self.topics_by_id = self._unique_by_id(self.topics, "expert topics")
        self.topic_aliases = self._build_topic_aliases()
        self.reviewed_indices = self._load_reviewed_indices()
        self.reviewed_outlines = self._load_reviewed_outlines()
        self.outline_rows = [
            row for outline in self.reviewed_outlines for row in outline.rows
        ]
        self.outlines_by_id = self._unique_by_id(
            self.outline_rows, "expert reviewed outlines"
        )
        base_entities = read_jsonl(self.entities_path)
        indexed_entities = [
            row["entity"]
            for index in self.reviewed_indices
            for row in index.rows
        ]
        curated_entities = self._merge_entities(base_entities, indexed_entities)
        self.curated_entity_alias_owners = self._entity_alias_owners(curated_entities)
        self.entities = self._merge_entities(
            list(game_catalog.entities),
            curated_entities,
            prefer_later_canonical=True,
        )
        self.entities_by_id = self._unique_by_id(self.entities, "reference entities")
        self.native_elements = self._validate_native_elements()
        self.media_annotations = self._load_media_annotations()
        self.media_annotation_rows = [
            row
            for annotations in self.media_annotations
            for row in annotations.rows
        ]
        self.media_annotations_by_id = self._unique_by_id(
            self.media_annotation_rows, "expert source media annotations"
        )
        for row in self.outline_rows:
            unknown_entity_ids = sorted(
                entity_id
                for entity_id in row.get("entity_ids", [])
                if entity_id not in self.entities_by_id
            )
            if unknown_entity_ids:
                raise ValueError(
                    f"expert reviewed outline {row['id']} references unknown entities: "
                    + ", ".join(unknown_entity_ids)
                )
        manual_passages = read_jsonl(self.passages_path)
        indexed_passages = [
            self._indexed_passage(row)
            for index in self.reviewed_indices
            for row in index.rows
        ]
        self.passages = [*manual_passages, *indexed_passages]
        self.passages_by_id = self._unique_by_id(self.passages, "expert passages")
        self._passage_origins = {
            passage["id"]: (self.passages_path, line_number)
            for line_number, passage in enumerate(manual_passages, start=1)
        }
        for index in self.reviewed_indices:
            for line_number, row in enumerate(index.rows, start=1):
                self._passage_origins[row["id"]] = (index.path, line_number)
        self._outline_origins = {
            row["id"]: (outline.path, line_number)
            for outline in self.reviewed_outlines
            for line_number, row in enumerate(outline.rows, start=1)
        }
        self._media_annotation_origins = {
            row["id"]: (annotations.path, line_number)
            for annotations in self.media_annotations
            for line_number, row in enumerate(annotations.rows, start=1)
        }
        self.claims = read_jsonl(self.claims_path)
        self.entity_aliases = self._build_entity_aliases()
        self.documents = self._build_search_documents()
        self._tracked_paths = self._loaded_paths()
        self._identities = {
            path: file_identity(path) for path in self._tracked_paths
        }

    @staticmethod
    def _unique_by_id(
        rows: list[dict[str, Any]], label: str
    ) -> dict[str, dict[str, Any]]:
        result = {}
        for row in rows:
            row_id = row.get("id")
            if not isinstance(row_id, str) or not row_id:
                raise ValueError(f"{label} contains a missing ID")
            if row_id in result:
                raise ValueError(f"{label} contains duplicate ID {row_id}")
            result[row_id] = row
        return result

    def _load_sources(self) -> dict[str, ExpertSource]:
        sources = {}
        for source_id, catalog in sorted(self.catalog_by_id.items()):
            raw_cache = catalog.get("raw_cache")
            if not isinstance(raw_cache, str) or not raw_cache:
                continue
            root = (self.repository / raw_cache).resolve(strict=False)
            manifest_path = root / "source.json"
            if not manifest_path.is_file():
                continue
            manifest = read_json(manifest_path)
            if manifest.get("schema") != EXPERT_SOURCE_SCHEMA:
                raise ValueError(
                    f"cataloged expert source has an unexpected schema: {source_id}"
                )
            if manifest.get("id") != source_id or root.name != source_id:
                raise ValueError(f"expert source directory/ID mismatch: {source_id}")
            canonical = manifest.get("canonical_corpus")
            if not isinstance(canonical, dict):
                raise ValueError(f"expert source has no canonical corpus: {source_id}")
            text_record = canonical.get("text")
            records_record = canonical.get("records")
            if not isinstance(text_record, dict) or not isinstance(records_record, dict):
                raise ValueError(f"expert source corpus is incomplete: {source_id}")
            text_path = owned_path(root, text_record.get("path"))
            records_path = owned_path(root, records_record.get("path"))
            document = read_json(records_path)
            records = document.get("posts") if isinstance(document, dict) else document
            if not isinstance(records, list) or any(
                not isinstance(record, dict) for record in records
            ):
                raise ValueError(f"expert source records have an invalid shape: {source_id}")
            elements_path = None
            elements: list[dict[str, Any]] = []
            derived = manifest.get("derived_document")
            if derived is not None and source_id not in self._rebuilding_derived_sources:
                if not isinstance(derived, dict):
                    raise ValueError(
                        f"expert source derived document is invalid: {source_id}"
                    )
                candidate = owned_path(root, derived.get("path"), must_exist=False)
                elements_document = read_json(candidate) if candidate.is_file() else None
                candidate_elements = (
                    elements_document.get("elements", [])
                    if isinstance(elements_document, dict)
                    else []
                )
                if (
                    isinstance(elements_document, dict)
                    and elements_document.get("schema")
                    == EXPERT_DOCUMENT_ELEMENTS_SCHEMA
                    and elements_document.get("source_id") == source_id
                    and derived.get("schema") == EXPERT_DOCUMENT_ELEMENTS_SCHEMA
                    and derived.get("input_text_sha256")
                    == sha256(text_path)
                    and isinstance(elements_document.get("input"), dict)
                    and elements_document["input"].get("path")
                    == text_record.get("path")
                    and elements_document["input"].get("sha256")
                    == derived.get("input_text_sha256")
                    and isinstance(candidate_elements, list)
                    and all(isinstance(element, dict) for element in candidate_elements)
                ):
                    elements_path = candidate
                    elements = candidate_elements
            sources[source_id] = ExpertSource(
                source_id=source_id,
                root=root,
                manifest_path=manifest_path,
                manifest=manifest,
                text_path=text_path,
                records_path=records_path,
                records=tuple(records),
                elements_path=elements_path,
                elements=tuple(elements),
            )
        return sources

    def _validate_native_elements(self) -> tuple[dict[str, Any], ...]:
        rows = []
        seen_ids = set()
        for source_id, source in sorted(self.sources.items()):
            record_entity_kind_hints = self.reviewed_record_entity_kind_hints(
                source_id
            )
            record_ids = {str(record.get("id")) for record in source.records}
            element_ids = {
                str(element.get("id"))
                for element in source.elements
                if isinstance(element.get("id"), str) and element.get("id")
            }
            source_lines = source.text_path.read_text(encoding="utf-8").splitlines()
            for expected_ordinal, element in enumerate(source.elements):
                element_id = element.get("id")
                span = element.get("source_span")
                parent_id = element.get("parent_id")
                entity_ids = element.get("entity_ids", [])
                topic_ids = element.get("topic_ids", [])
                authored_label = element.get("authored_label")
                label_valid = authored_label is None and not entity_ids and not topic_ids
                if isinstance(authored_label, dict):
                    resolution = authored_label.get("resolution")
                    label_entity_ids = authored_label.get("entity_ids")
                    topic_resolution = authored_label.get("topic_resolution")
                    label_topic_ids = authored_label.get("topic_ids")
                    resolution_context = authored_label.get("resolution_context")
                    contextual_resolutions = {
                        "exact_ambiguous_alias_in_reviewed_record_context",
                        "exact_ambiguous_alias_after_authored_qualifier_in_reviewed_record_context",
                    }
                    context_valid = (
                        resolution not in contextual_resolutions
                        and resolution_context is None
                    )
                    if (
                        resolution in contextual_resolutions
                        and isinstance(resolution_context, dict)
                    ):
                        context_valid = (
                            resolution_context.get("basis")
                            == "reviewed_source_outline_decisions"
                            and resolution_context.get("record_id")
                            == element.get("record_id")
                            and isinstance(resolution_context.get("entity_kind"), str)
                            and bool(resolution_context["entity_kind"])
                            and record_entity_kind_hints.get(
                                resolution_context["record_id"]
                            )
                            == resolution_context["entity_kind"]
                            and all(
                                self.entities_by_id[entity_id].get("kind")
                                == resolution_context["entity_kind"]
                                for entity_id in entity_ids
                            )
                        )
                    label_valid = (
                        element.get("kind") == "paragraph"
                        and isinstance(authored_label.get("text"), str)
                        and bool(authored_label["text"].strip())
                        and resolution
                        in {
                            "exact_alias",
                            "exact_alias_after_authored_qualifier",
                            "split_exact_alias",
                            *contextual_resolutions,
                            "unresolved",
                        }
                        and isinstance(label_entity_ids, list)
                        and label_entity_ids == entity_ids
                        and (
                            (resolution == "unresolved" and not entity_ids)
                            or (resolution != "unresolved" and bool(entity_ids))
                        )
                        and topic_resolution
                        in {
                            "exact_alias",
                            "exact_alias_after_authored_qualifier",
                            "split_exact_alias",
                            "unresolved",
                        }
                        and isinstance(label_topic_ids, list)
                        and label_topic_ids == topic_ids
                        and (
                            (topic_resolution == "unresolved" and not topic_ids)
                            or (topic_resolution != "unresolved" and bool(topic_ids))
                        )
                        and context_valid
                    )
                valid = (
                    isinstance(element_id, str)
                    and bool(element_id)
                    and element_id not in seen_ids
                    and element.get("ordinal") == expected_ordinal
                    and element.get("kind")
                    in {"heading", "paragraph", "image", "table", "code"}
                    and str(element.get("record_id")) in record_ids
                    and (parent_id is None or parent_id in element_ids)
                    and isinstance(element.get("heading_path"), list)
                    and all(
                        isinstance(value, str) and value
                        for value in element.get("heading_path", [])
                    )
                    and isinstance(span, dict)
                    and isinstance(span.get("start_line"), int)
                    and isinstance(span.get("end_line"), int)
                    and 1 <= span.get("start_line") <= span.get("end_line")
                    and span.get("end_line") <= len(source_lines)
                    and isinstance(element.get("text"), str)
                    and isinstance(element.get("markdown"), str)
                    and isinstance(entity_ids, list)
                    and all(entity_id in self.entities_by_id for entity_id in entity_ids)
                    and isinstance(topic_ids, list)
                    and all(topic_id in self.topics_by_id for topic_id in topic_ids)
                    and label_valid
                )
                if valid:
                    original = "\n".join(
                        source_lines[
                            span["start_line"] - 1 : span["end_line"]
                        ]
                    )
                    valid = original == element["markdown"]
                if not valid:
                    continue
                seen_ids.add(element_id)
                rows.append({"source_id": source_id, **element})
        return tuple(rows)

    def reviewed_record_entity_kind_hints(self, source_id: str) -> dict[str, str]:
        hints = {}
        for outline in self.outline_rows:
            if outline.get("source_id") != source_id:
                continue
            decisions = outline.get("decisions", [])
            if not decisions or any(
                not isinstance(decision, str)
                or decision not in REVIEWED_DECISION_ENTITY_KINDS
                for decision in decisions
            ):
                continue
            kinds = {REVIEWED_DECISION_ENTITY_KINDS[decision] for decision in decisions}
            record = outline.get("record")
            if len(kinds) == 1 and isinstance(record, dict):
                record_id = record.get("id")
                if isinstance(record_id, str) and record_id:
                    hints[record_id] = next(iter(kinds))
        return hints

    def _load_reviewed_indices(self) -> tuple[ExpertReviewedIndex, ...]:
        indices = []
        for source_id, catalog in sorted(self.catalog_by_id.items()):
            specs = catalog.get("reviewed_indices", [])
            if not isinstance(specs, list):
                raise ValueError(f"expert source reviewed indices are invalid: {source_id}")
            for spec in specs:
                if not isinstance(spec, dict):
                    raise ValueError(
                        f"expert source reviewed index specification is invalid: {source_id}"
                    )
                relative = spec.get("path")
                path = owned_path(self.repository, relative)
                if not path.is_relative_to(self.expert_root):
                    raise ValueError(
                        f"expert reviewed index is outside reference/expert: {relative}"
                    )
                rows = read_jsonl(path)
                for line_number, row in enumerate(rows, start=1):
                    entity = row.get("entity")
                    record = row.get("record")
                    anchors = row.get("anchors")
                    valid = (
                        row.get("schema") == EXPERT_SOURCE_ENTITY_RECORD_SCHEMA
                        and row.get("source_id") == source_id
                        and isinstance(row.get("id"), str)
                        and bool(row.get("id"))
                        and isinstance(entity, dict)
                        and entity.get("schema") == "reference-entity-v1"
                        and isinstance(entity.get("id"), str)
                        and bool(entity.get("id"))
                        and isinstance(entity.get("kind"), str)
                        and bool(entity.get("kind"))
                        and isinstance(entity.get("canonical_name"), str)
                        and bool(entity.get("canonical_name"))
                        and isinstance(entity.get("aliases"), list)
                        and all(
                            isinstance(alias, str) and alias
                            for alias in entity.get("aliases", [])
                        )
                        and isinstance(entity.get("related"), list)
                        and all(
                            isinstance(edge, dict)
                            for edge in entity.get("related", [])
                        )
                        and isinstance(record, dict)
                        and isinstance(record.get("id"), str)
                        and bool(record.get("id"))
                        and isinstance(anchors, list)
                        and bool(anchors)
                        and all(isinstance(anchor, str) and anchor for anchor in anchors)
                    )
                    if not valid:
                        raise ValueError(
                            f"invalid expert reviewed index row at {path}:{line_number}"
                        )
                indices.append(
                    ExpertReviewedIndex(
                        source_id=source_id,
                        path=path,
                        rows=tuple(rows),
                    )
                )
        return tuple(indices)

    def _load_reviewed_outlines(self) -> tuple[ExpertReviewedOutline, ...]:
        outlines = []
        for source_id, catalog in sorted(self.catalog_by_id.items()):
            specs = catalog.get("reviewed_outlines", [])
            if not isinstance(specs, list):
                raise ValueError(f"expert source reviewed outlines are invalid: {source_id}")
            for spec in specs:
                if not isinstance(spec, dict):
                    raise ValueError(
                        f"expert source reviewed outline specification is invalid: {source_id}"
                    )
                relative = spec.get("path")
                path = owned_path(self.repository, relative)
                if not path.is_relative_to(self.expert_root):
                    raise ValueError(
                        f"expert reviewed outline is outside reference/expert: {relative}"
                    )
                rows = read_jsonl(path)
                seen_record_ids = set()
                for line_number, row in enumerate(rows, start=1):
                    record = row.get("record")
                    record_id = str(record.get("id")) if isinstance(record, dict) else ""
                    anchors = row.get("anchors")
                    valid = (
                        row.get("schema") == EXPERT_SOURCE_OUTLINE_RECORD_SCHEMA
                        and row.get("source_id") == source_id
                        and isinstance(row.get("id"), str)
                        and bool(row.get("id"))
                        and isinstance(record, dict)
                        and bool(record_id)
                        and record_id not in seen_record_ids
                        and isinstance(row.get("kind"), str)
                        and bool(row.get("kind"))
                        and isinstance(row.get("title"), str)
                        and bool(row.get("title"))
                        and isinstance(row.get("summary"), str)
                        and isinstance(anchors, list)
                        and all(isinstance(anchor, str) and anchor for anchor in anchors)
                        and isinstance(row.get("topic_ids", []), list)
                        and all(
                            topic_id in self.topics_by_id
                            for topic_id in row.get("topic_ids", [])
                        )
                        and isinstance(row.get("entity_ids", []), list)
                        and all(
                            isinstance(entity_id, str) and entity_id
                            for entity_id in row.get("entity_ids", [])
                        )
                        and isinstance(row.get("phases", []), list)
                        and all(
                            isinstance(phase, str) and phase
                            for phase in row.get("phases", [])
                        )
                        and isinstance(row.get("decisions", []), list)
                        and all(
                            isinstance(decision, str) and decision
                            for decision in row.get("decisions", [])
                        )
                    )
                    if not valid:
                        raise ValueError(
                            f"invalid expert reviewed outline row at {path}:{line_number}"
                        )
                    seen_record_ids.add(record_id)
                outlines.append(
                    ExpertReviewedOutline(
                        source_id=source_id,
                        path=path,
                        rows=tuple(rows),
                    )
                )
        return tuple(outlines)

    def _load_media_annotations(self) -> tuple[ExpertMediaAnnotations, ...]:
        annotation_files = []
        for source_id, catalog in sorted(self.catalog_by_id.items()):
            specs = catalog.get("media_annotations", [])
            if not isinstance(specs, list):
                raise ValueError(
                    f"expert source media annotations are invalid: {source_id}"
                )
            source = self.sources.get(source_id)
            if source is None:
                continue
            declared_assets = (
                {
                    asset.get("path"): asset
                    for asset in source.manifest.get("assets", [])
                    if isinstance(asset, dict)
                    and isinstance(asset.get("path"), str)
                }
                if source is not None
                else {}
            )
            for spec in specs:
                if not isinstance(spec, dict):
                    raise ValueError(
                        f"expert source media annotation specification is invalid: {source_id}"
                    )
                relative = spec.get("path")
                path = owned_path(self.repository, relative)
                if not path.is_relative_to(self.expert_root):
                    raise ValueError(
                        f"expert source media annotations are outside reference/expert: {relative}"
                    )
                rows = read_jsonl(path)
                accepted_rows = []
                for line_number, row in enumerate(rows, start=1):
                    asset = row.get("asset")
                    entity_ids = row.get("entity_ids")
                    observation = row.get("observation")
                    selector = row.get("selector")
                    transcription = row.get("transcription")
                    scope = row.get("scope")
                    asset_relative = asset.get("path") if isinstance(asset, dict) else None
                    declared = declared_assets.get(asset_relative)
                    if not isinstance(asset, dict):
                        raise ValueError(
                            f"invalid expert source media annotation at {path}:{line_number}"
                        )
                    if declared is None:
                        continue
                    asset_path = (
                        owned_path(source.root, asset_relative, must_exist=False)
                        if source is not None
                        and isinstance(asset_relative, str)
                        and declared is not None
                        else None
                    )
                    asset_hash = sha256(asset_path) if asset_path and asset_path.is_file() else None
                    if (
                        asset_hash is None
                        or asset_hash != str(declared.get("sha256", "")).casefold()
                        or asset_hash != str(asset.get("sha256", "")).casefold()
                    ):
                        continue
                    valid = (
                        row.get("schema") == EXPERT_SOURCE_MEDIA_ANNOTATION_SCHEMA
                        and row.get("source_id") == source_id
                        and isinstance(row.get("id"), str)
                        and bool(row.get("id"))
                        and row.get("annotation_kind") == "tier_membership"
                        and isinstance(asset, dict)
                        and isinstance(asset_relative, str)
                        and bool(asset_relative)
                        and isinstance(asset.get("sha256"), str)
                        and len(asset["sha256"]) == 64
                        and declared is not None
                        and isinstance(entity_ids, list)
                        and bool(entity_ids)
                        and len(entity_ids) == 1
                        and all(
                            isinstance(entity_id, str) and bool(entity_id)
                            for entity_id in entity_ids
                        )
                        and len(entity_ids) == len(set(entity_ids))
                        and all(entity_id in self.entities_by_id for entity_id in entity_ids)
                        and isinstance(observation, dict)
                        and isinstance(observation.get("source_label"), str)
                        and bool(observation["source_label"].strip())
                        and isinstance(observation.get("tier_label"), str)
                        and bool(observation["tier_label"].strip())
                        and isinstance(selector, dict)
                        and selector.get("type") == "tier_row_label"
                        and selector.get("value") == observation.get("tier_label")
                        and isinstance(transcription, dict)
                        and transcription.get("method") == "model_visual_inspection"
                        and isinstance(transcription.get("created_by"), str)
                        and bool(transcription["created_by"].strip())
                        and isinstance(transcription.get("created_at"), str)
                        and bool(transcription["created_at"].strip())
                        and transcription.get("review_state")
                        == "model_transcribed_unverified"
                        and isinstance(scope, dict)
                        and scope.get("coverage") == "partial_targeted"
                        and scope.get("image_complete") is False
                    )
                    if not valid:
                        raise ValueError(
                            f"invalid expert source media annotation at {path}:{line_number}"
                        )
                    accepted_rows.append(row)
                annotation_files.append(
                    ExpertMediaAnnotations(
                        source_id=source_id,
                        path=path,
                        rows=tuple(accepted_rows),
                    )
                )
        return tuple(annotation_files)

    def _build_topic_aliases(self) -> dict[str, str]:
        aliases = {}
        for topic_id, topic in self.topics_by_id.items():
            valid = (
                topic.get("schema") == EXPERT_TOPIC_SCHEMA
                and isinstance(topic.get("canonical_name"), str)
                and bool(topic.get("canonical_name"))
                and isinstance(topic.get("aliases", []), list)
                and all(
                    isinstance(alias, str) and alias
                    for alias in topic.get("aliases", [])
                )
                and isinstance(topic.get("tags", []), list)
                and all(
                    isinstance(tag, str) and tag for tag in topic.get("tags", [])
                )
                and isinstance(topic.get("description", ""), str)
            )
            if not valid:
                raise ValueError(f"expert topic is invalid: {topic_id}")
            values = [topic_id, topic.get("canonical_name"), *topic.get("aliases", [])]
            for value in values:
                if not isinstance(value, str) or not value:
                    continue
                normalized = value.casefold()
                owner = aliases.get(normalized)
                if owner is not None and owner != topic_id:
                    raise ValueError(
                        f"ambiguous expert topic alias {value!r}: {owner}, {topic_id}"
                    )
                aliases[normalized] = topic_id
        return aliases

    @staticmethod
    def _merge_entities(
        base_entities: list[dict[str, Any]],
        indexed_entities: list[dict[str, Any]],
        *,
        prefer_later_canonical: bool = False,
    ) -> list[dict[str, Any]]:
        merged: dict[str, dict[str, Any]] = {}
        order = []
        for entity in [*base_entities, *indexed_entities]:
            entity_id = entity.get("id")
            if not isinstance(entity_id, str) or not entity_id:
                raise ValueError("reference entity contains a missing ID")
            current = merged.get(entity_id)
            if current is None:
                merged[entity_id] = {
                    **entity,
                    "aliases": list(dict.fromkeys(entity.get("aliases", []))),
                    "related": [dict(edge) for edge in entity.get("related", [])],
                }
                order.append(entity_id)
                continue
            for field in ("schema", "kind"):
                if current.get(field) != entity.get(field):
                    raise ValueError(
                        f"conflicting reference entity {entity_id} field {field}"
                    )
            if current.get("canonical_name") != entity.get("canonical_name"):
                if not prefer_later_canonical:
                    raise ValueError(
                        f"conflicting reference entity {entity_id} field canonical_name"
                    )
                current["canonical_name"] = entity["canonical_name"]
            current["aliases"] = list(
                dict.fromkeys([*current.get("aliases", []), *entity.get("aliases", [])])
            )
            related = [*current.get("related", []), *entity.get("related", [])]
            current["related"] = list(
                {
                    (edge.get("entity_id"), edge.get("relation")): dict(edge)
                    for edge in related
                }.values()
            )
        return [merged[entity_id] for entity_id in order]

    @staticmethod
    def _preferred_encounter_alias_owner(
        owners: set[str], entities_by_id: dict[str, dict[str, Any]]
    ) -> str | None:
        encounters = [
            entity_id
            for entity_id in owners
            if entities_by_id[entity_id].get("kind") == "encounter"
        ]
        if len(encounters) != 1:
            return None
        if any(
            entities_by_id[entity_id].get("kind") not in {"encounter", "enemy"}
            for entity_id in owners
        ):
            return None
        return encounters[0]

    @classmethod
    def _entity_alias_owners(cls, entities: list[dict[str, Any]]) -> dict[str, str]:
        candidates: dict[str, set[str]] = {}
        for entity in entities:
            entity_id = entity["id"]
            for value in (
                entity_id,
                entity.get("canonical_name"),
                *entity.get("aliases", []),
            ):
                if not isinstance(value, str) or not value:
                    continue
                normalized = value.casefold()
                candidates.setdefault(normalized, set()).add(entity_id)
        entities_by_id = {entity["id"]: entity for entity in entities}
        owners = {}
        for normalized, candidates_for_alias in candidates.items():
            if len(candidates_for_alias) == 1:
                owners[normalized] = next(iter(candidates_for_alias))
                continue
            preferred = cls._preferred_encounter_alias_owner(
                candidates_for_alias, entities_by_id
            )
            if preferred is None:
                raise ValueError(
                    f"ambiguous curated reference entity alias {normalized!r}: "
                    + ", ".join(sorted(candidates_for_alias))
                )
            owners[normalized] = preferred
        return owners

    def _compatible_entity_alias_owner(self, owners: set[str]) -> str | None:
        return self._preferred_encounter_alias_owner(owners, self.entities_by_id)

    @staticmethod
    def _indexed_passage(row: dict[str, Any]) -> dict[str, Any]:
        return {
            "schema": "expert-passage-v1",
            "id": row["id"],
            "source_id": row["source_id"],
            "record": row["record"],
            "entity_ids": [row["entity"]["id"]],
            "anchors": row["anchors"],
            "tags": row.get("tags", []),
            "summary": row.get("derived_role_summary", ""),
            "review_state": row.get("review_state", "located"),
        }

    def _build_entity_aliases(self) -> dict[str, str]:
        candidates: dict[str, set[str]] = {}
        for entity_id, entity in self.entities_by_id.items():
            values = [entity_id, entity.get("canonical_name"), *entity.get("aliases", [])]
            for value in values:
                if not isinstance(value, str) or not value:
                    continue
                normalized = value.casefold()
                candidates.setdefault(normalized, set()).add(entity_id)
        aliases = {}
        self.ambiguous_entity_aliases = {}
        for normalized, owners in candidates.items():
            if len(owners) == 1:
                aliases[normalized] = next(iter(owners))
                continue
            preferred = self.curated_entity_alias_owners.get(normalized)
            if preferred is None:
                preferred = self._compatible_entity_alias_owner(owners)
            if preferred in owners:
                aliases[normalized] = preferred
            else:
                self.ambiguous_entity_aliases[normalized] = sorted(owners)
        return aliases

    def display_path(self, path: Path) -> str:
        try:
            return path.relative_to(self.repository).as_posix()
        except ValueError:
            return str(path)

    def _source_for_note(self, path: Path) -> str | None:
        matching = [
            source_id
            for source_id in self.sources
            if path.stem == source_id or path.stem.startswith(f"{source_id}-")
        ]
        return max(matching, key=len) if matching else None

    def _document(
        self, path: Path, evidence_kind: str, source_id: str | None = None
    ) -> SearchDocument:
        return SearchDocument(
            path=path,
            display_path=self.display_path(path),
            evidence_kind=evidence_kind,
            source_id=source_id,
            lines=tuple(path.read_text(encoding="utf-8").splitlines()),
        )

    def _build_search_documents(self) -> tuple[SearchDocument, ...]:
        documents = []
        if self.notes_root.is_dir():
            for path in sorted(self.notes_root.glob("*.md")):
                documents.append(
                    self._document(path, "tracked_expert_note", self._source_for_note(path))
                )
        for path, kind in (
            (self.catalog_path, "tracked_source_catalog"),
            (self.claims_path, "tracked_expert_claim"),
            (self.passages_path, "tracked_expert_passage"),
            (self.entities_path, "tracked_entity_index"),
        ):
            documents.append(self._document(path, kind))
        for index in self.reviewed_indices:
            documents.append(
                self._document(index.path, "tracked_expert_index", index.source_id)
            )
        for outline in self.reviewed_outlines:
            documents.append(
                self._document(
                    outline.path, "tracked_expert_outline", outline.source_id
                )
            )
        for source in self.sources.values():
            documents.append(
                self._document(
                    source.text_path, "canonical_expert_corpus", source.source_id
                )
            )
        return tuple(documents)

    def _loaded_paths(self) -> tuple[Path, ...]:
        paths = {document.path for document in self.documents}
        if self.notes_root.is_dir():
            paths.add(self.notes_root)
        for source in self.sources.values():
            paths.update(
                (source.manifest_path, source.text_path, source.records_path)
            )
            paths.add(source.root)
            paths.update(
                directory
                for directory in (source.root / "corpus", source.root / "assets")
                if directory.is_dir()
            )
            if source.elements_path is not None:
                paths.add(source.elements_path)
            for asset in source.manifest.get("assets", []):
                if isinstance(asset, dict) and isinstance(asset.get("path"), str):
                    path = owned_path(source.root, asset["path"], must_exist=False)
                    if path.is_file():
                        paths.add(path)
        paths.update(index.path for index in self.reviewed_indices)
        paths.update(outline.path for outline in self.reviewed_outlines)
        paths.update(annotations.path for annotations in self.media_annotations)
        paths.update(self._game_entity_paths)
        paths.update(
            (
                self.catalog_path,
                self.passages_path,
                self.claims_path,
                self.topics_path,
                self.entities_path,
            )
        )
        return tuple(sorted(paths))

    def passage_origin(self, passage_id: str) -> tuple[Path, int]:
        return self._passage_origins[passage_id]

    def outline_origin(self, outline_id: str) -> tuple[Path, int]:
        return self._outline_origins[outline_id]

    def media_annotation_origin(self, annotation_id: str) -> tuple[Path, int]:
        return self._media_annotation_origins[annotation_id]

    def has_changed(self) -> bool:
        return any(
            not path.exists() or file_identity(path) != identity
            for path, identity in self._identities.items()
        )

    def require_source(self, source_id: str) -> ExpertSource:
        source = self.sources.get(source_id)
        if source is None:
            raise ValueError(
                f"local expert source {source_id!r} is not installed; list sources first"
            )
        return source
