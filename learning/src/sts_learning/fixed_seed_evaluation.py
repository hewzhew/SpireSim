"""Fixed held-out panels: every planned seed/replica gets one outcome row."""
from __future__ import annotations

import dataclasses
import json
import math
from collections.abc import Callable
from dataclasses import dataclass

from .fixed_combat_environment import FixedCombatUnresolved
from .driver import partition_routine_decisions, merge_routine_ordinals
from .semantic_batch import select_semantic_decision_rows
from .outcomes import TerminalAttemptOutcome, TerminalStepBatch
from .policy import BatchPolicyChoice, DETERMINISTIC_SELECTION
from .run_resource_trace import RunPublicContext
from .seeds import SeedPartition, SeedPartitionSpec


@dataclass(frozen=True)
class FixedSeedPanel:
    seeds: tuple[int, ...]
    replicas: int = 2
    slots: int = 4
    max_steps: int = 512
    max_selection_rounds: int = 256
    partition: SeedPartitionSpec = SeedPartitionSpec()

    def __post_init__(self):
        object.__setattr__(self, "seeds", tuple(self.seeds))
        if not self.seeds or len(set(self.seeds)) != len(self.seeds):
            raise ValueError("panel seeds must be nonempty and unique")
        if any(type(seed) is not int or self.partition.classify(seed) is not SeedPartition.HELD_OUT for seed in self.seeds):
            raise ValueError("panel contains a non-held-out seed")
        for name in ("replicas", "slots", "max_steps", "max_selection_rounds"):
            if type(getattr(self, name)) is not int or getattr(self, name) <= 0:
                raise ValueError(f"{name} must be a positive integer")


@dataclass(frozen=True)
class FixedSeedOutcome:
    seed: int
    replica: int
    outcome: TerminalAttemptOutcome | None
    censored_reason: str | None
    combat_fallbacks: int | None = None
    fallback_terminal_zero_hp: bool | None = None


