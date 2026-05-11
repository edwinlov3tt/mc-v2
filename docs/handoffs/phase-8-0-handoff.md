# Phase 8.0 Handoff — Service Daemon MVP

**Status:** Proposed (next to start)
**Date:** 2026-05-10
**ADR:** [ADR-0029](../decisions/0029-phase-8-service-daemon.md) (Proposed — accept before implementation)
**Research note:** [mosaic-service-daemon.md](../research-notes/mosaic-service-daemon.md)
**Prerequisites:** Phase 4C (complete), cross-coord dep-graph fix (complete)
**Estimated effort:** 4–6 sessions
**Crate:** `mc-daemon` (new — deployment shell; tokio + axum permitted per ADR-0025 Rule 1.6)
**Branch:** `phase-8/daemon-mvp`

---

## What this phase ships

The smallest useful daemon: `mc up` starts a persistent HTTP service that hot-loads cubes, caches them in memory, serves query/write/trace via REST, and survives crashes.

After this phase, you can:
```bash
mc up
# In another terminal:
curl -X POST http://localhost:8787/api/v1/query \
  -d '{"cube":"marketing-finance","where":{"Time":"Q1_2025"},"show":["Spend","Revenue"]}'
# → instant response from warm cache
```

---

## Phase 8.0 MVP scope (what to build)

| # | Feature |
|---|---|
| 1 | `mc up` / `mc down` / `mc status` commands |
| 2 | HTTP API: `POST /api/v1/{query,write,trace}` + `GET /api/v1/{health,status,cubes}` |
| 3 | Per-cube actor model (tokio mpsc channel, sequential within cube) |
| 4 | Hot cube cache (load-on-first-request, LRU eviction, budget-driven) |
| 5 | Write-ahead journal (single-cell writes, crash recovery) |
| 6 | Durability handoff to `.tessera/writes.jsonl` (four-source model preserved) |
| 7 | Signal handling + graceful shutdown (Ctrl+C, SIGTERM) |
| 8 | `daemon.toml` configuration |
| 9 | Optional API key auth (refuse non-localhost without key) |
| 10 | Single-workspace mode only |

## What is NOT in Phase 8.0

- MCP server (Phase 8.1)
- Org mode / multi-workspace (Phase 8.1)
- Tessera schedule integration (Phase 8.1)
- `mc ps` / `mc reload` (Phase 8.1)
- Warm restart with content hashes (Phase 8.1)
- Tessera batch journal protocol (Phase 8.1)
- Grout integration (Phase 8.5)
- All other verb endpoints beyond query/write/trace (Phase 8.1)

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│  mc-daemon crate                                 │
├─────────────────────────────────────────────────┤
│  server.rs        Axum HTTP server + routes      │
│  config.rs        daemon.toml parsing + CLI args │
│  actor.rs         Per-cube actor (mpsc + spawn_  │
│                   blocking for cube ops)          │
│  cache.rs         CubeCache (LRU, budget, reg)   │
│  journal.rs       Write-ahead journal (JSONL)    │
│  handlers/        Per-verb HTTP handlers         │
│    query.rs       POST /api/v1/query             │
│    write.rs       POST /api/v1/write             │
│    trace.rs       POST /api/v1/trace             │
│    admin.rs       GET health/status/cubes        │
│  signals.rs       SIGTERM/SIGINT handling        │
│  auth.rs          Bearer token middleware        │
├─────────────────────────────────────────────────┤
│  mc-workspace     Workspace discovery + loading  │
│  mc-model         Parse → validate → compile     │
│  mc-core          Cube engine (untouched)        │
└─────────────────────────────────────────────────┘
```

---

## Key design decisions (from ADR-0029)

### Per-cube actor

Each loaded cube lives in its own tokio task behind an mpsc channel. HTTP handlers send requests to the channel and await a reply via oneshot. Cube operations run via `spawn_blocking` (they're sync + CPU-bound).

```rust
struct CubeActor {
    cube: Cube,
    refs: ModelRefs,
    workspace_path: PathBuf,
    journal: WriteJournal,
}

enum CubeRequest {
    Query { params: QueryParams, reply: oneshot::Sender<Result<QueryResult>> },
    Write { params: WriteParams, reply: oneshot::Sender<Result<WriteResult>> },
    Trace { params: TraceParams, reply: oneshot::Sender<Result<TraceResult>> },
}
```

### Write path (journal + durability)

```
Client POST /api/v1/write
  → auth check (if api_key set)
  → route to cube actor via channel
  → actor receives request
  → spawn_blocking:
      1. Write "pending" entry to .mosaic/write-journal.jsonl
      2. Apply write to cube (Cube::write)
      3. Append to .tessera/writes.jsonl (durable four-source persistence)
      4. Write "committed" entry to journal
  → reply to client with success
