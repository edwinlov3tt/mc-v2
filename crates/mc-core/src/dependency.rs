//! Cell-level dependency graph: forward edges (cell → cells it reads)
//! and reverse edges (cell → cells that read it). Used by `cube.rs` to
//! detect cycles at registration time and propagate dirty state on
//! writes.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.12.
//!
//! Phase 1 builds this graph **lazily**: edges are materialized when a
//! rule first evaluates at a coordinate. Pre-computing the entire graph
//! up front would require enumerating every (leaf coord × rule)
//! combination, which works for Acme (~2,520 leaves × 5 rules) but
//! doesn't generalize.
//!
//! Hierarchy edges (consolidated coord → leaf descendants) are added by
//! the cube builder when a hierarchy is bound. They're a fixed structural
//! contribution to the graph rather than a lazy by-product of reads, so
//! they show up in `forward`/`reverse` from cube-build time forward.

use ahash::{AHashMap, AHashSet};

use crate::coordinate::CellCoordinate;
use crate::id::{HierarchyId, RuleId};

#[derive(Clone, Debug)]
pub struct DependencyEdge {
    /// The cell this edge points TO — i.e., the cell that the source
    /// reads. Forward edges go `from → edge.to`; reverse edges go
    /// `edge.to → from`.
    pub to: CellCoordinate,
    pub via: DependencySource,
}

#[derive(Clone, Debug)]
pub enum DependencySource {
    Rule(RuleId),
    Hierarchy(HierarchyId),
}

#[derive(Clone, Debug, Default)]
pub struct DependencyGraph {
    /// `from` cell → list of cells `from` reads.
    forward: AHashMap<CellCoordinate, Vec<DependencyEdge>>,
    /// `to` cell → list of cells that read `to` (the reverse-edge index).
    /// Used for fast invalidation: when `to` changes, every coord in
    /// `reverse[to]` becomes dirty.
    reverse: AHashMap<CellCoordinate, Vec<CellCoordinate>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an edge `from` reads `to`. Idempotent within a (from, to,
    /// via) triple — adding the same logical edge twice does not
    /// duplicate it.
    ///
    /// Per spec §15 I-Dep-1: callers must not introduce a cycle. This
    /// function does NOT check for cycles on each add; cycle detection
    /// is run separately via `detect_cycle()` when a new rule is being
    /// registered (the only way new edges enter the graph in the
    /// general case).
    pub fn add_edge(&mut self, from: CellCoordinate, edge: DependencyEdge) {
        let to = edge.to.clone();
        // Forward: append iff not already present (compare by `to` and
        // `via` discriminant — same RuleId or same HierarchyId).
        let entry = self.forward.entry(from.clone()).or_default();
        let already = entry.iter().any(|e| {
            e.to == to
                && match (&e.via, &edge.via) {
                    (DependencySource::Rule(a), DependencySource::Rule(b)) => a == b,
                    (DependencySource::Hierarchy(a), DependencySource::Hierarchy(b)) => a == b,
                    _ => false,
                }
        });
        if !already {
            entry.push(edge);
        }
        // Reverse: append iff `from` not already in reverse[to].
        let rev = self.reverse.entry(to).or_default();
        if !rev.iter().any(|c| c == &from) {
            rev.push(from);
        }
    }

    /// All coords that read `coord`. Empty slice if no one reads it.
    pub fn dependents_of(&self, coord: &CellCoordinate) -> &[CellCoordinate] {
        self.reverse.get(coord).map(Vec::as_slice).unwrap_or(&[])
    }

    /// All edges from `coord` — i.e., the cells `coord` reads.
    pub fn dependencies_of(&self, coord: &CellCoordinate) -> &[DependencyEdge] {
        self.forward.get(coord).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Compute the closure of dependents of `root`: every coord that
    /// transitively reads `root`. The set INCLUDES the cells reachable
    /// via the reverse edges; it does NOT include `root` itself unless
    /// `root` reads itself (which is forbidden by cycle detection).
    ///
    /// Used for dirty propagation per spec §16 I-Dirty-1.
    pub fn closure_of_dependents(&self, root: &CellCoordinate) -> AHashSet<CellCoordinate> {
        let mut seen: AHashSet<CellCoordinate> = AHashSet::new();
        let mut stack: Vec<CellCoordinate> = self.dependents_of(root).to_vec();
        while let Some(c) = stack.pop() {
            if !seen.insert(c.clone()) {
                continue;
            }
            for next in self.dependents_of(&c) {
                if !seen.contains(next) {
                    stack.push(next.clone());
                }
            }
        }
        seen
    }

    /// Number of distinct `from` coords that have outgoing edges.
    pub fn forward_node_count(&self) -> usize {
        self.forward.len()
    }

    /// Total forward-edge count, summed across all `from` coords.
    pub fn forward_edge_count(&self) -> usize {
        self.forward.values().map(Vec::len).sum()
    }

    /// True iff the graph contains no edges (forward or reverse).
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty() && self.reverse.is_empty()
    }

    /// Run a cycle check across the current forward edges. Returns the
    /// cycle path if one exists.
    ///
    /// Per spec §15 I-Dep-1, the graph is acyclic; cycle detection is
    /// run when a new rule is registered (so a fresh edge that would
    /// close a cycle is rejected before it's committed). The full
    /// cell-level cycle scan is O(N+E) over forward edges — fine for
    /// Phase 1 cube sizes.
    pub fn detect_cycle(&self) -> Option<Vec<CellCoordinate>> {
        cycle_scan(&self.forward)
    }
}

fn cycle_scan(
    forward: &AHashMap<CellCoordinate, Vec<DependencyEdge>>,
) -> Option<Vec<CellCoordinate>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    // Gather all nodes appearing on either side of any edge.
    let mut nodes: AHashSet<CellCoordinate> = AHashSet::new();
    for (from, edges) in forward {
        nodes.insert(from.clone());
        for e in edges {
            nodes.insert(e.to.clone());
        }
    }

