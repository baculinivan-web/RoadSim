#!/usr/bin/env python3
"""Regression tests for the SUMO source/build pin guard."""

from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import tomllib
import unittest

from check_sumo_pin import DEFAULT_MANIFEST, validate


def baseline() -> dict[str, object]:
    with Path(DEFAULT_MANIFEST).open("rb") as stream:
        return tomllib.load(stream)


class SumoPinGuardTests(unittest.TestCase):
    def test_committed_pin_is_accepted(self) -> None:
        self.assertEqual(validate(baseline()), [])

    def test_short_source_revision_is_rejected(self) -> None:
        document = deepcopy(baseline())
        document["source_commit"] = "7717f23"
        self.assertIn("source_commit must be a full lowercase Git commit", validate(document))

    def test_optional_gpl_extras_are_rejected(self) -> None:
        document = deepcopy(baseline())
        document["bundle_optional_gpl_extras"] = True
        build = document["build"]
        assert isinstance(build, dict)
        build["build_optional_gpl_extras"] = True
        errors = validate(document)
        self.assertIn("optional GPL extras must not be bundled", errors)
        self.assertIn(
            "build strategy must produce headless libsumo from the exact commit", errors
        )

    def test_missing_supported_target_is_rejected(self) -> None:
        document = deepcopy(baseline())
        targets = document["targets"]
        assert isinstance(targets, list)
        targets.pop()
        self.assertIn("target matrix does not match NFR-030", validate(document))


if __name__ == "__main__":
    unittest.main()
