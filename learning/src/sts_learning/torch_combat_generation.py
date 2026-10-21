"""One-root bounded generations for same-root combat win learning."""

from __future__ import annotations

import operator
from dataclasses import dataclass
from typing import Protocol

from .combat_signals import CombatGroupSignalSummary
from .combat_driver import CombatGroupDriver
from .combat_experience import CombatExperienceLimits
from .combat_outcomes import CombatTerminalKind
from .policy import BehaviorManifestId
from .torch_behavior import (
    CategoricalTorchBehaviorController,
    TorchBehaviorBinding,
)
from .torch_combat_training import (
    CombatWinTrainingResult,
    SynchronousCombatWinTrainer,
)
from .torch_optimizer_wiring import (
    TorchOptimizerWiringError,
    require_exact_optimizer_parameters,
)
from .torch_policy import RaggedCandidateScorer
from .torch_provenance import combat_win_trainer_implementation


class TorchCombatGenerationError(RuntimeError):
    """A combat generation is miswired or cannot safely progress."""


class CombatRootSource(Protocol):
    def combat_group(self, slot_index: int, replicate_count: int): ...


@dataclass(frozen=True)
class CombatWinGenerationResult:
    root_id: str
    exact_combat_state_hash: str
    active_manifest_id_before: BehaviorManifestId
    active_training_step_before: int
    replicate_count: int
    wins: int
    losses: int
    unresolved: int
    model_rounds: int
    transitions: int
    signals: CombatGroupSignalSummary
    training: CombatWinTrainingResult
    promotion: TorchBehaviorBinding | None

    @property
    def promoted(self) -> bool:
        return self.promotion is not None


@dataclass(frozen=True)
class _PendingCombatPromotion:
    root_id: str
    exact_combat_state_hash: str
    active_manifest_id_before: BehaviorManifestId
    active_training_step_before: int
    replicate_count: int
    wins: int
    losses: int
    unresolved: int
    model_rounds: int
    transitions: int
    signals: CombatGroupSignalSummary
    training: CombatWinTrainingResult


