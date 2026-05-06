# Master Gap Report — Phase 6A.1 Post-Ship Audit

## Synthesizer: Claude Sonnet 4.6 (Audit Instance D)
## Date: 2026-05-06
## Source audits: A (Data-In), B (Calculation), C (Data-Out + Agent Surface)
## Scope: All gaps surviving as of HEAD `44a7437` (Phase 6A.1)

---

## Spot-check results (pre-synthesis)

Fifteen citations verified by running the cited file:line before synthesis:

| Finding | Cited line | Verified? | Notes |
|---|---|---|---|
| A:G-OPEN-1 | `mc-recipe/src/schema.rs:184` | ✓ | DriverKind enum confirmed; no Xlsx variant |
| A:G-OPEN-8 | `mc-drivers/src/csv_driver.rs:29` | ✓ | `SCHEMA_INFERENCE_ROWS: usize = 100` exact |
| A:E-1 | `mc-tessera/src/incremental.rs:142` | ✓ | Appends `WHERE {column} > '{last_value}'` unconditionally |
| A:G-OPEN-4 | `mc-recipe/src/schema.rs` | ✓ | `fill_missing`/`carry_forward` grep returns zero matches |
| A:G-CLOSED-1 | `mc-tessera/src/time_format.rs:59` | ✓ | `parse_strptime` starts at line 63 |
| B:G-OPEN-1 | `mc-core/src/rule.rs:107` | ✓ | `DimElement(DimensionId)` variant confirmed at line 108 |
| B:G-OPEN-5 | `mc-model/src/schema.rs:604` | ✓ | `ParsedLookupTable.key_dimension: String` (singular) |
| B:G-OPEN-8 | `mc-model/src/validate.rs:1706` | ✓ | `check_fitted_model_blocks` does not validate arity |
| B:G-OPEN-3 | `mc-model/src/formula.rs` | ✓ | `extrapolate`/`carry_forward` grep returns zero matches |
| B:E-1 | `mc-model/src/formula.rs:638` | ✓ | `rolling_avg` parser confirmed; no partial-window policy |
| C:G-OPEN-1 | `mc-cli/src/query.rs:278` | ✓ | `load_model` ends at `apply_canonical_inputs`; no writes.jsonl read |
| C:G-OPEN-3 | `mc-cli/src/trace.rs:136` | ✓ | `None => TraceTree { source: "input".to_string(), ... }` |
| C:E-4 | `mc-cli/src/trace.rs:207` | ✓ | `format!("{:?}", expr_summary.op)` — Debug, not formula string |
| C:G-OPEN-12 | `mc-cli/src/mcp.rs:214` | ✓ | `("value", "string", "New numeric value to set.", true)` |
| C:G-OPEN-9 | `mc-cli/src/transform.rs:167` | ✓ | `std::process::Command::new("curl")` |

**Finding quality:** all 15 citations confirmed exact. One line-number is off-by-4 (B:E-1 references 634; actual parser entry is 638) — minor; no finding mischaracterizes the code.

---

## Section 1 — Master gap inventory

All gaps deduplicated and renumbered M-1 through M-48. Cross-audit merges are noted.
Six entries from A and B were merged (same user need, different lens): M-11, M-12, M-13, M-14, M-43.

### Agent Surface — CRIT/P0

**M-1: Write-log replay not wired into `load_model`**
- **Use case blocked:** Any agent that calls `mc model write` and then immediately calls `mc model query`, `whatif`, or `trace` gets stale data. The P0 "write-then-read" loop cannot work without this.
- **Evidence:** `crates/mc-cli/src/query.rs:278–317` — `load_model` ends at `apply_canonical_inputs`; `.tessera/writes.jsonl` is never read. `whatif_demo.py:44–56` (email-matchback) exists precisely because this is broken.
- **Impact:** ~30 lines of Python eliminated per agent use-case; blocks every iterative "what-if then check" agent loop.
- **Source audits:** C
- **6A/6A.1 status:** Not addressed; explicitly listed as P0 known debt in the 6A.1 completion report.

---

**M-2: `mc model trace` formula field shows Debug-format AST instead of formula string**
- **Use case blocked:** LLM reasoning chain. An agent calling `mc model trace` to explain a value gets `"formula": "Mul"` instead of `"formula": "Spend / CPC"`. Explainability is the primary reason for the trace verb.
- **Evidence:** `crates/mc-cli/src/trace.rs:207` — `format!("{:?}", expr_summary.op)`.
- **Impact:** Trace output is functionally unusable for LLM-driven explanation; all agent explain-value tasks fall back to Python.
- **Source audits:** C
- **6A/6A.1 status:** Not addressed.

---

**M-3: `mc tessera transform` recipe parser incompatible with `mc-recipe` YAML schema**
- **Use case blocked:** Any agent that creates a recipe using `mc tessera propose` (Phase 5B) and then passes it to `mc tessera transform`. Silent empty output or incorrect mappings.
- **Evidence:** `crates/mc-cli/src/transform.rs:202–326` — bespoke line-scanner handles only `column_mappings`, `mappings`, `defaults`, `json_path`, `output_columns`, `scale`; does not parse `source.driver`, `on_error`, `time_format`, or any Phase 5B schema fields.
- **Impact:** `mc tessera transform` is unreliable as an agent surface for the API-fetch use case; blocks the KellyBets scenario from the 6A handoff.
- **Source audits:** C
- **6A/6A.1 status:** Not addressed.

---

### Agent Surface — MAJ (Phase 6A.2 candidates)

**M-4: `mc model whatif` single-cell-only; no multi-cell override**
- **Use case blocked:** Budget reallocation across 4 markets simultaneously (`budget_reallocator.py:43–63`). With single-cell override, each market call reloads the model independently, losing cross-market interaction effects.
- **Evidence:** `crates/mc-cli/src/whatif.rs:13–20` — `WhatifCommand.set_coord: String` (singular). `budget_reallocator.py:120–132` — inner loop writes 4 cells per sweep point.
- **Impact:** ~80 lines of Python eliminated; blocks any correlated multi-input scenario.
- **Source audits:** C
- **6A/6A.1 status:** Not addressed.

---

