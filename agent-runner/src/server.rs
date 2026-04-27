use std::collections::HashMap;
use std::hash::{Hash, Hasher};
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
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    config::Settings,
    docker_client::DockerRuntime,
    exec::{DockerRunner, ExecutionBackend},
    history::{
        cleanup::purge_expired_history,
        store::{HistoryStore, HistoryStoreError},
    },
    models::{
        ErrorResponse, MessageRequest, MessageResponse, RunRequest, RunResponse,
        ValidatedRunRequest,
    },
    normalizer::normalize_agent_output,
    pipeline::MentionRef,
    platforms::{PlatformRegistry, delivery_context_from_event, run_response_output},
    protocol::{
        CanonicalEvent, CanonicalMessage, MessagePart, OutboundAction, OutboundMessage,
        OutboundPlan, ReactionAction,
    },
    scheduler::{Admission, SchedulerSnapshot, SchedulerState, TaskSummary, TerminalState},
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
    runner: Arc<dyn Runner>,
    scheduler: Arc<RunScheduler>,
    session_store: SessionStore,
    history_store: Arc<StdMutex<HistoryStore>>,
    history_cleanup_state: Arc<StdMutex<HistoryCleanupState>>,
    platform_registry: PlatformRegistry,
    message_override: Option<Arc<dyn Messenger>>,
}

#[derive(Clone)]
struct ScheduledRun {
    event: CanonicalEvent,
    request: RunRequest,
    validated: ValidatedRunRequest,
    summary: TaskSummary,
}

struct RunScheduler {
    inner: Mutex<RunSchedulerInner>,
    runner: Arc<dyn Runner>,
    platform_registry: PlatformRegistry,
    message_override: Option<Arc<dyn Messenger>>,
}

struct RunSchedulerInner {
    state: SchedulerState,
    queued_payloads: HashMap<String, ScheduledRun>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScheduleFeedback {
    Started,
    Queued { tasks_ahead: usize },
    QueueFull,
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

impl RunScheduler {
    fn new(
        max_concurrent_runs: usize,
        max_queue_depth: usize,
        runner: Arc<dyn Runner>,
        platform_registry: PlatformRegistry,
        message_override: Option<Arc<dyn Messenger>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(RunSchedulerInner {
                state: SchedulerState::new(max_concurrent_runs, max_queue_depth),
                queued_payloads: HashMap::new(),
            }),
            runner,
            platform_registry,
            message_override,
        })
    }

    async fn submit(
        self: &Arc<Self>,
        event: CanonicalEvent,
        request: RunRequest,
        validated: ValidatedRunRequest,
    ) -> ScheduleFeedback {
        let task_id = Uuid::new_v4().to_string();
        let trigger_message_id = request.trigger_message_id.clone();
        let summary = TaskSummary {
            task_id: task_id.clone(),
            conversation_key: conversation_key_for_request(&request),
            actor_id: request.user_id.clone(),
            trigger_message_id,
        };
        let scheduled = ScheduledRun {
            event,
            request,
            validated,
            summary: summary.clone(),
        };

        let mut start_now = None;
        let feedback = {
            let mut inner = self.inner.lock().await;
            match inner.state.admit(summary) {
                Admission::StartNow => {
                    start_now = Some(scheduled);
                    ScheduleFeedback::Started
                }
                Admission::Queued { tasks_ahead } => {
                    inner.queued_payloads.insert(task_id, scheduled);
                    ScheduleFeedback::Queued { tasks_ahead }
                }
                Admission::QueueFull => ScheduleFeedback::QueueFull,
            }
        };

        if let Some(task) = start_now {
            self.clone().spawn_task(task);
        }

        feedback
    }

    async fn snapshot(&self) -> SchedulerSnapshot {
        self.inner.lock().await.state.snapshot()
    }

    fn spawn_task(self: Arc<Self>, task: ScheduledRun) {
        tokio::spawn(async move {
            self.run_task(task).await;
        });
    }

