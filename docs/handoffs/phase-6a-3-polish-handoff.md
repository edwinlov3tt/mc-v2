# Phase 6A.3 Handoff — Agent Surface Polish

> **Audience:** the Claude Code instance that implements Phase 6A.3.
> **You inherit `main` at `ef99952` (763 / 0 / 5 tests). You'll work on
> the branch `phase-6a-3/agent-surface-polish` — see process-notes §11
> for the git workflow rule (single instance, sequential = branch but
> no worktree).**
>
> **This is the cleanup phase for the 7 items deferred from Phase 6A.2.**
> Each was scoped as "should-fix, drop if scope creep" in the 6A.2
> handoff and dropped because Block 1 + 32 regression tests consumed
> the session. They're now standalone scope. None are blocking; all
> are well-bounded and have implementation sketches in the 6A.2 handoff
> §"Block 2".
>
> **Hard rule:** Phase 6A.3 modifies only `crates/mc-cli/`. It does NOT
> touch `mc-core`, `mc-fixtures`, `mc-model`, `mc-recipe`, `mc-drivers`,
> `mc-tessera`, or `mosaic-plugin/`. One exception: item 6 declares
> `ureq.workspace = true` in `crates/mc-cli/Cargo.toml` (already in
> the workspace; this is making the existing transitive dep explicit).
>
> **Scope discipline:** if you encounter a wall not covered by the
> per-item Decision Matrix, file a SPEC QUESTION. Don't expand scope.
> The 6A.2 handoff established that pre-empted decisions + SPEC QUESTION
> escape valve = no second pass. Same pattern here.

---

## The one paragraph you must internalize

7 items. ~6 of them are bounded enough to ship in a single session.
The biggest two by agent value are item 1 (multi-cell whatif) and
item 4 (query group-by) — those eliminate Python workarounds in
budget_reallocator.py and ltv_report.py respectively. Items 2, 3, 5,
6 are smaller polish + perf fixes. Item 7 is a 5-minute rename. The
6A.2 handoff already has the implementation sketches; this handoff
binds the decisions for the most likely walls and adds Phase 6A.3-
specific regression tests.

---

## Production-quality framing

Same as Phase 6A.2: this is a no-second-pass phase. The 6A.2 audit
pattern (Decision Matrix per item + SPEC QUESTION escape valve) caught
real bugs that would have required a second-pass fix. Apply the same
discipline here — follow the matrices, file SPEC QUESTIONs for
uncovered walls.

The audits at `docs/audits/master-gap-report.md` and
`docs/audits/codex-phase-6a-followup.md` already verified each of
these items as actionable bug-fixes-without-design. No new audit
needed.

---

## Items (7 total)

### Item 1 — Multi-cell whatif (`--set` repeatable)

**File:** `crates/mc-cli/src/whatif.rs`.

**The use case.** `budget_reallocator.py` (email-matchback) sweeps
budgets across 4 markets simultaneously. With single-cell override,
each market call reloads the model and loses cross-market interaction
effects. Multi-cell whatif lets one invocation override 4 cells
atomically and read the resulting state.

**The fix.** Make `--set` repeatable. Each `--set "coord=value"`
becomes one entry in a `Vec<(CellCoordinate, ScalarValue)>`. Apply
all overrides BEFORE running the compute pass, then read the
configured `--show` outputs.

**Verified API.** `mc-core` exposes `Cube::write` (used by Phase 6A
write verb). For multi-cell, you can either call `write` in a loop
or use `Cube::write_batch` if it exists. **Verify which is public**
before implementing. Either works for correctness; loop-of-write is
simpler if write_batch isn't public.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Same coord set twice (`--set "X=1" --set "X=2"`) | **Last write wins.** Document in `--help`. | Matches whatif semantics; no error. |
| W2: Apply order — sequential or atomic? | **All overrides applied first, THEN compute.** Don't interleave writes with reads. | This is what the user expects; matches mc-core's WriteBatch semantics. |
| W3: One override fails (e.g., coord refers to derived measure) | **Snapshot before apply; rollback on any failure; return all errors with line context.** Exit code 1. | Atomic semantics. The user gets ALL the errors at once, not just the first. |
| W4: JSON output shape — single result or per-override? | **Single envelope** with `overrides: [{coord, value}, ...]` (echo of inputs) and `show: { coord: { before, after, delta } }` (one entry per `--show` target, NOT per `--set`). | The user asked for what-if scenarios, not what-if per individual cell. |
| W5: `--dry-run` interaction | **Apply all overrides, compute, report deltas, then snapshot-rollback before exit.** No persistent change. Same pattern as single-cell whatif. | Already-correct invariant. |
| W6: How many overrides max? | **No hard limit.** Document as "intended for ≤100 cells; larger sets should use `mc tessera apply`." | Don't engineer for edge cases that aren't in scope. |

