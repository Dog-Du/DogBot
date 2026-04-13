use agent_runner::{
    api_proxy::build_app as build_api_proxy_app, api_proxy_config::ApiProxySettings,
    config::Settings, server::build_app,
};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let settings = Settings::from_env()?;
    let proxy_settings = ApiProxySettings::from_env()?;
    let bind_addr = settings.bind_addr.clone();
    let proxy_bind_addr = proxy_settings.bind_addr.clone();
    let app = build_app(settings)?;
    let proxy_app = build_api_proxy_app(proxy_settings);
    let listener = TcpListener::bind(&bind_addr).await?;
    let proxy_listener = TcpListener::bind(&proxy_bind_addr).await?;
    tokio::try_join!(
        axum::serve(listener, app),
        axum::serve(proxy_listener, proxy_app)
    )?;
    Ok(())
}
