---
name: mosaic-recipe-format
description: How to author a Mosaic Tessera recipe — the declarative YAML contract that imports external data into a cube. Use when writing or editing a `*.recipe.yaml` file, when answering "how do I import data into a Mosaic cube," when debugging an MC5xxx diagnostic, or when scaffolding a new recipe for any of the six supported drivers (CSV, SQLite, DuckDB, Postgres, DuckDB-attached-Postgres, HTTP/JSON). For the model side (the YAML cube being imported into) see `skills/authoring/SKILL.md`.
---

# Authoring a Mosaic Tessera Recipe

A Mosaic *recipe* is a single YAML file that tells Tessera (the Mosaic data-ingestion engine) how to bring external data into a cube. A recipe answers four questions:

1. **Where does the data come from?** (`source:` block + `driver:`)
2. **How do source columns map onto cube dimensions and measures?** (`columns:` array + `defaults:`)
3. **What if rows fail?** (`on_error:` — `abort` / `skip_row` / `quarantine`)
4. **How does this batch fit with what's already in the cube?** (`write_disposition:`, `incremental:`)

A recipe has *no executable code*. It is parsed, validated, and (later, by Stream D) executed against a target Mosaic model. If the recipe is invalid the validator emits one or more **MC5xxx** diagnostics in a JSON envelope identical in shape to the model layer's MC1xxx-MC3xxx diagnostics.

## When to write a recipe

Write a recipe when:

- A user wants to load external data (CSV, SQL, REST) into a Mosaic cube.
- The data shape is *roughly* tabular and maps to leaf-level cube coordinates.
- The user has (or you can construct) a Mosaic model YAML file that names the dimensions and measures the data feeds.

Do NOT write a recipe to:

- Compute derived metrics (Clicks, Leads, Revenue, Gross_Profit in the Acme model). Those are Derived measures and are computed by rules. Recipes write to **Input measures only**.
- Replicate a full slice (e.g., "wipe the entire `Aggressive` scenario and replace it"). Phase 5A `replace` is **coordinate-level**: it overwrites only the cells the recipe produces.
- Scrape or transform documents. That's Phase 5D scope.

---

## The full recipe schema

Every field's purpose, type, and required-ness:

```yaml
version: 1                  # u32 — must be 1 in Phase 5A (otherwise MC5012)
name: my_import             # String — free-form recipe name
description: "..."          # Option<String> — optional prose
model: ../models/cube.yaml  # String — path to the target Mosaic YAML model,
                            # resolved relative to THIS recipe file's directory.
                            # Path-escapes outside the workspace root → MC5017.

source:                     # SourceConfig — where the data lives.
  driver: csv               # DriverKind — one of: csv | sqlite | duckdb |
                            # postgres | duckdb_postgres | http_json
                            # Anything else → MC5002.
  path: ./data.csv          # Option<String> — file path (CSV/SQLite/DuckDB)
  query: "SELECT ..."       # Option<String> — SQL query (SQLite/DuckDB/Postgres)
  table: campaigns          # Option<String> — bare table name; mutually
                            # exclusive with `query` → MC5003.
  url: "https://..."        # Option<String> — for `http_json`
  json_path: "$.data[*]"    # Option<String> — JSONPath into the response body

columns:                    # Vec<ColumnMapping> — REQUIRED. Each entry maps
                            # one source column to one cube target.
  - source: month           # String — name in the source schema
    dimension: Time         # Option<String> — XOR with `measure`. Both set →
                            #   MC5011 (ambiguous). Neither + skip != true →
                            #   MC5011 (no-target).
    measure: Spend          # Option<String> — must reference an Input measure.
                            #   Unknown name → MC5005. Derived measure → MC5018.
    type: f64               # Option<String> — if set, must be compatible with
                            #   target measure's data_type → MC5006.
    scale: 0.001            # Option<f64> — runtime multiplier (Stream D)
    format: "%Y-%m"         # Option<String> — driver-specific format hint
    skip: true              # Option<bool> — when true, ignore this column

defaults:                   # HashMap<String, String> — dim_name → element_name
  Scenario: Baseline        # Each key must be a real dimension → MC5008.
  Version: Working          # Each value must be a real element → MC5009.
                            # A dimension cannot appear in BOTH `columns:`
                            #   and `defaults:` → MC5016.

write_disposition: replace  # WriteDisposition — Phase 5A: only `replace`
                            # (coordinate-level overwrite — does NOT clear
                            # cells absent from the incoming data).

incremental: false          # bool — Phase 5A: must be false. Watermark
                            # config (true) deferred to Phase 5C.

batch:
  size: 50000               # Option<usize> — runtime batch size; default 50K

on_error: abort             # OnError — `abort` (default) | `skip_row` |
                            # `quarantine`. See "on_error semantics" below.

on_missing_element: error   # OnMissingElement — Phase 5A: only `error`
                            # (auto-create deferred to Phase 5C).

credentials:                # HashMap<String, String> — env-only in Phase 5A
  dsn: "${env.PG_DSN}"      # `${env.X}` interpolation. Unset env var → MC5013
                            # at runtime (Stream D fires; not the validator).
```

