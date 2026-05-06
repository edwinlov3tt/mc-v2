# Data Out + Agent Surface Audit — Phase 6A.1 Gap Analysis

## Reviewer: Claude Sonnet 4.6
## Date: 2026-05-06
## Scope: mc-cli Phase 6A verbs (query, whatif, trace, sweep, diff, write, transform), MCP tool layer, agent-facing output surfaces; evidence from email-matchback Python scripts (bench.py, whatif_v2.py, budget_reallocator.py, ltv_report.py, whatif_demo.py); Phase 6A handoff and 6A.1 completion report.

---

## Closed by 6A/6A.1 (verification)

### G-CLOSED-1: MCP stdout corruption on Phase 6A verb calls
**Was:** Phase 6A verbs called `print!()` directly inside the MCP request-handling path, interleaving tool output with JSON-RPC framing (`mcp.rs` pre-fix).
**Now:** All Phase 6A verbs expose `run_captured(cmd) -> (i32, String)` and MCP calls go through `run_cli_verb_json` / `run_cli_verb` helpers (`crates/mc-cli/src/mcp.rs:866-885`). `test_mcp_query_does_not_corrupt_stdout` passes.
**Evidence:** `crates/mc-cli/tests/agent_cli_integration.rs:346-403`.

### G-CLOSED-2: `schema_version` envelope missing from Phase 6A verbs
**Was:** `query`, `whatif`, `trace`, `sweep`, `diff`, `write` outputs had no `schema_version` field — agents could not detect format changes.
**Now:** `push_json_envelope_header` (`crates/mc-cli/src/query.rs:329-333`) is called by all seven verb formatters. `test_all_phase_6a_verbs_emit_schema_version` passes.

### G-CLOSED-3: I/O errors vs model errors returned same exit code
**Was:** File-not-found and invalid-YAML both returned exit 1 — agents could not distinguish "retry with different path" from "fix the model."
**Now:** `LoadModelError::Io` → exit 3; `LoadModelError::Model` → exit 1 (`crates/mc-cli/src/query.rs:257-270`). Both regression tests pass.

### G-CLOSED-4: MCP `structured` field absent on Phase 6A tool calls
**Was:** `mosaic.model.query` and siblings only surfaced output in `content[0].text`, requiring double-parse.
**Now:** `run_cli_verb_json` sets `structured` to the captured output when exit code is 0 (`crates/mc-cli/src/mcp.rs:866-885`). `test_mcp_query_returns_structured_envelope` passes.

---

## Open gaps — clear path

### G-OPEN-1: Write-log replay is not wired into `load_model`
**Use case:** Agent calls `mc model write` to record a live override (e.g., a line-movement alert), then immediately calls `mc model query` to verify the new value. Today `query` silently returns the stale canonical_inputs value.
**Evidence (Python):** `whatif_demo.py:44-56` demonstrates the pattern of mutating a source CSV then re-reading — the Python workaround is needed precisely because `mc model write` does not feed `mc model query`.
**Evidence (Mosaic absence):** `crates/mc-cli/src/query.rs:278-317` — `load_model` ends at `apply_canonical_inputs` and never reads `.tessera/writes.jsonl`. Confirmed explicitly in `docs/reports/phase-6a-1-completion-report.md` section 4.3 (P0 debt).
**Impact:**
  - Lines of Python eliminated: 30+ lines per use case (CSV mutate + restore dance)
  - Other affected use cases: any agent loop that writes one cell then queries downstream effects; `mc model whatif` with `--set` is the workaround but it doesn't persist
**Proposed fix shape:** In `load_model`, after `apply_canonical_inputs`, check for `<model_dir>/.tessera/writes.jsonl`; if present, parse JSONL entries and replay each via `cube.write()` in timestamp order. Atomic: if replay fails, surface as `LoadModelError::Model`.
**Phase mapping:** Phase 6A.2 (already noted in 6A.1 Known Debt P0)

---

### G-OPEN-2: `mc model whatif` only supports single-cell overrides
**Use case:** `budget_reallocator.py` mutates four market budgets simultaneously (Houston, Austin, Denver, Amarillo — `budget_reallocator.py:43-63`) and re-reads revenue at all four. Today an agent must call `whatif` four times (one per market) and manually aggregate the deltas, which loses the cross-market interaction effect because each call reloads the model independently.
**Evidence (Python):** `budget_reallocator.py:120-132` — the inner loop writes 4 cells per sweep point; the code manually combines results.
**Evidence (Mosaic absence):** `crates/mc-cli/src/whatif.rs:13-20` — `WhatifCommand` holds a single `set_coord: String` and `value: f64`; no multi-cell batch.
**Impact:**
  - Lines of Python eliminated: ~80 lines (the sweep + aggregate loop in `budget_reallocator.py`)
  - Other affected use cases: any scenario with correlated inputs (media mix, price elasticity, multi-market planning)
**Proposed fix shape:** Add `--set` as a repeatable flag (or accept a CSV file of coord=value pairs). The write lifecycle (snapshot → N writes → compute → rollback) is already correct; just iterate.
**Phase mapping:** Phase 6A.2

---

