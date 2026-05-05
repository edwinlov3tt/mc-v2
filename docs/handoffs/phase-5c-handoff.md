# Phase 5C Handoff — Tessera Driver Expansion + Cron Scheduling

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 5C.
> **You inherit a green Phase 5B** (all existing tests pass; Tessera
> core engine operational with 6 drivers + recipe authoring skills).
> **Branch:** `phase-5c/driver-expansion` (create from `main`).
>
> **Read these before touching code:**
> - [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md) Decision 9 (5C scope) + Decision 12 (out-of-scope items 5C picks up)
> - [ADR-0010 Amendment 1](../decisions/0010-amendment-1-stream-c-pin-corrections.md) (DuckDB pin corrections — context for dep management)
> - [`CLAUDE.md`](../../CLAUDE.md) (naming convention, project rules)
>
> **Hard rule:** Phase 5C touches ONLY `mc-drivers`, `mc-recipe`,
> `mc-tessera`, and `mc-cli`. The locked crates (`mc-core`,
> `mc-model`, `mc-fixtures`) remain untouched. The Phase 4 plugin
> content (`mosaic-plugin/`) is locked unless adding new import-related
> skill examples.

---

## The one paragraph you must internalize before writing code

**Phase 5C is demand-driven expansion, not architecture.** The architecture
shipped in 5A (SourceDriver trait, recipe schema, Tessera orchestrator,
WriteBatch kernel). Phase 5C extends that architecture in six dimensions:
new drivers (MySQL, D1, Snowflake, BigQuery, SQL Server), cron scheduling
(the critical feature for sports betting — odds data ingested on cadence),
incremental loads (watermark/cursor state), new write dispositions (append +
merge), element auto-creation (`on_missing_element: create`), and ADR-0014
time_format enforcement. Each feature is independently shippable. One driver
failing CI does not block another. The cron daemon is the most architecturally
novel piece — it is a sync single-process loop, NOT an async runtime.

---

## Scope — the 7 deliverables

### 1. New source drivers

| Driver | Crate dep | Notes |
|---|---|---|
| **MySQL** | `mysql` (pure Rust, sync, 1.78-clean) | Feature flag: `mysql` |
| **Cloudflare D1** | `ureq` (already in tree) | REST API only; custom driver; PK-cursor pagination mandatory |
| **Snowflake** | `odbc-api` | ODBC wrapper; system-installed ODBC driver required |
| **SQL Server** | `odbc-api` (shared with Snowflake) | Same ODBC pattern; system driver required |
| **BigQuery** | `ureq` (already in tree) | REST API; service-account JSON auth; custom driver |

Each driver:
- Implements the existing `SourceDriver` trait (frozen by ADR-0010 Appendix C).
- Lives in its own module under `crates/mc-drivers/src/`.
- Is gated behind a Cargo feature flag (default = off; CI enables all features).
- Ships with a dedicated test fixture (committed dataset or mock server).
- Gets a new `DriverKind` variant in `mc-recipe/src/schema.rs`.

### 2. Cron scheduling (the critical feature)

**Why this matters:** sports betting data (odds, lines, results) changes on
cadence. Pre-game lines update daily. In-play odds update every 30 seconds.
Post-game results arrive once. A scheduler that fires `mc tessera apply` at
the right moments is the difference between "Mosaic is a batch tool" and
"Mosaic is a real-time analytics substrate."

**CLI verbs:**

```
mc tessera schedule <recipe_path> --cron "<expr>"   # register
mc tessera schedule list                             # show all
mc tessera schedule remove <schedule_id>            # unregister
mc tessera daemon                                    # run the scheduler loop
mc tessera daemon --once                            # fire all due schedules NOW, then exit
```

**Cron expression format:**

Standard 5-field: `minute hour day-of-month month day-of-week`

Built-in presets:
- `@hourly` = `0 * * * *`
- `@daily` = `0 0 * * *`
- `@weekly` = `0 0 * * 0`
- `@every 5m` = every 5 minutes (non-standard but common)
- `@every 30s` = every 30 seconds (non-standard; sub-minute precision)
- `@every 1h` = every hour on the hour

**Schedule registry:**

```
<model_dir>/.tessera/schedules.json
```

Shape:
```json
{
  "version": 1,
  "schedules": [
    {
      "id": "sched_pregame_odds_1717891234_8372",
      "recipe_path": "./recipes/pregame-odds.recipe.yaml",
      "cron": "@daily",
      "created_at": "2026-05-04T10:00:00Z",
      "status": "active",
      "last_run": "2026-05-04T00:00:00Z",
      "last_result": "success",
      "failure_count": 0,
      "on_failure": null
    }
  ]
}
```

