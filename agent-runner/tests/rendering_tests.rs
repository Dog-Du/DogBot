use agent_runner::rendering::{degrade_markdown, parse_media_actions};

#[test]
fn markdown_degradation_keeps_lists_and_urls() {
    let rendered = degrade_markdown("## Title\n\n- item\n\n[link](https://example.com)");
    assert!(rendered.contains("Title"));
    assert!(rendered.contains("- item"));
    assert!(rendered.contains("https://example.com"));
}

#[test]
fn markdown_degradation_preserves_parentheses_and_generic_link_labels() {
    let rendered = degrade_markdown(
        "Keep (parentheses) and [docs](https://example.com/docs) in place.",
    );
    assert!(rendered.contains("Keep (parentheses)"));
    assert!(rendered.contains("docs: https://example.com/docs"));
}

#[test]
fn media_action_parser_extracts_send_image_block() {
    let stdout = r#"before
```dogbot-action
{"type":"send_image","source_type":"remote_url","source_value":"https://example.com/a.png","caption_text":"done","target_conversation":"qq:group:100"}
```
"#;

    let actions = parse_media_actions(stdout);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action_type, "send_image");
}

#[test]
fn media_action_parser_handles_multiple_blocks_and_ignores_invalid_json() {
    let stdout = "before\r\n```dogbot-action\r\n{\"type\":\"send_image\",\"source_type\":\"remote_url\",\"source_value\":\"https://example.com/a.png\",\"caption_text\":\"done\",\"target_conversation\":\"qq:group:100\"}\r\n```\r\nmiddle\r\n```dogbot-action\r\n{not-json}\r\n```\r\n```dogbot-action\r\n{\"type\":\"send_image\",\"source_type\":\"stored_asset\",\"source_value\":\"asset-1\",\"caption_text\":null,\"target_conversation\":\"qq:private:1\"}\r\n```\r\nafter";

    let actions = parse_media_actions(stdout);
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].source_value, "https://example.com/a.png");
    assert_eq!(actions[1].source_type, "stored_asset");
}
