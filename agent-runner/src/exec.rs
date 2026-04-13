use std::time::Instant;

use async_trait::async_trait;
use tokio::time::{Duration, timeout};

use crate::docker_client::DockerRuntime;
use crate::models::{ErrorResponse, RunRequest, RunResponse, ValidatedRunRequest};

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
    container_name: String,
}

impl DockerRunner {
    pub fn new(runtime: DockerRuntime, container_name: String) -> Self {
        Self {
            runtime,
            container_name,
        }
    }
}

#[async_trait]
impl ExecutionBackend for DockerRunner {
    async fn execute(
        &self,
        request: RunRequest,
        validated: ValidatedRunRequest,
    ) -> Result<RunResponse, ErrorResponse> {
        self.runtime
            .ensure_container_running(&self.container_name)
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
            .create_claude_exec(&self.container_name, &validated.cwd, &request.prompt)
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
                        .kill_pid(&self.container_name, pid, "TERM")
                        .await;
                    let _ = self
                        .runtime
                        .kill_pid(&self.container_name, pid, "KILL")
                        .await;
                }
                let _ = self.runtime.kill_claude_execs(&self.container_name).await;

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
