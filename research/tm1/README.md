# research/tm1/

IBM TM1 reference manuals. **The dominant prior art for the engine.**

MarketingCubes V2 is TM1-inspired (multidimensional, sparse, write-back, derived-from-rules). Most of the brief's terminology — "cube," "dimension," "rule," "consolidation," "scope" — comes from this heritage. When the brief feels under-specified, these manuals are usually the place to look for the canonical TM1 behavior we're approximating.

## Contents

| File | What it covers |
|---|---|
| [`tm1_api.pdf`](./tm1_api.pdf) | TM1 API reference — particularly useful for thinking about the writeback contract and the cell-level read/write surface. |
| [`tm1_dg_dvlpr.pdf`](./tm1_dg_dvlpr.pdf) | TM1 Developer Guide — design patterns, rule semantics, common pitfalls. |
| [`tm1_inst.pdf`](./tm1_inst.pdf) | TM1 install / configuration manual — useful for understanding the deployment context the engine emulates. |
| [`tm1_op.pdf`](./tm1_op.pdf) | TM1 operations manual — runtime behavior and admin-level concepts. |

## Notes

- TM1's rule language is close to but not identical to the engine's `Expr`. Where the brief's rule semantics differ from TM1, the **brief wins** (it is the contract).
- TM1 uses **stargate** indices and feeders. Phase 1 of MarketingCubes V2 deliberately ships without these — the lazy dependency graph and dirty propagation in [`../../crates/mc-core/src/dependency.rs`](../../crates/mc-core/src/dependency.rs) and [`../../crates/mc-core/src/dirty.rs`](../../crates/mc-core/src/dirty.rs) are the analogues. If a future phase considers feeder-style optimization, this is where to start.
- TM1 is process-per-server; MarketingCubes V2 is single-threaded by Phase 1 design (CLAUDE.md §1).