def evaluate_fixed_seed_panel(env_factory: Callable, policy_factory: Callable,
                              panel: FixedSeedPanel, progress: Callable | None = None,
                              evidence_sink: Callable[[dict[str, object]], None] | None = None,
                              *, controller_factory: Callable | None = None):
    """No reset, retry, optimizer, or model selection is performed here.

    policy_factory receives exact cohort seeds and replica; callers bind their
    model and per-seed RNG streams. Unknown rolls back the batch, so all its
    remaining active slots are explicitly censored; completed rows are retained.
    """
    rows = []
    for replica in range(panel.replicas):
        for offset in range(0, len(panel.seeds), panel.slots):
            seeds = panel.seeds[offset:offset+panel.slots]
            env = env_factory(list(seeds))
            if env.slot_count != len(seeds) or env.terminal_count:
                raise ValueError("fixed panel requires fresh nonterminal slots")
            controller = controller_factory(env) if controller_factory is not None else None
            policy = policy_factory(seeds, replica) if controller is None else controller
            identity = getattr(policy, "behavior_manifest_id", None)
            completed = {}
            fallback_counts = [0] * len(seeds)
            fallback_terminal_deaths = [False] * len(seeds)
            has_executor_evidence = hasattr(env, "last_combat_fallbacks")
            reason = "step_limit"
            for _ in range(panel.max_steps):
                if len(completed) == len(seeds):
                    break
                for _ in range(panel.max_selection_rounds if controller is None else 0):
                    if env.ready:
                        break
                    source_batch = env.decision_batch(semantic=True)
                    batch, learned_rows, routines = partition_routine_decisions(env, source_batch)
                    decisions = []
                    ordinals = ()
                    if batch is not None:
                        choice = policy.choose(batch)
                        if not isinstance(choice, BatchPolicyChoice) or choice.behavior_manifest_id != identity or policy.behavior_manifest_id != identity:
                            raise ValueError("panel policy choice has invalid identity")
                        if len(choice.ordinals) != len(batch["slot_indices"]):
                            raise ValueError("panel policy choice has incorrect row count")
                        ordinals = choice.ordinals
                        if evidence_sink:
                            predictor = getattr(policy, "predict_row_values", None)
                            values = predictor(batch) if callable(predictor) else None
                            distribution = getattr(policy, "candidate_probability_rows", None)
                            probabilities = distribution(batch) if callable(distribution) else None
                            decisions.extend({**event, "owner": "model"} for event in _public_choices(
                                env, batch, choice, seeds, replica, values, probabilities))
                    if routines and evidence_sink:
                        routine_rows = tuple(sorted(routines))
                        routine_batch = select_semantic_decision_rows(source_batch, routine_rows)
                        routine_choice = BatchPolicyChoice.create(tuple(routines[row] for row in routine_rows),
                            identity, tuple(DETERMINISTIC_SELECTION for _ in routine_rows))
                        probabilities = tuple(tuple(float(index == routines[row]) for index in range(
                            int(source_batch["candidate_counts"][row]))) for row in routine_rows)
                        decisions.extend({**event, "owner": "routine"} for event in _public_choices(
                            env, routine_batch, routine_choice, seeds, replica, candidate_probabilities=probabilities))
                    env.choose(list(source_batch["decision_token_ids"]),
                               merge_routine_ordinals(source_batch, learned_rows, routines, ordinals))
                    if evidence_sink:
                        for decision in decisions:
                            evidence_sink(decision)
                if controller is None and not env.ready:
                    reason = "selection_limit"
                    break
                before = _public_contexts(env, seeds) if evidence_sink else {}
                try:
                    if controller is None:
                        result = env.step()
                    else:
                        fixed, decisions, before, after = controller.advance(
                            max_selection_rounds=panel.max_selection_rounds)
                        if not fixed.advanced:
                            raise FixedCombatUnresolved(fixed)
                        if evidence_sink:
                            for decision in decisions:
                                choice = BatchPolicyChoice.create((decision["ordinal"],), identity,
                                                                  (DETERMINISTIC_SELECTION,))
                                audit = decision["audit"]
                                probabilities = (tuple(float(index == decision["ordinal"])
                                    for index in range(len(audit["candidates"])))
                                    if audit is not None else (),)
                                contexts = {decision["slot"]: RunPublicContext(**decision["context"])}
                                for event in _public_choices(env,
                                        {"slot_indices": [decision["slot"]]}, choice, seeds, replica,
                                        candidate_probabilities=probabilities,
                                        contexts=contexts, audits=[audit]):
                                    evidence_sink(event)
                        result = fixed.transitions
                except FixedCombatUnresolved:
                    reason = "executor_unresolved_batch"
                    break
                if has_executor_evidence:
                    step_fallbacks = env.last_combat_fallbacks
                    if step_fallbacks is None:
                        raise ValueError("fixed panel advanced without committed executor evidence")
                    for slot, terminal_zero_hp in step_fallbacks:
                        if type(slot) is not int or not 0 <= slot < len(seeds):
                            raise ValueError("fixed panel executor evidence has invalid slot")
                        fallback_counts[slot] += 1
                        fallback_terminal_deaths[slot] |= terminal_zero_hp
                if evidence_sink:
                    after = _public_contexts(env, seeds)
                    for slot in result["slot_indices"]:
                        slot = int(slot)
                        evidence_sink({"kind": "resource_step", "seed": seeds[slot],
                                       "replica": replica, "before": dataclasses.asdict(before[slot]),
                                       "after": dataclasses.asdict(after[slot])})
                for outcome in TerminalStepBatch.from_bridge_step(result, slot_count=len(seeds)).attempts:
                    if outcome.slot_index in completed:
                        raise ValueError("panel received a duplicate terminal slot")
                    completed[outcome.slot_index] = outcome
            rows.extend(FixedSeedOutcome(seed, replica, completed.get(slot),
                                        None if slot in completed else reason,
                                        fallback_counts[slot] if has_executor_evidence else None,
                                        fallback_terminal_deaths[slot] if has_executor_evidence else None)
                        for slot, seed in enumerate(seeds))
            if progress is not None:
                progress(tuple(rows))
    return tuple(rows)


def _public_contexts(env, seeds):
    source = getattr(env, "public_run_contexts", None)
    if not callable(source):
        raise ValueError("fixed panel evidence requires public_run_contexts")
    rows = tuple(RunPublicContext.from_bridge_row(row) for row in source())
    by_slot = {row.slot_index: row for row in rows}
    if len(by_slot) != len(rows) or set(by_slot) != set(range(len(seeds))):
        raise ValueError("fixed panel public contexts do not cover the cohort")
    if any(row.seed != seeds[slot] for slot, row in by_slot.items()):
        raise ValueError("fixed panel public context seed changed")
    return by_slot


