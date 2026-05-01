//! Permission scopes — grant-based capability checks for read/write
//! and friends.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.14 (minimal Phase 1) and
//! engine-semantics.md §17.
//!
//! The Acme demo runs entirely as the cube's root principal; the table
//! is exercised by integration tests (§10.6) that grant a non-root
//! principal a Florida-only Write scope and verify Atlanta writes are
//! rejected.
//!
//! No authentication. No identity provider. Authentication is the
//! caller's job — the engine receives a `PrincipalId` already verified.

use ahash::AHashMap;

use crate::coordinate::CellCoordinate;
use crate::dimension::Dimension;
use crate::id::{CubeId, DimensionId, ElementId, PrincipalId};

/// Bitfield of capabilities granted by a [`Grant`]. Per spec §17 the
/// granted capabilities are positive — there is no `deny` rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet(pub u32);

pub mod capability {
    pub const READ: u32 = 1 << 0;
    pub const WRITE: u32 = 1 << 1;
    pub const APPROVE: u32 = 1 << 2;
    pub const LOCK: u32 = 1 << 3;
    pub const UNLOCK: u32 = 1 << 4;
    pub const ADMIN: u32 = 1 << 5;
}

impl CapabilitySet {
    pub fn empty() -> Self {
        Self(0)
    }

    pub fn with(bits: u32) -> Self {
        Self(bits)
    }

    pub fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    pub fn add(&mut self, bit: u32) {
        self.0 |= bit;
    }
}

/// A subset-of-cells specifier shared by `PermissionTable`, `LockTable`,
/// and `SliceQuery` (per spec §20: "Scope — pattern over coordinates").
/// One binding per dim slot; absent dims = no constraint (treated as
/// `All`).
#[derive(Clone, Debug, Default)]
pub struct ScopePattern {
    pub bindings: AHashMap<DimensionId, ScopeBinding>,
}

#[derive(Clone, Debug)]
pub enum ScopeBinding {
    One(ElementId),
    Many(Vec<ElementId>),
    Subtree(ElementId),
    All,
}

impl ScopePattern {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, dim: DimensionId, binding: ScopeBinding) -> Self {
        self.bindings.insert(dim, binding);
        self
    }

    /// True iff `coord` matches every binding in this pattern. Absent
    /// dims are treated as `All`. `Subtree(root)` requires walking the
    /// dim's default hierarchy to test descent — see `dim_at` plumbing
    /// in `PermissionTable::check`.
    fn matches(&self, coord: &CellCoordinate, dims: &[Dimension]) -> bool {
        for (i, d) in dims.iter().enumerate() {
            let element = coord.element_at(i);
            let Some(binding) = self.bindings.get(&d.id) else {
                continue; // absent → All
            };
            if !binding_matches(binding, element, d) {
                return false;
            }
        }
        true
    }
}

/// Pattern-binding matching primitive shared by `permission.rs` and
/// `lock.rs`.
pub(crate) fn binding_matches(binding: &ScopeBinding, element: ElementId, dim: &Dimension) -> bool {
    match binding {
        ScopeBinding::One(target) => *target == element,
        ScopeBinding::Many(targets) => targets.contains(&element),
        ScopeBinding::Subtree(root) => element_is_in_subtree(*root, element, dim),
        ScopeBinding::All => true,
    }
}

pub(crate) fn element_is_in_subtree(
    root: ElementId,
    candidate: ElementId,
    dim: &Dimension,
) -> bool {
    if root == candidate {
        return true;
    }
    // Walk parent pointers from candidate up; true if we reach `root`.
    let h = dim.default_hierarchy();
    let mut cur = candidate;
    while let Some(&parent) = h.parent_of.get(&cur) {
        if parent == root {
            return true;
        }
        cur = parent;
    }
    false
}

#[derive(Clone, Debug)]
pub struct Grant {
    pub principal: PrincipalId,
    pub pattern: ScopePattern,
    pub capabilities: CapabilitySet,
}

#[derive(Debug)]
pub struct PermissionTable {
    cube: CubeId,
    grants: Vec<Grant>,
    /// Phase 1: the cube's root principal has full access on every
    /// coordinate. The integration auth layer (Phase 3+) replaces this
    /// with a real identity binding.
    root_principal: PrincipalId,
}

impl PermissionTable {
    pub fn new(cube: CubeId, root: PrincipalId) -> Self {
        Self {
            cube,
            grants: Vec::new(),
            root_principal: root,
        }
    }

    pub fn cube(&self) -> CubeId {
        self.cube
    }

    pub fn root_principal(&self) -> PrincipalId {
        self.root_principal
    }

    pub fn grant(&mut self, grant: Grant) {
        self.grants.push(grant);
    }

