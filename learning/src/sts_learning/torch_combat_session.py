"""Compact artifact-to-runner boundary for same-root combat win learning."""

from __future__ import annotations

import operator
from pathlib import Path

from .combat_root_artifacts import (
    load_combat_root_source,
    normalize_combat_root_artifact,
    read_combat_root_artifact,
)
from .policy import BehaviorManifestId
from .torch_behavior import (
    TorchBehaviorPublication,
)
from .torch_combat_generation import (
    BoundedCombatWinGenerationRunner,
    CombatWinGenerationResult,
)
from .torch_combat_owners import create_combat_win_owner_graph
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinSessionConfig,
    TorchCombatSessionError,
)
from .torch_policy import RaggedCandidateScorer


class CombatWinSession:
    """One live fixed-root runner with explicit behavior publication."""

    def __init__(
        self,
        runner: BoundedCombatWinGenerationRunner,
        *,
        artifact_byte_count: int,
    ) -> None:
        if not isinstance(runner, BoundedCombatWinGenerationRunner):
            raise TorchCombatSessionError(
                "combat session requires a combat generation runner"
            )
        self.runner = runner
        self.artifact_byte_count = _positive_integer(
            artifact_byte_count,
            "artifact_byte_count",
        )

    @property
    def active_behavior_manifest_id(self) -> BehaviorManifestId:
        active = self.runner.controller.snapshot.active_manifest_id
        if active is None:
            raise TorchCombatSessionError("combat session has no active behavior")
        return active

    def advance(self) -> CombatWinGenerationResult:
        """Run at most one complete group and its immediate live promotion."""

        return self.runner.advance()

    def publish_active_behavior(self) -> TorchBehaviorPublication:
        """Durably publish the active frozen scorer without changing policy."""

        return self.runner.controller.publish_active()


class CombatWinSessionFactory:
    """Own repetitive combat-root, scorer, optimizer, and controller wiring."""

    def __init__(
        self,
        root: str | Path,
        bridge: CombatSessionBridge,
        config: CombatWinSessionConfig,
    ) -> None:
        if not isinstance(bridge, CombatSessionBridge):
            raise TorchCombatSessionError("combat session bridge must be typed")
        if not isinstance(config, CombatWinSessionConfig):
            raise TorchCombatSessionError("combat session config must be typed")
        self.root = Path(root).resolve()
        if self.root.exists() and not self.root.is_dir():
            raise TorchCombatSessionError(
                "combat session root is not a directory"
            )
        self.root.mkdir(parents=True, exist_ok=True)
        self.bridge = bridge
        self.config = config

    def new_from_artifact_file(
        self,
        artifact: str | Path,
        *,
        model_seed: int,
        behavior_seed: int,
        initial_scorer: RaggedCandidateScorer | None = None,
        initial_scorer_actor_only: bool = False,
    ) -> CombatWinSession:
        """Read one bounded opaque artifact and create generation zero."""

        payload = read_combat_root_artifact(
            artifact,
            max_bytes=self.config.limits.max_artifact_bytes,
        )
        return self.new_from_artifact_bytes(
            payload,
            model_seed=model_seed,
            behavior_seed=behavior_seed,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )

    def new_from_artifact_bytes(
        self,
        payload: bytes | bytearray | memoryview,
        *,
        model_seed: int,
        behavior_seed: int,
        initial_scorer: RaggedCandidateScorer | None = None,
        initial_scorer_actor_only: bool = False,
    ) -> CombatWinSession:
        """Import exact roots and create one fully wired in-process session."""

        self._require_unused_root()
        artifact = normalize_combat_root_artifact(
            payload,
            max_bytes=self.config.limits.max_artifact_bytes,
        )
        model_seed = _torch_seed(model_seed, "model_seed")
        behavior_seed = _torch_seed(behavior_seed, "behavior_seed")
        source = load_combat_root_source(
            self.bridge,
            artifact,
            expected_roots=self.config.expected_roots,
            max_bytes=self.config.limits.max_artifact_bytes,
        )
        return self._new_from_combat_root_source(
            source,
            artifact_byte_count=len(artifact),
            model_seed=model_seed,
            behavior_seed=behavior_seed,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )

    def _new_from_combat_root_source(
        self,
        source: object,
        *,
        artifact_byte_count: int,
        model_seed: int,
        behavior_seed: int,
        initial_scorer: RaggedCandidateScorer | None = None,
        initial_scorer_actor_only: bool = False,
    ) -> CombatWinSession:
        if not callable(getattr(source, "combat_group", None)):
            raise TorchCombatSessionError(
                "combat-root artifact loader returned an invalid source"
            )

        owners = create_combat_win_owner_graph(
            self.root,
            self.bridge,
            self.config.profile,
            self.config.limits,
            model_seed=model_seed,
            controller_seed=behavior_seed,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )
        runner = BoundedCombatWinGenerationRunner(
            source,
            slot_index=self.config.root_slot_index,
            replicate_count=self.config.replicate_count,
            limits=self.config.limits.experience,
            trainer=owners.trainer,
            controller=owners.controller,
            shadow_scorer=owners.shadow_scorer,
        )
        return CombatWinSession(
            runner,
            artifact_byte_count=artifact_byte_count,
        )

    def _require_unused_root(self) -> None:
        if any(self.root.iterdir()):
            raise TorchCombatSessionError(
                "new combat session requires an unused experiment root"
            )

def _torch_seed(value: object, name: str) -> int:
    normalized = _nonnegative_integer(value, name)
    if normalized >= 1 << 63:
        raise TorchCombatSessionError(f"{name} must be below 2^63")
    return normalized


def _positive_integer(value: object, name: str) -> int:
    normalized = _nonnegative_integer(value, name)
    if normalized == 0:
        raise TorchCombatSessionError(f"{name} must be positive")
    return normalized


def _nonnegative_integer(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise TorchCombatSessionError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise TorchCombatSessionError(f"{name} must be an integer") from error
    if normalized < 0:
        raise TorchCombatSessionError(f"{name} must be non-negative")
    return normalized
