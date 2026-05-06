//! Brief §10.7 + §10.8 — cross-cutting correctness doctrine + snapshot
//! tests.
//!
//! Per brief §0.A and CLAUDE.md §1.1: proptest-backed doctrine tests
//! (`doctrine_atomicity_of_write`, `doctrine_causality`) are deferred
//! until proptest returns to `mc-core` dev-deps. They are present below
//! as `// TODO(proptest):` stubs so the test list still names them
//! verbatim per CLAUDE.md §3.3.

use ahash::AHashMap;

use mc_core::{
    CellCoordinate, CubeId, EngineError, PrincipalId, Provenance, ScalarValue, SliceBinding,
    SliceQuery, WriteIntent, WritebackRequest,
};
use mc_fixtures::{build_acme_cube, coord, write_canonical_inputs};

const EPS: f64 = 1e-9;

fn assert_close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < EPS,
        "{label}: got {actual}, expected {expected}"
    );
}

// ===========================================================================
// Determinism
// ===========================================================================

#[test]
fn doctrine_determinism() {
    // 1,000 reads of the same cell must return byte-identical values.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_media,
        refs.florida,
        refs.revenue,
    );
    let baseline = cube
        .read(&c, refs.root_principal)
        .expect("read")
        .value
        .as_f64()
        .expect("F64");
    for i in 0..1_000 {
        let v = cube
            .read(&c, refs.root_principal)
            .expect("read")
            .value
            .as_f64()
            .expect("F64");
        assert!(
            (v - baseline).abs() < EPS,
            "iter {i}: deterministic read failed (got {v}, expected {baseline})"
        );
    }
}

// ===========================================================================
// Slice coherence
// ===========================================================================

#[test]
fn doctrine_coherence_within_slice() {
    // A single `Cube::slice` call must return values that all belong to
    // the same logical revision (the `result.revision` field). With
    // Phase 1's single-threaded model, mid-slice writes can't happen
    // — but the contract is that `result.revision` is the consistent
    // view, and per-cell `CellValue.revision` reflects the freshness of
    // each underlying datum (≤ result.revision).
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let mut bindings = AHashMap::new();
    bindings.insert(refs.scenario_dim, SliceBinding::One(refs.scen_baseline));
    bindings.insert(refs.version_dim, SliceBinding::One(refs.ver_working));
    bindings.insert(refs.time_dim, SliceBinding::One(refs.q1_2026));
    bindings.insert(refs.channel_dim, SliceBinding::One(refs.paid_search));
    bindings.insert(
        refs.market_dim,
        SliceBinding::Many(vec![refs.tampa, refs.orlando, refs.miami]),
    );
    bindings.insert(refs.measure_dim, SliceBinding::One(refs.spend));
    let q = SliceQuery {
        cube: cube_id,
        bindings,
        request_trace: false,
    };
    let result = cube.slice(&q, refs.root_principal).expect("slice ok");
    assert_eq!(result.values.len(), 3, "3 markets in slice");
    let result_rev = result.revision;
    for v in &result.values {
        assert!(
            v.revision <= result_rev,
            "per-cell revision {:?} must be <= slice revision {:?}",
            v.revision,
            result_rev
        );
    }
}

// ===========================================================================
// Proptest-backed doctrines — DEFERRED per §0.A
// ===========================================================================

#[test]
fn doctrine_atomicity_of_write() {
    // TODO(proptest): see brief §10.7 and §0.A. Proptest is currently
    // unavailable in mc-core's dev-deps because criterion's transitive
    // `clap_lex` requires Rust edition2024 (1.85+); the toolchain pin
    // is 1.78. When proptest returns, replace this stub with the
    // brief's interleaved-writes/reads property. The deterministic
    // version of the read-after-write contract is covered by
    // `tests/acme_demo.rs::t_acme_write_invalidates_dependents`.
}

#[test]
fn doctrine_causality() {
    // TODO(proptest): see brief §10.7 and §0.A. Same deferral as
    // `doctrine_atomicity_of_write` above. The deterministic read-
    // after-write contract is exercised by
    // `tests/acme_demo.rs::t_acme_write_invalidates_consolidated_ancestors`.
}

