use std::fs;

#[test]
fn compose_defines_required_claude_runner_limits() {
    let compose = fs::read_to_string("../compose/docker-compose.yml").expect("failed to read compose file");
    let required = [
        "cpus: \"2.0\"",
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
}
