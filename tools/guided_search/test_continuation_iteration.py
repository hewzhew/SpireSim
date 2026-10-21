import pytest

from continuation_iteration import compare, validate_cases


def case(seed, group=None):
    return dict(label=f"case-{seed}", spec=dict(seed=seed), group=group or f"seed-{seed}")


def test_complete_combat_test_is_separate_from_training_and_validation():
    validate_cases([case(1)], [case(2)], [case(3)])
    with pytest.raises(ValueError, match="seeds must be disjoint"):
        validate_cases([case(1)], [case(2)], [case(1)])
    with pytest.raises(ValueError, match="groups overlap"):
        validate_cases([case(1, "source")], [case(2, "source")], [case(3)])


def test_budget_misses_do_not_become_deaths_or_zero_hp_wins():
    base = dict(rows=[dict(label=str(i), hp=h) for i,h in enumerate([None, 30, 40, None])])
    new = dict(rows=[dict(label=str(i), hp=h) for i,h in enumerate([20, None, 42, None])])
    assert compare(base, new) == dict(common_wins=1, hp_delta_on_common_wins=2,
                                     better=1, worse=0, equal=0,
                                     gained_witnesses=1, lost_witnesses=1)
