//! Signal handling for graceful and forced shutdown.
//!
//! Per ADR-0029 Decision 12:
//! - First Ctrl+C (SIGINT) or SIGTERM → graceful shutdown (drain in-flight, 30s max)
//! - Second Ctrl+C within 5s → forced exit (no drain, exit 1)
//!
//! Uses tokio's signal handling rather than raw libc (the daemon already has
//! a tokio runtime, unlike the tessera daemon which was sync-only).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared shutdown flag checked by the server and actor loops.
#[derive(Clone)]
pub struct ShutdownSignal {
    flag: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownSignal {
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Check if shutdown has been requested.
    pub fn is_shutdown(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// Trigger shutdown.
    pub fn trigger(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Wait until shutdown is triggered.
    pub async fn wait(&self) {
        if self.is_shutdown() {
            return;
        }
        self.notify.notified().await;
    }
}

/// Install signal handlers and return the shutdown signal.
///
/// Per ADR-0029 Decision 12:
/// - First signal → set shutdown flag, return gracefully
/// - Second signal within 5s → forced exit (exit code 1)
pub fn install_signal_handlers() -> ShutdownSignal {
    let shutdown = ShutdownSignal::new();
    let shutdown_clone = shutdown.clone();

    tokio::spawn(async move {
        wait_for_signal().await;
        tracing::info!("Received shutdown signal, draining in-flight requests (30s max)...");
        shutdown_clone.trigger();

        // Wait for second signal (forced exit)
        let force_deadline = tokio::time::sleep(std::time::Duration::from_secs(5));
        tokio::select! {
            _ = wait_for_signal() => {
                tracing::warn!("Received second signal — forced exit");
                std::process::exit(1);
            }
            _ = force_deadline => {
                // 5s window expired without second signal — graceful path continues
            }
        }
    });

    shutdown
}

/// Wait for SIGINT or SIGTERM.
async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigint = signal(SignalKind::interrupt()).expect("failed to register SIGINT");
        let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM");
        tokio::select! {
            _ = sigint.recv() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to register Ctrl+C handler");
    }
}
