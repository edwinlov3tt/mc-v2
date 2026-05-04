//! DuckDB-federated Postgres driver tests.
//!
//! Same gating as `postgres_tests.rs`: the federation tests are
//! `#[ignore]` and require a live Postgres + the
//! `tests/fixtures/postgres_setup.sql` fixture, plus
//! `MC_DRIVERS_TEST_PG_DSN` exported. These additionally require
//! network access for DuckDB's `INSTALL postgres;` extension download
//! on first use.
//!
//! See `postgres_tests.rs` header for setup instructions.

use std::path::PathBuf;

use mc_drivers::{duckdb_postgres_driver, ColumnData, ColumnDataType, SourceDriver};

const ENV_KEY: &str = "MC_DRIVERS_TEST_PG_DSN";

fn dsn() -> String {
    std::env::var(ENV_KEY).expect(
        "set MC_DRIVERS_TEST_PG_DSN to the test database DSN \
         (see postgres_tests.rs and tests/fixtures/postgres_setup.sql)",
    )
}

fn temp_duckdb() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mc_drivers_duckdb_pg_{}_{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
#[ignore = "requires live Postgres + duckdb postgres_scanner extension; see file header"]
fn t_duckdb_postgres_driver_federates_query() {
    let duckdb_path = temp_duckdb();
    let mut d = duckdb_postgres_driver(
        &duckdb_path,
        &dsn(),
        "SELECT id, spend, channel, active FROM pg.public.mc_drivers_orders ORDER BY id",
    )
    .expect("federated driver opens");

    let schema = d.schema().expect("schema");
    assert_eq!(schema.len(), 4);
    // Federation surfaces the same logical types DuckDB sees after
    // postgres_scanner translates Postgres types.
    assert!(matches!(
        schema[0].data_type,
        ColumnDataType::I64 | ColumnDataType::F64
    ));

    let batch = d.fetch_batch(100).expect("ok").expect("rows");
    assert_eq!(batch.row_count, 5);

    // First column (id) should hold the values 1..=5 regardless of whether
    // postgres_scanner exposed it as I64 or F64.
    let ids: Vec<f64> = match &batch.columns[0].data {
        ColumnData::I64(v) => v.iter().filter_map(|x| x.map(|i| i as f64)).collect(),
        ColumnData::F64(v) => v.iter().filter_map(|x| *x).collect(),
        _ => panic!("unexpected id column type"),
    };
    assert_eq!(ids.len(), 5);
    assert!((ids[0] - 1.0).abs() < 1e-9);
    assert!((ids[4] - 5.0).abs() < 1e-9);

    let _ = std::fs::remove_file(&duckdb_path);
}

#[test]
#[ignore = "requires live Postgres + duckdb postgres_scanner extension; see file header"]
fn t_duckdb_postgres_driver_cancel_returns_none() {
    let duckdb_path = temp_duckdb();
    let mut d = duckdb_postgres_driver(
        &duckdb_path,
        &dsn(),
        "SELECT id FROM pg.public.mc_drivers_orders",
    )
    .expect("d");
    d.cancel();
    assert!(d.fetch_batch(10).unwrap().is_none());
    let _ = std::fs::remove_file(&duckdb_path);
}