**Daemon execution model:**

```rust
// Pseudocode — the daemon is a SYNC loop. No tokio. No async.
fn daemon_loop(model_dir: &Path) -> Result<(), TesseraError> {
    let mut registry = load_schedules(model_dir)?;
    loop {
        let now = current_time();
        for schedule in &mut registry.schedules {
            if schedule.status != "active" { continue; }
            if is_due(schedule, now) {
                match execute_recipe(&schedule.recipe_path) {
                    Ok(report) => {
                        schedule.last_run = now;
                        schedule.last_result = "success";
                        schedule.failure_count = 0;
                    }
                    Err(e) => {
                        schedule.failure_count += 1;
                        if schedule.failure_count == 1 {
                            // Retry once after 60s backoff
                            schedule.next_retry = now + 60s;
                        } else {
                            schedule.status = "failed";
                            schedule.last_result = format!("failed: {e}");
                            // Fire on_failure hook if configured
                        }
                        log_to_audit(schedule, e);
                    }
                }
                persist_schedules(&registry)?;
            }
        }
        // Sleep until the next scheduled event (min 1s, max 60s).
        let next_wake = compute_next_wake(&registry);
        std::thread::sleep(next_wake);
    }
}
```

**Hard constraints on the daemon:**
- Single process, single thread (the `std::thread::sleep` loop above).
- No `tokio`, no `async`, no thread pool.
- No distributed scheduling. Single machine only.
- Daemon writes a PID file at `<model_dir>/.tessera/daemon.pid` — prevents double-start.
- `SIGTERM` / `SIGINT` causes graceful shutdown (finish current recipe if running, then exit).
- The daemon does NOT hold the cube in memory between runs. Each recipe execution is a fresh `Tessera::prepare` + `Tessera::apply`. This keeps memory bounded.

**Error handling:**
- On recipe failure: log to audit, retry ONCE after 60-second backoff.
- On second failure: mark schedule `status: "failed"`, stop retrying.
- `--on-failure <command>` hook: shell-exec the command with env vars `TESSERA_SCHEDULE_ID`, `TESSERA_RECIPE`, `TESSERA_ERROR`. Optional.
- Failed schedules stay in the registry. `mc tessera schedule remove` or manual edit to clear.

**Sports betting examples:**

```bash
# Pre-game odds: ingest next-day lines every morning at 6 AM
mc tessera schedule ./recipes/pregame-odds.recipe.yaml --cron "0 6 * * *"

# In-play odds: ingest live lines every 30 seconds during game windows
mc tessera schedule ./recipes/inplay-odds.recipe.yaml --cron "@every 30s"

# Post-game results: ingest final scores daily at midnight
mc tessera schedule ./recipes/postgame-results.recipe.yaml --cron "0 0 * * *"

# Market-close data: ingest after markets close (4:30 PM ET weekdays)
mc tessera schedule ./recipes/market-close.recipe.yaml --cron "30 16 * * 1-5"

# Start the daemon (runs in foreground; use systemd/launchd for background)
mc tessera daemon
```

Example recipe for in-play odds (`inplay-odds.recipe.yaml`):
```yaml
version: 1
name: inplay_odds_live
description: "Live in-play odds from the sportsbook API. Runs every 30s during game windows."
model: ../models/sportsbook.yaml

source:
  driver: http_json
  url: "https://api.sportsbook.example/v2/odds/live"
  json_path: "$.events[*]"

columns:
  - source: event_id
    dimension: Event
  - source: market_type
    dimension: Market
  - source: selection
    dimension: Selection
  - source: odds_decimal
    measure: Odds
    type: f64
  - source: implied_prob
    measure: ImpliedProbability
    type: f64
  - source: volume
    measure: Volume
    type: f64

defaults:
  scenario: Live
  version: Working
  time: Current

write_disposition: merge
incremental: true
incremental_config:
  strategy: watermark
  column: last_updated
  format: iso8601

batch:
  size: 10000

on_error: skip_row
on_missing_element: create

credentials:
  api_key: "${env.SPORTSBOOK_API_KEY}"
```

### 3. Incremental loads

**Recipe schema extension:**

```yaml
incremental: true
incremental_config:
  strategy: watermark | cursor
  column: "<column_name>"        # the high-water-mark or cursor column
  format: iso8601 | unix_epoch | integer  # how to parse the column
  initial_value: null            # optional starting point (null = full load first time)
```