    async fn run_task(self: Arc<Self>, task: ScheduledRun) {
        self.send_start_reaction_best_effort(&task).await;

        let result = self
            .runner
            .run(task.request.clone(), task.validated.clone())
            .await;
        let (terminal, terminal_summary) = match result {
            Ok(response) => {
                info!(
                    task_id = %task.summary.task_id,
                    conversation = %task.summary.conversation_key,
                    exit_code = response.exit_code,
                    timed_out = response.timed_out,
                    stdout_chars = response.stdout.chars().count(),
                    stderr_chars = response.stderr.chars().count(),
                    duration_ms = response.duration_ms,
                    "scheduled runner completed"
                );
                match self.deliver_run_response(&task.event, &response).await {
                    Ok(summary) => (TerminalState::Completed, summary),
                    Err(error) => {
                        warn!(
                            task_id = %task.summary.task_id,
                            conversation = %task.summary.conversation_key,
                            "scheduled run delivery failed: {error}"
                        );
                        (TerminalState::Failed, Some(error))
                    }
                }
            }
            Err(error) => {
                let summary = format!("runner failed: {}", error.message);
                warn!(
                    task_id = %task.summary.task_id,
                    conversation = %task.summary.conversation_key,
                    error_code = %error.error_code,
                    "scheduled runner failed"
                );
                self.deliver_error_best_effort(&task.event, &error).await;
                (TerminalState::Failed, Some(summary))
            }
        };

        let promoted = {
            let mut inner = self.inner.lock().await;
            let promoted =
                inner
                    .state
                    .finish(&task.summary.conversation_key, terminal, terminal_summary);
            promoted
                .into_iter()
                .filter_map(|summary| inner.queued_payloads.remove(&summary.task_id))
                .collect::<Vec<_>>()
        };

        for next in promoted {
            self.clone().spawn_task(next);
        }
    }

    async fn send_start_reaction_best_effort(&self, task: &ScheduledRun) {
        let Some(plan) = start_reaction_plan(&task.event, &task.summary.task_id) else {
            return;
        };
        if let Err(error) = deliver_plan_with(
            &self.platform_registry,
            &self.message_override,
            &task.event,
            &plan,
        )
        .await
        {
            warn!(
                task_id = %task.summary.task_id,
                conversation = %task.summary.conversation_key,
                "failed to send start reaction: {}",
                error.message
            );
        }
    }

    async fn deliver_run_response(
        &self,
        event: &CanonicalEvent,
        response: &RunResponse,
    ) -> Result<Option<String>, String> {
        let output = run_response_output(response);
        if output.is_empty() {
            return Ok(Some("runner produced empty output".to_string()));
        }

        let plan = normalize_agent_output(output)
            .map_err(|err| format!("failed to normalize agent output: {err}"))?;
        deliver_plan_with(
            &self.platform_registry,
            &self.message_override,
            event,
            &plan,
        )
        .await
        .map_err(|err| err.message)?;
        Ok(Some(short_terminal_summary(output)))
    }

