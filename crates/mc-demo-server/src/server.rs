//! Axum HTTP server — per ADR-0019 Decision 2.
//!
//! Routes:
//!   GET  /api/registry  — returns the performance_tables registry
//!   POST /api/upload    — accepts multipart zip upload, returns detection results
//!   GET  /*             — serves static frontend files

use crate::registry::Registry;
use crate::upload::{self, AppState};
use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

// ANSI color codes
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

/// Start the axum server.
pub async fn start(port: u16, static_dir: Option<&str>) {
    // Pre-warm registry at startup (Decision 11 optimization #3).
    let registry_path = find_registry_path();
    let registry = match Registry::from_file(&registry_path) {
        Ok(r) => {
            println!(
                "  {GREEN}Registry loaded:{RESET} {BOLD}{}{RESET} tactic specs",
                r.len()
            );
            r
        }
        Err(e) => {
            eprintln!("  {RED}{BOLD}WARNING:{RESET} {RED}Could not load registry: {e}{RESET}");
            eprintln!("  {DIM}Detection will not work. Place performance_tables.csv in demo/registry/{RESET}");
            Registry::from_csv("product_name,subproduct_name,table_name,file_name,headers,description,is_required,sort_order\n").expect("empty registry")
        }
    };

    // Pre-compile narrative templates at startup (Decision 11 optimization #5).
    let narratives_path = find_narratives_path();
    let templates = crate::narrative::load_templates(&narratives_path);
    println!(
        "  {GREEN}Templates loaded:{RESET} {BOLD}{}{RESET} narrative templates",
        templates.len()
    );

    let state = Arc::new(AppState {
        registry,
        templates,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut app = Router::new()
        .route("/api/registry", get(handle_registry))
        .route("/api/upload", post(handle_upload))
        .route("/api/health", get(handle_health))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // 50MB
        .layer(cors)
        .with_state(state);

    // Serve static frontend files if a directory is provided
    if let Some(dir) = static_dir {
        let serve_dir =
            tower_http::services::ServeDir::new(dir).append_index_html_on_directories(true);
        app = app.fallback_service(serve_dir);
    }

    let addr = format!("0.0.0.0:{port}");
    println!("  {GREEN}Starting server on{RESET} {BOLD}{CYAN}http://localhost:{port}{RESET}");

    // Open browser
    println!("  {DIM}Opening browser...{RESET}");
    let url = format!("http://localhost:{port}");
    if let Err(e) = open::that(&url) {
        eprintln!("  {YELLOW}Could not open browser: {e}{RESET}");
        println!("  Open {CYAN}{url}{RESET} manually.");
    }

    println!("  {DIM}Press Ctrl-C to stop.{RESET}");
    println!();

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind address");

    axum::serve(listener, app).await.expect("server error");
}

async fn handle_health() -> &'static str {
    "ok"
}

async fn handle_registry(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "count": state.registry.len(),
        "specs": state.registry.all_specs(),
    }))
}

async fn handle_upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<upload::UploadResponse>, (StatusCode, String)> {
    // Read the uploaded file
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("multipart error: {e}")))?
    {
        if field.name() == Some("file")
            || field.content_type().map_or(false, |ct| ct.contains("zip"))
        {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("read error: {e}")))?;
            file_bytes = Some(bytes.to_vec());
            break;
        }
        // Also accept any field as the file if it looks like binary data
        if file_bytes.is_none() {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, format!("read error: {e}")))?;
            if !bytes.is_empty() {
                file_bytes = Some(bytes.to_vec());
            }
        }
    }

    let bytes = file_bytes.ok_or((StatusCode::BAD_REQUEST, "no file uploaded".to_string()))?;

    let response = upload::process_upload(&state.registry, &state.templates, &bytes)
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e))?;

    Ok(Json(response))
}

/// Find the registry CSV file, checking several likely locations.
fn find_registry_path() -> String {
    let candidates = [
        "demo/registry/performance_tables.csv",
        "../demo/registry/performance_tables.csv",
        "../../demo/registry/performance_tables.csv",
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return path.to_string();
        }
    }
    // Default — may fail at load time with a helpful error.
    candidates[0].to_string()
}

/// Find the narratives directory, checking several likely locations.
fn find_narratives_path() -> String {
    let candidates = [
        "demo/narratives",
        "../demo/narratives",
        "../../demo/narratives",
    ];
    for path in &candidates {
        if std::path::Path::new(path).is_dir() {
            return path.to_string();
        }
    }
    candidates[0].to_string()
}
