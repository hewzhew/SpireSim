"""Evaluate one published combat behavior on an independent root artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
import time
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

import torch

from .combat_evaluation import (
    CombatEvaluationLimits,
    CombatEvaluationRootResult,
    CombatHeldOutEvaluationResult,
    CombatHeldOutEvaluator,
    combat_observed_resource_frontier,
)
from .combat_outcomes import combat_advantage_has_signal
from .combat_potion_lane import (
    CombatPotionLane,
    CombatPotionLaneError,
    normalize_combat_potion_slots,
)
from .combat_root_artifacts import (
    load_combat_root_source,
    read_combat_root_artifact,
)
from .combat_root_audit import (
    CombatRootAudit,
    CombatRootAuditError,
    read_combat_root_audits,
)
from .published_combat_behavior import (
    PublishedCombatBehavior,
    recover_published_combat_behavior,
)
from .published_run_behavior import (
    PublishedRunBehavior,
    is_run_training_publication,
    recover_published_run_behavior,
)
from .torch_combat_session_config import (
    CombatSessionBridge,
    CombatWinSessionLimits,
)
from .torch_behavior import (
    FrozenCategoricalTorchPolicy,
    FrozenCombatGreedyTorchPolicy,
    FrozenDecisionRule,
    FrozenGreedyTorchPolicy,
    FrozenReplicateCategoricalTorchPolicy,
)
from .torch_session_config import CategoricalSessionBridge


COMBAT_EVALUATION_SCHEMA = "sts-learning-combat-held-out-evaluation-v16"
COMBAT_ACTION_TRACE_SCHEMA = "sts-learning-combat-action-trace-v2"
COMBAT_ACTION_TRACE_FILENAME = "combat-traces.jsonl"


class CombatEvaluationCommandError(RuntimeError):
    """A held-out combat evaluation command is malformed."""


@dataclass(frozen=True)
class CombatEvaluationCommandConfig:
    artifact: Path
    behavior: Path
    output: Path
    root_count: int
    replicate_count: int
    behavior_seed_base: int
    decision_rule: FrozenDecisionRule = FrozenDecisionRule.SAMPLED
    potion_lane: CombatPotionLane = CombatPotionLane.ALL
    potion_slots: tuple[int, ...] = ()
    trace_replicates_per_root: int = 0

    def __post_init__(self) -> None:
        artifact = Path(self.artifact).resolve()
        behavior = Path(self.behavior).resolve()
        output = Path(self.output).resolve()
        if not artifact.is_file():
            raise CombatEvaluationCommandError(
                "combat evaluation artifact is not a file"
            )
        if not behavior.is_dir():
            raise CombatEvaluationCommandError(
                "published combat behavior is not a directory"
            )
        if output.exists() and (
            not output.is_dir() or any(output.iterdir())
        ):
            raise CombatEvaluationCommandError(
                "combat evaluation output must be absent or empty"
            )
        if output == behavior or behavior in output.parents:
            raise CombatEvaluationCommandError(
                "combat evaluation output must stay outside the behavior directory"
            )
        root_count = _positive(self.root_count, "root_count")
        replicate_count = _positive(self.replicate_count, "replicate_count")
        behavior_seed_base = _seed(
            self.behavior_seed_base,
            "behavior_seed_base",
        )
        trace_replicates_per_root = _nonnegative(
            self.trace_replicates_per_root,
            "trace_replicates_per_root",
        )
        if trace_replicates_per_root > replicate_count:
            raise CombatEvaluationCommandError(
                "combat evaluation cannot trace more replicates than it runs"
            )
        if not isinstance(self.potion_lane, CombatPotionLane):
            raise CombatEvaluationCommandError(
                "combat evaluation potion_lane must be typed"
            )
        if not isinstance(self.decision_rule, FrozenDecisionRule):
            raise CombatEvaluationCommandError(
                "combat evaluation decision_rule must be typed"
            )
        try:
            potion_slots = normalize_combat_potion_slots(
                self.potion_lane,
                self.potion_slots,
            )
        except CombatPotionLaneError as error:
            raise CombatEvaluationCommandError(str(error)) from error
        if replicate_count < 2:
            raise CombatEvaluationCommandError(
                "combat evaluation requires at least two replicates"
            )
        if behavior_seed_base + root_count * replicate_count > 1 << 63:
            raise CombatEvaluationCommandError(
                "behavior seeds must stay below 2^63"
            )
        object.__setattr__(self, "artifact", artifact)
        object.__setattr__(self, "behavior", behavior)
        object.__setattr__(self, "output", output)
        object.__setattr__(self, "root_count", root_count)
        object.__setattr__(self, "replicate_count", replicate_count)
        object.__setattr__(self, "behavior_seed_base", behavior_seed_base)
        object.__setattr__(self, "potion_slots", potion_slots)
        object.__setattr__(
            self,
            "trace_replicates_per_root",
            trace_replicates_per_root,
        )

    @property
    def behavior_seeds(self) -> tuple[tuple[int, ...], ...]:
        """Explicit root-major RNG seed matrix indexed by replicate slot."""

        return tuple(
            tuple(
                self.behavior_seed_base
                + root_index * self.replicate_count
                + replicate_index
                for replicate_index in range(self.replicate_count)
            )
            for root_index in range(self.root_count)
        )

    @property
    def loader_behavior_seeds(self) -> tuple[int, ...]:
        """One recovery seed per root; the first replicate retains its stream."""

        return tuple(row[0] for row in self.behavior_seeds)


def run_combat_evaluation(
    config: CombatEvaluationCommandConfig,
    *,
    bridge: CombatSessionBridge | None = None,
    run_bridge: CategoricalSessionBridge | None = None,
    print_completion: bool = True,
) -> dict[str, object]:
    """Recover exact frozen behavior and evaluate it without training owners."""

    if not isinstance(config, CombatEvaluationCommandConfig):
        raise CombatEvaluationCommandError(
            "combat evaluation config must be typed"
        )
    active_bridge = bridge if bridge is not None else CombatSessionBridge.installed()
    if not isinstance(active_bridge, CombatSessionBridge):
        raise CombatEvaluationCommandError(
            "combat evaluation bridge must be typed"
        )
    session_limits = CombatWinSessionLimits()
    artifact = read_combat_root_artifact(
        config.artifact,
        max_bytes=session_limits.max_artifact_bytes,
    )
    artifact_sha256 = hashlib.sha256(artifact).hexdigest()
    recovered: PublishedCombatBehavior | PublishedRunBehavior
    if is_run_training_publication(config.behavior):
        active_run_bridge = (
            run_bridge
            if run_bridge is not None
            else CategoricalSessionBridge.installed()
        )
        if not isinstance(active_run_bridge, CategoricalSessionBridge):
            raise CombatEvaluationCommandError(
                "run behavior recovery bridge must be typed"
            )
        if active_run_bridge.semantic_schema != active_bridge.semantic_schema:
            raise CombatEvaluationCommandError(
                "run behavior and combat evaluation semantic schemas differ"
            )
        recovered = recover_published_run_behavior(
            config.behavior,
            active_run_bridge,
            config.loader_behavior_seeds,
        )
    else:
        recovered = recover_published_combat_behavior(
            config.behavior,
            active_bridge,
            session_limits,
            config.loader_behavior_seeds,
        )
        if artifact_sha256 == recovered.training_artifact_sha256:
            raise CombatEvaluationCommandError(
                "combat held-out evaluation artifact matches the training artifact"
            )
    source = load_combat_root_source(
        active_bridge,
        artifact,
        expected_roots=config.root_count,
        max_bytes=session_limits.max_artifact_bytes,
    )
    try:
        root_audits = read_combat_root_audits(
            source,
            tuple(range(config.root_count)),
        )
    except CombatRootAuditError as error:
        raise CombatEvaluationCommandError(str(error)) from error
    evaluation_policies = tuple(
        policy.bind_combat_only()
        if isinstance(policy, FrozenCombatGreedyTorchPolicy)
        else policy
        for policy in recovered.policies
    )
    if config.decision_rule is FrozenDecisionRule.GREEDY:
        evaluation_policies = tuple(
            FrozenGreedyTorchPolicy.from_behavior(policy)
            for policy in recovered.policies
        )
    else:
        evaluation_policies = tuple(
            _bind_replicate_rng_streams(policy, seeds)
            if isinstance(policy, FrozenCategoricalTorchPolicy)
            else policy
            for policy, seeds in zip(
                evaluation_policies,
                config.behavior_seeds,
                strict=True,
            )
        )
    evaluator = CombatHeldOutEvaluator(
        source,
        slot_indices=tuple(range(config.root_count)),
        replicate_count=config.replicate_count,
        policies=evaluation_policies,
        max_roots=config.root_count,
        limits=CombatEvaluationLimits(
            max_model_rounds=session_limits.experience.max_model_rounds,
            max_transitions=session_limits.experience.max_transitions,
        ),
        potion_lane=config.potion_lane,
        potion_slots=config.potion_slots,
        trace_replicates_per_root=config.trace_replicates_per_root,
    )

    started = time.perf_counter()
    result = evaluator.evaluate()
    elapsed = time.perf_counter() - started
    summary = _summary(
        config,
        recovered,
        result,
        root_audits=root_audits,
        artifact_bytes=len(artifact),
        artifact_sha256=artifact_sha256,
        elapsed=elapsed,
    )
    config.output.mkdir(parents=True, exist_ok=True)
    if config.trace_replicates_per_root:
        _write_decision_traces(config.output, result)
    with (config.output / "evaluation.json").open(
        "x",
        encoding="utf-8",
        newline="\n",
    ) as destination:
        json.dump(summary, destination, separators=(",", ":"), sort_keys=True)
        destination.write("\n")

    root_wins = ",".join(str(root.wins) for root in result.roots)
    root_final_hp = ",".join(
        str(sum(outcome.final_hp for outcome in root.group.outcomes))
        for root in result.roots
    )
    root_enemy_final_hp = ",".join(
        str(sum(outcome.enemy_final_hp for outcome in root.group.outcomes))
        for root in result.roots
    )
    root_enemy_final_hp_ranges = ",".join(
        f"{min(outcome.enemy_final_hp for outcome in root.group.outcomes)}-"
        f"{max(outcome.enemy_final_hp for outcome in root.group.outcomes)}"
        for root in result.roots
    )
    root_enemy_hp_signals = ",".join(
        str(
            sum(
                combat_advantage_has_signal(value)
                for value in root.group.grouped_advantages().enemy_hp_progress
            )
        )
        for root in result.roots
    )
    root_potions_used = ",".join(
        str(sum(outcome.potions_used for outcome in root.group.outcomes))
        for root in result.roots
    )
    root_potions_discarded = ",".join(
        str(sum(outcome.potions_discarded for outcome in root.group.outcomes))
        for root in result.roots
    )
    root_sites = ",".join(
        f"A{root.context.act}F{root.context.floor}"
        for root in result.roots
    )
    root_start_potions = ",".join(
        "+".join(
            potion
            for potion in root.context.potion_ids
            if potion is not None
        )
        or "-"
        for root in result.roots
    )
    resource_frontiers = tuple(
        combat_observed_resource_frontier(root.group.outcomes)
        for root in result.roots
    )
    root_resource_frontiers = ",".join(
        str(len(frontier.frontier_replicate_indices))
        for frontier in resource_frontiers
    )
    root_resource_dominated = ",".join(
        str(len(frontier.dominated_replicate_indices))
        for frontier in resource_frontiers
    )
    potion_losses = Counter(
        potion
        for root in result.roots
        for outcome in root.group.outcomes
        for potion in _potion_identity_change(
            root.context.potion_ids,
            outcome.final_potion_ids,
        )[0]
    )
    potion_gains = Counter(
        potion
        for root in result.roots
        for outcome in root.group.outcomes
        for potion in _potion_identity_change(
            root.context.potion_ids,
            outcome.final_potion_ids,
        )[1]
    )
    lost_potions = (
        ",".join(
            f"{potion}:{count}"
            for potion, count in sorted(potion_losses.items())
        )
        or "none"
    )
    gained_potions = (
        ",".join(
            f"{potion}:{count}"
            for potion, count in sorted(potion_gains.items())
        )
        or "none"
    )
    if print_completion:
        print(
            f"evaluation_complete=true wins={result.wins} losses={result.losses} "
            f"decision_rule={config.decision_rule.value} "
            f"potion_lane={result.potion_lane.value} "
            f"potion_slots={_potion_slots_text(result)} "
            f"root_wins={root_wins} root_final_hp_sums={root_final_hp} "
            f"root_enemy_final_hp_sums={root_enemy_final_hp} "
            f"root_enemy_final_hp_ranges={root_enemy_final_hp_ranges} "
            f"root_enemy_hp_signal_replicates={root_enemy_hp_signals} "
            f"root_potions_used={root_potions_used} "
            f"root_potions_discarded={root_potions_discarded} "
            f"root_resource_frontiers={root_resource_frontiers} "
            f"root_resource_dominated={root_resource_dominated} "
            f"root_sites={root_sites} root_start_potions={root_start_potions} "
            f"lost_potions={lost_potions} gained_potions={gained_potions} "
            f"seconds={elapsed:.3f} output={config.output}",
            flush=True,
        )
    return summary


def _potion_slots_text(result: CombatHeldOutEvaluationResult) -> str:
    if result.potion_lane is CombatPotionLane.ALL:
        return "all"
    if result.potion_lane is CombatPotionLane.NEVER:
        return "none"
    return "+".join(str(slot) for slot in result.potion_slots)


def _summary(
    config: CombatEvaluationCommandConfig,
    recovered: PublishedCombatBehavior | PublishedRunBehavior,
    result: CombatHeldOutEvaluationResult,
    *,
    root_audits: tuple[CombatRootAudit, ...],
    artifact_bytes: int,
    artifact_sha256: str,
    elapsed: float,
) -> dict[str, object]:
    combat_trained = isinstance(recovered, PublishedCombatBehavior)
    if len(root_audits) != len(result.roots):
        raise CombatEvaluationCommandError(
            "combat evaluation root audits are misaligned"
        )
    for audit, root in zip(root_audits, result.roots, strict=True):
        if (
            audit.seed != root.context.seed
            or audit.act != root.context.act
            or audit.floor != root.context.floor
            or audit.encounter_id != root.context.encounter_id
        ):
            raise CombatEvaluationCommandError(
                "combat evaluation root audit changed during evaluation"
            )
    roots = tuple(
        _root_summary(slot_index, root)
        for slot_index, root in enumerate(result.roots)
    )
    outcomes = tuple(
        outcome
        for root in result.roots
        for outcome in root.group.outcomes
    )
    potion_losses = Counter(
        potion
        for root in result.roots
        for outcome in root.group.outcomes
        for potion in _potion_identity_change(
            root.context.potion_ids,
            outcome.final_potion_ids,
        )[0]
    )
    potion_gains = Counter(
        potion
        for root in result.roots
        for outcome in root.group.outcomes
        for potion in _potion_identity_change(
            root.context.potion_ids,
            outcome.final_potion_ids,
        )[1]
    )
    resource_frontiers = tuple(
        combat_observed_resource_frontier(root.group.outcomes)
        for root in result.roots
    )
    return {
        "schema": COMBAT_EVALUATION_SCHEMA,
        "kind": "completed",
        "decision_rule": config.decision_rule.value,
        "evaluation_manifest_id": result.behavior_manifest_id.digest.hex(),
        "potion_lane": result.potion_lane.value,
        "potion_slots": result.potion_slots,
        "artifact": str(config.artifact),
        "artifact_sha256": artifact_sha256,
        "artifact_bytes": artifact_bytes,
        "behavior": str(config.behavior),
        "behavior_manifest_id": recovered.manifest_id.digest.hex(),
        "behavior_checkpoint_id": recovered.checkpoint_id.digest.hex(),
        "behavior_training_step": recovered.training_step,
        "behavior_training_kind": "combat" if combat_trained else "run",
        "behavior_training_root_count": (
            recovered.training_root_count if combat_trained else None
        ),
        "behavior_training_artifact_sha256": (
            recovered.training_artifact_sha256 if combat_trained else None
        ),
        "behavior_training_all_loss_axis": (
            recovered.training_all_loss_axis.name.lower()
            if combat_trained
            else None
        ),
        "behavior_training_potion_lane": recovered.training_potion_lane.value,
        "behavior_training_potion_slots": (
            recovered.training_potion_slots if combat_trained else ()
        ),
        "behavior_run_sampling_mode": (
            None if combat_trained else recovered.training_sampling_mode.value
        ),
        "behavior_run_episode_root_attempts": (
            None if combat_trained else recovered.training_episode_root_attempts
        ),
        "behavior_run_combat_decision_rule": (
            None
            if combat_trained
            else recovered.training_combat_decision_rule.value
        ),
        "behavior_run_combat_anchor_manifest_id": (
            None
            if combat_trained or recovered.combat_anchor_manifest_id is None
            else recovered.combat_anchor_manifest_id.digest.hex()
        ),
        "behavior_run_combat_anchor_checkpoint_id": (
            None
            if combat_trained or recovered.combat_anchor_checkpoint_id is None
            else recovered.combat_anchor_checkpoint_id.digest.hex()
        ),
        "behavior_run_combat_anchor_scorer": (
            None
            if combat_trained or recovered.combat_anchor_scorer is None
            else {
                "hidden_dim": recovered.combat_anchor_scorer.hidden_dim,
                "relation_layers": (
                    recovered.combat_anchor_scorer.relation_layers
                ),
                "value_head": recovered.combat_anchor_scorer.value_head,
                "value_head_width": (
                    recovered.combat_anchor_scorer.value_head_width
                ),
            }
        ),
        "behavior_run_critic_initialization_manifest_id": (
            None
            if combat_trained
            or recovered.critic_initialization_manifest_id is None
            else recovered.critic_initialization_manifest_id.digest.hex()
        ),
        "behavior_run_critic_initialization_checkpoint_id": (
            None
            if combat_trained
            or recovered.critic_initialization_checkpoint_id is None
            else recovered.critic_initialization_checkpoint_id.digest.hex()
        ),
        "behavior_run_critic_initialization_training_step": (
            None
            if combat_trained
            else recovered.critic_initialization_training_step
        ),
        "behavior_run_critic_initialization_policy_update": (
            None
            if combat_trained
            or recovered.critic_initialization_policy_update is None
            else recovered.critic_initialization_policy_update.name.lower()
        ),
        "behavior_run_objective": (
            None
            if combat_trained
            else {
                "attempts_per_update": recovered.objective.attempts_per_update,
                "advantage_mode": recovered.objective.advantage_mode.name.lower(),
                "decision_scope": recovered.objective.decision_scope.name.lower(),
                "policy_update": recovered.objective.policy_update.rule.name.lower(),
                "normalize_advantage": (
                    recovered.objective.policy_update.normalize_advantage
                ),
                "value_clip_coefficient": (
                    recovered.objective.policy_update.value_clip_coefficient
                ),
            }
        ),
        "root_count": config.root_count,
        "root_audits": tuple(audit.as_mapping() for audit in root_audits),
        "replicate_count": config.replicate_count,
        "decision_trace": {
            "file": (
                COMBAT_ACTION_TRACE_FILENAME
                if config.trace_replicates_per_root
                else None
            ),
            "record_count": sum(
                len(root.decision_traces) for root in result.roots
            ),
            "replicates_per_root": config.trace_replicates_per_root,
            "schema": COMBAT_ACTION_TRACE_SCHEMA,
        },
        "behavior_seed_scope": "root_replicate_independent",
        "behavior_seeds": config.behavior_seeds,
        "wins": result.wins,
        "losses": result.losses,
        "final_hp_sum": sum(outcome.final_hp for outcome in outcomes),
        "hp_loss_sum": sum(outcome.hp_loss for outcome in outcomes),
        "enemy_final_hp_sum": sum(
            outcome.enemy_final_hp for outcome in outcomes
        ),
        "enemy_final_hp_min": min(
            outcome.enemy_final_hp for outcome in outcomes
        ),
        "enemy_final_hp_max": max(
            outcome.enemy_final_hp for outcome in outcomes
        ),
        "enemy_hp_progress_signal_roots": sum(
            root["enemy_hp_progress_signal_replicates"] > 0 for root in roots
        ),
        "enemy_hp_progress_signal_replicates": sum(
            root["enemy_hp_progress_signal_replicates"] for root in roots
        ),
        "gold_delta_sum": sum(
            outcome.final_gold - root.context.gold
            for root in result.roots
            for outcome in root.group.outcomes
        ),
        "potions_used": sum(outcome.potions_used for outcome in outcomes),
        "potions_discarded": sum(
            outcome.potions_discarded for outcome in outcomes
        ),
        "lost_potion_ids": dict(sorted(potion_losses.items())),
        "gained_potion_ids": dict(sorted(potion_gains.items())),
        "observed_resource_frontier_replicates": sum(
            len(frontier.frontier_replicate_indices)
            for frontier in resource_frontiers
        ),
        "observed_resource_dominated_replicates": sum(
            len(frontier.dominated_replicate_indices)
            for frontier in resource_frontiers
        ),
        "observed_resource_incomparable_winning_pairs": sum(
            frontier.incomparable_pair_count
            for frontier in resource_frontiers
        ),
        "turns_sum": sum(outcome.turns for outcome in outcomes),
        "cards_played_sum": sum(outcome.cards_played for outcome in outcomes),
        "encounters": _encounter_summaries(result.roots),
        "roots": roots,
        "elapsed_seconds": elapsed,
    }


def _bind_replicate_rng_streams(
    policy: FrozenCategoricalTorchPolicy,
    seeds: tuple[int, ...],
) -> FrozenReplicateCategoricalTorchPolicy:
    if not seeds:
        raise CombatEvaluationCommandError(
            "combat evaluation requires replicate RNG seeds"
        )
    generators = (policy.generator,) + tuple(
        torch.Generator(device=policy.generator.device).manual_seed(seed)
        for seed in seeds[1:]
    )
    return FrozenReplicateCategoricalTorchPolicy.from_categorical(
        policy,
        generators,
    )


def _root_summary(
    slot_index: int,
    root: CombatEvaluationRootResult,
) -> dict[str, object]:
    outcomes = root.group.outcomes
    context = root.context
    enemy_hp_progress = root.group.grouped_advantages().enemy_hp_progress
    resource_frontier = combat_observed_resource_frontier(outcomes)
    return {
        "slot_index": slot_index,
        "root_id": root.group.root_id,
        "exact_combat_state_hash": root.group.exact_combat_state_hash,
        "wins": root.wins,
        "losses": root.losses,
        "model_rounds": root.model_rounds,
        "transitions": root.transitions,
        "context": {
            "seed": context.seed,
            "encounter_id": context.encounter_id,
            "monster_ids": context.monster_ids,
            "act": context.act,
            "floor": context.floor,
            "ascension_level": context.ascension_level,
            "turn": context.turn,
            "is_boss_fight": context.is_boss_fight,
            "is_elite_fight": context.is_elite_fight,
            "monster_count": context.monster_count,
            "living_monster_count": context.living_monster_count,
            "master_deck_card_count": context.master_deck_card_count,
            "relic_count": context.relic_count,
            "hand_card_count": context.hand_card_count,
            "hp": context.hp,
            "max_hp": context.max_hp,
            "gold": context.gold,
            "potion_slot_count": context.potion_slot_count,
            "filled_potion_count": context.filled_potion_count,
            "usable_potion_count": context.usable_potion_count,
            "potion_ids": context.potion_ids,
        },
        "final_hp_sum": sum(outcome.final_hp for outcome in outcomes),
        "hp_loss_sum": sum(outcome.hp_loss for outcome in outcomes),
        "enemy_start_hp": outcomes[0].enemy_start_hp,
        "enemy_final_hp_sum": sum(
            outcome.enemy_final_hp for outcome in outcomes
        ),
        "enemy_final_hp_min": min(
            outcome.enemy_final_hp for outcome in outcomes
        ),
        "enemy_final_hp_max": max(
            outcome.enemy_final_hp for outcome in outcomes
        ),
        "enemy_hp_progress_signal_replicates": sum(
            combat_advantage_has_signal(value) for value in enemy_hp_progress
        ),
        "potions_used": sum(outcome.potions_used for outcome in outcomes),
        "potions_discarded": sum(
            outcome.potions_discarded for outcome in outcomes
        ),
        "observed_resource_order": {
            "winning_replicate_indices": (
                resource_frontier.winning_replicate_indices
            ),
            "frontier_replicate_indices": (
                resource_frontier.frontier_replicate_indices
            ),
            "dominated_replicate_indices": (
                resource_frontier.dominated_replicate_indices
            ),
            "dominators_by_replicate": (
                resource_frontier.dominators_by_replicate
            ),
            "strict_order_pair_count": (
                resource_frontier.strict_order_pair_count
            ),
            "equivalent_pair_count": resource_frontier.equivalent_pair_count,
            "incomparable_pair_count": (
                resource_frontier.incomparable_pair_count
            ),
        },
        "outcomes": tuple(
            {
                "replicate_index": outcome.replicate_index,
                "terminal_kind": outcome.terminal_kind,
                "won": outcome.won,
                "final_hp": outcome.final_hp,
                "final_max_hp": outcome.final_max_hp,
                "final_gold": outcome.final_gold,
                "gold_delta": outcome.final_gold - context.gold,
                "hp_loss": outcome.hp_loss,
                "enemy_final_hp": outcome.enemy_final_hp,
                "turns": outcome.turns,
                "potions_used": outcome.potions_used,
                "potions_discarded": outcome.potions_discarded,
                "cards_played": outcome.cards_played,
                "final_potion_ids": outcome.final_potion_ids,
                "lost_potion_ids": _potion_identity_change(
                    context.potion_ids,
                    outcome.final_potion_ids,
                )[0],
                "gained_potion_ids": _potion_identity_change(
                    context.potion_ids,
                    outcome.final_potion_ids,
                )[1],
            }
            for outcome in outcomes
        ),
    }


def _write_decision_traces(
    output: Path,
    result: CombatHeldOutEvaluationResult,
) -> None:
    with (output / COMBAT_ACTION_TRACE_FILENAME).open(
        "x",
        encoding="utf-8",
        newline="\n",
    ) as destination:
        for root_slot_index, root in enumerate(result.roots):
            for decision in root.decision_traces:
                json.dump(
                    {
                        "schema": COMBAT_ACTION_TRACE_SCHEMA,
                        "root_slot_index": root_slot_index,
                        "root_id": root.group.root_id,
                        "exact_combat_state_hash": (
                            root.group.exact_combat_state_hash
                        ),
                        "decision": decision,
                    },
                    destination,
                    separators=(",", ":"),
                    sort_keys=True,
                )
                destination.write("\n")


def _encounter_summaries(
    roots: tuple[CombatEvaluationRootResult, ...],
) -> tuple[dict[str, object], ...]:
    grouped: dict[str, list[CombatEvaluationRootResult]] = {}
    for root in roots:
        grouped.setdefault(root.context.encounter_id, []).append(root)
    return tuple(
        _encounter_summary(encounter_id, tuple(encounter_roots))
        for encounter_id, encounter_roots in sorted(grouped.items())
    )


def _encounter_summary(
    encounter_id: str,
    roots: tuple[CombatEvaluationRootResult, ...],
) -> dict[str, object]:
    outcomes = tuple(
        outcome
        for root in roots
        for outcome in root.group.outcomes
    )
    monster_lineups = Counter(root.context.monster_ids for root in roots)
    return {
        "encounter_id": encounter_id,
        "root_count": len(roots),
        "replicate_count": len(outcomes),
        "wins": sum(outcome.won for outcome in outcomes),
        "losses": sum(not outcome.won for outcome in outcomes),
        "final_hp_sum": sum(outcome.final_hp for outcome in outcomes),
        "hp_loss_sum": sum(outcome.hp_loss for outcome in outcomes),
        "enemy_final_hp_sum": sum(outcome.enemy_final_hp for outcome in outcomes),
        "potions_used": sum(outcome.potions_used for outcome in outcomes),
        "potions_discarded": sum(
            outcome.potions_discarded for outcome in outcomes
        ),
        "monster_lineups": tuple(
            {
                "monster_ids": monster_ids,
                "root_count": count,
            }
            for monster_ids, count in sorted(monster_lineups.items())
        ),
    }


def _potion_identity_change(
    starting: tuple[str | None, ...],
    final: tuple[str | None, ...],
) -> tuple[tuple[str, ...], tuple[str, ...]]:
    retained = Counter(potion for potion in final if potion is not None)
    lost: list[str] = []
    for potion in starting:
        if potion is None:
            continue
        if retained[potion] > 0:
            retained[potion] -= 1
        else:
            lost.append(potion)
    existing = Counter(potion for potion in starting if potion is not None)
    gained: list[str] = []
    for potion in final:
        if potion is None:
            continue
        if existing[potion] > 0:
            existing[potion] -= 1
        else:
            gained.append(potion)
    return tuple(lost), tuple(gained)


def _positive(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized == 0:
        raise CombatEvaluationCommandError(f"{name} must be a positive integer")
    return normalized


def _nonnegative(value: object, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise CombatEvaluationCommandError(
            f"{name} must be a non-negative integer"
        )
    return value


def _seed(value: object, name: str) -> int:
    normalized = _nonnegative(value, name)
    if normalized >= 1 << 63:
        raise CombatEvaluationCommandError(
            f"{name} must be an integer in [0, 2^63)"
        )
    return normalized


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Evaluate one published frozen behavior on an opaque combat-root batch."
        ),
    )
    parser.add_argument("--artifact", type=Path, required=True)
    parser.add_argument("--behavior", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--roots", type=int, required=True)
    parser.add_argument("--replicates", type=int, default=8)
    parser.add_argument("--behavior-seed-base", type=int, default=10_000)
    parser.add_argument(
        "--decision-rule",
        choices=tuple(rule.value for rule in FrozenDecisionRule),
        default=FrozenDecisionRule.SAMPLED.value,
    )
    parser.add_argument("--trace-replicates-per-root", type=int, default=0)
    parser.add_argument(
        "--potion-lane",
        choices=tuple(lane.value for lane in CombatPotionLane),
        default=CombatPotionLane.ALL.value,
    )
    parser.add_argument("--potion-slot", action="append", type=int, default=[])
    return parser


def main() -> int:
    arguments = _parser().parse_args()
    run_combat_evaluation(
        CombatEvaluationCommandConfig(
            artifact=arguments.artifact,
            behavior=arguments.behavior,
            output=arguments.output,
            root_count=arguments.roots,
            replicate_count=arguments.replicates,
            behavior_seed_base=arguments.behavior_seed_base,
            decision_rule=FrozenDecisionRule(arguments.decision_rule),
            potion_lane=CombatPotionLane(arguments.potion_lane),
            potion_slots=tuple(arguments.potion_slot),
            trace_replicates_per_root=arguments.trace_replicates_per_root,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
