# ADR-0023 — PPTX Cascade Matcher + Profile System

**Status:** Accepted (2026-05-08)  
**Date:** 2026-05-08  
**Author:** Edwin Lovett III  
**Depends on:** Phase 6D (demo MVP), PPTX table extractor (shipped in `mc-demo-server/src/pptx.rs`)  
**Research note:** [`../external-conversations/pptx-cascade-matcher-v2.md`](../external-conversations/pptx-cascade-matcher-v2.md) (full spec with worked examples)

---

## Context

Phase 6D shipped a PPTX table extractor that pulls tables from PowerPoint slides in ~20ms with no external dependencies. The extractor works. The downstream **registry matcher** does not — derived filenames don't match the registry, and naive header-overlap matching is dominated by shared columns (Date, Impressions, Clicks, CTR appear in 100+ of 292 registry entries).

Analysis of four Lumina PPTX decks revealed three signals the matcher wasn't using: first-column tactic names (ground truth for pivot tables), section dividers (propagate product context to subsequent slides), and the discriminating power of rare headers vs. common ones. Additionally, the report system is **modular**: every deck combines a different mix of product-section blocks depending on which tactics the advertiser ran. Slide-index mapping is therefore the wrong primary strategy.

This ADR specifies a cascade matcher with 7 ordered steps (skip gate + 6 matching/review signals), a profile system for persistent learning, and an explicit duplicate/continuation/skip handling strategy with a binding ingestion invariant.

---

## Decisions

### Decision 1: Cascade architecture (7 ordered steps: skip gate + 6 matching/review signals)

The matcher runs these steps in order for each extracted table. The first step that produces a result above the confidence threshold wins.

| Step | Signal | Confidence | Typical coverage |
|---|---|---|---|
| 0 | **Skip rule** — table matches a skip pattern (KPI cards, charts) | 1.0 | ~15% of slides |
| 1 | **Profile override** — pinned (slide_index, table_index) mapping | 1.0 | Rare (edge cases only) |
| 2 | **Continuation candidate** — same section, no header row, column count match | flags for review | ~5% of tables |
| 3 | **Section + table-family cross-product** — divider context × family title match | 0.92 | ~60% of tables |
| 4 | **First-column tactic lookup** — Product/Tactic/Channel column → registry | 0.92-1.0 | ~10% (pivot tables) |
| 5 | **Section + title narrowing** — product context + table title narrows to 1 candidate | 0.85 | ~5% |
| 6 | **TF-IDF weighted F1** — IDF-scored header overlap among remaining candidates | 0.20-0.50 | ~5% |

Tables below all thresholds go to the review queue (status = Unresolved, flag_for_review = true).

**Why cascade, not a single scoring function:** different signals have different reliability. A first-column exact match ("Targeted Display" in the Product column) is ground truth — confidence 1.0. TF-IDF header scoring is a probabilistic fallback — confidence 0.3-0.5. Mixing them into one formula wastes the high-confidence signals. The cascade lets each signal fire at its natural confidence level.

---

### Decision 2: Profile schema — `sections × table_families`

Profiles decouple **what tactic** (section) from **what kind of table** (family). The cross-product of sections and families produces the full mapping space.

```yaml
sections:
  - id: meta
    title_matchers:
      - equals: "Meta"
    propagates:
      product_name: "Meta"
      default_subproduct: "Facebook - Link Click"

table_families:
  - id: monthly_performance
    title_matchers:
      - contains: "Monthly Performance"
    table_name: "Monthly Performance"
```

A table in the Meta section with "Monthly Performance" in its title maps to `(Meta, Facebook - Link Click, Monthly Performance)` — 13 profile entries produce the same coverage as 42 flat mappings.

**Profiles live at `.mosaic/pptx-profiles/<profile-id>.yaml`.** One file per reporting template. Selection is explicit in v1 (`--profile lumina-charts`); auto-selection by deck fingerprint is Phase 7C.

