"""Bounded execution loop for one exact same-root combat replicate group."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Protocol

from .combat_experience import (
    BoundedCombatGroupExperience,
    CombatExperienceError,
    CombatExperienceLimits,
    CompletedCombatGroupExperience,
)
from .combat_outcomes import (
    CombatGroupOutcomeAccumulator,
    CombatOutcomeError,
    CombatTerminalStepBatch,
)
from .policy import BatchPolicyChoice


class CombatGroupPolicy(Protocol):
    def choose(self, decision_batch: Mapping[str, object]) -> BatchPolicyChoice: ...


class CombatGroupEnvironment(Protocol):
    root_id: str
    exact_combat_state_hash: str
    replicate_count: int
    terminal_count: int
    ready: bool

    def decision_batch(self, *, semantic: bool) -> Mapping[str, object]: ...

    def decision_progress(self) -> Sequence[object]: ...

    def choose(self, ordinals: list[int]) -> None: ...

    def step(self) -> Mapping[str, object]: ...


@dataclass(frozen=True)
class CombatGroupRunResult:
    experience: CompletedCombatGroupExperience
    model_rounds: int
    transitions: int


class CombatGroupDriver:
    """Run one exact combat group and return only a complete bounded experience."""

    def __init__(
        self,
        env: CombatGroupEnvironment,
        policy: CombatGroupPolicy,
        limits: CombatExperienceLimits,
    ) -> None:
        self.env = env
        self.policy = policy
        self.limits = limits

    def run(self) -> CombatGroupRunResult:
        if self.env.terminal_count != 0:
            raise CombatExperienceError("combat group driver requires a fresh group")
        collector = BoundedCombatGroupExperience(
            root_id=self.env.root_id,
            exact_combat_state_hash=self.env.exact_combat_state_hash,
            replicate_count=self.env.replicate_count,
            limits=self.limits,
        )
        outcomes = CombatGroupOutcomeAccumulator(
            root_id=self.env.root_id,
            exact_combat_state_hash=self.env.exact_combat_state_hash,
            replicate_count=self.env.replicate_count,
        )
        model_rounds = 0
        transitions = 0
        while outcomes.terminal_count < self.env.replicate_count:
            while not self.env.ready:
                if model_rounds >= self.limits.max_model_rounds:
                    raise CombatExperienceError("combat group exceeded model-round limit")
                decision_batch = self.env.decision_batch(semantic=True)
                prepared = collector.prepare(decision_batch)
                progress = self.env.decision_progress()
                choice = self.policy.choose(decision_batch)
                bound = collector.bind_choice(prepared, choice, progress)
                self.env.choose(list(bound.selected_ordinals))
                collector.commit(bound)
                model_rounds += 1
            if transitions >= self.limits.max_transitions:
                raise CombatExperienceError("combat group exceeded transition limit")
            step = self.env.step()
            try:
                terminal = CombatTerminalStepBatch.from_bridge_step(
                    step,
                    replicate_count=self.env.replicate_count,
                )
                outcomes.record(terminal)
            except CombatOutcomeError as error:
                raise CombatExperienceError(str(error)) from error
            transitions += 1
            if outcomes.terminal_count != self.env.terminal_count:
                raise CombatExperienceError(
                    "bridge and caller combat terminal counts diverged"
                )
        return CombatGroupRunResult(
            experience=collector.finish(outcomes.finish()),
            model_rounds=model_rounds,
            transitions=transitions,
        )
