# ADBC and Arrow viability for Mosaic Phase 5

## Bottom line

This report evaluates whether Apache Arrow and ADBC are the right technical direction for Mosaic’s Phase 5 ingestion work. The short answer is: **Arrow as the internal batch format is a good direction; ADBC as the primary cross-database ingestion foundation in Rust is not yet strong enough to be the only foundation, especially if Mosaic intends to stay on Rust 1.78.** ADBC is real, useful, and improving quickly, but the Rust surface is still pre-1.0, current Rust releases require a newer compiler than 1.78, and the driver story is uneven across your likely targets. fileciteturn0file0 citeturn28search2turn5view1turn18search1turn15search1

If you want the most conservative move before committing Phase 5, the right bet is **not** “ADBC everywhere.” The right bet is **an Arrow-first ingestion boundary with source-specific adapters**, where ADBC is an optional adapter for the sources where it is genuinely mature enough today, namely SQLite and PostgreSQL, and possibly Snowflake if you accept the Go-driver and deployment complexity. BigQuery and the Rust-native DataFusion ADBC path are not ready enough to anchor the phase. citeturn29search0turn6view5turn30view0turn8view0turn16search0

## What ADBC is and how it compares to JDBC and ODBC

ADBC is an Apache Arrow project that defines a **database connectivity API standard** and ships libraries and drivers around that standard. Conceptually, it occupies the same role as JDBC or ODBC: one application-facing API, many database-specific drivers, and a driver manager layer. The important difference is that ADBC is deliberately focused on **bulk, columnar, Arrow-native retrieval and ingestion**, rather than trying to replace JDBC or ODBC for every workload. The project itself describes ADBC as **complementary** to JDBC and ODBC, not a wholesale replacement. citeturn28search2turn10search1

That distinction matters for Mosaic. If the goal is to get rows out of heterogeneous systems and move them into a cube import pipeline with low copy overhead, ADBC is pointed at the right problem. If the goal is maximal database coverage, maximum ecosystem maturity, straightforward deployment, or stable Rust APIs on an older compiler, ADBC is much weaker than mature source-specific clients and weaker than the older ODBC/JDBC universe. citeturn28search2turn5view1turn35search0turn35search3turn34search0

Arrow Flight SQL sits next to ADBC, not inside it. The ADBC repository explicitly distinguishes them: **Flight SQL is a wire protocol and transport**, while **ADBC is a client API specification**. A database must implement Flight SQL explicitly to speak it, whereas ADBC can wrap an existing protocol that is not Arrow-native. In practice, Flight SQL is most relevant when the source system already supports it and you want fully Arrow-native round trips over the network. citeturn10search1turn28search2

## The Rust crates that matter and how mature they look

The core Rust ADBC crates today are `adbc_core`, `adbc_driver_manager`, and `adbc_ffi`. On top of those are at least two driver-oriented Rust crates: `adbc_datafusion` and `adbc_snowflake`. On the Arrow side, the important Rust crates for Mosaic are `arrow`, `arrow-array`, `arrow-schema`, `arrow-select`, `arrow-ipc`, and `arrow-flight`. The `adbc_driver_manager` crate is the practical center of gravity for Rust right now because most usable database drivers are still implemented in C/C++ or Go and are consumed from Rust through dynamic or static loading. citeturn5view0turn13view1turn16search7turn11search2turn20search1

There are two maturity signals that should make you cautious. First, the official ADBC repo says the **API standard is stable**, but the **libraries are still under development**. Second, the April 2026 ADBC 23 release notes explicitly call out a **breaking change in the Rust APIs** and describe them as **pre-1.0**; that is exactly the wrong signal if you want a low-risk foundation layer. citeturn10search1turn5view1

The Rust documentation quality is also mixed. `adbc_core`’s docs.rs page shows only **57.85% documentation coverage** and **zero documented examples**, while the official Rust quickstart is centered on the experimental DataFusion driver rather than on the external databases you are more likely to use in Mosaic. That does not make the project unusable, but it does increase adoption cost and raises the odds that your team will end up reading source or diffing examples across languages. citeturn13view1turn32search0

