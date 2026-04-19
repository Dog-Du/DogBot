from pathlib import Path

from scripts.audit_legacy_runtime_memory import classify_memory_file


def test_classify_memory_file_marks_empty_as_ignore(tmp_path: Path) -> None:
    path = tmp_path / "empty.md"
    path.write_text("   ", encoding="utf-8")
    result = classify_memory_file(path)
    assert result["classification"] == "ignore"


def test_classify_memory_file_marks_structured_content_as_candidate(
    tmp_path: Path,
) -> None:
    path = tmp_path / "prefs.md"
    path.write_text("- prefers rust\n- likes concise replies\n", encoding="utf-8")
    result = classify_memory_file(path)
    assert result["classification"] == "candidate"
    assert result["reason"] == "structured_bullet_memory"
