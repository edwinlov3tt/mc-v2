# Mosaic Service Daemon — Phase 8 Design Research

**Status:** Research note (pre-ADR); captures design intent for Phase 8
**Date:** 2026-05-09
**Prerequisite phases:** Phase 4C (org/workspace primitive), Phase 7A (narrative engine — complete)
**Deployment shape:** ADR-0025 Shape 4
**Crate:** `mc-daemon` (new) — deployment shell per ADR-0025 Rule 1.6

---

## The one-line pitch

`mc up` starts a persistent Mosaic service that hot-loads workspaces, caches cubes in memory, serves an HTTP + MCP API, and survives restarts — the first real "service" deployment shape.

---

## Part 1: What the daemon does

The daemon is **Shape 4** in ADR-0025's deployment sequence. It sits between "CLI tool on your laptop" (Shape 1-3) and "multi-tenant cloud service" (Shape 6). It's the first deployment where Mosaic is a long-running process rather than a fire-and-forget command.

**Core responsibilities:**

| Responsibility | What it means |
|---|---|
| Hot cube cache | Cubes stay loaded in memory; queries hit warm cache, not cold-load pipeline |
| Workspace discovery | Scans configured workspace paths; auto-loads cubes on first request |
| HTTP API | REST endpoints mirroring the CLI verbs (query, whatif, trace, sweep, diff, write, narrate) |
| MCP server | Same operations via Model Context Protocol for AI agent consumption |
| Crash recovery | On restart, re-loads cubes from last known state; in-flight writes are journaled |
| Signal handling | Graceful shutdown on SIGTERM/SIGINT — flush pending writes, close connections |
| Org-awareness | Knows which org/workspace it serves (inherits Phase 4C manifests) |
| Ledger append | Narrative results append to workspace ledger (`.mosaic/analysis-ledger.jsonl`) |
| Tessera scheduling | Subsumes the existing tessera daemon (cron-driven recipe execution) |

