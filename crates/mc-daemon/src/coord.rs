//! Override coordinate resolution — shared by `/whatif` and `/sweep`.
//!
//! Per ADR-0032 Amendment 3: overlay `override.at` (or `vary.at`) onto
//! the request's `where` clause, then resolve the merged coord to
//! exactly one cell. Returns an error if zero or multiple cells match.

use mc_model::ModelRefs;
use std::collections::BTreeMap;

use crate::error_envelope::MosaicError;

/// Merge `override_at` onto `base_where` and resolve to a single cell.
///
/// Per ADR-0032 Amendment 3:
/// 1. Start with `base_where` as the base coordinate filter.
/// 2. Overlay `override_at` — fields in `at` REPLACE matching fields in `where`.
/// 3. The merged coordinate must resolve to exactly one cell.
/// 4. Zero cells → `MosaicError::UnknownCoordinate`.
/// 5. Multiple cells → `MosaicError::AmbiguousCoordinate`.
pub fn merge_override_coord(
    refs: &ModelRefs,
    cube_name: &str,
    base_where: &BTreeMap<String, String>,
    override_at: &BTreeMap<String, String>,
) -> Result<(mc_core::CellCoordinate, BTreeMap<String, String>), MosaicError> {
    let mut merged: BTreeMap<String, String> = base_where.clone();
    for (dim, elem) in override_at {
        merged.insert(dim.clone(), elem.clone());
    }

    // Validate dimension names against the cube.
    for dim in merged.keys() {
        if !refs.dimensions.contains_key(dim) {
            let available: Vec<String> = refs.dimension_order.clone();
            return Err(MosaicError::UnknownDimension {
                cube: cube_name.to_string(),
                requested: dim.clone(),
                available,
            });
        }
    }

    // Validate element names against their dimensions.
    for (dim, elem) in &merged {
        if refs.element(dim, elem).is_none() {
            let available: Vec<String> = refs
                .elements
                .keys()
                .filter(|(d, _)| d == dim)
                .map(|(_, e)| e.clone())
                .collect();
            return Err(MosaicError::UnknownElement {
                cube: cube_name.to_string(),
                dimension: dim.clone(),
                requested: elem.clone(),
                available,
            });
        }
    }

    // Resolve to a CellCoordinate via ModelRefs.
    match refs.coord_from_names(&merged) {
        Some(coord) => Ok((coord, merged)),
        None => {
            // All dims and elements validated above — if resolution still
            // fails, the merged coord is under-specified (missing dims).
            if merged.len() < refs.dimension_order.len() {
                Err(MosaicError::AmbiguousCoordinate {
                    coord: merged,
                    match_count: 0,
                })
            } else {
                Err(MosaicError::UnknownCoordinate { coord: merged })
            }
        }
    }
}
