//! CSV source driver.
//!
//! Wraps the `csv` crate. Reads UTF-8 CSV with a header row. Schema is
//! inferred from the first `SCHEMA_INFERENCE_ROWS` records (default 100).
//!
//! ## Schema inference rule
//!
//! For every column the driver scans up to the first 100 data rows
//! (after the header). If every non-empty value parses as `i64` the column
//! becomes `ColumnDataType::I64`. Otherwise, if every non-empty value parses
//! as `f64`, the column becomes `ColumnDataType::F64`. Otherwise (any value
//! is non-numeric, OR the column is empty across all sampled rows), the
//! column becomes `ColumnDataType::Str`. Booleans are NOT inferred from CSV
//! (`"true"` / `"false"` come back as `Str`); CSV is not strongly typed and
//! the cost of a wrong guess is higher than the cost of one extra cast.
//! Empty cells are SQL NULL — `nullable` is `true` whenever the inference
//! sample contained at least one empty cell.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::{
    Column, ColumnData, ColumnDataType, ColumnSchema, DriverError, RowBatch, SourceDriver,
};

/// Number of data rows scanned to infer per-column types. Scanning the
/// entire file would defeat the purpose of streaming; 100 rows balances
/// inference accuracy against cost on multi-million-row files.
const SCHEMA_INFERENCE_ROWS: usize = 100;

/// Construct a CSV driver reading from `path`.
///
/// The file is opened immediately and the schema is inferred eagerly from
/// up to the first 100 data rows; type inference is documented on the
/// module above. `fetch_batch` then re-opens the file and streams from
/// the start so the inferred schema is always consistent with returned
/// data.
pub fn csv_driver(path: &Path) -> Result<CsvDriver, DriverError> {
    CsvDriver::new(path)
}

/// CSV driver. Holds the inferred schema, a streaming `csv::Reader`, and
/// a cancellation flag.
pub struct CsvDriver {
    path: PathBuf,
    schema: Vec<ColumnSchema>,
    reader: csv::Reader<BufReader<File>>,
    cancelled: bool,
    exhausted: bool,
}

impl std::fmt::Debug for CsvDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CsvDriver")
            .field("path", &self.path)
            .field("schema", &self.schema)
            .field("cancelled", &self.cancelled)
            .field("exhausted", &self.exhausted)
            .finish()
    }
}

impl CsvDriver {
    fn new(path: &Path) -> Result<Self, DriverError> {
        let schema = infer_schema(path)?;
        let reader = open_reader(path)?;
        Ok(CsvDriver {
            path: path.to_path_buf(),
            schema,
            reader,
            cancelled: false,
            exhausted: false,
        })
    }
}

impl SourceDriver for CsvDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        Ok(self.schema.clone())
    }

    fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        if self.cancelled || self.exhausted || max_rows == 0 {
            return Ok(None);
        }

        let mut columns: Vec<Vec<Option<String>>> = (0..self.schema.len())
            .map(|_| Vec::with_capacity(max_rows.min(1024)))
            .collect();
        let mut row_count = 0;

        for record in self.reader.records().take(max_rows) {
            let record = record.map_err(|e| DriverError::MalformedSource {
                message: format!("{} (in {})", e, self.path.display()),
            })?;
            for (i, col) in columns.iter_mut().enumerate() {
                let raw = record.get(i).unwrap_or("");
                if raw.is_empty() {
                    col.push(None);
                } else {
                    col.push(Some(raw.to_string()));
                }
            }
            row_count += 1;
        }

        if row_count == 0 {
            self.exhausted = true;
            return Ok(None);
        }
        if row_count < max_rows {
            self.exhausted = true;
        }

        let columns = self
            .schema
            .iter()
            .zip(columns.into_iter())
            .map(|(schema, raw)| coerce_column(schema, raw))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(RowBatch { columns, row_count }))
    }

    fn cancel(&mut self) {
        self.cancelled = true;
    }
}

