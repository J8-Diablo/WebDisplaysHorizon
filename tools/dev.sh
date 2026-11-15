#!/bin/sh
# The development loop for the mod in the game:
#
#   tools/dev.sh build [LOG]   compile the native mod, and HCEF, into the dev instance's game
#   tools/dev.sh snapshot      freeze copies of content/ and of hcef-host for play
#   tools/dev.sh play          start the dev instance on the test world, with those copies
#   tools/dev.sh ctl ARGS...   horizon-ctl of the build the dev instance plays
#
# The mod depends on HCEF, the browser mod, whose repository is expected beside this one
# (HCEF_REPO says otherwise): `native-build` builds only the mods of the folder it is given,
# so `build` hands it a folder holding a copy of both. A copy, because the launcher compiles
# for six to thirteen minutes and the repositories can then be edited meanwhile. `play` uses
# frozen copies for the same reason: a content/ being regenerated or a hcef-host being
# rebuilt under a running game gives a game that fails in ways that are not the mod's.
#
# The launcher's build takes --trust-native-code: the mods' own code runs in the game with
# the player's rights. It is the player who says so; this script is for the mod's author on
# their own machine. It never builds onto another instance than WD_INSTANCE.
#
#   HORIZON_LAUNCHER   the launcher (default: its usual place under %LOCALAPPDATA%)
#   WD_INSTANCE        the instance to build for and play (default: webdisplays-dev)
#   WD_WORLD           the world `play` opens (default: wd-e3)
#   HCEF_REPO          HCEF's repository (default: ../HCEF)
HERE="$(cd "$(dirname "$0")/.." && pwd)"
L="${HORIZON_LAUNCHER:-$LOCALAPPDATA/Programs/Horizon Launcher/horizon-launcher.exe}"
INSTANCE="${WD_INSTANCE:-webdisplays-dev}"
HCEF="${HCEF_REPO:-$(dirname "$HERE")/HCEF}"
WORK="$HERE/target/dev"

# Copies the native mod of the repository $1 to $2: its manifest and its package's sources.
stage() {
  mkdir -p "$2/api" || return 1
  cp "$1/native/horizon-mixin.toml" "$2/" && cp "$1/native/api/Cargo.toml" "$2/api/" && cp -r "$1/native/api/src" "$2/api/src"
}

case "$1" in
  build)
    [ -f "$HCEF/native/horizon-mixin.toml" ] || { echo "no HCEF at $HCEF: clone it beside this repository, or set HCEF_REPO" >&2; exit 2; }
    B="$WORK/native"
    rm -rf "$B"
    stage "$HCEF" "$B/hcef" && stage "$HERE" "$B/webdisplays" || exit 1
    LOG="${2:-$WORK/build.log}"
    "$L" native-build "$(cygpath -w "$B")" --instance "$INSTANCE" --trust-native-code > "$LOG" 2>&1
    code=$?
    grep -aiE '^error|E_[A-Z_]+' -A8 "$LOG" | head -40
    tail -3 "$LOG" | cut -c1-200
    exit $code
    ;;
  snapshot)
    [ -f "$HCEF/host/target/release/hcef-host.exe" ] || { echo "hcef-host is not built: HCEF's tools/build-host.sh" >&2; exit 2; }
    rm -rf "$WORK/snap"; mkdir -p "$WORK/snap/host" || exit 1
    cp -r "$HERE/content" "$WORK/snap/content"
    cd "$HCEF/host/target/release" || exit 1
    find . -maxdepth 1 -type f ! -name '*.pdb' ! -name '*.d' ! -name '*.lib' ! -name '.*' -exec cp {} "$WORK/snap/host/" \;
    [ -d locales ] && cp -r locales "$WORK/snap/host/"
    echo "snapshot in $WORK/snap"
    ;;
  play)
    [ -f "$WORK/snap/host/hcef-host.exe" ] || { echo "no snapshot: tools/dev.sh snapshot" >&2; exit 2; }
    HCEF_HOST="$(cygpath -w "$WORK/snap/host/hcef-host.exe")"
    export HCEF_HOST
    exec "$L" play --instance "$INSTANCE" --world "${WD_WORLD:-wd-e3}" --creative --arg --control \
      --arg --dev-mod --arg "$(cygpath -w "$WORK/snap/content")"
    ;;
  ctl)
    shift
    BUILD="$("$L" instance list 2>/dev/null | grep "($INSTANCE)" | grep -o 'native-[a-z0-9-]*')"
    exec "$APPDATA/horizon-launcher/versions/$BUILD/horizon-ctl.exe" "$@"
    ;;
  *)
    sed -n '2,8p' "$0" | cut -c3-
    exit 2
    ;;
esac