**Regression tests (4 required):**
1. `test_whatif_multiple_set_flags_apply_atomically` — 2 overrides interact (e.g., Spend Q1 + AOV Q1 → Revenue Q1 reflects both).
2. `test_whatif_same_coord_set_twice_last_wins`.
3. `test_whatif_one_override_fails_rolls_back_all` — set one valid + one to a derived measure; expect exit 1 + state unchanged.
4. `test_whatif_dry_run_does_not_persist_overrides` — confirm via subsequent `query` returns unchanged values.

---

### Item 2 — Single-compile sweep

**File:** `crates/mc-cli/src/sweep.rs`.

**The bug (already cited in the 6A.2 handoff).** `sweep.rs:160-166`
calls `load_model` per sweep point. `find_coefficient_index` at
`sweep.rs:323-333` parses YAML again per point. For 100 points → ~200
YAML reads. Not a correctness issue; just slow.

**The fix.** Compile once before the sweep loop; clone the cube per
iteration; rollback after metric eval. Pre-resolve coefficient index
once before the loop.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Cube cloning cost — does Cube::clone exist? Is it cheap? | **Yes, `Cube::clone` is part of mc-core's public API since Phase 1A.** Snapshot+rollback is the cheaper path; use that instead of clone. | Use `cube.snapshot()` before each iteration's parameter mutation; `cube.rollback_to(snapshot)` after the metric eval. |
| W2: Should we drop `find_coefficient_index` redundancy entirely, or just lift it out of the loop? | **Lift it out and pre-resolve before the loop starts.** Compute the index once; pass to the loop body. | Standard refactor. |
| W3: What about `--coefficient` that doesn't exist in the model? | **Error before the loop starts.** Don't enter the loop and discover the error 50 iterations in. | Fail-fast. |
| W4: Backward compat — does the JSON output shape change? | **No.** This is a perf-only fix; the output is byte-identical. | No envelope change → no schema_version bump. |

**Regression tests (2 required):**
1. `test_sweep_does_not_reload_yaml_per_point` — count the number of
   filesystem reads (use a `std::sync::atomic::AtomicUsize` counter
   wrapped around `load_model` in a test build, OR measure wall-clock
   over a 10-point sweep and assert the per-point time is < 5ms).
2. `test_sweep_unknown_coefficient_fails_before_loop` — assert exit
   code 1 with helpful error message.

---

### Item 3 — Sweep `--metric-where`

**File:** `crates/mc-cli/src/sweep.rs`.

**The bug.** `sweep.rs::eval_metric` calls `enumerate_leaf_coords(cube,
refs)` returning ALL leaf coordinates. Sweep metric "sum(Revenue)"
sums across every market/time/channel combo, not the one the user
actually wants to optimize.

**The fix.** Add `--metric-where <expr>` that reuses
`query.rs::Filter` (already a public type). Restrict the leaf-coord
enumeration in `eval_metric` to coords matching the filter.

**Verified API.** `query.rs` exposes `Filter::parse(&str) -> Result<Filter, ...>`
and `Filter::matches(&CellCoordinate, &Refs) -> bool`. **Confirm**
they're `pub` (or `pub(crate)` and same crate) before importing in
`sweep.rs`. If they're private, lift them to `pub(crate)` (Phase
6A.3 is mc-cli only — same crate, fine).

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Empty filter / flag absent — what's the default behavior? | **Same as today: enumerate ALL leaf coords.** No flag = no scoping. | Backward compat. |
| W2: Filter syntax — same as `--where` on query? | **Yes, identical.** Reuse `Filter::parse`. | No two parsers; future Phase 3I will unify them entirely. |
| W3: Filter matches zero coords | **Return Null metric, not error.** Document as "no matching coords." | The user asked a valid question; the answer is "nothing matched." |
| W4: Combined with `--coord-where` (existing scope flag, if any) | **AND-combine.** A coord must match BOTH to be included. | Composable scoping. |

**Regression tests (3 required):**
1. `test_sweep_metric_where_restricts_to_subset` — 4 markets in cube;
   `--metric-where "Market=Houston"` returns only Houston metric.
