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
        let session_store =
            SessionStore::open(&settings.session_db_path).map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "session_store_unavailable".into(),
                message: err.to_string(),
                timed_out: false,
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
        let session =
            load_runtime_session(&self.session_store, &request).map_err(map_session_store_error)?;

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
        let first = self.execute_once(&validated, &session).await?;

        if should_retry_with_fresh_session(&first, session.is_new) {
            let reset_session = reset_runtime_session(&self.session_store, &request)
                .map_err(map_session_store_error)?;

            let retried = self.execute_once(&validated, &reset_session).await?;
            return Ok(with_duration(retried, started.elapsed().as_millis()));
        }

        Ok(with_duration(first, started.elapsed().as_millis()))
    }
}

impl DockerRunner {
    async fn execute_once(
        &self,
        validated: &ValidatedRunRequest,
        session: &crate::session_store::SessionRecord,
    ) -> Result<RunResponse, ErrorResponse> {
        let exec = self
            .runtime
            .create_claude_exec(
                &self.container_spec.container_name,
                &validated.cwd,
                build_claude_command(
                    &validated.prompt,
                    &validated.system_prompt,
                    &session.claude_session_id,
                    session.is_new,
                ),
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
                duration_ms: 0,
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

fn with_duration(mut response: RunResponse, duration_ms: u128) -> RunResponse {
    response.duration_ms = duration_ms;
    response
}

fn load_runtime_session(
    session_store: &SessionStore,
    request: &RunRequest,
) -> Result<crate::session_store::SessionRecord, SessionStoreError> {
    if !request.session_id.trim().is_empty() {
        session_store.validate_external_session_binding(
            &request.session_id,
            &request.platform,
            &request.platform_account_id,
            &request.conversation_id,
        )?;
    }

    let session = session_store.get_or_create_conversation_session(
        &request.platform,
        &request.platform_account_id,
        &request.conversation_id,
    )?;

    if !request.session_id.trim().is_empty() {
        session_store.bind_external_session_id(
            &request.session_id,
            &request.platform,
            &request.platform_account_id,
            &request.conversation_id,
        )?;
    }

    Ok(session)
}

fn reset_runtime_session(
    session_store: &SessionStore,
    request: &RunRequest,
) -> Result<crate::session_store::SessionRecord, SessionStoreError> {
    if !request.session_id.trim().is_empty() {
        session_store.validate_external_session_binding(
            &request.session_id,
            &request.platform,
            &request.platform_account_id,
            &request.conversation_id,
        )?;
    }

    let session = session_store.reset_conversation_session(
        &request.platform,
        &request.platform_account_id,
        &request.conversation_id,
    )?;

    if !request.session_id.trim().is_empty() {
        session_store.bind_external_session_id(
            &request.session_id,
            &request.platform,
            &request.platform_account_id,
            &request.conversation_id,
        )?;
    }

    Ok(session)
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
    system_prompt: &str,
    claude_session_id: &str,
    is_new_session: bool,
) -> Vec<String> {
    let mut command = vec![
        "claude".to_string(),
        "--print".to_string(),
        "--dangerously-skip-permissions".to_string(),
        "--append-system-prompt".to_string(),
        system_prompt.to_string(),
        "--add-dir".to_string(),
        "/workspace".to_string(),
        "/state".to_string(),
        "/state/claude-prompt".to_string(),
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
    use super::{
        build_claude_command, load_runtime_session, reset_runtime_session,
        should_retry_with_fresh_session,
    };
    use crate::models::{RunRequest, RunResponse};
    use crate::session_store::SessionStore;

    fn base_request() -> RunRequest {
        RunRequest {
            platform: "qq".into(),
            platform_account_id: "qq:bot_uin:123".into(),
            conversation_id: "qq:group:5566".into(),
            session_id: "legacy-session-1".into(),
            user_id: "qq:user:1".into(),
            chat_type: "group".into(),
            cwd: "/workspace".into(),
            prompt: "hello".into(),
            trigger_summary: Some("hello".into()),
            reply_excerpt: None,
            timeout_secs: None,
        }
    }

    #[test]
    fn build_claude_command_uses_session_id_for_new_sessions() {
        let command = build_claude_command("hello", "system", "uuid-1", true);
        assert_eq!(
            command,
            vec![
                "claude".to_string(),
                "--print".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--append-system-prompt".to_string(),
                "system".to_string(),
                "--add-dir".to_string(),
                "/workspace".to_string(),
                "/state".to_string(),
                "/state/claude-prompt".to_string(),
                "--session-id".to_string(),
                "uuid-1".to_string(),
                "hello".to_string(),
            ]
        );
    }

    #[test]
    fn build_claude_command_uses_resume_for_existing_sessions() {
        let command = build_claude_command("hello", "system", "uuid-1", false);
        assert_eq!(
            command,
            vec![
                "claude".to_string(),
                "--print".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--append-system-prompt".to_string(),
                "system".to_string(),
                "--add-dir".to_string(),
                "/workspace".to_string(),
                "/state".to_string(),
                "/state/claude-prompt".to_string(),
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

    #[test]
    fn runtime_session_path_uses_platform_account_and_conversation_identity() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::open(temp.path().join("runner.db")).unwrap();

        let first = load_runtime_session(&store, &base_request()).unwrap();

        let mut same_conversation_different_legacy = base_request();
        same_conversation_different_legacy.session_id = "legacy-session-2".into();
        same_conversation_different_legacy.user_id = "qq:user:2".into();
        let second = load_runtime_session(&store, &same_conversation_different_legacy).unwrap();

        let mut different_platform_account = base_request();
        different_platform_account.platform_account_id = "qq:bot_uin:999".into();
        different_platform_account.session_id = "legacy-session-3".into();
        let third = load_runtime_session(&store, &different_platform_account).unwrap();

        let mut different_conversation = base_request();
        different_conversation.conversation_id = "qq:group:7788".into();
        different_conversation.session_id = "legacy-session-4".into();
        let fourth = load_runtime_session(&store, &different_conversation).unwrap();

        assert_eq!(first.claude_session_id, second.claude_session_id);
        assert_ne!(first.claude_session_id, third.claude_session_id);
        assert_ne!(first.claude_session_id, fourth.claude_session_id);
    }

    #[test]
    fn runtime_session_path_binds_external_session_alias() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::open(temp.path().join("runner.db")).unwrap();
        let request = base_request();

        let session = load_runtime_session(&store, &request).unwrap();
        let fetched = store.get_session(&request.session_id).unwrap().unwrap();

        assert_eq!(fetched.session_key, session.session_key);
        assert_eq!(fetched.claude_session_id, session.claude_session_id);
    }

    #[test]
    fn runtime_session_reset_keeps_canonical_identity() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::open(temp.path().join("runner.db")).unwrap();
        let request = base_request();

        let first = load_runtime_session(&store, &request).unwrap();
        let reset = reset_runtime_session(&store, &request).unwrap();
        let fetched = store.get_session(&request.session_id).unwrap().unwrap();

        assert_ne!(first.claude_session_id, reset.claude_session_id);
        assert_eq!(reset.claude_session_id, fetched.claude_session_id);
    }

    #[test]
    fn runtime_session_reset_rejects_conflicting_alias_without_rotating_session() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::open(temp.path().join("runner.db")).unwrap();

        let mut conflicting_request = base_request();
        conflicting_request.conversation_id = "qq:group:7788".into();
        store
            .get_or_create_bound_session(
                &conflicting_request.session_id,
                &conflicting_request.platform,
                &conflicting_request.platform_account_id,
                &conflicting_request.conversation_id,
            )
            .unwrap();

        let request = base_request();
        let original = store
            .get_or_create_conversation_session(
                &request.platform,
                &request.platform_account_id,
                &request.conversation_id,
            )
            .unwrap();

        let err = reset_runtime_session(&store, &request).unwrap_err();
        assert!(matches!(
            err,
            crate::session_store::SessionStoreError::SessionConflict { .. }
        ));

        let current = store
            .get_or_create_conversation_session(
                &request.platform,
                &request.platform_account_id,
                &request.conversation_id,
            )
            .unwrap();
        assert_eq!(current.claude_session_id, original.claude_session_id);
    }
}
