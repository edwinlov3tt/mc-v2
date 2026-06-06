//! SPIKE (2026-06-06): graph-kernel-as-impact-substrate.
//!
//! Proving the thesis in docs/research-notes/_active/graph-kernel-as-impact-substrate.md:
//! Mosaic's dependency-graph kernel can compute the EXACT blast radius of
//! changing a non-numeric "intent" node, and the affected set is the
//! transitive closure of dependents — deterministically, with no grep.
//!
//! This is NOT product code. It models a tiny project's intent graph
//! (decisions + code-facts + edges) directly on `DependencyGraph` and
//! asserts that `closure_of_dependents` returns the right blast radius.
//!
//! Per the spike handoff: model intent on the graph DIRECTLY (decoupled
//! from the cube/measure framing) to isolate the ENGINE question from the
//! EDGE-AUTHORING question. The engine is what this file tests; the
//! authoring assessment is in the verdict report.

use mc_core::{CellCoordinate, DependencyEdge, DependencyGraph, DependencySource};
use mc_core::id::{CubeId, ElementId, RuleId};

// ---------------------------------------------------------------------------
// Intent-graph modeling: each project-knowledge node is a coordinate whose
// single element id names the node. Edges express "A exists because of B"
// as "A reads B" (forward edge A -> B), so changing B puts A in B's
// closure_of_dependents.
// ---------------------------------------------------------------------------

const PROJECT: u64 = 1;

// Node ids (the "elements" — names of intent/code nodes)
const DECISION_CORS_POLICY: u64 = 10; // ADR-012: CORS policy
const DECISION_NO_HARDCODED_ASSETS: u64 = 11; // UI commitment
const ENDPOINT_API_CHAT: u64 = 20;
const MIDDLEWARE_CORS: u64 = 21;
const COMPONENT_FOLDER_VIEW: u64 = 22;
const COMPONENT_SIDEBAR: u64 = 23;
const TEST_CORS_SMOKE: u64 = 30;
const DEPLOY_WORKER: u64 = 31;

fn node(id: u64) -> CellCoordinate {
    CellCoordinate::from_parts(CubeId(PROJECT), [ElementId(id)])
}

/// "`reader` exists because of / depends on `dep`" — a declared intent edge.
/// In graph terms: reader reads dep (forward edge reader -> dep), so dep's
/// reverse-edge set (its dependents) includes reader.
fn depends_on(g: &mut DependencyGraph, reader: u64, dep: u64, edge_id: u64) {
    g.add_edge(
        node(reader),
        DependencyEdge {
            to: node(dep),
            via: DependencySource::Rule(RuleId(edge_id)),
        },
    );
}

/// Build the tiny project intent graph (8 nodes, 8 edges).
///
///   decision_cors_policy ─────────┐
///        ▲                         │
///        │ (endpoint, middleware depend on it)
///   endpoint_api_chat ◄── test_cors_smoke ──► middleware_cors
///        ▲                                          ▲
///        │ deploy_worker depends on endpoint        │
///   deploy_worker                          test also depends on middleware
///
///   decision_no_hardcoded_assets ◄── component_folder_view
///                                ◄── component_sidebar  (SEPARATE subgraph)
fn build_intent_graph() -> DependencyGraph {
    let mut g = DependencyGraph::new();

    // CORS decision subgraph
    depends_on(&mut g, ENDPOINT_API_CHAT, DECISION_CORS_POLICY, 1);
    depends_on(&mut g, MIDDLEWARE_CORS, DECISION_CORS_POLICY, 2);
    // test depends on endpoint + middleware (2 hops from the decision)
    depends_on(&mut g, TEST_CORS_SMOKE, ENDPOINT_API_CHAT, 3);
    depends_on(&mut g, TEST_CORS_SMOKE, MIDDLEWARE_CORS, 4);
    // deploy depends on endpoint (2 hops from the decision)
    depends_on(&mut g, DEPLOY_WORKER, ENDPOINT_API_CHAT, 5);

    // Separate "no hardcoded assets" decision subgraph (must NOT appear in
    // the CORS blast radius)
    depends_on(&mut g, COMPONENT_FOLDER_VIEW, DECISION_NO_HARDCODED_ASSETS, 6);
    depends_on(&mut g, COMPONENT_SIDEBAR, DECISION_NO_HARDCODED_ASSETS, 7);

    g
}

// ---------------------------------------------------------------------------
// STEP 2 — the blast-radius assertion (the core of the spike)
// ---------------------------------------------------------------------------

