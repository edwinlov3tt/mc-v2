//! Recipe YAML → typed [`Recipe`] deserialization.
//!
//! [`parse`] is the public entry point. It runs `serde_yaml::from_str`
//! and, on failure, best-effort-classifies the error into one of:
//!
//! - **MC5007** [`RecipeError::MissingField`] — the deserializer reported
//!   a `missing field` for one of the recipe's required fields.
//! - **MC5002** [`RecipeError::UnknownDriver`] — the deserializer reported
//!   an `unknown variant` whose context names the driver enum (best-effort
//!   string match against the legal driver names).
//! - **MC5001** [`RecipeError::Syntax`] — anything else (general YAML
//!   syntax error, type-mismatch, …).
//!
//! After successful deserialization, [`parse`] enforces
//! [`Recipe::version`] `== 1` and emits **MC5012**
//! [`RecipeError::UnsupportedVersion`] otherwise.
//!
//! Serialization is provided via [`to_yaml`], which delegates to
//! `serde_yaml::to_string` with no further processing — the schema's
//! serde-rename rules + `skip_serializing_if = "Option::is_none"`
//! attributes give roundtrip-stable output.

use crate::error::RecipeError;
use crate::schema::Recipe;

/// Parse a Tessera recipe from YAML text. Returns the typed [`Recipe`]
/// on success, or a single [`RecipeError`] on failure.
///
/// On success, [`Recipe::version`] is guaranteed to equal `1` — the
/// version check fires inside this function so callers don't need to
/// duplicate it.
///
/// # Diagnostic codes
///
/// - **MC5001** — general YAML / deserialization failure.
/// - **MC5002** — unknown driver variant (best-effort classification
///   from serde_yaml's message).
/// - **MC5007** — missing required field.
/// - **MC5012** — version is not 1.
pub fn parse(yaml: &str) -> Result<Recipe, RecipeError> {
    let recipe: Recipe = match serde_yaml::from_str::<Recipe>(yaml) {
        Ok(r) => r,
        Err(e) => return Err(classify_serde_error(&e)),
    };

    if recipe.version != 1 {
        return Err(RecipeError::UnsupportedVersion {
            path: "/version".to_string(),
            version: recipe.version,
        });
    }

    Ok(recipe)
}

/// Serialize a [`Recipe`] back to YAML text. Used for the roundtrip
/// stability test; downstream tools that need YAML output may also call
/// it. Round-trip property: `parse(to_yaml(parse(s)?)?)? == parse(s)?`.
pub fn to_yaml(recipe: &Recipe) -> Result<String, RecipeError> {
    serde_yaml::to_string(recipe).map_err(|e| RecipeError::Syntax {
        path: "/".to_string(),
        message: format!("serialize failed: {e}"),
    })
}

/// Classify a `serde_yaml::Error` into the appropriate [`RecipeError`]
/// variant. Best-effort string matching against serde_yaml 0.9.34's
/// error format. Falls back to [`RecipeError::Syntax`] (MC5001) for
/// any error that doesn't match a more specific pattern.
fn classify_serde_error(err: &serde_yaml::Error) -> RecipeError {
    let msg = err.to_string();
    let path = format_serde_path(err);

    // MC5007: missing required field.
    //
    // serde_yaml emits "missing field `name`" or "field `xxx`: missing field
    // `yyy`" depending on whether the failure is on the top-level struct or
    // a nested struct. Either way, the substring "missing field" identifies
    // it uniquely.
    if msg.contains("missing field") {
        return RecipeError::MissingField { path, message: msg };
    }

    // MC5002: unknown driver variant.
    //
    // serde_yaml emits "unknown variant `mysql`, expected one of `csv`,
    // `sqlite`, `duckdb`, `postgres`, `duckdb_postgres`, `http_json`".
    // We disambiguate from other enum violations (OnError, WriteDisposition,
    // OnMissingElement) by looking for both `csv` and `sqlite` in the
    // expected list — the only enum that lists both is DriverKind.
    if msg.contains("unknown variant") && msg.contains("`csv`") && msg.contains("`sqlite`") {
        let driver = extract_backtick_after(&msg, "unknown variant ").unwrap_or_default();
        return RecipeError::UnknownDriver {
            path: "/source/driver".to_string(),
            driver,
        };
    }

    // Default: general syntax / type-mismatch error.
    RecipeError::Syntax { path, message: msg }
}

