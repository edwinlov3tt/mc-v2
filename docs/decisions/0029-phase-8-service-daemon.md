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

### Decision 3: Hot cube cache — load-on-first-request, LRU eviction

Cubes are NOT loaded at startup. They're registered (paths known) but stay cold. First API request targeting a cube triggers cold-load → cache it. Subsequent requests hit warm cache.

**Eviction:** LRU (least-recently-accessed). Configurable budget (default 512MB). When budget exceeded, evict least-recent cube. Never evict during an active request.

**Cache invalidation:** Cube is invalidated (reloaded from disk) when:
- Tessera import runs (new data → revision bumps)
- `mc model write` via API (direct cell write → revision bumps)
- Explicit `mc reload [--cube <name>]`

**No filesystem watcher in Phase 8.** Manual reload only. Auto-reload is Phase 8.1 enhancement.

### Decision 4: Warm restart — content hashes, opt-in pre-loading

On graceful shutdown, write `.mosaic/cache-manifest.json` recording cube names + **content hash (SHA-256)** of each cube's model YAML (not mtime — mtime is unreliable).

On restart:
- Daemon reads manifest but does NOT pre-load cubes by default
- Cubes stay cold until first request (same lazy behavior as fresh start)
- The manifest is informational only

**Opt-in pre-loading:** `mc up --preload marketing-finance` or `daemon.toml`: `preload_cubes = ["marketing-finance"]`. On pre-load, verify content hash — if changed, cold-load fresh instead.

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
- **Without `--api-key`, daemon refuses to bind to non-localhost.** If `--host 0.0.0.0` without `--api-key` → error and exit.
- With `--api-key`, all requests require `Authorization: Bearer <key>`. Missing → 401.
- Health endpoint (`/api/v1/health`) exempt from auth
- This is a shared secret, not real auth — Phase 9 adds users/sessions/RBAC

### Decision 8: Write-ahead journal — crash recovery

Every write is journaled BEFORE cube mutation. Per-cube sequential access (Decision 2) makes ordering natural — queries never see partially-applied writes.

**Journal format:** `.mosaic/write-journal.jsonl`
```json
{"ts":"...","cube":"marketing-finance","coord":[...],"value":15000.0,"status":"pending"}
{"ts":"...","cube":"marketing-finance","coord":[...],"value":15000.0,"status":"committed"}
```

**On crash restart:** Replay entries with "pending" but no "committed." Ignore truncated last line (write never acknowledged to client → client retries).

**Rotation:** At 10MB, rotate to `write-journal-{timestamp}.jsonl`. Old segments deleted on next graceful shutdown.

### Decision 9: Tessera integration — subsumes existing daemon

The Phase 8 daemon runs Tessera schedules internally. No separate `mc tessera daemon` process needed.

**Transition:**
- `mc up` checks `.tessera/daemon.pid` — refuses to start if tessera daemon running
- Same `schedules.json` format, same schedule IDs, same behavior
- `mc tessera daemon` remains as standalone fallback (prints deprecation warning in Phase 9)

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
cors_origins = ["http://localhost:*"]
```

### Decision 12: Error handling

| Scenario | Behavior |
|---|---|
| Cube fails to load | Stays cold. Error logged. Health reports "degraded." Requests → 503 with diagnostic. |
| Write fails after journal | Entry stays "pending." Replayed on restart. Client gets error; can retry. |
| Tessera recipe malformed | Error logged. Schedule failure_count++. After 2 consecutive failures, schedule paused. |
| Out of cache budget | Evict LRU cube before loading new one. |

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

1. `mc up` starts daemon, binds HTTP on configured port, prints banner
2. `mc down` gracefully shuts down (flush writes, remove PID)
3. `mc status` reports health when running; "not running" when not
4. `POST /api/v1/query` returns correct cell values from a loaded cube
5. `POST /api/v1/write` persists to write journal then applies to cube
6. `POST /api/v1/trace` returns computation trace
7. Cubes load on first request (not at startup)
8. LRU eviction works when cache budget exceeded
9. `--api-key` enables auth; without it, daemon refuses non-localhost bind
10. Crash + restart replays uncommitted journal entries correctly
11. `GET /api/v1/health` always responds (even without auth)
12. All existing tests pass unchanged
13. `cargo test --workspace` passes
14. `cargo clippy --all-targets --workspace -- -D warnings` passes
15. No changes to `mc-core`

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
