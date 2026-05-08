# Phase 7A.4 Handoff — Benchmark Aggregation

> **Audience:** the Claude Code instance that implements Phase 7A.4.
> **You inherit `main` at the tip of `phase-7a-3/cross-period-analysis`
> (983 / 0 / 5 tests). You'll work on the branch
> `phase-7a-4/benchmark-aggregation`.**
>
> **This phase gives each workspace a mirror to its own history.**
> Phase 7A.3 can say "this is the third consecutive month." Phase 7A.4
> adds: "and that's below your own historical median for this channel."
> The benchmark library is built from the workspace's own ledger —
> no cross-customer data, no servers, no anonymization. Just the
> workspace owner's history turned into percentile distributions
> that the narrative evaluator can query.
>
> **The binding design is in
> [`docs/decisions/0021-phase-7a-4-benchmark-aggregation.md`](../decisions/0021-phase-7a-4-benchmark-aggregation.md).
> Read it in full before starting.**

---

## The one paragraph you must internalize

The interpretation ledger (Phase 7A.2) already records every evidence
value that went into every narrative: the CTR that triggered a
`ctr_trend` narrative, the impressions count that triggered an
`impressions_mom` narrative, etc. After 6 months of operation that
ledger is dense with the workspace's own performance numbers. Phase
7A.4 reads those evidence values, groups them by metric + scope
(channel, market), computes percentile distributions (p10/p25/p50/
p75/p90), and writes the result to `.mosaic/benchmark-library.json`.
The narrative evaluator then gets a new family of functions —
`benchmark_p50()`, `benchmark_percentile()`, `benchmark_z_score()` —
that templates can use to say "CTR is in the bottom quartile of your
own historical range." No LLM, no external API, no data leaving the
workspace. Just the workspace owner's own numbers, summarized.

---

## What gets built (4 sessions estimated)

### Session 1 (~3-4h): Benchmark library schema + build pipeline

**Goal:** `mc model build-benchmarks` reads the ledger and writes
`.mosaic/benchmark-library.json`.

**Deliverables:**

