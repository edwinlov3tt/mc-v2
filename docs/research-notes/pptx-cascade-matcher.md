# Research Note: PPTX Cascade Matcher

> **Status:** Research (pre-implementation)  
> **Date:** 2026-05-08  
> **Relates to:** Phase 6D demo, PPTX extraction, Tessera driver strategy  
> **Prerequisite:** PPTX table extractor already shipped in `mc-demo-server/src/pptx.rs`

---

## 1. The Problem

The PPTX table extractor works — it pulls 48 clean tables from the Lumina Charts deck in 20ms. But the **registry matcher** can't identify which tactic each table belongs to because:

- PPTX filenames are derived from slide titles (`monthly-performance-slide58-t3.csv`), not registry patterns (`report-targeteddisplay-monthly-performance`)
- Naive header overlap is dominated by shared columns (`Date`, `Impressions`, `Clicks`, `CTR`) that appear in 30+ registry entries
- Each table was matched independently with no context from surrounding slides

## 2. Signals the Current Matcher Misses

Three strong signals exist in the PPTX that the flat matcher ignores:

### 2.1 First-column values are ground truth (pivot tables)

Slide 3 "Display - Product Performance" has a `Product` column. The first data row says `Targeted Display` — an exact match to `subproduct_name` in the registry. Similarly:
- Slide 5: first row = `YouTube True View`
- Slide 7: rows = `Sponsored Social Mentions`, `Link Clicks`

For pivot-style tables (Product/Tactic/Channel as first column), the first-column values resolve the match in one lookup. Covers roughly half the deck.

### 2.2 Section dividers propagate context

Slide 10 is just the word "Meta" — a section divider. Slides 11-13 inherit "Meta" as their parent product. A monthly performance table on slide 12 doesn't say "Meta" anywhere on it, but the divider three slides earlier tells you. A linear pass tracking section state resolves this.

### 2.3 Discriminating headers are in the tail, not the head

The first 5 columns are noise (`Date`, `Impressions`, `Clicks`, `CTR`, `Spend`). The last 2-4 columns are the fingerprint:
- `Reach; Frequency` → Hulu RON
- `Audience Reach; Premium Content` → Hulu Audience Targeted
- `Foot Traffic; Visitation Rate` → Geofencing
- `Open Rate; Deliveries; Bounces` → Email Marketing

TF-IDF weighting (rare headers count more than common ones) disambiguates far better than raw percentage overlap. "Foot Traffic" appearing anywhere is a near-certain match for Geofencing.

## 3. Proposed Architecture: Cascade Matcher

Five steps, in priority order. Each step either resolves the match or narrows candidates for the next step.

### Step 1: Section-divider pass

Walk slides linearly. Detect "minimal title-only" slides as section markers (one short text element, no tables, no images beyond a background). Propagate the section name to all subsequent slides until the next divider.

```
Slide 1: "Lumina Report" → section: None
Slide 2: "Display Ads" → section: "Display Ads"  (divider detected)
Slide 3: table found → inherits section "Display Ads"
...
Slide 10: "Meta" → section: "Meta"  (divider detected)
Slide 12: table found → inherits section "Meta"
```

Implementation: one pass through the slide list before per-slide extraction. Store `current_section: Option<String>` as state. Heuristic for divider detection: slide has ≤2 text shapes, no tables, total text length < 50 chars.

### Step 2: First-column lookup (pivot tables)

If the table has a first column whose header is one of `Product`, `Tactic`, `Channel`, `Campaign`, `Ad Set`, `Creative`, `Platform`, `Device`, `Station` (category-style headers), take the non-header cell values and match against `subproduct_name` in the registry.

```
Headers: [Product, Impressions, Clicks, CTR(%), Total Conversions]
Row 1:   [Targeted Display, 269352, 4078, 1.51, 0]
→ Lookup "Targeted Display" in registry subproduct_name → exact match, confidence 1.0
```

For pivot tables this is ground truth. Skip to "matched" — no further steps needed.

### Step 3: Section + table-title narrowing

Combine the section name (from step 1) with the slide's subtitle or section header text to narrow registry candidates.

```
Section: "Meta"
Slide subtitle: "Monthly Performance"
→ Filter registry to entries where product_name contains "Meta" AND table_name = "Monthly Performance"
→ Narrows from 190+ candidates to 1-3
```

If this produces exactly 1 candidate → matched.

### Step 4: TF-IDF header scoring

Among the narrowed candidates, score each by TF-IDF weighted header overlap.

**IDF computation (done once at startup):**
```
For each unique header across all registry entries:
  idf(header) = log(total_registry_entries / entries_containing_header)
```

- `Impressions`: appears in 180/190 entries → IDF ≈ 0.02 (nearly worthless)
- `Foot Traffic`: appears in 2/190 entries → IDF ≈ 4.55 (highly discriminating)
- `Audience Reach`: appears in 3/190 entries → IDF ≈ 4.15

**Score per candidate:**
```
score = sum(idf(h) for h in candidate.headers if h in actual_headers) / sum(idf(h) for h in candidate.headers)
```

Pick the candidate with the highest score above a threshold (e.g., 0.5).

### Step 5: UI fallback (unresolved tables)

Tables below the confidence threshold go to a UI panel showing:
- The extracted table (first few rows)
- The top-3 registry candidates with match scores
- A "Confirm" dropdown

User confirmations are saved to a **profile file**.

## 4. Profile Persistence

```yaml
# .mosaic/pptx-profiles/lumina.yaml
profile_name: "Lumina Charts"
created_at: "2026-05-08"
mappings:
  - slide_title_contains: "Display - Product Performance"
    section: "Display Ads"
    product: "Blended Tactics"
    subproduct: "Targeted Display"
    table_name: "Campaign Performance"
    source: "auto-first-column"  # or "user-confirmed"
    
  - slide_title_contains: "Monthly Performance"
    section: "Meta"
    product: "Social"
    subproduct: "Sponsored Social"
    table_name: "Monthly Performance"
    source: "user-confirmed"
```

First upload: 70-80% auto-resolve, user confirms the rest. Profile saved. Second upload: 100% automatic from profile. Profiles are version-controllable and shareable across organizations.

Profile lookup is step 0 — checked before the cascade even runs.

## 5. Architectural Notes

- **Keep matcher free of demo-server dependencies.** Input: registry + PPTX bytes. Output: `Vec<(ParsedCsv, Option<TacticSpec>, f64 confidence)>`. This makes the eventual lift to a `mc-drivers/pptx` Tessera driver a move-the-files refactor.
- **The PPTX extractor itself stays unchanged.** The cascade matcher wraps around it — extractor produces tables, matcher resolves their identity.
- **Long-term home is a Tessera driver**, not demo-server code. The cube model should consume data uniformly regardless of source. Even though the demo wires it into the upload path directly, the matcher should be designed for the Tessera lift.

## 6. Implementation Phasing

**Session A (demo-ready):** Section-divider pass + first-column lookup + TF-IDF header fallback. This alone probably gets the Lumina deck from "nothing matches" to "most things match." No UI needed — unmatched tables show as "Not in registry" (current behavior).

**Session B (6D follow-up):** Unmatched-tables UI panel in the demo frontend + profile save/load. Makes the system self-improving.

**Phase 7C+:** Python/Tessera extractor scripts as the general-purpose solution for orgs whose layouts don't follow conventions at all. The PPTX profile system is evidence the pattern works.

---

*End of research note. The PPTX table extractor is shipped and working. The gap is purely in the matching layer — the cascade approach uses signals already present in the deck (first-column values, section dividers, TF-IDF headers) rather than requiring manual YAML mappings.*
