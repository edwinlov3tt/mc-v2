# ADR-0029: Phase 8 — Mosaic Service Daemon

**Status:** Proposed
**Date:** 2026-05-10
**Deciders:** project owner
**Phase:** 8 (first real service deployment — ADR-0025 Shape 4)
**Crate:** `mc-daemon` (new deployment shell; tokio + axum permitted per ADR-0025 Rule 1.6)
**Prerequisites:** Phase 4C (complete), cross-coord dep-graph fix (complete), ADR-0025 (accepted)
**Research note:** `docs/research-notes/mosaic-service-daemon.md` (full design context + amendments)

---

## Context

Mosaic is a CLI tool today. Users run `mc model query`, get a result, the process exits. There is no persistent state between commands — every invocation cold-loads the model from YAML, compiles it, and evaluates.

Phase 8 introduces the **service daemon** — a long-running process that keeps cubes hot in memory, serves an HTTP + MCP API, handles Tessera scheduling, and survives restarts. This is the deployment shape that makes Mosaic usable for:
- Interactive grid editing (Phase 6B web UI connecting to the daemon)
- AI agent workflows (MCP tools querying the daemon directly)
- Personal "anywhere access" (daemon + Tailscale/tunnel)
- Scheduled data ingestion (Tessera cron without a separate process)

The daemon is Shape 4 in ADR-0025's deployment sequence. It's the bridge between "developer tool on your laptop" and "production cloud service" (Phase 9).

---

## Decisions

### Decision 1: Command — `mc up` / `mc down` / `mc status` / `mc ps`

| Command | What it does |
|---|---|
| `mc up` | Start the daemon (foreground by default; `--detach` for background) |
| `mc down` | Graceful shutdown (SIGTERM to PID in `.mosaic/daemon.pid`) |
| `mc status` | Daemon health: uptime, port, loaded cubes, cache utilization |
| `mc ps` | List cubes: name, workspace, state (warm/cold), cache size, last access, revision |

Default port: **8787**. Configurable via `--port` or `daemon.toml`.

The daemon is a subcommand of the existing `mc` binary. One install, one binary, all commands. The `mc-daemon` crate compiles into `mc`.

### Decision 2: Concurrency — per-cube actor (sequential within, parallel across)

`Cube::read()` takes `&mut self`. This is a kernel design invariant (CLAUDE.md §2.15) — reads mutate the cube (lazy graph population, caching, dirty-flag clearing). Making reads `&self` would require major kernel refactoring; that's Phase 9+ territory.

**Model: per-cube actor via tokio mpsc channel.**

Each loaded cube lives in its own tokio task. All requests targeting that cube are sent via a channel. The task processes them sequentially. Different cubes run on different tasks — true parallelism across cubes.

```rust
struct CubeActor {
    cube: Cube,
    refs: ModelRefs,
    rx: mpsc::Receiver<CubeRequest>,
}
```

**Consequence:** Concurrent requests to the SAME cube serialize. Dashboard with 5 widgets → widgets render sequentially. For personal single-user deployment: acceptable. For production multi-user: Phase 9 explores MVCC or kernel `&self` reads.

**No `Arc<Mutex<Cube>>`** — the actor model is cleaner and avoids async-mutex footguns.

**Tokio runtime guidance:** Cube operations (read, write, eval) are synchronous and potentially CPU-heavy (large cubes, deep rule chains). The per-cube actor task should use `tokio::task::spawn_blocking` for the actual cube operations to avoid starving the tokio worker pool. The actor loop itself is async (receives from channel), but dispatches work to the blocking pool:

```rust
async fn actor_loop(mut rx: mpsc::Receiver<CubeRequest>, mut cube: Cube) {
    while let Some(req) = rx.recv().await {
        // Dispatch to blocking pool — cube ops are sync + CPU-bound
        let result = tokio::task::spawn_blocking(move || {
            // Execute the cube operation (read, write, etc.)
            let r = handle_request(&mut cube, &req);
            (cube, r)  // Move cube back out
        }).await.unwrap();
        cube = result.0;
        req.reply(result.1);
    }
}
```

This ensures that even CPU-heavy cube evaluations (e.g., consolidation over deep hierarchies) don't block other cubes' actors or the HTTP accept loop.

### Decision 3: Hot cube cache — load-on-first-request, LRU eviction

Cubes are NOT loaded at startup. They're registered (paths known) but stay cold. First API request targeting a cube triggers cold-load → cache it. Subsequent requests hit warm cache.

