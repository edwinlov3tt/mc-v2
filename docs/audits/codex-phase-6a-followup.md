# Codex Follow-up Audit — Phase 6A.1 Gap Review

## 1. Executive summary

Sonnet's audit is broadly accurate on the main Phase 6A agent-usability gaps. The most important findings are repo-real: post-hoc writes are persisted but not replayed, trace output is not good enough for agent explanation, `mc tessera transform` does not consume the real `mc-recipe` schema, and MCP schemas reject natural JSON numbers for numeric fields.

Top 5 issues truly blocking agent usability:

1. **Write-log replay is the top P0.** `mc model write` appends `.tessera/writes.jsonl`, then `mc model query` returns the old canonical value. This breaks the basic write-then-read loop.
2. **Trace output is not agent-safe.** Formula strings are debug op labels, trace JSON can emit duplicate `inputs` object keys, and consolidated trace metadata should expose `source: "consolidation"` plus `child_count`.
3. **Transform is not compatible with `mc-recipe`.** A real recipe such as `crates/mc-recipe/examples/recipes/acme-csv-import.recipe.yaml` produces only defaults under `mc tessera transform`, not mapped rows.
4. **MCP schemas and coercion are wrong for numeric fields.** `whatif.value`, `write.value`, `limit`, `depth`, and `preview` are advertised/handled as strings; JSON numbers fail.
5. **Core analysis verbs still force Python wrappers for common tasks.** No multi-cell whatif, no sweep metric scoping, no query group-by, and no pagination/truncation warnings.

Top 5 real issues to defer:

1. XLSX driver and year-blocked layouts.
2. Formula language/modeling expansion: parameters, indicators, string literals, and broad dimension identity functions.
3. Scenario fallback semantics: `scenario_ref`, `actual_ref(..., fallback)`, Plan-to-Actual mirroring, and LOCF/carry-forward.
4. Recipe pipeline expansion: chaining, multi-file ingest, aggregation transforms, gzip, and retry quarantine.
5. Report/template verb and multi-axis optimization/sweep.

Findings overstated, false, or needing more evidence:

- **M-5 is overstated as written.** Current core/CLI trace does return `source: "consolidation(27 children)"` for an Acme consolidated coordinate. The real bug is the weak JSON shape and the dangerous fallback to `source: "input"` when no trace is present.
- **M-17 overstates `norm_cdf` risk.** Runtime eval already returns `Null` for `sigma <= 0`; predict arity mismatch also returns `Null`. Load-time validation is still missing.
- **M-28 is false in current HEAD.** `crates/mc-cli/Cargo.toml:21` explicitly declares `serde_json = "1"`.
- **E-C and E-D are overplayed.** Rolling-average partial-window and negative-lag-as-lead behavior have tests.
- **E-G needs evidence.** The timestamp helper is crude and dependency-avoiding, but I did not find a current behavior break.

## 2. Verification method

Current repo state:

- Branch: `main`
- HEAD: `bbe9a41f2786d11de933dde634a1f` (`docs: refresh README + HANDOFF + CURRENT_STATE + MASTER_PHASE_PLAN through Phase 6A.1`)
- Sonnet Phase 6A.1 completion point cited in docs: `44a7437`
- Code changes since `44a7437`: none found; `git diff --name-status 44a7437..HEAD` showed docs only.
- Rust toolchain: `rustc 1.78.0`, `cargo 1.78.0`

Commands run:

```text
git status --short --untracked-files=all
git branch --show-current
git rev-parse HEAD
git log --oneline -n 20
git diff --name-status 44a7437..HEAD
rustc --version
cargo --version
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run --quiet --bin mc -- model trace crates/mc-model/examples/acme.yaml --coord "Scenario=Baseline,Version=Working,Time=Q1_2026,Channel=Paid_Media,Market=Florida,Measure=Spend" --format json
cargo run --quiet --bin mc -- model query /tmp/mosaic-write-replay.Oy1nTW/acme.yaml --coord "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend" --format json
cargo run --quiet --bin mc -- model write /tmp/mosaic-write-replay.Oy1nTW/acme.yaml --coord "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend" --value 999 --format json
cargo run --quiet --bin mc -- tessera transform --source crates/mc-model/examples/acme.inputs.csv --recipe crates/mc-recipe/examples/recipes/acme-csv-import.recipe.yaml --preview 1 --format json
printf ... | cargo run --quiet --bin mc -- mcp
```

Command results:

- `cargo fmt --check --all`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: first sandboxed run failed because HTTP tests could not bind local ephemeral ports; escalated rerun passed **731 passed, 0 failed, 5 ignored**.
- Write replay repro: initial query returned `10500`; write returned `"after": 999`; subsequent query still returned `10500`; `.tessera/writes.jsonl` existed.
- Consolidated trace repro: returned `source: "consolidation(27 children)"`, so Sonnet's "consolidated trace is input" claim is not true for this path.
- MCP numeric repro: JSON number `value: 999` for `mosaic.model.whatif` returned `missing required argument: value`.
- Transform repro: a real `mc-recipe` Acme CSV recipe returned `[{"Scenario":"Baseline","Version":"Working"}]`, proving mappings were not consumed.

