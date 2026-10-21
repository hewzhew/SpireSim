from itertools import combinations

import pytest

from tools.guided_search.discovery_value import (
    discovery_hit_probability, uniform_discovery_value,
)


@pytest.mark.parametrize("values", [(-4., -1., 2., 7., 9.), (3., 3., 3., 3., 3.), (-2., 0., 0., 5., 5.)])
@pytest.mark.parametrize("offer_count", [1, 3, 5])
@pytest.mark.parametrize("skip", [None, 0., 5.])
def test_aggregation_matches_explicit_offers_and_tied_choice_policy(values, offer_count, skip):
    offers = list(combinations(range(len(values)), offer_count))
    expected = skipped = 0.
    chosen = [0.] * len(values)
    for offer in offers:
        best = max(values[i] for i in offer)
        if skip is not None and best <= skip:
            expected += skip / len(offers)
            skipped += 1. / len(offers)
        else:
            expected += best / len(offers)
            winners = [i for i in offer if values[i] == best]
            for i in winners:
                chosen[i] += 1. / (len(offers) * len(winners))
    result = uniform_discovery_value(values, offer_count=offer_count, skip_value=skip)
    assert result.expected_value == pytest.approx(expected)
    assert result.skip_probability == pytest.approx(skipped)
    assert result.selection_probabilities == pytest.approx(chosen)
    assert sum(result.selection_probabilities) + result.skip_probability == pytest.approx(1.)


def test_context_can_promote_a_low_baseline_card_without_retraining_offer_sets():
    baseline = [0.] * 15
    specialized = baseline.copy()
    specialized[0] = 40.
    result = uniform_discovery_value(specialized)
    assert result.expected_value == pytest.approx(8.)
    assert result.selection_probabilities[0] == pytest.approx(.2)
    assert discovery_hit_probability(15, 1) == pytest.approx(.2)
    assert discovery_hit_probability(15, 3) == pytest.approx(1 - 220 / 455)


def test_invalid_pool_sizes_and_nonfinite_estimates_are_rejected():
    for values, count in [([], 3), ([0.], 3), ([0.], True), ([float("nan")] * 3, 3)]:
        with pytest.raises(ValueError):
            uniform_discovery_value(values, offer_count=count)
    with pytest.raises(ValueError):
        discovery_hit_probability(15, 16)
