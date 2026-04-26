use std::fs;

#[test]
fn compose_defines_required_claude_runner_limits() {
    let compose =
        fs::read_to_string("../deploy/docker/docker-compose.yml")
            .expect("failed to read compose file");
    let required = [
        "image: ${CLAUDE_IMAGE_NAME:-dogbot/claude-runner:local}",
        "cpus: \"4.0\"",
        "mem_limit: 4g",
        "memswap_limit: 4g",
        "pids_limit: 256",
        "read_only: true",
        "ANTHROPIC_BASE_URL: ${ANTHROPIC_BASE_URL:-http://127.0.0.1:${BIFROST_PORT:-8080}/anthropic}",
        "ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY:-dummy}",
        "ANTHROPIC_DEFAULT_SONNET_MODEL: ${BIFROST_MODEL:-primary/model-id}",
        "BIFROST_UPSTREAM_BASE_URL: ${BIFROST_UPSTREAM_BASE_URL:-https://example.com}",
        "BIFROST_UPSTREAM_API_KEY: ${BIFROST_UPSTREAM_API_KEY:-replace-me}",
        "- /tmp:size=256m,mode=1777",
        "/workspace",
        "/state",
    ];

    for item in required {
        assert!(
            compose.contains(item),
            "missing compose contract entry: {item}"
        );
    }

    let forbidden = [
        "/var/run/docker.sock",
        "/root",
        "/home",
        "API_PROXY_UPSTREAM_TOKEN",
        "MINIMAX_API_KEY",
    ];

    for item in forbidden {
        assert!(
            !compose.contains(item),
            "compose should not expose forbidden entry: {item}"
        );
    }
}

#[test]
fn dockerfile_bootstrap_repairs_claude_state_permissions() {
    let dockerfile = fs::read_to_string("../deploy/docker/Dockerfile")
        .expect("failed to read dockerfile");

    let required = [
        "mkdir -p /workspace /state /state/claude /state/claude/debug",
        "chown -R claude:claude /state/claude",
    ];

    for item in required {
        assert!(
            dockerfile.contains(item),
            "missing claude bootstrap contract entry: {item}"
        );
    }
}

#[test]
fn claude_runner_entrypoint_execs_runtime_launch_script() {
    let entrypoint = fs::read_to_string("../deploy/docker/entrypoint.sh")
        .expect("failed to read entrypoint file");
    let common =
        fs::read_to_string("../scripts/lib/common.sh").expect("failed to read common helper file");

    let required = [
        "/usr/local/bin/claude-bootstrap.sh",
        "/state/claude-runner/launch.sh",
        "exec \"$launch_script\"",
    ];

    for item in required {
        assert!(
            entrypoint.contains(item),
            "missing runtime launch entrypoint contract entry: {item}"
        );
    }

    assert!(
        !entrypoint.contains("bifrost -host 127.0.0.1"),
        "entrypoint should stay thin and should not start bifrost directly"
    );

    let helper_required = [
        "dogbot_write_claude_runner_runtime",
        "prompt_root=\"/state/claude-prompt\"",
        "bifrost -host 127.0.0.1 -port \"$port\" -app-dir \"$bifrost_dir\"",
        "config.json",
        "default_model=\"${BIFROST_MODEL:-primary/model-id}\"",
        "stripped_model=\"${default_model#*/}\"",
        "[$default_model, $stripped_model]",
    ];

    for item in helper_required {
        assert!(
            common.contains(item),
            "missing generated runtime launch contract entry: {item}"
        );
    }

    assert!(
        !common.contains("\"*\""),
        "generated runtime launch should not rely on wildcard model matching"
    );
    assert!(
        !common.contains("ensure_link \"$prompt_root/CLAUDE.md\""),
        "generated runtime launch should not project CLAUDE.md into /workspace"
    );
    assert!(
        !common.contains("ensure_link \"$prompt_root/persona.md\""),
        "generated runtime launch should not project persona.md into /workspace"
    );
    assert!(
        !common.contains("ensure_link \"$prompt_root/.claude\""),
        "generated runtime launch should not project .claude into /workspace"
    );
}
