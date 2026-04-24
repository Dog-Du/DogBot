use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum AssetSource {
    WorkspacePath(String),
    ManagedStore(String),
    ExternalUrl(String),
    PlatformNativeHandle(String),
    BridgeHandle(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRef {
    pub asset_id: String,
    pub kind: String,
    pub mime: String,
    pub size_bytes: u64,
    pub source: AssetSource,
}
