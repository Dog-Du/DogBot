use serde_json::Value;

use crate::protocol::{CanonicalEvent, CanonicalMessage, EventKind, MessagePart, OutboundMessage};

pub fn decode_napcat_event(payload: &Value, platform_account: &str) -> Option<CanonicalEvent> {
    if payload.get("post_type").and_then(Value::as_str) != Some("message") {
        return None;
    }

    let message_type = payload.get("message_type").and_then(Value::as_str)?;
    let user_id = value_as_string(payload.get("user_id")?)?;
    let message_id = value_as_string(payload.get("message_id")?)?;
    let timestamp_epoch_secs = payload
        .get("time")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let bot_uin = platform_account
        .strip_prefix("qq:bot_uin:")
        .unwrap_or_default();

    let raw_message = payload
        .get("raw_message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut parts = parse_message_parts(payload.get("message"), platform_account, bot_uin);
    if parts.is_empty()
        || !parts
            .iter()
            .any(|part| matches!(part, MessagePart::Mention { .. }))
    {
        let fallback_parts = parse_raw_message_parts(raw_message, platform_account, bot_uin);
        if !fallback_parts.is_empty() {
            parts = fallback_parts;
        }
    }

    let mentions = parts
        .iter()
        .filter_map(|part| match part {
            MessagePart::Mention { actor_id, .. } => Some(actor_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    let conversation = match message_type {
        "group" => format!("qq:group:{}", value_as_string(payload.get("group_id")?)?),
        "private" => format!("qq:private:{user_id}"),
        _ => return None,
    };

    Some(CanonicalEvent {
        platform: "qq".into(),
        platform_account: platform_account.to_string(),
        conversation,
        actor: format!("qq:user:{user_id}"),
        event_id: format!("qq:event:{message_id}"),
        timestamp_epoch_secs,
        kind: EventKind::Message {
            message: CanonicalMessage {
                message_id,
                reply_to: parse_reply_to(payload.get("message"), raw_message),
                parts,
                mentions,
                native_metadata: payload.clone(),
            },
        },
        raw_native_payload: payload.clone(),
    })
}

pub fn compile_outbound_message(
    message: &OutboundMessage,
    mention_user_id: Option<&str>,
) -> Result<String, String> {
    let mut output = String::new();

    if let Some(reply_to) = message.reply_to.as_deref() {
        let reply_to = normalize_qq_target_id(reply_to)?;
        output.push_str(&format!("[CQ:reply,id={reply_to}]"));
    }

    if let Some(mention_user_id) = mention_user_id {
        let mention_user_id = normalize_qq_target_id(mention_user_id)?;
        output.push_str(&format!("[CQ:at,qq={mention_user_id}] "));
    }

    for part in &message.parts {
        match part {
            MessagePart::Text { text } => output.push_str(&escape_cq_text(text)),
            unsupported => {
                return Err(format!("unsupported outbound QQ part: {unsupported:?}"));
            }
        }
    }

    Ok(output)
}

fn parse_message_parts(
    message: Option<&Value>,
    platform_account: &str,
    bot_uin: &str,
) -> Vec<MessagePart> {
    let mut parts = Vec::new();
    let Some(segments) = message.and_then(Value::as_array) else {
        return parts;
    };

    for segment in segments {
        let Some(segment_type) = segment.get("type").and_then(Value::as_str) else {
            continue;
        };
        let data = segment.get("data").unwrap_or(&Value::Null);

        match segment_type {
            "text" => {
                if let Some(text) = data.get("text").and_then(Value::as_str) {
                    parts.push(MessagePart::Text {
                        text: text.to_string(),
                    });
                }
            }
            "at" => {
                let Some(qq) = data.get("qq").and_then(value_as_string) else {
                    continue;
                };
                let actor_id = if qq == bot_uin {
                    platform_account.to_string()
                } else {
                    format!("qq:user:{qq}")
                };
                parts.push(MessagePart::Mention {
                    actor_id,
                    display: format!("@{qq}"),
                });
            }
            _ => {}
        }
    }

    parts
}

fn parse_raw_message_parts(
    raw_message: &str,
    platform_account: &str,
    bot_uin: &str,
) -> Vec<MessagePart> {
    let mut parts = Vec::new();
    let mut remaining = raw_message;

    while let Some(index) = remaining.find("[CQ:at,qq=") {
        let before = &remaining[..index];
        if !before.is_empty() {
            parts.push(MessagePart::Text {
                text: before.to_string(),
            });
        }

        let after = &remaining[index + "[CQ:at,qq=".len()..];
        let Some(end) = after.find(']') else {
            break;
        };

        let qq = &after[..end];
        let actor_id = if qq == bot_uin {
            platform_account.to_string()
        } else {
            format!("qq:user:{qq}")
        };
        parts.push(MessagePart::Mention {
            actor_id,
            display: format!("@{qq}"),
        });
        remaining = &after[end + 1..];
    }

    let text = strip_reply_tokens(remaining);
    if !text.is_empty() {
        parts.push(MessagePart::Text {
            text: text.to_string(),
        });
    }

    parts
}

fn parse_reply_to(message: Option<&Value>, raw_message: &str) -> Option<String> {
    if let Some(segments) = message.and_then(Value::as_array) {
        for segment in segments {
            if segment.get("type").and_then(Value::as_str) != Some("reply") {
                continue;
            }

            let Some(data) = segment.get("data") else {
                continue;
            };
            if let Some(reply_id) = data.get("id").and_then(value_as_string) {
                return Some(reply_id);
            }
        }
    }

    extract_reply_from_raw_message(raw_message)
}

fn normalize_qq_target_id(value: &str) -> Result<&str, String> {
    let normalized = value
        .strip_prefix("qq:user:")
        .or_else(|| value.strip_prefix("qq:bot_uin:"))
        .unwrap_or(value);
    if normalized.is_empty() || !normalized.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(format!("invalid QQ target id: {value}"));
    }
    Ok(normalized)
}

fn escape_cq_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
}

fn strip_reply_tokens(value: &str) -> String {
    let mut remaining = value;
    let mut output = String::new();

    while let Some(index) = remaining.find("[CQ:reply,id=") {
        output.push_str(&remaining[..index]);
        let after = &remaining[index + "[CQ:reply,id=".len()..];
        let Some(end) = after.find(']') else {
            break;
        };
        remaining = &after[end + 1..];
    }

    output.push_str(remaining);
    output
}

fn extract_reply_from_raw_message(raw_message: &str) -> Option<String> {
    let index = raw_message.find("[CQ:reply,id=")?;
    let after = &raw_message[index + "[CQ:reply,id=".len()..];
    let end = after.find(']')?;
    Some(after[..end].to_string())
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}
