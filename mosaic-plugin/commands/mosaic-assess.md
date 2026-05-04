---
name: mosaic-assess
description: Discover the user's data sources and propose a tailored Mosaic model. Acts as a solutions architect — analyzes files, databases, and business context, then shows specifically what Mosaic can do with concrete numbers from their actual data. On approval, builds the model + recipes + loads data end-to-end.
arguments:
  - name: context
    description: Optional natural-language description of what you want to model, or a path to a data file. If omitted, the agent discovers data sources in the current directory.
    required: false
---

# /mosaic-assess

Run a Mosaic consultation on the user's data. The mosaic-consultant agent handles the entire flow:

1. **Discover** — find data sources in the user's environment (files, DBs, APIs)
2. **Sample** — read headers + sample rows to understand the data shape
3. **Classify** — identify dimensions (categorical) vs measures (numeric), and which measures should be Derived (calculated from others)
4. **Propose** — present a tailored model design with specific numbers from the user's data
5. **Demonstrate value** — show a derived measure calculating from real inputs; show what happens when an input changes
6. **Build (on approval)** — generate model YAML + import recipe + load data

## Usage

```
/mosaic-assess
```
→ Discovers data in the current directory and proposes a model.

```
/mosaic-assess "I have campaign performance data and want to forecast ROI"
```
→ Focuses the assessment on the described business problem.

```
/mosaic-assess path/to/data.xlsx
```
→ Assesses the specific file and proposes a model for it.

## What it produces

On completion (with user approval at each gate), you get:
- A validated Mosaic model YAML (dimensions, measures, rules)
- An import recipe mapping your data to the cube
- Your actual data loaded into the cube (if mc tessera is available)
- A demonstration of derived measures calculating from real inputs
- A "what if" example showing how changing an input affects derived metrics

## Prerequisites

- `mc` binary on PATH (for validation: `mc model validate/lint`)
- Data source accessible (file readable, DB connectable if relevant)
- For full end-to-end (loading data): `mc tessera apply` available

## The value proposition in one sentence

> "Show me your data; I'll show you your forecast."
