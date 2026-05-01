//! Hierarchies: parent/child trees over the elements of a single dimension.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.6.
//!
//! Phase 1 supports single-parent forests only: every element has at most
//! one parent. Multi-parent / alternate hierarchies are Phase 2+.
//!
//! `HierarchyBuilder::build()` validates structurally:
//!   1. NaN / ±Inf weights → `EngineError::InvalidWeight`
//!   2. Duplicate edges (same parent+child) → `EngineError::DuplicateHierarchyEdge`
//!   3. Multi-parent → `EngineError::MultipleParents`
//!   4. Cycles (DFS) → `EngineError::HierarchyCycle { path }`
//!
//! "Edges reference only valid elements in the dimension" is checked one
//! layer up, in `DimensionBuilder::add_hierarchy`, where the element list is
//! actually visible. Hierarchy itself is structural-only.

use ahash::{AHashMap, AHashSet};

use crate::error::EngineError;
use crate::id::{DimensionId, ElementId, HierarchyId};

#[derive(Clone, Debug)]
pub struct HierarchyEdge {
    pub parent: ElementId,
    pub child: ElementId,
    pub weight: f64,
}

#[derive(Clone, Debug)]
pub struct Hierarchy {
    pub id: HierarchyId,
    pub name: String,
    pub dimension: DimensionId,
    pub edges: Vec<HierarchyEdge>,
    pub roots: Vec<ElementId>,
    pub leaves: AHashSet<ElementId>,
    pub consolidated: AHashSet<ElementId>,
    /// parent → outgoing edges, for fast forward walks
    pub children_of: AHashMap<ElementId, Vec<HierarchyEdge>>,
    /// child → parent, for fast invalidation walks
    pub parent_of: AHashMap<ElementId, ElementId>,
}

impl Hierarchy {
    pub fn builder(id: HierarchyId, name: impl Into<String>, dim: DimensionId) -> HierarchyBuilder {
        HierarchyBuilder {
            id,
            name: name.into(),
            dim,
            staged_edges: Vec::new(),
        }
    }

    /// Returns every leaf reachable from `root` with the cumulative weight
    /// product. For Phase 1's all-`weight=1.0` Acme demo, every cumulative
    /// weight is `1.0`. For weighted hierarchies, the product follows the
    /// path from `root` to the leaf.
    pub fn descendants(&self, root: ElementId) -> Vec<(ElementId, f64)> {
        let mut out = Vec::new();
        // Stack of (elem, cumulative_weight_so_far)
        let mut stack: Vec<(ElementId, f64)> = vec![(root, 1.0)];
        while let Some((cur, weight)) = stack.pop() {
            if self.leaves.contains(&cur) {
                out.push((cur, weight));
                continue;
            }
            if let Some(children) = self.children_of.get(&cur) {
                for edge in children {
                    stack.push((edge.child, weight * edge.weight));
                }
            } else {
                // No outgoing edges and not in leaves — treat as a degenerate
                // leaf. This case shouldn't occur for a well-formed hierarchy
                // but we don't panic.
                out.push((cur, weight));
            }
        }
        out
    }

    pub fn is_leaf(&self, e: ElementId) -> bool {
        self.leaves.contains(&e)
    }

    pub fn is_consolidated(&self, e: ElementId) -> bool {
        self.consolidated.contains(&e)
    }

    /// Walk parent pointers from `leaf` up to the root, returning each
    /// ancestor with the cumulative weight product. The leaf itself is NOT
    /// included.
    pub fn ancestors(&self, leaf: ElementId) -> Vec<(ElementId, f64)> {
        let mut out = Vec::new();
        let mut current = leaf;
        let mut weight = 1.0_f64;
        while let Some(&parent) = self.parent_of.get(&current) {
            // Find the edge weight from parent → current to compose the
            // cumulative weight upward.
            if let Some(children) = self.children_of.get(&parent) {
                if let Some(edge) = children.iter().find(|e| e.child == current) {
                    weight *= edge.weight;
                }
            }
            out.push((parent, weight));
            current = parent;
        }
        out
    }
}

