use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    pub platform: String,
    pub conversation_id: String,
    pub session_id: String,
    pub user_id: String,
    pub chat_type: String,
    pub cwd: String,
    pub prompt: String,
    pub timeout_secs: Option<u64>,
}

impl RunRequest {
    const ALLOWED_PREFIXES: [&'static str; 2] = ["workspace", "state"];

    pub fn effective_timeout(&self, default_timeout: u64, max_timeout: u64) -> Result<u64, String> {
        let timeout = self.timeout_secs.unwrap_or(default_timeout);
        if timeout > max_timeout {
            return Err("timeout exceeds configured max".to_string());
        }
        Ok(timeout)
    }

    pub fn validate(
        &self,
        default_timeout: u64,
        max_timeout: u64,
    ) -> Result<ValidatedRunRequest, String> {
        let timeout_secs = self.effective_timeout(default_timeout, max_timeout)?;
        let cwd = Self::validate_cwd(&self.cwd)?;
        Ok(ValidatedRunRequest { timeout_secs, cwd })
    }

    fn validate_cwd(cwd: &str) -> Result<String, String> {
        if cwd.is_empty() {
            return Err("cwd must be provided".into());
        }

        let path = Path::new(cwd);
        if !path.is_absolute() {
            return Err(format!("cwd {cwd} must be absolute"));
        }

        let mut components = path.components();
        match components.next() {
            Some(Component::RootDir) => {}
            _ => return Err(format!("cwd {cwd} must start with a root '/'.", cwd = cwd)),
        }

        let prefix = match components.next() {
            Some(Component::Normal(seg)) => seg,
            Some(Component::ParentDir) => {
                return Err(format!("cwd {cwd} contains traversal segments."));
            }
            _ => {
                return Err(format!(
                    "cwd {cwd} does not contain a valid workspace/state prefix."
                ));
            }
        };

        let prefix_str = prefix
            .to_str()
            .ok_or_else(|| format!("cwd {cwd} contains invalid characters."))?;

        if !Self::ALLOWED_PREFIXES.contains(&prefix_str) {
            return Err(format!(
                "cwd {cwd} is not within allowed prefixes: {}",
                Self::ALLOWED_PREFIXES.join(", ")
            ));
        }

        for component in components {
            if matches!(component, Component::ParentDir) {
                return Err(format!("cwd {cwd} contains traversal segments."));
            }
        }

        Ok(cwd.to_string())
    }
}

#[derive(Debug)]
pub struct ValidatedRunRequest {
    pub timeout_secs: u64,
    pub cwd: String,
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
    pub timed_out: bool,
    pub duration_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub status: String,
    pub error_code: String,
    pub message: String,
    pub timed_out: bool,
}
