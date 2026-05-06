//! Scalar values and cell data types.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.3.

use crate::error::EngineError;

/// The actual datum carried by every cell.
///
/// `ScalarValue::Null` is a first-class value distinct from numeric zero
/// (per engine-semantics §7.3 I-CellValue-7 and §21 rule 8).
#[derive(Clone, PartialEq, Debug)]
pub enum ScalarValue {
    F64(f64),
    I64(i64),
    Bool(bool),
    /// Index into the parent measure's `CellDataType::Category` vec.
    Category(usize),
    /// String value produced by expression evaluation only.
    ///
    /// **Binding boundary (Phase 3J ADR-0016 Decision 2 / Amendment §1):**
    /// `Str` values exist exclusively in the expression-evaluation domain.
    /// They are produced by `Expr::StrLiteral`, `Expr::DimElement`, and
    /// `CrossCoordRead::CurrentElementName`, and consumed by `Expr::StrEq`
    /// / `Expr::StrNeq`, by lookup-key conversion (`scalar_to_lookup_key`),
    /// and by parse-time element-name resolution. They MUST NEVER reach:
    /// `Cube::write` (rejected with `EngineError::TypeMismatch`),
    /// `HashMapStore` storage, consolidation, the dirty tracker, snapshot
    /// machinery, the writeback NaN check (`debug_assert!` guards this),
    /// or any cell value comparison in trace output.
    ///
    /// This bounded scope keeps Phase 3J shippable as an additive eval-
    /// layer change. Storing strings in cells would cascade through every
    /// kernel subsystem (consolidation needs type-aware aggregation;
    /// dirty propagation needs string-aware diffs; writeback needs to
    /// re-define the NaN-rejection contract); that work is Phase 4+ scope.
    Str(String),
    Null,
}

impl ScalarValue {
    /// Construct an `F64` value while rejecting NaN and infinity.
    ///
    /// Per spec §3.3: NaN must never appear in storage. The Cube's writeback
    /// path uses this constructor at the API boundary; tests in
    /// `tests/value_nan.rs` pin the behavior. Phase 1 treats NaN/Inf as a
    /// programming error, not a data value.
    pub fn checked_f64(v: f64) -> Result<Self, EngineError> {
        validate_finite_f64(v)?;
        Ok(ScalarValue::F64(v))
    }

    /// Returns the f64 if this is an `F64` variant, else None.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ScalarValue::F64(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns the i64 if this is an `I64` variant, else None.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            ScalarValue::I64(v) => Some(*v),
            _ => None,
        }
    }

    /// The data-type tag corresponding to this value.
    ///
    /// `Null` does not have an intrinsic dtype — it conforms to whatever the
    /// surrounding measure declares. We return `CellDataType::F64` as a
    /// stable placeholder for `Null` because the data-type-matches check
    /// (see `CellDataType::matches`) treats `Null` as compatible with every
    /// dtype regardless.
    pub fn dtype(&self) -> CellDataType {
        match self {
            ScalarValue::F64(_) => CellDataType::F64,
            ScalarValue::I64(_) => CellDataType::I64,
            ScalarValue::Bool(_) => CellDataType::Bool,
            // Categories carry their domain at the measure level, not at the
            // value level. Returning a placeholder dtype here is fine because
            // `CellDataType::matches` is the actual validation primitive and
            // it consults the measure's dtype, not the value's.
            ScalarValue::Category(_) => CellDataType::F64,
            ScalarValue::Str(_) => CellDataType::F64, // Str is transient; never stored
            ScalarValue::Null => CellDataType::F64,
        }
    }

    /// True if this value is `Null`.
    pub fn is_null(&self) -> bool {
        matches!(self, ScalarValue::Null)
    }
}

/// Declared data type of a measure. Stored on `MeasureMeta`; cells report
/// the measure's dtype, not their own.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CellDataType {
    F64,
    I64,
    Bool,
    /// Inclusive category list. Stored on the measure.
    Category(Vec<String>),
}

impl CellDataType {
    /// True if `value` is a permissible inhabitant of this dtype.
    ///
    /// `Null` is compatible with every dtype (per spec §7 I-CellValue-1
    /// + null-poison policy). `F64` rejects non-finite values; that check
    /// lives in `validate_finite_f64`, not here, because `CellValue` may
    /// pre-validate at construction time and skip the re-check in hot paths.
    pub fn matches(&self, value: &ScalarValue) -> bool {
        match (self, value) {
            (_, ScalarValue::Null) => true,
            (CellDataType::F64, ScalarValue::F64(_)) => true,
            (CellDataType::I64, ScalarValue::I64(_)) => true,
            (CellDataType::Bool, ScalarValue::Bool(_)) => true,
            (CellDataType::Category(domain), ScalarValue::Category(idx)) => *idx < domain.len(),
            _ => false,
        }
    }
}

/// Reject `f64` values that must not appear in storage: NaN and ±Infinity.
///
/// Returns `EngineError::InvalidValue` with a static reason string. Cube
/// writeback calls this at the API boundary; the integration tests in
/// `tests/value_nan.rs` exercise both NaN and Inf rejections.
pub fn validate_finite_f64(v: f64) -> Result<(), EngineError> {
    if v.is_nan() {
        return Err(EngineError::InvalidValue(
            "f64 NaN is not a valid cell value",
        ));
    }
    if v.is_infinite() {
        return Err(EngineError::InvalidValue(
            "f64 ±Infinity is not a valid cell value",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtype_matches_basic_types() {
        assert!(CellDataType::F64.matches(&ScalarValue::F64(1.5)));
        assert!(CellDataType::I64.matches(&ScalarValue::I64(42)));
        assert!(CellDataType::Bool.matches(&ScalarValue::Bool(true)));
    }

    #[test]
    fn dtype_rejects_cross_type_values() {
        assert!(!CellDataType::F64.matches(&ScalarValue::I64(42)));
        assert!(!CellDataType::I64.matches(&ScalarValue::F64(1.0)));
        assert!(!CellDataType::Bool.matches(&ScalarValue::F64(1.0)));
    }

    #[test]
    fn null_is_compatible_with_every_dtype() {
        // Per §7 I-CellValue-1: dtype matches even when value is Null.
        assert!(CellDataType::F64.matches(&ScalarValue::Null));
        assert!(CellDataType::I64.matches(&ScalarValue::Null));
        assert!(CellDataType::Bool.matches(&ScalarValue::Null));
        assert!(CellDataType::Category(vec!["a".into()]).matches(&ScalarValue::Null));
    }

    #[test]
    fn category_index_must_be_in_domain() {
        let dt = CellDataType::Category(vec!["a".into(), "b".into()]);
        assert!(dt.matches(&ScalarValue::Category(0)));
        assert!(dt.matches(&ScalarValue::Category(1)));
        assert!(!dt.matches(&ScalarValue::Category(2)));
    }

    #[test]
    fn checked_f64_accepts_finite_values() {
        assert!(matches!(
            ScalarValue::checked_f64(1.5),
            Ok(ScalarValue::F64(_))
        ));
        assert!(matches!(
            ScalarValue::checked_f64(0.0),
            Ok(ScalarValue::F64(_))
        ));
        assert!(matches!(
            ScalarValue::checked_f64(-1e10),
            Ok(ScalarValue::F64(_))
        ));
    }

    #[test]
    fn null_is_null() {
        assert!(ScalarValue::Null.is_null());
        assert!(!ScalarValue::F64(0.0).is_null());
    }
}