**Why not flat `table_patterns` only:** the Lumina report system is modular — 7 sections × 6 table families per deck, different mix per advertiser. Flat patterns require authoring every combination; the cross-product requires authoring each dimension once.

---

### Decision 3: Rollup pre-pass — deck manifest

Before per-table matching, scan the first 10 slides for "Product Performance" / "Overall Performance" pivot tables. These declare which tactics the deck contains.

**Detection rule:** slide title contains "Overall Performance" OR "Top KPIs", AND the slide has a table with first-column header in {Product, Tactic, Channel, Platform}, AND 2+ data rows.

**The manifest serves three purposes:**
1. Validates divider detection — only allow dividers whose titles resolve to manifest tactics
2. Auto-bootstraps profiles — when no profile exists, the manifest proposes the sections list
3. Coverage check — warns when declared tactics have no section, or sections appear that aren't in the manifest

**Empty manifest is not an error.** If a deck has no rollup pivots in the first 10 slides, the manifest is empty. Divider detection still operates via the registry and profile resolution paths — the manifest is one signal, not a gate. The cascade produces correct results for non-Lumina decks that lack rollup slides; it just has one fewer signal to work with.

**Why:** the rollup is the deck telling you what it contains. Ignoring it and reverse-engineering structure from dividers does extra work and produces false positives.

---

### Decision 4: Divider detection with resolution guards

A slide is a section divider if and only if:

1. **Structural:** zero data tables AND <15 lines of text
2. **Title length:** non-empty and <60 characters
3. **Resolution (at least one):**
   - Title matches a profile-declared section name (after normalization + aliases)
   - Title matches a registry product_name or subproduct_name (after aliases.tactic)
   - Title appears in the deck manifest as a declared tactic

If conditions 1-2 hold but 3 fails, the slide is content, not a divider. This prevents false positives on date strings ("01/04/2026"), creative names ("Day_Video_04.20-2026"), and keywords ("solar panel cost") — all verified against real decks.

**Propagation:** once detected, a divider's section context applies to all subsequent slides until the next divider.

---

### Decision 5: Three kinds of aliases

Aliases are separated by purpose because they apply at different cascade steps:

| Kind | Applied when | Example |
|---|---|---|
| `aliases.tactic` | First-column lookup, rollup parsing, divider resolution | "Geo-Fencing w/Foot Traffic" → "Geofencing with Foot Traffic" |
| `aliases.header` | Header tokenization, before IDF scoring | "Campaign" → "Campaign Name" |
| `aliases.registry` | Tie-breaking when scores are within 0.001 | Meta/Link Click ↔ Meta/Facebook - Link Click |

**Tactic aliases** have two output forms: `canonical` (single target, resolves immediately) and `expands_to` (multiple candidates). When `expands_to` fires during first-column lookup, it **narrows the candidate pool for downstream cascade steps** to just those expanded entries, rather than falling through to the full registry. This prevents the TF-IDF step from doing unnecessary work and picking a non-matching tactic on a tie.

**Why separate:** conflating them produces false matches. A header alias that's too aggressive ("Click" → "Clicks") corrupts IDF scoring. A tactic alias that's too narrow misses the "Sponsored Social Mentions" → {Link Click, Awareness, ThruPlay} expansion.

---

### Decision 6: TF-IDF weighted F1 scoring

**IDF formula (smoothed):** `idf(token) = ln((N+1) / (df(token)+1)) + 1` where N=292 registry entries.

Real computed values from the registry:
- `impressions`: IDF ≈ 1.007 (universal, near-zero weight)
- `foot traffic`: IDF ≈ 4.378 (highly discriminating)
- `premium content`: IDF ≈ 5.987 (unique fingerprint)

**Scoring:** IDF-weighted F1 over normalized header token sets. F1 is preferred over Jaccard because PPTX tables often show a subset of registry headers — F1 is more forgiving when one side is shorter.

