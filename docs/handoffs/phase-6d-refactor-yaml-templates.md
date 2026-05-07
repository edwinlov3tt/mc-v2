# Phase 6D Refactor — YAML-Driven Narrative Template Engine

> **For the implementing instance.** This refactor replaces the
> hardcoded Rust template functions in `narrative.rs` with a
> YAML-driven template engine. The engine parses template
> definitions from `demo/narratives/*.yaml` at startup and
> evaluates them generically at request time.
>
> **Why this matters:** Without this, the demo is theater — "we
> moved if-statements from JavaScript to Rust." With this, the
> demo proves the concept: "analysts and LLMs author templates
> in YAML; the engine runs them deterministically forever. No
> code changes needed to add templates."
>
> **Scope:** ~3-4 hours of focused work. The YAML template file
> already exists at `demo/narratives/display-like.yaml` with all
> 13 template definitions. The work is: build the generic evaluator
> that reads them.

---

## The goal

**Before (current — 1,063 lines of Rust):**
```rust
fn eval_engagement_acceleration(cube: &IngestedCube) -> Vec<NarrativeOutput> {
    let click_growth = /* hardcoded formula */;
    let impr_growth = /* hardcoded formula */;
    if click_growth > impr_growth * 1.5 {
        vec![NarrativeOutput {
            text: format!("Engagement is accelerating...clicks grew {:.0}%...", click_growth),
            template_id: "engagement_acceleration".to_string(),
            ...
        }]
    } else { vec![] }
}
// Repeated 13 times with different hardcoded logic
```

**After (this refactor — ~500 lines of generic engine + ~200 lines of YAML):**
```rust
// narrative.rs becomes a GENERIC evaluator:
fn evaluate_templates(templates: &[Template], cube: &IngestedCube) -> Vec<NarrativeOutput> {
    templates.iter()
        .filter(|t| t.applies_to(cube))           // table_type match
        .filter(|t| t.when.evaluate(&cube.context()))  // predicate check
        .map(|t| t.render(&cube.context()))        // binding resolution + string substitution
        .collect()
}

// Templates are DATA loaded from YAML at startup:
// demo/narratives/display-like.yaml defines the templates
// Adding a new template = adding 10 lines of YAML, zero Rust changes
```

**The pitch this enables:** "See this YAML file? An analyst wrote
these rules. Or an LLM authored them. The engine runs them in 1ms.
Want to add a new insight? Add 10 lines of YAML. No deploy, no
code review, no Rust compiler. Just YAML."

---

## Architecture

```
Startup (mc start):
  1. Load demo/narratives/*.yaml
  2. Parse into Vec<TemplateDefinition>
  3. Compile `when:` predicates into evaluatable expressions
  4. Store in AppState (shared across requests)

Per-request (POST /api/upload):
  1. Ingest CSVs → cubes (existing, unchanged)
  2. For each cube:
     a. Build an EvalContext from cube data (named values)
     b. Filter templates by table_type match
     c. Evaluate each template's `when:` against the context
     d. For templates that fire: resolve bindings → substitute into template string
  3. Return NarrativeOutput vec (existing shape, unchanged)
```

---

## The 3 components to build

### 1. Template YAML parser (~100 lines)

Parse `demo/narratives/*.yaml` into:

```rust
#[derive(Debug, Deserialize)]
struct TemplateFile {
    narrative_format_version: u32,
    templates: Vec<TemplateDefinition>,
}

#[derive(Debug, Deserialize)]
struct TemplateDefinition {
    id: String,
    family: Vec<String>,
    severity: Severity,
    table_types: Vec<String>,
    #[serde(default)]
    sort_order: i32,
    when: String,              // expression string — evaluated at runtime
    template: String,          // output string with {placeholder} syntax
    bindings: BTreeMap<String, String>,  // name → expression string
}
```

Use `serde_yaml` (already in the workspace via mc-model). Load all
`*.yaml` files from `demo/narratives/` at startup.

### 2. Expression evaluator (~250 lines)

A mini evaluator that handles the expressions in `when:` and `bindings:`.

**What it needs to evaluate:**

