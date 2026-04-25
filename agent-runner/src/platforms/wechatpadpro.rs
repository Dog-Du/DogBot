use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::{Map, Value, json};

use crate::{
    config::Settings,
    dispatch::{CapabilityProfile, dispatch_plan},
    models::{ErrorResponse, MessageResponse},
    platforms::{
        DeliveryContext, IngressRoute, PlatformAdapter,
        common::{integer_value, string_value},
    },
    protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart, OutboundPlan},
};

const INGRESS_ROUTES: &[IngressRoute] = &[IngressRoute {
    path: "/v1/platforms/wechatpadpro/events",
    allow_head: true,
}];

pub struct WeChatPadProPlatform {
    client: reqwest::Client,
    base_url: String,
    account_key: Option<String>,
    platform_account: String,
    mention_names: Vec<String>,
}

impl WeChatPadProPlatform {
    pub fn from_settings(settings: &Settings) -> Result<Self, ErrorResponse> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|err| internal_error(&format!("failed to build WeChatPadPro client: {err}")))?;

        Ok(Self {
            client,
            base_url: settings.wechatpadpro_base_url.trim_end_matches('/').to_string(),
            account_key: settings.wechatpadpro_account_key.clone(),
            platform_account: settings
                .platform_wechatpadpro_account_id
                .clone()
                .unwrap_or_else(|| "wechatpadpro:account:bot".to_string()),
            mention_names: settings.platform_wechatpadpro_bot_mention_names.clone(),
        })
    }
}

#[async_trait]
impl PlatformAdapter for WeChatPadProPlatform {
    fn platform_id(&self) -> &'static str {
        "wechatpadpro"
    }

    fn ingress_routes(&self) -> &'static [IngressRoute] {
        INGRESS_ROUTES
    }

    fn decode_event(&self, payload: &Value) -> Option<CanonicalEvent> {
        let mention_names = self.mention_names.iter().map(String::as_str).collect::<Vec<_>>();
        decode_webhook_event(payload, &self.platform_account, &mention_names)
    }

    fn capabilities(&self) -> CapabilityProfile {
        CapabilityProfile {
            supports_reply: false,
            supports_reaction: false,
            supports_sticker: false,
        }
    }

    async fn send_plan(
        &self,
        context: &DeliveryContext,
        plan: &OutboundPlan,
    ) -> Result<MessageResponse, ErrorResponse> {
        dispatch_plan(self.platform_id(), &self.capabilities(), plan)
            .await
            .map_err(|message| ErrorResponse {
                status: "error".into(),
                error_code: "delivery_invalid_plan".into(),
                message,
                timed_out: false,
            })?;

        let account_key = self.account_key.as_deref().ok_or_else(|| ErrorResponse {
            status: "error".into(),
            error_code: "delivery_unavailable".into(),
            message: "WECHATPADPRO_ACCOUNT_KEY is not configured".into(),
            timed_out: false,
        })?;

        let mut last_response = MessageResponse {
            status: "ok".into(),
            message_id: None,
        };

        for message in &plan.messages {
            let text = compile_message_text(message).map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "delivery_invalid_plan".into(),
                message: err,
                timed_out: false,
            })?;

            let payload = if let Some(native_event) = context.native_event.as_ref() {
                compile_text_reply(native_event, &text)
            } else {
                build_text_reply_from_context(context, &text)
            };

            let response = self
                .client
                .post(format!("{}/message/SendTextMessage", self.base_url))
                .query(&[("key", account_key)])
                .json(&payload)
                .send()
                .await
                .map_err(|err| ErrorResponse {
                    status: "error".into(),
                    error_code: "delivery_unavailable".into(),
                    message: format!("failed to reach WeChatPadPro API: {err}"),
                    timed_out: false,
                })?;

            let status = response.status();
            let body: Value = response.json().await.map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "delivery_invalid_response".into(),
                message: format!("WeChatPadPro SendTextMessage returned non-JSON response: {err}"),
                timed_out: false,
            })?;

            if !status.is_success() || body.get("Code").and_then(Value::as_i64) != Some(200) {
                return Err(ErrorResponse {
                    status: "error".into(),
                    error_code: "delivery_failed".into(),
                    message: format!("WeChatPadPro SendTextMessage failed: {body}"),
                    timed_out: false,
                });
            }

            last_response = MessageResponse {
                status: "ok".into(),
                message_id: body
                    .get("Data")
                    .and_then(|data| data.get("MsgId"))
                    .and_then(string_value)
                    .or_else(|| body.get("MsgId").and_then(string_value)),
            };
        }

        Ok(last_response)
    }
}

