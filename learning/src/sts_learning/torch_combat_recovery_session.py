"""One verified source win followed by bounded reverse-curriculum training."""

from __future__ import annotations

import operator
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

import torch

from .combat_driver import CombatGroupDriver
from .combat_experience import CompletedCombatGroupExperience
from .combat_outcomes import CombatTerminalOutcome
from .combat_potion_lane import CombatPotionLaneRootSource
from .combat_recovery import (
    CombatRecoveryPlan,
    CombatRecoveryRootSource,
    replay_winning_recovery_roots,
)
from .combat_root_artifacts import (
    load_combat_root_source,
    normalize_combat_root_artifact,
    read_combat_root_artifact,
)
from .policy import BehaviorManifestId
from .torch_behavior import TorchBehaviorPublication
from .torch_combat_batch_generation import (
    BoundedCombatWinBatchGenerationRunner,
    CombatWinBatchGenerationResult,
)
from .torch_combat_owners import create_combat_win_owner_graph
from .torch_combat_session import _positive_integer, _torch_seed
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinBatchSessionConfig,
)
from .torch_policy import RaggedCandidateScorer


class TorchCombatRecoverySessionError(RuntimeError):
    """A reverse-curriculum session lost an exact source or behavior boundary."""


@dataclass(frozen=True)
class CombatRecoveryDiscoveryResult:
    """Compact facts for the winning source group used to derive recovery roots."""

    source_artifact_root_count: int
    source_root_slot: int
    root_id: str
    exact_combat_state_hash: str
    behavior_manifest_id: BehaviorManifestId
    replicate_count: int
    wins: int
    losses: int
    model_rounds: int
    transitions: int
    teacher_replicate_index: int
    teacher_final_hp: int
    teacher_turns: int
    recovery_root_count: int


class CombatWinRecoverySession:
    """One live reverse curriculum with explicit behavior publication."""

    def __init__(
        self,
        runner: BoundedCombatWinBatchGenerationRunner,
        *,
        artifact_byte_count: int,
        plan: CombatRecoveryPlan,
        discovery: CombatRecoveryDiscoveryResult,
    ) -> None:
        if not isinstance(runner, BoundedCombatWinBatchGenerationRunner):
            raise TorchCombatRecoverySessionError(
                "recovery session requires a batch generation runner"
            )
        if not isinstance(plan, CombatRecoveryPlan):
            raise TorchCombatRecoverySessionError(
                "recovery session requires a typed replay plan"
            )
        if not isinstance(discovery, CombatRecoveryDiscoveryResult):
            raise TorchCombatRecoverySessionError(
                "recovery session requires typed discovery facts"
            )
        self.runner = runner
        self.artifact_byte_count = _positive_integer(
            artifact_byte_count,
            "artifact_byte_count",
        )
        self.plan = plan
        self.discovery = discovery

    @property
    def active_behavior_manifest_id(self) -> BehaviorManifestId:
        active = self.runner.controller.snapshot.active_manifest_id
        if active is None:
            raise TorchCombatRecoverySessionError(
                "recovery session has no active behavior"
            )
        return active

    def advance(self) -> CombatWinBatchGenerationResult:
        """Sample every derived root, then train and promote at most once."""

        return self.runner.advance()

    def publish_active_behavior(self) -> TorchBehaviorPublication:
        """Durably publish the active frozen scorer without changing policy."""

        return self.runner.controller.publish_active()


