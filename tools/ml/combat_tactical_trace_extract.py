#!/usr/bin/env python3
"""Export CombatTacticalEpisodeV1 records from turn-plan guidance-lab reports.

This is a diagnostic/learning handoff, not a policy script.  It keeps the
simulator report as the fact source, derives tactical deltas deterministically,
and records counterfactual fields only relative to the candidate set available
in the same root report.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from collections import Counter
from pathlib import Path
from typing import Any, Iterable


EPISODE_SCHEMA = "CombatTacticalEpisodeV1"
EPISODE_VERSION = 2
LABEL_ROLE = "diagnostic_tactical_trace_not_policy_label"
EXTRACTOR_ID = "combat_tactical_trace_extract_v1"
PUBLIC_MONSTER_FIELDS = (
    "slot",
    "enemy_id",
    "hp",
    "max_hp",
    "block",
    "alive",
    "escaped",
    "dying",
    "half_dead",
    "visible_intent",
    "preview_damage_per_hit",
)


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def current_git_commit() -> str | None:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    commit = result.stdout.strip()
    return commit or None


def hash_json(value: Any) -> str:
    payload = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode(
        "utf-8"
    )
    return hashlib.blake2b(payload, digest_size=32).hexdigest()


def iter_labs(path: Path, payload: Any) -> Iterable[tuple[dict[str, Any], dict[str, Any]]]:
    if not isinstance(payload, dict):
        return
    schema = payload.get("schema_name")
    if schema == "CombatTurnPlanGuidanceLabV1Report":
        yield (
            {
                "source_file": str(path),
                "benchmark_name": None,
                "case_id": None,
                "input_kind": None,
                "input_path": None,
            },
            payload,
        )
        return
    if schema == "CombatTurnPlanGuidanceLabBenchmarkV1Report":
        benchmark_name = payload.get("benchmark_name")
        for case in payload.get("cases", []):
            if not isinstance(case, dict) or not isinstance(case.get("lab"), dict):
                continue
            yield (
                {
                    "source_file": str(path),
                    "benchmark_name": benchmark_name,
                    "case_id": case.get("id"),
                    "input_kind": case.get("input_kind"),
                    "input_path": case.get("input_path"),
                },
                case["lab"],
            )


def resolve_input_path(report_path: Path, input_path: Any) -> Path | None:
    if not isinstance(input_path, str) or not input_path:
        return None
    path = Path(input_path)
    if path.exists() or path.is_absolute():
        return path
    candidate = report_path.parent / path
    if candidate.exists():
        return candidate
    return path


def public_enemy_slots_from_capture(path: Path | None) -> list[dict[str, Any]]:
    if path is None or not path.exists():
        return []
    try:
        payload = load_json(path)
    except (OSError, json.JSONDecodeError):
        return []
    summary = payload.get("summary") if isinstance(payload, dict) else {}
    monsters = summary.get("monsters") if isinstance(summary, dict) else []
    if not isinstance(monsters, list):
        return []
    out: list[dict[str, Any]] = []
    for monster in monsters:
        if not isinstance(monster, dict):
            continue
        public = {field: monster.get(field) for field in PUBLIC_MONSTER_FIELDS if field in monster}
        if public:
            out.append(public)
    return out


def as_dict(value: Any) -> dict[str, Any]:
    return value if isinstance(value, dict) else {}


def as_list(value: Any) -> list[Any]:
    return value if isinstance(value, list) else []


def int_value(value: Any, default: int = 0) -> int:
    return value if isinstance(value, int) else default


def int_or_none(value: Any) -> int | None:
    return value if isinstance(value, int) else None


def bool_value(value: Any) -> bool:
    return value if isinstance(value, bool) else False


def target_sort_key(candidate: dict[str, Any]) -> tuple[int, int, int, int]:
    target = as_dict(candidate.get("target"))
    terminal = target.get("terminal")
    tier = 0
    if target.get("complete_win") and terminal == "win":
        tier = 3
    elif terminal == "win":
        tier = 2
    elif terminal == "unresolved":
        tier = 1
    final_hp = int_value(target.get("final_hp"), -10**9)
    child_hp_loss = int_value(target.get("child_search_hp_loss"), 10**9)
    nodes = int_value(target.get("nodes_expanded"), 10**9)
    return (tier, final_hp, -child_hp_loss, -nodes)


def plan_index(candidate: dict[str, Any]) -> int:
    return int_value(as_dict(candidate.get("plan")).get("plan_index"), 10**9)


def state_delta(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    fields = (
        "player_hp",
        "player_block",
        "energy",
        "turn_count",
        "living_enemy_count",
        "total_enemy_hp",
        "visible_incoming_damage",
        "hand_count",
        "draw_count",
        "discard_count",
        "exhaust_count",
        "limbo_count",
        "queued_cards_count",
    )
    return {
        field: int_value(after.get(field)) - int_value(before.get(field))
        for field in fields
        if isinstance(before.get(field), int) and isinstance(after.get(field), int)
    }


def enemy_slots_by_slot(state: dict[str, Any]) -> dict[int, dict[str, Any]]:
    slots: dict[int, dict[str, Any]] = {}
    for enemy in as_list(state.get("enemy_slots")):
        if not isinstance(enemy, dict):
            continue
        slot = enemy.get("slot")
        if isinstance(slot, int):
            slots[slot] = enemy
    return slots


def enemy_slot_deltas(before: dict[str, Any], after: dict[str, Any]) -> list[dict[str, Any]]:
    before_slots = enemy_slots_by_slot(before)
    after_slots = enemy_slots_by_slot(after)
    deltas: list[dict[str, Any]] = []
    for slot in sorted(set(before_slots) | set(after_slots)):
        before_enemy = before_slots.get(slot, {})
        after_enemy = after_slots.get(slot, {})
        hp_before = int_or_none(before_enemy.get("hp"))
        hp_after = int_or_none(after_enemy.get("hp"))
        block_before = int_or_none(before_enemy.get("block"))
        block_after = int_or_none(after_enemy.get("block"))
        incoming_before = int_or_none(before_enemy.get("visible_incoming_damage"))
        incoming_after = int_or_none(after_enemy.get("visible_incoming_damage"))
        alive_before = before_enemy.get("alive") if isinstance(before_enemy.get("alive"), bool) else None
        alive_after = after_enemy.get("alive") if isinstance(after_enemy.get("alive"), bool) else None
        killed = alive_before is True and alive_after is False
        hp_delta = hp_after - hp_before if hp_before is not None and hp_after is not None else None
        block_delta = (
            block_after - block_before
            if block_before is not None and block_after is not None
            else None
        )
        incoming_delta = (
            incoming_after - incoming_before
            if incoming_before is not None and incoming_after is not None
            else None
        )
        deltas.append(
            {
                "slot": slot,
                "enemy_id_before": before_enemy.get("enemy_id"),
                "enemy_id_after": after_enemy.get("enemy_id"),
                "alive_before": alive_before,
                "alive_after": alive_after,
                "killed": killed,
                "hp_before": hp_before,
                "hp_after": hp_after,
                "hp_delta": hp_delta,
                "hp_removed": max(0, -hp_delta) if isinstance(hp_delta, int) else None,
                "block_before": block_before,
                "block_after": block_after,
                "block_delta": block_delta,
                "incoming_before": incoming_before,
                "incoming_after": incoming_after,
                "incoming_delta": incoming_delta,
                "incoming_removed": max(0, -incoming_delta)
                if isinstance(incoming_delta, int)
                else None,
                "incoming_removed_by_kill": incoming_before
                if killed and isinstance(incoming_before, int)
                else 0,
            }
        )
    return deltas


def enemy_slot_delta_summary(deltas: list[dict[str, Any]]) -> dict[str, Any]:
    if not deltas:
        return {
            "enemy_slot_delta_available": False,
            "killed_enemy_slots": [],
            "killed_enemy_count": None,
            "enemy_hp_removed_by_slot": None,
            "visible_incoming_removed_by_slot": None,
            "visible_incoming_removed_by_kill": None,
        }
    killed_slots = [delta["slot"] for delta in deltas if delta.get("killed") is True]
    hp_removed = sum(
        int_value(delta.get("hp_removed")) for delta in deltas if delta.get("hp_removed") is not None
    )
    incoming_removed = sum(
        int_value(delta.get("incoming_removed"))
        for delta in deltas
        if delta.get("incoming_removed") is not None
    )
    incoming_removed_by_kill = sum(
        int_value(delta.get("incoming_removed_by_kill")) for delta in deltas
    )
    return {
        "enemy_slot_delta_available": bool(deltas),
        "killed_enemy_slots": killed_slots,
        "killed_enemy_count": len(killed_slots),
        "enemy_hp_removed_by_slot": hp_removed,
        "visible_incoming_removed_by_slot": incoming_removed,
        "visible_incoming_removed_by_kill": incoming_removed_by_kill,
    }


def tactical_event(
    *,
    kind: str,
    step_index: Any,
    targets: list[dict[str, Any]] | None = None,
    magnitude: dict[str, Any] | None = None,
    evidence: list[str] | None = None,
) -> dict[str, Any]:
    return {
        "data_role": "DerivedDeterministic",
        "availability": "AfterStep",
        "kind": kind,
        "actor_step_index": step_index,
        "targets": targets or [],
        "magnitude": magnitude or {},
        "evidence": evidence or [],
    }


def tactical_events_from_delta(
    *,
    step_index: Any,
    facts: dict[str, Any] | None,
    delta: dict[str, Any],
) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    enemy_delta = as_dict(delta.get("enemy_delta"))
    threat_delta = as_dict(delta.get("threat_delta"))
    player_delta = as_dict(delta.get("player_delta"))
    resource_delta = as_dict(delta.get("resource_delta"))
    slot_deltas = [item for item in as_list(enemy_delta.get("slot_deltas")) if isinstance(item, dict)]
    damaged_slots = [
        item
        for item in slot_deltas
        if isinstance(item.get("hp_removed"), int) and item["hp_removed"] > 0
    ]
    if damaged_slots:
        events.append(
            tactical_event(
                kind="TargetFocus",
                step_index=step_index,
                targets=[{"enemy_slot": item.get("slot")} for item in damaged_slots],
                magnitude={
                    "enemy_hp_removed": sum(int_value(item.get("hp_removed")) for item in damaged_slots),
                    "damaged_enemy_slots": [item.get("slot") for item in damaged_slots],
                },
                evidence=["tactical_delta.enemy_delta.slot_deltas"],
            )
        )
    killed_slots = [
        item.get("slot") for item in slot_deltas if item.get("killed") is True
    ]
    if killed_slots:
        events.append(
            tactical_event(
                kind="KillWindow",
                step_index=step_index,
                targets=[{"enemy_slot": slot} for slot in killed_slots],
                magnitude={"killed_enemy_count": len(killed_slots)},
                evidence=["tactical_delta.enemy_delta.slot_deltas.killed"],
            )
        )
    incoming_removed = int_or_none(threat_delta.get("incoming_removed"))
    incoming_removed_by_kill = int_or_none(threat_delta.get("incoming_removed_by_kill"))
    if (incoming_removed and incoming_removed > 0) or (
        incoming_removed_by_kill and incoming_removed_by_kill > 0
    ):
        events.append(
            tactical_event(
                kind="ThreatRemoval",
                step_index=step_index,
                targets=[{"enemy_slot": slot} for slot in killed_slots],
                magnitude={
                    "incoming_removed": incoming_removed,
                    "incoming_removed_by_kill": incoming_removed_by_kill,
                },
                evidence=[
                    "tactical_delta.threat_delta.incoming_removed",
                    "tactical_delta.threat_delta.incoming_removed_by_kill",
                ],
            )
        )
    incoming_before = int_or_none(threat_delta.get("incoming_before"))
    hp_lost = int_or_none(player_delta.get("hp_lost"))
    block_delta = int_or_none(player_delta.get("block_delta"))
    if incoming_before is not None and incoming_before > 0:
        events.append(
            tactical_event(
                kind="DefenseCoverage",
                step_index=step_index,
                magnitude={
                    "incoming_before": incoming_before,
                    "hp_lost": hp_lost,
                    "block_delta": block_delta,
                    "covered_without_hp_loss": hp_lost == 0,
                },
                evidence=[
                    "tactical_delta.threat_delta.incoming_before",
                    "tactical_delta.player_delta.hp_lost",
                ],
            )
        )
    if enemy_delta.get("all_enemies_dead_after_step") is True:
        events.append(
            tactical_event(
                kind="LethalWindow",
                step_index=step_index,
                magnitude={"all_enemies_dead": True},
                evidence=["tactical_delta.enemy_delta.living_enemy_count_delta"],
            )
        )
    if isinstance(facts, dict) and facts.get("action_kind") in ("use_potion", "discard_potion"):
        events.append(
            tactical_event(
                kind="ResourceTiming",
                step_index=step_index,
                magnitude={"resource_kind": facts.get("action_kind")},
                evidence=["action_facts.action_kind"],
            )
        )
    energy_delta = int_or_none(resource_delta.get("energy_delta"))
    if energy_delta is not None and energy_delta < 0:
        events.append(
            tactical_event(
                kind="TempoCost",
                step_index=step_index,
                magnitude={"energy_spent_estimate": -energy_delta},
                evidence=["tactical_delta.resource_delta.energy_delta"],
            )
        )
    return events


def step_tactical_delta(
    facts: dict[str, Any] | None,
    state_before: dict[str, Any] | None,
    state_after: dict[str, Any] | None,
) -> dict[str, Any]:
    exact = as_dict(facts.get("exact_one_step_delta")) if isinstance(facts, dict) else {}
    immediate = as_dict(facts.get("immediate")) if isinstance(facts, dict) else {}
    mechanics = as_dict(facts.get("mechanics")) if isinstance(facts, dict) else {}
    has_state_summary = isinstance(state_before, dict) and isinstance(state_after, dict)
    summary_delta = state_delta(state_before or {}, state_after or {}) if has_state_summary else {}

    hp_delta = summary_delta.get("player_hp", int_or_none(exact.get("player_hp_delta")))
    block_delta = summary_delta.get("player_block", int_or_none(exact.get("player_block_delta")))
    energy_delta = summary_delta.get("energy", int_or_none(exact.get("energy_delta")))
    total_enemy_hp_delta = summary_delta.get(
        "total_enemy_hp",
        int_or_none(exact.get("total_enemy_hp_delta")),
    )
    living_enemy_delta = summary_delta.get("living_enemy_count")
    incoming_before = (
        int_or_none(state_before.get("visible_incoming_damage"))
        if isinstance(state_before, dict)
        else None
    )
    incoming_after = (
        int_or_none(state_after.get("visible_incoming_damage"))
        if isinstance(state_after, dict)
        else None
    )
    incoming_delta = summary_delta.get("visible_incoming_damage")
    slot_deltas = (
        enemy_slot_deltas(state_before, state_after)
        if isinstance(state_before, dict) and isinstance(state_after, dict)
        else []
    )
    slot_summary = enemy_slot_delta_summary(slot_deltas)

    return {
        "data_role": "DerivedDeterministic" if exact or summary_delta else "Unavailable",
        "availability": "AfterStep" if exact or summary_delta else "not_recorded",
        "exact_one_step_delta": exact or None,
        "state_summary_delta": summary_delta or None,
        "player_delta": {
            "hp_delta": hp_delta,
            "hp_lost": max(0, -hp_delta) if isinstance(hp_delta, int) else None,
            "block_delta": block_delta,
        },
        "resource_delta": {
            "energy_delta": energy_delta,
            "hand_delta": summary_delta.get("hand_count", int_or_none(exact.get("hand_delta"))),
            "draw_delta": summary_delta.get("draw_count", int_or_none(exact.get("draw_delta"))),
            "discard_delta": summary_delta.get(
                "discard_count",
                int_or_none(exact.get("discard_delta")),
            ),
            "exhaust_delta": summary_delta.get(
                "exhaust_count",
                int_or_none(exact.get("exhaust_delta")),
            ),
            "limbo_delta": summary_delta.get("limbo_count", int_or_none(exact.get("limbo_delta"))),
            "queued_cards_delta": summary_delta.get(
                "queued_cards_count",
                int_or_none(exact.get("queued_cards_delta")),
            ),
        },
        "enemy_delta": {
            "total_hp_delta": total_enemy_hp_delta,
            "total_hp_removed": max(0, -total_enemy_hp_delta)
            if isinstance(total_enemy_hp_delta, int)
            else None,
            "total_block_delta": int_or_none(exact.get("total_enemy_block_delta")),
            "living_enemy_count_delta": living_enemy_delta,
            "enemy_kill_count_estimate": max(0, -living_enemy_delta)
            if isinstance(living_enemy_delta, int)
            else None,
            "all_enemies_dead_after_step": int_or_none(state_after.get("living_enemy_count")) == 0
            if isinstance(state_after, dict)
            else None,
            "slot_deltas": slot_deltas or None,
            **slot_summary,
        },
        "threat_delta": {
            "incoming_before": incoming_before,
            "incoming_after": incoming_after,
            "incoming_delta": incoming_delta,
            "incoming_removed": max(0, -incoming_delta) if isinstance(incoming_delta, int) else None,
            "incoming_removed_by_kill": slot_summary.get("visible_incoming_removed_by_kill"),
            "visible_attack_mitigation_hint": int_value(
                mechanics.get("visible_attack_mitigation_hint")
            ),
        },
        "action_effect_hints": {
            "damage_hint": int_value(immediate.get("action_payload_damage_hint")),
            "block_hint": int_value(immediate.get("block_hint")),
            "target_progress_hint": int_value(immediate.get("target_progress_hint")),
            "all_enemy_progress_hint": int_value(immediate.get("all_enemy_progress_hint")),
        },
    }


def sum_exact_deltas(action_facts: list[dict[str, Any]]) -> dict[str, int]:
    fields = (
        "player_hp_delta",
        "player_block_delta",
        "energy_delta",
        "hand_delta",
        "draw_delta",
        "discard_delta",
        "exhaust_delta",
        "limbo_delta",
        "queued_cards_delta",
        "total_enemy_hp_delta",
        "total_enemy_block_delta",
    )
    out = {field: 0 for field in fields}
    for facts in action_facts:
        exact = as_dict(facts.get("exact_one_step_delta"))
        for field in fields:
            out[field] += int_value(exact.get(field))
    return out


def action_kind_counts(action_facts: list[dict[str, Any]], actions: list[dict[str, Any]]) -> Counter[str]:
    counts: Counter[str] = Counter()
    for index, action in enumerate(actions):
        facts = action_facts[index] if index < len(action_facts) else {}
        kind = facts.get("action_kind") or action_kind_from_key(str(action.get("action_key") or ""))
        counts[str(kind)] += 1
    return counts


def action_kind_from_key(action_key: str) -> str:
    if "/play_card/" in action_key or action_key.startswith("combat/play_card"):
        return "play_card"
    if "/use_potion/" in action_key or action_key.startswith("combat/use_potion"):
        return "use_potion"
    if "/discard_potion/" in action_key or action_key.startswith("combat/discard_potion"):
        return "discard_potion"
    if action_key.endswith("/end_turn") or action_key == "combat/end_turn":
        return "end_turn"
    return "unknown"


def is_low_impact_exhaust_action(facts: dict[str, Any]) -> bool:
    immediate = as_dict(facts.get("immediate"))
    mechanics = as_dict(facts.get("mechanics"))
    exact = as_dict(facts.get("exact_one_step_delta"))
    return (
        bool_value(immediate.get("exhausts_card"))
        and int_value(immediate.get("damage_hint")) <= 0
        and int_value(immediate.get("action_payload_damage_hint")) <= 0
        and int_value(immediate.get("block_hint")) <= 0
        and int_value(immediate.get("target_progress_hint")) <= 0
        and int_value(immediate.get("all_enemy_progress_hint")) <= 0
        and int_value(mechanics.get("visible_attack_mitigation_hint")) <= 0
        and int_value(mechanics.get("persistent_enemy_strength_down")) <= 0
        and int_value(mechanics.get("temporary_enemy_strength_down")) <= 0
        and int_value(mechanics.get("enemy_vulnerable")) <= 0
        and int_value(mechanics.get("enemy_weak")) <= 0
        and int_value(mechanics.get("player_strength_gain")) <= 0
        and int_value(mechanics.get("player_temporary_strength_gain")) <= 0
        and int_value(exact.get("energy_delta")) <= 0
        and int_value(exact.get("hand_delta")) <= 0
    )


def plan_tactical_summary(
    root_state: dict[str, Any],
    plan: dict[str, Any],
    action_facts: list[dict[str, Any]],
) -> dict[str, Any]:
    end_state = as_dict(plan.get("end_state"))
    root_to_end = state_delta(root_state, end_state)
    exact_sums = sum_exact_deltas(action_facts)
    actions = [action for action in as_list(plan.get("actions")) if isinstance(action, dict)]
    counts = action_kind_counts(action_facts, actions)
    player_hp_delta = root_to_end.get("player_hp", exact_sums.get("player_hp_delta", 0))
    total_enemy_hp_delta = root_to_end.get(
        "total_enemy_hp",
        exact_sums.get("total_enemy_hp_delta", 0),
    )
    living_enemy_delta = root_to_end.get("living_enemy_count", 0)
    slot_deltas = enemy_slot_deltas(root_state, end_state) if end_state else []
    slot_summary = enemy_slot_delta_summary(slot_deltas)
    target_slots = []
    damage_hint_total = 0
    block_hint_total = 0
    mitigation_hint_total = 0
    exhaust_action_count = 0
    low_impact_exhaust_action_count = 0
    low_impact_exhaust_cards = []
    for facts in action_facts:
        target = as_dict(facts.get("target"))
        if isinstance(target.get("target_slot"), int):
            target_slots.append(target["target_slot"])
        immediate = as_dict(facts.get("immediate"))
        mechanics = as_dict(facts.get("mechanics"))
        damage_hint_total += int_value(immediate.get("action_payload_damage_hint"))
        block_hint_total += int_value(immediate.get("block_hint"))
        mitigation_hint_total += int_value(mechanics.get("visible_attack_mitigation_hint"))
        if bool_value(immediate.get("exhausts_card")):
            exhaust_action_count += 1
        if is_low_impact_exhaust_action(facts):
            low_impact_exhaust_action_count += 1
            card = as_dict(facts.get("card"))
            low_impact_exhaust_cards.append(card.get("card_id") or card.get("name") or "unknown")
    return {
        "data_role": "DerivedDeterministic",
        "availability": "EndOfPlan",
        "root_to_end_delta": root_to_end,
        "exact_step_delta_sum": exact_sums if action_facts else None,
        "action_kind_counts": dict(counts),
        "cards_played": counts.get("play_card", 0),
        "potion_actions": counts.get("use_potion", 0) + counts.get("discard_potion", 0),
        "hp_lost_to_plan_boundary": max(0, -player_hp_delta),
        "enemy_hp_removed_to_plan_boundary": max(0, -total_enemy_hp_delta),
        "enemy_kill_count_to_plan_boundary": max(0, -living_enemy_delta),
        "enemy_slot_deltas_to_plan_boundary": slot_deltas or None,
        "enemy_slots_killed_to_plan_boundary": slot_summary["killed_enemy_slots"],
        "enemy_hp_removed_by_slot_to_plan_boundary": slot_summary["enemy_hp_removed_by_slot"],
        "visible_incoming_boundary_delta": root_to_end.get("visible_incoming_damage"),
        "visible_incoming_removed_to_plan_boundary": max(
            0,
            -int_value(root_to_end.get("visible_incoming_damage")),
        ),
        "visible_incoming_removed_by_slot_to_plan_boundary": slot_summary[
            "visible_incoming_removed_by_slot"
        ],
        "visible_incoming_removed_by_kill_to_plan_boundary": slot_summary[
            "visible_incoming_removed_by_kill"
        ],
        "damage_hint_total": damage_hint_total,
        "block_hint_total": block_hint_total,
        "visible_attack_mitigation_hint_total": mitigation_hint_total,
        "resource_use": {
            "exhaust_action_count": exhaust_action_count,
            "low_impact_exhaust_action_count": low_impact_exhaust_action_count,
            "low_impact_exhaust_cards": low_impact_exhaust_cards,
            "net_energy_delta": exact_sums.get("energy_delta", 0),
            "net_hand_delta": exact_sums.get("hand_delta", 0),
            "net_draw_delta": exact_sums.get("draw_delta", 0),
            "net_discard_delta": exact_sums.get("discard_delta", 0),
            "net_exhaust_delta": exact_sums.get("exhaust_delta", 0),
        },
        "target_slots": target_slots,
        "unique_target_slots": sorted(set(target_slots)),
        "all_enemies_dead_at_plan_boundary": end_state.get("terminal") == "win"
        or int_value(end_state.get("living_enemy_count")) == 0,
        "energy_unspent_at_plan_boundary": int_or_none(end_state.get("energy")),
    }


def step_trace(
    action: dict[str, Any],
    facts: dict[str, Any] | None,
    state_before: dict[str, Any] | None = None,
    state_after: dict[str, Any] | None = None,
    state_before_exact_hash: str | None = None,
    state_after_exact_hash: str | None = None,
    exact_state_hash_kind: str | None = None,
) -> dict[str, Any]:
    has_state_summary = isinstance(state_before, dict) and isinstance(state_after, dict)
    state_before_summary_hash = hash_json(state_before) if isinstance(state_before, dict) else None
    state_after_summary_hash = hash_json(state_after) if isinstance(state_after, dict) else None
    has_exact_hashes = bool(state_before_exact_hash and state_after_exact_hash)
    tactical_delta = step_tactical_delta(facts, state_before, state_after)
    return {
        "step_index": action.get("step_index"),
        "action": {
            "data_role": "ObservedExact",
            "availability": "BeforeStep",
            "action_id": action.get("action_id"),
            "action_key": action.get("action_key"),
            "action_debug": action.get("action_debug"),
            "input": action.get("input"),
        },
        "state_before_ref": (
            f"exact_state:{state_before_exact_hash}"
            if state_before_exact_hash
            else f"state_summary:{state_before_summary_hash}"
            if state_before_summary_hash
            else None
        ),
        "state_after_ref": (
            f"exact_state:{state_after_exact_hash}"
            if state_after_exact_hash
            else f"state_summary:{state_after_summary_hash}"
            if state_after_summary_hash
            else None
        ),
        "state_ref_kind": "exact_state_hash" if has_exact_hashes else "summary_hash_not_exact_state_hash"
        if has_state_summary
        else None,
        "exact_state_hash_kind": exact_state_hash_kind if has_exact_hashes else None,
        "state_before_exact_state_hash": state_before_exact_hash,
        "state_after_exact_state_hash": state_after_exact_hash,
        "state_summary_hash_algorithm": (
            "blake2b_256_canonical_json_of_state_summary_v1" if has_state_summary else None
        ),
        "state_before_summary_hash": state_before_summary_hash,
        "state_after_summary_hash": state_after_summary_hash,
        "state_before_summary": state_before if isinstance(state_before, dict) else None,
        "state_after_summary": state_after if isinstance(state_after, dict) else None,
        "state_snapshot_availability": (
            "exact_state_hash_and_summary_recorded"
            if has_exact_hashes
            else "summary_recorded_exact_state_ref_not_exported"
            if has_state_summary
            else "not_recorded_in_current_turn_plan_report"
        ),
        "action_facts": facts,
        "tactical_delta": tactical_delta,
        "tactical_events": tactical_events_from_delta(
            step_index=action.get("step_index"),
            facts=facts,
            delta=tactical_delta,
        ),
    }


def candidate_trace(
    root_state: dict[str, Any],
    candidate: dict[str, Any],
) -> dict[str, Any]:
    plan = as_dict(candidate.get("plan"))
    end_fingerprints = as_dict(candidate.get("end_fingerprints"))
    end_exact_state_hash = end_fingerprints.get("exact_state_hash")
    source_steps = [step for step in as_list(plan.get("steps")) if isinstance(step, dict)]
    if source_steps:
        actions = [as_dict(step.get("action")) for step in source_steps]
        action_facts = [as_dict(step.get("action_facts")) for step in source_steps]
        state_pairs = [
            (
                as_dict(step.get("state_before")),
                as_dict(step.get("state_after")),
                step.get("state_before_exact_state_hash")
                if isinstance(step.get("state_before_exact_state_hash"), str)
                else None,
                step.get("state_after_exact_state_hash")
                if isinstance(step.get("state_after_exact_state_hash"), str)
                else None,
                step.get("exact_state_hash_kind")
                if isinstance(step.get("exact_state_hash_kind"), str)
                else None,
            )
            for step in source_steps
        ]
    else:
        actions = [action for action in as_list(plan.get("actions")) if isinstance(action, dict)]
        action_facts = [facts for facts in as_list(plan.get("action_facts")) if isinstance(facts, dict)]
        state_pairs = [(None, None, None, None, None) for _ in actions]
    target = as_dict(candidate.get("target"))
    child_search = as_dict(candidate.get("child_search"))
    limitations = []
    if not action_facts:
        limitations.append("action_facts_not_available_in_source_report")
    if len(action_facts) != len(actions):
        limitations.append("action_facts_count_does_not_match_action_count")
    has_exact_step_hashes = bool(source_steps) and all(
        pair[2] and pair[3] for pair in state_pairs
    )
    if source_steps and not has_exact_step_hashes:
        limitations.append("some_exact_state_refs_hashes_not_available_for_steps")
    if not source_steps:
        limitations.append("state_before_after_refs_not_available_in_current_turn_plan_report")
    plan_id = f"plan:{plan.get('plan_index')}"
    steps = []
    for index, action in enumerate(actions):
        facts = action_facts[index] if index < len(action_facts) else None
        (
            state_before,
            state_after,
            state_before_exact_hash,
            state_after_exact_hash,
            exact_state_hash_kind,
        ) = state_pairs[index] if index < len(state_pairs) else (None, None, None, None, None)
        steps.append(
            step_trace(
                action,
                facts,
                state_before,
                state_after,
                state_before_exact_hash=state_before_exact_hash,
                state_after_exact_hash=state_after_exact_hash,
                exact_state_hash_kind=exact_state_hash_kind,
            )
        )
    event_counts: Counter[str] = Counter()
    for step in steps:
        for event in as_list(step.get("tactical_events")):
            if isinstance(event, dict) and event.get("kind"):
                event_counts[str(event["kind"])] += 1
    plan_summary = plan_tactical_summary(root_state, plan, action_facts)
    plan_summary["tactical_event_counts"] = dict(event_counts)
    return {
        "plan_id": plan_id,
        "plan_index": plan.get("plan_index"),
        "generation": {
            "source": "TurnPlanEnumerator",
            "bucket": plan.get("bucket"),
            "stop_reason": plan.get("stop_reason"),
            "outcome_class": plan.get("outcome_class"),
            "survival_bucket": plan.get("survival_bucket"),
            "progress_bucket": plan.get("progress_bucket"),
        },
        "steps": steps,
        "plan_summary": plan_summary,
        "final_state_ref": (
            f"exact_state:{end_exact_state_hash}" if isinstance(end_exact_state_hash, str) else None
        ),
        "final_state_hash": end_exact_state_hash if isinstance(end_exact_state_hash, str) else None,
        "final_state_hash_kind": "eval_combat_state_fingerprint_v1"
        if isinstance(end_exact_state_hash, str)
        else None,
        "final_state_summary": plan.get("end_state"),
        "outcome_attachment": {
            "data_role": "SearchLabel",
            "availability": "PostSearch",
            "source": target.get("source"),
            "target_kind": target.get("target_kind"),
            "terminal": target.get("terminal"),
            "complete_win": target.get("complete_win"),
            "post_root_player_hp": target.get("post_root_player_hp"),
            "child_search_hp_loss": target.get("child_search_hp_loss"),
            "final_hp": target.get("final_hp"),
            "nodes_expanded": target.get("nodes_expanded"),
            "limitations": target.get("limitations") or [],
            "child_search": child_search or None,
        },
        "counterfactual": {},
        "limitations": limitations,
    }


def pareto_plan_ids(traces: list[dict[str, Any]]) -> list[str]:
    frontier = []
    summaries = [(trace, as_dict(trace.get("plan_summary"))) for trace in traces]
    for trace, summary in summaries:
        dominated = False
        hp_loss = int_value(summary.get("hp_lost_to_plan_boundary"))
        enemy_hp_removed = int_value(summary.get("enemy_hp_removed_to_plan_boundary"))
        kills = int_value(summary.get("enemy_kill_count_to_plan_boundary"))
        potion_actions = int_value(summary.get("potion_actions"))
        final_hp = int_value(
            as_dict(trace.get("outcome_attachment")).get("final_hp"),
            -10**9,
        )
        for other, other_summary in summaries:
            if other is trace:
                continue
            other_hp_loss = int_value(other_summary.get("hp_lost_to_plan_boundary"))
            other_enemy_hp_removed = int_value(other_summary.get("enemy_hp_removed_to_plan_boundary"))
            other_kills = int_value(other_summary.get("enemy_kill_count_to_plan_boundary"))
            other_potion_actions = int_value(other_summary.get("potion_actions"))
            other_final_hp = int_value(
                as_dict(other.get("outcome_attachment")).get("final_hp"),
                -10**9,
            )
            at_least_as_good = (
                other_hp_loss <= hp_loss
                and other_enemy_hp_removed >= enemy_hp_removed
                and other_kills >= kills
                and other_potion_actions <= potion_actions
                and other_final_hp >= final_hp
            )
            strictly_better = (
                other_hp_loss < hp_loss
                or other_enemy_hp_removed > enemy_hp_removed
                or other_kills > kills
                or other_potion_actions < potion_actions
                or other_final_hp > final_hp
            )
            if at_least_as_good and strictly_better:
                dominated = True
                break
        if not dominated:
            frontier.append(str(trace.get("plan_id")))
    return frontier


def root_tactical_context(traces: list[dict[str, Any]]) -> dict[str, Any]:
    summaries = [as_dict(trace.get("plan_summary")) for trace in traces]
    outcomes = [as_dict(trace.get("outcome_attachment")) for trace in traces]
    hp_losses = [int_value(summary.get("hp_lost_to_plan_boundary")) for summary in summaries]
    enemy_removed = [int_value(summary.get("enemy_hp_removed_to_plan_boundary")) for summary in summaries]
    kills = [int_value(summary.get("enemy_kill_count_to_plan_boundary")) for summary in summaries]
    incoming_removed = [
        int_value(summary.get("visible_incoming_removed_to_plan_boundary")) for summary in summaries
    ]
    incoming_removed_by_kill = [
        summary.get("visible_incoming_removed_by_kill_to_plan_boundary")
        for summary in summaries
        if isinstance(summary.get("visible_incoming_removed_by_kill_to_plan_boundary"), int)
    ]
    potion_actions = [int_value(summary.get("potion_actions")) for summary in summaries]
    final_hps = [
        outcome.get("final_hp")
        for outcome in outcomes
        if isinstance(outcome.get("final_hp"), int)
    ]
    pareto = pareto_plan_ids(traces)
    best_hp_loss = min(hp_losses) if hp_losses else None
    best_enemy_removed = max(enemy_removed) if enemy_removed else None
    best_kills = max(kills) if kills else None
    best_incoming_removed = max(incoming_removed) if incoming_removed else None
    best_incoming_removed_by_kill = (
        max(incoming_removed_by_kill) if incoming_removed_by_kill else None
    )
    best_final_hp = max(final_hps) if final_hps else None
    no_hp_loss_candidate_exists = any(loss == 0 for loss in hp_losses)
    no_potion_candidate_exists = any(actions == 0 for actions in potion_actions)
    terminal_win_plan_exists = any(
        as_dict(trace.get("plan_summary")).get("all_enemies_dead_at_plan_boundary")
        for trace in traces
    )
    enemy_kill_candidate_exists = any(kill_count > 0 for kill_count in kills)
    threat_removal_candidate_exists = any(removed > 0 for removed in incoming_removed)
    threat_removal_by_kill_candidate_exists = any(
        removed > 0 for removed in incoming_removed_by_kill
    )
    for trace in traces:
        summary = as_dict(trace.get("plan_summary"))
        outcome = as_dict(trace.get("outcome_attachment"))
        hp_loss = int_value(summary.get("hp_lost_to_plan_boundary"))
        plan_potion_actions = int_value(summary.get("potion_actions"))
        plan_kills = int_value(summary.get("enemy_kill_count_to_plan_boundary"))
        plan_incoming_removed = int_value(summary.get("visible_incoming_removed_to_plan_boundary"))
        counterfactual = {
            "data_role": "Counterfactual",
            "availability": "PostSearch",
            "candidate_set_scope": "same_root_bounded_turn_plan_candidates",
            "is_on_simple_pareto_frontier": trace.get("plan_id") in pareto,
            "missed_no_hp_loss_candidate": no_hp_loss_candidate_exists and hp_loss > 0,
            "missed_enemy_kill_candidate": enemy_kill_candidate_exists and plan_kills == 0,
            "missed_threat_removal_candidate": threat_removal_candidate_exists
            and plan_incoming_removed == 0,
            "potion_used_when_no_potion_candidate_exists": no_potion_candidate_exists
            and plan_potion_actions > 0,
        }
        if best_hp_loss is not None:
            counterfactual["hp_loss_regret_vs_best_boundary"] = hp_loss - best_hp_loss
        if best_enemy_removed is not None:
            counterfactual["enemy_hp_progress_gap_vs_best_boundary"] = best_enemy_removed - int_value(
                summary.get("enemy_hp_removed_to_plan_boundary")
            )
        if best_kills is not None:
            counterfactual["kill_count_gap_vs_best_boundary"] = best_kills - int_value(
                summary.get("enemy_kill_count_to_plan_boundary")
            )
        if best_incoming_removed is not None:
            counterfactual["incoming_removed_gap_vs_best_boundary"] = (
                best_incoming_removed
                - int_value(summary.get("visible_incoming_removed_to_plan_boundary"))
            )
        if best_incoming_removed_by_kill is not None:
            plan_incoming_removed_by_kill = summary.get(
                "visible_incoming_removed_by_kill_to_plan_boundary"
            )
            if isinstance(plan_incoming_removed_by_kill, int):
                counterfactual["incoming_removed_by_kill_gap_vs_best_boundary"] = (
                    best_incoming_removed_by_kill - plan_incoming_removed_by_kill
                )
                counterfactual["missed_threat_removal_by_kill_candidate"] = (
                    threat_removal_by_kill_candidate_exists
                    and plan_incoming_removed_by_kill == 0
                )
        if best_final_hp is not None and isinstance(outcome.get("final_hp"), int):
            counterfactual["final_hp_regret_vs_best_labeled"] = best_final_hp - outcome["final_hp"]
        trace["counterfactual"] = counterfactual
    return {
        "data_role": "Counterfactual",
        "availability": "PostSearch",
        "candidate_count": len(traces),
        "terminal_win_plan_exists": terminal_win_plan_exists,
        "lethal_candidate_exists": terminal_win_plan_exists,
        "enemy_kill_candidate_exists": enemy_kill_candidate_exists,
        "threat_removal_candidate_exists": threat_removal_candidate_exists,
        "threat_removal_by_kill_candidate_exists": threat_removal_by_kill_candidate_exists,
        "complete_win_label_exists": any(outcome.get("complete_win") for outcome in outcomes),
        "no_hp_loss_to_boundary_candidate_exists": no_hp_loss_candidate_exists,
        "no_potion_candidate_exists": no_potion_candidate_exists,
        "best_hp_loss_to_boundary": best_hp_loss,
        "best_enemy_hp_removed_to_boundary": best_enemy_removed,
        "best_enemy_kill_count_to_boundary": best_kills,
        "best_visible_incoming_removed_to_boundary": best_incoming_removed,
        "best_visible_incoming_removed_by_kill_to_boundary": best_incoming_removed_by_kill,
        "best_final_hp_labeled": best_final_hp,
        "pareto_frontier_plan_ids": pareto,
        "limitations": [
            "counterfactuals_are_relative_to_bounded_candidate_set_not_global_optimum",
        ],
    }


def root_candidate_action_coverage(traces: list[dict[str, Any]]) -> dict[str, Any]:
    by_key: dict[str, dict[str, Any]] = {}
    for trace in traces:
        steps = [step for step in as_list(trace.get("steps")) if isinstance(step, dict)]
        if not steps:
            continue
        action = as_dict(steps[0].get("action"))
        action_key = action.get("action_key")
        if not isinstance(action_key, str) or not action_key:
            continue
        entry = by_key.setdefault(
            action_key,
            {
                "action_key": action_key,
                "action_id": action.get("action_id"),
                "action_debug": action.get("action_debug"),
                "input": action.get("input"),
                "candidate_plan_ids": [],
            },
        )
        entry["candidate_plan_ids"].append(trace.get("plan_id"))
    actions = sorted(by_key.values(), key=lambda item: str(item.get("action_key")))
    return {
        "data_role": "DerivedDeterministic",
        "availability": "RootOnly",
        "source": "turn_plan_enumerator_first_actions",
        "coverage_scope": "actions_covered_by_bounded_turn_plan_candidates",
        "covered_action_count": len(actions),
        "candidate_first_actions": actions,
        "limitations": [
            "this is not a full legal action mask; it only covers first actions present in the bounded candidate plans",
        ],
    }


def root_action_kind(action: dict[str, Any]) -> str:
    action_key = action.get("action_key")
    if isinstance(action_key, str):
        if action_key.startswith("combat/end_turn"):
            return "end_turn"
        if action_key.startswith("combat/play_card"):
            return "play_card"
        if action_key.startswith("combat/use_potion"):
            return "use_potion"
        parts = action_key.split("/")
        if len(parts) >= 2 and parts[0] == "combat":
            return parts[1] or "combat_other"
    input_value = action.get("input")
    if input_value == "EndTurn":
        return "end_turn"
    if isinstance(input_value, dict):
        if "PlayCard" in input_value:
            return "play_card"
        if "UsePotion" in input_value:
            return "use_potion"
    return "unknown"


def action_key_set(actions: list[Any]) -> set[str]:
    keys = set()
    for action in actions:
        if not isinstance(action, dict):
            continue
        action_key = action.get("action_key")
        if isinstance(action_key, str) and action_key:
            keys.add(action_key)
    return keys


def root_action_kind_counts(actions: list[dict[str, Any]]) -> dict[str, int]:
    counts = Counter(root_action_kind(action) for action in actions)
    return dict(sorted(counts.items()))


def first_action_summary_key(summary: dict[str, Any]) -> str | None:
    action = as_dict(summary.get("action"))
    action_key = action.get("action_key")
    return action_key if isinstance(action_key, str) and action_key else None


def first_action_summary_bucket_counts(summaries: list[dict[str, Any]]) -> dict[str, int]:
    counts: Counter[str] = Counter()
    for summary in summaries:
        for key, value in as_dict(summary.get("bucket_counts")).items():
            counts[str(key)] += int_value(value)
    return dict(sorted(counts.items()))


def root_action_coverage_diagnostic(mask: dict[str, Any]) -> dict[str, Any] | None:
    if mask.get("complete_legal_mask") is not True:
        return None
    legal_actions = [action for action in as_list(mask.get("legal_actions")) if isinstance(action, dict)]
    eligible_actions = [
        action
        for action in as_list(mask.get("candidate_eligible_actions"))
        if isinstance(action, dict)
    ]
    coverage = as_dict(mask.get("candidate_action_coverage"))
    candidate_first_actions = [
        action
        for action in as_list(coverage.get("candidate_first_actions"))
        if isinstance(action, dict)
    ]
    preselection_actions = [
        action
        for action in as_list(mask.get("preselection_first_actions"))
        if isinstance(action, dict)
    ]
    preselection_summaries = [
        summary
        for summary in as_list(mask.get("preselection_first_action_summaries"))
        if isinstance(summary, dict)
    ]
    equivalence_actions = [
        action
        for action in as_list(mask.get("equivalence_representative_actions"))
        if isinstance(action, dict)
    ]
    candidate_keys = action_key_set(candidate_first_actions)
    eligible_keys = action_key_set(eligible_actions)
    equivalence_keys = action_key_set(equivalence_actions)
    preselection_keys = action_key_set(preselection_actions)
    missing_legal_actions = [
        action
        for action in legal_actions
        if isinstance(action.get("action_key"), str) and action["action_key"] not in candidate_keys
    ]
    ineligible_actions = [
        action
        for action in legal_actions
        if isinstance(action.get("action_key"), str) and action["action_key"] not in eligible_keys
    ]
    eligible_but_uncovered_actions = [
        action
        for action in eligible_actions
        if isinstance(action.get("action_key"), str) and action["action_key"] not in candidate_keys
    ]
    eligible_compressed_actions = [
        action
        for action in eligible_actions
        if isinstance(action.get("action_key"), str)
        and action["action_key"] not in equivalence_keys
    ]
    equivalence_not_preselected_actions = [
        action
        for action in equivalence_actions
        if isinstance(action.get("action_key"), str)
        and action["action_key"] not in preselection_keys
    ]
    eligible_not_preselected_actions = [
        action
        for action in eligible_actions
        if isinstance(action.get("action_key"), str)
        and action["action_key"] not in preselection_keys
    ]
    preselected_but_unselected_actions = [
        action
        for action in preselection_actions
        if isinstance(action.get("action_key"), str) and action["action_key"] not in candidate_keys
    ]
    preselected_but_unselected_summaries = [
        summary
        for summary in preselection_summaries
        if (key := first_action_summary_key(summary)) is not None and key not in candidate_keys
    ]
    legal_count = len(legal_actions)
    candidate_count = len(candidate_first_actions)
    eligible_count = len(eligible_actions)
    equivalence_count = len(equivalence_actions)
    preselection_count = len(preselection_actions)
    return {
        "data_role": "DerivedDeterministic",
        "availability": "RootOnly",
        "source": "root_legal_action_mask_vs_bounded_candidate_first_actions",
        "legal_action_count": legal_count,
        "candidate_eligible_action_count": eligible_count,
        "equivalence_representative_action_count": equivalence_count,
        "preselection_first_action_count": preselection_count,
        "candidate_first_action_count": candidate_count,
        "missing_legal_action_count": len(missing_legal_actions),
        "ineligible_action_count": len(ineligible_actions),
        "eligible_but_uncovered_action_count": len(eligible_but_uncovered_actions),
        "eligible_compressed_action_count": len(eligible_compressed_actions),
        "equivalence_not_preselected_action_count": len(equivalence_not_preselected_actions),
        "eligible_not_preselected_action_count": len(eligible_not_preselected_actions),
        "preselected_but_unselected_action_count": len(preselected_but_unselected_actions),
        "candidate_first_action_coverage_ratio": (
            candidate_count / legal_count if legal_count else 0.0
        ),
        "candidate_eligible_action_coverage_ratio": (
            eligible_count / legal_count if legal_count else 0.0
        ),
        "equivalence_representative_action_coverage_ratio": (
            equivalence_count / legal_count if legal_count else 0.0
        ),
        "preselection_first_action_coverage_ratio": (
            preselection_count / legal_count if legal_count else 0.0
        ),
        "legal_by_kind": root_action_kind_counts(legal_actions),
        "equivalence_representative_by_kind": root_action_kind_counts(equivalence_actions),
        "preselection_first_by_kind": root_action_kind_counts(preselection_actions),
        "candidate_first_by_kind": root_action_kind_counts(candidate_first_actions),
        "missing_legal_by_kind": root_action_kind_counts(missing_legal_actions),
        "eligible_but_uncovered_by_kind": root_action_kind_counts(eligible_but_uncovered_actions),
        "eligible_compressed_by_kind": root_action_kind_counts(eligible_compressed_actions),
        "equivalence_not_preselected_by_kind": root_action_kind_counts(
            equivalence_not_preselected_actions
        ),
        "eligible_not_preselected_by_kind": root_action_kind_counts(
            eligible_not_preselected_actions
        ),
        "preselected_but_unselected_by_kind": root_action_kind_counts(
            preselected_but_unselected_actions
        ),
        "preselected_but_unselected_bucket_counts": first_action_summary_bucket_counts(
            preselected_but_unselected_summaries
        ),
        "ineligible_by_kind": root_action_kind_counts(ineligible_actions),
        "missing_legal_action_examples": [
            action.get("action_key") for action in missing_legal_actions[:5]
        ],
        "eligible_but_uncovered_action_examples": [
            action.get("action_key") for action in eligible_but_uncovered_actions[:5]
        ],
        "eligible_not_preselected_action_examples": [
            action.get("action_key") for action in eligible_not_preselected_actions[:5]
        ],
        "eligible_compressed_action_examples": [
            action.get("action_key") for action in eligible_compressed_actions[:5]
        ],
        "equivalence_not_preselected_action_examples": [
            action.get("action_key") for action in equivalence_not_preselected_actions[:5]
        ],
        "preselected_but_unselected_action_examples": [
            action.get("action_key") for action in preselected_but_unselected_actions[:5]
        ],
        "limitations": [
            "missing means absent from bounded candidate first actions, not proven strategically bad",
        ],
    }


def root_legal_action_mask(root: dict[str, Any], traces: list[dict[str, Any]]) -> dict[str, Any]:
    mask = as_dict(root.get("root_action_mask"))
    if mask:
        full_mask = {
            **mask,
            "candidate_action_coverage": root_candidate_action_coverage(traces),
        }
        diagnostic = root_action_coverage_diagnostic(full_mask)
        if diagnostic is not None:
            full_mask["coverage_diagnostic"] = diagnostic
        return full_mask
    fallback = root_candidate_action_coverage(traces)
    diagnostic = {
        "data_role": "DerivedDeterministic",
        "availability": "RootOnly",
        "source": "root_legal_action_mask_missing",
        "candidate_first_action_count": fallback.get("covered_action_count"),
        "candidate_first_by_kind": root_action_kind_counts(
            [
                action
                for action in as_list(fallback.get("candidate_first_actions"))
                if isinstance(action, dict)
            ]
        ),
        "limitations": [
            "source report did not include full legal actions, so missing-action diagnostics are unavailable",
        ],
    }
    return {
        "data_role": "DerivedDeterministic",
        "availability": "RootOnly",
        "source": "turn_plan_enumerator_first_actions_fallback",
        "complete_legal_mask": False,
        "legal_action_count": None,
        "candidate_eligible_action_count": fallback.get("covered_action_count"),
        "legal_actions": None,
        "candidate_eligible_actions": None,
        "candidate_action_coverage": fallback,
        "coverage_diagnostic": diagnostic,
        "limitations": [
            "source report did not include root_action_mask; only bounded candidate first-action coverage is available",
        ],
    }


def trace_plan_index(trace: dict[str, Any]) -> int:
    return int_value(trace.get("plan_index"), 10**9)


def summary_int(trace: dict[str, Any], key: str, default: int = 0) -> int:
    return int_value(as_dict(trace.get("plan_summary")).get(key), default)


def outcome_int(trace: dict[str, Any], key: str, default: int = -10**9) -> int:
    return int_value(as_dict(trace.get("outcome_attachment")).get(key), default)


def selected_diagnostic_traces(
    traces: list[dict[str, Any]],
) -> list[tuple[list[str], dict[str, Any]]]:
    if not traces:
        return []

    def safety_key(trace: dict[str, Any]) -> tuple[int, int, int, int, int, int]:
        return (
            summary_int(trace, "hp_lost_to_plan_boundary", 10**9),
            -outcome_int(trace, "final_hp"),
            -summary_int(trace, "enemy_hp_removed_to_plan_boundary"),
            -summary_int(trace, "enemy_kill_count_to_plan_boundary"),
            summary_int(trace, "potion_actions"),
            trace_plan_index(trace),
        )

    def progress_key(trace: dict[str, Any]) -> tuple[int, int, int, int, int, int]:
        return (
            -summary_int(trace, "enemy_kill_count_to_plan_boundary"),
            -summary_int(trace, "enemy_hp_removed_to_plan_boundary"),
            summary_int(trace, "hp_lost_to_plan_boundary", 10**9),
            summary_int(trace, "potion_actions"),
            -outcome_int(trace, "final_hp"),
            trace_plan_index(trace),
        )

    def label_key(trace: dict[str, Any]) -> tuple[int, int, int, int]:
        outcome = as_dict(trace.get("outcome_attachment"))
        terminal = outcome.get("terminal")
        complete_win = bool_value(outcome.get("complete_win"))
        tier = 2 if complete_win else 1 if terminal == "win" else 0
        return (
            -tier,
            -outcome_int(trace, "final_hp"),
            summary_int(trace, "hp_lost_to_plan_boundary", 10**9),
            trace_plan_index(trace),
        )

    selections = [
        ("first", min(traces, key=trace_plan_index)),
        ("safety", min(traces, key=safety_key)),
        ("progress", min(traces, key=progress_key)),
        ("label", min(traces, key=label_key)),
    ]
    merged: dict[str, tuple[list[str], dict[str, Any]]] = {}
    for role, trace in selections:
        plan_id = str(trace.get("plan_id") or f"plan:{trace_plan_index(trace)}")
        if plan_id in merged:
            merged[plan_id][0].append(role)
        else:
            merged[plan_id] = ([role], trace)
    return list(merged.values())


def candidate_target_signature(trace: dict[str, Any]) -> tuple[bool, Any, Any]:
    outcome = as_dict(trace.get("outcome_attachment"))
    return (
        bool(outcome.get("complete_win")),
        outcome.get("terminal"),
        outcome.get("final_hp"),
    )


def candidate_search_label_sets(
    traces: list[dict[str, Any]],
    summary: dict[str, Any],
) -> dict[str, Any]:
    selected_plan_index = summary.get("best_target_plan_index")
    selected_trace = next(
        (
            trace
            for trace in traces
            if isinstance(trace.get("plan_index"), int)
            and trace.get("plan_index") == selected_plan_index
        ),
        None,
    )
    if selected_trace is None and isinstance(selected_plan_index, int):
        selected_trace = next(
            (trace for index, trace in enumerate(traces) if index == selected_plan_index),
            None,
        )
    if selected_trace is None:
        return {
            "selected_plan_index": selected_plan_index,
            "selected_plan_id": None,
            "equivalent_hp_outcome_plan_indices": [],
            "equivalent_hp_outcome_plan_ids": [],
            "limitations": ["selected_plan_not_found_in_candidate_traces"],
        }

    selected_complete_win, selected_terminal, selected_final_hp = candidate_target_signature(
        selected_trace
    )
    same_terminal_indices: list[int] = []
    same_terminal_ids: list[str] = []
    exact_hp_indices: list[int] = []
    exact_hp_ids: list[str] = []
    hp_within: dict[int, tuple[list[int], list[str]]] = {
        1: ([], []),
        3: ([], []),
        5: ([], []),
    }
    for trace in traces:
        plan_index = trace.get("plan_index")
        plan_id = str(trace.get("plan_id"))
        complete_win, terminal, final_hp = candidate_target_signature(trace)
        if (complete_win, terminal) != (selected_complete_win, selected_terminal):
            continue
        if isinstance(plan_index, int):
            same_terminal_indices.append(plan_index)
        same_terminal_ids.append(plan_id)
        if not isinstance(final_hp, int) or not isinstance(selected_final_hp, int):
            continue
        hp_delta = abs(final_hp - selected_final_hp)
        if hp_delta == 0:
            if isinstance(plan_index, int):
                exact_hp_indices.append(plan_index)
            exact_hp_ids.append(plan_id)
        for threshold, (indices, ids) in hp_within.items():
            if hp_delta <= threshold:
                if isinstance(plan_index, int):
                    indices.append(plan_index)
                ids.append(plan_id)
    return {
        "selected_plan_index": selected_plan_index,
        "selected_plan_id": selected_trace.get("plan_id"),
        "selected_terminal": selected_terminal,
        "selected_complete_win": selected_complete_win,
        "selected_final_hp": selected_final_hp,
        "same_terminal_plan_indices": same_terminal_indices,
        "same_terminal_plan_ids": same_terminal_ids,
        "equivalent_hp_outcome_plan_indices": exact_hp_indices,
        "equivalent_hp_outcome_plan_ids": exact_hp_ids,
        "hp_within_1_plan_indices": hp_within[1][0],
        "hp_within_1_plan_ids": hp_within[1][1],
        "hp_within_3_plan_indices": hp_within[3][0],
        "hp_within_3_plan_ids": hp_within[3][1],
        "hp_within_5_plan_indices": hp_within[5][0],
        "hp_within_5_plan_ids": hp_within[5][1],
        "limitations": [
            "equivalence_sets_are_relative_to_bounded_candidate_set_not_global_optimum",
        ],
    }


def candidate_set_contrast(traces: list[dict[str, Any]]) -> dict[str, Any]:
    first = min(traces, key=trace_plan_index) if traces else {}
    first_counterfactual = as_dict(as_dict(first).get("counterfactual"))
    role_plans = []
    for roles, trace in selected_diagnostic_traces(traces):
        summary = as_dict(trace.get("plan_summary"))
        outcome = as_dict(trace.get("outcome_attachment"))
        role_plans.append(
            {
                "roles": roles,
                "plan_id": trace.get("plan_id"),
                "plan_index": trace.get("plan_index"),
                "selection_scope": "same_root_bounded_candidate_set",
                "hp_lost_to_plan_boundary": summary.get("hp_lost_to_plan_boundary"),
                "enemy_hp_removed_to_plan_boundary": summary.get(
                    "enemy_hp_removed_to_plan_boundary"
                ),
                "enemy_kill_count_to_plan_boundary": summary.get(
                    "enemy_kill_count_to_plan_boundary"
                ),
                "potion_actions": summary.get("potion_actions"),
                "final_hp": outcome.get("final_hp"),
                "terminal": outcome.get("terminal"),
                "complete_win": outcome.get("complete_win"),
            }
        )
    return {
        "data_role": "DiagnosticDerived",
        "availability": "PostSearch",
        "selector_set": "first_safety_progress_label_v1",
        "candidate_set_scope": "same_root_bounded_turn_plan_candidates",
        "role_plans": role_plans,
        "first_plan_gaps": {
            "plan_id": first.get("plan_id") if first else None,
            "hp_loss_regret_vs_best_boundary": first_counterfactual.get(
                "hp_loss_regret_vs_best_boundary"
            ),
            "enemy_hp_progress_gap_vs_best_boundary": first_counterfactual.get(
                "enemy_hp_progress_gap_vs_best_boundary"
            ),
            "kill_count_gap_vs_best_boundary": first_counterfactual.get(
                "kill_count_gap_vs_best_boundary"
            ),
            "final_hp_regret_vs_best_labeled": first_counterfactual.get(
                "final_hp_regret_vs_best_labeled"
            ),
        },
        "selector_definitions": {
            "first": "lowest enumerated plan_index",
            "safety": "lowest boundary hp loss, then higher labeled final hp",
            "progress": "highest boundary kills and enemy hp removed, then lower hp loss",
            "label": "best bounded child-search label, preferring complete wins and final hp",
        },
        "limitations": [
            "diagnostic_only_not_policy_label",
            "relative_to_bounded_candidate_set_not_global_optimum",
        ],
    }


def short_action_label(action_key: Any, action_debug: Any = None) -> str:
    key = str(action_key or action_debug or "")
    if not key:
        return "?"
    if key == "combat/end_turn" or key.endswith("/end_turn"):
        return "end"
    if "/card:" in key:
        card = key.split("/card:", 1)[1].split("/", 1)[0]
        return display_card_token(card)
    if key.startswith("combat/play_card"):
        return "play_card"
    if "use_potion" in key:
        return "use_potion"
    return key.rsplit("/", 1)[-1]


def display_card_token(card: str) -> str:
    token = card.split("#", 1)[0]
    upgrade_suffix = ""
    if token.endswith("+0"):
        token = token[:-2]
    elif token.endswith("+1"):
        token = token[:-2]
        upgrade_suffix = "+"
    elif "+" in token:
        token, upgrade_suffix = token.rsplit("+", 1)
        upgrade_suffix = f"+{upgrade_suffix}"
    for class_suffix in ("_R", "_G", "_B", "_P"):
        if token.endswith(class_suffix):
            token = token[: -len(class_suffix)]
            break
    return token.replace("_", " ") + upgrade_suffix


def action_preview(trace: dict[str, Any], limit: int = 5) -> str:
    steps = [step for step in as_list(trace.get("steps")) if isinstance(step, dict)]
    labels = []
    for step in steps[:limit]:
        action = as_dict(step.get("action"))
        labels.append(short_action_label(action.get("action_key"), action.get("action_debug")))
    if len(steps) > limit:
        labels.append(f"+{len(steps) - limit}")
    return " -> ".join(labels) if labels else "-"


def diagnostic_trace_line(roles: list[str], trace: dict[str, Any]) -> str:
    summary = as_dict(trace.get("plan_summary"))
    outcome = as_dict(trace.get("outcome_attachment"))
    return (
        f"      {'/'.join(roles)}: "
        f"plan={trace.get('plan_index')} "
        f"hp_loss={summary.get('hp_lost_to_plan_boundary')} "
        f"enemy_removed={summary.get('enemy_hp_removed_to_plan_boundary')} "
        f"kills={summary.get('enemy_kill_count_to_plan_boundary')} "
        f"final_hp={outcome.get('final_hp')} "
        f"terminal={outcome.get('terminal')} "
        f"actions=[{action_preview(trace)}]"
    )


def sorted_dict_display(value: Any) -> dict[str, Any]:
    return dict(sorted(as_dict(value).items()))


def print_compact_cases(episodes: list[dict[str, Any]], case_limit: int) -> None:
    print("  cases:")
    for episode in episodes[:case_limit]:
        source = as_dict(episode.get("source"))
        context = as_dict(episode.get("root_tactical_context"))
        print(f"    case={source.get('case_id') or source.get('input_label')}")
        print(
            "      "
            f"candidates={context.get('candidate_count')} "
            f"best_hp_loss={context.get('best_hp_loss_to_boundary')} "
            f"best_enemy_removed={context.get('best_enemy_hp_removed_to_boundary')} "
            f"threat_removal={context.get('threat_removal_candidate_exists')} "
            f"kill={context.get('enemy_kill_candidate_exists')} "
            f"best_final_hp={context.get('best_final_hp_labeled')} "
            f"pareto={len(as_list(context.get('pareto_frontier_plan_ids')))}"
        )
    if len(episodes) > case_limit:
        print(f"    ... {len(episodes) - case_limit} more episode(s)")


def print_diagnostic_cases(episodes: list[dict[str, Any]], case_limit: int) -> None:
    print("  diagnostic_cases:")
    for episode in episodes[:case_limit]:
        source = as_dict(episode.get("source"))
        context = as_dict(episode.get("root_tactical_context"))
        root_view = as_dict(as_dict(episode.get("root")).get("public_view"))
        root_mask = as_dict(as_dict(episode.get("root")).get("legal_action_mask"))
        root_enumeration = as_dict(as_dict(episode.get("root")).get("turn_plan_enumeration"))
        coverage = as_dict(root_mask.get("coverage_diagnostic"))
        root_state = as_dict(root_view.get("state"))
        traces = [trace for trace in as_list(episode.get("candidate_plans")) if isinstance(trace, dict)]
        trace_by_id = {str(trace.get("plan_id")): trace for trace in traces}
        contrast = as_dict(episode.get("candidate_set_contrast"))
        case_id = source.get("case_id") or source.get("input_label")
        print(
            f"    case={case_id} "
            f"candidates={context.get('candidate_count')} "
            f"root_hp={root_state.get('player_hp')} "
            f"incoming={root_state.get('visible_incoming_damage')} "
            f"enemy_hp={root_state.get('total_enemy_hp')} "
            f"threat_removal={context.get('threat_removal_candidate_exists')} "
            f"kill={context.get('enemy_kill_candidate_exists')}"
        )
        print(
            "      action_coverage: "
            f"legal={coverage.get('legal_action_count')} "
            f"eligible={coverage.get('candidate_eligible_action_count')} "
            f"equiv={coverage.get('equivalence_representative_action_count')} "
            f"preselect={coverage.get('preselection_first_action_count')} "
            f"selected={coverage.get('candidate_first_action_count')} "
            f"compressed={coverage.get('eligible_compressed_by_kind')} "
            f"preselected_unselected={coverage.get('preselected_but_unselected_by_kind')}"
        )
        print(
            "      bucket_pressure: "
            f"preselection={sorted_dict_display(root_enumeration.get('preselection_bucket_counts'))} "
            f"selected={sorted_dict_display(root_enumeration.get('selected_bucket_counts'))} "
            f"preselected_unselected="
            f"{sorted_dict_display(coverage.get('preselected_but_unselected_bucket_counts'))}"
        )
        for role_plan in as_list(contrast.get("role_plans")):
            if not isinstance(role_plan, dict):
                continue
            trace = trace_by_id.get(str(role_plan.get("plan_id")))
            if trace:
                print(diagnostic_trace_line(as_list(role_plan.get("roles")), trace))
        gaps = as_dict(contrast.get("first_plan_gaps"))
        print(
            "      gaps(first_vs_candidate_set): "
            f"hp_loss_regret={gaps.get('hp_loss_regret_vs_best_boundary')} "
            f"progress_gap={gaps.get('enemy_hp_progress_gap_vs_best_boundary')} "
            f"kill_gap={gaps.get('kill_count_gap_vs_best_boundary')} "
            f"final_hp_regret={gaps.get('final_hp_regret_vs_best_labeled')}"
        )
    if len(episodes) > case_limit:
        print(f"    ... {len(episodes) - case_limit} more episode(s)")


def episode_from_lab(
    meta: dict[str, Any],
    lab: dict[str, Any],
    *,
    sim_commit: str | None,
    sim_commit_source: str,
    extractor_commit: str | None,
) -> dict[str, Any]:
    report_path = Path(str(meta.get("source_file") or ""))
    input_path = resolve_input_path(report_path, meta.get("input_path"))
    root = as_dict(lab.get("root"))
    initial_context = as_dict(root.get("initial_context"))
    root_state = as_dict(initial_context.get("state"))
    root_state_enemy_slots = [
        enemy for enemy in as_list(root_state.get("enemy_slots")) if isinstance(enemy, dict)
    ]
    enemy_slots = root_state_enemy_slots or public_enemy_slots_from_capture(input_path)
    candidates = [candidate for candidate in as_list(lab.get("candidates")) if isinstance(candidate, dict)]
    traces = [candidate_trace(root_state, candidate) for candidate in candidates]
    context = root_tactical_context(traces)
    root_fingerprints = as_dict(lab.get("root_fingerprints"))
    root_exact_state_hash = root_fingerprints.get("exact_state_hash")
    root_exact_state_ref = (
        f"exact_state:{root_exact_state_hash}" if isinstance(root_exact_state_hash, str) else None
    )
    limitations = []
    if not root_exact_state_ref or any(not trace.get("final_state_hash") for trace in traces):
        limitations.append("some_exact_state_refs_and_hashes_not_exported_by_source_report")
    if not enemy_slots:
        limitations.append("enemy_slot_public_view_not_available_from_capture")
    if any("action_facts_not_available_in_source_report" in trace["limitations"] for trace in traces):
        limitations.append("some_candidate_action_facts_missing")
    return {
        "schema_name": EPISODE_SCHEMA,
        "schema_version": EPISODE_VERSION,
        "label_role": LABEL_ROLE,
        "source": {
            **meta,
            "input_label": lab.get("input_label"),
        },
        "provenance": {
            "data_role": "ObservedExact",
            "extractor_id": EXTRACTOR_ID,
            "extractor_git_commit": extractor_commit,
            "sim_commit": sim_commit,
            "sim_commit_source": sim_commit_source,
            "candidate_generator_id": as_dict(root.get("enumeration")).get("planning_policy"),
            "search_config": root.get("config"),
            "root_report_schema": root.get("schema_name"),
            "lab_schema": lab.get("schema_name"),
            "policy_quality_claim": lab.get("policy_quality_claim"),
            "notes": lab.get("notes") or [],
        },
        "root": {
            "exact_state_ref": root_exact_state_ref,
            "exact_state_hash": root_exact_state_hash
            if isinstance(root_exact_state_hash, str)
            else None,
            "exact_state_hash_kind": "eval_combat_state_fingerprint_v1"
            if root_exact_state_ref
            else None,
            "turn_plan_enumeration": root.get("enumeration"),
            "public_view": {
                "data_role": "ObservedExact",
                "availability": "RootOnly",
                "state": root_state,
                "phase_profile": initial_context.get("phase_profile"),
                "frontier_value": initial_context.get("frontier_value"),
                "enemy_slots": enemy_slots,
            },
            "legal_action_mask": root_legal_action_mask(root, traces),
        },
        "candidate_plans": traces,
        "root_tactical_context": context,
        "candidate_set_contrast": candidate_set_contrast(traces),
        "label_bundle": {
            "data_role": "SearchLabel",
            "availability": "PostSearch",
            "source": "bounded_child_search_targets_in_turn_plan_guidance_lab",
            "summary": lab.get("summary"),
            "target_sets": candidate_search_label_sets(traces, as_dict(lab.get("summary"))),
            "limitations": [
                "labels_are_oracle_under_current_simulator_and_budget_not_human_policy",
            ],
        },
        "limitations": limitations,
    }


def extract(
    inputs: list[Path],
    out_jsonl: Path | None,
    *,
    summary_only: bool,
    case_limit: int,
    report_mode: str,
    sim_commit: str | None,
    sim_commit_source: str,
    extractor_commit: str | None,
) -> None:
    episodes: list[dict[str, Any]] = []
    for path in inputs:
        for meta, lab in iter_labs(path, load_json(path)):
            episodes.append(
                episode_from_lab(
                    meta,
                    lab,
                    sim_commit=sim_commit,
                    sim_commit_source=sim_commit_source,
                    extractor_commit=extractor_commit,
                )
            )

    if out_jsonl:
        out_jsonl.parent.mkdir(parents=True, exist_ok=True)
        with out_jsonl.open("w", encoding="utf-8") as handle:
            for episode in episodes:
                handle.write(json.dumps(episode, ensure_ascii=False, separators=(",", ":")))
                handle.write("\n")

    counters: Counter[str] = Counter()
    total_candidates = 0
    total_candidates_with_action_facts = 0
    total_candidates_with_step_state_summaries = 0
    total_candidates_with_step_summary_refs = 0
    total_root_legal_actions = 0
    total_root_equivalence_representative_actions = 0
    total_root_preselection_first_actions = 0
    total_root_candidate_first_actions = 0
    total_root_missing_legal_by_kind: Counter[str] = Counter()
    total_root_eligible_uncovered_by_kind: Counter[str] = Counter()
    total_root_eligible_compressed_by_kind: Counter[str] = Counter()
    total_root_equivalence_not_preselected_by_kind: Counter[str] = Counter()
    total_root_eligible_not_preselected_by_kind: Counter[str] = Counter()
    total_root_preselected_unselected_by_kind: Counter[str] = Counter()
    total_root_preselected_unselected_bucket_counts: Counter[str] = Counter()
    total_root_ineligible_by_kind: Counter[str] = Counter()
    total_preselection_bucket_counts: Counter[str] = Counter()
    total_selected_bucket_counts: Counter[str] = Counter()
    for episode in episodes:
        candidates = as_list(episode.get("candidate_plans"))
        total_candidates += len(candidates)
        root = as_dict(episode.get("root"))
        if as_dict(root.get("public_view")).get("enemy_slots"):
            counters["episodes_with_enemy_slots"] += 1
        mask = as_dict(root.get("legal_action_mask"))
        enumeration = as_dict(root.get("turn_plan_enumeration"))
        total_preselection_bucket_counts.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(enumeration.get("preselection_bucket_counts")).items()
            }
        )
        total_selected_bucket_counts.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(enumeration.get("selected_bucket_counts")).items()
            }
        )
        if mask.get("complete_legal_mask") is True:
            counters["episodes_with_complete_legal_action_mask"] += 1
        legal_count = int_or_none(mask.get("legal_action_count"))
        if legal_count is not None:
            total_root_legal_actions += legal_count
        coverage = as_dict(mask.get("candidate_action_coverage"))
        covered_count = int_or_none(coverage.get("covered_action_count"))
        if covered_count is not None:
            total_root_candidate_first_actions += covered_count
        diagnostic = as_dict(mask.get("coverage_diagnostic"))
        equivalence_count = int_or_none(diagnostic.get("equivalence_representative_action_count"))
        if equivalence_count is not None:
            total_root_equivalence_representative_actions += equivalence_count
        preselection_count = int_or_none(diagnostic.get("preselection_first_action_count"))
        if preselection_count is not None:
            total_root_preselection_first_actions += preselection_count
        total_root_missing_legal_by_kind.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(diagnostic.get("missing_legal_by_kind")).items()
            }
        )
        total_root_eligible_uncovered_by_kind.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(
                    diagnostic.get("eligible_but_uncovered_by_kind")
                ).items()
            }
        )
        total_root_eligible_compressed_by_kind.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(diagnostic.get("eligible_compressed_by_kind")).items()
            }
        )
        total_root_equivalence_not_preselected_by_kind.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(
                    diagnostic.get("equivalence_not_preselected_by_kind")
                ).items()
            }
        )
        total_root_eligible_not_preselected_by_kind.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(
                    diagnostic.get("eligible_not_preselected_by_kind")
                ).items()
            }
        )
        total_root_preselected_unselected_by_kind.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(
                    diagnostic.get("preselected_but_unselected_by_kind")
                ).items()
            }
        )
        total_root_preselected_unselected_bucket_counts.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(
                    diagnostic.get("preselected_but_unselected_bucket_counts")
                ).items()
            }
        )
        total_root_ineligible_by_kind.update(
            {
                str(key): int_value(value)
                for key, value in as_dict(diagnostic.get("ineligible_by_kind")).items()
            }
        )
        episode_has_action_facts = False
        episode_has_step_state_summaries = False
        episode_has_step_summary_refs = False
        for plan in candidates:
            steps = [step for step in as_list(as_dict(plan).get("steps")) if isinstance(step, dict)]
            if steps and any(as_dict(step).get("action_facts") for step in steps):
                total_candidates_with_action_facts += 1
                episode_has_action_facts = True
            if steps and all(
                as_dict(step).get("state_before_summary") and as_dict(step).get("state_after_summary")
                for step in steps
            ):
                total_candidates_with_step_state_summaries += 1
                episode_has_step_state_summaries = True
            if steps and all(
                as_dict(step).get("state_before_ref") and as_dict(step).get("state_after_ref")
                for step in steps
            ):
                total_candidates_with_step_summary_refs += 1
                episode_has_step_summary_refs = True
        if episode_has_action_facts:
            counters["episodes_with_action_facts"] += 1
        if episode_has_step_state_summaries:
            counters["episodes_with_step_state_summaries"] += 1
        if episode_has_step_summary_refs:
            counters["episodes_with_step_summary_refs"] += 1
        context = as_dict(episode.get("root_tactical_context"))
        if context.get("no_hp_loss_to_boundary_candidate_exists"):
            counters["episodes_with_no_hp_loss_candidate"] += 1
        if context.get("enemy_kill_candidate_exists"):
            counters["episodes_with_enemy_kill_candidate"] += 1
        if context.get("threat_removal_candidate_exists"):
            counters["episodes_with_threat_removal_candidate"] += 1
        if context.get("threat_removal_by_kill_candidate_exists"):
            counters["episodes_with_threat_removal_by_kill_candidate"] += 1
        if context.get("complete_win_label_exists"):
            counters["episodes_with_complete_win_label"] += 1
        contrast = as_dict(episode.get("candidate_set_contrast"))
        gaps = as_dict(contrast.get("first_plan_gaps"))
        if int_value(gaps.get("hp_loss_regret_vs_best_boundary")) > 0:
            counters["first_plan_hp_loss_regret_positive"] += 1
        if int_value(gaps.get("enemy_hp_progress_gap_vs_best_boundary")) > 0:
            counters["first_plan_progress_gap_positive"] += 1
        if int_value(gaps.get("final_hp_regret_vs_best_labeled")) > 0:
            counters["first_plan_final_hp_regret_positive"] += 1
        for role_plan in as_list(contrast.get("role_plans")):
            if not isinstance(role_plan, dict):
                continue
            roles = set(str(role) for role in as_list(role_plan.get("roles")))
            if "first" in roles:
                if "safety" in roles:
                    counters["first_plan_is_safety"] += 1
                if "progress" in roles:
                    counters["first_plan_is_progress"] += 1
                if "label" in roles:
                    counters["first_plan_is_label"] += 1
    print("CombatTacticalTraceExtract")
    print(f"  episodes={len(episodes)} candidates={total_candidates}")
    print(f"  episodes_with_enemy_slots={counters['episodes_with_enemy_slots']}")
    coverage_ratio = (
        total_root_candidate_first_actions / total_root_legal_actions
        if total_root_legal_actions > 0
        else 0.0
    )
    print(
        "  root_legal_action_mask="
        f"episodes={counters['episodes_with_complete_legal_action_mask']}/{len(episodes)} "
        f"legal_actions={total_root_legal_actions} "
        f"equivalence_representatives={total_root_equivalence_representative_actions} "
        f"preselection_first_actions={total_root_preselection_first_actions} "
        f"candidate_first_actions={total_root_candidate_first_actions} "
        f"coverage_ratio={coverage_ratio:.3f}"
    )
    if total_root_missing_legal_by_kind:
        print(
            "  root_missing_legal_by_kind="
            f"{dict(sorted(total_root_missing_legal_by_kind.items()))}"
        )
    if total_root_eligible_uncovered_by_kind:
        print(
            "  root_eligible_but_uncovered_by_kind="
            f"{dict(sorted(total_root_eligible_uncovered_by_kind.items()))}"
        )
    if total_root_eligible_compressed_by_kind:
        print(
            "  root_eligible_compressed_by_kind="
            f"{dict(sorted(total_root_eligible_compressed_by_kind.items()))}"
        )
    if total_root_equivalence_not_preselected_by_kind:
        print(
            "  root_equivalence_not_preselected_by_kind="
            f"{dict(sorted(total_root_equivalence_not_preselected_by_kind.items()))}"
        )
    if total_root_eligible_not_preselected_by_kind:
        print(
            "  root_eligible_not_preselected_by_kind="
            f"{dict(sorted(total_root_eligible_not_preselected_by_kind.items()))}"
        )
    if total_root_preselected_unselected_by_kind:
        print(
            "  root_preselected_but_unselected_by_kind="
            f"{dict(sorted(total_root_preselected_unselected_by_kind.items()))}"
        )
    if total_root_preselected_unselected_bucket_counts:
        print(
            "  root_preselected_but_unselected_bucket_counts="
            f"{dict(sorted(total_root_preselected_unselected_bucket_counts.items()))}"
        )
    if total_root_ineligible_by_kind:
        print(
            "  root_ineligible_by_kind="
            f"{dict(sorted(total_root_ineligible_by_kind.items()))}"
        )
    if total_preselection_bucket_counts:
        print(
            "  turn_plan_preselection_bucket_counts="
            f"{dict(sorted(total_preselection_bucket_counts.items()))}"
        )
    if total_selected_bucket_counts:
        print(
            "  turn_plan_selected_bucket_counts="
            f"{dict(sorted(total_selected_bucket_counts.items()))}"
        )
    print(
        "  action_facts_coverage="
        f"episodes={counters['episodes_with_action_facts']} "
        f"candidates={total_candidates_with_action_facts}/{total_candidates}"
    )
    print(
        "  step_state_summary_coverage="
        f"episodes={counters['episodes_with_step_state_summaries']} "
        f"candidates={total_candidates_with_step_state_summaries}/{total_candidates}"
    )
    print(
        "  step_summary_ref_coverage="
        f"episodes={counters['episodes_with_step_summary_refs']} "
        f"candidates={total_candidates_with_step_summary_refs}/{total_candidates}"
    )
    print(f"  episodes_with_no_hp_loss_candidate={counters['episodes_with_no_hp_loss_candidate']}")
    print(f"  episodes_with_enemy_kill_candidate={counters['episodes_with_enemy_kill_candidate']}")
    print(
        "  episodes_with_threat_removal_candidate="
        f"{counters['episodes_with_threat_removal_candidate']}"
    )
    print(
        "  episodes_with_threat_removal_by_kill_candidate="
        f"{counters['episodes_with_threat_removal_by_kill_candidate']}"
    )
    print(f"  episodes_with_complete_win_label={counters['episodes_with_complete_win_label']}")
    print(
        "  first_plan_contrast="
        f"hp_loss_regret_positive={counters['first_plan_hp_loss_regret_positive']} "
        f"progress_gap_positive={counters['first_plan_progress_gap_positive']} "
        f"final_hp_regret_positive={counters['first_plan_final_hp_regret_positive']} "
        f"is_safety={counters['first_plan_is_safety']} "
        f"is_progress={counters['first_plan_is_progress']} "
        f"is_label={counters['first_plan_is_label']}"
    )
    if out_jsonl:
        print(f"  jsonl={out_jsonl}")
    if summary_only:
        return
    if report_mode == "diagnostic":
        print_diagnostic_cases(episodes, case_limit)
    else:
        print_compact_cases(episodes, case_limit)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", nargs="+", type=Path)
    parser.add_argument("--out-jsonl", type=Path)
    parser.add_argument("--summary-only", action="store_true")
    parser.add_argument("--case-limit", type=int, default=12)
    parser.add_argument("--report-mode", choices=("compact", "diagnostic"), default="compact")
    parser.add_argument(
        "--sim-commit",
        help="Simulator git commit for provenance; defaults to current git HEAD.",
    )
    args = parser.parse_args()
    sim_commit = args.sim_commit or current_git_commit()
    sim_commit_source = "cli" if args.sim_commit else "current_git_head_at_extraction_time"
    extractor_commit = current_git_commit()
    extract(
        args.inputs,
        args.out_jsonl,
        summary_only=args.summary_only,
        case_limit=max(0, args.case_limit),
        report_mode=args.report_mode,
        sim_commit=sim_commit,
        sim_commit_source=sim_commit_source,
        extractor_commit=extractor_commit,
    )


if __name__ == "__main__":
    main()
