//! DuckDB driver tests using temp-file databases.
//!
//! Like the SQLite tests, we seed each test with a fresh temp-file
//! DuckDB database (DuckDB's `:memory:` cannot be shared across
//! connections within the same process). Cleanup runs at the end of
//! every test.

use std::path::{Path, PathBuf};

use duckdb::Connection;
use mc_drivers::{duckdb_driver, ColumnData, ColumnDataType, SourceDriver};

fn temp_db(setup_sql: &str, label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mc_drivers_duckdb_{}_{}_{}.db",
        label,
        std::process::id(),
        rand_suffix()
    ));
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_file({
        let mut q = p.clone();
        q.set_extension("db.wal");
        q
    });
    {
        let conn = Connection::open(&p).expect("open temp duckdb");
        conn.execute_batch(setup_sql).expect("setup sql");
    }
    p
}

fn cleanup(p: &Path) {
    let _ = std::fs::remove_file(p);
    let mut q = p.to_path_buf();
    q.set_extension("db.wal");
    let _ = std::fs::remove_file(&q);
}

/// Tiny non-crypto random suffix using thread time entropy. Avoids
/// collisions when tests run in parallel.
fn rand_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
}

#[test]
fn t_duckdb_driver_reads_typed_columns() {
    let p = temp_db(
        "
        CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            spend DOUBLE,
            channel VARCHAR,
            active BOOLEAN
        );
        INSERT INTO orders VALUES
            (1, 100.5, 'search', TRUE),
            (2,  50.0, 'social', FALSE),
            (3, 250.0, 'search', TRUE);
        ",
        "typed",
    );

    let mut d = duckdb_driver(
        &p,
        "SELECT id, spend, channel, active FROM orders ORDER BY id",
    )
    .expect("driver");

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
    assert_eq!(batch.row_count, 3);

    if let ColumnData::I64(v) = &batch.columns[0].data {
        assert_eq!(v, &vec![Some(1), Some(2), Some(3)]);
    } else {
        panic!("id wrong type");
    }
    if let ColumnData::F64(v) = &batch.columns[1].data {
        assert!((v[0].unwrap() - 100.5).abs() < 1e-9);
    } else {
        panic!();
    }
    if let ColumnData::Bool(v) = &batch.columns[3].data {
        assert_eq!(v, &vec![Some(true), Some(false), Some(true)]);
    } else {
        panic!();
    }

    assert!(d.fetch_batch(100).unwrap().is_none());
    cleanup(&p);
}

#[test]
fn t_duckdb_driver_handles_nulls() {
    let p = temp_db(
        "
        CREATE TABLE x (a INTEGER, b DOUBLE, c VARCHAR);
        INSERT INTO x VALUES (1, 1.5, 'one'), (NULL, NULL, NULL), (3, 3.5, 'three');
        ",
        "null",
    );
    let mut d = duckdb_driver(&p, "SELECT a, b, c FROM x ORDER BY a NULLS LAST").expect("d");
    let batch = d.fetch_batch(100).unwrap().expect("rows");
    assert_eq!(batch.row_count, 3);
    if let ColumnData::I64(v) = &batch.columns[0].data {
        assert!(v.iter().any(Option::is_none));
    } else {
        panic!();
    }
    cleanup(&p);
}

#[test]
fn t_duckdb_driver_batches_at_max_rows() {
    let p = temp_db(
        "
        CREATE TABLE t (id INTEGER);
        INSERT INTO t VALUES (1),(2),(3),(4),(5);
        ",
        "batch",
    );
    let mut d = duckdb_driver(&p, "SELECT id FROM t ORDER BY id").expect("d");
    let b1 = d.fetch_batch(2).unwrap().expect("b1");
    assert_eq!(b1.row_count, 2);
    let b2 = d.fetch_batch(2).unwrap().expect("b2");
    assert_eq!(b2.row_count, 2);
    let b3 = d.fetch_batch(2).unwrap().expect("b3");
    assert_eq!(b3.row_count, 1);
    assert!(d.fetch_batch(2).unwrap().is_none());
    cleanup(&p);
}

#[test]
fn t_duckdb_driver_cancel_returns_none() {
    let p = temp_db(
        "CREATE TABLE t(x INTEGER); INSERT INTO t VALUES (1),(2),(3);",
        "cancel",
    );
    let mut d = duckdb_driver(&p, "SELECT x FROM t").expect("d");
    d.cancel();
    assert!(d.fetch_batch(10).unwrap().is_none());
    cleanup(&p);
}

#[test]
fn t_duckdb_driver_missing_file_yields_source_not_found() {
    let p = std::path::PathBuf::from("/no/such/duckdb/file.db");
    let err = duckdb_driver(&p, "SELECT 1").expect_err("missing");
    let s = format!("{}", err);
    assert!(
        s.contains("not found") || s.contains("not readable"),
        "{}",
        s
    );
}

#[test]
fn t_duckdb_driver_decimal_renders_to_f64() {
    let p = temp_db(
        "
        CREATE TABLE money (amt DECIMAL(10,4));
        INSERT INTO money VALUES (123.4567), (1.0001), (-50.5000);
        ",
        "decimal",
    );
    let mut d = duckdb_driver(&p, "SELECT amt FROM money ORDER BY amt").expect("d");
    let schema = d.schema().expect("schema");
    assert_eq!(schema[0].data_type, ColumnDataType::F64);
    let batch = d.fetch_batch(100).unwrap().expect("rows");
    if let ColumnData::F64(v) = &batch.columns[0].data {
        assert!((v[0].unwrap() - (-50.5)).abs() < 1e-9);
        assert!((v[2].unwrap() - 123.4567).abs() < 1e-9);
    } else {
        panic!();
    }
    cleanup(&p);
}
