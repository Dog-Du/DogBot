use serde::{Deserialize, Serialize};

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
    const ALLOWED_CWD_PREFIXES: [&'static str; 2] = ["/workspace", "/state"];

    pub fn effective_timeout(&self, default_timeout: u64, max_timeout: u64) -> Result<u64, String> {
        let timeout = self.timeout_secs.unwrap_or(default_timeout);
        if timeout > max_timeout {
            return Err("timeout exceeds configured max".to_string());
        }
        Ok(timeout)
    }

    pub fn validate_cwd(&self) -> Result<(), String> {
        if Self::ALLOWED_CWD_PREFIXES
            .iter()
            .any(|prefix| self.cwd.starts_with(prefix))
        {
            return Ok(());
        }

        let allowed = Self::ALLOWED_CWD_PREFIXES.join(", ");
        Err(format!(
            "cwd {cwd} is not within allowed prefixes ({allowed})",
            cwd = self.cwd,
            allowed = allowed
        ))
    }
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