```
// Arithmetic
current.Impressions - prev.Impressions
(current.Clicks - prev.Clicks) / prev.Clicks * 100
abs(click_growth)

// Comparisons
period_count >= 2
campaign_avg.CTR > 0.30
min_by.Device.CTR.value < campaign_avg.CTR * 0.25

// Logical
period_count >= 2 AND current.Impressions > prev.Impressions
count_where(Impressions < 500, geo_dimension) > 0

// Conditionals (for bindings that produce strings)
if(current.Clicks >= prev.Clicks, 'grew', 'declined')
if(abs_pct > 100, 'more than doubled', if(abs_pct > 50, 'surged', 'increased'))

// Built-in functions
abs(x), min(a, b), max(a, b)
count_where(condition, dimension)
any_where(condition, dimension)
names_where(condition, dimension)
first_where(condition, dimension)
```

**Implementation approach (RECOMMENDED — keep it simple for the demo):**

Don't build a full recursive-descent parser. Instead:

1. **Pre-compute ALL possible named values** into a flat `HashMap<String, Value>` (the "context") before template evaluation. For example:
   ```rust
   context = {
     "current.Impressions": 30655.0,
     "prev.Impressions": 25102.0,
     "period_count": 2.0,
     "campaign_avg.CTR": 0.44,
     "max_by.Device.CTR.name": "Tablet",
     "max_by.Device.CTR.value": 0.83,
     "min_by.Device.CTR.name": "PC Desktop or Laptop",
     "min_by.Device.CTR.value": 0.07,
     "sum.Impressions": 55757.0,
     "sum.Conversions": 0.0,
     "click_growth": 110.0,  // pre-computed common derivations
     "impr_growth": 22.0,
     ...
   }
   ```

2. **Evaluate `when:` predicates** against this context using a
   simple expression evaluator. Since the context is pre-computed,
   most predicates reduce to `variable > constant` or
   `variable AND variable`. A ~150-line evaluator handles:
   - Variable lookup: `context.get(name)`
   - Constants: numeric literals, string literals in `'quotes'`
   - Operators: `+`, `-`, `*`, `/`, `>`, `<`, `>=`, `<=`, `==`, `!=`, `AND`, `OR`, `NOT`
   - Functions: `abs()`, `if(cond, then, else)`
   - The special `true` literal (always-fire templates)

3. **Evaluate `bindings:`** — same evaluator, but the result can be
   either a number (formatted per the template's format hints) or
   a string (from `if(...)` conditionals).

4. **Substitute into template:** regex-replace `{name}` and
   `{name:format}` with the resolved binding values.

**Why pre-computed context works:** The cube data is finite and known
at evaluation time. There are ~50-80 possible named values for any
given cube (current/prev values for each measure × a few aggregate
functions). Pre-computing them ALL into a HashMap before template
evaluation means the evaluator never needs to call back into the
cube — it's just arithmetic + comparison on a flat namespace.

### 3. Context builder (~150 lines)

Builds the `HashMap<String, Value>` from an `IngestedCube`:

```rust
fn build_context(cube: &IngestedCube) -> HashMap<String, Value> {
    let mut ctx = HashMap::new();

    // Period info
    ctx.insert("period_count", Value::Num(cube.periods.len()));
    ctx.insert("current.period_name", Value::Str(cube.current_period_name()));
    ctx.insert("prev.period_name", Value::Str(cube.prev_period_name()));

    // Per-measure: current, prev, sum, campaign_avg
    for measure in &cube.measures {
        ctx.insert(format!("current.{}", measure.name), Value::Num(measure.current()));
        ctx.insert(format!("prev.{}", measure.name), Value::Num(measure.prev()));
        ctx.insert(format!("sum.{}", measure.name), Value::Num(measure.sum()));
        ctx.insert(format!("campaign_avg.{}", measure.name), Value::Num(measure.avg()));
    }

    // Per-dimension aggregates: max_by, min_by
    for dim in &cube.dimensions {
        for measure in &cube.measures {
            let (max_name, max_val) = dim.max_by(measure);
            let (min_name, min_val) = dim.min_by(measure);
            ctx.insert(format!("max_by.{}.{}.name", dim.name, measure.name), Value::Str(max_name));
            ctx.insert(format!("max_by.{}.{}.value", dim.name, measure.name), Value::Num(max_val));
            ctx.insert(format!("min_by.{}.{}.name", dim.name, measure.name), Value::Str(min_name));
            ctx.insert(format!("min_by.{}.{}.value", dim.name, measure.name), Value::Num(min_val));
        }
    }

    // Pre-computed derivations (common formulas)
    if let (Some(cur_clicks), Some(prev_clicks)) = (ctx.get_num("current.Clicks"), ctx.get_num("prev.Clicks")) {
        if prev_clicks > 0.0 {
            ctx.insert("click_growth", Value::Num((cur_clicks - prev_clicks) / prev_clicks * 100.0));
        }
    }
    // ... similar for impr_growth, ctr_change, etc.

    // count_where / any_where / names_where (pre-computed per dimension)
    for dim in &cube.dimensions {
        let low_sample = dim.elements.iter().filter(|e| e.impressions < 500.0).count();
        ctx.insert(format!("count_where(Impressions < 500, {})", dim.name), Value::Num(low_sample));
        // ... etc
    }

    ctx
}
```

---

## What gets deleted

The entire body of `narrative.rs`'s 13 `eval_*` functions (~900
lines) gets replaced by the generic `evaluate_templates` function
(~50 lines) + the expression evaluator (~250 lines) + the context
builder (~150 lines) + the YAML loader (~50 lines). Net: from
1,063 lines of hardcoded logic to ~500 lines of generic engine.