2. `test_sweep_metric_where_zero_matches_returns_null`.
3. `test_sweep_metric_where_combines_with_existing_scope_flags`.

---

### Item 4 — Query `--group-by`

**File:** `crates/mc-cli/src/query.rs`.

**The use case.** `ltv_report.py` (email-matchback) constructs separate
probes per market because `mc model query --aggregate "sum(Revenue)"`
returns one global scalar. With `--group-by Market`, one invocation
returns per-market revenue.

**The fix.** Add `--group-by <DimName>` (repeatable). Partition matched
coords by the group-key dimensions before aggregating; emit one row
per group.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Multi-dim group-by — `--group-by Market --group-by Time` | **Cross-product.** One row per (Market, Time) tuple. | Standard SQL group-by semantics. |
| W2: Output JSON shape | **Array of objects:** `{ group: { Market: "Houston", Time: "Jan_2026" }, value: 100.0 }`. | Easy for agents to parse; preserves group-key identity. |
| W3: Aggregate functions supported | **Same as `--aggregate`: sum, avg, min, max, count.** No new functions. | No scope creep. |
| W4: Interaction with `--show` | **Mutually exclusive.** Either `--show` (raw cells) or `--aggregate --group-by` (aggregated rows). Error if both set. | Different output shapes; can't interleave. |
| W5: Ordering of output rows | **Sort by group keys lexicographically (Element-name order, not ElementId order).** Deterministic. | Tests need stable ordering; ElementId is non-deterministic across model reloads. |
| W6: Empty group (no coords match for a particular group key combo) | **Don't emit a row for empty groups.** | Sparse semantics; matches SQL group-by-having behavior. |
| W7: `--limit` / `--offset` interaction | **Apply AFTER grouping.** `--limit 10` means "first 10 group rows," not "first 10 coords." | The user is paginating groups, not cells. |
| W8: Envelope shape — same as ungrouped query? | **Same envelope** (`schema_version`, `limit`, `offset`, `count`, `truncated`, `next_offset`, `rows`). `rows` is the array of group objects. | Consistency with non-grouped query. No version bump. |

**Regression tests (5 required):**
1. `test_query_group_by_market_returns_per_market_aggregate`.
2. `test_query_group_by_two_dims_returns_cross_product`.
3. `test_query_group_by_with_show_errors` — confirm exit 2 (CLI usage error).
4. `test_query_group_by_empty_group_skipped`.
5. `test_query_group_by_with_limit_paginates_groups`.

---

### Item 5 — Write response `write_id` / `revision_id`

**File:** `crates/mc-cli/src/write.rs`.

**The use case.** Audit + diff. After `mc model write`, the agent wants
a durable handle so a subsequent `mc model diff --since <write_id>`
or `mc model trace` can reference the specific write.

**The fix.** Add a monotonic `write_id` (next integer). Computed by
counting lines in `writes.jsonl` BEFORE append. Include in both:
- The JSONL entry (so replay can preserve the IDs)
- The JSON response

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: First write to a new file — what's the ID? | **`1`.** (1-indexed, not 0-indexed.) | Matches human intuition; "the first write" is ID 1. |
| W2: How is `write_id` computed? | **Count `\n` characters in the existing file before append.** Atomic enough for single-threaded use. Don't use file size or filesystem timestamps. | Single source of truth = the file itself. |
| W3: Concurrent writes from multiple processes? | **Out of scope.** Document as "single-writer assumption; concurrent writes are not supported." | Mosaic is single-threaded today. Phase 7 worries about multi-tenancy. |
| W4: What about the existing `writes.jsonl` entries that don't have a write_id? | **Replay assigns IDs based on line position** (line 1 → ID 1, etc.). Don't try to read pre-existing IDs from old entries; they don't have them. | Backward compat without schema change. |
| W5: JSON response field name — `write_id` or `revision_id`? | **`write_id`.** "Revision" suggests a richer concept (branching, etc.); "write" matches the verb name. | Naming consistency. |
| W6: Should `mc model query` echo the latest `write_id` so agents can chain? | **Yes — query envelope adds `as_of_write_id` field** showing the highest write_id replayed. Null if no writes.jsonl exists. | Agents can pin queries to a specific revision. |
| W7: Schema version bump for adding `as_of_write_id` to query envelope? | **No.** Additive field; existing parsers see one extra field. | Same rule as 6A.2 pagination. |

**Regression tests (4 required):**
1. `test_write_response_includes_monotonic_write_id` — first write returns `write_id: 1`; second returns `write_id: 2`.
2. `test_writes_jsonl_entries_include_write_id`.
3. `test_query_envelope_includes_as_of_write_id`.
4. `test_query_envelope_as_of_write_id_null_when_no_writes`.

