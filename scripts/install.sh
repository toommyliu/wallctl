#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "${repo_dir}"

if [[ ! -f Cargo.lock ]]; then
  echo "Cargo.lock is missing. Run 'cargo generate-lockfile' before installing." >&2
  exit 1
fi

cargo install --path . --locked

cargo_bin="${CARGO_HOME:-${HOME}/.cargo}/bin"
installed="${cargo_bin}/wallctl"

if [[ ! -x "${installed}" ]]; then
  echo "wallctl was not found at ${installed} after cargo install." >&2
  exit 1
fi

"${installed}" --version

case ":${PATH}:" in
  *":${cargo_bin}:"*) ;;
  *)
    cat <<EOF

PATH hint:
  ${cargo_bin} is not on PATH for this shell.
  Add this to your shell profile if needed:

    export PATH="${cargo_bin}:\$PATH"
EOF
    ;;
esac