---

## The 6 binding semantic rules

These are enforced by `mc-recipe`'s validator. Internalize them — they are the structural laws of recipe authoring.

### Rule 1 — Column mappings are 1:1

A `columns[i]` entry maps **exactly one** source column to **exactly one** cube target — either a dimension OR a measure. Both fields set is ambiguous; neither field set with `skip != true` is a silent drop. Both shapes fire **MC5011**.

```yaml
# WRONG — both dimension and measure set (1:1 violation):
- source: x
  dimension: Time
  measure: Spend       # → MC5011 (ambiguous target)

# WRONG — column goes nowhere:
- source: orphan       # → MC5011 (no target)

# RIGHT — explicit skip:
- source: campaign_id
  skip: true           # OK — ignored
```

### Rule 2 — Defaults vs. columns are mutually exclusive

A dimension is either varying-per-row (declared in `columns:` via `dimension: X`) OR constant (declared in `defaults:` as a key). Never both. If both, **MC5016** fires.

```yaml
# WRONG:
columns:
  - { source: scen, dimension: Scenario }
defaults:
  Scenario: Baseline    # → MC5016 (mutual exclusion)

# RIGHT (column varies per row):
columns:
  - { source: scen, dimension: Scenario }

# RIGHT (constant):
defaults:
  Scenario: Baseline
```

### Rule 3 — Input measures only

Recipes write to measures with `role: "Input"` in the model. Targeting a Derived measure fires **MC5018**. Derived measures are computed by rules; the kernel rejects writes to them at write time anyway. Catching it at recipe-validation time gives a friendlier error.

```yaml
# WRONG (Acme's Clicks = Spend / CPC is Derived):
- source: clicks
  measure: Clicks       # → MC5018

# RIGHT — feed the inputs that drive Clicks:
- source: spend
  measure: Spend
- source: cpc
  measure: CPC
```

### Rule 4 — `write_disposition: replace` is coordinate-level

`replace` overwrites ONLY the coordinates the current recipe produces. It does NOT clear pre-existing cells in the target slice that aren't present in the incoming data. An import with missing rows does **not** wipe data. (Full-slice replace is deferred to Phase 5C.)

### Rule 5 — `model:` path resolution

The `model:` field is resolved relative to the **recipe file's directory**. Path-escapes outside the workspace root fire **MC5017**.

```yaml
# Recipe at /workspace/imports/q3.recipe.yaml + workspace_root /workspace:
model: ../models/marketing.yaml   # → /workspace/models/marketing.yaml ✓
model: ../../etc/passwd           # → /etc/passwd → MC5017 (escapes /workspace)
```

The path-escape check is **lexical** (no filesystem touches). It runs only when the caller supplies a workspace root + recipe directory; in-memory recipes (no file context) skip the check.

### Rule 6 — `on_error` semantics

`on_error:` controls what happens when a single row fails to materialize. Three accepted values (anything else → MC5001 at parse time):

