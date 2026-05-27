# mc-daemon

Mosaic service daemon — persistent HTTP API with hot cube cache, per-cube actors, and crash recovery.

## Quick start

```bash
# Start the daemon
cargo run --release -p mc-daemon -- up --port 8787 --api-key test-key

# Query
curl -X POST http://127.0.0.1:8787/api/v1/query \
  -H "Authorization: Bearer test-key" \
  -H "Content-Type: application/json" \
  -d '{"cube":"nba-totals","where":{"Game":"LAL_at_BOS","Scenario":"Base","Version":"Working","Sportsbook":"Pinnacle","Time":"2026_04_15"},"show":["Predicted_Total"]}'

# Whatif (Phase 8.2)
curl -X POST http://127.0.0.1:8787/api/v1/whatif \
  -H "Authorization: Bearer test-key" \
  -H "Content-Type: application/json" \
  -d '{"schema_version":"1.0","cube":"nba-totals","overrides":[{"at":{"Measure":"avg_pace"},"value":102.4}],"where":{"Game":"LAL_at_BOS","Scenario":"Base","Version":"Working","Sportsbook":"Pinnacle","Time":"2026_04_15"},"show":["Predicted_Total","P_Over"]}'

# Sweep (Phase 8.2)
curl -X POST http://127.0.0.1:8787/api/v1/sweep \
  -H "Authorization: Bearer test-key" \
  -H "Content-Type: application/json" \
  -d '{"schema_version":"1.0","cube":"nba-totals","vary":{"kind":"override","at":{"Game":"LAL_at_BOS","Measure":"Market_Line"},"range":{"start":7.5,"stop":10.5,"step":0.5}},"where":{"Scenario":"Base","Version":"Working","Sportsbook":"Pinnacle","Time":"2026_04_15"},"overrides":[],"show":["P_Over"],"metric":null,"goal":"none"}'

# Reload (Phase 8.2)
curl -X POST http://127.0.0.1:8787/api/v1/reload \
  -H "Authorization: Bearer test-key" \
  -H "Content-Type: application/json" \
  -d '{"schema_version":"1.0","cubes":["nba-totals"]}'

# OpenAPI spec (Phase 8.2)
curl http://127.0.0.1:8787/api/v1/openapi.json -H "Authorization: Bearer test-key" | jq .info
```

## Operational notes

### /reload 408 caveat (Amendment 6)

A 408 response from `/reload` means the HTTP request timed out, **NOT** that the reload operation was cancelled. The daemon's actor continues the reload to completion (or failure) and the result is discarded from the HTTP response. Clients receiving 408 on `/reload` **MUST** verify cube state via `GET /api/v1/cubes` (check revision number) or `GET /api/v1/status` before retrying. Retrying immediately can produce 409 `ReloadInProgress` conflicts.

Proper cancellation through the actor channel is Phase 9+ work — kernel changes required.

### Known inconsistencies

Phase 8.0 endpoints (`/query`, `/write`, `/trace`) use a thin error envelope (`{"error":"..."}`). Phase 8.2 endpoints (`/whatif`, `/sweep`, `/reload`) use the rich error envelope per ADR-0032 Decision 8 (`{schema_version, error: {code, message, diagnostic, context}}`). Future 8.x cleanup will migrate 8.0 endpoints to the rich envelope.
