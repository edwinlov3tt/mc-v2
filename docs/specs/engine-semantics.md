# MarketingCubes — Engine Semantics Spec

**Status:** Draft 1 — definitive for Phase 1
**Audience:** Engine implementers (Rust). This document is the source of truth for what the kernel means by every named concept.
**Out of scope:** model-backed cells, DuckDB integration, WASM bindings, CRDTs, schema marketplace, multi-user collaboration, LLM rules. These appear in later spec drafts.

---

## 0. How to read this document

Every concept below is defined in five fixed sections, in this order:

1. **Definition** — plain English. One paragraph.
2. **Rust shape** — concrete struct/enum/trait sketch. Not implementation; a typed contract.
3. **Invariants** — properties that must hold at all times. The kernel is responsible for upholding these. Violating any is a bug.
4. **Marketing-to-finance example** — concrete instance using a single canonical planning model (the "Acme demo cube"). Every concept reuses the same example so a reader can stitch them together.
5. **Failure modes** — what a sloppy implementation produces. These are guard-rails for the test suite.

The Acme demo cube, used end-to-end:

| Dimension | Members (leaves) | Hierarchy |
|---|---|---|
| Scenario | `Baseline`, `Aggressive`, `Conservative` | flat |
| Version | `Working`, `Submitted`, `Approved` | flat |
| Time | `Jan_2026`, `Feb_2026`, `Mar_2026` (etc.) | Month → Quarter → Year |
| Channel | `Paid_Search`, `Paid_Social`, `Display`, `Email`, `Organic` | Channel → Channel_Group |
| Market | `Tampa`, `Orlando`, `Miami`, `Atlanta`, `Charlotte` (etc.) | City → State → Region |
| Measure | `Spend`, `CPC`, `CVR`, `Close_Rate`, `AOV`, `COGS_Rate`, `Clicks`, `Leads`, `Customers`, `Revenue`, `Gross_Profit` | flat |

Marketing-to-finance flow encoded as rules:

```
Clicks       = Spend / CPC                 (per Channel × Market × Time)
Leads        = Clicks * CVR
Customers    = Leads * Close_Rate
Revenue      = Customers * AOV
Gross_Profit = Revenue * (1 - COGS_Rate)
```

Every example below references this cube. When a concept is dimension-agnostic, the example uses one of the rules above; when it's dimension-specific, it uses the Time, Channel, or Market hierarchy.

---

## 1. Cube

### 1.1 Definition

A **Cube** is a named, finite-dimensional sparse array of cells. It is the top-level container in the engine. A cube binds together a fixed list of dimensions, a measure dimension (which dictates the data type and rules of each cell), and a sparse store of cell values. A cube is the unit of dependency tracking, locking, snapshotting, and writeback authorization.

### 1.2 Rust shape

```rust
pub struct Cube {
    id: CubeId,                       // stable, unique within Workspace
    name: String,                     // human-readable, unique within Workspace
    dimensions: Vec<DimensionRef>,    // fixed at creation; order matters for storage
    measure_dimension: DimensionRef,  // exactly one; must be in `dimensions`
    rules: RuleSet,                   // see §10
    locks: LockTable,                 // see §18
    permissions: PermissionTable,     // see §17
    store: Box<dyn CellStore>,        // see §6
    revision: Revision,               // monotonic integer; bumps on every write
}

pub struct CubeId(u64);
pub struct Revision(u64);
```

### 1.3 Invariants

- **I-Cube-1.** `dimensions` is non-empty and contains no duplicates (by `DimensionId`).
- **I-Cube-2.** `measure_dimension` is one of the entries in `dimensions`.
- **I-Cube-3.** Dimension order is stable for the cube's lifetime. Reordering creates a new cube.
- **I-Cube-4.** Every cell coordinate stored, read, or written must reference exactly the dimensions in `dimensions`, in the same order.
- **I-Cube-5.** `revision` is strictly monotonic. Two consecutive successful writes produce two consecutive revisions.
- **I-Cube-6.** All rule cells in `rules` reference measures that exist in `measure_dimension`.

### 1.4 Acme demo example

```rust
let acme = Cube::new(CubeBuilder {
    name: "Acme_MarketingFinance".into(),
    dimensions: vec![
        scenario_dim, version_dim, time_dim, channel_dim, market_dim, measure_dim,
    ],
    measure_dimension: measure_dim,
    rules: RuleSet::from(vec![
        rule_clicks(),       // Clicks = Spend / CPC
        rule_leads(),        // Leads = Clicks * CVR
        rule_customers(),    // Customers = Leads * Close_Rate
        rule_revenue(),      // Revenue = Customers * AOV
        rule_gross_profit(), // Gross_Profit = Revenue * (1 - COGS_Rate)
    ]),
    ..Default::default()
});
```

A cell coordinate in Acme has 6 components: `(scenario, version, time, channel, market, measure)`. Every read or write specifies all six.

### 1.5 Failure modes

- **Dropping a dimension late.** A cube whose dimension list mutates produces silently incorrect coordinates: yesterday's `(s, v, t, c, m, M)` becomes today's `(s, v, t, c, M)` and the storage layer either stores it under a wrong key or panics.
- **Allowing duplicate cubes.** Two cubes with the same `name` lets writers race; only one wins, the other's writes vanish.
- **Mutable dimension order.** If the storage layout depends on dimension order and order changes, every existing cell becomes unreachable.
- **Forgetting `revision` bump.** Downstream caches, snapshots, and dirty-tracking all use `revision` as the freshness key. Skipping a bump silently leaks stale data.

---

## 2. Dimension

### 2.1 Definition

A **Dimension** is a named, ordered set of **Elements** (the leaves) plus zero or more **Hierarchies** that group those elements into trees. A dimension is the unit of indexing along one axis of a cube. Examples: `Time`, `Channel`, `Market`, `Measure`, `Scenario`, `Version`. A dimension is *not* a list of values for one cube — it is a shared, reusable catalog that may be referenced by many cubes.

### 2.2 Rust shape

```rust
pub struct Dimension {
    id: DimensionId,
    name: String,
    elements: Vec<Element>,           // ordered; index is element position
    element_index: HashMap<ElementId, usize>,
    hierarchies: Vec<Hierarchy>,      // see §3
    default_hierarchy: HierarchyId,   // exactly one; used when no hierarchy specified
    kind: DimensionKind,
    is_frozen: bool,                  // see §2.3 I-Dim-5
}

pub enum DimensionKind {
    Standard,                         // Time, Channel, Market — user-defined
    Measure,                          // Measure dimension — special: drives cell typing
    Scenario,                         // Scenario dimension — drives versioned reads
    Version,                          // Version dimension — drives published/draft state
}

pub struct DimensionId(u64);
```

### 2.3 Invariants

- **I-Dim-1.** `elements` is non-empty, has no duplicates by `ElementId`, and has stable insertion order.
- **I-Dim-2.** `element_index` is the inverse of `elements`: for every `e` at position `i`, `element_index[e.id] == i`.
- **I-Dim-3.** Every hierarchy in `hierarchies` references only `ElementId`s that exist in `elements`.
- **I-Dim-4.** `default_hierarchy` is one of the entries in `hierarchies`.
- **I-Dim-5.** Once a dimension has been bound to a cube (`is_frozen = true`), elements may be **appended** but never **removed or reordered**. Removing an element invalidates every existing cube cell that references it.
- **I-Dim-6.** A `DimensionKind::Measure` dimension is the only kind whose elements carry typing information for cells; other kinds are pure index dimensions.

### 2.4 Acme demo example

```rust
let time_dim = Dimension::new("Time", DimensionKind::Standard)
    .with_elements(vec![
        // Leaves only. Hierarchy adds Q1, Q2, FY2026 etc.
        Element::leaf("Jan_2026"),
        Element::leaf("Feb_2026"),
        Element::leaf("Mar_2026"),
        Element::leaf("Apr_2026"),
        // ...
    ])
    .with_hierarchy(time_calendar_hierarchy())
    .with_default_hierarchy("Calendar");
```

The `Measure` dimension is structurally the same but each `Element` carries dtype metadata (`f64`, `i64`, `bool`, `category`) and `is_input` vs `is_derived` flags so the rule engine knows which cells accept writes.

### 2.5 Failure modes

- **Reordering elements after freeze.** If `Tampa` was element 3 and is now element 7, every coordinate that stored it as the integer 3 now points to whatever is at position 3 (e.g., `Atlanta`). The cube silently corrupts. **The kernel must reject reordering after freeze.**
- **Letting two dimensions share an `ElementId`.** Element IDs that collide across dimensions make hierarchies ambiguous when cubes mix them. IDs are namespace-scoped to a single dimension.
- **Mutable name lookups.** Looking up `"Tampa"` by string at runtime in a hot path turns a `O(1)` index lookup into a `O(n)` string compare and a hashmap miss. Element resolution is by `ElementId`, with name lookups confined to definition time.
- **No `DimensionKind`.** Treating `Measure` like a plain dimension means cell typing has to live somewhere else; it ends up as a global registry that drifts from the schema.

---

## 3. Hierarchy

### 3.1 Definition

A **Hierarchy** is a tree (more precisely, a forest of trees — one per top-level rollup) defined over the elements of a single dimension. Each non-leaf node is a **consolidated element** whose value is computed by aggregating its children. Each edge from parent to child carries a numeric **weight** (typically `+1.0` for additive aggregation, `-1.0` for net-of, fractional for weighted averages). One dimension may have multiple hierarchies; only one is the default, but consumers can ask for a specific hierarchy by name.

### 3.2 Rust shape

```rust
pub struct Hierarchy {
    id: HierarchyId,
    name: String,                          // unique within parent Dimension
    dimension: DimensionId,
    edges: Vec<HierarchyEdge>,             // tree edges; immutable after freeze
    consolidated_elements: HashSet<ElementId>, // every parent across all edges
    aggregation: AggregationRule,
}

pub struct HierarchyEdge {
    parent: ElementId,
    child: ElementId,
    weight: f64,                            // see §3.3 I-Hier-3
}

pub enum AggregationRule {
    Sum,                                   // children weighted-summed (default)
    WeightedAverage { weight_measure: ElementId },  // weighted by another measure
    Min,
    Max,
    Custom { fn_id: AggregationFnId },     // for advanced rollups
}
```

### 3.3 Invariants

- **I-Hier-1.** The edge set forms a forest: every child has at most one parent; there are no cycles.
- **I-Hier-2.** Every `parent` and `child` ID in `edges` exists in the parent dimension's `elements`.
- **I-Hier-3.** Edge weights are finite (`f64`, not NaN, not infinite). Zero is allowed (a contributing-zero edge) but a hierarchy with all-zero outgoing weights for a parent must not exist — that parent has no contribution and should be removed or revisited.
- **I-Hier-4.** Leaves are elements with no outgoing edges where they are the parent. Roots are elements that appear only as parents, never as children.
- **I-Hier-5.** A consolidated element's value is *derived*. Direct writes to consolidated elements are rejected (see §13 Writeback).
- **I-Hier-6.** Every element in the dimension is either a leaf, a consolidated node, or both (an element can be a leaf in one hierarchy and a consolidated in another).
- **I-Hier-7.** The hierarchy is acyclic. Cycle detection happens at hierarchy-build time, not at consolidation time.

### 3.4 Acme demo example

The Time hierarchy:

