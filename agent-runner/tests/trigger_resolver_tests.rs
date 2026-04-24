use agent_runner::protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart};
use agent_runner::trigger_resolver::{TriggerDecision, TriggerResolver};

fn message_event(
    conversation: &str,
    platform_account: &str,
    text: &str,
    mentions: Vec<&str>,
    reply_to: Option<&str>,
) -> CanonicalEvent {
    CanonicalEvent {
        platform: conversation
            .split(':')
            .next()
            .unwrap_or_default()
            .to_string(),
        platform_account: platform_account.into(),
        conversation: conversation.into(),
        actor: "platform:user:1".into(),
        event_id: "evt-1".into(),
        timestamp_epoch_secs: 1,
        kind: EventKind::Message {
            message: CanonicalMessage {
                message_id: "m1".into(),
                reply_to: reply_to.map(str::to_string),
                parts: if text.is_empty() {
                    Vec::new()
                } else {
                    vec![MessagePart::Text { text: text.into() }]
                },
                mentions: mentions.into_iter().map(str::to_string).collect(),
                native_metadata: serde_json::json!({}),
            },
        },
        raw_native_payload: serde_json::json!({}),
    }
}

#[test]
fn private_message_runs_for_any_non_empty_text() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "qq:private:1",
        "qq:bot_uin:123",
        "请帮我总结一下",
        vec![],
        None,
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn private_status_command_is_still_supported() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "qq:private:1",
        "qq:bot_uin:123",
        "/agent-status",
        vec![],
        None,
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Status);
}

#[test]
fn group_message_requires_bot_mention() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "wechatpadpro:group:123@chatroom",
        "wechatpadpro:account:wxid_bot",
        "看这个",
        vec![],
        None,
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn group_message_runs_when_bot_mention_present_without_command_prefix() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "qq:group:100",
        "qq:bot_uin:123",
        "麻烦看下",
        vec!["qq:bot_uin:123"],
        None,
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn group_message_runs_when_mention_has_no_extra_text() {
    let resolver = TriggerResolver::default();
    let message = CanonicalEvent {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:wxid_bot".into(),
        conversation: "wechatpadpro:group:123@chatroom".into(),
        actor: "wechatpadpro:user:wxid_user".into(),
        event_id: "evt-4".into(),
        timestamp_epoch_secs: 1,
        kind: EventKind::Message {
            message: CanonicalMessage {
                message_id: "m4".into(),
                reply_to: None,
                parts: vec![MessagePart::Mention {
                    actor_id: "wechatpadpro:account:wxid_bot".into(),
                    display: "@bot".into(),
                }],
                mentions: vec!["wechatpadpro:account:wxid_bot".into()],
                native_metadata: serde_json::json!({}),
            },
        },
        raw_native_payload: serde_json::json!({}),
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn group_message_mentioning_someone_else_does_not_run() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "qq:group:100",
        "qq:bot_uin:123",
        "但是 @别的人",
        vec!["qq:user:999"],
        None,
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn group_message_status_command_requires_bot_mention() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "wechatpadpro:group:123@chatroom",
        "wechatpadpro:account:wxid_bot",
        "/agent-status",
        vec![],
        None,
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn group_message_status_command_with_bot_mention_returns_status() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "wechatpadpro:group:123@chatroom",
        "wechatpadpro:account:wxid_bot",
        "/agent-status",
        vec!["wechatpadpro:account:wxid_bot"],
        None,
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Status);
}

#[test]
fn private_non_status_command_like_text_still_runs() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "qq:private:1",
        "qq:bot_uin:123",
        "这个是 /agent-status2 不是状态命令，/agented 也不是",
        vec![],
        None,
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn group_reply_without_mention_does_not_run() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "qq:group:100",
        "qq:bot_uin:123",
        "我补充一下",
        vec![],
        Some("bot-msg-1"),
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn non_group_non_private_message_does_not_run() {
    let resolver = TriggerResolver::default();
    let message = message_event(
        "qq:system:1",
        "qq:bot_uin:123",
        "still should not run",
        vec!["qq:bot_uin:123"],
        Some("bot-msg-1"),
    );

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}
