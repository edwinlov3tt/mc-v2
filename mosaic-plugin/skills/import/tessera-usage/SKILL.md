---
name: mosaic-tessera-usage
description: How to use the `mc tessera` CLI verbs to apply, dry-run, roll back, and audit Tessera imports against a Mosaic cube. Use when running Tessera from the command line, when explaining the `.tessera/` sidecar state model, or when debugging a runtime MC5013/MC5014/MC5015 diagnostic. For recipe authoring (the YAML format itself) see `skills/import/recipe-format/SKILL.md`.
---

# Using Tessera from the CLI

Tessera is Mosaic's data-ingestion engine — the modern replacement for TM1's TurboIntegrator. Once a recipe is written (see `recipe-format` skill), you run it with the `mc tessera *` verbs.

Five verbs, one shared `--format text|json` flag:

| Verb | What it does |
|---|---|
| `mc tessera dry-run <recipe.yaml>` | Validate recipe + plan; **does not write**. Surfaces MC5xxx diagnostics. |
| `mc tessera apply <recipe.yaml>` | Run the full pipeline: connect → fetch → transform → bulk-write → audit. |
| `mc tessera history <model_dir>` | List import audit records in chronological order. |
| `mc tessera rollback <import_id> --model-dir <path>` | Mark an import inactive; the cube reverts to pre-import state on next load. |
| `mc tessera audit <model_dir>` | Same data as `history`, fuller per-record formatting. |

JSON output uses the Phase 3B envelope shape (`schema_version: "1.0"`) for diagnostics and a structured object for reports.

---

## Typical workflow

```bash
# 1. Dry-run first — catch recipe errors without touching the cube.
mc tessera dry-run hubspot-q3.recipe.yaml

# 2. If clean, apply.
mc tessera apply hubspot-q3.recipe.yaml

# 3. Inspect what just happened.
mc tessera history ./models

# 4. If the import was wrong, roll it back.
mc tessera rollback imp_hubspot_q3_1738629000_123456 --model-dir ./models
```

Every verb is **non-interactive**: no prompts, no progress bars without `--format`. Output is plain text (default) or JSON for tooling integration.

---

## What `apply` actually does

When you run `mc tessera apply <recipe.yaml>`, the orchestrator:

1. **Parses** the recipe via `mc_recipe::parse`. MC5001/MC5002/MC5007/MC5012 fire here.
2. **Resolves the model path** relative to the recipe directory.
3. **Loads + validates** the model via `mc_model::load`.
4. **Validates the recipe** against the model. MC5003-MC5011, MC5016-MC5018 fire here.
5. **Resolves credentials** by interpolating `${env.NAME}` references. **MC5013** fires when an env var isn't set.
6. **Constructs the source driver** (CSV / SQLite / DuckDB / Postgres / DuckDB-Postgres / HTTP-JSON). **MC5014** fires when a source file is missing; **MC5015** fires when a remote connection fails.
7. **Resolves column mappings** to concrete `(DimensionId, ElementId)` and measure-target IDs.
8. **Fetches** rows from the driver in batches of `recipe.batch.size` (default 50,000).
9. **Transforms** each row into N cell writes (one per mapped measure column), looking up dimension-element values via `ModelRefs::element`.
10. **Stages** the cells in a `mc_core::WriteBatch`.
11. **Commits** atomically. The kernel runs validate → snapshot → apply in three phases. Validation failures roll back with no mutation.
12. **Persists** the imported cells to `<model_dir>/.tessera/imports/<import_id>.cells.jsonl`.
13. **Captures** the pre-commit snapshot identifier (no full snapshot file written for now — Phase 5A simplification).
14. **Marks active** in `<model_dir>/.tessera/active-imports.json`.
15. **Appends** an audit record to `<model_dir>/.tessera/audit.jsonl`.
16. **Returns** an `ImportReport` with `import_id`, `rows_written`, `rows_failed`, `timing` (fetch/transform/validate/commit/total ms), `snapshot_id`, and `audit_path`.

---

## The `.tessera/` sidecar state model

```
<model_dir>/.tessera/
├── audit.jsonl                  one JSON record per import (append-only)
├── imports/
│   └── <import_id>.cells.jsonl  cells written by this import
├── snapshots/
│   └── <snapshot_id>.cells.jsonl pre-commit snapshot (Phase 5A: empty placeholder)
├── quarantine/
│   └── <import_id>.jsonl        rows that failed under `on_error: quarantine`
└── active-imports.json          manifest of currently-active imports
```

The directory is **ephemeral** — never commit it to source control. Add `.tessera/` to your model directory's `.gitignore`. Re-running `mc tessera apply` against a fresh `.tessera/` rebuilds it from scratch.

---

## Error-handling policies (`on_error` field)