class BoundedCombatWinGenerationRunner:
    """Run one same-root group and promote only after one real optimizer step."""

    def __init__(
        self,
        source: CombatRootSource,
        *,
        slot_index: int,
        replicate_count: int,
        limits: CombatExperienceLimits,
        trainer: SynchronousCombatWinTrainer,
        controller: CategoricalTorchBehaviorController,
        shadow_scorer: RaggedCandidateScorer,
    ) -> None:
        if not callable(getattr(source, "combat_group", None)):
            raise TorchCombatGenerationError(
                "combat generation requires a combat-root source"
            )
        if not isinstance(limits, CombatExperienceLimits):
            raise TorchCombatGenerationError(
                "combat generation requires experience limits"
            )
        if not isinstance(trainer, SynchronousCombatWinTrainer):
            raise TorchCombatGenerationError(
                "combat generation requires a combat-win trainer"
            )
        if not isinstance(controller, CategoricalTorchBehaviorController):
            raise TorchCombatGenerationError(
                "combat generation requires a behavior controller"
            )
        if not isinstance(shadow_scorer, RaggedCandidateScorer):
            raise TorchCombatGenerationError(
                "combat generation requires a shadow scorer"
            )
        self.source = source
        self.slot_index = _nonnegative_integer(slot_index, "slot_index")
        self.replicate_count = _positive_integer(
            replicate_count,
            "replicate_count",
        )
        if self.replicate_count < 2:
            raise TorchCombatGenerationError(
                "combat generation requires at least two replicates"
            )
        if trainer.objective_config.groups_per_update != 1:
            raise TorchCombatGenerationError(
                "one-root generation requires groups_per_update equal to one"
            )
        self.limits = limits
        self.trainer = trainer
        self.controller = controller
        self.shadow_scorer = shadow_scorer
        self._root_identity: tuple[str, str] | None = None
        self._pending_promotion: _PendingCombatPromotion | None = None
        self._validate_wiring()

    @property
    def pending_promotion(self) -> bool:
        return self._pending_promotion is not None

    def advance(self) -> CombatWinGenerationResult:
        """Run at most one group; a failed promotion is retried before new play."""

        self._validate_wiring()
        if self._pending_promotion is not None:
            return self._promote_pending()

        controller_before = self.controller.snapshot
        manifest_id = controller_before.active_manifest_id
        training_step = controller_before.active_training_step
        if manifest_id is None or training_step is None:
            raise TorchCombatGenerationError(
                "combat generation controller has no active behavior"
            )
        group = self.source.combat_group(self.slot_index, self.replicate_count)
        identity = (
            getattr(group, "root_id", None),
            getattr(group, "exact_combat_state_hash", None),
        )
        if self._root_identity is None:
            self._root_identity = identity
        elif identity != self._root_identity:
            raise TorchCombatGenerationError(
                "combat generation source changed its exact root"
            )
        if getattr(group, "replicate_count", None) != self.replicate_count:
            raise TorchCombatGenerationError(
                "combat generation source returned a different replicate count"
            )
        run = CombatGroupDriver(group, self.controller, self.limits).run()
        experience = run.experience
        if experience.behavior_manifest_id != manifest_id:
            raise TorchCombatGenerationError(
                "combat group behavior differs from the active generation"
            )
        if identity != (experience.root_id, experience.exact_combat_state_hash):
            raise TorchCombatGenerationError(
                "combat generation result changed its exact root"
            )

        outcomes = experience.outcomes.outcomes
        wins = sum(
            outcome.terminal_kind is CombatTerminalKind.WIN
            for outcome in outcomes
        )
        losses = sum(
            outcome.terminal_kind is CombatTerminalKind.LOSS
            for outcome in outcomes
        )
        unresolved = sum(
            outcome.terminal_kind is CombatTerminalKind.UNRESOLVED
            for outcome in outcomes
        )
        training = self.trainer.train((experience,))
        pending = _PendingCombatPromotion(
            root_id=experience.root_id,
            exact_combat_state_hash=experience.exact_combat_state_hash,
            active_manifest_id_before=manifest_id,
            active_training_step_before=training_step,
            replicate_count=len(outcomes),
            wins=wins,
            losses=losses,
            unresolved=unresolved,
            model_rounds=run.model_rounds,
            transitions=run.transitions,
            signals=experience.signal_summary(),
            training=training,
        )
        if not training.updated:
            return _generation_result(pending, promotion=None)

        if (
            training.optimizer_steps_applied <= 0
            or training.optimizer_steps_after
            != training_step + training.optimizer_steps_applied
        ):
            raise TorchCombatGenerationError(
                "combat trainer optimizer-step accounting is inconsistent"
            )
        self._pending_promotion = pending
        return self._promote_pending()

    def _promote_pending(self) -> CombatWinGenerationResult:
        pending = self._pending_promotion
        if pending is None:
            raise TorchCombatGenerationError(
                "combat generation has no pending promotion"
            )
        controller = self.controller.snapshot
        if (
            controller.active_manifest_id != pending.active_manifest_id_before
            or controller.active_training_step != pending.active_training_step_before
        ):
            raise TorchCombatGenerationError(
                "combat generation active behavior changed during pending promotion"
            )
        if (
            self.trainer.snapshot.optimizer_steps
            != pending.training.optimizer_steps_after
        ):
            raise TorchCombatGenerationError(
                "combat generation trainer changed during pending promotion"
            )
        promotion = self.controller.promote_live(
            self.shadow_scorer,
            training_step=pending.training.optimizer_steps_after,
        )
        result = _generation_result(pending, promotion=promotion)
        self._pending_promotion = None
        return result

    def _validate_wiring(self) -> None:
        if self.trainer.scorer is not self.shadow_scorer:
            raise TorchCombatGenerationError(
                "combat trainer does not score the generation shadow model"
            )
        if self.trainer.registry is not self.controller.publisher.registry:
            raise TorchCombatGenerationError(
                "combat trainer and controller do not share one manifest registry"
            )
        controller = self.controller.snapshot
        manifest_id = controller.active_manifest_id
        training_step = controller.active_training_step
        if manifest_id is None or training_step is None:
            raise TorchCombatGenerationError(
                "combat generation controller has no active behavior"
            )
        expected_optimizer_steps = training_step + int(
            self._pending_promotion is not None
        )
        if self.trainer.snapshot.optimizer_steps != expected_optimizer_steps:
            raise TorchCombatGenerationError(
                "combat trainer and active behavior generation are misaligned"
            )
        manifest = self.trainer.registry.resolve(manifest_id)
        if manifest.trainer_implementation != combat_win_trainer_implementation(
            self.trainer.objective_config
        ):
            raise TorchCombatGenerationError(
                "active behavior conflicts with the combat trainer implementation"
            )
        try:
            require_exact_optimizer_parameters(
                self.trainer.optimizer,
                self.shadow_scorer,
            )
        except TorchOptimizerWiringError as error:
            raise TorchCombatGenerationError(f"combat generation {error}") from error


def _generation_result(
    pending: _PendingCombatPromotion,
    *,
    promotion: TorchBehaviorBinding | None,
) -> CombatWinGenerationResult:
    return CombatWinGenerationResult(
        root_id=pending.root_id,
        exact_combat_state_hash=pending.exact_combat_state_hash,
        active_manifest_id_before=pending.active_manifest_id_before,
        active_training_step_before=pending.active_training_step_before,
        replicate_count=pending.replicate_count,
        wins=pending.wins,
        losses=pending.losses,
        unresolved=pending.unresolved,
        model_rounds=pending.model_rounds,
        transitions=pending.transitions,
        signals=pending.signals,
        training=pending.training,
        promotion=promotion,
    )


def _positive_integer(value: object, name: str) -> int:
    normalized = _nonnegative_integer(value, name)
    if normalized == 0:
        raise TorchCombatGenerationError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchCombatGenerationError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchCombatGenerationError(f"{name} must be an integer") from error
    if normalized < 0:
        raise TorchCombatGenerationError(f"{name} must be non-negative")
    return normalized
