# NBA Totals Cartridge Handoff — Sports-Betting Domain Cartridge

> **Audience:** a fresh Claude Code instance working on the NBA totals
> sports-betting cartridge. You inherit main at the latest commit with
> 694/0 tests.
>
> **This is a MODEL-ONLY task.** You are writing YAML + CSV + docs.
> You are NOT modifying any Rust crate. The formula engine already
> supports everything you need (predict, calibrate, norm_cdf, exp,
> if, safe_div, bucket, sum_over, lookup, actual_ref, time_anchor).
>
> **The existing draft at `examples/sports-betting/nba-totals.yaml`
> has schema issues.** Your job is to rebuild it correctly from scratch
> using actual production weights from the claw-core codebase.

---

## The one paragraph you must internalize

This cartridge proves that Mosaic can serve as a **production-grade
sports-betting evaluation engine** — replacing the Python inference
pipeline from claw-core with pure YAML formulas. The three-layer
Model-as-Judge architecture (Calculator → Judge → Investigator)
means the model doesn't just predict totals — it grades its own
predictions, detects failure modes, and identifies when NOT to bet.
The cartridge is both a working artifact AND a marketing
demonstration of Mosaic's capabilities.

---

## Dimension structure (6 dims — fits current kernel)

The current kernel requires exactly 6 dimensions. Use these 6
(all natural to the domain — no placeholders):

```yaml
dimensions:
  - name: "Scenario"
    kind: "Scenario"
    actuals_element: "Actual"
    elements:
      - { name: "Actual", scenario_meta: "Default" }
      - { name: "Predicted", scenario_meta: "NonDefault" }

  - name: "Version"
    kind: "Version"
    elements:
      - { name: "Working", version_state: "Draft" }

  - name: "Time"
    kind: "Time"
    granularity: "day"
    elements:
      # One element per game date (15-20 dates for the sample)
      - { name: "2025_04_15" }
      - { name: "2025_04_16" }
      # ...

  - name: "Sportsbook"
    kind: "Standard"
    elements:
      # Per-book lines enable line-shopping / edge optimization
      - { name: "Pinnacle" }      # sharp consensus
      - { name: "DraftKings" }
      - { name: "FanDuel" }

  - name: "Game"
    kind: "Standard"
    elements:
      # Each game is a leaf element
      - { name: "LAL_at_BOS" }
      - { name: "GSW_at_MIL" }
      - { name: "PHX_at_DEN" }
      # ... 15 games for the sample

  - name: "Measure"
    kind: "Measure"
    elements: []
```

**Why Sportsbook is the 6th dim (not a placeholder):** lines differ
per book. P(Over) at DraftKings ≠ P(Over) at Pinnacle because the
line is different. The edge-maximization problem is "which book has
the best line for my prediction?" — that's a cross-Sportsbook
operation. Without Sportsbook as a dimension, you can't model
line-shopping, which is where most profit comes from.

**Why not League:** for this NBA-only cartridge, League would be a
singleton ("NBA"). That adds no analytical value in the 6-dim
constraint. When ADR-0001 Amendment 1 ships (flexible dim count),
the cartridge expands to 7 dims adding League for multi-sport.

---

## Production weights from claw-core

Read these files to extract the REAL production coefficients:

1. `/Users/edwinlovettiii/Projects/claw-core/src/services/probability-model.ts`
   — the production Worker's fallback weights. Look for the coefficient
   array + intercept + residual_std. These are the deployed V1.6 Lasso
   weights converted from standardized to raw space.

2. `/Users/edwinlovettiii/Projects/claw-core/training/features.py`
   — the V1.6 feature list (which features have non-zero coefficients
   after Lasso sparsity). The model uses ~9-15 features out of 50+
   candidates.

3. `/Users/edwinlovettiii/Projects/claw-core/training/simulator/oddsmath.py`
   — the odds math (American→implied, vig removal, P(Over), EV, Kelly).
   Port these as formula rules.

