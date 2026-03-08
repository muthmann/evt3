#!/usr/bin/env bash
# Download large test fixtures that are excluded from the repository.
# Files are placed in evt3-core/test_data/ and skipped by integration tests
# when absent, so this script is optional but required for full test coverage.
#
# Usage (automatic, if direct URLs are known):
#   EVT3_RAW_URL=<url> EVT3_H5_URL=<url> ./scripts/download-test-data.sh
#
# Usage (manual): open the kDrive share page in a browser, download each file,
# then move them into evt3-core/test_data/:
#   Share page: https://kdrive.infomaniak.com/app/share/975517/ad8aa115-068e-4f29-9d16-663a7a9b5e02
#
# To get curl-able direct URLs from kDrive:
#   1. Open the share page in your browser
#   2. Open DevTools → Network tab
#   3. Click "Download" on the file
#   4. Copy the URL from the network request that fetches the actual file bytes
#   5. Set EVT3_RAW_URL / EVT3_H5_URL to those URLs and re-run this script

set -euo pipefail

DEST="$(cd "$(dirname "$0")/.." && pwd)/evt3-core/test_data"
mkdir -p "$DEST"

download_file() {
    local url="$1"
    local dest="$2"
    local name
    name="$(basename "$dest")"

    if [[ -f "$dest" ]]; then
        echo "  $name already exists — skipping"
        return
    fi

    echo "  Downloading $name..."
    curl -fL --progress-bar -o "$dest" "$url"
    echo "  → $dest"
}

# ── laser.raw ────────────────────────────────────────────────────────────────
if [[ -n "${EVT3_RAW_URL:-}" ]]; then
    download_file "$EVT3_RAW_URL" "$DEST/laser.raw"
else
    echo "EVT3_RAW_URL not set — skipping laser.raw"
    echo "  To get the URL: open the kDrive share page, click Download on laser.raw,"
    echo "  capture the URL from DevTools → Network, then set EVT3_RAW_URL."
fi

# ── laser.hdf5 ───────────────────────────────────────────────────────────────
if [[ -n "${EVT3_H5_URL:-}" ]]; then
    download_file "$EVT3_H5_URL" "$DEST/laser.h5"
else
    echo "EVT3_H5_URL not set — skipping laser.h5"
    echo "  To get the URL: open the kDrive share page, click Download on laser.hdf5,"
    echo "  capture the URL from DevTools → Network, then set EVT3_H5_URL."
fi

echo ""
echo "Share page: https://kdrive.infomaniak.com/app/share/975517/ad8aa115-068e-4f29-9d16-663a7a9b5e02"
echo ""
echo "Run integration tests:"
echo "  cargo test -p evt3-core                                                    # .raw tests"
echo "  HDF5_DIR=\$(brew --prefix hdf5) cargo test -p evt3-core --features hdf5   # + HDF5 tests"
