//! NaN / Infinity rejection at the value boundary.
//!
//! Pin spec §3.3 + the v1.1 cleanup commitment:
//!   "ScalarValue::F64(f) where f.is_nan() is rejected at the writeback
//!    boundary (returns WritebackError::InvalidValue). NaN must never
//!    appear in storage."
//!
//! In Phase 1 there is no `Cube::write` yet (cube.rs is a later
//! deliverable). The same check is exposed via `validate_finite_f64` and
//! `ScalarValue::checked_f64` so tests can pin behavior today, and so
//! `Cube::write` will call the same primitive when it lands.

use mc_core::{validate_finite_f64, EngineError, ScalarValue};

#[test]
fn validate_finite_f64_rejects_nan() {
    let err = validate_finite_f64(f64::NAN).expect_err("NaN must be rejected");
    assert!(matches!(err, EngineError::InvalidValue(_)));
}

#[test]
fn validate_finite_f64_rejects_positive_infinity() {
    let err = validate_finite_f64(f64::INFINITY).expect_err("+Infinity must be rejected");
    assert!(matches!(err, EngineError::InvalidValue(_)));
}

#[test]
fn validate_finite_f64_rejects_negative_infinity() {
    let err = validate_finite_f64(f64::NEG_INFINITY).expect_err("-Infinity must be rejected");
    assert!(matches!(err, EngineError::InvalidValue(_)));
}

#[test]
fn validate_finite_f64_accepts_normal_values() {
    validate_finite_f64(0.0).expect("0.0 must be accepted");
    validate_finite_f64(-0.0).expect("-0.0 must be accepted");
    validate_finite_f64(1.5).expect("1.5 must be accepted");
    validate_finite_f64(-1e100).expect("-1e100 must be accepted");
    validate_finite_f64(f64::MIN_POSITIVE).expect("MIN_POSITIVE must be accepted (subnormal)");
    validate_finite_f64(f64::MAX).expect("MAX must be accepted");
}

#[test]
fn checked_f64_constructor_rejects_nan() {
    let err =
        ScalarValue::checked_f64(f64::NAN).expect_err("ScalarValue::checked_f64(NaN) must error");
    assert!(matches!(err, EngineError::InvalidValue(_)));
}

#[test]
fn checked_f64_constructor_rejects_inf() {
    let err = ScalarValue::checked_f64(f64::INFINITY)
        .expect_err("ScalarValue::checked_f64(+Inf) must error");
    assert!(matches!(err, EngineError::InvalidValue(_)));
}

#[test]
fn checked_f64_constructor_accepts_finite_value() {
    let v = ScalarValue::checked_f64(11_500.0).expect("finite must pass");
    match v {
        ScalarValue::F64(x) => assert!((x - 11_500.0).abs() < 1e-12),
        other => panic!("expected F64, got {other:?}"),
    }
}

#[test]
fn dtype_match_does_not_re_check_nan() {
    // Per value.rs: `CellDataType::matches` is structural — it verifies
    // dtype tags, not finiteness. NaN finiteness is a separate concern
    // handled by `validate_finite_f64`. This test pins that separation so
    // a future refactor doesn't accidentally couple the two.
    use mc_core::CellDataType;
    let nan = ScalarValue::F64(f64::NAN);
    // dtype matches structurally (F64 matches F64) — even though the value
    // is non-finite. The boundary check (validate_finite_f64) is the place
    // that rejects it.
    assert!(CellDataType::F64.matches(&nan));
}
