//! Locks: scoped advisories or hard write-blocks.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.15 (minimal Phase 1) and
//! engine-semantics.md §18.
//!
//! Locks share the `ScopePattern` shape with permissions. A `Hard` lock
//! blocks writes to its scoped cells by everyone except the owner; a
//! `Soft` lock allows the write but surfaces the lock's note as an
//! advisory in the writeback result.
//!
//! Locks have a mandatory `expires_at` per spec §18 I-Lock-4. The cube
//! purges expired locks lazily on the next conflict check
//! (`purge_expired`).

use crate::coordinate::CellCoordinate;
use crate::dimension::Dimension;
use crate::id::{CubeId, LockId, PrincipalId};
use crate::permission::ScopePattern;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockKind {
    /// Advisory: writes by other principals succeed but produce a
    /// non-fatal `WritebackResult::soft_lock_notes` entry.
    Soft,
    /// Enforced: writes by other principals are rejected with
    /// `EngineError::LockedCell`.
    Hard,
}

#[derive(Clone, Debug)]
pub struct Lock {
    pub id: LockId,
    pub owner: PrincipalId,
    pub pattern: ScopePattern,
    pub kind: LockKind,
    pub acquired_at: u64,
    /// Per spec §18 I-Lock-4: expiration is mandatory. Stored as Unix
    /// seconds. The lock is silently dropped at the next
    /// `check_write` / `purge_expired` after `now >= expires_at`.
    pub expires_at: u64,
    pub note: Option<String>,
}

#[derive(Debug)]
pub struct LockTable {
    cube: CubeId,
    locks: Vec<Lock>,
}

impl LockTable {
    pub fn new(cube: CubeId) -> Self {
        Self {
            cube,
            locks: Vec::new(),
        }
    }

    pub fn cube(&self) -> CubeId {
        self.cube
    }

    pub fn len(&self) -> usize {
        self.locks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.locks.is_empty()
    }

    /// Acquire a new lock. Per spec §18 I-Lock-1: a Hard lock that
    /// overlaps an existing Hard lock owned by a different principal is
    /// rejected. (Phase 1 does NOT pre-validate that `principal` has the
    /// `LOCK` capability — that check belongs in the cube layer where
    /// the permission table is in scope.)
    ///
    /// Returns the new lock's id on success.
    pub fn acquire(&mut self, lock: Lock, dims: &[Dimension]) -> Result<LockId, ConflictKind> {
        // Lazily purge so the conflict scan only sees live locks.
        self.purge_expired(lock.acquired_at);

        if matches!(lock.kind, LockKind::Hard) {
            for existing in &self.locks {
                if matches!(existing.kind, LockKind::Hard)
                    && existing.owner != lock.owner
                    && patterns_overlap(&existing.pattern, &lock.pattern, dims)
                {
                    return Err(ConflictKind::Hard {
                        existing: existing.id,
                        owner: existing.owner,
                    });
                }
            }
        }

        let id = lock.id;
        self.locks.push(lock);
        Ok(id)
    }

    /// Release a lock. Per spec §18 I-Lock-6: the caller must be either
    /// the lock owner or hold the `Unlock` capability. Phase 1 takes
    /// only the owner check here; the capability check happens in the
    /// cube layer.
    pub fn release(&mut self, lock_id: LockId, principal: PrincipalId) -> Result<(), ReleaseError> {
        let position = self
            .locks
            .iter()
            .position(|l| l.id == lock_id)
            .ok_or(ReleaseError::NotFound)?;
        if self.locks[position].owner != principal {
            return Err(ReleaseError::NotOwner);
        }
        self.locks.remove(position);
        Ok(())
    }

    /// Drop every lock whose `expires_at <= now`. Returns the count
    /// removed.
    pub fn purge_expired(&mut self, now: u64) -> usize {
        let before = self.locks.len();
        self.locks.retain(|l| l.expires_at > now);
        before - self.locks.len()
    }

    /// Returns the conflicting Hard lock, if any, blocking
    /// `principal`'s write to `coord` at `now`. Soft locks DO NOT block
    /// — they surface advisories via `soft_locks_covering` instead.
    ///
    /// Per spec §18 I-Lock-2.
    pub fn check_write(
        &mut self,
        principal: PrincipalId,
        dims: &[Dimension],
        coord: &CellCoordinate,
        now: u64,
    ) -> Option<&Lock> {
        self.purge_expired(now);
        self.locks.iter().find(|l| {
            matches!(l.kind, LockKind::Hard)
                && l.owner != principal
                && coord_in_pattern(coord, &l.pattern, dims)
        })
    }