One more non-obvious issue: the `adbc_driver_manager` documentation says its managed objects can be shared across threads, but **their operations are serialized under the hood**. That means “thread-safe” does not necessarily mean “highly parallel per-handle.” If Mosaic’s importer expects one connection or one statement object to fan out hard across threads, the behavior will not match the most optimistic interpretation. citeturn11search0

## Driver-by-driver viability for the databases you asked about

### SQLite

SQLite is the best ADBC fit in your target list. In the current ADBC driver status matrix, the SQLite driver is listed as **stable**, with support for SQL, prepared statements, select queries, update queries, transactions, bulk ingestion, and database metadata. It is implemented in C and can be used from Rust via the driver manager. citeturn29search0turn6view5turn6view3

For Mosaic, that makes SQLite the one database where ADBC in Rust is easiest to justify on technical grounds. The failure modes are mostly operational rather than conceptual: dynamic library distribution, versioning, and the fact that in Rust you are usually going through the driver manager rather than a native Rust SQLite ADBC driver. Compared with `rusqlite`, however, ADBC still loses on ecosystem maturity, build simplicity, and plain-Rust ergonomics. `rusqlite` remains the conservative SQLite choice today. citeturn11search0turn34search0

### PostgreSQL

PostgreSQL is the second credible option. The current ADBC matrix lists the PostgreSQL driver as **stable**, and it supports select queries, updates, transactions, bulk ingestion, and metadata. The official driver docs also explain why it can be fast: it tries to read results through PostgreSQL `COPY` where possible. citeturn29search0turn6view5turn7search0

The problem is that PostgreSQL is also where several important limitations show up. The official status page says the driver does **not** have full type support. The PostgreSQL driver docs say unknown PostgreSQL types fall back to Arrow `binary` plus opaque-type metadata, and the status page notes that prepared statements with parameters that **return result sets** are not really supported because the driver is built around `COPY` for speed. The cookbook also shows a concrete gotcha: `SHOW` queries fail unless you explicitly disable the `COPY` optimization. Those are not edge trivia; they are exactly the kind of surprises that leak into an ingestion framework. citeturn6view5turn7search0turn7search1turn7search5

My judgment is that PostgreSQL via ADBC is **usable**, but only if Mosaic treats it as a fast-path adapter for straightforward extraction, not as a completely transparent abstraction over all PostgreSQL behavior. If you need very broad PostgreSQL feature coverage or want fewer surprises, `tokio-postgres`, `postgres`, or `sqlx` are safer foundations. citeturn35search3turn35search4turn35search0

### Snowflake

Snowflake is where the answer gets more conditional. The ADBC matrix lists Snowflake as **stable**, and the Snowflake docs make clear that a Rust wrapper crate exists. The driver supports bulk ingestion and strong Arrow-oriented flows. But under the hood, the Rust wrapper is still a wrapper around the **Go** Snowflake driver, and the crate docs say the default “bundled” mode **builds the driver from source and links it statically**, which requires a **Go compiler at build time**. docs.rs also currently fails to build the Snowflake crate documentation, which is another signal that this path is more operationally fragile than the SQLite or PostgreSQL paths. citeturn29search0turn30view0turn31search3

The Snowflake driver also has some real edge cases. The docs say bulk ingestion works by writing Arrow data to Parquet, uploading it to a temporary internal stage, and then issuing `COPY` commands; that requires `CREATE STAGE` privilege and a current database and schema to be set. There is also an open issue stating that binding `DECIMAL` parameters in the Snowflake driver is not implemented, which is especially concerning because Snowflake workloads often lean heavily on decimal numerics. citeturn30view0turn28search5

