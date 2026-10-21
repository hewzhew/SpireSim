"""Bounded fixed-behavior signal census over an opaque combat-root batch."""

from __future__ import annotations

import operator
import tempfile
from collections.abc import Sequence
from dataclasses import dataclass, replace
from pathlib import Path

from .combat_curriculum import (
    CombatFrontierPlan,
    CombatRootCompetenceEvidence,
    build_combat_frontier_plan,
)
from .combat_potion_lane import (
    CombatPotionLane,
    CombatPotionLaneError,
    CombatPotionLaneRootSource,
    normalize_combat_potion_slots,
)
from .combat_root_artifacts import (
    load_combat_root_source,
    normalize_combat_root_artifact,
    read_combat_root_artifact,
)
from .combat_signals import CombatSignalCensus, build_combat_signal_census
from .torch_combat_generation import CombatWinGenerationResult
from .torch_combat_session import (
    CombatWinSessionFactory,
    _torch_seed,
)
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinSessionConfig,
    TorchCombatSessionError,
)
from .torch_policy import RaggedCandidateScorer


@dataclass(frozen=True)
class CombatWinSignalCensusResult:
    """Compact per-root generations plus their distinct-root signal census."""

    generations: tuple[CombatWinGenerationResult, ...]
    census: CombatSignalCensus
    frontier: CombatFrontierPlan

    def __post_init__(self) -> None:
        generations = tuple(self.generations)
        if not generations or not all(
            isinstance(result, CombatWinGenerationResult)
            for result in generations
        ):
            raise TorchCombatSessionError(
                "combat signal census requires typed generation results"
            )
        if not isinstance(self.census, CombatSignalCensus):
            raise TorchCombatSessionError(
                "combat signal census requires a typed census"
            )
        if not isinstance(self.frontier, CombatFrontierPlan):
            raise TorchCombatSessionError(
                "combat signal census requires a typed frontier plan"
            )
        if self.frontier.objective_config.groups_per_update != 1:
            raise TorchCombatSessionError(
                "combat signal census frontier requires one-group objective provenance"
            )
        if len(generations) != self.census.group_count:
            raise TorchCombatSessionError(
                "combat signal census generation count is misaligned"
            )
        expected = build_combat_signal_census(
            tuple(result.signals for result in generations),
            max_groups=len(generations),
        )
        if self.census != expected:
            raise TorchCombatSessionError(
                "combat signal census does not match its generations"
            )
        expected_frontier = build_combat_frontier_plan(
            tuple(
                _competence_evidence(index, result)
                for index, result in enumerate(generations)
            ),
            self.frontier.objective_config,
            max_roots=len(generations),
        )
        if self.frontier != expected_frontier:
            raise TorchCombatSessionError(
                "combat signal census frontier does not match its generations"
            )
        object.__setattr__(self, "generations", generations)


