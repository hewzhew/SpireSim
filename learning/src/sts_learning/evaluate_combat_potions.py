"""One-command no-potion, exact-root-potion, and unrestricted evaluation sweep."""

from __future__ import annotations

import argparse
import json
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

from .combat_potion_lane import CombatPotionLane
from .evaluate_combat import (
    CombatEvaluationCommandConfig,
    CombatEvaluationCommandError,
    run_combat_evaluation,
)
from .torch_combat_session_config import CombatSessionBridge
from .torch_session_config import CategoricalSessionBridge


COMBAT_POTION_SWEEP_SCHEMA = "sts-learning-combat-potion-sweep-v3"


@dataclass(frozen=True)
class CombatPotionSweepCommandConfig:
    artifact: Path
    behavior: Path
    output: Path
    root_count: int
    replicate_count: int
    behavior_seed_base: int

    def __post_init__(self) -> None:
        output = Path(self.output).resolve()
        if output.exists() and (
            not output.is_dir() or any(output.iterdir())
        ):
            raise CombatEvaluationCommandError(
                "combat potion sweep output must be absent or empty"
            )
        probe = CombatEvaluationCommandConfig(
            artifact=self.artifact,
            behavior=self.behavior,
            output=output / "no-potions",
            root_count=self.root_count,
            replicate_count=self.replicate_count,
            behavior_seed_base=self.behavior_seed_base,
            potion_lane=CombatPotionLane.NEVER,
        )
        object.__setattr__(self, "artifact", probe.artifact)
        object.__setattr__(self, "behavior", probe.behavior)
        object.__setattr__(self, "output", output)
        object.__setattr__(self, "root_count", probe.root_count)
        object.__setattr__(self, "replicate_count", probe.replicate_count)
        object.__setattr__(
            self,
            "behavior_seed_base",
            probe.behavior_seed_base,
        )


def run_combat_potion_sweep(
    config: CombatPotionSweepCommandConfig,
    *,
    bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
) -> dict[str, object]:
    """Run the complete bounded lane set with identical roots and RNG seeds."""

    if not isinstance(config, CombatPotionSweepCommandConfig):
        raise CombatEvaluationCommandError(
            "combat potion sweep config must be typed"
        )
    lane_results: list[tuple[str, dict[str, object], Path]] = []

    no_potions = _run_lane(
        config,
        bridge,
        run_bridge,
        label="never",
        directory="no-potions",
        lane=CombatPotionLane.NEVER,
    )
    lane_results.append(no_potions)
    for slot in _filled_root_slots(no_potions[1]):
        lane_results.append(
            _run_lane(
                config,
                bridge,
                run_bridge,
                label=f"root-slot-{slot}",
                directory=f"root-slot-{slot}",
                lane=CombatPotionLane.ROOT_SLOTS,
                potion_slots=(slot,),
            )
        )
    lane_results.append(
        _run_lane(
            config,
            bridge,
            run_bridge,
            label="all",
            directory="all-potions",
            lane=CombatPotionLane.ALL,
        )
    )

    _validate_lane_identity(tuple(result for _, result, _ in lane_results))
    summary = _summary(config, tuple(lane_results))
    config.output.mkdir(parents=True, exist_ok=True)
    with (config.output / "potion-sweep.json").open(
        "x",
        encoding="utf-8",
        newline="\n",
    ) as destination:
        json.dump(summary, destination, separators=(",", ":"), sort_keys=True)
    print(
        "combat_potion_sweep_complete=true "
        f"lanes={','.join(lane['label'] for lane in summary['lanes'])} "
        f"wins={','.join(str(lane['wins']) for lane in summary['lanes'])} "
        "final_hp_sums="
        f"{','.join(str(lane['final_hp_sum']) for lane in summary['lanes'])} "
        "potions_used="
        f"{','.join(str(lane['potions_used']) for lane in summary['lanes'])} "
        f"output={config.output}",
        flush=True,
    )
    print(
        "combat_potion_sweep_roots "
        "metrics=wins/final_hp_sum/potions_used/potions_discarded "
        f"count={len(summary['roots'])}",
        flush=True,
    )
    for root in summary["roots"]:
        print(_root_completion(root), flush=True)
    return summary


def _run_lane(
    config: CombatPotionSweepCommandConfig,
    bridge: CombatSessionBridge | None,
    run_bridge: CategoricalSessionBridge | None,
    *,
    label: str,
    directory: str,
    lane: CombatPotionLane,
    potion_slots: tuple[int, ...] = (),
) -> tuple[str, dict[str, object], Path]:
    output = config.output / directory
    result = run_combat_evaluation(
        CombatEvaluationCommandConfig(
            artifact=config.artifact,
            behavior=config.behavior,
            output=output,
            root_count=config.root_count,
            replicate_count=config.replicate_count,
            behavior_seed_base=config.behavior_seed_base,
            potion_lane=lane,
            potion_slots=potion_slots,
        ),
        bridge=bridge,
        run_bridge=run_bridge,
        print_completion=False,
    )
    return label, result, output


def _filled_root_slots(result: dict[str, object]) -> tuple[int, ...]:
    filled = {
        slot
        for root in result["roots"]
        for slot, potion in enumerate(root["context"]["potion_ids"])
        if potion is not None
    }
    return tuple(sorted(filled))


