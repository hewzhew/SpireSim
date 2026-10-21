from __future__ import annotations

from pathlib import Path

from sts_reference.knowledge import (
    LocalKnowledgeRetrieverV2,
    ResearchQueryV2,
    _natural_external_aliases,
)


class EmptyExpertOwner:
    def literal_search(self, *_: object, **__: object) -> dict[str, object]:
        return {
            "schema": "sts-expert-literal-search-v1",
            "total_match_count": 0,
            "returned_match_count": 0,
            "truncated": False,
            "matches": [],
        }


def test_expert_aliases_keep_natural_formatting_but_reject_protocol_only_ids() -> None:
    shrug = {
        "stable_id": "ShrugItOff",
        "display_name": None,
        "external_id": "Shrug It Off",
        "external_aliases": [],
    }
    apparition = {
        "stable_id": "Apparition",
        "display_name": None,
        "external_id": "Ghostly",
        "external_aliases": [],
    }

    assert _natural_external_aliases(shrug) == ["Shrug It Off"]
    assert _natural_external_aliases(apparition) == []


def mechanics_document(
    *, eligible_scope_count: int, match_count: int
) -> dict[str, object]:
    scope = {
        "source_id": "fixture-wiki",
        "page_id": 7,
        "page_title": "Brutality",
        "url": "https://example.invalid/Brutality",
        "revision_id": 11,
        "revision_timestamp": "2026-08-21T00:00:00Z",
        "revision_sha1": "fixture-sha1",
        "scope_type": "page",
        "scope_bindings": [
            {
                "scope": "direct",
                "entity_key": "card:Brutality",
                "kind": "card",
                "stable_id": "Brutality",
            }
        ],
    }
    return {
        "schema": "sts-wiki-entity-mechanics-search-v1",
        "scope_rule": "single_entity_page_or_unique_exact_section",
        "entity": {
            "entity_key": "card:Brutality",
            "kind": "card",
            "stable_id": "Brutality",
            "display_name": None,
        },
        "eligible_scope_count": eligible_scope_count,
        "eligible_scopes": [dict(scope) for _ in range(eligible_scope_count)],
        "excluded_page_count": 0,
        "excluded_pages": [],
        "match_count": match_count,
        "matches": [
            {**scope, "snippet": "Draw 1 additional card and lose 1 HP."}
            for _ in range(match_count)
        ],
    }


def test_mechanics_channel_is_bounded_cached_and_keeps_provenance(
    tmp_path: Path,
) -> None:
    calls: list[dict[str, object]] = []

    class Owner:
        def search(self, **arguments: object) -> dict[str, object]:
            calls.append(arguments)
            return mechanics_document(eligible_scope_count=3, match_count=2)

    retriever = LocalKnowledgeRetrieverV2(
        tmp_path,
        mechanics_owner=Owner(),
        expert_owner=EmptyExpertOwner(),
        mechanics_max_results=2,
        mechanics_max_scope_records=1,
    )
    request = ResearchQueryV2(
        channel="wiki_reference",
        entity_kind="card",
        stable_id="Brutality",
        query="Draw 1 additional card",
    )
    first = retriever.search(request)
    second = retriever.search(request)

    assert first is second
    assert len(calls) == 1
    assert calls[0]["limit"] == 2
    assert first.status == "matched"
    assert first.truncated is True
    assert first.evidence["eligible_scope_count"] == 3
    assert first.evidence["eligible_scopes_truncated"] is True
    scopes = first.evidence["eligible_scopes"]
    assert isinstance(scopes, list)
    assert scopes[0]["revision_sha1"] == "fixture-sha1"
    assert first.evidence["match_count"] == 2