**Acceptance thresholds:**
- `top1 >= 0.30` AND (`margin >= 0.05` OR `relative_margin >= 0.20`) → auto-accept
- `top1 in [0.20, 0.30)` → flag for review with best guess
- `top1 < 0.20` → unmatched

Thresholds are configurable per profile.

**Why not simpler scoring:** raw percentage overlap gives equal weight to "Impressions" (appears in 290/292 entries) and "Foot Traffic" (9/292). IDF weighting is the minimal correct approach — it's the difference between "everything looks the same" and "this is clearly Geofencing."

---

### Decision 7: Duplicate-table detection

**Fingerprint:** `(sorted normalized headers, sorted normalized first-column values, row count)`. Two tables with identical fingerprints mapped to the same `(product_name, table_name)` are duplicates. First occurrence (lowest slide_index) is canonical; subsequent are marked `status = Duplicate, duplicate_of = <canonical>` and excluded from ingestion.

**Section-pair awareness:** profiles declare `duplicate_section_pairs` — dedup only fires between known overlapping sections. The SEM/Search & Intent Media overlap is the verified case: both sections contain identical Monthly/Campaign/Keyword tables in the same deck.

**Conservative by default:** duplicate suppression only auto-excludes when BOTH conditions hold: (1) identical fingerprint AND (2) both tables map to the same normalized `table_name` AND the sections are a profile-declared `duplicate_section_pair`. Without a declared pair, duplicate-looking tables across different sections are flagged for review, not auto-excluded. This prevents legitimate same-shape tables in unrelated sections from being silently dropped.

**Why all three fingerprint components:** headers alone are too lax (every monthly table has the same headers). Adding first-column values catches identity. Adding row_count catches different time windows that shouldn't be considered duplicates.

---

### Decision 8: Skip rules and continuation detection

**Skip rules** fire at Step 0 — before any matching. Patterns include: `Reach & Frequency`, `Search Impression Share`, `Top KPIs`, `Calls by Day of Week`. Skipped tables are excluded silently with no review prompt.

**Continuation tables** are detected at Step 2: same section context, no header row, column count matches the previous table. They are flagged for review (`status = ContinuationCandidate`) but NOT auto-merged in v1. Auto-merge is Phase 7C after observing false-positive rates.

**Why not auto-merge:** silent auto-merge of misidentified continuations is a data corruption risk. A table that looks like a continuation but is actually a separate breakdown (different tactic, same columns) would corrupt the cube. The review queue is the honest path.

**Ingestion invariant (binding):** only `status = Matched` results are eligible for cube ingestion. `Skipped`, `Duplicate`, `ContinuationCandidate`, `TfidfUncertain`, and `Unresolved` are excluded unless the user explicitly confirms them in the review UI. This invariant is enforced at the **ingestion adapter** — the function that converts `Vec<MatchResult>` into `Vec<ParsedCsv>` for the downstream pipeline. Debug builds assert that no non-Matched result passes through.

**Skip diagnostics:** skipped tables are excluded silently from the user-facing review panel, but their counts are included in the diagnostic/JSON output:

```
Skipped: 17 tables (Reach & Frequency: 3, KPI cards: 8, ...)
```

Silent for users. Visible for developers.

---

### Decision 9: Module independence from demo-server

The matcher module takes registry + extracted tables + profile + IDF table as inputs and returns scored match results. It has NO dependency on `mc-demo-server`, `axum`, or any HTTP concepts.

**Public API:**

```rust
pub fn match_deck(
    tables: &[ExtractedTable],
    registry: &Registry,
    profile: Option<&PptxProfile>,
    idf: &IdfTable,
) -> DeckMatchResult

pub struct MatchResult {
    pub table: ExtractedTable,
    pub mapping: Option<RegistryMapping>,
    pub confidence: f64,
    pub status: MatchStatus,
    pub alternatives: Vec<(RegistryMapping, f64)>,
    pub flag_for_review: bool,
    pub duplicate_of: Option<TableRef>,
    pub evidence: MatchEvidence,      // required debug payload
}

/// Debug/introspection payload — shipped from day 1 for cascade debugging.
pub struct MatchEvidence {
    pub normalized_headers: Vec<String>,
    pub section_context: Option<String>,
    pub table_family: Option<String>,
    pub manifest_candidates: Vec<String>,
    pub candidate_count_before_narrowing: usize,
    pub candidate_count_after_narrowing: usize,
    pub top_scores: Vec<ScoredCandidate>,
    pub fired_step: MatchSource,      // which cascade step produced this result
}
```

