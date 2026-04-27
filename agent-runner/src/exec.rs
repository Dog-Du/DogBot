use std::time::Instant;

use async_trait::async_trait;

use crate::config::Settings;
use crate::docker_client::{ContainerSpec, DockerRuntime};
use crate::history::store::{HistoryReadGrant, HistoryReadGrantToken, HistoryStore};
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
    history_store: HistoryStore,
    settings: Settings,
}

impl DockerRunner {
    pub fn new(runtime: DockerRuntime, settings: Settings) -> Result<Self, ErrorResponse> {
        let container_spec = ContainerSpec::from_settings(&settings);
        let session_store = SessionStore::open(&settings).map_err(|err| ErrorResponse {
            status: "error".into(),
            error_code: "session_store_unavailable".into(),
            message: err.to_string(),
            timed_out: false,
        })?;
        let history_store = HistoryStore::open(&settings).map_err(|err| ErrorResponse {
            status: "error".into(),
            error_code: "history_store_unavailable".into(),
            message: err.to_string(),
            timed_out: false,
        })?;

        Ok(Self {
            runtime,
            container_spec,
            session_store,
            history_store,
            settings,
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
        let first = self.execute_once(&request, &validated, &session).await?;

        if should_retry_with_fresh_session(&first, session.is_new) {
            let reset_session = reset_runtime_session(&self.session_store, &request)
                .map_err(map_session_store_error)?;

            let retried = self
                .execute_once(&request, &validated, &reset_session)
                .await?;
            return Ok(with_duration(retried, started.elapsed().as_millis()));
        }

        Ok(with_duration(first, started.elapsed().as_millis()))
    }
}

impl DockerRunner {
    async fn execute_once(
        &self,
        request: &RunRequest,
        validated: &ValidatedRunRequest,
        session: &crate::session_store::SessionRecord,
    ) -> Result<RunResponse, ErrorResponse> {
        let history_token = self.create_history_read_token(request)?;
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
                history_exec_env(&history_token),
            )
            .await
            .map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "exec_create_failed".into(),
                message: err.to_string(),
                timed_out: false,
            })?;

        let result = self.runtime.collect_exec_output(&exec.id).await;

