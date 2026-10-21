#!/usr/bin/env python3
"""Render an exact combat resource-frontier report for human investigation.

This is deliberately an offline artifact reader.  It never loads the simulator,
runs search, or reconstructs missing state from action keys.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def main() -> int:
    args = parse_args()
    report = load_report(args.report)
    print(
        render_report(
            report,
            args.candidate,
            args.turn,
            args.cutpoints_only,
            args.exploration,
        )
    )
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Review replay-exact resource-frontier candidates without rerunning "
            "combat search or loading the simulator."
        )
    )
    parser.add_argument("report", type=Path, help="CombatResourceFrontierProbeV1Report JSON")
    parser.add_argument(
        "--candidate",
        default="0",
        help=(
            "Candidate index or lane id to expand; use 'all' for every candidate "
            "(default: 0, the preserve-resource-first frontier ordering)."
        ),
    )
    parser.add_argument(
        "--turn",
        type=int,
        action="append",
        help="Only expand this turn; may be repeated.",
    )
    parser.add_argument(
        "--cutpoints-only",
        action="store_true",
        help="Show candidate summary, first divergence, and cutpoints without action rows.",
    )
    parser.add_argument(
        "--exploration",
        action="store_true",
        help="Expand the compact typed observation after every search-lane quantum.",
    )
    return parser.parse_args()


def load_report(path: Path) -> dict[str, Any]:
    try:
        report = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read resource-frontier report {path}: {error}") from error
    if report.get("schema_name") != "CombatResourceFrontierProbeV1Report":
        raise SystemExit(
            "expected CombatResourceFrontierProbeV1Report; refusing to guess another schema"
        )
    if report.get("schema_version") != 1:
        raise SystemExit(
            f"unsupported resource-frontier schema version {report.get('schema_version')!r}"
        )
    candidates = report.get("frontier")
    if not isinstance(candidates, list):
        raise SystemExit("resource-frontier report has no typed frontier array")
    missing = [
        index
        for index, candidate in enumerate(candidates)
        if not isinstance(candidate.get("storyboard"), dict)
    ]
    if missing:
        raise SystemExit(
            "candidate storyboard is absent for indices "
            f"{missing}; rerun the fixed-root probe with the current worker"
        )
    return report


def render_report(
    report: dict[str, Any],
    candidate_selector: str,
    selected_turns: list[int] | None,
    cutpoints_only: bool,
    show_exploration: bool,
) -> str:
    candidates = report["frontier"]
    lines = [render_header(report), "", "LANES"]
    lines.extend(render_lanes(report.get("lanes") or []))
    lines.extend(["", "SEARCH PROGRESS"])
    lines.extend(render_search_progress(report))
    if show_exploration:
        lines.extend(["", "EXPLORATION QUANTA"])
        lines.extend(render_exploration(report.get("service_trace") or []))
    lines.extend(["", "OBSERVED FRONTIER"])
    lines.extend(render_candidate_table(candidates))
    if len(candidates) > 1:
        lines.extend(["", "FIRST DIVERGENCE"])
        lines.extend(render_first_divergence(candidates))

    selected = select_candidates(candidates, candidate_selector)
    turn_filter = set(selected_turns or [])
    for index, candidate in selected:
        lines.extend(["", render_candidate_heading(index, candidate)])
        storyboard = candidate["storyboard"]
        lines.extend(render_review_focus(storyboard.get("cutpoints") or [], turn_filter))
        for turn in storyboard.get("turns") or []:
            if turn_filter and turn.get("turn") not in turn_filter:
                continue
            lines.extend(render_turn(turn, cutpoints_only=cutpoints_only))
        lines.extend(render_cutpoints(storyboard.get("cutpoints") or [], turn_filter))
    return "\n".join(lines)


def render_header(report: dict[str, Any]) -> str:
    openness = "OPEN / budget-censored" if report.get("frontier_open") else "CLOSED / exhausted"
    return (
        f"RESOURCE FRONTIER  {openness}\n"
        f"input: {report.get('input_label', '?')}\n"
        f"stop={report.get('stop', '?')}  total_nodes={report.get('total_nodes_expanded', '?')}  "
        f"elapsed_ms={report.get('elapsed_ms', '?')}  revisions={report.get('frontier_revision', '?')}"
    )


def render_lanes(lanes: list[dict[str, Any]]) -> list[str]:
    if not lanes:
        return ["  (none)"]
    rows = []
    for lane in lanes:
        state = "exhausted" if lane.get("exhausted") else "open"
        potion = lane.get("entry_potion_id") or "preserve all entry potions"
        rows.append(
            "  {lane}: nodes={nodes}, wins={wins}, queue={queue}, {state}, "
            "last={last}, resource={potion}".format(
                lane=lane.get("lane_id", "?"),
                nodes=lane.get("nodes_expanded", "?"),
                wins=lane.get("retained_complete_win_count", "?"),
                queue=lane.get("remaining_work_items", "?"),
                state=state,
                last=lane.get("last_stop", "?"),
                potion=potion,
            )
        )
    return rows


def render_candidate_table(candidates: list[dict[str, Any]]) -> list[str]:
    if not candidates:
        return ["  (no replay-verified complete win observed)"]
    rows = []
    for index, candidate in enumerate(candidates):
        outcome = candidate.get("outcome") or {}
        storyboard = candidate.get("storyboard") or {}
        rows.append(
            "  [{index}] hp={hp}, potions_spent={potions}, turns={turns}, actions={actions}, "
            "first_seen_at_total_nodes={nodes}, lane={lane}".format(
                index=index,
                hp=outcome.get("final_hp", "?"),
                potions=outcome.get("total_potions_consumed", "?"),
                turns=len(storyboard.get("turns") or []),
                actions=candidate.get("action_count", "?"),
                nodes=candidate.get("first_observed_total_nodes", "?"),
                lane=candidate.get("source_lane_id", "?"),
            )
        )
    return rows


def render_search_progress(report: dict[str, Any]) -> list[str]:
    improvements = report.get("improvements") or []
    if not improvements:
        return ["  (no observed frontier improvement)"]
    rows = []
    for improvement in improvements:
        rows.append(
            "  rev {revision} at total_nodes={total} / lane_nodes={lane_nodes}: "
            "served={lane}, frontier_size={size}, keep_best_hp={keep}, "
            "spend_best_hp={spend}".format(
                revision=improvement.get("frontier_revision", "?"),
                total=improvement.get("total_nodes_expanded", "?"),
                lane_nodes=improvement.get("lane_nodes_expanded", "?"),
                lane=improvement.get("served_lane_id", "?"),
                size=improvement.get("frontier_size", "?"),
                keep=display_optional(improvement.get("preserved_entry_potion_best_hp")),
                spend=display_optional(improvement.get("spending_best_hp")),
            )
        )
    rows.append(
        "  no later observed frontier revision before "
        f"{report.get('stop', '?')} at total_nodes={report.get('total_nodes_expanded', '?')}"
    )
    return rows


def render_exploration(trace: list[dict[str, Any]]) -> list[str]:
    if not trace:
        return [
            "  (typed service trace absent; rerun the fixed-root probe with the current worker)"
        ]
    rows = []
    for observation in trace:
        marker = " FRONTIER_CHANGED" if observation.get("resource_frontier_changed") else ""
        rows.append(
            "  #{index:02} {lane}: +{actual}/{requested} nodes -> lane={lane_nodes}, "
            "total={total}, best_win_hp={hp}, wins={wins}, queue={queue}, "
            "lane_rev={lane_revision}, stop={stop}{marker}".format(
                index=integer(observation.get("service_index")),
                lane=observation.get("lane_id", "?"),
                actual=observation.get("nodes_expanded_this_service", "?"),
                requested=observation.get("requested_additional_nodes", "?"),
                lane_nodes=observation.get("lane_nodes_expanded", "?"),
                total=observation.get("total_nodes_expanded", "?"),
                hp=display_optional(observation.get("lane_best_win_hp")),
                wins=observation.get("lane_retained_complete_win_count", "?"),
                queue=observation.get("remaining_work_items", "?"),
                lane_revision=observation.get("lane_candidate_frontier_revision", "?"),
                stop=observation.get("stop", "?"),
                marker=marker,
            )
        )
    return rows


def render_first_divergence(candidates: list[dict[str, Any]]) -> list[str]:
    baseline = candidates[0]
    baseline_actions = flattened_story_actions(baseline)
    lines = []
    for index, candidate in enumerate(candidates[1:], start=1):
        other_actions = flattened_story_actions(candidate)
        prefix = 0
        while (
            prefix < len(baseline_actions)
            and prefix < len(other_actions)
            and action_identity(baseline_actions[prefix]) == action_identity(other_actions[prefix])
        ):
            prefix += 1
        last_shared = baseline_actions[prefix - 1] if prefix else None
        if last_shared is None:
            lines.append(f"  [0] vs [{index}]: no shared action prefix")
        else:
            lines.append(
                f"  [0] vs [{index}]: {prefix} shared actions; last shared "
                f"#{last_shared.get('action_index')} T{action_turn(baseline, prefix - 1)} "
                f"{last_shared.get('label', '?')}"
            )
        lines.append(f"    [0] {next_action_label(baseline, baseline_actions, prefix)}")
        lines.append(f"    [{index}] {next_action_label(candidate, other_actions, prefix)}")
    return lines


def select_candidates(
    candidates: list[dict[str, Any]], selector: str
) -> list[tuple[int, dict[str, Any]]]:
    if selector.casefold() == "all":
        return list(enumerate(candidates))
    try:
        index = int(selector)
    except ValueError:
        exact = [
            (index, candidate)
            for index, candidate in enumerate(candidates)
            if candidate.get("source_lane_id") == selector
        ]
        if exact:
            return exact
        partial = [
            (index, candidate)
            for index, candidate in enumerate(candidates)
            if selector.casefold() in str(candidate.get("source_lane_id", "")).casefold()
        ]
        if len(partial) == 1:
            return partial
        available = ", ".join(
            f"[{index}] {candidate.get('source_lane_id', '?')}"
            for index, candidate in enumerate(candidates)
        )
        raise SystemExit(f"candidate selector {selector!r} is not unique; available: {available}")
    if not 0 <= index < len(candidates):
        raise SystemExit(f"candidate index {index} is outside 0..{len(candidates) - 1}")
    return [(index, candidates[index])]


def render_candidate_heading(index: int, candidate: dict[str, Any]) -> str:
    outcome = candidate.get("outcome") or {}
    terminal = (candidate.get("storyboard") or {}).get("terminal", "?")
    return (
        f"CANDIDATE [{index}]  lane={candidate.get('source_lane_id', '?')}  "
        f"terminal={terminal}  final_hp={outcome.get('final_hp', '?')}  "
        f"potions_spent={outcome.get('total_potions_consumed', '?')}"
    )


def render_review_focus(
    cutpoints: list[dict[str, Any]], turn_filter: set[int]
) -> list[str]:
    selected = [
        cutpoint
        for cutpoint in cutpoints
        if (not turn_filter or cutpoint.get("turn") in turn_filter)
    ]
    losses = [
        cutpoint
        for cutpoint in selected
        if cutpoint.get("signal")
        in {"end_turn_transition_hp_loss", "self_transition_hp_loss"}
    ]
    losses.sort(
        key=lambda cutpoint: (
            -integer(cutpoint.get("observed_amount")),
            integer(cutpoint.get("action_index")),
        )
    )
    markers = [
        cutpoint
        for cutpoint in selected
        if cutpoint.get("signal") in {"unused_energy_at_end_turn", "potion_spent"}
    ]
    markers.sort(key=lambda cutpoint: integer(cutpoint.get("action_index")))
    lines = ["  REVIEW FOCUS (navigation hints, not error verdicts)"]
    if not losses and not markers:
        return lines + ["    (no HP-loss, potion, or unused-energy cutpoint)"]
    if losses:
        lines.append("    largest observed HP-loss transitions:")
    for cutpoint in losses[:4]:
        lines.append(
            "      #{index} T{turn} {signal} amount={amount}: {label}".format(
                index=cutpoint.get("action_index", "?"),
                turn=cutpoint.get("turn", "?"),
                signal=cutpoint.get("signal", "?"),
                amount=cutpoint.get("observed_amount", "?"),
                label=cutpoint.get("label", "?"),
            )
        )
    if markers:
        lines.append("    resource/tempo markers (not ranked against HP):")
    for cutpoint in markers:
        lines.append(
            "      #{index} T{turn} {signal} amount={amount}: {label}".format(
                index=cutpoint.get("action_index", "?"),
                turn=cutpoint.get("turn", "?"),
                signal=cutpoint.get("signal", "?"),
                amount=cutpoint.get("observed_amount", "?"),
                label=cutpoint.get("label", "?"),
            )
        )
    return lines


def render_turn(turn: dict[str, Any], *, cutpoints_only: bool) -> list[str]:
    start = turn.get("start") or {}
    end = turn.get("end") or {}
    turn_number = turn.get("turn", "?")
    lines = [
        "  TURN {turn}  hp {start_hp}->{end_hp} ({hp_delta:+})  enemy_hp {start_enemy}->{end_enemy} "
        "({enemy_delta:+})  visible_incoming={incoming}".format(
            turn=turn_number,
            start_hp=start.get("player_hp", "?"),
            end_hp=end.get("player_hp", "?"),
            hp_delta=integer(turn.get("observed_player_hp_delta")),
            start_enemy=total_enemy_hp(start.get("enemies") or []),
            end_enemy=total_enemy_hp(end.get("enemies") or []),
            enemy_delta=integer(turn.get("observed_enemy_hp_delta")),
            incoming=start.get("visible_incoming_damage", "?"),
        )
    ]
    lines.append(f"    hand: {render_hand(start.get('hand') or [])}")
    lines.append(f"    enemies: {render_enemies(start.get('enemies') or [])}")
    if not cutpoints_only:
        before_player_powers = start.get("player_powers") or []
        before_enemies = start.get("enemies") or []
        for action in turn.get("actions") or []:
            lines.append(render_action(action, before_player_powers, before_enemies))
            before_player_powers = action.get("after_player_powers") or []
            before_enemies = action.get("after_enemies") or []
    loss_parts = []
    if turn.get("self_transition_hp_loss"):
        loss_parts.append(f"self_loss={turn['self_transition_hp_loss']}")
    if turn.get("end_turn_transition_hp_loss"):
        loss_parts.append(f"end_turn_loss={turn['end_turn_transition_hp_loss']}")
    if turn.get("observed_hp_gain"):
        loss_parts.append(f"hp_gain={turn['observed_hp_gain']}")
    if turn.get("ended_with_unused_energy") is not None:
        loss_parts.append(f"unused_energy={turn['ended_with_unused_energy']}")
    if loss_parts:
        lines.append(f"    turn signals: {', '.join(loss_parts)}")
    return lines


def render_action(
    action: dict[str, Any],
    before_player_powers: list[dict[str, Any]],
    before_enemies: list[dict[str, Any]],
) -> str:
    changes = []
    add_change(changes, "hp", action, "before_player_hp", "after_player_hp")
    add_change(changes, "block", action, "before_player_block", "after_player_block")
    add_change(changes, "energy", action, "before_energy", "after_energy")
    add_change(changes, "enemy_hp", action, "before_total_enemy_hp", "after_total_enemy_hp")
    drawn = action.get("drawn_cards") or []
    if drawn:
        changes.append("draw=" + ",".join(render_drawn_card(card) for card in drawn))
    changes.extend(
        render_power_changes(
            "player",
            before_player_powers,
            action.get("after_player_powers") or [],
        )
    )
    changes.extend(
        render_enemy_changes(before_enemies, action.get("after_enemies") or [])
    )
    signals = action.get("signals") or []
    if signals:
        changes.append("CUT=" + ",".join(str(signal) for signal in signals))
    suffix = f" | {'; '.join(changes)}" if changes else ""
    return f"    #{action.get('action_index', '?'):>2} {action.get('label', '?')}{suffix}"


def render_cutpoints(cutpoints: list[dict[str, Any]], turn_filter: set[int]) -> list[str]:
    selected = [
        cutpoint
        for cutpoint in cutpoints
        if not turn_filter or cutpoint.get("turn") in turn_filter
    ]
    lines = ["  CUTPOINT INDEX (navigation hints, not error verdicts)"]
    if not selected:
        return lines + ["    (none)"]
    for cutpoint in selected:
        lines.append(
            "    #{index} T{turn} {signal} amount={amount}: {label}".format(
                index=cutpoint.get("action_index", "?"),
                turn=cutpoint.get("turn", "?"),
                signal=cutpoint.get("signal", "?"),
                amount=cutpoint.get("observed_amount", "?"),
                label=cutpoint.get("label", "?"),
            )
        )
    return lines


def flattened_story_actions(candidate: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        action
        for turn in (candidate.get("storyboard") or {}).get("turns") or []
        for action in turn.get("actions") or []
    ]


def action_identity(action: dict[str, Any]) -> Any:
    return action.get("input")


def action_turn(candidate: dict[str, Any], action_offset: int) -> Any:
    action = flattened_story_actions(candidate)[action_offset]
    action_index = action.get("action_index")
    for turn in (candidate.get("storyboard") or {}).get("turns") or []:
        if any(row.get("action_index") == action_index for row in turn.get("actions") or []):
            return turn.get("turn", "?")
    return "?"


def next_action_label(
    candidate: dict[str, Any], actions: list[dict[str, Any]], offset: int
) -> str:
    if offset >= len(actions):
        return "<line ended>"
    action = actions[offset]
    return (
        f"#{action.get('action_index')} T{action_turn(candidate, offset)} "
        f"{action.get('label', '?')}"
    )


def render_hand(hand: list[dict[str, Any]]) -> str:
    if not hand:
        return "(empty)"
    return ", ".join(
        "[{index}] {name}{upgrade}(cost={cost}{playable})".format(
            index=card.get("hand_index", "?"),
            name=card.get("card_name", card.get("card_id", "?")),
            upgrade="+" if card.get("upgrades") else "",
            cost=card.get("cost_for_turn", "?"),
            playable="" if card.get("playable") else ", not-playable",
        )
        for card in hand
    )


def render_enemies(enemies: list[dict[str, Any]]) -> str:
    living = [enemy for enemy in enemies if enemy.get("alive")]
    if not living:
        return "(none living)"
    return "; ".join(
        "{name}[{slot}] {hp}/{max_hp}{block} {intent}{damage}{powers}".format(
            name=enemy.get("enemy_name", "?"),
            slot=enemy.get("monster_slot", "?"),
            hp=enemy.get("hp", "?"),
            max_hp=enemy.get("max_hp", "?"),
            block=(f" block={enemy.get('block')}" if enemy.get("block") else ""),
            intent=enemy.get("visible_intent", "unknown"),
            damage=render_intent_damage(enemy),
            powers=render_power_suffix(enemy.get("powers") or []),
        )
        for enemy in living
    )


def render_intent_damage(enemy: dict[str, Any]) -> str:
    total = enemy.get("visible_total_damage")
    if total is None:
        return ""
    per_hit = enemy.get("visible_damage_per_hit")
    hits = enemy.get("visible_hits")
    if per_hit is not None and hits not in (None, 0, 1):
        return f" {per_hit}x{hits}={total}"
    return f" {total}"


def render_drawn_card(card: dict[str, Any]) -> str:
    return f"{card.get('id', '?')}{'+' if card.get('upgrades') else ''}"


def render_enemy_changes(
    before_enemies: list[dict[str, Any]], after_enemies: list[dict[str, Any]]
) -> list[str]:
    changes = []
    before_by_slot = {enemy.get("monster_slot"): enemy for enemy in before_enemies}
    for after in after_enemies:
        slot = after.get("monster_slot")
        before = before_by_slot.get(slot)
        if before is None:
            continue
        subject = f"{after.get('enemy_name', '?')}[{slot}]"
        if before.get("block") != after.get("block"):
            changes.append(f"{subject}.block={before.get('block')}->{after.get('block')}")
        changes.extend(
            render_power_changes(
                subject,
                before.get("powers") or [],
                after.get("powers") or [],
            )
        )
    return changes


def render_power_changes(
    subject: str,
    before_powers: list[dict[str, Any]],
    after_powers: list[dict[str, Any]],
) -> list[str]:
    before = {power.get("power_id"): power for power in before_powers}
    after = {power.get("power_id"): power for power in after_powers}
    changes = []
    for power_id in sorted(set(before) | set(after), key=str):
        old = before.get(power_id)
        new = after.get(power_id)
        old_amount = integer((old or {}).get("amount"))
        new_amount = integer((new or {}).get("amount"))
        if old_amount == new_amount:
            continue
        name = (new or old or {}).get("power_name", power_id)
        changes.append(f"{subject}.{name}={old_amount}->{new_amount}")
    return changes


def render_power_suffix(powers: list[dict[str, Any]]) -> str:
    if not powers:
        return ""
    return " powers=" + ",".join(
        f"{power.get('power_name', power.get('power_id', '?'))}:{power.get('amount', '?')}"
        for power in powers
    )


def add_change(
    changes: list[str], label: str, row: dict[str, Any], before_key: str, after_key: str
) -> None:
    before = row.get(before_key)
    after = row.get(after_key)
    if before != after:
        changes.append(f"{label}={before}->{after}")


def total_enemy_hp(enemies: list[dict[str, Any]]) -> int:
    return sum(
        max(0, integer(enemy.get("hp")))
        for enemy in enemies
        if enemy.get("alive")
    )


def integer(value: Any) -> int:
    return int(value or 0)


def display_optional(value: Any) -> Any:
    return "-" if value is None else value


if __name__ == "__main__":
    raise SystemExit(main())
