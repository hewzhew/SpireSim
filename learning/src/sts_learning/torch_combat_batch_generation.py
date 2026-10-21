"""One frozen-behavior update over a bounded batch of distinct combat roots."""

from __future__ import annotations

import operator
from collections.abc import Sequence
from dataclasses import dataclass

import torch

from .combat_driver import CombatGroupDriver
from .combat_experience import CombatExperienceLimits
from .combat_objective import CombatAllLossAxis, CombatAllWinAxis
from .combat_outcomes import CombatTerminalKind
from .combat_signals import CombatGroupSignalSummary
from .policy import BehaviorManifestId
from .torch_behavior import (
    CategoricalTorchBehaviorController,
    TorchBehaviorBinding,
)
from .torch_combat_generation import CombatRootSource
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


class TorchCombatBatchGenerationError(RuntimeError):
    """A multi-root combat generation cannot progress without contamination."""


@dataclass(frozen=True)
class CombatWinRootGenerationResult:
    root_id: str
    exact_combat_state_hash: str
    replicate_count: int
    wins: int
    losses: int
    unresolved: int
    model_rounds: int
    transitions: int
    signals: CombatGroupSignalSummary

    def __post_init__(self) -> None:
        if not isinstance(self.signals, CombatGroupSignalSummary):
            raise TorchCombatBatchGenerationError(
                "combat root generation requires typed signals"
            )
        if (self.root_id, self.exact_combat_state_hash) != (
            self.signals.root_id,
            self.signals.exact_combat_state_hash,
        ):
            raise TorchCombatBatchGenerationError(
                "combat root generation signals changed exact identity"
            )
        if self.replicate_count != self.wins + self.losses + self.unresolved:
            raise TorchCombatBatchGenerationError(
                "combat root generation outcomes are misaligned"
            )
        if self.signals.replicate_count != self.replicate_count:
            raise TorchCombatBatchGenerationError(
                "combat root generation signal replicates are misaligned"
            )


@dataclass(frozen=True)
class CombatWinBatchGenerationResult:
    active_manifest_id_before: BehaviorManifestId
    active_training_step_before: int
    roots: tuple[CombatWinRootGenerationResult, ...]
    training: CombatWinTrainingResult
    promotion: TorchBehaviorBinding | None

    def __post_init__(self) -> None:
        roots = tuple(self.roots)
        if not roots or not all(
            isinstance(root, CombatWinRootGenerationResult) for root in roots
        ):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires typed root results"
            )
        if not isinstance(self.training, CombatWinTrainingResult):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires a typed training result"
            )
        if len(roots) != self.training.group_count:
            raise TorchCombatBatchGenerationError(
                "combat batch generation root count is misaligned"
            )
        if sum(root.replicate_count for root in roots) != self.training.replicate_count:
            raise TorchCombatBatchGenerationError(
                "combat batch generation replicate count is misaligned"
            )
        if sum(root.signals.decision_count for root in roots) != self.training.decision_count:
            raise TorchCombatBatchGenerationError(
                "combat batch generation decision count is misaligned"
            )
        win_signal_groups = sum(root.signals.win.has_signal for root in roots)
        eligible_terminal_hp_signal_groups = sum(
            not root.signals.win.has_signal
            and root.wins == root.replicate_count
            and root.signals.terminal_hp.has_signal
            for root in roots
        )
        terminal_hp_signal_groups = (
            eligible_terminal_hp_signal_groups
            if self.training.all_win_axis is CombatAllWinAxis.TERMINAL_HP
            else 0
        )
        eligible_enemy_hp_progress_signal_groups = sum(
            not root.signals.win.has_signal
            and root.losses == root.replicate_count
            and root.signals.enemy_hp_progress.has_signal
            for root in roots
        )
        enemy_hp_progress_signal_groups = (
            eligible_enemy_hp_progress_signal_groups
            if self.training.all_loss_axis
            is CombatAllLossAxis.ENEMY_HP_PROGRESS
            else 0
        )
        if win_signal_groups != self.training.win_signal_group_count:
            raise TorchCombatBatchGenerationError(
                "combat batch generation win signal count is misaligned"
            )
        if terminal_hp_signal_groups != self.training.terminal_hp_signal_group_count:
            raise TorchCombatBatchGenerationError(
                "combat batch generation terminal-HP signal count is misaligned"
            )
        if (
            enemy_hp_progress_signal_groups
            != self.training.enemy_hp_progress_signal_group_count
        ):
            raise TorchCombatBatchGenerationError(
                "combat batch generation enemy-HP signal count is misaligned"
            )
        if (
            win_signal_groups
            + terminal_hp_signal_groups
            + enemy_hp_progress_signal_groups
            != self.training.signal_group_count
        ):
            raise TorchCombatBatchGenerationError(
                "combat batch generation selected signal count is misaligned"
            )
        if self.promotion is not None and not isinstance(
            self.promotion,
            TorchBehaviorBinding,
        ):
            raise TorchCombatBatchGenerationError(
                "combat batch generation promotion must be typed"
            )
        object.__setattr__(self, "roots", roots)

    @property
    def promoted(self) -> bool:
        return self.promotion is not None


