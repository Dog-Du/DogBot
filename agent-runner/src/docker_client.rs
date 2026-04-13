use std::collections::HashMap;

use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, LogOutput, StartContainerOptions,
};
use bollard::errors::Error as BollardError;
use bollard::exec::{CreateExecOptions, CreateExecResults, StartExecResults};
use bollard::models::HostConfig;
use futures_util::StreamExt;

use crate::config::Settings;

#[derive(Clone)]
pub struct DockerRuntime {
    docker: Docker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSpec {
    pub container_name: String,
    pub image_name: String,
    pub workspace_dir: String,
    pub state_dir: String,
    pub anthropic_base_url: String,
    pub api_proxy_auth_token: String,
    pub cpu_cores: u64,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub pids_limit: i64,
}

impl ContainerSpec {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            container_name: settings.container_name.clone(),
            image_name: settings.image_name.clone(),
            workspace_dir: settings.workspace_dir.clone(),
            state_dir: settings.state_dir.clone(),
            anthropic_base_url: settings.anthropic_base_url.clone(),
            api_proxy_auth_token: settings.api_proxy_auth_token.clone(),
            cpu_cores: settings.container_cpu_cores,
            memory_mb: settings.container_memory_mb,
            disk_gb: settings.container_disk_gb,
            pids_limit: settings.container_pids_limit,
        }
    }

    pub fn create_config(&self) -> Config<String> {
        let memory_bytes = (self.memory_mb as i64) * 1024 * 1024;
        let nano_cpus = (self.cpu_cores as i64) * 1_000_000_000;
        let mut tmpfs = HashMap::new();
        tmpfs.insert("/tmp".to_string(), "size=256m,mode=1777".to_string());
        tmpfs.insert("/run".to_string(), "size=64m".to_string());

        let env = vec![
            format!("ANTHROPIC_BASE_URL={}", self.anthropic_base_url),
            format!("ANTHROPIC_AUTH_TOKEN={}", self.api_proxy_auth_token),
            "CLAUDE_CONFIG_DIR=/state/claude".to_string(),
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1".to_string(),
            "CLAUDE_CODE_DISABLE_TERMINAL_TITLE=1".to_string(),
            "CLAUDE_CODE_ATTRIBUTION_HEADER=0".to_string(),
        ];

        Config {
            image: Some(self.image_name.clone()),
            env: Some(env),
            working_dir: Some("/workspace".to_string()),
            host_config: Some(HostConfig {
                nano_cpus: Some(nano_cpus),
                memory: Some(memory_bytes),
                memory_swap: Some(memory_bytes),
                pids_limit: Some(self.pids_limit),
                readonly_rootfs: Some(true),
                binds: Some(vec![
                    format!("{}:/workspace", self.workspace_dir),
                    format!("{}:/state", self.state_dir),
                ]),
                tmpfs: Some(tmpfs),
                extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

impl DockerRuntime {
    pub fn connect() -> Result<Self, BollardError> {
        Ok(Self {
            docker: Docker::connect_with_local_defaults()?,
        })
    }

    pub async fn ensure_container_running(&self, spec: &ContainerSpec) -> Result<(), BollardError> {
        match self
            .docker
            .inspect_container(&spec.container_name, None::<InspectContainerOptions>)
            .await
        {
            Ok(container) => {
                let is_running = container
                    .state
                    .and_then(|state| state.running)
                    .unwrap_or(false);

                if !is_running {
                    self.docker
                        .start_container(
                            &spec.container_name,
                            None::<StartContainerOptions<String>>,
                        )
                        .await?;
                }

                Ok(())
            }
            Err(BollardError::DockerResponseServerError {
                status_code: 404, ..
            }) => {
                match self
                    .docker
                    .create_container(
                        Some(CreateContainerOptions {
                            name: spec.container_name.clone(),
                            platform: None,
                        }),
                        spec.create_config(),
                    )
                    .await
                {
                    Ok(_) => {}
                    Err(BollardError::DockerResponseServerError {
                        status_code: 409, ..
                    }) => {}
                    Err(err) => return Err(err),
                }
                self.docker
                    .start_container(&spec.container_name, None::<StartContainerOptions<String>>)
                    .await?;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    pub async fn create_claude_exec(
        &self,
        container_name: &str,
        cwd: &str,
        command: Vec<String>,
    ) -> Result<CreateExecResults, BollardError> {
        self.docker
            .create_exec(
                container_name,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(command),
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
                        "pkill -TERM -f 'claude --print' || true; pkill -KILL -f 'claude --print' || true"
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
