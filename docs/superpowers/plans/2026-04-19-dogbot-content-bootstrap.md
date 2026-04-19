# DogBot Content Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a repository-managed content bootstrap pipeline so DogBot can import starter skills, memory taxonomy, and example templates from pinned upstream sources without making runtime depend on upstream formats.

**Architecture:** Keep runtime loading simple: `agent-runner` should only read `content/packs/` manifests and `content/policies/`. Add an offline sync tool that reads `content/sources.lock.json`, snapshots selected upstream paths into `content/upstream/`, and generates normalized packs for OpenViking examples, OpenHands starter skills, and Mem0 memory taxonomy. Add a separate legacy-runtime-memory audit tool so old Claude runtime memory becomes an import candidate source instead of a canonical store.

**Tech Stack:** Rust, `serde`, `serde_json`, `rusqlite`, Python 3, `pytest`, existing `cargo test`, repository-managed JSON manifests

---

## File Structure

- Create: `content/sources.lock.json`
  - pinned upstream definitions and import modes
- Create: `content/packs/base/manifest.json`
  - DogBot-owned base prompt/resource pack
- Create: `content/packs/qq/manifest.json`
  - QQ-specific pack metadata
- Create: `content/packs/wechat/manifest.json`
  - WeChat-specific pack metadata
- Create: `content/packs/starter-skills/manifest.json`
  - generated starter-skill pack manifest
- Create: `content/packs/memory-baseline/manifest.json`
  - generated memory taxonomy manifest
- Create: `content/packs/ov-examples/manifest.json`
  - generated OpenViking examples pack manifest
- Create: `content/local/packs/.gitkeep`
- Create: `content/local/overrides/.gitkeep`
- Create: `content/upstream/.gitkeep`
- Modify: `agent-runner/src/context/repo_loader.rs`
  - parse pack manifests, list enabled items, reject malformed packs
- Modify: `agent-runner/src/context/context_pack.rs`
  - render loaded pack summaries into Claude-facing context
- Modify: `agent-runner/src/context/mod.rs`
- Modify: `agent-runner/src/lib.rs`
- Create: `agent-runner/tests/repo_loader_tests.rs`
  - loader parsing and validation coverage
- Modify: `agent-runner/tests/context_run_tests.rs`
  - `/v1/runs` context includes pack items
- Create: `scripts/sync_content_sources.py`
  - clone/export selected upstream paths and generate packs
- Create: `scripts/audit_legacy_runtime_memory.py`
  - classify legacy runtime memory into `ignore / candidate / manual_review`
- Create: `scripts/tests/test_sync_content_sources.py`
  - pytest coverage for lock parsing and pack generation
- Create: `scripts/tests/test_audit_legacy_runtime_memory.py`
  - pytest coverage for audit classification
- Modify: `scripts/check_structure.sh`
  - ensure new scripts/files exist and Python scripts compile
- Modify: `README.md`
  - document content bootstrap flow and source-of-truth boundary
- Modify: `docs/control-plane-integration.md`
  - document sync step and legacy audit step

### Task 1: Add content source schema and starter pack layout

**Files:**
- Create: `content/sources.lock.json`
- Create: `content/packs/base/manifest.json`
- Create: `content/packs/qq/manifest.json`
- Create: `content/packs/wechat/manifest.json`
- Create: `content/packs/starter-skills/manifest.json`
- Create: `content/packs/memory-baseline/manifest.json`
- Create: `content/packs/ov-examples/manifest.json`
- Create: `content/local/packs/.gitkeep`
- Create: `content/local/overrides/.gitkeep`
- Create: `content/upstream/.gitkeep`
- Modify: `scripts/check_structure.sh`

- [ ] **Step 1: Write the failing structure test**

Add to `scripts/check_structure.sh`:

```bash
  "content/sources.lock.json"
  "content/packs/base/manifest.json"
  "content/packs/qq/manifest.json"
  "content/packs/wechat/manifest.json"
  "content/packs/starter-skills/manifest.json"
  "content/packs/memory-baseline/manifest.json"
  "content/packs/ov-examples/manifest.json"
  "scripts/sync_content_sources.py"
  "scripts/audit_legacy_runtime_memory.py"
  "scripts/tests/test_sync_content_sources.py"
  "scripts/tests/test_audit_legacy_runtime_memory.py"
```