The `NarrativeOutput` struct and `evaluate_all` public boundary
stay unchanged — callers don't know the engine was refactored.

---

## Decision Matrix

| Wall | Binding decision |
|---|---|
| Full recursive-descent parser for expressions, or simpler approach? | **Pre-computed context + simple evaluator.** Don't build a full parser. Pre-compute ~80 named values; the evaluator just does lookups + arithmetic + comparisons. This is fast, simple, and sufficient for the template patterns we have. |
| Where does `serde_yaml` come from? | **Already in workspace** via mc-model. mc-demo-server can add `serde_yaml.workspace = true` to its Cargo.toml. |
| What if a template's `when:` expression references a variable not in the context? | **Template doesn't fire** (treated as false). Log a warning to stderr. Don't crash. |
| What if a binding's expression fails to evaluate? | **Use `"N/A"` as the substitution value.** The narrative still renders; the specific number is just missing. Log warning. |
| Should the YAML templates be hot-reloadable (change YAML, see new narratives without restart)? | **No for the demo.** Templates load at startup. Restart to pick up changes. Hot-reload is a Phase 7A.1 feature. |
| Format hints (`{value:.2f}`, `{value:,.0f}`) — implement or skip? | **Implement.** This is what makes the output look professional. Use Rust's `format!` macro with runtime format strings (or a simple formatter that handles `.Nf` and `,.Nf` patterns). |
| `if(cond, then, else)` in bindings — how deep can they nest? | **2 levels deep max** (i.e., `if(a, x, if(b, y, z))`). The YAML templates use this for verb selection ("more than doubled" / "surged" / "increased"). Don't support arbitrary recursion; 2 levels covers all current templates. |

---

## Acceptance criteria

- [ ] `demo/narratives/display-like.yaml` is the source of truth for all templates
- [ ] `narrative.rs` contains ZERO hardcoded template strings or hardcoded comparison logic — all of it comes from the YAML
- [ ] Adding a new template (append 10 lines to the YAML file) produces new narrative output WITHOUT any Rust code change or recompilation (verify by adding a test template, restarting `mc start`, re-uploading)
- [ ] All 17 narratives still fire on the sample data (same output as before the refactor — regression check)
- [ ] Processing time stays under 200ms (the YAML parse at startup doesn't affect per-request time; the pre-computed context approach keeps per-request eval fast)
- [ ] The `evaluate_all` public function signature is unchanged (callers don't know about the refactor)
- [ ] `cargo test --workspace` still passes 926/0/5

---

## The demo script addition

After the refactor, add this to the demo flow:

1. Show the YAML file to leadership: "See these 10 lines? That's
   one analytical rule. An analyst wrote it — or an LLM authored
   it during setup. No code."
2. **Live edit during the demo:** Open the YAML, add a simple
   template (e.g., a new threshold), restart `mc start`, re-upload.
   New narrative appears. "That took 30 seconds and zero code."
3. "The engine processes 13 templates in 1ms. Adding 100 more
   templates doesn't make it slower — they're all evaluated in
   the same pass against the same pre-computed context."

That's the moment the demo goes from "impressive" to "this is
real." The YAML file IS the product.

---

## Commit strategy

One commit: `refactor(6D): YAML-driven narrative template engine`

This is a single cohesive refactor — splitting it into multiple
commits would just create intermediate broken states. One commit
that replaces the hardcoded functions with the generic engine +
keeps all tests passing.
