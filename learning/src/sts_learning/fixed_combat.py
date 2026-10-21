"""Explicit oracle combat continuation for strategic-policy experiments.

This adapter is not an OnlineBatchDriver environment: unresolved searches have
no transition and must be counted as censored, never fabricated as defeats.
It does not own recovery, model inputs, trajectories, or checkpoint schemas.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class FixedCombatConfig:
    backend: str = "portfolio"
    generation_work: int = 4096
    wall_ms: int = 500
    quantum_wall_ms: int = 50
    atomic_nodes: int | None = 4096
    atomic_wall_ms: int = 1000
    allow_potions: bool = True
    fallback_action_limit: int | None = None

    def __post_init__(self) -> None:
        if self.backend not in ("portfolio", "whole_combat"):
            raise ValueError("fixed combat backend must be portfolio or whole_combat")
        for value in (self.generation_work, self.wall_ms, self.quantum_wall_ms,
                      self.atomic_wall_ms):
            if type(value) is not int or value <= 0:
                raise ValueError("fixed combat limits must be positive integers")
        if self.atomic_nodes is not None and (
            type(self.atomic_nodes) is not int or self.atomic_nodes <= 0
        ):
            raise ValueError("atomic_nodes must be positive or None")
        if type(self.allow_potions) is not bool:
            raise ValueError("allow_potions must be boolean")
        if self.fallback_action_limit is not None and (
            type(self.fallback_action_limit) is not int or self.fallback_action_limit <= 0
        ):
            raise ValueError("fallback_action_limit must be positive or None")


@dataclass(frozen=True)
class FixedCombatStep:
    transitions: dict[str, Any] | None
    evidence: dict[str, Any]
    unresolved_slot: int | None

    @property
    def advanced(self) -> bool:
        return self.transitions is not None


class FixedCombatController:
    """A fixed executor configuration applied to explicitly chosen actions.

    Call environment.choose normally (including symbolic choices), then step.
    On unresolved the environment still has those same pending choices. A
    separately configured controller can retry them with another stated budget.
    Search plans live in evidence; only strategic ordinals enter transitions.
    """

    def __init__(self, environment: Any, config: FixedCombatConfig) -> None:
        self.environment = environment
        options = fixed_combat_options(environment, config)
        self._options_json = json.dumps(options, sort_keys=True, separators=(",", ":"))

    @property
    def options(self) -> dict[str, Any]:
        """Copy of full native options for experiment identity and receipts."""
        return json.loads(self._options_json)

    def step(self) -> FixedCombatStep:
        result = self.environment.step_with_fixed_combat(self._options_json)
        evidence = json.loads(result.pop("fixed_combat_evidence_json"))
        status = result.pop("status")
        if status == "unresolved":
            return FixedCombatStep(None, evidence, result["unresolved_slot_index"])
        if status != "advanced":
            raise RuntimeError(f"unknown fixed combat status: {status}")
        return FixedCombatStep(result, evidence, None)


def fixed_combat_options(environment: Any, config: FixedCombatConfig) -> dict[str, Any]:
    """Resolve explicit Python limits against the installed native defaults."""
    if not isinstance(config, FixedCombatConfig):
        raise TypeError("fixed combat configuration must be typed")
    options = json.loads(environment.fixed_combat_options_json())
    options["backend"] = config.backend
    options["fallback_action_limit"] = config.fallback_action_limit
    search = options["search"]
    search["max_generation_work"] = config.generation_work
    search["wall_ms"] = config.wall_ms
    search["quantum_wall_ms"] = config.quantum_wall_ms
    search["potion_policy"] = "all" if config.allow_potions else "never"
    search["atomic_challenger"] = (
        None if config.atomic_nodes is None else {
            "max_nodes": config.atomic_nodes, "wall_ms": config.atomic_wall_ms,
        }
    )
    return options