    /// Per engine-semantics.md §17 I-Perm-1: every read/write checks
    /// permissions before completing. Returns `true` iff `principal` has
    /// the requested `capability_bit` on `coord` either by being the
    /// root principal or by virtue of one of the registered grants.
    pub fn check(
        &self,
        principal: PrincipalId,
        dims: &[Dimension],
        coord: &CellCoordinate,
        capability_bit: u32,
    ) -> bool {
        if principal == self.root_principal {
            return true;
        }
        for g in &self.grants {
            if g.principal != principal {
                continue;
            }
            if !g.capabilities.has(capability_bit) {
                continue;
            }
            if g.pattern.matches(coord, dims) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dimension::{Dimension, DimensionKind};
    use crate::element::Element;
    use crate::hierarchy::Hierarchy;
    use crate::id::{CubeId, DimensionId, ElementId, IdGenerator};

    /// Build a small two-dim cube fixture: one Market dim
    /// (USA → {Florida, Georgia} → cities), one Measure dim. Just enough
    /// to exercise scope-binding matching including Subtree.
    ///
    /// Returns `(dims, tampa, atlanta, florida, measure, market_dim_id)`.
    fn fixture() -> (
        Vec<Dimension>,
        ElementId,
        ElementId,
        ElementId,
        ElementId,
        DimensionId,
    ) {
        let g = IdGenerator::new();
        let market_dim_id = g.dimension();
        let measure_dim_id = g.dimension();
        let usa = g.element();
        let florida = g.element();
        let georgia = g.element();
        let tampa = g.element();
        let atlanta = g.element();
        let measure = g.element();

        let market_h = Hierarchy::builder(g.hierarchy(), "geo", market_dim_id)
            .add_edge(usa, florida, 1.0)
            .add_edge(usa, georgia, 1.0)
            .add_edge(florida, tampa, 1.0)
            .add_edge(georgia, atlanta, 1.0)
            .build()
            .expect("hier");

        let market_dim = Dimension::builder(market_dim_id, "Market", DimensionKind::Standard)
            .add_element(Element::leaf(usa, "USA", market_dim_id))
            .expect("ok")
            .add_element(Element::leaf(florida, "Florida", market_dim_id))
            .expect("ok")
            .add_element(Element::leaf(georgia, "Georgia", market_dim_id))
            .expect("ok")
            .add_element(Element::leaf(tampa, "Tampa", market_dim_id))
            .expect("ok")
            .add_element(Element::leaf(atlanta, "Atlanta", market_dim_id))
            .expect("ok")
            .add_hierarchy(market_h)
            .expect("ok")
            .default_hierarchy("geo")
            .build()
            .expect("dim");

        let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
            .add_element(Element::leaf(measure, "Spend", measure_dim_id))
            .expect("ok")
            .build()
            .expect("dim");
        let dims = vec![market_dim, measure_dim];
        (dims, tampa, atlanta, florida, measure, market_dim_id)
    }

    #[test]
    fn root_principal_has_full_access() {
        let (dims, tampa, _atlanta, _florida, measure, _market_dim_id) = fixture();
        let mut table = PermissionTable::new(CubeId(1), PrincipalId(1));
        let coord = CellCoordinate::from_parts(CubeId(1), [tampa, measure]);
        assert!(table.check(PrincipalId(1), &dims, &coord, capability::READ));
        assert!(table.check(PrincipalId(1), &dims, &coord, capability::WRITE));
        // Even ADMIN, which we never granted explicitly.
        assert!(table.check(PrincipalId(1), &dims, &coord, capability::ADMIN));

        // No-op grant table only matters for non-root.
        table.grant(Grant {
            principal: PrincipalId(99),
            pattern: ScopePattern::new(),
            capabilities: CapabilitySet::with(capability::READ),
        });
    }

    #[test]
    fn non_root_with_no_grant_cannot_read() {
        let (dims, tampa, _atlanta, _florida, measure, _market_dim_id) = fixture();
        let table = PermissionTable::new(CubeId(1), PrincipalId(1));
        let coord = CellCoordinate::from_parts(CubeId(1), [tampa, measure]);
        assert!(!table.check(PrincipalId(99), &dims, &coord, capability::READ));
    }

    #[test]
    fn subtree_binding_grants_descendants() {
        let (dims, tampa, atlanta, florida, measure, market_dim_id) = fixture();
        let mut table = PermissionTable::new(CubeId(1), PrincipalId(1));
        // Grant: PrincipalId(99) can write within the Florida subtree.
        table.grant(Grant {
            principal: PrincipalId(99),
            pattern: ScopePattern::new().with(market_dim_id, ScopeBinding::Subtree(florida)),
            capabilities: CapabilitySet::with(capability::WRITE),
        });
        let tampa_coord = CellCoordinate::from_parts(CubeId(1), [tampa, measure]);
        let atlanta_coord = CellCoordinate::from_parts(CubeId(1), [atlanta, measure]);
        assert!(table.check(PrincipalId(99), &dims, &tampa_coord, capability::WRITE));
        assert!(!table.check(PrincipalId(99), &dims, &atlanta_coord, capability::WRITE));
        // Even within Florida, the grant is WRITE only — READ should fail.
        assert!(!table.check(PrincipalId(99), &dims, &tampa_coord, capability::READ));
    }

    #[test]
    fn capability_set_helpers() {
        let mut s = CapabilitySet::empty();
        assert!(!s.has(capability::READ));
        s.add(capability::READ);
        s.add(capability::WRITE);
        assert!(s.has(capability::READ));
        assert!(s.has(capability::WRITE));
        assert!(!s.has(capability::APPROVE));
    }

    #[test]
    fn missing_dim_in_pattern_treated_as_all() {
        // An empty ScopePattern matches every coord. Useful for "this
        // principal can do X anywhere."
        let (dims, tampa, _atlanta, _florida, measure, _market_dim_id) = fixture();
        let mut table = PermissionTable::new(CubeId(1), PrincipalId(1));
        table.grant(Grant {
            principal: PrincipalId(99),
            pattern: ScopePattern::new(), // no bindings → matches all
            capabilities: CapabilitySet::with(capability::READ),
        });
        let coord = CellCoordinate::from_parts(CubeId(1), [tampa, measure]);
        assert!(table.check(PrincipalId(99), &dims, &coord, capability::READ));
    }
}
