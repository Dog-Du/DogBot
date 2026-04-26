use agent_runner::{api_proxy_config::ApiProxySettings, bootstrap, config::Settings};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    let settings = Settings::from_env()?;
    let proxy_settings = ApiProxySettings::from_env_optional()?;
    bootstrap::run(settings, proxy_settings).await
}
