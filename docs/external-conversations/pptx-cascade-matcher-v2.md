# Research Note — PPTX Cascade Matcher + Profile Schema (v2)

**Status:** Draft for review (supersedes v1)
**Date:** 2026-05-08
**Author:** Edwin Lovett III (drafted with Claude Desktop, incorporating
GPT review and additional Lumina deck samples)
**Targets:** Phase 6E or post-7A.5 follow-up
**Related:** v1 of this note; ADR-0022 (7A.5)

---

## What changed from v1

Three substantive revisions, driven by additional Lumina deck samples
that revealed the report system is **modular, not fixed**: every deck
combines a different mix of section blocks depending on which tactics
the advertiser ran.

1. **Profile schema rewritten** — `table_patterns` cross-product
   replaced with `sections` × `table_families`. Same expressiveness,
   ~⅓ the authoring burden. `table_patterns` retained as an optional
   override for genuine edge cases (pivot tables, etc.).

2. **New rollup pre-pass.** Slides 3–7 of every Lumina deck contain a
   "Product Performance" pivot table listing which tactics the deck
   contains. Reading these first as a deck manifest auto-bootstraps
   structure, eliminates false-positive divider detection, and warns
   when declared tactics have no corresponding section.

3. **Divider detection guards.** v1's "minimal-text slide = divider"
   heuristic fired false positives on date strings, creative names,
   and keywords. A divider candidate is now only valid if its title
   resolves to a registry product/subproduct (with aliases) or
   appears in the deck manifest.

Plus minor additions: aliases as a first-class block (separated into
tactic / header / registry kinds), explicit duplicate-table
detection, skip-table mechanism, and explicit deferral of
continuation-table merging past v1.

---

## Why this exists

Phase 6D shipped a PPTX text extractor (hand-rolled XML, no new deps,
~20ms for 48 tables). The extractor works. The downstream **registry
matcher** does not — derived filenames don't match the registry, and
naive header-overlap matching is dominated by shared boilerplate
columns (Date, Impressions, Clicks, CTR appear in 100+ entries).

This note specifies the replacement: a cascade matcher that uses
six signals — deck manifest, section context, table family, first-column
tactic name, narrowed candidates, IDF-weighted header scoring — in
priority order, with a profile-based learning mechanism for the
unresolved tail.

The cascade is built so it can later move into a `mc-drivers/pptx`
crate as a Tessera driver without a rewrite — keep it free of
demo-server dependencies, take registry + extracted tables as
inputs, return scored matches as output.

---

## Inputs

```rust
pub struct ExtractedTable {
    pub slide_index: u32,           // 1-based
    pub table_index: u32,           // 0-based within slide
    pub slide_title: Option<String>,
    pub table_title: Option<String>, // text immediately above the table
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub struct RegistryEntry {
    pub product_name: String,
    pub subproduct_name: String,
    pub table_name: String,
    pub file_name: String,
    pub headers: Vec<String>,        // already split on `;`
    pub sort_order: u32,
}

pub struct PptxProfile { /* see schema below */ }

pub struct DeckManifest {           // produced by the rollup pre-pass
    pub declared_tactics: Vec<DeclaredTactic>,
    pub source_slides: Vec<u32>,
}

pub struct DeclaredTactic {
    pub raw_label: String,           // "Targeted Display"
    pub normalized: String,          // "targeted display"
    pub registry_match: Option<RegistryEntry>,  // resolved via aliases
    pub source_slide: u32,
    pub source_table: u32,
    pub co_occurring: Vec<String>,   // other tactics in the same rollup pivot
}
```

The matcher returns:

```rust
pub struct MatchResult {
    pub table: ExtractedTable,
    pub mapping: Option<RegistryMapping>,
    pub confidence: f64,                     // 0.0–1.0
    pub source: MatchSource,
    pub alternatives: Vec<(RegistryMapping, f64)>,
    pub flag_for_review: bool,
    pub status: MatchStatus,                 // matched | duplicate_of | skipped | continuation_candidate
    pub duplicate_of: Option<TableRef>,
}

pub enum MatchSource {
    ProfileOverride,
    SectionFamily,           // section + table_family cross product (most common path)
    ProfilePattern,          // legacy table_pattern override
    FirstColumnExact,
    FirstColumnFuzzy,
    NarrowingUnique,
    TfidfMatch,
    TfidfUncertain,
    SkipRule,
    Unmatched,
}

pub enum MatchStatus {
    Matched,
    Duplicate,             // same data already ingested under a different section
    Skipped,               // explicit skip rule fired (KPI cards, charts, etc.)
    ContinuationCandidate, // looks like a continuation of the previous table
    Unresolved,
}
```

---

## Tokenization

Header strings need to be normalized before any matching. The
registry stores headers as `Date; Impressions; Clicks; CTR; Spend`.
The PPTX may produce `Date | Impressions | Link Clicks | CTR(Link
Click-Through Rate)`. Both must reduce to the same token set when
they refer to the same metric.

**Rules, applied in order:**

