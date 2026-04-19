#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import shutil
from pathlib import Path


SAFE_TOP_LEVEL_TARGETS = (
    "skills",
    "telemetry",
    "paste-cache",
    "shell-snapshots",
    "history.jsonl",
)

SAFE_NESTED_TARGETS = (
    "cache/changelog.md",
    "projects/*/memory",
)


def plan_cleanup(root: Path) -> list[Path]:
    if not root.exists():
        return []

    planned: list[Path] = []
    seen: set[Path] = set()

    for relative in SAFE_TOP_LEVEL_TARGETS:
        path = root / relative
        if path.exists() and path not in seen:
            planned.append(path)
            seen.add(path)

    for relative in SAFE_NESTED_TARGETS:
        for path in sorted(root.glob(relative)):
            if path.exists() and path not in seen:
                planned.append(path)
                seen.add(path)

    return planned


def remove_path(path: Path) -> None:
    if path.is_dir():
        shutil.rmtree(path)
    else:
        path.unlink()


def cleanup_directory(root: Path, *, dry_run: bool = False) -> dict[str, object]:
    if not root.exists():
        return {
            "root": str(root),
            "missing_root": True,
            "removed": [],
            "removed_count": 0,
        }

    targets = plan_cleanup(root)
    removed = [str(path) for path in targets]

    if not dry_run:
        for path in targets:
            remove_path(path)

    return {
        "root": str(root),
        "missing_root": False,
        "removed": removed,
        "removed_count": len(removed),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--json", action="store_true", dest="emit_json")
    args = parser.parse_args()

    result = cleanup_directory(args.root, dry_run=args.dry_run)

    if args.emit_json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0

    if result["missing_root"]:
        print(f"Legacy Claude content root does not exist: {args.root}")
        return 0

    action = "Would remove" if args.dry_run else "Removed"
    print(f"{action} {result['removed_count']} legacy Claude content path(s) under {args.root}")
    for path in result["removed"]:
        print(f"- {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
