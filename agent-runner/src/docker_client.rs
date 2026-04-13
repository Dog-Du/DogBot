use bollard::Docker;
use bollard::container::{InspectContainerOptions, LogOutput, StartContainerOptions};
use bollard::errors::Error as BollardError;
use bollard::exec::{CreateExecOptions, CreateExecResults, StartExecResults};
use futures_util::StreamExt;

#[derive(Clone)]
pub struct DockerRuntime {
    docker: Docker,
}

impl DockerRuntime {
    pub fn connect() -> Result<Self, BollardError> {
        Ok(Self {
            docker: Docker::connect_with_local_defaults()?,
        })
    }

    pub async fn ensure_container_running(&self, container_name: &str) -> Result<(), BollardError> {
        let container = self
            .docker
            .inspect_container(container_name, None::<InspectContainerOptions>)
            .await?;

        let is_running = container
            .state
            .and_then(|state| state.running)
            .unwrap_or(false);

        if !is_running {
            self.docker
                .start_container(container_name, None::<StartContainerOptions<String>>)
                .await?;
        }

        Ok(())
    }

    pub async fn create_claude_exec(
        &self,
        container_name: &str,
        cwd: &str,
        prompt: &str,
    ) -> Result<CreateExecResults, BollardError> {
        self.docker
            .create_exec(
                container_name,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(vec![
                        "claude".to_string(),
                        "-p".to_string(),
                        prompt.to_string(),
                    ]),
                    working_dir: Some(cwd.to_string()),
                    ..Default::default()
                },
            )
            .await
    }

    pub async fn collect_exec_output(
        &self,
        exec_id: &str,
    ) -> Result<(String, String, i64), BollardError> {
        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } =
            self.docker.start_exec(exec_id, None).await?
        {
            while let Some(next) = output.next().await {
                match next? {
                    LogOutput::StdOut { message } | LogOutput::Console { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    LogOutput::StdErr { message } => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    LogOutput::StdIn { .. } => {}
                }
            }
        }

        let exit_code = self
            .docker
            .inspect_exec(exec_id)
            .await?
            .exit_code
            .unwrap_or_default();

        Ok((stdout, stderr, exit_code))
    }

    pub async fn exec_pid(&self, exec_id: &str) -> Result<Option<i64>, BollardError> {
        Ok(self.docker.inspect_exec(exec_id).await?.pid)
    }

    pub async fn kill_pid(
        &self,
        container_name: &str,
        pid: i64,
        signal: &str,
    ) -> Result<(), BollardError> {
        let exec = self
            .docker
            .create_exec(
                container_name,
                CreateExecOptions {
                    attach_stdout: Some(false),
                    attach_stderr: Some(false),
                    cmd: Some(vec![
                        "sh".to_string(),
                        "-lc".to_string(),
                        format!("kill -{signal} {pid}"),
                    ]),
                    ..Default::default()
                },
            )
            .await?;

        let _ = self.docker.start_exec(&exec.id, None).await?;
        Ok(())
    }

    pub async fn kill_claude_execs(&self, container_name: &str) -> Result<(), BollardError> {
        let exec = self
            .docker
            .create_exec(
                container_name,
                CreateExecOptions {
                    attach_stdout: Some(false),
                    attach_stderr: Some(false),
                    cmd: Some(vec![
                        "sh".to_string(),
                        "-lc".to_string(),
                        "pkill -TERM -f 'claude -p' || true; pkill -KILL -f 'claude -p' || true"
                            .to_string(),
                    ]),
                    ..Default::default()
                },
            )
            .await?;

        let _ = self.docker.start_exec(&exec.id, None).await?;
        Ok(())
    }
}
