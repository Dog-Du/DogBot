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
}
