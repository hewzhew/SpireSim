from __future__ import annotations

import pytest

from sts_learning.complete_turn_policy_improvement import (
    CompleteTurnCandidatePolicyOutcome,
    CompleteTurnPolicyDeviationRow,
    CompleteTurnPolicyImprovementError,
    CompleteTurnPolicyOutcome,
    CompleteTurnStrictResourceDominance,
)


def _outcome(
    ordinal: int, outcome: CompleteTurnPolicyOutcome
) -> CompleteTurnCandidatePolicyOutcome:
    return CompleteTurnCandidatePolicyOutcome(ordinal=ordinal, outcome=outcome)


def test_policy_relative_pairs_compare_only_resolved_wins_with_losses() -> None:
    row = CompleteTurnPolicyDeviationRow(
        continuation_contract_id="frozen-policy-v1",
        candidate_count=4,
        outcomes=(
            _outcome(0, CompleteTurnPolicyOutcome.RESOLVED_WIN),
            _outcome(1, CompleteTurnPolicyOutcome.UNKNOWN),
            _outcome(2, CompleteTurnPolicyOutcome.RESOLVED_WIN),
            _outcome(3, CompleteTurnPolicyOutcome.RESOLVED_LOSS),
        ),
    )

    assert row.resolved_preference_pairs() == ((0, 3), (2, 3))


def test_co_winners_and_all_unknown_rows_create_no_preference() -> None:
    co_winners = CompleteTurnPolicyDeviationRow(
        continuation_contract_id="frozen-policy-v1",
        candidate_count=2,
        outcomes=(
            _outcome(0, CompleteTurnPolicyOutcome.RESOLVED_WIN),
            _outcome(1, CompleteTurnPolicyOutcome.RESOLVED_WIN),
        ),
    )
    unknown = CompleteTurnPolicyDeviationRow(
        continuation_contract_id="frozen-policy-v1",
        candidate_count=2,
        outcomes=(
            _outcome(0, CompleteTurnPolicyOutcome.UNKNOWN),
            _outcome(1, CompleteTurnPolicyOutcome.UNKNOWN),
        ),
    )

    assert co_winners.resolved_preference_pairs() == ()
    assert unknown.resolved_preference_pairs() == ()


def test_rust_verified_resource_dominance_orders_only_the_named_co_winners() -> None:
    row = CompleteTurnPolicyDeviationRow(
        continuation_contract_id="frozen-policy-v1",
        candidate_count=4,
        outcomes=(
            _outcome(0, CompleteTurnPolicyOutcome.RESOLVED_WIN),
            _outcome(1, CompleteTurnPolicyOutcome.RESOLVED_WIN),
            _outcome(2, CompleteTurnPolicyOutcome.RESOLVED_WIN),
            _outcome(3, CompleteTurnPolicyOutcome.RESOLVED_LOSS),
        ),
        strict_resource_dominance=(
            CompleteTurnStrictResourceDominance(
                dominant_ordinal=1,
                dominated_ordinal=0,
            ),
        ),
    )

    assert row.resolved_preference_pairs() == (
        (0, 3),
        (1, 3),
        (2, 3),
        (1, 0),
    )


@pytest.mark.parametrize(
    "dominance, message",
    [
        (
            CompleteTurnStrictResourceDominance(0, 2),
            "two replay-verified wins",
        ),
        (
            CompleteTurnStrictResourceDominance(0, 3),
            "outside the candidate row",
        ),
    ],
)
def test_resource_dominance_rejects_unverified_or_misaligned_pairs(
    dominance: CompleteTurnStrictResourceDominance,
    message: str,
) -> None:
    with pytest.raises(CompleteTurnPolicyImprovementError, match=message):
        CompleteTurnPolicyDeviationRow(
            continuation_contract_id="frozen-policy-v1",
            candidate_count=3,
            outcomes=(
                _outcome(0, CompleteTurnPolicyOutcome.RESOLVED_WIN),
                _outcome(1, CompleteTurnPolicyOutcome.RESOLVED_WIN),
                _outcome(2, CompleteTurnPolicyOutcome.RESOLVED_LOSS),
            ),
            strict_resource_dominance=(dominance,),
        )


@pytest.mark.parametrize(
    "outcomes, message",
    [
        ((_outcome(0, CompleteTurnPolicyOutcome.UNKNOWN),), "every candidate"),
        (
            (
                _outcome(1, CompleteTurnPolicyOutcome.UNKNOWN),
                _outcome(0, CompleteTurnPolicyOutcome.UNKNOWN),
            ),
            "ordered and complete",
        ),
    ],
)
def test_policy_deviation_row_rejects_implicit_or_misaligned_unknowns(
    outcomes: tuple[CompleteTurnCandidatePolicyOutcome, ...], message: str
) -> None:
    with pytest.raises(CompleteTurnPolicyImprovementError, match=message):
        CompleteTurnPolicyDeviationRow(
            continuation_contract_id="frozen-policy-v1",
            candidate_count=2,
            outcomes=outcomes,
        )
