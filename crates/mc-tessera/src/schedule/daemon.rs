//! Sync daemon loop for cron-scheduled recipe execution.
//!
//! The daemon is a single-process loop that:
//! 1. Reads `schedules.json` on start and at each wake.
//! 2. Computes the next wake time (min of all schedule next-fires, clamped to 60s).
//! 3. Sleeps until the wake time.
//! 4. Fires due schedules by calling `Tessera::prepare` + `Tessera::apply`.
//! 5. Persists updated schedule state after each execution.
//! 6. Retries once on failure (60s backoff), then marks "failed".
//!
//! Signal handling: uses `AtomicBool` for shutdown. On Unix, registers
//! SIGTERM/SIGINT via `libc::signal` (the only justified `unsafe` in Phase 5C).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::runner::Tessera;
use crate::TesseraError;

use super::cron_expr::CronExpr;
use super::registry::ScheduleRegistry;

/// The cron daemon.
#[derive(Debug)]
pub struct Daemon {
    /// Model directory root.
    model_dir: PathBuf,
    /// Shutdown flag (set by signal handler or `--once` mode).
    shutdown: Arc<AtomicBool>,
    /// If true, fire all due schedules once and exit.
    once: bool,
}

impl Daemon {
    /// Create a new daemon for the given model directory.
    pub fn new(model_dir: &Path, once: bool) -> Self {
        Self {
            model_dir: model_dir.to_path_buf(),
            shutdown: Arc::new(AtomicBool::new(false)),
            once,
        }
    }

    /// Run the daemon loop. Blocks until shutdown signal or `--once` completes.
    pub fn run(&self) -> Result<(), TesseraError> {
        self.check_pid_file()?;
        self.write_pid_file()?;

        // Install signal handlers (Unix only).
        self.install_signal_handlers();

        let result = self.main_loop();

        // Clean up PID file on exit.
        let pid_path = self.pid_path();
        let _ = std::fs::remove_file(pid_path);

        result
    }

    fn main_loop(&self) -> Result<(), TesseraError> {
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                return Ok(());
            }

            let mut registry = ScheduleRegistry::load(&self.model_dir)?;
            let now = SystemTime::now();
            let now_secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

            let mut any_fired = false;

            for schedule in &mut registry.schedules {
                if schedule.status == "failed" && schedule.failure_count >= 2 {
                    // Permanently failed — skip unless retry time has come for
                    // a one-retry schedule.
                    if let Some(retry_at) = schedule.next_retry {
                        if now_secs < retry_at {
                            continue;
                        }
                        // Fall through to attempt retry.
                    } else {
                        continue;
                    }
                }

                let cron = match CronExpr::parse(&schedule.cron) {
                    Ok(c) => c,
                    Err(_) => {
                        schedule.status = "failed".to_string();
                        schedule.last_result = Some("invalid cron expression".to_string());
                        continue;
                    }
                };

                let after = if let Some(ref last) = schedule.last_run {
                    parse_rfc3339_to_system_time(last).unwrap_or(UNIX_EPOCH)
                } else {
                    // Never run — use created_at as the reference.
                    parse_rfc3339_to_system_time(&schedule.created_at).unwrap_or(UNIX_EPOCH)
                };

                let next_fire = cron.next_fire(after);

                if now >= next_fire {
                    any_fired = true;
                    let timestamp = super::cron_expr::now_rfc3339();

                    // Execute the recipe.
                    let recipe_path = if Path::new(&schedule.recipe_path).is_absolute() {
                        PathBuf::from(&schedule.recipe_path)
                    } else {
                        self.model_dir.join(&schedule.recipe_path)
                    };

                    match Self::execute_recipe(&recipe_path) {
                        Ok(_) => {
                            schedule.last_run = Some(timestamp);
                            schedule.last_result = Some("ok".to_string());
                            schedule.failure_count = 0;
                            schedule.next_retry = None;
                            schedule.status = "active".to_string();
                        }
                        Err(e) => {
                            schedule.last_run = Some(timestamp);
                            schedule.last_result = Some(format!("error: {e}"));
                            schedule.failure_count += 1;

                            if schedule.failure_count >= 2 {
                                // Two consecutive failures — mark failed.
                                schedule.status = "failed".to_string();
                                schedule.next_retry = None;
                            } else {
                                // First failure — schedule retry in 60s.
                                schedule.status = "active".to_string();
                                schedule.next_retry = Some(now_secs + 60);
                            }
                        }
                    }
                }
            }

            registry.save(&self.model_dir)?;

            if self.once {
                return Ok(());
            }

