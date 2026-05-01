//! Concrete `HashMapStore` for cell persistence.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.9.
//!
//! Phase 1 uses this concrete struct directly — there is no `CellStore`
//! trait, no trait object, and no pluggable backend. The trait is a Phase 2
//! concern once a second backend (Arrow / LSM / Roaring) actually justifies
//! the abstraction. Defining it now would force `clone_box`,
//! `Debug`-on-trait-object, and `Clone for Box<dyn Trait>` plumbing for a v1
//! that only has one impl.

use ahash::AHashMap;

use crate::cell::StoredCell;
use crate::coordinate::CellCoordinate;

#[derive(Clone, Debug, Default)]
pub struct HashMapStore {
    cells: AHashMap<CellCoordinate, StoredCell>,
}

impl HashMapStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&self, coord: &CellCoordinate) -> Option<&StoredCell> {
        self.cells.get(coord)
    }

    pub fn write(&mut self, coord: CellCoordinate, cell: StoredCell) {
        self.cells.insert(coord, cell);
    }

    pub fn remove(&mut self, coord: &CellCoordinate) -> Option<StoredCell> {
        self.cells.remove(coord)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&CellCoordinate, &StoredCell)> {
        self.cells.iter()
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Provenance;
    use crate::id::{CubeId, ElementId, PrincipalId, Revision};
    use crate::value::ScalarValue;

    fn make_coord() -> CellCoordinate {
        CellCoordinate::from_parts(CubeId(1), [ElementId(1), ElementId(2)])
    }

    fn make_cell() -> StoredCell {
        StoredCell {
            value: ScalarValue::F64(123.45),
            provenance: Provenance::Input {
                written_at: 1_700_000_000,
                written_by: PrincipalId(1),
            },
            uncertainty: None,
            revision: Revision(1),
        }
    }

    #[test]
    fn write_then_read_returns_same_cell() {
        let mut store = HashMapStore::new();
        let coord = make_coord();
        store.write(coord.clone(), make_cell());
        let got = store.read(&coord).expect("present");
        match &got.value {
            ScalarValue::F64(v) => assert!((v - 123.45).abs() < 1e-12),
            other => panic!("unexpected value variant: {:?}", other),
        }
    }

    #[test]
    fn read_missing_returns_none() {
        let store = HashMapStore::new();
        let coord = make_coord();
        assert!(store.read(&coord).is_none());
    }

    #[test]
    fn write_overwrites_existing_cell() {
        let mut store = HashMapStore::new();
        let coord = make_coord();
        store.write(coord.clone(), make_cell());

        let mut updated = make_cell();
        updated.value = ScalarValue::F64(999.0);
        updated.revision = Revision(2);
        store.write(coord.clone(), updated);

        let got = store.read(&coord).expect("present");
        assert_eq!(got.value.as_f64(), Some(999.0));
        assert_eq!(got.revision, Revision(2));
    }

    #[test]
    fn remove_evicts_and_returns_prior() {
        let mut store = HashMapStore::new();
        let coord = make_coord();
        store.write(coord.clone(), make_cell());
        let removed = store.remove(&coord).expect("present");
        assert!(matches!(removed.value, ScalarValue::F64(_)));
        assert!(store.read(&coord).is_none());
    }

    #[test]
    fn len_and_is_empty_track_population() {
        let mut store = HashMapStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        store.write(make_coord(), make_cell());
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }
}
