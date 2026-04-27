use std::fs;

#[test]
fn claude_prompt_lists_reply_format_skill_in_claude_md() {
    let claude_md =
        fs::read_to_string("../claude-prompt/CLAUDE.md").expect("failed to read CLAUDE.md");

    assert!(
        claude_md.contains("skills/reply-format/SKILL.md"),
        "CLAUDE.md must reference the reply-format skill"
    );
    assert!(
        claude_md.contains("MUST read"),
        "CLAUDE.md must require reading the reply-format skill"
    );
}

#[test]
fn claude_prompt_lists_history_read_skill_in_claude_md() {
    let claude_md =
        fs::read_to_string("../claude-prompt/CLAUDE.md").expect("failed to read CLAUDE.md");

    assert!(
        claude_md.contains("skills/history-read/SKILL.md"),
        "CLAUDE.md must reference the history-read skill"
    );
}

#[test]
fn claude_prompt_guides_long_running_commands_to_background() {
    let claude_md =
        fs::read_to_string("../claude-prompt/CLAUDE.md").expect("failed to read CLAUDE.md");

    assert!(
        claude_md.contains("Long-running commands"),
        "CLAUDE.md must document long-running command behavior"
    );
    assert!(
        claude_md.contains("nohup <command>")
            && claude_md.contains("/workspace/.run/logs")
            && claude_md.contains("Return early"),
        "CLAUDE.md must tell the agent to background long tasks and return early"
    );
}

#[test]
fn reply_format_skill_exists_and_mentions_no_markdown_rule() {
    let skill = fs::read_to_string("../claude-prompt/skills/reply-format/SKILL.md")
        .expect("failed to read reply-format skill");

    assert!(
        skill.contains("dogbot-action"),
        "reply-format skill must document dogbot-action blocks"
    );
    assert!(
        skill.contains("Do not use Markdown"),
        "reply-format skill must forbid Markdown output"
    );
    assert!(
        skill.contains("/workspace"),
        "reply-format skill must document /workspace media constraints"
    );
    assert!(
        skill.contains("trigger_message_id") && skill.contains("mention_refs"),
        "reply-format skill must explain trigger message metadata"
    );
    assert!(
        skill.contains("\"reply_to\":null") && skill.contains("\"reply_to\":\""),
        "reply-format skill must document reply_to override semantics"
    );
    assert!(
        skill.contains("reaction_add")
            && skill.contains("trigger_message_id")
            && skill.contains("explicitly asks for a reaction"),
        "reply-format skill must explain when to prefer structured reaction output"
    );
    assert!(
        skill.contains("mainly social or interactive")
            && skill.contains("only use one of these emojis")
            && skill.contains("🫡")
            && skill.contains("❤"),
        "reply-format skill must define autonomous social reaction policy and emoji whitelist"
    );
}
