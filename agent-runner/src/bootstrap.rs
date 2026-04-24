use std::error::Error;

use tokio::net::TcpListener;

use crate::{
    api_proxy::build_app as build_api_proxy_app, api_proxy_config::ApiProxySettings,
    config::Settings, server::build_app,
};

pub async fn run(
    settings: Settings,
    proxy_settings: Option<ApiProxySettings>,
) -> Result<(), Box<dyn Error>> {
    let bind_addr = settings.bind_addr.clone();
    let app = build_app(settings)?;
    let listener = TcpListener::bind(&bind_addr).await?;

    if let Some(proxy_settings) = proxy_settings {
        let proxy_bind_addr = proxy_settings.bind_addr.clone();
        let proxy_app = build_api_proxy_app(proxy_settings);
        let proxy_listener = TcpListener::bind(&proxy_bind_addr).await?;
        tokio::try_join!(
            axum::serve(listener, app),
            axum::serve(proxy_listener, proxy_app)
        )?;
        return Ok(());
    }

    axum::serve(listener, app).await?;
    Ok(())
}
