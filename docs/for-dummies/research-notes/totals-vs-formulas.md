# Totals vs Formulas — For Dummies

> **In one line:** in Excel, the user writes `=SUM(...)` to get a total. In this system, totals are *already there* — the user navigates to them. It's a different shape, not a missing feature.

> **No technical counterpart yet.** Most for-dummies notes have a same-named file in [`../../research-notes/`](../../research-notes/). This one doesn't, because it's a hybrid: the *consolidation* part describes shipped Phase 1A behavior; the *ad-hoc calculation* part captures a product question that future phases will need to answer. Treat the second half as scope-capture for a future phase, not as documentation of existing behavior.

---

## The analogy: pivot table, not formula bar

Open a spreadsheet. Click cell `B14`. Type `=SUM(B2:B13)`. Press Enter. You wrote a formula; the formula computes a total.

Now open a pivot table on the same data. Drag "Time" to rows, "Spend" to values. Expand "Q1" to see Jan / Feb / Mar; collapse it to see just the quarterly total. **You wrote no formulas.** The pivot table knows that a quarter is the sum of its months because the *structure* says so.

This system is the pivot table. The model definition (Phase 3A) declares:

- **Hierarchy** — Jan, Feb, Mar are leaves; Q1 is their parent; FY_2026 is the root.
- **Aggregation** — the Spend measure rolls up by `Sum`. CPC rolls up by `WeightedAverage`. Etc.

Every consolidated value is now reachable as a coordinate — no formula needed.

## What's actually happening

When you set up the model, you write something like (in YAML):

```yaml
hierarchies:
  - dimension: "Time"
    edges:
      - { child: "Jan_2026", parent: "Q1_2026", weight: 1.0 }
      - { child: "Feb_2026", parent: "Q1_2026", weight: 1.0 }
      - { child: "Mar_2026", parent: "Q1_2026", weight: 1.0 }
      # ... and so on for Q2, Q3, Q4 ...
      - { child: "Q1_2026",  parent: "FY_2026", weight: 1.0 }
      - { child: "Q2_2026",  parent: "FY_2026", weight: 1.0 }

measures:
  - { name: "Spend", role: "Input", aggregation: "Sum" }
```

That's the entire setup. From now on, any read at coordinate `(..., Q1_2026, ..., Spend)` walks the hierarchy: it finds Jan/Feb/Mar under Q1, looks up Spend at each, sums them, returns the total. The user never typed `=SUM`. The kernel did the work because the *meaning* of "Q1 Spend" is "sum of Jan+Feb+Mar Spend."

The same goes for nested hierarchies. Reading at `(..., FY_2026, All_Channels, USA, Spend)` walks four hierarchies simultaneously: Time (FY → quarters → months), Channel (All_Channels → groups → leaves), Market (USA → regions → states → cities), and just sums the relevant leaves. **One read; the right total comes back.**

The UI's job is just to show the user a navigable grid. Click a row labeled "Q1" — it expands to its three months. Click "FY_2026" — it expands to the four quarters. The numbers are always live; the grid is just a viewer over them.

## Why we care

The Excel mental model says "I have data, and I write formulas to ask questions about it." The planning model says "I declare what things are made of, and I read them at the level I care about."

The trade is real:

- **Excel wins for one-off ad-hoc**. If you need a weird total once and you'll never need it again, writing `=SUM(B2,B7,B11)` in a scratch cell is fast.
- **The planning model wins for repeatability**. Once "Q1 Spend" is structurally meaningful, every user who reads it gets the same answer; no formula drift, no broken references when someone inserts a row, no "wait, why are these two reports different?"

Finance teams pick the planning shape because audit trails matter more than ad-hoc speed. A CFO's quarterly forecast can't have a `=SUM` typo in row 7 of one tab and a corrected version in another. Either Q1 Spend means "the sum of Jan/Feb/Mar Spend" everywhere, or the model is broken.

## One thing that's easy to get wrong

The first instinct from an Excel user is to look for "the cell where I write the formula." There isn't one. Asking "where does the SUM happen?" is like asking "where does Q1's quarter-ness happen?" — it's not a calculation, it's a structural fact.

The Excel-to-planning-model translation:

| Excel question | Planning model answer |
|---|---|
| "Where do I put `=SUM(B2:B13)`?" | Read the consolidated coord. The sum is the read. |
| "How do I add subtotals every 3 rows?" | Add intermediate hierarchy nodes (Q1, Q2, Q3, Q4). Subtotals are now coords. |
| "How do I make a derived column like `Revenue * 0.85`?" | Add a measure with a rule: `body: { mul: [{ ref: "Revenue" }, 0.85] }`. |
| "How do I make a 'what-if' version with different assumptions?" | The Version dimension. Author the same cube under "Aggressive" or "Conservative" — same structure, different inputs. |
| "Where do I put a one-off calculation I just want for this meeting?" | **(open question — see below)** |

That last row is the part the system doesn't have a clean answer for yet.

## The ad-hoc gap (scope-capture for a future phase)

Three flavors of "I want a calculation right now" that don't fit cleanly into the structural model. None of these are currently scoped; this section captures them so a future phase can decide whether/how to address.

### Ad-hoc flavor 1: status-bar sum (cheap)

User selects 5 cells in the grid. The UI shows their sum at the bottom of the screen. Same as Excel's status bar. **Doesn't touch the kernel.** Pure UI feature in Phase 6 territory — read each selected coord, sum the values client-side, display.

**Cost to add:** small. **When it'd belong:** any time after Phase 6 ships a grid view. **Why it's not blocking:** it's a viewing convenience, not a model artifact — no audit trail, no save, no sharing.

### Ad-hoc flavor 2: scratch slice / named view (medium)

User wants to compute something more involved — "Spend × 1.15 for the next 6 months, summed by channel" — for a specific scenario meeting. They'd want to save it as a named view ("Q3 Aggressive Lift Plan") so they can re-open it later.

This sits in an awkward middle ground:

- **Too elaborate to be UI-only** — the calculation has named coords, an aggregation, a saved label.
- **Too one-off to belong in the model** — adding a `Spend_x115` measure to the canonical model just to answer one meeting question pollutes the schema.

The natural shape is a **scratch / sandbox layer** that lives between the canonical model and the UI: lightweight derived definitions saved per-user or per-session, not promoted into the model unless someone explicitly does so. This is roughly what TM1 calls "personal sandboxes" or what some BI tools call "calculated columns."

**Cost to add:** medium. Needs a small kernel addition (or a clean layer above it) for "session-scoped derived measures" that don't get baked into the cube schema. **When it'd belong:** probably Phase 6.x or 7.x. **Why it's worth thinking about:** if it's missing, users will work around it by editing the model file directly, which pollutes the audit trail.

### Ad-hoc flavor 3: model edit ("I want a real new measure / hierarchy node")

User says: "Actually, I always want a US_East rollup of states, not just USA." That's not ad-hoc — it's a model gap that should be fixed in the model.

**Cost to add:** the user (or analyst) edits `model.yaml`, adds the new hierarchy node, reloads. With Phase 4 (LLM authoring), they say "add US_East as a parent of NY, NJ, PA, MA, CT" in plain English and the LLM emits the YAML edit.

**Why it's not really ad-hoc:** if a question is going to be asked twice, it belongs in the model. The discipline is "ad-hoc once is fine; ad-hoc twice is a model edit."

## Where this might land in the phase plan

If/when these get scoped, they'd likely sit at:

- **Flavor 1 (status-bar sum):** part of Phase 6 (UI proof). Cheap UI feature; ships when the grid view does.
- **Flavor 2 (scratch slice / named view):** new sub-phase, probably **Phase 6.x or Phase 7.x**, or possibly its own Phase 5.x if it's needed for the data-import workflow ("import these actuals; show me the sum across channels for verification before committing"). Needs an ADR for the scratch-layer scope (where does it live? per-user or per-session? does it persist across sessions? does it interact with snapshots?).
- **Flavor 3 (model edit):** already covered by Phase 3A (manual YAML edit) and Phase 4 (LLM-driven YAML edit). Not really a new feature — it's the existing path being applied iteratively.

The product question this note should help answer: *do users actually want flavor 2, or do they always want flavor 1 plus the discipline of "if it's recurring, edit the model"?* TM1 shipped flavor 2; some modern planning tools intentionally don't. There's a real strategic call here.

---

*Tied to: [phase-3a.md](../phases/phase-3a.md) (the Phase that introduces the YAML model file users edit for flavor 3), [weighted-average-consolidation.md](./weighted-average-consolidation.md) (how non-Sum aggregations work — same structural shape, different math).*