4. `/Users/edwinlovettiii/Projects/claw-core/training/simulator/books.py`
   — book tier classification (sharp/mid/soft). Use as lookup data.

5. Search for calibration map output — look for any JSON file in
   `/Users/edwinlovettiii/Projects/claw-core/training/` containing
   `raw` and `calibrated` fields (from EXP-016). If not found, use
   8 reasonable PAVA points (slight overconfidence correction: raw
   0.52 → calibrated 0.50, raw 0.58 → calibrated 0.55, etc.).

---

## Three-layer rule architecture

### Layer 1: Calculator (predict + price)

```yaml
# 1. Raw prediction from fitted model
- name: "rule_predicted_total"
  target_measure: "Predicted_Total"
  body: "predict('nba_v16_lasso', avg_pace, combined_off_rating, ...)"
  declared_dependencies: [<all feature measures>]

# 2. P(Over) via normal CDF — uses the BOOK-SPECIFIC line (not consensus)
- name: "rule_p_over"
  target_measure: "P_Over"
  body: "1 - norm_cdf(Market_Line, Predicted_Total, 17.251)"
  declared_dependencies: ["Market_Line", "Predicted_Total"]

# 3. Calibrate the raw probability
- name: "rule_calibrated_p"
  target_measure: "Calibrated_P"
  body: "calibrate(P_Over, 'v16_calibration')"
  declared_dependencies: ["P_Over"]

# 4. Expected value per dollar (using BOOK-SPECIFIC odds)
- name: "rule_ev"
  target_measure: "EV_Per_Dollar"
  body: "Calibrated_P * (Decimal_Odds - 1) - (1 - Calibrated_P)"
  declared_dependencies: ["Calibrated_P", "Decimal_Odds"]

# 5. Quarter-Kelly sizing (0 if negative EV)
- name: "rule_kelly"
  target_measure: "Kelly_Fraction"
  body: "if(EV_Per_Dollar > 0, safe_div(Calibrated_P * Decimal_Odds - 1, Decimal_Odds - 1, 0) * 0.25, 0)"
  declared_dependencies: ["EV_Per_Dollar", "Calibrated_P", "Decimal_Odds"]
```

### Layer 2: Judge (grade predictions against actuals)

```yaml
# 6. Signed error
- name: "rule_prediction_error"
  target_measure: "Prediction_Error"
  body: "Predicted_Total - Actual_Total"
  declared_dependencies: ["Predicted_Total", "Actual_Total"]

# 7. Absolute error
- name: "rule_abs_error"
  target_measure: "Abs_Error"
  body: "abs(Predicted_Total - Actual_Total)"
  declared_dependencies: ["Predicted_Total", "Actual_Total"]

# 8. Direction correct (did we pick the right side?)
- name: "rule_direction_correct"
  target_measure: "Direction_Correct"
  body: |
    if((P_Over > 0.5 and Actual_Total > Market_Line)
       or (P_Over <= 0.5 and Actual_Total <= Market_Line), 1, 0)
  declared_dependencies: ["P_Over", "Actual_Total", "Market_Line"]

# 9. Brier score component
- name: "rule_brier"
  target_measure: "Brier_Component"
  body: "(Calibrated_P - Direction_Correct) * (Calibrated_P - Direction_Correct)"
  declared_dependencies: ["Calibrated_P", "Direction_Correct"]

# 10. Per-game profit (in units)
- name: "rule_profit"
  target_measure: "Profit_Units"
  body: |
    if(Kelly_Fraction > 0,
       if(Direction_Correct == 1, Decimal_Odds - 1, -1),
       0)
  declared_dependencies: ["Kelly_Fraction", "Direction_Correct", "Decimal_Odds"]
```

### Layer 3: Investigator (diagnose failure modes)

