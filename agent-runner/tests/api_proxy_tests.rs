use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agent_runner::api_proxy::build_test_app;
use agent_runner::api_proxy_config::{ApiProxySettings, ProviderConfig, ProviderKind};
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
fn settings_require_only_local_proxy_token_for_claude_container() {
    let settings = ApiProxySettings::from_env_map(HashMap::from([
        ("API_PROXY_ACTIVE_PROVIDER".into(), "packy".into()),
        ("API_PROXY_PACKY_BASE_URL".into(), "https://www.packyapi.com".into()),
        ("API_PROXY_PACKY_UPSTREAM_TOKEN".into(), "packy-secret".into()),
    ]))
    .unwrap();

    assert_eq!(settings.local_auth_token, "local-proxy-token");
    assert_eq!(settings.active_provider, ProviderKind::Packy);
    assert_eq!(
        settings.active_provider_config().unwrap().upstream_token,
        "packy-secret".to_string()
    );
}

#[tokio::test]
async fn proxy_rewrites_model_and_auth_for_active_provider() {
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
        active_provider: ProviderKind::Packy,
        packy: Some(ProviderConfig {
            base_url: format!("http://{upstream_addr}"),
            upstream_token: "packy-secret".into(),
            upstream_auth_header: "x-api-key".into(),
            upstream_auth_scheme: None,
            model: Some("packy-model".into()),
        }),
        glm_official: None,
        minimax_official: None,
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
        Some("packy-secret")
    );
    assert_eq!(
        forwarded.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(forwarded.body["model"], "packy-model");
}

#[tokio::test]
async fn proxy_rejects_invalid_local_auth_token() {
    let settings = ApiProxySettings::from_env_map(HashMap::from([
        ("API_PROXY_ACTIVE_PROVIDER".into(), "packy".into()),
        ("API_PROXY_PACKY_BASE_URL".into(), "https://www.packyapi.com".into()),
        ("API_PROXY_PACKY_UPSTREAM_TOKEN".into(), "packy-secret".into()),
    ]))
    .unwrap();
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
