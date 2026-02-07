use axum::Json;

use crate::models::ClusterResponse;

pub async fn get_cluster() -> Json<ClusterResponse> {
    Json(ClusterResponse {
        mode: "single-node".to_string(),
        node_id: None,
        peers: vec![],
    })
}