**Watermark strategy:** track the MAX value of `column` across all rows
returned. On the next run, inject `WHERE <column> > <last_max>` into the
query (or pass it as a query parameter for HTTP drivers). The driver is
responsible for honoring the watermark in its query construction.

**Cursor strategy:** track the last-seen primary key value. On next run,
resume with `WHERE <pk_column> > <last_cursor>`. Requires a stable sort
order in the source.

**State persistence:**

```
<model_dir>/.tessera/incremental/<recipe_name>.state.json
```

Shape:
```json
{
  "recipe_name": "inplay_odds_live",
  "strategy": "watermark",
  "column": "last_updated",
  "last_value": "2026-05-04T15:32:00Z",
  "last_run": "2026-05-04T15:32:05Z",
  "rows_since_full_load": 48520
}
```

**CLI verb for state reset:**
```bash
mc tessera reset-state <recipe_name>   # clears incremental state; next run = full load
```

**Implementation notes:**
- The `SourceDriver` trait does NOT change. Incremental state injection happens at the recipe/orchestrator layer: the orchestrator modifies the `source.query` before constructing the driver (appending a WHERE clause or replacing a `{{watermark}}` placeholder).
- For HTTP/JSON drivers, the watermark is passed as a query parameter (the recipe specifies `incremental_config.param_name` for HTTP sources).
- State is written AFTER successful commit only. A failed import does not advance the watermark.

### 4. Write dispositions: `append` and `merge`

Extend the `WriteDisposition` enum in `mc-recipe/src/schema.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WriteDisposition {
    #[default]
    Replace,
    /// Add new cells without overwriting existing coordinates.
    /// If a coordinate already exists in the cube, the existing value
    /// is preserved (incoming row is silently dropped for that coord).
    Append,
    /// Upsert by coordinate. New coordinates are inserted; existing
    /// coordinates are overwritten with the incoming value. Coordinates
    /// not in the incoming data are untouched.
    Merge,
}
```

**Behavior matrix:**

| Disposition | Coord exists? | Coord in incoming? | Result |
|---|---|---|---|
| `replace` | yes | yes | overwrite |
| `replace` | yes | no | untouched |
| `replace` | no | yes | insert |
| `append` | yes | yes | keep existing (skip incoming) |
| `append` | yes | no | untouched |
| `append` | no | yes | insert |
| `merge` | yes | yes | overwrite |
| `merge` | yes | no | untouched |
| `merge` | no | yes | insert |

Note: `replace` and `merge` have identical behavior in Phase 5C. The
semantic distinction matters for future full-slice-replace (clear + write)
which `replace` will evolve toward; `merge` will always remain coordinate-
level upsert.

**Implementation:** the orchestrator (`mc-tessera/src/runner.rs`) checks
the disposition before staging each cell:
- `replace` / `merge`: always stage (overwrite).
- `append`: check `cube.read(coord)` — if the coord already has a non-Null
  value, skip it.

### 5. `on_missing_element: create`

Extend the `OnMissingElement` enum in `mc-recipe/src/schema.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnMissingElement {
    #[default]
    Error,
    /// Auto-create the missing element as a leaf in the target dimension.
    /// Does not modify hierarchy structure (new elements have no parent).
    Create,
}
```

**Behavior:**
- When a row references a dimension element name not found in the model's
  dimension, and `on_missing_element: create` is set, auto-create the element
  at the leaf level of the dimension's default hierarchy.
- The element gets a fresh `ElementId` assigned by the cube.
- No hierarchy parent is assigned (the element is an orphan leaf).
- A lint warning fires if more than 100 elements are auto-created in a single
  import (suggests the model schema is wrong, not the data).
- The auto-created elements are NOT persisted back to the model YAML. They
  exist only in the in-memory cube and the `.tessera/` sidecar state. A
  future `mc model sync-from-imports` command could write them back.

### 6. ADR-0014 time_format enforcement (MC5030-MC5033)

**Recipe schema extension** (new fields on `ColumnMapping`):

