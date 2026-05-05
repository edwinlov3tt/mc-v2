#!/usr/bin/env python3
"""
Export a fitted sklearn linear model to Mosaic YAML format.

Usage:
    python export-from-sklearn.py --model path/to/model.pkl --output weights.yaml
    python export-from-sklearn.py --model path/to/model.pkl --scaler path/to/scaler.pkl --output weights.yaml

Supports: Lasso, Ridge, ElasticNet, LinearRegression, LogisticRegression (binary).

If a StandardScaler is provided (--scaler), coefficients are automatically
converted from standardized space to raw feature space so the Mosaic predict()
function can operate directly on raw inputs without a standardization block.

Conversion math (standardized -> raw):
    raw_coef[i]    = beta_i / std_i
    raw_intercept  = beta_0 - sum(beta_i * mean_i / std_i)

This matches the conversion done in claw-core/training/train_v16_final.py.

SECURITY NOTE: This script uses pickle to load sklearn model files (.pkl),
which is the standard serialization format for sklearn. Only load .pkl files
that you created yourself or trust completely — pickle can execute arbitrary
code during deserialization.
"""

import argparse
import json
import pickle  # Required for sklearn model deserialization (.pkl files)
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    import yaml
    HAS_YAML = True
except ImportError:
    HAS_YAML = False

try:
    import numpy as np
except ImportError:
    print("ERROR: numpy is required. Install with: pip install numpy")
    sys.exit(1)


def load_model(path: Path):
    """Load a pickled sklearn model.

    SECURITY: Only load .pkl files you trust. Pickle can execute arbitrary code.
    """
    with open(path, "rb") as f:
        model = pickle.load(f)  # noqa: S301 — sklearn models require pickle

    # Validate it's a supported model type
    supported = (
        "Lasso", "Ridge", "ElasticNet", "LinearRegression",
        "LogisticRegression", "LassoCV", "RidgeCV", "ElasticNetCV",
    )
    model_type = type(model).__name__
    if model_type not in supported:
        print(f"WARNING: Model type '{model_type}' is not explicitly supported.")
        print(f"  Supported: {supported}")
        print(f"  Attempting extraction anyway (needs .coef_ and .intercept_)...")

    if not hasattr(model, "coef_") or not hasattr(model, "intercept_"):
        print(f"ERROR: Model lacks .coef_ or .intercept_ attributes.")
        sys.exit(1)

    return model


def load_scaler(path: Path):
    """Load a pickled StandardScaler.

    SECURITY: Only load .pkl files you trust. Pickle can execute arbitrary code.
    """
    with open(path, "rb") as f:
        scaler = pickle.load(f)  # noqa: S301 — sklearn scalers require pickle
    if not hasattr(scaler, "mean_") or not hasattr(scaler, "scale_"):
        print("ERROR: Scaler lacks .mean_ or .scale_ attributes.")
        sys.exit(1)
    return scaler


def convert_to_raw(model, scaler):
    """Convert standardized coefficients to raw feature space."""
    coefs = model.coef_.flatten()
    intercept = float(model.intercept_)
    means = scaler.mean_
    stds = scaler.scale_

    raw_coefs = coefs / stds
    raw_intercept = intercept - float(np.sum(coefs * means / stds))

    return raw_intercept, raw_coefs


def build_mosaic_yaml(
    model,
    feature_names: list[str],
    model_name: str,
    intercept: float,
    coefficients: np.ndarray,
    residual_std: float | None = None,
    metadata: dict | None = None,
) -> dict:
    """Build the Mosaic YAML structure for a fitted model."""

    coef_list = []
    for name, weight in zip(feature_names, coefficients):
        coef_list.append({
            "feature": name,
            "weight": round(float(weight), 6),
        })

    fitted_model = {
        "name": model_name,
        "method": "linear",
        "intercept": round(intercept, 6),
        "coefficients": coef_list,
        "standardization": None,
    }

    if residual_std is not None:
        fitted_model["residual_std"] = round(residual_std, 3)

    meta = {
        "fitted_at": datetime.now(timezone.utc).isoformat(),
        "algorithm": type(model).__name__.lower(),
        "source": "export-from-sklearn.py",
    }
    if hasattr(model, "alpha"):
        meta["alpha"] = float(model.alpha) if hasattr(model.alpha, "__float__") else model.alpha

    non_zero = int(np.sum(np.abs(coefficients) > 1e-6))
    meta["non_zero_features"] = non_zero
    meta["total_features"] = len(feature_names)

    if metadata:
        meta.update(metadata)

    fitted_model["metadata"] = meta

    return {"fitted_models": [fitted_model]}


