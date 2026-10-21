from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator


EntityKind = Literal["card", "potion", "relic", "enemy", "encounter", "event"]
ResearchStatus = Literal["matched", "no_match", "unknown", "error"]
EvidenceClass = Literal[
    "simulator_fact",
    "wiki_reference",
    "wiki_strategy",
    "expert_strategy",
]


class GameplayEntityRef(BaseModel):
    """One exact simulator-owned gameplay identity plus optional typed facts."""

    model_config = ConfigDict(extra="forbid")

    kind: EntityKind
    stable_id: str = Field(min_length=1, max_length=128)
    display_name: str | None = Field(default=None, max_length=256)
    simulator_facts: dict[str, Any] = Field(default_factory=dict)

    @property
    def entity_key(self) -> str:
        return f"{self.kind}:{self.stable_id}"


class ResearchCandidate(BaseModel):
    """A complete typed decision candidate; identity never comes from its label."""

    model_config = ConfigDict(extra="forbid")

    candidate_id: str = Field(min_length=1, max_length=512)
    label: str | None = Field(default=None, max_length=1_024)
    typed_candidate: dict[str, Any]
    entities: list[GameplayEntityRef] = Field(default_factory=list)
    entityless_reason: str | None = Field(default=None, max_length=512)

    @model_validator(mode="after")
    def validate_identity_boundary(self) -> ResearchCandidate:
        keys = [entity.entity_key for entity in self.entities]
        if len(keys) != len(set(keys)):
            raise ValueError("candidate entity identities must be distinct")
        if not self.entities and not self.entityless_reason:
            raise ValueError(
                "an entityless candidate requires an explicit entityless_reason"
            )
        if self.entities and self.entityless_reason is not None:
            raise ValueError(
                "a candidate with entities cannot also claim an entityless_reason"
            )
        return self


class DecisionResearchRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    schema_id: Literal["sts-reference-decision-request-v3"] = Field(alias="schema")
    decision_site: str = Field(min_length=1, max_length=128)
    public_context: dict[str, Any]
    candidates: list[ResearchCandidate] = Field(min_length=1)
    contextual_entities: list[GameplayEntityRef] = Field(default_factory=list)

    @model_validator(mode="after")
    def validate_complete_surface(self) -> DecisionResearchRequest:
        candidate_ids = [candidate.candidate_id for candidate in self.candidates]
        if len(candidate_ids) != len(set(candidate_ids)):
            raise ValueError("decision candidate IDs must be distinct")
        context_keys = [entity.entity_key for entity in self.contextual_entities]
        if len(context_keys) != len(set(context_keys)):
            raise ValueError("contextual entity identities must be distinct")
        return self


class EntityResearchRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    schema_id: Literal["sts-reference-entity-request-v2"] = Field(
        default="sts-reference-entity-request-v2",
        alias="schema",
    )
    entity: GameplayEntityRef


class CitationLocator(BaseModel):
    """Self-contained, hash-bound locator accepted by research_citation."""

    model_config = ConfigDict(extra="forbid")

    citation_id: str
    evidence_class: Literal["wiki_reference", "wiki_strategy", "expert_strategy"]
    entity: GameplayEntityRef
    source_id: str
    location: dict[str, Any]


class EvidenceLane(BaseModel):
    model_config = ConfigDict(extra="forbid")

    evidence_class: EvidenceClass
    status: ResearchStatus
    query: str
    citations: list[CitationLocator]
    evidence: dict[str, Any]
    truncated: bool = False


class EntityEvidence(BaseModel):
    model_config = ConfigDict(extra="forbid")

    entity: GameplayEntityRef
    simulator_fact: EvidenceLane
    wiki_reference: EvidenceLane
    wiki_strategy: EvidenceLane
    expert_strategy: EvidenceLane


class CandidateEvidenceCoverage(BaseModel):
    model_config = ConfigDict(extra="forbid")

    candidate_id: str
    label: str | None
    entity_keys: list[str]
    entityless_reason: str | None
    evidence_status_by_entity: dict[str, dict[str, ResearchStatus]]


class DecisionResearchResponse(BaseModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    schema_id: Literal["sts-reference-decision-response-v2"] = Field(
        default="sts-reference-decision-response-v2",
        alias="schema",
    )
    decision_site: str
    public_context: dict[str, Any]
    candidate_coverage: list[CandidateEvidenceCoverage]
    candidate_entities: list[EntityEvidence]
    contextual_entities: list[EntityEvidence]
    site_projection: dict[str, Any] | None = None
    boundaries: list[str]
    diagnostics: dict[str, Any]


class EntityResearchResponse(BaseModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    schema_id: Literal["sts-reference-entity-response-v2"] = Field(
        default="sts-reference-entity-response-v2",
        alias="schema",
    )
    result: EntityEvidence
    boundaries: list[str]


class CitationRecord(BaseModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    schema_id: Literal["sts-reference-citation-v2"] = Field(
        default="sts-reference-citation-v2",
        alias="schema",
    )
    citation_id: str
    evidence_class: Literal["wiki_reference", "wiki_strategy", "expert_strategy"]
    entity_key: str
    source_id: str
    title: str | None
    url: str | None
    revision_id: int | None
    revision_timestamp: str | None
    locator: dict[str, Any]
    text: str


class DiagnosticResponse(BaseModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    schema_id: Literal["sts-reference-diagnostic-v3"] = Field(
        default="sts-reference-diagnostic-v3",
        alias="schema",
    )
    entity: GameplayEntityRef
    lane_statuses: dict[str, ResearchStatus]
    details: dict[str, Any]


class ServiceStatusResponse(BaseModel):
    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    schema_id: Literal["sts-reference-service-status-v2"] = Field(
        default="sts-reference-service-status-v2",
        alias="schema",
    )
    service_name: Literal["sts_reference"] = "sts_reference"
    package_version: str
    current_package_version: str
    python_version: str
    python_executable: str
    process_id: int
    started_at: str
    repository: str
    tools: list[str]
    runtime_source_sha256: str
    current_source_sha256: str
    implementation_mode: Literal["fresh_subprocess_per_call"]
    implementation_source_sha256: str
    last_worker_invocation_id: str | None
    last_worker_process_id: int | None
    last_worker_source_sha256: str | None
    restart_required: bool
