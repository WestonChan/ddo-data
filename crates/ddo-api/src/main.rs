use anyhow::{Context, Result};
use ddo_api::{app, AppState};
use std::net::SocketAddr;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let db_path = PathBuf::from(std::env::var("DDO_DB_PATH").unwrap_or_else(|_| "ddo.db".to_string()));
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let state = AppState::open(&db_path).with_context(|| format!("opening {}", db_path.display()))?.with_rate_limit();
    tracing::info!(dataset = %state.dataset().upstream_sha, built_at = %state.dataset().built_at, "serving {}", db_path.display());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app(state).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
