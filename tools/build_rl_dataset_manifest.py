#!/usr/bin/env python3
"""Build a small manifest for RLDS-style run datasets.

The manifest records which exported datasets belong to train/eval, basic
episode/step/candidate counts, and git provenance. It does not inspect policy
quality and does not change any training data.
"""

from __future__ import annotations

import argparse
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def main() -> int:
    args = parse_args()
    manifest = {
        "schema": "rl_dataset_manifest_v0",
        "dataset_id": args.dataset_id,
        "created_at_utc": datetime.now(timezone.utc).isoformat(),
        "repo": repo_provenance(),
        "generator": {
            "notes": args.notes,
            "command": args.generator_command,
        },
        "splits": {
            "train": {
                "datasets": [dataset_summary(path) for path in args.train],
            },
            "eval": {
                "datasets": [dataset_summary(path) for path in args.eval],
            },
        },
    }
    manifest["totals"] = {
        "train": split_totals(manifest["splits"]["train"]["datasets"]),
        "eval": split_totals(manifest["splits"]["eval"]["datasets"]),
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote RL dataset manifest to {args.out}")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build rl_dataset_manifest_v0.")
    parser.add_argument("--dataset-id", required=True)
    parser.add_argument("--train", type=Path, nargs="+", required=True)
    parser.add_argument("--eval", type=Path, nargs="*", default=[])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--notes", default="")
    parser.add_argument(
        "--generator-command",
        default="cargo run -p sts_oracle_tools --bin rl_dataset_export -- --input <session-trace> --out <dataset-json>",
    )
    return parser.parse_args()


def dataset_summary(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    episodes = data.get("episodes") or []
    step_count = 0
    decision_step_count = 0
    candidate_count = 0
    terminal_step_count = 0
    truncated_step_count = 0
    for episode in episodes:
        for step in episode.get("steps") or []:
            step_count += 1
            if step.get("is_last"):
                terminal_step_count += int(bool(step.get("is_terminal")))
                truncated_step_count += int(not bool(step.get("is_terminal")))
                continue
            candidates = (step.get("step_metadata") or {}).get("action_candidates_v0") or []
            if candidates:
                decision_step_count += 1
                candidate_count += len(candidates)

    return {
        "path": str(path),
        "schema": data.get("schema"),
        "format_basis": data.get("format_basis"),
        "input_path": data.get("input_path"),
        "source_file_count": (data.get("metadata") or {}).get("source_file_count"),
        "episode_count": len(episodes),
        "step_count": step_count,
        "decision_step_count": decision_step_count,
        "candidate_count": candidate_count,
        "terminal_step_count": terminal_step_count,
        "truncated_step_count": truncated_step_count,
    }


def split_totals(datasets: list[dict[str, Any]]) -> dict[str, int]:
    keys = [
        "source_file_count",
        "episode_count",
        "step_count",
        "decision_step_count",
        "candidate_count",
        "terminal_step_count",
        "truncated_step_count",
    ]
    return {
        key: sum(int(dataset.get(key) or 0) for dataset in datasets)
        for key in keys
    }


def repo_provenance() -> dict[str, Any]:
    commit = git_output(["rev-parse", "HEAD"])
    status = git_output(["status", "--short"])
    return {
        "commit": commit,
        "dirty": bool(status),
        "dirty_status": status.splitlines() if status else [],
    }


def git_output(args: list[str]) -> str:
    try:
        return subprocess.check_output(["git", *args], text=True, stderr=subprocess.DEVNULL).strip()
    except Exception:
        return ""


if __name__ == "__main__":
    raise SystemExit(main())
