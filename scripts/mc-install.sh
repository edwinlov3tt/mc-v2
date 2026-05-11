#!/bin/bash
# Rebuild and install the mc binary to ~/.cargo/bin
# Run this after making changes, or use `mc-dev` wrapper for auto-rebuild.
set -e
cd "$(dirname "$0")/.."
cargo install --path crates/mc-cli --bin mc --locked --quiet 2>/dev/null
echo "mc installed to ~/.cargo/bin/mc"