#[test]
fn changing_cors_decision_yields_exact_blast_radius() {
    let g = build_intent_graph();

    let blast = g.closure_of_dependents(&node(DECISION_CORS_POLICY));

    // EXACTLY the four CORS-dependent nodes, transitively:
    //   direct:   endpoint_api_chat, middleware_cors
    //   2 hops:   test_cors_smoke (via endpoint+middleware), deploy_worker (via endpoint)
    let expected: std::collections::HashSet<CellCoordinate> = [
        node(ENDPOINT_API_CHAT),
        node(MIDDLEWARE_CORS),
        node(TEST_CORS_SMOKE),
        node(DEPLOY_WORKER),
    ]
    .into_iter()
    .collect();

    let got: std::collections::HashSet<CellCoordinate> = blast.into_iter().collect();

    assert_eq!(
        got, expected,
        "blast radius of changing the CORS decision must be EXACTLY the 4 \
         transitively-dependent nodes"
    );
}

#[test]
fn cors_blast_radius_excludes_unrelated_decision_subgraph() {
    let g = build_intent_graph();
    let blast = g.closure_of_dependents(&node(DECISION_CORS_POLICY));

    // The "no hardcoded assets" subgraph depends on a DIFFERENT decision —
    // it must be absent from the CORS blast radius. This is the property
    // that makes it impact analysis and not "return everything."
    assert!(
        !blast.contains(&node(COMPONENT_FOLDER_VIEW)),
        "folder view depends on a different decision; must not be in CORS blast radius"
    );
    assert!(
        !blast.contains(&node(COMPONENT_SIDEBAR)),
        "sidebar depends on a different decision; must not be in CORS blast radius"
    );
    // and the root decision itself is not in its own dependents
    assert!(
        !blast.contains(&node(DECISION_CORS_POLICY)),
        "the changed node itself is not in its own blast radius"
    );
}

#[test]
fn changing_other_decision_is_isolated() {
    let g = build_intent_graph();
    let blast = g.closure_of_dependents(&node(DECISION_NO_HARDCODED_ASSETS));

    let expected: std::collections::HashSet<CellCoordinate> =
        [node(COMPONENT_FOLDER_VIEW), node(COMPONENT_SIDEBAR)]
            .into_iter()
            .collect();
    let got: std::collections::HashSet<CellCoordinate> = blast.into_iter().collect();

    assert_eq!(
        got, expected,
        "the two decisions have disjoint blast radii — changing one cannot \
         silently affect the other's subgraph"
    );
}

// ---------------------------------------------------------------------------
// STEP 3 — the "why" / trace assertion
//
// read_with_trace is a Cube-level API (it needs a populated store). At the
// pure-graph level the equivalent of "why is X in the blast radius" is the
// edge chain X -> ... -> root via dependencies_of (forward edges). We walk
// it here to prove the chain is reconstructable and legible.
// ---------------------------------------------------------------------------

/// Walk forward edges from `start` and return the set of nodes reachable
/// (what `start` transitively depends ON) — the "why am I affected" chain.
fn why_chain(g: &DependencyGraph, start: u64) -> std::collections::HashSet<u64> {
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![node(start)];
    while let Some(c) = stack.pop() {
        for edge in g.dependencies_of(&c) {
            let to_id = edge.to.element_at(0).0;
            if seen.insert(to_id) {
                stack.push(edge.to.clone());
            }
        }
    }
    seen
}

#[test]
fn trace_explains_why_a_node_is_affected() {
    let g = build_intent_graph();

    // Why is the CORS smoke test in the blast radius?
    // → it depends on endpoint + middleware, which depend on the CORS decision.
    let chain = why_chain(&g, TEST_CORS_SMOKE);

    assert!(chain.contains(&ENDPOINT_API_CHAT), "test depends on endpoint");
    assert!(chain.contains(&MIDDLEWARE_CORS), "test depends on middleware");
    assert!(
        chain.contains(&DECISION_CORS_POLICY),
        "test transitively depends on the CORS decision — that's WHY it's in the blast radius"
    );
    // and it must NOT trace to the unrelated decision
    assert!(
        !chain.contains(&DECISION_NO_HARDCODED_ASSETS),
        "the smoke test has nothing to do with the hardcoded-assets decision"
    );
}

#[test]
fn deploy_worker_traces_to_cors_decision_two_hops() {
    let g = build_intent_graph();
    let chain = why_chain(&g, DEPLOY_WORKER);
    // deploy -> endpoint -> decision (2 hops); proves transitive trace
    assert!(chain.contains(&ENDPOINT_API_CHAT));
    assert!(
        chain.contains(&DECISION_CORS_POLICY),
        "deploy_worker is in the blast radius transitively via the endpoint"
    );
    assert!(!chain.contains(&MIDDLEWARE_CORS), "deploy doesn't depend on middleware directly");
}
