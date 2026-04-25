pub mod common;
pub mod qq;
pub mod wechatpadpro;

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{
    config::Settings,
    models::{ErrorResponse, MessageResponse, RunResponse},
    protocol::{CanonicalEvent, OutboundPlan},
};

#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryContext {
    pub platform: String,
    pub platform_account: String,
    pub conversation_id: String,
    pub target_actor_id: Option<String>,
    pub target_display_name: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub native_event: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngressRoute {
    pub path: &'static str,
    pub allow_head: bool,
}

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    fn platform_id(&self) -> &'static str;
    fn ingress_routes(&self) -> &'static [IngressRoute];

    fn probe_payload(&self) -> Value {
        json!({ "status": "ok" })
    }

    fn decode_event(&self, payload: &Value) -> Option<CanonicalEvent>;

    async fn send_plan(
        &self,
        context: &DeliveryContext,
        plan: &OutboundPlan,
    ) -> Result<MessageResponse, ErrorResponse>;
}

#[derive(Clone, Default)]
pub struct PlatformRegistry {
    adapters: HashMap<String, Arc<dyn PlatformAdapter>>,
    ingress_routes: Vec<(String, IngressRoute)>,
}

impl PlatformRegistry {
    pub fn from_settings(settings: &Settings) -> Result<Self, ErrorResponse> {
        let mut registry = Self::default();
        registry.register(Arc::new(qq::QqPlatform::from_settings(settings)?));
        registry.register(Arc::new(wechatpadpro::WeChatPadProPlatform::from_settings(
            settings,
        )?));
        Ok(registry)
    }

    pub fn register(&mut self, adapter: Arc<dyn PlatformAdapter>) {
        let platform_id = adapter.platform_id().to_string();
        for route in adapter.ingress_routes() {
            self.ingress_routes.push((platform_id.clone(), *route));
        }
        self.adapters.insert(platform_id, adapter);
    }

    pub fn get(&self, platform_id: &str) -> Option<Arc<dyn PlatformAdapter>> {
        self.adapters.get(platform_id).cloned()
    }

    pub fn ingress_routes(&self) -> &[(String, IngressRoute)] {
        &self.ingress_routes
    }
}

pub fn delivery_context_from_event(event: &CanonicalEvent) -> DeliveryContext {
    let conversation_scope = event.conversation.split(':').nth(1);
    let target_actor_id = if conversation_scope == Some("group") {
        Some(event.actor.clone())
    } else {
        None
    };
    let reply_to_message_id = event.message().map(|message| message.message_id.clone());

    DeliveryContext {
        platform: event.platform.clone(),
        platform_account: event.platform_account.clone(),
        conversation_id: event.conversation.clone(),
        target_actor_id,
        target_display_name: None,
        reply_to_message_id,
        native_event: Some(event.raw_native_payload.clone()),
    }
}

pub fn run_response_output(response: &RunResponse) -> &str {
    let stdout = response.stdout.trim();
    if !stdout.is_empty() {
        stdout
    } else {
        response.stderr.trim()
    }
}