and:

```bash
uv run python -m py_compile "$repo_root/scripts/sync_content_sources.py"
uv run python -m py_compile "$repo_root/scripts/audit_legacy_runtime_memory.py"
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
bash scripts/check_structure.sh
```

Expected:
- FAIL because the new `content/` manifests and Python scripts do not exist yet

- [ ] **Step 3: Create the pinned source lock file**

Create `content/sources.lock.json`:

```json
{
  "version": 1,
  "sources": [
    {
      "source_id": "openviking_examples",
      "repo_url": "https://github.com/volcengine/OpenViking.git",
      "ref": "1e51c5444785eabac33608459780903000e63117",
      "license": "Apache-2.0",
      "selected_paths": ["examples"],
      "import_mode": "copy_examples",
      "target_pack": "ov-examples"
    },
    {
      "source_id": "openhands_extensions",
      "repo_url": "https://github.com/OpenHands/extensions.git",
      "ref": "fae914d4ada9e85ae0686dd7c0a50781d229679b",
      "license": "MIT",
      "selected_paths": ["skills"],
      "import_mode": "skill_pack",
      "target_pack": "starter-skills"
    },
    {
      "source_id": "mem0_taxonomy",
      "repo_url": "https://github.com/mem0ai/mem0.git",
      "ref": "93da5ef8f7267130d6fd9a3ea6b815a9efb5d7ad",
      "license": "Apache-2.0",
      "selected_paths": ["docs"],
      "import_mode": "taxonomy_only",
      "target_pack": "memory-baseline"
    }
  ]
}
```

- [ ] **Step 4: Create minimal local and generated pack manifests**

Create `content/packs/base/manifest.json`:

```json
{
  "pack_id": "base",
  "version": 1,
  "title": "DogBot Base Pack",
  "kind": "resource-pack",
  "source": {
    "source_id": "dogbot_local",
    "repo_url": "local",
    "ref": "workspace",
    "license": "Proprietary"
  },
  "items": []
}
```

Create `content/packs/qq/manifest.json` and `content/packs/wechat/manifest.json` with the same shape and platform-specific titles. Create `content/packs/starter-skills/manifest.json`, `content/packs/memory-baseline/manifest.json`, and `content/packs/ov-examples/manifest.json` with empty `items` arrays plus the matching upstream `source_id`.

- [ ] **Step 5: Create support directories for local overrides and upstream snapshots**

Create:

```text
content/local/packs/.gitkeep
content/local/overrides/.gitkeep
content/upstream/.gitkeep
```

- [ ] **Step 6: Run structure checks to verify they pass**

Run:

```bash
bash scripts/check_structure.sh
```

Expected:
- PASS

- [ ] **Step 7: Commit**

```bash
git add content/sources.lock.json content/packs/base/manifest.json content/packs/qq/manifest.json content/packs/wechat/manifest.json content/packs/starter-skills/manifest.json content/packs/memory-baseline/manifest.json content/packs/ov-examples/manifest.json content/local/packs/.gitkeep content/local/overrides/.gitkeep content/upstream/.gitkeep scripts/check_structure.sh
git commit -m "feat: add content bootstrap layout"
```

### Task 2: Teach `agent-runner` to load pack manifests instead of raw upstream content

**Files:**
- Modify: `agent-runner/src/context/repo_loader.rs`
- Modify: `agent-runner/src/context/context_pack.rs`
- Modify: `agent-runner/src/context/mod.rs`
- Modify: `agent-runner/src/lib.rs`
- Create: `agent-runner/tests/repo_loader_tests.rs`
- Modify: `agent-runner/tests/context_run_tests.rs`

- [ ] **Step 1: Write the failing loader test**

Create `agent-runner/tests/repo_loader_tests.rs`:

