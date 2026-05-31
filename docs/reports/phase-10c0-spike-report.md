# Phase 10C.0 Spike Report — Param-Recompute (gates all of Phase 10C)

**Verdict: 🟢 GREEN — zero kernel change.** When you change a `param(name)`
value and re-read a Derived measure that depends on it, the value **MOVES**
— *provided the read happens against a cube whose derived caches have not
already been populated for the prior param value.* The existing
`snapshot` → `rollback_to(snapshot)` → mutate-`reference_data` → read
pattern that `sweep.rs` already uses for the `coef:` axis works
**identically** for a `param:` axis, because `rollback_to` (cube.rs:2801)
busts both derived caches on every iteration. `mc model backtest`'s
primary axis can be built as the planned CLI composition with **no
`mc-core` change**. AC #17 ("zero kernel change") **holds**.

The naive in-place "mutate the param on a cube you've already read from,
then re-read" *does* serve stale cache (sub-question b, below) — but that
is **not** the pattern backtest needs to use, and it is the same hazard
`coef:` sweeps already avoid via `rollback_to`. The mechanism the two
reviewers flagged as possibly-nonexistent exists; it just isn't the
in-place setter they looked for — it's the rollback-per-point loop.

---

## The two sub-questions, answered

### (a) SETTER — is there an in-place `param(name)` override API? **No dedicated setter; the field is `pub`.**

There is **no** `Cube::set_parameter(name, value)` method. But
`Cube.reference_data` is a `pub` field (cube.rs:86) and
`ReferenceData.parameters` is a `pub AHashMap<String, f64>` (cube.rs:3070).
A caller mutates a param in one line:

```rust
cube.reference_data.parameters.insert("threshold".to_string(), 0.20);
```

This is **exactly** how `sweep.rs` overrides coefficients today — it
reaches into the same `pub reference_data` and mutates a `get_mut` entry
in place (`override_coefficient`, sweep.rs:369–384):

```rust
fn override_coefficient(cube: &mut Cube, model_name: ..., coeff_index: ..., value: f64) -> bool {
    if let Some(model_data) = cube.reference_data.fitted_models.get_mut(model_name) {
        if coeff_index < model_data.coefficients.len() {
            model_data.coefficients[coeff_index].1 = value;   // in-place pub-field mutation
            ...
```

So the "setter" question is a non-issue: parameters are reached the same
way coefficients are. A thin `Cube::set_parameter` could be added for
ergonomics later, but it is **not required** for 10C.1 — the field is
already public and writable. (If 10C.1 wants the ergonomic wrapper, it is
a ~5-line additive method, not a behavior change.)

### (b) RECOMPUTE / CACHE — does the derived cell recompute? **Yes, with `rollback_to`; no, on a naive in-place re-read.**

Two distinct cases, both proven by tests in
`crates/mc-model/tests/param_recompute_spike.rs`:

| Case | Pattern | Result | Test |
|---|---|---|---|
| Fresh / pre-cache | set param **before** first read | **MOVES** | `spike_param_mutation_before_first_read_is_picked_up` |
| Naive in-place | read, set param, **re-read same cube** | **STALE** | `spike_param_mutation_after_first_read_serves_stale_cache` |
| **Sweep pattern** | read, snapshot, **`rollback_to` → set param → read** (per point) | **MOVES** | `spike_sweep_pattern_rollback_makes_param_move` |
| Control | two cubes compiled fresh w/ different param | MOVES | `spike_control_fresh_cubes_show_param_matters` |

All four tests pass:

```
$ cargo test -p mc-model --test param_recompute_spike 2>&1 | tail -8
running 4 tests
test spike_param_mutation_before_first_read_is_picked_up ... ok
test spike_param_mutation_after_first_read_serves_stale_cache ... ok
test spike_sweep_pattern_rollback_makes_param_move ... ok
test spike_control_fresh_cubes_show_param_matters ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

---

## Why the naive in-place re-read goes stale (and why it doesn't matter)

The reviewers were right that a param change participates in **no** dirty
propagation. cube.rs:3069 (the suspect line), verbatim:

> Phase 3J item 3: named scalar constants from the `parameters:` YAML
> block ... Resolution is a single HashMap lookup at eval time; **no
> dependency-graph participation (constants don't participate in dirty
> propagation).**

So a bare `parameters.insert(...)` neither marks any cell dirty nor bumps
the cube revision. The derived-leaf cache check in `read_derived_leaf`
(cube.rs:509–535) then stays "fresh":

```rust
let has_edges = !self.deps.dependencies_of(coord).is_empty();   // true: rule reads `signal`
let cached_fresh = !self.dirty.is_dirty(coord)                  // not dirtied by param change
    && self.store.read(coord).map(|s| {
        if has_edges { /* time_anchor check only — passes */ }
        else { s.revision == self.revision }
    }).unwrap_or(false);
