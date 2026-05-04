## On performance — yes, this is where you spend your edge

Your instinct to optimize hard for write speed is correct, and I want to give you a longer view of what that looks like, because "faster bulk writes" is the visible tip of a much bigger performance story.

The 10-30× speedup the report proposed for `write_batch` is real but conservative. With genuine effort, you can probably push to 50-100× or more. Here's what's actually available in priority order:

**Tier 1: Architectural amortization (the "free" wins).** Single revision bump per batch, single dirty-set update, deferred listener firing, batch-level validation. This is what the report proposed. It's mechanical: identify everything that's per-cell-overhead and move it to per-batch. For 1M cells, this alone gets you from ~165 seconds to maybe ~10-20 seconds.

**Tier 2: Memory layout and cache discipline.** Your kernel currently uses `IndexMap` and `HashMap` with `hashbrown`. That's fine for random access, but bulk loads can do dramatically better with sorted-by-coordinate insertion paths that exploit cache locality. If incoming rows are sorted by their cube coordinates before insertion, you can walk the storage structure once instead of jumping around. SoA (Struct of Arrays) layouts let SIMD do work on coordinate validation. For numeric measure values specifically, columnar storage paths can use SIMD for type validation and NaN checking. **This is a 2-5× win on top of Tier 1.**

**Tier 3: Parallelism with extreme discipline.** Your project has religiously avoided rayon and threads, and that's been the right call for the kernel proper. But bulk ingestion is exactly the kind of embarrassingly parallel workload where it pays off. The trick is bounded parallelism: parse rows in parallel (rayon::par_iter), validate in parallel, but funnel the actual cube writes through a single committer thread to preserve the kernel's single-writer invariant. This is a 4-8× win on a typical 8-core laptop, more on workstations. **But it's a real architectural decision because it introduces rayon to the workspace.** Worth doing, but worth doing as a deliberate ADR — the same way you'd ADR a toolchain bump.

**Tier 4: Memory-mapped storage and write-ahead logging.** This is later territory (probably Phase 7+) where bulk writes don't even hit memory in the traditional sense — they go to a write-ahead log first, and the in-memory representation is updated lazily. This is how DuckDB and ClickHouse get their headline numbers. It changes your durability story too (you can crash mid-load and recover). Big architectural shift. Not Phase 5A.

**Tier 5: SIMD-accelerated aggregations.** Once you have the columnar paths from Tier 2, applying SIMD to the consolidation/rollup calculations gives you another order of magnitude on read performance. This is where you stop being "faster than TM1" and start being "faster than DuckDB at OLAP queries." Real research-grade performance work. Years out, but worth knowing it's on the path.

For Phase 5A, the realistic ambition is **Tier 1 + Tier 2 + a deliberate ADR on whether to add rayon for Tier 3**. That gets you to 50-100× current speed, which is enough to ingest 1M cells in 1-3 seconds. That's the kind of number that makes prospective customers' eyes widen during demos.

The thing I want you to know about chasing performance, though: **benchmarking discipline matters more than the optimizations themselves.** PERF.md has been load-bearing for your project precisely because every claim in it is reproducible. Phase 5A's ingestion benchmarks need to be at least as rigorous — fixed datasets, fixed hardware spec, before/after comparisons in the same PR, multiple measurement runs to detect variance. Without that discipline, performance work becomes folklore. With it, you can credibly claim "we're 75× faster than TM1's bulk load" and have the numbers to prove it.

If you want, I can help you draft a Phase 5A performance brief that lays out the benchmark suite, target numbers, and the order of optimizations. That kind of document is what makes the difference between "we tried to make it fast" and "we have a measured 47× speedup over our Phase 4 baseline at 1M cells."

## On Go FFI alternatives — a longer answer

Now the harder question. You asked whether you can replace Go FFI with Python, C++, or rolling your own. The technically correct answer is "yes, all of these are possible." The strategically correct answer is more nuanced. Let me work through it.

