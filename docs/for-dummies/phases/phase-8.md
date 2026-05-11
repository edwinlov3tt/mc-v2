# How the Daemon Works (for-dummies)

> You just shipped Phase 8.0. Here's what it does and how to use it.

---

## The 30-second version

Before Phase 8, every `mc model query` command had to:
1. Read the model YAML from disk
2. Parse + validate + compile it
3. Load all the input data
4. Evaluate the formula you asked about
5. Print the result
6. Exit (throw everything away)

Next query? Do it all again from scratch. Every. Single. Time.

**The daemon keeps everything in memory.** You start it once with `mc up`, it loads your cubes on first use, and then queries are instant because the cube is already hot in RAM. It's the difference between "open Excel, wait for it to load, look at cell" every time vs. "Excel is already open, just click the cell."

---

## How to use it right now

```bash
# Start the daemon (foreground — you'll see logs)
mc up

# You'll see:
#   Mosaic daemon running
#   Port: http://localhost:8787
#   Workspace: ./
#   ...

# In another terminal, query a cube:
curl -X POST http://localhost:8787/api/v1/query \
  -H "Content-Type: application/json" \
  -d '{
    "cube": "marketing-finance",
    "where": { "Time": "Q1_2025", "Market": "Houston" },
    "show": ["Spend", "Revenue", "ROAS"]
  }'

# First query: takes a second (cold-loading the cube)
# Second query: instant (cube is warm in memory)

# Write a value:
curl -X POST http://localhost:8787/api/v1/write \
  -H "Content-Type: application/json" \
  -d '{
    "cube": "marketing-finance",
    "coord": ["Baseline", "Working", "Q1_2025", "Paid_Search", "Houston", "Spend"],
    "value": 16000.0
  }'

# Trace a computation:
curl -X POST http://localhost:8787/api/v1/trace \
  -H "Content-Type: application/json" \
  -d '{
    "cube": "marketing-finance",
    "coord": ["Baseline", "Working", "Q1_2025", "Paid_Search", "Houston", "Revenue"]
  }'

# Check what's loaded:
curl http://localhost:8787/api/v1/cubes

# Check health:
curl http://localhost:8787/api/v1/health

# Stop the daemon:
mc down
```

---

## The three commands

| Command | What it does |
|---|---|
| `mc up` | Start the daemon. Finds `workspace.yaml` in the current directory, registers cubes, starts the HTTP server. |
| `mc down` | Stop the daemon gracefully. Flushes any pending writes, removes the PID file, exits cleanly. |
| `mc status` | Check if the daemon is running. Shows health info if it is, "not running" if it isn't. |

---

## What happens when you query

```
You:  POST /api/v1/query  { cube: "marketing-finance", ... }
         │
         ▼
    ┌─────────────┐
    │ HTTP Server  │  (Axum — handles routing, auth, CORS)
    └──────┬──────┘
           │  sends request via channel
           ▼
    ┌─────────────────┐
    │ Cube Actor      │  (one per cube — owns the cube exclusively)
    │                 │
    │  Is cube warm?  │
    │  ├─ No  → cold-load from YAML (parse → validate → compile → apply inputs)
    │  └─ Yes → use the in-memory cube
    │                 │
    │  Execute query   │  (runs in spawn_blocking — doesn't block other cubes)
    │  Return result   │
    └─────────────────┘
           │
           ▼
    You get JSON response in ~1ms (warm) or ~1-2s (cold, first time)
```

---

## What happens when you write

```
You:  POST /api/v1/write  { cube: "marketing-finance", coord: [...], value: 16000 }
         │
         ▼
    ┌─────────────┐
    │ HTTP Server  │
    └──────┬──────┘
           │
           ▼
    ┌─────────────────────────────────────────────┐
    │ Cube Actor                                   │
    │                                              │
    │  1. Write "pending" to write-journal.jsonl   │  ← crash protection
    │  2. Apply write to cube in memory            │  ← cube updates
    │  3. Append to .tessera/writes.jsonl          │  ← durable persistence
    │  4. Write "committed" to write-journal.jsonl │  ← confirms completion
    │  5. Reply "ok" to you                        │  ← you get the ack
    │                                              │
    └─────────────────────────────────────────────┘
```

If the daemon crashes between step 1 and step 4, when it restarts it replays the "pending" entry that was never "committed." Your write is never lost.

---

## The actor model (why it matters)

Each cube gets its own **actor** — a dedicated background task with a message queue. Think of it like each cube has its own personal assistant:

```
                    ┌──────────────────┐
 query request ───► │ marketing-finance │ ◄─── write request
 trace request ───► │     Actor        │
                    └──────────────────┘
                    
                    ┌──────────────────┐
 query request ───► │  brand-awareness  │
                    │     Actor        │
                    └──────────────────┘
```

**Within a cube:** requests are handled one at a time (because `Cube::read()` needs exclusive access). A query waits for a write to finish, etc.

**Across cubes:** fully parallel. Querying `marketing-finance` doesn't block `brand-awareness`. They're different actors on different threads.

---

## The cache (hot vs cold)