```
FY2026 (root)
├── Q1_2026  (weight 1.0 → FY2026)
│   ├── Jan_2026  (weight 1.0 → Q1_2026)
│   ├── Feb_2026  (weight 1.0 → Q1_2026)
│   └── Mar_2026  (weight 1.0 → Q1_2026)
├── Q2_2026
│   ├── Apr_2026
│   ├── May_2026
│   └── Jun_2026
├── Q3_2026
└── Q4_2026
```

The Market hierarchy:

```
USA (root)
├── Southeast (region)
│   ├── Florida (state)
│   │   ├── Tampa
│   │   ├── Orlando
│   │   └── Miami
│   ├── Georgia
│   │   └── Atlanta
│   └── North_Carolina
│       └── Charlotte
└── Northeast
    ├── New_York_State
    │   └── New_York_City
    └── Massachusetts
        └── Boston
```

The Channel hierarchy:

```
All_Channels
├── Paid_Media
│   ├── Paid_Search
│   ├── Paid_Social
│   └── Display
└── Owned_Earned
    ├── Email
    └── Organic
```

The Measure dimension is flat — measures don't roll up. (Some engines model measure groups; we don't in v1.)

A read of Q1_2026 Spend in Florida × Paid_Media is a triple-consolidated cell. Its value walks all three hierarchies: it sums {Jan, Feb, Mar} × {Tampa, Orlando, Miami} × {Paid_Search, Paid_Social, Display} — 27 leaf cells in this slice — with all weights `1.0`.

### 3.5 Failure modes

- **Cycle in the tree.** A cycle (`A → B → A`) makes consolidation non-terminating. The kernel must run a cycle check at hierarchy-build time and reject. Cycle-at-runtime is the worst possible failure mode because it produces stack overflows in production reads.
- **Multiple parents.** If `Tampa → Florida` and `Tampa → SoutheastDirect` both exist, summing Florida and summing Southeast both include Tampa and `FY total = Florida + Northeast` double-counts Tampa. v1 rejects multi-parent hierarchies; alternate-hierarchy support handles this case explicitly.
- **Floating-point weight drift.** Storing `1/3` as a weight and summing 3 children produces `0.9999999...`. Documented behavior: weights are `f64` and the kernel does not silently round. If exact arithmetic matters, use a different aggregation rule.
- **Ambiguous default.** A dimension with no `default_hierarchy` and consumers that don't specify one produces an arbitrary choice. The default must be explicit at definition time.
- **Forgetting to update consolidations on leaf write.** If writing a leaf doesn't dirty its consolidated ancestors, every consolidated read returns stale data. See §15 Dependency.

---

## 4. Element

### 4.1 Definition

An **Element** is a single named member of a dimension. It is either a **leaf** (no children in any hierarchy) or **consolidated** (has children in at least one hierarchy). A leaf element is writable in the sense that cells coordinated on that element can accept input writes. A consolidated element is read-only: its cells are computed by walking its hierarchy.

### 4.2 Rust shape

```rust
pub struct Element {
    id: ElementId,
    name: String,                          // unique within Dimension
    dimension: DimensionId,
    attributes: HashMap<AttributeKey, AttributeValue>,  // user-defined metadata
    measure_meta: Option<MeasureMeta>,      // populated only for Measure dim
}

pub struct MeasureMeta {
    dtype: CellDataType,
    role: MeasureRole,                     // Input | Derived | Both
    aggregation: AggregationRule,           // overrides hierarchy default per measure
    format_hint: Option<FormatSpec>,        // currency, percent, count — display only
}

pub enum MeasureRole {
    Input,                                 // accepts writes; no rule attached
    Derived,                               // value comes from a Rule; rejects writes
    Both,                                  // input cells exist where no rule applies; rule fills the rest (advanced)
}

pub enum CellDataType {
    F64,
    I64,
    Bool,
    Category(Vec<String>),
}

pub struct ElementId(u64);
```

### 4.3 Invariants

- **I-Elem-1.** `name` is unique within the parent dimension.
- **I-Elem-2.** An element's `dimension` field matches the dimension that owns it.
- **I-Elem-3.** `measure_meta` is `Some(_)` if and only if the parent dimension has `kind == Measure`.
- **I-Elem-4.** A `Derived` measure must have at least one rule attached in some cube using this dimension; if no cube has such a rule, reads return undefined-cell errors instead of zeros.
- **I-Elem-5.** Element attributes are arbitrary key/value metadata; they do not affect computation. The kernel never branches on attribute values.

### 4.4 Acme demo example

```rust
let measure_dim = Dimension::new("Measure", DimensionKind::Measure)
    .with_elements(vec![
        // Inputs (writable)
        Element::measure("Spend",      F64, MeasureRole::Input,   AggregationRule::Sum),
        Element::measure("CPC",        F64, MeasureRole::Input,   AggregationRule::WeightedAverage { weight_measure: spend_id }),
        Element::measure("CVR",        F64, MeasureRole::Input,   AggregationRule::WeightedAverage { weight_measure: clicks_id }),
        Element::measure("Close_Rate", F64, MeasureRole::Input,   AggregationRule::WeightedAverage { weight_measure: leads_id }),
        Element::measure("AOV",        F64, MeasureRole::Input,   AggregationRule::WeightedAverage { weight_measure: customers_id }),
        Element::measure("COGS_Rate",  F64, MeasureRole::Input,   AggregationRule::WeightedAverage { weight_measure: revenue_id }),

        // Derived (read-only; computed from rules)
        Element::measure("Clicks",       F64, MeasureRole::Derived, AggregationRule::Sum),
        Element::measure("Leads",        F64, MeasureRole::Derived, AggregationRule::Sum),
        Element::measure("Customers",    F64, MeasureRole::Derived, AggregationRule::Sum),
        Element::measure("Revenue",      F64, MeasureRole::Derived, AggregationRule::Sum),
        Element::measure("Gross_Profit", F64, MeasureRole::Derived, AggregationRule::Sum),
    ]);
```

The aggregation rule is *per measure*, not per hierarchy: `Spend` sums across child Months; `CPC` weighted-averages across child Months using `Spend` as the weight. Without per-measure aggregation, rolling up a CPC by simple sum produces a meaningless number (sum of unit prices).

### 4.5 Failure modes

- **Strings as element identifiers in the hot path.** Comparing `"Tampa" == "tampa"` because of casing differences is a common bug. The kernel uses `ElementId` integers internally; string lookups are confined to the definition layer.
- **Treating all measures as additive.** Rolling up `CPC` with `Sum` gives a number that looks plausible but is wrong by an order of magnitude. The kernel must enforce that every measure declares its `AggregationRule` and respects it during consolidation.
- **Allowing writes to `Derived` measures.** A user writing 999 to a Revenue cell would silently overwrite the rule's output until the next dependency invalidation. The kernel rejects writes at the API boundary; see §13.
- **Mutating `dtype` after data exists.** Changing a measure's dtype from `F64` to `I64` either loses precision (silent rounding) or panics (NaN cast). The kernel rejects dtype changes on populated measures.

---

## 5. Measure

### 5.1 Definition

A **Measure** is one specific element of the special `Measure` dimension that carries the data type and role of a class of cells. "Spend," "Clicks," "Revenue," "Gross_Profit" are all measures. A measure is the *vertical* axis of a cube — it tells you what kind of number lives in a cell; the other dimensions tell you which slice. A measure also carries the aggregation rule used to roll up consolidated cells of that measure.

### 5.2 Rust shape

`Measure` is not a separate type from `Element`. It is an `Element` whose parent dimension has `DimensionKind::Measure` and whose `measure_meta` is `Some(_)`. The kernel exposes a typed view:

```rust
pub struct MeasureRef<'cube> {
    element: &'cube Element,
}

impl<'cube> MeasureRef<'cube> {
    pub fn id(&self) -> ElementId { ... }
    pub fn name(&self) -> &str { ... }
    pub fn dtype(&self) -> CellDataType { ... }
    pub fn role(&self) -> MeasureRole { ... }
    pub fn aggregation(&self) -> &AggregationRule { ... }
}
```

### 5.3 Invariants

- **I-Meas-1.** Every measure belongs to exactly one dimension and that dimension has `kind == Measure`.
- **I-Meas-2.** A cube has exactly one measure dimension. Multiple-measure-dimension cubes are not supported in v1.
- **I-Meas-3.** A measure's role (`Input`/`Derived`/`Both`) determines which writes the cube accepts at coordinates referencing this measure.
- **I-Meas-4.** A measure's aggregation rule is honored during every consolidation read. Two reads of the same consolidated coordinate with different requested aggregation rules return different values; the rule is part of the read query.

### 5.4 Acme demo example

`Spend` is an `Input` `F64` measure with `Sum` aggregation. `CPC` is an `Input` `F64` measure with weighted-average aggregation (weighted by `Spend`). `Revenue` is a `Derived` `F64` measure with `Sum` aggregation, computed by the `rule_revenue` rule attached at cube-build time.

### 5.5 Failure modes

- **Storing measures as a separate registry.** Measures live as elements of a dimension. A separate global registry drifts from the dimension's element list and produces "measure exists but isn't in the dim" errors.
- **Confusing role with rule presence.** A `Derived` measure without an attached rule is a definition bug, not a runtime fallback. The kernel detects this at cube-build time and refuses to construct the cube.
- **Mismatched aggregation between hierarchy and measure.** If the Channel hierarchy has `AggregationRule::Sum` but the `CPC` measure declares weighted average, the cube must use the measure's rule and ignore the hierarchy's. v1 documents this precedence: measure aggregation always wins.

---

## 6. CellCoordinate

### 6.1 Definition

A **CellCoordinate** is a fully-qualified address into a cube. It binds one element from each of the cube's dimensions in the same order as the cube's dimension list. A coordinate may resolve to a leaf cell (all elements are leaves in the addressed hierarchies) or a consolidated cell (at least one element is non-leaf). Coordinates are dense fixed-size vectors of `ElementId`s — *not* string maps — so equality, hashing, and storage indexing are O(D) where D is the number of dimensions.

### 6.2 Rust shape

```rust
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CellCoordinate {
    cube: CubeId,                          // disambiguates between cubes
    elements: SmallVec<[ElementId; 8]>,    // length == cube.dimensions.len()
}

pub struct CellCoordinateBuilder {
    cube: CubeId,
    slots: Vec<Option<ElementId>>,         // one slot per dimension
}

impl CellCoordinateBuilder {
    pub fn set(&mut self, dim: DimensionId, element: ElementId) -> Result<()> { ... }
    pub fn build(self) -> Result<CellCoordinate> { ... }   // err if any slot None
}
```

### 6.3 Invariants

- **I-Coord-1.** Length of `elements` matches the cube's dimension count.
- **I-Coord-2.** `elements[i]` is a valid `ElementId` in `cube.dimensions[i]`.
- **I-Coord-3.** Coordinate equality is deep equality of the `ElementId` slice.
- **I-Coord-4.** Hashing is deterministic and depends only on the slice contents (not allocation address).
- **I-Coord-5.** A coordinate with at least one consolidated element resolves to a derived value; its cell cannot be written directly. See §13.
- **I-Coord-6.** Coordinates from different cubes are never equal, even if they happen to have the same element list.

### 6.4 Acme demo example

```rust
// Tampa, Paid Search, March 2026, Working version, Baseline scenario, Spend measure
let coord = CellCoordinateBuilder::new(acme.id())
    .set(scenario_dim_id, baseline_id)
    .set(version_dim_id, working_id)
    .set(time_dim_id, mar_2026_id)
    .set(channel_dim_id, paid_search_id)
    .set(market_dim_id, tampa_id)
    .set(measure_dim_id, spend_id)
    .build()?;
```

