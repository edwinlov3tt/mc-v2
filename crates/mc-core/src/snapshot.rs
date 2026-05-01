//! Snapshots — coherent immutable views of the cube at a specific
//! revision.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.16 (minimal Phase 1) and
//! engine-semantics.md §19.
//!
//! Phase 1: a snapshot is just a clone of the live `HashMapStore` plus a
//! revision number. Reading through a snapshot is a direct hashmap
//! lookup. Snapshot creation is O(N) over store size; we accept that
//! for Acme-scale (≤ ~25K cells) and revisit in Phase 3 with copy-on-
//! write or version-vector storage.

use crate::cell::StoredCell;
use crate::coordinate::CellCoordinate;
use crate::id::CubeId;
use crate::revision::Revision;
use crate::store::HashMapStore;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub cube: CubeId,
    pub revision: Revision,
    /// Wall-clock seconds when the snapshot was captured. Diagnostic
    /// only; reads use `revision` as the freshness key.
    pub captured_at: u64,
    /// Optional human-readable handle. Per spec §19 I-Snap-7 labels are
    /// advisory; two snapshots can share a label.
    pub label: Option<String>,
    /// Phase 1: deep clone of the cube's store at snapshot time.
    /// `cube.rs::rollback_to` replaces the live store with a clone of
    /// this field on rollback.
    pub(crate) store: HashMapStore,
}

impl Snapshot {
    pub fn read(&self, coord: &CellCoordinate) -> Option<&StoredCell> {
        self.store.read(coord)
    }

    /// Borrow the snapshot's store. `cube.rs::rollback_to` clones
    /// `self.store` directly through the field today; this accessor is
    /// reserved for the slice-via-snapshot reads that arrive with
    /// step 15 (slice.rs).
    #[allow(dead_code)]
    pub(crate) fn store(&self) -> &HashMapStore {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Provenance;
    use crate::id::{CubeId, ElementId, PrincipalId, Revision};
    use crate::value::ScalarValue;

    fn make_store_with_one_cell(rev: Revision) -> HashMapStore {
        let mut store = HashMapStore::new();
        let coord = CellCoordinate::from_parts(CubeId(1), [ElementId(1), ElementId(2)]);
        store.write(
            coord,
            StoredCell {
                value: ScalarValue::F64(123.45),
                provenance: Provenance::Input {
                    written_at: 1_700_000_000,
                    written_by: PrincipalId(1),
                },
                uncertainty: None,
                revision: rev,
            },
        );
        store
    }

    #[test]
    fn snapshot_read_returns_stored_cell() {
        let store = make_store_with_one_cell(Revision(5));
        let snap = Snapshot {
            cube: CubeId(1),
            revision: Revision(5),
            captured_at: 1_700_000_000,
            label: Some("Approved".into()),
            store,
        };
        let coord = CellCoordinate::from_parts(CubeId(1), [ElementId(1), ElementId(2)]);
        let cell = snap.read(&coord).expect("present");
        assert_eq!(cell.value.as_f64(), Some(123.45));
        assert_eq!(cell.revision, Revision(5));
    }

    #[test]
    fn snapshot_label_is_optional() {
        let store = HashMapStore::new();
        let labeled = Snapshot {
            cube: CubeId(1),
            revision: Revision(0),
            captured_at: 0,
            label: Some("FY2026_Approved".into()),
            store: store.clone(),
        };
        let unlabeled = Snapshot {
            cube: CubeId(1),
            revision: Revision(0),
            captured_at: 0,
            label: None,
            store,
        };
        assert!(labeled.label.is_some());
        assert!(unlabeled.label.is_none());
    }

    #[test]
    fn snapshot_clone_is_independent_of_live_store() {
        // After a snapshot, mutations to the original store don't appear
        // in the snapshot.
        let mut live = make_store_with_one_cell(Revision(5));
        let snap = Snapshot {
            cube: CubeId(1),
            revision: Revision(5),
            captured_at: 0,
            label: None,
            store: live.clone(),
        };
        let coord = CellCoordinate::from_parts(CubeId(1), [ElementId(1), ElementId(2)]);
        // Mutate live: write a new value.
        live.write(
            coord.clone(),
            StoredCell {
                value: ScalarValue::F64(999.0),
                provenance: Provenance::Input {
                    written_at: 1_700_000_001,
                    written_by: PrincipalId(1),
                },
                uncertainty: None,
                revision: Revision(6),
            },
        );
        // Snapshot still sees the old value.
        assert_eq!(
            snap.read(&coord).expect("present").value.as_f64(),
            Some(123.45)
        );
        // Live sees the new one.
        assert_eq!(
            live.read(&coord).expect("present").value.as_f64(),
            Some(999.0)
        );
    }
}
