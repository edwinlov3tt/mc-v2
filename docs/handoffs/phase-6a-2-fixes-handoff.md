# Phase 6A.2 Handoff — Agent Surface Correctness Patch

> **Audience:** the Claude Code instance that implements Phase 6A.2.
> **You inherit `main` at `bbe9a41` (`docs: refresh ... through Phase 6A.1`), 731 / 0 / 5 tests.**
>
> **This phase makes the agent surface actually trustworthy.** Phase 6A
> shipped the 7 verbs + MCP server; Phase 6A.1 closed the silent-correctness
> bugs surfaced in code review. A subsequent multi-instance audit
> (Sonnet × 3 lenses + Codex independent verification under
> `docs/audits/`) found that several agent-facing paths still don't behave
> the way the 6A handoff committed to. The biggest is **write-then-read
> coherence**: `mc model write` writes to `.tessera/writes.jsonl` but
> `mc model query` ignores it, so any "write a cell, then check downstream
> effects" agent loop returns stale data. Phase 6A.2 fixes that and
> ~6 sibling correctness issues. **No new capabilities, no kernel
> changes, no new design** — every item has a confirmed reproduction
> from the audit and a clear-path fix.
>
> **Hard rule:** Phase 6A.2 modifies only `mc-cli/` and one file in
> `mc-tessera/`. It does NOT touch `mc-core`, `mc-fixtures`, `mc-model`,
> `mc-recipe`, `mc-drivers`, or `mosaic-plugin/`. It does NOT add new
> dependencies (with one explicit exception — `ureq` from
> Should-Fix #6, which is already in the workspace and already a
> transitive dep of mc-cli). Toolchain stays at Rust 1.78.
>
> **Scope discipline:** every must-fix has a verified reproduction
> command in §"Block 1". Run the repro BEFORE you fix it (to confirm
> the audit's claim) and AFTER you fix it (to confirm the fix works).
> If a should-fix item turns out to be larger than its description,
> drop it and note the deferral in the completion report. Don't expand
> scope.

---

## The one paragraph you must internalize

The post-6A audit (4 reports under `docs/audits/`, totaling ~2,400
lines, with Codex performing independent verification at HEAD
`bbe9a41`) found 48 candidate gaps. After Codex's verification, ~12
are actionable as bug-fixes-without-design and ~10 require ADRs
before scoping. Phase 6A.2 ships the actionable bucket. Everything
else is deferred — see §"Out of Scope" and §"Email-matchback gaps
NOT in 6A.2" for the explicit list of what NOT to touch and why.
Codex verified 5 issues with reproduction commands; those are your
must-fix list. Codex also identified 4 findings that Sonnet got
wrong or overstated (notably the "`serde_json` implicit dep" claim
is **false** — `mc-cli/Cargo.toml:21` explicitly declares it). Don't
chase those. Read
[`docs/audits/codex-phase-6a-followup.md`](../audits/codex-phase-6a-followup.md)
§3 for the full verification table; §"What NOT to fix" below summarizes.

---

## Production-quality framing

**This is a no-second-pass phase.** Every fix below ships once and
should not be revisited. To make that possible:

1. **Pre-empted decisions.** Each must-fix item below has a
   "Decision Matrix" subsection that names the likely walls and
   binds the decision in advance. Follow the matrix. Don't deviate
   without filing a SPEC QUESTION.
2. **Verified APIs.** I (the PM) walked the public surfaces of
   `mc-model`, `mc-recipe`, and `mc-cli` to confirm every API
   referenced in the fix instructions actually exists at HEAD
   `bbe9a41`. File paths and line numbers are accurate as of that
   commit.
3. **Backward compat inventory.** §"Backward Compat Inventory" below
   lists every JSON envelope shape, CLI flag, and MCP tool schema
   you must NOT break (and the 2 you ARE breaking, with
   `schema_version` bumps).
4. **No hidden scope.** §"Email-matchback gaps NOT in 6A.2"
   explicitly closes every audit-surfaced gap that doesn't fit;
   you should not be tempted by any of them.

If a wall isn't in the matrix, file a SPEC QUESTION using the
format in §"SPEC QUESTION format". Do not guess. The cost of one
SPEC QUESTION round-trip is far less than shipping a fix that
needs another fix.

---

## Block 1 — Must-fix (P0 + 6 P1s)

Each item below has a Codex-verified reproduction. Run the repro,
fix the bug, re-run the repro, write a regression test that pins
the new behavior.

### 1.1 — Write-log replay (P0)

**The bug.** `mc model write` appends to `.tessera/writes.jsonl`,
but `load_model` in `query.rs` only reads YAML + canonical inputs;
post-hoc writes are silently ignored on read.

**Reproduction (Codex-verified):**
```bash
tmp=$(mktemp -d /tmp/mosaic-write-replay.XXXXXX)
cp crates/mc-model/examples/acme.yaml \
   crates/mc-model/examples/acme.inputs.csv "$tmp"/
mc model query "$tmp/acme.yaml" --coord \
  "Scenario=Baseline,Version=Working,Time=Jan_2026,\
Channel=Paid_Search,Market=Tampa,Measure=Spend" --format json
# Returns 10500
mc model write "$tmp/acme.yaml" --coord \
  "Scenario=Baseline,Version=Working,Time=Jan_2026,\
Channel=Paid_Search,Market=Tampa,Measure=Spend" --value 999 --format json
# Returns "after": 999
mc model query "$tmp/acme.yaml" --coord \
  "Scenario=Baseline,Version=Working,Time=Jan_2026,\
Channel=Paid_Search,Market=Tampa,Measure=Spend" --format json
# Currently: returns 10500 (BROKEN). Expected after fix: 999.
```

**The fix (binding: follow process-notes Rule 9 verbatim).** The
four-source state model in
[`docs/process-notes.md`](../process-notes.md) §9 already specifies
which verbs replay which sources:

- **Replay all four sources (current operational reality):**
  `mc model query`, `whatif`, `trace`, `query --output`, `diff`
  (current side).
- **Replay only the first three (reproducibility / pristine state):**
  `mc model test`, `sweep`, `mc tessera apply`.

Implement a **`LoadPolicy` enum** in `query.rs::load_model` (or a
new small `loader.rs` module if cleaner):

```rust
pub enum LoadPolicy {
    /// Replay YAML + canonical_inputs + .tessera/audit.jsonl + .tessera/writes.jsonl
    CurrentReality,
    /// Replay YAML + canonical_inputs + .tessera/audit.jsonl only
    Reproducible,
}

pub fn load_model_with_policy(path: &str, policy: LoadPolicy) -> Result<LoadedModel, LoadModelError> { ... }

/// Backwards-compat shim that defaults to CurrentReality (the most-common case).
pub fn load_model(path: &str) -> Result<LoadedModel, LoadModelError> {
    load_model_with_policy(path, LoadPolicy::CurrentReality)
}
```

Then update each verb's call site:
- `query.rs`, `whatif.rs`, `trace.rs`, `diff.rs` → `LoadPolicy::CurrentReality` (default)
- `sweep.rs`, `mc model test` (in `main.rs`) → `LoadPolicy::Reproducible`
- `mc model write` reads with `CurrentReality` so subsequent writes
  layer correctly on existing patches.

**Replay shape.** Read `.tessera/writes.jsonl` line-by-line after
`apply_canonical_inputs`. Each line is one append-only write event;
apply in file order via `cube.write(...)`. If the log is missing
or empty, skip silently (this is the normal case for fresh models).
If a line fails to deserialize, return a typed
`LoadModelError::WriteLogCorrupt { line_number, message }` that
maps to exit code 3.

**Decision Matrix (pre-empts likely SPEC QUESTIONs):**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: A `writes.jsonl` entry references a coord whose element no longer exists in the YAML (YAML edited post-write) | **Error with `LoadModelError::WriteLogStaleCoord { line_number, coord_string, missing_element }`. Exit code 3.** | The user explicitly wrote that cell; silently dropping is a correctness violation. The error gives an LLM enough context to either restore the YAML element or curate the writes.jsonl. |
| W2: A `writes.jsonl` entry references a coord that's now a derived measure (was input when written) | **Let the kernel reject (`EngineError::DerivedNotWritable`); wrap the inner error inside `LoadModelError::WriteLogReplayFailed { line_number, inner }`. Exit code 3.** | The kernel error is already specific. Wrapping preserves provenance for debugging. |
| W3: Mid-file corruption (line 47 of 100 is malformed JSON) | **Error at first bad line (don't roll back lines 1–46). Lines 1–46 stay applied in-memory for this run; the next invocation will encounter the same line and fail again until the user fixes the file.** | JSONL is line-independent by design. Don't write rollback logic — adds complexity, no clear benefit. |
| W4: Where does `LoadPolicy` enum live? | **New `crates/mc-cli/src/loader.rs` module.** Re-export `LoadPolicy`, `load_model`, `load_model_with_policy` from there. `query.rs::load_model` becomes a thin re-export shim for backward compat (it's used by other CLI files). | `query.rs` is already the largest CLI file; adding the policy logic inline pushes it past readable. New module is the right shape. |
| W5: Should `writes.jsonl` support compaction (squash duplicate writes to same coord)? | **NO.** Out of scope. JSONL is append-only by design; replay applies in order; last-write-wins emerges naturally. Compaction is a future feature. | Don't paint into a corner — keeping the file append-only-and-replayed-in-order is the simplest mental model. |
| W6: What about the 5 `mc tessera` verbs (apply / status / etc.)? Should they replay writes? | **NO** — Tessera verbs all operate on the canonical model state, NOT post-hoc writes. `mc tessera apply` uses `Reproducible` policy. Document in `loader.rs` doc comment. | Per process-notes Rule 9. |
| W7: Should `LoadPolicy::CurrentReality` also replay `.tessera/audit.jsonl` (Tessera import audit log)? | **YES** — process-notes Rule 9 says all four sources for current-reality verbs. If the audit log replay is non-trivial, scope it as a separate function `apply_tessera_audit(&mut Cube, &Path)` invoked between `apply_canonical_inputs` and the writes-jsonl replay. If `mc-tessera` already provides this (check `mc-tessera/src/sidecar.rs` and the existing `mc tessera apply` flow), reuse it. | Process-notes Rule 9 is binding. |
| W8: How are coords stringified in `writes.jsonl` today? Round-trip-safe? | **Verify before writing replay code.** Read `crates/mc-cli/src/write.rs` lines around 175-194 (the JSONL append). Confirm the coord serializer in `write.rs` matches a deserializer you can reuse (likely `parse_coord_string` from `query.rs`). If mismatch, that's a separate bug — fix in this item and add a regression test. | This is the load-bearing assumption for replay correctness. |

**Edge cases that REQUIRE regression tests (in addition to the 3 below):**
- Empty `writes.jsonl` file (zero lines) — should be silent no-op.
- `writes.jsonl` exists but is empty (0 bytes) — should be silent no-op.
- Two writes to the same coord — second wins; `query` returns the second value.
- `writes.jsonl` line that writes a measure not declared in the YAML — wraps W1 and tests it.

**If you hit a wall not in this matrix:** file a SPEC QUESTION. Common candidates: the audit log replay shape, schema-version handling for forward-compat write entries.

**Regression tests (3 minimum required + 4 edge-case tests above = 7 total):**
1. `test_query_reflects_post_hoc_write`: write 999, query returns 999.
2. `test_test_ignores_post_hoc_writes`: same setup, `mc model test`
   does not include the write (goldens still pass against canonical).
3. `test_write_log_corrupt_returns_exit_3`: corrupt the JSONL,
   confirm typed error + exit code.
4. `test_write_log_empty_file_silent_noop`.
5. `test_write_log_two_writes_same_coord_last_wins`.
6. `test_write_log_stale_element_returns_exit_3` (W1 case).
7. `test_write_log_to_derived_measure_returns_exit_3` (W2 case).

---

### 1.2 — Trace formula field shows debug AST (P1)

**The bug.** `crates/mc-cli/src/trace.rs:207` uses
`format!("{:?}", expr_summary.op)`, producing `"formula": "Mul"`
instead of `"formula": "Spend / CPC"`. Trace is functionally
unusable for LLM explanation.

**Reproduction:**
```bash
mc model trace crates/mc-model/examples/acme.yaml --coord \
  "Scenario=Baseline,Version=Working,Time=Q1_2026,\
Channel=Paid_Media,Market=Florida,Measure=Conversions" --format json
# Currently: "formula": "Div" or similar Debug output
# Expected: "formula": "Spend / CPC"
```

**The fix.** Carry the authored formula string from the rule body
into the trace output via CLI-side rendering — no kernel change.

**Verified API (HEAD `bbe9a41`):** `mc_model::inspect::summarize(&ValidatedModel) -> ModelSummary`
exists at `crates/mc-model/src/inspect.rs:121` and is publicly
re-exported via `mc_model::inspect_text` and friends at
`crates/mc-model/src/lib.rs:60`. The `ModelSummary` struct exposes
the rendered formula string for each rule (this is what
`mc model inspect` prints). The implementer can either:

- **Path A (preferred):** call `mc_model::inspect::summarize()` once
  during `load_model_with_policy` and stash a
  `HashMap<RuleId, String>` (or `HashMap<MeasureId, String>` if
  rules are keyed by their target measure) on `LoadedModel`.
  Trace consults the map.
- **Path B (alternative):** walk the `ValidatedRule.body` AST in a
  small helper inside `mc-cli/src/trace.rs::render_formula(&Expr,
  &Refs) -> String`. Mirror the minimal-paren rules in
  `mc-model/src/formula.rs`'s round-trip serializer.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: `mc_model::inspect::summarize` returns rendered formulas, but maps them by what key? Rule name? Measure ID? Output coord? | **Inspect the `ModelSummary` struct shape (`mc-model/src/inspect.rs`) before deciding the HashMap key. If it keys by measure name, use measure name. If keys by rule name, use rule name. The trace's `ExprSummary` should carry whichever key is present at the cube level.** Pick the path that requires zero `mc-model` API changes. | API stability rule: don't extend `ModelSummary` unless absolutely required. |
| W2: A consolidated coord (rollup) has no rule body — what goes in the trace's `formula` field? | **`null` (JSON null).** Combined with `source: "consolidation"` and `child_count`, the agent has full info. | Stable schema; field always present; null when not applicable. Forward compat for strict-schema agents. |
| W3: An input cell has no rule and no formula — what goes in `formula`? | **`null`.** Same as W2. | Consistency. |
| W4: Path A uses `summarize` which loads ALL rules even if trace touches only one. Performance concern? | **Path A is fine.** Acme has 5 rules; production cubes have at most ~50–200 rules. Per-load summarize cost is sub-ms. | Don't optimize prematurely. |
| W5: Both Path A and Path B work. Which to ship? | **Path A unless it requires extending `ModelSummary`'s public API.** If you find that `summarize` doesn't expose what you need without modification, fall back to Path B and write the helper. | Simpler integration. |
| W6: Path B's `render_formula` — should it match `formula.rs`'s round-trip serializer byte-for-byte? | **YES** — agents compare formula strings; drift between inspect output and trace output would confuse LLM reasoning. If implementing Path B, copy the paren rules verbatim from `formula.rs`. Better: factor out the render function in mc-model (touches mc-model, requires SPEC QUESTION first). | Single source of truth for formula rendering. |

**If you hit a wall not in this matrix:** file a SPEC QUESTION.
Common candidate: if `ModelSummary` doesn't expose a public formula-
string field, decide whether to extend the struct (touches mc-model
public API; SPEC QUESTION required) or write the local renderer.

**Regression tests (3 required):**
1. `test_trace_emits_authored_formula_for_derived_cell` — assert
   `formula` field equals the round-trip-rendered formula for a known
   Acme rule (e.g., `Revenue = Customers * AOV`).
2. `test_trace_consolidated_coord_has_null_formula` — formula field
   is `null` (not the string `"null"`, not omitted).
3. `test_trace_input_cell_has_null_formula`.

---

### 1.3 — Trace JSON has duplicate `inputs` keys (P1, Codex-only finding)

**The bug.** `trace.rs:216-225` keys each child input by measure
name. Consolidated trace at e.g. `Spend[Q1_2026, Florida]` produces
27 children all keyed as `"Spend"`. JSON parsers retain only the
last; agents see 1 child instead of 27.

**Reproduction:**
```bash
mc model trace crates/mc-model/examples/acme.yaml --coord \
  "Scenario=Baseline,Version=Working,Time=Q1_2026,\
Channel=Paid_Media,Market=Florida,Measure=Spend" --format json | \
  python3 -c "import json,sys; d=json.load(sys.stdin); \
print('children:', len(d.get('inputs', {})))"
# Currently: prints "children: 1" (parser deduped)
# Expected after fix: prints "children: 27"
```

**The fix.** Change `inputs` from a JSON object to a JSON **array**
of child trace nodes, where each child carries its full coordinate.
Schema becomes:

```json
{
  "coord": "Scenario=...,Time=Q1_2026,...,Measure=Spend",
  "value": 12500.0,
  "source": "consolidation",
  "child_count": 27,
  "inputs": [
    { "coord": "Time=Jan_2026,...", "value": 4100.0, "source": "input" },
    { "coord": "Time=Feb_2026,...", "value": 4200.0, "source": "input" },
    ...
  ]
}
```

**Bonus fix bundled here (Codex M-5 correction):** in `trace.rs:139-148`,
when `Cube::trace` returns `None` at a coordinate that IS consolidated
(i.e., not a leaf), the current fallback wrongly emits
`source: "input"`. Change the fallback to detect the
consolidated case and emit `source: "consolidation"` with
`child_count` equal to the leaf-coord enumeration. The
"trace at consolidated coords always returns input" claim from the
Sonnet master report (M-5) was overstated by Codex's repro — the
real bug is this fallback path, not the primary path.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: This is a breaking change to the trace JSON envelope. Bump `schema_version` from `"1.0"` to something else? | **Bump trace's envelope to `schema_version: "1.1"`.** Document in the JSON schema that 1.0 is deprecated as of this commit. Other verbs stay at `"1.0"` (they're not changing shape). | Forward-compat for strict-schema agents. The user explicitly asked for production quality — a silent shape change would burn anyone parsing the old shape. |
| W2: Does the schema_version bump apply to ALL JSON envelopes from the trace verb, or just the top-level? | **Top-level only.** Nested input objects don't carry `schema_version` (they never did). | Single envelope per response. |
| W3: Field name — keep `inputs` (now array) or rename to `children` (semantically clearer for consolidation)? | **Keep `inputs`.** Renaming compounds the breaking change without value. The schema_version bump signals the array switch. | Minimize churn. |
| W4: Trace `--depth` interaction — does the array shape change how depth is counted? | **No interaction.** Each level is its own depth; arrays at level N contain objects whose `inputs` arrays are level N+1. `--depth 2` means render 2 levels regardless of object-vs-array. | The depth limit predates this change and stays unchanged. |
| W5: `child_count` on input cells (no children at all)? | **Emit `child_count: 0`** if `source: "input"`. Always present, always an integer. | Stable schema. |
| W6: `child_count` on derived (rule-evaluated) cells — count the dependencies? | **YES.** `child_count` = `inputs.len()` after rendering. The two fields are always consistent (asserted by a regression test). | Same field semantics across all source types. |
| W7: How deep should the recursive trace go by default? | **No change** to the default depth (currently 2 per `crates/mc-cli/src/trace.rs`). This phase doesn't touch depth handling. | Out of scope. |

**Schema 1.1 envelope shape (binding):**

```json
{
  "schema_version": "1.1",
  "coord": "...",
  "value": 12500.0,
  "source": "consolidation",   // "input" | "rule" | "consolidation"
  "child_count": 27,             // always integer; 0 for inputs
  "formula": "Spend / CPC",      // string for rule; null for input/consolidation
  "inputs": [                    // ALWAYS array; empty [] for input cells
    { "coord": "...", "value": 4100.0, "source": "input", "child_count": 0, "formula": null, "inputs": [] },
    ...
  ]
}
```

Every field is always present with the correct type; never omit
fields conditionally. Strict schema = LLM-friendly schema.

**If you hit a wall not in this matrix:** file a SPEC QUESTION.
Common candidate: if changing the envelope to schema_version 1.1
breaks existing integration tests (likely — there are tests in
`agent_cli_integration.rs`), update them and add a note in the
completion report. Don't keep the old shape "for compat."

**Regression tests (4 required):**
1. `test_trace_consolidated_emits_array_with_all_children` — assert
   the array length matches the leaf count for the tested coord;
   `child_count` matches `inputs.len()`.
2. `test_trace_fallback_at_consolidated_coord_labels_consolidation`.
3. `test_trace_envelope_schema_version_is_1_1`.
4. `test_trace_input_cell_has_empty_inputs_array_and_zero_child_count`.

---

### 1.4 — MCP numeric params advertised + handled as strings (P1)

**The bug.** `mosaic.model.whatif`, `write`, and several other tools
advertise `value`, `limit`, `depth`, `preview` as `"string"` in their
JSON schema (`mcp.rs:190-266`) AND parse via `as_str_owned`
(`mcp.rs:638-650`). JSON numbers fail with
`missing required argument: value`. LLM clients send numbers
naturally and break.

**Reproduction (Codex-verified):**
```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call",\
"params":{"name":"mosaic.model.whatif","arguments":{\
"path":"crates/mc-model/examples/acme.yaml",\
"set_coord":"Scenario=Baseline,Version=Working,Time=Jan_2026,\
Channel=Paid_Search,Market=Tampa,Measure=Spend",\
"value":999,"show":"Revenue"}}}' | mc mcp
# Currently: error "missing required argument: value"
# Expected after fix: accepted; whatif runs.
```

**The fix.** For each numeric/integer parameter in `mcp.rs`:
1. Change the JSON schema descriptor from `"string"` to `"number"`
   (for `value`) or `"integer"` (for `limit`, `depth`, `preview`).
2. Use a coercing accessor that accepts both `JsonValue::Number`
   and `JsonValue::String` (the latter for backwards compat with
   any client that was working around the bug). The accessor lives
   alongside `as_str_owned` in `mcp.rs`.
3. **Bonus (also Codex):** for query/whatif/trace/sweep/diff/write
   MCP responses, `structured` is currently a JSON-encoded **string**
   when it should be a parsed JSON **object**. Agents have to parse
   twice. Fix at the point where `run_cli_verb_json` populates the
   ToolOutcome — parse the captured stdout into `JsonValue` and
   embed it as `structured`, mirroring the Phase 4A pattern at
   `mcp.rs:404,412,449`.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Backward compat — should string values still be accepted, or break old clients? | **Accept BOTH number and string.** Coerce strings to numbers via a coercing accessor. Codex specifically suggested this pattern. | Production-quality. Tools advertised the wrong type for months — old clients sending strings were following the documented contract. Don't break them; just coerce. |
| W2: What about `as_str_owned`-based existing tests in `tests/mcp_smoke.rs` and `agent_cli_integration.rs` that pass string values? | **Keep them as regression tests for the compat path.** Add NEW tests that send numbers as the canonical type. Both must pass. | Tests document behavior; both behaviors must be locked. |
| W3: Should `structured` field of MCP responses become a parsed `JsonValue` for ALL Phase 6A verbs, or just the ones not yet wired? | **All 6 Phase 6A verbs:** `query`, `whatif`, `trace`, `sweep`, `diff`, `write`. The `transform` verb is item 1.5 (covered there). | Consistency. The Phase 4A `validate`/`inspect`/`lint` tools already do this (see `mcp.rs:404,412,449`). |
| W4: What if the verb's stdout is text/CSV (not JSON)? | **`structured` stays `None`** in that case. Only set `structured` to a parsed JsonValue when the verb's stdout is valid JSON. Wrap `serde_json::from_str()` in a graceful fallback (`structured: None` if parse fails). | Don't crash on text output. |
| W5: Coercion direction — accept `JsonValue::String("123")` as `i64`? Float `"123.45"` as `f64`? | **Yes for both.** The coercing accessor tries the canonical type first; if string, tries `.parse()`; on parse error, returns the existing "missing required argument" or "wrong type" error. | Standard JSON-RPC pattern. |
| W6: What about `Bool` parameters that may have been advertised as strings somewhere? | **Audit `mcp.rs:190-266` for `"string"` types that should be `"boolean"`.** Look at `mosaic.tessera.apply.dry_run` and similar. Apply the same coercion pattern. | One-stop fix. |
| W7: Should `tools/list` descriptions be updated to mention the change? | **YES** — update each affected tool's `description` field to be explicit about types. | LLM agents read these descriptions. |

**Affected MCP tools (audit before fixing):** the implementer should
grep `mcp.rs:190-266` for the string `"string"` and verify each
parameter against the actual semantic type. Codex pointed out
`whatif.value`, `write.value`, `query.limit`, `trace.depth`,
`tessera.transform.preview`. There may be more.

**If you hit a wall not in this matrix:** file a SPEC QUESTION.
Common candidate: an MCP tool that takes a JSON object as a
parameter (not a primitive) — these need different handling
(don't coerce; just accept JsonValue).

**Regression tests (5 required):**
1. `test_mcp_whatif_accepts_json_number_value`.
2. `test_mcp_whatif_still_accepts_string_value` (compat).
3. `test_mcp_query_accepts_integer_limit`.
4. `test_mcp_query_returns_parsed_structured_json_object`.
5. `test_mcp_trace_accepts_integer_depth`.

---

### 1.5 — `mc tessera transform` ignores real `mc-recipe` schema (P1)

**The bug.** `crates/mc-cli/src/transform.rs:202-326` is a bespoke
line-scanner that handles only `column_mappings`, `mappings`,
`defaults`, `json_path`, `output_columns`, `scale`. Real
`mc-recipe` YAML uses `source` + `columns` + `defaults` (per
`mc-recipe/src/schema.rs:35-97`). Any agent generating a recipe
via Phase 5B (`mosaic-importer`) and passing it to `mc tessera
transform` gets only defaults; mapped rows are dropped.

**Reproduction (Codex-verified):**
```bash
mc tessera transform \
  --source crates/mc-model/examples/acme.inputs.csv \
  --recipe crates/mc-recipe/examples/recipes/acme-csv-import.recipe.yaml \
  --preview 1 --format json
# Currently: returns [{"Scenario":"Baseline","Version":"Working"}] only
# Expected: returns full mapped row with Time, Channel, Market, Measure, value
```

**The fix.** Replace the bespoke parser with `mc_recipe::Recipe`
deserialization.

**Verified API (HEAD `bbe9a41`):** `mc_recipe::parse(&str) -> Result<Recipe, RecipeError>`
exists at `crates/mc-recipe/src/parse.rs:40`, publicly re-exported
from `crates/mc-recipe/src/lib.rs:62`. `Recipe` carries `source`,
`columns: HashMap<String, ColumnSpec>`, `defaults`, `on_error`, etc.
mc-cli already depends on mc-recipe transitively via mc-tessera; you
may need to add an explicit `mc-recipe = { path = "../mc-recipe" }`
to `mc-cli/Cargo.toml` so the import resolves cleanly.

Steps:

1. Drop the line-scanner code (`transform.rs:202-326`).
2. Add explicit `mc-recipe = { path = "../mc-recipe" }` to
   `mc-cli/Cargo.toml` if not already present (check first).
3. Call `mc_recipe::parse(&recipe_yaml_string)` in `transform.rs`.
4. Translate the parsed Recipe's `columns: HashMap<String, ColumnSpec>`
   into the column-mapping resolution logic that already runs in
   `transform.rs::apply_recipe`.
5. Preserve `defaults`, `json_path`, `scale` semantics.
6. **Codex bonus:** also fix `transform.rs::format_json_output`
   (~line 517-543) to wrap output in a `schema_version: "1.0"`
   envelope to match the Phase 6A.1 envelope discipline. Currently
   transform emits a raw array; this contradicts the data-out audit's
   claim that "all Phase 6A verbs emit envelopes." MCP transform
   tool's `structured` field should also be a parsed JSON object,
   not a string (covered by item 1.4 above).

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: The old mini-schema parser handles fields like `mappings` and `output_columns` that don't exist in `mc_recipe::Recipe`. Keep them as a fallback or remove? | **Remove entirely.** No deprecation period. The old parser was a Phase 6A shortcut; no shipped users depend on it. | Keep one source of truth for recipe shape. |
| W2: A real `mc-recipe::Recipe` fails to parse (RecipeError) — what's the CLI exit code? | **Exit code 1** (model/recipe error, per Phase 6A invariants). Error message includes the RecipeError code (MC5xxx) and message. | Consistent with `mc model validate` behavior. |
| W3: The Recipe's `source.driver` field doesn't matter for transform (transform always uses the local file path); should it be validated against the actual fetched data? | **Validate that `source.driver` is `csv-local`, `csv-https`, or `http-json`.** Other drivers don't make sense for transform (e.g., `postgres` would imply a DB connection that transform isn't built for). Error with MC5xxx if mismatched. | Explicit error messages help LLMs pick the right driver. |
| W4: What about recipes that use `json_path` (HTTP source) — does `mc_recipe::Recipe` support that? | **YES** — it's in `Recipe.source.json_path`. Re-use as-is. | Verified at `mc-recipe/src/schema.rs`. |
| W5: Transform's `--source` flag overrides the recipe's `source.path`. Keep that behavior or honor the recipe's source? | **CLI flag wins.** `--source` is the canonical override — that's how transform is designed. Document in `--help` text. | Preserves existing UX. |
| W6: Transform JSON envelope shape — same as `query`'s? | **Yes:** `{ "schema_version": "1.0", "rows": [...], "count": N }`. Match `query.rs`'s shape. | Consistency. |
| W7: Should the MCP tool's `structured` field include the rows array, or just the metadata? | **Include the full envelope** (rows + count + schema_version) parsed as JsonValue. Mirror item 1.4's pattern. | Agents need the data, not just confirmation. |

**If you hit a wall not in this matrix:** file a SPEC QUESTION.
Common candidate: a Recipe field that has runtime semantics
(e.g., a credential interpolation `${env.X}`) that the transform
verb shouldn't expand — surface explicitly.

**Regression tests (3 required):**
1. `test_transform_with_acme_recipe_emits_mapped_rows` — using
   `crates/mc-recipe/examples/recipes/acme-csv-import.recipe.yaml`.
2. `test_transform_json_envelope_has_schema_version`.
3. `test_transform_recipe_parse_error_returns_exit_1` — confirm
   exit code on malformed recipe.

---

### 1.6 — Incremental SQL appends `WHERE` unconditionally (P1)

**The bug.** `crates/mc-tessera/src/incremental.rs:145-146` does
`format!("{query} WHERE {column} > '{last_value}'")` regardless of
whether the source query already has a `WHERE` clause. Second
incremental run on a query like `SELECT * FROM events WHERE tenant_id = 7`
produces invalid SQL: `... WHERE tenant_id = 7 WHERE updated_at > '...'`.

**The fix.** Use the placeholder `{watermark}` token if present in
the query (preferred — most flexible). Otherwise, regex-detect
existing `WHERE` and append with `AND`; insert the watermark clause
**before** any `ORDER BY` / `LIMIT` / `GROUP BY` clauses. Keep the
implementation minimal — no general-purpose SQL parser. Document
in the function's doc comment that complex queries should use the
`{watermark}` placeholder.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Regex for detecting `WHERE` — what about quoted strings containing the literal `"WHERE"`? | **Document the limitation, don't fix it in 6A.2.** If the user has a query with `WHERE` inside a quoted literal, they MUST use the `{watermark}` placeholder. Doc-comment on `inject_watermark` lists this explicitly. | Don't write a SQL tokenizer. The placeholder path is the safe escape valve. |
| W2: CTE queries (`WITH x AS (SELECT ...) SELECT ...`)? | **Document as unsupported without `{watermark}` placeholder.** Same as W1 — the placeholder handles it. | Same rationale. |
| W3: What's the placeholder syntax — `{watermark}`, `:watermark`, or `?`? | **`{watermark}`** — verify the existing code already uses this syntax (Codex's reference to "Use placeholder when possible" implies it does). If not, define `{watermark}` as the canonical syntax in this fix. | Curly-brace syntax is unambiguous and matches mc-recipe's other interpolations (e.g., `${env.X}`). |
| W4: `WHERE` detection regex — case sensitivity? | **Case-insensitive match** (`(?i)\bWHERE\b`). SQL keywords are conventionally case-insensitive. | Standard SQL behavior. |
| W5: `ORDER BY` / `LIMIT` / `GROUP BY` / `HAVING` insertion point — find first occurrence? | **First occurrence of any of `(?i)\b(ORDER\s+BY|LIMIT|GROUP\s+BY|HAVING)\b`.** Insert watermark clause immediately before. | Standard SQL clause order. |
| W6: What about subqueries with their own WHERE clauses? | **Same limitation as W1, W2.** Document. Use `{watermark}` placeholder for complex queries. | Same rationale. |
| W7: Watermark value escaping — what if `last_value` contains `'` (single quote)? | **Escape via `last_value.replace("'", "''")`** (standard SQL doubling). The driver-side parameterized query path is the long-term fix; for now this is sufficient. | Matches the existing pattern in mc-tessera if any. Verify before writing. |

**If you hit a wall not in this matrix:** file a SPEC QUESTION.
Common candidate: an existing query in the codebase or test
fixtures that requires a different injection strategy.

**Regression tests (5 required):**
1. `test_inject_watermark_no_existing_where`.
2. `test_inject_watermark_with_existing_where_uses_and`.
3. `test_inject_watermark_with_order_by_inserts_before`.
4. `test_inject_watermark_placeholder_used_when_present`.
5. `test_inject_watermark_escapes_single_quote_in_value`.

---

### 1.7 — Query has no pagination / no truncation warning (P1)

**The bug.** `query.rs:180` defaults `limit = 10000`; `query.rs:203-206`
breaks the loop silently. Agents can't tell if results were
truncated.

**The fix.** Add three things to the JSON envelope:
- `limit: <integer>` (echo of the effective limit)
- `count: <integer>` (rows actually returned)
- `truncated: <bool>` (true iff loop broke before exhausting matches)
- `next_offset: <integer or null>` (if truncated, the value to pass
  as `--offset` next time; null otherwise)

Add a CLI flag `--offset <integer>` (default 0) that skips the
first N matches before applying limit. Add to MCP schema with
`"integer"` type (see 1.4 above).

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Should `--offset` work without `--limit`? | **YES.** `--offset 100` with the default `--limit 10000` means "skip 100, take 10000." | Sensible defaults. |
| W2: What if `--offset` > total matches? | **Return empty `rows: []`, `count: 0`, `truncated: false`, `next_offset: null`.** Document this as the natural behavior. | No special-casing. |
| W3: Should the envelope's `count` reflect rows-returned or total-matches? | **Rows-returned** (after offset+limit applied). Total-matches is expensive (requires fully materializing the query); skip in 6A.2. | Cheap, useful, parseable by agents. |
| W4: `next_offset` calculation — `offset + count`? Or `offset + limit`? | **`offset + count`.** If `truncated` is true, this is the same as `offset + limit`; if false, this is the position past the last match (irrelevant since `truncated: false`). | Forward-paginating agents want "where to start next." |
| W5: Should `--limit 0` mean "no limit" or "return zero rows"? | **`--limit 0` returns zero rows** (matches the literal interpretation). For "no limit," omit the flag (default 10000) or use a very high number explicitly. | Don't overload `0`. |
| W6: Streaming for very large queries? | **Out of scope for 6A.2.** Pagination is sufficient. Document as future work. | Don't bloat scope. |
| W7: Should the envelope add `next_offset` even when `truncated: false`? | **YES**, with value `null` (JSON null). Stable schema; field always present. | Forward compat for strict-schema agents. |
| W8: This expands the JSON envelope — do we bump `schema_version`? | **NO**, stay at `"1.0"` for query. The new fields are additive (existing parsers see additional fields, not changed shape). The trace verb (item 1.3) is the only schema_version bump in 6A.2. | Additive vs. breaking. |
| W9: `--offset` on MCP — same coercion as item 1.4? | **YES** — accept JSON number or coerced string, advertised as `"integer"`. Wire through the same coercing accessor. | Consistency. |

**Envelope shape (binding):**

```json
{
  "schema_version": "1.0",
  "limit": 10000,
  "offset": 0,
  "count": 42,
  "truncated": false,
  "next_offset": null,
  "rows": [ ... ]
}
```

When `truncated: true`, `next_offset` is `offset + count` as
integer.

**If you hit a wall not in this matrix:** file a SPEC QUESTION.
Common candidate: how to surface the truncation in `--format text`
(probably a footer line "...truncated; use --offset N to continue");
the JSON shape is the canonical contract.

**Regression tests (4 required):**
1. `test_query_with_low_limit_reports_truncated_true`.
2. `test_query_with_offset_skips_first_n_matches`.
3. `test_query_offset_beyond_matches_returns_empty`.
4. `test_query_envelope_includes_all_pagination_fields`.

---

## Block 2 — Should-fix (drop if scope creep)

Pull these in only if Block 1 finishes with budget remaining. Each
is well-defined and bounded; if any turns out to be a multi-day
exercise, defer to a follow-up phase and note in the completion
report.

### 2.1 — Multi-cell whatif

**File:** `crates/mc-cli/src/whatif.rs`.
**Today:** singular `--set` flag.
**Fix:** make `--set` repeatable; each `--set "coord=value"` becomes
one entry in a `Vec<(CellCoordinate, ScalarValue)>`. Apply via
`Cube::write_batch` (already in mc-core since Phase 5A).
**Test:** `test_whatif_multiple_set_flags_apply_atomically`.

### 2.2 — Single-compile sweep

**File:** `crates/mc-cli/src/sweep.rs`.
**Today:** `load_model` called per sweep point; `find_coefficient_index`
parses YAML again per point. For 100 points, ~200 YAML reads.
**Fix:** compile once before the sweep loop; clone the cube per
iteration; rollback after metric eval. Pre-resolve coefficient index
once.
**Test:** `test_sweep_does_not_reload_yaml_per_point` (use a
`std::sync::atomic::AtomicUsize` counter wrapped around `load_model`
in a test build, or measure wall-clock with a 10-point sweep and
assert it doesn't scale linearly with point count).

### 2.3 — Sweep `--metric-where`

**File:** `crates/mc-cli/src/sweep.rs`.
**Today:** metric evaluates over ALL leaf coords.
**Fix:** add `--metric-where <expr>` that reuses `query.rs`'s filter
parser; restrict the leaf-coord enumeration in `eval_metric` to
matching coords only.
**Test:** `test_sweep_metric_where_restricts_to_subset`.

### 2.4 — Query `--group-by`

**File:** `crates/mc-cli/src/query.rs`.
**Today:** `--aggregate` returns one scalar; no per-dimension
grouping.
**Fix:** add `--group-by <DimName>` (repeatable). Partition matched
coords by the group-key dimensions before aggregating; emit one row
per group.
**Test:** `test_query_group_by_market_returns_per_market_aggregate`.

### 2.5 — Write response `write_id` / `revision_id`

**File:** `crates/mc-cli/src/write.rs`.
**Today:** JSON response has no durable handle; `writes.jsonl`
entries also have no sequence number.
**Fix:** add a monotonic `write_id` (next integer; computed by
counting lines in `writes.jsonl` before append). Include in both
the JSONL entry and the JSON response. Useful for diff and audit.
**Test:** `test_write_response_includes_monotonic_write_id`.

### 2.6 — `ureq` instead of curl subprocess

**Files:** `crates/mc-cli/Cargo.toml`, `crates/mc-cli/src/transform.rs`.
**Today:** `transform.rs:159-177` shells out to `curl`. Auth headers,
TLS client certs, proxy settings can't be configured.
**Fix:** add `ureq.workspace = true` to `mc-cli/Cargo.toml`
(workspace already pins it via mc-drivers — this is making the
existing transitive dep explicit). Replace the curl subprocess with
a `ureq::get(url).call()`.
**Test:** `test_transform_url_fetch_uses_ureq` (use a local HTTP
server fixture; same pattern as mc-drivers tests).

---

## Block 3 — Latent bug bundle (1-line fixes; bundle if cheap)

These are small and not worth their own block but should land
opportunistically as you touch nearby code:

- **E-F:** `whatif --dry-run` emits `would_affect: <--show list>`
  instead of computed dependents. Either rename the field to
  `requested_outputs` (5 minutes) or compute the actual dependent
  closure (medium). Pick the rename if you don't have time for the
  closure computation.
  **File:** `whatif.rs:339-342`.

---

## Email-matchback gaps NOT in 6A.2 (and where they go)

The post-6A audit examined 10 Python scripts in
`~/Projects/email-matchback/scripts/mosaic/` totaling ~1,260 lines.
Of those, ~350 lines are now eliminated by Phase 6A's verbs (the
"goldens-as-probes" pattern). Phase 6A.1 closed another small
chunk by wiring `time_format` for non-ISO dates. The remaining
~670 lines of "should-be-engine" Python span 4 categories. **None
of them belong in 6A.2** because they all require either new
schema design, kernel changes, or new dependencies — exactly the
things 6A.2 explicitly forbids.

If you find yourself thinking "while I'm here, I could just add
`is_element()` to the formula parser…" — stop. That's Phase 3I.
Same for every row in this table.

| Email-matchback Python | Lines | Target phase | Why NOT 6A.2 |
|---|---:|---|---|
| `flatten_ltd_comparison.py` — Excel year-blocked grid → long CSV | 220 | **Phase 5D** | Requires `calamine` xlsx dep + ADR for layout descriptor; touches `mc-recipe/src/schema.rs` (locked surface) |
| `build_ltv_cohort.py` — customer rows → cohort aggregation | 200 | **Phase 5D** (Tessera transformation phase) | Requires `group_by` + `derive_dim` recipe steps; ADR-0010 amendment; changes recipe streaming model |
| `prepare_mmm_inputs.py` — 464 hand-generated indicator rows | 80 | **Phase 3I** | Needs `is_element(Dim, "Element")` in formula parser OR new `Indicator` measure role; touches `mc-model/src/formula.rs` (locked surface) |
| `prepare_v2_inputs.py` — Plan→Actual mirror, Nov/Dec extension, Q1 anchor pre-compute | 170 | **Phase 3I/3J** | Needs `coalesce_scenario()` / `extrapolate_last_value()` formula functions + `parameters:` block; touches `mc-model/src/{schema,formula}.rs` (locked surfaces) |
| `tide-mmm.yaml` — 5 single-key seasonality lookup tables | (model-side) | **Phase 3I** | Needs multi-key `lookup_tables` schema; ADR amendment to ADR-0013 |
| MMM output bounds (Amarillo -$5,706 case) | (model-side) | **Phase 3H amendment** | `output_bound: {min: 0}` on fitted models; touches `mc-model/src/schema.rs` (locked surface) |
| MMM adstock + saturation transforms | (model-side) | **Phase 3H.2** | Native `adstock:` block on `fitted_models:`; touches `mc-core/src/cube.rs::resolve_cross_coord_read` (locked surface) |

**The 6A.2 directive on these:** if you encounter a workaround for
any of these in the email-matchback scripts and feel the urge to
"just add the missing function," resist. The 6A.2 hard rules forbid
touching mc-core, mc-model, mc-recipe, mc-drivers, or
mosaic-plugin/. Items 1.1–1.7 are scoped to mc-cli + one file in
mc-tessera precisely so this constraint is mechanical, not
judgment-based.

**Sequencing recommendation (PM perspective, for the implementer's
context):** after 6A.2 ships, the next phase is Phase 3I (formula
language completion + parser unification + indicators + math
primitives + multi-key lookup tables). 5D (Tessera driver expansion
including xlsx) and 3H amendments (output_bound + adstock) are
parallel candidates after 3I.

---

## Backward Compat Inventory

Phase 6A.2 makes precisely **two** breaking changes, both with
explicit `schema_version` bumps. Everything else is additive.

**Breaking changes (with version bumps):**

| What | Old shape | New shape | Version bump |
|---|---|---|---|
| Trace JSON envelope | `inputs: { "Spend": {...}, "Spend": {...} }` (object with duplicate keys) | `inputs: [{...}, {...}]` (array; with `child_count` field) | `schema_version: "1.0"` → `"1.1"` |
| MCP numeric param types | `value: "999"` (string) | `value: 999` (number; coerced from string for compat) | No version bump (existing string clients still work via coercion) |

**Additive changes (no version bump needed):**

| What | What changes | Why no bump |
|---|---|---|
| Query JSON envelope | New fields: `limit`, `offset`, `count`, `truncated`, `next_offset` | Existing parsers see additional fields, not modified ones |
| `mc model write` JSON response | New field: `write_id` (Block 2.5 if shipped) | Additive |
| `mc tessera transform` JSON envelope | `schema_version` field added (was missing entirely); rows wrapped in envelope | Was technically broken; fix is bringing it into compliance with the documented Phase 6A invariant |
| `mc model whatif` `--set` | Repeatable flag (Block 2.1 if shipped) | Old single-use still works |
| `mc model query` `--group-by` | New flag (Block 2.4 if shipped) | Optional, off by default |
| `mc model sweep` `--metric-where` | New flag (Block 2.3 if shipped) | Optional |

**Things you must NOT break (regression-test these explicitly):**

- `mc demo` output matches brief §4.6 byte-for-byte.
- `mc model test crates/mc-model/examples/acme.yaml` returns 9/9 goldens passing.
- `mc model validate` / `inspect` / `lint` JSON envelopes stay at `schema_version: "1.0"` with no shape changes.
- The 5 original MCP tools (`mosaic.demo`, `mosaic.model.{validate,inspect,lint,test}`) keep their existing schemas.
- `mc tessera apply` continues to work end-to-end on the Acme recipe.
- `cargo test --workspace` count goes UP, not down (no tests dropped).

**Things that WILL change for downstream consumers:**

Anything parsing `mc model trace --format json` output today is
parsing duplicate JSON object keys (which is well-defined as
"last-key-wins" in JSON spec but loses information). The new array
shape is the correct one. Update the Phase 4A/4B Python adapters
if they parse trace output (audit during implementation).

---

## How to think about likely walls (PM-side context)

These are observations from the PM's read of the codebase that
might not be obvious from the per-item Decision Matrices but apply
across multiple items.

**Observation 1: `LoadedModel` is the integration point.**
Several items (1.1 LoadPolicy, 1.2 trace formula HashMap) all
attach data to `LoadedModel`. After 6A.2, `LoadedModel` will
likely look like:

```rust
pub struct LoadedModel {
    pub cube: Cube,
    pub root_principal: Principal,
    pub refs: ModelRefs,
    pub policy: LoadPolicy,                          // NEW (item 1.1)
    pub formulas: HashMap<MeasureId, String>,        // NEW (item 1.2)
}
```

If this gets crowded, factor a `LoadedModelMetadata` struct.
Don't worry about it until both items 1.1 and 1.2 are landed.

**Observation 2: Many regression tests will share fixture setup.**
Items 1.1, 1.2, 1.3 all need a model with derived measures + at
least one consolidation level + a write log. Build a single
`tests/fixtures/write_log_fixture.rs` helper that creates a temp
dir + Acme YAML + Acme CSV + a writes.jsonl with 2-3 entries.
Reuse across all items 1.1–1.3 tests.

**Observation 3: `agent_cli_integration.rs` is the primary
integration test surface.** You'll likely add 15–25 new tests
there. Group them by item (e.g., `mod write_log_replay { ... }`,
`mod trace_formula { ... }`) for readability.

**Observation 4: The `mcp.rs` tool definitions are repetitive.**
Refactoring all 12 tools to use a coercing-accessor helper (item
1.4) is more code than fixing each by hand, but the helper is
worth it because Block 2.1 (multi-cell whatif) and Block 2.3/2.4
(metric-where, group-by) all add new MCP parameters that benefit
from the same coercion.

**Observation 5: Read process-notes Rule 9 BEFORE starting
item 1.1.** It's the binding spec for which verbs replay which
sources. Don't reinvent — implement.

---

## Out of Scope (explicitly deferred)

These are real gaps the audits surfaced but explicitly are NOT
6A.2 work. Do not patch any of these. Each requires an ADR before
phase scoping.

| Finding | Why deferred | Future phase |
|---|---|---|
| **M-22 / M-23**: XLSX driver + year-blocked layout (220 lines of `flatten_ltd_comparison.py`) | Needs `calamine` dep, MSRV check, sheet/range/merged-cell semantics | Phase 5D ADR |
| **M-11**: `is_element()` / indicator generation (464 hand-generated rows in MMM) | Codex prefers narrow `is_element(Dim, "Element")` over broader string-literal expansion; design choice | Phase 3I |
| **M-12**: `extrapolate_last_value()` / LOCF | Past-gap vs. future-gap have different semantics; needs `Scope` system extension (currently `AllLeaves` only — see audit S-1) | Phase 3I or later, ADR-gated |
| **M-13**: `actual_ref(measure, fallback)` or `scenario_ref()` | Three viable shapes; cross-coord dep-graph implications | Phase 3I ADR amendment |
| **M-14**: `parameters:` block | Namespace, type, override, lineage rules undecided | Phase 3I/3J ADR |
| **M-15**: math primitives (`pow`, `sqrt`, `ln`, `norm_inv`) | Already committed for Phase 3I per `formula-language-expansion.md` | Phase 3I |
| **M-16**: multi-key `lookup_tables` | Schema amendment + recipe + formula syntax all touch it | Phase 3I/3J |
| **M-17**: predict arity validation, norm_cdf sigma guard | Runtime already returns Null; only load-time validation missing; bundle with Phase 3I | Phase 3H amendment or 3I |
| **M-18**: `avg_over` / `min_over` / `max_over` / `wavg_over` | One parser case + one Expr variant each; defer to formula expansion | Phase 3I |
| **M-19**: aggregation methods beyond Sum/WeightedAvg/Min/Max | Requires mc-core consolidation change | New phase needed |
| **M-21**: `ifs()` / `switch()` | Ergonomic improvement; not a correctness blocker | Phase 3I |
| **M-24**: `mc tessera retry-quarantine` verb | Operational feature; needs idempotency design | Phase 5C amendment |
| **M-25**: multi-file ingest | Recipe chaining design first | Phase 5D ADR |
| **M-26**: aggregation transforms (`group_by` in recipes) | Incompatible with streaming row model | Phase 5D ADR |
| **M-10**: `mc model report` verb | Product-design gap | Phase 6B/6C ADR |
| **M-44**: multi-axis sweep / optimization | Cartesian explosion without constraint handling | ADR required |

---

## What NOT to fix (Codex corrected Sonnet on these)

Drop these from any "actionable" list. The audit had them but
Codex's verification proved them wrong, overstated, or already-fixed:

| Sonnet finding | Codex finding | Why |
|---|---|---|
| **M-5** "trace returns `source: input` at consolidated coords" | Wrong for the primary path. The fallback exists but isn't triggered for normal consolidated reads. | The real bug is the duplicate-key JSON shape (covered as item 1.3) AND the fallback labeling (bundled with 1.3) — but NOT a wholesale "trace is broken." |
| **M-17** "norm_cdf with sigma≤0 produces NaN, violating engine invariant" | False at runtime. `rule.rs:755-768` already returns Null for `sigma <= 0`. | Only load-time validation is missing; defer to Phase 3I. |
| **M-28** "`serde_json` is an implicit transitive dep that should be made explicit" | **False.** `crates/mc-cli/Cargo.toml:21` explicitly declares `serde_json = "1"`. | Sonnet didn't read Cargo.toml. Drop entirely. |
| **E-C** "`rolling_avg` partial-window behavior untested" | False. `test_rolling_avg_partial_window` exists. | Drop; just add doc clarification if you happen to be in that file. |
| **E-D** "negative-lag-as-lead behavior untested" | False. `eval_lag_with_negative_leads` and `test_lag_negative_is_lead` exist. | Drop. |
| **E-G** "`days_to_ymd` may mis-date near leap-year boundaries" | Codex couldn't reproduce. | Demote to P3 doc; not actionable. |

If you find yourself reaching for any of these — stop. They're not
in scope.

---

## Hard Rules (binding)

1. **`mc-fixtures` is locked.** `git diff bbe9a41 -- crates/mc-fixtures/`
   returns 0 lines at the end.
2. **`mc-core` is locked.** `git diff bbe9a41 -- crates/mc-core/`
   returns 0 lines at the end. (6A.1 already amended `FittedModelData`
   for CRIT-1; 6A.2 should not need any further mc-core change. If
   you find yourself wanting one — stop, file a SPEC QUESTION.)
3. **`mc-model`, `mc-recipe`, `mc-drivers`, `mosaic-plugin/` all
   locked.** Same rule.
4. **No new dependencies** except the explicit `ureq.workspace = true`
   declaration in `mc-cli/Cargo.toml` from Should-Fix #2.6 (and only
   if you ship that item).
