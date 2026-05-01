# Snapshot as Deep-Clone — For Dummies

> **In one line:** taking a "snapshot" of the cube means literally photocopying the whole thing. No clever shared-memory tricks. The simplest possible thing that works, on purpose.

[Technical version →](../../research-notes/snapshot-as-deep-clone.md)

---

## The analogy: Save As

You're working on a Word document — your Q3 budget spreadsheet, say. You hit *File → Save As* and name it `Q3_Approved.docx`. Now there are two files on your hard drive: `budget.docx` (your live working copy) and `Q3_Approved.docx` (a frozen point-in-time copy).

If you keep editing `budget.docx`, the `Q3_Approved.docx` file doesn't change. They're independent. That's "Save As."

A more clever approach would be **delta storage** — what Git does for source code, what some database systems do under the hood. Instead of duplicating the whole document, the system would just record *"Q3_Approved is the working copy as of yesterday plus a list of cells that have changed since."* That saves disk space because the unchanged parts are stored once and shared.

We're doing the dumb thing. Save As. Full photocopy. Why? Because it's simpler, it's correct by construction, and at the size we're dealing with (about 25,000 cells = under a megabyte), the photocopy takes microseconds. The "clever" approach has at least three places where bugs could hide; the photocopy approach has zero. Save As wins.

## What's actually happening

The cube has all its data sitting in a `HashMapStore` — basically a giant key-value dictionary mapping *(cell address)* → *(stored value, who wrote it, when, what revision)*.

When you call `cube.snapshot("Q3_Approved")`, the engine does this:

1. Make a complete copy of the entire HashMapStore.
2. Wrap it in a `Snapshot` struct alongside the current revision number and the label.
3. Hand the Snapshot back to whoever asked.

That's it. There's no shared memory, no clever pointers, no "copy-on-write." The Snapshot owns its own independent copy of the data. If the cube gets edited 5 minutes later, the Snapshot stays untouched.

When you call `cube.rollback_to(snapshot)`, the reverse happens:

1. Replace the cube's live store with a fresh photocopy of the snapshot's store.
2. Bump the cube's revision number.
3. Wipe the dirty list.
4. Drop any cached derived-leaf values (their formula version might no longer match).

That's the whole feature.

## Why we care

The cube needs snapshots so finance teams can do their workflow:

- *"Take a snapshot, label it 'Q3_Approved' — this is what we presented to the board."*
- *"Now keep editing the working copy for Q4 planning."*
- *"Oh wait, somebody messed up. Roll back to 'Q3_Approved'."*

The dumb-photocopy approach makes this iron-clad. The snapshot is genuinely a frozen point in time. There's no "wait, did this leak through and corrupt my snapshot?" question — because it's a separate copy.

The reason the spec is so explicit (*"No COW. No persistence. No cleverness."*) is that the temptation to optimize this is enormous to a programmer. But: until we have a real workload that proves photocopying is too slow, every line of cleverness is a line where bugs hide. Phase 1 ships with the dumb shape. Phase 2 measures it. If it's actually slow at scale, *then* we get clever, with real numbers backing the change.

## One thing that's easy to get wrong

It's tempting to assume the snapshot is somehow "live" — that if you edit the cube, the snapshot updates too. It does **not**. The snapshot is genuinely independent and frozen.

The other tempting "clever" move is to make the snapshot only record changes since the previous snapshot. That'd be a delta scheme. Don't. The brief and the operating manual are both explicit: full clone, every time, no cleverness. If we measure later that this is too slow, we'll write an Architecture Decision Record explaining why we changed our mind, and Phase 2 will replace the dumb shape with a measured one. Until then, dumb is correct.

A subtlety: the rollback drops cached derived-leaf cells but keeps cached consolidated cells. That asymmetry is on purpose — the consolidated values are pure math over the hierarchy structure and stay valid post-rollback, while the derived ones reference rule versions that may not be current anymore. It's a small thing but it explains why "rollback" doesn't completely empty the cache.

---

*Tied to: [two-caching-layers](./two-caching-layers-in-read.md) (the asymmetric pruning of cached values on rollback), [lazy-dependency-graph](./lazy-dependency-graph.md) (the dependency graph itself isn't rolled back — it just gets re-populated by the next reads).*