Files inspected:

- Operating docs: `CLAUDE.md`, `docs/CURRENT_STATE.md`, `docs/HANDOFF.md`, `docs/process-notes.md`, `docs/roadmap/MASTER_PHASE_PLAN.md`
- Phase docs/reviews/handoffs: `docs/reports/phase-6a-1-completion-report.md`, `docs/handoffs/phase-6a-agent-ready-cli-handoff.md`, `docs/handoffs/phase-3e-3f-3g-handoff.md`, `docs/reviews/phase-3-5-6-shipped-review.md`
- Research notes: `docs/research-notes/formula-language-expansion.md`, `docs/research-notes/model-as-judge-architecture.md`, `docs/research-notes/time-anchor-runtime-parameter.md`
- Input audits: `docs/audits/calculation-audit.md`, `docs/audits/data-in-audit.md`, `docs/audits/data-out-audit.md`, `docs/audits/master-gap-report.md`
- Code: `crates/mc-cli/src/{query,write,whatif,trace,sweep,diff,mcp,transform}.rs`, `crates/mc-core/src/{cube,rule,value}.rs`, `crates/mc-model/src/{schema,formula,compile,validate}.rs`, `crates/mc-recipe/src/schema.rs`, `crates/mc-drivers/src/csv_driver.rs`, `crates/mc-tessera/src/{incremental,transform,time_format}.rs`, root and crate `Cargo.toml` files.

Unverified or only partially verified:

- I did not inspect the external Python scripts Sonnet referenced outside this repo; I verified the Mosaic side of the claims.
- I did not run live network URL transform tests; the code path clearly shells out to `curl`.
- I did not run XLSX/gzip examples because no drivers exist in current schema/code.

## 3. Finding verification table