| Value | Behavior |
|---|---|
| `abort` (default) | Transactional. Any row error fails the entire import. No partial commit. |
| `skip_row` | Skip the failing row; remaining rows proceed; row appears in audit log as `status: "skipped"`. |
| `quarantine` | Write the failing row + diagnostic to a per-import quarantine file; remaining rows proceed. |

`mc-recipe` only validates the value is one of the three — behavioral enforcement lives in Stream D (`mc-tessera`).

---

## All 18 MC5xxx diagnostic codes

Codes MC5001-MC5018 are stable forever (CVE-style retirement; ADR-0010 + ADR-0005 amendment #11). Each fires under exactly the conditions below.

### Parse-stage codes

| Code | Fires when | Example fix |
|---|---|---|
| **MC5001** | YAML / deserialization failure (malformed YAML, type mismatch, unrecognized enum variant other than driver). | Validate YAML syntax; re-check field types. |
| **MC5002** | `source.driver:` is not one of `csv` / `sqlite` / `duckdb` / `postgres` / `duckdb_postgres` / `http_json`. | Pick a supported driver. |
| **MC5007** | A required field is missing (`version`, `name`, `model`, `source`, `source.driver`, `columns`). | Add the missing field. |
| **MC5012** | `version:` is not `1`. Phase 5A pins recipes at version 1. | Change to `version: 1`. |

### Validate-stage codes (require a loaded `ValidatedModel`)

| Code | Fires when | Example fix |
|---|---|---|
| **MC5003** | `source.table:` and `source.query:` both set. | Pick one. |
| **MC5004** | A column's `dimension:` references a name not in the model. | Use a real dim name; check spelling. |
| **MC5005** | A column's `measure:` references a name not in the model. | Use a real measure name; check spelling. |
| **MC5006** | A column's `type:` is incompatible with the target measure's `data_type` (case-insensitive). | Change `type:` to match the measure (e.g., `f64` for an F64 measure). |
| **MC5008** | A `defaults:` key isn't a declared dimension. | Use a real dim name. |
| **MC5009** | A `defaults:` value isn't an element of the named dim. | Use a real element name (often the leaf). |
| **MC5010** | Same `source:` column appears twice in `columns:`. | De-duplicate. |
| **MC5011** | A column has no clear single target (no-target OR ambiguous-target). | Set exactly one of `dimension`/`measure`, or set `skip: true`. |
| **MC5016** | A dimension appears in BOTH `columns:` and `defaults:`. | Decide: varying-per-row (columns) OR constant (defaults). |
| **MC5017** | Resolved `model:` path escapes the workspace root. | Move the model inside the workspace, or correct the path. |
| **MC5018** | A column maps to a Derived measure. | Map to the Inputs that drive the Derived measure (e.g., Spend + CPC instead of Clicks). |

### Runtime-stage codes (fired by Stream D / `mc-tessera`)

| Code | Fires when |
|---|---|
| **MC5013** | A `${env.X}` reference in `credentials:` names an unset environment variable. |
| **MC5014** | The source file (`source.path:`) is not readable (not found, permission denied, IO error). |
| **MC5015** | The source connection failed (Postgres DSN unreachable, HTTP endpoint down, auth rejected). |

`mc-recipe` defines these as variant-level placeholders for namespace uniformity; the validator does not fire them itself (no FS / network / env access in `mc-recipe`).

---

## Diagnostic envelope

Every MC5xxx diagnostic emits in the same JSON shape used by Phase 3B (`schema_version: "1.0"`):

```json
{
  "schema_version": "1.0",
  "diagnostics": [
    {
      "code": "MC5004",
      "severity": "error",
      "path": "/columns/2/dimension",
      "message": "column \"market_region\" references unknown dimension \"Region\""
    }
  ]
}
```

- `code` — MC5xxx string.
- `severity` — `"error"` (lowercase; distinct from mc-model's `"Error"`).
- `path` — JSON pointer into the recipe YAML (e.g., `/columns/2/dimension`).
- `message` — human-readable sentence with named values quoted for stability.

Sort order (deterministic):

1. `severity` desc (errors first).
2. `code` asc.
3. `path` asc.
4. `message` asc.

---

## Driver-by-driver examples

### CSV — local file, minimal mapping

```yaml
version: 1
name: q3_actuals
model: ../models/acme.yaml
source:
  driver: csv
  path: ./q3_actuals.csv
columns:
  - { source: month, dimension: Time }
  - { source: channel, dimension: Channel }
  - { source: market, dimension: Market }
  - { source: spend, measure: Spend, type: f64 }
  - { source: cpc, measure: CPC, type: f64 }
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

### SQLite / DuckDB — query-based

```yaml
source:
  driver: sqlite        # or `duckdb`
  path: ./data.sqlite
  query: |
    SELECT period AS month, ch AS channel, mkt AS market,
           spend, cpc, cvr, close_rate, aov, cogs_rate
    FROM monthly_metrics
    WHERE plan = 'Baseline'
```

Use `query:` OR `table:`, never both. `query:` with a multi-line block scalar (`|`) is the canonical form for non-trivial SQL.

### Postgres — DSN via env interpolation

```yaml
source:
  driver: postgres
  query: "SELECT month, channel, market, spend, cpc FROM analytics.acme_q3"
credentials:
  dsn: "${env.PG_DSN}"     # MC5013 fires at runtime if PG_DSN is unset
```

### DuckDB attached to Postgres

```yaml
source:
  driver: duckdb_postgres
  path: ./local.duckdb     # local DuckDB acting as the engine
  query: |
    SELECT month, channel, market, spend, cpc
    FROM postgres_db.analytics.acme_monthly
credentials:
  dsn: "${env.PG_DSN}"
```

### HTTP/JSON — REST endpoint

```yaml
source:
  driver: http_json
  url: "https://api.example.com/v1/marketing/monthly"
  json_path: "$.data.items[*]"
columns:
  - { source: period, dimension: Time }
  - { source: channel, dimension: Channel }
  - { source: market, dimension: Market }
  - { source: spend_usd, measure: Spend, type: f64 }
  - { source: campaign_id, skip: true }    # ignored explicitly
credentials:
  authorization: "Bearer ${env.ACME_API_TOKEN}"
```

---

## Common authoring mistakes (and how to fix them)

1. **"My recipe rejects but the validator says MC5018."** You're trying to write to a Derived measure (Clicks/Leads/Customers/Revenue/Gross_Profit in Acme). Map the inputs that drive it, not the derived value.

2. **"MC5016 keeps firing on `Scenario`."** You have `Scenario` as both a column dimension AND a default. Pick one: vary per row (column) or set constant (default).

3. **"MC5011 fires on a column I want to ignore."** Add `skip: true` explicitly. Empty mappings are not silently dropped.

4. **"MC5006 says my type is incompatible."** The recipe's `type:` is compared case-insensitively against the model measure's `data_type`. `f64` matches `F64`. Most Acme measures are F64.

5. **"My CSV has a `Region` column but the model has `Market`."** That fires MC5004. Either rename the source column upstream, or update the recipe to use `dimension: Market` and accept that the CSV header doesn't match the dim name (the source name is independent — only `dimension:` / `measure:` must match the model).

6. **"MC5009 fires on `Scenario: Actual`."** The Acme model declares Scenario elements `Baseline | Aggressive | Conservative`. `Actual` isn't one of them. Use a declared element or extend the model.

---

## Cross-references

- Model side (the YAML being imported into): `skills/authoring/SKILL.md`.
- Diagnostic-debugging across all MCxxxx codes: `skills/debugging/SKILL.md`.
- Acme reference model: `skills/domain-schemas/marketing-mix/SKILL.md`.
- Phase 5 architecture (this skill's authoritative source): `docs/decisions/0010-phase-5-tessera-architecture.md`.
- Stream B implementation: `crates/mc-recipe/`.
- Worked example recipes: `crates/mc-recipe/examples/recipes/`.
