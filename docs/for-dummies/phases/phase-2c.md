# Phase 2C — For Dummies

> **In one line:** we stretched the test cube to 100× its normal size to find out what breaks at real-world scale. We didn't fix anything in this phase. We just *measured*.

[Technical version → completion report](../../reports/phase-2c-completion-report.md) · [PERF.md §6.12 / §6.13 / §6.14](../../PERF.md) · [handoff](../../handoffs/phase-2c-handoff.md)

---

## The analogy: stress-testing the bridge before you open it

Imagine the city builds a new bridge. Before they let cars on it, they drive a fleet of dump trucks across in increasing numbers — 1 truck, 10 trucks, 50 trucks, 100 trucks — and watch what happens. Do the deck plates flex? Does the suspension creak? Where does it start to make worrying noises?

That's exactly what Phase 2C did to the kernel.

Up until Phase 2C, every benchmark we'd run was on the **Acme demo cube** — a toy-sized cube with 2,520 input cells. Acme is what the engine was built to run; Acme is what the brief specifies. But finance teams have cubes with **hundreds of thousands of cells**. We had no idea — *literally no data* — about how the engine behaves at that scale.

Phase 2C drove the dump trucks across.

## What we actually did

Three concrete things:

**(1) Built scaled versions of the Acme cube.** Same shape, same rules, same dimensions — just *more cities*. Acme has 7 cities; we built `_10x` (70 cities), `_50x` (350 cities), and `_100x` (700 cities) versions. The total cell count scales the same way: 2,520 cells at 1×, 25,200 at 10×, 126,000 at 50×, 252,000 at 100×.

We did **not** change the kernel. The scaled cubes use the exact same `mc-core` code as Acme; only the test fixture knows about the bigger sizes. This is critical: anything we measured was the kernel's actual behavior at scale, not some special "scale mode."

**(2) Re-ran every existing benchmark at the bigger sizes.** All five of the bench files Phase 1B / 2A / 2B had built — cold reads, warm reads, leaf writes, dirty propagation, snapshot/rollback — got new variants tagged `_10x`, `_50x`, `_100x`. About 27 new bench rows total.

**(3) Built one *new* benchmark called `combined_workflow`** that simulates what a real planner would actually do during a session: open the cube, do 100 edits, take a snapshot every 10 edits and *keep the snapshots live* (a "stacked sandbox" — the TM1 pattern that finance teams have used for forty years), and read consolidated values along the way. This is the closest thing we have to "what does an actual user feel like?"

Then we wrote everything up in `PERF.md` §6.12 / §6.13 / §6.14.

## What we found

Two findings, one expected and one not.

**Expected finding (the calm part).** Within a session — once the cube is loaded — per-edit cost stays *flat*. You can do 100 edits in a row, hold 10 snapshots live, and the 100th edit costs the same as the first. No slow-creep. The engine handles a planner session at 50× scale fine; an edit takes ~2 ms, a recompute is sub-100 ms. Finance teams would not notice latency.

**Unexpected finding (the cliff).** **Bulk-loading inputs into a 50× cube takes 231 seconds.** Almost four minutes. That's *23× slower than acceptable* (the patience-limit gate from ADR-0003 says bulk imports should be ≤ 10 seconds). And the trajectory between sizes is **super-linear**: at 10× scale, each write costs about 4.33× more than at 1×; at 50× scale, each write costs **19.7× more than at 1×**. The cost-per-write doesn't just go up with cube size; it goes up *faster* than cube size goes up.

We tried 100× and gave up — the bench tool estimated each row would take more than 38 minutes, and we'd already learned what we needed to learn.

That cliff between 10× and 50× is the headline. It's the entire reason Phase 2D exists.

## Why we care / what would have happened if we didn't do this

Three things would have gone wrong without Phase 2C:

**(1) We would have optimized blind.** Going into Phase 2 we had a list of *suspected* slow spots in the engine — five or six candidates labeled §9.2, §9.3, §9.4, §9.5, §9.6 in PERF.md. Without scale data, we'd have had to *guess* which one was the real problem. We might have spent six weeks polishing §9.2 (per-write fixed cost) and made the engine 30% faster on benchmarks while the actual user-facing problem (bulk-load takes four minutes) sat untouched.

Phase 2C made it unambiguous: the cliff is in §9.3 (the dirty-list data structure), not §9.2. That's not a guess; that's measured.

**(2) We would have been blindsided in production.** A finance team migrates their planning cube. It's 250,000 cells. They click "Import." The engine sits there for four minutes. They Slack the team: *"Is this thing broken?"* That's how product-market fit dies. Phase 2C catches the failure on a synthetic cube in a benchmark, weeks before any real user sees it.

**(3) We would have argued instead of measured.** "Is the dirty-list a problem?" "Maybe — let me think about it." vs. "Yes — at 50× scale each write costs 19.7× more than at 1×, and the dirty list is the only thing that grows during bulk load, see PERF.md §6.12.7." Measurement closes arguments. The whole point of an ADR-driven culture is that decisions are backed by data, not by who-talks-loudest.

## One thing that's easy to get wrong

The natural reaction to seeing "231 seconds for a bulk-load" is "let's fix it!" — and Phase 2C *deliberately did not.* The phase rule was **measurement only — no kernel source change**. Why? Because a phase that measures *and* optimizes can't tell you which optimization actually mattered. If we'd measured AND swapped the dirty-list AND added some other tweak in one phase, we wouldn't know which change drove the improvement (or the regression). By keeping Phase 2C measurement-only, we now have a clean baseline (`phase-2c-workload-baseline`, tag at commit `789db15`) that Phase 2D's optimization will diff against. Improvement attributable to *one specific change*, end of argument.

The other thing that's easy to misread: Phase 2C did **not** prove the engine is broken. The engine works perfectly at 50× scale within an interactive session — edits are fast, reads are fast, snapshots are fast. The *only* place the cliff shows up is in the *bulk-load path* (importing all initial values at once). Phase 2D fixes that one path; everything else is already fine.

## What Phase 2C is and isn't

| It is | It isn't |
|---|---|
| Diagnostic — an MRI scan of the kernel at scale | Treatment — no kernel code changed |
| A baseline for future optimization to diff against | A complete production-workload benchmark |
| The reason Phase 2D's pick is anchored in real numbers | An exhaustive coverage of every possible cube shape |
| A measurement of *Acme-shape cubes scaled by Market dim* | A measurement of cubes with deeper hierarchies, more measures, etc. |

If a future phase needs to cover *other* cube shapes (more measures, deeper hierarchies, weirder rule chains), that's a new phase, with its own scaled-fixture builder, its own baseline, and its own findings.

---

*Tied to: [phase-2d.md](./phase-2d.md) (the next phase, which acts on Phase 2C's cliff finding), [`../research-notes/dirty-propagation-as-per-write-delta.md`](../research-notes/dirty-propagation-as-per-write-delta.md) (the dirty-list data structure that Phase 2C identified as the cliff source).*