```rust
pub struct ColumnMapping {
    // ... existing fields ...

    /// strptime-style format string for parsing date/time columns.
    /// Required for non-ISO date formats (MC5030 fires without it).
    /// Example: "M/d/yyyy h:mm a", "%Y-%m-%d", "yyyy-MM-dd'T'HH:mm:ss"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_format: Option<String>,

    /// IANA timezone identifier for the source timestamps.
    /// Required for timezone-less timestamps (MC5031 fires without it).
    /// Example: "America/New_York", "Europe/London", "UTC"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_timezone: Option<String>,

    /// How to bucket a parsed timestamp into the model's Time dimension.
    /// Maps a specific date/time to a Time element (e.g., "2026-Q3",
    /// "2026-05", "2026-W18").
    /// Values: "year", "quarter", "month", "week", "day"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_to_period: Option<String>,
}
```

**Diagnostic codes:**

| Code | Fires when | Severity |
|---|---|---|
| MC5030 | Non-ISO date column without explicit `time_format` | Error |
| MC5031 | Timezone-less timestamp without `time_timezone` | Error |
| MC5032 | Non-IANA timezone identifier in `time_timezone` | Error |
| MC5033 | Parsed date doesn't map to any declared Time element | Warning (or Error if `on_missing_element: error`) |

**ISO 8601 auto-detection:** if the source column's values match ISO 8601
patterns (`YYYY-MM-DD`, `YYYY-MM-DDTHH:MM:SSZ`, etc.), no `time_format`
is required. The orchestrator auto-detects ISO and parses without explicit
format. MC5030 only fires for ambiguous date formats (e.g., `05/04/2026` —
is that May 4 or April 5?).

**IANA timezone validation:** `time_timezone` values are validated against
a static list of IANA timezone identifiers (embedded at compile time from
the IANA tz database). Non-IANA strings (e.g., "EST", "PST", "Eastern")
fire MC5032. The fix message suggests the correct IANA identifier.

**`map_to_period` behavior:** after parsing the date to a civil date, the
orchestrator maps it to the Time dimension element matching the specified
granularity. Example: `2026-05-04` with `map_to_period: month` maps to
the Time element `"2026-05"` (or whatever naming convention the model
uses — the orchestrator tries common patterns: `YYYY-MM`, `YYYY-Qn`,
`YYYY-Www`). If no match, MC5033 fires.

### 7. DuckDB federation documentation

No code changes required. Document that existing `duckdb` driver supports:
- `INSTALL postgres; LOAD postgres; ATTACH 'postgres:dsn=...'` for Postgres federation
- `INSTALL mysql; LOAD mysql; ATTACH 'mysql:host=...'` for MySQL federation

Add recipe examples in `examples/recipes/` showing federation queries.
Add a test fixture proving the federation path compiles and exercises the
`duckdb_driver` with an ATTACH statement.

---

## Per-driver implementation guidance

### MySQL (`mysql` crate)

- **Dep:** `mysql = "25"` (pure Rust, sync API, no system deps)
- **Feature flag:** `feature = "mysql"` in `mc-drivers/Cargo.toml`
- **Verification:** run `cargo +1.78 build --features mysql` in a scratch
  project BEFORE committing the dep pin. Follow the verification protocol
  from ADR-0010 Amendment 1.
- **Constructor:** `pub fn mysql_driver(dsn: &str, query: &str) -> Result<impl SourceDriver, DriverError>`
- **Test fixture:** embedded test using a small in-memory dataset (no external MySQL server required for CI). Use `mysql`'s `Opts::from_url` parsing test at minimum. Integration tests against a real MySQL are gated behind `#[cfg(feature = "mysql-integration")]`.
- **Schema inference:** `DESCRIBE` or `INFORMATION_SCHEMA.COLUMNS` query.

### Cloudflare D1 (REST API)

- **Dep:** `ureq` (already in tree) — no new dep.
- **Feature flag:** `feature = "d1"` in `mc-drivers/Cargo.toml`
- **Constructor:** `pub fn d1_driver(account_id: &str, database_id: &str, api_token: &str, query: &str) -> Result<impl SourceDriver, DriverError>`
- **Critical constraints (from research report):**
  - **4 requests/second rate limit.** The driver MUST implement a token-bucket rate limiter (or simpler: `std::thread::sleep(Duration::from_millis(250))` between requests). Exceeding the rate limit returns HTTP 429.
  - **PK-cursor pagination MANDATORY.** D1 does NOT support `OFFSET` without billing explosion (every OFFSET re-scans from row 0). Pagination must use `WHERE <pk> > <last_seen_pk> ORDER BY <pk> LIMIT <batch_size>`.
  - **100 bound parameters maximum** per query. If the query has more than 100 params, split into multiple requests.
  - **30-second query timeout.** Queries exceeding 30s are killed by D1. Large tables must be paginated.
  - **No transactions.** D1 is eventually consistent. The driver fetches best-effort snapshots.
