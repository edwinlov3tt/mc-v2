# Phase [N] Handoff — [One-line title]

> **Audience:** the Claude Code instance running in this repo that picks up Phase [N].
> **You inherit a green Phase [N-1].** Read this whole file before touching code.

---

## Where Phase [N-1] ended

- **Last commit:** `<short-sha>` — *<commit subject>*
- **Test status:** N / N passing.
- **Determinism:** 10 / 10 identical (or specify the gate run).
- **Demo / smoke check:** `<command>` exits 0 and matches the expected reference output.
- **Gates green:** build, fmt, clippy.
- **Toolchain:** Rust X.Y pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml). [Bump policy: explicit approval required / okay-to-bump].
- **Outstanding deferral being addressed by this phase:** brief acceptance criterion N (description).

For the full Phase [N-1] audit, read [`../reports/phase-[N-1]-completion-report.md`](../reports/phase-[N-1]-completion-report.md). The non-negotiable operating manual is [`../../CLAUDE.md`](../../CLAUDE.md). Read its sections [list] before writing any code.

---

## Phase [N] prompt (verbatim — this is your contract)

> [Paste the user's prompt here verbatim. Don't paraphrase. The prompt is the contract.]

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need but the user-facing prompt didn't include. Pull them from the previous phase's completion report, the source code, the spec, and your own running notes. Examples:

### A. [Topic, e.g., toolchain blocker rationale]

What's true today and why. Concrete suggestions for the first thing to try. What "stop and report options" looks like.

### B. [Topic, e.g., fixture surface area you will use]

The public API the next instance will call against. Code excerpts. Golden values to use as sanity checks.

### C. [Topic, e.g., caching layers / dirty propagation / lazy graph]

Behaviors that affect benchmarking or test design and that aren't obvious from the spec.

### D. [Topic, e.g., known hot spots from previous phase]

Comments-tagged Phase 2 follow-ups, ceiling violations, etc. Don't fix them; document them as findings.

---

## Pointers to existing files you will most likely touch

| Why you might touch it | File | Phase [N] action |
|---|---|---|
| … | [`...`](...) | … |

Files you should **NOT** touch unless [explicit condition the prompt allows]:

- Anything in `crates/mc-core/src/` — production behavior is locked.
- `crates/mc-core/tests/*.rs` — tests are contracts; don't loosen.
- [List others.]
- The locked input contracts: [`../engine-semantics.md`](../specs/engine-semantics.md), [`../phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md).

---

## Reproducible commands you can rely on

These all exit 0 today on the inherited HEAD. They are the ground state your work must preserve.

```bash
cd /path/to/marketingcubes-v2
source $HOME/.cargo/env

cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                # N / 0
cargo run --release --bin mc -- demo
```

---

## Final checklist before you call Phase [N] done

- [ ] Every item in the prompt's scope list is implemented.
- [ ] All sanity assertions ran green before any timing or measurement was recorded.
- [ ] All N existing tests still pass.
- [ ] `cargo fmt --check --all` clean.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] CLI demo / smoke check still passes.
- [ ] The completion report is written at [`../reports/phase-[N]-completion-report.md`](../reports/phase-[N]-completion-report.md) and answers all sections of [`./phase-completion-report.md`](./phase-completion-report.md).
- [ ] The completion report posted in chat in the format the prompt specifies.
- [ ] No out-of-scope additions (re-read the hard rules list in the prompt above).
- [ ] **You did not start Phase [N+1] features.**

If you are uncertain at any point, the resolution order is:

1. The Phase [N] prompt above.
2. [`../reports/phase-[N-1]-completion-report.md`](../reports/phase-[N-1]-completion-report.md).
3. [`../engine-semantics.md`](../specs/engine-semantics.md) and [`../phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md).
4. [`../../CLAUDE.md`](../../CLAUDE.md).
5. Anything else.

If those still don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.