```

If crash between step 1 and step 4: on restart, replay pending entries.

### Journal format

`.mosaic/write-journal.jsonl` — workspace-qualified:
```json
{"seq":1,"ts":"2026-05-10T14:30:00Z","workspace":"./","cube":"marketing-finance","coord":["Baseline","Working","Q1_2025","Paid_Search","Houston","Spend"],"value":15000.0,"status":"pending"}
{"seq":1,"ts":"2026-05-10T14:30:00Z","workspace":"./","cube":"marketing-finance","coord":["Baseline","Working","Q1_2025","Paid_Search","Houston","Spend"],"value":15000.0,"status":"committed"}
```

### Cache

```rust
struct CubeCache {
    registered: HashMap<CubeKey, CubeRegistration>,  // all known cubes (cold or warm)
    actors: HashMap<CubeKey, mpsc::Sender<CubeRequest>>,  // warm cubes with running actors
    budget_bytes: usize,
    current_bytes: usize,
}

struct CubeKey {
    workspace_path: PathBuf,
    cube_name: String,
}

struct CubeRegistration {
    model_path: PathBuf,
    state: CubeState,  // Cold | Warm { loaded_at, last_accessed, estimated_bytes }
}
```

- First request → cold-load (parse + validate + compile + apply inputs) → start actor → cache
- LRU eviction when budget exceeded: stop actor, drop cube, mark cold
- Never evict cube with in-flight requests

### Auth middleware

```rust
async fn auth_layer(req: Request, next: Next) -> Response {
    // Skip auth for /api/v1/health
    if req.uri().path() == "/api/v1/health" { return next.run(req).await; }
    
    // If no api_key configured, allow all (localhost-only binding enforced at startup)
    let Some(expected) = &config.api_key else { return next.run(req).await; };
    
    // Check Bearer token
    match req.headers().get("authorization") {
        Some(v) if v == format!("Bearer {expected}") => next.run(req).await,
        _ => (StatusCode::UNAUTHORIZED, "Missing or invalid API key").into_response(),
    }
}
```

### Startup sequence

```
mc up [--port 8787] [--workspace .] [--api-key <key>] [--detach]
  1. Load daemon.toml (merge with CLI flags; CLI wins)
  2. Validate: if host is non-localhost and no api_key → exit with error
  3. Discover workspace (read workspace.yaml from --workspace path)
  4. Register all cubes from workspace manifest (cold — don't load yet)
  5. Check for existing .mosaic/daemon.pid → error if running
  6. Write .mosaic/daemon.pid
  7. Replay write-journal.jsonl (uncommitted entries → load affected cubes → apply)
  8. Register signal handlers (SIGTERM, SIGINT → graceful shutdown)
  9. Start Axum server on configured port
  10. Print banner (port, workspace, cube count, PID)
  11. Enter event loop
```

### Graceful shutdown

```
Signal received (or mc down sends SIGTERM):
  1. Set shutdown flag (AtomicBool)
  2. Stop accepting new HTTP connections
  3. Wait for in-flight requests (max 30s timeout)
  4. Send shutdown message to all cube actors
  5. Actors flush any pending state
  6. Remove .mosaic/daemon.pid
  7. Exit 0

Double Ctrl+C (within 5s of first):
  → Forced exit (no drain, exit 1)
```

---

## daemon.toml

```toml
[daemon]
port = 8787
host = "127.0.0.1"
api_key = ""            # empty = no auth (localhost only)

[cache]
budget_mb = 512

[timeouts]
default_ms = 60000
sweep_ms = 120000
narrate_ms = 90000

[logging]
format = "auto"         # auto | json | pretty
level = "info"

[api]
max_request_body_mb = 10
cors_origins = "auto"   # auto = localhost origins when local; empty for non-localhost
```

---

## API endpoints (Phase 8.0 MVP)

### `POST /api/v1/query`

Request:
```json
{
  "cube": "marketing-finance",
  "where": { "Time": "Q1_2025", "Market": "Houston" },
  "show": ["Spend", "Clicks", "Revenue"]
}
```

Response (same schema_version envelope as CLI):
```json
{
  "schema_version": "1.0",
  "results": [
    { "coord": {...}, "values": { "Spend": 15000.0, "Clicks": 12500.0, "Revenue": 45000.0 } }
  ]
}
```

### `POST /api/v1/write`

Request:
```json
{
  "cube": "marketing-finance",
  "coord": ["Baseline", "Working", "Q1_2025", "Paid_Search", "Houston", "Spend"],
  "value": 16000.0
}
```

Response:
```json
{
  "schema_version": "1.0",
  "status": "ok",
  "revision_after": 48,
  "dirty_count": 5
}
```

### `POST /api/v1/trace`

Request:
```json
{
  "cube": "marketing-finance",
  "coord": ["Baseline", "Working", "Q1_2025", "Paid_Search", "Houston", "Revenue"]
}
```

Response: Same trace JSON structure as `mc model trace --format json`.

### `GET /api/v1/health` (no auth required)

Response: `{"status": "healthy", "uptime_seconds": 3600}`

### `GET /api/v1/status` (auth required)

Response: Full diagnostics (cube counts, cache usage, journal state, degraded cubes).

### `GET /api/v1/cubes`

Response: `{"cubes": [{"name": "marketing-finance", "state": "warm", "revision": 47}]}`

---

## Dependencies

```toml
# crates/mc-daemon/Cargo.toml
[dependencies]
mc-core = { path = "../mc-core" }
mc-model = { path = "../mc-model" }
mc-workspace = { path = "../mc-workspace" }
tokio = { version = "1", features = ["full"] }
axum = "0.7"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tower-http = { version = "0.5", features = ["cors"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }
```

Add `mc-daemon` to workspace members in root `Cargo.toml`. Wire `mc up`/`mc down`/`mc status` as subcommands in `crates/mc-cli/src/main.rs` that call `mc_daemon::run()`.

---

## Tests

### Unit tests (in mc-daemon)

- Config parsing (daemon.toml + CLI flag merge)
- Auth middleware (valid key, invalid key, missing key, health exempt)
- Journal write + commit + recovery (pending without committed = replay)
- Journal truncated last line = ignored
- Cache registration + cold-load + eviction
- Startup refuses non-localhost without api_key

### Integration tests

- Start daemon → query → get result → shutdown
- Start daemon → write → query → see updated value
- Start daemon → write → kill process → restart → verify write persisted
- Start daemon → exceed cache budget → verify LRU eviction
- Start daemon without api_key on localhost → works
- Start daemon without api_key on 0.0.0.0 → refuses to start
- Double Ctrl+C → forced exit

---

## Acceptance criteria

**Functional:**
1. `mc up` starts, binds port, prints banner
2. `mc down` graceful shutdown (flush, remove PID)
3. `mc status` reports health/not-running
4. `POST /api/v1/query` returns correct values from warm cube
5. `POST /api/v1/write` journals → applies → persists to `.tessera/writes.jsonl`
6. `POST /api/v1/trace` returns computation trace
7. Cubes load on first request (not at startup)
8. LRU eviction when budget exceeded
9. Auth: refuses non-localhost without key (IPv4 + IPv6 checked)
10. Crash + restart replays uncommitted journal entries
11. `GET /api/v1/health` responds without auth (minimal payload)
12. Ctrl+C = graceful; double = forced
13. All existing tests pass unchanged
14. `cargo test --workspace` passes
15. `cargo clippy --all-targets --workspace -- -D warnings` passes
16. No changes to `mc-core`

**Performance:**
17. Cold-load (Acme cube, first query): < 2 seconds
18. Warm-query (single-cell read): < 10ms p99
19. Write throughput (sustained): > 500 writes/second
20. Cache stays within budget under load

---

## Cross-links

- **ADR-0029:** All binding decisions
- **ADR-0025 Rule 1.6:** Deployment shells may use tokio/axum
- **ADR-0026:** Org/workspace model (Phase 8.1 inherits)
- **Research note:** `docs/research-notes/mosaic-service-daemon.md`
- **Tessera daemon:** `crates/mc-tessera/src/schedule/daemon.rs` (signal handling pattern)
- **Demo server:** `crates/mc-demo-server/` (axum scaffolding pattern)
- **Four-source model:** `docs/process-notes.md` Rule 11

---

**End of handoff. Phase 8.0 MVP: three verbs, one workspace, hot cache, crash recovery, API key auth. Ship it, then Phase 8.1 adds the rest.**
