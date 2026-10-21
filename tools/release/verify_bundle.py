"""Verify a downloaded full-run bundle using the installed environment.

By default no checkout, replay executable or Java installation is used.
Pass --runner /path/to/sts_lockstep to additionally replay the original raw
Java records through the current Rust comparator before Python execution.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New result directory")
    parser.add_argument("--character", choices=["ironclad", "silent", "defect", "watcher"])
    parser.add_argument("--runner", type=Path, help="Optional current sts_lockstep binary for fresh raw comparison")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    if manifest["schema"] != "sts-release-verification-bundle-v1":
        parser.error("unknown bundle format")
    for name, digest in manifest["files"].items():
        path = (root / name).resolve()
        if not path.is_relative_to(root) or sha256(path) != digest:
            parser.error(f"bundle file differs: {name}")
    args.output.mkdir(parents=True, exist_ok=False)
    runner = args.runner.resolve() if args.runner else None
    results = []
    for case in manifest["cases"]:
        if args.character and args.character != case["id"]:
            continue
        output = args.output.resolve() / case["id"]
        command = [sys.executable, str(root / "runner/python/trajectory.py"),
                   "--output", str(output), "--require-build-profile", "release",
                   "--stagger-commits", "0", "20", "1", "--reject-extra-input-once"]
        if runner:
            command += ["--session", case["session"], "--through", str(case["through"]),
                        "--repository", str(root), "--runner", str(runner)]
        else:
            command += ["--trajectory", str(root / case["trajectory"]),
                        "--trajectory-sha256", case["trajectory_sha256"]]
        with (args.output / (case["id"] + ".log")).open("w", encoding="utf-8") as log:
            completed = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=900)
        report_path = output / "report.json"
        report = json.loads(report_path.read_text(encoding="utf-8")) if report_path.exists() else {}
        passed = (completed.returncode == 0 and report.get("status") == "passed"
                  and report.get("semantic_actions") == case["semantic_actions"]
                  and report.get("complete_heart_run") is True
                  and report.get("slot_rewards") == [1, 1, 1])
        record = {"character": case["id"], "status": "passed" if passed else "failed",
                  "semantic_actions": report.get("semantic_actions"), "report": str(report_path),
                  "error": report.get("error"), "mode": "raw_and_package" if runner else "installed_package"}
        results.append(record)
        print(json.dumps(record), flush=True)
        (args.output / "summary.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    return 0 if results and all(r["status"] == "passed" for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
