//! Brief §10.2 — writeback validation and intent semantics.
//!
//! Test names per CLAUDE.md §2.6 / §3.3 are contractual; do not rename.

use mc_core::{
    EngineError, PrincipalId, Revision, ScalarValue, VersionState, WriteIntent, WritebackRequest,
};
use mc_fixtures::{build_acme_cube, coord, write_canonical_inputs};

#[test]
fn t_write_to_derived_cell_returns_error() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    // Revenue is Derived; writes must be rejected per spec §13 I-WB-2.
    let req = WritebackRequest {
        coord: coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.revenue,
        ),
        new_value: ScalarValue::F64(99.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    };
    let revision_before = cube.revision();
    let err = cube.write(req).expect_err("derived must reject");
    assert!(
        matches!(err, EngineError::DerivedCellNotWritable { .. }),
        "got {err:?}"
    );
    assert_eq!(
        cube.revision(),
        revision_before,
        "revision must not advance on rejected write"
    );
}

#[test]
fn t_write_to_consolidated_cell_returns_error() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    // Q1_2026 is consolidated in Time; spec §13 I-WB-1.
    let req = WritebackRequest {
        coord: coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.q1_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        new_value: ScalarValue::F64(50_000.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    };
    let err = cube.write(req).expect_err("consolidated must reject");
    assert!(
        matches!(err, EngineError::ConsolidatedCellNotWritable { .. }),
        "got {err:?}"
    );
}

#[test]
fn t_write_with_wrong_dtype_returns_error() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let req = WritebackRequest {
        coord: coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        new_value: ScalarValue::I64(50_000),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    };
    let err = cube.write(req).expect_err("type mismatch must reject");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

#[test]
fn t_write_with_nan_returns_error() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let req = WritebackRequest {
        coord: coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        new_value: ScalarValue::F64(f64::NAN),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    };
    let err = cube.write(req).expect_err("NaN must reject");
    assert!(matches!(err, EngineError::InvalidValue(_)), "got {err:?}");
}

#[test]
fn t_write_with_inf_returns_error() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let req = WritebackRequest {
        coord: coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        new_value: ScalarValue::F64(f64::INFINITY),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    };
    let err = cube.write(req).expect_err("Inf must reject");
    assert!(matches!(err, EngineError::InvalidValue(_)), "got {err:?}");
}

#[test]
fn t_write_stale_revision_returns_error() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let r0 = cube.revision();
    let target = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    cube.write(WritebackRequest {
        coord: target.clone(),
        new_value: ScalarValue::F64(11_500.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("first write");
    let r1 = cube.revision();
    assert_ne!(r0, r1, "revision must advance");
    // Now attempt a write that expects the *old* revision.
    let err = cube
        .write(WritebackRequest {
            coord: target,
            new_value: ScalarValue::F64(99_999.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: Some(r0),
            now_unix_seconds: 0,
        })
        .expect_err("stale revision must reject");
    assert!(
        matches!(err, EngineError::StaleRevision { .. }),
        "got {err:?}"
    );
}

#[test]
fn t_write_revision_bumps_monotonically() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let target = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let mut prev = cube.revision();
    for i in 0..100 {
        let res = cube
            .write(WritebackRequest {
                coord: target.clone(),
                new_value: ScalarValue::F64(1_000.0 + i as f64),
                principal: refs.root_principal,
                intent: WriteIntent::Set,
                expected_revision: None,
                now_unix_seconds: 0,
            })
            .expect("write ok");
        assert_eq!(res.revision_before, prev, "iter {i}: before");
        assert_eq!(
            res.revision_after,
            Revision(prev.0 + 1),
            "iter {i}: after must be exactly +1"
        );
        prev = res.revision_after;
    }
}

#[test]
fn t_write_to_approved_version_returns_error() {
    // Per spec §9 I-Ver-3 / §13 I-WB-3: writes to Approved versions are
    // rejected with LockedVersion.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let req = WritebackRequest {
        coord: coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_approved,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        new_value: ScalarValue::F64(50_000.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    };
    let err = cube.write(req).expect_err("approved version must reject");
    match err {
        EngineError::LockedVersion { state, .. } => {
            assert_eq!(state, VersionState::Approved, "expected Approved state");
        }
        other => panic!("expected LockedVersion, got {other:?}"),
    }
}

#[test]
fn t_write_with_invalid_principal_returns_error() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let outsider = PrincipalId(999);
    let req = WritebackRequest {
        coord: coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            refs.spend,
        ),
        new_value: ScalarValue::F64(11_500.0),
        principal: outsider,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    };
    let err = cube
        .write(req)
        .expect_err("ungranted principal must reject");
    assert!(
        matches!(err, EngineError::InsufficientPermission { .. }),
        "got {err:?}"
    );
}

#[test]
fn t_write_increment_intent() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let target = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    // Read the current value to know the baseline.
    let pre = cube
        .read(&target, refs.root_principal)
        .expect("read pre")
        .value
        .as_f64()
        .expect("F64");
    let delta = 250.0;
    cube.write(WritebackRequest {
        coord: target.clone(),
        new_value: ScalarValue::F64(delta),
        principal: refs.root_principal,
        intent: WriteIntent::Increment,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("increment ok");
    let post = cube
        .read(&target, refs.root_principal)
        .expect("read post")
        .value
        .as_f64()
        .expect("F64");
    assert!(
        (post - (pre + delta)).abs() < 1e-9,
        "post={post}, pre+delta={}",
        pre + delta
    );
}

#[test]
fn t_write_clear_intent() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let target = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    cube.write(WritebackRequest {
        coord: target.clone(),
        new_value: ScalarValue::Null,
        principal: refs.root_principal,
        intent: WriteIntent::Clear,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("clear ok");
    let v = cube.read(&target, refs.root_principal).expect("read post");
    assert!(matches!(v.value, ScalarValue::Null), "got {:?}", v.value);
}
