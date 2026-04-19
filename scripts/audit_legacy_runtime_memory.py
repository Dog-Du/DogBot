#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def classify_memory_file(path: Path) -> dict[str, Any]:
    raw = path.read_text(encoding="utf-8").strip()
    if not raw:
        return {"classification": "ignore", "reason": "empty"}
    if raw.startswith("- ") or raw.startswith("* "):
        return {"classification": "candidate", "reason": "structured_bullet_memory"}
    if len(raw.splitlines()) > 20:
        return {"classification": "manual_review", "reason": "long_unstructured_note"}
    return {"classification": "candidate", "reason": "short_text_memory"}


def audit_directory(root: Path) -> list[dict[str, Any]]:
    results = []
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        result = classify_memory_file(path)
        result["source_path"] = str(path)
        results.append(result)
    return results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    payload = json.dumps({"items": audit_directory(args.root)}, ensure_ascii=False, indent=2)
    if args.output:
        args.output.write_text(payload + "\n", encoding="utf-8")
    else:
        print(payload)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
