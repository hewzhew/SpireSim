from __future__ import annotations

import importlib
import json
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Callable, Literal, Protocol

RESEARCH_CHANNELS_V2 = {"expert", "wiki_reference"}
REFERENCE_ENTITY_KINDS_V2 = {
    "card",
    "potion",
    "relic",
    "enemy",
    "encounter",
    "event",
}


@dataclass(frozen=True)
class ResearchQueryV2:
    channel: Literal["expert", "wiki_reference"]
    query: str
    entity_kind: str | None = None
    stable_id: str | None = None
    include_related: bool = False

    def __post_init__(self) -> None:
        if self.channel not in RESEARCH_CHANNELS_V2:
            raise ValueError(
                "research query channel must be expert or wiki_reference"
            )
        if not isinstance(self.query, str) or not self.query.strip():
            raise ValueError("research query text must be non-empty")
        query = self.query.strip()
        if len(query) > 256 or "\n" in query or "\r" in query:
            raise ValueError(
                "research query text must be one line and at most 256 characters"
            )
        if not isinstance(self.include_related, bool):
            raise ValueError("research include_related must be boolean")
        stable_id = self.stable_id
        if self.channel == "expert":
            if (
                self.entity_kind is not None
                or stable_id is not None
                or self.include_related
            ):
                raise ValueError("expert research accepts only channel and query")
        else:
            if self.entity_kind not in REFERENCE_ENTITY_KINDS_V2:
                raise ValueError(
                    "Wiki reference research requires an exact supported entity_kind"
                )
            if not isinstance(stable_id, str) or not stable_id.strip():
                raise ValueError("Wiki reference research requires an exact stable_id")
            stable_id = stable_id.strip()
            if len(stable_id) > 128 or "\n" in stable_id or "\r" in stable_id:
                raise ValueError(
                    "Wiki reference stable_id must be one line and at most 128 characters"
                )
            if self.include_related and self.entity_kind != "encounter":
                raise ValueError(
                    "include_related is valid only for encounter Wiki references"
                )
        object.__setattr__(self, "query", query)
        object.__setattr__(self, "stable_id", stable_id)


def _natural_identity_key(value: str) -> str:
    return "".join(character for character in value.casefold() if character.isalnum())


def _natural_external_aliases(entity: dict[str, Any]) -> list[str]:
    """Keep protocol aliases only when they are formatting variants of a public name."""
    public_names = [entity.get("stable_id"), entity.get("display_name")]
    public_keys = {
        _natural_identity_key(name)
        for name in public_names
        if isinstance(name, str) and name.strip()
    }
    external_names = [
        entity.get("external_id"),
        *entity.get("external_aliases", []),
    ]
    return [
        alias
        for alias in external_names
        if isinstance(alias, str)
        and alias.strip()
        and _natural_identity_key(alias) in public_keys
    ]


@dataclass(frozen=True)
class KnowledgeSearchResultV2:
    request: ResearchQueryV2
    status: Literal["matched", "no_match", "unknown", "error"]
    evidence: dict[str, object]
    truncated: bool

    def to_json(self) -> dict[str, object]:
        return asdict(self)


class MechanicsOwnerV1(Protocol):
    def search(
        self,
        *,
        kind: str,
        stable_id: str,
        query: str,
        include_related: bool = False,
        raw_query: bool = False,
        limit: int = 20,
    ) -> dict[str, Any]: ...


class ExpertOwnerV1(Protocol):
    def literal_search(
        self,
        query: str,
        *,
        source_id: str | None = None,
        max_results: int = 40,
        max_characters: int | None = None,
        max_line_characters: int = 1_200,
    ) -> dict[str, Any]: ...

    def literal_search_any(
        self,
        queries: list[str],
        *,
        source_id: str | None = None,
        max_results: int = 40,
        max_characters: int | None = None,
        max_line_characters: int = 1_200,
    ) -> dict[str, Any]: ...

    def entity_evidence(
        self,
        name: str,
        *,
        source_id: str | None = None,
    ) -> dict[str, Any]: ...