pub fn decode_webhook_event(
    payload: &Value,
    platform_account: &str,
    mention_names: &[&str],
) -> Option<CanonicalEvent> {
    let event = unwrap_event(payload);
    if !is_text_event(&event) {
        return None;
    }
    let message_id = extract_message_id(&event)?;
    let content = extract_content(&event);
    let sender = extract_sender(&event);
    let normalized = strip_leading_mention(&content, mention_names);
    let mentions = if normalized != content {
        vec![platform_account.to_string()]
    } else {
        Vec::new()
    };
    let conversation = if let Some(room_id) = extract_group_id(&event) {
        format!("wechatpadpro:group:{room_id}")
    } else {
        format!("wechatpadpro:private:{sender}")
    };

    Some(CanonicalEvent {
        platform: "wechatpadpro".into(),
        platform_account: platform_account.to_string(),
        conversation,
        actor: format!("wechatpadpro:user:{sender}"),
        event_id: format!("wechatpadpro:event:{message_id}"),
        timestamp_epoch_secs: extract_timestamp(&event),
        kind: EventKind::Message {
            message: CanonicalMessage {
                message_id,
                reply_to: extract_reply_to(&event),
                parts: vec![MessagePart::Text {
                    text: normalized.clone(),
                }],
                mentions,
                native_metadata: event.clone(),
            },
        },
        raw_native_payload: payload.clone(),
    })
}

pub fn compile_text_reply(payload: &Value, text: &str) -> Value {
    let event = unwrap_event(payload);
    let is_group = is_group_event(&event);
    let target = if is_group {
        extract_group_id(&event).unwrap_or_default()
    } else {
        extract_sender(&event)
    };
    let sender_id = extract_sender(&event);
    let sender_name = extract_sender_name(&event);

    build_text_reply_payload(&target, text, is_group, Some(&sender_id), Some(&sender_name))
}

fn build_text_reply_from_context(context: &DeliveryContext, text: &str) -> Value {
    let is_group = context.conversation_id.split(':').nth(1) == Some("group");
    let target = context
        .conversation_id
        .splitn(3, ':')
        .nth(2)
        .unwrap_or_default()
        .to_string();

    let sender_id = context
        .target_actor_id
        .as_deref()
        .map(normalize_wechat_target_id);
    let sender_name = context.target_display_name.as_deref();

    build_text_reply_payload(&target, text, is_group, sender_id, sender_name)
}

fn build_text_reply_payload(
    target: &str,
    text: &str,
    is_group: bool,
    sender_id: Option<&str>,
    sender_name: Option<&str>,
) -> Value {
    let mut msg_item = json!({
        "MsgType": 1,
        "ToUserName": target,
        "TextContent": text,
    });

    if is_group {
        if let Some(sender_name) = sender_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            msg_item["TextContent"] = Value::String(format!("@{sender_name} {text}"));
        } else if let Some(sender_id) = sender_id.filter(|value| !value.is_empty()) {
            msg_item["TextContent"] = Value::String(format!("@{sender_id} {text}"));
        }
        if let Some(sender_id) = sender_id.filter(|value| !value.is_empty()) {
            msg_item["AtWxIDList"] = Value::Array(vec![Value::String(sender_id.to_string())]);
        }
    }

    json!({
        "MsgItem": [msg_item],
    })
}

fn compile_message_text(message: &crate::protocol::OutboundMessage) -> Result<String, String> {
    let mut output = String::new();
    for part in &message.parts {
        match part {
            MessagePart::Text { text } => output.push_str(text),
            MessagePart::Mention { display, .. } => output.push_str(display),
            unsupported => {
                return Err(format!(
                    "unsupported outbound WeChatPadPro part: {unsupported:?}"
                ));
            }
        }
    }
    let text = output.trim().to_string();
    if text.is_empty() {
        return Err("empty outbound WeChatPadPro text message".to_string());
    }
    Ok(text)
}

fn unwrap_event(payload: &Value) -> Value {
    if let Some(message) = payload.get("message").and_then(Value::as_object) {
        return Value::Object(merge_top_level_metadata(message, payload));
    }

    if let Some(data) = payload.get("data").and_then(Value::as_object) {
        if let Some(message) = data.get("message").and_then(Value::as_object) {
            return Value::Object(merge_top_level_metadata(message, payload));
        }
        return Value::Object(data.clone());
    }

    payload.clone()
}

fn merge_top_level_metadata(message: &Map<String, Value>, payload: &Value) -> Map<String, Value> {
    let mut merged = message.clone();
    for key in ["event_type", "type", "uuid", "timestamp", "signature"] {
        if merged.contains_key(key) {
            continue;
        }
        if let Some(value) = payload.get(key) {
            merged.insert(key.to_string(), value.clone());
        }
    }
    merged
}

