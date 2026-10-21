"""Bounded command-line composition for real multi-root combat training."""

from __future__ import annotations

import argparse
import hashlib
import json
import time
from collections.abc import Mapping
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Protocol, TextIO

from .combat_objective import (
    CombatAllLossAxis,
    CombatAllWinAxis,
    CombatPolicyUpdateConfig,
    CombatPolicyUpdateRule,
    CombatWinObjectiveConfig,
)
from .combat_potion_lane import (
    CombatPotionLane,
    CombatPotionLaneError,
    normalize_combat_potion_slots,
)
from .combat_root_artifacts import (
    load_combat_root_source,
    read_combat_root_artifact,
)
from .combat_root_audit import CombatRootAuditError, read_combat_root_audits
from .combat_rollout import COMBAT_ROLLOUT_VALUE_HEAD_WIDTH
from .manifests import ManifestArtifactId
from .policy import BehaviorManifestId
from .torch_combat_batch_generation import (
    BoundedCombatWinBatchGenerationRunner,
    CombatWinBatchGenerationResult,
    CombatWinRootGenerationResult,
)
from .torch_combat_batch_session import CombatWinBatchSessionFactory
from .torch_combat_census import (
    CombatWinSignalCensusResult,
    CombatWinSignalCensusRunner,
)
from .torch_behavior import TorchBehaviorPublication
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinBatchSessionConfig,
    CombatWinSessionConfig,
    CombatWinSessionLimits,
    CombatWinSessionProfile,
)
from .torch_policy import RaggedCandidateScorer, RaggedScorerConfig
from .torch_provenance import AdamTrainingConfig
from .torch_session_config import CategoricalSessionBridge


LEGACY_COMBAT_TRAINING_SCHEMA = "sts-learning-combat-training-v4"
PREVIOUS_COMBAT_TRAINING_SCHEMA = "sts-learning-combat-training-v5"
COMBAT_TRAINING_SCHEMA = "sts-learning-combat-training-v6"


class CombatTrainingCommandError(RuntimeError):
    """A bounded combat training command is malformed."""


class _CombatTrainingSession(Protocol):
    artifact_byte_count: int
    runner: BoundedCombatWinBatchGenerationRunner

    def advance(self) -> CombatWinBatchGenerationResult: ...

    def publish_active_behavior(self) -> TorchBehaviorPublication: ...


@dataclass(frozen=True)
class _CombatWarmStart:
    """Verified scorer weights imported without claiming trainer resume."""

    manifest_id: BehaviorManifestId
    checkpoint_id: ManifestArtifactId
    training_step: int
    scorer: RaggedCandidateScorer
    provenance_mismatches: tuple[str, ...] = ()


