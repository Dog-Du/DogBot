use agent_runner::history::retrieval::build_history_evidence_pack;

#[test]
fn retrieval_pack_includes_anchor_recent_window_and_attachment_stub() {
    let pack = build_history_evidence_pack(
        "qq:group:100",
        "当前问题 /agent 总结",
        &[
            ("m1", "机器人上一条消息", false),
            ("m2", "用户补充消息", true),
        ],
    );

    assert!(pack.contains("Anchor message"));
    assert!(pack.contains("Recent context"));
    assert!(pack.contains("attachment present"));
}
