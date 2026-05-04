The Claude research report is dramatically more rigorous than GPT's was. It hit the real-world constraints with citations and specific evidence — issue numbers, PR numbers, version pins, rate limits from Cloudflare's actual documentation. **And it reaches a different conclusion: don't bet on ADBC.**

Let me translate what it actually says, then address your "I'm willing to do the work" framing directly, because that's the real question.

## What the report found, in plain English

The report says ADBC is genuinely a bad bet for Phase 5A in May 2026, for three reasons that aren't about effort:

**Reason 1: The Apache Arrow team's release policy is incompatible with your project.** Apache Arrow Rust (the library ADBC depends on) ships a new major version every 3 months and refuses to backport security fixes to older versions. Every new version bumps the minimum Rust version. They're at Rust 1.85 now (you're pinned at 1.78). Even if you bump to 1.85 today, in 3 months they'll bump to 1.87, and you'll be on the same treadmill forever. **This isn't an effort problem — it's a "we can't get off this treadmill once we're on it" problem.**

**Reason 2: Most ADBC drivers in Rust are actually shims around Go libraries.** The Snowflake ADBC driver literally says "this crate is a wrapper around the Go driver" in its own documentation. Same for BigQuery, Postgres-via-ADBC, and Flight SQL. So when you ship Mosaic with ADBC support, you're shipping `.so` files (compiled Go code) that customers have to install correctly on Windows, Mac, and Linux. There's an open GitHub issue (#3149, July 2025) where ADBC just doesn't compile on Windows because of this. **This is a customer experience problem — your users have to install Go libraries to run your tool.**

**Reason 3: The "10-100x faster" speed claim doesn't apply to your workload.** Those benchmarks are for pulling huge result sets _between_ analytical engines (DuckDB → Snowflake, that kind of thing). Your workload is "run a query, get rows back, map each row to cube cells." For _that_ workload, the speed difference between ADBC and a regular Postgres driver is nothing — your bottleneck is the kernel's per-cell write cost, not the database wire protocol. **You wouldn't get the performance benefit even if you paid the costs.**

The report also points out something nobody had flagged before: **Cloudflare D1 is fundamentally HTTP-only from outside Cloudflare's network.** You can't talk to D1 with rusqlite or any SQLite driver. You can only hit its REST API. And the REST API has hard limits — 1000 rows per query, 30 seconds per request, 100 bound parameters max, 4 requests per second sustained. For pulling 100K rows, you're looking at 30-60 seconds of HTTP roundtrips minimum, with retry logic for rate limits. **D1 is real but it's a Phase 5B feature, not a 5A feature, regardless of effort.**

## Now let me address your actual question

You said: _"not concerned with doing more work if it means we support more. If we need to spend 3 weeks supporting one specific kind of integration to get access to 1-2 database services I'll do it gleefully."_

That's exactly the right instinct, and I want to honor it — but redirect it. Here's why ADBC isn't the place to spend that ambition:

**Spending 3 weeks on ADBC doesn't get you 1-2 database services. It gets you SQLite (which you already get for free with rusqlite) and a permanent toolchain treadmill.** The other ADBC drivers (Postgres, Snowflake, BigQuery) are Go-based shims that don't actually solve the database connectivity problem — they shift it from "I need a Postgres driver" to "I need to bundle Go-compiled libraries on every customer's machine." That's not a coverage upgrade; it's a deployment downgrade.

**Spending 3 weeks on the _right_ things gives you genuinely more.** Here's what I mean:

**Option 1: Native Postgres support in Phase 5A.** The report classifies the sync `postgres` Rust crate as 🟡 (yellow) because it pulls tokio in transitively. But your code doesn't have to _use_ tokio — the `postgres` crate handles all that internally and gives you a sync API. If you're willing to accept "tokio is a transitive dependency but appears nowhere in our own code," you can ship native Postgres support in Phase 5A using `postgres` directly, no ADBC, no Go FFI, no MSRV bump. **This is the call I'd push you to make actively.** It costs you a hardline "no tokio anywhere" rule (which becomes "no async in our code, period") in exchange for first-class Postgres support. Most enterprise customers will care about Postgres support; nobody will care that tokio is a transitive dep.

**Option 2: A genuinely fast bulk-write path.** The report's `write_batch` proposal is the kind of work that would actually produce a 10-30× speedup on real workloads (1M cells: 165 seconds → 5-16 seconds). This is the kernel-level engineering you mentioned wanting to do. It's the kind of thing where Claude Code earns its money on hard performance work, and it's the _actual_ moat — nobody else's planning tool can ingest a million cells in under 16 seconds. Compare that to ADBC, which would give you maybe 20% improvement on the wire protocol while leaving the kernel bottleneck untouched.

**Option 3: LLM-assisted recipe authoring as part of Phase 5A, not deferred to 5B.** This was deferred to keep 5A small, but if you have appetite for more work, this is where it pays off. Mosaic + Phase 4 plugin + LLM-assisted recipes = "tell Mosaic 'import last quarter's HubSpot data' and watch it propose a recipe, validate it, and execute it." Nobody else has that. TM1 doesn't. Anaplan doesn't. Pigment doesn't. **This is genuinely the moat — not the database connectivity itself.**

**Option 4: DuckDB extensions properly leveraged.** The report mentions this almost as a fallback, but it's actually a genuine product win. DuckDB's `postgres_scanner`, `mysql_scanner`, `sqlite_scanner` extensions let you write `ATTACH 'host=...' AS pg (TYPE postgres)` and then query Postgres data through DuckDB, with DuckDB doing the wire-protocol work. This isn't a hack — it's how a lot of modern data tools work, and it gives you Postgres + MySQL + SQLite + Parquet + CSV + JSON + S3 + Iceberg + Delta Lake support **for free** because DuckDB already handles all of those. With 3 weeks of polish, you could ship Phase 5A with a credible "Mosaic talks to anything DuckDB talks to" story, which is roughly 15+ data sources.

## The Phase 5 plan that takes maximum advantage of your ambition

Here's what I'd actually do with the appetite you described:

**Phase 5A (4-6 weeks instead of the lean 3): Real ingestion layer.**

- `rusqlite` for direct SQLite (pinned to 0.31.0 for 1.78 compatibility)
- `duckdb-rs` for direct DuckDB + federation extensions (pinned to 1.3.x for 1.78 compatibility)
- `postgres` (sync wrapper) for native Postgres support — accept tokio as transitive
- `ureq` for HTTP/JSON ingestion
- `csv` for CSV files
- `write_batch` API in mc-core (the real performance work)
- Recipe format with the dbt+dlt+Singer hybrid the report proposes
- The `WritebackContext` parameter change in mc-core
- LLM-assisted recipe authoring via Phase 4 plugin (don't defer this — it's the moat)

This is genuinely 4-6 weeks of work if you do it right, and it ships:

- Native SQLite, DuckDB, Postgres, plus federation to MySQL/Postgres/SQLite via DuckDB
- LLM-assisted recipe generation
- 10-30× faster bulk writes than today
- A real recipe format that's not embarrassing in 2 years
- Deferred to 5B without apology: D1, Snowflake, BigQuery, OCR, scheduled refresh, vault

**Phase 5B (3-4 weeks): Cloud and orchestration.**

- D1 via REST with proper rate-limit handling
- Snowflake/BigQuery via Parquet export (customer's warehouse exports to Parquet, Mosaic ingests via DuckDB)
- Cron scheduling
- Better credential handling (still no full vault, but maybe encrypted local file store)

**Phase 5C (later): The treadmill question.**

- Revisit ADBC if arrow-rs ever ships an LTS branch with security backports
- Revisit ADBC if a pure-Rust ADBC Postgres driver appears
- Otherwise, more DuckDB extensions (Iceberg, Delta Lake, etc.)

## The one specific call I'd push you to make right now

**Decide whether `postgres` (sync wrapper, transitive tokio) is acceptable in Phase 5A.**

This is the highest-leverage decision in the entire phase. Two paths:

**Path 1: Strict "no tokio anywhere, even transitive."** Then Phase 5A is SQLite + DuckDB + DuckDB-federated Postgres + HTTP + CSV. Postgres works but only through DuckDB. Customer-facing story: "Mosaic supports Postgres via DuckDB's federation engine."

**Path 2: Pragmatic "no async in our code, but transitive deps allowed."** Then Phase 5A adds native Postgres alongside DuckDB-federated Postgres. Customer-facing story: "Mosaic supports Postgres natively, with DuckDB federation as a fallback for joining across databases."

**My recommendation: Path 2.** The "no tokio transitively" rule was set when you had no real reason to pull it in. Now you have a real reason — first-class Postgres is a real customer need — and the cost is purely cosmetic (tokio appears in `Cargo.lock`, but nowhere in your source code). Your discipline about "no async/await in our code" is the rule that actually matters. The transitive dep rule was a proxy for that, and it's now blocking the substance it was meant to protect.

If you take Path 2, the Phase 5A scope is meaningfully bigger, and your willingness to do the work pays off in real customer-facing capability rather than infrastructure on a treadmill.

## Bottom line

The report is right that ADBC is wrong. But the report is being conservative about what Phase 5A can include. With your stated appetite for work, Phase 5A can be **larger and more ambitious in the right places**:

- Real Postgres support natively (Path 2 decision)
- Real bulk-write performance (kernel `write_batch`)
- Real LLM-assisted authoring (don't defer to 5B)
- DuckDB-federated everything as a free bonus

This is a genuinely better Phase 5A than either the lean version OR the ADBC-first fantasy. It costs ~5-6 weeks instead of ~3, gives you a real moat (LLM-assisted authoring + bulk-write performance), gives you genuine Postgres support that customers will actually use, and doesn't put you on the arrow-rs treadmill.

If you want, I can help you draft the actual decision points for ADR-0010 with this scope in mind — particularly the Path 1 vs Path 2 call on tokio-as-transitive-dep, which is the key strategic decision for the whole phase.