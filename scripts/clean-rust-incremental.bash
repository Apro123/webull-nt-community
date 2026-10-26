#!/usr/bin/env bash
# Reclaim disk space from Rust incremental compilation caches under target/.
#
# Cargo keeps a separate incremental/ tree per profile (debug, nextest, release,
# …). Feature-flag and dependency-hash changes leave orphaned crate hash
# directories behind; those are safe to delete. Compiled deps (.rlib) are kept,
# so the next build is much cheaper than a full `cargo clean`.
#
# Prefer wiping entire incremental/ dirs over "keep newest hash only": one crate
# often has several active hashes (lib vs tests, feature sets).
#
# Usage:
#   bash scripts/clean-rust-incremental.bash           # dry-run
#   bash scripts/clean-rust-incremental.bash --apply   # delete
#   CARGO_TARGET_DIR=/other/target bash scripts/clean-rust-incremental.bash --apply

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-${REPO_ROOT}/target}"
APPLY=false

usage() {
  cat <<'EOF'
Usage: bash scripts/clean-rust-incremental.bash [--apply]

Remove Rust incremental compilation caches under target/*/incremental
(or $CARGO_TARGET_DIR/*/incremental). Keeps deps and final artifacts.

  (default)   Dry-run: list directories and sizes only
  --apply     Delete the listed incremental directories

EOF
}

case "${1:-}" in
  "") ;;
  --apply)
    APPLY=true
    ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    echo "ERROR: unknown argument: ${1}" >&2
    usage >&2
    exit 1
    ;;
esac

# Directory size in KiB (portable GNU/BSD du)
dir_kib() {
  du -sk "$1" 2>/dev/null | awk '{ print $1 }'
}

fmt_mib() {
  awk -v kib="$1" 'BEGIN { printf "%.0f MiB", kib / 1024 }'
}

if [[ ! -d "$TARGET_DIR" ]]; then
  echo "No target directory at ${TARGET_DIR} (nothing to clean)."
  exit 0
fi

dirs=()
while IFS= read -r -d '' dir; do
  dirs+=("$dir")
done < <(find "$TARGET_DIR" -mindepth 2 -maxdepth 2 -type d -name incremental -print0 2>/dev/null | sort -z)

if [[ ${#dirs[@]} -eq 0 ]]; then
  echo "No incremental/ directories under ${TARGET_DIR}."
  exit 0
fi

echo "Mode: $([[ "$APPLY" == true ]] && echo APPLY || echo DRY-RUN)"
echo "Target: ${TARGET_DIR}"
echo ""

total_kib=0
for dir in "${dirs[@]}"; do
  kib="$(dir_kib "$dir")"
  kib="${kib:-0}"
  total_kib=$((total_kib + kib))
  printf '  %s\t%s\n' "$(fmt_mib "$kib")" "$dir"
done

echo ""
echo "Total: $(fmt_mib "$total_kib")"

if [[ "$APPLY" != true ]]; then
  echo ""
  echo "Dry-run complete. Re-run with --apply to delete."
  echo "For a full wipe of target/ use: make clean-builds"
  exit 0
fi

echo ""
for dir in "${dirs[@]}"; do
  echo "  rm -rf ${dir}"
  rm -rf "$dir"
done

echo ""
echo "Done. Next cargo/nextest build will recreate incremental caches as needed."
