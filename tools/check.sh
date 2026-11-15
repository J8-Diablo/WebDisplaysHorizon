#!/bin/sh
# Type-checks the native mod against the pinned Horizon source, without the launcher:
#   tools/check.sh [path to Horizon]
# The default is the source the launcher downloaded for the revision in
# native/horizon-mixin.toml. CARGO_CMD=test runs the crate's tests instead.
#
# It then checks the allow-list rule: every registry package the crate resolves to must be
# in Horizon's own Cargo.lock at the same version, as the launcher's build requires
# (docs/native-mods.md, "The lock file is the allow list"). The playable game is the
# launcher's:
#   horizon-launcher native-build native --instance <instance> --trust-native-code
HERE="$(cd "$(dirname "$0")/.." && pwd)"
REV="$(sed -n 's/^horizon_revision = "\([0-9a-f]\{40\}\)"/\1/p' "$HERE/native/horizon-mixin.toml")"
H="${1:-$APPDATA/horizon-launcher/native/source/$REV}"
H="$(printf '%s' "$H" | tr '\\' '/')"
if [ ! -f "$H/Cargo.lock" ]; then
  echo "no Horizon source at $H (revision $REV): pass its path, or build any native mod once" >&2
  exit 2
fi
# The mod depends on HCEF (the browser mod), which the launcher adds to its package as the
# crate `hcef`. Here that is done on a manifest of its own beside the package, over the same
# sources: native/.check/Cargo.toml (not in git). HCEF's repository is expected beside this
# one; HCEF_REPO says otherwise.
HCEF="${HCEF_REPO:-$(dirname "$HERE")/HCEF}"
# Cargo reads the path in the manifest: a Windows one where the shell's is /c/...
HCEF="$(cygpath -m "$HCEF" 2>/dev/null || printf '%s' "$HCEF")"
if [ ! -f "$HCEF/native/api/Cargo.toml" ]; then
  echo "no HCEF at $HCEF: clone it beside this repository, or set HCEF_REPO" >&2
  exit 2
fi
STAGE="$HERE/native/.check"
mkdir -p "$STAGE" || exit 1
{
  cat "$HERE/native/api/Cargo.toml"
  printf 'hcef = { path = "%s/native/api" }\n\n[lib]\npath = "../api/src/lib.rs"\n' "$HCEF"
} > "$STAGE/Cargo.toml"
cd "$STAGE" || exit 1
# Starting from Horizon's lock keeps every shared package at Horizon's version.
cp "$H/Cargo.lock" Cargo.lock
CARGO_TARGET_DIR="$HERE/native/api/target" cargo ${CARGO_CMD:-check} \
  --config "patch.\"https://github.com/horizon-rust/horizon\".horizon-client.path=\"$H/crates/horizon-client\"" \
  --config "patch.crates-io.wgpu-hal.path=\"$H/third_party/wgpu-hal\"" \
  --config "patch.crates-io.naga.path=\"$H/third_party/naga\"" \
  --config "patch.crates-io.libmimalloc-sys.path=\"$H/third_party/libmimalloc-sys\"" || exit 1
exec python -I "$HERE/tools/locked.py" "$H/Cargo.lock" Cargo.lock
