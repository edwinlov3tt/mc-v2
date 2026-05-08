# 6E Session B Handoff — PPTX Review UI + Profile Save

> **Audience:** the Claude Code instance implementing Session B.
> **Branch:** `6e-session-b/pptx-review-ui` (cut from `main`).
> **Starting state:** 1089 tests, 0 failures. Session A shipped the cascade
> matcher in `pptx_match.rs` + `pptx_profile.rs`. The matcher produces
> `DeckMatchResult` with `MatchResult` per table, each carrying status
> (Matched/Skipped/Duplicate/ContinuationCandidate/Unresolved),
> confidence, alternatives, and evidence.
>
> **This session adds the frontend review panel and profile save.**
> When a PPTX upload has unmatched/uncertain tables, the user sees
> them in a review panel with a top-3 dropdown, confirms or skips
> each, and their choices are saved to the profile for next time.

---

## What gets built

### 1. Backend: expose match results in upload response

**Update `UploadResponse`** in `upload.rs` to include match results
when the upload is a PPTX:

```rust
pub struct UploadResponse {
    // ... existing fields ...
    /// PPTX match results (None for CSV uploads).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pptx_match_summary: Option<PptxMatchSummary>,
}

#[derive(Debug, Serialize)]
pub struct PptxMatchSummary {
    pub total_tables: usize,
    pub auto_resolved: usize,
    pub skipped: usize,
    pub duplicates: usize,
    pub review_needed: usize,
    pub unmatched: usize,
    /// Tables needing review (status != Matched && status != Skipped && status != Duplicate)
    pub review_items: Vec<ReviewItem>,
}

#[derive(Debug, Serialize)]
pub struct ReviewItem {
    pub slide_index: u32,
    pub table_index: u32,
    pub slide_title: Option<String>,
    pub table_title: Option<String>,
    pub headers: Vec<String>,
    pub row_count: usize,
    pub first_row: Vec<String>,
    pub status: String, // "unresolved", "uncertain", "continuation_candidate"
    pub best_guess: Option<ReviewCandidate>,
    pub alternatives: Vec<ReviewCandidate>,
}

#[derive(Debug, Serialize)]
pub struct ReviewCandidate {
    pub product_name: String,
    pub subproduct_name: String,
    pub table_name: String,
    pub confidence: f64,
    pub source: String,
}
```

### 2. Backend: confirm/skip endpoint

**New endpoint: `POST /api/pptx-review`**

Accepts user decisions for review items and saves them to the profile:

```rust
#[derive(Deserialize)]
struct ReviewDecision {
    slide_index: u32,
    table_index: u32,
    action: ReviewAction,
}

#[derive(Deserialize)]
#[serde(tag = "action")]
enum ReviewAction {
    /// User confirmed a mapping — save to profile as an override
    Confirm {
        product_name: String,
        subproduct_name: String,
        table_name: String,
    },
    /// User said "skip this table" — save to profile skip_tables
    Skip { reason: String },
}

// POST /api/pptx-review
async fn handle_pptx_review(
    State(state): State<Arc<AppState>>,
    Json(decisions): Json<Vec<ReviewDecision>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)>
```

For each decision:
- **Confirm:** add an override to the profile's `overrides:` list
  `{ slide_index, table_index, mapping: { product, subproduct, table_name } }`
- **Skip:** add to the profile's `skip_tables:` list
  `{ when: { slide_index, table_index }, reason: "User skipped" }`

Write the updated profile back to `.mosaic/pptx-profiles/lumina-charts.yaml`
using atomic write (tmp + rename).

After saving, return `{ "saved": N, "profile": "lumina-charts" }`.

### 3. Frontend: review panel

Add a review section to the results view that appears ONLY when
`pptx_match_summary?.review_needed > 0`.

```
┌────────────────────────────────────────────────────┐
│  PPTX Review — 4 tables need confirmation          │
├────────────────────────────────────────────────────┤
│                                                    │
│  Slide 12, Table 0                                 │
│  Title: "Monthly Performance"                      │
│  Headers: Date, Impressions, Clicks, CTR(%), ...   │
│  5 rows (first: 05-2026, 13,656, 5, 0.04, 0)      │
│                                                    │
│  Best guess: SEM / Monthly Performance (32%)       │
│                                                    │
│  [Confirm ▾ ] [Skip]                               │
│   ├ SEM / SEM / Monthly Performance (32%)          │
│   ├ Blended / Targeted Display / Monthly (28%)     │
│   └ Email / 1:1 Marketing / Monthly (22%)          │
│                                                    │
│  ─────────────────────────────────────────────     │
│  Slide 17, Table 0  (Reach & Frequency — skipped)  │
│  [already handled by skip rule]                    │
│                                                    │
├────────────────────────────────────────────────────┤
│  [Save All Decisions]                              │
└────────────────────────────────────────────────────┘
```