#[derive(Debug)]
pub struct HierarchyBuilder {
    id: HierarchyId,
    name: String,
    dim: DimensionId,
    staged_edges: Vec<HierarchyEdge>,
}

impl HierarchyBuilder {
    pub fn add_edge(mut self, parent: ElementId, child: ElementId, weight: f64) -> Self {
        self.staged_edges.push(HierarchyEdge {
            parent,
            child,
            weight,
        });
        self
    }

    pub fn build(self) -> Result<Hierarchy, EngineError> {
        let HierarchyBuilder {
            id,
            name,
            dim,
            staged_edges,
        } = self;

        // 1. Validate weights are finite.
        for edge in &staged_edges {
            if !edge.weight.is_finite() {
                return Err(EngineError::InvalidWeight(edge.weight));
            }
        }

        // 2. Detect duplicate edges (same parent + child). Two distinct edges
        //    with the same (parent, child) can never coexist in a forest.
        //    We also catch (parent, child) repeated even if weights differ —
        //    the user must explicitly pick one weight.
        let mut seen: AHashSet<(ElementId, ElementId)> = AHashSet::new();
        for edge in &staged_edges {
            if !seen.insert((edge.parent, edge.child)) {
                return Err(EngineError::DuplicateHierarchyEdge {
                    parent: edge.parent,
                    child: edge.child,
                });
            }
        }

        // 3. Build parent_of map; reject multi-parent.
        let mut parent_of: AHashMap<ElementId, ElementId> = AHashMap::new();
        for edge in &staged_edges {
            if let Some(&existing) = parent_of.get(&edge.child) {
                if existing != edge.parent {
                    return Err(EngineError::MultipleParents {
                        element: edge.child,
                        existing,
                        attempted: edge.parent,
                    });
                }
                // existing == edge.parent: caught by duplicate-edge check above.
            }
            parent_of.insert(edge.child, edge.parent);
        }

        // 4. Build children_of map.
        let mut children_of: AHashMap<ElementId, Vec<HierarchyEdge>> = AHashMap::new();
        for edge in &staged_edges {
            children_of
                .entry(edge.parent)
                .or_default()
                .push(edge.clone());
        }

        // 5. Cycle detection (DFS with white/gray/black).
        if let Some(path) = detect_cycle(&children_of) {
            return Err(EngineError::HierarchyCycle { path });
        }

        // 6. Compute leaves and consolidated sets.
        let mut all_elements: AHashSet<ElementId> = AHashSet::new();
        let mut parents: AHashSet<ElementId> = AHashSet::new();
        let mut children: AHashSet<ElementId> = AHashSet::new();
        for edge in &staged_edges {
            all_elements.insert(edge.parent);
            all_elements.insert(edge.child);
            parents.insert(edge.parent);
            children.insert(edge.child);
        }

        // Leaf = appears as a child but never as a parent.
        // (An element that appears only as a parent is a root, not a leaf.)
        let leaves: AHashSet<ElementId> = children
            .iter()
            .copied()
            .filter(|e| !parents.contains(e))
            .collect();

        // Consolidated = appears as a parent (i.e., has at least one child).
        let consolidated: AHashSet<ElementId> = parents.clone();

        // Roots = appears as a parent but never as a child.
        let mut roots: Vec<ElementId> = parents
            .iter()
            .copied()
            .filter(|e| !children.contains(e))
            .collect();
        roots.sort();

        Ok(Hierarchy {
            id,
            name,
            dimension: dim,
            edges: staged_edges,
            roots,
            leaves,
            consolidated,
            children_of,
            parent_of,
        })
    }
}