@dataclass(frozen=True)
class CombatTrainingCommandConfig:
    artifact: Path
    output: Path
    root_count: int
    replicate_count: int
    updates: int
    model_seed: int
    behavior_seed_base: int
    potion_lane: CombatPotionLane = CombatPotionLane.ALL
    potion_slots: tuple[int, ...] = ()
    warm_start_behavior: Path | None = None
    policy_update: CombatPolicyUpdateConfig = CombatPolicyUpdateConfig()
    all_loss_axis: CombatAllLossAxis = CombatAllLossAxis.NONE
    optimizer: AdamTrainingConfig = AdamTrainingConfig()

    def __post_init__(self) -> None:
        artifact = Path(self.artifact).resolve()
        output = Path(self.output).resolve()
        if not artifact.is_file():
            raise CombatTrainingCommandError("combat artifact is not a file")
        if output.exists() and (
            not output.is_dir() or any(output.iterdir())
        ):
            raise CombatTrainingCommandError(
                "combat training output must be absent or empty"
            )
        warm_start_behavior = self.warm_start_behavior
        if warm_start_behavior is not None:
            warm_start_behavior = Path(warm_start_behavior).resolve()
            if not warm_start_behavior.is_dir():
                raise CombatTrainingCommandError(
                    "combat warm-start behavior is not a directory"
                )
            if output == warm_start_behavior or warm_start_behavior in output.parents:
                raise CombatTrainingCommandError(
                    "combat training output must stay outside the warm-start behavior"
                )
        root_count = _positive(self.root_count, "root_count")
        replicate_count = _positive(self.replicate_count, "replicate_count")
        updates = _nonnegative(self.updates, "updates")
        model_seed = _seed(self.model_seed, "model_seed")
        behavior_seed_base = _seed(
            self.behavior_seed_base,
            "behavior_seed_base",
        )
        if not isinstance(self.potion_lane, CombatPotionLane):
            raise CombatTrainingCommandError(
                "combat training potion_lane must be typed"
            )
        try:
            potion_slots = normalize_combat_potion_slots(
                self.potion_lane,
                self.potion_slots,
            )
        except CombatPotionLaneError as error:
            raise CombatTrainingCommandError(str(error)) from error
        if not isinstance(self.policy_update, CombatPolicyUpdateConfig):
            raise CombatTrainingCommandError(
                "combat training policy_update must be typed"
            )
        if not isinstance(self.all_loss_axis, CombatAllLossAxis):
            raise CombatTrainingCommandError(
                "combat training all_loss_axis must be typed"
            )
        if not isinstance(self.optimizer, AdamTrainingConfig):
            raise CombatTrainingCommandError(
                "combat training optimizer must be typed"
            )
        if root_count < 2 and updates > 0:
            raise CombatTrainingCommandError(
                "combat training updates require at least two roots"
            )
        if replicate_count < 2:
            raise CombatTrainingCommandError(
                "combat training requires at least two replicates"
            )
        if behavior_seed_base + root_count > 1 << 63:
            raise CombatTrainingCommandError("behavior seeds must stay below 2^63")
        object.__setattr__(self, "artifact", artifact)
        object.__setattr__(self, "output", output)
        object.__setattr__(self, "root_count", root_count)
        object.__setattr__(self, "replicate_count", replicate_count)
        object.__setattr__(self, "updates", updates)
        object.__setattr__(self, "model_seed", model_seed)
        object.__setattr__(self, "behavior_seed_base", behavior_seed_base)
        object.__setattr__(self, "potion_slots", potion_slots)
        object.__setattr__(self, "warm_start_behavior", warm_start_behavior)

    @property
    def behavior_seeds(self) -> tuple[int, ...]:
        return tuple(
            self.behavior_seed_base + index
            for index in range(self.root_count)
        )