class CombatWinSignalCensusRunner:
    """Run each artifact root once from equal initial model weights.

    Roots use caller-supplied independent behavior RNG seeds. Each generation
    owns an isolated trainer and any update is discarded after its compact
    result is captured; this is diagnostic signal coverage, not cross-root
    training or policy publication.
    """

    def __init__(
        self,
        bridge: CombatSessionBridge,
        config: CombatWinSessionConfig,
        *,
        max_roots: int,
        potion_lane: CombatPotionLane = CombatPotionLane.ALL,
        potion_slots: Sequence[int] = (),
    ) -> None:
        if not isinstance(bridge, CombatSessionBridge):
            raise TorchCombatSessionError("combat signal census bridge must be typed")
        if not isinstance(config, CombatWinSessionConfig):
            raise TorchCombatSessionError("combat signal census config must be typed")
        bound = _positive_integer(max_roots, "max_roots")
        if config.expected_roots > bound:
            raise TorchCombatSessionError(
                "combat signal census expected roots exceed max_roots"
            )
        if config.root_slot_index != 0:
            raise TorchCombatSessionError(
                "combat signal census config root_slot_index must be zero"
            )
        self.bridge = bridge
        self.config = config
        self.max_roots = bound
        if not isinstance(potion_lane, CombatPotionLane):
            raise TorchCombatSessionError(
                "combat signal census potion_lane must be typed"
            )
        try:
            normalized_potion_slots = normalize_combat_potion_slots(
                potion_lane,
                potion_slots,
            )
        except CombatPotionLaneError as error:
            raise TorchCombatSessionError(str(error)) from error
        self.potion_lane = potion_lane
        self.potion_slots = normalized_potion_slots

    def run_from_artifact_file(
        self,
        artifact: str | Path,
        *,
        model_seed: int,
        behavior_seeds: Sequence[int],
        initial_scorer: RaggedCandidateScorer | None = None,
        initial_scorer_actor_only: bool = False,
    ) -> CombatWinSignalCensusResult:
        payload = read_combat_root_artifact(
            artifact,
            max_bytes=self.config.limits.max_artifact_bytes,
        )
        return self.run_from_artifact_bytes(
            payload,
            model_seed=model_seed,
            behavior_seeds=behavior_seeds,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )

    def run_from_artifact_bytes(
        self,
        payload: bytes | bytearray | memoryview,
        *,
        model_seed: int,
        behavior_seeds: Sequence[int],
        initial_scorer: RaggedCandidateScorer | None = None,
        initial_scorer_actor_only: bool = False,
    ) -> CombatWinSignalCensusResult:
        artifact = normalize_combat_root_artifact(
            payload,
            max_bytes=self.config.limits.max_artifact_bytes,
        )
        model_seed = _torch_seed(model_seed, "model_seed")
        seeds = tuple(behavior_seeds)
        if len(seeds) != self.config.expected_roots:
            raise TorchCombatSessionError(
                "combat signal census requires one behavior seed per root"
            )
        seeds = tuple(
            _torch_seed(seed, f"behavior_seeds[{index}]")
            for index, seed in enumerate(seeds)
        )
        if len(set(seeds)) != len(seeds):
            raise TorchCombatSessionError(
                "combat signal census requires distinct behavior seeds"
            )
        source = load_combat_root_source(
            self.bridge,
            artifact,
            expected_roots=self.config.expected_roots,
            max_bytes=self.config.limits.max_artifact_bytes,
        )
        return self.run_from_combat_root_source(
            source,
            artifact_byte_count=len(artifact),
            model_seed=model_seed,
            behavior_seeds=seeds,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )

    def run_from_combat_root_source(
        self,
        source: object,
        *,
        artifact_byte_count: int,
        model_seed: int,
        behavior_seeds: Sequence[int],
        initial_scorer: RaggedCandidateScorer | None = None,
        initial_scorer_actor_only: bool = False,
    ) -> CombatWinSignalCensusResult:
        """Census one already-validated source without decoding it again."""

        if not callable(getattr(source, "combat_group", None)):
            raise TorchCombatSessionError(
                "combat signal census requires a combat-root source"
            )
        artifact_bytes = _positive_integer(
            artifact_byte_count,
            "artifact_byte_count",
        )
        model_seed = _torch_seed(model_seed, "model_seed")
        seeds = tuple(behavior_seeds)
        if len(seeds) != self.config.expected_roots:
            raise TorchCombatSessionError(
                "combat signal census requires one behavior seed per root"
            )
        seeds = tuple(
            _torch_seed(seed, f"behavior_seeds[{index}]")
            for index, seed in enumerate(seeds)
        )
        if len(set(seeds)) != len(seeds):
            raise TorchCombatSessionError(
                "combat signal census requires distinct behavior seeds"
            )
        source = CombatPotionLaneRootSource(
            source,
            self.potion_lane,
            self.potion_slots,
        )

        generations = []
        with tempfile.TemporaryDirectory(prefix="sts-combat-signal-census-") as root:
            root_path = Path(root)
            for slot_index, behavior_seed in enumerate(seeds):
                slot_config = replace(
                    self.config,
                    root_slot_index=slot_index,
                )
                factory = CombatWinSessionFactory(
                    root_path / f"root-{slot_index:04d}",
                    self.bridge,
                    slot_config,
                )
                session = factory._new_from_combat_root_source(
                    source,
                    artifact_byte_count=artifact_bytes,
                    model_seed=model_seed,
                    behavior_seed=behavior_seed,
                    initial_scorer=initial_scorer,
                    initial_scorer_actor_only=initial_scorer_actor_only,
                )
                generations.append(session.advance())

        generation_tuple = tuple(generations)
        census = build_combat_signal_census(
            tuple(result.signals for result in generation_tuple),
            max_groups=self.max_roots,
        )
        frontier = build_combat_frontier_plan(
            tuple(
                _competence_evidence(index, result)
                for index, result in enumerate(generation_tuple)
            ),
            self.config.profile.objective,
            max_roots=self.max_roots,
        )
        return CombatWinSignalCensusResult(generation_tuple, census, frontier)


def _competence_evidence(
    source_slot: int,
    result: CombatWinGenerationResult,
) -> CombatRootCompetenceEvidence:
    return CombatRootCompetenceEvidence(
        source_slot=source_slot,
        root_id=result.root_id,
        exact_combat_state_hash=result.exact_combat_state_hash,
        replicate_count=result.replicate_count,
        wins=result.wins,
        losses=result.losses,
        unresolved=result.unresolved,
        signals=result.signals,
    )


def _positive_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchCombatSessionError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchCombatSessionError(f"{name} must be an integer") from error
    if normalized <= 0:
        raise TorchCombatSessionError(f"{name} must be positive")
    return normalized