| Sonnet ID | Sonnet claim | Verification status | Current severity | Evidence from current code | Recommended fix approach | Recommended phase | Test coverage needed | Notes / risks |
|---|---|---:|---:|---|---|---|---|---|
| M-1 | Write-log replay missing in `load_model` | Confirmed | P0 | `query.rs:278-317` loads YAML and canonical inputs only. `write.rs:175-194` appends `writes.jsonl`. Repro confirmed write 999 then query 10500. | Add loader policy: current-reality loader replays Tessera audit and writes; reproducible loader omits post-hoc writes. | Phase 6A.2 | CLI integration: write then query/trace/whatif/diff current side; test/sweep remain reproducible unless explicitly changed. | Process notes say query/whatif/trace/query-output/diff replay all four; test and sweep do not. Do not blindly replay writes everywhere. |
| M-2 | Trace formula field shows debug AST/op, not readable formula | Confirmed | P1 | `trace.rs:192-204` uses `format!("{:?}", expr_summary.op)`. `cube.rs:2132-2142` maps Phase 3E+ ops to `Const`. | Carry authored/canonical formula strings to trace rendering, likely via `LoadedModel` metadata in mc-cli. | Phase 6A.2 | Trace JSON for Acme formulas and Phase 3H formulas should show actual infix formula. | Avoid kernel behavior change if CLI can map `rule_id` to formula string. |
| M-3 | `mc tessera transform` parser incompatible with `mc-recipe` schema | Confirmed, with nuance | P1 | `transform.rs:202-326` only scans `column_mappings`, `mappings`, `defaults`, `json_path`, `output_columns`, `scale`. `mc-recipe` schema uses `source` and `columns` at `schema.rs:35-97`, `217-258`. Repro with Acme recipe emitted only defaults. | Parse real `mc_recipe::Recipe` first and translate `columns/defaults/source.json_path`; optionally keep legacy mini-schema as deprecated fallback. | Phase 6A.2 | Transform Acme CSV recipe and HTTP JSON recipe through CLI/MCP. | The bespoke parser was a Phase 6A shortcut, not necessarily accidental, but it violates the current agent contract. |
| M-4 | `whatif` is single-cell only | Confirmed | P1 | `whatif.rs:13-20` has singular `set_coord` and `value`; parser has one `--set` at `whatif.rs:42-48`. | Add batch override syntax or JSON input, preferably reusing kernel batch write semantics. | Phase 6A.2 should-fix | Multi-cell whatif test where two inputs interact; atomic error test. | Needs careful CLI/MCP schema, but no ADR if scoped as repeatable overrides. |
| M-5 | Trace returns `source: input` at consolidated coordinates | Overstated | P2 | Fallback exists at `trace.rs:136-148`, but core consolidated trace is built at `cube.rs:667-688`. Repro returned `source: "consolidation(27 children)"`. | Fix fallback labeling and JSON shape; expose `source: "consolidation"` and `child_count` as separate fields. | Phase 6A.2 | Consolidated trace regression on Q1/Paid_Media/Florida with child count. | Sonnet's cited fallback is real, but not triggered for the tested consolidated Acme path. |
| M-6 | Sweep reloads YAML 2N+1 times | Confirmed | P1 | `sweep.rs:160-166` loads per point; baseline loads again at `254-280`; coefficient lookup reparses at `323-333`. | Compile/load once, clone/snapshot/rollback per point, pre-resolve coefficient index. | Phase 6A.2 should-fix | Sweep point count and same results before/after single-compile refactor. | Keep write replay policy in mind: process notes currently classify sweep as reproducible/pristine. |
| M-7 | Diff only compares `--left` and `--right`; no since/before/after | Confirmed | P2 | `diff.rs:15-22` has only left/right; parser requires both at `68-70`. | Add persisted-state diff only after write-log/audit replay semantics are stable. | Phase 6A.2 nice or later | Diff current vs previous write log entry. | Not as blocking as write replay. |
| M-8 | Sweep metric evaluates globally; no metric scoping | Confirmed | P1 | `sweep.rs:336-425` enumerates all leaf coords for every metric. | Add `--metric-where` using the same filter implementation as query. | Phase 6A.2 should-fix | Market-scoped sweep metric test. | Phase 3I parser unification should eventually remove duplicate filter parsing. |
| M-9 | Query aggregate has no group-by | Confirmed | P1 | Aggregate path bypasses row output at `query.rs:185-197`; `run_aggregate` at `971-1119` returns one aggregate object. | Add `--group-by` over dimension names and partition matching coords before aggregating. | Phase 6A.2 should-fix | `sum(Revenue)` grouped by Market/Time. | Needs clear JSON shape and pagination interaction. |
| M-10 | No report/template verb | Confirmed | Future | No `mc model report` verb; query text fixed widths at `query.rs:1188-1224`. | ADR for report spec before code. | Phase 6B/6C or later | Template fixture if ADR chooses templates. | Product/design gap, not a bug. |
| M-11 | Indicator generation needs dimension identity formulas | Partially confirmed | P2 | `ScalarValue::Str` exists but is transient only at `value.rs:12-20`. `DimElement` exists at `rule.rs:105-108` and compile maps dimension refs at `compile.rs:520-525`; formula parser has no `is_element` or general strings. | Prefer narrow `is_element(Dim, "Element")` first to avoid stored string type expansion. | Phase 3I | Formula parse/eval tests for `is_element`. | Current element strings exist internally, but not as general cell values. |
| M-12 | No `extrapolate_last_value` / carry-forward | Confirmed | Future | No such function in formula parser; time-series funcs present but do not perform LOCF. | ADR for anchor-aware LOCF semantics. | Phase 3I or later | Past-gap vs future-gap cases. | Do not patch as naive `if_null(prev(...))`; semantics are domain-sensitive. |
| M-13 | No `scenario_ref` or `actual_ref` fallback | Confirmed | Future | `ParsedActualRefBody` has only `measure` at `schema.rs:430-434`; writer emits one arg at `formula.rs:1273-1277`. | ADR for scenario fallback chain. | Phase 3I/3J | Plan future period fallback tests. | Formula-owned read semantics are cleaner than recipe mirroring, but need design. |
| M-14 | No `parameters:` block | Confirmed | Future | `ParsedModel` top-level fields at `schema.rs:25-61` include no parameters block. | ADR for parameter namespace/type/lifecycle. | Phase 3I/3J | Parser/validator/eval tests. | Lookup tables/benchmarks cover some constants today. |
| M-15 | Missing math/stat primitives | Confirmed | P2 | Parser has `exp`/`norm_cdf` but no `pow`, `sqrt`, `ln`, `norm_inv`; unknown funcs fail. | Implement as Phase 3I formula expansion. | Phase 3I | Parser/eval/round-trip tests per function. | Already planned in formula-language research note. |
| M-16 | Lookup tables single-key only | Confirmed | P2 | `ParsedLookupTable.key_dimension: String` at `schema.rs:595-604`. | Schema amendment for compound keys, not quick patch. | Phase 3I/3J | Multi-key lookup tests. | Impacts recipe and formula syntax. |
| M-17 | Fitted model arity and `norm_cdf` validation missing | Partially confirmed | P2 | Validator checks fitted block basics at `validate.rs:1706-1768` but not `predict()` arity. Runtime arity returns `Null` at `cube.rs:1108-1110`. `norm_cdf` runtime guards `sigma <= 0` at `rule.rs:755-768`. | Add load-time diagnostics for predict arity; optional literal sigma validation. | Phase 6A.2 nice or Phase 3I | Invalid predict arity should fail validation with code. | Sonnet's NaN claim is false for current runtime. |
| M-18 | Only `sum_over`, no avg/min/max/wavg over dimension | Confirmed | P2 | Parser only handles `sum_over` at `formula.rs:760-779`; core enum only `SumOver` at `rule.rs:101-105`. | Formula expansion after semantics for nulls/weights are defined. | Phase 3I or later | Eval and consolidation parity tests. | Not a Phase 6A.2 bug. |
| M-19 | Aggregation methods limited | Confirmed | Future | Validator accepts Sum/Min/Max/WeightedAverage and rejects others at `validate.rs:854-889`. | ADR if adding Median/Count/Distinct/custom rollups. | Future | Validator plus consolidation tests. | Current behavior matches shipped kernel scope. |
| M-20 | Missing `output_bound`; logical ops do not short-circuit | Partially confirmed | P2 | `ParsedFittedModel` has no bound at `schema.rs:630-645`. `And`/`Or` evaluate both sides at `rule.rs:546-560`. | Add output bounds only after fitted-model schema amendment; short-circuit can be low-risk perf/correctness polish. | Phase 3I/3J | Bound validation and null/zero short-circuit tests. | Short-circuit is not a top agent blocker. |
| M-21 | No `ifs` or `switch` | Confirmed | P3 | Formula parser only has 3-arg `if` at `formula.rs:482-497`. | Add only with formula grammar expansion. | Phase 3I or later | Parser/eval/round-trip tests. | Ergonomics, not blocker. |
| M-22 | No XLSX driver | Confirmed | Future | `DriverKind` has no Xlsx at `mc-recipe/schema.rs:184-210`. | Driver ADR including dependency/MSRV review. | Phase 5D or later | XLSX fixture ingest tests. | Requires dependency and layout design. |
| M-23 | No sheet/header/skip/year-blocked layout support | Confirmed | Future | `SourceConfig` has path/query/table/url/json_path/format only at `schema.rs:114-153`; CSV driver always `has_headers(true)` at `csv_driver.rs:147-149`. | Recipe layout ADR. | Phase 5D or later | Header rows, skipped rows, year-blocked fixture. | Do not bolt this onto current CSV reader ad hoc. |
| M-24 | Retry quarantine workflow missing | Confirmed | P2 | Quarantine sidecar exists; no retry command found. | Add retry command only after deciding idempotency and audit semantics. | Phase 5C amendment or 5D | Quarantine then retry regression. | Operational feature, not Phase 6A.2 core. |
| M-25 | Multi-file ingest missing | Confirmed | Future | `SourceConfig` has single `path/query/url` at `schema.rs:119-139`. | ADR for source arrays and file-derived dimensions. | Phase 5D | Multi-file recipe fixture. | Needs deterministic file order and provenance. |
| M-26 | Aggregation transforms impossible in recipes | Confirmed | Future | `Recipe` has `source`, `columns`, `defaults`, no aggregate/group_by at `schema.rs:35-97`. | ADR for pre-cube aggregation with streaming constraints. | Phase 5D | Group-by sum/avg fixture. | Changes row-to-cell cardinality. |
| M-27 | Query limit has no pagination/truncation warning | Confirmed | P1 | Default `limit` at `query.rs:180`; loop breaks at `203-206`; JSON count has no `truncated` field. | Add `offset`, `limit`, `truncated`, `next_offset`, warnings. | Phase 6A.2 | Limit 1 on multi-row query reports truncation. | Important for agents and UI. |
| M-28 | `serde_json` implicit dependency | False | P3 | `crates/mc-cli/Cargo.toml:21` explicitly declares `serde_json = "1"`. | No code fix needed. Maybe move to workspace for consistency if desired. | Cleanup only | None. | Sonnet did not verify Cargo.toml. |
| M-29 | Transform fetch shells out to curl | Confirmed | P2 | `transform.rs:159-177` uses `std::process::Command::new("curl")`. Workspace has `ureq` at root `Cargo.toml:45`; mc-cli does not depend on it. | Prefer `ureq.workspace = true` in mc-cli if Phase 6A.2 authorizes dependency declaration; otherwise keep local-file transform only. | Phase 6A.2 should-fix | URL transform test with local HTTP server. | Adding a direct dependency is not a Cargo update but still must be explicit. |
| M-30 | Filter parser/tokenizer rejects hyphens | Partially confirmed | P3 | Identifier tokenizer allows only alnum/underscore at `query.rs:486-490`; quoted string values work. | Defer to Phase 3I parser unification; document quoting workaround. | Phase 3I | Hyphenated element value filter. | Less severe than claimed for element values because string literals are available in filters. |
| M-31 | MCP value/limit/depth parameters typed as strings | Confirmed | P1 | Tool schemas at `mcp.rs:190-266`; `tool_whatif` uses `as_str_owned` at `638-650`; `tool_write` at `776-788`. Repro with JSON number failed. | Use numeric/integer JSON schema types and coercing accessors accepting both numbers and strings. | Phase 6A.2 | JSON-RPC tests with numeric `value`, integer `limit/depth/preview`. | Also consider `structured` object vs string. |
| M-32 | Write response lacks durable write id/revision | Confirmed | P2 | JSON output at `write.rs:197-207`; log entry at `187-190`; no revision/write_id. | Add `write_id` and maybe `revision` when replay semantics are added. | Phase 6A.2 should-fix | Write response/log schema regression. | Useful for diff and audit. |
| M-33 | ISO week needs `%V`; cannot compute from date | Confirmed | P2 | `time_format.rs:266-269` explicitly says week requires `%V`. | Add ISO week computation from Y-M-D or document hard requirement. | Phase 5C amendment | Date-to-week ingest tests. | Not Phase 6A.2 unless touching ingest bugs. |
| M-34 | CSV/parser strictness and schema inference limitations | Confirmed | P2 | Inference samples first 100 rows at `csv_driver.rs:30`, `171-214`; I64 parse later fails on f64 at `217-255`; no skip rows/header config. | Split into bug fixes and layout design. | Phase 5D mostly | Mixed int/f64 after row 100; quoted CSV/header tests. | Some strictness is intentional model parser policy, not necessarily driver policy. |
| E-A | Incremental SQL appends second `WHERE` | Confirmed | P1 | `incremental.rs:145-146` always appends `WHERE`; tests at `267-295` omit existing-WHERE case. | Use placeholder when possible; otherwise append `AND` if existing WHERE and insert before ORDER/LIMIT. | Phase 6A.2 or Phase 5C patch | Existing WHERE unit test. | Small code bug with data correctness impact. |
| E-B | `on_missing_element: create` ID stability risk | Confirmed | P2 | Dynamic id uses `DYNAMIC_ELEMENT_BASE + refs.elements.len()` at `transform.rs:238-245`. | Stable sidecar/hashing scheme before broad create use. | Phase 5D | Reapply same rows in different order. | Existing behavior can be stable in a single deterministic run, but not robust enough for replay. |
| E-C | Rolling average partial-window policy unclear | Overstated | P3 | Formula integration has `test_rolling_avg_partial_window`; core has rolling avg tests. | Document policy; no urgent code fix. | Docs/Phase 3I cleanup | Existing tests plus doc assertion. | Sonnet missed current coverage. |
| E-D | Negative lag behavior untested/ambiguous | Overstated | P3 | Core test `eval_lag_with_negative_leads`; formula integration `test_lag_negative_is_lead`. | Document negative lag as lead or reject by ADR if undesired. | Docs/Phase 3I cleanup | Keep existing tests. | Current implementation intentionally supports it. |
| E-E | Calibration out-of-range behavior unsafe | Partially confirmed | P3 | PAVA clamps to first/last points at `cube.rs:1173-1196`; schema does not clearly bound outputs. | Add validation/docs for calibration point output range if probabilities are required. | Phase 3H amendment | Calibration point validation tests. | Not currently the NaN-style risk Sonnet implies. |
| E-F | `whatif --dry-run would_affect` is just `--show` | Confirmed | P2 | `whatif.rs:338-344` serializes `cmd.show`. | Either rename to `requested_outputs` or compute dependency closure. | Phase 6A.2 nice | Dry-run dependency list test. | Current output is misleading. |
| E-G | Manual timestamp helper fragile | Overstated | P3 | `write.rs:242-286` does dependency-free UTC date conversion; no failure found. | Add unit tests or use approved time dep later. | Cleanup | Leap day/month boundary tests. | Low priority. |
| COD-1 | Trace JSON uses duplicate input keys | Confirmed | P1 | `trace.rs:216-225` keys each child by measure name; `write_trace_json_node` emits an object at `294-306`. Consolidated Spend emitted many duplicate `"Spend"` keys. | Change `inputs` to an array of child nodes with coordinate/edge metadata. | Phase 6A.2 | JSON parse preserves all 27 consolidated children. | This is worse for agents than Sonnet stated: JSON parsers may keep only the last duplicate key. |
| COD-2 | Transform JSON/MCP lacks envelope/structured output | Confirmed | P1 | `format_json_output` returns raw array at `transform.rs:517-543`; MCP `tool_transform` uses `run_cli_verb` with `structured: None` at `mcp.rs:811-856`. | Wrap JSON transform output or at least MCP structured output in schema-versioned envelope. | Phase 6A.2 | CLI/MCP transform JSON envelope regression. | Data-out audit said all Phase 6A verbs emit envelopes; transform contradicts that. |
| S-1 | Rule scope remains `AllLeaves` only | Confirmed | Future | `compile.rs:252-258` maps only `AllLeaves`; rule comments say only scope is AllLeaves. | ADR for scoped rules before implementation. | Future Phase 3/4 | Scoped rule overlap tests. | Product gap, not Phase 6A.2. |