```yaml
# 11. Confidence bucket (for calibration analysis)
- name: "rule_confidence_bucket"
  target_measure: "Confidence_Bucket"
  body: "bucket(Calibrated_P, 'confidence_bins')"
  declared_dependencies: ["Calibrated_P"]

# 12. Overconfidence flag
- name: "rule_overconfidence"
  target_measure: "Overconfidence_Flag"
  body: "if(Calibrated_P > 0.60 and Direction_Correct == 0, 1, 0)"
  declared_dependencies: ["Calibrated_P", "Direction_Correct"]

# 13. Should bet (abstention logic)
- name: "rule_should_bet"
  target_measure: "Should_Bet"
  body: "if(EV_Per_Dollar > 0.03 and Calibrated_P > 0.56, 1, 0)"
  declared_dependencies: ["EV_Per_Dollar", "Calibrated_P"]

# 14. Error severity
- name: "rule_error_severity"
  target_measure: "Error_Severity"
  body: "bucket(Abs_Error, 'error_severity')"
  declared_dependencies: ["Abs_Error"]

# 15. Best book for this game (which sportsbook has highest EV?)
- name: "rule_best_book_ev"
  target_measure: "Best_Book_EV"
  body: "if(EV_Per_Dollar == sum_over('Sportsbook', EV_Per_Dollar), 1, 0)"
  declared_dependencies: ["EV_Per_Dollar"]
```

**Rule 15 is the line-shopping rule** — it identifies which book
offers the best EV for each game. `sum_over("Sportsbook", EV)` isn't
quite right for "max across books" (it sums, not maxes). Use
`if(EV_Per_Dollar >= max_ev_this_game, 1, 0)` where `max_ev_this_game`
is a separate intermediate. Or simplify: the Should_Bet rule already
gates on threshold; the user looks at the highest-EV book manually.
Keep it simple for the first cartridge.

---

## Sample data (CSV)

Create `nba-totals.inputs.csv` in long format:
```
Scenario,Version,Time,Sportsbook,Game,Measure,value
```