`MatchSource` (ProfileOverride, SectionFamily, FirstColumnExact, etc.) lives on `MatchEvidence.fired_step`, not duplicated on `MatchResult`. The evidence struct is the single source of truth for how the match was produced.

**Why:** the matcher's eventual home is a Tessera driver (`mc-drivers/pptx`), not the demo server. Building it independent from day 1 means the Tessera lift is a file move, not a rewrite. The demo server calls `match_deck` from its upload handler; the Tessera driver calls the same function from its recipe executor.

**v1 location:** `crates/mc-demo-server/src/pptx_match.rs` (adjacent to the existing `pptx.rs` extractor). Moves to `crates/mc-drivers/` when the Tessera driver ships.

**Binding independence test:** even while located under `mc-demo-server`, `pptx_match.rs` must define its own input/output structs or use only shared registry/extractor structs passed in by value/reference. It must not reference `axum`, upload handlers, request/response types, server state, workspace routing, or demo UI types. `grep -rn "axum\|upload::\|server::\|workspace::" crates/mc-demo-server/src/pptx_match.rs` must return zero matches.

**`table_title` extraction rule:** `table_title` is the nearest non-date, non-KPI text shape above the table within the same slide, preferring the closest text vertically above the table element. Fallback to `slide_title` when no nearby title text is found. This is a source of extraction risk — different PPTX generators place title text differently. The extractor's `table_title` inference is best-effort, and the cascade's later steps (TF-IDF, section context) compensate when it's wrong.

**IDF table sharing:** the `IdfTable` is built once at registry load and is immutable. Implementation should use `Arc<IdfTable>` or equivalent to avoid rebuilding per `match_deck` call.

---

### Decision 10: Diagnostic codes MC7060-MC7067

| Code | Condition | Severity |
|---|---|---|
| MC7060 | Profile schema version mismatch | Warning |
| MC7061 | Profile section/family references unknown registry tactic | Warning |
| MC7062 | Override references slide_index that doesn't exist in the deck | Warning |
| MC7063 | Profile alias canonical not found in registry | Warning |
| MC7064 | ≥30% of tables fell through to TfidfUncertain — profile may need updating | Info |
| MC7065 | Manifest-declared tactic has no corresponding section divider | Warning |
| MC7066 | Section divider found whose tactic isn't in the manifest | Info |
| MC7067 | Duplicate-section-pair declared but no duplicates found | Info |

---

### Decision 11: Header tokenization rules

Applied in order: split on `;` (registry) or cell boundaries (PPTX) → strip parenthetical expressions (`CTR(%)` → `CTR`) → lowercase → trim and collapse whitespace → drop empty tokens.

**Do not** stem/lemmatize ("Click" and "Clicks" remain distinct), remove stop words ("Total Leads" stays intact), or split multi-word tokens ("Link Clicks" is one token).

**Why explicit:** tokenization ambiguity was the root cause of v1's matching failures. `CTR(Link Click-Through Rate)` must normalize to the same token as `CTR(%)` — both become `ctr`. This is the foundation all scoring depends on.

---

## Scope boundaries

**v1 ships (2 sessions):**

Session A:
- Tokenization + IDF table computation at registry load
- Rollup pre-pass building DeckManifest
- Section divider detection with all three guards
- Full cascade (Steps 0-6) with section/table_family resolution
- Skip rules, continuation detection (flag only), duplicate detection
- Profile YAML loader for the v2 schema
- Lumina profile (`lumina-charts.yaml`) with all sections + families from 4 decks
- Regression fixtures with golden mappings per deck
- MC7060-MC7067 diagnostic codes

