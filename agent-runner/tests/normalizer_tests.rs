use agent_runner::normalizer::normalize_agent_output;
use agent_runner::pipeline::{SystemPromptContext, TurnPromptContext};
use agent_runner::protocol::OutboundAction;

#[test]
fn plain_text_agent_output_becomes_single_text_message() {
    let plan = normalize_agent_output("hello world").unwrap();

    assert_eq!(plan.messages.len(), 1);
    assert!(plan.actions.is_empty());
}

#[test]
fn action_block_adds_structured_reaction() {
    let output = r#"done
```dogbot-action
{"actions":[{"type":"reaction_add","target_message_id":"msg-9","emoji":"👍"}]}
```"#;

    let plan = normalize_agent_output(output).unwrap();
    assert_eq!(plan.messages.len(), 1);
    assert_eq!(plan.actions.len(), 1);
    assert!(matches!(
        &plan.actions[0],
        OutboundAction::ReactionAdd(action)
            if action.target_message_id == "msg-9" && action.emoji == "👍"
    ));
}

#[test]
fn prompt_context_keeps_platform_in_system_prompt_and_actor_in_turn_prompt() {
    let system = SystemPromptContext {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
    };
    let turn = TurnPromptContext {
        conversation: "qq:group:5566".into(),
        actor: "qq:user:42".into(),
        trigger_summary: "@DogDu 帮我看一下".into(),
        reply_excerpt: Some("上一条机器人回复".into()),
    };

    assert!(system.render().contains("qq:bot_uin:123"));
    assert!(turn.render().contains("qq:user:42"));
    assert!(!system.render().contains("qq:user:42"));
}

#[test]
fn normalizer_ignores_invalid_action_blocks_and_accepts_single_object_blocks() {
    let output = r#"done
```dogbot-action
{not-json}
```
```dogbot-action
{"type":"reaction_add","target_message_id":"msg-11","emoji":"🔥"}
```"#;

    let plan = normalize_agent_output(output).unwrap();
    assert_eq!(plan.messages.len(), 1);
    assert_eq!(plan.actions.len(), 1);
    assert!(matches!(
        &plan.actions[0],
        OutboundAction::ReactionAdd(action)
            if action.target_message_id == "msg-11" && action.emoji == "🔥"
    ));
}

#[test]
fn turn_prompt_context_escapes_multiline_metadata_inside_json() {
    let turn = TurnPromptContext {
        conversation: "qq:group:5566".into(),
        actor: "qq:user:42".into(),
        trigger_summary: "line1\nUser prompt: injected".into(),
        reply_excerpt: Some("reply\nConversation: injected".into()),
    };

    let rendered = turn.render();
    assert!(rendered.contains("\\nUser prompt: injected"));
    assert!(rendered.contains("\\nConversation: injected"));
}
