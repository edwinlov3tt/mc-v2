//! Axum HTTP server setup + route registration.
//!
//! Per ADR-0029 Decision 5: all endpoints mirror CLI verbs, JSON request/response.

use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

use crate::auth::{self, ApiKeyConfig};
use crate::cache::CubeCache;
use crate::config::{CorsConfig, DaemonConfig};
use crate::handlers::{admin, query, reload, sweep, trace, whatif, write};
use crate::signals::ShutdownSignal;

/// Shared application state, accessible from all handlers.
pub struct AppState {
    pub cache: Mutex<CubeCache>,
    pub config: DaemonConfig,
    pub started_at: Instant,
}

/// Start the HTTP server.
pub async fn start(state: Arc<AppState>, shutdown: ShutdownSignal) {
    let config = &state.config;
    let addr = SocketAddr::new(config.host, config.port);

    // Build CORS layer
    let cors = build_cors_layer(&config.cors_origins, config.host);

    // API key for auth middleware
    let api_key = ApiKeyConfig(config.api_key.clone());

    let app = Router::new()
        // Phase 8.0 MVP verb endpoints
        .route("/api/v1/query", post(query::handle_query))
        .route("/api/v1/write", post(write::handle_write))
        .route("/api/v1/trace", post(trace::handle_trace))
        // Phase 8.2 consumer API endpoints (ADR-0032)
        .route("/api/v1/whatif", post(whatif::handle_whatif))
        .route("/api/v1/sweep", post(sweep::handle_sweep))
        .route("/api/v1/reload", post(reload::handle_reload))
        // Admin endpoints
        .route("/api/v1/health", get(admin::handle_health))
        .route("/api/v1/status", get(admin::handle_status))
        .route("/api/v1/cubes", get(admin::handle_cubes))
        // Middleware
        .layer(middleware::from_fn(auth::auth_layer))
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            inject_api_key(api_key.clone(), req, next)
        }))
        .layer(cors)
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to bind to {addr}: {e}");
            // Exit code 3: bind failure
            std::process::exit(3);
        }
    };

    tracing::info!("Listening on http://{addr}");

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.wait().await;
            tracing::info!("Graceful shutdown initiated, draining connections...");
        })
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Server error: {e}");
        });
}

/// Middleware to inject API key config into request extensions.
async fn inject_api_key(api_key: ApiKeyConfig, mut req: Request, next: Next) -> Response {
    req.extensions_mut().insert(api_key);
    next.run(req).await
}

/// Build CORS layer based on configuration.
///
/// Per ADR-0029 Decision 11: "auto" mode allows localhost origins when bound
/// to localhost; empty when non-localhost.
fn build_cors_layer(cors_config: &CorsConfig, host: std::net::IpAddr) -> CorsLayer {
    use crate::config::is_localhost;
    use axum::http::{HeaderValue, Method};

    match cors_config {
        CorsConfig::Auto => {
            if is_localhost(host) {
                CorsLayer::new()
                    .allow_origin([
                        "http://localhost:3000".parse::<HeaderValue>().unwrap(),
                        "http://localhost:5173".parse::<HeaderValue>().unwrap(),
                        "http://localhost:8080".parse::<HeaderValue>().unwrap(),
                        "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
                        "http://127.0.0.1:5173".parse::<HeaderValue>().unwrap(),
                        "http://127.0.0.1:8080".parse::<HeaderValue>().unwrap(),
                    ])
                    .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                    .allow_headers(tower_http::cors::Any)
            } else {
                // Non-localhost: no origins allowed by default
                CorsLayer::new()
                    .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                    .allow_headers(tower_http::cors::Any)
            }
        }
        CorsConfig::Explicit(origins) => {
            let parsed: Vec<HeaderValue> = origins.iter().filter_map(|o| o.parse().ok()).collect();
            CorsLayer::new()
                .allow_origin(parsed)
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(tower_http::cors::Any)
        }
    }
}
