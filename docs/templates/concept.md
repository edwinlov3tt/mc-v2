# [Concept name]

**Status:** `active | superseded`
**Created:** YYYY-MM-DD
**Last touched:** YYYY-MM-DD
**Spans phases:** `1A, 1B, 2, …`

---

## Rule (one sentence)

The thing the engine does. Lead with the rule, not the rationale.

## Why this rule

The reason this rule exists. What goes wrong without it. What alternatives we considered.

## Where it shows up

- **Source:** [`../../crates/.../foo.rs`](../../crates/...) — the production enforcement point(s).
- **Tests:** [`../../crates/mc-core/tests/...`](../../crates/mc-core/tests/) — the assertion(s) that lock the contract.
- **Spec:** [`../engine-semantics.md`](../engine-semantics.md) §X / [`../phase-1-rust-kernel-build-brief.md`](../phase-1-rust-kernel-build-brief.md) §Y.

## Edge cases / gotchas

What surprises a reader. What the rule does NOT cover. What is easy to get wrong if you're not paying attention.

## Related concepts

- [`./other-concept.md`](./)

## History

- YYYY-MM-DD — created based on [`../experiments/...`](../experiments/) / [`../reports/...`](../reports/).
- YYYY-MM-DD — refined after [`../experiments/...`](../experiments/) showed [...].
- (If `superseded`) → replaced by [`./new-concept.md`](./new-concept.md).
