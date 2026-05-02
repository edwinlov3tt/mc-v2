# Claude Code: TM1 LOC scale + what MarketingCubes actually needs to ship

**Date:** 2026-05-01
**Source:** Claude Code response, pasted into the project chat by the project owner.
**Context:** asked how big TM1's codebase is and how much of it MarketingCubes V2 needs to recreate. Companion to GPT-5's LNM-substrate framing in [`./2026-05-01-chat-gpt-lnm-vision.md`](./2026-05-01-chat-gpt-lnm-vision.md).
**Significance:** the response sizes the project — Phase 1 ≈ 15-20K LOC, Phase 5 LNM substrate ≈ 50-100K LOC, vs TM1's full suite at 3-5M. Anchors ADR-0003's "smaller is the whole point" framing. This file preserves the primary source verbatim.

---

Not an elementary question at all — it's a question that gets at something real about how big this project actually is, and the answer reframes the scope discussion in a useful way.

## The honest "I don't know" first

I don't have access to TM1's source code (it's IBM-proprietary, has been since IBM acquired Applix in 2007, was Applix proprietary before that since 1984), and even if I did, I couldn't point at a public LOC count because IBM doesn't publish one. So everything below is informed estimation, not a measurement. Take the numbers as orders of magnitude, not figures.

## Rough estimation for TM1

TM1's core server (the C/C++ codebase that handles the cube engine, rules, persistence, replication, security, and the TM1 server protocol) is probably in the **500K to 1.5M lines of code** range, with the most likely number being 700K-900K LOC for just the server. That excludes:

- TM1 Web (the browser UI)
- TM1 Architect / Perspectives (the Windows clients)
- Planning Analytics Workspace (the modern web UI)
- TI (TurboIntegrator) — adds another 100-200K
- The MDX engine
- The REST API layer (added much later)
- Connectors and integrations
- The 40 years of accumulated test suites, migration tools, build infrastructure

If you count the whole IBM Planning Analytics suite end-to-end, you're probably looking at **3-5 million lines of code** across all components. That's a reasonable estimate based on comparable enterprise products: SAP HANA core is reportedly ~3M, Oracle's database core is reportedly larger, Microsoft Analysis Services is in the same range. TM1 is older than all of these and has been continuously developed since 1984, so the upper bound is plausible.

The breakdown I'd guess for the 700-900K core server number:

- Cube engine, dimensions, hierarchies, cells: 80-120K
- Rule engine and feeders: 60-100K
- Storage, persistence, write-ahead log: 80-120K
- Replication and clustering: 50-80K
- Security, locking, transactions: 40-70K
- TI process engine: 80-120K
- Network protocol, RPC: 50-80K
- MDX evaluation: 60-100K
- Admin, configuration, logging: 80-120K
- Glue, error handling, tests: 100-200K

These numbers are calibrated against analogous open-source databases of similar maturity. Could be off by 2x in either direction. The shape is what matters.

## What you actually need

Here's where the answer gets interesting. You do not need anything close to those numbers.

**For Phase 1 as scoped in the brief:** I'd estimate **8,000 to 15,000 LOC of Rust** for `mc-core`, plus 1-3K for fixtures and CLI. Maybe 20K total including tests. This is the deterministic kernel that does cells, dimensions, hierarchies, rules, dirty propagation, trace, snapshots, writeback, and consolidation. It's roughly 5-8% of TM1's core engine LOC and gives you 60-70% of TM1's actual day-to-day value.

**For the LNM-substrate vision through Phase 5 (kernel + persistence + DuckDB integration + model cells + YAML layer + LLM authoring):** I'd estimate **40,000 to 80,000 LOC of Rust**, plus another 10-20K of Python or TypeScript for the model authoring layer. So 50-100K total. This is roughly **3-7% of TM1's full enterprise suite**.

The reason the gap is so large isn't that Rust is magic. The reason is that you're consciously *not* building 90%+ of what TM1 has accumulated. That's a feature, not a limitation.

## What you're not building (and why that's the whole point)

TM1's massive LOC count is a function of forty years of enterprise accommodation. Most of it is not analytic engine work. It's:

**Backwards compatibility.** TM1 supports cube definitions from 1985. It supports four different rule syntaxes accumulated over decades. It maintains binary file formats from before Y2K. You don't need any of this — your file format starts now.

**Enterprise IT integration.** LDAP, Kerberos, SAML, AD, CAM, IBM Cognos integration, IBM Cloud Pak, JDBC drivers, ODBC drivers, OLE DB providers, COM automation, .NET bindings. Massive surface area, all of it required for Fortune 500 sales. You're not selling to that buyer. Skip all of it.

**Multiple client SDKs.** TM1 has C, Java, .NET, Python, REST, and OData clients, each with their own quirks and version-skew handling. You ship one Rust kernel and one well-designed REST API. That's two SDKs of work, not seven.

**Multiple UI surfaces.** TM1 ships Architect (Windows desktop), Perspectives (Excel add-in), Web (browser), Planning Analytics Workspace (modern browser), Cognos Analytics integration, Cognos Office, and Cognos Workspace Advanced. Each one is a multi-hundred-thousand-LOC project. You ship a CLI and eventually a single web UI. That's a 95% reduction in UI surface area.

**Replication, clustering, hot-standby, failover.** TM1 supports active-active replication for HA, log shipping for DR, and various clustering modes. You build single-node first; clustering is years away.

**The TI language.** TurboIntegrator is a full programming language with its own parser, evaluator, debugger, and standard library. Probably 100-200K LOC alone. You don't build TI. You let people write Python or TypeScript that calls the kernel through an API. That's the modern shape.

**MDX.** TM1 supports a substantial subset of Microsoft's MDX query language for compatibility with Excel pivot tables and Cognos. Tens of thousands of LOC for the parser and evaluator. You don't need MDX. You ship a clean query API and let people use SQL via DuckDB integration when they need ad hoc queries.

**Decades of edge case patches.** Every enterprise software product accumulates a long tail of bug fixes for customer-specific edge cases that nobody can remove because somebody depends on them. TM1 has 40 years of this. You have zero.

If you back all of that out, the *actual analytic engine work* in TM1 is probably 150-250K LOC. Your 50-100K total target is therefore in the right neighborhood for what you're actually building, just expressed in a more efficient language with modern abstractions and without four decades of compatibility weight.

## Is Rust meaningfully more efficient than C in LOC terms?

This is a real question and the answer is: somewhat, but not dramatically.

Rust is more concise than C for the same functionality, mostly because:

- The type system replaces a lot of defensive coding (null checks, ownership tracking, manual cleanup) with compiler enforcement
- `Result` and `?` replace verbose error-handling boilerplate
- `derive` replaces hand-written serialization, equality, hashing, debugging
- Pattern matching on enums is much terser than C's tagged unions and switch statements
- Iterator chains replace explicit loops
- Standard library is much larger and higher-level than C's

Realistic Rust-vs-C ratios for equivalent functionality are roughly **0.5x to 0.7x** — Rust takes about half to two-thirds the lines of C for the same job. Some specific subsystems (anything with manual memory management, anything with complex error handling) will be 0.3-0.4x because the C version was mostly boilerplate. Other subsystems (numerical algorithms, data structures) are closer to 0.8-0.9x because the work is the work.

Compared to C++ the ratio is closer to 0.7-0.9x — C++ has more abstractions than C so there's less to save.

So Rust gives you maybe a 30-50% LOC reduction over an equivalent C codebase. That's meaningful but not the dominant factor. The dominant factor is that you're building **a much smaller product**, not that you're building the same product more efficiently.

## What this actually means

A few honest implications.

**Phase 1 is genuinely small.** 15-20K LOC of Rust is a 4-8 week solo project for a competent Rust engineer working full-time, longer with care and review. That's not "millions of lines" territory. It's "well-scoped open-source library" territory. Comparable in size to early DuckDB, early sled, or a single component of a larger system.

**The LNM substrate target through Phase 5 is also tractable.** 50-100K LOC is a 1-3 person team for 12-24 months. That's Cube.dev's rough scale, that's early DuckDB's rough scale. It is *not* TM1's scale and shouldn't be. If you find yourself building toward TM1's LOC count, something has gone wrong.

**The codebase grows fastest from features you don't need.** Every "let me add support for X" decision is a multi-thousand-line commitment when you account for tests, docs, integration paths, and edge case handling. The discipline you've already shown in the brief — punting model cells to Phase 4, deferring DuckDB to Phase 3, refusing to add `serde` until needed — is exactly the discipline that keeps the LOC count tractable.

**Rust isn't the reason you'll succeed.** Rust is the right choice (correctness, performance, single-binary distribution, WASM compatibility, no GC pauses), but the LOC efficiency is a secondary benefit. The reason this project is achievable at small scale is *scope discipline*, not language choice. If you wrote it in Go or Zig or even modern C++ with care, you'd ship something close to the same size. Rust just makes correctness easier to enforce, which matters more than terseness.

**The real lesson for resourcing.** When you eventually pitch this or hire for it, the right framing is "this is a 50-100K LOC project at maturity, not a 1-3M LOC project." That changes everything about how you think about team size, runway, and competitive timing. You don't need to outspend IBM. You need to out-focus them.

## A useful comparison

For sense of scale, some open-source projects with rough LOC counts:

- **Redis** — ~200K LOC of C. Single-author for many years, then small team. World-class infrastructure.
- **SQLite** — ~150K LOC of C (excluding generated code and tests). Tiny team. Most-deployed database in history.
- **DuckDB** — ~500K LOC of C++ now (multi-year team effort), but the first shippable version was much smaller, probably 50-100K.
- **Sled** (Rust) — ~30-50K LOC at peak.
- **TigerBeetle** (Zig, financial transactions) — ~50K LOC for a serious correctness-critical financial database.
- **Cube.dev** core — somewhere in 100-300K range across multiple languages.

You're targeting something between SQLite and DuckDB in scope. Both are tractable, both are well-known projects, both are vastly smaller than TM1. The size you need is achievable. The size of TM1 is *what you're explicitly avoiding by being focused.*

So no, you don't need millions of lines, and yes, Rust helps somewhat, but the bigger story is that you've correctly scoped a problem that doesn't require the millions in the first place.
