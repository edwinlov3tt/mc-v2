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

## Files

- `crates/mc-demo-server/` — Rust server (axum + mc-core)
- `demo/frontend/` — React frontend (Vite + Tailwind)
- `demo/registry/performance_tables.csv` — tactic registry
- `demo/sample-data/` — sample CSVs from Scotts RV / Targeted Display
