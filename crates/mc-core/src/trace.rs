//! Trace types — DATA ONLY.
//!
//! Per phase-1-rust-kernel-build-brief.md §3.11. The trace generation
//! algorithm itself (recursive walk during read) lands with `cube.rs` and
//! `consolidation.rs` in a later deliverable. This module exists now only
//! so `cell.rs` can hold an `Option<Trace>` field without forward-ref
//! gymnastics; the structs below have no behavior.

use crate::coordinate::CellCoordinate;
use crate::id::{HierarchyId, PrincipalId, RuleId};
use crate::revision::Revision;
use crate::value::ScalarValue;

#[derive(Clone, Debug)]
pub struct Trace {
    pub root: TraceNode,
    pub revision: Revision,
    pub elapsed_us: u64,
}

#[derive(Clone, Debug)]
pub struct TraceNode {
    pub coord: CellCoordinate,
    pub value: ScalarValue,
    pub operation: TraceOp,
    pub children: Vec<TraceNode>,
}

#[derive(Clone, Debug)]
pub enum TraceOp {
    InputLookup {
        written_at: u64,
        written_by: PrincipalId,
    },
    RuleEvaluation {
        rule_id: RuleId,
        expr_summary: ExprSummary,
    },
    /// Multi-hierarchy aware (per cleanup pass v1.1). Same shape as
    /// `Provenance::Consolidation` in `cell.rs`.
    Consolidation {
        hierarchies: smallvec::SmallVec<[HierarchyId; 4]>,
        child_count: u32,
    },
    DefaultFallback {
        default: ScalarValue,
        reason: &'static str,
    },
    NullPoison {
        upstream: CellCoordinate,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct ExprSummary {
    pub op: ExprOp,
    pub arity: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum ExprOp {
    Const,
    SelfRef,
    Add,
    Sub,
    Mul,
    Div,
    IfNull,
}
