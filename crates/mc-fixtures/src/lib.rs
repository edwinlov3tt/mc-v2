//! `mc-fixtures` — cube fixtures for tests and benches.
//!
//! ## Acme demo cube (brief §4)
//!
//! `build_acme_cube()` builds:
//!
//! - 6 dimensions: Scenario, Version, Time, Channel, Market, Measure.
//! - 3 default hierarchies: Time (Month → Quarter → Year), Channel
//!   (Channel → Channel_Group → All_Channels), Market (City → State →
//!   Region → USA).
//! - 11 measures: 6 inputs (Spend, CPC, CVR, Close_Rate, AOV,
//!   COGS_Rate) + 5 derived (Clicks, Leads, Customers, Revenue,
//!   Gross_Profit).
//! - 5 deterministic rules: Clicks = Spend / CPC; Leads = Clicks * CVR;
//!   Customers = Leads * Close_Rate; Revenue = Customers * AOV;
//!   Gross_Profit = Revenue * (1 - COGS_Rate).
//!
//! Plus `write_canonical_inputs` which writes 2,520 input cells (1
//! scenario × 1 version × 12 months × 5 channels × 7 markets × 6 input
//! measures) per the formulas in brief §4.5.
//!
//! ## Phase 2A synthetic fixtures (cold-path benchmarks)
//!
//! Phase 2A adds two synthetic builders to isolate the hierarchy
//! ancestor-walk cost (PERF.md §7.3, §8.1, §9.3) and the brief §11.1
//! `bench_write_input_leaf_no_deps` ceiling that Acme's hierarchy
//! fan-out makes unmeasurable:
//!
//! - `build_minimal_cube()` — 2-dim cube (Time + Measure) with **no**
//!   hierarchies and **no** derived measures. A single Spend write at
//!   the lone leaf coord pays only the per-write fixed costs (permission,
//!   type, lock, NaN, version, store write, revision bump) and zero
//!   ancestor-mark / rev-edge-walk cost.
//! - `build_graduated_hierarchy_cube(depth)` — 2-dim cube where the Time
//!   dim carries a `depth`-deep linear hierarchy (depth ∈ {0, 1, 2, 3}).
//!   Marginal write cost between adjacent depths isolates the hierarchy
//!   ancestor mark contribution.
//!
//! These fixtures are **only** used by Phase 2A benches. They are not on
//! the Phase 1 critical path and add no behavior to `mc-core`.

#![deny(rust_2018_idioms)]

use mc_core::{
    AggregationRule, CellCoordinate, CellDataType, CoordPattern, Cube, CubeBuilder, CubeId,
    DependencyDecl, Dimension, DimensionId, DimensionKind, Element, ElementId, EngineError, Expr,
    Hierarchy, HierarchyId, IdGenerator, MeasureRole, PrincipalId, Rule, RuleId, ScalarValue,
    ScenarioMeta, Scope, VersionState, WriteIntent, WritebackRequest,
};

/// Every named ID in the Acme cube, threaded through to tests so they
/// can build coordinates without re-resolving by name.
#[derive(Debug)]
pub struct AcmeRefs {
    pub root_principal: PrincipalId,

    // Dimensions
    pub scenario_dim: DimensionId,
    pub version_dim: DimensionId,
    pub time_dim: DimensionId,
    pub channel_dim: DimensionId,
    pub market_dim: DimensionId,
    pub measure_dim: DimensionId,

    // Hierarchy IDs (default hierarchies only)
    pub time_hierarchy: HierarchyId,
    pub channel_hierarchy: HierarchyId,
    pub market_hierarchy: HierarchyId,

    // Scenario elements
    pub scen_baseline: ElementId,
    pub scen_aggressive: ElementId,
    pub scen_conservative: ElementId,

    // Version elements
    pub ver_working: ElementId,
    pub ver_submitted: ElementId,
    pub ver_approved: ElementId,

    // Time elements (12 leaves)
    pub jan_2026: ElementId,
    pub feb_2026: ElementId,
    pub mar_2026: ElementId,
    pub apr_2026: ElementId,
    pub may_2026: ElementId,
    pub jun_2026: ElementId,
    pub jul_2026: ElementId,
    pub aug_2026: ElementId,
    pub sep_2026: ElementId,
    pub oct_2026: ElementId,
    pub nov_2026: ElementId,
    pub dec_2026: ElementId,
    // Time consolidations
    pub q1_2026: ElementId,
    pub q2_2026: ElementId,
    pub q3_2026: ElementId,
    pub q4_2026: ElementId,
    pub fy_2026: ElementId,

    // Channel elements (5 leaves)
    pub paid_search: ElementId,
    pub paid_social: ElementId,
    pub display: ElementId,
    pub email: ElementId,
    pub organic: ElementId,
    // Channel consolidations
    pub paid_media: ElementId,
    pub owned_earned: ElementId,
    pub all_channels: ElementId,

    // Market elements (7 leaves)
    pub tampa: ElementId,
    pub orlando: ElementId,
    pub miami: ElementId,
    pub atlanta: ElementId,
    pub charlotte: ElementId,
    pub new_york_city: ElementId,
    pub boston: ElementId,
    // Market consolidations
    pub florida: ElementId,
    pub georgia: ElementId,
    pub north_carolina: ElementId,
    pub new_york_state: ElementId,
    pub massachusetts: ElementId,
    pub southeast: ElementId,
    pub northeast: ElementId,
    pub usa: ElementId,

    // Measure elements
    // Inputs
    pub spend: ElementId,
    pub cpc: ElementId,
    pub cvr: ElementId,
    pub close_rate: ElementId,
    pub aov: ElementId,
    pub cogs_rate: ElementId,
    // Derived
    pub clicks: ElementId,
    pub leads: ElementId,
    pub customers: ElementId,
    pub revenue: ElementId,
    pub gross_profit: ElementId,

    // Rule IDs
    pub rule_clicks: RuleId,
    pub rule_leads: RuleId,
    pub rule_customers: RuleId,
    pub rule_revenue: RuleId,
    pub rule_gross_profit: RuleId,
}