A consolidated coordinate (Florida, Q1, Paid_Media, Spend) has the same shape but with consolidated `ElementId`s in three positions:

```rust
let consolidated = CellCoordinateBuilder::new(acme.id())
    .set(scenario_dim_id, baseline_id)
    .set(version_dim_id, working_id)
    .set(time_dim_id, q1_2026_id)        // consolidated
    .set(channel_dim_id, paid_media_id)  // consolidated
    .set(market_dim_id, florida_id)      // consolidated
    .set(measure_dim_id, spend_id)
    .build()?;
```

Reading `consolidated` triggers a hierarchy walk: 3 months × 3 markets (Tampa/Orlando/Miami) × 3 channels (Paid_Search/Paid_Social/Display) = 27 leaf reads, summed with weight 1.0 each.

### 6.5 Failure modes

- **String maps as coordinates.** A `HashMap<String, String>` coordinate is 10x slower than a `SmallVec<ElementId>` for equality, hashing, and storage. At cube scale this is the difference between a sub-second recalc and a multi-second one.
- **Partial coordinates.** Allowing `(scenario, time, market, measure)` (missing channel) makes the storage layer guess. v1 rejects partial coordinates; reads/writes always specify all dimensions. Slices (§12) are the supported way to read multiple cells.
- **Dimension order ambiguity.** If two coordinates with the same element set but different dimension orders compare equal, the storage layer's invariants break. Coordinates are tied to a specific dimension order via the parent cube.
- **Cross-cube collisions.** A coordinate from cube A applied to cube B silently reads/writes whatever the dimensions happen to align with. Coordinates carry their `CubeId` to prevent this.

---

## 7. CellValue

### 7.1 Definition

A **CellValue** is what a read returns for a single cell. It is more than a raw number: it carries the value, the value's type, where it came from (input, rule output, consolidation), and *optionally* uncertainty and trace metadata. The optional fields are how the same type serves both deterministic finance cells (no uncertainty needed) and probabilistic model cells (uncertainty required) without forcing one to fake the other.

**This shape is a deliberate correction from earlier drafts where every cell carried `(point, std)`.** Deterministic cells must not be forced to produce fake `std=0` values.

### 7.2 Rust shape

```rust
pub struct CellValue {
    value: ScalarValue,                    // the actual datum
    dtype: CellDataType,                   // matches the measure's dtype
    provenance: Provenance,                // how this value was produced
    uncertainty: Option<Uncertainty>,      // only for cells that compute it
    trace: Option<Trace>,                  // only when explicitly requested
    revision: Revision,                    // cube revision when this value was computed
}

pub enum ScalarValue {
    F64(f64),
    I64(i64),
    Bool(bool),
    Category(usize),                       // index into Element's category vec
    Null,                                  // explicit "no value" — distinct from 0
}

pub enum Provenance {
    Input { written_at: Timestamp, written_by: PrincipalId },
    Rule { rule_id: RuleId, computed_at: Revision },
    Consolidation {
        // A triple-consolidated cell (e.g., Q1 × Paid_Media × Florida) walks
        // up to three hierarchies simultaneously, so this is a small list.
        // SmallVec to keep the common single-hierarchy case allocation-free.
        hierarchies: SmallVec<[HierarchyId; 4]>,
        child_count: u32,
    },
    Default { source: DefaultSource },     // dimension-default fallback
}

pub enum Uncertainty {
    StdDev(f64),                           // 1-sigma; assume Gaussian
    Interval { low: f64, high: f64, confidence: f64 },  // arbitrary CI
    Distribution(DistributionRef),         // reference to a stored distribution
}

pub struct DistributionRef(u64);           // points into a distribution registry; Phase 4+
```

### 7.3 Invariants

