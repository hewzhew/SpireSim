"""Attach coarse outcome labels to RLDS-style branch datasets.

This is deliberately not a causal evaluator. A selected decision inherits the
episode's later outcome so we can review correlations such as "these picks often
ended in combat_gap" without claiming the pick caused that outcome.
"""

from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path
from statistics import mean
from typing import Any


def main() -> int:
    args = parse_args()
    inputs = resolve_inputs(args)
    episodes, decisions = collect_rows(inputs)
    report = build_report(
        inputs=inputs,
        episodes=episodes,
        decisions=decisions,
        min_count=args.min_count,
    )
    write_jsonl(args.episode_out, episodes)
    write_jsonl(args.decision_out, decisions)
    if args.report_out:
        write_json(args.report_out, report)
    if args.markdown_out:
        args.markdown_out.parent.mkdir(parents=True, exist_ok=True)
        args.markdown_out.write_text(render_markdown(report), encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Label RLDS episode and selected-decision rows with later outcomes."
    )
    parser.add_argument("--input", type=Path, nargs="+", help="RLDS dataset JSON files.")
    parser.add_argument("--manifest", type=Path, help="Optional rl_dataset_manifest_v0.")
    parser.add_argument(
        "--manifest-split",
        choices=["train", "eval", "all"],
        default="all",
        help="Which manifest split to label when --manifest is used.",
    )
    parser.add_argument(
        "--episode-out",
        type=Path,
        required=True,
        help="Output JSONL with one row per episode.",
    )
    parser.add_argument(
        "--decision-out",
        type=Path,
        required=True,
        help="Output JSONL with one row per selected decision.",
    )
    parser.add_argument("--report-out", type=Path, help="Optional JSON summary output.")
    parser.add_argument("--markdown-out", type=Path, help="Optional Markdown summary output.")
    parser.add_argument(
        "--min-count",
        type=int,
        default=2,
        help="Minimum selected-label count to include in aggregate tables.",
    )
    return parser.parse_args()


def resolve_inputs(args: argparse.Namespace) -> list[Path]:
    if args.manifest:
        if args.input:
            raise SystemExit("--manifest cannot be combined with --input")
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
        if manifest.get("schema") != "rl_dataset_manifest_v0":
            raise SystemExit(f"unsupported manifest schema: {manifest.get('schema')}")
        splits = ["train", "eval"] if args.manifest_split == "all" else [args.manifest_split]
        paths = []
        for split in splits:
            datasets = ((manifest.get("splits") or {}).get(split) or {}).get("datasets") or []
            paths.extend(Path(dataset["path"]) for dataset in datasets if dataset.get("path"))
        return paths
    if not args.input:
        raise SystemExit("provide --input or --manifest")
    return args.input