- **API endpoint:** `https://api.cloudflare.com/client/v4/accounts/{account_id}/d1/database/{database_id}/query`
- **Auth:** Bearer token in `Authorization` header.
- **Response shape:** `{ "result": [{ "results": [...], "meta": {...} }], "success": true }`
- **Test fixture:** mock HTTP responses via a local HTTP server in tests (use `std::net::TcpListener` + manual HTTP response writing, no external mock framework needed).

### Snowflake (ODBC)

- **Dep:** `odbc-api = "8"` (or latest 1.78-compatible version)
- **Feature flag:** `feature = "snowflake"` in `mc-drivers/Cargo.toml`
- **System dependency:** requires `unixODBC` (macOS: `brew install unixodbc`; Linux: `apt install unixodbc-dev`) + Snowflake ODBC driver installed and registered in `odbcinst.ini`.
- **Constructor:** `pub fn snowflake_driver(connection_string: &str, query: &str) -> Result<impl SourceDriver, DriverError>`
- **Connection string format:** `Driver={Snowflake};Server=<account>.snowflakecomputing.com;Database=<db>;Schema=<schema>;Uid=<user>;Pwd=<pass>;`
- **Test fixture:** ODBC tests are gated behind `#[cfg(feature = "snowflake-integration")]`. Unit tests mock the ODBC calls at the `odbc-api` level where possible.
- **NOTE:** this driver breaks the single-binary distribution story. Document prominently. Users who need Snowflake must install the system ODBC driver separately.

### SQL Server (ODBC)

- **Dep:** same `odbc-api` as Snowflake (shared).
- **Feature flag:** `feature = "sqlserver"` in `mc-drivers/Cargo.toml`
- **System dependency:** requires `unixODBC` + Microsoft ODBC Driver 18 for SQL Server.
- **Constructor:** `pub fn sqlserver_driver(connection_string: &str, query: &str) -> Result<impl SourceDriver, DriverError>`
- **Connection string format:** `Driver={ODBC Driver 18 for SQL Server};Server=<host>,<port>;Database=<db>;Uid=<user>;Pwd=<pass>;Encrypt=yes;TrustServerCertificate=no;`
- **Same test/distribution constraints as Snowflake.**

### BigQuery (REST API)

- **Dep:** `ureq` (already in tree) — no new dep.
- **Feature flag:** `feature = "bigquery"` in `mc-drivers/Cargo.toml`
- **Constructor:** `pub fn bigquery_driver(project_id: &str, credentials_json: &str, query: &str) -> Result<impl SourceDriver, DriverError>`
- **Auth:** service-account JSON key file. The driver reads the key, generates a JWT, exchanges it for an access token via Google's OAuth2 token endpoint. All sync (ureq).
- **API flow:**
  1. POST `https://bigquery.googleapis.com/bigquery/v2/projects/{project}/jobs` — submit query job.
  2. Poll GET `.../jobs/{jobId}` until `status.state == "DONE"`.
  3. GET `.../jobs/{jobId}/getQueryResults?startIndex=0&maxResults=<batch_size>` — paginate results.
- **Rate limiting:** BigQuery has generous rate limits (100 concurrent queries per project). No special rate limiting needed in the driver.
- **Test fixture:** mock HTTP responses. Integration tests gated behind `#[cfg(feature = "bigquery-integration")]`.

---

## Cron scheduling architecture (detailed)

### Module layout

```
crates/mc-tessera/src/
  schedule/
    mod.rs          # pub use re-exports
    registry.rs     # Schedule struct, load/save schedules.json
    cron_expr.rs    # Cron expression parser + next-fire calculator
    daemon.rs       # The main loop
    commands.rs     # CLI command handlers (schedule, list, remove)
```

### Cron expression parser

Implement a minimal 5-field cron parser. Do NOT add a cron-parsing crate
(keep deps minimal). The parser handles:

- Standard 5-field: `minute hour dom month dow`
- Wildcards: `*`
- Ranges: `1-5`
- Steps: `*/5`, `1-30/2`
- Lists: `1,3,5`
- Day-of-week: 0=Sunday through 6=Saturday
- Named days: `MON`, `TUE`, etc. (case-insensitive)
- Named months: `JAN`, `FEB`, etc.
- Presets: `@hourly`, `@daily`, `@weekly`, `@monthly`, `@yearly`
- Sub-minute: `@every Ns`, `@every Nm`, `@every Nh` (N seconds/minutes/hours)

