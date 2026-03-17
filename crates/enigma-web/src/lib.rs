mod auth;
mod models;
mod routes;
mod state;
mod static_files;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub use state::WebConfig;

use state::AppState;

/// Start the web UI server. Opens its own ManifestDb connection to the same SQLite file.
pub async fn start_web_server(
    config: WebConfig,
    db_path: &str,
    enigma_config: enigma_core::config::EnigmaSettings,
) -> anyhow::Result<()> {
    let db = enigma_core::manifest::ManifestDb::open(Path::new(db_path))?;

    let state = Arc::new(AppState {
        db: Mutex::new(db),
        config: enigma_config,
        jwt_secret: config.jwt_secret.clone(),
        admin_user: config.admin_user.clone(),
        admin_pass: config.admin_pass.clone(),
    });

    if state.jwt_secret == "enigma-jwt-secret-change-me" {
        tracing::warn!("Using default JWT secret — set jwt_secret in config for production!");
    }

    if state.admin_user == "admin" && state.admin_pass == "enigma" {
        tracing::warn!(
            "Using default admin credentials — change admin_user/admin_pass in config for production!"
        );
    }

    let app = routes::build_router(state);

    let addr: SocketAddr = config.listen_addr.parse()?;
    tracing::info!("Starting web interface on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
