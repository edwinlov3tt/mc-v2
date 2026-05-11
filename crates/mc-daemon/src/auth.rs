//! Bearer token authentication middleware.
//!
//! Per ADR-0029 Decision 7:
//! - `--api-key` enables bearer token auth on all endpoints
//! - Health endpoint (`/api/v1/health`) is exempt from auth
//! - Without `--api-key`, all requests are allowed (binding is localhost-only)
//! - With `--api-key`, missing or wrong token → 401 Unauthorized

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Auth middleware layer. Checks `Authorization: Bearer <key>` header.
///
/// Skips auth for `/api/v1/health` (monitoring needs unauthenticated access).
pub async fn auth_layer(request: Request, next: Next) -> Response {
    // Extract expected key from request extensions (set by the server layer)
    let expected_key = request.extensions().get::<ApiKeyConfig>().cloned();

    let path = request.uri().path().to_string();

    // Health endpoint is always auth-exempt
    if path == "/api/v1/health" {
        return next.run(request).await;
    }

    // If no api_key configured, allow all (localhost-only binding enforced at startup)
    let expected = match expected_key {
        Some(ApiKeyConfig(Some(ref key))) => key.clone(),
        _ => return next.run(request).await,
    };

    // Check Bearer token
    match request.headers().get("authorization") {
        Some(value) => {
            let value_str = value.to_str().unwrap_or("");
            if value_str == format!("Bearer {expected}") {
                next.run(request).await
            } else {
                (StatusCode::UNAUTHORIZED, "Invalid API key").into_response()
            }
        }
        None => (StatusCode::UNAUTHORIZED, "Missing API key").into_response(),
    }
}

/// Wrapper for the API key, stored in request extensions.
#[derive(Clone, Debug)]
pub struct ApiKeyConfig(pub Option<String>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_config_none() {
        let config = ApiKeyConfig(None);
        assert!(config.0.is_none());
    }

    #[test]
    fn test_api_key_config_some() {
        let config = ApiKeyConfig(Some("test-key".into()));
        assert_eq!(config.0.as_deref(), Some("test-key"));
    }
}
