#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'HELP'
Usage: scripts/build-artifacts.sh [all|full|live] [output-directory]

Build Release app bundles and ZIP archives for this Mac's architecture.
Defaults to both apps in the repository's dist/ directory.
Requires macOS, Xcode, and Cargo. Bundles are ad-hoc signed, not notarized.
HELP
}

case "${1:-all}" in
  -h|--help) usage; exit 0 ;;
  all) schemes=("Wallctl" "Wallctl Live") ;;
  full) schemes=("Wallctl") ;;
  live) schemes=("Wallctl Live") ;;
  *) usage >&2; exit 1 ;;
esac
if [[ $# -gt 2 ]]; then
  usage >&2
  exit 1
fi

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "$(uname -s)" != Darwin ]]; then
  echo "App artifacts must be built on macOS." >&2
  exit 1
fi
for tool in xcodebuild cargo codesign ditto; do
  if ! command -v "${tool}" >/dev/null 2>&1; then
    echo "Required tool not found: ${tool}" >&2
    exit 1
  fi
done

output_dir="${2:-${repo_dir}/dist}"
mkdir -p "${output_dir}"
output_dir="$(cd "${output_dir}" && pwd)"
build_dir="$(mktemp -d "${TMPDIR:-/tmp}/wallctl-artifacts.XXXXXX")"
trap 'rm -rf "${build_dir}"' EXIT
architecture="$(uname -m)"

for scheme in "${schemes[@]}"; do
  echo "Building ${scheme} for ${architecture}..."
  log_file="${output_dir}/${scheme}-build.log"
  if ! xcodebuild \
    -project "${repo_dir}/macos/WallctlApp/Wallctl.xcodeproj" \
    -scheme "${scheme}" \
    -configuration Release \
    -derivedDataPath "${build_dir}" \
    "ARCHS=${architecture}" \
    ONLY_ACTIVE_ARCH=YES \
    CODE_SIGNING_ALLOWED=NO \
    build >"${log_file}" 2>&1; then
    tail -n 40 "${log_file}" >&2
    echo "Build failed. Full log: ${log_file}" >&2
    exit 1
  fi

  app="${build_dir}/Build/Products/Release/${scheme}.app"
  codesign --force --sign - "${app}/Contents/Helpers/wallctl"
  codesign --force --sign - "${app}"
  codesign --verify --deep --strict "${app}"

  archive="${output_dir}/${scheme}-${architecture}.zip"
  # Publish the completed archive only after packaging succeeds.
  ditto -c -k --sequesterRsrc --keepParent "${app}" "${build_dir}/${scheme}.zip"
  mv -f "${build_dir}/${scheme}.zip" "${archive}"
  echo "Created ${archive}"
done
