//! Mosaic demo server — Phase 6D Marketing Report Demo MVP.
//!
//! Serves a React frontend and exposes an HTTP API for uploading
//! marketing CSV zip files, auto-detecting tactics from a 190-entry
//! registry, and returning structured report data. No LLM, no
//! hallucination, sub-200ms processing.

pub mod banner;
pub mod ingest;
pub mod narrative;
pub mod pptx;
pub mod pptx_match;
pub mod pptx_profile;
pub mod registry;
pub mod server;
pub mod timing;
pub mod upload;
pub mod workspace;

/// Start the demo server: print banner, boot axum, open browser.
///
/// This is the sync entry point called by `mc start` in mc-cli.
/// Internally creates a tokio runtime.
pub fn run(port: u16, static_dir: Option<&str>) {
    banner::print_banner();

    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(async {
        server::start(port, static_dir).await;
    });
}
