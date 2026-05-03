# Phases — For Dummies

> **Plain-English versions of what each phase actually did and why.** If you find yourself reading a completion report or a handoff and your eyes glaze over, come here first. Same content, same conclusions, just translated into normal-human.

The phases section walks through the *optimization phases* (Phase 2 and onward). Phase 1 was building the kernel itself — you can skip that one for now; it's not where the interesting questions are. Phase 2's job is to *measure* the kernel, find what's slow, and fix it surgically. Phase 3 is the big leap into letting users author cubes without writing Rust.

## The phases at a glance

| Phase | One-line takeaway | Status | For-dummies note |
|---|---|---|---|
| 1A | Build the engine. | complete | (no note yet) |
| 1B | Run the first benchmarks at toy size. | complete | (no note yet) |
| 2A | Add cold-start benchmarks. *Reads from a "fresh wake-up" cube, not a warmed-up one.* | complete | (no note yet) |
| 2B | Stop copying a giant chunk of memory on every consolidated read. | complete | (no note yet) |
| **2C** | **Stretch the test cube to 100× the toy size and see what falls over.** | **complete** | **[phase-2c.md](./phase-2c.md)** |
| **2D** | **Fix the thing that fell over: replace the dirty-list with a hotel-room-light-board (and find a Phase 1A bug along the way).** | **complete** | **[phase-2d.md](./phase-2d.md)** |
| **3A** | **Let humans author cubes by writing a YAML file instead of Rust. New `mc-model` crate; kernel doesn't change.** | **complete** | **[phase-3a.md](./phase-3a.md)** |
| **3B** | **Add the four-verb mental model for YAML authors: validate / inspect / lint / test. 10 lint rules + structured diagnostics for future LLM + UI consumption.** | **complete** | **[phase-3b.md](./phase-3b.md)** |
| **3C** | **Make model files self-contained: delete the embarrassing Acme-name special case in the CLI; let YAML models declare their own input data via a new `canonical_inputs:` block (sibling CSV or inline tabular).** | **complete** | **[phase-3c.md](./phase-3c.md)** |
| **3D** | **Excel's formula bar comes to YAML: rule bodies can now be authored as `"Customers * AOV"` instead of nested s-expression-shaped objects. Both forms still work; the kernel doesn't know the difference.** | **complete** | **[phase-3d.md](./phase-3d.md)** |

## How to read these

Each phase note is structured the same way:

1. **The analogy** — the everyday thing the phase is shaped like.
2. **What we actually did / are about to do** — same idea, in MarketingCubes terms, still in plain English.
3. **Why we care / what would happen if we didn't** — the bug, slowness, or surprise this phase prevents.
4. **One thing that's easy to get wrong** — the gotcha.

## What this section does NOT cover

This section is about **what each phase did and why**. It does NOT cover:

- The *technical contracts* (those live in [`../../specs/`](../../specs/) and are intentionally machine-precise).
- The *moment-to-moment decisions* during a phase (those live in [`../../decisions/`](../../decisions/) — Architecture Decision Records).
- The *audit trail* that proves the phase actually shipped what it claimed (those live in [`../../reports/`](../../reports/)).
- Any *cross-cutting concept* that applies to multiple phases — see [`../research-notes/`](../research-notes/) for things like "how dirty propagation works" or "why snapshots are deep-clones."

The for-dummies phase notes are a **why-and-what** layer over those documents, not a replacement for them.