---

### Item 6 — `ureq` instead of curl subprocess

**Files:** `crates/mc-cli/Cargo.toml`, `crates/mc-cli/src/transform.rs`.

**The bug (already cited in the 6A.2 handoff).** `transform.rs:159-177`
shells out to `curl`. Auth headers, TLS client certs, proxy settings
can't be configured. `ureq` is already in the workspace via mc-drivers.

**The fix.**
1. Add `ureq.workspace = true` to `mc-cli/Cargo.toml`.
2. Replace the `std::process::Command::new("curl")` block with a
   `ureq::get(url).call()`.
3. Map `ureq::Error` to a typed `LoadModelError::HttpFetchFailed`
   (or extend `LoadModelError` with a new variant). Exit code 3
   (I/O error per Phase 6A invariants).

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Does adding `ureq.workspace = true` to mc-cli's Cargo.toml require a Cargo.lock change? | **No.** ureq is already a transitive dep via mc-drivers; the workspace dep declaration just makes it explicit for mc-cli. Run `cargo build --workspace` to verify. If Cargo.lock changes, surface as SPEC QUESTION. | Existing transitive dep. |
| W2: Auth headers / proxy support in scope? | **Out of scope for 6A.3.** Phase 6A.3 just unblocks the basic GET; auth + proxy are future Phase 6C distribution work or Phase 5D Tessera improvements. | Don't scope-creep. |
| W3: Timeout — how long should `ureq::get` wait? | **30 seconds.** Configurable via `--timeout-secs <N>` flag (default 30). | Sensible default; agents can override for slow APIs. |
| W4: Response size limit — what if the URL returns 1GB? | **Cap at 100 MB.** Return error if response exceeds the cap. | Agent-safe default; transform isn't built for streaming large responses. |
| W5: HTTPS cert validation | **Default ureq behavior** (validates against system root CAs). No `--insecure` flag in Phase 6A.3. | Don't ship a way to disable cert validation — that's a security footgun. |

**Regression tests (3 required):**
1. `test_transform_url_fetch_uses_ureq_not_curl` — confirm by stubbing
   the URL and asserting auth headers don't break (curl version
   silently drops them).
2. `test_transform_url_timeout_returns_exit_3`.
3. `test_transform_url_oversized_response_returns_error`.

---

### Item 7 — Rename `whatif --dry-run would_affect`

**File:** `crates/mc-cli/src/whatif.rs:339-342`.

**The bug.** `whatif --dry-run` emits `would_affect: <--show list>` which
is literally the input flag echoed back. Misleading — the user expects
"these are the cells that would change" not "these are the cells you
asked about."

**The fix.** Two paths (the 6A.2 handoff said "pick the rename if you
don't have time for the closure computation"). For 6A.3, since multi-
cell whatif (item 1) is in scope, the closure path is more relevant.

**Decision Matrix:**

| Wall you'll hit | Binding decision | Why |
|---|---|---|
| W1: Path A (rename) vs Path B (compute closure)? | **Path A (rename to `requested_outputs`).** Path B requires walking the dep graph from each override coord, which mc-core supports but is enough scope to defer to a follow-up. | 5-minute fix vs. 1-hour fix; this phase is polish. |
| W2: Should we keep `would_affect` as a deprecated alias? | **No.** No shipped agents depend on it. Clean rename. | No compat shim needed. |
| W3: Schema version bump? | **No.** Field rename in the dry-run envelope; the `--dry-run` verb output isn't covered by `schema_version` (it's an additive sub-mode). | Stable. |

**Regression tests (1 required):**
1. `test_whatif_dry_run_emits_requested_outputs_field` — and confirm `would_affect` is gone.

---

## Out of Scope

These were on the "should-fix" list at some point but explicitly are
NOT 6A.3 work. Each has a clearer home:

| Item | Why deferred | Future phase |
|---|---|---|
| Compute actual dependent closure for `whatif --dry-run` (Path B in item 7) | Requires dep-graph traversal; designed but not scoped | Phase 6A.4 if demanded |
| `mc model trace` on consolidated coords (P1 known debt) | Already deferred in 6A.1 / 6A.2 reports | Future phase |
| Streaming for very large query results | Out-of-scope for 6A.2 + 6A.3 | Phase 6B/UI or 6C |
| Multi-axis sweep / optimization | Cartesian explosion w/o constraint handling | ADR required |

