//! Duplicate element detection tests for `Dimension`.
//!
//! Pin spec §2 I-Dim-1 (no duplicate ElementIds) and the brief §3.5
//! contract that duplicate names are also rejected.

use mc_core::{Dimension, DimensionId, DimensionKind, Element, ElementId, EngineError};

fn dim_id() -> DimensionId {
    DimensionId(1)
}

#[test]
fn duplicate_element_id_rejected() {
    // Two elements with the same ElementId — invalid.
    let id = ElementId(42);
    let err = Dimension::builder(dim_id(), "Time", DimensionKind::Standard)
        .add_element(Element::leaf(id, "Jan", dim_id()))
        .expect("first element ok")
        .add_element(Element::leaf(id, "Feb", dim_id()))
        .expect_err("duplicate id must be rejected");
    assert!(matches!(
        err,
        EngineError::DuplicateElementId { id: dup, .. } if dup == id
    ));
}

#[test]
fn duplicate_element_name_rejected() {
    // Two elements with the same name — invalid (per build brief §3.5).
    let err = Dimension::builder(dim_id(), "Time", DimensionKind::Standard)
        .add_element(Element::leaf(ElementId(1), "Jan", dim_id()))
        .expect("first element ok")
        .add_element(Element::leaf(ElementId(2), "Jan", dim_id()))
        .expect_err("duplicate name must be rejected");
    match err {
        EngineError::DuplicateElementName { name, .. } => assert_eq!(name, "Jan"),
        other => panic!("expected DuplicateElementName, got {other:?}"),
    }
}

#[test]
fn distinct_ids_and_names_succeed() {
    let dim = Dimension::builder(dim_id(), "Time", DimensionKind::Standard)
        .add_element(Element::leaf(ElementId(1), "Jan", dim_id()))
        .expect("ok")
        .add_element(Element::leaf(ElementId(2), "Feb", dim_id()))
        .expect("ok")
        .add_element(Element::leaf(ElementId(3), "Mar", dim_id()))
        .expect("ok")
        .build()
        .expect("build ok");
    assert_eq!(dim.elements.len(), 3);
    assert_eq!(dim.element_by_name("Feb").map(|e| e.id), Some(ElementId(2)));
    assert_eq!(dim.position(ElementId(3)), Some(2));
}

#[test]
fn empty_dimension_rejected() {
    let err = Dimension::builder(dim_id(), "Empty", DimensionKind::Standard)
        .build()
        .expect_err("dimension with zero elements is invalid");
    assert!(matches!(err, EngineError::DimensionEmpty { .. }));
}

#[test]
fn lookup_by_unknown_name_returns_none() {
    let dim = Dimension::builder(dim_id(), "Time", DimensionKind::Standard)
        .add_element(Element::leaf(ElementId(1), "Jan", dim_id()))
        .expect("ok")
        .build()
        .expect("build ok");
    assert!(dim.element_by_name("December").is_none());
    assert!(dim.element(ElementId(999)).is_none());
}

#[test]
fn elements_preserve_insertion_order() {
    // Per spec §2 I-Dim-1: elements have stable insertion order.
    let dim = Dimension::builder(dim_id(), "Time", DimensionKind::Standard)
        .add_element(Element::leaf(ElementId(10), "Mar", dim_id()))
        .expect("ok")
        .add_element(Element::leaf(ElementId(20), "Jan", dim_id()))
        .expect("ok")
        .add_element(Element::leaf(ElementId(30), "Feb", dim_id()))
        .expect("ok")
        .build()
        .expect("build ok");
    assert_eq!(dim.elements[0].name, "Mar");
    assert_eq!(dim.elements[1].name, "Jan");
    assert_eq!(dim.elements[2].name, "Feb");
}
