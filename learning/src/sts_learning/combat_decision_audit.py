"""Typed parsing for exact combat candidates exposed by the Rust bridge."""

from __future__ import annotations

import copy
import json
import operator
from collections.abc import Mapping, Sequence
from dataclasses import dataclass


class CombatDecisionAuditError(ValueError):
    """A bridge combat-decision audit is malformed or misaligned."""


@dataclass(frozen=True)
class CombatDecisionAudit:
    phase: str
    selection_prefix: tuple[int, ...]
    candidates: tuple[dict[str, object], ...]


def read_combat_decision_audit(
    env: object,
    replicate_index: object,
) -> CombatDecisionAudit | None:
    """Read one diagnostic candidate surface without advancing the environment."""

    replicate = _nonnegative_integer(replicate_index, "combat audit replicate")
    source = getattr(env, "combat_decision_audit_json", None)
    if not callable(source):
        raise CombatDecisionAuditError(
            "combat environment does not expose combat_decision_audit_json()"
        )
    raw = source(replicate)
    if raw is None:
        return None
    if not isinstance(raw, str) or not raw:
        raise CombatDecisionAuditError(
            "combat decision audit must be non-empty JSON text or null"
        )
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as error:
        raise CombatDecisionAuditError(
            "combat decision audit is not valid JSON"
        ) from error
    if not isinstance(payload, Mapping):
        raise CombatDecisionAuditError(
            "combat decision audit must decode to a mapping"
        )
    if payload.get("schema") != "sts-learning-combat-decision-audit-v1":
        raise CombatDecisionAuditError(
            "combat decision audit schema is unsupported"
        )
    phase = payload.get("phase")
    if phase not in ("combat_root", "combat_selection"):
        raise CombatDecisionAuditError(
            "combat decision audit phase is unsupported"
        )
    raw_prefix = payload.get("selection_prefix")
    if not isinstance(raw_prefix, Sequence) or isinstance(
        raw_prefix,
        (str, bytes),
    ):
        raise CombatDecisionAuditError(
            "combat decision selection prefix must be a sequence"
        )
    selection_prefix = tuple(
        _nonnegative_integer(value, f"selection_prefix[{index}]")
        for index, value in enumerate(raw_prefix)
    )
    if phase == "combat_root" and selection_prefix:
        raise CombatDecisionAuditError(
            "combat root audit cannot carry a selection prefix"
        )

    raw_candidates = payload.get("candidates")
    if not isinstance(raw_candidates, Sequence) or isinstance(
        raw_candidates,
        (str, bytes),
    ):
        raise CombatDecisionAuditError(
            "combat decision candidates must be a sequence"
        )
    candidates: list[dict[str, object]] = []
    for raw_candidate in raw_candidates:
        if not isinstance(raw_candidate, Mapping):
            raise CombatDecisionAuditError(
                "combat decision candidate must be a mapping"
            )
        candidate = copy.deepcopy(dict(raw_candidate))
        kind = candidate.get("kind")
        if not isinstance(kind, str) or not kind:
            raise CombatDecisionAuditError(
                "combat decision candidate kind must be non-empty text"
            )
        candidates.append(candidate)
    if not candidates:
        raise CombatDecisionAuditError(
            "combat decision audit must contain at least one candidate"
        )
    return CombatDecisionAudit(
        phase=phase,
        selection_prefix=selection_prefix,
        candidates=tuple(candidates),
    )


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatDecisionAuditError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatDecisionAuditError(f"{name} must be an integer") from error
    if normalized < 0:
        raise CombatDecisionAuditError(f"{name} must be non-negative")
    return normalized
