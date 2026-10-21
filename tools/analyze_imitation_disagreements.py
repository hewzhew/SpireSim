"""Summarize model-vs-policy disagreements from imitation scored JSONL.

This is an offline review helper. It does not train a model and does not feed
model preferences back into the runner. Its job is to make disagreement
patterns visible alongside final run outcomes.
"""

from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


def main() -> int:
    args = parse_args()
    rows = read_jsonl(args.scored)
    report = build_report(
        rows,
        scored_input=str(args.scored),
        high_margin_limit=args.high_margin_limit,
    )
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if args.markdown_out:
        args.markdown_out.parent.mkdir(parents=True, exist_ok=True)
        args.markdown_out.write_text(render_markdown(report), encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Summarize policy/model disagreements from scored imitation rows."
    )
    parser.add_argument(
        "--scored",
        type=Path,
        required=True,
        help="JSONL produced by train_imitation_candidate_ranker.py --scored-out.",
    )
    parser.add_argument("--out", type=Path, help="Optional JSON report output.")
    parser.add_argument("--markdown-out", type=Path, help="Optional Markdown table output.")
    parser.add_argument("--high-margin-limit", type=int, default=20)
    return parser.parse_args()


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    with path.open(encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def build_report(
    rows: list[dict[str, Any]], *, scored_input: str, high_margin_limit: int
) -> dict[str, Any]:
    groups: dict[tuple[str, str, int], list[dict[str, Any]]] = defaultdict(list)
    for row in rows:
        groups[
            (
                str(row.get("dataset_path")),
                str(row.get("episode_id")),
                int(row.get("step_index") or 0),
            )
        ].append(row)

    disagreements = []
    agreements = 0
    for key, group in groups.items():
        selected = next((row for row in group if row.get("selected")), None)
        if selected is None:
            continue
        predicted = max(group, key=lambda row: float(row.get("model_prob_selected") or 0.0))
        if predicted.get("candidate_index") == selected.get("candidate_index"):
            agreements += 1
            continue
        disagreements.append(disagreement_row(key, selected, predicted))

    return {
        "schema": "imitation_disagreement_outcome_report_v0",
        "scored_input": scored_input,
        "decision_step_count": agreements + len(disagreements),
        "agreement_count": agreements,
        "disagreement_count": len(disagreements),
        "agreement_rate": ratio(agreements, agreements + len(disagreements)),
        "summary": summarize(disagreements),
        "high_margin_disagreements": sorted(
            disagreements, key=lambda row: row["prob_margin"], reverse=True
        )[:high_margin_limit],
    }


def disagreement_row(
    key: tuple[str, str, int], selected: dict[str, Any], predicted: dict[str, Any]
) -> dict[str, Any]:
    selected_candidate = candidate(selected)
    predicted_candidate = candidate(predicted)
    context = selected.get("display", {}).get("context") or {}
    predicted_prob = float(predicted.get("model_prob_selected") or 0.0)
    selected_prob = float(selected.get("model_prob_selected") or 0.0)
    return {
        "dataset_path": key[0],
        "episode_id": key[1],
        "step_index": key[2],
        "context": context,
        "direction": direction(selected_candidate, predicted_candidate),
        "selected_kind": selected_candidate.get("kind"),
        "selected_label": selected_candidate.get("label"),
        "predicted_kind": predicted_candidate.get("kind"),
        "predicted_label": predicted_candidate.get("label"),
        "selected_prob": selected_prob,
        "predicted_prob": predicted_prob,
        "prob_margin": predicted_prob - selected_prob,
    }


def candidate(row: dict[str, Any]) -> dict[str, Any]:
    return ((row.get("display") or {}).get("candidate") or {})


def direction(selected: dict[str, Any], predicted: dict[str, Any]) -> str:
    selected_skip = is_skip_like(selected.get("kind"))
    predicted_skip = is_skip_like(predicted.get("kind"))
    if predicted_skip and not selected_skip:
        return "model_prefers_skip"
    if selected_skip and not predicted_skip:
        return "model_prefers_action"
    return "model_prefers_different_action"


def is_skip_like(kind: Any) -> bool:
    if not isinstance(kind, str):
        return False
    return "Skip" in kind or "Leave" in kind


def summarize(disagreements: list[dict[str, Any]]) -> dict[str, Any]:
    by_boundary = Counter()
    by_final_blocker = Counter()
    by_direction = Counter()
    by_kind_pair = Counter()
    by_label_pair = Counter()
    skip_pressure = Counter()
    action_pressure = Counter()
    for row in disagreements:
        context = row.get("context") or {}
        by_boundary[str(context.get("boundary_kind"))] += 1
        by_final_blocker[str(context.get("final_blocker_kind"))] += 1
        by_direction[row["direction"]] += 1
        by_kind_pair[f"{row['selected_kind']} -> {row['predicted_kind']}"] += 1
        by_label_pair[f"{row['selected_label']} -> {row['predicted_label']}"] += 1
        if row["direction"] == "model_prefers_skip":
            skip_pressure[str(row.get("selected_label"))] += 1
        elif row["direction"] == "model_prefers_action":
            action_pressure[str(row.get("predicted_label"))] += 1

    return {
        "by_boundary_kind": counter_top(by_boundary),
        "by_final_blocker_kind": counter_top(by_final_blocker),
        "by_direction": counter_top(by_direction),
        "selected_to_predicted_kind": counter_top(by_kind_pair),
        "selected_to_predicted_label": counter_top(by_label_pair, limit=15),
        "skip_pressure_selected_label": counter_top(skip_pressure, limit=15),
        "action_pressure_predicted_label": counter_top(action_pressure, limit=15),
    }


def counter_top(counter: Counter, limit: int = 10) -> list[dict[str, Any]]:
    return [{"key": key, "count": count} for key, count in counter.most_common(limit)]


def ratio(numerator: int, denominator: int) -> float | None:
    if denominator == 0:
        return None
    return numerator / denominator


def render_markdown(report: dict[str, Any]) -> str:
    lines = [
        "# Imitation Disagreement Outcome Report",
        "",
        f"- decision steps: {report['decision_step_count']}",
        f"- disagreements: {report['disagreement_count']}",
        f"- agreement rate: {report['agreement_rate']:.3f}",
        "",
        "## High Margin Disagreements",
        "",
        "| step | boundary | final | selected | predicted | margin |",
        "| --- | --- | --- | --- | --- | ---: |",
    ]
    for row in report.get("high_margin_disagreements", []):
        context = row.get("context") or {}
        lines.append(
            "| {episode}:{step} | {boundary} | {final} | {selected} | {predicted} | {margin:.3f} |".format(
                episode=row.get("episode_id"),
                step=row.get("step_index"),
                boundary=context.get("boundary_kind"),
                final=context.get("final_blocker_kind"),
                selected=markdown_cell(row.get("selected_label")),
                predicted=markdown_cell(row.get("predicted_label")),
                margin=row.get("prob_margin") or 0.0,
            )
        )
    lines.append("")
    return "\n".join(lines)


def markdown_cell(value: Any) -> str:
    return str(value).replace("|", "\\|").replace("\n", " ")


if __name__ == "__main__":
    raise SystemExit(main())