If Mosaic needs Snowflake read support only, ADBC is plausible. If it needs write support, broad type fidelity, or dead-simple deployment, I would not make Snowflake ADBC part of the first committed architecture. I would isolate it behind a separate adapter and keep it out of the core ingestion contract until it survives real acceptance tests. citeturn30view0turn31search3turn28search5

### DuckDB

DuckDB is tricky because the signals are mixed. The current ADBC matrix lists DuckDB as **stable**, but the ADBC DuckDB documentation still says support is “still in progress,” points to a long-running tracking issue, and emphasizes that DuckDB’s ADBC support is developed separately from the Arrow project. In other words, “stable” here should not be read as “boring and fully settled in every language binding.” citeturn29search0turn3search5

For Rust specifically, DuckDB is usable only through the driver-manager/shared-library path, not through a first-class native Rust ADBC crate. That makes it workable, but not a compelling reason to standardize Mosaic on ADBC. If you want DuckDB in the architecture, the stronger argument is actually DuckDB’s broader interoperability: it can read PostgreSQL and SQLite directly via extensions and can query Arrow tables and `RecordBatchReader`s directly. That makes DuckDB a good **adapter or staging engine**, but not evidence that ADBC in Rust is your cleanest source abstraction. citeturn11search0turn38search0turn38search1turn38search3

### BigQuery

BigQuery is **not mature enough in Rust** for this role. The official ADBC docs say there are two official BigQuery drivers in development: a **Beta C# driver** and an **Experimental Go driver**. The same docs say Rust can reach the Go driver only via the driver manager. That means you have no native Rust BigQuery ADBC story, only a shared-library bridge into an experimental driver family. citeturn8view0turn8view1turn29search0

That is already enough to keep BigQuery out of a conservative Phase 5A. On top of that, there is at least one documented issue in the C# BigQuery driver around chunk-reading/retries. I would not anchor the Mosaic ingestion abstraction on a BigQuery ADBC path until the Go driver is no longer marked experimental and the Rust-path operational story is cleaner. citeturn28search1turn8view2

### Flight SQL and the Rust-native DataFusion path

Flight SQL is useful to understand, but it is not the answer to your immediate multi-database question. In current ADBC docs, the **Go Flight SQL driver** is stable, while the C# and Java implementations are behind it. There is no equivalent first-class Rust ADBC Flight SQL package in the current status table. On the Rust side, the Arrow ecosystem does have an `arrow-flight` crate, and at least version 54.0.0 had an MSRV of 1.71.1, so Flight SQL is **possible** in Rust, but it is a separate integration choice and only pays off when the data source already speaks Flight SQL. citeturn29search0turn20search1turn10search1

The native Rust ADBC DataFusion driver is explicitly **experimental**, and its Cargo metadata shows `rust-version = "1.88.0"`. That alone disqualifies it for a Rust 1.78 codebase. It is useful as a proof that the Rust ADBC abstractions can work natively, but it is not the right justification for choosing ADBC as your ingestion foundation today. citeturn29search0turn16search0turn32search0

## The biggest risks, failure modes, and places this can go wrong

The first risk is **compiler drift**. Current ADBC Rust workspace metadata shows `rust-version = "1.81"` for the main Rust workspace, which is already above Rust 1.78. The current top-level `arrow` crate is even further ahead at `rust-version = "1.85"` and uses edition 2024. If Mosaic stays on Rust 1.78, the answer is simple: **current ADBC Rust is not compatible as-is**. citeturn18search1turn26search3turn15search1

There is a nuance here: the ADBC Rust crates depend on `arrow-array` and `arrow-schema` in a range that still includes older Arrow releases, and `arrow-array 54.3.0` had `rust-version = "1.70"`. So it is theoretically possible to pin older Arrow components in a narrow way. But that does **not** change the current ADBC workspace MSRV, and it pushes you into a version-pinning strategy where your importer core becomes hostage to dependency archaeology. That is not a good basis for Phase 5 unless you explicitly choose a split-workspace or sidecar approach. citeturn13view1turn22search5turn18search1

