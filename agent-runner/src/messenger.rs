use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Value, json};

use crate::{
    config::Settings,
    models::{ErrorResponse, MessageRequest, MessageResponse},
    session_store::SessionRecord,
};

#[async_trait]
pub trait MessageDelivery: Send + Sync {
    async fn send(
        &self,
        request: MessageRequest,
        session: SessionRecord,
    ) -> Result<MessageResponse, ErrorResponse>;
}

#[derive(Clone)]
pub struct NapCatMessenger {
    client: reqwest::Client,
    base_url: String,
}

impl NapCatMessenger {
    pub fn from_settings(settings: &Settings) -> Result<Self, ErrorResponse> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(token) = settings.napcat_access_token.as_deref() {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| internal_error("invalid NAPCAT_ACCESS_TOKEN header value"))?;
            headers.insert(AUTHORIZATION, value);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|err| internal_error(&format!("failed to build NapCat client: {err}")))?;

        Ok(Self {
            client,
            base_url: settings
                .napcat_api_base_url
                .trim_end_matches('/')
                .to_string(),
        })
    }

    fn build_payload(
        &self,
        request: &MessageRequest,
        session: &SessionRecord,
    ) -> Result<(&'static str, Value), ErrorResponse> {
        let message = format_message(request);
        let Some((_, scope, target_id)) = parse_conversation_id(&session.conversation_id) else {
            return Err(internal_error("invalid conversation_id in session store"));
        };

        match (session.platform.as_str(), scope) {
            ("qq", "private") => Ok((
                "/send_private_msg",
                json!({
                    "user_id": target_id,
                    "message": message,
                    "auto_escape": false
                }),
            )),
            ("qq", "group") => Ok((
                "/send_group_msg",
                json!({
                    "group_id": target_id,
                    "message": message,
                    "auto_escape": false
                }),
            )),
            _ => Err(ErrorResponse {
                status: "error".into(),
                error_code: "unsupported_platform".into(),
                message: format!(
                    "unsupported platform or conversation scope: {} {}",
                    session.platform, session.conversation_id
                ),
                timed_out: false,
            }),
        }
    }
}

#[async_trait]
impl MessageDelivery for NapCatMessenger {
    async fn send(
        &self,
        request: MessageRequest,
        session: SessionRecord,
    ) -> Result<MessageResponse, ErrorResponse> {
        let (path, payload) = self.build_payload(&request, &session)?;
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

        let message_id = body
            .get("data")
            .and_then(|data| data.get("message_id"))
            .map(json_to_string);

        Ok(MessageResponse {
            status: "ok".into(),
            message_id,
        })
    }
}

fn parse_conversation_id(value: &str) -> Option<(&str, &str, &str)> {
    let mut parts = value.splitn(3, ':');
    let platform = parts.next()?;
    let scope = parts.next()?;
    let target_id = parts.next()?;
    Some((platform, scope, target_id))
}

fn format_message(request: &MessageRequest) -> String {
    let mut output = String::new();

    if let Some(reply_to) = request.reply_to_message_id.as_deref() {
        output.push_str(&format!("[CQ:reply,id={reply_to}]"));
    }

    if let Some(mention) = request.mention_user_id.as_deref() {
        output.push_str(&format!("[CQ:at,qq={mention}] "));
    }

    output.push_str(&request.text);
    output
}

fn json_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn internal_error(message: &str) -> ErrorResponse {
    ErrorResponse {
        status: "error".into(),
        error_code: "internal_error".into(),
        message: message.into(),
        timed_out: false,
    }
}