- **I-CellValue-1.** `dtype` matches the measure's declared dtype. A cell of an `F64` measure cannot return `ScalarValue::Bool`.
- **I-CellValue-2.** `provenance` is always populated; there is no "unknown source" cell.
- **I-CellValue-3.** A `Provenance::Input` cell has `uncertainty == None` unless the user explicitly attached one. The kernel never injects fake `std=0`.
- **I-CellValue-4.** A `Provenance::Rule` cell's `uncertainty` is populated if and only if the rule's expression carries uncertainty (Phase 4+); for v1 deterministic rules, it is `None`.
- **I-CellValue-5.** `revision` reflects the cube state at compute time. Two reads of the same coordinate with the same revision return identical CellValues.
- **I-CellValue-6.** `trace` is `Some(_)` only when the read explicitly requested a trace (the engine does not produce traces speculatively because they're expensive). See §14.
- **I-CellValue-7.** `ScalarValue::Null` is a first-class value distinct from numeric zero. `Sum(Null, 5.0) = 5.0` (Null is identity for Sum); `Multiply(Null, 5.0) = Null` (Null poisons multiplication unless the rule explicitly handles it).

### 7.4 Acme demo example

A read of `Spend` for `(Baseline, Working, Mar_2026, Paid_Search, Tampa, Spend)`:

```rust
CellValue {
    value: ScalarValue::F64(50_000.0),
    dtype: CellDataType::F64,
    provenance: Provenance::Input {
        written_at: ts("2026-03-15T14:23:00Z"),
        written_by: principal("edwin"),
    },
    uncertainty: None,
    trace: None,
    revision: Revision(3142),
}
```

A read of `Revenue` for the same coordinate (`Revenue` is derived):

```rust
CellValue {
    value: ScalarValue::F64(125_000.0),
    dtype: CellDataType::F64,
    provenance: Provenance::Rule {
        rule_id: rule_revenue_id,
        computed_at: Revision(3142),
    },
    uncertainty: None,                     // deterministic rule
    trace: None,
    revision: Revision(3142),
}
```

A read of `Spend` rolled up to `Florida × Q1 × Paid_Media`:

```rust
CellValue {
    value: ScalarValue::F64(1_350_000.0),
    dtype: CellDataType::F64,
    provenance: Provenance::Consolidation {
        hierarchies: smallvec![time_calendar_id, channel_grouping_id, market_geo_id],
        child_count: 27,
    },
    uncertainty: None,
    trace: None,
    revision: Revision(3142),
}
```

### 7.5 Failure modes

- **Forcing `std=0` on every input.** As discussed at length: this pollutes deterministic finance cells with fake precision and adds noise to every interface that consumes the value.
- **Returning a bare `f64` instead of `CellValue`.** Callers can't distinguish "Spend was 0 because we wrote 0" from "Spend was missing and the engine defaulted." Provenance is never recoverable downstream.
- **Allowing a measure's stored dtype to drift from `CellValue.dtype`.** If `Revenue` is declared `F64` but a buggy rule returns `I64`, the consumer's serializer panics or silently truncates. Type checking happens at the cell boundary.
- **Eager trace generation.** Producing a trace on every read makes a 100K-cell read 100x slower. Traces are opt-in.
- **Ignoring `Null`.** Treating Null as zero in some rules and as missing-data in others is the most common silent-corruption bug in planning systems. The engine's rule evaluator must define Null semantics per operator and stick to them.

---

## 8. Scenario

### 8.1 Definition

A **Scenario** is one element of the special `Scenario` dimension and represents an alternative set of input assumptions that share the same structure as the baseline plan. Scenarios are the most common form of "what if" in planning: `Baseline` vs `Aggressive` vs `Conservative` are three scenarios that share dimensions, hierarchies, measures, and rules, but differ in their input values. A scenario is *not* a copy of a cube; it is an axis along which inputs vary.

### 8.2 Rust shape

`Scenario` is not a separate type. It is an element of a `DimensionKind::Scenario` dimension. Its semantics live in how the cube engine treats reads and writes that specify a scenario coordinate:

```rust
pub struct ScenarioMeta {
    is_default: bool,                      // exactly one scenario per cube is default
    derives_from: Option<ElementId>,       // optional inheritance: scenario inherits inputs from another
    description: String,
}
```

Scenario inheritance (Phase 2+) lets `Aggressive` reuse `Baseline`'s inputs except where it explicitly overrides them. Phase 1 has no inheritance: every scenario is independent.

### 8.3 Invariants

- **I-Scen-1.** A cube has exactly one `DimensionKind::Scenario` dimension or zero (no-scenario cube). Two scenario dimensions in one cube is undefined.
- **I-Scen-2.** Exactly one scenario element is marked `is_default`.
- **I-Scen-3.** Reads that don't specify a scenario use the default scenario.
- **I-Scen-4.** Writes always specify a scenario explicitly (no implicit default-write); this prevents accidental cross-scenario writes.
- **I-Scen-5.** Scenarios are evaluated independently. A change to `Baseline` Spend does not affect `Aggressive` Revenue unless `Aggressive` inherits `Baseline` (Phase 2+).

### 8.4 Acme demo example

```rust
let scenario_dim = Dimension::new("Scenario", DimensionKind::Scenario)
    .with_elements(vec![
        Element::scenario("Baseline",     ScenarioMeta { is_default: true,  ..default() }),
        Element::scenario("Aggressive",   ScenarioMeta { is_default: false, ..default() }),
        Element::scenario("Conservative", ScenarioMeta { is_default: false, ..default() }),
    ]);
```

Three scenarios. Each holds its own Spend/CPC/CVR inputs across all (Time × Channel × Market) leaves. The same rules run against each scenario's inputs. Reading `Revenue` for `(Baseline, ...)` and `(Aggressive, ...)` produces two different numbers because their input Spend values differ.

### 8.5 Failure modes

- **Scenario as a cube property instead of a dimension.** Some legacy planning tools store the scenario as cube-level metadata. This means side-by-side comparison of scenarios requires loading two cubes; rules that reference "this cell across scenarios" are impossible. Modeling scenario as a dimension is non-negotiable.
- **No default scenario.** A read with no scenario specified falls through to undefined behavior — either an error, or the first scenario alphabetically, or whatever the storage layer returns. Make the default explicit.
- **Implicit cross-scenario writes.** A write API that defaults to "current scenario" silently corrupts the wrong scenario's inputs when the user's mental context drifts. Require explicit scenario in every write.
- **Scenario inheritance leaking writes.** If `Aggressive` inherits from `Baseline` and the user writes to `Aggressive.Spend.Mar_2026`, that write must land in `Aggressive`'s overlay, not in `Baseline`. Inheritance is read-time, not write-time. (This is a Phase 2+ concern but worth flagging now.)

---

## 9. Version

### 9.1 Definition

A **Version** is one element of the special `Version` dimension and represents a workflow state of the plan: `Working`, `Submitted`, `Approved`, `Locked`, `Archived`. Whereas a Scenario is "what if X is true," a Version is "this is the snapshot we're agreeing to right now." Versions interact with locks: once a version is `Approved`, its cells become read-only; the only way to change them is to create a new version.

### 9.2 Rust shape

```rust
pub struct VersionMeta {
    state: VersionState,
    parent: Option<ElementId>,             // version this one was forked from
    created_at: Timestamp,
    locked_at: Option<Timestamp>,
    locked_by: Option<PrincipalId>,
}

pub enum VersionState {
    Draft,                                 // freely editable; the everyday working version
    Submitted,                             // editing locked except by approvers
    Approved,                              // fully locked; read-only
    Archived,                              // not visible in default reads but preserved
}
```

### 9.3 Invariants

- **I-Ver-1.** A cube has exactly one `DimensionKind::Version` dimension or zero (no-version cube; all data is implicitly in one anonymous version).
- **I-Ver-2.** A version's `state` transitions follow a fixed lattice: `Draft → Submitted → Approved`; `Approved → Archived` (one-way, archival is permanent unless a fresh fork is made). Backwards transitions are not allowed; reverting requires forking.
- **I-Ver-3.** Cells in an `Approved` or `Archived` version are read-only. Writes are rejected.
- **I-Ver-4.** Forking a version creates a new version element whose `parent` points to the source. Cells in the new version copy-on-write from the parent (Phase 3+ persistence concern).
- **I-Ver-5.** Version state changes are logged in the audit trail with the principal who performed the change (see §17).
- **I-Ver-6.** A read with no version specified uses the most-recent `Draft` (configurable per cube).

### 9.4 Acme demo example

The Version dimension holds workflow states:

```rust
let version_dim = Dimension::new("Version", DimensionKind::Version)
    .with_elements(vec![
        Element::version("Working",   VersionState::Draft),
        Element::version("Submitted", VersionState::Submitted),
        Element::version("Approved",  VersionState::Approved),
    ]);
```

A typical lifecycle: planners edit `(Working, ...)` cells freely; on Friday the FP&A team submits `Working → Submitted`; on Monday the CFO approves `Submitted → Approved`. After approval, no one can edit Approved; if a number needs to change, a new `Working_v2` fork is made.

### 9.5 Failure modes

- **Versions as commit messages.** Some tools encode version into a string column ("Q1_v3_final_FINAL"). Without a structured state machine, "is this approved" becomes a string parse. Model version as a dimension element with explicit state.
- **Allowing edits to approved versions.** If `Approved` cells are silently editable when the user has admin rights, the audit trail lies. The kernel rejects writes to Approved at the API boundary; admin overrides go through a different (logged) path.
- **No fork-on-edit semantics.** "User edits an approved cell" should either produce an error or auto-fork to a new draft. Both are valid product choices but the engine must do exactly one consistently.
- **Conflating Version and Scenario.** "Working_Aggressive" as one version conflates two orthogonal axes. Scenarios are alternative inputs; versions are workflow states. Keep them separate.

---

## 10. Rule

### 10.1 Definition

A **Rule** is a typed expression tree (or compiled function) that computes the value of a `Derived` measure for some subset of coordinates. A rule has a target (which measure it computes), a scope (which coordinates it applies to), an expression body (the formula), and an explicit list of dependencies (which other cells the rule reads from). Rules in v1 are *not* parsed from strings — they are constructed via a typed builder API. A YAML representation can be derived but the AST is the source of truth.

### 10.2 Rust shape

```rust
pub struct Rule {
    id: RuleId,
    cube: CubeId,
    target_measure: ElementId,             // a Derived measure in cube.measure_dimension
    scope: Scope,                          // which coordinates this rule applies to
    body: Expr,                            // the expression tree
    declared_dependencies: Vec<DependencyDecl>,  // explicit; validated at runtime
    priority: i32,                          // for overlapping scopes
}

pub enum Expr {
    // Leaves
    Const(ScalarValue),
    CellRef(CellRefSpec),                  // reference to another cube cell
    SelfRef(ElementOffset),                // "the same coordinate but a different measure"
    Param(ParamRef),                       // rule-time parameter (e.g., a constant from a slice)

    // Arithmetic
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),

    // Conditional / null-handling
    IfNull(Box<Expr>, Box<Expr>),          // (value, fallback)
    IfElse(Box<Expr>, Box<Expr>, Box<Expr>), // (cond, then, else)

    // Aggregation (within a slice; rare in v1)
    SumOver(DimensionId, Box<Expr>),       // dynamic aggregation, used in advanced rules
}

pub struct CellRefSpec {
    cube: CubeId,                          // for cross-cube references (Phase 5+)
    fixed: HashMap<DimensionId, ElementId>, // dimensions pinned to specific elements
    follows_self: HashSet<DimensionId>,     // dimensions that mirror the target coordinate
}

pub struct DependencyDecl {
    cube: CubeId,
    measure: ElementId,
    coordinate_pattern: CoordPattern,      // see §15
}
```

### 10.3 Invariants

- **I-Rule-1.** `target_measure` is a `MeasureRole::Derived` measure in the rule's cube.
- **I-Rule-2.** `scope` matches at least one valid coordinate in the cube. A rule whose scope matches nothing is a definition error.
- **I-Rule-3.** `body` is well-typed: every `Expr` node's output type matches what its parent expects, and the root's output type matches `target_measure.dtype`.
- **I-Rule-4.** `declared_dependencies` is a complete superset of the cells the rule actually reads. The kernel validates this with full-scan dependency tracing during evaluation; missing declarations produce errors (see §15 Dependency / §22 Correctness).
- **I-Rule-5.** Rules cannot create cycles. A rule whose target appears in its own dependency closure is rejected at definition time (see §15 invariants).
- **I-Rule-6.** Two rules with overlapping scope and the same priority is a definition error. Disambiguate by priority or non-overlap.
- **I-Rule-7.** Rules are deterministic: the same inputs at the same revision produce the same output, byte-for-byte. (Stochastic operations live in model cells, Phase 4+.)

### 10.4 Acme demo example

The Revenue rule:

```rust
let rule_revenue = Rule {
    id: RuleId(...),
    cube: acme.id(),
    target_measure: revenue_id,
    scope: Scope::AllLeaves {
        // applies to every (scenario, version, time-leaf, channel-leaf, market-leaf, Revenue) cell
    },
    body: Expr::Mul(
        Box::new(Expr::SelfRef(ElementOffset::Measure(customers_id))),
        Box::new(Expr::SelfRef(ElementOffset::Measure(aov_id))),
    ),
    declared_dependencies: vec![
        DependencyDecl {
            cube: acme.id(),
            measure: customers_id,
            coordinate_pattern: CoordPattern::SameAsTarget,
        },
        DependencyDecl {
            cube: acme.id(),
            measure: aov_id,
            coordinate_pattern: CoordPattern::SameAsTarget,
        },
    ],
    priority: 0,
};
```

Reading the value at `(Baseline, Working, Mar_2026, Paid_Search, Tampa, Revenue)`:

1. Engine looks up rule for `Revenue` measure.
2. Engine evaluates `body`: `Mul(Customers, AOV)`.
3. `SelfRef(Customers)` resolves to `(Baseline, Working, Mar_2026, Paid_Search, Tampa, Customers)`.
4. That cell is itself derived (`Customers = Leads * Close_Rate`). The engine recursively evaluates.
5. Eventually all references resolve to `Input` cells (Spend, CPC, CVR, Close_Rate, AOV).
6. The arithmetic flows back up, producing the final Revenue value.

### 10.5 Failure modes

- **String-parsed rules.** "Revenue = Customers * AOV" parsed at runtime is the standard approach (Excel, TM1) but introduces a parser, a type system, error messages, and surprises (`""` vs `null`, locale-dependent decimals). v1 punts: rules are constructed in code via a typed builder. A string-based DSL can be added in v3.
- **Missing dependency declarations.** A rule that reads a cell without declaring it produces correct output but fails to invalidate when that cell changes. Stale numbers everywhere. The kernel must full-scan-validate declared deps against actually-read deps and error on mismatch.
- **Cycles.** `A = B + 1`, `B = A + 1` is a non-terminating evaluation. Cycle detection at definition time, not at evaluation time.
- **Hidden aggregation.** A rule whose body silently aggregates over a dimension that wasn't declared is non-deterministic at the typing level. Aggregation operators (`SumOver`, `AvgOver`) are explicit in the AST.
- **Undefined `Null` behavior.** `Mul(5, Null)`: is it 0, Null, or an error? Pick one (we pick Null-poisoning) and apply it everywhere. The most-cited surprise in spreadsheet logic is inconsistent null handling.
- **Letting two rules write the same cell.** Conflict resolution by "last rule registered wins" is a recipe for invisible bugs. Rules with overlapping scope must have explicit priority.

---

## 11. Consolidation

### 11.1 Definition

**Consolidation** is the operation of computing the value of a cell whose coordinate includes one or more consolidated (non-leaf) elements. The kernel walks the relevant hierarchies, gathers the contributing leaf cells, applies each leaf's measure-specific aggregation rule, and combines the results. Consolidation is *implicit* — there is no `consolidate()` API call. Reads of consolidated coordinates trigger consolidation under the hood; writes to consolidated coordinates are rejected.

### 11.2 Rust shape

Consolidation is a behavior, not a struct. It manifests in the read path:

```rust
pub trait ConsolidationStrategy {
    fn consolidate(
        &self,
        cube: &Cube,
        coord: &CellCoordinate,
        measure: &MeasureRef,
        snapshot: &Snapshot,                // see §19
    ) -> Result<CellValue>;
}

pub struct StandardConsolidation;          // v1 default

impl ConsolidationStrategy for StandardConsolidation {
    fn consolidate(...) -> Result<CellValue> {
        // 1. For each consolidated dimension in coord, expand to leaf descendants.
        // 2. Cartesian-product the leaf descendants across all consolidated dims.
        // 3. For each leaf coordinate, recursively read the cell.
        // 4. Apply measure.aggregation to combine, using hierarchy edge weights.
        // 5. Return a CellValue with Provenance::Consolidation.
    }
}
```

### 11.3 Invariants

- **I-Cons-1.** Consolidation respects the measure's `AggregationRule`, not the hierarchy's default. (Per §5 I-Meas-4.)
- **I-Cons-2.** Consolidation respects edge weights. A weight-2 edge contributes 2× the leaf's value to the parent.
- **I-Cons-3.** Consolidation handles `Null` according to the aggregation rule's null policy. For `Sum`, Null contributes 0; for `WeightedAverage`, Null contributes nothing to numerator or denominator (excluded). For `Min`/`Max`, Null is excluded.
- **I-Cons-4.** A coordinate with consolidated elements in multiple dimensions multiplies the leaf set: 3 quarter-months × 3 cities × 3 channels = 27 leaves.
- **I-Cons-5.** Consolidation is *cached* per (coordinate, revision). Re-reading the same consolidated cell in the same revision returns the cached value without re-walking the hierarchy.
- **I-Cons-6.** When a leaf cell is dirtied, every consolidated cell that includes it is also dirtied. The dirty propagation walks the hierarchies upward.
- **I-Cons-7.** Consolidation cells are always derived; writes to them are rejected with `WritebackError::ConsolidatedCellNotWritable`.

### 11.4 Acme demo example

Reading `(Baseline, Working, Q1_2026, Paid_Media, Florida, Spend)`:

1. The cube identifies three consolidated elements: `Q1_2026` (Time), `Paid_Media` (Channel), `Florida` (Market).
2. It expands each: `Q1_2026 → {Jan_2026, Feb_2026, Mar_2026}`, `Paid_Media → {Paid_Search, Paid_Social, Display}`, `Florida → {Tampa, Orlando, Miami}`.
3. Cartesian product: 3 × 3 × 3 = 27 leaf coordinates.
4. For each leaf, read the `Spend` cell. Most are `Input` cells; missing values are `Null`.
5. Apply `Spend`'s aggregation rule: `Sum`. Null contributes 0.
6. Multiply each leaf by its edge-weight product (here all 1.0, so no scaling).
7. Return the sum as a `CellValue` with `Provenance::Consolidation { hierarchy: ..., child_count: 27 }`.

For `(Baseline, Working, Q1_2026, Paid_Media, Florida, CPC)`, step 5 changes: `CPC`'s rule is `WeightedAverage { weight_measure: spend_id }`. The kernel reads each leaf's `Spend` (the weight) and `CPC` simultaneously, computes `Σ(CPC_i × Spend_i) / Σ(Spend_i)`. Different math, same code path.

### 11.5 Failure modes

- **Sum-everything rollups.** Treating every measure as additive at consolidation time produces nonsense for prices, ratios, and rates. Per-measure aggregation rules are non-negotiable.
- **Consolidating before recompute.** If a leaf has a stale value and consolidation reads it, the parent is stale too. Dirty propagation must run before any consolidation that includes the dirty leaf, or the cache is poisoned.
- **Forgetting weights.** Edge weights default to 1.0 but a weighted-average hierarchy (e.g., 30/70 split between two markets) needs weights, and dropping them produces uniform averages that are silently wrong.
- **Null-ignoring vs null-zeroing inconsistency.** If `Sum` treats Null as 0 and `Average` treats Null as missing, two different reads of the same data report different "sample sizes." Document and test the per-rule null policy.
- **Unbounded consolidation.** A read of `(All_Scenarios, All_Versions, FY2026, All_Channels, USA, Spend)` could expand to millions of leaves. The kernel must handle this either by enforcing scope limits, by lazy iteration, or by accelerated aggregation paths (Phase 2+). v1 tolerates the naive walk for cube sizes ≤ 1M cells.

---

## 12. Slice

### 12.1 Definition

A **Slice** is a multi-cell read that returns a region of a cube as a structured collection. Where a cell read returns one `CellValue`, a slice read returns many — one per coordinate in the slice. A slice is defined by binding *some* dimensions to specific elements (or sets of elements) and leaving *others* free. The free dimensions enumerate the cells in the result. Slices are how the engine answers "give me Spend × Channel × Time for Florida in Baseline" without forcing the caller to assemble per-cell reads.

### 12.2 Rust shape

```rust
pub struct SliceQuery {
    cube: CubeId,
    bindings: HashMap<DimensionId, SliceBinding>,
    snapshot: Option<Revision>,            // None = current; Some = historical
    request_trace: bool,                   // attach traces to each cell
    request_uncertainty: bool,             // include uncertainty if available
}

pub enum SliceBinding {
    One(ElementId),                        // pin to a single element
    Many(Vec<ElementId>),                  // pin to a specific list
    Subtree(ElementId),                    // all descendants of this consolidated element
    All,                                   // every leaf in the dimension
    AllConsolidated,                       // every non-leaf in the dimension
}

pub struct SliceResult {
    coords: Vec<CellCoordinate>,
    values: Vec<CellValue>,                // same length as coords
    revision: Revision,
}
```

### 12.3 Invariants

- **I-Slice-1.** Every dimension in the cube has a binding; partial slices are not supported. A "wildcard" is `SliceBinding::All`.
- **I-Slice-2.** The cardinality of a slice is the product of the cardinalities of its bindings. The kernel rejects slices that exceed a configurable limit (default: 10M cells per slice in v1).
- **I-Slice-3.** Slice results are deterministic: the same query at the same revision returns the same coordinates in the same order.
- **I-Slice-4.** A slice with `request_trace: true` returns a trace per cell — expensive but allowed for audit reads.
- **I-Slice-5.** Slices honor permissions: cells the principal cannot read are omitted (or returned as `RestrictedCell` markers — TBD per Phase 1 review).
- **I-Slice-6.** A slice that crosses a snapshot boundary uses the requested snapshot for *every* cell, never mixing live and historical reads.

### 12.4 Acme demo example

"Show me Spend by Channel and Time for Florida, Baseline, Working":

```rust
let slice = SliceQuery {
    cube: acme.id(),
    bindings: hashmap! {
        scenario_dim_id => SliceBinding::One(baseline_id),
        version_dim_id => SliceBinding::One(working_id),
        time_dim_id => SliceBinding::All,           // all months as leaves
        channel_dim_id => SliceBinding::All,         // all channels as leaves
        market_dim_id => SliceBinding::Subtree(florida_id),  // Tampa, Orlando, Miami
        measure_dim_id => SliceBinding::One(spend_id),
    },
    snapshot: None,
    request_trace: false,
    request_uncertainty: false,
};

let result = cube.slice(&slice)?;
// result.coords.len() == 12 months × 5 channels × 3 cities = 180 cells
```

A second slice reads quarterly Revenue at the consolidated level:

```rust
SliceQuery {
    bindings: hashmap! {
        ...
        time_dim_id => SliceBinding::Many(vec![q1_id, q2_id, q3_id, q4_id]),
        channel_dim_id => SliceBinding::Many(vec![paid_media_id, owned_earned_id]),
        market_dim_id => SliceBinding::One(usa_id),                // top-level rollup
        measure_dim_id => SliceBinding::One(revenue_id),
        ...
    },
    ...
}
// 4 quarters × 2 channel groups × 1 market × 1 measure = 8 cells, each consolidated
```

### 12.5 Failure modes

- **Implicit dimension wildcarding.** A slice query that omits a dimension and "implicitly slices everything" produces unexpected scale: the user wanted 180 cells, got 18 million. Require explicit binding.
- **Mixing snapshots.** If a slice's first 100 cells came from revision N and the next 100 from revision N+1 (because a write happened mid-slice), the results lie. Slices must take a coherent snapshot.
- **Unbounded result sets.** A slice of a billion-cell cube returned as a `Vec<CellValue>` runs the host out of memory. Slices either iterate (Phase 2+ streaming) or cap at a configurable limit and require pagination.
- **Permission leakage in error messages.** Returning "you can't read 5 cells in this slice" with cell coordinates leaks data about which cells exist. The slice either redacts silently or returns a structured permission error.

---

## 13. Writeback

### 13.1 Definition

**Writeback** is the act of changing the value of a cell. Writes target *input cells only* — cells whose coordinate references a leaf in every consolidated dimension *and* whose measure is `Input` or `Both` (in input mode). Writes to derived cells (rule outputs, consolidations, model cells) are rejected. Writeback is the surface where authorization, locking, version state, and dependency invalidation all converge. Every write is atomic with respect to readers and produces exactly one revision bump.

### 13.2 Rust shape

```rust
pub struct WritebackRequest {
    coord: CellCoordinate,
    new_value: ScalarValue,
    principal: PrincipalId,                // who is writing
    intent: WriteIntent,
    expected_revision: Option<Revision>,    // for optimistic concurrency
}

pub enum WriteIntent {
    Set,                                   // overwrite
    Increment,                             // add to current
    Clear,                                 // set to Null
}

pub struct WritebackResult {
    coord: CellCoordinate,
    old_value: Option<CellValue>,
    new_value: CellValue,
    revision_before: Revision,
    revision_after: Revision,
    invalidated: Vec<CellCoordinate>,      // cells dirtied by this write
}

pub enum WritebackError {
    DerivedCellNotWritable { coord: CellCoordinate, source: Provenance },
    ConsolidatedCellNotWritable { coord: CellCoordinate },
    LockedCell { coord: CellCoordinate, lock: LockState },
    LockedVersion { version: ElementId, state: VersionState },
    InsufficientPermission { coord: CellCoordinate, principal: PrincipalId },
    TypeMismatch { expected: CellDataType, got: ScalarValue },
    StaleRevision { expected: Revision, actual: Revision },
    UnknownCoordinate { reason: String },
}
```

### 13.3 Invariants

- **I-WB-1.** Writes to coordinates with at least one consolidated element are rejected (`ConsolidatedCellNotWritable`).
- **I-WB-2.** Writes to coordinates whose measure is `MeasureRole::Derived` are rejected (`DerivedCellNotWritable`).
- **I-WB-3.** Writes to coordinates in an `Approved` or `Archived` version are rejected (`LockedVersion`).
- **I-WB-4.** Writes to locked cells (see §18) are rejected unless the principal owns the lock.
- **I-WB-5.** Writes are checked against the principal's permission scope (see §17). Insufficient permission is rejected; the cell value is unchanged.
- **I-WB-6.** A successful write is atomic: the revision bumps once, the new value becomes visible, and downstream cells become dirty in the same transition. Readers see either the pre-write or post-write state, never a partial state.
- **I-WB-7.** A successful write returns the list of invalidated coordinates so callers can pre-warm caches if they care.
- **I-WB-8.** `expected_revision` (when set) implements optimistic concurrency: if the cube has advanced past `expected_revision`, the write is rejected with `StaleRevision`. Callers retry with a fresh revision.
- **I-WB-9.** Type mismatches (writing an `I64` to an `F64` cell or vice versa) are rejected at the API boundary, never silently coerced.

### 13.4 Acme demo example

Successful write:

```rust
let result = cube.write(WritebackRequest {
    coord: coord_for("Baseline", "Working", "Mar_2026", "Paid_Search", "Tampa", "Spend"),
    new_value: ScalarValue::F64(50_000.0),
    principal: principal("edwin"),
    intent: WriteIntent::Set,
    expected_revision: Some(Revision(3141)),
})?;

assert_eq!(result.revision_after, Revision(3142));
// invalidated includes:
//   Clicks, Leads, Customers, Revenue, Gross_Profit at the same coord
//   plus the same measures at every consolidated ancestor coord
//   (Q1, Florida, Paid_Media, etc.)
```

Rejected write (derived cell):

```rust
let err = cube.write(WritebackRequest {
    coord: coord_for("Baseline", "Working", "Mar_2026", "Paid_Search", "Tampa", "Revenue"),
    new_value: ScalarValue::F64(125_000.0),
    principal: principal("edwin"),
    intent: WriteIntent::Set,
    expected_revision: None,
}).unwrap_err();

assert!(matches!(err, WritebackError::DerivedCellNotWritable { .. }));
// Cube state is unchanged. Revenue continues to be computed by rule_revenue.
```

Rejected write (consolidated cell):

```rust
let err = cube.write(WritebackRequest {
    coord: coord_for("Baseline", "Working", "Q1_2026", "Paid_Media", "Florida", "Spend"),
    new_value: ScalarValue::F64(1_350_000.0),
    ...
}).unwrap_err();

assert!(matches!(err, WritebackError::ConsolidatedCellNotWritable { .. }));
// The user is editing a rollup. They must allocate the new total across leaves.
// (Spreading rules — e.g., proportional spread across leaves — are a Phase 2+ feature.)
```

### 13.5 Failure modes

- **Silent acceptance of derived-cell writes.** A common spreadsheet bug: user types a number into a formula cell, formula is replaced with the constant, the formula is gone. This is data loss disguised as input. The engine rejects, hard.
- **Non-atomic writes.** If a writer updates Spend, then before the dirty-propagation completes a reader fetches Revenue, the reader sees stale Revenue with new Spend — internally inconsistent. Atomicity requires the write + invalidation to commit as one revision bump.
- **Type coercion.** Writing `"50000"` (string) to an `F64` cell silently converted to `50000.0` masks a class of integration bugs (CSV importer not parsing right). Reject coercions at the boundary.
- **Lost-update races.** Without `expected_revision`, two writers concurrently overwriting the same cell produces last-writer-wins with no notification. v1 supports optimistic concurrency; production callers should use it.
- **Permission checks after write.** Checking permissions after the write completes (and rolling back on fail) leaks data through timing and produces audit trail garbage. Permissions check before the write commits.
- **Spreading without explicit consent.** Some planning tools auto-spread a write to a consolidated cell across its leaves proportionally. v1 does not do this implicitly; the caller must perform the spread explicitly. Implicit spreading silently loses information about which leaf actually changed.

---

## 14. Trace

### 14.1 Definition

A **Trace** is a structured explanation of how a cell's value was computed. It is a tree whose root is the requested cell and whose children are the inputs the cell read, recursively. Each node carries the cell's value, the operation that produced it (rule, consolidation, input lookup), and a reference to the rule or hierarchy responsible. Traces are the engine's audit primitive: a user looking at a Revenue number should be able to ask "where did this come from?" and get a complete provenance tree without code archaeology.

### 14.2 Rust shape

```rust
pub struct Trace {
    root: TraceNode,
    revision: Revision,                    // snapshot at trace time
    elapsed_us: u64,                       // diagnostic
}

pub struct TraceNode {
    coord: CellCoordinate,
    value: ScalarValue,
    operation: TraceOp,
    children: Vec<TraceNode>,
    note: Option<String>,                  // human-readable annotation
}

pub enum TraceOp {
    InputLookup { written_at: Timestamp, written_by: PrincipalId },
    RuleEvaluation { rule_id: RuleId, expression: ExprSummary },
    Consolidation {
        hierarchies: SmallVec<[HierarchyId; 4]>,
        child_count: u32,
        weights: Vec<f64>,
    },
    DefaultFallback { default: ScalarValue, reason: String },
    NullPoison { upstream_null_coord: CellCoordinate },
}

pub struct ExprSummary {
    op: String,                            // "Mul", "Add", "Div", etc.
    arity: u32,
}
```

### 14.3 Invariants

- **I-Trace-1.** A trace is a directed acyclic graph (in fact, a tree, because each cell read produces a fresh subtree). No cycles.
- **I-Trace-2.** Every leaf node is either an `InputLookup` or a `DefaultFallback`. Internal nodes are always derived (rule, consolidation, or null-poison).
- **I-Trace-3.** A trace's root value matches the cell's current `CellValue` at the trace's `revision`. Two reads at the same revision produce structurally-identical traces (subject to rule scope and hierarchy nondeterminism, which v1 forbids).
- **I-Trace-4.** Trace generation is opt-in. The engine never materializes traces unless requested.
- **I-Trace-5.** Trace size is bounded by the closure of the cell's dependencies. For a deeply-rolled-up consolidated cell, this can be thousands of leaf nodes; the trace API supports incremental walking via depth limits and lazy children.
- **I-Trace-6.** A trace is *exhaustive*: every dependency that contributed to the value appears in the trace. A rule that reads from cell X but doesn't list it in the trace is a bug.

### 14.4 Acme demo example

Trace of `(Baseline, Working, Mar_2026, Paid_Search, Tampa, Revenue)`:

```
Revenue = 125_000.0
└── Operation: RuleEvaluation { rule: rule_revenue, expr: Mul }
    ├── Customers = 25.0
    │   └── Operation: RuleEvaluation { rule: rule_customers, expr: Mul }
    │       ├── Leads = 250.0
    │       │   └── Operation: RuleEvaluation { rule: rule_leads, expr: Mul }
    │       │       ├── Clicks = 5_000.0
    │       │       │   └── Operation: RuleEvaluation { rule: rule_clicks, expr: Div }
    │       │       │       ├── Spend = 50_000.0
    │       │       │       │   └── Operation: InputLookup { written_at: ..., written_by: edwin }
    │       │       │       └── CPC = 10.0
    │       │       │           └── Operation: InputLookup { written_at: ..., written_by: edwin }
    │       │       └── CVR = 0.05
    │       │           └── Operation: InputLookup { ... }
    │       └── Close_Rate = 0.10
    │           └── Operation: InputLookup { ... }
    └── AOV = 5_000.0
        └── Operation: InputLookup { ... }
```

The root is `Revenue`, computed by the `rule_revenue` rule. Below it: `Customers × AOV`. `Customers` itself is derived (`Leads × Close_Rate`). And so on, down to the five inputs (Spend, CPC, CVR, Close_Rate, AOV) at the bottom, each an `InputLookup` carrying who wrote it and when.

For a trace of `(Baseline, Working, Q1_2026, Paid_Media, Florida, Revenue)`, the root is a `Consolidation` node with 27 children (3 months × 3 cities × 3 channels × 1 measure), each itself a derived `Revenue` subtree.

### 14.5 Failure modes

- **Lossy traces.** A trace that omits steps ("Revenue = some big formula = 125,000") is useless. Traces must reproduce every intermediate value.
- **Inconsistent traces.** A trace whose final value doesn't match the cell's actual value indicates either trace bugs or evaluation bugs. Either way, the engine is broken. Traces are tested with strict equality against the cell value.
- **Eager generation.** Producing traces on every read multiplies read latency by the depth of the rule tree. Traces are explicit opt-ins.
- **Loss of provenance for input cells.** An `InputLookup` node that doesn't record `written_at` and `written_by` makes the trace useless for audit. Provenance must be carried into the trace.
- **No trace API for consolidations.** A trace that only explains rule evaluations and not hierarchy walks leaves the user wondering where the rollup numbers came from. Consolidation must be traceable.
- **Trace for null cells.** A trace of a Null cell should explain *why* it's Null (input never written, or rule short-circuited because of a null input upstream). `NullPoison` carries the upstream coordinate that introduced the null.

---

## 15. Dependency

### 15.1 Definition

A **Dependency** is an edge in the cube's directed acyclic graph (DAG) of cell-to-cell reads. If rule A reads cell B, then B is a dependency of A. Dependencies are *declared explicitly* by rules and validated by the engine during evaluation. The engine uses the DAG for two things: (1) cycle detection at definition time, (2) dirty propagation at write time. v1 deliberately does not implement automatic feeder inference (where the engine derives dependencies from the rule body); explicit declarations are the contract, and full-scan validation is the safety net.

### 15.2 Rust shape

```rust
pub struct DependencyGraph {
    cube: CubeId,
    edges: HashMap<CellCoordinate, Vec<DependencyEdge>>,
    reverse_edges: HashMap<CellCoordinate, Vec<CellCoordinate>>,  // for fast invalidation
}

pub struct DependencyEdge {
    from: CellCoordinate,                  // the dependent cell
    to: CellCoordinate,                    // the cell it reads
    via: DependencySource,
}

pub enum DependencySource {
    Rule { rule_id: RuleId },
    Hierarchy { hierarchy: HierarchyId },  // consolidation edge
}

pub enum CoordPattern {
    Exact(CellCoordinate),
    SameAsTarget,                          // same coord as the rule target
    OffsetMeasure(ElementId),              // same coord but a different measure
    SubtreeOf { dim: DimensionId, root: ElementId },
    // Phase 2+: pattern matching for richer rules
}
```

### 15.3 Invariants

- **I-Dep-1.** The dependency graph is acyclic. Cycles are detected at rule registration and rejected.
- **I-Dep-2.** Every rule's `declared_dependencies` is a complete superset of the cells the rule actually reads at evaluation time. Unauthorized reads (the rule reads a cell it didn't declare) cause an `UndeclaredDependencyError` in validation mode and a missed-invalidation bug in production mode. The engine runs validation mode in tests and an opt-in mode in production.
- **I-Dep-3.** Hierarchy edges are auto-generated from the hierarchy structure: the consolidated coordinate has a dependency on each of its leaf descendants.
- **I-Dep-4.** When a cell is dirtied, every cell with a transitive dependency on it is also dirtied. The dirty set is the closure of the reverse-edge graph.
- **I-Dep-5.** A cell's dependency edges are defined relative to a coordinate. Two rules with different scopes produce different edges. The graph is concrete (per-coordinate), not abstract (per-rule).
- **I-Dep-6.** Dependency edges may cross cubes (Phase 5+). v1 confines edges to a single cube; cross-cube dependencies are rejected.