```rust
use agent_runner::context::repo_loader::RepoContentLoader;

#[test]
fn repo_loader_reads_pack_manifests_and_items() {
    let temp = tempfile::tempdir().unwrap();
    let pack_dir = temp.path().join("packs/base");
    std::fs::create_dir_all(&pack_dir).unwrap();
    std::fs::write(
        pack_dir.join("manifest.json"),
        r#"{
            "pack_id":"base",
            "version":1,
            "title":"DogBot Base Pack",
            "kind":"resource-pack",
            "source":{"source_id":"dogbot_local","repo_url":"local","ref":"workspace","license":"Proprietary"},
            "items":[
                {
                    "id":"base.system",
                    "kind":"prompt",
                    "path":"prompts/system.md",
                    "title":"System Prompt",
                    "summary":"base prompt",
                    "tags":["base"],
                    "enabled_by_default":true,
                    "platform_overrides":[],
                    "upstream_path":""
                }
            ]
        }"#,
    )
    .unwrap();

    let loader = RepoContentLoader::new(temp.path().display().to_string());
    let packs = loader.load_packs().unwrap();

    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0].pack_id, "base");
    assert_eq!(packs[0].items[0].id, "base.system");
}
```

- [ ] **Step 2: Write the failing context rendering test**

Add to `agent-runner/tests/context_run_tests.rs`:

```rust
#[tokio::test]
async fn run_endpoint_includes_enabled_pack_items_in_context() {
    let settings = test_settings();
    let pack_dir = std::path::Path::new(&settings.content_root).join("packs/base");
    std::fs::create_dir_all(&pack_dir).unwrap();
    std::fs::write(
        pack_dir.join("manifest.json"),
        r#"{
            "pack_id":"base",
            "version":1,
            "title":"DogBot Base Pack",
            "kind":"resource-pack",
            "source":{"source_id":"dogbot_local","repo_url":"local","ref":"workspace","license":"Proprietary"},
            "items":[
                {
                    "id":"base.system",
                    "kind":"prompt",
                    "path":"prompts/system.md",
                    "title":"System Prompt",
                    "summary":"base prompt",
                    "tags":["base"],
                    "enabled_by_default":true,
                    "platform_overrides":[],
                    "upstream_path":""
                }
            ]
        }"#,
    )
    .unwrap();

    let runner = Arc::new(CapturingRunner::default());
    let app = build_test_app_with_settings(runner.clone(), settings);
    let payload = serde_json::to_vec(&base_request()).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/runs")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let prompt = runner.captured_prompt().unwrap();
    assert!(prompt.contains("Enabled pack items:"));
    assert!(prompt.contains("base.system"));
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```bash
cargo test --test repo_loader_tests --manifest-path agent-runner/Cargo.toml
cargo test --test context_run_tests --manifest-path agent-runner/Cargo.toml run_endpoint_includes_enabled_pack_items_in_context
```

Expected:
- FAIL because `RepoContentLoader` does not parse manifests and the context pack renderer does not include pack items

- [ ] **Step 4: Implement pack manifest types and loader**

Update `agent-runner/src/context/repo_loader.rs` with:

```rust
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct PackManifest {
    pub pack_id: String,
    pub version: u32,
    pub title: String,
    pub kind: String,
    pub source: PackSource,
    pub items: Vec<PackItem>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct PackSource {
    pub source_id: String,
    pub repo_url: String,
    pub ref_: String,
    pub license: String,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct PackItem {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub enabled_by_default: bool,
    pub platform_overrides: Vec<String>,
    pub upstream_path: String,
}
```

and loader methods:

```rust
pub fn load_packs(&self) -> Result<Vec<PackManifest>, RepoLoaderError> { /* scan content_root/packs/*/manifest.json */ }

pub fn enabled_items(&self) -> Result<Vec<PackItem>, RepoLoaderError> {
    Ok(self
        .load_packs()?
        .into_iter()
        .flat_map(|pack| pack.items.into_iter())
        .filter(|item| item.enabled_by_default)
        .collect())
}
```

Use:

```rust
#[serde(rename = "ref")]
pub ref_: String,
```

so JSON stays `ref`.

- [ ] **Step 5: Render enabled pack items into the Claude-facing context**

Update `agent-runner/src/context/context_pack.rs`:

```rust
use super::repo_loader::PackItem;

pub fn render_context_pack_with_history_and_items(
    scopes: &[ReadableScopes],
    history_evidence: Option<&str>,
    items: &[PackItem],
) -> String {
    let mut output = render_context_pack_with_history(scopes, history_evidence);
    if !items.is_empty() {
        output.push_str("\nEnabled pack items:\n");
        for item in items {
            output.push_str(&format!(
                "- {} [{}] {}\n",
                item.id, item.kind, item.summary
            ));
        }
    }
    output
}
```

Update `agent-runner/src/server.rs` to call `RepoContentLoader::enabled_items()` and pass the result into the new renderer.

- [ ] **Step 6: Run tests to verify they pass**

Run:

```bash
cargo test --test repo_loader_tests --manifest-path agent-runner/Cargo.toml
cargo test --test context_run_tests --manifest-path agent-runner/Cargo.toml run_endpoint_includes_enabled_pack_items_in_context
```

Expected:
- PASS

- [ ] **Step 7: Commit**

```bash
git add agent-runner/src/context/repo_loader.rs agent-runner/src/context/context_pack.rs agent-runner/src/context/mod.rs agent-runner/src/lib.rs agent-runner/tests/repo_loader_tests.rs agent-runner/tests/context_run_tests.rs agent-runner/src/server.rs
git commit -m "feat: load repository content packs"
```

### Task 3: Add offline source sync and pack generation

**Files:**
- Create: `scripts/sync_content_sources.py`
- Create: `scripts/tests/test_sync_content_sources.py`
- Modify: `content/packs/starter-skills/manifest.json`
- Modify: `content/packs/memory-baseline/manifest.json`
- Modify: `content/packs/ov-examples/manifest.json`

- [ ] **Step 1: Write the failing sync-tool tests**

Create `scripts/tests/test_sync_content_sources.py`:

```python
import json
from pathlib import Path

from scripts.sync_content_sources import load_sources_lock, generate_pack_manifest


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
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
uv run --with pytest python -m pytest scripts/tests/test_sync_content_sources.py -q
```

Expected:
- FAIL because `scripts.sync_content_sources` does not exist

- [ ] **Step 3: Implement the sync tool**

Create `scripts/sync_content_sources.py` with:

```python
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import tempfile
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
    if import_mode == "skill_pack":
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
                    "title": example.stem,
                    "summary": f"Imported example {example.stem}",
                    "tags": ["example"],
                    "enabled_by_default": False,
                    "platform_overrides": [],
                    "upstream_path": str(example.relative_to(upstream_root)),
                }
            )
    elif import_mode == "taxonomy_only":
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
        "kind": "resource-pack" if import_mode == "copy_examples" else "skill-pack",
        "source": {
            "source_id": source_id,
            "repo_url": source_repo,
            "ref": source_ref,
            "license": source_license,
        },
        "items": items,
    }