5. **Toolchain stays at Rust 1.78.** No `rust-toolchain.toml` edit.
6. **No `Cargo.lock` pin churn.** If Block 2.6 (`ureq`) requires a
   pin change, surface as SPEC QUESTION first; do not silently bump.
7. **Backward compat:** every existing test that passes pre-6A.2
   must still pass after. Locked behavior includes the JSON envelope
   shape for verbs not modified (you change the shape for transform
   in 1.5 and trace in 1.3 — both are additive in the documented
   sense).
8. **Preserve `process-notes` Rule 9 verbatim.** Item 1.1's
   `LoadPolicy` enum implements that rule; do not invent new
   semantics.

---

## Acceptance Gates (lean — no determinism loop, no benchmarks)

The 6A.1 handoff included a 10× determinism check. **Skip that for
6A.2.** None of the items in this phase touch concurrency, ordering,
or anything that introduces nondeterminism — they're all bug fixes
against deterministic code paths. A single `cargo test --workspace`
run is sufficient. Skip `cargo bench` entirely (no perf change).

Required gates (all fast):
- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (count up from 731 / 0 / 5;
  expect ~+12-18 new tests for the regressions).

Per-fix gates (run after each must-fix):
- [ ] **1.1 write-log replay:** the bash repro from §1.1 returns 999
  on the final query.
