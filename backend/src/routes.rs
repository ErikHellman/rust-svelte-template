use crate::AppState;
use axum::Router;
use axum::http::{HeaderValue, Method};
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub fn build(state: AppState) -> Router {
    let api = Router::new()
        .nest("/auth", crate::auth::router())
        .nest("/notes", crate::notes::router())
        .with_state(state.clone());

    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "static".into());
    let static_service = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(format!("{static_dir}/index.html")));

    let cors = build_cors(&state);

    Router::new()
        .nest("/api", api)
        .fallback_service(static_service)
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(cors)
}

fn build_cors(state: &AppState) -> CorsLayer {
    // In dev the Vite server runs on a different origin and proxies /api;
    // we still permit a local-only origin to make calling the API directly
    // from a dev browser ergonomic. In prod the SPA is same-origin.
    let dev_origin = HeaderValue::from_static("http://localhost:5173");
    let layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
        ])
        .allow_credentials(true);

    if state.config.public_base_url.contains("localhost") {
        layer.allow_origin(dev_origin)
    } else {
        layer
    }
}