        match result {
            Ok((stdout, stderr, exit_code)) => Ok(RunResponse {
                status: "ok".into(),
                stdout,
                stderr,
                exit_code,
                timed_out: false,
                duration_ms: 0,
            }),
            Err(err) => Err(ErrorResponse {
                status: "error".into(),
                error_code: "exec_failed".into(),
                message: err.to_string(),
                timed_out: false,
            }),
        }
    }

    fn create_history_read_token(
        &self,
        request: &RunRequest,
    ) -> Result<HistoryReadGrantToken, ErrorResponse> {
        let grants = history_read_grants_for_request(&self.settings, request);
        self.history_store
            .create_read_grants(grants)
            .map_err(|err| ErrorResponse {
                status: "error".into(),
                error_code: "history_grant_failed".into(),
                message: err.to_string(),
                timed_out: false,
            })
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
    if is_new_session || response.timed_out {
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

fn history_read_grants_for_request(
    settings: &Settings,
    request: &RunRequest,
) -> Vec<HistoryReadGrant> {
    let ttl_secs = settings.history_run_token_ttl_secs;
    if is_admin_private_history_request(settings, request) {
        return admin_history_platform_accounts(settings, request)
            .into_iter()
            .map(|platform_account| HistoryReadGrant {
                platform_account,
                conversation_id: None,
                actor_id: request.user_id.clone(),
                is_admin: true,
                ttl_secs,
            })
            .collect();
    }

    vec![HistoryReadGrant {
        platform_account: request.platform_account_id.clone(),
        conversation_id: Some(request.conversation_id.clone()),
        actor_id: request.user_id.clone(),
        is_admin: false,
        ttl_secs,
    }]
}

fn is_admin_private_history_request(settings: &Settings, request: &RunRequest) -> bool {
    request.chat_type == "private"
        && settings
            .admin_actor_ids
            .iter()
            .any(|actor_id| actor_id == &request.user_id)
}

fn admin_history_platform_accounts(settings: &Settings, request: &RunRequest) -> Vec<String> {
    let mut accounts = Vec::new();
    push_unique_trimmed(&mut accounts, &request.platform_account_id);
    if let Some(account) = settings.platform_qq_account_id.as_deref() {
        push_unique_trimmed(&mut accounts, account);
    }
    if let Some(account) = settings.platform_wechatpadpro_account_id.as_deref() {
        push_unique_trimmed(&mut accounts, account);
    }
    accounts
}

fn push_unique_trimmed(values: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if value.is_empty() || values.iter().any(|existing| existing == value) {
        return;
    }
    values.push(value.to_string());
}

fn history_exec_env(token: &HistoryReadGrantToken) -> Vec<String> {
    vec![
        format!("DOGBOT_HISTORY_DATABASE_URL={}", token.database_url),
        format!("DOGBOT_HISTORY_RUN_TOKEN={}", token.token),
        format!(
            "PGOPTIONS=-c dogbot.run_token={} -c statement_timeout=5000",
            token.token
        ),
    ]
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
        build_claude_command, history_exec_env, history_read_grants_for_request,
        load_runtime_session, reset_runtime_session, should_retry_with_fresh_session,
    };
    use crate::config::Settings;
    use crate::history::store::HistoryReadGrantToken;
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
            trigger_message_id: None,
            trigger_reply_to_message_id: None,
            mention_refs: Vec::new(),
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
    fn detects_missing_session_message_in_stderr_even_when_claude_exits_nonzero() {
        let response = RunResponse {
            status: "ok".into(),
            stdout: String::new(),
            stderr: "No conversation found with session ID: abc".into(),
            exit_code: 1,
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
    fn ordinary_history_grant_is_scoped_to_current_conversation() {
        let settings = Settings::from_env_map(std::collections::HashMap::new()).unwrap();
        let request = base_request();

        let grants = history_read_grants_for_request(&settings, &request);

        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].platform_account, request.platform_account_id);
        assert_eq!(grants[0].conversation_id, Some(request.conversation_id));
        assert_eq!(grants[0].actor_id, request.user_id);
        assert!(!grants[0].is_admin);
        assert_eq!(grants[0].ttl_secs, 1800);
    }

    #[test]
    fn admin_private_history_grants_cover_configured_platform_accounts() {
        let mut settings = Settings::from_env_map(std::collections::HashMap::from([
            (
                "DOGBOT_ADMIN_ACTOR_IDS".to_string(),
                "qq:user:admin".to_string(),
            ),
            (
                "PLATFORM_QQ_ACCOUNT_ID".to_string(),
                "qq:bot_uin:123".to_string(),
            ),
            (
                "PLATFORM_WECHATPADPRO_ACCOUNT_ID".to_string(),
                "wechatpadpro:account:bot".to_string(),
            ),
        ]))
        .unwrap();
        settings.history_run_token_ttl_secs = 1800;
        let mut request = base_request();
        request.chat_type = "private".into();
        request.user_id = "qq:user:admin".into();
        request.platform_account_id = "qq:bot_uin:999".into();

        let grants = history_read_grants_for_request(&settings, &request);
        let mut platform_accounts = grants
            .iter()
            .map(|grant| grant.platform_account.as_str())
            .collect::<Vec<_>>();
        platform_accounts.sort_unstable();

        assert_eq!(
            platform_accounts,
            vec![
                "qq:bot_uin:123",
                "qq:bot_uin:999",
                "wechatpadpro:account:bot"
            ]
        );
        assert!(grants.iter().all(|grant| grant.conversation_id.is_none()));
        assert!(grants.iter().all(|grant| grant.is_admin));
        assert!(grants.iter().all(|grant| grant.actor_id == "qq:user:admin"));
    }

    #[test]
    fn history_exec_env_contains_reader_url_token_and_pgoptions() {
        let token = HistoryReadGrantToken {
            token: "token-1".into(),
            database_url: "postgres://reader:pw@postgres:5432/dogbot".into(),
        };

        let env = history_exec_env(&token);

        assert_eq!(
            env,
            vec![
                "DOGBOT_HISTORY_DATABASE_URL=postgres://reader:pw@postgres:5432/dogbot",
                "DOGBOT_HISTORY_RUN_TOKEN=token-1",
                "PGOPTIONS=-c dogbot.run_token=token-1 -c statement_timeout=5000",
            ]
        );
    }

    #[test]
    fn runtime_session_path_uses_platform_account_and_conversation_identity() {
        let Some(scope) = PostgresSessionScope::new() else {
            return;
        };
        let store = &scope.store;

        let first = load_runtime_session(store, &scope.request()).unwrap();

        let mut same_conversation_different_legacy = scope.request();
        same_conversation_different_legacy.session_id = scope.external("legacy-session-2");
        same_conversation_different_legacy.user_id = scope.external("qq:user:2");
        let second = load_runtime_session(store, &same_conversation_different_legacy).unwrap();

        let mut different_platform_account = scope.request();
        different_platform_account.platform_account_id = scope.account("qq:bot_uin:999");
        different_platform_account.session_id = scope.external("legacy-session-3");
        let third = load_runtime_session(store, &different_platform_account).unwrap();

        let mut different_conversation = scope.request();
        different_conversation.conversation_id = "qq:group:7788".into();
        different_conversation.session_id = scope.external("legacy-session-4");
        let fourth = load_runtime_session(store, &different_conversation).unwrap();

        assert_eq!(first.claude_session_id, second.claude_session_id);
        assert_ne!(first.claude_session_id, third.claude_session_id);
        assert_ne!(first.claude_session_id, fourth.claude_session_id);
    }

    #[test]
    fn runtime_session_path_binds_external_session_alias() {
        let Some(scope) = PostgresSessionScope::new() else {
            return;
        };
        let request = scope.request();

        let session = load_runtime_session(&scope.store, &request).unwrap();
        let fetched = scope
            .store
            .get_session(&request.session_id)
            .unwrap()
            .unwrap();

        assert_eq!(fetched.session_key, session.session_key);
        assert_eq!(fetched.claude_session_id, session.claude_session_id);
    }

    #[test]
    fn runtime_session_reset_keeps_canonical_identity() {
        let Some(scope) = PostgresSessionScope::new() else {
            return;
        };
        let request = scope.request();

        let first = load_runtime_session(&scope.store, &request).unwrap();
        let reset = reset_runtime_session(&scope.store, &request).unwrap();
        let fetched = scope
            .store
            .get_session(&request.session_id)
            .unwrap()
            .unwrap();

        assert_ne!(first.claude_session_id, reset.claude_session_id);
        assert_eq!(reset.claude_session_id, fetched.claude_session_id);
    }

    #[test]
    fn runtime_session_reset_rejects_conflicting_alias_without_rotating_session() {
        let Some(scope) = PostgresSessionScope::new() else {
            return;
        };
        let store = &scope.store;

        let mut conflicting_request = scope.request();
        conflicting_request.conversation_id = "qq:group:7788".into();
        store
            .get_or_create_bound_session(
                &conflicting_request.session_id,
                &conflicting_request.platform,
                &conflicting_request.platform_account_id,
                &conflicting_request.conversation_id,
            )
            .unwrap();

        let request = scope.request();
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

    struct PostgresSessionScope {
        store: SessionStore,
        suffix: String,
    }

    impl PostgresSessionScope {
        fn new() -> Option<Self> {
            let Some(url) = std::env::var("DOGBOT_TEST_DATABASE_URL").ok() else {
                eprintln!("DOGBOT_TEST_DATABASE_URL unset; skipping postgres integration test");
                return None;
            };
            let store = SessionStore::open_database_url(url).unwrap();
            store.initialize_schema().unwrap();
            Some(Self {
                store,
                suffix: uuid::Uuid::new_v4().to_string(),
            })
        }

        fn request(&self) -> RunRequest {
            let mut request = base_request();
            request.platform_account_id = self.account(&request.platform_account_id);
            request.session_id = self.external(&request.session_id);
            request.user_id = self.external(&request.user_id);
            request
        }

        fn external(&self, value: &str) -> String {
            format!("{value}:{}", self.suffix)
        }

        fn account(&self, value: &str) -> String {
            format!("{value}:test:{}", self.suffix)
        }
    }
}