**M-5: `mc model trace` returns `"source": "input"` at consolidated/rollup coordinates**
- **Use case blocked:** Agents querying AllMarkets, AllTime, AllChannels — the most common analytical summary coordinates. `ltv_report.py:27–32` reads `AllTenure` and `AllMarkets`; trace on these silently lies.
- **Evidence:** `crates/mc-cli/src/trace.rs:139–148` — `None => TraceTree { source: "input".to_string(), ... }`. Acknowledged debt in 6A.1 completion report.
- **Impact:** Any model with rollup dimensions returns factually wrong trace output; LLM reasoning chains built on this are broken.
- **Source audits:** C
- **6A/6A.1 status:** Not addressed; listed as known debt.

---

**M-6: `mc model sweep` reloads YAML 2N+1 times per sweep**
- **Use case blocked:** Budget optimization. `budget_reallocator.py:118` times 343 evaluations at ~12 s; a Mosaic-native sweep at 30ms per YAML parse would take ~20 s.
- **Evidence:** `crates/mc-cli/src/sweep.rs:166` — `load_model` called once per sweep point; `find_coefficient_index` at line 197 calls `load_model` a second time per point.
- **Impact:** Dissuades agents from using `mc model sweep`; they fall back to Python loops calling `mc model test` instead.
- **Source audits:** C
- **6A/6A.1 status:** Listed as P1 known debt (MAJ-4) in 6A.1 report.

---

**M-7: `mc model diff --since last` and snapshot modes unimplemented**
- **Use case blocked:** "What lines moved since last ingest?" — the primary post-`mc tessera apply` agent question. `bench.py:106–117` explicitly constructs a before/after pattern for exactly this.
- **Evidence:** `crates/mc-cli/src/diff.rs:69–79` — parser requires both `--left` and `--right`; no `--since`, `--before`, or `--after` flag.
- **Impact:** Agents detecting post-ingest changes must maintain their own snapshot/diff logic in Python.
- **Source audits:** C
- **6A/6A.1 status:** Committed in Phase 6A handoff; not shipped.

---

**M-8: `mc model sweep` metric evaluates globally; no `--metric-where` scoping**
- **Use case blocked:** Per-market revenue optimization. `budget_reallocator.py:78–80` sweeps 4 specific markets; `mc model sweep --metric "sum(PredictedRevenue)"` sums across all market/time/channel combos.
- **Evidence:** `crates/mc-cli/src/sweep.rs:344–424` — `eval_metric` calls `enumerate_leaf_coords(cube, refs)` returning ALL leaf coordinates; no filter.
- **Impact:** Sweep results are misleading for multi-market models; agents fall back to Python probes.
- **Source audits:** C
- **6A/6A.1 status:** Not addressed.

---

**M-9: `--show` dimension names + `--aggregate` are mutually exclusive; no group-by**
- **Use case blocked:** "For each Market, sum(Revenue)" — any grouped aggregate query. `budget_reallocator.py:80–103` constructs separate probes per market precisely because this is missing.
- **Evidence:** `crates/mc-cli/src/query.rs:185–197` — `--aggregate` and `--show` are dispatched separately; `run_aggregate` at lines 998–1078 only handles `ScalarValue::F64`, not `ScalarValue::Str` from dimension-name `--show`.
- **Impact:** Every "aggregate by dimension" agent query requires post-processing in Python.
- **Source audits:** C
- **6A/6A.1 status:** Not addressed.

---

**M-10: No `mc model report` verb for formatted multi-section output**
- **Use case blocked:** Shareable formatted artifacts. `ltv_report.py` is 117 lines that produce a 4-section human-readable report entirely from cube values; there is no CLI equivalent.
- **Evidence:** No `mc model report` verb exists. `crates/mc-cli/src/query.rs:1199–1221` — `format_text` uses fixed 15-char column widths; no section grouping, no number formatting, no label lookup.
- **Impact:** Every "produce a readable summary for a human" use case requires Python.
- **Source audits:** C (G-OPEN-6, G-DESIGN-1)
- **6A/6A.1 status:** Not addressed; needs design (see M-46 for template/report design).

---

### Formula Layer — MAJ (cross-cutting: A+B)

**M-11: No `is_element()` or string literals — indicator/one-hot generation requires Python**
- **Use case blocked:** Pooled MMM market dummies (`IsHouston`, `IsAustin`, `IsDenver`, `IsAmarillo`). Any model that needs to branch on "which element am I at" in a dimension.
- **Evidence (Python):** `prepare_mmm_inputs.py:70–85` — generates 4 markets × 29 months × 4 indicators = 464 input rows. `tide-mmm.yaml:151–154` — four Input measures carrying only coordinate identity.
- **Evidence (Mosaic absence):** `crates/mc-core/src/rule.rs:108` — `DimElement(DimensionId)` exists internally but is unexposed. `crates/mc-model/src/formula.rs` — no `is_element()`, no `current_element()`, no string literals (Phase 3I deferred per `docs/research-notes/formula-language-expansion.md:719–733`).
- **Impact:** 20 lines of Python + 464 CSV rows eliminated; also blocks geography-conditional rules and segment-tiered pricing.
- **Source audits:** A (G-DESIGN-2), B (G-OPEN-1, G-DESIGN-1)
- **6A/6A.1 status:** Not addressed; Phase 3I is the committed vehicle.

---

**M-12: No `extrapolate_last_value` / carry-forward formula function**
- **Use case blocked:** Extending AdSpend to Nov/Dec 2026 where source data stops at October. Any "last-known-value baseline" projection.
- **Evidence (Python):** `prepare_v2_inputs.py:154–176` and `prepare_mmm_inputs.py:47–65` — identical LOCF loops in both scripts.
- **Evidence (Mosaic absence):** `crates/mc-model/src/formula.rs` — no `extrapolate_last_value`, no `fill_forward`. `lag()` with a Null source returns Null — it does not fill forward.
- **Impact:** ~25 lines eliminated from each script (~50 combined); affects any FP&A model where year-end data arrives late.
- **Source audits:** A (G-OPEN-4), B (G-OPEN-3, G-DESIGN-4)
- **6A/6A.1 status:** Not addressed. Interaction with `is_future()` scope is a design question (see M-47).

---

