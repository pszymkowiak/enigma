use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};

use crate::models::{NamespaceResponse, ObjectResponse};
use crate::state::AppState;

pub async fn list_namespaces(State(state): State<Arc<AppState>>) -> Json<Vec<NamespaceResponse>> {
    let db = state.db.lock().unwrap();
    let ns = db.list_namespaces().unwrap_or_default();
    Json(
        ns.iter()
            .map(|(id, name, created_at)| {
                let count = db.count_objects_with_prefix(*id, "").unwrap_or(0);
                NamespaceResponse {
                    id: *id,
                    name: name.clone(),
                    created_at: created_at.clone(),
                    object_count: count,
                }
            })
            .collect(),
    )
}

pub async fn list_objects(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Json<Vec<ObjectResponse>> {
    let db = state.db.lock().unwrap();
    let ns_id = db.get_namespace_id(&name).unwrap_or(None);
    let Some(ns_id) = ns_id else {
        return Json(vec![]);
    };
    let objects = db.list_objects(ns_id, "", 1000, "").unwrap_or_default();
    Json(
        objects
            .iter()
            .map(|(key, size, etag, created_at)| ObjectResponse {
                key: key.clone(),
                size: *size,
                etag: etag.clone(),
                created_at: created_at.clone(),
            })
            .collect(),
    )
}
