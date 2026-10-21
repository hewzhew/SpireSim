"""Policy-relative evidence for complete-turn one-step deviations.

This module does not claim that a losing continuation makes its first candidate
globally bad. It only records whether the same frozen continuation contract won
or lost after each candidate. Unknown outcomes remain absent from preferences.
"""

from __future__ import annotations

import operator
from dataclasses import dataclass
from enum import Enum


class CompleteTurnPolicyImprovementError(ValueError):
    """Complete-turn continuation evidence is incomplete or misaligned."""


class CompleteTurnPolicyOutcome(str, Enum):
    RESOLVED_WIN = "resolved_win"
    RESOLVED_LOSS = "resolved_loss"
    UNKNOWN = "unknown"


@dataclass(frozen=True)
class CompleteTurnCandidatePolicyOutcome:
    ordinal: int
    outcome: CompleteTurnPolicyOutcome

    def __post_init__(self) -> None:
        try:
            ordinal = operator.index(self.ordinal)
        except TypeError as error:
            raise CompleteTurnPolicyImprovementError(
                "candidate ordinal must be an integer"
            ) from error
        if ordinal < 0:
            raise CompleteTurnPolicyImprovementError(
                "candidate ordinal must be non-negative"
            )
        if not isinstance(self.outcome, CompleteTurnPolicyOutcome):
            raise CompleteTurnPolicyImprovementError(
                "candidate outcome must be a CompleteTurnPolicyOutcome"
            )
        object.__setattr__(self, "ordinal", ordinal)


@dataclass(frozen=True)
class CompleteTurnStrictResourceDominance:
    """One Rust-verified strict dominance relation between complete wins."""

    dominant_ordinal: int
    dominated_ordinal: int

    def __post_init__(self) -> None:
        for field in ("dominant_ordinal", "dominated_ordinal"):
            value = getattr(self, field)
            try:
                ordinal = operator.index(value)
            except TypeError as error:
                raise CompleteTurnPolicyImprovementError(
                    f"{field} must be an integer"
                ) from error
            if ordinal < 0:
                raise CompleteTurnPolicyImprovementError(
                    f"{field} must be non-negative"
                )
            object.__setattr__(self, field, ordinal)
        if self.dominant_ordinal == self.dominated_ordinal:
            raise CompleteTurnPolicyImprovementError(
                "strict resource dominance cannot compare a candidate with itself"
            )


@dataclass(frozen=True)
class CompleteTurnPolicyDeviationRow:
    continuation_contract_id: str
    candidate_count: int
    outcomes: tuple[CompleteTurnCandidatePolicyOutcome, ...]
    strict_resource_dominance: tuple[CompleteTurnStrictResourceDominance, ...] = ()

    def __post_init__(self) -> None:
        if not isinstance(self.continuation_contract_id, str) or not (
            self.continuation_contract_id.strip()
        ):
            raise CompleteTurnPolicyImprovementError(
                "continuation contract id must be non-empty"
            )
        try:
            candidate_count = operator.index(self.candidate_count)
        except TypeError as error:
            raise CompleteTurnPolicyImprovementError(
                "candidate count must be an integer"
            ) from error
        if candidate_count <= 0:
            raise CompleteTurnPolicyImprovementError(
                "candidate count must be positive"
            )
        if len(self.outcomes) != candidate_count:
            raise CompleteTurnPolicyImprovementError(
                "one explicit outcome is required for every candidate"
            )
        ordinals = tuple(outcome.ordinal for outcome in self.outcomes)
        if ordinals != tuple(range(candidate_count)):
            raise CompleteTurnPolicyImprovementError(
                "candidate outcomes must be ordered and complete"
            )
        dominance_pairs: list[tuple[int, int]] = []
        for dominance in self.strict_resource_dominance:
            if not isinstance(dominance, CompleteTurnStrictResourceDominance):
                raise CompleteTurnPolicyImprovementError(
                    "strict resource dominance must use typed evidence"
                )
            pair = (dominance.dominant_ordinal, dominance.dominated_ordinal)
            if any(ordinal >= candidate_count for ordinal in pair):
                raise CompleteTurnPolicyImprovementError(
                    "strict resource dominance ordinal is outside the candidate row"
                )
            if any(
                self.outcomes[ordinal].outcome
                is not CompleteTurnPolicyOutcome.RESOLVED_WIN
                for ordinal in pair
            ):
                raise CompleteTurnPolicyImprovementError(
                    "strict resource dominance requires two replay-verified wins"
                )
            if pair in dominance_pairs:
                raise CompleteTurnPolicyImprovementError(
                    "strict resource dominance pairs must be unique"
                )
            if (pair[1], pair[0]) in dominance_pairs:
                raise CompleteTurnPolicyImprovementError(
                    "strict resource dominance cannot contain a two-candidate cycle"
                )
            dominance_pairs.append(pair)
        object.__setattr__(self, "candidate_count", candidate_count)

    def resolved_preference_pairs(self) -> tuple[tuple[int, int], ...]:
        """Return win-over-loss pairs without ordering co-winners or unknowns."""

        wins = tuple(
            outcome.ordinal
            for outcome in self.outcomes
            if outcome.outcome is CompleteTurnPolicyOutcome.RESOLVED_WIN
        )
        losses = tuple(
            outcome.ordinal
            for outcome in self.outcomes
            if outcome.outcome is CompleteTurnPolicyOutcome.RESOLVED_LOSS
        )
        terminal_pairs = tuple((win, loss) for win in wins for loss in losses)
        resource_pairs = tuple(
            (dominance.dominant_ordinal, dominance.dominated_ordinal)
            for dominance in self.strict_resource_dominance
        )
        return terminal_pairs + resource_pairs
