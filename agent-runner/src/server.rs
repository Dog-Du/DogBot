use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
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
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::warn;

use crate::{
    config::Settings,
    docker_client::DockerRuntime,
    exec::{DockerRunner, ExecutionBackend},
    history::{cleanup::purge_expired_history, store::HistoryStore},
    messenger::{MessageDelivery, NapCatMessenger},
    models::{
        ErrorResponse, MessageRequest, MessageResponse, RunRequest, RunResponse,
        ValidatedRunRequest,
    },
    platforms::{qq, wechatpadpro},
    protocol::CanonicalEvent,
    session_store::SessionStore,
    trigger_resolver::{TriggerDecision, TriggerResolver},
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
    history_store: Arc<StdMutex<HistoryStore>>,
    history_cleanup_state: Arc<StdMutex<HistoryCleanupState>>,
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

const DEFAULT_HISTORY_RETENTION_DAYS: i64 = 180;
const HISTORY_CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);
const DEFAULT_QQ_ACCOUNT_ID: &str = "qq:bot_uin:123";
const DEFAULT_QQ_BOT_ID: &str = "123";
const DEFAULT_WECHATPADPRO_ACCOUNT_ID: &str = "wechatpadpro:account:bot";

#[derive(Default)]
struct HistoryCleanupState {
    last_run_started_at: Option<Instant>,
}

impl HistoryCleanupState {
    fn should_run(&mut self, now: Instant, interval: Duration) -> bool {
        match self.last_run_started_at {
            Some(last_run_started_at) if now.duration_since(last_run_started_at) < interval => {
                false
            }
            _ => {
                self.last_run_started_at = Some(now);
                true
            }
        }
    }
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
    settings.history_db_path = temp_state_dir
        .join("state/history.db")
        .display()
        .to_string();
    settings.platform_qq_account_id = Some(DEFAULT_QQ_ACCOUNT_ID.into());
    settings.platform_qq_bot_id = Some(DEFAULT_QQ_BOT_ID.into());
    settings.platform_wechatpadpro_account_id = Some(DEFAULT_WECHATPADPRO_ACCOUNT_ID.into());
    build_test_app_with_settings(runner, settings)
}

pub fn build_test_app_with_settings(runner: Arc<dyn Runner>, settings: Settings) -> Router {
    let session_store = SessionStore::open(&settings.session_db_path).expect("session store");
    let history_store = Arc::new(StdMutex::new(
        HistoryStore::open(&settings.history_db_path).expect("history store"),
    ));
    let queue_tx = spawn_dispatcher(settings.clone(), runner);
    let messenger =
        Arc::new(NapCatMessenger::from_settings(&settings).expect("default NapCat messenger"));
    router(AppState {
        settings,
        queue_tx,
        session_store,
        history_store,
        history_cleanup_state: Arc::new(StdMutex::new(HistoryCleanupState::default())),
        messenger,
    })
}

