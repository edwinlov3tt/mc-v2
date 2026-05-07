//! Pipeline timing infrastructure — per ADR-0019 Decision 11.
//!
//! Wraps `std::time::Instant` per stage, prints the breakdown table
//! to stdout, and serializes into the JSON response's `timing` object.

use serde::Serialize;
use std::time::Instant;

// ANSI color codes
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

/// Accumulates per-stage timing for a single upload request.
#[derive(Debug, Clone, Serialize)]
pub struct PipelineTiming {
    pub registry_match_ms: f64,
    pub cube_compile_ms: f64,
    pub cube_populate_ms: f64,
    pub narrative_eval_ms: f64,
    pub serialize_ms: f64,
}

impl PipelineTiming {
    pub fn total_ms(&self) -> f64 {
        self.registry_match_ms
            + self.cube_compile_ms
            + self.cube_populate_ms
            + self.narrative_eval_ms
            + self.serialize_ms
    }
}

/// Builder that records timestamps as the pipeline progresses.
pub struct PipelineTimer {
    start: Instant,
    registry_done: Option<Instant>,
    compile_done: Option<Instant>,
    populate_done: Option<Instant>,
    narrative_done: Option<Instant>,
    serialize_done: Option<Instant>,
}

impl PipelineTimer {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
            registry_done: None,
            compile_done: None,
            populate_done: None,
            narrative_done: None,
            serialize_done: None,
        }
    }

    pub fn mark_registry_done(&mut self) {
        self.registry_done = Some(Instant::now());
    }

    pub fn mark_compile_done(&mut self) {
        self.compile_done = Some(Instant::now());
    }

    pub fn mark_populate_done(&mut self) {
        self.populate_done = Some(Instant::now());
    }

    pub fn mark_narrative_done(&mut self) {
        self.narrative_done = Some(Instant::now());
    }

    pub fn mark_serialize_done(&mut self) {
        self.serialize_done = Some(Instant::now());
    }

    /// Build the final timing record.
    pub fn finish(&self) -> PipelineTiming {
        let registry = self.registry_done.unwrap_or(self.start);
        let compile = self.compile_done.unwrap_or(registry);
        let populate = self.populate_done.unwrap_or(compile);
        let narrative = self.narrative_done.unwrap_or(populate);
        let serialize = self.serialize_done.unwrap_or(narrative);

        PipelineTiming {
            registry_match_ms: ms(self.start, registry),
            cube_compile_ms: ms(registry, compile),
            cube_populate_ms: ms(compile, populate),
            narrative_eval_ms: ms(populate, narrative),
            serialize_ms: ms(narrative, serialize),
        }
    }

    /// Print the timing breakdown to stdout (the terminal where `mc start` runs).
    pub fn print_to_terminal(&self, label: &str, csv_count: usize, tactic_count: usize) {
        let t = self.finish();
        let now = chrono_now();
        println!(
            "{DIM}{now}{RESET}  {BOLD}POST /api/upload{RESET} {CYAN}{label}{RESET} {DIM}({csv_count} CSVs, {tactic_count} tactics){RESET}"
        );
        print_stage("Registry match", t.registry_match_ms);
        print_stage("Cube compile", t.cube_compile_ms);
        print_stage("Cube populate", t.cube_populate_ms);
        print_stage("Narrative eval", t.narrative_eval_ms);
        print_stage("Serialize", t.serialize_ms);
        println!("  {DIM}─────────────────────────{RESET}");
        let total = t.total_ms();
        let color = if total < 200.0 { GREEN } else { YELLOW };
        println!("  {BOLD}{color}Done               {total:.2}ms{RESET}");
        println!();
    }
}

fn print_stage(name: &str, ms_val: f64) {
    println!("  {DIM}{name:<19}{RESET} {ms_val:.2}ms");
}

fn ms(from: Instant, to: Instant) -> f64 {
    to.duration_since(from).as_secs_f64() * 1000.0
}

fn chrono_now() -> String {
    // Simple timestamp without pulling in chrono crate.
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = days_to_date(days);
    format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02}",)
}

fn days_to_date(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;
    loop {
        let year_days = if is_leap(year) { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        year += 1;
    }
    let month_days: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
