use std::time::Instant;

use async_trait::async_trait;
use tokio::time::{Duration, timeout};

use crate::config::Settings;
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
}

impl DockerRunner {
    pub fn new(runtime: DockerRuntime, settings: Settings) -> Result<Self, ErrorResponse> {
        let container_spec = ContainerSpec::from_settings(&settings);
        let session_store = SessionStore::open(&settings.session_db_path).map_err(|err| {
            ErrorResponse {
                status: "error".into(),
                error_code: "session_store_unavailable".into(),
                message: err.to_string(),
                timed_out: false,
            }
        })?;

        Ok(Self {
            runtime,
            container_spec,
            session_store,
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
        let exec = self
            .runtime
            .create_claude_exec(
                &self.container_spec.container_name,
                &validated.cwd,
                build_claude_command(&request.prompt, &session.claude_session_id, session.is_new),
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
            Ok(Ok((stdout, stderr, exit_code))) => Ok(RunResponse {
                status: "ok".into(),
                stdout,
                stderr,
                exit_code,
                timed_out: false,
                duration_ms: started.elapsed().as_millis(),
            }),
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

fn build_claude_command(
    prompt: &str,
    claude_session_id: &str,
    is_new_session: bool,
) -> Vec<String> {
    let mut command = vec!["claude".to_string(), "--print".to_string()];

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
    use super::build_claude_command;

    #[test]
    fn build_claude_command_uses_session_id_for_new_sessions() {
        let command = build_claude_command("hello", "uuid-1", true);
        assert_eq!(
            command,
            vec![
                "claude".to_string(),
                "--print".to_string(),
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
                "--resume".to_string(),
                "uuid-1".to_string(),
                "hello".to_string(),
            ]
        );
    }
}