def run_combat_training(
    config: CombatTrainingCommandConfig,
    *,
    bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
) -> dict[str, object]:
    """Run bounded online updates and publish the final active behavior."""

    if not isinstance(config, CombatTrainingCommandConfig):
        raise CombatTrainingCommandError("combat training config must be typed")
    active_bridge = bridge if bridge is not None else CombatSessionBridge.installed()
    if not isinstance(active_bridge, CombatSessionBridge):
        raise CombatTrainingCommandError("combat training bridge must be typed")
    scorer_config = RaggedScorerConfig(
        value_head=config.policy_update.uses_value_baseline,
        value_head_width=(
            COMBAT_ROLLOUT_VALUE_HEAD_WIDTH
            if config.policy_update.uses_value_baseline
            else 1
        ),
    )
    census_profile = replace(
        CombatWinSessionProfile(),
        scorer=scorer_config,
        optimizer=config.optimizer,
        objective=CombatWinObjectiveConfig(
            groups_per_update=1,
            policy_update=config.policy_update,
            all_loss_axis=config.all_loss_axis,
        ),
    )
    limits = replace(
        CombatWinSessionLimits(),
        owner_capacity=max(16, config.updates + 1),
    )
    warm_start, warm_start_training_kind = _recover_combat_warm_start(
        config.warm_start_behavior,
        active_bridge,
        run_bridge,
        config.behavior_seed_base,
    )
    artifact = read_combat_root_artifact(
        config.artifact,
        max_bytes=limits.max_artifact_bytes,
    )
    audit_source = load_combat_root_source(
        active_bridge,
        artifact,
        expected_roots=config.root_count,
        max_bytes=limits.max_artifact_bytes,
    )
    try:
        root_audits = read_combat_root_audits(
            audit_source,
            tuple(range(config.root_count)),
        )
    except CombatRootAuditError as error:
        raise CombatTrainingCommandError(str(error)) from error
    initial_scorer = None if warm_start is None else warm_start.scorer
    initial_scorer_actor_only = (
        warm_start_training_kind == "run"
        or (
            warm_start is not None
            and bool(warm_start.provenance_mismatches)
        )
    )
    training_slots = tuple(range(config.root_count))
    training_behavior_seeds = config.behavior_seeds
    frontier_record: dict[str, object] = {
        "frontier_admission": "not-run-zero-updates",
        "frontier_training_slots": training_slots,
        "frontier_rescue_slots": (),
        "frontier_solved_slots": (),
        "frontier_roots": (),
    }
    if config.updates:
        census = CombatWinSignalCensusRunner(
            active_bridge,
            CombatWinSessionConfig(
                expected_roots=config.root_count,
                replicate_count=config.replicate_count,
                profile=census_profile,
                limits=limits,
            ),
            max_roots=config.root_count,
            potion_lane=config.potion_lane,
            potion_slots=config.potion_slots,
        ).run_from_combat_root_source(
            audit_source,
            artifact_byte_count=len(artifact),
            model_seed=config.model_seed,
            behavior_seeds=config.behavior_seeds,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )
        training_slots = census.frontier.training_slots
        if not training_slots:
            raise CombatTrainingCommandError(
                "combat fixed-behavior census found no trainable frontier; "
                f"rescue_slots={census.frontier.rescue_slots} "
                f"solved_slots={census.frontier.solved_slots}"
            )
        training_behavior_seeds = tuple(
            config.behavior_seeds[source_slot]
            for source_slot in training_slots
        )
        profile = replace(
            census_profile,
            objective=census.frontier.training_objective_config(),
        )
        frontier_record = _frontier_configuration(census)
        session = CombatWinBatchSessionFactory(
            config.output,
            active_bridge,
            CombatWinBatchSessionConfig(
                expected_roots=len(training_slots),
                max_roots=config.root_count,
                replicate_count=config.replicate_count,
                profile=profile,
                limits=limits,
                potion_lane=config.potion_lane,
                potion_slots=config.potion_slots,
            ),
        ).new_from_frontier_root_source(
            audit_source,
            census.frontier,
            artifact_byte_count=len(artifact),
            model_seed=config.model_seed,
            behavior_seeds=training_behavior_seeds,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )
    else:
        profile = replace(
            census_profile,
            objective=replace(
                census_profile.objective,
                groups_per_update=config.root_count,
            ),
        )
        session = CombatWinBatchSessionFactory(
            config.output,
            active_bridge,
            CombatWinBatchSessionConfig(
                expected_roots=config.root_count,
                max_roots=config.root_count,
                replicate_count=config.replicate_count,
                profile=profile,
                limits=limits,
                potion_lane=config.potion_lane,
                potion_slots=config.potion_slots,
            ),
        ).new_from_artifact_bytes(
            artifact,
            model_seed=config.model_seed,
            behavior_seeds=training_behavior_seeds,
            initial_scorer=initial_scorer,
            initial_scorer_actor_only=initial_scorer_actor_only,
        )

    return _run_combat_training_session(
        config,
        session=session,
        profile=profile,
        behavior_seeds=training_behavior_seeds,
        warm_start_manifest_id=(
            None if warm_start is None else warm_start.manifest_id.digest.hex()
        ),
        warm_start_checkpoint_id=(
            None if warm_start is None else warm_start.checkpoint_id.digest.hex()
        ),
        warm_start_training_step=(
            None if warm_start is None else warm_start.training_step
        ),
        warm_start_training_kind=warm_start_training_kind,
        warm_start_provenance_mismatches=(
            () if warm_start is None else warm_start.provenance_mismatches
        ),
        configuration_extra={
            "root_audits": tuple(audit.as_mapping() for audit in root_audits),
            **frontier_record,
        },
        source_root_slots=training_slots,
    )


