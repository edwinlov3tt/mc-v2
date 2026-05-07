# ADR-0019: Phase 6D — Marketing Report Demo MVP

**Status:** Proposed
**Date:** 2026-05-07
**Deciders:** project owner
**Phase:** 6D (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 6D is a **vertical-slice demo MVP** that proves Mosaic can produce deterministic marketing narrative reports from raw CSV exports — instantly, with no LLM inference cost, no hallucination, and full payload transparency. The demo target: upload a zip of CSVs from 3-4 different tactics (Targeted Display, STV, SEM, Meta), and the system auto-detects each tactic, populates cubes, fires narrative templates, and returns a formatted report. Leadership demo at work. "Look how fast this generates a monthly report" is the pitch.

---

## Context

The project owner does monthly marketing reports at work. The current process: export CSVs from a marketing platform (11 report types per tactic × 20 product categories × 59 sub-products = ~190 unique report shapes), manually analyze numbers, write narrative paragraphs ("Tampa Search ran 23% over budget at $11,500..."), compile into a report. This takes hours and costs thousands in LLM tokens if AI-assisted.

Mosaic's formula engine (Phase 3, complete) can compute every comparison deterministically. The missing piece is the **narrative rendering layer** — templates that turn computed values into human-readable prose — and the **delivery surface** — a UI where a user uploads CSVs and gets a report back.

A separate repo (`ignite-report-ai`) already has:
- CSV parsing infrastructure (PapaParse + schema validation + header matching)
- A complete tactic registry (`performance_tables.csv` — 190+ rows mapping product → sub-product → table_name → expected_headers)
- React + Vite + Tailwind + shadcn/ui scaffold
- Import/export transformers

Phase 6D reuses ~30-40% of that infrastructure and adds the Mosaic kernel integration + narrative template engine.

**Why Phase 6D and not Phase 7A.** Phase 6's description in MASTER_PHASE_PLAN.md says "at least one shipped proof-of-value internal use case demonstrating that the system produces a correct plan a human operator trusts." This IS that use case. It's not productization (Phase 7); it's an internal proof.

---

## Decisions

### Decision 1: Scope — vertical-slice demo MVP across 5 capability layers

| Layer | What ships | What's deferred |
|---|---|---|
| **Tactic detection** | Registry-driven auto-detection from `performance_tables.csv` | Auto-learning new report shapes |
| **Cube ingestion** | CSV → auto-generated cube schema → Tessera recipe → populated cube | Multi-period cube accumulation; incremental loads |
| **Narrative templates** | Minimum-viable: placeholder substitution + `when:` predicates + format hints + conditional branching; 8-10 templates across 4 tactic families (display-like, video-like, search-like, social-like) | Full composition, notability filters, benchmark versioning, ledger persistence |
| **Workspace routing** | Advertiser × Order → directory; lightweight manifest | Full Phase 4C workspaces; shared catalogs; cross-cube refs |
| **UI + `mc start`** | Scrappy: zip upload → detection display → narrative report → raw JSON payload view. `mc start` prints banner + boots server + opens browser. | Full Phase 6B planning grid; interactive template editor; `mc start` interactive launcher with workspace creation prompts |

**Deliberately cut (not in this phase):**
- Narrative ledger / historical analysis
- Benchmark aggregation from evidence trails
- Cross-period memory ("this is the third consecutive month...")
- Full TUI (per Claude Desktop's recommendation — skip the TUI; the browser is the interactive surface)
- Production deployment (Vercel, auth, multi-tenancy)
- Full `mc start` interactive launcher (workspace creation prompts are stretch; v1 is just banner + server + browser)

### Decision 2: Architecture — demo server as a Rust binary + React frontend

```
mc start
  → prints banner (ASCII art + version)
  → starts HTTP server on localhost:8080 (axum)
  → opens browser to http://localhost:8080
  → serves React frontend (static files embedded or from disk)

POST /api/upload (multipart/form-data — zip file)
  → extract zip
  → match each CSV filename against performance_tables registry
  → for each detected tactic:
      → auto-generate cube YAML from registry headers
      → populate cube via mc-core (in-process, not subprocess)
      → evaluate narrative templates against populated cube
  → return JSON: { workspace, tactics: [{ product, subproduct, narratives: [...] }] }

GET /api/registry
  → returns the performance_tables registry (for the detection display)
```

**The demo server links against `mc-core`, `mc-model`, and `mc-tessera` as library crates.** No subprocess spawning. The Rust binary compiles cube schemas on the fly, populates them from CSV data, evaluates formula-driven narrative templates, and returns structured JSON. The React frontend is presentation only.

### Decision 3: Registry-driven tactic detection

The `performance_tables.csv` file (190+ rows) maps `file_name` → `product_name` + `subproduct_name` + `table_name` + `headers`. When a CSV is uploaded:

1. Extract the filename (e.g., `report-targeteddisplay-monthly-performance.csv`)
2. Match against `file_name` column in the registry
3. If matched: know the product, sub-product, table type, and expected headers
4. Validate actual CSV headers against expected headers (Jaccard similarity for fuzzy matching)
5. If no match: report as "unknown tactic" in the detection display

**The registry IS the generalization.** No hardcoded tactic logic. Adding support for a new tactic = adding a row to the registry + writing a narrative template for that tactic family.

### Decision 4: Minimum-viable narrative template engine

**Not the full Phase 7A engine.** Just enough to produce compelling narratives for the demo:

```yaml
narratives:
  - id: impressions_mom_change
    family: display-like           # which tactic families this fires for
    severity: info                 # info | warning | critical
    when: "abs(period_delta(Impressions) / prev(Impressions)) > 0.05"  # >5% change
    template: "{tactic_name} impressions {direction} {abs_pct_change:.1f}% from {prev_period} ({prev_impressions:,.0f}) to {current_period} ({current_impressions:,.0f})."
    bindings:
      direction: "if(period_delta(Impressions) >= 0, 'grew', 'declined')"
      abs_pct_change: "abs(period_delta(Impressions) / prev(Impressions) * 100)"
      prev_impressions: "prev(Impressions)"
      current_impressions: "Impressions"
      prev_period: "prev_period_name()"     # e.g., "July"
      current_period: "current_period_name()"  # e.g., "August"
```

Templates reference the formula engine's existing functions (`period_delta`, `prev`, `if`, `is_element`, `avg_over`, etc. — all shipped in Phase 3). The narrative engine evaluates bindings, substitutes placeholders, and formats numbers.

**Template families** (4 for the demo):
- **display-like**: CTR-focused, conversion-focused, spend-efficiency (Targeted Display, Addressable Display, Social Display, Native)
- **video-like**: completion-rate-focused, reach-focused, frequency-capping (STV variants, Hulu, YouTube TV, Targeted Video)
- **search-like**: CPC-focused, conversion-rate-focused, quality-score (SEM, Google Search, Spark)
- **social-like**: engagement-focused, link-click-focused, awareness-focused (Meta variants, TikTok, Snapchat, Twitter, Pinterest, LinkedIn)

Each family gets 2-3 templates. Total: ~10 templates that cover the most common narrative patterns across all 59 sub-products.

### Decision 5: `mc start` command with banner

**Binding banner (ASCII art):**

```
  ███╗   ███╗ ██████╗ ███████╗ █████╗ ██╗ ██████╗
  ████╗ ████║██╔═══██╗██╔════╝██╔══██╗██║██╔════╝
  ██╔████╔██║██║   ██║███████╗███████║██║██║
  ██║╚██╔╝██║██║   ██║╚════██║██╔══██║██║██║
  ██║ ╚═╝ ██║╚██████╔╝███████║██║  ██║██║╚██████╗
  ╚═╝     ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝╚═╝ ╚═════╝

  Large Numbers Model · v0.1.0
  912 tests · 75 diagnostic codes · Phase 3 complete

  Starting server on http://localhost:8080
  Opening browser...
  Press Ctrl-C to stop.
```

**v1 behavior (demo):** print banner → start axum server → open browser → serve frontend. No interactive prompts in v1 (stretch goal: inquire-based workspace creation per Claude Desktop's notes).

**Post-demo (Phase 6C+):** `mc start` evolves into the full interactive launcher with workspace creation, cartridge browsing, settings configuration, and `mc serve` delegation.

### Decision 6: New dependencies (demo-scoped)

| Dep | Purpose | Crate |
|---|---|---|
| HTTP server | API endpoints | `axum` + `tower-http` (static files) |
| Multipart upload | Zip file handling | `axum-multipart` or `multer` |
| Zip extraction | Unpack uploaded archives | `zip` |
| CSV parsing | Parse report CSVs | Hand-rolled (existing `mc-model/src/csv.rs`) OR `csv` crate |
| Terminal color | Banner rendering | `crossterm` (already a transitive dep?) OR hand-rolled ANSI |
| Browser open | `mc start` opens browser | `open` crate (cross-platform) |

**These deps go in a NEW crate `mc-demo-server` (NOT in `mc-core` or `mc-model`).** The core kernel stays dependency-clean. The demo server is a separate binary that links against the kernel as a library.

### Decision 7: ignite-report-ai reuse strategy

| Component | Action | Source path |
|---|---|---|
| `performance_tables.csv` / `.json` | **Copy into `demo/registry/`** | `ignite-report-ai/performance_tables.*` |
| CSV parsing + header validation | **Adapt** (Rust reimplementation of the PapaParse + Jaccard matching logic) | `ignite-report-ai/src/lib/fileParser.ts` |
| React + Vite + Tailwind scaffold | **Lift + strip** campaign-specific pages; add upload + narrative pages | `ignite-report-ai/src/` |
| StructuredReport data model | **Adapt** for narrative output shape | `ignite-report-ai/src/types/structuredReport.ts` |
| Import/export transformers | **Reference only** (pattern, not code) | `ignite-report-ai/src/lib/importExport/` |

**The demo frontend is a SEPARATE package** (not compiled into the Rust binary). It's a Vite project in `demo/frontend/` that builds to static files. The Rust server serves those files via `tower-http::services::ServeDir`.

### Decision 8: Demo success criteria

The demo is successful when:

1. **Upload a zip with 3-4 CSVs from different tactics** (Targeted Display + STV + SEM + Meta) → system detects all 4 correctly.
2. **Narrative report renders within 2 seconds** of upload completing (deterministic — no LLM call).
3. **At least 3 narrative paragraphs per tactic** that read like a human analyst wrote them.
4. **The "show payload" view displays the raw JSON** so leadership can see there's no LLM behind the curtain.
5. **The conversion-tracking alarm fires** for zero-conversion tactics (the "wow" moment).
6. **`mc start` prints the banner** and leadership sees the product name + version.

### Decision 9: Relationship to proper phases

Phase 6D is a PROOF, not a replacement for the proper phases:

| Proper phase | What 6D proves | What the proper phase adds |
|---|---|---|
| 4C (workspaces) | Lightweight advertiser × order routing works | Full shared catalogs, `$ref:` resolution, workspace-level lint |
| 5D (Tessera xlsx + multi-file) | Multi-CSV-from-zip ingestion works | XLSX support, group_by transforms, incremental loads |
| 6B (web UI) | Browser-based report display works | Full planning grid, drill-down, edit, snapshot comparison |
| 6C (distribution) | `mc start` with banner works | `cargo-dist` cross-compile, Homebrew tap, curl installer, self-update |
| 7A (narrative engine) | Template substitution + conditional narratives work | Full composition, notability filters, ledger persistence, benchmark aggregation |

After the demo, each proper phase builds production-grade infrastructure. The demo MVP is the forcing function that proves the concept and gets leadership buy-in.

### Decision 11: Performance contract — sub-200ms backend processing with CLI timing display

**Binding target:** the full backend pipeline (zip extraction → tactic detection → cube compilation + population → narrative template evaluation → JSON serialization) completes in **< 200ms** for the sample dataset (~50 rows across 11 CSVs). User-perceived latency (including browser render) < 100ms additional.

**CLI timing display (binding):** every upload processed by the demo server prints a timing line to the terminal:

```
[2026-05-07 09:15:03] POST /api/upload — Scotts RV (11 CSVs, 4 tactics)
  Registry match:    1.2ms
  Cube compile:      8.4ms
  Cube populate:     3.1ms
  Narrative eval:    0.8ms
  Serialize:         0.4ms
  ─────────────────────────
  Done               14.1ms
```

Implementation: wrap each pipeline stage in `std::time::Instant::now()` + `.elapsed()`. Print to stdout (the terminal where `mc start` is running). The total `Done Xms` line is the demo punchline — leadership sees the processing time in the terminal while the report appears in the browser.

**The JSON response also includes timing:**

```json
{
  "schema_version": "1.0",
  "processing_time_ms": 14.1,
  "timing": {
    "registry_match_ms": 1.2,
    "cube_compile_ms": 8.4,
    "cube_populate_ms": 3.1,
    "narrative_eval_ms": 0.8,
    "serialize_ms": 0.4
  },
  "tactics": [ ... ],
  "narratives": [ ... ]
}
```

The frontend displays `processing_time_ms` as a badge on the report page: "Processed in 14ms" — next to the narrative output.

**5 baked-in optimizations (binding — not afterthoughts):**

1. **In-memory zip extraction.** Use `zip::ZipArchive::new(Cursor::new(bytes))` — no temp files written to disk. Saves ~10ms of filesystem I/O.
2. **Skip the YAML round-trip.** Construct `ParsedModel` directly in Rust from the registry + CSV data — don't generate YAML strings and re-parse them. Saves ~5ms of parse overhead.
3. **Pre-warm the registry at `mc start` time.** Parse `performance_tables.csv` into a `HashMap<String, TacticSpec>` once at server startup, before the first request. Registry lookup is then a single hash-map get (~100ns).
4. **Reuse cube instances across templates.** One cube per tactic, populated once; all narrative templates evaluate against the same cube. Don't re-populate per template.
5. **Pre-compile narrative templates at startup.** Parse the narrative YAML files + compile `when:` predicates to `Expr` ASTs at server startup. Per-request work is eval only (~1µs per expression).

**Why this matters for the demo:** speed is the proof that there's no LLM behind the curtain. An LLM generating the same output takes 3-8 seconds. Mosaic does it in 14ms. The terminal timing display makes this undeniable — leadership sees the number in real-time.

### Decision 12: Process flow — ADR-first per Rule 1

Per process-notes Rule 1 self-test:
1. Kernel change? No (demo server links against mc-core as a library; no mc-core modifications).
2. Runtime dep added? **Yes** (axum, zip, open, crossterm — in new `mc-demo-server` crate).
3. Contract surface change? Yes (new `mc start` CLI verb; new HTTP API; new narrative template YAML schema).

New deps + contract surface changes → ADR-first. This ADR is the binding design document. Handoff follows immediately after acceptance.

---

## Out of scope

- Narrative ledger / historical analysis (Phase 7A.2)
- Benchmark aggregation from evidence trails (Phase 7A.4)
- Cross-period memory (Phase 7A.3)
- Full TUI (per Claude Desktop: skip; browser is the interactive surface)
- Interactive `mc start` launcher with workspace creation prompts (stretch goal; v1 is banner + server + browser)
- Production deployment (auth, multi-tenancy, Vercel)
- Full Phase 6B planning grid (the demo UI is upload + display only)
- PPTX extraction (available in ignite-report-ai; defer unless demo needs it)

---

## Cross-links

- **Narrative engine proposal:** [`../research-notes/narrative-engine-and-ledger-proposal.md`](../research-notes/narrative-engine-and-ledger-proposal.md) (to be drafted)
- **Multi-domain workspaces proposal:** [`../research-notes/multi-domain-workspaces-proposal.md`](../research-notes/multi-domain-workspaces-proposal.md)
- **ignite-report-ai repo:** `https://github.com/edwinlov3tt/ignite-report-ai`
- **Performance tables registry:** `ignite-report-ai/performance_tables.csv` (190+ tactic → report → header mappings)
- **Demo data:** `docs/.demo-data/report-targeteddisplay-*.csv` (Scotts RV / Rockford / Targeted Display sample)
- **Claude Desktop launcher notes:** captured in this ADR §Decision 5 and §"Out of scope" (full interactive launcher)
- **Grid UI prototype:** `docs/prototypes/mosaic-grid-prototype.html`
- **Phase 3 retrospective:** [`../reports/phase-3-retrospective.md`](../reports/phase-3-retrospective.md) (formula engine is complete; narratives build on it)

---

## Acceptance amendments

*(None as of authoring. Project owner review pending.)*
