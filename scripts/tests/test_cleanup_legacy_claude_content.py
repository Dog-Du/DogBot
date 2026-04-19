from pathlib import Path

from scripts.cleanup_legacy_claude_content import cleanup_directory, plan_cleanup


def seed_legacy_claude_root(root: Path) -> None:
    (root / "skills" / "old-skill").mkdir(parents=True)
    (root / "projects" / "project-a" / "memory").mkdir(parents=True)
    (root / "telemetry").mkdir(parents=True)
    (root / "paste-cache").mkdir(parents=True)
    (root / "shell-snapshots").mkdir(parents=True)
    (root / "cache").mkdir(parents=True)
    (root / "sessions").mkdir(parents=True)
    (root / "session-env").mkdir(parents=True)
    (root / "plugins" / "data").mkdir(parents=True)
    (root / "projects" / "project-a").mkdir(parents=True, exist_ok=True)
    (root / "debug").mkdir(parents=True)

    (root / "settings.json").write_text("{}", encoding="utf-8")
    (root / "history.jsonl").write_text('{"event":"legacy"}\n', encoding="utf-8")
    (root / "cache" / "changelog.md").write_text("# old\n", encoding="utf-8")
    (root / "projects" / "project-a" / "note.jsonl").write_text("keep\n", encoding="utf-8")
    (root / ".keep").write_text("", encoding="utf-8")


def test_plan_cleanup_targets_only_safe_legacy_items(tmp_path: Path) -> None:
    seed_legacy_claude_root(tmp_path)

    planned = {path.relative_to(tmp_path).as_posix() for path in plan_cleanup(tmp_path)}

    assert "skills" in planned
    assert "projects/project-a/memory" in planned
    assert "telemetry" in planned
    assert "paste-cache" in planned
    assert "shell-snapshots" in planned
    assert "history.jsonl" in planned
    assert "cache/changelog.md" in planned

    assert "sessions" not in planned
    assert "session-env" not in planned
    assert "plugins" not in planned
    assert "settings.json" not in planned
    assert "projects/project-a/note.jsonl" not in planned
    assert ".keep" not in planned
    assert "debug" not in planned


def test_cleanup_directory_removes_targeted_items_and_preserves_runtime_state(
    tmp_path: Path,
) -> None:
    seed_legacy_claude_root(tmp_path)

    result = cleanup_directory(tmp_path)

    assert result["removed_count"] == 7
    assert result["missing_root"] is False

    assert not (tmp_path / "skills").exists()
    assert not (tmp_path / "projects" / "project-a" / "memory").exists()
    assert not (tmp_path / "telemetry").exists()
    assert not (tmp_path / "paste-cache").exists()
    assert not (tmp_path / "shell-snapshots").exists()
    assert not (tmp_path / "history.jsonl").exists()
    assert not (tmp_path / "cache" / "changelog.md").exists()

    assert (tmp_path / "sessions").exists()
    assert (tmp_path / "session-env").exists()
    assert (tmp_path / "plugins").exists()
    assert (tmp_path / "settings.json").exists()
    assert (tmp_path / "projects" / "project-a" / "note.jsonl").exists()
    assert (tmp_path / ".keep").exists()
    assert (tmp_path / "debug").exists()


def test_cleanup_directory_reports_missing_root_without_error(tmp_path: Path) -> None:
    missing = tmp_path / "missing"

    result = cleanup_directory(missing)

    assert result == {"root": str(missing), "missing_root": True, "removed": [], "removed_count": 0}
