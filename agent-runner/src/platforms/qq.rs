use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Value, json};

use crate::{
    config::Settings,
    dispatch::dispatch_plan,
    models::{ErrorResponse, MessageResponse},
    platforms::{
        DeliveryContext, IngressRoute, PlatformAdapter,
        common::{normalize_actor_id, string_value},
    },
    protocol::{
        AssetRef, CanonicalEvent, CanonicalMessage, EventKind, MessagePart, OutboundAction,
        OutboundMessage, OutboundPlan,
    },
};

const INGRESS_ROUTES: &[IngressRoute] = &[IngressRoute {
    path: "/v1/platforms/qq/napcat/ws",
    allow_head: false,
}];

pub struct QqPlatform {
    client: reqwest::Client,
    base_url: String,
    platform_account: String,
}

impl QqPlatform {
    pub fn from_settings(settings: &Settings) -> Result<Self, ErrorResponse> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(token) = settings.napcat_access_token.as_deref() {
            let value = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| {
                internal_error("invalid NAPCAT_ACCESS_TOKEN header value")
            })?;
            headers.insert(AUTHORIZATION, value);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|err| internal_error(&format!("failed to build NapCat client: {err}")))?;

        let platform_account = settings
            .platform_qq_account_id
            .clone()
            .or_else(|| {
                settings
                    .platform_qq_bot_id
                    .as_ref()
                    .map(|bot_id| format!("qq:bot_uin:{bot_id}"))
            })
            .unwrap_or_else(|| "qq:bot_uin:123".to_string());

        Ok(Self {
            client,
            base_url: settings.napcat_api_base_url.trim_end_matches('/').to_string(),
            platform_account,
        })
    }

    async fn send_encoded_message(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Result<MessageResponse, ErrorResponse> {
        let Some((_, scope, target_id)) = parse_conversation_id(conversation_id) else {
            return Err(internal_error("invalid conversation_id in session store"));
        };
        let numeric_target_id = target_id.parse::<i64>().ok();

        let (path, payload) = match scope {
            "private" | "FriendMessage" => (
                "/send_private_msg",
                json!({
                    "user_id": numeric_target_id.unwrap_or_default(),
                    "message": message,
                    "auto_escape": false,
                }),
            ),
            "group" | "GroupMessage" => (
                "/send_group_msg",
                json!({
                    "group_id": numeric_target_id.unwrap_or_default(),
                    "message": message,
                    "auto_escape": false,
                }),
            ),
            _ => {
                return Err(ErrorResponse {
                    status: "error".into(),
                    error_code: "unsupported_platform".into(),
                    message: format!("unsupported QQ conversation scope: {conversation_id}"),
                    timed_out: false,
                });
            }
        };

        let response = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .json(&payload)
            .send()
            .await
            .map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "delivery_unavailable".into(),
                message: format!("failed to reach NapCat API: {err}"),
                timed_out: false,
            })?;

        let status = response.status();
        let body: Value = response.json().await.map_err(|err| ErrorResponse {
            status: "error".into(),
            error_code: "delivery_invalid_response".into(),
            message: format!("NapCat API returned invalid JSON: {err}"),
            timed_out: false,
        })?;

        if !status.is_success() {
            return Err(ErrorResponse {
                status: "error".into(),
                error_code: "delivery_failed".into(),
                message: format!("NapCat API returned {status}: {body}"),
                timed_out: false,
            });
        }

        Ok(MessageResponse {
            status: "ok".into(),
            message_id: body
                .get("data")
                .and_then(|data| data.get("message_id"))
                .and_then(string_value),
        })
    }

    async fn send_reaction_add(
        &self,
        action: &crate::protocol::ReactionAction,
    ) -> Result<(), ErrorResponse> {
        let message_id = normalize_qq_target_id(&action.target_message_id)
            .map_err(|message| ErrorResponse {
                status: "error".into(),
                error_code: "delivery_invalid_plan".into(),
                message,
                timed_out: false,
            })?
            .parse::<i64>()
            .map_err(|_| internal_error("invalid QQ reaction target id"))?;
        let payload = json!({
            "message_id": message_id,
            "emoji_id": action.emoji,
        });

        let response = self
            .client
            .post(format!("{}/set_msg_emoji_like", self.base_url))
            .json(&payload)
            .send()
            .await
            .map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "delivery_unavailable".into(),
                message: format!("failed to reach NapCat API: {err}"),
                timed_out: false,
            })?;

        let status = response.status();
        let body: Value = response.json().await.map_err(|err| ErrorResponse {
            status: "error".into(),
            error_code: "delivery_invalid_response".into(),
            message: format!("NapCat API returned invalid JSON: {err}"),
            timed_out: false,
        })?;

        if !status.is_success() {
            return Err(ErrorResponse {
                status: "error".into(),
                error_code: "delivery_failed".into(),
                message: format!("NapCat API returned {status}: {body}"),
                timed_out: false,
            });
        }

        Ok(())
    }
}

