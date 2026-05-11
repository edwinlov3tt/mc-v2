# How Workspaces Work (for-dummies)

> You just shipped Phase 4C. Here's how to actually use it.

---

## The 30-second version

A **workspace** is a folder with a `workspace.yaml` file that tells Mosaic "here are my cubes, here are my shared resources, here's how they connect." It's the layer above individual cube YAML files.

**Without a workspace:** each cube is an island. You run `mc model validate my-cube.yaml` one file at a time.

**With a workspace:** cubes share dimension catalogs, fitted models, and links. You run `mc workspace validate` and everything validates together.

---

## How to create one right now

```bash
# Option 1: Scaffold from scratch
mc workspace init my-project
cd my-project
# Creates:
#   my-project/
#     workspace.yaml
#     cubes/
#     catalogs/

# Option 2: Scaffold with a domain hint
mc workspace init sports-research --domain sports-betting
```

---

## What workspace.yaml looks like

```yaml
workspace_format_version: 1
name: "acme-marketing"
id: "acme-marketing"
description: "Acme Corp marketing finance workspace"

# Shared dimension catalogs — cubes reference these via $ref
shared_dimensions:
  - id: "Channel"
    source: "catalogs/channels.yaml"
  - id: "Market"
    source: "catalogs/markets.yaml"

# Cubes in this workspace
cubes:
  - path: "cubes/marketing-finance.yaml"
  - path: "cubes/brand-awareness.yaml"

# Cross-cube links (declarative — engine doesn't enforce yet, but tooling validates)
links:
  - from_cube: "marketing-finance"
    from_measure: "Revenue"
    to_cube: "brand-awareness"
    to_measure: "Revenue_Reference"
    kind: "ReadOnly"
    description: "Brand awareness cube references marketing revenue"
```

---

## What a shared catalog looks like

`catalogs/channels.yaml`:
```yaml
catalog_format_version: 1
dimension: "Channel"
elements:
  - { name: "Paid_Search", parent: "Digital" }
  - { name: "Display", parent: "Digital" }
  - { name: "Social", parent: "Digital" }
  - { name: "TV", parent: "Traditional" }
  - { name: "Print", parent: "Traditional" }
hierarchy:
  - { name: "Digital", children: ["Paid_Search", "Display", "Social"] }
  - { name: "Traditional", children: ["TV", "Print"] }
  - { name: "All_Channels", children: ["Digital", "Traditional"] }
```

---

## How cubes reference shared catalogs ($ref)

In your cube YAML, instead of declaring elements inline:

```yaml
# Before (standalone cube — all elements inline)
dimensions:
  - name: "Channel"
    kind: "Standard"
    elements:
      - { name: "Paid_Search", parent: "Digital" }
      - { name: "Display", parent: "Digital" }
      # ... 20 more lines
```

You write:

```yaml
# After (workspace cube — reference the shared catalog)
dimensions:
  - name: "Channel"
    kind: "Standard"
    $ref: "workspace://shared_dimensions/Channel"
```

When you run `mc workspace validate`, the resolver reads `catalogs/channels.yaml` and inlines those elements into the cube. The cube YAML stays small and DRY. Two cubes sharing the same Channel dimension reference the same catalog — change it once, both cubes update.

---

## The CLI verbs

```bash
# Validate everything (cubes + catalogs + links)
mc workspace validate
# or from another directory:
mc workspace validate --path ~/projects/acme-workspace

# Lint (naming consistency, unused catalogs, missing descriptions)
mc workspace lint

# Run all cube goldens (tests)
mc workspace test

# Show what's in the workspace
mc workspace inspect
mc workspace inspect --format json
```

---

## Directory layout (what a real workspace looks like)

```
acme-workspace/
├── workspace.yaml              ← the manifest
├── catalogs/                   ← shared dimension catalogs
│   ├── channels.yaml
│   ├── markets.yaml
│   └── time-periods.yaml
├── cubes/                      ← one YAML per cube
│   ├── marketing-finance.yaml
│   └── brand-awareness.yaml
├── fitted/                     ← shared fitted models (optional)
│   └── mmm-lasso-v3.yaml
├── fixtures/                   ← inputs + test data
│   ├── canonical-inputs/
│   │   └── marketing-finance.inputs.csv
│   └── test-fixtures/
└── .mosaic/                    ← runtime state (gitignored)
    ├── analysis-ledger.jsonl
    └── benchmark-library.json
```

---

## How orgs work (for agencies / multi-workspace setups)

An **org** is a folder with an `org.yaml` that groups workspaces:

```yaml
org_format_version: 1
name: "Brightside Agency"
id: "brightside"
description: "All client workspaces"

workspaces:
  - path: "workspaces/hvac-client"
    name: "HVAC Client"
  - path: "workspaces/roofing-client"
    name: "Roofing Client"
  - path: "workspaces/agency-benchmarks"
    name: "Internal Benchmarks"
```

Directory layout:
```
brightside/
├── org.yaml
├── templates/              ← org-level templates (inherited by all workspaces)
├── benchmarks/             ← org-level benchmarks
└── workspaces/
    ├── hvac-client/
    │   ├── workspace.yaml
    │   └── cubes/...
    ├── roofing-client/
    │   ├── workspace.yaml
    │   └── cubes/...
    └── agency-benchmarks/
        ├── workspace.yaml
        └── cubes/...
```

---

## What you CAN'T do yet (future phases)

| Feature | When |
|---|---|
| Cross-cube formula refs (`cube_a::Revenue` in a rule body) | Phase 5+ (Tier C — kernel changes needed) |
| Capability grants (use/view/fork permissions between orgs) | Phase 8 (daemon enforces) |
| Multi-user access control | Phase 9 (cloud) |
| `mc workspace add-cube` / `mc workspace add-catalog` | Future polish |
| Hot-reload (auto-detect catalog changes) | Phase 6B (UI) |
| Cartridge marketplace (install domain packs) | Phase 10+ |

---

## Quick start: convert an existing cube to a workspace

If you have a standalone `my-cube.yaml` and want to move it into a workspace:

```bash
# 1. Create the workspace
mc workspace init my-project
cd my-project

# 2. Move your cube
mv ../my-cube.yaml cubes/

# 3. Edit workspace.yaml to list it
#    cubes:
#      - path: "cubes/my-cube.yaml"

# 4. (Optional) Extract shared dimensions into catalogs
#    Pull dimension elements out of the cube YAML into catalogs/
#    Replace inline elements with $ref: "workspace://shared_dimensions/DimName"

# 5. Validate
mc workspace validate
```

Your existing standalone cube still works fine without a workspace. Workspaces are additive — they don't replace single-file authoring for simple cases.

---

## The real example: Acme as a workspace

Check out `examples/acme-workspace/` in the repo. It's the Acme Marketing Finance demo cube converted to workspace form with shared Channel and Market catalogs. Run:

```bash
mc workspace validate --path examples/acme-workspace
mc workspace inspect --path examples/acme-workspace
```

---

## Element types (bonus feature from Phase 4C)

Dimensions can now declare what type their elements should be:

```yaml
dimensions:
  - name: "Time"
    kind: "Standard"
    element_type: "date"       # validates all elements parse as dates
    elements: [...]

  - name: "Revenue_Bucket"
    kind: "Standard"
    element_type: "numeric"    # validates all elements parse as numbers
    elements: [...]
```

Types: `string` (default, no validation), `date` (elements must parse as time periods), `numeric` (elements must parse as numbers). Useful for catching typos in dimension definitions early.