/// DFS-based cycle detection. Returns the cycle path (the sequence of
/// elements forming the loop) if one exists. White/gray/black coloring:
///   - White: unvisited
///   - Gray:  on current DFS stack
///   - Black: finished
/// A back-edge to a Gray node means we hit a cycle.
fn detect_cycle(children_of: &AHashMap<ElementId, Vec<HierarchyEdge>>) -> Option<Vec<ElementId>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    // Gather every node mentioned as a parent or child.
    let mut nodes: AHashSet<ElementId> = AHashSet::new();
    for (&parent, edges) in children_of {
        nodes.insert(parent);
        for e in edges {
            nodes.insert(e.child);
        }
    }

    let mut color: AHashMap<ElementId, Color> =
        nodes.iter().copied().map(|n| (n, Color::White)).collect();
    let mut stack_path: Vec<ElementId> = Vec::new();

    // Iterative DFS using an explicit work stack to avoid recursion-depth
    // limits on pathological inputs. We mark Gray on push, Black on pop.
    for &start in &nodes {
        if color.get(&start).copied().unwrap_or(Color::White) != Color::White {
            continue;
        }
        // (node, child-index-to-visit-next)
        let mut work: Vec<(ElementId, usize)> = vec![(start, 0)];
        color.insert(start, Color::Gray);
        stack_path.push(start);

        while let Some(&(cur, idx)) = work.last() {
            let outgoing = children_of.get(&cur).map(Vec::as_slice).unwrap_or(&[]);
            if idx < outgoing.len() {
                // Advance the child pointer in the work stack. Indexing by
                // (work.len() - 1) is safe: `work.last()` above bound the
                // loop condition to a non-empty stack.
                let top = work.len() - 1;
                work[top].1 = idx + 1;
                let next = outgoing[idx].child;
                match color.get(&next).copied().unwrap_or(Color::White) {
                    Color::White => {
                        color.insert(next, Color::Gray);
                        stack_path.push(next);
                        work.push((next, 0));
                    }
                    Color::Gray => {
                        // Back-edge → cycle. Build the cycle path: from `next`
                        // through stack_path until end, plus the back-edge.
                        if let Some(start_idx) = stack_path.iter().position(|&n| n == next) {
                            let mut path: Vec<ElementId> = stack_path[start_idx..].to_vec();
                            // Close the cycle visually by repeating the first
                            // node at the end.
                            path.push(next);
                            return Some(path);
                        }
                        // Should be unreachable: a Gray node must be on the
                        // stack_path. If we somehow miss it, return what we
                        // have.
                        return Some(vec![next]);
                    }
                    Color::Black => {
                        // Already explored, no cycle through here.
                    }
                }
            } else {
                // Done with this node.
                color.insert(cur, Color::Black);
                work.pop();
                stack_path.pop();
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dim() -> DimensionId {
        DimensionId(1)
    }
    fn h() -> HierarchyId {
        HierarchyId(1)
    }

    #[test]
    fn build_simple_three_month_quarter() {
        let q1 = ElementId(100);
        let jan = ElementId(1);
        let feb = ElementId(2);
        let mar = ElementId(3);
        let hier = Hierarchy::builder(h(), "Calendar", dim())
            .add_edge(q1, jan, 1.0)
            .add_edge(q1, feb, 1.0)
            .add_edge(q1, mar, 1.0)
            .build()
            .expect("hierarchy must build");
        assert_eq!(hier.roots, vec![q1]);
        assert_eq!(hier.leaves.len(), 3);
        assert!(hier.consolidated.contains(&q1));
        assert!(hier.is_leaf(jan));
        assert!(!hier.is_leaf(q1));
    }

    #[test]
    fn descendants_returns_three_leaves_with_unit_weight() {
        let q1 = ElementId(100);
        let jan = ElementId(1);
        let feb = ElementId(2);
        let mar = ElementId(3);
        let hier = Hierarchy::builder(h(), "Calendar", dim())
            .add_edge(q1, jan, 1.0)
            .add_edge(q1, feb, 1.0)
            .add_edge(q1, mar, 1.0)
            .build()
            .expect("hierarchy must build");
        let descendants = hier.descendants(q1);
        assert_eq!(descendants.len(), 3);
        for (_, w) in &descendants {
            assert!((*w - 1.0).abs() < 1e-12);
        }
    }
}