---

## Hard Rules (binding)

1. **`mc-fixtures`, `mc-core`, `mc-model`, `mc-recipe`, `mc-drivers`,
   `mc-tessera`, `mosaic-plugin/` all locked.** `git diff ef99952 --
   <each-of-these>` must return 0 lines.
2. **Allowed touch:** `crates/mc-cli/` only.
3. **One Cargo.toml change allowed:** `ureq.workspace = true` in
   `crates/mc-cli/Cargo.toml` (item 6). No other dep changes.
4. **No `Cargo.lock` pin churn.** ureq is already pinned via
   mc-drivers; verify no lockfile changes after item 6.
5. **Toolchain stays Rust 1.78.**
6. **Backward compat:** every existing test passes. The Phase 6A
   envelope schemas (1.0 + trace 1.1) stay valid; this phase adds
   ONLY additive fields (`as_of_write_id`, group-by `rows` shape,
   `requested_outputs` rename in dry-run sub-envelope which isn't
   schema-versioned).

---

## Acceptance Gates (lean)

Same as 6A.2 — no 10× determinism loop, no `cargo bench`, single
test run is sufficient.

- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (763 → expect ~+22 = ~785).
- [ ] Locked-surfaces grep clean (`git diff ef99952 -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/ crates/mc-recipe/ crates/mc-drivers/ crates/mc-tessera/ mosaic-plugin/` returns 0).
- [ ] All 7 items shipped with their required regression tests.

Per-item smoke checks (paste each output in completion report):
- [ ] **Item 1:** `mc model whatif crates/mc-model/examples/acme.yaml --set "...A...=100" --set "...B...=200" --show "..." --format json` returns deltas reflecting both overrides.
- [ ] **Item 2:** Run a 10-point sweep; total wall-clock < 1s on Acme.
- [ ] **Item 3:** Sweep with `--metric-where "Market=..."` returns a different metric than without.
- [ ] **Item 4:** Query with `--group-by Market` returns one row per market.
- [ ] **Item 5:** Two consecutive writes return `write_id: 1` then `write_id: 2`.
- [ ] **Item 6:** Transform with `--source <https-url>` succeeds without curl on PATH.
- [ ] **Item 7:** `mc model whatif ... --dry-run --format json | jq '.requested_outputs'` returns the show list; `.would_affect` is null/absent.

---

## Order of Operations

1. Read this handoff in full.
2. Skim [`docs/process-notes.md`](../process-notes.md) §11 (git
   workflow — confirms branch shape) and §9 (four-source state
   model — relevant for items 1, 5).
3. Skim the 6A.2 handoff §"Block 2" — it has the original
   implementation sketches that this handoff binds.
4. **Order:** ship items in numeric order (1 → 7). Item 1 is the
   highest-value (closes budget_reallocator.py); item 7 is trivial.
5. Run gates after each item lands. Don't batch-test all 7 at the
   end — a regression in item 3 would be invisible until you finish
   item 7.
6. Write the completion report at `docs/reports/phase-6a-3-completion-report.md`.
7. **Stop.** Do not commit, push, or tag — that happens after PM
   review of the audit.

---

## Completion Report Expectations

Per process-notes Rule 10. Same shape as 6A.1 / 6A.2:
- **Shipped** — what landed for each of the 7 items.
- **Per-item smoke check outputs** — paste each from the
   "Acceptance Gates" section.
- **Acceptance gates checklist** — checkbox status.
- **Known debt** — anything noticed but not fixed.
- **Locked surfaces grep** — paste output.

---

## SPEC QUESTION Format

Same as before:

```
SPEC QUESTION: [one-line summary]

Context: [where in the handoff this came up]
Spec text: [literal quote]
The conflict / ambiguity: [what's unclear]
My proposed interpretation: [your best guess]
What I would do without confirmation: [the conservative path]
```

Most likely SPEC QUESTION candidates in 6A.3:
- Item 1: how mc-core's `WriteBatch` (if public) interacts with the
  whatif snapshot/rollback pattern.
- Item 4: whether `--group-by` with `--aggregate count` is meaningful
  (count of what — coords? distinct values?).
- Item 6: whether ureq is allowed to be added to mc-cli's Cargo.toml
  given the workspace dep is already pinned (it should be — verify).

---

*End of handoff. Phase 6A.3 is the last polish on the Phase 6A
agent surface. After this ships, the surface is solid and the next
natural phase is either 3I (formula language completion + indicators
+ math primitives) or 6B (web UI).*
