# Mosaic Marketing Report Demo

Phase 6D vertical-slice demo MVP. Upload a zip of marketing CSVs,
get an instant narrative report. No LLM, no hallucination, sub-200ms.

## Quick start

```bash
# From the repo root:
cargo run --release --bin mc -- start --static demo/frontend/dist
```

This prints the Mosaic banner, starts the server on `http://localhost:8080`,
and opens your browser. Upload the sample zip to test.

## Sample data

A pre-built test zip lives at `demo/sample-data/`. To recreate it:

```bash
cd demo/sample-data && zip scotts-rv-targeted-display.zip *.csv
```

Upload this zip through the browser UI.

## What happens

1. **Tactic detection** — each CSV filename is matched against a 292-entry
   registry (`demo/registry/performance_tables.csv`). Product, sub-product,
   table type, and expected headers are identified.

2. **Cube ingestion** — matched CSVs are ingested into Mosaic cubes via
   `CubeBuilder` directly (no YAML round-trip). Numeric columns become
   measures; the first column becomes a category dimension.

3. **Narrative evaluation** — pre-compiled templates fire against populated
   cubes: MoM trends, device rankings, creative rankings, geo concentration,
   zero-conversion alarms.

4. **Workspace routing** — CSVs are grouped by tactic. Each tactic gets its
   own cube set and narrative bundle. A cross-tactic summary is generated.

The terminal shows a timing breakdown for every upload:

```
2026-05-07 15:35:00  POST /api/upload  Scotts RV  (11 CSVs, 1 tactics)
  Registry match     0.5ms
  Cube compile       0.8ms
  Cube populate      0.0ms
  Narrative eval     0.0ms
  Serialize          0.0ms
  ─────────────────────────
  Done               1.3ms
```

## Frontend development

The frontend is a Vite + React + Tailwind project in `demo/frontend/`.

```bash
cd demo/frontend
npm install
npm run dev    # Dev server on :5173 (proxies /api to :8080)
```

To rebuild the static files served by `mc start`:

```bash
cd demo/frontend && npm run build
```

## Demo script

For the leadership demo:

1. Open terminal. Run `cargo run --release --bin mc -- start --static demo/frontend/dist`
2. Banner appears. Browser opens to the upload page.
3. Upload `demo/sample-data/scotts-rv-targeted-display.zip`
4. Report appears instantly. Point out:
   - **Summary section** — "1 tactic processed, 55K impressions, 0.44% CTR"
   - **Alerts** — "Zero conversions recorded. Recommend verifying pixel."
   - **Time trends** — "Impressions grew 22%", "Clicks more than doubled"
   - **Device ranking** — "Tablet was top at 0.83% CTR, nearly 2x average"
   - **Geo concentration** — "Illinois accounts for 81% of impressions"
5. Click **Show Payload** — raw JSON with evidence objects, template IDs, timing
6. Click **Copy as Markdown** — paste into a doc or email
7. Look at the terminal — "Done 1.3ms"
8. "This is deterministic. No LLM. 912 tests. 292-tactic registry. Sub-2ms."

## Architecture

```
mc start
  -> prints banner (ANSI colors)
  -> starts axum server on localhost:8080
  -> opens browser
  -> serves React frontend (static files from demo/frontend/dist/)

POST /api/upload (multipart zip)
  -> extract zip in-memory (no temp files)
  -> match filenames against 292-entry registry
  -> build Mosaic cubes via CubeBuilder (skip YAML)
  -> evaluate narrative templates
  -> group by tactic, build cross-tactic summary
  -> return JSON with timing data
```

## Interpretation Ledger (Phase 7A.2)

Every upload automatically writes structured narrative entries to
`.mosaic/analysis-ledger.jsonl` (append-only JSONL). This creates a
durable history for cross-period analysis and benchmark aggregation.

### CLI usage

```bash
# Generate narratives + save to ledger
mc model narrate model.yaml --save-ledger

# Query the ledger
mc model query-ledger . --severity warning
mc model query-ledger . --repeated 3 --since 2026-01
mc model query-ledger . --template clicks_down --format json

# Export for analysis
mc model ledger-export . --format csv > analysis.csv
mc model ledger-export . --format jsonl --since 2026-03
```

### Privacy boundary

Ledger entries contain whatever dimension names and measure values your
model declares. If your model uses real advertiser names as dimension
elements, those appear in the ledger. Configure dimension elements
accordingly if ledger privacy is a concern.

- No automatic PII detection in v1 (Phase 7A.4 scope).
- Benchmark contribution is opt-in only (planning doc Q8).
- Entries are immutable once written — the query layer handles schema
  version differences across model changes.

## Files

- `crates/mc-demo-server/` — Rust server (axum + mc-core)
- `crates/mc-narrative/src/ledger.rs` — Ledger schema, write/read paths, query filters
- `demo/frontend/` — React frontend (Vite + Tailwind)
- `demo/registry/performance_tables.csv` — tactic registry
- `demo/sample-data/` — sample CSVs from Scotts RV / Targeted Display
