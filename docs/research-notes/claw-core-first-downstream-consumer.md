# claw-core: First Downstream Consumer of the Phase 8.0 Daemon

**Status:** Research note (cross-repo handshake; possible Phase 8.0.1 driver)
**Date:** 2026-05-27
**Author:** claw-core LLM session (Claude Opus 4.7)
**Source:** filed from `claw-core` per ADR-0001 (Accepted 2026-05-26) cross-repo
feature-request pattern; surfaces three Mosaic daemon endpoints that claw-core
needs and the Phase 8.0 MVP didn't ship.

---

## Context

`claw-core` (the Cloudflare-Worker NBA totals pricing engine) accepted
[ADR-0001 — Mosaic as Modeling Substrate](https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0001-mosaic-as-modeling-substrate.md)
on 2026-05-26. Mosaic is the model brain; the Worker is the runtime
shell. Cartridges per sport live in `mc-v2/examples/sports-betting/`
(NBA already there, MLB ships with claw-core's ADR-0002 / Phase 1.0).

Phase 0.4a of claw-core shipped 2026-05-26 evening:

- Mac Mini hosts `mc up` daemon (workspace `examples/sports-betting/`)
  with bearer-token auth
- Cloudflare Tunnel routes `mosaic-primary.edwinlovett.com` → the daemon
- claw-core's Worker has a `mosaic-runner-client.ts` that calls the
  daemon's REST API with primary→fallback failover
- A real V1.6 Lasso prediction (LAL @ BOS = 228.13) now flows
  Worker → Tunnel → daemon → cartridge end-to-end

This note exists because three daemon endpoints that ADR-0001 assumed
exist (per ADR-0029's planned surface) are NOT in the Phase 8.0 MVP.
Filing as a downstream-consumer signal so a Phase 8.0.1 / 8.0.2 can
decide whether to add them.

## The gap

Daemon endpoints currently registered (per
`crates/mc-daemon/src/server.rs`):

- `POST /api/v1/query`
- `POST /api/v1/write`
- `POST /api/v1/trace`
- `GET  /api/v1/health` (auth-exempt)
- `GET  /api/v1/status`
- `GET  /api/v1/cubes`

Endpoints ADR-0029 listed but the MVP didn't ship:

- `POST /api/v1/whatif`
- `POST /api/v1/sweep`
- `POST /api/v1/reload`
- `POST /api/v1/diff`
- `POST /api/v1/narrate`
- `POST /api/v1/narrate-trends`
- `POST /api/v1/snapshot`
- `POST /api/v1/rollback`
- `GET  /api/v1/cubes/:name`

## What claw-core actually needs (ranked by criticality)

### CRIT — `POST /api/v1/whatif`

**What it does (per CLI):** issue a query against a cube with
per-call coordinate overrides applied transiently (not persisted to
the cube state).

**Why claw-core needs it:** the production prediction flow is:

```
Worker pulls live odds + features for tonight's game
  → POST /api/v1/whatif with overrides for this game's features
  → daemon evaluates cartridge against the overridden coords
  → returns Predicted_Total, P_Over, Calibrated_P_Over
Worker applies Kelly + line-shop + writes prediction to D1
```

Without `/api/v1/whatif`, the Worker has two ugly alternatives:

1. `POST /api/v1/write` to set today's features into the cube, then
   `POST /api/v1/query`. This pollutes the cube's persistent state on
   every prediction request. Over a season we'd accumulate stale
   "tonight's features" entries.
2. Add tonight's game as a new `canonical_inputs` row, push a cartridge
   reload, then query. Requires either editing the YAML on every
   prediction (insane) or running the cube in some "live ingest" mode
   that doesn't exist.

`/api/v1/whatif` is the right primitive. It's literally what `mc model
whatif` does at the CLI; this is just exposing it over HTTP.

**Suggested request shape** (mirroring `mc model whatif --set
<coord>=<n> --show <measures>`):

```json
POST /api/v1/whatif
{
  "cube": "<name from workspace>",
  "set": ["Game=LAL_at_BOS,Measure=avg_pace=102.4",
          "Game=LAL_at_BOS,Measure=combined_off_rating=225.1"],
  "where": {"Game": "LAL_at_BOS", "Scenario": "Base", "Version": "Working",
            "Sportsbook": "Pinnacle", "Time": "2026_04_15"},
  "show": ["Predicted_Total", "P_Over", "Calibrated_P_Over"]
}
```

Response shape: same as `/api/v1/query` (`{schema_version, results[{coord, values}]}`).

**Effort estimate (rough, from outside):** the handler is a thin wrapper
around the existing `mc model whatif` evaluator. Most of the work is
plumbing the JSON shape through `axum` and validating overrides at
parse time. Probably 0.5–1 day given the codebase pattern.

### HIGH — `POST /api/v1/sweep`

**What it does (per CLI):** sweep a coefficient or input across a
range, return per-point metric values. The "slider" workflow.

**Why claw-core needs it:** this is the load-bearing motivator for
ADR-0001 per claw-core's project owner ("I want the option that would
allow us to do things like take our variables that help us to
determine a prediction and run those variables across a 'slider' to
find out which combination of variables produces the best outcomes").

The CLI works today:

```
$ time mc model sweep nba-totals.yaml \
    --model nba_v16_lasso --coefficient avg_pace \
    --range 2.5:3.5:0.1 --metric mean --goal maximize --format json
{ "schema_version": "1.0", "metric": "mean", "goal": "maximize",
  "baseline": 0, "sweep": [{"value": 2.5, "metric": 0, ...}, ...] }
real    0.022s
```

22ms for an 11-step sweep — that's plenty of headroom for the
project-owner's interactive use case. claw-core ADR-0001 AC-11 set a
5s ceiling on a 30-step sweep; that's 200× headroom.

But the Worker (or any LLM session driving the slider through the
Tunnel) can't call this today. The endpoint just doesn't exist.

**Suggested request shape** (mirroring CLI flags):

```json
POST /api/v1/sweep
{
  "cube": "<name>",
  "model": "nba_v16_lasso",
  "coefficient": "avg_pace",
  "range": "2.5:3.5:0.1",
  "metric": "mean",
  "goal": "maximize",
  "metric_where": "<optional filter>",
  "set": ["<optional fixed override>"]
}
```

Response: same shape as the CLI JSON output.

**Effort:** also a thin handler around the existing CLI evaluator.

### MEDIUM — `POST /api/v1/reload`

**What it does:** force the daemon to re-read its workspace's
cartridge YAMLs from disk and rebuild affected cubes.

**Why claw-core needs it:** ADR-0001 Decision 5 (floating-pin on
`mc-v2` main, validated by per-host daily-pull) requires the daemon
to pick up a new cartridge build without a process restart.

ADR-0001 Phase 0.4 handoff Step 4 specifies:

```bash
# After cargo install --force succeeds:
curl -X POST -H "Authorization: Bearer $TOKEN" \
  https://mosaic-primary.<domain>/api/v1/reload
```

Today the workaround is `launchctl unload + load` on the runner
plist, but that's heavier than needed (the binary is the same after a
`cargo install --force`; we just want the daemon to re-read YAMLs).

**Suggested request shape:**

```json
POST /api/v1/reload
{
  "cubes": ["nba-totals"]   // optional; omit to reload all
}
```

Response:

```json
{
  "reloaded": ["nba-totals"],
  "errors": []
}
```

**Effort:** the daemon already has cube-loading code; this is an
endpoint that calls the existing logic for the named cubes and
returns the results.

## Lower-priority / nice-to-have (don't block on these)

- `GET /api/v1/cubes/:name` — detail view of a single cube. claw-core
  could use this for ops dashboards but the bulk `GET /cubes` covers the
  basic "is the cartridge loaded" question.
- `POST /api/v1/diff` — comparing two cube states. Useful for
  retroactive backtesting later, not now.
- `POST /api/v1/narrate` — Mosaic Phase 7 narrative engine. Interesting
  for KB delivery long-term, not load-bearing for Phase 1.0 MLB.
- `POST /api/v1/snapshot` / `POST /api/v1/rollback` — write-history
  support. Useful for the slider-then-commit workflow but not blocking.

## What claw-core is doing in the meantime

- **`mosaic-runner-client.ts` has `whatifCartridge()` defined** (calls
  `/api/v1/whatif`) but flagged as unavailable until this endpoint
  ships. The function exists so callsites compile; runtime returns a
  `MosaicError` saying "endpoint not yet implemented in daemon."
- **`sweepCartridge()` not yet added** — waiting on the endpoint to
  exist before claw-core takes the dependency. Will be added in the
  PR that consumes the new endpoint.
- **Daily-pull reload step uses `launchctl kickstart`** instead of
  `/api/v1/reload`. Works but is heavier than needed.
- **ADR-0001 AC-11 (sweep <5s)** is marked deferred-on-Mosaic-feature
  in claw-core's tracking. CLI proof exists; HTTP path doesn't.

## Suggested Mosaic-side sequencing

If Mosaic Phase 8.0.1 is the right home for these endpoints, the order
that maximizes claw-core unblock is:

1. **whatif** (critical for prediction path)
2. **reload** (critical for floating-pin safety net)
3. **sweep** (load-bearing for the slider workflow + AC-11)

The handlers are all thin wrappers around existing CLI evaluators. The
shape considerations (auth, schema_version envelope, error response
format) are already established by `query`/`write`/`trace`. This feels
like a 1–2 day Mosaic phase.

## Cross-repo coordination

Per claw-core's ADR-0001 Decision 5 (floating-pin policy), claw-core
will not pin to a specific mc-v2 tag. The daily-pull job rebuilds
against `main` each night and validates cartridges before swapping
the binary. When these endpoints ship in mc-v2, claw-core picks them
up on the next daily-pull tick, and the corresponding `whatif` /
`sweep` callsites in `mosaic-runner-client.ts` switch from "throws
MosaicError" to "returns the JSON response."

If the Mosaic-side decision is "won't add these endpoints" or "different
shape than suggested," claw-core needs to know early so the workaround
(per-request `POST /api/v1/write` followed by `query`, accepting the
state-pollution cost) gets escalated to ADR-amendment status.

## Cross-links

- ADR-0001 (claw-core): https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0001-mosaic-as-modeling-substrate.md
- ADR-0002 (claw-core, MLB cartridge): https://github.com/edwinlov3tt/claw-core/blob/main/docs/decisions/ADR-0002-mlb-totals-cartridge.md
- Phase 0.4 handoff (claw-core): https://github.com/edwinlov3tt/claw-core/blob/main/docs/handoffs/phase-0.4-mosaic-substrate-handoff.md
- ADR-0029 (mc-v2, Phase 8 daemon — the planned-surface doc):
  [`../decisions/0029-phase-8-service-daemon.md`](../decisions/0029-phase-8-service-daemon.md)
- Phase 8.0 completion: `2800d12 feat(mc-daemon): Phase 8.0 — Mosaic service daemon MVP`
- Workspace manifest that claw-core's daemon reads:
  `examples/sports-betting/workspace.yaml` (committed 2026-05-27)

## Notes

- claw-core's slash-command process now has its own ADR/handoff/research-note
  shape (mirrored from `mc-v2`'s spec-dev workflow). This note is the first
  cross-repo handshake — written by a claw-core LLM session, filed in mc-v2.
  Mosaic-side decision-makers (project owner + any Mosaic LLM session) should
  treat it as a feature-request signal, not a binding spec.
- If a Mosaic LLM session writes a counter-research-note (e.g. "we should ship
  these as JSON-RPC over `mc mcp` instead of REST"), claw-core will adapt;
  ADR-0001 Decision 6 was explicitly chosen because the REST API exists, but
  the architectural decision is "talk to the daemon," not "talk to it over
  REST specifically."