    let mut color: AHashMap<CellCoordinate, Color> =
        nodes.iter().cloned().map(|n| (n, Color::White)).collect();

    let mut starts: Vec<CellCoordinate> = nodes.into_iter().collect();
    // Deterministic iteration order (CLAUDE.md §2.11): sort by element
    // slice first, then cube id. CellCoordinate impls PartialOrd? No —
    // we hash but don't order. Sort by Debug repr is a stable cheap
    // proxy here since the actual order doesn't affect correctness, only
    // determinism of the reported cycle path.
    starts.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));

    let mut stack_path: Vec<CellCoordinate> = Vec::new();

    for start in starts {
        if color.get(&start).copied().unwrap_or(Color::White) != Color::White {
            continue;
        }
        let mut work_stack: Vec<(CellCoordinate, Vec<CellCoordinate>, usize)> = Vec::new();
        let mut deps: Vec<CellCoordinate> = forward
            .get(&start)
            .map(|edges| edges.iter().map(|e| e.to.clone()).collect())
            .unwrap_or_default();
        deps.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        work_stack.push((start.clone(), deps, 0));
        color.insert(start.clone(), Color::Gray);
        stack_path.push(start);

        while !work_stack.is_empty() {
            let top = work_stack.len() - 1;
            let cur = work_stack[top].0.clone();
            let idx = work_stack[top].1.len().min(work_stack[top].2);
            let next_dep = work_stack[top].1.get(idx).cloned();

            if let Some(next) = next_dep {
                work_stack[top].2 = idx + 1;
                match color.get(&next).copied().unwrap_or(Color::White) {
                    Color::White => {
                        color.insert(next.clone(), Color::Gray);
                        stack_path.push(next.clone());
                        let mut next_deps: Vec<CellCoordinate> = forward
                            .get(&next)
                            .map(|edges| edges.iter().map(|e| e.to.clone()).collect())
                            .unwrap_or_default();
                        next_deps.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
                        work_stack.push((next, next_deps, 0));
                    }
                    Color::Gray => {
                        if let Some(start_idx) = stack_path.iter().position(|n| n == &next) {
                            let mut path: Vec<CellCoordinate> = stack_path[start_idx..].to_vec();
                            path.push(next);
                            return Some(path);
                        }
                        return Some(vec![next]);
                    }
                    Color::Black => {}
                }
            } else {
                color.insert(cur, Color::Black);
                work_stack.pop();
                stack_path.pop();
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{CubeId, ElementId, HierarchyId, RuleId};

    fn coord(cube: u64, elements: &[u64]) -> CellCoordinate {
        CellCoordinate::from_parts(CubeId(cube), elements.iter().map(|&e| ElementId(e)))
    }

    fn rule_edge(to: CellCoordinate, rule_id: u64) -> DependencyEdge {
        DependencyEdge {
            to,
            via: DependencySource::Rule(RuleId(rule_id)),
        }
    }

    #[test]
    fn empty_graph_has_no_edges_no_dependents() {
        let g = DependencyGraph::new();
        assert!(g.is_empty());
        assert_eq!(g.forward_edge_count(), 0);
        assert!(g.dependents_of(&coord(1, &[1])).is_empty());
        assert!(g.detect_cycle().is_none());
    }

    #[test]
    fn add_edge_records_forward_and_reverse() {
        let mut g = DependencyGraph::new();
        let a = coord(1, &[10]);
        let b = coord(1, &[20]);
        // a reads b
        g.add_edge(a.clone(), rule_edge(b.clone(), 1));
        assert_eq!(g.forward_edge_count(), 1);
        assert_eq!(g.dependencies_of(&a).len(), 1);
        // Reverse: b is read by a.
        let dependents = g.dependents_of(&b);
        assert_eq!(dependents.len(), 1);
        assert_eq!(&dependents[0], &a);
    }

    #[test]
    fn add_edge_is_idempotent_for_same_via() {
        let mut g = DependencyGraph::new();
        let a = coord(1, &[10]);
        let b = coord(1, &[20]);
        g.add_edge(a.clone(), rule_edge(b.clone(), 1));
        g.add_edge(a.clone(), rule_edge(b.clone(), 1));
        // Same edge added twice — dedup.
        assert_eq!(g.forward_edge_count(), 1);
        assert_eq!(g.dependents_of(&b).len(), 1);
    }

    #[test]
    fn distinct_via_does_not_dedup() {
        // Same (from, to) but one via Rule(1) and one via Hierarchy(2)
        // are conceptually different edges (a rule dep AND a hierarchy
        // rollup, e.g.) — both should be recorded.
        let mut g = DependencyGraph::new();
        let a = coord(1, &[10]);
        let b = coord(1, &[20]);
        g.add_edge(a.clone(), rule_edge(b.clone(), 1));
        g.add_edge(
            a.clone(),
            DependencyEdge {
                to: b.clone(),
                via: DependencySource::Hierarchy(HierarchyId(7)),
            },
        );
        assert_eq!(g.forward_edge_count(), 2);
        // But reverse still dedups by `from` — only one entry: a.
        assert_eq!(g.dependents_of(&b).len(), 1);
    }

    #[test]
    fn closure_walks_three_rule_chain() {
        // Acme-shaped: Spend → Clicks → Leads → Customers → Revenue → GP.
        // Edges (forward = "read"): Clicks reads Spend, Leads reads
        // Clicks, etc.
        let mut g = DependencyGraph::new();
        let spend = coord(1, &[1]);
        let clicks = coord(1, &[2]);
        let leads = coord(1, &[3]);
        let customers = coord(1, &[4]);
        let revenue = coord(1, &[5]);
        let gp = coord(1, &[6]);
        g.add_edge(clicks.clone(), rule_edge(spend.clone(), 1));
        g.add_edge(leads.clone(), rule_edge(clicks.clone(), 2));
        g.add_edge(customers.clone(), rule_edge(leads.clone(), 3));
        g.add_edge(revenue.clone(), rule_edge(customers.clone(), 4));
        g.add_edge(gp.clone(), rule_edge(revenue.clone(), 5));

        let closure = g.closure_of_dependents(&spend);
        // Everything downstream of Spend is dirty.
        assert!(closure.contains(&clicks));
        assert!(closure.contains(&leads));
        assert!(closure.contains(&customers));
        assert!(closure.contains(&revenue));
        assert!(closure.contains(&gp));
        // Spend itself is NOT in its own dependents closure.
        assert!(!closure.contains(&spend));
        assert_eq!(closure.len(), 5);
    }

    #[test]
    fn closure_does_not_include_unrelated_cells() {
        let mut g = DependencyGraph::new();
        let spend = coord(1, &[1]);
        let clicks = coord(1, &[2]);
        let unrelated_a = coord(1, &[100]);
        let unrelated_b = coord(1, &[200]);
        g.add_edge(clicks.clone(), rule_edge(spend.clone(), 1));
        g.add_edge(unrelated_b.clone(), rule_edge(unrelated_a.clone(), 7));

        let closure = g.closure_of_dependents(&spend);
        assert!(closure.contains(&clicks));
        assert!(!closure.contains(&unrelated_a));
        assert!(!closure.contains(&unrelated_b));
    }

    #[test]
    fn detect_cycle_two_node_back_edge() {
        let mut g = DependencyGraph::new();
        let a = coord(1, &[1]);
        let b = coord(1, &[2]);
        g.add_edge(a.clone(), rule_edge(b.clone(), 1));
        // Closing the cycle: b reads a.
        g.add_edge(b.clone(), rule_edge(a.clone(), 2));
        let cycle = g.detect_cycle().expect("cycle expected");
        assert!(cycle.contains(&a) && cycle.contains(&b));
    }

    #[test]
    fn detect_cycle_three_node_back_edge() {
        let mut g = DependencyGraph::new();
        let a = coord(1, &[1]);
        let b = coord(1, &[2]);
        let c = coord(1, &[3]);
        g.add_edge(a.clone(), rule_edge(b.clone(), 1));
        g.add_edge(b.clone(), rule_edge(c.clone(), 2));
        g.add_edge(c.clone(), rule_edge(a.clone(), 3));
        assert!(g.detect_cycle().is_some());
    }

    #[test]
    fn detect_cycle_acyclic_chain_returns_none() {
        let mut g = DependencyGraph::new();
        let a = coord(1, &[1]);
        let b = coord(1, &[2]);
        let c = coord(1, &[3]);
        let d = coord(1, &[4]);
        g.add_edge(a.clone(), rule_edge(b.clone(), 1));
        g.add_edge(b.clone(), rule_edge(c.clone(), 2));
        g.add_edge(c.clone(), rule_edge(d.clone(), 3));
        assert!(g.detect_cycle().is_none());
    }
}