def test_mechanics_unknown_and_no_match_remain_distinct(tmp_path: Path) -> None:
    documents = iter(
        [
            mechanics_document(eligible_scope_count=0, match_count=0),
            mechanics_document(eligible_scope_count=1, match_count=0),
            mechanics_document(eligible_scope_count=1, match_count=0),
        ]
    )

    class Owner:
        def search(self, **_: object) -> dict[str, object]:
            return next(documents)

    retriever = LocalKnowledgeRetrieverV2(
        tmp_path,
        mechanics_owner=Owner(),
        expert_owner=EmptyExpertOwner(),
    )
    unknown = retriever.search(
        ResearchQueryV2(
            channel="wiki_reference",
            entity_kind="relic",
            stable_id="Abacus",
            query="gain Block",
        )
    )
    no_match = retriever.search(
        ResearchQueryV2(
            channel="wiki_reference",
            entity_kind="card",
            stable_id="Brutality",
            query="unrelated literal",
        )
    )

    assert unknown.status == "unknown"
    assert unknown.evidence["eligible_scope_count"] == 0
    assert no_match.status == "no_match"
    assert no_match.evidence["eligible_scope_count"] == 1
    assert no_match.evidence["retrieval_mode"] == "exact_identity_fallback"


def test_mechanics_no_match_falls_back_to_exact_stable_identity(
    tmp_path: Path,
) -> None:
    calls: list[dict[str, object]] = []

    class Owner:
        def search(self, **arguments: object) -> dict[str, object]:
            calls.append(arguments)
            return mechanics_document(
                eligible_scope_count=3,
                match_count=0 if len(calls) == 1 else 2,
            )

    retriever = LocalKnowledgeRetrieverV2(
        tmp_path,
        mechanics_owner=Owner(),
        expert_owner=EmptyExpertOwner(),
    )
    result = retriever.search(
        ResearchQueryV2(
            channel="wiki_reference",
            entity_kind="card",
            stable_id="Brutality",
            query="self damage per turn and draw",
        )
    )

    assert [call["query"] for call in calls] == [
        "self damage per turn and draw",
        "Brutality",
    ]
    assert result.status == "matched"
    assert result.evidence["retrieval_mode"] == "exact_identity_fallback"
    assert result.evidence["requested_query"] == "self damage per turn and draw"
    assert result.evidence["requested_query_match_count"] == 0
    assert result.evidence["effective_query"] == "Brutality"
    assert result.evidence["match_count"] == 2


def test_mechanics_exact_scope_falls_back_to_localized_page_title(
    tmp_path: Path,
) -> None:
    calls: list[dict[str, object]] = []

    class Owner:
        def search(self, **arguments: object) -> dict[str, object]:
            calls.append(arguments)
            document = mechanics_document(
                eligible_scope_count=1,
                match_count=0 if len(calls) == 1 else 1,
            )
            for scope in document["eligible_scopes"]:
                scope["page_title"] = "进阶之灾"
            for match in document["matches"]:
                match["page_title"] = "进阶之灾"
            return document

    retriever = LocalKnowledgeRetrieverV2(
        tmp_path,
        mechanics_owner=Owner(),
        expert_owner=EmptyExpertOwner(),
    )
    result = retriever.search(
        ResearchQueryV2(
            channel="wiki_reference",
            entity_kind="card",
            stable_id="AscendersBane",
            query="AscendersBane",
        )
    )

    assert [call["query"] for call in calls] == ["AscendersBane", "进阶之灾"]
    assert result.status == "matched"
    assert result.evidence["retrieval_mode"] == "localized_title_fallback"
    assert result.evidence["requested_query_match_count"] == 0
    assert result.evidence["effective_query"] == "进阶之灾"


def test_expert_channel_uses_structured_resident_owner_and_cache(tmp_path: Path) -> None:
    calls = []

    class ExpertOwner:
        def literal_search(self, query: str, **bounds: object) -> dict[str, object]:
            calls.append((query, bounds))
            return {
                "schema": "sts-expert-literal-search-v1",
                "query": query,
                "source_id": None,
                "total_match_count": 2,
                "returned_match_count": 1,
                "truncated": True,
                "matches": [
                    {
                        "source_id": "fixture-guide",
                        "evidence_kind": "canonical_expert_corpus",
                        "path": "corpus/text.md",
                        "line_number": 7,
                        "text": "reviewed expert statement",
                    }
                ],
            }

    retriever = LocalKnowledgeRetrieverV2(
        tmp_path,
        mechanics_owner=object(),
        expert_owner=ExpertOwner(),
        expert_max_results=3,
        expert_max_characters=2_000,
    )
    request = ResearchQueryV2("expert", "Brutality")
    first = retriever.search(request)
    second = retriever.search(request)

    assert first is second
    assert len(calls) == 1
    assert calls[0][1]["max_results"] == 3
    assert calls[0][1]["max_characters"] == 2_000
    assert first.status == "matched"
    assert first.truncated is True
    assert first.evidence["matches"][0]["source_id"] == "fixture-guide"