@dataclass(frozen=True)
class _PendingBatchPromotion:
    active_manifest_id_before: BehaviorManifestId
    active_training_step_before: int
    roots: tuple[CombatWinRootGenerationResult, ...]
    training: CombatWinTrainingResult


class BoundedCombatWinBatchGenerationRunner:
    """Collect distinct roots under one behavior, train once, and promote once."""

    def __init__(
        self,
        source: CombatRootSource,
        *,
        slot_indices: Sequence[int],
        replicate_count: int,
        behavior_generators: Sequence[torch.Generator],
        max_roots: int,
        limits: CombatExperienceLimits,
        trainer: SynchronousCombatWinTrainer,
        controller: CategoricalTorchBehaviorController,
        shadow_scorer: RaggedCandidateScorer,
    ) -> None:
        if not callable(getattr(source, "combat_group", None)):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires a combat-root source"
            )
        if not isinstance(limits, CombatExperienceLimits):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires experience limits"
            )
        if not isinstance(trainer, SynchronousCombatWinTrainer):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires a combat-win trainer"
            )
        if not isinstance(controller, CategoricalTorchBehaviorController):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires a behavior controller"
            )
        if not isinstance(shadow_scorer, RaggedCandidateScorer):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires a shadow scorer"
            )

        slots = tuple(
            _nonnegative_integer(slot, f"slot_indices[{index}]")
            for index, slot in enumerate(slot_indices)
        )
        if not slots:
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires at least one root"
            )
        if len(set(slots)) != len(slots):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires distinct root slots"
            )
        root_bound = _positive_integer(max_roots, "max_roots")
        if len(slots) > root_bound:
            raise TorchCombatBatchGenerationError(
                "combat batch generation roots exceed max_roots"
            )
        generators = tuple(behavior_generators)
        if len(generators) != len(slots):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires one behavior generator per root"
            )
        if not all(isinstance(generator, torch.Generator) for generator in generators):
            raise TorchCombatBatchGenerationError(
                "combat batch generation behavior generators must be typed"
            )
        if len({id(generator) for generator in generators}) != len(generators):
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires independent behavior generators"
            )

        replicates = _positive_integer(replicate_count, "replicate_count")
        if replicates < 2:
            raise TorchCombatBatchGenerationError(
                "combat batch generation requires at least two replicates"
            )
        if trainer.objective_config.groups_per_update != len(slots):
            raise TorchCombatBatchGenerationError(
                "combat batch generation roots must equal groups_per_update"
            )

        self.source = source
        self.slot_indices = slots
        self.replicate_count = replicates
        self.behavior_generators = generators
        self.max_roots = root_bound
        self.limits = limits
        self.trainer = trainer
        self.controller = controller
        self.shadow_scorer = shadow_scorer
        self._root_identities: dict[int, tuple[str, str]] = {}
        self._pending_promotion: _PendingBatchPromotion | None = None
        self._validate_wiring()

    @property
    def pending_promotion(self) -> bool:
        return self._pending_promotion is not None

    def advance(self) -> CombatWinBatchGenerationResult:
        """Collect one group per root before attempting one optimizer update."""

        self._validate_wiring()
        if self._pending_promotion is not None:
            return self._promote_pending()

        controller_before = self.controller.snapshot
        manifest_id = controller_before.active_manifest_id
        training_step = controller_before.active_training_step
        if manifest_id is None or training_step is None:
            raise TorchCombatBatchGenerationError(
                "combat batch generation controller has no active behavior"
            )
        policies = tuple(
            self.controller.fork_active(generator)
            for generator in self.behavior_generators
        )
        generator_states = tuple(
            generator.get_state().clone() for generator in self.behavior_generators
        )
        experiences = []
        roots = []
        observed_identities: dict[int, tuple[str, str]] = {}
        try:
            for slot_index, policy in zip(
                self.slot_indices,
                policies,
                strict=True,
            ):
                group = self.source.combat_group(slot_index, self.replicate_count)
                identity = (
                    getattr(group, "root_id", None),
                    getattr(group, "exact_combat_state_hash", None),
                )
                expected = self._root_identities.get(slot_index)
                if expected is not None and identity != expected:
                    raise TorchCombatBatchGenerationError(
                        "combat batch generation source changed an exact root"
                    )
                if identity in observed_identities.values():
                    raise TorchCombatBatchGenerationError(
                        "combat batch generation source repeated an exact root"
                    )
                if getattr(group, "replicate_count", None) != self.replicate_count:
                    raise TorchCombatBatchGenerationError(
                        "combat batch generation source changed the replicate count"
                    )
                observed_identities[slot_index] = identity

                run = CombatGroupDriver(group, policy, self.limits).run()
                experience = run.experience
                if experience.behavior_manifest_id != manifest_id:
                    raise TorchCombatBatchGenerationError(
                        "combat group behavior differs from the frozen batch"
                    )
                if identity != (
                    experience.root_id,
                    experience.exact_combat_state_hash,
                ):
                    raise TorchCombatBatchGenerationError(
                        "combat batch generation result changed its exact root"
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
                signals = experience.signal_summary()
                roots.append(
                    CombatWinRootGenerationResult(
                        root_id=experience.root_id,
                        exact_combat_state_hash=experience.exact_combat_state_hash,
                        replicate_count=len(outcomes),
                        wins=wins,
                        losses=losses,
                        unresolved=unresolved,
                        model_rounds=run.model_rounds,
                        transitions=run.transitions,
                        signals=signals,
                    )
                )
                experiences.append(experience)

            if self.controller.snapshot != controller_before:
                raise TorchCombatBatchGenerationError(
                    "active behavior changed while collecting the combat batch"
                )
            training = self.trainer.train(tuple(experiences))
        except Exception:
            for generator, state in zip(
                self.behavior_generators,
                generator_states,
                strict=True,
            ):
                generator.set_state(state)
            raise

        self._root_identities.update(observed_identities)
        pending = _PendingBatchPromotion(
            active_manifest_id_before=manifest_id,
            active_training_step_before=training_step,
            roots=tuple(roots),
            training=training,
        )
        if not training.updated:
            return _batch_result(pending, promotion=None)
        if (
            training.optimizer_steps_applied <= 0
            or training.optimizer_steps_after
            != training_step + training.optimizer_steps_applied
        ):
            raise TorchCombatBatchGenerationError(
                "combat batch trainer optimizer-step accounting is inconsistent"
            )
        self._pending_promotion = pending
        return self._promote_pending()

    def _promote_pending(self) -> CombatWinBatchGenerationResult:
        pending = self._pending_promotion
        if pending is None:
            raise TorchCombatBatchGenerationError(
                "combat batch generation has no pending promotion"
            )
        controller = self.controller.snapshot
        if (
            controller.active_manifest_id != pending.active_manifest_id_before
            or controller.active_training_step != pending.active_training_step_before
        ):
            raise TorchCombatBatchGenerationError(
                "active behavior changed during pending batch promotion"
            )
        if self.trainer.snapshot.optimizer_steps != pending.training.optimizer_steps_after:
            raise TorchCombatBatchGenerationError(
                "combat batch trainer changed during pending promotion"
            )
        promotion = self.controller.promote_live(
            self.shadow_scorer,
            training_step=pending.training.optimizer_steps_after,
        )
        result = _batch_result(pending, promotion=promotion)
        self._pending_promotion = None
        return result

    def _validate_wiring(self) -> None:
        if self.trainer.scorer is not self.shadow_scorer:
            raise TorchCombatBatchGenerationError(
                "combat batch trainer does not score the shadow model"
            )
        if self.trainer.registry is not self.controller.publisher.registry:
            raise TorchCombatBatchGenerationError(
                "combat batch trainer and controller do not share one registry"
            )
        controller = self.controller.snapshot
        manifest_id = controller.active_manifest_id
        training_step = controller.active_training_step
        if manifest_id is None or training_step is None:
            raise TorchCombatBatchGenerationError(
                "combat batch generation controller has no active behavior"
            )
        expected_optimizer_steps = training_step + int(
            self._pending_promotion is not None
        )
        if self.trainer.snapshot.optimizer_steps != expected_optimizer_steps:
            raise TorchCombatBatchGenerationError(
                "combat batch trainer and active behavior are misaligned"
            )
        manifest = self.trainer.registry.resolve(manifest_id)
        if manifest.trainer_implementation != combat_win_trainer_implementation(
            self.trainer.objective_config
        ):
            raise TorchCombatBatchGenerationError(
                "active behavior conflicts with the combat batch trainer"
            )
        try:
            require_exact_optimizer_parameters(
                self.trainer.optimizer,
                self.shadow_scorer,
            )
        except TorchOptimizerWiringError as error:
            raise TorchCombatBatchGenerationError(
                f"combat batch generation {error}"
            ) from error


def _batch_result(
    pending: _PendingBatchPromotion,
    *,
    promotion: TorchBehaviorBinding | None,
) -> CombatWinBatchGenerationResult:
    return CombatWinBatchGenerationResult(
        active_manifest_id_before=pending.active_manifest_id_before,
        active_training_step_before=pending.active_training_step_before,
        roots=pending.roots,
        training=pending.training,
        promotion=promotion,
    )


def _positive_integer(value: object, name: str) -> int:
    normalized = _nonnegative_integer(value, name)
    if normalized == 0:
        raise TorchCombatBatchGenerationError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchCombatBatchGenerationError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchCombatBatchGenerationError(f"{name} must be an integer") from error
    if normalized < 0:
        raise TorchCombatBatchGenerationError(f"{name} must be non-negative")
    return normalized
