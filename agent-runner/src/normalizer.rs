use serde::{Deserialize, Deserializer};
use serde_json::Value;
use tracing::warn;

use crate::protocol::{
    AssetRef, AssetSource, MessagePart, OutboundAction, OutboundMessage, OutboundPlan,
    ReactionAction,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionEnvelope {
    #[serde(default)]
    reply_to: ReplyToDirective,
    #[serde(default)]
    mentions: Vec<MentionSpec>,
    #[serde(default)]
    actions: Vec<ActionItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionItem {
    #[serde(rename = "type")]
    action_type: String,
    target_message_id: Option<String>,
    emoji: Option<String>,
    #[serde(default)]
    reply_to: ReplyToDirective,
    source_type: Option<String>,
    source_value: Option<String>,
    caption_text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct MentionSpec {
    actor_id: String,
    display: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ActionBlock {
    Envelope(ActionEnvelope),
    Single(ActionItem),
}

#[derive(Debug, Clone, Default)]
enum ReplyToDirective {
    #[default]
    Missing,
    Null,
    Value(String),
}

impl<'de> Deserialize<'de> for ReplyToDirective {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(match value {
            Value::Null => Self::Null,
            Value::String(text) => Self::Value(text),
            _ => {
                return Err(serde::de::Error::custom(
                    "reply_to must be a string or null",
                ));
            }
        })
    }
}

pub fn normalize_agent_output(output: &str) -> Result<OutboundPlan, serde_json::Error> {
    let normalized = output.replace("\r\n", "\n");
    let mut body = String::new();
    let mut body_reply_to = None;
    let mut suppress_body_default_reply = false;
    let mut body_mentions = Vec::new();
    let mut actions = Vec::new();
    let mut messages = Vec::new();
    let mut remaining = normalized.as_str();

    while let Some(index) = remaining.find("```dogbot-action") {
        body.push_str(&remaining[..index]);
        let block = &remaining[index + "```dogbot-action".len()..];
        let block = block.trim_start_matches(|ch: char| ch.is_whitespace());

        let Some((json_block, rest)) = block.split_once("\n```") else {
            break;
        };

        match serde_json::from_str::<ActionBlock>(json_block.trim()) {
            Ok(ActionBlock::Envelope(envelope)) => {
                if let Some((reply_to, suppress_default_reply)) =
                    parse_reply_to_directive(&envelope.reply_to)
                {
                    body_reply_to = reply_to;
                    suppress_body_default_reply = suppress_default_reply;
                }
                body_mentions.extend(envelope.mentions);
                append_action_items(&mut messages, &mut actions, envelope.actions);
            }
            Ok(ActionBlock::Single(item)) => {
                append_action_items(&mut messages, &mut actions, vec![item])
            }
            Err(err) => warn!("failed to parse dogbot-action block: {err}"),
        }

        remaining = rest;
    }

    body.push_str(remaining);

    let body = degrade_markdown(body.trim());
    if !body.is_empty() || !body_mentions.is_empty() {
        let mut message = OutboundMessage {
            parts: body_mentions
                .into_iter()
                .map(|mention| MessagePart::Mention {
                    actor_id: mention.actor_id,
                    display: mention.display,
                })
                .collect(),
            reply_to: None,
            suppress_default_reply: false,
            delivery_policy: None,
        };
        if !body.is_empty() {
            message.parts.push(MessagePart::Text {
                text: body.to_string(),
            });
        }
        message.reply_to = body_reply_to;
        message.suppress_default_reply = suppress_body_default_reply;
        messages.insert(0, message);
    }

    Ok(OutboundPlan {
        messages,
        actions,
        delivery_report_policy: None,
    })
}

fn append_action_items(
    messages: &mut Vec<OutboundMessage>,
    actions: &mut Vec<OutboundAction>,
    items: Vec<ActionItem>,
) {
    for item in items {
        if item.action_type == "reaction_add" || item.action_type == "reaction_remove" {
            let Some(target_message_id) = item.target_message_id.filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Some(emoji) = item.emoji.filter(|value| !value.is_empty()) else {
                continue;
            };

            let action = ReactionAction {
                target_message_id,
                emoji,
            };
            actions.push(if item.action_type == "reaction_add" {
                OutboundAction::ReactionAdd(action)
            } else {
                OutboundAction::ReactionRemove(action)
            });
            continue;
        }

        let Some(message) = media_action_to_message(&item) else {
            continue;
        };
        messages.push(message);
    }
}

fn media_action_to_message(item: &ActionItem) -> Option<OutboundMessage> {
    let part_kind = match item.action_type.as_str() {
        "send_image" => "image",
        "send_file" => "file",
        "send_voice" => "voice",
        "send_video" => "video",
        "send_sticker" => "sticker",
        _ => return None,
    };
    let source = match item.source_type.as_deref()? {
        "workspace_path" => AssetSource::WorkspacePath(item.source_value.clone()?),
        "stored_asset" => AssetSource::ManagedStore(item.source_value.clone()?),
        "remote_url" => AssetSource::ExternalUrl(item.source_value.clone()?),
        "platform_native_handle" => AssetSource::PlatformNativeHandle(item.source_value.clone()?),
        "bridge_handle" => AssetSource::BridgeHandle(item.source_value.clone()?),
        _ => return None,
    };

    let asset = AssetRef {
        asset_id: format!(
            "{part_kind}:{}",
            item.source_value.as_deref().unwrap_or_default()
        ),
        kind: part_kind.to_string(),
        mime: default_mime(part_kind).to_string(),
        size_bytes: 0,
        source,
    };

    let mut parts = vec![match part_kind {
        "image" => MessagePart::Image { asset },
        "file" => MessagePart::File { asset },
        "voice" => MessagePart::Voice { asset },
        "video" => MessagePart::Video { asset },
        "sticker" => MessagePart::Sticker { asset },
        _ => return None,
    }];

    if let Some(caption_text) = item.caption_text.as_deref().map(str::trim)
        && !caption_text.is_empty()
    {
        parts.push(MessagePart::Text {
            text: caption_text.to_string(),
        });
    }

    Some(OutboundMessage {
        parts,
        reply_to: parse_reply_to_directive(&item.reply_to).and_then(|(reply_to, _)| reply_to),
        suppress_default_reply: parse_reply_to_directive(&item.reply_to)
            .map(|(_, suppress_default_reply)| suppress_default_reply)
            .unwrap_or(false),
        delivery_policy: None,
    })
}

fn normalize_reply_to_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_reply_to_directive(value: &ReplyToDirective) -> Option<(Option<String>, bool)> {
    match value {
        ReplyToDirective::Missing => None,
        ReplyToDirective::Null => Some((None, true)),
        ReplyToDirective::Value(value) => {
            normalize_reply_to_value(value).map(|reply_to| (Some(reply_to), false))
        }
    }
}

fn default_mime(part_kind: &str) -> &'static str {
    match part_kind {
        "image" => "image/*",
        "file" => "application/octet-stream",
        "voice" => "audio/*",
        "video" => "video/*",
        "sticker" => "application/x-sticker",
        _ => "application/octet-stream",
    }
}

fn degrade_markdown(input: &str) -> String {
    input
        .lines()
        .map(degrade_markdown_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn degrade_markdown_line(line: &str) -> String {
    let line = line
        .strip_prefix("## ")
        .or_else(|| line.strip_prefix("# "))
        .unwrap_or(line);
    let line = line.replace("**", "").replace('`', "");
    replace_markdown_links(&line)
}

fn replace_markdown_links(input: &str) -> String {
    let mut output = String::new();
    let mut rest = input;

    while let Some(label_start) = rest.find('[') {
        let before = &rest[..label_start];
        output.push_str(before);
        let candidate = &rest[label_start + 1..];

        let Some(label_end) = candidate.find("](") else {
            output.push_str(&rest[label_start..]);
            return output;
        };
        let label = &candidate[..label_end];
        let url_candidate = &candidate[label_end + 2..];
        let Some(url_end) = url_candidate.find(')') else {
            output.push_str(&rest[label_start..]);
            return output;
        };
        let url = &url_candidate[..url_end];
        output.push_str(label);
        output.push_str(": ");
        output.push_str(url);
        rest = &url_candidate[url_end + 1..];
    }

    output.push_str(rest);
    output
}
