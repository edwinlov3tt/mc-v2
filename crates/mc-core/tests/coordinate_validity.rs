//! Coordinate validity tests for `CellCoordinate` + `CellCoordinateBuilder`.
//!
//! Pin spec §6 I-Coord-1..6 and the build-brief §3.7 contract.

use mc_core::{
    CellCoordinate, CellCoordinateBuilder, CubeId, Dimension, DimensionId, DimensionKind, Element,
    ElementId, EngineError,
};

fn build_simple_dims() -> (
    Vec<Dimension>,
    DimensionId,
    DimensionId,
    ElementId,
    ElementId,
) {
    // Two dims: D1 with elements {E1=1, E2=2}; D2 with elements {E3=3}.
    let d1_id = DimensionId(10);
    let d2_id = DimensionId(20);
    let e1 = ElementId(1);
    let e2 = ElementId(2);
    let e3 = ElementId(3);
    let d1 = Dimension::builder(d1_id, "D1", DimensionKind::Standard)
        .add_element(Element::leaf(e1, "E1", d1_id))
        .expect("ok")
        .add_element(Element::leaf(e2, "E2", d1_id))
        .expect("ok")
        .build()
        .expect("build d1");
    let d2 = Dimension::builder(d2_id, "D2", DimensionKind::Standard)
        .add_element(Element::leaf(e3, "E3", d2_id))
        .expect("ok")
        .build()
        .expect("build d2");
    (vec![d1, d2], d1_id, d2_id, e1, e3)
}

#[test]
fn build_with_all_slots_set_succeeds() {
    let cube = CubeId(99);
    let (dims, d1_id, d2_id, e1, e3) = build_simple_dims();
    let coord = CellCoordinateBuilder::new(cube, &dims)
        .set(d1_id, e1)
        .expect("d1 ok")
        .set(d2_id, e3)
        .expect("d2 ok")
        .build()
        .expect("build ok");
    assert_eq!(coord.cube, cube);
    assert_eq!(coord.arity(), 2);
    assert_eq!(coord.elements(), &[e1, e3]);
}

#[test]
fn build_with_missing_slot_rejected() {
    let cube = CubeId(99);
    let (dims, d1_id, _d2_id, e1, _e3) = build_simple_dims();
    let err = CellCoordinateBuilder::new(cube, &dims)
        .set(d1_id, e1)
        .expect("d1 ok")
        .build()
        .expect_err("missing d2 slot must be rejected");
    assert!(matches!(err, EngineError::CoordinateMissingDimension(_)));
}

#[test]
fn unknown_dimension_rejected() {
    let cube = CubeId(99);
    let (dims, _d1_id, _d2_id, e1, _e3) = build_simple_dims();
    // DimensionId(999) is not in the dims slice.
    let err = CellCoordinateBuilder::new(cube, &dims)
        .set(DimensionId(999), e1)
        .expect_err("unknown dim must be rejected");
    assert!(matches!(
        err,
        EngineError::CoordinateDimensionNotInCube { dim } if dim == DimensionId(999)
    ));
}

#[test]
fn element_not_in_dimension_rejected() {
    let cube = CubeId(99);
    let (dims, d1_id, _d2_id, _e1, _e3) = build_simple_dims();
    // ElementId(999) doesn't exist in d1.
    let err = CellCoordinateBuilder::new(cube, &dims)
        .set(d1_id, ElementId(999))
        .expect_err("unknown element must be rejected");
    assert!(matches!(
        err,
        EngineError::CoordinateElementNotInDimension { element, dim }
        if element == ElementId(999) && dim == d1_id
    ));
}

#[test]
fn element_from_other_dimension_rejected() {
    // E3 belongs to D2; setting it on D1 must be rejected.
    let cube = CubeId(99);
    let (dims, d1_id, _d2_id, _e1, e3) = build_simple_dims();
    let err = CellCoordinateBuilder::new(cube, &dims)
        .set(d1_id, e3)
        .expect_err("element from other dim must be rejected");
    assert!(matches!(
        err,
        EngineError::CoordinateElementNotInDimension { element, dim }
        if element == e3 && dim == d1_id
    ));
}

#[test]
fn equal_coords_compare_equal_and_hash_equal() {
    use std::collections::HashSet;
    let cube = CubeId(99);
    let (dims, d1_id, d2_id, e1, e3) = build_simple_dims();
    let coord_a = CellCoordinateBuilder::new(cube, &dims)
        .set(d1_id, e1)
        .expect("ok")
        .set(d2_id, e3)
        .expect("ok")
        .build()
        .expect("ok");
    let coord_b = CellCoordinateBuilder::new(cube, &dims)
        // Set in reverse order — must produce the same coord.
        .set(d2_id, e3)
        .expect("ok")
        .set(d1_id, e1)
        .expect("ok")
        .build()
        .expect("ok");
    assert_eq!(coord_a, coord_b);
    let mut s: HashSet<CellCoordinate> = HashSet::new();
    s.insert(coord_a.clone());
    s.insert(coord_b);
    assert_eq!(s.len(), 1, "equal coords must hash to the same bucket");
}

#[test]
fn coords_from_different_cubes_are_distinct() {
    // Per spec §6 I-Coord-6: coords from different cubes are never equal.
    let (dims, d1_id, d2_id, e1, e3) = build_simple_dims();
    let cube_a = CubeId(1);
    let cube_b = CubeId(2);
    let a = CellCoordinateBuilder::new(cube_a, &dims)
        .set(d1_id, e1)
        .expect("ok")
        .set(d2_id, e3)
        .expect("ok")
        .build()
        .expect("ok");
    let b = CellCoordinateBuilder::new(cube_b, &dims)
        .set(d1_id, e1)
        .expect("ok")
        .set(d2_id, e3)
        .expect("ok")
        .build()
        .expect("ok");
    assert_ne!(a, b);
}

#[test]
fn with_element_replaces_one_slot() {
    let cube = CubeId(99);
    let (dims, d1_id, d2_id, e1, e3) = build_simple_dims();
    let original = CellCoordinateBuilder::new(cube, &dims)
        .set(d1_id, e1)
        .expect("ok")
        .set(d2_id, e3)
        .expect("ok")
        .build()
        .expect("ok");
    let updated = original.with_element(0, ElementId(2));
    assert_eq!(updated.element_at(0), ElementId(2));
    assert_eq!(updated.element_at(1), e3);
    // Original is unchanged (Clone semantics).
    assert_eq!(original.element_at(0), e1);
}

#[test]
fn from_parts_preserves_order() {
    let coord = CellCoordinate::from_parts(CubeId(1), [ElementId(7), ElementId(8), ElementId(9)]);
    assert_eq!(coord.arity(), 3);
    assert_eq!(
        coord.elements(),
        &[ElementId(7), ElementId(8), ElementId(9)]
    );
}
