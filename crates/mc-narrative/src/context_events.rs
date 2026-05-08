//! Context events — operational annotations that explain findings from causes
//! outside the cube. Phase 7A.5 (ADR-0022 Decision 4).
//!
//! A context event records "budget was cut 40%" or "3 creatives paused" for
//! a specific period and scope. Templates query these via `has_context_event()`,
//! `context_description()`, and `context_event_count()`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Top-level schema for `.mosaic/context-events.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEventsFile {
    /// Schema version for forward compatibility.
    pub schema_version: String,
    /// The context events in this file.
    pub events: Vec<ContextEvent>,
}

/// A single context event annotation.
///
/// Per ADR-0022 Decision 4: context events are hand-edited annotations
/// stored in `.mosaic/context-events.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEvent {
    /// Unique identifier. Convention: `ce-{period}-{seq}`.
    pub id: String,
    /// The reporting period this event applies to.
    pub period: String,
    /// Scope filter — `{ Channel: "X", Market: "Y" }`. Empty = all scopes.
    #[serde(default)]
    pub scope: BTreeMap<String, String>,
    /// Event type category (e.g., "budget_change", "creative_pause").
    #[serde(rename = "type")]
    pub event_type: String,
    /// Human-readable explanation. Used in template interpolation via
    /// `context_description()`.
    pub description: String,
    /// Provenance — who logged this event.
    #[serde(default)]
    pub source: Option<String>,
    /// ISO date. After this date, the event no longer matches.
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Read context events from `.mosaic/context-events.yaml` in the given directory.
///
/// Returns an empty vec if the file doesn't exist (graceful degradation,
/// same pattern as benchmark library loading).
pub fn read_context_events(dir: &std::path::Path) -> Vec<ContextEvent> {
    let path = dir.join(".mosaic").join("context-events.yaml");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    match serde_yaml::from_str::<ContextEventsFile>(&content) {
        Ok(file) => file.events,
        Err(e) => {
            eprintln!(
                "  \x1b[33mwarn\x1b[0m MC7054: cannot parse context events {}: {e}",
                path.display()
            );
            Vec::new()
        }
    }
}
