//! Phase 2D — `WritebackResult.invalidated` semantic correction tests.
//!
//! Per Phase 2D handoff §A.6 (`docs/handoffs/phase-2d-handoff.md`).
//! These tests pin the *marginal* semantics of
//! `WritebackResult.invalidated` per the brief's type doc + spec
//! §13 + I-WB-7: `invalidated` contains exactly the coords that
//! transitioned **clean → dirty during this single `write()` call**.
//! Coords already dirty before the call must NOT appear in
//! `invalidated`.
//!
//! Phase 1A inadvertently implemented the cumulative-dirty reading
//! (the brief's compact pseudocode at line 1938 was ambiguous);
//! Phase 2D corrects it. Tests A–D are the regression net that, had
//! they existed in Phase 1A, would have caught the bug. Test D in
//! particular ties the new semantics to the brief §10.1 per-write
//! upper bound (≤ 215 marks), which the cumulative reading
//! trivially exceeded once any prior dirty had accumulated.

use mc_core::{CellCoordinate, ScalarValue, WriteIntent, WritebackRequest};
use mc_fixtures::{build_acme_cube, coord, write_canonical_inputs};

/// Brief §10.1 per-write delta upper bound: ≤ 6 × ancestor_count + 5
/// = 215. The `invalidated` Vec must respect this on every individual
/// write under the corrected marginal semantics.
const PER_WRITE_DIRTY_BOUND: usize = 215;