The second risk is **API churn**. The ADBC API standard is stable, but the Rust library layer is not. The 0.23 release changed the Rust `RecordBatchReader` return type in a breaking way, and an open pull request exists to add async traits to the core Rust abstractions. That implies the current synchronous trait surface is still evolving and that the maintainers themselves do not consider it done. citeturn5view1turn10search3

The third risk is **packaging and deployment complexity**. In Rust, the realistic production path for most drivers is the driver manager loading a dynamic library or a statically linked C-compatible implementation. The driver-manifest and connection-profile work improves this, but it does not remove the burden of shipping the right shared objects, entrypoints, and profiles. This is the kind of complexity that looks acceptable in a prototype and then becomes painful in CI, packaging, and customer environments. citeturn11search0turn4search5turn33search0

The fourth risk is **driver-specific semantic leakage**. PostgreSQL’s `COPY` optimization, Snowflake’s staging-and-`COPY` ingestion model, BigQuery’s experimental status, and DuckDB’s “stable but still in progress” documentation all show the same pattern: ADBC gives you a unifying shell, but not truly uniform database behavior. If Mosaic’s architecture assumes the abstraction is fully even across backends, it will break. citeturn7search0turn7search1turn30view0turn8view0turn3search5

The fifth risk is **OS- and language-implementation-specific bugs**. The current ADBC driver-status docs warn of a known macOS x86_64 problem when using two Go-based drivers in the same process. If you imagine one future Mosaic binary loading Snowflake plus BigQuery, or Snowflake plus Flight SQL Go, that warning becomes very relevant. citeturn29search0turn6view3

## What a minimal Rust ADBC query looks like and how cleanly it returns batches

A minimal Rust ADBC flow is straightforward in shape even if the surrounding packaging is not: load a driver, create a database handle, open a connection, create a statement, set SQL, execute, and receive an Arrow `RecordBatchReader`. The code below is a trimmed Rust example for SQLite using the driver manager, following the official crate example shape.

```rust
use adbc_core::options::{AdbcVersion, OptionDatabase};
use adbc_core::{Connection, Database, Driver, Statement};
use adbc_driver_manager::ManagedDriver;
use arrow_array::RecordBatch;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut driver =
        ManagedDriver::load_dynamic_from_name("adbc_driver_sqlite", None, AdbcVersion::V100)?;

    let db = driver.new_database_with_opts([
        (OptionDatabase::Uri, ":memory:".into()),
    ])?;

    let mut conn = db.new_connection()?;
    let mut stmt = conn.new_statement()?;

    stmt.set_sql_query("select 1 as id, 'ok' as status")?;
    let reader = stmt.execute()?;

    let batches: Result<Vec<RecordBatch>, _> = reader.collect();
    let row_count: usize = batches?.iter().map(|b| b.num_rows()).sum();

    println!("rows={row_count}");
    Ok(())
}
```

That shape is consistent with the official `adbc_driver_manager` example and the Rust quickstart. More importantly, it answers one of your key questions directly: **yes, ADBC in Rust can return Arrow columnar batches cleanly**. The returned value is a `RecordBatchReader`, and the current Rust release notes explicitly discuss this reader type as part of the Rust API. citeturn11search0turn32search0turn5view1

From Mosaic’s perspective, that means the transport side of the problem is attractive. A `RecordBatch` is already a schema plus column arrays, which is exactly the sort of intermediate representation you want before mapping into cube coordinates and measures. The difficult parts are not “can ADBC yield batches?” but rather “can every chosen driver yield the right types consistently?” and “what is the coercion/error policy for decimals, timestamps, binary, nulls, arrays, and opaque fallback types?” That is an inference from the Arrow `RecordBatch` model plus the documented driver type limitations. citeturn11search2turn6view5turn7search0

