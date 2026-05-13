#!/bin/bash
# Sync mosaic-plugin/ to the public repo at github.com/edwinlov3tt/mosaic-plugin
# Run from the mc-v2 repo root after committing changes.
set -e
cd "$(dirname "$0")/.."
echo "Pushing mosaic-plugin/ to public repo..."
git subtree push --prefix=mosaic-plugin mosaic-plugin main
echo "Done — https://github.com/edwinlov3tt/mosaic-plugin"
