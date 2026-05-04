//! CSV driver tests using committed fixture files.

use std::path::PathBuf;

use mc_drivers::{csv_driver, ColumnData, ColumnDataType, SourceDriver};

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures");
    p.push(name);
    p
}

#[test]
fn t_csv_driver_loads_committed_fixture() {
    let mut d = csv_driver(&fixture("sample.csv")).expect("driver opens fixture");

    let schema = d.schema().expect("schema");
    assert_eq!(schema.len(), 4);
    assert_eq!(schema[0].name, "id");
    assert_eq!(schema[0].data_type, ColumnDataType::I64);
    assert_eq!(schema[1].name, "name");
    assert_eq!(schema[1].data_type, ColumnDataType::Str);
    assert_eq!(schema[2].name, "score");
    assert_eq!(schema[2].data_type, ColumnDataType::F64);
    assert_eq!(schema[3].name, "active");
    assert_eq!(schema[3].data_type, ColumnDataType::Str);

    let batch = d
        .fetch_batch(100)
        .expect("fetch_batch ok")
        .expect("non-empty batch");
    assert_eq!(batch.row_count, 5);
    assert_eq!(batch.columns.len(), 4);

    if let ColumnData::I64(v) = &batch.columns[0].data {
        assert_eq!(v, &vec![Some(1), Some(2), Some(3), Some(4), Some(5)]);
    } else {
        panic!("id should be I64");
    }
    if let ColumnData::F64(v) = &batch.columns[2].data {
        assert!((v[0].unwrap() - 98.5).abs() < 1e-9);
        assert!((v[4].unwrap() - 99.99).abs() < 1e-9);
    } else {
        panic!("score should be F64");
    }

    assert!(d.fetch_batch(100).expect("ok").is_none(), "exhausted");
}

#[test]
fn t_csv_driver_handles_nulls() {
    let mut d = csv_driver(&fixture("sample_with_nulls.csv")).expect("driver");

    let schema = d.schema().expect("schema");
    assert!(schema.iter().any(|c| c.name == "name" && c.nullable));
    assert!(schema.iter().any(|c| c.name == "score" && c.nullable));
    assert!(schema.iter().any(|c| c.name == "id" && c.nullable));

    let batch = d.fetch_batch(100).unwrap().expect("rows");
    assert_eq!(batch.row_count, 5);

    let id_col = match &batch.columns[0].data {
        ColumnData::I64(v) => v,
        _ => panic!("id should be I64"),
    };
    assert_eq!(id_col[4], None, "missing id maps to None");

    let name_col = match &batch.columns[1].data {
        ColumnData::Str(v) => v,
        _ => panic!("name should be Str"),
    };
    assert_eq!(name_col[1], None, "empty name maps to None");

    let score_col = match &batch.columns[2].data {
        ColumnData::F64(v) => v,
        _ => panic!("score should be F64"),
    };
    assert_eq!(score_col[2], None, "empty score maps to None");
}

#[test]
fn t_csv_driver_mixed_column_falls_back_to_str() {
    let mut d = csv_driver(&fixture("sample_types.csv")).expect("driver");
    let schema = d.schema().expect("schema");

    assert_eq!(
        schema
            .iter()
            .find(|c| c.name == "int_col")
            .unwrap()
            .data_type,
        ColumnDataType::I64
    );
    assert_eq!(
        schema
            .iter()
            .find(|c| c.name == "float_col")
            .unwrap()
            .data_type,
        ColumnDataType::F64
    );
    assert_eq!(
        schema
            .iter()
            .find(|c| c.name == "str_col")
            .unwrap()
            .data_type,
        ColumnDataType::Str
    );
    assert_eq!(
        schema
            .iter()
            .find(|c| c.name == "mixed_col")
            .unwrap()
            .data_type,
        ColumnDataType::Str,
        "any non-numeric value forces Str inference"
    );

    let batch = d.fetch_batch(100).unwrap().expect("rows");
    assert_eq!(batch.row_count, 4);
}

#[test]
fn t_csv_driver_batches_at_max_rows() {
    let mut d = csv_driver(&fixture("sample.csv")).expect("driver");

    let b1 = d.fetch_batch(2).unwrap().expect("batch 1");
    assert_eq!(b1.row_count, 2);

    let b2 = d.fetch_batch(2).unwrap().expect("batch 2");
    assert_eq!(b2.row_count, 2);

    let b3 = d.fetch_batch(2).unwrap().expect("batch 3");
    assert_eq!(b3.row_count, 1);

    assert!(d.fetch_batch(2).unwrap().is_none(), "exhausted");
}

#[test]
fn t_csv_driver_cancel_returns_none() {
    let mut d = csv_driver(&fixture("sample.csv")).expect("driver");
    d.cancel();
    assert!(d.fetch_batch(100).unwrap().is_none(), "cancelled → None");
    // Cancel is idempotent.
    d.cancel();
    assert!(d.fetch_batch(100).unwrap().is_none());
}

#[test]
fn t_csv_driver_missing_file_yields_source_not_found() {
    let p = PathBuf::from("/this/path/should/not/exist/ever.csv");
    let err = csv_driver(&p).expect_err("missing file errors");
    let s = format!("{}", err);
    assert!(
        s.contains("not found") || s.contains("not be opened") || s.contains("not readable"),
        "expected SourceFileNotFound, got {}",
        s
    );
}

#[test]
fn t_csv_driver_empty_max_rows_returns_none() {
    let mut d = csv_driver(&fixture("sample.csv")).expect("driver");
    assert!(d.fetch_batch(0).unwrap().is_none(), "max_rows=0 → None");
    // Subsequent normal reads still work.
    let b = d
        .fetch_batch(100)
        .unwrap()
        .expect("normal read works after 0-batch");
    assert_eq!(b.row_count, 5);
}
