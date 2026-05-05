# ADR-0001 Amendment 1: Flexible Dimension Count (4 required + N domain-specific)

**Status:** Proposed
**Date:** 2026-05-05
**Filed by:** PM, after the NBA totals cartridge surfaced the 6-dim rigidity
**Amends:** [ADR-0001](./0001-phase-1-scope.md) (Phase 1 scope — canonical 6 dimensions)

---

## What changed

### The original rule (ADR-0001 / brief §3)

> Every Mosaic cube has exactly 6 dimensions in the canonical order:
> `[Scenario, Version, Time, Channel, Market, Measure]`

This was correct for Phase 1's marketing-finance proof domain (the Acme demo). Every marketing model naturally has Channel (Paid_Search, Email, etc.) and Market (Tampa, Orlando, etc.).

### The problem

Non-marketing domains don't have "Channel" and "Market":

| Domain | Natural dimensions | Forced workaround |
|---|---|---|
| Sports betting | Scenario, Version, Time, **Game**, Measure | Stuff "Game" into "Channel" or "Market"? Add a singleton "Market: All"? |
| SaaS metrics | Scenario, Version, Time, **Product/Plan**, **Segment**, Measure | Which one is "Channel"? Which is "Market"? |
| Sales pipeline | Scenario, Version, Time, **Rep**, **Stage**, Measure | Neither Rep nor Stage maps to Channel/Market |
| Stock forecasting | Scenario, Version, Time, **Ticker**, **Sector**, Measure | Ticker ≠ Channel; Sector ≠ Market |
| FP&A | Scenario, Version, Time, **Department**, **Entity**, Measure | Department isn't a Channel |
| Demand planning | Scenario, Version, Time, **Product**, **Region**, Measure | Close to marketing, but not identical |

The NBA totals cartridge (the first real non-marketing model) has 5 natural dimensions: Scenario, Version, Time, Game, Measure. Forcing it to 6 means either:
- A singleton placeholder dim (`Market: "All"`) that bloats every coordinate with a meaningless slot
- Misusing a dim name ("Channel" holding game identifiers, confusing the LLM and the user)

Both are worse than just allowing 5 dims.

### The fix

**Relax from "exactly 6 in a fixed order" to "4 required + N domain-specific in a declared order."**

---

## Decision

### Required dimensions (4 — always present, always in this relative order)

| Position | Dimension | Kind | Why required |
|---|---|---|---|
| First | Scenario | `"Scenario"` | Plan-vs-actual is fundamental to planning |
| Second | Version | `"Version"` | Draft-vs-approved is fundamental to workflow |
| Third | Time | `"Time"` | Temporal axis is fundamental to forecasting |
| Last | Measure | `"Measure"` | The kernel's measure-slot convention is structural |

### Domain-specific dimensions (0+ — user declares them between Time and Measure)

The slots between Time (position 3) and Measure (last position) are user-defined. A model can have 0, 1, 2, or more domain-specific dims:

```yaml
# Marketing (the Acme pattern — 2 domain dims):
dimensions:
  - { name: "Scenario", kind: "Scenario", ... }
  - { name: "Version", kind: "Version", ... }
  - { name: "Time", kind: "Time", ... }
  - { name: "Channel", kind: "Standard", ... }    # domain-specific
  - { name: "Market", kind: "Standard", ... }     # domain-specific
  - { name: "Measure", kind: "Measure", ... }

# Sports betting (1 domain dim):
dimensions:
  - { name: "Scenario", kind: "Scenario", ... }
  - { name: "Version", kind: "Version", ... }
  - { name: "Time", kind: "Time", ... }
  - { name: "Game", kind: "Standard", ... }       # domain-specific
  - { name: "Measure", kind: "Measure", ... }

# Simple forecast (0 domain dims):
dimensions:
  - { name: "Scenario", kind: "Scenario", ... }
  - { name: "Version", kind: "Version", ... }
  - { name: "Time", kind: "Time", ... }
  - { name: "Measure", kind: "Measure", ... }

# Complex enterprise (3 domain dims):
dimensions:
  - { name: "Scenario", kind: "Scenario", ... }
  - { name: "Version", kind: "Version", ... }
  - { name: "Time", kind: "Time", ... }
  - { name: "Department", kind: "Standard", ... }  # domain-specific
  - { name: "Entity", kind: "Standard", ... }      # domain-specific
  - { name: "Product", kind: "Standard", ... }     # domain-specific
  - { name: "Measure", kind: "Measure", ... }
```

