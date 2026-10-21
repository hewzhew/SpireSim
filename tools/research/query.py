from __future__ import annotations

from pathlib import Path
import re
import sqlite3
from typing import Any, Callable

from tools.expert.query import ExpertKnowledgeOwner
from tools.wiki.entity_query import search_entity_pages
from tools.wiki.wiki_corpus import read_page_sections


LOCAL_RESEARCH_ENTITY_SCHEMA = "sts-local-research-entity-v1"
WIKI_STRATEGY_HEADING_QUERY = "策略 OR 攻略 OR 战术 OR Tips OR Strategy"
WIKI_ENTITY_KINDS = frozenset(
    {"card", "potion", "relic", "enemy", "encounter", "event"}
)


class LocalResearchEvidence:
    """Retrieve attributed local evidence without making a gameplay decision."""

    def __init__(
        self,
        repository: Path,
        *,
        expert_owner: Any | None = None,
        page_search: Callable[..., dict[str, Any]] | None = None,
        page_sections: Callable[..., dict[str, Any] | None] | None = None,
        max_wiki_matches_per_entity: int = 4,
        max_wiki_strategy_characters: int = 8_000,
    ) -> None:
        if (
            max_wiki_matches_per_entity < 1
            or max_wiki_strategy_characters < 1
        ):
            raise ValueError("local research bounds must be positive")
        self._repository = repository.resolve(strict=True)
        self._expert_owner = expert_owner or ExpertKnowledgeOwner(self._repository)
        self._entity_database = (
            self._repository
            / ".oracle-lab"
            / "external-research"
            / "entities.sqlite3"
        )
        self._wiki_database = (
            self._repository
            / ".oracle-lab"
            / "external-research"
            / "wiki.sqlite3"
        )
        self._page_search = page_search or search_entity_pages
        self._page_sections = page_sections or read_page_sections
        self._injected_page_search = page_search is not None
        self._max_wiki_matches_per_entity = max_wiki_matches_per_entity
        self._max_wiki_strategy_characters = max_wiki_strategy_characters

    @staticmethod
    def _section_title_key(value: object) -> str:
        return " ".join(str(value or "").split()).casefold()

    @classmethod
    def _is_strategy_section_title(cls, value: object) -> bool:
        title = cls._section_title_key(value)
        return any(term in title for term in ("策略", "攻略", "战术")) or bool(
            re.search(r"\b(?:tips?|strateg(?:y|ies)|tactics?)\b", title)
        )

    def _strategy_passages(
        self, references: list[dict[str, Any]]
    ) -> list[dict[str, Any]]:
        pages: dict[tuple[str, int, int, str], dict[str, Any]] = {}
        for reference in references:
            for match in reference.get("matches", []):
                key = (
                    match["source_id"],
                    match["page_id"],
                    match["revision_id"],
                    match["revision_sha1"],
                )
                page = pages.setdefault(
                    key,
                    {
                        **{
                            field: match[field]
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
                        "shared_entity_page": bool(
                            match.get("shared_entity_page")
                        ),
                        "attributed_entity_ids": [],
                    },
                )
                if reference["entity_id"] not in page["attributed_entity_ids"]:
                    page["attributed_entity_ids"].append(reference["entity_id"])

        passages = []
        for page in pages.values():
            outline = self._page_sections(
                database=self._wiki_database,
                source_id=page["source_id"],
                title=page["title"],
                expected_revision_id=page["revision_id"],
                expected_revision_sha1=page["revision_sha1"],
                include_plain_text=True,
            )
            if outline is None:
                raise ValueError(
                    "matched Wiki page disappeared while reading strategy section: "
                    f"{page['source_id']}:{page['title']}"
                )
            for section in outline["sections"]:
                if not self._is_strategy_section_title(section.get("title")):
                    continue
                text = str(section.get("subtree_plain_text", "")).strip()
                character_count = len(text)
                returned_text = text[: self._max_wiki_strategy_characters]
                passages.append(
                    {
                        **page,
                        "section_document_order": section["document_order"],
                        "section_title": section["title"],
                        "section_path": section["path"],
                        "text": returned_text,
                        "text_character_count": character_count,
                        "returned_character_count": len(returned_text),
                        "truncated": len(returned_text) < character_count,
                    }
                )
        return passages

    def _wiki_reference(self, resolved: dict[str, Any]) -> dict[str, Any] | None:
        entity = resolved["entity"]
        kind = entity.get("kind")
        entity_id = entity.get("id")
        if kind not in WIKI_ENTITY_KINDS or not isinstance(entity_id, str):
            return None
        prefix = f"{kind}:"
        if not entity_id.startswith(prefix) or len(entity_id) == len(prefix):
            return {
                "entity_id": entity_id,
                "status": "error",
                "error": "reference entity does not expose kind-prefixed simulator identity",
            }
        stable_id = entity_id[len(prefix) :]
        identity = {
            "entity_id": entity_id,
            "kind": kind,
            "stable_id": stable_id,
            "canonical_name": entity.get("canonical_name"),
            "matched_alias": resolved.get("matched_alias"),
        }
        if not self._injected_page_search and (
            not self._entity_database.is_file() or not self._wiki_database.is_file()
        ):
            return {
                **identity,
                "status": "unavailable",
                "eligible_page_count": 0,
                "match_count": 0,
                "matches": [],
                "reason": "local Wiki corpus or entity graph is not installed",
            }
        try:
            result = self._page_search(
                entity_database=self._entity_database,
                wiki_database=self._wiki_database,
                kind=kind,
                stable_id=stable_id,
                query=WIKI_STRATEGY_HEADING_QUERY,
                include_related=kind == "encounter",
                raw_query=True,
                limit=self._max_wiki_matches_per_entity,
            )
        except (OSError, ValueError, sqlite3.Error) as error:
            return {
                **identity,
                "status": "error",
                "eligible_page_count": 0,
                "match_count": 0,
                "matches": [],
                "error": str(error),
            }
        eligible_page_count = int(result.get("eligible_page_count", 0))
        matches = list(result.get("matches", []))
        status = (
            "matched"
            if matches
            else "available_no_strategy_heading"
            if eligible_page_count
            else "no_accepted_page"
        )
        return {
            **identity,
            "status": status,
            "eligible_page_count": eligible_page_count,
            "match_count": len(matches),
            "eligible_pages": result.get("eligible_pages", []),
            "matches": matches,
        }

    def exact_entity(
        self,
        *,
        kind: str,
        stable_id: str,
        canonical_name: str | None = None,
    ) -> dict[str, Any]:
        """Read one simulator-identified entity without parsing natural language.

        This entry deliberately has no recommendation field.  The simulator's
        kind and stable ID own identity; aliases are used only inside the
        already-reviewed expert index and accepted Wiki graph.
        """

        if kind not in WIKI_ENTITY_KINDS:
            raise ValueError(f"unsupported research entity kind: {kind}")
        if not stable_id or not stable_id.strip():
            raise ValueError("research entity stable_id must be non-empty")
        stable_id = stable_id.strip()
        entity_id = f"{kind}:{stable_id}"
        query_name = entity_id
        try:
            expert = self._expert_owner.entity_evidence(query_name)
        except ValueError as error:
            if "unknown reference entity or alias" not in str(error):
                raise
            expert = {
                "schema": "sts-expert-entity-evidence-v1",
                "entity": {
                    "id": entity_id,
                    "kind": kind,
                    "canonical_name": canonical_name,
                },
                "status": "not_resolved",
                "direct_reviewed_passages": [],
                "direct_unreviewed_literal_matches": [],
                "related_reviewed_passages": [],
                "source_contexts": [],
                "boundary": (
                    "no reviewed expert entity record resolved; this is a coverage "
                    "state, not negative gameplay evidence"
                ),
            }
        resolved = {
            "entity": {
                "id": entity_id,
                "kind": kind,
                "canonical_name": canonical_name,
            },
            "matched_alias": None,
        }
        wiki_reference = self._wiki_reference(resolved)
        references = [wiki_reference] if wiki_reference is not None else []
        strategy_passages = self._strategy_passages(references)
        return {
            "schema": LOCAL_RESEARCH_ENTITY_SCHEMA,
            "entity": {
                "entity_id": entity_id,
                "kind": kind,
                "stable_id": stable_id,
                "canonical_name": canonical_name,
                "identity_source": "typed simulator request",
            },
            "expert_strategy": expert,
            "wiki_strategy": {
                "strategy_heading_query": WIKI_STRATEGY_HEADING_QUERY,
                "entities": references,
                "strategy_passage_count": len(strategy_passages),
                "strategy_passages": strategy_passages,
                "boundary": (
                    "Wiki strategy/tips text is revision-attributed page reference; "
                    "it is not expert consensus, a policy score, or a teacher label"
                ),
            },
            "boundary": (
                "typed identity, Wiki reference, Wiki strategy, and attributed expert "
                "strategy remain separate; this result makes no gameplay recommendation"
            ),
        }