## 4. Top-priority implementation plan

Recommended next implementation phase: **Phase 6A.2 - Agent Surface Correctness Patch**.

### Must fix

| Item | Files likely touched | Expected behavior after fix | Regression tests to add | Risks / compatibility concerns |
|---|---|---|---|---|
| Loader policy and write-log replay | `crates/mc-cli/src/query.rs`, `write.rs`, `trace.rs`, `whatif.rs`, `diff.rs`; maybe a small loader helper module | `query`, `query --output`, `whatif`, `trace`, diff current side, and write pre-read show current operational reality including `.tessera/writes.jsonl`; `test` remains reproducible; `sweep` remains pristine unless explicitly changed. | Temp model: write 999 then query returns 999; trace/whatif see 999; `mc model test` still ignores writes; malformed log diagnostic. | Process notes and completion report drift on sweep. Implement a policy enum, not a single hidden behavior change. |
| Trace agent shape | `crates/mc-cli/src/trace.rs`, possibly `query.rs` `LoadedModel` metadata | Trace returns readable formula strings, unique child arrays, separate `source`, `child_count`, and coordinate fields. | Derived cell formula string test; consolidated trace has 27 child array entries with no duplicate JSON keys. | Avoid touching kernel unless needed. A CLI-level map from rule name/id to authored formula should be enough. |
| MCP schema/coercion and structured output | `crates/mc-cli/src/mcp.rs` | JSON numbers accepted for numeric params; integers accepted for limits/depth/preview; descriptors advertise number/integer; successful JSON CLI verbs expose structured data as JSON object or a clearly documented stable envelope. | JSON-RPC tests for `whatif.value: 999`, `write.value: 999`, `query.limit: 1`, and query `structured` type. | Existing clients may send strings; accept both during transition. |
| Transform recipe compatibility | `crates/mc-cli/src/transform.rs`, `mcp.rs`; no new dependency required for parsing because mc-cli already depends on `mc-recipe` | `mc tessera transform --recipe` accepts real `mc-recipe` YAML for CSV/HTTP JSON recipes and emits mapped rows, not only defaults. | Acme CSV recipe transform should emit Scenario/Version/Time/Channel/Market/Measure/value; HTTP JSON fixture if local server available. | Decide whether legacy mini-schema is fallback or removed. Do not invent a second recipe language. |
| Pagination/truncation warnings | `crates/mc-cli/src/query.rs`, `mcp.rs` descriptor | Query envelopes include `limit`, `offset`, `count`, `truncated`, and `next_offset` or equivalent. | Query with `--limit 1` over many rows returns one row and `truncated: true`. | JSON shape change should remain additive. |
| Incremental SQL WHERE injection | `crates/mc-tessera/src/incremental.rs` | Existing `WHERE` gets `AND`, placeholder still works, no duplicate `WHERE`. | Unit tests for no WHERE, existing WHERE, placeholder, ORDER BY/LIMIT if supported. | SQL rewriting can get complex. Keep minimal and recommend placeholder for complex SQL. |

