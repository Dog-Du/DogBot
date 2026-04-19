#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


def load_sources_lock(lock_path: Path) -> dict[str, Any]:
    return json.loads(lock_path.read_text(encoding="utf-8"))


def generate_pack_manifest(
    *,
    source_id: str,
    source_license: str,
    source_repo: str,
    source_ref: str,
    import_mode: str,
    upstream_root: Path,
    target_pack: str,
) -> dict[str, Any]:
    items: list[dict[str, Any]] = []
    kind = "resource-pack"

    if import_mode == "skill_pack":
        kind = "skill-pack"
        for skill_file in sorted(upstream_root.glob("skills/*/SKILL.md")):
            skill_name = skill_file.parent.name
            items.append(
                {
                    "id": f"{target_pack}.{skill_name}",
                    "kind": "skill",
                    "path": str(skill_file.relative_to(upstream_root)),
                    "title": skill_name.replace("-", " ").title(),
                    "summary": f"Imported skill {skill_name}",
                    "tags": [skill_name],
                    "enabled_by_default": True,
                    "platform_overrides": [],
                    "upstream_path": str(skill_file.relative_to(upstream_root)),
                }
            )
    elif import_mode == "copy_examples":
        for example in sorted(upstream_root.rglob("*.md")):
            items.append(
                {
                    "id": f"{target_pack}.{example.stem}",
                    "kind": "resource",
                    "path": str(example.relative_to(upstream_root)),
                    "title": example.stem.replace("-", " ").title(),
                    "summary": f"Imported example {example.stem}",
                    "tags": ["example"],
                    "enabled_by_default": False,
                    "platform_overrides": [],
                    "upstream_path": str(example.relative_to(upstream_root)),
                }
            )
    elif import_mode == "taxonomy_only":
        kind = "taxonomy-pack"
        items.append(
            {
                "id": f"{target_pack}.baseline",
                "kind": "memory-taxonomy",
                "path": "taxonomy/baseline.json",
                "title": "DogBot Memory Baseline",
                "summary": "Mem0-inspired memory taxonomy for DogBot scopes.",
                "tags": ["memory", "taxonomy"],
                "enabled_by_default": True,
                "platform_overrides": [],
                "upstream_path": "derived-from-docs",
            }
        )
    else:
        raise ValueError(f"unsupported import_mode: {import_mode}")

    return {
        "pack_id": target_pack,
        "version": 1,
        "title": target_pack.replace("-", " ").title(),
        "kind": kind,
        "source": {
            "source_id": source_id,
            "repo_url": source_repo,
            "ref": source_ref,
            "license": source_license,
        },
        "items": items,
    }


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )


def clone_source(repo_url: str, ref: str, checkout_root: Path) -> Path:
    subprocess.run(
        ["git", "clone", "--depth", "1", repo_url, str(checkout_root)],
        check=True,
        capture_output=True,
        text=True,
    )
    subprocess.run(
        ["git", "-C", str(checkout_root), "fetch", "--depth", "1", "origin", ref],
        check=True,
        capture_output=True,
        text=True,
    )
    subprocess.run(
        ["git", "-C", str(checkout_root), "checkout", ref],
        check=True,
        capture_output=True,
        text=True,
    )
    return checkout_root


def snapshot_selected_paths(
    *,
    checkout_root: Path,
    selected_paths: list[str],
    upstream_dir: Path,
) -> None:
    if upstream_dir.exists():
        shutil.rmtree(upstream_dir)
    upstream_dir.mkdir(parents=True, exist_ok=True)
    for relative in selected_paths:
        source_path = checkout_root / relative
        if not source_path.exists():
            continue
        destination = upstream_dir / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        if source_path.is_dir():
            shutil.copytree(source_path, destination, dirs_exist_ok=True)
        else:
            shutil.copy2(source_path, destination)


def write_source_metadata(source: dict[str, Any], upstream_dir: Path) -> None:
    write_json(
        upstream_dir / "SOURCE.json",
        {
            "source_id": source["source_id"],
            "repo_url": source["repo_url"],
            "requested_ref": source["ref"],
            "resolved_commit": source["ref"],
            "license": source["license"],
            "imported_at": datetime.now(UTC).isoformat(),
        },
    )


def sync_sources(content_root: Path, *, skip_clone: bool) -> dict[str, Any]:
    lock = load_sources_lock(content_root / "sources.lock.json")
    report: dict[str, Any] = {"version": lock["version"], "sources": []}

    for source in lock["sources"]:
        upstream_dir = content_root / "upstream" / source["source_id"]
        if skip_clone:
            upstream_dir.mkdir(parents=True, exist_ok=True)
        else:
            with tempfile.TemporaryDirectory(prefix=f"{source['source_id']}-") as tmp:
                checkout_root = clone_source(
                    source["repo_url"], source["ref"], Path(tmp) / "repo"
                )
                snapshot_selected_paths(
                    checkout_root=checkout_root,
                    selected_paths=source["selected_paths"],
                    upstream_dir=upstream_dir,
                )
        write_source_metadata(source, upstream_dir)

        manifest = generate_pack_manifest(
            source_id=source["source_id"],
            source_license=source["license"],
            source_repo=source["repo_url"],
            source_ref=source["ref"],
            import_mode=source["import_mode"],
            upstream_root=upstream_dir,
            target_pack=source["target_pack"],
        )
        write_json(content_root / "packs" / source["target_pack"] / "manifest.json", manifest)
        report["sources"].append(
            {
                "source_id": source["source_id"],
                "target_pack": source["target_pack"],
                "item_count": len(manifest["items"]),
            }
        )

    write_json(content_root / "import-report.json", report)
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--content-root", type=Path, default=Path("content"))
    parser.add_argument("--skip-clone", action="store_true")
    args = parser.parse_args()
    sync_sources(args.content_root, skip_clone=args.skip_clone)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
