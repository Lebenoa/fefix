#!/usr/bin/env sh
# Local build & install of fefix from this checkout.
#
# Produces a self-sufficient installation:
#   ~/.cargo/bin/ffx                        the release binary
#   ~/.cargo/runtime/grammars/*.so         tree-sitter parsers (built or symlinked)
#   ~/.cargo/runtime/queries/*.scm         indent/highlight/... queries (symlinked)
#
# The loader looks for `runtime/` beside the cargo home, so an installed `ffx`
# finds its queries and grammars from any working directory without
# FEFIX_RUNTIME or a wrapper script.
#
# Usage:
#   contrib/build-install.sh             build + install + runtime setup
#   contrib/build-install.sh --opt       add -C target-cpu=native
#   contrib/build-install.sh --no-cache  wipe cached grammar sources first
set -eu

cd "$(dirname "$0")/.."
REPO="$PWD"

OPT=""
while [ $# -gt 0 ]; do
    case "$1" in
        --opt) OPT="--config 'build.rustflags=[\"-C\",\"target-cpu=native\"]'" ;;
        --no-cache) rm -rf runtime/grammars/sources ;;
        *) echo "unknown option: $1" >&2; exit 1 ;;
    esac
    shift
done

command -v cargo >/dev/null 2>&1 || { echo "cargo not found — install rustup first: https://rustup.rs" >&2; exit 1; }
command -v cc >/dev/null 2>&1 || command -v c++ >/dev/null 2>&1 || command -v clang >/dev/null 2>&1 || { echo "no C++ compiler found (needed for tree-sitter grammars)" >&2; exit 1; }

CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
RT_DST="$CARGO_HOME/runtime"

echo "==> cargo install --path helix-term --bin ffx"
# The build script auto-fetches + auto-builds grammars into runtime/grammars/.
eval cargo install --path helix-term --bin ffx $OPT

echo "==> linking runtime files into $RT_DST"
# queries/, themes/, tutor/ ship with the source checkout; grammars/ is
# produced by the auto-build above. Link everything not already present so a
# stale local copy never shadows a fresh build.
rm -rf "$RT_DST/queries" "$RT_DST/themes" "$RT_DST/tutor"
for entry in runtime/*; do
    name="$(basename "$entry")"
    if [ -e "$RT_DST/$name" ]; then
        continue
    fi
    ln -s "$REPO/$entry" "$RT_DST/$name"
done

echo "==> verifying"
if ffx --health 2>&1 | grep -q 'Indent queries: ✘'; then
    echo "warning: some languages report missing indent queries" >&2
fi
ffx --health | sed -n '3,8p'
echo "==> done: $(ffx --version)"