def _public_choices(env, batch, choice, seeds, replica, predicted_values=None,
                    candidate_probabilities=None, *, contexts=None, audits=None):
    source = getattr(env, "strategic_decision_audit_json", None)
    if audits is None and not callable(source):
        raise ValueError("fixed panel evidence requires strategic_decision_audit_json")
    contexts = _public_contexts(env, seeds) if contexts is None else contexts
    if predicted_values is not None:
        predicted_values = tuple(predicted_values)
        if len(predicted_values) != len(choice.ordinals) or any(
            isinstance(value, bool) or not isinstance(value, (float, int))
            or not math.isfinite(value) for value in predicted_values
        ):
            raise ValueError("panel value predictions must be finite and cover each decision row")
    if candidate_probabilities is not None:
        candidate_probabilities = tuple(tuple(row) for row in candidate_probabilities)
        if len(candidate_probabilities) != len(choice.ordinals):
            raise ValueError("panel candidate probabilities must cover each decision row")
    events = []
    for row_index, (slot, ordinal) in enumerate(zip(batch["slot_indices"], choice.ordinals, strict=True)):
        slot = int(slot)
        if audits is None:
            audit_json = source(slot)
            audit = None if audit_json is None else json.loads(audit_json)
        else:
            audit = audits[row_index]
        if audit is None:
            continue
        if not 0 <= ordinal < len(audit["candidates"]):
            raise ValueError("fixed panel audit candidate count disagrees with policy")
        probabilities = None
        if candidate_probabilities is not None:
            probabilities = candidate_probabilities[row_index]
            if (len(probabilities) != len(audit["candidates"])
                    or any(isinstance(value, bool) or not isinstance(value, (float, int))
                           or not math.isfinite(value) or not 0.0 <= value <= 1.0
                           for value in probabilities)
                    or not math.isclose(sum(probabilities), 1.0, rel_tol=0.0, abs_tol=1e-9)):
                raise ValueError("panel candidate probabilities must align with public candidates")
            selected = choice.selection_probabilities[row_index].value
            if selected is not None and not math.isclose(
                    probabilities[ordinal], selected, rel_tol=1e-8, abs_tol=1e-12):
                raise ValueError("panel candidate probabilities disagree with sampled choice")
        event = {"kind": "strategic_choice", "seed": seeds[slot],
                 "replica": replica, "site": audit["decision_site"],
                 "candidate_ordinal": int(ordinal),
                 "candidate": audit["candidates"][ordinal],
                 "public_context": dataclasses.asdict(contexts[slot])}
        if predicted_values is not None:
            event["predicted_return"] = float(predicted_values[row_index])
        if audit["decision_site"] in ("map", "campfire"):
            if audit.get("schema") != "sts-learning-strategic-decision-audit-v2":
                raise ValueError("fixed panel route/campfire evidence requires v2 public loadout audit")
            deck = audit.get("public_deck")
            relics = audit.get("public_relics")
            if (not isinstance(deck, list) or not isinstance(relics, list)
                    or any(not isinstance(card, dict) or "card" not in card
                           or "card_lineage" in card for card in deck)
                    or any(not isinstance(relic, dict) or "relic" not in relic
                           for relic in relics)):
                raise ValueError("fixed panel public loadout is malformed")
            event["public_deck"] = deck
            event["public_relics"] = relics
        summaries = None
        if audit["decision_site"] == "map":
            summaries = audit.get("route_summaries")
            if not isinstance(summaries, list) or len(summaries) != len(audit["candidates"]):
                raise ValueError("fixed panel map audit route summaries are misaligned")
        event["available_candidates"] = [
            {"ordinal": index, "candidate": candidate,
             **({"probability": float(probabilities[index])} if probabilities is not None else {}),
             **({"route_summary": summaries[index]} if summaries is not None else {})}
            for index, candidate in enumerate(audit["candidates"])
        ]
        events.append(event)
    return tuple(events)


def panel_summary(rows):
    completed = [row.outcome for row in rows if row.outcome is not None]
    return {"episodes": len(rows), "completed": len(completed),
            "censored": len(rows)-len(completed),
            "wins": sum(row.terminal_reward > 0 for row in completed),
            "completed_reached_act2": sum(row.terminal_act >= 2 for row in completed),
            "completed_died_act1": sum(row.terminal_act == 1 and row.terminal_reward <= 0
                                       for row in completed),
            "mean_terminal_floor": sum(row.terminal_floor for row in completed)/len(completed) if completed else None,
            "completed_with_fallback": sum(row.outcome is not None and (row.combat_fallbacks or 0) > 0 for row in rows),
            "completed_by_fallback_death": sum(row.outcome is not None and row.fallback_terminal_zero_hp is True for row in rows),
            "completed_with_unknown_executor": sum(row.outcome is not None and row.combat_fallbacks is None for row in rows)}


def paired_seed_floor_deltas(initial, final, *, require_fallback_free=False):
    """Replica-averaged pairs; never silently drop censored or missing cases."""
    left = {(row.seed, row.replica): row for row in initial}
    right = {(row.seed, row.replica): row for row in final}
    if len(left) != len(initial) or len(right) != len(final) or left.keys() != right.keys():
        raise ValueError("paired panels must have identical unique seed/replica coverage")
    if any(row.outcome is None for row in (*initial, *final)):
        return None
    if require_fallback_free and any(row.combat_fallbacks != 0 for row in (*initial, *final)):
        return None
    seeds = sorted({seed for seed, _ in left})
    return {seed: sum(right[key].outcome.terminal_floor-left[key].outcome.terminal_floor
                      for key in left if key[0] == seed)/sum(key[0] == seed for key in left)
            for seed in seeds}
