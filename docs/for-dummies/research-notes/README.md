# Research Notes — For Dummies

> **Plain-English versions of the technical research notes.** Read these first; if you want the deep dive, the same-named file in [`../../research-notes/`](../../research-notes/) has every line of code referenced.

The research notes capture *non-obvious choices* the engine makes — decisions that someone reading the code would miss the reason for unless they happened to read the spec, the completion report, AND the operating manual. This translation gives you the gist with an analogy you can hold onto.

## The 6 notes

| For-dummies | One-line takeaway | Technical version |
|---|---|---|
| [`lazy-dependency-graph.md`](./lazy-dependency-graph.md) | "We don't write down which recipes use eggs until you actually cook one." | [→](../../research-notes/lazy-dependency-graph.md) |
| [`dirty-propagation-as-per-write-delta.md`](./dirty-propagation-as-per-write-delta.md) | "What changed *because of this edit*" vs "what's dirty in the whole cube." | [→](../../research-notes/dirty-propagation-as-per-write-delta.md) |
| [`null-vs-zero-vs-nan.md`](./null-vs-zero-vs-nan.md) | A blank tax-form field, a zero on a tax-form field, and a CALCULATOR-ERROR on a tax-form field are three different things. | [→](../../research-notes/null-vs-zero-vs-nan.md) |
| [`weighted-average-consolidation.md`](./weighted-average-consolidation.md) | Your GPA isn't the average of your course grades — it's the *credit-weighted* average. Same for CPC. | [→](../../research-notes/weighted-average-consolidation.md) |
| [`two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) | Two scratch-pads sitting next to the engine to remember answers it already worked out. | [→](../../research-notes/two-caching-layers-in-read.md) |
| [`snapshot-as-deep-clone.md`](./snapshot-as-deep-clone.md) | "Save As" makes a full photocopy. We just photocopy. The fancy stuff comes later. | [→](../../research-notes/snapshot-as-deep-clone.md) |

## How to read these

Each one is structured the same way:

1. **The analogy** — the everyday thing this technical concept is shaped like.
2. **What's actually happening in the engine** — the same idea, but in MC terms, still in plain English.
3. **Why we care / why it'd be wrong otherwise** — the bug or confusion this discipline prevents.
4. **One thing that's easy to get wrong** — the gotcha.

If something here goes too deep, that's a bug in the writing — let me know.