            if !any_fired {
                // Compute next wake time (min of all next fires, clamped to 60s).
                let sleep_dur = self.compute_sleep_duration(&registry, now);
                // Sleep in 1s increments to check shutdown flag.
                let deadline = SystemTime::now() + sleep_dur;
                while SystemTime::now() < deadline {
                    if self.shutdown.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }

    fn execute_recipe(recipe_path: &Path) -> Result<(), TesseraError> {
        let prepared = Tessera::prepare(recipe_path)?;
        let _report = Tessera::apply(prepared)?;
        Ok(())
    }

    fn compute_sleep_duration(&self, registry: &ScheduleRegistry, now: SystemTime) -> Duration {
        let mut min_wait = Duration::from_secs(60);

        for schedule in &registry.schedules {
            if schedule.status == "failed" && schedule.failure_count >= 2 {
                continue;
            }

            let cron = match CronExpr::parse(&schedule.cron) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let after = if let Some(ref last) = schedule.last_run {
                parse_rfc3339_to_system_time(last).unwrap_or(UNIX_EPOCH)
            } else {
                parse_rfc3339_to_system_time(&schedule.created_at).unwrap_or(UNIX_EPOCH)
            };

            let next_fire = cron.next_fire(after);
            if let Ok(wait) = next_fire.duration_since(now) {
                if wait < min_wait {
                    min_wait = wait;
                }
            } else {
                // Already past due — wake immediately.
                min_wait = Duration::from_secs(0);
            }
        }

        // Clamp max to 60s to ensure periodic re-reads of schedules.json.
        if min_wait > Duration::from_secs(60) {
            min_wait = Duration::from_secs(60);
        }

        min_wait
    }

    fn pid_path(&self) -> PathBuf {
        self.model_dir.join(".tessera").join("daemon.pid")
    }

    fn check_pid_file(&self) -> Result<(), TesseraError> {
        let pid_path = self.pid_path();
        if pid_path.exists() {
            let content =
                std::fs::read_to_string(&pid_path).map_err(|e| TesseraError::io(&pid_path, e))?;
            let pid_str = content.trim();

            // Check if the process is still running.
            if let Ok(pid) = pid_str.parse::<u32>() {
                if is_process_running(pid) {
                    return Err(TesseraError::SidecarInconsistent {
                        message: format!(
                            "daemon already running (PID {pid}). Remove {} to force.",
                            pid_path.display()
                        ),
                    });
                }
            }
            // Stale PID file — remove it.
            let _ = std::fs::remove_file(pid_path);
        }
        Ok(())
    }

    fn write_pid_file(&self) -> Result<(), TesseraError> {
        let tessera_dir = self.model_dir.join(".tessera");
        if !tessera_dir.exists() {
            std::fs::create_dir_all(&tessera_dir).map_err(|e| TesseraError::io(&tessera_dir, e))?;
        }
        let pid_path = self.pid_path();
        let pid = std::process::id();
        std::fs::write(&pid_path, pid.to_string()).map_err(|e| TesseraError::io(&pid_path, e))?;
        Ok(())
    }

    fn install_signal_handlers(&self) {
        #[cfg(unix)]
        {
            // SAFETY: This is the only justified unsafe in Phase 5C.
            // We register SIGTERM and SIGINT handlers that set the
            // AtomicBool shutdown flag. The signal handler function is
            // async-signal-safe (only does an atomic store).
            static SHUTDOWN_FLAG: AtomicBool = AtomicBool::new(false);

            // Copy our flag's address intent — we use a static because
            // signal handlers can't capture closures.
            SHUTDOWN_FLAG.store(false, Ordering::SeqCst);

            // We store a reference to our instance's shutdown flag via
            // a thread that monitors the static.
            let shutdown = Arc::clone(&self.shutdown);
            thread::spawn(move || {
                while !SHUTDOWN_FLAG.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(200));
                }
                shutdown.store(true, Ordering::SeqCst);
            });

            unsafe extern "C" fn handler(_sig: libc::c_int) {
                SHUTDOWN_FLAG.store(true, Ordering::SeqCst);
            }

            // SAFETY: `handler` is async-signal-safe (atomic store only).
            // `libc::signal` is the POSIX signal registration API.
            unsafe {
                libc::signal(libc::SIGTERM, handler as libc::sighandler_t);
                libc::signal(libc::SIGINT, handler as libc::sighandler_t);
            }
        }

        #[cfg(not(unix))]
        {
            // Non-Unix: no signal handling. The shutdown flag is checked
            // periodically in the sleep loop.
        }
    }
}

/// Check if a process with the given PID is running.
fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: kill(pid, 0) is the POSIX way to check process existence.
        // It does not actually send a signal.
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Parse a basic RFC3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) to SystemTime.
fn parse_rfc3339_to_system_time(s: &str) -> Option<SystemTime> {
    // Expected format: 2026-05-05T00:00:00Z
    if s.len() < 20 {
        return None;
    }
    let year: i32 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let hour: u32 = s.get(11..13)?.parse().ok()?;
    let min: u32 = s.get(14..16)?.parse().ok()?;
    let sec: u32 = s.get(17..19)?.parse().ok()?;

    // Convert civil date to unix timestamp (inverse of Hinnant's algorithm).
    let secs = ymdhms_to_unix(year, month, day, hour, min, sec)?;
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

/// Convert civil date/time to unix seconds. Inverse of Hinnant's algorithm.
fn ymdhms_to_unix(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> Option<u64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let y = if month <= 2 {
        year as i64 - 1
    } else {
        year as i64
    };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = (y - era * 400) as u64;
    let m = month;
    let doy = if m > 2 {
        (153 * (m as u64 - 3) + 2) / 5 + day as u64 - 1
    } else {
        (153 * (m as u64 + 9) + 2) / 5 + day as u64 - 1
    };
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe as i64 - 719_468;

    let total_secs = days * 86_400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64;
    if total_secs < 0 {
        return None;
    }
    Some(total_secs as u64)
}