```

and a CLI that:

```python
def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--content-root", type=Path, default=Path("content"))
    parser.add_argument("--skip-clone", action="store_true")
    args = parser.parse_args()
    ...
```

Use `git clone --depth 1` plus `git checkout <ref>` when not running with `--skip-clone`. Write `SOURCE.json` and `manifest.json` into the expected directories.

- [ ] **Step 4: Seed initial generated manifests**

Use the sync tool’s `generate_pack_manifest()` logic to create deterministic initial manifests for:

```text
content/packs/starter-skills/manifest.json
content/packs/memory-baseline/manifest.json
content/packs/ov-examples/manifest.json
```

Keep the first commit conservative:
- `starter-skills.items` may be empty until real upstream snapshot is synced
- `memory-baseline.items` should include one baseline taxonomy item
- `ov-examples.items` may be empty initially

- [ ] **Step 5: Run tests to verify they pass**

Run:

```bash
uv run --with pytest python -m pytest scripts/tests/test_sync_content_sources.py -q
```

Expected:
- PASS

- [ ] **Step 6: Commit**

```bash
git add scripts/sync_content_sources.py scripts/tests/test_sync_content_sources.py content/packs/starter-skills/manifest.json content/packs/memory-baseline/manifest.json content/packs/ov-examples/manifest.json
git commit -m "feat: add content source sync tool"
```

### Task 4: Add legacy runtime memory audit and import classification

**Files:**
- Create: `scripts/audit_legacy_runtime_memory.py`
- Create: `scripts/tests/test_audit_legacy_runtime_memory.py`

- [ ] **Step 1: Write the failing audit tests**

Create `scripts/tests/test_audit_legacy_runtime_memory.py`:

```python
from pathlib import Path