def _validate_lane_identity(results: tuple[dict[str, object], ...]) -> None:
    if not results:
        raise CombatEvaluationCommandError(
            "combat potion sweep produced no lanes"
        )
    first = results[0]
    boundary = (
        first["artifact_sha256"],
        first["behavior_manifest_id"],
        first["behavior_checkpoint_id"],
        first["behavior_training_artifact_sha256"],
        first["behavior_training_all_loss_axis"],
        first["behavior_training_potion_lane"],
        tuple(first["behavior_training_potion_slots"]),
        tuple(first["behavior_seeds"]),
    )
    roots = tuple(
        (root["root_id"], root["exact_combat_state_hash"])
        for root in first["roots"]
    )
    for result in results[1:]:
        if (
            result["artifact_sha256"],
            result["behavior_manifest_id"],
            result["behavior_checkpoint_id"],
            result["behavior_training_artifact_sha256"],
            result["behavior_training_all_loss_axis"],
            result["behavior_training_potion_lane"],
            tuple(result["behavior_training_potion_slots"]),
            tuple(result["behavior_seeds"]),
        ) != boundary:
            raise CombatEvaluationCommandError(
                "combat potion sweep lanes changed behavior or RNG identity"
            )
        if tuple(
            (root["root_id"], root["exact_combat_state_hash"])
            for root in result["roots"]
        ) != roots:
            raise CombatEvaluationCommandError(
                "combat potion sweep lanes changed exact root identity"
            )


def _summary(
    config: CombatPotionSweepCommandConfig,
    lanes: tuple[tuple[str, dict[str, object], Path], ...],
) -> dict[str, object]:
    first = lanes[0][1]
    return {
        "schema": COMBAT_POTION_SWEEP_SCHEMA,
        "kind": "completed",
        "artifact": str(config.artifact),
        "artifact_sha256": first["artifact_sha256"],
        "behavior": str(config.behavior),
        "behavior_manifest_id": first["behavior_manifest_id"],
        "behavior_checkpoint_id": first["behavior_checkpoint_id"],
        "behavior_training_artifact_sha256": first[
            "behavior_training_artifact_sha256"
        ],
        "behavior_training_all_loss_axis": first[
            "behavior_training_all_loss_axis"
        ],
        "behavior_training_potion_lane": first[
            "behavior_training_potion_lane"
        ],
        "behavior_training_potion_slots": first[
            "behavior_training_potion_slots"
        ],
        "root_count": config.root_count,
        "replicate_count": config.replicate_count,
        "behavior_seeds": first["behavior_seeds"],
        "lanes": tuple(
            _lane_summary(config.output, label, result, output)
            for label, result, output in lanes
        ),
        "roots": tuple(
            _root_summary(slot, lanes)
            for slot in range(config.root_count)
        ),
    }


def _lane_summary(
    root: Path,
    label: str,
    result: dict[str, object],
    output: Path,
) -> dict[str, object]:
    return {
        "label": label,
        "potion_lane": result["potion_lane"],
        "potion_slots": result["potion_slots"],
        "evaluation": str((output / "evaluation.json").relative_to(root)),
        "wins": result["wins"],
        "losses": result["losses"],
        "final_hp_sum": result["final_hp_sum"],
        "potions_used": result["potions_used"],
        "potions_discarded": result["potions_discarded"],
        "lost_potion_ids": result["lost_potion_ids"],
        "gained_potion_ids": result["gained_potion_ids"],
        "root_wins": tuple(root["wins"] for root in result["roots"]),
        "root_final_hp_sums": tuple(
            root["final_hp_sum"] for root in result["roots"]
        ),
        "root_potions_used": tuple(
            root["potions_used"] for root in result["roots"]
        ),
    }


def _root_summary(
    slot: int,
    lanes: tuple[tuple[str, dict[str, object], Path], ...],
) -> dict[str, object]:
    first = lanes[0][1]["roots"][slot]
    return {
        "slot_index": slot,
        "root_id": first["root_id"],
        "exact_combat_state_hash": first["exact_combat_state_hash"],
        "context": first["context"],
        "lanes": tuple(
            _root_lane_summary(label, result["roots"][slot])
            for label, result, _ in lanes
        ),
    }


def _root_lane_summary(label: str, root: dict[str, object]) -> dict[str, object]:
    lost = Counter(
        potion
        for outcome in root["outcomes"]
        for potion in outcome["lost_potion_ids"]
    )
    gained = Counter(
        potion
        for outcome in root["outcomes"]
        for potion in outcome["gained_potion_ids"]
    )
    return {
        "label": label,
        "wins": root["wins"],
        "losses": root["losses"],
        "final_hp_sum": root["final_hp_sum"],
        "potions_used": root["potions_used"],
        "potions_discarded": root["potions_discarded"],
        "lost_potion_ids": dict(sorted(lost.items())),
        "gained_potion_ids": dict(sorted(gained.items())),
    }


def _root_completion(root: dict[str, object]) -> str:
    context = root["context"]
    potions = "+".join(
        potion for potion in context["potion_ids"] if potion is not None
    ) or "none"
    lanes = ",".join(
        f"{lane['label']}:{lane['wins']}/{lane['final_hp_sum']}/"
        f"{lane['potions_used']}/{lane['potions_discarded']}"
        for lane in root["lanes"]
    )
    return (
        f"combat_potion_sweep_root slot={root['slot_index']} "
        f"site=A{context['act']}F{context['floor']} "
        f"hp={context['hp']}/{context['max_hp']} potions={potions} "
        f"lanes={lanes}"
    )


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Compare no-potion, each filled root potion slot, and all-potion "
            "combat action surfaces under one frozen behavior."
        ),
    )
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--roots", type=int, required=True)
    parser.add_argument("--replicates", type=int, default=8)
    parser.add_argument("--behavior-seed-base", type=int, default=10_000)
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    run_combat_potion_sweep(
        CombatPotionSweepCommandConfig(
            artifact=arguments.artifact,
            behavior=arguments.behavior,
            output=arguments.output,
            root_count=arguments.roots,
            replicate_count=arguments.replicates,
            behavior_seed_base=arguments.behavior_seed_base,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