- [ ] **1.2 trace formula:** `mc model trace ... | jq '.formula'`
  returns a readable string like `"Spend / CPC"` on a derived cell.
- [ ] **1.3 trace duplicate keys:** consolidated trace JSON has
  `inputs` as an array with `child_count` matching the array length.
- [ ] **1.4 MCP numeric:** the JSON-RPC repro from §1.4 succeeds.
- [ ] **1.5 transform recipe:** the bash repro from §1.5 emits the
  full mapped row, not just defaults.
- [ ] **1.6 incremental WHERE:** unit test
  `test_inject_watermark_with_existing_where_uses_and` passes.
- [ ] **1.7 pagination:** envelope has `truncated: true` when limit
  is reached.

Locked-surfaces verification:
- [ ] `git diff bbe9a41 -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/ crates/mc-recipe/ crates/mc-drivers/ mosaic-plugin/`
  returns 0 lines.
- [ ] All edits scoped to: `crates/mc-cli/` + `crates/mc-tessera/src/incremental.rs`.

Forbidden-pattern grep (CLAUDE.md §6.2):
- [ ] `grep -rn "\.unwrap()\|\.expect(\|panic!(" crates/mc-core/src/`
  returns zero (mc-core unchanged anyway, but verify).

---

## Order of Operations