from scripts.audit_legacy_runtime_memory import classify_memory_file


def test_classify_memory_file_marks_empty_as_ignore(tmp_path: Path) -> None:
    path = tmp_path / "empty.md"
    path.write_text("   ", encoding="utf-8")
    result = classify_memory_file(path)
    assert result["classification"] == "ignore"


def test_classify_memory_file_marks_structured_content_as_candidate(tmp_path: Path) -> None:
    path = tmp_path / "prefs.md"
    path.write_text("- prefers rust\n- likes concise replies\n", encoding="utf-8")
    result = classify_memory_file(path)
    assert result["classification"] == "candidate"
    assert result["reason"] == "structured_bullet_memory"
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
uv run --with pytest python -m pytest scripts/tests/test_audit_legacy_runtime_memory.py -q
```

Expected:
- FAIL because `scripts.audit_legacy_runtime_memory` does not exist

- [ ] **Step 3: Implement the audit tool**

Create `scripts/audit_legacy_runtime_memory.py`:

```python
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
    results = audit_directory(args.root)
    payload = json.dumps({"items": results}, ensure_ascii=False, indent=2)
    if args.output:
        args.output.write_text(payload, encoding="utf-8")
    else:
        print(payload)
    return 0
```

- [ ] **Step 4: Run tests to verify they pass**

Run:

```bash
uv run --with pytest python -m pytest scripts/tests/test_audit_legacy_runtime_memory.py -q
```

Expected:
- PASS

- [ ] **Step 5: Commit**

```bash
git add scripts/audit_legacy_runtime_memory.py scripts/tests/test_audit_legacy_runtime_memory.py
git commit -m "feat: add legacy runtime memory audit tool"
```

### Task 5: Document the bootstrap workflow and verify the integrated slice

**Files:**
- Modify: `README.md`
- Modify: `docs/control-plane-integration.md`

- [ ] **Step 1: Update README with the bootstrap content flow**

Add a section to `README.md` describing:

```markdown
## Content Bootstrap

DogBot now uses a repository-managed content bootstrap flow:

- `content/sources.lock.json` pins upstream content sources
- `scripts/sync_content_sources.py` snapshots selected upstream content into `content/upstream/`
- `agent-runner` only reads normalized packs from `content/packs/`
- `scripts/audit_legacy_runtime_memory.py` audits legacy Claude runtime memory before any import
```

- [ ] **Step 2: Update control-plane integration docs**

Add to `docs/control-plane-integration.md`:

```markdown
## Content Bootstrap Checks

Run:

```bash
uv run python scripts/sync_content_sources.py --content-root ./content --skip-clone
uv run python scripts/audit_legacy_runtime_memory.py ./runtime/claude-memory --output ./content/import-report.json
```

The runtime only consumes `content/packs/` and `content/policies/`.
```

- [ ] **Step 3: Run integrated verification**

Run:

```bash
bash scripts/check_structure.sh
cargo test --test config_tests --test context_store_tests --test repo_loader_tests --test context_run_tests --manifest-path agent-runner/Cargo.toml
uv run --with pytest python -m pytest scripts/tests/test_sync_content_sources.py scripts/tests/test_audit_legacy_runtime_memory.py -q
uv run --with pytest --with fastapi --with httpx python -m pytest qq_adapter/tests wechatpadpro_adapter/tests -q
```

Expected:
- all listed suites pass

- [ ] **Step 4: Commit**

```bash
git add README.md docs/control-plane-integration.md
git commit -m "docs: describe content bootstrap workflow"
```

## Self-Review Checklist

- Each spec requirement maps to a task:
  - source lock and topology: Task 1
  - runtime pack loading: Task 2
  - sync pipeline: Task 3
  - legacy runtime memory audit: Task 4
  - docs and integration checks: Task 5
- No placeholders remain in task steps; each test and command is concrete
- Type names are consistent across the plan:
  - `PackManifest`
  - `PackSource`
  - `PackItem`
  - `RepoContentLoader::load_packs()`
  - `RepoContentLoader::enabled_items()`
