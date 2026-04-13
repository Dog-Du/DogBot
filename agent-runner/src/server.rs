use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bollard::errors::Error as BollardError;
use serde_json::json;

use crate::{
    config::Settings,
    docker_client::DockerRuntime,
    exec::{DockerRunner, ExecutionBackend},
    models::{ErrorResponse, RunRequest, RunResponse, ValidatedRunRequest},
};

#[async_trait]
pub trait Runner: Send + Sync {
    async fn run(
        &self,
        request: RunRequest,
        validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse>;
}

#[async_trait]
impl Runner for DockerRunner {
    async fn run(
        &self,
        request: RunRequest,
        validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
        self.execute(request, validated).await
    }
}

#[derive(Clone)]
struct AppState {
    settings: Settings,
    runner: Arc<dyn Runner>,
}

pub fn build_test_app(runner: Arc<dyn Runner>) -> Router {
    let settings = Settings::from_env_map(HashMap::new()).expect("default settings");
    router(AppState { settings, runner })
}

pub fn build_app(settings: Settings) -> Result<Router, BollardError> {
    let runtime = DockerRuntime::connect()?;
    let runner = Arc::new(DockerRunner::new(runtime, settings.container_name.clone()));
    Ok(router(AppState { settings, runner }))
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/runs", post(run))
        .with_state(state)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn run(State(state): State<AppState>, Json(request): Json<RunRequest>) -> Response {
    let validated = match request.validate(
        state.settings.default_timeout_secs,
        state.settings.max_timeout_secs,
    ) {
        Ok(validated) => validated,
        Err(message) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: "error".into(),
                    error_code: "invalid_request".into(),
                    message,
                    timed_out: false,
                }),
            )
                .into_response();
        }
    };

    match state.runner.run(request, validated).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) if error.timed_out => (StatusCode::REQUEST_TIMEOUT, Json(error)).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response(),
    }
}
