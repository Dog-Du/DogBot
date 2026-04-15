use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
};
use reqwest::Client;
use serde_json::Value;

use crate::api_proxy_config::{ApiProxySettings, ProviderConfig};

#[derive(Clone)]
struct AppState {
    settings: ApiProxySettings,
    client: Client,
}

pub fn build_test_app(settings: ApiProxySettings) -> Router {
    build_router(settings, Client::new())
}

pub fn build_app(settings: ApiProxySettings) -> Router {
    build_router(settings, Client::new())
}

fn build_router(settings: ApiProxySettings, client: Client) -> Router {
    let state = AppState { settings, client };
    Router::new()
        .route("/", any(proxy_request))
        .route("/{*path}", any(proxy_request))
        .with_state(state)
}

async fn proxy_request(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !is_authorized(&headers, &state.settings.local_auth_token) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let provider = &state.settings.upstream;

    let url = format!(
        "{}{}",
        provider.base_url.trim_end_matches('/'),
        uri.path_and_query().map(|value| value.as_str()).unwrap_or("/")
    );

    let forwarded_body = match rewrite_body_if_needed(provider, uri.path(), &headers, body).await {
        Ok(body) => body,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };

    let upstream_headers = build_upstream_headers(provider, &headers);
    let mut request = state.client.request(method, url).headers(upstream_headers);
    request = request.body(forwarded_body);

    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("upstream request failed: {error}"),
            )
                .into_response();
        }
    };

    let status = response.status();
    let mut out = Response::builder().status(status);
    if let Some(content_type) = response.headers().get(reqwest::header::CONTENT_TYPE) {
        out = out.header(reqwest::header::CONTENT_TYPE, content_type);
    }
    let body_bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("upstream response read failed: {error}"),
            )
                .into_response();
        }
    };

    out.body(Body::from(body_bytes))
        .unwrap_or_else(|_| (StatusCode::BAD_GATEWAY, "failed to build response").into_response())
}

fn is_authorized(headers: &HeaderMap, expected_token: &str) -> bool {
    extract_inbound_token(headers)
        .map(|token| token == expected_token)
        .unwrap_or(false)
}

fn extract_inbound_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-api-key").and_then(|value| value.to_str().ok()) {
        return Some(value.to_string());
    }

    let authorization = headers.get(reqwest::header::AUTHORIZATION)?;
    let authorization = authorization.to_str().ok()?;
    if let Some(value) = authorization.strip_prefix("Bearer ") {
        return Some(value.to_string());
    }
    Some(authorization.to_string())
}

fn build_upstream_headers(provider: &ProviderConfig, inbound: &HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in inbound {
        if matches!(
            name.as_str(),
            "host" | "content-length" | "authorization" | "x-api-key" | "connection"
        ) {
            continue;
        }
        headers.insert(name.clone(), value.clone());
    }

    let auth_header = HeaderName::from_bytes(provider.upstream_auth_header.as_bytes())
        .unwrap_or_else(|_| HeaderName::from_static("x-api-key"));
    let auth_value = match provider.upstream_auth_scheme.as_deref() {
        Some(scheme) if !scheme.is_empty() => format!("{scheme} {}", provider.upstream_token),
        _ => provider.upstream_token.clone(),
    };
    headers.insert(
        auth_header,
        HeaderValue::from_str(&auth_value).expect("valid upstream auth header"),
    );
    headers
}

async fn rewrite_body_if_needed(
    provider: &ProviderConfig,
    path: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Vec<u8>, &'static str> {
    if path != "/v1/messages" || provider.model.is_none() || !is_json(headers) {
        return Ok(body.to_vec());
    }

    let mut payload: Value =
        serde_json::from_slice(&body).map_err(|_| "invalid JSON request body")?;
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "model".to_string(),
            Value::String(provider.model.clone().expect("checked model")),
        );
    }

    serde_json::to_vec(&payload).map_err(|_| "failed to serialize JSON request body")
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.starts_with("application/json"))
        .unwrap_or(false)
}
