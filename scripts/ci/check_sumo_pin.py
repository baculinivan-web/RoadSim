#!/usr/bin/env python3
"""Validate the exact SUMO source/build/license pin used by worker packaging."""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = ROOT / "supply-chain" / "sumo-engine.toml"
NATIVE_CMAKE = ROOT / "workers" / "roadsim-sumo-worker" / "native" / "CMakeLists.txt"
RUST_PIN = ROOT / "workers" / "roadsim-sumo-worker" / "src" / "pin.rs"
EXPECTED_TARGETS = {
    ("windows", "x86_64", "Windows 11"),
    ("macos", "aarch64", "macOS 14"),
    ("macos", "x86_64", "macOS 14"),
    ("linux", "x86_64", "Ubuntu 24.04 LTS"),
}
ROOT_KEYS = {
    "schema_version",
    "engine_name",
    "engine_version",
    "source_repository",
    "source_tag",
    "source_commit",
    "license_expression",
    "distribution_status",
    "bundle_optional_gpl_extras",
    "build",
    "targets",
}
BUILD_KEYS = {
    "strategy",
    "cmake_build_type",
    "build_libsumo",
    "build_gui",
    "build_optional_gpl_extras",
}
TARGET_KEYS = {"os", "architecture", "minimum"}


def validate(document: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if set(document) != ROOT_KEYS:
        errors.append("root fields differ from the closed schema")
    version = document.get("engine_version")
    if document.get("schema_version") != 1:
        errors.append("schema_version must equal 1")
    if document.get("engine_name") != "eclipse.sumo":
        errors.append("engine_name must equal eclipse.sumo")
    if not isinstance(version, str) or re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) is None:
        errors.append("engine_version must be an exact three-part release")
    else:
        expected_tag = f"v{version.replace('.', '_')}"
        if document.get("source_tag") != expected_tag:
            errors.append(f"source_tag must equal {expected_tag}")
    if document.get("source_repository") != "https://github.com/eclipse-sumo/sumo.git":
        errors.append("source_repository must be the official HTTPS repository")
    commit = document.get("source_commit")
    if not isinstance(commit, str) or re.fullmatch(r"[0-9a-f]{40}", commit) is None:
        errors.append("source_commit must be a full lowercase Git commit")
    if document.get("license_expression") != "EPL-2.0":
        errors.append("license_expression must equal EPL-2.0")
    if document.get("distribution_status") != "review_pending_e10_t12":
        errors.append("distribution must remain pending until E10-T12")
    if document.get("bundle_optional_gpl_extras") is not False:
        errors.append("optional GPL extras must not be bundled")

    build = document.get("build")
    if not isinstance(build, dict) or set(build) != BUILD_KEYS:
        errors.append("build fields differ from the closed schema")
    else:
        expected_build = {
            "strategy": "source_from_exact_commit",
            "cmake_build_type": "Release",
            "build_libsumo": True,
            "build_gui": False,
            "build_optional_gpl_extras": False,
        }
        if build != expected_build:
            errors.append("build strategy must produce headless libsumo from the exact commit")

    targets = document.get("targets")
    observed_targets: set[tuple[str, str, str]] = set()
    if not isinstance(targets, list):
        errors.append("targets must be an array")
    else:
        for target in targets:
            if not isinstance(target, dict) or set(target) != TARGET_KEYS:
                errors.append("target fields differ from the closed schema")
                continue
            if not all(isinstance(target[key], str) for key in TARGET_KEYS):
                errors.append("target values must be strings")
                continue
            observed_targets.add(
                (target["os"], target["architecture"], target["minimum"])
            )
        if len(observed_targets) != len(targets):
            errors.append("target matrix contains duplicates")
        if observed_targets != EXPECTED_TARGETS:
            errors.append("target matrix does not match NFR-030")
    return errors


def validate_native_bridge(
    document: dict[str, Any], cmake_text: str, rust_pin_text: str
) -> list[str]:
    errors: list[str] = []
    version = document.get("engine_version")
    revision = document.get("source_commit")
    if not isinstance(version, str) or f'"{version}" CACHE STRING' not in cmake_text:
        errors.append("native bridge version differs from engine pin")
    if not isinstance(revision, str) or f'"{revision}"' not in cmake_text:
        errors.append("native bridge revision differs from engine pin")
    if not isinstance(version, str) or f'ENGINE_VERSION: &str = "{version}"' not in rust_pin_text:
        errors.append("Rust worker version differs from engine pin")
    if not isinstance(revision, str) or f'SOURCE_REVISION: &str = "{revision}"' not in rust_pin_text:
        errors.append("Rust worker revision differs from engine pin")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    arguments = parser.parse_args()
    try:
        with arguments.manifest.open("rb") as stream:
            document = tomllib.load(stream)
    except (OSError, tomllib.TOMLDecodeError) as error:
        print(f"SUMO-PIN-INVALID: cannot read manifest: {error}", file=sys.stderr)
        return 1
    try:
        cmake_text = NATIVE_CMAKE.read_text(encoding="utf-8")
        rust_pin_text = RUST_PIN.read_text(encoding="utf-8")
    except OSError as error:
        print(f"SUMO-PIN-INVALID: cannot read native bridge config: {error}", file=sys.stderr)
        return 1
    errors = validate(document) + validate_native_bridge(document, cmake_text, rust_pin_text)
    if errors:
        for error in errors:
            print(f"SUMO-PIN-INVALID: {error}", file=sys.stderr)
        return 1
    print(
        "SUMO pin accepted "
        f"({document['engine_version']}@{document['source_commit']}; "
        f"{len(document['targets'])} targets)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
