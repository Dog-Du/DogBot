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
    models::{
        ErrorResponse, MessageRequest, MessageResponse, RunRequest, RunResponse,
        ValidatedRunRequest,
    },
    normalizer::normalize_agent_output,
    platforms::{PlatformRegistry, delivery_context_from_event, run_response_output},
    protocol::{CanonicalEvent, MessagePart, OutboundMessage, OutboundPlan},
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

#[derive(Clone)]
struct AppState {
    settings: Settings,
    queue_tx: mpsc::Sender<QueuedRun>,
    session_store: SessionStore,
    history_store: Arc<StdMutex<HistoryStore>>,
    history_cleanup_state: Arc<StdMutex<HistoryCleanupState>>,
    platform_registry: PlatformRegistry,
    message_override: Option<Arc<dyn Messenger>>,
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
    settings.wechatpadpro_base_url = "http://127.0.0.1:38849".into();
    build_test_app_with_settings(runner, settings)
}

pub fn build_test_app_with_settings(runner: Arc<dyn Runner>, settings: Settings) -> Router {
    let session_store = SessionStore::open(&settings.session_db_path).expect("session store");
    let history_store = Arc::new(StdMutex::new(
        HistoryStore::open(&settings.history_db_path).expect("history store"),
    ));
    let queue_tx = spawn_dispatcher(settings.clone(), runner);
    let platform_registry =
        PlatformRegistry::from_settings(&settings).expect("default platform registry");
    router(AppState {
        settings,
        queue_tx,
        session_store,
        history_store,
        history_cleanup_state: Arc::new(StdMutex::new(HistoryCleanupState::default())),
        platform_registry,
        message_override: None,
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
    let platform_registry =
        PlatformRegistry::from_settings(&settings).expect("default platform registry");
    router(AppState {
        settings,
        queue_tx,
        session_store,
        history_store,
        history_cleanup_state: Arc::new(StdMutex::new(HistoryCleanupState::default())),
        platform_registry,
        message_override: Some(messenger),
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
    let platform_registry =
        PlatformRegistry::from_settings(&settings).map_err(|err| io::Error::other(err.message))?;
    let queue_tx = spawn_dispatcher(settings.clone(), runner);
    Ok(router(AppState {
        settings,
        queue_tx,
        session_store,
        history_store,
        history_cleanup_state: Arc::new(StdMutex::new(HistoryCleanupState::default())),
        platform_registry,
        message_override: None,
    }))
}

fn router(state: AppState) -> Router {
    let mut router = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/runs", post(run))
        .route("/v1/messages", post(send_message));

    for (platform_id, route) in state.platform_registry.ingress_routes().iter().cloned() {
        let platform_id_get = platform_id.clone();
        let mut method_router = get(move |State(state): State<AppState>| {
            let platform_id = platform_id_get.clone();
            async move { platform_probe(state, &platform_id).await }
        });
        if route.allow_head {
            let platform_id_head = platform_id.clone();
            method_router = method_router.head(move |State(state): State<AppState>| {
                let platform_id = platform_id_head.clone();
                async move { platform_probe(state, &platform_id).await }
            });
        }

        let platform_id_post = platform_id.clone();
        method_router = method_router.post(move |State(state): State<AppState>, body: Bytes| {
            let platform_id = platform_id_post.clone();
            async move { handle_platform_event(state, &platform_id, body).await }
        });

        router = router.route(route.path, method_router);
    }

    router.with_state(state)
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

async fn platform_probe(state: AppState, platform_id: &str) -> Json<Value> {
    let payload = state
        .platform_registry
        .get(platform_id)
        .map(|adapter| adapter.probe_payload())
        .unwrap_or_else(|| json!({ "status": "ok" }));
    Json(payload)
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

    map_run_result_to_response(execute_run_request(&state, request).await)
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

    match deliver_message_request(&state, request, session).await {
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

async fn handle_platform_event(state: AppState, platform_id: &str, body: Bytes) -> Response {
    maybe_purge_expired_history(&state);

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(err) => {
            return error_response(StatusCode::BAD_REQUEST, "invalid_json", &err.to_string())
                .into_response();
        }
    };

    let Some(adapter) = state.platform_registry.get(platform_id) else {
        return error_response(
            StatusCode::NOT_FOUND,
            "platform_not_found",
            "platform is not registered",
        )
        .into_response();
    };

    let Some(event) = adapter.decode_event(&payload)
    else {
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
        TriggerDecision::Status => {
            let plan = status_outbound_plan();
            match deliver_plan_for_event(state, &event, &plan).await {
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
        TriggerDecision::Run => {
            let Some(request) = build_run_request_from_event(&event) else {
                return accepted_response("ignored");
            };
            let run_response = match execute_run_request(state, request).await {
                Ok(response) => response,
                Err(error) => return map_run_result_to_response(Err(error)),
            };

            let output = run_response_output(&run_response);
            if output.is_empty() {
                return accepted_response("accepted");
            }

            let plan = match normalize_agent_output(output) {
                Ok(plan) => plan,
                Err(err) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(to_internal_error_message(&format!(
                            "failed to normalize agent output: {err}"
                        ))),
                    )
                        .into_response();
                }
            };

            match deliver_plan_for_event(state, &event, &plan).await {
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
    }
}

fn status_outbound_plan() -> OutboundPlan {
    OutboundPlan {
        messages: vec![OutboundMessage {
            parts: vec![MessagePart::Text {
                text: "agent-runner ok".into(),
            }],
            reply_to: None,
            delivery_policy: None,
        }],
        actions: vec![],
        delivery_report_policy: None,
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

async fn execute_run_request(
    state: &AppState,
    request: RunRequest,
) -> Result<RunResponse, ErrorResponse> {
    let validated = match request.validate(
        state.settings.default_timeout_secs,
        state.settings.max_timeout_secs,
    ) {
        Ok(validated) => validated,
        Err(message) => {
            return Err(ErrorResponse {
                status: "error".into(),
                error_code: "invalid_request".into(),
                message,
                timed_out: false,
            });
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
            return Err(ErrorResponse {
                status: "error".into(),
                error_code: "queue_full".into(),
                message: "run queue is full".into(),
                timed_out: false,
            });
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            return Err(to_internal_error_message("run queue is closed"));
        }
    }

    match receiver.await {
        Ok(result) => result,
        Err(_) => Err(to_internal_error_message("dispatcher dropped run result")),
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

fn map_run_result_to_response(result: Result<RunResponse, ErrorResponse>) -> Response {
    match result {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) if error.error_code == "invalid_request" => {
            (StatusCode::BAD_REQUEST, Json(error)).into_response()
        }
        Err(error) if error.error_code == "queue_full" || error.error_code == "rate_limited" => {
            (StatusCode::TOO_MANY_REQUESTS, Json(error)).into_response()
        }
        Err(error) if error.timed_out => (StatusCode::REQUEST_TIMEOUT, Json(error)).into_response(),
        Err(error) if error.error_code == "session_conflict" => {
            (StatusCode::CONFLICT, Json(error)).into_response()
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response(),
    }
}

async fn deliver_message_request(
    state: &AppState,
    request: MessageRequest,
    session: crate::session_store::SessionRecord,
) -> Result<MessageResponse, ErrorResponse> {
    if let Some(override_messenger) = &state.message_override {
        return override_messenger.send(request, session).await;
    }

    let Some(adapter) = state.platform_registry.get(&session.platform) else {
        return Err(ErrorResponse {
            status: "error".into(),
            error_code: "unsupported_platform".into(),
            message: format!("platform is not registered: {}", session.platform),
            timed_out: false,
        });
    };

    let plan = OutboundPlan {
        messages: vec![OutboundMessage {
            parts: vec![MessagePart::Text {
                text: request.text.clone(),
            }],
            reply_to: request.reply_to_message_id.clone(),
            delivery_policy: None,
        }],
        actions: vec![],
        delivery_report_policy: None,
    };
    let context = crate::platforms::DeliveryContext {
        platform: session.platform.clone(),
        platform_account: session.platform_account.clone(),
        conversation_id: session.conversation_id.clone(),
        target_actor_id: request.mention_user_id.clone(),
        target_display_name: None,
        reply_to_message_id: request.reply_to_message_id.clone(),
        native_event: None,
    };

    adapter.send_plan(&context, &plan).await
}

async fn deliver_plan_for_event(
    state: &AppState,
    event: &CanonicalEvent,
    plan: &OutboundPlan,
) -> Result<MessageResponse, ErrorResponse> {
    if let Some(override_messenger) = &state.message_override {
        let text = plan
            .messages
            .iter()
            .flat_map(|message| message.parts.iter())
            .filter_map(|part| match part {
                MessagePart::Text { text } => Some(text.as_str()),
                MessagePart::Mention { display, .. } => Some(display.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        let request = MessageRequest {
            session_id: event.conversation.clone(),
            text,
            reply_to_message_id: None,
            mention_user_id: None,
        };
        let session = state
            .session_store
            .get_or_create_bound_session(
                &event.conversation,
                &event.platform,
                &event.platform_account,
                &event.conversation,
            )
            .map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "session_store_failed".into(),
                message: err.to_string(),
                timed_out: false,
            })?;
        return override_messenger.send(request, session).await;
    }

    let Some(adapter) = state.platform_registry.get(&event.platform) else {
        return Err(ErrorResponse {
            status: "error".into(),
            error_code: "unsupported_platform".into(),
            message: format!("platform is not registered: {}", event.platform),
            timed_out: false,
        });
    };

    let context = delivery_context_from_event(event);
    adapter.send_plan(&context, plan).await
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
