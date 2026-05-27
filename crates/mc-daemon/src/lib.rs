//! `mc-daemon` — Mosaic service daemon.
//!
//! Persistent HTTP service with hot cube cache, per-cube actors, and crash
//! recovery. Phase 8.0 MVP: three verbs (query, write, trace), single
//! workspace, optional API key auth.
//!
//! Per ADR-0029: deployment shell for the Mosaic kernel. Uses tokio + axum
//! (permitted per ADR-0025 Rule 1.6). The kernel (`mc-core`) stays untouched.

pub mod actor;
pub mod auth;
pub mod cache;
pub mod config;
pub mod coord;
pub mod error_envelope;
pub mod handlers;
pub mod journal;
pub mod loader;
pub mod server;
pub mod signals;

use cache::{CubeCache, CubeKey};
use config::DaemonConfig;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Exit codes per Claude Desktop review.
pub mod exit_codes {
    /// Clean shutdown.
    pub const OK: i32 = 0;
    /// Forced shutdown (double Ctrl+C).
    pub const FORCED: i32 = 1;
    /// Configuration error (bad daemon.toml, missing workspace.yaml).
    pub const CONFIG_ERROR: i32 = 2;
    /// Bind failure (port in use, non-localhost without api_key).
    pub const BIND_ERROR: i32 = 3;
    /// PID conflict (daemon already running).
    pub const PID_CONFLICT: i32 = 4;
}

/// Start the daemon. This is the main entry point called by `mc up`.
pub fn run(config: DaemonConfig) {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(async {
        run_async(config).await;
    });
}

