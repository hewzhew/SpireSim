import sqlite3
from copy import deepcopy
from pathlib import Path

import pytest

from sts_reference.models import (
    DecisionResearchRequest,
    EntityResearchRequest,
    GameplayEntityRef,
)
from sts_reference.research import ResearchService, _citation_locator


REPOSITORY = Path(__file__).parents[2]
LANES = ("wiki_reference", "wiki_strategy", "expert_strategy")


@pytest.fixture(scope="module")
def service():
    return ResearchService(REPOSITORY)


def entity_request(display_name=None, stable_id="BattleTrance"):
    return EntityResearchRequest(
        entity=GameplayEntityRef(kind="card", stable_id=stable_id, display_name=display_name),
    )


def decision():
    return DecisionResearchRequest.model_validate(
        {
            "schema": "sts-reference-decision-request-v3",
            "decision_site": "card_reward",
            "public_context": {"input_provenance": "constructed", "act": 1, "hp": 25},
            "candidates": [
                {
                    "candidate_id": "take",
                    "label": "Battle Trance",
                    "typed_candidate": {"kind": "take_card", "card": "BattleTrance"},
                    "entities": [
                        {
                            "kind": "card",
                            "stable_id": "BattleTrance",
                            "simulator_facts": {"cost": 0},
                        }
                    ],
                },
                {
                    "candidate_id": "skip",
                    "typed_candidate": {"kind": "skip_card_reward"},
                    "entities": [],
                    "entityless_reason": "Skip adds no entity",
                },
            ],
            "contextual_entities": [{"kind": "enemy", "stable_id": "Lagavulin"}],
        }
    )


def test_display_name_cannot_redirect_exact_entity_or_citation(service):
    plain = service.research_entity(entity_request()).result
    mislabeled = service.research_entity(
        entity_request("Bash")
    ).result
    for lane in LANES:
        left, right = getattr(plain, lane), getattr(mislabeled, lane)
        assert left.status == right.status != "error"
        assert [c.citation_id for c in left.citations] == [
            c.citation_id for c in right.citations
        ]
    for locator in mislabeled.expert_strategy.citations:
        assert service.citation(locator).entity_key == "card:BattleTrance"
    exact = service._research_evidence.exact_entity(
        kind="card", stable_id="BattleTrance", canonical_name="Bash"
    )
    assert exact["expert_strategy"]["entity"]["id"] == "card:BattleTrance"


def test_unknown_identity_cannot_borrow_the_display_names_evidence(service):
    result = service.research_entity(
        entity_request("BattleTrance", "NotAnInstalledCard")
    ).result
    assert result.entity.entity_key == "card:NotAnInstalledCard"
    for lane in LANES:
        assert getattr(result, lane).status != "matched"
        assert not getattr(result, lane).citations


def test_broken_generation_preserves_candidate_identity_public_context_and_supplied_facts(
    monkeypatch,
):
    def unavailable(self):
        raise ValueError("stale derived entity index")

    monkeypatch.setattr(ResearchService, "_load_owners", unavailable)
    broken = ResearchService(REPOSITORY)
    request = decision()
    response = broken.research_decision(request)
    assert response.public_context == request.public_context
    assert [row.candidate_id for row in response.candidate_coverage] == ["take", "skip"]
    assert response.candidate_coverage[1].entityless_reason == "Skip adds no entity"
    result = response.candidate_entities[0]
    assert result.simulator_fact.evidence["facts"] == {"cost": 0}
    for entity in [*response.candidate_entities, *response.contextual_entities]:
        for lane in LANES:
            assert getattr(entity, lane).status == "error"
            assert "stale derived" in getattr(entity, lane).evidence["error"]


def test_read_failure_is_error_and_does_not_fall_back_to_a_previous_result(
    service, monkeypatch,
):
    request = entity_request()
    assert service.research_entity(request).result.expert_strategy.status == "matched"
    with monkeypatch.context() as patch:

        def unavailable(**kwargs):
            raise sqlite3.DatabaseError("fixture database unavailable")

        patch.setattr(service._research_evidence, "exact_entity", unavailable)
        result = service.research_entity(request).result
        for lane in LANES:
            assert getattr(result, lane).status == "error"
            assert not getattr(result, lane).citations
    assert service.research_entity(request).result.expert_strategy.status == "matched"


@pytest.mark.parametrize("changed_field", ["excerpt", "record_locator"])
def test_citation_rejects_changed_content_at_the_same_location(
    service, monkeypatch, changed_field,
):
    locator = service.research_entity(entity_request()).result.expert_strategy.citations[0]
    assert service.citation(locator).text
    original = service._expert.entity_evidence

    def changed(query):
        evidence = deepcopy(original(query))
        for passage in evidence.get("direct_reviewed_passages", []):
            if changed_field == "excerpt":
                passage["excerpt"] = "Changed source text at the same location"
            else:
                passage["record"]["locator"] = "Changed record locator"
        return evidence

    monkeypatch.setattr(service._expert, "entity_evidence", changed)
    with pytest.raises(ValueError, match="content no longer matches"):
        service.citation(locator)


def test_citation_without_a_content_pin_requires_fresh_research(service):
    locator = service.research_entity(entity_request()).result.expert_strategy.citations[0]
    location = dict(locator.location)
    location.pop("content_sha256")
    old_locator = _citation_locator(
        evidence_class=locator.evidence_class,
        entity=locator.entity,
        source_id=locator.source_id,
        location=location,
    )
    with pytest.raises(ValueError, match="research the entity again"):
        service.citation(old_locator)
