# decisions/

Architecture Decision Records (ADRs). **One file per decision.**

Each ADR captures **why** the engine looks the way it does at a moment in time — not just what was chosen, but the alternatives considered, the trade-offs accepted, and what would need to change to revisit the decision.

ADRs are append-only. When a decision is revised, the new ADR supersedes the old one (and the old one's status becomes `Superseded by ADR-NNNN`); the original record is preserved.

## Format

ADRs follow the standard short form (Michael Nygard style):

- **Status** — `Proposed | Accepted | Deprecated | Superseded by ADR-NNNN`
- **Context** — what situation forces a decision
- **Decision** — what we chose, in concrete terms
- **Consequences** — what follows (upsides, accepted trade-offs, reversal cost)
- **Alternatives considered** — what we rejected and why
- **Cross-links** — to specs, source, reports, related ADRs

Use [`../templates/adr.md`](../templates/adr.md) as the starting point.

## Naming

`NNNN-short-slug.md` where `NNNN` is a four-digit sequence number, zero-padded. Number sequentially. Do not renumber when one ADR supersedes another — supersession is captured in the status field.

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](./0001-phase-1-scope.md) | Phase 1 scope: smallest kernel that runs the Acme demo | Accepted |
| [0002](./0002-perf-assertions-in-benchmarks-not-tests.md) | Performance assertions belong in criterion benchmarks, not in `cargo test` | Accepted |
| [0003](./0003-workload-sketch.md) | Workload sketch & perception thresholds | Accepted — Provisional (sunset 2026-11-01) |
| [0004](./0004-phase-3a-model-definition-format.md) | Phase 3A model-definition format & parser scope | Accepted (with acceptance amendments) |
| [0005](./0005-phase-3b-model-qa-linter-diagnostics.md) | Phase 3B — Model QA, Linter, and Diagnostics | Accepted (with acceptance amendments) |
| [0006](./0006-phase-3c-model-test-fixtures.md) | Phase 3C — Model Test Fixtures and Input Sets | Accepted (with acceptance amendments; redefines Phase 3C from formulas to fixtures, swaps formulas to Phase 3D) |
| [0007](./0007-phase-3d-friendly-formula-syntax.md) | Phase 3D — Friendly Formula Syntax | Accepted (with acceptance amendments; first ADR drafted under the "handoff-first parallel flow" — see [`process-notes.md`](../process-notes.md) §1 for when to use which flow) |
| [0008](./0008-phase-4-llm-authoring-and-plugin-ecosystem.md) | Phase 4 — LLM-Assisted Authoring + Mosaic Plugin Ecosystem | Accepted (with 9 acceptance amendments; major restructure dropped the Rust LLM client crate — Phase 4B is Python reference adapters under `mosaic-plugin/examples/adapters/`; plugin = institutional knowledge / actual moat) |
| [0009](./0009-lnm-substrate-as-product-vision.md) | LNM substrate — AI-native planning kernel as the product vision | Accepted (originally drafted as ADR-0003 on macmini2 2026-05-01; renumbered to ADR-0009 on 2026-05-03 to avoid clash with the workload-sketch ADR-0003 that landed first on the primary branch — content unchanged. Strategic framing for Phases 3–7; complementary to and predates [`../strategy/POSITIONING.md`](../strategy/POSITIONING.md).) |
| [0010](./0010-phase-5-tessera-architecture.md) | Phase 5 — Tessera Architecture (data ingestion engine) | Accepted (with 2 amendments) |
| [0011](./0011-phase-3e-conditionals-and-basic-operations.md) | Phase 3E — Conditionals and Basic Operations | Accepted |
| [0012](./0012-phase-3f-time-series-operations.md) | Phase 3F — Time-Series and Period Operations | Accepted |
| [0013](./0013-phase-3g-reference-data-blocks.md) | Phase 3G — Reference-Data Blocks | Accepted |
| [0014](./0014-time-representation.md) | Time Representation in Mosaic | Accepted |
| [0015](./0015-phase-3i-formula-language-completion.md) | Phase 3I — Formula Language Completion | Accepted |
| [0016](./0016-phase-3j-formula-deferred-items.md) | Phase 3J — Formula Authoring Deferred Items | Accepted |
| [0017](./0017-phase-3h-1-fitted-model-output-bound.md) | Phase 3H.1 — Fitted-Model Output Bound | Accepted |
| [0018](./0018-phase-3h-2-fitted-model-adstock-saturation.md) | Phase 3H.2 — Fitted-Model Adstock + Saturation | Accepted (closes formula-engine deferred queue) |
| [0019](./0019-phase-6d-marketing-report-demo-mvp.md) | Phase 6D — Marketing Report Demo MVP | Accepted |
| [0020](./0020-phase-7a-narrative-engine-plan.md) | Phase 7A — Narrative Engine Plan | Accepted |
| [0021](./0021-phase-7a-4-benchmark-aggregation.md) | Phase 7A.4 — Benchmark Aggregation (Privacy-Aware) | Accepted |
| [0022](./0022-phase-7a-5-explanation-chains.md) | Phase 7A.5 — Explanation Chains + Context Events | Accepted |
| [0023](./0023-pptx-cascade-matcher.md) | Phase 6E — PPTX Cascade Matcher | Accepted |
| [0024](./0024-rich-diagnostic-rendering.md) | Phase 7A.6 — Rich Diagnostic Rendering | Proposed |
| [0025](./0025-kernel-discipline-and-deployment-architecture.md) | Kernel Discipline and Deployment Shape Architecture | **Accepted** — cross-cutting constitutional document; applies to all phases |
| [0026](./0026-org-workspace-resource-scope-capability-grants.md) | Organization, Workspace, Resource Scope, and Capability Grants | **Accepted** — implementation vehicle: Phase 4C |
| [0027](./0027-cross-coord-dependency-graph-fix.md) | Cross-Coordinate Dependency Graph Fix | **Proposed** — performance fix; target: before Phase 8 |
| [0028](./0028-phase-5d-tessera-xlsx-driver.md) | Phase 5D — Tessera XLSX Driver and Layout Descriptors | **Proposed** — XLSX + skip_rows/header_row |
| [0029](./0029-phase-8-service-daemon.md) | Phase 8 — Mosaic Service Daemon | **Proposed** — `mc up`, per-cube actor, hot cache, write journal, API key auth |
| [0030](./0030-model-authoring-ergonomics.md) | Phase 3K — Model Authoring Ergonomics | **Accepted** — auto-element population + JSON schema generation (6 Desktop amendments folded in); shipped `94f45e6` |
| [0031](./0031-nbinom-sf-formula-function.md) | Phase 3L — `nbinom_sf()` Negative Binomial Survival Function | **Proposed** — distributional formula primitive for MLB cartridge; hand-rolled (no stats dep); driven by claw-core EXP-028 |
| [0032](./0032-phase-8-2-consumer-api-surface.md) | Phase 8.2 — Consumer API Surface (`/whatif`, `/sweep`, `/reload`) | **Proposed** — three HTTP endpoints carved out of ADR-0029's Phase 8.1; unblocks claw-core's production prediction loop and slider workflow over Cloudflare Tunnel |

## When to write an ADR

- **Anytime the project draws a scope line** that affects what does and does not get built. Phase 1 scope (this ADR file's first entry) is the canonical example.
- **Anytime a non-trivial implementation choice has at least one credible alternative.** If the choice was obvious, it's not an ADR.
- **Anytime the engine behavior locks in a contract** that downstream phases will be built on top of.

## When NOT to write an ADR

- Routine implementation that follows the brief and semantics doc verbatim.
- Bug fixes (those are commit messages).
- Documentation-shape choices (those are README content).

## Relationship to the rest of `docs/`

| File type | Captures | Lives in |
|---|---|---|
| Brief / engine-semantics | The contract the engine must implement | [`../specs/`](../specs/) |
| ADR | A decision about scope, design, or trade-offs | [`./`](./) (here) |
| Phase completion report | What shipped + acceptance criteria | [`../reports/`](../reports/) |
| Phase handoff | What the next phase needs to know | [`../handoffs/`](../handoffs/) |
| Research note | A distilled lesson from research / a benchmark / a spike | [`../research-notes/`](../research-notes/) |

ADRs and reports complement each other: a report says "we shipped X with these gates"; an ADR says "we chose X over Y because of Z."
