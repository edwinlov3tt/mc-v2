# Phase 3A — For Dummies

> **In one line:** today, building a new cube means writing Rust code. Phase 3A makes it possible to write a *config file* instead. Same kernel, same math — just a much friendlier door to walk through.

[Technical version → handoff](../../handoffs/phase-3a-handoff.md) · [ADR-0004](../../decisions/0004-phase-3a-model-definition-format.md)

---

## The analogy: IKEA assembly instructions

Today, building a cube in MarketingCubes is like making furniture from scratch. You start with raw lumber (the `mc-core` builder API), measure everything yourself, cut to size, drill the holes, glue the joints. The result is a custom piece that does exactly what you wanted — but only a woodworker can build it.

`mc-fixtures::build_acme_cube()` is a 700-line Rust function that does exactly this woodworking for the demo cube. Every dimension, every element, every hierarchy edge, every rule — written out by hand, in code.

That's fine for one demo cube. It's a problem for a real product where:

- A planning analyst wants to model their company's marketing funnel without learning Rust.
- An LLM should be able to generate a cube from a natural-language prompt (Phase 4).
- A web UI should let someone edit the cube in a form (Phase 6).

Phase 3A turns the woodworking shop into an **IKEA assembly line**. Instead of cutting wood, you write a list of parts and connections in a YAML file:

```yaml
dimensions:
  - name: "Time"
    elements:
      - { id: "Jan_2026", name: "January 2026" }
      - { id: "Feb_2026", name: "February 2026" }
      # ...

measures:
  - { name: "Spend", role: "Input", aggregation: "Sum" }
  - { name: "Revenue", role: "Derived", aggregation: "Sum" }

rules:
  - target: "Revenue"
    body: { mul: [{ ref: "Customers" }, { ref: "AOV" }] }
```

A new crate called `mc-model` reads the YAML, checks for mistakes (the IKEA quality inspector), and assembles the cube using the same builder API the Rust path uses. Same furniture comes out either way; one path is for woodworkers, one is for everyone else.

## What Phase 3A will actually do

Five concrete pieces of work:

**(1) Create a new crate called `mc-model`.** It lives at `crates/mc-model/`. Its job: take a YAML file, validate it, and translate it into the same builder calls `build_acme_cube()` makes. The kernel (`mc-core`) doesn't change at all — it's the woodworker who's now optionally working from a parts list instead of measuring lumber by hand.

**(2) Pick YAML and stick to a "safe subset" of it.** YAML is famously full of footguns — `yes` parses as a boolean, unquoted version numbers like `1.78` parse as floats, and so on. Phase 3A's safe subset says: quote everything name-like, no fancy syntax (no anchors, no merge keys, no custom tags). The validator catches anything that violates the subset before the cube gets built.

**(3) Build a three-stage pipeline:**

```
YAML file  ──parse──▶  ParsedModel  ──validate──▶  ValidatedModel  ──compile──▶  Cube
```

Three stages on purpose, not for over-engineering. Each stage gives a different *kind* of error:
- A typo in YAML syntax (missing colon, bad indentation) → "parse error" with line:column
- A typo in a measure name (typed `Spnd` instead of `Spend`) → "validation error" with model context, like *"rule R1 references measure 'Spnd' which is not declared, did you mean 'Spend'?"*
- An impossible kernel state → "compile error" (rare, would normally be a bug)

This matters enormously for Phase 4 (LLM authoring). When the LLM generates malformed YAML, you want to tell it *which kind* of mistake it made so it can fix the right thing.

**(4) Re-express the Acme cube as `acme.yaml`.** All 6 dimensions, all 11 measures, all 5 rules — written out in YAML instead of Rust. This becomes the proof-of-concept that the YAML path actually works. The original `build_acme_cube()` Rust function **stays put** as the canonical reference; the YAML version is the new path that has to match it byte-for-byte.

**(5) Add `--model <path>` to the CLI.** So you can run:

```bash
cargo run --release --bin mc -- demo                                    # uses the Rust path (existing)
cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml   # uses the YAML path (new)
```

The acceptance gate is brutal and simple: those two commands must produce **byte-for-byte identical output**. If `diff` shows any difference, Phase 3A doesn't ship.

## What's in the validator

The YAML file goes through 10 separate checks before the cube is built. Each one catches a specific class of authoring mistake:

| Validator | Catches |
|---|---|
| Duplicate names | Two dimensions both called "Time," two elements with the same id, etc. |
| Missing dimensions | A measure references a dim name that wasn't declared |
| Invalid hierarchy edges | A hierarchy edge points to an element that doesn't exist |
| Hierarchy cycles | January is in Q1, Q1 is in January (impossible loop) |
| Rules referencing unknown measures | `Revenue = Customers * AOV` but no measure called "AOV" exists |
| Derived measures without rules | A measure is marked "derived" but no rule computes it (would silently always return Null) |
| Input measures with rules | A measure is marked "input" but a rule targets it (one of these has to be wrong) |
| Rule cycles | Rule A reads B, rule B reads A — infinite loop |
| Unsupported aggregation | A measure says `agg: median` but the kernel doesn't implement Median |
| Golden test mismatches | The expected value in the YAML doesn't match what the cube actually produces |

The validator returns *all* errors at once, not just the first one — so editing a 500-line YAML and getting a list of 7 problems lets you fix them in one pass instead of seven.

## Why we care / what would happen if we didn't

Three things would go wrong without Phase 3A:

