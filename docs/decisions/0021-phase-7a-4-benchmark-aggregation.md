# ADR-0021 — Phase 7A.4: Benchmark Aggregation

**Status:** Proposed  
**Date:** 2026-05-07  
**Author:** Edwin Lovett III  
**Depends on:** ADR-0020 (Phase 7A narrative engine plan), Phase 7A.2 (interpretation ledger)

---

## Context

Phase 7A.2 ships an append-only JSONL ledger at `.mosaic/analysis-ledger.jsonl` that records every narrative event: which templates fired, the evidence values, the period, the scope (channel, market, tactic), and the severity. After 6+ months of operation, a workspace accumulates a dense record of its own performance patterns.

Phase 7A.3 adds cross-period analysis — "this is the third consecutive month of impressions decline." That answers "is this a trend?" For reporting, the next question is "is this normal for us?" — how does this month's CTR compare to our own historical range? Is the current impression volume above or below our typical pattern? Is a 12% MoM decline alarming or within our normal variance?

**Phase 7A.4 answers "is this normal for us?" by building percentile distributions from the workspace's own ledger data.** This is the workspace owner's own performance intelligence, not a comparison against other organizations. The data never leaves the workspace — Mosaic does not run servers, does not aggregate across customers, and does not store data on any third-party infrastructure.

---

## What changes from the original framing

The narrative engine plan (ADR-0020 Q8) discussed benchmark aggregation in terms of cross-workspace comparison. **That framing is superseded by this ADR.** Benchmarks in Mosaic are:

- Built from the workspace owner's own ledger (not from other customers' data)
- Stored locally in `.mosaic/benchmark-library.json` (not sent anywhere)
- Computed on-device at CLI time (not uploaded to any server)
- Owned entirely by the workspace owner

This means there is no cross-customer data sharing, no PII exposure to third parties, and no anonymization requirements. The workspace owner already owns all the data in their own ledger.

---

## Decisions

### Decision 1: Benchmark library location and format

**`.mosaic/benchmark-library.json`** — alongside the ledger.

```json
{
  "schema_version": "1.0",
  "generated_at": "2026-05-07T18:00:00Z",
  "workspace": "scotts-rv",
  "period_range": { "from": "2025-11", "to": "2026-04" },
  "period_count": 6,
  "benchmarks": {
    "CTR": {
      "channel": "Targeted Display",
      "p10": 0.08,
      "p25": 0.12,
      "p50": 0.18,
      "p75": 0.24,
      "p90": 0.31,
      "mean": 0.19,
      "stddev": 0.07,
      "sample_count": 6
    }
  }
}
```

