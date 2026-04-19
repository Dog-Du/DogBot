use std::time::Instant;

use async_trait::async_trait;
use tokio::time::{Duration, timeout};
use tracing::warn;

use crate::config::Settings;
use crate::context::memory_intent::capture_memory_intent;
use crate::context::object_store::ContextObjectStore;
use crate::docker_client::{ContainerSpec, DockerRuntime};
use crate::models::{ErrorResponse, RunRequest, RunResponse, ValidatedRunRequest};
use crate::session_store::{SessionStore, SessionStoreError};

#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    async fn execute(
        &self,
        request: RunRequest,
        validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse>;
}

#[derive(Clone)]
pub struct DockerRunner {
    runtime: DockerRuntime,
    container_spec: ContainerSpec,
    session_store: SessionStore,
    context_store: Option<ContextObjectStore>,
}

impl DockerRunner {
    pub fn new(runtime: DockerRuntime, settings: Settings) -> Result<Self, ErrorResponse> {
        let container_spec = ContainerSpec::from_settings(&settings);
        let session_store =
            SessionStore::open(&settings.session_db_path).map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "session_store_unavailable".into(),
                message: err.to_string(),
                timed_out: false,
            })?;

        let context_store = match ContextObjectStore::open(&settings.control_plane_db_path) {
            Ok(store) => Some(store),
            Err(err) => {
                warn!(
                    "control-plane store unavailable at {}: {}; memory candidates will not be persisted",
                    settings.control_plane_db_path,
                    err
                );
                None
            }
        };

        Ok(Self {
            runtime,
            container_spec,
            session_store,
            context_store,
        })
    }
}

#[async_trait]
impl ExecutionBackend for DockerRunner {
    async fn execute(
        &self,
        request: RunRequest,
        validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
        let session = self
            .session_store
            .get_or_create_session(
                &request.session_id,
                &request.platform,
                &request.conversation_id,
                &request.user_id,
            )
            .map_err(map_session_store_error)?;

        self.runtime
            .ensure_container_running(&self.container_spec)
            .await
            .map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "container_unavailable".into(),
                message: err.to_string(),
                timed_out: false,
            })?;

        let started = Instant::now();
        let first = self
            .execute_once(
                &request.prompt,
                &validated,
                &session,
                &request.user_id,
                &request.conversation_id,
            )
            .await?;

        if should_retry_with_fresh_session(&first, session.is_new) {
            let reset_session = self
                .session_store
                .reset_session(
                    &request.session_id,
                    &request.platform,
                    &request.conversation_id,
                    &request.user_id,
                )
                .map_err(map_session_store_error)?;

            let retried = self
                .execute_once(
                    &request.prompt,
                    &validated,
                    &reset_session,
                    &request.user_id,
                    &request.conversation_id,
                )
                .await?;
            return Ok(with_duration(retried, started.elapsed().as_millis()));
        }

        Ok(with_duration(first, started.elapsed().as_millis()))
    }
}

impl DockerRunner {
    async fn execute_once(
        &self,
        prompt: &str,
        validated: &ValidatedRunRequest,
        session: &crate::session_store::SessionRecord,
        actor_id: &str,
        conversation_id: &str,
    ) -> Result<RunResponse, ErrorResponse> {
        let exec = self
            .runtime
            .create_claude_exec(
                &self.container_spec.container_name,
                &validated.cwd,
                build_claude_command(prompt, &session.claude_session_id, session.is_new),
            )
            .await
            .map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "exec_create_failed".into(),
                message: err.to_string(),
                timed_out: false,
            })?;

        let result = timeout(
            Duration::from_secs(validated.timeout_secs),
            self.runtime.collect_exec_output(&exec.id),
        )
        .await;

        match result {
            Ok(Ok((stdout, stderr, exit_code))) => {
                if let Some(store) = &self.context_store {
                    if let Some(captured_intent) = capture_memory_intent(&stdout) {
                        if let Err(err) = store.insert_memory_candidate(
                            actor_id,
                            conversation_id,
                            &captured_intent.raw_json,
                        ) {
                            warn!(
                                "failed to persist memory candidate for actor={} conversation={}: {}",
                                actor_id, conversation_id, err
                            );
                        }
                    }
                }

                Ok(RunResponse {
                    status: "ok".into(),
                    stdout,
                    stderr,
                    exit_code,
                    timed_out: false,
                    duration_ms: 0,
                })
            }
            Ok(Err(err)) => Err(ErrorResponse {
                status: "error".into(),
                error_code: "exec_failed".into(),
                message: err.to_string(),
                timed_out: false,
            }),
            Err(_) => {
                if let Ok(Some(pid)) = self.runtime.exec_pid(&exec.id).await {
                    let _ = self
                        .runtime
                        .kill_pid(&self.container_spec.container_name, pid, "TERM")
                        .await;
                    let _ = self
                        .runtime
                        .kill_pid(&self.container_spec.container_name, pid, "KILL")
                        .await;
                }
                let _ = self
                    .runtime
                    .kill_claude_execs(&self.container_spec.container_name)
                    .await;

                Err(ErrorResponse {
                    status: "error".into(),
                    error_code: "timeout".into(),
                    message: "command exceeded timeout".into(),
                    timed_out: true,
                })
            }
        }
    }
}

