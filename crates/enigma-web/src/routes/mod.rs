pub mod cluster;
pub mod namespaces;
pub mod status;
pub mod storage;

use std::sync::Arc;

use axum::Router;
use axum::middleware;
use axum::routing::{get, post};

use crate::auth;
use crate::state::AppState;
use crate::static_files;

pub fn build_router(state: Arc<AppState>) -> Router {
    // Protected API routes (require JWT)
    let api = Router::new()
        .route("/api/status", get(status::get_status))
        .route("/api/storage/providers", get(storage::get_providers))
        .route("/api/storage/chunks/stats", get(storage::get_chunk_stats))
        .route("/api/storage/backups", get(storage::get_backups))
        .route("/api/namespaces", get(namespaces::list_namespaces))
        .route(
            "/api/namespaces/{name}/objects",
            get(namespaces::list_objects),
        )
        .route("/api/cluster", get(cluster::get_cluster))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .with_state(state.clone());

    // Public auth route
    let auth_routes = Router::new()
        .route("/api/auth/login", post(auth::login))
        .with_state(state);

    Router::new()
        .merge(auth_routes)
        .merge(api)
        .fallback(static_files::static_handler)
}
