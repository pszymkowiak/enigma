use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Multipart, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use enigma_auth::middleware::require_permission;
use enigma_auth::AuthUser;

use crate::state::AppState;

// ── Types ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BrowseQuery {
    #[serde(default)]
    pub path: String,
}

#[derive(Serialize)]
pub struct BrowseResponse {
    pub path: String,
    pub folders: Vec<FolderItem>,
    pub files: Vec<FileItem>,
}

#[derive(Serialize)]
pub struct FolderItem {
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct FileItem {
    pub name: String,
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct MkdirRequest {
    pub path: String,
}

#[derive(Deserialize)]
pub struct DeleteQuery {
    pub path: String,
}

// ── Error wrapper ────────────────────────────────────────────

pub(crate) enum FilesError {
    Auth(enigma_auth::error::AuthError),
    Internal(String),
}

impl From<enigma_auth::error::AuthError> for FilesError {
    fn from(e: enigma_auth::error::AuthError) -> Self {
        FilesError::Auth(e)
    }
}

impl IntoResponse for FilesError {
    fn into_response(self) -> Response {
        match self {
            FilesError::Auth(e) => e.into_response(),
            FilesError::Internal(msg) => {
                let body = serde_json::json!({ "error": msg });
                (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
            }
        }
    }
}

fn get_s3_and_bucket(state: &AppState) -> Result<(&enigma_s3::EnigmaS3State, String), FilesError> {
    let s3 = state
        .s3_state
        .as_deref()
        .ok_or_else(|| FilesError::Internal("file storage not configured".into()))?;
    let bucket = {
        let db = s3
            .db
            .lock()
            .map_err(|_| FilesError::Internal("db lock".into()))?;
        let ns = db
            .list_namespaces()
            .map_err(|e| FilesError::Internal(e.to_string()))?;
        ns.first()
            .map(|(_id, name, _created)| name.clone())
            .ok_or_else(|| FilesError::Internal("no namespace available".into()))?
    };
    Ok((s3, bucket))
}

// ── Handlers ─────────────────────────────────────────────────

/// GET /api/files?path=folder/
pub async fn browse(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<BrowseQuery>,
) -> Result<Json<BrowseResponse>, FilesError> {
    require_permission(&auth_user, "buckets:read")?;
    tracing::info!(user = %auth_user.username, path = %q.path, "browsing files");

    let (s3, bucket) = get_s3_and_bucket(&state)?;
    let listing = enigma_s3::ops::list_folder(s3, &bucket, &q.path)
        .await
        .map_err(|e| {
            tracing::error!(user = %auth_user.username, path = %q.path, error = %e, "browse failed");
            FilesError::Internal(e.to_string())
        })?;

    tracing::info!(
        user = %auth_user.username,
        path = %q.path,
        folders = listing.folders.len(),
        files = listing.files.len(),
        "browse OK"
    );

    Ok(Json(BrowseResponse {
        path: listing.path,
        folders: listing
            .folders
            .into_iter()
            .map(|f| FolderItem {
                name: f.name,
                path: f.path,
            })
            .collect(),
        files: listing
            .files
            .into_iter()
            .map(|f| FileItem {
                name: f.name,
                key: f.key,
                size: f.size,
                etag: f.etag,
                created_at: f.created_at,
            })
            .collect(),
    }))
}

/// POST /api/files/upload  (multipart: path + file)
pub async fn upload(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, FilesError> {
    require_permission(&auth_user, "buckets:write")?;
    let started = Instant::now();
    tracing::info!(user = %auth_user.username, "upload started");

    let (s3, bucket) = get_s3_and_bucket(&state)?;

    let mut path_prefix = String::new();
    let mut file_name = String::new();
    let mut file_data: Vec<u8> = Vec::new();
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| {
            tracing::error!(user = %auth_user.username, error = %e, "multipart read failed");
            FilesError::Internal(e.to_string())
        })?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "path" => {
                path_prefix = field
                    .text()
                    .await
                    .map_err(|e| {
                        tracing::error!(error = %e, "reading path field failed");
                        FilesError::Internal(e.to_string())
                    })?;
            }
            "file" => {
                file_name = field
                    .file_name()
                    .unwrap_or("unnamed")
                    .to_string();
                content_type = field.content_type().map(|s| s.to_string());
                tracing::info!(
                    user = %auth_user.username,
                    file = %file_name,
                    content_type = content_type.as_deref().unwrap_or("unknown"),
                    "receiving file data"
                );
                file_data = field
                    .bytes()
                    .await
                    .map_err(|e| {
                        tracing::error!(
                            user = %auth_user.username,
                            file = %file_name,
                            error = %e,
                            "reading file bytes failed"
                        );
                        FilesError::Internal(e.to_string())
                    })?
                    .to_vec();
                tracing::info!(
                    user = %auth_user.username,
                    file = %file_name,
                    size_bytes = file_data.len(),
                    size_mb = file_data.len() / (1024 * 1024),
                    "file data received"
                );
            }
            _ => {}
        }
    }

    if file_name.is_empty() || file_data.is_empty() {
        tracing::warn!(user = %auth_user.username, "upload rejected: missing file field");
        return Err(FilesError::Internal("missing file field".into()));
    }

    let key = format!("{}{}", path_prefix, file_name);
    tracing::info!(
        user = %auth_user.username,
        key = %key,
        size_bytes = file_data.len(),
        "storing object to cloud providers"
    );

    let etag = enigma_s3::ops::store_object(s3, &bucket, &key, &file_data, content_type.as_deref())
        .await
        .map_err(|e| {
            tracing::error!(
                user = %auth_user.username,
                key = %key,
                size_bytes = file_data.len(),
                error = %e,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "store_object failed"
            );
            FilesError::Internal(e.to_string())
        })?;

    let elapsed = started.elapsed();
    let size = file_data.len();
    let mbps = if elapsed.as_secs_f64() > 0.0 {
        (size as f64 / 1_048_576.0) / elapsed.as_secs_f64()
    } else {
        0.0
    };
    tracing::info!(
        user = %auth_user.username,
        key = %key,
        size_bytes = size,
        size_mb = size / (1024 * 1024),
        etag = %etag,
        elapsed_ms = elapsed.as_millis() as u64,
        throughput_mbps = format_args!("{:.1}", mbps),
        "upload complete"
    );

    Ok(Json(serde_json::json!({
        "key": key,
        "size": size,
        "etag": etag,
    })))
}

/// GET /api/files/download?path=folder/file.txt
pub async fn download(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<BrowseQuery>,
) -> Result<Response, FilesError> {
    require_permission(&auth_user, "buckets:read")?;
    let started = Instant::now();
    tracing::info!(user = %auth_user.username, path = %q.path, "download started");

    let (s3, bucket) = get_s3_and_bucket(&state)?;
    let file = enigma_s3::ops::retrieve_object(s3, &bucket, &q.path)
        .await
        .map_err(|e| {
            tracing::error!(user = %auth_user.username, path = %q.path, error = %e, "download failed");
            FilesError::Internal(e.to_string())
        })?;

    tracing::info!(
        user = %auth_user.username,
        path = %q.path,
        size_bytes = file.size,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "download complete"
    );

    let filename = q.path.rsplit('/').next().unwrap_or(&q.path);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    if let Some(ct) = &file.content_type {
        if let Ok(v) = HeaderValue::from_str(ct) {
            headers.insert(header::CONTENT_TYPE, v);
        }
    }
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&file.size.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("0")),
    );

    Ok((headers, Body::from(file.data)).into_response())
}

