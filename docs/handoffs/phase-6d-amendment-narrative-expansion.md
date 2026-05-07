# Phase 6D Amendment — Narrative Expansion + ROI Calculator

> **For the implementing instance.** This amendment adds 7 new
> narrative templates + an ROI comparison calculator to the demo UI.
> Scope: ~2-3 hours of focused work on top of the completed Session 5.

---

## Part 1: 7 New Narrative Templates

Add these to `demo/narratives/display-like.yaml` (or wherever
your template definitions live). Each one exercises a different
analytical pattern that the current 6 templates don't cover.

### Template 1 — Engagement Velocity vs Reach Growth

**What it proves:** ratio-of-ratios comparison (two growth rates
compared against each other).

```yaml
- id: engagement_acceleration
  family: display-like
  severity: info
  when: "period_count >= 2 AND abs(click_growth_pct) > abs(impr_growth_pct) * 1.5"
  template: "Engagement is accelerating faster than reach: clicks grew {click_growth_pct:.0f}% while impressions grew only {impr_growth_pct:.0f}%. The campaign is improving its ability to convert attention into action."
  bindings:
    click_growth_pct: "(Clicks - prev(Clicks)) / prev(Clicks) * 100"
    impr_growth_pct: "(Impressions - prev(Impressions)) / prev(Impressions) * 100"
```

**Expected output on Scotts RV data:** "Engagement is accelerating
faster than reach: clicks grew 110% while impressions grew only
22%."

### Template 2 — Industry Benchmark Comparison

**What it proves:** the system contextualizes data against industry
norms — deterministically, no LLM judgment needed.

```yaml
- id: ctr_vs_benchmark
  family: display-like
  severity: info
  when: "true"
  template: "Campaign CTR of {campaign_ctr:.2f}% is {multiple:.1f}x the industry average for Targeted Display ({benchmark}%). {interpretation}"
  bindings:
    campaign_ctr: "avg_over(CTR, Time)"
    benchmark: "0.10"
    multiple: "avg_over(CTR, Time) / 0.10"
    interpretation: "if(avg_over(CTR, Time) > 0.30, 'This significant outperformance indicates strong creative-audience alignment.', if(avg_over(CTR, Time) > 0.10, 'Performance is above industry norms — targeting appears effective.', 'Performance is at or below industry norms — review targeting and creative.'))"
```

**Expected:** "Campaign CTR of 0.44% is 4.4x the industry average
for Targeted Display (0.10%)."

### Template 3 — Uniform Momentum Detection

**What it proves:** pattern detection across ALL metrics
simultaneously.

```yaml
- id: uniform_momentum
  family: display-like
  severity: info
  when: "period_count >= 2 AND period_delta(Impressions) > 0 AND period_delta(Clicks) > 0 AND period_delta(CTR) > 0"
  template: "All key metrics improved from {prev_period} to {current_period}: impressions (+{impr_pct:.0f}%), clicks (+{click_pct:.0f}%), and CTR (+{ctr_pct:.0f}%). Uniform positive momentum across reach, engagement, and efficiency indicates the campaign is strengthening — not trading one metric for another."
  bindings:
    impr_pct: "(Impressions - prev(Impressions)) / prev(Impressions) * 100"
    click_pct: "(Clicks - prev(Clicks)) / prev(Clicks) * 100"
    ctr_pct: "(CTR - prev(CTR)) / prev(CTR) * 100"
```

**Expected:** "All key metrics improved from Jul 2025 to Aug 2025:
impressions (+22%), clicks (+110%), and CTR (+74%)."

### Template 4 — Zero-Engagement Alarm (Generalized)

**What it proves:** threshold + volume filter detecting waste.

```yaml
- id: zero_engagement_alarm
  family: display-like
  severity: warning
  when: "any_over(Clicks, City) == 0 AND corresponding_over(Impressions, City) > 50"
  template: "⚠️ {zero_city} received {zero_impressions:,.0f} impressions with zero clicks. This area is consuming delivery with no engagement signal — evaluate whether geo-targeting includes this area intentionally."
```

**Expected:** "⚠️ Monroe Center received 89 impressions with zero
clicks."

### Template 5 — Device Underperformance Alarm

**What it proves:** relative comparison (device CTR vs campaign
average with a severity threshold).

