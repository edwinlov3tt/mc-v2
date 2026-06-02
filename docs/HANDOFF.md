# HANDOFF

> **5-minute orientation for a new session.** Read this first, then [`CURRENT_STATE.md`](./CURRENT_STATE.md), then [`roadmap/MASTER_PHASE_PLAN.md`](./roadmap/MASTER_PHASE_PLAN.md), then the active handoff document linked below.

---

## What this project is

**Mosaic** (renamed from "MarketingCubes V2" on 2026-05-03) — a Rust-based, AI-powered Large Numbers Model (LNM) platform with a TM1-inspired multidimensional kernel at the bottom. Today's deliverable executes the **Acme demo** (6 dimensions, 11 measures, 5 rules, 2,520 input cells) end-to-end via the Rust kernel AND via YAML model authoring (Phase 3A → 3D).

The project is **spec-driven**. Two files in [`specs/`](./specs/) are the contract:

- [`specs/engine-semantics.md`](./specs/engine-semantics.md) — what the kernel *means*.
- [`specs/phase-1-rust-kernel-build-brief.md`](./specs/phase-1-rust-kernel-build-brief.md) — what to *build* in Phase 1.

The **operating manual** ([`../CLAUDE.md`](../CLAUDE.md)) is the rules of engagement. Read its sections 0, 1, 1.1, 2, 3, 5, and 6 before touching code. The brief overrides the semantics doc; CLAUDE.md overrides nothing but tells you how to behave.

The **scope decision** that produced both contracts is recorded in [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md).

---

## What ships today (Phase 1A → Phase 6A.1)

