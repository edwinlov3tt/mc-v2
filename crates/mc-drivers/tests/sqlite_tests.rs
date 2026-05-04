//! SQLite driver tests using in-memory databases.
//!
//! Tests use `:memory:` rather than committed `.db` files. SQLite's in-memory
//! database cannot be shared across connections, so each test must seed
//! its own data through a separate `rusqlite::Connection` call before
//! invoking the driver. We do this by writing the database to a temp
//! file, then handing the file path to the driver.

use std::path::{Path, PathBuf};

use mc_drivers::{sqlite_driver, ColumnData, ColumnDataType, SourceDriver};
use rusqlite::Connection;

fn temp_db(setup_sql: &str, label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mc_drivers_sqlite_{}_{}.db",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_file(&p);
    {
        let conn = Connection::open(&p).expect("open temp sqlite");
        conn.execute_batch(setup_sql).expect("setup sql");
    }
    p
}

fn cleanup(p: &Path) {
    let _ = std::fs::remove_file(p);
}

#[test]
fn t_sqlite_driver_reads_typed_columns() {
    let p = temp_db(
        "
        CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            spend REAL,
            channel TEXT,
            active BOOLEAN
        );
        INSERT INTO orders VALUES
            (1, 100.5, 'search', 1),
            (2,  50.0, 'social', 0),
            (3, 250.0, 'search', 1);
        ",
        "typed",
    );

    let mut d = sqlite_driver(
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
        panic!("id column wrong type");
    }
    if let ColumnData::F64(v) = &batch.columns[1].data {
        assert!((v[0].unwrap() - 100.5).abs() < 1e-9);
    } else {
        panic!("spend column wrong type");
    }
    if let ColumnData::Bool(v) = &batch.columns[3].data {
        assert_eq!(v, &vec![Some(true), Some(false), Some(true)]);
    } else {
        panic!("active column wrong type");
    }

    assert!(d.fetch_batch(100).expect("ok").is_none(), "exhausted");
    cleanup(&p);
}

#[test]
fn t_sqlite_driver_handles_nulls() {
    let p = temp_db(
        "
        CREATE TABLE x (a INTEGER, b REAL, c TEXT);
        INSERT INTO x VALUES (1, 1.5, 'one'), (NULL, NULL, NULL), (3, 3.5, 'three');
        ",
        "null",
    );
    let mut d = sqlite_driver(&p, "SELECT a, b, c FROM x ORDER BY a NULLS LAST").expect("d");
    let batch = d.fetch_batch(100).unwrap().expect("rows");
    assert_eq!(batch.row_count, 3);
    if let ColumnData::I64(v) = &batch.columns[0].data {
        assert!(v.contains(&None));
    } else {
        panic!();
    }
    if let ColumnData::Str(v) = &batch.columns[2].data {
        assert!(v.contains(&None));
    } else {
        panic!();
    }
    cleanup(&p);
}

#[test]
fn t_sqlite_driver_batches_at_max_rows() {
    let p = temp_db(
        "
        CREATE TABLE t (id INTEGER);
        INSERT INTO t VALUES (1),(2),(3),(4),(5);
        ",
        "batch",
    );
    let mut d = sqlite_driver(&p, "SELECT id FROM t ORDER BY id").expect("d");
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
fn t_sqlite_driver_cancel_returns_none() {
    let p = temp_db(
        "CREATE TABLE t(x INTEGER); INSERT INTO t VALUES (1),(2),(3);",
        "cancel",
    );
    let mut d = sqlite_driver(&p, "SELECT x FROM t").expect("d");
    d.cancel();
    assert!(d.fetch_batch(10).unwrap().is_none(), "cancel honored");
    cleanup(&p);
}

#[test]
fn t_sqlite_driver_missing_file_yields_source_not_found() {
    let p = std::path::PathBuf::from("/no/such/sqlite/file.db");
    let err = sqlite_driver(&p, "SELECT 1").expect_err("missing");
    let s = format!("{}", err);
    assert!(s.contains("not found") || s.contains("no such"), "{}", s);
}

#[test]
fn t_sqlite_driver_bad_query_errors_at_construction() {
    let p = temp_db("CREATE TABLE t(x INTEGER);", "badquery");
    let err = sqlite_driver(&p, "SELECT zzz FROM nonexistent").expect_err("bad query");
    let s = format!("{}", err);
    assert!(s.contains("query failed") || s.contains("no such"), "{}", s);
    cleanup(&p);
}
