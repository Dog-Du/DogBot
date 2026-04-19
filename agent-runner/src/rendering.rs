use serde::Deserialize;
use tracing::warn;

#[derive(Debug, Clone, Deserialize)]
pub struct MediaAction {
    #[serde(rename = "type")]
    pub action_type: String,
    pub source_type: String,
    pub source_value: String,
    pub caption_text: Option<String>,
    pub target_conversation: String,
}

pub fn degrade_markdown(input: &str) -> String {
    input
        .lines()
        .map(degrade_markdown_line)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse_media_actions(stdout: &str) -> Vec<MediaAction> {
    let normalized = stdout.replace("\r\n", "\n");
    normalized
        .split("```dogbot-action")
        .skip(1)
        .map(|chunk| chunk.trim_start_matches(|ch: char| ch.is_whitespace()))
        .filter_map(|chunk| chunk.split("\n```").next())
        .filter_map(|json| {
            let json = json.trim();
            match serde_json::from_str::<MediaAction>(json) {
                Ok(action) => Some(action),
                Err(err) => {
                    warn!("failed to parse dogbot-action block: {err}");
                    None
                }
            }
        })
        .collect()
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