### 15.4 Acme demo example

After registering all five marketing-finance rules, the dependency graph for one specific coordinate `(Baseline, Working, Mar_2026, Paid_Search, Tampa, Revenue)`:

```
Edges from Revenue:
  Revenue       → Customers   (via rule_revenue)
  Revenue       → AOV         (via rule_revenue)

Edges from Customers:
  Customers     → Leads       (via rule_customers)
  Customers     → Close_Rate  (via rule_customers)

Edges from Leads:
  Leads         → Clicks      (via rule_leads)
  Leads         → CVR         (via rule_leads)

Edges from Clicks:
  Clicks        → Spend       (via rule_clicks)
  Clicks        → CPC         (via rule_clicks)

(All five inputs — Spend, CPC, CVR, Close_Rate, AOV — are leaf input cells with no outgoing edges.)
```

Reverse edges (for invalidation): if Spend is dirtied, the reverse-edge walk hits Clicks, then Leads, then Customers, then Revenue, then Gross_Profit. All five dependent cells are marked dirty in one pass.

For consolidated coordinates, the graph adds hierarchy edges:

```
(Q1_2026 Spend) ← (Jan_2026 Spend), (Feb_2026 Spend), (Mar_2026 Spend)
(Florida Spend) ← (Tampa Spend), (Orlando Spend), (Miami Spend)
```

