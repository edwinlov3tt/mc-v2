//! Brief §10.6 — locks and permissions.
//!
//! Per CLAUDE.md §3.3 / brief §10 test names are contractual.

use mc_core::{
    capability, CapabilitySet, EngineError, Grant, Lock, LockId, LockKind, PrincipalId,
    ScalarValue, ScopeBinding, ScopePattern, WriteIntent, WritebackRequest,
};
use mc_fixtures::{build_acme_cube, coord};

// ===========================================================================
// Permissions
// ===========================================================================

#[test]
fn t_root_principal_can_read_and_write_anywhere() {
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
    cube.write(WritebackRequest {
        coord: target.clone(),
        new_value: ScalarValue::F64(11_500.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("root write");
    let v = cube.read(&target, refs.root_principal).expect("root read");
    assert_eq!(v.value.as_f64(), Some(11_500.0));
}

#[test]
fn t_non_root_with_no_grant_cannot_read() {
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
    let outsider = PrincipalId(99);
    let err = cube
        .read(&target, outsider)
        .expect_err("ungranted read must reject");
    assert!(
        matches!(err, EngineError::InsufficientPermission { .. }),
        "got {err:?}"
    );
}

#[test]
fn t_grant_for_subtree_allows_writes_in_subtree() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let analyst = PrincipalId(50);
    // Grant analyst Write on the Florida market subtree (across all
    // other dims).
    cube.grant(Grant {
        principal: analyst,
        pattern: ScopePattern::new().with(refs.market_dim, ScopeBinding::Subtree(refs.florida)),
        capabilities: CapabilitySet::with(capability::READ | capability::WRITE),
    });
    // Tampa is in the Florida subtree → write succeeds.
    cube.write(WritebackRequest {
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
        new_value: ScalarValue::F64(12_000.0),
        principal: analyst,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("Tampa (in Florida subtree) must succeed");
    // Atlanta is in Georgia, not Florida → write rejected.
    let err = cube
        .write(WritebackRequest {
            coord: coord(
                cube_id,
                &refs,
                refs.scen_baseline,
                refs.ver_working,
                refs.mar_2026,
                refs.paid_search,
                refs.atlanta,
                refs.spend,
            ),
            new_value: ScalarValue::F64(12_000.0),
            principal: analyst,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect_err("Atlanta (outside Florida) must reject");
    assert!(
        matches!(err, EngineError::InsufficientPermission { .. }),
        "got {err:?}"
    );
}

// ===========================================================================
// Locks
// ===========================================================================

fn florida_pattern(refs: &mc_fixtures::AcmeRefs) -> ScopePattern {
    ScopePattern::new().with(refs.market_dim, ScopeBinding::Subtree(refs.florida))
}

fn make_hard_lock(refs: &mc_fixtures::AcmeRefs, owner: PrincipalId, expires_at: u64) -> Lock {
    Lock {
        id: LockId(1),
        owner,
        pattern: florida_pattern(refs),
        kind: LockKind::Hard,
        acquired_at: 0,
        expires_at,
        note: None,
    }
}

#[test]
fn t_hard_lock_blocks_other_principals() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let principal_a = refs.root_principal;
    let principal_b = PrincipalId(60);
    // Grant B Write on Florida AND Atlanta so the failure is permission-
    // independent.
    cube.grant(Grant {
        principal: principal_b,
        pattern: ScopePattern::new(), // empty pattern == All dims unconstrained
        capabilities: CapabilitySet::with(capability::READ | capability::WRITE),
    });

    // A acquires a Hard lock on Florida (root has LOCK capability
    // implicitly).
    cube.acquire_lock(make_hard_lock(&refs, principal_a, 1_000_000))
        .expect("A acquires Hard lock on Florida");

    // B writes to Atlanta — succeeds (different subtree).
    cube.write(WritebackRequest {
        coord: coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.atlanta,
            refs.spend,
        ),
        new_value: ScalarValue::F64(11_700.0),
        principal: principal_b,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("Atlanta write by B succeeds (outside lock)");
    // B's write to Tampa (inside Florida lock) is rejected.
    let err = cube
        .write(WritebackRequest {
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
            new_value: ScalarValue::F64(99_999.0),
            principal: principal_b,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect_err("Tampa (inside Florida lock) must reject for B");
    assert!(matches!(err, EngineError::LockedCell { .. }), "got {err:?}");
}

#[test]
fn t_lock_owner_can_still_write_within_lock() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let owner = refs.root_principal;
    cube.acquire_lock(make_hard_lock(&refs, owner, 1_000_000))
        .expect("acquire");
    // Owner writes inside the locked subtree — succeeds.
    cube.write(WritebackRequest {
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
        principal: owner,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("owner write within own lock succeeds");
}

#[test]
fn t_expired_lock_does_not_block() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let principal_a = refs.root_principal;
    let principal_b = PrincipalId(70);
    cube.grant(Grant {
        principal: principal_b,
        pattern: ScopePattern::new(),
        capabilities: CapabilitySet::with(capability::READ | capability::WRITE),
    });
    // Lock expires at t=10.
    cube.acquire_lock(make_hard_lock(&refs, principal_a, 10))
        .expect("acquire lock with short expiry");
    // After t=20, the lock has expired. B's write must succeed.
    cube.write(WritebackRequest {
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
        principal: principal_b,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 20,
    })
    .expect("post-expiry write succeeds");
}

#[test]
fn t_soft_lock_allows_writes_but_marks_advisory() {
    // Phase 1 surfaces soft-lock advisories via WritebackResult.soft_lock_notes
    // (per spec §18 I-Lock-3). The lock does NOT block.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let owner = refs.root_principal;
    let other = PrincipalId(80);
    cube.grant(Grant {
        principal: other,
        pattern: ScopePattern::new(),
        capabilities: CapabilitySet::with(capability::READ | capability::WRITE),
    });
    let soft = Lock {
        id: LockId(2),
        owner,
        pattern: florida_pattern(&refs),
        kind: LockKind::Soft,
        acquired_at: 0,
        expires_at: 1_000_000,
        note: Some("Florida budget under review".to_string()),
    };
    cube.acquire_lock(soft).expect("acquire soft");
    let result = cube
        .write(WritebackRequest {
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
            principal: other,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("soft lock does not block");
    assert!(
        result.soft_lock_notes.iter().any(|n| n.contains("Florida")),
        "soft-lock note must surface in WritebackResult: {:?}",
        result.soft_lock_notes
    );
}

#[test]
fn t_release_lock_by_non_owner_without_unlock_capability_fails() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let owner = refs.root_principal;
    let intruder = PrincipalId(90);
    let lock_id = cube
        .acquire_lock(make_hard_lock(&refs, owner, 1_000_000))
        .expect("acquire");
    let err = cube
        .release_lock(lock_id, intruder)
        .expect_err("non-owner release must reject");
    assert!(
        matches!(err, EngineError::InsufficientPermission { .. }),
        "got {err:?}"
    );
}
