"""Typed, evidence-only local reference service."""

from .models import (
    DecisionResearchRequest,
    DecisionResearchResponse,
    EntityResearchRequest,
    EntityResearchResponse,
)
from .research import ResearchService

__all__ = [
    "DecisionResearchRequest",
    "DecisionResearchResponse",
    "EntityResearchRequest",
    "EntityResearchResponse",
    "ResearchService",
]
