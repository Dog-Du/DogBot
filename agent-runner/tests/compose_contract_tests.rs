use std::fs;

#[test]
fn compose_defines_required_claude_runner_limits() {
    let compose =
        fs::read_to_string("../compose/docker-compose.yml").expect("failed to read compose file");
    let required = [
        "image: ${CLAUDE_IMAGE_NAME:-dogbot/claude-runner:local}",
        "cpus: \"4.0\"",
        "mem_limit: 4g",
        "memswap_limit: 4g",
        "pids_limit: 256",
        "read_only: true",
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
        "PACKYAPI_KEY",
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
    let dockerfile = fs::read_to_string("../docker/claude-runner/Dockerfile")
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
