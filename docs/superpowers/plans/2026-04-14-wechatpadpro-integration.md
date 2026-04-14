# WeChatPadPro Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add optional WeChat personal-account ingress through WeChatPadPro while preserving AstrBot as the single orchestration layer.

**Architecture:** Introduce an optional WeChatPadPro deployment stack alongside the existing NapCat stack, wire it into repository env/config/scripts, and document how AstrBot should connect to it. Reuse the existing platform-neutral AstrBot bridge and `agent-runner` unchanged unless verification exposes a compatibility gap.

**Tech Stack:** Docker Compose, shell scripts, Rust verification commands, AstrBot WebUI configuration, Markdown docs.

---

### Task 1: Add WeChatPadPro deployment stack

**Files:**
- Create: `compose/wechatpadpro-stack.yml`

- [ ] **Step 1: Write the deployment stack file**

```yaml
services:
  wechatpadpro_mysql:
    image: mysql:8.0
    container_name: ${WECHATPADPRO_MYSQL_CONTAINER_NAME:-wechatpadpro_mysql}
    restart: unless-stopped
    command:
      - --default-authentication-plugin=mysql_native_password
      - --character-set-server=utf8mb4
      - --collation-server=utf8mb4_unicode_ci
    environment:
      MYSQL_ROOT_PASSWORD: ${WECHATPADPRO_MYSQL_ROOT_PASSWORD}
      MYSQL_DATABASE: ${WECHATPADPRO_MYSQL_DATABASE:-weixin}
      MYSQL_USER: ${WECHATPADPRO_MYSQL_USER:-weixin}
      MYSQL_PASSWORD: ${WECHATPADPRO_MYSQL_PASSWORD}
    volumes:
      - "${WECHATPADPRO_MYSQL_DIR:-/srv/wechatpadpro/mysql}:/var/lib/mysql"
    healthcheck:
      test: ["CMD-SHELL", "mysqladmin ping -h 127.0.0.1 -uroot -p$$MYSQL_ROOT_PASSWORD"]
      interval: 10s
      timeout: 5s
      retries: 20

  wechatpadpro_redis:
    image: redis:7-alpine
    container_name: ${WECHATPADPRO_REDIS_CONTAINER_NAME:-wechatpadpro_redis}
    restart: unless-stopped
    command: ["redis-server", "--requirepass", "${WECHATPADPRO_REDIS_PASSWORD}"]
    volumes:
      - "${WECHATPADPRO_REDIS_DIR:-/srv/wechatpadpro/redis}:/data"
    healthcheck:
      test: ["CMD-SHELL", "redis-cli -a $$WECHATPADPRO_REDIS_PASSWORD ping | grep PONG"]
      interval: 10s
      timeout: 5s
      retries: 20

  wechatpadpro:
    image: ${WECHATPADPRO_IMAGE}
    container_name: ${WECHATPADPRO_CONTAINER_NAME:-wechatpadpro}
    restart: unless-stopped
    depends_on:
      wechatpadpro_mysql:
        condition: service_healthy
      wechatpadpro_redis:
        condition: service_healthy
    ports:
      - "${WECHATPADPRO_HOST_PORT:-38849}:1238"
    environment:
      MYSQL_ROOT_PASSWORD: ${WECHATPADPRO_MYSQL_ROOT_PASSWORD}
      MYSQL_DATABASE: ${WECHATPADPRO_MYSQL_DATABASE:-weixin}
      MYSQL_USER: ${WECHATPADPRO_MYSQL_USER:-weixin}
      MYSQL_PASSWORD: ${WECHATPADPRO_MYSQL_PASSWORD}
      MYSQL_PORT: ${WECHATPADPRO_MYSQL_PORT:-3306}
      REDIS_PASSWORD: ${WECHATPADPRO_REDIS_PASSWORD}
      REDIS_PORT: ${WECHATPADPRO_REDIS_PORT:-6379}
      WECHAT_PORT: ${WECHATPADPRO_WECHAT_PORT:-8080}
      DB_HOST: ${WECHATPADPRO_MYSQL_CONTAINER_NAME:-wechatpadpro_mysql}
      DB_PORT: ${WECHATPADPRO_MYSQL_PORT:-3306}
      DB_DATABASE: ${WECHATPADPRO_MYSQL_DATABASE:-weixin}
      DB_USERNAME: ${WECHATPADPRO_MYSQL_USER:-weixin}
      DB_PASSWORD: ${WECHATPADPRO_MYSQL_PASSWORD}
      REDIS_HOST: ${WECHATPADPRO_REDIS_CONTAINER_NAME:-wechatpadpro_redis}
      REDIS_DB: ${WECHATPADPRO_REDIS_DB:-0}
      ADMIN_KEY: ${WECHATPADPRO_ADMIN_KEY}
    volumes:
      - "${WECHATPADPRO_DATA_DIR:-/srv/wechatpadpro/data}:/app/data"
```