async fn run_async(config: DaemonConfig) {
    // Initialize logging
    init_logging(&config);

    let workspace_path = config.workspace_path.clone();

    // Step 5: Check for existing daemon PID
    if let Err(msg) = check_pid_file(&workspace_path) {
        tracing::error!("{msg}");
        std::process::exit(exit_codes::PID_CONFLICT);
    }

    // Also check for tessera daemon PID (per ADR-0029 Decision 9)
    if let Err(msg) = check_tessera_pid(&workspace_path) {
        tracing::error!("{msg}");
        std::process::exit(exit_codes::PID_CONFLICT);
    }

    // Step 6: Write PID file
    if let Err(e) = write_pid_file(&workspace_path) {
        tracing::error!("Failed to write PID file: {e}");
        std::process::exit(exit_codes::CONFIG_ERROR);
    }

    // Step 3: Discover workspace
    let cube_entries = match discover_cubes(&workspace_path) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::error!("Workspace discovery failed: {e}");
            remove_pid_file(&workspace_path);
            std::process::exit(exit_codes::CONFIG_ERROR);
        }
    };

    // Step 4: Register cubes (cold)
    let mut cache = CubeCache::new(config.cache_budget_mb, workspace_path.clone());
    for (name, model_path) in &cube_entries {
        let key = CubeKey {
            workspace_path: workspace_path.clone(),
            cube_name: name.clone(),
        };
        cache.register(key, model_path.clone());
    }

    let registered_count = cache.registered_count();

    // Step 7: Replay write journal (uncommitted entries)
    let journal = match journal::WriteJournal::open(&workspace_path) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to open write journal: {e}");
            remove_pid_file(&workspace_path);
            std::process::exit(exit_codes::CONFIG_ERROR);
        }
    };
    let uncommitted = journal.replay_uncommitted();
    if !uncommitted.is_empty() {
        tracing::info!(
            "Replaying {} uncommitted journal entries...",
            uncommitted.len()
        );
        for entry in &uncommitted {
            // Load the affected cube and apply the write
            let key = CubeKey {
                workspace_path: workspace_path.clone(),
                cube_name: entry.cube.clone(),
            };
            match cache.get_or_load(&key).await {
                Ok(tx) => {
                    let dim_order = cache.get_dimension_order(&key).unwrap_or(&[]).to_vec();
                    let refs = cache.get_refs(&key).cloned();

                    if let Some(refs) = refs {
                        let coord_names = actor::coord_names_from_array(&entry.coord, &dim_order);
                        if let Some(coord) = actor::resolve_coord(&refs, &coord_names) {
                            let coord_string = actor::coord_to_string(&coord_names, &dim_order);
                            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                            let _ = tx
                                .send(actor::CubeRequest::Write {
                                    coord,
                                    coord_names: entry.coord.clone(),
                                    coord_string,
                                    value: entry.value,
                                    reply: reply_tx,
                                })
                                .await;
                            match reply_rx.await {
                                Ok(Ok(_)) => {
                                    tracing::info!(
                                        "Replayed journal entry seq={} for cube '{}'",
                                        entry.seq,
                                        entry.cube
                                    );
                                }
                                Ok(Err(e)) => {
                                    tracing::warn!(
                                        "Journal replay failed for seq={}: {e}",
                                        entry.seq
                                    );
                                }
                                Err(_) => {
                                    tracing::warn!(
                                        "Journal replay: actor dropped for seq={}",
                                        entry.seq
                                    );
                                }
                            }
                        } else {
                            tracing::warn!(
                                "Journal replay: could not resolve coord for seq={}",
                                entry.seq
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Journal replay: could not load cube '{}': {e}", entry.cube);
                }
            }
        }
    }

    // Step 8: Install signal handlers
    let shutdown = signals::install_signal_handlers();

    // Build app state
    let state = Arc::new(server::AppState {
        cache: Mutex::new(cache),
        config: config.clone(),
        started_at: Instant::now(),
    });

    // Step 10: Print banner
    print_banner(&config, registered_count);

    // Step 9: Start Axum server
    server::start(state.clone(), shutdown.clone()).await;

    // After server stops: graceful cleanup
    tracing::info!("Server stopped, cleaning up...");

    // Flush journal
    if let Ok(journal) = journal::WriteJournal::open(&workspace_path) {
        let _ = journal.truncate();
    }

    // Shutdown all actors
    {
        let mut cache = state.cache.lock().await;
        cache.shutdown_all().await;
    }

    // Remove PID file
    remove_pid_file(&workspace_path);

    tracing::info!("Daemon shut down cleanly");
}

/// Stop the daemon by sending SIGTERM to the PID in the PID file.
/// Called by `mc down`.
pub fn stop(workspace_path: &Path) -> i32 {
    let pid_path = workspace_path.join(".mosaic").join("daemon.pid");
    if !pid_path.exists() {
        eprintln!("Mosaic daemon is not running (no PID file found).");
        return 1;
    }
    let content = match std::fs::read_to_string(&pid_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Could not read PID file: {e}");
            return 1;
        }
    };
    let pid_str = content.trim();
    let pid: u32 = match pid_str.parse() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Invalid PID in file: {pid_str:?}");
            return 1;
        }
    };

    // Check if process is running
    if !is_process_running(pid) {
        eprintln!("Daemon process {pid} is not running. Removing stale PID file.");
        let _ = std::fs::remove_file(&pid_path);
        return 1;
    }

    // Send SIGTERM
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    {
        eprintln!("mc down is only supported on Unix. Kill process {pid} manually.");
        return 1;
    }

    println!("Sent shutdown signal to daemon (PID {pid}).");
    println!("Waiting for graceful shutdown...");

    // Wait up to 30s for PID file to disappear
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if !pid_path.exists() {
            println!("Daemon stopped.");
            return 0;
        }
    }

    eprintln!("Daemon did not stop within 30s. You may need to kill it manually (PID {pid}).");
    1
}

/// Report daemon status. Called by `mc status`.
pub fn status(workspace_path: &Path) -> i32 {
    let pid_path = workspace_path.join(".mosaic").join("daemon.pid");
    if !pid_path.exists() {
        println!("Mosaic daemon is not running. Use `mc up` to start.");
        return 1;
    }
    let content = match std::fs::read_to_string(&pid_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Could not read PID file: {e}");
            return 1;
        }
    };
    let pid_str = content.trim();
    let pid: u32 = match pid_str.parse() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Invalid PID: {pid_str:?}");
            return 1;
        }
    };

    if !is_process_running(pid) {
        println!("Mosaic daemon is not running (stale PID file for {pid}).");
        let _ = std::fs::remove_file(&pid_path);
        return 1;
    }

    println!("Mosaic daemon is running (PID {pid}).");

    // Try to query the health endpoint
    // For Phase 8.0, just report the PID status. Full health query
    // would require an HTTP client dep we don't have.
    0
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn init_logging(config: &DaemonConfig) {
    use tracing_subscriber::fmt;
    use tracing_subscriber::EnvFilter;

    let level = match config.log_level {
        config::LogLevel::Debug => "debug",
        config::LogLevel::Info => "info",
        config::LogLevel::Warn => "warn",
        config::LogLevel::Error => "error",
    };

    let filter = EnvFilter::new(format!("mc_daemon={level},info"));

    match config.log_format {
        config::LogFormat::Json => {
            fmt().json().with_env_filter(filter).init();
        }
        config::LogFormat::Pretty => {
            fmt().pretty().with_env_filter(filter).init();
        }
        config::LogFormat::Auto => {
            // Auto: pretty for TTY, JSON for detached
            if config.detach {
                fmt().json().with_env_filter(filter).init();
            } else {
                fmt().with_env_filter(filter).init();
            }
        }
    }
}