1. Read this handoff in full.
2. Read [`docs/audits/codex-phase-6a-followup.md`](../audits/codex-phase-6a-followup.md)
   §3 for the full verification table — that's your source of truth
   for what's real vs. overstated.
3. Skim [`docs/process-notes.md`](../process-notes.md) Rule 9 — the
   four-source state model is the binding spec for item 1.1.
4. **Block 1 in numerical order** — each item has a repro; verify-
   then-fix-then-verify. 1.1 first (highest stakes); the others can
   be done in parallel mentally but commit linearly.
5. **Block 2 only if Block 1 + 1.7 finishes with capacity.** If any
   should-fix turns out to be larger than its description, drop it
   to a follow-up phase and note in the completion report.
6. Run all gates. Write the completion report at
   `docs/reports/phase-6a-2-completion-report.md`.
7. **Stop.** Do not commit, push, or tag — that happens after PM review.

---

## Completion Report Expectations

Per process-notes Rule 10. Report includes:

1. **Shipped** — what landed for each must-fix and should-fix item.
2. **Acceptance gates** — checkbox list mirroring §"Acceptance Gates" above.
3. **Per-fix repro outputs** — paste the actual command output
   showing each repro now returns the expected value.
4. **Should-fix scorecard** — for each Block 2 item, mark
   SHIPPED / DEFERRED with a one-line reason.