```
mc up starts:
  marketing-finance  → registered (cold — path known, not loaded)
  brand-awareness    → registered (cold)

First query to marketing-finance:
  marketing-finance  → LOADING... (parse YAML, validate, compile, apply inputs)
  marketing-finance  → warm ✓ (in memory, fast)

100 more queries to marketing-finance:
  → instant (already warm)

Cache budget exceeded (512MB default):
  Least-recently-used cube gets evicted → cold again
  Next query to that cube → cold-load again
```

The cache is **lazy** — it only loads cubes you actually use. If your workspace has 20 cubes but you only query 3, only 3 get loaded.

---

## Auth (keeping it safe)

```bash
# Localhost only — no auth needed (default)
mc up
# Only accessible from this machine

# Network-exposed — API key REQUIRED
mc up --host 0.0.0.0 --api-key my-secret-key
# Now accessible from other machines, but every request needs:
#   Authorization: Bearer my-secret-key

# Without --api-key, this REFUSES to start:
mc up --host 0.0.0.0
# Error: "Refusing to bind to non-localhost without --api-key"
```

The health endpoint (`/api/v1/health`) always works without auth — it only returns `{"status":"healthy","uptime_seconds":N}`, nothing sensitive.

Full diagnostics (`/api/v1/status`) require auth — it shows cube names, cache usage, journal state.

---

## Configuration (daemon.toml)

Put a `daemon.toml` in your workspace directory (next to `workspace.yaml`):

```toml
[daemon]
port = 8787                 # HTTP port
host = "127.0.0.1"         # localhost only (safe default)
api_key = ""                # empty = no auth needed (localhost only)

[cache]
budget_mb = 512             # max memory for cached cubes

[timeouts]
default_ms = 60000          # 60 second default request timeout
sweep_ms = 120000           # sweep gets 2 minutes (it's slow)

[logging]
format = "auto"             # "auto" = pretty in terminal, JSON when detached
level = "info"              # debug | info | warn | error
```

CLI flags override daemon.toml. So `mc up --port 9000` beats whatever's in the config file.

---

## Crash recovery

The daemon uses a **write-ahead journal** (`.mosaic/write-journal.jsonl`). It works like this:

```
Normal operation:
  write "pending"  →  apply to cube  →  persist to writes.jsonl  →  write "committed"

Crash between "pending" and "committed":
  Journal has a "pending" entry with no matching "committed"
  
On restart:
  Daemon reads journal → finds uncommitted entry → replays it → done
  Your write is safe. The client didn't get an "ok" so it might retry,
  but the journal has it covered either way.
```

The journal rotates at 10MB and gets cleaned up on graceful shutdown.

---

## Graceful vs forced shutdown

```
Ctrl+C (first time):
  "Shutting down gracefully..."
  → Stops accepting new requests
  → Waits up to 30 seconds for in-flight requests to finish
  → Flushes journal
  → Removes PID file
  → Exit 0

Ctrl+C (second time within 5 seconds):
  "Forced shutdown!"
  → Immediate exit
  → Exit 1
  → In-flight requests are dropped (but journal has their writes)
```

---

## Files the daemon creates

```
.mosaic/
├── daemon.pid                  # PID file (prevents running two daemons)
├── daemon.log                  # Log output when running with --detach
├── write-journal.jsonl         # Crash recovery journal (pending/committed entries)
└── (existing files unchanged)
    ├── analysis-ledger.jsonl
    ├── benchmark-library.json
    └── context-events.yaml
```

---

## The personal deployment story

With the daemon + Tailscale (or Cloudflare Tunnel), you get "anywhere access":

```bash
# On your home machine:
mc up --host 0.0.0.0 --api-key my-secret --detach
# Daemon running in background on port 8787

# From your laptop, phone, anywhere on your Tailscale network:
curl -H "Authorization: Bearer my-secret" \
  http://home-machine:8787/api/v1/query \
  -d '{"cube":"nba-totals","where":{"Time":"2025-03"},"show":["Predicted_Total"]}'
```

No cloud infrastructure. No monthly bill. Just a daemon on a machine you control.

---

## What's NOT in Phase 8.0 (coming in 8.1)

| Feature | Phase |
|---|---|
| All verb endpoints (whatif, sweep, diff, narrate) | 8.1 |
| MCP server (AI agents connect directly) | 8.1 |
| Org mode (multiple workspaces) | 8.1 |
| Tessera scheduled imports inside the daemon | 8.1 |
| `mc ps` (list loaded cubes) | 8.1 |
| `mc reload` (reload cube from disk) | 8.1 |
| Warm restart (remember which cubes were hot) | 8.1 |
| Grout integration (signed exports, hash chains) | 8.5 |

---

## Quick reference

```bash
mc up                              # start (foreground)
mc up --detach                     # start (background)
mc up --port 9000                  # custom port
mc up --api-key secret123          # enable auth
mc up --host 0.0.0.0 --api-key k  # network-exposed + auth

mc down                            # graceful stop
mc status                          # is it running?

# API endpoints:
POST /api/v1/query                 # read cells
POST /api/v1/write                 # write a cell
POST /api/v1/trace                 # trace computation
GET  /api/v1/cubes                 # list cubes
GET  /api/v1/health                # health check (no auth)
GET  /api/v1/status                # full diagnostics (auth required)
```