pub fn build_test_app_with_message_support(
    runner: Arc<dyn Runner>,
    messenger: Arc<dyn Messenger>,
    settings: Settings,
) -> Router {
    let session_store = SessionStore::open(&settings.session_db_path).expect("session store");
    let history_store = Arc::new(StdMutex::new(
        HistoryStore::open(&settings.history_db_path).expect("history store"),
    ));
    let queue_tx = spawn_dispatcher(settings.clone(), runner);
    router(AppState {
        settings,
        queue_tx,
        session_store,
        history_store,
        history_cleanup_state: Arc::new(StdMutex::new(HistoryCleanupState::default())),
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
    let history_store = Arc::new(StdMutex::new(
        HistoryStore::open(&settings.history_db_path)
            .map_err(|err| io::Error::other(err.to_string()))?,
    ));
    let messenger = Arc::new(
        NapCatMessenger::from_settings(&settings).map_err(|err| io::Error::other(err.message))?,
    );
    let queue_tx = spawn_dispatcher(settings.clone(), runner);
    Ok(router(AppState {
        settings,
        queue_tx,
        session_store,
        history_store,
        history_cleanup_state: Arc::new(StdMutex::new(HistoryCleanupState::default())),
        messenger,
    }))
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/runs", post(run))
        .route("/v1/messages", post(send_message))
        .route(
            "/v1/platforms/wechatpadpro/events",
            get(wechat_probe)
                .head(wechat_probe)
                .post(handle_wechatpadpro_event),
        )
        .route(
            "/v1/platforms/qq/napcat/ws",
            get(qq_probe).post(handle_qq_napcat_event),
        )
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

async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn wechat_probe() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn qq_probe() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn run(State(state): State<AppState>, body: Bytes) -> Response {
    maybe_purge_expired_history(&state);

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

    request.user_id = match normalize_context_identifier(&request.user_id, "user_id") {
        Ok(user_id) => user_id,
        Err(message) => {
            return error_response(StatusCode::BAD_REQUEST, "invalid_request", &message)
                .into_response();
        }
    };
    request.conversation_id =
        match normalize_context_identifier(&request.conversation_id, "conversation_id") {
            Ok(conversation_id) => conversation_id,
            Err(message) => {
                return error_response(StatusCode::BAD_REQUEST, "invalid_request", &message)
                    .into_response();
            }
        };
    request.platform_account_id =
        match normalize_context_identifier(&request.platform_account_id, "platform_account_id") {
            Ok(platform_account_id) => platform_account_id,
            Err(message) => {
                return error_response(StatusCode::BAD_REQUEST, "invalid_request", &message)
                    .into_response();
            }
        };

    match enqueue_run_request(&state, request).await {
        Ok(response) => response,
        Err(response) => response,
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

    request.session_id = request.session_id.trim().to_string();

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

async fn handle_wechatpadpro_event(State(state): State<AppState>, body: Bytes) -> Response {
    maybe_purge_expired_history(&state);

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(err) => {
            return error_response(StatusCode::BAD_REQUEST, "invalid_json", &err.to_string())
                .into_response();
        }
    };

    let platform_account = state
        .settings
        .platform_wechatpadpro_account_id
        .clone()
        .unwrap_or_else(|| DEFAULT_WECHATPADPRO_ACCOUNT_ID.to_string());
    let mention_names = state
        .settings
        .platform_wechatpadpro_bot_mention_names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    let Some(event) =
        wechatpadpro::decode_webhook_event(&payload, &platform_account, &mention_names)
    else {
        return accepted_response("ignored");
    };

    handle_canonical_event(&state, event).await
}

async fn handle_qq_napcat_event(State(state): State<AppState>, body: Bytes) -> Response {
    maybe_purge_expired_history(&state);

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(err) => {
            return error_response(StatusCode::BAD_REQUEST, "invalid_json", &err.to_string())
                .into_response();
        }
    };

    let platform_account = state
        .settings
        .platform_qq_account_id
        .clone()
        .or_else(|| {
            state
                .settings
                .platform_qq_bot_id
                .as_ref()
                .map(|bot_id| format!("qq:bot_uin:{bot_id}"))
        })
        .unwrap_or_else(|| DEFAULT_QQ_ACCOUNT_ID.to_string());

    let Some(event) = qq::decode_napcat_event(&payload, &platform_account) else {
        return accepted_response("ignored");
    };

    handle_canonical_event(&state, event).await
}

async fn handle_canonical_event(state: &AppState, event: CanonicalEvent) -> Response {
    let decision = TriggerResolver::default().resolve(&event);

    if let Err(err) = ensure_history_ingest_state_for_trigger(state, &event, &decision) {
        warn!(
            conversation = %event.conversation,
            event_id = %event.event_id,
            "failed to update history ingest state: {err}"
        );
    }

    if let Err(err) = mirror_history_event_if_enabled(state, &event) {
        warn!(
            conversation = %event.conversation,
            event_id = %event.event_id,
            "failed to mirror canonical event into history store: {err}"
        );
    }

    match decision {
        TriggerDecision::Ignore => accepted_response("ignored"),
        TriggerDecision::Run | TriggerDecision::Status => {
            let Some(request) = build_run_request_from_event(&event) else {
                return accepted_response("ignored");
            };
            match enqueue_run_request(state, request).await {
                Ok(response) => response,
                Err(response) => response,
            }
        }
    }
}

fn build_run_request_from_event(event: &CanonicalEvent) -> Option<RunRequest> {
    let message = event.message()?;
    let prompt = message.project_plain_text().trim().to_string();
    let chat_type = match conversation_scope(&event.conversation) {
        Some("private") => "private",
        Some("group") => "group",
        _ => "unknown",
    };

    Some(RunRequest {
        platform: event.platform.clone(),
        platform_account_id: event.platform_account.clone(),
        conversation_id: event.conversation.clone(),
        session_id: event.conversation.clone(),
        user_id: event.actor.clone(),
        chat_type: chat_type.into(),
        cwd: "/workspace".into(),
        prompt: prompt.clone(),
        trigger_summary: Some(prompt),
        reply_excerpt: None,
        timeout_secs: None,
    })
}

async fn enqueue_run_request(state: &AppState, request: RunRequest) -> Result<Response, Response> {
    let validated = match request.validate(
        state.settings.default_timeout_secs,
        state.settings.max_timeout_secs,
    ) {
        Ok(validated) => validated,
        Err(message) => {
            return Err(
                error_response(StatusCode::BAD_REQUEST, "invalid_request", &message)
                    .into_response(),
            );
        }
    };

    let (responder, receiver) = oneshot::channel();
    match state.queue_tx.try_send(QueuedRun {
        request,
        validated,
        responder,
    }) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            return Err(error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "queue_full",
                "run queue is full",
            )
            .into_response());
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(to_internal_error_message("run queue is closed")),
            )
                .into_response());
        }
    }

    match receiver.await {
        Ok(Ok(response)) => Ok((StatusCode::OK, Json(response)).into_response()),
        Ok(Err(error)) if error.timed_out => {
            Ok((StatusCode::REQUEST_TIMEOUT, Json(error)).into_response())
        }
        Ok(Err(error)) if error.error_code == "rate_limited" => {
            Ok((StatusCode::TOO_MANY_REQUESTS, Json(error)).into_response())
        }
        Ok(Err(error)) if error.error_code == "session_conflict" => {
            Ok((StatusCode::CONFLICT, Json(error)).into_response())
        }
        Ok(Err(error)) => Ok((StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(to_internal_error_message("dispatcher dropped run result")),
        )
            .into_response()),
    }
}