    async fn deliver_error_best_effort(&self, event: &CanonicalEvent, error: &ErrorResponse) {
        let plan = OutboundPlan {
            messages: vec![OutboundMessage::text(&format!(
                "执行失败：{}",
                error.message
            ))],
            actions: vec![],
            delivery_report_policy: None,
        };
        if let Err(delivery_error) = deliver_plan_with(
            &self.platform_registry,
            &self.message_override,
            event,
            &plan,
        )
        .await
        {
            warn!(
                conversation = %event.conversation,
                "failed to deliver scheduled runner error: {}",
                delivery_error.message
            );
        }
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
    let session_store = SessionStore::open(&settings).expect("session store");
    let history_store = Arc::new(StdMutex::new(
        HistoryStore::open(&settings).expect("history store"),
    ));
    let platform_registry =
        PlatformRegistry::from_settings(&settings).expect("default platform registry");
    let scheduler = RunScheduler::new(
        settings.max_concurrent_runs,
        settings.max_queue_depth,
        runner.clone(),
        platform_registry.clone(),
        None,
    );
    router(AppState {
        runner,
        scheduler,
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
    let session_store = SessionStore::open(&settings).expect("session store");
    let history_store = Arc::new(StdMutex::new(
        HistoryStore::open(&settings).expect("history store"),
    ));
    let platform_registry =
        PlatformRegistry::from_settings(&settings).expect("default platform registry");
    let scheduler = RunScheduler::new(
        settings.max_concurrent_runs,
        settings.max_queue_depth,
        runner.clone(),
        platform_registry.clone(),
        Some(messenger.clone()),
    );
    router(AppState {
        runner,
        scheduler,
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
    let session_store =
        SessionStore::open(&settings).map_err(|err| io::Error::other(err.to_string()))?;
    session_store
        .initialize_schema()
        .map_err(|err| io::Error::other(err.to_string()))?;
    let history_store =
        HistoryStore::open(&settings).map_err(|err| io::Error::other(err.to_string()))?;
    history_store
        .initialize_schema()
        .map_err(|err| io::Error::other(err.to_string()))?;
    let history_store = Arc::new(StdMutex::new(history_store));
    let platform_registry =
        PlatformRegistry::from_settings(&settings).map_err(|err| io::Error::other(err.message))?;
    let scheduler = RunScheduler::new(
        settings.max_concurrent_runs,
        settings.max_queue_depth,
        runner.clone(),
        platform_registry.clone(),
        None,
    );
    Ok(router(AppState {
        runner,
        scheduler,
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
    info!(
        platform = platform_id,
        body_bytes = body.len(),
        "received platform ingress payload"
    );

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(err) => {
            warn!(
                platform = platform_id,
                "failed to decode platform payload JSON: {err}"
            );
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

    let Some(event) = adapter.decode_event(&payload) else {
        info!(
            platform = platform_id,
            "platform payload decoded to no canonical event"
        );
        return accepted_response("ignored");
    };

    handle_canonical_event(&state, event).await
}

async fn handle_canonical_event(state: &AppState, event: CanonicalEvent) -> Response {
    let decision = TriggerResolver::default().resolve(&event);
    info!(
        platform = %event.platform,
        conversation = %event.conversation,
        actor = %event.actor,
        event_id = %event.event_id,
        decision = ?decision,
        summary = %summarize_event_for_log(&event),
        "decoded canonical event"
    );

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
            let plan = status_outbound_plan(&state.scheduler.snapshot().await);
            info!(
                platform = %event.platform,
                conversation = %event.conversation,
                summary = %summarize_plan_for_log(&plan),
                "delivering status outbound plan"
            );
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
            info!(
                platform = %request.platform,
                conversation = %request.conversation_id,
                actor = %request.user_id,
                trigger_message_id = request.trigger_message_id.as_deref().unwrap_or(""),
                trigger_reply_to_message_id = request
                    .trigger_reply_to_message_id
                    .as_deref()
                    .unwrap_or(""),
                mention_refs = request.mention_refs.len(),
                prompt_chars = request.prompt.chars().count(),
                trigger_summary_chars = request
                    .trigger_summary
                    .as_deref()
                    .map(str::chars)
                    .map(Iterator::count)
                    .unwrap_or(0),
                "built run request from canonical event"
            );
            let validated = match request.validate() {
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
            match state
                .scheduler
                .submit(event.clone(), request, validated)
                .await
            {
                ScheduleFeedback::Started => accepted_response("accepted"),
                ScheduleFeedback::Queued { tasks_ahead } => {
                    let plan = queue_wait_plan(tasks_ahead);
                    deliver_plan_best_effort(state, &event, &plan).await;
                    accepted_response("queued")
                }
                ScheduleFeedback::QueueFull => {
                    let plan = queue_full_plan();
                    deliver_plan_best_effort(state, &event, &plan).await;
                    accepted_response("queue_full")
                }
            }
        }
    }
}

fn status_outbound_plan(snapshot: &SchedulerSnapshot) -> OutboundPlan {
    let recent = snapshot
        .recent_terminal
        .last()
        .map(|item| format!("{} {:?}", item.conversation_key, item.state))
        .unwrap_or_else(|| "none".to_string());
    let running = if snapshot.running.is_empty() {
        "none".to_string()
    } else {
        snapshot
            .running
            .iter()
            .map(|task| format!("{} {}", task.conversation_key, task.task_id))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let text = format!(
        "agent-runner ok\nrunning: {}/{}\nqueued: {}/{}\nrunning detail: {}\nrecent: {}",
        snapshot.active_count,
        snapshot.max_concurrent,
        snapshot.waiting_count,
        snapshot.max_queue_depth,
        running,
        recent
    );
    OutboundPlan {
        messages: vec![OutboundMessage {
            parts: vec![MessagePart::Text { text }],
            reply_to: None,
            suppress_default_reply: false,
            delivery_policy: None,
        }],
        actions: vec![],
        delivery_report_policy: None,
    }
}

fn queue_wait_plan(tasks_ahead: usize) -> OutboundPlan {
    OutboundPlan {
        messages: vec![OutboundMessage::text(&format!(
            "前面还有 {tasks_ahead} 个任务，轮到这条时我会给你一个反应。"
        ))],
        actions: vec![],
        delivery_report_policy: None,
    }
}

fn queue_full_plan() -> OutboundPlan {
    OutboundPlan {
        messages: vec![OutboundMessage::text("现在等待队列满了，稍后再发一次。")],
        actions: vec![],
        delivery_report_policy: None,
    }
}

async fn deliver_plan_best_effort(state: &AppState, event: &CanonicalEvent, plan: &OutboundPlan) {
    if let Err(error) = deliver_plan_for_event(state, event, plan).await {
        warn!(
            platform = %event.platform,
            conversation = %event.conversation,
            error_code = %error.error_code,
            message = %error.message,
            "best-effort feedback delivery failed"
        );
    }
}

fn start_reaction_plan(event: &CanonicalEvent, task_id: &str) -> Option<OutboundPlan> {
    if event.platform != "qq" {
        return None;
    }
    let target_message_id = event.message()?.message_id.clone();
    Some(OutboundPlan {
        messages: vec![],
        actions: vec![OutboundAction::ReactionAdd(ReactionAction {
            target_message_id,
            emoji: random_start_reaction(task_id).to_string(),
        })],
        delivery_report_policy: None,
    })
}

fn random_start_reaction(seed: &str) -> &'static str {
    const REACTIONS: [&str; 6] = ["👍", "🙏", "🫡", "😂", "❤", "👏"];
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    let index = (hasher.finish() as usize) % REACTIONS.len();
    REACTIONS[index]
}

fn conversation_key_for_request(request: &RunRequest) -> String {
    format!(
        "{}::{}",
        request.platform_account_id, request.conversation_id
    )
}

fn short_terminal_summary(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.chars().count() <= 80 {
        return trimmed.to_string();
    }
    let preview = trimmed.chars().take(80).collect::<String>();
    format!("{preview}...")
}

fn build_run_request_from_event(event: &CanonicalEvent) -> Option<RunRequest> {
    let message = event.message()?;
    let prompt = message.project_plain_text().trim().to_string();
    let (trigger_summary, mention_refs) =
        render_trigger_summary_with_refs(message, &event.platform_account);
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
        trigger_summary: Some(trigger_summary),
        trigger_message_id: Some(message.message_id.clone()),
        trigger_reply_to_message_id: message.reply_to.clone(),
        mention_refs,
        reply_excerpt: None,
        timeout_secs: None,
    })
}

fn render_trigger_summary_with_refs(
    message: &CanonicalMessage,
    platform_account: &str,
) -> (String, Vec<MentionRef>) {
    let mut summary = String::new();
    let mut mention_refs = Vec::new();

    for part in &message.parts {
        match part {
            MessagePart::Text { text } => summary.push_str(text),
            MessagePart::Mention { actor_id, display } => {
                if actor_id == platform_account {
                    summary.push_str(display);
                    continue;
                }

                let ref_id = format!("m{}", mention_refs.len() + 1);
                summary.push_str(display);
                summary.push_str("[#");
                summary.push_str(&ref_id);
                summary.push(']');
                mention_refs.push(MentionRef {
                    ref_id,
                    actor_id: actor_id.clone(),
                    display: display.clone(),
                });
            }
            _ => {}
        }
    }

    let summary = summary.trim().to_string();
    if summary.is_empty() {
        (
            message.project_plain_text().trim().to_string(),
            mention_refs,
        )
    } else {
        (summary, mention_refs)
    }
}

async fn execute_run_request(
    state: &AppState,
    request: RunRequest,
) -> Result<RunResponse, ErrorResponse> {
    let validated = match request.validate() {
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

    info!(
        conversation = %request.conversation_id,
        actor = %request.user_id,
        "running direct run request"
    );
    state.runner.run(request, validated).await
}

fn ensure_history_ingest_state_for_trigger(
    state: &AppState,
    event: &CanonicalEvent,
    decision: &TriggerDecision,
) -> Result<(), HistoryStoreError> {
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
) -> Result<(), HistoryStoreError> {
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
    info!(
        session_id = %request.session_id,
        platform = %session.platform,
        conversation = %session.conversation_id,
        reply_to = request.reply_to_message_id.as_deref().unwrap_or(""),
        mention_user = request.mention_user_id.as_deref().unwrap_or(""),
        text_chars = request.text.chars().count(),
        "delivering explicit message request"
    );
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
            suppress_default_reply: false,
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
    deliver_plan_with(
        &state.platform_registry,
        &state.message_override,
        event,
        plan,
    )
    .await
}

async fn deliver_plan_with(
    platform_registry: &PlatformRegistry,
    message_override: &Option<Arc<dyn Messenger>>,
    event: &CanonicalEvent,
    plan: &OutboundPlan,
) -> Result<MessageResponse, ErrorResponse> {
    info!(
        platform = %event.platform,
        conversation = %event.conversation,
        event_id = %event.event_id,
        summary = %summarize_plan_for_log(plan),
        "delivering outbound plan for canonical event"
    );
    if let Some(override_messenger) = message_override {
        if plan.messages.is_empty() {
            return Ok(MessageResponse {
                status: "ok".into(),
                message_id: None,
            });
        }

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
        let session = session_record_from_event(event);
        return override_messenger.send(request, session).await;
    }

    let Some(adapter) = platform_registry.get(&event.platform) else {
        return Err(ErrorResponse {
            status: "error".into(),
            error_code: "unsupported_platform".into(),
            message: format!("platform is not registered: {}", event.platform),
            timed_out: false,
        });
    };

    let context = delivery_context_from_event(event);
    let response = adapter.send_plan(&context, plan).await;
    match &response {
        Ok(message) => info!(
            platform = %event.platform,
            conversation = %event.conversation,
            message_id = message.message_id.as_deref().unwrap_or(""),
            "platform delivery succeeded"
        ),
        Err(error) => error!(
            platform = %event.platform,
            conversation = %event.conversation,
            error_code = %error.error_code,
            message = %error.message,
            "platform delivery failed"
        ),
    }
    response
}

fn session_record_from_event(event: &CanonicalEvent) -> crate::session_store::SessionRecord {
    let session_key = format!(
        "conversation::{}::{}::{}",
        event.platform, event.platform_account, event.conversation
    );
    let timestamp = event.timestamp_epoch_secs;
    crate::session_store::SessionRecord {
        session_key,
        external_session_id: event.conversation.clone(),
        claude_session_id: String::new(),
        platform: event.platform.clone(),
        platform_account: event.platform_account.clone(),
        conversation_id: event.conversation.clone(),
        user_id: String::new(),
        created_at_epoch_secs: timestamp,
        last_used_at_epoch_secs: timestamp,
        is_new: false,
    }
}

fn summarize_event_for_log(event: &CanonicalEvent) -> String {
    match &event.kind {
        crate::protocol::EventKind::Message { message } => format!(
            "kind=message message_id={} reply_to={} parts={} mentions={}",
            message.message_id,
            message.reply_to.as_deref().unwrap_or("-"),
            message.parts.len(),
            message.mentions.len()
        ),
        crate::protocol::EventKind::ReactionAdded {
            target_message_id,
            emoji,
        } => format!("kind=reaction_added target_message_id={target_message_id} emoji={emoji}"),
        crate::protocol::EventKind::ReactionRemoved {
            target_message_id,
            emoji,
        } => format!("kind=reaction_removed target_message_id={target_message_id} emoji={emoji}"),
    }
}

fn summarize_plan_for_log(plan: &OutboundPlan) -> String {
    let mut replies = 0usize;
    let mut suppressed_replies = 0usize;
    let mut mentions = 0usize;
    let mut text_parts = 0usize;
    let mut media_parts = 0usize;
    let mut reaction_add = 0usize;
    let mut reaction_remove = 0usize;

    for message in &plan.messages {
        if message.reply_to.is_some() {
            replies += 1;
        }
        if message.suppress_default_reply {
            suppressed_replies += 1;
        }

        for part in &message.parts {
            match part {
                MessagePart::Text { .. } => text_parts += 1,
                MessagePart::Mention { .. } => mentions += 1,
                MessagePart::Image { .. }
                | MessagePart::File { .. }
                | MessagePart::Voice { .. }
                | MessagePart::Video { .. }
                | MessagePart::Sticker { .. } => media_parts += 1,
                MessagePart::Quote { .. } => replies += 1,
            }
        }
    }

    for action in &plan.actions {
        match action {
            crate::protocol::OutboundAction::ReactionAdd(_) => reaction_add += 1,
            crate::protocol::OutboundAction::ReactionRemove(_) => reaction_remove += 1,
        }
    }

    format!(
        "messages={} actions={} replies={} suppress_default_replies={} mentions={} text_parts={} media_parts={} reaction_add={} reaction_remove={}",
        plan.messages.len(),
        plan.actions.len(),
        replies,
        suppressed_replies,
        mentions,
        text_parts,
        media_parts,
        reaction_add,
        reaction_remove
    )
}

fn accepted_response(status: &str) -> Response {
    (StatusCode::OK, Json(json!({ "status": status }))).into_response()
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

#[cfg(test)]
mod logging_summary_tests {
    use super::{summarize_event_for_log, summarize_plan_for_log};
    use crate::protocol::{
        CanonicalEvent, CanonicalMessage, EventKind, MessagePart, OutboundAction, OutboundMessage,
        OutboundPlan, ReactionAction,
    };

    #[test]
    fn event_summary_mentions_trigger_message_and_refs() {
        let event = CanonicalEvent {
            platform: "qq".into(),
            platform_account: "qq:bot_uin:123".into(),
            conversation: "qq:group:5566".into(),
            actor: "qq:user:42".into(),
            event_id: "evt-1".into(),
            timestamp_epoch_secs: 1,
            kind: EventKind::Message {
                message: CanonicalMessage {
                    message_id: "msg-9".into(),
                    reply_to: Some("msg-7".into()),
                    parts: vec![
                        MessagePart::Mention {
                            actor_id: "qq:bot_uin:123".into(),
                            display: "@DogDu".into(),
                        },
                        MessagePart::Text {
                            text: " 请看 ".into(),
                        },
                        MessagePart::Mention {
                            actor_id: "qq:user:77".into(),
                            display: "@fly-dog".into(),
                        },
                    ],
                    mentions: vec!["qq:bot_uin:123".into()],
                    native_metadata: serde_json::json!({}),
                },
            },
            raw_native_payload: serde_json::json!({}),
        };

        let summary = summarize_event_for_log(&event);
        assert!(summary.contains("kind=message"));
        assert!(summary.contains("message_id=msg-9"));
        assert!(summary.contains("reply_to=msg-7"));
        assert!(summary.contains("mentions=1"));
        assert!(summary.contains("parts=3"));
    }

    #[test]
    fn plan_summary_mentions_message_action_and_reply_counts() {
        let plan = OutboundPlan {
            messages: vec![OutboundMessage {
                parts: vec![
                    MessagePart::Mention {
                        actor_id: "qq:user:77".into(),
                        display: "@fly-dog".into(),
                    },
                    MessagePart::Text {
                        text: "收到".into(),
                    },
                ],
                reply_to: Some("msg-9".into()),
                suppress_default_reply: false,
                delivery_policy: None,
            }],
            actions: vec![OutboundAction::ReactionAdd(ReactionAction {
                target_message_id: "msg-9".into(),
                emoji: "👍".into(),
            })],
            delivery_report_policy: None,
        };

        let summary = summarize_plan_for_log(&plan);
        assert!(summary.contains("messages=1"));
        assert!(summary.contains("actions=1"));
        assert!(summary.contains("replies=1"));
        assert!(summary.contains("mentions=1"));
        assert!(summary.contains("reaction_add=1"));
        assert!(summary.contains("text_parts=1"));
    }
}
