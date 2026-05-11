//! Hot cube cache — load-on-first-request, LRU eviction.
//!
//! Per ADR-0029 Decision 3:
//! - Cubes are NOT loaded at startup. They're registered (paths known) but stay cold.
//! - First API request targeting a cube triggers cold-load → cache it.
//! - LRU eviction when budget exceeded.
//! - Never evict during an active request.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::mpsc;

use crate::actor::{self, CubeRequest};
use crate::journal::WriteJournal;
use crate::loader;

/// Key identifying a unique cube within the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CubeKey {
    pub workspace_path: PathBuf,
    pub cube_name: String,
}

impl std::fmt::Display for CubeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.workspace_path.display(), self.cube_name)
    }
}

/// Registration info for a cube (known path, may or may not be loaded).
#[derive(Debug, Clone)]
pub struct CubeRegistration {
    pub model_path: PathBuf,
    pub state: CubeState,
}

/// Current state of a registered cube.
#[derive(Debug, Clone)]
pub enum CubeState {
    /// Registered but not loaded. Will cold-load on first request.
    Cold,
    /// Loaded and cached in memory.
    Warm {
        loaded_at: Instant,
        last_accessed: Instant,
        estimated_bytes: usize,
    },
    /// Failed to load. Requests will get 503.
    Degraded { error: String },
}

/// The cube cache manages registered cubes, warm actors, and LRU eviction.
pub struct CubeCache {
    /// All known cubes (cold or warm).
    registered: HashMap<CubeKey, CubeRegistration>,
    /// Warm cubes with running actors.
    actors: HashMap<CubeKey, mpsc::Sender<CubeRequest>>,
    /// ModelRefs for warm cubes (needed for coordinate resolution in handlers).
    refs: HashMap<CubeKey, mc_model::ModelRefs>,
    /// Dimension ordering for warm cubes.
    dimension_orders: HashMap<CubeKey, Vec<String>>,
    /// Root principal for warm cubes.
    principals: HashMap<CubeKey, mc_core::PrincipalId>,
    /// LRU tracking (front = least recently used).
    access_order: VecDeque<CubeKey>,
    /// Memory budget in bytes.
    budget_bytes: usize,
    /// Current estimated memory usage.
    current_bytes: usize,
    /// Workspace path for journal creation.
    workspace_path: PathBuf,
}

/// Public info about a cube (for /api/v1/cubes endpoint).
#[derive(Debug)]
pub struct CubeInfo {
    pub name: String,
    pub state: String,
    pub revision: Option<u64>,
}

impl CubeCache {
    /// Create a new cache with the given budget.
    pub fn new(budget_mb: usize, workspace_path: PathBuf) -> Self {
        Self {
            registered: HashMap::new(),
            actors: HashMap::new(),
            refs: HashMap::new(),
            dimension_orders: HashMap::new(),
            principals: HashMap::new(),
            access_order: VecDeque::new(),
            budget_bytes: budget_mb * 1024 * 1024,
            current_bytes: 0,
            workspace_path,
        }
    }

    /// Register a cube (cold — don't load yet).
    pub fn register(&mut self, key: CubeKey, model_path: PathBuf) {
        self.registered.insert(
            key,
            CubeRegistration {
                model_path,
                state: CubeState::Cold,
            },
        );
    }

    /// Get the actor sender for a cube, loading it if necessary.
    ///
    /// Returns `None` if the cube is not registered.
    /// Returns `Err` if loading fails.
    pub async fn get_or_load(
        &mut self,
        key: &CubeKey,
    ) -> Result<mpsc::Sender<CubeRequest>, String> {
        // Fast path: already warm
        if self.actors.contains_key(key) {
            // Update LRU
            self.touch_lru(key);
            if let Some(reg) = self.registered.get_mut(key) {
                if let CubeState::Warm {
                    ref mut last_accessed,
                    ..
                } = reg.state
                {
                    *last_accessed = Instant::now();
                }
            }
            return Ok(self.actors.get(key).unwrap().clone());
        }

        // Check if registered
        let reg = self
            .registered
            .get(key)
            .ok_or_else(|| format!("cube '{}' not found", key.cube_name))?;

        // Check if degraded
        if let CubeState::Degraded { ref error } = reg.state {
            return Err(format!("cube '{}' failed to load: {error}", key.cube_name));
        }

        let model_path = reg.model_path.clone();

        // Evict if needed before loading
        // Rough estimate: 1MB per cube (will be refined after load)
        let estimated = 1024 * 1024;
        while self.current_bytes + estimated > self.budget_bytes && !self.access_order.is_empty() {
            self.evict_lru().await;
        }

        // Cold-load the cube
        tracing::info!("Cold-loading cube '{}'...", key.cube_name);
        let load_result = {
            let path = model_path.clone();
            tokio::task::spawn_blocking(move || loader::load_cube(&path))
                .await
                .map_err(|e| format!("spawn_blocking failed: {e}"))?
        };

        let loaded = match load_result {
            Ok(l) => l,
            Err(e) => {
                let err_msg = e.to_string();
                tracing::error!("Failed to load cube '{}': {err_msg}", key.cube_name);
                if let Some(reg) = self.registered.get_mut(key) {
                    reg.state = CubeState::Degraded {
                        error: err_msg.clone(),
                    };
                }
                return Err(err_msg);
            }
        };

        // Estimate memory usage (rough: store cell count * 64 bytes)
        let cell_count = loaded.cube.store().len();
        let estimated_bytes = cell_count * 64 + 1024 * 1024; // cells + overhead

        // Create journal for this cube's actor
        let journal = WriteJournal::open(&self.workspace_path)
            .map_err(|e| format!("journal open failed: {e}"))?;

        // Compute model_dir for tessera writes persistence
        let model_dir = model_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let workspace_rel = "./".to_string();

        // Save refs and dimension order before spawning actor
        let refs_clone = loaded.refs.clone();
        let dim_order = loaded.refs.dimension_order.clone();
        let principal = loaded.root_principal;

        // Spawn actor
        let tx = actor::spawn_actor(
            loaded.cube,
            loaded.refs,
            loaded.root_principal,
            journal,
            workspace_rel,
            key.cube_name.clone(),
            model_dir,
        );

        // Update cache state
        self.actors.insert(key.clone(), tx.clone());
        self.refs.insert(key.clone(), refs_clone);
        self.dimension_orders.insert(key.clone(), dim_order);
        self.principals.insert(key.clone(), principal);
        self.current_bytes += estimated_bytes;

        if let Some(reg) = self.registered.get_mut(key) {
            reg.state = CubeState::Warm {
                loaded_at: Instant::now(),
                last_accessed: Instant::now(),
                estimated_bytes,
            };
        }
        self.access_order.push_back(key.clone());

        tracing::info!(
            "Cube '{}' loaded ({cell_count} cells, ~{}KB)",
            key.cube_name,
            estimated_bytes / 1024
        );

        Ok(tx)
    }

