from __future__ import annotations

import os
import subprocess
import sys
import unittest


class OptionalDependencyBoundaryTests(unittest.TestCase):
    def test_package_root_does_not_import_torch(self) -> None:
        script = """
import importlib.abc
import sys

class RejectTorch(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path, target=None):
        if fullname == "torch" or fullname.startswith("torch."):
            raise RuntimeError("ordinary sts_learning import reached PyTorch")
        return None

sys.meta_path.insert(0, RejectTorch())
import sts_learning
assert "torch" not in sys.modules
"""
        completed = subprocess.run(
            [sys.executable, "-c", script],
            env=os.environ.copy(),
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)


if __name__ == "__main__":
    unittest.main()
