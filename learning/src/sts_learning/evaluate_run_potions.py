"""Paired whole-run evaluation with and without combat potion candidates."""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path

from .combat_potion_lane import CombatPotionLane
from .evaluate_run import (
    RunEvaluationCommandConfig,
    RunEvaluationCommandError,
    RunPotionLane,
    run_run_evaluation,
)
from .torch_combat_session_config import CombatSessionBridge
from .torch_session_config import CategoricalSessionBridge


RUN_POTION_COMPARISON_SCHEMA = "sts-learning-run-potion-comparison-v2"


@dataclass(frozen=True)
class RunPotionComparisonCommandConfig:
    behavior: Path
    output: Path
    terminal_attempts: int
    max_batch_steps: int
    behavior_seed: int
    ascension_level: int
    held_out_seed_start: int = 0

    def __post_init__(self) -> None:
        output = Path(self.output).resolve()
        if output.exists() and (
            not output.is_dir() or any(output.iterdir())
        ):
            raise RunEvaluationCommandError(
                "run potion comparison output must be absent or empty"
            )
        probe = RunEvaluationCommandConfig(
            behavior=self.behavior,
            output=output / "all-potions",
            slot_count=1,
            terminal_attempts=self.terminal_attempts,
            max_batch_steps=self.max_batch_steps,
            behavior_seed=self.behavior_seed,
            ascension_level=self.ascension_level,
            held_out_seed_start=self.held_out_seed_start,
            potion_lane=RunPotionLane.ALL,
        )
        object.__setattr__(self, "behavior", probe.behavior)
        object.__setattr__(self, "output", output)
        object.__setattr__(self, "terminal_attempts", probe.terminal_attempts)
        object.__setattr__(self, "max_batch_steps", probe.max_batch_steps)
        object.__setattr__(self, "behavior_seed", probe.behavior_seed)
        object.__setattr__(self, "ascension_level", probe.ascension_level)
        object.__setattr__(
            self,
            "held_out_seed_start",
            probe.held_out_seed_start,
        )


