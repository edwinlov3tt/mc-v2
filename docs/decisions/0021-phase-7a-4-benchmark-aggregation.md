# ADR-0021: Phase 7A.4 — Benchmark Aggregation

**Status:** Proposed
**Date:** 2026-05-07
**Deciders:** project owner
**Phase:** 7A.4 (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 7A.4 turns the interpretation ledger into aggregate intelligence. Every advertiser's narrative evidence contributes to an internal benchmark library — anonymized, aggregated by industry × geography × period × metric. The moat: the benchmark library gets better with every report run, making future reports more valuable without additional LLM cost. This is built into the platform's terms of service — if you're running ads with us, your anonymized performance data contributes to industry benchmarks. No opt-out. Standard practice for platforms that provide comparative analytics (Google Ads auction insights, Meta's delivery estimates, LinkedIn's campaign benchmarks all work this way).

---

## Context

Phase 7A.2 shipped the interpretation ledger — every narrative output is durably stored with full evidence (measure values, dimension context, severity, template_id). Phase 7A.3 (in progress) adds cross-period analysis, reading the ledger for trend detection.

The next logical step: **aggregate across ALL ledgers in the platform** (across all advertisers, all markets, all periods) to produce benchmark statistics. When a new advertiser uploads their first report, the narrative templates can immediately compare against "what does normal look like for HVAC / Targeted Display / Southeast US / Q4?" — because hundreds of other HVAC advertisers already contributed their evidence.

**Why this matters strategically:**

1. **Network effect.** Every new advertiser makes the benchmarks better; better benchmarks make the platform more valuable for every existing advertiser. Classic two-sided network effect.

2. **Zero marginal cost.** The benchmark library is computed from data we already have (the ledger). No additional data collection, no API calls, no LLM cost. Just aggregation math.

3. **Competitive moat.** A new competitor can't replicate this without having the same volume of historical performance data. The longer Mosaic runs, the better the benchmarks get.

4. **Immediate template quality improvement.** Today's templates hardcode benchmarks (`benchmark: "0.10"` for display CTR). With 7A.4, templates reference the dynamic benchmark library and get auto-updated values as the library grows.

**The privacy model (binding — project owner directive):**

> **Benchmarks are part of the platform, not opt-in.** If a business runs ads through our platform, their anonymized performance data contributes to industry benchmarks. This is covered in the platform's terms of service. There is no opt-out for benchmark contribution. This is standard practice — Google Ads provides "auction insights" from ALL advertisers; Meta provides "estimated action rates" from ALL campaigns; LinkedIn provides "campaign benchmarks" from ALL accounts. Users benefit from the aggregate while no individual's data is exposed.

The privacy protection is **anonymization**, not **opt-out**:
- Individual advertiser names never appear in benchmarks
- Individual campaign values never appear (only aggregates with minimum sample sizes)
- k-anonymity threshold ensures no individual can be identified from the aggregate
- The aggregate is the product; individual data is the raw material that's never exposed

---

## Decisions

### Decision 1: Scope

**In scope (binding):**

- Aggregation pipeline that reads ALL ledger entries in the system
- Anonymization layer (strips advertiser names, hashes identifiers, enforces k-anonymity)
- Benchmark computation: percentile distribution (P10, P25, P50, P75, P90) by industry × geography × tactic × metric × period
- Benchmark library schema (extends Phase 3G `benchmarks:` block with distribution + sample metadata)
- Benchmark staleness detection (MC7040 lint warning when referencing a benchmark older than `stale_after_days`)
- `mc benchmark refresh` CLI verb (rebuilds benchmarks from local ledger data)
- `mc benchmark list` CLI verb (shows available benchmarks with metadata)
- Template syntax for referencing dynamic benchmarks: `benchmark("display_ctr", industry, geography)` → looks up the library at eval time
- Narrative templates that use benchmarks: "Your CTR of 0.44% is at the 82nd percentile for HVAC / Targeted Display in the Southeast US"
- MCP tool: `mosaic.benchmark.list`, `mosaic.benchmark.refresh`

**Out of scope:**
- Central/cloud benchmark registry (aggregation is LOCAL in v1 — your own platform's data)
- Cross-organization federated benchmarks (Phase 7+ productization)
- Real-time benchmark streaming (refresh is batch, not live)
- Benchmark marketplace (selling/buying benchmark datasets)
- Differential privacy noise injection (k-anonymity is sufficient for v1)

### Decision 2: No opt-out — benchmarks are part of the platform

**Binding (project owner directive):**

Every ledger entry contributes to the benchmark library. There is no per-workspace, per-advertiser, or per-campaign opt-out for benchmark contribution. The privacy protection is anonymization (no individual data exposed), not consent-based exclusion.

**Rationale:**
- Industry standard: Google Ads, Meta, LinkedIn, programmatic platforms ALL use aggregate advertiser data for benchmarks without opt-out
- The value proposition requires completeness: benchmarks with 30% opt-out rates produce biased statistics (high-performing advertisers opt out → benchmarks skew low → comparisons become meaningless)
- The business relationship (running ads through the platform) implies agreement to platform analytics per terms of service
- Individual data is NEVER exposed; only aggregates with minimum sample thresholds

**What this means for implementation:**
- No `opt_in` / `opt_out` field on workspaces or ledger entries
- No privacy-boundary configuration per-advertiser
- The aggregation pipeline processes ALL available ledger data unconditionally
- The ONLY protection is anonymization (Decision 3 below)

**ADR-0020's Q8 answer ("opt-in only") is SUPERSEDED by this decision.** The project owner reviewed and directed: platform-level contribution, no opt-out. The ADR-0020 planning doc's Q8 was a conservative default for a hypothetical multi-tenant SaaS; the actual business model is a media platform where aggregate analytics are a core platform feature.

### Decision 3: Anonymization layer (k-anonymity, not opt-out)

**Binding anonymization rules:**

1. **Advertiser names are NEVER stored in benchmarks.** The benchmark library contains only: industry, geography (region/DMA — not specific city unless sample size permits), tactic, metric name, period, and aggregate statistics. No advertiser name, no order ID, no campaign name.

2. **k-anonymity threshold: k=5 minimum** (binding). A benchmark cell (industry × geography × tactic × metric × period) is only published if it contains contributions from at least 5 distinct advertisers. Below that threshold, the cell is suppressed.

3. **Individual values are never stored.** The aggregation pipeline computes percentiles from raw values and then DISCARDS the raw values. The benchmark library contains only: P10, P25, P50 (median), P75, P90, count, mean. No individual data points are retained.

4. **Geography granularity floor.** Benchmarks aggregate to DMA/region level minimum (not zip code, not city). Example: "Southeast US / HVAC / Targeted Display / CTR" is a valid benchmark. "Rockford IL / HVAC / Targeted Display / CTR" is NOT (too specific; could identify an advertiser).

5. **Temporal aggregation floor.** Benchmarks aggregate to quarterly minimum (not weekly, not monthly for small sample sizes). If a quarterly cell doesn't meet k=5, it rolls up to annual.

### Decision 4: Benchmark library schema

**Binding schema (extends Phase 3G `benchmarks:` block):**

```yaml
benchmarks:
  - id: hvac_targeted_display_ctr_southeast_q4_2025
    domain: marketing
    metric: CTR
    metric_unit: percent
    industry: HVAC
    geography: Southeast US
    tactic: Targeted Display
    period: "2025-Q4"
    period_type: quarterly
    distribution:
      p10: 0.018
      p25: 0.029
      p50: 0.041
      p75: 0.054
      p90: 0.072
    mean: 0.043
    sample_size: 23       # number of distinct advertisers contributing
    refreshed_at: "2026-01-15T00:00:00Z"
    stale_after_days: 180  # MC7040 fires if template references this after 180 days
    methodology: "Percentile distribution from platform ledger; k=5 minimum; DMA-level geography"
```

**The benchmark ID is deterministic:** `{industry}_{tactic}_{metric}_{geography}_{period}` lowercased and slugified. Same inputs always produce same ID.

### Decision 5: Aggregation pipeline

**Binding pipeline steps (executed by `mc benchmark refresh`):**

```
1. Read ALL ledger entries from the specified directory tree
   (all .mosaic/analysis-ledger.jsonl files under the workspace root)

2. Extract evidence fields from each entry:
   - metric values (Impressions, Clicks, CTR, Spend, CPM, etc.)
   - scope fields (industry, geography/market, tactic/channel)
   - report_period

3. Group by: industry × geography(DMA) × tactic × metric × period(quarterly)

4. For each group:
   a. Count distinct advertisers (from scope hash, not name)
   b. If count < k (5): suppress this cell
   c. If count >= k: compute P10, P25, P50, P75, P90, mean

5. Write benchmark library to demo/benchmarks/platform-benchmarks.yaml
   (or .mosaic/benchmarks.yaml at workspace level)

6. Report: "Refreshed N benchmark cells from M ledger entries
   across K advertisers. Suppressed J cells below k=5 threshold."
```

**Performance:** the aggregation runs in <1 second for 10K ledger entries (typical for a platform with 100 advertisers × 12 months × ~10 entries per month). This is a batch operation run periodically, not per-request.

### Decision 6: Template integration — `benchmark()` function

**Binding syntax in narrative templates:**

```yaml
- id: ctr_vs_platform_benchmark
  when: "true"
  template: >
    Your CTR of {campaign_ctr:.2f}% is at the {percentile_label}
    for {industry} / {tactic} in {geography}
    (platform median: {median:.2f}%; your position: P{percentile}).
  bindings:
    campaign_ctr: "campaign_avg.CTR"
    median: "benchmark('CTR', 'p50')"
    percentile: "benchmark_percentile('CTR', campaign_avg.CTR)"
    percentile_label: >
      if(percentile >= 90, 'top 10%',
        if(percentile >= 75, 'top quartile',
          if(percentile >= 50, 'above median',
            if(percentile >= 25, 'below median',
              'bottom quartile'))))
```

Two new evaluator functions:
- `benchmark(metric, stat)` — looks up the benchmark library for the current scope (industry × geography × tactic from the cube) and returns the requested statistic (p10, p25, p50, p75, p90, mean).
- `benchmark_percentile(metric, value)` — returns where the given value falls in the benchmark distribution (0-100 scale).

**Benchmark lookup scope:** the evaluator infers industry, geography, and tactic from the cube's metadata (same scope fields used for ledger entries). If no benchmark exists for the exact scope, returns Null (template skips per existing Null-handling behavior).

### Decision 7: CLI verbs

**Binding:**

```bash
# Rebuild benchmarks from all available ledger data
mc benchmark refresh <workspace-dir>
  --min-sample 5        # k-anonymity threshold (default 5)
  --period-type quarterly  # aggregation granularity (quarterly | annual)
  --output benchmarks.yaml  # where to write

# List available benchmarks
mc benchmark list <workspace-dir>
  --industry HVAC
  --metric CTR
  --format json|text

# Show where a specific value falls in the benchmark
mc benchmark compare <workspace-dir> --metric CTR --value 0.44
  → "0.44% CTR is at P82 for HVAC / Targeted Display / Southeast US (Q4 2025)"
```

### Decision 8: MC7040-MC7043 diagnostic codes

| Code | Stage | Meaning |
|---|---|---|
| MC7040 | lint | Template references a benchmark that is stale (past `stale_after_days`) |
| MC7041 | aggregation | Benchmark cell suppressed: sample size below k-anonymity threshold |
| MC7042 | eval | Benchmark lookup failed: no benchmark exists for the requested scope |
| MC7043 | aggregation | Geography granularity too fine: city-level benchmark attempted (only DMA+ allowed) |

### Decision 9: Staleness + refresh cadence

**Binding defaults:**
- `stale_after_days: 180` (6 months; configurable per benchmark)
- MC7040 fires as a **lint warning** (not error) when a template references a stale benchmark
- The narrative still renders (using the stale value); the warning signals "this benchmark should be refreshed"
- Recommended refresh cadence: quarterly (after each quarter's data has accumulated)
- `mc benchmark refresh` is a manual command; no automatic cron in v1. (Phase 7+ could add auto-refresh.)

### Decision 10: Advertiser identity hashing

To count "distinct advertisers" without storing names:

**Binding approach:** hash the scope fields that identify an advertiser. The aggregation pipeline sees `sha256(advertiser_scope_fields)` — it can count distinct hashes without knowing which advertiser each hash represents.

```rust
fn advertiser_hash(scope: &BTreeMap<String, String>) -> String {
    let key = scope.values().collect::<Vec<_>>().join("|");
    sha256_hex(&key)
}
```

The raw scope fields (which may contain advertiser names) are ONLY used for hashing during aggregation; the hash is used for counting; neither the raw scope nor the hash appears in the final benchmark library.

### Decision 11: Implementation order

4 sessions estimated:

1. **Aggregation pipeline + anonymization** — read ledger entries, group, enforce k=5, compute percentiles, write benchmark YAML
2. **`mc benchmark refresh` + `mc benchmark list` + `mc benchmark compare` verbs** — CLI surface
3. **`benchmark()` + `benchmark_percentile()` evaluator functions** — template integration
4. **MC7040-MC7043 + staleness lint + demo integration** — ship-ready

### Decision 12: Relationship to external benchmark sources

Phase 3G already ships static `benchmarks:` blocks with hardcoded values (e.g., `industry_cpc: 5.50` with source attribution). Phase 7A.4's dynamic benchmarks COEXIST with static benchmarks:

- Static benchmarks (Phase 3G): hand-authored, sourced from external reports (WordStream, etc.), declared per-cartridge
- Dynamic benchmarks (Phase 7A.4): computed from the platform's own ledger data, refreshed periodically

Templates can reference EITHER:
- `benchmark('CTR', 'p50')` → looks up the dynamic platform benchmark (7A.4)
- `lookup('industry_cpc', Channel)` → looks up the static cartridge benchmark (Phase 3G, unchanged)

Both coexist. Dynamic benchmarks are "what OUR platform's advertisers achieve"; static benchmarks are "what the industry reports say." A template can compare against both: "Your CPC is $4.20 — below the platform median ($5.80, P45) and below the WordStream industry average ($6.50)."

---

## Alternatives considered

### Alt 1: Opt-in contribution (ADR-0020 Q8 original answer)

Considered. ADR-0020 planning doc originally answered Q8 as "opt-in only, unconditionally." **Superseded by project owner directive:** the business is a media platform; benchmark contribution is a platform feature covered by ToS, not a user preference. Opt-in would produce incomplete data → biased benchmarks → useless comparisons.

### Alt 2: Differential privacy noise injection

Considered. Adding mathematical noise to aggregates (ε-differential privacy). **Rejected for v1:** k-anonymity (k=5, DMA-level geography, quarterly minimum) provides sufficient protection for a B2B media platform context. Differential privacy adds complexity, reduces benchmark precision, and is overkill when the aggregation already suppresses small cells and never exposes individual values.

### Alt 3: Per-advertiser benchmark visibility (only see your own industry)

Considered. An advertiser in HVAC only sees HVAC benchmarks. **Rejected:** benchmarks are aggregate statistics, not individual data. Showing an HVAC advertiser the "Retail / Targeted Display / CTR" benchmark doesn't expose any retail advertiser's data — it's a percentile distribution from 50+ contributors. No reason to restrict visibility.

### Alt 4: Cloud-hosted central benchmark service

Considered. A central API that all platform instances report to and query from. **Rejected for v1:** unnecessary complexity. The platform has one instance with all ledger data locally. Central hosting is Phase 7+ multi-tenant productization scope.

### Alt 5: Real-time benchmark updates (streaming aggregation)

Considered. Benchmarks update live as new ledger entries arrive. **Rejected:** batch refresh (quarterly) is sufficient and simpler. Marketing benchmarks don't change fast enough to warrant real-time. A quarterly refresh cycle matches the industry's reporting cadence.

---

## Cross-links

- **ADR-0020 planning doc (superseded Q8 answer):** [`0020-phase-7a-narrative-engine-plan.md`](0020-phase-7a-narrative-engine-plan.md) — Q8 original "opt-in only" answer is superseded by this ADR's Decision 2
- **Phase 3G (static benchmarks foundation):** [`0013-phase-3g-reference-data-blocks.md`](0013-phase-3g-reference-data-blocks.md)
- **Phase 7A.2 (ledger that feeds this):** tag `phase-7a-2-interpretation-ledger`
- **Phase 7A.3 (cross-period that reads the same ledger):** in progress on branch `phase-7a-3/cross-period-analysis`

---

## Acceptance amendments

*(None as of authoring. Project owner review pending.)*
