#!/usr/bin/env python3
"""Run the review-only Frozen Case Panel V0a."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any


CASE_PATHS = [
    Path("fixtures/combat_cases/frozen_v0a_awakened_one_1552225675_a3f48.json"),
    Path("fixtures/combat_cases/frozen_v0a_collector_1552225671_a2f32.json"),
    Path("fixtures/combat_cases/frozen_v0a_gremlin_leader_1552225671_a2f29.json"),
]

ROW_FIELDS = [
    "case_id",
    "case_path",
    "case_origin_seed",
    "captured_at_commit",
    "reviewed_at_commit",
    "lane",
    "search_config_summary",
    "complete_win",
    "outcome_tier",
    "final_hp",
    "turns",
    "potions_used",
    "first_action_key",
    "first_action_role",
    "key_card_played",
    "key_card_first_play_step",
    "living_enemy_count",
    "total_enemy_hp",
    "half_dead_enemy_count",
    "phase_pending_enemy_player_died",
    "nodes_expanded",
    "elapsed_ms",
    "deadline_hit",
    "tool_status",
]


def main() -> int:
    args = parse_args()
    output_root = Path(args.output_root)
    reviews_dir = output_root / "reviews"
    reviews_dir.mkdir(parents=True, exist_ok=True)
    reviewed_at_commit = git_commit()
    rows: list[dict[str, Any]] = []
    for case_path in CASE_PATHS:
        review_path = reviews_dir / f"{case_path.stem}.review.json"
        command = review_command(case_path, review_path, args)
        completed = subprocess.run(command)
        if completed.returncode != 0:
            rows.append(process_failure_row(case_path, reviewed_at_commit, completed.returncode))
            continue
        review = read_json(review_path)
        rows.extend(rows_from_review(review, reviewed_at_commit=reviewed_at_commit))

    write_jsonl(output_root / "panel_rows.jsonl", rows)
    write_markdown_table(output_root / "panel_table.md", rows)
    print(f"wrote {output_root / 'panel_rows.jsonl'}")
    print(f"wrote {output_root / 'panel_table.md'}")
    return 1 if any(row["tool_status"] != "ok" for row in rows) else 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-root", default="target/frozen-case-panel-v0a")
    parser.add_argument("--slow-nodes", type=int, default=800_000)
    parser.add_argument("--slow-ms", type=int, default=8_000)
    parser.add_argument("--diagnostic-potion-max", type=int, default=3)
    parser.add_argument("--action-preview-limit", type=int, default=12)
    return parser.parse_args()


def review_command(case_path: Path, review_path: Path, args: argparse.Namespace) -> list[str]:
    return [
        "cargo",
        "run",
        "--quiet",
        "-p",
        "sts_oracle_tools",
        "--bin",
        "combat_case_review",
        "--",
        "--case",
        str(case_path),
        "--frozen-panel-lanes",
        "--slow-nodes",
        str(args.slow_nodes),
        "--slow-ms",
        str(args.slow_ms),
        "--diagnostic-potion-max",
        str(args.diagnostic_potion_max),
        "--action-preview-limit",
        str(args.action_preview_limit),
        "--compact",
        "--write-review",
        str(review_path),
    ]


def rows_from_review(
    review: dict[str, Any], reviewed_at_commit: str | None = None
) -> list[dict[str, Any]]:
    case_path = review.get("case_path")
    case_id = Path(case_path).stem if case_path else None
    source = review.get("source") or {}
    lanes = ((review.get("frozen_panel_lanes") or {}).get("lanes")) or []
    rows = []
    for lane in lanes:
        lane_review = lane.get("review") or {}
        progress = ((lane_review.get("facts") or {}).get("diagnostic_progress")) or None
        key_play = first_key_card_play(lane.get("key_card_lifecycle"))
        final_hp = first_present(lane_review.get("final_hp"), get(progress, "final_hp"))
        turns = first_present(lane_review.get("turns"), get(progress, "turns"))
        potions_used = first_present(
            lane_review.get("potions_used"), get(progress, "potions_used")
        )
        half_dead = get(progress, "half_dead_enemy_count")
        row = {
            "case_id": case_id,
            "case_path": case_path,
            "case_origin_seed": source.get("seed"),
            "captured_at_commit": review.get("captured_at_commit"),
            "reviewed_at_commit": reviewed_at_commit,
            "lane": lane.get("lane"),
            "search_config_summary": lane.get("search_config_summary"),
            "complete_win": bool(lane_review.get("complete_win")),
            "outcome_tier": outcome_tier(lane_review, progress, "ok"),
            "final_hp": final_hp,
            "turns": turns,
            "potions_used": potions_used,
            "first_action_key": first_action_key(progress),
            "first_action_role": None,
            "key_card_played": key_play is not None,
            "key_card_first_play_step": get(key_play, "step_index"),
            "living_enemy_count": get(progress, "living_enemy_count"),
            "total_enemy_hp": get(progress, "total_enemy_hp"),
            "half_dead_enemy_count": half_dead,
            "phase_pending_enemy_player_died": bool(
                final_hp is not None and final_hp <= 0 and (half_dead or 0) > 0
            ),
            "nodes_expanded": lane_review.get("nodes_expanded"),
            "elapsed_ms": lane_review.get("elapsed_ms"),
            "deadline_hit": bool(lane_review.get("deadline_hit")),
            "tool_status": "ok",
        }
        rows.append(row)
    return rows


def outcome_tier(
    lane_review: dict[str, Any], progress: dict[str, Any] | None, tool_status: str
) -> str:
    if tool_status != "ok":
        return "malformed_or_tool_failure"
    if lane_review.get("complete_win"):
        return "complete_win"
    final_hp = first_present(lane_review.get("final_hp"), get(progress, "final_hp"))
    half_dead = get(progress, "half_dead_enemy_count") or 0
    turns = first_present(lane_review.get("turns"), get(progress, "turns"))
    if final_hp is not None and final_hp <= 0 and half_dead > 0:
        return "phase_complete_but_player_died"
    if lane_review.get("deadline_hit") and (final_hp is None or final_hp > 0):
        return "survived_to_deadline"
    if final_hp is not None and final_hp <= 0:
        if half_dead == 0 and (turns or 0) >= 2:
            return "died_after_progress"
        return "died_early"
    return "incomplete_or_unknown"


def process_failure_row(
    case_path: Path, reviewed_at_commit: str | None, exit_code: int
) -> dict[str, Any]:
    row = {field: None for field in ROW_FIELDS}
    row.update(
        {
            "case_id": case_path.stem,
            "case_path": str(case_path),
            "reviewed_at_commit": reviewed_at_commit,
            "outcome_tier": "malformed_or_tool_failure",
            "tool_status": f"process_exit_{exit_code}",
        }
    )
    return row


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as handle:
        for row in rows:
            handle.write(json.dumps(row, sort_keys=True) + "\n")


def write_markdown_table(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(render_markdown_table(rows), encoding="utf-8", newline="\n")


def render_markdown_table(rows: list[dict[str, Any]]) -> str:
    columns = [
        "case_id",
        "lane",
        "outcome_tier",
        "complete_win",
        "key_card_played",
        "key_card_first_play_step",
        "final_hp",
        "turns",
        "potions_used",
        "nodes_expanded",
        "elapsed_ms",
        "deadline_hit",
        "tool_status",
    ]
    lines = [
        "| " + " | ".join(columns) + " |",
        "| " + " | ".join("---" for _ in columns) + " |",
    ]
    for row in rows:
        lines.append("| " + " | ".join(markdown_value(row.get(col)) for col in columns) + " |")
    return "\n".join(lines) + "\n"


def markdown_value(value: Any) -> str:
    if value is None:
        return ""
    return str(value).replace("|", "\\|")


def first_action_key(progress: dict[str, Any] | None) -> str | None:
    preview = get(progress, "action_key_preview") or []
    return preview[0] if preview else None


def first_key_card_play(lifecycle: dict[str, Any] | None) -> dict[str, Any] | None:
    tracked_cards = get(lifecycle, "tracked_cards") or []
    plays = [
        card.get("first_play")
        for card in tracked_cards
        if card.get("played_in_replay") and card.get("first_play")
    ]
    return min(plays, key=lambda play: play.get("step_index", 10**9)) if plays else None


def first_present(*values: Any) -> Any:
    for value in values:
        if value is not None:
            return value
    return None


def get(value: dict[str, Any] | None, key: str) -> Any:
    return value.get(key) if isinstance(value, dict) else None


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def git_commit() -> str | None:
    completed = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    if completed.returncode != 0:
        return None
    return completed.stdout.strip()


if __name__ == "__main__":
    raise SystemExit(main())
