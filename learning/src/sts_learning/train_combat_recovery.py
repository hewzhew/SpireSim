"""Bounded reverse-curriculum combat training from one verified source win."""

from __future__ import annotations

import argparse
from dataclasses import replace
from pathlib import Path

from .combat_objective import CombatWinObjectiveConfig
from .combat_potion_lane import CombatPotionLane
from .combat_rollout import COMBAT_ROLLOUT_VALUE_HEAD_WIDTH
from .torch_combat_recovery_session import CombatWinRecoverySessionFactory
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinBatchSessionConfig,
    CombatWinSessionLimits,
    CombatWinSessionProfile,
)
from .torch_policy import RaggedScorerConfig
from .torch_provenance import AdamTrainingConfig
from .torch_session_config import CategoricalSessionBridge
from .train_combat import (
    CombatTrainingCommandConfig,
    CombatTrainingCommandError,
    _policy_update,
    _potion_slots_text,
    _recover_combat_warm_start,
    _run_combat_training_session,
)


def run_combat_recovery_training(
    config: CombatTrainingCommandConfig,
    *,
    source_expected_roots: int = 1,
    source_root_slot: int = 0,
    bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
) -> dict[str, object]:
    """Discover one win, train on its exact suffix roots, and publish behavior."""

    if not isinstance(config, CombatTrainingCommandConfig):
        raise CombatTrainingCommandError(
            "combat recovery config must be typed"
        )
    active_bridge = bridge if bridge is not None else CombatSessionBridge.installed()
    if not isinstance(active_bridge, CombatSessionBridge):
        raise CombatTrainingCommandError(
            "combat recovery bridge must be typed"
        )
    source_behavior_seed = config.behavior_seed_base
    recovery_behavior_seeds = tuple(
        config.behavior_seed_base + index + 1
        for index in range(config.root_count)
    )
    if recovery_behavior_seeds[-1] >= 1 << 63:
        raise CombatTrainingCommandError(
            "combat recovery behavior seeds must stay below 2^63"
        )
    profile = replace(
        CombatWinSessionProfile(),
        scorer=RaggedScorerConfig(
            value_head=config.policy_update.uses_value_baseline,
            value_head_width=(
                COMBAT_ROLLOUT_VALUE_HEAD_WIDTH
                if config.policy_update.uses_value_baseline
                else 1
            ),
        ),
        optimizer=config.optimizer,
        objective=CombatWinObjectiveConfig(
            groups_per_update=config.root_count,
            policy_update=config.policy_update,
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
        source_behavior_seed,
    )
    session = CombatWinRecoverySessionFactory(
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
        source_expected_roots=source_expected_roots,
        source_root_slot=source_root_slot,
    ).new_from_artifact_file(
        config.artifact,
        model_seed=config.model_seed,
        source_behavior_seed=source_behavior_seed,
        recovery_behavior_seeds=recovery_behavior_seeds,
        initial_scorer=(None if warm_start is None else warm_start.scorer),
        initial_scorer_actor_only=(
            warm_start_training_kind == "run"
            or (
                warm_start is not None
                and bool(warm_start.provenance_mismatches)
            )
        ),
    )

    discovery = session.discovery
    return _run_combat_training_session(
        config,
        session=session,
        profile=profile,
        behavior_seeds=recovery_behavior_seeds,
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
            "curriculum": "verified-win-terminal-nearest",
            "teacher_selection": "highest-final-hp-then-lowest-index",
            "source_replicate_count": config.replicate_count,
            "recovery_replicate_count": config.replicate_count,
            "source_behavior_seed": source_behavior_seed,
            "source_artifact_root_count": discovery.source_artifact_root_count,
            "source_root_slot": discovery.source_root_slot,
            "source_root_id": discovery.root_id,
            "source_exact_combat_state_hash": discovery.exact_combat_state_hash,
            "source_wins": discovery.wins,
            "source_losses": discovery.losses,
            "teacher_replicate_index": discovery.teacher_replicate_index,
            "teacher_final_hp": discovery.teacher_final_hp,
            "teacher_turns": discovery.teacher_turns,
        },
        completion_extra={
            "source_wins": discovery.wins,
            "source_losses": discovery.losses,
            "teacher_replicate_index": discovery.teacher_replicate_index,
            "teacher_final_hp": discovery.teacher_final_hp,
            "recovery_root_count": discovery.recovery_root_count,
        },
    )


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Train from terminal-nearest roots of one replay-verified behavior win."
        ),
    )
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--roots", type=int, required=True)
    parser.add_argument("--replicates", type=int, default=8)
    parser.add_argument("--updates", type=int, required=True)
    parser.add_argument("--model-seed", type=int, default=0)
    parser.add_argument("--behavior-seed-base", type=int, default=1_000)
    parser.add_argument("--learning-rate", type=float, default=1e-3)
    parser.add_argument("--source-expected-roots", type=int, default=1)
    parser.add_argument("--source-root-slot", type=int, default=0)
    parser.add_argument("--warm-start-behavior", type=Path)
    parser.add_argument(
        "--policy-update",
        choices=("reinforce", "ppo-clip", "ppo-clip-value"),
        default="reinforce",
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
    summary = run_combat_recovery_training(
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
        ),
        source_expected_roots=arguments.source_expected_roots,
        source_root_slot=arguments.source_root_slot,
    )
    print(
        "training_complete=true curriculum=verified-win-terminal-nearest "
        f"optimizer_steps={summary['optimizer_steps']} "
        f"policy_update={arguments.policy_update} "
        f"source_wins={summary['source_wins']} "
        f"source_losses={summary['source_losses']} "
        f"source_artifact_roots={arguments.source_expected_roots} "
        f"source_root_slot={arguments.source_root_slot} "
        f"teacher_replicate={summary['teacher_replicate_index']} "
        f"teacher_final_hp={summary['teacher_final_hp']} "
        f"recovery_roots={summary['recovery_root_count']} "
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
