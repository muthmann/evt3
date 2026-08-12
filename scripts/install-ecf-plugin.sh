#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Build and install the Prophesee HDF5 ECF plugin from source.

Usage:
  ./scripts/install-ecf-plugin.sh [--prefix DIR] [--force]

Options:
  --prefix DIR  Install plugin libraries into DIR.
                Defaults to $HDF5_ECF_INSTALL_DIR or ~/.local/share/hdf5/plugin
  --force       Rebuild and reinstall even if the plugin is already present
  -h, --help    Show this help message
EOF
}

die() {
  echo "Error: $*" >&2
  exit 1
}

find_cpp_compiler() {
  local candidate
  for candidate in c++ clang++ g++; do
    if command -v "${candidate}" >/dev/null 2>&1; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done
  return 1
}

detect_hdf5_root() {
  if [[ -n "${HDF5_DIR:-}" ]]; then
    printf '%s\n' "${HDF5_DIR}"
    return 0
  fi

  if command -v brew >/dev/null 2>&1; then
    local brew_prefix
    if brew_prefix="$(brew --prefix hdf5 2>/dev/null)"; then
      printf '%s\n' "${brew_prefix}"
      return 0
    fi
  fi

  if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists hdf5 2>/dev/null; then
    pkg-config --variable=prefix hdf5
    return 0
  fi

  if command -v h5cc >/dev/null 2>&1; then
    local h5cc_path
    h5cc_path="$(command -v h5cc)"
    printf '%s\n' "$(cd "$(dirname "${h5cc_path}")/.." && pwd)"
    return 0
  fi

  return 1
}

copy_plugin_artifacts() {
  local plugin_build_dir="$1"
  local build_dir="$2"
  local install_dir="$3"
  local artifact
  local plugin_path

  mkdir -p "${install_dir}"

  for artifact in "${plugin_build_dir}"/libH5Zecf.*; do
    cp -P "${artifact}" "${install_dir}/"
  done

  for artifact in "${build_dir}"/libhdf5_ecf_codec*; do
    cp -P "${artifact}" "${install_dir}/"
  done

  if [[ "$(uname -s)" == "Darwin" ]]; then
    plugin_path="$(find "${install_dir}" -maxdepth 1 -type f -name 'libH5Zecf.*' | head -n1)"
    if [[ -n "${plugin_path}" ]]; then
      install_name_tool -delete_rpath "${build_dir}" "${plugin_path}" 2>/dev/null || true
      install_name_tool -add_rpath "@loader_path" "${plugin_path}" 2>/dev/null || true
    fi
  fi
}

FORCE=0
INSTALL_DIR="${HDF5_ECF_INSTALL_DIR:-$HOME/.local/share/hdf5/plugin}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      [[ $# -ge 2 ]] || die "--prefix requires a directory argument"
      INSTALL_DIR="$2"
      shift 2
      ;;
    --force)
      FORCE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

for tool in git cmake; do
  command -v "${tool}" >/dev/null 2>&1 || die "'${tool}' not found in PATH"
done

CPP_COMPILER="$(find_cpp_compiler)" || die "no C++ compiler found in PATH (tried: c++, clang++, g++)"
HDF5_ROOT="$(detect_hdf5_root)" || die $'HDF5 not found.\n  macOS:  brew install hdf5\n  Ubuntu: sudo apt install libhdf5-dev'

[[ -d "${HDF5_ROOT}" ]] || die "detected HDF5 root does not exist: ${HDF5_ROOT}"

if [[ ${FORCE} -eq 0 ]] && compgen -G "${INSTALL_DIR}/libH5Zecf.*" >/dev/null; then
  echo "ECF plugin already installed at ${INSTALL_DIR}"
  echo "Run with --force to rebuild."
  echo
  echo "Add this to your shell config:"
  echo "  export HDF5_PLUGIN_PATH=\"${INSTALL_DIR}\""
  exit 0
fi

BUILD_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/hdf5-ecf.XXXXXX")"
trap 'rm -rf "${BUILD_ROOT}"' EXIT

SRC_DIR="${BUILD_ROOT}/src"
BUILD_DIR="${BUILD_ROOT}/build"
PLUGIN_BUILD_DIR="${BUILD_DIR}/lib/hdf5/plugin"

echo "Using C++ compiler: ${CPP_COMPILER}"
echo "Using HDF5 root: ${HDF5_ROOT}"
echo "Installing plugin to: ${INSTALL_DIR}"
echo
echo "Cloning prophesee-ai/hdf5_ecf..."
git clone --depth=1 https://github.com/prophesee-ai/hdf5_ecf.git "${SRC_DIR}"

echo "Configuring with CMake..."
cmake -S "${SRC_DIR}" -B "${BUILD_DIR}" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_CXX_COMPILER="${CPP_COMPILER}" \
  -DHDF5_ROOT="${HDF5_ROOT}"

echo "Building..."
cmake --build "${BUILD_DIR}" --parallel

echo "Installing..."
copy_plugin_artifacts "${PLUGIN_BUILD_DIR}" "${BUILD_DIR}" "${INSTALL_DIR}"

if ! compgen -G "${INSTALL_DIR}/libH5Zecf.*" >/dev/null; then
  die "install completed but libH5Zecf.* was not found in ${INSTALL_DIR}"
fi

if ! compgen -G "${INSTALL_DIR}/libhdf5_ecf_codec*" >/dev/null; then
  die "install completed but libhdf5_ecf_codec* was not found in ${INSTALL_DIR}"
fi

echo
echo "Done."
echo "Add this to your shell config (~/.zshrc or ~/.bashrc):"
echo
echo "  export HDF5_PLUGIN_PATH=\"${INSTALL_DIR}\""
echo
echo "Then verify HDF5 decoding:"
echo "  HDF5_PLUGIN_PATH=\"${INSTALL_DIR}\" HDF5_DIR=\"${HDF5_ROOT}\" \\"
echo "  cargo test -p evt3 --features hdf5 -- --show-output"
