use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    const ALLOWED_PREFIXES: [&'static str; 2] = ["/workspace", "/state"];

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

        if !cwd.starts_with('/') {
            return Err(format!("cwd {cwd} must be absolute"));
        }

        for allowed in Self::ALLOWED_PREFIXES {
            if cwd == allowed {
                return Ok(cwd.to_string());
            }
        }

        Err(format!(
            "cwd {cwd} is not an approved root; allowed values are {}",
            Self::ALLOWED_PREFIXES.join(", ")
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedRunRequest {
    pub timeout_secs: u64,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunResponse {
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
    pub timed_out: bool,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub status: String,
    pub error_code: String,
    pub message: String,
    pub timed_out: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MessageRequest {
    pub session_id: String,
    pub text: String,
    pub reply_to_message_id: Option<String>,
    pub mention_user_id: Option<String>,
}

impl MessageRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.session_id.trim().is_empty() {
            return Err("session_id must be provided".into());
        }

        if self.text.trim().is_empty() {
            return Err("text must be provided".into());
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MessageResponse {
    pub status: String,
    pub message_id: Option<String>,
}