### Why Go is even in the picture (the actual reason)

The ADBC partner drivers (Snowflake, BigQuery, Flight SQL, Postgres-via-ADBC) wrap Go libraries because **the original implementers of those database connectors at Apache Arrow happened to write them in Go.** It's not that Go is technically superior for this — it's that Go was the language of the team that built them, and Apache adopted what existed. The wrappers in Rust/Python/C# all bridge to the same Go binaries because re-implementing each driver in each language would be a massive duplication of effort.

This is actually useful information because it tells you the alternatives.

### Could you use Python instead?

Mechanically, yes. Python has excellent database driver coverage — psycopg2/psycopg3 for Postgres, snowflake-connector-python (also written in C with Python bindings, no Go), google-cloud-bigquery, mysqlclient, pyodbc for SQL Server. Many of these are mature, pure-Python or Python-with-C-extensions, and don't have the Go FFI problem.

You could ship Mosaic with a small Python sidecar process that handles "exotic" database connections — Mosaic spawns a Python subprocess, the subprocess runs a query, marshals results as JSON or Arrow IPC over stdin/stdout, the Rust side ingests them. **This is actually how Phase 4B's Python adapters already work** — your Python adapters call Mosaic via subprocess. You'd just be inverting that flow for ingestion.

The cost: customers have to install Python alongside Mosaic. For developer-tier customers this is fine (they have Python anyway). For non-technical buyers (the finance teams that are TM1's actual buyers), "you also need Python 3.10+" is a real adoption tax. Python's GIL makes parallelism awkward. And you take on a new language ecosystem's worth of dependency management, version skew, and debugging complexity. **But you avoid Go FFI and you get genuinely good database coverage.**

### Could you use C++?

Yes, and this is the more interesting answer. The major databases all ship C/C++ client libraries:

- libpq for Postgres (the canonical, battle-tested C library)
- libmariadb / libmysqlclient for MySQL
- snowflake C/C++ connector
- ClickHouse has clickhouse-cpp
- SQLite is C natively
- ODBC drivers for SQL Server, DB2, Oracle (via unixODBC or iODBC)

These are mature, well-supported, and **don't have the cross-platform binary distribution problem that Go has**. C libraries dynamically link against system libs in a way that's well-understood; Go's static-binary model creates the deployment friction the report flagged.

In Rust, you'd consume these via FFI bindings — `bindgen` for header parsing, `cc` crate for build, plus a thin Rust wrapper. There are existing crates that do this for several of them (`pq-sys`, `mysqlclient-sys`, etc.). For Snowflake specifically, Snowflake ships an ODBC driver (C-based) that works on every major platform. **You could write a `mc-snowflake` crate that wraps the Snowflake ODBC driver via odbc-api or direct FFI, and you'd have native Rust Snowflake support without Go anywhere in sight.**

The cost: writing C FFI bindings is real work, and it's the kind of work that's tedious and detail-heavy in ways that occasionally produce subtle memory-safety bugs. But it's well-understood territory — the Rust ecosystem has been doing C FFI for a decade and there are good patterns.

For Mosaic specifically, **C FFI for high-value database drivers is a credible long-term strategy.** It's the kind of thing where "spending 3 weeks per driver" actually pays off durably — once you have a working Snowflake C-FFI wrapper, it doesn't go on a treadmill the way arrow-rs does.

### Could you build your own pure-Rust drivers?

Yes, and here's where your "I'd rather build proprietary" instinct gets really interesting. Database wire protocols are public specifications. The Postgres frontend/backend protocol is documented, the MySQL protocol is documented, even Snowflake's wire protocol (which is HTTPS + JSON-flavored) is documented through their public API.

Pure-Rust drivers exist for some of this:

- `tokio-postgres` is a pure-Rust Postgres protocol implementation (just async-flavored)
- `mysql_async` and `mysql` (sync) are pure-Rust MySQL protocols
- For Snowflake, no major pure-Rust client exists yet — but you could build one
- For BigQuery, it's just HTTPS + JSON, so a pure-Rust client is mostly a `reqwest` or `ureq` exercise

**Building your own drivers is real engineering, but it's not unreasonable engineering for the strategic value.** A pure-Rust, no-FFI, no-Go, no-Python Snowflake driver would be a genuinely differentiated piece of infrastructure. You'd own it, ship it as part of Mosaic, never depend on anyone else's release schedule, and customers would never have to install anything beyond Mosaic itself.

The cost is multi-month per database. Postgres protocol alone is several thousand lines of careful work. Snowflake is harder because some of their wire protocol is undocumented and you'd be reverse-engineering certain parts. SQL Server's TDS protocol is documented but baroque. MySQL is the simplest of the major ones.

### What I'd actually recommend

Given your appetite, here's the strategy I think makes sense as a multi-phase arc:

**Phase 5A: Native sync drivers via crate ecosystem, no Go anywhere.**

- `rusqlite` for SQLite (pure Rust + C SQLite, no Go)
- `duckdb-rs` for DuckDB (Rust + C, no Go)
- `postgres` (sync wrapper) for Postgres — yes, transitive tokio, but pure Rust + C libpq, no Go
- `mysql` (sync) for MySQL — pure Rust, no Go
- `ureq` for HTTP/JSON
- `csv` for CSV

This gets you SQLite + DuckDB + Postgres + MySQL on day one with zero Go FFI and zero deployment surprises. Plus DuckDB federation gives you "DuckDB can also talk to Postgres/MySQL/SQLite/Parquet/etc." as a bonus.

**Phase 5B: ODBC-based drivers for enterprise databases.**

- Snowflake via Snowflake ODBC (C-based, wrapped via `odbc-api`)
- SQL Server via Microsoft ODBC Driver (C-based)
- Other enterprise sources via their ODBC drivers

This is where you spend the "3 weeks per integration" appetite and it pays off because ODBC drivers are mature and stable. The `odbc-api` crate handles the FFI layer well. **You're using existing native database libraries, not Go FFI.**

**Phase 5C+: Pure-Rust proprietary drivers where strategically valuable.**

This is where the proprietary instinct gets serious. For each database, ask: is there strategic value in owning the driver? For most databases, no — you don't need a proprietary Postgres driver because the existing ones work fine. But for _Snowflake specifically_, owning a pure-Rust driver could be genuinely valuable:

- Mosaic ships as a single binary with no external dependencies
- Customers don't need to install Snowflake ODBC driver separately (which is a real installation pain point)
- You can optimize the driver for Mosaic's specific access patterns (bulk reads of analytical queries, not OLTP)
- It becomes part of your moat — competitors can't replicate it without similar engineering investment

Building a pure-Rust Snowflake driver is probably a 2-3 month focused project. That's real but not infeasible, and the result is a proprietary asset that meaningfully differentiates Mosaic.

### On "owning more of the stack"

Your instinct toward proprietary work is correct strategically, but it's worth being precise about what to make proprietary and what not to.

**Worth making proprietary (real strategic value):**

- The kernel itself (you already do this — mc-core is the core IP)
- The bulk-write performance path (tier 1-3 from earlier)
- The LLM-assisted authoring layer (your Phase 4 plugin work)
- The recipe format and validation logic (your unique abstraction)
- A pure-Rust Snowflake driver if you build one (strategic differentiator)
- SIMD-accelerated aggregation paths (eventually)
- The model-cell / regression infrastructure (when you build it)

**Not worth making proprietary (commodity work, just use existing):**

- SQLite driver — `rusqlite` is fine, has 40M+ downloads, would take you years to match
- DuckDB driver — same, the existing one is mature
- HTTP client — `ureq` exists and works
- CSV parser — `csv` crate is canonical
- YAML parser — you already use one, no value in rolling your own
- JSON parser — same
- Postgres driver for OLTP-style workloads — existing ones are solid

The pattern is: **make proprietary the things that are unique to Mosaic's value proposition (the kernel, the LLM layer, the performance path, the recipe abstraction), and use the ecosystem for everything that's commodity infrastructure.** This is what successful technical companies do. You don't see Linear writing their own database driver; you see them writing their own sync engine.

The strategic reason this matters: every proprietary thing you build is something you have to maintain, secure, and evolve forever. There's a real cost to ownership. So you want to own the things where ownership creates durable advantage, and rent (depend on) the things where ownership is just maintenance burden.

### One specific recommendation about distribution and "ship as one binary"

If you really want to lean into proprietary positioning, the move that matters most isn't writing your own database drivers — it's **shipping Mosaic as a single, statically-linked binary with everything bundled**. This is a deployment story that very few enterprise tools achieve, and it's an enormous trust signal.

Concretely: SQLite bundled (rusqlite has a `bundled` feature), DuckDB bundled (duckdb-rs has the same), libpq linked statically when possible, MySQL client bundled. Mosaic becomes a 30-60MB binary that runs on any modern Linux/Mac/Windows machine with zero installation steps. Compare that to TM1, which requires a server install, a Windows-specific Architect client, a license server, and various other components. **"Drop the binary on the machine and run it" is a moat all by itself in enterprise software.**

This is achievable in Phase 5A if you commit to it as a constraint. It rules out anything that requires a system-installed library (which is part of why ADBC's Go FFI is so painful — you can't bundle a Go-compiled `.so` cleanly). It pushes you toward bundled-C-libraries-via-build.rs patterns. It's a discipline that pays off forever because every new dependency you add gets evaluated against "does this still let us ship a single binary?"

## Pulling it together

Your instincts are right and your appetite is well-placed. Here's the synthesis:

1. **Performance is where you spend hard.** Phase 5A's `write_batch` work, then memory layout, then a deliberate ADR on rayon for parallelism. Real benchmark suite. Target 50-100× current per-cell write speed at 1M cells. This is the moat that competitors can't match easily.
    
2. **Database connectivity uses existing native (non-Go) crates first.** rusqlite + duckdb-rs + postgres (sync wrapper, accept transitive tokio) + mysql (sync) covers the vast majority of customer needs without any Go FFI, without ADBC, without deployment headaches.
    
3. **ODBC for enterprise databases in Phase 5B.** Snowflake, SQL Server, Oracle, and other enterprise sources via their native ODBC drivers, wrapped through `odbc-api`. This is where your "3 weeks per integration" appetite pays off durably.
    
4. **Pure-Rust proprietary drivers for strategically valuable cases later.** A pure-Rust Snowflake driver in Phase 5C or 6 is a real moat and a credible 2-3 month project. Skip this for non-strategic databases.
    
5. **Single-binary distribution is the under-recognized moat.** Commit to "Mosaic ships as one statically-linked binary, no system dependencies" as a hard rule. This forces good architectural choices and gives you a deployment story TM1 can't touch.
    
6. **Make proprietary the things that are unique to Mosaic.** Kernel, performance path, LLM authoring, recipe format, model-cells. Don't make proprietary the commodity stuff (file parsers, HTTP clients, basic SQL drivers) — that's just maintenance debt.
    

This shapes Phase 5A as a genuinely ambitious phase: native Postgres + MySQL + SQLite + DuckDB + DuckDB-federation + a 50-100× faster bulk-write path + LLM-assisted authoring + single-binary distribution. That's 6-8 weeks of focused work and it produces something measurably better than TM1 on every dimension that matters.

Want me to help you draft the performance brief for the bulk-write path? That's the document that turns "make it fast" into "here are the specific benchmarks, target numbers, and optimization order with measurement plans for each tier." It's the load-bearing piece that makes the speed claim defensible rather than aspirational.