---
name: mosaic-sql-mapping
description: How to author Mosaic Tessera recipes against SQL sources — SQLite, DuckDB, Postgres, and the DuckDB-attached-Postgres federation driver. Covers `query:` vs `table:` mode (and their mutual exclusion — MC5003), credential handling via `${env.VAR}` interpolation, SQL best practices for recipe queries (column aliasing, scoped WHERE clauses, leaf-only projections), and worked examples for each of the four SQL-family drivers. Use whenever the source is a SQLite / DuckDB / Postgres database, when a user provides a SQL query or table name, or when debugging an MC5003 / MC5013 / MC5015 fired against a SQL-family recipe. Builds on `skills/import/recipe-format/SKILL.md`.
---

# Authoring Mosaic Recipes for SQL Sources

This skill covers the four SQL-family drivers Phase 5A ships:

| `driver:` value | What it reads | Connection mechanics |
|---|---|---|
| `sqlite` | A local SQLite `.db` / `.sqlite` file | `path:` — filesystem path |
| `duckdb` | A local DuckDB `.duckdb` file | `path:` — filesystem path |
| `postgres` | A remote Postgres database | DSN supplied via `credentials.dsn` |
| `duckdb_postgres` | DuckDB engine attached to a remote Postgres instance | `path:` to local DuckDB + `credentials.dsn` for the attached Postgres |

The recipe schema and the six semantic rules live in [`../recipe-format/SKILL.md`](../recipe-format/SKILL.md). This skill goes deeper on the SQL-shaped concerns — query vs table mode, column aliasing, credential interpolation, and SQL best practices that produce clean recipes.

---

## `query:` vs `table:` (binding for SQLite, DuckDB, Postgres, DuckDB-attached-Postgres)

A SQL-family recipe must specify exactly one of:

- **`query:`** — a SQL statement whose result-set columns are the recipe's source columns.
- **`table:`** — a bare table name; equivalent to `SELECT * FROM <table>`.

Both set fires **MC5003**. Neither set produces an "no source columns" failure at runtime (caught by Stream C as a driver error, not by `mc-recipe`).

Default to **`query:` for any non-trivial source**. Use `table:` only when the source table's schema already matches the cube's dim/measure layout exactly (rare in practice).

```yaml
# RIGHT — query (most common):
source:
  driver: sqlite
  path: ./acme.sqlite
  query: |
    SELECT month, channel, market, spend, cpc
    FROM monthly_metrics
    WHERE scenario = 'Baseline'

# RIGHT — table (terse; only when the table's columns exactly match the recipe):
source:
  driver: sqlite
  path: ./acme.sqlite
  table: monthly_metrics

# WRONG — both set:
source:
  driver: sqlite
  path: ./acme.sqlite
  query: "SELECT ..."
  table: monthly_metrics    # → MC5003 (mutual exclusion)
```

---

## Driver-by-driver patterns

### SQLite — `driver: sqlite`

Use when:
- The source is a single `.sqlite` / `.db` file accessible by relative path from the recipe.
- You want zero-install dependency-free read access (Phase 5A bundles SQLite via `rusqlite` features).

Anatomy:

```yaml
source:
  driver: sqlite
  path: ./hubspot_export.sqlite      # required; relative to the recipe file
  query: |
    SELECT
      month,
      channel,
      market,
      spend,
      cpc
    FROM monthly_metrics
    WHERE quarter = 'Q3' AND scenario = 'Baseline'
```

No credentials needed — SQLite is file-based.

### DuckDB — `driver: duckdb`