The parser produces a `CronExpr` struct with a `fn next_fire(&self, after: SystemTime) -> SystemTime` method.

### Daemon lifecycle

1. On start: read `schedules.json`. Write `daemon.pid`.
2. Compute the next wake time (earliest `next_fire` across all active schedules).
3. Sleep until wake time (clamped to 60s max to handle new schedules being registered externally).
4. On wake: re-read `schedules.json` (handles external additions/removals while daemon runs).
5. For each schedule where `now >= next_fire(last_run)`: execute recipe.
6. Persist updated schedule state after each execution.
7. On SIGTERM/SIGINT: set a flag, finish current recipe (if any), exit cleanly.

### Signal handling

Use `std::sync::atomic::AtomicBool` for the shutdown flag. Register signal
handlers via a thin platform-specific layer:

```rust
#[cfg(unix)]
fn install_signal_handler(flag: &'static AtomicBool) {
    // Use signal_hook crate or raw libc::signal
}
```

If adding `signal-hook` as a dep is unacceptable (dep minimalism), use raw
`libc::signal` behind `#[cfg(unix)]` — this is one of the rare cases where
a thin unsafe FFI is justified for correct daemon behavior. Document it.
On Windows, use `SetConsoleCtrlHandler` via `winapi` or skip graceful
shutdown (document as a known Windows limitation).

### Daemon security considerations

The daemon runs as the invoking user. It does NOT:
- Elevate privileges
- Listen on any network port
- Accept remote commands
- Run arbitrary shell commands (except the `--on-failure` hook, which the user explicitly configures)

The PID file prevents accidental double-start. If the PID file exists and
the process is still running, `mc tessera daemon` exits with a clear error.

---

## Recipe schema extensions summary

New fields added to `mc-recipe/src/schema.rs`:

```rust
// In ColumnMapping:
pub time_format: Option<String>,
pub time_timezone: Option<String>,
pub map_to_period: Option<String>,

// Top-level Recipe:
pub incremental_config: Option<IncrementalConfig>,

// New struct:
pub struct IncrementalConfig {
    pub strategy: IncrementalStrategy,
    pub column: String,
    pub format: Option<String>,        // "iso8601" | "unix_epoch" | "integer"
    pub initial_value: Option<String>,
    pub param_name: Option<String>,    // for HTTP drivers: query param name
}

pub enum IncrementalStrategy {
    Watermark,
    Cursor,
}

// WriteDisposition gains two variants:
pub enum WriteDisposition {
    Replace,
    Append,
    Merge,
}

// OnMissingElement gains one variant:
pub enum OnMissingElement {
    Error,
    Create,
}

// DriverKind gains 5 variants:
pub enum DriverKind {
    Csv, Sqlite, Duckdb, Postgres, DuckdbPostgres, HttpJson,
    // Phase 5C additions:
    Mysql,
    D1,
    Snowflake,
    Sqlserver,
    Bigquery,
}
```

---

## Hard rules

1. **Rust 1.78 toolchain pin remains.** Every new dep must be verified via the fresh-resolve protocol from ADR-0010 Amendment 1.
2. **No async in Mosaic source.** The cron daemon is a sync loop with `std::thread::sleep`. No `tokio`, no `.await`, no `async fn` anywhere.
3. **New deps require PM approval** per ADR-0010 amendment #6. Phase 5C has pre-approved drivers (MySQL, ODBC, ureq-reuse for D1/BigQuery). Any dep not listed here: open a SPEC QUESTION.
4. **mc-core, mc-model, mc-fixtures are LOCKED.** Zero changes permitted.
5. **Feature flags for all new drivers.** Default compilation (no features) must still produce a working binary with the Phase 5A drivers only.
6. **One driver failing CI does not block another.** Each driver's tests are gated behind its feature flag.
7. **The daemon is NOT a service manager.** It does not supervise child processes, manage systemd units, or write init scripts. It is a single foreground loop. Users wrap it in systemd/launchd/supervisor themselves.
8. **Incremental state is written AFTER successful commit only.** A failed import never advances the watermark. This prevents data loss from partial imports.

---

## SPEC QUESTION triggers

Open a SPEC QUESTION (do not proceed silently) if you encounter:

1. **ODBC system-library dependency breaks CI.** The ODBC drivers require system-installed libraries that may not be present in CI. Proposed resolution: gate ODBC driver tests behind integration-test features; skip in default CI; document system requirements prominently.