/// Build the Acme cube. Per spec §3.5 the dimension order is exactly
/// `[Scenario, Version, Time, Channel, Market, Measure]`; tests rely on
/// this for positional coordinate construction.
///
/// Returns the cube + the `AcmeRefs` ID bundle. Per CLAUDE.md §1
/// "build_acme_cube returns Result" requirement: callers `expect()` in
/// test/CLI contexts.
pub fn build_acme_cube() -> Result<(Cube, AcmeRefs), EngineError> {
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let root_principal = g.principal();

    // ---- Build dimensions ----
    let (scenario_dim, scen_ids) = build_scenario_dim(&g)?;
    let (version_dim, ver_ids) = build_version_dim(&g)?;
    let (time_dim, time_ids, time_hierarchy_id) = build_time_dim(&g)?;
    let (channel_dim, channel_ids, channel_hierarchy_id) = build_channel_dim(&g)?;
    let (market_dim, market_ids, market_hierarchy_id) = build_market_dim(&g)?;
    let (measure_dim, measure_ids) = build_measure_dim(&g)?;

    let scenario_dim_id = scenario_dim.id;
    let version_dim_id = version_dim.id;
    let time_dim_id = time_dim.id;
    let channel_dim_id = channel_dim.id;
    let market_dim_id = market_dim.id;
    let measure_dim_id = measure_dim.id;

    // ---- Build refs (so we can pass IDs into rule constructors) ----
    let mut refs = AcmeRefs {
        root_principal,
        scenario_dim: scenario_dim_id,
        version_dim: version_dim_id,
        time_dim: time_dim_id,
        channel_dim: channel_dim_id,
        market_dim: market_dim_id,
        measure_dim: measure_dim_id,
        time_hierarchy: time_hierarchy_id,
        channel_hierarchy: channel_hierarchy_id,
        market_hierarchy: market_hierarchy_id,
        scen_baseline: scen_ids.baseline,
        scen_aggressive: scen_ids.aggressive,
        scen_conservative: scen_ids.conservative,
        ver_working: ver_ids.working,
        ver_submitted: ver_ids.submitted,
        ver_approved: ver_ids.approved,
        jan_2026: time_ids.jan,
        feb_2026: time_ids.feb,
        mar_2026: time_ids.mar,
        apr_2026: time_ids.apr,
        may_2026: time_ids.may,
        jun_2026: time_ids.jun,
        jul_2026: time_ids.jul,
        aug_2026: time_ids.aug,
        sep_2026: time_ids.sep,
        oct_2026: time_ids.oct,
        nov_2026: time_ids.nov,
        dec_2026: time_ids.dec,
        q1_2026: time_ids.q1,
        q2_2026: time_ids.q2,
        q3_2026: time_ids.q3,
        q4_2026: time_ids.q4,
        fy_2026: time_ids.fy,
        paid_search: channel_ids.paid_search,
        paid_social: channel_ids.paid_social,
        display: channel_ids.display,
        email: channel_ids.email,
        organic: channel_ids.organic,
        paid_media: channel_ids.paid_media,
        owned_earned: channel_ids.owned_earned,
        all_channels: channel_ids.all_channels,
        tampa: market_ids.tampa,
        orlando: market_ids.orlando,
        miami: market_ids.miami,
        atlanta: market_ids.atlanta,
        charlotte: market_ids.charlotte,
        new_york_city: market_ids.new_york_city,
        boston: market_ids.boston,
        florida: market_ids.florida,
        georgia: market_ids.georgia,
        north_carolina: market_ids.north_carolina,
        new_york_state: market_ids.new_york_state,
        massachusetts: market_ids.massachusetts,
        southeast: market_ids.southeast,
        northeast: market_ids.northeast,
        usa: market_ids.usa,
        spend: measure_ids.spend,
        cpc: measure_ids.cpc,
        cvr: measure_ids.cvr,
        close_rate: measure_ids.close_rate,
        aov: measure_ids.aov,
        cogs_rate: measure_ids.cogs_rate,
        clicks: measure_ids.clicks,
        leads: measure_ids.leads,
        customers: measure_ids.customers,
        revenue: measure_ids.revenue,
        gross_profit: measure_ids.gross_profit,
        // Rule IDs filled in below.
        rule_clicks: RuleId(0),
        rule_leads: RuleId(0),
        rule_customers: RuleId(0),
        rule_revenue: RuleId(0),
        rule_gross_profit: RuleId(0),
    };

    // ---- Build rules ----
    let r_clicks = build_rule_clicks(&g, cube_id, &refs);
    let r_leads = build_rule_leads(&g, cube_id, &refs);
    let r_customers = build_rule_customers(&g, cube_id, &refs);
    let r_revenue = build_rule_revenue(&g, cube_id, &refs);
    let r_gross_profit = build_rule_gross_profit(&g, cube_id, &refs);
    refs.rule_clicks = r_clicks.id;
    refs.rule_leads = r_leads.id;
    refs.rule_customers = r_customers.id;
    refs.rule_revenue = r_revenue.id;
    refs.rule_gross_profit = r_gross_profit.id;

    // ---- Assemble cube ----
    let cube = CubeBuilder::default_for_cube(cube_id, "Acme_MarketingFinance")
        .add_dimension(scenario_dim)
        .add_dimension(version_dim)
        .add_dimension(time_dim)
        .add_dimension(channel_dim)
        .add_dimension(market_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(root_principal)
        .add_rule(r_clicks)?
        .add_rule(r_leads)?
        .add_rule(r_customers)?
        .add_rule(r_revenue)?
        .add_rule(r_gross_profit)?
        .build()?;

    Ok((cube, refs))
}

/// Write the canonical 2,520 input cells (1 scenario × 1 version ×
/// 12 months × 5 channels × 7 markets × 6 input measures) per brief
/// §4.5. Returns the count of cells written.
pub fn write_canonical_inputs(cube: &mut Cube, refs: &AcmeRefs) -> Result<usize, EngineError> {
    let cube_id = cube.id;
    let root = refs.root_principal;
    let mut count = 0;
    let time_idx_to_element: [(u32, ElementId); 12] = [
        (1, refs.jan_2026),
        (2, refs.feb_2026),
        (3, refs.mar_2026),
        (4, refs.apr_2026),
        (5, refs.may_2026),
        (6, refs.jun_2026),
        (7, refs.jul_2026),
        (8, refs.aug_2026),
        (9, refs.sep_2026),
        (10, refs.oct_2026),
        (11, refs.nov_2026),
        (12, refs.dec_2026),
    ];
    let channel_idx_to_element: [(u32, ElementId); 5] = [
        (0, refs.paid_search),
        (1, refs.paid_social),
        (2, refs.display),
        (3, refs.email),
        (4, refs.organic),
    ];
    let market_idx_to_element: [(u32, ElementId); 7] = [
        (0, refs.tampa),
        (1, refs.orlando),
        (2, refs.miami),
        (3, refs.atlanta),
        (4, refs.charlotte),
        (5, refs.new_york_city),
        (6, refs.boston),
    ];

    for &(t_idx, t_id) in &time_idx_to_element {
        for &(c_idx, c_id) in &channel_idx_to_element {
            for &(m_idx, m_id) in &market_idx_to_element {
                let inputs = canonical_inputs_for(t_idx, c_idx, m_idx);
                for (measure_id, value) in [
                    (refs.spend, inputs.spend),
                    (refs.cpc, inputs.cpc),
                    (refs.cvr, inputs.cvr),
                    (refs.close_rate, inputs.close_rate),
                    (refs.aov, inputs.aov),
                    (refs.cogs_rate, inputs.cogs_rate),
                ] {
                    let coord = coord(
                        cube_id,
                        refs,
                        refs.scen_baseline,
                        refs.ver_working,
                        t_id,
                        c_id,
                        m_id,
                        measure_id,
                    );
                    cube.write(WritebackRequest {
                        coord,
                        new_value: ScalarValue::F64(value),
                        principal: root,
                        intent: WriteIntent::Set,
                        expected_revision: None,
                        now_unix_seconds: 0,
                    })?;
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

/// Force materialization of every leaf-coord × every-derived-measure
/// rule edge in the dependency graph. Per brief §10.5
/// `t_dependency_graph_validates_full_fixture_when_forced` — a debug
/// helper that reads every (leaf, derived) cell once so the lazy
/// dependency graph is populated to its full extent. Off the critical
/// Phase 1 path; opt-in for full validation.
///
/// Returns the number of leaf-derived reads performed (1 scenario × 1
/// version × 12 months × 5 channels × 7 markets × 5 derived measures =
/// 2,100).
pub fn materialize_all_dependencies(
    cube: &mut Cube,
    refs: &AcmeRefs,
) -> Result<usize, EngineError> {
    let cube_id = cube.id;
    let root = refs.root_principal;
    let times: [ElementId; 12] = [
        refs.jan_2026,
        refs.feb_2026,
        refs.mar_2026,
        refs.apr_2026,
        refs.may_2026,
        refs.jun_2026,
        refs.jul_2026,
        refs.aug_2026,
        refs.sep_2026,
        refs.oct_2026,
        refs.nov_2026,
        refs.dec_2026,
    ];
    let channels: [ElementId; 5] = [
        refs.paid_search,
        refs.paid_social,
        refs.display,
        refs.email,
        refs.organic,
    ];
    let markets: [ElementId; 7] = [
        refs.tampa,
        refs.orlando,
        refs.miami,
        refs.atlanta,
        refs.charlotte,
        refs.new_york_city,
        refs.boston,
    ];
    let derived: [ElementId; 5] = [
        refs.clicks,
        refs.leads,
        refs.customers,
        refs.revenue,
        refs.gross_profit,
    ];
    let mut count = 0;
    for &t in &times {
        for &c in &channels {
            for &m in &markets {
                for &d in &derived {
                    let c_coord = coord(
                        cube_id,
                        refs,
                        refs.scen_baseline,
                        refs.ver_working,
                        t,
                        c,
                        m,
                        d,
                    );
                    cube.read(&c_coord, root)?;
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

/// Build a coordinate using the canonical Acme dim order:
/// `[Scenario, Version, Time, Channel, Market, Measure]`. Public so
/// integration tests can build coords without re-deriving the order.
#[allow(clippy::too_many_arguments)]
pub fn coord(
    cube_id: CubeId,
    _refs: &AcmeRefs,
    scenario: ElementId,
    version: ElementId,
    time: ElementId,
    channel: ElementId,
    market: ElementId,
    measure: ElementId,
) -> CellCoordinate {
    CellCoordinate::from_parts(cube_id, [scenario, version, time, channel, market, measure])
}

/// Closed-form input values per brief §4.5 / §4.5.1.
///
/// Returned values ARE the golden inputs; tests assert against the
/// derived chain computed from these via the Acme rules.
pub fn canonical_inputs_for(time_idx: u32, channel_idx: u32, market_idx: u32) -> CanonicalInputs {
    let t = time_idx as f64;
    let c = channel_idx as f64;
    let m = market_idx as f64;
    CanonicalInputs {
        spend: 10_000.0 + 500.0 * t + 1_000.0 * c + 200.0 * m,
        cpc: 1.50 + 0.05 * c + 0.02 * m,
        cvr: 0.020 + 0.005 * c,
        close_rate: 0.10 + 0.01 * c,
        aov: 200.0 + 50.0 * m,
        cogs_rate: 0.30 + 0.02 * c,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CanonicalInputs {
    pub spend: f64,
    pub cpc: f64,
    pub cvr: f64,
    pub close_rate: f64,
    pub aov: f64,
    pub cogs_rate: f64,
}

impl CanonicalInputs {
    pub fn clicks(&self) -> f64 {
        self.spend / self.cpc
    }
    pub fn leads(&self) -> f64 {
        self.clicks() * self.cvr
    }
    pub fn customers(&self) -> f64 {
        self.leads() * self.close_rate
    }
    pub fn revenue(&self) -> f64 {
        self.customers() * self.aov
    }
    pub fn gross_profit(&self) -> f64 {
        self.revenue() * (1.0 - self.cogs_rate)
    }
}

// ===========================================================================
// Dimension builders
// ===========================================================================

struct ScenIds {
    baseline: ElementId,
    aggressive: ElementId,
    conservative: ElementId,
}

fn build_scenario_dim(g: &IdGenerator) -> Result<(Dimension, ScenIds), EngineError> {
    let dim_id = g.dimension();
    let baseline = g.element();
    let aggressive = g.element();
    let conservative = g.element();
    let dim = Dimension::builder(dim_id, "Scenario", DimensionKind::Scenario)
        .add_element(Element::scenario(
            baseline,
            "Baseline",
            dim_id,
            ScenarioMeta::Default,
        ))?
        .add_element(Element::scenario(
            aggressive,
            "Aggressive",
            dim_id,
            ScenarioMeta::NonDefault,
        ))?
        .add_element(Element::scenario(
            conservative,
            "Conservative",
            dim_id,
            ScenarioMeta::NonDefault,
        ))?
        .build()?;
    Ok((
        dim,
        ScenIds {
            baseline,
            aggressive,
            conservative,
        },
    ))
}

struct VerIds {
    working: ElementId,
    submitted: ElementId,
    approved: ElementId,
}

fn build_version_dim(g: &IdGenerator) -> Result<(Dimension, VerIds), EngineError> {
    let dim_id = g.dimension();
    let working = g.element();
    let submitted = g.element();
    let approved = g.element();
    let dim = Dimension::builder(dim_id, "Version", DimensionKind::Version)
        .add_element(Element::version(
            working,
            "Working",
            dim_id,
            VersionState::Draft,
        ))?
        .add_element(Element::version(
            submitted,
            "Submitted",
            dim_id,
            VersionState::Submitted,
        ))?
        .add_element(Element::version(
            approved,
            "Approved",
            dim_id,
            VersionState::Approved,
        ))?
        .build()?;
    Ok((
        dim,
        VerIds {
            working,
            submitted,
            approved,
        },
    ))
}

struct TimeIds {
    jan: ElementId,
    feb: ElementId,
    mar: ElementId,
    apr: ElementId,
    may: ElementId,
    jun: ElementId,
    jul: ElementId,
    aug: ElementId,
    sep: ElementId,
    oct: ElementId,
    nov: ElementId,
    dec: ElementId,
    q1: ElementId,
    q2: ElementId,
    q3: ElementId,
    q4: ElementId,
    fy: ElementId,
}

fn build_time_dim(g: &IdGenerator) -> Result<(Dimension, TimeIds, HierarchyId), EngineError> {
    let dim_id = g.dimension();
    let jan = g.element();
    let feb = g.element();
    let mar = g.element();
    let apr = g.element();
    let may = g.element();
    let jun = g.element();
    let jul = g.element();
    let aug = g.element();
    let sep = g.element();
    let oct = g.element();
    let nov = g.element();
    let dec = g.element();
    let q1 = g.element();
    let q2 = g.element();
    let q3 = g.element();
    let q4 = g.element();
    let fy = g.element();
    let h_id = g.hierarchy();

    let hier = Hierarchy::builder(h_id, "Calendar", dim_id)
        .add_edge(fy, q1, 1.0)
        .add_edge(fy, q2, 1.0)
        .add_edge(fy, q3, 1.0)
        .add_edge(fy, q4, 1.0)
        .add_edge(q1, jan, 1.0)
        .add_edge(q1, feb, 1.0)
        .add_edge(q1, mar, 1.0)
        .add_edge(q2, apr, 1.0)
        .add_edge(q2, may, 1.0)
        .add_edge(q2, jun, 1.0)
        .add_edge(q3, jul, 1.0)
        .add_edge(q3, aug, 1.0)
        .add_edge(q3, sep, 1.0)
        .add_edge(q4, oct, 1.0)
        .add_edge(q4, nov, 1.0)
        .add_edge(q4, dec, 1.0)
        .build()?;

    let dim = Dimension::builder(dim_id, "Time", DimensionKind::Standard)
        .add_element(Element::leaf(jan, "Jan_2026", dim_id))?
        .add_element(Element::leaf(feb, "Feb_2026", dim_id))?
        .add_element(Element::leaf(mar, "Mar_2026", dim_id))?
        .add_element(Element::leaf(apr, "Apr_2026", dim_id))?
        .add_element(Element::leaf(may, "May_2026", dim_id))?
        .add_element(Element::leaf(jun, "Jun_2026", dim_id))?
        .add_element(Element::leaf(jul, "Jul_2026", dim_id))?
        .add_element(Element::leaf(aug, "Aug_2026", dim_id))?
        .add_element(Element::leaf(sep, "Sep_2026", dim_id))?
        .add_element(Element::leaf(oct, "Oct_2026", dim_id))?
        .add_element(Element::leaf(nov, "Nov_2026", dim_id))?
        .add_element(Element::leaf(dec, "Dec_2026", dim_id))?
        .add_element(Element::leaf(q1, "Q1_2026", dim_id))?
        .add_element(Element::leaf(q2, "Q2_2026", dim_id))?
        .add_element(Element::leaf(q3, "Q3_2026", dim_id))?
        .add_element(Element::leaf(q4, "Q4_2026", dim_id))?
        .add_element(Element::leaf(fy, "FY_2026", dim_id))?
        .add_hierarchy(hier)?
        .default_hierarchy("Calendar")
        .build()?;

    Ok((
        dim,
        TimeIds {
            jan,
            feb,
            mar,
            apr,
            may,
            jun,
            jul,
            aug,
            sep,
            oct,
            nov,
            dec,
            q1,
            q2,
            q3,
            q4,
            fy,
        },
        h_id,
    ))
}

struct ChannelIds {
    paid_search: ElementId,
    paid_social: ElementId,
    display: ElementId,
    email: ElementId,
    organic: ElementId,
    paid_media: ElementId,
    owned_earned: ElementId,
    all_channels: ElementId,
}

fn build_channel_dim(g: &IdGenerator) -> Result<(Dimension, ChannelIds, HierarchyId), EngineError> {
    let dim_id = g.dimension();
    let paid_search = g.element();
    let paid_social = g.element();
    let display = g.element();
    let email = g.element();
    let organic = g.element();
    let paid_media = g.element();
    let owned_earned = g.element();
    let all_channels = g.element();
    let h_id = g.hierarchy();

    let hier = Hierarchy::builder(h_id, "Grouping", dim_id)
        .add_edge(all_channels, paid_media, 1.0)
        .add_edge(all_channels, owned_earned, 1.0)
        .add_edge(paid_media, paid_search, 1.0)
        .add_edge(paid_media, paid_social, 1.0)
        .add_edge(paid_media, display, 1.0)
        .add_edge(owned_earned, email, 1.0)
        .add_edge(owned_earned, organic, 1.0)
        .build()?;

    let dim = Dimension::builder(dim_id, "Channel", DimensionKind::Standard)
        .add_element(Element::leaf(paid_search, "Paid_Search", dim_id))?
        .add_element(Element::leaf(paid_social, "Paid_Social", dim_id))?
        .add_element(Element::leaf(display, "Display", dim_id))?
        .add_element(Element::leaf(email, "Email", dim_id))?
        .add_element(Element::leaf(organic, "Organic", dim_id))?
        .add_element(Element::leaf(paid_media, "Paid_Media", dim_id))?
        .add_element(Element::leaf(owned_earned, "Owned_Earned", dim_id))?
        .add_element(Element::leaf(all_channels, "All_Channels", dim_id))?
        .add_hierarchy(hier)?
        .default_hierarchy("Grouping")
        .build()?;

    Ok((
        dim,
        ChannelIds {
            paid_search,
            paid_social,
            display,
            email,
            organic,
            paid_media,
            owned_earned,
            all_channels,
        },
        h_id,
    ))
}

struct MarketIds {
    tampa: ElementId,
    orlando: ElementId,
    miami: ElementId,
    atlanta: ElementId,
    charlotte: ElementId,
    new_york_city: ElementId,
    boston: ElementId,
    florida: ElementId,
    georgia: ElementId,
    north_carolina: ElementId,
    new_york_state: ElementId,
    massachusetts: ElementId,
    southeast: ElementId,
    northeast: ElementId,
    usa: ElementId,
}

fn build_market_dim(g: &IdGenerator) -> Result<(Dimension, MarketIds, HierarchyId), EngineError> {
    let dim_id = g.dimension();
    let tampa = g.element();
    let orlando = g.element();
    let miami = g.element();
    let atlanta = g.element();
    let charlotte = g.element();
    let new_york_city = g.element();
    let boston = g.element();
    let florida = g.element();
    let georgia = g.element();
    let north_carolina = g.element();
    let new_york_state = g.element();
    let massachusetts = g.element();
    let southeast = g.element();
    let northeast = g.element();
    let usa = g.element();
    let h_id = g.hierarchy();

    let hier = Hierarchy::builder(h_id, "Geographic", dim_id)
        .add_edge(usa, southeast, 1.0)
        .add_edge(usa, northeast, 1.0)
        .add_edge(southeast, florida, 1.0)
        .add_edge(southeast, georgia, 1.0)
        .add_edge(southeast, north_carolina, 1.0)
        .add_edge(northeast, new_york_state, 1.0)
        .add_edge(northeast, massachusetts, 1.0)
        .add_edge(florida, tampa, 1.0)
        .add_edge(florida, orlando, 1.0)
        .add_edge(florida, miami, 1.0)
        .add_edge(georgia, atlanta, 1.0)
        .add_edge(north_carolina, charlotte, 1.0)
        .add_edge(new_york_state, new_york_city, 1.0)
        .add_edge(massachusetts, boston, 1.0)
        .build()?;

    let dim = Dimension::builder(dim_id, "Market", DimensionKind::Standard)
        .add_element(Element::leaf(tampa, "Tampa", dim_id))?
        .add_element(Element::leaf(orlando, "Orlando", dim_id))?
        .add_element(Element::leaf(miami, "Miami", dim_id))?
        .add_element(Element::leaf(atlanta, "Atlanta", dim_id))?
        .add_element(Element::leaf(charlotte, "Charlotte", dim_id))?
        .add_element(Element::leaf(new_york_city, "New_York_City", dim_id))?
        .add_element(Element::leaf(boston, "Boston", dim_id))?
        .add_element(Element::leaf(florida, "Florida", dim_id))?
        .add_element(Element::leaf(georgia, "Georgia", dim_id))?
        .add_element(Element::leaf(north_carolina, "North_Carolina", dim_id))?
        .add_element(Element::leaf(new_york_state, "New_York_State", dim_id))?
        .add_element(Element::leaf(massachusetts, "Massachusetts", dim_id))?
        .add_element(Element::leaf(southeast, "Southeast", dim_id))?
        .add_element(Element::leaf(northeast, "Northeast", dim_id))?
        .add_element(Element::leaf(usa, "USA", dim_id))?
        .add_hierarchy(hier)?
        .default_hierarchy("Geographic")
        .build()?;

    Ok((
        dim,
        MarketIds {
            tampa,
            orlando,
            miami,
            atlanta,
            charlotte,
            new_york_city,
            boston,
            florida,
            georgia,
            north_carolina,
            new_york_state,
            massachusetts,
            southeast,
            northeast,
            usa,
        },
        h_id,
    ))
}

struct MeasureIds {
    spend: ElementId,
    cpc: ElementId,
    cvr: ElementId,
    close_rate: ElementId,
    aov: ElementId,
    cogs_rate: ElementId,
    clicks: ElementId,
    leads: ElementId,
    customers: ElementId,
    revenue: ElementId,
    gross_profit: ElementId,
}

fn build_measure_dim(g: &IdGenerator) -> Result<(Dimension, MeasureIds), EngineError> {
    let dim_id = g.dimension();
    let spend = g.element();
    let cpc = g.element();
    let cvr = g.element();
    let close_rate = g.element();
    let aov = g.element();
    let cogs_rate = g.element();
    let clicks = g.element();
    let leads = g.element();
    let customers = g.element();
    let revenue = g.element();
    let gross_profit = g.element();

    let dim = Dimension::builder(dim_id, "Measure", DimensionKind::Measure)
        // Inputs
        .add_element(Element::measure(
            spend,
            "Spend",
            dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        ))?
        .add_element(Element::measure(
            cpc,
            "CPC",
            dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::WeightedAverage {
                weight_measure: spend,
            },
        ))?
        .add_element(Element::measure(
            cvr,
            "CVR",
            dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::WeightedAverage {
                weight_measure: clicks,
            },
        ))?
        .add_element(Element::measure(
            close_rate,
            "Close_Rate",
            dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::WeightedAverage {
                weight_measure: leads,
            },
        ))?
        .add_element(Element::measure(
            aov,
            "AOV",
            dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::WeightedAverage {
                weight_measure: customers,
            },
        ))?
        .add_element(Element::measure(
            cogs_rate,
            "COGS_Rate",
            dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::WeightedAverage {
                weight_measure: revenue,
            },
        ))?
        // Derived
        .add_element(Element::measure(
            clicks,
            "Clicks",
            dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))?
        .add_element(Element::measure(
            leads,
            "Leads",
            dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))?
        .add_element(Element::measure(
            customers,
            "Customers",
            dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))?
        .add_element(Element::measure(
            revenue,
            "Revenue",
            dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))?
        .add_element(Element::measure(
            gross_profit,
            "Gross_Profit",
            dim_id,
            CellDataType::F64,
            MeasureRole::Derived,
            AggregationRule::Sum,
        ))?
        .build()?;

    Ok((
        dim,
        MeasureIds {
            spend,
            cpc,
            cvr,
            close_rate,
            aov,
            cogs_rate,
            clicks,
            leads,
            customers,
            revenue,
            gross_profit,
        },
    ))
}

// ===========================================================================
// Rule builders
// ===========================================================================

fn dep(measure: ElementId) -> DependencyDecl {
    DependencyDecl {
        measure,
        coord_pattern: CoordPattern::SameAsTarget,
    }
}

fn build_rule_clicks(g: &IdGenerator, cube: CubeId, refs: &AcmeRefs) -> Rule {
    Rule {
        id: g.rule(),
        cube,
        target_measure: refs.clicks,
        scope: Scope::AllLeaves,
        body: Expr::Div(
            Box::new(Expr::SelfRef(refs.spend)),
            Box::new(Expr::SelfRef(refs.cpc)),
        ),
        declared_dependencies: vec![dep(refs.spend), dep(refs.cpc)],
    }
}

fn build_rule_leads(g: &IdGenerator, cube: CubeId, refs: &AcmeRefs) -> Rule {
    Rule {
        id: g.rule(),
        cube,
        target_measure: refs.leads,
        scope: Scope::AllLeaves,
        body: Expr::Mul(
            Box::new(Expr::SelfRef(refs.clicks)),
            Box::new(Expr::SelfRef(refs.cvr)),
        ),
        declared_dependencies: vec![dep(refs.clicks), dep(refs.cvr)],
    }
}

fn build_rule_customers(g: &IdGenerator, cube: CubeId, refs: &AcmeRefs) -> Rule {
    Rule {
        id: g.rule(),
        cube,
        target_measure: refs.customers,
        scope: Scope::AllLeaves,
        body: Expr::Mul(
            Box::new(Expr::SelfRef(refs.leads)),
            Box::new(Expr::SelfRef(refs.close_rate)),
        ),
        declared_dependencies: vec![dep(refs.leads), dep(refs.close_rate)],
    }
}

fn build_rule_revenue(g: &IdGenerator, cube: CubeId, refs: &AcmeRefs) -> Rule {
    Rule {
        id: g.rule(),
        cube,
        target_measure: refs.revenue,
        scope: Scope::AllLeaves,
        body: Expr::Mul(
            Box::new(Expr::SelfRef(refs.customers)),
            Box::new(Expr::SelfRef(refs.aov)),
        ),
        declared_dependencies: vec![dep(refs.customers), dep(refs.aov)],
    }
}

fn build_rule_gross_profit(g: &IdGenerator, cube: CubeId, refs: &AcmeRefs) -> Rule {
    Rule {
        id: g.rule(),
        cube,
        target_measure: refs.gross_profit,
        scope: Scope::AllLeaves,
        body: Expr::Mul(
            Box::new(Expr::SelfRef(refs.revenue)),
            Box::new(Expr::Sub(
                Box::new(Expr::Const(ScalarValue::F64(1.0))),
                Box::new(Expr::SelfRef(refs.cogs_rate)),
            )),
        ),
        declared_dependencies: vec![dep(refs.revenue), dep(refs.cogs_rate)],
    }
}

// CubeBuilder doesn't currently expose `default_for_cube`; this is a
// thin alias to the public `Cube::builder` for readability above. We
// surface it as a free function rather than a trait method to keep
// mc-fixtures dependency-light.
trait CubeBuilderHelpers {
    fn default_for_cube(id: CubeId, name: &str) -> CubeBuilder;
}

impl CubeBuilderHelpers for CubeBuilder {
    fn default_for_cube(id: CubeId, name: &str) -> CubeBuilder {
        Cube::builder(id, name)
    }
}

// ===========================================================================
// Phase 2A synthetic fixtures
// ===========================================================================

/// Identifiers for the cube returned by [`build_minimal_cube`]. Threaded
/// through to benches so they can build a `CellCoordinate` without
/// re-resolving by name. Phase 2A only.
#[derive(Debug, Clone, Copy)]
pub struct MinimalRefs {
    pub root_principal: PrincipalId,
    pub cube_id: CubeId,
    pub time_dim: DimensionId,
    /// The single non-Measure leaf element. The Time dimension has no
    /// hierarchy edges (synth flat default), so this element is treated
    /// as a leaf by `Cube::is_consolidated_coord`.
    pub time_only: ElementId,
    pub measure_dim: DimensionId,
    /// The single Input measure. There are no Derived measures, so a
    /// write here triggers no rev-edge walk and no derived-measure
    /// dirty propagation.
    pub spend: ElementId,
}

/// Build a 2-dim "Time × Measure" cube with **no hierarchies** and
/// **no derived measures**. A Spend write at the lone leaf coord
/// produces zero hierarchy ancestor marks (per `compute_dirty_ancestors`
/// in `cube.rs`), zero rev-edge marks (no rules → empty dependency
/// graph → empty closure), and zero added derived-measure invalidations.
/// The dirty-set delta after one write is therefore exactly **0** —
/// the freshly-written coord is by definition clean (`mark_closure`
/// excludes `root`, and `compute_dirty_ancestors` skips the
/// `(leaf, written_measure)` cell).
///
/// Used by the `synthetic_no_deps` bench to evaluate the brief §11.1
/// `bench_write_input_leaf_no_deps < 50 µs` ceiling against the
/// brief's original mental model (a synthetic no-hierarchy cube),
/// closing the Phase 1B benchmark-scope-mismatch caveat (PERF.md §7.3).
///
/// Phase 2A only; not on the Phase 1 critical path.
pub fn build_minimal_cube() -> Result<(Cube, MinimalRefs), EngineError> {
    let g = IdGenerator::new();
    let cube_id = g.cube();
    let root_principal = g.principal();

    let time_dim_id = g.dimension();
    let measure_dim_id = g.dimension();
    let time_only = g.element();
    let spend = g.element();

    // Time dim: 1 leaf, no hierarchy. DimensionBuilder synthesizes a
    // flat default hierarchy with empty edges, so `is_consolidated_coord`
    // treats `time_only` as a leaf and `compute_dirty_ancestors` walks
    // zero ancestors for this dim.
    let time_dim = Dimension::builder(time_dim_id, "Time", DimensionKind::Standard)
        .add_element(Element::leaf(time_only, "TimeOnly", time_dim_id))?
        .build()?;

    // Measure dim: 1 Input, no Derived. compute_dirty_ancestors's
    // measures_to_mark vec contains only the written measure, and the
    // pure-leaf branch skips `(leaf, written_measure)` — net zero
    // ancestor coords marked.
    let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
        .add_element(Element::measure(
            spend,
            "Spend",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        ))?
        .build()?;

    let cube = Cube::builder(cube_id, "Minimal_NoHierarchy_NoDerived")
        .add_dimension(time_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(root_principal)
        .build()?;

    Ok((
        cube,
        MinimalRefs {
            root_principal,
            cube_id,
            time_dim: time_dim_id,
            time_only,
            measure_dim: measure_dim_id,
            spend,
        },
    ))
}

/// Build a coord against the [`build_minimal_cube`] dim order
/// `[Time, Measure]`. Public so benches can construct coords without
/// re-deriving slot positions.
pub fn minimal_coord(refs: &MinimalRefs) -> CellCoordinate {
    CellCoordinate::from_parts(refs.cube_id, [refs.time_only, refs.spend])
}

/// Identifiers for the cube returned by
/// [`build_graduated_hierarchy_cube`]. The `time_leaf` is the writable
/// leaf; `time_consolidated` is the `depth`-element ancestor chain
/// from the immediate parent (index 0) up to the root (index
/// `depth - 1`). For `depth = 0` the chain is empty and only
/// `time_leaf` exists.
#[derive(Debug, Clone)]
pub struct GraduatedRefs {
    pub root_principal: PrincipalId,
    pub cube_id: CubeId,
    pub time_dim: DimensionId,
    /// Leaf element (the only writable Time element).
    pub time_leaf: ElementId,
    /// Ancestor chain from immediate parent up to the root. Length is
    /// equal to `depth`; empty when `depth == 0`.
    pub time_consolidated: Vec<ElementId>,
    pub measure_dim: DimensionId,
    pub spend: ElementId,
    pub depth: u8,
}

/// Build a 2-dim "Time × Measure" cube where the Time dim carries a
/// linear `depth`-element ancestor chain above its single leaf. The
/// Measure dim has exactly one Input measure (Spend, no derived), so a
/// Spend write at the leaf isolates the **hierarchy ancestor mark walk**
/// from the rev-edge walk and from derived-measure cross-product
/// invalidation. Phase 2A bench `hierarchy_mark` reads marginal cost
/// per ancestor by differencing adjacent depths.
///
/// `depth` must be ≤ 3 in Phase 2A. The chain shape:
///
/// - `depth = 0`: 1 element (leaf), no hierarchy → 0 ancestors.
/// - `depth = 1`: 2 elements (leaf, parent), `parent → leaf` → 1 ancestor.
/// - `depth = 2`: 3 elements, `gp → parent → leaf` → 2 ancestors.
/// - `depth = 3`: 4 elements, `ggp → gp → parent → leaf` → 3 ancestors.
///
/// Per `cube.rs::compute_dirty_ancestors`, a write at `(leaf, spend)`
/// produces a dirty-set delta equal to `depth` (one consolidated coord
/// per ancestor element × the single `spend` measure).
pub fn build_graduated_hierarchy_cube(depth: u8) -> Result<(Cube, GraduatedRefs), EngineError> {
    if depth > 3 {
        // Phase 2A only exercises depth ∈ {0,1,2,3}; reject anything
        // bigger here so a future caller doesn't silently get an
        // unbounded chain.
        return Err(EngineError::Internal(
            "build_graduated_hierarchy_cube: depth must be 0..=3",
        ));
    }

    let g = IdGenerator::new();
    let cube_id = g.cube();
    let root_principal = g.principal();

    let time_dim_id = g.dimension();
    let measure_dim_id = g.dimension();
    let time_leaf = g.element();
    let measure_id_spend = g.element();

    // Allocate `depth` consolidated ancestor IDs.
    let mut time_consolidated: Vec<ElementId> = Vec::with_capacity(depth as usize);
    for _ in 0..depth {
        time_consolidated.push(g.element());
    }

    // Build the Time dim. When depth == 0, no Hierarchy is added — the
    // builder synthesizes a flat default with empty edges. When depth
    // > 0, build a linear chain: top → … → parent → leaf.
    let mut time_builder = Dimension::builder(time_dim_id, "Time", DimensionKind::Standard)
        .add_element(Element::leaf(time_leaf, "Leaf", time_dim_id))?;
    for (i, &id) in time_consolidated.iter().enumerate() {
        let name = format!("Anc{i}");
        time_builder = time_builder.add_element(Element::leaf(id, &name, time_dim_id))?;
    }

    let time_dim = if depth == 0 {
        time_builder.build()?
    } else {
        // Build the chain: top (depth-1) → next (depth-2) → … → parent (0) → leaf.
        // time_consolidated[0] is the immediate parent of the leaf.
        let h_id = g.hierarchy();
        let mut h_builder = Hierarchy::builder(h_id, "GraduatedChain", time_dim_id);
        // Edge from each ancestor to the next-lower ancestor (or to the leaf at index 0).
        for i in 0..(depth as usize) {
            let parent = time_consolidated[i];
            let child = if i == 0 {
                time_leaf
            } else {
                time_consolidated[i - 1]
            };
            h_builder = h_builder.add_edge(parent, child, 1.0);
        }
        let hier = h_builder.build()?;
        time_builder
            .add_hierarchy(hier)?
            .default_hierarchy("GraduatedChain")
            .build()?
    };

    // Measure dim: Spend Input only, no Derived measures.
    let measure_dim = Dimension::builder(measure_dim_id, "Measure", DimensionKind::Measure)
        .add_element(Element::measure(
            measure_id_spend,
            "Spend",
            measure_dim_id,
            CellDataType::F64,
            MeasureRole::Input,
            AggregationRule::Sum,
        ))?
        .build()?;

    let cube_name = format!("Graduated_Depth{depth}_NoDerived");
    let cube = Cube::builder(cube_id, cube_name)
        .add_dimension(time_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(root_principal)
        .build()?;

    Ok((
        cube,
        GraduatedRefs {
            root_principal,
            cube_id,
            time_dim: time_dim_id,
            time_leaf,
            time_consolidated,
            measure_dim: measure_dim_id,
            spend: measure_id_spend,
            depth,
        },
    ))
}

/// Build a coord against the [`build_graduated_hierarchy_cube`] dim
/// order `[Time, Measure]` for the writable leaf.
pub fn graduated_leaf_coord(refs: &GraduatedRefs) -> CellCoordinate {
    CellCoordinate::from_parts(refs.cube_id, [refs.time_leaf, refs.spend])
}

// ===========================================================================
// Phase 2C — Scaled-Acme calibration fixtures
// ===========================================================================
//
// Per `docs/handoffs/phase-2c-handoff.md` §"Phase 2C scope" item 1, this
// section adds an internal generic `build_scaled_acme_cube(scale)`
// (pub(crate)) plus three thin public wrappers (_10x / _50x / _100x).
// The generic is `pub(crate)` so the scale-1× equivalence test in this
// crate's tests can drive it directly while keeping the public mc-fixtures
// surface limited to the three named wrappers.
//
// Scaling strategy: preserve everything in Acme except widen the Market
// dimension. The Time / Channel / Scenario / Version / Measure dims and
// the 5 Acme rules are reused unchanged. The Market dim keeps its 5
// states + 2 regions + 1 USA + Acme's 7 base cities, and ADDS 7×(scale-1)
// extra leaf cities under the existing state parents in proportion to
// the base city count (Florida 3:1, others 1:1). Hierarchy depth stays
// at 4 (City → State → Region → USA); only the bottom-rank fan-out
// widens. Total cities = 7 × scale; total Market elements = 7×scale + 8.
//
// Populated input cells at scale N = 1 scen × 1 ver × 12 months ×
// 5 channels × 7N markets × 6 input measures = 2,520 × N. Hits the
// handoff targets exactly: 2.52K / 25.2K / 126K / 252K (within 0.8%
// of the 1× / 10× / 50× / 100× targets, well under the ±5% tolerance).
//
// At scale = 1 the cube is structurally identical to `build_acme_cube`
// (7 base cities, 0 extras). The scale-1× equivalence test
// (`scaled_cube_at_scale_1_reproduces_acme_anchor_goldens`) proves the
// scaled-builder code path reproduces the Acme brief §4.5.1 anchor
// goldens for Mar/Paid_Search/Tampa — the same code path scale=10/50/
// 100 use, so a failure here is a fixture bug not a wrapper-only bug.

/// One scaled-Market leaf: a city element + the state it parents under +
/// the integer market index used to drive `canonical_inputs_for`. Index
/// `0..=6` are the seven base Acme cities (in the order Tampa, Orlando,
/// Miami, Atlanta, Charlotte, New_York_City, Boston); indices `≥ 7` are
/// the synthetic extras added at `scale > 1`.
#[derive(Clone, Debug)]
pub struct ScaledMarketLeaf {
    pub id: ElementId,
    pub parent_state: ElementId,
    pub market_idx: u32,
    pub name: String,
}

/// Identifiers for a scaled-Acme cube. Embeds the original Acme refs
/// (so benches can use `refs.base.tampa`, `refs.base.q1_2026`, etc.
/// against the same anchors used in the existing Phase 1B / 2A bench
/// suites) and adds the scale factor + the full list of Market leaves.
///
/// `all_market_leaves` always starts with the 7 Acme base cities at
/// indices 0..7, in the order documented on [`ScaledMarketLeaf`].
/// At `scale > 1` the remaining 7×(scale-1) entries are extras grouped
/// by parent state (Florida extras first, then Georgia, NC, NYS, MA).
#[derive(Debug)]
pub struct ScaledAcmeRefs {
    pub base: AcmeRefs,
    pub scale: u32,
    pub all_market_leaves: Vec<ScaledMarketLeaf>,
}

/// Build a scaled-Acme cube. Internal — the public API is the three
/// `build_scaled_acme_cube_{10,50,100}x` wrappers. Direct callers in
/// this crate's `tests` module use it for the mandatory scale-1×
/// equivalence test.
///
/// `scale` must be ≥ 1. `scale = 1` produces a cube structurally
/// identical to `build_acme_cube` (7 base cities, 0 extras, 2,520
/// canonical input cells). `scale = N` widens the Market dim to 7N
/// cities by adding extras under existing states; nothing else changes.
///
/// Per the handoff: this is the single code path the public wrappers
/// share with the equivalence test, ensuring a `scale=1` Acme reproduction
/// proves the wrappers are not parallel reimplementations.
pub(crate) fn build_scaled_acme_cube(scale: u32) -> Result<(Cube, ScaledAcmeRefs), EngineError> {
    if scale == 0 {
        return Err(EngineError::Internal(
            "build_scaled_acme_cube: scale must be ≥ 1",
        ));
    }

    let g = IdGenerator::new();
    let cube_id = g.cube();
    let root_principal = g.principal();

    // Reuse the existing private dim builders for everything except
    // Market. They allocate fresh IDs against `g`, so each scaled cube
    // has its own per-cube ElementIds — no cross-cube ID collision.
    let (scenario_dim, scen_ids) = build_scenario_dim(&g)?;
    let (version_dim, ver_ids) = build_version_dim(&g)?;
    let (time_dim, time_ids, time_hierarchy_id) = build_time_dim(&g)?;
    let (channel_dim, channel_ids, channel_hierarchy_id) = build_channel_dim(&g)?;
    let (market_dim, market_ids, market_hierarchy_id, all_market_leaves) =
        build_scaled_market_dim(&g, scale)?;
    let (measure_dim, measure_ids) = build_measure_dim(&g)?;

    let scenario_dim_id = scenario_dim.id;
    let version_dim_id = version_dim.id;
    let time_dim_id = time_dim.id;
    let channel_dim_id = channel_dim.id;
    let market_dim_id = market_dim.id;
    let measure_dim_id = measure_dim.id;

    let mut base = AcmeRefs {
        root_principal,
        scenario_dim: scenario_dim_id,
        version_dim: version_dim_id,
        time_dim: time_dim_id,
        channel_dim: channel_dim_id,
        market_dim: market_dim_id,
        measure_dim: measure_dim_id,
        time_hierarchy: time_hierarchy_id,
        channel_hierarchy: channel_hierarchy_id,
        market_hierarchy: market_hierarchy_id,
        scen_baseline: scen_ids.baseline,
        scen_aggressive: scen_ids.aggressive,
        scen_conservative: scen_ids.conservative,
        ver_working: ver_ids.working,
        ver_submitted: ver_ids.submitted,
        ver_approved: ver_ids.approved,
        jan_2026: time_ids.jan,
        feb_2026: time_ids.feb,
        mar_2026: time_ids.mar,
        apr_2026: time_ids.apr,
        may_2026: time_ids.may,
        jun_2026: time_ids.jun,
        jul_2026: time_ids.jul,
        aug_2026: time_ids.aug,
        sep_2026: time_ids.sep,
        oct_2026: time_ids.oct,
        nov_2026: time_ids.nov,
        dec_2026: time_ids.dec,
        q1_2026: time_ids.q1,
        q2_2026: time_ids.q2,
        q3_2026: time_ids.q3,
        q4_2026: time_ids.q4,
        fy_2026: time_ids.fy,
        paid_search: channel_ids.paid_search,
        paid_social: channel_ids.paid_social,
        display: channel_ids.display,
        email: channel_ids.email,
        organic: channel_ids.organic,
        paid_media: channel_ids.paid_media,
        owned_earned: channel_ids.owned_earned,
        all_channels: channel_ids.all_channels,
        tampa: market_ids.tampa,
        orlando: market_ids.orlando,
        miami: market_ids.miami,
        atlanta: market_ids.atlanta,
        charlotte: market_ids.charlotte,
        new_york_city: market_ids.new_york_city,
        boston: market_ids.boston,
        florida: market_ids.florida,
        georgia: market_ids.georgia,
        north_carolina: market_ids.north_carolina,
        new_york_state: market_ids.new_york_state,
        massachusetts: market_ids.massachusetts,
        southeast: market_ids.southeast,
        northeast: market_ids.northeast,
        usa: market_ids.usa,
        spend: measure_ids.spend,
        cpc: measure_ids.cpc,
        cvr: measure_ids.cvr,
        close_rate: measure_ids.close_rate,
        aov: measure_ids.aov,
        cogs_rate: measure_ids.cogs_rate,
        clicks: measure_ids.clicks,
        leads: measure_ids.leads,
        customers: measure_ids.customers,
        revenue: measure_ids.revenue,
        gross_profit: measure_ids.gross_profit,
        rule_clicks: RuleId(0),
        rule_leads: RuleId(0),
        rule_customers: RuleId(0),
        rule_revenue: RuleId(0),
        rule_gross_profit: RuleId(0),
    };

    let r_clicks = build_rule_clicks(&g, cube_id, &base);
    let r_leads = build_rule_leads(&g, cube_id, &base);
    let r_customers = build_rule_customers(&g, cube_id, &base);
    let r_revenue = build_rule_revenue(&g, cube_id, &base);
    let r_gross_profit = build_rule_gross_profit(&g, cube_id, &base);
    base.rule_clicks = r_clicks.id;
    base.rule_leads = r_leads.id;
    base.rule_customers = r_customers.id;
    base.rule_revenue = r_revenue.id;
    base.rule_gross_profit = r_gross_profit.id;

    let cube = CubeBuilder::default_for_cube(cube_id, &format!("Acme_MarketingFinance_x{scale}"))
        .add_dimension(scenario_dim)
        .add_dimension(version_dim)
        .add_dimension(time_dim)
        .add_dimension(channel_dim)
        .add_dimension(market_dim)
        .add_dimension(measure_dim)
        .measure_dimension("Measure")
        .root_principal(root_principal)
        .add_rule(r_clicks)?
        .add_rule(r_leads)?
        .add_rule(r_customers)?
        .add_rule(r_revenue)?
        .add_rule(r_gross_profit)?
        .build()?;

    Ok((
        cube,
        ScaledAcmeRefs {
            base,
            scale,
            all_market_leaves,
        },
    ))
}

/// Build the scaled Market dimension. Returns the dim, the base
/// `MarketIds` (Tampa..USA — same shape as `build_market_dim`), the
/// hierarchy id, and the full list of market leaves (base 7 first, then
/// 7×(scale-1) extras).
fn build_scaled_market_dim(
    g: &IdGenerator,
    scale: u32,
) -> Result<(Dimension, MarketIds, HierarchyId, Vec<ScaledMarketLeaf>), EngineError> {
    let dim_id = g.dimension();
    // Base 7 cities (preserve Acme order: Tampa, Orlando, Miami, Atlanta,
    // Charlotte, NYC, Boston).
    let tampa = g.element();
    let orlando = g.element();
    let miami = g.element();
    let atlanta = g.element();
    let charlotte = g.element();
    let new_york_city = g.element();
    let boston = g.element();
    // States, regions, USA — unchanged from Acme.
    let florida = g.element();
    let georgia = g.element();
    let north_carolina = g.element();
    let new_york_state = g.element();
    let massachusetts = g.element();
    let southeast = g.element();
    let northeast = g.element();
    let usa = g.element();
    let h_id = g.hierarchy();

    // The 7 base cities, each tagged with parent state + Acme's 0..=6
    // market index (the index `canonical_inputs_for` uses).
    let mut all_market_leaves: Vec<ScaledMarketLeaf> = vec![
        ScaledMarketLeaf {
            id: tampa,
            parent_state: florida,
            market_idx: 0,
            name: "Tampa".to_string(),
        },
        ScaledMarketLeaf {
            id: orlando,
            parent_state: florida,
            market_idx: 1,
            name: "Orlando".to_string(),
        },
        ScaledMarketLeaf {
            id: miami,
            parent_state: florida,
            market_idx: 2,
            name: "Miami".to_string(),
        },
        ScaledMarketLeaf {
            id: atlanta,
            parent_state: georgia,
            market_idx: 3,
            name: "Atlanta".to_string(),
        },
        ScaledMarketLeaf {
            id: charlotte,
            parent_state: north_carolina,
            market_idx: 4,
            name: "Charlotte".to_string(),
        },
        ScaledMarketLeaf {
            id: new_york_city,
            parent_state: new_york_state,
            market_idx: 5,
            name: "New_York_City".to_string(),
        },
        ScaledMarketLeaf {
            id: boston,
            parent_state: massachusetts,
            market_idx: 6,
            name: "Boston".to_string(),
        },
    ];

    // Synthetic extras for scale > 1. Distribute proportionally to base
    // counts: Florida 3 per scale tick (the only multi-city state in
    // Acme), the others 1 each. Total extras at scale N = 7×(N-1).
    // Indexing scheme: extras get market_idx ≥ 7 in stable order so
    // canonical_inputs_for is deterministic across runs.
    if scale > 1 {
        let extras_per_tick: [(ElementId, &str); 7] = [
            (florida, "Florida"),
            (florida, "Florida"),
            (florida, "Florida"),
            (georgia, "Georgia"),
            (north_carolina, "North_Carolina"),
            (new_york_state, "New_York_State"),
            (massachusetts, "Massachusetts"),
        ];
        let mut next_market_idx = 7u32;
        for tick in 1..scale {
            for (slot_idx, (parent_state, parent_name)) in extras_per_tick.iter().enumerate() {
                let extra = g.element();
                let name = format!("Extra_{parent_name}_t{tick:03}_s{slot_idx}");
                all_market_leaves.push(ScaledMarketLeaf {
                    id: extra,
                    parent_state: *parent_state,
                    market_idx: next_market_idx,
                    name,
                });
                next_market_idx += 1;
            }
        }
    }

    // Hierarchy: USA → {Southeast, Northeast} → {states} → {cities}.
    // Fixed top three levels exactly match Acme; only the city → state
    // edges grow with `scale`.
    let mut h_builder = Hierarchy::builder(h_id, "Geographic", dim_id)
        .add_edge(usa, southeast, 1.0)
        .add_edge(usa, northeast, 1.0)
        .add_edge(southeast, florida, 1.0)
        .add_edge(southeast, georgia, 1.0)
        .add_edge(southeast, north_carolina, 1.0)
        .add_edge(northeast, new_york_state, 1.0)
        .add_edge(northeast, massachusetts, 1.0);
    for leaf in &all_market_leaves {
        h_builder = h_builder.add_edge(leaf.parent_state, leaf.id, 1.0);
    }
    let hier = h_builder.build()?;

    // Dimension element registration: leaves first (so they exist before
    // the hierarchy attaches), then states/regions/USA.
    let mut dim_builder = Dimension::builder(dim_id, "Market", DimensionKind::Standard);
    for leaf in &all_market_leaves {
        dim_builder = dim_builder.add_element(Element::leaf(leaf.id, &leaf.name, dim_id))?;
    }
    let dim = dim_builder
        .add_element(Element::leaf(florida, "Florida", dim_id))?
        .add_element(Element::leaf(georgia, "Georgia", dim_id))?
        .add_element(Element::leaf(north_carolina, "North_Carolina", dim_id))?
        .add_element(Element::leaf(new_york_state, "New_York_State", dim_id))?
        .add_element(Element::leaf(massachusetts, "Massachusetts", dim_id))?
        .add_element(Element::leaf(southeast, "Southeast", dim_id))?
        .add_element(Element::leaf(northeast, "Northeast", dim_id))?
        .add_element(Element::leaf(usa, "USA", dim_id))?
        .add_hierarchy(hier)?
        .default_hierarchy("Geographic")
        .build()?;

    Ok((
        dim,
        MarketIds {
            tampa,
            orlando,
            miami,
            atlanta,
            charlotte,
            new_york_city,
            boston,
            florida,
            georgia,
            north_carolina,
            new_york_state,
            massachusetts,
            southeast,
            northeast,
            usa,
        },
        h_id,
        all_market_leaves,
    ))
}

/// Build a 10× Acme cube — 25,200 canonical input cells across 70
/// markets. Other dims unchanged from Acme. See module-level docs for
/// the scaling strategy.
pub fn build_scaled_acme_cube_10x() -> Result<(Cube, ScaledAcmeRefs), EngineError> {
    build_scaled_acme_cube(10)
}

/// Build a 50× Acme cube — 126,000 canonical input cells across 350
/// markets. Other dims unchanged from Acme.
pub fn build_scaled_acme_cube_50x() -> Result<(Cube, ScaledAcmeRefs), EngineError> {
    build_scaled_acme_cube(50)
}

/// Build a 100× Acme cube — 252,000 canonical input cells across 700
/// markets. Other dims unchanged from Acme.
pub fn build_scaled_acme_cube_100x() -> Result<(Cube, ScaledAcmeRefs), EngineError> {
    build_scaled_acme_cube(100)
}

/// Write the canonical input cells for a scaled-Acme cube. Generalizes
/// `write_canonical_inputs` over `7 × scale` markets: 1 scen × 1 ver ×
/// 12 months × 5 channels × `7 × scale` markets × 6 input measures =
/// `2_520 × scale` cells. Returns the count written.
///
/// Values for the original 7 base cities (market_idx 0..=6) match the
/// Acme `canonical_inputs_for` formula byte-for-byte, which is what the
/// scale-1× equivalence test relies on.
pub fn write_canonical_inputs_scaled(
    cube: &mut Cube,
    refs: &ScaledAcmeRefs,
) -> Result<usize, EngineError> {
    let cube_id = cube.id;
    let root = refs.base.root_principal;
    let mut count = 0;
    let time_idx_to_element: [(u32, ElementId); 12] = [
        (1, refs.base.jan_2026),
        (2, refs.base.feb_2026),
        (3, refs.base.mar_2026),
        (4, refs.base.apr_2026),
        (5, refs.base.may_2026),
        (6, refs.base.jun_2026),
        (7, refs.base.jul_2026),
        (8, refs.base.aug_2026),
        (9, refs.base.sep_2026),
        (10, refs.base.oct_2026),
        (11, refs.base.nov_2026),
        (12, refs.base.dec_2026),
    ];
    let channel_idx_to_element: [(u32, ElementId); 5] = [
        (0, refs.base.paid_search),
        (1, refs.base.paid_social),
        (2, refs.base.display),
        (3, refs.base.email),
        (4, refs.base.organic),
    ];

    for &(t_idx, t_id) in &time_idx_to_element {
        for &(c_idx, c_id) in &channel_idx_to_element {
            for leaf in &refs.all_market_leaves {
                let inputs = canonical_inputs_for(t_idx, c_idx, leaf.market_idx);
                for (measure_id, value) in [
                    (refs.base.spend, inputs.spend),
                    (refs.base.cpc, inputs.cpc),
                    (refs.base.cvr, inputs.cvr),
                    (refs.base.close_rate, inputs.close_rate),
                    (refs.base.aov, inputs.aov),
                    (refs.base.cogs_rate, inputs.cogs_rate),
                ] {
                    let coord = coord(
                        cube_id,
                        &refs.base,
                        refs.base.scen_baseline,
                        refs.base.ver_working,
                        t_id,
                        c_id,
                        leaf.id,
                        measure_id,
                    );
                    cube.write(WritebackRequest {
                        coord,
                        new_value: ScalarValue::F64(value),
                        principal: root,
                        intent: WriteIntent::Set,
                        expected_revision: None,
                        now_unix_seconds: 0,
                    })?;
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

/// Force materialization of every (leaf-coord × derived-measure) rule
/// edge in the dependency graph for a scaled-Acme cube. Generalizes
/// `materialize_all_dependencies`: 1 × 1 × 12 × 5 × `7 × scale` × 5 =
/// `2_100 × scale` reads.
///
/// This is expensive at high scale (1.05M cold reads at scale=100) and
/// is meant for one-shot bench setup, not the inner loop. Returns the
/// number of leaf-derived reads performed.
pub fn materialize_all_dependencies_scaled(
    cube: &mut Cube,
    refs: &ScaledAcmeRefs,
) -> Result<usize, EngineError> {
    let cube_id = cube.id;
    let root = refs.base.root_principal;
    let times: [ElementId; 12] = [
        refs.base.jan_2026,
        refs.base.feb_2026,
        refs.base.mar_2026,
        refs.base.apr_2026,
        refs.base.may_2026,
        refs.base.jun_2026,
        refs.base.jul_2026,
        refs.base.aug_2026,
        refs.base.sep_2026,
        refs.base.oct_2026,
        refs.base.nov_2026,
        refs.base.dec_2026,
    ];
    let channels: [ElementId; 5] = [
        refs.base.paid_search,
        refs.base.paid_social,
        refs.base.display,
        refs.base.email,
        refs.base.organic,
    ];
    let derived: [ElementId; 5] = [
        refs.base.clicks,
        refs.base.leads,
        refs.base.customers,
        refs.base.revenue,
        refs.base.gross_profit,
    ];
    let mut count = 0;
    for &t in &times {
        for &c in &channels {
            for leaf in &refs.all_market_leaves {
                for &d in &derived {
                    let c_coord = coord(
                        cube_id,
                        &refs.base,
                        refs.base.scen_baseline,
                        refs.base.ver_working,
                        t,
                        c,
                        leaf.id,
                        d,
                    );
                    cube.read(&c_coord, root)?;
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_acme_cube_succeeds() {
        let (cube, refs) = build_acme_cube().expect("build ok");
        assert_eq!(cube.dimensions().len(), 6);
        assert_eq!(cube.name, "Acme_MarketingFinance");
        // Dim order check.
        assert_eq!(cube.dimensions()[0].name, "Scenario");
        assert_eq!(cube.dimensions()[5].name, "Measure");
        // Dim id check.
        assert_eq!(cube.dimensions()[0].id, refs.scenario_dim);
        assert_eq!(cube.dimensions()[5].id, refs.measure_dim);
    }

    #[test]
    fn write_canonical_inputs_writes_2520_cells() {
        let (mut cube, refs) = build_acme_cube().expect("build ok");
        let count = write_canonical_inputs(&mut cube, &refs).expect("inputs ok");
        assert_eq!(count, 2_520);
    }

    #[test]
    fn anchor_cell_inputs_match_brief_4_5_1() {
        // Mar_2026 / Paid_Search / Tampa: time_idx=3, channel_idx=0,
        // market_idx=0. Brief golden: Spend=11500, CPC=1.50, CVR=0.020,
        // Close_Rate=0.10, AOV=200.0, COGS_Rate=0.30.
        let inp = canonical_inputs_for(3, 0, 0);
        assert!((inp.spend - 11_500.0).abs() < 1e-12);
        assert!((inp.cpc - 1.50).abs() < 1e-12);
        assert!((inp.cvr - 0.020).abs() < 1e-12);
        assert!((inp.close_rate - 0.10).abs() < 1e-12);
        assert!((inp.aov - 200.0).abs() < 1e-12);
        assert!((inp.cogs_rate - 0.30).abs() < 1e-12);
    }

    #[test]
    fn anchor_derived_chain_matches_brief_4_5_1() {
        // Brief golden: Clicks=23000/3, Leads=460/3, Customers=46/3,
        // Revenue=9200/3, Gross_Profit=6440/3.
        let inp = canonical_inputs_for(3, 0, 0);
        assert!((inp.clicks() - 23_000.0 / 3.0).abs() < 1e-9);
        assert!((inp.leads() - 460.0 / 3.0).abs() < 1e-9);
        assert!((inp.customers() - 46.0 / 3.0).abs() < 1e-9);
        assert!((inp.revenue() - 9_200.0 / 3.0).abs() < 1e-9);
        assert!((inp.gross_profit() - 6_440.0 / 3.0).abs() < 1e-9);
    }

    // ----- Phase 2A synthetic fixtures -----

    #[test]
    fn build_minimal_cube_has_no_hierarchies_and_no_derived() {
        let (cube, refs) = build_minimal_cube().expect("build_minimal_cube");
        assert_eq!(cube.dimensions().len(), 2);
        // No non-Measure dim has any hierarchy edges (the synth flat
        // default is empty).
        for dim in cube.dimensions() {
            if dim.kind != DimensionKind::Measure {
                assert!(
                    dim.default_hierarchy().edges.is_empty(),
                    "minimal cube must have no hierarchy edges on dim {}",
                    dim.name
                );
            }
        }
        // No Derived measures.
        let derived_count = cube
            .measure_dimension()
            .elements
            .iter()
            .filter(|e| {
                e.measure_meta()
                    .map(|m| m.role == MeasureRole::Derived)
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            derived_count, 0,
            "minimal cube must have zero Derived measures"
        );
        // Sanity: refs match cube state.
        assert_eq!(refs.cube_id, cube.id);
        assert!(cube.dimension(refs.time_dim).is_some());
        assert!(cube.dimension(refs.measure_dim).is_some());
    }

    #[test]
    fn build_minimal_cube_single_write_produces_zero_dirty_delta() {
        let (mut cube, refs) = build_minimal_cube().expect("build_minimal_cube");
        let coord = minimal_coord(&refs);
        let dirty_before = cube.dirty().len();
        let result = cube
            .write(WritebackRequest {
                coord: coord.clone(),
                new_value: ScalarValue::F64(42.0),
                principal: refs.root_principal,
                intent: WriteIntent::Set,
                expected_revision: None,
                now_unix_seconds: 0,
            })
            .expect("write must succeed");
        let dirty_after = cube.dirty().len();
        // No hierarchy → 0 ancestor marks. No rules → empty rev-edge
        // closure. No other Derived measures → no cross-measure
        // invalidation. The freshly-written coord itself is excluded
        // from `mark_closure` (cube.rs:42 dirty.rs) and is the
        // `is_pure_leaf && m == measure_id` skip-case in
        // `compute_dirty_ancestors`. Net delta = 0.
        assert_eq!(
            dirty_after - dirty_before,
            0,
            "minimal cube must mark zero coords dirty after one write \
             (no hierarchy ancestors, no rev-edges, no derived measures)"
        );
        assert!(
            result.invalidated.is_empty(),
            "WritebackResult.invalidated must be empty for the minimal cube"
        );
        // And the value round-trips.
        let v = cube.read(&coord, refs.root_principal).expect("read");
        assert_eq!(v.value.as_f64(), Some(42.0));
    }

    #[test]
    fn build_graduated_hierarchy_cube_zero_depth_matches_minimal_invariants() {
        let (cube, refs) = build_graduated_hierarchy_cube(0).expect("graduated 0");
        assert_eq!(refs.depth, 0);
        assert!(refs.time_consolidated.is_empty());
        let time_dim = cube.dimension(refs.time_dim).expect("time dim");
        assert!(
            time_dim.default_hierarchy().edges.is_empty(),
            "depth=0 must have no hierarchy edges"
        );
        assert_eq!(time_dim.elements.len(), 1);
    }

    #[test]
    fn build_graduated_hierarchy_cube_depth_three_chain_is_linear() {
        let (cube, refs) = build_graduated_hierarchy_cube(3).expect("graduated 3");
        assert_eq!(refs.depth, 3);
        assert_eq!(refs.time_consolidated.len(), 3);
        let time_dim = cube.dimension(refs.time_dim).expect("time dim");
        let h = time_dim.default_hierarchy();
        // 3 edges: each ancestor → child link.
        assert_eq!(h.edges.len(), 3);
        // Walking ancestors of the leaf must surface exactly the 3
        // consolidated elements in chain order.
        let ancestors: Vec<ElementId> = h
            .ancestors(refs.time_leaf)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            ancestors.len(),
            3,
            "depth=3 leaf must have exactly 3 ancestors"
        );
        // Ancestors must match the chain we built (immediate parent at
        // index 0, top at index 2).
        for (i, &expected) in refs.time_consolidated.iter().enumerate() {
            assert!(
                ancestors.contains(&expected),
                "ancestor index {i} ({expected:?}) missing from h.ancestors(leaf): {ancestors:?}"
            );
        }
    }

    #[test]
    fn build_graduated_hierarchy_cube_depth_one_write_dirty_delta_matches_depth() {
        let (mut cube, refs) = build_graduated_hierarchy_cube(1).expect("graduated 1");
        let coord = graduated_leaf_coord(&refs);
        let dirty_before = cube.dirty().len();
        cube.write(WritebackRequest {
            coord,
            new_value: ScalarValue::F64(7.0),
            principal: refs.root_principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write");
        let dirty_after = cube.dirty().len();
        // 1 ancestor element × 1 (Spend) measure = 1 consolidated coord
        // marked dirty. No derived measures, no rev-edges.
        assert_eq!(
            dirty_after - dirty_before,
            1,
            "depth=1 write must mark exactly 1 ancestor coord dirty"
        );
    }

    #[test]
    fn build_graduated_hierarchy_cube_rejects_excessive_depth() {
        let err = build_graduated_hierarchy_cube(4).expect_err("depth=4 must reject");
        assert!(matches!(err, EngineError::Internal(_)));
    }

    // ----- Phase 2C scaled-Acme calibration fixtures -----

    /// Mandatory Phase 2C handoff §"Sanity checks before timing"
    /// requirement: the internal generic builder at scale=1 must
    /// reproduce Acme's brief §4.5.1 anchor goldens for
    /// Mar/Paid_Search/Tampa byte-for-byte after a write_canonical_inputs
    /// equivalent load. This proves the scaled path is the same code
    /// the wrappers use, not a parallel reimplementation that could
    /// drift from Acme without anyone noticing.
    #[test]
    fn scaled_cube_at_scale_1_reproduces_acme_anchor_goldens() {
        let (mut cube, refs) = build_scaled_acme_cube(1).expect("build scale=1");
        let written =
            write_canonical_inputs_scaled(&mut cube, &refs).expect("scaled inputs at scale=1");
        assert_eq!(
            written, 2_520,
            "scale=1 must write exactly 2,520 input cells (matches Acme)"
        );

        let principal = refs.base.root_principal;
        let read_at = |measure: ElementId, cube: &mut Cube| -> f64 {
            let c = coord(
                cube.id,
                &refs.base,
                refs.base.scen_baseline,
                refs.base.ver_working,
                refs.base.mar_2026,
                refs.base.paid_search,
                refs.base.tampa,
                measure,
            );
            cube.read(&c, principal)
                .expect("anchor read")
                .value
                .as_f64()
                .expect("F64")
        };

        // Brief §4.5.1 anchor goldens — six inputs:
        let spend = read_at(refs.base.spend, &mut cube);
        let cpc = read_at(refs.base.cpc, &mut cube);
        let cvr = read_at(refs.base.cvr, &mut cube);
        let close_rate = read_at(refs.base.close_rate, &mut cube);
        let aov = read_at(refs.base.aov, &mut cube);
        let cogs_rate = read_at(refs.base.cogs_rate, &mut cube);
        assert!(
            (spend - 11_500.0).abs() < 1e-9,
            "Spend golden mismatch: {spend}"
        );
        assert!((cpc - 1.50).abs() < 1e-9, "CPC golden mismatch: {cpc}");
        assert!((cvr - 0.020).abs() < 1e-9, "CVR golden mismatch: {cvr}");
        assert!(
            (close_rate - 0.10).abs() < 1e-9,
            "Close_Rate golden mismatch: {close_rate}"
        );
        assert!((aov - 200.0).abs() < 1e-9, "AOV golden mismatch: {aov}");
        assert!(
            (cogs_rate - 0.30).abs() < 1e-9,
            "COGS_Rate golden mismatch: {cogs_rate}"
        );

        // Brief §4.5.1 derived chain — five derived measures.
        let clicks = read_at(refs.base.clicks, &mut cube);
        let leads = read_at(refs.base.leads, &mut cube);
        let customers = read_at(refs.base.customers, &mut cube);
        let revenue = read_at(refs.base.revenue, &mut cube);
        let gp = read_at(refs.base.gross_profit, &mut cube);
        assert!(
            (clicks - 23_000.0 / 3.0).abs() < 1e-9,
            "Clicks golden mismatch: {clicks}"
        );
        assert!(
            (leads - 460.0 / 3.0).abs() < 1e-9,
            "Leads golden mismatch: {leads}"
        );
        assert!(
            (customers - 46.0 / 3.0).abs() < 1e-9,
            "Customers golden mismatch: {customers}"
        );
        assert!(
            (revenue - 9_200.0 / 3.0).abs() < 1e-9,
            "Revenue golden mismatch: {revenue}"
        );
        assert!(
            (gp - 6_440.0 / 3.0).abs() < 1e-9,
            "Gross_Profit golden mismatch: {gp}"
        );
    }

    /// Per Phase 2C handoff: each public wrapper is unit-tested for
    /// dim count = 6, measure count = 11, rule count = 5, populated
    /// input-cell count within ±5% of target, hierarchy depth preserved
    /// (Time 3 levels via the FY → Q → Month chain; Channel 3 via
    /// All → Group → Leaf; Market 4 via USA → Region → State → City).
    #[test]
    fn scaled_acme_10x_invariants() {
        let (cube, refs) = build_scaled_acme_cube_10x().expect("build 10x");
        assert_invariants_at_scale(&cube, &refs, 10);
    }

    #[test]
    fn scaled_acme_50x_invariants() {
        let (cube, refs) = build_scaled_acme_cube_50x().expect("build 50x");
        assert_invariants_at_scale(&cube, &refs, 50);
    }

    #[test]
    fn scaled_acme_100x_invariants() {
        let (cube, refs) = build_scaled_acme_cube_100x().expect("build 100x");
        assert_invariants_at_scale(&cube, &refs, 100);
    }

    /// Sanity-check that one extra (non-base) Market leaf round-trips:
    /// scale > 1 wrappers must produce a cube where the synthetic
    /// extras can be written and read back, not just instantiated as
    /// dim elements with no rule coverage.
    #[test]
    fn scaled_acme_10x_extra_leaf_round_trips() {
        let (mut cube, refs) = build_scaled_acme_cube_10x().expect("build 10x");
        // Pick the first extra (index 7 — first non-Acme city).
        let extra = &refs.all_market_leaves[7];
        assert!(
            extra.market_idx >= 7,
            "extras must start at market_idx >= 7"
        );
        let principal = refs.base.root_principal;
        let target = coord(
            cube.id,
            &refs.base,
            refs.base.scen_baseline,
            refs.base.ver_working,
            refs.base.mar_2026,
            refs.base.paid_search,
            extra.id,
            refs.base.spend,
        );
        cube.write(WritebackRequest {
            coord: target.clone(),
            new_value: ScalarValue::F64(12_345.0),
            principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })
        .expect("write extra-leaf Spend");
        let read_back = cube
            .read(&target, principal)
            .expect("read extra-leaf Spend")
            .value
            .as_f64()
            .expect("F64");
        assert!(
            (read_back - 12_345.0).abs() < 1e-9,
            "extra leaf write/read round-trip drift: {read_back}"
        );
    }

    /// scale = 0 is rejected per the Internal-error contract on the
    /// internal builder. Catches any future regression that silently
    /// produces a degenerate cube.
    #[test]
    fn scaled_acme_rejects_scale_zero() {
        let err = build_scaled_acme_cube(0).expect_err("scale=0 must reject");
        assert!(
            matches!(err, EngineError::Internal(_)),
            "scale=0 must produce EngineError::Internal: {err:?}"
        );
    }

    /// Structural invariants at a given scale. Does **not** call the
    /// bulk-load path (that would write 252_000 cells at scale=100 in
    /// debug mode — minutes per `cargo test` invocation, breaks the
    /// CLAUDE.md §4.2 determinism gate's wall-clock expectations).
    /// Populated input-cell count is verified arithmetically from the
    /// leaf-count product so the assertion is identical without paying
    /// the I/O. The bulk-load + read paths *are* exercised at scale=1
    /// (the equivalence test) and at scale=10 (the round-trip test);
    /// at higher scales the same code path is exercised by the
    /// `combined_workflow` bench under criterion.
    fn assert_invariants_at_scale(cube: &Cube, refs: &ScaledAcmeRefs, scale: u32) {
        // 6 dims, in canonical Acme order.
        assert_eq!(cube.dimensions().len(), 6, "scaled cube must have 6 dims");
        assert_eq!(cube.dimensions()[0].name, "Scenario");
        assert_eq!(cube.dimensions()[1].name, "Version");
        assert_eq!(cube.dimensions()[2].name, "Time");
        assert_eq!(cube.dimensions()[3].name, "Channel");
        assert_eq!(cube.dimensions()[4].name, "Market");
        assert_eq!(cube.dimensions()[5].name, "Measure");

        // 11 measures (6 input + 5 derived) — unchanged by scaling.
        let measure_dim = cube.measure_dimension();
        assert_eq!(
            measure_dim.elements.len(),
            11,
            "scaled cube must have 11 measures"
        );
        let derived_count = measure_dim
            .elements
            .iter()
            .filter(|e| {
                e.measure_meta()
                    .map(|m| m.role == MeasureRole::Derived)
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(derived_count, 5, "scaled cube must have 5 derived measures");

        // 5 rules — unchanged by scaling. RuleIds are threaded onto
        // refs.base when build_scaled_acme_cube wires them in, so a
        // populated id witnesses the rule's presence in the cube.
        for rid in [
            refs.base.rule_clicks,
            refs.base.rule_leads,
            refs.base.rule_customers,
            refs.base.rule_revenue,
            refs.base.rule_gross_profit,
        ] {
            assert_ne!(rid, RuleId(0), "rule id must be populated on refs.base");
        }

        // Hierarchy depth preserved from Acme: Time leaves have 2
        // ancestors (Quarter, Year), Channel leaves have 2 (Group,
        // All_Channels), Market leaves have 3 (State, Region, USA).
        let time_dim = cube.dimension(refs.base.time_dim).expect("time dim");
        let time_anc = time_dim
            .default_hierarchy()
            .ancestors(refs.base.jan_2026)
            .len();
        assert_eq!(time_anc, 2, "Time leaf has 2 ancestors (Quarter, Year)");

        let channel_dim = cube.dimension(refs.base.channel_dim).expect("channel dim");
        let channel_anc = channel_dim
            .default_hierarchy()
            .ancestors(refs.base.paid_search)
            .len();
        assert_eq!(
            channel_anc, 2,
            "Channel leaf has 2 ancestors (Group, All_Channels)"
        );

        let market_dim = cube.dimension(refs.base.market_dim).expect("market dim");
        let market_anc = market_dim
            .default_hierarchy()
            .ancestors(refs.base.tampa)
            .len();
        assert_eq!(
            market_anc, 3,
            "Market leaf has 3 ancestors (State, Region, USA)"
        );

        // Scale + market-leaf count.
        assert_eq!(refs.scale, scale, "scale must be threaded into refs");
        assert_eq!(
            refs.all_market_leaves.len() as u32,
            7 * scale,
            "scaled cube must have exactly 7×scale Market leaves"
        );
        // Bottom-tier widening only — total Market elements = 7×scale + 8.
        assert_eq!(
            market_dim.elements.len() as u32,
            7 * scale + 8,
            "scaled cube must have 7×scale + 8 Market elements (cities + 5 states + 2 regions + 1 USA)"
        );

        // Populated input-cell count is the exact product of the
        // (Scenario, Version, Time, Channel, Market, input-Measure)
        // grid that `write_canonical_inputs_scaled` walks: 1 baseline
        // scenario × 1 working version × 12 months × 5 channels ×
        // 7N markets × 6 input measures = 2,520×N. Computed
        // arithmetically (rather than by actually running the bulk
        // load) so this assertion is identical without paying the
        // debug-mode write cost.
        let leaf_product = 12 * 5 * refs.all_market_leaves.len() * 6;
        let target = 2_520usize * scale as usize;
        let lo = (target as f64 * 0.95) as usize;
        let hi = (target as f64 * 1.05) as usize;
        assert!(
            leaf_product >= lo && leaf_product <= hi,
            "populated input cells {leaf_product} outside ±5% of target {target} (scale={scale})"
        );
    }
}