- [ ] **Step 2: Sanity-check the YAML**

Run: `sed -n '1,260p' compose/wechatpadpro-stack.yml`
Expected: file exists and contains `wechatpadpro_mysql`, `wechatpadpro_redis`, and `wechatpadpro`

- [ ] **Step 3: Commit**

```bash
git add compose/wechatpadpro-stack.yml
git commit -m "feat: add WeChatPadPro compose stack"
```

### Task 2: Extend env template and deploy scripts

**Files:**
- Modify: `deploy/myqqbot.env.example`
- Modify: `scripts/deploy_stack.sh`
- Modify: `scripts/stop_stack.sh`

- [ ] **Step 1: Write the WeChatPadPro env section**

```env
ENABLE_WECHATPADPRO=0
WECHATPADPRO_IMAGE=ghcr.io/bclz-wyz/wechatpadpro:latest
WECHATPADPRO_CONTAINER_NAME=wechatpadpro
WECHATPADPRO_HOST_PORT=38849
WECHATPADPRO_ADMIN_KEY=change-me
WECHATPADPRO_DATA_DIR=/srv/wechatpadpro/data

WECHATPADPRO_MYSQL_CONTAINER_NAME=wechatpadpro_mysql
WECHATPADPRO_MYSQL_ROOT_PASSWORD=change-me-root
WECHATPADPRO_MYSQL_DATABASE=weixin
WECHATPADPRO_MYSQL_USER=weixin
WECHATPADPRO_MYSQL_PASSWORD=change-me-db
WECHATPADPRO_MYSQL_PORT=3306
WECHATPADPRO_MYSQL_DIR=/srv/wechatpadpro/mysql

WECHATPADPRO_REDIS_CONTAINER_NAME=wechatpadpro_redis
WECHATPADPRO_REDIS_PASSWORD=change-me-redis
WECHATPADPRO_REDIS_PORT=6379
WECHATPADPRO_REDIS_DB=0
WECHATPADPRO_REDIS_DIR=/srv/wechatpadpro/redis

WECHATPADPRO_WECHAT_PORT=8080
```

- [ ] **Step 2: Update `deploy_stack.sh` to create directories and conditionally start WeChatPadPro**

```bash
mkdir -p \
  "${WECHATPADPRO_DATA_DIR:-/srv/wechatpadpro/data}" \
  "${WECHATPADPRO_MYSQL_DIR:-/srv/wechatpadpro/mysql}" \
  "${WECHATPADPRO_REDIS_DIR:-/srv/wechatpadpro/redis}"

if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
  if [[ "$compose_cmd" == "docker compose" ]]; then
    docker compose --env-file "$env_file" -f "$repo_root/compose/wechatpadpro-stack.yml" up -d
  else
    docker-compose --env-file "$env_file" -f "$repo_root/compose/wechatpadpro-stack.yml" up -d
  fi
fi
```

- [ ] **Step 3: Update `stop_stack.sh` to conditionally stop WeChatPadPro**