    /// Get ModelRefs for a warm cube (needed for coordinate resolution).
    pub fn get_refs(&self, key: &CubeKey) -> Option<&mc_model::ModelRefs> {
        self.refs.get(key)
    }

    /// Get dimension ordering for a warm cube.
    pub fn get_dimension_order(&self, key: &CubeKey) -> Option<&[String]> {
        self.dimension_orders.get(key).map(|v| v.as_slice())
    }

    /// Get root principal for a warm cube.
    pub fn get_principal(&self, key: &CubeKey) -> Option<mc_core::PrincipalId> {
        self.principals.get(key).copied()
    }

    /// List all registered cubes with their state.
    pub fn list_cubes(&self) -> Vec<CubeInfo> {
        self.registered
            .iter()
            .map(|(key, reg)| {
                let state_str = match &reg.state {
                    CubeState::Cold => "cold".to_string(),
                    CubeState::Warm { .. } => "warm".to_string(),
                    CubeState::Degraded { .. } => "degraded".to_string(),
                };
                CubeInfo {
                    name: key.cube_name.clone(),
                    state: state_str,
                    revision: None, // Would need actor query to get this
                }
            })
            .collect()
    }

    /// Number of registered cubes.
    pub fn registered_count(&self) -> usize {
        self.registered.len()
    }

    /// Number of warm (loaded) cubes.
    pub fn warm_count(&self) -> usize {
        self.actors.len()
    }

    /// Current memory usage in bytes.
    pub fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    /// Budget in bytes.
    pub fn budget_bytes(&self) -> usize {
        self.budget_bytes
    }

    /// Resolve a cube name to a CubeKey (single-workspace mode).
    pub fn resolve_key(&self, cube_name: &str) -> Option<CubeKey> {
        self.registered
            .keys()
            .find(|k| k.cube_name == cube_name)
            .cloned()
    }

    /// List of degraded cube names.
    pub fn degraded_cubes(&self) -> Vec<String> {
        self.registered
            .iter()
            .filter_map(|(key, reg)| match &reg.state {
                CubeState::Degraded { .. } => Some(key.cube_name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Shutdown all actors gracefully.
    pub async fn shutdown_all(&mut self) {
        let keys: Vec<CubeKey> = self.actors.keys().cloned().collect();
        for key in keys {
            if let Some(tx) = self.actors.remove(&key) {
                let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                let _ = tx.send(CubeRequest::Shutdown { reply: reply_tx }).await;
                let _ = tokio::time::timeout(std::time::Duration::from_secs(5), reply_rx).await;
            }
        }
    }

    /// Touch a key in the LRU order (move to back = most recent).
    fn touch_lru(&mut self, key: &CubeKey) {
        if let Some(pos) = self.access_order.iter().position(|k| k == key) {
            self.access_order.remove(pos);
        }
        self.access_order.push_back(key.clone());
    }

    /// Evict the least-recently-used cube.
    async fn evict_lru(&mut self) {
        if let Some(key) = self.access_order.pop_front() {
            tracing::info!("Evicting cube '{}' (LRU)", key.cube_name);

            // Send shutdown to actor
            if let Some(tx) = self.actors.remove(&key) {
                let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                let _ = tx.send(CubeRequest::Shutdown { reply: reply_tx }).await;
                let _ = tokio::time::timeout(std::time::Duration::from_secs(5), reply_rx).await;
            }

            // Update memory tracking
            if let Some(reg) = self.registered.get_mut(&key) {
                if let CubeState::Warm {
                    estimated_bytes, ..
                } = reg.state
                {
                    self.current_bytes = self.current_bytes.saturating_sub(estimated_bytes);
                }
                reg.state = CubeState::Cold;
            }

            self.refs.remove(&key);
            self.dimension_orders.remove(&key);
            self.principals.remove(&key);
        }
    }
}