**M-13: `actual_ref()` has no fallback; no `scenario_ref()` — forces Plan→Actual mirroring in Python**
- **Use case blocked:** Future-month formulas that read actual spend but fall back to plan spend when actuals aren't yet available.
- **Evidence (Python):** `prepare_v2_inputs.py:134–152` and `prepare_mmm_inputs.py:34–46` — identical "mirror Plan→Actual" loops injecting ~20–30 rows per run.
- **Evidence (Mosaic absence):** `crates/mc-model/src/schema.rs:430–434` — `ParsedActualRefBody.measure: String` only; no second argument. No `scenario_ref()` function in `formula.rs`.
- **Impact:** ~25 lines eliminated from each script (~50 combined); affects any rolling-forecast model where future actuals haven't landed.
- **Source audits:** A (G-OPEN-5), B (G-OPEN-4, G-DESIGN-2)
- **6A/6A.1 status:** Not addressed; coupled to cross-coord dep-graph design (M-39).

---

**M-14: No `parameters:` block — time-invariant constants must be broadcast to every Time leaf**
- **Use case blocked:** Q1-2026 per-dollar anchor constants that apply across all time periods. Any calibration constant, annual budget target, or conversion rate that is time-invariant.
- **Evidence (Python):** `prepare_v2_inputs.py:180–194` — broadcasts anchor to every Time element: 5 anchor measures × 29 months × 4 markets = ~580 CSV rows storing identical constants.
- **Evidence (Mosaic absence):** `crates/mc-model/src/schema.rs:25–61` — `ParsedModel` has no `parameters:` block. No partial-coordinate binding semantics anywhere in the schema.
- **Impact:** ~30 lines eliminated; ~580 CSV rows eliminated; affects any model with calibration constants or annual targets.
- **Source audits:** A (G-OPEN-6), B (G-OPEN-2)
- **6A/6A.1 status:** Not addressed; needs ADR before scoping.

---

### Formula Layer — MAJ (Calculation only)

**M-15: Phase 3I math primitives absent**
- **Use case blocked:** NPV, compound growth (FP&A), safety stock (demand planning), Kelly criterion (sports betting), SaaS churn curves. `docs/research-notes/formula-language-expansion.md:629–715` shows Finance FP&A at 85% coverage without these.
- **Evidence:** `crates/mc-model/src/formula.rs:817–843` — `exp()` and `norm_cdf()` present (3H); `pow`, `sqrt`, `ln`, `log10`, `round`, `floor`, `ceil`, `mod`, `norm_inv` all absent. MC1007 fires on unknown function name.
- **Impact:** Entire NPV / Kelly / safety-stock formula classes require Python. Likely 100+ lines across domain models.
- **Source audits:** B (G-OPEN-7)
- **6A/6A.1 status:** Not addressed; Phase 3I is the committed vehicle.

---

**M-16: `lookup_tables` single-key only**
- **Use case blocked:** Per-market per-month seasonality tables. Today five near-identical single-key tables exist in `tide-matchback.yaml` where one two-key table would suffice.
- **Evidence:** `crates/mc-model/src/schema.rs:604` — `ParsedLookupTable.key_dimension: String` (singular); validator at `validate.rs:1298–1329` validates one key dimension.
- **Impact:** Model verbosity — 5 tables collapse to 1; also blocks territory-by-time rate tables and product-by-channel margin tables.
- **Source audits:** B (G-OPEN-5)
- **6A/6A.1 status:** Not addressed; Phase 3G amendment.

---

**M-17: `predict()` arity mismatch not validated + `norm_cdf` sigma≤0 not guarded**
- **Use case blocked:** Silent wrong results when feature count doesn't match coefficient count; NaN-entering-storage when sigma=0 at runtime.
- **Evidence:** `crates/mc-model/src/validate.rs:1706–1768` — no cross-reference between formula `predict()` arity and model coefficient count. `validate.rs:1706–1846` — MC1021 planned but not implemented. Eval-time sigma=0 in normal CDF produces +Inf or NaN, violating the engine's NaN-exclusion invariant (engine-semantics §7).
- **Impact:** Silent wrong predictions; potential NaN-in-storage invariant violation.
- **Source audits:** B (G-OPEN-8, G-OPEN-9)
- **6A/6A.1 status:** Not addressed; Phase 3H amendment.

---

**M-18: `sum_over` only — no `avg_over`, `min_over`, `max_over`, `wavg_over`**
- **Use case blocked:** Market-average ROAS (requires weighted average over markets); "max across all channels" (performance relative to best channel); top-N ranking patterns.
- **Evidence:** `crates/mc-model/src/formula.rs:760–778` — only `sum_over`. `crates/mc-core/src/rule.rs:104` — `SumOver(DimensionId, ElementId)` hardcoded to sum semantics.
- **Impact:** Multi-aggregate queries require multiple rules or Python post-processing.
- **Source audits:** B (G-OPEN-10)
- **6A/6A.1 status:** Not addressed; Phase 3G amendment or 3I.

---

**M-19: Aggregation methods — only Sum / WeightedAverage / Min / Max**
- **Use case blocked:** Median NPS per region, variance of regional margins, beginning/end-of-period balance semantics.
- **Evidence:** `crates/mc-model/src/validate.rs:854–922` — `check_aggregation_methods_supported` rejects anything other than `Sum`, `WeightedAverage`, `Min`, `Max`.
- **Impact:** Survey/satisfaction models, risk models, and time-ordered balance semantics cannot be expressed without Python post-aggregation.
- **Source audits:** B (G-OPEN-11)
- **6A/6A.1 status:** Not addressed; requires mc-core consolidation change (new phase needed).

---

**M-20: `output_bound` missing on fitted models; `and`/`or` not short-circuit**
- **Use case blocked:** (a) Negative Amarillo revenue from Ridge MMM — predictions must be clamped to ≥ 0; any logistic model with no sigmoid enforcement produces out-of-[0,1] probabilities. (b) Formulas with expensive cross-coord reads in `or` predicates pay full eval cost even when first predicate short-circuits.
- **Evidence:** `crates/mc-model/src/schema.rs:633–646` — `ParsedFittedModel` has no `output_bound`. `crates/mc-core/src/rule.rs:546–562` — `And`/`Or` evaluate both sides unconditionally.
- **Impact:** (a) Correctness risk for extrapolating linear models; (b) performance concern only, not a correctness bug.
- **Source audits:** B (G-OPEN-6, G-OPEN-12)
- **6A/6A.1 status:** Not addressed; (a) Phase 3H amendment; (b) Phase 3I or 6A.2.

---

