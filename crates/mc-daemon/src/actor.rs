//! Per-cube actor — sequential access within a cube, parallel across cubes.
//!
//! Per ADR-0029 Decision 2: each loaded cube lives in its own tokio task
//! behind an mpsc channel. All requests targeting that cube are sent via the
//! channel. The task processes them sequentially. Different cubes run on
//! different tasks — true parallelism across cubes.
//!
//! Cube operations (read, write, eval) are synchronous and potentially
//! CPU-heavy. They run via `tokio::task::spawn_blocking` to avoid starving
//! the tokio worker pool.

use mc_core::{
    CellCoordinate, Cube, PrincipalId, ScalarValue, TraceNode, WriteIntent, WritebackRequest,
};
use mc_model::ModelRefs;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

use crate::journal::{self, WriteJournal};

/// Channel capacity for per-cube request queue.
const ACTOR_CHANNEL_CAPACITY: usize = 256;

/// A request sent to a cube actor.
pub enum CubeRequest {
    Query {
        coord: CellCoordinate,
        reply: oneshot::Sender<Result<QueryResult, String>>,
    },
    Write {
        coord: CellCoordinate,
        coord_names: Vec<String>,
        coord_string: String,
        value: f64,
        reply: oneshot::Sender<Result<WriteResult, String>>,
    },
    Trace {
        coord: CellCoordinate,
        reply: oneshot::Sender<Result<TraceResult, String>>,
    },
    /// Phase 8.2: transient override query via `Cube::query_with_overrides`.
    /// Per ADR-0032 Decision 3: no revision bump, no journal touch.
    WhatIf {
        read_coords: Vec<CellCoordinate>,
        overrides: Vec<(CellCoordinate, ScalarValue)>,
        reply: oneshot::Sender<Result<WhatIfResult, String>>,
    },
    /// Phase 8.2: force reload cube from disk.
    /// Per ADR-0032 Decision 5 + Amendment 4.
    Reload {
        reply: oneshot::Sender<Result<ReloadResult, String>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

/// Result of a query operation.
#[derive(Debug)]
pub struct QueryResult {
    pub value: ScalarValue,
    pub revision: u64,
}

/// Result of a write operation.
#[derive(Debug)]
pub struct WriteResult {
    pub revision_after: u64,
    pub dirty_count: usize,
    pub write_id: u64,
}

/// Result of a trace operation.
#[derive(Debug)]
pub struct TraceResult {
    pub value: ScalarValue,
    pub trace: Option<TraceNode>,
}

/// Result of a whatif (transient override) operation.
/// Per ADR-0032 Decision 3: revision is unchanged.
#[derive(Debug)]
pub struct WhatIfResult {
    pub values: Vec<mc_core::CellValue>,
    pub revision: u64,
}

/// Result of a reload operation.
/// Per ADR-0032 Decision 5 + Amendment 4.
#[derive(Debug)]
pub struct ReloadResult {
    pub previous_revision: u64,
    pub new_revision: u64,
    pub duration_ms: u64,
}

/// State held by the actor (moved in/out of spawn_blocking).
struct ActorState {
    cube: Cube,
    #[allow(dead_code)] // Kept for future use in Phase 8.1 (coord resolution within actor)
    refs: ModelRefs,
    principal: PrincipalId,
    journal: WriteJournal,
    workspace_rel: String,
    cube_name: String,
    model_dir: PathBuf,
}

/// Spawn a cube actor task. Returns the sender half of the channel.
pub fn spawn_actor(
    cube: Cube,
    refs: ModelRefs,
    principal: PrincipalId,
    journal: WriteJournal,
    workspace_rel: String,
    cube_name: String,
    model_dir: PathBuf,
) -> mpsc::Sender<CubeRequest> {
    let (tx, rx) = mpsc::channel(ACTOR_CHANNEL_CAPACITY);

    let state = ActorState {
        cube,
        refs,
        principal,
        journal,
        workspace_rel,
        cube_name,
        model_dir,
    };

    tokio::spawn(actor_loop(rx, state));

    tx
}

/// The actor event loop. Receives requests from the channel, dispatches
/// cube operations via spawn_blocking.
///
/// Per ADR-0029 Decision 2: the actor loop is async (channel receive);
/// cube operations are sync and dispatch to the blocking pool.
async fn actor_loop(mut rx: mpsc::Receiver<CubeRequest>, mut state: ActorState) {
    while let Some(req) = rx.recv().await {
        match req {
            CubeRequest::Query { coord, reply } => {
                // Move state into spawn_blocking, execute, move back
                let result = tokio::task::spawn_blocking(move || {
                    let principal = state.principal;
                    let r = state
                        .cube
                        .read(&coord, principal)
                        .map(|cell| QueryResult {
                            value: cell.value,
                            revision: state.cube.revision().0,
                        })
                        .map_err(|e| e.to_string());
                    (state, r)
                })
                .await;
                match result {
                    Ok((s, r)) => {
                        state = s;
                        let _ = reply.send(r);
                    }
                    Err(e) => {
                        tracing::error!("spawn_blocking panicked: {e}");
                        return; // Actor is dead
                    }
                }
            }
            CubeRequest::Write {
                coord,
                coord_names,
                coord_string,
                value,
                reply,
            } => {
                let result = tokio::task::spawn_blocking(move || {
                    let r = handle_write(&mut state, &coord, &coord_names, &coord_string, value);
                    (state, r)
                })
                .await;
                match result {
                    Ok((s, r)) => {
                        state = s;
                        let _ = reply.send(r);
                    }
                    Err(e) => {
                        tracing::error!("spawn_blocking panicked: {e}");
                        return;
                    }
                }
            }
            CubeRequest::Trace { coord, reply } => {
                let result = tokio::task::spawn_blocking(move || {
                    let principal = state.principal;
                    let r = state
                        .cube
                        .read_with_trace(&coord, principal)
                        .map(|cell| TraceResult {
                            value: cell.value,
                            trace: cell.trace.map(|t| t.root),
                        })
                        .map_err(|e| e.to_string());
                    (state, r)
                })
                .await;
                match result {
                    Ok((s, r)) => {
                        state = s;
                        let _ = reply.send(r);
                    }
                    Err(e) => {
                        tracing::error!("spawn_blocking panicked: {e}");
                        return;
                    }
                }
            }
            CubeRequest::WhatIf {
                read_coords,
                overrides,
                reply,
            } => {
                let result = tokio::task::spawn_blocking(move || {
                    let principal = state.principal;
                    let r = state
                        .cube
                        .query_with_overrides(&read_coords, &overrides, principal)
                        .map(|values| WhatIfResult {
                            values,
                            revision: state.cube.revision().0,
                        })
                        .map_err(|e| e.to_string());
                    (state, r)
                })
                .await;
                match result {
                    Ok((s, r)) => {
                        state = s;
                        let _ = reply.send(r);
                    }
                    Err(e) => {
                        tracing::error!("spawn_blocking panicked: {e}");
                        return;
                    }
                }
            }
            CubeRequest::Reload { reply } => {
                let result = tokio::task::spawn_blocking(move || {
                    let start = std::time::Instant::now();
                    let previous_revision = state.cube.revision().0;

                    // Re-load the cube from disk using the existing model path.
                    let load_result = crate::loader::load_cube(&state.model_dir);
                    match load_result {
                        Ok(loaded) => {
                            state.cube = loaded.cube;
                            state.refs = loaded.refs;
                            state.principal = loaded.root_principal;
                            let new_revision = state.cube.revision().0;
                            let duration_ms = start.elapsed().as_millis() as u64;
                            (
                                state,
                                Ok(ReloadResult {
                                    previous_revision,
                                    new_revision,
                                    duration_ms,
                                }),
                            )
                        }
                        Err(e) => (state, Err(e.to_string())),
                    }
                })
                .await;
                match result {
                    Ok((s, r)) => {
                        state = s;
                        let _ = reply.send(r);
                    }
                    Err(e) => {
                        tracing::error!("spawn_blocking panicked: {e}");
                        return;
                    }
                }
            }
            CubeRequest::Shutdown { reply } => {
                tracing::info!("Cube actor '{}' shutting down", state.cube_name);
                let _ = reply.send(());
                return;
            }
        }
    }
    // Channel closed — all senders dropped
    tracing::debug!("Cube actor '{}' channel closed", state.cube_name);
}

/// Execute the write path inside spawn_blocking.
///
/// Per ADR-0029 Decision 8 (durability handoff):
/// 1. Write "pending" entry to journal
/// 2. Apply write to cube
/// 3. Append to .tessera/writes.jsonl (four-source persistence)
/// 4. Write "committed" entry to journal
/// 5. Reply to client
fn handle_write(
    state: &mut ActorState,
    coord: &CellCoordinate,
    coord_names: &[String],
    coord_string: &str,
    value: f64,
) -> Result<WriteResult, String> {
    // Step 1: Journal "pending"
    let seq = state
        .journal
        .write_pending(&state.workspace_rel, &state.cube_name, coord_names, value)
        .map_err(|e| format!("journal write failed: {e}"))?;

    // Step 2: Apply to cube
    let write_result = state
        .cube
        .write(WritebackRequest {
            coord: coord.clone(),
            new_value: ScalarValue::F64(value),
            principal: state.principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .map_err(|e| format!("write failed: {e}"))?;

    // Step 3: Append to .tessera/writes.jsonl (durable four-source persistence)
    let write_id = journal::append_tessera_write(&state.model_dir, coord_string, value)
        .map_err(|e| format!("tessera write log failed: {e}"))?;

    // Step 4: Journal "committed"
    state
        .journal
        .write_committed(
            seq,
            &state.workspace_rel,
            &state.cube_name,
            coord_names,
            value,
        )
        .map_err(|e| format!("journal commit failed: {e}"))?;

    Ok(WriteResult {
        revision_after: state.cube.revision().0,
        dirty_count: write_result.invalidated.len(),
        write_id,
    })
}

/// Resolve a coordinate from dimension=element name pairs against ModelRefs.
///
/// Returns the resolved CellCoordinate plus a canonical string representation.
pub fn resolve_coord(
    refs: &ModelRefs,
    where_map: &BTreeMap<String, String>,
) -> Option<CellCoordinate> {
    refs.coord_from_names(where_map)
}

/// Build a canonical coordinate string from name pairs.
pub fn coord_to_string(names: &BTreeMap<String, String>, dimension_order: &[String]) -> String {
    let mut parts = Vec::new();
    for dim in dimension_order {
        if let Some(elem) = names.get(dim) {
            parts.push(format!("{dim}={elem}"));
        }
    }
    parts.join(",")
}

/// Build coordinate name pairs from the ordered element names in the API request.
pub fn coord_names_from_array(
    coord_array: &[String],
    dimension_order: &[String],
) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (i, elem) in coord_array.iter().enumerate() {
        if i < dimension_order.len() {
            map.insert(dimension_order[i].clone(), elem.clone());
        }
    }
    map
}
