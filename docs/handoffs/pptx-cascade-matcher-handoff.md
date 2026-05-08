# PPTX Cascade Matcher Handoff — Session A

> **Audience:** the Claude Code instance that implements the PPTX cascade matcher.
> **You inherit `main` at 1031 / 0 tests. You'll work on the branch
> `pptx-cascade-matcher`.**
>
> **The PPTX table extractor already works** (`pptx.rs` pulls 48 tables
> from a Lumina deck in 20ms). The problem is **semantic matching** —
> the registry can't identify which tactic each table belongs to because
> PPTX-derived filenames don't match, and naive header overlap is
> dominated by shared columns. This handoff builds the cascade matcher
> that fixes that.
>
> **The binding design is in
> [`docs/decisions/0023-pptx-cascade-matcher.md`](../decisions/0023-pptx-cascade-matcher.md).
> Read it in full before starting.** The detailed spec with worked
> examples is in
> [`docs/external-conversations/pptx-cascade-matcher-v2.md`](../external-conversations/pptx-cascade-matcher-v2.md).

---

## The one paragraph you must internalize

The extractor produces `ExtractedTable` structs. The registry has 292
`TacticSpec` entries. The cascade matcher sits between them: for each
table, it walks 7 ordered steps — skip rule, profile override,
continuation detection, section×family cross-product, first-column
tactic lookup, section+title narrowing, TF-IDF header scoring — and
returns a `MatchResult` with mapping, confidence, evidence, and status.
Only `status = Matched` results flow into cube ingestion. Everything
else goes to the review queue or gets skipped. A profile YAML
(`.mosaic/pptx-profiles/lumina-charts.yaml`) declares sections (what
tactic), table families (what kind of table), aliases, skip rules, and
duplicate pairs. The profile is read-only in this session — the review
UI that writes back to profiles is Session B scope.

---

## What gets built (Session A, ~4-5h)

### Part 1: Foundation types + tokenization + IDF table

**New file: `crates/mc-demo-server/src/pptx_match.rs`**

This module has ZERO imports from axum, upload handlers, server state,
or workspace routing. Binding test: `grep -rn "axum\|upload::\|server::\|workspace::" crates/mc-demo-server/src/pptx_match.rs` returns zero.

**Types:**

```rust
/// A table extracted from a PPTX slide, enriched with positional context.
#[derive(Debug, Clone)]
pub struct ExtractedTable {
    pub slide_index: u32,       // 1-based
    pub table_index: u32,       // 0-based within slide
    pub slide_title: Option<String>,
    pub table_title: Option<String>, // nearest text above the table
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Result of matching one table against the registry.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub table: ExtractedTable,
    pub mapping: Option<RegistryMapping>,
    pub confidence: f64,        // 0.0–1.0
    pub status: MatchStatus,
    pub alternatives: Vec<(RegistryMapping, f64)>,
    pub flag_for_review: bool,
    pub duplicate_of: Option<TableRef>,
    pub evidence: MatchEvidence,
}

/// What the table maps to in the registry.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryMapping {
    pub product_name: String,
    pub subproduct_name: String,
    pub table_name: String,
}

#[derive(Debug, Clone)]
pub enum MatchStatus {
    Matched,
    Duplicate,
    Skipped,
    ContinuationCandidate,
    Unresolved,
}

#[derive(Debug, Clone)]
pub enum MatchSource {
    ProfileOverride,
    SectionFamily,
    ProfilePattern,
    FirstColumnExact,
    FirstColumnFuzzy,
    NarrowingUnique,
    TfidfMatch,
    TfidfUncertain,
    SkipRule,
    Unmatched,
}

/// Debug payload — every MatchResult carries this for cascade introspection.
#[derive(Debug, Clone)]
pub struct MatchEvidence {
    pub normalized_headers: Vec<String>,
    pub section_context: Option<String>,
    pub table_family: Option<String>,
    pub manifest_candidates: Vec<String>,
    pub candidate_count_before_narrowing: usize,
    pub candidate_count_after_narrowing: usize,
    pub top_scores: Vec<(RegistryMapping, f64)>,
    pub fired_step: MatchSource,
}

/// The deck-level result from match_deck.
#[derive(Debug)]
pub struct DeckMatchResult {
    pub manifest: DeckManifest,
    pub matches: Vec<MatchResult>,
    pub coverage_warnings: Vec<String>,
    pub stats: MatchStats,
}

/// Deck manifest from rollup pre-pass.
#[derive(Debug, Clone, Default)]
pub struct DeckManifest {
    pub declared_tactics: Vec<DeclaredTactic>,
    pub source_slides: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct DeclaredTactic {
    pub raw_label: String,
    pub normalized: String,
    pub registry_match: Option<RegistryMapping>,
}

/// Match statistics for diagnostics.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MatchStats {
    pub total_tables: usize,
    pub auto_resolved: usize,
    pub review_needed: usize,
    pub unmatched: usize,
    pub skipped: usize,
    pub duplicates: usize,
    pub by_source: HashMap<String, usize>,
}
```

