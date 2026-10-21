"""Aggregate contextual card values without enumerating discovery offers.

Contract: a uniform k-subset of distinct pool entries, one chosen entry, and
fixed values on one common objective/continuation scale. These are caller
estimates, not card rankings defined here. This does not model fixed-seed RNG
effects, offer-dependent continuation values, or future resource opportunity
cost. A potion's selected-copy count is already part of each input value.
"""
from dataclasses import dataclass
from math import comb, isfinite
from typing import Sequence


@dataclass(frozen=True)
class DiscoveryValue:
    expected_value: float
    selection_probabilities: tuple[float, ...]
    skip_probability: float


def _counts(pool_size, offer_count):
    if (type(pool_size) is not int or type(offer_count) is not int
            or not 1 <= offer_count <= pool_size):
        raise ValueError("require a nonempty pool and 1..pool_size offered entries")
    return comb(pool_size, offer_count)


def uniform_discovery_value(values: Sequence[float], *, offer_count=3,
                            skip_value=None) -> DiscoveryValue:
    """Expected best offered value; choose uniformly among tied best entries.

    If skipping is legal, skip ties at skip_value. Skip still consumes the
    potion: its value is the continuation after consumption with no card.
    """
    values = tuple(float(v) for v in values)
    total = _counts(len(values), offer_count)
    if not all(isfinite(v) for v in values) or (skip_value is not None
            and not isfinite(skip_value)):
        raise ValueError("discovery values must be finite")
    ordered = sorted(range(len(values)), key=values.__getitem__)
    probability = [0.] * len(values)
    skip_probability = expected = 0.
    start = 0
    while start < len(ordered):
        end = start + 1
        value = values[ordered[start]]
        while end < len(ordered) and values[ordered[end]] == value:
            end += 1
        mass = (comb(end, offer_count) - comb(start, offer_count)) / total
        if skip_value is not None and value <= skip_value:
            skip_probability += mass
            expected += mass * skip_value
        else:
            expected += mass * value
            for i in ordered[start:end]:
                probability[i] = mass / (end - start)
        start = end
    return DiscoveryValue(expected, tuple(probability), skip_probability)


def discovery_hit_probability(pool_size: int, matching_cards: int, *, offer_count=3) -> float:
    """Chance that a uniform distinct offer contains at least one matching card."""
    total = _counts(pool_size, offer_count)
    if type(matching_cards) is not int or not 0 <= matching_cards <= pool_size:
        raise ValueError("matching cards must be within the declared pool")
    return 1. - comb(pool_size - matching_cards, offer_count) / total