### Should fix

- Multi-cell whatif/batch overrides using repeatable flags or a JSON payload.
- Single-compile sweep and pre-resolved coefficient index.
- `--metric-where` for sweep.
- `--group-by` for query aggregates.
- Write response `write_id` and revision fields.
- Transform URL fetch without `curl`, if a direct `ureq.workspace = true` dependency is approved.

### Nice to have

- Rename or compute `whatif --dry-run would_affect`.
- Better text formatting for query/diff.
- Diff modes beyond left/right.
- Output `warnings: []` envelope field.
- MCP output schemas, not just input schemas.

### Defer

- XLSX/layout drivers.
- Formula string-literal/general-parameter expansion.
- Scenario fallback/LOCF.
- Recipe chaining, multi-file ingest, aggregation transforms.
- Report/template verb and multi-axis optimization.

## 5. Design items that should NOT be patched blindly

| Design item | Why it is real | Why not patched quickly | Owner | Smallest useful version |
|---|---|---|---|---|
| XLSX driver and year-blocked layouts | Current drivers omit XLSX and layout controls; real business data arrives this way. | Requires dependency/MSRV review, sheet/range semantics, merged-cell behavior, and calendar mapping. | Phase 5D ADR | One XLSX sheet, explicit header row, rectangular range, no merged-cell semantics. |
| `parameters:` block | Current models lack a top-level parameter namespace. | Type system, override rules, lineage, and CLI write semantics need design. | Phase 3I/3J ADR | Numeric constants only, read-only in formulas, explicit type. |
| `scenario_ref` / `actual_ref` fallback | Future Plan formulas often need Actual with fallback. | Cross-scenario reads affect dependency graph and null/fallback semantics. | Phase 3I ADR amendment | `actual_ref(Measure, fallback_expr)` or a narrow `scenario_ref(Measure, "Scenario")`, not both at once. |
| Carry-forward / LOCF | Source data often stops before forecast horizon. | Past missing actuals and future extrapolation are different semantics; needs anchor awareness. | Phase 3I/5D ADR | `carry_forward(measure, max_periods)` gated by `is_future()` examples. |
| Recipe chaining | Agents need raw-to-normalized-to-cube pipelines. | Intermediate artifact contracts, caching, failure recovery, and provenance are not defined. | Phase 5D ADR | Two-step local chain with explicit output file between steps. |
| Aggregation transforms | Cohort/customer rows need pre-cube grouping. | Streaming row model currently maps rows to cells; aggregation requires buffering and group keys. | Phase 5D ADR | `group_by` plus `sum` only for one source. |
| Multi-file ingest | Cohort workflows often use one file per market/entity. | File ordering, file-derived dimensions, sidecar provenance, and partial failure behavior matter. | Phase 5D ADR | Static `sources: []` with deterministic order and optional file-derived default. |
| Multi-frequency Time dimensions | Weekly, monthly, quarterly, and fiscal calendars collide. | Time hierarchy, anchors, ISO weeks, fiscal periods, and scenario reads need common semantics. | Phase 3/5 ADR | One frequency per model plus explicit converter/import mapping. |
| Report/template verb | Python scripts exist mostly to format multi-section reports. | Template language, output formats, chart hints, and stable schema need product design. | Phase 6B/6C ADR | Saved query bundle that emits Markdown sections. |
| Multi-axis sweep / optimization | Budget allocation needs constrained multi-variable search. | Cartesian explosion and constraint handling need planner input. | Phase 6C or optimization ADR | Repeatable `--set/--range` for small grids with explicit max-points guard. |