fn open_reader(path: &Path) -> Result<csv::Reader<BufReader<File>>, DriverError> {
    let file = File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound
            || e.kind() == std::io::ErrorKind::PermissionDenied
        {
            DriverError::SourceFileNotFound {
                path: path.to_path_buf(),
                message: e.to_string(),
            }
        } else {
            DriverError::Io {
                path: path.to_path_buf(),
                message: e.to_string(),
            }
        }
    })?;
    Ok(csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(BufReader::new(file)))
}

fn infer_schema(path: &Path) -> Result<Vec<ColumnSchema>, DriverError> {
    let mut reader = open_reader(path)?;

    let header = reader.headers().map_err(|e| DriverError::MalformedSource {
        message: format!("missing or unreadable header in {}: {}", path.display(), e),
    })?;
    let names: Vec<String> = header.iter().map(|s| s.to_string()).collect();
    let n_cols = names.len();
    if n_cols == 0 {
        return Err(DriverError::MalformedSource {
            message: format!("CSV {} has no columns", path.display()),
        });
    }

    let mut all_int = vec![true; n_cols];
    let mut all_float = vec![true; n_cols];
    let mut any_value = vec![false; n_cols];
    let mut any_null = vec![false; n_cols];

    for record in reader.records().take(SCHEMA_INFERENCE_ROWS) {
        let record = record.map_err(|e| DriverError::MalformedSource {
            message: format!(
                "malformed CSV record in {} during schema inference: {}",
                path.display(),
                e
            ),
        })?;
        for i in 0..n_cols {
            let raw = record.get(i).unwrap_or("");
            if raw.is_empty() {
                any_null[i] = true;
                continue;
            }
            any_value[i] = true;
            if all_int[i] && raw.parse::<i64>().is_err() {
                all_int[i] = false;
            }
            if all_float[i] && raw.parse::<f64>().is_err() {
                all_float[i] = false;
            }
        }
    }

    Ok(names
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let data_type = if !any_value[i] {
                ColumnDataType::Str
            } else if all_int[i] {
                ColumnDataType::I64
            } else if all_float[i] {
                ColumnDataType::F64
            } else {
                ColumnDataType::Str
            };
            ColumnSchema {
                name,
                data_type,
                nullable: any_null[i],
            }
        })
        .collect())
}

fn coerce_column(schema: &ColumnSchema, raw: Vec<Option<String>>) -> Result<Column, DriverError> {
    let data = match schema.data_type {
        ColumnDataType::I64 => {
            let mut out = Vec::with_capacity(raw.len());
            for v in raw {
                match v {
                    None => out.push(None),
                    Some(s) => {
                        let parsed = s.parse::<i64>().map_err(|_| DriverError::TypeMismatch {
                            column: schema.name.clone(),
                            message: format!(
                                "value {:?} did not parse as i64 (column inferred as I64)",
                                s
                            ),
                        })?;
                        out.push(Some(parsed));
                    }
                }
            }
            ColumnData::I64(out)
        }
        ColumnDataType::F64 => {
            let mut out = Vec::with_capacity(raw.len());
            for v in raw {
                match v {
                    None => out.push(None),
                    Some(s) => {
                        let parsed = s.parse::<f64>().map_err(|_| DriverError::TypeMismatch {
                            column: schema.name.clone(),
                            message: format!(
                                "value {:?} did not parse as f64 (column inferred as F64)",
                                s
                            ),
                        })?;
                        out.push(Some(parsed));
                    }
                }
            }
            ColumnData::F64(out)
        }
        ColumnDataType::Bool => {
            let mut out = Vec::with_capacity(raw.len());
            for v in raw {
                match v {
                    None => out.push(None),
                    Some(s) => match s.as_str() {
                        "true" | "True" | "TRUE" | "1" => out.push(Some(true)),
                        "false" | "False" | "FALSE" | "0" => out.push(Some(false)),
                        other => {
                            return Err(DriverError::TypeMismatch {
                                column: schema.name.clone(),
                                message: format!("value {:?} did not parse as bool", other),
                            })
                        }
                    },
                }
            }
            ColumnData::Bool(out)
        }
        ColumnDataType::Str => ColumnData::Str(raw),
    };
    Ok(Column {
        name: schema.name.clone(),
        data,
    })
}
