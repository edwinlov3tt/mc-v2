# NBA Totals Prediction Cartridge

A production-grade Mosaic domain cartridge for NBA game-total (over/under) predictions, using real V1.6 Lasso model weights from the claw-core production system.

## What This Cartridge Does

This cartridge evaluates NBA game totals across three analytical layers:

| Layer | Name | Purpose |
|-------|------|---------|
| 1 | **Calculator** | Predict game total, compute P(Over), calibrate, price EV, size via Kelly |
| 2 | **Judge** | Grade predictions against actuals: error magnitude, direction accuracy, Brier score |
| 3 | **Investigator** | Diagnose failure modes: overconfidence, pace-driven misses, bet quality signals |

## The Model

**Algorithm:** Lasso regression (L1, alpha=0.7) with StandardScaler  
**Training data:** 3,685 NBA games (2022-23, 2023-24, 2024-25 seasons)  
**Holdout performance:** MAE=13.783 pts, Direction=66.4%, Residual std=17.251  
**Non-zero features:** 9 of 50 candidates survive L1 sparsity  

Key surviving features (by raw coefficient magnitude):
- `avg_pace` (+3.016) — pace is the single strongest predictor
- `home_missing_top_scorers` (-1.203) — missing stars significantly reduces total
- `combined_def_rating` (+0.602) — higher defensive rating = more points allowed
- `combined_off_rating` (+0.548) — offensive firepower
- `avg_recent_total_10` (+0.331) — recent scoring trends

The intercept (-435.18) is large and negative because the model was fit in standardized space then converted to raw-feature space for direct inference.

## Quick Start

```bash
# Validate the model structure
mc model validate examples/sports-betting/nba-totals.yaml

# Run lint checks
mc model lint examples/sports-betting/nba-totals.yaml

# Run golden tests (hand-computed expected values)
mc model test examples/sports-betting/nba-totals.yaml

# Evaluate all games with the full pipeline
mc model eval examples/sports-betting/nba-totals.yaml
```

## Interpreting Layer 2 Output (Judge)

After games settle (Actual_Total is populated), Layer 2 produces:

| Measure | Interpretation |
|---------|---------------|
| `Prediction_Error` | Signed error. Positive = model over-predicted. Watch for systematic bias. |
| `Abs_Error` | Magnitude of miss. Typical range: 8-18 pts (1 sigma = 17.25). |
| `Direction_Correct` | Binary: did the model predict the correct side vs the market line? Target: >60%. |
| `Brier_Component` | Per-game calibration score. Lower = better. Good range: 0.20-0.25. |
| `Confidence_Bucket` | 1-5 scale. Higher confidence bets should have higher win rates (calibration check). |
| `Profit_Units` | P/L in units if the Kelly signal was followed. Aggregates to portfolio return. |

**Healthy model signature:**
- Mean `Prediction_Error` near 0 (no systematic bias)
- Mean `Abs_Error` < 14 (better than MAE threshold)
- `Direction_Correct` sum / game count > 0.60
- `Profit_Units` sum > 0 over 50+ games

## Interpreting Layer 3 Output (Investigator)

Layer 3 identifies WHERE and WHY the model fails:

| Measure | What It Catches |
|---------|-----------------|
| `Overconfidence_Flag` | Games where Calibrated_P > 0.60 but prediction was wrong. These are the expensive failures. |
| `High_Pace_Miss` | High-pace games (>102 possessions/48) with large misses (>15 pts). The model may systematically mis-weight pace in uptempo matchups. |
| `Should_Bet` | Signal: EV > 3% AND confidence > 56%. These are the games the model would bet on. |
| `Error_Severity` | 1=good(<8), 2=moderate(8-15), 3=bad(15-25), 4=severe(>25). Track bucket distribution over time. |
| `Revenue_Share` | Which games drive portfolio P/L. Helps identify whether profits come from many small edges or a few large ones. |

**Investigation workflow:**
1. Filter for `Overconfidence_Flag = 1` games
2. Check: are they clustered in specific matchup types (e.g., playoff games)?
3. Check: does `High_Pace_Miss` correlate with conference or time-of-season?
4. If `Error_Severity = 4` is frequent, the model may need retraining or a contextual offset

## Running a What-If Analysis

Change any input feature to see how the prediction shifts:

```bash
# What if the home team's top scorer is out?
mc model eval examples/sports-betting/nba-totals.yaml \
  --override "Game=LAL_at_BOS,Measure=home_missing_top_scorers,value=1"

# The predicted total should drop by ~1.2 points (the coefficient for home_missing_top_scorers)
```