if cached_fresh && !request_trace { return /* STALE cached value */ }
```

The consolidated cache (cube.rs:~800–813) keys on
`s.revision == self.revision` and would *also* go stale, since a param
change doesn't bump revision.

**`rollback_to` is the cache-bust that makes the sweep pattern work**
(cube.rs:2801–2832). On every call it:

1. `self.revision = self.revision.next()` — **busts the consolidated
   cache** (snapshot-restored consolidated cells now carry a stale
   revision, so `s.revision == self.revision` is false).
2. `self.dirty.clear_all()` and `self.time_anchor_cache.clear()`.
3. **Prunes every `Provenance::Rule { .. }` cell from the store** — so
   the derived-leaf cache finds *no stored value* on the next read
   (`store.read(coord)` → `None` → `cached_fresh = false`) and recomputes
   against current `reference_data`.

This is precisely why `coef:` sweeps already produce moving metrics
despite coefficients being non-dirty-participating `reference_data`: the
recompute is driven by the `rollback_to` at the top of each sweep
iteration (sweep.rs:283), **not** by the override marking anything dirty.
Parameters inherit the same free lunch.

---

## Recommended 10C.1 path + estimate

**Build `param:` exactly as `coef:` is built today** (sweep.rs:273–320 is
the template):

1. Load + compile the model once.
2. Evaluate the baseline metric.
3. Take **one** snapshot (`cube.snapshot(...)`).
4. For each param value in the sweep grid:
   a. `cube.rollback_to(&snapshot)` (busts both caches),
   b. `cube.reference_data.parameters.insert(name, value)` (the override),
   c. evaluate the metric / read the derived measures.

This is a new `SweepTarget::Parameter { name }` variant alongside the
existing `Coefficient` and `Cell` variants, plus the one-line override and
a `--param name` arg parse. It is **pure `mc-cli` composition**; `mc-core`
is untouched.

**Estimate (10C.1 param-axis portion only):** ~30–50 LOC in `mc-cli`
(`SweepTarget::Parameter` variant, arg parse, the rollback+insert+eval
arm). No new caches, no kernel function, no ADR. Test surface: one
integration test mirroring `spike_sweep_pattern_rollback_makes_param_move`
against a realistic model, plus the existing sweep test patterns.

**Optional ergonomic add (not required):** a `Cube::set_parameter(&mut
self, name: &str, value: f64)` wrapper (~5 LOC, additive, no behavior
change) if 10C.1 prefers a named method over the raw `pub`-field insert.
This is a style choice, not a gate.

### One guardrail for the 10C.1 implementer

Do **not** override the param *without* a preceding `rollback_to` (or
without using a never-read cube). The naive in-place "set then re-read on
an already-evaluated cube" serves stale cache — proven by
`spike_param_mutation_after_first_read_serves_stale_cache`. Following the
`coef:` template (rollback per point) avoids this entirely. If 10C.1 ever
needs to mutate a param on a hot cube *without* rollback, that — and only
that — would require the additive cache-bust (the YELLOW path), which is
**not** the recommended design.

---

## What this spike did and did not do

- **Did:** build a minimal param-dependent derived cube, prove the param
  materially changes the derived output (control), and characterize all
  three override orderings (pre-cache, naive in-place, rollback-sweep).
- **Did NOT:** build the `backtest` command (10C.1), touch `mc-core`, add a
  setter to the kernel, or fold in the other 7 ADR-0036 amendments.
- **Spike artifact:** `crates/mc-model/tests/param_recompute_spike.rs`
  (4 tests, marked SPIKE CODE in the module doc). The `pub`-field insert
  used in the tests is the same mechanism `override_coefficient` uses; it
  is representative of the real 10C.1 path, not throwaway scaffolding.

## Gates run

```
cargo test -p mc-model --test param_recompute_spike   → 4 passed; 0 failed
cargo fmt --check -p mc-model                          → clean
cargo clippy -p mc-model --tests                       → clean (no warnings)
```

## Source lines that explain the behavior

- `crates/mc-core/src/cube.rs:3069` — params "don't participate in dirty propagation" (the flagged line).
- `crates/mc-core/src/cube.rs:86` + `:3070` — `pub reference_data` / `pub parameters` (the "setter" is a pub field).
- `crates/mc-core/src/cube.rs:509–535` — `read_derived_leaf` `cached_fresh` (why naive in-place re-read is stale).
- `crates/mc-core/src/cube.rs:~800–813` — `read_consolidated` `cached_fresh` (revision-keyed consolidated cache).
- `crates/mc-core/src/cube.rs:2801–2832` — `rollback_to` (revision bump + Rule-cell prune = the cache-bust).
- `crates/mc-cli/src/sweep.rs:273–320` — the snapshot/rollback sweep loop (the composition template for 10C.1).
- `crates/mc-cli/src/sweep.rs:369–384` — `override_coefficient` (in-place `pub reference_data` mutation, the pattern `param:` parallels).