class CombatWinRecoverySessionFactory:
    """Build one reverse curriculum from a canonical single-root artifact."""

    def __init__(
        self,
        root: str | Path,
        bridge: CombatSessionBridge,
        config: CombatWinBatchSessionConfig,
        *,
        source_expected_roots: int = 1,
        source_root_slot: int = 0,
    ) -> None:
        if not isinstance(bridge, CombatSessionBridge):
            raise TorchCombatRecoverySessionError(
                "recovery session bridge must be typed"
            )
        if not isinstance(config, CombatWinBatchSessionConfig):
            raise TorchCombatRecoverySessionError(
                "recovery session requires a typed batch config"
            )
        self.source_expected_roots = _positive_integer(
            source_expected_roots,
            "source_expected_roots",
        )
        if isinstance(source_root_slot, bool):
            raise TorchCombatRecoverySessionError(
                "source_root_slot must be an integer, not bool"
            )
        try:
            normalized_source_slot = operator.index(source_root_slot)
        except TypeError as error:
            raise TorchCombatRecoverySessionError(
                "source_root_slot must be an integer"
            ) from error
        if not 0 <= normalized_source_slot < self.source_expected_roots:
            raise TorchCombatRecoverySessionError(
                "source_root_slot must identify a root in the source artifact"
            )
        self.source_root_slot = normalized_source_slot
        self.root = Path(root).resolve()
        if self.root.exists() and not self.root.is_dir():
            raise TorchCombatRecoverySessionError(
                "recovery session root is not a directory"
            )
        self.root.mkdir(parents=True, exist_ok=True)
        self.bridge = bridge
        self.config = config

    def new_from_artifact_file(
        self,
        artifact: str | Path,
        *,
        model_seed: int,
        source_behavior_seed: int,
        recovery_behavior_seeds: Sequence[int],
        initial_scorer: RaggedCandidateScorer | None = None,
        initial_scorer_actor_only: bool = False,
    ) -> CombatWinRecoverySession:
        payload = read_combat_root_artifact(
            artifact,
            max_bytes=self.config.limits.max_artifact_bytes,
        )
        return self.new_from_artifact_bytes(
            payload,
            model_seed=model_seed,
            source_behavior_seed=source_behavior_seed,
            recovery_behavior_seeds=recovery_behavior_seeds,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )

    def new_from_artifact_bytes(
        self,
        payload: bytes | bytearray | memoryview,
        *,
        model_seed: int,
        source_behavior_seed: int,
        recovery_behavior_seeds: Sequence[int],
        initial_scorer: RaggedCandidateScorer | None = None,
        initial_scorer_actor_only: bool = False,
    ) -> CombatWinRecoverySession:
        self._require_unused_root()
        artifact = normalize_combat_root_artifact(
            payload,
            max_bytes=self.config.limits.max_artifact_bytes,
        )
        normalized_model_seed = _torch_seed(model_seed, "model_seed")
        source_seed = _torch_seed(source_behavior_seed, "source_behavior_seed")
        recovery_seeds = tuple(recovery_behavior_seeds)
        if len(recovery_seeds) != self.config.expected_roots:
            raise TorchCombatRecoverySessionError(
                "recovery session requires one behavior seed per derived root"
            )
        normalized_recovery_seeds = tuple(
            _torch_seed(seed, f"recovery_behavior_seeds[{index}]")
            for index, seed in enumerate(recovery_seeds)
        )
        all_seeds = (source_seed, *normalized_recovery_seeds)
        if len(set(all_seeds)) != len(all_seeds):
            raise TorchCombatRecoverySessionError(
                "source and recovery behavior seeds must be distinct"
            )

        source = load_combat_root_source(
            self.bridge,
            artifact,
            expected_roots=self.source_expected_roots,
            max_bytes=self.config.limits.max_artifact_bytes,
        )
        source = CombatPotionLaneRootSource(
            source,
            self.config.potion_lane,
            self.config.potion_slots,
        )
        owners = create_combat_win_owner_graph(
            self.root,
            self.bridge,
            self.config.profile,
            self.config.limits,
            model_seed=normalized_model_seed,
            controller_seed=source_seed,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )
        source_policy = owners.controller.fork_active(
            torch.Generator(device="cpu").manual_seed(source_seed)
        )
        source_group = source.combat_group(
            self.source_root_slot,
            self.config.replicate_count,
        )
        source_run = CombatGroupDriver(
            source_group,
            source_policy,
            self.config.limits.experience,
        ).run()
        experience = source_run.experience
        active_manifest = owners.controller.snapshot.active_manifest_id
        if experience.behavior_manifest_id != active_manifest:
            raise TorchCombatRecoverySessionError(
                "source discovery changed the frozen behavior"
            )
        teacher = _highest_final_hp_winner(experience)
        plan = replay_winning_recovery_roots(
            source,
            slot_index=self.source_root_slot,
            experience=experience,
            teacher_replicate_index=teacher.replicate_index,
            max_roots=self.config.expected_roots,
        )
        if plan.root_count != self.config.expected_roots:
            raise TorchCombatRecoverySessionError(
                "winning replay was shorter than the required recovery root count"
            )

        recovery_source = CombatPotionLaneRootSource(
            CombatRecoveryRootSource(plan),
            self.config.potion_lane,
            self.config.potion_slots,
        )
        generators = tuple(
            torch.Generator(device="cpu").manual_seed(seed)
            for seed in normalized_recovery_seeds
        )
        runner = BoundedCombatWinBatchGenerationRunner(
            recovery_source,
            slot_indices=tuple(range(self.config.expected_roots)),
            replicate_count=self.config.replicate_count,
            behavior_generators=generators,
            max_roots=self.config.max_roots,
            limits=self.config.limits.experience,
            trainer=owners.trainer,
            controller=owners.controller,
            shadow_scorer=owners.shadow_scorer,
        )
        outcomes = experience.outcomes.outcomes
        wins = sum(outcome.won for outcome in outcomes)
        discovery = CombatRecoveryDiscoveryResult(
            source_artifact_root_count=self.source_expected_roots,
            source_root_slot=self.source_root_slot,
            root_id=experience.root_id,
            exact_combat_state_hash=experience.exact_combat_state_hash,
            behavior_manifest_id=experience.behavior_manifest_id,
            replicate_count=len(outcomes),
            wins=wins,
            losses=len(outcomes) - wins,
            model_rounds=source_run.model_rounds,
            transitions=source_run.transitions,
            teacher_replicate_index=teacher.replicate_index,
            teacher_final_hp=teacher.final_hp,
            teacher_turns=teacher.turns,
            recovery_root_count=plan.root_count,
        )
        return CombatWinRecoverySession(
            runner,
            artifact_byte_count=len(artifact),
            plan=plan,
            discovery=discovery,
        )

    def _require_unused_root(self) -> None:
        if any(self.root.iterdir()):
            raise TorchCombatRecoverySessionError(
                "new recovery session requires an unused experiment root"
            )


def _highest_final_hp_winner(
    experience: CompletedCombatGroupExperience,
) -> CombatTerminalOutcome:
    winners = tuple(outcome for outcome in experience.outcomes.outcomes if outcome.won)
    if not winners:
        raise TorchCombatRecoverySessionError(
            "source discovery produced no verified winning replicate"
        )
    return max(
        winners,
        key=lambda outcome: (outcome.final_hp, -outcome.replicate_index),
    )