1. Split on `;` (registry) or table-cell boundaries (PPTX).
2. Strip parenthetical expressions: `CTR(%)` → `CTR`,
   `CTR (Link Click-Through Rate)` → `CTR`. Regex: `\s*\([^)]*\)`.
3. Lowercase.
4. Trim and collapse whitespace to single spaces.
5. Drop empty tokens.

**Do not:**
- Stem or lemmatize. "Click" and "Clicks" should remain distinct
  if the registry treats them so.
- Remove stop words. "Total Leads" must stay as "total leads".
- Split multi-word tokens. "Link Clicks" is one token, not two.

**Examples (verified against the actual registry):**

| Raw header                       | Normalized        |
|----------------------------------|-------------------|
| `Date`                           | `date`            |
| `CTR(%)`                         | `ctr`             |
| `CTR (Link Click-Through Rate)`  | `ctr`             |
| `Link Clicks`                    | `link clicks`     |
| `Conversions (Default)`          | `conversions`     |
| `Post Engagements`               | `post engagements`|
| `25% Completion`                 | `25% completion`  |

The same tokenization applies to first-column row values (for
tactic-name lookup) and to slide/table titles where applicable.

---

## IDF computation

The IDF table is computed once at startup over the registry. It
does not change per upload.

**Formula (smoothed IDF):**

```
idf(token) = ln((N + 1) / (df(token) + 1)) + 1
```

Where:
- `N` is the number of registry entries (currently 292).
- `df(token)` is the number of registry entries whose normalized
  header set contains `token`.

Tokens unknown to the registry (appearing in a PPTX table but
in no registry entry) get `idf = ln(N + 1) + 1` — the maximum
possible IDF.

**Computed values from the actual registry** (N = 292):

| Token              | df  | idf   | Notes                      |
|--------------------|-----|-------|----------------------------|
| `impressions`      | 290 | 1.007 | Universal, near-zero weight|
| `spend`            | 280 | 1.042 | Universal                  |
| `cpm`              | 203 | 1.362 | Common                     |
| `clicks`           | 104 | 2.026 | Common                     |
| `ctr`              | 101 | 2.055 | Common                     |
| `date`             | 62  | 2.537 | Moderate                   |
| `reach`            | 50  | 2.748 | Moderate                   |
| `conversions`      | 47  | 2.809 | Moderate                   |
| `link clicks`      | 27  | 3.348 | Discriminating             |
| `cplc`             | 27  | 3.348 | Discriminating             |
| `foot traffic`     | 9   | 4.378 | Highly discriminating      |
| `post engagements` | 8   | 4.483 | Highly discriminating      |
| `open rate`        | 6   | 4.734 | Highly discriminating      |
| `watch time`       | 3   | 5.294 | Near-unique                |
| `premium content`  | 1   | 5.987 | Unique → strong fingerprint|

This is the disambiguation engine: rare headers carry the signal,
common ones are noise.

---

## Per-table scoring

For a candidate registry entry vs. an extracted table, compute
**IDF-weighted F1** over the normalized token sets.

```
T = table.normalized_tokens
E = entry.normalized_tokens
I = T ∩ E
inter_idf = sum(idf(t) for t in I)
table_idf = sum(idf(t) for t in T)
entry_idf = sum(idf(t) for t in E)

recall    = inter_idf / entry_idf
precision = inter_idf / table_idf
f1        = 2 * recall * precision / (recall + precision)   // 0 if either is 0
```

**Why F1 not Jaccard.** PPTX exports often show a *subset* of the
registry's full header list. Jaccard penalizes both directions
equally, which punishes the right answer when the table is a subset.
F1 is more forgiving when one side is shorter.

**Score range and interpretation.** With real data, F1 lands in
the 0.2–0.5 range for correct matches. Don't use absolute
thresholds; use *margin* between top1 and top2.

---

## Rollup pre-pass — building the deck manifest

Before any per-table matching, scan the first 10 slides for "Product
Performance" pivot tables. These declare which tactics the deck
contains and act as ground truth for downstream divider detection.

**Detection rule:** a slide hosts a rollup pivot if all hold:
1. Slide title contains "Overall Performance" OR "Top KPIs"
2. The slide has at least one table with a first-column header in
   `{Product, Tactic, Channel, Platform}` (case-insensitive)
3. That table has 2+ data rows

**For each rollup pivot found:**
1. Extract first-column values (skip header row)
2. Apply tactic aliases (see `aliases.tactic` in the profile)
3. Look up each in the registry by `subproduct_name` (exact, then
   fuzzy)
4. Record as a `DeclaredTactic` with co-occurring values for context

**The manifest's purposes:**
- **Validate divider detection.** Only allow section dividers whose
  titles resolve to a tactic that appears in the manifest (or in
  the registry with high alias confidence).
- **Auto-bootstrap profiles.** When no profile exists for a deck,
  the manifest becomes the proposed `sections` block — user
  confirms in the review UI, profile is saved.
- **Coverage check.** After per-table matching, warn if any
  manifest tactic has no corresponding section divider downstream
  (declared but not delivered) or any section divider is found
  whose tactic isn't in the manifest (delivered but not declared).

**Verified against the three Lumina decks:**

