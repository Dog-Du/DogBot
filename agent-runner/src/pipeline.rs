#[derive(Debug, Clone)]
pub struct SystemPromptContext {
    pub platform: String,
    pub platform_account: String,
}

impl SystemPromptContext {
    pub fn render(&self) -> String {
        format!(
            "Current platform: {}\nCurrent platform account: {}",
            self.platform, self.platform_account
        )
    }
}

#[derive(Debug, Clone)]
pub struct TurnPromptContext {
    pub conversation: String,
    pub actor: String,
    pub trigger_summary: String,
    pub reply_excerpt: Option<String>,
}

impl TurnPromptContext {
    pub fn render(&self) -> String {
        let reply_excerpt = self.reply_excerpt.as_deref().unwrap_or("");
        format!(
            "Conversation: {}\nActor: {}\nTrigger message: {}\nReply excerpt: {}",
            self.conversation, self.actor, self.trigger_summary, reply_excerpt
        )
    }

    pub fn render_with_user_prompt(&self, user_prompt: &str) -> String {
        format!("{}\n\nUser prompt: {}", self.render(), user_prompt.trim())
    }
}
