# Phase 4D Handoff — Verbose CLI Mode

**Status:** Proposed (next to start)
**Date:** 2026-05-09
**Predecessor:** Phase 7A.6 (complete), Phase 6E (complete)
**Estimated effort:** 2–3 sessions
**Crate(s) touched:** `mc-cli` only (no new crates, no kernel changes)

---

## What this phase does

Add a `--verbose` / `-v` flag to the Phase 6A CLI verbs that enriches output with **prose summaries** built from measure `description:` fields. This makes CLI output human-friendly for operators who want to understand results in context, without requiring the full `mc-narrative` crate.

**Before (current):**
```
$ mc model query --where "Time=Q1_2025,Market=Houston" --show Spend,Clicks
Spend: 15000.0
Clicks: 12500.0
```

**After (with `--verbose`):**
```
$ mc model query --where "Time=Q1_2025,Market=Houston" --show Spend,Clicks --verbose
Spend: 15,000.00
  Marketing dollars allocated to a channel/market/period (USD). Current value: $15,000.00

Clicks: 12,500.00
  Derived click volume; computed as Spend / CPC at the leaf level. Current value: 12,500
```

---

## Scope (what to build)

### 1. Add `--verbose` / `-v` flag to these verbs:

| Verb | What verbose adds |
|---|---|
| `query` | After each measure value, print its `description:` (with optional `{value}` substitution) |
| `whatif` | For each changed cell in the delta, print the measure description explaining what the measure represents |
| `trace` | At the root of the trace tree, print the target measure's description |
| `sweep` | For the swept metric, print its description |
| `diff` | For each diffing measure, print its description |
| `write` | After confirming the write, print what the measure represents |

### 2. `{value}` placeholder substitution

If a measure's `description:` field contains `{value}`, replace it with the actual cell value formatted appropriately:
- F64 values: format with comma separators, 2 decimal places for currency/percent, 0 for counts
- Null values: render as "—" or "no value"

Example:
```yaml
description: "Marketing spend for this period: {value} USD."
```
Renders as: `Marketing spend for this period: $15,000.00 USD.`

### 3. Text output only

Verbose mode applies to `--format text` output only (the default). JSON and CSV output are machine-readable and should NOT be modified by verbose — the description data is already available via `mc model inspect` for programmatic consumers.

### 4. Graceful degradation

If a measure has no `description:` field (it's `Option<String>`), verbose mode simply omits the description line for that measure. No error, no placeholder text like "No description available."

---

## What NOT to build

- **No new crate.** This lives entirely in `mc-cli`.
- **No `mc-narrative` dependency.** Verbose mode is simple string formatting, not template evaluation.
- **No changes to JSON/CSV output.** Verbose is text-mode only.
- **No changes to `mc-core` or `mc-model`.** The `description` field already exists on `ParsedMeasure` in the schema. Just read it.
- **No new tests in `mc-core`.** All tests are CLI integration tests (output string matching).
- **No schema_version bump.** Verbose doesn't change the JSON envelope.

---

## Implementation path

### Step 1: Add the flag to argument parsing

Each verb's `parse()` function in `crates/mc-cli/src/{query,whatif,trace,sweep,diff,write}.rs` needs:

```rust
"--verbose" | "-v" => verbose = true,
```

Add a `verbose: bool` field to whatever options struct each verb uses. Default: `false`.

### Step 2: Thread the model's measure descriptions to the output formatter

The verbs already load the model (they need it for query/eval). The `ParsedMeasure` struct has `pub description: Option<String>`. When `verbose` is true and the output format is text:

1. After printing a measure's value, check if the measure has a description
2. If yes, print it on the next line, indented (2 spaces)
3. If the description contains `{value}`, substitute the formatted cell value

### Step 3: Format values nicely for verbose prose

Create a small helper in `mc-cli` (not `mc-core`):

```rust
fn format_value_for_prose(value: f64, unit: Option<&str>) -> String {
    // Format with comma separators
    // Add $ prefix for currency
    // Add % suffix for percent
    // Round to 2 decimals for currency/percent, 0 for counts
}
```

The measure's `unit` field (already in `ParsedMeasure`) drives formatting: `"currency"`, `"percent"`, `"number"`.

### Step 4: Integration tests

Add tests that:
1. Run `mc model query --verbose` on the Acme demo cube and verify descriptions appear
2. Run `mc model query --verbose` on a fixture without descriptions and verify clean output (no crash, no "no description" noise)
3. Run `mc model query --format json --verbose` and verify JSON is unchanged (verbose is text-only)
4. Verify `{value}` substitution works correctly

---

## Files to modify

| File | Change |
|---|---|
| `crates/mc-cli/src/query.rs` | Add `--verbose`/`-v` flag; enrich text output |
| `crates/mc-cli/src/whatif.rs` | Same |
| `crates/mc-cli/src/trace.rs` | Same |
| `crates/mc-cli/src/sweep.rs` | Same |
| `crates/mc-cli/src/diff.rs` | Same |
| `crates/mc-cli/src/write.rs` | Same |
| `crates/mc-cli/src/lib.rs` or a new `format_helpers.rs` | Shared prose formatting helper |
| `crates/mc-cli/tests/` | Integration tests for verbose output |

---

## Reference: Acme demo cube measure descriptions

All 11 measures already have descriptions (from `models/acme_marketing_finance/model.yaml`):

| Measure | Description |
|---|---|
| Spend | Marketing dollars allocated to a channel/market/period (USD). |
| CPC | Cost per click (USD/click); rolls up as a Spend-weighted average. |
| Clicks | Derived click volume; computed as Spend / CPC at the leaf level. |
| Impressions | Total ad impressions served in a channel/market/period. |
| CVR | Conversion rate (conversions/click); Spend-weighted average on rollup. |
| Conversions | Derived conversion count; Clicks × CVR. |
| Revenue | Derived revenue; Conversions × AOV. |
| AOV | Average order value (USD/conversion); Spend-weighted average. |
| ROAS | Return on ad spend; Revenue / Spend. Derived ratio. |
| Close_Rate | Close rate of leads into customers; Spend-weighted average. |
| COGS_Rate | Cost of goods sold as % of revenue; Spend-weighted average. |

---

## Acceptance criteria

1. `mc model query --verbose` produces prose-enriched text output with measure descriptions
2. `mc model query -v` works as shorthand
3. `{value}` substitution renders correctly with proper number formatting
4. Measures without descriptions degrade gracefully (no output for that line)
5. `--format json` and `--format csv` are unaffected by `--verbose`
6. All 6A verbs (`query`, `whatif`, `trace`, `sweep`, `diff`, `write`) support the flag
7. `cargo test --workspace` passes
8. `cargo clippy --all-targets --workspace -- -D warnings` passes
9. No new dependencies added

---

## Cross-links

- **MASTER_PHASE_PLAN:** Phase 4D row
- **ADR-0008:** Phase 4 (LLM authoring + plugin ecosystem) — 4D is a small leaf under this umbrella
- **Phase 6A verbs:** The target verbs that get the flag
- **Schema field:** `ParsedMeasure::description` in `crates/mc-model/src/schema.rs`

---

**End of handoff. This phase is small, well-scoped, and self-contained. Ship it in 2-3 sessions.**
