use std::collections::HashMap;
use std::str::FromStr;

use crate::config::ConfigError;

pub fn string_or_default(
    env_map: &HashMap<String, String>,
    key: &str,
    default: &str,
) -> String {
    optional_trimmed(env_map, key).unwrap_or_else(|| default.to_string())
}

pub fn optional_trimmed(env_map: &HashMap<String, String>, key: &str) -> Option<String> {
    env_map.get(key).and_then(|value| trim_to_option(value))
}

pub fn parse_or_default<T>(
    env_map: &HashMap<String, String>,
    key: &'static str,
    default: T,
) -> Result<T, ConfigError>
where
    T: FromStr,
{
    match env_map.get(key) {
        Some(raw) => raw.parse().map_err(|_| ConfigError::InvalidInt(key)),
        None => Ok(default),
    }
}

pub fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
