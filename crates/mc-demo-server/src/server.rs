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
use serde::Deserialize;
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

    // Phase 7A.4: load benchmark library if present.
    let benchmark_library = {
        let cwd = std::env::current_dir().unwrap_or_default();
        match mc_narrative::benchmark::read_benchmark_library(&cwd) {
            Ok(lib) => {
                println!(
                    "  {GREEN}Benchmarks loaded:{RESET} {BOLD}{}{RESET} metrics, periods {} → {}",
                    lib.benchmarks.len(),
                    lib.period_range.from,
                    lib.period_range.to,
                );
                Some(lib)
            }
            Err(_) => {
                println!("  {DIM}No benchmark library found (run `mc model build-benchmarks` to create one){RESET}");
                None
            }
        }
    };

    // ADR-0023 Decision 9: build IDF table once at startup, share via Arc.
    let idf_table = Arc::new(crate::pptx_match::IdfTable::build(&registry));
    println!(
        "  {GREEN}IDF table built:{RESET} {BOLD}{}{RESET} tokens from {} registry entries",
        idf_table.registry_size(),
        registry.len()
    );

    let state = Arc::new(AppState {
        registry,
        templates,
        benchmark_library,
        idf_table,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut app = Router::new()
        .route("/api/registry", get(handle_registry))
        .route("/api/upload", post(handle_upload))
        .route("/api/pptx-review", post(handle_pptx_review))
        .route("/api/health", get(handle_health))
        .route("/api/benchmarks", get(handle_benchmarks))
        // Template editor route deferred to Phase 7B.
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

/// GET /api/benchmarks — returns the benchmark library JSON if present, 404 if not.
async fn handle_benchmarks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match &state.benchmark_library {
        Some(lib) => Ok(Json(
            serde_json::to_value(lib).unwrap_or(serde_json::Value::Null),
        )),
        None => Err(StatusCode::NOT_FOUND),
    }
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

    let response = upload::process_upload(
        &state.registry,
        &state.templates,
        &bytes,
        state.benchmark_library.as_ref(),
        &state.idf_table,
    )
    .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e))?;

    Ok(Json(response))
}

// ─── PPTX Review Endpoint ───────────────────────────────────────────────────

/// A single review decision from the frontend.
#[derive(Debug, Deserialize)]
struct ReviewDecision {
    slide_index: u32,
    table_index: u32,
    action: ReviewAction,
}

/// What the user decided for this table.
#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
enum ReviewAction {
    #[serde(rename = "confirm")]
    Confirm {
        product_name: String,
        subproduct_name: String,
        table_name: String,
    },
    #[serde(rename = "skip")]
    Skip { reason: String },
}

/// POST /api/pptx-review — accepts user decisions for unmatched PPTX tables
/// and writes them back to the profile as overrides/skip rules.
async fn handle_pptx_review(
    State(_state): State<Arc<AppState>>,
    Json(decisions): Json<Vec<ReviewDecision>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if decisions.is_empty() {
        return Ok(Json(
            serde_json::json!({ "saved": 0, "profile": "lumina-charts" }),
        ));
    }

    // Load existing profile (try demo/sample-data first, then cwd).
    let cwd = std::env::current_dir().unwrap_or_default();
    let profile_dirs = [std::path::Path::new("demo/sample-data"), cwd.as_path()];
    let (profile_dir, mut profile) = profile_dirs
        .iter()
        .find_map(|d| crate::pptx_profile::load_profile(d, "lumina-charts").map(|p| (*d, p)))
        .ok_or((
            StatusCode::NOT_FOUND,
            "lumina-charts profile not found".to_string(),
        ))?;

    let mut saved = 0usize;
    for decision in &decisions {
        match &decision.action {
            ReviewAction::Confirm {
                product_name,
                subproduct_name,
                table_name,
            } => {
                // Add override to profile (avoid duplicates for same position).
                let already = profile.overrides.iter().any(|o| {
                    o.slide_index == decision.slide_index && o.table_index == decision.table_index
                });
                if !already {
                    profile.overrides.push(crate::pptx_profile::OverrideDef {
                        slide_index: decision.slide_index,
                        table_index: decision.table_index,
                        product_name: product_name.clone(),
                        subproduct_name: subproduct_name.clone(),
                        table_name: table_name.clone(),
                    });
                }
                saved += 1;
            }
            ReviewAction::Skip { reason } => {
                // Add positional skip rule to profile.
                let already = profile.skip_tables.iter().any(|s| {
                    s.when.slide_index == Some(decision.slide_index)
                        && s.when.table_index == Some(decision.table_index)
                });
                if !already {
                    profile.skip_tables.push(crate::pptx_profile::SkipRule {
                        when: crate::pptx_profile::SkipCondition {
                            table_title_contains_any: vec![],
                            slide_title_contains: None,
                            slide_index: Some(decision.slide_index),
                            table_index: Some(decision.table_index),
                        },
                        reason: reason.clone(),
                    });
                }
                saved += 1;
            }
        }
    }

    // Atomic write back to profile.
    crate::pptx_profile::save_profile(profile_dir, &profile)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    eprintln!(
        "  [pptx-review] Saved {saved} decisions to profile ({})",
        profile_dir
            .join(".mosaic/pptx-profiles/lumina-charts.yaml")
            .display()
    );

    Ok(Json(serde_json::json!({
        "saved": saved,
        "profile": "lumina-charts",
    })))
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
