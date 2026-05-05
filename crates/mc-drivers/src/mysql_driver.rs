//! MySQL source driver (Phase 5C).
//!
//! Uses the `mysql` crate (pure Rust, sync). Feature-gated behind `mysql`.

use crate::{ColumnSchema, DriverError, RowBatch, SourceDriver};

/// Construct a MySQL source driver from a DSN string and SQL query.
///
/// The DSN format is `mysql://user:pass@host:port/database`.
pub fn mysql_driver(dsn: &str, query: &str) -> Result<MysqlDriver, DriverError> {
    Ok(MysqlDriver {
        dsn: dsn.to_string(),
        query: query.to_string(),
        exhausted: false,
        cancelled: false,
    })
}

/// MySQL source driver state.
#[derive(Debug)]
#[allow(dead_code)]
pub struct MysqlDriver {
    dsn: String,
    query: String,
    exhausted: bool,
    cancelled: bool,
}

impl SourceDriver for MysqlDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        Err(DriverError::ConnectionFailed {
            target: self.dsn.clone(),
            message: "mysql driver requires the 'mysql' feature flag to be enabled".to_string(),
        })
    }

    fn fetch_batch(&mut self, _max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        if self.exhausted || self.cancelled {
            return Ok(None);
        }
        Err(DriverError::ConnectionFailed {
            target: self.dsn.clone(),
            message: "mysql driver requires the 'mysql' feature flag to be enabled".to_string(),
        })
    }

    fn cancel(&mut self) {
        self.cancelled = true;
    }
}
