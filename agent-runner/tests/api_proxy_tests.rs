use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agent_runner::api_proxy::build_test_app;
use agent_runner::api_proxy_config::{ApiProxySettings, ProviderConfig};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Method, Request, StatusCode},
    response::IntoResponse,
    routing::any,
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[test]
fn proxy_settings_are_optional_when_upstream_is_unset() {
    let settings = ApiProxySettings::from_env_map_optional(HashMap::new()).unwrap();
    assert!(settings.is_none());
}

#[test]
fn proxy_settings_load_when_explicitly_configured() {
    let settings = ApiProxySettings::from_env_map_optional(HashMap::from([
        (
            "API_PROXY_UPSTREAM_BASE_URL".into(),
            "https://upstream.example.com".into(),
        ),
        ("API_PROXY_UPSTREAM_TOKEN".into(), "upstream-secret".into()),
    ]))
    .unwrap();

    let settings = settings.expect("proxy settings");
    assert_eq!(settings.local_auth_token, "local-proxy-token");
    assert_eq!(
        settings.upstream.upstream_token,
        "upstream-secret".to_string()
    );
}

#[tokio::test]
async fn proxy_rewrites_model_and_auth_for_upstream() {
    let captured = Arc::new(Mutex::new(None::<CapturedRequest>));
    let upstream_app = upstream_router(captured.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, upstream_app).await.unwrap();
    });

    let settings = ApiProxySettings {
        bind_addr: "127.0.0.1:9000".into(),
        local_auth_token: "local-proxy-token".into(),
        upstream: ProviderConfig {
            base_url: format!("http://{upstream_addr}"),
            upstream_token: "upstream-secret".into(),
            upstream_auth_header: "x-api-key".into(),
            upstream_auth_scheme: None,
            model: Some("upstream-model".into()),
        },
    };

    let app = build_test_app(settings);
    let payload = json!({
        "model": "should-be-replaced",
        "messages": [{"role": "user", "content": "hello"}]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .header("x-api-key", "local-proxy-token")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let forwarded = captured.lock().unwrap().clone().expect("captured request");
    assert_eq!(forwarded.method, Method::POST);
    assert_eq!(forwarded.path, "/v1/messages");
    assert_eq!(
        forwarded.headers.get("x-api-key").map(String::as_str),
        Some("upstream-secret")
    );
    assert_eq!(
        forwarded.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(forwarded.body["model"], "upstream-model");
}

#[tokio::test]
async fn proxy_deduplicates_version_prefix_when_base_url_already_contains_it() {
    let captured = Arc::new(Mutex::new(None::<CapturedRequest>));
    let upstream_app = upstream_router(captured.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, upstream_app).await.unwrap();
    });

    let settings = ApiProxySettings {
        bind_addr: "127.0.0.1:9000".into(),
        local_auth_token: "local-proxy-token".into(),
        upstream: ProviderConfig {
            base_url: format!("http://{upstream_addr}/v1"),
            upstream_token: "upstream-secret".into(),
            upstream_auth_header: "authorization".into(),
            upstream_auth_scheme: Some("Bearer".into()),
            model: Some("upstream-model".into()),
        },
    };

    let app = build_test_app(settings);
    let payload = json!({
        "model": "should-be-replaced",
        "messages": [{"role": "user", "content": "hello"}]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .header("x-api-key", "local-proxy-token")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let forwarded = captured.lock().unwrap().clone().expect("captured request");
    assert_eq!(forwarded.path, "/v1/messages");
    assert_eq!(
        forwarded.headers.get("authorization").map(String::as_str),
        Some("Bearer upstream-secret")
    );
}

#[tokio::test]
async fn proxy_strips_anthropic_custom_tool_type_for_compatible_upstreams() {
    let captured = Arc::new(Mutex::new(None::<CapturedRequest>));
    let upstream_app = upstream_router(captured.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, upstream_app).await.unwrap();
    });

    let settings = ApiProxySettings {
        bind_addr: "127.0.0.1:9000".into(),
        local_auth_token: "local-proxy-token".into(),
        upstream: ProviderConfig {
            base_url: format!("http://{upstream_addr}"),
            upstream_token: "upstream-secret".into(),
            upstream_auth_header: "x-api-key".into(),
            upstream_auth_scheme: None,
            model: Some("upstream-model".into()),
        },
    };

    let app = build_test_app(settings);
    let payload = json!({
        "model": "should-be-replaced",
        "max_tokens": 32,
        "tools": [
            {
                "type": "custom",
                "name": "noop",
                "description": "No-op test tool",
                "input_schema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            },
            {
                "type": "web_search_20250305",
                "name": "web_search"
            }
        ],
        "messages": [{"role": "user", "content": "hello"}]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages?beta=true")
                .header("content-type", "application/json")
                .header("x-api-key", "local-proxy-token")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let forwarded = captured.lock().unwrap().clone().expect("captured request");
    assert_eq!(forwarded.body["tools"][0].get("type"), None);
    assert_eq!(forwarded.body["tools"][0]["name"], "noop");
    assert_eq!(
        forwarded.body["tools"][1]["type"],
        Value::String("web_search_20250305".into())
    );
}

#[tokio::test]
async fn proxy_rejects_invalid_local_auth_token() {
    let settings = ApiProxySettings::from_env_map_optional(HashMap::from([
        (
            "API_PROXY_UPSTREAM_BASE_URL".into(),
            "https://upstream.example.com".into(),
        ),
        ("API_PROXY_UPSTREAM_TOKEN".into(), "upstream-secret".into()),
    ]))
    .unwrap();
    let settings = settings.expect("proxy settings");
    let app = build_test_app(settings);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("content-type", "application/json")
                .header("x-api-key", "wrong-token")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[derive(Clone, Debug)]
struct CapturedRequest {
    method: Method,
    path: String,
    headers: HashMap<String, String>,
    body: Value,
}

fn upstream_router(captured: Arc<Mutex<Option<CapturedRequest>>>) -> Router {
    Router::new()
        .route(
            "/{*path}",
            any(
                move |State(captured): State<Arc<Mutex<Option<CapturedRequest>>>>,
                      req: Request<Body>| async move {
                    let (parts, body) = req.into_parts();
                    let body_bytes = to_bytes(body, usize::MAX).await.unwrap();
                    let json_body: Value =
                        serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
                    let headers = parts
                        .headers
                        .iter()
                        .map(|(name, value)| {
                            (
                                name.as_str().to_string(),
                                value.to_str().unwrap_or_default().to_string(),
                            )
                        })
                        .collect::<HashMap<_, _>>();

                    *captured.lock().unwrap() = Some(CapturedRequest {
                        method: parts.method,
                        path: parts.uri.path().to_string(),
                        headers,
                        body: json_body,
                    });

                    (StatusCode::OK, Body::from(r#"{"ok":true}"#)).into_response()
                },
            ),
        )
        .with_state(captured)
}
