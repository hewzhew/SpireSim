"""Frozen trajectory exports must be explicitly pinned before execution."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


RUNNER = Path(__file__).with_name("trajectory.py")


class TrajectorySourceTests(unittest.TestCase):
    def test_unpinned_export_is_rejected_before_creating_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            result = subprocess.run([
                sys.executable, str(RUNNER), "--trajectory", str(root / "missing.json"),
                "--output", str(root / "result"),
            ], capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(2, result.returncode)
            self.assertIn("--trajectory requires", result.stderr)
            self.assertFalse((root / "result").exists())

    def test_changed_export_stops_before_replay_or_environment_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exported = root / "expected.json"
            exported.write_text('{"schema":"changed"}', encoding="utf-8")
            result = subprocess.run([
                sys.executable, str(RUNNER), "--trajectory", str(exported),
                "--trajectory-sha256", "0" * 64,
                "--runner", str(root / "nonexistent-runner"),
                "--output", str(root / "result"),
            ], capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(1, result.returncode, result.stdout + result.stderr)
            report = json.loads((root / "result/report.json").read_text(encoding="utf-8"))
            self.assertIn("SHA-256 differs", report["error"])
            self.assertFalse((root / "result/actual.jsonl").exists())
            self.assertFalse((root / "result/replay.log").exists())


if __name__ == "__main__":
    unittest.main()