### Validation rules (replacing the old "exactly 6" check)

1. **Minimum 4 dimensions.** (Scenario + Version + Time + Measure)
2. **First dim must be `kind: "Scenario"`.** (Position 0)
3. **Second dim must be `kind: "Version"`.** (Position 1)
4. **Third dim must be `kind: "Time"`.** (Position 2)
5. **Last dim must be `kind: "Measure"`.** (Last position, regardless of total count)
6. **All dims between Time and Measure must be `kind: "Standard"`.** (Domain-specific dims)
7. **Maximum 10 dimensions total.** (Practical limit to prevent pathological sparsity; lint warning at >7)
8. **No duplicate dim names.** (Existing rule, unchanged)

### What this changes in the kernel

**`CellCoordinate`** is currently a fixed-size 6-slot array. It becomes a **variable-length** array (sized at cube-construction time based on the model's dimension count).

Implementation options:

**Option A (minimal change):** `CellCoordinate` becomes `SmallVec<[ElementId; 6]>` — still stack-allocated for 6 dims (common case), heap-allocated for 7+. The `6` in `SmallVec` is a capacity hint, not a hard limit.

**Option B (zero-cost for existing models):** Keep the fixed-6 internal representation but allow the YAML to declare 4-6 dims. Models with <6 dims get synthetic singleton dims inserted during compile (invisible to the user). The NBA cartridge declares 5 dims; the compiler inserts a hidden singleton "All" dim at the right position. User never sees it; coordinates still work.

**Option C (cleanest long-term):** `CellCoordinate` is `Vec<ElementId>` (heap-allocated, any size). Slightly slower for the 6-dim case (~5ns overhead per coord construction) but simplest implementation.

**My recommendation: Option B for now (zero-cost for existing models; no kernel perf regression), with a note that Option A is the migration path if we ever need >6 dims to be first-class at the kernel level.**

Option B means:
- The model YAML validator accepts 4-10 dims
- The compile step inserts synthetic singleton dims to pad to 6 (if the model has <6)
- The kernel stays at fixed-6 internally (no CellCoordinate change, no perf regression)
- `mc model inspect` hides the synthetic dims from the user
- Rules + goldens + CSV reference only the user-declared dims (the synthetics are invisible)

This is identical to what Acme does today if you declare a dim with one element — except the system does it for you instead of making you write `Channel: [{ name: "All" }]` in the YAML.

---

## Consequences

### Positive

- Non-marketing domains can be modeled naturally (sports: 5 dims; SaaS: 6-7 dims)
- No more singleton placeholder dims cluttering the user's model
- The NBA cartridge works without workarounds
- LLM authoring becomes simpler (the `skills/schema-design/SKILL.md` no longer needs to teach "if your domain doesn't have a Channel, add a dummy one")
- Future domains (stocks, FP&A, demand) can use exactly the dims they need

### Negative / accepted trade-offs

- The Acme demo and all existing models (6 dims) are unchanged — Option B is fully backwards-compatible
- CellCoordinate internally stays at 6 slots (no perf regression for existing workloads)
- The "canonical 6-dim order" rule in CLAUDE.md needs updating (it currently says "exactly, always")
- The `skills/schema-design/SKILL.md` needs updating (dim order section)

### Reversal cost

Low. The change is additive (accept more dim counts). Reverting means rejecting models that currently validate, which is a breaking change — but since no production models with <6 or >6 dims exist yet (only the draft NBA cartridge), reversal is cheap if done before any non-6-dim model ships.

---

## Implementation scope

| Change | Location | Effort |
|---|---|---|
| Validator: accept 4-10 dims instead of exactly 6 | `mc-model/src/validate.rs` | ~20 lines |
| Validator: check kind order (Scenario, Version, Time, ...Standard..., Measure) | Same file | ~30 lines |
| Compiler: insert synthetic singleton dims to pad to 6 if <6 declared | `mc-model/src/compile.rs` | ~50 lines |
| Inspect: hide synthetic dims from output | `mc-model/src/inspect.rs` | ~10 lines |
| CSV loader: map user-declared dim names to kernel dim slots (accounting for synthetics) | `mc-model/src/resolve_inputs.rs` or equivalent | ~30 lines |
| Update CLAUDE.md §2.16 (coordinate slot order) | `CLAUDE.md` | ~5 lines |
| Update `skills/schema-design/SKILL.md` Rule 1 | Plugin skill | ~20 lines |
| Fix NBA cartridge to use 5 dims (remove placeholder) | `examples/sports-betting/nba-totals.yaml` | ~10 lines |
| **Total** | | **~2-3 hours** |

No kernel changes (mc-core stays at 6-slot CellCoordinate). The flexibility is purely in the model layer (mc-model) which pads models with <6 dims to 6 during compile.

---

## Alternatives considered

1. **Keep exactly 6 dims, force singleton placeholders.** Rejected — every non-marketing domain writes `Channel: [All]` or `Market: [All]` which is meaningless, confusing, and bloats every coordinate with a slot nobody uses.

2. **Allow any number of dims with any order.** Rejected — too permissive. Scenario/Version/Time/Measure have special kernel semantics (scenario_meta, version_state, time_anchor, measure roles). They must be present and in a known position for the kernel to function correctly.

3. **Option A (SmallVec CellCoordinate).** Deferred — not needed for the immediate fix. Option B (pad during compile) is zero-cost for existing models and doesn't require a kernel data-structure change. Option A is the migration path if models with >6 dims become common enough to justify the CellCoordinate change.

4. **Option C (Vec CellCoordinate).** Rejected for Phase 1 due to the ~5ns per-coord overhead on every read/write. The kernel's hot paths construct coordinates millions of times; even small overhead matters at scale. Option B avoids this entirely.

---

## Diagnostic codes

| Code | Fires when |
|---|---|
| MC2060 | Model has fewer than 4 dimensions |
| MC2061 | First dimension is not `kind: "Scenario"` |
| MC2062 | Second dimension is not `kind: "Version"` |
| MC2063 | Third dimension is not `kind: "Time"` |
| MC2064 | Last dimension is not `kind: "Measure"` |
| MC2065 | A dimension between Time and Measure is not `kind: "Standard"` |
| MC3020 | Model has more than 7 dimensions (lint warning — sparsity concern) |
| MC2066 | Model has more than 10 dimensions (hard cap) |

---

## Cross-links

- [ADR-0001](./0001-phase-1-scope.md) — the original "exactly 6 dims" rule this amends
- [CLAUDE.md §2.16](../../CLAUDE.md) — coordinate slot order (needs update)
- [skills/schema-design/SKILL.md](../../mosaic-plugin/skills/schema-design/SKILL.md) — Rule 1 (needs update)
- [NBA totals cartridge](../../examples/sports-betting/nba-totals.yaml) — the first model that benefits
- [ADR-0014](./0014-time-representation.md) — Time-kind dimension semantics (unchanged; Time is still position 2)
- [ADR-0011](./0011-phase-3e-conditionals-and-basic-operations.md) — actual_ref + actuals_element (unchanged; Scenario is still position 0)

---

## Notes

This is a purely model-layer change. The kernel (`mc-core`) continues to use 6-slot `CellCoordinate` internally. The model layer (`mc-model`) gains the ability to accept 4-10 dims in the YAML and pads to 6 during compile by inserting invisible singleton dims. Existing 6-dim models (Acme, Tide Cleaners) are completely unaffected.

The "exactly 6 dims" rule served its purpose in Phase 1: it simplified the kernel, prevented scope creep, and made the Acme demo self-contained. Now that non-marketing domains are being modeled (sports betting, SaaS, stocks), the rigidity is a liability rather than a feature. This amendment retires the rigidity while preserving the kernel's internal invariant (6-slot coordinates) through compile-time padding.

**The LNM (Large Numbers Model) vision requires domain flexibility.** A planning substrate that can ONLY model things with exactly 6 dimensions named Scenario/Version/Time/Channel/Market/Measure isn't a substrate — it's a marketing-finance tool with delusions of generality. This amendment makes the generality real.
