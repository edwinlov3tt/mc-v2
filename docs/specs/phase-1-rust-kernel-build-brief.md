# MarketingCubes — Phase 1 Rust Kernel Build Brief

**Status:** Definitive. This is the document Claude Code will execute against.
**Audience:** Rust implementer (Claude Code or human).
**Source of truth for semantics:** [engine-semantics.md](./engine-semantics.md) — every concept used here is defined there.
**Phase 1 boundary:** everything below is in scope. Anything not below is out of scope and **must not be added** without an updated brief.

---

## 0. Phase 1 mission statement

Build the smallest Rust planning kernel that can execute the Acme demo end-to-end.

The Acme demo:

> Define dimensions for Scenario, Version, Time, Channel, Market, Measure with
> hierarchies for Time (Month → Quarter → Year), Channel (Channel → Channel_Group),
> and Market (City → State → Region). Write leaf-level Spend, CPC, CVR, Close_Rate,
> AOV, and COGS_Rate. Compute Clicks, Leads, Customers, Revenue, and Gross_Profit
> from deterministic rules. Read consolidated values where March rolls into Q1,
> Tampa rolls into Florida, and Paid_Search rolls into Paid_Media. Reject writes to
> derived cells. Reject writes to consolidated cells. Return a complete trace for a
> Revenue cell. Pass every correctness test in §10. Hit every benchmark in §11.

If the demo runs and every test passes, Phase 1 is done. If anything below this is
missing, Phase 1 is not done. There is no partial credit.

---

## 0.A Active deviations from this brief

> **Read this before §2.5, §10.7, §11, §12, and §15 step 6.** A small number
> of brief-mandated items are temporarily inert. They are listed here in one
> place so a reader doesn't bounce between this brief and `CLAUDE.md`.

The brief was written under the assumption that `criterion`, `proptest`, and
`insta` would all be usable in `mc-core` on Rust 1.78 (the toolchain pinned
in [`rust-toolchain.toml`](../rust-toolchain.toml)). That assumption is
**currently false** because of an external blocker:

> On Rust 1.78, criterion's transitive dependency `clap_lex 1.1.0` requires
> `edition2024`, which was only stabilized in 1.85. Pulling criterion in
> breaks `cargo build`.

The same toolchain-version interaction also affects how proptest and insta
resolve, so all three are deferred together, not individually.

### What this changes (concrete list)

| Brief reference | Status | What to do today |
|---|---|---|
| §2.5 `mc-core` dev-deps include `criterion`, `proptest`, `insta` | **mc-core opts out** | Workspace declarations stay; `mc-core/Cargo.toml` does not pull them in. See §2.5.1 below. |
| §10.7 `doctrine_atomicity_of_write`, `doctrine_causality`, and other proptest tests | **Deferred** | Leave as `// TODO(proptest):` stubs in `tests/correctness.rs` with a comment pointing to this section. The non-proptest doctrine tests still run. |
| §10.1 `t_acme_trace_root_value_equals_read_value` (proptest variant) | **Deferred** | Same as above — keep the deterministic version, defer the proptest random-coord sweep. |
| §11 `benches/` directory + criterion bench harness | **Deferred** | Do not create `benches/`. Instead, leave a `crates/mc-core/BENCHES_DEFERRED.md` note pointing to this section. |
| §12 acceptance criterion (5) — `cargo bench` clears 1A ceilings | **Inert** | Phase 1 ships on correctness gates only while criterion is out. Resume the benchmark gate when it returns. |
| §15 step 18 — implement benches | **Skipped** | Same. |

### What this does NOT change