Use when:
- The source is a `.duckdb` file (DuckDB's native columnar format).
- You want analytical performance for read-heavy recipes (DuckDB's columnar engine outpaces SQLite for aggregations).

Same shape as SQLite — `path:` + `query:`, no credentials:

```yaml
source:
  driver: duckdb
  path: ./acme.duckdb
  query: |
    SELECT period AS month, ch AS channel, mkt AS market,
           usd_spend AS spend, cost_per_click AS cpc
    FROM v_acme_baseline_working
```

### Postgres — `driver: postgres`

Use when:
- The source is a remote Postgres instance.
- You have a connection string (DSN) available via an environment variable.

Postgres recipes do **not** use `path:`. The connection comes entirely from `credentials.dsn`:

```yaml
source:
  driver: postgres
  query: |
    SELECT month, channel, market, spend, cpc
    FROM analytics.acme_monthly
    WHERE plan = 'Baseline'

credentials:
  dsn: "${env.PG_DSN}"   # e.g., postgres://user:pass@host:5432/db
```

The `${env.PG_DSN}` syntax is interpolated at runtime by Stream D. If `PG_DSN` is unset, **MC5013** fires at runtime (not at validation time — the recipe layer doesn't read environment variables).

### DuckDB-attached-Postgres — `driver: duckdb_postgres`

Use when:
- You want DuckDB's analytical engine to query a remote Postgres instance (DuckDB's `postgres_scan` extension).
- You need DuckDB-mediated cross-database joins (e.g., joining a local DuckDB table with a remote Postgres view).

Recipe shape:

```yaml
source:
  driver: duckdb_postgres
  path: ./local.duckdb            # DuckDB engine — local file
  query: |
    SELECT month, channel, market, spend, cpc
    FROM postgres_db.analytics.acme_monthly
    WHERE plan = 'Aggressive'

credentials:
  dsn: "${env.PG_DSN}"             # the attached Postgres instance
```

The `path:` field still names the DuckDB file; the `query:` runs through the DuckDB engine and references the attached Postgres schema.

---

## Credential handling — `${env.VAR}` interpolation

Phase 5A supports **only** `${env.VAR}` style references in the `credentials:` block. The `${secret.ref}` resolver (vault-backed) is deferred to Phase 5E (Grout).

```yaml
credentials:
  dsn: "${env.PG_DSN}"
  # Multiple credential keys are allowed; each is independently resolved:
  api_token: "${env.HUBSPOT_TOKEN}"
```

Rules:

1. The interpolation is **runtime**. `mc-recipe`'s validator does not read the environment; it just records the templated value verbatim.
2. The variable must be set in the environment Stream D runs in. An unset variable fires **MC5013** at runtime.
3. Never inline a literal credential. `dsn: "postgres://user:pass@host/db"` works but commits secrets to source control — always use the env interpolation.
4. The credential keys (`dsn`, `api_token`, `authorization`, etc.) are driver-conventional, not validated by `mc-recipe`. Postgres looks for `dsn`; HTTP/JSON looks for `authorization`. Use the conventional name.

---

## SQL best practices for recipe queries

These aren't recipe-validator rules; they're patterns that produce queries the LLM and downstream operator can reason about.

### 1. Project columns whose names match the cube target — alias when they don't

The recipe's `columns[i].source:` must match the result-set column name. When the table's column names diverge from the cube's, **alias in the query**:

```yaml
# Source table: campaign_metrics(period, ch, mkt, usd_spend, cost_per_click)
# Cube wants:   month, channel, market, spend, cpc
source:
  driver: sqlite
  path: ./data.sqlite
  query: |
    SELECT
      period       AS month,
      ch           AS channel,
      mkt          AS market,
      usd_spend    AS spend,
      cost_per_click AS cpc
    FROM campaign_metrics
columns:
  - { source: month,   dimension: Time }
  - { source: channel, dimension: Channel }
  - { source: market,  dimension: Market }
  - { source: spend,   measure: Spend, type: f64 }
  - { source: cpc,     measure: CPC,   type: f64 }
```

### 2. Scope rows with WHERE — let SQL filter, not the recipe

Push every filter you can into the query. There is no `WHERE`-equivalent at the recipe layer; an unscoped query that returns 50M rows will run all 50M rows through Stream D.

```yaml
# Right:
query: |
  SELECT month, channel, market, spend, cpc
  FROM monthly_metrics
  WHERE scenario = 'Baseline'
    AND version  = 'Working'
    AND month BETWEEN '2026-01' AND '2026-03'
```

Common scoping patterns:
- Filter by scenario / version / time so the rows match what the recipe's `defaults:` declare.
- Restrict to a date range that matches the period you're importing.
- Exclude rows with `NULL` in mandatory dim columns (`WHERE channel IS NOT NULL AND market IS NOT NULL`) — these would otherwise hit `on_error` at write time.

### 3. Project leaf-level coordinates only (don't aggregate in SQL)

The cube does its own consolidation. Importing aggregate rollups (`Q1_2026` instead of `Jan_2026 + Feb_2026 + Mar_2026`) writes consolidated coordinates, which fires **MC2020** when the cube re-validates. Always project leaf rows.

```sql
-- WRONG — pre-aggregated to quarters:
SELECT 'Q1_2026' AS month, channel, market, SUM(spend) AS spend, AVG(cpc) AS cpc
FROM raw_events
GROUP BY channel, market

-- RIGHT — leaf-month rows; let the cube consolidate:
SELECT month, channel, market, spend, cpc
FROM monthly_metrics
WHERE month IN ('Jan_2026','Feb_2026','Mar_2026')
```

### 4. Match cube element names exactly in dimension columns

The recipe's dimension columns project values that must match declared element names in the model. Acme uses `Title_Case_With_Underscores` (`Paid_Search`, `Jan_2026`, `New_York_City`). Your query must produce those exact strings — case, spaces, underscores all matter.

```sql
-- If the source table stores 'paid search' (lowercase, space):
SELECT
  REPLACE(INITCAP(channel), ' ', '_') AS channel,  -- 'paid search' -> 'Paid_Search'
  ...
FROM monthly_metrics
```

### 5. NULL handling — exclude or coalesce explicitly

Stream D's row transformer rejects rows where a dimension column is NULL (no coordinate to write). Decide upstream: filter them out, or coalesce to a sentinel element you've added to the model.

```sql
-- Filter out rows with missing dims:
WHERE channel IS NOT NULL AND market IS NOT NULL

-- OR coalesce to an "Unknown" element (must exist in the model):
SELECT
  COALESCE(channel, 'Unknown_Channel') AS channel,
  ...
```

Don't rely on `on_error: skip_row` to silently drop NULL-dim rows — be explicit about what's happening.

### 6. Type the projected columns to match the cube

For numeric measures, ensure SQL projects them as numeric (not text):

```sql
-- SQLite is loose-typed; if spend is stored as TEXT:
SELECT
  ...,
  CAST(spend AS REAL) AS spend,
  CAST(cpc   AS REAL) AS cpc
FROM ...
```

A textual `spend` column won't match `type: f64` and fires a runtime conversion error.

---

## `table:` mode (when to use it)

`table:` is a shortcut for `SELECT * FROM <table>`. Reach for it only when:

1. The table has exactly the columns you want — no extras to skip, no renames needed.
2. The dimension column values already match the cube's element names.
3. The row-set is naturally scoped (the table is already filtered to "the relevant rows").

In practice, `table:` is rare — most real-world tables have at least one column you want to skip or alias. When in doubt, write a `query:`.

```yaml
# OK if `monthly_baseline_metrics` is exactly the columns you want:
source:
  driver: sqlite
  path: ./acme.sqlite
  table: monthly_baseline_metrics
```

---

## Common SQL-recipe pitfalls

### "Both `query:` and `table:` set" → MC5003

Pick one. The most common cause is editing a `table:` recipe to add filtering: switch fully to `query:` and remove `table:`.

### "MC5005 — measure not declared" after a query

The `query:` projected a column whose name doesn't match the recipe's `columns[i].measure:`. Common cause: the SQL `AS` alias and the `source:` field drifted out of sync. Audit the SELECT list against the `columns:` array.

### "MC5018 fires on `Clicks`"

Acme's `Clicks` is a Derived measure (`Clicks = Spend / CPC`); it's not writeable. If the source has a `clicks` column, **don't map it** — set `skip: true` or omit it from the projection. Map the inputs (`Spend` + `CPC`) instead.

### "Postgres recipe fails to connect"

`mc-recipe` validation passes; `mc tessera apply` fails with **MC5015** (connection failure). Walk through:

1. Is `${env.PG_DSN}` set in the runtime env?
2. Is the DSN well-formed (`postgres://user:pass@host:port/dbname`)?
3. Is the host reachable (firewall, VPN, etc.)?
4. Are the user's permissions sufficient for the SELECT?

`mc-recipe` only validates the recipe's structure; connectivity is Stream D's responsibility.

### "Element-not-found at runtime"

The query produces rows whose dimension values aren't declared in the model. Either filter them out in SQL, or extend the model to add the missing elements. Phase 5A's `on_missing_element: error` is the only supported policy — auto-create is Phase 5C.

---

## Worked examples

### SQLite — minimal Spend + CPC

```yaml
version: 1
name: hubspot_q3_actuals
description: "Q3 HubSpot Spend + CPC import via SQLite query."
model: ../models/acme.yaml

source:
  driver: sqlite
  path: ./hubspot_q3.sqlite
  query: |
    SELECT month, channel, market, spend, cpc
    FROM campaign_metrics
    WHERE quarter = 'Q3'
      AND scenario = 'Baseline'

columns:
  - { source: month,   dimension: Time }
  - { source: channel, dimension: Channel }
  - { source: market,  dimension: Market }
  - { source: spend,   measure: Spend, type: f64 }
  - { source: cpc,     measure: CPC,   type: f64 }

defaults:
  Scenario: Baseline
  Version: Working

write_disposition: replace
incremental: false
batch: { size: 50000 }
on_error: abort
on_missing_element: error
credentials: {}
```

### Postgres — all 6 Inputs, env-supplied DSN, scenario-scoped

```yaml
version: 1
name: acme_postgres_aggressive
description: "Postgres import — Aggressive scenario, all 6 Acme inputs."
model: ../models/acme.yaml

source:
  driver: postgres
  query: |
    SELECT month, channel, market,
           spend, cpc, cvr, close_rate, aov, cogs_rate
    FROM analytics.acme_monthly
    WHERE plan = 'Aggressive'

columns:
  - { source: month,      dimension: Time }
  - { source: channel,    dimension: Channel }
  - { source: market,     dimension: Market }
  - { source: spend,      measure: Spend,      type: f64 }
  - { source: cpc,        measure: CPC,        type: f64 }
  - { source: cvr,        measure: CVR,        type: f64 }
  - { source: close_rate, measure: Close_Rate, type: f64 }
  - { source: aov,        measure: AOV,        type: f64 }
  - { source: cogs_rate,  measure: COGS_Rate,  type: f64 }

defaults:
  Scenario: Aggressive
  Version: Working

write_disposition: replace
incremental: false
batch: { size: 50000 }
on_error: abort
on_missing_element: error
credentials:
  dsn: "${env.PG_DSN}"
```

### DuckDB-attached-Postgres — federated query

```yaml
version: 1
name: acme_federated
description: "DuckDB engine attached to remote Postgres for cross-DB joins."
model: ../models/acme.yaml

source:
  driver: duckdb_postgres
  path: ./local.duckdb
  query: |
    SELECT
      m.month,
      m.channel,
      m.market,
      m.spend,
      m.cpc
    FROM postgres_db.analytics.acme_monthly AS m
    WHERE m.plan = 'Baseline'

columns:
  - { source: month,   dimension: Time }
  - { source: channel, dimension: Channel }
  - { source: market,  dimension: Market }
  - { source: spend,   measure: Spend, type: f64 }
  - { source: cpc,     measure: CPC,   type: f64 }

defaults:
  Scenario: Baseline
  Version: Working

write_disposition: replace
incremental: false
batch: { size: 50000 }
on_error: abort
on_missing_element: error
credentials:
  dsn: "${env.PG_DSN}"
```

---

## Cross-references

- General recipe schema + the 18 MC5xxx codes: [`../recipe-format/SKILL.md`](../recipe-format/SKILL.md).
- CSV driver: [`../csv-mapping/SKILL.md`](../csv-mapping/SKILL.md).
- HTTP/JSON driver: [`../api-mapping/SKILL.md`](../api-mapping/SKILL.md).
- Acme reference model: [`../../domain-schemas/marketing-mix/SKILL.md`](../../domain-schemas/marketing-mix/SKILL.md).
- Worked SQL examples: `crates/mc-recipe/examples/recipes/acme-{sqlite,duckdb,postgres}-import.recipe.yaml`.
