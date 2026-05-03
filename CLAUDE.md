# CLAUDE.md — Mosaic Rust Kernel & Model Layer

> **You are Claude Code, implementing the Rust kernel + model layer for Mosaic.**
>
> Two documents in `docs/` are the kernel contract:
> - `docs/specs/engine-semantics.md` — what the kernel *means* (invariants, semantics).
> - `docs/specs/phase-1-rust-kernel-build-brief.md` — what was built in Phase 1 (exact types, tests, fixtures).
>
> This file is your **operating manual**. It does not override the brief or
> the semantics spec — those win every conflict — but it tells you how to
> keep yourself honest while implementing them.
>
> **Read this entire file at the start of every session before touching code.**

---

## Project name + naming convention (rename note)

**The project was renamed from "MarketingCubes V2" → "Mosaic" on 2026-05-03.** Mosaic is positioned as an AI-powered Large Numbers Model (LNM) platform — see [`docs/strategy/POSITIONING.md`](docs/strategy/POSITIONING.md) for the strategic framing.

**Naming convention rule (binding):**

- **Product name in prose:** Mosaic (replaces "MarketingCubes" / "MarketingCubes V2").
- **Crate names + code identifiers:** **the `mc-` prefix STAYS UNCHANGED.** `mc-core`, `mc-model`, `mc-fixtures`, `mc-cli` keep their names. The `mc` prefix is now a backronym for "Mosaic Core" rather than "MarketingCubes," but the file paths, crate names, and module names do not change.
- **Diagnostic codes:** `MC1xxx` / `MC2xxx` / `MC3xxx` / `MC4xxx` namespace stays. The `MC` prefix is now "Mosaic Code." Codes are forever (CVE-style retirement per ADR-0005 amendment #11); renaming the prefix would break Phase 4 LLM consumers and Phase 6 UI consumers pinned to the existing codes.
- **Historical docs (ADRs, completion reports, past handoffs, specs, original PRD, research notes, archived material) keep their original "MarketingCubes" naming.** Those are snapshots of past states; rewriting them would corrupt the audit trail. Future readers of historical docs understand "this doc predates the 2026-05-03 rename."
- **Active / forward-looking docs use "Mosaic":** README, CLAUDE.md (this file), HANDOFF.md, CURRENT_STATE.md, MASTER_PHASE_PLAN.md, strategy docs, process-notes, and any new for-dummies / handoff / completion-report / ADR going forward.
- **Cargo.toml descriptions and lib.rs/main.rs lead doc-comments:** updated to use "Mosaic" so `cargo doc` and `cargo metadata` reflect the current name.

When reading any historical doc that mentions "MarketingCubes" or "MarketingCubes V2," mentally substitute "Mosaic." When writing any new doc, use "Mosaic." When writing code identifiers (`mc-foo`, `MC1234`), use the existing convention. **No file renames.**

---

## 0. Hierarchy of authority

The two `docs/` files have **different jobs**, not competing authority. Use the
right one for the question you're asking.

### When deciding WHAT TO IMPLEMENT in Phase 1 (scope, types, tests, signatures)

1. `docs/specs/phase-1-rust-kernel-build-brief.md` — the build contract
2. `docs/specs/engine-semantics.md` — fills gaps not covered by the brief
3. `CLAUDE.md` (this file) — process and self-checks
4. Any prior chat instructions
5. Your own intuition about what would be "nicer"

### When deciding WHAT A CONCEPT MEANS (semantics, invariants, vocabulary)

1. `docs/specs/engine-semantics.md` — the source of truth on meaning
2. `docs/specs/phase-1-rust-kernel-build-brief.md` — Phase-1-specific narrowings
3. `CLAUDE.md` (this file)

### How to handle apparent contradictions

The brief sometimes **intentionally narrows** the semantics doc for Phase 1
scope. These are not bugs — they are deliberate. Examples:

- Semantics doc shows `Box<dyn CellStore>` as the long-term interface; the
  brief mandates a concrete `HashMapStore` for Phase 1 (brief change-log #4).
- Semantics doc allows multiple hierarchies per dimension; brief restricts
  Phase 1 to one default hierarchy.
- Semantics doc defines `MeasureRole::Both`; brief excludes it from Phase 1.

**If the brief intentionally narrows the semantics doc, obey the brief.**
If they appear to truly contradict each other (not a narrowing — an actual
disagreement), **stop and ask**. Do not pick one and proceed silently.

If your intuition disagrees with anything above it, **your intuition is wrong**.

---

## 1. Project identity (the 60-second cheat sheet)

| Thing | Value |
|---|---|
| Workspace | Cargo workspace at repo root, three crates: `mc-core`, `mc-fixtures`, `mc-cli` |
| Toolchain | Rust 1.78, pinned in `rust-toolchain.toml` |
| Allowed runtime deps (mc-core) | `smallvec`, `ahash`, `thiserror`, `once_cell` |
| Phase 1 dev deps (mc-core) | `mc-fixtures`, `criterion` (Phase 1B). `proptest`/`insta` are still declared at workspace level only — not pulled into `mc-core` (see §1.1 below) |
| Banned deps | `serde`, `tokio`, `async-std`, `rayon`, `anyhow`, anything else not in §2.5 of the brief |
| Demo cube | "Acme_MarketingFinance" — 6 dimensions, 11 measures, 5 rules, 2,520 input cells |
| Dimension order | `[Scenario, Version, Time, Channel, Market, Measure]` — exactly, always |
| Storage | `HashMapStore` (concrete struct, NOT a trait object in Phase 1) |
| Concurrency | Single-threaded. No `unsafe`. No `async`. No threads. |

If you find yourself wanting to add a dependency, change a dimension order, or
introduce a trait abstraction not in the brief — **stop**.

### 1.1 The criterion/proptest/insta deviation (criterion CLOSED 2026-05-01; proptest/insta still active)

The brief §2.5 lists `criterion = "0.5"`, `proptest = "1"`, and `insta = "1"`
as workspace dev-dependencies, and §10/§11 reference proptest tests and
criterion benchmarks. Originally none of them were pulled into `mc-core`
because:

> On Rust 1.78 (pinned in `rust-toolchain.toml`), criterion's transitive
> dependency `clap_lex 1.1.0` requires `edition2024`, which was only
> stabilized in 1.85.

**Phase 1B (2026-05-01) closed the criterion side** by pinning three
transitive deps in `Cargo.lock` (`clap` → 4.4.18, `clap_lex` → 0.6.0,
`half` → 2.4.1). `criterion = "0.5"` (the brief's pin) is unchanged at
the workspace level; only `mc-core/Cargo.toml` gained
`criterion.workspace = true` plus five `[[bench]]` entries. `cargo bench
--workspace` now runs the brief §11 suite and is part of the gate. See
[`docs/PERF.md`](docs/PERF.md) for the full diagnosis and benchmark
table.

`proptest` and `insta` are **still not** in `mc-core` dev-deps. That
side of the deviation is no longer toolchain-blocked — Phase 1B
demonstrated that pre-edition2024 transitive pins make the toolchain
viable. They are deferred for a separate reason now: the §10.7
proptest doctrines and any insta-driven snapshot tests are themselves
Phase 2 work, and pulling the crates in without using them would only
slow `cargo build`. While that side of the deviation is active:

- Do **not** add `proptest` or `insta` to `mc-core` dev-deps unless
  you're also implementing the test that needs them.
- Do **not** implement the proptest tests in §10.7
  (`doctrine_atomicity_of_write`, `doctrine_causality`,
  `t_acme_trace_root_value_equals_read_value`'s proptest variant)
  without a Phase-2 prompt. Leave them as `// TODO(proptest):` stubs
  with a comment pointing at the brief section.
- The criterion-side rules above are **closed** — the `benches/`
  directory exists, `[[bench]]` entries are in
  `crates/mc-core/Cargo.toml`, and `cargo bench` is contractual via
  §6.1 step 4 + the new ceiling check in §6.4.

If you encounter the §10.7 proptest stubs and your instinct is "I'll
just add `proptest` real quick" — that work belongs to Phase 2. Surface
it in chat or open an ADR; don't quietly add it.

When pinning needs revisiting (e.g. on Rust 1.85+ bump), see
[`docs/PERF.md`](docs/PERF.md) §9.7 for the housekeeping checklist.

---

## 2. Known weaknesses (and how to counter them)

These are the failure modes most likely to bite you on *this specific project*.
Each one is paired with an explicit countermeasure.

### 2.1 Eager-when-the-spec-says-lazy

**The trap.** The brief says the dependency graph is built on demand (lazy).
There is a test (`t_dependency_graph_empty_after_build`) that asserts it's
empty right after `build_acme_cube()`. Your instinct will be to materialize
everything at construction "for safety." That silently fails the test.

**The countermeasure.** Before writing any "build" or "init" code, grep the
brief for the word "lazy" and re-read §3.12 and §10.5. If you're about to
populate a graph in a constructor, *don't*. The graph is populated by reads.

### 2.2 Renaming for "idiomatic Rust"

**The trap.** You will want to rename `WritebackResult` to `WriteResponse`,
`DirtyTracker` to `DirtySet`, `mark_closure` to `invalidate`, etc. Every
rename is a contract violation. Tests reference these names exactly.

**The countermeasure.** Before defining any public type, function, or test,
search the brief for the name you're about to use. If it appears, copy it
character-for-character. If it doesn't appear and you think you need it,
**stop and check** §3 again — it's almost certainly there under a different
section.

### 2.3 `unwrap()` and `expect()` creep

**The trap.** While iterating, you'll write `.unwrap()` to make the borrow
checker happy and intend to "clean it up later." You won't.

**The countermeasure.** Run this grep before any commit:
```bash
grep -rn "\.unwrap()\|\.expect(" crates/mc-core/src/
```
The only acceptable matches are `unreachable!()` calls with an `Internal`
invariant comment. Tests, benches, fixtures, and `mc-cli` may use
`expect("static reason")`. `mc-core/src/` may not.

### 2.4 Silent type coercion via `as` casts

**The trap.** `value as f64`, `count as u32`, `len() as i64` — these all
violate "no silent type coercion" if the source data came from a cell.

**The countermeasure.** `as` casts are only allowed for index arithmetic on
internal structures (e.g., `usize → u32` for an `ElementId` you're
constructing). Casts on `ScalarValue` payloads or anything that crosses a
cell boundary must go through explicit checked conversions that return
`Result<_, EngineError::TypeMismatch>`. If you're about to write `as`,
ask: "Is this internal bookkeeping or is this a value flowing through the
engine?" If the latter, you need a checked conversion.

### 2.5 Null / 0.0 / NaN conflation

**The trap.** Rust's `f64` is forgiving. You'll be tempted to use `0.0` as a
default for "no value," `f64::NAN` as a sentinel, or `==` to compare floats.

**The countermeasure.**
- `Null` is `ScalarValue::Null`, a distinct enum variant. It is NOT `0.0`.
- `Div` by zero returns `Null` (per §7 of the brief), not `f64::INFINITY` and
  not `f64::NAN`.
- NaN must never enter storage. Reject at writeback (§3.18, step "NaN check").
- Float comparisons in tests use `< 1e-9` epsilon, never `==`.
- Re-read §7 ("Null and arithmetic semantics") before implementing any rule
  evaluation. Print the table and keep it next to your editor.

### 2.6 Test-fudging when stuck

**The trap.** A test fails. The fastest path to green is to loosen the
assertion. You will rationalize this as "the spec was over-specified."

**The countermeasure.** Test names and assertions in §10 of the brief are
**contracts**. If a test fails:
1. First, assume the *implementation* is wrong. Spend 30 minutes proving
   that before considering the test could be wrong.
2. If after that you genuinely believe the test is wrong, **stop and
   surface it**. Do not edit the test. Write a note in the chat explaining
   what's wrong with it and why, and wait for confirmation.
3. Bumping a numeric bound (e.g., dirty-set size from 215 to whatever you
   got) is *always* test-fudging. The bound is derived from spec; if you
   exceed it, your dirty propagation is over-marking.

### 2.7 Adding traits "for testability"

**The trap.** You'll want to make `HashMapStore` implement a `CellStore` trait
"so we can mock it in tests." The brief explicitly says no trait, no trait
object, in Phase 1.

**The countermeasure.** Concrete types only. Tests use the real `HashMapStore`.
The `CellStore` trait is documented as a Phase 2 introduction. If you find
yourself reaching for `impl Trait for X`, ask: "Does the brief require this
trait?" If no, don't add it.

### 2.8 Recursive evaluation that doesn't track reads

**The trap.** When evaluating a rule body, you'll write a clean recursive
`eval(expr) -> ScalarValue` function and forget that the engine is contractually
required to *capture* every coordinate read during eval and validate it
against `declared_dependencies`. This is a §3.10 / §10.7 invariant
(`doctrine_no_silent_dependency_miss`).

**The countermeasure.** Your evaluator signature should be something like:
```rust
fn eval(expr: &Expr, ctx: &mut EvalCtx) -> Result<ScalarValue, EngineError>
```
where `EvalCtx` accumulates a `Vec<CellCoordinate>` of actual reads. After
evaluation, compare that vec against the rule's `declared_dependencies`
expanded over the target coord. Any actual read that's not in the declared
set is `EngineError::UndeclaredDependency`.

### 2.9 Forgetting hierarchy rollups in dirty propagation

**The trap.** You'll implement dirty propagation by walking
`deps.reverse_edges` and forget that hierarchy ancestors must also be marked
dirty. The §10.1 dirty-set tests will catch this — but only if you actually
run them.

**The countermeasure.** §8 of the brief is the verbatim algorithm for dirty
propagation. Implement it step-by-step in that order. Both rule dependents
*and* hierarchy ancestors must be in the dirty set after a write.

### 2.10 Wrong CPC consolidation (simple sum vs weighted average)

**The trap.** `Spend` consolidates with `Sum`. CPC, CVR, Close_Rate, AOV,
COGS_Rate, and the derived ratio measures consolidate with *weighted average*
(weighted by Spend). Your instinct is to default to Sum everywhere.

**The countermeasure.** Per the brief, every measure has an explicit
`AggregationRule`. Read it from the measure's `MeasureMeta`. Never default
to Sum. The test `t_acme_read_consolidated_cpc_uses_weighted_average`
specifically asserts the value is *not* equal to either simple sum or
simple average.

### 2.11 HashMap iteration nondeterminism in tests

**The trap.** `ahash::AHashMap::iter()` order is nondeterministic across
runs. Tests that compare iterated sequences directly will flake.

**The countermeasure.** `HashMapStore::iter()` itself **may return
nondeterministic order** — do not pay sort cost on every iteration. Any
test (or other caller) that needs deterministic order must collect into a
`Vec` and sort by `CellCoordinate` before asserting:

```rust
let mut entries: Vec<_> = store.iter().collect();
entries.sort_by(|(a, _), (b, _)| a.cmp(b));
```

This is a **CLAUDE.md amendment to brief §15 step 6**, which reads
"`iter()` ordering (deterministic via key sort)" — that wording is
ambiguous and the engineering-correct interpretation is "tests sort,"
not "iter() sorts internally." Consolidation walks targeted coordinates
via `read()`, not via `iter()`, so the hot path is unaffected. If/when
the brief is updated to clarify, this section should be re-aligned.

### 2.12 Premature SIMD / parallelism / arena allocation

**The trap.** You'll see Phase 1B benchmark targets and start optimizing.

**The countermeasure.** Phase 1A targets are 20× looser than 1B for a reason.
Phase 1 ships when 1A passes, period. No SIMD, no rayon, no bumpalo, no
custom allocators. A `HashMap`, recursive rule eval, and `Box<Expr>` is the
expected shape. Optimization is Phase 2 and it must be justified by 1B
benchmarks documented in `PERF.md`.

### 2.13 Skipping `cargo fmt` / `cargo clippy` / `cargo test` before claiming done

**The trap.** "I think it works, let me tell the user." The fmt or clippy
output then has 47 warnings and 12 unused imports.

**The countermeasure.** "Done" requires all three to pass. See §6 below.

### 2.14 Hallucinating crate APIs

**The trap.** You'll write `smallvec::SmallVec::with_capacity_in(...)` or some
other API that doesn't exist in the pinned version.

**The countermeasure.** Before importing a function from `smallvec`,
`ahash`, `thiserror`, `once_cell`, `criterion`, `proptest`, or `insta`,
check the version pinned in `Cargo.toml`. If you're unsure of an API,
write a 5-line `examples/check.rs` script that imports and calls the
function, run `cargo check --example check`, and confirm it compiles
*before* using it in the kernel.

### 2.15 `Cube::read` taking `&self` when it needs `&mut self`

**The trap.** Read APIs feel like they should be immutable. But the read
algorithm in §6 of the brief mutates the dependency graph (lazy population),
the cell store cache (caching computed values), and the dirty tracker
(clearing dirty after recompute). It needs `&mut self`.

**The countermeasure.** `Cube::read` and `Cube::read_with_trace` take
`&mut self`. Don't fight this. If the borrow checker is mad about
concurrent reads, that's the *correct* signal — Phase 1 is single-threaded.

### 2.16 Coordinate slot order

**The trap.** You'll build `CellCoordinate` from a `HashMap<DimensionId,
ElementId>` and the slots will be in some other order.

**The countermeasure.** The canonical order is `[Scenario, Version, Time,
Channel, Market, Measure]`. The `Cube` stores its dimensions in this order;
`CellCoordinate` slots are indexed positionally against `cube.dimensions`.
A coordinate built any other way is wrong, even if it "happens to compare
equal."

### 2.17 Recursive Expr eval blowing the stack

**The trap.** With deep enough rule chains, naive recursion can stack-overflow.
Phase 1 chains are at most 5 deep so this is unlikely *in practice* — but
proptest may generate deeper synthetic chains.

**The countermeasure.** Stack-recursive eval is fine for Phase 1. Don't
preemptively switch to an explicit stack. If a proptest case overflows,
that's a bug to surface, not silently mitigate.

### 2.18 Snapshot misimplementation

**The trap.** You'll try to make `Snapshot` clever — copy-on-write, partial
refs, etc. Phase 1 says snapshot is a clone of the store.

**The countermeasure.** `Snapshot` holds a `HashMapStore` by value. Taking
a snapshot is `store.clone()`. Rolling back is `cube.store = snapshot.store`.
Done. No COW. No persistence. No cleverness.

---

## 3. Strict requirements — Syntax

These are non-negotiable formatting and idiom rules. If you write code that
violates any of these, fix it before declaring the file done.

### 3.1 Forbidden patterns (will fail review)

| Pattern | Why forbidden | What to do instead |
|---|---|---|
| `.unwrap()` in `mc-core/src/` | §12 acceptance criterion 10 | Return `Result<_, EngineError>` |
| `.expect(...)` in `mc-core/src/` | Same | Same — `Result` everywhere |
| `panic!()` in `mc-core/src/` | Same | `EngineError::Internal` |
| `unsafe` anywhere | §13 of brief | Find a safe alternative; if none, stop and ask |
| `serde::*` imports | Banned dep | None — Phase 1 has no serialization |
| `tokio::*`, `async fn`, `.await` | Banned | Phase 1 is sync only |
| `Box<dyn Trait>` for storage | §3.9 | Use concrete `HashMapStore` |
| `as f64` / `as i64` / `as u32` on cell values | Silent coercion | Checked conversion through `EngineError::TypeMismatch` |
| `value == 0.0` (or any float `==`) | Float comparison hazard | `(a - b).abs() < 1e-9` |
| `value == f64::NAN` | NaN never equals itself | `value.is_nan()` |
| `f64::INFINITY` as a return value | §7 says div-by-zero is Null | Return `ScalarValue::Null` |
| `HashMap<…>::iter()` in tests asserting order | Nondeterministic | Sort by key first |
| `println!()` in `mc-core/` | §12 criterion 10 | None — only `mc-cli` may print |

### 3.2 Required patterns

| Pattern | Where required |
|---|---|
| `///` doc comments on every public item | All public types/fns in `mc-core` |
| `// Per engine-semantics.md §X I-Y-Z: …` comments | At every invariant enforcement point |
| `#[derive(Debug)]` on every public type | All public types |
| `#[derive(Clone)]` on `HashMapStore`, `Cube`, `Snapshot` | These specifically |
| `thiserror::Error` for `EngineError`, `WritebackError` | Both error enums |
| `#[non_exhaustive]` on public error enums | `EngineError`, `WritebackError`, `MeasureRole` if growable |
| `#[must_use]` on `Result`-returning functions | All fallible APIs |

### 3.3 Naming discipline

- Every public type, function, module, trait, and test name in §3 and §10
  of the brief is **exact**. No synonyms, no abbreviations, no "more idiomatic"
  variants.
- Newtype IDs use `pub struct FooId(u64);` shape consistently.
- Builder methods return `Self` by value (not `&mut Self`) for fluent chains
  that match the brief's worked example in §5.
- Test functions are `t_<scenario>` for §10.1–10.6, `doctrine_<rule>` for
  §10.7, snake_case throughout.

---

## 4. Strict requirements — Testing

### 4.1 Test contract

§10 of the brief lists 60+ tests by exact name. Every one of them must:

1. **Exist** as a `#[test] fn t_…` or `#[test] fn doctrine_…` in the file
   indicated by the brief's section header (e.g., `acme_demo.rs`,
   `correctness.rs`).
2. **Match the assertion described** in the comment beneath the test name.
3. **Pass** by the time you declare the milestone done.

Do **not**:
- Rename a test "for clarity."
- Comment out a test as "TODO."
- Replace a strict assertion with a looser one.
- Combine two tests "because they overlap."
- Skip a test under `#[ignore]`.

### 4.2 Determinism rule

`cargo test --workspace` must produce identical pass/fail status across 10
consecutive runs. This means:

- Tests that depend on iteration order must collect-and-sort (see §2.11).
- No test depends on wall-clock time except via `written_at: 0` for
  fixture data. Real timestamps are fine in `mc-cli`, never in tests.
- When proptest comes back (currently deferred per §1.1), proptest tests
  use a fixed seed (`PROPTEST_CASES=256` and a deterministic RNG seed,
  set in `proptest.toml` at the workspace root or via
  `proptest::test_runner::Config { failure_persistence: None, ..Default }`
  with a seeded source).

### 4.3 Float assertions

Every floating-point assertion in tests uses an epsilon:
```rust
assert!((actual - expected).abs() < 1e-9, "got {actual}, expected {expected}");
```
The brief uses `1e-9` as the canonical tolerance. Don't loosen it.

### 4.4 Test-driven order

When implementing a module, write the test first (or copy it from the brief
if it's listed in §10), confirm it fails, then implement. This catches:

- Type signature mismatches between what you're building and what the test
  expects.
- Missing constructors / builder methods.
- Naming drift (you typoed `Concolidator` instead of `Consolidator`).

---

## 5. Strict requirements — Implementation

### 5.1 Implement in the order in §15 of the brief

The brief's §15 is a sequenced path. Follow it. Do not jump ahead to
`Cube::write` because it's "the interesting bit." Each step depends on the
previous step's types being stable.

### 5.2 One module per session boundary

When working on a module (e.g., `dimension.rs`):
1. Read the brief section that defines its types (§3.X).
2. Read the engine-semantics section that defines its semantics (§Y of
   `engine-semantics.md`).
3. Write the public type signatures with `///` doc comments referencing
   the spec section.
4. Write the constructors / builders.
5. Write the tests listed in §15 for this module.
6. Implement until tests pass.
7. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test -p mc-core` for the relevant test files.
8. Commit. Move on.

Do not work across module boundaries unless the brief explicitly requires
it (e.g., `cube.rs` necessarily references most other modules).

### 5.3 Spec-comment every invariant (once per enforcement site)

Every place you enforce an invariant from `engine-semantics.md`, leave a
comment of the form:

```rust
// Per engine-semantics.md §13 I-WB-2: writeback rejects derived cells.
if measure.role == MeasureRole::Derived {
    return Err(WritebackError::DerivedCellNotWritable { … });
}
```

**One comment per enforcement site, not per branch.** The comment goes at
the function or method that owns the invariant — usually the public API
boundary that decides whether to error or proceed. If a private helper
does the actual check, comment the helper. Do not repeat the same `// Per
engine-semantics.md §X` on every internal branch inside an enforcing
function — the enclosing comment covers them.

Heuristic: if you've already written `// Per engine-semantics.md §X` once
in a function, a second one in the same function is noise. If a helper
gets called from multiple enforcement contexts, the helper itself
documents its invariant once.

Major boundaries that always need this comment:

- Writeback validation (cell-not-writable, locked-version, type-mismatch, NaN reject)
- Coordinate construction and validation (dim count, dim order, cross-cube reject)
- Rule registration (cycle detection, undeclared dependency, well-typed body)
- Hierarchy build (cycle detection, weight validation)
- Consolidation strategy selection (Sum / WeightedAverage / Min / Max per measure)
- Permission and lock checks (the `permissions.check` and `locks.check_write` calls)
- Frozen-dimension mutation rejection
- Snapshot cube-ID match check

If a section of code does none of those things, it doesn't need a spec
reference comment.

### 5.4 Errors over panics

Every fallible operation returns `Result<_, EngineError>` (or
`Result<_, WritebackError>` for writeback paths). Construct errors with
their full struct fields populated — don't lose context with bare variants.

```rust
// Bad
return Err(EngineError::CoordinateMismatch);

// Good
return Err(EngineError::CoordinateMismatch {
    expected_dims: cube.dimensions.iter().map(|d| d.id()).collect(),
    actual_dims: coord.dim_ids().to_vec(),
});
```

### 5.5 Public surface minimalism

Default to `pub(crate)` over `pub`. The only types that are `pub` are those
listed in §3 of the brief. Internal helpers are `pub(crate)` or private.

### 5.6 Module re-exports

`mc-core/src/lib.rs` re-exports the public surface. Match the brief's §3
ordering. Do not add `pub use` for items that aren't in §3.

---

## 6. The self-check protocol — run before declaring ANY task done

This is the gate. Before saying "this is complete," "milestone X is ready,"
or "Phase 1 is shippable," run **every step** below and report the results.

### 6.1 The build gate

```bash
# 1. Format check (no diffs)
cargo fmt --check --all

# 2. Lint clean (zero warnings)
cargo clippy --all-targets --workspace -- -D warnings

# 3. Build clean (zero warnings)
cargo build --release --workspace

# 4. Tests pass (workspace-wide)
cargo test --workspace --all-features

# 5. Determinism check (10 runs, identical pass/fail)
for i in {1..10}; do cargo test --workspace --all-features -q || echo "FAIL run $i"; done
```

If any of these fail or produce warnings, you are **not** done. Fix and re-run.

### 6.2 The forbidden-pattern grep

```bash
# Should return zero matches in mc-core/src/
grep -rn "\.unwrap()\|\.expect(\|panic!(\|unimplemented!(\|todo!(" crates/mc-core/src/

# Should return zero matches anywhere
grep -rn "unsafe" crates/mc-core/src/

# Should return zero matches anywhere
grep -rn "use serde\|use tokio\|use rayon\|use anyhow" crates/

# Should return zero matches in mc-core (println is for mc-cli only)
grep -rn "println!\|eprintln!\|dbg!" crates/mc-core/src/
```

Any matches are violations. Fix them.

### 6.3 The naming-conformance check

For every public type/function/test in the file you just modified, verify it
appears verbatim in §3 or §10 of the brief. If it doesn't appear and you
created it, ask yourself: should this be `pub(crate)` instead? If yes, demote
it. If no, you've added something not in the contract — stop and ask.

### 6.4 The benchmark gate (only when claiming Phase 1 done)

> **Active as of Phase 1B (2026-05-01).** Criterion 0.5 builds on
> Rust 1.78 with three transitive pins in `Cargo.lock` (see §1.1).
> The five bench files live in `crates/mc-core/benches/` and are wired
> via `[[bench]]` entries in `crates/mc-core/Cargo.toml`.

```bash
cargo bench --workspace
```

Compare every benchmark result against its 1A ceiling in §11 of the brief.
Any benchmark above its 1A ceiling is a ship-blocker, **with two
documented Phase 1B caveats** that only Phase 2A can resolve. Do **not**
quote those caveats as "the ceilings passed":

- **Brief §11.2 consolidation rows** (cold-read ceilings) are not
  comparable to today's `consolidated_read.rs` numbers. The benches
  measure warm-cache hits (~67 ns); the brief's 50 µs / 1 ms / 20 ms /
  5 ms / 2 ms ceilings were calibrated against cold reads. See
  [`docs/PERF.md`](docs/PERF.md) §6.3 banner + §7.4. Phase 2A's first
  task is to add cold-path variants (PERF.md §9.1).
- **`write_input_leaf_no_deps`** (1A < 50 µs) measures ~165 µs on Acme,
  which equals `write_input_leaf` because every write pays the
  hierarchy ancestor mark walk regardless of rule fan-out. The brief's
  "no-deps" condition implicitly assumes a synthetic no-hierarchy cube
  that does not exist yet. Phase 1B accepts this as a benchmark-scope
  mismatch; Phase 2A should add the synthetic fixture before treating
  the ceiling as either met or missed (PERF.md §7.3, §9.3).

Don't loosen ceilings to "pass" them. Don't optimize the kernel against
warm-cache numbers. Treat both caveats as **measurement gaps**, not
performance failures.

1B targets are not ship-blockers but should be logged in `PERF.md`.

### 6.5 The CLI demo gate

```bash
cargo run --release --bin mc -- demo
```

Output must match the shape in §4.6 of the brief. Numbers must match
`golden_inputs()` byte-for-byte.

### 6.6 The acceptance-criteria checklist

Before the final "Phase 1 done" claim, walk every one of the 10 items in
§12 of the brief and report status for each:

- [ ] (1) `cargo build --release --workspace` zero warnings
- [ ] (2) `cargo clippy --all-targets --workspace -- -D warnings` exits 0
- [ ] (3) `cargo fmt --check --all` exits 0
- [ ] (4) `cargo test --workspace` 100% pass (excluding §10.7 proptest stubs deferred per §1.1)
- [ ] (5) `cargo bench --workspace` every bench under its 1A ceiling — Phase 1B (2026-05-01) shipped tooling + baseline; 8/14 §11 ceilings directly comparable and pass; 6 §11.2 consolidation ceilings are warm-only (cold-path benches deferred to Phase 2A); 1 §11.1 row over ceiling as documented benchmark-scope mismatch. See [`docs/PERF.md`](docs/PERF.md) §6.3, §7.3, §7.4.
- [ ] (6) `target/release/mc demo` matches §4.6 output
- [ ] (7) `docs/specs/engine-semantics.md` and `docs/specs/phase-1-rust-kernel-build-brief.md` unchanged
- [ ] (8) No `mc-core` reference to any §1 out-of-scope item
- [ ] (9) 10 consecutive `cargo test` runs identical
- [ ] (10) Zero `unwrap()`/`expect()` in `mc-core/src/` (greps clean)

Report this checklist explicitly in the chat when claiming done. Do not
say "all done" without it.

---

## 7. Decision trees for common pitfalls

### 7.1 "Should I add this dependency?"

```
Is it in §2.5 of the brief?
├── Yes → use it.
└── No  → STOP.
         Can the same thing be done with std + the existing 4 deps?
         ├── Yes → do that. Inline a small helper if needed.
         └── No  → stop and ask. Update the brief or find an alternative.
```

### 7.2 "Should I add this trait?"

```
Is the trait listed in §3 of the brief?
├── Yes → implement exactly the trait shape in §3.
└── No  → STOP.
         Am I trying to enable testing/mocking?
         ├── Yes → tests use real types in Phase 1. No mock traits.
         └── No  → am I trying to abstract over multiple impls?
                   ├── Yes → Phase 1 has at most one impl per concept. No.
                   └── No  → why am I adding a trait? Probably don't.
```

### 7.3 "A test is failing. What do I do?"

```
Is the test name in §10 of the brief?
├── Yes → the test is contractual. The implementation is wrong.
│         Debug the implementation. Do not edit the test.
│         If after 30 minutes I genuinely think the test is wrong,
│         STOP and explain in chat. Do not edit it silently.
└── No  → I wrote this test. Is the implementation under test correct
          per the brief?
          ├── Yes → the test I wrote is wrong. Fix the test.
          └── No  → fix the implementation.
```

### 7.4 "I want to clone() something to make borrow checker happy"

```
Is the cloned thing inside a hot path (read, write, eval, consolidate)?
├── Yes → benchmark it. If 1A ceiling is at risk, restructure the
│         lifetime. Otherwise it's fine for Phase 1.
└── No  → it's fine. Move on.
```

### 7.5 "I want to add a `From<X> for Y` impl"

```
Does it convert between two types that the brief explicitly defines as
distinct (e.g., ScalarValue → CellValue with default Provenance)?
├── Yes → STOP. The brief wants explicit construction. From-impl masks
│         where defaults come from.
└── No  → if it's between an internal type and its newtype wrapper, fine.
          Otherwise, default to a named constructor (Foo::from_bar(...))
          rather than a From impl.
```

### 7.6 "Floating point comparison"

```
Am I comparing two floats?
├── In test → use `(a - b).abs() < 1e-9`.
├── In rule eval → never compare; arithmetic flows through.
├── For zero-check in division → use `value.abs() < 1e-300` to detect
│   zero-ish-and-treat-as-zero (per §7 of brief).
└── For NaN check → `value.is_nan()`. Reject at writeback.
```

---

## 8. Forbidden phrases (red flags in your own reasoning)

If you catch yourself thinking or writing any of these, **stop and recheck**:

- "This would be easy to add while I'm here…" — scope creep
- "It's basically the same as…" — naming drift
- "The test is over-specifying this…" — test-fudging incoming
- "For clarity I'll rename…" — contract violation
- "A trait would be cleaner…" — premature abstraction
- "Let me just `.unwrap()` for now…" — it stays
- "This optimization is obvious…" — Phase 1A is naive
- "I'll come back to that comment later…" — you won't
- "The error case is unreachable…" — return `Result` anyway
- "I think this is right…" — verify against the spec section before proceeding

---

## 9. Definition of Done — per task

A task is done when **all** of the following are true:

- [ ] Code compiles clean (`cargo build --release` zero warnings).
- [ ] Clippy clean (`cargo clippy --all-targets -- -D warnings`).
- [ ] Formatted (`cargo fmt --check`).
- [ ] All tests in §10 covering this module pass.
- [ ] Forbidden-pattern grep shows zero matches.
- [ ] Every public item has a `///` doc comment referencing the spec.
- [ ] Every invariant enforcement point has a `// Per engine-semantics.md §X` comment.
- [ ] No new dependency introduced.
- [ ] No item in §1 (out of scope) is implemented or referenced.

If a task is part of Phase 1 milestone completion, also run §6.4–6.6.

---

## 10. Recovery protocol — when you've gone off the rails

Sometimes you'll realize partway through that you've drifted: added a trait
that shouldn't be there, used `unwrap()` everywhere, named a struct wrong.
The recovery move depends on how deep you are.

### 10.1 If <30 minutes of work in

`git checkout -- .` and start over from the brief. The fastest fix.

### 10.2 If 30 minutes – 2 hours in

Stop coding. List every drift in chat:
- "I added trait `X` not in spec — I'll remove it."
- "I named the struct `Y` instead of `Z` — renaming."
- "I have N `unwrap()` calls — converting to `Result`."

Make a fix-up plan, get confirmation, then execute it as a separate commit
labeled "fix: align with brief §X."

### 10.3 If you've shipped a milestone with drift

Stop. Do not start the next milestone. Open an issue listing the drift, then
remediate it before resuming. Drift compounds; the longer you let it sit,
the more later code depends on it.

---

## 11. Communication protocol with the human

When asking for clarification, use this format:

```
SPEC QUESTION: [one-line summary]

Context: [where in the brief this came up]
Spec text: [literal quote from engine-semantics.md or brief]
The conflict / ambiguity: [what's unclear]
My proposed interpretation: [your best guess]
What I would do without confirmation: [the conservative path]
```

Do not say "I'll just guess" or proceed silently when you're unsure.

When reporting completion, use this format:

```
DONE: [task name]

Build:    cargo build --release --workspace ✓
Format:   cargo fmt --check --all          ✓
Lint:     cargo clippy ... -- -D warnings  ✓
Tests:    cargo test --workspace           [N]/[N] passed
Greps:    forbidden patterns               0 matches
Spec:     [list of §X invariant comments added]
Files:    [list of files touched]

Notes: [anything the human should know]
```

---

## 12. The one-line summary

> **Implement the brief literally, run the gates honestly, surface every
> deviation explicitly. Your taste is not invited to this party.**

---

*End of `CLAUDE.md`. If this file conflicts with anything in `docs/`, the
docs win. If you're about to violate this file because "the situation calls
for it" — the situation does not call for it.*