2. **Cron daemon security: should the `--on-failure` hook shell-exec arbitrary commands?** The hook runs as the daemon's user. A malicious schedules.json could inject commands. Proposed resolution: require the hook command to be specified at `mc tessera daemon` startup (CLI arg), NOT in `schedules.json`. The schedule file only references recipes.

3. **D1 rate limiting: is 250ms sleep between requests sufficient?** The 4 req/s limit is per-account, not per-database. If multiple Mosaic instances hit the same account simultaneously, they'll collectively exceed the limit. Proposed resolution: accept 250ms spacing as sufficient for single-instance; document the multi-instance caveat; implement retry-on-429 with exponential backoff.

4. **Incremental state corruption recovery.** If `incremental/<recipe>.state.json` is corrupted or manually edited to an invalid value, the next run may skip data or duplicate data. Proposed resolution: validate state on load; if invalid, treat as "no state" (full reload) + emit a warning diagnostic.

5. **`on_missing_element: create` and concurrent imports.** If two imports run simultaneously and both try to create the same element, they may conflict. Proposed resolution: Phase 5C is single-process (daemon runs one recipe at a time). Document that concurrent `mc tessera apply` invocations with `create` may race. Proper resolution is Phase 7+ (distributed coordination).

6. **BigQuery JWT generation without an external crate.** Generating a signed JWT requires RSA or EC signing. The pure-Rust options (`ring`, `rsa`) may have MSRV issues on 1.78. Proposed resolution: verify `ring` or `rsa` crate builds on 1.78 before committing. If neither works, fall back to `ureq` + external `gcloud auth print-access-token` command (document as a known limitation).

---

## Acceptance gates

### Per-driver gates

For each new driver (MySQL, D1, Snowflake, SQL Server, BigQuery):

- [ ] Implements `SourceDriver` trait correctly
- [ ] `schema()` returns accurate column metadata
- [ ] `fetch_batch()` paginates correctly (no data loss, no duplicates)
- [ ] `cancel()` is cooperative (next fetch returns None)
- [ ] Feature-flagged (compiles without the feature; no compilation errors in default build)
- [ ] Test fixture committed (unit tests pass in CI without external services)
- [ ] Integration test exists (gated behind feature flag; documents required setup)
- [ ] `DriverKind` variant added to recipe schema
- [ ] Recipe example committed to `examples/recipes/`

### Cron scheduling gates

