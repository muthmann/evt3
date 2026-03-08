#!/usr/bin/env bash
# Download large test fixtures that are excluded from the repository.
# Files are placed in evt3-core/test_data/ and skipped by integration tests
# when absent, so this script is optional but required for full test coverage.
#
# Usage:
#   ./scripts/download-test-data.sh
#
# The kDrive share link below is a browser share page. To get direct download
# URLs, open the share link in a browser, right-click each file, and copy the
# direct download link — then replace the placeholder URLs below.

set -euo pipefail

DEST="$(cd "$(dirname "$0")/.." && pwd)/evt3-core/test_data"
mkdir -p "$DEST"

# ── laser.raw ────────────────────────────────────────────────────────────────
# Direct download URL for laser.raw (replace with actual URL from kDrive share)
RAW_URL="${EVT3_RAW_URL:-}"
if [[ -z "$RAW_URL" ]]; then
    echo "Set EVT3_RAW_URL to the direct download URL for laser.raw"
    echo "Share page: https://kdrive.infomaniak.com/app/share/975517/ad8aa115-068e-4f29-9d16-663a7a9b5e02"
else
    echo "Downloading laser.raw..."
    curl -fL --progress-bar -o "$DEST/laser.raw" "$RAW_URL"
    echo "  → $DEST/laser.raw"
fi

# ── laser.h5 ─────────────────────────────────────────────────────────────────
# Direct download URL for laser.h5 (replace with actual URL from kDrive share)
H5_URL="${EVT3_H5_URL:-}"
if [[ -z "$H5_URL" ]]; then
    echo "Set EVT3_H5_URL to the direct download URL for laser.h5"
    echo "Share page: https://kdrive.infomaniak.com/app/share/975517/ad8aa115-068e-4f29-9d16-663a7a9b5e02"
else
    echo "Downloading laser.h5..."
    curl -fL --progress-bar -o "$DEST/laser.h5" "$H5_URL"
    echo "  → $DEST/laser.h5"
fi

echo ""
echo "Test data is ready. Run tests with:"
echo "  cargo test -p evt3-core                         # .raw tests"
echo "  HDF5_DIR=\$(brew --prefix hdf5) cargo test -p evt3-core --features hdf5  # + HDF5 tests"
