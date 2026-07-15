use axum::body::Body;
use axum::extract::Path as AxumPath;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "ui/dist"]
struct FrontendAssets;

/// Serve the embedded Vite build for any non-API path, falling back to `index.html` for
/// client-side routing (SPA fallback). If `ui/dist` doesn't exist at build time (e.g.
/// during backend-only development), `rust-embed` embeds an empty set and this always 404s,
/// which is expected when running the UI separately via `npm run dev`.
pub async fn spa_fallback(AxumPath(path): AxumPath<String>) -> Response {
    serve_embedded(&path)
}

pub async fn spa_root() -> Response {
    serve_embedded("index.html")
}

fn serve_embedded(path: &str) -> Response {
    let path = path.trim_start_matches('/');
    match FrontendAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.into_owned()))
                .unwrap()
        }
        None => match FrontendAssets::get("index.html") {
            Some(content) => Response::builder()
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(content.data.into_owned()))
                .unwrap(),
            None => (StatusCode::NOT_FOUND, "UI build not found; run `npm run build` in ui/").into_response(),
        },
    }
}