**M-21: No `ifs()` / `switch()` variadic conditional**
- **Use case blocked:** Models with 4–6 branching cases (dashboard segmentation, scenario-type branching). Nested `if(if(if(...)))` chains are hard to read and error-prone — a missed else silently returns Null.
- **Evidence:** `crates/mc-model/src/formula.rs:481–497` — only `if(cond, then, else)` with exactly 3 arguments.
- **Impact:** Ergonomics; not a correctness issue. Models with multi-way branching become unreadable.
- **Source audits:** B (G-OPEN-13)
- **6A/6A.1 status:** Not addressed; Phase 3I.

---

### Data Ingestion — MAJ

**M-22: No xlsx/Excel driver**
- **Use case blocked:** The primary real-world data source for Tide Cleaners is "Tide Cleaners - LTD Comparison.xlsx." This is the root cause of 220 lines of Python in `flatten_ltd_comparison.py` — the entire file exists only to read xlsx.
- **Evidence (Python):** `email-matchback/scripts/mosaic/flatten_ltd_comparison.py:1–238` — entire file.
- **Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs:184–210` — no `Xlsx` variant in `DriverKind`.
- **Impact:** ~220 lines eliminated (all of `flatten_ltd_comparison.py`); blocks any FP&A or demand planning user whose data lives in Excel (the majority of enterprise planning data).
- **Source audits:** A (G-OPEN-1)
- **6A/6A.1 status:** Not addressed; needs ADR (Phase 5C amendment or 5D).

---

**M-23: Year-blocked / banded layout not expressible in any recipe**
- **Use case blocked:** Multi-year Excel workbooks with non-row-1 headers and multiple header blocks per sheet. `flatten_ltd_comparison.py` hardcodes row offsets for 3 year-blocks.
- **Evidence:** `crates/mc-recipe/src/schema.rs:114–153` — `SourceConfig` has no `skip_rows`, `header_row`, `sheet`, or layout descriptor fields.
- **Impact:** ~220 lines eliminated (shared with M-22; this is the layout-parsing half of `flatten_ltd_comparison.py`); also affects government data releases and financial reporting.
- **Source audits:** A (G-OPEN-2)
- **6A/6A.1 status:** Not addressed; coupled to M-22 xlsx driver. Needs ADR.

---

**M-24: `mc tessera retry-quarantine` unimplemented**
- **Use case blocked:** Re-processing bad-data rows after source data correction. ADR-0010 Decision 7 explicitly deferred this to Phase 5C, but it was not shipped.
- **Evidence:** `crates/mc-tessera/src/sidecar.rs:77–81` — `quarantine_path()` exists and quarantine files are written; `crates/mc-cli/src/` has no `retry-quarantine` verb (confirmed by `ls`).
- **Impact:** Users must re-run full imports or write Python retry loops.
- **Source audits:** A (G-OPEN-11)
- **6A/6A.1 status:** Committed in ADR-0010 Decision 7; not shipped.

---

**M-25: Multi-file ingest from a single recipe — not possible**
- **Use case blocked:** `build_ltv_cohort.py` reads 4 xlsx files (one per market) and aggregates them into one output. Today a single recipe maps to a single source path.
- **Evidence:** `email-matchback/scripts/mosaic/build_ltv_cohort.py:34–39` — `MARKET_FROM_FILE` maps 4 filenames to 4 market values. `crates/mc-recipe/src/schema.rs:114–153` — `SourceConfig` has a single `path`/`query`/`url`.
- **Impact:** Any multi-file aggregation workflow requires Python orchestration.
- **Source audits:** A (G-DESIGN-4)
- **6A/6A.1 status:** Not addressed; needs ADR before phase scoping.

---

**M-26: Aggregation transforms (group_by + sum/avg before cube write) not possible in recipes**
- **Use case blocked:** `build_ltv_cohort.py` aggregates ~5,000 customer rows to ~300 cohort rows (16:1 fan-in) before writing. No recipe can express "N source rows → 1 cell."
- **Evidence:** `email-matchback/scripts/mosaic/build_ltv_cohort.py:93–161` — entire aggregation pipeline.
- **Evidence (Mosaic absence):** `crates/mc-recipe/src/schema.rs` — no `aggregate:`, `group_by:`, or `rollup:` directive.
- **Impact:** Customer-level → cohort aggregation always requires Python or manual SQL pre-processing.
- **Source audits:** A (G-DESIGN-5)
- **6A/6A.1 status:** Not addressed. SQL-driver workaround exists (DuckDB UNION + GROUP BY) but requires SQL knowledge.

---

### Agent Surface + Ingestion — MIN (clear path, Phase 6A.2 or 5C amendment)

**M-27: `--limit` default with no pagination or truncation warning**
- **Evidence:** `crates/mc-cli/src/query.rs:180` — `limit = 10000`; no `--offset`, no `truncated: bool` in envelope.
- **Source audits:** C (G-OPEN-7)

**M-28: `serde_json` used in `transform.rs` and `write.rs` without explicit dep declaration**
- **Evidence:** `crates/mc-cli/src/transform.rs:347` and `crates/mc-cli/src/write.rs:189` — direct `serde_json::` calls; implicit transitive dep.
- **Source audits:** C (G-OPEN-8, E-1)

**M-29: `mc tessera transform` URL fetch via `curl` subprocess**
- **Evidence:** `crates/mc-cli/src/transform.rs:167–177` — `std::process::Command::new("curl")`. `mc-drivers` already depends on `ureq`.
- **Source audits:** C (G-OPEN-9)

**M-30: `--where` tokenizer rejects hyphenated identifiers**
- **Evidence:** `crates/mc-cli/src/query.rs:486–490` — identifier rule: start = `is_ascii_alphabetic() || '_'`; no hyphen.
- **Source audits:** C (G-OPEN-10)

**M-31: MCP `value` / `limit` / `depth` parameters typed as `"string"` not `"number"` / `"integer"`**
- **Evidence:** `crates/mc-cli/src/mcp.rs:214` — `("value", "string", ...)`.
- **Source audits:** C (G-OPEN-12)

**M-32: `mc model write` response lacks stable `revision_id`**
- **Evidence:** `crates/mc-cli/src/write.rs:197–217` — JSON output has no `write_id` or log-sequence.
- **Source audits:** C (G-OPEN-13)

**M-33: ISO week from date computation absent in `canonicalize_period`**
- **Evidence:** `crates/mc-tessera/src/time_format.rs:263–293` — explicit `// not implemented` comment for week-from-ymd.
- **Source audits:** A (G-OPEN-3)