#[async_trait]
impl PlatformAdapter for QqPlatform {
    fn platform_id(&self) -> &'static str {
        "qq"
    }

    fn ingress_routes(&self) -> &'static [IngressRoute] {
        INGRESS_ROUTES
    }

    fn decode_event(&self, payload: &Value) -> Option<CanonicalEvent> {
        decode_napcat_event(payload, &self.platform_account)
    }

    async fn send_plan(
        &self,
        context: &DeliveryContext,
        plan: &OutboundPlan,
    ) -> Result<MessageResponse, ErrorResponse> {
        dispatch_plan(plan).map_err(|message| ErrorResponse {
            status: "error".into(),
            error_code: "delivery_invalid_plan".into(),
            message,
            timed_out: false,
        })?;

        let mut last_response = MessageResponse {
            status: "ok".into(),
            message_id: None,
        };

        for action in &plan.actions {
            match action {
                OutboundAction::ReactionAdd(action) => self.send_reaction_add(action).await?,
                OutboundAction::ReactionRemove(_) => {}
            }
        }

        for (index, message) in plan.messages.iter().enumerate() {
            let mention_user_id = if index == 0 {
                context.target_actor_id.as_deref()
            } else {
                None
            };
            let message = if index == 0 && message.reply_to.is_none() {
                let mut message = message.clone();
                message.reply_to = context.reply_to_message_id.clone();
                message
            } else {
                message.clone()
            };
            let encoded = compile_outbound_message(&message, mention_user_id).map_err(|err| {
                ErrorResponse {
                    status: "error".into(),
                    error_code: "delivery_invalid_plan".into(),
                    message: err,
                    timed_out: false,
                }
            })?;

            last_response = self
                .send_encoded_message(&context.conversation_id, &encoded)
                .await?;
        }

        Ok(last_response)
    }
}

pub fn decode_napcat_event(payload: &Value, platform_account: &str) -> Option<CanonicalEvent> {
    if payload.get("post_type").and_then(Value::as_str) != Some("message") {
        return None;
    }

    let message_type = payload.get("message_type").and_then(Value::as_str)?;
    let user_id = string_value(payload.get("user_id")?)?;
    let message_id = string_value(payload.get("message_id")?)?;
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
        "group" => format!("qq:group:{}", string_value(payload.get("group_id")?)?),
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
            MessagePart::Mention { actor_id, .. } => {
                let mention_target = normalize_qq_target_id(actor_id)?;
                output.push_str(&format!("[CQ:at,qq={mention_target}]"));
            }
            MessagePart::Image { asset } => {
                output.push_str(&format!("[CQ:image,file={}]", qq_asset_reference(asset)));
            }
            MessagePart::File { asset } => {
                output.push_str(&format!("[CQ:file,file={}]", qq_asset_reference(asset)));
            }
            MessagePart::Voice { asset } => {
                output.push_str(&format!("[CQ:record,file={}]", qq_asset_reference(asset)));
            }
            MessagePart::Video { asset } => {
                output.push_str(&format!("[CQ:video,file={}]", qq_asset_reference(asset)));
            }
            MessagePart::Sticker { asset } => {
                output.push_str(&format!("[CQ:image,file={}]", qq_asset_reference(asset)));
            }
            MessagePart::Quote {
                target_message_id, ..
            } => {
                let target = normalize_qq_target_id(target_message_id)?;
                output.push_str(&format!("[CQ:reply,id={target}]"));
            }
        }
    }

    Ok(output)
}

fn qq_asset_reference(asset: &AssetRef) -> String {
    match &asset.source {
        crate::protocol::AssetSource::WorkspacePath(path) => format!("file://{path}"),
        crate::protocol::AssetSource::ManagedStore(value)
        | crate::protocol::AssetSource::ExternalUrl(value)
        | crate::protocol::AssetSource::PlatformNativeHandle(value)
        | crate::protocol::AssetSource::BridgeHandle(value) => value.clone(),
    }
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
                let Some(qq) = data.get("qq").and_then(string_value) else {
                    continue;
                };
                let actor_id = if qq == bot_uin {
                    platform_account.to_string()
                } else {
                    normalize_actor_id(&qq, "qq:user:")
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
            normalize_actor_id(qq, "qq:user:")
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
            if let Some(reply_id) = data.get("id").and_then(string_value) {
                return Some(reply_id);
            }
        }
    }

    extract_reply_from_raw_message(raw_message)
}

fn parse_conversation_id(value: &str) -> Option<(&str, &str, &str)> {
    let mut parts = value.splitn(3, ':');
    let platform = parts.next()?;
    let scope = parts.next()?;
    let target_id = parts.next()?;
    Some((platform, scope, target_id))
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

fn internal_error(message: &str) -> ErrorResponse {
    ErrorResponse {
        status: "error".into(),
        error_code: "internal_error".into(),
        message: message.into(),
        timed_out: false,
    }
}