So the honest answer to “How hard is it to turn ADBC output into a cube import batch?” is: **moderate, and easier than row-wise clients**. If you define Mosaic’s internal import boundary as an Arrow-like column batch, ADBC reduces copying and makes extraction easier. But the real workload is still schema normalization, dimension-key resolution, measure typing, and precise error attribution per batch or per source row. citeturn11search2turn16search7

## Recommended Phase 5A architecture

The recommended Phase 5A architecture is:

**Use Arrow-style columnar batches as the ingestion boundary, but do not make ADBC the mandatory source interface.**

Concretely, that means:

Create a Mosaic internal abstraction such as `ImportBatch` or `ColumnBatch` whose reference implementation is either `RecordBatch` itself or a thin wrapper around it. This should become the stable contract between source readers and cube import. That keeps the good part of the Arrow decision while avoiding early lock-in to the weakest part of the ADBC story. citeturn11search2turn16search7

Implement **source-specific readers first** for the sources you actually need to ship soon. For SQLite, use `rusqlite` unless you specifically need ADBC interoperability. For PostgreSQL, prefer `tokio-postgres`, `postgres`, or `sqlx` if you need stability and broad feature coverage; add an ADBC PostgreSQL adapter only if benchmarks show a meaningful ingestion advantage for your workload. citeturn34search0turn35search3turn35search4turn35search0turn7search0

Add an **experimental ADBC adapter crate** behind a Cargo feature or in a sibling workspace that is allowed to use Rust 1.81+. Start with **SQLite and PostgreSQL only**. Do not put BigQuery into Phase 5A. Do not make Snowflake required for Phase 5A unless you have already accepted the Go-based wrapper build flow and test matrix. citeturn18search1turn8view0turn31search3turn30view0

Treat **Snowflake, BigQuery, and Flight SQL** as Phase 5B or as isolated adapters. If you need them earlier, consider moving them into a sidecar or separate ingestion worker that can run on a newer Rust toolchain and own the foreign shared-library/runtime complexity without contaminating the Mosaic core. This is an architectural inference from the documented MSRV and driver-status spread. citeturn29search0turn18search1turn16search0

Do not tie secrets and environment configuration to ADBC profiles in Phase 5A. The new profile mechanism is useful and can reduce hardcoding, but it is still a driver-manager feature, not a complete secrets-management story. Use it only as a convenience layer if needed, not as the core contract. citeturn33search0

If you want a single sentence architecture recommendation: **Phase 5A should standardize on Arrow `RecordBatch`-style imports, with native Rust adapters for core sources and ADBC as an opt-in adapter, not as the root of the ingestion tree.** citeturn11search2turn16search7turn29search0

## Open questions and final recommendation

There are still a few uncertainties that I would not hide. The first is that ADBC is moving fast enough that some docs are inconsistent across consumers: for example, current official ADBC status pages list more drivers than some downstream ecosystem docs still talk about, and DuckDB’s status messaging is mixed. That is another sign of a maturing ecosystem, not a frozen one. citeturn29search0turn37search0turn3search5

The second is that the real decision should be benchmarked against Mosaic’s actual cube-load shape. If Phase 5 is dominated by simple, wide extracts that map cleanly into measures and dimensions, Arrow batches will help. If it is dominated by aggressive type cleanup, source-specific SQL behavior, auth edge cases, or niche database semantics, ADBC’s abstraction benefit shrinks quickly. That is an inference from the documented driver limitations and the row-to-column tradeoffs in related tools like Polars and ConnectorX. citeturn37search0turn37search1turn36search0

My final recommendation is therefore clear:

**Commit to Arrow-shaped batches in Phase 5A. Do not commit to ADBC as the sole ingestion substrate.**  
Use ADBC experimentally for **SQLite and PostgreSQL only**, behind an adapter boundary and on a newer Rust toolchain if necessary.  
Defer **BigQuery**. Treat **Snowflake** as conditional and isolated.  
If Mosaic must remain on **Rust 1.78**, do **not** make current Rust ADBC a required Phase 5 dependency. citeturn18search1turn15search1turn29search0turn8view0turn30view0