**M-34: Ingestion minor polish cluster (skip_rows, %e/%j tokens, gzip support, f64 inference widening)**
- Four small additions to CSV / strptime handling; none require ADR.
- **Evidence:** A (G-OPEN-7, G-OPEN-8, G-OPEN-10, G-OPEN-12).

**M-35: Secrets vault integration**
- **Evidence:** `crates/mc-tessera/src/secrets.rs` — only `EnvVarSecretResolver` implemented.
- **Source audits:** A (G-OPEN-9)
- **6A/6A.1 status:** Deferred to Phase 5E (explicitly in scope).

**M-36: Object-store driver (S3/GCS/Azure Blob prefix scanning)**
- **Evidence:** No `S3`/`Gcs`/`AzureBlob` variant in `DriverKind`. Needs MSRV check for `object_store` crate.
- **Source audits:** A (G-OPEN-13)

---

### Latent Bugs (not gaps but correctness risks)

**E-A: Incremental watermark appends `WHERE` without checking for existing `WHERE` clause**
- **Evidence:** `crates/mc-tessera/src/incremental.rs:148` — `format!("{query} WHERE {column} > '{last_value}'")`. Query with existing WHERE produces invalid SQL on second run.

**E-B: `on_missing_element: create` generates non-deterministic ElementIds across sessions**
- **Evidence:** `crates/mc-tessera/src/transform.rs:240–243` — `ElementId(DYNAMIC_ELEMENT_BASE + refs.elements.len())`. IDs differ if recipes process rows in different order. Rollback + re-apply produces orphaned coordinates.

**E-C: `rolling_avg` partial-window behavior not locked by any golden**
- **Evidence:** No golden tests `rolling_avg` at index 0 or 1. If it returns Null at boundary, `predict(...)` silently drops early months.

**E-D: `lag(measure, negative_k)` (lead behavior) unconfirmed in eval path**
- **Evidence:** Parser accepts negative literals; no golden tests negative-lag; `cube.rs` eval not confirmed.

**E-E: `calibrate()` out-of-range behavior unspecified; may produce probability > 1**
- **Evidence:** Validator checks PAVA points are ascending but does not specify boundary behavior (clamp vs. extrapolate vs. Null).

**E-F: `whatif --dry-run` `would_affect` is literally the `--show` list, not computed dependents**
- **Evidence:** `crates/mc-cli/src/whatif.rs:339–342` — `would_affect` is `cmd.show.iter()`.

**E-G: Hand-rolled `days_to_ymd` in write.rs may mis-date near leap-year month boundaries**
- **Evidence:** `crates/mc-cli/src/write.rs:260–286` — comment: "good enough for logging."

---

## Section 2 — Cross-cutting patterns

Six patterns appear in two or more lenses. These are the highest-leverage fixes because each one closes gaps across the full data → calculate → output pipeline.

### Pattern P1: Dimension-identity formulas (A + B)

**"The formula evaluator knows which element it's at, but cannot express that as a value."**

- Data-In lens: Python generates 464 indicator rows because the recipe can't express "for Market=Houston, write 1.0 for IsHouston."
- Calculation lens: Formulas can't branch on "if I'm at Market=Houston" because there are no string literals and no `is_element()` function.
- Root cause: `Expr::DimElement(DimensionId)` exists in the kernel (`rule.rs:108`) but is unexposed to the formula language or the recipe layer.
- Unified fix: String literal support + `current_element(DimName)` in Phase 3I simultaneously solves the formula gap and makes the recipe-side indicator generation obsolete.
- Gaps closed: M-11 (464 rows + 20 lines Python).

### Pattern P2: Cross-scenario reads (A + B)

**"The engine always knows the caller's scenario but cannot read from a different scenario in a formula."**

- Data-In lens: Python mirrors Plan→Actual (2 × ~25 lines) because recipes can only write what they read; they can't synthesize across scenarios.
- Calculation lens: `actual_ref()` returns Null for future months; no fallback and no `scenario_ref()` means Python must pre-load the data.
- Root cause: `ParsedActualRefBody` is single-argument; the kernel has no cross-scenario read primitive in the formula layer.
- Unified fix: `scenario_ref(measure, "ScenarioName")` in the formula language makes the recipe mirroring obsolete. This is coupled to M-39 (MAJ-3 cross-coord dep graph).
- Gaps closed: M-13 (~50 lines Python across two scripts).

### Pattern P3: Time-invariant constants broadcast to time leaves (A + B)

**"A scalar that doesn't vary by time must be stored at every time element."**

- Data-In lens: Python broadcasts Q1 anchor constants to 29 time periods × 4 markets = ~580 rows.
- Calculation lens: No `parameters:` block means model can't declare "this value is constant across Time."
- Root cause: The YAML schema has no partial-coordinate binding; every measure must be declared at every coordinate in its scope.
- Unified fix: A `parameters:` block that fixes some dimensions while leaving others free is the model-layer solution; a recipe `broadcast_to_dimension:` is the weaker ingestion-side workaround.
- Gaps closed: M-14 (~580 CSV rows + 30 lines Python).

### Pattern P4: Carry-forward extrapolation (A + B)

**"Both the ingestion layer and the formula layer need a way to fill missing time-series values forward."**

- Data-In lens: Recipe has no `fill_missing: strategy: carry_forward` directive.
- Calculation lens: No `extrapolate_last_value()` or `fill_forward()` formula function.
- Design complication: Unconditional carry-forward fills past-gaps (where genuinely missing actuals should stay Null) AND future-gaps. The right semantic is anchor-conditional, which requires `is_future()` composability (M-47). A recipe-level directive would be simpler but less expressive; a formula function is more expressive but needs the scope-interaction design resolved first.
- Gaps closed: M-12 (~50 lines Python across two scripts).

### Pattern P5: Formula parser and filter parser gap (B + C)

**"Both the model formula evaluator and the CLI `--where` filter need string comparison, but they are separate parsers at different maturity levels."**

- Calculation lens: `--where` filter parser is Phase-6A custom code; the formula parser is the real evaluator. Unifying them is committed for Phase 3I per `formula-language-expansion.md:719–733`.
- Data-Out lens: The filter tokenizer (`query.rs:486–490`) rejects hyphens; `--metric-where` on sweep (M-8) would need the same parser; group-by (M-9) would need a filter-like expression.
- Root cause: Two parsers doing overlapping work. The Phase 3I unification resolves this but requires a design decision on how much formula capability is allowed in filter expressions (cross-coord reads in `--where` are semantically ambiguous).
- Gaps closed when resolved: M-8, M-9, M-30, parts of M-44.

