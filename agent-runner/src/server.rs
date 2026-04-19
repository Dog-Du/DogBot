use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::{
    config::Settings,
    context::{
        context_pack::render_context_pack,
        identity::ActorId,
        scope::ScopeResolver,
    },
    docker_client::DockerRuntime,
    exec::{DockerRunner, ExecutionBackend},
    messenger::{MessageDelivery, NapCatMessenger},
    models::{
        ErrorResponse, MessageRequest, MessageResponse, RunRequest, RunResponse,
        ValidatedRunRequest,
    },
    session_store::SessionStore,
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
pub trait Messenger: Send + Sync {
    async fn send(
        &self,
        request: MessageRequest,
        session: crate::session_store::SessionRecord,
    ) -> Result<MessageResponse, ErrorResponse>;
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

#[async_trait]
impl Messenger for NapCatMessenger {
    async fn send(
        &self,
        request: MessageRequest,
        session: crate::session_store::SessionRecord,
    ) -> Result<MessageResponse, ErrorResponse> {
        MessageDelivery::send(self, request, session).await
    }
}

#[derive(Clone)]
struct AppState {
    settings: Settings,
    queue_tx: mpsc::Sender<QueuedRun>,
    session_store: SessionStore,
    messenger: Arc<dyn Messenger>,
}

struct QueuedRun {
    request: RunRequest,
    validated: ValidatedRunRequest,
    responder: oneshot::Sender<Result<RunResponse, ErrorResponse>>,
}

#[derive(Default)]
struct RateState {
    global: VecDeque<Instant>,
    by_user: HashMap<String, VecDeque<Instant>>,
    by_conversation: HashMap<String, VecDeque<Instant>>,
}

struct InMemoryRateLimiter {
    window: Duration,
    global_limit: usize,
    user_limit: usize,
    conversation_limit: usize,
    state: Mutex<RateState>,
}

impl InMemoryRateLimiter {
    fn new(settings: &Settings) -> Self {
        Self {
            window: Duration::from_secs(60),
            global_limit: settings.global_rate_limit_per_minute,
            user_limit: settings.user_rate_limit_per_minute,
            conversation_limit: settings.conversation_rate_limit_per_minute,
            state: Mutex::new(RateState::default()),
        }
    }

    async fn check_and_record(&self, request: &RunRequest) -> Result<(), ErrorResponse> {
        let now = Instant::now();
        let mut state = self.state.lock().await;
        prune_window(&mut state.global, now, self.window);
        state.by_user.retain(|_, events| {
            prune_window(events, now, self.window);
            !events.is_empty()
        });
        state.by_conversation.retain(|_, events| {
            prune_window(events, now, self.window);
            !events.is_empty()
        });

        if is_limit_exhausted(self.global_limit, state.global.len()) {
            return Err(rate_limit_error("global rate limit exceeded"));
        }

        {
            let user_events = state.by_user.entry(request.user_id.clone()).or_default();
            if is_limit_exhausted(self.user_limit, user_events.len()) {
                return Err(rate_limit_error("user rate limit exceeded"));
            }
        }

        {
            let conversation_events = state
                .by_conversation
                .entry(request.conversation_id.clone())
                .or_default();
            if is_limit_exhausted(self.conversation_limit, conversation_events.len()) {
                return Err(rate_limit_error("conversation rate limit exceeded"));
            }
        }

        state.global.push_back(now);
        state
            .by_user
            .entry(request.user_id.clone())
            .or_default()
            .push_back(now);
        state
            .by_conversation
            .entry(request.conversation_id.clone())
            .or_default()
            .push_back(now);
        Ok(())
    }
}

pub fn build_test_app(runner: Arc<dyn Runner>) -> Router {
    let mut settings = Settings::from_env_map(HashMap::new()).expect("default settings");
    let temp_state_dir = std::env::temp_dir().join(format!(
        "agent-runner-tests-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    settings.workspace_dir = temp_state_dir.join("workdir").display().to_string();
    settings.state_dir = temp_state_dir.join("state").display().to_string();
    settings.session_db_path = temp_state_dir.join("state/runner.db").display().to_string();
    build_test_app_with_settings(runner, settings)
}

pub fn build_test_app_with_settings(runner: Arc<dyn Runner>, settings: Settings) -> Router {
    let session_store = SessionStore::open(&settings.session_db_path).expect("session store");
    let queue_tx = spawn_dispatcher(settings.clone(), runner);
    let messenger =
        Arc::new(NapCatMessenger::from_settings(&settings).expect("default NapCat messenger"));
    router(AppState {
        settings,
        queue_tx,
        session_store,
        messenger,
    })
}

pub fn build_test_app_with_message_support(
    runner: Arc<dyn Runner>,
    messenger: Arc<dyn Messenger>,
    settings: Settings,
) -> Router {
    let session_store = SessionStore::open(&settings.session_db_path).expect("session store");
    let queue_tx = spawn_dispatcher(settings.clone(), runner);
    router(AppState {
        settings,
        queue_tx,
        session_store,
        messenger,
    })
}

pub fn build_app(settings: Settings) -> io::Result<Router> {
    let runtime = DockerRuntime::connect().map_err(|err| io::Error::other(err.to_string()))?;
    let runner = Arc::new(
        DockerRunner::new(runtime, settings.clone())
            .map_err(|err| io::Error::other(err.message))?,
    );
    let session_store = SessionStore::open(&settings.session_db_path)
        .map_err(|err| io::Error::other(err.to_string()))?;
    let messenger = Arc::new(
        NapCatMessenger::from_settings(&settings).map_err(|err| io::Error::other(err.message))?,
    );
    let queue_tx = spawn_dispatcher(settings.clone(), runner);
    Ok(router(AppState {
        settings,
        queue_tx,
        session_store,
        messenger,
    }))
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/runs", post(run))
        .route("/v1/messages", post(send_message))
        .with_state(state)
}

fn spawn_dispatcher(settings: Settings, runner: Arc<dyn Runner>) -> mpsc::Sender<QueuedRun> {
    let (queue_tx, queue_rx) = mpsc::channel::<QueuedRun>(settings.max_queue_depth);
    let rate_limiter = Arc::new(InMemoryRateLimiter::new(&settings));
    let queue_rx = Arc::new(Mutex::new(queue_rx));

    for _ in 0..settings.max_concurrent_runs.max(1) {
        let runner = Arc::clone(&runner);
        let rate_limiter = Arc::clone(&rate_limiter);
        let queue_rx = Arc::clone(&queue_rx);

        tokio::spawn(async move {
            loop {
                let item = {
                    let mut receiver = queue_rx.lock().await;
                    receiver.recv().await
                };

                let Some(item) = item else {
                    break;
                };

                if let Err(error) = rate_limiter.check_and_record(&item.request).await {
                    let _ = item.responder.send(Err(error));
                    continue;
                }

                let result = runner.run(item.request, item.validated).await;
                let _ = item.responder.send(result);
            }
        });
    }

    queue_tx
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn run(State(state): State<AppState>, body: Bytes) -> Response {
    let mut request: RunRequest = match serde_json::from_slice(&body) {
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

    let actor_id = match ActorId::new(request.user_id.clone()) {
        Some(actor_id) => actor_id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    status: "error".into(),
                    error_code: "invalid_request".into(),
                    message: "user_id must be non-empty".into(),
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

    let scopes = ScopeResolver::new().readable_scopes(
        &actor_id,
        &request.conversation_id,
        &request.platform_account_id,
    );
    request.prompt = format!("{}{}", render_context_pack(&scopes), request.prompt);

    let (responder, receiver) = oneshot::channel();
    match state.queue_tx.try_send(QueuedRun {
        request,
        validated,
        responder,
    }) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorResponse {
                    status: "error".into(),
                    error_code: "queue_full".into(),
                    message: "run queue is full".into(),
                    timed_out: false,
                }),
            )
                .into_response();
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(to_internal_error_message("run queue is closed")),
            )
                .into_response();
        }
    }

    match receiver.await {
        Ok(Ok(response)) => (StatusCode::OK, Json(response)).into_response(),
        Ok(Err(error)) if error.timed_out => {
            (StatusCode::REQUEST_TIMEOUT, Json(error)).into_response()
        }
        Ok(Err(error)) if error.error_code == "rate_limited" => {
            (StatusCode::TOO_MANY_REQUESTS, Json(error)).into_response()
        }
        Ok(Err(error)) if error.error_code == "session_conflict" => {
            (StatusCode::CONFLICT, Json(error)).into_response()
        }
        Ok(Err(error)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(to_internal_error_message("dispatcher dropped run result")),
        )
            .into_response(),
    }
}

async fn send_message(State(state): State<AppState>, body: Bytes) -> Response {
    let mut request: MessageRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(err) => {
            return error_response(StatusCode::BAD_REQUEST, "invalid_json", &err.to_string())
                .into_response();
        }
    };

    if let Err(message) = request.validate() {
        return error_response(StatusCode::BAD_REQUEST, "invalid_request", &message)
            .into_response();
    }

    let session = match state.session_store.get_session(&request.session_id) {
        Ok(Some(session)) => session,
        Ok(None) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "session_not_found",
                "session_id is unknown",
            )
            .into_response();
        }
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(to_internal_error_message(&format!(
                    "session store failure: {err}"
                ))),
            )
                .into_response();
        }
    };

    if request.mention_user_id.is_none() && is_group_conversation(&session.conversation_id) {
        request.mention_user_id = Some(session.user_id.clone());
    }

    match state.messenger.send(request, session).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) if error.error_code == "unsupported_platform" => {
            (StatusCode::BAD_REQUEST, Json(error)).into_response()
        }
        Err(error) if error.error_code.starts_with("delivery_") => {
            (StatusCode::BAD_GATEWAY, Json(error)).into_response()
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response(),
    }
}

fn is_limit_exhausted(limit: usize, current_len: usize) -> bool {
    limit > 0 && current_len >= limit
}

fn is_group_conversation(conversation_id: &str) -> bool {
    let mut parts = conversation_id.splitn(3, ':');
    let _platform = parts.next();
    matches!(parts.next(), Some("group" | "GroupMessage"))
}

fn prune_window(events: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while let Some(front) = events.front() {
        if now.duration_since(*front) >= window {
            events.pop_front();
        } else {
            break;
        }
    }
}

fn rate_limit_error(message: &str) -> ErrorResponse {
    ErrorResponse {
        status: "error".into(),
        error_code: "rate_limited".into(),
        message: message.into(),
        timed_out: false,
    }
}

fn to_internal_error_message(message: &str) -> ErrorResponse {
    ErrorResponse {
        status: "error".into(),
        error_code: "internal_error".into(),
        message: message.into(),
        timed_out: false,
    }
}

fn error_response(
    status: StatusCode,
    error_code: &str,
    message: &str,
) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            status: "error".into(),
            error_code: error_code.into(),
            message: message.into(),
            timed_out: false,
        }),
    )
}