**Eviction:** LRU (least-recently-accessed). Configurable budget (default 512MB). When budget exceeded, evict least-recent cube. Never evict during an active request.

**Cache invalidation — two distinct mechanisms:**

1. **Revision invalidation (hot cube stays loaded, stale cells recompute):** After an API write or Tessera import, the cube remains in the actor — its revision bumps, the dependency graph marks affected cells dirty, and subsequent reads recompute only those cells. The cube is NOT reloaded from disk. This is the normal path for data changes.

2. **Reload (evict + cold-load from disk):** Only triggered by explicit `mc reload [--cube <name>]`. This re-reads the model YAML, recompiles, and replaces the actor's cube. Used when the model definition changes (new rules, new dimensions), not when cell data changes.

**No filesystem watcher in Phase 8.** Manual reload only. Auto-reload is Phase 8.1 enhancement.

### Decision 4: Warm restart — content hashes, opt-in pre-loading

On graceful shutdown, write `.mosaic/cache-manifest.json` recording **workspace-qualified cube identifiers** + **content hash (SHA-256)** of each cube's model YAML (not mtime — mtime is unreliable). Entries are keyed by `{workspace_path, cube_name, model_path}` to handle org mode where two workspaces may have cubes with the same name.

On restart:
- Daemon reads manifest but does NOT pre-load cubes by default
- Cubes stay cold until first request (same lazy behavior as fresh start)
- The manifest is informational only

**Opt-in pre-loading:** `mc up --preload marketing-finance` or `daemon.toml`: `preload_cubes = ["marketing-finance"]`. On pre-load, verify content hash — if hash differs, skip pre-load and leave cube cold (see Decision 14). First request will cold-load from current YAML.

### Decision 5: HTTP API — mirrors CLI verbs

All endpoints mirror CLI verbs. JSON request/response. Same envelope schema.

```
POST /api/v1/query              POST /api/v1/whatif
POST /api/v1/trace              POST /api/v1/sweep
POST /api/v1/diff               POST /api/v1/write
POST /api/v1/narrate            POST /api/v1/narrate-trends
POST /api/v1/reload             POST /api/v1/snapshot
POST /api/v1/rollback

GET  /api/v1/cubes              GET  /api/v1/cubes/:name
GET  /api/v1/health             GET  /api/v1/status
GET  /api/v1/workspaces         GET  /api/v1/workspaces/:ws  (org mode)
GET  /api/v1/org                                              (org mode)
```

Requests specify `cube` (and `workspace` in org mode):
```json
{ "cube": "marketing-finance", "where": { "Time": "Q1_2025" }, "show": ["Spend"] }
```

### Decision 6: MCP server — same operations, tool naming

The daemon IS the MCP server. Tools use underscore naming:

```
mosaic.query          mosaic.whatif          mosaic.trace
mosaic.sweep          mosaic.diff            mosaic.write
mosaic.narrate        mosaic.narrate_trends  mosaic.cubes
mosaic.status         mosaic.reload
```

**Write gating:** `mosaic.write` requires `write_enabled = true` in `daemon.toml` (default: `false`). Prevents AI agents from accidentally modifying cube state. HTTP writes are always enabled.

### Decision 7: Authentication — API key from day one

Phase 8 ships with optional bearer-token auth:

- `mc up --api-key <key>` enables authentication
- **Without `--api-key`, daemon refuses to bind to non-localhost.** The check covers: any `--host` value other than `127.0.0.1`, `localhost`, or `::1` (IPv6 localhost) requires `--api-key`. This includes `0.0.0.0`, specific LAN IPs (`192.168.x.x`), and any other non-loopback address.
- With `--api-key`, all requests require `Authorization: Bearer <key>`. Missing → 401.
- Health endpoint (`/api/v1/health`) exempt from auth (needed for monitoring/healthchecks)
- This is a shared secret, not real auth — Phase 9 adds users/sessions/RBAC

**Why HTTP writes are always enabled but MCP writes are gated (Decision 6):** HTTP clients are explicitly invoked by humans or scripts with visible intent. MCP agents may operate without human review. The asymmetry is intentional — gate the path where unreviewed writes are most likely.

### Decision 8: Write-ahead journal — crash recovery

Every write is journaled BEFORE cube mutation. Per-cube sequential access (Decision 2) makes ordering natural — queries never see partially-applied writes.

**Journal writes happen inside the actor task (Option A).** The actor receives a write request, journals it with a monotonic sequence number, applies it to the cube, marks it committed. This keeps journal ordering and cube state in lockstep within the same task — no cross-task coordination needed. Journal latency is in the cube's critical path but SSD writes are <1ms; this is acceptable for Phase 8's single-user target.