def test_expert_channel_expands_exact_entity_aliases_once(tmp_path: Path) -> None:
    calls = []

    class ExpertOwner:
        def literal_search_any(
            self, queries: list[str], **bounds: object
        ) -> dict[str, object]:
            calls.append((queries, bounds))
            return {
                "schema": "sts-expert-literal-search-v1",
                "query": queries[0],
                "queries": queries,
                "source_id": None,
                "total_match_count": 1,
                "returned_match_count": 1,
                "truncated": False,
                "matches": [
                    {
                        "source_id": "fixture-guide",
                        "evidence_kind": "canonical_expert_corpus",
                        "path": "corpus/text.md",
                        "line_number": 9,
                        "text": "残暴每回合提供额外抽牌。",
                    }
                ],
            }

        def literal_search(self, query: str, **_: object) -> dict[str, object]:
            raise AssertionError(f"unexpected unexpanded search: {query}")

    retriever = LocalKnowledgeRetrieverV2(
        tmp_path,
        mechanics_owner=object(),
        expert_owner=ExpertOwner(),
        expert_alias_resolver=lambda query: [query, "残暴"],
    )

    result = retriever.search(ResearchQueryV2("expert", "Brutality"))

    assert calls[0][0] == ["Brutality", "残暴"]
    assert result.status == "matched"
    assert result.evidence["identity_expanded_queries"] == ["Brutality", "残暴"]


def test_expert_channel_prefers_reviewed_entity_evidence_over_literal_hits(
    tmp_path: Path,
) -> None:
    class ExpertOwner:
        def entity_evidence(self, name: str) -> dict[str, object]:
            assert name == "Apparition"
            return {
                "schema": "sts-expert-entity-evidence-v1",
                "entity": {"id": "card:Apparition"},
                "boundary": "unreviewed literals are not entity evidence",
                "source_contexts": [
                    {
                        "source_id": "fixture-guide",
                        "published_at": "2022-01-01",
                        "scope_note": "A20H with save/load allowed",
                    }
                ],
                "direct_reviewed_passages": [
                    {
                        "source_id": "fixture-guide",
                        "evidence_kind": "reviewed_expert_passage",
                        "path": "reference/expert/passages.jsonl",
                        "line_number": 4,
                        "record": {"id": "apparition-context"},
                        "excerpt": "Apparition has one reviewed conditional interaction.",
                    }
                ],
                "direct_unreviewed_literal_matches": [
                    {"record_id": "ambiguous-ethereal-slang"}
                ],
                "related_reviewed_passages": [],
            }

        def literal_search(self, *_: object, **__: object) -> dict[str, object]:
            raise AssertionError("reviewed entity evidence must replace literal search")

    retriever = LocalKnowledgeRetrieverV2(
        tmp_path,
        mechanics_owner=object(),
        expert_owner=ExpertOwner(),
        expert_alias_resolver=lambda _: ["Apparition", "灵体"],
    )

    result = retriever.search(ResearchQueryV2("expert", "灵体"))

    assert result.status == "matched"
    assert result.evidence["retrieval_mode"] == "reviewed_entity"
    assert result.evidence["identity_expanded_queries"] == ["灵体", "Apparition"]
    assert result.evidence["total_match_count"] == 1
    assert result.evidence["unreviewed_literal_match_count"] == 1
    assert result.evidence["source_contexts"][0]["scope_note"].endswith(
        "save/load allowed"
    )
    assert result.evidence["matches"][0]["text"].startswith("Apparition")