def _scope_provenance(scope: dict[str, Any]) -> dict[str, Any]:
    fields = (
        "source_id",
        "page_id",
        "page_title",
        "url",
        "revision_id",
        "revision_timestamp",
        "revision_sha1",
        "scope_type",
        "section_document_order",
        "section_title",
        "section_path",
        "scope_bindings",
    )
    return {field: scope[field] for field in fields if field in scope}


def _excluded_provenance(page: dict[str, Any]) -> dict[str, Any]:
    fields = (
        "source_id",
        "page_id",
        "page_title",
        "url",
        "revision_id",
        "revision_timestamp",
        "revision_sha1",
        "scope_binding",
        "binding_status",
        "reason",
    )
    return {field: page[field] for field in fields if field in page}


def _match_evidence(match: dict[str, Any], max_snippet_characters: int) -> dict[str, Any]:
    result = _scope_provenance(match)
    snippet = str(match.get("snippet", ""))
    if len(snippet) > max_snippet_characters:
        snippet = snippet[: max_snippet_characters - 1].rstrip() + "…"
    result["snippet"] = snippet
    return result


class LocalKnowledgeRetrieverV2:
    """Bounded two-channel lookup over local expert and Wiki owners."""

    def __init__(
        self,
        repository: Path,
        *,
        expert_max_results: int = 8,
        expert_max_characters: int = 8_000,
        mechanics_max_results: int = 6,
        mechanics_max_scope_records: int = 12,
        mechanics_max_characters: int = 12_000,
        mechanics_max_snippet_characters: int = 600,
        mechanics_owner: MechanicsOwnerV1 | None = None,
        expert_owner: ExpertOwnerV1 | None = None,
        expert_alias_resolver: Callable[[str], list[str]] | None = None,
    ) -> None:
        bounds = (
            expert_max_results,
            expert_max_characters,
            mechanics_max_results,
            mechanics_max_scope_records,
            mechanics_max_characters,
            mechanics_max_snippet_characters,
        )
        if any(bound < 1 for bound in bounds):
            raise ValueError("knowledge retrieval bounds must be positive")
        self._repository = repository.resolve(strict=True)
        self._expert_max_results = expert_max_results
        self._expert_max_characters = expert_max_characters
        self._mechanics_max_results = mechanics_max_results
        self._mechanics_max_scope_records = mechanics_max_scope_records
        self._mechanics_max_characters = mechanics_max_characters
        self._mechanics_max_snippet_characters = mechanics_max_snippet_characters
        self._entity_database = (
            self._repository / ".oracle-lab" / "external-research" / "entities.sqlite3"
        )
        self._wiki_database = (
            self._repository / ".oracle-lab" / "external-research" / "wiki.sqlite3"
        )
        repository_text = str(self._repository)
        if repository_text not in sys.path:
            sys.path.insert(0, repository_text)
        if mechanics_owner is None:
            module = importlib.import_module("tools.wiki.entity_query")
            mechanics_owner = module.AttributedMechanicsOwner(
                entity_database=self._entity_database,
                wiki_database=self._wiki_database,
            )
        self._mechanics_owner = mechanics_owner
        if expert_owner is None:
            module = importlib.import_module("tools.expert.query")
            expert_owner = module.ExpertKnowledgeOwner(self._repository)
        self._expert_owner = expert_owner
        self._expert_alias_resolver = (
            expert_alias_resolver or self._resolve_exact_wiki_entity_aliases
        )
        self._cache: dict[ResearchQueryV2, KnowledgeSearchResultV2] = {}

    def _resolve_exact_wiki_entity_aliases(self, query: str) -> list[str]:
        if not self._entity_database.is_file():
            return []
        module = importlib.import_module("tools.wiki.entity_query")
        entities = module.query_entities(
            database=self._entity_database,
            kind=None,
            stable_id=None,
            query=query,
        )
        if len(entities) != 1:
            return []
        entity = entities[0]
        aliases = [
            entity.get("stable_id"),
            entity.get("display_name"),
            *_natural_external_aliases(entity),
            *[
                edge.get("title")
                for edge in entity.get("edges", [])
                if edge.get("status") == "accepted"
            ],
        ]
        unique = []
        seen = set()
        for alias in aliases:
            if not isinstance(alias, str) or not alias.strip():
                continue
            normalized = alias.strip()
            folded = normalized.casefold()
            if folded not in seen:
                unique.append(normalized)
                seen.add(folded)
        return unique

    def search(self, request: ResearchQueryV2) -> KnowledgeSearchResultV2:
        if request in self._cache:
            return self._cache[request]
        if request.channel == "expert":
            result = self._search_expert(request)
        else:
            result = self._search_wiki_reference(request)
        self._cache[request] = result
        return result

    def _search_expert(self, request: ResearchQueryV2) -> KnowledgeSearchResultV2:
        try:
            aliases = self._expert_alias_resolver(request.query)
            queries = list(dict.fromkeys([request.query, *aliases]))
            entity_evidence = getattr(self._expert_owner, "entity_evidence", None)
            entity_raw = None
            if callable(entity_evidence):
                entity_name = aliases[0] if aliases else request.query
                try:
                    entity_raw = entity_evidence(entity_name)
                except ValueError as error:
                    if "unknown reference entity or alias" not in str(error):
                        raise
            if entity_raw is not None:
                direct = entity_raw.get("direct_reviewed_passages", [])
                if not isinstance(direct, list):
                    raise ValueError("expert entity evidence direct passages are invalid")
                matches = []
                characters = 0
                for passage in direct:
                    if not isinstance(passage, dict):
                        raise ValueError("expert entity evidence passage is invalid")
                    match = {
                        "source_id": passage.get("source_id"),
                        "evidence_kind": passage.get("evidence_kind"),
                        "path": passage.get("path"),
                        "line_number": passage.get("line_number"),
                        "text": passage.get("excerpt"),
                        "line_truncated": False,
                        "record": passage.get("record"),
                    }
                    serialized_length = len(json.dumps(match, ensure_ascii=False))
                    if len(matches) >= self._expert_max_results:
                        continue
                    if characters + serialized_length > self._expert_max_characters:
                        continue
                    matches.append(match)
                    characters += serialized_length
                evidence = {
                    "evidence_class": "expert_strategy",
                    "source_schema": entity_raw.get("schema"),
                    "retrieval_mode": "reviewed_entity",
                    "identity_expanded_queries": queries,
                    "entity": entity_raw.get("entity"),
                    "boundary": entity_raw.get("boundary"),
                    "source_contexts": entity_raw.get("source_contexts", []),
                    "total_match_count": len(direct),
                    "returned_match_count": len(matches),
                    "unreviewed_literal_match_count": len(
                        entity_raw.get("direct_unreviewed_literal_matches", [])
                    ),
                    "related_reviewed_passage_count": len(
                        entity_raw.get("related_reviewed_passages", [])
                    ),
                    "matches": matches,
                }
                return KnowledgeSearchResultV2(
                    request=request,
                    status="matched" if direct else "unknown",
                    evidence=evidence,
                    truncated=len(matches) < len(direct),
                )
            literal_search_any = getattr(
                self._expert_owner, "literal_search_any", None
            )
            if callable(literal_search_any) and len(queries) > 1:
                raw = literal_search_any(
                    queries,
                    max_results=self._expert_max_results,
                    max_characters=self._expert_max_characters,
                )
            else:
                raw = self._expert_owner.literal_search(
                    request.query,
                    max_results=self._expert_max_results,
                    max_characters=self._expert_max_characters,
                )
            raw["identity_expanded_queries"] = queries
        except (OSError, ValueError) as error:
            return KnowledgeSearchResultV2(
                request=request,
                status="error",
                evidence={
                    "evidence_class": "expert_strategy",
                    "message": str(error),
                },
                truncated=False,
            )
        return KnowledgeSearchResultV2(
            request=request,
            status="matched" if raw["total_match_count"] else "no_match",
            evidence={"evidence_class": "expert_strategy", **raw},
            truncated=raw["truncated"],
        )

    def _search_wiki_reference(
        self, request: ResearchQueryV2
    ) -> KnowledgeSearchResultV2:
        assert request.entity_kind is not None
        assert request.stable_id is not None
        try:
            raw = self._mechanics_owner.search(
                kind=request.entity_kind,
                stable_id=request.stable_id,
                query=request.query,
                include_related=request.include_related,
                limit=self._mechanics_max_results,
            )
            requested_match_count = raw["match_count"]
            retrieval_mode = "requested_literal"
            effective_query = request.query
            if (
                raw["eligible_scope_count"] > 0
                and raw["match_count"] == 0
                and request.query.casefold() != request.stable_id.casefold()
            ):
                raw = self._mechanics_owner.search(
                    kind=request.entity_kind,
                    stable_id=request.stable_id,
                    query=request.stable_id,
                    include_related=request.include_related,
                    limit=self._mechanics_max_results,
                )
                retrieval_mode = "exact_identity_fallback"
                effective_query = request.stable_id
            if raw["eligible_scope_count"] > 0 and raw["match_count"] == 0:
                localized_title = next(
                    (
                        scope["page_title"]
                        for scope in raw["eligible_scopes"]
                        if isinstance(scope, dict)
                        and isinstance(scope.get("page_title"), str)
                        and scope["page_title"]
                        and scope["page_title"].casefold()
                        != effective_query.casefold()
                    ),
                    None,
                )
                if localized_title is not None:
                    raw = self._mechanics_owner.search(
                        kind=request.entity_kind,
                        stable_id=request.stable_id,
                        query=localized_title,
                        include_related=request.include_related,
                        limit=self._mechanics_max_results,
                    )
                    retrieval_mode = "localized_title_fallback"
                    effective_query = localized_title
        except (OSError, ValueError) as error:
            return KnowledgeSearchResultV2(
                request=request,
                status="error",
                evidence={
                    "evidence_class": "wiki_reference",
                    "message": str(error),
                },
                truncated=False,
            )

        scopes = [
            _scope_provenance(scope)
            for scope in raw["eligible_scopes"][: self._mechanics_max_scope_records]
        ]
        excluded = [
            _excluded_provenance(page)
            for page in raw["excluded_pages"][: self._mechanics_max_scope_records]
        ]
        matches = [
            _match_evidence(match, self._mechanics_max_snippet_characters)
            for match in raw["matches"]
        ]
        truncated = (
            len(scopes) < raw["eligible_scope_count"]
            or len(excluded) < raw["excluded_page_count"]
        )
        evidence: dict[str, object] = {
            "evidence_class": "wiki_reference",
            "source_schema": raw["schema"],
            "scope_rule": raw["scope_rule"],
            "entity": raw["entity"],
            "retrieval_mode": retrieval_mode,
            "requested_query": request.query,
            "requested_query_match_count": requested_match_count,
            "effective_query": effective_query,
            "eligible_scope_count": raw["eligible_scope_count"],
            "eligible_scopes": scopes,
            "eligible_scopes_truncated": len(scopes) < raw["eligible_scope_count"],
            "excluded_page_count": raw["excluded_page_count"],
            "excluded_pages": excluded,
            "excluded_pages_truncated": len(excluded) < raw["excluded_page_count"],
            "match_count": raw["match_count"],
            "matches": matches,
        }
        while (
            len(json.dumps(evidence, ensure_ascii=False))
            > self._mechanics_max_characters
        ):
            if len(matches) > 1:
                matches.pop()
                truncated = True
                continue
            if len(scopes) > 1 or (scopes and matches):
                scopes.pop()
                evidence["eligible_scopes_truncated"] = True
                truncated = True
                continue
            if len(excluded) > 1:
                excluded.pop()
                evidence["excluded_pages_truncated"] = True
                truncated = True
                continue
            if matches and matches[0].get("snippet"):
                snippet = str(matches[0]["snippet"])
                matches[0]["snippet"] = snippet[: max(0, len(snippet) // 2)]
                truncated = True
                continue
            raise RuntimeError("mechanics knowledge metadata exceeds its output budget")
        if raw["eligible_scope_count"] == 0:
            status: Literal["matched", "no_match", "unknown", "error"] = "unknown"
        elif raw["match_count"] == 0:
            status = "no_match"
        else:
            status = "matched"
        return KnowledgeSearchResultV2(
            request=request,
            status=status,
            evidence=evidence,
            truncated=truncated,
        )
