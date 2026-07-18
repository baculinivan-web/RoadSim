#!/usr/bin/env python3
"""Regression tests for the RoadSim dependency graph guard."""

from __future__ import annotations

import subprocess
from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[2]
GUARD = ROOT / "scripts" / "ci" / "check_dependency_graph.py"
POLICY = ROOT / "supply-chain" / "dependency-policy.toml"
FIXTURES = ROOT / "fixtures" / "dependency-graph"


class DependencyGraphGuardTests(unittest.TestCase):
    def run_guard(self, fixture: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(GUARD),
                "--metadata",
                str(FIXTURES / fixture),
                "--policy",
                str(POLICY),
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_allowed_core_dependency_is_accepted(self) -> None:
        result = self.run_guard("allowed-domain-types.json")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("dependency graph accepted", result.stdout)

    def test_domain_to_ui_regression_is_rejected(self) -> None:
        result = self.run_guard("forbidden-domain-ui.json")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("ARCH-FORBIDDEN-DEPENDENCY", result.stderr)
        self.assertIn("roadsim-domain -> roadsim-editor-ui", result.stderr)
        self.assertIn("roadsim-domain -> roadsim-helper -> roadsim-editor-ui", result.stderr)
        self.assertIn("domain-is-independent", result.stderr)


if __name__ == "__main__":
    unittest.main()
