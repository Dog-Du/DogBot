#[derive(Debug, Clone)]
pub struct SystemPromptContext {
    pub platform: String,
    pub platform_account: String,
}

impl SystemPromptContext {
    pub fn render(&self) -> String {
        format!(
            concat!(
                "Current platform: {}\n",
                "Current platform account: {}\n\n",
                "Before replying, read and follow /state/claude-prompt/CLAUDE.md.\n",
                "Before composing any DogBot reply body or dogbot-action block, MUST read and follow /state/claude-prompt/skills/reply-format/SKILL.md.\n",
                "Reply using plain text plus optional ```dogbot-action``` JSON blocks only.\n",
                "Do not use Markdown in outbound social-platform messages.\n",
                "Do not emit QQ, WeChat, or other platform-private syntax directly.\n",
                "When sending media, only reference files that already exist under /workspace."
            ),
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
        let context = json!({
            "conversation": self.conversation,
            "actor": self.actor,
            "trigger_summary": self.trigger_summary,
            "reply_excerpt": self.reply_excerpt,
        });
        format!("Turn context (JSON):\n{context}")
    }

    pub fn render_with_user_prompt(&self, user_prompt: &str) -> String {
        format!("{}\n\nUser prompt:\n{}", self.render(), user_prompt.trim())
    }
}
use serde_json::json;
