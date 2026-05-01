//! Newtype identifiers + monotonic ID generator.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.1.

use std::cell::Cell;

/// Stable, unique within a single workspace.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct CubeId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct DimensionId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ElementId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct HierarchyId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct RuleId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct PrincipalId(pub u64);

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct LockId(pub u64);

/// Monotonic per-cube revision counter.
///
/// Bumps on every successful write. Used by dirty tracking, snapshots, and
/// optimistic concurrency. Per spec §20 (Cross-cutting glossary).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Revision(pub u64);

impl Revision {
    pub const ZERO: Self = Revision(0);

    /// Returns the next revision in sequence. Does not mutate self.
    #[inline]
    pub fn next(self) -> Self {
        Revision(self.0 + 1)
    }
}

/// Allocates monotonically-increasing IDs for every kind of entity in the
/// engine.
///
/// Phase 1 is single-threaded; the counters are plain `Cell<u64>`.
/// Phase 2+ swaps these for `AtomicU64`.
///
/// Per spec §3.1.
#[derive(Debug, Default)]
pub struct IdGenerator {
    cube: Cell<u64>,
    dimension: Cell<u64>,
    element: Cell<u64>,
    hierarchy: Cell<u64>,
    rule: Cell<u64>,
    principal: Cell<u64>,
    lock: Cell<u64>,
}

impl IdGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Internal helper: bump a counter and return the new value.
    fn bump(counter: &Cell<u64>) -> u64 {
        let next = counter.get() + 1;
        counter.set(next);
        next
    }

    pub fn cube(&self) -> CubeId {
        CubeId(Self::bump(&self.cube))
    }

    pub fn dimension(&self) -> DimensionId {
        DimensionId(Self::bump(&self.dimension))
    }

    pub fn element(&self) -> ElementId {
        ElementId(Self::bump(&self.element))
    }

    pub fn hierarchy(&self) -> HierarchyId {
        HierarchyId(Self::bump(&self.hierarchy))
    }

    pub fn rule(&self) -> RuleId {
        RuleId(Self::bump(&self.rule))
    }

    pub fn principal(&self) -> PrincipalId {
        PrincipalId(Self::bump(&self.principal))
    }

    pub fn lock(&self) -> LockId {
        LockId(Self::bump(&self.lock))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_zero_is_zero() {
        assert_eq!(Revision::ZERO, Revision(0));
    }

    #[test]
    fn revision_next_is_strictly_monotonic() {
        let r0 = Revision(7);
        let r1 = r0.next();
        let r2 = r1.next();
        assert_eq!(r1, Revision(8));
        assert_eq!(r2, Revision(9));
        assert!(r1 > r0);
        assert!(r2 > r1);
    }

    #[test]
    fn id_generator_yields_distinct_ids_per_kind() {
        let g = IdGenerator::new();
        let c1 = g.cube();
        let c2 = g.cube();
        assert_ne!(c1, c2);
        assert_eq!(c1, CubeId(1));
        assert_eq!(c2, CubeId(2));
    }

    #[test]
    fn id_generator_kinds_are_independent() {
        // Per spec §2 I-Dim-1 / I-Cube-1: IDs of different kinds are
        // independent counters; CubeId(1) and DimensionId(1) co-exist.
        let g = IdGenerator::new();
        let cube = g.cube();
        let dim = g.dimension();
        let elem = g.element();
        assert_eq!(cube.0, 1);
        assert_eq!(dim.0, 1);
        assert_eq!(elem.0, 1);
    }

    #[test]
    fn id_types_implement_required_traits() {
        // Hashable + Ord — these are load-bearing for HashMap and BTreeSet
        // usage downstream.
        use std::collections::HashSet;
        let mut s: HashSet<ElementId> = HashSet::new();
        s.insert(ElementId(1));
        s.insert(ElementId(2));
        s.insert(ElementId(1));
        assert_eq!(s.len(), 2);
    }
}