### Pattern P6: Write-then-read coherence (C alone, but blocks all of A+B)

**"The agent surface writes a cell but subsequent reads ignore the write."**

- Data-Out lens only: M-1 (write-log replay) is a CRIT that makes all iterative agent loops broken.
- Why cross-cutting: Any improvement to formula expressiveness (M-11–M-21) or ingestion (M-22–M-26) is fully testable by agents only after M-1 is closed. The write-then-query agent loop is the primary test harness for all other gaps.
- Gaps unblocked: Every M-N in §1 that an agent would verify by writing a cell then querying downstream effects.

**Pattern rank by closure value:** P1 > P5 > P2 > P3 > P4 > P6.
P6 is ranked last despite being CRIT because it blocks verification rather than unlocking new capabilities; it must ship first but it closes only one gap directly.

---

## Section 3 — Phase mapping

| Gap | Proposed phase | Rationale |
|---|---|---|
| **M-1** Write-log replay | **Phase 6A.2** | P0 known debt; clear-path fix in `query.rs` |
| **M-2** Trace formula debug format | **Phase 6A.2** | Bug fix in `trace.rs`; no design needed |
| **M-3** Transform recipe incompatibility | **Phase 6A.2** | Bug fix; align `transform.rs` parser with mc-recipe schema |
| **M-4** Whatif multi-cell | **Phase 6A.2** | `--set` as repeatable flag; snapshot lifecycle already correct |
| **M-5** Trace at consolidated coords | **Phase 6A.2** | P1 known debt; enumerate leaf children in `build_trace_tree` |
| **M-6** Sweep YAML reload | **Phase 6A.2** | P1 known debt (MAJ-4); compile once before sweep loop |
| **M-7** diff --since last | **Phase 6A.2** | Committed in 6A handoff; not shipped |
| **M-8** Sweep metric global | **Phase 6A.2** | `--metric-where` reuses `Filter` infrastructure from `query.rs` |
| **M-9** No group-by | **Phase 6A.2** | `--group-by` flag; partition before aggregate |
| **M-10** No report verb | **Needs ADR** | Design choice between template engine, golden extensions, notebook export |
| **M-11** Indicator / is_element | **Phase 3I** | Committed; requires string literals which are Phase 3I scope |
| **M-12** Carry-forward extrapolation | **Phase 3I or 3F amendment** | Formula function; design question on is_future() scope (see M-47) |
| **M-13** scenario_ref / actual_ref fallback | **Needs ADR** | Coupled to MAJ-3 (M-39); three viable shapes, no consensus |
| **M-14** parameters: block | **Needs ADR** | New YAML block + partial-coord semantics; ADR before scoping |
| **M-15** Math primitives | **Phase 3I** | Committed per formula-language-expansion.md; 9 new parser cases |
| **M-16** Multi-key lookup tables | **Phase 3G amendment** | Additive schema change; `key_dimensions: Vec<String>` backward-compatible |
| **M-17** Predict arity + norm_cdf sigma | **Phase 3H amendment** | Validation-only fix; no new schema blocks |
| **M-18** avg_over family | **Phase 3G amendment or 3I** | One parser case + one kernel Expr variant each |
| **M-19** Advanced aggregation (Median, Variance) | **New phase (touches mc-core)** | Requires consolidation engine change; not Phase 3-safe |
| **M-20** output_bound + and/or short-circuit | **Phase 3H amendment** (bound); **Phase 6A.2** (short-circuit) | Two independent fixes |
| **M-21** ifs() / switch() | **Phase 3I** | Maps to chain of If nodes at compile time; no kernel change |
| **M-22** xlsx driver | **New sub-phase (5C.1 or 5D)** | New crate dep (calamine); new schema fields; needs ADR |
| **M-23** Year-blocked layout | **New sub-phase (5D)** | Coupled to M-22; `layout:` block in SourceConfig; needs ADR |
| **M-24** retry-quarantine verb | **Phase 5C amendment** | Committed in ADR-0010 Decision 7; straightforward CLI verb |
| **M-25** Multi-file ingest | **Needs ADR** | Four viable shapes; coupled to recipe chaining design (M-42) |
| **M-26** Aggregation transforms | **Needs ADR** | Incompatible with streaming batch design without intermediate buffer |
| **M-27** --limit / pagination | **Phase 6A.2** | `--offset` flag + `truncated: bool` in envelope |
| **M-28** serde_json implicit dep | **Phase 6A.2** | Dependency hygiene; either explicit decl or switch to hand-rolled emitter |
| **M-29** curl subprocess | **Phase 6A.2** | Switch to `ureq` already in workspace via mc-drivers |
| **M-30** Hyphenated identifiers in --where | **Phase 6A.2** | Tokenizer rule change |
| **M-31** MCP value type | **Phase 6A.2** | Schema descriptor change; coercing accessor |
| **M-32** revision_id on write | **Phase 6A.2** | Monotonic counter added to `writes.jsonl` and JSON response |
| **M-33** ISO week from date | **Phase 6A.2** | 1-function addition to `canonicalize_period` |
| **M-34** Ingestion polish cluster | **Phase 6A.2 or 5C amendment** | skip_rows, %e/%j, gzip, f64 widening; all ≤ 20-line changes |
| **M-35** Secrets vault | **Phase 5E** | Already the designated placeholder |
| **M-36** Object-store driver | **Needs ADR** | MSRV check for `object_store` crate required first |
| **M-37** Recipe chaining | **Needs ADR** | Phase 5D placeholder is closest; four viable shapes |
| **M-38** Scenario key translation | **Needs ADR** | No existing phase placeholder; model-level `aliases:` is cleanest |
| **M-39** MAJ-3 cross-coord dep graph | **Needs ADR** | Five open architectural questions documented in `cross-coord-dep-graph.md` |
| **M-40** Filter-formula unification | **Phase 3I** | Committed; design decision on cross-coord in filters required |
| **M-41** Goal-seek / Monte Carlo | **Needs ADR** | Goal-seek may fit Phase 3I; optimization belongs in Tessera |
| **M-42** Multi-frequency Time dimensions | **Needs ADR** | MC2036 (`time_dim_count <= 1`) must be relaxed; high-cost change |
| **M-43** Scenario fallback chain | **Needs ADR** | Three viable shapes; coupled to M-39 |
| **M-44** Multi-axis sweep | **Needs ADR** | Exponential blowup without constraint handling; optimization library needed |
| **M-45** Cross-model comparison | **Needs ADR** | Requires shared coordinate key agreement; no current mechanism |
| **M-46** Chart/visualization spec surface | **Phase 6B** | Principled separation: Mosaic provides data, Phase 6B renders |
| **M-47** Carry-forward × is_future() scope | **Needs ADR** | Unconditional carry-forward is semantically wrong for past-gaps; needs scope system extension |
| **M-48** Rule scope system (AllLeaves only) | **Needs ADR** | `compile.rs:252–256` compiles only `"AllLeaves"`; no `FutureLeaves`, `InputScope`, etc. — prerequisite for M-47 and M-12 conditional carry-forward |