- Rust 1.78 workspace, **7 crates**: `mc-core`, `mc-fixtures`, `mc-model`, `mc-cli`, `mc-recipe`, `mc-drivers`, `mc-tessera`.
- **`mosaic-plugin/`** at workspace root — Claude Code plugin with 6 skills, 4 agents, 7 commands, 12 MCP tools, Python reference adapters (Anthropic + OpenAI).
- **731 / 0 / 5 tests passing** (5 ignored require live external services; documented as acceptable).
- 10 / 10 determinism gate runs identical.
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build --release`, `cargo test --workspace` all green.
- `cargo bench --workspace` baselined across `phase-2a-cold-path-baseline`, `phase-2b-consolidation-fast-path`, `phase-2c-workload-baseline`. Per-tag JSON under [`reports/bench-data/`](./reports/bench-data/).
- `target/release/mc demo` matches brief §4.6 byte-for-byte; YAML path (`mc demo --model ...`) is byte-identical.
- `mc-core` runtime deps unchanged since Phase 1B: `smallvec`, `ahash`, `thiserror`, `once_cell`. New crates added their own (e.g. `serde_yaml` in `mc-model`).
- `mc-core` locked since Phase 2D except for narrow amendments (Phase 6A.1 corrected `FittedModelData` shape for CRIT-1; rule.rs epsilon swap for MIN-6).
- `mc-fixtures` locked byte-for-byte since Phase 1A.
- One sanctioned `unsafe` site (Phase 5C signal-handler in `mc-tessera/src/schedule/daemon.rs`); documented in CLAUDE.md §3.1.

**Mosaic is now usable.** A user can author a YAML model with formulas, validate/lint/test it, ingest real data via Tessera, query the cube via the CLI, and integrate with AI agents via MCP. `cargo install --path crates/mc-cli --locked` puts the `mc` binary on PATH.

Full audits: per-phase completion reports under [`reports/`](./reports/). Performance baseline at [`PERF.md`](./PERF.md). Master plan at [`roadmap/MASTER_PHASE_PLAN.md`](./roadmap/MASTER_PHASE_PLAN.md).

---

## What is queued

**Phase 6B — Web UI / planning grid — not started.** The natural next phase. Phase 6A made the CLI a complete capability layer; 6B renders the same data visually with drill-down, edit, snapshot/rollback, and version comparison. No handoff doc yet.

**Phase 6A.2 — single-compile sweep + transform polish (P1 known debt).** Filed in [`reports/phase-6a-1-completion-report.md`](./reports/phase-6a-1-completion-report.md):
- `mc model sweep` reads the YAML 2N times for N parameter points (P1 perf debt).
- `mc tessera transform` uses curl subprocess for HTTP fetches — should switch to `ureq` (P1).
- Write-log replay not yet wired into `load_model` — `mc model write` is silently ignored by subsequent reads (**P0**, four-source-state-model rule per process-notes Rule 9).

**Phase 3I — Formula-parser unification + string literals.** Filed in [`research-notes/formula-language-expansion.md`](./research-notes/formula-language-expansion.md) §7I.8. Phase 6A's `--where` filter parser is currently a separate hand-rolled implementation. 3I unifies them.

**Phase 6C — Distribution & install pipeline (TBD).** Anchor: `cargo-dist` cross-compile matrix → GitHub Releases + Homebrew tap + `curl | sh` installer + `mosaic update` self-update verb. Six placeholder crate names already reserved on crates.io (`mosaic-cli`, `mosaic-engine`, `mosaic-lnm`, `mosaic-core`, `mosaic-recipe`, `mosaic-tessera`).

**Phase 7 — Productization.** Multi-tenancy, customer-facing app, auth, audit, SLAs. Not started. Phase 6 must complete first.

**Phase 2 housekeeping** — all closed. Q1 ADR-0003 Accepted–Provisional (sunset 2026-11-01); Q2 toolchain bump deferred (no current driver); Q3 baselines under [`reports/bench-data/`](./reports/bench-data/).

---

## Where to look in this folder

| Question | File |
|---|---|
| What's live right now? | [`CURRENT_STATE.md`](./CURRENT_STATE.md) |
| What's the master phase plan (Phase 1 → 7)? | [`roadmap/MASTER_PHASE_PLAN.md`](./roadmap/MASTER_PHASE_PLAN.md) |
| What was the last phase's audit? | [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md), [`reports/phase-2a-completion-report.md`](./reports/phase-2a-completion-report.md), [`reports/phase-2b-completion-report.md`](./reports/phase-2b-completion-report.md), [`reports/phase-2c-completion-report.md`](./reports/phase-2c-completion-report.md) |
| What did the benchmarks show? | [`PERF.md`](./PERF.md) — Phase 1B baseline §6, Phase 2A cold-path §6.7–§6.10, Phase 2B before/after §6.11, Phase 2C 10× / 50× / 100× rows §6.12 + combined-workflow §6.13 + scaling-shape summary §6.14 |
| What was the most recent handoff? | [`handoffs/phase-6a-agent-ready-cli-handoff.md`](./handoffs/phase-6a-agent-ready-cli-handoff.md) (Phase 6A delivered) and [`handoffs/phase-6a-1-fixes-handoff.md`](./handoffs/phase-6a-1-fixes-handoff.md) (Phase 6A.1 delivered). All earlier per-phase handoffs in [`handoffs/`](./handoffs/). |
| What was the most recent code review? | [`reviews/phase-3-5-6-shipped-review.md`](./reviews/phase-3-5-6-shipped-review.md) — independent Sonnet review across phases 3E–G, 3H, 5C, 6A. The 11 actionable findings drove Phase 6A.1. |
| What is the contract? | [`specs/engine-semantics.md`](./specs/engine-semantics.md) and [`specs/phase-1-rust-kernel-build-brief.md`](./specs/phase-1-rust-kernel-build-brief.md) |
| Why was this scope chosen? | [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md) |
| What rules govern how I work? | [`../CLAUDE.md`](../CLAUDE.md) |
| Where are the templates? | [`templates/`](./templates/) |
| Where are the reference PDFs? | [`../research/`](../research/) |

---

## How to think about this project

**It is an engine, not a model.** The kernel is single-threaded, allocates conservatively, returns `Result` everywhere, and has no hidden global state. Performance and correctness are gated by the brief; do not "improve" things unless the brief asks. CLAUDE.md §2 lists the recurring traps; re-read it.

**Drift is the enemy.** Every public type and test name is a contract. Renaming for "clarity" is a contract violation. CLAUDE.md §2.2 spells this out.

**Decisions get ADRs.** When you make a non-trivial choice during implementation, write an ADR in [`decisions/`](./decisions/). Future-you (or the next instance) will need to know the alternatives you considered and why you rejected them. The first ADR ([0001](./decisions/0001-phase-1-scope.md)) is the model.

**Reports describe what shipped.** When a phase ends, write a completion report in [`reports/`](./reports/) that answers the template's nine sections honestly — including deviations and out-of-scope-not-added confirmation.