Per ADR-0010 amendment #9:

| Policy | Behavior |
|---|---|
| `abort` (default) | Transactional. The first row error stops the import; the WriteBatch is dropped (no commit, no mutation). |
| `skip_row` | Failed rows count toward `rows_failed`; the import continues with the rest. |
| `quarantine` | Failed rows are written to `<model_dir>/.tessera/quarantine/<import_id>.jsonl` with the original row data + the diagnostic. |

`quarantine` rows are **not** auto-reprocessed. A future `mc tessera retry-quarantine` verb lands in Phase 5C.

---

## Rollback semantics

`mc tessera rollback <import_id> --model-dir <path>` does **two** things:

1. Removes `<import_id>` from `active-imports.json`.
2. Appends a synthetic `event: "rollback"` record to `audit.jsonl`.

It does **not** delete the import's `cells.jsonl` (that's audit history). The next time the cube is reconstructed (via `Tessera::load_active` or `mc tessera apply` of another recipe), the rolled-back import is skipped — its cells aren't replayed, so the cube state matches "as if that import never ran."

This means:

- **Multiple imports** + a single rollback is idempotent — only the named import is skipped.
- **Rollback before any other apply** returns the cube to its empty (model-default) state.
- **Rollback of an unknown `import_id`** errors with `ImportNotFound`.

---

## Worked example: import then roll back

```bash
# Apply: write 100 spend cells.
$ mc tessera apply spend-q3.recipe.yaml
Tessera apply: spend_q3
  import_id      : imp_spend_q3_1738629000_123456
  rows_written   : 100
  ...

# Inspect history.
$ mc tessera history ./acme
import_id                              event   timestamp             rows  failed
imp_spend_q3_1738629000_123456         apply   2026-05-04T12:00:00Z  100   0

# Realize the recipe pulled the wrong quarter; roll back.
$ mc tessera rollback imp_spend_q3_1738629000_123456 --model-dir ./acme
rolled back import imp_spend_q3_1738629000_123456

# History now shows two records: the original apply + the rollback.
$ mc tessera history ./acme
import_id                              event     timestamp             rows  failed
imp_spend_q3_1738629000_123456         apply     2026-05-04T12:00:00Z  100   0
imp_spend_q3_1738629000_123456         rollback  2026-05-04T12:01:00Z  0     0

# active-imports.json is now empty; replays via load_active will skip
# the rolled-back import.
```

---

## JSON output (`--format json`)

Every verb accepts `--format json` for tooling integration. The output shape:

- **`apply` / `dry-run`**: a serialized `ImportReport` / `DryRunReport` object.
- **`history` / `audit`**: an array of `AuditRecord` objects.
- **`rollback`**: `{"rolled_back": "<import_id>", "model_dir": "<path>"}`.
- **Errors**: the Phase 3B envelope `{"schema_version": "1.0", "diagnostics": [{"code": "MC5xxx", ...}]}` is written to **stdout** (not stderr) and the exit code is non-zero.

---

## Common diagnostic codes

| Code | Stage | What it means |
|---|---|---|
| MC5001 | parse | Malformed recipe YAML |
| MC5004 | validate | Column references unknown dimension |
| MC5005 | validate | Column references unknown measure |
| MC5011 | validate | Column has no single target (no `dimension`/`measure`) or both |
| MC5013 | runtime | `${env.X}` reference where `X` is unset (MC5xxx is fired by Tessera, not mc-recipe) |
| MC5014 | runtime | Source file not found (CSV / SQLite / DuckDB / DuckDB file backing duckdb_postgres) |
| MC5015 | runtime | Connection failure (Postgres DSN, HTTP endpoint) |
| MC5016 | validate | Dimension appears in both `columns:` and `defaults:` |
| MC5017 | validate | `model:` path escapes workspace root |
| MC5018 | validate | Column targets a Derived measure (Phase 5A writes Input only) |

Run `mc tessera dry-run` against a recipe FIRST to catch MC5001-MC5012 + MC5016-MC5018. The runtime codes (MC5013-MC5015) only fire on `apply`.

---

## What Phase 5A does NOT support yet

- **Long-format CSVs** (`Measure` as a dim column + a `value` column). Wide format only — every measure column is its own `ColumnMapping`. *Long-format support is queued as Phase 5A.1.*
- **`write_disposition: append` / `merge`**. Phase 5C.
- **Element auto-creation** (`on_missing_element: create`). Phase 5C.
- **Incremental loads** with watermarks. Phase 5C.
- **MySQL / D1 / Snowflake / BigQuery drivers**. Phase 5C.
- **`${secret.ref}` interpolation** (vault-backed credentials). Phase 5E (Grout).

Phase 5A ships exactly the surface above. Future expansions land in named sub-phases.