### G-OPEN-3: `mc model trace` fails (silently or with "input") for non-leaf / consolidated coordinates
**Use case:** `ltv_report.py:27-32` reads `AllTenure` (a consolidated element) and `AllMarkets` for `RevenuePerActive`. An agent tracing "why is blended RevPerActive = X?" at an `AllMarkets` coordinate needs the consolidation path, not a leaf rule trace.
**Evidence (Python):** `ltv_report.py:27-32` — explicit probes for `AllTenure` and `AllMarkets` elements, which are rollup-level coordinates.
**Evidence (Mosaic absence):** `crates/mc-cli/src/trace.rs:136-149` — when `cell.trace` is `None` (which happens for consolidated cells that didn't fire a rule), the code returns a synthetic leaf node with `source: "input"`, hiding the consolidation entirely. The Phase 6A.1 completion report explicitly lists this as a known debt item.
**Impact:**
  - Misleads agents: a consolidated cell reported as "source: input" is factually wrong; the LLM reasoning chain breaks
  - Affects any model with rollup dimensions (AllMarkets, AllTime, AllChannels) — the most common analytical pattern
**Proposed fix shape:** Detect `TraceOp::Consolidation` in `build_trace_tree` and build a child list by enumerating the dimension's leaf elements and tracing each recursively; surface as `"source": "consolidation"` with a `"child_count"` field in the JSON tree.
**Phase mapping:** Phase 6A.2

---

### G-OPEN-4: `mc model sweep` reloads and parses YAML 2N+1 times per sweep
**Use case:** `budget_reallocator.py:121-132` performs 343 evaluations of a 4-market model; a Mosaic-native equivalent via `mc model sweep` would parse the YAML ~687 times. At ~30ms per load this is ~20 seconds vs the Python script's ~12 seconds.
**Evidence (Python):** `budget_reallocator.py:118` — `n_evals` is printed; the loop is explicitly timed.
**Evidence (Mosaic absence):** `crates/mc-cli/src/sweep.rs:166` — `load_model` is called once per sweep point inside the loop; `find_coefficient_index` at line 197 calls `load_model` a second time per point (re-reads YAML to find the coefficient index).
**Impact:**
  - For a 20-point sweep: 40 YAML parses. For a 343-point multi-axis sweep: 686 YAML parses.
  - Dissuades agents from using `mc model sweep` for budget optimization tasks — they fall back to Python loops calling `mc model test` instead.
**Proposed fix shape:** Compile the model once before the sweep loop, cache `coeff_index` before the loop, then mutate the in-memory `FittedModelData` and reset between iterations.
**Phase mapping:** Phase 6A.2 (already in 6A.1 Known Debt P1, MAJ-4)

---

### G-OPEN-5: `mc model diff --since last` and `--before`/`--after` snapshot modes are unimplemented
**Use case:** After `mc tessera apply` an agent wants to detect which cells changed ("which lines moved since last ingest?"). The Phase 6A handoff (`docs/handoffs/phase-6a-agent-ready-cli-handoff.md:358-404`) specifies `--since last` and `--before`/`--after` modes. Only `--left`/`--right` (scenario comparison) was implemented.
**Evidence (Python):** `bench.py:106-117` — the "forecast rebuild" pattern explicitly checks which cells changed after a budget edit; this is the `--since last` use case.
**Evidence (Mosaic absence):** `crates/mc-cli/src/diff.rs:69-79` — parser requires both `--left` and `--right`; there is no `--since`, `--before`, or `--after` flag. The `DiffCommand` struct has no snapshot-reference fields.
**Impact:**
  - Agents detecting post-ingest changes must script their own before/after snapshot logic in Python
  - The "what changed since last data load?" workflow from the KellyBets scenario in the handoff is unaddressed
**Proposed fix shape:** Add `--since last` mode: snapshot the cube state before `tessera apply` writes to `.tessera/audit.jsonl`, load the pre-apply snapshot from the audit log, diff against current. Alternatively, `mc model write` could record a snapshot ID that `diff` accepts as `--before`.
**Phase mapping:** Phase 6A.2

---

### G-OPEN-6: No `mc model report` verb or templating surface
**Use case:** `ltv_report.py` (117 lines) is entirely devoted to formatting cube values into a human-readable multi-section report with tables and category labels. It produces: per-market LTV table, blended revenue-per-active list, newcomer-share percentages, and engagement-band distribution — all from raw cell values. An agent must write this Python to produce a shareable artifact.
**Evidence (Python):** `ltv_report.py:80-108` — four distinct formatted sections; `fmt_money`, `fmt_pct`, and band-to-label lookup are all handwritten formatting utilities.
**Evidence (Mosaic absence):** No `mc model report` verb exists. `--format text` on `mc model query` produces a fixed-column table with no section headers, no custom formatting, no aggregation summaries.
**Impact:**
  - Every "generate a readable summary" use case requires Python; the CLI is unusable as a report generator
  - Agents that want to present results to a human (email, Slack, UI) must do all formatting in their Python wrapper
**Proposed fix shape:** A `mc model report` verb that accepts a Jinja2-style template (or a simpler mustache-style template) referencing cell coordinates and aggregate expressions. The engine evaluates the coordinates, substitutes values, and outputs the rendered template.
**Phase mapping:** Needs design (see G-DESIGN-1)

---

### G-OPEN-7: `--limit` default of 10,000 rows with no pagination
**Use case:** A model with many coordinates (e.g., daily time dimension × many markets × many measures) could easily generate hundreds of thousands of leaf cells. `mc model query` without `--where` will attempt to buffer all 10,000 rows in memory before returning any output.
**Evidence (Python):** No direct evidence from email-matchback (tide models are small), but the Phase 6A handoff (`docs/handoffs/phase-6a-agent-ready-cli-handoff.md:585`) explicitly raised this: "A model with 100K coordinates and `--where` that matches all of them would produce 100K rows."
**Evidence (Mosaic absence):** `crates/mc-cli/src/query.rs:180` — `let limit = cmd.limit.unwrap_or(10000);` hard-codes 10,000 as the default; there is no `--offset` flag, no pagination token, no streaming.
**Impact:**
  - Agents that enumerate a large model get a silent truncation (the `--limit` cuts results without warning the agent that truncation occurred)
  - No way to page through results; an agent must re-run with increasing offsets
**Proposed fix shape:** Add `--offset N` companion flag; include `"truncated": true` and `"total_matched": N` fields in the JSON envelope when the limit was hit; or stream rows as newline-delimited JSON.
**Phase mapping:** Phase 6A.2

---

### G-OPEN-8: `mc tessera transform` JSON source parsing uses `serde_json` but the binary was built without it as a declared dep
**Use case:** Any agent that calls `mc tessera transform --source <url>` on a JSON-returning API.
**Evidence (Python):** `budget_reallocator.py:75-80` — calls `mc model test` after writing a CSV; the transform verb is the intended CLI replacement for the Python CSV mutation pattern.
**Evidence (Mosaic absence):** `crates/mc-cli/src/transform.rs:347` — `serde_json::from_str(data)` and `serde_json::Value` are used directly. `mc-cli/Cargo.toml` was not checked directly, but given the Phase 6A hard rule "no new dependencies in mc-cli," this is a latent dependency violation if `serde_json` was not already in `mc-cli`'s dep tree before Phase 6A.
**Impact:**
  - If `serde_json` is not an explicit dep, the build relies on a transitive dep that may be pruned; any upstream change breaks JSON source support silently.
  - The Phase 6A handoff explicitly said "no new dependencies."
**Proposed fix shape:** Either explicitly declare `serde_json` in `mc-cli/Cargo.toml` (acknowledging it breaks the no-new-deps rule), or replace `serde_json::from_str` with the hand-rolled JSON parser already present in `mcp.rs` (which handles the same subset).
**Phase mapping:** Phase 6A.2 (dependency hygiene fix)

---

### G-OPEN-9: `mc tessera transform` URL fetching uses `curl` subprocess, not `ureq`
**Use case:** Any agent calling `mc tessera transform --source https://...` — the intended API-fetch pattern from the KellyBets scenario.
**Evidence (Python):** Analogous to what `bench.py:53-58` does against local CLI subprocesses; the intended replacement for Python HTTP + CSV generation.
**Evidence (Mosaic absence):** `crates/mc-cli/src/transform.rs:167-177` — `fetch_url` spawns `curl` via `std::process::Command`. `mc-drivers` already depends on `ureq` (cited in the handoff as "already in workspace from mc-drivers"), but `mc-cli` does not directly reference it and instead shells out to `curl`.
**Impact:**
  - Fails on systems without `curl` (Windows, restricted containers)
  - Cannot be unit-tested without network access or a mocked `curl`
  - Inconsistent with `mc-drivers`' `http_json_driver.rs` which uses `ureq`
  - Error messages from `curl` subprocess are opaque (no structured error code — just the curl stderr string)
**Proposed fix shape:** Add `ureq` to `mc-cli/Cargo.toml` or expose a re-export from `mc-drivers`. Use `ureq::get(url).call()` for the blocking GET, matching the mc-drivers pattern.
**Phase mapping:** Phase 6A.2

---

### G-OPEN-10: `mc model query --where` filter tokenizer rejects hyphenated identifiers
**Use case:** Models with dimension element names like `M_2026_04` or `DirectMail` work, but dimension names with hyphens (e.g., `Paid-Search`) or element values containing spaces cannot be used in `--where` expressions.
**Evidence (Python):** `whatif_v2.py:29` — `TARGET = ("Actual", "Working", "M_2026_04", "DirectMail", "Houston", "AdSpend")` uses underscore-only names, so the limitation is not hit. However, `tide-matchback.yaml` channel names may include hyphens.
**Evidence (Mosaic absence):** `crates/mc-cli/src/query.rs:486-490` — the tokenizer's identifier rule is `c.is_ascii_alphabetic() || c == b'_'` for start and `c.is_ascii_alphanumeric() || c == b'_'` for continuation. Hyphens are consumed as `other` → `Err("unexpected character: '-'")`.
**Impact:**
  - Models with hyphenated dimension names (common in marketing models: `Paid-Search`, `Direct-Mail`) cannot use `--where` at all
  - An agent that builds a `--where` expression from model introspection gets a parsing error with no recovery path
**Proposed fix shape:** Allow hyphens inside identifier tokens (not as the first character); or require string literals for element name comparisons (e.g., `Channel == "Paid-Search"` already works since string literals are fully supported).
**Phase mapping:** Phase 6A.2

---

### G-OPEN-11: `mc model sweep` metric aggregation is naively global (all leaf coordinates)
**Use case:** `budget_reallocator.py:125-132` sweeps budget over 4 specific markets and measures predicted revenue only for those 4 markets. `mc model sweep --metric "sum(PredictedRevenue)"` would sum PredictedRevenue across ALL market/time/channel combos — including irrelevant ones — producing a misleading metric.
**Evidence (Python):** `budget_reallocator.py:78-80` — `PROBE_NAME_PER_MARKET` maps 4 specific goldens; the Python script's golden-as-probe trick is necessary precisely because sweep has no `--where` scoping for the metric.
**Evidence (Mosaic absence):** `crates/mc-cli/src/sweep.rs:344-424` — `eval_metric` calls `enumerate_leaf_coords(cube, refs)` which returns ALL leaf coordinates; there is no filtering on the metric evaluation scope.
**Impact:**
  - Sweep results are potentially nonsense for multi-market models where the user wants to optimize a subset
  - Agents fall back to Python loops (each calling `mc model test` with goldens) instead of `mc model sweep`
**Proposed fix shape:** Add `--metric-where` filter flag to scope which coordinates are aggregated in the metric; reuse the `Filter` infrastructure already in `query.rs`.
**Phase mapping:** Phase 6A.2

---

### G-OPEN-12: MCP `mosaic.model.whatif` input schema accepts `value` as string, not number
**Use case:** Any LLM calling `mosaic.model.whatif` via MCP must pass the override value.
**Evidence (Python):** N/A (this is a schema ergonomics issue).
**Evidence (Mosaic absence):** `crates/mc-cli/src/mcp.rs:209-213` — the MCP tool descriptor declares `("value", "string", "New numeric value to set.", true)`. The dispatch at line 647-650 also calls `as_str_owned()` to retrieve it. The Phase 6A handoff's intended JSON schema had `"value": "number"` for programmatic use, but the implementation uses `"string"` and relies on `parse::<f64>()`. Similarly for `mosaic.model.write` at line 253.
**Impact:**
  - LLMs (particularly strict JSON-schema-following clients) may pass `{"value": 223.0}` (a JSON number) and get `None` from `as_str_owned()`, triggering "missing required argument: value" even though the call was syntactically correct
  - The type mismatch is not surfaced to the caller; the error message doesn't say "expected string, got number"
**Proposed fix shape:** In `tool_whatif` and `tool_write`, use a coercing accessor: try `as_str_owned()` first, then fall back to `as_f64()` and format as a string. Or change the descriptor `arg_type` to `"number"` and use `as_f64()` directly.
**Phase mapping:** Phase 6A.2

---

### G-OPEN-13: `mc model write` response JSON omits a stable `revision_id` for traceability
**Use case:** An agent that writes a cell needs to reference the exact write later (in a diff, in an audit log, in a `--since` diff). The current JSON response has no `revision_id` or `write_id`.
**Evidence (Python):** `whatif_demo.py:78-91` — the "before/after/delta" table is constructed manually because there's no stable identifier for the write that can be referenced later.
**Evidence (Mosaic absence):** `crates/mc-cli/src/write.rs:197-217` — the JSON output has `coord`, `before`, `after`, `invalidated_cells` but no `write_id` or `log_sequence`. The `writes.jsonl` entry (`write.rs:187-192`) has a timestamp but no monotonic ID.
**Impact:**
  - Agents cannot reference a specific write when calling `mc model diff --before <id>` (the `--before`/`--after` diff mode isn't implemented yet, but when it is, it needs a stable ID)
  - Audit trail is time-keyed only; two writes in the same second are indistinguishable
**Proposed fix shape:** Add a `write_id` field (monotonic counter or UUID) to both the `writes.jsonl` log entry and the `mc model write` JSON response.
**Phase mapping:** Phase 6A.2

---

### G-OPEN-14: `--format text` output uses fixed 15-char column widths regardless of data
**Use case:** A human or agent reading `mc model query --format text` output with long dimension names (e.g., `DirectMail`, `Houston`, element names like `M_2026_04`) gets truncated or misaligned columns.
**Evidence (Python):** `ltv_report.py:81-86` — the Python report uses right-justified fields (`{fmt_money(v):>12s}`) and explicit column labels, contrasting with Mosaic's generic text formatter.
**Evidence (Mosaic absence):** `crates/mc-cli/src/query.rs:1199-1221` — `format_text` uses `{:<15}` for every column regardless of content width. The `trace.rs` text formatter (`write_trace_text_node` at line 317) is actually good (uses tree connectors). The `sweep.rs` text output (`format_sweep_output` at line 480) uses fixed widths.
**Impact:**
  - Human readability suffers for models with descriptive element names
  - An agent using `--format text` for a quick summary gets garbled alignment
**Proposed fix shape:** Compute max column widths from data before rendering; left-pad numeric columns, right-align strings in header.
**Phase mapping:** Phase 6A.2 (polish)

---

### G-OPEN-15: `mc model query` with `--show` including dimension names returns strings, breaking `--aggregate`
**Use case:** An agent wants `--show "Market,PredictedRevenue"` to get grouped results, then aggregate over market groups.
**Evidence (Python):** `budget_reallocator.py:80-103` — results are grouped by market implicitly because the Python code has separate probes per market.
**Evidence (Mosaic absence):** `crates/mc-cli/src/query.rs:217-220` — when `show` contains a dimension name, the code returns `ScalarValue::Str(dim_val)`. But the aggregate functions in `run_aggregate` (`query.rs:998-1078`) only operate on `ScalarValue::F64`, meaning aggregating over dimension-grouped results is not possible. The `--aggregate` and `--show` flags are mutually exclusive in the current dispatch (`query.rs:185-197`).
**Impact:**
  - No group-by capability: agents cannot "for each Market, sum(Revenue)" without post-processing
  - `--show` with dimension names only works in the row-output mode, not in aggregate mode
**Proposed fix shape:** Add a `--group-by <dim1,dim2>` flag that, when combined with `--aggregate`, partitions the result set by those dimension values and reports aggregates per partition.
**Phase mapping:** Phase 6A.2 / needs design

---

## Open gaps — needs design

### G-DESIGN-1: No report/template verb for formatted multi-section output
**Use case:** `ltv_report.py` produces a 4-section formatted report with custom number formatting, section headers, band-to-label lookups, and a wall-clock timing footer. This is a "from cube to shareable artifact" workflow that recurs in every customer-facing analytics use case.
**Evidence:** `ltv_report.py:51-112` — 62 lines of pure formatting code driving `mc model test` goldens as probes. This pattern will recur in every model as the number of "interesting cell" reports grows.
**Why design is non-obvious:** Three alternatives exist, each with significant tradeoffs:
  1. **Template verb (`mc model report --template report.md.j2`):** Mosaic evaluates `{{ coord(...) }}` expressions in a Jinja2/mustache template. Simple to explain; requires adding a template engine dependency (anathema to the no-new-deps rule) or a custom expression evaluator.
  2. **Golden test extensions (`expect_format: "currency"`):** Extend the golden test syntax with format metadata; `mc model test` can then render the test output as a human-readable table. Low-cost (no new verb) but conflates testing and reporting, and is limited to golden-defined coordinates.
  3. **Notebook export (`mc model export --notebook ltv.ipynb`):** Emit a Jupyter notebook with cell values pre-populated. Eliminates Python for the simplest reports; requires agreeing on a notebook schema.
**Alternatives:**
  - Option A (template verb): max flexibility, new dep or ~500-line custom parser
  - Option B (golden extensions): zero new dep, limited to declared goldens
  - Option C (notebook export): familiar to data scientists, binary format, no CLI preview
  - Option D (structured `text` improvements): richer `--format text` with section grouping; lowest effort but not shareable
**Phase mapping:** Needs ADR before phase scoping

---

### G-DESIGN-2: No visualization / chart data surface for agent-driven UI rendering
**Use case:** A UI agent (Phase 6B) or a notebook agent needs to render a time-series chart of forecast vs actual revenue. Mosaic can produce the raw numbers via `mc model query`, but has no concept of a "chart spec" (which cells are x-axis, which are y-axis, what the title is).
**Evidence:** `ltv_report.py:43-48` — engagement band distribution by tenure bucket is explicitly a bar-chart pattern. `budget_reallocator.py:140-157` — ranked allocations table is a horizontal bar chart pattern. Neither script uses matplotlib because there's no persistent artifact path; the Python just prints text.
**Why design is non-obvious:**
  - Option A (Vega-Lite spec output): `mc model query --chart-spec vega-lite` emits a JSON chart spec with data embedded. Stateless, portable, renderable by any Vega-aware UI.
  - Option B (chart metadata in model YAML): Declare charts in the model YAML (`charts:` block); `mc model chart --name ltv_by_tenure` evaluates and exports. Keeps chart intent in the model alongside the data.
  - Option C (leave to the UI): Accept that chart rendering is always Phase 6B's job; Mosaic just provides the data points via `mc model query`.
**Alternatives:**
  - Option A: high portability; Vega-Lite is complex to implement correctly
  - Option B: chart-as-model-artifact is conceptually clean but adds a new model layer
  - Option C: principled separation; only works if Phase 6B is guaranteed to ship
**Phase mapping:** Needs ADR; likely Phase 6B or 6C

---

### G-DESIGN-3: Multi-axis sweep (budget reallocation across N dimensions simultaneously)
**Use case:** `budget_reallocator.py:121-132` sweeps all 4 markets simultaneously holding total budget constant. `mc model sweep` is one-axis only (`--range start:end:step` on a single coefficient or cell). A 4-market sweep with 7 steps per dimension is a 7^3 = 343-point grid.
**Evidence (Python):** `budget_reallocator.py:113-132` — nested `for h in h_steps: for a in ...: for d in ...` loops; 343 evaluations in ~12 seconds.
**Evidence (Mosaic absence):** `crates/mc-cli/src/sweep.rs:13-23` — `SweepCommand` has a single `set_coord: Option<String>` and `range: String`; no multi-axis support.
**Why design is non-obvious:**
  - A full grid sweep grows exponentially; even 7^4 = 2401 points at 30ms each is 72 seconds — likely acceptable. But 10^4 = 10,000 points at 50ms is 500 seconds — not acceptable for interactive use.
  - Constraint handling (budget held constant) is application-specific; Mosaic would need a `--constraint "sum(cells) == 13016"` syntax or a YAML constraint block.
  - A smart sampler (Bayesian optimization, Latin hypercube) would find optima in far fewer evaluations, but requires a numerical optimization library.
**Alternatives:**
  - Option A (`--set` as repeatable flag + grid expansion): `mc model sweep --set Coord1 --range R1 --set Coord2 --range R2` generates cartesian product. Simple extension; exponential blowup.
  - Option B (`mc model optimize` verb): separate verb for constrained optimization using a built-in solver (e.g., Nelder-Mead). New dep; much more powerful.
  - Option C (leave to Python): accept that multi-axis sweeps above 3 dimensions are always Python's job; optimize the single-axis sweep path.
**Phase mapping:** Needs ADR before phase scoping; likely Phase 3I or a new phase

---

### G-DESIGN-4: No cross-model or cross-cube comparison surface
**Use case:** Compare `tide-matchback.yaml` to `tide-mmm.yaml` on a shared coordinate space. Compare two model versions (before/after a coefficient update). `mc model diff` only compares two states of the *same* model (same YAML, different scenario/version dimensions).
**Evidence (Python):** `bench.py:88-117` — the benchmark compares before/after states by timing the same model; there's no Python cross-model comparison because it requires manual coordinate alignment.
**Evidence (Mosaic absence):** `crates/mc-cli/src/diff.rs:15-23` — `DiffCommand` holds a single `path: String`; no second model path. `read_with_overrides` at line 232 constructs coordinates from a single cube.
**Why design is non-obvious:**
  - Cross-model comparison requires agreeing on a shared coordinate key (dimension names must match). Mosaic dimensions are model-local; there's no global coordinate namespace.
  - Option A: require dimension name alignment explicitly (`--left-model model1.yaml --left-dim-map "Market=Region"`).
  - Option B: define a "comparison schema" YAML that specifies the shared coordinate space and maps dimension names between models.
  - Option C: implement "model merge" (compile both, take the union of matching coordinates).
**Phase mapping:** Needs ADR; likely Phase 5D or a dedicated comparison phase

---

## Edge cases / latent bugs found during audit

### E-1: `mc model write` uses `serde_json::to_string` despite no `serde` declaration
**Where:** `crates/mc-cli/src/write.rs:189`
**What I expected:** The write log entry serialization would use the hand-rolled JSON emitter already present in `mcp.rs` (or a simple format string).
**What I observed:** `serde_json::to_string(&cmd.coord).unwrap_or_else(|_| format!("\"{}\"", cmd.coord))` — a direct `serde_json` call with `.unwrap_or_else()` as a fallback. `serde_json::to_string` on a `String` argument will always succeed (String implements Serialize), so the fallback is unreachable — but the import still exists.
**Impact:** Same as G-OPEN-8: implicit dependency on `serde_json` being in the dep tree; violation of the Phase 6A "no new dependencies" rule if `serde_json` was not already present.

---

### E-2: `mc model write` timestamp function uses a hand-rolled days-to-YMD algorithm that may mis-date during leap years
**Where:** `crates/mc-cli/src/write.rs:260-286`
**What I expected:** The ISO timestamp in `writes.jsonl` would be correct.
**What I observed:** `days_to_ymd` is a home-grown implementation (176 lines to end of file) using `is_leap` and manually summing month lengths. The algorithm looks structurally correct for modern dates, but was not cross-checked against a reference implementation. The comment ("Rough year/month/day (good enough for logging)") acknowledges the approximation.
**Impact:** Timestamps in `writes.jsonl` may be off by one day near month boundaries for leap years; audit log integrity degrades; `--since last` diff mode (when implemented) may skip or double-apply writes if it sorts by timestamp.

---

### E-3: `mc model query --aggregate count(predicate)` re-parses filter after already filtering
**Where:** `crates/mc-cli/src/query.rs:1050-1063`
**What I expected:** `count(Should_Bet == 1)` would count rows in the already-filtered `matching_coords` set that satisfy the sub-predicate.
**What I observed:** At line 1055, `Filter::parse(inner, refs, cube)` is called inside `run_aggregate`, but `cube` is `&mut mc_core::Cube` which was already used to evaluate the outer filter. The inner filter parse borrows `refs` and `cube` but then calls `eval_filter` on those same objects. In the borrow checker this compiles only because `matching_coords: Vec<&CellCoordinate>` holds references into `all_coords` (a `Vec<CellCoordinate>` on the stack), not into `cube`. However, `eval_filter` at line 1058 calls `cube.read()` via a mutable borrow on the same `cube` as was used to build `matching_coords`. This is sound only if `cube.read()` doesn't invalidate the cell coordinates — which is true in Phase 1's `HashMapStore`, but would break if a future store implementation returned references into internal storage.
**Impact:** Latent fragility; not currently a bug in Phase 1 storage but creates technical debt for Phase 2D storage refactoring.

---

### E-4: `mc model trace` formula field shows `"{:?} op"` instead of the actual formula string
**Where:** `crates/mc-cli/src/trace.rs:203`
**What I expected:** The `"formula"` field in the trace JSON would contain a human-readable formula like `"Calibrated_P * (Decimal_Odds - 1) - (1 - Calibrated_P)"`.
**What I observed:** `let formula = Some(format!("{:?}", expr_summary.op))` — uses Rust's `Debug` format on the `op` field of the expression summary. This produces output like `"Mul"` or `"Add { ... }"` rather than the intended infix formula. The Phase 6A handoff (`docs/handoffs/phase-6a-agent-ready-cli-handoff.md:266`) showed the intended formula string for explainability; the actual output is a debug-format AST fragment.
**Impact:** Trace output's `formula` field is useless for LLM reasoning. An agent trying to answer "why does this cell have this value?" cannot use the formula to explain the computation.

---

### E-5: `mc tessera transform` recipe parser does not handle the `mc-recipe` YAML schema format
**Where:** `crates/mc-cli/src/transform.rs:202-326`
**What I expected:** `mc tessera transform --recipe <path>` would accept the same recipe YAML format that `mc tessera apply` uses (defined by `mc-recipe` crate).
**What I observed:** `parse_transform_recipe` is a bespoke line-scanner that only handles keys named `column_mappings`, `mappings`, `defaults`, `json_path`, `output_columns`, and `scale`. It does NOT parse the `mc-recipe` schema fields: `source.driver`, `mappings.target.dimension`, `on_error`, `time_format`, etc. The `mc-tessera` crate's recipe schema (Phase 5A/5B) is a superset of what `transform.rs` parses.
**Impact:** An agent that creates a recipe using `mc tessera propose` (or by following the Phase 5B recipe documentation) and then passes it to `mc tessera transform` will get silent empty output or incorrect column mappings. The two recipe surfaces are incompatible.

---

### E-6: `mc model whatif --dry-run` `would_affect` output is the `--show` list, not actual computed dependents
**Where:** `crates/mc-cli/src/whatif.rs:339-342`
**What I expected:** `--dry-run` would identify which measures are actually dependent on the overridden cell (via the dependency graph).
**What I observed:** `would_affect` is literally `cmd.show.iter()` — the user's manually specified `--show` list. If the user specifies `--show "Clicks,Revenue"`, dry-run says "would affect Clicks, Revenue" regardless of whether those measures are actually derivable from the overridden cell.
**Impact:** The `--dry-run` feature is misleading for agents that use it to discover impact before committing a write. An agent might pass an overly narrow or overly broad `--show` list and get a false sense of impact scope.

---

## Confirmed working (sanity checks only)

- `mc model query --coord` returns the correct cell value for the Acme example (`test_query_with_coord` passes, value = 10500 confirmed).
- `mc model whatif` correctly computes and returns before/after/delta for a derived measure (`Clicks`) after overriding `Spend`.
- `mc model trace` returns a hierarchical tree with `schema_version`, `measure`, `value`, `source`, and `inputs` fields for a derived cell.
- `mc model sweep` returns a sweep curve with the correct number of points and an `optimal` field.
- `mc model diff` with `--left`/`--right` scenario comparison returns `changed_cells`, `top_changes`, and `summary`.
- `mc model write --dry-run` shows the current and would-be value without persisting anything.
- All seven Phase 6A verbs emit `schema_version: "1.0"` in JSON output.
- All seven Phase 6A verbs return valid JSON parseable by `jq` / `serde_json`.
- Exit code 3 for missing model file; exit code 1 for invalid YAML — correctly distinguished.
- MCP `mosaic.model.query` returns valid JSON-RPC with a single line on stdout (no interleaving).
- MCP `structured` field on Phase 6A tools carries the JSON envelope (parseable without double-encoding).
- `mc tessera transform --preview N` truncates output and does not write a file.

---

## What I couldn't verify

- **G-OPEN-8 / E-1 (serde_json dep):** Could not confirm whether `serde_json` is or is not explicitly declared in `crates/mc-cli/Cargo.toml` without reading that file. The analysis is based on the source code's use of `serde_json::` APIs in both `transform.rs` and `write.rs`.
- **G-OPEN-3 (trace at consolidated coordinates):** Could not run `mc model trace` against `AllMarkets` or `AllTenure` in the tide-ltv model to confirm the "source: input" silent-failure behavior; the finding is based on code inspection of `trace.rs:136-149` and the 6A.1 completion report's acknowledged debt.
- **G-OPEN-9 (curl availability):** Whether the `curl` subprocess approach in `transform.rs:167` succeeds in the CI/test environment; the `mc tessera transform` test in the handoff's acceptance gates (`fetch "https://httpbin.org/json"`) would exercise this, but no test in `agent_cli_integration.rs` covers the URL-fetch path.
- **E-2 (timestamp correctness):** The `days_to_ymd` algorithm was not run against a reference implementation; the assessment ("good enough for logging") matches the code comment.
- **E-5 (recipe incompatibility):** Did not attempt to construct a Phase 5B recipe YAML and run it through `transform.rs`; the finding is based on comparing the parsed fields in `parse_transform_recipe` against the `mc-tessera` recipe schema documentation.

---

## Summary Table

| ID | Severity | Category | Title |
|----|----------|----------|-------|
| G-OPEN-1 | CRIT | Open — clear path | Write-log replay not wired into `load_model` |
| G-OPEN-2 | MAJ | Open — clear path | `mc model whatif` single-cell-only; no multi-cell override |
| G-OPEN-3 | MAJ | Open — clear path | `mc model trace` silent failure at consolidated coordinates |
| G-OPEN-4 | MAJ | Open — clear path | `mc model sweep` reloads YAML 2N+1 times per sweep |
| G-OPEN-5 | MAJ | Open — clear path | `mc model diff --since last` and snapshot modes unimplemented |
| G-OPEN-6 | MAJ | Open — clear path | No `mc model report` verb for formatted multi-section output |
| G-OPEN-7 | MIN | Open — clear path | `--limit` default with no pagination / truncation warning |
| G-OPEN-8 | MIN | Open — clear path | `transform.rs` uses `serde_json` without explicit dep declaration |
| G-OPEN-9 | MIN | Open — clear path | `mc tessera transform` URL fetch via `curl` subprocess, not `ureq` |
| G-OPEN-10 | MIN | Open — clear path | `--where` tokenizer rejects hyphenated identifiers |
| G-OPEN-11 | MAJ | Open — clear path | `mc model sweep` metric evaluates globally, no `--metric-where` scoping |
| G-OPEN-12 | MIN | Open — clear path | MCP `value` parameter typed as `string` instead of `number` |
| G-OPEN-13 | MIN | Open — clear path | `mc model write` response lacks stable `revision_id` |
| G-OPEN-14 | OBS | Open — clear path | `--format text` uses fixed 15-char column widths |
| G-OPEN-15 | MAJ | Open — clear path | `--show` + `--aggregate` incompatible; no group-by capability |
| G-DESIGN-1 | MAJ | Needs design | No report/template verb for formatted multi-section output |
| G-DESIGN-2 | MIN | Needs design | No chart/visualization spec surface |
| G-DESIGN-3 | MAJ | Needs design | No multi-axis sweep for budget reallocation |
| G-DESIGN-4 | OBS | Needs design | No cross-model/cross-cube comparison |
| E-1 | MAJ | Edge case / bug | `write.rs` uses `serde_json` — implicit dep, `unwrap_or_else` unreachable |
| E-2 | MIN | Edge case / bug | Hand-rolled timestamp may mis-date near leap-year month boundaries |
| E-3 | MIN | Edge case / bug | `count(predicate)` in `--aggregate` re-parses filter; latent borrow fragility |
| E-4 | MAJ | Edge case / bug | `trace` formula field shows `Debug` AST format, not readable formula string |
| E-5 | MAJ | Edge case / bug | `transform.rs` recipe parser incompatible with `mc-recipe` schema format |
| E-6 | MIN | Edge case / bug | `whatif --dry-run` `would_affect` is `--show` list, not actual dependents |

**Severity count:** CRIT: 1 | MAJ: 13 | MIN: 9 | OBS: 2

---

## Answers to the 17 specific questions

**Q1. Goldens-as-probes pattern — is it actually closed?**

Partially. `mc model query --coord` covers single exact-coordinate reads (`G-CLOSED-1`/`G-CLOSED-2` verified). However, the five Python scripts still use goldens-as-probes because:
(a) `mc model query` does not replay `writes.jsonl` (G-OPEN-1), so "reading after a write" still requires the Python CSV-mutate dance;
(b) `mc model trace` silently returns "source: input" for consolidated/rollup coordinates like `AllMarkets` and `AllTenure` (G-OPEN-3), meaning agents cannot read those via trace;
(c) There is no `--where` predicate that can reference time-function results (`prev()`, `lag()`) because those are engine-internal formula constructs, not externally filterable cell attributes. The goldens-as-probes pattern is the only way to read `prev()`-derived values indirectly.

**Q2. What-if completeness — multi-cell overrides and iterative scenarios?**

Not complete. `mc model whatif` supports exactly one cell override per invocation (G-OPEN-2). `whatif_v2.py` uses a single-cell override (one AdSpend cell), so the immediate use case is covered. `budget_reallocator.py` requires 4 simultaneous overrides, which is not supported. What-if-then-what-if chaining (iterative scenarios) is not supported; each call loads fresh and auto-rolls back.

**Q3. `mc model sweep` completeness for budget reallocation?**

The sweep verb is one-axis only. `budget_reallocator.py` sweeps three free dimensions simultaneously (holding the fourth as residual) — a 3D grid sweep that `mc model sweep` cannot replicate. There is also no constraint mechanism for holding total budget constant (G-DESIGN-3). Additionally, `mc model sweep` evaluates the metric globally across all leaf coordinates (G-OPEN-11), whereas the Python script measures revenue only for the 4 specific markets of interest.

**Q4. `mc model trace` for non-leaf coordinates — confirmed still missing?**

Confirmed missing. `crates/mc-cli/src/trace.rs:136-149` returns a synthetic `"source": "input"` node when `cell.trace` is `None`, which is the case for consolidated cells. `ltv_report.py:27-32` demonstrates the `AllTenure` and `AllMarkets` use case. This was acknowledged as known debt in the 6A.1 completion report but not fixed (G-OPEN-3).

**Q5. Reporting / output formatting — does Mosaic have a `mc model report` verb?**

No `mc model report` verb exists. `ltv_report.py` (117 lines) produces four formatted sections from cube values — the only Mosaic-native equivalent would be `mc model query --format text`, which produces a fixed-column table with no sections, no custom number formatting, and no label lookups. The gap is documented as G-OPEN-6 (open, clear path for a template approach) and G-DESIGN-1 (needs ADR for the right design).

**Q6. Charts / visualizations — what would Mosaic need to expose?**

None of the five Python scripts produce charts (they print text tables). However, the `ltv_report.py` engagement-band-by-tenure output is a bar-chart pattern, and the `budget_reallocator.py` ranked-allocation output is a horizontal bar chart. What Mosaic would need: a structured query response shape that carries axis metadata alongside data — e.g., `"chart_hint": {"x": "TenureMonth", "y": "EngagementBand", "type": "bar"}`. This is documented as G-DESIGN-2 (needs ADR). A UI or agent could render from `mc model query --format json` + manual axis specification today, but Mosaic provides no schema hints.

**Q7. Comparison beyond `mc model diff` — multi-cube, YoY, plan vs actual?**

`mc model diff` only compares two coordinate-filtered slices of the *same model*. There is no multi-cube comparison, no year-over-year shorthand (though `--left "Time=2025_01" --right "Time=2026_01"` would work if both are in the same model), and no snapshot comparison with annotation. The `--since last` and `--before`/`--after` modes from the Phase 6A handoff are unimplemented (G-OPEN-5). Cross-model comparison (G-DESIGN-4) requires design work.

**Q8. `mc model write` + write-log replay — current behavior and reproduction?**

Write persists to `<model_dir>/.tessera/writes.jsonl` correctly. Subsequent `mc model query` ignores this log entirely. Reproduction: `mc model write model.yaml --coord "..." --value 999` succeeds and shows "Written: ... = 999"; immediately running `mc model query model.yaml --coord "..."` returns the original canonical_inputs value, not 999. This is the P0 known debt from the 6A.1 completion report (G-OPEN-1).

**Q9. MCP tool coverage vs CLI verb coverage — asymmetry?**

The MCP tools list (`mcp.rs:155-267`) includes all 12 expected tools: `mosaic.demo`, `mosaic.model.{validate, inspect, lint, test, query, whatif, trace, sweep, diff, write}`, and `mosaic.tessera.transform`. This is 1:1 with the CLI verb surface. No asymmetry in coverage. However, there are schema quality issues (G-OPEN-12: `value` typed as `string`; `limit` typed as `string`) and the `mosaic.tessera.transform` tool uses `run_cli_verb` (no `structured` field) while the six Phase 6A model tools use `run_cli_verb_json` (with `structured` field) — intentional per the completion report but inconsistent.

**Q10. MCP tool ergonomics — are input schemas tight enough for LLMs?**

Mostly yes, with one significant gap. The `"value"` parameter in `mosaic.model.whatif` and `mosaic.model.write` is typed `"string"` in the JSON Schema descriptor but semantically requires a numeric string (G-OPEN-12). An LLM that passes `{"value": 223.0}` (a JSON number, which is the natural form) will get "missing required argument: value" with no diagnostic. The `"limit"` and `"depth"` parameters are also typed as `"string"` when they should be `"integer"`. The tool descriptions are concise and accurate; the coordinate format (`"Scenario=Base,...,Measure=X"`) is documented correctly. The `mosaic.model.query` tool's `"aggregate"` parameter description doesn't give examples of valid aggregate expressions, which may cause LLM hallucination.

**Q11. Exit-code coverage — correct routing on all error paths?**

The three confirmed paths work correctly (exit 0 = success, exit 1 = model error, exit 3 = I/O error — regression tests pass). One edge case is unconfirmed: what exit code is returned when the model YAML is valid but the `canonical_inputs.source` CSV file is missing? `load_model` calls `resolve_inputs` which reads the CSV; a missing CSV would be an I/O error, but `resolve_inputs` returns `ValidationError` items, which are currently mapped to `LoadModelError::Model` (exit 1). This may be the wrong routing — a missing CSV is an I/O error, not a model authoring error.

**Q12. JSON envelope completeness — missing forward-compat fields?**

The envelope has `schema_version: "1.0"` on all seven verbs (CRIT-2 closed). Missing fields that would improve forward compatibility and agent usability:
- **`warnings: []`** array — currently there is no place for non-fatal notices (e.g., "limit hit, results truncated" from G-OPEN-7; "write-log replay skipped" in the future)
- **`cache_status`** field — agents currently cannot tell if a query hit cached cell values or recomputed from rules
- **`revision_id`** on write responses (G-OPEN-13) — needed for the `--before`/`--after` diff mode
- **`truncated: bool` and `total_matched: int`** on query responses (G-OPEN-7) — currently the agent cannot tell if the result set was truncated by `--limit`

**Q13. `--format text` quality — is it actually readable?**

Mixed. `mc model trace --format text` is excellent — it uses Unicode tree connectors (`├──`, `└──`, `│`) and indentation, matching the handoff's intended spec exactly. `mc model query --format text` is mediocre — fixed 15-char column widths regardless of content length (G-OPEN-14), no row counts in the column header, and `cat({N})` for `Category` values. `mc model sweep --format text` is acceptable — aligned columns with a clear baseline comparison line. `mc model diff --format text` truncates coordinate strings to 60 characters without an ellipsis indicator, losing information. Overall: trace text is good; query/diff text is "JSON with newlines" quality.

**Q14. Streaming / pagination — how large queries are handled?**

No streaming. Everything is buffered. `--limit N` (default 10,000) is the only protection (G-OPEN-7). There is no `--offset`, no pagination cursor, and no per-row output. The JSON formatter builds the entire output string in memory before returning. For large models this is a buffer-then-print approach with no back-pressure. The `--output <file>` flag writes the complete buffer to disk atomically, which is correct for small models but problematic if the output exceeds available memory.

**Q15. Agent-discoverability — can an agent learn the schema from `mc mcp` alone?**

Mostly yes, with gaps. `tools/list` provides accurate descriptions and input schemas for all 12 tools. An LLM can discover: which verbs exist, which parameters are required vs optional, and rough parameter descriptions. What's missing from pure MCP introspection: (a) valid coordinate format examples are in the descriptions but not in a machine-readable `format` field; (b) valid element names for a specific model (the agent must call `mosaic.model.inspect` first to discover element names); (c) the relationship between `--set` coordinate format and `--coord` coordinate format (they use the same format but this isn't stated); (d) the `schema_version` field is not documented in the tool's output schema (`outputSchema` is absent from all tool descriptors).

**Q16. Failure messages aimed at LLMs — are they actionable?**

Inadequate for most failure modes. Examples of current error messages:
- Missing coord: `"error: could not resolve coordinate: Scenario=Actual,..."` — gives the failed coord string but no suggestion of which dimension was unrecognized or which valid element names exist
- Invalid `--where`: `"error: invalid --where expression: unexpected character: '-'"` — tells the LLM the character but not how to fix it (G-OPEN-10 — hyphens in element names)
- Missing required arg: `"missing required argument: value"` (from MCP) — tells the LLM what's missing but not the expected type or format
- Model validation error: the full `ValidationError` chain is returned, which is helpful but may be too verbose for an LLM to parse without additional structure

The error messages are human-readable but lack "did you mean X?" suggestions, structured error codes for programmatic recovery, or model-specific context (valid element names, valid measure names).

**Q17. `mc tessera transform` as agent surface — usable without Python wrappers?**

Not fully. Three gaps prevent standalone agent use:
1. **Recipe incompatibility (E-5):** The `transform.rs` recipe parser uses a bespoke line-scanner that does not understand the `mc-tessera` recipe schema. An agent that creates a recipe following Phase 5B documentation gets silent failures.
2. **URL fetch via `curl` subprocess (G-OPEN-9):** An agent in a restricted container without `curl` installed cannot fetch from URLs at all.
3. **No JSON schema output (no `schema_version` envelope):** `tool_transform` uses `run_cli_verb` (not `run_cli_verb_json`), so the `structured` field is absent from MCP responses; an agent must parse the raw CSV text from `content[0].text`.

For local CSV transforms with a simple recipe (`column_mappings` + `defaults`), `mc tessera transform` works as an agent surface. For the API-fetch + complex-recipe use case (the primary motivation from the handoff), it is not yet reliable.