    /// Returns every Soft lock whose pattern covers `coord` (used to
    /// build the `soft_lock_notes` field on `WritebackResult`).
    pub fn soft_locks_covering(&self, dims: &[Dimension], coord: &CellCoordinate) -> Vec<&Lock> {
        self.locks
            .iter()
            .filter(|l| {
                matches!(l.kind, LockKind::Soft) && coord_in_pattern(coord, &l.pattern, dims)
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ConflictKind {
    Hard {
        existing: LockId,
        owner: PrincipalId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseError {
    NotFound,
    NotOwner,
}

fn coord_in_pattern(coord: &CellCoordinate, pattern: &ScopePattern, dims: &[Dimension]) -> bool {
    // Pattern matching = same logic as PermissionTable::matches; we
    // re-implement at module level so lock.rs doesn't depend on a private
    // permission helper.
    for (i, d) in dims.iter().enumerate() {
        let element = coord.element_at(i);
        let Some(binding) = pattern.bindings.get(&d.id) else {
            continue;
        };
        if !crate::permission::binding_matches(binding, element, d) {
            return false;
        }
    }
    true
}

/// Two patterns overlap iff there exists at least one coord that
/// satisfies both — Phase 1 implementation: per-dim bindings are
/// compatible. For `Subtree(a)` vs `Subtree(b)` we approximate by
/// checking subtree containment in either direction (which is sound:
/// if neither contains the other, the subtrees are disjoint and the
/// patterns don't overlap on that dim). The `One`/`Many`/`All`
/// combinations are handled directly.
fn patterns_overlap(a: &ScopePattern, b: &ScopePattern, dims: &[Dimension]) -> bool {
    for d in dims {
        let ba = a.bindings.get(&d.id);
        let bb = b.bindings.get(&d.id);
        match (ba, bb) {
            (None, _) | (_, None) => {
                // One side has no binding on this dim → treats it as
                // All; no exclusion possible from this dim alone.
                continue;
            }
            (Some(a), Some(b)) => {
                if !bindings_overlap(a, b, d) {
                    return false;
                }
            }
        }
    }
    true
}

fn bindings_overlap(
    a: &crate::permission::ScopeBinding,
    b: &crate::permission::ScopeBinding,
    dim: &Dimension,
) -> bool {
    use crate::permission::ScopeBinding::*;
    match (a, b) {
        (All, _) | (_, All) => true,
        (One(x), One(y)) => x == y,
        (One(x), Many(ys)) | (Many(ys), One(x)) => ys.contains(x),
        (Many(xs), Many(ys)) => xs.iter().any(|x| ys.contains(x)),
        (One(x), Subtree(root)) | (Subtree(root), One(x)) => {
            crate::permission::element_is_in_subtree(*root, *x, dim)
        }
        (Many(xs), Subtree(root)) | (Subtree(root), Many(xs)) => xs
            .iter()
            .any(|x| crate::permission::element_is_in_subtree(*root, *x, dim)),
        (Subtree(a_root), Subtree(b_root)) => {
            // Overlap iff one root is in the other's subtree (including
            // equality). Disjoint subtrees → no overlap.
            crate::permission::element_is_in_subtree(*a_root, *b_root, dim)
                || crate::permission::element_is_in_subtree(*b_root, *a_root, dim)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dimension::{Dimension, DimensionKind};
    use crate::element::Element;
    use crate::hierarchy::Hierarchy;
    use crate::id::{CubeId, ElementId, IdGenerator, PrincipalId};
    use crate::permission::{capability, ScopeBinding};

    fn fixture() -> (Vec<Dimension>, ElementId, ElementId, ElementId, ElementId) {
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
        (
            vec![market_dim, measure_dim],
            florida,
            tampa,
            atlanta,
            measure,
        )
    }

    fn coord(market: ElementId, measure: ElementId) -> CellCoordinate {
        CellCoordinate::from_parts(CubeId(1), [market, measure])
    }

    fn florida_pattern(florida: ElementId, market_dim_id: crate::id::DimensionId) -> ScopePattern {
        ScopePattern::new().with(market_dim_id, ScopeBinding::Subtree(florida))
    }

    #[test]
    fn hard_lock_blocks_writes_by_other_principals() {
        let (dims, florida, tampa, atlanta, measure) = fixture();
        let market_dim_id = dims[0].id;
        let mut table = LockTable::new(CubeId(1));
        let lock = Lock {
            id: LockId(1),
            owner: PrincipalId(7),
            pattern: florida_pattern(florida, market_dim_id),
            kind: LockKind::Hard,
            acquired_at: 100,
            expires_at: 200,
            note: None,
        };
        table.acquire(lock, &dims).expect("acquire ok");

        // Owner can write within Florida.
        let tampa_coord = coord(tampa, measure);
        let atlanta_coord = coord(atlanta, measure);
        assert!(table
            .check_write(PrincipalId(7), &dims, &tampa_coord, 150)
            .is_none());

        // Other principal blocked within Florida.
        let blocker = table
            .check_write(PrincipalId(99), &dims, &tampa_coord, 150)
            .expect("expected block");
        assert_eq!(blocker.id, LockId(1));

        // Other principal NOT blocked outside Florida.
        assert!(table
            .check_write(PrincipalId(99), &dims, &atlanta_coord, 150)
            .is_none());
    }

    #[test]
    fn expired_lock_does_not_block() {
        let (dims, florida, tampa, _atlanta, measure) = fixture();
        let market_dim_id = dims[0].id;
        let mut table = LockTable::new(CubeId(1));
        table
            .acquire(
                Lock {
                    id: LockId(1),
                    owner: PrincipalId(7),
                    pattern: florida_pattern(florida, market_dim_id),
                    kind: LockKind::Hard,
                    acquired_at: 100,
                    expires_at: 200,
                    note: None,
                },
                &dims,
            )
            .expect("acquire ok");
        let tampa_coord = coord(tampa, measure);

        // After expiry, no block.
        assert!(table
            .check_write(PrincipalId(99), &dims, &tampa_coord, 250)
            .is_none());
        // And the lock is purged.
        assert!(table.is_empty());
    }

    #[test]
    fn soft_lock_does_not_block_but_surfaces_note() {
        let (dims, florida, tampa, _atlanta, measure) = fixture();
        let market_dim_id = dims[0].id;
        let mut table = LockTable::new(CubeId(1));
        table
            .acquire(
                Lock {
                    id: LockId(1),
                    owner: PrincipalId(7),
                    pattern: florida_pattern(florida, market_dim_id),
                    kind: LockKind::Soft,
                    acquired_at: 100,
                    expires_at: 200,
                    note: Some("editing — please coordinate".into()),
                },
                &dims,
            )
            .expect("acquire ok");

        let tampa_coord = coord(tampa, measure);
        assert!(table
            .check_write(PrincipalId(99), &dims, &tampa_coord, 150)
            .is_none());
        let advisories = table.soft_locks_covering(&dims, &tampa_coord);
        assert_eq!(advisories.len(), 1);
        assert_eq!(
            advisories[0].note.as_deref(),
            Some("editing — please coordinate")
        );
    }

    #[test]
    fn hard_lock_conflict_with_existing_other_owner() {
        let (dims, florida, _tampa, _atlanta, _measure) = fixture();
        let market_dim_id = dims[0].id;
        let mut table = LockTable::new(CubeId(1));
        table
            .acquire(
                Lock {
                    id: LockId(1),
                    owner: PrincipalId(7),
                    pattern: florida_pattern(florida, market_dim_id),
                    kind: LockKind::Hard,
                    acquired_at: 100,
                    expires_at: 200,
                    note: None,
                },
                &dims,
            )
            .expect("first ok");
        // Different owner attempting Hard lock on the same scope.
        let result = table.acquire(
            Lock {
                id: LockId(2),
                owner: PrincipalId(99),
                pattern: florida_pattern(florida, market_dim_id),
                kind: LockKind::Hard,
                acquired_at: 110,
                expires_at: 200,
                note: None,
            },
            &dims,
        );
        assert!(matches!(result, Err(ConflictKind::Hard { .. })));
    }

    #[test]
    fn release_by_non_owner_rejected() {
        let (dims, florida, _tampa, _atlanta, _measure) = fixture();
        let market_dim_id = dims[0].id;
        let mut table = LockTable::new(CubeId(1));
        let id = table
            .acquire(
                Lock {
                    id: LockId(1),
                    owner: PrincipalId(7),
                    pattern: florida_pattern(florida, market_dim_id),
                    kind: LockKind::Hard,
                    acquired_at: 100,
                    expires_at: 200,
                    note: None,
                },
                &dims,
            )
            .expect("ok");
        let result = table.release(id, PrincipalId(99));
        assert_eq!(result, Err(ReleaseError::NotOwner));
        assert_eq!(table.len(), 1);
        // Owner can release.
        table.release(id, PrincipalId(7)).expect("owner ok");
        assert!(table.is_empty());
    }

    #[test]
    fn capability_const_smoke() {
        // Just to make sure permission.rs's capability bits are
        // re-importable from this module's tests too.
        assert_ne!(capability::READ, capability::WRITE);
    }
}
