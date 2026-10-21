"""Stable identities for public policy-decision prefixes.

The prefix contains only Rust-sanitized public decision snapshots and the
public candidate identities actually selected at earlier boundaries.  Episode
seeds, slot ordinals, simulator handles, and private RNG state are deliberately
absent.  This is the strongest history boundary currently available from the
learning bridge; it is a decision-boundary history, not yet a transcript of
every non-decision public event.
"""

from __future__ import annotations

import hashlib
import json
from collections.abc import Sequence
from dataclasses import dataclass

from .decision_progress import PublicDecisionSnapshot


class PublicDecisionPrefixError(ValueError):
    """A public decision prefix was malformed or privately identified."""


_PREFIX_PERSON = b"sts-pub-prefix1"


@dataclass(frozen=True)
class PublicDecisionPrefixStepV1:
    """One earlier public decision and the public action selected there."""

    snapshot_id: str
    selected_candidate_id: str

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "snapshot_id",
            _nonempty_text(self.snapshot_id, "snapshot_id"),
        )
        object.__setattr__(
            self,
            "selected_candidate_id",
            _nonempty_text(
                self.selected_candidate_id,
                "selected_candidate_id",
            ),
        )


def public_combat_entry_prefix_id_v1(
    previous_decisions: Sequence[PublicDecisionPrefixStepV1],
    current_snapshot: PublicDecisionSnapshot,
) -> str:
    """Bind one combat entry to its complete captured public decision prefix."""

    if not isinstance(current_snapshot, PublicDecisionSnapshot):
        raise PublicDecisionPrefixError(
            "current_snapshot must be a typed public decision snapshot"
        )
    if not current_snapshot.is_combat:
        raise PublicDecisionPrefixError("current snapshot must be a combat decision")
    try:
        previous = tuple(previous_decisions)
    except TypeError as error:
        raise PublicDecisionPrefixError(
            "previous_decisions must be a sequence"
        ) from error
    if not all(isinstance(step, PublicDecisionPrefixStepV1) for step in previous):
        raise PublicDecisionPrefixError(
            "previous_decisions must contain typed prefix steps"
        )
    payload = {
        "schema": "sts-learning-public-decision-prefix-v1",
        "previous_decisions": tuple(
            {
                "snapshot_id": step.snapshot_id,
                "selected_candidate_id": step.selected_candidate_id,
            }
            for step in previous
        ),
        "current_combat_snapshot_id": current_snapshot.snapshot_id,
    }
    encoded = json.dumps(
        payload,
        ensure_ascii=True,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("ascii")
    return hashlib.blake2b(
        encoded,
        digest_size=32,
        person=_PREFIX_PERSON,
    ).hexdigest()


def selected_public_prefix_step_v1(
    snapshot: PublicDecisionSnapshot,
    selected_ordinal: int,
) -> PublicDecisionPrefixStepV1:
    """Resolve a policy ordinal before private execution handles are applied."""

    if not isinstance(snapshot, PublicDecisionSnapshot):
        raise PublicDecisionPrefixError("snapshot must be typed")
    if isinstance(selected_ordinal, bool) or not isinstance(selected_ordinal, int):
        raise PublicDecisionPrefixError("selected_ordinal must be an integer")
    if not 0 <= selected_ordinal < len(snapshot.candidate_ids):
        raise PublicDecisionPrefixError(
            "selected_ordinal is outside the public candidate surface"
        )
    return PublicDecisionPrefixStepV1(
        snapshot_id=snapshot.snapshot_id,
        selected_candidate_id=snapshot.candidate_ids[selected_ordinal],
    )


def _nonempty_text(value: object, name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise PublicDecisionPrefixError(f"{name} must be non-empty text")
    return value
