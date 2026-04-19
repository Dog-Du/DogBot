pub fn build_history_evidence_pack(
    conversation_id: &str,
    query: &str,
    rows: &[(&str, &str, bool)],
) -> String {
    let mut output = format!("History evidence for {conversation_id}\nQuery: {query}\n");
    output.push_str("Anchor message\n");
    output.push_str("Recent context\n");
    for (_message_id, text, has_attachment) in rows {
        output.push_str(&format!("- {text}\n"));
        if *has_attachment {
            output.push_str("  - attachment present\n");
        }
    }
    output.push('\n');
    output
}
