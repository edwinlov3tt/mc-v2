# Phase 2D — For Dummies

> **In one line:** Phase 2C found that bulk-loading a 50× cube takes four minutes. Phase 2D replaces the data structure that's slowing it down with a different one that's O(1) — same speed regardless of how big the cube gets. Goal: drop bulk-load from 231 seconds to under 50.

[Technical version → handoff](../../handoffs/phase-2d-handoff.md) · [PERF.md §6.14](../../PERF.md) · [readiness audit](../../reports/phase-2d-readiness-audit.md)

---

## The analogy: paper checklist vs. hotel-room key board

Imagine you run a 30-room bed-and-breakfast. Every room has a key on a hook behind the front desk. You need to know two things at any moment: *(a)* is room 12 occupied right now? *(b)* which rooms are vacant?

**Approach 1: paper checklist.** Every time a guest checks in, you write the room number on a piece of paper. Every time they check out, you cross it off. To find out if room 12 is occupied, you scan the paper for "12." Easy when the list has 5 entries. Painful when it has 500.

**Approach 2: light-board behind the desk.** You build a board with one light bulb per room — 30 bulbs, in a 6×5 grid. Bulb on = occupied; bulb off = vacant. Check-in: flip switch 12 to on. Check-out: flip switch 12 to off. Want to know if room 12 is occupied? Look at bulb 12. Want to see all vacant rooms? Scan the board. **The cost of any single operation never grows**, no matter how full the inn gets — it's always one flip or one glance.

The paper checklist is what the kernel uses today. It's called an `AHashSet<CellCoordinate>` — a hash-set of cell addresses. As more entries get added, the set has to "rehash" itself periodically (like rewriting your paper checklist on a fresh sheet because the old one is messy), and finding out whether an entry is already in the set involves probing through buckets that get more crowded as the set fills up. At 5 entries: trivial. At 305,000 entries (which is what Phase 2C measured at 50× scale): *each operation costs measurably more than it did at 5*. That's the cliff.

The light-board is a **bitset** — a fixed-size array of bits, one per possible room. Each cell in the cube gets a unique number from 0 to N (where N is the total number of possible coordinates), and "is dirty" becomes "is bit 47,213 set?" One memory access. Same speed at 5 dirty cells as at 5 million.

Phase 2D rips out the paper checklist and bolts on the light-board.

## What Phase 2D will actually do

Three concrete pieces of work:

**(1) Compute "the shape of the cube" once at build time.** When you create a cube, the engine knows exactly how many elements each dimension has — Acme has 3 scenarios × 3 versions × 17 time periods × 8 channels × 15 markets × 11 measures. That's `3 × 3 × 17 × 8 × 15 × 11 = 201,960` possible coordinates, total. Multiplied that out once, stored. We call this number the cube's *cardinality*; the small struct that holds the per-dimension breakdown is called `CubeShape`.

**(2) Replace the dirty-list's internal storage.** The `DirtyTracker` struct currently holds a hash-set. Phase 2D changes its internals to hold a bit-array sized to the cube's cardinality. **The public methods stay exactly the same** — `mark`, `clear`, `is_dirty`, `iter`, `len`, etc. — so nothing outside this one struct has to know anything changed. Internally, every `mark(coord)` becomes "compute coord's index → flip the bit at that index"; every `is_dirty(coord)` becomes "look at the bit." Both are O(1) and don't slow down as the dirty set grows.

**(3) Prove the new representation behaves identically to the old one.** Phase 2D ships with a kernel unit test that builds *both* a fresh AHashSet-backed tracker and a fresh bitset-backed tracker, drives the exact same sequence of mark/clear operations against both, and asserts they agree on every observable answer (`is_dirty`, `len`, the contents of `iter`). If that test passes, every higher-level test that depended on the old dirty-list semantics inherits the equivalence.

The acceptance gate is single and load-bearing: **the 50× bulk-load row drops from 231 seconds to under 50 seconds**. That's a 4.6× improvement. Other rows (write_input_leaf, dirty_propagation) will likely improve by smaller amounts as a free side-effect.

## Memory check (because "one bit per possible coordinate" sounds scary)

It isn't, at our scales:

| Scale | Possible coordinates | Bitset size |
|---|---:|---:|
| Acme (1×) | 201,960 | ~25 KB |
| 10× | 1,050,192 | ~128 KB |
| 50× | 4,820,112 | ~588 KB |
| 100× | 9,532,512 | ~1.16 MB |

