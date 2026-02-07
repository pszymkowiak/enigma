use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "frontend/dist"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try exact path first
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [(header::CONTENT_TYPE, mime.as_ref().to_string())],
            file.data,
        )
            .into_response();
    }

    // SPA fallback: serve index.html for any non-API route
    if let Some(file) = Assets::get("index.html") {
        return ([(header::CONTENT_TYPE, "text/html".to_string())], file.data).into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}