```yaml
- id: device_underperformance
  family: display-like
  severity: warning
  when: "min_over(CTR, Device) < avg_over(CTR, Device) * 0.25"
  template: "⚠️ {worst_device} is significantly underperforming at {worst_ctr:.2f}% CTR — {deficit_pct:.0f}% below the campaign average ({avg_ctr:.2f}%). This device served {worst_impressions:,.0f} impressions ({worst_share:.0f}% of total) with minimal engagement."
  bindings:
    deficit_pct: "(1 - min_over(CTR, Device) / avg_over(CTR, Device)) * 100"
```

**Expected:** "⚠️ PC (Desktop or Laptop) is significantly
underperforming at 0.07% CTR — 84% below the campaign average
(0.44%)."

### Template 6 — Data Sufficiency Disclosure

**What it proves:** the system is honest about its own analytical
confidence — a quality signal that builds trust.

```yaml
- id: data_sufficiency
  family: display-like
  severity: info
  when: "true"
  sort_order: -1  # fires first in the report
  template: "ℹ️ This analysis is based on {period_count} reporting period{plural}. {confidence}"
  bindings:
    plural: "if(period_count > 1, 's', '')"
    confidence: "if(period_count == 1, 'Single-period snapshot — no trend analysis possible. All comparisons are against industry benchmarks only.', if(period_count == 2, 'Directional trends are visible but 3+ periods are recommended for statistically confident trend assessment.', 'Sufficient data for meaningful trend analysis across all metrics.'))"
```

**Expected:** "ℹ️ This analysis is based on 2 reporting periods.
Directional trends are visible but 3+ periods are recommended for
statistically confident trend assessment."

### Template 7 — Small-Sample Reliability Warning

**What it proves:** statistical rigor — flagging areas where the
data can't support confident conclusions.

```yaml
- id: small_sample_warning
  family: display-like
  severity: warning
  when: "count_where(Impressions < 500, City) > 0"
  template: "⚠️ {count} geographic area{plural} had fewer than 500 impressions ({area_list}). CTR values for these areas should be considered directionally indicative only — sample sizes are insufficient for confident performance assessment."
```

**Expected:** "⚠️ 2 geographic areas had fewer than 500 impressions
(Cherry Valley, Monroe Center). CTR values for these areas should
be considered directionally indicative only."

---

## Implementation notes for the templates

The `when:` predicates and `bindings:` above are PSEUDO-formulas
showing intent. The implementing instance should translate them
into whatever evaluation approach `narrative.rs` already uses from
Session 3. Key patterns needed:

- **`period_count`:** count of distinct Time-dimension elements in
  the cube. Probably already available from Session 2's cube shape.
- **`period_delta(X)`:** `X - prev(X)`. Already in the formula
  engine since Phase 3F.
- **`prev(X)`:** previous time period's value. Phase 3F.
- **`avg_over(X, Dim)`:** average across a dimension. Phase 3I.
- **`min_over(X, Dim)`:** minimum across a dimension. Phase 3I.
- **`count_where(condition, Dim)`:** count of elements matching a
  condition. May need a small helper if not already in narrative.rs.
- **`any_over(X, Dim) == 0`:** "any element in Dim where X is 0."
  Similar helper.

If any of these require non-trivial work, the implementing instance
should prioritize Templates 2, 3, 5, 6 (which use simpler predicates)
and defer 1, 4, 7 (which need the count/any helpers).

---

## Part 2: ROI Comparison Calculator

Add a new section to the demo UI (below the narrative report,
above the "Show Payload" view) that shows the economic argument.

### UI Component: "LLM vs Mosaic LNM Comparison"

**Layout:** a card/panel with two columns + an input field:

```
┌─────────────────────────────────────────────────────────┐
│  LLM vs Mosaic LNM — Cost & Speed Comparison            │
│                                                          │
│  Reports per month: [  250  ]  ← editable input          │
│                                                          │
│  ┌─────────────────────┐  ┌─────────────────────────┐   │
│  │  LLM (Claude/GPT)   │  │  Mosaic LNM              │  │
│  │                      │  │                           │  │
│  │  Per report:         │  │  Per report:              │  │
│  │  ~25,000 tokens      │  │  0 tokens                 │  │
│  │  ~$0.12 cost         │  │  $0.00 cost               │  │
│  │  ~45 sec processing  │  │  {actual_ms}ms processing │  │
│  │                      │  │                           │  │
│  │  250 reports/month:  │  │  250 reports/month:       │  │
│  │  6.25M tokens        │  │  0 tokens                 │  │
│  │  $30.00/month        │  │  $0.00/month              │  │
│  │  3.1 hours waiting   │  │  {total_sec} seconds      │  │
│  │                      │  │                           │  │
│  │  Annual:             │  │  Annual:                  │  │
│  │  $360/year           │  │  $0/year                  │  │
│  │  37.5 hours/year     │  │  {total_min} minutes/year │  │
│  └─────────────────────┘  └─────────────────────────┘   │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │  Savings with Mosaic LNM:                           │ │
│  │  $360/year in token costs                           │ │
│  │  37.4 hours/year in processing time                 │ │
│  │  Zero hallucination risk                            │ │
│  │  Deterministic, auditable, reproducible              │ │
│  └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### Constants (hardcode for the demo)

```typescript
const LLM_CONSTANTS = {
  tokens_per_report: 25000,     // from the real report JSONs (22K-27K tokens)
  cost_per_1k_tokens: 0.005,    // ~$0.005/1K tokens (Claude Sonnet output pricing ballpark)
  seconds_per_report: 45,       // realistic for a 3-tactic report with analysis
};
```

### Reactive calculations (update when input changes)

```typescript
const reports = inputValue;  // from the editable field, default 250

// LLM side
const llm_tokens_month = LLM_CONSTANTS.tokens_per_report * reports;
const llm_cost_month = llm_tokens_month / 1000 * LLM_CONSTANTS.cost_per_1k_tokens;
const llm_time_month_hours = (LLM_CONSTANTS.seconds_per_report * reports) / 3600;
const llm_cost_year = llm_cost_month * 12;
const llm_time_year_hours = llm_time_month_hours * 12;

// Mosaic side
const mosaic_ms_per_report = actualProcessingTimeMs;  // from the last upload's processing_time_ms
const mosaic_time_month_sec = (mosaic_ms_per_report * reports) / 1000;
const mosaic_time_year_min = (mosaic_time_month_sec * 12) / 60;
// cost is always $0
```

### Where it gets the actual Mosaic processing time

The JSON response already includes `processing_time_ms`. The
calculator reads that value and plugs it into the Mosaic column.
So if the actual processing was 2.5ms, the calculator shows:

```
Mosaic LNM: 2.5ms per report
250 reports/month: 0.6 seconds total
Annual: 7.5 seconds total
```

vs

```
LLM: 45 sec per report
250 reports/month: 3.1 hours total
Annual: 37.5 hours total
```

The contrast sells itself. **The slider/input makes it interactive
for the demo call** — leadership can type in their actual report
volume and see the savings scale.

### Styling

Match the existing dark theme. Use green for Mosaic column numbers,
red/orange for LLM column costs. The savings banner at the bottom
should be prominent (maybe a green-tinted card with bold numbers).

Keep it minimal — no charts, no animations. Just clean numbers
that update instantly when the input changes. The speed of the
calculation itself (instant reactivity) reinforces the "deterministic
= fast" message.

---

## Acceptance criteria for this amendment

- [ ] At least 5 of the 7 new templates fire on the Scotts RV
  sample data (Templates 2, 3, 4, 5, 6 should all fire; 1 fires
  if the velocity condition is met; 7 fires if small-sample helper
  is implemented).
- [ ] Total narrative count increases from ~10 to ~15-17.
- [ ] ROI calculator displays in the demo UI below the narratives.
- [ ] Calculator's Mosaic processing time reads from the actual
  `processing_time_ms` in the JSON response (not hardcoded).
- [ ] Calculator reactively updates when the user changes the
  reports-per-month input.
- [ ] The "savings" summary shows annual token cost savings +
  annual time savings.
- [ ] Terminal timing still shows "Done Xms" (existing functionality
  preserved).
- [ ] All existing tests still pass.

---

## What NOT to build

- Don't add charts or visualizations to the calculator. Numbers only.
- Don't make the LLM constants configurable in the UI. Hardcode
  them for the demo (real values from the attached report JSONs).
- Don't add a "per-token cost" slider. Keep it simple — one input
  (reports per month), one comparison.
- Don't add the Tier 2 templates (creative message winner, size
  efficiency, concentration index, etc.) unless Tier 1 is done
  and there's time remaining.