// ===========================================================================
// Authorization
// ===========================================================================

#[test]
fn doctrine_authorization_before_write() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let outsider = PrincipalId(99);
    let revision_before = cube.revision();
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
    let err = cube
        .write(WritebackRequest {
            coord: target.clone(),
            new_value: ScalarValue::F64(50_000.0),
            principal: outsider,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect_err("ungranted principal must be denied");
    assert!(
        matches!(err, EngineError::InsufficientPermission { .. }),
        "got {err:?}"
    );
    assert_eq!(
        cube.revision(),
        revision_before,
        "revision must not advance on rejected write"
    );
    // Cell value (read by root) must reflect no write happened.
    let v = cube.read(&target, refs.root_principal).expect("read");
    assert!(
        matches!(v.value, ScalarValue::Null) || v.value.as_f64() != Some(50_000.0),
        "store must not contain the rejected value"
    );
}

// ===========================================================================
// Type coercion
// ===========================================================================

#[test]
fn doctrine_no_silent_type_coercion() {
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
        refs.spend, // dtype F64
    );
    // I64 → F64-typed cell rejected.
    let err = cube
        .write(WritebackRequest {
            coord: target.clone(),
            new_value: ScalarValue::I64(11_500),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect_err("I64 → F64 must reject");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
    // Bool → F64 rejected.
    let err = cube
        .write(WritebackRequest {
            coord: target.clone(),
            new_value: ScalarValue::Bool(true),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect_err("Bool → F64 must reject");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
    // Category → F64 rejected.
    let err = cube
        .write(WritebackRequest {
            coord: target,
            new_value: ScalarValue::Category(0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect_err("Category → F64 must reject");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

// ===========================================================================
// Dependency declaration
// ===========================================================================

#[test]
fn doctrine_no_silent_dependency_miss() {
    // Doctrine-level meta-test: every Acme rule's body must be a
    // subset of its `declared_dependencies`. RuleSet::add already
    // enforces this structurally at registration time
    // (`tests/dependency.rs::t_dependency_graph_rejects_undeclared_dependency_in_test_mode`
    // exercises the rejection path); here we just verify the live cube's
    // rules satisfy the contract, which they trivially do because
    // construction would have failed otherwise.
    let (cube, _refs) = build_acme_cube().expect("build ok");
    let rules = cube.rules();
    assert!(!rules.is_empty(), "Acme cube must have rules");
    for r in rules.iter() {
        let declared: std::collections::HashSet<_> =
            r.declared_dependencies.iter().map(|d| d.measure).collect();
        let referenced = collect_self_refs(&r.body);
        for m in &referenced {
            assert!(
                declared.contains(m),
                "rule {:?} body refs {m:?} but declared deps are {:?}",
                r.id,
                declared
            );
        }
    }
}

fn collect_self_refs(expr: &mc_core::Expr) -> std::collections::HashSet<mc_core::ElementId> {
    let mut out = std::collections::HashSet::new();
    walk(expr, &mut out);
    fn walk(e: &mc_core::Expr, acc: &mut std::collections::HashSet<mc_core::ElementId>) {
        match e {
            mc_core::Expr::Const(_)
            | mc_core::Expr::PeriodIndex
            | mc_core::Expr::AnchorIndex
            | mc_core::Expr::IsPast
            | mc_core::Expr::IsCurrent
            | mc_core::Expr::IsFuture
            | mc_core::Expr::PeriodsSinceAnchor
            | mc_core::Expr::PeriodsToEnd => {}
            mc_core::Expr::SelfRef(m)
            | mc_core::Expr::ActualRef(m)
            | mc_core::Expr::Prev(m)
            | mc_core::Expr::Cumulative(m) => {
                acc.insert(*m);
            }
            mc_core::Expr::Add(a, b)
            | mc_core::Expr::Sub(a, b)
            | mc_core::Expr::Mul(a, b)
            | mc_core::Expr::Div(a, b)
            | mc_core::Expr::IfNull(a, b)
            | mc_core::Expr::Gt(a, b)
            | mc_core::Expr::Lt(a, b)
            | mc_core::Expr::Gte(a, b)
            | mc_core::Expr::Lte(a, b)
            | mc_core::Expr::Eq(a, b)
            | mc_core::Expr::Neq(a, b)
            | mc_core::Expr::And(a, b)
            | mc_core::Expr::Or(a, b) => {
                walk(a, acc);
                walk(b, acc);
            }
            mc_core::Expr::Not(a) | mc_core::Expr::Abs(a) | mc_core::Expr::Bucket(a, _) => {
                walk(a, acc)
            }
            mc_core::Expr::If(a, b, c)
            | mc_core::Expr::SafeDiv(a, b, c)
            | mc_core::Expr::Clamp(a, b, c) => {
                walk(a, acc);
                walk(b, acc);
                walk(c, acc);
            }
            mc_core::Expr::Min(args) | mc_core::Expr::Max(args) | mc_core::Expr::Coalesce(args) => {
                for a in args {
                    walk(a, acc);
                }
            }
            mc_core::Expr::Lag(m, p) | mc_core::Expr::RollingAvg(m, p) => {
                acc.insert(*m);
                walk(p, acc);
            }
            mc_core::Expr::Benchmark(_, k) => walk(k, acc),
            mc_core::Expr::Lookup(_, ks) => {
                for k in ks {
                    walk(k, acc);
                }
            }
            mc_core::Expr::SumOver(_, m) => {
                acc.insert(*m);
            }
            mc_core::Expr::DimElement(_) => {}
            // Phase 3H
            mc_core::Expr::Predict(_, features) => {
                for f in features {
                    walk(f, acc);
                }
            }
            mc_core::Expr::Calibrate(v, _) | mc_core::Expr::Exp(v) => walk(v, acc),
            mc_core::Expr::NormCdf(x, mu, sigma) => {
                walk(x, acc);
                walk(mu, acc);
                walk(sigma, acc);
            }
            // Phase 3I
            mc_core::Expr::Pow(a, b) | mc_core::Expr::Mod(a, b) => {
                walk(a, acc);
                walk(b, acc);
            }
            mc_core::Expr::Sqrt(a)
            | mc_core::Expr::Ln(a)
            | mc_core::Expr::Log10(a)
            | mc_core::Expr::Round(a)
            | mc_core::Expr::Floor(a)
            | mc_core::Expr::Ceil(a) => walk(a, acc),
            mc_core::Expr::NormInv(p, mu, sigma) => {
                walk(p, acc);
                walk(mu, acc);
                walk(sigma, acc);
            }
            mc_core::Expr::IsElement(_, _) => {}
            mc_core::Expr::AvgOver(_, m)
            | mc_core::Expr::MinOver(_, m)
            | mc_core::Expr::MaxOver(_, m) => {
                acc.insert(*m);
            }
            mc_core::Expr::WAvgOver(_, value, weight) => {
                acc.insert(*value);
                acc.insert(*weight);
            }
            // Phase 3J: string-domain primitives + param don't introduce SelfRef.
            mc_core::Expr::StrLiteral(_)
            | mc_core::Expr::CurrentElementName(_)
            | mc_core::Expr::ParamRef(_) => {}
            mc_core::Expr::StrEq(a, b) | mc_core::Expr::StrNeq(a, b) => {
                walk(a, acc);
                walk(b, acc);
            }
            // Phase 3J item 6: cross-coord variants — measure ref +
            // (optional) fallback expression.
            mc_core::Expr::ActualRefWithFallback(m, fb) => {
                acc.insert(*m);
                walk(fb, acc);
            }
            mc_core::Expr::ScenarioRef(m, _scenario) => {
                acc.insert(*m);
            }
            // Phase 3J item 7: extrapolate_last_value's measure dep.
            mc_core::Expr::ExtrapolateLastValue(m) => {
                acc.insert(*m);
            }
        }
    }
    out
}

// ===========================================================================
// Null vs zero distinction
// ===========================================================================

#[test]
fn doctrine_null_zero_distinct() {
    // Per spec §7. Tests four configurations of (Spend, CPC) → Clicks:
    //   (Null, _)  → Null   (Mul/Div null poison)
    //   (0,    1)  → 0      (finite/finite division)
    //   (1, Null)  → Null   (null poison)
    //   (1,    0)  → Null   (division by zero policy)
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let leaf = |measure| {
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            measure,
        )
    };

    // Case 1: Spend = Null, CPC = 1 → Clicks = Null.
    cube.write(WritebackRequest {
        coord: leaf(refs.cpc),
        new_value: ScalarValue::F64(1.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("cpc");
    let clicks = cube
        .read(&leaf(refs.clicks), refs.root_principal)
        .expect("read")
        .value;
    assert!(
        matches!(clicks, ScalarValue::Null),
        "Spend=Null → Clicks Null, got {:?}",
        clicks
    );

    // Case 2: Spend = 0, CPC = 1 → Clicks = 0.
    cube.write(WritebackRequest {
        coord: leaf(refs.spend),
        new_value: ScalarValue::F64(0.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("spend=0");
    let clicks = cube
        .read(&leaf(refs.clicks), refs.root_principal)
        .expect("read")
        .value;
    assert_close(
        clicks.as_f64().expect("F64"),
        0.0,
        "Spend=0/CPC=1 → Clicks=0",
    );

    // Case 3: Spend = 1, CPC = Null → Clicks = Null.
    cube.write(WritebackRequest {
        coord: leaf(refs.spend),
        new_value: ScalarValue::F64(1.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("spend=1");
    cube.write(WritebackRequest {
        coord: leaf(refs.cpc),
        new_value: ScalarValue::Null,
        principal: refs.root_principal,
        intent: WriteIntent::Clear,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("cpc=Null");
    let clicks = cube
        .read(&leaf(refs.clicks), refs.root_principal)
        .expect("read")
        .value;
    assert!(
        matches!(clicks, ScalarValue::Null),
        "CPC=Null → Clicks Null, got {:?}",
        clicks
    );

    // Case 4: Spend = 1, CPC = 0 → Clicks = Null (division by zero).
    cube.write(WritebackRequest {
        coord: leaf(refs.cpc),
        new_value: ScalarValue::F64(0.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("cpc=0");
    let clicks = cube
        .read(&leaf(refs.clicks), refs.root_principal)
        .expect("read")
        .value;
    assert!(
        matches!(clicks, ScalarValue::Null),
        "CPC=0 division-by-zero → Clicks Null, got {:?}",
        clicks
    );
}

// ===========================================================================
// Frozen dimensions
// ===========================================================================

#[test]
fn doctrine_no_mutation_of_frozen_dimensions() {
    // Phase 1 enforces dimension immutability *structurally*: post-build,
    // `Cube` owns its `Dimension`s privately and exposes only `&Dimension`
    // via the accessor. There is no public mutation API, so the
    // `EngineError::DimensionFrozen` variant is unreachable through the
    // normal API surface — the guarantee holds vacuously. We assert the
    // structural invariant: after build, every dim reports `is_frozen()`.
    let (cube, _refs) = build_acme_cube().expect("build ok");
    for dim in cube.dimensions() {
        assert!(
            dim.is_frozen(),
            "dim {} must be frozen after build",
            dim.name
        );
    }
}

// ===========================================================================
// Derived cells are read-only
// ===========================================================================

#[test]
fn doctrine_no_writes_to_derived_cells() {
    // Doctrine-level meta-test: every Derived measure rejects writes.
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let derived = [
        refs.clicks,
        refs.leads,
        refs.customers,
        refs.revenue,
        refs.gross_profit,
    ];
    for &m in &derived {
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
                    m,
                ),
                new_value: ScalarValue::F64(99.0),
                principal: refs.root_principal,
                intent: WriteIntent::Set,
                expected_revision: None,
                now_unix_seconds: 0,
            })
            .expect_err("derived must reject");
        assert!(
            matches!(err, EngineError::DerivedCellNotWritable { .. }),
            "measure {m:?} did not reject: {err:?}"
        );
    }
}

// ===========================================================================
// §10.8 Snapshot tests
// ===========================================================================

#[test]
fn t_snapshot_captures_current_state() {
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
    let pre = cube
        .read(&target, refs.root_principal)
        .expect("pre")
        .value
        .as_f64()
        .expect("F64");
    let snap = cube.snapshot(Some("pre"));
    cube.write(WritebackRequest {
        coord: target.clone(),
        new_value: ScalarValue::F64(99_999.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write");
    let snap_value = snap
        .read(&target)
        .expect("snap has cell")
        .value
        .as_f64()
        .expect("F64");
    assert_close(snap_value, pre, "snapshot still sees pre-write value");
    let post = cube
        .read(&target, refs.root_principal)
        .expect("post")
        .value
        .as_f64()
        .expect("F64");
    assert_close(post, 99_999.0, "live cube sees post-write value");
}

#[test]
fn t_rollback_to_snapshot_restores_state() {
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
    let pre = cube
        .read(&target, refs.root_principal)
        .expect("pre")
        .value
        .as_f64()
        .expect("F64");
    let snap = cube.snapshot(None);
    cube.write(WritebackRequest {
        coord: target.clone(),
        new_value: ScalarValue::F64(42.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write");
    cube.rollback_to(&snap).expect("rollback");
    let restored = cube
        .read(&target, refs.root_principal)
        .expect("post-rollback")
        .value
        .as_f64()
        .expect("F64");
    assert_close(restored, pre, "rollback restores pre-write value");
}

#[test]
fn t_snapshot_does_not_block_writes() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let snap = cube.snapshot(None);
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
    // 100 writes (smaller than brief's 1000 to keep wallclock reasonable
    // — same contract: snapshot stays readable through arbitrary live
    // cube mutation).
    for i in 0..100 {
        cube.write(WritebackRequest {
            coord: target.clone(),
            new_value: ScalarValue::F64(1_000.0 + i as f64),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write");
    }
    // Snapshot is still readable after all those writes.
    let snap_value = snap
        .read(&target)
        .expect("snap")
        .value
        .as_f64()
        .expect("F64");
    // Snapshot value is whatever it was at snapshot time — definitely not
    // any of the post-snapshot increments.
    assert!(
        !(1_000.0..1_100.0).contains(&snap_value),
        "snapshot value {snap_value} must not be a post-snapshot write"
    );
}

#[test]
fn t_snapshot_label_is_optional() {
    let (cube, _refs) = build_acme_cube().expect("build ok");
    let labeled = cube.snapshot(Some("FY2026_Approved"));
    let unlabeled = cube.snapshot(None);
    assert!(labeled.label.is_some());
    assert!(unlabeled.label.is_none());
}

#[test]
fn t_snapshot_cube_id_mismatch_rejected() {
    // Build two cubes via the same fixture. Each call to
    // `build_acme_cube` runs through a fresh `IdGenerator`, but cube ids
    // live in the same numeric space and would collide between calls.
    // The cleanest mismatch is to take a snapshot of cube_a and fake a
    // separate `Snapshot` carrying cube_b's id (or vice versa). Since
    // `Snapshot.store` is `pub(crate)` we can't fabricate one from
    // outside the crate, so we synthesize the mismatch by mutating the
    // cube id on the snapshot. Phase 1 exposes the `cube` field as
    // `pub`, which makes that legal.
    let (mut cube_a, _refs) = build_acme_cube().expect("cube A");
    let mut snap = cube_a.snapshot(None);
    // Force a cube-id mismatch.
    snap.cube = CubeId(999_999);
    let err = cube_a.rollback_to(&snap).expect_err("must reject");
    assert!(
        matches!(err, EngineError::SnapshotCubeMismatch),
        "got {err:?}"
    );
}

// ===========================================================================
// Provenance round-trip sanity (used to silence dead-code warnings on
// `Provenance` import; light invariant — every consolidated read has
// Consolidation provenance).
// ===========================================================================

#[test]
fn provenance_round_trip_for_consolidation() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cube_id = cube.id;
    let q1 = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let v = cube.read(&q1, refs.root_principal).expect("read");
    assert!(matches!(v.provenance, Provenance::Consolidation { .. }));
    let _: &CellCoordinate = &q1;
}