def main():
    parser = argparse.ArgumentParser(
        description="Export sklearn linear model to Mosaic YAML format"
    )
    parser.add_argument(
        "--model", required=True, type=Path,
        help="Path to pickled sklearn model (.pkl)"
    )
    parser.add_argument(
        "--scaler", type=Path, default=None,
        help="Path to pickled StandardScaler (.pkl). If provided, converts to raw space."
    )
    parser.add_argument(
        "--features", type=Path, default=None,
        help="Path to JSON file with ordered feature names list"
    )
    parser.add_argument(
        "--feature-names", nargs="+", default=None,
        help="Feature names in order (alternative to --features file)"
    )
    parser.add_argument(
        "--name", default="my_model",
        help="Model name for the fitted_models block"
    )
    parser.add_argument(
        "--residual-std", type=float, default=None,
        help="Residual standard deviation (for probability calculations)"
    )
    parser.add_argument(
        "--output", required=True, type=Path,
        help="Output path (.yaml or .json)"
    )
    parser.add_argument(
        "--format", choices=["yaml", "json"], default=None,
        help="Output format (auto-detected from extension if not specified)"
    )

    args = parser.parse_args()

    # Load model
    print(f"Loading model from {args.model}...")
    model = load_model(args.model)
    print(f"  Type: {type(model).__name__}")
    print(f"  Coefficients shape: {model.coef_.shape}")
    print(f"  Intercept: {float(model.intercept_):.6f}")

    # Determine coefficients
    if args.scaler:
        print(f"\nLoading scaler from {args.scaler}...")
        scaler = load_scaler(args.scaler)
        intercept, coefficients = convert_to_raw(model, scaler)
        print(f"  Converted to raw space.")
        print(f"  Raw intercept: {intercept:.6f}")
    else:
        intercept = float(model.intercept_)
        coefficients = model.coef_.flatten()
        print(f"\n  No scaler provided — using coefficients as-is (raw space assumed).")

    # Feature names
    if args.features:
        with open(args.features) as f:
            feature_names = json.load(f)
    elif args.feature_names:
        feature_names = args.feature_names
    elif hasattr(model, "feature_names_in_"):
        feature_names = list(model.feature_names_in_)
    else:
        feature_names = [f"feature_{i}" for i in range(len(coefficients))]
        print(f"  WARNING: No feature names provided. Using generic names.")

    if len(feature_names) != len(coefficients):
        print(f"ERROR: Feature name count ({len(feature_names)}) != "
              f"coefficient count ({len(coefficients)})")
        sys.exit(1)

    # Non-zero analysis
    non_zero_mask = np.abs(coefficients) > 1e-6
    non_zero_count = int(non_zero_mask.sum())
    print(f"\n  Non-zero coefficients: {non_zero_count}/{len(coefficients)}")
    print(f"  Top 10 by absolute weight:")
    sorted_idx = np.argsort(np.abs(coefficients))[::-1]
    for i in sorted_idx[:10]:
        if abs(coefficients[i]) > 1e-6:
            print(f"    {feature_names[i]:40s} {coefficients[i]:+.6f}")

    # Build output
    result = build_mosaic_yaml(
        model=model,
        feature_names=feature_names,
        model_name=args.name,
        intercept=intercept,
        coefficients=coefficients,
        residual_std=args.residual_std,
    )

    # Determine format
    fmt = args.format
    if fmt is None:
        fmt = "yaml" if args.output.suffix in (".yaml", ".yml") else "json"

    # Write output
    print(f"\nWriting {fmt.upper()} to {args.output}...")
    with open(args.output, "w") as f:
        if fmt == "yaml":
            if not HAS_YAML:
                print("ERROR: PyYAML not installed. Install with: pip install pyyaml")
                print("  Falling back to JSON output.")
                json.dump(result, f, indent=2)
            else:
                yaml.dump(result, f, default_flow_style=False, sort_keys=False)
        else:
            json.dump(result, f, indent=2)

    print(f"Done. Model exported as '{args.name}'.")
    print(f"\nTo use in a Mosaic model YAML, copy the fitted_models block into your model file.")
    print(f"Then reference it in a rule body as: predict(\"{args.name}\", feature1, feature2, ...)")


if __name__ == "__main__":
    main()
