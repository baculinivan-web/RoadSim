#!/usr/bin/env python3
"""Reject Cargo dependency edges forbidden by RoadSim architecture policy."""

from __future__ import annotations

import argparse
from collections import deque
import fnmatch
import json
from pathlib import Path
import sys
import tomllib
from typing import Any


def load_policy(path: Path) -> dict[str, Any]:
    with path.open("rb") as policy_file:
        policy = tomllib.load(policy_file)

    if policy.get("schema_version") != 1:
        raise ValueError("dependency policy must use schema_version = 1")
    if not policy.get("diagnostic_code"):
        raise ValueError("dependency policy must define diagnostic_code")

    seen_ids: set[str] = set()
    for rule in policy.get("rules", []):
        rule_id = rule.get("id")
        if not rule_id or rule_id in seen_ids:
            raise ValueError(f"dependency policy has a missing or duplicate rule id: {rule_id!r}")
        seen_ids.add(rule_id)
        if not rule.get("from") or not rule.get("to") or not rule.get("rationale"):
            raise ValueError(f"dependency policy rule {rule_id!r} is incomplete")

    if not seen_ids:
        raise ValueError("dependency policy must contain at least one rule")
    return policy


def load_metadata(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as metadata_file:
        metadata = json.load(metadata_file)
    if not isinstance(metadata.get("packages"), list):
        raise ValueError("Cargo metadata is missing packages")
    if not isinstance(metadata.get("resolve", {}).get("nodes"), list):
        raise ValueError("Cargo metadata is missing resolve.nodes")
    return metadata


def matches(name: str, patterns: list[str]) -> bool:
    return any(fnmatch.fnmatchcase(name, pattern) for pattern in patterns)


def find_violations(metadata: dict[str, Any], policy: dict[str, Any]) -> list[dict[str, str]]:
    package_names = {package["id"]: package["name"] for package in metadata["packages"]}
    adjacency: dict[str, list[str]] = {}
    violations: list[dict[str, str]] = []

    for node in metadata["resolve"]["nodes"]:
        source = package_names.get(node["id"])
        if source is None:
            raise ValueError(f"resolve node references unknown package id {node['id']!r}")
        adjacency[node["id"]] = []
        for dependency in node.get("deps", []):
            target = package_names.get(dependency["pkg"])
            if target is None:
                raise ValueError(
                    f"dependency from {source!r} references unknown package id {dependency['pkg']!r}"
                )
            adjacency[node["id"]].append(dependency["pkg"])

    for source_id, direct_dependencies in adjacency.items():
        source = package_names[source_id]
        for rule in policy["rules"]:
            if not matches(source, rule["from"]):
                continue
            queue = deque(
                (dependency_id, [source_id, dependency_id])
                for dependency_id in direct_dependencies
            )
            visited: set[str] = set()
            while queue:
                target_id, path = queue.popleft()
                if target_id in visited:
                    continue
                visited.add(target_id)
                target = package_names[target_id]
                if matches(target, rule["to"]):
                    violations.append(
                        {
                            "rule": rule["id"],
                            "source": source,
                            "target": target,
                            "path": " -> ".join(package_names[package_id] for package_id in path),
                            "rationale": rule["rationale"],
                        }
                    )
                queue.extend(
                    (dependency_id, [*path, dependency_id])
                    for dependency_id in adjacency.get(target_id, [])
                    if dependency_id not in visited
                )

    return sorted(violations, key=lambda item: (item["rule"], item["source"], item["target"]))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", type=Path, required=True, help="Cargo metadata JSON")
    parser.add_argument("--policy", type=Path, required=True, help="RoadSim dependency policy TOML")
    args = parser.parse_args()

    try:
        policy = load_policy(args.policy)
        metadata = load_metadata(args.metadata)
        violations = find_violations(metadata, policy)
    except (OSError, ValueError, KeyError, json.JSONDecodeError, tomllib.TOMLDecodeError) as error:
        print(f"ARCH-DEPENDENCY-GUARD-INVALID: {error}", file=sys.stderr)
        return 2

    if violations:
        diagnostic_code = policy["diagnostic_code"]
        for violation in violations:
            print(
                f"{diagnostic_code}: {violation['source']} -> {violation['target']} "
                f"violates {violation['rule']} via {violation['path']}: {violation['rationale']}",
                file=sys.stderr,
            )
        return 1

    print(f"dependency graph accepted ({len(metadata['resolve']['nodes'])} packages checked)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