Writing Tampa's Mar Spend dirties Florida's Mar Spend (hierarchy), which dirties Florida's Q1 Spend (hierarchy), and so on. Plus everything downstream via rule edges (Florida Mar Revenue, Florida Q1 Revenue, etc.).

### 15.5 Failure modes

- **Implicit / inferred dependencies as the v1 contract.** Static analysis on rule bodies to infer dependencies is brittle. A rule that does `if condition then read_X else read_Y` has dynamic dependencies that static analysis can over- or under-approximate. v1 makes the contract explicit; the dev declares everything. Auto-inference can come later as a *suggestion* tool, not a guarantee.
- **Unvalidated declarations.** A rule that declares it reads X but actually reads X and Y silently misses Y-invalidation. The engine must run validation: at evaluation time, record the actual reads and compare against the declaration. Test mode panics on mismatch; production mode logs and proceeds with the more conservative dirty set.
- **Cycle detection at evaluation time.** A non-terminating evaluation due to a cycle is the worst possible failure mode (production server hang). Cycles must be detected when the rule is registered, not when it runs.
- **Coarse-grained invalidation.** "Any write dirties everything" works for small cubes but produces full-cube recalc on every keystroke. The reverse-edge graph must be precise enough to mark only the actually-affected closure.
- **Forgetting hierarchy edges in invalidation.** A leaf write that dirties downstream rules but not its consolidated parents leaves stale rollups. Hierarchy edges are first-class in the dependency graph.
- **Cross-revision dependency leakage.** A dependency edge that pointed at a now-deleted cell (after a dimension element removal) becomes a dangling reference. v1 forbids element removal after freeze (per §2 I-Dim-5), which sidesteps this.

---

## 16. DirtyCell

### 16.1 Definition

A **DirtyCell** is a cell whose cached value is no longer valid. Cells become dirty when an upstream input or rule changes; they are recomputed on next read. The dirty state is the engine's mechanism for lazy evaluation: writes are O(1) (mark dirty, don't recompute); reads are O(closure size on first read after dirtying, then O(1) cached). DirtyCell is not a separate type — it is a flag in the cell store. The dirty set is the set of cells currently flagged.

### 16.2 Rust shape

