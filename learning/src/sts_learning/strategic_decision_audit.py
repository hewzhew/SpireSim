"""Typed parsing for exact strategic candidates exposed by the Rust bridge."""

from __future__ import annotations

import copy
import json
import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass


class StrategicDecisionAuditError(ValueError):
    """A bridge strategic-decision audit is malformed or misaligned."""


@dataclass(frozen=True)
class StrategicDecisionAudit:
    decision_site: str
    candidates: tuple[dict[str, object], ...]

    def selected_mapping(self, ordinal: object) -> dict[str, object]:
        selected_ordinal = _nonnegative_integer(ordinal, "selected ordinal")
        if selected_ordinal >= len(self.candidates):
            raise StrategicDecisionAuditError(
                "selected strategic ordinal exceeds the audited candidate count"
            )
        return {
            "decision_site": self.decision_site,
            "selected_ordinal": selected_ordinal,
            "selected_action": copy.deepcopy(self.candidates[selected_ordinal]),
            "candidates": copy.deepcopy(self.candidates),
        }


def read_strategic_decision_audit(
    env: object,
    slot_index: object,
) -> StrategicDecisionAudit | None:
    slot = _nonnegative_integer(slot_index, "strategic audit slot")
    source = getattr(env, "strategic_decision_audit_json", None)
    if not callable(source):
        raise StrategicDecisionAuditError(
            "environment does not expose strategic_decision_audit_json()"
        )
    raw = source(slot)
    if raw is None:
        return None
    if not isinstance(raw, str) or not raw:
        raise StrategicDecisionAuditError(
            "strategic decision audit must be non-empty JSON text or null"
        )
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as error:
        raise StrategicDecisionAuditError(
            "strategic decision audit is not valid JSON"
        ) from error
    if not isinstance(payload, Mapping):
        raise StrategicDecisionAuditError(
            "strategic decision audit must decode to a mapping"
        )
    if payload.get("schema") not in (
        "sts-learning-strategic-decision-audit-v1",
        "sts-learning-strategic-decision-audit-v2",
    ):
        raise StrategicDecisionAuditError(
            "strategic decision audit schema is unsupported"
        )
    decision_site = payload.get("decision_site")
    if not isinstance(decision_site, str) or not decision_site:
        raise StrategicDecisionAuditError(
            "strategic decision audit site must be non-empty text"
        )
    raw_candidates = payload.get("candidates")
    if not isinstance(raw_candidates, Sequence) or isinstance(
        raw_candidates,
        (str, bytes),
    ):
        raise StrategicDecisionAuditError(
            "strategic decision candidates must be a sequence"
        )
    candidates: list[dict[str, object]] = []
    for raw_candidate in raw_candidates:
        if not isinstance(raw_candidate, Mapping):
            raise StrategicDecisionAuditError(
                "strategic decision candidate must be a mapping"
            )
        candidate = copy.deepcopy(dict(raw_candidate))
        kind = candidate.get("kind")
        if not isinstance(kind, str) or not kind:
            raise StrategicDecisionAuditError(
                "strategic decision candidate kind must be non-empty text"
            )
        candidates.append(candidate)
    if not candidates:
        raise StrategicDecisionAuditError(
            "strategic decision audit must contain at least one candidate"
        )
    return StrategicDecisionAudit(
        decision_site=decision_site,
        candidates=tuple(candidates),
    )


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise StrategicDecisionAuditError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise StrategicDecisionAuditError(f"{name} must be an integer") from error
    if normalized < 0:
        raise StrategicDecisionAuditError(f"{name} must be non-negative")
    return normalized