## 6. Formula/modeling follow-up

| Candidate | Recommendation | Why |
|---|---|---|
| `is_element(Dim, "Element")` | Prefer as the first narrow fix. | It returns numeric/bool and avoids expanding stored kernel cell value types. It uses current coordinate identity without needing general string values. |
| `current_element(Dim)` plus string literals | Defer to Phase 3I. | `ScalarValue::Str` exists but is documented as transient lookup-key data, not stored cell data (`value.rs:18-20`). General string literals require parser, schema, and comparison semantics. |
| `indicators:` YAML block | Defer. | This is a modeling convenience, but `is_element` covers the smallest useful formula case without adding generated-measure machinery. |
| `parameters:` YAML block | Defer with ADR. | Lookup tables and benchmarks cover many constants today. A real parameter block needs namespace, type, override, and lineage rules. |
| `scenario_ref()` | Defer with ADR. | More general than `actual_ref`, but it opens broad cross-scenario dependency semantics. |
| `actual_ref(measure, fallback)` | Good candidate after ADR. | Narrower than `scenario_ref` and aligned with existing Scenario `actuals_element`, but fallback must distinguish Null, missing, and zero. |
| `extrapolate_last_value()` | Defer. | LOCF semantics are anchor-dependent and can hide genuinely missing past actuals. |
| `output_bound` on fitted models | Defer to fitted-model schema amendment. | Useful for probability-like outputs, but not agent-surface critical. Logistic method already bounds by sigmoid; linear needs explicit design. |
| `predict` arity validation | Add as small validator fix. | Runtime returns Null on mismatch; load-time diagnostic would make modeling errors visible earlier. |
| `norm_cdf` sigma guard | No urgent runtime fix. | Current eval returns Null for `sigma <= 0` at `rule.rs:755-768`. Optional load-time literal validation can be Phase 3I polish. |
| `avg_over/min_over/max_over/wavg_over` | Defer to Phase 3I or later. | `sum_over` is the shipped Phase 3G scope. More reducers need null and weighting semantics. |