/// **Test A — Fresh write on a clean cube reports the marginal closure.**
///
/// Build Acme. Without loading any inputs, write a single Spend
/// cell. `invalidated` must enumerate exactly the cells the brief
/// §10.1 / §13 worked example name: the 5 derived measures at the
/// same leaf coord plus the same measures at every consolidated
/// ancestor coord. Per the §10.1 bound, the count is ≤ 215.
///
/// On a clean cube the marginal set equals the cumulative dirty
/// set — `invalidated.len() == cube.dirty().len()`.
#[test]
fn t_phase_2d_write_a_clean_cube_invalidated_is_marginal_closure() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    assert_eq!(cube.dirty().len(), 0, "fresh cube must start clean");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let result = cube
        .write(WritebackRequest {
            coord: c.clone(),
            new_value: ScalarValue::F64(11_500.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write");

    assert!(
        result.invalidated.len() <= PER_WRITE_DIRTY_BOUND,
        "invalidated.len()={} exceeds brief §10.1 per-write bound {PER_WRITE_DIRTY_BOUND}",
        result.invalidated.len()
    );
    assert_eq!(
        result.invalidated.len(),
        cube.dirty().len(),
        "on a clean cube the marginal invalidated set must equal the cumulative dirty set"
    );

    // Membership: rule-dependents (the 5 derived measures at the same
    // leaf coord) must all appear.
    let derived_at_same_leaf: Vec<CellCoordinate> = [
        refs.clicks,
        refs.leads,
        refs.customers,
        refs.revenue,
        refs.gross_profit,
    ]
    .into_iter()
    .map(|m| {
        coord(
            cube_id,
            &refs,
            refs.scen_baseline,
            refs.ver_working,
            refs.mar_2026,
            refs.paid_search,
            refs.tampa,
            m,
        )
    })
    .collect();
    for derived_coord in &derived_at_same_leaf {
        assert!(
            result.invalidated.contains(derived_coord),
            "missing derived dependent in invalidated: {derived_coord:?}"
        );
    }

    // Membership: at least one hierarchy ancestor coord (Q1 Spend at
    // the same leaf market×channel) must appear — proves the
    // hierarchy-walk arm contributes to the marginal set too.
    let q1_spend_ancestor = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.q1_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    assert!(
        result.invalidated.contains(&q1_spend_ancestor),
        "missing hierarchy ancestor in invalidated: {q1_spend_ancestor:?}"
    );
}

/// **Test B — Repeated identical write returns empty `invalidated`
/// for already-dirty dependents.**
///
/// Without any intervening reads, the second write at the same coord
/// must NOT re-report dependents that are already dirty from the
/// first write. The cumulative `cube.dirty` is unchanged-or-larger
/// across the two writes; `WritebackResult.invalidated` for the
/// second write is a strict subset (here: empty, since the only
/// transitions are duplicates).
#[test]
fn t_phase_2d_write_b_repeated_write_skips_already_dirty() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );

    let first = cube
        .write(WritebackRequest {
            coord: c.clone(),
            new_value: ScalarValue::F64(11_500.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("first write");
    let invalidated_first = first.invalidated.len();
    let dirty_after_first = cube.dirty().len();
    assert!(
        invalidated_first > 0,
        "first write on a clean cube must report some marginal coords"
    );

    let second = cube
        .write(WritebackRequest {
            coord: c.clone(),
            new_value: ScalarValue::F64(99_999.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("second write");
    let invalidated_second = second.invalidated.len();
    let dirty_after_second = cube.dirty().len();

    assert_eq!(
        invalidated_second, 0,
        "second identical write must NOT re-report already-dirty dependents \
         (got invalidated.len()={invalidated_second}, expected 0; \
         dirty: {dirty_after_first} → {dirty_after_second})"
    );
    assert!(
        dirty_after_second >= dirty_after_first,
        "cumulative dirty set must not shrink across writes \
         ({dirty_after_first} → {dirty_after_second})"
    );
}

/// **Test C — After recompute, transitions clean → dirty are
/// reported again.**
///
/// Read forces recompute on a derived cell, which clears its dirty
/// flag. A subsequent write at an upstream input must re-report the
/// recomputed cell in `invalidated` (it transitioned clean → dirty
/// again). This is the load-bearing semantic distinction:
/// `invalidated` is a *transition* set, not a *cumulative-state*
/// set.
#[test]
fn t_phase_2d_write_c_recompute_then_redirty_reports_again() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;
    let spend = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let cpc = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.cpc,
    );
    let revenue = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.revenue,
    );

    // Set up: write Spend + CPC so Revenue is computable.
    cube.write(WritebackRequest {
        coord: spend.clone(),
        new_value: ScalarValue::F64(11_500.0),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write spend");
    cube.write(WritebackRequest {
        coord: cpc.clone(),
        new_value: ScalarValue::F64(1.5),
        principal: refs.root_principal,
        intent: WriteIntent::Set,
        expected_revision: None,
        now_unix_seconds: 0,
    })
    .expect("write cpc");

    // Force recompute of Revenue → its dirty bit gets cleared.
    let _ = cube
        .read(&revenue, refs.root_principal)
        .expect("recompute revenue");
    assert!(
        !cube.dirty().is_dirty(&revenue),
        "post-recompute Revenue must be clean before the third write"
    );

    // Third write: Spend at the same coord. Revenue at this coord
    // now transitions clean → dirty again — must appear in
    // `invalidated`.
    let third = cube
        .write(WritebackRequest {
            coord: spend.clone(),
            new_value: ScalarValue::F64(50_000.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("third write");

    assert!(
        third.invalidated.contains(&revenue),
        "Revenue at the same leaf coord transitioned clean → dirty and must appear in \
         the third write's invalidated set (got len={})",
        third.invalidated.len()
    );
    assert!(
        third.invalidated.len() <= PER_WRITE_DIRTY_BOUND,
        "third write invalidated.len()={} exceeds brief §10.1 per-write bound \
         {PER_WRITE_DIRTY_BOUND}",
        third.invalidated.len()
    );
}

/// **Test D — Bulk-ingest preserves the §10.1 per-write bound.**
///
/// Run `write_canonical_inputs` (the 2,520-write bulk loader). For
/// each individual write, assert
/// `result.invalidated.len() ≤ PER_WRITE_DIRTY_BOUND` and
/// `cube.dirty().len()` grows monotonically. This is the test that
/// would have caught the Phase 1A bug originally — under the
/// cumulative reading, `invalidated.len()` would have grown without
/// bound across the bulk-load (peaking near the cumulative dirty
/// saturation), which is exactly the §6.14 cliff.
#[test]
fn t_phase_2d_write_d_bulk_ingest_preserves_per_write_bound() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    let cube_id = cube.id;

    // Walk the canonical-input write list directly so we can
    // observe each WritebackResult instead of going through
    // `write_canonical_inputs` (which discards them). The
    // (time × channel × market × measure) layout matches
    // mc-fixtures::write_canonical_inputs.
    let times: [mc_core::ElementId; 12] = [
        refs.jan_2026,
        refs.feb_2026,
        refs.mar_2026,
        refs.apr_2026,
        refs.may_2026,
        refs.jun_2026,
        refs.jul_2026,
        refs.aug_2026,
        refs.sep_2026,
        refs.oct_2026,
        refs.nov_2026,
        refs.dec_2026,
    ];
    let channels: [mc_core::ElementId; 5] = [
        refs.paid_search,
        refs.paid_social,
        refs.display,
        refs.email,
        refs.organic,
    ];
    let markets: [mc_core::ElementId; 7] = [
        refs.tampa,
        refs.orlando,
        refs.miami,
        refs.atlanta,
        refs.charlotte,
        refs.new_york_city,
        refs.boston,
    ];
    let measures: [mc_core::ElementId; 6] = [
        refs.spend,
        refs.cpc,
        refs.cvr,
        refs.close_rate,
        refs.aov,
        refs.cogs_rate,
    ];

    let mut prior_dirty = cube.dirty().len();
    let mut writes_observed = 0usize;
    let mut max_invalidated_observed = 0usize;
    for &t in &times {
        for &ch in &channels {
            for &m in &markets {
                for &measure in &measures {
                    let c = coord(
                        cube_id,
                        &refs,
                        refs.scen_baseline,
                        refs.ver_working,
                        t,
                        ch,
                        m,
                        measure,
                    );
                    let result = cube
                        .write(WritebackRequest {
                            coord: c,
                            new_value: ScalarValue::F64(1.0),
                            principal: refs.root_principal,
                            intent: WriteIntent::Set,
                            expected_revision: None,
                            now_unix_seconds: 0,
                        })
                        .expect("bulk write");
                    let now_dirty = cube.dirty().len();
                    assert!(
                        result.invalidated.len() <= PER_WRITE_DIRTY_BOUND,
                        "bulk write {writes_observed}: invalidated.len()={} exceeds brief \
                         §10.1 per-write bound {PER_WRITE_DIRTY_BOUND} \
                         (cumulative dirty before/after = {prior_dirty}/{now_dirty})",
                        result.invalidated.len()
                    );
                    assert!(
                        now_dirty >= prior_dirty,
                        "cumulative dirty set must not shrink during bulk-ingest \
                         ({prior_dirty} → {now_dirty})"
                    );
                    if result.invalidated.len() > max_invalidated_observed {
                        max_invalidated_observed = result.invalidated.len();
                    }
                    prior_dirty = now_dirty;
                    writes_observed += 1;
                }
            }
        }
    }

    assert_eq!(writes_observed, 2_520, "Acme bulk-ingest is 2,520 writes");
    assert!(
        prior_dirty > max_invalidated_observed,
        "after a 2,520-write bulk-ingest the cumulative dirty set ({prior_dirty}) \
         must exceed the largest single-write invalidated count \
         ({max_invalidated_observed}) — the latter stays bounded by §10.1 \
         while the former grows unbounded by construction"
    );
}

/// **Test E (smoke) — the demo-CLI's printed dirty count is the
/// marginal count, not the cumulative.**
///
/// Drives the same setup as `mc-cli demo`'s "Writing Spend ... " block:
/// load all 2,520 inputs, then write Spend at the test coord. Asserts
/// the resulting `invalidated.len()` is small (single- or double-digit,
/// well under the §10.1 bound), not the cumulative ~17 K+ that the
/// Phase 1A reading would have produced. Brief §4.6 says
/// "exact N depends on impl; bounded — see §8" — this asserts the
/// order-of-magnitude.
#[test]
fn t_phase_2d_write_e_demo_dirty_count_is_marginal() {
    let (mut cube, refs) = build_acme_cube().expect("build ok");
    write_canonical_inputs(&mut cube, &refs).expect("inputs");
    let cumulative_dirty_before = cube.dirty().len();
    assert!(
        cumulative_dirty_before > 1_000,
        "after canonical-input loading the cumulative dirty set should be much larger \
         than any single-write marginal count (got {cumulative_dirty_before})"
    );
    let cube_id = cube.id;
    let c = coord(
        cube_id,
        &refs,
        refs.scen_baseline,
        refs.ver_working,
        refs.mar_2026,
        refs.paid_search,
        refs.tampa,
        refs.spend,
    );
    let result = cube
        .write(WritebackRequest {
            coord: c,
            new_value: ScalarValue::F64(50_000.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("demo-style spend write");
    assert!(
        result.invalidated.len() <= PER_WRITE_DIRTY_BOUND,
        "demo spend write invalidated.len()={} exceeds the brief §10.1 per-write bound \
         {PER_WRITE_DIRTY_BOUND}",
        result.invalidated.len()
    );
    // The demo's expected output reads "9 dependent cells dirtied"
    // (or similar small count) — under the cumulative reading it
    // would have been > 17,000. Allow some slack: assert < 100 to
    // catch any future regression that re-introduces cumulative
    // collection.
    assert!(
        result.invalidated.len() < 100,
        "demo spend write invalidated.len()={} should be a small two-digit number under \
         the corrected marginal semantics; a much larger value indicates the Phase 1A \
         cumulative reading has been re-introduced",
        result.invalidated.len()
    );
}
