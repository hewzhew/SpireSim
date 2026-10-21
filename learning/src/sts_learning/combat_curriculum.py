"""Typed competence partitioning for exact combat roots."""

from __future__ import annotations

import operator
from collections.abc import Sequence
from dataclasses import dataclass
from enum import IntEnum

from .combat_objective import (
    CombatAllLossAxis,
    CombatAllWinAxis,
    CombatWinObjectiveConfig,
)
from .combat_signals import CombatGroupSignalSummary


class CombatCurriculumError(ValueError):
    """Combat-root competence evidence cannot form a safe curriculum."""


class CombatCompetenceBand(IntEnum):
    """Observed win support for one exact root under one frozen behavior."""

    ALL_LOSS = 0
    MIXED = 1
    ALL_WIN = 2
    UNRESOLVED = 3


@dataclass(frozen=True)
class CombatRootCompetenceEvidence:
    """Compact outcome and signal evidence for one artifact source slot."""

    source_slot: int
    root_id: str
    exact_combat_state_hash: str
    replicate_count: int
    wins: int
    losses: int
    unresolved: int
    signals: CombatGroupSignalSummary

    def __post_init__(self) -> None:
        source_slot = _nonnegative_integer(self.source_slot, "source_slot")
        replicate_count = _positive_integer(
            self.replicate_count,
            "replicate_count",
        )
        wins = _nonnegative_integer(self.wins, "wins")
        losses = _nonnegative_integer(self.losses, "losses")
        unresolved = _nonnegative_integer(self.unresolved, "unresolved")
        if wins + losses + unresolved != replicate_count:
            raise CombatCurriculumError(
                "combat competence terminals must equal replicate_count"
            )
        if not isinstance(self.signals, CombatGroupSignalSummary):
            raise CombatCurriculumError(
                "combat competence evidence requires typed signals"
            )
        if (self.root_id, self.exact_combat_state_hash) != (
            self.signals.root_id,
            self.signals.exact_combat_state_hash,
        ):
            raise CombatCurriculumError(
                "combat competence evidence changed exact root identity"
            )
        if self.signals.replicate_count != replicate_count:
            raise CombatCurriculumError(
                "combat competence signals changed replicate_count"
            )
        mixed = 0 < wins < replicate_count
        if self.signals.win.has_signal != mixed:
            raise CombatCurriculumError(
                "combat competence win signal disagrees with outcome support"
            )
        if wins == 0 and self.signals.terminal_hp.has_signal:
            raise CombatCurriculumError(
                "all-loss combat root cannot carry terminal-HP signal"
            )
        object.__setattr__(self, "source_slot", source_slot)
        object.__setattr__(self, "replicate_count", replicate_count)
        object.__setattr__(self, "wins", wins)
        object.__setattr__(self, "losses", losses)
        object.__setattr__(self, "unresolved", unresolved)

    @property
    def band(self) -> CombatCompetenceBand:
        if self.unresolved:
            return CombatCompetenceBand.UNRESOLVED
        if self.wins == 0:
            return CombatCompetenceBand.ALL_LOSS
        if self.wins == self.replicate_count:
            return CombatCompetenceBand.ALL_WIN
        return CombatCompetenceBand.MIXED


@dataclass(frozen=True)
class CombatFrontierPlan:
    """Exact train, rescue, and solved partition for one bounded root census."""

    objective_config: CombatWinObjectiveConfig
    roots: tuple[CombatRootCompetenceEvidence, ...]
    survival_frontier_slots: tuple[int, ...]
    resource_frontier_slots: tuple[int, ...]
    rescue_slots: tuple[int, ...]
    solved_slots: tuple[int, ...]

    def __post_init__(self) -> None:
        if not isinstance(self.objective_config, CombatWinObjectiveConfig):
            raise CombatCurriculumError(
                "combat frontier plan requires typed objective config"
            )
        roots = tuple(self.roots)
        if not roots or not all(
            isinstance(root, CombatRootCompetenceEvidence) for root in roots
        ):
            raise CombatCurriculumError(
                "combat frontier plan requires typed root evidence"
            )
        source_slots = tuple(root.source_slot for root in roots)
        identities = tuple(
            (root.root_id, root.exact_combat_state_hash) for root in roots
        )
        if len(set(source_slots)) != len(source_slots):
            raise CombatCurriculumError(
                "combat frontier plan repeats a source slot"
            )
        if len(set(identities)) != len(identities):
            raise CombatCurriculumError(
                "combat frontier plan repeats an exact root"
            )
        expected = _partition_slots(roots, self.objective_config)
        actual = (
            tuple(self.survival_frontier_slots),
            tuple(self.resource_frontier_slots),
            tuple(self.rescue_slots),
            tuple(self.solved_slots),
        )
        if actual != expected:
            raise CombatCurriculumError(
                "combat frontier plan categories disagree with root evidence"
            )
        object.__setattr__(self, "roots", roots)
        object.__setattr__(self, "survival_frontier_slots", actual[0])
        object.__setattr__(self, "resource_frontier_slots", actual[1])
        object.__setattr__(self, "rescue_slots", actual[2])
        object.__setattr__(self, "solved_slots", actual[3])

    @property
    def training_slots(self) -> tuple[int, ...]:
        selected = set(self.survival_frontier_slots)
        selected.update(self.resource_frontier_slots)
        return tuple(
            root.source_slot
            for root in self.roots
            if root.source_slot in selected
        )

    @property
    def root_count(self) -> int:
        return len(self.roots)

    def training_objective_config(self) -> CombatWinObjectiveConfig:
        if not self.training_slots:
            raise CombatCurriculumError(
                "combat frontier plan has no trainable roots"
            )
        return CombatWinObjectiveConfig(
            groups_per_update=len(self.training_slots),
            all_win_axis=self.objective_config.all_win_axis,
            all_loss_axis=self.objective_config.all_loss_axis,
            policy_update=self.objective_config.policy_update,
        )

    def evidence_for_slot(self, source_slot: int) -> CombatRootCompetenceEvidence:
        normalized = _nonnegative_integer(source_slot, "source_slot")
        for root in self.roots:
            if root.source_slot == normalized:
                return root
        raise CombatCurriculumError(
            "combat frontier plan does not contain source_slot"
        )


