//! Dimensions: named, ordered sets of elements with optional hierarchies.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.5.

use ahash::AHashMap;

use crate::element::Element;
use crate::error::EngineError;
use crate::hierarchy::Hierarchy;
use crate::id::{DimensionId, ElementId, HierarchyId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DimensionKind {
    Standard,
    Measure,
    Scenario,
    Version,
}

/// `Clone` is enabled on Phase 1 because `cube.rs::read_consolidated`
/// needs to clone the dimensions slice during a recursive read to avoid
/// holding a `&Cube` borrow across child reads (per kickoff Rule 5).
/// Phase 2 optimization (deferred per §0.A bench gate) can replace this
/// with a `Cow` or by indexing dimensions positionally; the clone is
/// cheap on Acme-scale dims (≤ 50 elements / hierarchy entries each).
#[derive(Clone, Debug)]
pub struct Dimension {
    pub id: DimensionId,
    pub name: String,
    pub kind: DimensionKind,
    pub elements: Vec<Element>,
    pub element_index: AHashMap<ElementId, usize>,
    pub element_by_name: AHashMap<String, ElementId>,
    pub hierarchies: Vec<Hierarchy>,
    pub default_hierarchy: HierarchyId,
    is_frozen: bool,
}

impl Dimension {
    pub fn builder(
        id: DimensionId,
        name: impl Into<String>,
        kind: DimensionKind,
    ) -> DimensionBuilder {
        DimensionBuilder {
            id,
            name: name.into(),
            kind,
            elements: Vec::new(),
            element_index: AHashMap::new(),
            element_by_name: AHashMap::new(),
            hierarchies: Vec::new(),
            default_hierarchy_name: None,
        }
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.element_index.get(&id).map(|&pos| &self.elements[pos])
    }

    pub fn element_by_name(&self, name: &str) -> Option<&Element> {
        self.element_by_name
            .get(name)
            .and_then(|id| self.element(*id))
    }

    pub fn position(&self, id: ElementId) -> Option<usize> {
        self.element_index.get(&id).copied()
    }

    pub fn contains_element(&self, id: ElementId) -> bool {
        self.element_index.contains_key(&id)
    }

    pub fn hierarchy(&self, id: HierarchyId) -> Option<&Hierarchy> {
        self.hierarchies.iter().find(|h| h.id == id)
    }

    pub fn default_hierarchy(&self) -> &Hierarchy {
        // Per spec §2 I-Dim-4: default_hierarchy is one of the entries in
        // hierarchies. Validated at `DimensionBuilder::build` time, so the
        // None branch is unreachable for any well-formed `Dimension`.
        match self.hierarchy(self.default_hierarchy) {
            Some(h) => h,
            None => unreachable!(
                "Internal invariant violated: DimensionBuilder::build \
                 ensures default_hierarchy references a member of \
                 self.hierarchies. See spec §2 I-Dim-4."
            ),
        }
    }

    pub fn is_measure_dimension(&self) -> bool {
        self.kind == DimensionKind::Measure
    }

    pub fn is_frozen(&self) -> bool {
        self.is_frozen
    }

    /// Internal: flips the frozen flag when this dimension is bound to a
    /// cube. Per spec §2 I-Dim-5: post-freeze mutation is rejected. Phase 1
    /// has no append-after-freeze API at all; the flag exists for forward
    /// compatibility and so tests can assert it.
    ///
    /// `#[allow(dead_code)]` until `cube.rs` lands and `Cube::build` calls
    /// this on every dimension it absorbs.
    #[allow(dead_code)]
    pub(crate) fn freeze(&mut self) {
        self.is_frozen = true;
    }
}

#[derive(Debug)]
pub struct DimensionBuilder {
    id: DimensionId,
    name: String,
    kind: DimensionKind,
    elements: Vec<Element>,
    element_index: AHashMap<ElementId, usize>,
    element_by_name: AHashMap<String, ElementId>,
    hierarchies: Vec<Hierarchy>,
    default_hierarchy_name: Option<String>,
}

impl DimensionBuilder {
    /// Add a leaf element. For measure / version / scenario kinds, prefer
    /// the typed helpers below.
    pub fn add_element(mut self, element: Element) -> Result<Self, EngineError> {
        let id = element.id;
        let name = element.name.clone();

        if self.element_index.contains_key(&id) {
            return Err(EngineError::DuplicateElementId { id, dim: self.id });
        }
        if self.element_by_name.contains_key(&name) {
            return Err(EngineError::DuplicateElementName {
                name: name.clone(),
                dim: self.id,
            });
        }

        let position = self.elements.len();
        self.element_index.insert(id, position);
        self.element_by_name.insert(name, id);
        self.elements.push(element);
        Ok(self)
    }

    pub fn add_hierarchy(mut self, hierarchy: Hierarchy) -> Result<Self, EngineError> {
        // Verify every edge endpoint is a member of this dimension. This is
        // the check the hierarchy module deliberately doesn't do (see
        // hierarchy.rs module doc).
        for edge in &hierarchy.edges {
            if !self.element_index.contains_key(&edge.parent) {
                return Err(EngineError::HierarchyEdgeReferencesUnknownElement {
                    id: edge.parent,
                    dim: self.id,
                });
            }
            if !self.element_index.contains_key(&edge.child) {
                return Err(EngineError::HierarchyEdgeReferencesUnknownElement {
                    id: edge.child,
                    dim: self.id,
                });
            }
        }
        // Reject duplicate hierarchy names within a dimension; users
        // identify default_hierarchy by name in the builder API.
        if self.hierarchies.iter().any(|h| h.name == hierarchy.name) {
            return Err(EngineError::DuplicateElementName {
                name: hierarchy.name,
                dim: self.id,
            });
        }
        self.hierarchies.push(hierarchy);
        Ok(self)
    }

    pub fn default_hierarchy(mut self, name: impl Into<String>) -> Self {
        self.default_hierarchy_name = Some(name.into());
        self
    }

    pub fn build(self) -> Result<Dimension, EngineError> {
        let DimensionBuilder {
            id,
            name,
            kind,
            elements,
            element_index,
            element_by_name,
            hierarchies,
            default_hierarchy_name,
        } = self;

        if elements.is_empty() {
            return Err(EngineError::DimensionEmpty { name: name.clone() });
        }

        // Resolve default hierarchy by name. If the dimension has no
        // hierarchies declared, synthesize a flat hierarchy with no edges so
        // every element is a leaf and operations that expect a default
        // hierarchy don't break.
        let (hierarchies, default_id) = if hierarchies.is_empty() {
            // Synthesize a flat default. The synthesized hierarchy has no
            // edges; `Hierarchy::is_leaf` returns false for an element that
            // doesn't appear in `leaves`, but consumers that need a flat
            // hierarchy treat "not consolidated" as leaf via Dimension::position.
            // For Phase 1 we keep the synthesized hierarchy minimal: empty
            // edges, empty leaves/consolidated/children_of/parent_of, and
            // `roots` empty as well. Consumers should special-case
            // `hierarchies.is_empty_synthesized()` style logic in Phase 2 if
            // needed; today, all Acme dims that need hierarchies declare them.
            let synth = Hierarchy {
                id: HierarchyId(0),
                name: format!("{}_flat", name),
                dimension: id,
                edges: Vec::new(),
                roots: Vec::new(),
                leaves: ahash::AHashSet::new(),
                consolidated: ahash::AHashSet::new(),
                children_of: AHashMap::new(),
                parent_of: AHashMap::new(),
            };
            let id_ = synth.id;
            (vec![synth], id_)
        } else if let Some(default_name) = default_hierarchy_name {
            let resolved = hierarchies
                .iter()
                .find(|h| h.name == default_name)
                .map(|h| h.id)
                .ok_or(EngineError::DefaultHierarchyNotFound { name: default_name })?;
            (hierarchies, resolved)
        } else {
            // No explicit default given; pick the first hierarchy declared.
            let first_id = hierarchies[0].id;
            (hierarchies, first_id)
        };

        Ok(Dimension {
            id,
            name,
            kind,
            elements,
            element_index,
            element_by_name,
            hierarchies,
            default_hierarchy: default_id,
            is_frozen: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Element;

    #[test]
    fn build_simple_three_element_dimension() {
        let dim_id = DimensionId(7);
        let e1 = ElementId(1);
        let e2 = ElementId(2);
        let e3 = ElementId(3);
        let dim = Dimension::builder(dim_id, "Time", DimensionKind::Standard)
            .add_element(Element::leaf(e1, "Jan", dim_id))
            .expect("element add")
            .add_element(Element::leaf(e2, "Feb", dim_id))
            .expect("element add")
            .add_element(Element::leaf(e3, "Mar", dim_id))
            .expect("element add")
            .build()
            .expect("dim build");
        assert_eq!(dim.elements.len(), 3);
        assert_eq!(dim.position(e1), Some(0));
        assert_eq!(dim.position(e2), Some(1));
        assert_eq!(dim.position(e3), Some(2));
        assert_eq!(dim.element_by_name("Feb").map(|e| e.id), Some(e2));
    }
}