fn ensure_history_ingest_state_for_trigger(
    state: &AppState,
    event: &CanonicalEvent,
    decision: &TriggerDecision,
) -> rusqlite::Result<()> {
    if matches!(decision, TriggerDecision::Ignore) {
        return Ok(());
    }

    let store = state
        .history_store
        .lock()
        .expect("history store mutex poisoned");
    if store.ingest_enabled(&event.platform_account, &event.conversation)? {
        return Ok(());
    }

    store.upsert_ingest_state(
        &event.platform_account,
        &event.conversation,
        true,
        DEFAULT_HISTORY_RETENTION_DAYS,
    )
}

fn mirror_history_event_if_enabled(
    state: &AppState,
    event: &CanonicalEvent,
) -> rusqlite::Result<()> {
    let store = state
        .history_store
        .lock()
        .expect("history store mutex poisoned");
    if !store.ingest_enabled(&event.platform_account, &event.conversation)? {
        return Ok(());
    }

    store.insert_canonical_event(event)
}

fn maybe_purge_expired_history(state: &AppState) {
    let should_run = {
        let mut cleanup_state = state
            .history_cleanup_state
            .lock()
            .expect("history cleanup mutex poisoned");
        cleanup_state.should_run(Instant::now(), HISTORY_CLEANUP_INTERVAL)
    };

    if !should_run {
        return;
    }

    let store = state
        .history_store
        .lock()
        .expect("history store mutex poisoned");
    if let Err(err) = purge_expired_history(&store) {
        warn!("failed to purge expired history: {err}");
    }
}

fn accepted_response(status: &str) -> Response {
    (StatusCode::OK, Json(json!({ "status": status }))).into_response()
}

fn is_limit_exhausted(limit: usize, current_len: usize) -> bool {
    limit > 0 && current_len >= limit
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

fn normalize_context_identifier(value: &str, field_name: &str) -> Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(format!("{field_name} must be non-empty"));
    }

    if normalized.chars().any(|ch| ch.is_control() || ch == '`') {
        return Err(format!(
            "{field_name} contains unsupported control characters or backticks"
        ));
    }

    Ok(normalized.to_string())
}

fn conversation_scope(conversation: &str) -> Option<&str> {
    let mut parts = conversation.splitn(3, ':');
    let _platform = parts.next()?;
    parts.next()
}