**Frontend types** (add to App.tsx interfaces):

```typescript
interface ReviewCandidate {
  product_name: string
  subproduct_name: string
  table_name: string
  confidence: number
  source: string
}

interface ReviewItem {
  slide_index: number
  table_index: number
  slide_title: string | null
  table_title: string | null
  headers: string[]
  row_count: number
  first_row: string[]
  status: string
  best_guess: ReviewCandidate | null
  alternatives: ReviewCandidate[]
}

interface PptxMatchSummary {
  total_tables: number
  auto_resolved: number
  skipped: number
  duplicates: number
  review_needed: number
  unmatched: number
  review_items: ReviewItem[]
}
```

**Component: `ReviewPanel`**

- Shows only when `response.pptx_match_summary?.review_needed > 0`
- Each review item shows: slide/table index, title, headers, first row preview
- Dropdown defaulting to `best_guess` with `alternatives` as options
- "Skip" button marks the table as skip
- "Save All Decisions" button POSTs to `/api/pptx-review`
- After save: show success toast, note "re-upload to see updated results"
- Styling: same neutral/warm palette as the rest of the demo. Use the
  amber/yellow accent for review items (they need attention but aren't errors).

### 4. Match summary banner

At the top of the results view, when a PPTX was uploaded, show a summary line:

```
PPTX: 48 tables — 43 matched, 1 skipped, 4 need review
```

Color coding: matched count in green, skipped in gray, review in amber.

---

## Implementation notes

**Profile write path:** `pptx_profile.rs` currently only reads. Add:

```rust
pub fn save_profile(dir: &Path, profile: &PptxProfile) -> Result<(), String>
```

Atomic write: serialize to YAML string via `serde_yaml::to_string`,
write to `.tmp`, rename. Same pattern as ledger/benchmark writes.

**Profile location:** the demo server's working directory. Use
`std::env::current_dir()` as the workspace root, same as the
benchmark library loading.

**No re-evaluation on confirm.** The review UI saves decisions to the
profile. The user re-uploads to see the updated results. This keeps
the upload path stateless (no session state between requests).

**Frontend rebuild:** after modifying `App.tsx`, run:
```bash
cd demo/frontend && npm run build
```
The `dist/` directory is gitignored but needs to be present for
`--static demo/frontend/dist` to serve the updated frontend.

---

## Hard Rules

1. **`mc-core`, `mc-model`, `mc-fixtures`, `mc-recipe`, `mc-drivers`, `mc-tessera`, `mc-narrative`, `mc-diagnostics` all locked.** Zero diff.
2. **`pptx_match.rs` stays module-independent.** The review endpoint lives in `server.rs`, not `pptx_match.rs`. Profile writing lives in `pptx_profile.rs`.
3. **The existing CSV upload path is unchanged.** `pptx_match_summary` is `None` for CSV uploads.
4. **Profile overrides use slide_index + table_index.** These are positional — they work for the same deck template but may break across different deck layouts. That's acceptable for v1; Session B's scope is "save what the user confirmed," not "generalize across decks."

---

## Acceptance Gates

- [ ] `cargo fmt --check --all` + `cargo clippy` + `cargo build --release` exit 0
- [ ] `cargo test --workspace` passes (1089 → ~1093)
- [ ] PPTX upload response includes `pptx_match_summary` with correct counts
- [ ] Review panel shows for PPTX uploads with unmatched tables
- [ ] Dropdown shows top-3 alternatives with confidence
- [ ] "Skip" marks a table as skipped
- [ ] "Save All Decisions" POSTs to `/api/pptx-review` and writes profile
- [ ] Re-upload after save: previously-unmatched tables now resolve via profile overrides
- [ ] CSV upload: no review panel, no `pptx_match_summary` in response
- [ ] Locked surfaces: zero diff