```rust
pub struct DirtyTracker {
    cube: CubeId,
    dirty_set: HashSet<CellCoordinate>,
    dirty_count: usize,                    // for diagnostics
    last_marked_revision: Revision,
}

impl DirtyTracker {
    pub fn mark(&mut self, coord: &CellCoordinate);
    pub fn mark_closure(&mut self, root: &CellCoordinate, graph: &DependencyGraph);
    pub fn is_dirty(&self, coord: &CellCoordinate) -> bool;
    pub fn clear(&mut self, coord: &CellCoordinate);  // called after recompute
    pub fn snapshot(&self) -> DirtySnapshot;
}

pub struct DirtySnapshot {
    revision: Revision,
    coords: BTreeSet<CellCoordinate>,
}
```

### 16.3 Invariants

- **I-Dirty-1.** Marking a cell dirty also marks every cell with a transitive dependency on it dirty (closure marking via the reverse-edge graph).
- **I-Dirty-2.** A cell can be dirty without having been read since dirtying. Lazy recompute means the value isn't refreshed until requested.
- **I-Dirty-3.** Reading a dirty cell triggers recompute *before* the read returns. The read either blocks waiting for recompute or runs the computation inline; either way, the caller never observes a stale value.
- **I-Dirty-4.** After successful recompute, the cell is removed from the dirty set and its value is cached at the current revision.
- **I-Dirty-5.** The dirty set is per-cube. A write to cube A does not dirty cells in cube B (until cross-cube refs in Phase 5+).
- **I-Dirty-6.** Marking is idempotent: marking an already-dirty cell is a no-op.
- **I-Dirty-7.** The dirty tracker survives reads but is reset on snapshot rollback (§19).

### 16.4 Acme demo example

State machine:

```
Initial:  All cells either Input (clean, with values) or Derived (no cached value).
          dirty_set = {} (everything is "implicitly dirty until first read", but we use
          "explicitly clean" only after a successful read materializes the cache.)

Write: Spend(Tampa, Mar, Paid_Search) = 50_000
  → Tampa Mar Paid_Search Spend now has provenance Input, value 50_000.
  → DirtyTracker.mark_closure(written_coord) walks the reverse-edge graph:
    - rule edges:
      Tampa Mar Paid_Search Clicks       → dirty
      Tampa Mar Paid_Search Leads        → dirty
      Tampa Mar Paid_Search Customers    → dirty
      Tampa Mar Paid_Search Revenue      → dirty
      Tampa Mar Paid_Search Gross_Profit → dirty
    - hierarchy edges (Spend rolls up):
      Tampa Q1 Paid_Search Spend         → dirty
      Tampa Q1 Paid_Media Spend          → dirty (Tampa rolls into Q1, Paid_Search into Paid_Media)
      Florida Mar Paid_Search Spend      → dirty
      Florida Mar Paid_Media Spend       → dirty
      Florida Q1 Paid_Media Spend        → dirty
      Southeast Q1 Paid_Media Spend      → dirty
      USA Q1 Paid_Media Spend            → dirty
      ... and combinations across hierarchies ...
    - and rules over consolidated coords:
      Florida Q1 Paid_Media Revenue      → dirty
      USA Q1 Paid_Media Gross_Profit     → dirty
      ...

Total dirtied for one Spend write in Acme: tens of cells (a small fraction of the cube).

Read: Florida Q1 Paid_Media Revenue
  → Cell is in dirty_set.
  → Engine recursively reads its dependencies, recomputing each as it goes.
  → All dependencies clean and cached after this read.
  → Returns CellValue with new value, revision = current.
```

### 16.5 Failure modes

- **Eager recompute on write.** Recomputing every dirtied cell at write time blocks the writer for proportional time and wastes work for cells nobody ever reads. Lazy recompute on read is the right model — it's literally the TM1 model with a different name.
- **Coarse marking.** "On any write, mark all derived cells dirty" works but produces O(N) re-reads instead of O(closure size). The reverse-edge graph must be precise.
- **Forgetting to clear after recompute.** A cell that stays in the dirty set after being computed is recomputed on every read forever. Cache poisoning via leaked dirty flags.
- **Dirty leak across snapshots.** If snapshot rollback doesn't reset the dirty set, the rolled-back cube has stale flags pointing at coordinates that no longer reflect their pre-rollback dependencies. Snapshot transitions reset dirty state.
- **Race between dirty and read.** If a writer marks dirty but the marker isn't visible to a reader running concurrently, the reader sees a stale cached value. Atomicity (per I-WB-6) covers this: the marking is part of the same revision bump as the write.
- **Dirty as the only state.** Some systems use a tri-state (clean/dirty/computing) to handle re-entrance. v1 collapses to binary because rule evaluation is single-threaded per cube; multi-threaded evaluation (Phase 4+) needs the tri-state.

---

## 17. PermissionScope

### 17.1 Definition

A **PermissionScope** is the set of (coordinate-pattern, capability) pairs authorized for a principal. A capability is one of `Read`, `Write`, `Approve`, `Lock`, `Unlock`, `Admin`. A scope is the engine's primary access-control primitive: writes are checked against the scope before they commit; reads are filtered through the scope. Scopes are *not* full enterprise auth (no SSO, no groups, no admin UI); they are the semantic model that lets an enterprise auth layer plug in later.

### 17.2 Rust shape

```rust
pub struct PermissionTable {
    cube: CubeId,
    grants: Vec<Grant>,
}

pub struct Grant {
    principal: PrincipalId,
    pattern: ScopePattern,                 // which cells this grant covers
    capabilities: CapabilitySet,
    granted_at: Timestamp,
    granted_by: PrincipalId,
    expires_at: Option<Timestamp>,
}

pub struct ScopePattern {
    bindings: HashMap<DimensionId, ScopeBinding>,
}

pub enum ScopeBinding {
    One(ElementId),
    Many(Vec<ElementId>),
    Subtree(ElementId),
    All,
}

pub struct CapabilitySet(u32);              // bitfield

pub mod capability {
    pub const READ: u32     = 1 << 0;
    pub const WRITE: u32    = 1 << 1;
    pub const APPROVE: u32  = 1 << 2;       // can transition Submitted → Approved
    pub const LOCK: u32     = 1 << 3;       // can place locks
    pub const UNLOCK: u32   = 1 << 4;       // can remove others' locks
    pub const ADMIN: u32    = 1 << 5;       // grants new permissions
}

pub struct PrincipalId(u64);
```

### 17.3 Invariants

- **I-Perm-1.** Every read and write checks the principal's scope before completing. No fast-path that bypasses checks.
- **I-Perm-2.** Capabilities are positive grants; the absence of a grant is the absence of a permission. There is no "deny" rule (which produces the well-known order-dependent bugs of allowlist/denylist mixing).
- **I-Perm-3.** Multiple grants for the same principal are OR'd: a cell is accessible if any matching grant authorizes it.
- **I-Perm-4.** `Admin` is a meta-capability: it lets a principal create new grants. It does not implicitly grant `Read` or `Write` — admins must grant themselves those explicitly. This is a small annoyance that prevents the common "I have admin so I can do anything" ambient-authority bug.
- **I-Perm-5.** Permission grants never apply retroactively. A grant created at time T does not authorize reads at revisions before T.
- **I-Perm-6.** The cube root user (created at cube-init) has implicit `Admin` and full `Read`/`Write`. This is an integration point for an external auth system.
- **I-Perm-7.** Grant expiration is checked at use time. An expired grant returns `InsufficientPermission` errors as if it never existed.

### 17.4 Acme demo example

```rust
// FP&A team can read everything in Acme
acme.grant(Grant {
    principal: principal("fpa_team"),
    pattern: ScopePattern::all(),
    capabilities: CapabilitySet::with(capability::READ),
    granted_by: principal("admin"),
    ..default()
});

// Florida regional planner can write inputs in Florida only, in Working version, Baseline scenario
acme.grant(Grant {
    principal: principal("planner_fl"),
    pattern: ScopePattern {
        bindings: hashmap! {
            scenario_dim_id => ScopeBinding::One(baseline_id),
            version_dim_id => ScopeBinding::One(working_id),
            time_dim_id => ScopeBinding::All,
            channel_dim_id => ScopeBinding::All,
            market_dim_id => ScopeBinding::Subtree(florida_id),
            measure_dim_id => ScopeBinding::Many(vec![spend_id, cpc_id, cvr_id, close_rate_id, aov_id]),
        },
    },
    capabilities: CapabilitySet::with(capability::READ | capability::WRITE),
    ..default()
});

// CFO can approve any Submitted version
acme.grant(Grant {
    principal: principal("cfo"),
    pattern: ScopePattern::all(),
    capabilities: CapabilitySet::with(capability::READ | capability::APPROVE),
    ..default()
});
```

A write attempt by `planner_fl` to `(Baseline, Working, Mar_2026, Paid_Search, Tampa, Spend)` succeeds: the grant covers it. A write attempt by the same user to `(Baseline, Working, Mar_2026, Paid_Search, Atlanta, Spend)` fails (Atlanta is in Georgia, not Florida): `WritebackError::InsufficientPermission`.

### 17.5 Failure modes

- **Implicit admin powers.** A user with `Admin` who can transitively read or write anything by virtue of being an admin produces invisible permission paths. Capabilities are explicit; admins must grant themselves what they need.
- **Allow + Deny mixing.** Combined allow/deny rules with precedence are notorious sources of "the user could do X yesterday but not today" bugs. v1 supports only positive grants.
- **Permission checks after write.** Discussed in §13; checking after the write commits leaks data. Checks happen first.
- **Pattern matching that's slow.** A write that has to evaluate hundreds of grant patterns with `Subtree` bindings is O(grants × tree depth). v1 uses a small-N grant table; if the table grows, indexed lookups are added.
- **Forgetting expiration.** A grant with `expires_at: Some(yesterday)` that the engine still honors is a security bug. Expiration is checked at every use.
- **No audit trail.** Every grant change must be logged with who, what, when. Otherwise "who gave X access to Y?" is unanswerable.

---

## 18. LockState

### 18.1 Definition

A **LockState** is a flag attached to a cell or slice that prevents writes by anyone other than the lock holder. Locks are how planners coordinate: "I'm working on this slice; nobody else should change it until I release." A lock has an owner, a scope (which cells it covers), a kind (`Soft` advisory vs `Hard` enforced), and an expiration. Locks compose with permission scopes: a user must have `Lock` capability to place a lock and `Write` capability to acquire it on cells in the lock scope.

### 18.2 Rust shape

```rust
pub struct LockTable {
    cube: CubeId,
    locks: Vec<Lock>,
}

pub struct Lock {
    id: LockId,
    owner: PrincipalId,
    pattern: ScopePattern,                 // same shape as PermissionScope
    kind: LockKind,
    acquired_at: Timestamp,
    expires_at: Timestamp,                 // mandatory; v1 has no infinite locks
    note: Option<String>,
}

pub enum LockKind {
    Soft,                                  // advisory: warn-on-write but allow
    Hard,                                  // enforced: reject other writers
}

pub enum LockError {
    Conflict { existing: LockId, principal: PrincipalId },
    InsufficientCapability,
    PatternConflict { reason: String },
    Expired { lock: LockId },
}
```

### 18.3 Invariants