For systematic exploration:

```bash
# Sweep avg_pace from 96 to 104 to see sensitivity
mc model sweep examples/sports-betting/nba-totals.yaml \
  --measure avg_pace \
  --range 96:104:0.5 \
  --target Predicted_Total \
  --game LAL_at_BOS
```

## Using Sweep (Phase 3H.1)

Once `mc model sweep` ships, you can run sensitivity analysis:

```bash
# How does the predicted total vary with pace?
mc model sweep nba-totals.yaml \
  --feature avg_pace --from 96 --to 104 --steps 20 \
  --output sweep_pace.csv

# How does EV change with the market line?
mc model sweep nba-totals.yaml \
  --feature Market_Line --from 210 --to 240 --steps 30 \
  --output sweep_line.csv

# Multi-feature sweep: pace x off_rating interaction
mc model sweep nba-totals.yaml \
  --feature avg_pace --from 97 --to 103 --steps 7 \
  --feature combined_off_rating --from 215 --to 235 --steps 5 \
  --output sweep_pace_off.csv
```

## Calibration

The calibration map (PAVA isotonic regression) was fit on 1,312 out-of-distribution settled bets from the 2020-21 and 2021-22 seasons (neither used in model training). It shows:

- The model is slightly **overconfident** across most of the range (raw > calibrated)
- Brier score improvement: +1.5% (raw 0.2491 -> calibrated 0.2453)
- The correction is small because the raw model is already reasonably calibrated

Calibration points (raw probability -> calibrated probability):
```
0.510 -> 0.487  (overconfident by 2.3pp)
0.535 -> 0.518  (overconfident by 1.7pp)
0.560 -> 0.542  (overconfident by 1.8pp)
0.590 -> 0.571  (overconfident by 1.9pp)
0.625 -> 0.609  (overconfident by 1.6pp)
0.670 -> 0.652  (overconfident by 1.8pp)
0.730 -> 0.714  (overconfident by 1.6pp)
0.820 -> 0.791  (overconfident by 2.9pp)
```

## Book Tier Classification

The `book_tiers` lookup table classifies sportsbooks by line quality:

| Tier | Books | Use |
|------|-------|-----|
| Sharp (1) | Pinnacle, Circa, Bookmaker | Consensus benchmark for CLV measurement |
| Mid (2) | FanDuel, BetRivers, William Hill, Unibet, SuperBook | Decent lines, some lag behind sharp |
| Soft (3) | DraftKings, BetMGM, Caesars, PointsBet, Bovada, etc. | Wider vig, slower line movement — where edge lives |

**Strategy:** The model finds edge by comparing its predictions against soft-tier lines. Sharp books are used as the truth benchmark for Closing Line Value (CLV) calculation.

## Exporting Your Own Model

If you have a fitted sklearn model, convert it to Mosaic format:

```bash
# Basic export (model already in raw feature space)
python export-from-sklearn.py \
  --model my_model.pkl \
  --feature-names avg_pace combined_off_rating combined_def_rating \
  --name "my_nba_model" \
  --residual-std 17.5 \
  --output my_weights.yaml

# With StandardScaler conversion (model trained on scaled features)
python export-from-sklearn.py \
  --model my_model.pkl \
  --scaler my_scaler.pkl \
  --features feature_names.json \
  --name "my_nba_model_v2" \
  --residual-std 16.8 \
  --output my_weights.yaml
```

The script handles the standardized-to-raw coefficient conversion automatically when a scaler is provided.

## File Structure

```
examples/sports-betting/
├── nba-totals.yaml           # Complete model: dimensions, measures, rules, fitted artifacts
├── nba-totals.inputs.csv     # 15 sample games with full feature vectors + actuals
├── README.md                 # This file
└── export-from-sklearn.py    # Reference: how to dump sklearn -> Mosaic YAML
```

## Honest Disclaimer

This is a demonstration of Mosaic's model-evaluation capabilities using real production weights from a deployed sports-betting system. The model has demonstrated 66.4% directional accuracy and positive expected value on holdout data.

**Past performance does not guarantee future results.** The model may have edge or it may not — Mosaic tells you which, honestly. Layer 2 (Judge) grades every prediction against reality. Layer 3 (Investigator) identifies systematic failure patterns. If the model stops working, Mosaic will show you that clearly through rising Brier scores, declining direction accuracy, and increasing `Overconfidence_Flag` frequency.

Sports betting involves real financial risk. This cartridge is an analytical tool, not financial advice. Use at your own discretion and within your means.