**(1) The kernel would stay locked behind a Rust IDE.** Every new cube — every customer's planning model, every prototype, every internal experiment — would require a Rust developer to write 700-line builder functions. That's not a product; that's a library only its authors can use.

**(2) Phase 4 (LLM authoring) would be impossible.** LLMs are great at generating structured config files like YAML. They are terrible at generating idiomatic Rust that compiles cleanly against a specific builder API. Phase 4's whole premise — *"describe your planning model in plain English; the LLM emits YAML; the validator catches mistakes; the LLM iterates until it's right"* — is built directly on top of Phase 3A's deterministic schema. Without 3A, there's nothing for the LLM to *aim at*.

**(3) Phase 6 (UI editor) would have nothing to render.** A web UI for editing cubes needs a *schema* — a description of "here are the fields, here are the valid values, here's how they nest." Phase 3A's `mc-model::Schema` is exactly that. Without it, the UI would have to be hand-coded to know about every field on every type in `mc-core`, and would break every time the kernel changed.

In short: **Phase 3A is the door.** Phase 4 walks through it from the LLM side. Phase 6 walks through it from the UI side. Phase 5 (loading real data) needs a stable model artifact to load actuals into. Without Phase 3A, all three of those phases are blocked.

## One thing that's easy to get wrong

The biggest temptation is to **skip the three-stage pipeline**: parse the YAML and call `CubeBuilder` directly, in one step, because it would be less code in Phase 3A. Don't.

The reason the three stages exist is that they have *different audiences for their errors*. Parse errors blame the YAML syntax (you can show a line:column to a human or an LLM). Validation errors blame the model's semantics (you can suggest "did you mean 'Spend'?"). Compile errors blame the kernel state (which should basically never happen, and if it does it's a kernel bug). Mixing them produces unactionable errors that look like *"something went wrong somewhere."*

A bigger, sneakier temptation: **add `serde` to `mc-core`** to make the parsing easier. The whole point of `mc-model` being a separate crate is that the kernel stays free of serialization concerns — it's a math engine, not a config-file processor. ADR-0004 calls this out as a hard rule: parser dependencies live *only* in `mc-model`. Putting `serde` in `mc-core` is a Phase 3A scope failure, not an oversight.

The other thing that's easy to misread is **what Phase 3A is NOT**:

- It is **NOT a UI**. There's no web page, no editor, no buttons. Just a YAML file you author in a text editor.
- It is **NOT LLM authoring**. That's Phase 4. Phase 3A only ships the deterministic, hand-authored path.
- It is **NOT a friendly formula language**. You'll write rules like `body: { mul: [{ ref: "Customers" }, { ref: "AOV" }] }`, not `Revenue = Customers * AOV`. The friendly formula syntax is Phase 3C — a follow-on that compiles `Revenue = Customers * AOV` into the same structured tree this phase ships.
- It is **NOT multi-cube**. One YAML file describes one cube. No imports, no cross-cube references. That's a future phase if it's ever needed.
- It is **NOT a data import path**. Loading actuals from a CSV or an API is Phase 5. Phase 3A authors the *structure* of the cube; Phase 5 fills in real numbers.

## What Phase 3A is and isn't

| It is | It isn't |
|---|---|
| A new way to author cubes (YAML files instead of Rust functions) | Any change to the kernel itself |
| A separate crate (`mc-model`) that translates YAML → existing builder calls | A reason to add `serde` or any parser dep to `mc-core` |
| A three-stage pipeline (Parse → Validate → Compile) | A YAML-straight-into-CubeBuilder shortcut |
| A schema LLMs (Phase 4) and UIs (Phase 6) will both consume | An LLM authoring layer or a UI |
| A 10-validator surface that catches authoring mistakes pre-build | A friendly formula language (`Revenue = Customers * AOV`) — that's Phase 3C |
| Single-cube only | Multi-cube models or cross-cube references |
| The Acme cube re-expressed in YAML, byte-for-byte equivalent to the Rust path | A replacement for `build_acme_cube()` — that Rust function stays as the regression-test floor |

## How long will it take?

Probably 4–6 hours for a focused implementer. The work splits roughly into:

- ~1 hr — set up the new crate, pick the YAML library, design the schema types
- ~1 hr — implement the parser (mostly serde-driven, fairly mechanical)
- ~1.5 hr — implement the 10 validators + their negative tests
- ~1 hr — implement the compile step (`ValidatedModel` walks → builder calls)
- ~30 min — write `acme.yaml` from `build_acme_cube()`
- ~30 min — add the `--model` CLI flag, run the byte-for-byte diff, write the completion report

The bench gate isn't a thing for Phase 3A (no kernel change, no perf claim). The acceptance gate is the demo diff being empty. If everything compiles and the diff is clean, ship it.

## What ships at the end of Phase 3A

When this Phase is done, you can:

```bash
# Edit a YAML file in any text editor
vim crates/mc-model/examples/acme.yaml

# Run the demo against either path; output is identical
cargo run --release --bin mc -- demo
cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml

# Both produce the same numbers; the YAML version is the new authoring path
```

That's it. No new features, no new behaviors. Just a much wider door — the same kernel, now reachable from a YAML file you can give to anyone.

---

*Tied to: [phase-2d.md](./phase-2d.md) (the most recent shipped phase, on top of which 3A is built), [ADR-0004](../../decisions/0004-phase-3a-model-definition-format.md) (the strategic contract this phase implements).*