fn with_duration(mut response: RunResponse, duration_ms: u128) -> RunResponse {
    response.duration_ms = duration_ms;
    response
}

fn should_retry_with_fresh_session(response: &RunResponse, is_new_session: bool) -> bool {
    if is_new_session || response.timed_out || response.exit_code != 0 {
        return false;
    }

    let stdout = response.stdout.trim();
    let stderr = response.stderr.trim();
    looks_like_missing_session(stdout) || looks_like_missing_session(stderr)
}

fn looks_like_missing_session(text: &str) -> bool {
    text.contains("No conversation found with session ID:")
}

fn build_claude_command(
    prompt: &str,
    claude_session_id: &str,
    is_new_session: bool,
) -> Vec<String> {
    let mut command = vec![
        "claude".to_string(),
        "--print".to_string(),
        "--dangerously-skip-permissions".to_string(),
        "--add-dir".to_string(),
        "/workspace".to_string(),
        "/state".to_string(),
    ];

    if is_new_session {
        command.push("--session-id".to_string());
    } else {
        command.push("--resume".to_string());
    }

    command.push(claude_session_id.to_string());
    command.push(prompt.to_string());
    command
}

fn map_session_store_error(err: SessionStoreError) -> ErrorResponse {
    let error_code = match err {
        SessionStoreError::SessionConflict { .. } => "session_conflict",
        _ => "session_store_failed",
    };

    ErrorResponse {
        status: "error".into(),
        error_code: error_code.into(),
        message: err.to_string(),
        timed_out: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_claude_command, should_retry_with_fresh_session};
    use crate::models::RunResponse;

    #[test]
    fn build_claude_command_uses_session_id_for_new_sessions() {
        let command = build_claude_command("hello", "uuid-1", true);
        assert_eq!(
            command,
            vec![
                "claude".to_string(),
                "--print".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--add-dir".to_string(),
                "/workspace".to_string(),
                "/state".to_string(),
                "--session-id".to_string(),
                "uuid-1".to_string(),
                "hello".to_string(),
            ]
        );
    }

    #[test]
    fn build_claude_command_uses_resume_for_existing_sessions() {
        let command = build_claude_command("hello", "uuid-1", false);
        assert_eq!(
            command,
            vec![
                "claude".to_string(),
                "--print".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--add-dir".to_string(),
                "/workspace".to_string(),
                "/state".to_string(),
                "--resume".to_string(),
                "uuid-1".to_string(),
                "hello".to_string(),
            ]
        );
    }

    #[test]
    fn detects_missing_session_message_in_stdout() {
        let response = RunResponse {
            status: "ok".into(),
            stdout: "No conversation found with session ID: abc".into(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
            duration_ms: 0,
        };

        assert!(should_retry_with_fresh_session(&response, false));
    }

    #[test]
    fn does_not_retry_when_session_is_new() {
        let response = RunResponse {
            status: "ok".into(),
            stdout: "No conversation found with session ID: abc".into(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
            duration_ms: 0,
        };

        assert!(!should_retry_with_fresh_session(&response, true));
    }
}
