# ADR-0026: Organization, Workspace, Resource Scope, and Capability Grants

**Status:** Accepted
**Date:** 2026-05-09
**Accepted:** 2026-05-09
**Deciders:** project owner, with input from Claude Desktop and GPT
**Phase:** Cross-cutting (foundational for Phase 4C, Phase 8 daemon, Phase 9 cloud)
**Prerequisite ADR:** ADR-0025 (kernel discipline and deployment architecture) — read that first

> This ADR defines Mosaic's organizational container model and capability-grant system. It enables Mosaic to scale from a solo founder's single-user deployment all the way to enterprise agencies, partner networks, franchises, holding companies, and multi-tenant cloud deployments — without rebuilding the architecture. The kernel remains unaware of these concepts; they live in workspace manifests, the Phase 4C primitive layer, and future shell crates.

---

## Context

Mosaic's current architecture is single-user and single-workspace. As it grows toward agency deployments, enterprise clients, and the cartridge marketplace, it needs a scalable structural model for:

- **Agencies** managing multiple clients (each client's data must be isolated)
- **Enterprise clients** with multiple departments and their own users/audit trails
- **Partner networks** where vendors distribute cartridges/benchmarks without handing over IP ownership
- **Holding companies** with acquired brands that need separate data boundaries but shared executive reporting
- **Personal users** with multiple projects across different domains (sports betting, finance, marketing)

Without an explicit organizational model, every deployment becomes a bespoke integration. With one, the same kernel serves all of these shapes.

The structural pattern is not novel. Slack Enterprise Grid (org of workspaces), GitHub (org/team/repo), Hex (workspace/groups/projects), and Cloudflare Workers (account/resource scoping) have all converged on similar models because those shapes fit real enterprise software. This ADR borrows their convergent wisdom and adapts it to Mosaic's specific architecture.

**What phase implements this:** Phase 4C (currently "multi-domain workspace primitive") should be rescoped to implement this ADR's decisions. Phase 8 (daemon) and Phase 9 (cloud) both require this foundation to be in place.

**What does NOT get built yet:** RBAC systems, billing infrastructure, SCIM/SAML admin surfaces, partner marketplace, cross-org analytics engines. Those are Phase 9+ implementation work. This ADR defines the architecture; the shell infrastructure builds on it.

---

## Decisions

### Decision 1: The four-entity model

Mosaic's organizational model uses four entities:

```
Organization
└── Workspace
    └── Cube (model)
        └── Cell (coordinate × revision)

Plus: Managed Org Relationship (inter-org connection without ownership transfer)
```

**Organization:** The top-level ownership, trust, billing, and security boundary. An org owns its workspaces, templates, benchmarks, and installed cartridges. Org identity is the root of all permission decisions.

**Workspace:** An operating domain inside an org. Maps to a client (for agencies), a department (for enterprises), a project (for personal use), or an environment (dev/staging/prod). Each workspace has its own cubes, audit logs, write logs, snapshots, and narrative ledger. Workspaces don't cross org boundaries without explicit grants.

**Cube:** The actual Mosaic model inside a workspace. Multi-dimensional, with rules, scenarios, versions, and narratives. Cubes don't span workspaces; cross-workspace analysis requires an explicit rollup model or org-level aggregation.

**Managed Org Relationship:** When one org has a defined relationship with another org — without the first org owning the second. This handles agencies managing enterprise clients, parent companies overseeing acquired brands, vendors granting cartridge access to partners, and franchisors managing franchisees.

**Why not "Account/Client" as a fourth entity type?** Claude Desktop review flagged this risk: "Account" as a separate entity type creates vocabulary ambiguity — when someone says "client," do they mean account, workspace, or managed org? Collapsed to three entity types (org/workspace/cube) plus the managed-org relationship. A client IS a workspace or IS a separate org per the decision rule in Decision 2. No third vocabulary concept needed.

### Decision 2: The workspace-vs-org decision rule

This is the most important decision in this ADR. It resolves the most common structural ambiguity.

> **Use a workspace** when the parent org owns the operating environment.
>
> **Use a separate org** when the entity needs its own identity, users, billing, audit trail, data ownership, or downstream workspaces.

Concrete application:

| Business shape | Structure | Rationale |
|---|---|---|
| Solo founder with multiple projects | One org, many workspaces | Same user, same billing, different projects |
| Agency with small clients | Agency org, client as workspace | Agency owns the workflow; client only sees reports |
| Agency with enterprise client | Agency org manages Client org; client has own workspaces | Client owns data, has own users, may leave agency |
| Holding company + acquired brands | Parent org with child orgs | Each brand has own data/users; parent has visibility via grants |
| Partner/reseller | Partner org; grants to vendor's cartridges | Partner uses vendor IP without owning it |
| DBA brands (same legal entity) | One org, one workspace per brand | Same security/billing boundary |
| DBA brands (separate teams/contracts) | Parent org with child orgs per brand | Different users, different access, different audit needs |
| Multi-department enterprise | One org, one workspace per dept | Org owns all departments; IT manages centrally |

### Decision 3: Capability-based grants (not enumerated relationship types)

Inter-org relationships use a **generic capability-grant model**, NOT enumerated relationship types. Specific relationship shapes (parent/subsidiary, agency-managed, partner, reseller) are conventions for common capability combinations — they are NOT first-class architectural concepts.

**The five capability levels:**

| Capability | What it allows |
|---|---|
| `use` | Run reports, evaluate templates, use cubes |
| `view` | See a resource and its outputs |
| `fork` | Copy a resource into the grantee's own org |
| `contribute` | Submit changes back to the source org |
| `admin` | Modify the resource directly (rare; usually only owning org) |

Grants combine arbitrarily. Common combinations:
- Agency-managed client: `use + view` on client's workspace; client has `admin` on their own workspace
- Partner using vendor cartridge: `use + view` on cartridge; no `fork` (prevents IP leakage)
- Holding company + subsidiary: parent has `view + use` on child templates; child has `admin` on own workspace
- Marketplace cartridge buyer: `use` only; `fork` requires separate license

**Why capability-based instead of relationship types:**

Relationship type enums are notoriously hard to get right. A franchise might be `child_of` AND `reseller_of` simultaneously. Post-acquisition companies may operate as `partner_of` for years before full integration. Once types are in the ADR and code starts depending on them (permission checks: `if relationship_type == 'manages'`), adding a new type requires migrating all dependent code.

Capability grants are an attribute model, not a type model. New business shapes don't require schema changes — they're new combinations of existing capabilities. This is how AWS IAM (attribute-based, not role-typed), Cloudflare's account model, and GitHub's team-permission model work. These systems have scaled to millions of orgs; enumerated relationship types have not.

**Note on future Grout integration:** When Grout ships (Phase 8.5+), each capability grant will be signed by the grantor's key and recorded in an append-only hash-chained grant log. Tampering with a grant record produces a detectable hash break. Capability grants being attribute-based also makes this signing straightforward — there's no type-specific signing logic, just sign the capability payload.

### Decision 4: Five-level template inheritance chain

Templates, benchmarks, and cartridge components resolve through a five-level chain. **Most-specific wins.**

```
Level 1: Marketplace cartridge        (public / community baseline)
Level 2: Provider org cartridge        (vendor-specific standards)
Level 3: Customer org templates        (org-wide overrides)
Level 4: Workspace templates           (domain-specific overrides)
Level 5: User overrides                (personal customization)
```

Same chain applies to benchmarks:
```
Level 1: Public benchmarks
Level 2: Provider benchmarks
Level 3: Org benchmarks
Level 4: Workspace benchmarks
```

**Why five levels, not four:** Claude Desktop's original framing had four levels (cartridge → org → workspace → user). GPT's review added the marketplace cartridge as a level above provider-org cartridges. This matters when a vendor's cartridge is distinct from a community-published cartridge from the marketplace. Without this distinction, vendor-specific standards can't override community defaults without handing over control of the community version. Five levels is the architecturally correct depth.

**Example resolution:** Home Services Marketing Cartridge provides defaults → Agency Org overrides tone and KPI thresholds → HVAC Client Workspace overrides local market benchmark assumptions → Analyst User saves preferred executive summary format. Most-specific (user) wins.

### Decision 5: Resource scoping

Every Mosaic resource has an explicit scope. A resource may not be accessed outside its scope without a grant.

| Resource | Default scope | Cross-scope access |
|---|---|---|
| Cube model | Workspace | Never (cubes don't span workspaces) |
| Cube cells | Workspace | Via explicit rollup model only |
| Audit logs | Workspace | Read-only via `view` grant to parent org |
| Write logs | Workspace | Read-only via `view` grant to parent org |
| Snapshots | Workspace | Via `fork` grant for cloning |
| Narratives | Workspace | Read-only via `view` grant |
| Templates | Org or Workspace | Workspace inherits from org; explicit grants for cross-org |
| Benchmarks | Org or Workspace | Same |
| Fitted models | Workspace (typically) | Via `fork` grant |
| Cartridges | Org-installed | Inherited by all workspaces in org; shared via grants |
| Plugin skills | Org-installed | Same |
| Members/teams | Org | Authorization scoped to workspace via team memberships |

**Scope violations fail closed.** An attempt to access a resource outside its scope returns an error, not empty data. Silently returning empty data is worse than failing; it produces unexplained gaps that operators mistake for real data.

### Decision 6: Cross-workspace correctness guardrail

Cross-workspace aggregation is **only** permitted when workspaces share a compatible cartridge or an explicit org-level rollup model. This is a correctness requirement, not just a permissions requirement.

**Why correctness matters here:** A marketing cube and a sports-betting cube may live in the same org. They have fundamentally different dimensions, measures, and semantics. Joining their data produces meaningless numbers that look authoritative. Mosaic must not enable this silently.

**What v1 MUST support:**
- List reports/findings across all workspaces in an org ("show me all warnings this month")
- Aggregate narrative ledger entries for workspaces using the same cartridge
- Compare workspaces using the same cartridge and same schema
- Org-level rollup dashboards where an explicit rollup model defines the aggregation semantics

**What v1 MUST NOT promise:**
- Universal joins across unrelated workspaces
- A single query language that understands all cube schemas transparently
- Cross-org raw data blending by default
- Automatic schema reconciliation when workspaces share dimension names but different semantics

**The safe rule:** any cross-workspace aggregation path must have an explicit cartridge-compatibility declaration or an org-level rollup model with defined semantics. If no such declaration exists, the query fails with an informative error.

### Decision 7: What v1 does NOT implement

The following items are explicitly deferred. They are listed here to prevent premature implementation:

- **Full enterprise RBAC.** Org/workspace membership and capability grants are enough for Phase 4C. Fine-grained RBAC (teams, roles, conditional grants) is Phase 9+.
- **Billing infrastructure.** Stripe, usage metering, subscription tiers — Phase 9 work.
- **SCIM provisioning.** Automated user sync from IdP (Okta, Azure AD) — Phase 9+.
- **SAML/OIDC admin surfaces.** SSO configuration UI — Phase 9+.
- **Partner marketplace.** Cartridge publishing, discovery, purchase flows — Phase 10+.
- **Cross-org analytics engine.** Federated queries, cross-tenant aggregation — deferred until privacy model is clear (Phase 7A.4's privacy-aware aggregation shapes the approach).
- **Org tree UI.** Visual org/workspace hierarchy browser — Phase 6B or later.

Committing the architecture now (Phase 4C) without building the heavy infrastructure means Phase 8 daemon and Phase 9 cloud inherit the correct structure without needing to retrofit it.

---

## Common business shapes illustrated

### Personal use
```
Org: Edwin's Workspace (personal)
├── Workspace: Sports Betting Research
├── Workspace: Marketing Finance
├── Workspace: Investment Portfolio
└── Workspace: Side Projects
```

### Small agency
```
Org: Brightside Agency
├── Workspace: HVAC Client
├── Workspace: Roofing Client
├── Workspace: Dentist Client
└── Workspace: Agency Internal Benchmarks
```

### Agency with enterprise client
```
Org: Brightside Agency
  manages (capability: use + view) →

Org: National Home Services Brand
├── Workspace: Paid Media
├── Workspace: SEO
├── Workspace: Franchise Reporting
└── Workspace: Executive Finance
```

### Partner/reseller network
```
Org: Mosaic Vendor
  grants (use + view) to →

Org: Partner Agency
  manages →

Org: Client A
├── Workspace: Marketing
└── Workspace: Reporting

Org: Client B
├── Workspace: Marketing
└── Workspace: Reporting
```

### Holding company
```
Org: Parent Company
├── Shared templates, benchmarks, executive reporting standards
│
├── Child Org: Acquired Brand A      [parent has: view + use]
│   ├── Workspace: Marketing
│   └── Workspace: Finance
│
└── Child Org: Acquired Brand B      [parent has: view + use]
    ├── Workspace: Marketing
    └── Workspace: Finance
```

---

## Alternatives considered

### Alt 1: Two-level model (org/workspace only)

Considered. Simpler model without managed-org relationships.

**Rejected because:**
- Fails for enterprise clients who need their own users, billing, and audit trails (can't be workspaces)
- Forces holding companies and franchise networks into a structure that doesn't match their legal/operational reality
- Works for agencies with simple client relationships; breaks for enterprise and partner network use cases

Two-level works for Phase 4C scope but the managed-org relationship concept needs to be designed now so Phase 8 daemon doesn't have to retrofit it.

### Alt 2: Three-level model with "Account/Client" as a distinct entity

Considered. GPT's original framing introduced "Account or Client" as a fourth entity alongside Org, Workspace, Managed-Org.

**Rejected because:**
- Creates vocabulary ambiguity: when someone says "client," do they mean an Account entity, a Workspace, or a Managed Org?
- The decision rule (workspace when parent owns the environment; separate org when client needs own identity) resolves every case without a third vocabulary concept
- An account IS a workspace or an org; adding a third type multiplies the vocabulary without adding structural expressiveness

Collapsed to: Org, Workspace, Managed-Org relationship.

### Alt 3: Enumerated relationship types (parent_of, partner_of, reseller_of, etc.)

Considered. Hard-code relationship types in the ADR; code checks relationship type.

**Rejected because:**
- Enums are notoriously hard to extend without migrating dependent code
- Real business relationships are messier than typed enums suggest (franchise = parent_of + reseller_of simultaneously)
- Once types are in ADRs, future code starts checking them, and blast radius grows
- Capability-based grants are attribute models; they scale to new relationship shapes without schema changes

Capability grants instead of type enums. This is how AWS IAM (attribute-based), Cloudflare account model, and GitHub team permissions work.

### Alt 4: Four-level template chain (skip marketplace layer)

Considered. Simpler four-level chain without the marketplace cartridge level.

**Rejected because:**
- A vendor's cartridge is architecturally distinct from a community-published marketplace cartridge
- Without the marketplace level, vendors can't have their standards override community defaults
- Phase 10+ marketplace use cases require the fifth level to be correct from the start; retrofitting it would require migrating all inheritance chains

Five levels is the correct depth.

---

## Out of scope

- Full Grout integration with signed grant chains (Phase 8.5+ — Grout research note)
- Billing and subscription model (Phase 9)
- SCIM/SAML user provisioning (Phase 9)
- Partner marketplace and cartridge publishing (Phase 10+)
- Cross-org federated analytics (deferred; privacy model depends on Phase 7A.4)
- Org tree administration UI (Phase 6B or later)
- API rate limiting per org (Phase 9)

---

## Cross-links

- **ADR-0025** — kernel discipline; binds this ADR via Decision 7 pointer
- **Phase 4C** — implementation vehicle for this architecture
- **Phase 8** — service daemon (must be org-aware; inherits from this ADR)
- **Phase 9** — cloud service (multi-tenant; tenant = org; inherits from this ADR)
- **ADR-0008** (Phase 4, LLM authoring + plugin ecosystem) — plugin skills are org-installed per Decision 5 here
- **ADR-0010** (Tessera) — Tessera imports are workspace-scoped per Decision 5 here
- **Phase 7A.4** (benchmark aggregation) — workspace-local benchmarks; org-level aggregation design shapes cross-workspace privacy model
- **Grout research note** [`../research-notes/grout-security-architecture-vision.md`] — signed grant chains are Grout's application-layer enforcement of these grants
- **Mosaic architecture and vision** [`../strategy/mosaic-architecture-and-vision.md`] — Part 3 is the detailed source for this ADR's decisions
- **Master phase plan** [`../roadmap/MASTER_PHASE_PLAN.md`]

---

## Notes

**Why design now, implement in Phase 4C.** The org/workspace structure is a design decision with long lead time — Phase 8 daemon and Phase 9 cloud both need to inherit it. Getting the shape right now (while it's cheap) prevents costly retrofits when daemon and cloud work begins. Implementation is Phase 4C; design is this ADR.

**The decision rule is load-bearing.** The workspace-vs-separate-org rule in Decision 2 resolves the most common ambiguity. Every implementation team that hits this question without the rule makes a different decision. The rule creates a defensible default that scales across business shapes.

**Capability grants enable the partnership model without IP loss.** A partner who uses your cartridges should not get raw control of your benchmark library — that's your moat. Capability grants (use without admin) let partners use Mosaic's institutional knowledge without controlling it. The marketplace business model depends on this distinction.

**Cross-workspace correctness is harder than cross-workspace permissions.** The guardrail in Decision 6 is a correctness requirement, not just a security requirement. Joining a marketing cube and a sports-betting cube produces nonsense numbers that look authoritative. The architecture must prevent this by design, not by convention.

**On the pattern recognition (Slack, GitHub, Hex, Cloudflare).** These four systems independently converged on similar org/workspace/resource patterns because those patterns fit enterprise software at scale. Mosaic is not copying them; it's recognizing that the convergence is signal. Where these systems differ from each other in detail, Mosaic picks the shape that fits its specific semantics (cell coordinate + provenance, not file or channel).
