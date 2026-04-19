use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryIntent {
    pub scope: String,
    pub summary: String,
    pub raw_evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapturedMemoryIntent {
    pub raw_json: String,
    pub intent: MemoryIntent,
}

pub fn extract_memory_intent_json(stdout: &str) -> Option<String> {
    // Extract the first fenced block:
    //
    // ```dogbot-memory
    // { ...json... }
    // ```
    //
    // We deliberately keep this permissive: if parsing fails, we return None rather than
    // failing the run.
    let mut in_block = false;
    let mut payload = String::new();

    for line in stdout.lines() {
        let trimmed = line.trim();

        if !in_block {
            if trimmed.starts_with("```dogbot-memory") {
                in_block = true;
            }
            continue;
        }

        if trimmed == "```" {
            break;
        }

        payload.push_str(line);
        payload.push('\n');
    }

    if !in_block {
        return None;
    }

    let payload = payload.trim();
    if payload.is_empty() {
        return None;
    }

    Some(payload.to_string())
}

pub fn parse_memory_intent(stdout: &str) -> Option<MemoryIntent> {
    capture_memory_intent(stdout).map(|captured| captured.intent)
}

pub(crate) fn capture_memory_intent(stdout: &str) -> Option<CapturedMemoryIntent> {
    let raw_json = extract_memory_intent_json(stdout)?;
    let intent = serde_json::from_str(&raw_json).ok()?;
    Some(CapturedMemoryIntent { raw_json, intent })
}