| Deck | Slide 3 rollup tactics                                      | Section dividers found                                  |
|------|-------------------------------------------------------------|---------------------------------------------------------|
| 1    | Addressable Display, Targeted Native                        | Addressable Display, Meta, E-Mail, Search & Intent Media, SEM, Targeted Native |
| 2    | Targeted Display, Addressable Display, Geo-Fencing w/ Foot Traffic | Addressable Display, Call Performance, Geo-Fencing w/Foot Traffic, Search & Intent Media, SEM, Targeted Display |
| 3    | Targeted Display, Geo-Fencing w/ Foot Traffic               | Meta, Geo-Fencing w/Foot Traffic, Sponsored Social Mentions, Streaming TV, Targeted Display |

The display rollup is one of several rollups (Display Ads, Social
Ads, SEM Ads, STV Ads, E-mail Marketing). Each rollup contributes
its tactics to a combined manifest. Note that some sections (Meta,
SEM, E-Mail) appear as dividers without being in a Display rollup —
they're declared by the Social/SEM/E-Mail rollups instead.

---

## Section divider detection (with guards)

A slide qualifies as a section divider if and only if:

1. **Structural**: the slide has zero data tables (KPI-card layouts
   with single-cell tables don't count as data tables) AND fewer
   than ~15 lines of text.
2. **Title length**: slide title is non-empty and shorter than 60
   characters.
3. **Resolution** (at least one of):
   - a. Title resolves to a profile-declared section name (after
        normalization + aliases)
   - b. Title resolves to a registry `product_name` or
        `subproduct_name` (after `aliases.tactic`)
   - c. Title appears in the deck manifest as a declared tactic

If only conditions 1 and 2 hold but 3 doesn't, the slide is treated
as ordinary content — no section context is propagated. This
prevents date strings ("01/04/2026"), creative names
("Day_Video_04.20-2026"), and keywords ("solar panel cost") from
being mistaken for section dividers.

**Propagation rule:** once detected, a divider's section context
applies to all subsequent slides until the next divider. Section
context includes `product_name` and `default_subproduct` (see
schema below).

**Mid-section overview slides** (e.g., "Addressable Display
Overview" appearing after the "Addressable Display" divider) do
NOT establish a new section. They're handled by the section's
`title_matchers` block as additional within-section identifiers
without resetting context.

---

## Cascade algorithm

For each extracted table, run these steps in order. Stop and
return as soon as a step produces a result with confidence ≥ 0.6
(or as marked).

```
INPUTS: table T, profile P (may be empty), registry R, idf table, manifest M

# Step 0: Skip rule
if P.skip_tables matches (T.slide_index, T.table_index, T context):
    return MatchResult(status=Skipped, source=SkipRule, confidence=1.0)

# Step 1: Profile override (highest priority)
if P has override for (T.slide_index, T.table_index):
    return MatchResult(
        mapping = override.mapping,
        confidence = 1.0,
        source = ProfileOverride,
        status = Matched
    )

# Step 2: Continuation-candidate detection
if T looks like a continuation of the previous slide's table
   (same section context, no header row, column count match):
    return MatchResult(
        status = ContinuationCandidate,
        flag_for_review = true,
        source = Unmatched   // user decides whether to merge in review UI
    )

# Step 3: Build context
section_ctx = nearest preceding divider's propagated context, if any
slide_title = T.slide_title
table_title = T.table_title

# Step 4: Section + Table-family cross product (the common path)
if section_ctx is set:
    family = first table_family in P whose title_matchers match
             (slide_title, table_title)
    if family found:
        if family.use_first_column_lookup:
            for row in T.rows:
                value = apply_aliases(normalize(row[0]), P.aliases.tactic)
                lookup = R.find_by_subproduct(value, fuzzy=True)
                if lookup:
                    return MatchResult(
                        mapping = (section_ctx.product_name,
                                   lookup.subproduct_name,
                                   family.table_name),
                        confidence = 0.95,
                        source = SectionFamily,
                        status = Matched
                    )
            // Fall through if no row matched
        else:
            return MatchResult(
                mapping = (section_ctx.product_name,
                           section_ctx.default_subproduct,
                           family.table_name),
                confidence = 0.92,
                source = SectionFamily,
                status = Matched
            )

# Step 5: Profile pattern (legacy override; rarely used in v2)
for pattern in P.table_patterns:
    if pattern.when matches context:
        // (apply pattern as in v1)
        return MatchResult(...)

# Step 6: First-column tactic lookup (no profile required)
if T.first_column_header.lower() in {"product", "tactic", "channel", "platform"}:
    for row in T.rows:
        value = apply_aliases(normalize(row[0]), P.aliases.tactic)
        match = R.find_by_subproduct_exact(value)
        if match: return MatchResult(confidence=1.0, source=FirstColumnExact, ...)
        match = R.find_by_subproduct_fuzzy(value)
        if match: return MatchResult(confidence=0.92, source=FirstColumnFuzzy, ...)

# Step 7: Section + table_name narrowing (no family hit)
candidates = R.entries
if section_ctx: candidates = [e for e in candidates if e.product_name == section_ctx.product_name]
if table_title:
    narrowed = [e for e in candidates if e.table_name.lower() in table_title.lower()
                                       or table_title.lower() in e.table_name.lower()]
    if narrowed: candidates = narrowed
if len(candidates) == 1:
    return MatchResult(mapping=candidates[0], confidence=0.85, source=NarrowingUnique)

# Step 8: TF-IDF F1 ranking
if not candidates: candidates = R.entries
scored = [(score_f1(T, c, idf), c) for c in candidates]
scored.sort(desc by score)

top1_score, top1_entry = scored[0]
top2_score = scored[1].score if len(scored) > 1 else 0.0
margin = top1_score - top2_score
relative_margin = margin / top1_score if top1_score > 0 else 0

if top1_score >= 0.30 and (margin >= 0.05 or relative_margin >= 0.20):
    return MatchResult(
        mapping = top1_entry,
        confidence = top1_score,
        source = TfidfMatch,
        alternatives = scored[1..3],
        status = Matched
    )
else:
    return MatchResult(
        mapping = top1_entry if top1_score >= 0.20 else None,
        confidence = top1_score,
        source = TfidfUncertain if top1_score >= 0.20 else Unmatched,
        flag_for_review = true,
        alternatives = scored[0..3],
        status = Unresolved
    )
```

**Threshold rationale (informed by actual deck data):**
- `top1 >= 0.30`: empirical floor for "this is plausibly correct."
- `margin >= 0.05` (absolute) OR `relative_margin >= 0.20`: at low
  absolute scores, a 20% relative gap matters more than a 0.05
  absolute gap; both accepted.
- `top1 in [0.20, 0.30)`: surface as best guess but flag for review.
- `top1 < 0.20`: unmatched.

Thresholds are configurable per profile.

---

## Duplicate-table detection

Some decks emit the same data under different section names.
Verified case: deck 1 has both a "Search & Intent Media" section
(slide 55) AND a "SEM" section (slide 72), each with its own
"Search Engine Marketing (SEM) Overview" / Monthly Performance /
Campaign Performance / Keyword Performance tables. Without dedup,
the cube ingests the same SEM data twice.

**Fingerprint:** for each successfully matched table, compute:

```
fingerprint = (
    sorted(normalized_headers),
    sorted(normalized_first_column_values),
    row_count
)
```

**Dedup rule:**

1. Group all matched tables by mapped `(product_name, table_name)`.
2. Within each group, find tables with identical fingerprints.
3. The first occurrence (lowest slide_index) is canonical.
4. Subsequent occurrences are marked
   `status = Duplicate, duplicate_of = <canonical_ref>` and excluded
   from cube ingestion.

**Why all three fingerprint components:**
- Headers alone: too lax (every monthly table has the same headers).
- Headers + first-column values: catches reordered rows but misses
  truncation differences.
- Adding row_count: catches the case where one section has 12 rows
  and the other has 10 (different time windows shown). These should
  NOT be considered duplicates.

**Section-pair awareness.** Optionally, profiles can declare
`duplicate_section_pairs` so dedup only applies between known
duplicate sections — not across unrelated sections that happen to
have similar data:

```yaml
duplicate_section_pairs:
  - sections: [search_intent_media, sem]
    note: |
      Lumina decks with both blocks contain the same SEM data;
      keep the Search & Intent Media occurrence as canonical.
    canonical: search_intent_media
```

When a `canonical` is specified, it overrides the lowest-slide-index
rule.

---

## Profile schema (v2)

Profiles live at `.mosaic/pptx-profiles/<profile-id>.yaml`.

### Full example

```yaml
schema_version: "2.0"
profile_id: lumina-charts
description: |
  PulseMax / Lumina Charts PPTX export template. Modular product-section
  blocks; each section combines an Overview slide, optional Monthly,
  Campaign, Tactic, Creative, Geo, Device, Demographic, and Conversion
  tables in varying combinations.
created_at: "2026-05-08T15:00:00Z"
created_by: edwin
last_updated: "2026-05-08T15:30:00Z"
upload_count: 0

# Optional confidence threshold overrides
thresholds:
  auto_match_min_score: 0.30
  auto_match_min_margin: 0.05
  auto_match_min_relative_margin: 0.20
  flag_for_review_min_score: 0.20

# Aliases — three kinds, applied in different cascade steps
aliases:
  # Tactic-name normalization (used by first-column lookup AND rollup parsing)
  tactic:
    - input: "Link Clicks"
      canonical: "Link Click"
    - input: "Geo-Fencing w/ Foot Traffic"
      canonical: "Geofencing with Foot Traffic"
    - input: "Geo-Fencing w/Foot Traffic"
      canonical: "Geofencing with Foot Traffic"
    - input: "Sponsored Social Mentions"
      # Ambiguous — expands to multiple registry candidates;
      # downstream cascade narrows by table family + headers
      expands_to:
        - "Sponsored Social Mentions - Link Click"
        - "Sponsored Social Mentions - Awareness"
        - "Sponsored Social Mentions - ThruPlay"

  # Header-token normalization (applied before tokenization)
  header:
    - input: "campaign"
      canonical: "campaign name"
    - input: "platform"
      canonical: "platform"

  # Registry duplicates — different registry entries that should be
  # treated as the same logical tactic
  registry:
    - canonical:
        product_name: "Meta"
        subproduct_name: "Facebook - Link Click"
      duplicates_of:
        - { product_name: "Meta", subproduct_name: "Link Click" }

# Sections — what propagates as context after a divider
sections:
  - id: meta
    title_matchers:
      - equals: "Meta"
      - starts_with: "Facebook Overview"
    propagates:
      product_name: "Meta"
      default_subproduct: "Facebook - Link Click"

  - id: addressable_display
    title_matchers:
      - equals: "Addressable Display"
      - starts_with: "Addressable Display Overview"
    propagates:
      product_name: "Blended Tactics"
      default_subproduct: "Addressable Display"

  - id: targeted_display
    title_matchers:
      - equals: "Targeted Display"
      - starts_with: "Targeted Display Overview"
    propagates:
      product_name: "Blended Tactics"
      default_subproduct: "Targeted Display"

  - id: targeted_native
    title_matchers:
      - equals: "Targeted Native"
      - starts_with: "Targeted Native Overview"
    propagates:
      product_name: "Blended Tactics"
      default_subproduct: "Targeted Native"

  - id: geofencing
    title_matchers:
      - equals_any: ["Geo-Fencing w/Foot Traffic", "Geo-Fencing w/ Foot Traffic"]
      - starts_with: "Geo-Fencing w/Foot Traffic Overview"
    propagates:
      product_name: "Addressable Solutions"
      default_subproduct: "Geofencing with Foot Traffic"

  - id: search_intent_media
    title_matchers:
      - equals_any: ["Search & Intent Media", "SEM"]
      - contains: "Search Engine Marketing"
    propagates:
      product_name: "SEM"
      default_subproduct: "SEM"

  - id: sponsored_social
    title_matchers:
      - equals: "Sponsored Social Mentions"
    propagates:
      product_name: "SSM"
      default_subproduct: "Sponsored Social Mentions - Link Click"

  - id: email_marketing
    title_matchers:
      - equals_any: ["E-Mail Marketing", "E-mail Marketing"]
      - starts_with: "E-Mail Marketing Overview"
    propagates:
      product_name: "Email Marketing"
      default_subproduct: "1:1 Marketing"

  - id: streaming_tv
    title_matchers:
      - equals_any: ["Streaming TV", "STV"]
    propagates:
      product_name: "STV"
      default_subproduct: "Streaming TV OTT"

  - id: call_performance
    title_matchers:
      - equals: "Call Performance"
    propagates:
      product_name: "Call Performance"
      default_subproduct: "Call Recording"

# Table families — what kind of table this is
# Cross-product with sections gives the (product, subproduct, table_name)
table_families:
  - id: monthly_performance
    title_matchers:
      - contains: "Monthly Performance"
    table_name: "Monthly Performance"

  - id: campaign_performance
    title_matchers:
      - contains: "Campaign Performance"
    table_name: "Campaign Performance"

  - id: tactic_performance
    title_matchers:
      - contains: "Tactic Performance"
    table_name: "Tactic Performance"

  - id: creative_performance
    title_matchers:
      - contains_any: ["Creative Performance", "Creative By Name", "Creative By Size"]
    table_name: "Creative Performance"

  - id: geo_performance
    title_matchers:
      - contains_any: ["Geo Performance", "Performance by City", "Performance by Zip"]
    table_name: "Geo Performance"

  - id: device_performance
    title_matchers:
      - contains: "Device Performance"
    table_name: "Device Performance"

  - id: demographic_performance
    title_matchers:
      - contains: "Demographic Performance"
    table_name: "Demographic Performance"

  - id: conversion_performance
    title_matchers:
      - contains: "Conversion Performance"
    table_name: "Conversion Performance"

  # Pivot table family — first-column values become the subproduct
  - id: product_overview_pivot
    title_matchers:
      - contains_any: ["Product Performance", "Product Performance (Display)",
                        "Product Performance (Video)"]
    use_first_column_lookup: true
    first_column_header_in: ["Product", "Tactic"]
    table_name: "Campaign Performance"   # what pivot rows map INTO
    note: |
      Used by Display - Product Performance, Video - Product Performance,
      and STV - Product Performance pivots in the rollup slides. Each
      row's first-column value resolves to a subproduct via aliases +
      registry lookup.

# Skip rules — tables to ignore entirely
skip_tables:
  - when:
      table_title_contains_any: ["Reach & Frequency", "Search Impression Share",
                                  "CTR Last 6 Months", "VCR Last 6 Months",
                                  "Top KPIs", "Key Performance Indicators"]
    reason: "summary or chart-only slides; not data tables"

  - when:
      slide_title_contains: "Calls by Day of Week"
    reason: "chart aggregation, not a tactic table"

# Duplicate-section pairs — known overlap that should be deduped
duplicate_section_pairs:
  - sections: [search_intent_media]
    matches_section_titles: ["SEM"]
    canonical: search_intent_media
    note: |
      Some Lumina decks contain both a Search & Intent Media block and
      a separate SEM block with the same data. Keep the Search & Intent
      Media occurrence as canonical.

# Hard overrides — pinned mappings for specific (slide, table) pairs
overrides: []

# Legacy table_patterns — kept for v1 compatibility, rarely used in v2
table_patterns: []

# Statistics — written by the matcher, not by hand
stats:
  total_uploads: 0
  tables_seen: 0
  auto_resolved: 0
  user_corrected: 0
  unresolved: 0
  duplicates_suppressed: 0
  by_source:
    SectionFamily: 0
    FirstColumnExact: 0
    FirstColumnFuzzy: 0
    NarrowingUnique: 0
    TfidfMatch: 0
    TfidfUncertain: 0
    SkipRule: 0
    Unmatched: 0
```

### Field semantics — `title_matchers`

Each section and table_family declares one or more `title_matchers`.
A matcher is satisfied if ANY of its keys match. Across multiple
matchers in the same list, ANY match counts. Available keys:

| Key                | Meaning                                          |
|--------------------|--------------------------------------------------|
| `equals`           | Normalized title equals the literal              |
| `equals_any`       | Normalized title equals any literal in the list  |
| `starts_with`      | Normalized title begins with the literal         |
| `contains`         | Normalized title contains the literal            |
| `contains_any`     | Normalized title contains any literal in list    |
| `regex`            | Title matches the regex (for power users)        |

Title normalization: lowercase, strip whitespace, collapse spaces,
strip surrounding punctuation. Section titles also pass through
`aliases.tactic` for canonical resolution.

### Field semantics — `sections`

**`title_matchers`** identifies what counts as this section's
divider OR an in-section overview slide. Both update context
without re-triggering divider logic.

**`propagates.product_name`** is required. **`default_subproduct`**
is used by the SectionFamily matcher when no first-column lookup is
needed. For sections where multiple subproducts coexist (e.g.,
"Sponsored Social Mentions" expanding to Link Click / Awareness /
ThruPlay), set `default_subproduct` to the most common variant and
let the cascade's later steps (TF-IDF, first-column lookup) refine.

### Field semantics — `table_families`

**`use_first_column_lookup`** — when true, rows of the table each
become individual matches; the family supplies the `table_name` and
the section supplies the `product_name`, but `subproduct_name` comes
from the row's first-column value (after aliases + registry lookup).

**`first_column_header_in`** — restricts the family to tables whose
first-column header is one of the listed strings. Prevents a "Geo
Performance" family from accidentally matching a "City" demographic
table that happens to have "Geo" in the title.

### Field semantics — `aliases`

**`aliases.tactic`** has two output forms: `canonical` (single
target) and `expands_to` (multiple candidates). Single-target wins
immediately. Expanded aliases require downstream cascade narrowing
to disambiguate.

**`aliases.header`** is applied during tokenization, before IDF
scoring. Use sparingly — over-aggressive header aliasing risks
false matches (e.g., merging "Click" and "Clicks").

**`aliases.registry`** patches registry data quality issues. The
`canonical` is preferred when scoring ties.

### Field semantics — `skip_tables` and `duplicate_section_pairs`

Both new in v2. Skip rules win over everything (they're Step 0 of
the cascade). Duplicate-section pairs only apply after successful
matching — they don't prevent matching, they prevent ingestion of
the second occurrence.

---

## Worked examples (from the actual Lumina decks)

### Example 1 — Deck 2, Slide 3 rollup → manifest

**Extracted:**
- slide_title: "Display Ads - Overall Performance"
- table_title: "Display - Product Performance"
- headers: `Product, Impressions, Clicks, CTR(%), Total Conversions`
- rows:
  - `Targeted Display, 694727, 9869, 1.42, 464`
  - `Addressable Display, 172987, 269, 0.16, 47`
  - `Geo-Fencing w/ Foot Traffic, 166767, 164, 0.1, 21`

**Rollup pre-pass:**

1. Slide title contains "Overall Performance" ✓
2. Table has first-column header "Product" ✓
3. 3 data rows ≥ 2 ✓ → qualifies as rollup pivot.
4. Apply `aliases.tactic` to each first-column value:
   - "Targeted Display" → no alias, lookup as-is
   - "Addressable Display" → no alias, lookup as-is
   - "Geo-Fencing w/ Foot Traffic" → canonical "Geofencing with Foot Traffic"
5. Registry lookup by subproduct_name (exact, then fuzzy):
   - "Targeted Display" ✓ matches `Blended Tactics / Targeted Display`
   - "Addressable Display" ✓ matches `Blended Tactics / Addressable Display`
   - "Geofencing with Foot Traffic" ✓ matches `Addressable Solutions / Geofencing with Foot Traffic`

**Manifest output:**

```
declared_tactics:
  - { Targeted Display, blended-tactics/targeted-display, slide=3 }
  - { Addressable Display, blended-tactics/addressable-display, slide=3 }
  - { Geofencing with Foot Traffic, addressable-solutions/geofencing-..., slide=3 }
```

**Coverage check:** later in the deck, dividers are found at slide 7
(Addressable Display), slide 31 (Geo-Fencing w/Foot Traffic), and
slide 96 (Targeted Display). All three manifest tactics have
corresponding sections — no warning.

### Example 2 — Deck 2, Slide 11 → SectionFamily match

**Context (from deck 2 walk):**
- Slide 7 divider: "Addressable Display" → propagates
  `product_name: Blended Tactics, default_subproduct: Addressable Display`.
- Slide 11 is in the Addressable Display section.

**Extracted (slide 11):**
- slide_title: "Addressable Display Overview"
- table_title: "Monthly Performance"
- headers: `Date, Impressions, Clicks, Foot Traffic Visits, Total Conversions`

**Cascade:**

1. Skip rule? No.
2. Profile override? No.
3. Continuation candidate? No.
4. Build context: `section_ctx = (Blended Tactics, Addressable Display)`.
5. Find matching table_family: `monthly_performance`
   (`title_matchers.contains: "Monthly Performance"` ✓).
6. `use_first_column_lookup: false` → use defaults from section.
7. Compose mapping:
   `(product_name="Blended Tactics", subproduct_name="Addressable Display", table_name="Monthly Performance")`.

**Result:** mapping resolved with confidence = 0.92, source =
`SectionFamily`, status = `Matched`. No need to enter the TF-IDF
fallback at all.

### Example 3 — Deck 1, SEM duplicate suppression

**Context:** deck 1 has both a "Search & Intent Media" section
(slide 55) and an "SEM" section (slide 72). Per profile,
`duplicate_section_pairs` lists these as canonical-aliased.

**Tables matched (per cascade):**
- Slide 56 / Search & Intent Media / Monthly Performance →
  matched as `(SEM, SEM, Monthly Performance)`.
- Slide 73 / SEM / Monthly Performance → matched as
  `(SEM, SEM, Monthly Performance)`.

**Dedup pass:**

1. Group by mapped key `(SEM, SEM, Monthly Performance)`.
2. Compute fingerprints:
   - Slide 56: `(['date','impressions','clicks','ctr','spend',...], ['01/2026','02/2026',...], 6)`
   - Slide 73: same fingerprint.
3. Sections involved: `[search_intent_media, sem]` matches the
   declared duplicate pair, with canonical = `search_intent_media`.
4. Slide 56 is in `search_intent_media` → kept as canonical.
5. Slide 73 marked `status = Duplicate, duplicate_of = slide_56`.
   Excluded from cube ingestion.

### Example 4 — Out-of-domain table (Reach & Frequency)

**Extracted:**
- slide_title: "Reach & Frequency"
- table_title: "Reach & Frequency"
- headers: `Tactic, Impressions, Reach, Frequency`

**Cascade:**

1. Skip rule fires: `table_title_contains_any: ["Reach & Frequency", ...]`
   matches.
2. Return `status = Skipped, source = SkipRule, confidence = 1.0`.

The table is excluded from ingestion silently — no review prompt,
no false-positive ingestion, no TF-IDF noise. This is the correct
v2 behavior; v1 would have flagged it for review on every upload.

### Example 5 — False-positive divider rejected

**Context:** deck 3 has slide 21 with title-only content
"Day_Video_04.20-2026" (a creative name shown alone on a slide).

**Divider detection:**

1. Structural: zero data tables, < 15 lines of text ✓
2. Title length: < 60 chars ✓
3. Resolution check:
   - Profile section match? No (no section has that title).
   - Registry product/subproduct match? No.
   - Deck manifest match? No.
4. Resolution fails → slide is treated as content, not a divider.
   Section context from the previous divider (Meta) continues to
   propagate.

This is the v2 fix for the false-positive divider problem.

---

## Edge cases and open questions

**Q1: Sections with non-canonical product names.** The Lumina deck
uses "Sponsored Social Mentions" as a section title, but the
registry has no `subproduct_name` exactly matching that — only
`Sponsored Social Mentions - Link Click`, `... - Awareness`, etc.
Resolution: section `propagates.default_subproduct` is set to the
most common variant; downstream first-column lookup or TF-IDF
narrows to the right one when the table contains discriminating
data.

**Q2: Header mismatch between deck and registry** ("Campaign" vs.
"Campaign Name"). Resolved by `aliases.header` if it becomes
prevalent; until then, the TF-IDF fallback handles it imperfectly
but acceptably (top1 stays correct, just with lower margin).

**Q3: Manifest-declared tactic with no corresponding section.** A
deck might declare "Targeted Display" in the rollup but have no
"Targeted Display" divider downstream (e.g., the deck was truncated
or the section block is missing). Coverage check warns; the data
might still be ingested if a SectionFamily match fires from another
section — but the warning surfaces the inconsistency.

**Q4: Section divider appearing without manifest backing.** A deck
section appears (e.g., "Spark") that wasn't in any rollup pivot.
Treat as a real section if it matches a profile section or
registry product; warn if it matches neither and let TF-IDF do its
work.

**Q5: Continuation tables.** Detection ships in v1 (Step 2 of the
cascade flags them). Auto-merging does NOT ship in v1 — the
"merge?" prompt is a Phase 7C concern. For the demo, continuations
flow into the review queue; the user can manually copy rows
between tables if needed. This is intentional: silent auto-merge
of misidentified continuations is a data corruption risk worse
than missing some rows.

**Q6: PPTX header tokens not in the registry corpus.** "Total
Leads" appears in the deck but not in any registry headers
(verified). These tokens get max IDF (= max possible). Their
absence from any candidate's `entry_tokens` means they don't help
disambiguate; their presence in `table_tokens` modestly lowers
precision for all candidates. This is correct behavior — the
matcher shouldn't pretend an unknown header is meaningful signal.

**Q7: Two registry entries with identical headers.** Real (Meta /
Link Click vs. Meta / Facebook - Link Click). Handled by
`aliases.registry`. PM default: when scores tie within 0.001, the
canonical (per `aliases.registry`) wins.

**Q8: Multilingual decks.** Out of scope. Tokenization assumes
English headers and section titles.

**Q9: Where does the profile live in the workspace?**
`.mosaic/pptx-profiles/<profile-id>.yaml`. One file per template.
Profile selection at upload time can be:
   (a) explicit (`mc model upload --profile lumina-charts`),
   (b) automatic via filename pattern (configured in profile),
   (c) automatic via slide-1 fingerprint match across stored profiles.

Auto-selection is a v2.5 concern; v1 ships explicit selection.

---

## Implementation notes

**Where this lives.** Build it as `crates/mc-narrative/src/pptx_match.rs`
or a small adjacent module for v1. When it stabilizes, lift to
`crates/mc-drivers/pptx/`. Keep the public API small:

```rust
pub fn match_deck(
    tables: &[ExtractedTable],
    registry: &Registry,
    profile: Option<&PptxProfile>,
    idf: &IdfTable,
) -> DeckMatchResult

pub struct DeckMatchResult {
    pub manifest: DeckManifest,
    pub matches: Vec<MatchResult>,
    pub coverage_warnings: Vec<CoverageWarning>,
}
```

`match_deck` is the entry point. Internally it runs the rollup
pre-pass, divider detection, per-table cascade, and dedup pass in
that order. The IDF table is pure data; build it once at registry
load and pass it in.

**Test fixtures.** All four Lumina decks are natural regression
fixtures (the original PulseMax deck plus the three new ones).
Tag the expected mappings for representative slides per deck and
lock them as golden tests. Suggested coverage:

| Deck | Slides to lock                                      |
|------|-----------------------------------------------------|
| Original | 3 (rollup), 5 (Video pivot), 7 (Social pivot), 12 (Facebook Monthly), 16 (Facebook Campaign) |
| 1 (627917) | 3 (rollup), 11 (AD Monthly), 56 + 73 (SEM dup) |
| 2 (792946) | 3 (rollup, 3-tactic), 11 (AD Monthly), 31 (Geo divider), 96 (TD divider) |
| 3 (959819) | 3+4+5 (rollups), 7 (Meta divider), 21 (false-positive divider rejected) |

**Performance.** The rollup pre-pass is O(slides). Divider
detection is O(slides). Per-table cascade is O(tables ×
candidates), with section narrowing typically reducing candidates
to <10. A 100-slide deck with ~50 tables runs in <50ms total. No
async needed.

**Diagnostic codes (suggested):**

| Code   | Condition                                              |
|--------|--------------------------------------------------------|
| MC7060 | Profile schema version mismatch                        |
| MC7061 | Profile section/family references unknown registry tactic |
| MC7062 | Override references slide_index that doesn't exist     |
| MC7063 | Profile alias canonical not found in registry          |
| MC7064 | Many tables fell through to TfidfUncertain (≥30%) — profile may need updating |
| MC7065 | Manifest-declared tactic has no corresponding section divider |
| MC7066 | Section divider found whose tactic isn't in the manifest |
| MC7067 | Duplicate-section-pair declared but no duplicates found across uploads |

---

## Sequencing recommendation

**Pre-7A.5 (1 short session):**
- Tokenization, IDF, F1 scoring (unchanged from v1).
- Cascade Steps 0–8 with section/table_family resolution.
- Rollup pre-pass building `DeckManifest`.
- Divider detection with all three guards.
- Skip rules.
- No UI yet; output goes to log + best-effort match.
- Lumina deck regression fixtures with golden mappings (table above).

**Post-7A.5 (1 session):**
- Profile YAML loader/writer for the v2 schema.
- "Unmatched + ContinuationCandidate + Unresolved" review panel in
  demo UI with top-3 dropdown and "skip" / "merge with previous"
  affordances.
- Save user confirmations to profile (sections, families, skip
  rules, aliases all writable from UI).
- Stats block telemetry.
- Coverage warnings surfaced in upload result.

**Phase 7C territory:**
- Tessera driver wrapper (`mc-drivers/pptx`) consuming the same
  matcher module.
- Continuation-table auto-merge (after observing real-world false
  positive/negative rates).
- Profile auto-selection by deck fingerprinting.
- Python extractor scripts as the escape hatch for non-Lumina decks
  the cascade can't handle.
- Profile sharing / publishing (community profiles for popular
  reporting platforms).

---

*End of v2 research note. Ready for review and Claude Code handoff.*