Session B:
- "Unmatched tables" review panel in demo UI with top-3 dropdown
- "Skip" and "confirm" affordances in the review UI
- Save user confirmations to profile (sections, families, skip rules, aliases)
- Stats block telemetry
- Coverage warnings surfaced in upload result

**v1 profile authoring scope:** v1 ships with `lumina-charts.yaml` as a checked-in fixture (authored by the PM, not user-editable via UI). User-authored profiles via the review UI are Session B scope. The review UI in Session B writes back to the profile; v1 Session A reads profiles only.

**Test corpus note:** the four Lumina decks contain real advertiser data (impressions counts, keywords, campaign names). Before checking PPTX files into test fixtures, either (a) confirm with the agency that fixture use is permitted, or (b) sanitize metric values and keywords. The structural matching tests (section detection, divider guards, cascade step coverage) don't need real numbers — synthetic data preserves all matching signals.

**v1 does NOT ship:**
- Continuation-table auto-merge (Phase 7C)
- Profile auto-selection by deck fingerprint (Phase 7C)
- Python/Tessera extractor scripts (Phase 7C)
- Profile sharing/publishing (Phase 7C+)
- Multilingual support (out of scope)
- User-editable profiles from the UI (Session B, not Session A)

---

## Alternatives considered

### Static YAML mapping file (rejected)

Author 20-30 slide-title → tactic mappings by hand. Rejected because:
1. First-column lookup resolves most pivot tables without any mapping
2. The section/family cross-product is more maintainable for modular decks
3. Manual authoring doesn't scale to new advertisers with different section mixes

### Python extractor scripts only (rejected for v1)

Each org writes a Python script that knows their template. Rejected for v1 because:
1. Requires Python on the user's machine
2. Overkill for Lumina decks which follow conventions detectable by the cascade
3. The right escape hatch for truly bespoke layouts, but Phase 7C territory

### Single scoring function (rejected)

Combine all signals into one weighted score. Rejected because first-column exact match is ground truth (confidence 1.0) while TF-IDF is probabilistic (confidence 0.3-0.5). Mixing them wastes the high-confidence signals.

---

## Success criteria

- [ ] Lumina deck (original): ≥80% of tables auto-resolved (source ≠ Unmatched/TfidfUncertain)
- [ ] Auto-resolved mappings spot-checked against golden fixtures with ≥95% precision (wrong ingestion is worse than unresolved)
- [ ] No table with `status ≠ Matched` is ingested without explicit user confirmation (ingestion invariant)
- [ ] Lumina deck 1 (627917): SEM duplicate correctly suppressed
- [ ] Lumina deck 2 (792946): 3-tactic rollup manifest built correctly
- [ ] Lumina deck 3 (959819): false-positive divider ("Day_Video_04.20-2026") rejected
- [ ] Profile loads from `.mosaic/pptx-profiles/lumina-charts.yaml`
- [ ] First-column lookup resolves "Targeted Display" → exact registry match
- [ ] Skip rule fires on Reach & Frequency tables
- [ ] MC7060-MC7067 codes swept free before implementation, then shipped
- [ ] Matcher module has zero imports from `mc-demo-server` (`grep` clean for axum/upload/server/workspace)
- [ ] Performance: `match_deck` completes in <50ms for a 100-slide, 50-table deck
- [ ] `cargo test --workspace` passes (1031 → expect ~+10 = ~1041)
- [ ] Locked surfaces (mc-core, mc-model, mc-fixtures, mc-recipe, mc-drivers, mc-tessera): zero diff

---

*The PPTX cascade matcher turns PowerPoint uploads from "nothing matches" to "most things match" by using signals already present in the deck. The profile system makes it self-improving: first upload resolves 80%+, user confirms the rest, second upload resolves 100%. The matcher is built independent of the demo server so it lifts cleanly into a Tessera driver when that phase arrives.*