- [ ] `mc tessera schedule <recipe> --cron "<expr>"` registers correctly
- [ ] `mc tessera schedule list` shows registered schedules
- [ ] `mc tessera schedule remove <id>` removes correctly
- [ ] `mc tessera daemon` starts, reads schedules, executes recipes on time
- [ ] `mc tessera daemon --once` fires all due schedules and exits
- [ ] Cron expression parser handles all standard 5-field patterns
- [ ] Sub-minute precision works (`@every 30s`)
- [ ] Retry-once-on-failure works (60s backoff, then mark failed)
- [ ] PID file prevents double-start
- [ ] Daemon survives recipe failures (doesn't crash on one bad recipe)
- [ ] `schedules.json` persists across daemon restart
- [ ] Signal handling: SIGTERM causes graceful shutdown

### Incremental load gates

- [ ] Watermark strategy: high-water-mark advances only on success
- [ ] Cursor strategy: last-seen PK advances only on success
- [ ] `mc tessera reset-state <recipe>` clears state (next run = full load)
- [ ] State file at `<model_dir>/.tessera/incremental/<name>.state.json`
- [ ] First run with no state does a full load
- [ ] State corruption (invalid JSON) triggers full reload + warning

### Write disposition gates

- [ ] `append`: existing coordinates preserved; new coordinates inserted
- [ ] `merge`: existing coordinates overwritten; new coordinates inserted
- [ ] `replace`: unchanged behavior from Phase 5A

### Element auto-creation gates

- [ ] `on_missing_element: create` adds leaf elements to the dimension
- [ ] New elements have no hierarchy parent (orphan leaves)
- [ ] Warning fires if >100 elements created in one import
- [ ] `on_missing_element: error` (default) unchanged from Phase 5A

### Time format enforcement gates

- [ ] MC5030: non-ISO date without `time_format` fires at validation time
- [ ] MC5031: timezone-less timestamp without `time_timezone` fires
- [ ] MC5032: non-IANA timezone string fires (with helpful suggestion)
- [ ] MC5033: parsed date with no matching Time element fires
- [ ] ISO 8601 auto-detected (no `time_format` needed)
- [ ] `map_to_period` correctly buckets dates into Time elements

### Global gates

- [ ] `cargo build --workspace --all-features` zero warnings
- [ ] `cargo clippy --all-targets --workspace --all-features -- -D warnings` exits 0
- [ ] `cargo fmt --check --all` exits 0
- [ ] `cargo test --workspace --all-features` passes (excluding integration tests)
- [ ] All existing Phase 5A/5B tests still pass (no regressions)
- [ ] No new dependencies in `mc-core`
- [ ] Forbidden-pattern grep clean in `mc-core/src/`

---

## Implementation order (recommended)

1. **Recipe schema extensions** (mc-recipe) — add the new fields, enums, serde support. Everything downstream depends on these types existing.
2. **Write dispositions** (mc-tessera/runner.rs) — small behavioral change in the existing orchestrator; unlocks `append`/`merge` immediately.
3. **`on_missing_element: create`** (mc-tessera/transform.rs) — another small orchestrator change.
4. **Incremental loads** (mc-tessera/incremental/) — new module; state persistence + watermark injection.
5. **MySQL driver** (mc-drivers) — pure Rust, no system deps, fast to validate.
6. **D1 driver** (mc-drivers) — uses existing `ureq`, interesting pagination logic.
7. **BigQuery driver** (mc-drivers) — uses existing `ureq`, JWT auth challenge.
8. **ODBC drivers** (mc-drivers) — Snowflake + SQL Server share `odbc-api`; ship together.
9. **ADR-0014 time_format enforcement** (mc-tessera/transform.rs + mc-recipe/validator.rs) — MC5030-MC5033.
10. **Cron scheduling** (mc-tessera/schedule/) — the largest feature; do last after the orchestrator is fully extended with incremental/append/merge/create.
11. **DuckDB federation documentation** — pure docs + examples; no code.

This order minimizes blocked dependencies: the schema changes unlock
everything; the orchestrator changes are small and independent; the drivers
are fully parallel; the cron daemon builds on the complete orchestrator.

---

## Files you will modify

| File | Changes |
|---|---|
| `crates/mc-recipe/src/schema.rs` | New enums, new fields on ColumnMapping, IncrementalConfig struct |
| `crates/mc-recipe/src/validator.rs` | MC5030-MC5033 validation rules |
| `crates/mc-drivers/src/lib.rs` | New driver re-exports, new DriverError variants |
| `crates/mc-drivers/src/mysql_driver.rs` | NEW |
| `crates/mc-drivers/src/d1_driver.rs` | NEW |
| `crates/mc-drivers/src/snowflake_driver.rs` | NEW |
| `crates/mc-drivers/src/sqlserver_driver.rs` | NEW |
| `crates/mc-drivers/src/bigquery_driver.rs` | NEW |
| `crates/mc-drivers/Cargo.toml` | New optional deps + feature flags |
| `crates/mc-tessera/src/runner.rs` | append/merge logic, on_missing_element:create, incremental state |
| `crates/mc-tessera/src/schedule/` | NEW module (daemon, registry, cron parser, commands) |
| `crates/mc-tessera/src/incremental.rs` | NEW (state management) |
| `crates/mc-tessera/Cargo.toml` | Any new deps for signal handling |
| `crates/mc-cli/src/main.rs` | New CLI verbs (schedule, daemon, reset-state) |
| `Cargo.toml` (workspace) | New dep pins if needed |
| `examples/recipes/` | New example recipes for each driver + cron + incremental |

---

## What success looks like

Phase 5C is done when a user can:

1. Write a recipe targeting MySQL, D1, Snowflake, SQL Server, or BigQuery and run `mc tessera apply` successfully.
2. Schedule a recipe to run every 30 seconds and see the daemon execute it on time.
3. Run an incremental load that only fetches rows newer than the last import.
4. Import data with `write_disposition: merge` that correctly upserts.
5. Import data referencing unknown elements and have them auto-created.
6. Specify `time_format: "M/d/yyyy"` on a date column and have Tessera parse it correctly into the Time dimension.
7. See clear diagnostic messages (MC5030-MC5033) when time format configuration is wrong.

The sports betting proof-of-concept: a recipe scheduled `@every 30s` that
ingests live odds from an HTTP/JSON endpoint, incrementally (watermark on
`last_updated`), with `merge` disposition and `on_missing_element: create`,
executing reliably via `mc tessera daemon` for hours without intervention.