Plan-to-Actual mirroring should not be solved only in recipes. Recipes can prepopulate convenience data, but the model formula is the right place for "for past/current periods read Actual, otherwise read Plan/forecast" because that is business logic. The smallest defensible formula path is an ADR for `actual_ref(..., fallback)` or `scenario_ref`, not a broad recipe-side mirror.

## 7. Data ingestion follow-up

| Finding | Classification | Recommendation |
|---|---|---|
| XLSX driver | Driver expansion | Phase 5D ADR with dependency/MSRV gate. |
| Layout/header rows/year-blocked data | Recipe schema design | Do not patch CSV/XLSX readers directly; add schema-level layout concepts. |
| Strict CSV parser limitations | Driver expansion plus recipe schema design | Quoting is a driver bug/feature; header/skip rows are schema. |
| `time_format` parser tokens `%e`, `%j` | Bug fix | Small Phase 5C amendment. |
| ISO week computation from date | Bug fix/design | Add date-to-ISO-week computation or keep explicit `%V` documented. |
| Schema inference widening to f64 | Bug fix | If first 100 rows are int and later rows are float, widen instead of failing. |
| `on_missing_element: create` ID stability | Bug fix/design | Need deterministic sidecar or hash scheme before broad use. |
| Incremental WHERE injection | Bug fix | Phase 6A.2 candidate because it is clear and tested locally. |
| Gzip CSV | Driver expansion | Phase 5D, likely dependency review. |
| Retry quarantine | Enterprise/deferred | Needs operational semantics, not a quick patch. |

## 8. Agent/data-out follow-up

Smallest changes that make Mosaic genuinely agent-usable:

1. Fix current-state loading so writes are visible to read/explain verbs.
2. Make trace JSON parse-safe and explanatory: formula strings, input arrays, child coordinates, and explicit consolidation metadata.
3. Fix MCP schemas/coercion so agents can send natural JSON values.
4. Align transform with `mc-recipe` and return an envelope/structured result under MCP.
5. Add truncation/pagination metadata to query.
6. Add at least one batch path: multi-cell whatif or repeatable `--set`.
7. Add scoped sweep metrics and group-by aggregates if Phase 6A.2 has room.

JSON envelopes are mostly fixed for model verbs, but not fully for transform. MCP `structured` is currently a JSON string for query-like tools, not a parsed JSON object. That is acceptable only as a temporary bridge; agents still have to parse twice.

