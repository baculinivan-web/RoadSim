#!/usr/bin/env python3
"""Verify that CI and local checks use RoadSim's exact pinned Rust toolchain."""

from __future__ import annotations

from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]


def command_version(command: list[str]) -> str:
    return subprocess.run(command, check=True, capture_output=True, text=True).stdout.strip()


def main() -> int:
    toolchain_text = (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8")
    manifest_text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    channel_match = re.search(r'^channel\s*=\s*"([^"]+)"', toolchain_text, re.MULTILINE)
    rust_version_match = re.search(
        r'^rust-version\s*=\s*"([^"]+)"', manifest_text, re.MULTILINE
    )
    if channel_match is None or rust_version_match is None:
        print("TOOLCHAIN-CONFIG-INVALID: missing channel or workspace rust-version", file=sys.stderr)
        return 1
    channel = channel_match.group(1)
    rust_version = rust_version_match.group(1)

    rustc = command_version(["rustc", "--version"])
    cargo = command_version(["cargo", "--version"])
    match = re.match(r"rustc (\d+\.\d+\.\d+)", rustc)
    if match is None or match.group(1) != channel:
        print(f"TOOLCHAIN-VERSION-MISMATCH: expected rustc {channel}, got {rustc}", file=sys.stderr)
        return 1
    if not channel.startswith(f"{rust_version}."):
        print(
            f"TOOLCHAIN-MSRV-MISMATCH: channel {channel} and workspace rust-version {rust_version} differ",
            file=sys.stderr,
        )
        return 1

    command_version(["rustfmt", "--version"])
    command_version(["cargo", "clippy", "--version"])
    print(f"toolchain accepted ({rustc}; {cargo})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
