use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use axum::{
    body::Bytes,
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bollard::errors::Error as BollardError;
use serde_json::json;
use tokio::sync::Semaphore;

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
    run_slots: Arc<Semaphore>,
}

pub fn build_test_app(runner: Arc<dyn Runner>) -> Router {
    let settings = Settings::from_env_map(HashMap::new()).expect("default settings");
    router(AppState {
        settings,
        runner,
        run_slots: Arc::new(Semaphore::new(1)),
    })
}

pub fn build_app(settings: Settings) -> Result<Router, BollardError> {
    let runtime = DockerRuntime::connect()?;
    let runner = Arc::new(DockerRunner::new(runtime, settings.container_name.clone()));
    Ok(router(AppState {
        settings,
        runner,
        run_slots: Arc::new(Semaphore::new(1)),
    }))
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

async fn run(State(state): State<AppState>, body: Bytes) -> Response {
    let request: RunRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: "error".into(),
                    error_code: "invalid_json".into(),
                    message: err.to_string(),
                    timed_out: false,
                }),
            )
                .into_response();
        }
    };

    let _permit = match state.run_slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorResponse {
                    status: "error".into(),
                    error_code: "busy".into(),
                    message: "another run is already in progress".into(),
                    timed_out: false,
                }),
            )
                .into_response();
        }
    };

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
