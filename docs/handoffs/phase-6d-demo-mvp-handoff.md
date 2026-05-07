# Phase 6D Handoff — Marketing Report Demo MVP

> **Audience:** the Claude Code instance (or instances) that build Phase 6D.
> **You inherit `main` at HEAD (912 / 0 / 5 tests post-Phase 3H.2).
> Phase 3 is complete. The formula engine is at the "completion line."
> Phase 6D builds a vertical-slice demo ON TOP of the formula engine.**
>
> **This is a BUILD phase, not a fix phase.** Unlike Phases 6A.1–6A.3
> and 3I–3H.2 (which fixed / extended existing code), Phase 6D creates
> NEW infrastructure: a demo HTTP server, a narrative template engine,
> a React frontend, and the `mc start` CLI command. The work lives in
> a new `demo/` directory (NOT integrated into the Phase-tracked crates
> yet) plus a new `mc-demo-server` crate.
>
> **Timeline:** ~20 focused hours across 5 build sessions. The project
> owner wants this demo-ready within a week of starting. Each session
> ends with something demo-able (progressive delivery, not big-bang).

---

## The demo pitch (what leadership sees)

```
$ mc start

  ███╗   ███╗ ██████╗ ███████╗ █████╗ ██╗ ██████╗
  ████╗ ████║██╔═══██╗██╔════╝██╔══██╗██║██╔════╝
  ██╔████╔██║██║   ██║███████╗███████║██║██║
  ██║╚██╔╝██║██║   ██║╚════██║██╔══██║██║██║
  ██║ ╚═╝ ██║╚██████╔╝███████║██║  ██║██║╚██████╗
  ╚═╝     ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝╚═╝ ╚═════╝

  Large Numbers Model · v0.1.0
  912 tests · Formula engine complete

  Starting server on http://localhost:8080
  Opening browser...
```

User uploads a zip of marketing CSVs. Within 2 seconds:

> **Targeted Display — Scotts RV / Rockford**
>
> Impressions grew 22% from July (25,102) to August (30,655). Clicks more than doubled, increasing 110% from 79 to 166. CTR strengthened from 0.31% to 0.54%.
>
> Tablet was the top-performing device by engagement: 0.83% CTR — nearly double the campaign average. Desktop underperformed at 0.07% CTR.
>
> ⚠️ Zero conversions recorded across the entire campaign. Recommend verifying conversion pixel installation.

User clicks "Show Payload" → sees the raw JSON with evidence objects, template IDs, severity tags. **No LLM. No hallucination. Deterministic.**

---

## Session-by-session build plan

### Session 1 (~3-4 hours): Scaffold + tactic detection

**Goal:** Upload a zip → see detected tactics with matched headers.

**Deliverables:**
1. New `demo/` directory at repo root with:
   - `demo/frontend/` — Vite + React + Tailwind project (lift scaffold from ignite-report-ai; strip campaign-specific pages)
   - `demo/registry/` — copy `performance_tables.csv` + `.json` from ignite-report-ai
   - `demo/sample-data/` — copy the `.demo-data/` CSVs for testing
