use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use crate::protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart};

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

    let mut msg_item = json!({
        "MsgType": 1,
        "ToUserName": target,
        "TextContent": text,
    });

    if is_group {
        if !sender_name.is_empty() {
            msg_item["TextContent"] = Value::String(format!("@{} {}", sender_name, text));
        }
        if !sender_id.is_empty() {
            msg_item["AtWxIDList"] = Value::Array(vec![Value::String(sender_id)]);
        }
    }

    json!({
        "MsgItem": [msg_item],
    })
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

fn string_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn integer_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(value) => value.as_i64(),
        Value::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}
