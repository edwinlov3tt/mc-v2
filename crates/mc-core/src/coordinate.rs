//! Cell coordinates: fully-qualified addresses into a cube.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.7.
//!
//! A coordinate binds one element from each of the cube's dimensions in the
//! cube's dimension order. Equality and hashing are O(D) over the element
//! slice; the cube's `CubeId` disambiguates coordinates across cubes.
//!
//! In the brief the builder takes `&Cube`. This module's builder accepts
//! `&[Dimension]` directly so the type can be tested before `cube.rs` lands.
//! When `cube.rs` arrives, `Cube::coordinate_builder(&self) ->
//! CellCoordinateBuilder<'_>` will simply pass `(self.id, &self.dimensions)`
//! through to `CellCoordinateBuilder::new`.

use smallvec::SmallVec;

use crate::dimension::Dimension;
use crate::error::EngineError;
use crate::id::{CubeId, DimensionId, ElementId};

/// A fully-qualified cell address. Equality and hashing depend only on
/// `cube` and the element slice.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CellCoordinate {
    pub cube: CubeId,
    elements: SmallVec<[ElementId; 8]>,
}

impl CellCoordinate {
    /// Construct directly from a cube id and an element slice. This is the
    /// low-level constructor; prefer `CellCoordinateBuilder` for validation.
    /// The slice MUST be in the parent cube's dimension order; this
    /// constructor performs no validation.
    pub fn from_parts(cube: CubeId, elements: impl IntoIterator<Item = ElementId>) -> Self {
        Self {
            cube,
            elements: elements.into_iter().collect(),
        }
    }

    pub fn elements(&self) -> &[ElementId] {
        &self.elements
    }

    pub fn element_at(&self, dim_position: usize) -> ElementId {
        self.elements[dim_position]
    }

    /// Return a new coordinate with the element at `dim_position` replaced.
    /// Panics in debug if `dim_position` is out of bounds; production builds
    /// silently ignore out-of-bounds (consistent with `slice::get_unchecked`
    /// risk avoidance — callers must check first).
    pub fn with_element(&self, dim_position: usize, e: ElementId) -> CellCoordinate {
        debug_assert!(dim_position < self.elements.len());
        let mut next: SmallVec<[ElementId; 8]> = self.elements.clone();
        if dim_position < next.len() {
            next[dim_position] = e;
        }
        CellCoordinate {
            cube: self.cube,
            elements: next,
        }
    }

    pub fn arity(&self) -> usize {
        self.elements.len()
    }
}

/// Builder that validates element membership against the cube's dimension
/// list as elements are bound. See module docs for the rationale on why this
/// takes `&[Dimension]` directly in Phase 1.
#[derive(Debug)]
pub struct CellCoordinateBuilder<'cube> {
    cube: CubeId,
    dimensions: &'cube [Dimension],
    /// One slot per dimension, indexed by position in `dimensions`.
    slots: Vec<Option<ElementId>>,
}

impl<'cube> CellCoordinateBuilder<'cube> {
    pub fn new(cube: CubeId, dimensions: &'cube [Dimension]) -> Self {
        Self {
            cube,
            dimensions,
            slots: vec![None; dimensions.len()],
        }
    }

    /// Bind `element` to the given dimension. Validates that the dimension
    /// is part of this cube and that the element is a member.
    pub fn set(mut self, dim: DimensionId, element: ElementId) -> Result<Self, EngineError> {
        let position = self
            .dimensions
            .iter()
            .position(|d| d.id == dim)
            .ok_or(EngineError::CoordinateDimensionNotInCube { dim })?;
        let dimension = &self.dimensions[position];
        if !dimension.contains_element(element) {
            return Err(EngineError::CoordinateElementNotInDimension { element, dim });
        }
        self.slots[position] = Some(element);
        Ok(self)
    }

    /// Convenience setter that resolves dim and element by name.
    pub fn set_by_name(self, dim_name: &str, element_name: &str) -> Result<Self, EngineError> {
        // Resolve outside `set` so the error path is precise.
        let dimension = self
            .dimensions
            .iter()
            .find(|d| d.name == dim_name)
            .ok_or_else(|| EngineError::DimensionNotFound {
                name: dim_name.to_string(),
            })?;
        let element = dimension.element_by_name(element_name).ok_or_else(|| {
            EngineError::ElementNotFound(
                ElementId(0), // placeholder; the lookup failed by name
                dimension.id,
            )
        })?;
        self.set(dimension.id, element.id)
    }

    /// Build the coordinate. Returns
    /// `EngineError::CoordinateMissingDimension(dim)` for the FIRST unset
    /// slot. Determinism: dimension-position order is the cube's declared
    /// dimension order, so the same builder + same setters produce the same
    /// error for incomplete builders.
    pub fn build(self) -> Result<CellCoordinate, EngineError> {
        let mut elements: SmallVec<[ElementId; 8]> = SmallVec::with_capacity(self.slots.len());
        for (position, slot) in self.slots.iter().enumerate() {
            match slot {
                Some(id) => elements.push(*id),
                None => {
                    return Err(EngineError::CoordinateMissingDimension(
                        self.dimensions[position].id,
                    ))
                }
            }
        }
        Ok(CellCoordinate {
            cube: self.cube,
            elements,
        })
    }
}