```bash
if [[ "${ENABLE_WECHATPADPRO:-0}" == "1" ]]; then
  if [[ "$compose_cmd" == "docker compose" ]]; then
    docker compose --env-file "$env_file" -f "$repo_root/compose/wechatpadpro-stack.yml" down
  else
    docker-compose --env-file "$env_file" -f "$repo_root/compose/wechatpadpro-stack.yml" down
  fi
fi
```

- [ ] **Step 4: Verify the scripts still parse**

Run:
- `bash -n scripts/deploy_stack.sh`
- `bash -n scripts/stop_stack.sh`

Expected: no output, zero exit code

- [ ] **Step 5: Commit**

```bash
git add deploy/myqqbot.env.example scripts/deploy_stack.sh scripts/stop_stack.sh
git commit -m "feat: add optional WeChatPadPro deploy controls"
```

### Task 3: Document AstrBot + WeChatPadPro setup

**Files:**
- Modify: `deploy/README.md`
- Modify: `README.md`

- [ ] **Step 1: Add WeChatPadPro architecture and dependency notes**

```md
When `ENABLE_WECHATPADPRO=1`, the stack becomes:

QQ -> NapCat -> AstrBot -> agent-runner
WeChat -> WeChatPadPro -> AstrBot -> agent-runner

Additional services started:
- `wechatpadpro`
- `wechatpadpro_mysql`
- `wechatpadpro_redis`
```

- [ ] **Step 2: Add env documentation for WeChatPadPro**

```md
Important WeChatPadPro settings:

- `ENABLE_WECHATPADPRO=1`
- `WECHATPADPRO_HOST_PORT`
- `WECHATPADPRO_ADMIN_KEY`
- `WECHATPADPRO_DATA_DIR`
- `WECHATPADPRO_MYSQL_*`
- `WECHATPADPRO_REDIS_*`
```

- [ ] **Step 3: Add AstrBot WebUI configuration instructions**

```md
In AstrBot WebUI:

1. Open `消息平台`
2. Add adapter `wechatpadpro(微信)`
3. Fill:
   - `admin_key`: `WECHATPADPRO_ADMIN_KEY`
   - `host`: host/IP where WeChatPadPro is reachable
   - `port`: `WECHATPADPRO_HOST_PORT`
4. Save and check AstrBot console logs
5. Scan the QR code shown in AstrBot logs

If no messages arrive after login, enable AstrBot's `是否启用主动消息轮询` for the adapter.
```

- [ ] **Step 4: Add warning and compatibility notes**

```md
Notes:

- WeChatPadPro is non-official and carries account-risk.
- It requires a logged-in phone for the same account.
- The Docker path is documented as Linux-only and not arm64-friendly in AstrBot docs.
- This repository does not store WeChatPadPro auth codes in git.
```

- [ ] **Step 5: Verify documentation references**

Run:
- `rg -n "WeChatPadPro|wechatpadpro" README.md deploy/README.md deploy/myqqbot.env.example`

Expected: entries appear in all three files

- [ ] **Step 6: Commit**

```bash
git add README.md deploy/README.md deploy/myqqbot.env.example
git commit -m "docs: add WeChatPadPro deployment guide"
```

### Task 4: Verify repository state

**Files:**
- Test: `compose/wechatpadpro-stack.yml`
- Test: `scripts/deploy_stack.sh`
- Test: `scripts/stop_stack.sh`

- [ ] **Step 1: Run project verification**

Run:
- `cargo test --manifest-path agent-runner/Cargo.toml`
- `./scripts/check_structure.sh`

Expected: all tests pass; structure check passes

- [ ] **Step 2: Run shell/document verification**

Run:
- `bash -n scripts/deploy_stack.sh`
- `bash -n scripts/stop_stack.sh`
- `sed -n '1,220p' compose/wechatpadpro-stack.yml`

Expected: scripts parse; compose file is present and readable

- [ ] **Step 3: Review git diff**

Run: `git diff --stat HEAD~3..HEAD`
Expected: only WeChatPadPro deployment/docs changes plus no accidental runtime state files

- [ ] **Step 4: Commit final touch-ups if needed**

```bash
git add -A
git commit -m "chore: finalize WeChatPadPro integration scaffolding"
```
