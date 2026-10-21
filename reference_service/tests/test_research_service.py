from pathlib import Path

import pytest
from pydantic import ValidationError

from sts_reference.models import (
    DecisionResearchRequest,
    EntityResearchRequest,
    GameplayEntityRef,
)
from sts_reference.research import ResearchService


REPOSITORY = Path(__file__).parents[2]


def test_entityless_candidate_must_be_explicit() -> None:
    with pytest.raises(ValidationError, match="entityless_reason"):
        DecisionResearchRequest.model_validate(
            {
                "schema": "sts-reference-decision-request-v3",
                "decision_site": "card_reward",
                "public_context": {},
                "candidates": [
                    {
                        "candidate_id": "skip",
                        "typed_candidate": {"kind": "skip_card_reward"},
                        "entities": [],
                    }
                ],
                "contextual_entities": [],
            }
        )


def test_card_reward_uses_exact_entities_and_has_no_recommendation_layer() -> None:
    service = ResearchService(REPOSITORY)
    request = DecisionResearchRequest.model_validate(
        {
            "schema": "sts-reference-decision-request-v3",
            "decision_site": "card_reward",
            "public_context": {"act": 1, "floor": 1},
            "candidates": [
                {
                    "candidate_id": stable_id,
                    "label": stable_id,
                    "typed_candidate": {"kind": "take_card", "card": stable_id},
                    "entities": [{"kind": "card", "stable_id": stable_id}],
                }
                for stable_id in ("BattleTrance", "Thunderclap", "IronWave")
            ]
            + [
                {
                    "candidate_id": "skip",
                    "label": "skip",
                    "typed_candidate": {"kind": "skip_card_reward"},
                    "entities": [],
                    "entityless_reason": "skip does not directly identify an entity",
                }
            ],
            "contextual_entities": [],
        }
    )

    response = service.research_decision(request)

    assert [row.candidate_id for row in response.candidate_coverage] == [
        "BattleTrance",
        "Thunderclap",
        "IronWave",
        "skip",
    ]
    battle_trance = next(
        row for row in response.candidate_entities if row.entity.stable_id == "BattleTrance"
    )
    assert battle_trance.wiki_reference.status == "matched"
    assert battle_trance.expert_strategy.status == "matched"
    assert response.diagnostics["decision_authority"] == "none"
    assert response.candidate_coverage[-1].entityless_reason


@pytest.mark.parametrize(
    ("kind", "stable_id"),
    [
        ("relic", "FusionHammer"),
        ("relic", "CallingBell"),
        ("enemy", "Lagavulin"),
        ("event", "WeMeetAgain"),
        ("event", "Neow"),
    ],
)
def test_representative_entity_kinds_resolve_without_identity_error(
    kind: str, stable_id: str
) -> None:
    response = ResearchService(REPOSITORY).research_entity(
        EntityResearchRequest(
            entity=GameplayEntityRef(kind=kind, stable_id=stable_id),
        )
    )

    assert response.result.entity.entity_key == f"{kind}:{stable_id}"
    assert response.result.wiki_reference.status != "error"
    assert response.result.expert_strategy.status != "error"


def test_hash_bound_expert_citation_round_trips() -> None:
    service = ResearchService(REPOSITORY)
    response = service.research_entity(
        EntityResearchRequest(
            entity=GameplayEntityRef(kind="card", stable_id="BattleTrance"),
        )
    )
    locator = response.result.expert_strategy.citations[0]

    citation = service.citation(locator)

    assert citation.citation_id == locator.citation_id
    assert citation.entity_key == "card:BattleTrance"
    assert citation.text


def test_hash_bound_wiki_citation_round_trips() -> None:
    service = ResearchService(REPOSITORY)
    response = service.research_entity(
        EntityResearchRequest(
            entity=GameplayEntityRef(kind="card", stable_id="BattleTrance"),
        )
    )
    locator = response.result.wiki_reference.citations[0]

    assert len(response.result.wiki_reference.citations) >= 2

    citation = service.citation(locator)

    assert citation.citation_id == locator.citation_id
    assert citation.entity_key == "card:BattleTrance"
    assert citation.revision_id == locator.location["revision_id"]
    assert citation.text

    tampered = locator.model_copy(update={"citation_id": "refv2:tampered"})
    with pytest.raises(ValueError, match="content hash"):
        service.citation(tampered)


def test_revision_bound_wiki_strategy_section_round_trips() -> None:
    service = ResearchService(REPOSITORY)
    response = service.research_entity(
        EntityResearchRequest(
            entity=GameplayEntityRef(kind="card", stable_id="TrueGrit"),
        )
    )
    locator = response.result.wiki_strategy.citations[0]

    citation = service.citation(locator)

    assert citation.evidence_class == "wiki_strategy"
    assert citation.entity_key == "card:TrueGrit"
    assert citation.locator["section_document_order"] >= 0
    assert citation.text