**Tokenization** (see ADR-0023 Decision 11):

```rust
/// Normalize a header string for matching.
/// Strip parentheticals, lowercase, trim, collapse whitespace.
pub fn normalize_header(h: &str) -> String
```

Rules in order: strip `(...)` content → lowercase → trim → collapse
whitespace → drop empty. `CTR(Link Click-Through Rate)` → `ctr`.
`Link Clicks` stays as `link clicks` (don't split multi-word tokens).

**IDF table** (see ADR-0023 Decision 6):

```rust
/// Pre-computed IDF weights over registry headers.
/// Built once at registry load, shared via Arc.
pub struct IdfTable {
    scores: HashMap<String, f64>,  // normalized token → IDF score
    n: usize,                      // total registry entries
}

impl IdfTable {
    /// Build from registry entries.
    /// idf(token) = ln((N+1) / (df(token)+1)) + 1
    pub fn build(registry: &[TacticSpec]) -> Self
}
```

**F1 scoring:**

```rust
/// IDF-weighted F1 between a table's headers and a registry entry's headers.
pub fn score_f1(
    table_tokens: &[String],
    entry_tokens: &[String],
    idf: &IdfTable,
) -> f64
```

Compute recall = `inter_idf / entry_idf`, precision = `inter_idf / table_idf`,
F1 = harmonic mean. Return 0.0 if either side is empty.

---

### Part 2: Profile schema + loader

**New file: `crates/mc-demo-server/src/pptx_profile.rs`**

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PptxProfile {
    pub schema_version: String,
    pub profile_id: String,
    #[serde(default)]
    pub thresholds: MatchThresholds,
    #[serde(default)]
    pub aliases: AliasConfig,
    #[serde(default)]
    pub sections: Vec<SectionDef>,
    #[serde(default)]
    pub table_families: Vec<TableFamilyDef>,
    #[serde(default)]
    pub skip_tables: Vec<SkipRule>,
    #[serde(default)]
    pub duplicate_section_pairs: Vec<DuplicatePair>,
    #[serde(default)]
    pub overrides: Vec<OverrideDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatchThresholds {
    #[serde(default = "default_030")]
    pub auto_match_min_score: f64,      // 0.30
    #[serde(default = "default_005")]
    pub auto_match_min_margin: f64,     // 0.05
    #[serde(default = "default_020")]
    pub auto_match_min_relative_margin: f64, // 0.20
    #[serde(default = "default_020")]
    pub flag_for_review_min_score: f64, // 0.20
}

#[derive(Debug, Clone, Deserialize)]
pub struct SectionDef {
    pub id: String,
    pub title_matchers: Vec<TitleMatcher>,
    pub propagates: SectionPropagates,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SectionPropagates {
    pub product_name: String,
    pub default_subproduct: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableFamilyDef {
    pub id: String,
    pub title_matchers: Vec<TitleMatcher>,
    pub table_name: String,
    #[serde(default)]
    pub use_first_column_lookup: bool,
    #[serde(default)]
    pub first_column_header_in: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleMatcher {
    Equals(String),
    EqualsAny(Vec<String>),
    StartsWith(String),
    Contains(String),
    ContainsAny(Vec<String>),
}
```

**Loader:**

```rust
pub fn load_profile(dir: &Path, profile_id: &str) -> Option<PptxProfile>
```

Reads `.mosaic/pptx-profiles/{profile_id}.yaml`. Returns `None` if absent.

**Ship the Lumina profile** as `demo/sample-data/.mosaic/pptx-profiles/lumina-charts.yaml`
with all 10 sections and 9 table families from the v2 research note's
full example. This is a checked-in fixture, not user-editable in v1.

---

### Part 3: Rollup pre-pass + divider detection

**Rollup pre-pass** (builds `DeckManifest`):

Scan first 10 slides for tables where:
1. Slide title contains "Overall Performance" OR "Top KPIs"
2. First-column header is in {Product, Tactic, Channel, Platform}
3. Table has ≥2 data rows

For each qualifying table, extract first-column values, apply
`aliases.tactic`, look up in registry by `subproduct_name` (exact
then fuzzy). Record as `DeclaredTactic`.

**Empty manifest is not an error** — divider detection still works
via profile/registry paths.

**Divider detection** (with guards):

A slide is a section divider if and only if:
1. Zero data tables AND <15 lines of text
2. Title non-empty and <60 chars
3. Title resolves to: (a) profile section, (b) registry product/subproduct
   after aliases, or (c) manifest declared tactic

If 1+2 hold but 3 fails → content slide, NOT a divider. This kills
false positives on date strings, creative names, and keywords.

Propagation: divider's section context applies to all subsequent
slides until the next divider.

---

### Part 4: The cascade (Steps 0-6)

Implement the full cascade from ADR-0023 Decision 1. For each
`ExtractedTable`:

**Step 0: Skip rule.** Check `profile.skip_tables` matchers
(table_title_contains_any, slide_title_contains). If match →
`Skipped, confidence=1.0`.

**Step 1: Profile override.** Check `profile.overrides` by
(slide_index, table_index). If match → `Matched, confidence=1.0`.

**Step 2: Continuation candidate.** If same section context as
previous table, no header row detected (first row doesn't match
any known header pattern), and column count matches previous table →
`ContinuationCandidate, flag_for_review=true`.

**Step 3: Section + table-family.** If `section_ctx` is set, find
matching `table_family` from profile. If `use_first_column_lookup`:
iterate rows, apply `aliases.tactic`, registry lookup. Else: compose
`(section.product_name, section.default_subproduct, family.table_name)`.
Confidence 0.92-0.95.

**Step 4: First-column tactic lookup (no profile required).** If
first-column header is in {Product, Tactic, Channel, Platform},
iterate rows, normalize + alias, registry exact match → 1.0, fuzzy
→ 0.92. `expands_to` aliases narrow candidate pool for downstream.

**Step 5: Section + title narrowing.** Filter candidates by
`section_ctx.product_name` and `table_title`. If exactly 1
candidate remains → `NarrowingUnique, confidence=0.85`.

**Step 6: TF-IDF F1 ranking.** Score all remaining candidates.
Apply thresholds from ADR-0023 Decision 6:
- `top1 >= 0.30` AND (`margin >= 0.05` OR `relative_margin >= 0.20`) → `TfidfMatch`
- `top1 in [0.20, 0.30)` → `TfidfUncertain, flag_for_review=true`
- `top1 < 0.20` → `Unmatched`

---

### Part 5: Duplicate detection + ingestion adapter

**Duplicate detection** (post-cascade):

Fingerprint each matched table: `(sorted normalized headers, sorted
normalized first-column values, row count)`. Group by mapped
`(product_name, table_name)`. Within each group, identical
fingerprints where sections are a declared `duplicate_section_pair`
→ mark later occurrence as `Duplicate, duplicate_of=<canonical>`.

Without a declared pair, duplicate-looking tables are flagged for
review, not auto-excluded.

**Ingestion adapter:**

```rust
/// Convert matched results into ParsedCsv for the existing pipeline.
/// INVARIANT: only status=Matched results pass through.
/// Debug builds assert this.
pub fn to_parsed_csvs(results: &[MatchResult]) -> Vec<ParsedCsv>
```

This is where the ingestion invariant from ADR-0023 Decision 8 is
enforced. The function filters to `Matched` only, constructs
`ParsedCsv` with a filename that matches the registry's expected
format (so the downstream detect_tactics → ingest → narrate pipeline
works unchanged), and debug-asserts that nothing non-Matched slipped
through.

---

### Part 6: Wire into upload pipeline + diagnostics

**Update `pptx.rs`:** the existing `extract_pptx` returns
`Vec<ParsedCsv>`. Add a new `extract_pptx_tables` that returns
`Vec<ExtractedTable>` (richer struct with slide_index, table_index,
slide_title, table_title). The existing `extract_pptx` becomes a
thin wrapper for backwards compat.

**Update `upload.rs` → `process_upload`:** when `is_pptx(bytes)`:

```rust
let tables = pptx::extract_pptx_tables(bytes)?;
let profile = pptx_profile::load_profile(&cwd, "lumina-charts");
let idf = state.idf_table.clone(); // Arc<IdfTable>, built at startup
let deck_result = pptx_match::match_deck(&tables, &state.registry, profile.as_ref(), &idf);
let csvs = pptx_match::to_parsed_csvs(&deck_result.matches);
// Print diagnostic summary to terminal
eprintln!("  [pptx] {} tables: {} matched, {} skipped, {} review, {} dup",
    deck_result.stats.total_tables, deck_result.stats.auto_resolved,
    deck_result.stats.skipped, deck_result.stats.review_needed,
    deck_result.stats.duplicates);
```

Then `csvs` flows into the existing `detect_tactics → build_tactic_groups → narrate` pipeline unchanged.

**Build IdfTable at startup** in `server.rs`, alongside registry and
template loading. Store as `Arc<IdfTable>` in `AppState`.

**MC7060-MC7067 diagnostic codes:** implement as warnings/info
printed to stderr during `match_deck`. Sweep all 8 codes FREE before
implementation (grep the codebase), then ship them.

---

## Hard Rules (binding)

1. **`mc-core`, `mc-model`, `mc-fixtures`, `mc-recipe`, `mc-drivers`, `mc-tessera`, `mc-narrative` all locked.** Zero diff.
2. **`pptx_match.rs` has zero imports from axum/upload/server/workspace.** Grep-clean binding test.
3. **Only `status = Matched` flows into cube ingestion.** Debug-assert at the ingestion adapter.
4. **The existing CSV upload path is unchanged.** PPTX detection + cascade only runs when `is_pptx(bytes)` is true.
5. **v1 profile (`lumina-charts.yaml`) is checked-in and read-only.** No UI editing in this session.
6. **The IdfTable is built once at startup** and shared via `Arc`. Not rebuilt per upload.
7. **MatchEvidence is populated for every result** — even skipped/unmatched. Cascade debugging from day 1.
8. **Per-session commits.** At least 2 commits for this session.

---

## Acceptance Gates

- [ ] `cargo fmt --check --all` + `cargo clippy --all-targets --workspace -- -D warnings` + `cargo build --release --workspace` all exit 0
- [ ] `cargo test --workspace` passes (1031 → expect ~1041)
- [ ] Lumina deck (original): ≥80% auto-resolved
- [ ] Auto-resolved mappings match golden fixtures with ≥95% precision
- [ ] No table with `status ≠ Matched` ingested without review
- [ ] Lumina deck first-column lookup: "Targeted Display" → exact registry match
- [ ] Skip rule fires on "Reach & Frequency" tables
- [ ] False-positive divider guard: date strings and creative names rejected
- [ ] `grep -rn "axum\|upload::\|server::\|workspace::" crates/mc-demo-server/src/pptx_match.rs` returns 0
- [ ] Performance: `match_deck` <50ms for the Lumina deck
- [ ] MC7060-MC7067 codes swept free, then shipped
- [ ] Locked surfaces: zero diff (mc-core, mc-model, mc-fixtures, mc-recipe, mc-drivers, mc-tessera, mc-narrative)
- [ ] MatchEvidence populated on every result

---

## Test Fixtures

Use the original Lumina PPTX at `/Users/edwinlovettiii/Downloads/1778249994166_lumina_charts.pptx` as the primary fixture. Lock golden mappings for these slides:

| Slide | Expected match | Cascade step |
|---|---|---|
| 3 | Rollup → manifest declares Targeted Display | Rollup pre-pass |
| 3 table | product_overview_pivot → Targeted Display / Campaign Performance | SectionFamily + first-column |
| 5 table | product_overview_pivot → YouTube True View | SectionFamily + first-column |
| 7 table | Social pivot → Link Clicks / Sponsored Social | SectionFamily + first-column |
| 12 | Meta / Monthly Performance | SectionFamily |
| 16 | Meta / Campaign Performance | SectionFamily |
| 17 | Reach & Frequency → Skipped | SkipRule |
| 58 | Targeted Display / Monthly Performance | SectionFamily |
| 61 | Targeted Display / Campaign Performance | SectionFamily |

**Test corpus note:** if committing PPTX extracts as test data, sanitize
metric values if real customer data. Structural matching tests don't
need real numbers — synthetic preserves all signals.

---

## SPEC QUESTION candidates

- How should the extractor distinguish `table_title` from `slide_title`
  when a slide has multiple text shapes above the table? (PM default:
  nearest text shape vertically above the table; fallback to slide_title.
  Best-effort — TF-IDF compensates when it's wrong.)

- Should unmatched/review tables be included in the upload response JSON
  for the frontend to display? (PM default: yes, with `flag_for_review`
  field. The current frontend ignores them; Session B adds the review UI.)

- How to handle the SEM/Search duplicate pair if only one of the two
  sections appears in the deck? (PM default: no dedup triggered. The
  pair only fires when both sections are present in the same deck.)

---

*End of handoff. The PPTX extractor works. The cascade matcher makes
it useful. After this session, uploading a Lumina PowerPoint to the
demo produces the same narratives as uploading CSVs — auto-detected
sections, matched tactics, skipped noise, duplicates suppressed, and
full diagnostic evidence on every result.*