fn discover_cubes(workspace_path: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let workspace = mc_workspace::parse::parse_workspace(workspace_path)
        .map_err(|e| format!("workspace parse error: {e}"))?;

    let mut entries = Vec::new();
    for cube_entry in &workspace.cubes {
        let model_path = workspace_path.join(&cube_entry.path);
        let name = cube_entry.name.clone().unwrap_or_else(|| {
            cube_entry
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed")
                .to_string()
        });
        entries.push((name, model_path));
    }
    Ok(entries)
}

fn check_pid_file(workspace_path: &Path) -> Result<(), String> {
    let pid_path = workspace_path.join(".mosaic").join("daemon.pid");
    if pid_path.exists() {
        let content = std::fs::read_to_string(&pid_path)
            .map_err(|e| format!("could not read PID file: {e}"))?;
        let pid_str = content.trim();
        if let Ok(pid) = pid_str.parse::<u32>() {
            if is_process_running(pid) {
                return Err(format!(
                    "Daemon is already running (PID {pid}). Use `mc down` to stop it."
                ));
            }
        }
        // Stale PID file — remove it
        let _ = std::fs::remove_file(&pid_path);
    }
    Ok(())
}

fn check_tessera_pid(workspace_path: &Path) -> Result<(), String> {
    let pid_path = workspace_path.join(".tessera").join("daemon.pid");
    if pid_path.exists() {
        let content = std::fs::read_to_string(&pid_path).unwrap_or_default();
        let pid_str = content.trim();
        if let Ok(pid) = pid_str.parse::<u32>() {
            if is_process_running(pid) {
                return Err(format!(
                    "Tessera daemon is running (PID {pid}). Stop it with \
                     `mc tessera daemon --stop` before starting the service daemon."
                ));
            }
        }
    }
    Ok(())
}

fn write_pid_file(workspace_path: &Path) -> std::io::Result<()> {
    let mosaic_dir = workspace_path.join(".mosaic");
    std::fs::create_dir_all(&mosaic_dir)?;
    let pid_path = mosaic_dir.join("daemon.pid");
    std::fs::write(pid_path, std::process::id().to_string())?;
    Ok(())
}

fn remove_pid_file(workspace_path: &Path) {
    let pid_path = workspace_path.join(".mosaic").join("daemon.pid");
    let _ = std::fs::remove_file(pid_path);
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    // kill(pid, 0) checks if process exists without sending a signal
    unsafe {
        libc::kill(pid as libc::pid_t, 0) == 0
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn print_banner(config: &DaemonConfig, cube_count: usize) {
    let pid = std::process::id();
    let host = config.host;
    let port = config.port;
    let auth = if config.api_key.is_some() {
        "enabled"
    } else {
        "disabled"
    };

    println!();
    println!("  ┌─────────────────────────────────────────┐");
    println!("  │  Mosaic daemon running                   │");
    println!("  │  Port:      http://{host}:{port:<5}         │");
    println!(
        "  │  Workspace: {:<29}│",
        truncate_str(&config.workspace_path.display().to_string(), 29)
    );
    println!("  │  Cubes:     {cube_count} registered, 0 loaded        │");
    println!("  │  Auth:      {auth:<29}│");
    println!("  │  PID:       {pid:<29}│");
    println!("  │                                          │");
    println!("  │  Press Ctrl+C to stop                    │");
    println!("  └─────────────────────────────────────────┘");
    println!();
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("...{}", &s[s.len() - (max - 3)..])
    }
}
