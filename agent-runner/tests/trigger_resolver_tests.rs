use agent_runner::inbound_models::InboundMessage;
use agent_runner::trigger_resolver::{TriggerDecision, TriggerResolver};

#[test]
fn private_message_runs_for_any_non_empty_text() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:private:1".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m1".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "请帮我总结一下".into(),
        mentions: vec![],
        is_group: false,
        is_private: true,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn private_status_command_is_still_supported() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:private:1".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m-status".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "/agent-status".into(),
        mentions: vec![],
        is_group: false,
        is_private: true,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Status);
}

#[test]
fn group_message_requires_bot_mention() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:wxid_bot".into(),
        conversation_id: "wechat:group:123@chatroom".into(),
        actor_id: "wechat:user:wxid_user".into(),
        message_id: "m2".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "看这个".into(),
        mentions: vec![],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn group_message_runs_when_bot_mention_present_without_command_prefix() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:group:100".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m3".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "麻烦看下".into(),
        mentions: vec!["qq:bot_uin:123".into()],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn group_message_runs_when_mention_has_no_extra_text() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:wxid_bot".into(),
        conversation_id: "wechat:group:123@chatroom".into(),
        actor_id: "wechat:user:wxid_user".into(),
        message_id: "m4".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "".into(),
        mentions: vec!["wechatpadpro:account:wxid_bot".into()],
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
        normalized_text: "但是 @别的人".into(),
        mentions: vec!["qq:user:999".into()],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn group_message_status_command_requires_bot_mention() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:wxid_bot".into(),
        conversation_id: "wechat:group:123@chatroom".into(),
        actor_id: "wechat:user:wxid_user".into(),
        message_id: "m6".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "/agent-status".into(),
        mentions: vec![],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn group_message_status_command_with_bot_mention_returns_status() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "wechatpadpro".into(),
        platform_account: "wechatpadpro:account:wxid_bot".into(),
        conversation_id: "wechat:group:123@chatroom".into(),
        actor_id: "wechat:user:wxid_user".into(),
        message_id: "m6b".into(),
        reply_to_message_id: None,
        raw_segments_json: "[]".into(),
        normalized_text: "/agent-status".into(),
        mentions: vec!["wechatpadpro:account:wxid_bot".into()],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Status);
}

#[test]
fn private_non_status_command_like_text_still_runs() {
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

    assert_eq!(resolver.resolve(&message), TriggerDecision::Run);
}

#[test]
fn group_reply_without_mention_does_not_run() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:group:100".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m8".into(),
        reply_to_message_id: Some("bot-msg-1".into()),
        raw_segments_json: "[]".into(),
        normalized_text: "我补充一下".into(),
        mentions: vec![],
        is_group: true,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}

#[test]
fn non_group_non_private_message_does_not_run() {
    let resolver = TriggerResolver::default();
    let message = InboundMessage {
        platform: "qq".into(),
        platform_account: "qq:bot_uin:123".into(),
        conversation_id: "qq:system:1".into(),
        actor_id: "qq:user:1".into(),
        message_id: "m9".into(),
        reply_to_message_id: Some("bot-msg-1".into()),
        raw_segments_json: "[]".into(),
        normalized_text: "still should not run".into(),
        mentions: vec!["qq:bot_uin:123".into()],
        is_group: false,
        is_private: false,
        timestamp_epoch_secs: 1,
    };

    assert_eq!(resolver.resolve(&message), TriggerDecision::Ignore);
}
