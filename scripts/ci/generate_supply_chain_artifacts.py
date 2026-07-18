#!/usr/bin/env python3
"""Collect CycloneDX SBOMs and emit a deterministic Rust/native license inventory."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import sys
import tempfile
import tomllib
from typing import Any


REQUIRED_NATIVE_FIELDS = {
    "name",
    "version",
    "license",
    "source",
    "scope",
    "platforms",
    "bundled",
    "review",
}


def load_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as input_file:
        return json.load(input_file)


def native_components(path: Path) -> list[dict[str, Any]]:
    with path.open("rb") as native_file:
        inventory = tomllib.load(native_file)
    if inventory.get("schema_version") != 1:
        raise ValueError("native dependency inventory must use schema_version = 1")
    components = inventory.get("components")
    if not isinstance(components, list):
        raise ValueError("native dependency inventory must define components as a list")

    for index, component in enumerate(components):
        missing = REQUIRED_NATIVE_FIELDS - component.keys()
        if missing:
            raise ValueError(f"native component {index} is missing fields: {sorted(missing)}")
        if component["scope"] not in {"build", "runtime", "build-and-runtime"}:
            raise ValueError(f"native component {component['name']!r} has invalid scope")
        if not isinstance(component["platforms"], list) or not component["platforms"]:
            raise ValueError(f"native component {component['name']!r} needs at least one platform")
        if not isinstance(component["bundled"], bool):
            raise ValueError(f"native component {component['name']!r} bundled must be boolean")
    return sorted(components, key=lambda item: (item["name"], item["version"]))


def rust_components(metadata: dict[str, Any]) -> list[dict[str, Any]]:
    components = []
    for package in metadata.get("packages", []):
        license_expression = package.get("license")
        raw_license_file = package.get("license_file")
        license_file = Path(raw_license_file).name if raw_license_file else None
        if not license_expression and not license_file:
            raise ValueError(f"Rust package {package['name']} {package['version']} has no license metadata")
        components.append(
            {
                "name": package["name"],
                "version": package["version"],
                "license": license_expression,
                "license_file": license_file,
                "source": package.get("source") or "workspace",
            }
        )
    return sorted(components, key=lambda item: (item["name"], item["version"], item["source"]))


def redact_workspace_paths(value: Any, root: Path) -> Any:
    if isinstance(value, dict):
        return {key: redact_workspace_paths(item, root) for key, item in value.items()}
    if isinstance(value, list):
        return [redact_workspace_paths(item, root) for item in value]
    if isinstance(value, str):
        root_text = str(root)
        return value.replace(f"{root_text}/", "/roadsim-workspace/").replace(
            root_text, "/roadsim-workspace"
        )
    return value


def workspace_sboms(root: Path, metadata: dict[str, Any]) -> list[tuple[Path, str, dict[str, Any]]]:
    packages = {package["id"]: package for package in metadata.get("packages", [])}
    workspace_members = metadata.get("workspace_members", [])
    if not workspace_members:
        raise ValueError("Cargo metadata has no workspace_members")

    collected: list[tuple[Path, str, dict[str, Any]]] = []
    names: set[str] = set()
    for package_id in workspace_members:
        package = packages.get(package_id)
        if package is None:
            raise ValueError(f"workspace member references unknown package id {package_id!r}")
        manifest = Path(package["manifest_path"]).resolve()
        try:
            manifest.relative_to(root)
        except ValueError as error:
            raise ValueError(f"workspace manifest is outside the repository: {manifest}") from error
        source = manifest.with_name(f"{package['name']}.cdx.json")
        if not source.is_file():
            raise ValueError(f"cargo cyclonedx did not produce {source}")
        sbom = load_json(source)
        if sbom.get("bomFormat") != "CycloneDX" or not sbom.get("specVersion"):
            raise ValueError(f"invalid CycloneDX document: {source}")
        component = sbom.get("metadata", {}).get("component", {})
        name = component.get("name")
        if not name:
            raise ValueError(f"CycloneDX document has no metadata.component.name: {source}")
        if name in names:
            raise ValueError(f"duplicate CycloneDX component name: {name}")
        names.add(name)
        collected.append((source, f"{name}.cdx.json", redact_workspace_paths(sbom, root)))
    return collected


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    try:
        root = args.root.resolve()
        output = args.output.resolve()
        if output.exists():
            raise ValueError(f"output path already exists: {output}")
        metadata = load_json(args.metadata)
        native = native_components(args.native)
        rust = rust_components(metadata)
        sboms = workspace_sboms(root, metadata)
        inventory = {
            "artifact_schema_version": 1,
            "rust_components": rust,
            "native_components": native,
            "cyclonedx_documents": sorted(filename for _, filename, _ in sboms),
        }
        output.parent.mkdir(parents=True, exist_ok=True)
        staging = Path(tempfile.mkdtemp(prefix=f".{output.name}-", dir=output.parent))
        try:
            for _, filename, sbom in sboms:
                serialized_sbom = json.dumps(sbom, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
                if str(root) in serialized_sbom:
                    raise ValueError(f"CycloneDX document still contains workspace path: {filename}")
                (staging / filename).write_text(serialized_sbom, encoding="utf-8")
            (staging / "license-inventory.json").write_text(
                json.dumps(inventory, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            staging.rename(output)
        except Exception:
            shutil.rmtree(staging, ignore_errors=True)
            raise
        for source, _, _ in sboms:
            try:
                source.unlink()
            except OSError as error:
                print(f"SUPPLY-CHAIN-CLEANUP-WARNING: {error}", file=sys.stderr)
    except (OSError, ValueError, KeyError, json.JSONDecodeError, tomllib.TOMLDecodeError) as error:
        print(f"SUPPLY-CHAIN-ARTIFACT-INVALID: {error}", file=sys.stderr)
        return 1

    print(
        f"supply-chain artifact ready ({len(rust)} Rust, {len(native)} native components, "
        f"{len(sboms)} CycloneDX documents)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
