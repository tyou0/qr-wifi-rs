#!/usr/bin/env python3
"""Rewrite every source-controlled release version in one testable step.

Cargo.lock is intentionally excluded: Cargo regenerates it from Cargo.toml in the
next release step, and `cargo check --locked` later verifies the committed lock.
"""

import argparse
import json
from pathlib import Path
import re


def parse_version(value: str) -> str:
    components = value.split(".")
    if len(components) != 3:
        raise ValueError("expected exactly three numeric components (X.Y.Z)")
    parsed = []
    for component in components:
        if not re.fullmatch(r"0|[1-9][0-9]*", component):
            raise ValueError("components must be canonical decimal integers")
        number = int(component)
        if number > 65_535:
            raise ValueError("Chrome extension components must be <= 65535")
        parsed.append(number)
    if not any(parsed):
        raise ValueError("Chrome extension version must be greater than 0.0.0")
    return value


def update(root: Path, version: str) -> None:
    cargo = root / "Cargo.toml"
    text = cargo.read_text()
    updated, count = re.subn(
        r'(?m)^(\[workspace\.package\]\nversion = ")[^"]+("$)',
        rf"\g<1>{version}\g<2>",
        text,
        count=1,
    )
    if count != 1:
        raise RuntimeError("workspace.package version was not updated exactly once")
    cargo.write_text(updated)

    for relative in ("src-tauri/tauri.conf.json", "extension/manifest.json"):
        path = root / relative
        document = json.loads(path.read_text())
        document["version"] = version
        path.write_text(json.dumps(document, indent=2) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    version = parse_version(args.version)
    if not args.validate_only:
        update(args.root.resolve(), version)


if __name__ == "__main__":
    main()
