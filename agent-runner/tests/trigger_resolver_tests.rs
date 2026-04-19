use agent_runner::inbound_models::InboundMessage;
use agent_runner::trigger_resolver::{TriggerDecision, TriggerResolver};

#[test]
fn private_message_requires_agent_token_anywhere_in_normalized_text() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:private:1".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m1".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "请帮我 /agent 总结一下".into(),
        mentions: vec![],
        is_group: false,
        is_private: true,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn group_message_requires_agent_token_and_mention_or_reply() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:wxid_bot".into(),
        conversation_id: "wechat:group:123@chatroom".into(),
        actor_id: "wechat:user:wxid_user".into(),
        message_id: "m2".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "/agent 看这个".into(),
        mentions: vec![],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn group_message_runs_when_agent_token_and_mention_present() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:group:100".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m3".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "麻烦 /agent 看下".into(),
        mentions: vec!["qq:bot_uin:123".into()],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn group_message_runs_when_agent_token_and_reply_present() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:wxid_bot".into(),
        conversation_id: "wechat:group:123@chatroom".into(),
        actor_id: "wechat:user:wxid_user".into(),
        message_id: "m4".into(),
        reply_to_message_id: Some("bot-msg-1".into()),
        raw_segments_json: "[]".into(),
        normalized_text: "再补充一下 /agent".into(),
        mentions: vec![],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn group_message_mentioning_someone_else_does_not_run() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:group:100".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m5".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "/agent 但是 @别的人".into(),
        mentions: vec!["qq:user:999".into()],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn group_message_status_command_bypasses_mention_gate() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:wxid_bot".into(),
        conversation_id: "wechat:group:123@chatroom".into(),
        actor_id: "wechat:user:wxid_user".into(),
        message_id: "m6".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "看下 /agent-status".into(),
        mentions: vec![],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Status);
}

#[test]
fn commands_require_token_boundaries() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:private:1".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m7".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "这个是 /agent-status2 不是状态命令，/agented 也不是".into(),
        mentions: vec![],
        is_group: false,
        is_private: true,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn non_group_non_private_message_does_not_run_with_reply_or_mention() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:system:1".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m8".into(),
        reply_to_message_id: Some("bot-msg-1".into()),
        raw_segments_json: "[]".into(),
        normalized_text: "/agent still should not run".into(),
        mentions: vec!["qq:bot_uin:123".into()],
        is_group: false,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}