/// Format the source location embedded in a `serde_yaml::Error` as a
/// JSON-pointer-ish path. serde_yaml 0.9.34 doesn't expose the YAML
/// path itself, only `(line, column, index)`; we encode the line/column
/// as a fragment under the root pointer.
fn format_serde_path(err: &serde_yaml::Error) -> String {
    match err.location() {
        Some(loc) => format!("/<line:{}:col:{}>", loc.line(), loc.column()),
        None => "/".to_string(),
    }
}

/// Extract the contents of the first backtick-quoted word after `prefix`
/// in `s`. Returns `None` if `prefix` isn't found or the first backtick
/// after `prefix` is unterminated.
fn extract_backtick_after(s: &str, prefix: &str) -> Option<String> {
    let after_prefix = s.find(prefix).map(|i| i + prefix.len())?;
    let rest = &s[after_prefix..];
    let open = rest.find('`')? + 1;
    let close_rel = rest[open..].find('`')?;
    Some(rest[open..open + close_rel].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DriverKind, OnError, OnMissingElement, WriteDisposition};

    fn minimal_csv_yaml() -> String {
        r#"
version: 1
name: minimal
model: ./acme.yaml
source:
  driver: csv
  path: ./data.csv
columns:
  - { source: spend, measure: Spend }
  - { source: channel, dimension: Channel }
"#
        .to_string()
    }

    #[test]
    fn parses_minimal_recipe() {
        let r = parse(&minimal_csv_yaml()).unwrap();
        assert_eq!(r.version, 1);
        assert_eq!(r.name, "minimal");
        assert_eq!(r.model, "./acme.yaml");
        assert!(matches!(r.source.driver, DriverKind::Csv));
        assert_eq!(r.columns.len(), 2);
        assert!(matches!(r.write_disposition, WriteDisposition::Replace));
        assert!(matches!(r.on_error, OnError::Abort));
        assert!(matches!(r.on_missing_element, OnMissingElement::Error));
        assert!(!r.incremental);
        assert_eq!(r.batch.size, None);
    }

    #[test]
    fn rejects_unsupported_version_mc5012() {
        let yaml = r#"
version: 2
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns: []
"#;
        let err = parse(yaml).unwrap_err();
        assert_eq!(err.code(), "MC5012");
    }

    #[test]
    fn rejects_unknown_driver_mc5002() {
        let yaml = r#"
version: 1
name: x
model: ./m.yaml
source: { driver: oracle, path: ./d.csv }
columns: []
"#;
        let err = parse(yaml).unwrap_err();
        assert_eq!(err.code(), "MC5002");
        if let RecipeError::UnknownDriver { driver, .. } = err {
            assert_eq!(driver, "oracle");
        } else {
            panic!("expected UnknownDriver");
        }
    }

    #[test]
    fn rejects_missing_field_mc5007() {
        // missing `model` field
        let yaml = r#"
version: 1
name: x
source: { driver: csv, path: ./d.csv }
columns: []
"#;
        let err = parse(yaml).unwrap_err();
        assert_eq!(err.code(), "MC5007");
    }

    #[test]
    fn rejects_general_syntax_mc5001() {
        let yaml = "{{{ not yaml at all";
        let err = parse(yaml).unwrap_err();
        assert_eq!(err.code(), "MC5001");
    }

    #[test]
    fn parses_explicit_defaults_and_credentials() {
        let yaml = r#"
version: 1
name: x
model: ./m.yaml
source: { driver: csv, path: ./d.csv }
columns:
  - { source: spend, measure: Spend }
defaults:
  Scenario: Baseline
  Version: Working
credentials:
  PG_DSN: "${env.PG_DSN}"
"#;
        let r = parse(yaml).unwrap();
        assert_eq!(r.defaults.get("Scenario").unwrap(), "Baseline");
        assert_eq!(r.credentials.get("PG_DSN").unwrap(), "${env.PG_DSN}");
    }

    #[test]
    fn empty_collections_default_to_empty_when_omitted() {
        let r = parse(&minimal_csv_yaml()).unwrap();
        assert!(r.defaults.is_empty());
        assert!(r.credentials.is_empty());
    }

    #[test]
    fn extract_backtick_after_finds_word() {
        let msg = "unknown variant `mysql`, expected one of `csv`, `sqlite`";
        let extracted = extract_backtick_after(msg, "unknown variant ");
        assert_eq!(extracted.as_deref(), Some("mysql"));
    }

    #[test]
    fn extract_backtick_after_returns_none_when_prefix_missing() {
        let extracted = extract_backtick_after("nothing here", "unknown variant ");
        assert_eq!(extracted, None);
    }
}
