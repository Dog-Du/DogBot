import json
from pathlib import Path

from scripts.sync_content_sources import generate_pack_manifest, load_sources_lock


def test_load_sources_lock_reads_sources(tmp_path: Path) -> None:
    lock_path = tmp_path / "sources.lock.json"
    lock_path.write_text(
        json.dumps(
            {
                "version": 1,
                "sources": [
                    {
                        "source_id": "openhands_extensions",
                        "repo_url": "https://github.com/OpenHands/extensions.git",
                        "ref": "abc123",
                        "license": "MIT",
                        "selected_paths": ["skills"],
                        "import_mode": "skill_pack",
                        "target_pack": "starter-skills",
                    }
                ],
            }
        ),
        encoding="utf-8",
    )

    data = load_sources_lock(lock_path)

    assert data["version"] == 1
    assert data["sources"][0]["source_id"] == "openhands_extensions"


def test_generate_skill_pack_manifest_marks_enabled_items(tmp_path: Path) -> None:
    upstream_root = tmp_path / "upstream"
    skill_dir = upstream_root / "skills" / "summarize"
    skill_dir.mkdir(parents=True)
    (skill_dir / "SKILL.md").write_text("# Summarize\n", encoding="utf-8")

    manifest = generate_pack_manifest(
        source_id="openhands_extensions",
        source_license="MIT",
        source_repo="https://github.com/OpenHands/extensions.git",
        source_ref="abc123",
        import_mode="skill_pack",
        upstream_root=upstream_root,
        target_pack="starter-skills",
    )

    assert manifest["pack_id"] == "starter-skills"
    assert manifest["items"][0]["id"] == "starter-skills.summarize"
    assert manifest["items"][0]["enabled_by_default"] is True
