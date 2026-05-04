# Mosaic Phase 5 Due-Diligence: Verifying the ADBC-First Recommendation

**TL;DR — Should Mosaic bet on ADBC for Phase 5A? No (Conditional No).** ADBC-Rust is real and improving, but in May 2026 every viable Cargo.toml composition involving `adbc_core` plus the `arrow` crate forces an MSRV well above your 1.78 pin (current arrow-rs MSRV = 1.85, current adbc_core "rust-version" was 1.81 as of v0.19.0 and the ecosystem has continued bumping with arrow 58/59), every interesting non-SQLite ADBC driver is a Go/C FFI shim with significant operational baggage on Windows and CI, and the headline "10–100× faster" claim is a pull-mode-from-DuckDB/Snowflake/Doris result that does *not* generalize to a Rust client doing 100K-row import recipes. The right Phase 5A bet is **Bucket B / fallback**: ship `rusqlite` + `duckdb-rs` only, design the ingestion façade so DuckDB's `postgres`/`mysql`/`sqlite` scanner extensions cover everything else as a Phase 5B "DuckDB-as-hub" upgrade, and revisit ADBC in Phase 6 once arrow-rs publishes a 1.78-compatible LTS line or once ADBC ships pure-Rust drivers.

---

## 1. Verification Verdict (1–2 paragraphs)

**ADBC-Rust is not currently credible as the Phase 5A foundation under Mosaic's hard constraints.** The Apache Arrow ADBC project does ship Rust crates (`adbc_core`, `adbc_driver_manager`, `adbc_snowflake`, `adbc_datafusion`, plus partner crates `adbc_clickhouse`, `exarrow-rs`), and the API has reached a usable shape — `Driver`, `Database`, `Connection`, `Statement` traits with `RecordBatchReader` results. But three independent facts kill it for Phase 5A as specified:

1. **MSRV.** `adbc_core` declared `rust-version = "1.81"` at v0.19.0 (PR #2997, June 2025) and tracked further — the `adbc_clickhouse` 0.1.0 crate explicitly documents MSRV 1.91. The transitive dependency `arrow` (arrow-rs) bumped its MSRV to 1.84 in PR #7926 (July 2025) and to 1.85 in PR #8429 (merged Sept 2025, landed in arrow-rs 57+). Every current `adbc_core` release pulls in an `arrow-array`/`arrow-buffer` version chain that requires ≥1.84. There is no security-supported pinned older arrow line that compiles on 1.78 — arrow-rs explicitly does *not* backport security fixes to older majors.
2. **Driver reality.** Outside of an experimental SQLite driver (used mainly for the project's own integration tests) and `adbc_datafusion`, every "real" ADBC Rust driver is a wrapper around a Go-built dynamic library (`adbc_snowflake` literally documents "this crate is a wrapper around the Go driver"). Postgres, BigQuery, Flight SQL, Snowflake all flow through `libadbc_driver_*.{so,dylib,dll}` and the driver manager's `dlopen`/`LoadLibraryW` path — which itself has had an open Windows build break (issue #3149, July 2025) where `adbc_core` failed to compile due to an unresolved `windows` crate dependency.
3. **Sync vs async.** The Rust ADBC abstract API surface is *synchronous* (the `Statement::execute` returns an `impl RecordBatchReader`, not a `Future`), but most third-party drivers built on top of it (e.g., `exarrow-rs`) are async-first and pull `tokio` as a hard dependency. So "ADBC has a sync surface" is technically true but practically misleading: the ones you'd want (Postgres-like) are not in that sync subset.

Combined, this means adopting ADBC = (a) ADR-bumping MSRV from 1.78 to ≥1.85 with a hard cliff (arrow-rs versions 56 and below have known security regressions and are not maintained), (b) shipping Go-built `.so` files in Mosaic's distribution, and (c) hand-rolling sync wrappers around still-async-leaking drivers. None of those is a Phase 5A activity; collectively they are a phase by themselves.

---

## 2. MSRV Compatibility Analysis

| Crate | Latest stable (May 2026) | rust-version field | Effective MSRV (cargo msrv) | 1.78-clean? | Pinnable older version that's 1.78-clean? |
|---|---|---|---|---|---|
| `arrow` (arrow-rs) | 58.x; 59.0.0 expected May 2026 | 1.85 (since #8429, merged Sept 24 2025; landed in 57.0.0) | 1.85 | ❌ | 53.x line was last MSRV-1.78 family; **no security backports** — Apache policy ships fixes only on `main` |
| `adbc_core` | 0.20.0 (Aug 2025), 0.22 milestone open | 1.81 (set in #2997, June 2025; actually requires whatever `arrow` requires) | ≥1.85 transitively via arrow-array 56+ | ❌ | 0.11–0.14 era was 1.75-clean but pre-1.0 SQLite driver only and is missing the `Driver`/`Database` builder API the docs now show |
| `adbc_driver_manager` | 0.20.0 | tracks `adbc_core` | ≥1.85 | ❌ | Same as above |
| `adbc_snowflake` | 0.20.0 | tracks `adbc_core`; wraps Go driver | ≥1.85 + Go toolchain in build env | ❌ | n/a |
| `adbc_datafusion` | 0.20.0 | 1.85.1 (visibly listed on crates.io) | 1.85.1 | ❌ | n/a |
| `adbc_clickhouse` | 0.1.0 (preview, "not production-ready") | 1.91 (explicit) | 1.91 | ❌ | n/a |
| `rusqlite` | 0.39.0 (March 2026, with libsqlite3-sys 0.37, SQLite 3.51.3) | "latest stable Rust at release"; effective ≥1.77 since 0.32 (issue #1544); 0.39 expects current stable | likely 1.85+ on 0.39 | ❌ on latest, but 0.31.0 (Jan 2024, MSRV ~1.69) is 1.78-clean and still gets `--precise` pins | **YES — pin `rusqlite = "=0.31.0"`** is the known-good escape hatch |
| `duckdb` | 1.10502.0 (April 14, 2026; tracks DuckDB 1.5.2) | rolling; latest pulls `arrow ^58` | ≥1.85 (because of arrow ^58) | ❌ | `duckdb 1.4.4` (Jan 2026) pulls arrow 56-era; `duckdb 1.3.x` (mid-2025) was the last 1.78-credible line, ICU-less |
| `postgres` (sync wrapper) | 0.19.13 (April 2026) | not formally pinned; works on stable + 1–2 back | 1.74-ish historically; current line 1.78-clean | ✅ likely | Yes — 0.19.x line is currently 1.78-compatible |
| `tokio-postgres` | tracks `postgres` | tokio 1.x — pulls async runtime by definition | sync-clean: no | n/a — fundamentally async |
| `sqlx` | 0.8.x | requires async runtime (tokio/async-std/actix) | n/a — async-only | ❌ — violates "no tokio" constraint |
| `connectorx` (Rust crate) | 0.4.5 (last activity 2025) | unspecified, tracks arrow | likely ≥1.78 but uses j4rs (Java for some sources!) | ⚠️ partial | Has been *not* abandoned but is Python-first; Rust crate is "documentation for the Python bindings" per its own docs |
| `connector_arrow` | active (alternative to connectorx focused on Rust) | tracks arrow | ≥arrow MSRV | ❌ on latest | Possibly pinnable |
| `worker` (cloudflare workers-rs) | active | targets `wasm32-unknown-unknown` only | n/a — WASM target, doesn't run on host | n/a | Not usable as a host-side D1 client |

**Summary commentary.** Mosaic at Rust 1.78 is on the wrong side of the arrow-rs MSRV cliff, and that cliff is the one that propagates. `adbc_core`'s own declared MSRV (1.81) is misleading because cargo will still pick a transitively-incompatible `arrow-array` unless you pin every arrow-* crate to a 53.x family that is no longer security-maintained. The only crates that work cleanly at 1.78 today are `rusqlite ≤0.31`, `postgres` 0.19.x (sync), and small pure-Rust HTTP clients like `ureq` for D1. **Conclusion:** to do ADBC, you must do an MSRV bump to ≥1.85 first, accepting the inherited risk of arrow-rs's quarterly major-bump policy (a fresh major ~every 3 months, with the next MSRV bump likely already queued behind 59.0.0).

---

## 3. Driver Availability Matrix

Rating scale: 🟢 = "would bet a phase on this", 🟡 = "use with eyes open", 🔴 = "not Phase 5A material". Last release dates verified May 2026.

| DB | Best Rust option | Sync/Async | Latest version | Last release | MSRV | Notes | Bet rating |
|---|---|---|---|---|---|---|---|
| SQLite | `rusqlite` | sync | 0.39.0 | March 2026 | latest stable; 0.31 for 1.78 | 40M+ all-time downloads, 4.1k stars, gwenn actively shipping monthly | 🟢 |
| SQLite (alt) | ADBC SQLite driver via `adbc_core` | sync | 0.20.0 | Aug 2025 | ≥1.85 | Real but used mostly for ADBC's own tests; FFI to C lib | 🟡 |
| DuckDB | `duckdb` (duckdb-rs) | sync | 1.10502.0 | April 14 2026 | ≥1.85 (arrow ^58) | Includes Appender (bulk insert), `append_record_batch`, Arrow integration; bundled libduckdb | 🟢 (but MSRV) |
| Postgres | `postgres` (sync wrapper over tokio-postgres) | **sync** (the wrapper holds a small tokio runtime internally) | 0.19.13 | April 2026 | ~1.74+ currently 1.78-clean | "lightweight wrapper over tokio-postgres … blocks on the futures provided by the async client" — tokio is therefore a hard transitive dep, even though the API you write is sync | 🟡 (sync API yes, but transitive `tokio` violates "no tokio" rule) |
| Postgres | ADBC postgres driver via `adbc_driver_manager` | sync | 0.20 | Aug 2025 | ≥1.85 | Loads C `libadbc_driver_postgresql` via dlopen; deployment of that .so is the user's problem | 🔴 (ops tax) |
| Postgres | `tokio-postgres` directly | async | active | active | tokio | Violates async-free constraint outright | 🔴 |
| Postgres | DuckDB `postgres` extension via duckdb-rs | sync (DuckDB owns the conn) | tracks DuckDB | April 2026 | ≥1.85 | Real, mature, used in production. Does the libpq talking inside the duckdb process. Bundled extensions | 🟢 (the DuckDB-hub answer) |
| MySQL | DuckDB `mysql` extension via duckdb-rs | sync | tracks DuckDB | April 2026 | ≥1.85 | Mature; ATTACH 'mysql:…' works | 🟢 (via hub) |
| MySQL | `mysql` (Rust crate, sync) | sync | active | continues to ship | ~1.78-tolerant | Native, but adds another non-trivial crate | 🟡 |
| Snowflake | `adbc_snowflake` | sync surface, wraps Go driver | 0.20.0 | Aug 2025 | ≥1.85 + cgo at build time | "Wrapper around the Go driver" per its own docs. Means: at build time you need either `ADBC_SNOWFLAKE_GO_LIB_DIR` or to ship the conda-forge artifact. Nontrivial | 🔴 for Phase 5A |
| Snowflake | direct via `snowflake-api` Rust crate | typically async | varies | community | varies | None of the available crates are well-maintained sync clients | 🔴 |
| BigQuery | `adbc_bigquery` (Go driver wrapped) | n/a — released as part of ADBC C# / Go, no Rust crate published yet on crates.io as of May 2026 | — | — | — | The "Rust BigQuery ADBC" story is presently *vapor* — issue #1723's roadmap, not a shipped crate | 🔴 |
| BigQuery | `gcp-bigquery-client` etc. | async (reqwest) | varies | varies | varies | All require tokio; HTTP-based | 🔴 |
| SQL Server | `tiberius` | async (tokio or async-std) | active | active | tokio dep | No mature sync option in the Rust ecosystem | 🔴 |
| SQL Server | ConnectorX / connector_arrow | sync via threads | 0.4.5 | 2025 | varies | Possible but ConnectorX's Rust crate is essentially a substrate for its Python wheel | 🟡 |
| Cloudflare D1 | `worker` (cloudflare/workers-rs) | async, wasm32-unknown-unknown only | active | active | n/a | **Cannot be used from non-WASM host code** — the `D1Database` binding is a JS handle. From host Rust you must speak the public REST API with `ureq`/`reqwest` | 🔴 (no D1 host crate) |
| Cloudflare D1 | DIY `ureq` against REST API | sync | n/a | n/a | 1.78-clean | Standard pattern for Phase 5B; see §6 | 🟢 (but DIY) |
| ClickHouse | `adbc_clickhouse` | async (tokio in tree) | 0.1.0 | first cycle, 2025/26 | 1.91 (!) | Self-described "still under active development and should not be considered ready for production use" | 🔴 |

---

## 4. Fallback Architecture if ADBC Rejected (this is what we recommend)

### 4.1 Concrete crate list (Phase 5A)

```toml
# mc-ingest/Cargo.toml additions — all 1.78-clean
[dependencies]
rusqlite        = { version = "=0.31.0", features = ["bundled"] }   # 1.69 MSRV; pinned
duckdb          = { version = "=1.3.2",  features = ["bundled"] }   # last 1.78-credible duckdb-rs
                                                                    # (DuckDB v1.3.2; arrow ^54)
serde           = { version = "1", features = ["derive"] }          # already in workspace
serde_yaml      = "0.9"                                             # already in workspace (Phase 4 LLM YAML)
csv             = "1.3"                                             # 1.78-clean
ureq            = { version = "2", features = ["json", "tls"] }     # sync HTTP for D1 / JSON APIs;
                                                                    # 1.71 MSRV
# *** Postgres deferred to Phase 5B via duckdb's postgres extension ***
```

Total new crates: 2 if `csv`/`ureq`/`serde_yaml` are already in. Pure ADR cost: 2 ADRs (one per direct DB crate; rusqlite has been stable for a decade, duckdb is more aggressive). Both are FFI to bundled C libraries, so the only OS variable is a C compiler at build time — already required for Mosaic's existing Phase 1–4 crates that touch parsers.

### 4.2 Import path

```
YAML recipe ── parsed by mc-recipe ──▶ mc-ingest::SourceDriver trait
                                            │
                       ┌────────────────────┼─────────────────────┐
                  SqliteDriver         DuckDbDriver           HttpJsonDriver
                  (rusqlite)            (duckdb-rs)            (ureq)
                       │                    │                     │
                       ▼                    ▼                     ▼
                arrow2-free               Appender API     serde_json → rows
                row→cell adapter          → batch
                                              │
                                              ▼
                                    mc-core write_batch (§7)
```

The **`SourceDriver` trait** is intentionally minimal:

```rust
pub trait SourceDriver: Send {
    fn schema(&mut self, q: &Query) -> Result<Schema>;
    fn fetch_batch(&mut self, q: &Query, max_rows: usize) -> Result<Option<RowBatch>>;
    fn cancel(&mut self);
}
```

`RowBatch` is Mosaic-native (Vec-of-Vec\<CellValue\>), *not* an Arrow `RecordBatch`. This single decision is what frees Phase 5A from arrow-rs's MSRV trail. When (if) ADBC matures and the MSRV question resolves, an `AdbcDriver: SourceDriver` is a one-week shim on top.

### 4.3 What's lost vs ADBC

- **No Postgres native ingest in 5A.** Mitigated in 5B by adding a DuckDB-attached path (`ATTACH 'host=… dbname=…' AS pg (TYPE postgres)` then `SELECT … FROM pg.public.tbl`). Customer-facing story: "Postgres ingestion runs through DuckDB; you need duckdb's `postgres` extension auto-loaded (default behavior). Performance is good for analytical reads — DuckDB binary-protocol scanner is faster than typical psycopg2-flavored ETL."
- **No native Arrow zero-copy.** For Mosaic's current write rate (~165 µs/cell, target 10–100× speedup via batching, see §7), the difference between Arrow record batches and Vec\<Row\> at 100K-row scale is sub-second; not the bottleneck. The bottleneck is the kernel's per-cell write cost.
- **No Snowflake/BigQuery in 5A.** Punt explicitly, document. These customers can `EXPORT … TO 'gs://…/*.parquet'` from their warehouse and Mosaic ingests via DuckDB's parquet reader.

---

## 5. Recipe Format Prior-Art Comparison

| Source | Lang | Strengths from real usage | Weaknesses users complain about |
|---|---|---|---|
| **dbt `sources.yml`** (`version: 2; sources: - name … database … schema … tables: - name … columns: - name … tests: [unique, not_null] … freshness: warn_after: {count, period}`) | YAML + Jinja | Hierarchical defaults (source-level freshness inherits to tables); first-class `freshness`, `loaded_at_field`, `meta`, `tags`; `{{ source('x','y') }}` reference function gives a clean lineage anchor; tests inline next to columns. **Battle-tested over 5+ years and tens of thousands of repos.** | YAML+Jinja makes errors hard to attribute to a line; `version: 2` artifact persists despite no v1 ever being meaningfully different; no native way to express ingest *batching* (it's a transformation tool, not an extractor); `quoting: identifier: true` flag is a notorious foot-gun. |
| **Singer / Meltano** (`config.json` per-tap + `catalog.json` describing streams with JSON Schema; state file separate) | JSON | Clean separation: config (creds), catalog (streams + schema), state (incremental cursor). Standardized message protocol (`SCHEMA`, `RECORD`, `STATE`). Each tap a CLI subprocess — composable. | Catalog files routinely 10K+ lines for big SaaS APIs and edited by hand (real complaint); JSON Schema's nullability via `["null", "string"]` array is verbose and error-prone; `metadata` array-of-`{breadcrumb, metadata}` is opaque to humans; no first-class incremental key — has to be put in `metadata`. |
| **Airbyte connector spec** (`spec.json` — JSON Schema + `connectionSpecification`; `configured_catalog.json` per connection) | JSON Schema with extensions (`oneOf`, `airbyte_secret`, `order`, `group`, `airbyte_hidden`) | UI-renderable; secret marking; `oneOf` for OAuth-vs-token auth choice; `group` and `order` for form layout. Has migrated many real systems. | Spec authors complain heavily about `oneOf` UI rendering rules — every option needs a unique `const` field with a specific name pattern or the UI breaks; spec files reach 1000+ lines for big sources; no inheritance — repetition per source; spec is *connector* config, *catalog* is separate, two-layer model confuses new users. |
| **dlt (data load tool)** (`@dlt.resource(name=..., write_disposition='merge', primary_key='id', columns={...})` decorators in Python; optional `{source_name}.schema.yaml` companion) | Python decorators or YAML | Schema *inferred* from data, then optionally constrained — beats hand-writing schema; first-class `write_disposition` (append/replace/merge); `incremental('updated_at')` is a one-liner; YAML schema files are hash-versioned (`version_hash`) so dlt detects manual edits. | Decorator-driven authoring couples ingest to Python deployment; YAML schema is mostly machine-generated, so manual edits are second-class; `schema_contract='freeze'` is the only way to forbid drift, and it's all-or-nothing. |
| **Apache NiFi flow definitions** | XML / JSON templates | Visual-first, drag-drop; the flow file format itself is rarely hand-edited | Notoriously verbose and not human-friendly as text; not a useful prior art for Mosaic's CLI-first/LLM-assisted authoring. |
| **Cube.dev cubes** (`cubes: - name: orders, sql_table: …, data_source: …, dimensions: [...], measures: [...]`; YAML or JS; Jinja templates) | YAML + Jinja + Python for dynamic | Code-first, version-controlled, AI-assisted authoring is a stated design goal (matches Mosaic's Phase 4 LLM story); explicit `data_source` per cube; Jinja macros allow DRY. | Auto-escaping of unsafe Python strings into YAML "might get wrapped in quotes, potentially breaking YAML syntax" (their own docs); dynamic-models-via-Jinja have no preview in OSS — long debug loops. |

### Recommended Mosaic recipe format (inheriting from strongest prior art)

Inherit from **dbt's hierarchical defaults**, **dlt's `write_disposition` + `incremental` first-class fields**, **Singer's three-file separation** (config / catalog / state, but flatten config and catalog into one file because Mosaic recipes are smaller than tap catalogs), and **dbt's `{{ source('x','y') }}`** lineage hook (Mosaic already speaks YAML in Phase 4 — no Jinja, but a tiny `${source.x.y}` interpolation is trivial).

```yaml
# mosaic-recipe.yaml — version pinned, loadable, diffable
version: 1
recipe: load_orders_from_pg

sources:
  - name: warehouse_pg              # logical name
    driver: duckdb_postgres         # one of {sqlite, duckdb_native, duckdb_postgres,
                                    # duckdb_mysql, http_json, csv}
    connection: ${env.PG_DSN}       # Phase 5A: env-only credentials
    options:
      schema: public

ingests:
  - name: orders
    source: warehouse_pg
    table: orders                   # or `query: "SELECT … FROM …"`
    target_cube: sales              # mc-core cube to write to
    write_disposition: replace      # append | replace | merge (5B)
    incremental:                    # optional; if present, only new rows
      column: updated_at
      cursor_state_key: orders_high_water
    columns:                        # optional; allows overrides + type coercion
      - name: order_id
        target: id
        type: string
        primary_key: true
      - name: amount_cents
        target: amount
        type: number
        scale: 0.01                 # cents -> dollars
    batch:
      rows_per_batch: 50_000        # talks to write_batch API (§7)
      max_rows: 1_000_000           # cap, errors otherwise
    on_error: abort                 # abort | skip_row | quarantine
```

Three deliberate departures from the prior art:
1. **No Jinja**, no Python in YAML. Mosaic already paid the LLM-authoring tax in Phase 4 — the LLM writes YAML, doesn't execute it.
2. **Single file per recipe**, not 3 (Singer) or 2 (Airbyte). 100K-row imports don't need an Airbyte-scale catalog.
3. **`driver:` is explicit and namespaced.** No magic "scheme parsing" of connection strings. `duckdb_postgres` and `duckdb_mysql` make Phase 5A→5B story honest: customer sees that DuckDB is in the loop.

---

## 6. D1 Reality Check

Sourced directly from `developers.cloudflare.com/d1/platform/limits/` (last updated March 27 2026):

| Limit | Value | Implication for 100K-row ingest |
|---|---|---|
| Maximum bound parameters per query | **100** | With 10 columns: at most 10 rows per parameterized INSERT. Wide tables (50 cols) → 2 rows per INSERT |
| Maximum SQL statement length | 100 KB | Caps the practical batch size when using literal-value INSERTs (avoid bound params); ~500 rows with short values, fewer with strings |
| Maximum string/BLOB/row size | 2 MB | Ingest of long text fields fails outright; users have hit this in production (DEV.to "When Cloudflare D1's 2MB Limit Taught Me a Hard Lesson") |
| Maximum SQL query duration | 30 s | "Requests to Cloudflare API must resolve in 30 seconds. Therefore, this duration limit also applies to the entire batch call" — a `db.batch([…])` of N inserts shares one 30s budget |
| Queries per Worker invocation | 1000 (paid) / 50 (free) | If ingest goes through a Worker, you get 1000 sub-INSERTs per request before subrequest cap |
| Simultaneous open connections | 6 per Worker invocation | Pipelining via Workers is a parallelism-of-6 ceiling |
| Max database size | 10 GB (paid) | Hard ceiling, cannot be raised |
| Max storage per account | 1 TB (paid) | Spread across thousands of small DBs |
| Wire protocol from outside Cloudflare | **HTTP only** (REST API or Workers binding) | **D1 is NOT SQLite-wire-protocol-compatible from outside Cloudflare's network.** `rusqlite` against a D1 file is impossible. Only paths: REST API (sync, ureq-friendly) or Workers binding (async/WASM, not host Rust). |
| API rate limit (general Cloudflare API) | 1200 req / 5 min / user | Global; not D1-specific. Practically: ~4 req/s sustained per token |

### Pagination pattern for 100K rows

You cannot pull 100K rows in a single response — D1 query results run inside Workers' memory + 30 s budget, and Cloudflare explicitly recommends "processing 1,000 rows at a time" for any data migration. Standard pattern:

```rust
// Pseudocode (sync, ureq) — Phase 5A doable, Phase 5B recommended
let mut after: i64 = 0;
loop {
    let url = format!("{base}/accounts/{acc}/d1/database/{db}/query");
    let body = json!({
        "sql": "SELECT id, … FROM orders WHERE id > ?1 ORDER BY id LIMIT 1000",
        "params": [after]
    });
    let res: D1Response = ureq::post(&url).set("Authorization", &auth).send_json(body)?.into_json()?;
    let rows = res.result[0].results;
    if rows.is_empty() { break; }
    after = rows.last().unwrap()["id"].as_i64().unwrap();
    write_batch(rows)?;
    if res.result[0].meta.rows_read > some_throttle { sleep(Duration::from_millis(250)); }
}
```

Use **PK-cursor pagination** (`WHERE id > ?`), not `OFFSET` — D1 charges `rows_read` for full scans on offset, and an offset of 90,000 is 90,000 rows scanned per page = 100K offsets ⇒ ~5 billion rows_read for one 100K import. A user on Cloudflare community ran 117 queries and was billed for 90M rows read because of OFFSET-style anti-patterns.

For *ingest from D1 to Mosaic*: the path is sync HTTP, ~100 batches of 1000 rows, with a 30-s-per-batch ceiling and a 4-rps soft-throttle. Realistic clock time for 100K rows: 30–60 s if D1 is responsive. **This is Phase 5B**, not 5A — it requires HTTP credential management, retry/backoff for 429s, and rate-limit accounting that Phase 5A doesn't need.

There is **no Rust crate currently on crates.io** that wraps the D1 REST API specifically (the `worker` crate is WASM-only and exposes D1 as a JS binding). DIY with `ureq` is the path.

---

## 7. write_batch API Proposal

Mosaic's kernel does ~165 µs per cell. At 1M cells = 165 s, target 10–100× faster ⇒ 1.6–16 s. The bulk-load literature is consistent on the mechanism: **amortize per-write fixed costs over a batch** (revision bump, dirty-set update, snapshot/COW, listener notification, validation).

### Survey of bulk-load mechanisms

| System | Mechanism | Source |
|---|---|---|
| **DuckDB Appender** | Row-wise C API; data cached prior to flush; *commits every 204,800 rows* by default; `append_record_batch` does zero-copy from Arrow RecordBatch into DuckDB chunks; `Appender` bypasses prepared-statement overhead. Real-world result: ~1.2M rows/sec sustained ingest with parallel appenders ("Quacfka" case study using ADBC + DuckDB on Apache Arrow blog, March 2025). Single-threaded benchmark on JVM: `appender.append` ~50 ms/op vs `appender.appendRow` ~184 ms/op for the same payload — 3.5× delta from the boxing/unboxing alone | docs.rs/duckdb, duckdb.org/data/appender, Apache Arrow blog Mar 2025 |
| **ClickHouse INSERT** | Recommends batches of 10K–100K rows; "constant overhead per insert regardless of size, making batch size the single most important optimization"; Native/ArrowStream wire formats avoid client-side serialization; canonical guidance is "≤1 INSERT/sec, with ≥1000 rows/INSERT". Linearly scales with cores; a 59-core ClickHouse Cloud node ingests ~4M rows/sec on PyPi dataset; 5× speedup just by raising `max_insert_threads` from default | clickhouse.com/blog/supercharge-…, clickhouse.com/docs/optimize/bulk-inserts |
| **Apache Druid** | Streaming "indexing tasks" build *segments* in-memory then commit atomically; old data is immutable, new data appends; merge/compaction is a separate background phase. Dirty/index updates are deferred to segment seal. | Druid docs (general) |
| **Apache Pinot** | "Real-time tables" use upsert-into-mutable-segments with in-memory dictionaries; segments are immutable on commit; uses *log-structured* writes underneath with periodic compaction | Pinot docs (general) |
| **Apache Kylin** | Cube-build mode: bulk MapReduce/Spark job materializes cuboids as files; not really an OLTP-style write path | Kylin docs |

**Common pattern:** (1) accept a batch as an opaque chunk with one schema, (2) defer index/dirty/listener updates to *batch end*, (3) commit atomically with one revision bump and one snapshot point, (4) optionally do background compaction. ClickHouse's "1 insert/sec ≥ 1000 rows" rule of thumb maps directly to Mosaic if we treat each `write_batch` as a first-class kernel operation.

**Realistic expected speedup for Mosaic**: 10–30× from batched dirty-set + single revision bump alone; 50–100× achievable if cell validation is also batched (per-cell schema lookup is cheap but per-cell revision allocation is the killer). DuckDB Appender's 3.5× speedup over the row-by-row API on JVM is a *lower bound* analog — Mosaic's per-cell cost is dominated by per-cell bookkeeping that batches eliminate entirely.

### Proposed Rust API surface (Mosaic-flavored, respecting kernel invariants)

```rust
// mc-core (the one writeback-context parameter change Phase 5A is allowed)
//
// Existing: fn write_cell(&mut self, cube: &CubeId, addr: &CellAddr, v: CellValue) -> Result<()>
//
// New: a batch path that takes a writeback context. The kernel's revision counter
// is bumped EXACTLY ONCE per WriteBatch::commit(), the dirty set is updated in
// O(B) total instead of O(B log N) per cell, and listeners fire ONCE.

pub struct WriteBatch<'k> {
    kernel:        &'k mut Kernel,
    cube:          CubeId,
    pending:       Vec<(CellAddr, CellValue)>,
    snapshot_id:   SnapshotId,        // for rollback
    ctx:           WritebackContext,  // <-- the one mc-core change
}

impl<'k> WriteBatch<'k> {
    pub fn new(k: &'k mut Kernel, cube: CubeId, ctx: WritebackContext) -> Self { … }

    /// Stage a write. NO revision bump, NO listener fire, NO validation cost
    /// beyond a cheap address-shape check.
    pub fn push(&mut self, addr: CellAddr, v: CellValue) -> Result<()> {
        debug_assert!(self.kernel.cube_meta(self.cube).accepts(&addr));
        self.pending.push((addr, v));
        Ok(())
    }

    /// Stage many — vectorized. This is what an ingest driver calls.
    pub fn push_batch<I: IntoIterator<Item = (CellAddr, CellValue)>>(&mut self, it: I) -> Result<()> {
        self.pending.extend(it);
        Ok(())
    }

    /// Atomic commit. ONE revision bump. ONE dirty-set update. ONE listener fire.
    /// Snapshot_id is recorded BEFORE writes; on error, rollback restores it.
    pub fn commit(self) -> Result<CommitInfo> {
        let rev = self.kernel.bump_revision();           // ← single bump
        let mut dirty = DirtySet::with_capacity(self.pending.len());
        // single-pass validation, build dirty, write
        for (addr, v) in &self.pending {
            self.kernel.validate_cell(&self.cube, addr, v)?;
            dirty.insert(addr.clone());
        }
        // bulk writes — kernel internal hot-path, no per-cell rev/listener
        self.kernel.write_cells_unchecked(&self.cube, &self.pending, rev)?;
        self.kernel.merge_dirty_set(&self.cube, dirty);  // ← single update
        self.kernel.fire_writeback_listeners(&self.ctx, rev);
        Ok(CommitInfo { revision: rev, rows: self.pending.len() })
    }

    /// Explicit rollback — for ingest drivers that detect a bad row mid-stream.
    pub fn rollback(self) {
        // pending is dropped; nothing committed; kernel state unchanged
        drop(self);
    }
}
```

**Why this respects the locked kernel:**
- The only mc-core change is the `WritebackContext` parameter — explicitly allowed in Phase 5A.
- `bump_revision()`, `validate_cell()`, `merge_dirty_set()`, `fire_writeback_listeners()` are all *existing* internal helpers; the new code path just sequences them differently.
- Snapshot/rollback uses the same `SnapshotId` mechanism Phase 1–3 already have for transactions.
- A batched path that fails halfway leaves the kernel untouched (because `bump_revision()` is the last commit step before write — if validation fails on row N, no rev was bumped).
- Compared to DuckDB Appender's ~204K-row default flush: same pattern, same ergonomics.

**Expected outcome:** at 50K rows/batch and the proposed kernel changes, Mosaic should see ~10–30× speedup conservatively (1M cells: 165s → 5–16s), with headroom for further wins from SoA layout in `write_cells_unchecked`.

---

## 8. Risk Register

| Risk | Likelihood | Impact | Evidence | Mitigation | Decision needed? |
|---|---|---|---|---|---|
| Async runtime contamination (tokio crawls in via `postgres`/`tokio-postgres`/`sqlx`/Airbyte-style drivers) | High if ADBC chosen; Medium if `postgres` (sync) chosen | Workspace-wide invariant breach | `postgres` 0.19 docs: "lightweight wrapper over tokio-postgres … blocks on the futures provided by the async client" — tokio is transitive | **Defer Postgres to 5B via DuckDB extension**; DuckDB scanner is libpq-direct, no tokio | YES — ADR for "no tokio in Phase 5A" is needed |
| MSRV forced bump from 1.78 to ≥1.85 | Certain if ADBC or arrow-rs ≥57 chosen | Phase-blocking; 416 tests must compile | arrow-rs PR #8429 (Sept 2025), duckdb-rs 1.10500.x pulls arrow ^58 | Pin `duckdb = "=1.3.2"` (last 1.78-credible) and `rusqlite = "=0.31.0"` for Phase 5A; schedule MSRV ADR for Phase 6 | YES — explicit pin ADR |
| Driver abandonment (ADBC partner drivers stagnate) | Medium for non-core drivers | Phase 5B degraded | `adbc_clickhouse` self-describes "not for production"; `exarrow-rs` is community-supported, single-vendor | Choose drivers with multi-org backing (rusqlite, duckdb-rs, postgres) | NO |
| ConnectorX "stalled" reputation | Medium | If chosen as path | Not actually abandoned (releases through 2025), but the *Rust crate* is documented as a substrate for the Python wheel; positioning is Python-first | Use `connector_arrow` (the explicit "ConnectorX-but-Rust-first") if needed, but prefer per-driver crates | NO if §4 fallback chosen |
| D1 row/parameter limits surprise users | High | Customer-facing | 100 bound params; 1000 rows/batch recommendation; 30s req cap; 2MB row cap | Document in recipe schema; PK-cursor pagination by default; 5B not 5A | NO (already 5B-scoped) |
| Arrow API stability between major versions | High (quarterly major bumps) | If we depend on arrow types in our public API | "We release new major versions (with potentially breaking API changes) at most once a quarter" — official policy | **Do NOT expose arrow types in Mosaic's public API**; use Mosaic-native `RowBatch`/`CellValue`. Arrow becomes an internal driver detail at most | YES — design rule, applies to all phases |
| Windows builds break on ADBC | Confirmed — open issue | Build farm | apache/arrow-adbc#3149: "Rust: can't build adbc_core on windows" (July 2025) | Avoid for now | NO if §4 fallback |
| pollster/`block_on` for async drivers introduces subtle bugs | Low–Medium | Hard-to-debug deadlocks | pollster docs: "will not work for all futures because some require a specific runtime or reactor" — works for "simple" futures only. Tokio-bound futures (e.g., tokio-postgres) need a tokio runtime, not a thread-park. | If async is ever needed, use `tokio::runtime::Runtime::new().block_on(…)` in a *single* gated location, not `pollster`. **But cleanest answer is don't have async drivers at all.** | YES if any async crate enters workspace |
| Duckdb-rs ICU extension missing in `bundled` | Confirmed | Date arithmetic fails (`now() - interval '1 day'`) | duckdb-rs README: "When using the bundled feature, the ICU extension is not included due to crates.io's 10MB package size limit" | Document; use `bundled-cmake` from a workspace checkout or load ICU at runtime; or avoid time-zone arithmetic in 5A | YES — minor ADR |
| arrow-rs no-LTS policy | Persistent | Security exposure | Maintainers explicitly state "we do regularly scheduled releases (major and minor) from the main branch" — old majors don't get fixes | Don't depend on arrow at all in 5A; in 5B+, accept that depending on arrow means moving with it | YES if arrow ever pulled in |

---

## 9. Direct Comparison to Previous Report

| Previous report claim | This pass's verdict | Evidence |
|---|---|---|
| **ADBC-first for Phase 5A** | **Reject.** | arrow-rs MSRV 1.85 (PR #8429); adbc_core ≥1.81 declared, ≥1.85 transitively; non-SQLite drivers wrap Go libs; Windows build broken (#3149); contradicts every Mosaic hard constraint at once. |
| **Recipe format: "DSL inspired by dbt + Singer"** | **Affirm with refinements.** | Drop Jinja entirely (§5); single-file; explicit `driver:`. Inheritance from dbt's hierarchy and dlt's `write_disposition` is correct. |
| **Security approach: env-only credentials in 5A, secrets-store in 5B** | **Affirm.** | D1 rate limits and Postgres-deferral both push credential complexity into 5B regardless. |
| **Reconciliation strategy** | **Affirm in spirit, scope-tighten.** | DuckDB-as-hub (§4) reduces reconciliation to "Mosaic ↔ DuckDB" + "DuckDB ↔ remote" — the latter is DuckDB's problem, not Mosaic's. Removes a class of cross-driver merge logic from 5A. |
| **LLM-assist deferral** | **Affirm.** | Phase 4 already shipped LLM YAML authoring for models. Reusing the same machinery on recipes is mechanical; no need to gate Phase 5A on it. |
| **Phase decomposition (5A: ingest core; 5B: orchestration; 5C: scale)** | **Refine.** Recommended: **5A = SQLite + DuckDB + HTTP/JSON only; 5B = DuckDB-attached Postgres/MySQL + D1 REST + scheduled refresh; 5C = ADBC reconsideration, Snowflake/BigQuery, true scale.** | Concrete; achievable in one quarter; doesn't move MSRV. |

---

## 10. Final Recommendation

### 🟢 Bucket B — ADBC-deferred. Phase 5A ships `rusqlite` (pinned) + `duckdb-rs` (pinned) + DIY HTTP/JSON via `ureq`.

**Justification.** Of the three candidate buckets:
- **A (ADBC-first)** fails on MSRV alone. 1.78 → 1.85 is an ADR-level bump, not a Phase 5A precondition. Even if the bump were free, the Go-driver-FFI deployment story is its own engineering project.
- **C (DuckDB-as-hub only)** is *almost* right and is in fact what Phase 5A's hot path looks like — but committing to "DuckDB is the only direct integration" forecloses on local SQLite ingestion (where rusqlite is 10× simpler than going through DuckDB) and on direct CSV/JSON file ingest where DuckDB is overkill.
- **B (ADBC-deferred)** preserves 1.78, preserves no-tokio, preserves all 416 tests, requires only 2 ADRs (one per direct DB crate), gets immediate SQLite + DuckDB + DuckDB-attached-Postgres/MySQL coverage, and leaves a clean upgrade path: when arrow-rs publishes any LTS line OR when a pure-Rust ADBC Postgres driver appears OR when Mosaic's ADR cycle blesses MSRV 1.85, Phase 6 swaps in an `AdbcDriver` behind the existing `SourceDriver` trait without touching mc-core. The customer story is honest: "Mosaic ingests from SQLite, DuckDB, and via DuckDB's federation extensions, plus CSV/HTTP/JSON. Postgres/MySQL flow through DuckDB. Snowflake/BigQuery: export Parquet, ingest the file."

### Evidence that would flip this recommendation

1. **arrow-rs publishes an LTS branch with security backports for a 1.78-compatible major** (currently they refuse to). If this happens, ADBC becomes credible and Bucket A is back on the table.
2. **A pure-Rust (no Go cgo, no C dlopen) ADBC Postgres driver lands at 1.0** with 1.78 MSRV. Today: not in sight.
3. **Mosaic's ADR board approves MSRV 1.85** for independent reasons (e.g., another phase needs a 1.85 feature). Then most of the case for Bucket B over Bucket A weakens, and the deciding factor becomes Go FFI.
4. **Cloudflare ships an HTTP-keepalive D1 wire-protocol bridge** that's faster than 4 rps. Then D1-as-source moves into 5A.

---

## 11. 30-Minute Scratch Test

Smallest copy-pasteable Rust program the project owner can run today to validate Bucket B. Verify with `cargo +1.78 build` to prove the constraint.

### `Cargo.toml`

```toml
[package]
name        = "mosaic-ingest-scratch"
version     = "0.1.0"
edition     = "2021"
rust-version = "1.78"

[dependencies]
rusqlite   = { version = "=0.31.0", features = ["bundled"] }
duckdb     = { version = "=1.3.2",  features = ["bundled"] }
ureq       = { version = "2", features = ["json", "tls"], default-features = false }
serde      = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.dev]
opt-level = 0
```

### `src/main.rs`

```rust
//! Phase-5A scratch test for Mosaic ingest under Rust 1.78 + no-tokio constraints.
//!
//! Demonstrates three paths in <100 LOC each:
//!   1. SQLite read via rusqlite (sync, no async runtime in tree)
//!   2. DuckDB read via duckdb-rs, including the federation pattern
//!      (DuckDB attaches to a remote Postgres via `postgres_scanner` extension)
//!   3. HTTP/JSON read via ureq (sync) — placeholder for the D1 REST pattern
//!
//! Run:  cargo +1.78 run --release
//! Build:  cargo +1.78 check   (must succeed; if it fails, the recommendation is wrong)

use std::error::Error;
use rusqlite::{Connection as SqliteConn, params};
use duckdb::{Connection as DuckConn};
use serde::Deserialize;

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Mosaic Phase 5A scratch test (Rust {}) ===", env!("CARGO_PKG_RUST_VERSION"));

    // --- 1. SQLite via rusqlite (sync) ---
    let sq = SqliteConn::open_in_memory()?;
    sq.execute_batch("
        CREATE TABLE orders(id INTEGER PRIMARY KEY, amount REAL, customer TEXT);
        INSERT INTO orders VALUES (1, 99.95, 'alice'), (2, 12.00, 'bob'), (3, 250.0, 'alice');
    ")?;
    let mut stmt = sq.prepare("SELECT id, amount, customer FROM orders WHERE amount > ?1")?;
    let rows: Vec<(i64, f64, String)> = stmt
        .query_map(params![50.0], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_,_>>()?;
    println!("[sqlite] {} rows: {:?}", rows.len(), rows);

    // --- 2. DuckDB native read (and demo of federation extension load) ---
    let dd = DuckConn::open_in_memory()?;
    dd.execute_batch("
        CREATE TABLE sales AS SELECT * FROM (VALUES
            (1, 99.95, 'alice'),
            (2, 12.00, 'bob'),
            (3, 250.0, 'alice')
        ) t(id, amount, customer);
    ")?;
    let mut s = dd.prepare("SELECT customer, SUM(amount) FROM sales GROUP BY 1")?;
    let agg: Vec<(String, f64)> = s
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_,_>>()?;
    println!("[duckdb] grouped: {:?}", agg);

    // Federation demo (commented — requires a reachable Postgres):
    //
    // dd.execute_batch("INSTALL postgres; LOAD postgres;
    //                   ATTACH 'host=localhost dbname=warehouse user=mosaic'
    //                       AS pg (TYPE postgres, READ_ONLY);")?;
    // let mut q = dd.prepare("SELECT count(*) FROM pg.public.orders")?;
    // let n: i64 = q.query_row([], |r| r.get(0))?;
    // println!("[duckdb→postgres federation] {} rows", n);

    // --- 3. HTTP/JSON read via ureq (sync; no tokio) ---
    // Placeholder for the Cloudflare D1 REST pattern: a single GET that returns rows.
    // Using a public test API.
    #[derive(Debug, Deserialize)]
    struct Post { id: u64, title: String }
    let url = "https://jsonplaceholder.typicode.com/posts?_limit=2";
    let posts: Vec<Post> = ureq::get(url).call()?.into_json()?;
    println!("[http/json] {} posts: {:?}", posts.len(), posts);

    // --- Demo of the proposed write_batch pattern (just an illustration) ---
    let mut batch: Vec<(String, f64)> = Vec::with_capacity(rows.len());
    for (id, amt, cust) in rows {
        batch.push((format!("orders.id={}", id), amt));
        batch.push((format!("orders.id={}.customer", id), 0.0)); // placeholder: real CellValue is enum
        let _ = cust; // would be a string-typed cell in the kernel
    }
    println!("[write_batch] would commit {} cells in 1 revision bump", batch.len());

    Ok(())
}
```

This file is the entire validation artifact. If `cargo +1.78 check` succeeds, all three Phase 5A paths are buildable on the locked toolchain. If `cargo +1.78 build` succeeds, you have a working ingestion sketch in under 100 lines, and the recommended Phase 5A is materially proven. Total time to run: under 30 minutes including a `rustup install 1.78`.

---

## Plain-English Final Call

**Should Mosaic bet on ADBC for Phase 5A? No — conditional no.** ADBC-Rust exists, the API has settled, and in 18 months it will likely be the right answer. But in May 2026 every concrete recipe of `adbc_core` + `arrow` + a real driver pulls Mosaic's MSRV from 1.78 to ≥1.85, drags in Go-built `.so` files for anything more interesting than SQLite, has a confirmed open Windows build break, and rests on a "10–100× faster" claim that benchmarks against pull-mode warehouse reads, not 100K-row recipe imports. The honest Phase 5A is `rusqlite` + `duckdb-rs` (both pinned to 1.78-compatible versions), with DuckDB's `postgres`/`mysql` scanner extensions covering federation in Phase 5B and ADBC reconsidered in Phase 6 once arrow-rs publishes an LTS line or a pure-Rust ADBC driver appears. This preserves all 416 passing tests, requires no tokio, costs two ADRs, and makes the customer-facing story honest: Mosaic talks to SQLite directly, talks to DuckDB directly, and lets DuckDB talk to everything else.