## 9. Reproduction commands

Write replay P0:

```bash
tmp=$(mktemp -d /tmp/mosaic-write-replay.XXXXXX)
cp crates/mc-model/examples/acme.yaml crates/mc-model/examples/acme.inputs.csv "$tmp"/
cargo run --quiet --bin mc -- model query "$tmp/acme.yaml" --coord "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend" --format json
cargo run --quiet --bin mc -- model write "$tmp/acme.yaml" --coord "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend" --value 999 --format json
cargo run --quiet --bin mc -- model query "$tmp/acme.yaml" --coord "Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend" --format json
```

Expected after fix: final query returns `999`. Current behavior: final query returns `10500`.

Trace consolidated shape:

```bash
cargo run --quiet --bin mc -- model trace crates/mc-model/examples/acme.yaml --coord "Scenario=Baseline,Version=Working,Time=Q1_2026,Channel=Paid_Media,Market=Florida,Measure=Spend" --format json
```

Current behavior: `source` is consolidation, but `inputs` is an object with repeated `"Spend"` keys. Expected after fix: `inputs` is an array and child count is explicit.

MCP numeric value mismatch:

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"mosaic.model.whatif","arguments":{"path":"crates/mc-model/examples/acme.yaml","set_coord":"Scenario=Baseline,Version=Working,Time=Jan_2026,Channel=Paid_Search,Market=Tampa,Measure=Spend","value":999,"show":"Revenue"}}}' | cargo run --quiet --bin mc -- mcp
```

Current behavior: `missing required argument: value`. Expected after fix: accepted as numeric value.

Transform recipe compatibility:

```bash
cargo run --quiet --bin mc -- tessera transform --source crates/mc-model/examples/acme.inputs.csv --recipe crates/mc-recipe/examples/recipes/acme-csv-import.recipe.yaml --preview 1 --format json
```

Current behavior: only `Scenario` and `Version` defaults appear. Expected after fix: mapped Time, Channel, Market, Measure, and value fields appear.

Incremental SQL WHERE injection unit test idea:

```rust
inject_watermark(
    "SELECT * FROM events WHERE tenant_id = 7",
    &config_for_updated_at,
    &state_with_last_value("2026-05-01"),
)
```

Expected after fix: `SELECT * FROM events WHERE tenant_id = 7 AND updated_at > '2026-05-01'`.

Query group-by test idea:

```bash
cargo run --quiet --bin mc -- model query crates/mc-model/examples/acme.yaml --where "Time == 'Jan_2026'" --aggregate "sum(Revenue)" --group-by Market --format json
```

This command is proposed syntax, not current syntax. It should not be documented as available until implemented.

## 10. Proposed acceptance gates for Phase 6A.2

- `cargo fmt --check --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --release --workspace`
- `cargo test --workspace`
- Existing demo still passes.
- Representative model query over Acme returns expected value.
- Write replay regression: write then query/trace/whatif see the new value.
- `mc model test` regression: does not silently include post-hoc writes unless explicitly requested.
- Consolidated trace regression: source is consolidation, child count is explicit, child inputs are an array.
- Trace formula regression: readable formula string for derived cell.
- MCP structured response regression: query/whatif/trace/write return stable structured JSON, not only text.
- MCP numeric input regression: JSON numbers accepted for `value`.
- Transform recipe compatibility regression: real `mc-recipe` Acme CSV recipe maps rows correctly.
- Transform MCP regression: structured output present for JSON transform responses.
- Incremental SQL regression: existing WHERE gets AND, not second WHERE.
- No Rust toolchain bump.
- No `cargo update`.
- No unauthorized dependencies.

## 11. Final recommendation

Recommended next phase name: **Phase 6A.2 - Agent Surface Correctness Patch**.

Exact scope:

- Loader policy and write-log replay.
- Trace output correctness and agent-safe JSON.
- MCP schema/coercion/structured-output cleanup.
- `mc tessera transform` compatibility with `mc-recipe`.
- Query truncation metadata.
- Incremental SQL WHERE bug.
- Optional if time: multi-cell whatif, sweep single-compile, `--metric-where`, and query `--group-by`.

Explicit out-of-scope:

- Phase 3I formula language expansion except small validation diagnostics.
- XLSX/layout/multi-file/aggregation recipe design.
- Parameters, indicators block, scenario fallback chain, LOCF.
- Report/template verb.
- Multi-axis optimization.
- Toolchain changes, `cargo update`, and new dependencies unless explicitly approved.

ADR required before implementation:

- **No ADR required** for the core Phase 6A.2 bugfix scope above; the four-source state model already exists in `docs/process-notes.md:139-164`.
- **ADR required** before xlsx/layouts, parameters, scenario fallback, LOCF, recipe chaining, aggregation transforms, multi-file ingest, report/template, and multi-axis optimization.

Verdict: proceed with Phase 6A.2 before Phase 6B or Phase 3I. The repo is test-clean at current HEAD, but the agent surface is not yet reliable enough to build UI/planning workflows on top of it.