- **I-Lock-1.** Two `Hard` locks with overlapping patterns owned by different principals cannot coexist. The second acquisition fails with `Conflict`.
- **I-Lock-2.** A `Hard` lock blocks writes by everyone except the owner. The lock owner can still write within their permission scope.
- **I-Lock-3.** A `Soft` lock does not block writes but produces an advisory warning in the writeback result.
- **I-Lock-4.** Every lock has an explicit `expires_at`. v1 has no infinite locks; expired locks are removed lazily on the next conflict check.
- **I-Lock-5.** A principal acquiring a lock must have the `Lock` capability *and* the `Write` capability on the scoped cells. (You can't lock cells you can't write.)
- **I-Lock-6.** Removing a lock requires `Unlock` capability or being the lock owner.
- **I-Lock-7.** Locks do not affect reads. Reads always succeed regardless of locks.
- **I-Lock-8.** Locks on consolidated patterns expand to cover all leaves in the pattern. Locking `(Florida, Q1, Paid_Media, Spend)` locks every leaf in that subtree.

### 18.4 Acme demo example

The Florida planner acquires a hard lock on her slice for Friday's submission deadline:

```rust
let lock = acme.acquire_lock(Lock {
    owner: principal("planner_fl"),
    pattern: ScopePattern {
        bindings: hashmap! {
            scenario_dim_id => ScopeBinding::One(baseline_id),
            version_dim_id => ScopeBinding::One(working_id),
            time_dim_id => ScopeBinding::Subtree(q1_2026_id),
            channel_dim_id => ScopeBinding::All,
            market_dim_id => ScopeBinding::Subtree(florida_id),
            measure_dim_id => ScopeBinding::All,
        },
    },
    kind: LockKind::Hard,
    expires_at: Friday_5pm_ET,
    note: Some("Submitting Q1 Florida plan; please don't edit".into()),
})?;

// The Atlanta planner tries to write to Florida's Tampa Spend (out of their scope anyway):
let err = acme.write(WritebackRequest {
    coord: tampa_mar_paid_search_spend,
    new_value: ScalarValue::F64(75_000.0),
    principal: principal("planner_ga"),
    ...
}).unwrap_err();
// First fails permission check (Atlanta planner can't write to Florida).
// Even if they could, lock would block it.

// FL planner can still write within their lock+scope.
let result = acme.write(WritebackRequest {
    coord: tampa_mar_paid_search_spend,
    new_value: ScalarValue::F64(60_000.0),
    principal: principal("planner_fl"),
    ...
})?; // succeeds

// After Friday 5pm, lock expires; other writers can write again.
```

### 18.5 Failure modes

- **Locks without expiration.** A planner places a lock, leaves the company, and the lock persists forever. Mandatory expiration prevents this; admins can extend if needed.
- **Lock-without-permission acquisition.** A user with `Lock` capability who doesn't have `Write` could lock cells they can't actually edit, denying everyone access. Require both capabilities.
- **Lock check after write.** A write that completes and is then checked against locks (and rolled back on conflict) wastes work and leaks data via timing. Check first.
- **Coarse lock patterns.** A user who wants to lock one cell and locks the whole cube with `ScopeBinding::All` blocks everyone unnecessarily. v1 trusts the user to scope correctly; UI guidance is the mitigation.
- **Lock conflicts as silent failures.** A second-acquisition that silently no-ops instead of erroring leaves the user thinking they have a lock when they don't. Always return explicit success/failure.
- **Reads affected by locks.** Some systems use locks to gate reads as well; v1 explicitly does not, because it makes audit trails inconsistent ("the FP&A team couldn't see Friday's numbers because someone else was editing them").

---

## 19. Snapshot

### 19.1 Definition

A **Snapshot** is a coherent view of the cube at a specific revision. Reads take a snapshot implicitly (the current revision) or explicitly (a historical revision). Snapshots are the engine's mechanism for point-in-time queries, audit, rollback, and isolation. A snapshot is *not* a full copy of the cube state — it is a logical view; the implementation uses the same underlying storage with revision filtering. Snapshots are cheap to create (O(1)) but reads through old snapshots may be slower if the storage layer needs to filter out newer revisions.

### 19.2 Rust shape

```rust
pub struct Snapshot {
    cube: CubeId,
    revision: Revision,
    captured_at: Timestamp,                // wall-clock time, for diagnostics
    label: Option<String>,                 // human-readable, e.g., "FY2026_Approved_Plan"
}

pub struct SnapshotHandle<'cube> {
    snapshot: Snapshot,
    cube: &'cube Cube,
}

impl<'cube> SnapshotHandle<'cube> {
    pub fn read(&self, coord: &CellCoordinate) -> Result<CellValue>;
    pub fn slice(&self, query: &SliceQuery) -> Result<SliceResult>;
    pub fn trace(&self, coord: &CellCoordinate) -> Result<Trace>;
}
```

### 19.3 Invariants

- **I-Snap-1.** A snapshot is immutable. Reads through a snapshot at revision N return values as they were at revision N, regardless of later writes.
- **I-Snap-2.** Snapshots do not block writes. A snapshot is a logical view; writes proceed on the live cube and the snapshot continues to read historical data.
- **I-Snap-3.** Snapshots have a retention policy (Phase 3+; v1 keeps every revision in memory). A snapshot whose revision has been pruned returns `SnapshotExpired` errors.
- **I-Snap-4.** Two snapshots at the same revision return identical values for every read (subject to the `request_trace` and `request_uncertainty` flags affecting only output shape).
- **I-Snap-5.** A read through a snapshot still respects permission scopes — but uses scopes as they existed at the snapshot's revision, not current scopes.
- **I-Snap-6.** Slicing through a snapshot is atomic: every cell in the slice is at the snapshot's revision, never mixing with newer revisions.
- **I-Snap-7.** Snapshot labels are advisory metadata; they do not affect behavior. Two snapshots can share a label (though this is discouraged).

### 19.4 Acme demo example

```rust
// Capture a snapshot at the moment of approval
let approved_snapshot = acme.snapshot()
    .label("FY2026_Approved_Plan")
    .capture()?;

// Three months later, the planner wants to know: was Tampa's Q1 Spend the same
// in the approved plan as it is today?
let approved_spend = approved_snapshot.read(&tampa_q1_spend_coord)?;
let current_spend = acme.read(&tampa_q1_spend_coord)?;
let variance = current_spend.value.as_f64()? - approved_spend.value.as_f64()?;

// Or: re-read the entire approved plan for an audit
let approved_slice = approved_snapshot.slice(&full_cube_slice_query)?;

// Or: rollback the cube to the approved state (Phase 3+; effectively makes a new
// revision whose values match the snapshot)
acme.rollback_to(&approved_snapshot)?;
```

### 19.5 Failure modes

- **Mixed-revision slices.** A slice that captures cell A at revision N and cell B at revision N+1 produces internally-inconsistent reports. Snapshots must capture coherently.
- **Snapshot tied to a write lock.** Some implementations block writes during snapshot. This serializes the cube and produces a hot-spot on snapshot creation. Snapshots are O(1) and concurrent with writes.
- **Pruning without notification.** If old snapshots are pruned and consumers still hold references, reads silently fail. Either keep snapshots forever (v1) or fail explicitly with `SnapshotExpired`.
- **Snapshot of derived cells without rule version.** A snapshot at revision N must use the rule definitions as they were at revision N, not current rules. Otherwise a snapshot of last year's plan produces this year's recalculated numbers, defeating the purpose. v1 punts this: rules are immutable per cube. Phase 3+ tracks rule version per snapshot.
- **No way to label.** Without labels, snapshots are identified by revision number, which is opaque. "What does revision 3142 mean?" is unanswerable. Labels are the human handle.
- **Permission drift.** A snapshot read using current permissions may fail or return different cells than the snapshot's contemporary permissions would have. Decide explicitly: v1 uses current permissions (operational simplicity); Phase 3+ uses snapshot-time permissions (audit fidelity).

---

## 20. Cross-cutting glossary

A few terms used across multiple sections, defined once.

- **Principal:** A user, service, or other authenticated identity. Identified by `PrincipalId`. Authentication is out of scope for the engine; principals are passed in by the caller.
- **Provenance:** Where a value came from. One of `Input | Rule | Consolidation | Default`.
- **Revision:** Monotonic per-cube version counter. Bumps on every successful write. Used by dirty tracking, snapshots, and optimistic concurrency.
- **Workspace:** Top-level container for a set of cubes that share dimensions, principals, and permissions. Phase 3+ concept; v1 has one workspace.
- **Scope:** A pattern over coordinates. Used by permissions, locks, and slice queries (which all share the `ScopePattern` shape).
- **Capability:** A bit in `CapabilitySet`. Read/Write/Approve/Lock/Unlock/Admin.

---

## 21. Engine semantics — non-negotiable correctness rules

These rules transcend any single concept. The engine implementation must uphold all of them simultaneously. They are the contract.

1. **Determinism.** Two reads of the same coordinate at the same revision return byte-identical `CellValue`s.
2. **Coherence.** A slice or trace returns values from a single coherent revision; never mixes revisions.
3. **Atomicity.** Writes commit atomically. Readers see pre-write state or post-write state, never partial.
4. **Causality.** Dependents are dirtied before they are read. A read of a cell that depends on a recent write returns the up-to-date value, not stale.
5. **Authorization.** Every read and write checks permissions before completing.
6. **No silent type coercion.** Type mismatches at the cell boundary are errors, not coercions.
7. **No silent dependency miss.** Rules whose declared dependencies don't match observed dependencies fail validation in test mode.
8. **No silent null-zero confusion.** `Null` is a distinct value from `0`; rules define their null policy explicitly.
9. **No mutation of frozen dimensions.** Once a dimension is bound to a cube, elements may be appended but never removed or reordered.
10. **No writes to derived cells.** Consolidations and rule outputs are read-only.

These rules are tested in the Phase 1 Correctness Doctrine (see the Phase 1 Build Brief).

---

## 22. What this spec does NOT cover

To prevent scope creep:

- **Model-backed cells.** A `Provenance::Model { artifact_hash, ... }` variant exists in spirit but is not implemented in v1. Model cells are a Phase 4 concept.
- **DuckDB integration.** External storage adapters are Phase 5+.
- **WASM bindings.** The Rust core is built to be WASM-compilable but the binding crate is Phase 6+.
- **CRDT-based collaborative editing.** v1 is single-writer; multi-writer coordination is Phase 7+.
- **String-parsed rules / DSL.** v1 rules are typed AST builders. A DSL surface is Phase 4+.
- **LLM rule authoring.** Out of scope until a stable engine exists.
- **Schema marketplace.** Schemas are first-class artifacts but the marketplace tooling is post-launch.
- **Auto-feeder inference.** v1 requires explicit dependency declarations. Inference is a research bet for v3.
- **Spreading writes to consolidated cells.** v1 rejects them outright. Spreading rules are Phase 2+.

---

## 23. Glossary alignment with TM1

For readers coming from TM1:

| TM1 term | MarketingCubes term | Notes |
|---|---|---|
| Cube | Cube | Same |
| Dimension | Dimension | Same |
| Element | Element | Same |
| Hierarchy | Hierarchy | Same; v1 supports one default hierarchy per dimension |
| Consolidation | Consolidation | Same; v1 enforces per-measure aggregation rules |
| Rule | Rule | TM1 rules are string-parsed; ours are AST |
| Feeder | DependencyDecl | TM1 feeders are manual; ours are explicit declarations validated by the engine |
| Skipcheck | (n/a) | We don't need it; sparse storage is the default |
| TI process | (n/a) | Not in v1; admin scripts are out-of-band |
| Data Reservation | Lock | Same idea, simpler implementation |
| Security on cube/dimension/element/cell | PermissionScope | Generalized to scope patterns |
| Snapshot / save-data | Snapshot | Logical, not file-based |
| TM1 Server Admin | Workspace | Phase 3+ |

---

*End of engine semantics spec, v1.*
