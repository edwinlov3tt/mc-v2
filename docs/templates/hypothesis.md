# H[NNN]: [One-line claim]

**Status:** `open | testing | confirmed | rejected | superseded`
**Filed:** YYYY-MM-DD
**Last touched:** YYYY-MM-DD
**Phase context:** `1A post-ship | 1B prep | 2 prep | …`

---

## Claim

The specific assertion in one sentence. Avoid weasel words.

## Why we care

What changes if this is true? What changes if it's false? If neither outcome changes anything, this hypothesis isn't worth filing.

## Prerequisites

What must already exist before we can test? Examples:

- A specific fixture or benchmark harness.
- Particular tooling (e.g., criterion working under Rust 1.78).
- A feature gate flipped.
- A particular dataset or input.

## Expected test method

How would we test? Be specific:

- Comparison vs which baseline.
- What metric distinguishes confirm from reject.
- How many runs / trials.
- Pass/fail threshold.

## Resolution

(Filled in when status moves to `confirmed`, `rejected`, or `superseded`.)

- **Resolved by:** [`../experiments/...`](../experiments/) (or [`../dead-ends/...`](../dead-ends/) if the answer is "this approach fails")
- **Outcome:** one or two sentences.
- **Follow-on hypothesis:** if the answer raised a new question, link it.

## Cross-links

- Concept: [`../concepts/...`](../concepts/)
- Source code involved: [`../../crates/...`](../../crates/)
