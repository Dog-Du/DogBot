use agent_runner::{
    api_proxy_config::ApiProxySettings, bootstrap, config::Settings,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let settings = Settings::from_env()?;
    let proxy_settings = ApiProxySettings::from_env()?;
    bootstrap::run(settings, proxy_settings).await
}