class CombatFrontierRootSource:
    """Expose only trainable frontier roots from an identity-checked source."""

    def __init__(self, source: object, plan: CombatFrontierPlan) -> None:
        if not callable(getattr(source, "combat_group", None)):
            raise CombatCurriculumError(
                "combat frontier root source requires combat_group"
            )
        if not isinstance(plan, CombatFrontierPlan):
            raise CombatCurriculumError(
                "combat frontier root source requires a typed plan"
            )
        if not plan.training_slots:
            raise CombatCurriculumError(
                "combat frontier plan has no trainable roots"
            )
        self.source = source
        self.plan = plan
        self.source_slots = plan.training_slots

    @property
    def root_count(self) -> int:
        return len(self.source_slots)

    def combat_group(self, slot_index: int, replicate_count: int):
        selected_slot = _nonnegative_integer(slot_index, "slot_index")
        if selected_slot >= len(self.source_slots):
            raise CombatCurriculumError(
                "combat frontier selected slot is out of range"
            )
        source_slot = self.source_slots[selected_slot]
        expected = self.plan.evidence_for_slot(source_slot)
        group = self.source.combat_group(source_slot, replicate_count)
        if (
            getattr(group, "root_id", None),
            getattr(group, "exact_combat_state_hash", None),
        ) != (expected.root_id, expected.exact_combat_state_hash):
            raise CombatCurriculumError(
                "combat frontier source changed an exact root"
            )
        return group


def build_combat_frontier_plan(
    roots: Sequence[CombatRootCompetenceEvidence],
    objective_config: CombatWinObjectiveConfig,
    *,
    max_roots: int,
) -> CombatFrontierPlan:
    """Partition bounded distinct roots without dropping hard or solved evidence."""

    if not isinstance(objective_config, CombatWinObjectiveConfig):
        raise CombatCurriculumError(
            "combat frontier plan requires typed objective config"
        )
    bound = _positive_integer(max_roots, "max_roots")
    normalized = tuple(roots)
    if not normalized:
        raise CombatCurriculumError(
            "combat frontier plan requires at least one root"
        )
    if len(normalized) > bound:
        raise CombatCurriculumError(
            "combat frontier plan exceeds max_roots"
        )
    partition = _partition_slots(normalized, objective_config)
    return CombatFrontierPlan(
        objective_config=objective_config,
        roots=normalized,
        survival_frontier_slots=partition[0],
        resource_frontier_slots=partition[1],
        rescue_slots=partition[2],
        solved_slots=partition[3],
    )


def _partition_slots(
    roots: Sequence[CombatRootCompetenceEvidence],
    objective_config: CombatWinObjectiveConfig,
) -> tuple[tuple[int, ...], tuple[int, ...], tuple[int, ...], tuple[int, ...]]:
    survival = []
    resource = []
    rescue = []
    solved = []
    for root in roots:
        if not isinstance(root, CombatRootCompetenceEvidence):
            raise CombatCurriculumError(
                "combat frontier plan requires typed root evidence"
            )
        if root.band is CombatCompetenceBand.UNRESOLVED:
            rescue.append(root.source_slot)
        elif root.band is CombatCompetenceBand.ALL_LOSS:
            if (
                objective_config.all_loss_axis
                is CombatAllLossAxis.ENEMY_HP_PROGRESS
                and root.losses == root.replicate_count
                and root.signals.enemy_hp_progress.has_signal
            ):
                survival.append(root.source_slot)
            else:
                rescue.append(root.source_slot)
        elif root.band is CombatCompetenceBand.MIXED:
            survival.append(root.source_slot)
        elif (
            objective_config.all_win_axis is CombatAllWinAxis.TERMINAL_HP
            and root.signals.terminal_hp.has_signal
        ):
            resource.append(root.source_slot)
        else:
            solved.append(root.source_slot)
    return (
        tuple(survival),
        tuple(resource),
        tuple(rescue),
        tuple(solved),
    )


def _positive_integer(value: object, name: str) -> int:
    normalized = _nonnegative_integer(value, name)
    if normalized == 0:
        raise CombatCurriculumError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatCurriculumError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatCurriculumError(f"{name} must be an integer") from error
    if normalized < 0:
        raise CombatCurriculumError(f"{name} must be non-negative")
    return normalized