---

## Section 4 — Sequencing recommendation

Assume ~3 phases of delivery capacity before re-audit. The following three phases close the most gaps at highest impact.

### Recommended Phase 1: Phase 6A.2 (agent surface + polish)

**Rationale:** M-1 (write-log replay) is the load-bearing fix. Without it, no agent loop that writes a cell then reads downstream effects works. It is also the gating prerequisite for **verifying** every other gap closure — once 6A.2 ships, agents can finally test "write an indicator cell, see the formula evaluate it." M-2 (trace formula), M-3 (transform recipe compatibility), M-5 (trace at consolidated), and M-6 (sweep reload) are bugs or acknowledged debt that degrade the 6A value proposition daily. M-4, M-7, M-8, M-9, M-27–M-34 together close the budget-reallocation, diff, and grouping workflows that the email-matchback audit showed Python had to cover.

This phase requires no ADRs and no new dependencies. Fourteen to sixteen gaps close directly; several more become verifiable for the first time.

**Confidence:** High. All M-1 through M-10 and M-27–M-34 have clear-path fixes. The sweep YAML reload (M-6) requires a structural change to `sweep.rs` but the approach is documented (compile once before the loop). No design uncertainty.

### Recommended Phase 2: Phase 3I (formula language completion)

**Rationale:** Phase 3I is already committed. It closes M-11 (indicator generation — the single largest Python workaround in the calculate layer), M-15 (math primitives — gates entire FP&A/demand-planning domains), M-21 (ifs/switch — ergonomics), and M-40 (filter-formula unification — prerequisite for M-8's metric scoping and M-9's group-by). Additionally, string literal support from Phase 3I is the prerequisite for `current_element(DimName)` which makes M-11 fully expressible in YAML. Phase 3I's `norm_inv` addition also closes half of M-17 (sigma validation for norm_cdf).

The dependency ordering matters: Phase 6A.2 must ship before Phase 3I so that agents can immediately verify new formula capabilities via write-then-query loops. Doing 3I before 6A.2 ships capabilities that can't be tested end-to-end.

**Confidence:** High. The formula-language-expansion.md research note specifies all 9 missing math primitives and their edge-case semantics (div-by-zero → Null, ln(0) → Null, etc.). The parser architecture (add one match arm + one Expr variant per function) is proven at 3E–3H.

### Recommended Phase 3: Formula amendment bundle (3G + 3H + 3F amendments)

**Rationale:** Rather than a single large phase, bundle three amendments that each touch only `mc-model` (no kernel change, no new dep):

- **3G amendment:** M-16 (multi-key lookup tables) + M-18 (avg_over family). Both are additive schema changes with one compiler path each.
- **3H amendment:** M-17 (predict arity validation + sigma guard). Validation-only; no new schema.
- **3F amendment:** M-12 (extrapolate_last_value) + the non-anchor-conditional path. The anchor-conditional design (M-47) is deferred to ADR; ship the unconditional `extrapolate_last_value(measure)` which is useful for future-gap filling and can be composed with `if(is_future(), ...)` by the author.

This bundle closes 5–6 gaps and eliminates the remaining ~50 lines of Python carry-forward / seasonality-table boilerplate. Scope risk is low because none of these touch mc-core.

**Alternative to this phase:** Phase 5D (xlsx driver + year-blocked layout). This closes the largest single Python workaround (~220 lines for `flatten_ltd_comparison.py`) but requires a new crate dep (`calamine`), new schema blocks, and more implementation surface. Higher value ceiling; higher scope risk. Recommend the amendment bundle first, then 5D as the fourth phase if/when the re-audit confirms the formula gaps are closed.

**Confidence:** Medium-high. The 3G/3H amendments are well-specified. The 3F amendment (extrapolate_last_value) needs a one-page design note clarifying partial-window behavior before implementation to avoid shipping the wrong semantic (should it return Null at t=0, or the value at t=0?).

---

## Section 5 — What the audits missed

I scanned the following files not directed by A/B/C lens prompts: `crates/mc-model/src/lint.rs`, `crates/mc-model/src/compile.rs`, `crates/mc-tessera/src/sidecar.rs`, `crates/mc-cli/src/` (full file list), `mosaic-plugin/skills/`. Four findings not surfaced in any audit:

### S-1: Rule scope system is `"AllLeaves"` only — no scope extensibility path

`crates/mc-model/src/compile.rs:252–256`:
```rust
let scope = match rule.scope.as_str() {
    "AllLeaves" => Scope::AllLeaves,
    _ => return Err(EngineError::Internal("compile: validator missed an unknown rule scope")),
};
```

The schema's `scope` field is a free string (`schema.rs:191–192`), but the compiler rejects any value other than `"AllLeaves"` with an `Internal` error. The validator must also accept only `"AllLeaves"` (per the validator at `validate.rs`) or this error would fire on valid models. This means `FutureLeaves`, `ConditionalScope`, `InputScope`, and similar scope extensions — which M-47 (carry-forward × is_future) and M-12 (conditional carry-forward) depend on — cannot be shipped without both a validator change AND a compiler change AND a kernel `Scope` enum extension. The calculation audit mentioned scope extension as a possible fix shape for M-47 but did not flag that the current scope system has no extension mechanism. This is a prerequisite dependency the phase planning must account for.

**Recommendation:** Any ADR for M-12/M-47 must include a scope-system extension plan. Without it, the "FutureLeaves scope" fix shape is not implementable.

### S-2: `mosaic-plugin/skills/` now has 9 skills but Phase 4A shipped 6