def _recover_combat_warm_start(
    behavior: Path | None,
    combat_bridge: CombatSessionBridge,
    run_bridge: CategoricalSessionBridge | None,
    behavior_seed: int,
) -> tuple[_CombatWarmStart | None, str | None]:
    if behavior is None:
        return None, None
    from .published_run_behavior import (
        is_run_training_publication,
        recover_published_run_behavior,
    )

    if is_run_training_publication(behavior):
        active_run_bridge = (
            run_bridge
            if run_bridge is not None
            else CategoricalSessionBridge.installed()
        )
        if not isinstance(active_run_bridge, CategoricalSessionBridge):
            raise CombatTrainingCommandError(
                "run warm-start recovery bridge must be typed"
            )
        if active_run_bridge.semantic_schema != combat_bridge.semantic_schema:
            raise CombatTrainingCommandError(
                "run warm-start and combat training semantic schemas differ"
            )
        recovered = recover_published_run_behavior(
            behavior,
            active_run_bridge,
            (behavior_seed,),
        )
        return (
            _CombatWarmStart(
                manifest_id=recovered.manifest_id,
                checkpoint_id=recovered.checkpoint_id,
                training_step=recovered.training_step,
                scorer=recovered.policies[0].frozen_scorer,
            ),
            "run",
        )
    from .published_combat_behavior import recover_compatible_combat_scorer

    recovered = recover_compatible_combat_scorer(
        behavior,
        combat_bridge,
        CombatWinSessionLimits(),
    )
    return (
        _CombatWarmStart(
            manifest_id=recovered.source_manifest_id,
            checkpoint_id=recovered.checkpoint_id,
            training_step=recovered.training_step,
            scorer=recovered.scorer,
            provenance_mismatches=recovered.provenance_mismatches,
        ),
        "combat",
    )


def _frontier_configuration(
    census: CombatWinSignalCensusResult,
) -> dict[str, object]:
    frontier = census.frontier
    return {
        "frontier_admission": "fixed-behavior-census-v1",
        "frontier_training_slots": frontier.training_slots,
        "frontier_survival_slots": frontier.survival_frontier_slots,
        "frontier_resource_slots": frontier.resource_frontier_slots,
        "frontier_rescue_slots": frontier.rescue_slots,
        "frontier_solved_slots": frontier.solved_slots,
        "frontier_roots": tuple(
            {
                "slot_index": root.source_slot,
                "root_id": root.root_id,
                "exact_combat_state_hash": root.exact_combat_state_hash,
                "band": root.band.name,
                "wins": root.wins,
                "losses": root.losses,
                "unresolved": root.unresolved,
            }
            for root in frontier.roots
        ),
    }