2. New `crates/mc-demo-server/` crate:
   - `Cargo.toml` with deps: `axum`, `tower-http`, `tokio`, `zip`, `serde_json`, `open`, `crossterm`
   - `src/main.rs` — `mc start` entry point (print banner → start server → open browser). **Pre-warm registry at startup** (Decision 11 optimization #3) — parse `performance_tables.csv` into `HashMap` BEFORE accepting requests.
   - `src/registry.rs` — parse `performance_tables.csv` into a `HashMap<String, TacticSpec>`
   - `src/timing.rs` — `PipelineTimer` struct that wraps `std::time::Instant` per stage; prints the breakdown table to stdout (Decision 11 CLI timing display); serializes into the JSON response's `timing` object.
   - `src/upload.rs` — `POST /api/upload` handler (extract zip **in-memory via `Cursor`** per Decision 11 optimization #1 → match filenames against registry → return detection results). **Every upload prints the timing breakdown to the terminal.**
3. React frontend:
   - Upload page with drag-drop zone
   - Detection results display (table of: filename → product → sub-product → table_type → header match confidence)

**Demo-able at end of session:** Upload zip → see "Detected: Targeted Display Monthly Performance (100% header match), Creative Performance (95% match)..." in the browser.

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| Where does `mc-demo-server` live? | New crate in `crates/mc-demo-server/`. Links against `mc-core`, `mc-model`, `mc-tessera` as path deps. Has its own `[[bin]]` entry (`mosaic-demo` or `mc` with a `start` subcommand). |
| `mc start` as a new binary or a subcommand of existing `mc`? | **New subcommand on existing `mc` binary** (add to `crates/mc-cli/src/main.rs`). The demo server logic lives in `mc-demo-server` as a library; `mc-cli` calls it. This keeps one binary (`mc`) as the user entry point. |
| Frontend build: embedded in Rust binary or served from disk? | **Served from disk** in dev (Vite dev server on :5173, API on :8080, CORS). For the demo, `npm run build` and serve the `dist/` via `tower-http::ServeDir`. |
| `tokio` in the workspace? | **Yes — mc-demo-server adds `tokio` as a runtime dep.** This is a new crate; the existing `mc-core` stays tokio-free per the kernel rules. The demo server is async (axum requires it). This is the first async code in the workspace; it's isolated to the demo server. CLAUDE.md's "no tokio" rule applies to `mc-core` only. |
| Performance tables registry format? | **Parse the CSV at startup** into a `Vec<TacticSpec>`. Each spec has: `product_name`, `subproduct_name`, `table_name`, `file_name` (used for matching), `headers: Vec<String>`, `is_required`, `sort_order`. Filename matching is prefix-based (`report-targeteddisplay-monthly-performance` matches a CSV named `report-targeteddisplay-monthly-performance.csv`). |

---

### Session 2 (~3-4 hours): Cube ingestion pipeline

**Goal:** Detected CSV → populated Mosaic cube with real values.

**Deliverables:**
1. `src/ingest.rs` in `mc-demo-server`:
   - Takes a detected `TacticSpec` + parsed CSV rows
   - Auto-generates a cube YAML in memory (dimensions from header structure; measures from numeric columns)
   - Compiles via `mc_model::parse` → `mc_model::validate` → `mc_model::compile`
   - Populates via the existing `apply_canonical_inputs` path (or raw `Cube::write` for simplicity)
2. `POST /api/upload` now returns populated cube values alongside detection results
3. Frontend: detection page shows "populated X measures across Y coordinates" per tactic

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| How to auto-generate cube YAML from CSV headers? | **Template-driven.** For each tactic family (display-like, video-like, search-like, social-like), ship a YAML template with dimension placeholders. The ingest pipeline fills in the dimensions from the registry headers and the CSV data. This avoids generating arbitrary YAML at runtime. |
| What if a CSV has columns not in the registry? | **Ignore extra columns; warn in the detection display.** Don't fail. |
| What if a CSV is missing required columns? | **Error for that tactic; continue processing other CSVs in the zip.** Report missing columns in the detection display. |
| Measures: all numeric columns become measures? | **Yes for the demo.** Every numeric column (Impressions, Clicks, CTR, Spend, CPM, etc.) becomes a measure. Non-numeric columns (Campaign Name, City, State) become dimension elements. |
| Time dimension from Date column? | **Parse `MM-YYYY` format** (as seen in the sample data: `07-2025`, `08-2025`). Map to element names like `Jul_2025`, `Aug_2025`. Use `mc-tessera`'s time_format if applicable; otherwise hand-parse for the demo. |

---

### Session 3 (~4-5 hours): Narrative template engine

**Goal:** Populated cube → rendered narrative paragraphs.

**Deliverables:**
1. `src/narrative.rs` in `mc-demo-server`:
   - Loads narrative templates from YAML files in `demo/narratives/`
   - For each tactic's populated cube: evaluates `when:` predicates, resolves `bindings:`, substitutes `template:` placeholders, formats numbers
   - Returns `Vec<NarrativeOutput>` per tactic with: `id`, `severity`, `text`, `evidence: HashMap<String, f64>`, `template_id`
2. `demo/narratives/` directory with 4 template families:
   - `display-like.yaml` — CTR comparison, device ranking, geo concentration, conversion alarm
   - `video-like.yaml` — completion rate, reach growth, frequency capping
   - `search-like.yaml` — CPC trend, conversion rate, quality score
   - `social-like.yaml` — engagement ranking, link-click efficiency, awareness reach
3. `POST /api/upload` now returns narratives alongside detection + cube values
4. Frontend: narrative report page with rendered paragraphs per tactic

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| Template evaluation: use the formula engine (mc-core) or hand-roll? | **Use the formula engine** for `when:` predicates and numeric `bindings:`. The demo server already links against mc-core; calling `eval_expr` is the right move. String bindings (direction words like "grew" / "declined") use simple `if` expressions that the formula engine handles. |
| String formatting: Rust-side or JS-side? | **Rust-side.** The API returns fully-rendered text strings. The frontend just displays them. No client-side number formatting. |
| How many templates per family? | **3 per family minimum** (impression/click trend, device/geo ranking, alarm/warning). Total: ~12 templates. Each ~10 lines of YAML. |
| What if a template's `when:` predicate fails (e.g., `prev()` returns Null at first period)? | **Skip silently.** A template that can't fire doesn't produce output. This is correct behavior — "no change worth mentioning" produces no narrative. |
| Evidence objects: what fields? | **Every binding value that went into the template.** If the template uses `{current_impressions}` and `{prev_impressions}`, the evidence includes both. Plus `tactic`, `period`, `severity`, `template_id`. This is the "show payload" data. |
| **Design for extraction to `mc-narrative` crate (BINDING)** | **`narrative.rs` must be self-contained and extraction-ready.** Define `NarrativeTemplate`, `NarrativeOutput`, `NarrativeEvidence` as clean structs with no demo-server-specific types. Keep formula-engine calls behind a function boundary (`pub fn evaluate_templates(templates: &[NarrativeTemplate], cube: &mut Cube, refs: &Refs) -> Vec<NarrativeOutput>`). No axum types, no HTTP types, no frontend types leak into the narrative logic. After the demo ships, Phase 7A.1 extracts `narrative.rs` → `crates/mc-narrative/` as a mechanical refactor (move file + add Cargo.toml + expose public API), NOT a rewrite. Document at the top of `narrative.rs`: `// Designed for extraction to mc-narrative crate in Phase 7A.1. Keep self-contained.` |

---

### Session 4 (~3-4 hours): Workspace routing + multi-tactic

**Goal:** Upload CSVs from 3-4 different tactics → get per-tactic narratives + an overall summary.

**Deliverables:**
1. `src/workspace.rs` in `mc-demo-server`:
   - Routes uploaded CSVs to separate cubes by detected tactic
   - Manages a lightweight workspace directory structure:
     ```
     .mosaic-workspace/
       scotts-rv-rockford/            # auto-named from first CSV or user input
         cubes/
           targeted-display.cube.json  # serialized cube state
           stv-hulu-ron.cube.json
         narratives/
           report-2025-08.json         # the narrative output
     ```
   - Cross-tactic comparison templates ("Overall: 4 tactics processed. Highest engagement: STV Hulu RON at 92% completion. Lowest: Targeted Display at 0.44% CTR.")
2. Frontend: workspace view showing all detected tactics with their narratives; overall summary at top
3. `POST /api/upload` accepts workspace name (optional; auto-generates from first CSV's campaign name if absent)

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| Workspace persistence: in-memory or on-disk? | **On-disk** (JSON files in `.mosaic-workspace/`). The demo should survive a server restart. |
| Multi-upload: overwrite or accumulate? | **Overwrite per tactic.** Uploading new Targeted Display CSVs replaces the previous Targeted Display cube. Different tactics coexist. |
| Cross-tactic narratives: how? | **A special "summary" template family** that reads from ALL cubes in the workspace. Iterates tactic names + their headline metrics; produces 2-3 summary sentences. |
| Advertiser name: where from? | **From the CSV filename or campaign name column.** The campaign-performance CSV has "Campaign Name" as first column; parse the advertiser name from it. Fallback: user specifies in the upload form. |

---

### Session 5 (~2-3 hours): Polish + demo prep

**Goal:** Demo-ready. No rough edges visible to leadership.

**Deliverables:**
1. **The "Show Payload" view** — a toggle on the narrative page that shows the raw JSON with evidence objects, template IDs, severity tags, and the workspace manifest. This is the "proof we're not cheating" moment.
2. **The `mc start` banner polish** — proper ASCII art, version number, test count, "Formula engine complete" tagline.
3. **Error states** — what happens when a CSV doesn't match any registry entry? When a required column is missing? When the zip is malformed? Clean error messages in the UI.
4. **Copy-to-clipboard** for the rendered narrative report (Markdown format). Leadership can paste into a doc or email.
5. **Demo script** — a step-by-step walkthrough for the demo call:
   - Show terminal: `mc start` → banner appears → browser opens
   - Upload the Scotts RV zip (sample data)
   - Detection display: "Found 11 CSVs; detected: Targeted Display (11 reports)"
   - Narrative report renders in <2 seconds
   - Click "Show Payload" → raw JSON with evidence
   - Click "Copy as Markdown" → paste into a doc
   - "Questions?" → point to the 912-test suite, the formula engine, the 190-tactic registry

---

## Hard Rules

1. **`mc-core`, `mc-fixtures`, `mc-model`, `mc-recipe`, `mc-drivers`, `mc-tessera`, `mosaic-plugin/` are NOT modified.** Phase 6D adds a new crate (`mc-demo-server`) and a new directory (`demo/`). No changes to shipped crates.
2. **`tokio` is allowed ONLY in `mc-demo-server`.** The existing kernel crates stay sync-only per CLAUDE.md §1.
3. **The frontend is a separate Vite project** in `demo/frontend/`. Not compiled into the Rust binary. Not published to crates.io. It's demo infrastructure.
4. **The narrative templates are YAML files in `demo/narratives/`**, not integrated into `mc-model`'s schema yet. Integration comes in the proper Phase 7A.
5. **The performance_tables registry lives in `demo/registry/`** (copied from ignite-report-ai). Not yet part of `mc-recipe` or `mc-model`.
6. **Per-session commit discipline (Rule 11).** Each session gets at least one commit. Progressive delivery: each session ends with something demo-able.
7. **No production deployment.** This runs on `localhost:8080` only. No Vercel, no auth, no HTTPS. That's Phase 7 scope.

---

## Directory structure (end state)

```
demo/
├── README.md                          (how to run the demo)
├── frontend/                          (Vite + React + Tailwind)
│   ├── package.json
│   ├── vite.config.ts
│   ├── src/
│   │   ├── App.tsx                    (upload → detect → narrate → report flow)
│   │   ├── pages/Upload.tsx
│   │   ├── pages/Detection.tsx
│   │   ├── pages/Report.tsx
│   │   ├── pages/Payload.tsx
│   │   └── components/...
│   └── dist/                          (built static files, gitignored)
├── registry/
│   ├── performance_tables.csv
│   └── performance_tables.json
├── narratives/
│   ├── display-like.yaml
│   ├── video-like.yaml
│   ├── search-like.yaml
│   └── social-like.yaml
├── sample-data/
│   └── scotts-rv-targeted-display/    (the .demo-data CSVs, ready for zip)
└── scripts/
    └── build-frontend.sh              (npm run build in frontend/)

crates/mc-demo-server/
├── Cargo.toml
└── src/
    ├── main.rs                        (mc start entry point + banner)
    ├── server.rs                      (axum routes + static file serving)
    ├── registry.rs                    (performance_tables parser)
    ├── upload.rs                      (zip extraction + tactic detection)
    ├── ingest.rs                      (CSV → cube population via mc-core)
    ├── narrative.rs                   (template engine + rendering)
    └── workspace.rs                   (advertiser × order routing)
```

---

## Acceptance criteria

- [ ] `mc start` prints the banner and opens `http://localhost:8080` in the browser.
- [ ] Uploading a zip with 3+ CSVs from the sample data triggers correct tactic detection against the registry.
- [ ] Each detected tactic produces at least 3 narrative paragraphs.
- [ ] The conversion-tracking alarm fires for zero-conversion tactics.
- [ ] The "Show Payload" view displays structured JSON with evidence objects + `processing_time_ms` + `timing` breakdown.
- [ ] **Backend processing completes in < 200ms** for the sample data set (per ADR-0019 Decision 11 performance contract).
- [ ] **The terminal running `mc start` displays the timing breakdown** for every upload: per-stage ms + total `Done Xms` line.
- [ ] **The frontend displays "Processed in Xms"** badge on the report page (reads from `processing_time_ms` in the JSON response).
- [ ] The demo works offline (no LLM calls, no network requests beyond localhost).
- [ ] `cargo test --workspace` still passes 912/0/5 (no regressions in existing crates).
- [ ] All 5 Decision 11 optimizations are implemented: in-memory zip, skip YAML round-trip, pre-warm registry, reuse cubes, pre-compile templates.

---

## SPEC QUESTION format

Same as always (CLAUDE.md §11). Most likely candidates:

- Session 1: how to add `mc start` as a subcommand to the existing `mc` binary without polluting `mc-cli` with async deps (might need a thin shim that spawns the demo server binary, rather than linking `tokio` into `mc-cli`).
- Session 2: whether to use `mc-model::compile` directly or build a lighter "demo-mode compile" that skips validation for speed.
- Session 3: how to call `eval_expr` from outside `Cube::read` (the formula engine expects an `EvalCtx` that lives inside the cube's read path; narrative templates might need to evaluate formulas against cube state without going through the full read machinery).

---

*End of handoff. Phase 6D is where Mosaic goes from "engine that works" to "product that demos." The formula engine (Phase 3) computes the numbers; the narrative engine (Phase 6D) turns them into words. After the demo lands, the proper phases (4C, 5D, 6B, 7A) build production-grade infrastructure on top of what the demo proved.*