1. **Benchmark library schema** in `mc-narrative/src/benchmark.rs`:

   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct BenchmarkLibrary {
       pub schema_version: String,        // "1.0"
       pub generated_at: String,          // ISO-8601 UTC
       pub workspace: String,             // model directory name
       pub period_range: PeriodRange,
       pub period_count: usize,
       pub benchmarks: BTreeMap<String, MetricBenchmark>, // keyed by "metric::scope_key"
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct PeriodRange {
       pub from: String,   // earliest report_period in the ledger sample
       pub to: String,     // latest report_period
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct MetricBenchmark {
       pub metric: String,
       pub scope: BTreeMap<String, String>,  // { channel: "Targeted Display", ... }
       pub p10: f64,
       pub p25: f64,
       pub p50: f64,
       pub p75: f64,
       pub p90: f64,
       pub mean: f64,
       pub stddev: f64,
       pub sample_count: usize,
   }
   ```

2. **Build function** —
   `pub fn build_benchmark_library(ledger: &[LedgerEntry], workspace: &str, since: Option<&str>) -> BenchmarkLibrary`:

   - Filters entries by `report_period >= since` if `--since` is given
   - Iterates all entries; for each entry, iterates its `evidence` map
   - Groups numeric evidence values by `(field_name, scope_key)` where
     scope_key is `k1=v1,k2=v2` from the entry's BTreeMap scope
   - Computes percentiles using a simple sort-based approach (no
     external crate needed — sort the Vec<f64>, index at the right
     position)
   - Returns a `BenchmarkLibrary` with one `MetricBenchmark` per
     (field_name, scope_key) group

3. **Write function** —
   `pub fn write_benchmark_library(dir: &Path, lib: &BenchmarkLibrary) -> Result<(), BenchmarkError>`:

   - Atomic write: `.mosaic/benchmark-library.json.tmp` → rename
   - Same pattern as `write_ledger_entry` in `ledger.rs` (tmp+rename
     for POSIX atomicity)
   - Creates `.mosaic/` if absent

4. **Read function** —
   `pub fn read_benchmark_library(dir: &Path) -> Result<BenchmarkLibrary, BenchmarkError>`:

   - Reads `.mosaic/benchmark-library.json`
   - Parses with serde_json
   - Returns `BenchmarkError::NotFound` if the file is absent (callers
     treat this as "no benchmarks available, skip benchmark templates")

5. **New CLI verb** — `mc model build-benchmarks`:
   ```
   mc model build-benchmarks <model-dir> [--since <period>]
   ```
   - Reads the ledger from `.mosaic/analysis-ledger.jsonl`
   - Calls `build_benchmark_library`
   - Writes `.mosaic/benchmark-library.json`
   - Prints summary to terminal:
     ```
     [benchmarks] Built from 47 ledger entries across 6 periods (2025-11 → 2026-04)
     [benchmarks] CTR::channel=Targeted Display  p50=0.18%  (6 samples)
     [benchmarks] Impressions::channel=Targeted Display  p50=52,430  (6 samples)
     [benchmarks] Wrote .mosaic/benchmark-library.json
     ```

**Percentile computation:**

```rust
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
```

Sort ascending, index. No interpolation needed for Phase 7A.4 (the
nearest-rank method is sufficient for 6-24 samples).

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| What fields from `evidence` become benchmarks? | **All numeric fields.** The evidence BTreeMap has string keys → serde_json::Value. Filter to `Value::Number` entries, extract as f64. Non-numeric evidence (strings, arrays) is skipped. |
| Scope key format | **Same as ledger index:** `k1=v1,k2=v2` from BTreeMap (deterministic, sorted). The benchmark library key is `metric::scope_key`, e.g., `CTR::channel=Targeted Display,market=Rockford`. |
| Minimum samples for a benchmark | **1.** The `sample_count` field tells the reader. No suppression. A 1-sample "benchmark" isn't useful but it's honest. Templates can guard with `benchmark_sample_count('CTR') >= 3` if they care. |
| What if `--since` filters out all entries? | **Empty library.** `period_count: 0`, empty `benchmarks`. Write it anyway — subsequent builds will replace it. |
| The `workspace` field | **The model directory's `file_name()` component.** E.g., `"scotts-rv"` from `/path/to/scotts-rv/model.yaml`. Informational only. |

**Regression tests (5 minimum):**
1. `test_build_benchmark_library_computes_percentiles`
2. `test_build_benchmark_library_groups_by_scope`
3. `test_write_and_read_benchmark_library_roundtrip`
4. `test_build_benchmarks_since_filter`
5. `test_build_benchmarks_empty_ledger_produces_empty_library`

---

### Session 2 (~3-4h): Benchmark evaluator functions + `show-benchmarks` verb

**Goal:** Narrative templates can call `benchmark_p50()` etc. New
`show-benchmarks` verb displays the library.

**Deliverables:**

1. **`BenchmarkIndex`** in `mc-narrative/src/evaluator.rs` (alongside
   the existing `LedgerIndex`):

   ```rust
   pub struct BenchmarkIndex {
       /// Key: (metric_name, scope_key) → MetricBenchmark
       entries: HashMap<(String, String), MetricBenchmark>,
   }
   ```

   Built once per `evaluate_all` call from the benchmark library.
   Passed into evaluation alongside the ledger index.

2. **Benchmark evaluator functions** — add to the dispatch table in
   `evaluator.rs` alongside `ledger_count`, `ledger_streak`, etc.:

   ```
   benchmark_p10(metric)              → f64
   benchmark_p25(metric)              → f64
   benchmark_p50(metric)              → f64
   benchmark_p75(metric)              → f64
   benchmark_p90(metric)              → f64
   benchmark_mean(metric)             → f64
   benchmark_percentile(metric, value) → f64   (0-100: where does value fall?)
   benchmark_above_median(metric)     → f64    (1.0 if value > p50, 0.0 otherwise)
   benchmark_z_score(metric, value)   → f64    ((value - mean) / stddev)
   benchmark_sample_count(metric)     → f64    (cast to f64 for uniformity)
   ```

   **Scope matching:** when the evaluator is processing a cube with
   scope `{ channel: "Targeted Display" }`, benchmark lookups use
   that scope to build the scope_key and find the matching benchmark.
   Fallback: if no scoped benchmark exists, try the empty scope key
   (aggregated across all scopes). If still nothing, return 0.0 (same
   graceful degradation as ledger functions).

   **`benchmark_percentile` implementation:**
   ```rust
   // Where does `value` fall in the historical distribution?
   // Returns a 0-100 percentile rank.
   fn benchmark_percentile(benchmark: &MetricBenchmark, value: f64) -> f64 {
       if value <= benchmark.p10 { return 10.0; }
       if value <= benchmark.p25 { return 25.0; }
       if value <= benchmark.p50 { return 50.0; }
       if value <= benchmark.p75 { return 75.0; }
       if value <= benchmark.p90 { return 90.0; }
       100.0
   }
   ```
   Linear interpolation between breakpoints is a Phase 7B refinement.
   Nearest-breakpoint is sufficient for Phase 7A.4.

3. **`evaluate_all` signature update** — add `benchmark: Option<&BenchmarkLibrary>`:

   ```rust
   pub fn evaluate_all(
       templates: &[TemplateDefinition],
       cubes: &[CubeData],
       ledger: Option<&[LedgerEntry]>,
       benchmark: Option<&BenchmarkLibrary>,  // NEW
   ) -> Vec<NarrativeOutput>
   ```

   All existing callers pass `None` — zero behavior change. The
   `BenchmarkIndex` is only built when `benchmark` is `Some`.

4. **New CLI verb** — `mc model show-benchmarks`:
   ```
   mc model show-benchmarks <model-dir> [--metric <name>] [--format json|text]
   ```
   - Text format: a table per metric (p10, p25, p50, p75, p90, samples)
   - JSON format: pretty-prints the raw library JSON
   - Does NOT rebuild — reads the existing library only

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| Current scope in benchmark lookup | **From the CubeData's scope field** (same as ledger scope matching in Phase 7A.3). The `CubeData` struct already carries scope. |
| What if `stddev == 0` for `benchmark_z_score`? | **Return 0.0.** All samples identical → z-score of zero is correct (no deviation from mean). |
| `benchmark_above_median` compared to what value? | **`current.Metric` evaluated against the scope's p50.** The function takes only the metric name; it reads the current cube's value internally. See implementation note below. |
| Two-arg vs one-arg benchmark functions | **One-arg for `benchmark_p50(metric)` etc.** The "value to compare" functions (`benchmark_percentile`, `benchmark_z_score`) take two args: metric + value. This mirrors the template usage pattern: `benchmark_percentile('CTR', current.CTR)`. |

**Implementation note on `benchmark_above_median`:**

```
when: "benchmark_above_median('CTR') == 1"
```

This is shorthand for "current.CTR > benchmark_p50('CTR')". The
evaluator implements it as: evaluate `current.CTR` from the cube,
compare to the p50 from the benchmark. This requires the evaluator
to read the current cube value — which it already does for binding
evaluation. No special machinery needed.

**Regression tests (5 minimum):**
1. `test_benchmark_p50_returns_median`
2. `test_benchmark_percentile_ranks_value`
3. `test_benchmark_above_median_returns_correct_boolean`
4. `test_benchmark_z_score_computation`
5. `test_benchmark_functions_return_zero_when_no_library`

---

### Session 3 (~2-3h): Benchmark templates + demo integration

**Goal:** Ship 4-6 benchmark templates. Wire demo server to load the
benchmark library. Show the template editor prototype in the demo UI.

**Deliverables:**

1. **New template file** — `demo/narratives/benchmark-templates.yaml`:

   ```yaml
   narrative_format_version: 1

   templates:
     - id: ctr_above_own_median
       family: [display-like]
       severity: success
       table_types: ["Monthly Performance"]
       when: "benchmark_above_median('CTR') == 1 AND benchmark_sample_count('CTR') >= 3"
       template: >
         {tactic_name} CTR of {current_ctr:.2f}% is above this
         campaign's historical median ({median_ctr:.2f}%) across
         {sample_count:.0f} reporting periods. Performance is in
         the upper half of this workspace's own track record.
       bindings:
         current_ctr: "current.CTR"
         median_ctr: "benchmark_p50('CTR')"
         sample_count: "benchmark_sample_count('CTR')"

     - id: ctr_below_own_p25
       family: [display-like]
       severity: warning
       table_types: ["Monthly Performance"]
       when: >
         benchmark_percentile('CTR', current.CTR) < 25
         AND benchmark_sample_count('CTR') >= 3
       template: >
         {tactic_name} CTR of {current_ctr:.2f}% is in the bottom
         quartile of this campaign's historical performance (your
         typical range: {p25_ctr:.2f}%–{p75_ctr:.2f}%). This is
         below the normal range for this channel.
       bindings:
         current_ctr: "current.CTR"
         p25_ctr: "benchmark_p25('CTR')"
         p75_ctr: "benchmark_p75('CTR')"

     - id: impressions_unusually_high
       family: [display-like]
       severity: info
       table_types: ["Monthly Performance"]
       when: >
         benchmark_percentile('Impressions', current.Impressions) >= 90
         AND benchmark_sample_count('Impressions') >= 3
       template: >
         {tactic_name} impressions of {current_value:,.0f} are in the
         top 10% of this campaign's historical delivery
         (p90={p90_value:,.0f}). This is an unusually high-reach period.
       bindings:
         current_value: "current.Impressions"
         p90_value: "benchmark_p90('Impressions')"

     - id: ctr_benchmark_context
       family: [display-like]
       severity: info
       table_types: ["Monthly Performance", "Campaign Performance"]
       when: "benchmark_sample_count('CTR') >= 2"
       template: >
         Historical context: across {sample_count:.0f} prior periods,
         this campaign's CTR has ranged from {p10_ctr:.2f}%
         (p10) to {p90_ctr:.2f}% (p90), with a median of
         {median_ctr:.2f}%.
       bindings:
         sample_count: "benchmark_sample_count('CTR')"
         p10_ctr: "benchmark_p10('CTR')"
         p90_ctr: "benchmark_p90('CTR')"
         median_ctr: "benchmark_p50('CTR')"

     - id: spend_efficiency_trending
       family: [display-like]
       severity: info
       table_types: ["Monthly Performance"]
       when: >
         period_count >= 2
         AND benchmark_sample_count('CTR') >= 3
         AND benchmark_z_score('CTR', current.CTR) > 1.5
       template: >
         CTR is {z_score:.1f} standard deviations above this
         campaign's historical mean — a statistically notable
         outperformance. The current period may represent a
         best-in-class creative or targeting combination worth
         preserving.
       bindings:
         z_score: "benchmark_z_score('CTR', current.CTR)"
   ```

2. **Demo server integration** in `mc-demo-server/src/narrative.rs`:

   - On startup: attempt to load `.mosaic/benchmark-library.json`
     from the workspace directory; store as `Option<BenchmarkLibrary>`
   - Pass to `evaluate_all` (which now accepts `benchmark: Option<&BenchmarkLibrary>`)
   - Add `GET /api/benchmarks` endpoint: returns the benchmark
     library JSON if present, 404 if not
   - Terminal log on startup: if library exists, print
     `[benchmarks] Loaded library: N metrics, period range X–Y`

3. **Template editor prototype** — wire `demo/template-editor-prototype.html`
   into the demo UI as a second tab or accessible via a button. The
   prototype is already built as a standalone HTML file. The demo
   server should serve it at `/template-editor` (static file serving —
   no backend changes needed beyond adding the route). This gives the
   demo a "Build a Template" button that opens the editor.

   Implementation:
   ```rust
   // In mc-demo-server/src/main.rs, add alongside other static routes:
   .route("/template-editor", get(|| async {
       axum::response::Html(include_str!("../../../demo/template-editor-prototype.html"))
   }))
   ```

   The frontend `index.html` should add a button:
   ```html
   <a href="/template-editor" target="_blank" class="btn-secondary">
     Build a Template
   </a>
   ```

**Decision Matrix:**

| Wall | Binding decision |
|---|---|
| Does `narrate-trends` also load benchmarks? | **Yes** — `narrate-trends` loads ledger + benchmark library and passes both to `evaluate_all`. Trend templates can reference benchmark functions. |
| Does `mc model narrate` (base verb) load benchmarks? | **Yes, if present.** Same graceful degradation: load if `.mosaic/benchmark-library.json` exists, skip if not. |
| Auto-rebuild on upload? | **No.** The demo server does not rebuild the benchmark library on upload. Explicit rebuild via `mc model build-benchmarks`. Upload only re-evaluates templates against the existing library. |
| Template editor: full integration or link? | **Static file link** (`/template-editor`). The prototype is self-contained HTML — no backend needed. A "Build a Template" button in the demo nav is sufficient for demo purposes. |

**Regression tests (3 minimum):**
1. `test_benchmark_templates_fire_with_loaded_library`
2. `test_benchmark_templates_skip_without_library`
3. `test_demo_server_loads_benchmark_library_on_startup`

---

### Session 4 (~2-3h): MC7040-MC7044 + polish + acceptance gates

**Goal:** Ship-ready with diagnostic codes, performance verification,
and all acceptance gates green.

**Deliverables:**

1. **MC7040-MC7044 diagnostic codes** in `mc-narrative/src/error.rs`:

   | Code | Condition | Severity |
   |---|---|---|
   | MC7040 | Benchmark library schema version mismatch | Warning |
   | MC7041 | `benchmark_*()` references metric not in library | Warning (template skips) |
   | MC7042 | Benchmark library is stale (ledger has newer entries) | Info |
   | MC7043 | `build-benchmarks` ran with fewer than 2 periods | Info |
   | MC7044 | Benchmark library write failed (disk full, permissions) | Error |

   MC7041 and MC7042 are emitted during evaluation (warn once per
   evaluation run, not once per template). Use the same
   warn-once-per-evaluation pattern as MC7031 (lookback exceeds
   depth).

2. **Staleness check for MC7042:**

   In `read_benchmark_library`, after loading: read the ledger and
   find its latest `report_period`. If the ledger's latest period
   is later than the library's `period_range.to`, emit MC7042.

   ```
   [warn MC7042] Benchmark library may be stale: ledger has entries up to
   2026-05, library covers through 2026-04. Run `mc model build-benchmarks`
   to update.
   ```

3. **Performance check:** benchmark lookup must be < 1ms per function
   call. The `BenchmarkIndex` is a HashMap; lookup is O(1). Verify
   with a test against a library with 1000 metric entries.

4. **Pre-flight code sweep:**
   ```bash
   for code in MC7040 MC7041 MC7042 MC7043 MC7044; do
     grep -rn "$code" crates/ | wc -l
   done
   ```
   All counts should be ≥ 1 after implementation (codes are shipped,
   not just declared).

5. **`show-benchmarks` text output format:**
   ```
   Benchmark Library — scotts-rv
   Built: 2026-05-07  |  Periods: 2025-11 → 2026-04  |  6 periods

   CTR (Targeted Display)          6 samples
     p10=0.08%  p25=0.12%  p50=0.18%  p75=0.24%  p90=0.31%
     mean=0.19%  stddev=0.07%

   Impressions (Targeted Display)  6 samples
     p10=28,450  p25=41,200  p50=52,430  p75=67,800  p90=84,100
     mean=54,320  stddev=15,400
   ```

**Regression tests (3 minimum):**
1. `test_mc7042_stale_library_warning`
2. `test_mc7041_missing_metric_warning`
3. `test_show_benchmarks_text_output_format`

---

## Hard Rules (binding)

1. **`mc-core`, `mc-model`, `mc-fixtures`, `mc-recipe`, `mc-drivers`, `mc-tessera` all locked.**
2. **`BenchmarkLibrary` and `BenchmarkIndex` live in `mc-narrative/src/benchmark.rs`** (new module). Benchmark functions live in `evaluator.rs` alongside ledger functions.
3. **`evaluate_all` gains `benchmark: Option<&BenchmarkLibrary>`.** All existing callers pass `None`. Zero behavior change when no library is present.
4. **No cross-workspace data.** The benchmark library is built from the workspace's own ledger only. No network I/O. No data leaves the workspace.
5. **No minimum sample suppression.** A benchmark with 1 sample is valid (with `sample_count: 1`). Templates guard with `benchmark_sample_count('X') >= N` if they need statistical confidence.
6. **Atomic writes** (tmp + rename) for both the ledger (Phase 7A.2 pattern) and the benchmark library.
7. **Per-session commits (Rule 11).** 4 commits minimum.

---

## Acceptance Gates (lean)

- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (983 → expect ~+16 = ~999).
- [ ] `mc model build-benchmarks` with the Scotts RV ledger produces a valid `.mosaic/benchmark-library.json`.
- [ ] `benchmark_p50('CTR')` returns the correct value in a template evaluation with a loaded library.
- [ ] A benchmark template (`ctr_above_own_median`) fires when CTR is above the workspace's p50.
- [ ] Benchmark templates silently skip when no library is present.
- [ ] `mc model show-benchmarks` displays the library in text and JSON formats.
- [ ] MC7040-MC7044 codes swept FREE before implementation, then shipped.
- [ ] Demo server loads and logs benchmark library on startup.
- [ ] Template editor accessible at `/template-editor` in the demo.
- [ ] Locked surfaces: zero diff.

---

## SPEC QUESTION candidates

- Session 1: When grouping evidence values for benchmark computation,
  should the same metric appear once per ledger ENTRY or once per
  ledger ENTRY × PERIOD? If a workspace uploads 3 months and runs
  `narrate --save-ledger` twice per month, should the benchmark see
  6 samples or 12? (PM default: deduplicate by `report_period` within
  a (metric, scope_key) group — latest entry per period wins.)

- Session 2: `benchmark_above_median('CTR')` — does it compare
  `current.CTR` or `campaign_avg.CTR` to the p50? Current period vs.
  campaign-total is meaningful for monthly-performance templates vs.
  campaign-performance templates. (PM default: `current.CTR` for
  monthly templates; the template author chooses by writing
  `benchmark_percentile('CTR', campaign_avg.CTR)` if they want
  campaign-total.)

- Session 3: Should `narrate` + `narrate-trends` reload the benchmark
  library from disk on every call, or cache it in memory for the
  server? (PM default: reload from disk each evaluation call in the
  CLI; the demo server loads on startup and only reloads if the file's
  mtime changed — same pattern as template loading.)

---

## Completion context

After 7A.4 ships, the Phase 7A arc is complete:

| Phase | Capability |
|---|---|
| 7A.1 | Narrative engine: YAML templates + formula evaluation → structured narratives |
| 7A.2 | Interpretation ledger: every narrative event persisted to JSONL |
| 7A.3 | Cross-period analysis: "third consecutive month" from ledger streaks |
| 7A.4 | Own-workspace benchmarks: "below your historical median" from percentile library |

The narrative engine at this point can say:

> "Paid Search CTR of 0.09% is in the bottom quartile of your historical
> performance (your typical range: 0.12%–0.31%), and this is the second
> consecutive month below your p25. The trend is worsening."

That sentence requires 7A.1 (template evaluation) + 7A.2 (ledger storage)
+ 7A.3 (streak detection) + 7A.4 (percentile benchmarks) — all four phases
working together. No LLM looks at the data. Deterministic, from structured
evidence, using only the workspace's own history.

---

*End of handoff. Phase 7A.4 is the last piece of the institutional memory
arc. After this ships, each workspace has a mirror to its own performance
history — not compared against other customers, not stored on any server,
just the workspace's own ledger turned into context that makes every
narrative more meaningful.*