def _run_combat_training_session(
    config: CombatTrainingCommandConfig,
    *,
    session: _CombatTrainingSession,
    profile: CombatWinSessionProfile,
    behavior_seeds: tuple[int, ...],
    warm_start_manifest_id: str | None,
    warm_start_checkpoint_id: str | None,
    warm_start_training_step: int | None,
    warm_start_training_kind: str | None = None,
    warm_start_provenance_mismatches: tuple[str, ...] = (),
    configuration_extra: Mapping[str, object] | None = None,
    completion_extra: Mapping[str, object] | None = None,
    source_root_slots: tuple[int, ...] | None = None,
) -> dict[str, object]:
    """Journal and publish one already-constructed combat training session."""

    normalized_source_slots = (
        tuple(range(config.root_count))
        if source_root_slots is None
        else tuple(source_root_slots)
    )
    if not normalized_source_slots:
        raise CombatTrainingCommandError(
            "combat training requires at least one selected root"
        )
    if len(normalized_source_slots) != len(behavior_seeds):
        raise CombatTrainingCommandError(
            "combat training selected roots and behavior seeds are misaligned"
        )
    if len(set(normalized_source_slots)) != len(normalized_source_slots):
        raise CombatTrainingCommandError(
            "combat training selected roots must be distinct"
        )
    if any(
        slot < 0 or slot >= config.root_count
        for slot in normalized_source_slots
    ):
        raise CombatTrainingCommandError(
            "combat training selected root is outside the source artifact"
        )
    configuration = {
        "schema": COMBAT_TRAINING_SCHEMA,
        "kind": "configuration",
        "artifact": str(config.artifact),
        "artifact_sha256": _sha256(config.artifact),
        "artifact_bytes": session.artifact_byte_count,
        "root_count": config.root_count,
        "training_root_count": len(normalized_source_slots),
        "training_root_slots": normalized_source_slots,
        "replicate_count": config.replicate_count,
        "updates": config.updates,
        "model_seed": config.model_seed,
        "behavior_seeds": behavior_seeds,
        "all_win_axis": profile.objective.all_win_axis.name,
        "all_loss_axis": profile.objective.all_loss_axis.name,
        "policy_update_rule": profile.objective.policy_update.rule.name,
        "policy_update_epochs": profile.objective.policy_update.epochs,
        "policy_clip_coefficient": (
            profile.objective.policy_update.clip_coefficient
        ),
        "policy_entropy_coefficient": (
            profile.objective.policy_update.entropy_coefficient
        ),
        "policy_max_grad_norm": profile.objective.policy_update.max_grad_norm,
        "policy_target_kl": profile.objective.policy_update.target_kl,
        "policy_value_loss_coefficient": (
            profile.objective.policy_update.value_loss_coefficient
        ),
        "optimizer_learning_rate": profile.optimizer.learning_rate,
        "potion_lane": config.potion_lane.value,
        "potion_slots": config.potion_slots,
        "initialization": (
            "random"
            if config.warm_start_behavior is None
            else "published-behavior"
        ),
        "warm_start_behavior": (
            None
            if config.warm_start_behavior is None
            else str(config.warm_start_behavior)
        ),
        "warm_start_manifest_id": warm_start_manifest_id,
        "warm_start_checkpoint_id": warm_start_checkpoint_id,
        "warm_start_training_step": warm_start_training_step,
        "warm_start_training_kind": warm_start_training_kind,
        "warm_start_provenance_mismatches": (
            warm_start_provenance_mismatches
        ),
    }
    _extend_record(configuration, configuration_extra)

    total_wins = 0
    total_losses = 0
    total_unresolved = 0
    started = time.perf_counter()
    with (config.output / "training.jsonl").open(
        "x",
        encoding="utf-8",
        newline="\n",
    ) as journal:
        _write(journal, configuration)
        for generation in range(config.updates):
            generation_started = time.perf_counter()
            result = session.advance()
            elapsed = time.perf_counter() - generation_started
            wins = sum(root.wins for root in result.roots)
            losses = sum(root.losses for root in result.roots)
            total_wins += wins
            total_losses += losses
            unresolved = sum(root.unresolved for root in result.roots)
            total_unresolved += unresolved
            _write(
                journal,
                _generation(
                    generation,
                    result,
                    elapsed,
                    normalized_source_slots,
                ),
            )
            root_wins = ",".join(str(root.wins) for root in result.roots)
            root_objectives = ",".join(
                _selected_objective(
                    root,
                    result.training.all_win_axis,
                    result.training.all_loss_axis,
                )
                for root in result.roots
            )
            print(
                f"generation={generation} "
                f"step_before={result.active_training_step_before} "
                f"wins={wins} losses={losses} unresolved={unresolved} "
                f"signal_groups={result.training.signal_group_count} "
                f"status={result.training.status.name} "
                f"promoted={str(result.promoted).lower()} "
                f"decisions={result.training.decision_count} "
                f"optimizer_steps={result.training.optimizer_steps_applied} "
                f"loss={result.training.loss:.8g} "
                f"approx_kl={result.training.approximate_kl:.6g} "
                f"clip_fraction={result.training.clip_fraction:.6g} "
                f"entropy={result.training.entropy:.6g} "
                f"value_loss={result.training.value_loss:.6g} "
                f"root_wins={root_wins} root_objectives={root_objectives} "
                f"seconds={elapsed:.3f}",
                flush=True,
            )

        publication = session.publish_active_behavior()
        snapshot = session.runner.trainer.snapshot
        summary: dict[str, object] = {
            "schema": COMBAT_TRAINING_SCHEMA,
            "kind": "completed",
            "updates": config.updates,
            "optimizer_steps": snapshot.optimizer_steps,
            "deliveries": snapshot.deliveries,
            "no_update_deliveries": snapshot.no_update_deliveries,
            "total_wins": total_wins,
            "total_losses": total_losses,
            "total_unresolved": total_unresolved,
            "elapsed_seconds": time.perf_counter() - started,
            "final_manifest_id": publication.manifest_id.digest.hex(),
            "final_checkpoint_id": publication.checkpoint_id.digest.hex(),
        }
        _extend_record(summary, completion_extra)
        _write(journal, summary)
    return summary


