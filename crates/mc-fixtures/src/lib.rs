//! `mc-fixtures` — the Acme demo cube fixture.
//!
//! Per phase-1-rust-kernel-build-brief.md §4. Builds:
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
}
