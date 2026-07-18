#!/usr/bin/env python3
"""Validate repository-local links in Markdown files."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys
from urllib.parse import unquote


LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)(?:\s+[^)]*)?\)")
EXTERNAL_SCHEMES = ("http://", "https://", "mailto:", "tel:")


def iter_markdown_files(root: Path):
    for path in root.rglob("*.md"):
        if ".git" not in path.parts and "target" not in path.parts:
            yield path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    root = args.root.resolve()
    failures: list[str] = []
    checked = 0

    for markdown in iter_markdown_files(root):
        text = markdown.read_text(encoding="utf-8")
        for match in LINK.finditer(text):
            destination = match.group(1).strip("<>")
            if destination.startswith(EXTERNAL_SCHEMES) or destination.startswith("#"):
                continue
            relative_path = unquote(destination.split("#", 1)[0])
            if not relative_path:
                continue
            checked += 1
            target = (markdown.parent / relative_path).resolve()
            try:
                target.relative_to(root)
            except ValueError:
                failures.append(f"DOC-LINK-OUTSIDE-REPOSITORY: {markdown.relative_to(root)} -> {destination}")
                continue
            if not target.exists():
                line = text.count("\n", 0, match.start()) + 1
                failures.append(
                    f"DOC-LINK-MISSING: {markdown.relative_to(root)}:{line} -> {destination}"
                )

    if failures:
        print("\n".join(sorted(failures)), file=sys.stderr)
        return 1
    print(f"Markdown links accepted ({checked} local links checked)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