def _extend_record(
    record: dict[str, object],
    extra: Mapping[str, object] | None,
) -> None:
    if extra is None:
        return
    overlap = record.keys() & extra.keys()
    if overlap:
        fields = ", ".join(sorted(overlap))
        raise CombatTrainingCommandError(
            f"combat training journal extension repeats fields: {fields}"
        )
    record.update(extra)


def _generation(
    index: int,
    result: CombatWinBatchGenerationResult,
    elapsed: float,
    source_root_slots: tuple[int, ...],
) -> dict[str, object]:
    if len(source_root_slots) != len(result.roots):
        raise CombatTrainingCommandError(
            "combat generation roots changed the selected frontier width"
        )
    return {
        "schema": COMBAT_TRAINING_SCHEMA,
        "kind": "generation",
        "generation": index,
        "active_training_step_before": result.active_training_step_before,
        "active_manifest_id_before": result.active_manifest_id_before.digest.hex(),
        "active_manifest_id_after": (
            result.promotion.manifest_id.digest.hex()
            if result.promotion is not None
            else result.active_manifest_id_before.digest.hex()
        ),
        "promoted": result.promoted,
        "status": result.training.status.name,
        "loss": result.training.loss,
        "signal_group_count": result.training.signal_group_count,
        "win_signal_group_count": result.training.win_signal_group_count,
        "terminal_hp_signal_group_count": (
            result.training.terminal_hp_signal_group_count
        ),
        "enemy_hp_progress_signal_group_count": (
            result.training.enemy_hp_progress_signal_group_count
        ),
        "decision_count": result.training.decision_count,
        "optimizer_steps_after": result.training.optimizer_steps_after,
        "optimizer_steps_applied": result.training.optimizer_steps_applied,
        "approximate_kl": result.training.approximate_kl,
        "clip_fraction": result.training.clip_fraction,
        "entropy": result.training.entropy,
        "value_loss": result.training.value_loss,
        "roots": tuple(
            _root(
                training_slot_index,
                source_root_slot,
                root,
                result.training.all_win_axis,
                result.training.all_loss_axis,
            )
            for training_slot_index, (source_root_slot, root) in enumerate(
                zip(source_root_slots, result.roots, strict=True)
            )
        ),
        "elapsed_seconds": elapsed,
    }


def _root(
    training_slot_index: int,
    source_root_slot: int,
    root: CombatWinRootGenerationResult,
    all_win_axis: CombatAllWinAxis,
    all_loss_axis: CombatAllLossAxis,
) -> dict[str, object]:
    signals = root.signals
    return {
        "slot_index": source_root_slot,
        "training_slot_index": training_slot_index,
        "root_id": root.root_id,
        "exact_combat_state_hash": root.exact_combat_state_hash,
        "wins": root.wins,
        "losses": root.losses,
        "unresolved": root.unresolved,
        "model_rounds": root.model_rounds,
        "transitions": root.transitions,
        "decision_count": signals.decision_count,
        "win_signal_replicates": signals.win.replicate_count,
        "terminal_hp_signal_replicates": signals.terminal_hp.replicate_count,
        "enemy_hp_progress_signal_replicates": (
            signals.enemy_hp_progress.replicate_count
        ),
        "potion_signal_replicates": signals.potion_retention.replicate_count,
        "selected_objective": _selected_objective(
            root,
            all_win_axis,
            all_loss_axis,
        ),
    }


def _selected_objective(
    root: CombatWinRootGenerationResult,
    all_win_axis: CombatAllWinAxis,
    all_loss_axis: CombatAllLossAxis,
) -> str:
    if root.signals.win.has_signal:
        return "win"
    if (
        all_win_axis is CombatAllWinAxis.TERMINAL_HP
        and root.wins == root.replicate_count
        and root.signals.terminal_hp.has_signal
    ):
        return "hp"
    if (
        all_loss_axis is CombatAllLossAxis.ENEMY_HP_PROGRESS
        and root.losses == root.replicate_count
        and root.signals.enemy_hp_progress.has_signal
    ):
        return "enemy-hp-progress"
    return "none"