/// DELETE /api/files?path=folder/file.txt
pub async fn delete(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DeleteQuery>,
) -> Result<Json<serde_json::Value>, FilesError> {
    require_permission(&auth_user, "buckets:write")?;
    tracing::info!(user = %auth_user.username, path = %q.path, "deleting file");

    let (s3, bucket) = get_s3_and_bucket(&state)?;
    enigma_s3::ops::remove_object(s3, &bucket, &q.path)
        .await
        .map_err(|e| {
            tracing::error!(user = %auth_user.username, path = %q.path, error = %e, "delete failed");
            FilesError::Internal(e.to_string())
        })?;

    tracing::info!(user = %auth_user.username, path = %q.path, "file deleted");
    Ok(Json(serde_json::json!({ "deleted": q.path })))
}

/// POST /api/files/mkdir  { "path": "folder/subfolder/" }
pub async fn mkdir(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<MkdirRequest>,
) -> Result<Json<serde_json::Value>, FilesError> {
    require_permission(&auth_user, "buckets:write")?;

    let (s3, bucket) = get_s3_and_bucket(&state)?;

    // S3 convention: create a zero-byte object with trailing slash as the "directory marker"
    let path = if req.path.ends_with('/') {
        req.path.clone()
    } else {
        format!("{}/", req.path)
    };

    tracing::info!(user = %auth_user.username, path = %path, "creating folder");
    enigma_s3::ops::store_object(s3, &bucket, &path, &[], None)
        .await
        .map_err(|e| {
            tracing::error!(user = %auth_user.username, path = %path, error = %e, "mkdir failed");
            FilesError::Internal(e.to_string())
        })?;

    tracing::info!(user = %auth_user.username, path = %path, "folder created");
    Ok(Json(serde_json::json!({ "created": path })))
}
