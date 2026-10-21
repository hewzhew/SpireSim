"""Assemble a local release candidate from explicit, verified inputs.

This packages existing evidence and validators. Replay and acceptance semantics
remain in sts_parity_lockstep_runner and tests/trajectory.py.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import zipfile


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--study", type=Path, required=True, help="Verified four-character export directory")
    parser.add_argument("--wheel", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, required=True, help="New candidate directory")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    if (repo / "LICENSE").read_bytes() != (repo / "bindings/python_learning/LICENSE").read_bytes():
        parser.error("Python distribution license differs from the project license")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=repo):
        parser.error("commit the candidate sources before packaging")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
    study = args.study.resolve()
    cases = json.loads((study / "cases.json").read_text(encoding="utf-8"))["cases"]
    if sorted(c["id"] for c in cases) != ["defect", "ironclad", "silent", "watcher"]:
        parser.error("expected one complete export for each character")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    bundle = output / "verification"
    bundle.mkdir()
    records = []
    sources = {}
    for case in cases:
        character = case["id"]
        expected_path = study / character / "expected.json"
        report_path = study / character / "report.json"
        expected = json.loads(expected_path.read_text(encoding="utf-8"))
        report = json.loads(report_path.read_text(encoding="utf-8"))
        if not (report["status"] == "passed" and report["complete_heart_run"]
                and sha256(expected_path) == report["expected_sha256"]
                and report["verification"] == "raw_evidence_replay_and_installed_package"):
            raise ValueError(f"export is not a pinned complete raw-evidence acceptance: {character}")
        destination = bundle / "trajectories" / character
        destination.mkdir(parents=True)
        shutil.copy2(expected_path, destination / "expected.json")
        shutil.copy2(report_path, destination / "export-report.json")
        records.append({"id": character, "session": case["session"], "through": case["through"],
                        "trajectory": f"trajectories/{character}/expected.json",
                        "trajectory_sha256": report["expected_sha256"],
                        "export_report_sha256": sha256(report_path), "semantic_actions": report["semantic_actions"]})
        for scope in expected["sources"]:
            identity = scope["session_id"]
            raw = Path(scope["evidence_directory"])
            if not raw.is_dir() or raw.name != "java-evidence" or raw.parent.name != identity:
                raise ValueError(f"source layout differs: {identity}")
            if identity in sources:
                continue
            relative = Path(".oracle-lab/parity-manual/sessions") / identity
            target = bundle / relative
            target.mkdir(parents=True)
            shutil.copytree(raw, target / "java-evidence")
            # The existing source-chain loader requires the scope directory;
            # live epochs, autosaves and interactive heads are not replay inputs.
            (target / "epochs").mkdir()
            (target / "epochs/.keep").write_bytes(b"")
            root = raw.parent / "rust-root.json"
            if root.exists():
                shutil.copy2(root, target / root.name)
            sources[identity] = {"directory": relative.as_posix(), "java_producer": scope["java_producer"]}
    runner_dir = bundle / "runner/python"
    runner_dir.mkdir(parents=True)
    shutil.copy2(repo / "bindings/python_learning/tests/trajectory.py", runner_dir / "trajectory.py")
    shutil.copy2(repo / "tools/release/verify_bundle.py", bundle / "verify.py")
    shutil.copy2(repo / "bindings/python_learning/examples/policy_loop.py", bundle / "policy_loop.py")
    for name in ("LICENSE", "THIRD_PARTY_NOTICES.md"):
        shutil.copy2(repo / name, bundle / name)
    (bundle / "README.md").write_text(
        "# Four-character full-run verification\n\n"
        "Install the matching release wheel, then run `python verify.py --output results`.\n"
        "This checks four A20 Heart trajectories with three staggered slots per character,\n"
        "actual choose/step, recovery, rejected-input rollback and formal terminal rewards.\n\n"
        "For fresh original-record comparison, build sts_lockstep from the matching source\n"
        "and run `python verify.py --runner PATH_TO_STS_LOCKSTEP --output raw-results`.\n"
        "The bundled raw Java records and typed adopted parent prefixes are used offline;\n"
        "the original game is not started. Original export bytes and source producers are\n"
        "retained. Their historical absolute directory strings are provenance, not input\n"
        "paths used by the portable verifier.\n\n"
        "`policy_loop.py --policy your_module:choose --seeds 42 --character silent` connects\n"
        "your own policy to public NumPy decisions. The callback is choose(batch, schema)\n"
        "and returns one ordinal per pending row. See the source release's getting-started\n"
        "guide for selection prefixes, progress limits and model integration.\n\n"
        f"Candidate source commit: `{revision}`.\n", encoding="utf-8")
    files = {p.relative_to(bundle).as_posix(): sha256(p) for p in sorted(bundle.rglob("*")) if p.is_file()}
    manifest = {"schema": "sts-release-verification-bundle-v1", "source_revision": revision,
                "cases": records, "sources": sources, "files": files}
    (bundle / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    with zipfile.ZipFile(output / "sts-verification-0.1.0.zip", "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for p in sorted(bundle.rglob("*")):
            if p.is_file():
                archive.write(p, "sts-verification/" + p.relative_to(bundle).as_posix())
    for wheel in args.wheel:
        with zipfile.ZipFile(wheel) as archive:
            metadata, = [p for p in archive.namelist() if p.endswith(".dist-info/METADATA")]
            if "License-Expression: MIT" not in archive.read(metadata).decode():
                raise ValueError(f"wheel lacks project MIT metadata: {wheel}")
            if not any(p.endswith("/licenses/LICENSE") for p in archive.namelist()):
                raise ValueError(f"wheel lacks project license text: {wheel}")
            license_name, = [p for p in archive.namelist() if p.endswith("/licenses/LICENSE")]
            if archive.read(license_name) != (repo / "LICENSE").read_bytes():
                raise ValueError(f"wheel license differs from the project license: {wheel}")
        shutil.copy2(wheel, output / wheel.name)
    subprocess.run(["git", "archive", "--format=zip", "--prefix=sts-sim/", "-o",
                    str(output / "sts-sim-0.1.0-source.zip"), revision], cwd=repo, check=True)
    assets = {p.name: sha256(p) for p in sorted(output.iterdir()) if p.is_file()}
    (output / "SHA256SUMS").write_text("".join(f"{digest}  {name}\n" for name, digest in assets.items()), encoding="utf-8")
    release = {"source_revision": revision, "assets": assets, "characters": len(records),
               "semantic_actions": sum(r["semantic_actions"] for r in records), "source_scopes": len(sources)}
    (output / "candidate.json").write_text(json.dumps(release, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(release, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