- **Determinism testing still happens** — §6.1 step 5 ("10 consecutive
  `cargo test --workspace` runs identical pass/fail") is the contractually-
  required determinism check. Proptest random-search testing is additive,
  not the determinism gate.
- **Correctness coverage stays.** Of the §10.7 doctrine tests, only the
  ones that explicitly say "proptest" or rely on proptest's case generation
  are deferred. `doctrine_determinism`, `doctrine_no_silent_type_coercion`,
  `doctrine_no_writes_to_derived_cells`, `doctrine_null_zero_distinct`,
  etc. are deterministic tests and are required.
- **Phase 1A ceilings still exist.** They become contractual the moment
  criterion returns. Keep designs honest in the meantime; don't take the
  bench-gate deferral as a license to write 50× slower code.

### When this deviation closes

Any one of these resolves the block:

1. The toolchain pin in `rust-toolchain.toml` bumps to a version that
   stabilizes `edition2024` (Rust 1.85+). At that point the brief's §2.5
   stands as written.
2. `criterion` (and/or its transitive deps) ship a release that doesn't
   require `edition2024`.
3. We pin a known-good older minor release of `criterion` (and similarly
   for `proptest`/`insta`) that compiles on 1.78.

When any of those happens: revert the deferral notes in §2.5, §10.7, §11,
§12, §15; delete `BENCHES_DEFERRED.md`; pull the three crates back into
`mc-core/Cargo.toml`; and remove the `// TODO(proptest):` stubs by
filling them in.

### The "I'll just add proptest real quick" trap

Don't. Adding proptest to `mc-core/Cargo.toml` on Rust 1.78 fails the
build. Surface the desire in chat instead — the resolution is a toolchain
or version-pin discussion, not a snap decision in a coding session.

`CLAUDE.md` §1.1 covers the same ground from the implementer's-process
angle. This section is the contract-doc mirror.

---

## 1. Out of scope for Phase 1

These are **not allowed** in Phase 1. The kernel must compile and pass tests without
any of them. Adding any of these without an updated brief is a contract violation.

- Model-backed cells (Lasso, Ridge, BayesianRidge, XGBoost, GLMs, ARIMA, etc.)
- DuckDB integration / external storage / SQL adapters
- WASM bindings (the kernel may be WASM-compatible incidentally, but no `wasm-bindgen` work)
- Python bindings (no PyO3)
- HTTP/gRPC server
- CRDTs / multi-writer concurrency / operational transforms
- LLM rule authoring / string-parsed DSL
- Schema marketplace / schema registry
- Auto-feeder inference / static analysis of rule bodies
- Persistence to disk (snapshots are in-memory only)
- Write-ahead log
- Atomic blue-green model swaps
- Cross-cube references
- Spreading writes to consolidated cells
- Multi-hierarchy per dimension (v1: one default hierarchy per dimension; alternates are Phase 2+)
- Scenario inheritance
- Version forking (lattice transitions Draft → Submitted → Approved are honored as
  state, but actual fork-with-copy-on-write is Phase 3)
- Rule priorities for overlapping scopes (v1: rules must have non-overlapping scopes; overlap is a definition error)
- Streaming slice iteration (v1: slices materialize in memory, capped at 1M cells)
- Distribution-typed uncertainty (`Uncertainty::Distribution`); only `StdDev` and `Interval` are exposed types but **no rule produces them in v1** (deterministic rules only, all inputs uncertain=None)
- Cross-revision permission diffing
- Custom aggregation functions (`AggregationRule::Custom`)
- `MeasureRole::Both` (v1: every measure is strictly `Input` or `Derived`)

If the implementer encounters a need for one of these, the answer is **stop and update
the brief**, not "add it quickly."

---

## 2. Project structure

The Phase 1 deliverable is a Cargo workspace at the repository root with three
crates. No more, no less.

```
marketingcubes/
├── Cargo.toml                      # workspace manifest
├── rust-toolchain.toml             # pinned toolchain
├── README.md
├── crates/
│   ├── mc-core/                    # the kernel
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── error.rs
│   │   │   ├── id.rs                  # newtype IDs (CubeId, DimensionId, etc.)
│   │   │   ├── revision.rs
│   │   │   ├── value.rs               # ScalarValue, CellDataType
│   │   │   ├── element.rs
│   │   │   ├── dimension.rs
│   │   │   ├── hierarchy.rs
│   │   │   ├── coordinate.rs
│   │   │   ├── cell.rs                # CellValue, Provenance
│   │   │   ├── store.rs               # CellStore trait + HashMapStore impl
│   │   │   ├── rule.rs                # Rule, Expr, RuleSet
│   │   │   ├── trace.rs               # Trace, TraceNode, TraceOp
│   │   │   ├── dependency.rs          # DependencyGraph
│   │   │   ├── dirty.rs               # DirtyTracker
│   │   │   ├── permission.rs          # PermissionTable, Grant (minimal Phase 1)
│   │   │   ├── lock.rs                # LockTable (minimal Phase 1)
│   │   │   ├── snapshot.rs            # in-memory only
│   │   │   ├── consolidation.rs       # ConsolidationStrategy
│   │   │   ├── cube.rs                # Cube + CubeBuilder
│   │   │   └── slice.rs               # SliceQuery, SliceResult
│   │   ├── tests/                  # integration tests (see §10)
│   │   │   ├── acme_demo.rs           # the canonical end-to-end test
│   │   │   ├── correctness.rs         # the Correctness Doctrine
│   │   │   ├── writeback.rs
│   │   │   ├── consolidation.rs
│   │   │   ├── trace.rs
│   │   │   ├── dependency.rs
│   │   │   └── locks_permissions.rs
│   │   └── benches/                # criterion benchmarks (see §11)
│   │       ├── leaf_read_write.rs
│   │       ├── consolidation_read.rs
│   │       └── full_recompute.rs
│   │
│   ├── mc-fixtures/                # the Acme demo cube as a reusable fixture
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── acme.rs                # build_acme_cube() builder
│   │       └── inputs.rs              # canonical input data
│   │
│   └── mc-cli/                     # tiny CLI to run the demo (no UI)
│       ├── Cargo.toml
│       └── src/
│           └── main.rs                # `mc demo` runs the Acme demo and prints results
│
└── docs/
    ├── engine-semantics.md         # already exists
    └── phase-1-rust-kernel-build-brief.md   # this document
```

### 2.1 Workspace `Cargo.toml`

```toml
[workspace]
resolver = "2"
members = ["crates/mc-core", "crates/mc-fixtures", "crates/mc-cli"]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.78"
license = "MIT OR Apache-2.0"
repository = "https://github.com/edwinlovettiii/marketingcubes"

[workspace.dependencies]
# Strict, minimal dependency set for Phase 1.
smallvec = { version = "1", features = ["const_generics", "union"] }
ahash = "0.8"
thiserror = "1"
once_cell = "1"
# For tests + benches only:
criterion = { version = "0.5", default-features = false }
proptest = "1"
insta = { version = "1", features = ["yaml"] }
```

### 2.2 Toolchain pin

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.78"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

### 2.3 `mc-core` `Cargo.toml`

> **DEVIATION ACTIVE** — see §0.A. The block below is what this section
> *would* look like if the toolchain blocker were resolved. The
> implementation today omits `criterion`, `proptest`, `insta`, and the
> `[[bench]]` entries; see §2.3.1 for the as-shipped form.

```toml
# Aspirational form — restore when §0.A's deviation closes.
[package]
name = "mc-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
smallvec.workspace = true
ahash.workspace = true
thiserror.workspace = true
once_cell.workspace = true

[dev-dependencies]
criterion.workspace = true
proptest.workspace = true
insta.workspace = true
mc-fixtures = { path = "../mc-fixtures" }

[[bench]]
name = "leaf_read_write"
harness = false

[[bench]]
name = "consolidation_read"
harness = false

[[bench]]
name = "full_recompute"
harness = false
```

#### 2.3.1 `mc-core` `Cargo.toml` — as-shipped form (active until §0.A closes)

```toml
[package]
name = "mc-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
smallvec.workspace = true
ahash.workspace = true
thiserror.workspace = true
once_cell.workspace = true

[dev-dependencies]
mc-fixtures = { path = "../mc-fixtures" }

# `criterion`, `proptest`, and `insta` are declared at workspace level
# (workspace.dependencies in the root Cargo.toml) but NOT pulled into
# mc-core. See §0.A "Active deviations" for the toolchain blocker.
# When that closes, restore the form in §2.3 and re-add the [[bench]]
# entries alongside the bench files (see §15 step 18).
```

The workspace-level `[workspace.dependencies]` declarations in §2.1 stay
unchanged — only `mc-core` opts out of pulling them in. `mc-cli` and
`mc-fixtures` never depended on them in the first place.

### 2.4 `mc-fixtures` `Cargo.toml`

```toml
[package]
name = "mc-fixtures"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
mc-core = { path = "../mc-core" }
```

### 2.5 `mc-cli` `Cargo.toml`

```toml
[package]
name = "mc-cli"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
mc-core = { path = "../mc-core" }
mc-fixtures = { path = "../mc-fixtures" }

[[bin]]
name = "mc"
path = "src/main.rs"
```

**No other crates. No additional dependencies.** If a third-party dep seems
necessary, the answer is to inline a minimal implementation, not to add the dep.

---

## 3. Module-by-module specification

This section is the implementation contract. Every public type listed must exist
with the listed signature. Names are exact.

### 3.1 `id.rs` — newtype identifiers

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct CubeId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct DimensionId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ElementId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct HierarchyId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct RuleId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct PrincipalId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct LockId(pub u64);

/// Monotonic per-cube revision counter.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Revision(pub u64);

impl Revision {
    pub const ZERO: Self = Revision(0);
    pub fn next(self) -> Self { Revision(self.0 + 1) }
}
```

ID generation in Phase 1 is **workspace-local and monotonic**, via a small
`IdGenerator` defined in this same `id.rs` module:

```rust
/// Allocates monotonically-increasing IDs for every kind of entity in the
/// engine. Phase 1 is single-threaded; the counters are plain `Cell<u64>`.
/// Phase 2+ swaps these for `AtomicU64`.
#[derive(Debug, Default)]
pub struct IdGenerator {
    cube: std::cell::Cell<u64>,
    dimension: std::cell::Cell<u64>,
    element: std::cell::Cell<u64>,
    hierarchy: std::cell::Cell<u64>,
    rule: std::cell::Cell<u64>,
    principal: std::cell::Cell<u64>,
    lock: std::cell::Cell<u64>,
}

impl IdGenerator {
    pub fn new() -> Self;
    pub fn cube(&self) -> CubeId;
    pub fn dimension(&self) -> DimensionId;
    pub fn element(&self) -> ElementId;
    pub fn hierarchy(&self) -> HierarchyId;
    pub fn rule(&self) -> RuleId;
    pub fn principal(&self) -> PrincipalId;
    pub fn lock(&self) -> LockId;
}
```

There is no `Workspace` struct in Phase 1 — IDs are unique within a single
`IdGenerator` instance, which the fixture builders thread through their helpers.
A `Workspace` is a Phase 3+ concept (see spec §20).

### 3.2 `revision.rs`

Just re-exports `Revision` from `id.rs`. Kept as a separate module for
forward-compat with snapshot/version logic.

### 3.3 `value.rs`

```rust
#[derive(Clone, PartialEq, Debug)]
pub enum ScalarValue {
    F64(f64),
    I64(i64),
    Bool(bool),
    Category(usize),
    Null,
}

impl ScalarValue {
    pub fn as_f64(&self) -> Option<f64> { ... }
    pub fn as_i64(&self) -> Option<i64> { ... }
    pub fn dtype(&self) -> CellDataType { ... }
    pub fn is_null(&self) -> bool { ... }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CellDataType {
    F64,
    I64,
    Bool,
    Category(Vec<String>),
}

impl CellDataType {
    pub fn matches(&self, value: &ScalarValue) -> bool { ... }
}
```

`ScalarValue::F64(f)` where `f.is_nan()` is **rejected at the writeback boundary**
(returns `WritebackError::InvalidValue`). NaN must never appear in storage. Phase 1
treats NaN as a programming error, not a data value.

### 3.4 `element.rs`

```rust
#[derive(Clone, Debug)]
pub struct Element {
    pub id: ElementId,
    pub name: String,
    pub dimension: DimensionId,
    /// Populated only when the parent dimension is `DimensionKind::Measure`.
    pub measure_meta: Option<MeasureMeta>,
    /// Populated only when the parent dimension is `DimensionKind::Version`.
    pub version_state: Option<VersionState>,
    /// Populated only when the parent dimension is `DimensionKind::Scenario`.
    pub scenario_meta: Option<ScenarioMeta>,
    // arbitrary user attributes are Phase 2+
}

#[derive(Clone, Debug)]
pub struct MeasureMeta {
    pub dtype: CellDataType,
    pub role: MeasureRole,
    pub aggregation: AggregationRule,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MeasureRole {
    Input,
    Derived,
}

#[derive(Clone, Debug)]
pub enum AggregationRule {
    Sum,
    WeightedAverage { weight_measure: ElementId },
    Min,
    Max,
}

/// Carried only by elements of a `DimensionKind::Version` dimension.
/// Drives writeback gating: `Approved` and `Archived` versions are read-only.
/// Per spec §9.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VersionState {
    Draft,
    Submitted,
    Approved,
    Archived,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScenarioMeta {
    Default,
    NonDefault,
}

impl Element {
    pub fn leaf(id: ElementId, name: impl Into<String>, dim: DimensionId) -> Self;
    pub fn measure(
        id: ElementId, name: impl Into<String>, dim: DimensionId,
        dtype: CellDataType, role: MeasureRole, agg: AggregationRule,
    ) -> Self;
    pub fn version(
        id: ElementId, name: impl Into<String>, dim: DimensionId, state: VersionState,
    ) -> Self;
    pub fn scenario(
        id: ElementId, name: impl Into<String>, dim: DimensionId, meta: ScenarioMeta,
    ) -> Self;
}

impl Element {
    /// Returns Some(state) only for elements in a Version dimension.
    pub fn version_state(&self) -> Option<VersionState>;
}
```

`MeasureRole::Both` is **not** in Phase 1. `AggregationRule::Custom` is **not** in
Phase 1.

### 3.5 `dimension.rs`

```rust
#[derive(Debug)]
pub struct Dimension {
    pub id: DimensionId,
    pub name: String,
    pub kind: DimensionKind,
    pub elements: Vec<Element>,
    pub element_index: ahash::AHashMap<ElementId, usize>,   // ElementId → position
    pub element_by_name: ahash::AHashMap<String, ElementId>,
    pub hierarchies: Vec<Hierarchy>,
    pub default_hierarchy: HierarchyId,
    is_frozen: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DimensionKind {
    Standard,
    Measure,
    Scenario,
    Version,
}

impl Dimension {
    pub fn builder(id: DimensionId, name: impl Into<String>, kind: DimensionKind) -> DimensionBuilder;
    pub fn element(&self, id: ElementId) -> Option<&Element>;
    pub fn element_by_name(&self, name: &str) -> Option<&Element>;
    pub fn position(&self, id: ElementId) -> Option<usize>;
    pub fn hierarchy(&self, id: HierarchyId) -> Option<&Hierarchy>;
    pub fn default_hierarchy(&self) -> &Hierarchy;
    pub fn is_measure_dimension(&self) -> bool;
    pub fn is_frozen(&self) -> bool;
    pub(crate) fn freeze(&mut self);
}

pub struct DimensionBuilder { ... }

impl DimensionBuilder {
    pub fn add_element(self, name: impl Into<String>) -> Self;
    pub fn add_measure(self, name: impl Into<String>, dtype: CellDataType,
                       role: MeasureRole, agg: AggregationRule) -> Self;
    pub fn add_hierarchy(self, h: Hierarchy) -> Self;
    pub fn default_hierarchy(self, name: impl Into<String>) -> Self;
    pub fn build(self) -> Result<Dimension, EngineError>;
}
```

`is_frozen` flips to `true` when the dimension is bound to its first cube. After
freeze, only `add_element`-style appends are allowed (Phase 2+). Phase 1 forbids
all post-freeze mutation.

### 3.6 `hierarchy.rs`

```rust
#[derive(Clone, Debug)]
pub struct Hierarchy {
    pub id: HierarchyId,
    pub name: String,
    pub dimension: DimensionId,
    pub edges: Vec<HierarchyEdge>,
    pub roots: Vec<ElementId>,
    pub leaves: ahash::AHashSet<ElementId>,
    pub consolidated: ahash::AHashSet<ElementId>,
    /// parent → children index for fast walk
    pub children_of: ahash::AHashMap<ElementId, Vec<HierarchyEdge>>,
    /// child → parent index for invalidation
    pub parent_of: ahash::AHashMap<ElementId, ElementId>,
}

#[derive(Clone, Debug)]
pub struct HierarchyEdge {
    pub parent: ElementId,
    pub child: ElementId,
    pub weight: f64,
}

impl Hierarchy {
    pub fn builder(id: HierarchyId, name: impl Into<String>, dim: DimensionId) -> HierarchyBuilder;
    pub fn descendants(&self, root: ElementId) -> Vec<(ElementId, f64)>;
    pub fn is_leaf(&self, e: ElementId) -> bool;
    pub fn is_consolidated(&self, e: ElementId) -> bool;
    pub fn ancestors(&self, leaf: ElementId) -> Vec<(ElementId, f64)>;
}

pub struct HierarchyBuilder { ... }

impl HierarchyBuilder {
    pub fn add_edge(self, parent: ElementId, child: ElementId, weight: f64) -> Self;
    pub fn build(self) -> Result<Hierarchy, EngineError>;
}
```

The builder must:

1. Reject NaN/Inf weights (`EngineError::InvalidWeight`).
2. Reject duplicate edges (same parent+child).
3. Reject any element that has more than one parent (single-parent forest only in Phase 1).
4. Detect cycles (`EngineError::HierarchyCycle { path: Vec<ElementId> }`).
5. Compute `roots` (elements never appearing as a child).
6. Compute `leaves` (elements never appearing as a parent).
7. Build `children_of` and `parent_of` indexes.

`descendants(root)` returns every leaf reachable from `root` with the cumulative
weight product. For Phase 1's all-`weight=1.0` Acme demo, every cumulative weight
is `1.0`.

### 3.7 `coordinate.rs`

```rust
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CellCoordinate {
    pub cube: CubeId,
    elements: smallvec::SmallVec<[ElementId; 8]>,
}

impl CellCoordinate {
    pub fn elements(&self) -> &[ElementId];
    pub fn element_at(&self, dim_position: usize) -> ElementId;
    pub fn with_element(&self, dim_position: usize, e: ElementId) -> CellCoordinate;
}

pub struct CellCoordinateBuilder<'cube> {
    cube: &'cube Cube,
    slots: Vec<Option<ElementId>>,
}

impl<'cube> CellCoordinateBuilder<'cube> {
    pub fn new(cube: &'cube Cube) -> Self;
    pub fn set(self, dim: DimensionId, element: ElementId) -> Result<Self, EngineError>;
    pub fn set_by_name(self, dim_name: &str, element_name: &str) -> Result<Self, EngineError>;
    pub fn build(self) -> Result<CellCoordinate, EngineError>;
}
```

Equality and hashing depend only on `cube` and the element slice. The slice is
stored in the cube's dimension order, so two builders setting the same elements
produce the same coordinate regardless of `set` call order.

### 3.8 `cell.rs`

```rust
#[derive(Clone, Debug)]
pub struct CellValue {
    pub value: ScalarValue,
    pub dtype: CellDataType,
    pub provenance: Provenance,
    pub uncertainty: Option<Uncertainty>,
    pub trace: Option<crate::trace::Trace>,
    pub revision: Revision,
}

#[derive(Clone, Debug)]
pub enum Provenance {
    Input { written_at: u64, written_by: PrincipalId },
    Rule { rule_id: RuleId, computed_at: Revision },
    /// A single consolidated cell may aggregate across MULTIPLE hierarchies
    /// simultaneously (e.g., Q1 × Paid_Media × Florida walks the Time, Channel,
    /// and Market hierarchies at once). The list captures every hierarchy the
    /// consolidation walked. The `child_count` is the number of leaf coords
    /// that contributed to the value.
    Consolidation {
        hierarchies: smallvec::SmallVec<[HierarchyId; 4]>,
        child_count: u32,
    },
    Default { reason: &'static str },
}

#[derive(Clone, Debug)]
pub enum Uncertainty {
    StdDev(f64),
    Interval { low: f64, high: f64, confidence: f64 },
}
```

`written_at` is a `u64` Unix-seconds timestamp. Phase 1 never produces
`Uncertainty` from any built-in path — every `CellValue` returned by Phase 1 has
`uncertainty: None`. The field exists for forward compat and is allowed in
user-supplied input writes (the kernel passes it through but does no math with it).

### 3.9 `store.rs`

Phase 1 uses **a concrete `HashMapStore` directly** — there is no `CellStore`
trait, no trait object, and no pluggable backend. The trait is a Phase 2 concern
once we want to swap in Arrow/LSM/Roaring storage. Defining it now would force
us to design `clone_box`, `Debug`-on-trait-object, and `Clone`-on-`Box<dyn Trait>`
plumbing for a v1 that only has one impl.

```rust
#[derive(Clone, Debug)]
pub struct StoredCell {
    pub value: ScalarValue,
    pub provenance: Provenance,
    pub uncertainty: Option<Uncertainty>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Default)]
pub struct HashMapStore {
    cells: ahash::AHashMap<CellCoordinate, StoredCell>,
}

impl HashMapStore {
    pub fn new() -> Self;
    pub fn read(&self, coord: &CellCoordinate) -> Option<&StoredCell>;
    pub fn write(&mut self, coord: CellCoordinate, cell: StoredCell);
    pub fn remove(&mut self, coord: &CellCoordinate) -> Option<StoredCell>;
    pub fn iter(&self) -> impl Iterator<Item = (&CellCoordinate, &StoredCell)>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

`HashMapStore: Clone` is derived. `Cube` and `Snapshot` both hold a
`HashMapStore` by value. Snapshot's `Clone` of the cube state is just
`store.clone()`.

The `CellStore` trait will be introduced in Phase 2 alongside the second
backend. Phase 1's API is intentionally non-trait so that adding the trait
later is an additive change rather than a behavioral one.

### 3.10 `rule.rs`

```rust
#[derive(Debug)]
pub struct RuleSet {
    rules: Vec<Rule>,
    by_target: ahash::AHashMap<ElementId, Vec<RuleId>>,  // target measure → rules
}

impl RuleSet {
    pub fn new() -> Self;
    pub fn add(&mut self, rule: Rule) -> Result<(), EngineError>;
    pub fn rule(&self, id: RuleId) -> Option<&Rule>;
    pub fn rules_for_measure(&self, measure: ElementId) -> &[RuleId];
    pub fn iter(&self) -> impl Iterator<Item = &Rule>;
}

#[derive(Debug)]
pub struct Rule {
    pub id: RuleId,
    pub cube: CubeId,
    pub target_measure: ElementId,
    pub scope: Scope,
    pub body: Expr,
    pub declared_dependencies: Vec<DependencyDecl>,
}

#[derive(Clone, Debug)]
pub enum Scope {
    /// Rule applies to every leaf coordinate in non-measure dimensions
    /// where the measure is `target_measure`. Phase 1 supports this only.
    AllLeaves,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Const(ScalarValue),
    SelfRef(ElementId),               // same coord, different measure (this dim's element)
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    IfNull(Box<Expr>, Box<Expr>),     // (primary, fallback)
}

#[derive(Clone, Debug)]
pub struct DependencyDecl {
    pub measure: ElementId,
    pub coord_pattern: CoordPattern,
}

#[derive(Clone, Debug)]
pub enum CoordPattern {
    SameAsTarget,                     // Phase 1 supports this only
}
```

Phase 1 supports exactly: `Const`, `SelfRef`, `Add`, `Sub`, `Mul`, `Div`,
`IfNull`. `SumOver`, `IfElse`, cross-cube refs, fixed-coordinate refs are all
Phase 2+.

`Scope::AllLeaves` is the only scope. It means: the rule applies to every
coordinate where the measure dim's element is `target_measure` and every other
dim's element is a leaf (in the default hierarchy).

`CoordPattern::SameAsTarget` is the only pattern. It means: "this dependency is
read at the same coordinate as the rule target, just with a different measure."

This matches every Acme rule:

```rust
// Clicks = Spend / CPC
Rule {
    target_measure: clicks_id,
    scope: Scope::AllLeaves,
    body: Expr::Div(
        Box::new(Expr::SelfRef(spend_id)),
        Box::new(Expr::SelfRef(cpc_id)),
    ),
    declared_dependencies: vec![
        DependencyDecl { measure: spend_id, coord_pattern: CoordPattern::SameAsTarget },
        DependencyDecl { measure: cpc_id,   coord_pattern: CoordPattern::SameAsTarget },
    ],
    ...
}
```

`RuleSet::add` validates:

1. `target_measure` is a `MeasureRole::Derived` measure in some dimension.
2. Every `SelfRef` in `body` references a measure that exists (in the same dim).
3. `body` is well-typed (every node returns `F64` for Phase 1; mixed types are rejected).
4. `declared_dependencies` is a complete superset of the measures actually
   referenced by `body`. (Walk the AST, collect `SelfRef` measures, compare.)
5. Adding this rule does not create a cycle in the rule-target → dep-measure graph.
6. No other rule has the same `target_measure` and overlapping scope. Since Phase
   1's only scope is `AllLeaves`, this means no two rules share a target measure.

### 3.11 `trace.rs`

```rust
#[derive(Clone, Debug)]
pub struct Trace {
    pub root: TraceNode,
    pub revision: Revision,
    pub elapsed_us: u64,
}

#[derive(Clone, Debug)]
pub struct TraceNode {
    pub coord: CellCoordinate,
    pub value: ScalarValue,
    pub operation: TraceOp,
    pub children: Vec<TraceNode>,
}

#[derive(Clone, Debug)]
pub enum TraceOp {
    InputLookup { written_at: u64, written_by: PrincipalId },
    RuleEvaluation { rule_id: RuleId, expr_summary: ExprSummary },
    /// Multi-hierarchy aware. Same shape as Provenance::Consolidation.
    Consolidation {
        hierarchies: smallvec::SmallVec<[HierarchyId; 4]>,
        child_count: u32,
    },
    DefaultFallback { default: ScalarValue, reason: &'static str },
    NullPoison { upstream: CellCoordinate },
}

#[derive(Clone, Copy, Debug)]
pub struct ExprSummary {
    pub op: ExprOp,
    pub arity: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum ExprOp { Const, SelfRef, Add, Sub, Mul, Div, IfNull }
```

Trace generation is opt-in. The cube's `read` API takes a flag; `read_with_trace`
produces a Trace, plain `read` does not.

### 3.12 `dependency.rs`

```rust
#[derive(Debug)]
pub struct DependencyGraph {
    forward: ahash::AHashMap<CellCoordinate, Vec<DependencyEdge>>,  // cell → cells it reads
    reverse: ahash::AHashMap<CellCoordinate, Vec<CellCoordinate>>,   // cell → cells reading it
}

#[derive(Clone, Debug)]
pub struct DependencyEdge {
    pub to: CellCoordinate,
    pub via: DependencySource,
}

#[derive(Clone, Debug)]
pub enum DependencySource {
    Rule(RuleId),
    Hierarchy(HierarchyId),
}

impl DependencyGraph {
    pub fn new() -> Self;
    pub fn add_edge(&mut self, from: CellCoordinate, edge: DependencyEdge);
    pub fn dependents_of(&self, coord: &CellCoordinate) -> &[CellCoordinate];
    pub fn dependencies_of(&self, coord: &CellCoordinate) -> &[DependencyEdge];
    pub fn closure_of_dependents(&self, root: &CellCoordinate) -> ahash::AHashSet<CellCoordinate>;
    pub fn detect_cycle(&self) -> Option<Vec<CellCoordinate>>;
}
```

Phase 1's dependency graph is built **on-demand**: when a rule first evaluates at
a coordinate, the engine materializes the edges. Pre-computing the entire graph
would require enumerating all leaf coordinates × all rules, which for Acme is
~10K coordinates × 5 rules = ~50K edges. That's fine, but for larger cubes it's
unacceptable. v1 builds edges lazily and tests rely on the lazy behavior matching
eager behavior.

`closure_of_dependents` is the closure used for dirty propagation.

`detect_cycle` runs at every `add_edge` call; if a new edge would create a cycle,
the call returns the cycle path and the edge is not added. The caller (rule
registration) escalates this to `EngineError::DependencyCycle`.

### 3.13 `dirty.rs`

```rust
pub struct DirtyTracker {
    set: ahash::AHashSet<CellCoordinate>,
}

impl DirtyTracker {
    pub fn new() -> Self;
    pub fn mark(&mut self, coord: CellCoordinate);
    pub fn mark_closure(&mut self, root: &CellCoordinate, graph: &DependencyGraph);
    pub fn is_dirty(&self, coord: &CellCoordinate) -> bool;
    pub fn clear(&mut self, coord: &CellCoordinate);
    pub fn len(&self) -> usize;
    pub fn iter(&self) -> impl Iterator<Item = &CellCoordinate>;
}
```

### 3.14 `permission.rs` (minimal)

Phase 1 is single-user from the engine's perspective, but the permission types
must exist so writes can carry a `principal: PrincipalId` and the writeback API
shape is stable for Phase 2.

```rust
#[derive(Debug)]
pub struct PermissionTable {
    cube: CubeId,
    grants: Vec<Grant>,
    /// Phase 1: the cube's "root" principal who has full access. Created at cube init.
    root_principal: PrincipalId,
}

#[derive(Clone, Debug)]
pub struct Grant {
    pub principal: PrincipalId,
    pub pattern: ScopePattern,
    pub capabilities: CapabilitySet,
}

#[derive(Clone, Debug)]
pub struct ScopePattern {
    pub bindings: ahash::AHashMap<DimensionId, ScopeBinding>,
}

#[derive(Clone, Debug)]
pub enum ScopeBinding {
    One(ElementId),
    Many(Vec<ElementId>),
    Subtree(ElementId),
    All,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CapabilitySet(pub u32);

pub mod capability {
    pub const READ: u32 = 1 << 0;
    pub const WRITE: u32 = 1 << 1;
    pub const APPROVE: u32 = 1 << 2;
    pub const LOCK: u32 = 1 << 3;
    pub const UNLOCK: u32 = 1 << 4;
    pub const ADMIN: u32 = 1 << 5;
}

impl PermissionTable {
    pub fn new(cube: CubeId, root: PrincipalId) -> Self;
    pub fn grant(&mut self, grant: Grant);
    pub fn check(
        &self,
        principal: PrincipalId,
        cube: &Cube,
        coord: &CellCoordinate,
        cap: u32,
    ) -> bool;
}
```

Phase 1's `check` returns `true` for the `root_principal` and otherwise scans the
grants list. The table is small (≤ 16 grants) so linear scan is fine.

The Acme demo uses the root principal exclusively. Permission *enforcement*
exists but the demo never exercises it with non-root principals. The tests in §10
exercise it.

### 3.15 `lock.rs` (minimal)

```rust
#[derive(Debug)]
pub struct LockTable {
    cube: CubeId,
    locks: Vec<Lock>,
}

#[derive(Clone, Debug)]
pub struct Lock {
    pub id: LockId,
    pub owner: PrincipalId,
    pub pattern: ScopePattern,
    pub kind: LockKind,
    pub acquired_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug)]
pub enum LockKind {
    Soft,
    Hard,
}

impl LockTable {
    pub fn new(cube: CubeId) -> Self;
    pub fn acquire(&mut self, lock: Lock) -> Result<LockId, EngineError>;
    pub fn release(&mut self, lock_id: LockId, principal: PrincipalId) -> Result<(), EngineError>;
    /// Returns the conflicting lock if a write is blocked, None if allowed.
    pub fn check_write(
        &self,
        principal: PrincipalId,
        cube: &Cube,
        coord: &CellCoordinate,
        now: u64,
    ) -> Option<&Lock>;
    pub fn purge_expired(&mut self, now: u64);
}
```

### 3.16 `snapshot.rs` (minimal)

Phase 1 supports in-memory snapshots only.

```rust
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub cube: CubeId,
    pub revision: Revision,
    pub captured_at: u64,
    pub label: Option<String>,
    /// Phase 1: full clone of the cube's store. Acme is small enough.
    /// Phase 3 replaces this with copy-on-write or version-vector storage.
    pub(crate) store: HashMapStore,
}

impl Snapshot {
    pub fn read(&self, coord: &CellCoordinate) -> Option<&StoredCell>;
}
```

Phase 1 snapshots are cheap-to-take only because the cube is small. Real
copy-on-write or version-vector snapshots are Phase 3.

The cube's `snapshot()` method returns a `Snapshot`. The cube's `rollback_to()`
method (Phase 1) replaces the live store's contents with the snapshot's clone.
This is destructive; it's tested in §10 but not part of the Acme demo flow.

### 3.17 `consolidation.rs`

```rust
pub struct Consolidator;

impl Consolidator {
    pub fn read(
        cube: &Cube,
        coord: &CellCoordinate,
        request_trace: bool,
    ) -> Result<CellValue, EngineError>;
}
```

The implementation:

1. For each dim in the coord, determine if its element is consolidated.
2. If no dim is consolidated, this is a leaf coord — delegate to the rule
   evaluator (or input lookup).
3. Otherwise, expand each consolidated dim to its leaf descendants (with weight
   products from the hierarchy).
4. Cartesian-product across all consolidated dims. Each combination is a leaf
   coordinate.
5. For each leaf coord, recursively read its value (input or rule).
6. Combine according to the measure's `AggregationRule`:
   - `Sum`: sum of (leaf_value × weight_product). Null counts as 0.
   - `WeightedAverage { weight_measure }`: read both this measure and the weight
     measure for each leaf; compute `Σ(v_i × w_i) / Σ(w_i)`. Null v contributes
     nothing; Null w contributes nothing.
   - `Min` / `Max`: ignore Nulls; if all Null, result is Null.
7. Return a `CellValue` with `Provenance::Consolidation` and the appropriate
   `child_count`.

The Acme demo exercises:
- `Sum` for Spend, Clicks, Leads, Customers, Revenue, Gross_Profit at all rollup
  levels.
- `WeightedAverage` for CPC (weighted by Spend), CVR (weighted by Clicks),
  Close_Rate (weighted by Leads), AOV (weighted by Customers), COGS_Rate
  (weighted by Revenue) — at rollup levels only; at leaves, the input value is
  returned directly.

### 3.18 `cube.rs`

```rust
#[derive(Debug)]
pub struct Cube {
    pub id: CubeId,
    pub name: String,
    dimensions: Vec<Dimension>,
    measure_dimension_position: usize,
    rules: RuleSet,
    locks: LockTable,
    permissions: PermissionTable,
    store: HashMapStore,
    revision: Revision,
    deps: DependencyGraph,
    dirty: DirtyTracker,
}

impl Cube {
    pub fn builder(id: CubeId, name: impl Into<String>) -> CubeBuilder;

    pub fn read(&mut self, coord: &CellCoordinate, principal: PrincipalId) -> Result<CellValue, EngineError>;
    pub fn read_with_trace(&mut self, coord: &CellCoordinate, principal: PrincipalId) -> Result<CellValue, EngineError>;
    pub fn slice(&mut self, query: &SliceQuery, principal: PrincipalId) -> Result<SliceResult, EngineError>;

    pub fn write(&mut self, req: WritebackRequest) -> Result<WritebackResult, EngineError>;

    pub fn snapshot(&self, label: Option<&str>) -> Snapshot;
    pub fn rollback_to(&mut self, snap: &Snapshot) -> Result<Revision, EngineError>;

    pub fn revision(&self) -> Revision;
    pub fn dimension(&self, id: DimensionId) -> Option<&Dimension>;
    pub fn dimension_by_name(&self, name: &str) -> Option<&Dimension>;
    pub fn measure_dimension(&self) -> &Dimension;
}

pub struct CubeBuilder { ... }

impl CubeBuilder {
    pub fn add_dimension(self, dim: Dimension) -> Self;
    pub fn measure_dimension(self, name: &str) -> Self;
    pub fn add_rule(self, rule: Rule) -> Result<Self, EngineError>;
    pub fn root_principal(self, p: PrincipalId) -> Self;
    pub fn build(self) -> Result<Cube, EngineError>;
}

// ----- writeback request/result/intent (referenced by §6 and §10) -----

#[derive(Clone, Debug)]
pub struct WritebackRequest {
    pub coord: CellCoordinate,
    pub new_value: ScalarValue,
    pub principal: PrincipalId,
    pub intent: WriteIntent,
    /// Optional optimistic-concurrency check. If `Some(r)` and the cube has
    /// advanced past `r`, the write is rejected with `StaleRevision`.
    pub expected_revision: Option<Revision>,
}

#[derive(Clone, Copy, Debug)]
pub enum WriteIntent {
    /// Replace the existing value with `new_value`.
    Set,
    /// Add `new_value` (numeric only) to the existing value. Null + x = x.
    /// Type mismatches are rejected.
    Increment,
    /// Set the cell to `ScalarValue::Null` regardless of `new_value`.
    Clear,
}

#[derive(Clone, Debug)]
pub struct WritebackResult {
    pub coord: CellCoordinate,
    pub old_value: Option<CellValue>,
    pub new_value: CellValue,
    pub revision_before: Revision,
    pub revision_after: Revision,
    /// Coordinates marked dirty by this write — both rule dependents and
    /// hierarchy ancestors. Order is unspecified; equality is by set content.
    pub invalidated: Vec<CellCoordinate>,
    /// Soft-lock advisories: if a `Soft` lock covered this coord, the lock's
    /// `note` (if any) is surfaced here. Phase 1 emits at most one entry.
    pub soft_lock_notes: Vec<String>,
}
```

`read`'s algorithm:

1. Permission check: `permissions.check(principal, self, coord, capability::READ)`. If false, return `EngineError::InsufficientPermission`.
2. If coord references any consolidated element, delegate to `Consolidator::read`.
3. Else if coord's measure is `Input`:
   - Look up `store.read(coord)`. If present, return it.
   - If absent, return `CellValue` with `ScalarValue::Null` and
     `Provenance::Default { reason: "no input written" }`.
4. Else (measure is `Derived`):
   - If `dirty.is_dirty(coord)` or no cached value: re-evaluate the rule.
     - Find the rule for this measure (`rules.rules_for_measure`).
     - Walk `body`, recursively reading each `SelfRef`'s coordinate.
     - Capture the actual reads as a `Vec<CellCoordinate>` and compare to
       `declared_dependencies` (after expanding `SameAsTarget`). If a real read
       isn't in declared, return `EngineError::UndeclaredDependency`.
     - Update `deps` with the edges.
     - Compute the value.
     - Store in `store` with `Provenance::Rule`.
     - `dirty.clear(coord)`.
     - Return the `CellValue`.
   - Else: return the cached value.

`write`'s algorithm:

1. Permission check (`capability::WRITE`).
2. Lock check (`locks.check_write`).
3. If coord references any consolidated element, return `WritebackError::ConsolidatedCellNotWritable`.
4. Look up the measure for this coord. If `MeasureRole::Derived`, return `WritebackError::DerivedCellNotWritable`.
5. Type check: ensure `req.new_value`'s dtype matches the measure's dtype.
6. NaN check: reject NaN.
7. If `req.expected_revision` is set and doesn't equal current revision, return `StaleRevision`.
8. Bump revision.
9. Write the value into the store with `Provenance::Input`.
10. Compute the dirty closure: `deps.closure_of_dependents(coord)` plus all
    consolidated ancestors via the hierarchies.
11. Mark each dirty.
12. Return `WritebackResult` with the invalidated set.

`rollback_to`:

1. Validate that `snap.cube == self.id`. Else return `SnapshotCubeMismatch`.
2. Replace `self.store` with `snap.store.clone()`.
3. Bump revision.
4. Clear `dirty` (snapshot's stored values are by definition fresh).
5. Clear all cached derived-cell entries that came from rule evaluation. The
   easiest correct implementation is to drop every cell whose `provenance` is
   `Rule { .. }` from the store after the snapshot copy — they will be lazily
   recomputed on next read. (Snapshots only need to preserve `Input` and
   `Consolidation` entries, but it's safer to keep everything and let the next
   read decide; both behaviors are correct as long as the read returns the
   right value.)
6. Note: `deps` is **not** rolled back in Phase 1 — rules are immutable per
   cube so the dep graph at any revision is identical. Phase 3+ may track
   per-revision rule changes.

### 3.19 `slice.rs`

```rust
#[derive(Clone, Debug)]
pub struct SliceQuery {
    pub cube: CubeId,
    pub bindings: ahash::AHashMap<DimensionId, SliceBinding>,
    pub request_trace: bool,
}

#[derive(Clone, Debug)]
pub enum SliceBinding {
    One(ElementId),
    Many(Vec<ElementId>),
    Subtree(ElementId),
    All,
    AllConsolidated,
}

#[derive(Clone, Debug)]
pub struct SliceResult {
    pub coords: Vec<CellCoordinate>,
    pub values: Vec<CellValue>,
    pub revision: Revision,
}
```

Phase 1 `cube.slice` enforces `coords.len() <= 1_048_576` (1M cells). Larger
slices return `EngineError::SliceTooLarge`.

### 3.20 `error.rs`

```rust
#[derive(thiserror::Error, Debug)]
pub enum EngineError {
    #[error("dimension '{name}' not found")]
    DimensionNotFound { name: String },

    #[error("element id {0:?} not found in dimension {1:?}")]
    ElementNotFound(ElementId, DimensionId),

    #[error("dimension already frozen")]
    DimensionFrozen,

    #[error("hierarchy cycle: {path:?}")]
    HierarchyCycle { path: Vec<ElementId> },

    #[error("invalid hierarchy weight: {0}")]
    InvalidWeight(f64),

    #[error("element has multiple parents in single-parent hierarchy")]
    MultipleParents { element: ElementId, existing: ElementId, attempted: ElementId },

    #[error("coordinate slot for dimension {0:?} is unset")]
    CoordinateMissingDimension(DimensionId),

    #[error("dependency cycle detected")]
    DependencyCycle { path: Vec<CellCoordinate> },

    #[error("undeclared dependency: rule {rule:?} read {coord:?} but did not declare it")]
    UndeclaredDependency { rule: RuleId, coord: CellCoordinate },

    #[error("rule's target measure must be Derived; got {role:?}")]
    RuleTargetNotDerived { role: MeasureRole },

    #[error("rule body is not well-typed")]
    RuleBodyTypeMismatch { detail: String },

    #[error("two rules target the same measure: {0:?}")]
    DuplicateRuleTarget(ElementId),

    #[error("slice exceeds size limit: {actual} > {max}")]
    SliceTooLarge { actual: usize, max: usize },

    #[error("insufficient permission for principal {principal:?} on coord {coord:?}")]
    InsufficientPermission { principal: PrincipalId, coord: CellCoordinate },

    #[error("locked: cell {coord:?} held by principal {owner:?}")]
    LockedCell { coord: CellCoordinate, owner: PrincipalId },

    /// A write was attempted to a coordinate whose Version-dimension element
    /// is in `Approved` or `Archived` state. Per spec §9 I-Ver-3.
    #[error("locked version: write to {state:?} version {version:?} rejected")]
    LockedVersion { version: ElementId, state: VersionState },

    #[error("write rejected (derived cell): {coord:?}")]
    DerivedCellNotWritable { coord: CellCoordinate },

    #[error("write rejected (consolidated cell): {coord:?}")]
    ConsolidatedCellNotWritable { coord: CellCoordinate },

    #[error("stale revision: expected {expected:?}, current {current:?}")]
    StaleRevision { expected: Revision, current: Revision },

    #[error("type mismatch: expected {expected:?}, got value {got:?}")]
    TypeMismatch { expected: CellDataType, got: ScalarValue },

    #[error("invalid value: {0}")]
    InvalidValue(&'static str),

    #[error("snapshot mismatch")]
    SnapshotCubeMismatch,

    #[error("dimension {dim:?} has no default hierarchy")]
    NoDefaultHierarchy { dim: DimensionId },

    #[error("internal invariant violated: {0}")]
    Internal(&'static str),
}
```

`WritebackError` is a strict subset of `EngineError`; the writeback API returns
`Result<WritebackResult, EngineError>` directly.

---

## 4. The Acme demo fixture

`mc-fixtures` exposes one function:

```rust
pub fn build_acme_cube() -> Result<(mc_core::Cube, AcmeRefs), mc_core::EngineError>;

pub struct AcmeRefs {
    pub root_principal: PrincipalId,

    // Dimensions
    pub scenario_dim: DimensionId,
    pub version_dim: DimensionId,
    pub time_dim: DimensionId,
    pub channel_dim: DimensionId,
    pub market_dim: DimensionId,
    pub measure_dim: DimensionId,

    // Hierarchy IDs (default hierarchies only)
    pub time_hierarchy: HierarchyId,
    pub channel_hierarchy: HierarchyId,
    pub market_hierarchy: HierarchyId,

    // Scenario elements
    pub scen_baseline: ElementId,
    pub scen_aggressive: ElementId,
    pub scen_conservative: ElementId,

    // Version elements
    pub ver_working: ElementId,
    pub ver_submitted: ElementId,
    pub ver_approved: ElementId,

    // Time elements (leaves: 12 months for FY2026)
    pub jan_2026: ElementId, pub feb_2026: ElementId, pub mar_2026: ElementId,
    pub apr_2026: ElementId, pub may_2026: ElementId, pub jun_2026: ElementId,
    pub jul_2026: ElementId, pub aug_2026: ElementId, pub sep_2026: ElementId,
    pub oct_2026: ElementId, pub nov_2026: ElementId, pub dec_2026: ElementId,
    // Time consolidations
    pub q1_2026: ElementId, pub q2_2026: ElementId, pub q3_2026: ElementId, pub q4_2026: ElementId,
    pub fy_2026: ElementId,

    // Channel elements (leaves)
    pub paid_search: ElementId, pub paid_social: ElementId, pub display: ElementId,
    pub email: ElementId, pub organic: ElementId,
    // Channel consolidations
    pub paid_media: ElementId, pub owned_earned: ElementId, pub all_channels: ElementId,

    // Market elements (leaves)
    pub tampa: ElementId, pub orlando: ElementId, pub miami: ElementId,
    pub atlanta: ElementId, pub charlotte: ElementId,
    pub new_york_city: ElementId, pub boston: ElementId,
    // Market consolidations
    pub florida: ElementId, pub georgia: ElementId, pub north_carolina: ElementId,
    pub new_york_state: ElementId, pub massachusetts: ElementId,
    pub southeast: ElementId, pub northeast: ElementId, pub usa: ElementId,

    // Measure elements
    // Inputs
    pub spend: ElementId, pub cpc: ElementId, pub cvr: ElementId,
    pub close_rate: ElementId, pub aov: ElementId, pub cogs_rate: ElementId,
    // Derived
    pub clicks: ElementId, pub leads: ElementId, pub customers: ElementId,
    pub revenue: ElementId, pub gross_profit: ElementId,

    // Rule IDs (so tests can refer to them)
    pub rule_clicks: RuleId, pub rule_leads: RuleId, pub rule_customers: RuleId,
    pub rule_revenue: RuleId, pub rule_gross_profit: RuleId,
}
```

### 4.1 Dimensions

| Name | Kind | Element count |
|---|---|---|
| Scenario | Scenario | 3 (Baseline, Aggressive, Conservative; default = Baseline) |
| Version | Version | 3 (Working = Draft, Submitted = Submitted, Approved = Approved) |
| Time | Standard | 17 elements: 12 leaves (months) + 4 quarters + 1 year |
| Channel | Standard | 8 elements: 5 leaves + Paid_Media + Owned_Earned + All_Channels |
| Market | Standard | 15 elements: 7 leaves + 5 states + 2 regions + USA |
| Measure | Measure | 11 elements: 6 inputs + 5 derived |

### 4.2 Hierarchies

#### Time (Calendar)

```
FY_2026
├── Q1_2026
│   ├── Jan_2026 (w=1.0)
│   ├── Feb_2026 (w=1.0)
│   └── Mar_2026 (w=1.0)
├── Q2_2026
│   ├── Apr_2026 (w=1.0)
│   ├── May_2026 (w=1.0)
│   └── Jun_2026 (w=1.0)
├── Q3_2026
│   ├── Jul_2026 (w=1.0)
│   ├── Aug_2026 (w=1.0)
│   └── Sep_2026 (w=1.0)
└── Q4_2026
    ├── Oct_2026 (w=1.0)
    ├── Nov_2026 (w=1.0)
    └── Dec_2026 (w=1.0)
```

#### Channel (Grouping)

```
All_Channels
├── Paid_Media
│   ├── Paid_Search (w=1.0)
│   ├── Paid_Social (w=1.0)
│   └── Display (w=1.0)
└── Owned_Earned
    ├── Email (w=1.0)
    └── Organic (w=1.0)
```

#### Market (Geographic)

```
USA
├── Southeast
│   ├── Florida
│   │   ├── Tampa (w=1.0)
│   │   ├── Orlando (w=1.0)
│   │   └── Miami (w=1.0)
│   ├── Georgia
│   │   └── Atlanta (w=1.0)
│   └── North_Carolina
│       └── Charlotte (w=1.0)
└── Northeast
    ├── New_York_State
    │   └── New_York_City (w=1.0)
    └── Massachusetts
        └── Boston (w=1.0)
```

### 4.3 Measures (verbatim aggregation rules)

| Name | Role | dtype | Aggregation |
|---|---|---|---|
| Spend | Input | F64 | Sum |
| CPC | Input | F64 | WeightedAverage(weight=Spend) |
| CVR | Input | F64 | WeightedAverage(weight=Clicks) |
| Close_Rate | Input | F64 | WeightedAverage(weight=Leads) |
| AOV | Input | F64 | WeightedAverage(weight=Customers) |
| COGS_Rate | Input | F64 | WeightedAverage(weight=Revenue) |
| Clicks | Derived | F64 | Sum |
| Leads | Derived | F64 | Sum |
| Customers | Derived | F64 | Sum |
| Revenue | Derived | F64 | Sum |
| Gross_Profit | Derived | F64 | Sum |

### 4.4 Rules

```
Clicks       = Spend / CPC
Leads        = Clicks * CVR
Customers    = Leads * Close_Rate
Revenue      = Customers * AOV
Gross_Profit = Revenue * (1 - COGS_Rate)
```

In `Expr` form:

```rust
// Clicks = Spend / CPC
Expr::Div(Box::new(Expr::SelfRef(spend)), Box::new(Expr::SelfRef(cpc)))

// Leads = Clicks * CVR
Expr::Mul(Box::new(Expr::SelfRef(clicks)), Box::new(Expr::SelfRef(cvr)))

// Customers = Leads * Close_Rate
Expr::Mul(Box::new(Expr::SelfRef(leads)), Box::new(Expr::SelfRef(close_rate)))

// Revenue = Customers * AOV
Expr::Mul(Box::new(Expr::SelfRef(customers)), Box::new(Expr::SelfRef(aov)))

// Gross_Profit = Revenue * (1 - COGS_Rate)
Expr::Mul(
    Box::new(Expr::SelfRef(revenue)),
    Box::new(Expr::Sub(
        Box::new(Expr::Const(ScalarValue::F64(1.0))),
        Box::new(Expr::SelfRef(cogs_rate)),
    )),
)
```

### 4.5 Canonical input data

`mc-fixtures::inputs::write_canonical_inputs(cube, refs)` writes a deterministic
set of values to enable test assertions. Every leaf coordinate combining
{Baseline} × {Working} × {12 months} × {5 channels} × {7 cities} × {6 input
measures} = 12 × 5 × 7 × 6 = **2,520 input cells**. The data follows a
deterministic formula:

```
seed = hash(scenario, version, time_idx, channel_idx, market_idx, measure_idx)

For each coord:
  spend       = 10_000 + 500 * time_idx + 1_000 * channel_idx + 200 * market_idx
  cpc         = 1.50 + 0.05 * channel_idx + 0.02 * market_idx       (always > 0)
  cvr         = 0.020 + 0.005 * channel_idx                          (in [0.02, 0.05])
  close_rate  = 0.10 + 0.01 * channel_idx                            (in [0.10, 0.15])
  aov         = 200.0 + 50.0 * market_idx                            (in [200, 600])
  cogs_rate   = 0.30 + 0.02 * channel_idx                            (in [0.30, 0.40])
```

Where `time_idx` is 1..=12, `channel_idx` is 0..=4 (paid_search=0..organic=4),
`market_idx` is 0..=6 (tampa=0..boston=6).

This produces predictable, hand-computable golden values for the assertion tests
in §10. Aggressive and Conservative scenarios use the same inputs scaled by
×1.20 and ×0.85 respectively (only Spend is scaled; ratios are identical).

The fixture writes only `Baseline × Working` to keep the cube small. The other
scenarios/versions are present in the dimension but have no data; reads of them
return `Null` (with `Provenance::Default`).

#### 4.5.1 Hand-calculated golden values

Every test assertion in §10 traces back to one of these numbers. The CLI demo
in §4.6 prints these exact values. The `golden_inputs()` helper in
`mc-fixtures` returns the input rows; the derived rows below are computed by
the rules in §4.4 against those inputs, with **no rounding** until the final
display step. Internal arithmetic uses `f64`; comparisons in tests use
`abs(actual - expected) < 1e-6`.

**Element index assignments (consistent throughout):**

- `time_idx`: Jan=1, Feb=2, Mar=3, Apr=4, May=5, Jun=6, Jul=7, Aug=8, Sep=9, Oct=10, Nov=11, Dec=12
- `channel_idx`: Paid_Search=0, Paid_Social=1, Display=2, Email=3, Organic=4
- `market_idx`: Tampa=0, Orlando=1, Miami=2, Atlanta=3, Charlotte=4, New_York_City=5, Boston=6

**Anchor cell — Baseline × Working × Mar_2026 × Paid_Search × Tampa**
(time_idx=3, channel_idx=0, market_idx=0):

| Measure | Computation | Exact value |
|---|---|---|
| Spend | 10000 + 500·3 + 1000·0 + 200·0 | **11,500** |
| CPC | 1.50 + 0.05·0 + 0.02·0 | **1.50** |
| CVR | 0.020 + 0.005·0 | **0.020** |
| Close_Rate | 0.10 + 0.01·0 | **0.10** |
| AOV | 200.0 + 50.0·0 | **200.0** |
| COGS_Rate | 0.30 + 0.02·0 | **0.30** |
| Clicks | 11,500 / 1.50 | **7,666.6̄** (≈ 7,666.6667) |
| Leads | 7,666.6̄ × 0.020 | **153.3̄** (≈ 153.3333) |
| Customers | 153.3̄ × 0.10 | **15.3̄** (≈ 15.3333) |
| Revenue | 15.3̄ × 200 | **3,066.6̄** (≈ 3,066.6667) |
| Gross_Profit | 3,066.6̄ × (1 − 0.30) | **2,146.6̄** (≈ 2,146.6667) |

The `6̄` notation means "repeating 6"; tests compare `f64` with the tolerance
above so the trailing digits aren't load-bearing. (Specifically:
Clicks = 23000/3, Leads = 460/3, Customers = 46/3, Revenue = 9200/3,
Gross_Profit = 6440/3.)

**After writing Spend = 50,000 to the same coordinate** (the canonical CLI demo
edit; the other inputs are unchanged):

| Measure | Computation | Exact value |
|---|---|---|
| Spend | (overwritten) | **50,000** |
| Clicks | 50,000 / 1.50 | **33,333.3̄** (= 100000/3) |
| Leads | 33,333.3̄ × 0.020 | **666.6̄** (= 2000/3) |
| Customers | 666.6̄ × 0.10 | **66.6̄** (= 200/3) |
| Revenue | 66.6̄ × 200 | **13,333.3̄** (= 40000/3) |
| Gross_Profit | 13,333.3̄ × 0.70 | **9,333.3̄** (= 28000/3) |

**Consolidated samples** (all use Baseline × Working in the Scenario/Version
dims):

| Coordinate | Computation | Exact value |
|---|---|---|
| Q1_2026 × Paid_Search × Tampa × Spend | (10500 + 11000 + 11500) | **33,000** |
| Mar_2026 × Paid_Search × Florida × Spend | (11500 + 11700 + 11900) | **35,100** |
| Mar_2026 × Paid_Media × Tampa × Spend | (11500 + 12500 + 13500) | **37,500** |
| Q1_2026 × Paid_Media × Florida × Spend | (27 leaves; closed-form below) | **329,400** |
| Q1_2026 × Paid_Search × Tampa × CPC | weighted-avg, all CPC=1.50 | **1.50** |
| Q1_2026 × Paid_Search × Florida × CPC | weighted-avg, see derivation | **≈ 1.5202381** |

Closed-form derivation for the 27-leaf Spend rollup
`Q1 × Paid_Media × Florida`:

```
Σ_{t∈{1,2,3}} Σ_{c∈{0,1,2}} Σ_{m∈{0,1,2}} (10000 + 500·t + 1000·c + 200·m)
  = 27·10000 + 500·(1+2+3)·9 + 1000·(0+1+2)·9 + 200·(0+1+2)·9
  = 270,000 + 27,000 + 27,000 + 5,400
  = 329,400
```

Closed-form derivation for the 9-leaf weighted-average CPC rollup
`Q1 × Paid_Search × Florida`:

```
Per-market totals (Spend over Jan+Feb+Mar):
  Tampa  (m=0): 10500 + 11000 + 11500 = 33,000   CPC = 1.50
  Orlando(m=1): 10700 + 11200 + 11700 = 33,600   CPC = 1.52
  Miami  (m=2): 10900 + 11400 + 11900 = 34,200   CPC = 1.54

Numerator   = 1.50·33000 + 1.52·33600 + 1.54·34200
            = 49,500 + 51,072 + 52,668
            = 153,240
Denominator = 33,000 + 33,600 + 34,200
            = 100,800
CPC = 153,240 / 100,800 = 1.5202380952...
```

These six rows are the assertions in tests `t_acme_read_consolidated_*` (§10.1)
and `t_acme_read_consolidated_cpc_uses_weighted_average` (§10.1).

### 4.6 The CLI demo

`mc demo` (in `mc-cli`) runs:

```
$ mc demo
Building Acme cube...
  6 dimensions, 3 hierarchies, 11 measures, 5 rules
  Loaded 2,520 input cells in 1 scenario × 1 version

Reading sample cells:
  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Spend)        =     11_500.00
  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Clicks)       =      7_666.67
  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Leads)        =        153.33
  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Customers)    =         15.33
  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Revenue)      =      3_066.67
  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Gross_Profit) =      2_146.67

Reading consolidated cells:
  (Baseline, Working, Q1_2026,  Paid_Search, Tampa,   Spend)      =     33_000.00
  (Baseline, Working, Mar_2026, Paid_Search, Florida, Spend)      =     35_100.00
  (Baseline, Working, Mar_2026, Paid_Media,  Tampa,   Spend)      =     37_500.00
  (Baseline, Working, Q1_2026,  Paid_Media,  Florida, Spend)      =    329_400.00
  (Baseline, Working, Q1_2026,  Paid_Search, Florida, CPC)        =          1.5202381

Trace for (Mar_2026, Paid_Search, Tampa, Revenue):
  Revenue = 3_066.67 (Rule rule_revenue: Mul)
  ├── Customers = 15.33 (Rule rule_customers: Mul)
  │   ├── Leads = 153.33 (Rule rule_leads: Mul)
  │   │   ├── Clicks = 7_666.67 (Rule rule_clicks: Div)
  │   │   │   ├── Spend = 11_500.00 (Input)
  │   │   │   └── CPC = 1.50 (Input)
  │   │   └── CVR = 0.020 (Input)
  │   └── Close_Rate = 0.10 (Input)
  └── AOV = 200.00 (Input)

Writing Spend(Mar_2026, Paid_Search, Tampa) = 50_000:
  Written. Revision 1 → 2.
  N dependent cells dirtied.   (exact N depends on impl; bounded — see §8)

Re-reading Revenue:
  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Revenue) = 13_333.33   (was 3_066.67)
  (Baseline, Working, Mar_2026, Paid_Search, Tampa, Gross_Profit) = 9_333.33   (was 2_146.67)

Rejecting write to Revenue (derived):
  Error: write rejected (derived cell): ...

Rejecting write to Q1_2026 Spend (consolidated):
  Error: write rejected (consolidated cell): ...

Done.
```

The CLI is a smoke test for human eyes; it prints the same numbers the test
suite asserts on. CI runs the test suite, not the CLI.

---

## 5. Build the cube — exact construction order

```rust
pub fn build_acme_cube() -> Result<(Cube, AcmeRefs), EngineError> {
    let id_gen = IdGenerator::new();
    let cube_id = id_gen.cube();
    let root = id_gen.principal();

    // 1. Dimensions (each builder returns Result so dimension build-time
    //    invariants — frozen mutation, missing default hierarchy, etc. —
    //    propagate cleanly).
    let scenario_dim = build_scenario_dim(&id_gen)?;
    let version_dim  = build_version_dim(&id_gen)?;
    let time_dim     = build_time_dim(&id_gen)?;     // includes hierarchy
    let channel_dim  = build_channel_dim(&id_gen)?;  // includes hierarchy
    let market_dim   = build_market_dim(&id_gen)?;   // includes hierarchy
    let measure_dim  = build_measure_dim(&id_gen)?;

    // 2. Capture all the IDs into `refs`
    let refs = AcmeRefs::from(&scenario_dim, &version_dim, &time_dim,
                              &channel_dim, &market_dim, &measure_dim);

    // 3. Cube builder
    let mut cube = Cube::builder(cube_id, "Acme_MarketingFinance")
        .add_dimension(scenario_dim)
        .add_dimension(version_dim)
        .add_dimension(time_dim)
        .add_dimension(channel_dim)
        .add_dimension(market_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(root)
        .add_rule(build_rule_clicks(&refs, &id_gen)?)?
        .add_rule(build_rule_leads(&refs, &id_gen)?)?
        .add_rule(build_rule_customers(&refs, &id_gen)?)?
        .add_rule(build_rule_revenue(&refs, &id_gen)?)?
        .add_rule(build_rule_gross_profit(&refs, &id_gen)?)?
        .build()?;

    // 4. Load input data
    write_canonical_inputs(&mut cube, &refs)?;

    Ok((cube, refs))
}
```

Tests, the CLI, and benchmarks invoke the fixture with `expect`:

```rust
let (cube, refs) = build_acme_cube().expect("acme fixture must build");
```

`expect` is allowed in test/bench/CLI code per the `unwrap`-ban exception in
§12 acceptance criterion 10.

**Dimension build order matters:** Measure dim must be the last `add_dimension`
call so coordinates are constructed with the measure in the last slot. The
canonical dimension order in every coordinate is:

```
[Scenario, Version, Time, Channel, Market, Measure]
```

Tests assert this order.

---

## 6. Read algorithm (verbatim)

```rust
fn read_inner(
    cube: &mut Cube,
    coord: &CellCoordinate,
    principal: PrincipalId,
    request_trace: bool,
) -> Result<CellValue, EngineError> {
    // 1. Permission
    if !cube.permissions.check(principal, cube, coord, capability::READ) {
        return Err(EngineError::InsufficientPermission { principal, coord: coord.clone() });
    }

    // 2. Determine if the coord is consolidated.
    let coord_kind = classify_coord(cube, coord);
    match coord_kind {
        CoordKind::Leaf => read_leaf(cube, coord, request_trace),
        CoordKind::Consolidated { dims } => Consolidator::read(cube, coord, request_trace),
    }
}

fn read_leaf(cube: &mut Cube, coord: &CellCoordinate, trace: bool) -> Result<CellValue, EngineError> {
    let measure_pos = cube.measure_dimension_position;
    let measure_id = coord.element_at(measure_pos);
    let measure_dim = &cube.dimensions[measure_pos];
    let measure = measure_dim.element(measure_id).unwrap();
    let meta = measure.measure_meta.as_ref().unwrap();

    match meta.role {
        MeasureRole::Input => read_input_leaf(cube, coord, trace),
        MeasureRole::Derived => read_derived_leaf(cube, coord, trace),
    }
}
```

`read_input_leaf` is straight cache lookup. `read_derived_leaf`:

1. If `cube.dirty.is_dirty(coord)` or `cube.store.read(coord).is_none()`:
   - Find the rule for `measure_id`.
   - Evaluate `rule.body` recursively. Each `SelfRef(measure)` resolves to a
     coord identical to `coord` except with the measure slot replaced.
   - Track every coord actually read; compare to `rule.declared_dependencies`
     after expanding `SameAsTarget`. Any unauthorized read is an error in test
     mode (cfg flag).
   - Add forward and reverse edges to `cube.deps`.
   - Compute the value (with null-poisoning math; see §7).
   - Write to `store` with `Provenance::Rule`.
   - `cube.dirty.clear(coord)`.
2. Return the cached `CellValue`.

If `request_trace`, the recursion materializes a `Trace` alongside the value.

---

## 7. Null and arithmetic semantics (verbatim)

These rules are non-negotiable. Tests in §10 pin them.

| Op | LHS | RHS | Result |
|---|---|---|---|
| `Add` | Null | Null | Null |
| `Add` | Null | x | x |
| `Add` | x | Null | x |
| `Sub` | Null | Null | Null |
| `Sub` | Null | x | -x |
| `Sub` | x | Null | x |
| `Mul` | Null | _ | Null |
| `Mul` | _ | Null | Null |
| `Div` | Null | _ | Null |
| `Div` | _ | Null | Null |
| `Div` | x | 0.0 | Null  (no panic, no infinity) |
| `Div` | x | y where y is f64 close to 0 | Null when `\|y\| < 1e-300` (treated as 0) |
| `IfNull` | Null | fallback | fallback |
| `IfNull` | x | _ | x |

NaN must never appear in storage (rejected at writeback) and must never appear in
intermediate computation (any operand producing NaN is treated as Null).
Infinity is similarly rejected.

For consolidation:

| Aggregation | Null behavior |
|---|---|
| `Sum` | Null contributes 0; if all children Null, result is Null |
| `WeightedAverage` | Null value contributes nothing to numerator; Null weight contributes nothing to numerator or denominator; if numerator weight is 0, result is Null |
| `Min` / `Max` | Nulls excluded; if all Null, result is Null |

---

## 8. Dirty propagation (exact algorithm)

On a successful write to coord `C`:

```
1. Bump cube.revision.
2. Update cube.store with the new value at C.
3. Compute initial dirty set: {C}.
4. Compute hierarchy ancestors of C across each consolidated dim:
   For each dimension D in cube:
     If C's element in D has ancestors in D's default hierarchy:
       For each ancestor A and each combination of leaves in the
       *other* dimensions of C, the corresponding consolidated coord
       is dirty. Phase 1 marks per-coord, not per-aggregate-pattern.
5. Compute rule dependents: closure under cube.deps.reverse for the
   coords above.
6. Mark all in cube.dirty.
7. Return WritebackResult { invalidated: <full dirty set> }.
```

For Acme, a write to `(Baseline, Working, Mar_2026, Paid_Search, Tampa, Spend)`
must produce a dirty set with at minimum the following membership properties.
Tests assert these as **set predicates**, not exact counts, because two
correct implementations may differ on whether a never-read consolidated coord
is in the dirty set or is simply uncached (and therefore implicitly dirty when
first read). Both behaviors are correct as long as a subsequent read returns
the right value.

**Required-present** (the dirty set MUST include these coords):

- The same leaf coord at each of the 5 derived measures: Clicks, Leads, Customers, Revenue, Gross_Profit.
- The Spend coord at every coordinate that is a strict hierarchy ancestor of
  the leaf in **at least one** consolidated dimension (Time, Channel, or
  Market) and a leaf in the others. Concretely for Mar/Paid_Search/Tampa Spend
  the tier-1 ancestors are: Q1 × Paid_Search × Tampa, Mar × Paid_Media × Tampa,
  Mar × Paid_Search × Florida.
- The full triple-consolidated Spend coord: Q1 × Paid_Media × Florida × Spend.
  (Plus partial pairs like Q1 × Paid_Media × Tampa.)
- **At any consolidated coord where Spend is dirty, every derived measure is
  also dirty at that coord.** (Because Clicks consolidation reads the same
  leaves Spend consolidates from, and the Clicks rule reads Spend at the leaf.)

**Required-absent** (the dirty set MUST NOT include these coords):

- Any coord whose Market is Atlanta, Charlotte, NYC, or Boston (no path through Tampa's leaf).
- Any coord whose Channel is Email or Organic (not in Paid_Media).
- Any coord whose Scenario is Aggressive or Conservative.
- Any coord whose Version is Submitted or Approved.
- Any coord whose Measure is one of the input measures other than Spend at the
  written leaf and its hierarchy ancestors (CPC, CVR, Close_Rate, AOV,
  COGS_Rate are inputs and never become dirty just because Spend was written).

**Bounded** (the dirty set's total size MUST satisfy):

```
|dirty_set| <= 6 × |hierarchy_ancestor_coords_of(written_coord)|
            + 5
```

where `hierarchy_ancestor_coords_of` counts every Cartesian-product of
ancestors-or-self across the 3 hierarchical dims minus the 1 leaf coord, and
the `6×` factor accounts for Spend plus 5 derived measures at each ancestor
coord. The `+5` covers the leaf-coord derived measures. For the Acme leaf
write here, the upper bound expands to:

```
ancestors(Mar) × ancestors(Paid_Search) × ancestors(Tampa) − 1
  = (Mar, Q1, FY)          → 3
  × (Paid_Search, Paid_Media, All_Channels) → 3
  × (Tampa, Florida, Southeast, USA)        → 4
  − 1                                        = 35
6 × 35 + 5 = 215
```

So `|dirty_set| <= 215` is the contract for this specific write. Tests assert
the bound, the required-present set, and the required-absent set — never an
exact count.

Why a bound and not an exact number? An implementation that lazily marks
ancestors only when they have been previously cached will produce a smaller
dirty set than one that eagerly marks every Cartesian-product ancestor.
Both are correct on first read after the write, because uncached cells
recompute regardless. Pinning an exact count would over-specify the
implementation and force one strategy over the other.

---

## 9. Fixtures — exact data

`mc-fixtures` exports these helpers and data:

```rust
pub fn build_acme_cube() -> Result<(Cube, AcmeRefs), EngineError>;

pub fn write_canonical_inputs(cube: &mut Cube, refs: &AcmeRefs) -> Result<usize, EngineError>;
// Returns count of cells written. Asserted to equal 2_520 for Baseline × Working.

pub fn coord(cube: &Cube, refs: &AcmeRefs,
             scenario: ElementId, version: ElementId, time: ElementId,
             channel: ElementId, market: ElementId, measure: ElementId)
    -> CellCoordinate;
// Builds a coord using the dimension order [Scen, Ver, Time, Channel, Market, Measure].

pub fn golden_inputs() -> Vec<(GoldenCoordSpec, f64)>;
// Returns deterministic golden values for every Baseline × Working leaf.
```

Tests in §10 reference `golden_inputs()` for assertions.

---

## 10. Correctness Doctrine — exact tests

Every test below is a `#[test]` function in `crates/mc-core/tests/`. Each test
name is exactly as listed; renaming or omitting any of these is a contract
violation.

All tests use `mc_fixtures::build_acme_cube` for setup unless noted.

### 10.1 `acme_demo.rs` — the canonical end-to-end test

```rust
#[test]
fn t_acme_build_succeeds()
// Cube builds; 6 dims, 3 hierarchies, 11 measures, 5 rules.

#[test]
fn t_acme_input_count_is_2520()
// write_canonical_inputs returns 2520.

#[test]
fn t_acme_read_input_leaf_returns_written_value()
// Read (Baseline, Working, Mar_2026, Paid_Search, Tampa, Spend) returns
// the exact value from golden_inputs().

#[test]
fn t_acme_read_derived_leaf_clicks()
// Read Clicks at the same coord. Assert
// abs(value - (spend / cpc)) < 1e-9 against golden_inputs.

#[test]
fn t_acme_read_derived_leaf_revenue()
// Read Revenue. Assert
// abs(value - (spend / cpc * cvr * close_rate * aov)) < 1e-9.

#[test]
fn t_acme_read_derived_leaf_gross_profit()
// Read Gross_Profit. Assert
// abs(value - (revenue * (1 - cogs_rate))) < 1e-9.

#[test]
fn t_acme_read_consolidated_q1_spend()
// Read (Baseline, Working, Q1_2026, Paid_Search, Tampa, Spend).
// Assert == sum of (Jan, Feb, Mar) × (Paid_Search) × (Tampa) × Spend.

#[test]
fn t_acme_read_consolidated_florida_spend()
// Read (Baseline, Working, Mar_2026, Paid_Search, Florida, Spend).
// Assert == sum of (Tampa, Orlando, Miami) × Spend.

#[test]
fn t_acme_read_consolidated_paid_media_spend()
// Read (Baseline, Working, Mar_2026, Paid_Media, Tampa, Spend).
// Assert == sum of (Paid_Search, Paid_Social, Display) × Spend.

#[test]
fn t_acme_read_triple_consolidated_spend()
// Read (Baseline, Working, Q1_2026, Paid_Media, Florida, Spend).
// Assert == sum of 27 leaves (3 months × 3 channels × 3 markets).

#[test]
fn t_acme_read_consolidated_cpc_uses_weighted_average()
// Read (Baseline, Working, Q1_2026, Paid_Search, Florida, CPC).
// Assert == sum(cpc_i * spend_i) / sum(spend_i) for the 9 leaves.
// NOT equal to simple sum(cpc_i) and NOT equal to simple avg(cpc_i).

#[test]
fn t_acme_read_consolidated_revenue_at_q1_florida_paid_media()
// Read (Baseline, Working, Q1_2026, Paid_Media, Florida, Revenue).
// Assert == sum of 27 leaf Revenues, each computed by the rule chain.

#[test]
fn t_acme_trace_for_revenue_returns_full_tree()
// read_with_trace returns a Trace whose root.value matches the cell value
// and whose tree depth is exactly 5 (Revenue → Customers → Leads → Clicks → Spend|CPC).
// Total leaf-input nodes in trace == 5 (Spend, CPC, CVR, Close_Rate, AOV).

#[test]
fn t_acme_trace_root_value_equals_read_value()
// For 100 random coords (use proptest), the trace's root value equals the
// cell value byte-for-byte.
// **DEFERRED proptest variant per §0.A** — until proptest returns, ship a
// deterministic version that picks ~10 representative coords by hand
// (anchor leaf, three single-consolidated, the triple-consolidated, plus
// one-of-each derived measure). The proptest random sweep replaces this
// once §0.A closes.

#[test]
fn t_acme_write_to_input_succeeds()
// Write Spend = 50_000 at (Baseline, Working, Mar_2026, Paid_Search, Tampa).
// Revision bumps. Subsequent read returns 50_000.

#[test]
fn t_acme_write_invalidates_dependents()
// After the above write, read Revenue at the same leaf coord. Asserted to be
// recomputed (not stale).

#[test]
fn t_acme_write_invalidates_consolidated_ancestors()
// After the above write, read (Baseline, Working, Q1_2026, Paid_Search, Tampa, Spend).
// Assert it equals the new sum (with the modified March value, not stale Q1 cached).

#[test]
fn t_acme_dirty_set_required_present_after_one_spend_write()
// After writing Spend at (Baseline, Working, Mar_2026, Paid_Search, Tampa),
// assert the dirty set INCLUDES every coord in §8's "required-present" list:
//   - 5 leaf-coord derived measures (Clicks, Leads, Customers, Revenue, Gross_Profit)
//   - Spend at Q1 × Paid_Search × Tampa
//   - Spend at Mar × Paid_Media × Tampa
//   - Spend at Mar × Paid_Search × Florida
//   - Spend at Q1 × Paid_Media × Florida (the triple-consolidated coord)

#[test]
fn t_acme_dirty_set_required_absent_after_one_spend_write()
// After the same write, assert the dirty set EXCLUDES every coord in §8's
// "required-absent" list:
//   - Any coord with Market ∈ {Atlanta, Charlotte, NYC, Boston}
//   - Any coord with Channel ∈ {Email, Organic}
//   - Any coord with Scenario ∈ {Aggressive, Conservative}
//   - Any coord with Version ∈ {Submitted, Approved}
//   - Any input measure other than Spend at the written coord/ancestors

#[test]
fn t_acme_dirty_set_size_within_bound_after_one_spend_write()
// Assert cube.dirty.len() <= 215 (the §8 upper bound for this write).
// Implementations that mark eagerly will hit close to 215; lazy
// implementations may report fewer. Both are correct as long as reads of
// the required-present coords return up-to-date values.
```

### 10.2 `writeback.rs`

```rust
#[test]
fn t_write_to_derived_cell_returns_error()
// Attempt to write to Revenue. Assert WritebackError::DerivedCellNotWritable.
// Cube state unchanged (revision unchanged).

#[test]
fn t_write_to_consolidated_cell_returns_error()
// Attempt to write Spend at (Baseline, Working, Q1_2026, Paid_Search, Tampa).
// Assert WritebackError::ConsolidatedCellNotWritable.

#[test]
fn t_write_with_wrong_dtype_returns_error()
// Attempt to write ScalarValue::I64(50_000) to an F64 measure.
// Assert TypeMismatch.

#[test]
fn t_write_with_nan_returns_error()
// Attempt to write ScalarValue::F64(f64::NAN). Assert InvalidValue.

#[test]
fn t_write_with_inf_returns_error()
// Attempt to write ScalarValue::F64(f64::INFINITY). Assert InvalidValue.

#[test]
fn t_write_stale_revision_returns_error()
// Take revision r0. Write something (advances to r1). Attempt write with
// expected_revision = r0. Assert StaleRevision.

#[test]
fn t_write_revision_bumps_monotonically()
// 100 successful writes; assert each increases revision by exactly 1.

#[test]
fn t_write_to_approved_version_returns_error()
// (Skipped: requires version state machine — Phase 2 fully implements;
// Phase 1 emits LockedVersion if Version dim's element has VersionState::Approved.)
// Implement minimally: if the coord's Version element's state is Approved,
// reject with LockedVersion. Test the rejection.

#[test]
fn t_write_with_invalid_principal_returns_error()
// Write with PrincipalId(99) (no grants). Assert InsufficientPermission.

#[test]
fn t_write_increment_intent()
// Use WriteIntent::Increment. Assert old + delta == new.

#[test]
fn t_write_clear_intent()
// Use WriteIntent::Clear. Assert value becomes Null.
```

### 10.3 `consolidation.rs`

```rust
#[test]
fn t_sum_aggregation_with_all_leaves_present()
// Custom small-cube fixture: 3 months only. All Spends written. Assert
// quarterly Spend == sum of 3 months.

#[test]
fn t_sum_aggregation_with_one_null_leaf()
// Same fixture, write only Jan and Mar. Assert quarterly Spend == Jan + Mar
// (not Null). Provenance is Consolidation.

#[test]
fn t_sum_aggregation_with_all_null_leaves()
// Same fixture, no writes. Assert quarterly Spend value is Null,
// provenance is Consolidation { child_count: 3 }.

#[test]
fn t_weighted_average_basic()
// 3 months: spend = [10, 20, 30], cpc = [1, 2, 3].
// Assert quarterly CPC == (10*1 + 20*2 + 30*3) / (10+20+30) == 140/60.

#[test]
fn t_weighted_average_with_null_weight()
// Same fixture, but Feb spend is Null. Feb shouldn't contribute at all.
// Assert quarterly CPC == (10*1 + 30*3) / (10 + 30) == 100/40.

#[test]
fn t_weighted_average_zero_total_weight()
// 3 months: spend = [0, 0, 0], cpc = [1, 2, 3].
// Assert quarterly CPC is Null (no signal).

#[test]
fn t_min_aggregation_with_nulls()
// 3 months: spend = [Null, 5, 10]. Assert min == 5.

#[test]
fn t_max_aggregation_with_nulls()
// 3 months: spend = [Null, 5, 10]. Assert max == 10.

#[test]
fn t_consolidation_caches_value_within_revision()
// Read consolidated Q1 Spend; record duration. Read again immediately;
// assert second read is at least 10x faster (cache hit).

#[test]
fn t_consolidation_recomputes_after_dependent_dirty()
// Read consolidated Q1 Spend (caches). Write to Mar leaf. Read consolidated
// Q1 Spend. Assert new value reflects the write.

#[test]
fn t_consolidation_at_root_level_in_three_dims()
// Read (Baseline, Working, FY_2026, All_Channels, USA, Spend).
// Assert == sum of all 12 × 5 × 7 = 420 leaves.

#[test]
fn t_consolidation_provenance_has_correct_child_count()
// Read consolidated coord with mixed levels. Assert child_count == count
// of leaf coords that actually contributed.
```

### 10.4 `trace.rs`

```rust
#[test]
fn t_trace_for_input_cell_is_single_node()
// Trace for an Input leaf. Assert tree has exactly one node, op == InputLookup.

#[test]
fn t_trace_for_clicks_has_two_input_children()
// Trace for Clicks leaf. Assert root op == RuleEvaluation { rule_clicks, Div },
// children are exactly Spend (InputLookup) and CPC (InputLookup).

#[test]
fn t_trace_depth_for_revenue()
// Trace for Revenue leaf. Assert depth (longest root-to-leaf path) == 5.

#[test]
fn t_trace_depth_for_gross_profit()
// Trace for Gross_Profit leaf. Assert depth == 6.
// (Gross_Profit → Revenue → Customers → Leads → Clicks → Spend|CPC)

#[test]
fn t_trace_for_consolidated_cell_has_correct_child_count()
// Trace for (Q1, Paid_Search, Tampa, Spend). Assert root.children.len() == 3.

#[test]
fn t_trace_for_triple_consolidated_revenue()
// Trace for (Q1, Paid_Media, Florida, Revenue). Root op == Consolidation.
// 27 children, each itself a Revenue subtree.

#[test]
fn t_trace_root_value_equals_cell_value_property()
// proptest: pick 100 random coords, assert trace.root.value == read(coord).value.
// **DEFERRED per §0.A** — TODO(proptest) stub until proptest returns. The
// deterministic equivalent in §10.1 (`t_acme_trace_root_value_equals_read_value`)
// covers a hand-picked subset.

#[test]
fn t_trace_records_input_provenance_correctly()
// Assert InputLookup nodes carry written_at and written_by from the actual write.

#[test]
fn t_trace_with_null_input_emits_null_poison()
// Don't write Spend; read Clicks. Trace shows InputLookup with Null,
// rule evaluation produces Null. Assert root has op NullPoison or
// the Div node propagates Null per §7.
```

### 10.5 `dependency.rs`

```rust
#[test]
fn t_dependency_graph_is_empty_immediately_after_cube_build()
// Phase 1 builds the dependency graph LAZILY — edges are materialized when a
// rule first evaluates at a coordinate. Immediately after build, before any
// reads, deps.forward and deps.reverse are empty.

#[test]
fn t_dependency_graph_populates_on_first_read()
// Read Revenue at one leaf coord. Now deps contains the edges for THAT
// coordinate's rule chain (Revenue→Customers, Customers→Leads, etc.) — five
// rule edges materialized. Reading Revenue at a different leaf coord adds
// another five edges (concretely per-coord, not per-rule).

#[test]
fn t_dependency_graph_validates_full_fixture_when_forced()
// `mc-fixtures` exposes a debug helper `materialize_all_dependencies(&mut cube)`
// that reads every leaf-coord × every-derived-measure once. After it runs,
// deps contains 5 derived measures × 12 months × 5 channels × 7 markets ×
// 1 scenario × 1 version = 2,100 forward edges in the rule layer (each
// coord has one outgoing rule edge per declared dependency, so 2 deps per
// rule × 5 rules × 420 leaves = 4,200 dependency rows; tests assert
// deps.forward_edge_count() falls in this range).
// This test is OFF the critical Phase 1 path; it is opt-in for full validation.

#[test]
fn t_dependency_graph_detects_cycle_at_rule_addition()
// Build a cube with rules A=B+1, B=C+1, then attempt to add C=A+1.
// Assert cube.builder().add_rule returns Err(DependencyCycle).

#[test]
fn t_dependency_graph_rejects_undeclared_dependency_in_test_mode()
// Build a rule whose body references measure X but doesn't declare X.
// Add to cube. On first evaluation, assert UndeclaredDependency.

#[test]
fn t_dependency_invalidation_walks_full_closure()
// Write Spend. Manually inspect cube.dirty.iter() and assert it includes
// Clicks, Leads, Customers, Revenue, Gross_Profit at the same coord, plus
// hierarchy ancestors.

#[test]
fn t_dependency_does_not_invalidate_unrelated_cells()
// Write Spend at Tampa. Assert no Atlanta-coord cells are in the dirty set.
```

### 10.6 `locks_permissions.rs`

```rust
#[test]
fn t_root_principal_can_read_and_write_anywhere()

#[test]
fn t_non_root_with_no_grant_cannot_read()
// PrincipalId(99) with no grant. Read returns InsufficientPermission.

#[test]
fn t_grant_for_subtree_allows_writes_in_subtree()
// Grant Florida-only Write. Assert Tampa write succeeds; Atlanta write fails.

#[test]
fn t_hard_lock_blocks_other_principals()
// Principal A acquires Hard lock on Florida. Principal B with Write grant
// on Atlanta succeeds; B's write to Florida fails with LockedCell.

#[test]
fn t_lock_owner_can_still_write_within_lock()
// Principal A acquires Hard lock. A's writes within the lock succeed.

#[test]
fn t_expired_lock_does_not_block()
// Acquire lock with expires_at in the past. After purge_expired, writes succeed.

#[test]
fn t_soft_lock_allows_writes_but_marks_advisory()
// Soft lock. Other principal's write succeeds but WritebackResult should
// carry an advisory flag (Phase 1: log only, no struct field — TODO Phase 2).
// For Phase 1, just assert the soft lock doesn't error.

#[test]
fn t_release_lock_by_non_owner_without_unlock_capability_fails()
```

### 10.7 `correctness.rs` — the cross-cutting Correctness Doctrine

> **Proptest-backed tests in this section are DEFERRED per §0.A.** The
> deterministic doctrine tests (`doctrine_determinism`,
> `doctrine_coherence_within_slice`, `doctrine_authorization_before_write`,
> `doctrine_no_silent_type_coercion`, `doctrine_no_silent_dependency_miss`,
> `doctrine_null_zero_distinct`, `doctrine_no_mutation_of_frozen_dimensions`,
> `doctrine_no_writes_to_derived_cells`) are required and run today.
>
> The proptest-based tests below (`doctrine_atomicity_of_write`,
> `doctrine_causality`) are **`// TODO(proptest):` stubs** until proptest
> returns to `mc-core`'s dev-deps. The stubs must still exist as
> `#[test] fn name() { /* TODO(proptest): see brief §10.7 */ }` so the
> test file lists them; they just compile to no-ops. When proptest comes
> back, fill in the body per the comment beneath each name.

```rust
#[test]
fn doctrine_determinism()
// Read the same cell 1000 times; assert byte-identical values every time.

#[test]
fn doctrine_coherence_within_slice()
// Write happens between two cells in a slice. Assert the slice either has
// pre-write or post-write values throughout, never mixed.
// Implementation note: slice takes a snapshot of revision at start and
// reads everything against that snapshot.

#[test]
fn doctrine_atomicity_of_write()
// proptest: interleave writes and reads. Assert reader never sees a
// half-applied write (e.g., new value but old dirty state).
// **DEFERRED** per §0.A — currently a TODO(proptest) stub.

#[test]
fn doctrine_causality()
// proptest: for every (coord, write, read) sequence where read happens
// after write of an upstream cell, the read returns the post-write value.
// **DEFERRED** per §0.A — currently a TODO(proptest) stub.

#[test]
fn doctrine_authorization_before_write()
// Insert a permission denial. Cube state remains unchanged after rejected write.

#[test]
fn doctrine_no_silent_type_coercion()
// Attempt every cross-type write (I64 to F64, F64 to I64, etc.). Assert all
// return TypeMismatch.

#[test]
fn doctrine_no_silent_dependency_miss()
// In test mode (#[cfg(test)] or feature flag), every read tracks actual
// dependencies and asserts they match declarations. Fixture rule with
// missing declaration is rejected at first eval.

#[test]
fn doctrine_null_zero_distinct()
// Spend = Null vs Spend = 0.0. Read Clicks. In Null case: Clicks is Null.
// In zero case: Clicks is Null too (division by zero policy in §7).
// Then write CPC = Null but Spend = 1.0. Assert Clicks is Null (Mul/Div null).
// Then write CPC = 1.0 but Spend = 0.0. Assert Clicks = 0.0 (not Null).

#[test]
fn doctrine_no_mutation_of_frozen_dimensions()
// Attempt to remove an element after cube is built. Assert error
// DimensionFrozen.

#[test]
fn doctrine_no_writes_to_derived_cells()
// Already covered by t_write_to_derived_cell_returns_error; this is the
// doctrine-level meta-test that asserts every derived measure rejects writes.
```

### 10.8 Snapshot tests (in `correctness.rs`)

```rust
#[test]
fn t_snapshot_captures_current_state()
// Take snapshot at revision r. Read coord. Write coord. Read snapshot's
// value of coord — assert it's the pre-write value.

#[test]
fn t_rollback_to_snapshot_restores_state()
// Write, snapshot, write more, rollback. All cells read identical to
// snapshot's state.

#[test]
fn t_snapshot_does_not_block_writes()
// Take snapshot. Write 1000 times to live cube. Snapshot still readable.

#[test]
fn t_snapshot_label_is_optional()
// Take labeled and unlabeled snapshots. Both work.

#[test]
fn t_snapshot_cube_id_mismatch_rejected()
// Build two cubes A and B. Take snapshot of A. Attempt rollback of B
// using A's snapshot. Assert SnapshotCubeMismatch.
```

### 10.9 Test passing criteria

All 60+ tests above must pass — **with the §0.A carve-out: the proptest
variants in §10.7 (`doctrine_atomicity_of_write`, `doctrine_causality`)
and the proptest sweeps in §10.1 / §10.4 (`t_acme_trace_root_value_*`,
`t_trace_root_value_equals_cell_value_property`) compile to
`// TODO(proptest):` no-op stubs while criterion / proptest are out of
`mc-core`'s dev-deps. Their bodies fill in when §0.A closes.**

The CI pipeline runs `cargo test --workspace` with `--all-features` and
exits non-zero if any non-deferred test fails or any compiler warning
fires. The build is clippy-clean (`cargo clippy --all-targets --
-D warnings`).

---

## 11. Benchmarks — exact targets

> **THIS ENTIRE SECTION IS INERT WHILE §0.A IS ACTIVE.** Criterion is not
> pulled into `mc-core` on Rust 1.78 (see §0.A for the toolchain blocker),
> so the `crates/mc-core/benches/` directory is not yet created and
> `cargo bench --release` is not part of the Phase 1 ship gate today.
>
> What does still apply: the **Phase 1A ceilings below remain the design
> contract**. Implementations should be obviously-naive-but-not-pathological;
> when criterion returns and the bench harness lands, every 1A ceiling
> becomes a contractual ship-blocker again. Don't write 50× slower code
> just because the gate is temporarily off.
>
> When §0.A closes: create `crates/mc-core/BENCHES_DEFERRED.md → DELETED`,
> add the `[[bench]]` entries back to `mc-core/Cargo.toml` per §2.3, write
> the three bench files per §15 step 18, and re-enable acceptance
> criterion (5) per §12.

Three benchmark suites in `crates/mc-core/benches/`. Each uses Criterion. The
target hardware is a single-machine baseline: M1/M2 Mac or equivalent x86-64
laptop, single thread, release build (`cargo bench --release`).

**Phase 1 splits the benchmark targets into two tiers.** This avoids letting
performance tuning block correctness.

### Phase 1A — Correctness ceilings (CONTRACTUAL for shipping Phase 1)

These are the loose ceilings a naive-but-correct implementation should
satisfy. **Phase 1 ships when every benchmark is below its 1A ceiling.** A
straightforward `HashMap`-based store with recursive rule evaluation and no
SIMD/parallelism is expected to clear all of these.

### Phase 1B — Optimization targets (ASPIRATIONAL; track but do not gate on)

These are the numbers we want eventually. Missing a 1B target is *not* a
shipping blocker for Phase 1, but each miss should be tracked in a
`PERF.md` follow-up file so Phase 2/3 optimization work is justified by data.

### 11.1 `leaf_read_write.rs`

| Benchmark | Operation | 1A ceiling (ship) | 1B target (aspire) |
|---|---|---|---|
| `bench_read_input_leaf_cold` | First read of an input leaf (no cache) | < 20 µs | < 1 µs |
| `bench_read_input_leaf_warm` | Repeat read (cache hit) | < 5 µs | < 200 ns |
| `bench_read_derived_leaf_cold` | First read of a derived leaf (full rule chain Revenue, 5 levels deep) | < 100 µs | < 5 µs |
| `bench_read_derived_leaf_warm` | Repeat read (cache hit) | < 5 µs | < 200 ns |
| `bench_write_input_leaf` | Single Spend write including dirty propagation | < 200 µs | < 10 µs |
| `bench_write_input_leaf_no_deps` | Write to a coord with zero dependents (synthetic) | < 50 µs | < 2 µs |

### 11.2 `consolidation_read.rs`

| Benchmark | Operation | 1A ceiling (ship) | 1B target (aspire) |
|---|---|---|---|
| `bench_consolidation_3_leaves` | Q1 Spend at one leaf channel/market (3 months sum) | < 50 µs | < 3 µs |
| `bench_consolidation_27_leaves` | Q1, Paid_Media, Florida Spend (27 leaves) | < 1 ms | < 30 µs |
| `bench_consolidation_420_leaves` | FY_2026, All_Channels, USA Spend (420 leaves) | < 20 ms | < 500 µs |
| `bench_consolidation_revenue_27_leaves` | Q1, Paid_Media, Florida Revenue (27 leaves, full rule chain on each) | < 5 ms | < 200 µs |
| `bench_consolidation_weighted_avg_27` | Q1, Paid_Media, Florida CPC (weighted avg, 27 leaves, weights via Spend reads) | < 2 ms | < 100 µs |

### 11.3 `full_recompute.rs`

| Benchmark | Operation | 1A ceiling (ship) | 1B target (aspire) |
|---|---|---|---|
| `bench_full_recompute_after_one_write` | After one Spend write, read all dirtied derived cells | < 50 ms | < 1 ms |
| `bench_load_canonical_inputs` | `write_canonical_inputs` (2,520 cells) | < 2 s | < 50 ms |
| `bench_full_revenue_slice` | Slice all leaf Revenues (12×5×7=420 cells) cold | < 200 ms | < 5 ms |
| `bench_full_revenue_slice_warm` | Same slice, all cells cached | < 50 ms | < 1 ms |

### 11.4 Benchmark CI behavior

> **§0.A note** — the rules below describe steady-state CI behavior. They
> are not active today because criterion is out of `mc-core`'s dev-deps.

`cargo bench` is **not** run in CI on every PR (too slow, too variable). Instead:

- Every PR includes the diff between current bench numbers and the **1A
  ceilings** (run locally; results pasted in PR description).
- Any benchmark exceeding its **1A ceiling** is a CI failure (run nightly on a
  fixed machine).
- Missing a **1B target** is logged in `PERF.md` but does not fail the build.
- Phase 1 ships when every benchmark clears its 1A ceiling, regardless of 1B
  target status.

The 1A ceilings are deliberately loose — roughly 20× the 1B targets — to
accommodate a `HashMap`-only store with `Box`-allocated `Expr` trees and no
inlining hints. Phase 2's optimization work will close the 1A→1B gap; Phase 1
just needs to be obviously-correct first.

---

## 12. Acceptance criteria for Phase 1

Phase 1 is **done** when all of the following are simultaneously true:

1. `cargo build --release --workspace` produces three binaries (or library outputs) with zero warnings.
2. `cargo clippy --all-targets --workspace -- -D warnings` exits 0.
3. `cargo fmt --check --all` exits 0.
4. `cargo test --workspace` passes 100% of the tests in §10 — **excluding the
   §10.7 proptest-backed tests (`doctrine_atomicity_of_write`,
   `doctrine_causality`) and the §10.4 / §10.1 proptest variants of
   `t_trace_root_value_equals_cell_value_property` /
   `t_acme_trace_root_value_equals_read_value`, which are deferred per
   §0.A.** Those tests must still EXIST as `// TODO(proptest):` stubs in
   their respective files; they just compile to no-ops today.
5. ~~`cargo bench --release` produces results where every benchmark is below its
   hard ceiling listed in §11.~~ **DEFERRED per §0.A** — `cargo bench` is
   inert today because criterion is out of `mc-core`'s dev-deps. This
   criterion becomes contractual again when §0.A closes.
6. `target/release/mc demo` runs to completion and prints output matching the
   shape in §4.6 (numbers must match `golden_inputs()` byte-for-byte).
7. The `docs/` folder contains both `engine-semantics.md` and this brief unchanged.
8. No code in `crates/` references any item in §1's "out of scope" list.
9. All tests run deterministically: 10 consecutive runs of `cargo test --workspace`
   produce identical pass/fail status. (Once proptest returns per §0.A, also
   pin a fixed `PROPTEST_CASES` and seed; until then, every test in the
   suite is already deterministic by construction and the 10-run gate is
   what enforces this.)
10. The codebase has **no `unwrap()` anywhere**. Tests, benches, fixtures, and
    `mc-cli` may use `expect("static reason")` with a human-readable reason
    string. `mc-core` library code has neither `unwrap()` nor `expect()` outside
    of `unreachable!()` calls justified by an `EngineError::Internal` invariant
    comment. There is no `panic!()` in `mc-core` outside of paths that already
    indicate a violated `Internal` invariant. There are no `println!` calls
    outside of `mc-cli`.

If any one of those is false, Phase 1 is not done.

---

## 13. Definition of "extremely specific" — what claude-code must NOT do

The implementer (human or AI) must not:

- Add a new public type to `mc-core` not listed in §3.
- Add a dependency to `Cargo.toml` not listed in §2.5.
- Skip a test in §10 because it "seems redundant."
- Loosen a hard ceiling in §11 because the implementation is "naturally slower."
- Rename any module, struct, function, or test listed in §3, §10, §11.
- Inline implementation choices that conflict with `engine-semantics.md` (e.g.,
  silently coercing types because "the user probably meant").
- Build features beyond §1 even if they "would be easy to add."
- Silently extend `Expr` with new variants, even ones that look natural.
- Use `unsafe` anywhere in `mc-core`.
- Use threads or async runtimes.
- Use serialization frameworks (no `serde` in Phase 1).

If the implementer needs anything in this list, the answer is to **stop and
update this brief**, then resume.

---

## 14. Definition of "extremely specific" — what claude-code MUST do

The implementer must:

- Match the names in §3 exactly.
- Match the test names and assertions in §10 exactly.
- Treat each test in §10 as a binding contract; failing tests are not
  "investigation items," they are blockers.
- Cite section numbers from `engine-semantics.md` in code comments where
  invariants are enforced (`// Per engine-semantics.md §13 I-WB-2: ...`).
- Document every public function with `///` doc comments referencing the
  semantics spec.
- Run `cargo fmt` before every commit.
- Run `cargo clippy --all-targets -- -D warnings` before every commit.
- Run the full test suite before declaring any milestone done.
- Ask for a brief update before adding anything in §13.

---

## 15. Implementation order (recommended)

A sequenced path for the implementer:

1. **Skeleton** — workspace, three crates, empty `lib.rs` files, CI config.
2. **`id.rs`, `revision.rs`, `value.rs`, `error.rs`** — foundation types. No tests yet.
3. **`element.rs`, `dimension.rs`** — types + builders. Tests:
   - dimension build with 5 elements
   - duplicate-element rejection
   - measure dim with role/dtype
4. **`hierarchy.rs`** — including cycle detection. Tests:
   - 3-month → Q1 hierarchy
   - cycle rejection
   - weight validation
5. **`coordinate.rs`** — including builder. Tests:
   - coord equality
   - missing slot rejection
   - cross-cube coord rejection
6. **`store.rs`** — `HashMapStore` impl. Tests:
   - read/write/remove round-trip
   - **`iter()` ordering: nondeterministic at the API level.** `HashMapStore`
     is backed by `ahash::AHashMap`, whose iteration order is not stable
     across runs. The store does NOT pay the cost of sorting on every
     `iter()` call. Any caller (test or otherwise) that needs a
     deterministic sequence must collect into a `Vec` and sort by
     `CellCoordinate`:
     ```rust
     let mut entries: Vec<_> = store.iter().collect();
     entries.sort_by(|(a, _), (b, _)| a.cmp(b));
     ```
     Consolidation walks targeted leaves via `read()`, not via `iter()`,
     so the hot path is unaffected. (Earlier drafts of this brief said
     "deterministic via key sort," ambiguous between "iter() sorts" and
     "callers sort." The engineering-correct interpretation is the
     latter; this clarification is logged in §17 changelog v1.2.)
7. **`cell.rs`** — `CellValue`, `Provenance`. No tests at this layer; tested via cube.
8. **`rule.rs`** — `Rule`, `Expr`, `RuleSet::add`. Tests:
   - well-typed rule passes
   - undeclared dependency rejected
   - duplicate target rejected
9. **`dependency.rs`** — `DependencyGraph`, cycle detection. Tests:
   - 3-rule chain (no cycle)
   - 3-rule cycle rejected
   - reverse-edge closure
10. **`dirty.rs`** — `DirtyTracker`. Tests:
    - mark, unmark, closure walk
11. **`consolidation.rs`** — `Consolidator::read` for Sum and WeightedAverage,
    Min, Max. Tests in `consolidation.rs`.
12. **`permission.rs`, `lock.rs`** — minimal Phase 1 implementations. Tests in `locks_permissions.rs`.
13. **`snapshot.rs`** — clone-and-store. Tests at the cube level.
14. **`cube.rs`** — `Cube`, `CubeBuilder`, `read`, `write`, `slice`, `snapshot`,
    `rollback_to`. The biggest module. Tests in `acme_demo.rs` and `correctness.rs`.
15. **`slice.rs`** — `SliceQuery`, `SliceResult`. Cube's `slice` method delegates here.
16. **`mc-fixtures`** — `build_acme_cube`, `write_canonical_inputs`,
    `golden_inputs`, `coord` helper.
17. **Integration tests** — fill out everything in §10. This is where every
    earlier shortcut gets caught.
18. **Benchmarks** — implement and tune until every 1A ceiling passes.
    **DEFERRED per §0.A** until criterion returns to `mc-core`'s dev-deps.
    Until then, leave a `crates/mc-core/BENCHES_DEFERRED.md` note pointing
    to §0.A and skip this step.
19. **`mc-cli`** — minimal CLI that runs the demo per §4.6.

This order surfaces correctness bugs in the lower layers before the upper layers
mask them.

---

## 16. Glossary

- **§N** — section N of this document.
- **Spec §N** — section N of [engine-semantics.md](./engine-semantics.md).
- **Acme** — the canonical demo cube name; tests, benchmarks, and the CLI all use it.
- **Refs** — `AcmeRefs`, the struct holding all the IDs of the Acme cube for tests.
- **Golden values** — the deterministic output of `golden_inputs()` and derived
  values computed by hand from the formulas in §4.5.1. Tests assert against these.
- **1A ceiling** — the benchmark threshold above which the build fails. Phase 1
  ships when every benchmark clears its 1A ceiling.
- **1B target** — the aspirational benchmark target for Phase 2 optimization
  work. Missing a 1B target is logged but does not block Phase 1.
- **Phase 1 done** — all 10 conditions in §12 are simultaneously true.

---

## 17. Changelog

### v1.1 — Cleanup pass (2026-05-01)

Consistency-only edits in response to external review. No new features, no
structural changes to the spec or build phasing.

| # | Change | Sections touched |
|---|---|---|
| 1 | **Recalculated all Acme golden values.** Original CLI demo had Revenue=153.33 (which was Leads), AOV=4,000 (canonical formula gives 200), CVR=0.05 (formula gives 0.020). Added a dedicated §4.5.1 with hand-calculated golden table covering anchor-cell inputs, derived chain, post-write values, and consolidated samples (including 27-leaf Spend rollup and 9-leaf weighted-avg CPC closed forms). CLI demo output now matches. | §4.5, §4.5.1 (new), §4.6 |
| 2 | **Resolved lazy-vs-eager dependency graph contradiction.** §3.12 said lazy; one §10.5 test contradicted by asserting full graph after build. Kept lazy. Replaced the eager test with three new tests: empty-after-build, populates-on-first-read, and an opt-in `materialize_all_dependencies` validator that's off the critical path. | §3.12 (already lazy), §10.5 |
| 3 | **Softened dirty-set count assertion.** §8 had three different counts (320, 40, 135) for the same scenario and the corresponding test asserted exact equality. Replaced with structured contract: required-present set, required-absent set, and an upper bound (215 for the canonical write). Three replacement tests in §10.1. | §8, §10.1 |
| 4 | **Removed `Box<dyn CellStore>` from Phase 1.** Trait-object storage requires `clone_box`, `Debug`-on-trait-object, and a `Clone for Box<dyn …>` impl that nothing in Phase 1 actually exercises. `Cube.store: HashMapStore` and `Snapshot.store: HashMapStore` directly, both with derived `Clone`. The `CellStore` trait is documented as a Phase 2 introduction. | §3.9, §3.16, §3.18 |
| 5 | **Added `LockedVersion` to `EngineError`.** Spec §13 always promised this; the brief's enum was missing it while the §10.2 `t_write_to_approved_version_returns_error` test expected it. Now defined alongside a new `VersionState` type in `element.rs`. | §3.4, §3.20 |
| 6 | **Made `build_acme_cube` return `Result`.** Previous draft used `.unwrap()` repeatedly inside the example, contradicting the §12 acceptance criterion banning `unwrap()`. Updated both forward declarations and the worked example. Tests/CLI use `expect("static reason")`. Acceptance criterion #10 rewritten to allow `expect` in tests/benches/CLI/fixtures and ban it (alongside `unwrap`) in `mc-core` library code. | §4 (signature), §5, §9, §12 |
| 7 | **Made `Provenance::Consolidation` carry multiple hierarchy IDs.** A triple-consolidated cell (Q1 × Paid_Media × Florida) walks three hierarchies; the original single-`HierarchyId` field couldn't represent it. Same change applied to `TraceOp::Consolidation`. Spec §7 and §14 likewise. | §3.8, §3.11, semantics §7, §14 |
| 8 | **Added missing public type definitions.** `WritebackRequest`, `WritebackResult`, `WriteIntent`, and `IdGenerator` were used in §6 (read algorithm) and §15 (impl order) but never formally specced. Added under §3.18 (writeback types) and §3.1 (IdGenerator). Removed bogus `Workspace` reference; clarified Workspace is Phase 3+. | §3.1, §3.18 |
| 9 | **Re-ranged benchmark ceilings into Phase 1A (correctness) and Phase 1B (optimization).** Original ceilings (e.g., warm-cache reads < 1 µs) were too tight for a naïve correct implementation. The 1A column is the contractual ship gate (~20× looser); the 1B column is the aspirational target tracked for Phase 2 work. CI gates on 1A only. | §11 |
| 10 | **Glossary updated** for the renamed ceiling/target distinction. | §16 |

No semantic changes to the engine model, no scope expansion, no rule-language
extensions. Every edit is consistency-driven and traceable to the GPT review
of 2026-04-30.

### v1.2 — Reality alignment with `CLAUDE.md` (2026-05-01)

Drift-elimination pass. The Phase 1 implementation surfaced a toolchain
blocker (Rust 1.78 vs criterion's `clap_lex` requiring `edition2024`) and a
`HashMapStore::iter()` ordering ambiguity in §15 step 6. Both were already
documented in `CLAUDE.md` (operating manual, §1.1 and §2.11 respectively);
this changelog entry brings the brief itself into sync so a reader of the
brief alone gets the same picture as a reader of `CLAUDE.md`.

| # | Change | Sections touched |
|---|---|---|
| 11 | **New §0.A "Active deviations from this brief".** A single hub explaining the criterion/proptest/insta deferral, what's currently inert (§2.5 `mc-core` dev-deps, §10.7 proptest tests, §11 bench gate, §12 criterion 5, §15 step 18), what still applies (correctness gates + 1A ceilings as design contract), and what closes the deviation. Cross-referenced from every section that mentions the deferred crates. | §0.A (new) |
| 12 | **§2.3 split into aspirational + as-shipped forms.** The original `Cargo.toml` block stays as the post-deviation target; a new §2.3.1 shows the actual file shipped today (without criterion/proptest/insta and without `[[bench]]` entries). | §2.3, §2.3.1 (new) |
| 13 | **§10.7 + §10.4 + §10.1 proptest variants annotated as DEFERRED.** The two doctrine tests (`doctrine_atomicity_of_write`, `doctrine_causality`) and the two `t_*_root_value_equals_*_property` proptest sweeps are explicitly marked as `// TODO(proptest):` stubs that compile to no-ops today. The deterministic doctrine tests in §10.7 are unchanged and remain required. | §10.1, §10.4, §10.7 |
| 14 | **§11 wrapped in a "this whole section is inert" callout.** 1A ceilings remain the design contract; the bench harness itself is deferred until criterion returns. §11.4 (CI behavior) gains a §0.A pointer note. | §11, §11.4 |
| 15 | **§12 acceptance criteria 4, 5, and 9 annotated.** Criterion 4 carves out the §10.7 proptest stubs; criterion 5 (`cargo bench`) is struck-through with a §0.A pointer; criterion 9 clarifies that determinism is enforced by the 10-run gate today, with proptest's fixed-seed convention added back when the deviation closes. | §12 |
| 16 | **§15 step 6 iter() ordering rewritten.** Original wording "deterministic via key sort" was ambiguous between "iter() sorts internally" and "callers sort." Replaced with explicit "API is nondeterministic; callers collect-and-sort by `CellCoordinate`" plus a code snippet. This was previously documented as a `CLAUDE.md` amendment (§2.11 there); now in the brief itself. | §15 step 6 |
| 17 | **§15 step 18 (benchmarks) marked DEFERRED** with a pointer to §0.A and instruction to leave a `BENCHES_DEFERRED.md` note in `crates/mc-core/`. | §15 step 18 |

When the toolchain blocker resolves (any of: rust-toolchain bumps to 1.85+,
criterion ships a release without the edition2024-requiring `clap_lex`, or
`mc-core` pins older known-good versions of criterion/proptest/insta), the
remediation is mechanical: revert each annotation, restore §2.3 as the
shipped form, fill in the proptest stubs, write the bench files, and
re-enable the bench gate in §12. None of these v1.2 edits change the
engine semantics, the test contract, or the Phase 1 scope — they describe
what's contractual *today* vs *post-deviation*.

---

*End of Phase 1 Rust Kernel Build Brief, v1.2.*
