"""Shared scorer, optimizer, registry, and live-controller construction."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import torch

from .manifest_catalog import BoundedBehaviorManifestCatalog
from .manifests import BehaviorManifestRegistry
from .torch_behavior import (
    CategoricalTorchBehaviorController,
    TorchBehaviorPublisher,
)
from .torch_checkpoints import BoundedTorchCheckpointStore
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinSessionLimits,
    CombatWinSessionProfile,
    TorchCombatSessionError,
)
from .torch_combat_training import SynchronousCombatWinTrainer
from .torch_policy import (
    RaggedCandidateScorer,
    TorchPolicyError,
    load_scorer_warm_start,
)
from .torch_provenance import combat_win_training_manifest_template


@dataclass(frozen=True)
class CombatWinOwnerGraph:
    shadow_scorer: RaggedCandidateScorer
    controller: CategoricalTorchBehaviorController
    trainer: SynchronousCombatWinTrainer


def create_combat_win_owner_graph(
    root: Path,
    bridge: CombatSessionBridge,
    profile: CombatWinSessionProfile,
    limits: CombatWinSessionLimits,
    *,
    model_seed: int,
    controller_seed: int,
    initial_scorer: RaggedCandidateScorer | None = None,
    initial_scorer_actor_only: bool = False,
) -> CombatWinOwnerGraph:
    """Create one exact mutable shadow and one independent frozen behavior."""

    if initial_scorer is not None and not isinstance(
        initial_scorer,
        RaggedCandidateScorer,
    ):
        raise TorchCombatSessionError(
            "combat owner initial_scorer must be a RaggedCandidateScorer"
        )
    if type(initial_scorer_actor_only) is not bool:
        raise TorchCombatSessionError("initial_scorer_actor_only must be bool")

    def scorer_factory() -> RaggedCandidateScorer:
        return RaggedCandidateScorer.from_bridge_schema(
            bridge.semantic_schema,
            profile.scorer,
        ).to(profile.device_type)

    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(model_seed)
        shadow = scorer_factory()
    if initial_scorer is not None:
        try:
            load_scorer_warm_start(
                shadow,
                initial_scorer,
                actor_only=initial_scorer_actor_only,
            )
        except TorchPolicyError as error:
            raise TorchCombatSessionError(
                "combat owner initial scorer is incompatible with the maintained profile"
            ) from error
    checkpoint_store = BoundedTorchCheckpointStore(
        root / "behavior-checkpoints",
        limits.checkpoint_store,
    )
    catalog = BoundedBehaviorManifestCatalog(
        root / "behavior-manifests",
        limits.manifest_catalog,
    )
    registry = BehaviorManifestRegistry(capacity=limits.owner_capacity)
    controller = CategoricalTorchBehaviorController(
        TorchBehaviorPublisher(
            checkpoint_store,
            catalog,
            registry,
            combat_win_training_manifest_template(
                bridge.semantic_schema,
                profile.scorer,
                profile.behavior,
                profile.optimizer,
                profile.objective,
                device_type=profile.device_type,
            ),
        ),
        scorer_factory,
        profile.behavior,
        torch.Generator(device="cpu").manual_seed(controller_seed),
    )
    trainer = SynchronousCombatWinTrainer(
        shadow,
        profile.optimizer.create(shadow.parameters()),
        registry,
        limits.concat,
        profile.behavior,
        profile.objective,
    )
    controller.promote_live(shadow, training_step=0)
    return CombatWinOwnerGraph(
        shadow_scorer=shadow,
        controller=controller,
        trainer=trainer,
    )