fn extract_content(event: &Value) -> String {
    let raw = extract_raw_content(event);
    let (_sender, content) = parse_transport_prefixed_content(&raw);
    content
}

fn extract_raw_content(event: &Value) -> String {
    event
        .get("content")
        .or_else(|| event.get("Content"))
        .or_else(|| event.get("text"))
        .or_else(|| event.get("TextContent"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn parse_transport_prefixed_content(raw: &str) -> (Option<String>, String) {
    let trimmed = raw.trim();
    let Some((sender, body)) = trimmed.split_once(":\n") else {
        return (None, trimmed.to_string());
    };

    if sender.is_empty()
        || !sender
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '@' || ch == '-')
    {
        return (None, trimmed.to_string());
    }

    (Some(sender.trim().to_string()), body.trim().to_string())
}

fn extract_sender(event: &Value) -> String {
    for key in ["senderWxid", "senderWxId", "senderId"] {
        if let Some(sender) = event
            .get(key)
            .and_then(string_value)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return sender;
        }
    }

    let (prefixed_sender, _content) = parse_transport_prefixed_content(&extract_raw_content(event));
    if let Some(prefixed_sender) = prefixed_sender {
        return prefixed_sender;
    }

    event
        .get("fromUserName")
        .or_else(|| event.get("fromUsername"))
        .or_else(|| event.get("FromUserName"))
        .and_then(string_value)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.ends_with("@chatroom") && !value.is_empty())
        .unwrap_or_default()
}

fn extract_group_id(event: &Value) -> Option<String> {
    for key in [
        "roomId",
        "chatroomId",
        "chatRoomName",
        "fromChatRoom",
        "fromGroup",
    ] {
        if let Some(group_id) = event
            .get(key)
            .and_then(string_value)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return Some(group_id);
        }
    }

    for key in [
        "fromUserName",
        "fromUsername",
        "FromUserName",
        "toUserName",
        "toUsername",
        "ToUserName",
    ] {
        if let Some(group_id) = event
            .get(key)
            .and_then(string_value)
            .map(|value| value.trim().to_string())
            .filter(|value| value.ends_with("@chatroom"))
        {
            return Some(group_id);
        }
    }

    None
}

fn is_group_event(event: &Value) -> bool {
    match event.get("isGroup") {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_i64().unwrap_or_default() != 0,
        Some(Value::String(value)) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Some(_) => false,
        None => extract_group_id(event).is_some(),
    }
}

fn extract_sender_name(event: &Value) -> String {
    for key in [
        "senderNickName",
        "senderName",
        "fromNickname",
        "fromUserNickName",
        "senderNick",
    ] {
        if let Some(name) = event
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return name.to_string();
        }
    }

    String::new()
}

fn is_text_event(event: &Value) -> bool {
    if let Some(msg_type) = event
        .get("msgType")
        .or_else(|| event.get("MsgType"))
        .and_then(string_value)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        return matches!(msg_type.as_str(), "1" | "text");
    }

    !extract_content(event).is_empty()
}

fn extract_message_id(event: &Value) -> Option<String> {
    for key in ["msgId", "MsgId", "newMsgId", "newmsgId"] {
        if let Some(message_id) = event
            .get(key)
            .and_then(string_value)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return Some(message_id);
        }
    }

    None
}

fn extract_reply_to(event: &Value) -> Option<String> {
    for key in ["replyTo", "quoteMsgId", "replyMsgId", "reply_to_msg_id"] {
        if let Some(reply_to) = event
            .get(key)
            .and_then(string_value)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return Some(reply_to);
        }
    }

    None
}

fn extract_timestamp(event: &Value) -> i64 {
    for key in ["createTime", "CreateTime", "timestamp", "time"] {
        if let Some(value) = event.get(key).and_then(integer_value) {
            return value;
        }
    }

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn strip_leading_mention(content: &str, mention_names: &[&str]) -> String {
    let trimmed = content.trim();
    if !trimmed.starts_with('@') || mention_names.is_empty() {
        return trimmed.to_string();
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or_default();
    let mention = first.trim_start_matches('@');
    if !mention_names.iter().any(|name| *name == mention) {
        return trimmed.to_string();
    }

    parts.next().unwrap_or_default().trim().to_string()
}

fn normalize_wechat_target_id(value: &str) -> &str {
    value.strip_prefix("wechatpadpro:user:").unwrap_or(value)
}

fn internal_error(message: &str) -> ErrorResponse {
    ErrorResponse {
        status: "error".into(),
        error_code: "internal_error".into(),
        message: message.into(),
        timed_out: false,
    }
}