A megabyte of memory at the largest calibration scale. That's nothing — your laptop just used more than that to render this paragraph. The handoff has a *cardinality-explosion guard* that falls back to the old hash-set representation if some future cube has more than ~128 MB worth of bits, but no current calibration scale comes anywhere close.

## Why we care / what would happen if we didn't do this

Three things would go wrong without Phase 2D:

**(1) Bulk-loading at production scale would be unacceptable.** The 231-second number from Phase 2C isn't a benchmark curiosity — it's "the engine is unusable at 50× scale." Finance teams importing a quarterly forecast can't sit through four minutes of "Loading..." before they see their data. The first real customer would import their cube, the import would take minutes, and they'd Slack their team to ask if the tool is broken. Phase 2D is *not* a polish — it's the difference between "this works" and "this doesn't."

**(2) The cliff hides what's *next*.** As long as bulk-load is the dominant cost at scale, every other optimization candidate (read paths, snapshot copy, rule eval) is invisible behind it. You can't profile what you can't reach. Phase 2D removes the cliff so the next phase can see whatever comes after.

**(3) We'd be deferring a measured problem indefinitely.** It's tempting to say "well, it's slow, but we have other things to build." That's how technical debt becomes paralyzing. The §9.3 candidate has been on the suspected-slow list since Phase 1B; Phase 2C's cliff data finally gave it the load-bearing evidence to act on. Choosing to *not* fix it now would mean every future phase has to work around it — caching layers, partial-load fallbacks, "import in the background" UI tricks — all to paper over a problem that one surgical kernel change makes go away.

## One thing that's easy to get wrong

The temptation when a phase says "swap this data structure for a new one" is to *also* swap surrounding pieces while you're in there. **Don't.** Phase 2D's hard rule is that the source change is confined to two files (`dirty.rs` + `cube.rs`, optionally a third for the new `CubeShape`). Touching anything else — `consolidation.rs`, `rule.rs`, `dependency.rs` — is a sign the scope has crept and the phase will produce mixed results that can't be cleanly attributed.

The other thing that's easy to misread is the relationship between Phase 2C's *within-session flatness* finding and Phase 2D's *cross-scale cliff* finding. Phase 2C measured both, and they look like they contradict: "per-edit cost is flat across a 100-edit session" vs. "per-edit cost grows super-linearly between 10× and 50× scale." The reconciliation: **the cliff is in the bulk-load phase**, when the dirty set grows from 0 to 305,000 entries. *Within a session*, the dirty set was *already* full from the bulk-load that happened before the session started — so adding 100 more entries to a 305,000-entry set looks the same as adding 100 to a 305,005-entry set. Same data structure, same growth pain, just observed in different windows. Phase 2D fixes both windows because the bitset is O(1) regardless of which window you're looking at.

A subtlety the technical handoff makes a big deal of: the new bitset's `iter()` method walks set bits in deterministic order; the old hash-set's `iter()` was non-deterministic across runs. Tests that already followed the project's "always sort before comparing iter contents" rule (per CLAUDE.md §2.11) will pass unchanged — the bitset's deterministic order is *strictly stronger* than the hash-set's nondeterministic one, so anything that passed before passes now. Tests that *didn't* follow the rule were buggy already; Phase 2D doesn't try to fix those.

## What Phase 2D is and isn't

| It is | It isn't |
|---|---|
| A surgical kernel change confined to 2–3 files | A general-purpose dirty-tracker rewrite |
| A fix for the §9.3 cliff Phase 2C measured | A fix for §9.2 (per-write fixed cost), §9.5 (snapshot COW), or any other suspected slowdown |
| Backed by a kernel unit test that proves equivalence | A blind swap that hopes for the best |
| Single load-bearing acceptance gate (50× bulk-load ≤ 50 s) | A multi-target performance push |
| A possible *Phase 2 exit* — if 2D succeeds and no other cliff surfaces, Phase 2 is done | A guaranteed Phase 2D, period — it has explicit rollback paths if the bitset implementation balloons |

If Phase 2D's bitset implementation grows past ~250 lines or breaks any contract test in a non-trivial way, the handoff has two named fallback paths: switch to a Roaring Bitmap (Option B; new dependency, requires an ADR) or use a hashed `CellCoordinate` (Option C; smaller win but less risk). Neither is a Phase 2D scope rewrite — both are amendments to the same phase.

---

*Tied to: [phase-2c.md](./phase-2c.md) (the previous phase, which produced the cliff finding that Phase 2D acts on), [`../research-notes/dirty-propagation-as-per-write-delta.md`](../research-notes/dirty-propagation-as-per-write-delta.md) (the dirty-list concept the bitset replaces).*
