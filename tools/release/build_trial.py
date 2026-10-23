"""Build the bilingual trial site with a sealed replay and matching source ZIP.

The site uses the published v0.1.0 Python wheels. Their identity is separate
from this source snapshot and the recording's original game producer.
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
    parser.add_argument("--recording", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="New output directory")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=repo):
        parser.error("commit trial sources before packaging")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
    recording = args.recording.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    site = output / "site"
    site.mkdir()
    # Recording validation and rendering belong to the existing replay owner.
    subprocess.run(["node", "scripts/replay.mjs", "build", "--input", str(recording),
                    "--output", str(site / "replay")], cwd=repo, check=True)
    for path in sorted((repo / "tools/presentation/site").iterdir()):
        if path.suffix not in (".html", ".css"):
            continue
        text = path.read_text(encoding="utf-8").replace("__SOURCE_REVISION__", revision)
        (site / path.name).write_text(text, encoding="utf-8")
    shutil.copy2(repo / "LICENSE", site / "LICENSE")
    source = site / "source/SpireSim-source.zip"
    source.parent.mkdir()
    subprocess.run(["git", "archive", "--format=zip", "--prefix=SpireSim/", "-o",
                    str(source), revision], cwd=repo, check=True)
    record = json.loads((site / "replay/recording.json").read_text(encoding="utf-8"))
    manifest = {
        "schema": "spiresim-trial-v1",
        "source_revision": revision,
        "entry": "index.html",
        "source_archive": source.relative_to(site).as_posix(),
        "python_release": "https://github.com/hewzhew/SpireSim/releases/tag/v0.1.0",
        "recording": {"id": record["id"], "source_revision": record["source_revision"],
                      "manifest_sha256": sha256(recording / "manifest.json"),
                      "files": record["files"]},
        "files": {path.relative_to(site).as_posix(): sha256(path)
                  for path in sorted(site.rglob("*")) if path.is_file()},
    }
    (site / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    files = {path.relative_to(site).as_posix(): sha256(path)
             for path in sorted(site.rglob("*")) if path.is_file()}
    (site / "SHA256SUMS").write_text("".join(f"{digest}  {name}\n" for name, digest in files.items()), encoding="utf-8")
    archive = output / "SpireSim-trial.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zipped:
        for path in sorted(site.rglob("*")):
            if path.is_file():
                zipped.write(path, "SpireSim/" + path.relative_to(site).as_posix())
    report = {"source_revision": revision, "entry": "site/index.html", "archive": archive.name,
              "sha256": sha256(archive), "bytes": archive.stat().st_size,
              "recording_input": str(recording), "remote_modified": False}
    (output / "build.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
