use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionRef {
    pub ref_id: String,
    pub actor_id: String,
    pub display: String,
}

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
                "Before replying, read and follow /state/claude-prompt/CLAUDE.md to initialize the conversation.\n",
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
    pub trigger_message_id: Option<String>,
    pub trigger_reply_to_message_id: Option<String>,
    pub trigger_summary: String,
    pub mention_refs: Vec<MentionRef>,
    pub reply_excerpt: Option<String>,
}

impl TurnPromptContext {
    pub fn render(&self) -> String {
        let context = json!({
            "conversation": self.conversation,
            "actor": self.actor,
            "trigger_message_id": self.trigger_message_id,
            "trigger_reply_to_message_id": self.trigger_reply_to_message_id,
            "trigger_summary": self.trigger_summary,
            "mention_refs": self.mention_refs,
            "reply_excerpt": self.reply_excerpt,
        });
        format!("Turn context (JSON):\n{context}")
    }

    pub fn render_with_user_prompt(&self, user_prompt: &str) -> String {
        format!("{}\n\nUser prompt:\n{}", self.render(), user_prompt.trim())
    }
}