Format is plain JSON (not JSONL — it's a snapshot, not a log). Regenerated in full on each `mc model build-benchmarks` run.

**Why:** Co-located with the ledger; workspace-portable (export the `.mosaic/` directory, get the full history + benchmarks); human-readable and inspectable with standard tooling.

---

### Decision 2: No opt-out

Benchmark generation is always available once a ledger exists. There is no opt-out because the benchmarks are built from the workspace owner's own data. Opt-out would mean "don't summarize my own history" — there is no meaningful privacy reason to decline that, since the ledger itself is already the owner's data.

**Why:** Opt-out logic adds implementation complexity with no benefit. The workspace owner controls the ledger; they control the benchmarks by controlling what goes into the ledger.

---

### Decision 3: Benchmark computation pipeline

The `mc model build-benchmarks` verb:

1. **Reads** `.mosaic/analysis-ledger.jsonl` — all entries
2. **Groups** entries by (metric, channel, market) — whatever scope dimensions are present in the ledger
3. **Extracts** the evidence values for each metric per period
4. **Computes** percentile distributions (p10, p25, p50, p75, p90), mean, stddev
5. **Writes** `.mosaic/benchmark-library.json` atomically (tmp + rename, same pattern as the ledger write path)

No external calls. No network I/O. No suppression thresholds. No minimum sample counts (a benchmark with 2 samples is less reliable but still useful — the `sample_count` field tells the reader how much data backs it).

**Why:** Simple pipeline with no special cases. The workspace owner sees all of their own data.

---

### Decision 4: Benchmark query functions in the narrative evaluator

Phase 7A.4 adds a second family of evaluator functions alongside the ledger query functions (Phase 7A.3):

```
benchmark_p50(metric, channel?)  → f64
benchmark_p75(metric, channel?)  → f64
benchmark_p90(metric, channel?)  → f64
benchmark_percentile(metric, value, channel?) → f64   (where does value fall?)
benchmark_above_median(metric, channel?)      → bool (1.0/0.0)
benchmark_z_score(metric, value, channel?)    → f64
```

These functions query the benchmark library (loaded at evaluation time, same as the ledger). When no benchmark library exists, they return 0/Null — templates with benchmark predicates silently don't fire.

**Why:** Same dispatch pattern as ledger functions — function name prefix, cached index, graceful degradation when absent.

---

### Decision 5: CLI verbs

Two new verbs:

**`mc model build-benchmarks <model-dir> [--since <period>] [--channel <name>]`**
- Reads the ledger, computes the benchmark library, writes `.mosaic/benchmark-library.json`
- `--since` narrows which ledger entries contribute (useful for "last 12 months only")
- Prints summary: "Built benchmarks from 47 ledger entries across 6 periods: CTR (p50=0.18%), Impressions (p50=52,430), ..."

**`mc model show-benchmarks <model-dir> [--metric <name>] [--format json|text]`**
- Reads `.mosaic/benchmark-library.json` and displays it
- Text format: human-readable table; JSON format: raw file contents
- Does NOT rebuild — just reads the existing library

**Why:** Separate build vs. display verbs. Build is expensive (reads entire ledger); display is cheap. Explicit rebuild keeps the library deterministic (same inputs → same outputs).

---

### Decision 6: Benchmark templates

New template file `demo/narratives/benchmark-templates.yaml` with templates that reference the benchmark functions:

```yaml
- id: ctr_above_own_median
  family: [display-like]
  severity: success
  table_types: ["Monthly Performance"]
  when: "benchmark_above_median('CTR', current_scope.channel) == 1"
  template: >
    {tactic_name} CTR of {current_ctr:.2f}% is above this campaign's
    historical median ({median_ctr:.2f}%). Performance is in the upper
    half of this workspace's own track record.
  bindings:
    current_ctr: "current.CTR"
    median_ctr: "benchmark_p50('CTR', current_scope.channel)"

- id: ctr_below_own_p25
  family: [display-like]
  severity: warning
  table_types: ["Monthly Performance"]
  when: "benchmark_percentile('CTR', current.CTR, current_scope.channel) < 25"
  template: >
    {tactic_name} CTR of {current_ctr:.2f}% is in the bottom quartile
    of this campaign's own historical performance
    (p25={p25_ctr:.2f}%). This is below the typical range for
    this channel.
  bindings:
    current_ctr: "current.CTR"
    p25_ctr: "benchmark_p25('CTR', current_scope.channel)"
```

**Why:** Benchmark templates follow the same zero-Rust-for-new-templates rule. Adding a benchmark template = adding YAML.

---

### Decision 7: Demo server integration

The demo server:
1. On startup: loads `.mosaic/benchmark-library.json` if present (alongside ledger and regular templates)
2. On each upload: passes the benchmark library to `evaluate_all` (same pattern as the ledger)
3. After narrative evaluation: if no benchmark library exists, skips benchmark templates silently
4. Frontend: benchmark-driven narratives appear in the same narrative list (no separate section needed — they're just more narratives)

The demo server does NOT auto-rebuild the benchmark library on upload. Rebuilding is an explicit CLI action (`mc model build-benchmarks`). This keeps upload latency predictable.

---

### Decision 8: Diagnostic codes MC7040-MC7044

| Code | Condition | Severity |
|---|---|---|
| MC7040 | Benchmark library schema version mismatch (built by newer version) | Warning |
| MC7041 | `benchmark_percentile()` metric not found in library | Warning (template skips) |
| MC7042 | Benchmark library is stale (ledger has entries newer than library `generated_at`) | Info |
| MC7043 | `build-benchmarks` run with fewer than 2 ledger periods (output is valid but noted) | Info |
| MC7044 | Benchmark library write failed (disk full, permission denied) | Error |

---

### Decision 9: Minimum viable scope for Phase 7A.4

Phase 7A.4 ships:
- `build-benchmarks` and `show-benchmarks` CLI verbs
- `benchmark_p50`, `benchmark_p75`, `benchmark_p90`, `benchmark_percentile`, `benchmark_above_median`, `benchmark_z_score` evaluator functions
- `demo/narratives/benchmark-templates.yaml` with 4-6 templates
- Demo server loads benchmark library if present
- MC7040-MC7044 diagnostic codes
- `test_build_benchmarks_from_ledger` and 5 regression tests

Phase 7A.4 does NOT ship:
- Benchmark comparison across workspaces (that remains out of scope — data stays local)
- Benchmark export to external systems
- Benchmark history / versioning (the library is a snapshot, always overwritten)
- Benchmark visualization in the frontend (text narratives only in 7A.4; charts in Phase 7B)

---

## Alternatives considered

### Cross-workspace benchmarks (rejected)

The original framing explored aggregating benchmarks across all customer workspaces — "how does your CTR compare to the industry?" This was rejected because:

1. Mosaic does not run servers or store data on third-party infrastructure
2. Cross-workspace aggregation would require data leaving the workspace, which contradicts the product architecture
3. "Your own historical median" is more actionable than "industry average" — the workspace owner can explain variance in their own data; they cannot explain variance from anonymous others

If cross-workspace benchmarking becomes a product direction, it would require a separate ADR, a network layer, and explicit user consent architecture. Phase 7A.4 explicitly does not go there.

### Embedding benchmarks in the ledger (rejected)

Alternative: compute percentiles on the fly during `narrate`, without a separate library file. Rejected because:
- `narrate` would re-read and re-aggregate the entire ledger on every run — expensive for large ledgers
- The benchmark library can be built from a filtered window (`--since`) that differs from the full ledger
- Separating build from use makes the pipeline auditable (the PM can inspect `.mosaic/benchmark-library.json` to understand what the templates are comparing against)

---

## Success criteria

- [ ] `mc model build-benchmarks` produces a valid `.mosaic/benchmark-library.json` from the Scotts RV sample ledger
- [ ] `benchmark_p50('CTR')` returns the correct value when the benchmark library is loaded
- [ ] A benchmark template fires against a cube where CTR is above the workspace median
- [ ] A benchmark template silently does not fire when no benchmark library exists
- [ ] `mc model show-benchmarks` displays the library in text and JSON formats
- [ ] MC7040-MC7044 codes swept free before implementation
- [ ] `cargo test --workspace` passes with 6+ new tests
- [ ] Demo server loads and applies benchmark library when present
- [ ] Locked surfaces (mc-core, mc-model, mc-fixtures, mc-recipe, mc-drivers, mc-tessera): zero diff

---

*Phase 7A.4 completes the narrative intelligence arc: single-period analysis (7A.1) → durable history (7A.2) → trend detection (7A.3) → own-workspace benchmarks (7A.4). After this phase, the narrative engine can say "CTR is below your historical median for this channel" — deterministically, from structured evidence, using only the workspace's own data.*
