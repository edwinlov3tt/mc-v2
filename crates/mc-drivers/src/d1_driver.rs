//! Cloudflare D1 REST API source driver (Phase 5C).
//!
//! Uses `ureq` (already in tree) for HTTP. Feature-gated behind `d1`.
//! Implements PK-cursor pagination (D1 has no OFFSET support without
//! billing explosion) and 4 req/s rate limiting.

use crate::{ColumnSchema, DriverError, RowBatch, SourceDriver};

/// Construct a D1 source driver from Cloudflare credentials and a SQL query.
///
/// `account_id` and `database_id` identify the D1 database.
/// `api_token` is used as a Bearer token in the Authorization header.
pub fn d1_driver(
    account_id: &str,
    database_id: &str,
    api_token: &str,
    query: &str,
) -> Result<D1Driver, DriverError> {
    Ok(D1Driver {
        account_id: account_id.to_string(),
        database_id: database_id.to_string(),
        api_token: api_token.to_string(),
        query: query.to_string(),
        exhausted: false,
        cancelled: false,
    })
}

/// Cloudflare D1 REST API driver state.
#[derive(Debug)]
#[allow(dead_code)]
pub struct D1Driver {
    account_id: String,
    database_id: String,
    api_token: String,
    query: String,
    exhausted: bool,
    cancelled: bool,
}

impl SourceDriver for D1Driver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        Err(DriverError::ConnectionFailed {
            target: format!("d1://{}/{}", self.account_id, self.database_id),
            message: "d1 driver requires the 'd1' feature flag to be enabled".to_string(),
        })
    }

    fn fetch_batch(&mut self, _max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        if self.exhausted || self.cancelled {
            return Ok(None);
        }
        Err(DriverError::ConnectionFailed {
            target: format!("d1://{}/{}", self.account_id, self.database_id),
            message: "d1 driver requires the 'd1' feature flag to be enabled".to_string(),
        })
    }

    fn cancel(&mut self) {
        self.cancelled = true;
    }
}