def _write(journal: TextIO, value: dict[str, object]) -> None:
    journal.write(json.dumps(value, separators=(",", ":"), sort_keys=True))
    journal.write("\n")
    journal.flush()


def _sha256(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def _positive(value: int, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise CombatTrainingCommandError(f"{name} must be a positive integer")
    return value


def _nonnegative(value: int, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise CombatTrainingCommandError(f"{name} must be a nonnegative integer")
    return value


def _seed(value: int, name: str) -> int:
    if (
        isinstance(value, bool)
        or not isinstance(value, int)
        or not 0 <= value < 1 << 63
    ):
        raise CombatTrainingCommandError(f"{name} must be an integer in [0, 2^63)")
    return value


def _potion_slots_text(arguments: argparse.Namespace) -> str:
    lane = CombatPotionLane(arguments.potion_lane)
    if lane is CombatPotionLane.ALL:
        return "all"
    if lane is CombatPotionLane.NEVER:
        return "none"
    return "+".join(str(slot) for slot in arguments.potion_slot)


def _policy_update(name: str) -> CombatPolicyUpdateConfig:
    if name == "reinforce":
        return CombatPolicyUpdateConfig(
            rule=CombatPolicyUpdateRule.REINFORCE,
        )
    if name == "ppo-clip":
        return CombatPolicyUpdateConfig.ppo_clip()
    if name == "ppo-clip-value":
        return CombatPolicyUpdateConfig.ppo_clip_value()
    raise CombatTrainingCommandError("unsupported combat policy update")


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Train one shared behavior from an opaque combat-root batch.",
    )
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--roots", type=int, required=True)
    parser.add_argument("--replicates", type=int, default=8)
    parser.add_argument("--updates", type=int, required=True)
    parser.add_argument("--model-seed", type=int, default=0)
    parser.add_argument("--behavior-seed-base", type=int, default=1_000)
    parser.add_argument("--learning-rate", type=float, default=1e-3)
    parser.add_argument("--warm-start-behavior", type=Path)
    parser.add_argument(
        "--policy-update",
        choices=("reinforce", "ppo-clip", "ppo-clip-value"),
        default="reinforce",
    )
    parser.add_argument(
        "--all-loss-axis",
        choices=("none", "enemy-hp-progress"),
        default="none",
    )
    parser.add_argument(
        "--potion-lane",
        choices=tuple(lane.value for lane in CombatPotionLane),
        default=CombatPotionLane.ALL.value,
    )
    parser.add_argument("--potion-slot", action="append", type=int, default=[])
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    summary = run_combat_training(
        CombatTrainingCommandConfig(
            artifact=arguments.artifact,
            output=arguments.output,
            root_count=arguments.roots,
            replicate_count=arguments.replicates,
            updates=arguments.updates,
            model_seed=arguments.model_seed,
            behavior_seed_base=arguments.behavior_seed_base,
            potion_lane=CombatPotionLane(arguments.potion_lane),
            potion_slots=tuple(arguments.potion_slot),
            warm_start_behavior=arguments.warm_start_behavior,
            policy_update=_policy_update(arguments.policy_update),
            optimizer=AdamTrainingConfig(
                learning_rate=arguments.learning_rate,
            ),
            all_loss_axis=(
                CombatAllLossAxis.ENEMY_HP_PROGRESS
                if arguments.all_loss_axis == "enemy-hp-progress"
                else CombatAllLossAxis.NONE
            ),
        )
    )
    print(
        "training_complete=true "
        f"optimizer_steps={summary['optimizer_steps']} "
        f"policy_update={arguments.policy_update} "
        f"all_loss_axis={arguments.all_loss_axis} "
        f"potion_lane={arguments.potion_lane} "
        f"potion_slots={_potion_slots_text(arguments)} "
        f"wins={summary['total_wins']} losses={summary['total_losses']} "
        f"seconds={summary['elapsed_seconds']:.3f} "
        f"output={arguments.output.resolve()}",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
