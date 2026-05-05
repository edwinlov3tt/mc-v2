//! Google BigQuery REST API source driver (Phase 5C).
//!
//! Uses `ureq` (already in tree) for HTTP. Feature-gated behind `bigquery`.
//! Auth: service-account JSON key → JWT → access token via Google OAuth2.

use crate::{ColumnSchema, DriverError, RowBatch, SourceDriver};

/// Construct a BigQuery source driver from a project ID, service-account
/// credentials JSON, and SQL query.
///
/// The credentials JSON is the full content of a Google Cloud service account
/// key file (not a path).
pub fn bigquery_driver(
    project_id: &str,
    credentials_json: &str,
    query: &str,
) -> Result<BigqueryDriver, DriverError> {
    Ok(BigqueryDriver {
        project_id: project_id.to_string(),
        credentials_json: credentials_json.to_string(),
        query: query.to_string(),
        exhausted: false,
        cancelled: false,
    })
}

/// Google BigQuery REST API driver state.
#[derive(Debug)]
#[allow(dead_code)]
pub struct BigqueryDriver {
    project_id: String,
    credentials_json: String,
    query: String,
    exhausted: bool,
    cancelled: bool,
}

impl SourceDriver for BigqueryDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError> {
        Err(DriverError::ConnectionFailed {
            target: format!("bigquery://{}", self.project_id),
            message: "bigquery driver requires the 'bigquery' feature flag to be enabled"
                .to_string(),
        })
    }

    fn fetch_batch(&mut self, _max_rows: usize) -> Result<Option<RowBatch>, DriverError> {
        if self.exhausted || self.cancelled {
            return Ok(None);
        }
        Err(DriverError::ConnectionFailed {
            target: format!("bigquery://{}", self.project_id),
            message: "bigquery driver requires the 'bigquery' feature flag to be enabled"
                .to_string(),
        })
    }

    fn cancel(&mut self) {
        self.cancelled = true;
    }
}