def collect_rows(inputs: list[Path]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    episodes: list[dict[str, Any]] = []
    decisions: list[dict[str, Any]] = []
    for path in inputs:
        data = json.loads(path.read_text(encoding="utf-8"))
        for episode in data.get("episodes", []):
            episode_row = episode_labels(path, episode)
            episodes.append(episode_row)
            decisions.extend(decision_rows(path, episode, episode_row))
    return episodes, decisions


def episode_labels(path: Path, episode: dict[str, Any]) -> dict[str, Any]:
    metadata = episode.get("episode_metadata") or {}
    final_outcome = metadata.get("final_outcome") or {}
    if not isinstance(final_outcome, dict):
        final_outcome = {}
    status = final_outcome.get("status") or {}
    if not isinstance(status, dict):
        status = {}
    combat_summary = metadata.get("episode_combat_summary") or {}
    if not isinstance(combat_summary, dict):
        combat_summary = {}
    high_loss_attempts = combat_summary.get("high_hp_loss_attempts") or []
    if not isinstance(high_loss_attempts, list):
        high_loss_attempts = []

    final_act = int_field(final_outcome, "act")
    final_floor = int_field(final_outcome, "floor")
    final_hp = int_field(final_outcome, "hp")
    final_max_hp = int_field(final_outcome, "max_hp")
    blocker_kind = text_field(final_outcome, "blocker_kind")
    status_kind = text_field(status, "kind")
    reason = text_field(status, "reason") or text_field(final_outcome, "reason")
    final_hp_ratio_bp = ratio_basis_points(final_hp, final_max_hp)
    high_loss_values = [
        value
        for value in (int_field(attempt, "hp_loss") for attempt in high_loss_attempts)
        if value is not None
    ]

    flags = outcome_flags(
        final_act=final_act,
        final_floor=final_floor,
        final_hp_ratio_bp=final_hp_ratio_bp,
        blocker_kind=blocker_kind,
        status_kind=status_kind,
        reason=reason,
        high_hp_loss_count=len(high_loss_attempts),
    )
    return {
        "schema": "rl_episode_outcome_labels_v0",
        "dataset_path": str(path),
        "episode_id": episode.get("episode_id"),
        "seed": episode.get("seed"),
        "branch_id": episode.get("branch_id"),
        "final_act": final_act,
        "final_floor": final_floor,
        "final_hp": final_hp,
        "final_max_hp": final_max_hp,
        "final_hp_ratio_bp": final_hp_ratio_bp,
        "final_blocker_kind": blocker_kind,
        "final_status_kind": status_kind,
        "final_reason": reason,
        "progress_score": progress_score(final_act, final_floor),
        "high_hp_loss_attempt_count": len(high_loss_attempts),
        "max_high_hp_loss": max(high_loss_values) if high_loss_values else None,
        "combat_attempt_count": int_field(combat_summary, "attempt_count"),
        "outcome_flags": flags,
    }


def outcome_flags(
    *,
    final_act: int | None,
    final_floor: int | None,
    final_hp_ratio_bp: int | None,
    blocker_kind: str | None,
    status_kind: str | None,
    reason: str | None,
    high_hp_loss_count: int,
) -> list[str]:
    flags = []
    if final_act is not None and final_act >= 2:
        flags.append("reached_act2")
    if final_act is not None and final_act >= 3:
        flags.append("reached_act3")
    if final_floor is not None and final_floor >= 16:
        flags.append("reached_act1_boss_floor")
    if final_hp_ratio_bp is not None and final_hp_ratio_bp <= 3500:
        flags.append("low_final_hp")
    if high_hp_loss_count > 0:
        flags.append("high_hp_loss_observed")
    if blocker_kind:
        flags.append(f"blocker:{blocker_kind}")
    if status_kind and status_kind != blocker_kind:
        flags.append(f"status:{status_kind}")
    if blocker_kind == "combat_gap":
        flags.append("ended_in_combat_gap")
    if blocker_kind == "running" or status_kind == "running":
        flags.append("ended_running_or_soft_paused")
    if reason and ("owner " in reason or "MissingMarkedPolicy" in reason):
        flags.append("ended_in_owner_gap")
    return flags


def decision_rows(
    path: Path, episode: dict[str, Any], episode_row: dict[str, Any]
) -> list[dict[str, Any]]:
    rows = []
    for step in episode.get("steps", []):
        if step.get("is_last"):
            continue
        metadata = step.get("step_metadata") or {}
        action = step.get("action") or {}
        selected = action.get("features_v0") or metadata.get("selected_action_features_v0") or {}
        observation = metadata.get("observation_features_v0") or {}
        rows.append(
            {
                "schema": "rl_selected_decision_outcome_labels_v0",
                "dataset_path": str(path),
                "episode_id": episode.get("episode_id"),
                "seed": episode.get("seed"),
                "branch_id": episode.get("branch_id"),
                "step_index": metadata.get("t"),
                "context": decision_context(observation),
                "selected": selected_summary(action, selected),
                "episode_outcome": episode_row,
            }
        )
    return rows


def decision_context(observation: dict[str, Any]) -> dict[str, Any]:
    return {
        "act": observation.get("act"),
        "floor": observation.get("floor"),
        "hp": observation.get("hp"),
        "max_hp": observation.get("max_hp"),
        "hp_ratio_bp": observation.get("hp_ratio_bp"),
        "gold": observation.get("gold"),
        "deck_size": observation.get("deck_size"),
        "boss": observation.get("boss"),
        "boundary_kind": observation.get("boundary_kind"),
        "floors_to_act_boss": observation.get("floors_to_act_boss"),
    }


def selected_summary(action: dict[str, Any], features: dict[str, Any]) -> dict[str, Any]:
    return {
        "index": action.get("index"),
        "label": action.get("label") or features.get("label"),
        "kind": features.get("kind"),
        "card_id": features.get("card_id"),
        "relic_id": features.get("relic_id"),
        "potion_id": features.get("potion_id"),
        "event_id": features.get("event_id"),
        "price": features.get("price"),
        "is_skip": features.get("is_skip"),
        "is_buy": features.get("is_buy"),
        "is_remove": features.get("is_remove"),
        "is_pick": features.get("is_pick"),
    }


def build_report(
    *,
    inputs: list[Path],
    episodes: list[dict[str, Any]],
    decisions: list[dict[str, Any]],
    min_count: int,
) -> dict[str, Any]:
    return {
        "schema": "rl_outcome_label_report_v0",
        "inputs": [str(path) for path in inputs],
        "episode_count": len(episodes),
        "decision_count": len(decisions),
        "episode_summary": summarize_episode_rows(episodes),
        "decision_summary": summarize_decision_rows(decisions, min_count=min_count),
    }


def summarize_episode_rows(rows: list[dict[str, Any]]) -> dict[str, Any]:
    blocker = Counter(row.get("final_blocker_kind") for row in rows)
    flags = Counter(flag for row in rows for flag in row.get("outcome_flags") or [])
    progress_values = [row.get("progress_score") for row in rows if row.get("progress_score") is not None]
    final_floor_values = [row.get("final_floor") for row in rows if row.get("final_floor") is not None]
    high_loss_values = [row.get("high_hp_loss_attempt_count") or 0 for row in rows]
    return {
        "by_final_blocker_kind": counter_top(blocker),
        "by_outcome_flag": counter_top(flags, limit=20),
        "mean_progress_score": mean(progress_values) if progress_values else None,
        "mean_final_floor": mean(final_floor_values) if final_floor_values else None,
        "mean_high_hp_loss_attempt_count": mean(high_loss_values) if high_loss_values else None,
    }


def summarize_decision_rows(rows: list[dict[str, Any]], *, min_count: int) -> dict[str, Any]:
    by_boundary = Counter(row.get("context", {}).get("boundary_kind") for row in rows)
    by_selected_kind = Counter(row.get("selected", {}).get("kind") for row in rows)
    return {
        "by_boundary_kind": counter_top(by_boundary),
        "by_selected_kind": counter_top(by_selected_kind),
        "by_selected_label": selected_label_rows(rows, min_count=min_count),
    }


def selected_label_rows(rows: list[dict[str, Any]], *, min_count: int) -> list[dict[str, Any]]:
    grouped: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for row in rows:
        selected = row.get("selected") or {}
        grouped[(str(selected.get("kind")), str(selected.get("label")))].append(row)
    summaries = []
    for (kind, label), group in grouped.items():
        if len(group) < min_count:
            continue
        progress = [
            row.get("episode_outcome", {}).get("progress_score")
            for row in group
            if row.get("episode_outcome", {}).get("progress_score") is not None
        ]
        high_loss = [
            row.get("episode_outcome", {}).get("high_hp_loss_attempt_count") or 0
            for row in group
        ]
        blocker = Counter(
            row.get("episode_outcome", {}).get("final_blocker_kind") for row in group
        )
        summaries.append(
            {
                "selected_kind": kind,
                "selected_label": label,
                "count": len(group),
                "mean_progress_score": mean(progress) if progress else None,
                "mean_high_hp_loss_attempt_count": mean(high_loss) if high_loss else None,
                "by_final_blocker_kind": counter_top(blocker, limit=5),
            }
        )
    summaries.sort(key=lambda row: (-row["count"], str(row["selected_label"])))
    return summaries


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# RL Outcome Label Report",
        "",
        f"- episodes: {report['episode_count']}",
        f"- selected decisions: {report['decision_count']}",
        "",
        "## Episode Blockers",
        "",
    ]
    for row in report.get("episode_summary", {}).get("by_final_blocker_kind", []):
        lines.append(f"- {row['key']}: {row['count']}")
    lines.extend(
        [
            "",
            "## Selected Label Aggregates",
            "",
            "| selected | count | mean progress | mean high-loss combats | blockers |",
            "| --- | ---: | ---: | ---: | --- |",
        ]
    )
    for row in report.get("decision_summary", {}).get("by_selected_label", [])[:40]:
        blockers = ", ".join(
            f"{item['key']}={item['count']}" for item in row.get("by_final_blocker_kind") or []
        )
        lines.append(
            "| {label} | {count} | {progress} | {loss} | {blockers} |".format(
                label=markdown_cell(row.get("selected_label")),
                count=row.get("count"),
                progress=number_cell(row.get("mean_progress_score")),
                loss=number_cell(row.get("mean_high_hp_loss_attempt_count")),
                blockers=markdown_cell(blockers),
            )
        )
    lines.append("")
    return "\n".join(lines)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, sort_keys=True) + "\n")


def counter_top(counter: Counter, limit: int = 10) -> list[dict[str, Any]]:
    return [{"key": str(key), "count": count} for key, count in counter.most_common(limit)]


def progress_score(act: int | None, floor: int | None) -> int | None:
    if act is None or floor is None:
        return None
    return act * 100 + floor


def ratio_basis_points(numerator: int | None, denominator: int | None) -> int | None:
    if numerator is None or denominator is None or denominator == 0:
        return None
    return round(numerator * 10_000 / denominator)


def int_field(value: dict[str, Any], key: str) -> int | None:
    raw = value.get(key)
    if isinstance(raw, bool):
        return None
    if isinstance(raw, int):
        return raw
    if isinstance(raw, float):
        return int(raw)
    return None


def text_field(value: dict[str, Any], key: str) -> str | None:
    raw = value.get(key)
    return raw if isinstance(raw, str) else None


def markdown_cell(value: Any) -> str:
    return str(value).replace("|", "\\|").replace("\n", " ")


def number_cell(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, float):
        return f"{value:.2f}"
    return str(value)


if __name__ == "__main__":
    raise SystemExit(main())
