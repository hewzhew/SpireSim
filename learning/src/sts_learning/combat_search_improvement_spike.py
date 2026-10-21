"""Single-root conditioned-chance combat policy-improvement feasibility spike."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import statistics
from dataclasses import asdict
from pathlib import Path

from .combat_outcomes import CombatTerminalStepBatch
from .combat_root_artifacts import load_combat_root_source, read_combat_root_artifact
from .exact_successor_search import (
    paired_search_comparison,
    run_equal_work_successor_search,
    select_complete_search_proposal,
)
from .published_combat_behavior import recover_published_combat_behavior
from .torch_behavior import FrozenGreedyTorchPolicy
from .torch_combat_session_config import CombatSessionBridge, CombatWinSessionLimits


def _mean_and_standard_error(values: list[float]) -> tuple[float | None, float | None]:
    if not values:
        return None, None
    mean = statistics.fmean(values)
    standard_error = (
        statistics.stdev(values) / math.sqrt(len(values)) if len(values) > 1 else None
    )
    return mean, standard_error


def _root_audit(source: object, root_slot: int) -> dict[str, object]:
    audit = source.combat_root_audit(root_slot)
    return {
        "seed": audit.seed,
        "act": audit.act,
        "floor": audit.floor,
        "ascension_level": audit.ascension_level,
        "hp": audit.hp,
        "max_hp": audit.max_hp,
        "encounter_id": audit.encounter_id,
        "monster_ids": audit.monster_ids,
        "master_deck_cards": audit.master_deck_cards,
        "relic_ids": audit.relic_ids,
        "potion_ids": audit.potion_ids,
    }


def _rollout_action(
    source: object,
    particle_slot: int,
    root_ordinal: int,
    policy: FrozenGreedyTorchPolicy,
    *,
    max_model_rounds: int,
    max_transitions: int,
) -> dict[str, object]:
    group = source.combat_group(particle_slot, 1, potion_slots=[])
    forced = False
    model_rounds = 0
    transitions = 0
    while group.terminal_count < 1:
        while not group.ready:
            if model_rounds >= max_model_rounds:
                return {"status": "unknown", "reason": "model_round_limit"}
            decision = group.decision_batch(semantic=True)
            if not forced:
                row_splits = tuple(int(value) for value in decision["candidate_row_splits"])
                if len(row_splits) != 2 or not 0 <= root_ordinal < row_splits[1]:
                    raise RuntimeError(
                        "root candidate ordinal is not present on a chance particle"
                    )
                ordinals = [root_ordinal]
                forced = True
            else:
                ordinals = list(policy.choose(decision).ordinals)
            group.choose(ordinals)
            model_rounds += 1
        if transitions >= max_transitions:
            return {"status": "unknown", "reason": "transition_limit"}
        step = group.step()
        terminal = CombatTerminalStepBatch.from_bridge_step(step, replicate_count=1)
        transitions += 1
        if terminal.outcomes:
            outcome = terminal.outcomes[0]
            row = asdict(outcome)
            row["terminal_kind"] = outcome.terminal_kind.name.lower()
            row.update(
                status="terminal",
                model_rounds=model_rounds,
                transitions=transitions,
            )
            return row
    raise RuntimeError("combat chance rollout terminated without an outcome")


def _evaluate_action(
    source: object,
    particle_provenance_seeds: tuple[int, ...],
    ordinal: int,
    policy: FrozenGreedyTorchPolicy,
    *,
    max_model_rounds: int,
    max_transitions: int,
) -> dict[str, object]:
    outcomes = [
        {
            "particle_provenance_seed": seed,
            **_rollout_action(
                source,
                particle_slot,
                ordinal,
                policy,
                max_model_rounds=max_model_rounds,
                max_transitions=max_transitions,
            ),
        }
        for particle_slot, seed in enumerate(particle_provenance_seeds)
    ]
    terminal = [row for row in outcomes if row["status"] == "terminal"]
    wins = [row for row in terminal if row["won"]]
    win_rate, win_rate_standard_error = _mean_and_standard_error(
        [1.0 if row["won"] else 0.0 for row in terminal]
    )
    mean_final_hp, mean_final_hp_standard_error = _mean_and_standard_error(
        [float(row["final_hp"]) for row in terminal]
    )
    mean_winning_final_hp, mean_winning_final_hp_standard_error = (
        _mean_and_standard_error([float(row["final_hp"]) for row in wins])
    )
    return {
        "ordinal": ordinal,
        "sample_count": len(outcomes),
        "completed_count": len(terminal),
        "unknown_count": len(outcomes) - len(terminal),
        "wins": len(wins),
        "win_rate": win_rate,
        "win_rate_standard_error": win_rate_standard_error,
        "mean_final_hp": mean_final_hp,
        "mean_final_hp_standard_error": mean_final_hp_standard_error,
        "mean_winning_final_hp": mean_winning_final_hp,
        "mean_winning_final_hp_standard_error": mean_winning_final_hp_standard_error,
        "outcomes": outcomes,
    }


def _proposal(actions: list[dict[str, object]]) -> tuple[int | None, str]:
    if any(int(action["unknown_count"]) for action in actions):
        return None, "incomplete_equal_allocation"
    if not actions or max(int(action["wins"]) for action in actions) == 0:
        return None, "no_action_demonstrated_a_win"
    selected = max(
        actions,
        key=lambda action: (
            int(action["wins"]),
            sum(
                int(row["final_hp"])
                for row in action["outcomes"]
                if row["status"] == "terminal"
            ),
            -int(action["ordinal"]),
        ),
    )
    return int(selected["ordinal"]), "win_count_then_total_final_hp"


def _paired_summary(
    particle_provenance_seeds: tuple[int, ...],
    candidate: dict[str, object],
    baseline: dict[str, object],
) -> dict[str, object]:
    pairs = []
    for seed, candidate_row, baseline_row in zip(
        particle_provenance_seeds,
        candidate["outcomes"],
        baseline["outcomes"],
        strict=True,
    ):
        if candidate_row["status"] != "terminal" or baseline_row["status"] != "terminal":
            pairs.append({"particle_provenance_seed": seed, "status": "unknown"})
            continue
        both_win = bool(candidate_row["won"] and baseline_row["won"])
        pairs.append(
            {
                "particle_provenance_seed": seed,
                "status": "paired",
                "candidate_won": bool(candidate_row["won"]),
                "baseline_won": bool(baseline_row["won"]),
                "win_delta": int(candidate_row["won"]) - int(baseline_row["won"]),
                "both_win_final_hp_delta": (
                    int(candidate_row["final_hp"]) - int(baseline_row["final_hp"])
                    if both_win
                    else None
                ),
            }
        )
    completed = [pair for pair in pairs if pair["status"] == "paired"]
    both_win_hp = [
        int(pair["both_win_final_hp_delta"])
        for pair in completed
        if pair["both_win_final_hp_delta"] is not None
    ]
    win_deltas = [float(pair["win_delta"]) for pair in completed]
    mean_win_delta, mean_win_delta_standard_error = _mean_and_standard_error(win_deltas)
    mean_hp_delta, mean_hp_delta_standard_error = _mean_and_standard_error(
        [float(value) for value in both_win_hp]
    )
    return {
        "paired_count": len(completed),
        "unknown_count": len(pairs) - len(completed),
        "candidate_wins": int(candidate["wins"]),
        "baseline_wins": int(baseline["wins"]),
        "mean_win_delta": mean_win_delta,
        "mean_win_delta_standard_error": mean_win_delta_standard_error,
        "mean_both_win_final_hp_delta": mean_hp_delta,
        "mean_both_win_final_hp_delta_standard_error": mean_hp_delta_standard_error,
        "pairs": pairs,
    }


def _load_chance_population(
    LearningBatchEnv: object,
    artifact: bytes,
    *,
    sampler: str,
    expected_roots: int,
    source_slot: int,
    candidate_floor_seed_base_start: int,
    particle_count: int,
    max_candidates: int,
    max_bytes: int,
) -> tuple[object, tuple[int, ...], dict[str, object]]:
    if sampler == "conditioned-floor-chance":
        loader = getattr(
            LearningBatchEnv,
            "from_combat_entry_floor_chance_particles",
            None,
        )
        if not callable(loader):
            raise RuntimeError(
                "installed learning bridge does not provide conditioned combat-entry floor particles"
            )
        loaded = loader(
            artifact,
            expected_roots=expected_roots,
            source_slot=source_slot,
            candidate_floor_seed_base_start=candidate_floor_seed_base_start,
            max_candidates=max_candidates,
            required_particles=particle_count,
            max_bytes=max_bytes,
        )
        source, accepted, attempted, public_matches, duplicate_private_states = loaded
        accepted_seed_bases = tuple(int(seed) for seed in accepted)
        if len(accepted_seed_bases) != particle_count:
            raise RuntimeError(
                "conditioned combat-entry floor scan returned an incomplete population"
            )
        return (
            source,
            accepted_seed_bases,
            {
                "seed_semantics": "run_seed_base_applied_only_to_floor_local_streams",
                "candidate_floor_seed_base_start": candidate_floor_seed_base_start,
                "max_candidates": max_candidates,
                "attempted_candidate_count": int(attempted),
                "public_match_count": int(public_matches),
                "duplicate_private_state_count": int(duplicate_private_states),
                "accepted_particle_count": len(accepted_seed_bases),
            },
        )

    loader = getattr(LearningBatchEnv, "from_combat_public_chance_particles", None)
    if not callable(loader):
        raise RuntimeError("installed learning bridge does not provide public-chance particles")
    particle_seeds = tuple(
        candidate_floor_seed_base_start + index for index in range(particle_count)
    )
    return (
        loader(
            artifact,
            expected_roots=expected_roots,
            source_slot=source_slot,
            particle_seeds=list(particle_seeds),
            max_bytes=max_bytes,
        ),
        particle_seeds,
        {
            "seed_semantics": "independent_hidden_stream_seed",
            "candidate_particle_seed_start": candidate_floor_seed_base_start,
            "max_candidates": particle_count,
            "attempted_candidate_count": particle_count,
            "public_match_count": particle_count,
            "duplicate_private_state_count": 0,
            "accepted_particle_count": particle_count,
        },
    )


def _write_chance_artifact(
    source: object,
    particle_count: int,
    path: Path,
    *,
    max_bytes: int,
) -> dict[str, object]:
    payload = bytes(
        source.combat_root_artifact_bytes(
            list(range(particle_count)),
            max_bytes=max_bytes,
        )
    )
    if not payload or len(payload) > max_bytes:
        raise RuntimeError("chance-particle root artifact violates its byte bound")
    with path.open("xb") as destination:
        destination.write(payload)
    return {
        "artifact": str(path),
        "root_count": particle_count,
        "artifact_bytes": len(payload),
        "artifact_sha256": hashlib.sha256(payload).hexdigest(),
    }


def run(args: argparse.Namespace) -> dict[str, object]:
    limits = CombatWinSessionLimits()
    artifact = read_combat_root_artifact(args.artifact, max_bytes=limits.max_artifact_bytes)
    bridge = CombatSessionBridge.installed()
    try:
        from sts_learning_bridge import LearningBatchEnv
    except ImportError as error:
        raise RuntimeError("standalone learning bridge wheel is not installed") from error
    behavior = recover_published_combat_behavior(
        args.behavior,
        bridge,
        limits,
        behavior_seeds=(args.policy_seed,),
    )
    policy = FrozenGreedyTorchPolicy.from_behavior(behavior.policies[0])
    exact_source = load_combat_root_source(
        bridge,
        artifact,
        expected_roots=args.root_count,
        max_bytes=limits.max_artifact_bytes,
    )
    exact_group = exact_source.combat_group(args.root_slot, 1, potion_slots=[])
    audit_raw = exact_group.combat_decision_audit_json(0)
    if not isinstance(audit_raw, str):
        raise RuntimeError("source root has no atomic combat decision audit")
    audit = json.loads(audit_raw)
    candidates = audit.get("candidates")
    if not isinstance(candidates, list) or not candidates:
        raise RuntimeError("source root exposes no typed combat candidates")
    baseline_choice = policy.choose(exact_group.decision_batch(semantic=True))
    if len(baseline_choice.ordinals) != 1:
        raise RuntimeError("source root policy decision is not singular")
    baseline_ordinal = int(baseline_choice.ordinals[0])

    selection_source, selection_seeds, selection_chance_scan = _load_chance_population(
        LearningBatchEnv,
        artifact,
        sampler=args.chance_sampler,
        expected_roots=args.root_count,
        source_slot=args.root_slot,
        candidate_floor_seed_base_start=args.floor_seed_base,
        particle_count=args.selection_particles,
        max_candidates=args.max_chance_candidates,
        max_bytes=limits.max_artifact_bytes,
    )
    evaluation_floor_seed_base_start = (
        args.floor_seed_base + args.max_chance_candidates
    )
    evaluation_source, evaluation_seeds, evaluation_chance_scan = _load_chance_population(
        LearningBatchEnv,
        artifact,
        sampler=args.chance_sampler,
        expected_roots=args.root_count,
        source_slot=args.root_slot,
        candidate_floor_seed_base_start=evaluation_floor_seed_base_start,
        particle_count=args.evaluation_particles,
        max_candidates=args.max_chance_candidates,
        max_bytes=limits.max_artifact_bytes,
    )
    chance_particle_artifacts: dict[str, object] | None = None
    if args.chance_artifact_dir is not None:
        chance_particle_artifacts = {
            "selection": _write_chance_artifact(
                selection_source,
                args.selection_particles,
                args.chance_artifact_dir / "selection.combat-roots.bin",
                max_bytes=limits.max_artifact_bytes,
            ),
            "evaluation": _write_chance_artifact(
                evaluation_source,
                args.evaluation_particles,
                args.chance_artifact_dir / "evaluation.combat-roots.bin",
                max_bytes=limits.max_artifact_bytes,
            ),
        }
    exact_successor_search: dict[str, object] | None = None
    if args.oracle_binary is not None:
        selection_search = run_equal_work_successor_search(
            oracle_binary=args.oracle_binary,
            artifact=args.chance_artifact_dir / "selection.combat-roots.bin",
            root_count=args.selection_particles,
            candidate_count=len(candidates),
            output_dir=args.chance_artifact_dir / "selection-successors",
            solve_work_per_candidate=args.solve_work_per_candidate,
            candidate_jobs=args.candidate_jobs,
            max_artifact_bytes=limits.max_artifact_bytes,
            no_potions=True,
        )
        search_proposal_ordinal, search_proposal_rule = (
            select_complete_search_proposal(
                selection_search,
                baseline_ordinal=baseline_ordinal,
            )
        )
        evaluation_search = run_equal_work_successor_search(
            oracle_binary=args.oracle_binary,
            artifact=args.chance_artifact_dir / "evaluation.combat-roots.bin",
            root_count=args.evaluation_particles,
            candidate_count=len(candidates),
            output_dir=args.chance_artifact_dir / "evaluation-successors",
            solve_work_per_candidate=args.solve_work_per_candidate,
            candidate_jobs=args.candidate_jobs,
            max_artifact_bytes=limits.max_artifact_bytes,
            no_potions=True,
        )
        exact_successor_search = {
            "selection": selection_search,
            "proposal_ordinal": search_proposal_ordinal,
            "proposal_candidate": (
                None
                if search_proposal_ordinal is None
                else candidates[search_proposal_ordinal]
            ),
            "proposal_rule": search_proposal_rule,
            "evaluation": evaluation_search,
            "paired": (
                None
                if search_proposal_ordinal is None
                else paired_search_comparison(
                    evaluation_search,
                    candidate_ordinal=search_proposal_ordinal,
                    baseline_ordinal=baseline_ordinal,
                )
            ),
        }
    selection_actions = [
        {
            "candidate": candidates[ordinal],
            **_evaluate_action(
                selection_source,
                selection_seeds,
                ordinal,
                policy,
                max_model_rounds=args.max_model_rounds,
                max_transitions=args.max_transitions,
            ),
        }
        for ordinal in range(len(candidates))
    ]
    proposal_ordinal, proposal_rule = _proposal(selection_actions)

    evaluation: dict[str, object] | None = None
    if proposal_ordinal is not None:
        candidate_result = _evaluate_action(
            evaluation_source,
            evaluation_seeds,
            proposal_ordinal,
            policy,
            max_model_rounds=args.max_model_rounds,
            max_transitions=args.max_transitions,
        )
        baseline_result = (
            candidate_result
            if baseline_ordinal == proposal_ordinal
            else _evaluate_action(
                evaluation_source,
                evaluation_seeds,
                baseline_ordinal,
                policy,
                max_model_rounds=args.max_model_rounds,
                max_transitions=args.max_transitions,
            )
        )
        evaluation = {
            "candidate": candidate_result,
            "baseline": baseline_result,
            "paired": _paired_summary(evaluation_seeds, candidate_result, baseline_result),
        }

    return {
        "schema_name": "CombatSearchImprovementFeasibilitySpike",
        "schema_version": 3,
        "teacher_valid": False,
        "claim": "single_root_combat_entry_improvement_feasibility_only",
        "artifact_sha256": hashlib.sha256(artifact).hexdigest(),
        "source_root_slot": args.root_slot,
        "source_root": _root_audit(exact_source, args.root_slot),
        "behavior_manifest_id": behavior.manifest_id.digest.hex(),
        "chance_sampler": (
            "conditioned_combat_entry_floor_seed_rejection_v1"
            if args.chance_sampler == "conditioned-floor-chance"
            else "independent_hidden_stream_reseed_v1"
        ),
        "selection_chance_scan": selection_chance_scan,
        "evaluation_chance_scan": evaluation_chance_scan,
        "potion_lane": "never",
        "selection_particle_provenance_seeds": selection_seeds,
        "evaluation_particle_provenance_seeds": evaluation_seeds,
        "selection_evaluation_disjoint": set(selection_seeds).isdisjoint(evaluation_seeds),
        "chance_particle_artifacts": chance_particle_artifacts,
        "exact_successor_search": exact_successor_search,
        "baseline_ordinal": baseline_ordinal,
        "baseline_candidate": candidates[baseline_ordinal],
        "rollout": {
            "continuation_policy": "frozen_greedy_current_behavior",
            "proposal_ordinal": proposal_ordinal,
            "proposal_candidate": (
                None if proposal_ordinal is None else candidates[proposal_ordinal]
            ),
            "proposal_rule": proposal_rule,
            "selection_actions": selection_actions,
            "evaluation": evaluation,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--root-count", type=int, default=1)
    parser.add_argument("--root-slot", type=int, default=0)
    parser.add_argument("--selection-particles", type=int, default=8)
    parser.add_argument("--evaluation-particles", type=int, default=8)
    parser.add_argument("--floor-seed-base", type=int, default=2026081101000)
    parser.add_argument(
        "--chance-sampler",
        choices=("conditioned-floor-chance", "independent-streams"),
        default="conditioned-floor-chance",
    )
    parser.add_argument("--max-chance-candidates", type=int, default=65_536)
    parser.add_argument("--policy-seed", type=int, default=2026081101999)
    parser.add_argument("--max-model-rounds", type=int, default=512)
    parser.add_argument("--max-transitions", type=int, default=512)
    parser.add_argument("--chance-artifact-dir", type=Path)
    parser.add_argument("--oracle-binary", type=Path)
    parser.add_argument("--solve-work-per-candidate", type=int, default=5_000)
    parser.add_argument("--candidate-jobs", type=int, default=4)
    args = parser.parse_args()
    for name in (
        "root_count",
        "selection_particles",
        "evaluation_particles",
        "max_chance_candidates",
        "max_model_rounds",
        "max_transitions",
        "solve_work_per_candidate",
        "candidate_jobs",
    ):
        if getattr(args, name) <= 0:
            parser.error(f"--{name.replace('_', '-')} must be positive")
    if args.root_slot < 0 or args.root_slot >= args.root_count:
        parser.error("--root-slot must be inside --root-count")
    if args.output.exists():
        parser.error("--output must be a fresh path")
    if args.chance_artifact_dir is not None:
        if args.chance_artifact_dir.exists():
            parser.error("--chance-artifact-dir must be a fresh path")
        args.chance_artifact_dir.mkdir(parents=True)
    if args.oracle_binary is not None:
        if args.chance_artifact_dir is None:
            parser.error("--oracle-binary requires --chance-artifact-dir")
        if not args.oracle_binary.is_file():
            parser.error("--oracle-binary must be an existing file")

    result = run(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False) + "\n",
        encoding="utf-8",
    )
    rollout = result["rollout"]
    paired = None
    if rollout["evaluation"] is not None:
        paired = {
            key: value
            for key, value in rollout["evaluation"]["paired"].items()
            if key != "pairs"
        }
    exact_search = result["exact_successor_search"]
    search_paired = None
    if exact_search is not None and exact_search["paired"] is not None:
        search_paired = {
            key: value
            for key, value in exact_search["paired"].items()
            if key != "pairs"
        }
    print(
        json.dumps(
            {
                "output": str(args.output),
                "candidate_count": len(rollout["selection_actions"]),
                "baseline_ordinal": result["baseline_ordinal"],
                "rollout_proposal_ordinal": rollout["proposal_ordinal"],
                "rollout_proposal_rule": rollout["proposal_rule"],
                "rollout_paired": paired,
                "search_proposal_ordinal": (
                    None if exact_search is None else exact_search["proposal_ordinal"]
                ),
                "search_proposal_rule": (
                    None if exact_search is None else exact_search["proposal_rule"]
                ),
                "search_paired": search_paired,
            },
            ensure_ascii=False,
            separators=(",", ":"),
        )
    )


if __name__ == "__main__":
    main()