**15 games × 3 sportsbooks = 45 game-book combinations.** Each needs:
- All input features (avg_pace, combined_off_rating, etc.) — same per
  game across books (features don't change per book)
- Market_Line — DIFFERENT per book (this is why Sportsbook is a dim!)
- Decimal_Odds — DIFFERENT per book
- Actual_Total — same per game across books (one outcome)

**Realistic data ranges:**
- avg_pace: 96-104
- combined_off_rating: 215-230 (sum of two teams)
- combined_def_rating: 215-230
- avg_recent_total_5/10: 210-240
- Market_Line: 218-238 (varies 0.5-2 points across books)
- Decimal_Odds: 1.87-1.95 (varies per book; sharps closer to 1.91)
- Actual_Total: 195-260

**At least 3 games should show meaningful line differences across
books** (e.g., Pinnacle has 221.5, DraftKings has 222.0, FanDuel has
221.0) — this demonstrates why Sportsbook as a dimension matters.

---

## Golden tests

Pin at least 5-7 goldens covering:

1. A correct OVER prediction (P_Over > 0.5, actual > line, profit)
2. A correct UNDER prediction (P_Over < 0.5, actual < line)
3. A wrong prediction (Direction_Correct = 0, Overconfidence fires)
4. A game where EV > threshold at one book but not another
   (proves cross-book line-shopping)
5. The calibration check (calibrate() produces expected interpolated value)
6. Kelly = 0 for a negative-EV game (proves abstention)
7. A game where Should_Bet fires at Pinnacle but not DraftKings
   (different lines → different EV → different bet decision)

Golden #4 and #7 are the headline tests — they prove the Sportsbook
dimension adds real analytical value (different books → different
decisions for the same game).

---

## Reference data blocks

```yaml
fitted_models:
  - name: "nba_v16_lasso"
    method: "linear"
    intercept: <from claw-core probability-model.ts>
    coefficients:
      - { feature: "avg_pace", weight: <real value> }
      # ... 9 non-zero features from V1.6 Lasso
    residual_std: 17.251
    metadata:
      fitted_at: "2026-04-11T00:00:00Z"
      algorithm: "lasso"
      alpha: 0.7
      n_train: 3685
      holdout_mae: 13.783

calibration_maps:
  - name: "v16_calibration"
    method: "pava"
    points:
      - { raw: 0.48, calibrated: 0.46 }
      - { raw: 0.52, calibrated: 0.50 }
      # ... 8 points total
    metadata:
      fitted_at: "2026-04-30T01:29:14Z"
      sample_size: 1312
      calibrated_brier: 0.2453

status_thresholds:
  - name: "confidence_bins"
    bands:
      - { label: "Low", max: 0.54 }
      - { label: "Moderate", max: 0.58 }
      - { label: "Good", max: 0.62 }
      - { label: "Strong", max: 0.66 }
      - { label: "Very Strong" }

  - name: "error_severity"
    bands:
      - { label: "Good", max: 8.0 }
      - { label: "Moderate", max: 15.0 }
      - { label: "Bad", max: 25.0 }
      - { label: "Severe" }
```

---

## Acceptance gates

1. `mc model validate nba-totals.yaml` — 0 diagnostics
2. `mc model lint nba-totals.yaml` — 0 errors (info/warnings OK for chain depth)
3. `mc model test nba-totals.yaml` — all goldens pass
4. Golden #4 / #7 proves cross-book EV differs (Sportsbook dim has value)
5. Layer 2 rules produce non-Null grading for games with Actual_Total
6. Layer 3 rules produce non-Null diagnostic flags
7. predict() returns real predictions (not Null) — verified by golden #1
8. calibrate() returns real calibrated values (not Null) — verified by golden #5
9. `declared_dependencies` correct for every rule (no MC2005 / MC2010)
10. All aggregations are `Sum` (games are independent; no weighted consolidation needed)

---

## Files to produce

```
examples/sports-betting/
├── nba-totals.yaml              # COMPLETE model (overwrite existing draft)
├── nba-totals.inputs.csv        # 15 games × 3 books = ~675 rows (long format)
├── README.md                    # Update existing with correct dim structure
└── export-from-sklearn.py       # Keep existing (already correct)
```

---

## Hard rules

- **Do NOT modify any Rust crate.** This is YAML + CSV only.
- **Use REAL weights from claw-core** if you can find them. If not, use
  realistic Lasso coefficients (intercept ~-400 to -450; pace coef ~3;
  off/def rating coefs ~0.5; residual_std ~17.25).
- **All measures use `aggregation: "Sum"`** (games are independent leaves;
  no weighted consolidation across games makes analytical sense).
- **The CSV is long-format:** `Scenario,Version,Time,Sportsbook,Game,Measure,value`
- **At least 3 books with DIFFERENT lines per game** (the whole point of
  the Sportsbook dimension is that lines differ).
- **Don't invent features.** Use only the features from claw-core's
  V1.6 feature set (check `training/features.py` for the exact names
  that survive Lasso sparsity: avg_pace, combined_off_rating,
  combined_def_rating, avg_recent_total_5, avg_recent_total_10,
  home_recent_total_avg, combined_stl, home_missing_top_scorers,
  away_missing_top_scorers, pace_delta — approximately these 9-10).

---

## The honest disclaimer (must be in README)

> "This cartridge demonstrates Mosaic's model-evaluation capabilities
> using production-derived weights from a real sports-betting model.
> It is NOT a guarantee of profitability. The model may have edge or
> it may not — Mosaic tells you which, honestly, with measured evidence.
> Past performance does not guarantee future results. Use this as a
> framework for rigorous self-evaluation, not as betting advice."

---

## Resolution order

1. The claw-core source code (for real weights + features)
2. `mosaic-plugin/skills/formulas/SKILL.md` (for formula syntax)
3. `mosaic-plugin/skills/schema-design/SKILL.md` (for YAML structure)
4. `mosaic-plugin/skills/fitted-models/SKILL.md` (for predict/calibrate)
5. `crates/mc-model/examples/acme.yaml` (for structural reference)
6. `docs/research-notes/model-as-judge-architecture.md` (for the 3-layer pattern)

If anything is ambiguous, write a SPEC QUESTION and wait.
Do NOT commit. Report DONE when all 10 acceptance gates pass.
