"""Typed model-facing potion lanes shared by combat training and evaluation."""

from __future__ import annotations

import operator
from collections.abc import Sequence
from enum import Enum


class CombatPotionLaneError(RuntimeError):
    """A combat root source did not honor its declared potion lane."""


class CombatPotionLane(Enum):
    """Potion actions exposed to the model without changing engine legality."""

    ALL = "all"
    NEVER = "never"
    ROOT_SLOTS = "root-slots"


def normalize_combat_potion_slots(
    lane: CombatPotionLane,
    root_slots: Sequence[int] = (),
) -> tuple[int, ...]:
    """Validate the root-slot payload that completes one typed lane."""

    if not isinstance(lane, CombatPotionLane):
        raise CombatPotionLaneError("combat potion lane must be typed")
    normalized = tuple(
        _nonnegative_integer(slot, f"root_slots[{index}]")
        for index, slot in enumerate(root_slots)
    )
    if len(set(normalized)) != len(normalized):
        raise CombatPotionLaneError("combat potion root slots must be distinct")
    if lane is CombatPotionLane.ROOT_SLOTS and not normalized:
        raise CombatPotionLaneError(
            "root-slots potion lane requires at least one root slot"
        )
    if lane is not CombatPotionLane.ROOT_SLOTS and normalized:
        raise CombatPotionLaneError(
            "only the root-slots potion lane accepts root slots"
        )
    return normalized


class CombatPotionLaneRootSource:
    """Bind one root source to a checked model-facing potion action surface."""

    def __init__(
        self,
        source: object,
        lane: CombatPotionLane,
        root_slots: Sequence[int] = (),
    ) -> None:
        if not callable(getattr(source, "combat_group", None)):
            raise CombatPotionLaneError(
                "combat potion lane requires a combat-root source"
            )
        self.source = source
        self.lane = lane
        self.root_slots = normalize_combat_potion_slots(lane, root_slots)

    def combat_group(self, slot_index: int, replicate_count: int):
        requested = (
            None
            if self.lane is CombatPotionLane.ALL
            else (() if self.lane is CombatPotionLane.NEVER else self.root_slots)
        )
        group = (
            self.source.combat_group(slot_index, replicate_count)
            if requested is None
            else self.source.combat_group(slot_index, replicate_count, requested)
        )
        actual = getattr(group, "potion_slots", object())
        normalized_actual = None if actual is None else tuple(actual)
        if normalized_actual != requested:
            raise CombatPotionLaneError(
                "combat root source ignored the declared potion lane"
            )
        return group

    def public_run_contexts(self):
        """Preserve public root provenance beside the changed action surface."""

        provider = getattr(self.source, "public_run_contexts", None)
        if not callable(provider):
            raise CombatPotionLaneError(
                "combat root source omitted public run contexts"
            )
        return provider()


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatPotionLaneError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatPotionLaneError(f"{name} must be an integer") from error
    if normalized < 0:
        raise CombatPotionLaneError(f"{name} must be non-negative")
    return normalized