**What the daemon is NOT:**
- Not multi-tenant (that's Phase 9 cloud)
- Not horizontally scalable (single process; multiple cubes, one machine)
- Not a full web application server (Phase 6B's web UI consumes the daemon's API)
- Not responsible for auth/billing (Phase 9)

---

## Part 2: The startup command — `mc up`

### Why `mc up`

| Option | Feeling | Problem |
|---|---|---|
| `mc start` | Already exists (demo server) | Breaking change or awkward coexistence |
| `mc daemon start` | Explicit | Verbose for daily use |
| `mc serve` | Web-focused | Undersells the daemon (it's more than HTTP) |
| `mc up` | Docker-inspired, punchy | None — clean and available |

**Decision:** `mc up` is the daemon command. Short, memorable, unmistakable. `mc down` stops it.

`mc start` remains for the Phase 6D demo server during the transition period. Once the daemon is stable, `mc start` becomes an alias for `mc up` and the demo-specific mode is `mc up --demo`.

### Command surface

```
mc up [--port 8787] [--workspace <path>] [--org <path>] [--pid <file>]
    Start the Mosaic daemon. Discovers workspaces, loads cubes, serves API.
    Default port: 8787 (MOSA on a phone keypad).
    Default workspace: current directory (if workspace.yaml present).
    Writes PID to .mosaic/daemon.pid.
    Foreground by default; --detach for background.

mc up --detach
    Start daemon in background. Logs to .mosaic/daemon.log.

mc down [--pid <file>]
    Graceful shutdown. Sends SIGTERM to PID in .mosaic/daemon.pid.
    Waits for in-flight operations to complete (max 30s timeout).

mc status
    Show daemon health: uptime, loaded cubes, cache utilization, API port.
    If not running: "Mosaic daemon is not running. Use `mc up` to start."

mc ps
    List loaded cubes with their state: warm (cached), cold (evicted), loading.
```

### Startup sequence

```
mc up
  1. Read workspace.yaml (or org.yaml) from --workspace/--org path
  2. Validate workspace manifests (mc-workspace::validate)
  3. Check for existing daemon.pid — error if already running
  4. Write daemon.pid to .mosaic/daemon.pid
  5. Register signal handlers (SIGTERM, SIGINT → graceful shutdown)
  6. Initialize hot cube cache (empty; cubes load on first request)
  7. Start Tessera schedule loop (subsume existing tessera daemon)
  8. Bind HTTP + MCP listeners on configured port
  9. Print banner:

     ┌─────────────────────────────────────────┐
     │  Mosaic daemon running                   │
     │  Port:      http://localhost:8787        │
     │  Workspace: ./acme-workspace             │
     │  Cubes:     3 registered, 0 loaded       │
     │  PID:       48291                        │
     │                                          │
     │  Press Ctrl+C to stop                    │
     └─────────────────────────────────────────┘

  10. Enter event loop (accept connections, process requests, fire schedules)
```

### Graceful shutdown

```
SIGTERM/SIGINT received:
  1. Set shutdown flag (AtomicBool — same pattern as tessera daemon)
  2. Stop accepting new connections
  3. Wait for in-flight requests to complete (max 30s)
  4. Flush pending ledger writes
  5. Write final cache state to .mosaic/cache-manifest.json (for fast restart)
  6. Remove daemon.pid
  7. Exit 0
```

---

## Part 3: Hot cube cache

### The core idea

Cubes are expensive to load (parse YAML → validate → compile → apply inputs). Once loaded, they should stay in memory until evicted. Queries hit warm cubes in nanoseconds instead of cold-loading in seconds.

### Cache structure

```rust
pub struct CubeCache {
    cubes: HashMap<CubeId, CachedCube>,
    budget_bytes: usize,           // configurable max memory for cached cubes
    current_bytes: usize,          // estimated current usage
    access_order: VecDeque<CubeId>, // LRU tracking
}

pub struct CachedCube {
    pub cube: Cube,
    pub refs: ModelRefs,
    pub workspace_id: String,
    pub loaded_at: Instant,
    pub last_accessed: Instant,
    pub revision: u64,
    pub estimated_bytes: usize,
}
```

### Load-on-first-request

Cubes are NOT loaded at startup (that would make startup slow for workspaces with many cubes). Instead:

1. Daemon starts → reads workspace manifest → registers cube paths
2. First API request targeting a cube → cold-load that cube → cache it
3. Subsequent requests → hit warm cache
4. If cache budget exceeded → LRU eviction (least-recently-accessed cube removed)

### Eviction policy

- **Budget-driven:** configurable max memory (default: 512MB for personal use; adjustable)
- **LRU:** least-recently-accessed cube is evicted first
- **Never evict during an active request:** if a cube is being queried, it can't be evicted
- **Eviction is soft:** cube can be re-loaded on next request (just cold again)

### Cache invalidation

Cache entries are invalidated (cube reloaded from disk) when:
- Model YAML file changes on disk (filesystem watcher, or manual `mc reload`)
- Tessera import runs (new data → cube revision bumps → cached cube is stale)
- `mc model write` is called via API (direct cell write → revision bump)
- `mc reload [--cube <name>]` is called explicitly

### Warm restart

On graceful shutdown, the daemon writes `.mosaic/cache-manifest.json`:
```json
{
  "cubes_loaded": ["marketing-finance", "brand-awareness"],
  "last_revision": { "marketing-finance": 47, "brand-awareness": 12 }
}
```

On restart, if model files haven't changed (mtime check), the daemon pre-loads these cubes to restore warm state quickly. This avoids cold-start latency on service restart/deploy.

---

## Part 4: API surface

### HTTP API (REST)

All endpoints mirror CLI verbs. JSON request/response. Same envelope schema.

```
POST /api/v1/query          → mc model query equivalent
POST /api/v1/whatif         → mc model whatif equivalent
POST /api/v1/trace          → mc model trace equivalent
POST /api/v1/sweep          → mc model sweep equivalent
POST /api/v1/diff           → mc model diff equivalent
POST /api/v1/write          → mc model write equivalent
POST /api/v1/narrate        → mc model narrate equivalent
POST /api/v1/narrate-trends → mc model narrate-trends equivalent

GET  /api/v1/cubes          → list loaded/registered cubes
GET  /api/v1/cubes/:name    → cube metadata (dimensions, measures, revision)
GET  /api/v1/health         → daemon health check
GET  /api/v1/status         → full daemon status (uptime, cache, schedules)

POST /api/v1/reload         → force-reload a cube from disk
POST /api/v1/snapshot       → take a snapshot of a cube
POST /api/v1/rollback       → roll back to a snapshot
```

**Request routing:** All verb endpoints accept a `cube` field identifying which cube to target:
```json
{
  "cube": "marketing-finance",
  "where": { "Time": "Q1_2025", "Market": "Houston" },
  "show": ["Spend", "Clicks", "ROAS"]
}
```

### MCP server

Same operations exposed as MCP tools. The daemon IS the MCP server — no separate process needed. AI agents connect directly to the daemon's MCP endpoint.

```
Tools:
  mosaic.query      — query cells from a loaded cube
  mosaic.whatif     — what-if analysis
  mosaic.trace      — trace computation chain
  mosaic.sweep      — parameter sweep
  mosaic.diff       — compare states
  mosaic.write      — write a cell value
  mosaic.narrate    — generate narratives
  mosaic.cubes      — list available cubes
  mosaic.status     — daemon status
  mosaic.reload     — reload a cube
```

### Schema versioning

API responses carry `schema_version` (currently "1.0" for most verbs, "1.1" for trace). The daemon locks to these same versions. Future API evolution uses new schema versions; old versions remain supported for 2 major releases.

---

## Part 5: Workspace discovery and org awareness

### How the daemon finds workspaces

Three modes (selected by startup flags):

**Mode 1: Single workspace (default)**
```
mc up
# Looks for workspace.yaml in current directory
# Loads that one workspace
```

**Mode 2: Org mode**
```
mc up --org ./my-org
# Reads org.yaml → discovers all workspaces listed in org.workspaces[]
# Registers all cubes from all workspaces
# API requests specify workspace + cube
```

**Mode 3: Multi-path**
```
mc up --workspace ./ws-a --workspace ./ws-b
# Loads multiple specific workspaces (no org required)
```

### API routing with org/workspace

In org mode, API requests must specify workspace:
```json
{
  "workspace": "client-a",
  "cube": "marketing-finance",
  "where": { ... }
}
```

In single-workspace mode, workspace is implied (omit the field).

### Org-level endpoints (org mode only)

```
GET  /api/v1/org             → org metadata
GET  /api/v1/workspaces      → list workspaces in org
GET  /api/v1/workspaces/:ws  → workspace metadata + cube list
```

---

## Part 6: Tessera integration (subsumes tessera daemon)

The Phase 5C tessera daemon (`crates/mc-tessera/src/schedule/daemon.rs`) runs as a standalone process today. Phase 8 subsumes it — the service daemon runs Tessera schedules internally.

**What this means:**
- No separate `mc tessera daemon` process needed
- The service daemon reads `.tessera/schedules.json` and fires due recipes
- Import results flow directly into the cached cube (warm update, no cold-reload)
- Import audit logs still write to `.tessera/audit.jsonl`

**The transition:**
- Phase 8 ships: `mc up` handles scheduling internally
- `mc tessera daemon` becomes a fallback for users who don't want the full daemon
- Both can't run simultaneously (PID check prevents it)

---

## Part 7: State directory (`.mosaic/` evolution)

### Current state (pre-daemon)

```
.mosaic/
├── analysis-ledger.jsonl      # narrative generation log (Phase 7A.2)
├── benchmark-library.json     # workspace-local percentile library (Phase 7A.4)
├── context-events.yaml        # operational annotations (Phase 7A.5)
└── pptx-profiles/             # PPTX template profiles (Phase 6E)
```

### Phase 8 additions

```
.mosaic/
├── analysis-ledger.jsonl      # (existing)
├── benchmark-library.json     # (existing)
├── context-events.yaml        # (existing)
├── pptx-profiles/             # (existing)
├── daemon.pid                 # PID file (prevents duplicate daemons)
├── daemon.log                 # Log output when --detach
├── cache-manifest.json        # Last warm cube set (for fast restart)
├── write-journal.jsonl        # In-flight writes (crash recovery)
└── daemon.toml                # Daemon configuration (port, cache budget, etc.)
```

### daemon.toml (configuration)

```toml
[daemon]
port = 8787
host = "127.0.0.1"           # localhost only by default; 0.0.0.0 for network
detach = false
log_level = "info"

[cache]
budget_mb = 512               # max memory for cached cubes
warm_restart = true           # pre-load previously-cached cubes on restart

[tessera]
schedule_enabled = true       # run tessera schedules in the daemon
max_concurrent_imports = 1    # one import at a time (Phase 8 is single-threaded)

[api]
cors_origins = ["http://localhost:*"]  # for Phase 6B web UI
request_timeout_ms = 30000
max_request_body_mb = 50
```

---

## Part 8: Crash recovery

### The write journal

Every write operation (via API or Tessera import) is first journaled to `.mosaic/write-journal.jsonl` BEFORE being applied to the in-memory cube. If the daemon crashes mid-write:

1. On restart, daemon reads write-journal.jsonl
2. Replays any entries that weren't confirmed (no corresponding "committed" entry)
3. Applies them to the re-loaded cube
4. Truncates the journal

This is a minimal WAL (write-ahead log). It guarantees that acknowledged writes are never lost, even on crash.

### What crashes can lose

- In-flight queries (not writes) — safe to lose; client retries
- Narrative generation in progress — safe; re-run on demand
- Tessera import in progress — safe; Tessera's own audit log tracks partial imports

### What crashes can NOT lose

- Any write that was acknowledged to the API client (journaled first)
- Ledger entries (appended atomically via tmp+rename — same as today)
- Tessera audit entries (same atomic append pattern)

---

## Part 9: How Grout integrates (Phase 8.5)

The daemon is where Grout's hash chains become live rather than theoretical. Specifically:

**Hash-chained writes:** Every write-journal entry includes `prev_hash`. The journal IS the hash chain. Tampering with persisted write history is detectable on restart.

**Signed exports:** The daemon serves `GET /api/v1/export` which produces a signed archive per the Grout spec (manifest.json + signature.sig). The workspace's Grout key signs the export.

**Canary checks:** On startup, the daemon verifies canary records in the ledger and workspace state. Missing canary = integrity violation → alert in daemon.log + health endpoint reports "integrity_warning".

**Grant verification:** In org mode, when a request specifies a workspace, the daemon verifies the caller's grant against the signed grant chain before serving data.

---

## Part 10: The Phase 6B connection

Phase 6B (web UI) was described as "`mc serve` runs locally; browser-based interaction." With Phase 8's daemon, the story becomes:

1. `mc up` starts the daemon (API + MCP)
2. The Phase 6B web UI is a static SPA that connects to the daemon's HTTP API
3. `mc up --ui` (or `mc up --static ./dist`) also serves the web UI bundle
4. The UI is a consumer of the daemon, not a separate process

This means Phase 6B doesn't need its own server — it rides on the daemon. The daemon becomes the single process that serves both API clients (agents, scripts) and human users (web UI).

---

## Part 11: Personal deployment story

For the project owner's personal use case (multiple domains — sports betting, marketing, investing):

```
# Start daemon pointing at personal org
mc up --org ~/mosaic-workspaces

# From anywhere (Tailscale / Cloudflare Tunnel):
curl http://my-machine:8787/api/v1/cubes
# → ["sports-betting/nba-totals", "marketing/acme-finance", "investing/portfolio"]

# Query a specific cube
curl -X POST http://my-machine:8787/api/v1/query \
  -d '{"workspace": "sports-betting", "cube": "nba-totals", ...}'
```

With Tailscale or Cloudflare Tunnel exposing port 8787, this gives "anywhere access" to all workspaces from any device. No cloud infrastructure needed — just the daemon on a machine you control.

---

## Part 12: Crate structure

```
crates/mc-daemon/
├── Cargo.toml          # deps: mc-core, mc-model, mc-workspace, mc-narrative,
│                       #        mc-tessera, tokio, axum, serde_json, tracing
├── src/
│   ├── lib.rs          # pub fn run(config: DaemonConfig)
│   ├── config.rs       # DaemonConfig parsing from daemon.toml + CLI flags
│   ├── cache.rs        # CubeCache (hot cube storage, LRU eviction)
│   ├── server.rs       # Axum HTTP server setup + route handlers
│   ├── mcp.rs          # MCP server implementation
│   ├── handlers/       # Per-verb API handlers (query, whatif, trace, ...)
│   │   ├── query.rs
│   │   ├── whatif.rs
│   │   ├── trace.rs
│   │   ├── sweep.rs
│   │   ├── diff.rs
│   │   ├── write.rs
│   │   ├── narrate.rs
│   │   └── admin.rs    # reload, snapshot, rollback, status
│   ├── journal.rs      # Write-ahead journal for crash recovery
│   ├── scheduler.rs    # Tessera schedule integration (subsumes tessera daemon)
│   ├── discovery.rs    # Workspace/org discovery from paths
│   └── signals.rs      # SIGTERM/SIGINT handling + graceful shutdown
└── tests/
    └── integration.rs  # Daemon startup, query, write, shutdown cycle
```

**Key dep note:** `tokio` and `axum` are permitted in `mc-daemon` per ADR-0025 Rule 1.6 (deployment shells may add async). The kernel calls remain sync; the daemon wraps them in `tokio::task::spawn_blocking()`.

---

## Part 13: Implementation sequencing

### Phase 8.0 — Minimal viable daemon (MVP)

Ship the smallest thing that's useful:

1. `mc up` starts a daemon serving one workspace
2. HTTP API for `query`, `write`, `trace` (the three most-used verbs)
3. Hot cube cache (load-on-first-request, LRU eviction)
4. `mc down` and `mc status`
5. Graceful shutdown with signal handling
6. Write journal for crash recovery
7. `daemon.toml` for basic configuration

**This is usable for personal deployment.** One machine, one workspace, warm cubes, HTTP access.

### Phase 8.1 — Full API + org mode

8. All verb endpoints (whatif, sweep, diff, narrate, narrate-trends)
9. MCP server
10. Org mode (`--org` flag, multi-workspace routing)
11. Tessera schedule integration (subsume tessera daemon)
12. Warm restart (cache-manifest.json)
13. `mc ps` and `mc reload`

### Phase 8.5 — Grout integration

14. Hash-chained write journal
15. Signed exports via API
16. Canary checks on startup
17. Grant verification (org mode)
18. `mc grout verify` command

---

## Part 14: What inherits from existing code

| Existing code | Reuse in Phase 8 |
|---|---|
| `mc-demo-server` (axum + tokio) | Server scaffolding pattern; axum route setup |
| Tessera daemon (signal handling, PID file) | Signal handler, PID management, schedule loop |
| `mc-cli` verb implementations | Business logic; daemon handlers call same functions |
| `.mosaic/` directory convention | State directory; daemon adds entries |
| `mc-workspace` (Phase 4C) | Workspace discovery and manifest loading |

The daemon is NOT a rewrite of the demo server. It's a new crate that inherits patterns from both the demo server (axum/tokio/routes) and the tessera daemon (signals/PID/scheduling).

---

## Part 15: Open questions for the Phase 8 ADR

1. **Port number.** 8787 (MOSA on keypad) or 3000 (convention) or configurable-with-no-default? Recommend: 8787 default, configurable in daemon.toml.

2. **Authentication.** Phase 8 has no auth (single-user, localhost). Phase 9 adds it. But should Phase 8 support optional API key auth (`--api-key <key>`) for Tailscale/tunnel deployments? Probably yes — lightweight bearer token check before Phase 9's full auth.

3. **Filesystem watcher vs manual reload.** Should the daemon watch model files for changes and auto-reload? Or require explicit `mc reload`? Recommend: manual reload for v1 (filesystem watchers are OS-specific and finicky). Auto-reload is Phase 8.1 enhancement.

4. **Concurrent requests.** Cubes are `&mut self` for reads (per CLAUDE.md §2.15). This means concurrent reads on the same cube need serialization. Options: (a) per-cube mutex (simple, some contention), (b) per-cube read/write lock (readers share, writers exclusive), (c) clone-on-read (expensive). Recommend: (a) per-cube mutex for v1. Contention is minimal for personal use.

5. **Web UI bundling.** Should `mc up --ui` serve a bundled web UI? Or should the UI be a separate `npm run dev` process during development? Recommend: `--static <dir>` flag (same as demo server), with a future `mc up --ui` that serves an embedded bundle once Phase 6B ships a production build.

6. **Daemon binary.** Should the daemon be `mc up` (same binary, subcommand) or `mosaic-daemon` (separate binary)? Recommend: same binary. `mc up` is a subcommand of `mc`. One install, one binary, all commands. The daemon code lives in `mc-daemon` crate but compiles into the `mc` binary.

---

## Cross-links

- **ADR-0025 Decision 2, Shape 4:** The deployment shape this phase implements
- **ADR-0025 Decision 3:** Caching strategy rules (coordinate+revision, budget-driven, not exhaustive)
- **ADR-0026:** Org/workspace architecture the daemon is aware of
- **Phase 4C handoff:** Workspace primitive the daemon inherits
- **Grout research note:** `docs/research-notes/grout-security-architecture-vision.md` — Phase 8.5 integration
- **Tessera daemon:** `crates/mc-tessera/src/schedule/daemon.rs` — subsume into Phase 8
- **Demo server:** `crates/mc-demo-server/` — pattern reference (axum + tokio)
- **Vision doc Part 4:** `docs/strategy/mosaic-architecture-and-vision.md` — Shape 4 narrative

- **TM1 comparison:** `docs/strategy/mosaic-vs-tm1-strategic-comparison.md` — TM1 daemon-mode optimizations (multi-level caching, lazy consolidation) inform Phase 8 priorities

---

## Part 16: Amendments from Claude Desktop review (2026-05-10)

These resolve the open questions and address specific concerns raised during review. The ADR incorporates these as binding decisions.

### Amendment 1: Concurrency model — per-cube channel (RESOLVED)

**Verified:** `Cube::read()` takes `&mut self` (cube.rs line 164). Reads mutate the cube (lazy graph population + caching computed values + dirty flag clearing). Making reads `&self` would require major kernel refactoring — out of scope for Phase 8.

**Decision: Per-cube tokio channel (Option A — sequential access per cube).**

Each cube gets a dedicated tokio task. All requests targeting that cube are sent via an mpsc channel to that task. The task processes them sequentially. Different cubes run on different tasks → true parallelism across cubes, sequential within a cube.

```rust
// Conceptual shape
struct CubeActor {
    cube: Cube,
    refs: ModelRefs,
    rx: mpsc::Receiver<CubeRequest>,
}

enum CubeRequest {
    Query { params: QueryParams, reply: oneshot::Sender<QueryResult> },
    Write { params: WriteParams, reply: oneshot::Sender<WriteResult> },
    // ...
}
```

**Latency implication:** Concurrent requests to the SAME cube serialize. A dashboard with 5 widgets querying the same cube → widgets render sequentially (~5× latency vs parallel). For personal use this is acceptable. For production multi-user: Phase 9 explores MVCC or kernel `&self` reads.

**Mitigation for dashboard case:** The daemon can batch multiple pending queries into one pass — group concurrent waiting requests for the same cube, execute them in sequence within one task wakeup, return all results together. This amortizes scheduling overhead even though execution is sequential.

**What this means for the implementer:**
- Each loaded cube lives in its own tokio task behind an mpsc channel
- HTTP handlers send requests to the appropriate cube's channel and await the reply
- Tokio's task scheduler provides cross-cube parallelism for free
- No `Arc<Mutex<Cube>>` — cleaner and avoids async mutex poison

### Amendment 2: Warm restart — content hashes, no default pre-loading (RESOLVED)

**Problem with mtime:** Filesystem mtime is unreliable (clock skew, NFS, git checkout, file copy preserves timestamps). Content hash is authoritative.

**Decision:**
- `cache-manifest.json` records **content hash (SHA-256)** of each cube's model YAML, not mtime
- On restart, daemon reads the manifest but does NOT pre-load cubes by default
- Cubes are registered (paths known) but stay cold until first request (same lazy-load as fresh start)
- The manifest is informational: "these are the cubes the previous session was using"
- **Opt-in pre-loading:** `mc up --preload marketing-finance,brand-awareness` for specific cubes, or `daemon.toml` `preload_cubes = [...]` for deployments that want zero cold-start latency
- On pre-load, verify content hash matches before loading — if hash changed, cold-load instead (model was edited while daemon was down)

### Amendment 3: Write journal rigor (RESOLVED)

**Ordering:** Queries to a cube BLOCK while a write-apply is in progress. Write-apply is atomic from the perspective of concurrent queries. The per-cube channel serialization (Amendment 1) makes this natural — write request is processed, cube mutates, then the next request (which might be a query) sees the new state.

**Journal rotation:** Active journal is `write-journal.jsonl`. When it exceeds 10MB, rotate to `write-journal-{timestamp}.jsonl` and start fresh. Old segments are deleted after the next graceful shutdown confirms all writes are durable (cube snapshot taken).

**Crash during journal write:** On replay, parse each line. If the last line is truncated/malformed (incomplete JSON), ignore it — that write was never acknowledged to the client (acknowledgment happens AFTER successful journal write). The client will retry. Add a log warning: "Ignoring truncated journal entry (crash during write)."

**Journal entry format:**
```json
{"ts": "2026-05-10T14:30:00Z", "cube": "marketing-finance", "coord": [...], "value": 15000.0, "status": "pending"}
{"ts": "2026-05-10T14:30:00Z", "cube": "marketing-finance", "coord": [...], "value": 15000.0, "status": "committed"}
```

On restart: entries with "pending" but no corresponding "committed" are replayed. Entries with "committed" are safe to ignore (already applied).

### Amendment 4: Auth defaults — API key from day one (RESOLVED)

**Decision:** Phase 8 ships with optional bearer-token auth.

- `mc up --api-key <key>` enables authentication
- Without `--api-key`, daemon **refuses to bind to non-localhost addresses**. If `--host 0.0.0.0` is set without `--api-key`, daemon exits with error: "Refusing to bind to non-localhost without --api-key. Set an API key for network-exposed deployments."
- With `--api-key`, all requests must include `Authorization: Bearer <key>` header. Requests without it get 401.
- API key is also configurable in `daemon.toml`: `api_key = "..."` (alternative to CLI flag)
- This is NOT full auth (no users, no sessions, no RBAC) — just a shared secret. Phase 9 adds real auth.
- Health endpoint (`/api/v1/health`) is exempt from auth (needed for monitoring)

### Amendment 5: Tessera daemon transition plan (RESOLVED)

**Phase 8 transition path:**
1. `mc up` checks for `.tessera/daemon.pid` on startup. If a tessera daemon is running → error: "Tessera daemon is running (PID {pid}). Stop it with `mc tessera daemon --stop` before starting the service daemon."
2. `mc up` handles all Tessera schedules internally (reads `.tessera/schedules.json`, fires due recipes, updates schedule state)
3. `mc tessera daemon` remains functional as a standalone fallback for users who don't want the full daemon
4. Phase 9+: `mc tessera daemon` becomes a compatibility shim that prints "Use `mc up` instead" and exits

**What happens to existing schedules:** They continue to work under `mc up`. Same `schedules.json` format, same schedule IDs, same behavior. The only difference is which process fires them.

### Amendment 6: Additional resolved decisions

**MCP naming:** All MCP tools match the HTTP API path segments exactly. `mosaic.narrate_trends` (not `narrate-trends` — MCP tools use underscores). Add `mosaic.narrate_trends` to the MCP tool list.

**Write-tool gating:** `mosaic.write` MCP tool requires `write_enabled = true` in `daemon.toml` (default: `false`). This prevents AI agents from accidentally modifying cube state. Explicit opt-in for write operations via MCP. HTTP API writes are always enabled (human-initiated).

**`mc ps` shows:** cube name, workspace, state (warm/cold/loading), cache size (bytes), last access time, revision number. Not active requests (too transient to be useful in a CLI snapshot).

**Request timeout:** Default 60s (not 30s). Sweep and narrate can be slow. Per-verb override in `daemon.toml`:
```toml
[timeouts]
default_ms = 60000
sweep_ms = 120000
narrate_ms = 90000
```

**Max request body:** 10MB default (not 50MB). Tessera imports go through the Tessera recipe path, not the HTTP body. The only large body is `mc model write` with many cells, which is typically < 1MB. 50MB was too permissive.

### Amendment 7: Non-goals (explicit)

Phase 8 daemon is NOT:
- A database (cube state lives in files; daemon is a cache layer)
- A multi-process system (single process, multiple tasks)
- Horizontally scalable (one daemon per machine; scale-out is Phase 9)
- A security boundary (API key is a speed bump, not real auth — Phase 9 adds real auth)
- A long-term storage layer (files are authoritative; daemon serves from cache)
- A full web application server (serves API; web UI is a static bundle)
- Multi-tenant (one org per daemon; multi-tenancy is Phase 9)

### Amendment 8: Error handling decisions

| Scenario | Behavior |
|---|---|
| Cube fails to load (malformed YAML) | Cube stays cold. Error logged. Health endpoint reports "degraded" with failing cube name. Requests to that cube get 503 with diagnostic. |
| Write fails after journaling | Journal entry stays "pending." On next retry (or restart), replayed. Client gets error; can retry. |
| Tessera recipe malformed | Schedule fires → error logged → schedule `failure_count` increments → after 2 consecutive failures, schedule paused. Same behavior as existing tessera daemon. |
| Cube file deleted while warm | Next reload attempt fails → cube evicted from cache → requests get 404. Cube removed from `mc ps` listing. |
| Out of cache budget during load | Evict LRU cube before loading new one. If no cube can be evicted (all actively in use), queue the load until one frees. |

---

**End of research note. When Phase 8 ADR drafts, this note is the starting point. The binding decisions live in the ADR; this note captures design intent, resolved questions, and the amendments from Claude Desktop review.**