5. **Known debt** — anything you noticed but didn't fix (file
   follow-ups).
6. **Locked surfaces grep** — paste the output (should be empty).

### Headline Acceptance Gates

- [ ] Item 1.1 fix lands; write-then-read repro returns 999; 3 regression tests added.
- [ ] Item 1.2 fix lands; trace emits authored formula string; 1 regression test added.
- [ ] Item 1.3 fix lands; trace `inputs` is an array with `child_count` field; 2 regression tests added.
- [ ] Item 1.4 fix lands; MCP accepts JSON numbers + returns parsed `structured`; 3 regression tests added.
- [ ] Item 1.5 fix lands; `mc tessera transform` consumes real `mc-recipe` YAML correctly; 2 regression tests added.
- [ ] Item 1.6 fix lands; incremental SQL `WHERE` injection works with existing WHERE clauses; 3 regression tests added.
- [ ] Item 1.7 fix lands; query envelope has `truncated`/`count`/`limit`/`next_offset`; `--offset` flag works; 2 regression tests added.
- [ ] Each Block 2 item: shipped or explicitly deferred (5–6 items).
- [ ] All four full-workspace gates green.
- [ ] Locked surfaces grep clean.

---

## SPEC QUESTION Format

If anything in this handoff conflicts with `engine-semantics.md`,
`CLAUDE.md`, `process-notes.md`, or another binding doc, surface
as a SPEC QUESTION before guessing:

```
SPEC QUESTION: [one-line summary]

Context: [where in the handoff this came up]
Spec text: [literal quote]
The conflict / ambiguity: [what's unclear]
My proposed interpretation: [your best guess]
What I would do without confirmation: [the conservative path]
```

Two areas where SPEC QUESTIONs are likely:
- **Item 1.1 edge case:** what to do when a write-log entry's
  coordinate references an element no longer in the YAML.
- **Item 1.5 schema translation:** if the real `mc-recipe::Recipe`
  parser surfaces a recipe shape `transform.rs` doesn't know how to
  handle (e.g., a complex `json_path` with array slicing).

---

*End of handoff. The instance reading this should now have everything
needed to ship 6A.2 in a single focused session — same shape as 6A.1,
tighter scope, no determinism loop.*
