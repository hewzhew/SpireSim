from sts_learning.exact_successor_search import select_complete_search_proposal


def test_search_proposal_requires_strict_improvement_over_baseline() -> None:
    tied = {
        "actions": (
            {
                "ordinal": 0,
                "exact_win_count": 4,
                "winning_final_hp_sum": 320,
                "budget_unknown_count": 0,
            },
            {
                "ordinal": 1,
                "exact_win_count": 4,
                "winning_final_hp_sum": 320,
                "budget_unknown_count": 0,
            },
        )
    }
    strict = {
        "actions": (
            {
                "ordinal": 0,
                "exact_win_count": 4,
                "winning_final_hp_sum": 300,
                "budget_unknown_count": 0,
            },
            {
                "ordinal": 1,
                "exact_win_count": 4,
                "winning_final_hp_sum": 320,
                "budget_unknown_count": 0,
            },
        )
    }

    assert select_complete_search_proposal(tied, baseline_ordinal=1) == (
        None,
        "no_search_improvement_over_baseline",
    )
    assert select_complete_search_proposal(strict, baseline_ordinal=0) == (
        1,
        "exact_win_count_then_winning_final_hp",
    )