The `mosaic-plugin/skills/` directory contains: `assessment`, `authoring`, `debugging`, `domain-schemas`, `fitted-models`, `formulas`, `import`, `schema-design`, `testing` — 9 skills. Phase 4A's completion report documented 6 skills. None of the audits checked whether the 3 new skills (`assessment`, `fitted-models`, `import`) are consistent with the current Phase 3H YAML schema (fitted_models blocks, calibration_maps) or whether they reference diagnostic codes that were renumbered or retired since 4A. The plugin is the primary LLM authoring interface; stale skill content is a silent correctness risk for Phase 4B-style adapter runs.

**Recommendation:** A plugin skill consistency check should be part of Phase 6A.2 acceptance gates or a 4A.1 amendment, not deferred to Phase 4.

### S-3: `mc-cli` has no `retry.rs` or quarantine retry verb — gap is confirmed missing, not just unverified

The audit (A:G-OPEN-11) noted `mc tessera retry-quarantine` is absent. The independent scan of `crates/mc-cli/src/` confirms: `diff.rs, main.rs, mcp.rs, query.rs, sweep.rs, tessera.rs, trace.rs, transform.rs, whatif.rs, write.rs` — no retry verb file. This is not uncertainty; it's a confirmed missing deliverable (M-24) from ADR-0010 Decision 7.

### S-4: `mc-model/src/lint.rs` lint rules do not cover Phase 3E–3H additions

`lint.rs` module doc lists MC3001–MC3011 (with MC3008 retired). None of the 10 active lint rules cover:
- Rules that use `predict()` with more features than the declared model's coefficient count (this should be MC3012 or higher, not just a validation error — a lint warning at "you have 6 features but the model was fitted on 7" is actionable)
- Rules that use `actual_ref()` in a model with no `actuals_element` declared on the Scenario dimension (currently a runtime failure, not a lint warning)
- Fitted models declared but never referenced in any rule body (analogous to MC3009/MC3010 for measures)
- `norm_cdf` called with a literal sigma ≤ 0 (already planned as MC1021 per calculation audit B:G-OPEN-9 but not in lint.rs)

These are all "wrong but not invalid" patterns that the linter is the right home for. No audit flagged the gap in lint coverage post-3H.

---

## Section 6 — Confidence calibration

### Phase 6A.2 confidence

**Evidence making me confident:**
- M-1 (write-log replay) has a clear implementation sketch in the 6A.1 completion report: read `writes.jsonl` after `apply_canonical_inputs`, replay in timestamp order. No architectural decisions required.
- All other M-4–M-9, M-27–M-34 gaps are confirmed by reading the cited source lines. The fixes are proportional to their diagnoses.
- The Phase 6A acceptance gates and test suite already cover the affected verbs; regression risk is bounded.

**Evidence that would change the recommendation:**
- If `.tessera/writes.jsonl` turns out to have a partial-replay correctness issue (e.g., replaying a write for a cell that no longer exists in the current model version), the implementation would need a compatibility check that is not yet designed.
- If M-9 (group-by) requires significant changes to the query planner, it may be demoted to a separate phase.

**Follow-up audit that would resolve uncertainty:** None needed for M-1–M-3; they are bugs. For M-9, a 30-minute spike to check whether `--group-by` can be implemented as a post-query aggregation step (without modifying the cube iteration path) would confirm the scope.

---

### Phase 3I confidence

**Evidence making me confident:**
- `formula-language-expansion.md` specifies all 9 math primitives, their edge-case semantics (ln(0) → Null, sqrt(-1) → Null, mod(a,0) → Null), and their Expr variants.
- The Phase 3E–3H arc demonstrated that adding parser cases + Expr variants + kernel evals is a reliable, bounded process. Phase 3H added `predict`, `calibrate`, `exp`, `norm_cdf` — 4 complex additions — in one phase. Phase 3I's 9 additions are simpler (pure math, no YAML blocks).
- String literal support (needed for M-11 `is_element`) is the only novel type in Phase 3I. The formula parser currently has no string type; adding it requires a `ScalarValue::Str` variant in the eval path. This is a kernel-adjacent change that needs careful scoping.

**Evidence that would change the recommendation:**
- If `ScalarValue::Str` turns out to require changes to `mc-core`'s storage or consolidation logic (the kernel is currently `f64`-only for cell values), the `current_element()` / string comparison path could require a kernel amendment, elevating Phase 3I's scope. If this is the case, `is_element(DimName, "Element")` returning `0.0`/`1.0` (no string involved) is the safer interim path for M-11.

**Follow-up audit:** A 2-hour design spike on `ScalarValue::Str` propagation through `Cube::read` and `Cube::write` before Phase 3I implementation begins would bound this risk.

---

### Formula amendment bundle (3G + 3H + 3F) confidence

**Evidence making me confident:**
- M-16 (multi-key lookup) is purely additive: `key_dimensions: Vec<String>` alongside existing `key_dimension: String` is backward-compatible; the `lookup()` function gains variadic dimension arguments.
- M-17 (predict arity + sigma validation) is a validator-only fix: read the feature count from the rule body, compare to the coefficient count in the fitted model block. No eval change.
- M-18 (avg_over family) follows the exact pattern of `sum_over`: one parser case, one `Expr` variant, one eval dispatch in `cube.rs`. Phase 3G already proved this pattern.
- M-20 (output_bound) is a schema field addition + two lines in the eval path: `clamp(result, bound.min, bound.max)`.

**Evidence that would change the recommendation:**
- M-12 (extrapolate_last_value) — the partial-window behavior at `t=0` is unspecified. If the semantics question (return Null vs. return the value at t=0) is not resolved before implementation, this will ship with an untested boundary. A one-page design note must precede implementation.
- If the 3G amendment for M-16 multi-key lookup requires changes to the `DependencyGraph` (because a multi-key lookup introduces a cross-dimensional read that the single-coord dep graph doesn't model), this becomes coupled to M-39 (MAJ-3) and the scope balloons. Verify whether `lookup("table", DimA, DimB)` is a same-coord read (it is — the result is stored at the target cell's coord) before committing to 3G amendment scope.

**Follow-up audit:** None specifically required, but the Section 5 finding S-1 (rule scope system is AllLeaves-only) is a prerequisite check for M-12's conditional carry-forward variant — confirm whether `extrapolate_last_value` can be expressed without a scope extension before implementing it.

---

*End of master-gap-report.md.*