def run_run_potion_comparison(
    config: RunPotionComparisonCommandConfig,
    *,
    combat_bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
) -> dict[str, object]:
    """Run two one-slot lanes so terminal episode seeds remain exactly paired."""

    if not isinstance(config, RunPotionComparisonCommandConfig):
        raise RunEvaluationCommandError(
            "run potion comparison config must be typed"
        )
    active_combat_bridge = combat_bridge or CombatSessionBridge.installed()
    active_run_bridge = run_bridge or CategoricalSessionBridge.installed()
    lanes = tuple(
        _run_lane(
            config,
            active_combat_bridge,
            active_run_bridge,
            lane,
            directory,
        )
        for lane, directory in (
            (CombatPotionLane.ALL, "all-potions"),
            (CombatPotionLane.NEVER, "no-potions"),
        )
    )
    _validate_identity(lanes)
    summary = _summary(config, lanes)
    config.output.mkdir(parents=True, exist_ok=True)
    with (config.output / "potion-comparison.json").open(
        "x",
        encoding="utf-8",
        newline="\n",
    ) as destination:
        json.dump(summary, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")
    comparison = summary["comparison"]
    print(
        "run_potion_comparison_complete=true "
        f"ascension={config.ascension_level} "
        f"all_floor_sum={lanes[0][1]['terminal_floor_sum']} "
        f"never_floor_sum={lanes[1][1]['terminal_floor_sum']} "
        f"paired={comparison['paired_terminal_seeds']} "
        f"never_deeper={comparison['never_deeper']} "
        f"same={comparison['same_floor']} "
        f"never_shallower={comparison['never_shallower']} "
        f"output={config.output}",
        flush=True,
    )
    for seed in summary["seeds"]:
        print(
            "run_potion_comparison_seed="
            f"{seed['seed']} all_floor={_optional(seed['all_floor'])} "
            f"never_floor={_optional(seed['never_floor'])} "
            f"delta={_optional(seed['floor_delta_never_minus_all'])} "
            f"all_lost={_identities(seed['all_potion_identity_losses'])} "
            f"never_lost={_identities(seed['never_potion_identity_losses'])}",
            flush=True,
        )
    return summary


def _run_lane(
    config: RunPotionComparisonCommandConfig,
    combat_bridge: CombatSessionBridge,
    run_bridge: CategoricalSessionBridge,
    lane: CombatPotionLane,
    directory: str,
) -> tuple[CombatPotionLane, dict[str, object], Path]:
    output = config.output / directory
    result = run_run_evaluation(
        RunEvaluationCommandConfig(
            behavior=config.behavior,
            output=output,
            slot_count=1,
            terminal_attempts=config.terminal_attempts,
            max_batch_steps=config.max_batch_steps,
            behavior_seed=config.behavior_seed,
            ascension_level=config.ascension_level,
            held_out_seed_start=config.held_out_seed_start,
            potion_lane=RunPotionLane(lane.value),
        ),
        combat_bridge=combat_bridge,
        run_bridge=run_bridge,
    )
    return lane, result, output


def _validate_identity(
    lanes: tuple[tuple[CombatPotionLane, dict[str, object], Path], ...],
) -> None:
    first = lanes[0][1]
    identity = (
        first["behavior_manifest_id"],
        first["behavior_checkpoint_id"],
        first["behavior_seed"],
        first["ascension_level"],
        first["held_out_seed_start"],
        first["slot_count"],
        first["terminal_attempt_target"],
        first["max_batch_steps"],
    )
    for _, result, _ in lanes[1:]:
        if (
            result["behavior_manifest_id"],
            result["behavior_checkpoint_id"],
            result["behavior_seed"],
            result["ascension_level"],
            result["held_out_seed_start"],
            result["slot_count"],
            result["terminal_attempt_target"],
            result["max_batch_steps"],
        ) != identity:
            raise RunEvaluationCommandError(
                "run potion comparison changed behavior, seeds, or bounds"
            )


def _summary(
    config: RunPotionComparisonCommandConfig,
    lanes: tuple[tuple[CombatPotionLane, dict[str, object], Path], ...],
) -> dict[str, object]:
    all_terminal = _terminal_seeds(lanes[0][1])
    never_terminal = _terminal_seeds(lanes[1][1])
    paired = tuple(sorted(set(all_terminal).intersection(never_terminal)))
    seed_rows = tuple(
        _seed_row(seed, all_terminal.get(seed), never_terminal.get(seed))
        for seed in sorted(set(all_terminal).union(never_terminal))
    )
    deltas = tuple(
        never_terminal[seed]["last_floor"] - all_terminal[seed]["last_floor"]
        for seed in paired
    )
    first = lanes[0][1]
    return {
        "schema": RUN_POTION_COMPARISON_SCHEMA,
        "kind": (
            "completed"
            if all(result["target_reached"] for _, result, _ in lanes)
            else "censored"
        ),
        "behavior": str(config.behavior),
        "behavior_manifest_id": first["behavior_manifest_id"],
        "behavior_checkpoint_id": first["behavior_checkpoint_id"],
        "behavior_seed": config.behavior_seed,
        "ascension_level": config.ascension_level,
        "held_out_seed_start": config.held_out_seed_start,
        "terminal_attempt_target": config.terminal_attempts,
        "max_batch_steps": config.max_batch_steps,
        "lanes": tuple(
            {
                "potion_lane": lane.value,
                "evaluation": str(
                    (output / "evaluation.json").relative_to(config.output)
                ),
                "target_reached": result["target_reached"],
                "terminal_attempts": result["terminal_attempts"],
                "terminal_floor_sum": result["terminal_floor_sum"],
                "combat_hp_loss_sum": result["combat_hp_loss_sum"],
                "potion_identity_losses": result[
                    "combat_potion_identity_losses"
                ],
                "batch_steps": result["batch_steps"],
            }
            for lane, result, output in lanes
        ),
        "comparison": {
            "paired_terminal_seeds": len(paired),
            "all_only_terminal_seeds": tuple(
                sorted(set(all_terminal) - set(never_terminal))
            ),
            "never_only_terminal_seeds": tuple(
                sorted(set(never_terminal) - set(all_terminal))
            ),
            "never_deeper": sum(delta > 0 for delta in deltas),
            "same_floor": sum(delta == 0 for delta in deltas),
            "never_shallower": sum(delta < 0 for delta in deltas),
            "paired_floor_delta_never_minus_all": sum(deltas),
        },
        "seeds": seed_rows,
    }


def _terminal_seeds(result: dict[str, object]) -> dict[int, dict[str, object]]:
    rows = {
        row["seed"]: row
        for row in result["combat_seed_summaries"]
        if row["terminal_reward"] is not None
    }
    if len(rows) != result["terminal_attempts"]:
        raise RunEvaluationCommandError(
            "run evaluation terminal seed summaries are incomplete"
        )
    return rows


def _seed_row(
    seed: int,
    all_row: dict[str, object] | None,
    never_row: dict[str, object] | None,
) -> dict[str, object]:
    all_floor = None if all_row is None else all_row["last_floor"]
    never_floor = None if never_row is None else never_row["last_floor"]
    return {
        "seed": seed,
        "all_floor": all_floor,
        "never_floor": never_floor,
        "floor_delta_never_minus_all": (
            None
            if all_floor is None or never_floor is None
            else never_floor - all_floor
        ),
        "all_hp_loss": None if all_row is None else all_row["hp_loss_sum"],
        "never_hp_loss": (
            None if never_row is None else never_row["hp_loss_sum"]
        ),
        "all_potion_identity_losses": (
            () if all_row is None else all_row["potion_identity_losses"]
        ),
        "never_potion_identity_losses": (
            () if never_row is None else never_row["potion_identity_losses"]
        ),
    }


def _optional(value: object) -> str:
    return "none" if value is None else str(value)


def _identities(values: object) -> str:
    return ",".join(f"{name}:{count}" for name, count in values) or "none"


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Compare all and never combat-potion surfaces on exactly paired runs."
        ),
    )
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--attempts", type=int, default=8)
    parser.add_argument("--max-batch-steps", type=int, default=4096)
    parser.add_argument("--behavior-seed", type=int, default=10_000)
    parser.add_argument(
        "--ascension",
        type=int,
        choices=range(21),
        required=True,
    )
    parser.add_argument("--held-out-seed-start", type=int, default=0)
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    run_run_potion_comparison(
        RunPotionComparisonCommandConfig(
            behavior=arguments.behavior,
            output=arguments.output,
            terminal_attempts=arguments.attempts,
            max_batch_steps=arguments.max_batch_steps,
            behavior_seed=arguments.behavior_seed,
            ascension_level=arguments.ascension,
            held_out_seed_start=arguments.held_out_seed_start,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
