//! Postgres driver tests.
//!
//! These tests are gated behind `#[ignore]` because they require a live
//! Postgres instance with the schema from
//! `tests/fixtures/postgres_setup.sql` loaded. Run with:
//!
//! ```sh
//! createdb mc_drivers_test
//! psql mc_drivers_test < crates/mc-drivers/tests/fixtures/postgres_setup.sql
//! export MC_DRIVERS_TEST_PG_DSN='postgres://localhost/mc_drivers_test'
//! cargo test -p mc-drivers -- --ignored postgres
//! ```
//!
//! If `MC_DRIVERS_TEST_PG_DSN` is unset and the test was run with
//! `--ignored`, the test panics with a clear setup hint. If the env var
//! is set but the connection fails, the underlying `DriverError` is
//! surfaced (typically `ConnectionFailed` → MC5015 once Stream D is in
//! place).

use mc_drivers::{postgres_driver, ColumnData, ColumnDataType, SourceDriver};

const ENV_KEY: &str = "MC_DRIVERS_TEST_PG_DSN";

fn dsn() -> String {
    std::env::var(ENV_KEY).expect(
        "set MC_DRIVERS_TEST_PG_DSN to the test database DSN \
         (see tests/fixtures/postgres_setup.sql)",
    )
}

#[test]
#[ignore = "requires live Postgres; see file header"]
fn t_postgres_driver_reads_typed_columns() {
    let mut d = postgres_driver(
        &dsn(),
        "SELECT id, spend, channel, active FROM mc_drivers_orders ORDER BY id",
    )
    .expect("driver opens against MC_DRIVERS_TEST_PG_DSN");

    let schema = d.schema().expect("schema");
    assert_eq!(schema.len(), 4);
    assert_eq!(schema[0].name, "id");
    assert_eq!(schema[0].data_type, ColumnDataType::I64);
    assert_eq!(schema[1].name, "spend");
    assert_eq!(schema[1].data_type, ColumnDataType::F64);
    assert_eq!(schema[2].name, "channel");
    assert_eq!(schema[2].data_type, ColumnDataType::Str);
    assert_eq!(schema[3].name, "active");
    assert_eq!(schema[3].data_type, ColumnDataType::Bool);

    let batch = d.fetch_batch(100).expect("ok").expect("rows");
    assert_eq!(batch.row_count, 5);

    if let ColumnData::I64(v) = &batch.columns[0].data {
        assert_eq!(v, &vec![Some(1), Some(2), Some(3), Some(4), Some(5)]);
    } else {
        panic!("id wrong type");
    }
    if let ColumnData::F64(v) = &batch.columns[1].data {
        assert!((v[0].unwrap() - 100.50).abs() < 1e-9);
        assert_eq!(v[4], None, "spend NULL on row 5");
    } else {
        panic!("spend wrong type");
    }
    if let ColumnData::Bool(v) = &batch.columns[3].data {
        assert_eq!(v[0], Some(true));
        assert_eq!(v[1], Some(false));
    } else {
        panic!("active wrong type");
    }

    assert!(d.fetch_batch(100).unwrap().is_none(), "exhausted");
}

#[test]
#[ignore = "requires live Postgres; see file header"]
fn t_postgres_driver_batches_at_max_rows() {
    let mut d =
        postgres_driver(&dsn(), "SELECT id FROM mc_drivers_orders ORDER BY id").expect("driver");
    let b1 = d.fetch_batch(2).unwrap().expect("b1");
    assert_eq!(b1.row_count, 2);
    let b2 = d.fetch_batch(2).unwrap().expect("b2");
    assert_eq!(b2.row_count, 2);
    let b3 = d.fetch_batch(2).unwrap().expect("b3");
    assert_eq!(b3.row_count, 1);
    assert!(d.fetch_batch(2).unwrap().is_none());
}

#[test]
#[ignore = "requires live Postgres; see file header"]
fn t_postgres_driver_cancel_returns_none() {
    let mut d = postgres_driver(&dsn(), "SELECT id FROM mc_drivers_orders").expect("d");
    d.cancel();
    assert!(d.fetch_batch(10).unwrap().is_none());
}

#[test]
fn t_postgres_driver_bad_dsn_yields_connection_failed() {
    // Always runs (no #[ignore]). Connect to localhost port 1 — almost
    // never an open Postgres socket, fails fast with ECONNREFUSED rather
    // than hanging on DNS.
    let err = postgres_driver("postgres://127.0.0.1:1/nonexistent", "SELECT 1")
        .expect_err("connection should fail");
    let s = format!("{}", err);
    assert!(
        s.contains("connection failed")
            || s.contains("Connection")
            || s.contains("refused")
            || s.contains("connect"),
        "expected ConnectionFailed-shaped error, got: {}",
        s
    );
}