**Sequence numbers:** Each journal entry gets a monotonic sequence number assigned in the actor. On crash recovery, entries are replayed in sequence order regardless of flush ordering. If two pending entries exist for the same cell, the higher sequence number wins.

**Journal format:** `.mosaic/write-journal.jsonl`

Journal entries are **workspace-qualified** — in org mode, two workspaces may have cubes with the same name. The full key is `(workspace_path, cube_name)`:

```json
{"seq":1,"ts":"...","workspace":"./","cube":"marketing-finance","coord":[...],"value":15000.0,"status":"pending"}
{"seq":1,"ts":"...","workspace":"./","cube":"marketing-finance","coord":[...],"value":15000.0,"status":"committed"}
{"seq":2,"ts":"...","workspace":"workspaces/client-a","cube":"plan","coord":[...],"value":8200.0,"status":"pending"}
```

**Durability handoff to the four-source model (critical — GPT P0 #1):** The daemon journal is crash recovery, NOT long-term persistence. After a write is applied to the cube, the daemon ALSO appends it to `.tessera/writes.jsonl` (the existing post-hoc write log that is source #3 in the four-source model: compiled YAML, canonical inputs, Tessera imports, post-hoc writes). The acknowledgment to the client happens only after BOTH the journal "committed" entry AND the `.tessera/writes.jsonl` append succeed. This ensures that the four-source model remains the authoritative persistence story. The daemon journal exists only for crash recovery of in-flight writes — not as a new fifth source of truth.

**On crash restart:** Replay entries with "pending" but no "committed," in sequence order. If two pending entries target the same cell, higher `seq` wins. Ignore truncated last line (write never acknowledged to client → client retries). Replayed writes are also appended to `.tessera/writes.jsonl` during replay.

**Rotation:** At 10MB, rotate to `write-journal-{timestamp}.jsonl`. Old segments deleted on next graceful shutdown.

**Tessera bulk writes (Decision 8a — Phase 8.1 scope):** Tessera imports produce potentially thousands of cell updates. Writing each individually to the journal would produce enormous journal volume and unacceptable latency. Instead, Tessera imports use a **batch protocol**:

```json
{"seq":100,"ts":"...","workspace":"./","cube":"marketing-finance","type":"batch_begin","import_id":"imp_abc","row_count":5000,"sidecar":".tessera/imports/imp_abc.cells.jsonl"}
... (no per-cell journal entries during batch — cells written to sidecar) ...
{"seq":100,"ts":"...","workspace":"./","cube":"marketing-finance","type":"batch_commit","import_id":"imp_abc","content_hash":"sha256:abc123"}
```

- `batch_begin` records the import_id and the sidecar file path (`.tessera/imports/<id>.cells.jsonl`)
- The actual cell data is written to the sidecar file (existing Tessera convention), NOT to the journal
- `batch_commit` includes the content_hash of the sidecar — verifiable on recovery
- `batch_begin` + sidecar write + sidecar fsync + `batch_commit` is the ordering
- If crashed mid-batch (no `batch_commit`): sidecar exists but is unverified. On restart, delete the incomplete sidecar and re-run the import via the Tessera schedule retry mechanism.
- If `batch_commit` exists: sidecar is authoritative; the batch was fully applied. No replay needed (Tessera's own audit log confirms).
- This ties into the existing `.tessera/imports/`, `.tessera/audit.jsonl`, and `.tessera/active-imports.json` infrastructure — no new persistence concepts.

**Phase 8.0 MVP scope:** Single-cell writes only (per-write journaling). Tessera batch protocol ships in Phase 8.1 alongside Tessera schedule integration. Phase 8.0 acceptance criteria do NOT include batch recovery.

### Decision 9: Tessera integration — subsumes existing daemon

The Phase 8 daemon runs Tessera schedules internally. No separate `mc tessera daemon` process needed.

**Transition path for existing users:**
1. `mc tessera daemon --stop` (stop existing tessera daemon)
2. `mc up` (start service daemon — schedules now run inside it)
3. Existing schedules work immediately (same `schedules.json` format, same schedule IDs)

**Guard:** `mc up` checks `.tessera/daemon.pid` — refuses to start if tessera daemon running, with message: "Stop tessera daemon first: `mc tessera daemon --stop`"

**Compatibility:** `mc tessera daemon` remains as standalone fallback for Phase 8. Phase 9 prints deprecation warning. Phase 10 removes it.

### Decision 10: Workspace and org discovery

Three modes:
- **Single workspace (default):** `mc up` — workspace.yaml in current directory
- **Org mode:** `mc up --org ./my-org` — reads org.yaml, discovers all workspaces
- **Multi-path:** `mc up --workspace ./ws-a --workspace ./ws-b`

In org mode, API requests must specify `workspace` field.

### Decision 11: Configuration — daemon.toml

```toml
[daemon]
port = 8787
host = "127.0.0.1"
api_key = ""                    # empty = auth disabled (localhost only)

[cache]
budget_mb = 512
preload_cubes = []

[tessera]
schedule_enabled = true

[mcp]
write_enabled = false           # opt-in for agent writes

[timeouts]
default_ms = 60000
sweep_ms = 120000
narrate_ms = 90000

[api]
max_request_body_mb = 10
# CORS: auto-computed from host binding
# localhost → ["http://localhost:*", "http://127.0.0.1:*"]
# non-localhost with api_key → [] (must be explicitly configured)
cors_origins = "auto"
```

### Decision 12: Error handling

| Scenario | Behavior |
|---|---|
| Cube fails to load | Stays cold. Error logged. Health reports "degraded." Requests → 503 with diagnostic. |
| Write fails after journal | Entry stays "pending." Replayed on restart. Client gets error; can retry. |
| Tessera recipe malformed | Error logged. Schedule failure_count++. After 2 consecutive failures, schedule paused. |
| Out of cache budget | Evict LRU cube before loading new one. |
| `mc reload --cube X` with in-flight queries | Drain: let queued requests in X's actor channel complete, then reload. New requests queue behind the reload. |
| `mc reload` during active Tessera import | Block reload until import completes. Log warning. |
| Ctrl+C (SIGINT) | Graceful shutdown identical to `mc down`. Set shutdown flag, drain in-flight, flush journal, exit 0. |
| Ctrl+C twice in rapid succession | First = graceful (30s timeout). Second within 5s = forced exit (no drain, exit 1). |
| stdin closed (SSH disconnect) | Daemon continues running (foreground process ignores SIGHUP unless `--detach`). Use `mc down` from another session to stop. |

### Decision 13: Logging and observability

**Log format:** Structured (JSON lines) when `--detach`; human-readable when foreground. Configurable in `daemon.toml`:
```toml
[logging]
format = "auto"   # auto | json | pretty
level = "info"    # debug | info | warn | error
```

`auto` = JSON when detached, pretty when foreground (TTY detection).

**What gets logged at each level:**
- `debug`: every request received, cache hits/misses, actor channel depth
- `info`: startup/shutdown, cube load/evict, Tessera import completion, reload
- `warn`: slow queries (>5s), cache budget pressure, Tessera schedule failure, truncated journal entry on recovery
- `error`: cube load failure, write journal I/O error, unrecoverable state

**Health endpoint (`GET /api/v1/health`) — auth-exempt, minimal payload:**
```json
{
  "status": "healthy",          // healthy | degraded | unhealthy
  "uptime_seconds": 3600
}
```

Minimal by design — unauthenticated callers learn only whether the service is up, not internal state. Cube counts, cache usage, journal state, and degraded cube names are sensitive operational data.

**Status endpoint (`GET /api/v1/status`) — requires auth, full diagnostics:**
```json
{
  "status": "healthy",
  "uptime_seconds": 3600,
  "cubes_registered": 5,
  "cubes_warm": 3,
  "cache_bytes_used": 134217728,
  "cache_budget_bytes": 536870912,
  "pending_journal_entries": 0,
  "degraded_cubes": []
}
```

### Decision 14: Warm restart hash-mismatch behavior

When `daemon.toml` specifies `preload_cubes` or `mc up --preload` is used:
- Daemon reads `.mosaic/cache-manifest.json` for stored content hashes
- If content hash matches current file → pre-load (fast, uses cached compilation if available)
- **If content hash differs → skip pre-load, leave cube cold.** Log: `"Cube 'X' model changed since last run; skipping pre-load (will cold-load on first request)."`
- If manifest doesn't exist (first run) → skip pre-load for all cubes

No surprises. Pre-loading only accelerates cubes whose models haven't changed.

---

## Implementation phasing

### Phase 8.0 — MVP (ship this first)

1. `mc up` / `mc down` / `mc status`
2. HTTP API for `query`, `write`, `trace` (three most-used verbs)
3. Per-cube actor model (Decision 2)
4. Hot cube cache: load-on-first-request, LRU eviction (Decision 3)
5. Write-ahead journal (Decision 8)
6. Signal handling + graceful shutdown
7. `daemon.toml` configuration
8. Optional API key auth (Decision 7)
9. Single-workspace mode only

### Phase 8.1 — Full API + org mode

10. All remaining verb endpoints (whatif, sweep, diff, narrate, narrate_trends)
11. MCP server (Decision 6)
12. Org mode (Decision 10)
13. Tessera schedule integration (Decision 9)
14. `mc ps` and `mc reload`
15. Warm restart with content hashes (Decision 4)

### Phase 8.5 — Grout integration (separate ADR)

16. Hash-chained write journal
17. Signed exports via API
18. Canary checks on startup
19. Grant verification (org mode)

---

## Acceptance criteria (Phase 8.0 MVP)

**Functional:**
1. `mc up` starts daemon, binds HTTP on configured port, prints banner
2. `mc down` gracefully shuts down (flush writes, remove PID)
3. `mc status` reports health when running; "not running" when not
4. `POST /api/v1/query` returns correct cell values from a loaded cube
5. `POST /api/v1/write` persists to write journal then applies to cube
6. `POST /api/v1/trace` returns computation trace
7. Cubes load on first request (not at startup)
8. LRU eviction works when cache budget exceeded
9. `--api-key` enables auth; without it, daemon refuses non-localhost bind (including specific IPs and IPv6 non-loopback)
10. Crash + restart replays uncommitted single-cell journal entries correctly (Tessera batch protocol is Phase 8.1)
11. `GET /api/v1/health` responds without auth with minimal payload (status + uptime only); `GET /api/v1/status` requires auth for full diagnostics
12. Ctrl+C triggers graceful shutdown; double Ctrl+C forces exit
13. All existing tests pass unchanged
14. `cargo test --workspace` passes
15. `cargo clippy --all-targets --workspace -- -D warnings` passes
16. No changes to `mc-core`

**Performance baselines (measured, not just "it works"):**
17. Cold-load latency (first query to Acme cube): < 2 seconds
18. Warm-query latency (single-cell read on warm cube): < 10ms p99
19. Write throughput (sustained single-cell writes): > 500 writes/second
20. Cache budget enforcement: memory stays within configured budget under load

---

## Out of scope

- Multi-tenant (Phase 9)
- RBAC / user management (Phase 9)
- Horizontal scaling / replicas (Phase 9)
- Filesystem watcher / auto-reload (Phase 8.1)
- Full MCP server (Phase 8.1)
- Org mode (Phase 8.1)
- Grout integration (Phase 8.5)
- Web UI bundling (Phase 6B ships separately)
- Kernel changes for `&self` reads (Phase 9 exploration)

---

## Cross-links

- **ADR-0025 Decision 2, Shape 4:** Deployment shape this implements
- **ADR-0025 Decision 3:** Caching rules (coordinate+revision, budget-driven)
- **ADR-0026:** Org/workspace architecture daemon inherits
- **ADR-0027:** Cross-coord dep-graph fix (cache precision for daemon)
- **Research note:** `docs/research-notes/mosaic-service-daemon.md`
- **Grout research:** `docs/research-notes/grout-security-architecture-vision.md`
- **Tessera daemon:** `crates/mc-tessera/src/schedule/daemon.rs` (subsume)
- **Demo server:** `crates/mc-demo-server/` (pattern reference)
- **TM1 comparison Part 7:** TM1 daemon-mode optimizations inform priorities

---

## Notes

**Why per-cube actor, not mutex.** The actor model (tokio task + mpsc channel) is cleaner than `Arc<Mutex<Cube>>` for async code. No mutex poisoning, no async-mutex await-while-holding issues, natural backpressure via channel capacity, and the sequential-within-cube guarantee is automatic.

**Why API key, not "wait for Phase 9."** The research note encourages Tailscale/tunnel deployments for personal use. That means network-exposed daemon. Network-exposed without auth is irresponsible. A bearer token is 10 lines of middleware and eliminates the "exposed daemon" risk class.

**Why content hash, not mtime.** mtime is wrong surprisingly often (git checkout, file copy, NFS, clock skew). Content hash is authoritative. SHA-256 of a 500-line YAML file takes microseconds. No reason to use the less reliable option.

**Effort estimate.** Phase 8.0 MVP: 4-6 sessions. The axum server scaffolding, actor model, cache, journal, and signal handling are all well-understood patterns. The novelty is in the composition, not in any individual piece.
